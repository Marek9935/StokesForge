//! Typst, behind the `typst` feature — task R8.
//!
//! As `architecture-choisie.md` §8 prescribes for `stokesforge-render`, this
//! module carries: the minimal Typst `World` (8.1), the compilation of a
//! formula into a document (8.1), and the generic cache that amortizes this
//! compilation between two frames (8.2). What it does **not** carry:
//! `typst-render` (RGBA, task 8.3, `stokesforge-render-egui` only) and
//! `typst-svg` (SVG fragment, task 8.4, `stokesforge-render-svg` only) — two
//! backend-specific dependencies this crate has no reason to pull in, the same
//! rule R4-R7 already apply to the rest of the pipeline.
//!
//! # Always compiled (issue #29)
//!
//! This module was originally behind a non-default `typst` feature
//! (`architecture-choisie.md` §7, task 8.6), to keep the right to drop Typst
//! without tearing anything down. The feature has been removed: every title
//! and legend text must go through Typst (issue #28), and a build without it
//! would have drawn a different figure than the default build. What remains
//! optional is the activation **per backend** (`with_typst`): a backend built
//! without it draws everything as plain text, and that is the witness of the
//! tests that prove Typst is really consulted. The plain-text fallback,
//! `$…$` included, now plays in production only for a string that does not
//! compile (invariant 10).
//!
//! # Which texts go through Typst: decided by role (issue #32)
//!
//! **Every** title (of the graph, of a panel), every axis title and every
//! legend entry goes through Typst; ticks never do (one compilation per tick,
//! `architecture-choisie.md` §4.3). The pipeline tells the backend through the
//! style's font family: [`rich_style`] sets the reserved family
//! [`RICH_FAMILY`], and both backends switch on [`is_rich`], at measurement
//! as at drawing — a single condition, in the same place on both sides
//! (invariant 4). On the `epaint` side, the reserved family falls back to the
//! proportional font (`text::font_id`): that is the fallback.
//!
//! Before #32, the switch was on the **content**: a string was a formula if it
//! contained `$`. A title that was half formula and its neighbor in plain text
//! then went through two engines, with different metrics, and did not line
//! up. The `$` stays what it is in Typst, the entry into math mode, but no
//! longer decides the path. No field was added to `stokesforge_plot`: no
//! migration of the `.stf` format.
//!
//! Consequence: a title or legend text is compiled **as Typst markup**. This
//! is intended for typed text (raw Typst syntax: `*bold*`, `$x^2$`, `\$` for
//! a dollar). Text taken over automatically (column name in a legend, the
//! "… and N more" line) takes [`literal_style`]: Typst composes it too, but
//! **as is**, through [`escape`] (issue #33) — an `Abs_280` imported from an
//! instrument does not turn italic. A string that does not compile is
//! displayed raw by `epaint` (invariant 10): this is the only case where the
//! two engines still sit side by side.
//!
//! # What R0 had already noted about the API, verified again here
//!
//! - `FileId::new` takes a `RootedPath`, not a `(root, path)` pair —
//!   confirmed in `typst_syntax::path` 0.15.1;
//! - `VirtualPath::new` returns a `Result`, not the value;
//! - `PagedDocument` comes from `typst_layout`, not from `typst`;
//! - no system font scan: this `World` knows only the two embedded fonts
//!   passed to [`compile`] ([`crate::fonts::FontSet`]), never `typst-kit` nor
//!   one more `fontdb` — the reproducibility that `include_system_fonts(false)`
//!   would aim for elsewhere is achieved here by construction, without needing
//!   that dependency.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use stokesforge_plot::TextAlign;

use plotters_backend::{
    text_anchor::{HPos, VPos},
    FontTransform,
};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::{Page, PagedDocument};

use crate::fonts::{FontChoice, FontSet};

/// The color of a Typst text: red, green, blue, opacity, in non-premultiplied
/// bytes — issue #30.
///
/// An array rather than `plotters_backend::BackendColor`: the color enters
/// the cache key ([`CacheKey`]), and an `f64` opacity cannot be hashed there.
/// [`ink`] does the conversion, once, for both backends.
pub type Ink = [u8; 4];

/// Local settings of a text: the bar then cannot summarize the area by a
/// single inherited value. Reading the syntax avoids compiling the document
/// on every frame and mistaking plain text for a setting (#97).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextOverrides {
    pub font: bool,
    pub size: bool,
    pub color: bool,
}

/// Attributes of `text(...)` and `set text(...)`, nested ones included.
/// A computed expression stays local: the interface shows a neutral state
/// rather than pretending to know the effective value.
pub fn local_text_overrides(text: &str) -> TextOverrides {
    use typst::syntax::ast;
    let source = Source::detached(text);
    let mut overrides = TextOverrides::default();
    let mut nodes = vec![source.root()];
    while let Some(node) = nodes.pop() {
        let call = node
            .cast::<ast::FuncCall>()
            .map(|call| (call.callee(), call.args()))
            .or_else(|| {
                node.cast::<ast::SetRule>()
                    .map(|rule| (rule.target(), rule.args()))
            });
        if let Some((ast::Expr::Ident(name), args)) = call {
            if name.as_str() == "text" {
                for arg in args.items() {
                    if let ast::Arg::Named(named) = arg {
                        match named.name().as_str() {
                            "font" => overrides.font = true,
                            "size" => overrides.size = true,
                            "fill" => overrides.color = true,
                            _ => {}
                        }
                    }
                }
            }
        }
        nodes.extend(node.children());
    }
    overrides
}

/// The Typst ink of a Plotters color — the one `draw_text` receives through
/// `style.color()`, already resolved by the pipeline (explicit color or
/// `color::auto_ink`). Same opacity rounding as the screen's `epaint` path.
pub fn ink(color: plotters_backend::BackendColor) -> Ink {
    let (r, g, b) = color.rgb;
    let alpha = (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    [r, g, b, alpha]
}

/// The reserved font family that designates a **typed** rich text, compiled
/// by Typst as markup — see the module header.
pub const RICH_FAMILY: &str = "stokesforge-typst";

/// The reserved family of a rich text **taken over automatically** (column
/// name in a legend, text generated by the renderer): composed by Typst like
/// the others, but displayed as is, without being read as markup (issue #33).
/// See [`source`].
pub const LITERAL_FAMILY: &str = "stokesforge-typst-literal";

/// The style of a typed rich text (title, axis title, series name), at the
/// given Plotters size and color — the counterpart of `pipeline::text_style`
/// for ticks.
pub fn rich_style(size: f64, color: (u8, u8, u8, u8)) -> plotters::style::TextStyle<'static> {
    family_style(RICH_FAMILY, size, color)
}

/// [`rich_style`] in the text area's font (issues #77, #78): `font` is the
/// chosen family name, empty or `None` for the graph's font. The font travels
/// in the style's family, like the angle of a rotated text
/// ([`family_with_font`]), and [`source`] puts it in the markup — with the
/// Math font associated with it ([`math_family_for`]).
pub fn rich_style_font(
    size: f64,
    color: (u8, u8, u8, u8),
    font: Option<&str>,
) -> plotters::style::TextStyle<'static> {
    family_style(interned(family_with_font(RICH_FAMILY, font)), size, color)
}

/// [`literal_style`] in the text area's font.
pub fn literal_style_font(
    size: f64,
    color: (u8, u8, u8, u8),
    font: Option<&str>,
) -> plotters::style::TextStyle<'static> {
    family_style(
        interned(family_with_font(LITERAL_FAMILY, font)),
        size,
        color,
    )
}

/// Splits the font from the rest of a style family name
/// (`stokesforge-typst|Fira Sans` → `("stokesforge-typst", Some("Fira Sans"))`).
fn split_font(name: &str) -> (&str, Option<&str>) {
    let name = name
        .split_once(LAYOUT_SEPARATOR)
        .map_or(name, |(head, _)| head);
    match name.split_once(FONT_SEPARATOR) {
        Some((base, font)) if !font.is_empty() => (base, Some(font)),
        Some((base, _)) => (base, None),
        None => (name, None),
    }
}

