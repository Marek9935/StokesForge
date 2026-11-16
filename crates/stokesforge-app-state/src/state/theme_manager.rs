//! The theme manager of the Theme tab of Graph Properties (#542, S5 of #514):
//! what the selected theme can do (Rename, Update From This Graph, Reset,
//! Delete, Use for new graphs), each greyed with its reason; the Rename Theme
//! window; the confirmation before a user theme is deleted; the default theme
//! of new graphs seen from the Preferences.
//!
//! **Where each change lives.** A user theme or an edited built-in lives in the
//! library of this machine ([`crate::user_themes`]): outside the history, so a
//! delete asks first. A project theme lives in the project
//! ([`Command::SetProjectThemes`]): undoable, so a delete asks nothing. Opened
//! from Graph Properties, a project change joins the macro of the window: its
//! Cancel takes it back, like every other change made there.
//!
//! **The graph is the draft.** Save Theme As, Update From This Graph and the
//! rest read the style of the graph as the open Graph Properties draft holds
//! it ([`AppState::spec_for_theme`]), never the model alone: what the user sees
//! is what is saved, committed or not.

use stokesforge_commands::Command;
use stokesforge_core::NodeId;
use stokesforge_plot::{same_theme_name, GraphTheme, PlotSpec, MAX_THEME_NAME_CHARS};

use super::AppState;
use crate::theme_window::{ApplyThemeWindow, RenameThemeWindow, ThemeButtons, ThemeSource};
use crate::tr;
use crate::user_themes::{ThemeRef, UserThemeError};

impl AppState {
    /// The theme selected in the picker of the Theme tab.
    pub fn theme_selection(&self) -> &ThemeSource {
        &self.theme_selection
    }

    /// Selects a theme in the picker of the Theme tab.
    pub fn select_theme(&mut self, source: ThemeSource) {
        self.theme_selection = source;
    }

    /// The Rename Theme window, while it is open.
    pub fn rename_theme_window(&self) -> Option<&RenameThemeWindow> {
        self.rename_theme_window.as_ref()
    }

    /// The Rename Theme window, to change the name being typed.
    pub fn rename_theme_window_mut(&mut self) -> Option<&mut RenameThemeWindow> {
        self.rename_theme_window.as_mut()
    }

    /// The user theme whose deletion waits for its confirmation.
    pub fn pending_theme_delete(&self) -> Option<&ThemeRef> {
        self.pending_theme_delete.as_ref()
    }

    /// The name of a theme of the library or of the project, as listed.
    pub fn theme_display_name(&self, theme: &ThemeRef) -> String {
        match theme {
            ThemeRef::Builtin(index) => crate::builtin_themes::display_name(*index),
            _ => self
                .theme_of(&ThemeSource::Theme(theme.clone()))
                .map(|theme| theme.name)
                .unwrap_or_default(),
        }
    }

    /// The style of `graph` as the user sees it: the open Graph Properties
    /// draft when it is on `graph`, the model otherwise.
    pub(super) fn spec_for_theme(&self, graph: NodeId) -> Option<PlotSpec> {
        self.plot_edit
            .as_ref()
            .filter(|edit| edit.graph == graph)
            .map(|edit| edit.spec.clone())
            .or_else(|| self.plot_spec(graph))
    }

    /// What the Theme tab offers for `source` (see [`ThemeButtons`]).
    pub fn theme_buttons(&self, source: &ThemeSource) -> ThemeButtons {
        let copied = || tr!("disabled-theme-copied");
        let library = || {
            self.user_themes_read_only()
                .map(|_| tr!("theme-save-mine-read-only"))
        };
        let reference = match source {
            ThemeSource::Copied => {
                return ThemeButtons {
                    rename: Some(copied()),
                    update: Some(copied()),
                    reset: Some(copied()),
                    delete: Some(copied()),
                    use_for_new: Some(copied()),
                }
            }
            ThemeSource::Theme(reference) => reference,
        };
        let in_library = !matches!(reference, ThemeRef::Project(_));
        let library_reason = in_library.then(library).flatten();
        ThemeButtons {
            rename: match reference {
                ThemeRef::Builtin(_) => Some(tr!("disabled-theme-rename-builtin")),
                ThemeRef::User(_) => library_reason.clone(),
                ThemeRef::Project(_) => None,
            },
            update: library_reason.clone(),
            reset: match reference {
                ThemeRef::Builtin(index) => {
                    if !self.user_themes.is_modified(*index) {
                        Some(tr!("disabled-theme-reset-unchanged"))
                    } else {
                        library_reason.clone()
                    }
                }
                _ => Some(tr!("disabled-theme-reset-own")),
            },
            delete: match reference {
                ThemeRef::Builtin(_) => Some(tr!("disabled-theme-delete-builtin")),
                ThemeRef::User(_) => library_reason,
                ThemeRef::Project(_) => None,
            },
            use_for_new: matches!(reference, ThemeRef::Project(_))
                .then(|| tr!("disabled-theme-new-project")),
        }
    }

