//! The "form" building block (#227, `docs/interface-standard.md`, § Choices,
//! layout): a **two-column** grid — labels on the left, fields
//! aligned on one vertical, checkboxes and notes in the fields column
//! (S-L1) — and the **exclusive choice** that is painted as radio buttons for
//! 2 or 3 options, as a dropdown list from 4 (S-C1), radios side by side if
//! each label fits in 3 words, one under the other otherwise (S-C2).
//!
//! Why not `egui::Grid`: an egui grid has no cell spanning two columns,
//! so a section title (S-L3) would widen the labels column there, and
//! `add_space` is forbidden in it (egui assertion) — yet the section title
//! sets its gap. The building block keeps ordinary rows
//! (`ui.horizontal`), where the "?" icon of `hover_help` is placed as elsewhere,
//! and aligns the fields column on the widest label, measured on the
//! previous frame and kept in egui's memory under the form's identifier
//! (as `egui::Grid` does for its columns). When this
//! width changes (first frame, other page), the frame is redone
//! (`request_discard`): a misaligned form is never seen.
//!
//! The Tab order follows the reading order (S-K6) because the rows are
//! painted from top to bottom and, within a row, from left to right: a
//! form does not have to set it.
//!
//! No collapsible section in a form (S-L5): too long, it goes into
//! tabs (`tab_strip`). The only exception, [`Form::collapsible`] (S-L8, #344):
//! an "Advanced" block closed by default, for settings that are almost never
//! touched and whose default is Auto. The field widths are those of
//! [`theme::FieldWidth`] (S-L2); the gaps, [`theme::SPACE_ITEM`] and
//! [`theme::SPACE_SECTION`] (S-L4).

use std::hash::Hash;

use eframe::egui;

use crate::color_picker::{color_button, ColorUsage, PickerEnv};
use crate::theme;

use super::hover_help::help_beside;

/// A form being painted: see [`form`].
pub(super) struct Form<'a> {
    ui: &'a mut egui::Ui,
    /// Width of the labels column kept for this frame (the widest of the
    /// previous frame).
    label_width: f32,
    /// The widest label painted in this frame.
    widest: f32,
}

/// Paints a form as a two-column grid (S-L1): `add` places its
/// rows through [`Form::row`], [`Form::field`], [`Form::note`] and
/// [`Form::section`]. `id_salt` tells two forms of the same `Ui` apart.
pub(super) fn form<R>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Form<'_>) -> R,
) -> R {
    let id = ui.make_persistent_id(("stokesforge_form", id_salt));
    let label_width = ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0);
    let (inner, widest) = {
        let mut form = Form {
            ui: &mut *ui,
            label_width,
            widest: 0.0,
        };
        let inner = add(&mut form);
        (inner, form.widest)
    };
    if (widest - label_width).abs() > 0.5 {
        ui.data_mut(|data| data.insert_temp(id, widest));
        ui.ctx().request_discard("labels column of a form");
    }
    inner
}

