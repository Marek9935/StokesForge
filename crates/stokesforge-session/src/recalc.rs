//! Recalcul des colonnes-formule, rejoué par l'acteur après chaque requête
//! qui a pu changer le projet (voir `actor::run`) — hors historique.
//!
//! # Ne recalculer que ce dont une source a changé (#193)
//!
//! Jusqu'à #193, chaque `Execute`/`Undo`/`Redo`/`AbortMacro` réussi
//! recalculait **toutes** les colonnes-formule de toutes les feuilles, sur
//! toutes leurs lignes — changer la couleur d'une courbe comprise. Le
//! [`Recalculator`] garde désormais, pour chaque colonne-formule, une
//! [`Fingerprint`] de ce qu'il a vu à son dernier calcul, et ne réécrit une
//! colonne que si cette empreinte a changé.
//!
//! ## Pourquoi une comparaison, et pas « A prévient B »
//!
//! Un vrai abonnement (A tient la liste des formules qui la lisent)
//! demanderait de maintenir ces listes à travers chaque annulation,
//! déplacement, copie, suppression et chargement — autant de chemins où un
//! oubli laisse une valeur fausse **en silence**. La comparaison n'a rien à
//! maintenir : elle relit, après chaque commande, l'état courant et le
//! confronte au mémo, ce qui ne coûte que des comparaisons d'entiers et de
//! textes courts. Aucune donnée de colonne n'est lue pour une colonne propre.
//!
//! ## Sur quoi repose la comparaison
//!
//! Sur le compteur de version des colonnes ([`stokesforge_core::Column::
//! version`]) et sa doctrine : **un numéro ne désigne jamais deux contenus
//! d'une même colonne** (A7, `guide-de-correction-robustesse.md` §F2). Toute
//! écriture de données, ordinaire, d'annulation ou de recalcul, reçoit un
//! numéro neuf du compteur unique du projet
//! (`Project::next_column_version`) ; une colonne insérée avec un numéro
//! (`AddColumn`, collage) fait passer ce compteur au-delà
//! (`Project::insert_node`) ; une restauration d'arbre (`Entry::Structural`)
//! ne ramène qu'une paire (version, contenu) qui a réellement coexisté.
//! Égalité de version ⇒ égalité de contenu : c'est l'hypothèse dont dépend
//! tout ce module, revérifiée dans l'acteur par la batterie `b22` et sur
//! l'historique par `b14`. Le sens contraire n'est pas requis : un même
//! contenu sous un numéro neuf ne coûte qu'un recalcul inutile.
//!
//! ## Ce que l'empreinte doit contenir — la règle à ne pas casser
//!
//! **Toute donnée qui entre dans le calcul d'une colonne-formule entre dans
//! son empreinte** : le texte de la formule, la colonne que désigne chaque
//! lettre (identifiant, pas lettre), la version de chacune, la longueur du
//! résultat (#194 : la plus longue des colonnes lues, à défaut celle des
//! colonnes ordinaires de la feuille), la position de la colonne elle-même, la version de
//! la colonne elle-même (une saisie directe dans une colonne-formule doit
//! être écrasée), et l'issue (formule illisible, prise dans un cycle,
//! calculée). Un calcul qui lirait une donnée de plus — le nom d'une
//! colonne, son unité, une autre feuille, une fonction `row()` — sans
//! l'ajouter ici serait « propre » à tort : la valeur affichée resterait
//! fausse sans erreur. Ce qu'il faut tester alors : `docs/tests.md`,
//! « Recalcul ciblé ».
//!
//! ## L'oracle
//!
//! Le recalcul complet n'a pas disparu : c'est [`FormulaRecalc::Full`], la
//! même boucle dont on ne consulte jamais le mémo. La batterie `b22` joue
//! des suites de commandes tirées au sort dans deux sessions, l'une ciblée,
//! l'autre complète, et exige des arbres égaux après chaque pas. Les deux
//! modes partagent tout le code sauf la décision « propre / à recalculer » :
//! c'est elle, et elle seule, que l'oracle éprouve.

//! # Calculs d'analyse (#181)
//!
//! Le même mécanisme suit les **calculs d'analyse** (`Tree::calculations`),
//! rejoués après les formules, hors historique (`Project::refresh_calculation_from`
//! réécrit les colonnes de sortie, les résultats et la péremption). Ce qui change :
//!
//! - l'**empreinte** d'un calcul est `CalcFingerprint` — la version de chaque
//!   colonne qu'il lit, et sa *définition* (opération, drapeaux, colonnes de
//!   sortie avec leur version). La règle du paragraphe précédent vaut ici aussi :
//!   toute donnée lue par `stokesforge_core::compute` entre dans l'empreinte ;
//! - un calcul **automatique** se recalcule quand l'empreinte change (toujours en
//!   mode `Full`) ; un **échec** garde les dernières valeurs et pose
//!   `Staleness::Failed`, sans nouvel essai tant que l'empreinte ne bouge pas ; une
//!   entrée disparue pose `Staleness::InputMissing` ;
//! - un calcul **manuel** (reconvolution, déconvolution, lots de plus de 20 séries)
//!   ne se relance jamais seul : un changement d'entrée le marque `Manual`. Il
//!   compare le **contenu** de ses entrées à celui du dernier calcul (`CalcMemo::seen`)
//!   plutôt que leurs versions — le mode complet réécrit les colonnes-formule à
//!   chaque passe, ce qui ferait changer les versions sans changer le contenu ;
//!   un changement de *définition* (`UpdateCalculation`, annulation) est un geste
//!   de l'utilisateur : le résultat est à jour par construction (`Trust`) ;
//! - au chargement le mémo est vidé et **aucun calcul n'est relancé** (A1, #316) :
//!   automatiques comme manuels sont crus, valeurs du fichier gardées
//!   (`Recalculator::run_after_load`, drapeau ponctuel `trust_loaded`, dans les
//!   deux modes). Leur marqueur de péremption vient du fichier, complété par la
//!   persistance : elle marque `Manual` un calcul dont l'empreinte du contenu
//!   des entrées (`Calculation::inputs_digest`) ne correspond plus. Les
//!   colonnes-formule, elles, restent recalculées au chargement ;
//! - formules et calculs se nourrissent mutuellement : `run` alterne les deux
//!   jusqu'à ce qu'un calcul n'écrive plus rien (au plus [`MAX_ROUNDS`] tours). Une
//!   boucle qui traverse un calcul (sa sortie nourrit une formule qui nourrit son
//!   entrée) ne se stabiliserait jamais : ses calculs sont marqués `Manual` et
//!   laissés tels quels (`calculations_in_a_cycle`).
//!
//! Tests : `tests/analyse.rs` (cas nommés), batterie `b22` (pas `AjouterCalcul`,
//! `RetirerCalcul`, `RecalculerPerimes`), mutants dans `docs/tests.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use stokesforge_analysis::ops::Operation;
use stokesforge_analysis::AnalysisError;
use stokesforge_core::{
    compute, evaluate_formula, topological_order, AuxRef, Calculation, ColumnType, ColumnValues,
    ComputeError, Formula, NodeId, NumericStatus, Project, SeriesRef, Staleness, ValueBlock,
};

