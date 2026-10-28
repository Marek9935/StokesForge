//! Font choice — text and Math — embedded or system.
//!
//! One-off task of 2026-09-13, outside the R0-R8 sequencing: embed three
//! pairs of sans-serif text font + Math font (Libertinus, Fira, Noto) and give
//! [`text::font_definitions`] a real choice rather than epaint's
//! `FontDefinitions::default()` that it returned until now without
//! discussion. See `docs/archive/newarchitecture/architecture-choisie.md` §7
//! for the documented decision and its date: it replaces the sentence « Fonts
//! embarquées uniquement, scan système désactivé », which predated this task.
//!
//! # What this module guarantees, and why it is the only place that matters
//!
//! [`text::font_definitions`] remains the **single source of text
//! measurement** (`text.rs`, task R4.1): the screen and the export keep
//! calling exactly the same function, hence sharing the same font set,
//! whatever choice is requested. This module does not break this invariant —
//! it gives it a parameter ([`FontChoice`]) instead of freezing it on epaint's
//! default. A caller who changes the font without passing the same
//! [`FontChoice`] on both sides would reproduce exactly the defect `text.rs`
//! exists to prevent; this module can do nothing about it alone, hence the
//! single entry point [`resolve`] rather than two separate functions "for the
//! screen" and "for the export".
//!
//! # The three embedded pairs
//!
//! Under the SIL Open Font License 1.1 (verified on each source, details and
//! provenance in `vendor/fonts/README.md`):
//!
//! | [`FontSet`] | Text | Math |
//! |---|---|---|
//! | [`FontSet::Libertinus`] | Libertinus Sans | Libertinus Math |
//! | [`FontSet::Fira`] | Fira Sans | Fira Math |
//! | [`FontSet::Noto`] | Noto Sans | Noto Sans Math |
//!
//! [`FontSet::Noto`] is the type's [`Default`]: it is the font used when
//! nothing is requested, and the one silently fallen back on when a requested
//! system font is not found — the two roles the user request assigns to Noto.
//!
//! # System font, and fallback
//!
//! [`FontChoice::System`] designates a font by its name ("Calibri", for
//! example). The resolution ([`resolve`]) looks for it on the current machine
//! through `fontdb` — in substance the same mechanism as the one
//! `stokesforge-render-svg::pdf` already uses on the export side so as not to
//! lose text silently (`architecture-choisie.md` §5, R6). Not found ⇒ fallback
//! to [`FontSet::Noto`], without error: a font name that matches nothing on
//! the machine where the project is reopened must neither crash, nor make the
//! text disappear, nor raise an error that no graph user should have to
//! handle.
//!
//! # What is not here
//!
//! Nothing wires this mechanism into `stokesforge_plot::PlotSpec`: the spec
//! has no font field today (verified in
//! `crates/stokesforge-plot/src/{spec,series,axis,panel,legend,style}.rs`),
//! and adding one would touch the persisted `.stf` format — a decision outside
//! the scope of this task (`CLAUDE.md`, pitfall "`.stf` format").
//! [`resolve`] is written so that a future field only has to pass it a
//! [`FontChoice`], with no rework.
//!
//! Ordinary text (ticks, axis titles) keeps going through the egui fonts and
//! never through Typst: only a formula (`$...$`, R8, feature `typst`,
//! `architecture-choisie.md` §4.3) consumes the Math fonts, through
//! [`crate::formula::resolve_fonts_with_math`]. [`FontSet::math_bytes`] thus
//! exposes them as raw bytes — not as `epaint::FontData`, since nothing
//! consumes that type for Math — for this single caller.

use std::sync::Arc;

use epaint::text::{FontData, FontDefinitions, FontFamily};

/// One of the three sans-serif text font + Math font pairs embedded in the
/// binary (`vendor/fonts/`, see the manifest next to it).
///
/// [`Default`] is [`FontSet::Noto`]: it is the font used when nothing is
/// requested, and the one [`resolve`] silently falls back on when a system
/// font is not found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSet {
    /// Libertinus Sans (text) + Libertinus Math.
    Libertinus,
    /// Fira Sans (text) + Fira Math.
    Fira,
    /// Noto Sans (text) + Noto Sans Math — the default and the fallback.
    Noto,
}

impl FontSet {
    /// The three pairs, in the order the user request lists them.
    pub const ALL: [FontSet; 3] = [FontSet::Libertinus, FontSet::Fira, FontSet::Noto];

