//! Le côté **hôte** de la surface `pyx` : résolution de chemins, lecture et
//! écriture de colonnes, annulation — le tout **sans jamais toucher au GIL**.
//!
//! Tout ce module s'exécute à l'intérieur d'un `Python::detach`, c'est-à-dire
//! GIL relâché. Rien ici ne nomme un type PyO3 : c'est ce qui permet au
//! marqueur `Ungil` de vérifier à la compilation qu'aucune valeur liée au GIL
//! ne traverse l'attente de l'acteur.
//!
//! # La résolution de chemin n'est plus réécrite ici
//!
//! Ce module a longtemps porté une copie de `Project::resolve` : la résolution
//! vivait dans le `Project`, que **l'acteur** possède, alors qu'un client n'a
//! qu'un `Arc<Tree>` via [`plotyx_session::Snapshot`]. Le doublon était assumé
//! et documenté, faute d'avoir le droit de toucher au socle.
//!
//! Il a été supprimé : `plotyx-core` expose désormais [`Tree::resolve`], seule
//! implémentation de la grammaire `[Book1]Sheet1!ColA` du dépôt, et le pont
//! l'appelle sur l'arbre de son instantané. Il n'y a plus rien à garder
//! synchronisé ici.

use std::path::Path;
use std::sync::Arc;

use plotyx_commands::{Command, CommandError, MacroKind};
use plotyx_core::{CoreError, NodeId, ValueBlock};
use plotyx_plot::{PlotSpec, SeriesAxis, ShapeKind, ShapeRef, ShapeSpec};
use plotyx_session::{SessionError, SessionHandle};

/// A2 (`guide-de-correction-robustesse.md` §F6, §4/§7 de
/// `cahier-des-charges-api-script.md`) — nombre de tentatives que
/// [`set_column`] s'accorde avant de renoncer. Cinq, comme le cahier des
/// charges le fixe : ni une variable d'environnement ni un paramètre, pour
/// que le contrat lu par un auteur de script soit celui que le code applique.
const MAX_SET_COLUMN_ATTEMPTS: u32 = 5;

/// Erreur de l'hôte **déjà traduite** dans le vocabulaire des quatre
/// exceptions de la surface `pyx`.
///
/// La traduction se fait ici, côté Rust pur ; c'est le fil de script qui la
/// convertit en exception Python, GIL en main. Aucune erreur de l'hôte
/// (`SessionError`, `CommandError`, `CoreError`) n'est transportée telle
/// quelle jusqu'à Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostError {
    /// Chemin d'objet mal formé, ou qui ne désigne rien dans le projet.
    UnknownPath(String),
    /// La poignée désigne un nœud absent de l'arbre courant — supprimé, ou
    /// d'un projet rechargé depuis (A3 : l'époque mémorisée par la poignée
    /// ne coïncide plus avec celle de l'instantané courant).
    BrokenReference(String),
    /// Valeur refusée : mauvais type, mauvaise longueur, mauvaise nature
    /// d'objet.
    InvalidValue(String),
    /// A2 — [`set_column`] a essuyé [`MAX_SET_COLUMN_ATTEMPTS`] refus
    /// consécutifs pour cause d'écriture concurrente sur la même colonne.
    ConcurrentWrite(String),
}

impl HostError {
    /// Traduit une erreur du modèle. Le `match` est **exhaustif** à dessein :
    /// une nouvelle variante de `CoreError` forcera une décision de traduction
    /// plutôt que de tomber dans un fourre-tout.
    fn from_core(error: CoreError) -> Self {
        match error {
            CoreError::Path(message) => HostError::UnknownPath(message),
            CoreError::UnknownNode(id) => {
                HostError::BrokenReference(format!("{id} no longer exists in the project"))
            }
            CoreError::InvalidName(message)
            | CoreError::DuplicateName(message)
            | CoreError::Structure(message)
            | CoreError::IndexOutOfBounds(message)
            | CoreError::TypeMismatch(message) => HostError::InvalidValue(message),
            // B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — plus aucun
            // nœud ne peut être créé : ce n'est ni un chemin, ni une
            // référence cassée, mais la commande qui l'aurait créé qui doit
            // être refusée.
            CoreError::IdentifiersExhausted => {
                HostError::InvalidValue(CoreError::IdentifiersExhausted.to_string())
            }
            // A2 — n'atteint normalement jamais cette traduction générique :
            // [`set_column`] intercepte cette variante avant tout appel à
            // `from_session`/`from_core`, pour retenter plutôt que traduire.
            // Couverte ici pour l'exhaustivité du `match`, avec le même
            // vocabulaire que la traduction que `set_column` construit
            // elle-même après la cinquième tentative.
            CoreError::ConcurrentModification { .. } => {
                HostError::ConcurrentWrite(error.to_string())
            }
            // Une colonne-formule refuse toute écriture directe de cellule
            // (`plotyx-commands::History::apply_data`) : côté script, c'est
            // une valeur invalide au même titre qu'un type incompatible, pas
            // une référence cassée ni un conflit de version.
            CoreError::FormulaColumnReadOnly { .. } => HostError::InvalidValue(error.to_string()),
        }
    }

