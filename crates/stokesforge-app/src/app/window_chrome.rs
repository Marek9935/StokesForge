//! Common frame of content windows: title bar (`window_header`),
//! frame and outline (`floating_window_frame`), resize handles,
//! and the placing of a window in an egui layer (`show_window_in_layer`).
//!
//! Moved as is from `app.rs` (#100, step 1; the rest — placement, stack,
//! detaching, hiding, maximizing — is #101), with no change of
//! behavior: it is the building block shared by the floating windows, the
//! detached windows and the modal dialogs. Submodule of `app` to keep
//! access to its private types (`Tab`, icon constants…).
//! Collapsed panels keep only Expand: Detach requires opening them.
//! The console groups its handles on the left (#142); its collapsed strip draws
//! its separator on the workspace side, like the managers' strips.

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole, ICON_SIZE};
use stokesforge_app_state::tr;

use super::tab::Tab;

/// Height of the header band painted by [`window_header`] — shared
/// with the computation of a floating window's collapsed size (B3,
/// `retours-2026-09-23-bugs.md`): an outline or a forced size
/// that does not use this same value becomes too big again as soon as
/// the content is hidden.
pub(super) const WINDOW_HEADER_HEIGHT: f32 = 28.0;

/// Inner margin of the `Frame` of a content window, floating or
/// detached — see [`floating_window_frame`] and the `Frame` built in
/// [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs). No vertical margin: the header
/// band (`WINDOW_HEADER_HEIGHT`) and the content's separators
/// already provide their own spacing, so egui's default margin
/// (`Frame::window`/`Frame::central_panel`) only added a dead white
/// band above and below, with no role. The horizontal margin stays
/// so as not to stick the text to the edge.
pub(super) const WINDOW_CONTENT_MARGIN: egui::Margin = egui::Margin {
    left: 6,
    right: 6,
    top: 0,
    bottom: 0,
};

/// Stroke of the highlight outline — the theme's selection color for
/// the active window, egui's default stroke otherwise. Shared between
/// [`paint_active_window_outline`] (internal floating windows and detached
/// windows, B1/B3, B7): same highlight logic on both sides, only
/// the notion of "active" differs (`is_active` computed from `active_tab`
/// for the former, from the OS focus of the viewport for the latter).
pub(super) fn active_window_stroke(style: &egui::Style, is_active: bool) -> egui::Stroke {
    if is_active {
        style.visuals.selection.stroke
    } else {
        style.visuals.window_stroke()
    }
}

/// `Frame` of a floating window — the usual `Frame::window`, without
/// recoloring according to activity. [`show_floating_window`] paints it on the
/// window's rectangle; the active state is painted over it, on that same
/// rectangle, by [`paint_active_window_outline`].
pub(super) fn floating_window_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style).inner_margin(WINDOW_CONTENT_MARGIN)
}

/// Paints the highlight outline of an active window on `rect`, the
/// window's very rectangle. Callers pass a `Painter` of the
/// right layer: the window's own on the MDI side,
/// the viewport's background layer on the detached OS window side.
pub(super) fn paint_active_window_outline(
    painter: &egui::Painter,
    rect: egui::Rect,
    corner_radius: egui::CornerRadius,
    style: &egui::Style,
) {
    painter.rect_stroke(
        rect,
        corner_radius,
        active_window_stroke(style, true),
        egui::StrokeKind::Inside,
    );
}

#[derive(Default)]
pub(super) struct WindowHeaderActions {
    pub(super) maximize: bool,
    /// Detach (placement [`HeaderPlacement::Floating`]) or reattach
    /// (placement [`HeaderPlacement::Detached`]) — the glyph and the tooltip
    /// change with `placement`, but it is always the caller that
    /// interprets this boolean according to the context it paints the header from.
    pub(super) detach: bool,
    pub(super) close: bool,
    pub(super) toggle_collapse: bool,
    /// Drag delta of the title during the frame — used as is by the
    /// MDI caller (`window_positions`). No practical effect for the
    /// detached caller, which prefers `title_drag_started`.
    pub(super) drag_delta: egui::Vec2,
    /// True on the frame where the title drag starts — it is the one the
    /// detached caller uses to trigger
    /// `ViewportCommand::StartDrag` only once, the OS window then taking
    /// over the gesture (`drag_delta` would no longer be reliable once
    /// the OS is in control of the pointer).
    pub(super) title_drag_started: bool,
}

/// Where [`window_header`] is painted from — drives the icon color, the
/// "detach"/"reattach" glyph and which buttons appear. Two
/// contexts, one function: it is the bar itself that must be
/// identical, not just its general look (see the top doc of the
/// module, § generic detaching).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HeaderPlacement {
    /// Floating window in the `CentralPanel` (MDI).
    Floating,
    /// Separate OS window (`StokesForgeApp::show_detached_tabs`), without
    /// native decoration — this bar is all the user has to
    /// move it, resize it through its maximize icons, or
    /// close/reattach it.
    Detached,
    /// Modal dialog ([`modal_dialog`](crate::app::dialogs::modal_dialog)): no collapse, no detaching, no
    /// maximizing — the title, the cross, and the drag.
    Dialog,
}

/// A header icon painted to the pixel, like the title and the collapse
/// chevron — never a `Button` (see the top doc of [`window_header`] for
/// the precise reason). `rect` is already clamped to the bar's height; the
/// rounded background on hover imitates a button's affordance without
/// reintroducing its native padding.
pub(super) fn header_icon(
    header: &mut egui::Ui,
    base_id: egui::Id,
    id_source: &str,
    rect: egui::Rect,
    icon: Icon,
    tooltip: &str,
) -> bool {
    let response = header.interact(rect, base_id.with(id_source), egui::Sense::click());
    if response.hovered() {
        header
            .painter()
            .rect_filled(rect, 4.0, header.visuals().widgets.hovered.bg_fill);
    }
    icon_image(icon, IconRole::Neutral).paint_at(
        header,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(ICON_SIZE)),
    );
    response.on_hover_text(tooltip).clicked()
}

/// Minimum size of a content window, floating or detached — enough
/// to keep the header (icons + title) and a fragment of content readable,
/// not a scientific constraint.
pub(super) const MIN_CONTENT_WINDOW_SIZE: egui::Vec2 = egui::vec2(220.0, 140.0);

/// Layer of floating windows: the one egui reserves for windows
/// that are reordered on click (`Order::Middle`), above the fixed panels
/// (`Order::Background`) and **below** menus, drop-down lists and
/// tooltips (`Order::Foreground`) — a context menu opened in a
/// window must come in front of its neighbors.
pub(super) const FLOATING_WINDOW_ORDER: egui::Order = egui::Order::Middle;

/// Thickness of the resize handles, placed **outside** the frame
/// (see [`floating_resize_handles`]) and counted in the window's clickable
/// area by [`show_floating_window`].
pub(super) const RESIZE_BORDER: f32 = 6.0;