    /// The family name of the text font, as embedded in the file itself
    /// (verified at retrieval, see the manifest).
    pub fn sans_family_name(self) -> &'static str {
        match self {
            FontSet::Libertinus => "Libertinus Sans",
            FontSet::Fira => "Fira Sans",
            FontSet::Noto => "Noto Sans",
        }
    }

    /// The family name of the associated Math font.
    pub fn math_family_name(self) -> &'static str {
        match self {
            FontSet::Libertinus => "Libertinus Math",
            FontSet::Fira => "Fira Math",
            FontSet::Noto => "Noto Sans Math",
        }
    }

    /// The raw bytes of the text font file, embedded in the binary at compile
    /// time.
    pub fn sans_bytes(self) -> &'static [u8] {
        match self {
            FontSet::Libertinus => {
                include_bytes!("../../../vendor/fonts/libertinus-sans/LibertinusSans-Regular.otf")
            }
            FontSet::Fira => {
                include_bytes!("../../../vendor/fonts/fira-sans/FiraSans-Regular.ttf")
            }
            FontSet::Noto => {
                include_bytes!("../../../vendor/fonts/noto-sans/NotoSans-Variable.ttf")
            }
        }
    }

    /// The raw bytes of the Math font file, embedded in the binary at compile
    /// time.
    ///
    /// Only caller: [`crate::formula::resolve_fonts_with_math`] (R8, feature
    /// `typst`) — see the module documentation. Raw bytes and not
    /// `epaint::FontData`: it is this Typst consumer that decides the form it
    /// needs, not this module.
    pub fn math_bytes(self) -> &'static [u8] {
        match self {
            FontSet::Libertinus => {
                include_bytes!("../../../vendor/fonts/libertinus-math/LibertinusMath-Regular.otf")
            }
            FontSet::Fira => {
                include_bytes!("../../../vendor/fonts/fira-math/FiraMath-Regular.otf")
            }
            FontSet::Noto => {
                include_bytes!("../../../vendor/fonts/noto-sans-math/NotoSansMath-Regular.ttf")
            }
        }
    }
}

impl Default for FontSet {
    /// Noto Sans / Noto Sans Math — point 1 of the user request.
    fn default() -> Self {
        FontSet::Noto
    }
}

/// The text font requested from [`resolve`].
///
/// [`Default`] is [`FontChoice::Embedded`]`(`[`FontSet::Noto`]`)`: with
/// nothing specified, StokesForge uses Noto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontChoice {
    /// One of the three embedded pairs, designated explicitly.
    Embedded(FontSet),
    /// A system font, by its exact name (for example `"Calibri"`).
    ///
    /// "It must always be possible to retrieve/use the system fonts instead":
    /// so this is not a font of last resort, but a first-class choice, on a
    /// par with one of the three embedded pairs. Not found on the current
    /// machine ⇒ [`resolve`] silently falls back to [`FontSet::Noto`].
    System(String),
}

impl Default for FontChoice {
    fn default() -> Self {
        FontChoice::Embedded(FontSet::default())
    }
}

/// Where the text font really comes from after resolution — for a caller who
/// wants to know whether a fallback happened (diagnostics, tests), without it
/// changing the returned [`FontDefinitions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// One of the three embedded pairs, as requested.
    Embedded(FontSet),
    /// The requested system font was found and is used as is.
    System(String),
    /// The requested system font was not found on this machine: we fell back
    /// to Noto, silently. The name kept is the one that was requested, not the
    /// one used — useful for a diagnostic, never shown to the user as an
    /// error.
    FallenBackToNoto {
        /// The name that was requested and not found.
        requested: String,
    },
}

/// The result of [`resolve`]: the [`FontDefinitions`] to actually set on an
/// `epaint::Fonts` (screen or export, indifferently — that is the whole
/// invariant of this module), the provenance of the text font, and the Math
/// font to pair with it, for the Typst consumer (R8).
#[derive(Debug, Clone)]
pub struct ResolvedFonts {
    /// To be set as is on an `epaint::Fonts` or an `egui::Context`.
    pub definitions: FontDefinitions,
    /// Where the text font used comes from.
    pub source: FontSource,
    /// The Math font to pair with this resolution.
    ///
    /// For an embedded choice, it is the Math of the same pair. For a system
    /// font — found or not — it is [`FontSet::Noto`]: a system font has no
    /// Math companion, and Noto is already the fallback of the rest of the
    /// mechanism. The Typst caller (R8,
    /// `crate::formula::resolve_fonts_with_math`) thus always has a coherent
    /// Math font to use, without having to guess.
    pub math: FontSet,
}

