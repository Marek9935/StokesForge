//! Resize handles of the colour bar inside the frame (#499).
//!
//! A selected bar that sits inside the frame (a corner, or dragged) shows
//! the handles of a drawn rectangle (same look and grab radius as
//! [`crate::shape_tools`]) around its box. Pulling one changes the **length
//! and thickness of the strip** (`ColorBarSpec::length` / `thickness`); the
//! texts keep their font size and are laid out again around the new strip by
//! the one drawing code of the bar. Outside the frame the length follows the
//! frame, so only the thickness handle is offered.
//!
//! Same principle as every drag: the preview lives in the view (a change on
//! the clone of the spec when the figure is built) and **one**
//! [`GraphEvent::ColorBarResized`] is sent on release.

use egui::{Painter, PointerButton, Pos2, Response, Stroke, StrokeKind};
use stokesforge_plot::{PlotSpec, SeriesAxis, TextAnchor, TextTarget};
use stokesforge_render::color_bar::{resize_handles, resized};
use stokesforge_render::PanelTransform;

use crate::shape_tools::{HANDLE_GRAB, HANDLE_HALF, SELECTION_BLUE};
use crate::{GraphEvent, GraphView};

/// Gap, in screen points, between the box of the bar and its outline (the
/// selection outline of a text), where the handles sit.
const OUTLINE_PADDING: f32 = 3.0;

/// The handles of a bar: its layer, and each side with its position.
type BarHandles = (usize, Vec<((i8, i8), Pos2)>);

/// A resized bar, as the figure shows it while the handle is pulled and as
/// the model receives it: `None` leaves the field as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarSize {
    pub(crate) panel: usize,
    pub(crate) length: Option<f64>,
    pub(crate) thickness: Option<f64>,
    /// New centre of a dragged bar.
    pub(crate) anchor: Option<TextAnchor>,
}

impl BarSize {
    /// Puts the size on the bar of `spec` (no effect without a bar).
    pub(crate) fn apply(&self, spec: &mut PlotSpec) {
        let Some(bar) = spec
            .panels
            .get_mut(self.panel)
            .and_then(|panel| panel.color_bar.as_mut())
        else {
            return;
        };
        if self.length.is_some() {
            bar.length = self.length;
        }
        if self.thickness.is_some() {
            bar.thickness = self.thickness;
        }
        if self.anchor.is_some() {
            bar.anchor = self.anchor;
        }
    }
}

/// A handle pulled, in progress.
#[derive(Debug, Clone)]
pub(crate) struct BarDrag {
    panel: usize,
    handle: (i8, i8),
    original: stokesforge_plot::ColorBarSpec,
    /// Strip length at the start, and centre of the box (figure points).
    length: f64,
    centre: (f64, f64),
    /// Press point in egui points, and the scale (screen per figure point)
    /// at the press: a figure that grows with the band changes its scale
    /// during the gesture, the gesture is measured on the screen.
    press: Pos2,
    scale: f32,
}

impl GraphView {
    /// The resize handles of the selected bar, in egui points: the sides of
    /// its box and their position, `None` without a selected inside bar.
    fn bar_handles(&self, spec: &PlotSpec) -> Option<BarHandles> {
        let TextTarget::ColorBar { panel } = self.selected_text? else {
            return None;
        };
        let bar = spec.panels.get(panel)?.color_bar.as_ref()?;
        let (centre, size, _) = self.text_screen_box(TextTarget::ColorBar { panel })?;
        let half = size / 2.0 + egui::vec2(OUTLINE_PADDING, OUTLINE_PADDING);
        let handles = resize_handles(bar)
            .into_iter()
            .map(|(hx, hy)| {
                (
                    (hx, hy),
                    centre + egui::vec2(f32::from(hx) * half.x, f32::from(hy) * half.y),
                )
            })
            .collect();
        Some((panel, handles))
    }