/// Length, along each side, of the two branches of a corner handle
/// (see [`floating_resize_handles`]): the corner can be targeted over ~18 points
/// instead of 6.
const CORNER_REACH: f32 = 12.0;

/// Paints a floating window of the workspace, frame included, in
/// its own layer. `window_rect` is the window's screen rectangle,
/// `viewport_rect` the visible portion of the workspace (what the
/// fixed panels leave). Returns `None` when the window is entirely
/// out of view: nothing is then painted or made clickable.
///
/// # Why an `egui::Area` and not an `egui::Window`
///
/// `egui::Window` is a dialog box: title bar, drag,
/// resize, collapse and clamping of its own. All these functions had
/// been cut out one by one then redone by hand ([`window_header`],
/// [`floating_resize_handles`], [`workspace_to_screen`](crate::app::window_placement::workspace_to_screen)…), and each frame
/// had to reconcile two rectangles: the one requested and the one
/// egui readjusted. Bugs 1 and 2 of
/// `retours-2026-09-24-bugs.md` (outline offset from a
/// readjusted window, window painted over the fixed panels) were
/// two forms of this disagreement. `egui::Area` is the bare building block under
/// `Window`: a layer, a position, an order on click — nothing else. The
/// frame, the outline and the clipping are set here, on the **same**
/// rectangle.
///
/// # The clickable area is the visible part
///
/// egui picks the window under the pointer from the rectangle of its
/// `Area` (`Memory::layer_id_at`), not from what is painted. The `Area`
/// is thus cut to the visible part of the window only (handles
/// included): a window straddling the edge of the view captures no
/// click meant for the Project Manager or the console, and its drawing is clipped
/// at the same edge (`constrain_to(viewport_rect)`, which also gives `Area` its
/// clip rectangle).
pub(super) fn show_floating_window<R>(
    ctx: &egui::Context,
    id: egui::Id,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    show_window_in_layer(
        ctx,
        id,
        FLOATING_WINDOW_ORDER,
        window_rect,
        viewport_rect,
        is_active,
        add_contents,
        add_resize_handles,
    )
}

/// [`show_floating_window`] in the layer order `order`: the only
/// writing of a window's frame, outline and clipping, shared by
/// the workspace windows (`Order::Middle`) and the modal
/// dialogs ([`modal_dialog`](crate::app::dialogs::modal_dialog), above).
#[allow(clippy::too_many_arguments)]
pub(super) fn show_window_in_layer<R>(
    ctx: &egui::Context,
    id: egui::Id,
    order: egui::Order,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    let area_rect = window_rect.expand(RESIZE_BORDER).intersect(viewport_rect);
    if !window_rect.intersects(viewport_rect) || !area_rect.is_positive() {
        return None;
    }
    let response = egui::Area::new(id)
        .order(order)
        .movable(false)
        .fade_in(false)
        .fixed_pos(area_rect.min)
        .constrain_to(viewport_rect)
        .show(ctx, |ui| {
            // The `Area`'s size is that of its content: we fix it to
            // the visible part, the real content living in a child `Ui` that
            // does not make it grow.
            ui.set_min_size(area_rect.size());
            let style = ui.style().clone();
            let frame = floating_window_frame(&style);
            let content_rect = window_rect - frame.total_margin();
            ui.painter().add(frame.paint(content_rect));
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            content.set_clip_rect(content_rect.intersect(ui.clip_rect()));
            let inner = add_contents(&mut content);
            // The drop target's border replaces the active outline (#386).
            if super::file_drop::is_drop_target(ui.ctx(), ui.layer_id()) {
                ui.painter().rect_stroke(
                    window_rect,
                    style.visuals.window_corner_radius,
                    crate::theme::drop_target_stroke(),
                    egui::StrokeKind::Inside,
                );
            } else if is_active {
                paint_active_window_outline(
                    ui.painter(),
                    window_rect,
                    style.visuals.window_corner_radius,
                    &style,
                );
            }
            add_resize_handles(ui, window_rect);
            inner
        });
    Some(response.inner)
}

