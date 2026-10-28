//! The StokesForge application state, **without a graphical interface** (#263,
//! work package #254).
//!
//! This crate receives everything `stokesforge-app` decides without drawing
//! it: [`state::AppState`] (the user's actions, their effect on the session,
//! the status messages), the selection, the preferences, the analysis forms,
//! the interface texts (`tr!`, [`i18n`]) and the errors put into sentences
//! ([`errors`]). It depends on **no** interface crate: not `egui`, not
//! `eframe`, not `winit`, not `wgpu`, nor `stokesforge-clipboard`, which pulls
//! in winit. The compiler holds the boundary that a text guard kept before
//! #263 (#255). The state tests (`tests/shell.rs`...) therefore compile
//! without a window.
//!
//! # What stays in `stokesforge-app`
//!
//! The `eframe` shell (`app`, `main.rs`), the theme, the icons and the field
//! building blocks that name an egui type. It reads this state and paints it;
//! it adds no product decision to it. The offer of the figure to the system
//! clipboard (`stokesforge-clipboard`) also stays there: the state keeps only
//! neutral bytes ([`state::FigureBytes`]).
//!
//! # A readable boundary
//!
//! The modules are public one by one, with no giant `pub use`: the path of an
//! import (`stokesforge_app_state::selection::...`) says where the item comes
//! from. Only [`tr!`] sits at the root (`#[macro_export]`).

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod analysis;
pub mod analysis_recent;
pub mod column_move;
pub mod errors;
pub mod file_picker;
pub mod history_text;
pub mod i18n;
pub mod object_drag;
pub mod object_format;
pub mod object_manager;
pub mod preferences;
pub mod quantities;
pub mod r_squared;
pub mod recovery;
pub mod selection;
pub mod startup;
pub mod state;
pub mod text_format;
pub mod user_colors;
