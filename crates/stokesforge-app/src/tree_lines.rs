//! L-shaped tree connectors (├ └ │) shared by the Object Manager and the
//! Project Manager (#112).
//!
//! egui's indent line (`indent_has_left_vline`) runs down the whole height of a
//! node's body, grandchildren included. Here, each node body reports the
//! vertical center of its direct children; the vertical line stops at the last
//! of them and each child gets a short horizontal stub. The `│` that crosses a
//! child's subtree exists only because this vertical line also covers the rows
//! that follow.
//!
//! Everything is painted with the `Painter` (no glyph), using the theme's
//! outline stroke.

use eframe::egui::{Pos2, Ui};

/// Call once at the root of a tree: turns off egui's indent line, replaced by
/// [`tree_children`].
pub fn begin_tree(ui: &mut Ui) {
    ui.visuals_mut().indent_has_left_vline = false;
}

/// Vertical centers of a node's direct children, filled in by the body.
#[derive(Default)]
pub struct TreeBranch {
    ys: Vec<f32>,
}

impl TreeBranch {
    /// Declares a direct child whose row is centered at `y` (for a collapsible
    /// node, the center of its header, not of its body).
    pub fn mark(&mut self, y: f32) {
        self.ys.push(y);
    }
}

/// Segments of one level: the vertical from `top` to the last child, then one
/// horizontal from `x` to `stub_end` per child. Empty without children.
pub fn connector_segments(top: f32, x: f32, stub_end: f32, ys: &[f32]) -> Vec<[Pos2; 2]> {
    let Some(last) = ys.iter().copied().reduce(f32::max) else {
        return Vec::new();
    };
    let mut segments = vec![[Pos2::new(x, top.min(last)), Pos2::new(x, last)]];
    segments.extend(
        ys.iter()
            .map(|&y| [Pos2::new(x, y), Pos2::new(stub_end.max(x), y)]),
    );
    segments
}

/// Body of a node: `add` draws the children and declares their row through
/// [`TreeBranch::mark`]; the connectors are painted afterwards.
pub fn tree_children<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui, &mut TreeBranch) -> R) -> R {
    let cursor = ui.cursor();
    let line_x = ui
        .painter()
        .round_to_pixel_center(cursor.left() - ui.spacing().indent * 0.5);
    let top = cursor.top();
    let stub_end = cursor.left() - 2.0;
    let mut branch = TreeBranch::default();
    let result = add(ui, &mut branch);
    let painter = ui.painter();
    let ys: Vec<f32> = branch
        .ys
        .iter()
        .map(|&y| painter.round_to_pixel_center(y))
        .collect();
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    for [from, to] in connector_segments(top, line_x, stub_end, &ys) {
        painter.line_segment([from, to], stroke);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_child_no_line() {
        assert!(connector_segments(0.0, 5.0, 12.0, &[]).is_empty());
    }

    #[test]
    fn the_vertical_stops_at_the_last_child() {
        let segments = connector_segments(10.0, 5.0, 12.0, &[20.0, 40.0, 70.0]);
        assert_eq!(segments[0], [Pos2::new(5.0, 10.0), Pos2::new(5.0, 70.0)]);
        // One horizontal stub per child, from the vertical to the row.
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[3], [Pos2::new(5.0, 70.0), Pos2::new(12.0, 70.0)]);
    }
}