/// Eight thin resize zones for an MDI floating window,
/// placed entirely **outside** `rect` (over [`RESIZE_BORDER`])
/// so as not to contest the hover with the content — the grid's scroll bars
/// run along the edge. They are painted in the window's
/// layer ([`show_floating_window`] counts this edge in its clickable
/// area): a window's handle thus never passes in front of another
/// window placed on top of it.
pub(super) fn floating_resize_handles(
    ui: &egui::Ui,
    tab: impl std::hash::Hash + std::fmt::Debug,
    rect: egui::Rect,
    position: &mut egui::Pos2,
    size: &mut egui::Vec2,
) {
    const BORDER: f32 = RESIZE_BORDER;
    // (source, zone, cursor, west, east, north, south) — the last four
    // say which sides this drag affects, combined for the corners.
    let mut zones: Vec<(String, egui::Rect, egui::CursorIcon, bool, bool, bool, bool)> = vec![
        (
            "n".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() - BORDER),
                egui::pos2(rect.right(), rect.top()),
            ),
            super::cursors::RESIZE_NORTH,
            false,
            false,
            true,
            false,
        ),
        (
            "s".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom() + BORDER),
            ),
            super::cursors::RESIZE_SOUTH,
            false,
            false,
            false,
            true,
        ),
        (
            "w".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left() - BORDER, rect.top()),
                egui::pos2(rect.left(), rect.bottom()),
            ),
            super::cursors::RESIZE_WEST,
            true,
            false,
            false,
            false,
        ),
        (
            "e".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.right(), rect.top()),
                egui::pos2(rect.right() + BORDER, rect.bottom()),
            ),
            super::cursors::RESIZE_EAST,
            false,
            true,
            false,
            false,
        ),
    ];
    // Corners: a 6 × 6 square outside the frame is too small to target (#75).
    // Each corner is thus an "L" laid along the two neighboring sides,
    // `CORNER_REACH` long, always outside the frame (the content's scroll
    // bars are not contested). Added after the sides,
    // they win where they overlap.
    for (name, x, y, cursor, west, north) in [
        (
            "nw",
            rect.left(),
            rect.top(),
            super::cursors::RESIZE_NW_SE,
            true,
            true,
        ),
        (
            "ne",
            rect.right(),
            rect.top(),
            super::cursors::RESIZE_NE_SW,
            false,
            true,
        ),
        (
            "sw",
            rect.left(),
            rect.bottom(),
            super::cursors::RESIZE_NE_SW,
            true,
            false,
        ),
        (
            "se",
            rect.right(),
            rect.bottom(),
            super::cursors::RESIZE_NW_SE,
            false,
            false,
        ),
    ] {
        // Corner direction: -1 outward, +1 inward, per axis.
        let sx = if west { 1.0 } else { -1.0 };
        let sy = if north { 1.0 } else { -1.0 };
        let horizontal = egui::Rect::from_two_pos(
            egui::pos2(x - sx * BORDER, y - sy * BORDER),
            egui::pos2(x + sx * CORNER_REACH, y),
        );
        let vertical = egui::Rect::from_two_pos(
            egui::pos2(x - sx * BORDER, y - sy * BORDER),
            egui::pos2(x, y + sy * CORNER_REACH),
        );
        for (suffix, zone) in [("h", horizontal), ("v", vertical)] {
            zones.push((
                format!("{name}-{suffix}"),
                zone,
                cursor,
                west,
                !west,
                north,
                !north,
            ));
        }
    }
    let base_id = egui::Id::new(("floating_resize_handle", tab));
    for (source, zone, cursor, west, east, north, south) in zones {
        let response = ui.interact(zone, base_id.with(&source), egui::Sense::drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ResizeHandle,
                ui.is_enabled(),
                resize_handle_label(&source),
            )
        });
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        let delta = response.drag_delta();
        if delta == egui::Vec2::ZERO {
            continue;
        }
        if west {
            let new_width = (size.x - delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
            position.x += size.x - new_width;
            size.x = new_width;
        }
        if east {
            size.x = (size.x + delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
        }
        if north {
            let new_height = (size.y - delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
            position.y += size.y - new_height;
            size.y = new_height;
        }
        if south {
            size.y = (size.y + delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
        }
    }
}

/// The two branches of a corner's L share its accessible direction;
/// the identifiers and the rectangles stay specific to each target.
fn resize_handle_label(source: &str) -> String {
    match source.split('-').next().unwrap_or(source) {
        "n" => tr!("window-resize-top"),
        "s" => tr!("window-resize-bottom"),
        "w" => tr!("window-resize-left"),
        "e" => tr!("window-resize-right"),
        "nw" => tr!("window-resize-top-left"),
        "ne" => tr!("window-resize-top-right"),
        "sw" => tr!("window-resize-bottom-left"),
        "se" => tr!("window-resize-bottom-right"),
        _ => unreachable!("unknown handle direction: {source}"),
    }
}

/// Invisible resize border for a detached window without
/// decoration (`.with_decorations(false)`, see the top doc of
/// [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs)): eight thin zones at the edges/corners of the
/// viewport, which delegate resizing to the OS as soon as the drag
/// starts (`ViewportCommand::BeginResize`) — same principle as
/// `ViewportCommand::StartDrag` for moving the title: no delta
/// computation on our side, the OS/window manager takes over.
pub(super) fn detached_resize_handles(ui: &egui::Ui) {
    const BORDER: f32 = 6.0;
    let rect = ui.ctx().input(|input| input.content_rect());
    let zones: [(&str, egui::Rect, egui::CursorIcon, egui::ResizeDirection); 8] = [
        (
            "n",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.right(), rect.top() + BORDER),
            ),
            super::cursors::RESIZE_NORTH,
            egui::ResizeDirection::North,
        ),
        (
            "s",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                rect.right_bottom(),
            ),
            super::cursors::RESIZE_SOUTH,
            egui::ResizeDirection::South,
        ),
        (
            "w",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + BORDER, rect.bottom()),
            ),
            super::cursors::RESIZE_WEST,
            egui::ResizeDirection::West,
        ),
        (
            "e",
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - BORDER, rect.top()),
                rect.right_bottom(),
            ),
            super::cursors::RESIZE_EAST,
            egui::ResizeDirection::East,
        ),
        (
            "nw",
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(BORDER, BORDER)),
            super::cursors::RESIZE_NW_SE,
            egui::ResizeDirection::NorthWest,
        ),
        (
            "ne",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.top()),
                egui::vec2(BORDER, BORDER),
            ),
            super::cursors::RESIZE_NE_SW,
            egui::ResizeDirection::NorthEast,
        ),
        (
            "sw",
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            super::cursors::RESIZE_NE_SW,
            egui::ResizeDirection::SouthWest,
        ),
        (
            "se",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            super::cursors::RESIZE_NW_SE,
            egui::ResizeDirection::SouthEast,
        ),
    ];
    let base_id = egui::Id::new("detached_resize_handle");
    for (source, zone, cursor, direction) in zones {
        let response = ui.interact(zone, base_id.with(source), egui::Sense::drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ResizeHandle,
                ui.is_enabled(),
                resize_handle_label(source),
            )
        });
        if response.hovered() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// Width of an action icon of the title bar (cross, detach,
/// maximize).
pub(super) const HEADER_ACTION_WIDTH: f32 = 24.0;

/// Panels keep their place; only their OS window moves.
/// A collapsed side strip keeps its vertical title and the Expand handle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelPlacement {
    Left,
    Right,
    Bottom,
    Detached,
}

/// Collapsed thickness, common to the side strips and the console.
pub(super) const PANEL_STRIP_THICKNESS: f32 = ICON_SIZE;

