//! The **host** side of the `pyst` surface: navigation, metadata, column
//! reading and writing, undo, all of it **without ever touching the GIL**.
//!
//! This whole module runs inside a `Python::detach`, that is, with the GIL
//! released. Nothing here names a PyO3 type: this is what lets the `Ungil` marker
//! check at compile time that no GIL-bound value crosses the wait for the actor.
//!
//! # Path resolution is no longer rewritten here
//!
//! This module long carried a copy of `Project::resolve`: resolution lived in
//! the `Project`, which **the actor** owns, whereas a client only has an
//! `Arc<Tree>` through [`stokesforge_session::Snapshot`]. The duplicate was
//! accepted and documented, for lack of permission to touch the foundation.
//!
//! It was removed: `stokesforge-core` now exposes [`Tree::resolve`], the only
//! implementation of the `[Book1]Sheet1!ColA` grammar in the repository, and the
//! bridge calls it on its snapshot's tree. There is nothing left to keep in sync
//! here.

use std::path::Path;
use std::sync::Arc;

use crate::engine::ScriptContext;
use stokesforge_commands::{Command, CommandError, MacroKind};
use stokesforge_core::{ColumnMeta, ColumnValues, CoreError, Node, NodeId, ValueBlock};
use stokesforge_plot::{PlotSpec, SeriesAxis, ShapeKind, ShapeRef, ShapeSpec};
use stokesforge_session::{SessionError, SessionHandle, Snapshot};

/// Host error **already translated** into the vocabulary of the four
/// exceptions of the `pyst` surface.
///
/// The translation happens here, in pure Rust; the script thread converts it
/// to a Python exception, with the GIL in hand. No host error (`SessionError`,
/// `CommandError`, `CoreError`) is carried as is all the way to Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostError {
    /// Malformed object path, or one that designates nothing in the project.
    UnknownPath(String),
    /// The handle designates a node absent from the current tree: deleted, or
    /// from a project reloaded since (A3: the epoch remembered by the handle no
    /// longer matches that of the current snapshot).
    BrokenReference(String),
    /// Refused value: wrong type, wrong length, wrong kind of
    /// object.
    InvalidValue(String),
    /// #312: the column changed since the version read (or provided by
    /// `expected_version`): the write is refused at once, with no retry.
    ConcurrentWrite(String),
}

impl HostError {
    /// Translates a model error. The `match` is **exhaustive** on purpose:
    /// a new `CoreError` variant will force a translation decision rather than
    /// falling into a catch-all.
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
            // B1 (`guide-de-correction-robustesse.md` §F2/F3/F4): no more node
            // can be created; it is neither a path nor a broken reference, but the
            // command that would have created it that must be refused.
            CoreError::IdentifiersExhausted => {
                HostError::InvalidValue(CoreError::IdentifiersExhausted.to_string())
            }
            // #312 (D2): immediate refusal, with no retry: the script raises
            // `ConcurrentWriteError` and decides by itself whether to read again.
            CoreError::ConcurrentModification { .. } => HostError::ConcurrentWrite(format!(
                "{error}: another client modified the column since it was read; \
                 read it again before writing"
            )),
            // A formula column refuses any direct cell write
            // (`stokesforge-commands::History::apply_data`): on the script side, that is
            // an invalid value just like an incompatible type, not a broken
            // reference nor a version conflict.
            CoreError::FormulaColumnReadOnly { .. } => HostError::InvalidValue(error.to_string()),
            // Same reason for a column written by an analysis calculation (#181):
            // `pyst.analysis` recalculates it, a script does not write it by hand.
            CoreError::CalculatedColumnReadOnly { .. } => {
                HostError::InvalidValue(error.to_string())
            }
            // #253: a row transposes only if all the columns of
            // its sheet are numeric and carry an x'.
            CoreError::TransposeNoXPrime { .. } | CoreError::TransposeNotNumeric { .. } => {
                HostError::InvalidValue(error.to_string())
            }
        }
    }

    /// Translates a session error.
    ///
    /// `Disconnected` becomes a **broken reference**: when the session is
    /// closed, no handle of the script designates anything anymore. The message
    /// says so explicitly so the cause is not confused with a deleted
    /// column.
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

