//! The "font" building block (#554, S-N1, S-N9, S-F7): **the only** font field
//! of the interface, used by the Format bar, the General tab and the Legend tab
//! of Graph Properties. A font is found by typing part of its name.
//!
//! # What a field does
//!
//! It is a typed field like the number field with a list
//! ([`crate::number_field`]): white fill, filled `▼` inside the frame
//! ([`crate::list_frame`]), showing the current font (« Graph font » for the
//! default, the neutral label for a mixed format).
//!
//! - a click in the text selects everything and opens **nothing**; the first
//!   change of the text opens the list below, filtered by
//!   [`stokesforge_app_state::font_search`] (every typed word somewhere in the
//!   name, any order, case ignored);
//! - `▼` or Alt+↓ opens the **whole** list, scrolled to the current font, the
//!   text selected and focused; plain ↑/↓ and the wheel on the closed field do
//!   nothing;
//! - open list: ↑/↓ highlight the selectable rows (no wrap, never a title),
//!   the highlight goes back to the first match when the text changes; Enter
//!   picks the highlighted row (nothing found: nothing happens, the list
//!   stays; nothing typed nor moved: no change, the list closes); an emptied
//!   field + Enter gives « Graph font »; a click on a row picks it;
//! - Escape closes the list and gives the old name back, **in one press**, and
//!   never reaches the text editor of the canvas nor the dialog: this is the
//!   one exception to the two levels of S-F7. Clicking elsewhere or Tab closes
//!   it too: the old name comes back and nothing is picked. An incomplete name
//!   is never kept (so, unlike `field_state::edit_text`, leaving reverts);
//! - the list keeps one size while it is open (it must not shrink under the
//!   pointer) and has no live preview on hover.
//!
//! The list is an `egui::Area` of its own, not `egui::Popup`: egui keeps only
//! one popup open per viewport, so in a modal dialog the list would close the
//! parent window (issue #9, like [`crate::number_field`]). It does not show up
//! in `egui::Popup::is_any_open` either, which is as true of the number list;
//! the two other readers of that test (`page_layout::handle_keys`,
//! `window_band::stop_keys`) also look at the text focus, which the open list
//! always keeps.
//!
//! # Focus after a pick
//!
//! When a text of the canvas is being edited, a pick or an Escape in the Format
//! bar gives focus **and selection** back to that editor
//! ([`take_return_focus`], read by `graph_text_edit`): egui shrinks the
//! selection of a field painted without focus (#242), so the editor restores
//! what it kept. Without an editor, the field keeps the focus, so that Delete
//! or a letter never reaches the graph or the grid.

use eframe::egui;
use stokesforge_app_state::font_search::{self, Direction, Row, Section};
use stokesforge_app_state::tr;
use stokesforge_render_egui::FamilyEntry;

use crate::app::shortcuts;
use crate::field_state;
use crate::list_frame;

/// Height of the open list, in points (fixed: it never shrinks, S-N1).
const LIST_HEIGHT: f32 = 280.0;
/// Narrowest width of the open list, in points.
const LIST_MIN_WIDTH: f32 = 240.0;

/// What a font field returns.
pub struct FontField {
    /// The input field (hover, help).
    pub response: egui::Response,
    /// The font picked this frame: `Some("")` is « Graph font ». `None` when
    /// nothing changed (including picking the font already in place).
    pub picked: Option<String>,
}

/// Where the list must scroll to on its next painting.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollTo {
    Top,
    /// Put this row in the middle (opening on the current font).
    Center(usize),
    /// Bring this row into view (arrows).
    Visible(usize),
}

/// What the field remembers between two frames.
#[derive(Clone, Debug, Default)]
struct State {
    open: bool,
    /// The text typed so far; `None` while nothing was typed (the field shows
    /// the current font, the list is whole).
    draft: Option<String>,
    /// The row highlighted by the arrows or by typing.
    cursor: Option<usize>,
    scroll_to: Option<ScrollTo>,
    /// Select the whole text as soon as the field has the focus.
    select_all: bool,
    /// Scroll position and height of the visible part of the list.
    offset: f32,
    view_height: f32,
    /// The list's rectangle of the last frame.
    area: Option<egui::Rect>,
}

/// How a frame closes the list.
#[derive(Debug)]
enum Exit {
    /// Escape, Enter without a choice, `▼`: the old name comes back and the
    /// focus goes back to the editor or to the field.
    Close,
    /// A row picked, by Enter or a click.
    Pick(String),
    /// Click elsewhere, Tab: the old name comes back, the focus is left alone.
    Leave,
}

