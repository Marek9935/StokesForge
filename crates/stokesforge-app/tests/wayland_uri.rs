//! Garde du patch transitoire #72, sans serveur graphique.
//! Le fichier inclus est celui compilé par winit : aucun second parseur.

#![cfg(target_os = "linux")]
#[path = "../../../vendor/smithay-clipboard/src/file_drop_uri.rs"]
mod uri_list;
