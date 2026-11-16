//! Graph themes from a script (#546): `pyst.apply_theme` and `pyst.themes`.
//!
//! A theme is found **by name**. Three sources, in this order:
//!
//! - **built-in** themes: the catalogue of `stokesforge_plot::builtin_themes`.
//!   Without a library injected by the host (the command line, the script
//!   tests) they are the originals, named by their key (`publication-wide`) or
//!   their English name. The application injects its own library before every
//!   run ([`ThemeLibrary`], [`crate::ScriptEngine::set_theme_library`]): the
//!   built-ins as the user left them (a modified built-in applies modified),
//!   each also named in every catalogue language, plus the user's themes;
//! - **user** themes: kept on one machine, so only the application knows them;
//! - **project** themes: read from the snapshot, they travel with the `.stf`.
//!
//! The families of `families=[...]` answer to their key, their English
//! window label and `style` (the six style parts) everywhere; the host adds the
//! labels of every catalogue language ([`ThemeLibrary::with_family_names`]).
//!
//! The library is plain data, never a callback (invariant 9). Applying is one
//! `SetPlotSpec` in the script's group, written like `set_graph`; a theme that
//! changes nothing writes nothing. Scripts never take a default theme, the
//! application's recent-theme order and use counters are not touched.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyString};
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{
    builtin_themes, default_theme_families, same_theme_name, GraphTheme, ThemeFamilies,
    ThemeFamily, STYLE_PARTS,
};
use stokesforge_render_svg::FigureTexts;
use stokesforge_session::Snapshot;

use crate::bridge::{self, HostError};
use crate::engine;
use crate::surface::{to_py_error, Handle};

/// Longest theme name a call accepts, in characters (a stored name is at most
/// [`stokesforge_plot::MAX_THEME_NAME_CHARS`] long; the rest is room for a
/// `builtin:`/`project:` prefix and stray spaces).
const MAX_QUERY_CHARS: usize = 200;

/// Where a theme comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSource {
    /// The built-in catalogue (possibly modified by the user on this machine).
    Builtin,
    /// The user's library on this machine.
    User,
    /// The open project.
    Project,
}

impl ThemeSource {
    /// The prefix that restricts a name to this source, and the `source` value
    /// of `pyst.themes()`.
    fn word(self) -> &'static str {
        match self {
            ThemeSource::Builtin => "builtin",
            ThemeSource::User => "user",
            ThemeSource::Project => "project",
        }
    }
}

/// One theme of a [`ThemeLibrary`].
#[derive(Debug, Clone)]
pub struct ThemeLibraryEntry {
    source: ThemeSource,
    reference: String,
    display: String,
    names: Vec<String>,
    theme: GraphTheme,
    modified: bool,
}

impl ThemeLibraryEntry {
    /// An entry. `reference` is the stable reference (`builtin:<key>`,
    /// `user:<id>`); `display` the name `pyst.themes()` shows; `names` every
    /// name the theme answers to (case and spaces ignored when matching).
    pub fn new(
        source: ThemeSource,
        reference: String,
        display: String,
        names: Vec<String>,
        theme: GraphTheme,
        modified: bool,
    ) -> Self {
        Self {
            source,
            reference,
            display,
            names,
            theme,
            modified,
        }
    }

    fn answers_to(&self, name: &str) -> bool {
        self.names
            .iter()
            .any(|known| same_theme_name(&squash(known), &squash(name)))
    }

    /// `source:name` as an error message lists it.
    fn prefixed_name(&self) -> String {
        let name = match self.source {
            ThemeSource::Builtin => self.reference.trim_start_matches("builtin:"),
            _ => self.theme.name.as_str(),
        };
        format!("{}:{name}", self.source.word())
    }
}

/// The themes a host makes known to scripts: the built-ins and the user's
/// themes of its machine. The host sets it before every run.
#[derive(Debug, Clone, Default)]
pub struct ThemeLibrary {
    entries: Vec<ThemeLibraryEntry>,
    /// Names of the families in the catalogue languages (#551); empty by
    /// default.
    family_names: Vec<(ThemeFamily, String)>,
    /// Names of the Style box (the six style parts) in the same languages.
    style_names: Vec<String>,
}

impl ThemeLibrary {
    /// A library of `entries` (built-ins first, then user themes, is the order
    /// `pyst.themes()` shows).
    pub fn new(entries: Vec<ThemeLibraryEntry>) -> Self {
        Self {
            entries,
            family_names: Vec::new(),
            style_names: Vec::new(),
        }
    }