/// `pyst.resolve`: object path -> node identifier, and the project's epoch
/// at the time of resolution (A3, `guide-de-correction-robustesse.md`
/// §F2/F6). This is the pair that [`crate::surface::Handle`] remembers, to
/// be able to refuse later if the project changed identity in the meantime
/// (a `Load`).
pub(crate) fn resolve(session: &SessionHandle, path: &str) -> Result<(NodeId, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    // Any resolution error, syntax included, is a path we do not know how
    // to follow: it is `UnknownPathError` in both cases, and the original
    // message says which of the two.
    let id = snapshot
        .tree
        .resolve(path)
        .map_err(|error| HostError::UnknownPath(error.to_string()))?;
    Ok((id, snapshot.epoch))
}

/// Refuses (`BrokenReference`) when the epoch remembered by a handle no
/// longer matches that of the current snapshot (A3): a `Load` took place since
/// the handle was produced by `resolve`, and `id`, even if it still exists in
/// the new tree, may designate a completely different object there: exactly
/// the case of the reproduction
/// `b6_handle_survives_a_load_and_names_another_object`. Checked **before**
/// any lookup of `id` in the tree, not after: looking first would let through
/// the coincidence that this check exists to rule out.
fn check_epoch(id: NodeId, expected_epoch: u64, current_epoch: u64) -> Result<(), HostError> {
    if expected_epoch == current_epoch {
        return Ok(());
    }
    Err(HostError::BrokenReference(format!(
        "{id} belongs to a project that no longer exists: a project was loaded since \
         this handle was obtained"
    )))
}

/// Validates the identity before looking for the node. All the new read
/// calls follow the same order as the foundation's calls: an identifier that
/// reappeared after a load never makes an old handle valid.
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

/// Root of the current project, with its identity.
pub(crate) fn root(session: &SessionHandle) -> Result<(NodeId, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    Ok((snapshot.tree.root(), snapshot.epoch))
}

/// Direct children, in model order. Without a handle, starts from the root.
/// Identifiers and epoch come from the same snapshot, even if another
/// client loads a project while the script receives the reply.
pub(crate) fn children(
    session: &SessionHandle,
    handle: Option<(NodeId, u64)>,
) -> Result<Vec<(NodeId, u64)>, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (id, epoch) = handle.unwrap_or((snapshot.tree.root(), snapshot.epoch));
    let node = checked_node(&snapshot, id, epoch)?;
    Ok(node.children().iter().map(|id| (*id, epoch)).collect())
}

/// What [`info`] returns: the shared node, its row count if any, and the
/// column's formula in letters.
pub(crate) type NodeInfo = (Arc<Node>, Option<usize>, Option<String>);

/// Node description, row count if any, and formula of a column
/// **in letters** (today's, `#REF` for a read column that is no longer in the
/// sheet), on a single snapshot. Sharing the node avoids copying its buffers or
/// its file metadata to extract a few fields, once the GIL is taken back.
pub(crate) fn info(session: &SessionHandle, id: NodeId, epoch: u64) -> Result<NodeInfo, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let row_count = if node.kind().is_worksheet() {
        Some(snapshot.tree.row_count(id).map_err(HostError::from_core)?)
    } else {
        node.as_column().map(|column| column.len())
    };
    Ok((
        Arc::clone(node),
        row_count,
        snapshot.tree.formula_letters(id),
    ))
}

/// Stored cells, with no conversion: only the Arc is cloned here. The copy
/// into Python values belongs to the script thread, after the GIL is retaken.
pub(crate) fn column_values(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<(ColumnValues, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} is {}, not a column",
            node.kind().with_article()
        ))
    })?;
    Ok((column.values().clone(), column.version()))
}

