//! Le côté **hôte** de la surface `pyst` : navigation, métadonnées, lecture et
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
//! qu'un `Arc<Tree>` via [`stokesforge_session::Snapshot`]. Le doublon était assumé
//! et documenté, faute d'avoir le droit de toucher au socle.
//!
//! Il a été supprimé : `stokesforge-core` expose désormais [`Tree::resolve`], seule
//! implémentation de la grammaire `[Book1]Sheet1!ColA` du dépôt, et le pont
//! l'appelle sur l'arbre de son instantané. Il n'y a plus rien à garder
//! synchronisé ici.

use std::path::Path;
use std::sync::Arc;

use crate::engine::ScriptContext;
use stokesforge_commands::{Command, CommandError, MacroKind};
use stokesforge_core::{ColumnMeta, ColumnValues, CoreError, Node, NodeId, ValueBlock};
use stokesforge_plot::{PlotSpec, SeriesAxis, ShapeKind, ShapeRef, ShapeSpec};
use stokesforge_session::{SessionError, SessionHandle, Snapshot};

/// A2 (`guide-de-correction-robustesse.md` §F6, §4/§7 de
/// `cahier-des-charges-api-script.md`) — nombre de tentatives que
/// [`set_column`] s'accorde avant de renoncer. Cinq, comme le cahier des
/// charges le fixe : ni une variable d'environnement ni un paramètre, pour
/// que le contrat lu par un auteur de script soit celui que le code applique.
const MAX_SET_COLUMN_ATTEMPTS: u32 = 5;

/// Erreur de l'hôte **déjà traduite** dans le vocabulaire des quatre
/// exceptions de la surface `pyst`.
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
    pub(crate) fn from_core(error: CoreError) -> Self {
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
            // (`stokesforge-commands::History::apply_data`) : côté script, c'est
            // une valeur invalide au même titre qu'un type incompatible, pas
            // une référence cassée ni un conflit de version.
            CoreError::FormulaColumnReadOnly { .. } => HostError::InvalidValue(error.to_string()),
            // Même raison pour une colonne écrite par un calcul d'analyse (#181) :
            // `pyst.analysis` la recalcule, un script ne l'écrit pas à la main.
            CoreError::CalculatedColumnReadOnly { .. } => {
                HostError::InvalidValue(error.to_string())
            }
        }
    }

    /// Traduit une erreur de la session.
    ///
    /// `Disconnected` devient une **référence cassée** : quand la session est
    /// fermée, plus aucune poignée du script ne désigne quoi que ce soit. Le
    /// message le dit explicitement pour que la cause ne se confonde pas avec
    /// une colonne supprimée.
    pub(crate) fn from_session(error: SessionError) -> Self {
        match error {
            SessionError::Disconnected => HostError::BrokenReference(
                "the StokesForge session is closed: no handle is valid anymore".to_owned(),
            ),
            SessionError::Persistence(error) => HostError::InvalidValue(error.to_string()),
            SessionError::Command(CommandError::Core(error)) => Self::from_core(error),
            SessionError::Command(error) => HostError::InvalidValue(error.to_string()),
        }
    }
}

/// `pyst.resolve` : chemin d'objet → identifiant de nœud, et l'époque du
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

/// Valide l'identité avant de chercher le nœud. Tous les nouveaux appels
/// de lecture suivent le même ordre que les appels du socle : un identifiant
/// réapparu après un chargement ne rend jamais une ancienne poignée valide.
pub(crate) fn checked_node(
    snapshot: &Snapshot,
    id: NodeId,
    epoch: u64,
) -> Result<&Arc<Node>, HostError> {
    check_epoch(id, epoch, snapshot.epoch)?;
    snapshot
        .tree
        .node_arc(id)
        .ok_or_else(|| HostError::BrokenReference(format!("{id} no longer exists in the project")))
}

/// Racine du projet courant, avec son identité.
pub(crate) fn root(session: &SessionHandle) -> Result<(NodeId, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    Ok((snapshot.tree.root(), snapshot.epoch))
}

