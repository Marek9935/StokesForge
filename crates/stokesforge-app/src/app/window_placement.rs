//! Placement of floating windows: default sizes, cascade, fixed-size
//! workspace (`WORKSPACE_SIZE`), clamping of a logical rectangle
//! (`clamp_to_workspace`), logical → screen conversion and scrolling to
//! reveal a window.
//!
//! Moved as is from `app.rs` (#101), with no change of behavior. Positions
//! and sizes are in **logical** workspace coordinates; the conversion to
//! screen coordinates happens only at paint time (`workspace_to_screen`).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_session::WindowGeometry;

use super::window_chrome::MIN_CONTENT_WINDOW_SIZE;

/// Default size of floating windows that do not show a graph — see the
/// module's top doc, § Confined floating windows. Trimmed to the available
/// work area if that is smaller.
pub(super) const FLOATING_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 420.0);

/// Default size of a graph window: the reference figure (`480×360`), its
/// return to the initial framing, the header and the margins all fit in it
/// from the opening.
pub(super) const GRAPH_WINDOW_SIZE: egui::Vec2 = egui::vec2(560.0, 460.0);

/// Default width of a [`Tab::Book`](crate::app::tab::Tab::Book) floating window — a third
/// less than [`FLOATING_WINDOW_SIZE`]: a grid does not need to be as wide
/// as a graph to stay readable, an explicit request.
pub(super) const BOOK_WINDOW_WIDTH: f32 = FLOATING_WINDOW_SIZE.x * 2.0 / 3.0;

/// Offset, in points, between two floating windows opened one after the
/// other (cascade) — see [`StokesForgeApp::next_cascade`](crate::app::StokesForgeApp::next_cascade).
pub(super) const CASCADE_STEP: f32 = 24.0;

/// Number of cascade steps before wrapping back to zero, so that a
/// succession of windows never drifts out of the work area.
pub(super) const CASCADE_WRAP: u32 = 8;

/// Floating rectangle of a persisted geometry.
pub(super) fn geometry_rect(geometry: &WindowGeometry) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(geometry.x as f32, geometry.y as f32),
        egui::vec2(geometry.width as f32, geometry.height as f32),
    )
}

/// Size to paint for a floating window this frame. The logical size (`current_size`) as is, except while
/// collapsed (B3, `retours-2026-09-23-bugs.md`): the content
/// is then no longer painted (only [`window_header`](crate::app::window_chrome::window_header) is), and forcing the
/// expanded size would paint a frame — and a highlight outline —
/// around an empty area.
///
/// Reused as is by [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs) (B9) to
/// compute the size to send to the OS viewport through
/// `ViewportCommand::InnerSize`: same header band, same collapsed
/// height — only what carries this size changes (the frame painted by
/// [`show_floating_window`](crate::app::window_chrome::show_floating_window) on the floating side, a real OS viewport on the detached side).
pub(super) fn floating_window_display_size(
    current_size: egui::Vec2,
    was_collapsed: bool,
    header_height: f32,
    window_margin: egui::Vec2,
) -> egui::Vec2 {
    if was_collapsed {
        egui::vec2(current_size.x, header_height + window_margin.y)
    } else {
        current_size
    }
}

/// Rectangle to keep in `window_geometries` (persisted in `.stf`) from the
/// rectangle actually painted this frame. The position comes from that
/// rectangle, but the size must stay the window's **logical** size: during
/// a collapse (B3), the rectangle actually painted is reduced to
/// [`floating_window_display_size`], and persisting it as is would make the
/// window reappear collapsed — even once expanded again — the next time
/// the project is opened.
///
/// Reused as is by [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs) (B9,
/// `retours-2026-09-23-bugs.md`) with
/// `DetachedTabViewModel::expanded_size` as the logical size: the
/// rectangle reported by the OS viewport during a collapse is, there too,
/// the reduced rectangle that the `InnerSize` sent on the switch produced,
/// not the size to persist.
pub(super) fn floating_window_geometry_to_persist(
    painted_rect: egui::Rect,
    current_size: egui::Vec2,
) -> egui::Rect {
    egui::Rect::from_min_size(painted_rect.min, current_size)
}

