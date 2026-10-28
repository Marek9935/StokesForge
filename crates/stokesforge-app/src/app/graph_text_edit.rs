//! In-place editors of a graph: the free text (issue #35, with its
//! color / size / rotation bar), the title and the legend name of a series
//! (issue #53), and the formatting requested by the format bar for the
//! field being edited (`pending_format_*`, `apply_pending_*`, #77/#78).
//!
//! Moved out of `app.rs` as is (#121).
//!
//! In-place editing and dragging do not touch the model while they
//! last: the preview lives in the view (`GraphView::set_annotation_preview`),
//! like the zoom, and validation sends **one** `Action::SetAnnotation` —
//! one history entry, Escape throws the preview away. The editor state is
//! stored in egui's memory under the view's identifier: the same
//! implementation serves the floating window and the detached window, without
//! passing anything through `AppState`.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::{GraphView, TextTarget};

use crate::color_picker::{ColorUsage, PickerEnv};
use crate::number_field::number_field;
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{series_list_label, Action};
use stokesforge_app_state::tr;

use super::inplace_edit;
use crate::color_picker::{saved_colors_id, SavedColorRequest};

/// The in-place editor of a free text, open on a view.
#[derive(Debug, Clone)]
pub(super) struct AnnotationEditor {
    pub(super) target: stokesforge_render_egui::AnnotationRef,
    pub(super) draft: stokesforge_render_egui::AnnotationSpec,
    /// The text as it is in the model; `None` for a new text.
    pub(super) original: Option<stokesforge_render_egui::AnnotationSpec>,
    /// Give the field focus on the next frame (opening).
    pub(super) focus: bool,
    /// The last center and size (screen) of the straight text rendered:
    /// an emptied text is no longer plotted, but the field must stay where it is.
    pub(super) anchor: Option<(egui::Pos2, egui::Vec2)>,
}

/// A title or a legend entry targeted by the right click (issue #53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextText {
    /// A title, an axis title or the whole legend.
    Title(TextTarget),
    /// The legend row of series `series` of layer `panel`.
    LegendEntry { panel: usize, series: usize },
}

/// What the in-place editor of a title modifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TextEditTarget {
    /// The entered text of a title.
    Title(TextTarget),
    /// The name, in the legend, of a series.
    LegendEntry(stokesforge_plot::SeriesRef),
    /// The name of a series group (#206).
    GroupName(stokesforge_plot::SeriesGroupRef),
}

/// The in-place editor of a title or a legend name (issue #53): a
/// field placed over the text. Enter or a click elsewhere validates, Escape cancels —
/// like the free text editor, without a settings bar (color and
/// size of a title are set in the graph properties).
#[derive(Debug, Clone)]
pub(super) struct TextEditor {
    pub(super) target: TextEditTarget,
    pub(super) buffer: String,
    /// The text as it was at opening: validating without having changed it
    /// sends nothing (and does not freeze a name derived from the column).
    original: String,
    /// Give the field focus on the next frame (opening).
    focus: bool,
    /// Center and size (screen) of the text plotted at opening.
    anchor: (egui::Pos2, egui::Vec2),
}

/// The formatting requested by the bar, waiting for the field being edited.
pub(super) fn pending_format_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_text_format")
}

/// The font requested by the bar for the free text being edited: it
/// applies to the editor's draft, not to the content (issue #78).
pub(super) fn pending_zone_font_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_zone_font")
}

/// Sets on `font` the font requested by the format bar, if there is one.
fn apply_pending_zone_font(ctx: &egui::Context, font: &mut String) {
    if let Some(Some(name)) =
        ctx.data_mut(|data| data.remove_temp::<Option<String>>(pending_zone_font_id()))
    {
        *font = name;
    }
}