/// Resolves a [`FontChoice`] into fonts that are really usable.
///
/// **The** entry point of this module: a caller that needs
/// [`FontDefinitions`] (to seed an `egui::Context`, or to build a
/// [`text::TextMeasurer`] on the export side) always goes through here, with
/// the same [`FontChoice`] on both sides — this is what holds the invariant
/// of `text.rs`.
///
/// Looking up a system font (`FontChoice::System`) scans the file system
/// (`fontdb`, loading of the system fonts): do not call in a tight loop, as
/// already for [`text::TextMeasurer`].
pub fn resolve(choice: &FontChoice) -> ResolvedFonts {
    match choice {
        FontChoice::Embedded(set) => ResolvedFonts {
            definitions: definitions_for(set.sans_family_name(), set.sans_bytes(), None),
            source: FontSource::Embedded(*set),
            math: *set,
        },
        FontChoice::System(name) => match load_system_font(name) {
            Some(data) => ResolvedFonts {
                definitions: definitions_for(name, &[], Some(data)),
                source: FontSource::System(name.clone()),
                math: FontSet::Noto,
            },
            None => ResolvedFonts {
                definitions: definitions_for(
                    FontSet::Noto.sans_family_name(),
                    FontSet::Noto.sans_bytes(),
                    None,
                ),
                source: FontSource::FallenBackToNoto {
                    requested: name.clone(),
                },
                math: FontSet::Noto,
            },
        },
    }
}

/// Builds [`FontDefinitions`] whose [`FontFamily::Proportional`] family starts
/// with `name`, followed by epaint's fallbacks already present (emoji,
/// `Ubuntu-Light`) — unchanged, so as not to lose the character coverage they
/// offer on what `name` does not cover. The [`FontFamily::Monospace`] family
/// is untouched: it is outside the scope of this task (embedding "sans-serif
/// + Math", not monospace).
///
/// `bytes` is ignored (and may be empty) when `owned` already carries the
/// data — this is the case of a system font, whose bytes come from `fontdb`
/// and not from an `include_bytes!`.
fn definitions_for(name: &str, bytes: &'static [u8], owned: Option<FontData>) -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    let data = owned.unwrap_or_else(|| FontData::from_static(bytes));
    definitions
        .font_data
        .insert(name.to_owned(), Arc::new(data));

    let mut proportional = vec![name.to_owned()];
    proportional.extend(
        definitions
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    definitions
        .families
        .insert(FontFamily::Proportional, proportional);

    definitions
}

/// Looks up a system font by its exact name, and returns its raw bytes (with
/// the face index inside the file) if it exists.
///
/// `None` covers both "no font of this name" and "found, but its bytes could
/// not be read": in both cases, [`resolve`] falls back to Noto without
/// distinction, exactly as the user request prescribes ("no blocking error, no
/// lost text").
///
/// `pub(crate)` (and not private) since the task of 2026-09-13 on
/// `formula.rs`: Typst needs the same raw bytes to build its own
/// `typst::text::Font`s, text and Math — same system resolution mechanism,
/// not a second one to write for this single consumer.
pub(crate) fn load_system_font(name: &str) -> Option<FontData> {
    let database = system_database();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        ..fontdb::Query::default()
    };
    let id = database.query(&query)?;
    database.with_face_data(id, |bytes, index| {
        let mut data = FontData::from_owned(bytes.to_vec());
        data.index = index;
        data
    })
}

/// The system font database, scanned **once** per process: the scan costs
/// from a few tens of milliseconds to a second, and serves both the
/// resolution of a font by its name and the list offered by the text format
/// bar (issue #63).
fn system_database() -> &'static fontdb::Database {
    static DATABASE: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
    DATABASE.get_or_init(|| {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        database
    })
}

/// The families a text can request through `#text(font: "…")`: first the
/// three embedded text fonts (always present, hence reproducible from one
/// machine to another), then the system ones, sorted and without duplicates.
/// A figure that names a system font is recomposed identically only on a
/// machine that has it (issue #63).
///
/// The first call scans the system (see [`system_database`]).
pub fn available_families() -> &'static [FamilyEntry] {
    static FAMILIES: std::sync::OnceLock<Vec<FamilyEntry>> = std::sync::OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut embedded: Vec<FamilyEntry> = FontSet::ALL
            .iter()
            .map(|set| FamilyEntry {
                name: set.sans_family_name().to_owned(),
                embedded: true,
            })
            .collect();
        let mut system: Vec<String> = system_database()
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .filter(|name| !embedded.iter().any(|entry| entry.name == *name))
            .collect();
        system.sort_by_key(|name| name.to_lowercase());
        system.dedup();
        embedded.extend(system.into_iter().map(|name| FamilyEntry {
            name,
            embedded: false,
        }));
        embedded
    })
}

