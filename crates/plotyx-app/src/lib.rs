//! Coquille UI minimale de PlotYx : le banc d'essai qui fait tourner **une
//! fois** la boucle complète — modèle → commande → annulation → sauvegarde →
//! rechargement — dans une vraie fenêtre.
//!
//! # Ce que ce crate n'est pas
//!
//! Ce n'est **pas** une ébauche de l'interface finale. Le docking, la grille
//! éditable à délégués, le panneau de propriétés et le système de graphes
//! seront repris entièrement plus tard ; rien ici ne doit les préempter. Le
//! banc est volontairement pauvre : son seul rôle est de prouver que
//! l'intégration bout en bout tient, tant qu'une intégration ratée ne coûte
//! qu'une réécriture de ce crate — et non celle de tout ce qu'on aurait bâti
//! par-dessus.
//!
//! # Découpage
//!
//! - [`state`] : toute la logique, **sans egui**, pilotée par un enum
//!   d'actions — donc testable en intégration continue, sans serveur
//!   graphique (`tests/shell.rs`) ;
//! - [`app`] : la glu `eframe`, qui traduit clavier et souris en actions et
//!   peint l'état. Rien de ce qu'elle contient n'est une décision de produit.
//!
//! # Ce que Qt donnait et qu'il faut écrire ici
//!
//! `QUndoStack` + `QAction::Undo` fournissaient presque gratuitement le
//! raccourci clavier, l'entrée de menu, son grisage et son libellé
//! (« Annuler *Renommer* »). Ici, chacun de ces quatre points est du code
//! explicite : le raccourci dans [`app`], le grisage par `add_enabled`, le
//! libellé par [`state::AppState::undo_label`].

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod app;
pub mod state;