/// Places a font field in the current row. `current` is the font of the
/// setting (`""` = « Graph font »), `None` for a mixed format (neutral label).
/// `return_to` is the identifier of the canvas text editor being edited, if
/// any: it gets the focus back after a pick or an Escape.
pub fn font_field(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: Option<&str>,
    return_to: Option<egui::Id>,
) -> FontField {
    font_field_in(
        ui,
        id_salt,
        width,
        current,
        stokesforge_render_egui::available_families(),
        return_to,
    )
}

/// [`font_field`] over an explicit list of families (the tests').
pub fn font_field_in(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: Option<&str>,
    families: &[FamilyEntry],
    return_to: Option<egui::Id>,
) -> FontField {
    let ctx = ui.ctx().clone();
    let id = field_state::field_id(ui, id_salt);
    let state_id = id.with("font_list");
    let enabled = ui.is_enabled();
    let mut state = if enabled {
        ctx.data(|data| data.get_temp::<State>(state_id))
            .unwrap_or_default()
    } else {
        State::default()
    };
    let default_label = tr!("format-font-default");
    let shown = match current {
        Some("") => default_label.as_str(),
        Some(name) => name,
        None => "",
    };
    let rows_for = |state: &State| {
        font_search::rows(
            families,
            current.unwrap_or(""),
            &default_label,
            state.draft.as_deref().unwrap_or(""),
        )
    };
    let (focused, had_focus) = ctx.memory(|memory| {
        (
            memory.has_focus(id),
            memory.has_focus(id) || memory.had_focus_last_frame(id),
        )
    });
    let mut rows: Option<Vec<Row<'_>>> = None;
    let mut exit: Option<Exit> = None;

    // — Keyboard, before the field: the `TextEdit` would take ↑/↓ and Enter,
    // and the editor of the canvas or the dialog would take Escape. egui drops
    // the focus at the start of the pass of an Escape: the open list (or the
    // editor to give the focus back to), not the focus, says the key is ours. —
    if enabled
        && (state.open || (had_focus && return_to.is_some()))
        && key(ui, &shortcuts::KEY_ESCAPE)
    {
        exit = Some(Exit::Close);
    } else if enabled && focused {
        // Alt+↓ first: a plain ↓ would read it (egui matches "no modifier"
        // with any combination).
        let alt_down = key(ui, &shortcuts::KEY_ALT_ARROW_DOWN);
        let down = !alt_down && key(ui, &shortcuts::KEY_ARROW_DOWN);
        let up = !alt_down && key(ui, &shortcuts::KEY_ARROW_UP);
        let enter = key(ui, &shortcuts::KEY_ENTER);
        if alt_down && !state.open {
            open_whole(&mut state, &rows_for(&State::default()), current);
        } else if state.open {
            let list = rows.get_or_insert_with(|| rows_for(&state));
            if down || up {
                let from = state
                    .cursor
                    .or_else(|| current.and_then(|c| font_search::current_index(list, c)));
                let to = match (from, down) {
                    (None, _) => font_search::first_selectable(list),
                    (Some(from), true) => Some(font_search::step(list, from, Direction::Down)),
                    (Some(from), false) => Some(font_search::step(list, from, Direction::Up)),
                };
                if let Some(to) = to {
                    state.cursor = Some(to);
                    state.scroll_to = Some(ScrollTo::Visible(to));
                }
            }
            if enter {
                match state.cursor.and_then(|index| list.get(index)) {
                    Some(row) => {
                        exit = Some(row_family(row).map_or(Exit::Close, Exit::Pick));
                    }
                    // Nothing typed nor moved: no change.
                    None if state.draft.is_none() => exit = Some(Exit::Close),
                    // Nothing found: nothing happens, the list stays.
                    None => {}
                }
            }
        } else if enter {
            exit = Some(Exit::Close);
        }
    }

    // — The field. —
    let mut buffer = match (&exit, &state.draft) {
        (None, Some(draft)) => draft.clone(),
        _ => shown.to_owned(),
    };
    let hint = if current.is_none() {
        tr!("format-font")
    } else {
        String::new()
    };
    let response = ui.add(
        egui::TextEdit::singleline(&mut buffer)
            .id(id)
            .hint_text(hint)
            .margin(egui::Margin {
                right: 4 + list_frame::ARROW_ROOM as i8,
                ..egui::Margin::symmetric(4, 2)
            })
            .desired_width(width),
    );
    if exit.is_none() && response.changed() {
        state.draft = Some(buffer.clone());
        state.open = true;
        state.scroll_to = Some(ScrollTo::Top);
        let list = rows_for(&state);
        state.cursor = font_search::first_selectable(&list);
        rows = Some(list);
    }
    // A click in the name selects it all (the list stays closed); with the list
    // open it places the caret, to edit what was typed.
    if response.gained_focus() || (response.clicked() && !state.open) {
        state.select_all = true;
    }
    if state.select_all && response.has_focus() {
        select_all(&ctx, id, buffer.chars().count());
        state.select_all = false;
    }

    // — Leaving: Tab, a click elsewhere, a field no longer drawn for. A press
    // inside the list or the field keeps the field open (the press on a row or
    // on the arrow takes the focus away for a frame: it is given back). —
    // The release counts as pressing: the click on a row or on the arrow is
    // completed on it, after the press took the focus away.
    let pressing = ctx.input(|input| input.pointer.primary_down() || input.pointer.any_released());
    let pointer = ctx.input(|input| input.pointer.interact_pos());
    let inside = |rect: Option<egui::Rect>| {
        pointer.is_some_and(|pos| rect.is_some_and(|rect| rect.contains(pos)))
    };
    let on_field = inside(Some(response.rect));
    if exit.is_none() && state.open && !response.has_focus() {
        if pressing && (on_field || inside(state.area)) {
            response.request_focus();
        } else {
            exit = Some(Exit::Leave);
        }
    }
    let pressed_outside =
        ctx.input(|input| input.pointer.any_pressed()) && !on_field && !inside(state.area);
    if exit.is_none() && state.open && pressed_outside {
        exit = Some(Exit::Leave);
    }

    // — The arrow. —
    let arrow = list_frame::arrow_rect(response.rect);
    let button = ui.interact(arrow, id.with("arrow"), egui::Sense::click());
    // The arrow is a button, not text (#553).
    crate::app::cursors::button_in_field(&button);
    list_frame::paint_arrow(ui, arrow, &button);
    if enabled && button.clicked() && exit.is_none() {
        if state.open {
            exit = Some(Exit::Close);
        } else {
            open_whole(&mut state, &rows_for(&State::default()), current);
            state.select_all = true;
            response.request_focus();
        }
    }

    // — The list. —
    if enabled && state.open && exit.is_none() {
        let list = rows.get_or_insert_with(|| rows_for(&state));
        let marked = state
            .cursor
            .or_else(|| current.and_then(|c| font_search::current_index(list, c)));
        let size = egui::vec2(response.rect.width().max(LIST_MIN_WIDTH), LIST_HEIGHT);
        let anchor = response.rect.left_bottom() + egui::vec2(0.0, 2.0);
        let query = state.draft.clone().unwrap_or_default();
        let mut view = View {
            offset: state.offset,
            height: state.view_height,
        };
        let scroll = state.scroll_to.take();
        let mut clicked = None;
        let area = egui::Area::new(id.with("list"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .constrain(true)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    clicked =
                        font_popup_contents(ui, list, marked, &query, size, scroll, &mut view);
                });
            });
        state.offset = view.offset;
        state.view_height = view.height;
        state.area = Some(area.response.rect);
        if let Some(row) = clicked.and_then(|index| list.get(index)) {
            exit = row_family(row).map(Exit::Pick);
        }
    }

    // — How the frame ends. —
    let mut picked = None;
    let ended = exit.is_some();
    match exit {
        None => {}
        Some(Exit::Leave) => state = State::default(),
        Some(close_or_pick) => {
            if let Exit::Pick(name) = close_or_pick {
                picked = (Some(name.as_str()) != current).then_some(name);
            }
            state = State::default();
            give_focus_back(&ctx, &response, return_to, &mut state);
        }
    }
    // A closing and a selection of the text show a frame late.
    if ended || state.select_all {
        ctx.request_repaint();
    }
    if enabled {
        ctx.data_mut(|data| data.insert_temp(state_id, state));
    } else {
        ctx.data_mut(|data| data.remove::<State>(state_id));
    }
    FontField { response, picked }
}

