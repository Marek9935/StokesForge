//! **The** application's tab strip (project standard, issue #19):
//! [`tab_strip`] / [`tab_strip_with`], its overflow memory, its gestures for
//! renaming, duplicating and deleting ([`TabOps`], [`TabEvent`]) and its
//! automatic scrolling ([`scroll_tab_strip_to_selected`]).
//!
//! Moved out of `app.rs` as is (#117). The strip of a workbook's sheets
//! (`sheet_tabs`) and the Workbook menu live with the workbook, in
//! `workbook.rs` (#120). The menus, arrows, tabs and rename are private
//! painting sections (#124); their calls create no extra egui scope and the
//! callbacks stay deferred until after drawing.
//!
//! Look (#169, option A of the tabs mockup): flat tabs, no frame. The active
//! one is written in the accent color and carries a mark **on the content
//! side** ([`TabStripSide`]) — under the label for a strip placed at the top
//! (properties pages), above it for a strip placed at the bottom (a
//! workbook's sheets); a grey rule separates the strip from the content.
//! Previously, the active tab was a button with the `selection.bg_fill`
//! background, the same as the selected entry of the Project Manager: three
//! roles, one look. Ruled out: option B, rectangular tabs in the Origin/Excel
//! style, busier with many sheets.

use eframe::egui;

use crate::icons::{icon_button, Icon, IconRole};
use crate::theme;
use stokesforge_app_state::tr;

use super::inplace_edit;
use super::menu_sections::MenuSections;
use super::shortcuts::SHORTCUT_DUPLICATE_SHEET;

/// Fixed height of the sheet tabs panel under the grid. `book_contents`
/// reserves it before building the table: the table's intrinsic height can no
/// longer push the tabs out of the window. Just the height of a tab and a
/// rule: the strip has no scroll bar left to house ("<" ">" buttons, see
/// `sheet_tabs`), and the grid's horizontal bar is below.
pub(super) const TAB_STRIP_HEIGHT: f32 = 24.0;

/// Horizontal margin of a tab label, on each side.
const TAB_PADDING_X: f32 = 10.0;

/// Thickness of the mark of the active (or hovered) tab.
const TAB_MARK: f32 = 2.0;

/// Position of the strip relative to its content: the mark of the active tab
/// and the separator rule go on the content side (#169).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabStripSide {
    /// Strip above the content (graph properties pages).
    Top,
    /// Strip below the content (a workbook's sheets).
    Bottom,
    /// Vertical strip at the left of the content (the rubrics of the
    /// Preferences window, #148): tabs stacked, the mark on the right edge of
    /// the active one, a vertical rule between the strip and the content. It
    /// has neither arrows nor scrolling: the rubrics are few.
    Left,
}

/// Memory from one frame to the next of the sheet tab strip: what is needed
/// to know, **before** drawing it (so before placing the "<" and ">"
/// buttons), whether tabs are hidden on the left or right — which can only be
/// seen by drawing the tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct TabStripOverflow {
    pub(super) hidden_left: bool,
    pub(super) hidden_right: bool,
}

/// What `sheet_tabs` (`workbook.rs`) drew: offset of the strip, hidden tabs,
/// and rectangles of the "<" and ">" buttons if they are shown. Only the
/// tests read it; the interface does not need it.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TabStripOutput {
    pub(super) offset: f32,
    pub(super) overflow: TabStripOverflow,
    pub(super) arrows: Option<[egui::Rect; 2]>,
    /// Indices of the first and last fully visible tabs.
    pub(super) fully_visible: Option<(usize, usize)>,
}

/// **The** application's tab strip (project standard, issue #19): every set
/// of tabs — a workbook's sheets, pages of the graph properties window — goes
/// through it, never through a row of `selectable_label`. Original style
/// (full-size button, no truncation): when it does not fit in the available
/// width, it scrolls horizontally rather than shrinking its tabs or folding
/// the hidden sheets into a "…" menu. It has **no** scroll bar: two buttons
/// "<" and ">", at the left of the strip, bring out the next hidden tab on
/// that side — and appear only if tabs are hidden. Changing the selected
/// sheet (by keyboard, script, or click) scrolls the strip just enough to
/// show the active tab and its neighbors ([`scroll_tab_strip_to_selected`]).
pub(crate) fn tab_strip<
    K: Copy + std::hash::Hash + std::fmt::Debug + PartialEq + Send + Sync + 'static,
>(
    ui: &mut egui::Ui,
    side: TabStripSide,
    tabs: &[(K, String)],
    selected: K,
    on_select: impl FnMut(K),
    add_label: Option<&str>,
    on_add: impl FnMut(),
) -> TabStripOutput {
    tab_strip_with(
        ui,
        side,
        tabs,
        selected,
        on_select,
        add_label,
        on_add,
        None,
        |_| {},
    )
}

/// [`tab_strip`] with, optionally, the gestures on a sheet: double-click
/// (in-place rename, validated by Enter or by leaving the field, abandoned by
/// Escape), context menu Rename / Duplicate / Delete. Requests go up through
/// `on_event`; the strip never touches the model.
#[allow(clippy::too_many_arguments)]
pub(super) fn tab_strip_with<
    K: Copy + std::hash::Hash + std::fmt::Debug + PartialEq + Send + Sync + 'static,