/// Same drawing and same icon cells as the content windows, with
/// compact typography. No path paints Close or Maximize (#100).
pub(super) fn panel_header(
    ui: &mut egui::Ui,
    title: &str,
    placement: PanelPlacement,
    collapsed: bool,
) -> WindowHeaderActions {
    let vertical = collapsed && matches!(placement, PanelPlacement::Left | PanelPlacement::Right);
    let action_width = if collapsed {
        ICON_SIZE
    } else {
        HEADER_ACTION_WIDTH
    };
    let header_height = if collapsed {
        ICON_SIZE
    } else {
        WINDOW_HEADER_HEIGHT
    };
    let size = egui::vec2(
        ui.available_width(),
        if vertical {
            ui.available_height()
        } else {
            header_height
        },
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let id = ui.id().with("panel_header");
    let mut header = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let cell = |pos| egui::Rect::from_min_size(pos, egui::vec2(action_width, header_height));
    let first_pos = if vertical {
        egui::pos2(rect.center().x - action_width / 2.0, rect.top())
    } else {
        rect.min
    };
    let mut toggle_rect = cell(first_pos);
    let mut detach_rect = if vertical {
        cell(first_pos + egui::vec2(0.0, header_height))
    } else if placement == PanelPlacement::Bottom {
        cell(first_pos + egui::vec2(action_width, 0.0))
    } else {
        cell(egui::pos2(rect.right() - action_width, rect.top()))
    };
    // Collapsing is targeted at the edge of the workspace, detaching at the
    // outer edge for the managers. The console keeps both
    // handles on the left, Detach only when it is expanded (#142).
    // A collapsed strip keeps only Expand, with no detach target.
    if placement == PanelPlacement::Left && !vertical {
        std::mem::swap(&mut toggle_rect, &mut detach_rect);
    }
    let mut actions = WindowHeaderActions::default();
    if placement != PanelPlacement::Detached {
        let icon = match (placement, collapsed) {
            (PanelPlacement::Left, true) | (PanelPlacement::Right, false) => Icon::CaretRight,
            (PanelPlacement::Left, false) | (PanelPlacement::Right, true) => Icon::CaretLeft,
            (_, true) => Icon::CaretUp,
            (_, false) => Icon::CaretDown,
        };
        actions.toggle_collapse = header_icon(
            &mut header,
            id,
            "collapse",
            toggle_rect,
            icon,
            &if collapsed {
                tr!("panel-expand")
            } else {
                tr!("panel-collapse")
            },
        );
    }
    let shows_detach = !collapsed || placement == PanelPlacement::Detached;
    if shows_detach {
        actions.detach = header_icon(
            &mut header,
            id,
            "detach",
            detach_rect,
            if placement == PanelPlacement::Detached {
                Icon::Reattach
            } else {
                Icon::Detach
            },
            &if placement == PanelPlacement::Detached {
                tr!("panel-reattach")
            } else {
                tr!("panel-detach")
            },
        );
    }
    let title_rect = if vertical {
        egui::Rect::from_min_max(rect.min + egui::vec2(0.0, header_height), rect.max)
    } else {
        egui::Rect::from_min_max(
            rect.min
                + egui::vec2(
                    match placement {
                        PanelPlacement::Detached => 0.0,
                        PanelPlacement::Bottom => {
                            (if shows_detach { 2.0 } else { 1.0 }) * action_width
                        }
                        _ => action_width,
                    },
                    0.0,
                ),
            egui::pos2(
                rect.right()
                    - if placement == PanelPlacement::Bottom {
                        0.0
                    } else {
                        action_width
                    },
                rect.bottom(),
            ),
        )
    };
    let painter = header.painter().with_clip_rect(title_rect);
    let font = egui::TextStyle::Body.resolve(header.style());
    if vertical {
        let galley = painter.layout_no_wrap(title.to_owned(), font, header.visuals().text_color());
        let pos = egui::pos2(
            title_rect.center().x - galley.size().y / 2.0,
            title_rect.center().y + galley.size().x / 2.0,
        );
        let mut shape = egui::epaint::TextShape::new(pos, galley, header.visuals().text_color());
        shape.angle = -std::f32::consts::FRAC_PI_2;
        painter.add(shape);
    } else {
        painter.text(
            title_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            font,
            header.visuals().text_color(),
        );
    }
    let title_response = header
        .interact(title_rect, id.with("title"), egui::Sense::drag())
        .on_hover_text(title);
    if placement == PanelPlacement::Detached {
        actions.title_drag_started = title_response.drag_started();
    }
    let stroke = header.visuals().widgets.noninteractive.bg_stroke;
    if vertical {
        // The separation stays in the strip: a Panel would reserve an outer
        // margin that would shift the icons' center by half a point.
        let x = if placement == PanelPlacement::Left {
            rect.right() - stroke.width / 2.0
        } else {
            rect.left() + stroke.width / 2.0
        };
        header.painter().vline(x, rect.y_range(), stroke);
    } else {
        header
            .painter()
            .hline(rect.x_range(), rect.bottom(), stroke);
        if placement == PanelPlacement::Bottom && collapsed {
            // Keep the whole top stroke inside the strip's clip, like the managers'
            // vertical separations.
            header
                .painter()
                .hline(rect.x_range(), rect.top() + stroke.width / 2.0, stroke);
        }
    }
    actions
}

/// Are detach and maximize painted? A single source for the drawing
/// and for the width reserved for the title (#50).
pub(super) fn shows_detach_and_maximize(placement: HeaderPlacement, tab: Option<Tab>) -> bool {
    match placement {
        HeaderPlacement::Floating => matches!(tab, Some(Tab::Book(_) | Tab::Graph(_))),
        HeaderPlacement::Detached => true,
        HeaderPlacement::Dialog => false,
    }
}

/// Width that the really painted icons take off the title: the
/// cross alone, or the cross plus detach and maximize (26 px step, 2 px
/// margin included).
pub(super) fn header_actions_width(with_detach_and_maximize: bool) -> f32 {
    let icons = if with_detach_and_maximize { 3.0 } else { 1.0 };
    2.0 + icons * (HEADER_ACTION_WIDTH + 2.0)
}

/// Title bar of the content windows — the same code base for the MDI
/// floating windows and the detached OS windows (`placement`, see
/// [`HeaderPlacement`]). The actions and the cross are painted the same
/// way as the title (see [`header_icon`]) and form a single visual
/// group; this also avoids a superimposed `Area`, which shifts when a
/// window goes behind another.
pub(super) fn window_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    title: &str,
    tab: Option<Tab>,
    collapsed: bool,
    placement: HeaderPlacement,
) -> WindowHeaderActions {
    const COLLAPSE_WIDTH: f32 = 24.0;

    let mut actions = WindowHeaderActions::default();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), WINDOW_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let mut header = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    // B5 (`retours-2026-09-23-bugs.md`): the collapse icon
    // was reserved to `Tab::Graph`, with no documented justification — an
    // extension never made rather than a choice. `Matrix`/`Note` stay
    // excluded for lack of a reason to add them.
    let collapse_width = if matches!(tab, Some(Tab::Graph(_) | Tab::Book(_))) {
        COLLAPSE_WIDTH
    } else {
        0.0
    };
    if collapse_width > 0.0 {
        let collapse_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(collapse_width, rect.height()));
        let is_book = matches!(tab, Some(Tab::Book(_)));
        let (icon, tooltip) = match (collapsed, is_book) {
            (true, true) => (Icon::CaretRight, tr!("window-header-show-workbook")),
            (true, false) => (Icon::CaretRight, tr!("window-header-show-graph")),
            (false, true) => (Icon::CaretDown, tr!("window-header-hide-workbook")),
            (false, false) => (Icon::CaretDown, tr!("window-header-hide-graph")),
        };
        if header_icon(&mut header, id, "collapse", collapse_rect, icon, &tooltip) {
            actions.toggle_collapse = true;
        }
    }
    let action_rect = |offset: f32| {
        egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - offset - HEADER_ACTION_WIDTH,
                rect.top() + 2.0,
            ),
            egui::vec2(HEADER_ACTION_WIDTH, rect.height() - 4.0),
        )
    };
    if header_icon(
        &mut header,
        id,
        "close",
        action_rect(2.0),
        Icon::Close,
        &tr!("window-header-close"),
    ) {
        actions.close = true;
    }
    // In MDI, detach/maximize stay reserved to Book/Graph (Matrix/Note
    // have no such UI path today, even though `show_detached_tabs` already
    // handles them generically). In a detached window, the four kinds
    // keep reattach/maximize in all circumstances — it is what
    // replaces the OS's native title bar removed with it
    // (`.with_decorations(false)`), so no question of losing a
    // capability for some tabs.
    let show_detach_and_maximize = shows_detach_and_maximize(placement, tab);
    if show_detach_and_maximize {
        let (detach_icon, detach_tooltip) = match placement {
            HeaderPlacement::Floating | HeaderPlacement::Dialog => {
                (Icon::Detach, tr!("window-header-detach"))
            }
            HeaderPlacement::Detached => (Icon::Reattach, tr!("window-header-reattach")),
        };
        if header_icon(
            &mut header,
            id,
            "detach",
            action_rect(28.0),
            detach_icon,
            &detach_tooltip,
        ) {
            actions.detach = true;
        }
        if header_icon(
            &mut header,
            id,
            "maximize",
            action_rect(54.0),
            Icon::Maximize,
            &tr!("window-header-maximize"),
        ) {
            actions.maximize = true;
        }
    }
    // The drag zone goes from the collapse arrow to the really
    // painted icons (#50); the title stays centered in the header and is clipped on
    // each side only by what is really there: the arrow on the left, the
    // icons on the right (nothing clips an empty side).
    let actions_width = header_actions_width(show_detach_and_maximize);
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + collapse_width, rect.top()),
        egui::pos2(rect.right() - actions_width, rect.bottom()),
    );
    let clip_rect = title_rect;
    header.painter().with_clip_rect(clip_rect).text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::TextStyle::Heading.resolve(header.style()),
        header.visuals().text_color(),
    );
    let drag_rect = title_rect;
    let drag = ui.interact(
        drag_rect,
        id.with("content_title_drag"),
        egui::Sense::drag(),
    );
    if drag.drag_started() {
        actions.title_drag_started = true;
    }
    if drag.dragged() {
        actions.drag_delta = ui.input(|input| input.pointer.delta());
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    actions
}

