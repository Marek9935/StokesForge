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

/// The shared worker of the `wl_display` `display`: held, it keeps the worker
/// alive and lets any thread offer through it.
///
/// # Safety contract
///
/// `display` must be a live `wl_display`; the caller drops the result before
/// the display goes away (the root does it in `on_exit`).
pub(crate) fn open(display: NonNull<c_void>) -> smithay_clipboard::Clipboard {
    // SAFETY: `display` comes from a `DisplayHandle` borrowed by the caller, so
    // valid at this call; the shared worker we join (or create) is egui-winit's
    // for this same display.
    unsafe { smithay_clipboard::Clipboard::new(display.as_ptr()) }
}

/// Offers `representations` through the worker.
pub(crate) fn offer(
    clipboard: &smithay_clipboard::Clipboard,
    representations: &[Representation],
) -> Result<(), ClipboardError> {
    let formats: Vec<(String, Arc<[u8]>)> = representations
        .iter()
        .map(|rep| (rep.mime.to_owned(), Arc::clone(&rep.bytes)))
        .collect();
    clipboard.store_formats(formats);
    Ok(())
}