>(
    ui: &mut egui::Ui,
    side: TabStripSide,
    tabs: &[(K, String)],
    selected: K,
    mut on_select: impl FnMut(K),
    add_label: Option<&str>,
    mut on_add: impl FnMut(),
    ops: Option<&TabOps<K>>,
    mut on_event: impl FnMut(TabEvent<K>),
) -> TabStripOutput {
    if side == TabStripSide::Left {
        return tab_strip_vertical(ui, tabs, selected, on_select);
    }
    let config = TabStripConfig {
        side,
        tabs,
        selected,
        add_label,
        ops,
    };
    let mut requests = TabStripRequests {
        select: None,
        add: false,
        events: Vec::new(),
    };
    if tabs.is_empty() {
        ui.allocate_response(
            egui::vec2(ui.available_width(), TAB_STRIP_HEIGHT),
            egui::Sense::click(),
        )
        .context_menu(|ui| tab_strip_add_menu(ui, add_label, &mut requests.add));
        if requests.add {
            on_add();
        }
        return TabStripOutput::default();
    }
    // Remembered per strip instance (scoped by the parent's `Id` stack, so
    // distinct between floating and detached windows): triggers automatic
    // scrolling again only when the selection really changes from one frame
    // to the next, not on every frame where it is simply carried over —
    // otherwise the user's manual scrolling would be undone continuously.
    let last_selected_id = ui.id().with("tab_strip_last_selected");
    let last_selected: Option<K> = ui.memory(|memory| memory.data.get_temp(last_selected_id));
    let should_scroll_to_selected = last_selected != Some(selected);
    ui.memory_mut(|memory| memory.data.insert_temp(last_selected_id, selected));
    let overflow_id = ui.id().with("tab_strip_overflow");
    let overflow: TabStripOverflow = ui
        .memory(|memory| memory.data.get_temp(overflow_id))
        .unwrap_or_default();

    // -1: bring out the tab hidden on the left; +1: on the right.
    let mut step: i8 = 0;
    let mut content = TabStripContent::default();
    let mut arrows = None;
    let mut offset = 0.0;
    // The rule goes under the tab marks: reserved before them, placed once
    // the strip is measured.
    let rule = ui.painter().add(egui::Shape::Noop);
    let band = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        (step, arrows) = tab_strip_arrows(ui, overflow);
        let band_width = ui.available_width();
        offset = egui::ScrollArea::horizontal()
            .id_salt("tab_strip_scroll")
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    content = tab_strip_contents(
                        ui,
                        &config,
                        &mut requests,
                        should_scroll_to_selected,
                        step,
                        band_width,
                    );
                });
            })
            .state
            .offset
            .x;
    });
    let band = band.response.rect;
    let rule_y = match side {
        TabStripSide::Top => band.bottom() - 0.5,
        TabStripSide::Bottom => band.top() + 0.5,
        TabStripSide::Left => unreachable!("the vertical strip returned above"),
    };
    ui.painter().set(
        rule,
        egui::Shape::hline(
            band.x_range(),
            rule_y,
            egui::Stroke::new(1.0, theme::tab_rule_color()),
        ),
    );
    ui.memory_mut(|memory| memory.data.insert_temp(overflow_id, content.overflow));
    if step != 0 {
        ui.ctx().request_repaint();
    }
    if let Some(id) = requests.select {
        on_select(id);
    }
    if requests.add {
        on_add();
    }
    for event in requests.events {
        on_event(event);
    }
    TabStripOutput {
        offset,
        overflow: content.overflow,
        arrows,
        fully_visible: content.fully_visible,
    }
}

/// The vertical strip ([`TabStripSide::Left`]): one column of flat tabs of the
/// same width, the active one marked on its right edge, a vertical rule
/// along the column. No gestures on the tabs (rename, add), no overflow.
fn tab_strip_vertical<K: Copy + PartialEq>(
    ui: &mut egui::Ui,
    tabs: &[(K, String)],
    selected: K,
    mut on_select: impl FnMut(K),
) -> TabStripOutput {
    let mut picked = None;
    // One width for every tab, that of the widest bold label.
    let width = tabs
        .iter()
        .map(|(_, name)| tab_label_width(ui, name))
        .fold(0.0, f32::max);
    let rule = ui.painter().add(egui::Shape::Noop);
    let column = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (id, name) in tabs {
            let response = tab_strip_label(ui, name, *id == selected, TabStripSide::Left, width);
            if response.clicked() {
                picked = Some(*id);
            }
        }
    });
    let column = column.response.rect;
    ui.painter().set(
        rule,
        egui::Shape::vline(
            column.right() - 0.5,
            column.y_range(),
            egui::Stroke::new(1.0, theme::tab_rule_color()),
        ),
    );
    if let Some(id) = picked {
        on_select(id);
    }
    TabStripOutput::default()
}

/// Width of a tab's label when bold, the reserved one (see [`tab_strip_label`]).
fn tab_label_width(ui: &egui::Ui, name: &str) -> f32 {
    egui::WidgetText::from(theme::bold(name).size(12.0))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        )
        .size()
        .x
}

// Data and requests of a frame, without Ui or an id of its own: the
// extractions keep the egui scopes and the order of the strip's callbacks.
struct TabStripConfig<'a, K> {
    side: TabStripSide,
    tabs: &'a [(K, String)],
    selected: K,
    add_label: Option<&'a str>,
    ops: Option<&'a TabOps<K>>,
}

struct TabStripRequests<K> {
    select: Option<K>,
    add: bool,
    events: Vec<TabEvent<K>>,
}

#[derive(Default)]
struct TabStripContent {
    overflow: TabStripOverflow,
    fully_visible: Option<(usize, usize)>,
}

fn tab_strip_add_menu(ui: &mut egui::Ui, add_label: Option<&str>, requested_add: &mut bool) {
    if let Some(label) = add_label {
        if ui.button(label).clicked() {
            *requested_add = true;
            ui.close();
        }
    }
}

