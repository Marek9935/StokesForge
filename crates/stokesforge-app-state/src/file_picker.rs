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
//! The application's pickers go through [`PickRequest`], pushed as
//! [`crate::state::Action::PickFile`]; `StokesForgeApp` starts it and applies
//! the answer. Only one picker is *tracked* at a time: a new request
//! **replaces** the previous one. Without that, a picker closed without the
//! portal answering would stay "pending" forever and prevent opening another
//! (follow-up of #192); its thread, blocked in `rfd`, is abandoned and its
//! possible answer is dropped.
//!
//! Default folders (#148): a request knows its [`FolderKind`]
//! ([`PickRequest::folder_kind`]); the starting folder comes from
//! [`FolderCandidates`] (session memory, Preferences default, fallback). Looking
//! at the disk (`is_dir`) and `set_directory` happen **on the picker's thread**:
//! a folder on a sleeping network drive would otherwise freeze the interface
//! thread, the very problem this module exists to avoid.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use stokesforge_plot::PlotSpec;

use crate::default_folders::{FolderCandidates, FolderKind};
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
    /// Import wizard: files to import.
    AddImportFiles,
    /// Preferences › Folders: the default folder of a kind.
    ChooseDefaultFolder { kind: FolderKind },
}

impl PickRequest {
    /// The kind of folder this picker starts in. Exhaustive, without `_`: a new
    /// request must say which default folder it follows.
    pub fn folder_kind(&self) -> FolderKind {
        match self {
            Self::OpenProject | Self::SaveProjectAs { .. } => FolderKind::Projects,
            Self::ExportGraph { .. } => FolderKind::Export,
            Self::ExportImportRecipe { .. } | Self::ImportImportRecipe | Self::AddImportFiles => {
                FolderKind::Import
            }
            Self::ChooseDefaultFolder { kind } => *kind,
        }
    }