    /// Traduit une erreur de la session.
    ///
    /// `Disconnected` devient une **référence cassée** : quand la session est
    /// fermée, plus aucune poignée du script ne désigne quoi que ce soit. Le
    /// message le dit explicitement pour que la cause ne se confonde pas avec
    /// une colonne supprimée.
    fn from_session(error: SessionError) -> Self {
        match error {
            SessionError::Disconnected => HostError::BrokenReference(
                "the PlotYx session is closed: no handle is valid anymore".to_owned(),
            ),
            SessionError::Persistence(error) => HostError::InvalidValue(error.to_string()),
            SessionError::Command(CommandError::Core(error)) => Self::from_core(error),
            SessionError::Command(error) => HostError::InvalidValue(error.to_string()),
        }
    }
}

/// `pyx.resolve` : chemin d'objet → identifiant de nœud, et l'époque du
/// projet au moment de la résolution (A3, `guide-de-correction-robustesse.md`
/// §F2/F6) — c'est cette paire que [`crate::surface::Handle`] mémorise, pour
/// pouvoir refuser plus tard si le projet a changé d'identité entre-temps
/// (un `Load`).
pub(crate) fn resolve(session: &SessionHandle, path: &str) -> Result<(NodeId, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    // Toute erreur de résolution — syntaxe comprise — est un chemin qu'on ne
    // sait pas suivre : c'est `UnknownPathError` dans les deux cas, et le
    // message d'origine dit lequel des deux.
    let id = snapshot
        .tree
        .resolve(path)
        .map_err(|error| HostError::UnknownPath(error.to_string()))?;
    Ok((id, snapshot.epoch))
}

/// Refuse (`BrokenReference`) quand l'époque mémorisée par une poignée ne
/// coïncide plus avec celle de l'instantané courant (A3) : un `Load` a eu
/// lieu depuis que la poignée a été produite par `resolve`, et `id`, même
/// s'il existe encore dans le nouvel arbre, peut y désigner un tout autre
/// objet — exactement le cas de la reproduction
/// `b6_poignee_survit_a_un_load_et_designe_un_autre_objet`. Vérifié **avant**
/// toute recherche de `id` dans l'arbre, pas après : chercher d'abord
/// laisserait passer la coïncidence que cette vérification existe pour
/// écarter.
fn check_epoch(id: NodeId, expected_epoch: u64, current_epoch: u64) -> Result<(), HostError> {
    if expected_epoch == current_epoch {
        return Ok(());
    }
    Err(HostError::BrokenReference(format!(
        "{id} belongs to a project that no longer exists: a project was loaded since \
         this handle was obtained"
    )))
}

/// `pyx.column` : le buffer `f64` partagé de la colonne, prêt à être emprunté
/// sans copie par numpy.
///
/// Rend un **clone de l'`Arc`** : c'est lui qui maintiendra le buffer vivant
/// sous la vue numpy, et qui forcera toute écriture ultérieure côté modèle à
/// passer par `Arc::make_mut`, donc à allouer un buffer neuf.
pub(crate) fn float_buffer(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<Arc<Vec<f64>>, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    check_epoch(id, epoch, snapshot.epoch)?;
    let node = snapshot.tree.node(id).ok_or_else(|| {
        HostError::BrokenReference(format!("{id} no longer exists in the project"))
    })?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} is {}, not a column",
            node.kind().with_article()
        ))
    })?;
    // `float_values`, pas `numeric_view` : cette surface rend une vue sans
    // copie sur le buffer de la colonne, ce qu'une colonne mixte n'a pas
    // (voir `plotyx_core::Column::numeric_view`). Lui rendre une copie
    // matérialisée serait une vue qui ne suit plus la colonne — un piège
    // silencieux pour un script qui croirait tenir le buffer.
    column.float_values().cloned().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "column \"{}\" is of type {}: pyx.column only returns numeric \
             columns",
            node.name(),
            column.column_type().type_name()
        ))
    })
}

