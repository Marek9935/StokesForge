//! Persistence for StokesForge: `save`/`load` of a complete [`stokesforge_core::Project`].
//!
//! # Frozen boundary
//!
//! Two functions, one direction each, a complete `Project` in/out and
//! nothing else:
//!
//! ```text
//! save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
//! load(path: &Path) -> Result<Project, PersistenceError>;
//! ```
//!
//! No caller knows the on-disk format; if it ever had to be
//! replaced (Zarr was considered), nothing above would have to
//! change. Since V1 the format itself is frozen (invariant 12). Loading
//! is **complete**: the whole project fits in memory, there is
//! no partial or deferred loading.
//!
//! # The format is not written into the model
//!
//! `stokesforge-core` derives no `Serialize`/`Deserialize`: this crate defines
//! its **own transport structures** (`schema.rs`, private) and writes by
//! hand the conversions to/from the in-memory model (`convert.rs`).
//! The on-disk schema is an explicit object, versioned independently of the
//! core types — making them diverge one day (renaming a field of the model
//! without breaking the files already written, for example) will only cost one
//! conversion function to adjust.
//!
//! # Five properties held from version 1
//!
//! Adding them afterwards would cost a migration; they are therefore in the
//! format since version 1, frozen (version rule: `docs/persistance.md`):
//!
//! 1. **Atomic write.** `save` writes to a temporary file in the same
//!    directory as the destination, then renames it onto the target
//!    (`tempfile::NamedTempFile::persist`). A reader never sees a
//!    half-written file; a write that fails leaves no trace
//!    on the destination.
//! 2. **Version number first.** The 4 bytes that follow the
//!    signature are the version number, read and checked **before**
//!    touching the body of the file. A file newer than this reader is
//!    cleanly refused (`UnsupportedVersion`), without trying to decode a
//!    body it would not necessarily understand. A file that is not a
//!    StokesForge file (wrong signature, or too short to carry one) is
//!    refused just as cleanly (`NotAStokesForgeFile`).
//! 3. **Missing value vs empty string.** Text columns distinguish
//!    `None` (missing) from `Some(String::new())` (empty string present) —
//!    this is already the convention of the core, this format only preserves it.
//!    Numeric missing values are `NaN`, written and read back
//!    **bit for bit** (`f64::to_le_bytes`/`from_le_bytes` keep the exact
//!    bit pattern).
//! 4. **Persisted `NodeId` counter.** `next_id_hint` — the **next**
//!    identifier to assign, not the largest already assigned — is written
//!    in the file; at load, [`Project::from_tree`] restarts
//!    the counter at `max(persisted value, largest id read + 1)`. A
//!    file whose persisted value lags behind the identifiers
//!    it contains (file written by a buggy version) therefore never
//!    makes the counter go backwards.
//! 5. **Unknown fields ignored at read.** This is what will make it possible
//!    to add data later (a graph specification, for
//!    example) without breaking the files already written: an older reader
//!    simply ignores what it does not know. **Direct consequence**:
//!    the container must be self-describing, that is, name its fields
//!    rather than position them. `bincode` encodes structures as
//!    positional tuples — adding a field there shifts everything after it, and
//!    removing one is just as breaking. **MessagePack** (`rmp-serde`) allows
//!    it: `rmp_serde::to_vec_named` serializes each structure as a
//!    named map, and its reader natively ignores a key that a structure
//!    does not declare. Any field added after version 1 will carry
//!    `#[serde(default)]`, so that an older file that does not carry it
//!    stays readable.
//!
//! The numeric column buffers escape the generic MessagePack encoding:
//! they are written as raw little-endian bytes via
//! `serde_bytes` (8 bytes per `f64`), not as a MessagePack array
//! of elements — a million points weigh 8 MB on disk, not a multiple of
//! that figure inflated by per-element encoding.
//!
//! # File layout
//!
//! ```text
//! bytes 0..4   signature "STKF"
//! bytes 4..8   FORMAT_VERSION, u32 little-endian
//! bytes 8..    MessagePack body (FileBody), rmp_serde::to_vec_named
//! ```
//!
//! The body lists the nodes of the tree in **pre-order from the
//! root** (`Tree::walk_all`): two successive saves of the same project
//! therefore produce files that are identical byte for byte.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod calculations;
pub mod clipboard;
mod convert;
mod error;
mod recipe_record;
mod schema;
mod transposes;
mod unknown;
mod version;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use stokesforge_core::{Node, NodeId, Project, Tree};

use convert::ConvertError;
use schema::{FileBody, NodeRecord, FORMAT_VERSION, MAGIC};

pub use error::{PersistenceError, PersistenceResult};

/// Maximum size of a `.stf` at open when the caller sets none
/// (A2, #316): 512 MB. The file not being compressed, this is also the
/// decompressed size. The application makes it adjustable (preferences).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Global cell budget at open when the caller sets none
/// (B1, #316): one billion. It is the sum of the lengths of all the
/// columns declared by the file; the application makes it adjustable.
pub const DEFAULT_MAX_CELLS: u64 = 1_000_000_000;

/// The caps applied when opening a `.stf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadLimits {
    /// Maximum size of the file, in bytes (A2).
    pub max_bytes: u64,
    /// Maximum number of declared cells, all columns together (B1).
    pub max_cells: u64,
}

impl Default for LoadLimits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_FILE_BYTES,
            max_cells: DEFAULT_MAX_CELLS,
        }
    }
}

/// A calculation that reading could not keep: its record is
/// unreadable (unknown or invalid operation, identifier out of bounds). Neutral
/// data: putting it into a sentence is the application's business.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardedCalculation {
    /// Identifier of the calculation in the file.
    pub id: u64,
    /// Operation family (`"baseline"`…) when the record still carries it;
    /// `None` if even that is unreadable.
    pub kind: Option<String>,
}

/// What the reading of a `.stf` degraded without failing (invariant 10):
/// the caller shows it to the user instead of losing it silently. The
/// dropped items are **not kept** — an unreadable record cannot be
/// rebuilt — and therefore disappear at the next
/// save.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Analysis calculations dropped (#278).
    pub discarded_calculations: Vec<DiscardedCalculation>,
    /// Correction traces dropped (#309): unreadable, out of bounds, of invalid shape
    /// or of an operation other than a correction. Counted here, but
    /// without a dedicated warning text for now.
    pub discarded_corrections: Vec<DiscardedCalculation>,
}

impl LoadReport {
    /// Nothing was dropped.
    pub fn is_clean(&self) -> bool {
        self.discarded_calculations.is_empty() && self.discarded_corrections.is_empty()
    }
}