/// Fixed size of the workspace (the MDI area that carries the floating
/// windows), independent of the main window's size — before this task,
/// `workspace_rect` was `ui.max_rect()`, so it shrank with the `eframe`
/// window and crushed the floating windows against its edges. A "generous"
/// bench rather than one fitted to a given screen; making it really
/// adjustable by the user (persistence, settings UI) remains to be done —
/// see `docs/work-packages`. Below this size, [`StokesForgeApp::ui`](eframe::App::ui) scrolls
/// the workspace ([`StokesForgeApp::workspace_scroll`](crate::app::StokesForgeApp::workspace_scroll)) rather than
/// shrinking or truncating the windows it contains.
pub(super) const WORKSPACE_SIZE: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// Maximum scroll (per axis) for a visible area of size `viewport_size` to
/// stay entirely inside [`WORKSPACE_SIZE`] — zero on an axis where the
/// whole workspace already fits, nothing scrolls there then.
pub(super) fn workspace_max_scroll(viewport_size: egui::Vec2) -> egui::Vec2 {
    (WORKSPACE_SIZE - viewport_size).max(egui::Vec2::ZERO)
}

/// Screen position of a workspace point (logical coordinates, independent
/// of scrolling) — the origin of the visible area minus the current
/// scroll.
pub(super) fn workspace_to_screen(
    pos: egui::Pos2,
    viewport_min: egui::Pos2,
    scroll: egui::Vec2,
) -> egui::Pos2 {
    viewport_min + (pos.to_vec2() - scroll)
}

/// Minimal scroll to apply to `scroll` for `target` (a rectangle in the
/// workspace) to become entirely visible again in an area of size
/// `viewport_size` — unchanged if `target` already is. Used to bring back
/// on screen a window that is already open but currently scrolled out of
/// view (click in the Project Manager): unlike
/// [`Response::scroll_to_me`](egui::Response::scroll_to_me) on the grid/sheet tabs side, the workspace
/// is not a classic `egui::ScrollArea` (the windows it carries paint on
/// their own layer, independent of any ambient scroll area), so there is
/// no native mechanism to reuse here.
pub(super) fn workspace_scroll_to_reveal(
    scroll: egui::Vec2,
    viewport_size: egui::Vec2,
    target: egui::Rect,
) -> egui::Vec2 {
    let max_scroll = workspace_max_scroll(viewport_size);
    let mut revealed = scroll;
    for axis in 0..2 {
        if target.min[axis] < revealed[axis] {
            revealed[axis] = target.min[axis];
        } else if target.max[axis] > revealed[axis] + viewport_size[axis] {
            revealed[axis] = target.max[axis] - viewport_size[axis];
        }
        revealed[axis] = revealed[axis].clamp(0.0, max_scroll[axis]);
    }
    revealed
}

/// Clamps a logical rectangle to the workspace: size reduced to at most
/// [`WORKSPACE_SIZE`], then position slid so that it fits entirely. This is
/// the only clamp applied to floating windows — egui adds no other (see
/// [`show_floating_window`](crate::app::window_chrome::show_floating_window)), so what this function returns is exactly what is
/// painted, outlined and persisted.
pub(super) fn clamp_to_workspace(rect: egui::Rect) -> egui::Rect {
    let size = rect.size().min(WORKSPACE_SIZE);
    let max_min = (WORKSPACE_SIZE - size).to_pos2();
    egui::Rect::from_min_size(rect.min.clamp(egui::Pos2::ZERO, max_min), size)
}

/// What a graph window remembers of the figure it last painted (#394), so that
/// the window follows the figure: when the figure's logical size changes (a cell
/// added or removed, an undo, a size field, a handle drag, an axis or a title
/// that makes the image grow around a fixed plot area, `pyst`), the window grows
/// or shrinks by the difference at the display scale it had, top-left fixed.
/// Window geometry stays in the shell (invariant 2): app-state never sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FigureFollow {
    /// Size of the figure in points the last time it was recorded.
    size: (u32, u32),
    /// Screen points per figure point: the "display scale" the window keeps.
    scale: f32,
}