/// Enfants directs, dans l'ordre du modèle. Sans poignée, part de la racine.
/// Identifiants et époque proviennent du même instantané, même si un autre
/// client charge un projet pendant que le script reçoit la réponse.
pub(crate) fn children(
    session: &SessionHandle,
    handle: Option<(NodeId, u64)>,
) -> Result<Vec<(NodeId, u64)>, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (id, epoch) = handle.unwrap_or((snapshot.tree.root(), snapshot.epoch));
    let node = checked_node(&snapshot, id, epoch)?;
    Ok(node.children().iter().map(|id| (*id, epoch)).collect())
}

/// Description du nœud et nombre de lignes éventuel, sur un seul instantané.
/// Partager le nœud évite de recopier ses buffers ou ses métadonnées de fichier
/// pour en extraire quelques champs, une fois le GIL repris.
pub(crate) fn info(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<(Arc<Node>, Option<usize>), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let row_count = if node.kind().is_worksheet() {
        Some(snapshot.tree.row_count(id).map_err(HostError::from_core)?)
    } else {
        node.as_column().map(|column| column.len())
    };
    Ok((Arc::clone(node), row_count))
}

/// Cellules stockées, sans conversion : seul l'Arc est cloné ici. La copie
/// en valeurs Python appartient au fil de script, après la reprise du GIL.
pub(crate) fn column_values(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<ColumnValues, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} is {}, not a column",
            node.kind().with_article()
        ))
    })?;
    Ok(column.values().clone())
}

/// `pyst.column` : le buffer `f64` partagé de la colonne, prêt à être emprunté
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
    let node = checked_node(&snapshot, id, epoch)?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} is {}, not a column",
            node.kind().with_article()
        ))
    })?;
    // `float_values`, pas `numeric_view` : cette surface rend une vue sans
    // copie sur le buffer de la colonne, ce qu'une colonne mixte n'a pas
    // (voir `stokesforge_core::Column::numeric_view`). Lui rendre une copie
    // matérialisée serait une vue qui ne suit plus la colonne — un piège
    // silencieux pour un script qui croirait tenir le buffer.
    column.float_values().cloned().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "column \"{}\" is of type {}: pyst.column only returns numeric \
             columns",
            node.name(),
            column.column_type().type_name()
        ))
    })
}

