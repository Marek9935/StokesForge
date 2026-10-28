//! Import of scientific instrument data files (CSV/TXT/ASC) into a
//! spreadsheet, in the manner of OriginLab's ASCII import wizard: the user
//! picks a **recipe** ([`ImportRecipe`]) that describes the format of a given
//! device (separator, decimal separator, header lines to skip, series
//! name...), applies it to a file through [`parse`], and can save it for
//! reuse ([`RecipeStore`]).
//!
//! Deliberately pure crate: no UI dependency, no dependency on
//! `stokesforge-commands`/`stokesforge-session`, and no dependency on
//! `stokesforge-core`. The parser's output vocabulary (`f64`, `NaN` for a
//! missing value) is the simplest there is, to stay independent.
//! `stokesforge-app` does the conversion to `stokesforge_core::Column` on its
//! side.
//!
//! [`preview`] classifies each line of a file by role (main header,
//! sub-header and its optional role, data, footer), like OriginLab's ASCII
//! import wizard, so that the import window shows live how a file will be
//! split while the user adjusts the recipe. See its top-level doc for the
//! cost budget on a large file.
//!
//! [`decode`] turns bytes into text before any splitting: UTF-8, UTF-16 with a
//! byte order mark, and as a fallback Windows-1252, the code page of device
//! software on Windows.

pub mod builtin;
pub mod choose;
pub mod config_dir;
mod data_end;
mod datetime;

#[doc(hidden)]
pub use datetime::parse_datetime as parse_datetime_for_agreement;
pub mod decode;
mod layout;
pub mod metadata;
mod numbers;
pub mod parse;
pub mod preview;
pub mod recent;
pub mod recipe;
pub mod store;

pub use builtin::builtin_recipes;
pub use choose::choose_recipe;
pub use decode::*;
pub use metadata::MetadataLine;
pub use parse::*;
pub use preview::*;
pub use recent::{RecentFiles, MAX_RECENT_FILES};
pub use recipe::*;
pub use store::*;
