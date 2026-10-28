//! Graphic objects on screen (issue #76): draw by dragging with the
//! Rectangle / Ellipse / Line / Arrow tools, select, move, resize with the
//! handles.
//!
//! Same principle as free texts (#35): **one gesture = a single change of
//! the model**. The preview lives in the view ([`GraphView`], field
//! `shape_preview`), set on the clone of the spec when the figure is built,
//! and a single [`GraphEvent::ShapeChanged`] is sent on release. The gesture
//! coordinates are figure pixels; they are converted to data by
//! [`PanelTransform`] at each frame, so the object stays anchored to the data
//! (it will follow the zoom afterwards).
//!
//! The handles of a rectangle or an ellipse are the eight points of its box
//! (four corners, four side midpoints); those of a line or an arrow, its two
//! endpoints. They are tested **before** the object: a corner of an ellipse's
//! box is not on the drawing.
//! A click inserts a default size in figure pixels (#153); Shift constrains
//! the handles to the initial ratio, without changing the free drawing.
//! These gestures go through the same view in both window types.

use egui::{Color32, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke, StrokeKind};
use stokesforge_plot::{AxisScale, PlotSpec, SeriesAxis, ShapeKind, ShapeRef, ShapeSpec};
use stokesforge_render::shapes::DrawnShape;
use stokesforge_render::{PanelTransform, PlotComponent};

use crate::{GraphEvent, GraphTool, GraphView};

/// Distance, in screen pixels, below which a drawing gesture is only a hand
/// tremor and creates nothing.
const MIN_DRAW_DISTANCE: f32 = 4.0;
/// Half-side of a handle, in screen pixels, and the radius where it is grabbed.
const HANDLE_HALF: f32 = 4.0;
const HANDLE_GRAB: f32 = 8.0;
const SELECTION_BLUE: Color32 = Color32::from_rgb(90, 150, 255);

/// A resize handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handle {
    /// A point of the box of a rectangle or an ellipse: `(-1|0|1,
    /// -1|0|1)` depending on the side (left/top = -1), excluding `(0, 0)`.
    Box(i8, i8),
    /// An endpoint of a line or an arrow: 1 (tail) or 2 (tip).
    Endpoint(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// Drawing a new object.
    Draw,
    Move,
    Resize(Handle),
}

/// A gesture on a graphic object, in progress.
#[derive(Debug, Clone)]
pub(crate) struct ShapeDrag {
    target: ShapeRef,
    original: ShapeSpec,
    mode: DragMode,
    /// Press point, in figure coordinates.
    press: Pos2,
    /// The two points of the object at the start of the gesture, in figure
    /// coordinates.
    start: (Pos2, Pos2),
    /// True as soon as the pointer has moved far enough for the gesture to count.
    armed: bool,
}

fn point(p: (f64, f64)) -> Pos2 {
    Pos2::new(p.0 as f32, p.1 as f32)
}

/// The handles of a drawn object, with their position in figure coordinates.
pub(crate) fn handles(drawn: &DrawnShape) -> Vec<(Handle, Pos2)> {
    if !drawn.kind.is_closed() {
        return vec![
            (Handle::Endpoint(1), point(drawn.p1)),
            (Handle::Endpoint(2), point(drawn.p2)),
        ];
    }
    let (p1, p2) = (point(drawn.p1), point(drawn.p2));
    let (left, right) = (p1.x.min(p2.x), p1.x.max(p2.x));
    let (top, bottom) = (p1.y.min(p2.y), p1.y.max(p2.y));
    let at = |side: i8, low: f32, high: f32| match side {
        -1 => low,
        1 => high,
        _ => (low + high) / 2.0,
    };
    let mut out = Vec::with_capacity(8);
    for hy in [-1, 0, 1] {
        for hx in [-1, 0, 1] {
            if hx != 0 || hy != 0 {
                out.push((
                    Handle::Box(hx, hy),
                    Pos2::new(at(hx, left, right), at(hy, top, bottom)),
                ));
            }
        }
    }
    out
}

/// Size in figure coordinates, independent of the axis units.
fn default_extent(kind: ShapeKind) -> egui::Vec2 {
    match kind {
        ShapeKind::Rectangle | ShapeKind::Ellipse => egui::vec2(48.0, 48.0),
        ShapeKind::Line | ShapeKind::Arrow => egui::vec2(80.0, 0.0),
    }
}