/// Menu of a tab (#152): Add ─ Rename · Duplicate ─ Delete, flat — the
/// right-click already designates the sheet, no "Sheet ▸" submenu.
fn tab_strip_tab_menu<K: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: K,
    config: &TabStripConfig<'_, K>,
    requests: &mut TabStripRequests<K>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, config.add_label.is_some(), |ui| {
        tab_strip_add_menu(ui, config.add_label, &mut requests.add);
    });
    let Some(ops) = config.ops else {
        return;
    };
    sections.section(ui, true, |ui| {
        let ctx = ui.ctx().clone();
        if ui
            .add(egui::Button::new(tr!("action-rename-sheet")))
            .on_hover_text(tr!("action-rename-sheet", "hint"))
            .clicked()
        {
            requests.events.push(TabEvent::BeginRename(id));
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-duplicate-sheet"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_DUPLICATE_SHEET)),
            )
            .on_hover_text(tr!("action-duplicate-sheet", "hint"))
            .clicked()
        {
            requests.events.push(TabEvent::Duplicate(id));
            ui.close();
        }
        if ui
            .add_enabled(
                ops.resettable.contains(&id),
                egui::Button::new(tr!("action-reset-sheet-column-widths")),
            )
            .on_hover_text(tr!("action-reset-sheet-column-widths", "hint"))
            .on_disabled_hover_text(tr!("action-reset-sheet-column-widths", "disabled"))
            .clicked()
        {
            requests.events.push(TabEvent::ResetColumnWidths(id));
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                ops.can_delete,
                egui::Button::new(tr!("action-delete-sheet")),
            )
            .on_hover_text(tr!("action-delete-sheet", "hint"))
            .on_disabled_hover_text(tr!("action-delete-sheet", "disabled"))
            .clicked()
        {
            requests.events.push(TabEvent::Delete(id));
            ui.close();
        }
    });
}

fn tab_strip_arrows(
    ui: &mut egui::Ui,
    overflow: TabStripOverflow,
) -> (i8, Option<[egui::Rect; 2]>) {
    let mut step = 0;
    let mut arrows = None;
    if overflow.hidden_left || overflow.hidden_right {
        let previous = ui
            .add_enabled_ui(overflow.hidden_left, |ui| {
                icon_button(
                    ui,
                    Icon::CaretLeft,
                    IconRole::Neutral,
                    tr!("tabs-previous-worksheets"),
                )
            })
            .inner;
        let next = ui
            .add_enabled_ui(overflow.hidden_right, |ui| {
                icon_button(
                    ui,
                    Icon::CaretRight,
                    IconRole::Neutral,
                    tr!("tabs-next-worksheets"),
                )
            })
            .inner;
        if previous.clicked() {
            step = -1;
        }
        if next.clicked() {
            step = 1;
        }
        arrows = Some([previous.rect, next.rect]);
    }

    (step, arrows)
}

/// Why this tab name is refused (S-T4, red frame): empty, or already taken by
/// another tab of the strip.
fn tab_name_refusal<K: PartialEq>(tabs: &[(K, String)], id: &K, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        Some(tr!("field-refused-name-empty"))
    } else if tabs.iter().any(|(other, text)| other != id && text == name) {
        Some(tr!("field-refused-name-taken"))
    } else {
        None
    }
}

/// The in-place rename of a tab: the common contract of
/// [`inplace_edit`](super::inplace_edit) (whole name selected, Enter or a click
/// elsewhere validate, Escape cancels, Tab validates); an empty or already
/// taken name keeps the field open under a red frame.
fn tab_strip_rename<K: Copy + std::hash::Hash + std::fmt::Debug + PartialEq>(
    ui: &mut egui::Ui,
    id: K,
    name: &str,
    tabs: &[(K, String)],
    requests: &mut TabStripRequests<K>,
) -> egui::Response {
    let buffer_id = ui.id().with(("tab_rename_buffer", id));
    let mut text = ui
        .memory(|memory| memory.data.get_temp::<String>(buffer_id))
        .unwrap_or_else(|| name.to_owned());
    let width = (name.chars().count().max(6) as f32 * 8.0).min(240.0);
    let refusal = tab_name_refusal(tabs, &id, &text);
    let shown = inplace_edit::show(
        ui,
        buffer_id.with("edit"),
        egui::TextEdit::singleline(&mut text)
            .font(egui::FontId::proportional(12.0))
            .desired_width(width),
        inplace_edit::Opening::SelectAll,
        inplace_edit::FocusLoss::Ends,
        refusal.as_deref(),
    );
    let refused = tab_name_refusal(tabs, &id, &text).is_some();
    match shown
        .end
        .map(|end| inplace_edit::resolve(end, text.trim() == name, refused))
    {
        Some(inplace_edit::Resolution::Close { write, end }) => {
            ui.memory_mut(|memory| memory.data.remove::<String>(buffer_id));
            requests.events.push(if write {
                TabEvent::CommitRename(id, text)
            } else {
                TabEvent::CancelRename
            });
            // Tab moves on to renaming the neighboring tab (Shift+Tab: the previous one).
            if let inplace_edit::EditEnd::Tab { shift } = end {
                let at = tabs.iter().position(|(other, _)| *other == id);
                let next = at.and_then(|at| {
                    if shift {
                        at.checked_sub(1)
                    } else {
                        Some(at + 1)
                    }
                });
                if let Some((next, _)) = next.and_then(|at| tabs.get(at)) {
                    requests.events.push(TabEvent::BeginRename(*next));
                }
            }
        }
        _ => {
            ui.memory_mut(|memory| memory.data.insert_temp(buffer_id, text));
        }
    }
    shown.response
}

fn tab_strip_tab<K: Copy + std::hash::Hash + std::fmt::Debug + PartialEq>(
    ui: &mut egui::Ui,
    id: K,
    name: &str,
    config: &TabStripConfig<'_, K>,
    requests: &mut TabStripRequests<K>,
) -> egui::Response {
    let renaming = config.ops.is_some_and(|ops| ops.renaming == Some(id));
    if renaming {
        tab_strip_rename(ui, id, name, config.tabs, requests)
    } else {
        let response = tab_strip_label(ui, name, config.selected == id, config.side, 0.0);
        response.context_menu(|ui| tab_strip_tab_menu(ui, id, config, requests));
        if response.clicked() {
            requests.select = Some(id);
        }
        if config.ops.is_some() && response.double_clicked() {
            requests.events.push(TabEvent::BeginRename(id));
        }
        response
    }
}

