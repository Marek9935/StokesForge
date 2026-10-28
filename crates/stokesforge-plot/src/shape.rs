//! The graphic objects of a panel — rectangle, ellipse, line, arrow
//! (issue #76), alongside the free texts ([`crate::AnnotationSpec`]).
//!
//! # Decisions of 2026-10-02
//!
//! - **Anchored to the data**, like a free text: the two points defining
//!   the object are in data coordinates of the panel (left or right axis),
//!   so the object follows the zoom and pan of the view;
//! - **a single ordered list** ([`crate::PanelSpec::shapes`]): the stacking
//!   order is the creation order (the last is on top), changeable
//!   with [`ShapeRef::bring_to_front`] / [`ShapeRef::send_to_back`]. Free texts
//!   are always plotted above the shapes;
//! - two points `(x1, y1)` / `(x2, y2)` for the four types: the opposite
//!   corners of a rectangle or of the box of an ellipse, the ends of a
//!   line, the tail and the tip of an arrow. The drag direction is therefore
//!   kept (an arrow drawn from right to left points left).
//!
//! Colors follow the repository convention: `#RRGGBB` or `#RRGGBBAA` (so
//! the fill opacity is the alpha of [`ShapeSpec::fill_color`]); empty =
//! automatic for the line (contrast with the background), **no fill**
//! for the fill.

use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// The kind of a graphic object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// Rectangle aligned with the figure axes, defined by two opposite corners.
    Rectangle,
    /// Ellipse aligned with the figure axes, inscribed in that rectangle.
    Ellipse,
    /// Line segment.
    Line,
    /// Line segment ending in a tip at `(x2, y2)`.
    Arrow,
}

impl ShapeKind {
    /// True if the kind has an area, hence a fill.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse)
    }
}

/// A graphic object placed on a panel, anchored to the data.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    /// Its kind.
    pub kind: ShapeKind,
    /// Visible at rendering; hiding keeps the content and its address (#81).
    pub visible: bool,
    /// Abscissa of the first point (corner, tail), in data.
    pub x1: f64,
    /// Ordinate of the first point, in data of the axis [`Self::axis`].
    pub y1: f64,
    /// Abscissa of the second point (opposite corner, tip), in data.
    pub x2: f64,
    /// Ordinate of the second point, in data of the axis [`Self::axis`].
    pub y2: f64,
    /// Ordinate it is attached to: left or right, like a series.
    pub axis: SeriesAxis,
    /// Line color; empty = automatic.
    pub line_color: String,
    /// Line thickness, like [`crate::LineSpec::width`].
    pub line_width: f64,
    /// Fill color (rectangle and ellipse); empty = none.
    pub fill_color: String,
}

impl ShapeSpec {
    /// Default line thickness.
    pub const DEFAULT_WIDTH: f64 = 1.5;
    /// Thickness accepted at rendering; beyond it, clamped to these bounds.
    pub const WIDTH_RANGE: (f64, f64) = (0.5, 20.0);

    /// An object of `kind` between `(x1, y1)` and `(x2, y2)`, on the left
    /// ordinate, automatic line, no fill.
    pub fn new(kind: ShapeKind, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            kind,
            visible: true,
            x1,
            y1,
            x2,
            y2,
            axis: SeriesAxis::Left,
            line_color: String::new(),
            line_width: Self::DEFAULT_WIDTH,
            fill_color: String::new(),
        }
    }

    /// The thickness actually plotted: [`Self::line_width`] clamped to
    /// [`Self::WIDTH_RANGE`], and the default for a non-finite value.
    pub fn effective_width(&self) -> f64 {
        if self.line_width.is_finite() {
            self.line_width
                .clamp(Self::WIDTH_RANGE.0, Self::WIDTH_RANGE.1)
        } else {
            Self::DEFAULT_WIDTH
        }
    }

    /// True if the four coordinates are finite; otherwise the object is ignored
    /// at rendering (damaged file) rather than projecting a `NaN`.
    pub fn is_finite(&self) -> bool {
        self.x1.is_finite() && self.y1.is_finite() && self.x2.is_finite() && self.y2.is_finite()
    }
}

/// The address of a graphic object in a [`PlotSpec`]: its layer, by its
/// index in [`PlotSpec::panels`], and its rank in [`crate::PanelSpec::shapes`]
/// (the stacking order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeRef {
    /// Index of the layer in [`PlotSpec::panels`].
    pub panel: usize,
    /// Rank of the object in its panel.
    pub index: usize,
}