    /// The library plus the names the Apply Theme window gives the families
    /// in the catalogue languages, and those of its Style box. A script may
    /// then pass `families=['Courbes']` (#551). Case and spaces are ignored;
    /// the keys, the English labels and `style` always work.
    #[must_use]
    pub fn with_family_names(
        mut self,
        family_names: Vec<(ThemeFamily, String)>,
        style_names: Vec<String>,
    ) -> Self {
        self.family_names = family_names;
        self.style_names = style_names;
        self
    }

    /// What a host without a library gets: the original built-ins, answering
    /// to their key and their English name only.
    fn headless() -> Self {
        let entries = (0..builtin_themes::COUNT)
            .filter_map(|index| {
                let theme = builtin_themes::original(index)?;
                let key = builtin_themes::KEYS.get(index)?;
                Some(ThemeLibraryEntry::new(
                    ThemeSource::Builtin,
                    format!("builtin:{key}"),
                    theme.name.clone(),
                    vec![(*key).to_owned(), theme.name.clone()],
                    theme,
                    false,
                ))
            })
            .collect();
        Self::new(entries)
    }
}

/// What a lookup works on: the injected (or headless) library and the themes of
/// the project in the snapshot.
struct Catalogue {
    entries: Vec<ThemeLibraryEntry>,
    /// False when the host injected nothing: user themes are then unknown, and
    /// the error says so.
    injected: bool,
}

impl Catalogue {
    fn build(library: Option<&ThemeLibrary>, tree: &Tree) -> Self {
        let injected = library.is_some();
        let mut entries = match library {
            Some(library) => library.entries.clone(),
            None => ThemeLibrary::headless().entries,
        };
        for theme in tree.project_themes() {
            entries.push(ThemeLibraryEntry::new(
                ThemeSource::Project,
                format!("project:{}", theme.name.trim().to_lowercase()),
                theme.name.clone(),
                vec![theme.name.clone()],
                theme.clone(),
                false,
            ));
        }
        // Built-ins, user, project: the order of `pyst.themes()`.
        entries.sort_by_key(|entry| match entry.source {
            ThemeSource::Builtin => 0,
            ThemeSource::User => 1,
            ThemeSource::Project => 2,
        });
        Self { entries, injected }
    }

    /// The theme `query` designates. A `builtin:`, `user:` or `project:` prefix
    /// restricts the source; the stable reference `themes()` shows is accepted
    /// too.
    ///
    /// Several matches are not an error ([`choose_among`]): the theme taken
    /// comes back with the warning to emit.
    fn find(&self, query: &str) -> Result<(&ThemeLibraryEntry, Option<String>), HostError> {
        check_query(query)?;
        let query = query.trim();
        let (only, name) = split_prefix(query);
        let lowered = query.to_lowercase();
        let matches: Vec<&ThemeLibraryEntry> = self
            .entries
            .iter()
            .filter(|entry| only.is_none_or(|source| entry.source == source))
            .filter(|entry| entry.answers_to(name) || entry.reference.to_lowercase() == lowered)
            .collect();
        match matches.as_slice() {
            [] => Err(self.not_found(query)),
            [only_one] => Ok((only_one, None)),
            _ => Ok(choose_among(name, only.is_some(), matches)),
        }
    }

    fn not_found(&self, query: &str) -> HostError {
        let shown = shorten(query);
        let hint = if self.injected {
            "pyst.themes() lists the themes available"
        } else {
            "user themes exist only in the application: from the command line only \
             the built-in themes and those of the project are available"
        };
        HostError::UnknownTheme(format!("no theme is called \"{shown}\"; {hint}"))
    }
}

/// Several themes answer to the same name (#546 T7): the script is never
/// stopped. Without a prefix the order is project, then user, then built-in; the
/// winner is applied and a warning names it and the themes ignored, with their
/// prefixed names, so the script can name one. With a prefix only one source is
/// searched, and the first of the list wins (a warning only when several of
/// that source match). `matches` has at least two entries.
fn choose_among<'a>(
    name: &str,
    restricted: bool,
    mut matches: Vec<&'a ThemeLibraryEntry>,
) -> (&'a ThemeLibraryEntry, Option<String>) {
    if !restricted {
        // Stable: same-source duplicates keep the list order.
        matches.sort_by_key(|entry| match entry.source {
            ThemeSource::Project => 0,
            ThemeSource::User => 1,
            ThemeSource::Builtin => 2,
        });
    }
    let (winner, ignored) = (matches[0], &matches[1..]);
    let listed: Vec<String> = ignored.iter().map(|entry| entry.prefixed_name()).collect();
    let warning = format!(
        "Theme '{}': {} applied, {} ignored",
        shorten(name),
        winner.prefixed_name(),
        listed.join(", ")
    );
    (winner, Some(warning))
}