    /// The choices of the « Default theme » setting: none, then the built-ins
    /// and the user's themes (a project theme cannot be the default).
    pub fn default_theme_choices(&self) -> Vec<(Option<ThemeRef>, String)> {
        let mut choices = vec![(None, tr!("theme-default-none"))];
        choices.extend(self.user_themes.entries().into_iter().map(|entry| {
            let name = match &entry.reference {
                ThemeRef::Builtin(index) => crate::builtin_themes::display_name(*index),
                _ => entry.theme.name.clone(),
            };
            (Some(entry.reference), name)
        }));
        choices
    }

    /// Sets the default theme of new graphs (« Use for new graphs » and the
    /// Preferences row: one setting). A failed write is said.
    pub(super) fn set_default_theme(&mut self, theme: Option<ThemeRef>) {
        if let Err(error) = self.set_new_graph_theme(theme) {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = crate::errors::io::error_message(&error)
            ));
        }
    }

    /// Opens the Rename Theme window on a user or project theme.
    pub(super) fn open_rename_theme(&mut self, theme: ThemeRef) {
        if matches!(theme, ThemeRef::Builtin(_)) {
            return;
        }
        let name = self.theme_display_name(&theme);
        if name.is_empty() {
            return;
        }
        self.rename_theme_window = Some(RenameThemeWindow { theme, name });
    }

    /// Why `name` cannot rename the project theme `old`, if it cannot.
    fn project_rename_refusal(&self, old: &str, name: &str) -> Option<UserThemeError> {
        if name.is_empty() {
            return Some(UserThemeError::EmptyName);
        }
        self.snapshot
            .tree
            .project_themes()
            .iter()
            .any(|theme| !same_theme_name(&theme.name, old) && same_theme_name(&theme.name, name))
            .then_some(UserThemeError::DuplicateName)
    }

    /// Renames a user or project theme: the use count follows the theme, the
    /// selection too. A refused name says why and the window stays open.
    pub(super) fn rename_theme(&mut self, theme: ThemeRef, name: String) {
        let name: String = name.trim().chars().take(MAX_THEME_NAME_CHARS).collect();
        let done: Result<ThemeRef, UserThemeError> = match &theme {
            ThemeRef::Builtin(_) => return,
            ThemeRef::User(id) => self.rename_user_theme(*id, &name).map(|()| theme.clone()),
            ThemeRef::Project(old) => {
                if let Some(refusal) = self.project_rename_refusal(old, &name) {
                    Err(refusal)
                } else {
                    let mut themes = self.snapshot.tree.project_themes().to_vec();
                    let Some(kept) = themes
                        .iter_mut()
                        .find(|kept| same_theme_name(&kept.name, old))
                    else {
                        self.fail(tr!("error-theme-unknown"));
                        return;
                    };
                    kept.name = name.clone();
                    self.execute(Command::SetProjectThemes { themes });
                    let renamed = ThemeRef::project(&name);
                    // The count is kept under the new name: ignored when the
                    // library is read-only (the ranking only loses a count).
                    let _ = self.user_themes.move_use(&theme, &renamed);
                    Ok(renamed)
                }
            }
        };
        match done {
            Ok(renamed) => {
                if self.theme_selection == ThemeSource::Theme(theme) {
                    self.theme_selection = ThemeSource::Theme(renamed);
                }
                self.rename_theme_window = None;
                self.notice(tr!("status-theme-renamed", name = name));
            }
            Err(error) => self.fail(crate::errors::theme::error_message(&error)),
        }
    }

    /// Update From This Graph: the style of `graph` (the draft) replaces that
    /// of the theme; an edited built-in keeps its name and can be reset.
    pub(super) fn update_theme(&mut self, theme: ThemeRef, graph: NodeId) {
        let name = self.theme_display_name(&theme);
        let done = match &theme {
            ThemeRef::Builtin(index) => self.edit_builtin_theme(*index, graph),
            ThemeRef::User(id) => self.overwrite_user_theme(*id, graph),
            ThemeRef::Project(old) => {
                let Some(spec) = self.spec_for_theme(graph) else {
                    return;
                };
                let mut themes = self.snapshot.tree.project_themes().to_vec();
                if let Some(kept) = themes
                    .iter_mut()
                    .find(|kept| same_theme_name(&kept.name, old))
                {
                    *kept = GraphTheme::capture(kept.name.clone(), &spec);
                    self.execute(Command::SetProjectThemes { themes });
                    Ok(())
                } else {
                    Err(UserThemeError::Unknown)
                }
            }
        };
        match done {
            Ok(()) => self.notice(tr!("status-theme-updated", name = name)),
            Err(error) => self.fail(crate::errors::theme::error_message(&error)),
        }
    }

    /// Reset: an edited built-in gets its original style back.
    pub(super) fn reset_theme(&mut self, theme: ThemeRef) {
        let ThemeRef::Builtin(index) = theme else {
            return;
        };
        let name = self.theme_display_name(&ThemeRef::Builtin(index));
        match self.reset_builtin_theme(index) {
            Ok(()) => self.notice(tr!("status-theme-reset", name = name)),
            Err(error) => self.fail(crate::errors::theme::error_message(&error)),
        }
    }

    /// Delete: a project theme goes at once (one undo step, nothing to
    /// confirm); a user theme waits for its confirmation, the library being
    /// outside the history. A built-in is never deleted.
    pub(super) fn delete_theme(&mut self, theme: ThemeRef) {
        match &theme {
            ThemeRef::Builtin(_) => {}
            ThemeRef::User(_) => {
                if self.user_themes.get(&theme).is_some() {
                    self.pending_theme_delete = Some(theme);
                }
            }
            ThemeRef::Project(old) => {
                let name = self.theme_display_name(&theme);
                let mut themes = self.snapshot.tree.project_themes().to_vec();
                let before = themes.len();
                themes.retain(|kept| !same_theme_name(&kept.name, old));
                if themes.len() != before {
                    self.execute(Command::SetProjectThemes { themes });
                    self.theme_gone(&theme, name);
                }
            }
        }
    }

    /// The confirmation of a user theme's deletion.
    pub(super) fn confirm_delete_theme(&mut self) {
        let Some(theme) = self.pending_theme_delete.take() else {
            return;
        };
        let ThemeRef::User(id) = theme else {
            return;
        };
        let name = self.theme_display_name(&theme);
        match self.delete_user_theme(id) {
            Ok(()) => self.theme_gone(&theme, name),
            Err(error) => self.fail(crate::errors::theme::error_message(&error)),
        }
    }

    /// After a deletion: the selection falls back to Light, the status says it.
    fn theme_gone(&mut self, theme: &ThemeRef, name: String) {
        if self.theme_selection == ThemeSource::Theme(theme.clone()) {
            self.theme_selection = ThemeSource::Theme(ThemeRef::Builtin(0));
        }
        self.notice(tr!("status-theme-deleted", name = name));
    }

    /// Apply Theme… from the Theme tab: the window previews and applies into
    /// the open draft of `graph`; the theme chosen is the selected one.
    pub(super) fn open_apply_theme_into_draft(&mut self, graph: NodeId) {
        if !self.is_graph(graph) {
            return;
        }
        let chosen = self.theme_selection.clone();
        self.apply_theme_window = Some(ApplyThemeWindow::new_into_draft(graph, chosen));
    }
}