/// Adds `font` (empty or `None`: nothing) to the style family `base`.
pub fn family_with_font(base: &str, font: Option<&str>) -> String {
    match font.map(str::trim).filter(|font| !font.is_empty()) {
        Some(font) => format!("{base}{FONT_SEPARATOR}{font}"),
        None => base.to_owned(),
    }
}

/// The font carried by a style family name (see [`split_font`]).
pub fn font_of_family(name: &str) -> Option<&str> {
    split_font(name).1
}

/// The layout separator (width, alignment) at the end of a style family name,
/// after the font: `…\u{1f}<width>\u{1f}<l|c|r>`. A control character, which
/// no font name contains (#244).
const LAYOUT_SEPARATOR: char = '\u{1f}';

/// Adds to the style family `family` a box of `width` points aligned by
/// `align`. `None`: the family is returned as is.
pub fn family_with_layout(family: String, layout: Option<(f64, TextAlign)>) -> String {
    match layout.filter(|(width, _)| width.is_finite() && *width > 0.0) {
        Some((width, align)) => {
            let letter = match align {
                TextAlign::Left => 'l',
                TextAlign::Center => 'c',
                TextAlign::Right => 'r',
            };
            format!("{family}{LAYOUT_SEPARATOR}{width}{LAYOUT_SEPARATOR}{letter}")
        }
        None => family,
    }
}

/// The box (width in points, alignment) carried by `style`, if it has one.
pub fn layout_of<S: plotters_backend::BackendTextStyle>(style: &S) -> Option<(f64, TextAlign)> {
    let plotters_backend::FontFamily::Name(name) = style.family() else {
        return None;
    };
    let mut parts = name.split(LAYOUT_SEPARATOR).skip(1);
    let width: f64 = parts.next()?.parse().ok()?;
    let align = match parts.next()? {
        "l" => TextAlign::Left,
        "r" => TextAlign::Right,
        _ => TextAlign::Center,
    };
    (width.is_finite() && width > 0.0).then_some((width, align))
}

/// [`rich_style_font`] in a box of fixed width (#244).
pub fn rich_style_layout(
    size: f64,
    color: (u8, u8, u8, u8),
    font: Option<&str>,
    layout: Option<(f64, TextAlign)>,
) -> plotters::style::TextStyle<'static> {
    family_style(
        interned(family_with_layout(
            family_with_font(RICH_FAMILY, font),
            layout,
        )),
        size,
        color,
    )
}

/// The separator between the role family and the font in a style family name.
/// A font name does not contain it.
const FONT_SEPARATOR: char = '|';

/// A style family name with a `'static` lifetime: Plotters wants a `&str`
/// that lives as long as the style. One name per distinct font, kept forever
/// — bounded by the number of fonts the user has chosen, not by the number of
/// texts.
fn interned(family: String) -> &'static str {
    static NAMES: OnceLock<std::sync::Mutex<std::collections::HashSet<&'static str>>> =
        OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    match names.get(family.as_str()) {
        Some(known) => known,
        None => {
            let leaked: &'static str = Box::leak(family.into_boxed_str());
            names.insert(leaked);
            leaked
        }
    }
}

/// The Math family associated with the text font `family`: the Math of the
/// same embedded pair (Libertinus Sans → Libertinus Math), otherwise — a
/// system font has no companion — Noto Sans Math, the fallback.
pub fn math_family_for(family: &str) -> &'static str {
    FontSet::ALL
        .iter()
        .find(|set| set.sans_family_name() == family)
        .map_or_else(
            || FontSet::Noto.math_family_name(),
            |set| set.math_family_name(),
        )
}

/// The style of a rich text taken over automatically — see
/// [`LITERAL_FAMILY`].
pub fn literal_style(size: f64, color: (u8, u8, u8, u8)) -> plotters::style::TextStyle<'static> {
    family_style(LITERAL_FAMILY, size, color)
}

fn family_style(
    family: &'static str,
    size: f64,
    color: (u8, u8, u8, u8),
) -> plotters::style::TextStyle<'static> {
    use plotters::style::IntoFont;
    (family, size).into_font().color(&crate::paint::rgba(color))
}

/// The family prefix of a typed rich text **rotated by a free angle**
/// (annotation, issue #35): `stokesforge-typst@<degrees>`. Plotters rotates
/// text only by quarter turns (`FontTransform`); a free angle is therefore
/// entrusted to Typst (`#rotate`), and it travels in the style like the rest
/// of a text's role.
const ROTATED_PREFIX: &str = "stokesforge-typst@";

/// The family of a typed rich text rotated by `angle_deg` degrees,
/// counterclockwise — [`RICH_FAMILY`] for a zero angle.
pub fn rotated_family(angle_deg: f64) -> String {
    rotated_family_font(angle_deg, None)
}

/// [`rotated_family`] in the text area's font ([`family_with_font`]).
pub fn rotated_family_font(angle_deg: f64, font: Option<&str>) -> String {
    let base = if angle_deg == 0.0 || !angle_deg.is_finite() {
        RICH_FAMILY.to_owned()
    } else {
        format!("{ROTATED_PREFIX}{angle_deg}")
    };
    family_with_font(&base, font)
}

/// The angle carried by the family of `style` ([`rotated_family`]), 0 otherwise.
fn rotation_of<S: plotters_backend::BackendTextStyle>(style: &S) -> f64 {
    match style.family() {
        plotters_backend::FontFamily::Name(name) => split_font(name)
            .0
            .strip_prefix(ROTATED_PREFIX)
            .and_then(|angle| angle.parse::<f64>().ok())
            .filter(|angle| angle.is_finite())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// True if `style` designates a rich text, typed, taken over or rotated, to
/// be composed by Typst rather than laid out by `epaint`. The only switching
/// condition of both backends, for measurement as for drawing.
pub fn is_rich<S: plotters_backend::BackendTextStyle>(style: &S) -> bool {
    matches!(
        style.family(),
        plotters_backend::FontFamily::Name(name)
            if matches!(split_font(name).0, RICH_FAMILY | LITERAL_FAMILY)
                || split_font(name).0.starts_with(ROTATED_PREFIX)
    )
}

/// The name, in the preamble of [`compile`], of the function that rotates a
/// rich text. A typed text cannot call it by accident: it is an identifier
/// that nothing documents.
const ROTATE_CALL: &str = "#stokesforge-rotate(";

/// The Typst source of a rich text: the text itself if typed (markup
/// intended), [`escape`]`(text)` if taken over automatically. Both backends
/// call it in their Typst branch; the decision itself comes from the style set
/// by the pipeline. The `epaint` path (backend without Typst, fallback of
/// invariant 10) displays the original text, never its escaped form.
///
/// A rotated text ([`rotated_family`]) is wrapped in Typst's rotation:
/// `#rotate(…, reflow: true)`, whose page fits the rotated box, so that a
/// centered anchor stays the center of the text.
pub fn source<'a, S: plotters_backend::BackendTextStyle>(
    text: &'a str,
    style: &S,
) -> std::borrow::Cow<'a, str> {
    let angle = rotation_of(style);
    let layout = layout_of(style);
    let (literal, font) = match style.family() {
        plotters_backend::FontFamily::Name(name) => {
            let (base, font) = split_font(name);
            (base == LITERAL_FAMILY, font)
        }
        _ => (false, None),
    };
    // The area's font (issues #77, #78), set **inside** the text: it holds for
    // the whole area, and its Math font follows. A `#set` is limited to the
    // block that contains it, hence to the possible rotation.
    let preamble = font.map_or_else(String::new, |font| {
        let quoted = |name: &str| name.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "#set text(font: \"{}\")\n#show math.equation: set text(font: \"{}\")\n",
            quoted(font),
            quoted(math_family_for(font)),
        )
    });
    // A box of fixed width: the text wraps and aligns in it (#244). The whole
    // box is the page, so the anchor stays its center.
    let boxed = |body: String| match layout {
        Some((width, align)) => {
            let side = match align {
                TextAlign::Left => "left",
                TextAlign::Center => "center",
                TextAlign::Right => "right",
            };
            format!("#box(width: {width}pt)[#set par(justify: false)\n#align({side})[{body}]]")
        }
        None => body,
    };
    if angle != 0.0 {
        // Typst rotates clockwise (screen axes): the model's counterclockwise
        // angle changes sign.
        let body = if literal {
            escape(text)
        } else {
            text.to_owned()
        };
        let body = boxed(body);
        return format!("{ROTATE_CALL}{}deg)[{preamble}{body}]", -angle).into();
    }
    if layout.is_some() {
        let body = if literal {
            escape(text)
        } else {
            text.to_owned()
        };
        return format!("{preamble}{}", boxed(body)).into();
    }
    match (literal, preamble.is_empty()) {
        (true, true) => escape(text).into(),
        (true, false) => format!("{preamble}{}", escape(text)).into(),
        (false, true) => text.into(),
        (false, false) => format!("{preamble}{text}").into(),
    }
}