/// `pyst.column`: the column's shared `f64` buffer, ready to be borrowed
/// without copy by numpy.
///
/// Returns a **clone of the `Arc`**: it is what keeps the buffer alive
/// under the numpy view, and what forces any later write on the model side to
/// go through `Arc::make_mut`, hence to allocate a new buffer.
pub(crate) fn float_buffer(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<(Arc<Vec<f64>>, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let node = checked_node(&snapshot, id, epoch)?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} is {}, not a column",
            node.kind().with_article()
        ))
    })?;
    // `float_values`, not `numeric_view`: this surface returns a view with no
    // copy on the column's buffer, which a mixed column does not have
    // (see `stokesforge_core::Column::numeric_view`). Returning it a
    // materialized copy would be a view that no longer follows the column: a
    // silent trap for a script that thinks it holds the buffer.
    let buffer = column.float_values().cloned().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "column \"{}\" is of type {}: pyst.column only returns numeric \
             columns",
            node.name(),
            column.column_type().type_name()
        ))
    })?;
    Ok((buffer, column.version()))
}

/// `pyst.set_column`: writes `values` into the column, **through a command**,
/// hence through the bus, hence through Ctrl+Z.
///
/// Refuses any length different from the column's (see the surface doc for
/// the reason for this restriction), **before** sending anything to the
/// actor: a refusal does not move the revision.
///
/// The length is validated before reserving the script's macro. If this
/// reservation is new, read again and recheck the epoch and the length:
/// a write or a Load may have gone through before the reservation. Under the
/// macro, other clients can no longer write.
///
/// #312: expected version, by priority: the script's `expected` argument,
/// then the version **retained at read time** (`pyst.column`/`pyst.values`), then
/// the current version (column never read: write allowed, not protected).
/// A refusal for a stale version is **immediate** (`ConcurrentWrite`).
pub(crate) fn set_column(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    values: Vec<f64>,
    expected: Option<u64>,
) -> Result<(), HostError> {
    let session = context.session();
    loop {
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
            // Validate again under the exclusive macro, before the command.
            continue;
        }
        let expected_version = expected
            .or_else(|| context.remembered_version(id, epoch))
            .unwrap_or_else(|| column.version());
        session
            .execute(Command::SetCells {
                column: id,
                start: 0,
                values: ValueBlock::Float(values),
                expected_version: Some(expected_version),
            })
            .map_err(HostError::from_session)?;
        retain_written_version(context, id, epoch)?;
        return Ok(());
    }
}

/// After a successful script write to a column **it has read**, the
/// retained version becomes the new version (read again on the snapshot, under
/// the macro): the script knows what it just wrote, its following writes do
/// not refuse themselves. A column never read stays unretained
/// (documented limit: unprotected write).
pub(crate) fn retain_written_version(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
) -> Result<(), HostError> {
    if context.remembered_version(id, epoch).is_none() {
        return Ok(());
    }
    let snapshot = context
        .session()
        .snapshot()
        .map_err(HostError::from_session)?;
    if let Some(column) = checked_node(&snapshot, id, epoch)?.as_column() {
        context.remember_version(id, epoch, column.version());
    }
    Ok(())
}

/// `pyst.undo`: one step of the host's history stack.
///
/// Returns `false` when there is nothing to undo: an empty stack is not an
/// error, it is the case the interface translates as a greyed-out button.
pub(crate) fn undo(context: &ScriptContext) -> Result<bool, HostError> {
    // Explicit undo is a commit point: it undoes the whole group built
    // so far, then allows new writes.
    context.finish_write(true)?;
    // #312: the versions read describe a state that the undo undoes.
    context.forget_versions();
    context
        .session()
        .undo()
        .map(|revision| revision.is_some())
        .map_err(HostError::from_session)
}

/// `pyst.redo`: symmetric to [`undo`]. An open write group is
/// first committed, which empties the redo stack: a `redo()` that follows
/// script writes therefore returns `false`.
pub(crate) fn redo(context: &ScriptContext) -> Result<bool, HostError> {
    context.finish_write(true)?;
    context.forget_versions();
    context
        .session()
        .redo()
        .map(|revision| revision.is_some())
        .map_err(HostError::from_session)
}

