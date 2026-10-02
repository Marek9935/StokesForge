//! Le socle commun aux deux backends de rendu — phase R4.
//!
//! Ce crate ne dessine rien tout seul. Il porte les deux choses que
//! `plotyx-render-egui` et `plotyx-render-svg` doivent absolument avoir **en
//! un seul exemplaire**, sans quoi le critère de fin du chantier de rendu —
//! *axes au même endroit à l'écran et à l'export* — ne serait qu'un vœu :
//!
//! - [`text`] : la source unique de mesure de texte (tâche R4.1), une
//!   `epaint::FontDefinitions` et l'unique `estimate_text_size` qui en
//!   découle ;
//! - [`pipeline`] : la suite d'appels Plotters, générique sur le backend, de
//!   sorte qu'il n'existe pas deux façons de construire la même figure ;
//! - [`color`] : la lecture des couleurs `#RRGGBB` du spec, également
//!   partagée.
//!
//! # Ce que la phase R5 y a ajouté
//!
//! Tout l'élargissement au `PlotSpec` réel est descendu ici, et pas une
//! ligne dans l'un ou l'autre backend — c'est la règle de la phase
//! (« aucune fonctionnalité n'entre dans un seul backend »), tenue par la
//! structure :
//!
//! - [`series`] : les six [`plotyx_plot::PlotType`] traduits en géométrie,
//!   une fois, pour le cadrage comme pour le tracé ;
//! - [`scale`] : les échelles d'axe, dont la logarithmique ;
//! - [`paint`] : les primitives en pixels — styles de trait, marqueurs,
//!   barres, moustaches ;
//! - [`legend`] : la légende, mise en page sur la mesure du projet et non
//!   sur celle de Plotters (la raison est dans le module) ;
//! - [`decimate`] : le LTTB ;
//! - [`sample`] : un graphe réaliste, partagé par l'exemple d'export et par
//!   le test d'accord, pour que ce qu'on regarde soit ce que le test
//!   compare.
//!
//! # Un crate non prévu par `architecture-choisie.md` §8
//!
//! Le découpage arrêté ne prévoyait que `plotyx-render-egui` et
//! `plotyx-render-svg`. Ce crate-ci s'ajoute, pour la même raison que
//! `plotyx-id` s'était ajouté en R3 : deux crates frères ont besoin de la
//! même chose, et aucun des deux ne peut dépendre de l'autre sans mentir sur
//! ce qu'il est. Faire porter la mesure par `plotyx-render-svg` obligerait le
//! backend écran à dépendre du crate d'export ; la faire porter par
//! `plotyx-render-egui` ferait entrer toute la pile egui dans l'export, ce
//! que le découpage interdit explicitement. La sortie est la même qu'en R3 :
//! descendre la partie commune dans un crate en dessous des deux.
//!
//! Conséquence sur les frontières, et elle va dans le bon sens : `epaint`
//! entre dans `plotyx-render-svg` par ici, pas en direct, et reste la **même
//! version** que celle qu'`egui` résout pour `plotyx-render-egui` — c'est
//! vérifié par `tests/frontiere.rs` plutôt que supposé.
//!
//! # Ce que ce crate ne connaît pas
//!
//! Ni `egui`, ni `eframe`, ni `plotters-svg`. Il voit `epaint` (pour mesurer)
//! et `plotters` sans ses fonts (pour construire la figure) — rien de la
//! surface de sortie. C'est ce qui lui permet d'être la référence commune
//! plutôt qu'un troisième dialecte.
//!
//! # Ce qu'une tâche ponctuelle du 2026-09-13 y a ajouté
//!
//! [`fonts`] : le choix entre trois paires de polices embarquées
//! (Libertinus, Fira, Noto — texte et Math) ou une police système désignée
//! par son nom, avec repli silencieux sur Noto. [`text::font_definitions`]
//! en devient le point d'entrée par défaut ; l'invariant qui fait foi
//! (mesurer avec exactement le même jeu de polices des deux côtés) ne change
//! pas — voir la documentation du module et
//! `docs/archive/newarchitecture/architecture-choisie.md` §7.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod clip;
pub mod color;
pub mod decimate;
pub mod fonts;
pub mod formula;
pub mod hit;
pub mod interact;
pub mod legend;
pub mod paint;
pub mod pipeline;
pub mod sample;
pub mod scale;
pub mod series;
pub mod text;

pub use color::{parse_hex, rgb_or};
pub use decimate::lttb;
pub use fonts::{available_families, FamilyEntry, FontChoice, FontSet, FontSource, ResolvedFonts};
pub use hit::{component_at, AxisSide, PlotComponent};
pub use interact::{nearest_on_curve, PanelTransform};
pub use pipeline::{
    draw, draw_on, AxisArea, AxisAreas, DrawnAnnotation, DrawnLegendRow, DrawnPanel, DrawnPlot,
    DrawnText,
};
pub use series::{Geometry, Prepared};
pub use text::{estimate, font_definitions, font_id, text_options, TextMeasurer, TextSource};
