//! Offer **the same figure in several forms** to the system clipboard
//! (issue #149): SVG preferably, then PNG, then PDF.
//!
//! # Why a separate crate
//!
//! egui only writes text (`Context::copy_text`) and `arboard`, which the tree
//! already contains, can only write text, HTML or a bitmap image:
//! neither offers `image/svg+xml` or `application/pdf`, nor several
//! types at once. A multi-format offer is **platform-specific** code,
//! with FFI: it lives here, with no dependency on the rest of the
//! workspace, rather than in the interface shell. This crate knows nothing
//! about StokesForge — it receives bytes labeled with a MIME type, in
//! the caller's order of preference.
//!
//! # An offer, not a choice
//!
//! [`offer`] publishes all the [`Representation`]s; it is **the receiving
//! application** that chooses among them, and some ignore the proposed
//! order (a word processor often takes the PNG before the SVG). The order
//! given is the order of intent — SVG, PNG, PDF — with no promise about the
//! receiver's choice.
//!
//! # Platforms
//!
//! | Display | Mechanism | Verified |
//! |---|---|---|
//! | Wayland | `Clipboard::store_formats` of the `vendor/smithay-clipboard` patch: the worker shared with egui-winit offers one `wl_data_source` per MIME type | by hand (see `docs/application/graph.md`) |
//! | X11 | an owner of the `CLIPBOARD` selection ([`x11`]), `TARGETS` then one type per representation | real test, `--ignored` |
//! | Windows | one registered format per representation (`PNG`, `image/svg+xml`, `application/pdf`) | compiles only |
//! | macOS | `NSPasteboard`, one UTI type per representation | compiles only |
//!
//! # Accepted limits
//!
//! - Wayland: the compositor accepts the selection only from the window that
//!   has keyboard focus and has just received an input (the "serial");
//!   otherwise the offer is **lost without an error** — this is also the
//!   behavior of egui-winit's text, and it is why the call starts from the Ctrl+C gesture.
//! - X11: no `INCR` protocol; a representation larger than an
//!   X request (256 KiB without the BIG-REQUESTS extension, about 16 MiB
//!   with it) is not offered. It is skipped, the others stay offered.
//! - The content disappears with the process (no clipboard manager
//!   supported on the X11 side).

use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

#[cfg(all(unix, not(target_os = "macos")))]
mod wayland;
#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// MIME types of a figure's representations, in StokesForge's order of
/// preference.
pub mod mime {
    /// Vector image.
    pub const SVG: &str = "image/svg+xml";
    /// Raster image.
    pub const PNG: &str = "image/png";
    /// PDF document.
    pub const PDF: &str = "application/pdf";
}

/// One form of the offered content: its bytes under a MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Representation {
    /// MIME type (see [`mime`]).
    pub mime: &'static str,
    /// The content; shared, it is never copied by the offer.
    pub bytes: Arc<[u8]>,
}

impl Representation {
    /// A representation of `bytes` under `mime`.
    pub fn new(mime: &'static str, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            mime,
            bytes: bytes.into(),
        }
    }
}

/// What can prevent offering the content. The text is raw English,
/// like that of the other crates below: the interface puts it into a sentence.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// No representation was given.
    #[error("no representation to offer")]
    Empty,
    /// This kind of display cannot receive the offer.
    #[error("the clipboard offer is not supported on this display ({0})")]
    Unsupported(&'static str),
    /// The system refused or failed the offer.
    #[error("could not offer the clipboard content: {0}")]
    Backend(String),
}

