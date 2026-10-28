//! The Windows offer against the **real clipboard**: a second access reads
//! each format back as Office, a browser or GIMP would.
//!
//! `#[ignore]`: the test overwrites the session's clipboard and requires an
//! interactive desktop, absent from a service runner. Run by hand:
//!
//! ```sh
//! cargo test -p stokesforge-clipboard --test windows -- --ignored
//! ```
#![cfg(windows)]

use clipboard_win::{formats, raw, Clipboard};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, RawDisplayHandle, WindowsDisplayHandle,
};
use stokesforge_clipboard::{mime, offer, Representation};

/// A Windows display: the backend needs no window.
struct Windows;

impl HasDisplayHandle for Windows {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        // SAFETY: the Windows handle carries no pointer.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// Reads back the bytes of the registered format `name`.
fn read_registered(name: &str) -> Vec<u8> {
    let _clipboard = Clipboard::new_attempts(10).expect("open the clipboard");
    let format = raw::register_format(name).expect("registered format");
    let mut out = Vec::new();
    raw::get_vec(format.get(), &mut out).expect("read the format");
    out
}

#[test]
#[ignore = "overwrites the real clipboard of the session"]
fn the_offer_is_read_back_format_by_format() {
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec();
    offer(
        &Windows,
        &[
            Representation::new(mime::PNG, png.clone()),
            Representation::new(mime::SVG, svg.clone()),
        ],
    )
    .expect("offer accepted");

    // PNG: under the registered name "PNG", not under the MIME type. Witness: a
    // `windows_format_name` that returned the MIME for PNG would leave that name
    // empty and make the read fail.
    assert_eq!(read_registered("PNG"), png);
    assert_eq!(read_registered(mime::SVG), svg);
    // No text format is offered by mistake.
    assert!(!raw::is_format_avail(formats::CF_UNICODETEXT));
}
