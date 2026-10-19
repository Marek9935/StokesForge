//! Boucle de l'acteur : le seul endroit du processus qui détient un
//! `&mut Project`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use stokesforge_commands::{Command, CommandError, CommandResult, History, MacroKind};
use stokesforge_core::{NodeId, Project};

use crate::error::{SessionError, SessionResult};
use crate::gauge::{RecalcEnd, RecalcGauge};
use crate::recalc::{FormulaRecalc, Recalculator};
use crate::snapshot::{HistoryState, Revision, Snapshot};
use crate::{ClientId, LoadReport, WindowGeometry};

/// Requête envoyée à l'acteur par un [`crate::SessionHandle`]. Privée au
/// crate : les clients ne construisent jamais de `Request` eux-mêmes, ils
/// passent par les méthodes de `SessionHandle`, qui portent chacune leur
/// canal de réponse à usage unique.
// `Execute` porte une `Command` entière (colonne et métadonnées comprises), bien
// plus grosse que les autres requêtes : le message est consommé aussitôt par
// l'acteur, le boîter n'y gagnerait qu'une allocation par commande.
#[allow(clippy::large_enum_variant)]
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
        /// Vrai (#239) : répondre dès la commande appliquée, puis recalculer
        /// les formules en pouvant être arrêté par la [`RecalcGauge`]. La
        /// révision rendue est alors celle d'avant recalcul ; une révision
        /// de plus la suit quand il est fini.
        interruptible: bool,
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
        /// Nature du groupe.
        kind: MacroKind,
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
        reply: Sender<SessionResult<(Revision, LoadReport)>>,
    },
    /// Remplace le projet par un projet vide, avec les mêmes gardes que Load.
    NewProject {
        reply: Sender<SessionResult<Revision>>,
    },
    /// Relance des calculs d'analyse (#181), hors historique : `ids` vide pour
    /// tous les calculs périmés. Refusée (`ForeignMacro`) si une macro est
    /// ouverte par un autre émetteur, comme toute écriture.
    Recalculate {
        ids: Vec<NodeId>,
        client: ClientId,
        reply: Sender<CommandResult<Revision>>,
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
    recalc: FormulaRecalc,
    gauge: Arc<RecalcGauge>,
    rx: Receiver<Request>,
) -> Project {
    let mut history = match history_limit {
        Some(limit) => History::with_limit(limit),
        None => History::new(),
    };
    let mut revision: Revision = 0;
    // A3 (`guide-de-correction-robustesse.md` §F2/F6) — époque du projet
    // courant : locale à ce fil, **jamais persistée** (voir la doc de
    // `Snapshot::epoch`). Avance sur `Load` ou `NewProject` réussi : ni
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
    // #193 — ce que le recalcul des formules a vu à son dernier passage
    // (voir `recalc`) : à côté du `Project`, comme les géométries, jamais
    // dans l'historique ni dans le `.stf`. Le projet reçu à la création n'a
    // jamais été recalculé par cet acteur : le mémo vide le fera entièrement
    // à la première requête mutante, comme avant #193.
    let mut formulas = Recalculator::new(recalc);

    for request in rx {
        match request {
            Request::Execute {
                command,
                client,
                reply,
                interruptible,
            } => {
                // A4 — une macro ouverte par un autre émetteur n'absorbe plus
                // silencieusement le travail de celui-ci : c'était le bug.
                if history.in_macro() && macro_owner != Some(client) {
                    let _ = reply.send(Err(CommandError::ForeignMacro("execute")));
                    continue;
                }
                let recorded = match history.execute_recorded(&mut project, command) {
                    Ok(recorded) => recorded,
                    Err(err) => {
                        let _ = reply.send(Err(err));
                        continue;
                    }
                };
                revision += 1;
                if interruptible {
                    // #239 — la commande est appliquée : on répond sans
                    // attendre le recalcul, que Stop peut interrompre.
                    let id = gauge.begin();
                    let _ = reply.send(Ok(revision));
                    let stop_gauge = Arc::clone(&gauge);
                    let completed = formulas
                        .run_interruptible(&mut project, move || stop_gauge.stop_requested(id));
                    let mut end = RecalcEnd::Done;
                    if !completed {
                        // Stop : annuler la commande comme Ctrl+Z, puis
                        // recalculer ce qui a changé (la passe interrompue a
                        // pu réécrire des colonnes). Dans une macro l'annulation
                        // est refusée : la commande reste, le recalcul s'achève.
                        // Une commande sans effet n'a rangé aucune entrée : un
                        // `undo` défairait alors l'action précédente, sans
                        // rapport — on n'annule que ce qu'on vient de ranger.
                        if recorded && matches!(history.undo(&mut project), Ok(true)) {
                            end = RecalcEnd::Stopped;
                        }
                        formulas.run(&mut project);
                    }
                    if purge_orphaned_geometries(&project, &mut geometries) {
                        geometry_revision += 1;
                    }
                    // Les formules ont changé sous la même révision : un
                    // client qui compare des révisions (cache de rendu) doit
                    // voir qu'il y a du neuf.
                    revision += 1;
                    gauge.finish(id, end);
                } else {
                    formulas.run(&mut project);
                    if purge_orphaned_geometries(&project, &mut geometries) {
                        geometry_revision += 1;
                    }
                    let _ = reply.send(Ok(revision));
                }
            }
            Request::Undo { reply } => {
                let outcome = history.undo(&mut project).map(|reverted| {
                    reverted.then(|| {
                        revision += 1;
                        formulas.run(&mut project);
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
                        formulas.run(&mut project);
                        if purge_orphaned_geometries(&project, &mut geometries) {
                            geometry_revision += 1;
                        }
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::BeginMacro {
                kind,
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
                    let result = history.begin_macro(kind);
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
                        formulas.run(&mut project);
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::Recalculate { ids, client, reply } => {
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("recalculate"))
                } else {
                    formulas.recalculate(&mut project, &ids);
                    revision += 1;
                    Ok(revision)
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
                // `stokesforge-persistence` lui réserve, jamais mêlée à l'arbre
                // annulable.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen("save")))
                } else {
                    stokesforge_persistence::save_with_geometries(&project, &geometries, &path)
                        .map_err(SessionError::from)
                };
                let _ = reply.send(outcome);
            }
            Request::Load { path, reply } => {
                let outcome = load_project(&mut project, &mut history, &mut geometries, &path).map(
                    |report| {
                        revision += 1;
                        // Les valeurs d'une colonne-formule sont écrites sur
                        // le disque (`stokesforge-persistence`, voir sa doc) mais
                        // traitées comme un cache qu'on ne fait pas confiance
                        // à l'aveugle : un fichier a pu être modifié par un
                        // autre processus, une version antérieure de StokesForge,
                        // ou porter des colonnes dont l'ordre (donc les
                        // lettres) a changé sans que les valeurs suivent.
                        // Recalculer systématiquement ici est le choix
                        // retenu (peu coûteux — voir `recalc_formulas` — et
                        // toujours correct) plutôt que de faire confiance au
                        // contenu chargé. Le mémo est vidé : il décrit un
                        // projet qui n'existe plus.
                        formulas.forget();
                        formulas.run(&mut project);
                        // A3 — l'identité du projet change : une poignée
                        // (`stokesforge_script::surface::Handle`) qui a mémorisé
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
                        (revision, report)
                    },
                );
                let _ = reply.send(outcome);
            }
            Request::NewProject { reply } => {
                // Cycle de vie, comme Load : aucune entrée annulable ne peut
                // survivre au remplacement de l'identité du projet.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen(
                        "new project",
                    )))
                } else {
                    Project::new("Project1")
                        .map_err(CommandError::from)
                        .map_err(SessionError::from)
                        .map(|empty| {
                            project = empty;
                            let _ = history.clear();
                            geometries.clear();
                            formulas.forget();
                            revision += 1;
                            epoch += 1;
                            geometry_revision += 1;
                            revision
                        })
                };
                let _ = reply.send(outcome);
            }
            Request::Shutdown => {
                // Constat non reproductible (guide-de-correction-robustesse.md
                // §7.3, table de tête de `stokesforge-session`) : une macro encore
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
/// géométries lu dans le fichier (`stokesforge_persistence::load_with_geometries`
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
) -> SessionResult<LoadReport> {
    if history.in_macro() {
        return Err(SessionError::Command(CommandError::MacroOpen("load")));
    }
    let (loaded, loaded_geometries, report) = stokesforge_persistence::load_with_report(path)?;
    *project = loaded;
    *geometries = loaded_geometries;
    // L'acteur est mono-thread et rien n'a pu rouvrir de macro entre la
    // vérification ci-dessus et cet appel : `clear()` ne peut pas échouer
    // ici. On ignore tout de même le résultat plutôt que d'appeler
    // `unwrap`/`expect`, bannis hors tests.
    let _ = history.clear();
    Ok(report)
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
            undo_label: history.undo_label().cloned(),
            redo_label: history.redo_label().cloned(),
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