impl Form<'_> {
    /// The form's `Ui`, for what spans the whole width (a
    /// table, a preview, a list of rows).
    pub(super) fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// A section title (S-L3), over the whole width: [`theme::section_title`].
    pub(super) fn section(&mut self, title: impl Into<String>) -> egui::Response {
        theme::section_title(self.ui, title)
    }

    /// A row: `label` in the labels column, `field` in the fields column
    /// (the field, then if applicable its "?" icon).
    pub(super) fn row<R>(
        &mut self,
        label: impl Into<egui::WidgetText>,
        field: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let label_width = self.label_width;
        let (inner, width) = self
            .ui
            .horizontal(|ui| {
                let width = ui.label(label).rect.width();
                ui.add_space((label_width - width).max(0.0));
                (field(ui), width)
            })
            .inner;
        self.widest = self.widest.max(width);
        inner
    }

    /// A row without a label: `field` in the fields column (checkbox,
    /// button, group of radios under its label).
    pub(super) fn field<R>(&mut self, field: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let indent = self.indent();
        self.ui
            .horizontal(|ui| {
                ui.add_space(indent);
                field(ui)
            })
            .inner
    }

    /// A checkbox in the fields column, its reminder as a "?" icon
    /// on the right if `hint` is given. Returns `true` if it changed.
    pub(super) fn checkbox(
        &mut self,
        value: &mut bool,
        text: impl Into<egui::WidgetText>,
        hint: Option<String>,
    ) -> bool {
        self.field(|ui| match hint {
            Some(hint) => help_beside(ui, hint, |ui| ui.checkbox(value, text).changed()),
            None => ui.checkbox(value, text).changed(),
        })
    }

    /// A color row: `label` on the left, the swatch of the common picker
    /// ([`color_button`], identified by its label like
    /// `color_field`) in the fields column. Returns `true` when a gesture
    /// has just ended.
    pub(super) fn color(
        &mut self,
        label: &str,
        value: &mut String,
        usage: ColorUsage,
        auto_label: Option<&str>,
        env: &mut PickerEnv,
    ) -> bool {
        self.row(label, |ui| {
            color_button(ui, label, value, usage, auto_label, env)
        })
    }

    /// A faded plain text (side effect, scope: S-M1) in the fields column,
    /// which wraps within its column.
    pub(super) fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        let indent = self.indent();
        self.ui.horizontal_top(|ui| {
            ui.add_space(indent);
            ui.vertical(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
            });
        });
    }

    /// The rows of `add`, greyed out if `enabled` is false (S-G4: values
    /// visible and kept). Returns the group's response, for its reason.
    pub(super) fn enabled<R>(
        &mut self,
        enabled: bool,
        add: impl FnOnce(&mut Form<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let label_width = self.label_width;
        let mut widest = self.widest;
        let response = self.ui.add_enabled_ui(enabled, |ui| {
            let mut inner_form = Form {
                ui,
                label_width,
                widest,
            };
            let inner = add(&mut inner_form);
            widest = inner_form.widest;
            inner
        });
        self.widest = widest;
        response
    }

    /// A collapsible block (S-L8, #344), **closed by default**: an arrow and its
    /// title on a row, then the rows of `add` — in the same grid,
    /// hence aligned on the fields above — when it is open. Returns
    /// `None` when closed. The open/closed state lives in egui's memory under
    /// `id_salt`: it is kept for the session, never in the file.
    /// Reserved for "Advanced" settings whose defaults are all Auto; a
    /// form that is too long always goes into tabs (S-L5).
    pub(super) fn collapsible<R>(
        &mut self,
        id_salt: impl Hash + std::fmt::Debug,
        title: impl Into<String>,
        add: impl FnOnce(&mut Form<'_>) -> R,
    ) -> Option<R> {
        let id = self
            .ui
            .make_persistent_id(("stokesforge_form_fold", id_salt));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            self.ui.ctx(),
            id,
            false,
        );
        let title = title.into();
        let accessible_title = title.clone();
        // Arrow and title are a single clickable area, in the labels column
        // like "Tick direction": no dead space between them, and
        // the title is not selectable (S-L8).
        let (toggled, width) = self
            .ui
            .horizontal(|ui| {
                let font = egui::TextStyle::Body.resolve(ui.style());
                let galley = ui
                    .painter()
                    .layout_no_wrap(title, font, ui.visuals().text_color());
                let icon = ui.spacing().icon_width;
                let gap = ui.spacing().icon_spacing;
                let size = egui::vec2(
                    icon + gap + galley.size().x,
                    galley.size().y.max(ui.spacing().interact_size.y),
                );
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &accessible_title)
                });
                let icon_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.left() + icon / 2.0, rect.center().y),
                    egui::vec2(icon, icon),
                );
                let icon_response =
                    ui.interact(icon_rect, response.id.with("icon"), egui::Sense::hover());
                let openness = if state.is_open() { 1.0 } else { 0.0 };
                egui::collapsing_header::paint_default_icon(ui, openness, &icon_response);
                let color = if response.hovered() {
                    ui.visuals().strong_text_color()
                } else {
                    ui.visuals().text_color()
                };
                ui.painter().galley(
                    egui::pos2(
                        rect.left() + icon + gap,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    color,
                );
                (response.clicked(), rect.width())
            })
            .inner;
        self.widest = self.widest.max(width);
        if toggled {
            state.toggle(self.ui);
        }
        let open = state.is_open();
        state.store(self.ui.ctx());
        open.then(|| add(self))
    }

    /// The indent of the fields column: the labels column and
    /// the gap between the two columns.
    fn indent(&self) -> f32 {
        if self.label_width > 0.0 {
            self.label_width + self.ui.spacing().item_spacing.x
        } else {
            0.0
        }
    }
}