/// `pyst.save` / `pyst.save_as`. Saving is not a command
/// (it does not change the model): it goes through `SessionHandle::save`, like
/// the application. An open write group is first **committed** (commit point,
/// like `undo`): the session refuses to write a state that a macro
/// abort could undo. Without `path`, writes to the engine's current path; with
/// one, saves there and makes it the current path.
pub(crate) fn save(context: &ScriptContext, path: Option<&Path>) -> Result<(), HostError> {
    let target = match path {
        Some(path) => path.to_path_buf(),
        None => context.project_path().ok_or_else(|| {
            HostError::InvalidValue(
                "the project has no path yet: use pyst.save_as(path) first".to_owned(),
            )
        })?,
    };
    if context.protected_path().as_deref() == Some(target.as_path()) {
        return Err(HostError::InvalidValue(
            "the project file was read with some data left out (calculations that could not \
             be read): saving over it would delete them for good; use pyst.save_as(path) with \
             another path"
                .to_owned(),
        ));
    }
    context.finish_write(true)?;
    context
        .session()
        .save(&target)
        .map_err(HostError::from_session)?;
    if let Ok(snapshot) = context.session().snapshot() {
        context.record_saved((snapshot.revision, snapshot.geometry_revision));
    }
    if path.is_some() {
        context.set_project_path(target);
    }
    Ok(())
}

/// Default PNG rasterization factor of `pyst.export_graph`: the
/// application's (`DEFAULT_PNG_EXPORT_SCALE` of `stokesforge-app-state`, which this
/// crate cannot read).
pub(crate) const DEFAULT_PNG_SCALE: f32 = 4.0;

/// `pyst.export_graph`: the same rendering as the application's export (size
/// `DEFAULT_SIZE`, default fonts, Typst), the format coming from the extension.
/// The text of an SVG stays text (the application's default).
pub(crate) fn export_graph(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
    path: &Path,
    png_scale: f32,
) -> Result<(), HostError> {
    use stokesforge_render_svg::{ExportFormat, SvgTextMode};
    let format = ExportFormat::from_path(path, png_scale, SvgTextMode::Text).ok_or_else(|| {
        HostError::InvalidValue(format!(
            "unknown export format for {}: use a .svg, .png or .pdf extension",
            path.display()
        ))
    })?;
    if !png_scale.is_finite() || png_scale <= 0.0 {
        return Err(HostError::InvalidValue(
            "png_scale must be a positive number".to_owned(),
        ));
    }
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let spec = graph_spec(&snapshot, id, epoch)?;
    let plot = stokesforge_core::resolve::resolve_spec(&snapshot.tree, &spec);
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    stokesforge_render_svg::export_to_file(
        path,
        &plot,
        stokesforge_render_svg::DEFAULT_SIZE,
        &measurer,
        &stokesforge_render_svg::FontChoice::default(),
        format,
    )
    .map_err(|error| HostError::InvalidValue(error.to_string()))
}

/// The arguments of `pyst.add_shape`, as a script writes them: text and
/// numbers, validated here in pure Rust.
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

/// The spec of graph `id`, after the epoch and kind checks.
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

/// The ordinary (non-overlaid) layer of the cell `(row, column)`: the one
/// designated by `panel=(row, column)` on the Python side (#167).
pub(crate) fn base_layer_at(spec: &PlotSpec, row: usize, column: usize) -> Option<usize> {
    spec.panels
        .iter()
        .position(|p| p.row == row && p.column == column && p.overlay.is_none())
}

/// `pyst.add_shape`: places a graphic object above the others of the panel
/// and returns its rank. `SetPlotSpec` joins the script's write group,
/// even if the object is its first mutation (#67 + #76).
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
        // Epoch, panel and rank come from the reserved snapshot: no
        // other client can overwrite the graph between this read and the write.
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, index) = prepare(&snapshot)?;
    }
    set_graph_spec(session, id, spec)?;
    Ok(index)
}

/// `pyst.remove_shape`: removes the object of rank `index` from the panel, in the
/// same group as the other writes of the run.
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