/// Façon dont l'acteur recalcule les colonnes-formule après chaque requête
/// qui a pu changer le projet (voir [`crate::Session::spawn_with_recalc`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormulaRecalc {
    /// Ne réécrit une colonne-formule que si une donnée de son calcul a
    /// changé depuis le dernier (voir la doc de tête du module). Le mode de
    /// toute session de production.
    #[default]
    Targeted,
    /// Réécrit toutes les colonnes-formule de toutes les feuilles, à chaque
    /// fois — le comportement d'avant #193. Sert d'**oracle** au test
    /// différentiel (`stokesforge-robustesse`, batterie `b22`) : aucun
    /// client de production ne l'emploie.
    Full,
}

/// Le mémo du recalcul : ce que l'acteur a vu au dernier calcul de chaque
/// colonne-formule. Vit dans l'acteur, à côté du `Project` — ni dans
/// l'historique, ni dans le `.stf`.
pub(crate) struct Recalculator {
    mode: FormulaRecalc,
    memo: HashMap<NodeId, Fingerprint>,
    /// Pendant du mémo pour les calculs d'analyse (#181) : voir la section
    /// « Calculs d'analyse » de la doc de tête.
    calc_memo: HashMap<NodeId, CalcMemo>,
    /// Arrêt demandé de l'extérieur (#239), lu entre deux colonnes pendant
    /// [`Recalculator::run_interruptible`] ; `None` le reste du temps.
    stop: Option<Box<dyn Fn() -> bool + Send>>,
    /// Vrai dès que `stop` a répondu oui pendant la passe en cours.
    interrupted: bool,
    /// Drapeau **ponctuel** armé par `Load` (A1, #316) : pendant la passe qui
    /// suit, un calcul sans mémo est cru tel quel (`CalcAction::Trust`), même
    /// automatique. Désarmé dès la fin de cette passe : un calcul sans mémo
    /// après `AddCalculation` ou une annulation reste recalculé.
    trust_loaded: bool,
}

/// Nombre maximal de tours « formules → calculs » d'une passe : une chaîne
/// formule → calcul → formule → calcul se stabilise en autant de tours que de
/// maillons ; un cycle (la sortie d'un calcul qui nourrit sa propre entrée par
/// une formule) ne se stabilise jamais et s'arrête ici, sur le dernier état
/// calculé.
const MAX_ROUNDS: usize = 8;

impl Recalculator {
    pub(crate) fn new(mode: FormulaRecalc) -> Self {
        Self {
            mode,
            memo: HashMap::new(),
            calc_memo: HashMap::new(),
            stop: None,
            interrupted: false,
            trust_loaded: false,
        }
    }

    /// Comme [`Recalculator::run`], mais s'arrête entre deux colonnes (ou deux
    /// calculs) dès que `stop` répond oui (#239). Rend `true` si la passe est
    /// allée à son terme. Une passe interrompue laisse un état cohérent : chaque
    /// colonne est écrite d'un bloc et notée dans le mémo après coup, celles
    /// qui n'ont pas été atteintes sont simplement vues périmées par la
    /// passe suivante.
    pub(crate) fn run_interruptible(
        &mut self,
        project: &mut Project,
        stop: impl Fn() -> bool + Send + 'static,
    ) -> bool {
        self.stop = Some(Box::new(stop));
        self.interrupted = false;
        self.run(project);
        self.stop = None;
        !std::mem::take(&mut self.interrupted)
    }

    /// Point d'arrêt : vrai si la passe doit s'interrompre (sticky).
    fn should_stop(&mut self) -> bool {
        if !self.interrupted {
            self.interrupted = self.stop.as_ref().is_some_and(|stop| stop());
        }
        self.interrupted
    }

    /// Oublie tout : le prochain [`Recalculator::run`] recalcule chaque
    /// colonne-formule. Appelé quand le projet est remplacé (`Load`,
    /// `NewProject`) : un `.stf` porte des valeurs de formule qu'on ne croit
    /// pas sur parole (voir `actor::run`, branche `Load`), et ses versions de
    /// colonne n'ont aucun rapport avec celles du projet précédent.
    ///
    /// Ne vide que le mémo : les **colonnes-formule** sont recalculées par la
    /// passe suivante, mais les **calculs d'analyse** (A1, #316) ne le sont plus
    /// à l'ouverture — voir [`Recalculator::run_after_load`].
    pub(crate) fn forget(&mut self) {
        self.memo.clear();
        self.calc_memo.clear();
    }

    /// La passe qui suit un `Load` (A1, #316) : les colonnes-formule sont
    /// recalculées comme toujours, mais **aucun calcul d'analyse** ne l'est —
    /// ses valeurs sont celles du fichier, recalcul à la demande (« Recalculate
    /// »). Un calcul dont les entrées ont changé hors de StokesForge a déjà été
    /// marqué périmé par la persistance (empreinte du contenu des entrées,
    /// `stokesforge_persistence`), sans calcul non plus. Le drapeau est à usage
    /// unique : rendu à `false` avant de revenir.
    pub(crate) fn run_after_load(&mut self, project: &mut Project) {
        self.trust_loaded = true;
        self.run(project);
        self.trust_loaded = false;
    }