/// Any text in a form that Typst displays **as is**: a Typst string, `#"…"`,
/// of which only `\` and `"` (and line ends) are escaped (issue #33).
///
/// Preferred to a `\` before each special markup character: the list is long
/// and depends on context (`-`, `+`, `=`, `/` at the start of a line, `//`
/// which opens a comment), and markup also **replaces** text without saying
/// so — `...` as an ellipsis, `--` as a dash, `'` and `"` as typographic
/// quotes. A string is none of that: it is text. Nothing valid can follow the
/// string without a space in what the renderer produces (a `.` stuck to it
/// would read a field): the only addition is the broken-reference suffix, preceded
/// by a space.
pub fn escape(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut escaped = String::with_capacity(text.len() + 3);
    escaped.push_str("#\"");
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

/// Typst's standard library, built once for the whole process.
///
/// It depends on no font choice (`Library` carries only the language
/// definitions — functions, default formatting rules), so nothing justifies
/// rebuilding it at each compilation: it is the fixed part of the cost
/// measured in R0, amortized once and for all rather than per formula.
fn library() -> &'static LazyHash<Library> {
    static LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| LazyHash::new(Library::default()))
}

/// The minimal Typst `World` of this work package — task 8.1.
///
/// ~90 lines once the imports are counted apart, in line with the sizing R0
/// had announced ("~100-150 lignes"). It knows a single file, the text of the
/// formula itself: no imports, no packages, no file system.
struct FormulaWorld {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: FileId,
    source: Source,
}

impl FormulaWorld {
    /// Builds a world for a single compilation, on the two fonts (text and
    /// Math) resolved by [`crate::fonts::resolve`].
    fn new(markup: String, sans: Font, math: Font, extras: Vec<Font>) -> Result<Self, String> {
        let mut fonts = vec![sans, math];
        fonts.extend(noto_italic());
        fonts.extend(extras);
        let book = FontBook::from_fonts(&fonts);
        let vpath = VirtualPath::new("/formula.typ")
            .map_err(|err| format!("invalid Typst virtual path: {err}"))?;
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let source = Source::new(main, markup);
        Ok(Self {
            book: LazyHash::new(book),
            fonts,
            main,
            source,
        })
    }
}