/// How an exclusive choice is painted (S-C1, S-C2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChoiceLayout {
    /// 2 or 3 options with labels of at most 3 words: radios side by side.
    RadiosRow,
    /// 2 or 3 options of which a label exceeds 3 words: radios one under
    /// the other.
    RadiosColumn,
    /// 4 options or more: dropdown list.
    List,
}

/// The number of options from which a choice becomes a list (S-C1).
pub(super) const LIST_FROM: usize = 4;

/// The number of words beyond which the radios go one under the other (S-C2).
const RADIO_ROW_WORDS: usize = 3;

/// The decision of S-C1 and S-C2, pure: the shape of a choice from its labels.
pub(super) fn choice_layout<S: AsRef<str>>(labels: &[S]) -> ChoiceLayout {
    if labels.len() >= LIST_FROM {
        ChoiceLayout::List
    } else if labels
        .iter()
        .all(|label| label.as_ref().split_whitespace().count() <= RADIO_ROW_WORDS)
    {
        ChoiceLayout::RadiosRow
    } else {
        ChoiceLayout::RadiosColumn
    }
}

/// An option of a [`choice`]: its value, its label, and the optional
/// reminder placed as a "?" icon next to its radio (or as a bubble on its
/// list entry).
pub(super) struct ChoiceOption<T> {
    pub(super) value: T,
    pub(super) label: String,
    pub(super) hint: Option<String>,
    pub(super) disabled: Option<String>,
}

impl<T> ChoiceOption<T> {
    /// An option without a reminder.
    pub(super) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            hint: None,
            disabled: None,
        }
    }

    /// Greys out an option with the reason for the refusal shown on hover.
    pub(super) fn disabled(mut self, reason: Option<String>) -> Self {
        self.disabled = reason;
        self
    }

    /// The same option, with its reminder.
    pub(super) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// An exclusive choice among fixed options, painted per [`choice_layout`]:
