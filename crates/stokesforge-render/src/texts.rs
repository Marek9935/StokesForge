//! The sentences the figure draws by itself.
//!
//! A few texts of a figure are not in the graph description: the placeholder
//! of a series whose column is gone, the « … and N more » line of a full
//! legend, the name of an unnamed row series. The renderer makes no
//! sentence of its own for the user: the application (`stokesforge-app-state`,
//! Fluent catalogs) supplies a [`FigureTexts`]; the script and CLI exports
//! use [`FigureTexts::default`], a fixed English.
//!
//! **Both backends must receive the same texts** (invariants 4 and 5): the
//! legend text is measured once and decides the frame, so a screen drawn with
//! one language and an export with another would disagree silently.

use std::fmt;
use std::sync::Arc;

type Phrase<T> = Arc<dyn Fn(T) -> String + Send + Sync>;
type NamePhrase = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// The texts drawn by the renderer, in one language.
#[derive(Clone)]
pub struct FigureTexts {
    /// Identifies the language. Two values with the same tag draw the same
    /// shapes: the screen cache compares this tag, never the closures.
    pub language: String,
    /// Placeholder of an unnamed series whose Y column no longer exists.
    pub broken_reference: String,
    broken_named: NamePhrase,
    more_entries: Phrase<usize>,
    row_series: Phrase<usize>,
}

impl FigureTexts {
    /// Builds the texts of a language from its three phrases.
    ///
    /// `broken_named` receives the series name, `more_entries` the number of
    /// legend entries left out, `row_series` the displayed row number
    /// (1 = the first row).
    pub fn new(
        language: impl Into<String>,
        broken_reference: impl Into<String>,
        broken_named: impl Fn(&str) -> String + Send + Sync + 'static,
        more_entries: impl Fn(usize) -> String + Send + Sync + 'static,
        row_series: impl Fn(usize) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            language: language.into(),
            broken_reference: broken_reference.into(),
            broken_named: Arc::new(broken_named),
            more_entries: Arc::new(more_entries),
            row_series: Arc::new(row_series),
        }
    }

    /// Legend text of a **named** series whose reference is broken.
    pub fn broken_named(&self, name: &str) -> String {
        (self.broken_named)(name)
    }

    /// Last line of a legend that left `count` entries out.
    pub fn more_entries(&self, count: usize) -> String {
        (self.more_entries)(count)
    }

    /// Name of an unnamed row series; `number` is the displayed row (1 = first).
    pub fn row_series(&self, number: usize) -> String {
        (self.row_series)(number)
    }
}

impl Default for FigureTexts {
    /// The fixed English of the script and CLI exports.
    fn default() -> Self {
        Self::new(
            "en",
            "(broken reference)",
            |name| format!("{name} — broken reference"),
            |count| format!("… and {count} more"),
            |number| format!("Row {number}"),
        )
    }
}

impl fmt::Debug for FigureTexts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FigureTexts")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}