/// Position and size of a UI window (workbook, graph, matrix,
/// note), in logical points — same unit as the rest of egui.
///
/// # Why this type lives here and not in `stokesforge-session`
///
/// The geometry of the windows lives outside the model tree, on the actor side of
/// `stokesforge-session` (see the head doc of that crate): it is neither a field
/// of `WindowMeta`, nor anything persistent in `stokesforge-core`. By
/// default this type should therefore have been defined in `stokesforge-session`. The
/// structural reason that rules it out: this crate (`stokesforge-persistence`) is
/// **below** `stokesforge-session` in the dependency graph
/// (`stokesforge-session` depends on `stokesforge-persistence`, never the reverse), and
/// [`save_with_geometries`]/[`load_with_geometries`] need to name this
/// type in their public signature. Defining it here, where `stokesforge-session`
/// simply reuses it (`pub use stokesforge_persistence::WindowGeometry`),
/// avoids a type duplicated on both sides of a conversion that would have nothing
/// to do. Unlike the rest of the schema (`schema.rs`), there is no
/// separate "model" type from which it would have to stay independent (the doctrine
/// "the format is not written into the model" concerns `stokesforge-core`, which
/// precisely does not carry this field): this public type therefore serves directly as the
/// on-disk representation, without a private `Record` that would mirror it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    /// Abscissa of the top-left corner.
    pub x: f64,
    /// Ordinate of the top-left corner.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// State of the window (open, maximized, closed, collapsed, stacking
    /// rank). `None`: never saved (earlier file, or window
    /// never seen) — the application then applies its default initial
    /// layout. Never changes `x`/`y`/`width`/`height`, which
    /// stay the **floating** geometry whatever happens to the window
    /// (issue #16).
    pub view: Option<WindowView>,
}

/// Display state of a content window, kept with its floating
/// [`WindowGeometry`]. A detached window is saved
/// `Open`: detaching is not remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowView {
    /// Open, maximized or closed.
    pub mode: WindowMode,
    /// Window collapsed onto its title bar.
    pub collapsed: bool,
    /// Stacking rank among the open windows: 0 = at the very bottom,
    /// the largest rank is in the foreground. A relative rank, not an
    /// egui layer value.
    pub stack: u32,
}

/// Mode of a content window. An unknown value (file of a newer
/// version) reads as `Closed`: graceful degradation, never a load
/// failure (invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// Floating window, open.
    Open,
    /// Window maximized in the workspace.
    Maximized,
    /// Window closed (its floating geometry is kept).
    Closed,
}

/// The **view** tables that a `.stf` carries next to the undoable tree, by
/// node identifier: never in `Project`, never through a command
/// (invariant 1 does not concern them, they are not model data).
/// A struct rather than a tuple: one more table (#325) no longer breaks
/// every caller.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewTables {
    /// Position, size and state of the content windows.
    pub window_geometries: HashMap<NodeId, WindowGeometry>,
    /// Width of the sheet columns (#325), in logical points. Each
    /// value is a strictly positive finite real: reading drops the
    /// rest, writing too.
    pub column_widths: HashMap<NodeId, f32>,
}

/// A column width that the file can carry: a strictly positive
/// finite real (invariant 10: NaN, ∞ or ≤ 0 dropped, never a failure).
pub fn is_valid_column_width(width: f32) -> bool {
    width.is_finite() && width > 0.0
}

/// Upper bound of a node identifier or of `next_id_hint` read from a file
/// (B1, `guide-de-correction-robustesse.md` §F3): far above
/// any real project, far below `u64::MAX` — the place where
/// `checked_add` of `IdAllocator::next` (mechanical, already in place on the
/// `stokesforge-core` side) refuses anyway, but too late for a loaded project
/// to stay usable.
const MAX_NODE_ID: u64 = 1_000_000_000_000;

/// Saves `project` to `path`, atomically: a temporary file is
/// written in the same directory as `path` then renamed onto the target; in
/// case of failure at any step, `path` is untouched and the
/// temporary is deleted (behavior of `NamedTempFile` on abandonment).
///
/// Two successive calls on the same `project` produce files that are
/// identical byte for byte (deterministic order of the nodes: pre-order
/// from the root).
///
/// # Detecting a concurrent writer (B8)
///
/// Before writing, `save` compares the save counter currently on
/// disk (if a file already exists at `path`) with the one that **this
/// instance of `project`, in this process,** saw the last time it
/// was loaded or saved to this same path. If they differ, another
/// writer modified the file in the meantime: `save` refuses
/// (`PersistenceError::ConcurrentModification`) rather than overwrite its
/// work silently. This tracking is an in-memory registry, specific to the
/// process (see [`tracked_counter`]) — none of it goes through the
/// disk except the counter itself (`FileBody::save_counter`), in line with
/// the doctrine of stamps (§F2 of the guide): only the value that makes
/// the comparison possible is persisted, never the identity of who saw it.
///
/// A `project` that this process has **never** loaded or saved to this
/// path (a new project, or a save to a new destination)
/// has no baseline to compare: `save` proceeds without a check, as
/// before B8 — it can only detect a change since IT saw the
/// file, not since the beginning of time.
///
/// Equivalent to [`save_with_geometries`] with empty view tables:
/// writes no window geometry block nor column width block. It is the function used by
/// callers that do not know this notion (`stokesforge-app`);
/// `stokesforge-session`, which owns the `NodeId -> WindowGeometry` table next to
/// the `Project`, calls `save_with_geometries` so that the file gets back
/// the window layout and the column width at reopening.
pub fn save(project: &Project, path: &Path) -> PersistenceResult<()> {
    save_with_geometries(project, &ViewTables::default(), path)
}