/// Imports a file into a new workbook, one sheet per plane. All the
/// analysis and conversion stay those of the CLI and the application.
/// File errors are detected before the first command; an
/// application error aborts the macro, leaving no partial import.
/// The macro excludes other clients and project loads during
/// creation: the identifiers returned belong to a single epoch.
pub(crate) fn import_file(
    context: &ScriptContext,
    path: &Path,
    recipe_name: Option<&str>,
) -> Result<Vec<(NodeId, u64)>, HostError> {
    let session = context.session();
    let store = stokesforge_import::RecipeStore::load();
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
                .unwrap_or_else(|| stokesforge_import::ImportRecipe::new("Untitled"))
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
            // Like the CLI: if the file name is not a valid node name
            // (or if it is taken), keep an automatic name.
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
            // Abort undoes the write: the versions read are stale.
            context.forget_versions();
            Err(error)
        }
    }
}

/// Creation commands return no identifier: under the exclusive macro,
/// the last child of `parent` is the one just created.
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

/// Mutation snapshot: check before the reservation, then recheck
/// under the macro. A Load between these two gestures never rehabilitates a
/// handle from an old epoch, even if its identifier exists again.
pub(crate) fn write_snapshot(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
) -> Result<Snapshot, HostError> {
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

/// Creates at the end of the parent; the exclusive macro makes recovering
/// the last child reliable, as for import. A missing parent means the root.
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

/// Rename and delete reuse the model's generic commands.
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

/// Replaces the values, length included, without changing the type, identity
/// or metadata. Writing first closes the refusals (type, formula, cap)
/// before removing the tail: a caught refusal leaves no truncation.
/// The sheet's other columns are never resized.
pub(crate) fn set_values(
    context: &ScriptContext,
    id: NodeId,
    epoch: u64,
    cells: ValueBlock,
    expected: Option<u64>,
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
        // into_mixed normalizes NaN to Empty: here, keep the explicit NaN.
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
    // #312: same priority as `set_column`.
    let version = expected
        .or_else(|| context.remembered_version(id, epoch))
        .unwrap_or_else(|| column.version());
    let session = context.session();
    // Atomicity: SetCells then DeleteCells are a single gesture. A nested
    // macro (LIFO inside the `Script` macro, invariant 8) allows undoing
    // the write if the tail deletion failed. No failure path of
    // `DeleteCells` is known under the script's exclusive macro (the
    // column was just validated and nobody else writes): no black-box
    // witness, the guard stays defensive.
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
        Ok(()) => {
            session.end_macro().map_err(HostError::from_session)?;
            retain_written_version(context, id, epoch)
        }
        Err(error) => {
            session.abort_macro().map_err(HostError::from_session)?;
            Err(error)
        }
    }
}

/// Changes only the requested fields, preserving import provenance
/// and other metadata. The type and version stay read-only.
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
                    Some("x3") => stokesforge_core::ColumnRole::X3,
                    Some("y3") => stokesforge_core::ColumnRole::Y3,
                    Some("x4") => stokesforge_core::ColumnRole::X4,
                    Some("y4") => stokesforge_core::ColumnRole::Y4,
                    Some("z") => stokesforge_core::ColumnRole::Z,
                    Some("x_error") => stokesforge_core::ColumnRole::XError,
                    Some("y_error") => stokesforge_core::ColumnRole::YError,
                    Some("label") => stokesforge_core::ColumnRole::Label,
                    Some("none") => stokesforge_core::ColumnRole::None,
                    _ => return Err(HostError::InvalidValue(
                        "unknown column role: use x, y, x2, y2, x3, y3, x4, y4, z, x_error, y_error, label or none"
                            .into(),
                    )),
                }
            }
            "long_name" => meta.long_name = value.unwrap_or_default(),
            "unit" => meta.unit = value.unwrap_or_default(),
            "comment" => meta.comment = value.unwrap_or_default(),
            "display_format" => meta.display_format = value.unwrap_or_default(),
            // Only a provided `formula` key touches the formula: the other
            // fields leave the stored formula as is (never a round trip
            // through letters, otherwise a `#REF` would block any change
            // of the unit). Letters to identifiers through
            // `Tree::formula_from_letters`, like the interface.
            "formula" => {
                meta.formula = match value {
                    None => None,
                    Some(text) => Some(
                        snapshot
                            .tree
                            .formula_from_letters(id, &text)
                            .map_err(|error| HostError::InvalidValue(error.to_string()))?,
                    ),
                }
            }
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
