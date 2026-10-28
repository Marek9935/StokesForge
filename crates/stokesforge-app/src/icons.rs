//! Interface icons: the only place that knows the SVG files.
//!
//! The icons are Phosphor duotone, baked by `scripts/icons-build.sh`
//! into **one variant per (icon, color role)** — `Image::tint` multiplies
//! the pixels and cannot recolor stroke and fill separately. Usage rules
//! and palette: `docs/icones.md`.
//!
//! Every icon display goes through [`icon_image`] or [`icon_button`]: uniform
//! size ([`ICON_SIZE`]) and alignment, mandatory tooltip (text already
//! translated by the caller, via `tr!`). No other module cites
//! `assets/icons` or an `.svg` — locked by the test
//! `no_icon_outside_the_component`.
//!
//! The SVGs are embedded (`include_bytes!`) and rasterized **on demand** by
//! `egui_extras` (once per size × `pixels_per_point`, then kept as a
//! texture): only the icons actually displayed cost anything. The theme of the
//! application is light and fixed (`theme.rs`): the role colors are
//! therefore constant, with no dark variant.

use eframe::egui::{self, Response, Ui, WidgetText};

/// Side of an icon, in points. Unique: an icon bigger or smaller
/// than its neighbors is a defect, not a local choice.
pub const ICON_SIZE: f32 = 16.0;

/// What the color of an icon says. `Neutral` by default; the other
/// roles are used only when the color carries information that the shape
/// does not (object types, destructive action).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconRole {
    Neutral,
    Workbook,
    Graph,
    Matrix,
    Note,
    Folder,
    Danger,
}

/// Available icons — a closed enum: adding an icon means adding a
/// variant, its lines in [`VARIANTS`] and its entry in `icons.tsv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Close,
    Detach,
    Reattach,
    Maximize,
    Restore,
    CaretDown,
    CaretRight,
    CaretLeft,
    CaretUp,
    Add,
    Delete,
    Folder,
    Workbook,
    Graph,
    Matrix,
    Note,
    Open,
    Save,
    SaveAs,
    Import,
    Export,
    Pointer,
    ZoomTool,
    TextTool,
    ResetZoom,
    NewProject,
    Properties,
    AddColumn,
    AddSheet,
    PlotLine,
    PlotScatter,
    PlotBar,
    Bold,
    Italic,
    Subscript,
    Superscript,
    Formula,
    AlignLeft,
    AlignCenter,
    AlignRight,
    ShapeRectangle,
    ShapeEllipse,
    ShapeLine,
    ShapeArrow,
    LineColor,
    FillColor,
    TextColor,
    Axis,
    /// The hover help of a field (#197): a question mark.
    Help,
    /// An analysis computation that is out of date (#200): warning triangle, at the top
    /// of the produced column and on the series that plots it.
    Stale,
    /// A column corrected in place (#212), distinct from a stale computation.
    ModifiedData,
    /// Barre Edit (#238).
    Undo,
    Redo,
    /// The handle of a row that is reordered by dragging.
    Grip,
}

macro_rules! svg {
    ($file:literal) => {
        (
            concat!("bytes://stokesforge-icons/", $file),
            include_bytes!(concat!("../assets/icons/", $file)) as &[u8],
        )
    };
}

/// Baked variants. The first line of an icon is its fallback when the
/// requested role does not exist for it.
/// egui URI and bytes of an embedded SVG.
type Svg = (&'static str, &'static [u8]);