/// radios (2–3 options) or dropdown list (4 and more). `same` says whether a
/// value is the current option (equality, or same variant when the option
/// carries a parameter). Returns `true` if the user picked a **different**
/// option, written into `value`.
pub(super) fn choice_by<T: Clone>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
    same: impl Fn(&T, &T) -> bool,
) -> bool {
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    let current = options.iter().position(|option| same(&option.value, value));
    let mut picked = None;
    match choice_layout(&labels) {
        ChoiceLayout::List => {
            let shown = current.map(|index| labels[index]).unwrap_or_default();
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        let mut entry = ui
                            .add_enabled(
                                option.disabled.is_none(),
                                egui::Button::selectable(
                                    current == Some(index),
                                    option.label.as_str(),
                                ),
                            )
                            .on_disabled_hover_text(option.disabled.as_deref().unwrap_or_default());
                        if let Some(hint) = &option.hint {
                            entry = entry.on_hover_text(hint);
                        }
                        if entry.clicked() {
                            picked = Some(index);
                        }
                    }
                });
        }
        layout => {
            let radios = |ui: &mut egui::Ui| {
                // A label that does not fit wraps (#335), it is not
                // clipped by the edge of the column.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.push_id(id_salt, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        let radio = |ui: &mut egui::Ui| {
                            ui.add_enabled(
                                option.disabled.is_none(),
                                egui::RadioButton::new(
                                    current == Some(index),
                                    option.label.as_str(),
                                ),
                            )
                            .on_disabled_hover_text(option.disabled.as_deref().unwrap_or_default())
                        };
                        let clicked = match &option.hint {
                            Some(hint) => help_beside(ui, hint.clone(), radio).clicked(),
                            None => radio(ui).clicked(),
                        };
                        if clicked {
                            picked = Some(index);
                        }
                    }
                });
            };
            if layout == ChoiceLayout::RadiosRow {
                ui.horizontal_wrapped(radios);
            } else {
                ui.vertical(radios);
            }
        }
    }
    match picked {
        Some(index) if current != Some(index) => {
            *value = options[index].value.clone();
            true
        }
        _ => false,
    }
}

/// [`choice_by`] for values comparable by equality.
pub(super) fn choice<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
) -> bool {
    choice_by(ui, id_salt, value, options, |a, b| a == b)
}

/// The corner radius of a text surrounded by [`toggle_texts`].
const TOGGLE_TEXT_RADIUS: u8 = 6;