impl FigureFollow {
    /// Records the figure `graph` paints this frame (`size` in points, `scale`
    /// the display scale of its view) and returns how far the window must grow
    /// (negative: shrink), in screen points, if the figure's size changed since
    /// the previous record. The scale used is the one from before the change, so
    /// that the figure keeps its display scale. Nothing is returned the first
    /// time a graph is seen: opening or loading a project never resizes a window.
    pub(super) fn observe(
        follows: &mut HashMap<NodeId, FigureFollow>,
        graph: NodeId,
        size: (u32, u32),
        scale: Option<f32>,
    ) -> Option<egui::Vec2> {
        let scale = scale.filter(|scale| scale.is_finite() && *scale > 0.0);
        let Some(follow) = follows.get_mut(&graph) else {
            follows.insert(
                graph,
                FigureFollow {
                    size,
                    scale: scale.unwrap_or(1.0),
                },
            );
            return None;
        };
        if follow.size == size {
            follow.scale = scale.unwrap_or(follow.scale);
            return None;
        }
        let delta = egui::vec2(
            (i64::from(size.0) - i64::from(follow.size.0)) as f32,
            (i64::from(size.1) - i64::from(follow.size.1)) as f32,
        ) * follow.scale;
        follow.size = size;
        Some(delta)
    }
}

/// The logical size of a floating window of top-left `position` after it grew
/// by `delta`: never below the minimum of a content window, never beyond
/// `limit` (right and down), the far corner, in workspace coordinates, of what
/// is visible: the scroll area's viewport, never past the virtual canvas. The
/// window stops there so that its title bar and buttons stay reachable, and the
/// figure is then contained in it.
pub(super) fn followed_floating_size(
    position: egui::Pos2,
    size: egui::Vec2,
    delta: egui::Vec2,
    limit: egui::Vec2,
) -> egui::Vec2 {
    let room = (limit.min(WORKSPACE_SIZE) - position.to_vec2()).max(MIN_CONTENT_WINDOW_SIZE);
    (size + delta).max(MIN_CONTENT_WINDOW_SIZE).min(room)
}