    /// Pointer tool: a drag starting from a handle of the selected bar
    /// resizes it. Returns `true` while the gesture is a bar resize.
    pub(crate) fn handle_bar_resize(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(_) = self.bar_drag.take() else {
                return false;
            };
            match self.bar_size_preview {
                Some(size) => {
                    self.events.push(GraphEvent::ColorBarResized {
                        panel: size.panel,
                        length: size.length,
                        thickness: size.thickness,
                        anchor: size.anchor,
                    });
                    self.hold_preview();
                }
                None => self.set_bar_size_preview(None),
            }
            return true;
        }
        if !response.dragged_by(PointerButton::Primary) {
            self.bar_grab_tried = false;
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.bar_drag.is_some();
        };
        // A handle is grabbed by the press only: once the first frame of the
        // gesture has found none, a moving bar (the title drag below) must not
        // slide a handle under the press point.
        if self.bar_drag.is_none() && !self.bar_grab_tried {
            self.bar_grab_tried = true;
            // Where the button went down, not where egui noticed the drag:
            // a slow start would otherwise lose the first millimetres.
            let origin = response
                .ctx
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pos - response.drag_delta());
            let Some((panel, handles)) = self.bar_handles(spec) else {
                return false;
            };
            let Some((handle, _)) = handles
                .into_iter()
                .find(|(_, at)| at.distance(origin) <= HANDLE_GRAB)
            else {
                return false;
            };
            let (Some(bar), Some(drawn), Some(scale)) = (
                spec.panels.get(panel).and_then(|p| p.color_bar.clone()),
                self.drawn_text(TextTarget::ColorBar { panel }).copied(),
                self.display_scale(),
            ) else {
                return false;
            };
            // The strip is as long as the box along the bar.
            let horizontal = stokesforge_render::color_bar::is_horizontal(&bar);
            let length = if horizontal {
                f64::from(drawn.size.0)
            } else {
                f64::from(drawn.size.1)
            };
            self.bar_drag = Some(BarDrag {
                panel,
                handle,
                original: bar,
                length,
                centre: drawn.center,
                press: origin,
                scale,
            });
        }
        let Some(drag) = self.bar_drag.clone() else {
            return false;
        };
        let delta = (pos - drag.press) / drag.scale;
        let result = resized(
            &drag.original,
            drag.length,
            drag.handle,
            (f64::from(delta.x), f64::from(delta.y)),
            drag.centre,
        );
        let anchor = stokesforge_render::color_bar::resize_moves_centre(&drag.original)
            .then(|| self.bar_anchor(drag.panel, &drag.original, result.centre))
            .flatten();
        self.set_bar_size_preview(Some(BarSize {
            panel: drag.panel,
            length: result.length,
            thickness: result.thickness,
            anchor,
        }));
        true
    }

    /// The data anchor of a dragged bar centred on `centre` (figure points).
    fn bar_anchor(
        &self,
        panel: usize,
        bar: &stokesforge_plot::ColorBarSpec,
        centre: (f64, f64),
    ) -> Option<TextAnchor> {
        let right = bar
            .anchor
            .is_some_and(|anchor| anchor.axis == SeriesAxis::Right);
        let (x, y) = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|drawn| drawn.index == panel)
            .and_then(|drawn| PanelTransform::of_axis(drawn, right))?
            .pixel_to_data(centre.0, centre.1)?;
        Some(TextAnchor {
            x,
            y,
            axis: if right {
                SeriesAxis::Right
            } else {
                SeriesAxis::Left
            },
        })
    }

    /// The handles of the selected bar, over the figure.
    pub(crate) fn paint_bar_handles(&self, painter: &Painter, spec: &PlotSpec) {
        let Some((_, handles)) = self.bar_handles(spec) else {
            return;
        };
        let stroke = Stroke::new(1.0, SELECTION_BLUE);
        for (_, at) in handles {
            let rect =
                egui::Rect::from_center_size(at, egui::vec2(HANDLE_HALF * 2.0, HANDLE_HALF * 2.0));
            painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Middle);
        }
    }
}