/// **Surrounded texts** that act as toggle buttons (#270): a
/// rounded frame around each label, the current option as a solid background in
/// the accent color, the others as an outline only. A click on another
/// option makes it current (returns `true`); a click on the current one changes
/// nothing. It is an exclusive choice, but for the tool row of the analysis
/// preview, where radios would weigh down a bar that changes with the analysis;
/// everywhere else, [`choice`]. Painted by hand: egui's selected background
/// is reserved for menus (`no_selectable_button_outside_a_menu`).
pub(super) fn toggle_texts<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
) -> bool {
    let mut picked = None;
    ui.push_id(id_salt, |ui| {
        ui.horizontal(|ui| {
            for (index, option) in options.iter().enumerate() {
                let active = *value == option.value;
                let font = egui::TextStyle::Button.resolve(ui.style());
                // Color chosen at paint time (`galley` takes it as a fallback).
                let galley = ui.painter().layout_no_wrap(
                    option.label.clone(),
                    font,
                    egui::Color32::PLACEHOLDER,
                );
                let size = galley.size() + egui::vec2(16.0, 6.0);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        active,
                        option.label.as_str(),
                    )
                });
                let radius = egui::CornerRadius::same(TOGGLE_TEXT_RADIUS);
                let (text_color, outline) = if active {
                    ui.painter().rect_filled(rect, radius, theme::accent());
                    (theme::text_on_accent(), theme::accent())
                } else {
                    let line = if response.hovered() {
                        ui.visuals().widgets.hovered.bg_stroke
                    } else {
                        ui.visuals().widgets.inactive.bg_stroke
                    };
                    (ui.visuals().text_color(), line.color)
                };
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(1.0, outline),
                    egui::StrokeKind::Inside,
                );
                ui.painter()
                    .galley(rect.center() - galley.size() / 2.0, galley, text_color);
                if response.clicked() {
                    picked = Some(index);
                }
                if let Some(hint) = &option.hint {
                    response.on_hover_text(hint);
                }
            }
        });
    });
    match picked {
        Some(index) if options[index].value != *value => {
            *value = options[index].value.clone();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    use super::super::test_support::{click, context, image, texts};
    use super::{choice, choice_layout, form, toggle_texts, ChoiceLayout, ChoiceOption};
    use crate::theme;

    /// S-C1, S-C2: 2–3 options as radios, side by side as long as each label
    /// fits in 3 words; 4 and more as a list, whatever their length.
    #[test]
    fn the_shape_of_a_choice_follows_the_number_of_options_and_words() {
        assert_eq!(choice_layout(&["Dot", "Comma"]), ChoiceLayout::RadiosRow);
        assert_eq!(
            choice_layout(&["None", "Constant", "Linear"]),
            ChoiceLayout::RadiosRow
        );
        assert_eq!(
            choice_layout(&["One sheet per file", "Side by side"]),
            ChoiceLayout::RadiosColumn,
            "a label of 4 words puts the radios one under the other"
        );
        assert_eq!(
            choice_layout(&["a b c", "d e f"]),
            ChoiceLayout::RadiosRow,
            "3 words still fit on the row"
        );
        assert_eq!(choice_layout(&["a", "b", "c", "d"]), ChoiceLayout::List);
        assert_eq!(
            choice_layout(&["one two three four", "b", "c", "d"]),
            ChoiceLayout::List,
            "from 4 options, the list, even with a long label"
        );
    }

    /// S-L1: the fields column is one vertical, whatever the
    /// row's label; a checkbox without a label is placed there too. Witness:
    /// the labels, for their part, all start on the left, at different widths.
    #[test]
    fn the_fields_of_a_form_are_aligned_on_one_vertical() {
        let context = context();
        let mut checkbox = false;
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = image(&context, Vec::new(), |ui| {
                form(ui, "trial", |form| {
                    form.row("Short", |ui| ui.label("field one"));
                    form.row("A much longer label", |ui| ui.label("field two"));
                    form.checkbox(&mut checkbox, "checkbox", None);
                    form.note("note");
                });
            });
        }
        // The painted rectangle of a text (its galley placed on the screen).
        let rect = |needle: &str| {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == needle => {
                        Some(text.galley.rect.translate(text.pos.to_vec2()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{needle:?} non peint"))
        };
        let field_one = rect("field one").left();
        assert!((field_one - rect("field two").left()).abs() < 0.5);
        assert!(
            rect("checkbox").left() > field_one,
            "the checkbox is in the fields column (after its tick)"
        );
        assert!((rect("note").left() - field_one).abs() < 0.5);
        // Witness: the labels start from the same left edge, and the longest
        // pushes the fields column beyond its end.
        let long = rect("A much longer label");
        assert!((rect("Short").left() - long.left()).abs() < 0.5);
        assert!(field_one > long.right());
        assert!(rect("Short").right() < long.right());
    }

    /// A click on a radio picks its value; a click on the already
    /// picked option returns no change.
    #[test]
    fn a_click_on_a_radio_picks_its_option() {
        let context = context();
        let mut value = 1;
        let options = [ChoiceOption::new(1, "One"), ChoiceOption::new(2, "Two")];
        let change = std::cell::Cell::new(false);
        let paint = |events: Vec<egui::Event>, value: &mut i32| {
            texts(&image(&context, events, |ui| {
                change.set(choice(ui, "radios", value, &options));
            }))
        };
        let painted_texts = paint(Vec::new(), &mut value);
        let two = painted_texts
            .iter()
            .find(|t| t.text == "Two")
            .unwrap()
            .center;
        let click_event = |pressed| egui::Event::PointerButton {
            pos: two,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        paint(vec![egui::Event::PointerMoved(two)], &mut value);
        paint(vec![click_event(true)], &mut value);
        paint(vec![click_event(false)], &mut value);
        assert_eq!(value, 2);
        assert!(change.get());
        paint(vec![click_event(true)], &mut value);
        paint(vec![click_event(false)], &mut value);
        assert!(!change.get(), "the already picked option changes nothing");
    }
    /// #335: in a narrow column, radios side by side or one under
    /// the other wrap instead of overflowing (and being clipped).
    #[test]
    fn radios_in_a_narrow_column_do_not_overflow() {
        let context = context();
        let mut value = 1;
        let options = [
            ChoiceOption::new(1, "Mathematical Correction"),
            ChoiceOption::new(2, "Constant (point)"),
        ];
        let width = 120.0;
        let shapes = image(&context, Vec::new(), |ui| {
            ui.allocate_ui(egui::vec2(width, 400.0), |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        choice(ui, "narrow", &mut value, &options);
                    });
                });
            });
        });
        for shape in &shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                let right_edge = text.pos.x + text.galley.rect.right();
                assert!(
                    right_edge <= width + 1.0,
                    "{:?} overflows the column ({right_edge} > {width})",
                    text.galley.job.text
                );
            }
        }
    }

    /// The same refusal must hold in radios and in a dropdown list.
    #[test]
    fn a_greyed_out_option_cannot_be_picked_in_radios_or_in_a_list() {
        use super::super::test_support::click;
        for count in [2, 4] {
            let context = context();
            let mut value = 1;
            let options: Vec<_> = (1..=count)
                .map(|n| {
                    ChoiceOption::new(n, n.to_string()).disabled((n == 2).then(|| "refusal".into()))
                })
                .collect();
            let mut paint = |ui: &mut egui::Ui| {
                choice(ui, "refusal", &mut value, &options);
            };
            let mut painted = image(&context, vec![], &mut paint);
            if count == 4 {
                let position = texts(&painted)
                    .iter()
                    .find(|t| t.text == "1")
                    .unwrap()
                    .center;
                click(&context, position, egui::PointerButton::Primary, &mut paint);
                painted = image(&context, vec![], &mut paint);
            }
            let position = texts(&painted)
                .iter()
                .find(|t| t.text == "2")
                .unwrap()
                .center;
            click(&context, position, egui::PointerButton::Primary, &mut paint);
            assert_eq!(value, 1);
        }
    }

    /// #270: surrounded texts are an exclusive toggle choice. The
    /// current option has the solid accent background and its light text; a click on
    /// the other makes it current; a click on the current one changes nothing.
    #[test]
    fn surrounded_texts_toggle_and_mark_the_current_option() {
        let context = context();
        let value = std::cell::Cell::new(1);
        let changes = std::cell::Cell::new(0);
        let options = vec![ChoiceOption::new(1, "Point"), ChoiceOption::new(2, "Zone")];
        let mut paint = |ui: &mut egui::Ui| {
            let mut current = value.get();
            if toggle_texts(ui, "toggle", &mut current, &options) {
                changes.set(changes.get() + 1);
            }
            value.set(current);
        };
        let painted = image(&context, vec![], &mut paint);
        let filled = |painted: &[egui::epaint::ClippedShape]| {
            painted
                .iter()
                .filter(|p| matches!(&p.shape, egui::Shape::Rect(r) if r.fill == theme::accent()))
                .count()
        };
        assert_eq!(filled(&painted), 1, "only one option is solid");
        let text = |painted: &[egui::epaint::ClippedShape], needle: &str| {
            texts(painted)
                .into_iter()
                .find(|t| t.text == needle)
                .unwrap()
        };
        assert_eq!(text(&painted, "Point").color, theme::text_on_accent());
        assert_ne!(text(&painted, "Zone").color, theme::text_on_accent());
        let (point, zone) = (
            text(&painted, "Point").center,
            text(&painted, "Zone").center,
        );
        click(&context, point, egui::PointerButton::Primary, &mut paint);
        assert_eq!(
            (value.get(), changes.get()),
            (1, 0),
            "the current one: nothing changes"
        );
        click(&context, zone, egui::PointerButton::Primary, &mut paint);
        assert_eq!((value.get(), changes.get()), (2, 1));
        let painted = image(&context, vec![], &mut paint);
        assert_eq!(filled(&painted), 1);
        assert_eq!(
            text(&painted, "Zone").color,
            theme::text_on_accent(),
            "Zone is solid"
        );
    }
}