/// Like [`save`], but also writes `view` into blocks **separate from
/// the undoable tree** (`FileBody::window_geometries`, `FileBody::column_widths`)
/// — never mixed with the `NodeRecord`s of `nodes`, never passed through `convert.rs`
/// (which only converts `Node`s). It is the function called by
/// `stokesforge-session::actor` so that the position of the windows and the width
/// of the columns survive a reload of the project. The widths whose
/// key is no longer a node of the tree, or which are not strictly
/// positive finite reals, are **not written**: the actor does not purge its
/// table on mutation (the width of a deleted column must come back with
/// Ctrl+Z, invariant 7), so this is where dead keys fall away.
pub fn save_with_geometries(
    project: &Project,
    view: &ViewTables,
    path: &Path,
) -> PersistenceResult<()> {
    // A file that this process never loaded or wrote to this path:
    // nothing proves that it reads back, so it is checked in full before
    // being replaced. A file already tracked (equal counter) is ours.
    let tracked = tracked_counter(path, project);
    let disk_counter = peek_save_counter(path, tracked.is_none())?;
    if let Some(found) = disk_counter {
        if let Some(expected) = tracked {
            if expected != found {
                return Err(PersistenceError::ConcurrentModification {
                    path: path.to_path_buf(),
                    expected,
                    found,
                });
            }
        }
    }
    let save_counter = disk_counter.unwrap_or(0) + 1;

    let parent_dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let tree = project.tree();
    let nodes: Vec<NodeRecord> = tree
        .walk_all()
        .into_iter()
        .filter_map(|id| tree.node(id))
        .map(convert::node_to_record)
        .collect();
    let window_geometries = view
        .window_geometries
        .iter()
        .map(|(id, geometry)| (id.raw(), (*geometry).into()))
        .collect();
    let column_widths = view
        .column_widths
        .iter()
        .filter(|(id, width)| tree.node(**id).is_some() && is_valid_column_width(**width))
        .map(|(id, width)| (id.raw(), *width))
        .collect();
    let body = FileBody {
        root: tree.root().raw(),
        next_id_hint: project.next_id_hint(),
        nodes,
        save_counter,
        saved_colors: convert::saved_colors_to_record(tree.saved_colors()),
        calculations: calculations::calculations_to_records(tree),
        correction_traces: calculations::corrections_to_records(tree),
        transposes: transposes::transposes_to_records(tree),
        window_geometries,
        column_widths,
    };

    // Encoding should never fail for our own transport types
    // (no non-string key, no I/O involved); we still keep the
    // error rather than panic, in case a future extension of the
    // schema introduced a case that fails to encode.
    let payload = rmp_serde::to_vec_named(&body).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(source),
    })?;

    let io_err = |source: std::io::Error| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = tempfile::NamedTempFile::new_in(parent_dir).map_err(io_err)?;
    file.write_all(MAGIC).map_err(io_err)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())
        .map_err(io_err)?;
    file.write_all(&payload).map_err(io_err)?;
    file.flush().map_err(io_err)?;
    file.as_file().sync_all().map_err(io_err)?;
    file.persist(path).map_err(|err| io_err(err.error))?;
    track_counter(path, project, save_counter);
    Ok(())
}

/// Loads a complete [`Project`] from `path`. The version is read and
/// checked before any decoding of the body: a file newer than this
/// reader is refused without its body being touched. A version `0` — a
/// number that never existed, the first real version being 1 — is
/// refused too, explicitly, rather than read as if
/// it were version 1.
///
/// Equivalent to [`load_with_geometries`] with its window geometry block
/// thrown away: it is the function used by callers that do not
/// know this notion (`stokesforge-app`, today).
pub fn load(path: &Path) -> PersistenceResult<Project> {
    load_with_geometries(path).map(|(project, _geometries)| project)
}

/// Like [`load`], but also returns the window geometry block read from
/// the file (`FileBody::window_geometries`), keyed by `NodeId`. An entry
/// whose key matches no node of the rebuilt project (corrupted
/// file, or orphan geometry left by a third-party tool) is
/// silently dropped rather than making the load fail —
/// invariant 10: a data defect degrades, it does not crash. It is the
/// function called by `stokesforge-session::actor` to get back the layout
/// of the windows when a project is reopened.
pub fn load_with_geometries(
    path: &Path,
) -> PersistenceResult<(Project, HashMap<NodeId, WindowGeometry>)> {
    load_with_report(path).map(|(project, view, _report)| (project, view.window_geometries))
}

/// Like [`load_with_geometries`], but also returns the [`LoadReport`] of what
/// the reading dropped (unreadable calculations, #278). It is the function called by
/// the session, so that the application reports it.
pub fn load_with_report(path: &Path) -> PersistenceResult<(Project, ViewTables, LoadReport)> {
    load_with_limit(path, DEFAULT_MAX_FILE_BYTES)
}

/// Like [`load_with_report`], with an explicit file size cap
/// (A2, #316): a file of more than `max_bytes` bytes is refused with
/// [`PersistenceError::TooLarge`] before any decoding. Exactly `max_bytes`
/// bytes passes. The `.stf` not being compressed, the decompressed size is the
/// size of the file. Reading is bounded (`take(max_bytes + 1)`): a file
/// that grows between the measurement and the reading does not get around the cap.
pub fn load_with_limit(
    path: &Path,
    max_bytes: u64,
) -> PersistenceResult<(Project, ViewTables, LoadReport)> {
    load_with_limits(
        path,
        LoadLimits {
            max_bytes,
            ..LoadLimits::default()
        },
    )
}

/// Like [`load_with_limit`], also with the global cell budget (B1,
/// #316): if the columns declare more than `max_cells` cells in total,
/// reading is refused with [`PersistenceError::TooManyCells`] **before**
/// any column allocation.
pub fn load_with_limits(
    path: &Path,
    limits: LoadLimits,
) -> PersistenceResult<(Project, ViewTables, LoadReport)> {
    use std::io::Read;
    let max_bytes = limits.max_bytes;
    let io = |source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let size = file.metadata().map_err(io)?.len();
    let too_large = |size| PersistenceError::TooLarge {
        path: path.to_path_buf(),
        size,
        limit: max_bytes,
    };
    if size > max_bytes {
        return Err(too_large(size));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(too_large(bytes.len() as u64));
    }

    let version = match version::read_header(&bytes) {
        version::Header::NotStokesForge => {
            return Err(PersistenceError::NotAStokesForgeFile {
                path: path.to_path_buf(),
            })
        }
        version::Header::Truncated => {
            return Err(PersistenceError::Corrupt {
                path: path.to_path_buf(),
                detail: "header truncated before the version number".to_string(),
            })
        }
        version::Header::ZeroVersion => {
            return Err(PersistenceError::Corrupt {
                path: path.to_path_buf(),
                detail: "version number 0 in the header: it never existed, \
                         the first real format version is 1"
                    .to_string(),
            })
        }
        version::Header::TooNew(found) => {
            return Err(PersistenceError::UnsupportedVersion {
                path: path.to_path_buf(),
                found,
                supported: FORMAT_VERSION,
            })
        }
        version::Header::Known(version) => version,
    };

    let mut body: FileBody = version::decode_body(version, version::payload(&bytes))
        .map_err(|source| classify_body_error(source, path))?;

    let save_counter = body.save_counter;
    let raw_geometries = std::mem::take(&mut body.window_geometries);
    let raw_widths = std::mem::take(&mut body.column_widths);
    for &id in raw_geometries.keys().chain(raw_widths.keys()) {
        check_node_id(id, path)?;
    }
    check_cell_budget(&body, limits.max_cells, path)?;
    let (project, report) = build_project_reported(body, path)?;
    track_counter(path, &project, save_counter);

    // A key without a matching node (corrupted file, or orphan geometry
    // left by a third-party tool that did not purge its own)
    // is dropped silently rather than making the load fail —
    // invariant 10. `stokesforge-session::actor` itself purges, at each
    // mutation, the orphan entries created during the life of the process;
    // this filter covers the case where the file already carried some even before
    // being opened.
    let tree = project.tree();
    let geometries = raw_geometries
        .into_iter()
        .filter_map(|(id, geometry)| {
            let node = NodeId::from_raw(id);
            tree.node(node).is_some().then_some((node, geometry.into()))
        })
        .collect();
    // Same filter for the column widths (#325), plus the value: a
    // non-finite real or ≤ 0 would make a nonsensical grid be painted.
    let column_widths = raw_widths
        .into_iter()
        .filter_map(|(id, width)| {
            let node = NodeId::from_raw(id);
            (tree.node(node).is_some() && is_valid_column_width(width)).then_some((node, width))
        })
        .collect();

    let view = ViewTables {
        window_geometries: geometries,
        column_widths,
    };
    Ok((project, view, report))
}

