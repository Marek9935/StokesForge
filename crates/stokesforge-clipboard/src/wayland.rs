//! Wayland: the clipboard worker **shared** with egui-winit.
//!
//! `vendor/smithay-clipboard` keeps one worker per `wl_display` (see
//! `vendor/winit/STOKESFORGE.md`); `Clipboard::new` on the same pointer returns
//! the same one. We do not keep our own handle: egui-winit holds its own as long
//! as the window exists, which keeps the worker — and so the selection
//! source — alive after this call.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::{ClipboardError, Representation};

/// Offers `representations` through the shared worker of the `wl_display` `display`.
pub(crate) fn offer(
    display: NonNull<c_void>,
    representations: &[Representation],
) -> Result<(), ClipboardError> {
    let formats: Vec<(String, Arc<[u8]>)> = representations
        .iter()
        .map(|rep| (rep.mime.to_owned(), Arc::clone(&rep.bytes)))
        .collect();
    // SAFETY: `display` comes from a `DisplayHandle` borrowed by `offer`, so
    // valid during this whole call; the shared worker we join (or create)
    // is egui-winit's for this same display.
    let clipboard = unsafe { smithay_clipboard::Clipboard::new(display.as_ptr()) };
    clipboard.store_formats(formats);
    Ok(())
}