/// The font a row stands for (`""` for « Graph font »); a title stands for none.
fn row_family(row: &Row<'_>) -> Option<String> {
    match row {
        Row::Default(_) => Some(String::new()),
        Row::Family(name) => Some((*name).to_owned()),
        Row::Title(_) => None,
    }
}

/// Opens the whole list: nothing typed, nothing moved, on the current font.
fn open_whole(state: &mut State, whole: &[Row<'_>], current: Option<&str>) {
    state.open = true;
    state.draft = None;
    state.cursor = None;
    state.select_all = true;
    state.scroll_to = Some(
        current
            .and_then(|c| font_search::current_index(whole, c))
            .map_or(ScrollTo::Top, ScrollTo::Center),
    );
}

/// After a pick or an Escape: the editor of the canvas gets the focus back
/// (with its selection), otherwise the field keeps it with its text selected.
fn give_focus_back(
    ctx: &egui::Context,
    response: &egui::Response,
    return_to: Option<egui::Id>,
    state: &mut State,
) {
    match return_to {
        Some(editor) => {
            ctx.data_mut(|data| data.insert_temp(return_focus_slot(), editor));
        }
        None => {
            state.select_all = true;
            response.request_focus();
        }
    }
}

fn return_focus_slot() -> egui::Id {
    egui::Id::new("stokesforge_font_list_return_focus")
}

/// Whether the font field asked for the focus to go back to the text editor
/// `field` (and forgets the request). Read by the editor when it is painted,
/// which is after the bar: it then restores the selection it kept (#242) and
/// takes the focus.
pub fn take_return_focus(ctx: &egui::Context, field: egui::Id) -> bool {
    ctx.data_mut(|data| {
        let asked = data.get_temp::<egui::Id>(return_focus_slot()) == Some(field);
        if asked {
            data.remove::<egui::Id>(return_focus_slot());
        }
        asked
    })
}

/// Consumes the key `shortcut` of the registry.
fn key(ui: &egui::Ui, shortcut: &egui::KeyboardShortcut) -> bool {
    ui.input_mut(|input| input.consume_shortcut(shortcut))
}

/// Selects all the text of the field `id` (`len` characters).
fn select_all(ctx: &egui::Context, id: egui::Id, len: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(len),
            )));
        state.store(ctx, id);
    }
}