/// Classifies a body decoding error: an unknown **enum variant**
/// (C5, level 1) is not the same thing as a truly corrupted
/// file — `rmp_serde`/`serde` name this case in the error message
/// ("unknown variant …"), which is enough to tell it apart without having to
/// inspect the bytes ourselves.
fn classify_body_error(source: version::BodyError, path: &Path) -> PersistenceError {
    let source = match source {
        version::BodyError::Decode(source) => source,
        // `read_header` already refused a version without a decoder.
        version::BodyError::UnknownVersion(found) => {
            return PersistenceError::UnsupportedVersion {
                path: path.to_path_buf(),
                found,
                supported: FORMAT_VERSION,
            }
        }
    };
    let detail = source.to_string();
    if detail.contains("unknown variant") {
        PersistenceError::UnknownVariant {
            path: path.to_path_buf(),
            detail,
        }
    } else {
        PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail,
        }
    }
}

/// Rebuilds a `Project` from an already decoded body. Separate from `load`
/// so that the internal tests can craft a `FileBody` by hand
/// (belt case of the identifier counter, unknown fields, invalid
/// model) without going back through the MessagePack encoding.
fn build_project(body: FileBody, path: &Path) -> PersistenceResult<Project> {
    build_project_reported(body, path).map(|(project, _report)| project)
}

/// [`build_project`], with the report of what the rebuild dropped.
fn build_project_reported(body: FileBody, path: &Path) -> PersistenceResult<(Project, LoadReport)> {
    check_node_id(body.root, path)?;
    check_node_id(body.next_id_hint, path)?;
    let mut nodes = Vec::with_capacity(body.nodes.len());
    for record in body.nodes {
        let NodeRecord {
            id,
            name,
            parent,
            kind,
        } = record;
        check_node_id(id, path)?;
        if let Some(parent_id) = parent {
            check_node_id(parent_id, path)?;
        }
        // The columns that a formula reads are identifiers like the
        // others: bounded at read (invariant 7), otherwise a crafted file
        // would make the allocator start at the edge of `u64`.
        if let schema::KindRecord::Column {
            formula_parts: Some(parts),
            ..
        } = &kind
        {
            for part in parts {
                if let schema::FormulaPartRecord::Column(read) = part {
                    check_node_id(*read, path)?;
                }
            }
        }
        let kind = convert::record_to_kind(kind).map_err(|err| convert_error(err, path))?;
        let node = Node::with_parent(
            NodeId::from_raw(id),
            &name,
            parent.map(NodeId::from_raw),
            kind,
        )
        .map_err(|source| PersistenceError::InvalidModel {
            path: path.to_path_buf(),
            source,
        })?;
        nodes.push(node);
    }

    let (mut kept, discarded_calculations, digests) =
        calculations::records_to_calculations_with_digests(body.calculations, MAX_NODE_ID);
    let (traces, discarded_corrections) =
        calculations::records_to_corrections(body.correction_traces, MAX_NODE_ID);
    let tree = Tree::from_nodes(NodeId::from_raw(body.root), nodes)
        .map_err(|source| PersistenceError::InvalidModel {
            path: path.to_path_buf(),
            source,
        })?
        .with_saved_colors(convert::record_to_saved_colors(body.saved_colors))
        .with_correction_traces(traces);
    // A1 (#316): have the inputs of a calculation changed since
    // the save? Compared on the values of the file, without recalculation.
    calculations::mark_changed_inputs(&mut kept, &digests, &tree);
    let linked = transposes::records_to_transposes(body.transposes, &tree, MAX_NODE_ID);
    let tree = tree.with_calculations(kept).with_transposes(linked);
    Ok((
        Project::from_tree(Arc::new(tree), body.next_id_hint),
        LoadReport {
            discarded_calculations,
            discarded_corrections,
        },
    ))
}

/// Refuses a node identifier (or `next_id_hint`) beyond
/// [`MAX_NODE_ID`] (B1): well above any real project, well
/// below `u64::MAX` where `IdAllocator::next` would refuse anyway —
/// but a project in which a node already carries an identifier close to this
/// boundary would stay unusable from the first following creation.
/// Bounding at read closes this entry path rather than counting on the
/// mechanical `checked_add` of `stokesforge-core`, which protects the allocator but
/// not the file that made it start so close to the edge.
fn check_node_id(id: u64, path: &Path) -> PersistenceResult<()> {
    if id > MAX_NODE_ID {
        Err(PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "node identifier {id} exceeds the limit accepted when reading \
                 ({MAX_NODE_ID})"
            ),
        })
    } else {
        Ok(())
    }
}

/// Refuses a file whose columns declare more than
/// `max_cells` cells in total (B1, #316). Only reads the lengths of the already
/// decoded records: nothing is allocated for the model columns.
fn check_cell_budget(body: &FileBody, max_cells: u64, path: &Path) -> PersistenceResult<()> {
    let mut cells: u64 = 0;
    for record in &body.nodes {
        if let schema::KindRecord::Column { values, .. } = &record.kind {
            let len = match values {
                schema::ValuesRecord::Float(bytes) => bytes.len() / 8,
                schema::ValuesRecord::Mixed { tags, .. } => tags.len(),
            };
            cells = cells.saturating_add(len as u64);
            if cells > max_cells {
                return Err(PersistenceError::TooManyCells {
                    path: path.to_path_buf(),
                    cells,
                    limit: max_cells,
                });
            }
        }
    }
    Ok(())
}