/// `pyst.set_column` : écrit `values` dans la colonne, **par une commande**,
/// donc par le bus, donc par Ctrl+Z.
///
/// Refuse toute longueur différente de celle de la colonne (voir la doc de la
/// surface pour la raison de cette restriction), **avant** d'envoyer quoi que
/// ce soit à l'acteur : un refus ne fait pas bouger la révision.
///
/// La longueur est validée avant de réserver la macro du script. Si cette
/// réservation est nouvelle, relire et revérifier l'époque et la longueur :
/// une écriture ou un Load a pu passer avant la réservation. Sous la macro,
/// les autres clients ne peuvent plus écrire ; `expected_version` et la borne
/// historique de cinq tentatives restent des garde-fous au moment d'appliquer.
pub(crate) fn set_column(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    values: Vec<f64>,
) -> Result<(), HostError> {
    let session = context.session();
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
                "column \"{}\" is of type {}: pyst.set_column only writes numeric \
                 values",
                node.name(),
                column.column_type().type_name()
            )));
        }
        if values.len() != column.len() {
            return Err(HostError::InvalidValue(format!(
                "expected length {} for column \"{}\", got {}: pyst.set_column writes a whole \
                 column, it neither extends nor truncates it",
                column.len(),
                node.name(),
                values.len()
            )));
        }
        if context.begin_write()? {
            // Valider à nouveau sous la macro exclusive, avant la commande.
            continue;
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

/// `pyst.undo` : un cran de la pile d'historique de l'hôte.
///
/// Rend `false` quand il n'y a rien à annuler — une pile vide n'est pas une
/// erreur, c'est le cas que l'interface traduit par un bouton grisé.
pub(crate) fn undo(context: &ScriptContext) -> Result<bool, HostError> {
    // L'annulation explicite est un point de validation : elle annule le
    // groupe entier construit jusqu'ici, puis permet de nouvelles écritures.
    context.finish_write(true)?;
    context
        .session()
        .undo()
        .map(|revision| revision.is_some())
        .map_err(HostError::from_session)
}

/// Les arguments de `pyst.add_shape`, tels qu'un script les écrit : du texte et
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
            if !color.is_empty() && stokesforge_plot::parse_hex(color).is_none() {
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
pub(crate) fn graph_spec(
    snapshot: &Snapshot,
    id: NodeId,
    epoch: u64,
) -> Result<PlotSpec, HostError> {
    let node = checked_node(snapshot, id, epoch)?;
    match node.kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => Ok(spec.clone()),
        other => Err(HostError::InvalidValue(format!(
            "{id} is {}, not a graph",
            other.with_article()
        ))),
    }
}

pub(crate) fn set_graph_spec(
    session: &SessionHandle,
    graph: NodeId,
    spec: PlotSpec,
) -> Result<(), HostError> {
    session
        .execute(Command::SetPlotSpec { graph, spec })
        .map(|_| ())
        .map_err(HostError::from_session)
}

/// Le calque ordinaire (non superposé) de la case `(row, column)` : celui que
/// désigne `panel=(ligne, colonne)` côté Python (#167).
pub(crate) fn base_layer_at(spec: &PlotSpec, row: usize, column: usize) -> Option<usize> {
    spec.panels
        .iter()
        .position(|p| p.row == row && p.column == column && p.overlay.is_none())
}

/// `pyst.add_shape` : pose un objet graphique au-dessus des autres du panneau
/// et rend son rang. `SetPlotSpec` rejoint le groupe d'écritures du script,
/// même si l'objet est sa première mutation (#67 + #76).
pub(crate) fn add_shape(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    args: &ShapeArgs,
) -> Result<usize, HostError> {
    let shape = args.build()?;
    let prepare = |snapshot: &Snapshot| {
        let mut spec = graph_spec(snapshot, id, epoch)?;
        let (row, column) = args.panel;
        let target = base_layer_at(&spec, row, column)
            .and_then(|layer| ShapeRef::next_in(&spec, layer))
            .ok_or_else(|| {
                HostError::InvalidValue(format!(
                    "the graph has no panel at row {row}, column {column}"
                ))
            })?;
        target.set(&mut spec, shape.clone());
        Ok::<_, HostError>((spec, target.index))
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (mut spec, mut index) = prepare(&snapshot)?;
    if context.begin_write()? {
        // Époque, panneau et rang viennent de l'instantané réservé : aucun
        // autre client ne peut écraser le graphe entre cette lecture et l'écriture.
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, index) = prepare(&snapshot)?;
    }
    set_graph_spec(session, id, spec)?;
    Ok(index)
}

/// `pyst.remove_shape` : retire l'objet de rang `index` du panneau, dans le
/// même groupe que les autres écritures de l'exécution.
pub(crate) fn remove_shape(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    panel: (usize, usize),
    index: usize,
) -> Result<(), HostError> {
    let prepare = |snapshot: &Snapshot| {
        let mut spec = graph_spec(snapshot, id, epoch)?;
        let layer = base_layer_at(&spec, panel.0, panel.1).unwrap_or(usize::MAX);
        let target = ShapeRef {
            panel: layer,
            index,
        };
        if !target.remove(&mut spec) {
            return Err(HostError::InvalidValue(format!(
                "no shape {index} in the panel at row {}, column {}",
                panel.0, panel.1
            )));
        }
        Ok(spec)
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let mut spec = prepare(&snapshot)?;
    if context.begin_write()? {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        spec = prepare(&snapshot)?;
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
    context: &ScriptContext,
    path: &Path,
    recipe_name: Option<&str>,
) -> Result<Vec<(NodeId, u64)>, HostError> {
    let session = context.session();
    let store = stokesforge_import::RecipeStore::load_with_builtins();
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
            stokesforge_import::choose_recipe(store.recipes(), path, &bytes)
                .cloned()
                .unwrap_or_else(|| stokesforge_import::ImportRecipe::new("Sans titre"))
        }
    };
    let items = stokesforge_importer::import_items(path, &recipe);
    if let Some(error) = items.iter().find_map(|item| item.table.as_ref().err()) {
        return Err(HostError::InvalidValue(error.to_string()));
    }
    let plans = stokesforge_importer::plan_import(&items, recipe.files_layout);
    if plans.is_empty() || plans.iter().all(|plan| plan.columns.is_empty()) {
        return Err(HostError::InvalidValue("no column to import".into()));
    }
    context.begin_write()?;
    session
        .begin_macro(MacroKind::Import)
        .map_err(HostError::from_session)?;
    let result = (|| {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        let root = snapshot.tree.root();
        let epoch = snapshot.epoch;
        let book = add_created_node(
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
            let sheet = add_created_node(
                session,
                book,
                Command::AddWorksheet {
                    book,
                    name: Some(plan.name),
                },
            )
            .or_else(|error| match error {
                HostError::InvalidValue(_) => {
                    add_created_node(session, book, Command::AddWorksheet { book, name: None })
                }
                _ => Err(error),
            })?;
            for planned in plan.columns {
                if let Some(column) = stokesforge_importer::make_import_column(&items, planned, 0) {
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
pub(crate) fn add_created_node(
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
        .ok_or_else(|| HostError::InvalidValue("created node not found after creation".into()))
}

/// Instantané de mutation : vérifier avant la réservation, puis revérifier
/// sous la macro. Un Load entre ces deux gestes ne réhabilite jamais une
/// poignée d'une ancienne époque, même si son identifiant existe à nouveau.
fn write_snapshot(context: &ScriptContext, id: NodeId, epoch: u64) -> Result<Snapshot, HostError> {
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    checked_node(&snapshot, id, epoch)?;
    if context.begin_write()? {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        checked_node(&snapshot, id, epoch)?;
        Ok(snapshot)
    } else {
        Ok(snapshot)
    }
}

/// Crée en fin de parent ; la macro exclusive rend fiable la récupération
/// du dernier enfant, comme pour l'import. Le parent absent désigne la racine.
pub(crate) fn create_node(
    context: &ScriptContext,
    parent: Option<(NodeId, u64)>,
    command: impl FnOnce(NodeId) -> Command,
) -> Result<(NodeId, u64), HostError> {
    let (parent, epoch) = match parent {
        Some(handle) => handle,
        None => root(context.session())?,
    };
    write_snapshot(context, parent, epoch)?;
    let id = add_created_node(context.session(), parent, command(parent))?;
    Ok((id, epoch))
}

/// Renommage et suppression réutilisent les commandes génériques du modèle.
pub(crate) fn mutate_node(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    command: Command,
) -> Result<(), HostError> {
    write_snapshot(context, id, epoch)?;
    context
        .session()
        .execute(command)
        .map_err(HostError::from_session)?;
    Ok(())
}

/// Remplace les valeurs, longueur comprise, sans changer le type, l'identité
/// ni les métadonnées. Écrire d'abord ferme les refus (type, formule, plafond)
/// avant de retirer la queue : un refus rattrapé ne laisse pas de troncature.
/// Les autres colonnes de la feuille ne sont jamais redimensionnées.
pub(crate) fn set_values(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    cells: ValueBlock,
) -> Result<(), HostError> {
    let snapshot = context
        .session()
        .snapshot()
        .map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let column = node
        .as_column()
        .ok_or_else(|| HostError::InvalidValue("expected a column handle".into()))?;
    let values = match (column.column_type().is_float_backed(), cells) {
        (true, ValueBlock::Float(values)) => ValueBlock::Float(values),
        (true, ValueBlock::Mixed(cells)) => {
            let mut values = Vec::with_capacity(cells.len());
            for cell in cells {
                values.push(match cell {
                    stokesforge_core::MixedCell::Number(value) => value,
                    stokesforge_core::MixedCell::Empty => f64::NAN,
                    stokesforge_core::MixedCell::Text(_) => {
                        return Err(HostError::InvalidValue(
                            "text cannot be written to a numeric column: create a mixed column"
                                .into(),
                        ))
                    }
                });
            }
            ValueBlock::Float(values)
        }
        (false, ValueBlock::Mixed(cells)) => ValueBlock::Mixed(cells),
        // into_mixed normalise NaN en Empty : ici, conserver le NaN explicite.
        (false, ValueBlock::Float(values)) => ValueBlock::Mixed(
            values
                .into_iter()
                .map(stokesforge_core::MixedCell::Number)
                .collect(),
        ),
    };
    let length = values.len();
    let snapshot = write_snapshot(context, id, epoch)?;
    let column = checked_node(&snapshot, id, epoch)?
        .as_column()
        .ok_or_else(|| HostError::InvalidValue("expected a column handle".into()))?;
    let previous_length = column.len();
    let version = column.version();
    let session = context.session();
    // Atomicité : SetCells puis DeleteCells sont un seul geste. Une macro
    // imbriquée (LIFO dans la macro `Script`, invariant 8) permet de défaire
    // l'écriture si la suppression de la queue échouait. Aucun chemin d'échec
    // de `DeleteCells` n'est connu sous la macro exclusive du script (la
    // colonne vient d'être validée et personne d'autre n'écrit) : pas de
    // témoin boîte noire, la garde reste défensive.
    session
        .begin_macro(MacroKind::Script)
        .map_err(HostError::from_session)?;
    let result = (|| {
        session
            .execute(Command::SetCells {
                column: id,
                start: 0,
                values,
                expected_version: Some(version),
            })
            .map_err(HostError::from_session)?;
        if length < previous_length {
            session
                .execute(Command::DeleteCells {
                    columns: vec![id],
                    at: length,
                    count: previous_length - length,
                })
                .map_err(HostError::from_session)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => session
            .end_macro()
            .map(|_| ())
            .map_err(HostError::from_session),
        Err(error) => {
            session.abort_macro().map_err(HostError::from_session)?;
            Err(error)
        }
    }
}

/// Modifie uniquement les champs demandés, en préservant provenance d'import
/// et autres métadonnées. Le type et la version restent des lectures seules.
pub(crate) fn set_column_meta(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    changes: Vec<(String, Option<String>)>,
) -> Result<(), HostError> {
    let snapshot = write_snapshot(context, id, epoch)?;
    let column = checked_node(&snapshot, id, epoch)?
        .as_column()
        .ok_or_else(|| HostError::InvalidValue("expected a column handle".into()))?;
    let mut meta: ColumnMeta = column.meta().clone();
    for (key, value) in changes {
        match key.as_str() {
            "role" => {
                meta.role = match value.as_deref() {
                    Some("x") => stokesforge_core::ColumnRole::X,
                    Some("y") => stokesforge_core::ColumnRole::Y,
                    Some("x2") => stokesforge_core::ColumnRole::X2,
                    Some("y2") => stokesforge_core::ColumnRole::Y2,
                    Some("z") => stokesforge_core::ColumnRole::Z,
                    Some("x_error") => stokesforge_core::ColumnRole::XError,
                    Some("y_error") => stokesforge_core::ColumnRole::YError,
                    Some("label") => stokesforge_core::ColumnRole::Label,
                    Some("none") => stokesforge_core::ColumnRole::None,
                    _ => return Err(HostError::InvalidValue(
                        "unknown column role: use x, y, x2, y2, z, x_error, y_error, label or none"
                            .into(),
                    )),
                }
            }
            "long_name" => meta.long_name = value.unwrap_or_default(),
            "unit" => meta.unit = value.unwrap_or_default(),
            "comment" => meta.comment = value.unwrap_or_default(),
            "display_format" => meta.display_format = value.unwrap_or_default(),
            "formula" => meta.formula = value,
            _ => {
                return Err(HostError::InvalidValue(format!(
                    "unknown column metadata field: {key}"
                )))
            }
        }
    }
    context
        .session()
        .execute(Command::SetColumnMeta { column: id, meta })
        .map_err(HostError::from_session)?;
    Ok(())
}
