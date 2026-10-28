//! Minimal UI shell of StokesForge: the test bench that runs the complete
//! loop — model → command → undo → save → reload — **once**, in a real
//! window.
//!
//! # What this crate is not
//!
//! It is **not** a sketch of the final interface. The editable grid with
//! delegates, the properties panel and a complete graph system will be taken
//! up entirely later; nothing here must preempt them. The bench is
//! deliberately poor: its role is to prove that the end-to-end integration
//! holds, while a failed integration costs only a rewrite of this crate — and
//! not that of everything that would have been built on top. Two technical
//! foundations are laid here: the layout is **confined floating windows**
//! (`egui::Window`, module [`app`]), and Save/Load go through a native file
//! picker (`rfd`) — see `docs/application/README.md` for the detail of what is
//! done and what is not yet.
//!
//! # Breakdown
//!
//! - `stokesforge-app-state` (neighboring crate, #263): all the logic,
//!   **without egui**, driven by an enum of actions — hence testable in
//!   continuous integration, without a graphical server
//!   (`stokesforge-app-state/tests/shell.rs`);
//! - [`app`]: the `eframe` glue, which translates keyboard and mouse into
//!   actions and paints the state. Nothing it contains is a product decision;
//! - [`theme`]: the house visual palette, set once on the `Context` at startup
//!   (`main.rs`) — the app does not follow the system light/dark theme.
//!
//! # What Qt gave and has to be written here
//!
//! `QUndoStack` + `QAction::Undo` provided almost for free the keyboard
//! shortcut, the menu entry, its graying and its label ("Undo *Rename*").
//! Here, each of these four points is explicit code: the shortcut in
//! [`app`], the graying by `add_enabled`, the label by `AppState::undo_label`
//! (from `stokesforge-app-state`).

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod app;
pub mod color_picker;
pub mod field_state;
pub mod icons;
pub mod list_frame;
pub mod number_field;
pub mod theme;
pub mod toolbar;
pub mod tree_lines;

/// The source reader shared by all the guards that read the code (#116): those
/// of `icons` and `theme` here, those of `tests/` through the same file. Since
/// #263 it also reads `stokesforge-app-state/src/`.
#[cfg(test)]
#[path = "../tests/support/sources.rs"]
mod sources;