    /// Recalcule les colonnes-formule du projet qui en ont besoin — accès
    /// **direct** à `Project` (`Project::set_length`/`Project::set_values`),
    /// jamais via `History::apply`/`Command::execute` : c'est un effet de
    /// bord silencieux rejoué après coup, pas une mutation annulable (voir la
    /// doc de tête de crate, `docs/commandes-et-historique.md`) — exactement
    /// le même statut que `actor::purge_orphaned_geometries`. Le texte de la
    /// formule (`Command::SetColumnMeta`) et les cellules sources
    /// (`Command::SetCells`, `InsertRows`, `DeleteRows`, ...) restent, eux,
    /// annulables normalement : c'est seulement le recalcul qui ne pousse
    /// jamais sa propre entrée.
    ///
    /// # Portée du déclenchement
    ///
    /// Appelée après **tout** `Execute`/`Undo`/`Redo`/`AbortMacro` qui a
    /// réussi et après tout `Load` (voir `actor::run`, pour chaque site
    /// d'appel et sa justification propre).
    ///
    /// Tout l'arbre est **inspecté**, pas seulement « la feuille que la
    /// commande vient de toucher » : `Undo`/`Redo`/`AbortMacro` ne portent
    /// aucune commande à inspecter (seule `History` sait ce qu'elle vient de
    /// restaurer, et elle ne l'expose pas), et `Command::Move` peut déplacer
    /// une colonne d'une feuille à une autre, changeant les lettres des deux.
    /// Inspecter n'est pas recalculer : une colonne dont l'empreinte n'a pas
    /// bougé n'est ni lue ni réécrite. Le coût d'une passe sans rien à
    /// recalculer est linéaire dans le nombre de nœuds, plus l'analyse de
    /// chaque texte de formule — jamais la taille des données.
    pub(crate) fn run(&mut self, project: &mut Project) {
        self.run_formulas(project);
        self.run_calculation_rounds(project, None);
    }

    /// Relance des calculs d'analyse, quel que soit leur état (action
    /// « Recalculate »). `ids` vide : tous ceux qui portent un marqueur de
    /// péremption (« Recalculate Out of Date »). Hors historique, comme
    /// [`Recalculator::run`] dont elle reprend la propagation.
    pub(crate) fn recalculate(&mut self, project: &mut Project, ids: &[NodeId]) {
        let forced: HashSet<NodeId> = if ids.is_empty() {
            project
                .tree()
                .calculations()
                .iter()
                .filter(|(_, calc)| calc.stale.is_some())
                .map(|(id, _)| *id)
                .collect()
        } else {
            ids.iter().copied().collect()
        };
        self.run_formulas(project);
        self.run_calculation_rounds(project, Some(forced));
    }

    fn run_formulas(&mut self, project: &mut Project) {
        let sheets: Vec<NodeId> = project
            .tree()
            .iter()
            .filter(|(_, node)| node.kind().is_worksheet())
            .map(|(id, _)| id)
            .collect();
        let mut seen = HashSet::new();
        for sheet in sheets {
            if self.should_stop() {
                // Passe interrompue : `seen` est incomplet, ne pas élaguer.
                return;
            }
            self.recalc_sheet(project, sheet, &mut seen);
        }
        // Une colonne supprimée, ou qui a perdu sa formule, sort du mémo :
        // hygiène, pour que le mémo reste borné par les colonnes-formule
        // présentes. Ce n'est pas ce qui rend le mémo juste — un identifiant
        // n'est jamais réattribué dans un même projet (invariant 7), et
        // `forget` couvre le projet remplacé ; une entrée périmée ne pourrait
        // donc jamais être prise pour une autre colonne.
        self.memo.retain(|id, _| seen.contains(id));
    }

    /// Vrai si `column` doit être (ré)écrite : mode complet, colonne absente
    /// du mémo, ou empreinte différente.
    fn is_stale(&self, column: NodeId, fingerprint: &Fingerprint) -> bool {
        match self.mode {
            FormulaRecalc::Full => true,
            FormulaRecalc::Targeted => self.memo.get(&column) != Some(fingerprint),
        }
    }

    /// Note ce qui vient d'être écrit dans `column` : l'empreinte reçoit la
    /// version **après** écriture, celle que la prochaine passe doit
    /// retrouver si personne d'autre n'y a touché.
    fn record(&mut self, project: &Project, column: NodeId, mut fingerprint: Fingerprint) {
        if self.mode == FormulaRecalc::Full {
            return;
        }
        match column_version(project, column) {
            Some(version) => {
                fingerprint.own_version = version;
                self.memo.insert(column, fingerprint);
            }
            None => {
                self.memo.remove(&column);
            }
        }
    }