/// `pyx.set_column` : écrit `values` dans la colonne, **par une commande**,
/// donc par le bus, donc par Ctrl+Z.
///
/// Refuse toute longueur différente de celle de la colonne (voir la doc de la
/// surface pour la raison de cette restriction), **avant** d'envoyer quoi que
/// ce soit à l'acteur : un refus ne fait pas bouger la révision.
///
/// # Concurrence optimiste (A2)
///
/// Entre l'instantané lu ici et l'instant où l'acteur applique la commande,
/// un autre client peut avoir modifié la même colonne — c'est exactement le
/// scénario de `b6_toctou_entre_instantane_et_commande`. La commande porte
/// donc `expected_version`, vérifié par `History::apply` au moment
/// d'appliquer, jamais avant : si la version a bougé, l'acteur refuse sans
/// toucher au modèle, et cette fonction relit un instantané frais et
/// retente — jusqu'à [`MAX_SET_COLUMN_ATTEMPTS`] tentatives, exactement la
/// politique documentée dans `cahier-des-charges-api-script.md` (§4, §7).
/// « Recalculer » pour cette surface-témoin, qui n'écrit qu'une colonne
/// entière de longueur fixe, veut dire : rejeter proprement
/// (`InvalidValueError`) si la longueur attendue a changé entre-temps, ou
/// renvoyer la même valeur contre la version fraîchement lue sinon. Au-delà
/// de [`MAX_SET_COLUMN_ATTEMPTS`] échecs, `ConcurrentWriteError` (§7)
/// plutôt que de retenter indéfiniment.
pub(crate) fn set_column(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
    values: Vec<f64>,
) -> Result<(), HostError> {
    for _ in 0..MAX_SET_COLUMN_ATTEMPTS {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        check_epoch(id, epoch, snapshot.epoch)?;
        let node = snapshot.tree.node(id).ok_or_else(|| {
            HostError::BrokenReference(format!("{id} no longer exists in the project"))
        })?;
        let column = node.as_column().ok_or_else(|| {
            HostError::InvalidValue(format!(
                "{id} is {}, not a column",
                node.kind().with_article()
            ))
        })?;
        if column.float_values().is_none() {
            return Err(HostError::InvalidValue(format!(
                "column \"{}\" is of type {}: pyx.set_column only writes numeric \
                 values",
                node.name(),
                column.column_type().type_name()
            )));
        }
        if values.len() != column.len() {
            return Err(HostError::InvalidValue(format!(
                "expected length {} for column \"{}\", got {}: pyx.set_column writes a whole \
                 column, it neither extends nor truncates it",
                column.len(),
                node.name(),
                values.len()
            )));
        }
        let expected_version = column.version();
        match session.execute(Command::SetCells {
            column: id,
            start: 0,
            values: ValueBlock::Float(values.clone()),
            expected_version: Some(expected_version),
        }) {
            Ok(_) => return Ok(()),
            // A2 — un autre client a modifié la colonne entre l'instantané
            // lu ci-dessus et l'application de cette commande : on relit un
            // instantané frais et on retente, plutôt que de refuser sec ou
            // d'écraser silencieusement son travail.
            Err(SessionError::Command(CommandError::Core(CoreError::ConcurrentModification {
                ..
            }))) => continue,
            Err(other) => return Err(HostError::from_session(other)),
        }
    }
    Err(HostError::ConcurrentWrite(format!(
        "writing column {id} was refused {MAX_SET_COLUMN_ATTEMPTS} times in a row: \
         another client keeps modifying it; read the current state again or retry later"
    )))
}

/// `pyx.undo` : un cran de la pile d'historique de l'hôte.
///
/// Rend `false` quand il n'y a rien à annuler — une pile vide n'est pas une
/// erreur, c'est le cas que l'interface traduit par un bouton grisé.
pub(crate) fn undo(session: &SessionHandle) -> Result<bool, HostError> {
    session
        .undo()
        .map(|revision| revision.is_some())
        .map_err(HostError::from_session)
}