/// Translates a low-level conversion error (`convert.rs`) into a public
/// error: in both cases, the file is inconsistent in a way
/// that a file written by this crate never produces.
fn convert_error(err: ConvertError, path: &Path) -> PersistenceError {
    match err {
        ConvertError::BadFloatLength(len) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("float column buffer of {len} bytes, not a multiple of 8"),
        },
        ConvertError::TooManyRows { found, limit } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("a column announces {found} rows, beyond the accepted limit ({limit})"),
        },
        ConvertError::BadMixedTag(tag) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "unknown mixed cell tag ({tag}): expected 0 (empty), \
                 1 (number) or 2 (text)"
            ),
        },
        ConvertError::BadMixedLength {
            expected,
            found,
            what,
        } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "inconsistent mixed column: the tags announce {expected} {what}, \
                 the array holds {found}"
            ),
        },
        ConvertError::Core(source) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("inconsistent column: {source}"),
        },
        ConvertError::ZeroHistogramBins => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "a series has histogram_bins of 0, which is not representable".to_string(),
        },
        ConvertError::HistogramBinsTooLarge(bins) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "a series has histogram_bins of {bins}, beyond the accepted limit \
                 ({})",
                convert::MAX_HISTOGRAM_BINS
            ),
        },
        ConvertError::PanelPositionOutOfBounds { row, column } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "panel at position (row={row}, column={column}), beyond the accepted \
                 limit ({})",
                convert::MAX_PANEL_POSITION
            ),
        },
    }
}

// -- B8: detection of a concurrent writer -----------------------------------
//
// In-memory registry, specific to this process, associating (canonical path,
// identity of a `Project`) with the last save counter observed for that
// pair. Never persists to disk — only the counter it describes
// goes through the disk (`FileBody::save_counter`), in line with the
// doctrine of stamps (§F2 of the correction guide).
//
// The key is not the path alone: two distinct `Project`s loaded from
// the same file (two windows, a script and the interface, …) must
// not be confused with each other — this is precisely the case B8 has to
// detect. A **weak** reference is kept on `Project::identity()` —
// NOT on the current tree (`project.tree()`): a first version of this
// registry keyed on the identity of the tree, relying on the fact that
// `Arc::make_mut` reuses the same allocation as long as nobody else
// has cloned it. This is false as soon as an `undo`/`redo` comes into play:
// `Project::replace_tree` reassigns to the instance an `Arc<Tree>` ALREADY SEEN at an
// earlier moment of THAT SAME instance (restored from the history
// stack) — the same address becomes "current" again while the
// save counter has advanced in the meantime, and the registry wrongly saw a
// stale counter there, refusing a perfectly legitimate save
// (found by `b14_the_disk_round_trip_holds_at_every_step`, a random sweep
// of operations including `Undo`/`Redo`). `Project::identity
// ()` is stable over the whole life of an instance, whatever tree
// it shows at a given moment, and it is exactly the identity that
// is needed here — see its documentation in `stokesforge-core`. `Weak::upgrade`
// fails cleanly for a `Project` abandoned long ago rather than
// risking a collision with a memory address reused by an unrelated
// value (as long as a `Weak` exists, the allocation it points to
// stays alive — two entries still "upgradable" at the same time can
// therefore never share an address).

struct TrackedSave {
    path: PathBuf,
    identity: Weak<()>,
    counter: u64,
}

fn registry() -> &'static Mutex<Vec<TrackedSave>> {
    static REGISTRY: OnceLock<Mutex<Vec<TrackedSave>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Canonical path if the file already exists; otherwise the path as is —
