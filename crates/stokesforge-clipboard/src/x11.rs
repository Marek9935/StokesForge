//! X11: an owner of the `CLIPBOARD` selection that serves several types.
//!
//! X11 has no clipboard: the selection stays **with its owner**,
//! who answers the applications' requests. `arboard` (in egui-winit) can only
//! put one type there at a time; so we hold the connection and the
//! window ourselves, in a thread, until another client takes the
//! selection (`SelectionClear`) — or the process ends.
//!
//! The ICCCM protocol, as far as useful: `TARGETS` lists the types,
//! `TIMESTAMP` says when we took the selection, a MIME type returns its
//! bytes. No `INCR`: a representation that does not fit in an
//! X request is not offered (see the crate docs). What an answer
//! decides is the pure function [`answer`], tested without a server; the rest is
//! plumbing tested against a real server (`tests/x11.rs`, `--ignored`).

use std::thread;

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent,
    SelectionRequestEvent, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::{ClipboardError, Representation};

/// An answer to a request to convert the selection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer<'a> {
    /// The list of offered types (`TARGETS`), atoms included.
    Targets(Vec<u32>),
    /// The time at which we took the selection (`TIMESTAMP`).
    Timestamp,
    /// The bytes of a representation.
    Data(&'a Representation),
    /// Nothing to answer: unknown type, or too big for an X request.
    Refuse,
}

/// What the request for type `target` gets. `offered` maps each MIME type
/// atom to its representation, in order of preference; `targets` and
/// `timestamp` are the `TARGETS` and `TIMESTAMP` atoms; `max_bytes` is the
/// maximum size of a property that a request can carry.
pub(crate) fn answer<'a>(
    target: u32,
    offered: &'a [(u32, Representation)],
    targets: u32,
    timestamp: u32,
    max_bytes: usize,
) -> Answer<'a> {
    if target == targets {
        let mut list = vec![targets, timestamp];
        list.extend(
            offered
                .iter()
                .filter(|(_, rep)| rep.bytes.len() <= max_bytes)
                .map(|(atom, _)| *atom),
        );
        return Answer::Targets(list);
    }
    if target == timestamp {
        return Answer::Timestamp;
    }
    match offered.iter().find(|(atom, _)| *atom == target) {
        Some((_, rep)) if rep.bytes.len() <= max_bytes => Answer::Data(rep),
        _ => Answer::Refuse,
    }
}

/// The atoms the owner needs.
struct Atoms {
    clipboard: u32,
    targets: u32,
    timestamp: u32,
    scratch: u32,
}

fn backend<E: std::fmt::Display>(error: E) -> ClipboardError {
    ClipboardError::Backend(error.to_string())
}

/// Takes the `CLIPBOARD` selection and serves `representations` from a thread,
/// as long as we own it.
pub(crate) fn offer(representations: &[Representation]) -> Result<(), ClipboardError> {
    let (conn, screen_number) = x11rb::connect(None).map_err(backend)?;
    let window = conn.generate_id().map_err(backend)?;
    let root = conn.setup().roots[screen_number].root;
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
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(backend)?;

    let intern = |name: &str| -> Result<u32, ClipboardError> {
        Ok(conn
            .intern_atom(false, name.as_bytes())
            .map_err(backend)?
            .reply()
            .map_err(backend)?
            .atom)
    };
    let atoms = Atoms {
        clipboard: intern("CLIPBOARD")?,
        targets: intern("TARGETS")?,
        timestamp: intern("TIMESTAMP")?,
        scratch: intern("STOKESFORGE_CLIPBOARD_TIME")?,
    };
    let mut offered = Vec::with_capacity(representations.len());
    for rep in representations {
        offered.push((intern(rep.mime)?, rep.clone()));
    }

    // A selection is taken "at a server time", not at `CurrentTime`:
    // an empty property change yields one through `PropertyNotify`.
    conn.change_property8(
        PropMode::APPEND,
        window,
        atoms.scratch,
        AtomEnum::STRING,
        &[],
    )
    .map_err(backend)?;
    conn.flush().map_err(backend)?;
    let time = loop {
        if let Event::PropertyNotify(event) = conn.wait_for_event().map_err(backend)? {
            if event.window == window && event.atom == atoms.scratch {
                break event.time;
            }
        }
    };

    conn.set_selection_owner(window, atoms.clipboard, time)
        .map_err(backend)?;
    conn.flush().map_err(backend)?;
    let owner = conn
        .get_selection_owner(atoms.clipboard)
        .map_err(backend)?
        .reply()
        .map_err(backend)?
        .owner;
    if owner != window {
        return Err(ClipboardError::Backend(
            "another client kept the CLIPBOARD selection".to_owned(),
        ));
    }

    thread::Builder::new()
        .name("stokesforge-x11-clipboard".to_owned())
        .spawn(move || serve(&conn, window, &atoms, &offered, time))
        .map(drop)
        .map_err(backend)
}

