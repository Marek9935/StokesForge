//! The user's graph themes seen from the application state (#539): the
//! library on this machine ([`crate::user_themes`]), the default theme of new
//! graphs, and the entry points the interface (#514 S4, S5) will call.
//!
//! Nothing here is in the history: the library is a user preference, like the
//! user's colours. A refused or failed change is returned as a
//! [`UserThemeError`] for the interface to put into a sentence.
//!
//! The default theme of new graphs rides in the spec of the creation command
//! itself ([`AppState::with_new_graph_defaults`]): one command, one history
//! step. Graphs made by scripts never go through it.

use stokesforge_plot::{GraphTheme, PlotSpec};

use super::AppState;
use crate::user_themes::{ReadOnlyReason, ThemeEntry, ThemeRef, UserThemeError, UserThemes};

pub use stokesforge_plot::default_theme_families;

impl AppState {
    /// The user's theme library (read access).
    pub fn user_themes(&self) -> &UserThemes {
        &self.user_themes
    }

    /// The themes a script may name (#546): the built-ins as the user left them
    /// (a modified one applies modified), each under its key, its English name
    /// and its name in every catalogue language (never the pseudo-locale),
    /// then the user's themes. Set on the engine before **every** run: the
    /// library changes between two runs of the same engine. The themes of the
    /// project are not here, a script reads them from the project it runs on.
    #[cfg(feature = "python")]
    pub(crate) fn script_theme_library(&self) -> stokesforge_script::ThemeLibrary {
        use crate::i18n::{build_loader, Language};
        use stokesforge_script::{ThemeLibrary, ThemeLibraryEntry, ThemeSource};
        let loaders: Vec<_> = Language::ALL
            .into_iter()
            .filter(|language| *language != Language::PseudoEnglish)
            .map(build_loader)
            .collect();
        let entries = self
            .user_themes
            .entries()
            .into_iter()
            .filter_map(|entry| match entry.reference {
                ThemeRef::Builtin(index) => {
                    let key = crate::builtin_themes::KEYS.get(index)?;
                    let english = crate::builtin_themes::NAMES.get(index)?;
                    let mut names = vec![(*key).to_owned(), (*english).to_owned()];
                    names.extend(
                        loaders
                            .iter()
                            .map(|loader| crate::builtin_themes::display_name_in(loader, index)),
                    );
                    Some(ThemeLibraryEntry::new(
                        ThemeSource::Builtin,
                        format!("builtin:{key}"),
                        (*english).to_owned(),
                        names,
                        entry.theme,
                        entry.modified,
                    ))
                }
                ThemeRef::User(id) => Some(ThemeLibraryEntry::new(
                    ThemeSource::User,
                    format!("user:{id}"),
                    entry.theme.name.clone(),
                    vec![entry.theme.name.clone()],
                    entry.theme,
                    false,
                )),
                ThemeRef::Project(_) => None,
            })
            .collect();
        ThemeLibrary::new(entries)
    }

    /// Why the library cannot be changed this session, if it cannot.
    pub fn user_themes_read_only(&self) -> Option<&ReadOnlyReason> {
        self.user_themes.read_only()
    }

    /// Reads and writes the library in `path` instead of the configuration
    /// folder, for tests.
    pub fn use_user_themes_file(&mut self, path: std::path::PathBuf) {
        self.user_themes = UserThemes::load_from(Some(path));
    }

    /// The tile order: built-ins and user themes of the machine plus the
    /// themes of the open project, most used first (ties keep that order).
    pub fn ranked_themes(&self, project: &[GraphTheme]) -> Vec<ThemeEntry> {
        self.user_themes.ranked(project)
    }

    /// Saves the style of `graph` as a new user theme; returns its id.
    pub fn add_user_theme(
        &mut self,
        name: &str,
        graph: stokesforge_core::NodeId,
    ) -> Result<u64, UserThemeError> {
        let spec = self.plot_spec(graph).ok_or(UserThemeError::Unknown)?;
        self.user_themes.add_from_graph(name, &spec)
    }

    /// Renames a user theme.
    pub fn rename_user_theme(&mut self, id: u64, name: &str) -> Result<(), UserThemeError> {
        self.user_themes.rename(id, name)
    }

    /// Deletes a user theme. A default theme that was this one falls back to
    /// none when read ([`AppState::new_graph_theme`]).
    pub fn delete_user_theme(&mut self, id: u64) -> Result<(), UserThemeError> {
        self.user_themes.delete(id)
    }

    /// Replaces the style of a user theme with that of `graph`.
    pub fn overwrite_user_theme(
        &mut self,
        id: u64,
        graph: stokesforge_core::NodeId,
    ) -> Result<(), UserThemeError> {
        let spec = self.plot_spec(graph).ok_or(UserThemeError::Unknown)?;
        self.user_themes.overwrite_from_graph(id, &spec)
    }

    /// Edits the built-in Light (`0`) or Dark (`1`) from `graph`.
    pub fn edit_builtin_theme(
        &mut self,
        index: usize,
        graph: stokesforge_core::NodeId,
    ) -> Result<(), UserThemeError> {
        let spec = self.plot_spec(graph).ok_or(UserThemeError::Unknown)?;
        self.user_themes.set_builtin_from_graph(index, &spec)
    }

    /// Gives a built-in its original colours back.
    pub fn reset_builtin_theme(&mut self, index: usize) -> Result<(), UserThemeError> {
        self.user_themes.reset_builtin(index)
    }

    /// Counts one application of a theme (a project theme included).
    pub fn record_theme_use(&mut self, reference: &ThemeRef) -> Result<(), UserThemeError> {
        self.user_themes.record_use(reference)
    }

    /// The default theme of new graphs, if it is set **and still exists**
    /// (a deleted user theme reads as none).
    pub fn new_graph_theme(&self) -> Option<(ThemeRef, GraphTheme)> {
        let reference = self.object_manager_preferences.new_graph_theme()?.clone();
        let theme = self.user_themes.get(&reference)?;
        Some((reference, theme))
    }

    /// Chooses the default theme of new graphs (`None`: no theme). A project
    /// theme is refused: only a built-in or a user theme can be the default.
    pub fn set_new_graph_theme(&mut self, theme: Option<ThemeRef>) -> std::io::Result<()> {
        self.object_manager_preferences.save_new_graph_theme(theme)
    }

    /// `spec` of a new graph with the default theme applied (every family but
    /// the texts and the ranges); `spec` itself if there is none.
    pub(super) fn with_default_theme(&self, spec: PlotSpec) -> PlotSpec {
        let Some((_, theme)) = self.new_graph_theme() else {
            return spec;
        };
        crate::theme_apply::themed_spec(
            &self.snapshot.tree,
            &spec,
            &theme,
            default_theme_families(),
            false,
        )
    }
}
