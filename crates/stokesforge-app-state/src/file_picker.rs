//! Native file pickers off the interface thread (issue #192).
//!
//! `rfd` waits for the portal's answer (`xdg-desktop-portal`) in a loop with
//! no timeout: called on egui's thread, a picker closed without an answer
//! (a click on the application, under GNOME/Wayland) froze the whole
//! application. So the picker opens on a separate thread and returns its
//! choice through a channel; the interface polls this channel on every frame
//! ([`FilePicker::poll`]) and stays alive whatever happens — at worst a
//! request stays pending, never a frozen application.
//!
//! The application's seven pickers go through [`PickRequest`], pushed as
//! [`crate::state::Action::PickFile`]; `StokesForgeApp` starts it and applies
//! the answer. Only one picker is *tracked* at a time: a new request
//! **replaces** the previous one. Without that, a picker closed without the
//! portal answering would stay "pending" forever and prevent opening another
//! (follow-up of #192); its thread, blocked in `rfd`, is abandoned and its
//! possible answer is dropped.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use stokesforge_plot::PlotSpec;

use crate::tr;

/// What the user is asked to choose, with what is needed to apply the
/// answer.
#[derive(Debug, Clone, PartialEq)]
pub enum PickRequest {
    /// File › Open: a project to load.
    OpenProject,
    /// File › Save As: the project's destination (suggested name).
    SaveProjectAs { suggested: String },
    /// Graph export: spec already framed by the view, suggested name.
    ExportGraph {
        spec: Box<PlotSpec>,
        file_name: String,
    },
    /// Import wizard: save the recipe to a JSON file.
    ExportImportRecipe { file_name: String },
    /// Import wizard: read a JSON recipe.
    ImportImportRecipe,
    /// Import wizard: files to import (starting folder optional).
    AddImportFiles { directory: Option<PathBuf> },
}

impl PickRequest {
    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        match self {
            Self::OpenProject => dialog
                .set_title(tr!("project-open-title"))
                .add_filter(tr!("project-file-filter"), &["stf"]),
            Self::SaveProjectAs { suggested } => dialog
                .set_title(tr!("project-save-title"))
                .add_filter(tr!("project-file-filter"), &["stf"])
                .set_file_name(suggested),
            Self::ExportGraph { file_name, .. } => dialog
                .set_title(tr!("export-dialog-title"))
                .add_filter("SVG", &["svg"])
                .add_filter("PDF", &["pdf"])
                .add_filter("PNG", &["png"])
                .set_file_name(file_name),
            Self::ExportImportRecipe { file_name } => dialog
                .set_title(tr!("import-files-export-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"])
                .set_file_name(file_name),
            Self::ImportImportRecipe => dialog
                .set_title(tr!("import-files-import-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"]),
            Self::AddImportFiles { directory } => {
                let dialog = dialog.set_title(tr!("import-files-add-title"));
                match directory {
                    Some(dir) => dialog.set_directory(dir),
                    None => dialog,
                }
            }
        }
    }

    /// Runs the picker (blocking); to be called on the dedicated thread.
    fn run(&self, dialog: rfd::FileDialog) -> Option<Vec<PathBuf>> {
        match self {
            Self::SaveProjectAs { .. }
            | Self::ExportGraph { .. }
            | Self::ExportImportRecipe { .. } => dialog.save_file().map(|path| vec![path]),
            Self::OpenProject | Self::ImportImportRecipe => {
                dialog.pick_file().map(|path| vec![path])
            }
            Self::AddImportFiles { .. } => dialog.pick_files(),
        }
    }
}

/// A picker's answer: `None` if cancelled or closed without a choice.
pub type Picked = Option<Vec<PathBuf>>;

struct Pending {
    request: PickRequest,
    receiver: Receiver<Picked>,
}

/// The picker in progress, if there is one.
#[derive(Default)]
pub struct FilePicker {
    pending: Option<Pending>,
}

impl FilePicker {
    /// Is a picker open?
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    /// Opens the picker on a separate thread, abandoning the one that was
    /// tracked (perhaps dead on the portal side). `wake` wakes the interface
    /// when the answer is ready.
    pub fn start(&mut self, request: PickRequest, wake: impl Fn() + Send + 'static) {
        let dialog = request.dialog();
        let worker = request.clone();
        self.start_with(request, wake, move || worker.run(dialog));
    }

    /// [`Self::start`] with the picker's execution injected: tests pass a
    /// fake one, otherwise `rfd` would open a real portal picker (a window
    /// that would stay open after the test).
    fn start_with(
        &mut self,
        request: PickRequest,
        wake: impl Fn() + Send + 'static,
        run: impl FnOnce() -> Picked + Send + 'static,
    ) {
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(run());
            wake();
        });
        self.pending = Some(Pending { request, receiver });
    }

    /// The answer of the finished picker, once only. Never blocks; a dead
    /// thread with no answer counts as "cancelled".
    pub fn poll(&mut self) -> Option<(PickRequest, Picked)> {
        let pending = self.pending.as_ref()?;
        let picked = match pending.receiver.try_recv() {
            Ok(picked) => picked,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        let pending = self.pending.take()?;
        Some((pending.request, picked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Follow-up of #192: a picker closed without a portal answer must not
    /// prevent opening another one.
    #[test]
    fn a_new_request_replaces_a_picker_left_unanswered() {
        let (_sender, receiver) = channel::<Picked>();
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::OpenProject,
                receiver,
            }),
        };
        // Fake picker: never a real `rfd` in a test.
        picker.start_with(PickRequest::ImportImportRecipe, || {}, || None);
        let pending = picker.pending.as_ref().expect("tracked");
        assert_eq!(pending.request, PickRequest::ImportImportRecipe);
    }

    #[test]
    fn without_an_open_picker_polling_does_not_block() {
        let mut picker = FilePicker::default();
        assert!(!picker.is_open());
        assert!(picker.poll().is_none());
    }

    #[test]
    fn a_dead_thread_with_no_answer_counts_as_cancelled() {
        let (sender, receiver) = channel::<Picked>();
        drop(sender);
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::OpenProject,
                receiver,
            }),
        };
        let (request, picked) = picker.poll().expect("answer");
        assert_eq!(request, PickRequest::OpenProject);
        assert_eq!(picked, None);
        assert!(!picker.is_open());
    }

    #[test]
    fn a_pending_answer_leaves_the_caller_free() {
        // The picker has not answered: polling returns immediately.
        let (_sender, receiver) = channel::<Picked>();
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::ImportImportRecipe,
                receiver,
            }),
        };
        assert!(picker.poll().is_none());
        assert!(picker.is_open());
    }
}