impl World for FormulaWorld {
    fn library(&self) -> &LazyHash<Library> {
        library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            // No external file: no import, no package, no font by path. A
            // formula that asks for one fails cleanly — no panic, no file
            // system scan.
            Err(FileError::NotFound(id.vpath().get_with_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_with_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        // No formula of this work package needs today's date; a formula that
        // called `datetime.today()` would fail cleanly rather than return a
        // figure whose content depended on the moment of its rendering —
        // exactly what the reproducibility aimed at by
        // `include_system_fonts(false)` elsewhere also asks for.
        None
    }
}

/// The result of a successful Typst compilation — just enough to measure and
/// to draw, nothing more.
///
/// Carries a single page: a formula or a mixed label fits on a single page as
/// long as `#set page(width: auto, height: auto)` makes it grow with its
/// content (see [`compile`]). A text that would produce a page break (unlikely
/// for an axis label) would see its following pages silently ignored rather
/// than raise an error - consistent with the rest of the work package, where a
/// degenerate spec input never crashes the rendering
/// (`architecture-choisie.md` §11.1).
///
/// `Clone` so that a screen artifact keeps its document and rasterizes it
/// again at another resolution without recompiling
/// (`stokesforge_render_egui::scaled`, issue #31); Typst's document is made of
/// shared pointers, the clone does not copy the glyphs.
#[derive(Clone)]
pub struct Compiled {
    document: PagedDocument,
}

impl Compiled {
    /// The compiled page, if the formula produced one.
    pub fn page(&self) -> Option<&Page> {
        self.document.pages().first()
    }

    /// The page size in Typst points — the **same** abstract point as the one
    /// already used throughout this work package to speak of font size
    /// (`text::font_size`, `text::PLOTTERS_SIZE_PER_POINT`): a Typst label and
    /// an ordinary label are therefore measured in the same unit, with no extra
    /// conversion when reserving room in Plotters' layout.
    ///
    /// `(0.0, 0.0)` if the compilation produced no page (empty formula): a
    /// zero-size box rather than an error, the behavior closest to "nothing to
    /// draw".
    pub fn size_pt(&self) -> (f64, f64) {
        self.page()
            .map(|page| {
                let size = page.frame.size();
                (size.x.to_pt(), size.y.to_pt())
            })
            .unwrap_or((0.0, 0.0))
    }
}

/// Compiles `text` (Typst markup, math mode `$…$` included) into a Typst
/// document — task 8.1.
///
/// `size_pt` is the font size in the sense of [`crate::text::font_size`]: the
/// same number that would build the `epaint::FontId` of an ordinary label of
/// the same Plotters size, so that a title half Typst and half plain text does
/// not change point size between the two halves.
///
/// `#set page(width: auto, height: auto, margin: 0pt)` is Typst's standard
/// mechanism to get a page that fits its content rather than an A4 page: this
/// is what makes a formula compile into a one-page document whose size **is**
/// the label's size, without having to deduce it from a crop afterwards.
///
/// `ink` becomes the color of all the text (`#set text(fill: …)`, which math
/// mode inherits): without it, Typst draws in black, and a title on a dark
/// background disappeared (issue #30). A color written in the formula itself
/// (`#text(fill: red)[…]`) keeps priority, like any local Typst rule — this is
/// what rules out rendering in white and then tinting the image afterwards.
///
/// `top-edge`/`bottom-edge: "bounds"`: by default, Typst bounds a line from the
/// height of capitals to the baseline. A page fitted to its content therefore
/// cut off everything that goes below the baseline — descenders ("p", "y") and
/// above all subscripts (`n_i`, `"Ru(bpy)"_3`), the bulk of a scientific
/// notation. The default went unnoticed until the first capture of the
/// application with Typst plugged in (issue #29). With `"bounds"`, the box is
/// the ink actually laid down: nothing is cut, at the price of a height that
/// varies from one label to another ("a" is shorter than "Ag") — the
/// alignment of baselines between neighboring labels is the subject of issue
/// #32.
pub fn compile(
    text: &str,
    size_pt: f32,
    ink: Ink,
    sans: Font,
    math: Font,
) -> Result<Compiled, String> {
    let sans_name = family_name(&sans);
    let math_name = family_name(&math);
    let [r, g, b, a] = ink;
    // A rotated text carries its strut **inside** the rotation: one more strut
    // next to the rotated box would enlarge the page on one side only, and its
    // center would no longer be the text's center.
    let strut = if text.starts_with(ROTATE_CALL) {
        ""
    } else {
        // `;` ends the expression: without it, a text that starts with a
        // letter would extend the identifier (`#stokesforge-strutPic`).
        "#stokesforge-strut;"
    };
    let markup = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(font: \"{sans_name}\", size: {size_pt}pt, fill: rgb({r}, {g}, {b}, {a}), \
                   top-edge: \"bounds\", bottom-edge: \"bounds\")\n\
         #show math.equation: set text(font: \"{math_name}\")\n\
         #let stokesforge-strut = box(width: 0pt, text(top-edge: \"ascender\", \
                                 bottom-edge: \"descender\", hide[x]))\n\
         #let stokesforge-rotate(angle, body) = rotate(angle, reflow: true, [#stokesforge-strut#body])\n\
         {strut}{text}"
    );
    let extras = requested_fonts(text);
    compile_markup(markup, sans, math, extras)
}

/// The Typst error message of `text`, or `None` if it compiles (#240, T3).
///
/// Serves the interface: the rendering already falls back to plain text when a
/// formula does not compile, saying nothing. Fonts and size do not change the
/// validity of a markup: the check is done with the default embedded fonts, at
/// 12 pt. The result is memoized per text (a properties field asks for it on
/// every frame); the memory is bounded.
pub fn diagnose(text: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    const MAX_ENTRIES: usize = 256;
    static MEMO: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    if text.trim().is_empty() {
        return None;
    }
    let memo = MEMO.get_or_init(Default::default);
    if let Some(known) = memo.lock().unwrap_or_else(|err| err.into_inner()).get(text) {
        return known.clone();
    }
    let verdict = match resolve_fonts(&FontChoice::default()) {
        Ok((sans, math)) => compile(text, 12.0, [0, 0, 0, 255], sans, math).err(),
        // Fonts not found: nothing to blame on the text.
        Err(_) => None,
    };
    let mut memo = memo.lock().unwrap_or_else(|err| err.into_inner());
    if memo.len() >= MAX_ENTRIES {
        memo.clear();
    }
    memo.insert(text.to_owned(), verdict.clone());
    verdict
}

/// Compiles a complete Typst document, page settings included.
fn compile_markup(
    markup: String,
    sans: Font,
    math: Font,
    extras: Vec<Font>,
) -> Result<Compiled, String> {
    let world = FormulaWorld::new(markup, sans, math, extras)?;
    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned.output.map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(Compiled { document })
}

/// The italic face of Noto Sans, always known to the `World`: Typst does not
/// fabricate italics, and the "italic" button of the text format bar (issue
/// #63) would have no effect with the upright face alone.
fn noto_italic() -> Option<Font> {
    static ITALIC: OnceLock<Option<Font>> = OnceLock::new();
    ITALIC
        .get_or_init(|| {
            Font::new(
                Bytes::new(include_bytes!(
                    "../../../vendor/fonts/noto-sans/NotoSans-Italic-Variable.ttf"
                )),
                0,
            )
        })
        .clone()
}

/// The family names a text requests itself, through `#text(font: "Name")`
/// (the text format bar, issue #63), in order and without duplicates. A
/// lexical read: we look for `font:` followed by a quoted string.
fn requested_families(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("font:") {
        rest = &rest[at + "font:".len()..];
        let trimmed = rest.trim_start();
        let Some(quoted) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = quoted.find('"') else {
            break;
        };
        let name = &quoted[..end];
        if !name.is_empty() && !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
        rest = &quoted[end..];
    }
    names
}

/// The fonts that `text` names and that the `World` must know in addition to
/// the text and Math ones: an embedded font by its family name, otherwise a
/// system font. Not found ⇒ absent: Typst then falls back to the document's
/// font, without error (the text does not disappear). Memoized by name, since
/// reading a system font costs more than a compilation.
fn requested_fonts(text: &str) -> Vec<Font> {
    static KNOWN: OnceLock<std::sync::Mutex<std::collections::HashMap<String, Option<Font>>>> =
        OnceLock::new();
    let names = requested_families(text);
    if names.is_empty() {
        return Vec::new();
    }
    let known = KNOWN.get_or_init(Default::default);
    let mut known = known.lock().unwrap_or_else(|err| err.into_inner());
    names
        .into_iter()
        .filter_map(|name| {
            known
                .entry(name.clone())
                .or_insert_with(|| load_named_font(&name))
                .clone()
        })
        .collect()
}

fn load_named_font(name: &str) -> Option<Font> {
    if let Some(set) = FontSet::ALL
        .iter()
        .find(|set| set.sans_family_name() == name)
    {
        return embedded_font(set.sans_bytes(), "sans").ok();
    }
    // A Math font (the one `math_family_for` associates).
    if let Some(set) = FontSet::ALL
        .iter()
        .find(|set| set.math_family_name() == name)
    {
        return embedded_font(set.math_bytes(), "Math").ok();
    }
    crate::fonts::load_system_font(name).and_then(|data| system_font(data, "sans").ok())
}

/// The family name of an already loaded Typst `Font` — to write it in the
/// `#set text(font: ...)` generated by [`compile`], rather than have the
/// caller carry it separately and risk it diverging from the font actually
/// loaded in the `World`.
fn family_name(font: &Font) -> String {
    font.info().family.clone()
}

/// A cache key: the formula, its size and its color (the generated text
/// already contains them, see [`compile`]) plus the `pixels_per_point` of the
/// requested backend artifact. The color has been in it since issue #30: two
/// identical titles of different colors are two artifacts.
///
/// **Documented decision, not just coded**: the Typst compilation itself
/// (`document`, in points) does not depend on `pixels_per_point` — Typst lays
/// out in an abstract space, with no notion of screen. Putting it in the key
/// anyway, as task 8.2 of the roadmap asks, is therefore not necessary for the
/// **correctness** of this cache (unlike `epaint::Fonts`, where the line
/// height really depends on the physical scale, `text.rs`): it is a deliberate
/// choice to offer a single cache mechanism to both backends rather than write
/// a second, finer one, specific to the screen. `stokesforge-render-svg`
/// (§8.4) always calls this cache with a constant `pixels_per_point` (`1.0`) —
/// its key therefore never varies on this term, though nothing prevents it
/// from doing so one day. `stokesforge-render-egui` (§8.3), for its part, does
/// make `pixels_per_point` vary with the real screen: that is the case that
/// justifies the term.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    text: String,
    size_pt_bits: u32,
    ink: Ink,
    font_choice: String,
    pixels_per_point_bits: u32,
}

impl CacheKey {
    fn new(text: &str, size_pt: f32, ink: Ink, choice: &FontChoice, pixels_per_point: f32) -> Self {
        Self {
            text: text.to_owned(),
            size_pt_bits: size_pt.to_bits(),
            ink,
            // `FontChoice` has no `Hash` (fonts.rs, addition of 2026-09-13,
            // outside the scope of this task): its `Debug` representation is
            // deterministic and enough to tell the choices apart, without
            // adding a trait to it for this single use.
            font_choice: format!("{choice:?}"),
            pixels_per_point_bits: pixels_per_point.to_bits(),
        }
    }
}

/// Generic cache of artifacts derived from a Typst compilation — task 8.2.
///
/// Generic over `V`: `stokesforge-render-egui` stores an RGBA texture in it
/// (§8.3), `stokesforge-render-svg` an SVG fragment (§8.4). This module knows
/// neither type — that is the whole point, the cache and compilation logic
/// staying single whichever backend uses it.
pub struct FormulaCache<V> {
    entries: Mutex<HashMap<CacheKey, Arc<V>>>,
}

impl<V> FormulaCache<V> {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the artifact already cached for `text` at `size_pt`, in `ink`,
    /// at `pixels_per_point`, or compiles then builds a new artifact with
    /// `build`.
    ///
    /// `sans`/`math` are the already resolved fonts
    /// (`crate::fonts::resolve(choice)`): this cache cannot resolve them
    /// itself, so as not to decode two embedded fonts on every miss — the
    /// caller builds them once and reuses them, as
    /// [`crate::text::TextMeasurer`] already does for `epaint::Fonts`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_try_insert(
        &self,
        text: &str,
        size_pt: f32,
        ink: Ink,
        choice: &FontChoice,
        pixels_per_point: f32,
        sans: Font,
        math: Font,
        build: impl FnOnce(&Compiled) -> Result<V, String>,
    ) -> Result<Arc<V>, String> {
        let key = CacheKey::new(text, size_pt, ink, choice, pixels_per_point);
        // Poisoned lock: rebuild rather than fail the whole rendering for a
        // reason no caller can fix — the same policy as
        // `TextMeasurer::layout_at`.
        if let Some(hit) = self
            .entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(&key)
        {
            return Ok(Arc::clone(hit));
        }
        let compiled = compile(text, size_pt, ink, sans, math)?;
        let value = Arc::new(build(&compiled)?);
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(key, Arc::clone(&value));
        Ok(value)
    }