/// Offers `representations` as the current content of the system clipboard,
/// replacing the previous one.
///
/// `display` is the display of the window that received the copy gesture
/// (`eframe::Frame` is one): it chooses the mechanism, and its lifetime
/// guarantees that a Wayland pointer stays valid during the call.
///
/// `Ok` means "offer sent", not "offer accepted": under Wayland the
/// compositor may discard it without answering (see *Accepted limits*).
///
/// # Errors
///
/// [`ClipboardError::Empty`] without a representation; [`ClipboardError::Unsupported`]
/// for a display without a backend; [`ClipboardError::Backend`] if the system
/// refuses.
pub fn offer(
    display: &impl HasDisplayHandle,
    representations: &[Representation],
) -> Result<(), ClipboardError> {
    if representations.is_empty() {
        return Err(ClipboardError::Empty);
    }
    let handle = display
        .display_handle()
        .map_err(|err| ClipboardError::Backend(err.to_string()))?;
    match handle.as_raw() {
        #[cfg(all(unix, not(target_os = "macos")))]
        RawDisplayHandle::Wayland(wayland) => wayland::offer(wayland.display, representations),
        #[cfg(all(unix, not(target_os = "macos")))]
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => x11::offer(representations),
        #[cfg(windows)]
        RawDisplayHandle::Windows(_) => windows::offer(representations),
        #[cfg(target_os = "macos")]
        RawDisplayHandle::AppKit(_) => macos::offer(representations),
        other => Err(ClipboardError::Unsupported(display_name(&other))),
    }
}

/// The name of a display, for [`ClipboardError::Unsupported`].
fn display_name(display: &RawDisplayHandle) -> &'static str {
    match display {
        RawDisplayHandle::Wayland(_) => "Wayland",
        RawDisplayHandle::Xlib(_) => "Xlib",
        RawDisplayHandle::Xcb(_) => "XCB",
        RawDisplayHandle::Windows(_) => "Windows",
        RawDisplayHandle::AppKit(_) => "AppKit",
        _ => "other",
    }
}

/// The Windows clipboard format name for a MIME type: the format
/// registered by applications is `PNG` (Office, browsers, GIMP); the
/// others keep their MIME type as name, Inkscape's convention for SVG.
#[cfg(any(windows, test))]
fn windows_format_name(mime: &str) -> &str {
    if mime == mime::PNG {
        "PNG"
    } else {
        mime
    }
}

/// The macOS uniform type identifier (UTI) of a MIME type, `None` with no
/// known equivalent.
#[cfg(any(target_os = "macos", test))]
fn macos_uti(mime: &str) -> Option<&'static str> {
    match mime {
        mime::PNG => Some("public.png"),
        mime::SVG => Some("public.svg-image"),
        mime::PDF => Some("com.adobe.pdf"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Without;
    impl HasDisplayHandle for Without {
        fn display_handle(
            &self,
        ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }

    #[test]
    fn without_a_representation_the_offer_is_refused_before_any_display() {
        // Witness: even with a valid display, having nothing to offer is a
        // caller error, not an empty success.
        assert!(matches!(offer(&Without, &[]), Err(ClipboardError::Empty)));
    }

    #[test]
    fn an_unavailable_display_is_an_error_not_a_panic() {
        let one = [Representation::new(mime::PNG, vec![1, 2, 3])];
        assert!(matches!(
            offer(&Without, &one),
            Err(ClipboardError::Backend(_))
        ));
    }

    #[test]
    fn windows_formats_and_macos_utis_cover_the_three_types() {
        assert_eq!(windows_format_name(mime::PNG), "PNG");
        assert_eq!(windows_format_name(mime::SVG), "image/svg+xml");
        assert_eq!(windows_format_name(mime::PDF), "application/pdf");
        assert_eq!(macos_uti(mime::PNG), Some("public.png"));
        assert_eq!(macos_uti(mime::SVG), Some("public.svg-image"));
        assert_eq!(macos_uti(mime::PDF), Some("com.adobe.pdf"));
        // An unknown type is not invented.
        assert_eq!(macos_uti("text/plain"), None);
    }

    #[test]
    fn a_representation_shares_its_bytes() {
        let bytes: Arc<[u8]> = Arc::from(vec![9u8; 4]);
        let a = Representation::new(mime::SVG, bytes.clone());
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.bytes, &b.bytes));
        assert_eq!(Arc::strong_count(&bytes), 3);
    }
}
