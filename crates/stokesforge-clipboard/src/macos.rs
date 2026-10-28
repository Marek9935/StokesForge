//! macOS: `NSPasteboard`, one UTI type per representation.
//!
//! The general pasteboard receives each representation under its uniform
//! type identifier (`public.png`, `public.svg-image`, `com.adobe.pdf`); the
//! writing order is the order of preference. **Not run**: this module is
//! compiled for macOS by `cargo check --target aarch64-apple-darwin`, nothing
//! more.

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSData, NSString};

use crate::{macos_uti, ClipboardError, Representation};

/// Replaces the general pasteboard with `representations`.
pub(crate) fn offer(representations: &[Representation]) -> Result<(), ClipboardError> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let mut offered = 0;
    for rep in representations {
        let Some(uti) = macos_uti(rep.mime) else {
            continue;
        };
        let data = NSData::with_bytes(&rep.bytes);
        if pasteboard.setData_forType(Some(&data), &NSString::from_str(uti)) {
            offered += 1;
        }
    }
    if offered > 0 {
        Ok(())
    } else {
        Err(ClipboardError::Backend(
            "NSPasteboard refused every representation".to_owned(),
        ))
    }
}