    /// Recalcule les colonnes-formule d'une seule feuille qui en ont besoin,
    /// dans l'ordre de leurs dépendances (tri topologique,
    /// [`stokesforge_core::topological_order`]) — une colonne-formule qui en
    /// référence une autre lit sa valeur **déjà recalculée**, jamais celle
    /// d'avant cette passe. Un cycle de dépendance (direct ou transitif) est
    /// détecté sans boucler : chaque colonne qui y participe reçoit un bloc
    /// entièrement `NaN` plutôt qu'une valeur qui n'aurait de toute façon
    /// aucun sens à choisir.
    ///
    /// La propagation est automatique : recalculer B écrit B, donc change sa
    /// version, et C = B + 1, qui vient après B dans l'ordre, voit son
    /// empreinte changer dans la même passe. L'ordre des étapes ci-dessous
    /// (illisibles, puis types, puis cycles, puis l'ordre topologique) fait
    /// qu'aucune colonne n'est écrite après qu'une colonne qui la lit a pris
    /// son empreinte : une seconde passe immédiate ne réécrit rien.
    fn recalc_sheet(&mut self, project: &mut Project, sheet: NodeId, seen: &mut HashSet<NodeId>) {
        let Ok(columns) = project.tree().children(sheet).map(<[NodeId]>::to_vec) else {
            return;
        };
        // Longueur d'un résultat (#194) : celle de la plus longue des colonnes
        // que la formule lit (calculée plus bas, colonne par colonne). À
        // défaut — formule illisible, prise dans un cycle, ou sans aucune
        // référence résolue — celle de la plus longue colonne **ordinaire**
        // de la feuille. Jamais une longueur qui compte les colonnes-formule
        // elles-mêmes : avant #194, `Tree::row_count` les comptait, et une
        // colonne-formule une fois allongée s'entretenait à cette longueur
        // (annuler l'allongement d'une source ne la raccourcissait plus).
        // Les colonnes ordinaires ne sont jamais écrites par la passe : cette
        // longueur est fixe pendant toute la passe.
        let ordinary_len = columns
            .iter()
            .filter_map(|&id| project.tree().node(id).and_then(|node| node.as_column()))
            .filter(|column| column.meta().formula.is_none())
            .map(stokesforge_core::Column::len)
            .max()
            .unwrap_or(0);

        // Formules analysées à l'avance : une erreur de *parse* est déjà un
        // résultat final (bloc `NaN`), elle ne participe pas au graphe de
        // dépendance — une colonne qui la référencerait la trouve
        // immédiatement disponible (à `NaN`), pas « en attente ».
        let mut parsed: BTreeMap<NodeId, (String, Formula)> = BTreeMap::new();
        for &id in &columns {
            let Some(source) = project
                .tree()
                .node(id)
                .and_then(|node| node.as_column())
                .and_then(|column| column.meta().formula.clone())
            else {
                continue;
            };
            seen.insert(id);
            match Formula::parse(&source) {
                Ok(formula) => {
                    parsed.insert(id, (source, formula));
                }
                Err(_error) => {
                    let fingerprint =
                        Fingerprint::new(project, id, source, Outcome::Unparsable, ordinary_len);
                    if self.is_stale(id, &fingerprint) {
                        write_formula_column(
                            project,
                            id,
                            vec![f64::NAN; ordinary_len],
                            &vec![NumericStatus::Missing; ordinary_len],
                        );
                        self.record(project, id, fingerprint);
                    }
                }
            }
        }

        // Type de sortie d'une colonne-formule : voir
        // `desired_formula_column_type`. Recalculé à chaque passe plutôt que
        // figé à la création : une formule référence potentiellement une
        // colonne qui apparaît ou disparaît ensuite. Appliqué directement au
        // modèle plutôt que par une commande annulable, comme
        // `write_formula_column` ci-dessous : c'est la même dérivation « hors
        // historique » que le recalcul des valeurs, pas une saisie
        // utilisateur à elle seule annulable. Pas de court-circuit par le
        // mémo ici : la comparaison des types ne lit aucune donnée, et un
        // changement de type change la version de la colonne, donc son
        // empreinte — elle sera réécrite plus bas.
        for (&id, (_, formula)) in &parsed {
            if let Some(desired) = desired_formula_column_type(formula, &columns, project) {
                let current = project
                    .tree()
                    .node(id)
                    .and_then(|node| node.as_column())
                    .map(stokesforge_core::Column::column_type);
                if current != Some(desired) {
                    let _ = project.set_column_type(id, desired);
                }
            }
        }

        // Graphe de dépendance entre colonnes-formule uniquement : une
        // référence vers une colonne ordinaire n'est pas une arête à
        // ordonner, elle est déjà disponible (voir la doc de
        // `stokesforge_core::topological_order`).
        let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> =
            parsed.keys().map(|&id| (id, BTreeSet::new())).collect();
        for (&id, (_, formula)) in &parsed {
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
            let Some((source, _)) = parsed.get(&id) else {
                continue;
            };
            // Un bloc `NaN` ne dépend que de la longueur et du type de la
            // colonne, déjà couvert par sa propre version (le type est fixé
            // juste au-dessus) : les sources n'entrent pas dans l'empreinte.
            // Y mettre leurs versions ferait se réécrire les membres d'un
            // cycle à chaque passe, chacun changeant la version de l'autre.
            // Leur longueur non plus : dans un cycle, elle se définirait par
            // elle-même ; c'est celle des colonnes ordinaires.
            let fingerprint =
                Fingerprint::new(project, id, source.clone(), Outcome::Blocked, ordinary_len);
            if self.is_stale(id, &fingerprint) {
                write_formula_column(
                    project,
                    id,
                    vec![f64::NAN; ordinary_len],
                    &vec![NumericStatus::Missing; ordinary_len],
                );
                self.record(project, id, fingerprint);
            }
        }
        for id in order {
            if self.should_stop() {
                return;
            }
            let Some((source, formula)) = parsed.get(&id) else {
                continue;
            };
            let Some(self_index) = columns.iter().position(|&c| c == id) else {
                continue;
            };
            // Lue ici, pas avant la boucle : les colonnes-formule déjà
            // traitées dans cette boucle (celles dont `id` dépend, forcément
            // plus tôt dans `order`) ont pu être réécrites, donc changer de
            // version — c'est ce qui propage le recalcul le long d'une
            // chaîne.
            let sources: Vec<_> = formula
                .referenced_columns()
                .into_iter()
                .map(|index| {
                    let resolved = columns.get(index).and_then(|&column_id| {
                        column_version(project, column_id).map(|version| (column_id, version))
                    });
                    (index, resolved)
                })
                .collect();
            // La plus longue des colonnes lues, lue elle aussi après les
            // colonnes-formule dont `id` dépend ; leur longueur suit leur
            // version, déjà dans l'empreinte.
            let target_len = sources
                .iter()
                .filter_map(|(_, resolved)| *resolved)
                .filter_map(|(column_id, _)| column_len(project, column_id))
                .max()
                .unwrap_or(ordinary_len);
            let fingerprint = Fingerprint::new(
                project,
                id,
                source.clone(),
                Outcome::Evaluated {
                    self_index,
                    sources,
                },
                target_len,
            );
            if !self.is_stale(id, &fingerprint) {
                continue;
            }
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
            // Rien n'est écrit, donc rien n'est noté : le mémo l'oublie, et la
            // prochaine passe réessaiera.
            if values.iter().any(Option::is_none) {
                self.memo.remove(&id);
                continue;
            }
            let values: Vec<_> = values.into_iter().flatten().collect();
            let evaluation = evaluate_formula(formula, self_index, &values, target_len);
            write_formula_column(project, id, evaluation.values, &evaluation.statuses);
            self.record(project, id, fingerprint);
        }
    }
}

