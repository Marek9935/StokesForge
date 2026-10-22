//! L'offre Windows contre le **vrai presse-papiers** : un second accès relit
//! chaque format comme le ferait Office, un navigateur ou GIMP.
//!
//! `#[ignore]` : le test écrase le presse-papiers de la session et exige un
//! bureau interactif, absent d'un runner en service. À lancer à la main :
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

/// Un affichage Windows : le backend n'a besoin d'aucune fenêtre.
struct Windows;

impl HasDisplayHandle for Windows {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        // SAFETY : le handle Windows ne porte aucun pointeur.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// Relit les octets du format enregistré `name`.
fn read_registered(name: &str) -> Vec<u8> {
    let _clipboard = Clipboard::new_attempts(10).expect("ouvrir le presse-papiers");
    let format = raw::register_format(name).expect("format enregistré");
    let mut out = Vec::new();
    raw::get_vec(format.get(), &mut out).expect("lire le format");
    out
}

#[test]
#[ignore = "écrase le vrai presse-papiers de la session"]
fn l_offre_est_relue_format_par_format() {
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec();
    offer(
        &Windows,
        &[
            Representation::new(mime::PNG, png.clone()),
            Representation::new(mime::SVG, svg.clone()),
        ],
    )
    .expect("offre acceptée");

    // PNG : sous le nom enregistré « PNG », pas sous le type MIME. Témoin : un
    // `windows_format_name` qui rendrait le MIME pour PNG laisserait ce nom
    // vide et ferait échouer la lecture.
    assert_eq!(read_registered("PNG"), png);
    assert_eq!(read_registered(mime::SVG), svg);
    // Aucun format texte n'est offert par erreur.
    assert!(!raw::is_format_avail(formats::CF_UNICODETEXT));
}
