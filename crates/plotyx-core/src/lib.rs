//! Modèle de données de PlotYx : l'arbre de projet (`Folder → Workbook →
//! Worksheet → Column`, `Graph`/`Matrix`/`Note` en feuilles) et rien d'autre
//! — voir `docs/rustrewrite/roadmap.md` phase 1.
//!
//! Règle non négociable, vérifiée par `tests/no_ui_dependency.rs` : ce crate
//! n'importe aucune dépendance UI.
//!
//! Le modèle est désormais construit. Il repose sur trois décisions :
//!
//! - **identité** — `NodeId` est un compteur monotone, jamais réutilisé sur
//!   toute la vie d'un projet (rechargement compris) ; l'allocateur vit dans
//!   `Project`, hors de l'arbre, pour qu'une annulation ne le fasse pas
//!   reculer ;
//! - **arbre persistant** — `Arc<Tree>` sur une carte plate
//!   `NodeId → Arc<Node>` : muter clone la carte et les seuls nœuds touchés,
//!   si bien qu'un ancien `Arc<Tree>` reste un instantané figé dont les nœuds
//!   intacts sont physiquement partagés avec l'arbre courant ;
//! - **une seule façade de mutation** — toute mutation passe par `Project`,
//!   dont l'unique `&mut` sera détenu par l'acteur de `plotyx-session` ;
//!   `Tree` n'expose publiquement que de la lecture.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod column;
pub mod error;
pub mod id;
pub mod name;
pub mod node;
pub mod path;
pub mod project;
pub mod tree;

pub use column::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, ValueBlock, ValuesDelta,
};
pub use error::{CoreError, CoreResult};
pub use id::{IdAllocator, NodeId};
pub use name::{check_name, RESERVED_NAME_CHARS};
pub use node::{Node, NodeKind, WindowMeta};
pub use path::{format_path, parse_path, ObjectPath, PathDepth};
pub use project::Project;
pub use tree::Tree;