/// Ce que l'acteur a vu au dernier calcul d'une colonne-formule : **toutes**
/// les données dont dépend ce qu'il y a écrit (voir la doc de tête du
/// module, « Ce que l'empreinte doit contenir »). Deux empreintes égales
/// garantissent que recalculer écrirait exactement le même contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    /// Texte de la formule, tel que saisi.
    source: String,
    /// L'issue du calcul, et les sources qu'elle a lues.
    outcome: Outcome,
    /// Longueur du résultat : la plus longue des colonnes lues, ou à défaut
    /// la plus longue colonne ordinaire de la feuille (#194). Pour une
    /// formule calculée, elle découle des versions des sources ; pour les
    /// autres issues, c'est la seule trace de la feuille dans l'empreinte.
    target_len: usize,
    /// Version de la colonne-formule elle-même. Comparée à la version
    /// courante avant d'écrire, remplacée par la version d'après écriture
    /// dans le mémo ([`Recalculator::record`]) : une saisie directe dans la
    /// colonne, l'annulation d'une telle saisie ou un changement de type la
    /// font changer, et la colonne est réécrite.
    own_version: u64,
}

impl Fingerprint {
    fn new(
        project: &Project,
        column: NodeId,
        source: String,
        outcome: Outcome,
        target_len: usize,
    ) -> Self {
        Self {
            source,
            outcome,
            target_len,
            own_version: column_version(project, column).unwrap_or(0),
        }
    }
}

/// L'issue d'un calcul de formule, avec ce qu'elle a lu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// La formule ne s'analyse pas : `NaN` partout, rien de lu.
    Unparsable,
    /// Prise dans un cycle, ou dépendante d'un cycle : `NaN` partout, rien
    /// de lu. Le type de sortie, qui dépend des colonnes désignées, est fixé
    /// avant l'écriture et changer de type change la version de la colonne.
    Blocked,
    /// Calculée. Chaque lettre référencée, avec la colonne qu'elle désigne et
    /// la version de son contenu (`None` : lettre hors de la feuille, le
    /// calcul dégrade en `NaN`). `self_index` est lu par
    /// `stokesforge_core::evaluate_formula` (auto-référence) : il entre donc
    /// dans l'empreinte, même si une auto-référence finit aujourd'hui dans
    /// [`Outcome::Blocked`] avant d'y arriver — aucun test ne peut donc le
    /// voir manquer (`docs/tests.md`, « Recalcul ciblé », mutants), et il
    /// reste là pour le jour où `evaluate_formula` s'en servirait autrement.
    Evaluated {
        self_index: usize,
        sources: Vec<(usize, Option<(NodeId, u64)>)>,
    },
}

/// Longueur courante de `column`, `None` si ce n'est pas (ou plus) une
/// colonne de l'arbre.
fn column_len(project: &Project, column: NodeId) -> Option<usize> {
    project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(stokesforge_core::Column::len)
}

/// Version courante de `column`, `None` si ce n'est pas (ou plus) une
/// colonne de l'arbre.
fn column_version(project: &Project, column: NodeId) -> Option<u64> {
    project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(stokesforge_core::Column::version)
}

/// Type que devrait porter la colonne-résultat de `formula` d'après les
/// colonnes qu'elle référence, ou `None` quand ça ne peut pas se décider
/// (référence hors bornes, ou vers une colonne qui a disparu) — dans ce cas,
/// l'appelant laisse le type de la colonne inchangé plutôt que de choisir au
/// hasard.
///
/// `Mixed` dès que la formule référence au moins une colonne — pas
/// seulement si l'une d'elles est déjà `Mixed`. N'importe quelle colonne,
/// même purement `Numeric`, peut avoir une case vide sur une ligne
/// quelconque ; or une colonne `Numeric` ne peut stocker que des nombres,
/// jamais le texte `"---"` qu'[`write_formula_column`] écrit désormais
/// aussi bien pour [`NumericStatus::Failed`] que pour
/// [`NumericStatus::Missing`] (case manquante, pas seulement en échec) —
/// sans quoi une opérande manquante resterait une case muette plutôt qu'un
/// résultat marqué comme non calculé. Seule une formule sans la moindre
/// référence (une constante pure, qui ne peut jamais manquer ni échouer)
/// reste `Numeric`.
fn desired_formula_column_type(
    formula: &Formula,
    columns: &[NodeId],
    project: &Project,
) -> Option<ColumnType> {
    let mut any_reference = false;
    for index in formula.referenced_columns() {
        let &column_id = columns.get(index)?;
        project
            .tree()
            .node(column_id)
            .and_then(|node| node.as_column())?;
        any_reference = true;
    }
    Some(if any_reference {
        ColumnType::Mixed
    } else {
        ColumnType::Numeric
    })
}

