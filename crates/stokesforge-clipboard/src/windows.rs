//! Windows: one registered clipboard format per representation.
//!
//! The Windows clipboard holds several formats of the same content; the
//! applications enumerate and take the richest one they understand.
//! `PNG` is the registered format that Office, browsers and GIMP read;
//! SVG and PDF keep their MIME type as name (Inkscape's convention
//! for SVG). Verified against the real clipboard by `tests/windows.rs` (#332, `--ignored`); also compiled by
//! `cargo check --target x86_64-pc-windows-gnu`.

use clipboard_win::{raw, Clipboard};

use crate::{windows_format_name, ClipboardError, Representation};

/// Replaces the clipboard with `representations`, one format each.
pub(crate) fn offer(representations: &[Representation]) -> Result<(), ClipboardError> {
    // The clipboard is briefly open in another process:
    // a few attempts before giving up.
    let _clipboard = Clipboard::new_attempts(10)
        .map_err(|err| ClipboardError::Backend(format!("could not open the clipboard: {err}")))?;
    raw::empty()
        .map_err(|err| ClipboardError::Backend(format!("could not empty the clipboard: {err}")))?;
    let mut offered = 0;
    let mut last_error = None;
    for rep in representations {
        let name = windows_format_name(rep.mime);
        let Some(format) = raw::register_format(name) else {
            last_error = Some(format!("could not register the {name} format"));
            continue;
        };
        match raw::set_without_clear(format.get(), &rep.bytes) {
            Ok(()) => offered += 1,
            Err(err) => last_error = Some(format!("could not set {name}: {err}")),
        }
    }
    if offered > 0 {
        Ok(())
    } else {
        Err(ClipboardError::Backend(
            last_error.unwrap_or_else(|| "nothing was offered".to_owned()),
        ))
    }
}
