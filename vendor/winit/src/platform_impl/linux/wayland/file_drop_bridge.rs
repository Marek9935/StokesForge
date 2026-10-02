//! Transition #72 : dépôt reçu par le presse-papiers, remis à la boucle winit.
//!
//! Un deuxième wl_data_device peut voler le dépôt au premier sur Mutter.
//! Aucun objet Wayland ne traverse ce canal : identifiant de surface, position
//! et chemins seulement. Le callback faible cesse de fonctionner à l'arrêt
//! de la boucle, même si l'adresse du display est ensuite réutilisée.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use calloop::channel::{self, Channel, Sender};

/// Événement transitoire #72 produit par le récepteur du presse-papiers.
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

/// Ne possède ni le display ni la boucle ; peut être appelé depuis le fil du presse-papiers.
#[derive(Clone)]
pub struct FileDropCallback(Weak<Sender<FileDropEvent>>);

impl FileDropCallback {
    /// Faux si la boucle a été détruite. Aucun pointeur natif n'est déréférencé.
    pub fn send(&self, event: FileDropEvent) -> bool {
        self.0
            .upgrade()
            .is_some_and(|sender| sender.send(event).is_ok())
    }
}

/// Recherche la boucle par le jeton d'adresse wl_display emprunté par egui.
/// L'adresse sert uniquement de clé ; le callback possède une référence faible.
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