/// Scroll position of the list as the last frame left it.
#[derive(Clone, Copy, Debug)]
struct View {
    offset: f32,
    height: f32,
}

/// The scroll offset that brings `target` where it belongs, or `None` if it
/// is already in place. `stride` is the distance between two rows.
fn scroll_offset(
    target: ScrollTo,
    rows: &[Row<'_>],
    stride: f32,
    row_height: f32,
    view: View,
) -> Option<f32> {
    let height = if view.height > 0.0 {
        view.height
    } else {
        LIST_HEIGHT
    };
    match target {
        ScrollTo::Top => Some(0.0),
        ScrollTo::Center(row) => Some((row as f32 * stride - (height - row_height) / 2.0).max(0.0)),
        ScrollTo::Visible(row) => {
            // A group title stays with its first family.
            let first = if row > 0 && matches!(rows.get(row - 1), Some(Row::Title(_))) {
                row - 1
            } else {
                row
            };
            let top = first as f32 * stride;
            let bottom = row as f32 * stride + row_height;
            if top < view.offset {
                Some(top)
            } else if bottom > view.offset + height {
                Some(bottom - height)
            } else {
                None
            }
        }
    }
}

/// The contents of the list (the name contains "popup": it is an exclusive
/// choice, `no_selectable_button_outside_a_menu`). Returns the clicked row.
fn font_popup_contents(
    ui: &mut egui::Ui,
    rows: &[Row<'_>],
    marked: Option<usize>,
    query: &str,
    size: egui::Vec2,
    scroll: Option<ScrollTo>,
    view: &mut View,
) -> Option<usize> {
    // The size never depends on what is shown: it must not shrink under the
    // pointer while the text filters the list.
    ui.set_width(size.x);
    ui.set_height(size.y);
    if rows.is_empty() {
        crate::app::empty_state::empty_state(ui, tr!("font-list-no-match", text = query.trim()));
        return None;
    }
    // The height a `selectable_label` really takes, so that the rows the
    // virtual list skips are as high as the ones it paints.
    let row_height =
        ui.spacing().interact_size.y.max(
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y,
        );
    let stride = row_height + ui.spacing().item_spacing.y;
    // RAW-SCROLL(#347): drop-down list, menu-like: no end margin
    let mut scroll_area = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(size.y);
    if let Some(offset) =
        scroll.and_then(|target| scroll_offset(target, rows, stride, row_height, *view))
    {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    let mut clicked = None;
    let output = scroll_area.show_rows(ui, row_height, rows.len(), |ui, range| {
        // Each row spans the whole list, not only its name.
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
            for index in range {
                match rows[index] {
                    Row::Title(section) => {
                        let title = match section {
                            Section::Embedded => tr!("format-font-embedded"),
                            Section::System => tr!("format-font-system"),
                        };
                        ui.allocate_ui(egui::vec2(ui.available_width(), row_height), |ui| {
                            ui.set_min_size(egui::vec2(ui.available_width(), row_height));
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| ui.label(crate::theme::dimmed_text(title)),
                            );
                        });
                    }
                    Row::Default(text) | Row::Family(text) => {
                        if ui.selectable_label(marked == Some(index), text).clicked() {
                            clicked = Some(index);
                        }
                    }
                }
            }
        });
    });
    view.offset = output.state.offset.y;
    view.height = output.inner_rect.height();
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fam(name: &str, embedded: bool) -> FamilyEntry {
        FamilyEntry {
            name: name.to_owned(),
            embedded,
        }
    }

    /// A fixed list, never the machine's fonts.
    fn sample() -> Vec<FamilyEntry> {
        vec![
            fam("Noto Sans", true),
            fam("Noto Serif", true),
            fam("Fira Code", false),
            fam("Noto Sans Mono", false),
            fam("Ubuntu", false),
        ]
    }

    fn event_key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn plain(key: egui::Key) -> egui::Event {
        event_key(key, egui::Modifiers::NONE)
    }

    fn text(text: &str) -> egui::Event {
        egui::Event::Text(text.to_owned())
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// A bench: one font field in a row, its font, the clock.
    struct Bench {
        ctx: egui::Context,
        families: Vec<FamilyEntry>,
        current: String,
        time: f64,
        id: Option<egui::Id>,
        field: egui::Rect,
        picks: Vec<String>,
        output: egui::FullOutput,
        /// The keys still in the input once the field was placed (not consumed).
        left_over: Vec<egui::Key>,
    }

    impl Bench {
        fn new(current: &str) -> Self {
            Self::with(current, sample())
        }

        fn with(current: &str, families: Vec<FamilyEntry>) -> Self {
            let ctx = egui::Context::default();
            ctx.set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&ctx);
            let mut bench = Self {
                ctx,
                families,
                current: current.to_owned(),
                time: 0.0,
                id: None,
                field: egui::Rect::NOTHING,
                picks: Vec::new(),
                output: egui::FullOutput::default(),
                left_over: Vec::new(),
            };
            bench.image(Vec::new());
            bench.image(Vec::new());
            bench
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            self.time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 500.0),
                )),
                events,
                time: Some(self.time),
                focused: true,
                ..Default::default()
            };
            let (families, current) = (&self.families, self.current.clone());
            let mut field = egui::Rect::NOTHING;
            let mut id = None;
            let mut picked = None;
            let mut left_over = Vec::new();
            let mut output = self.ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    id = Some(field_state::field_id(ui, "font"));
                    let result = font_field_in(ui, "font", 110.0, Some(&current), families, None);
                    field = result.response.rect;
                    picked = result.picked;
                });
                left_over = ui.input(|input| {
                    input
                        .events
                        .iter()
                        .filter_map(|event| match event {
                            egui::Event::Key {
                                key, pressed: true, ..
                            } => Some(*key),
                            _ => None,
                        })
                        .collect()
                });
            });
            self.left_over = left_over;
            output.textures_delta.clear();
            self.output = output;
            self.field = field;
            self.id = id;
            if let Some(name) = picked {
                self.current.clone_from(&name);
                self.picks.push(name);
            }
        }

        fn key(&mut self, key: egui::Key) {
            self.image(vec![plain(key)]);
            self.image(Vec::new());
        }

        fn typed(&mut self, typed: &str) {
            self.image(vec![text(typed)]);
            self.image(Vec::new());
        }

        fn click(&mut self, pos: egui::Pos2) {
            self.image(vec![egui::Event::PointerMoved(pos)]);
            self.image(vec![button(pos, true)]);
            self.image(vec![button(pos, false)]);
            self.image(Vec::new());
        }

        fn click_text(&mut self) {
            self.click(egui::pos2(self.field.left() + 12.0, self.field.center().y));
        }

        fn click_arrow(&mut self) {
            self.click(list_frame::arrow_rect(self.field).center());
        }

        fn state(&self) -> State {
            self.ctx
                .data(|data| data.get_temp::<State>(self.id.expect("id").with("font_list")))
                .unwrap_or_default()
        }

        fn open(&self) -> bool {
            self.state().open
        }

        fn painted(&self, needle: &str) -> bool {
            self.count(needle) > 0
        }

        fn count(&self, needle: &str) -> usize {
            self.output
                .shapes
                .iter()
                .filter(|painted| {
                    matches!(&painted.shape,
                        egui::Shape::Text(shape) if shape.galley.job.text == needle)
                })
                .count()
        }

        fn painted_containing(&self, needle: &str) -> bool {
            self.output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
            })
        }

        fn selection(&self) -> (usize, usize) {
            let state = egui::TextEdit::load_state(&self.ctx, self.id.expect("id")).expect("state");
            let range = state.cursor.char_range().expect("selection");
            let [low, high] = range.sorted_cursors();
            (low.index.0, high.index.0)
        }

        fn focused(&self) -> bool {
            self.ctx
                .memory(|memory| memory.has_focus(self.id.expect("id")))
        }

        /// The field has the focus, the whole list open or not as `open` says.
        fn focus(&mut self) {
            self.click_text();
            assert!(self.focused());
        }
    }

    #[test]
    fn a_click_in_the_name_selects_it_all_and_opens_nothing() {
        let mut bench = Bench::new("Fira Code");
        bench.click_text();
        assert!(bench.focused());
        assert_eq!(bench.selection(), (0, 9), "\"Fira Code\" entirely");
        assert!(!bench.open(), "the list stays closed");
        assert!(!bench.painted("Ubuntu"));
    }

    #[test]
    fn typing_opens_the_list_filtered_by_what_is_typed() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("noto");
        assert!(bench.open());
        assert!(bench.painted("Noto Sans Mono"));
        assert!(bench.painted("Noto Serif"));
        assert!(!bench.painted("Fira Code"), "filtered out");
        assert!(!bench.painted("Graph font"));
        assert!(bench.picks.is_empty(), "typing picks nothing");
    }

    #[test]
    fn typing_then_enter_picks_the_first_match_and_closes() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("fira");
        bench.image(vec![plain(egui::Key::Enter)]);
        assert!(
            !bench.left_over.contains(&egui::Key::Enter),
            "the field consumes Enter: the dialog never sees it"
        );
        bench.image(Vec::new());
        assert_eq!(bench.picks, vec!["Fira Code".to_owned()]);
        assert!(!bench.open());
        assert!(bench.painted("Fira Code"));
        assert!(!bench.painted("Noto Serif"), "list closed");
    }

    #[test]
    fn down_down_enter_picks_that_row_skipping_titles() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("noto");
        // Rows: title, Noto Sans (highlighted), Noto Serif, title, Noto Sans Mono.
        bench.key(egui::Key::ArrowDown);
        bench.key(egui::Key::ArrowDown);
        assert_eq!(bench.state().cursor, Some(4), "the title was skipped");
        bench.key(egui::Key::ArrowDown);
        assert_eq!(bench.state().cursor, Some(4), "no wrap at the end");
        bench.key(egui::Key::Enter);
        assert_eq!(bench.picks, vec!["Noto Sans Mono".to_owned()]);
        bench.focus();
        bench.typed("sans");
        for _ in 0..3 {
            bench.key(egui::Key::ArrowUp);
        }
        assert_eq!(bench.state().cursor, Some(1), "no wrap at the top");
    }

    #[test]
    fn typing_again_puts_the_highlight_back_on_the_first_match() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("noto");
        bench.key(egui::Key::ArrowDown);
        assert_eq!(bench.state().cursor, Some(2));
        bench.typed(" s");
        assert_eq!(bench.state().cursor, Some(1));
    }

    #[test]
    fn nothing_found_and_enter_changes_nothing_and_the_list_stays() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("zzz");
        assert!(bench.painted_containing("No font name contains \"zzz\""));
        bench.key(egui::Key::Enter);
        assert!(bench.picks.is_empty());
        assert!(bench.open(), "the list stays");
        assert!(bench.painted_containing("No font name contains \"zzz\""));
    }

    #[test]
    fn enter_on_an_untouched_field_changes_nothing_and_closes() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.key(egui::Key::Enter);
        assert!(bench.picks.is_empty());
        assert!(!bench.open());
        // The whole list opened by the arrow and not moved: same.
        bench.click_arrow();
        assert!(bench.open());
        bench.key(egui::Key::Enter);
        assert!(bench.picks.is_empty());
        assert!(!bench.open());
    }

    #[test]
    fn an_emptied_field_and_enter_give_the_graph_font() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.key(egui::Key::Backspace);
        assert!(bench.open(), "emptying is a change of the text");
        bench.key(egui::Key::Enter);
        assert_eq!(bench.picks, vec![String::new()]);
        assert!(bench.painted("Graph font"));
    }

    #[test]
    fn picking_the_font_already_in_place_is_no_change() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("ubuntu");
        bench.key(egui::Key::Enter);
        assert!(bench.picks.is_empty());
        assert!(!bench.open());
    }

    #[test]
    fn escape_gives_the_old_name_back_and_closes_in_one_press() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("fira");
        assert!(bench.open());
        bench.image(vec![plain(egui::Key::Escape)]);
        assert!(
            !bench.left_over.contains(&egui::Key::Escape),
            "the field consumes Escape: the editor and the dialog never see it"
        );
        bench.image(Vec::new());
        assert!(!bench.open(), "one press closes the list");
        assert!(bench.picks.is_empty());
        assert!(bench.painted("Ubuntu"), "the old name is back");
        assert!(!bench.painted("fira"));
        assert!(!bench.painted("Fira Code"));
        assert!(bench.focused(), "no focus lost to the canvas");
    }

    #[test]
    fn a_click_elsewhere_or_tab_closes_gives_the_old_name_back_and_picks_nothing() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("fira");
        assert!(bench.open());
        bench.click(egui::pos2(550.0, 450.0));
        assert!(!bench.open());
        assert!(bench.painted("Ubuntu"));
        assert!(bench.picks.is_empty());

        bench.focus();
        bench.typed("fira");
        assert!(bench.open());
        bench.image(vec![plain(egui::Key::Tab)]);
        bench.image(Vec::new());
        assert!(!bench.open());
        assert!(bench.painted("Ubuntu"));
        assert!(bench.picks.is_empty());
    }

    #[test]
    fn a_click_on_a_row_picks_it() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        bench.typed("noto");
        let row = bench
            .output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Noto Serif" => {
                    Some(shape.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("the row");
        // Well past the name, still inside the list.
        bench.click(egui::pos2(row.left() + 150.0, row.center().y));
        assert_eq!(bench.picks, vec!["Noto Serif".to_owned()]);
        assert!(!bench.open());
    }

    #[test]
    fn the_list_keeps_its_size_while_the_text_filters_it() {
        let mut bench = Bench::new("Ubuntu");
        bench.click_arrow();
        assert!(bench.open());
        bench.image(Vec::new());
        let whole = bench.state().area.expect("list rectangle");
        for typed in ["n", "o", "t", "o", "z"] {
            bench.typed(typed);
            let now = bench.state().area.expect("list rectangle");
            assert_eq!(now.size(), whole.size(), "after typing {typed:?}");
            assert_eq!(now.min, whole.min);
        }
    }

    #[test]
    fn arrows_and_the_wheel_on_the_closed_field_change_nothing() {
        let mut bench = Bench::new("Ubuntu");
        bench.focus();
        assert_eq!(bench.selection(), (0, 6));
        for key in [egui::Key::ArrowDown, egui::Key::ArrowUp] {
            bench.image(vec![plain(key)]);
            assert!(!bench.left_over.contains(&key), "{key:?} is consumed");
            bench.image(Vec::new());
        }
        assert_eq!(bench.selection(), (0, 6), "the caret did not move");
        bench.image(vec![
            egui::Event::PointerMoved(bench.field.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        bench.image(Vec::new());
        assert!(bench.picks.is_empty());
        assert!(!bench.open());
        assert!(bench.painted("Ubuntu"));
    }

    #[test]
    fn alt_down_opens_the_whole_list_on_the_current_font_with_the_text_selected() {
        let mut bench = Bench::new("Fira Code");
        bench.focus();
        bench.image(vec![event_key(egui::Key::ArrowDown, egui::Modifiers::ALT)]);
        bench.image(Vec::new());
        assert!(bench.open());
        assert!(bench.painted("Noto Serif") && bench.painted("Ubuntu"));
        assert!(
            bench.painted("Graph font"),
            "every row, Graph font included"
        );
        assert_eq!(bench.selection(), (0, 9));
    }

    #[test]
    fn the_arrow_lists_every_row_scrolled_to_the_current_font() {
        let families: Vec<FamilyEntry> = (0..120)
            .map(|index| fam(&format!("Face {index:03}"), false))
            .collect();
        let mut bench = Bench::with("Face 090", families);
        bench.click_arrow();
        bench.image(Vec::new());
        bench.image(Vec::new());
        assert!(bench.open());
        assert!(bench.state().offset > 0.0, "scrolled");
        // The field and its row both show the name.
        assert_eq!(bench.count("Face 090"), 2, "the current font is in view");
        assert!(!bench.painted("Face 000"), "the top is out of view");
        assert!(!bench.painted("Graph font"));
        assert!(bench.focused(), "the field keeps the focus");
        assert_eq!(bench.selection(), (0, 8), "text selected");
    }

    #[test]
    fn scrolling_brings_a_row_into_view_and_keeps_its_title_with_it() {
        let rows = [
            Row::Default("Graph font"),
            Row::Title(Section::Embedded),
            Row::Family("A"),
            Row::Family("B"),
        ];
        let view = View {
            offset: 0.0,
            height: 40.0,
        };
        assert_eq!(
            scroll_offset(ScrollTo::Top, &rows, 10.0, 8.0, view),
            Some(0.0)
        );
        assert_eq!(
            scroll_offset(ScrollTo::Visible(0), &rows, 10.0, 8.0, view),
            None
        );
        // Row 3 ends at 38 < 40: in view; a taller stride puts it out.
        assert_eq!(
            scroll_offset(ScrollTo::Visible(3), &rows, 10.0, 8.0, view),
            None
        );
        assert_eq!(
            scroll_offset(ScrollTo::Visible(3), &rows, 20.0, 8.0, view),
            Some(28.0)
        );
        // Going up to the first family shows its title too.
        let down = View {
            offset: 30.0,
            height: 40.0,
        };
        assert_eq!(
            scroll_offset(ScrollTo::Visible(2), &rows, 10.0, 8.0, down),
            Some(10.0)
        );
        assert_eq!(
            scroll_offset(ScrollTo::Center(3), &rows, 10.0, 8.0, view),
            Some(14.0)
        );
        assert_eq!(
            scroll_offset(ScrollTo::Center(0), &rows, 10.0, 8.0, view),
            Some(0.0)
        );
    }

    #[test]
    fn the_rows_and_the_titles_have_the_same_height() {
        let mut bench = Bench::new("Ubuntu");
        bench.click_arrow();
        bench.image(Vec::new());
        // The top of each name: « Graph font », a title, two families.
        let top = |label: &str| {
            bench
                .output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == label => Some(shape.pos.y),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{label} painted"))
        };
        let stride = top("Noto Serif") - top("Noto Sans");
        let across_the_title = top("Noto Sans") - top("Graph font");
        assert!(
            (across_the_title - 2.0 * stride).abs() < 0.6,
            "the title row is not one row high: {across_the_title} for {stride}"
        );
    }

    /// A bar greyed out drops what it held: no list, no draft.
    #[test]
    fn a_disabled_field_opens_no_list_and_keeps_no_draft() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let families = sample();
        let mut state_id = None;
        for frame in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.add_enabled_ui(false, |ui| {
                    let id = field_state::field_id(ui, "font").with("font_list");
                    state_id = Some(id);
                    if frame == 0 {
                        // The state it held while it was still enabled.
                        let held = State {
                            open: true,
                            draft: Some("fira".to_owned()),
                            ..State::default()
                        };
                        ui.ctx().data_mut(|data| data.insert_temp(id, held));
                    }
                    let field = font_field_in(ui, "font", 110.0, None, &families, None);
                    assert!(field.picked.is_none());
                });
            });
            output.textures_delta.clear();
            let painted_list = output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Text(shape) if shape.galley.job.text == "Fira Code")
            });
            assert!(!painted_list, "no list under a disabled field");
        }
        let held = ctx.data(|data| data.get_temp::<State>(state_id.expect("id")));
        assert!(held.is_none(), "the draft is dropped");
    }
}
