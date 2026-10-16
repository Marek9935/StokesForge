//! L'état de l'application StokesForge, **sans interface graphique** (#263,
//! chantier #254).
//!
//! Ce crate reçoit tout ce que `stokesforge-app` décide sans le dessiner :
//! [`state::AppState`] (les actions de l'utilisateur, leur effet sur la
//! session, les messages d'état), la sélection, les préférences, les formulaires
//! d'analyse, les textes d'interface (`tr!`, [`i18n`]) et les erreurs mises en
//! phrase ([`errors`]). Il ne dépend d'**aucun** crate d'interface — ni
//! `egui`, ni `eframe`, ni `winit`, ni `wgpu`, pas plus que
//! `stokesforge-clipboard` qui tire winit : c'est le compilateur qui tient la
//! frontière que gardait, avant #263, une garde de texte (#255). Les tests de
//! l'état (`tests/shell.rs`…) compilent donc sans fenêtre.
//!
//! # Ce qui reste dans `stokesforge-app`
//!
//! La coquille `eframe` (`app`, `main.rs`), le thème, les icônes et les briques
//! de champ qui nomment un type d'egui. Elle lit cet état et le peint ; elle
//! n'y ajoute aucune décision de produit. L'offre de la figure au
//! presse-papiers système (`stokesforge-clipboard`) y reste aussi : l'état ne
//! garde que des octets neutres ([`state::FigureBytes`]).
//!
//! # Une frontière lisible
//!
//! Les modules sont publics un à un, sans `pub use` géant : le chemin d'un
//! import (`stokesforge_app_state::selection::…`) dit d'où vient l'élément.
//! Seul [`tr!`] se trouve à la racine (`#[macro_export]`).

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod analysis;
pub mod analysis_recent;
pub mod errors;
pub mod file_picker;
pub mod history_text;
pub mod i18n;
pub mod object_drag;
pub mod object_format;
pub mod object_manager;
pub mod preferences;
pub mod quantities;
pub mod selection;
pub mod state;
pub mod text_format;
pub mod user_colors;
