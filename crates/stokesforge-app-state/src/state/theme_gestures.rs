//! The graph-theme gestures of the state (#541): Copy Theme, Paste Theme, the
//! Apply Theme window, Save Theme As. The data and the pure rules are in
//! [`crate::theme_window`]; this file is what needs the session: the graphs,
//! the project themes, the library, the undoable apply of S1.

use stokesforge_core::NodeId;
use stokesforge_plot::{same_theme_name, GraphTheme, MAX_THEMES};

use super::AppState;
use crate::theme_window::{
    paste_families, ApplyPreview, ApplyThemeWindow, PickerEntries, PickerEntry, SaveThemeWindow,
    ThemeSource, ThemeTarget,
};
use crate::tr;
use crate::user_themes::{ThemeEntry, ThemeRef, UserThemeError};

impl AppState {
    /// The theme Copy Theme put aside, if any (the app's own clipboard).
    pub fn theme_clipboard(&self) -> Option<&GraphTheme> {
        self.theme_clipboard.as_ref()
    }

    /// The Apply Theme window, while it is open.
    pub fn apply_theme_window(&self) -> Option<&ApplyThemeWindow> {
        self.apply_theme_window.as_ref()
    }

    /// The Apply Theme window, to change what the user clicked or hovers.
    pub fn apply_theme_window_mut(&mut self) -> Option<&mut ApplyThemeWindow> {
        self.apply_theme_window.as_mut()
    }

    /// The Save Theme As window, while it is open.
    pub fn save_theme_window(&self) -> Option<&SaveThemeWindow> {
        self.save_theme_window.as_ref()
    }

    /// The Save Theme As window, to change the name and the target.
    pub fn save_theme_window_mut(&mut self) -> Option<&mut SaveThemeWindow> {
        self.save_theme_window.as_mut()
    }

    /// True if `node` is a graph of the project.
    pub fn is_graph(&self, node: NodeId) -> bool {
        self.snapshot
            .tree
            .node(node)
            .is_some_and(|n| matches!(n.kind(), stokesforge_core::NodeKind::Graph { .. }))
    }

    /// `ids` reduced to the graphs, once each, in order: the target of a theme
    /// gesture (folders and their contents are not included).
    pub fn graphs_among(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut seen = std::collections::HashSet::new();
        ids.iter()
            .copied()
            .filter(|id| self.is_graph(*id) && seen.insert(*id))
            .collect()
    }

    /// The theme of `source`, if it still exists.
    pub fn theme_of(&self, source: &ThemeSource) -> Option<GraphTheme> {
        match source {
            ThemeSource::Copied => self.theme_clipboard.clone(),
            ThemeSource::Theme(ThemeRef::Project(name)) => self
                .snapshot
                .tree
                .project_themes()
                .iter()
                .find(|theme| same_theme_name(&theme.name, name))
                .cloned(),
            ThemeSource::Theme(reference) => self.user_themes.get(reference),
        }
    }

    /// What the picker lists now.
    pub fn picker_entries(&self) -> PickerEntries {
        let ranked = self.ranked_themes(self.snapshot.tree.project_themes());
        let entry = |ranked: ThemeEntry| {
            let name = match &ranked.reference {
                ThemeRef::Builtin(index) => crate::builtin_themes::display_name(*index),
                _ => ranked.theme.name.clone(),
            };
            let project = matches!(ranked.reference, ThemeRef::Project(_));
            (
                project,
                PickerEntry {
                    source: ThemeSource::Theme(ranked.reference),
                    name,
                    theme: ranked.theme,
                    modified: ranked.modified,
                },
            )
        };
        let mut entries = PickerEntries {
            copied: self.theme_clipboard.as_ref().map(|theme| PickerEntry {
                source: ThemeSource::Copied,
                name: theme.name.clone(),
                theme: theme.clone(),
                modified: false,
            }),
            ..PickerEntries::default()
        };
        for (project, item) in ranked.into_iter().map(entry) {
            if project {
                entries.project.push(item);
            } else {
                entries.mine.push(item);
            }
        }
        entries
    }

    /// The first target before and after what the Apply Theme window shows
    /// (the hovered tile and box included), `None` without a window, a target
    /// or a theme.
    pub fn apply_preview(&self) -> Option<ApplyPreview> {
        self.apply_preview_for(self.apply_theme_window.as_ref()?)
    }

    /// [`AppState::apply_preview`] of a window the interface is editing (it
    /// writes it back to the state once the frame is painted).
    pub fn apply_preview_for(&self, window: &ApplyThemeWindow) -> Option<ApplyPreview> {
        let first = *window.graphs().first()?;
        let before = if window.into_draft() {
            self.spec_for_theme(first)?
        } else {
            self.plot_spec(first)?
        };
        let theme = self.theme_of(window.shown_theme())?;
        let after = crate::theme_apply::themed_spec(
            &self.snapshot.tree,
            &before,
            &theme,
            window.shown_families(),
            window.shown_add_missing(),
        );
        Some(ApplyPreview { before, after })
    }

    /// Copy Theme: the style of `graph` goes to the app's own clipboard.
    pub(super) fn copy_theme(&mut self, graph: NodeId) {
        let Some(spec) = self.plot_spec(graph) else {
            return;
        };
        self.theme_clipboard = Some(GraphTheme::capture(tr!("theme-copied-name"), &spec));
        self.notice(tr!("status-theme-copied"));
    }