    /// Does the folder of the chosen file become the session's last folder of its
    /// kind? Every file dialog does; choosing a default folder is not a use of it.
    pub fn feeds_the_folder_memory(&self) -> bool {
        match self {
            Self::OpenProject
            | Self::SaveProjectAs { .. }
            | Self::ExportGraph { .. }
            | Self::ExportImportRecipe { .. }
            | Self::ImportImportRecipe
            | Self::AddImportFiles => true,
            Self::ChooseDefaultFolder { .. } => false,
        }
    }

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
            Self::AddImportFiles => dialog.set_title(tr!("import-files-add-title")),
            Self::ChooseDefaultFolder { .. } => {
                dialog.set_title(tr!("preferences-folders-pick-title"))
            }
        }
    }

    /// Builds the dialog (starting in `directory`, if any) and runs it: blocking,
    /// on the dedicated thread.
    fn show(&self, directory: Option<PathBuf>) -> Picked {
        let dialog = self.dialog();
        let dialog = match directory {
            Some(directory) => dialog.set_directory(directory),
            None => dialog,
        };
        self.run(dialog)
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
            Self::AddImportFiles => dialog.pick_files(),
            Self::ChooseDefaultFolder { .. } => dialog.pick_folder().map(|path| vec![path]),
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
    /// when the answer is ready. The starting folder is looked up (`is_dir`) on
    /// that thread, from `candidates`.
    pub fn start(
        &mut self,
        request: PickRequest,
        candidates: FolderCandidates,
        wake: impl Fn() + Send + 'static,
    ) {
        let worker = request.clone();
        self.start_resolving(
            request,
            candidates,
            wake,
            |folder| folder.is_dir(),
            move |directory| worker.show(directory),
        );
    }

    /// [`Self::start`] with the folder test and the picker's execution injected
    /// (tests): both run on the picker's thread, `show` receiving the starting
    /// folder, if any.
    fn start_resolving(
        &mut self,
        request: PickRequest,
        candidates: FolderCandidates,
        wake: impl Fn() + Send + 'static,
        is_dir: impl Fn(&Path) -> bool + Send + 'static,
        show: impl FnOnce(Option<PathBuf>) -> Picked + Send + 'static,
    ) {
        self.start_with(request, wake, move || show(candidates.resolve(is_dir)));
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

    /// Tests: tracks a picker that answers `answer` at once, without any dialog
    /// (never a real `rfd` in a test).
    #[cfg(any(test, feature = "test-support"))]
    pub fn start_fake(&mut self, request: PickRequest, answer: Picked) {
        self.start_with(request, || {}, move || answer);
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

    // -- Default folders (#148) --

    /// Every request follows one kind of folder (Open and Save As: projects;
    /// the exports: export; the recipe dialogs and the files to import: import);
    /// only choosing a default is not a use of a folder.
    #[test]
    fn each_request_follows_its_kind_of_folder() {
        let spec = || Box::new(PlotSpec::default());
        let cases = [
            (PickRequest::OpenProject, FolderKind::Projects, true),
            (
                PickRequest::SaveProjectAs {
                    suggested: "p.stf".into(),
                },
                FolderKind::Projects,
                true,
            ),
            (
                PickRequest::ExportGraph {
                    spec: spec(),
                    file_name: "g.svg".into(),
                },
                FolderKind::Export,
                true,
            ),
            (
                PickRequest::ExportImportRecipe {
                    file_name: "t.json".into(),
                },
                FolderKind::Import,
                true,
            ),
            (PickRequest::ImportImportRecipe, FolderKind::Import, true),
            (PickRequest::AddImportFiles, FolderKind::Import, true),
            (
                PickRequest::ChooseDefaultFolder {
                    kind: FolderKind::Export,
                },
                FolderKind::Export,
                false,
            ),
        ];
        for (request, kind, feeds) in cases {
            assert_eq!(request.folder_kind(), kind, "{request:?}");
            assert_eq!(request.feeds_the_folder_memory(), feeds, "{request:?}");
        }
    }

    fn wait_for(picker: &mut FilePicker) -> (PickRequest, Picked) {
        for _ in 0..500 {
            if let Some(answer) = picker.poll() {
                return answer;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the fake picker never answered");
    }

    /// The disk is looked at (`is_dir`) and the starting folder handed to the dialog
    /// **on the picker's thread**, never on the caller's (the interface's) thread.
    #[test]
    fn the_folder_is_looked_up_on_the_pickers_thread() {
        use std::sync::{Arc, Mutex};
        let caller = std::thread::current().id();
        let looked_up_on = Arc::new(Mutex::new(Vec::new()));
        let shown_in = Arc::new(Mutex::new(None));
        let mut picker = FilePicker::default();
        let (probe, shown) = (looked_up_on.clone(), shown_in.clone());
        picker.start_resolving(
            PickRequest::OpenProject,
            FolderCandidates {
                session_last: Some(PathBuf::from("/gone")),
                default: Some(PathBuf::from("/default")),
                fallback: None,
            },
            || {},
            move |folder| {
                probe.lock().unwrap().push(std::thread::current().id());
                folder == Path::new("/default")
            },
            move |directory| {
                *shown.lock().unwrap() = Some((std::thread::current().id(), directory));
                None
            },
        );
        let (request, picked) = wait_for(&mut picker);
        assert_eq!(request, PickRequest::OpenProject);
        assert_eq!(picked, None);
        let probes = looked_up_on.lock().unwrap().clone();
        assert!(!probes.is_empty());
        assert!(probes.iter().all(|thread| *thread != caller));
        let (thread, directory) = shown_in.lock().unwrap().clone().expect("shown");
        assert_ne!(thread, caller);
        assert_eq!(
            directory,
            Some(PathBuf::from("/default")),
            "the missing session folder is skipped"
        );
    }

    /// An empty default is never handed to the dialog as a directory.
    #[test]
    fn an_empty_default_is_never_set_as_the_directory() {
        let mut picker = FilePicker::default();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Some(Some(PathBuf::from("x")))));
        let out = seen.clone();
        picker.start_resolving(
            PickRequest::ImportImportRecipe,
            FolderCandidates {
                session_last: None,
                default: Some(PathBuf::new()),
                fallback: Some(PathBuf::new()),
            },
            || {},
            |_| true,
            move |directory| {
                *out.lock().unwrap() = Some(directory);
                None
            },
        );
        wait_for(&mut picker);
        assert_eq!(*seen.lock().unwrap(), Some(None));
    }
}