/// Les arguments de `pyx.add_shape`, tels qu'un script les écrit : du texte et
/// des nombres, validés ici en Rust pur.
#[derive(Debug, Clone)]
pub(crate) struct ShapeArgs {
    pub(crate) kind: String,
    pub(crate) points: (f64, f64, f64, f64),
    pub(crate) panel: (usize, usize),
    pub(crate) axis: String,
    pub(crate) line_color: String,
    pub(crate) line_width: f64,
    pub(crate) fill_color: String,
}

impl ShapeArgs {
    fn build(&self) -> Result<ShapeSpec, HostError> {
        let kind = match self.kind.as_str() {
            "rectangle" => ShapeKind::Rectangle,
            "ellipse" => ShapeKind::Ellipse,
            "line" => ShapeKind::Line,
            "arrow" => ShapeKind::Arrow,
            other => {
                return Err(HostError::InvalidValue(format!(
                    "unknown shape kind \"{other}\": use \"rectangle\", \"ellipse\", \"line\" \
                     or \"arrow\""
                )))
            }
        };
        let axis = match self.axis.as_str() {
            "left" => SeriesAxis::Left,
            "right" => SeriesAxis::Right,
            other => {
                return Err(HostError::InvalidValue(format!(
                    "unknown axis \"{other}\": use \"left\" or \"right\""
                )))
            }
        };
        for (name, color) in [
            ("line_color", &self.line_color),
            ("fill_color", &self.fill_color),
        ] {
            if !color.is_empty() && plotyx_plot::parse_hex(color).is_none() {
                return Err(HostError::InvalidValue(format!(
                    "{name} \"{color}\" is not a color: use \"#RRGGBB\" or \"#RRGGBBAA\", or \"\""
                )));
            }
        }
        if !self.line_width.is_finite() || self.line_width <= 0.0 {
            return Err(HostError::InvalidValue(
                "line_width must be a positive number".to_owned(),
            ));
        }
        let (x1, y1, x2, y2) = self.points;
        let shape = ShapeSpec {
            axis,
            line_color: self.line_color.clone(),
            line_width: self.line_width,
            fill_color: self.fill_color.clone(),
            ..ShapeSpec::new(kind, x1, y1, x2, y2)
        };
        if !shape.is_finite() {
            return Err(HostError::InvalidValue(
                "shape coordinates must be finite numbers".to_owned(),
            ));
        }
        Ok(shape)
    }
}

/// Le spec du graphe `id`, après les contrôles d'époque et de nature.
fn graph_spec(session: &SessionHandle, id: NodeId, epoch: u64) -> Result<PlotSpec, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    check_epoch(id, epoch, snapshot.epoch)?;
    let node = snapshot.tree.node(id).ok_or_else(|| {
        HostError::BrokenReference(format!("{id} no longer exists in the project"))
    })?;
    match node.kind() {
        plotyx_core::node::NodeKind::Graph { spec, .. } => Ok(spec.clone()),
        other => Err(HostError::InvalidValue(format!(
            "{id} is {}, not a graph",
            other.with_article()
        ))),
    }
}

fn set_graph_spec(session: &SessionHandle, graph: NodeId, spec: PlotSpec) -> Result<(), HostError> {
    session
        .execute(Command::SetPlotSpec { graph, spec })
        .map(|_| ())
        .map_err(HostError::from_session)
}

/// `pyx.add_shape` : pose un objet graphique au-dessus des autres du panneau
/// et rend son rang. Une commande `SetPlotSpec`, donc un Ctrl+Z.
pub(crate) fn add_shape(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
    args: &ShapeArgs,
) -> Result<usize, HostError> {
    let shape = args.build()?;
    let mut spec = graph_spec(session, id, epoch)?;
    let (row, column) = args.panel;
    let target = ShapeRef::next_in(&spec, row, column).ok_or_else(|| {
        HostError::InvalidValue(format!(
            "the graph has no panel at row {row}, column {column}"
        ))
    })?;
    target.set(&mut spec, shape);
    set_graph_spec(session, id, spec)?;
    Ok(target.index)
}