/// Answers requests until the selection or the connection is lost.
fn serve(
    conn: &RustConnection,
    window: u32,
    atoms: &Atoms,
    offered: &[(u32, Representation)],
    time: u32,
) {
    // A request header and its margins: a property must fit in the
    // rest, otherwise the server would refuse the request.
    let max_bytes = conn.maximum_request_bytes().saturating_sub(64);
    while let Ok(event) = conn.wait_for_event() {
        match event {
            Event::SelectionRequest(request) => {
                let property = reply(conn, &request, atoms, offered, time, max_bytes);
                let notify = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property,
                };
                let sent = conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify);
                if sent.is_err() || conn.flush().is_err() {
                    break;
                }
            }
            Event::SelectionClear(clear) if clear.selection == atoms.clipboard => break,
            _ => {}
        }
    }
    let _ = conn.destroy_window(window);
    let _ = conn.flush();
}

/// Writes the answer on the requesting window; returns the property to announce
/// (`NONE` — 0 — when refusing).
fn reply(
    conn: &RustConnection,
    request: &SelectionRequestEvent,
    atoms: &Atoms,
    offered: &[(u32, Representation)],
    time: u32,
    max_bytes: usize,
) -> u32 {
    const NONE: u32 = 0;
    if request.selection != atoms.clipboard {
        return NONE;
    }
    // ICCCM §2.2: an old requestor leaves the property empty, the target stands in for it.
    let property = if request.property == NONE {
        request.target
    } else {
        request.property
    };
    let written = match answer(
        request.target,
        offered,
        atoms.targets,
        atoms.timestamp,
        max_bytes,
    ) {
        Answer::Targets(list) => conn.change_property32(
            PropMode::REPLACE,
            request.requestor,
            property,
            AtomEnum::ATOM,
            &list,
        ),
        Answer::Timestamp => conn.change_property32(
            PropMode::REPLACE,
            request.requestor,
            property,
            AtomEnum::INTEGER,
            &[time],
        ),
        Answer::Data(rep) => conn.change_property8(
            PropMode::REPLACE,
            request.requestor,
            property,
            request.target,
            &rep.bytes,
        ),
        Answer::Refuse => return NONE,
    };
    if written.is_ok() {
        property
    } else {
        NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mime;

    const TARGETS: u32 = 10;
    const TIMESTAMP: u32 = 11;
    const SVG: u32 = 20;
    const PNG: u32 = 21;
    const PDF: u32 = 22;

    fn offered(sizes: [usize; 3]) -> Vec<(u32, Representation)> {
        vec![
            (SVG, Representation::new(mime::SVG, vec![1; sizes[0]])),
            (PNG, Representation::new(mime::PNG, vec![2; sizes[1]])),
            (PDF, Representation::new(mime::PDF, vec![3; sizes[2]])),
        ]
    }

    #[test]
    fn targets_lists_the_types_in_order_of_preference() {
        let offered = offered([10, 10, 10]);
        let response = answer(TARGETS, &offered, TARGETS, TIMESTAMP, 100);
        assert_eq!(
            response,
            Answer::Targets(vec![TARGETS, TIMESTAMP, SVG, PNG, PDF])
        );
    }

    #[test]
    fn a_type_too_big_is_neither_listed_nor_served() {
        // The PNG exceeds the X request: it is missing from the list and the
        // direct request is refused; the SVG and the PDF stay offered (witness:
        // each one answers fine before being too big).
        let offered = offered([10, 500, 10]);
        assert_eq!(
            answer(TARGETS, &offered, TARGETS, TIMESTAMP, 100),
            Answer::Targets(vec![TARGETS, TIMESTAMP, SVG, PDF])
        );
        assert_eq!(
            answer(PNG, &offered, TARGETS, TIMESTAMP, 100),
            Answer::Refuse
        );
        assert!(matches!(
            answer(SVG, &offered, TARGETS, TIMESTAMP, 100),
            Answer::Data(rep) if rep.mime == mime::SVG
        ));
        assert!(matches!(
            answer(PNG, &offered, TARGETS, TIMESTAMP, 1000),
            Answer::Data(rep) if rep.mime == mime::PNG
        ));
    }

    #[test]
    fn timestamp_and_unknown_type() {
        let offered = offered([1, 1, 1]);
        assert_eq!(
            answer(TIMESTAMP, &offered, TARGETS, TIMESTAMP, 100),
            Answer::Timestamp
        );
        assert_eq!(
            answer(999, &offered, TARGETS, TIMESTAMP, 100),
            Answer::Refuse
        );
    }
}