/// Shift keeps the **visual proportions of the start of the gesture**, even
/// on logarithmic axes. A side keeps its opposite side and the center of the
/// other dimension; a corner keeps the opposite corner. A line endpoint
/// stays on its initial direction (including horizontal or vertical).
fn resized_points(start: (Pos2, Pos2), handle: Handle, current: Pos2, shift: bool) -> (Pos2, Pos2) {
    if let Handle::Endpoint(endpoint) = handle {
        let (fixed, moving) = if endpoint == 1 {
            (start.1, start.0)
        } else {
            start
        };
        let direction = moving - fixed;
        let at = if shift && direction.length_sq() > 0.0 {
            fixed + direction * ((current - fixed).dot(direction) / direction.length_sq())
        } else {
            current
        };
        return if endpoint == 1 {
            (at, fixed)
        } else {
            (fixed, at)
        };
    }
    let Handle::Box(hx, hy) = handle else {
        unreachable!()
    };
    let rect = Rect::from_two_pos(start.0, start.1);
    let (mut left, mut right, mut top, mut bottom) =
        (rect.left(), rect.right(), rect.top(), rect.bottom());
    match hx {
        -1 => left = current.x,
        1 => right = current.x,
        _ => {}
    }
    match hy {
        -1 => top = current.y,
        1 => bottom = current.y,
        _ => {}
    }
    if shift && rect.width() > 0.0 && rect.height() > 0.0 {
        let ratio = rect.width() / rect.height();
        if hx == 0 {
            let half = (bottom - top).abs() * ratio / 2.0;
            left = rect.center().x - half;
            right = rect.center().x + half;
        } else if hy == 0 {
            let half = (right - left).abs() / ratio / 2.0;
            top = rect.center().y - half;
            bottom = rect.center().y + half;
        } else {
            let width = right - left;
            let height = bottom - top;
            let scale = (width.abs() / rect.width()).max(height.abs() / rect.height());
            let width = (if width < 0.0 { -1.0 } else { 1.0 }) * rect.width() * scale;
            let height = (if height < 0.0 { -1.0 } else { 1.0 }) * rect.height() * scale;
            if hx == -1 {
                left = right - width;
            } else {
                right = left + width;
            }
            if hy == -1 {
                top = bottom - height;
            } else {
                bottom = top + height;
            }
        }
    }
    (Pos2::new(left, top), Pos2::new(right, bottom))
}

impl GraphView {
    /// The preview of a graphic object, if there is one.
    pub fn shape_preview(&self) -> Option<&(ShapeRef, ShapeSpec)> {
        self.shape_preview.as_ref()
    }

    /// Sets (or removes) the preview of a graphic object: the figure shows it
    /// at the next frame, without touching the model.
    pub fn set_shape_preview(&mut self, preview: Option<(ShapeRef, ShapeSpec)>) {
        self.preview_until = None;
        if self.shape_preview != preview {
            self.shape_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// The selected graphic object, if there is one.
    pub fn selected_shape(&self) -> Option<ShapeRef> {
        self.selected_shape
    }

    /// Selects a graphic object (or nothing) — for example after bringing it
    /// to the front, which changes its rank.
    pub fn select_shape(&mut self, target: Option<ShapeRef>) {
        self.selected_shape = target;
        if target.is_some() {
            self.selected_series = None;
        }
        if target.is_some() {
            self.selected_annotation = None;
            self.selected_text = None;
        }
    }

    /// The graphic object under the egui pixel `pos`, looked up in the last
    /// drawn figure.
    pub fn shape_at(&self, pos: Pos2) -> Option<ShapeRef> {
        match self.component_at(pos)? {
            PlotComponent::Shape { panel, index } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(ShapeRef {
                    panel: drawn.index,
                    index,
                })
            }
            _ => None,
        }
    }

    /// The object as it was drawn in the last figure.
    pub fn drawn_shape(&self, target: ShapeRef) -> Option<&DrawnShape> {
        self.last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.index == target.panel)?
            .shapes
            .iter()
            .find(|shape| shape.index == target.index)
    }

    pub(crate) fn abort_shape_drag(&mut self) {
        if self.shape_drag.take().is_some() {
            self.set_shape_preview(None);
        }
    }