/// Écrit `values`/`statuses` dans `column`, en le tronquant/étendant d'abord
/// à sa longueur (`Project::set_length`) puisque `Project::set_values`
/// n'écrase jamais une queue de colonne plus longue que le bloc écrit.
/// Ignore silencieusement un échec (`column` disparu entre-temps, ce que
/// rien dans cette fonction ne provoque) plutôt que de paniquer — invariant
/// 10.
///
/// Pour une colonne-formule **mixte**, une ligne au statut
/// [`NumericStatus::Failed`] **ou** [`NumericStatus::Missing`] s'écrit
/// `MixedCell::Text("---")` plutôt que `MixedCell::Empty` : un résultat non
/// calculé, que ce soit parce qu'une colonne référencée porte du texte non
/// numérique (`Failed`) ou parce qu'une opérande manque tout simplement
/// (`Missing`), se voit toujours signalé plutôt que de laisser une case
/// muette qu'on confondrait avec une valeur pas encore saisie. `statuses`
/// doit avoir la même longueur que `values` ; sinon ce sont les valeurs
/// seules qui gouvernent (aucune ligne n'est marquée), comme avant l'ajout
/// de ce paramètre.
fn write_formula_column(
    project: &mut Project,
    column: NodeId,
    values: Vec<f64>,
    statuses: &[NumericStatus],
) {
    // Le bloc doit correspondre au type de la colonne, sans quoi
    // `set_values` refuse et le recalcul serait silencieusement perdu. Une
    // colonne-formule sans référence (constante pure) reste numérique, mais
    // dès qu'elle référence une colonne, `desired_formula_column_type` l'a
    // déjà retypée en mixte : `ValueBlock::into_mixed` fait alors des `NaN`
    // des cellules vides, ce que la formule voulait dire (invariant 10 —
    // dégrader, pas avaler) ; une ligne `Failed` ou `Missing` s'y écrit
    // `"---"` plutôt qu'une case vide, voir la doc ci-dessus.
    let is_mixed = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .is_some_and(|target| target.column_type() == ColumnType::Mixed);
    let length = values.len();
    let block = if is_mixed {
        let mut cells = ValueBlock::Float(values).into_mixed();
        if statuses.len() == cells.len() {
            for (cell, status) in cells.iter_mut().zip(statuses) {
                if matches!(status, NumericStatus::Failed | NumericStatus::Missing) {
                    *cell = stokesforge_core::MixedCell::Text("---".to_string());
                }
            }
        }
        ValueBlock::Mixed(cells)
    } else {
        ValueBlock::Float(values)
    };
    if project.set_length(column, length).is_err() {
        return;
    }
    let _ = project.set_values(column, 0, block);
}

// ── Calculs d'analyse (#181) ────────────────────────────────────────────

/// Ce que le recalcul a vu d'un calcul d'analyse à son dernier passage : les
/// versions des colonnes qu'il **lit**, et ce qui le **définit** (opération,
/// colonnes de sortie et leur version).
///
/// La séparation sert aux calculs manuels : un changement d'entrée les rend
/// périmés ; un changement de définition (`UpdateCalculation`, annulation) est
/// un geste de l'utilisateur dont le résultat est à jour par construction.
#[derive(Debug, Clone, PartialEq)]
struct CalcFingerprint {
    inputs: Vec<(NodeId, u64)>,
    definition: CalcDefinition,
}

#[derive(Debug, Clone, PartialEq)]
struct CalcDefinition {
    operation: Operation,
    /// La structure des entrées, pas seulement la liste à plat de leurs
    /// colonnes : `{x: None, y: A, σ: B}` et `{x: A, y: B, σ: None}` lisent les
    /// mêmes colonnes dans des rôles différents.
    series: Vec<SeriesRef>,
    aux: Option<AuxRef>,
    auto_recalc: bool,
    keep_results: bool,
    outputs: Vec<(NodeId, Option<u64>)>,
}

/// L'empreinte de `calc`, ou la première colonne d'entrée qui n'existe plus.
fn calc_fingerprint(project: &Project, calc: &Calculation) -> Result<CalcFingerprint, NodeId> {
    let mut inputs = Vec::new();
    for column in calc.input_columns() {
        match column_version(project, column) {
            Some(version) => inputs.push((column, version)),
            None => return Err(column),
        }
    }
    Ok(CalcFingerprint {
        inputs,
        definition: CalcDefinition {
            operation: calc.operation.clone(),
            series: calc.series.clone(),
            aux: calc.aux.clone(),
            auto_recalc: calc.auto_recalc,
            keep_results: calc.keep_results,
            outputs: calc
                .outputs
                .iter()
                .filter_map(|o| o.column)
                .map(|c| (c, column_version(project, c)))
                .collect(),
        },
    })
}

/// Le mémo d'un calcul : son empreinte et, pour un calcul **manuel** seulement,
/// le contenu de ses entrées à son dernier calcul (un `Arc` cloné, pas une
/// copie tant que la colonne n'est pas modifiée). Sans lui, une colonne
/// réécrite à l'identique — ce que fait le recalcul complet des formules à
/// chaque passe — périmerait un calcul que rien n'a changé.
struct CalcMemo {
    fingerprint: CalcFingerprint,
    seen: Vec<ColumnValues>,
}

/// Le contenu de chaque entrée est-il celui qu'on a vu ?
fn same_contents(project: &Project, inputs: &[(NodeId, u64)], seen: &[ColumnValues]) -> bool {
    inputs.len() == seen.len()
        && inputs.iter().zip(seen).all(|((column, _), before)| {
            project
                .tree()
                .node(*column)
                .and_then(|n| n.as_column())
                .is_some_and(|c| c.values() == before)
        })
}

fn snapshot_inputs(project: &Project, inputs: &[(NodeId, u64)]) -> Vec<ColumnValues> {
    inputs
        .iter()
        .filter_map(|(column, _)| {
            project
                .tree()
                .node(*column)
                .and_then(|n| n.as_column())
                .map(|c| c.values().clone())
        })
        .collect()
}

enum CalcAction {
    Skip,
    /// Les versions ont changé, pas le contenu : on retient les nouvelles.
    Renumber,
    /// Fait confiance aux données présentes (calcul manuel jamais vu, ou
    /// redéfini) : note l'empreinte, ne calcule rien.
    Trust,
    /// Une entrée a changé sous un calcul manuel.
    MarkStale,
    Recompute,
}

impl Recalculator {
    /// Parcourt les calculs dans l'ordre de leurs dépendances (la sortie de l'un
    /// peut être l'entrée d'un autre) ; si l'un a réécrit des données, repasse
    /// sur les formules — qui peuvent lire ces sorties — puis recommence, dans
    /// la limite de [`MAX_ROUNDS`]. Sans calcul dans le projet, ne coûte rien.
    fn run_calculation_rounds(&mut self, project: &mut Project, forced: Option<HashSet<NodeId>>) {
        if project.tree().calculations().is_empty() {
            self.calc_memo.clear();
            return;
        }
        let mut forced = forced;
        for _ in 0..MAX_ROUNDS {
            if self.should_stop() {
                return;
            }
            let changed = self.run_calculations(project, forced.take().as_ref());
            if !changed {
                break;
            }
            self.run_formulas(project);
        }
        let present = project.tree().calculations();
        self.calc_memo.retain(|id, _| present.contains_key(id));
    }

