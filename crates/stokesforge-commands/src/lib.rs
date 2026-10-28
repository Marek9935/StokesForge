//! Model commands and the history stack of StokesForge.
//!
//! # Role
//!
//! This crate is the **only** layer that mutates a `stokesforge_core::Project`
//! outside tests. It is the golden rule of the foundation: *no mutation outside
//! a command*. The `stokesforge-session` actor will consume it and hold the
//! single `&mut Project` of the process. A [`Command`] is a value, with no
//! state and no borrow, made to cross a channel between threads as it is.
//!
//! # The two kinds of entry
//!
//! The history is **hybrid**, because structure and data do not cost the same:
//!
//! - a **structure** change (tree, names, order, adding or removing nodes) is
//!   kept as a pair of `Arc<Tree>` snapshots. The tree is persistent, the two
//!   versions share everything that did not move, and keeping both costs one
//!   pointer. The earlier tree keeps the removed nodes: nothing to free by
//!   hand, and a restored node comes back under its **original identifier**;
//! - a **data** change (column contents) is kept as a **delta**: old and new
//!   values of only the cells written, replayed into the current tree. A
//!   snapshot would keep the whole column buffer here (8 MB for a million
//!   points) at every keystroke.
//!
//! This asymmetry gives a non-negotiable invariant: **strict LIFO**. Restoring
//! a whole tree overwrites everything written after it, so a structural entry
//! is undone only once all later entries are undone. The stack guarantees this
//! by construction, and a macro undoes its entries strictly in reverse order of
//! insertion, whatever their kinds.
//!
//! # One-way dependency
//!
//! This crate knows `stokesforge-core`, never the reverse, and imports no UI
//! dependency (`tests/no_ui_dependency.rs`).
//!
//! # Exemple
//!
//! ```
//! use stokesforge_commands::{Command, History};
//! use stokesforge_core::{Column, Project, ValueBlock};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut project = Project::new("Demo")?;
//! let root = project.root();
//! project.add_workbook(root, Some("Book1"))?;
//! let sheet = project.resolve_path("[Book1]Sheet1")?;
//! let a = project.add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))?;
//!
//! let mut history = History::new();
//! history.execute(&mut project, Command::SetCells {
//!     column: a,
//!     start: 0,
//!     values: ValueBlock::Float(vec![42.0]),
//!     expected_version: None,
//! })?;
//! assert_eq!(
//!     history.undo_label().map(ToString::to_string).as_deref(),
//!     Some("Edit Cells in Column A")
//! );
//! assert!(history.undo(&mut project)?);
//! # Ok(())
//! # }
//! ```

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod command;
pub mod error;
pub mod history;
pub mod label;

pub use command::Command;
pub use error::{CommandError, CommandResult};
pub use history::{DataDelta, Entry, History, DEFAULT_BYTE_LIMIT, DEFAULT_LIMIT, MAX_MACRO_DEPTH};
pub use label::{ColumnRef, HistoryLabel, MacroKind, PlotEditKind};