const VARIANTS: &[(Icon, IconRole, Svg)] = &[
    (Icon::Close, IconRole::Neutral, svg!("x.neutral.svg")),
    (Icon::Close, IconRole::Danger, svg!("x.danger.svg")),
    (
        Icon::Detach,
        IconRole::Neutral,
        svg!("arrow-square-out.neutral.svg"),
    ),
    (
        Icon::Reattach,
        IconRole::Neutral,
        svg!("arrow-square-in.neutral.svg"),
    ),
    (
        Icon::Maximize,
        IconRole::Neutral,
        svg!("corners-out.neutral.svg"),
    ),
    (
        Icon::Restore,
        IconRole::Neutral,
        svg!("corners-in.neutral.svg"),
    ),
    (
        Icon::CaretDown,
        IconRole::Neutral,
        svg!("caret-down.neutral.svg"),
    ),
    (
        Icon::CaretRight,
        IconRole::Neutral,
        svg!("caret-right.neutral.svg"),
    ),
    (
        Icon::CaretLeft,
        IconRole::Neutral,
        svg!("caret-left.neutral.svg"),
    ),
    (
        Icon::CaretUp,
        IconRole::Neutral,
        svg!("caret-up.neutral.svg"),
    ),
    (Icon::Add, IconRole::Neutral, svg!("plus.neutral.svg")),
    (Icon::Delete, IconRole::Danger, svg!("trash.danger.svg")),
    (
        Icon::Folder,
        IconRole::Folder,
        svg!("folder-simple.folder.svg"),
    ),
    (
        Icon::Workbook,
        IconRole::Workbook,
        svg!("table.workbook.svg"),
    ),
    (Icon::Graph, IconRole::Graph, svg!("chart-line.graph.svg")),
    (Icon::Matrix, IconRole::Matrix, svg!("grid-nine.matrix.svg")),
    (Icon::Note, IconRole::Note, svg!("note.note.svg")),
    (
        Icon::Open,
        IconRole::Neutral,
        svg!("folder-open.neutral.svg"),
    ),
    (
        Icon::Save,
        IconRole::Neutral,
        svg!("floppy-disk.neutral.svg"),
    ),
    (
        Icon::SaveAs,
        IconRole::Neutral,
        svg!("floppy-disk-back.neutral.svg"),
    ),
    (
        Icon::Import,
        IconRole::Neutral,
        svg!("download-simple.neutral.svg"),
    ),
    (Icon::Export, IconRole::Neutral, svg!("export.neutral.svg")),
    (Icon::Pointer, IconRole::Neutral, svg!("cursor.neutral.svg")),
    (
        Icon::ZoomTool,
        IconRole::Neutral,
        svg!("magnifying-glass-plus.neutral.svg"),
    ),
    (
        Icon::TextTool,
        IconRole::Neutral,
        svg!("text-t.neutral.svg"),
    ),
    (
        Icon::ShapeRectangle,
        IconRole::Neutral,
        svg!("rectangle.neutral.svg"),
    ),
    (
        Icon::ShapeEllipse,
        IconRole::Neutral,
        svg!("circle.neutral.svg"),
    ),
    (
        Icon::ShapeLine,
        IconRole::Neutral,
        svg!("line-segment.neutral.svg"),
    ),
    (
        Icon::ShapeArrow,
        IconRole::Neutral,
        svg!("arrow-up-right.neutral.svg"),
    ),
    (
        Icon::ResetZoom,
        IconRole::Neutral,
        svg!("arrows-out.neutral.svg"),
    ),
    (
        Icon::NewProject,
        IconRole::Neutral,
        svg!("file-plus.neutral.svg"),
    ),
    (
        Icon::Properties,
        IconRole::Neutral,
        svg!("sliders-horizontal.neutral.svg"),
    ),
    (
        Icon::AddColumn,
        IconRole::Neutral,
        svg!("columns-plus-right.neutral.svg"),
    ),
    (
        Icon::AddSheet,
        IconRole::Neutral,
        svg!("rows-plus-bottom.neutral.svg"),
    ),
    (
        Icon::PlotLine,
        IconRole::Neutral,
        svg!("chart-line.neutral.svg"),
    ),
    (
        Icon::PlotScatter,
        IconRole::Neutral,
        svg!("chart-scatter.neutral.svg"),
    ),
    (
        Icon::PlotBar,
        IconRole::Neutral,
        svg!("chart-bar.neutral.svg"),
    ),
    (
        Icon::AlignLeft,
        IconRole::Neutral,
        svg!("text-align-left.neutral.svg"),
    ),
    (
        Icon::AlignCenter,
        IconRole::Neutral,
        svg!("text-align-center.neutral.svg"),
    ),
    (
        Icon::AlignRight,
        IconRole::Neutral,
        svg!("text-align-right.neutral.svg"),
    ),
    (Icon::Bold, IconRole::Neutral, svg!("text-b.neutral.svg")),
    (
        Icon::Italic,
        IconRole::Neutral,
        svg!("text-italic.neutral.svg"),
    ),
    (
        Icon::Subscript,
        IconRole::Neutral,
        svg!("text-subscript.neutral.svg"),
    ),
    (
        Icon::Superscript,
        IconRole::Neutral,
        svg!("text-superscript.neutral.svg"),
    ),
    (
        Icon::Formula,
        IconRole::Neutral,
        svg!("function.neutral.svg"),
    ),
    (
        Icon::LineColor,
        IconRole::Neutral,
        svg!("pen-nib-modify.neutral.svg"),
    ),
    (
        Icon::FillColor,
        IconRole::Neutral,
        svg!("paint-bucket-modify.neutral.svg"),
    ),
    (
        Icon::TextColor,
        IconRole::Neutral,
        svg!("text-a-underline-modify.neutral.svg"),
    ),
    (Icon::Axis, IconRole::Neutral, svg!("ruler.neutral.svg")),
    (Icon::Help, IconRole::Neutral, svg!("question.neutral.svg")),
    (Icon::Stale, IconRole::Neutral, svg!("warning.neutral.svg")),
    (
        Icon::ModifiedData,
        IconRole::Neutral,
        svg!("pencil-simple.neutral.svg"),
    ),
    (
        Icon::Undo,
        IconRole::Neutral,
        svg!("arrow-counter-clockwise.neutral.svg"),
    ),
    (
        Icon::Redo,
        IconRole::Neutral,
        svg!("arrow-clockwise.neutral.svg"),
    ),
    (
        Icon::Grip,
        IconRole::Neutral,
        svg!("dots-six-vertical.neutral.svg"),
    ),
];