    /// Un tour sur tous les calculs. Rend `true` si des données ont changé.
    fn run_calculations(
        &mut self,
        project: &mut Project,
        forced: Option<&HashSet<NodeId>>,
    ) -> bool {
        // Qui produit quelle colonne ; un calcul dépend de ceux dont il lit une sortie.
        let mut producer: HashMap<NodeId, NodeId> = HashMap::new();
        for (&id, calc) in project.tree().calculations() {
            for column in calc.outputs.iter().filter_map(|o| o.column) {
                producer.insert(column, id);
            }
        }
        let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for (&id, calc) in project.tree().calculations() {
            let deps = calc
                .input_columns()
                .into_iter()
                .filter_map(|c| producer.get(&c).copied())
                .filter(|&p| p != id)
                .collect();
            edges.insert(id, deps);
        }
        let (order, blocked) = topological_order(&edges);
        let mut blocked: HashSet<NodeId> = blocked.into_iter().collect();
        blocked.extend(calculations_in_a_cycle(project));

        let mut changed = false;
        for id in order {
            if blocked.contains(&id) {
                continue;
            }
            if self.should_stop() {
                return changed;
            }
            changed |=
                self.update_calculation(project, id, forced.is_some_and(|f| f.contains(&id)));
        }
        // Un cycle (entre calculs, ou calcul → formule → calcul) n'a pas
        // d'ordre de calcul, et le relancer ne se stabiliserait jamais : on
        // les signale périmés sans y toucher.
        for id in blocked {
            let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
        }
        changed
    }

    /// Note l'empreinte vue de `calc` (et le contenu de ses entrées s'il est manuel).
    fn remember(
        &mut self,
        project: &Project,
        id: NodeId,
        calc: &Calculation,
        fingerprint: CalcFingerprint,
    ) {
        let seen = if calc.auto_recalc {
            Vec::new()
        } else {
            snapshot_inputs(project, &fingerprint.inputs)
        };
        self.calc_memo.insert(id, CalcMemo { fingerprint, seen });
    }

    fn update_calculation(&mut self, project: &mut Project, id: NodeId, force: bool) -> bool {
        let Some(calc) = project.tree().calculation(id).cloned() else {
            return false;
        };
        let fingerprint = match calc_fingerprint(project, &calc) {
            Ok(fingerprint) => fingerprint,
            Err(_missing) => {
                // Une colonne lue a disparu : les dernières valeurs restent, le
                // calcul est périmé. Retrouver la colonne (annulation) fait
                // repartir de zéro.
                let _ = project.set_calculation_stale(id, Some(Staleness::InputMissing));
                self.calc_memo.remove(&id);
                return false;
            }
        };
        let previous = self.calc_memo.get(&id);
        let action = if force {
            CalcAction::Recompute
        } else if self.trust_loaded && previous.is_none() {
            // Premier passage après un `Load`, dans les deux modes (sinon
            // l'oracle `Full` divergerait) : les valeurs du fichier font foi.
            CalcAction::Trust
        } else if calc.auto_recalc {
            match (self.mode, previous) {
                (FormulaRecalc::Targeted, Some(p)) if p.fingerprint == fingerprint => {
                    CalcAction::Skip
                }
                _ => CalcAction::Recompute,
            }
        } else {
            // Un calcul manuel n'obéit pas au mode : ses entrées se comparent
            // par le contenu, dans les deux modes (voir `CalcMemo`).
            match previous {
                None => CalcAction::Trust,
                Some(p) if p.fingerprint.definition != fingerprint.definition => CalcAction::Trust,
                Some(p) if p.fingerprint.inputs == fingerprint.inputs => CalcAction::Skip,
                Some(p) if same_contents(project, &fingerprint.inputs, &p.seen) => {
                    CalcAction::Renumber
                }
                Some(_) => CalcAction::MarkStale,
            }
        };
        match action {
            CalcAction::Skip => false,
            CalcAction::Renumber => {
                if let Some(memo) = self.calc_memo.get_mut(&id) {
                    memo.fingerprint = fingerprint;
                }
                false
            }
            CalcAction::Trust => {
                if calc.stale == Some(Staleness::InputMissing) {
                    // Les colonnes sont revenues mais rien ne dit que les
                    // valeurs gardées leur correspondent.
                    let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
                }
                self.remember(project, id, &calc, fingerprint);
                false
            }
            CalcAction::MarkStale => {
                if !matches!(calc.stale, Some(Staleness::Failed(_))) {
                    let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
                }
                false
            }
            CalcAction::Recompute => {
                match compute(
                    project.tree(),
                    &calc.operation,
                    &calc.series,
                    calc.aux.as_ref(),
                ) {
                    Ok(computed) => {
                        let changed = match project.refresh_calculation_from(id, computed) {
                            Ok(changed) => changed,
                            Err(_) => {
                                // Une sortie refuse le résultat (type changé,
                                // colonne protégée) : le dire, au lieu de
                                // mémoriser une empreinte et de laisser la
                                // colonne figée sans signe.
                                let _ = project.set_calculation_stale(
                                    id,
                                    Some(Staleness::Failed(AnalysisError::OutputUnwritable)),
                                );
                                self.remember(project, id, &calc, fingerprint);
                                return false;
                            }
                        };
                        // L'empreinte d'après écriture : celle que le prochain
                        // passage doit retrouver si personne n'y touche.
                        if let Some((after, fresh)) = project
                            .tree()
                            .calculation(id)
                            .cloned()
                            .and_then(|c| Some((calc_fingerprint(project, &c).ok()?, c)))
                        {
                            self.remember(project, id, &fresh, after);
                        }
                        changed
                    }
                    Err(ComputeError::MissingInput(_)) => {
                        let _ = project.set_calculation_stale(id, Some(Staleness::InputMissing));
                        self.calc_memo.remove(&id);
                        false
                    }
                    Err(ComputeError::Analysis(error)) => {
                        // Les dernières valeurs valides restent ; pas de nouvel
                        // essai tant qu'aucune entrée ne change.
                        let _ = project.set_calculation_stale(id, Some(Staleness::Failed(error)));
                        self.remember(project, id, &calc, fingerprint);
                        false
                    }
                }
            }
        }
    }
}

