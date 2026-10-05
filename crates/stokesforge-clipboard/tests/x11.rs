//! L'offre X11 contre un **vrai serveur** : un second client joue
//! l'application destinataire (`TARGETS`, puis chaque type) comme le ferait
//! LibreOffice ou Inkscape.
//!
//! `#[ignore]` : il faut un serveur (`DISPLAY`, XWayland suffit), absent de la
//! CI. À lancer à la main :
//!
//! ```sh
//! cargo test -p stokesforge-clipboard --test x11 -- --ignored
//! ```
#![cfg(all(unix, not(target_os = "macos")))]

use std::time::{Duration, Instant};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, RawDisplayHandle, XlibDisplayHandle,
};
use stokesforge_clipboard::{mime, offer, Representation};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::CURRENT_TIME;

/// Un affichage Xlib sans pointeur : le backend X11 se connecte lui-même.
struct Xlib;

impl HasDisplayHandle for Xlib {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        // SAFETY : l'affichage Xlib sans pointeur n'est jamais déréférencé.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// Le client destinataire : une connexion, une fenêtre, des demandes.
struct Demandeur {
    conn: RustConnection,
    window: u32,
    clipboard: u32,
    property: u32,
}

impl Demandeur {
    fn new() -> Self {
        let (conn, screen) = x11rb::connect(None).expect("serveur X11 (DISPLAY)");
        let window = conn.generate_id().unwrap();
        let root = conn.setup().roots[screen].root;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
        let atom = |name: &str| {
            conn.intern_atom(false, name.as_bytes())
                .unwrap()
                .reply()
                .unwrap()
                .atom
        };
        let clipboard = atom("CLIPBOARD");
        let property = atom("STOKESFORGE_TEST_REPLY");
        Self {
            conn,
            window,
            clipboard,
            property,
        }
    }

    fn atom(&self, name: &str) -> u32 {
        self.conn
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom
    }

    /// Demande `target` ; rend la propriété lue, ou `None` si le propriétaire refuse
    /// (ou ne répond pas dans les 5 s).
    fn demander(&self, target: u32) -> Option<(u32, Vec<u8>)> {
        self.conn
            .convert_selection(
                self.window,
                self.clipboard,
                target,
                self.property,
                CURRENT_TIME,
            )
            .unwrap();
        self.conn.flush().unwrap();
        let limite = Instant::now() + Duration::from_secs(5);
        while Instant::now() < limite {
            match self.conn.poll_for_event().unwrap() {
                // Pas de test sur `response_type` : un événement venu d'un
                // `SendEvent` porte le bit 0x80.
                Some(Event::SelectionNotify(event)) if event.requestor == self.window => {
                    if event.property == 0 {
                        return None;
                    }
                    let reply = self
                        .conn
                        .get_property(true, self.window, self.property, AtomEnum::ANY, 0, u32::MAX)
                        .unwrap()
                        .reply()
                        .unwrap();
                    return Some((reply.type_, reply.value));
                }
                Some(_) => {}
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        None
    }
}

fn atomes(octets: &[u8]) -> Vec<u32> {
    octets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect()
}

#[test]
#[ignore = "exige un serveur X11 (DISPLAY)"]
fn un_destinataire_lit_la_liste_des_types_puis_chaque_type() {
    let svg = Representation::new(mime::SVG, b"<svg>figure</svg>".to_vec());
    let png = Representation::new(mime::PNG, vec![0x89, b'P', b'N', b'G', 1, 2, 3]);
    let pdf = Representation::new(mime::PDF, b"%PDF-figure".to_vec());
    offer(&Xlib, &[svg.clone(), png.clone(), pdf.clone()]).expect("offre X11");

    let demandeur = Demandeur::new();
    let targets = demandeur.atom("TARGETS");
    let (genre, liste) = demandeur.demander(targets).expect("TARGETS");
    assert_eq!(genre, u32::from(AtomEnum::ATOM));
    let noms: Vec<String> = atomes(&liste)
        .into_iter()
        .map(|atome| {
            String::from_utf8(
                demandeur
                    .conn
                    .get_atom_name(atome)
                    .unwrap()
                    .reply()
                    .unwrap()
                    .name,
            )
            .unwrap()
        })
        .collect();
    // L'ordre est celui de la préférence : SVG, PNG, PDF.
    assert_eq!(
        noms,
        ["TARGETS", "TIMESTAMP", mime::SVG, mime::PNG, mime::PDF]
    );

    for rep in [&svg, &png, &pdf] {
        let (genre, octets) = demandeur
            .demander(demandeur.atom(rep.mime))
            .unwrap_or_else(|| panic!("{} refusé", rep.mime));
        assert_eq!(genre, demandeur.atom(rep.mime), "type de la propriété");
        assert_eq!(octets, &*rep.bytes, "octets de {}", rep.mime);
    }

    // Témoin : un type qu'on n'offre pas est refusé, pas servi à vide.
    assert!(demandeur
        .demander(demandeur.atom("text/x-inconnu"))
        .is_none());
}

#[test]
#[ignore = "exige un serveur X11 (DISPLAY)"]
fn une_representation_plus_grosse_qu_une_requete_est_ecartee_sans_perdre_les_autres() {
    // 256 Mio dépassent toute requête X, BIG-REQUESTS comprise.
    let enorme = Representation::new(mime::PNG, vec![7u8; 256 * 1024 * 1024]);
    let petit = Representation::new(mime::SVG, b"<svg/>".to_vec());
    offer(&Xlib, &[petit.clone(), enorme]).expect("offre X11");

    let demandeur = Demandeur::new();
    let (_, liste) = demandeur
        .demander(demandeur.atom("TARGETS"))
        .expect("TARGETS");
    assert!(
        !atomes(&liste).contains(&demandeur.atom(mime::PNG)),
        "le PNG trop gros ne doit pas être annoncé"
    );
    assert!(demandeur.demander(demandeur.atom(mime::PNG)).is_none());
    let (_, octets) = demandeur.demander(demandeur.atom(mime::SVG)).expect("SVG");
    assert_eq!(octets, &*petit.bytes);
}