/// `builtin:`, `user:` or `project:` at the start of `query` (case ignored), and
/// the rest.
fn split_prefix(query: &str) -> (Option<ThemeSource>, &str) {
    for source in [
        ThemeSource::Builtin,
        ThemeSource::User,
        ThemeSource::Project,
    ] {
        let prefix = source.word();
        let head = query.get(..prefix.len() + 1);
        if let Some(head) = head {
            if head[..prefix.len()].eq_ignore_ascii_case(prefix) && head.ends_with(':') {
                return (Some(source), query[prefix.len() + 1..].trim());
            }
        }
    }
    (None, query)
}

/// `text` with every run of spaces made one: "Publication   Wide" is the same
/// name as "Publication Wide".
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn check_query(query: &str) -> Result<(), HostError> {
    if query.contains('\0') {
        return Err(HostError::InvalidValue(
            "a theme name cannot contain a NUL character".to_owned(),
        ));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(HostError::InvalidValue(format!(
            "a theme name is at most {MAX_QUERY_CHARS} characters long"
        )));
    }
    Ok(())
}

fn shorten(text: &str) -> String {
    text.chars().take(MAX_QUERY_CHARS).collect()
}

/// A family name as compared: words joined by `-`, `_` taken as `-`, case
/// ignored (accents are kept).
fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
        .replace('_', "-")
}

/// The families `text` names: one family by its key (`title-fonts`), the same
/// with underscores, its English label (`Size`) or a name the host injected
/// (`Courbes`); `style` (or its injected names) is the six style parts.
fn parse_family(text: &str, library: Option<&ThemeLibrary>) -> Option<ThemeFamilies> {
    let normal = normalise(text);
    if normal == "style" {
        return Some(style_set());
    }
    if let Some(family) = ThemeFamily::ALL
        .into_iter()
        .find(|family| family.key() == normal || normalise(family.english_name()) == normal)
    {
        return Some(ThemeFamilies::only(family));
    }
    let library = library?;
    if library
        .style_names
        .iter()
        .any(|name| normalise(name) == normal)
    {
        return Some(style_set());
    }
    library
        .family_names
        .iter()
        .find(|(_, name)| normalise(name) == normal)
        .map(|(family, _)| ThemeFamilies::only(*family))
}

fn style_set() -> ThemeFamilies {
    STYLE_PARTS
        .iter()
        .fold(ThemeFamilies::none(), |set, part| set.with(*part))
}

/// The families a call asks for: `None` is the default theme's, an empty list
/// is refused, an unknown name lists the keys.
fn parse_families(
    names: Option<&[String]>,
    library: Option<&ThemeLibrary>,
) -> Result<ThemeFamilies, HostError> {
    let Some(names) = names else {
        return Ok(default_theme_families());
    };
    if names.is_empty() {
        return Err(HostError::InvalidValue(
            "families is empty: name at least one family, or leave it out for the \
             default ones"
                .to_owned(),
        ));
    }
    let mut set = ThemeFamilies::none();
    for name in names {
        let named = parse_family(name, library).ok_or_else(|| {
            let mut keys: Vec<&str> = ThemeFamily::ALL.iter().map(|f| f.key()).collect();
            keys.push("style");
            HostError::InvalidValue(format!(
                "unknown theme family \"{}\": use {}",
                shorten(name),
                keys.join(", ")
            ))
        })?;
        set = named.iter().fold(set, |set, family| set.with(family));
    }
    Ok(set)
}

fn apply_native(
    context: &engine::ScriptContext,
    graph: NodeId,
    epoch: u64,
    query: &str,
    families: ThemeFamilies,
    add_missing: bool,
) -> Result<Option<String>, HostError> {
    let library = context.theme_library();
    let prepare = |snapshot: &Snapshot| {
        let before = bridge::graph_spec(snapshot, graph, epoch)?;
        let catalogue = Catalogue::build(library.as_deref(), &snapshot.tree);
        let (entry, warning) = catalogue.find(query)?;
        let next = stokesforge_render_svg::themed_spec(
            &snapshot.tree,
            &before,
            &entry.theme,
            families,
            add_missing,
            &FigureTexts::default(),
        );
        let changed = next != before;
        Ok::<_, HostError>((next, changed, warning))
    };
    let session = context.session();
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    let (mut spec, mut changed, mut warning) = prepare(&snapshot)?;
    if !changed {
        return Ok(warning);
    }
    if context.begin_write()? {
        // Handle, theme and spec come from the reserved snapshot.
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        (spec, changed, warning) = prepare(&snapshot)?;
    }
    if changed {
        bridge::set_graph_spec(session, graph, spec)?;
    }
    Ok(warning)
}

/// One line of `pyst.themes()`, as plain data.
struct ThemeInfo {
    name: String,
    key: String,
    source: &'static str,
    modified: bool,
}