/// Applies the formatting requested by the format bar to `buffer`, according to
/// the selection that field `field` kept (egui keeps it when the
/// field loses focus on the click on the bar), then gives the field focus back with
/// the selection inside the tags.
fn apply_pending_format(ctx: &egui::Context, field: egui::Id, buffer: &mut String) {
    use egui::text::{CCursor, CCursorRange};
    let Some(format) = ctx
        .data_mut(|data| {
            data.remove_temp::<Option<stokesforge_app_state::text_format::TextFormat>>(
                pending_format_id(),
            )
        })
        .flatten()
    else {
        return;
    };
    let mut state = egui::TextEdit::load_state(ctx, field).unwrap_or_default();
    let length = buffer.chars().count();
    // egui shrinks the selection of a field painted without focus (#242): we
    // take the one the field had while it had it.
    let kept = ctx.data(|data| data.get_temp::<SelectionKept>(field.with("selection_kept")));
    let selection = kept.map(|SelectionKept(range)| range).unwrap_or_else(|| {
        state.cursor.char_range().map_or(length..length, |range| {
            let [start, end] = range.sorted_cursors();
            start.index.0..end.index.0
        })
    });
    let selection = selection.start.min(length)..selection.end.min(length);
    let done = stokesforge_app_state::text_format::apply(buffer, selection, &format);
    *buffer = done.text;
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(done.selection.start),
        CCursor::new(done.selection.end),
    )));
    state.store(ctx, field);
    ctx.data_mut(|data| {
        data.insert_temp(
            field.with("selection_kept"),
            SelectionKept(done.selection.clone()),
        )
    });
    ctx.memory_mut(|memory| memory.request_focus(field));
}

/// The last selection of the field when it had focus, in characters.
#[derive(Clone)]
struct SelectionKept(std::ops::Range<usize>);

/// Keeps the selection of field `field` if it has focus this frame, so
/// that the next click on the format bar finds it again (see
/// [`apply_pending_format`]).
fn keep_selection(ctx: &egui::Context, field: egui::Id) {
    if !ctx.memory(|memory| memory.has_focus(field)) {
        return;
    }
    let Some(range) = egui::TextEdit::load_state(ctx, field).and_then(|s| s.cursor.char_range())
    else {
        return;
    };
    let [start, end] = range.sorted_cursors();
    ctx.data_mut(|data| {
        data.insert_temp(
            field.with("selection_kept"),
            SelectionKept(start.index.0..end.index.0),
        )
    });
}

pub(super) fn text_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("text_editor")
}

/// Opens the editor on the text of a title; no effect if the title is
/// not plotted or no longer exists (the legend has no text of its own).
pub(super) fn open_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    target: TextTarget,
) {
    let Some(original) = stokesforge_core::compose::current_plot_spec(tree, graph)
        .and_then(|spec| target.text(&spec))
    else {
        return;
    };
    let Some((center, size, _)) = view.text_screen_box(target) else {
        return;
    };
    start_text_editor(
        ctx,
        view,
        TextEditTarget::Title(target),
        original,
        (center, size),
    );
}

/// Opens the editor on the name, in the legend, of a series.
pub(super) fn open_legend_entry_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    panel: usize,
    series: usize,
) {
    let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) else {
        return;
    };
    let address = stokesforge_plot::SeriesRef::new(panel, series);
    let Some(spec_series) = spec.series_at(address) else {
        return;
    };
    let Some(anchor) = view.legend_entry_screen_box(panel, series) else {
        return;
    };
    let original = stokesforge_app_state::state::series_list_label(tree, spec_series);
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(address),
        original,
        anchor,
    );
}

pub(super) fn open_series_name_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
) {
    let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) else {
        return;
    };
    let Some(value) = spec.series_at(series) else {
        return;
    };
    let anchor = view
        .legend_entry_screen_box(series.panel, series.index)
        .unwrap_or((
            view.display_rect()
                .map_or(ctx.content_rect().center(), |r| r.center()),
            egui::vec2(180.0, 24.0),
        ));
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(series),
        series_list_label(tree, value),
        anchor,
    );
}

