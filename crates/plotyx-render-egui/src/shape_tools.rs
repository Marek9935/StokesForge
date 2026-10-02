//! Les objets graphiques à l'écran (issue #76) : tracer au glisser avec les
//! outils Rectangle / Ellipse / Ligne / Flèche, sélectionner, déplacer,
//! redimensionner par les poignées.
//!
//! Même principe que les textes libres (#35) : **un geste = une seule
//! modification du modèle**. L'aperçu vit dans la vue ([`GraphView`], champ
//! `shape_preview`), posé sur le clone du spec à la construction de la figure,
//! et un seul [`GraphEvent::ShapeChanged`] part au relâchement. Les
//! coordonnées du geste sont des pixels de la figure ; elles sont converties
//! en données par [`PanelTransform`] à chaque image, donc l'objet reste ancré
//! aux données (il suivra le zoom ensuite).
//!
//! Les poignées d'un rectangle ou d'une ellipse sont les huit points de sa
//! boîte (quatre coins, quatre milieux de côté) ; celles d'une ligne ou d'une
//! flèche, ses deux extrémités. Elles sont testées **avant** l'objet : un
//! coin de boîte d'ellipse n'est pas sur le tracé.

use egui::{Color32, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke, StrokeKind};
use plotyx_plot::{AxisScale, PlotSpec, SeriesAxis, ShapeRef, ShapeSpec};
use plotyx_render::shapes::DrawnShape;
use plotyx_render::{PanelTransform, PlotComponent};

use crate::{GraphEvent, GraphTool, GraphView};

/// Distance, en pixels d'écran, sous laquelle un geste de tracé n'est qu'un
/// tremblement de main et ne crée rien.
const MIN_DRAW_DISTANCE: f32 = 4.0;
/// Demi-côté d'une poignée, en pixels d'écran, et rayon où elle se saisit.
const HANDLE_HALF: f32 = 4.0;
const HANDLE_GRAB: f32 = 8.0;
const SELECTION_BLUE: Color32 = Color32::from_rgb(90, 150, 255);

/// Une poignée de redimensionnement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handle {
    /// Un point de la boîte d'un rectangle ou d'une ellipse : `(-1|0|1,
    /// -1|0|1)` selon le côté (gauche/haut = -1), hors `(0, 0)`.
    Box(i8, i8),
    /// Une extrémité de ligne ou de flèche : 1 (queue) ou 2 (pointe).
    Endpoint(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// Tracé d'un objet neuf.
    Draw,
    Move,
    Resize(Handle),
}

/// Un geste sur un objet graphique, en cours.
#[derive(Debug, Clone)]
pub(crate) struct ShapeDrag {
    target: ShapeRef,
    original: ShapeSpec,
    mode: DragMode,
    /// Point d'appui, en repère de la figure.
    press: Pos2,
    /// Les deux points de l'objet au début du geste, en repère de la figure.
    start: (Pos2, Pos2),
    /// Vrai dès que le pointeur s'est éloigné d'assez pour que le geste compte.
    armed: bool,
}

fn point(p: (f64, f64)) -> Pos2 {
    Pos2::new(p.0 as f32, p.1 as f32)
}

/// Les poignées d'un objet tracé, avec leur position en repère de la figure.
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

impl GraphView {
    /// L'aperçu d'un objet graphique, s'il y en a un.
    pub fn shape_preview(&self) -> Option<&(ShapeRef, ShapeSpec)> {
        self.shape_preview.as_ref()
    }