/// Les calculs pris dans une boucle qui traverse des colonnes-formule : la
/// sortie d'un calcul nourrit une formule qui nourrit (de proche en proche) une
/// entrée du même calcul. Une boucle faite de formules seules est déjà traitée
/// par `recalc_sheet` (colonnes à `NaN`) ; ici on cherche celles qui passent
/// par un calcul — le graphe complet moins le graphe des formules.
fn calculations_in_a_cycle(project: &Project) -> HashSet<NodeId> {
    let tree = project.tree();
    let mut formula_edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for (sheet, node) in tree.iter() {
        if !node.kind().is_worksheet() {
            continue;
        }
        let Ok(columns) = tree.children(sheet) else {
            continue;
        };
        for &id in columns {
            let deps = formula_edges.entry(id).or_default();
            let source = tree
                .node(id)
                .and_then(|n| n.as_column())
                .and_then(|c| c.meta().formula.clone());
            if let Some(Ok(formula)) = source.as_deref().map(Formula::parse) {
                deps.extend(
                    formula
                        .referenced_columns()
                        .into_iter()
                        .filter_map(|index| columns.get(index).copied()),
                );
            }
        }
    }
    let mut full_edges = formula_edges.clone();
    for calc in tree.calculations().values() {
        let inputs = calc.input_columns();
        for output in calc.outputs.iter().filter_map(|o| o.column) {
            full_edges
                .entry(output)
                .or_default()
                .extend(inputs.iter().copied());
        }
    }
    let (_, blocked_full) = topological_order(&full_edges);
    let (_, blocked_formulas) = topological_order(&formula_edges);
    tree.calculations()
        .iter()
        .filter(|(_, calc)| {
            calc.outputs
                .iter()
                .filter_map(|o| o.column)
                .any(|c| blocked_full.contains(&c) && !blocked_formulas.contains(&c))
        })
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use stokesforge_core::{Column, ColumnMeta, ColumnType, ColumnValues};

    use super::*;

    /// Cellules numériques d'une colonne ; `None` pour ce qui ne l'est pas
    /// (le « --- » d'une colonne-formule pas encore calculée).
    fn nombres(project: &Project, column: NodeId) -> Vec<Option<f64>> {
        let column = project
            .tree()
            .node(column)
            .and_then(|n| n.as_column())
            .expect("colonne");
        match column.values() {
            ColumnValues::Float(values) => values.iter().map(|v| Some(*v)).collect(),
            ColumnValues::Mixed(cells) => cells
                .iter()
                .map(|cell| match cell {
                    stokesforge_core::MixedCell::Number(v) => Some(*v),
                    _ => None,
                })
                .collect(),
        }
    }

    fn valeurs(expected: &[f64]) -> Vec<Option<f64>> {
        expected.iter().map(|v| Some(*v)).collect()
    }

    /// `A [1,2,3]` puis `B = A*2`, `C = B+1`, `D = C+1` (une chaîne : chaque
    /// colonne attend la précédente ; les lettres sont positionnelles).
    fn chaine() -> (Project, [NodeId; 3]) {
        let mut project = Project::new("Demo").expect("projet");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("A");
        let mut formulas = Vec::new();
        for (name, text) in [("B", "A*2"), ("C", "B+1"), ("D", "C+1")] {
            let column = project
                .add_column(sheet, None, Some(name), Column::new(ColumnType::Numeric))
                .expect("colonne");
            project
                .set_column_meta(
                    column,
                    ColumnMeta {
                        formula: Some(text.into()),
                        ..Default::default()
                    },
                )
                .expect("formule");
            formulas.push(column);
        }
        (project, [formulas[0], formulas[1], formulas[2]])
    }

    fn version(project: &Project, column: NodeId) -> u64 {
        column_version(project, column).expect("version")
    }

    /// #239 : l'arrêt tombe entre deux colonnes. Ce qui était écrit le reste,
    /// le reste attend ; la passe suivante achève sans réécrire ce qui l'est
    /// déjà (le mémo est resté juste) et retombe sur le résultat complet.
    #[test]
    fn un_arret_entre_deux_colonnes_laisse_un_etat_que_la_passe_suivante_acheve() {
        let (mut project, [c, d, e]) = chaine();
        let polls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&polls);
        let mut recalc = Recalculator::new(FormulaRecalc::Targeted);
        // 1er contrôle : avant la feuille ; 2e : avant C ; 3e : avant D.
        let completed = recalc.run_interruptible(&mut project, move || {
            counter.fetch_add(1, Ordering::SeqCst) + 1 >= 3
        });
        assert!(!completed, "la passe aurait dû être interrompue");
        assert_eq!(
            nombres(&project, c),
            valeurs(&[2.0, 4.0, 6.0]),
            "C est écrite"
        );
        assert!(
            nombres(&project, d).iter().all(Option::is_none)
                && nombres(&project, e).iter().all(Option::is_none),
            "D et E n'ont pas été atteintes"
        );
        let c_version = version(&project, c);

        recalc.run(&mut project);
        assert_eq!(nombres(&project, d), valeurs(&[3.0, 5.0, 7.0]));
        assert_eq!(nombres(&project, e), valeurs(&[4.0, 6.0, 8.0]));
        assert_eq!(
            version(&project, c),
            c_version,
            "C, déjà à jour, n'est pas réécrite"
        );
    }

    /// Témoin : sans demande d'arrêt, la passe interruptible va au bout.
    #[test]
    fn sans_arret_la_passe_interruptible_va_au_bout() {
        let (mut project, [_, _, e]) = chaine();
        let mut recalc = Recalculator::new(FormulaRecalc::Targeted);
        assert!(recalc.run_interruptible(&mut project, || false));
        assert_eq!(nombres(&project, e), valeurs(&[4.0, 6.0, 8.0]));
    }
}