/// The inner size of a detached OS window after it grew by `delta`: never below
/// the minimum of a content window, never beyond the monitor when it is known.
pub(super) fn followed_viewport_size(
    size: egui::Vec2,
    delta: egui::Vec2,
    monitor: Option<egui::Vec2>,
) -> egui::Vec2 {
    let grown = (size + delta).max(MIN_CONTENT_WINDOW_SIZE);
    match monitor {
        Some(monitor) => grown.min(monitor.max(MIN_CONTENT_WINDOW_SIZE)),
        None => grown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // B3 (`retours-2026-09-23-bugs.md`): a collapsed window
    // kept an outline at its expanded size, a big green frame around an
    // empty area — because the window was forced (`.fixed_size`) to its
    // expanded size even when only its header band is painted.
    #[test]
    fn the_size_requested_from_egui_shrinks_to_the_header_during_a_collapse() {
        let current_size = egui::vec2(400.0, 300.0);
        let window_margin = egui::vec2(12.0, 12.0);

        let expanded = floating_window_display_size(current_size, false, 28.0, window_margin);
        assert_eq!(
            expanded, current_size,
            "expanded, the window keeps its logical size"
        );

        let collapsed = floating_window_display_size(current_size, true, 28.0, window_margin);
        assert_eq!(
            collapsed,
            egui::vec2(400.0, 40.0),
            "collapsed, only the header band (+ the Frame's margins) must be requested from egui"
        );
        assert!(
            collapsed.y < current_size.y,
            "the requested size must be strictly smaller than the expanded size"
        );
        // Only the vertical margin counts: the collapsed width never
        // depends on the horizontal margin.
        assert_eq!(
            floating_window_display_size(current_size, true, 28.0, egui::vec2(50.0, 12.0)),
            collapsed
        );
    }

    #[test]
    fn workspace_to_screen_offsets_by_the_view_origin_and_the_scroll() {
        let viewport_min = egui::pos2(40.0, 20.0);
        assert_eq!(
            workspace_to_screen(
                egui::pos2(500.0, 400.0),
                viewport_min,
                egui::vec2(300.0, 150.0)
            ),
            egui::pos2(240.0, 270.0)
        );
        // Without scrolling, the workspace and the screen differ only by the
        // origin of the visible area — same behavior as the old
        // `workspace_rect.min + position`.
        assert_eq!(
            workspace_to_screen(egui::pos2(10.0, 5.0), viewport_min, egui::Vec2::ZERO),
            viewport_min + egui::vec2(10.0, 5.0)
        );
    }

    #[test]
    fn workspace_max_scroll_is_zero_when_the_workspace_already_fits() {
        assert_eq!(
            workspace_max_scroll(WORKSPACE_SIZE),
            egui::Vec2::ZERO,
            "the visible area is already the workspace's size: nothing to scroll"
        );
        assert_eq!(
            workspace_max_scroll(WORKSPACE_SIZE * 2.0),
            egui::Vec2::ZERO,
            "a visible area larger than the workspace must never produce \
             a negative scroll"
        );
        let viewport = egui::vec2(800.0, 600.0);
        assert_eq!(workspace_max_scroll(viewport), WORKSPACE_SIZE - viewport);
        // Each axis on its own: a view wider than the workspace but less
        // tall scrolls only vertically.
        assert_eq!(
            workspace_max_scroll(egui::vec2(WORKSPACE_SIZE.x + 100.0, 600.0)),
            egui::vec2(0.0, WORKSPACE_SIZE.y - 600.0)
        );
    }

    #[test]
    fn workspace_scroll_to_reveal_does_not_move_if_already_visible() {
        let viewport_size = egui::vec2(800.0, 600.0);
        let scroll = egui::vec2(100.0, 50.0);
        let visible = egui::Rect::from_min_size(egui::pos2(200.0, 100.0), egui::vec2(300.0, 200.0));
        assert_eq!(
            workspace_scroll_to_reveal(scroll, viewport_size, visible),
            scroll,
            "a window already entirely in the visible area must not scroll"
        );
        // A window that runs exactly along the edges of the view is visible
        // too: no scrolling by a single point.
        let edge_to_edge = egui::Rect::from_min_size(scroll.to_pos2(), viewport_size);
        assert_eq!(
            workspace_scroll_to_reveal(scroll, viewport_size, edge_to_edge),
            scroll
        );
    }

    #[test]
    fn workspace_scroll_to_reveal_brings_an_out_of_view_window_back_on_screen() {
        let viewport_size = egui::vec2(800.0, 600.0);
        let scroll = egui::Vec2::ZERO;

        // Out of view to the right/bottom: scroll just enough to run along
        // the nearest edge of the targeted window, not center it arbitrarily.
        let past_right =
            egui::Rect::from_min_size(egui::pos2(1200.0, 50.0), egui::vec2(100.0, 80.0));
        let revealed = workspace_scroll_to_reveal(scroll, viewport_size, past_right);
        assert_eq!(revealed.x, past_right.max.x - viewport_size.x);
        assert_eq!(revealed.y, 0.0, "the vertical axis was already visible");
        let target_screen = egui::Rect::from_min_size(
            past_right.min.to_vec2().to_pos2() - revealed,
            past_right.size(),
        );
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size).contains_rect(target_screen),
            "after scrolling, the window must fit in the visible area: {target_screen:?}"
        );

        // Out of view to the left/top (scroll already advanced).
        let scroll = egui::vec2(500.0, 400.0);
        let before_left =
            egui::Rect::from_min_size(egui::pos2(50.0, 20.0), egui::vec2(100.0, 80.0));
        let revealed = workspace_scroll_to_reveal(scroll, viewport_size, before_left);
        assert_eq!(revealed.x, before_left.min.x);
        assert_eq!(revealed.y, before_left.min.y);

        // Always clamped to [0, max_scroll], even for a target that overflows
        // the workspace (window larger than it, degenerate case):
        // the scroll stops exactly at the bound, without exceeding it.
        let max_scroll = workspace_max_scroll(viewport_size);
        let out_of_bounds =
            egui::Rect::from_min_size(WORKSPACE_SIZE.to_pos2(), egui::vec2(50.0, 50.0));
        let revealed = workspace_scroll_to_reveal(egui::Vec2::ZERO, viewport_size, out_of_bounds);
        assert_eq!(revealed, max_scroll);
    }

    #[test]
    fn the_persisted_geometry_keeps_the_logical_size_even_when_collapsed() {
        // The rectangle actually painted this frame (`area_rect`) is that
        // of a collapsed window — smaller than its logical size.
        let painted_rect =
            egui::Rect::from_min_size(egui::pos2(50.0, 60.0), egui::vec2(400.0, 40.0));
        let current_size = egui::vec2(400.0, 300.0);

        let persisted = floating_window_geometry_to_persist(painted_rect, current_size);

        assert_eq!(
            persisted,
            egui::Rect::from_min_size(painted_rect.min, current_size),
            "the saved geometry must keep the expanded size, otherwise a reloaded project \
             would reopen the window tiny even after clicking \"expand\""
        );
    }

    // Bug 1 (`retours-2026-09-24-bugs.md`), the cause: the
    // position was clamped twice, by egui (`constrain_to`) and by
    // us, so that the outline followed the requested position and the frame
    // the readjusted position. There is now only one clamp, this one.
    #[test]
    fn a_position_outside_the_workspace_is_clamped_by_a_single_function() {
        let size = egui::vec2(500.0, 400.0);
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(egui::pos2(-300.0, 80.0), size)),
            egui::Rect::from_min_size(egui::pos2(0.0, 80.0), size),
        );
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(egui::pos2(1800.0, 1000.0), size)).max,
            WORKSPACE_SIZE.to_pos2(),
        );
        assert_eq!(
            clamp_to_workspace(egui::Rect::from_min_size(
                egui::pos2(10.0, 10.0),
                egui::vec2(5000.0, 5000.0),
            )),
            egui::Rect::from_min_size(egui::Pos2::ZERO, WORKSPACE_SIZE),
        );
        // The witness: a window already inside the workspace does not move
        // by a single point, even stuck to its opposite corner.
        for legal in [
            egui::Rect::from_min_size(egui::pos2(120.0, 80.0), size),
            egui::Rect::from_min_size((WORKSPACE_SIZE - size).to_pos2(), size),
        ] {
            assert_eq!(clamp_to_workspace(legal), legal);
        }
    }

    fn graph() -> NodeId {
        NodeId::from_raw(7)
    }

    #[test]
    fn the_first_record_of_a_graph_never_resizes() {
        let mut follows = HashMap::new();
        assert_eq!(
            FigureFollow::observe(&mut follows, graph(), (960, 360), Some(1.5)),
            None
        );
        assert_eq!(
            FigureFollow::observe(&mut follows, graph(), (960, 360), Some(1.5)),
            None,
            "same size"
        );
    }

    /// The difference is taken at the scale from before the change: a figure
    /// that grows by 480 points at scale 1.5 asks for 720 screen points.
    #[test]
    fn a_change_of_size_moves_the_window_by_the_difference_at_the_display_scale() {
        let mut follows = HashMap::new();
        FigureFollow::observe(&mut follows, graph(), (480, 360), Some(1.5));
        // The frame the figure changes is painted contained in the old window:
        // its fresh scale (1.0 here) must not be used.
        let grow = FigureFollow::observe(&mut follows, graph(), (960, 360), Some(0.75));
        assert_eq!(grow, Some(egui::vec2(720.0, 0.0)));
        // Undo: the figure shrinks back, the window with it.
        let shrink = FigureFollow::observe(&mut follows, graph(), (480, 360), Some(1.5));
        assert_eq!(shrink, Some(egui::vec2(-720.0, 0.0)));
    }

    #[test]
    fn a_floating_window_grows_right_and_down_but_stays_in_the_workspace() {
        let size = egui::vec2(560.0, 460.0);
        let position = egui::pos2(100.0, 50.0);
        assert_eq!(
            followed_floating_size(position, size, egui::vec2(720.0, 0.0), WORKSPACE_SIZE),
            egui::vec2(1280.0, 460.0)
        );
        // Not past the right edge of the workspace: 1920 - 100.
        assert_eq!(
            followed_floating_size(position, size, egui::vec2(5000.0, 5000.0), WORKSPACE_SIZE),
            WORKSPACE_SIZE - position.to_vec2()
        );
        // Not past the visible part either: the viewport shows 1000 x 600 from
        // the scroll offset (50, 0).
        assert_eq!(
            followed_floating_size(
                position,
                size,
                egui::vec2(5000.0, 5000.0),
                egui::vec2(1050.0, 600.0)
            ),
            egui::vec2(950.0, 550.0)
        );
        // Never below the minimum.
        assert_eq!(
            followed_floating_size(position, size, egui::vec2(-5000.0, -5000.0), WORKSPACE_SIZE),
            MIN_CONTENT_WINDOW_SIZE
        );
    }

    #[test]
    fn a_detached_window_is_clamped_to_its_monitor() {
        let size = egui::vec2(560.0, 460.0);
        assert_eq!(
            followed_viewport_size(
                size,
                egui::vec2(700.0, 0.0),
                Some(egui::vec2(1280.0, 800.0))
            ),
            egui::vec2(1260.0, 460.0)
        );
        assert_eq!(
            followed_viewport_size(
                size,
                egui::vec2(2000.0, 0.0),
                Some(egui::vec2(1280.0, 800.0))
            ),
            egui::vec2(1280.0, 460.0)
        );
        assert_eq!(
            followed_viewport_size(size, egui::vec2(2000.0, 0.0), None),
            egui::vec2(2560.0, 460.0)
        );
    }
}
