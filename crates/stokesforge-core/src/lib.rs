//! Modèle de données de StokesForge : l'arbre de projet (`Folder → Workbook →
//! Worksheet → Column`, `Graph`/`Matrix`/`Note` en feuilles) et rien d'autre
//! — voir `docs/archive/rustrewrite/roadmap.md` phase 1.
//!
//! Règle non négociable, vérifiée par `tests/no_ui_dependency.rs` : ce crate
//! n'importe aucune dépendance UI.
//!
//! # Depuis la phase R3 : une dépendance à `stokesforge-plot`, et une seule raison
//!
//! `NodeKind::Graph` porte désormais un `stokesforge_plot::PlotSpec`
//! (`docs/archive/newarchitecture/roadmap-rendu.md` phase R3) : un graphe du projet
//! porte son contenu de tracé, comme une colonne porte les siennes. Ce n'est
//! **pas** une porte ouverte au rendu — `stokesforge-plot` lui-même reste sans
//! dépendance de dessin (`stokesforge-plot/tests/no_ui_dependency.rs`), et sa
//! seule dépendance est désormais `stokesforge-id` (voir plus bas), pas ce crate.
//! [`resolve`] est le seul module qui traduit un spec en données prêtes à
//! tracer (`NodeId` → colonne) ; tout le reste du crate continue d'ignorer
//! jusqu'à l'existence de `PlotSpec`.
//!
//! **Pourquoi `stokesforge-id` et pas `stokesforge-core` dans `Cargo.toml` de
//! `stokesforge-plot`.** Avant R3, `stokesforge-plot` dépendait de ce crate en entier
//! pour son seul usage de `NodeId`. Une fois ce crate lui-même dépendant de
//! `stokesforge-plot` (pour `PlotSpec`), garder les deux arêtes aurait formé un
//! cycle Cargo direct — vérifié, pas supposé : `cargo check` le refuse avec
//! *cyclic package dependency*, Cargo n'autorisant pas de dépendances
//! normales circulaires (seules les dépendances de développement le
//! peuvent, ce qui ne s'applique pas ici puisque `PlotSpec` est un champ de
//! production, pas un besoin de test). `NodeId` a donc été descendu dans
//! `stokesforge-id`, crate minimal sans dépendance, réexporté ici à l'identique
//! (`pub use id::NodeId`, ci-dessous) : aucun appelant de
//! `stokesforge_core::NodeId` n'a rien à changer. Détail complet dans
//! `crates/stokesforge-id/src/lib.rs`.
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
//!   dont l'unique `&mut` sera détenu par l'acteur de `stokesforge-session` ;
//!   `Tree` n'expose publiquement que de la lecture.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod analysis_store;
pub mod calc;
pub mod column;
pub mod copy;
pub mod datetime;
pub mod error;
pub mod formula;
pub mod id;
pub mod name;
pub mod node;
pub mod number_format;
pub mod path;
pub mod project;
pub mod resolve;
pub mod tree;
pub mod tsv;

pub use calc::{
    compute, AuxRef, Calculation, CalculationDraft, ComputeError, Computed, ComputedColumn,
    CorrectionRecord, OutputDraft, OutputRef, ResultValue, SeriesRef, SeriesResult, Staleness,
    MAX_CALCULATION_SERIES,
};
pub use column::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, ImportSource, MetaLine,
    MixedCell, NumericStatus, ValueBlock, ValuesDelta,
};
pub use copy::NodeCopy;
pub use error::{CoreError, CoreResult};
pub use formula::{
    evaluate as evaluate_formula, topological_order, Formula, FormulaError, FormulaEvaluation,
};
pub use id::{IdAllocator, NodeId};
pub use name::{check_name, RESERVED_NAME_CHARS};
pub use node::{Node, NodeKind, WindowMeta};
pub use path::{format_path, parse_path, ObjectPath, PathDepth};
pub use project::{spreadsheet_name, Project};
pub use resolve::{
    bar_width, histogram, resolve_column, resolve_panel, resolve_series, resolve_spec,
    series_label, MissingRole, ResolvedPanel, ResolvedPlot, ResolvedSeries, ResolvedValues,
};
pub use tree::Tree;
