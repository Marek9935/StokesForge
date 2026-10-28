//! The X11 offer against a **real server**: a second client plays the
//! receiving application (`TARGETS`, then each type) as LibreOffice or
//! Inkscape would.
//!
//! `#[ignore]`: a server is needed (`DISPLAY`, XWayland is enough), absent from
//! CI. Run by hand:
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

/// An Xlib display without a pointer: the X11 backend connects by itself.
struct Xlib;

impl HasDisplayHandle for Xlib {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        // SAFETY: the pointerless Xlib display is never dereferenced.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// The receiving client: one connection, one window, some requests.
struct Requester {
    conn: RustConnection,
    window: u32,
    clipboard: u32,
    property: u32,
}

impl Requester {
    fn new() -> Self {
        let (conn, screen) = x11rb::connect(None).expect("X11 server (DISPLAY)");
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

    /// Requests `target`; returns the property read, or `None` if the owner refuses
    /// (or does not answer within 5 s).
    fn request(&self, target: u32) -> Option<(u32, Vec<u8>)> {
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
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.conn.poll_for_event().unwrap() {
                // No test on `response_type`: an event coming from a
                // `SendEvent` carries the 0x80 bit.
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

fn atoms(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect()
}

#[test]
#[ignore = "requires an X11 server (DISPLAY)"]
fn a_receiver_reads_the_list_of_types_then_each_type() {
    let svg = Representation::new(mime::SVG, b"<svg>figure</svg>".to_vec());
    let png = Representation::new(mime::PNG, vec![0x89, b'P', b'N', b'G', 1, 2, 3]);
    let pdf = Representation::new(mime::PDF, b"%PDF-figure".to_vec());
    offer(&Xlib, &[svg.clone(), png.clone(), pdf.clone()]).expect("X11 offer");

    let requester = Requester::new();
    let targets = requester.atom("TARGETS");
    let (kind, list) = requester.request(targets).expect("TARGETS");
    assert_eq!(kind, u32::from(AtomEnum::ATOM));
    let names: Vec<String> = atoms(&list)
        .into_iter()
        .map(|atom| {
            String::from_utf8(
                requester
                    .conn
                    .get_atom_name(atom)
                    .unwrap()
                    .reply()
                    .unwrap()
                    .name,
            )
            .unwrap()
        })
        .collect();
    // The order is the order of preference: SVG, PNG, PDF.
    assert_eq!(
        names,
        ["TARGETS", "TIMESTAMP", mime::SVG, mime::PNG, mime::PDF]
    );

    for rep in [&svg, &png, &pdf] {
        let (kind, bytes) = requester
            .request(requester.atom(rep.mime))
            .unwrap_or_else(|| panic!("{} refused", rep.mime));
        assert_eq!(kind, requester.atom(rep.mime), "type of the property");
        assert_eq!(bytes, &*rep.bytes, "bytes of {}", rep.mime);
    }

    // Witness: a type we do not offer is refused, not served empty.
    assert!(requester
        .request(requester.atom("text/x-unknown"))
        .is_none());
}

#[test]
#[ignore = "requires an X11 server (DISPLAY)"]
fn a_representation_larger_than_a_request_is_dropped_without_losing_the_others() {
    // 256 MiB exceeds any X request, BIG-REQUESTS included.
    let huge = Representation::new(mime::PNG, vec![7u8; 256 * 1024 * 1024]);
    let small = Representation::new(mime::SVG, b"<svg/>".to_vec());
    offer(&Xlib, &[small.clone(), huge]).expect("X11 offer");

    let requester = Requester::new();
    let (_, list) = requester
        .request(requester.atom("TARGETS"))
        .expect("TARGETS");
    assert!(
        !atoms(&list).contains(&requester.atom(mime::PNG)),
        "the PNG that is too big must not be announced"
    );
    assert!(requester.request(requester.atom(mime::PNG)).is_none());
    let (_, bytes) = requester.request(requester.atom(mime::SVG)).expect("SVG");
    assert_eq!(bytes, &*small.bytes);
}
