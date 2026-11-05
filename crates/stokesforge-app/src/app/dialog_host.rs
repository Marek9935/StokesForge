//! The host of the application: the one object eframe drives (#457).
//!
//! `AppHost` shares the [`StokesForgeApp`] through `Rc<RefCell<_>>` so that the
//! closure of a deferred viewport, which must be `'static + Send + Sync` and
//! captures nothing of the application, can reach it again on the main thread
//! through a `thread_local!` `Weak` ([`with_app`]). Every closure runs on the
//! main thread, from the pass of its own viewport and never inside the root's
//! `ui` in production; `try_borrow_mut` makes any nesting (embedded viewports
//! in tests) a no-op instead of a panic. No `unsafe`.
//!
//! Without an `AppHost` (most tests) the `Weak` is dead and [`with_app`] does
//! nothing. This module holds the host only; no behaviour hangs on it yet.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use eframe::egui;

use super::StokesForgeApp;

thread_local! {
    static APP: RefCell<Weak<RefCell<StokesForgeApp>>> = const { RefCell::new(Weak::new()) };
}

/// What eframe drives: forwards the three overrides of [`StokesForgeApp`].
pub struct AppHost(Rc<RefCell<StokesForgeApp>>);

impl AppHost {
    /// Wraps `app` and publishes it to the closures of this thread.
    #[must_use]
    pub fn new(app: StokesForgeApp) -> Self {
        let shared = Rc::new(RefCell::new(app));
        APP.with(|cell| *cell.borrow_mut() = Rc::downgrade(&shared));
        Self(shared)
    }
}

/// Runs `f` on the application when the call comes from the main thread of a
/// live [`AppHost`] and the application is not already borrowed; otherwise does
/// nothing and returns `None`.
#[cfg_attr(not(test), allow(dead_code))] // used from the next lot on
pub(super) fn with_app<R>(f: impl FnOnce(&mut StokesForgeApp) -> R) -> Option<R> {
    let shared = APP.with(|cell| cell.borrow().upgrade())?;
    let mut app = shared.try_borrow_mut().ok()?;
    Some(f(&mut app))
}

impl eframe::App for AppHost {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.logic(ctx, frame);
        }
    }

    fn on_exit(&mut self) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.on_exit();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.ui(ui, frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a live host the helper does nothing; with one it reaches the
    /// application, and a nested call (already borrowed) is a no-op, no panic.
    /// Witness: `borrow_mut` in `with_app`.
    #[test]
    fn the_helper_reaches_a_live_host_and_ignores_nesting() {
        use stokesforge_app_state::state::AppState;
        use stokesforge_core::Project;
        use stokesforge_session::Session;
        assert!(with_app(|_| ()).is_none(), "no host yet");
        let session = Session::spawn(Project::new("Host").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let host = AppHost::new(StokesForgeApp::new(state));
        assert_eq!(with_app(|_| 1), Some(1));
        let nested = with_app(|_| with_app(|_| 2));
        assert_eq!(nested, Some(None), "the nested call is skipped");
        drop(host);
        assert!(with_app(|_| ()).is_none(), "a dropped host is gone");
        session.shutdown().unwrap();
    }
}
