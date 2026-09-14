//! Boucle de l'acteur : le seul endroit du processus qui détient un
//! `&mut Project`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use plotyx_commands::{Command, CommandError, CommandResult, History};
use plotyx_core::{evaluate_formula, topological_order, Formula, NodeId, Project, ValueBlock};

use crate::error::{SessionError, SessionResult};
use crate::snapshot::{HistoryState, Revision, Snapshot};
use crate::{ClientId, WindowGeometry};

/// Requête envoyée à l'acteur par un [`crate::SessionHandle`]. Privée au
/// crate : les clients ne construisent jamais de `Request` eux-mêmes, ils
/// passent par les méthodes de `SessionHandle`, qui portent chacune leur
/// canal de réponse à usage unique.
pub(crate) enum Request {
    /// Exécute une commande. Répond la nouvelle révision, ou l'erreur rendue
    /// par `History::execute` — l'arbre et l'historique restent alors
    /// inchangés. Refusée (`ForeignMacro`) si une macro est ouverte par un
    /// autre émetteur que `client` (A4, voir la table en tête de crate).
    Execute {
        /// Commande à appliquer.
        command: Command,
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision, ou erreur.
        reply: Sender<CommandResult<Revision>>,
    },
    /// Annule l'entrée du sommet. `Ok(None)` si la pile est vide.
    Undo {
        /// Réponse : nouvelle révision si quelque chose a été annulé.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Rétablit l'entrée annulée la plus récente. `Ok(None)` si rien à rétablir.
    Redo {
        /// Réponse : nouvelle révision si quelque chose a été rétabli.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Ouvre une macro. `Err(MacroTooDeep)` au-delà de
    /// `History::MAX_MACRO_DEPTH` niveaux d'imbrication (B7,
    /// `guide-de-correction-robustesse.md` §F4). `Err(ForeignMacro)` si une
    /// macro est déjà ouverte par un autre émetteur que `client` (A4) — un
    /// même émetteur peut toujours imbriquer la sienne.
    BeginMacro {
        /// Libellé de la macro.
        label: String,
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision (l'état de l'historique a changé), ou
        /// l'erreur de `History::begin_macro`.
        reply: Sender<CommandResult<Revision>>,
    },
    /// Ferme la macro courante. `Err(ForeignMacro)` si l'émetteur n'est pas
    /// celui qui l'a ouverte (A4).
    EndMacro {
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`,
        /// `ForeignMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Annule la macro courante et tout ce qu'elle avait appliqué.
    /// `Err(ForeignMacro)` si l'émetteur n'est pas celui qui l'a ouverte —
    /// c'est exactement le refus qui ferme A4 : un `abort_macro` ne peut
    /// plus défaire le travail validé d'un autre client.
    AbortMacro {
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`,
        /// `ForeignMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Demande un instantané de l'état courant.
    Snapshot {
        /// Réponse : l'instantané demandé.
        reply: Sender<Snapshot>,
    },
    /// Pose (ou remplace) la géométrie de fenêtre d'un nœud. Ne touche ni
    /// `History` ni `revision` (voir la doc de tête de crate) : ce n'est pas
    /// une mutation du modèle, donc pas une entrée annulable. Toujours
    /// acceptée, y compris pour un `NodeId` qui n'existe pas (ou plus) dans
    /// l'arbre courant — l'entrée serait de toute façon purgée à la
    /// prochaine mutation qui constate son absence (voir `run`) ; valider ici
    /// n'apporterait rien qu'un aller-retour de plus.
    SetWindowGeometry {
        /// Nœud dont la fenêtre a bougé ou changé de taille.
        node: NodeId,
        /// Nouvelle géométrie.
        geometry: WindowGeometry,
        /// Réponse : nouvelle valeur de `geometry_revision` (voir la doc de
        /// tête de crate — un compteur distinct de `revision`, pour que
        /// déplacer une fenêtre ne fasse pas croire à un client de rendu que
        /// le modèle a changé).
        reply: Sender<u64>,
    },
    /// Sauvegarde le projet courant vers `path`. Ne change pas la révision :
    /// sauvegarder ne modifie pas le modèle.
    Save {
        /// Chemin de destination.
        path: PathBuf,
        /// Réponse : succès, ou erreur de persistance.
        reply: Sender<SessionResult<()>>,
    },
    /// Charge un projet complet depuis `path` et le substitue au projet
    /// courant. Vide l'historique. Refusé si une macro est ouverte.
    Load {
        /// Chemin source.
        path: PathBuf,
        /// Réponse : nouvelle révision, ou erreur (persistance, ou
        /// `Command(MacroOpen)` si une macro est ouverte).
        reply: Sender<SessionResult<Revision>>,
    },
    /// Demande l'arrêt de l'acteur.
    Shutdown,
}

/// Boucle de l'acteur : sert les requêtes dans l'ordre FIFO où elles arrivent
/// sur `rx`, jusqu'à ce que tous les émetteurs soient lâchés ou qu'un
/// `Shutdown` arrive. Rend le `Project` final, pour que l'appelant
/// (`Session::shutdown`) puisse le récupérer.
pub(crate) fn run(
    mut project: Project,
    history_limit: Option<usize>,
    rx: Receiver<Request>,
) -> Project {
    let mut history = match history_limit {
        Some(limit) => History::with_limit(limit),
        None => History::new(),
    };
    let mut revision: Revision = 0;
    // A3 (`guide-de-correction-robustesse.md` §F2/F6) — époque du projet
    // courant : locale à ce fil, **jamais persistée** (voir la doc de
    // `Snapshot::epoch`). N'avance que sur un `Load` qui réussit : ni
    // `Execute`/`Undo`/`Redo`, qui font déjà avancer `revision`, ni les
    // macros ne changent l'identité du projet.
    let mut epoch: u64 = 0;
    // A4 (`guide-de-correction-robustesse.md` §F6) — émetteur qui a ouvert la
    // macro courante, `None` hors macro. Remis à `None` dès que
    // `history.in_macro()` redevient faux, que la fermeture ait réussi ou
    // non (voir les branches `EndMacro`/`AbortMacro` : `History::end_macro`/
    // `abort_macro` peuvent avoir dépilé un niveau avant qu'une erreur ne
    // remonte).
    let mut macro_owner: Option<ClientId> = None;
    // Table `NodeId -> WindowGeometry`, à côté du `Project` (voir la doc de
    // tête de crate) : mutée uniquement par `SetWindowGeometry`, jamais par
    // une commande. `geometry_revision` est un compteur **distinct** de
    // `revision` — voir la doc de tête de crate pour la décision et le
    // pourquoi.
    let mut geometries: HashMap<NodeId, WindowGeometry> = HashMap::new();
    let mut geometry_revision: u64 = 0;

    for request in rx {
        match request {
            Request::Execute {
                command,
                client,
                reply,
            } => {
                // A4 — une macro ouverte par un autre émetteur n'absorbe plus
                // silencieusement le travail de celui-ci : c'était le bug.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("execute"))
                } else {
                    history.execute(&mut project, command).map(|()| {
                        revision += 1;
                        recalc_formulas(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::Undo { reply } => {
                let outcome = history.undo(&mut project).map(|reverted| {
                    reverted.then(|| {
                        revision += 1;
                        recalc_formulas(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::Redo { reply } => {
                let outcome = history.redo(&mut project).map(|restored| {
                    restored.then(|| {
                        revision += 1;
                        recalc_formulas(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::BeginMacro {
                label,
                client,
                reply,
            } => {
                // A4 — une macro déjà ouverte par un autre émetteur refuse
                // qu'on s'y imbrique ; le même émetteur le peut toujours
                // (jusqu'à `MAX_MACRO_DEPTH`, B7 ci-dessous).
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("begin_macro"))
                } else {
                    // B7 (`guide-de-correction-robustesse.md` §F4) —
                    // `begin_macro` peut refuser (`MacroTooDeep`) : la
                    // révision n'avance que si l'imbrication a réellement
                    // changé d'état. Un succès, en revanche, avance la
                    // révision même sans mutation du modèle (l'état de
                    // l'historique, `in_macro`, a changé).
                    let result = history.begin_macro(&label);
                    if result.is_ok() {
                        macro_owner = Some(client);
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::EndMacro { client, reply } => {
                // A4 — `NotInMacro` reste réservé au cas où aucune macro
                // n'est ouverte ; `ForeignMacro` couvre le cas où une macro
                // EST ouverte, mais pas par `client`.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("end_macro"))
                } else {
                    let result = history.end_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::AbortMacro { client, reply } => {
                // A4 — c'est ici que le bug se produisait : `abort_macro`
                // défaisait le travail d'un autre client sans le refuser.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("abort_macro"))
                } else {
                    let result = history.abort_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        // Un `abort_macro` revient à rejouer, d'un bloc,
                        // l'annulation de chaque commande absorbée dans la
                        // macro — exactement ce que ferait une suite
                        // d'`Undo` (qui, eux, recalculent après coup) : les
                        // colonnes-formule touchées par le travail défait
                        // doivent l'être aussi ici, sans quoi elles
                        // resteraient sur des valeurs qu'aucune formule
                        // actuellement en vigueur ne justifie plus.
                        recalc_formulas(&mut project);
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::Snapshot { reply } => {
                let snapshot = build_snapshot(
                    &project,
                    &history,
                    revision,
                    epoch,
                    &geometries,
                    geometry_revision,
                );
                let _ = reply.send(snapshot);
            }
            Request::SetWindowGeometry {
                node,
                geometry,
                reply,
            } => {
                // Ne touche ni `history` ni `revision` : voir la doc de tête
                // de crate — ce n'est pas une mutation du modèle. Toujours
                // acceptée (voir la doc de `Request::SetWindowGeometry`).
                geometries.insert(node, geometry);
                geometry_revision += 1;
                let _ = reply.send(geometry_revision);
            }
            Request::Save { path, reply } => {
                // A11 — palliatif déclaré (guide-de-correction-robustesse.md
                // §F6) : recopie de la garde de `load_project`, pas une
                // réponse à « lesquelles des huit requêtes une macro ouverte
                // doit-elle suspendre ? ». Sans elle, sauvegarder pendant une
                // macro écrit sur le disque un état qu'un `abort_macro`
                // ultérieur défera en mémoire — le fichier garde alors ce que
                // l'annulation a effacé. Ne touche ni au projet ni à
                // l'historique : la révision n'avance pas, sauvegarder ne
                // change pas le modèle. Refusée pour **quiconque** pendant
                // une macro, pas seulement son propriétaire (voir la table de
                // tête de crate) : l'état intermédiaire d'une macro que
                // n'importe qui pourrait encore avorter ne doit pas atteindre
                // le disque.
                //
                // `save_with_geometries` (et non `save`) : la géométrie des
                // fenêtres doit survivre à un rechargement (voir la doc de
                // tête de crate) — écrite dans le bloc séparé que
                // `plotyx-persistence` lui réserve, jamais mêlée à l'arbre
                // annulable.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen("save")))
                } else {
                    plotyx_persistence::save_with_geometries(&project, &geometries, &path)
                        .map_err(SessionError::from)
                };
                let _ = reply.send(outcome);
            }
            Request::Load { path, reply } => {
                let outcome =
                    load_project(&mut project, &mut history, &mut geometries, &path).map(|()| {
                        revision += 1;
                        // Les valeurs d'une colonne-formule sont écrites sur
                        // le disque (`plotyx-persistence`, voir sa doc) mais
                        // traitées comme un cache qu'on ne fait pas confiance
                        // à l'aveugle : un fichier a pu être modifié par un
                        // autre processus, une version antérieure de PlotYx,
                        // ou porter des colonnes dont l'ordre (donc les
                        // lettres) a changé sans que les valeurs suivent.
                        // Recalculer systématiquement ici est le choix
                        // retenu (peu coûteux — voir `recalc_formulas` — et
                        // toujours correct) plutôt que de faire confiance au
                        // contenu chargé.
                        recalc_formulas(&mut project);
                        // A3 — l'identité du projet change : une poignée
                        // (`plotyx_script::surface::Handle`) qui a mémorisé
                        // l'ancienne époque ne doit plus pouvoir désigner un
                        // `NodeId` du projet neuf sans être détectée. Un `Load`
                        // vide aussi l'historique (`load_project`), donc il n'y a
                        // plus de macro ouverte à ce point : pas besoin de
                        // remettre `macro_owner` à `None` ici, `load_project`
                        // l'a déjà refusé si une macro était ouverte.
                        epoch += 1;
                        // La table de géométries est entièrement remplacée par
                        // celle du fichier chargé (`load_project`, ci-dessous) :
                        // un client de rendu doit le savoir même si son contenu,
                        // par coïncidence, ressemblait à l'ancien.
                        geometry_revision += 1;
                        revision
                    });
                let _ = reply.send(outcome);
            }
            Request::Shutdown => {
                // Constat non reproductible (guide-de-correction-robustesse.md
                // §7.3, table de tête de `plotyx-session`) : une macro encore
                // ouverte à l'arrêt ne doit pas être commise en silence. Elle
                // est annulée, niveau par niveau, comme si son propriétaire
                // l'avait abandonnée avant de partir — il n'y a plus de
                // client à qui répondre, donc pas de vérification de
                // propriétaire ici (`abort_macro` prend `&mut project`
                // directement, hors du protocole `Request`). Une erreur en
                // cours de dépilement (cas déjà documenté par
                // `History::abort_macro` : « l'arbre peut alors être à moitié
                // défait ») ne doit pas transformer cette boucle en boucle
                // infinie : elle arrête le dépilement plutôt que de
                // l'ignorer.
                while history.in_macro() {
                    if history.abort_macro(&mut project).is_err() {
                        break;
                    }
                }
                break;
            }
        }
    }

    project
}

/// Charge un projet complet depuis `path` et le substitue à `*project` ; vide
/// l'historique et remplace entièrement `*geometries` par le bloc de
/// géométries lu dans le fichier (`plotyx_persistence::load_with_geometries`
/// filtre déjà les entrées orphelines — voir sa doc). Refuse
/// (`Err(Command(MacroOpen))`) si une macro est ouverte, **avant** de toucher
/// au disque : dans ce cas ni le projet, ni l'historique, ni les géométries
/// ne sont affectés. Si le chargement lui-même échoue (fichier absent,
/// corrompu, ...), l'erreur remonte telle quelle et, là aussi, rien n'a été
/// touché.
fn load_project(
    project: &mut Project,
    history: &mut History,
    geometries: &mut HashMap<NodeId, WindowGeometry>,
    path: &Path,
) -> SessionResult<()> {
    if history.in_macro() {
        return Err(SessionError::Command(CommandError::MacroOpen("load")));
    }
    let (loaded, loaded_geometries) = plotyx_persistence::load_with_geometries(path)?;
    *project = loaded;
    *geometries = loaded_geometries;
    // L'acteur est mono-thread et rien n'a pu rouvrir de macro entre la
    // vérification ci-dessus et cet appel : `clear()` ne peut pas échouer
    // ici. On ignore tout de même le résultat plutôt que d'appeler
    // `unwrap`/`expect`, bannis hors tests.
    let _ = history.clear();
    Ok(())
}

/// Recalcule toutes les colonnes-formule du projet — accès **direct** à
/// `Project` (`Project::set_length`/`Project::set_values`), jamais via
/// `History::apply`/`Command::execute` : c'est un effet de bord silencieux
/// rejoué après coup, pas une mutation annulable (voir la doc de tête de
/// crate, `docs/commandes-et-historique.md`) — exactement le même statut que
/// [`purge_orphaned_geometries`], qui rejoue déjà la purge de géométrie
/// après toute exécution structurelle. Le texte de la formule
/// (`Command::SetColumnMeta`) et les cellules sources (`Command::SetCells`,
/// `InsertRows`, `DeleteRows`, ...) restent, eux, annulables normalement :
/// c'est seulement le recalcul qui ne pousse jamais sa propre entrée.
///
/// # Portée du déclenchement
///
/// Appelée après **tout** `Execute`/`Undo`/`Redo`/`AbortMacro` qui a réussi
/// (donc a pu changer le projet) et après tout `Load` (voir `run`,
/// ci-dessus, pour chaque site d'appel et sa justification propre — en
/// particulier `Load`, qui recalcule plutôt que de faire confiance aux
/// valeurs lues du disque).
///
/// À l'intérieur de cette fonction, la portée est volontairement **large** :
/// tout l'arbre est balayé, pas seulement « la feuille que la commande vient
/// de toucher ». Deux raisons à ce choix plutôt qu'un déclenchement plus fin
/// piloté par `Command::is_structural()` ou par les variantes précises de
/// `Command` :
///
/// - `Undo`/`Redo`/`AbortMacro` ne portent aucune commande à inspecter ici —
///   seule `History` sait ce qu'elle vient de restaurer, et elle ne l'expose
///   pas (elle n'a d'ailleurs pas à le faire : c'est un détail
///   d'implémentation d'`Entry`) ; il faudrait lui ajouter une surface
///   dédiée rien que pour ce besoin.
/// - Même côté `Execute`, `Command::Move` peut déplacer une **colonne**
///   d'une feuille à une autre : les lettres de position changent à la fois
///   dans la feuille de départ et dans celle d'arrivée, alors que `Move` ne
///   distingue en rien ce cas d'un déplacement de dossier sans le moindre
///   effet sur une formule.
///
/// Le coût de ce balayage est celui de [`purge_orphaned_geometries`] :
/// linéaire dans le nombre de nœuds pour trouver les feuilles, puis borné
/// par le nombre de colonnes-formule effectivement présentes (le cas
/// courant : aucune) — jamais par la taille des données des colonnes qui
/// n'en portent pas.
fn recalc_formulas(project: &mut Project) {
    let sheets: Vec<NodeId> = project
        .tree()
        .iter()
        .filter(|(_, node)| node.kind().is_worksheet())
        .map(|(id, _)| id)
        .collect();
    for sheet in sheets {
        recalc_sheet(project, sheet);
    }
}

/// Recalcule les colonnes-formule d'une seule feuille, dans l'ordre de leurs
/// dépendances (tri topologique, [`plotyx_core::topological_order`]) — une
/// colonne-formule qui en référence une autre lit sa valeur **déjà
/// recalculée**, jamais celle d'avant cette passe. Un cycle de dépendance
/// (direct ou transitif) est détecté sans boucler : chaque colonne qui y
/// participe reçoit un bloc entièrement `NaN` et
/// `FormulaError::Cycle` plutôt qu'une valeur qui n'aurait de toute façon
/// aucun sens à choisir.
fn recalc_sheet(project: &mut Project, sheet: NodeId) {
    let Ok(columns) = project.tree().children(sheet).map(<[NodeId]>::to_vec) else {
        return;
    };
    // Longueur cible commune à toute la feuille : le maximum des longueurs
    // actuelles de ses colonnes (formule comprises), au moment où cette
    // passe commence — la même notion que `Tree::row_count`, qui gouverne
    // déjà `InsertRows`/`DeleteRows`.
    let target_len = project.tree().row_count(sheet).unwrap_or(0);

    // Formules analysées à l'avance : une erreur de *parse* est déjà un
    // résultat final (bloc `NaN`, `FormulaError::Parse`), elle ne participe
    // pas au graphe de dépendance — une colonne qui la référencerait la
    // trouve immédiatement disponible (à `NaN`), pas « en attente ».
    let mut parsed: BTreeMap<NodeId, Formula> = BTreeMap::new();
    for &id in &columns {
        let Some(source) = project
            .tree()
            .node(id)
            .and_then(|node| node.as_column())
            .and_then(|column| column.meta().formula.clone())
        else {
            continue;
        };
        match Formula::parse(&source) {
            Ok(formula) => {
                parsed.insert(id, formula);
            }
            Err(_error) => write_formula_column(project, id, vec![f64::NAN; target_len]),
        }
    }

    // Graphe de dépendance entre colonnes-formule uniquement : une référence
    // vers une colonne ordinaire n'est pas une arête à ordonner, elle est
    // déjà disponible (voir la doc de `plotyx_core::topological_order`).
    let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> =
        parsed.keys().map(|&id| (id, BTreeSet::new())).collect();
    for (&id, formula) in &parsed {
        for index in formula.referenced_columns() {
            if let Some(&dep_id) = columns.get(index) {
                if parsed.contains_key(&dep_id) {
                    if let Some(deps) = edges.get_mut(&id) {
                        deps.insert(dep_id);
                    }
                }
            }
        }
    }
    let (order, blocked) = topological_order(&edges);

    for id in blocked {
        write_formula_column(project, id, vec![f64::NAN; target_len]);
    }
    for id in order {
        let Some(formula) = parsed.get(&id) else {
            continue;
        };
        let Some(self_index) = columns.iter().position(|&c| c == id) else {
            continue;
        };
        // Relu à chaque colonne, pas une fois pour toute la feuille : les
        // colonnes-formule déjà traitées dans cette boucle (celles dont `id`
        // dépend, forcément plus tôt dans `order`) doivent être vues à jour.
        let values: Vec<_> = columns
            .iter()
            .map(|&column_id| {
                project
                    .tree()
                    .node(column_id)
                    .and_then(|node| node.as_column())
                    .map(|column| column.values().clone())
            })
            .collect();
        // Une colonne de `columns` a disparu entre la lecture de `columns`
        // et maintenant : ne peut pas arriver (cette fonction ne touche
        // jamais la structure de l'arbre), garde défensive plutôt qu'un
        // panic si l'hypothèse venait à être fausse un jour (invariant 10).
        if values.iter().any(Option::is_none) {
            continue;
        }
        let values: Vec<_> = values.into_iter().flatten().collect();
        let evaluation = evaluate_formula(formula, self_index, &values, target_len);
        write_formula_column(project, id, evaluation.values);
    }
}

/// Écrit `values` dans `column`, en le tronquant/étendant d'abord à sa
/// longueur (`Project::set_length`) puisque `Project::set_values` n'écrase
/// jamais une queue de colonne plus longue que le bloc écrit. Ignore
/// silencieusement un échec (`column` disparu entre-temps, ce que rien dans
/// cette fonction ne provoque) plutôt que de paniquer — invariant 10.
fn write_formula_column(project: &mut Project, column: NodeId, values: Vec<f64>) {
    if project.set_length(column, values.len()).is_err() {
        return;
    }
    let _ = project.set_values(column, 0, ValueBlock::Float(values));
}

/// Retire de `geometries` toute entrée dont le `NodeId` n'existe plus dans
/// l'arbre de `project` (nœud supprimé par une commande d'`Execute`, ou par
/// un `Undo`/`Redo` qui restaure un arbre où il n'existait pas) — nettoyage
/// sans risque d'une entrée de `HashMap` non référencée (voir la doc de tête
/// de crate : le choix retenu est de purger plutôt que de laisser traîner).
/// Rend `true` si quelque chose a effectivement été retiré, pour que
/// l'appelant décide s'il doit faire avancer `geometry_revision`.
fn purge_orphaned_geometries(
    project: &Project,
    geometries: &mut HashMap<NodeId, WindowGeometry>,
) -> bool {
    let tree = project.tree();
    let before = geometries.len();
    geometries.retain(|id, _| tree.node(*id).is_some());
    geometries.len() != before
}

/// Construit l'instantané courant : l'arbre et la table de géométries sont
/// partagés (`Arc::clone`/`Arc::new`), rien n'est recopié pour l'arbre ; la
/// table de géométries, elle, est recopiée dans un `Arc` neuf à chaque
/// instantané — un coût négligeable, borné par le nombre de fenêtres
/// ouvertes, pas par la taille du modèle.
fn build_snapshot(
    project: &Project,
    history: &History,
    revision: Revision,
    epoch: u64,
    geometries: &HashMap<NodeId, WindowGeometry>,
    geometry_revision: u64,
) -> Snapshot {
    Snapshot {
        tree: Arc::clone(project.tree()),
        revision,
        history: HistoryState {
            // C6 — pendant une macro, `undo` est toujours refusé
            // (`Err(MacroOpen)`) quoi que dise la pile : l'instantané ne doit
            // pas annoncer une annulation que l'acteur refusera.
            can_undo: history.can_undo() && !history.in_macro(),
            can_redo: history.can_redo(),
            undo_label: history.undo_label().map(str::to_string),
            redo_label: history.redo_label().map(str::to_string),
            in_macro: history.in_macro(),
        },
        epoch,
        window_geometries: Arc::new(geometries.clone()),
        geometry_revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N'existe que pour que le compilateur vérifie l'invariant `Send` — ce
    /// n'est pas un test comportemental.
    fn assert_send<T: Send>() {}

    #[test]
    fn request_is_send() {
        assert_send::<Request>();
    }
}
