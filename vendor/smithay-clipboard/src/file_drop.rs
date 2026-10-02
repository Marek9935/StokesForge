//! Transition StokesForge #72 : le même wl_data_device reçoit sélection et dépôt.
//!
//! Mutter envoie le dépôt à un seul récepteur du client. Créer un récepteur
//! dans winit puis un autre dans le presse-papiers perd donc les offres.
//! Ce module réutilise le fil et les sièges du presse-papiers ; un canal
//! faible remet les événements à winit, sans transporter d'objet Wayland.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::rc::Rc;
use std::time::Duration;

use sctk::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::reexports::calloop::PostAction;
use sctk::reexports::calloop::timer::{TimeoutAction, Timer};
use sctk::reexports::client::backend::ObjectId;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};
use winit::platform::wayland::{FileDropCallback, FileDropEvent};

use crate::state::State;
#[path = "file_drop_uri.rs"]
mod uri_list;

const URI_LIST: &str = "text/uri-list";
const MAX_URI_BYTES: usize = 1024 * 1024;

pub(crate) struct FileDrops {
    callback: Option<FileDropCallback>,
    transfers: HashMap<ObjectId, Rc<RefCell<Transfer>>>,
}

impl FileDrops {
    pub(crate) fn new(callback: Option<FileDropCallback>) -> Self {
        Self {
            callback,
            transfers: HashMap::new(),
        }
    }

    pub(crate) fn remove_device(&mut self, device: &WlDataDevice) {
        if let Some(transfer) = self.transfers.remove(&device.id()) {
            transfer.borrow_mut().cancelled = true;
        }
    }
}

struct Transfer {
    offer: DragOffer,
    callback: FileDropCallback,
    dropped: bool,
    cancelled: bool,
    completed: bool,
}

impl Transfer {
    fn left(&self) {
        self.callback.send(FileDropEvent::Left {
            surface: self.offer.surface.id().protocol_id(),
        });
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.completed = true;
        self.left();
        if self.offer.inner().is_alive() {
            self.offer.destroy();
        }
    }

    fn publish(&mut self, bytes: &[u8]) {
        if self.cancelled || self.completed {
            return;
        }
        self.callback.send(FileDropEvent::Dropped {
            surface: self.offer.surface.id().protocol_id(),
            position: (self.offer.x, self.offer.y),
            paths: uri_list::local_files(bytes),
        });
        // Seule Copy a été annoncée ; finish est permis après le Drop.
        if self.offer.inner().is_alive() {
            self.offer.finish();
            self.offer.destroy();
        }
        self.completed = true;
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let Some(offer) = device
            .data::<DataDeviceData>()
            .and_then(DataDeviceData::drag_offer)
        else {
            return;
        };
        let callback = self.file_drops.callback.clone();
        let accepted = callback.is_some()
            && offer.with_mime_types(|types| types.iter().any(|mime| mime == URI_LIST));
        offer.accept_mime_type(offer.serial, accepted.then(|| URI_LIST.to_owned()));
        if !accepted {
            return;
        }
        let callback = callback.unwrap();
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        callback.send(FileDropEvent::Entered {
            surface: surface.id().protocol_id(),
            position: (x, y),
        });
        self.file_drops.transfers.insert(
            device.id(),
            Rc::new(RefCell::new(Transfer {
                offer,
                callback,
                dropped: false,
                cancelled: false,
                completed: false,
            })),
        );
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        if let Some(transfer) = self.file_drops.transfers.get(&device.id()) {
            let transfer = transfer.borrow();
            transfer.callback.send(FileDropEvent::Motion {
                surface: transfer.offer.surface.id().protocol_id(),
                position: (x, y),
            });
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        if let Some(transfer) = self.file_drops.transfers.remove(&device.id()) {
            let mut transfer = transfer.borrow_mut();
            if !transfer.dropped {
                transfer.cancelled = true;
            }
            transfer.left();
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        // Les offres du presse-papiers restent traitées par load_selection.
    }

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(transfer) = self.file_drops.transfers.get(&device.id()).cloned() else {
            return;
        };
        let Some(current) = device
            .data::<DataDeviceData>()
            .and_then(DataDeviceData::drag_offer)
        else {
            return;
        };
        {
            let mut transfer = transfer.borrow_mut();
            transfer.offer = current;
            if transfer.offer.inner().version() >= 3
                && transfer.offer.selected_action != DndAction::Copy
            {
                transfer.cancel();
                return;
            }
            transfer.dropped = true;
            transfer.left();
        }
        // Réception après Drop : les sources qui ne fournissent rien au survol
        // fonctionnent aussi. Un seul read par readiness, jamais read_to_end.
        let received = transfer.borrow().offer.receive(URI_LIST.to_owned());
        let pipe = match received {
            Ok(pipe) => pipe,
            Err(error) => {
                eprintln!("[stokesforge-wayland] réception du dépôt : {error}");
                transfer.borrow_mut().cancel();
                return;
            }
        };
        let reader_transfer = transfer.clone();
        let mut bytes = Vec::new();
        let token = match self
            .loop_handle
            .insert_source(pipe, move |_, file, _state| {
                let mut buffer = [0; 8192];
                let mut input = &**file;
                match input.read(&mut buffer) {
                    Ok(0) => {
                        reader_transfer.borrow_mut().publish(&bytes);
                        PostAction::Remove
                    }
                    Ok(count) if bytes.len() + count <= MAX_URI_BYTES => {
                        bytes.extend_from_slice(&buffer[..count]);
                        PostAction::Continue
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => PostAction::Continue,
                    _ => {
                        reader_transfer.borrow_mut().cancel();
                        eprintln!("[stokesforge-wayland] liste d'URI illisible ou trop grande");
                        PostAction::Remove
                    }
                }
            }) {
            Ok(token) => token,
            Err(error) => {
                transfer.borrow_mut().cancel();
                eprintln!("[stokesforge-wayland] pipe du dépôt : {error}");
                return;
            }
        };
        let timer_transfer = transfer.clone();
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_secs(10)),
            move |_, _, state| {
                let mut transfer = timer_transfer.borrow_mut();
                if !transfer.completed {
                    transfer.cancel();
                    state.loop_handle.remove(token);
                    eprintln!("[stokesforge-wayland] délai de réception du dépôt dépassé");
                }
                TimeoutAction::Drop
            },
        ) {
            self.loop_handle.remove(token);
            transfer.borrow_mut().cancel();
            eprintln!("[stokesforge-wayland] délai du dépôt : {error}");
        }
    }
}
