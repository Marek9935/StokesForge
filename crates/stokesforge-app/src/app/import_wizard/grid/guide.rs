//! the first-time guide of the Import Wizard.
//! A light card over the grid (not a modal over the whole window), three
//! short steps, each with a pointer to the real thing it talks about. Opens on
//! the first import screen of a run unless the user asked not to see it again;
//! the « ? » button of the grid tabs reopens it.

use eframe::egui;
use egui::{pos2, Pos2, Rect};
use stokesforge_app_state::state::KeyEntry;
use stokesforge_app_state::tr;

use super::{row_of_line, row_y, Geom, GridUi, View, ROW_H};
use crate::app::shortcuts::KEY_ESCAPE;
use crate::theme;

const STEPS: u8 = 3;
const CARD_W: f32 = 300.0;

/// Where the guide points, in screen coordinates.
pub(super) struct Targets {
    pub grid: Rect,
    pub panel: Rect,
    pub line_number: Option<Pos2>,
    pub value: Option<Pos2>,
}

/// The file that remembers « don't show again » (in the temporary folder, like
/// the mockup's other state; `mockup.sh --forget` removes it).
fn seen_flag() -> std::path::PathBuf {
    // The tests point it at their own folder.
    if let Some(path) = std::env::var_os("STOKESFORGE_MOCKUP_GUIDE_FLAG") {
        return std::path::PathBuf::from(path);
    }
    std::env::temp_dir().join("stokesforge-mockup-488j-guide-seen")
}

/// The « ? » button: starts again at the first step.
pub(super) fn reopen(gs: &mut GridUi) {
    gs.guide = Some(0);
    gs.guide_dont = true;
    gs.guide_init = true;
}

fn close(gs: &mut GridUi) {
    if gs.guide_dont {
        let _ = std::fs::write(seen_flag(), b"seen");
    }
    gs.guide = None;
}

/// The line number and a value cell to point at: those of the units line when
/// the file has one, else of the second line.
pub(super) fn line_targets(
    g: &Geom,
    v: &View<'_>,
    gs: &GridUi,
    keys: &[KeyEntry],
) -> (Option<Pos2>, Option<Pos2>) {
    let line = keys
        .iter()
        .find(|k| k.key.to_uppercase().contains("UNITS"))
        .map_or(1, |k| k.line.saturating_sub(1));
    let r = row_of_line(v.rows, line);
    let mut y = row_y(g, gs.pan, r) + ROW_H / 2.0;
    if y < g.rows_top || y > g.rows_top + g.view_h {
        y = g.rows_top + 3.0 * ROW_H;
    }
    let number = pos2(g.nums.right() - 6.0, y);
    let x = (g.grid.left() + v.metrics.colx.get(1).copied().unwrap_or(60.0) - gs.pan.x + 30.0)
        .clamp(g.grid.left() + 10.0, g.grid.right() - 10.0);
    (Some(number), Some(pos2(x, y)))
}

/// Paints the card of the current step and its pointer, and handles its keys.
pub(super) fn show(ctx: &egui::Context, gs: &mut GridUi, targets: &Targets) {
    if !gs.guide_init {
        gs.guide_init = true;
        if !seen_flag().exists() {
            gs.guide = Some(0);
            gs.guide_dont = true;
        }
    }
    let Some(step) = gs.guide else {
        return;
    };
    if ctx.input_mut(|i| i.consume_shortcut(&KEY_ESCAPE)) {
        close(gs);
        return;
    }
    let text = match step {
        0 => tr!("import-guide-1"),
        1 => tr!("import-guide-2"),
        _ => tr!("import-guide-3"),
    };
    let last = step + 1 >= STEPS;
    let (mut next, mut done) = (false, false);
    let mut dont = gs.guide_dont;
    let area = egui::Area::new(egui::Id::new("stokesforge_import_j_guide"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(
            (targets.grid.right() - CARD_W - 48.0).max(targets.grid.left() + 20.0),
            targets.grid.top() + 78.0,
        ))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .stroke(egui::Stroke::new(1.5, theme::accent()))
                .shadow(ui.visuals().popup_shadow)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(CARD_W);
                    ui.label(theme::secondary_text(tr!(
                        "import-guide-step",
                        n = (step + 1).to_string(),
                        total = STEPS.to_string()
                    )));
                    ui.add(egui::Label::new(theme::bold(text)).wrap());
                    ui.add_space(theme::SPACE_SECTION);
                    ui.checkbox(&mut dont, tr!("import-guide-dont"));
                    ui.add_space(theme::SPACE_ITEM);
                    ui.horizontal(|ui| {
                        let label = if last {
                            tr!("import-guide-done")
                        } else {
                            tr!("import-guide-next")
                        };
                        let button = egui::Button::new(label).fill(ui.visuals().selection.bg_fill);
                        if ui.add(button).clicked() {
                            if last {
                                done = true;
                            } else {
                                next = true;
                            }
                        }
                    });
                });
        });
    // Over the wizard window, and clickable (a tooltip layer is not).
    ctx.move_to_top(area.response.layer_id);
    gs.guide_dont = dont;
    let card = area.response.rect;
    let target = match step {
        0 => targets.line_number,
        1 => targets.value,
        _ => Some(pos2(targets.panel.left() + 110.0, targets.panel.center().y)),
    };
    if let Some(target) = target {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("stokesforge_import_j_guide_pointer"),
        ));
        ctx.move_to_top(painter.layer_id());
        let from = if target.x < card.left() {
            card.left_center()
        } else {
            card.center_bottom()
        };
        let stroke = egui::Stroke::new(2.0, theme::accent());
        painter.line_segment([from, target], stroke);
        painter.circle_filled(target, 6.0, theme::accent());
        painter.circle_stroke(target, 10.0, stroke);
    }
    if done {
        close(gs);
        ctx.request_repaint();
    } else if next {
        gs.guide = Some(step + 1);
        ctx.request_repaint();
    }
}
