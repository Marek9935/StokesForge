//! X11 : un propriétaire de la sélection `CLIPBOARD` qui sert plusieurs types.
//!
//! X11 n'a pas de presse-papiers : la sélection reste **chez son propriétaire**,
//! qui répond aux demandes des applications. `arboard` (dans egui-winit) ne
//! sait y mettre qu'un type à la fois ; on tient donc la connexion et la
//! fenêtre nous-mêmes, dans un fil, jusqu'à ce qu'un autre prenne la
//! sélection (`SelectionClear`) — ou que le processus finisse.
//!
//! Le protocole ICCCM, dans la mesure utile : `TARGETS` liste les types,
//! `TIMESTAMP` dit quand on a pris la sélection, un type MIME rend ses
//! octets. Pas d'`INCR` : une représentation qui ne tient pas dans une
//! requête X n'est pas offerte (voir la doc du crate). Ce que décide une
//! réponse est la fonction pure [`answer`], testée sans serveur ; le reste est
//! de la plomberie testée contre un vrai serveur (`tests/x11.rs`, `--ignored`).

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

/// Une réponse à une demande de conversion de la sélection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer<'a> {
    /// La liste des types offerts (`TARGETS`), atomes compris.
    Targets(Vec<u32>),
    /// L'heure à laquelle on a pris la sélection (`TIMESTAMP`).
    Timestamp,
    /// Les octets d'une représentation.
    Data(&'a Representation),
    /// Rien à répondre : type inconnu, ou trop gros pour une requête X.
    Refuse,
}

/// Ce que vaut la demande du type `target`. `offered` associe chaque atome de
/// type MIME à sa représentation, dans l'ordre de préférence ; `targets` et
/// `timestamp` sont les atomes `TARGETS` et `TIMESTAMP` ; `max_bytes` la
/// taille maximale d'une propriété qu'une requête peut porter.
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

/// Les atomes dont le propriétaire a besoin.
struct Atoms {
    clipboard: u32,
    targets: u32,
    timestamp: u32,
    scratch: u32,
}

fn backend<E: std::fmt::Display>(error: E) -> ClipboardError {
    ClipboardError::Backend(error.to_string())
}

/// Prend la sélection `CLIPBOARD` et sert `representations` depuis un fil, tant
/// qu'on la possède.
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

    // Une sélection se prend « à une heure du serveur », pas à `CurrentTime` :
    // un changement de propriété vide en rend une par `PropertyNotify`.
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

/// Répond aux demandes jusqu'à la perte de la sélection ou de la connexion.
fn serve(
    conn: &RustConnection,
    window: u32,
    atoms: &Atoms,
    offered: &[(u32, Representation)],
    time: u32,
) {
    // Un en-tête de requête et ses marges : une propriété doit tenir dans le
    // reste, sans quoi la requête serait refusée par le serveur.
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

/// Écrit la réponse sur la fenêtre demandeuse ; rend la propriété à annoncer
/// (`NONE` — 0 — quand on refuse).
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
    // ICCCM §2.2 : un demandeur ancien laisse la propriété vide, la cible en tient lieu.
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
    fn targets_liste_les_types_dans_l_ordre_de_preference() {
        let offered = offered([10, 10, 10]);
        let reponse = answer(TARGETS, &offered, TARGETS, TIMESTAMP, 100);
        assert_eq!(
            reponse,
            Answer::Targets(vec![TARGETS, TIMESTAMP, SVG, PNG, PDF])
        );
    }

    #[test]
    fn un_type_trop_gros_n_est_ni_liste_ni_servi() {
        // Le PNG dépasse la requête X : il manque de la liste et la demande
        // directe est refusée ; le SVG et le PDF restent offerts (témoin :
        // chacun répond bien avant d'être trop gros).
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
    fn timestamp_et_type_inconnu() {
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
