//! The graph-theme gestures seen from the state (#541, S4 of #514): the theme
//! clipboard of Copy Theme and Paste Theme, the **Apply Theme** window and the
//! **Save Theme As** window, and the facts the menus need to grey an entry with
//! its reason. Nothing here names an interface type (principle 8): the windows
//! are structs of [`crate::state::AppState`], the interface paints them and
//! changes them through the methods below.
//!
//! # Where a theme comes from
//!
//! A [`ThemeSource`] designates what the picker offers: the **copied theme**
//! (the app's own clipboard, never the system one) or a theme of the library
//! ([`ThemeRef`]: built-in, user, project).
//!
//! # Hover first
//!
//! The Apply Theme window previews what the pointer rests on **before** any
//! click: a tile shows that theme, a box shows the choice with that box
//! flipped. The hovered override lives next to the choice and never replaces
//! it: only a click ([`ApplyThemeWindow::choose`], [`ApplyThemeWindow::set_family`]…)
//! changes the choice. The `shown_*` getters are what the preview draws.

use stokesforge_core::NodeId;
use stokesforge_plot::{ThemeFamilies, ThemeFamily};

use crate::tr;
use crate::user_themes::ThemeRef;

/// What the picker offers as a theme.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeSource {
    /// The theme copied by Copy Theme (the app's clipboard).
    Copied,
    /// A theme of the library or of the project.
    Theme(ThemeRef),
}

/// The six parts of the **Style** box, in the order the window lists them.
pub use stokesforge_plot::STYLE_PARTS;

/// The label of `family` in the language of `loader`: what the Apply Theme
/// window shows, and what a script may name (#551).
pub fn family_name_in(
    loader: &i18n_embed::fluent::FluentLanguageLoader,
    family: ThemeFamily,
) -> String {
    match family {
        ThemeFamily::Background => tr!(in loader; "theme-part-background"),
        ThemeFamily::TitleFonts => tr!(in loader; "theme-part-title-fonts"),
        ThemeFamily::Axes => tr!(in loader; "theme-part-axes"),
        ThemeFamily::Legend => tr!(in loader; "theme-part-legend"),
        ThemeFamily::Curves => tr!(in loader; "theme-part-curves"),
        ThemeFamily::ColorBar => tr!(in loader; "theme-part-color-bar"),
        ThemeFamily::Texts => tr!(in loader; "theme-what-texts"),
        ThemeFamily::SizeAndPage => tr!(in loader; "theme-what-size"),
        ThemeFamily::AxisRanges => tr!(in loader; "theme-what-axis-ranges"),
    }
}

/// [`family_name_in`] in the current language.
pub fn family_name(family: ThemeFamily) -> String {
    family_name_in(crate::i18n::loader(), family)
}

/// The label of the Style box (the six parts) in the language of `loader`.
pub fn style_name_in(loader: &i18n_embed::fluent::FluentLanguageLoader) -> String {
    tr!(in loader; "theme-what-style")
}

/// [`style_name_in`] in the current language.
pub fn style_name() -> String {
    style_name_in(crate::i18n::loader())
}

/// What Paste Theme applies: Style and Size, never the texts nor the ranges.
pub fn paste_families() -> ThemeFamilies {
    STYLE_PARTS.iter().fold(
        ThemeFamilies::only(ThemeFamily::SizeAndPage),
        |set, part| set.with(*part),
    )
}

/// What a new Apply Theme window ticks: Style, Size and Texts (not the ranges).
pub fn default_apply_families() -> ThemeFamilies {
    paste_families().with(ThemeFamily::Texts)
}

/// State of the Style box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleState {
    /// Its six parts are ticked.
    All,
    /// None is.
    None,
    /// Some are: the box shows the mixed mark.
    Mixed,
}

/// The state of the Style box for `families`.
pub fn style_state(families: ThemeFamilies) -> StyleState {
    let on = STYLE_PARTS
        .iter()
        .filter(|part| families.contains(**part))
        .count();
    match on {
        0 => StyleState::None,
        n if n == STYLE_PARTS.len() => StyleState::All,
        _ => StyleState::Mixed,
    }
}

/// `families` with the whole Style box set to `on`.
pub fn with_style(families: ThemeFamilies, on: bool) -> ThemeFamilies {
    STYLE_PARTS
        .iter()
        .fold(families, |set, part| set.set(*part, on))
}

/// The Apply Theme window (module documentation).
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyThemeWindow {
    graphs: Vec<NodeId>,
    chosen: ThemeSource,
    families: ThemeFamilies,
    add_missing: bool,
    hover_theme: Option<ThemeSource>,
    hover_families: Option<ThemeFamilies>,
    hover_add_missing: Option<bool>,
    into_draft: bool,
}