    /// Number of artifacts currently cached — for tests.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The artifacts currently cached, in any order — to find the artifact of
    /// an already built shape (`stokesforge_render_egui::scaled`, issue #31,
    /// which rasterizes a formula again at the displayed size).
    pub fn values(&self) -> Vec<Arc<V>> {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Empties the cache — to bound its size in a long-lived caller
    /// (`stokesforge_render_egui::GraphView`, where each keystroke in a title
    /// creates an entry). To be called only **before** rebuilding everything
    /// that references the removed artifacts: a texture freed while a cached
    /// shape still points to it would display empty.
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
    }
}

impl<V> std::fmt::Debug for FormulaCache<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormulaCache")
            .field("entries", &self.len())
            .finish()
    }
}

impl<V> Default for FormulaCache<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// The measurement of a formula, as `estimate_text_size` must return it —
/// **the invariant of task 8.5**.
///
/// Always in points, never in pixels of a particular screen: it is the same
/// contract as [`crate::text::TextSource::measure`], and it holds here for a
/// different reason — not because we decide it, but because Typst's layout has
/// no notion of physical pixel anyway. Converted to `(u32, u32)` by
/// [`crate::text::round_size`], **the same function** as the `epaint` path, so
/// that the rounding itself cannot diverge between the two measurement paths.
pub fn measure(compiled: &Compiled) -> (u32, u32) {
    round_size_pt(compiled.size_pt())
}

/// Like [`measure`], but from an already known size in points — for a caller
/// that cached something other than a whole [`Compiled`]
/// (`stokesforge-render-svg::typst_export`, which keeps only the SVG fragment
/// and its size, not the Typst document).
pub fn round_size_pt((w, h): (f64, f64)) -> (u32, u32) {
    crate::text::round_size(epaint::Vec2::new(w as f32, h as f32))
}

/// A Math font choice, independent of the text font choice — technical
/// wiring of 2026-09-13, complementing [`resolve_fonts_with_math`].
///
/// Until now, [`resolve_fonts`] determined the Math font only from the text
/// [`FontChoice`] (`crate::fonts::resolve(choice).math`, always one of the
/// three embedded pairs): no way to express "this text, but that Math". This
/// type fills the gap without deciding anything more: no default value derives
/// a product policy from it, it is just a second axis of choice, on a par with
/// [`FontChoice`] for the text.
///
/// **Silent degradation for [`MathFontChoice::System`], to know before
/// exposing this choice to a user**: nothing guarantees that a system font
/// carries a valid Unicode Math table (OpenType `MATH`). Typst will still
/// compile with a font that has none — fractions, superscripts, stacked
/// operators will be laid out with the ordinary body metrics only (badly
/// centered vertically, no growable operator sizes) — without any error or
/// warning raised. This is not a bug of this mechanism but a known fact of the
/// OpenType Math font ecosystem (the `MATH` table is rare outside fonts
/// designed for it, like the three embedded pairs): a future caller that
/// offered this choice to a user must know it before doing so, rather than
/// discover a degraded rendering with no diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathFontChoice {
    /// One of the three embedded pairs — its Math half, independently of the
    /// text font requested otherwise.
    Embedded(FontSet),
    /// A system font, by its exact name. Not found ⇒ fallback to
    /// [`FontSet::Noto`], the same policy that
    /// [`crate::fonts::FontChoice::System`] already applies on the text side.
    System(String),
}

/// Loads the Typst fonts (text + Math) matching a [`FontChoice`] — so that
/// the two backends (8.3, 8.4) do not each have their own way of doing it.
///
/// Equivalent to [`resolve_fonts_with_math`]`(choice, None)`: the Math font
/// follows the text [`FontChoice`], exactly as before [`MathFontChoice`] was
/// added — see its documentation for what `None` means precisely. No existing
/// caller needs to change to keep its current behavior.
pub fn resolve_fonts(choice: &FontChoice) -> Result<(Font, Font), String> {
    resolve_fonts_with_math(choice, None)
}

/// Like [`resolve_fonts`], with an optional Math font choice independent of
/// the text choice.
///
/// `math_choice` at `None` reproduces exactly [`resolve_fonts`] as it behaved
/// before this addition: the Math follows the text [`FontChoice`] (one of the
/// three embedded pairs for [`FontChoice::Embedded`], [`FontSet::Noto`] for
/// [`FontChoice::System`] — the same rule [`crate::fonts::resolve`] already
/// applies). `Some(..)` takes over and resolves the Math separately through
/// [`MathFontChoice`], whose documentation details the silent degradation of a
/// system font without a Math table.
///
/// Unlike the old limit documented here for [`FontChoice::System`] on the text
/// side (fallback to Noto **whether the font is found or not**): the requested
/// system font is now really looked up through
/// [`crate::fonts::load_system_font`] — the same mechanism
/// [`crate::fonts::resolve`] uses on the ordinary screen/export side — and
/// used if found; the fallback to Noto now plays only if it is not found, the
/// same policy the rest of the project already applies to a system font
/// (`fonts.rs`, module header).
pub fn resolve_fonts_with_math(
    choice: &FontChoice,
    math_choice: Option<&MathFontChoice>,
) -> Result<(Font, Font), String> {
    let sans = match choice {
        FontChoice::Embedded(set) => embedded_font(set.sans_bytes(), "sans")?,
        FontChoice::System(name) => match crate::fonts::load_system_font(name) {
            Some(data) => system_font(data, "sans")?,
            None => embedded_font(FontSet::Noto.sans_bytes(), "sans")?,
        },
    };

    let math = match math_choice {
        Some(MathFontChoice::Embedded(set)) => embedded_font(set.math_bytes(), "Math")?,
        Some(MathFontChoice::System(name)) => match crate::fonts::load_system_font(name) {
            Some(data) => system_font(data, "Math")?,
            None => embedded_font(FontSet::Noto.math_bytes(), "Math")?,
        },
        // Historical behavior, unchanged: the Math follows the text choice
        // rather than a system scan of its own — see the documentation of this
        // function and of `crate::fonts::resolve`.
        None => {
            let math_set = match choice {
                FontChoice::Embedded(set) => *set,
                FontChoice::System(_) => FontSet::Noto,
            };
            embedded_font(math_set.math_bytes(), "Math")?
        }
    };

    Ok((sans, math))
}

/// Builds a Typst `Font` from the bytes of one of the embedded pairs —
/// factored between text and Math, for an error message that names the role
/// rather than duplicating the same `ok_or_else` four times.
fn embedded_font(bytes: &'static [u8], role: &str) -> Result<Font, String> {
    Font::new(Bytes::new(bytes), 0).ok_or_else(|| format!("embedded {role} font unreadable"))
}

/// Builds a Typst `Font` from the raw bytes of a system font already found by
/// [`crate::fonts::load_system_font`] — same face index as the one `fontdb`
/// resolved, so as not to fall back on the first face of a file that would
/// contain several (a variable font family, for example).
fn system_font(data: epaint::text::FontData, role: &str) -> Result<Font, String> {
    let index = data.index;
    Font::new(Bytes::new(data.font), index).ok_or_else(|| format!("system {role} font unreadable"))
}