/// Unreachable as long as every icon has a variant (test `the_variants_table_…`).
const FALLBACK: Svg = svg!("x.neutral.svg");

fn variant(icon: Icon, role: IconRole) -> (&'static str, &'static [u8]) {
    let mut of_icon = VARIANTS.iter().filter(|(i, _, _)| *i == icon);
    of_icon
        .clone()
        .find(|(_, r, _)| *r == role)
        .or_else(|| of_icon.next())
        .map_or(FALLBACK, |(_, _, file)| *file)
}

/// The icon as an egui image, at the standard size.
pub fn icon_image(icon: Icon, role: IconRole) -> egui::Image<'static> {
    let (uri, bytes) = variant(icon, role);
    egui::Image::from_bytes(uri, bytes).fit_to_exact_size(egui::Vec2::splat(ICON_SIZE))
}

/// Frameless icon button. `tooltip` is mandatory: an icon alone does not
/// say what it does (text from `tr!`, supplied by the caller).
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    role: IconRole,
    tooltip: impl Into<WidgetText>,
) -> Response {
    ui.add(egui::Button::image(icon_image(icon, role)).frame(false))
        .on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The lines that load an icon outside `icons.rs`, `src/path:line`
    /// (comments excluded, tests included).
    fn icons_outside_component(sources: &[crate::sources::SourceFile]) -> Vec<String> {
        let mut offenders = Vec::new();
        for file in sources {
            if file.path == "icons.rs" {
                continue;
            }
            for (index, line) in file.code().lines().enumerate() {
                if line.contains("assets/icons") || line.contains("stokesforge-icons") {
                    offenders.push(format!("src/{}:{}", file.path, index + 1));
                }
            }
        }
        offenders
    }

    /// Guard of the standard: no icon is loaded outside this module.
    /// All the sources of `src/` are read, subdirectories included (#116).
    #[test]
    fn no_icon_outside_the_component() {
        let offenders = icons_outside_component(&crate::sources::sources());
        assert!(
            offenders.is_empty(),
            "icon loaded outside icons.rs — use icons::icon_image / \
             icons::icon_button: {offenders:?}"
        );
    }

    /// The witness: an icon loaded in a module of `src/app/` is seen;
    /// a comment and `icons.rs` itself pass.
    #[test]
    fn the_icons_guard_sees_a_load_outside_app_rs() {
        let file = |path: &str, text: &str| crate::sources::SourceFile {
            path: path.to_string(),
            text: text.to_string(),
            is_test: false,
        };
        let sources = [
            file(
                "app/bar.rs",
                "// assets/icons\nconst X: &[u8] = include_bytes!(\"../../assets/icons/x.svg\");",
            ),
            file(
                "app/other.rs",
                "fn f() { image(\"stokesforge-icons://x\"); }",
            ),
            file("icons.rs", "const Y: &str = \"assets/icons\";"),
        ];
        assert_eq!(
            icons_outside_component(&sources),
            ["src/app/bar.rs:2", "src/app/other.rs:1"]
        );
    }

    /// `icons.tsv` (what the script produces) and `VARIANTS` (what the code
    /// uses) say the same thing, and every baked file is an SVG.
    #[test]
    fn the_variants_table_follows_icons_tsv() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
        let mut expected = BTreeSet::new();
        for line in std::fs::read_to_string(dir.join("icons.tsv"))
            .unwrap()
            .lines()
        {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, roles) = line.split_once('\t').unwrap();
            for role in roles.split(',') {
                expected.insert(format!("{name}.{role}.svg"));
            }
        }
        let used: BTreeSet<String> = VARIANTS
            .iter()
            .map(|(_, _, (uri, _))| uri.rsplit('/').next().unwrap().to_owned())
            .collect();
        assert_eq!(used, expected);
        for (_, _, (uri, bytes)) in VARIANTS {
            assert!(
                std::str::from_utf8(bytes).unwrap().starts_with("<svg"),
                "{uri}"
            );
        }
    }

    /// Every variant really rasterizes (SVG loader installed).
    #[test]
    fn every_icon_loads() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        for (icon, role, _) in VARIANTS {
            let image = icon_image(*icon, *role);
            let mut loaded = None;
            for _ in 0..50 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    loaded = Some(
                        image
                            .clone()
                            .load_for_size(ui.ctx(), egui::Vec2::splat(ICON_SIZE)),
                    );
                });
                output.textures_delta.clear();
                if matches!(loaded, Some(Ok(egui::load::TexturePoll::Ready { .. }))) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(
                matches!(loaded, Some(Ok(egui::load::TexturePoll::Ready { .. }))),
                "{icon:?}/{role:?} does not load: {:?}",
                loaded.as_ref().map(|r| r.as_ref().err())
            );
        }
    }
}