impl ApplyThemeWindow {
    /// A window on `graphs` (the first one is previewed) with `chosen` ticked.
    pub fn new(graphs: Vec<NodeId>, chosen: ThemeSource) -> Self {
        Self {
            graphs,
            chosen,
            families: default_apply_families(),
            add_missing: false,
            hover_theme: None,
            hover_families: None,
            hover_add_missing: None,
            into_draft: false,
        }
    }

    /// A window opened from the Theme tab of Graph Properties (#542): it
    /// previews and applies **into the open draft** of `graph`, so the OK or
    /// Cancel of Graph Properties decides and no history step of its own is
    /// made.
    pub fn new_into_draft(graph: NodeId, chosen: ThemeSource) -> Self {
        Self {
            into_draft: true,
            ..Self::new(vec![graph], chosen)
        }
    }

    /// True when the theme goes into the Graph Properties draft.
    pub fn into_draft(&self) -> bool {
        self.into_draft
    }

    /// The graphs the theme goes to, in order.
    pub fn graphs(&self) -> &[NodeId] {
        &self.graphs
    }

    /// The chosen theme (a click, never a hover).
    pub fn chosen(&self) -> &ThemeSource {
        &self.chosen
    }

    /// The ticked families.
    pub fn families(&self) -> ThemeFamilies {
        self.families
    }

    /// The « Add missing layers » box.
    pub fn add_missing(&self) -> bool {
        self.add_missing
    }

    /// A click on a tile.
    pub fn choose(&mut self, source: ThemeSource) {
        self.chosen = source;
    }

    /// A click on a box of one family.
    pub fn set_family(&mut self, family: ThemeFamily, on: bool) {
        self.families = self.families.set(family, on);
    }

    /// A click on the Style box: all six parts on or off.
    pub fn set_style(&mut self, on: bool) {
        self.families = with_style(self.families, on);
    }

    /// A click on « Add missing layers ».
    pub fn set_add_missing(&mut self, on: bool) {
        self.add_missing = on;
    }

    /// The families a click on the box of `family` would leave ticked.
    pub fn families_if_family_flipped(&self, family: ThemeFamily) -> ThemeFamilies {
        self.families.set(family, !self.families.contains(family))
    }

    /// The families a click on the Style box would leave ticked (a mixed box
    /// goes to all on).
    pub fn families_if_style_flipped(&self) -> ThemeFamilies {
        with_style(self.families, style_state(self.families) != StyleState::All)
    }

    /// Sets what the pointer rests on (each field `None` when it rests on
    /// nothing of its kind); the choice itself is left alone.
    pub fn set_hover(
        &mut self,
        theme: Option<ThemeSource>,
        families: Option<ThemeFamilies>,
        add_missing: Option<bool>,
    ) {
        self.hover_theme = theme;
        self.hover_families = families;
        self.hover_add_missing = add_missing;
    }

    /// The theme the preview shows: the hovered tile, else the chosen one.
    pub fn shown_theme(&self) -> &ThemeSource {
        self.hover_theme.as_ref().unwrap_or(&self.chosen)
    }

    /// The families the preview applies: the hovered box flipped, else the
    /// ticked ones.
    pub fn shown_families(&self) -> ThemeFamilies {
        self.hover_families.unwrap_or(self.families)
    }

    /// « Add missing layers » as the preview applies it.
    pub fn shown_add_missing(&self) -> bool {
        self.hover_add_missing.unwrap_or(self.add_missing)
    }

    /// Why Apply is greyed, if it is: nothing ticked.
    pub fn apply_blocked(&self) -> Option<String> {
        self.families
            .is_empty()
            .then(|| tr!("theme-apply-nothing-checked"))
    }
}

/// Where Save Theme As puts the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeTarget {
    /// The user's library on this machine (the default).
    #[default]
    Mine,
    /// The open project (undoable, saved in the `.stf`).
    Project,
}

/// The Save Theme As window: the name and the target.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveThemeWindow {
    /// The graph whose style is saved.
    pub graph: NodeId,
    /// The name being typed.
    pub name: String,
    /// Where it goes.
    pub target: ThemeTarget,
}

/// The Rename Theme window (#542): the new name of a user or project theme.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameThemeWindow {
    /// The theme being renamed.
    pub theme: ThemeRef,
    /// The name being typed (starts as the current name).
    pub name: String,
}