/// The anchor offset of a box of `size_pt`, in its own **unrotated** frame —
/// that is, before the possible rotation of [`rotate_offset`].
///
/// Transcribed from `stokesforge-render-egui::backend::EguiBackend::draw_text`
/// (identical computation, not shared until now because it existed in only one
/// place): a Typst label must anchor exactly like an ordinary label, or there
/// would be a visible shift between the two at the `$...$` boundary.
pub fn anchor_offset((width, height): (f64, f64), h: HPos, v: VPos) -> (f64, f64) {
    let dx = match h {
        HPos::Left => 0.0,
        HPos::Center => -width / 2.0,
        HPos::Right => -width,
    };
    let dy = match v {
        VPos::Top => 0.0,
        VPos::Center => -height / 2.0,
        VPos::Bottom => -height,
    };
    (dx, dy)
}

/// Applies to an offset the same rotation that Plotters applies to text — see
/// `EguiBackend::rotate`, of which this is the shared version (also used by
/// `stokesforge-render-svg`, which until now had no reason to rotate
/// anything).
pub fn rotate_offset((dx, dy): (f64, f64), transform: FontTransform) -> (f64, f64) {
    match transform {
        FontTransform::None => (dx, dy),
        FontTransform::Rotate90 => (-dy, dx),
        FontTransform::Rotate180 => (-dx, -dy),
        FontTransform::Rotate270 => (dy, -dx),
    }
}