fn list_native(context: &engine::ScriptContext) -> Result<Vec<ThemeInfo>, HostError> {
    let snapshot = context
        .session()
        .snapshot()
        .map_err(HostError::from_session)?;
    let library = context.theme_library();
    let catalogue = Catalogue::build(library.as_deref(), &snapshot.tree);
    Ok(catalogue
        .entries
        .iter()
        .map(|entry| ThemeInfo {
            name: entry.display.clone(),
            key: entry.reference.clone(),
            source: entry.source.word(),
            modified: entry.modified,
        })
        .collect())
}

/// Applies the graph theme `theme` to `graph`, in the script's undoable group.
///
/// `theme` is a built-in theme (its key, `'publication-wide'`, or its name in
/// any language of the application, `'Publication Wide'`, `'Publication
/// large'`), a user theme of this machine or a theme of the project, by name.
/// Case and extra spaces are ignored; a `builtin:`, `user:` or `project:`
/// prefix restricts the source. A built-in the user modified applies modified.
///
/// `families` names the families to copy (`'background'`, `'texts'`,
/// `'title-fonts'`, `'axes'`, `'axis-ranges'`, `'size-and-page'`, `'legend'`,
/// `'curves'`, `'color-bar'`; `'title_fonts'` works too, and so do the names
/// the Apply Theme window shows, in English or French whatever the interface
/// language: `'Curves'`, `'Courbes'`, `'Size'`). `'style'` is the six style
/// parts, like the Style box. Left out, it is every family but the texts and the axis ranges, as for
/// the default theme of new graphs; an empty list is refused.
/// `add_missing_layers=True` adds empty layers up to the theme's count.
///
/// A script that names a user theme is not portable: user themes stay on one
/// machine and the command line does not know them. A project theme travels
/// with the project; a built-in may have been modified on a machine. The robust
/// script draws its own style and uses no theme.
///
/// When several themes answer to the name, the project's wins over the user's,
/// which wins over the built-in, and a `UserWarning` names the theme applied and
/// the ones ignored (`user:Fig A`); a prefix settles it.
///
/// Raises `UnknownThemeError` for a name no theme answers to, `InvalidValueError` for a bad family or a handle that is not a
/// graph, `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (graph, theme, *, families=None, add_missing_layers=false))]
pub(crate) fn apply_theme(
    py: Python<'_>,
    graph: &Handle,
    theme: &str,
    families: Option<&Bound<'_, PyAny>>,
    add_missing_layers: bool,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let names = families.map(family_names).transpose()?;
    let library = context.theme_library();
    let families = parse_families(names.as_deref(), library.as_deref()).map_err(to_py_error)?;
    let (id, epoch) = (graph.id, graph.epoch);
    let warning = py
        .detach(|| apply_native(&context, id, epoch, theme, families, add_missing_layers))
        .map_err(to_py_error)?;
    if let Some(message) = warning {
        warn(py, &message)?;
    }
    Ok(())
}

/// A `UserWarning` that shows every time, even from the same line of a loop
/// (`warn_explicit` with no registry skips the "once per place" filter); it
/// lands in the script's stderr, so the console shows it. A script that turns
/// warnings into errors gets the exception, as with any warning.
fn warn(py: Python<'_>, message: &str) -> PyResult<()> {
    let category = py.get_type::<pyo3::exceptions::PyUserWarning>();
    py.import("warnings")?.call_method1(
        "warn_explicit",
        (message, category, "<pyst.apply_theme>", 1),
    )?;
    Ok(())
}

/// The names of a `families` argument: a list or tuple of strings.
fn family_names(value: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let refused = || {
        crate::graph::invalid("families is a list of family names, for example ['curves', 'axes']")
    };
    if value.is_instance_of::<PyString>() {
        return Err(refused());
    }
    value.extract::<Vec<String>>().map_err(|_| refused())
}

/// The themes a script can name, as a list of dictionaries: `name`, `key` (the
/// stable reference, `builtin:<key>` or `user:<id>` or `project:<name>`, which
/// `apply_theme` accepts), `source` (`'builtin'`, `'user'` or `'project'`) and
/// `modified` (a built-in the user changed). Built-ins first, then the user's,
/// then the project's. From the command line no user theme is listed.
#[pyfunction]
pub(crate) fn themes(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let infos = py.detach(|| list_native(&context)).map_err(to_py_error)?;
    let list = PyList::empty(py);
    for info in infos {
        let item = PyDict::new(py);
        item.set_item("name", info.name)?;
        item.set_item("key", info.key)?;
        item.set_item("source", info.source)?;
        item.set_item("modified", info.modified)?;
        list.append(item)?;
    }
    Ok(list)
}