/// What the Theme tab of Graph Properties offers for the selected theme: each
/// control is `None` when it can act, `Some(reason)` when it is greyed (every
/// greyed control says why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeButtons {
    /// Rename…
    pub rename: Option<String>,
    /// Update From This Graph.
    pub update: Option<String>,
    /// Reset (an edited built-in only).
    pub reset: Option<String>,
    /// Delete (user and project themes).
    pub delete: Option<String>,
    /// Use for new graphs (a built-in or a user theme).
    pub use_for_new: Option<String>,
}

/// What an entry of the theme menus does for a target, with the reason when it
/// is greyed (module documentation of the menus: relevant but momentarily
/// impossible is greyed with its reason, never absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeMenu {
    /// Copy Theme: `Some(reason)` when greyed.
    pub copy: Option<String>,
    /// Paste Theme.
    pub paste: Option<String>,
    /// Apply Theme….
    pub apply: Option<String>,
    /// Save Theme As… (menu bar).
    pub save: Option<String>,
}

/// The state of the theme entries for `graphs` targets and a theme clipboard
/// that holds something (`clipboard`).
pub fn theme_menu(clipboard: bool, graphs: usize) -> ThemeMenu {
    let none = (graphs == 0).then(|| tr!("theme-needs-a-graph"));
    ThemeMenu {
        copy: none
            .clone()
            .or_else(|| (graphs > 1).then(|| tr!("theme-copy-one-graph"))),
        paste: none
            .clone()
            .or_else(|| (!clipboard).then(|| tr!("theme-nothing-copied"))),
        apply: none.clone(),
        save: none.or_else(|| (graphs > 1).then(|| tr!("theme-save-one-graph"))),
    }
}

/// A theme as the picker lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct PickerEntry {
    /// What a click chooses.
    pub source: ThemeSource,
    /// The name shown under the tile: the translated name of a built-in, the
    /// name as typed otherwise.
    pub name: String,
    /// The theme (its specimen is drawn from it).
    pub theme: stokesforge_plot::GraphTheme,
    /// An edited built-in (the distinctive mark, form 514-4 V4).
    pub modified: bool,
}

/// What the picker lists, by row group (module documentation of the picker).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PickerEntries {
    /// The copied theme, when there is one: it comes first.
    pub copied: Option<PickerEntry>,
    /// The themes of the open project, in the ranking order.
    pub project: Vec<PickerEntry>,
    /// The built-ins and the user's themes: Light and Dark first, then the
    /// rest by use.
    pub mine: Vec<PickerEntry>,
}

impl PickerEntries {
    /// Every source listed, the copied one first.
    pub fn sources(&self) -> impl Iterator<Item = &ThemeSource> {
        self.copied
            .iter()
            .chain(&self.project)
            .chain(&self.mine)
            .map(|entry| &entry.source)
    }
}

/// What the Apply Theme preview draws: the first target before and after.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyPreview {
    /// The graph as it is.
    pub before: stokesforge_plot::PlotSpec,
    /// The graph with what the window shows applied.
    pub after: stokesforge_plot::PlotSpec,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{build_loader, Language};

    fn languages() -> Vec<Language> {
        Language::ALL
            .into_iter()
            .filter(|language| *language != Language::PseudoEnglish)
            .collect()
    }

    /// The comparison a script makes: words joined by `-`, `_` as `-`, case
    /// ignored.
    fn normal(text: &str) -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase()
            .replace('_', "-")
    }

    #[test]
    fn every_family_and_the_style_box_have_a_name_in_both_catalogs() {
        for language in languages() {
            let loader = build_loader(language);
            let mut names: Vec<String> = ThemeFamily::ALL
                .into_iter()
                .map(|family| family_name_in(&loader, family))
                .collect();
            names.push(style_name_in(&loader));
            for name in names {
                assert!(
                    !name.is_empty() && !name.contains("theme-"),
                    "{language:?}: {name:?}"
                );
            }
        }
    }

    /// A name a script gives must designate one family: in a language no two
    /// families share a name, none is another family's key, none is `style`
    /// (unless it is the style name).
    #[test]
    fn the_family_names_do_not_collide_in_any_language() {
        for language in languages() {
            let loader = build_loader(language);
            for a in ThemeFamily::ALL {
                let name = normal(&family_name_in(&loader, a));
                assert_ne!(name, "style", "{language:?} {a:?}");
                for b in ThemeFamily::ALL {
                    if a != b {
                        assert_ne!(name, normal(&family_name_in(&loader, b)), "{language:?}");
                        assert_ne!(name, b.key(), "{language:?} {a:?} is the key of {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_english_names_of_the_plot_crate_are_the_english_labels() {
        let loader = build_loader(Language::English);
        for family in ThemeFamily::ALL {
            assert_eq!(family_name_in(&loader, family), family.english_name());
        }
    }
}
