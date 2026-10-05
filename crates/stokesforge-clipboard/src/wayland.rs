//! Wayland : le worker de presse-papiers **partagé** avec egui-winit.
//!
//! `vendor/smithay-clipboard` garde un worker par `wl_display` (voir
//! `vendor/winit/STOKESFORGE.md`) ; `Clipboard::new` sur le même pointeur rend
//! le même. On ne garde pas notre poignée : egui-winit tient la sienne tant
//! que la fenêtre existe, ce qui garde le worker — et donc la source de
//! sélection — en vie après cet appel.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::{ClipboardError, Representation};

/// Offre `representations` par le worker partagé du `wl_display` `display`.
pub(crate) fn offer(
    display: NonNull<c_void>,
    representations: &[Representation],
) -> Result<(), ClipboardError> {
    let formats: Vec<(String, Arc<[u8]>)> = representations
        .iter()
        .map(|rep| (rep.mime.to_owned(), Arc::clone(&rep.bytes)))
        .collect();
    // SAFETY : `display` vient d'un `DisplayHandle` emprunté par `offer`, donc
    // valide pendant tout cet appel ; le worker partagé qu'on rejoint (ou crée)
    // est celui d'egui-winit pour ce même affichage.
    let clipboard = unsafe { smithay_clipboard::Clipboard::new(display.as_ptr()) };
    clipboard.store_formats(formats);
    Ok(())
}