/// A flat tab (#169): label without a frame, written in the accent color if
/// active, with a mark on the content side — accent for the active one, grey
/// for the hovered one. `min_text_width`: width to reserve at least for the
/// label (the vertical strip gives its tabs one width), 0 otherwise.
fn tab_strip_label(
    ui: &mut egui::Ui,
    name: &str,
    selected: bool,
    side: TabStripSide,
    min_text_width: f32,
) -> egui::Response {
    // The width is that of the **bold** label for every tab: changing the
    // weight on selection then shifts no neighbor.
    let galley_of = |ui: &egui::Ui, text: egui::RichText| {
        egui::WidgetText::from(text.size(12.0)).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        )
    };
    let reserved = galley_of(ui, theme::bold(name))
        .size()
        .x
        .max(min_text_width);
    let size = egui::vec2(reserved + 2.0 * TAB_PADDING_X, TAB_STRIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = super::cursors::clickable(response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, name)
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let mark = if selected {
            Some(theme::accent())
        } else if hovered {
            Some(theme::tab_hover_mark_color())
        } else {
            None
        };
        if let Some(color) = mark {
            let mark_rect = match side {
                TabStripSide::Top => egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom() - TAB_MARK),
                    rect.max,
                ),
                TabStripSide::Bottom => egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right(), rect.top() + TAB_MARK),
                ),
                TabStripSide::Left => egui::Rect::from_min_max(
                    egui::pos2(rect.right() - TAB_MARK, rect.top()),
                    rect.max,
                ),
            };
            ui.painter().rect_filled(mark_rect, 0.0, color);
        }
        let text_color = if selected {
            theme::accent()
        } else if hovered {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().text_color()
        };
        let galley = galley_of(
            ui,
            if selected {
                theme::bold(name)
            } else {
                egui::RichText::new(name)
            },
        );
        let text_pos = if side == TabStripSide::Left {
            // Rubrics are read as a list: aligned on the left.
            egui::pos2(
                rect.left() + TAB_PADDING_X,
                rect.center().y - galley.size().y / 2.0,
            )
        } else {
            rect.center() - galley.size() / 2.0
        };
        ui.painter().galley(text_pos, galley, text_color);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                2.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
    response
}

fn tab_strip_contents<K: Copy + std::hash::Hash + std::fmt::Debug + PartialEq>(
    ui: &mut egui::Ui,
    config: &TabStripConfig<'_, K>,
    requests: &mut TabStripRequests<K>,
    should_scroll_to_selected: bool,
    step: i8,
    band_width: f32,
) -> TabStripContent {
    let mut fully_visible: Option<(usize, usize)> = None;
    let visible = ui.clip_rect();
    let mut last_hidden_left = None;
    let mut first_hidden_right = None;
    let mut tab_rects = Vec::with_capacity(config.tabs.len());
    let mut selected_index = None;
    for (index, (id, name)) in config.tabs.iter().enumerate() {
        let response = tab_strip_tab(ui, *id, name, config, requests);
        tab_rects.push(response.rect);
        if config.selected == *id {
            selected_index = Some(index);
        }
        if response.rect.left() >= visible.left() - 0.5
            && response.rect.right() <= visible.right() + 0.5
        {
            let first = fully_visible.map_or(index, |(first, _)| first);
            fully_visible = Some((first, index));
        }
        if response.rect.left() < visible.left() - 0.5 {
            last_hidden_left = Some(response.clone());
        }
        if first_hidden_right.is_none() && response.rect.right() > visible.right() + 0.5 {
            first_hidden_right = Some(response.clone());
        }
    }
    let new_overflow = TabStripOverflow {
        hidden_left: last_hidden_left.is_some(),
        hidden_right: first_hidden_right.is_some(),
    };
    if should_scroll_to_selected {
        if let Some(index) = selected_index {
            scroll_tab_strip_to_selected(ui, &tab_rects, index, visible);
        }
    }
    match step {
        -1 => {
            if let Some(tab) = &last_hidden_left {
                tab.scroll_to_me(Some(egui::Align::Min));
            }
        }
        1 => {
            if let Some(tab) = &first_hidden_right {
                tab.scroll_to_me(Some(egui::Align::Max));
            }
        }
        _ => {}
    }
    let used = ui.cursor().left() - ui.min_rect().left();
    let empty = ui.allocate_response(
        egui::vec2((band_width - used).max(1.0), TAB_STRIP_HEIGHT),
        egui::Sense::click(),
    );
    empty.context_menu(|ui| tab_strip_add_menu(ui, config.add_label, &mut requests.add));
    TabStripContent {
        overflow: new_overflow,
        fully_visible,
    }
}

/// What the tab strip asks for on a sheet; the caller turns it into
/// [`Action`](stokesforge_app_state::state::Action)s.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TabEvent<K> {
    BeginRename(K),
    CommitRename(K, String),
    CancelRename,
    Duplicate(K),
    /// Reset all columns of the sheet to the default width (#330).
    ResetColumnWidths(K),
    Delete(K),
}

/// Rename, duplicate and delete gestures of a sheet tab strip.
pub(super) struct TabOps<K> {
    /// Tab whose name is being edited.
    pub(super) renaming: Option<K>,
    /// False on the last sheet of a workbook: "Delete" greyed out.
    pub(super) can_delete: bool,
    /// Tabs with at least one column that has a stored width (#330): for the
    /// others, "Reset Column Widths" is greyed out.
    pub(super) resettable: Vec<K>,
}