/// Opens the editor on the name of a group, placed on the legend entry of
/// its first member (#206): F2 on a group selected in the graph
/// works even when the Object Manager is hidden or collapsed.
pub(super) fn open_group_name_editor_in_graph(
    ctx: &egui::Context,
    view: &GraphView,
    spec: &stokesforge_plot::PlotSpec,
    graph_group: stokesforge_plot::SeriesGroupRef,
) {
    let Some(panel) = spec.panels.get(graph_group.panel) else {
        return;
    };
    let Some(first) = panel.group_members(graph_group.group).first().copied() else {
        return;
    };
    let anchor = view
        .legend_entry_screen_box(graph_group.panel, first)
        .unwrap_or((
            view.display_rect()
                .map_or(ctx.content_rect().center(), |r| r.center()),
            egui::vec2(180.0, 24.0),
        ));
    start_text_editor(
        ctx,
        view,
        TextEditTarget::GroupName(graph_group),
        stokesforge_app_state::object_manager::group_title(panel, graph_group.group),
        anchor,
    );
}

fn start_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    target: TextEditTarget,
    original: String,
    anchor: (egui::Pos2, egui::Vec2),
) {
    let editor = TextEditor {
        target,
        buffer: original.clone(),
        original,
        focus: true,
        anchor,
    };
    ctx.data_mut(|data| data.insert_temp(text_editor_id(view), editor));
}

/// Paints the editor of a title or a legend name, and translates its
/// validation into **one** action.
pub(super) fn text_editor_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    mut editor: TextEditor,
    actions: &mut Vec<Action>,
) {
    let id = text_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.buffer);
    let (center, size) = editor.anchor;
    let field = egui::Rect::from_center_size(
        center,
        egui::vec2((size.x + 32.0).max(180.0), (size.y + 8.0).max(24.0)),
    );
    let mut accept = false;
    let mut cancelled = false;
    let field_id = id.with("field");
    let area = egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.min)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                // The common contract: all the text selected on opening,
                // Enter and Tab validate, Escape cancels. The "click elsewhere"
                // stays decided further down (`outside`), the field lives in a
                // separate layer.
                let shown = inplace_edit::show(
                    ui,
                    field_id,
                    egui::TextEdit::singleline(&mut editor.buffer)
                        .desired_width(field.width())
                        .hint_text(tr!("plot-text-placeholder")),
                    inplace_edit::Opening::SelectAll,
                    inplace_edit::FocusLoss::Ignored,
                    None,
                );
                keep_selection(ui.ctx(), field_id);
                editor.focus = false;
                match shown.end {
                    Some(
                        inplace_edit::EditEnd::Enter { .. } | inplace_edit::EditEnd::Tab { .. },
                    ) => accept = true,
                    Some(inplace_edit::EditEnd::Cancel) => cancelled = true,
                    _ => {}
                }
            });
        });
    let cancel =
        cancelled || ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ESCAPE));
    let outside = ctx
        .input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        })
        .is_some_and(|pos| {
            !area.response.rect.contains(pos)
                && !crate::toolbar::over_format_bar(ctx, pos)
                && ctx
                    .layer_id_at(pos)
                    .is_none_or(|layer| layer.order != egui::Order::Foreground)
        });
    if cancel {
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    if accept || outside {
        if editor.buffer != editor.original {
            actions.push(match editor.target {
                TextEditTarget::Title(target) => Action::SetTextContent {
                    graph,
                    target,
                    text: editor.buffer.clone(),
                },
                TextEditTarget::LegendEntry(series) => Action::SetLegendEntryName {
                    graph,
                    series,
                    name: editor.buffer.clone(),
                },
                TextEditTarget::GroupName(group) => Action::RenameSeriesGroup {
                    graph,
                    group,
                    name: editor.buffer.clone(),
                },
            });
        }
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

pub(super) fn annotation_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("annotation_editor")
}

/// Opens the editor on an existing text; no effect if the text no longer
/// exists in the model.
pub(super) fn open_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    target: stokesforge_render_egui::AnnotationRef,
) {
    let Some(original) = stokesforge_core::compose::current_plot_spec(tree, graph)
        .and_then(|spec| target.get(&spec).cloned())
    else {
        return;
    };
    start_annotation_editor(ctx, view, target, original.clone(), Some(original));
}