/// Places a window's content in the remaining space, clipped to that space:
/// content that is too big neither overflows nor pushes the frame.
pub(super) fn show_window_contents_bounded(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let rect = ui.available_rect_before_wrap();
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    content.set_clip_rect(ui.clip_rect().intersect(rect));
    add_contents(&mut content);
    ui.advance_cursor_after_rect(rect);
}

/// `pub(super)`: [`tests::find_icon_centers`] also serves the tests of `window_detach.rs`
/// that paint the header of a detached window.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use stokesforge_core::NodeId;

    /// The two targets of a floating corner announce the same direction,
    /// while each edge/corner stays distinct in the AccessKit tree.
    #[test]
    fn window_handles_announce_the_eight_directions() {
        for detached in [false, true] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let mut position = egui::pos2(80.0, 80.0);
            let mut size = egui::vec2(200.0, 180.0);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 500.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    if detached {
                        detached_resize_handles(ui);
                    } else {
                        floating_resize_handles(
                            ui,
                            Tab::Book(NodeId::from_raw(2)),
                            egui::Rect::from_min_size(position, size),
                            &mut position,
                            &mut size,
                        );
                    }
                },
            );
            output.textures_delta.clear();
            let mut labels = std::collections::BTreeMap::new();
            for (_, node) in output
                .platform_output
                .accesskit_update
                .expect("accessibility tree")
                .nodes
            {
                if node.role() != egui::accesskit::Role::Splitter {
                    continue;
                }
                assert!(!node.is_disabled());
                let bounds = node.bounds().expect("handle rectangle");
                assert!(bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0);
                *labels.entry(node.label().unwrap().to_owned()).or_insert(0) += 1;
            }
            let expected = [
                (tr!("window-resize-top"), 1),
                (tr!("window-resize-bottom"), 1),
                (tr!("window-resize-left"), 1),
                (tr!("window-resize-right"), 1),
                (tr!("window-resize-top-left"), if detached { 1 } else { 2 }),
                (tr!("window-resize-top-right"), if detached { 1 } else { 2 }),
                (
                    tr!("window-resize-bottom-left"),
                    if detached { 1 } else { 2 },
                ),
                (
                    tr!("window-resize-bottom-right"),
                    if detached { 1 } else { 2 },
                ),
            ]
            .into_iter()
            .collect();
            assert_eq!(labels, expected, "detached window: {detached}");
        }
    }

    /// Centers of the painted icons (images: rects with a texture), from left to
    /// right. The image loaders must be installed on the context,
    /// and the image must have already run once for the texture to be ready.
    pub(in crate::app) fn find_icon_centers(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Vec<egui::Pos2> {
        let mut centers: Vec<egui::Pos2> = shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect.center()),
                _ => None,
            })
            .collect();
        centers.sort_by(|a, b| a.x.total_cmp(&b.x));
        centers
    }

    /// Width of the header tests' screen: the header takes all of it.
    const HEADER_WIDTH: f32 = 400.0;

    /// Paints [`window_header`] alone, over the whole screen width, over
    /// `events.len() + 1` frames (the first with no event: the icons' SVG
    /// texture is ready only on the second). Returns the output of the last
    /// frame and the actions of each frame.
    fn paint_header(
        context: &egui::Context,
        title: &str,
        tab: Option<Tab>,
        placement: HeaderPlacement,
        events: Vec<Vec<egui::Event>>,
    ) -> (egui::FullOutput, Vec<WindowHeaderActions>) {
        egui_extras::install_image_loaders(context);
        let mut output = egui::FullOutput::default();
        let mut actions = Vec::new();
        for events in std::iter::once(Vec::new()).chain(events) {
            output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(HEADER_WIDTH, 120.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    actions.push(window_header(
                        ui,
                        egui::Id::new("test_header"),
                        title,
                        tab,
                        false,
                        placement,
                    ));
                },
            );
            output.textures_delta.clear();
        }
        (output, actions)
    }

    /// The icons of a header at rest, from left to right.
    fn header_icons(tab: Option<Tab>, placement: HeaderPlacement) -> Vec<egui::Pos2> {
        let context = egui::Context::default();
        let (output, _) = paint_header(&context, "Title", tab, placement, vec![Vec::new()]);
        find_icon_centers(&output.shapes)
    }

    // B7 (`retours-2026-09-23-bugs.md`): until then, the
    // highlight recolored the stroke of the window's `Frame`
    // (`floating_window_frame`) rather than painting a second outline over
    // it — but that stroke follows `content_ui.min_rect()`, not the requested
    // size, and so overflowed when the content exceeded it (tool band
    // that does not wrap). The `Frame` no longer encodes activity:
    // it always keeps egui's default stroke, whatever the state.
    #[test]
    fn the_floating_window_frame_always_keeps_the_default_stroke() {
        let style = egui::Style::default();

        let frame = floating_window_frame(&style);

        assert_eq!(
            frame.stroke.color,
            style.visuals.window_stroke().color,
            "the Frame no longer recolors according to activity (B7): the active outline is painted \
             separately by `paint_active_window_outline`, on the requested rectangle rather than the one \
             egui recomputes from the content"
        );
        assert_eq!(
            frame.inner_margin, WINDOW_CONTENT_MARGIN,
            "the frame's margin is that of the content windows, without a dead vertical band"
        );
    }

    // The recolored stroke now lives in `active_window_stroke`, used
    // by [`paint_active_window_outline`] rather than by the `Frame` — same
    // active/inactive distinction logic as before B7, moved from one
    // function to the other.
    #[test]
    fn active_window_stroke_always_distinguishes_active_from_inactive() {
        let style = egui::Style::default();

        let inactive = active_window_stroke(&style, false);
        let active = active_window_stroke(&style, true);

        assert_eq!(
            inactive.color,
            style.visuals.window_stroke().color,
            "an inactive window keeps egui's default stroke"
        );
        assert_eq!(
            active.color, style.visuals.selection.stroke.color,
            "an active window takes the theme's selection color"
        );
        assert_ne!(
            active.color, inactive.color,
            "without distinct colors, nothing would show which window is active"
        );
    }

    /// Paints floating windows through the production path
    /// ([`show_floating_window`]), over one frame, and returns egui's output.
    /// At least two frames are needed: the first is the measuring pass
    /// of a new `Area`, which paints nothing.
    fn paint_windows(
        context: &egui::Context,
        windows: &[(egui::Id, egui::Rect)],
        viewport: egui::Rect,
        active: bool,
        content: impl Fn(&mut egui::Ui),
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 1000.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            for &(id, rect) in windows {
                show_floating_window(ui.ctx(), id, rect, viewport, active, &content, |_, _| {});
            }
        });
        output.textures_delta.clear();
        output
    }

    /// Content wider than any test window — like a
    /// workbook with several columns.
    fn overflowing_content(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for index in 0..10 {
                ui.label(format!("column {index}"));
                ui.add_space(80.0);
            }
        });
    }

    /// All the painted rects, `Shape::Vec` flattened (egui's frame
    /// arrives as `Vec[shadow, rectangle]`).
    fn painted_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
        fn flatten(shape: &egui::Shape, out: &mut Vec<egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Rect(rect) => out.push(rect.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| flatten(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for painted in shapes {
            flatten(&painted.shape, &mut out);
        }
        out
    }

    // Bug 1 (`retours-2026-09-24-bugs.md`), the consequence, and B1/B3/B4/B7 of the
    // 2026-09-23 report: the frame, the active outline, the clickable area and
    // the content clipping all follow the same rectangle, whether the content
    // is short or overflows.
    #[test]
    fn the_frame_the_outline_and_the_clickable_area_follow_the_same_rectangle() {
        let style = egui::Style::default();
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 80.0), egui::vec2(220.0, 180.0));
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
        for overflows in [false, true] {
            let context = egui::Context::default();
            let id = egui::Id::new(("same_rectangle", overflows));
            let content = |ui: &mut egui::Ui| {
                if overflows {
                    overflowing_content(ui);
                } else {
                    ui.label("short content");
                }
            };
            paint_windows(&context, &[(id, window_rect)], viewport, true, content);
            let output = paint_windows(&context, &[(id, window_rect)], viewport, true, content);

            let rects = painted_rects(&output.shapes);
            let frame_rect = rects
                .iter()
                .find(|rect| rect.fill == style.visuals.window_fill())
                .expect("the frame must be painted");
            let outline = rects
                .iter()
                .find(|rect| rect.stroke.color == style.visuals.selection.stroke.color)
                .expect("the active outline must be painted");
            assert_eq!(frame_rect.rect, window_rect, "overflows={overflows}");
            assert_eq!(outline.rect, window_rect, "overflows={overflows}");
            assert_eq!(
                context.memory(|memory| memory.area_rect(id)),
                Some(window_rect.expand(RESIZE_BORDER)),
                "the clickable area is the window plus its handles, overflows={overflows}"
            );
            let mut texts = 0;
            for painted in &output.shapes {
                if let egui::Shape::Text(text) = &painted.shape {
                    texts += 1;
                    assert!(
                        window_rect.contains_rect(painted.clip_rect),
                        "{:?} overflows the window: clip {:?}",
                        text.galley.job.text,
                        painted.clip_rect,
                    );
                }
            }
            // Without painted text, the loop above would check nothing.
            assert!(
                texts > 0,
                "the content must be painted, overflows={overflows}"
            );
        }

        // The witness: the same inactive window is framed, but no
        // rectangle carries the selection color — it is indeed the active
        // outline that the search above found, not another stroke.
        let context = egui::Context::default();
        let id = egui::Id::new("same_rectangle_inactive");
        let content = |ui: &mut egui::Ui| {
            ui.label("short content");
        };
        paint_windows(&context, &[(id, window_rect)], viewport, false, content);
        let output = paint_windows(&context, &[(id, window_rect)], viewport, false, content);
        let rects = painted_rects(&output.shapes);
        assert!(rects
            .iter()
            .any(|rect| rect.fill == style.visuals.window_fill() && rect.rect == window_rect));
        assert!(
            !rects
                .iter()
                .any(|rect| rect.stroke.color == style.visuals.selection.stroke.color),
            "an inactive window has no selection outline"
        );
    }

    // Bug 2 (`retours-2026-09-24-bugs.md`): a window straddling
    // the edge of the view (scrolled workspace) painted
    // over the Project Manager, and captured the clicks there.
    #[test]
    fn a_window_straddling_the_edge_of_the_view_is_clipped_and_lets_clicks_through() {
        let context = egui::Context::default();
        let id = egui::Id::new("straddling");
        // The view starts at x = 400 (on the left: the Project Manager).
        let viewport = egui::Rect::from_min_max(egui::pos2(400.0, 0.0), egui::pos2(1400.0, 1000.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(250.0, 100.0), egui::vec2(500.0, 400.0));
        let windows = [(id, window_rect)];
        paint_windows(&context, &windows, viewport, true, overflowing_content);
        let output = paint_windows(&context, &windows, viewport, true, overflowing_content);

        assert!(!output.shapes.is_empty());
        for painted in &output.shapes {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "a shape of the window is clipped outside the view: {:?}",
                painted.clip_rect,
            );
        }
        let layer = egui::LayerId::new(FLOATING_WINDOW_ORDER, id);
        assert_ne!(
            context.layer_id_at(egui::pos2(300.0, 300.0)),
            Some(layer),
            "under the Project Manager, the hidden part of the window must capture no click"
        );
        assert_eq!(context.layer_id_at(egui::pos2(500.0, 300.0)), Some(layer));
    }

    /// A window entirely outside the visible portion of the
    /// workspace (scrolled elsewhere) is neither painted nor clickable, and its content
    /// is not even built; its visible neighbor is (witness).
    #[test]
    fn a_window_out_of_view_is_neither_painted_nor_clickable() {
        let context = egui::Context::default();
        let viewport = egui::Rect::from_min_max(egui::pos2(400.0, 0.0), egui::pos2(1400.0, 1000.0));
        let hidden = egui::Id::new("out_of_view");
        let visible = egui::Id::new("in_view");
        // To the left of the view, handles included: nothing goes beyond it.
        let hidden_rect =
            egui::Rect::from_min_size(egui::pos2(50.0, 100.0), egui::vec2(300.0, 200.0));
        let visible_rect =
            egui::Rect::from_min_size(egui::pos2(600.0, 100.0), egui::vec2(300.0, 200.0));
        let mut results = Vec::new();
        // Two frames: the first is the measuring pass of a new `Area`.
        for _ in 0..2 {
            results.clear();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    for (id, rect) in [(hidden, hidden_rect), (visible, visible_rect)] {
                        results.push(show_floating_window(
                            ui.ctx(),
                            id,
                            rect,
                            viewport,
                            true,
                            |_| id,
                            |_, _| {},
                        ));
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(results, vec![None, Some(visible)]);
        assert_eq!(
            context.memory(|memory| memory.area_rect(hidden)),
            None,
            "no clickable area is set for the window out of view"
        );
        assert_ne!(
            context.layer_id_at(hidden_rect.center()),
            Some(egui::LayerId::new(FLOATING_WINDOW_ORDER, hidden))
        );
        assert_eq!(
            context.layer_id_at(visible_rect.center()),
            Some(egui::LayerId::new(FLOATING_WINDOW_ORDER, visible))
        );
    }

    #[test]
    fn the_graph_header_keeps_its_title_centered_and_its_arrow_on_the_left() {
        let context = egui::Context::default();
        let (output, _) = paint_header(
            &context,
            "Graph1",
            Some(Tab::Graph(NodeId::from_raw(1))),
            HeaderPlacement::Floating,
            Vec::new(),
        );
        let icons = find_icon_centers(&output.shapes);

        let title = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => Some(shape),
                _ => None,
            })
            .expect("the title must be painted");
        let title_center = title.pos.x + title.galley.rect.center().x;
        assert!(
            (title_center - HEADER_WIDTH / 2.0).abs() < f32::EPSILON,
            "the title must be centered in the header, not after the arrow"
        );
        let collapse = icons.first().expect("the collapse arrow must be painted");
        assert!(
            collapse.x < 24.0,
            "the collapse arrow must stay in the top left corner"
        );

        // Regression: the action icons were placed through `Button::
        // small()`, a widget with its own frame/padding that does not
        // align pixel for pixel with the raw painted title — hence a
        // visible misalignment despite rects nominally centered on
        // the same line (see the top doc of `window_header`).
        // `header_icon` now paints everything the same way: the vertical
        // centers must coincide exactly.
        let title_center_y = title.pos.y + title.galley.rect.center().y;
        let close_center_y = icons.last().expect("the close button must be painted").y;
        assert!(
            (title_center_y - close_center_y).abs() < f32::EPSILON,
            "the title and the action icons must share the same vertical center"
        );
    }

    // B5 (`retours-2026-09-23-bugs.md`): the icon "Hide the
    // content of…" was reserved to `Tab::Graph` for no documented reason —
    // checks that it is indeed painted for `Tab::Book`, and still absent
    // for `Tab::Matrix`/`Tab::Note`, which do not have this control.
    #[test]
    fn the_collapse_icon_is_painted_for_workbooks_but_not_matrices() {
        // An icon in the left corner (the cross is on the right).
        let collapse_arrow = |tab: Tab| {
            header_icons(Some(tab), HeaderPlacement::Floating)
                .first()
                .is_some_and(|center| center.x < 24.0)
        };

        let node = NodeId::from_raw(1);
        assert!(
            collapse_arrow(Tab::Book(node)),
            "a workbook must carry the collapse icon"
        );
        assert!(
            collapse_arrow(Tab::Graph(node)),
            "a graph must keep the collapse icon"
        );
        assert!(
            !collapse_arrow(Tab::Matrix(node)),
            "a matrix does not have this control"
        );
        assert!(
            !collapse_arrow(Tab::Note(node)),
            "a note does not have this control"
        );
    }

    /// #50: a dialog paints only the cross, so it must reserved only
    /// its place; the width taken off the title follows the painted icons.
    #[test]
    fn the_reserved_width_follows_the_painted_icons() {
        let id = NodeId::from_raw(1);
        let cross = header_actions_width(shows_detach_and_maximize(HeaderPlacement::Dialog, None));
        let three = header_actions_width(shows_detach_and_maximize(
            HeaderPlacement::Floating,
            Some(Tab::Graph(id)),
        ));
        let matrix = header_actions_width(shows_detach_and_maximize(
            HeaderPlacement::Floating,
            Some(Tab::Matrix(id)),
        ));
        assert_eq!(cross, 28.0);
        assert_eq!(three, 80.0);
        assert_eq!(matrix, cross);
        assert!(shows_detach_and_maximize(HeaderPlacement::Detached, None));
    }

    /// #50, drawing side: the number of icons really painted on the right of
    /// the header is the one [`shows_detach_and_maximize`] announces, and
    /// all fit in the width that [`header_actions_width`] takes off
    /// the title — otherwise the title would pass under an icon (or be
    /// clipped for a missing icon).
    #[test]
    fn the_icons_painted_on_the_right_are_those_the_width_reserves() {
        let id = NodeId::from_raw(1);
        for (tab, placement) in [
            (Some(Tab::Graph(id)), HeaderPlacement::Floating),
            (Some(Tab::Book(id)), HeaderPlacement::Floating),
            (Some(Tab::Matrix(id)), HeaderPlacement::Floating),
            (Some(Tab::Note(id)), HeaderPlacement::Floating),
            (Some(Tab::Matrix(id)), HeaderPlacement::Detached),
            (Some(Tab::Graph(id)), HeaderPlacement::Detached),
            (None, HeaderPlacement::Dialog),
        ] {
            let on_the_right: Vec<_> = header_icons(tab, placement)
                .into_iter()
                .filter(|center| center.x > HEADER_WIDTH / 2.0)
                .collect();
            let three = shows_detach_and_maximize(placement, tab);
            assert_eq!(on_the_right.len(), if three { 3 } else { 1 }, "{tab:?}");
            let reserved = header_actions_width(three);
            for center in on_the_right {
                assert!(
                    center.x - ICON_SIZE / 2.0 >= HEADER_WIDTH - reserved,
                    "{tab:?}: an icon ({center:?}) overflows onto the title's place ({reserved})"
                );
            }
        }
        // A dialog has no collapse arrow on the left either.
        assert!(header_icons(None, HeaderPlacement::Dialog)
            .iter()
            .all(|center| center.x > HEADER_WIDTH / 2.0));
    }

    /// The title bar's gestures go up to the caller: dragging the
    /// title gives its displacement (and the start signal, only once),
    /// clicking the cross requests closing. The witness: dragging the title
    /// closes nothing.
    #[test]
    fn dragging_the_title_moves_and_clicking_the_cross_closes() {
        let tab = Some(Tab::Graph(NodeId::from_raw(1)));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        let context = egui::Context::default();
        let start = egui::pos2(HEADER_WIDTH / 2.0, WINDOW_HEADER_HEIGHT / 2.0);
        let end = start + egui::vec2(30.0, 10.0);
        let (_, actions) = paint_header(
            &context,
            "Title",
            tab,
            HeaderPlacement::Floating,
            vec![
                vec![egui::Event::PointerMoved(start)],
                vec![button(start, true)],
                vec![egui::Event::PointerMoved(end)],
                vec![button(end, false)],
            ],
        );
        let displacement = actions
            .iter()
            .fold(egui::Vec2::ZERO, |sum, action| sum + action.drag_delta);
        assert_eq!(displacement, end - start);
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.title_drag_started)
                .count(),
            1
        );
        assert!(actions.iter().all(|action| !action.close
            && !action.detach
            && !action.maximize
            && !action.toggle_collapse));

        let context = egui::Context::default();
        let cross = *header_icons(tab, HeaderPlacement::Floating)
            .last()
            .expect("the cross is the last icon on the right");
        let (_, actions) = paint_header(
            &context,
            "Title",
            tab,
            HeaderPlacement::Floating,
            vec![
                vec![egui::Event::PointerMoved(cross)],
                vec![button(cross, true)],
                vec![button(cross, false)],
            ],
        );
        assert!(actions.iter().any(|action| action.close));
        assert!(actions
            .iter()
            .all(|action| action.drag_delta == egui::Vec2::ZERO && !action.detach));
    }

    /// Drags, on a `(300, 300)`, `400 × 300` window, the targeted resize
    /// handle at `grab` by `displacement`, then releases and
    /// paints one more frame with no event. Returns the position and size
    /// after the drag, then after the rest frame.
    fn drag_handle(
        grab: impl FnOnce(egui::Rect) -> egui::Pos2,
        displacement: egui::Vec2,
    ) -> ((egui::Pos2, egui::Vec2), (egui::Pos2, egui::Vec2)) {
        let tab = Tab::Graph(NodeId::from_raw(1));
        let context = egui::Context::default();
        let state = std::cell::Cell::new((egui::pos2(300.0, 300.0), egui::vec2(400.0, 300.0)));

        let paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let (mut position, mut size) = state.get();
            let rect = egui::Rect::from_min_size(position, size);
            let mut output = context.run_ui(input, |ui| {
                floating_resize_handles(ui, tab, rect, &mut position, &mut size);
            });
            output.textures_delta.clear();
            state.set((position, size));
        };

        paint(Vec::new());
        let (position, size) = state.get();
        let start = grab(egui::Rect::from_min_size(position, size));
        paint(vec![egui::Event::PointerMoved(start)]);
        paint(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let end = start + displacement;
        paint(vec![egui::Event::PointerMoved(end)]);
        let after_drag = state.get();
        paint(vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(Vec::new());
        (after_drag, state.get())
    }

    /// Regression (§3, resizing by the 4 sides): `egui::Window`
    /// cuts `resize_left`/`resize_top` internally as soon as
    /// `.drag_area(WindowDrag::Off)` is set (needed for the drag of the
    /// home-made title bar) — both depend on `Area::is_movable()`,
    /// which `Off` forces to `false`, with no way to keep them active separately.
    /// Hence [`floating_resize_handles`], which entirely replaces the native
    /// resizing. This test drags its west handle and checks
    /// that the position *and* the size follow, then that a frame with no
    /// event makes nothing jump back.
    #[test]
    fn resizing_by_the_left_edge_moves_and_shrinks() {
        let ((position, size), at_rest) = drag_handle(
            |rect| egui::pos2(rect.left() - 3.0, rect.center().y),
            egui::vec2(-40.0, 0.0),
        );
        assert!(
            (position.x - 260.0).abs() < f32::EPSILON,
            "the west edge must move the window's corner by the same distance as the drag, position={position:?}"
        );
        assert!(
            (size.x - 440.0).abs() < f32::EPSILON,
            "the west edge must widen the window as much as the corner moves back, size={size:?}"
        );
        assert_eq!(
            (position.y, size.y),
            (300.0, 300.0),
            "a vertical edge does not touch the height"
        );
        // Next frame with no event at all: nothing must move anymore —
        // unlike the native bug, there is here no mechanism that
        // could make the window jump back (`position`/`size`
        // are our own state, not an internal memory that another
        // call could overwrite).
        assert_eq!(at_rest, (position, size));
    }

    /// Shrinking by the west edge stops at [`MIN_CONTENT_WINDOW_SIZE`], and
    /// the **east** edge then stays in place: the window does not slide to the
    /// right once its minimum size is reached.
    #[test]
    fn shrinking_by_the_left_edge_stops_at_the_minimum_size() {
        let east_edge = 300.0 + 400.0;
        let ((position, size), _) = drag_handle(
            |rect| egui::pos2(rect.left() - 3.0, rect.center().y),
            egui::vec2(600.0, 0.0),
        );
        assert_eq!(size.x, MIN_CONTENT_WINDOW_SIZE.x);
        assert_eq!(position.x + size.x, east_edge);
    }

    /// #75: a corner is targeted along its two neighboring sides, over
    /// `CORNER_REACH`, not only in a 6 × 6 square. Grabbed on the
    /// top edge **near** the north-west corner, the drag resizes on both
    /// axes; the witness, grabbed in the middle of the same edge, touches only
    /// the height.
    #[test]
    fn a_corner_is_grabbed_along_its_two_sides() {
        let displacement = egui::vec2(-20.0, -30.0);
        let ((position, size), _) = drag_handle(
            |rect| egui::pos2(rect.left() + CORNER_REACH - 2.0, rect.top() - 3.0),
            displacement,
        );
        assert_eq!(position, egui::pos2(280.0, 270.0), "north-west corner");
        assert_eq!(size, egui::vec2(420.0, 330.0), "north-west corner");

        let ((position, size), _) = drag_handle(
            |rect| egui::pos2(rect.center().x, rect.top() - 3.0),
            displacement,
        );
        assert_eq!(position, egui::pos2(300.0, 270.0), "north edge only");
        assert_eq!(size, egui::vec2(400.0, 330.0), "north edge only");
    }

    /// [`show_window_contents_bounded`], the common bound of floating and
    /// detached (B8) windows and of dialogs: the content is clipped to
    /// the space that remains under the header and does not push the frame,
    /// even if it claims more room than the screen. The witness: the parent `Ui`
    /// clips higher up (it covers the header) — content that inherited
    /// its clipping would be seen. The real contents (workbook, graph)
    /// often set their own clipping, which would hide the omission of this one.
    #[test]
    fn the_bounded_content_is_clipped_under_the_header_and_does_not_push_the_frame() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 150.0));
        let mut measures = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("header");
                    let space = ui.available_rect_before_wrap();
                    let parent = ui.clip_rect();
                    show_window_contents_bounded(ui, |ui| {
                        ui.label("content");
                        ui.allocate_space(egui::vec2(1000.0, 1000.0));
                    });
                    measures = Some((space, parent, ui.min_rect()));
                });
            },
        );
        output.textures_delta.clear();
        let (space, parent, frame_rect) = measures.expect("panel painted");
        assert!(
            parent.top() < space.top(),
            "witness: the parent covers the header ({parent:?} / {space:?})"
        );
        let clip = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(text) if text.galley.job.text == "content" => {
                    Some(painted.clip_rect)
                }
                _ => None,
            })
            .expect("content painted");
        assert_eq!(clip, parent.intersect(space), "clipped under the header");
        assert!(
            screen.contains_rect(frame_rect),
            "the content too big does not push the frame: {frame_rect:?}"
        );
    }
}
