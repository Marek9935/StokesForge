//! Smithay Clipboard
//!
//! Provides access to the Wayland clipboard for gui applications. The user
//! should have surface around.

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::Result;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use sctk::reexports::calloop::channel::{self, Sender};
use sctk::reexports::client::Connection;
use sctk::reexports::client::backend::Backend;

mod file_drop;
mod mime;
mod state;
mod worker;

/// Access to a Wayland clipboard.
pub struct Clipboard {
    // Un seul worker/récepteur par display, partagé entre les viewports d'egui.
    inner: Arc<Mutex<ClipboardInner>>,
}

struct ClipboardInner {
    request_sender: Sender<worker::Command>,
    request_receiver: Receiver<Result<String>>,
    clipboard_thread: Option<std::thread::JoinHandle<()>>,
}

impl Clipboard {
    /// Creates new clipboard which will be running on its own thread with its
    /// own event queue to handle clipboard requests.
    ///
    /// # Safety
    ///
    /// `display` must be a valid `*mut wl_display` pointer, and it must remain
    /// valid for as long as `Clipboard` object is alive.
    pub unsafe fn new(display: *mut c_void) -> Self {
        type Shared = Weak<Mutex<ClipboardInner>>;
        static CLIPBOARDS: OnceLock<Mutex<HashMap<usize, Shared>>> = OnceLock::new();
        let mut clipboards = CLIPBOARDS.get_or_init(Mutex::default).lock().unwrap();
        clipboards.retain(|_, clipboard| clipboard.strong_count() > 0);
        let key = display as usize;
        if let Some(inner) = clipboards.get(&key).and_then(Weak::upgrade) {
            return Self { inner };
        }
        let file_drop = winit::platform::wayland::file_drop_callback(key);
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);

        // Create channel to send data to clipboard thread.
        let (request_sender, rx_chan) = channel::channel();
        // Create channel to get data from the clipboard thread.
        let (clipboard_reply_sender, request_receiver) = mpsc::channel();

        let name = String::from("smithay-clipboard");
        let clipboard_thread =
            worker::spawn(name, connection, rx_chan, clipboard_reply_sender, file_drop);

        let inner = Arc::new(Mutex::new(ClipboardInner {
            request_receiver,
            request_sender,
            clipboard_thread,
        }));
        clipboards.insert(key, Arc::downgrade(&inner));
        Self { inner }
    }

    /// Load clipboard data.
    ///
    /// Loads content from a clipboard on a last observed seat.
    pub fn load(&self) -> Result<String> {
        let inner = self.inner.lock().unwrap();
        let _ = inner.request_sender.send(worker::Command::Load);

        if let Ok(reply) = inner.request_receiver.recv() {
            reply
        } else {
            // The clipboard thread is dead, however we shouldn't crash downstream, so
            // propogating an error.
            Err(std::io::Error::other("clipboard is dead."))
        }
    }

    /// Store to a clipboard.
    ///
    /// Stores to a clipboard on a last observed seat.
    pub fn store<T: Into<String>>(&self, text: T) {
        let request = worker::Command::Store(text.into());
        let _ = self.inner.lock().unwrap().request_sender.send(request);
    }

    /// Load primary clipboard data.
    ///
    /// Loads content from a  primary clipboard on a last observed seat.
    pub fn load_primary(&self) -> Result<String> {
        let inner = self.inner.lock().unwrap();
        let _ = inner.request_sender.send(worker::Command::LoadPrimary);

        if let Ok(reply) = inner.request_receiver.recv() {
            reply
        } else {
            // The clipboard thread is dead, however we shouldn't crash downstream, so
            // propogating an error.
            Err(std::io::Error::other("clipboard is dead."))
        }
    }

    /// Store to a primary clipboard.
    ///
    /// Stores to a primary clipboard on a last observed seat.
    pub fn store_primary<T: Into<String>>(&self, text: T) {
        let request = worker::Command::StorePrimary(text.into());
        let _ = self.inner.lock().unwrap().request_sender.send(request);
    }
}

impl Drop for ClipboardInner {
    fn drop(&mut self) {
        // Shutdown smithay-clipboard.
        let _ = self.request_sender.send(worker::Command::Exit);
        if let Some(clipboard_thread) = self.clipboard_thread.take() {
            let _ = clipboard_thread.join();
        }
    }
}