/// Adds a text at data point `(panel, x, y)` and opens
/// the editor on it: nothing enters the model before validation.
pub(super) fn add_annotation_at(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    (panel, x, y): (usize, f64, f64),
) {
    let Some(target) = stokesforge_core::compose::current_plot_spec(tree, graph)
        .and_then(|spec| stokesforge_render_egui::AnnotationRef::next_in(&spec, panel))
    else {
        return;
    };
    let draft = stokesforge_render_egui::AnnotationSpec::new(String::new(), x, y);
    start_annotation_editor(ctx, view, target, draft, None);
}

fn start_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    target: stokesforge_render_egui::AnnotationRef,
    draft: stokesforge_render_egui::AnnotationSpec,
    original: Option<stokesforge_render_egui::AnnotationSpec>,
) {
    view.set_annotation_preview(Some((target, draft.clone())));
    let editor = AnnotationEditor {
        target,
        draft,
        original,
        focus: true,
        anchor: None,
    };
    ctx.data_mut(|data| data.insert_temp(annotation_editor_id(view), editor));
}

/// Paints the in-place editor: the text field placed over the text, and
/// above it a color / size / rotation bar. Enter or a click elsewhere
/// validates, Escape cancels.
pub(super) fn annotation_editor_ui(
    ctx: &egui::Context,
    view: &mut GraphView,
    graph: NodeId,
    mut editor: AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let id = annotation_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.draft.text);
    apply_pending_zone_font(ctx, &mut editor.draft.font);
    // The frame of the rendered text; failing that (new text, or emptied hence no longer
    // plotted), the last known, then the center of the figure.
    if let Some((center, size, _)) = view.annotation_screen_box(editor.target) {
        editor.anchor = Some((center, size));
    }
    let (center, size) = editor.anchor.unwrap_or_else(|| {
        let center = view
            .display_rect()
            .map_or(egui::Pos2::ZERO, |rect| rect.center());
        (center, egui::Vec2::ZERO)
    });
    // The field is written in the render frame (#37): centered, at the body size of the
    // text, straight even if the text is rotated.
    let field = stokesforge_render_egui::annotation_field(
        center,
        size,
        editor.draft.effective_size(),
        view.display_scale().unwrap_or(1.0),
    );

    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };

    let mut accept = false;
    let mut cancelled = false;
    let toolbar = egui::Area::new(id.with("toolbar"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min - egui::vec2(0.0, 36.0))
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::color_picker::color_icon_button(
                        ui,
                        (crate::icons::Icon::TextColor, &tr!("format-color")),
                        ("annotation_color", id),
                        &mut editor.draft.color,
                        ColorUsage::Text,
                        Some(&tr!("plot-color-auto-contrast")),
                        &mut env,
                    );
                    ui.label(tr!("plot-text-size"));
                    number_field(
                        ui,
                        ("annotation_size", id),
                        &mut editor.draft.size,
                        &quantities::TEXT_SIZE,
                    );
                    ui.label(tr!("plot-text-rotation"));
                    number_field(
                        ui,
                        ("annotation_rotation", id),
                        &mut editor.draft.rotation,
                        &quantities::ROTATION,
                    );
                    // The alignment is only visible in a box of fixed
                    // width (handles of the selected text, #244); same
                    // buttons as the format bar (#246).
                    let mut requests = Vec::new();
                    crate::toolbar::align_buttons(
                        ui,
                        Some(editor.draft.align),
                        None,
                        &mut requests,
                    );
                    for request in requests {
                        if let crate::toolbar::ToolbarRequest::FormatText(
                            stokesforge_app_state::text_format::TextFormat::Align(align),
                        ) = request
                        {
                            editor.draft.align = align;
                        }
                    }
                    if ui
                        .button(tr!("plot-text-ok"))
                        .on_hover_text(tr!("plot-text-ok", "hint"))
                        .clicked()
                    {
                        accept = true;
                    }
                });
            });
        });
    // While typing, the straight field covers the text; as soon as it loses
    // focus (the rotation or the size is being set), it becomes transparent and
    // the — rotated — preview shows (#36).
    let field_id = id.with("field");
    let typing = editor.focus || ctx.memory(|memory| memory.has_focus(field_id));
    let text = egui::Area::new(id.with("text"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min)
        .constrain(true)
        .show(ctx, |ui| {
            let visuals = ui.visuals().clone();
            let outline = if typing {
                visuals.selection.stroke
            } else {
                visuals.widgets.inactive.bg_stroke
            };
            if typing {
                ui.painter()
                    .rect_filled(field.rect, 2.0, visuals.extreme_bg_color);
            }
            ui.painter()
                .rect_stroke(field.rect, 2.0, outline, egui::StrokeKind::Inside);
            let padding = (field.font_size * 0.3).round() as i8;
            let fresh = inplace_edit::begin(ui.ctx(), field_id);
            let response = ui.put(
                field.rect,
                egui::TextEdit::singleline(&mut editor.draft.text)
                    .id(field_id)
                    .event_filter(inplace_edit::KEYS)
                    .font(egui::FontId::proportional(field.font_size))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::same(padding))
                    .desired_width(field.rect.width())
                    .horizontal_align(egui::Align::Center)
                    .vertical_align(egui::Align::Center)
                    .text_color(if typing {
                        visuals.text_color()
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .hint_text(tr!("plot-text-placeholder")),
            );
            inplace_edit::measured(ui.ctx(), field_id, &editor.draft.text);
            keep_selection(ui.ctx(), field_id);
            // The common contract, except the click elsewhere: the editor carries
            // settings (size, rotation, color) that take focus without
            // ending the input, `outside` decides (further down).
            let end = inplace_edit::finish(
                ui,
                &response,
                fresh,
                inplace_edit::Opening::SelectAll,
                inplace_edit::FocusLoss::Ignored,
                None,
            );
            editor.focus = false;
            match end {
                Some(inplace_edit::EditEnd::Enter { .. } | inplace_edit::EditEnd::Tab { .. }) => {
                    accept = true;
                }
                Some(inplace_edit::EditEnd::Cancel) => cancelled = true,
                _ => {}
            }
        });

    actions.extend(env.requests.into_iter().map(SavedColorRequest::into_action));

    // Escape: the open color picker consumes it first for itself.
    let cancel =
        cancelled || ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ESCAPE));
    // A click outside the editor (and outside any popup layer, such as
    // the color picker) validates.
    let pressed_at = ctx.input(|input| {
        input
            .pointer
            .any_pressed()
            .then(|| input.pointer.interact_pos())
            .flatten()
    });
    let outside = pressed_at.is_some_and(|pos| {
        !toolbar.response.rect.contains(pos)
            && !text.response.rect.contains(pos)
            && !crate::toolbar::over_format_bar(ctx, pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order != egui::Order::Foreground)
    });

    if cancel {
        view.set_annotation_preview(None);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    if accept || outside {
        commit_annotation_editor(view, graph, &editor, actions);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    view.set_annotation_preview(Some((editor.target, editor.draft.clone())));
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

/// Validates the editor: one action, and only if something
/// changed. An emptied text is removed (an empty text is not plotted, it
/// could no longer be designated).
fn commit_annotation_editor(
    view: &mut GraphView,
    graph: NodeId,
    editor: &AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let empty = editor.draft.text.trim().is_empty();
    match (&editor.original, empty) {
        (None, true) => view.set_annotation_preview(None),
        (Some(_), true) => {
            actions.push(Action::RemoveAnnotation {
                graph,
                target: editor.target,
            });
            view.hold_preview();
        }
        (Some(original), false) if *original == editor.draft => {
            view.set_annotation_preview(None);
        }
        _ => {
            actions.push(Action::SetAnnotation {
                graph,
                target: editor.target,
                annotation: Box::new(editor.draft.clone()),
            });
            view.hold_preview();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::text::{CCursor, CCursorRange};
    use stokesforge_app_state::text_format::TextFormat;
    use stokesforge_plot::SeriesRef;
    use stokesforge_render_egui::{AnnotationRef, AnnotationSpec};

    fn context_with_selection(field: egui::Id, range: std::ops::Range<usize>) -> egui::Context {
        let ctx = egui::Context::default();
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(range.start),
            CCursor::new(range.end),
        )));
        state.store(&ctx, field);
        ctx
    }

    /// #63: the formatting requested by the bar applies to the highlight
    /// that the field kept, then the selection is placed within the tags.
    #[test]
    fn the_pending_formatting_applies_to_the_field_highlight() {
        let field = egui::Id::new("field");
        let ctx = context_with_selection(field, 4..7);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Bold)));
        let mut buffer = "Abs 280 nm".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
        let state = egui::text_edit::TextEditState::load(&ctx, field).unwrap();
        let [start, end] = state.cursor.char_range().unwrap().sorted_cursors();
        assert_eq!((start.index.0, end.index.0), (12, 15));
        assert!(
            ctx.memory(|memory| memory.has_focus(field)),
            "the field regains focus after the click on the bar"
        );
        // The request is consumed: a second frame does not replay it.
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
    }

    #[test]
    fn without_a_highlight_the_cursor_receives_the_tags() {
        let field = egui::Id::new("field");
        let ctx = context_with_selection(field, 3..3);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Subscript)));
        let mut buffer = "H2O".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O#sub[]");
    }

    /// With no request pending, the field text stays as it is (the witness of the
    /// two previous tests).
    #[test]
    fn without_a_pending_request_the_text_stays_as_it_is() {
        let field = egui::Id::new("field");
        let ctx = context_with_selection(field, 0..3);
        let mut buffer = "H2O".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O");
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), None::<TextFormat>));
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O");
    }

    /// #78: the font requested by the bar is set once on the draft
    /// of the free text, then the request is consumed.
    #[test]
    fn the_pending_font_is_set_once_on_the_draft() {
        let ctx = egui::Context::default();
        let mut font = "Fira Sans".to_owned();
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Fira Sans", "nothing pending, nothing changes");
        ctx.data_mut(|data| {
            data.insert_temp(pending_zone_font_id(), Some("Libertinus Serif".to_owned()))
        });
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Libertinus Serif");
        font = "Fira Sans".to_owned();
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Fira Sans", "the request is consumed");
    }

    fn editor(
        target: AnnotationRef,
        draft: AnnotationSpec,
        original: Option<AnnotationSpec>,
    ) -> AnnotationEditor {
        AnnotationEditor {
            target,
            draft,
            original,
            focus: false,
            anchor: None,
        }
    }

    /// Validating the editor of a free text sends only one action, and
    /// only if something changed; an emptied text is removed.
    #[test]
    fn validating_a_free_text_sends_one_action_and_only_if_it_changed() {
        let graph = NodeId::from_raw(5);
        let target = AnnotationRef { panel: 0, index: 0 };
        let pic = AnnotationSpec::new("Peak", 1.0, 2.0);
        let blank = AnnotationSpec::new("   ", 1.0, 2.0);
        let renamed = AnnotationSpec::new("Peak 2", 1.0, 2.0);
        for (case, draft, original, expected) in [
            ("new and empty", blank.clone(), None, vec![]),
            (
                "new",
                pic.clone(),
                None,
                vec![Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(pic.clone()),
                }],
            ),
            (
                "emptied",
                blank.clone(),
                Some(pic.clone()),
                vec![Action::RemoveAnnotation { graph, target }],
            ),
            ("unchanged", pic.clone(), Some(pic.clone()), vec![]),
            (
                "modified",
                renamed.clone(),
                Some(pic.clone()),
                vec![Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(renamed.clone()),
                }],
            ),
        ] {
            let mut view = GraphView::new();
            view.set_annotation_preview(Some((target, draft.clone())));
            let mut actions = Vec::new();
            commit_annotation_editor(
                &mut view,
                graph,
                &editor(target, draft, original),
                &mut actions,
            );
            assert_eq!(actions, expected, "{case}");
            // With no plotted figure (no revision to wait for), the preview
            // is cleared in all cases.
            assert_eq!(view.annotation_preview(), None, "{case}");
        }
    }

    /// A frame of the title editor open on `view`, if there is one.
    fn run_text_editor(
        ctx: &egui::Context,
        view: &GraphView,
        graph: NodeId,
        events: Vec<egui::Event>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let editor: Option<TextEditor> =
                ui.ctx().data(|data| data.get_temp(text_editor_id(view)));
            if let Some(editor) = editor {
                text_editor_ui(ui.ctx(), view, graph, editor, &mut actions);
            }
        });
        output.textures_delta.clear();
        actions
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_far() -> Vec<egui::Event> {
        let pos = egui::pos2(700.0, 500.0);
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    /// The in-place editor of a title or a legend name (#53): it stays
    /// open as long as nothing is done, Escape closes it without writing anything, Enter
    /// or a click elsewhere validates — through an action, and only if the text
    /// changed.
    #[test]
    fn the_title_editor_validates_on_enter_or_a_click_elsewhere_and_cancels_on_escape() {
        let graph = NodeId::from_raw(5);
        let view = GraphView::new();
        let id = text_editor_id(&view);
        let open = |ctx: &egui::Context, target: TextEditTarget, original: &str| {
            start_text_editor(
                ctx,
                &view,
                target,
                original.to_owned(),
                (egui::pos2(200.0, 200.0), egui::vec2(80.0, 20.0)),
            );
        };
        let type_text = |ctx: &egui::Context, text: &str| {
            ctx.data_mut(|data| {
                let mut editor: TextEditor = data.get_temp(id).unwrap();
                editor.buffer = text.to_owned();
                data.insert_temp(id, editor);
            });
        };
        let open_now =
            |ctx: &egui::Context| ctx.data(|data| data.get_temp::<TextEditor>(id)).is_some();
        let title = TextEditTarget::Title(TextTarget::Title);

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectrum");
        assert!(run_text_editor(&ctx, &view, graph, Vec::new()).is_empty());
        assert!(open_now(&ctx), "without a gesture, the editor stays open");
        type_text(&ctx, "Other");
        assert!(run_text_editor(&ctx, &view, graph, vec![key(egui::Key::Escape)]).is_empty());
        assert!(
            !open_now(&ctx),
            "Escape closes the editor without writing anything"
        );

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectrum");
        run_text_editor(&ctx, &view, graph, Vec::new());
        assert!(
            run_text_editor(&ctx, &view, graph, click_far()).is_empty(),
            "validating an unchanged text sends nothing"
        );
        assert!(!open_now(&ctx));

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectrum");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Absorbance");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, click_far()),
            vec![Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: "Absorbance".to_owned(),
            }]
        );
        assert!(!open_now(&ctx));

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectrum");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Absorbance");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, vec![key(egui::Key::Enter)]),
            vec![Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: "Absorbance".to_owned(),
            }],
            "Enter validates too"
        );

        let ctx = egui::Context::default();
        let series = SeriesRef::new(0, 1);
        open(&ctx, TextEditTarget::LegendEntry(series), "B");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Control");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, click_far()),
            vec![Action::SetLegendEntryName {
                graph,
                series,
                name: "Control".to_owned(),
            }]
        );
    }

    /// The editor of a free text (#35): the preview shows the draft, Escape
    /// throws it away without writing anything, a click elsewhere validates through an action.
    #[test]
    fn the_free_text_editor_validates_on_a_click_elsewhere_and_throws_the_preview_away_on_escape() {
        let graph = NodeId::from_raw(5);
        let target = AnnotationRef { panel: 0, index: 0 };
        let draft = AnnotationSpec::new("Peak", 1.0, 2.0);
        let run = |ctx: &egui::Context, view: &mut GraphView, events: Vec<egui::Event>| {
            let mut actions = Vec::new();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let editor: Option<AnnotationEditor> = ui
                    .ctx()
                    .data(|data| data.get_temp(annotation_editor_id(view)));
                if let Some(editor) = editor {
                    annotation_editor_ui(ui.ctx(), view, graph, editor, &mut actions);
                }
            });
            output.textures_delta.clear();
            actions
        };
        let open_now = |ctx: &egui::Context, view: &GraphView| {
            ctx.data(|data| data.get_temp::<AnnotationEditor>(annotation_editor_id(view)))
                .is_some()
        };

        let ctx = egui::Context::default();
        let mut view = GraphView::new();
        start_annotation_editor(&ctx, &mut view, target, draft.clone(), None);
        assert_eq!(
            view.annotation_preview(),
            Some(&(target, draft.clone())),
            "the preview shows the draft before any validation"
        );
        assert!(run(&ctx, &mut view, Vec::new()).is_empty());
        assert!(open_now(&ctx, &view));
        assert!(run(&ctx, &mut view, vec![key(egui::Key::Escape)]).is_empty());
        assert_eq!(
            view.annotation_preview(),
            None,
            "Escape throws the preview away"
        );
        assert!(!open_now(&ctx, &view));

        let ctx = egui::Context::default();
        let mut view = GraphView::new();
        start_annotation_editor(&ctx, &mut view, target, draft.clone(), None);
        run(&ctx, &mut view, Vec::new());
        assert_eq!(
            run(&ctx, &mut view, click_far()),
            vec![Action::SetAnnotation {
                graph,
                target,
                annotation: Box::new(draft.clone()),
            }]
        );
        assert!(!open_now(&ctx, &view));
    }
}