/// A family offered by [`available_families`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyEntry {
    /// The family name, as Typst recognizes it.
    pub name: String,
    /// Embedded in StokesForge (hence present everywhere), as opposed to a
    /// system font.
    pub embedded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point 1 of the user request: with nothing specified, it is Noto.
    #[test]
    fn the_default_choice_when_nothing_is_requested_is_noto() {
        assert_eq!(FontChoice::default(), FontChoice::Embedded(FontSet::Noto));
        let resolved = resolve(&FontChoice::default());
        assert_eq!(resolved.source, FontSource::Embedded(FontSet::Noto));
        assert_eq!(resolved.math, FontSet::Noto);
        assert!(resolved
            .definitions
            .font_data
            .contains_key(FontSet::Noto.sans_family_name()));
    }

    /// A system font that cannot exist on any test machine (deliberately
    /// absurd name) must fall back to Noto without panicking or raising an
    /// error.
    #[test]
    fn a_missing_system_font_silently_falls_back_to_noto() {
        let name = "StokesForge Font That Cannot Exist — Test R.13";
        let resolved = resolve(&FontChoice::System(name.to_owned()));
        assert_eq!(
            resolved.source,
            FontSource::FallenBackToNoto {
                requested: name.to_owned()
            }
        );
        assert_eq!(resolved.math, FontSet::Noto);
        assert!(resolved
            .definitions
            .font_data
            .contains_key(FontSet::Noto.sans_family_name()));
        // The first choice of the proportional family must be Noto, not merely
        // present somewhere in the font table.
        assert_eq!(
            resolved
                .definitions
                .families
                .get(&FontFamily::Proportional)
                .and_then(|fonts| fonts.first())
                .map(String::as_str),
            Some(FontSet::Noto.sans_family_name())
        );
    }

    /// Each of the three embedded pairs resolves to its own font, text and
    /// Math consistent with each other.
    #[test]
    fn each_embedded_pair_resolves_to_itself() {
        for set in FontSet::ALL {
            let resolved = resolve(&FontChoice::Embedded(set));
            assert_eq!(resolved.source, FontSource::Embedded(set));
            assert_eq!(resolved.math, set);
            assert!(resolved
                .definitions
                .font_data
                .contains_key(set.sans_family_name()));
            assert_eq!(
                resolved
                    .definitions
                    .families
                    .get(&FontFamily::Proportional)
                    .and_then(|fonts| fonts.first())
                    .map(String::as_str),
                Some(set.sans_family_name())
            );
        }
    }

    /// The six embedded font files really are fonts — not empty bytes nor a
    /// placeholder: each must parse with `skrifa`, the same library `epaint`
    /// uses to lay them out.
    #[test]
    fn the_six_embedded_files_are_valid_fonts() {
        for set in FontSet::ALL {
            for bytes in [set.sans_bytes(), set.math_bytes()] {
                assert!(!bytes.is_empty(), "{set:?}: empty file");
                skrifa::FontRef::new(bytes)
                    .unwrap_or_else(|err| panic!("{set:?}: invalid font ({err})"));
            }
        }
    }

    /// The family names announced by [`FontSet`] match what the font file
    /// itself declares — otherwise `fontdb` (export side,
    /// `stokesforge-render-svg::pdf`) and `epaint` (screen side) would not
    /// agree on the name to look for.
    #[test]
    fn family_names_match_the_file_content() {
        use skrifa::raw::TableProvider;
        use skrifa::string::StringId;

        for set in FontSet::ALL {
            for (bytes, expected) in [
                (set.sans_bytes(), set.sans_family_name()),
                (set.math_bytes(), set.math_family_name()),
            ] {
                let font = skrifa::FontRef::new(bytes)
                    .unwrap_or_else(|err| panic!("{set:?}: invalid font ({err})"));
                let name = font.name().unwrap_or_else(|err| {
                    panic!("{set:?}: `name` table missing ({err})");
                });
                let found = name
                    .name_record()
                    .iter()
                    .filter(|record| {
                        record.name_id() == StringId::TYPOGRAPHIC_FAMILY_NAME
                            || record.name_id() == StringId::FAMILY_NAME
                    })
                    .any(|record| {
                        record
                            .string(name.string_data())
                            .map(|s| s.to_string() == expected)
                            .unwrap_or(false)
                    });
                assert!(
                    found,
                    "{set:?}: no family name of the file equals \"{expected}\""
                );
            }
        }
    }
}
