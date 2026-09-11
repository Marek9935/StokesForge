//! `PlotSpec` : la description d'un graphe, **indépendante de tout moteur de
//! rendu**.
//!
//! Ce crate dit *ce qu'il faut tracer*. Il ne sait pas *comment* on le trace,
//! et c'est vérifié par le compilateur : sa seule dépendance est
//! `plotyx-core`, pour les [`plotyx_core::NodeId`]
//! (`tests/no_ui_dependency.rs`). Ni `plotters`, ni `egui`, ni `typst` — pas
//! même `serde`, la persistance ayant ses propres types d'enregistrement et
//! faisant elle-même la conversion.
//!
//! C'est la transposition exacte du « aucun Qt dans `core` » du socle, et la
//! raison en est mesurée : la couche équivalente de la version Python a
//! survécu à un changement complet de renderer sans qu'une ligne bouge
//! (`docs/newarchitecture/roadmap-rendu.md`).
//!
//! # Ce que la couche décrit
//!
//! ```text
//! PlotSpec
//! ├── title, background
//! └── panels : PanelSpec       (grille row/column, au moins un panneau)
//!     ├── title
//!     ├── bottom / left / right / top → AxisSpec  (échelle, limites, grille)
//!     ├── legend : LegendSpec
//!     └── series : SeriesSpec
//!         ├── y : NodeId, x / x_error / y_error : Option<NodeId>
//!         ├── plot_type : scatter | line | line+scatter | bar | histogram | error_bar
//!         ├── line : LineSpec      (style, épaisseur, couleur)
//!         ├── symbol : SymbolSpec  (forme, taille, couleurs)
//!         └── axis : SeriesAxis    (gauche ou droite, et rien d'autre)
//! ```
//!
//! # Trois décisions structurent ce crate
//!
//! - **référence par identité** — une série désigne ses colonnes par
//!   [`plotyx_core::NodeId`], jamais par nom ni par chemin
//!   (`architecture-choisie.md` §11.1). Renommer ou déplacer une colonne
//!   laisse le graphe intact ; la supprimer produit une référence brisée
//!   *détectable*, que le rendu devra afficher comme telle et jamais comme une
//!   série vide. C'est littéralement la première arête de dépendance du
//!   projet : un graphe est un nœud dérivé qui ne calcule rien ;
//!
//! - **aucune donnée** — un spec ne porte que des identités et du style. Il
//!   pèse ses chaînes, ce qui rend l'annulation d'une modification de graphe
//!   triviale : le delta d'annulation est l'ancien spec ;
//!
//! - **les états illégaux ne se valident pas, ils ne s'écrivent pas** — une
//!   série se rattache à [`SeriesAxis`] (deux variantes) et non à
//!   [`AxisSide`] (quatre) ; sa colonne Y est un `NodeId` et non une chaîne
//!   qui pourrait être vide ; son nombre de classes est un
//!   [`std::num::NonZeroU32`] qu'on n'a pas à ramener à 1 ; la position d'un
//!   panneau dans la grille est un `usize` qu'on n'a pas à ramener à 0. Là où
//!   la version Python levait ou corrigeait à l'exécution, il n'y a plus rien
//!   à lever. Le seul invariant qui reste à tenir par construction est « au
//!   moins un panneau » ([`Panels`]).
//!
//! # Ce qui n'est pas ici
//!
//! La **résolution** (`NodeId` → colonne de l'arbre, préparation des points,
//! classes d'histogramme, largeur de barre déduite) est la tranche suivante ;
//! elle sera le seul endroit du chantier de rendu à connaître un `Tree`.
//! L'**édition** (fonctions pures spec → spec) et la **sérialisation** ne sont
//! pas non plus dans ce crate : le format sur disque appartient à
//! `plotyx-persistence`, qui convertit depuis ses propres enregistrements.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod axis;
pub mod legend;
pub mod panel;
pub mod series;
pub mod spec;
pub mod style;

pub use axis::{AxisScale, AxisSide, AxisSpec};
pub use legend::{LegendPosition, LegendSpec};
pub use panel::PanelSpec;
pub use series::{PlotType, SeriesAxis, SeriesSpec, DEFAULT_HISTOGRAM_BINS};
pub use spec::{Panels, PlotSpec, SeriesRef, SPEC_FORMAT_VERSION};
pub use style::{palette_color, LineSpec, LineStyle, SymbolShape, SymbolSpec, DEFAULT_PALETTE};