/// a destination that does not exist yet has no canonical form.
fn canonical_or_given(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn matches(entry: &TrackedSave, canon: &Path, project: &Project) -> bool {
    entry.path == canon
        && entry.identity.upgrade().is_some()
        && Weak::ptr_eq(&entry.identity, &project.identity())
}

/// Counter that THIS instance of `project` (in the sense of the identity of its
/// tree) saw the last time it was loaded or saved to
/// `path`, in this process. `None` if this pair (path, instance) is
/// unknown to the registry: either this `Project` never went through
/// `load`/`save` for this path, or its tree has since been replaced
/// (`replace_tree`, undo restoration) — in both cases, there is
/// nothing to compare, and `save` proceeds without a check rather than refusing
/// wrongly.
fn tracked_counter(path: &Path, project: &Project) -> Option<u64> {
    let canon = canonical_or_given(path);
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.retain(|entry| entry.identity.strong_count() > 0);
    guard
        .iter()
        .find(|entry| matches(entry, &canon, project))
        .map(|entry| entry.counter)
}

/// Records (or updates) the last known counter for this pair
/// (path, instance) — called after each successful `load`/`save`.
fn track_counter(path: &Path, project: &Project, counter: u64) {
    let canon = canonical_or_given(path);
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.retain(|entry| entry.identity.strong_count() > 0);
    if let Some(entry) = guard
        .iter_mut()
        .find(|entry| matches(entry, &canon, project))
    {
        entry.counter = counter;
    } else {
        guard.push(TrackedSave {
            path: canon,
            identity: project.identity(),
            counter,
        });
    }
}

/// Reads only the save counter of a file already written by this
/// format, without building a `Project`: it is the cheap check that
/// `save` makes before overwriting an existing target.
///
/// `Ok(None)` if the file does not exist yet (nothing to compare: the
/// save is the first) or if it does not carry the signature of this format
/// (historical behavior kept for a target that is not a `.stf` —
/// B8 only covers the collision between two writes of this format, not the
/// replacement of a foreign file). A file **that carries the signature**
/// but cannot be read — truncated header, version 0, undecodable
/// body — is refused (`UnreadableTarget`), like that of a newer
/// version (`UnsupportedVersion`): better not to overwrite blindly
/// a file that cannot be read.
/// `validate`: in addition to the decoding, rebuilds the project as `load` would
/// do, to refuse (`UnreadableTarget`) a body that decodes but that
/// loading would reject. `save` only asks for it for a target that this
/// process never loaded or wrote (otherwise the file is ours).
fn peek_save_counter(path: &Path, validate: bool) -> PersistenceResult<Option<u64>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(PersistenceError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    // From here on the target says it is StokesForge: if it cannot be read, we
    // refuse to overwrite it (same diagnostics as `load`, in the same order).
    let version = match version::read_header(&bytes) {
        version::Header::NotStokesForge => return Ok(None),
        version::Header::Truncated => {
            return Err(PersistenceError::UnreadableTarget {
                path: path.to_path_buf(),
                detail: "header truncated before the version number".to_string(),
            })
        }
        version::Header::ZeroVersion => {
            return Err(PersistenceError::UnreadableTarget {
                path: path.to_path_buf(),
                detail: "version number 0 in the header".to_string(),
            })
        }
        version::Header::TooNew(found) => {
            return Err(PersistenceError::UnsupportedVersion {
                path: path.to_path_buf(),
                found,
                supported: FORMAT_VERSION,
            })
        }
        version::Header::Known(version) => version,
    };
    match version::decode_body(version, version::payload(&bytes)) {
        Ok(mut body) => {
            let save_counter = body.save_counter;
            if validate {
                // The body decodes, but would `load` refuse it (inconsistent
                // model, identifier out of bounds)? Then replacing it
                // would destroy a file that only a more tolerant tool can
                // still read.
                let unreadable = |source: PersistenceError| PersistenceError::UnreadableTarget {
                    path: path.to_path_buf(),
                    detail: source.to_string(),
                };
                let geometry_ids = std::mem::take(&mut body.window_geometries);
                let width_ids = std::mem::take(&mut body.column_widths);
                for &id in geometry_ids.keys().chain(width_ids.keys()) {
                    check_node_id(id, path).map_err(unreadable)?;
                }
                build_project_reported(body, path).map_err(unreadable)?;
            }
            Ok(Some(save_counter))
        }
        // The body does not decode: `save` does not diagnose (that is the
        // role of `load`), but it does not destroy either a file it
        // cannot read — a truncated `.stf` would be unrecoverable.
        Err(source) => Err(PersistenceError::UnreadableTarget {
            path: path.to_path_buf(),
            detail: source.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{KindRecord, TypeRecord};
    use serde::Serialize;
    use stokesforge_core::{Column, ColumnType, CoreError, NodeKind};

    fn folder_record(id: u64, name: &str, children: Vec<u64>) -> NodeRecord {
        NodeRecord {
            id,
            name: name.to_string(),
            parent: if id == 1 { None } else { Some(1) },
            kind: KindRecord::Folder { children },
        }
    }

    /// Criterion 2 (belt case): a file whose `next_id_hint` is
    /// lower than the largest id it contains must still make the
    /// counter restart above the largest id read.
    #[test]
    fn resume_uses_the_highest_id_seen_even_if_the_hint_is_stale() {
        let root = folder_record(1, "Root", vec![5]);
        let child = folder_record(5, "Enfant", vec![]);
        let body = FileBody {
            root: 1,
            next_id_hint: 2, // behind: the largest id present is 5
            nodes: vec![root, child],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let project = build_project(body, Path::new("test.plyx")).expect("valid model");
        assert_eq!(project.next_id_hint(), 6);
    }

    /// A body that decodes but that `load` would refuse (orphan node) is not
    /// overwritten by `save`: the target is unreadable for us. A file that
    /// this process has just loaded is re-saved, itself, without this
    /// costly check.
    #[test]
    fn save_refuses_a_target_that_decodes_but_does_not_load() {
        let root = folder_record(1, "Root", vec![]);
        let orphan = NodeRecord {
            id: 2,
            name: "Perdu".to_string(),
            parent: Some(99),
            kind: KindRecord::Folder { children: vec![] },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, orphan],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("encoding"));
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("incoherent.stf");
        std::fs::write(&path, &bytes).expect("write the test file");

        let project = Project::new("New").expect("project");
        let err = save(&project, &path).unwrap_err();
        assert!(
            matches!(err, PersistenceError::UnreadableTarget { .. }),
            "{err:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "file intact");

        // A sound file, loaded then re-saved: no blocking.
        let good = dir.path().join("sound.stf");
        save(&project, &good).expect("first write");
        let loaded = load(&good).expect("reload");
        save(&loaded, &good).expect("re-save of the loaded file");
    }

    /// Criterion 8: a well-formed but structurally inconsistent body must
    /// produce `InvalidModel`, not panic nor be silently accepted.
    #[test]
    fn incoherent_body_yields_invalid_model() {
        // Node whose parent is absent from the list.
        let root = folder_record(1, "Root", vec![]);
        let orphan = NodeRecord {
            id: 2,
            name: "Perdu".to_string(),
            parent: Some(99),
            kind: KindRecord::Folder { children: vec![] },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, orphan],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );

        // Two nodes sharing the same id.
        let a = folder_record(1, "A", vec![]);
        let b = folder_record(1, "B", vec![]);
        let body = FileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![a, b],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );

        // The root is not a folder.
        let root = NodeRecord {
            id: 1,
            name: "Root".to_string(),
            parent: None,
            kind: KindRecord::Note {
                hidden: false,
                label: String::new(),
                comment: String::new(),
                text: String::new(),
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![root],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );
    }

    /// A column whose float buffer length is not a multiple of
    /// 8 must be reported `Corrupt`, distinctly from an invalid model.
    #[test]
    fn bad_float_buffer_length_yields_corrupt_not_invalid_model() {
        let root = NodeRecord {
            id: 1,
            name: "Root".to_string(),
            parent: None,
            kind: KindRecord::Folder { children: vec![2] },
        };
        let column = NodeRecord {
            id: 2,
            name: "A".to_string(),
            parent: Some(1),
            kind: KindRecord::Column {
                column_type: TypeRecord::Numeric,
                role: schema::RoleRecord::Y,
                unit: String::new(),
                long_name: String::new(),
                comment: String::new(),
                display_format: String::new(),
                values: schema::ValuesRecord::Float(vec![0, 1, 2]), // 3 bytes, not a multiple of 8
                version: 0,
                formula_parts: None,
                file_metadata: Vec::new(),
                import_source: None,
                x_prime: None,
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, column],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: Default::default(),
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(matches!(err, PersistenceError::Corrupt { .. }), "{err:?}");
    }

    /// Local extended copies of the v1 schema, with an additional field at
    /// each level — simulates a file written by a future version that
    /// would have added data. The names of fields/variants must
    /// coincide with `schema.rs`; only the presence of an extra field
    /// changes.
    #[derive(Serialize)]
    struct ExtendedFileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<ExtendedNodeRecord>,
        extra: String,
    }

    #[derive(Serialize)]
    struct ExtendedNodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: ExtendedKindRecord,
        extra: String,
    }

    #[derive(Serialize)]
    enum ExtendedKindRecord {
        Folder {
            children: Vec<u64>,
        },
        Workbook {
            sheets: Vec<u64>,
            label: String,
            comment: String,
        },
        Worksheet {
            columns: Vec<u64>,
        },
        Column {
            column_type: TypeRecord,
            role: schema::RoleRecord,
            unit: String,
            long_name: String,
            comment: String,
            display_format: String,
            values: schema::ValuesRecord,
            plot_hint: u32,
        },
    }

    /// Criterion 6: unknown fields (added by a future version) are
    /// ignored at read rather than making the load fail.
    /// Minimal chain conforming to the containment rules of the core: root
    /// folder → workbook → sheet → column.
    #[test]
    fn unknown_fields_are_ignored_on_load() {
        let extended = ExtendedFileBody {
            root: 1,
            next_id_hint: 5,
            nodes: vec![
                ExtendedNodeRecord {
                    id: 1,
                    name: "Root".to_string(),
                    parent: None,
                    kind: ExtendedKindRecord::Folder { children: vec![2] },
                    extra: "ignored".to_string(),
                },
                ExtendedNodeRecord {
                    id: 2,
                    name: "Book1".to_string(),
                    parent: Some(1),
                    kind: ExtendedKindRecord::Workbook {
                        sheets: vec![3],
                        label: String::new(),
                        comment: String::new(),
                    },
                    extra: "ignored".to_string(),
                },
                ExtendedNodeRecord {
                    id: 3,
                    name: "Sheet1".to_string(),
                    parent: Some(2),
                    kind: ExtendedKindRecord::Worksheet { columns: vec![4] },
                    extra: "ignored".to_string(),
                },
                ExtendedNodeRecord {
                    id: 4,
                    name: "A".to_string(),
                    parent: Some(3),
                    kind: ExtendedKindRecord::Column {
                        column_type: TypeRecord::Numeric,
                        role: schema::RoleRecord::X,
                        unit: "ms".to_string(),
                        long_name: String::new(),
                        comment: String::new(),
                        display_format: String::new(),
                        values: schema::ValuesRecord::Float(1.5f64.to_le_bytes().to_vec()),
                        plot_hint: 42,
                    },
                    extra: "ignored too".to_string(),
                },
            ],
            extra: "future version".to_string(),
        };

        let payload = rmp_serde::to_vec_named(&extended).expect("valid encoding");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("extended.plyx");
        std::fs::write(&path, &bytes).expect("write the test file");

        let project = load(&path).expect("the load must ignore the unknown fields");
        // The persisted `next_id_hint` (5) is not contradicted by the largest
        // id read (4): the counter restarts directly at 5.
        assert_eq!(project.next_id_hint(), 5);
        let node = project
            .tree()
            .node(NodeId::from_raw(4))
            .expect("column present");
        let column = node.as_column().expect("column node");
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.meta().role, stokesforge_core::ColumnRole::X);
        assert_eq!(column.meta().unit, "ms");
        assert_eq!(column.len(), 1);
    }

    /// Local copy of the shape of the schema *before* phase R3: a `Graph`
    /// without a `spec` field — exactly what a file written by an earlier
    /// version of the software contains. Used to prove the exit criterion
    /// of task 3.2 without depending on an archived `.plyx` file: `spec`
    /// is `#[serde(default)]`, so a missing field must not make the load
    /// fail, and the rebuilt graph must carry
    /// `PlotSpec::default()`.
    #[derive(Serialize)]
    struct PreR3FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<PreR3NodeRecord>,
    }

    #[derive(Serialize)]
    struct PreR3NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: PreR3KindRecord,
    }

    #[derive(Serialize)]
    enum PreR3KindRecord {
        Folder { children: Vec<u64> },
        Graph { label: String, comment: String },
    }

    /// Exit criterion of task 3.2: a file written before phase R3
    /// (hence without the `spec` field) reads back, and the graph it contains
    /// carries a default `PlotSpec` rather than making the
    /// load fail.
    #[test]
    fn a_graph_written_before_r3_loads_with_a_default_spec() {
        let pre_r3 = PreR3FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                PreR3NodeRecord {
                    id: 1,
                    name: "Root".to_string(),
                    parent: None,
                    kind: PreR3KindRecord::Folder { children: vec![2] },
                },
                PreR3NodeRecord {
                    id: 2,
                    name: "Graph1".to_string(),
                    parent: Some(1),
                    kind: PreR3KindRecord::Graph {
                        label: String::new(),
                        comment: String::new(),
                    },
                },
            ],
        };

        let payload = rmp_serde::to_vec_named(&pre_r3).expect("valid encoding");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("pre_r3.plyx");
        std::fs::write(&path, &bytes).expect("write the test file");

        let project = load(&path).expect("a file without a spec field must still read back");
        let node = project
            .tree()
            .node(NodeId::from_raw(2))
            .expect("graph present");
        match node.kind() {
            NodeKind::Graph { spec, .. } => {
                assert_eq!(*spec, stokesforge_plot::PlotSpec::default());
            }
            other => panic!("expected Graph, got {other:?}"),
        }
    }

    /// `record_to_kind`/`kind_to_record` stay consistent for an empty
    /// column (length 0, neither corrupted nor invalid).
    #[test]
    fn empty_column_round_trips_through_records() {
        let column = Column::new(ColumnType::Numeric);
        let record = convert::kind_to_record(&NodeKind::Column(column.clone()));
        let rebuilt = convert::record_to_kind(record).expect("valid column");
        match rebuilt {
            NodeKind::Column(rebuilt_column) => assert_eq!(rebuilt_column, column),
            other => panic!("expected Column, got {other:?}"),
        }
    }

    /// Sanity check independent of `CoreError`: `Column::from_block` should
    /// never be reached with an inconsistent (type, block) pair
    /// from `record_to_kind`, but we check that the error, if it
    /// occurred, would stay a `CoreError` passed on as is.
    #[test]
    fn core_error_variant_exists_for_completeness() {
        let err = ConvertError::Core(CoreError::TypeMismatch("test".to_string()));
        assert!(matches!(
            err,
            ConvertError::Core(CoreError::TypeMismatch(_))
        ));
    }

    /// Window geometry (task "window geometry"): an entry
    /// set via `save_with_geometries` reads back identical via
    /// `load_with_geometries`, associated with the right `NodeId`.
    #[test]
    fn geometries_round_trip_through_save_and_load() {
        let mut project = stokesforge_core::Project::new("Demo").expect("valid project");
        let root = project.root();
        let book = project
            .add_empty_workbook(root, Some("Book1"))
            .expect("workbook created");

        let mut geometries = HashMap::new();
        geometries.insert(
            book,
            crate::WindowGeometry {
                x: 12.0,
                y: 34.0,
                width: 640.0,
                height: 480.0,
                view: Some(crate::WindowView {
                    mode: crate::WindowMode::Maximized,
                    collapsed: true,
                    stack: 3,
                }),
            },
        );

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("geometries.plyx");
        let view = ViewTables {
            window_geometries: geometries.clone(),
            column_widths: HashMap::new(),
        };
        save_with_geometries(&project, &view, &path).expect("save succeeded");

        let (loaded, loaded_geometries) = load_with_geometries(&path).expect("load succeeded");
        assert_eq!(loaded.tree().len(), project.tree().len());
        assert_eq!(loaded_geometries, geometries);
    }

    /// Backward compatibility: a file written before the addition of the
    /// geometry block (`FileBody` without `window_geometries`) reloads without
    /// error, with an empty geometry table — same pattern as
    /// `unknown_fields_are_ignored_on_load`/`a_graph_written_before_r3_loads_with_a_default_spec`
    /// above, which already simulate an older file for other
    /// `#[serde(default)]` fields.
    #[derive(Serialize)]
    struct PreGeometryFileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        save_counter: u64,
    }

    #[test]
    fn a_file_without_the_geometry_block_loads_with_an_empty_geometry_table() {
        let root = folder_record(1, "Root", vec![]);
        let pre_geometry = PreGeometryFileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![root],
            save_counter: 3,
        };

        let payload = rmp_serde::to_vec_named(&pre_geometry).expect("valid encoding");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("pre_geometry.plyx");
        std::fs::write(&path, &bytes).expect("write the test file");

        let (project, geometries) =
            load_with_geometries(&path).expect("a file without a geometry block must read back");
        assert!(geometries.is_empty());
        assert_eq!(project.next_id_hint(), 2);
    }
    /// Writes a body by hand (private schema) to a test file.
    fn write_body(dir: &tempfile::TempDir, name: &str, body: &FileBody) -> std::path::PathBuf {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&rmp_serde::to_vec_named(body).expect("encoding"));
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes).expect("write the test file");
        path
    }

    /// A minimal body (root 1, sheet 2) carrying these raw widths.
    fn body_with_widths(widths: Vec<(u64, f32)>) -> FileBody {
        FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                folder_record(1, "Root", vec![2]),
                folder_record(2, "Folder", vec![]),
            ],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            transposes: Default::default(),
            window_geometries: Default::default(),
            column_widths: widths.into_iter().collect(),
        }
    }

    /// #325: the widths set in `ViewTables` read back identical,
    /// associated with the right node. Witness: do not write the field.
    #[test]
    fn column_widths_round_trip_through_save_and_load() {
        let mut project = stokesforge_core::Project::new("Demo").expect("valid project");
        let root = project.root();
        let book = project
            .add_empty_workbook(root, Some("Book1"))
            .expect("workbook created");
        let view = ViewTables {
            window_geometries: HashMap::new(),
            column_widths: HashMap::from([(book, 77.5), (root, 12.0)]),
        };
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("widths.stf");
        save_with_geometries(&project, &view, &path).expect("save succeeded");
        let (_, loaded, _) = load_with_report(&path).expect("load succeeded");
        assert_eq!(loaded.column_widths, view.column_widths);
        // `load_with_geometries` keeps its signature and only shows the geometries.
        let (_, geometries) = load_with_geometries(&path).expect("load succeeded");
        assert!(geometries.is_empty());
    }

    /// #325: a key that is no longer a node is not written (the actor does not
    /// purge its table on mutation, it is the save that filters), nor
    /// a non-finite or zero value. Witness: remove the filter.
    #[test]
    fn save_drops_dead_keys_and_invalid_widths() {
        let project = stokesforge_core::Project::new("Demo").expect("valid project");
        let root = project.root();
        let dead = NodeId::from_raw(999);
        let view = ViewTables {
            window_geometries: HashMap::new(),
            column_widths: HashMap::from([
                (root, 40.0),
                (dead, 50.0),
                (NodeId::from_raw(1000), f32::NAN),
            ]),
        };
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("filter.stf");
        save_with_geometries(&project, &view, &path).expect("save succeeded");
        let (_, loaded, _) = load_with_report(&path).expect("load succeeded");
        assert_eq!(loaded.column_widths, HashMap::from([(root, 40.0)]));
        // The file itself does not carry the dead key: reading of the raw body.
        let bytes = std::fs::read(&path).unwrap();
        let body: FileBody = rmp_serde::from_slice(&bytes[MAGIC.len() + 4..]).unwrap();
        assert_eq!(body.column_widths.len(), 1);
        assert!(!body.column_widths.contains_key(&dead.raw()));
    }

    /// #325: a file written before the block reads back, empty table.
    #[test]
    fn a_file_without_the_widths_block_loads_with_an_empty_table() {
        #[derive(Serialize)]
        struct PreWidthsFileBody {
            root: u64,
            next_id_hint: u64,
            nodes: Vec<NodeRecord>,
            save_counter: u64,
        }
        let pre = PreWidthsFileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![folder_record(1, "Root", vec![])],
            save_counter: 0,
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&rmp_serde::to_vec_named(&pre).expect("encoding"));
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("without_widths.stf");
        std::fs::write(&path, &bytes).unwrap();
        let (_, view, _) = load_with_report(&path).expect("a file without the block reads back");
        assert!(view.column_widths.is_empty());
    }

    /// #325, invariant 10: an orphan key, NaN, ∞, 0 or negative are
    /// dropped one by one, without failure; the sound neighbouring value stays.
    #[test]
    fn hostile_column_widths_are_discarded_one_by_one() {
        let body = body_with_widths(vec![
            (2, 61.0),
            (77, 30.0), // orpheline
        ]);
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = write_body(&dir, "orphan.stf", &body);
        let (_, view, _) = load_with_report(&path).expect("load succeeded");
        assert_eq!(
            view.column_widths,
            HashMap::from([(NodeId::from_raw(2), 61.0)])
        );
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -3.0] {
            let body = body_with_widths(vec![(2, bad), (1, 20.0)]);
            let path = write_body(&dir, "hostile.stf", &body);
            let (_, view, _) = load_with_report(&path).expect("never a failure");
            assert_eq!(
                view.column_widths,
                HashMap::from([(NodeId::from_raw(1), 20.0)]),
                "width {bad:?} kept"
            );
        }
    }

    /// #325: an identifier beyond `MAX_NODE_ID` in the block of the
    /// widths is refused, as for the geometries.
    #[test]
    fn a_column_width_key_above_the_id_ceiling_is_refused() {
        let body = body_with_widths(vec![(MAX_NODE_ID + 1, 50.0)]);
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = write_body(&dir, "id_too_large.stf", &body);
        assert!(load_with_report(&path).is_err());
    }
}