    /// Pose (ou retire) l'aperçu d'un objet graphique : la figure le montre à
    /// la prochaine image, sans toucher au modèle.
    pub fn set_shape_preview(&mut self, preview: Option<(ShapeRef, ShapeSpec)>) {
        self.preview_until = None;
        if self.shape_preview != preview {
            self.shape_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// L'objet graphique sélectionné, s'il y en a un.
    pub fn selected_shape(&self) -> Option<ShapeRef> {
        self.selected_shape
    }

    /// Sélectionne un objet graphique (ou plus rien) — par exemple après
    /// l'avoir passé au premier plan, ce qui change son rang.
    pub fn select_shape(&mut self, target: Option<ShapeRef>) {
        self.selected_shape = target;
        if target.is_some() {
            self.selected_annotation = None;
            self.selected_text = None;
        }
    }

    /// L'objet graphique sous le pixel egui `pos`, cherché dans la dernière
    /// figure tracée.
    pub fn shape_at(&self, pos: Pos2) -> Option<ShapeRef> {
        match self.component_at(pos)? {
            PlotComponent::Shape { panel, index } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(ShapeRef {
                    row: drawn.row,
                    column: drawn.column,
                    index,
                })
            }
            _ => None,
        }
    }

    /// L'objet tel qu'il a été tracé dans la dernière figure.
    pub fn drawn_shape(&self, target: ShapeRef) -> Option<&DrawnShape> {
        self.last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.row == target.row && panel.column == target.column)?
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
            .find(|panel| panel.row == target.row && panel.column == target.column)?;
        PanelTransform::of_axis(panel, axis == SeriesAxis::Right)
    }

    /// Reporte deux points de la figure en données dans une copie de `shape`.
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

    /// `drag.original` décalée de l'écart pointeur, calculé en données ;
    /// `None` si un axe n'est pas linéaire (l'écart n'y est pas constant).
    fn shifted_shape(&self, drag: &ShapeDrag, current: Pos2) -> Option<ShapeSpec> {
        let panel = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.row == drag.target.row && panel.column == drag.target.column)?;
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

    /// Outil de tracé actif : un glisser principal trace l'objet (aperçu),
    /// le relâchement le valide en un seul [`GraphEvent::ShapeChanged`] puis
    /// rend la main à l'outil Pointeur.
    pub(crate) fn handle_shape_draw(&mut self, response: &Response, spec: &PlotSpec) {
        let Some(kind) = self.tool.shape_kind() else {
            return;
        };
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
        if !response.dragged_by(PointerButton::Primary) {
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
            let Some(target) = ShapeRef::next_in(spec, panel.row, panel.column) else {
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

    /// Outil Pointeur : un glisser parti d'une poignée de l'objet sélectionné
    /// le redimensionne, un glisser parti d'un objet le déplace. Rend `true`
    /// si le geste est un glisser d'objet (la navigation n'en fait rien).
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
                // Sur des axes linéaires, l'écart est reporté **en données** :
                // les points tracés sont arrondis au pixel, les refaire
                // passer par les pixels rognerait la taille d'un pixel à
                // chaque déplacement.
                if let Some(shape) = self.shifted_shape(&drag, current) {
                    self.set_shape_preview(Some((drag.target, shape)));
                    return true;
                }
                let delta = current - drag.press;
                (drag.start.0 + delta, drag.start.1 + delta)
            }
            DragMode::Resize(Handle::Endpoint(1)) => (current, drag.start.1),
            DragMode::Resize(Handle::Endpoint(_)) => (drag.start.0, current),
            DragMode::Resize(Handle::Box(hx, hy)) => {
                let (mut left, mut right) = (
                    drag.start.0.x.min(drag.start.1.x),
                    drag.start.0.x.max(drag.start.1.x),
                );
                let (mut top, mut bottom) = (
                    drag.start.0.y.min(drag.start.1.y),
                    drag.start.0.y.max(drag.start.1.y),
                );
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
                (Pos2::new(left, top), Pos2::new(right, bottom))
            }
        };
        if let Some(shape) = self.moved_shape(drag.target, &drag.original, a, b) {
            self.set_shape_preview(Some((drag.target, shape)));
        }
        true
    }

    /// Les poignées (et la boîte) de l'objet sélectionné, par-dessus la
    /// figure.
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
    use plotyx_plot::ShapeKind;

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
    fn un_rectangle_a_huit_poignees_et_une_fleche_deux() {
        let h = handles(&drawn(ShapeKind::Rectangle));
        assert_eq!(h.len(), 8);
        let at = |handle| h.iter().find(|(x, _)| *x == handle).map(|(_, p)| *p);
        assert_eq!(at(Handle::Box(-1, -1)), Some(Pos2::new(10.0, 20.0)));
        assert_eq!(at(Handle::Box(1, 1)), Some(Pos2::new(50.0, 60.0)));
        assert_eq!(at(Handle::Box(0, -1)), Some(Pos2::new(30.0, 20.0)));
        assert_eq!(handles(&drawn(ShapeKind::Arrow)).len(), 2);
    }
}