/// `pyx.remove_shape` : retire l'objet de rang `index` du panneau.
pub(crate) fn remove_shape(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
    panel: (usize, usize),
    index: usize,
) -> Result<(), HostError> {
    let mut spec = graph_spec(session, id, epoch)?;
    let target = ShapeRef {
        row: panel.0,
        column: panel.1,
        index,
    };
    if !target.remove(&mut spec) {
        return Err(HostError::InvalidValue(format!(
            "no shape {index} in the panel at row {}, column {}",
            panel.0, panel.1
        )));
    }
    set_graph_spec(session, id, spec)
}

/// Importe un fichier dans un classeur neuf, une feuille par plan. Toute
/// l'analyse et la conversion restent celles du CLI et de l'application.
/// Les erreurs de fichier sont détectées avant la première commande ; une
/// erreur d'application avorte la macro, sans laisser un import partiel.
/// La macro exclut les autres clients et les chargements de projet pendant
/// la création : les identifiants rendus appartiennent à une seule époque.
pub(crate) fn import_file(
    session: &SessionHandle,
    path: &Path,
    recipe_name: Option<&str>,
) -> Result<Vec<(NodeId, u64)>, HostError> {
    let store = plotyx_import::RecipeStore::load_with_builtins();
    let recipe = match recipe_name {
        Some(name) => store
            .recipes()
            .iter()
            .find(|r| r.name == name)
            .cloned()
            .ok_or_else(|| HostError::InvalidValue(format!("unknown template \"{name}\"")))?,
        None => {
            let bytes = std::fs::read(path)
                .map_err(|err| HostError::InvalidValue(format!("{}: {err}", path.display())))?;
            plotyx_import::choose_recipe(store.recipes(), path, &bytes)
                .cloned()
                .unwrap_or_else(|| plotyx_import::ImportRecipe::new("Sans titre"))
        }
    };
    let items = plotyx_importer::import_items(path, &recipe);
    if let Some(error) = items.iter().find_map(|item| item.table.as_ref().err()) {
        return Err(HostError::InvalidValue(error.to_string()));
    }
    let plans = plotyx_importer::plan_import(&items, recipe.files_layout);
    if plans.is_empty() || plans.iter().all(|plan| plan.columns.is_empty()) {
        return Err(HostError::InvalidValue("no column to import".into()));
    }
    session
        .begin_macro(MacroKind::Import)
        .map_err(HostError::from_session)?;
    let result = (|| {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        let root = snapshot.tree.root();
        let epoch = snapshot.epoch;
        let book = add_import_node(
            session,
            root,
            Command::AddEmptyWorkbook {
                parent: root,
                name: None,
            },
        )?;
        let mut sheets = Vec::new();
        for plan in plans {
            // Comme le CLI : si le nom du fichier n'est pas un nom de nœud
            // valide (ou s'il est pris), garder un nom automatique.
            let sheet = add_import_node(
                session,
                book,
                Command::AddWorksheet {
                    book,
                    name: Some(plan.name),
                },
            )
            .or_else(|error| match error {
                HostError::InvalidValue(_) => {
                    add_import_node(session, book, Command::AddWorksheet { book, name: None })
                }
                _ => Err(error),
            })?;
            for planned in plan.columns {
                if let Some(column) = plotyx_importer::make_import_column(&items, planned, 0) {
                    session
                        .execute(Command::AddColumn {
                            sheet,
                            index: None,
                            name: None,
                            column,
                        })
                        .map_err(HostError::from_session)?;
                }
            }
            sheets.push((sheet, epoch));
        }
        Ok(sheets)
    })();
    match result {
        Ok(sheets) => {
            session.end_macro().map_err(HostError::from_session)?;
            Ok(sheets)
        }
        Err(error) => {
            session.abort_macro().map_err(HostError::from_session)?;
            Err(error)
        }
    }
}

/// Les commandes de création ne rendent pas d'identifiant : sous la macro
/// exclusive, le dernier enfant de `parent` est celui qu'on vient de créer.
fn add_import_node(
    session: &SessionHandle,
    parent: NodeId,
    command: Command,
) -> Result<NodeId, HostError> {
    session.execute(command).map_err(HostError::from_session)?;
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    snapshot
        .tree
        .node(parent)
        .and_then(|n| n.children().last().copied())
        .ok_or_else(|| HostError::InvalidValue("imported node not found after creation".into()))
}
