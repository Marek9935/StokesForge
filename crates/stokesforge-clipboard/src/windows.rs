//! Windows : un format de presse-papiers enregistré par représentation.
//!
//! Le presse-papiers Windows tient plusieurs formats d'un même contenu ; les
//! applications énumèrent et prennent le plus riche qu'elles comprennent.
//! `PNG` est le format enregistré qu'Office, les navigateurs et GIMP lisent ;
//! le SVG et le PDF gardent leur type MIME comme nom (convention d'Inkscape
//! pour le SVG). Vérifié contre le vrai presse-papiers par `tests/windows.rs` (#332, `--ignored`) ; compilé aussi par
//! `cargo check --target x86_64-pc-windows-gnu`.

use clipboard_win::{raw, Clipboard};

use crate::{windows_format_name, ClipboardError, Representation};

/// Remplace le presse-papiers par `representations`, un format chacune.
pub(crate) fn offer(representations: &[Representation]) -> Result<(), ClipboardError> {
    // Le presse-papiers est ouvert par un autre processus pendant un instant :
    // quelques essais avant d'abandonner.
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
