//! macOS : `NSPasteboard`, un type UTI par représentation.
//!
//! Le pasteboard général reçoit chaque représentation sous son identifiant de
//! type uniforme (`public.png`, `public.svg-image`, `com.adobe.pdf`) ; l'ordre
//! d'écriture est l'ordre de préférence. **Non exécuté** : ce module est
//! compilé pour macOS par `cargo check --target aarch64-apple-darwin`, rien
//! de plus.

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSData, NSString};

use crate::{macos_uti, ClipboardError, Representation};

/// Remplace le pasteboard général par `representations`.
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
