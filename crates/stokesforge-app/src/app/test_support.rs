//! Test helpers shared by the surfaces of `src/app/` (#128).
//!
//! Each surface used to write its own copies of these helpers (up to four
//! for `painted`, three for `context`); a fix had to be redone everywhere.
//! They live here once, and the `tests` modules import them by name
//! (`use super::super::test_support::{…}`, never `*`: see
//! `no_global_import_outside_tests`, which tolerates the tests but not the
//! vagueness).
//!
//! What stays local to a module, on purpose: the benches (`bench`), which each
//! mount a different state; `grid::tests::Bench::image` and
//! `workbook::tests::click`, which drive a bench or a paint closure rather
//! than a bare context; and `menus::tests::context`, which adds the image
//! loaders and AccessKit to the base from here.

use eframe::egui;
use stokesforge_core::{Column, NodeId, Project};

/// A context with the application's fonts and theme: the width of tabs,
/// labels and menus depends on them.
pub(super) fn context() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(stokesforge_render_egui::font_definitions());
    crate::theme::install(&context);
    context
}

/// The raw input of a frame of `size` with `events`.
fn raw_input(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    }
}

/// A frame of `size` with `events`, where `content` draws; returns the full
/// output, textures dropped. Time advances only 1/60 s per frame: see
/// [`image`] for the surfaces whose animations must run to the end.
pub(super) fn frame_of(
    context: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    content: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = context.run_ui(raw_input(size, events), content);
    output.textures_delta.clear();
    output
}

/// A 1400 × 900 frame with `events`, 0.1 s after the previous one; returns
/// what was painted. Time advances so that animated scrolling runs to the end
/// (the tab strip shows the tab chosen by `scroll_to_rect`, spread over 0.3 s
/// at most).
pub(super) fn image(
    context: &egui::Context,
    events: Vec<egui::Event>,
    content: impl FnMut(&mut egui::Ui),
) -> Vec<egui::epaint::ClippedShape> {
    let mut raw = raw_input(egui::vec2(1400.0, 900.0), events);
    raw.time = Some(context.input(|input| input.time) + 0.1);
    let mut output = context.run_ui(raw, content);
    output.textures_delta.clear();
    output.shapes
}

/// A key pressed, without modifier.
pub(super) fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A click of `button` at `pos`, spread over three frames (hover, press,
/// release) as egui's click detection requires.
pub(super) fn click(
    context: &egui::Context,
    pos: egui::Pos2,
    button: egui::PointerButton,
    mut content: impl FnMut(&mut egui::Ui),
) {
    image(context, vec![egui::Event::PointerMoved(pos)], &mut content);
    for pressed in [true, false] {
        image(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::default(),
            }],
            &mut content,
        );
    }
}

/// Was a text containing `needle` painted?
pub(super) fn painted(output: &egui::FullOutput, needle: &str) -> bool {
    output.shapes.iter().any(|painted| {
        matches!(&painted.shape,
            egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
    })
}

/// A painted text: its content, its effective color and its center.
#[derive(Debug)]
pub(super) struct Text {
    pub(super) text: String,
    pub(super) color: egui::Color32,
    pub(super) center: egui::Pos2,
    /// Written in bold (`theme::bold`): the weight is a font variation, not a
    /// color.
    pub(super) bold: bool,
}

/// All the texts painted in `shapes`, with their effective color.
pub(super) fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<Text> {
    shapes
        .iter()
        .filter_map(|painted| match &painted.shape {
            egui::Shape::Text(shape) => {
                let section = shape
                    .galley
                    .job
                    .sections
                    .first()
                    .map_or(egui::Color32::PLACEHOLDER, |s| s.format.color);
                let color =
                    shape
                        .override_text_color
                        .unwrap_or(if section == egui::Color32::PLACEHOLDER {
                            shape.fallback_color
                        } else {
                            section
                        });
                Some(Text {
                    text: shape.galley.job.text.clone(),
                    color,
                    center: shape.pos + shape.galley.rect.center().to_vec2(),
                    bold: shape
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|s| s.format.coords == crate::theme::bold_variations()),
                })
            }
            _ => None,
        })
        .collect()
}

/// The painted text exactly equal to `wanted`, if there is exactly one.
pub(super) fn text_exact<'a>(texts: &'a [Text], wanted: &str) -> &'a Text {
    let found: Vec<&Text> = texts.iter().filter(|t| t.text == wanted).collect();
    assert_eq!(
        found.len(),
        1,
        "{wanted:?} must be painted once: {:?}",
        texts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    found[0]
}

/// The labels of a list rendered by `label`, all distinct and non-empty: two
/// choices of one drop-down list that display the same can no longer be told
/// apart.
pub(super) fn distinct<T: std::fmt::Debug>(choices: &[T], label: impl Fn(&T) -> String) {
    let labels: Vec<String> = choices.iter().map(&label).collect();
    for (i, a) in labels.iter().enumerate() {
        assert!(!a.trim().is_empty(), "{:?} has no label", choices[i]);
        for (j, b) in labels.iter().enumerate().skip(i + 1) {
            assert_ne!(
                a, b,
                "{:?} and {:?} display the same",
                choices[i], choices[j]
            );
        }
    }
}

/// A workbook with one sheet of two columns `[0, 1]` under the root: returns
/// the nodes of these columns (x, y), enough to plot a curve.
pub(super) fn xy_columns(project: &mut Project) -> (NodeId, NodeId) {
    let root = project.root();
    let book = project.add_empty_workbook(root, None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let x = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    let y = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    (x, y)
}