#[cfg(test)]
mod selection_between_two_formats {
    use super::*;
    use stokesforge_app_state::text_format::TextFormat;

    fn image(ctx: &egui::Context, field: egui::Id, buffer: &mut String, blur: bool) {
        let _ = crate::app::test_support::image(ctx, vec![], |ui| {
            if blur {
                ui.ctx().memory_mut(|m| m.surrender_focus(field));
            }
            apply_pending_format(ui.ctx(), field, buffer);
            let _ = inplace_edit::show(
                ui,
                field,
                egui::TextEdit::singleline(buffer),
                inplace_edit::Opening::SelectAll,
                inplace_edit::FocusLoss::Ignored,
                None,
            );
            keep_selection(ui.ctx(), field);
        });
    }

    /// #242: B then I on the same selection — egui shrinks the selection of a field
    /// painted without focus, so the field retains it itself.
    #[test]
    fn the_selection_survives_two_formats() {
        let ctx = crate::app::test_support::context();
        let field = egui::Id::new("field");
        let mut buffer = "λ (nm)".to_owned();
        for _ in 0..3 {
            image(&ctx, field, &mut buffer, false);
        }
        for format in [TextFormat::Bold, TextFormat::Italic] {
            // The click on the bar: the field loses focus, the request is set.
            image(&ctx, field, &mut buffer, true);
            ctx.data_mut(|d| d.insert_temp(pending_format_id(), Some(format)));
            image(&ctx, field, &mut buffer, false);
            image(&ctx, field, &mut buffer, false);
        }
        assert_eq!(buffer, "#strong[#emph[λ (nm)]]");
    }
}
