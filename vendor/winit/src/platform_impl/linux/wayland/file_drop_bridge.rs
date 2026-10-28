//! Transition #72: drop received by the clipboard, handed to the winit loop.
//!
//! A second wl_data_device can steal the drop from the first on Mutter.
//! No Wayland object crosses this channel: surface identifier, position
//! and paths only. The weak callback stops working when the loop stops,
//! even if the display address is reused afterwards.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use calloop::channel::{self, Channel, Sender};

/// Transitional event #72 produced by the clipboard receiver.
#[derive(Debug, PartialEq)]
pub enum FileDropEvent {
    Entered {
        surface: u32,
        position: (f64, f64),
    },
    Motion {
        surface: u32,
        position: (f64, f64),
    },
    Left {
        surface: u32,
    },
    Dropped {
        surface: u32,
        position: (f64, f64),
        paths: Vec<PathBuf>,
    },
}

type Routes = HashMap<usize, Weak<Sender<FileDropEvent>>>;
fn routes() -> &'static Mutex<Routes> {
    static ROUTES: OnceLock<Mutex<Routes>> = OnceLock::new();
    ROUTES.get_or_init(Mutex::default)
}

/// Owns neither the display nor the loop; may be called from the clipboard thread.
#[derive(Clone)]
pub struct FileDropCallback(Weak<Sender<FileDropEvent>>);

impl FileDropCallback {
    /// False if the loop was destroyed. No native pointer is dereferenced.
    pub fn send(&self, event: FileDropEvent) -> bool {
        self.0
            .upgrade()
            .is_some_and(|sender| sender.send(event).is_ok())
    }
}

/// Looks up the loop by the wl_display address token borrowed by egui.
/// The address serves only as a key; the callback holds a weak reference.
pub fn file_drop_callback(display: usize) -> Option<FileDropCallback> {
    let routes = routes().lock().ok()?;
    let sender = routes.get(&display)?;
    sender.upgrade()?;
    Some(FileDropCallback(sender.clone()))
}

pub(crate) struct Registration {
    display: usize,
    sender: Arc<Sender<FileDropEvent>>,
}

pub(crate) fn register(display: usize) -> (Registration, Channel<FileDropEvent>) {
    let (sender, receiver) = channel::channel();
    let sender = Arc::new(sender);
    routes()
        .lock()
        .unwrap()
        .insert(display, Arc::downgrade(&sender));
    (Registration { display, sender }, receiver)
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut routes) = routes().lock() {
            if routes
                .get(&self.display)
                .is_some_and(|route| route.ptr_eq(&Arc::downgrade(&self.sender)))
            {
                routes.remove(&self.display);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn le_recepteur_partage_transmet_les_depots_des_deux_fenetres() {
        let mut event_loop = calloop::EventLoop::<Vec<FileDropEvent>>::try_new().unwrap();
        let (registration, channel) = register(1001);
        event_loop
            .handle()
            .insert_source(channel, |event, _, events| {
                if let channel::Event::Msg(event) = event {
                    events.push(event);
                }
            })
            .unwrap();
        let root = file_drop_callback(1001).unwrap();
        let detached = file_drop_callback(1001).unwrap();
        let mut events = Vec::new();
        for (callback, surface, file) in
            [(&root, 30, "racine.csv"), (&detached, 120, "détaché.csv")]
        {
            assert!(callback.send(FileDropEvent::Entered {
                surface,
                position: (10.5, 20.5)
            }));
            assert!(callback.send(FileDropEvent::Motion {
                surface,
                position: (12.5, 22.5)
            }));
            assert!(callback.send(FileDropEvent::Dropped {
                surface,
                position: (15.5, 25.5),
                paths: vec![file.into()]
            }));
        }
        event_loop.dispatch(Duration::ZERO, &mut events).unwrap();
        assert_eq!(
            events,
            vec![
                FileDropEvent::Entered {
                    surface: 30,
                    position: (10.5, 20.5)
                },
                FileDropEvent::Motion {
                    surface: 30,
                    position: (12.5, 22.5)
                },
                FileDropEvent::Dropped {
                    surface: 30,
                    position: (15.5, 25.5),
                    paths: vec!["racine.csv".into()]
                },
                FileDropEvent::Entered {
                    surface: 120,
                    position: (10.5, 20.5)
                },
                FileDropEvent::Motion {
                    surface: 120,
                    position: (12.5, 22.5)
                },
                FileDropEvent::Dropped {
                    surface: 120,
                    position: (15.5, 25.5),
                    paths: vec!["détaché.csv".into()]
                },
            ]
        );
        drop(registration);
    }

    #[test]
    fn arret_et_reutilisation_du_display_ne_reaniment_pas_un_callback() {
        let (first, _channel) = register(1002);
        let old = file_drop_callback(1002).unwrap();
        drop(first);
        assert!(file_drop_callback(1002).is_none());
        let (second, _channel) = register(1002);
        assert!(!old.send(FileDropEvent::Left { surface: 30 }));
        assert!(file_drop_callback(1002)
            .unwrap()
            .send(FileDropEvent::Left { surface: 40 }));
        drop(second);
    }
}