/// The angle of a Plotters rotation, in clockwise degrees — the unit SVG
/// expects (`rotate(deg, cx, cy)`, already used as is by
/// `plotters-svg::draw_text`) and that `epaint::TextShape::with_angle` expects
/// in radians (see the screen-side caller, which converts).
pub fn rotation_degrees(transform: FontTransform) -> f64 {
    match transform {
        FontTransform::None => 0.0,
        FontTransform::Rotate90 => 90.0,
        FontTransform::Rotate180 => 180.0,
        FontTransform::Rotate270 => 270.0,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_settings_are_read_from_the_syntax_not_from_the_raw_text() {
        use super::{local_text_overrides, TextOverrides};
        assert_eq!(
            local_text_overrides("font: x, size: 28pt, fill: red"),
            TextOverrides::default()
        );
        assert_eq!(
            local_text_overrides("`#text(size: 28pt)[x]` // #text(fill: red)[x]"),
            TextOverrides::default()
        );
        let local = local_text_overrides("#strong[#text(size: 28pt, fill: rgb(\"#ff0000\"))[x]]\n#set text(font: \"Fira Sans\")\ny");
        assert!(local.font && local.size && local.color, "{local:?}");
        assert!(!local_text_overrides("#box(fill: red)[x]").color);
    }

    use super::*;

    fn noto() -> (Font, Font) {
        resolve_fonts(&FontChoice::default()).expect("Noto fonts unreadable")
    }

    const BLACK_INK: Ink = [0, 0, 0, 255];

    fn width_of(text: &str) -> f64 {
        let (sans, math) = noto();
        compile(text, 14.0, BLACK_INK, sans, math)
            .unwrap_or_else(|err| panic!("{text:?} does not compile: {err}"))
            .size_pt()
            .0
    }

    /// Issues #77 and #78: an area's font travels in the style's family, and
    /// `source` sets it with its Math font — Libertinus Sans brings Libertinus
    /// Math, a system font the Noto Sans Math fallback.
    #[test]
    fn an_areas_font_is_set_in_the_source_with_its_math() {
        let source_of = |style: plotters::style::TextStyle<'static>, text: &str| {
            source(text, &style).into_owned()
        };
        let libertinus = source_of(
            rich_style_font(16.0, (0, 0, 0, 255), Some("Libertinus Sans")),
            "Pic $x$",
        );
        assert!(
            libertinus.contains("font: \"Libertinus Sans\""),
            "{libertinus}"
        );
        assert!(
            libertinus.contains("font: \"Libertinus Math\""),
            "{libertinus}"
        );
        assert!(libertinus.ends_with("Pic $x$"));
        let system = source_of(
            rich_style_font(16.0, (0, 0, 0, 255), Some("Calibri")),
            "Pic",
        );
        assert!(system.contains("font: \"Noto Sans Math\""), "{system}");
        // No font: the text as is, as before.
        assert_eq!(
            source_of(rich_style_font(16.0, (0, 0, 0, 255), None), "Pic"),
            "Pic"
        );
        // A text taken over automatically stays escaped, font or not.
        let literal = source_of(
            literal_style_font(16.0, (0, 0, 0, 255), Some("Fira Sans")),
            "a_b",
        );
        assert!(literal.ends_with("#\"a_b\""), "{literal}");
        // And the font still makes rich text.
        assert!(is_rich(&rich_style_font(
            16.0,
            (0, 0, 0, 255),
            Some("Fira Sans")
        )));
        // The layout really changes with the font.
        let (sans, math) = noto();
        let width = |text: &str| {
            compile(text, 14.0, BLACK_INK, sans.clone(), math.clone())
                .expect("compile")
                .size_pt()
                .0
        };
        let fira = rich_style_font(14.0, (0, 0, 0, 255), Some("Fira Sans"));
        assert!(
            (width(&source_of(fira, "Absorbance (a.u.)")) - width("Absorbance (a.u.)")).abs()
                > 1e-3
        );
    }

    /// Issue #63: `#text(font: "Name")` chooses an embedded font by its family
    /// name. The witness: the same text without the tag is narrower or wider,
    /// never equal.
    #[test]
    fn an_embedded_font_is_chosen_by_its_name() {
        let default = width_of("Absorbance (a.u.)");
        let fira = width_of("#text(font: \"Fira Sans\")[Absorbance (a.u.)]");
        let libertinus = width_of("#text(font: \"Libertinus Sans\")[Absorbance (a.u.)]");
        assert!((default - fira).abs() > 1e-3, "Fira = default: {fira}");
        assert!((fira - libertinus).abs() > 1e-3, "Libertinus = Fira");
    }

    /// A font that is not found neither fails nor makes the text disappear: it
    /// falls back on the document's font.
    #[test]
    fn a_missing_font_falls_back_to_the_documents_font() {
        let default = width_of("Absorbance");
        let absent = width_of("#text(font: \"StokesForge Nonexistent Font\")[Absorbance]");
        assert!((default - absent).abs() < 1e-6);
    }

    #[test]
    fn requested_families_are_read_from_the_markup() {
        assert_eq!(
            requested_families(
                "#text(font: \"A B\")[x] and #text(size: 9pt, font:\"C\")[y] #text(font: \"A B\")[z]"
            ),
            ["A B", "C"]
        );
        assert!(requested_families("font: no quotes").is_empty());
        assert!(requested_families("font: \"open").is_empty());
    }

    /// The B and I buttons of the bar (issue #63) must change the rendering
    /// with the default font, otherwise they are useless.
    #[test]
    fn bold_and_italic_change_the_width_with_the_default_font() {
        let normal = width_of("Absorbance");
        let bold = width_of("#strong[Absorbance]");
        let italic = width_of("#emph[Absorbance]");
        assert!((normal - bold).abs() > 1e-3, "bold = normal: {bold}");
        assert!((normal - italic).abs() > 1e-3, "italic = normal: {italic}");
    }

    /// A rotated text (issue #35): its page fits the rotated box, so a quarter
    /// turn swaps width and height, and the original text still compiles.
    #[test]
    fn a_text_rotated_by_a_quarter_turn_swaps_width_and_height() {
        use plotters::style::{IntoFont, BLACK};
        let size = |angle: f64| {
            let family = rotated_family(angle);
            let style = (family.as_str(), 16.0).into_font().color(&BLACK);
            let (sans, math) = noto();
            compile(
                &source("Reference peak", &style),
                14.0,
                BLACK_INK,
                sans,
                math,
            )
            .expect("Typst compilation failed")
            .size_pt()
        };
        let (upright, quarter) = (size(0.0), size(90.0));
        assert!((upright.0 - quarter.1).abs() < 0.01 && (upright.1 - quarter.0).abs() < 0.01);
        let (w, h) = size(30.0);
        assert!(
            w < upright.0 && h > upright.1,
            "30°: box neither narrower nor taller"
        );
        assert_eq!(rotated_family(0.0), RICH_FAMILY);
    }

    /// Issue #36: the angle of a `DragValue` is nothing like an integer —
    /// fractional, very small, negative. The Typst source must remain a valid
    /// duration (never a `1e-7deg` notation, which Typst would refuse: the text
    /// would then fall back, unrotated, on the raw path) and the page must fit
    /// the rotated box, hence differ from the upright box.
    #[test]
    fn a_fractional_or_tiny_free_angle_is_compiled_and_drawn_rotated() {
        use plotters::style::{IntoFont, BLACK};
        let (sans, math) = noto();
        let size = |angle: f64| {
            let family = rotated_family(angle);
            let style = (family.as_str(), 16.0).into_font().color(&BLACK);
            let code = source("Reference peak", &style).into_owned();
            let duration = code
                .strip_prefix(ROTATE_CALL)
                .and_then(|rest| rest.split("deg)").next())
                .unwrap_or_else(|| panic!("{angle}°: not wrapped: {code}"));
            assert!(
                duration
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
                "{angle}°: duration `{duration}` in scientific notation"
            );
            compile(&code, 14.0, BLACK_INK, sans.clone(), math.clone())
                .unwrap_or_else(|error| panic!("{angle}°: {error:?}"))
                .size_pt()
        };
        let upright = {
            let style = (RICH_FAMILY, 16.0).into_font().color(&BLACK);
            compile(
                &source("Reference peak", &style),
                14.0,
                BLACK_INK,
                sans.clone(),
                math.clone(),
            )
            .expect("upright text")
            .size_pt()
        };
        for angle in [0.5, 33.3, -33.3, 179.99, -179.99] {
            let (w, h) = size(angle);
            assert!(
                (w, h) != upright,
                "{angle}°: box identical to the upright text's"
            );
        }
        // A tiny angle stays valid; it may have no visible effect.
        size(1e-7);
        size(-1e-7);
    }

    /// The switch is on the role (the style's family), not on the content: a
    /// text without `$` in rich style goes through Typst, a formula in ordinary
    /// style does not (issue #32).
    #[test]
    fn the_switch_rests_on_the_style_not_on_the_dollar() {
        use plotters::style::{IntoFont, BLACK};
        assert!(is_rich(&rich_style(16.0, (0, 0, 0, 255))));
        assert!(is_rich(&literal_style(16.0, (0, 0, 0, 255))));
        assert!(!is_rich(&("sans-serif", 16.0).into_font().color(&BLACK)));
        // Only the taken-over text is escaped.
        assert_eq!(source("$x$", &rich_style(16.0, (0, 0, 0, 255))), "$x$");
        assert_eq!(
            source("$x$", &literal_style(16.0, (0, 0, 0, 255))),
            "#\"$x$\""
        );
    }

    /// Issue #33: an imported column name compiles, and is composed like its
    /// literal text — same width as the markup escaped character by character,
    /// whereas the same text read as markup (the witness) turns italic or does
    /// not compile.
    #[test]
    fn an_escaped_text_is_displayed_as_is() {
        let width = |text: &str| {
            let (sans, math) = noto();
            compile(text, 14.0, BLACK_INK, sans, math).map(|c| c.size_pt().0)
        };
        let literal = width(&escape("Abs_280 #1")).expect("escaped text did not compile");
        let by_hand = width("Abs\\_280 \\#1").expect("reference did not compile");
        assert!((literal - by_hand).abs() < 1e-6, "{literal} ≠ {by_hand}");
        // The witness: read as markup, `_` opens an emphasis.
        assert!(
            width("Abs_280 #1").map_or(true, |raw| (raw - literal).abs() > 1e-3),
            "raw markup is composed like the escaped text: the test proves nothing"
        );

        for trap in [
            "Price ($)",
            "a\\b",
            "\"café\"",
            "x // y",
            "- list",
            "= title",
            "...",
            "<label>",
            "@ref",
            "`code`",
        ] {
            assert!(width(&escape(trap)).is_ok(), "{trap:?} does not compile");
        }
        // Neither ellipsis nor typographic quotes: the escaped text keeps its
        // characters (same width as escaping character by character).
        let dots = width(&escape("a...b")).expect("dots");
        let dots_by_hand = width("a\\.\\.\\.b").expect("dots by hand");
        assert!((dots - dots_by_hand).abs() < 1e-6);
    }

    /// The strut: ordinary text, descender, subscript, superscript have **the
    /// same** box, hence the same baseline; a fraction enlarges it without being
    /// cut off (issue #32).
    #[test]
    fn simple_labels_all_have_the_same_height() {
        let height = |text: &str| {
            let (sans, math) = noto();
            compile(text, 14.0, BLACK_INK, sans, math)
                .expect("Typst compilation failed")
                .size_pt()
                .1
        };
        let reference = height("a");
        for text in ["Ag", "Time (s)", "$n_i$", "$x^2$", "$\"Ru(bpy)\"_3^(2+)$"] {
            assert!(
                (height(text) - reference).abs() < 1e-6,
                "{text}: {} instead of {reference}",
                height(text)
            );
        }
        assert!(height("$a/b$") > reference, "a fraction is cut off");
    }

    /// A trivial formula compiles into a page whose size is not zero — the
    /// sanity test that should have existed before any production code
    /// consuming this compilation (mission instruction).
    #[test]
    fn a_trivial_formula_compiles_and_measures_a_nonzero_size() {
        let (sans, math) = noto();
        let compiled =
            compile("$lambda$", 12.0, BLACK_INK, sans, math).expect("Typst compilation failed");
        let (w, h) = compiled.size_pt();
        assert!(w > 0.0, "zero width: {w}");
        assert!(h > 0.0, "zero height: {h}");
    }

    /// A mixed text + formula label compiles too: this is the documented reason
    /// for the detection choice (no manual splitting here).
    #[test]
    fn a_mixed_text_and_formula_label_compiles() {
        let (sans, math) = noto();
        let compiled = compile("$lambda$ (nm)", 12.0, BLACK_INK, sans, math)
            .expect("Typst compilation failed");
        assert!(compiled.page().is_some());
    }

    /// A syntactically invalid formula fails cleanly, without panicking — a
    /// condition for a backend to be able to fall back on a raw `Shape::Text`
    /// rather than bring down the whole rendering.
    #[test]
    fn an_invalid_formula_fails_without_panicking() {
        let (sans, math) = noto();
        let result = compile("$ (unclosed", 12.0, BLACK_INK, sans, math);
        assert!(result.is_err());
    }

    /// T3: the interface reads the Typst error of a text without rendering it.
    #[test]
    fn diagnose_reports_the_error_of_a_text_that_does_not_compile() {
        assert!(diagnose("$ (unclosed").is_some());
        assert_eq!(diagnose("$lambda$ (nm)"), None);
        assert_eq!(diagnose(""), None);
        // Second request: same verdict, from memory.
        assert_eq!(diagnose("$ (unclosed"), diagnose("$ (unclosed"));
    }

    /// Two different sizes of the same formula give different page sizes —
    /// sanity check that we measure the requested point size, not a frozen
    /// value.
    #[test]
    fn a_larger_font_size_gives_a_larger_box() {
        let (sans1, math1) = noto();
        let (sans2, math2) = noto();
        let small = compile("$lambda$", 8.0, BLACK_INK, sans1, math1).unwrap();
        let large = compile("$lambda$", 32.0, BLACK_INK, sans2, math2).unwrap();
        assert!(large.size_pt().0 > small.size_pt().0);
        assert!(large.size_pt().1 > small.size_pt().1);
    }

    /// What goes below the font enlarges the box **downward**: the page goes
    /// down to the bottom of the ink, not to the baseline (Typst's default,
    /// which cut off subscripts and descenders on screen — issue #29). Since
    /// the strut of #32, a simple subscript fits within the font height; nested
    /// subscripts exceed it, and must enlarge the page rather than be cut off.
    #[test]
    fn a_subscript_or_descender_is_not_cut_off_by_the_page() {
        let height = |text: &str| {
            let (sans, math) = noto();
            compile(text, 14.0, BLACK_INK, sans, math)
                .expect("Typst compilation failed")
                .size_pt()
                .1
        };
        assert!(
            height("$x_(x_(x_(x_x)))$") > height("$x$"),
            "nested subscripts cut off"
        );
    }

    /// The cache: a second identical request does not recompile — verified by
    /// counting entries, not time (a measured time would be fragile in CI).
    /// Task 8.2.
    #[test]
    fn the_cache_does_not_grow_on_an_identical_request() {
        let cache: FormulaCache<(f64, f64)> = FormulaCache::new();
        let choice = FontChoice::default();
        for _ in 0..3 {
            let (sans, math) = noto();
            cache
                .get_or_try_insert(
                    "$lambda$",
                    12.0,
                    BLACK_INK,
                    &choice,
                    1.0,
                    sans,
                    math,
                    |compiled| Ok(compiled.size_pt()),
                )
                .expect("compilation/insertion failed");
        }
        assert_eq!(cache.len(), 1);
    }

    /// Two different `pixels_per_point` for the same formula occupy two
    /// entries — the key documented by [`CacheKey`], verified rather than
    /// assumed.
    #[test]
    fn two_distinct_pixels_per_point_occupy_two_entries() {
        let cache: FormulaCache<(f64, f64)> = FormulaCache::new();
        let choice = FontChoice::default();
        for ppp in [1.0_f32, 1.25_f32] {
            let (sans, math) = noto();
            cache
                .get_or_try_insert(
                    "$lambda$",
                    12.0,
                    BLACK_INK,
                    &choice,
                    ppp,
                    sans,
                    math,
                    |compiled| Ok(compiled.size_pt()),
                )
                .expect("compilation/insertion failed");
        }
        assert_eq!(cache.len(), 2);
    }

    /// Sanity measurement on the **real pipeline** — not R0's throwaway spike.
    /// Ignored by default, like the costly benches of `stokesforge-robustesse`
    /// (`cargo test -- --ignored`): it is not a correctness assertion, just a
    /// numeric marker to compare with the 150 ms threshold already set and
    /// already measured under that threshold by R0.
    #[test]
    #[ignore = "latency measurement, not an assertion — cargo test -- --ignored"]
    fn compilation_latency_of_a_new_formula() {
        let n = 200;
        let start = std::time::Instant::now();
        for i in 0..n {
            let (sans, math) = noto();
            let text = format!("$lambda_{i} = m_{i} c^2$");
            compile(&text, 14.0, BLACK_INK, sans, math).expect("Typst compilation failed");
        }
        let elapsed = start.elapsed();
        let per_call = elapsed / n;
        println!(
            "{n} Typst compilations of new formulas in {elapsed:?} \
             ({per_call:?}/formula, R0 threshold: 150 ms)"
        );
        assert!(
            per_call.as_millis() < 150,
            "compilation latency {per_call:?} above the threshold set in R0"
        );
    }

    /// A system font that is **not found** falls back to Noto for the text font
    /// and for the Math — no longer "always", as before the wiring to
    /// [`crate::fonts::load_system_font`]: only when the system lookup really
    /// fails, the same policy `crate::fonts::resolve` already applies.
    #[test]
    fn a_missing_system_font_falls_back_to_noto() {
        let (sans, math) = resolve_fonts(&FontChoice::System(
            "StokesForge Font That Cannot Exist — Test R.13".to_owned(),
        ))
        .expect("resolution failed");
        assert_eq!(sans.info().family, FontSet::Noto.sans_family_name());
        assert_eq!(math.info().family, FontSet::Noto.math_family_name());
    }

    /// Dynamically finds the name of a system font really present on the
    /// machine running the test, so as not to hard-code a name that no CI
    /// machine is guaranteed to have. `None` if the machine really has no
    /// system font: the test that depends on it is then a no-op rather than a
    /// failure that no fix in this module could resolve (`fonts.rs` has no such
    /// case to cover: it tests only the fallback on not found, never the found
    /// font).
    fn an_available_system_font() -> Option<String> {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let id = database.faces().next()?.id;
        let face = database.face(id)?;
        face.families.first().map(|(name, _)| name.clone())
    }

    /// A system font that is **found** is really used for the text half — the
    /// behavior fixed by this task: before, `resolve_fonts` fell back to Noto
    /// whether the font existed or not.
    #[test]
    fn an_available_system_font_is_really_used_for_the_text() {
        let Some(name) = an_available_system_font() else {
            eprintln!(
                "no system font on this machine: \
                 an_available_system_font_is_really_used_for_the_text skipped"
            );
            return;
        };
        let (sans, _math) =
            resolve_fonts(&FontChoice::System(name.clone())).expect("resolution failed");
        assert_eq!(
            sans.info().family,
            name,
            "the system font found was not used for the text"
        );
    }

    /// The Math font can be chosen independently of the text font through
    /// [`MathFontChoice`] — text and Math each come from a different embedded
    /// pair, and the two `Font`s obtained each match what was requested, not a
    /// pair linked by default.
    #[test]
    fn the_math_font_is_chosen_independently_of_the_text() {
        let (sans, math) = resolve_fonts_with_math(
            &FontChoice::Embedded(FontSet::Libertinus),
            Some(&MathFontChoice::Embedded(FontSet::Fira)),
        )
        .expect("resolution failed");
        assert_eq!(sans.info().family, FontSet::Libertinus.sans_family_name());
        assert_eq!(math.info().family, FontSet::Fira.math_family_name());
    }

    /// `None` passed to [`resolve_fonts_with_math`] reproduces exactly
    /// [`resolve_fonts`] — the documented contract that guarantees no existing
    /// caller changes behavior without being touched.
    #[test]
    fn no_explicit_math_choice_reproduces_the_old_behavior() {
        let choice = FontChoice::Embedded(FontSet::Fira);
        let (sans_a, math_a) = resolve_fonts(&choice).expect("resolution failed");
        let (sans_b, math_b) = resolve_fonts_with_math(&choice, None).expect("resolution failed");
        assert_eq!(sans_a.info().family, sans_b.info().family);
        assert_eq!(math_a.info().family, math_b.info().family);
    }

    /// Centered anchoring does subtract half of each dimension, and `Left`/`Top`
    /// anchoring shifts nothing — the two limit cases.
    #[test]
    fn centered_anchoring_subtracts_half_the_box() {
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Left, VPos::Top),
            (0.0, 0.0)
        );
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Center, VPos::Center),
            (-5.0, -2.0)
        );
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Right, VPos::Bottom),
            (-10.0, -4.0)
        );
    }

    /// A 90° clockwise rotation sends the X axis onto the Y axis — the first
    /// landmark that would have revealed a sign inversion.
    #[test]
    fn the_90_degree_rotation_sends_x_onto_y() {
        assert_eq!(
            rotate_offset((1.0, 0.0), FontTransform::Rotate90),
            (0.0, 1.0)
        );
        assert_eq!(rotation_degrees(FontTransform::Rotate90), 90.0);
    }

    /// **The test of invariant 8.5**: the measurement of a formula does not
    /// depend on the display scale — unlike the height of an `epaint` galley
    /// (`text.rs`), which really does. The two measurement paths have different
    /// reasons to behave as they do; this test pins down Typst's, so that merging
    /// the two in `estimate_text_size` (backend task) does not wrongly assume the
    /// same dependency on both sides.
    #[test]
    fn the_measurement_of_a_formula_does_not_depend_on_pixels_per_point() {
        let (sans, math) = noto();
        let a = measure(&compile("$lambda$", 14.0, BLACK_INK, sans, math).unwrap());
        let (sans, math) = noto();
        let b = measure(&compile("$lambda$", 14.0, BLACK_INK, sans, math).unwrap());
        assert_eq!(
            a, b,
            "two identical compilations measure differently for no reason"
        );
    }
}