    /// Paste Theme: Style and Size of the copied theme onto `graphs`, one undo
    /// step, no window. Nothing happens without a copied theme.
    pub(super) fn paste_theme(&mut self, graphs: &[NodeId]) {
        let Some(theme) = self.theme_clipboard.clone() else {
            return;
        };
        let graphs = self.graphs_among(graphs);
        self.apply_theme(&theme, paste_families(), &graphs, false);
    }

    /// Opens the Apply Theme window on the graphs among `ids`: the copied
    /// theme is chosen when there is one, Light otherwise.
    pub(super) fn open_apply_theme(&mut self, ids: &[NodeId]) {
        let graphs = self.graphs_among(ids);
        if graphs.is_empty() {
            return;
        }
        let chosen = if self.theme_clipboard.is_some() {
            ThemeSource::Copied
        } else {
            ThemeSource::Theme(ThemeRef::Builtin(0))
        };
        self.apply_theme_window = Some(ApplyThemeWindow::new(graphs, chosen));
    }

    /// Apply in the window: one undo step on every target, the use counted,
    /// the window closed. A refusal (nothing ticked, the theme gone) changes
    /// nothing and leaves the window open.
    pub(super) fn confirm_apply_theme(&mut self) {
        let Some(window) = self.apply_theme_window.clone() else {
            return;
        };
        if window.apply_blocked().is_some() {
            return;
        }
        let Some(theme) = self.theme_of(window.chosen()) else {
            return;
        };
        if window.into_draft() {
            self.apply_theme_into_draft(&theme, &window);
        } else {
            self.apply_theme(
                &theme,
                window.families(),
                window.graphs(),
                window.add_missing(),
            );
        }
        if let ThemeSource::Theme(reference) = window.chosen() {
            match self.record_theme_use(reference) {
                // A read-only library counts nothing, quietly: the apply worked.
                Ok(()) | Err(UserThemeError::ReadOnly) => {}
                Err(err) => self.warn(crate::errors::theme::error_message(&err)),
            }
        }
        self.apply_theme_window = None;
    }

    /// Apply from the Theme tab: the theme goes into the draft of Graph
    /// Properties and the window commits it as it commits any other edit, so
    /// the OK or the Cancel of Graph Properties decides and no history step is
    /// made here.
    fn apply_theme_into_draft(&mut self, theme: &GraphTheme, window: &ApplyThemeWindow) {
        let Some(graph) = window.graphs().first().copied() else {
            return;
        };
        let Some(current) = self.spec_for_theme(graph) else {
            return;
        };
        let next = crate::theme_apply::themed_spec(
            &self.snapshot.tree,
            &current,
            theme,
            window.families(),
            window.add_missing(),
        );
        if let Some(edit) = self.plot_edit.as_mut().filter(|edit| edit.graph == graph) {
            edit.spec = next;
            self.commit_plot_edit();
        }
    }

    /// Opens the Save Theme As window on `graph` (target Mine by default).
    pub(super) fn open_save_theme(&mut self, graph: NodeId) {
        if self.is_graph(graph) {
            self.save_theme_window = Some(SaveThemeWindow {
                graph,
                name: String::new(),
                target: ThemeTarget::Mine,
            });
        }
    }

    /// Why a theme named `name` cannot be saved to `target` now, if it cannot.
    fn save_refusal(&self, name: &str, target: ThemeTarget) -> Option<UserThemeError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Some(UserThemeError::EmptyName);
        }
        match target {
            ThemeTarget::Mine => self
                .user_themes_read_only()
                .map(|_| UserThemeError::ReadOnly),
            ThemeTarget::Project => {
                let themes = self.snapshot.tree.project_themes();
                if themes.iter().any(|t| same_theme_name(&t.name, trimmed)) {
                    Some(UserThemeError::DuplicateName)
                } else if themes.len() >= MAX_THEMES {
                    Some(UserThemeError::Full)
                } else {
                    None
                }
            }
        }
    }

    /// Save Theme As: the style of `graph` under `name` in `target`. Mine goes
    /// to the library of this machine (not undoable, survives a restart);
    /// Project is one undoable step. A refusal says why in the window (or the
    /// status line) and changes nothing; success closes the window.
    pub(super) fn save_theme_as(&mut self, graph: NodeId, name: String, target: ThemeTarget) {
        if let Some(err) = self.save_refusal(&name, target) {
            self.fail(crate::errors::theme::error_message(&err));
            return;
        }
        let saved: Result<(), UserThemeError> = match target {
            ThemeTarget::Mine => self.add_user_theme(name.trim(), graph).map(|id| {
                self.theme_selection = ThemeSource::Theme(ThemeRef::User(id));
            }),
            ThemeTarget::Project => {
                let Some(spec) = self.spec_for_theme(graph) else {
                    return;
                };
                let mut themes = self.snapshot.tree.project_themes().to_vec();
                themes.push(GraphTheme::capture(name.trim(), &spec));
                self.execute(stokesforge_commands::Command::SetProjectThemes { themes });
                self.theme_selection = ThemeSource::Theme(ThemeRef::project(name.trim()));
                Ok(())
            }
        };
        match saved {
            Ok(()) => {
                self.save_theme_window = None;
                self.notice(tr!("status-theme-saved", name = name.trim().to_string()));
            }
            Err(err) => self.fail(crate::errors::theme::error_message(&err)),
        }
    }
}