    fn shape_transform(&self, target: ShapeRef, axis: SeriesAxis) -> Option<PanelTransform> {
        let panel = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.index == target.panel)?;
        PanelTransform::of_axis(panel, axis == SeriesAxis::Right)
    }

    /// Carries two figure points into data in a copy of `shape`.
    fn moved_shape(
        &self,
        target: ShapeRef,
        shape: &ShapeSpec,
        a: Pos2,
        b: Pos2,
    ) -> Option<ShapeSpec> {
        let transform = self.shape_transform(target, shape.axis)?;
        let (x1, y1) = transform.pixel_to_data(a.x as f64, a.y as f64)?;
        let (x2, y2) = transform.pixel_to_data(b.x as f64, b.y as f64)?;
        Some(ShapeSpec {
            x1,
            y1,
            x2,
            y2,
            ..shape.clone()
        })
    }

    /// `drag.original` shifted by the pointer gap, computed in data; `None`
    /// if an axis is not linear (the gap is not constant there).
    fn shifted_shape(&self, drag: &ShapeDrag, current: Pos2) -> Option<ShapeSpec> {
        let panel = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.index == drag.target.panel)?;
        let right = drag.original.axis == SeriesAxis::Right;
        let y_scale = if right { panel.y2_scale } else { panel.y_scale };
        if panel.x_scale != AxisScale::Linear || y_scale != AxisScale::Linear {
            return None;
        }
        let transform = PanelTransform::of_axis(panel, right)?;
        let (x0, y0) = transform.pixel_to_data(drag.press.x as f64, drag.press.y as f64)?;
        let (x1, y1) = transform.pixel_to_data(current.x as f64, current.y as f64)?;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let original = &drag.original;
        Some(ShapeSpec {
            x1: original.x1 + dx,
            y1: original.y1 + dy,
            x2: original.x2 + dx,
            y2: original.y2 + dy,
            ..original.clone()
        })
    }

    /// Places an object of the default size of `kind` at the egui pixel `pos`
    /// (context menu, no drawing gesture): same [`GraphEvent::ShapeChanged`]
    /// event as the click of the drawing tool, and same selection. Returns
    /// `false` if `pos` falls in no drawn panel or if an axis forbids the
    /// conversion to data.
    pub fn insert_shape_at(&mut self, pos: Pos2, kind: ShapeKind, spec: &PlotSpec) -> bool {
        let Some((panel_index, _)) = self.panel_at(pos) else {
            return false;
        };
        let Some(press) = self.figure_position(pos) else {
            return false;
        };
        let Some(panel) = self
            .last
            .as_ref()
            .and_then(|last| last.panels.get(panel_index))
        else {
            return false;
        };
        let Some(target) = ShapeRef::next_in(spec, panel.index) else {
            return false;
        };
        let original = ShapeSpec::new(kind, 0.0, 0.0, 0.0, 0.0);
        let Some(shape) = self.moved_shape(target, &original, press, press + default_extent(kind))
        else {
            return false;
        };
        self.set_shape_preview(Some((target, shape.clone())));
        self.events.push(GraphEvent::ShapeChanged { target, shape });
        self.hold_preview();
        self.select_shape(Some(target));
        self.tool = GraphTool::Pointer;
        true
    }

    /// Active drawing tool: a primary drag draws the object (preview),
    /// release commits it in a single [`GraphEvent::ShapeChanged`] and then
    /// hands control back to the Pointer tool.
    pub(crate) fn handle_shape_draw(&mut self, response: &Response, spec: &PlotSpec, pan: bool) {
        let Some(kind) = self.tool.shape_kind() else {
            return;
        };
        // Ctrl or middle button at press: it is a pan, not a drawing, and a
        // Ctrl + click places nothing (#318).
        if pan {
            return;
        }
        // Egui also calls small drags a "click". Their 4 px guard is
        // unchanged: only a nearly still press (1 px of screen tolerance)
        // inserts the default size.
        if response.clicked_by(PointerButton::Primary) {
            let drag = self.shape_drag.take();
            let current = response
                .interact_pointer_pos()
                .and_then(|pos| self.figure_position(pos));
            if let (Some(drag), Some(current)) = (drag, current) {
                let scale = self.display_scale().unwrap_or(1.0);
                if (current - drag.press).length() * scale <= 1.0 {
                    if let Some(shape) = self.moved_shape(
                        drag.target,
                        &drag.original,
                        drag.press,
                        drag.press + default_extent(kind),
                    ) {
                        self.set_shape_preview(Some((drag.target, shape.clone())));
                        self.events.push(GraphEvent::ShapeChanged {
                            target: drag.target,
                            shape,
                        });
                        self.hold_preview();
                        self.select_shape(Some(drag.target));
                        self.tool = GraphTool::Pointer;
                        return;
                    }
                }
            }
            self.set_shape_preview(None);
            return;
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(drag) = self.shape_drag.take() else {
                return;
            };
            match self.shape_preview.clone() {
                Some((target, shape)) if drag.armed => {
                    self.events.push(GraphEvent::ShapeChanged { target, shape });
                    self.hold_preview();
                    self.select_shape(Some(target));
                    self.tool = GraphTool::Pointer;
                }
                _ => self.set_shape_preview(None),
            }
            return;
        }
        let primary_down = response.is_pointer_button_down_on()
            && response
                .ctx
                .input(|input| input.pointer.button_down(PointerButton::Primary));
        if !response.dragged_by(PointerButton::Primary) && !primary_down {
            return;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        if self.shape_drag.is_none() {
            let origin = pos - response.drag_delta();
            let Some((panel_index, _)) = self.panel_at(origin) else {
                return;
            };
            let Some(press) = self.figure_position(origin) else {
                return;
            };
            let Some(panel) = self
                .last
                .as_ref()
                .and_then(|last| last.panels.get(panel_index))
            else {
                return;
            };
            let Some(target) = ShapeRef::next_in(spec, panel.index) else {
                return;
            };
            self.shape_drag = Some(ShapeDrag {
                target,
                original: ShapeSpec::new(kind, 0.0, 0.0, 0.0, 0.0),
                mode: DragMode::Draw,
                press,
                start: (press, press),
                armed: false,
            });
        }
        let Some(mut drag) = self.shape_drag.clone() else {
            return;
        };
        let Some(current) = self.figure_position(pos) else {
            return;
        };
        let scale = self.display_scale().unwrap_or(1.0);
        drag.armed |= (current - drag.press).length() * scale > MIN_DRAW_DISTANCE;
        let preview = drag
            .armed
            .then(|| self.moved_shape(drag.target, &drag.original, drag.press, current))
            .flatten();
        if let Some(shape) = preview {
            self.set_shape_preview(Some((drag.target, shape)));
        }
        self.shape_drag = Some(drag);
    }

    /// Pointer tool: a drag starting from a handle of the selected object
    /// resizes it, a drag starting from an object moves it. Returns `true` if
    /// the gesture is an object drag (navigation does nothing with it).
    pub(crate) fn handle_shape_drag(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(drag) = self.shape_drag.take() else {
                return false;
            };
            match self.shape_preview.clone() {
                Some((target, shape)) if shape != drag.original => {
                    self.events.push(GraphEvent::ShapeChanged { target, shape });
                    self.hold_preview();
                }
                _ => self.set_shape_preview(None),
            }
            return true;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.shape_drag.is_some();
        };
        if self.shape_drag.is_none() {
            let origin = pos - response.drag_delta();
            let Some(press) = self.figure_position(origin) else {
                return false;
            };
            let scale = self.display_scale().unwrap_or(1.0);
            let grabbed = self.selected_shape.and_then(|selected| {
                let drawn = self.drawn_shape(selected)?;
                handles(drawn)
                    .into_iter()
                    .find(|(_, at)| (*at - press).length() * scale <= HANDLE_GRAB)
                    .map(|(handle, _)| (selected, DragMode::Resize(handle)))
            });
            let Some((target, mode)) =
                grabbed.or_else(|| self.shape_at(origin).map(|t| (t, DragMode::Move)))
            else {
                return false;
            };
            let (Some(original), Some(drawn)) =
                (target.get(spec).cloned(), self.drawn_shape(target).copied())
            else {
                return false;
            };
            self.select_shape(Some(target));
            self.shape_drag = Some(ShapeDrag {
                target,
                original,
                mode,
                press,
                start: (point(drawn.p1), point(drawn.p2)),
                armed: true,
            });
        }
        let Some(drag) = self.shape_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        let (a, b) = match drag.mode {
            DragMode::Draw => return true,
            DragMode::Move => {
                // On linear axes, the gap is carried **in data**: the drawn
                // points are rounded to the pixel, passing them through
                // pixels again would trim the size by a pixel at each move.
                if let Some(shape) = self.shifted_shape(&drag, current) {
                    self.set_shape_preview(Some((drag.target, shape)));
                    return true;
                }
                let delta = current - drag.press;
                (drag.start.0 + delta, drag.start.1 + delta)
            }
            DragMode::Resize(handle) => resized_points(
                drag.start,
                handle,
                current,
                response.ctx.input(|input| input.modifiers.shift),
            ),
        };
        if let Some(shape) = self.moved_shape(drag.target, &drag.original, a, b) {
            self.set_shape_preview(Some((drag.target, shape)));
        }
        true
    }

    /// The handles (and the box) of the selected object, over the figure.
    pub(crate) fn paint_shape_selection(&self, painter: &Painter) {
        let (Some(target), Some(display), Some(scale)) =
            (self.selected_shape, self.display_rect, self.display_scale())
        else {
            return;
        };
        let Some(drawn) = self.drawn_shape(target) else {
            return;
        };
        let to_screen = |p: Pos2| display.min + p.to_vec2() * scale;
        let stroke = Stroke::new(1.0, SELECTION_BLUE);
        if drawn.kind.is_closed() {
            let rect = Rect::from_two_pos(to_screen(point(drawn.p1)), to_screen(point(drawn.p2)));
            painter.add(Shape::rect_stroke(rect, 0.0, stroke, StrokeKind::Middle));
        }
        for (_, at) in handles(drawn) {
            let rect = Rect::from_center_size(
                to_screen(at),
                egui::vec2(HANDLE_HALF * 2.0, HANDLE_HALF * 2.0),
            );
            painter.rect_filled(rect, 0.0, Color32::WHITE);
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Middle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::ShapeKind;

    fn drawn(kind: ShapeKind) -> DrawnShape {
        DrawnShape {
            index: 0,
            kind,
            p1: (10.0, 20.0),
            p2: (50.0, 60.0),
            filled: false,
            width: 1.5,
        }
    }

    #[test]
    fn a_rectangle_has_eight_handles_and_an_arrow_two() {
        let h = handles(&drawn(ShapeKind::Rectangle));
        assert_eq!(h.len(), 8);
        let at = |handle| h.iter().find(|(x, _)| *x == handle).map(|(_, p)| *p);
        assert_eq!(at(Handle::Box(-1, -1)), Some(Pos2::new(10.0, 20.0)));
        assert_eq!(at(Handle::Box(1, 1)), Some(Pos2::new(50.0, 60.0)));
        assert_eq!(at(Handle::Box(0, -1)), Some(Pos2::new(30.0, 20.0)));
        assert_eq!(handles(&drawn(ShapeKind::Arrow)).len(), 2);
    }
    #[test]
    fn shift_keeps_the_initial_ratio_on_the_eight_handles() {
        let start = (Pos2::new(10.0, 20.0), Pos2::new(90.0, 60.0));
        for hx in [-1, 0, 1] {
            for hy in [-1, 0, 1] {
                if hx == 0 && hy == 0 {
                    continue;
                }
                // Enlarge, shrink, cross the anchor point and land exactly
                // on its vertical: no infinite ratio.
                for at in [
                    Pos2::new(-30.0, -40.0),
                    Pos2::new(40.0, 35.0),
                    Pos2::new(130.0, 110.0),
                    Pos2::new(90.0, 110.0),
                ] {
                    let handle = Handle::Box(hx, hy);
                    let (a, b) = resized_points(start, handle, at, true);
                    assert!(((b.x - a.x).abs() - 2.0 * (b.y - a.y).abs()).abs() < 1e-4);
                    if hx == 0 {
                        assert_eq!((a.x + b.x) / 2.0, 50.0);
                    } else if hx == -1 {
                        assert_eq!(b.x, 90.0);
                    } else {
                        assert_eq!(a.x, 10.0);
                    }
                    if hy == 0 {
                        assert_eq!((a.y + b.y) / 2.0, 40.0);
                    } else if hy == -1 {
                        assert_eq!(b.y, 60.0);
                    } else {
                        assert_eq!(a.y, 20.0);
                    }
                }
            }
        }
        assert_eq!(
            resized_points(start, Handle::Box(1, 1), Pos2::new(130.0, 110.0), false),
            (start.0, Pos2::new(130.0, 110.0))
        );
    }

    #[test]
    fn shift_keeps_the_direction_of_lines_and_supports_flat_objects() {
        for end in [
            Pos2::new(80.0, 40.0),
            Pos2::new(80.0, 0.0),
            Pos2::new(0.0, 40.0),
        ] {
            let start = (Pos2::ZERO, end);
            for endpoint in [1, 2] {
                let (a, b) = resized_points(
                    start,
                    Handle::Endpoint(endpoint),
                    Pos2::new(120.0, 95.0),
                    true,
                );
                let delta = b - a;
                assert!((delta.x * end.y - delta.y * end.x).abs() < 1e-3);
            }
        }
        let flat = (Pos2::ZERO, Pos2::new(80.0, 0.0));
        let current = Pos2::new(100.0, 30.0);
        assert_eq!(
            resized_points(flat, Handle::Box(1, 1), current, true),
            (Pos2::ZERO, current)
        );
    }
}