/// Automatic scrolling of the tab strip when the selected sheet changes:
/// **the smallest shift** that makes the selected tab and its two immediate
/// neighbors fully visible (so that what comes next is always seen). Nothing
/// moves if they already are; otherwise the strip aligns on the edge that was
/// exceeded. If the strip is too narrow for these three tabs, the selected tab
/// is centered.
///
/// Issue #1: the strip used to center the selected tab at every change —
/// clicking the last visible tab then shifted by two tabs or more instead of
/// one.
fn scroll_tab_strip_to_selected(
    ui: &egui::Ui,
    tab_rects: &[egui::Rect],
    selected: usize,
    visible: egui::Rect,
) {
    let current = tab_rects[selected];
    let first = tab_rects[selected.saturating_sub(1)];
    let last = tab_rects[(selected + 1).min(tab_rects.len() - 1)];
    let target = first.union(last);
    if target.width() > visible.width() {
        ui.scroll_to_rect(current, Some(egui::Align::Center));
    } else if target.right() > visible.right() + 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Max));
    } else if target.left() < visible.left() - 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Min));
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{context, key};
    use super::*;

    /// Ten tabs "Sheet N", with keys `0..10`.
    fn ten_tabs() -> Vec<(u32, String)> {
        (0..10)
            .map(|index| (index, format!("Sheet {index}")))
            .collect()
    }

    /// Paints a tab strip within `width` points, at time `*time + 0.1`, with
    /// `events`. Automatic scrolling is animated (`Response::scroll_to_me`):
    /// it spreads over `ScrollAnimation::duration` (up to 0.3 s by default);
    /// advancing `RawInput::time` explicitly simulates that lapse without
    /// depending on the real clock.
    fn paint_tabs(
        context: &egui::Context,
        width: f32,
        tabs: &[(u32, String)],
        selected: u32,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> (TabStripOutput, egui::FullOutput) {
        *time += 0.1;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 60.0),
            )),
            time: Some(*time),
            events,
            ..Default::default()
        };
        let mut strip = TabStripOutput::default();
        let mut output = context.run_ui(input, |ui| {
            strip = tab_strip(
                ui,
                TabStripSide::Bottom,
                tabs,
                selected,
                |_| {},
                None,
                || {},
            );
        });
        output.textures_delta.clear();
        (strip, output)
    }

    /// [`paint_tabs`] within 400 points (about five tabs out of ten).
    fn paint(
        context: &egui::Context,
        tabs: &[(u32, String)],
        selected: u32,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> TabStripOutput {
        paint_tabs(context, 400.0, tabs, selected, time, events).0
    }

    /// Center of the painted text `needle`, if it is painted.
    fn position(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == needle => {
                    Some(text.visual_bounding_rect().center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, needle)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|clipped| walk(&clipped.shape, needle))
    }

    fn click_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn the_properties_tabs_do_not_offer_an_empty_sheet_menu() {
        let context = context();
        let tabs = [(0, "Properties".to_string())];
        let mut time = 0.0;
        let (_, output) = paint_tabs(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        let pos = position(&output, "Properties").expect("witness: the tab is painted");
        paint_tabs(
            &context,
            800.0,
            &tabs,
            0,
            &mut time,
            vec![egui::Event::PointerMoved(pos)],
        );
        for pressed in [true, false] {
            paint_tabs(
                &context,
                800.0,
                &tabs,
                0,
                &mut time,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        let (_, output) = paint_tabs(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        assert!(position(&output, "Properties").is_some());
        assert!(position(&output, &tr!("menu-sheet")).is_none());
    }

    /// #169: the active tab carries an accent mark on the content side —
    /// under the label for a strip at the top, above it for a strip at the
    /// bottom — and its text is in the accent color. Witnesses: the inactive
    /// tab has neither a mark nor the accent, and no tab keeps the buttons'
    /// selection background; a grey rule runs under (or over) the whole strip.
    #[test]
    fn the_active_tab_is_underlined_on_the_content_side() {
        for side in [TabStripSide::Top, TabStripSide::Bottom, TabStripSide::Left] {
            let context = context();
            let tabs: Vec<(u32, String)> = vec![(0, "Graph".into()), (1, "Axes".into())];
            let run = || {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 100.0),
                    )),
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    tab_strip(ui, side, &tabs, 1, |_| {}, None, || {});
                });
                output.textures_delta.clear();
                output
            };
            run();
            let output = run();
            let rects = |fill: egui::Color32| -> Vec<egui::Rect> {
                output
                    .shapes
                    .iter()
                    .filter_map(|painted| match &painted.shape {
                        egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                        _ => None,
                    })
                    .collect()
            };
            let text = |needle: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|painted| match &painted.shape {
                        egui::Shape::Text(text) if text.galley.text() == needle => {
                            Some((text.visual_bounding_rect(), text.fallback_color))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{needle:?} is not painted"))
            };
            let marks = rects(theme::accent());
            assert_eq!(marks.len(), 1, "{side:?}: a single mark, the active one's");
            let mark = marks[0];
            let vertical = side == TabStripSide::Left;
            assert_eq!(
                if vertical {
                    mark.width()
                } else {
                    mark.height()
                },
                TAB_MARK
            );
            let (active, active_color) = text("Axes");
            let (inactive, inactive_color) = text("Graph");
            assert_eq!(active_color, theme::accent());
            let bold = |needle: &str| {
                super::super::test_support::texts(&output.shapes)
                    .into_iter()
                    .find(|t| t.text == needle)
                    .unwrap()
                    .bold
            };
            assert!(bold("Axes"), "{side:?}: the active tab is bold");
            assert!(!bold("Graph"), "{side:?}: witness, the inactive one is not");
            assert_ne!(inactive_color, theme::accent(), "{side:?}: witness");
            if vertical {
                // Stacked tabs: the mark faces the active label only.
                assert!(mark.y_range().contains(active.center().y), "{side:?}");
                assert!(!mark.y_range().contains(inactive.center().y), "{side:?}");
            } else {
                assert!(mark.x_range().contains(active.center().x), "{side:?}");
                assert!(!mark.x_range().contains(inactive.center().x), "{side:?}");
            }
            match side {
                TabStripSide::Top => assert!(mark.top() >= active.bottom(), "{mark:?} {active:?}"),
                TabStripSide::Bottom => {
                    assert!(mark.bottom() <= active.top(), "{mark:?} {active:?}")
                }
                // On the right edge: the mark is beyond the label.
                TabStripSide::Left => {
                    assert!(mark.left() >= active.right(), "{mark:?} {active:?}")
                }
            }
            let button_fill = context.global_style().visuals.selection.bg_fill;
            assert!(
                rects(button_fill).is_empty(),
                "{side:?}: no more button background"
            );
            let rule = output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::LineSegment { points, stroke }
                        if stroke.color == theme::tab_rule_color() =>
                    {
                        Some(*points)
                    }
                    _ => None,
                })
                .expect("the strip rule is painted");
            if vertical {
                assert_eq!(rule[0].x, rule[1].x, "the rule is vertical");
                assert!(
                    rule[1].y - rule[0].y >= 2.0 * TAB_STRIP_HEIGHT,
                    "the rule runs along the whole column"
                );
            } else {
                assert!(
                    rule[1].x - rule[0].x >= 390.0,
                    "the rule runs along the strip"
                );
            }
            match side {
                TabStripSide::Top => assert!(rule[0].y >= mark.top()),
                TabStripSide::Bottom => assert!(rule[0].y <= mark.bottom()),
                // The rule lies against the mark, on the content side.
                TabStripSide::Left => assert!(rule[0].x >= mark.left()),
            }
        }
    }

    /// Making a tab bold on selection shifts no tab: the reserved width is the
    /// bold one for all. The accent mark covers the active tab: its width,
    /// measured when the tab is active, must be the one that separates its
    /// left edge from the left edge of the next one, measured when the
    /// previous one is inactive (gap between tabs: 2 points).
    #[test]
    fn changing_the_active_tab_shifts_no_tab() {
        let tabs: Vec<(u32, String)> = vec![
            (0, "General".into()),
            (1, "Plot area".into()),
            (2, "Axes".into()),
        ];
        let active_mark = |selected: u32| {
            let context = context();
            let mut time = 0.0;
            let mut output = None;
            for _ in 0..2 {
                output =
                    Some(paint_tabs(&context, 600.0, &tabs, selected, &mut time, Vec::new()).1);
            }
            output
                .unwrap()
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect) if rect.fill == theme::accent() => Some(rect.rect),
                    _ => None,
                })
                .expect("the active tab mark is painted")
        };
        let marks: Vec<egui::Rect> = (0..3).map(active_mark).collect();
        for k in 0..2 {
            assert!(
                (marks[k + 1].left() - marks[k].left() - marks[k].width() - 2.0).abs() < 0.01,
                "tab {k} keeps its width when it is no longer active: {marks:?}"
            );
        }
    }

    /// #19: all the tabs of a strip that fits in its width are painted, and a
    /// click on one of them chooses it. The witness: as long as nothing is
    /// clicked, nothing is chosen — the selection carried over from one frame
    /// to the next is not a choice. (This is the graph properties strip; that
    /// they use it is enforced by
    /// `a_click_on_a_properties_tab_changes_the_page`, in
    /// `app/plot_settings.rs`.)
    #[test]
    fn a_click_on_a_tab_chooses_it() {
        let context = context();
        let tabs: Vec<(u32, String)> = ["Graph", "Axes", "Series", "Theme"]
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u32, name.to_string()))
            .collect();
        let chosen = std::cell::Cell::new(None);
        let run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 200.0),
                )),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                tab_strip(
                    ui,
                    TabStripSide::Top,
                    &tabs,
                    0,
                    |tab| chosen.set(Some(tab)),
                    None,
                    || {},
                );
            });
            output.textures_delta.clear();
            output
        };
        run(vec![]);
        let output = run(vec![]);
        for (_, name) in &tabs {
            assert!(position(&output, name).is_some(), "{name}");
        }
        let theme = position(&output, "Theme").unwrap();
        run(vec![egui::Event::PointerMoved(theme)]);
        assert_eq!(chosen.get(), None, "hovering a tab does not choose it");
        run(vec![click_event(theme, true)]);
        run(vec![click_event(theme, false)]);
        assert_eq!(chosen.get(), Some(3));
    }

    /// Ten tabs at natural size do not fit in 200 points: the strip must
    /// scroll rather than shrink its buttons or fold the hidden sheets into a
    /// menu. Each painted tab keeps its whole name, at the same width as in a
    /// strip wide enough for all (witness), and the tabs that do not fit are
    /// hidden on the right.
    #[test]
    fn tabs_scroll_rather_than_shrink() {
        let tabs = ten_tabs();
        let name_width = |output: &egui::FullOutput, needle: &str| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(text) if text.galley.text() == needle => {
                        Some(text.galley.rect.width())
                    }
                    _ => None,
                })
        };

        let wide = context();
        let mut time = 0.0;
        let mut at_wide = None;
        for _ in 0..3 {
            at_wide = Some(paint_tabs(&wide, 2000.0, &tabs, 0, &mut time, Vec::new()));
        }
        let (wide_strip, wide_output) = at_wide.unwrap();
        assert_eq!(wide_strip.fully_visible, Some((0, 9)));
        assert!(wide_strip.arrows.is_none());

        let narrow = context();
        let mut at_narrow = None;
        for _ in 0..3 {
            at_narrow = Some(paint_tabs(&narrow, 200.0, &tabs, 0, &mut time, Vec::new()));
        }
        let (strip, output) = at_narrow.unwrap();
        assert!(
            strip.overflow.hidden_right,
            "tabs must be hidden on the right: {strip:?}"
        );
        let (first, last) = strip.fully_visible.expect("visible tabs");
        assert!(first == 0 && last < 9, "{strip:?}");
        for (_, name) in &tabs[..=last] {
            assert_eq!(
                name_width(&output, name),
                name_width(&wide_output, name),
                "{name} must keep its natural width"
            );
        }
    }

    #[test]
    fn selecting_an_out_of_view_tab_scrolls_the_strip_to_show_it() {
        let context = context();
        let tabs = ten_tabs();
        let mut time = 0.0;
        // Select the first tab first, let the scrolling settle, then switch
        // to the last one: out of view, it must trigger a new scroll
        // (`should_scroll_to_selected`).
        let mut before = TabStripOutput::default();
        for _ in 0..3 {
            before = paint(&context, &tabs, 0, &mut time, Vec::new());
        }
        let mut after = TabStripOutput::default();
        for _ in 0..8 {
            after = paint(&context, &tabs, 9, &mut time, Vec::new());
        }
        assert!(
            after.offset > before.offset,
            "selecting an out-of-view tab must scroll the strip towards it: {before:?} -> {after:?}"
        );
        assert_eq!(after.fully_visible.map(|(_, last)| last), Some(9));
    }

    /// Issue #1: clicking the last fully visible tab shifts the strip just
    /// enough to show its right neighbor — one more tab, not two (the old
    /// behavior centered the selected tab). Symmetrically towards the left.
    #[test]
    fn selecting_the_last_visible_tab_shifts_by_one_tab_only() {
        let context = context();
        let tabs = ten_tabs();
        let mut time = 0.0;
        let mut strip = TabStripOutput::default();
        for _ in 0..4 {
            strip = paint(&context, &tabs, 0, &mut time, Vec::new());
        }
        let (_, last) = strip.fully_visible.expect("visible tabs");
        assert!(last + 2 < tabs.len(), "tabs must be hidden on the right");
        for _ in 0..8 {
            strip = paint(&context, &tabs, last as u32, &mut time, Vec::new());
        }
        let (first_after, last_after) = strip.fully_visible.unwrap();
        assert_eq!(
            last_after,
            last + 1,
            "only the right neighbor of the selected tab must appear: {strip:?}"
        );

        // The witness: selecting a tab already visible in the middle of the
        // strip, neighbors included, does not move it.
        let shift = strip.offset;
        for _ in 0..8 {
            strip = paint(
                &context,
                &tabs,
                (first_after + 2) as u32,
                &mut time,
                Vec::new(),
            );
        }
        assert_eq!(strip.offset, shift, "{strip:?}");

        // Towards the left: selecting the first visible tab shows its left
        // neighbor, and nothing more.
        for _ in 0..8 {
            strip = paint(&context, &tabs, first_after as u32, &mut time, Vec::new());
        }
        assert_eq!(
            strip.fully_visible.unwrap().0,
            first_after - 1,
            "only the left neighbor must appear: {strip:?}"
        );

        // A tab selected far from view (by script) becomes visible again.
        for _ in 0..8 {
            strip = paint(&context, &tabs, 9, &mut time, Vec::new());
        }
        assert_eq!(strip.fully_visible.unwrap().1, 9, "{strip:?}");
    }

    /// The "<" and ">" buttons appear only if tabs are hidden; ">" scrolls
    /// the strip to the right, then "<" becomes active.
    #[test]
    fn the_tab_arrows_appear_only_if_tabs_are_hidden() {
        let ctx = context();
        let tabs = ten_tabs();
        let mut time = 0.0;
        let mut strip = TabStripOutput::default();
        for _ in 0..3 {
            strip = paint(&ctx, &tabs, 0, &mut time, Vec::new());
        }
        assert_eq!(
            strip.overflow,
            TabStripOverflow {
                hidden_left: false,
                hidden_right: true
            }
        );
        let [_, next] = strip.arrows.expect("hidden tabs: arrows shown");
        let pos = next.center();
        paint(
            &ctx,
            &tabs,
            0,
            &mut time,
            vec![egui::Event::PointerMoved(pos), click_event(pos, true)],
        );
        paint(&ctx, &tabs, 0, &mut time, vec![click_event(pos, false)]);
        for _ in 0..6 {
            strip = paint(&ctx, &tabs, 0, &mut time, Vec::new());
        }
        assert!(strip.offset > 0.0, "\">\" must scroll the strip: {strip:?}");
        assert!(
            strip.overflow.hidden_left,
            "\"<\" becomes useful: {strip:?}"
        );

        // Two tabs fit in the strip: no arrows.
        let ctx = context();
        let two = &tabs[..2];
        for _ in 0..3 {
            strip = paint(&ctx, two, 0, &mut time, Vec::new());
        }
        assert!(strip.arrows.is_none(), "no hidden tab: no arrows");
    }

    /// Paints three tabs with [`tab_strip_with`], one frame per batch of
    /// events, and returns what the strip asked for: chosen tabs and
    /// [`TabEvent`]s emitted, in order.
    fn gestures(
        ops: Option<&TabOps<u32>>,
        batches: Vec<Vec<egui::Event>>,
    ) -> (Vec<u32>, Vec<TabEvent<u32>>) {
        let context = context();
        let tabs: Vec<(u32, String)> = (0..3).map(|i| (i, format!("Sheet {i}"))).collect();
        let mut chosen_tabs = Vec::new();
        let mut requests = Vec::new();
        for events in batches {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 60.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                tab_strip_with(
                    ui,
                    TabStripSide::Bottom,
                    &tabs,
                    0,
                    |tab| chosen_tabs.push(tab),
                    None,
                    || {},
                    ops,
                    |event| requests.push(event),
                );
            });
            output.textures_delta.clear();
        }
        (chosen_tabs, requests)
    }

    /// A double-click on a tab asks for its rename — only when the strip
    /// carries gestures ([`TabOps`]). The witness: without them, the same
    /// double-click only chooses the tab.
    #[test]
    fn double_click_asks_for_rename_only_with_gestures() {
        // Locate the position of "Sheet 1": the strip does not return it.
        let context = context();
        let tabs: Vec<(u32, String)> = (0..3).map(|i| (i, format!("Sheet {i}"))).collect();
        let mut time = 0.0;
        paint_tabs(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        let (_, output) = paint_tabs(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        let pos = position(&output, "Sheet 1").expect("painted tab");
        let double_click = || {
            vec![
                Vec::new(),
                vec![egui::Event::PointerMoved(pos)],
                vec![click_event(pos, true)],
                vec![click_event(pos, false)],
                vec![click_event(pos, true)],
                vec![click_event(pos, false)],
            ]
        };

        let ops = TabOps {
            renaming: None,
            can_delete: true,
            resettable: Vec::new(),
        };
        let (chosen_tabs, requests) = gestures(Some(&ops), double_click());
        assert_eq!(requests, vec![TabEvent::BeginRename(1)]);
        assert!(chosen_tabs.iter().all(|&tab| tab == 1));

        let (chosen_tabs, requests) = gestures(None, double_click());
        assert!(
            requests.is_empty(),
            "without gestures, no rename: {requests:?}"
        );
        assert!(!chosen_tabs.is_empty() && chosen_tabs.iter().all(|&tab| tab == 1));
    }

    /// The double-click asks for the selection first, then the rename: the
    /// callbacks stay deferred and ordered after the extractions.
    #[test]
    fn double_click_notifies_the_selection_before_the_rename() {
        #[derive(Debug, PartialEq)]
        enum Call {
            Selection(u32),
            Add,
            Gesture(TabEvent<u32>),
        }
        let context = context();
        let tabs = vec![(0, "First".into()), (1, "Second".into())];
        let ops = TabOps {
            renaming: None,
            can_delete: true,
            resettable: Vec::new(),
        };
        let calls = std::cell::RefCell::new(Vec::new());
        let paint = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 60.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    tab_strip_with(
                        ui,
                        TabStripSide::Bottom,
                        &tabs,
                        0,
                        |id| calls.borrow_mut().push(Call::Selection(id)),
                        Some("Add"),
                        || calls.borrow_mut().push(Call::Add),
                        Some(&ops),
                        |event| calls.borrow_mut().push(Call::Gesture(event)),
                    );
                },
            );
            output.textures_delta.clear();
            output
        };
        paint(Vec::new());
        let output = paint(Vec::new());
        assert!(
            calls.borrow().is_empty(),
            "witness: painting notifies no choice"
        );
        let pos = position(&output, "Second").unwrap();
        paint(vec![egui::Event::PointerMoved(pos)]);
        paint(vec![click_event(pos, true)]);
        paint(vec![click_event(pos, false)]);
        assert_eq!(
            calls.take(),
            [Call::Selection(1)],
            "a click only chooses the tab"
        );
        paint(vec![click_event(pos, true)]);
        paint(vec![click_event(pos, false)]);
        assert_eq!(
            calls.take(),
            [Call::Selection(1), Call::Gesture(TabEvent::BeginRename(1))]
        );
    }

    /// The in-place rename: the field opens with the whole name selected, the
    /// input replaces it and Enter validates it; Escape abandons it without
    /// validating anything.
    #[test]
    fn in_place_rename_is_validated_by_enter_and_abandoned_by_escape() {
        let ops = TabOps {
            renaming: Some(1),
            can_delete: true,
            resettable: Vec::new(),
        };
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        let (_, requests) = gestures(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Spectra".to_string())],
                vec![press(egui::Key::Enter)],
            ],
        );
        assert_eq!(
            requests,
            vec![TabEvent::CommitRename(1, "Spectra".to_string())]
        );

        let (_, requests) = gestures(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Spectra".to_string())],
                vec![press(egui::Key::Escape)],
            ],
        );
        assert_eq!(requests, vec![TabEvent::CancelRename]);
    }

    /// An already taken or empty name keeps the field open (Enter does not
    /// validate); Tab on a valid name writes it and opens the rename of the
    /// next tab, Shift+Tab that of the previous one (S-T4, #224).
    #[test]
    fn rename_refuses_a_taken_name_and_tab_moves_to_the_neighboring_tab() {
        let ops = TabOps {
            renaming: Some(1),
            can_delete: true,
            resettable: Vec::new(),
        };
        let (_, requests) = gestures(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Sheet 2".to_string())],
                vec![key(egui::Key::Enter)],
            ],
        );
        assert!(requests.is_empty(), "a taken name is not validated");

        let (_, requests) = gestures(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Spectra".to_string())],
                vec![key(egui::Key::Tab)],
            ],
        );
        assert_eq!(
            requests,
            vec![
                TabEvent::CommitRename(1, "Spectra".to_string()),
                TabEvent::BeginRename(2)
            ]
        );

        // Without a change, Tab validates nothing but still moves on to the next one.
        let (_, requests) = gestures(
            Some(&ops),
            vec![Vec::new(), Vec::new(), vec![key(egui::Key::Tab)]],
        );
        assert_eq!(
            requests,
            vec![TabEvent::CancelRename, TabEvent::BeginRename(2)]
        );
    }
}
