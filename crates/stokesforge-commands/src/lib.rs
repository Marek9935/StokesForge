//! Commandes du modèle et pile d'historique de StokesForge.
//!
//! # Rôle
//!
//! Ce crate est la **seule** couche qui mute un `stokesforge_core::Project` en
//! dehors des tests — c'est la règle d'or du socle : *aucune mutation hors
//! d'une commande*. Il sera consommé par l'acteur de `stokesforge-session`, qui
//! détiendra l'unique `&mut Project` du processus ; une [`Command`] est une
//! valeur, sans état ni emprunt, faite pour traverser un canal inter-threads
//! telle quelle.
//!
//! # Les deux natures d'entrée
//!
//! L'historique est **hybride**, parce que structure et données n'ont pas le
//! même coût :
//!
//! - une modification de **structure** (arbre, noms, ordre, ajout ou
//!   suppression de nœuds) est retenue comme un couple d'instantanés
//!   `Arc<Tree>` : l'arbre est persistant, les deux versions partagent tout ce
//!   qui n'a pas bougé, et garder les deux coûte un pointeur. L'arbre d'avant
//!   retient les nœuds supprimés — rien à libérer à la main, et un nœud
//!   restauré revient sous son **identifiant d'origine** ;
//! - une modification de **données** (contenu des colonnes) est retenue comme
//!   un **delta** : anciennes et nouvelles valeurs des seules cellules
//!   écrites, rejouées dans l'arbre courant. Un instantané retiendrait ici le
//!   buffer entier de la colonne — 8 Mo pour un million de points — à chaque
//!   frappe.
//!
//! De cette asymétrie découle un invariant non négociable : **LIFO strict**.
//! Restaurer un arbre entier écrase tout ce qui a été écrit après lui, donc
//! une entrée structurelle ne s'annule qu'une fois toutes les entrées
//! postérieures annulées. La pile le garantit par construction, et une macro
//! annule ses entrées strictement en ordre inverse d'insertion, quelles que
//! soient leurs natures.
//!
//! # Dépendance à sens unique
//!
//! Ce crate connaît `stokesforge-core`, jamais l'inverse — et n'importe aucune
//! dépendance UI (`tests/no_ui_dependency.rs`).
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