impl ShapeRef {
    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        (self.panel < spec.panels.len()).then_some(self.panel)
    }

    /// The designated object, if it exists.
    pub fn get<'a>(&self, spec: &'a PlotSpec) -> Option<&'a ShapeSpec> {
        spec.panels
            .get(self.panel_index(spec)?)?
            .shapes
            .get(self.index)
    }

    /// Places `shape` at this address: replaces the existing object, or adds it
    /// if `index` is the rank after the last. Returns `false` without changing
    /// anything if the panel does not exist or if `index` is further.
    pub fn set(&self, spec: &mut PlotSpec, shape: ShapeSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        match self.index.cmp(&panel.shapes.len()) {
            std::cmp::Ordering::Less => panel.shapes[self.index] = shape,
            std::cmp::Ordering::Equal => panel.shapes.push(shape),
            std::cmp::Ordering::Greater => return false,
        }
        true
    }

    /// Removes the designated object; returns `false` if it does not exist.
    pub fn remove(&self, spec: &mut PlotSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        if self.index >= panel.shapes.len() {
            return false;
        }
        panel.shapes.remove(self.index);
        true
    }

    /// Puts the object above all the others of its panel. Returns its
    /// new address, or `None` if it does not exist.
    pub fn bring_to_front(&self, spec: &mut PlotSpec) -> Option<Self> {
        let panel = spec.panels.get_mut(self.panel_index(spec)?)?;
        if self.index >= panel.shapes.len() {
            return None;
        }
        let shape = panel.shapes.remove(self.index);
        panel.shapes.push(shape);
        Some(Self {
            index: panel.shapes.len() - 1,
            ..*self
        })
    }

    /// Puts the object below all the others of its panel. Returns its new
    /// address, or `None` if it does not exist.
    pub fn send_to_back(&self, spec: &mut PlotSpec) -> Option<Self> {
        let panel = spec.panels.get_mut(self.panel_index(spec)?)?;
        if self.index >= panel.shapes.len() {
            return None;
        }
        let shape = panel.shapes.remove(self.index);
        panel.shapes.insert(0, shape);
        Some(Self { index: 0, ..*self })
    }

    /// The address of the next object added to the layer `panel`, or `None` if that
    /// layer does not exist.
    pub fn next_in(spec: &PlotSpec, panel: usize) -> Option<Self> {
        let source = spec.panels.get(panel)?;
        Some(Self {
            panel,
            index: source.shapes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64) -> ShapeSpec {
        ShapeSpec::new(ShapeKind::Rectangle, x, 0.0, x + 1.0, 1.0)
    }

    #[test]
    fn an_address_adds_replaces_and_removes() {
        let mut spec = PlotSpec::empty("A");
        let new = ShapeRef::next_in(&spec, 0).expect("panel 0,0");
        assert_eq!(new.index, 0);
        assert!(new.set(&mut spec, rect(0.0)));
        assert!(new.set(&mut spec, rect(5.0)));
        assert_eq!(spec.panels[0].shapes.len(), 1, "replaced, not added");
        assert_eq!(new.get(&spec).map(|s| s.x1), Some(5.0));
        assert!(!ShapeRef { index: 5, ..new }.set(&mut spec, rect(0.0)));
        assert!(ShapeRef::next_in(&spec, 3).is_none());
        assert!(new.remove(&mut spec));
        assert!(!new.remove(&mut spec));
    }

    #[test]
    fn front_and_back_reorder_and_return_the_new_address() {
        let mut spec = PlotSpec::empty("A");
        for x in [0.0, 1.0, 2.0] {
            let at = ShapeRef::next_in(&spec, 0).unwrap();
            at.set(&mut spec, rect(x));
        }
        let first = ShapeRef { panel: 0, index: 0 };
        let moved = first.bring_to_front(&mut spec).unwrap();
        assert_eq!(moved.index, 2);
        let xs: Vec<f64> = spec.panels[0].shapes.iter().map(|s| s.x1).collect();
        assert_eq!(xs, [1.0, 2.0, 0.0]);
        let back = moved.send_to_back(&mut spec).unwrap();
        assert_eq!(back.index, 0);
        let xs: Vec<f64> = spec.panels[0].shapes.iter().map(|s| s.x1).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0]);
        assert!(ShapeRef { index: 9, ..first }
            .bring_to_front(&mut spec)
            .is_none());
    }

    #[test]
    fn the_thickness_is_clamped_to_its_bounds() {
        let mut shape = rect(0.0);
        shape.line_width = 1e9;
        assert_eq!(shape.effective_width(), ShapeSpec::WIDTH_RANGE.1);
        shape.line_width = f64::NAN;
        assert_eq!(shape.effective_width(), ShapeSpec::DEFAULT_WIDTH);
    }
}
