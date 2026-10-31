//! Graph properties window: tabs (theme, axes, series, legend),
//! editors per family, the Typst formula reminder, the labels of the choices, and the
//! window for a column's metadata (same dialog family).
//! Entry points for `app.rs`: [`plot_settings_window`] and
//! [`column_metadata_window`].
//!
//! Moved out of `app.rs` (#119), then split by page (#124). The page
//! functions borrow only the draft they need and return the commit
//! request; the common body keeps the child Uis, their identifiers and
//! the order commit → saved colors → page/axis selection.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{
    AxisSide, AxisSpec, LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, SymbolShape,
    TickDirection,
};

use crate::color_picker::{ColorUsage, PickerEnv, SavedColorEdit};
use crate::field_state::{self, FieldIssue};
use crate::icons::ICON_SIZE;
use crate::number_field::{auto_number_field, format_value, number_field};
use crate::theme::{table_header, FieldWidth};
use stokesforge_app_state::cell_sides::{self, PLACES};
use stokesforge_app_state::frame_sizes::{
    free_fields, set_fixed_in_draft, set_free_field_in_draft, set_offset_in_draft,
    set_placement_in_draft, set_size_in_draft,
};
use stokesforge_app_state::length_unit::LengthUnit;
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{
    parse_x_prime, Action, AppState, PlotAxis, PlotSettings, PlotSettingsTab, SaveTarget,
};
use stokesforge_app_state::tr;
use stokesforge_core::compose::{FigureOffset, FrameAxis, FreeField};
use stokesforge_plot::{LayerPlacement, FALLBACK_FRAME};
use stokesforge_render_egui::FigureLayout;

use super::curve_settings::curve_settings_page;
use super::dialogs::{
    modal_dialog, modal_read_only, DialogOutcome, InteractiveSize, PlainSize, SizeClass,
};
use super::form::{choice, form, ChoiceOption, Form};
use super::graph_window::{published_axis_bounds, PanelBounds};
use super::hover_help::{help_text, hover_help};
use super::tab_strip::{tab_strip, tab_strip_left_with_reasons, TabStripSide};

/// Key, in the egui context, of "the formula help icon is hidden" (#148).
const FORMULA_HELP_HIDDEN: &str = "stokesforge_formula_help_hidden";

/// Properties window of a graph: a floating dialog (same family
/// as `import_window`), opened by the right-click on a component of the
/// graph or by the "Graph" menu, and organized into thematic tabs
/// ([`PlotSettingsTab`]). It replaces the former fixed panel on the right of
/// the workspace, which took up room permanently as soon as a
/// graph was active.
///
/// Edits the [`stokesforge_plot::PlotSpec`] through a transient draft
/// ([`stokesforge_app_state::state::PlotEdit`]), never directly.
///
/// **History anti-inflation strategy**: each widget updates the
/// draft live (immediate visual feedback), and `commit` — a simple
/// local boolean, set to `true` by any widget that has just finished
/// an interaction — triggers [`AppState::commit_plot_edit`] only
/// once, at the end of the function. Three families of widgets, three
/// ways to finish an interaction:
/// - **text** (titles, labels): `lost_focus()`,
///   like `note_contents`/`cell_widget` — never on each keystroke;
/// - **number** (line width, marker size, axis bounds, building block
///   [`crate::number_field`]): `NumberResponse::changed` — Enter, leaving the
///   field, arrow, wheel notch or choice in the list;
/// - **checkbox / dropdown menu**: a single discrete event per
///   click, committed right away.
///
/// Closing the window commits whatever still differs from the model
/// ([`AppState::close_plot_settings`]).
///
/// **A single panel, no multi-panel**: only `PlotSpec::panels[0]`
/// is editable here. Editing the first panel amounts, for now, to
/// editing *the* graph.
pub(super) fn plot_settings_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(settings) = state.plot_settings() else {
        return;
    };
    // The preference of the formula help icon, read by `formula_help` (the fields
    // are painted far from the state).
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(FORMULA_HELP_HIDDEN),
            !state.show_formula_help(),
        );
    });
    // Cloned before borrowing `state` mutably for the draft: a cloned
    // `Arc<Tree>` is a reference counter, not a copy.
    let tree = state.tree().clone();
    let Some(node) = tree.node(settings.graph) else {
        state.cancel_plot_settings();
        return;
    };
    // The series have their own window (#179): same draft, other title,
    // other size, other identity (hence its own place on screen).
    let curves = settings.tab == PlotSettingsTab::Series;
    let (title, id, size) = if curves {
        (
            tr!("curve-window-title", name = node.name()),
            "stokesforge_curve_settings",
            // The series (#179): a list and an editor side by side.
            InteractiveSize::Class(SizeClass::Large),
        )
    } else {
        (
            tr!("plot-window-title", name = node.name()),
            "stokesforge_plot_settings",
            // The window is then freely resizable, including taller
            // than its content (see [`plot_settings_contents`]).
            InteractiveSize::Class(SizeClass::Medium),
        )
    };
    state.begin_plot_edit(settings.graph);
    // The window opens on the active layer of the graph (#398).
    state.init_plot_edit_panel(
        settings.graph,
        super::graph_window::published_active_panel(ctx, settings.graph),
    );
    let outcome = modal_dialog(ctx, egui::Id::new(id), &title, size, |ui| {
        plot_settings_contents(ui, state, &tree, settings)
    });
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => state.close_plot_settings(),
        DialogOutcome::Cancel => state.cancel_plot_settings(),
    }
}

/// Window to consult the file metadata of a column (issue
/// #5): the key / value rows, sections included, read-only. Same
/// modal building block as the graph properties, at the foot of a read-only
/// window: a single button, Close (S-F2, #228). Only write: the
/// "Use as x'" button of a numeric value, which writes it into the
/// x' row of the column (#164).
pub(super) fn column_metadata_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(column) = state.column_metadata_view() else {
        return;
    };
    let meta = state.column_meta(column);
    let lines = meta.file_metadata;
    let title = if meta.long_name.is_empty() {
        tr!("workbook-metadata-title")
    } else {
        tr!(
            "workbook-metadata-title-named",
            name = meta.long_name.as_str()
        )
    };
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_column_metadata"),
        &title,
        PlainSize::Class(SizeClass::Medium),
        |ui| {
            crate::theme::page_scroll_area().show(ui, |ui| {
                egui::Grid::new("stokesforge_column_metadata_grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for line in &lines {
                            if line.is_section() {
                                ui.label(crate::theme::table_header(&line.key));
                                ui.label("");
                            } else if line.key.is_empty() {
                                ui.label("");
                                ui.label(&line.value);
                            } else {
                                ui.label(&line.key);
                                ui.label(&line.value);
                            }
                            // Mini import picker (#164): a numeric value
                            // from the file can feed the x' row.
                            if !line.key.is_empty()
                                && matches!(parse_x_prime(&line.value), Ok(Some(_)))
                                && ui
                                    .small_button(tr!("workbook-metadata-use-as-x-prime"))
                                    .on_hover_text(tr!("workbook-metadata-use-as-x-prime", "hint"))
                                    .clicked()
                            {
                                actions.push(Action::SetColumnXPrime {
                                    column,
                                    text: line.value.clone(),
                                });
                            }
                            ui.end_row();
                        }
                    });
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseColumnMetadata);
    }
}

/// Body of [`plot_settings_window`]: the tab strip, then the page of the
/// chosen tab.
fn plot_settings_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tree: &Tree,
    settings: PlotSettings,
) {
    let mut chosen_tab = settings.tab;
    let mut chosen_axis = settings.axis;
    // Same strip as the sheet tabs (`tab_strip`, issue #19): it
    // scrolls instead of shrinking when the window is narrow.
    if settings.tab != PlotSettingsTab::Series {
        let tabs: Vec<(PlotSettingsTab, String)> = PlotSettingsTab::ALL
            .iter()
            .map(|tab| (*tab, tab.label()))
            .collect();
        // The strip draws its own rule under the tabs (#169): no
        // separator in addition, only the space it used to leave.
        tab_strip(
            ui,
            TabStripSide::Top,
            &tabs,
            settings.tab,
            |tab| chosen_tab = tab,
            None,
            || {},
        );
        ui.add_space(ui.spacing().item_spacing.y);
    }

    let mut commit = false;
    let unit = state.length_unit();
    let published = super::graph_window::published_figure_layout(ui.ctx(), settings.graph);
    let mut gradient_actions: Vec<Action> = Vec::new();
    // Saved colors, read before borrowing the draft; the
    // picker requests (save, remove) are translated into actions
    // afterwards.
    let mut env = PickerEnv {
        project: state.saved_colors(SaveTarget::Project),
        user: state.saved_colors(SaveTarget::User),
        requests: Vec::new(),
    };
    if let Some(edit) = state
        .plot_edit_mut()
        .filter(|edit| edit.graph == settings.graph)
    {
        let spec = &mut edit.spec;
        edit.selected_panel = edit.selected_panel.min(spec.panels.len() - 1);
        if spec.panels.len() > 1 {
            // The "Graph" and "Theme" pages apply to the whole graph:
            // the layer choice stays displayed but grayed out, with its reason (#180).
            // Except the Graph page of a free graph, whose Size section edits
            // the layer chosen here (#398).
            let free = spec.layer_placement == LayerPlacement::Free;
            let per_layer = match chosen_tab {
                PlotSettingsTab::Graph => free,
                PlotSettingsTab::Theme => false,
                _ => true,
            };
            let layer_choice = ui.add_enabled_ui(per_layer, |ui| {
                egui::ComboBox::from_id_salt("plot_settings_layer")
                    .selected_text(tr!(
                        "object-manager-layer",
                        index = (edit.selected_panel + 1)
                    ))
                    .show_ui(ui, |ui| {
                        for index in 0..spec.panels.len() {
                            ui.selectable_value(
                                &mut edit.selected_panel,
                                index,
                                tr!("object-manager-layer", index = (index + 1)),
                            );
                        }
                    });
            });
            if !per_layer {
                layer_choice
                    .response
                    .on_hover_text(tr!("plot-layer-choice-disabled"));
            }
        }
        // The page takes the whole height of the window, even when its
        // content is shorter: without `auto_shrink(false)`, a resizable
        // `egui::Window` snaps to the height of its
        // content, and it could not be enlarged beyond. The same setting
        // makes it scroll when the window is smaller than the page.
        if chosen_tab == PlotSettingsTab::Series {
            // Two columns that each scroll: no common scrolling.
            if let Some(panel) = spec.panels.get_mut(edit.selected_panel) {
                commit |= curve_settings_page(
                    ui,
                    panel,
                    edit.selected_panel,
                    tree,
                    &mut edit.selected_series,
                    &mut env,
                    &mut gradient_actions,
                );
            }
        } else if chosen_tab == PlotSettingsTab::Axes {
            // Two levels (S-L9, #393): the vertical axis strip stays fixed and
            // only the page of the axis scrolls, so no common scrolling.
            commit |= axes_settings_page(
                ui,
                spec,
                edit.selected_panel,
                &mut chosen_axis,
                published_axis_bounds(ui.ctx(), settings.graph, edit.selected_panel),
                &mut env,
            );
        } else {
            crate::theme::page_scroll_area()
                .auto_shrink([false, false])
                .show(ui, |ui| match chosen_tab {
                    PlotSettingsTab::Graph => {
                        let layout = dialog_layout(ui.ctx(), settings.graph, spec, published);
                        commit |= graph_settings_page(
                            ui,
                            spec,
                            edit.selected_panel,
                            &mut env,
                            layout.as_ref(),
                            unit,
                        );
                    }
                    PlotSettingsTab::PlotArea => {
                        let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                            return;
                        };
                        commit |= plot_area_settings_page(ui, panel);
                    }
                    // The axes have their own fixed strip: see above.
                    PlotSettingsTab::Axes => {}
                    // The series have their window: see above.
                    PlotSettingsTab::Series => {}
                    PlotSettingsTab::Theme => {
                        commit |= theme_editor(ui, spec, &mut env);
                    }
                    PlotSettingsTab::Legend => {
                        let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                            return;
                        };
                        commit |= legend_editor(ui, &mut panel.legend);
                    }
                });
        }
    }
    if commit {
        state.commit_plot_edit();
    }
    for action in gradient_actions {
        state.dispatch(action);
    }
    for request in env.requests {
        state.dispatch(match request.edit {
            SavedColorEdit::Save => Action::SaveColor {
                usage: request.usage,
                color: request.color,
                target: request.target,
            },
            SavedColorEdit::Remove => Action::RemoveSavedColor {
                usage: request.usage,
                color: request.color,
                target: request.target,
            },
        });
    }
    if let Some(current) = state.plot_settings_mut() {
        current.tab = chosen_tab;
        current.axis = chosen_axis;
    }
}

/// The layout the size fields read: the one the graph's view last published
/// when it describes the draft (same grid, same legacy or stored frames), else
/// one computed from the draft with the measurement of the exports, memoized
/// until the draft changes. The graph may not be on screen, or the draft may
/// have just fixed the frames of a legacy graph.
fn dialog_layout(
    ctx: &egui::Context,
    graph: NodeId,
    draft: &PlotSpec,
    published: Option<FigureLayout>,
) -> Option<FigureLayout> {
    let stored = !draft.frame_widths.is_empty() || !draft.frame_heights.is_empty();
    // A free graph is always measured from the draft: the origin of the image
    // moves with every value typed, and the published layout may be one edit
    // behind (#398).
    let published = published.filter(|_| draft.layer_placement == LayerPlacement::Grid);
    if let Some(layout) = published.filter(|layout| {
        layout.placement == LayerPlacement::Grid
            && layout.frame_widths.len() == draft.columns()
            && layout.frame_heights.len() == draft.rows()
            && layout.is_explicit() == stored
    }) {
        return Some(layout);
    }
    let id = egui::Id::new(("graph_size_layout", graph));
    if let Some((cached, layout)) = ctx.data(|data| data.get_temp::<(PlotSpec, FigureLayout)>(id)) {
        if cached == *draft {
            return Some(layout);
        }
    }
    let layout = stokesforge_app_state::frame_sizes::figure_layout(draft)?;
    ctx.data_mut(|data| data.insert_temp(id, (draft.clone(), layout.clone())));
    Some(layout)
}

/// Graph page: title and background, as a two-column form (#227).
fn graph_settings_page(
    ui: &mut egui::Ui,
    spec: &mut PlotSpec,
    panel: usize,
    env: &mut PickerEnv,
    layout: Option<&FigureLayout>,
    unit: LengthUnit,
) -> bool {
    form(ui, "plot_graph_page", |form| {
        let mut commit = false;
        commit |= formula_field(
            form,
            &tr!("plot-general-title"),
            &tr!("plot-general-title", "hint"),
            &mut spec.title,
        );
        commit |= form.color(
            &tr!("plot-title-color"),
            &mut spec.title_color,
            ColorUsage::Text,
            Some(&tr!("plot-color-auto-contrast")),
            env,
        );
        commit |= form.color(
            &tr!("plot-general-background"),
            &mut spec.background,
            ColorUsage::Background,
            Some(&tr!("plot-color-auto-white")),
            env,
        );
        form.note(tr!("plot-general-dark-note"));
        commit |= form.checkbox(
            &mut spec.stale_warning,
            tr!("plot-stale-warning"),
            Some(tr!("plot-stale-warning", "hint")),
        );
        commit |= size_section(form, spec, panel, layout, unit);
        // S-L8 (#405): the figure size has an Auto default ("Grows with the
        // axes"), almost never changed: it lives in the closed Advanced block.
        commit |= form
            .collapsible("plot_graph_advanced", tr!("common-advanced"), |form| {
                figure_size_row(form, spec, layout)
            })
            .unwrap_or(false);
        commit
    })
}

/// What a legacy draft is drawn with, to give it its frames when Fixed, Left or
/// Top is set on it (see [`set_size_in_draft`]).
fn drawn_frames(spec: &PlotSpec, layout: Option<&FigureLayout>) -> (Vec<f64>, Vec<f64>) {
    let pick = |stored: Option<Vec<f64>>, painted: Option<&Vec<f64>>, count, fallback| {
        stored.unwrap_or_else(|| {
            painted
                .filter(|sizes| sizes.len() == count)
                .cloned()
                .unwrap_or_else(|| vec![fallback; count])
        })
    };
    (
        pick(
            spec.sanitized_frame_widths(),
            layout.map(|layout| &layout.frame_widths),
            spec.columns(),
            FALLBACK_FRAME.0,
        ),
        pick(
            spec.sanitized_frame_heights(),
            layout.map(|layout| &layout.frame_heights),
            spec.rows(),
            FALLBACK_FRAME.1,
        ),
    )
}

/// General › Advanced › Figure size (#405): "Grows with the axes" (default) or
/// "Fixed" (room for four axes from the start). A legacy draft is given its
/// frames in the same edit.
fn figure_size_row(
    form: &mut Form<'_>,
    spec: &mut PlotSpec,
    layout: Option<&FigureLayout>,
) -> bool {
    let options = [
        ChoiceOption::new(false, tr!("plot-figure-size-grows")),
        ChoiceOption::new(true, tr!("plot-figure-size-fixed")),
    ];
    let mut fixed = spec.fixed_figure;
    let changed = form.row(tr!("plot-figure-size"), |ui| {
        choice(ui, "plot_figure_size", &mut fixed, &options)
    });
    form.note(tr!("plot-figure-size", "hint"));
    if !changed {
        return false;
    }
    let (widths, heights) = drawn_frames(spec, layout);
    set_fixed_in_draft(spec, (&widths, &heights), fixed)
}

/// General › Size (#394): the width of the plot area (the frame) of each grid
/// column and its height for each grid row, in the unit of the Preferences, and
/// the size of the graph they add up to with the axes and the title (read-only).
/// Edits go into the draft like every other field of the window, in the one
/// undo step of the window; a graph from an old file is first given the frames
/// it is drawn with, so only the edited one changes.
fn size_section(
    form: &mut Form<'_>,
    spec: &mut PlotSpec,
    panel: usize,
    layout: Option<&FigureLayout>,
    unit: LengthUnit,
) -> bool {
    if spec.layer_placement == LayerPlacement::Free {
        return free_size_section(form, spec, panel, layout, unit);
    }
    // What each frame is drawn with: the stored size, else the legacy one the
    // last painted figure laid out (its `frame_widths` are the frames a legacy
    // graph is drawn with).
    let drawn = |stored: Option<Vec<f64>>, painted: Option<&Vec<f64>>, count: usize, fallback| {
        stored.unwrap_or_else(|| {
            painted
                .filter(|sizes| sizes.len() == count)
                .cloned()
                .unwrap_or_else(|| vec![fallback; count])
        })
    };
    let drawn_widths = drawn(
        spec.sanitized_frame_widths(),
        layout.map(|layout| &layout.frame_widths),
        spec.columns(),
        FALLBACK_FRAME.0,
    );
    let drawn_heights = drawn(
        spec.sanitized_frame_heights(),
        layout.map(|layout| &layout.frame_heights),
        spec.rows(),
        FALLBACK_FRAME.1,
    );
    if drawn_widths.len() != spec.columns() || drawn_heights.len() != spec.rows() {
        return false;
    }
    let symbol = unit.symbol();
    form.section(tr!("plot-size"));
    let mut commit = placement_row(form, spec);
    let quantity = quantities::frame_size(unit);
    for (axis, sizes) in [
        (FrameAxis::Column, &drawn_widths),
        (FrameAxis::Row, &drawn_heights),
    ] {
        for (index, &points) in sizes.iter().enumerate() {
            let number = index + 1;
            let label = match (axis, sizes.len() == 1) {
                (FrameAxis::Column, true) => tr!("plot-size-width", unit = symbol),
                (FrameAxis::Column, false) => {
                    tr!("plot-size-column-width", n = number, unit = symbol)
                }
                (FrameAxis::Row, true) => tr!("plot-size-height", unit = symbol),
                (FrameAxis::Row, false) => tr!("plot-size-row-height", n = number, unit = symbol),
            };
            let mut shown = unit.from_points(points);
            let id = ("graph_size", matches!(axis, FrameAxis::Row), index);
            let changed = form.row(label, |ui| {
                number_field(ui, id, &mut shown, &quantity).changed
            });
            if changed {
                commit |= set_size_in_draft(
                    spec,
                    (&drawn_widths, &drawn_heights),
                    axis,
                    index,
                    unit.to_points(shown),
                );
            }
        }
    }
    commit |= offset_rows(form, spec, layout, unit, (&drawn_widths, &drawn_heights));
    // The graph the frames make once the axes and the title are added around
    // them, as the last painted figure lays them out.
    let frames = (
        spec.sanitized_frame_widths().unwrap_or(drawn_widths),
        spec.sanitized_frame_heights().unwrap_or(drawn_heights),
    );
    let (total_width, total_height) = layout.map_or_else(
        || (frames.0.iter().sum(), frames.1.iter().sum()),
        |layout| layout.size_with_frames(&frames.0, &frames.1),
    );
    form.row(tr!("plot-size-total", unit = symbol), |ui| {
        ui.label(tr!(
            "plot-size-total-value",
            width = format_value(unit.from_points(total_width.round()), unit.decimals()),
            height = format_value(unit.from_points(total_height.round()), unit.decimals())
        ));
        false
    });
    form.note(tr!("plot-size-note"));
    commit
}

/// General › Size › Layer placement (#398): Regular grid or Free. Switching
/// goes through the draft like every field of the window; going free leaves
/// every plot area where it is drawn, going back applies the grid rules of the
/// model.
fn placement_row(form: &mut Form<'_>, spec: &mut PlotSpec) -> bool {
    let options = [
        ChoiceOption::new(false, tr!("plot-size-placement-grid")),
        ChoiceOption::new(true, tr!("plot-size-placement-free")),
    ];
    let mut free = spec.layer_placement == LayerPlacement::Free;
    let changed = form.row(tr!("plot-size-placement"), |ui| {
        choice(ui, "plot_layer_placement", &mut free, &options)
    });
    changed && set_placement_in_draft(spec, free)
}

/// General › Size of a free graph (#398): Left, Top, Width and Height of the
/// plot area of the layer chosen in the list, in the unit of the Preferences,
/// and the size of the graph around the layers (read-only). Left and Top are
/// distances from the edge of the image, which surrounds the layers.
fn free_size_section(
    form: &mut Form<'_>,
    spec: &mut PlotSpec,
    panel: usize,
    layout: Option<&FigureLayout>,
    unit: LengthUnit,
) -> bool {
    let symbol = unit.symbol();
    form.section(tr!("plot-size"));
    let mut commit = placement_row(form, spec);
    let Some(layout) = layout else {
        return commit;
    };
    let Some(fields) = free_fields(spec, layout, panel) else {
        return commit;
    };
    let size = quantities::frame_size(unit);
    let rows = [
        (
            FreeField::Left,
            tr!("plot-size-left", unit = symbol),
            fields.left.max(fields.min_left),
            quantities::free_offset(unit, fields.min_left),
        ),
        (
            FreeField::Top,
            tr!("plot-size-top", unit = symbol),
            fields.top.max(fields.min_top),
            quantities::free_offset(unit, fields.min_top),
        ),
        (
            FreeField::Width,
            tr!("plot-size-width", unit = symbol),
            fields.width,
            size,
        ),
        (
            FreeField::Height,
            tr!("plot-size-height", unit = symbol),
            fields.height,
            size,
        ),
    ];
    for (field, label, points, quantity) in rows {
        let mut shown = unit.from_points(points);
        let id = ("graph_free", panel, field as u8);
        let changed = form.row(label, |ui| {
            // A Left or Top below the layer's own bands is clamped to them by
            // the field itself (orange frame and bubble, S-clamp).
            number_field(ui, id, &mut shown, &quantity).changed
        });
        if changed {
            commit |= set_free_field_in_draft(spec, layout, panel, field, unit.to_points(shown));
        }
    }
    let (total_width, total_height) = (f64::from(layout.size.0), f64::from(layout.size.1));
    form.row(tr!("plot-size-total", unit = symbol), |ui| {
        ui.label(tr!(
            "plot-size-total-value",
            width = format_value(unit.from_points(total_width), unit.decimals()),
            height = format_value(unit.from_points(total_height), unit.decimals())
        ));
        false
    });
    form.note(tr!("plot-size-free-note"));
    commit
}

/// Left and Top (#405): the distance from the left and top edges of the image
/// to the first plot area, Auto when empty. A value smaller than the axes (and
/// the title, for Top) need is raised to that minimum, and the field says why.
/// The image grows past the reserve of a Fixed figure: the note below says so.
fn offset_rows(
    form: &mut Form<'_>,
    spec: &mut PlotSpec,
    layout: Option<&FigureLayout>,
    unit: LengthUnit,
    drawn: (&[f64], &[f64]),
) -> bool {
    let symbol = unit.symbol();
    let mut commit = false;
    for (which, label, minimum, stored) in [
        (
            FigureOffset::Left,
            tr!("plot-size-left", unit = symbol),
            layout.map_or(0.0, |layout| f64::from(layout.min_left)),
            spec.sanitized_figure_left(),
        ),
        (
            FigureOffset::Top,
            tr!("plot-size-top", unit = symbol),
            layout.map_or(0.0, |layout| f64::from(layout.min_top)),
            spec.sanitized_figure_top(),
        ),
    ] {
        let quantity = quantities::figure_offset(unit, minimum);
        let too_small = stored.is_some_and(|points| points < minimum);
        let mut shown = stored.map(|points| unit.from_points(points.max(minimum)));
        let changed = form.row(label, |ui| {
            let response = auto_number_field(
                ui,
                ("graph_offset", matches!(which, FigureOffset::Top)),
                &mut shown,
                &quantity,
                &tr!("common-auto"),
            );
            if too_small {
                let minimum_text = format_value(unit.from_points(minimum), unit.decimals());
                let why = match which {
                    FigureOffset::Left => {
                        tr!(
                            "plot-size-left-raised",
                            minimum = minimum_text,
                            unit = symbol
                        )
                    }
                    FigureOffset::Top => {
                        tr!(
                            "plot-size-top-raised",
                            minimum = minimum_text,
                            unit = symbol
                        )
                    }
                };
                response.response.clone().on_hover_text(why);
            }
            response.changed
        });
        if changed {
            commit |=
                set_offset_in_draft(spec, drawn, which, shown.map(|value| unit.to_points(value)));
        }
    }
    if layout.is_some_and(|layout| spec.fixed_figure && layout.fixed_overflow) {
        form.note(tr!("status-figure-grew"));
    }
    commit
}

/// Layer page: title and gridlines.
fn plot_area_settings_page(ui: &mut egui::Ui, panel: &mut PanelSpec) -> bool {
    form(ui, "plot_area_page", |form| {
        let mut commit = false;
        commit |= formula_field(
            form,
            &tr!("plot-area-layer-title"),
            &tr!("plot-area-layer-title", "hint"),
            &mut panel.title,
        );
        form.section(tr!("plot-area-gridlines"));
        commit |= form.checkbox(
            &mut panel.bottom.grid,
            tr!("plot-area-gridlines-vertical"),
            None,
        );
        commit |= form.checkbox(
            &mut panel.left.grid,
            tr!("plot-area-gridlines-horizontal"),
            None,
        );
        commit
    })
}

/// Axes page: a vertical strip at the left chooses the side of the cell
/// (fixed), the shared editor is at its right and alone scrolls (S-L9, #393).
///
/// Always the four sides, Bottom, Left, Right, Top, on every layer (#396): an
/// axis is named where it is drawn, and a side that another layer holds is
/// greyed out with the reason in its own bubble. What each side edits is the
/// rule of [`cell_sides`] (the Object Manager asks it too). `chosen_axis` is
/// moved to a usable side when the chosen one is taken.
pub(super) fn axes_settings_page(
    ui: &mut egui::Ui,
    spec: &mut PlotSpec,
    panel_index: usize,
    chosen_axis: &mut PlotAxis,
    bounds: Option<PanelBounds>,
    env: &mut PickerEnv,
) -> bool {
    let sides = cell_sides::sides(spec, panel_index);
    *chosen_axis = PlotAxis::from_side(cell_sides::usable_place(
        spec,
        panel_index,
        chosen_axis.side(),
    ));
    // #169: the axes are a set of tabs (one page per axis), so the common
    // strip. #393 (S-L9): they are the secondary level under the Graph
    // Properties tabs, so a vertical strip at the left of the page; it stays
    // fixed and only the page of the axis scrolls.
    let tabs: Vec<(PlotAxis, String)> = PLACES
        .into_iter()
        .map(|place| {
            let axis = PlotAxis::from_side(place);
            (axis, axis.label())
        })
        .collect();
    let side_of = |axis: &PlotAxis| sides.iter().find(|side| side.place == axis.side()).copied();
    // The row takes the whole height left: `ui.horizontal` would hand the
    // scroll area about one row.
    let size = ui.available_size();
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Min), |ui| {
        let mut picked = None;
        tab_strip_left_with_reasons(
            ui,
            &tabs,
            *chosen_axis,
            &|axis| side_of(axis).and_then(|side| side.reason()),
            |axis| picked = Some(axis),
        );
        if let Some(axis) = picked {
            *chosen_axis = axis;
        }
        let Some(side) = side_of(chosen_axis).filter(|side| side.available()) else {
            return false;
        };
        let Some(field) = side.field else {
            return false;
        };
        ui.vertical(|ui| {
            let computed = bounds.and_then(|bounds| match field {
                AxisSide::Bottom => bounds.bottom,
                AxisSide::Left => bounds.left,
                _ => bounds.right,
            });
            let Some(panel) = spec.panels.get_mut(panel_index) else {
                return false;
            };
            let decimal_comma = &mut spec.decimal_comma;
            crate::theme::page_scroll_area()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    form(ui, "plot_axis_page", |form| {
                        let mut commit = if side.mirror {
                            mirror_axis_editor(form, panel.axis_mut(field), env)
                        } else if side.own_scale {
                            // An overlaid layer's top / right: the axis
                            // fields plus the "Own scale" of the layer.
                            let own = panel.overlay.as_mut().map(|overlay| {
                                if field == AxisSide::Bottom {
                                    &mut overlay.x_top
                                } else {
                                    &mut overlay.y_right
                                }
                            });
                            let axis = if field == AxisSide::Bottom {
                                &mut panel.bottom
                            } else {
                                &mut panel.left
                            };
                            axis_editor(form, axis, computed, own, env)
                        } else {
                            axis_editor(form, panel.axis_mut(field), computed, None, env)
                        };
                        // Setting of the whole graph (#223), put under the
                        // axes display.
                        commit |= form.checkbox(
                            decimal_comma,
                            tr!("plot-axes-decimal-comma"),
                            Some(tr!("plot-axes-decimal-comma", "hint")),
                        );
                        commit
                    })
                })
                .inner
        })
        .inner
    })
    .inner
}

/// "Theme" tab: background, text color and axes color, set
/// globally (issue #11), plus predefined themes. The last setting
/// overwrites what had been set one by one: see [`stokesforge_plot::theme`].
/// Returns `true` if the draft must be committed.
fn theme_editor(
    ui: &mut egui::Ui,
    spec: &mut stokesforge_plot::PlotSpec,
    env: &mut PickerEnv,
) -> bool {
    form(ui, "plot_theme_page", |form| {
        let mut commit = false;
        form.section(tr!("plot-theme-themes"));
        form.field(|ui| {
            for (index, theme) in stokesforge_plot::THEMES.iter().enumerate() {
                if ui
                    .button(stokesforge_app_state::figure_texts::theme_name(index))
                    .clicked()
                {
                    spec.apply_theme(theme);
                    commit = true;
                }
            }
        });
        form.section(tr!("plot-theme-global-colors"));
        commit |= form.color(
            &tr!("plot-theme-background"),
            &mut spec.background,
            ColorUsage::Background,
            Some(&tr!("plot-color-auto-white")),
            env,
        );
        let mut text = spec.theme_text_color().to_string();
        let text_committed = form.color(
            &tr!("plot-theme-text"),
            &mut text,
            ColorUsage::Text,
            Some(&tr!("plot-color-auto-contrast")),
            env,
        );
        if text != spec.theme_text_color() {
            spec.set_text_color(&text);
        }
        commit |= text_committed;
        let mut axes = spec.theme_axes_color().to_string();
        let axes_committed = form.color(
            &tr!("plot-theme-axes"),
            &mut axes,
            ColorUsage::Text,
            Some(&tr!("plot-color-auto-contrast")),
            env,
        );
        if axes != spec.theme_axes_color() {
            spec.set_axes_color(&axes);
        }
        commit |= axes_committed;
        form.note(tr!("plot-theme-note"));
        commit
    })
}

/// The syntax reminder for the graph texts (issues #33, #197), as a hover
/// bubble on the help icon placed to the right of the field.
///
/// These texts are composed by Typst and read as **markup** (issue
/// #32): a chemist coming from Origin does not know its syntax, and a lone `_`
/// or `$` changes the rendering without warning. The reminder names Typst and
/// gives the reference page. An icon at the end of the field rather than a
/// collapsible memo below: the link with the field is visible, and the help no longer takes
/// height in the window (see [`super::hover_help`]).
///
/// The Preferences can hide it (Display › Interface, #148): the window then
/// leaves [`FORMULA_HELP_HIDDEN`] in the context for the fields it paints.
/// Hidden, the icon disappears; if it also carried the field's own `hint`, that
/// hint stays reachable as a plain "?".
fn formula_help(ui: &mut egui::Ui, hint: &str) {
    let hidden = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(egui::Id::new(FORMULA_HELP_HIDDEN)))
        .unwrap_or(false);
    if hidden {
        if !hint.is_empty() {
            help_text(ui, hint);
        }
        return;
    }
    hover_help(ui, |ui| {
        if !hint.is_empty() {
            ui.label(hint);
        }
        ui.label(table_header(tr!("plot-math-memo-header")));
        egui::Grid::new("plot_formula_help")
            .num_columns(2)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for (source, effect) in formula_memo_rows() {
                    ui.monospace(source);
                    ui.weak(effect);
                    ui.end_row();
                }
            });
        ui.weak(tr!(
            "plot-math-footer",
            url = "typst.app/docs/reference/math"
        ));
    });
}

/// The rows of the [`formula_help`] reminder: what is typed (Typst
/// syntax, never translated), what is obtained (translated).
fn formula_memo_rows() -> [(&'static str, String); 7] {
    [
        ("$lambda$  $Delta$", tr!("plot-math-greek")),
        ("$x_2$  $\"Ru(bpy)\"_3$", tr!("plot-math-subscript")),
        ("$x^2$  $\"cm\"^(-1)$", tr!("plot-math-superscript")),
        ("$Delta H = sum_i n_i$", tr!("plot-math-full")),
        ("$\"L mol\"^(-1)$", tr!("plot-math-upright")),
        ("*bold*  _italic_", tr!("plot-math-format")),
        ("\\$  \\_  \\*  \\#", tr!("plot-math-escape")),
    ]
}

/// A form row whose text field accepts a formula (#197),
/// committed on focus loss; `hint` is its reminder, first line of
/// the help icon for the Typst syntax ([`formula_help`]) placed at the far right (S-T6
/// : the "?" icon alone). The field takes the full width (S-L2), minus the
/// icon's room. Returns `true` if the draft must be committed — see the
/// head doc of [`plot_settings_window`].
fn formula_field(form: &mut Form<'_>, label: &str, hint: &str, value: &mut String) -> bool {
    form.row(label, |ui| {
        let width = FieldWidth::Full.points(ui) - ICON_SIZE - ui.spacing().item_spacing.x;
        let response = ui.add(egui::TextEdit::singleline(value).desired_width(width));
        // S-T3: once the text is committed (field left), the Typst error is painted
        // on the field; the graph, for its part, falls back to the raw text. No red
        // while typing, where the markup is necessarily unfinished.
        if !response.has_focus() {
            if let Some(error) = stokesforge_render_egui::formula::diagnose(value) {
                field_state::show_issue(ui, &response, &FieldIssue::refused(error));
            }
        }
        formula_help(ui, hint);
        response.lost_focus()
    })
}

/// Edits a mirror axis (#176): it takes the scale of the opposite axis and has neither
/// its own title nor axis color; one chooses to show it, with or without
/// tick labels, and to set its ticks (like any axis,
/// each for itself).
fn mirror_axis_editor(form: &mut Form<'_>, axis: &mut AxisSpec, env: &mut PickerEnv) -> bool {
    form.section(tr!("plot-axes-display-section"));
    let mut commit = form.checkbox(&mut axis.mirror, tr!("plot-axes-show-mirror"), None);
    commit |= form.field(|ui| {
        ui.add_enabled(
            axis.mirror,
            egui::Checkbox::new(&mut axis.tick_labels, tr!("plot-axes-show-tick-labels")),
        )
        .on_disabled_hover_text(tr!("disabled-mirror-off"))
        .changed()
    });
    commit |= label_every_row(form, axis, axis.mirror);
    form.note(tr!("plot-axes-mirror-note"));
    form.section(tr!("plot-axes-ticks-section"));
    commit |= form.field(|ui| {
        ui.add_enabled(
            axis.mirror,
            egui::Checkbox::new(&mut axis.ticks, tr!("plot-axes-show-ticks")),
        )
        .on_disabled_hover_text(tr!("disabled-mirror-off"))
        .changed()
    });
    commit |= tick_direction_row(form, axis, axis.mirror, tr!("disabled-mirror-off"));
    commit |= minor_ticks_rows(form, axis, axis.mirror, tr!("disabled-mirror-off"));
    commit |= advanced_ticks_block(form, axis, axis.mirror, tr!("disabled-mirror-off"), env);
    commit
}

/// The minor tick settings (#344): the checkbox, then their count between
/// two major ticks (Auto by default; no effect on a logarithmic
/// scale, grayed out). `enabled` false (mirror off) grays out everything, with
/// `reason`.
fn minor_ticks_rows(
    form: &mut Form<'_>,
    axis: &mut AxisSpec,
    enabled: bool,
    reason: String,
) -> bool {
    let mut commit = form.field(|ui| {
        ui.add_enabled(
            enabled,
            egui::Checkbox::new(&mut axis.minor_ticks, tr!("plot-axes-show-minor-ticks")),
        )
        .on_disabled_hover_text(&reason)
        .changed()
    });
    let counted = enabled && axis.minor_ticks && axis.scale == stokesforge_plot::AxisScale::Linear;
    commit |= form.row(tr!("plot-axes-minor-tick-count"), |ui| {
        let shown = ui.add_enabled_ui(counted, |ui| {
            auto_number_field(
                ui,
                "axis_minor_tick_count",
                &mut axis.minor_tick_count,
                &quantities::MINOR_TICK_COUNT,
                &tr!("plot-axes-minor-tick-count", "auto"),
            )
            .changed
        });
        shown.response.on_disabled_hover_text(reason);
        shown.inner
    });
    commit
}

/// "Label every" (#344): grayed out when the labels are hidden.
fn label_every_row(form: &mut Form<'_>, axis: &mut AxisSpec, enabled: bool) -> bool {
    form.row(tr!("plot-axes-label-every"), |ui| {
        let shown = ui.add_enabled_ui(enabled && axis.tick_labels, |ui| {
            number_field(
                ui,
                "axis_label_every",
                &mut axis.label_every,
                &quantities::LABEL_EVERY,
            )
            .changed
        });
        shown.inner
    })
}

/// A "number or Auto" row of the Advanced section (#344): `None` shows
/// "Auto" in gray, emptying the field makes it automatic. Grayed out with `reason`
/// when `enabled` is false (S-G4).
fn auto_value_row(
    form: &mut Form<'_>,
    label: String,
    id_salt: &'static str,
    value: &mut Option<f64>,
    quantity: &quantities::Quantity,
    enabled: bool,
    reason: String,
) -> bool {
    form.row(label, |ui| {
        let shown = ui.add_enabled_ui(enabled, |ui| {
            auto_number_field(ui, id_salt, value, quantity, &tr!("common-auto")).changed
        });
        shown.response.on_disabled_hover_text(reason);
        shown.inner
    })
}

/// The collapsible "Advanced" block of the ticks (#344), closed by default:
/// length of the major and minor tick marks, width and color, all
/// Auto as long as they are not touched. Per axis, never copied to the others.
/// `enabled` false (mirror off) grays out everything, with `reason`.
fn advanced_ticks_block(
    form: &mut Form<'_>,
    axis: &mut AxisSpec,
    enabled: bool,
    reason: String,
    env: &mut PickerEnv,
) -> bool {
    form.collapsible("plot_axis_advanced_ticks", tr!("common-advanced"), |form| {
        let major = enabled && axis.ticks;
        let minor = enabled && axis.minor_ticks;
        let major_reason = if enabled {
            tr!("disabled-ticks-off")
        } else {
            reason.clone()
        };
        let minor_reason = if enabled {
            tr!("disabled-minor-ticks-off")
        } else {
            reason.clone()
        };
        let mut commit = auto_value_row(
            form,
            tr!("plot-axes-tick-length"),
            "axis_tick_length",
            &mut axis.tick_length,
            &quantities::TICK_LENGTH,
            major,
            major_reason.clone(),
        );
        commit |= auto_value_row(
            form,
            tr!("plot-axes-minor-tick-length"),
            "axis_minor_tick_length",
            &mut axis.minor_tick_length,
            &quantities::TICK_LENGTH,
            minor,
            minor_reason,
        );
        let any = major || minor;
        let any_reason = if enabled {
            tr!("disabled-ticks-off")
        } else {
            reason
        };
        commit |= auto_value_row(
            form,
            tr!("plot-axes-tick-width"),
            "axis_tick_width",
            &mut axis.tick_width,
            &quantities::TICK_WIDTH,
            any,
            any_reason,
        );
        commit |= form
            .enabled(any, |form| {
                form.color(
                    &tr!("plot-axes-tick-mark-color"),
                    &mut axis.tick_mark_color,
                    ColorUsage::Text,
                    Some(&tr!("plot-axes-tick-mark-color", "auto")),
                    env,
                )
            })
            .inner;
        commit
    })
    .unwrap_or(false)
}

/// The direction of the ticks (#344): outward, inward or on both
/// sides. `enabled` false (mirror off, ticks off) grays out the choice,
/// with `reason`.
fn tick_direction_row(
    form: &mut Form<'_>,
    axis: &mut AxisSpec,
    enabled: bool,
    reason: String,
) -> bool {
    let enabled = enabled && axis.ticks;
    let reason = if axis.ticks {
        reason
    } else {
        tr!("disabled-ticks-off")
    };
    let options = [
        ChoiceOption::new(TickDirection::Out, tr!("plot-axes-tick-out")),
        ChoiceOption::new(TickDirection::In, tr!("plot-axes-tick-in")),
        ChoiceOption::new(TickDirection::Both, tr!("plot-axes-tick-both")),
    ];
    form.row(tr!("plot-axes-tick-direction"), |ui| {
        let shown = ui.add_enabled_ui(enabled, |ui| {
            choice(
                ui,
                "plot_axis_tick_direction",
                &mut axis.tick_direction,
                &options,
            )
        });
        shown.response.on_disabled_hover_text(reason);
        shown.inner
    })
}

/// Edits an [`AxisSpec`], in four groups: the axis title (label,
/// unit), its scale (logarithmic, From/To bounds: the axis direction is read
/// from their order), its display (axis, numbers, grid, color) and its
/// ticks (tick marks, direction, minor ticks, "Advanced" block). Returns `true` if the draft must be committed — see the head doc
/// of [`plot_settings_window`].
fn axis_editor(
    form: &mut Form<'_>,
    axis: &mut AxisSpec,
    computed: Option<(f64, f64)>,
    own_scale: Option<&mut bool>,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    form.section(tr!("plot-axes-title-section"));
    commit |= formula_field(
        form,
        &tr!("plot-axes-quantity"),
        &tr!("plot-axes-quantity", "hint"),
        &mut axis.label,
    );
    commit |= formula_field(
        form,
        &tr!("plot-axes-units"),
        &tr!("plot-axes-units", "hint"),
        &mut axis.unit,
    );
    commit |= form.color(
        &tr!("plot-title-color"),
        &mut axis.title_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );

    form.section(tr!("plot-axes-scale-section"));
    // An overlaid layer's top / right tab (#396): its scale is its own only
    // when ticked, otherwise it follows the layer it covers and the fields
    // below are dead.
    let mut scale_enabled = true;
    if let Some(own) = own_scale {
        commit |= form.checkbox(
            own,
            tr!("plot-axes-own-scale"),
            Some(tr!("plot-axes-own-scale", "hint")),
        );
        scale_enabled = *own;
    }
    let scale_rows = form.enabled(scale_enabled, |form| {
        let mut commit = false;
        let mut log_scale = axis.scale == stokesforge_plot::AxisScale::Log;
        if form.checkbox(
            &mut log_scale,
            tr!("plot-axes-log-scale"),
            Some(tr!("plot-axes-log-scale", "hint")),
        ) {
            axis.scale = if log_scale {
                stokesforge_plot::AxisScale::Log
            } else {
                stokesforge_plot::AxisScale::Linear
            };
            commit = true;
        }
        // From and To: always editable (#344). Under Auto, they show the computed
        // bounds, in display order; typing in one freezes both bounds
        // (the other is taken from the computation) and the direction follows the order: From > To = inverted
        // axis. The "Auto" button gives the bounds back to the data, direction kept.
        let auto = axis.auto_range;
        let (mut min, mut max) = if auto {
            computed
                .map(|(low, high)| {
                    if axis.inverted {
                        (high, low)
                    } else {
                        (low, high)
                    }
                })
                .or(axis.range_min.zip(axis.range_max))
                .unwrap_or((0.0, 0.0))
        } else {
            (axis.range_min.unwrap_or(0.0), axis.range_max.unwrap_or(0.0))
        };
        let mut changed = false;
        let mut auto_asked = false;
        // "From … to …" on a single row; the order is free, unlike the
        // analysis ranges (S-N11).
        form.row(tr!("plot-axes-from"), |ui| {
            let first = number_field(ui, "axis_min", &mut min, &quantities::AXIS_BOUND);
            ui.label(tr!("analysis-field-to"));
            let second = number_field(ui, "axis_max", &mut max, &quantities::AXIS_BOUND);
            changed |= first.changed || second.changed;
            auto_asked |= first.auto || second.auto;
            if ui
                .add_enabled(!auto, egui::Button::new(tr!("plot-axes-from-auto")))
                .on_hover_text(tr!("plot-axes-from-auto", "hint"))
                .on_disabled_hover_text(tr!("plot-axes-from-auto", "disabled"))
                .clicked()
            {
                auto_asked = true;
            }
        });
        if auto_asked {
            axis.auto_range = true;
            // The stored bounds take the axis direction again.
            *axis = axis.orient();
            commit = true;
        } else if changed {
            axis.auto_range = false;
            axis.range_min = Some(min);
            axis.range_max = Some(max);
            axis.inverted = min > max;
            commit = true;
        }
        commit
    });
    if !scale_enabled {
        scale_rows
            .response
            .on_disabled_hover_text(tr!("disabled-axis-own-scale-off"));
    }
    commit |= scale_rows.inner;

    form.section(tr!("plot-axes-display-section"));
    commit |= form.field(|ui| {
        ui.add_enabled(
            scale_enabled,
            egui::Checkbox::new(&mut axis.visible, tr!("plot-axes-show-axis")),
        )
        .on_disabled_hover_text(tr!("disabled-axis-own-scale-off"))
        .changed()
    });
    commit |= form.checkbox(
        &mut axis.tick_labels,
        tr!("plot-axes-show-tick-labels"),
        None,
    );
    commit |= label_every_row(form, axis, true);
    commit |= form.checkbox(&mut axis.grid, tr!("plot-axes-show-gridlines"), None);
    commit |= form.color(
        &tr!("plot-axes-color"),
        &mut axis.tick_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );
    form.note(tr!("plot-axes-color-note"));

    form.section(tr!("plot-axes-ticks-section"));
    commit |= form.checkbox(&mut axis.ticks, tr!("plot-axes-show-ticks"), None);
    commit |= tick_direction_row(form, axis, true, String::new());
    commit |= minor_ticks_rows(form, axis, true, String::new());
    commit |= advanced_ticks_block(form, axis, true, String::new(), env);
    commit
}

/// Edits the selected series: name, plot type, visibility, presence in
/// the legend, line style, marker style. Returns `true` if the draft
/// must be committed — see the head doc of [`plot_settings_window`].
///
/// [`PlotType::Unknown`] (C5, `guide-de-correction-robustesse.md` §F3) is
/// **never** in the offered list: it is the plot type of a
/// newer version of StokesForge, and offering it would suggest one can
/// go back to it when nothing knows how to rebuild it. A series that
/// carries it is still shown, read-only for this one field — the
/// rest (name, visibility…) stays editable as usual.
///
/// `locked` (#177): the **group** of the series is being set, not the series alone.
/// The settings specific to a series (name, visibility, presence in
/// the legend) are then grayed out, with a tooltip that says why, and the
/// colors hidden (#213: the group's color gradient provides them); the others (line style, symbol shape and size) stay active and the
/// page copies them to the whole group.
pub(super) fn series_editor(
    ui: &mut egui::Ui,
    series: &mut stokesforge_plot::SeriesSpec,
    env: &mut PickerEnv,
    locked: bool,
    auto_bar_width: f64,
    overlay: bool,
    error_columns: &[(NodeId, String)],
) -> bool {
    form(ui, "plot_series_editor", |form| {
        let mut commit = false;
        commit |= per_series(form, locked, |form| {
            formula_field(
                form,
                &tr!("plot-series-name"),
                &tr!("plot-series-name", "hint"),
                &mut series.name,
            )
        });

        // The plot type is always set series by series (#213), even in
        // "whole group" scope; it only touches the chosen series.
        commit |= form.row(tr!("plot-series-plot-type"), |ui| match &series.plot_type {
            PlotType::Unknown(_) => {
                ui.weak(tr!("plot-series-type-unknown"));
                false
            }
            _ => choice(
                ui,
                "plot_config_series_type",
                &mut series.plot_type,
                &[
                    PlotType::Scatter,
                    PlotType::Line,
                    PlotType::LineScatter,
                    PlotType::Bar,
                    PlotType::Histogram,
                    PlotType::ErrorBar,
                ]
                .map(|candidate| {
                    let label = plot_type_label(&candidate);
                    ChoiceOption::new(candidate, label)
                }),
            ),
        });
        // Bins and width (#359): only for the type they concern; in "whole
        // group" scope they are copied to the members of that type.
        match series.plot_type {
            PlotType::Histogram => {
                commit |= form.row(tr!("plot-series-bins"), |ui| {
                    let mut bins = series.histogram_bins.get();
                    let changed = number_field(
                        ui,
                        "series_histogram_bins",
                        &mut bins,
                        &quantities::HISTOGRAM_BINS,
                    )
                    .changed;
                    match stokesforge_plot::BinCount::new(bins) {
                        Some(count) if changed => {
                            series.histogram_bins = count;
                            true
                        }
                        _ => false,
                    }
                });
            }
            PlotType::Bar => {
                let hint = tr!(
                    "common-auto-value",
                    value = crate::number_field::format_value(
                        auto_bar_width,
                        quantities::BAR_WIDTH.decimals
                    )
                );
                commit |= form.row(tr!("plot-series-bar-width"), |ui| {
                    auto_number_field(
                        ui,
                        "series_bar_width",
                        &mut series.bar_width,
                        &quantities::BAR_WIDTH,
                        &hint,
                    )
                    .changed
                });
            }
            _ => {}
        }
        // Y axis (#358): Left or Right. Here only the series field changes;
        // `series_page` applies it through `PanelSpec::set_series_axis` (the
        // right axis becomes visible) and copies it to the group. Grayed out
        // on an overlaid layer: the rendering ignores Right there.
        commit |= form.row(tr!("plot-series-axis"), |ui| {
            let shown = ui.add_enabled_ui(!overlay, |ui| {
                choice(
                    ui,
                    "plot_config_series_axis",
                    &mut series.axis,
                    &[
                        ChoiceOption::new(
                            stokesforge_plot::SeriesAxis::Left,
                            tr!("plot-axis-left"),
                        ),
                        ChoiceOption::new(
                            stokesforge_plot::SeriesAxis::Right,
                            tr!("plot-axis-right"),
                        ),
                    ],
                )
            });
            if overlay {
                shown
                    .response
                    .on_disabled_hover_text(tr!("plot-series-axis", "disabled"));
            } else {
                shown
                    .response
                    .on_hover_text(tr!("plot-series-axis", "hint"));
            }
            shown.inner
        });
        // Error columns (#360): Y error and X error, "None" first. Per
        // series even in "whole group" scope (the columns are data, not
        // style), so `series_page` copies nothing. Not for a "row" series
        // (#166) nor a histogram, which has no point to put a whisker on.
        if series.row.is_none() && series.plot_type != PlotType::Histogram {
            commit |= error_row(
                form,
                (
                    tr!("plot-series-y-error"),
                    tr!("plot-series-y-error", "hint"),
                ),
                "plot_series_y_error",
                error_columns,
                &mut series.y_error,
            );
            commit |= error_row(
                form,
                (
                    tr!("plot-series-x-error"),
                    tr!("plot-series-x-error", "hint"),
                ),
                "plot_series_x_error",
                error_columns,
                &mut series.x_error,
            );
        }
        // "Show series" is the checkbox of the series list (curve_settings.rs).
        commit |= per_series(form, locked, |form| {
            form.checkbox(
                &mut series.show_in_legend,
                tr!("plot-series-show-in-legend"),
                None,
            )
        });

        form.section(tr!("plot-series-line-section"));
        if !locked {
            commit |= form.color(
                &tr!("plot-series-line-color"),
                &mut series.line.color,
                ColorUsage::Curves,
                None,
                env,
            );
        }
        commit |= form.row(tr!("plot-series-line-style"), |ui| {
            choice(
                ui,
                "plot_config_line_style",
                &mut series.line.style,
                &[
                    LineStyle::None,
                    LineStyle::Solid,
                    LineStyle::Dash,
                    LineStyle::Dot,
                    LineStyle::DashDot,
                ]
                .map(|candidate| ChoiceOption::new(candidate, line_style_label(candidate))),
            )
        });
        commit |= form.row(tr!("plot-series-line-width"), |ui| {
            number_field(
                ui,
                "series_line_width",
                &mut series.line.width,
                &quantities::LINE_WIDTH,
            )
            .changed
        });

        form.section(tr!("plot-series-symbol-section"));
        commit |= form.row(tr!("plot-series-symbol-shape"), |ui| {
            choice(
                ui,
                "plot_config_symbol_shape",
                &mut series.symbol.shape,
                &[
                    SymbolShape::None,
                    SymbolShape::Circle,
                    SymbolShape::Square,
                    SymbolShape::Triangle,
                    SymbolShape::Diamond,
                    SymbolShape::Plus,
                    SymbolShape::Cross,
                ]
                .map(|candidate| ChoiceOption::new(candidate, symbol_shape_label(candidate))),
            )
        });
        commit |= form.row(tr!("plot-series-symbol-size"), |ui| {
            number_field(
                ui,
                "series_symbol_size",
                &mut series.symbol.size,
                &quantities::SYMBOL_SIZE,
            )
            .changed
        });
        if !locked {
            commit |= form.color(
                &tr!("plot-series-symbol-color"),
                &mut series.symbol.color,
                ColorUsage::Curves,
                Some(&tr!("plot-color-same-as-line")),
                env,
            );
        }
        commit
    })
}

/// A "Y error" / "X error" row (#360): None, then the candidate columns.
fn error_row(
    form: &mut Form<'_>,
    (label, hint): (String, String),
    id_salt: &str,
    columns: &[(NodeId, String)],
    value: &mut Option<NodeId>,
) -> bool {
    form.row(label, |ui| {
        let mut options = vec![ChoiceOption::new(None, tr!("plot-series-error-none"))];
        options.extend(
            columns
                .iter()
                .map(|(id, name)| ChoiceOption::new(Some(*id), name.clone())),
        );
        let changed = choice(ui, id_salt, value, &options);
        help_text(ui, hint);
        changed
    })
}

/// A setting specific to a series: grayed out, with the tooltip that says why,
/// when the whole group is being set (`locked`, see [`series_editor`]).
fn per_series(form: &mut Form<'_>, locked: bool, add: impl FnOnce(&mut Form<'_>) -> bool) -> bool {
    let out = form.enabled(!locked, add);
    if locked {
        out.response.on_hover_text(tr!("curve-scope-locked"));
    }
    out.inner
}

/// Edits a [`stokesforge_plot::LegendSpec`]: visibility, anchor corner. Returns
/// `true` if the draft must be committed — see the head doc of
/// [`plot_settings_window`].
fn legend_editor(ui: &mut egui::Ui, legend: &mut stokesforge_plot::LegendSpec) -> bool {
    form(ui, "plot_legend_page", |form| {
        let mut commit = form.checkbox(&mut legend.visible, tr!("plot-legend-show"), None);
        commit |= form.row(tr!("plot-legend-position"), |ui| {
            choice(
                ui,
                "plot_config_legend_position",
                &mut legend.position,
                &[
                    LegendPosition::TopLeft,
                    LegendPosition::TopRight,
                    LegendPosition::BottomLeft,
                    LegendPosition::BottomRight,
                ]
                .map(|candidate| ChoiceOption::new(candidate, legend_position_label(candidate))),
            )
        });
        commit
    })
}

/// Label of a [`PlotType`], including [`PlotType::Unknown`] (out of
/// caution: this arm exists so that the function stays total, but
/// `series_editor` never shows it — an unknown type does not go through the
/// dropdown menu that calls this function).
fn plot_type_label(plot_type: &PlotType) -> String {
    match plot_type {
        PlotType::Scatter => tr!("plot-type-scatter"),
        PlotType::Line => tr!("plot-type-line"),
        PlotType::LineScatter => tr!("plot-type-line-symbol"),
        PlotType::Bar => tr!("plot-type-bar"),
        PlotType::Histogram => tr!("plot-type-histogram"),
        PlotType::ErrorBar => tr!("plot-type-error-bars"),
        PlotType::Unknown(_) => tr!("plot-type-unknown"),
    }
}

/// Label of a [`LineStyle`].
pub(super) fn line_style_label(style: LineStyle) -> String {
    match style {
        LineStyle::None => tr!("plot-line-style-none"),
        LineStyle::Solid => tr!("plot-line-style-solid"),
        LineStyle::Dash => tr!("plot-line-style-dashed"),
        LineStyle::Dot => tr!("plot-line-style-dotted"),
        LineStyle::DashDot => tr!("plot-line-style-dash-dot"),
    }
}

/// Label of a [`SymbolShape`].
pub(super) fn symbol_shape_label(shape: SymbolShape) -> String {
    match shape {
        SymbolShape::None => tr!("plot-symbol-none"),
        SymbolShape::Circle => tr!("plot-symbol-circle"),
        SymbolShape::Square => tr!("plot-symbol-square"),
        SymbolShape::Triangle => tr!("plot-symbol-triangle"),
        SymbolShape::Diamond => tr!("plot-symbol-diamond"),
        SymbolShape::Plus => tr!("plot-symbol-plus"),
        SymbolShape::Cross => tr!("plot-symbol-cross"),
    }
}

/// Label of a [`LegendPosition`].
fn legend_position_label(position: LegendPosition) -> String {
    match position {
        LegendPosition::TopLeft => tr!("plot-legend-top-left"),
        LegendPosition::TopRight => tr!("plot-legend-top-right"),
        LegendPosition::BottomLeft => tr!("plot-legend-bottom-left"),
        LegendPosition::BottomRight => tr!("plot-legend-bottom-right"),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use stokesforge_core::{Column, ColumnMeta, MetaLine, NodeId, Project};
    use stokesforge_plot::{
        LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, RawVariant, SeriesAxis,
        SeriesSpec, SymbolShape, THEMES,
    };
    use stokesforge_render_egui::formula;
    use stokesforge_session::Session;

    use super::{
        column_metadata_window, form, formula_field, formula_help, formula_memo_rows,
        legend_position_label, line_style_label, plot_settings_window, plot_type_label,
        symbol_shape_label, theme_editor, TickDirection, FORMULA_HELP_HIDDEN,
    };
    use crate::color_picker::PickerEnv;
    use stokesforge_app_state::i18n::{self, pseudo, Language, LANGUAGE_VARIABLE};
    use stokesforge_app_state::state::{
        series_list_label, Action, AppState, PlotAxis, PlotSettingsTab,
    };
    use stokesforge_app_state::tr;

    use super::super::test_support::{
        self, click, distinct, image, key, text_exact, texts, xy_columns, Text,
    };

    // -- Tooling ----------------------------------------------------------

    fn is_painted(texts: &[Text], wanted: &str) -> bool {
        texts.iter().any(|t| t.text == wanted)
    }

    fn click_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// The first panel, mutable (`Panels` can only be indexed for reading).
    fn first_panel(spec: &mut PlotSpec) -> &mut PanelSpec {
        spec.panels.get_mut(0).unwrap()
    }

    /// A one-series graph (B as a function of A) in a session, with the
    /// properties window closed. `adjust` tunes the graph before the
    /// session takes it; the fields keep alive what the state depends on.
    struct Harness {
        state: AppState,
        graph: NodeId,
        _session: Session,
    }

    fn harness(adjust: impl FnOnce(&mut PlotSpec)) -> Harness {
        let mut project = Project::new("Properties").unwrap();
        let root = project.root();
        let (x, y) = xy_columns(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let mut spec = PlotSpec::default().with_panels(panel);
        adjust(&mut spec);
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        Harness {
            state,
            graph,
            _session: session,
        }
    }

    impl Harness {
        /// Opens the properties window on `tab` (and `axis`), like the
        /// right-click or the "Graph" menu.
        fn open(&mut self, tab: PlotSettingsTab, axis: Option<PlotAxis>) {
            self.state.dispatch(Action::OpenPlotSettings {
                graph: self.graph,
                tab,
                axis,
            });
            assert_eq!(self.state.plot_settings().map(|s| s.tab), Some(tab));
        }

        /// The graph as the model holds it (not the draft).
        fn model(&self) -> PlotSpec {
            self.state.plot_spec(self.graph).unwrap()
        }

        /// The window's draft.
        fn draft(&self) -> PlotSpec {
            self.state.plot_edit().unwrap().spec.clone()
        }
    }

    /// Paints the properties window three times (layout settled,
    /// dialog centered) then a fourth with `events`; returns the texts of
    /// the last frame.
    fn paint_window(
        context: &egui::Context,
        state: &mut AppState,
        events: Vec<egui::Event>,
    ) -> Vec<Text> {
        for _ in 0..3 {
            image(context, Vec::new(), |ui| {
                plot_settings_window(ui.ctx(), state)
            });
        }
        texts(&image(context, events, |ui| {
            plot_settings_window(ui.ctx(), state)
        }))
    }

    /// Clicks on the text `label` of the window (a single occurrence),
    /// then returns the texts of the next frame.
    fn click_label(context: &egui::Context, state: &mut AppState, label: &str) -> Vec<Text> {
        let painted = paint_window(context, state, Vec::new());
        let pos = text_exact(&painted, label).center;
        click(context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        paint_window(context, state, Vec::new())
    }

    /// Scrolls the axes page to the bottom (the wheel over the text
    /// `hover`, painted before the scroll); the offset is kept by egui
    /// for the next frames.
    fn scroll_down(context: &egui::Context, state: &mut AppState, hover: &str) -> Vec<Text> {
        let painted = paint_window(context, state, Vec::new());
        let pos = text_exact(&painted, hover).center;
        paint_window(
            context,
            state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0.0, -1000.0),
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        paint_window(context, state, Vec::new())
    }

    /// The text `wanted` painted on the same row as `row` (within a few
    /// points of height): "Auto" appears there several times.
    fn in_the_row<'a>(texts: &'a [Text], row: &str, wanted: &str) -> &'a Text {
        let y = text_exact(texts, row).center.y;
        texts
            .iter()
            .find(|t| t.text == wanted && (t.center.y - y).abs() < 4.0)
            .unwrap_or_else(|| panic!("{wanted:?} absent from row {row:?}: {texts:?}"))
    }

    // -- Each tab, in each language -----------------------------------------

    /// A label specific to each tab: painted on its page, absent from the
    /// others. Chosen not to coincide with any other text of the six pages
    /// ("Axes" is both the tab and a theme color, "Line" is both
    /// a section and a plot type).
    fn own_labels() -> [(PlotSettingsTab, String); 6] {
        [
            (PlotSettingsTab::Graph, tr!("plot-general-dark-note")),
            (
                PlotSettingsTab::PlotArea,
                tr!("plot-area-gridlines-vertical"),
            ),
            (PlotSettingsTab::Axes, tr!("plot-axes-scale-section")),
            (PlotSettingsTab::Series, tr!("plot-series-line-width")),
            (PlotSettingsTab::Legend, tr!("plot-legend-show")),
            (PlotSettingsTab::Theme, tr!("plot-theme-global-colors")),
        ]
    }

    /// Issue #119 (before/after screenshots): each tab draws without
    /// panicking, shows the common strip of the six tabs, **its own** content
    /// and not that of the others, and changes nothing — neither the model,
    /// nor the draft, nor the history, nor the chosen tab. In the pseudo-locale
    /// (`STOKESFORGE_LANG=en-XA`), every painted text that is not data goes
    /// through the catalog: it is framed by `[[ ]]` (a missing `]]` would
    /// mean a text truncated by the layout).
    ///
    /// The language is the process's: it is
    /// [`each_tab_draws_in_english_and_in_the_pseudo_locale`] that reruns it
    /// in a subprocess with `en` then `en-XA` (the language is a global state:
    /// changing it here would disturb the neighboring tests). It reads the line
    /// "tabs painted in …" to know that the checks ran in the right language.
    #[test]
    #[ignore = "run in a subprocess by each_tab_draws_in_english_and_in_the_pseudo_locale"]
    fn each_tab_draws_without_changing_anything_in_the_process_language() {
        let language = i18n::language();
        let mut checked = 0;
        for tab in PlotSettingsTab::ALL {
            // A Y axis title colored differently from the graph title:
            // the "Theme" tab must not merge them into a single color
            // just by being drawn.
            let mut harness = harness(|spec| {
                spec.set_text_color("#336699");
                first_panel(spec).left.title_color = "#aa0000".into();
                first_panel(spec).left.tick_color = "#00aa00".into();
            });
            let history = harness.state.undo_label();
            harness.open(tab, None);
            let original = harness.model();
            let context = test_support::context();
            let painted = paint_window(&context, &mut harness.state, Vec::new());

            // The strip scrolls when it does not fit (#19): in English, the
            // six tabs fit; in the pseudo-locale, 30 % longer, only the
            // chosen tab is sure to be visible.
            assert!(
                is_painted(&painted, &tab.label()),
                "{tab:?}: the chosen tab is not visible in the strip"
            );
            if language == Language::English {
                for other in PlotSettingsTab::ALL {
                    assert!(
                        is_painted(&painted, &other.label()),
                        "{tab:?}: tab {other:?} missing from the strip"
                    );
                }
            }
            for (owner, label) in own_labels() {
                assert_eq!(
                    is_painted(&painted, &label),
                    owner == tab,
                    "{tab:?}: {label:?} belongs to {owner:?}"
                );
            }
            assert_eq!(harness.model(), original, "{tab:?}: the model changed");
            assert_eq!(harness.draft(), original, "{tab:?}: the draft changed");
            assert_eq!(harness.state.undo_label(), history, "{tab:?}: history");
            let settings = harness.state.plot_settings().expect("window stayed open");
            assert_eq!(settings.tab, tab);

            // Data does not go through the catalog: the series label taken
            // from its columns, numeric values, empty input fields (titles
            // not filled in).
            let tree = harness.state.tree().clone();
            let series = series_list_label(&tree, &original.panels[0].series[0]);
            let is_data =
                |text: &str| text.is_empty() || text == series || text.parse::<f64>().is_ok();
            for item in &painted {
                if language == Language::PseudoEnglish && !is_data(&item.text) {
                    assert!(
                        item.text.starts_with(pseudo::OPEN) && item.text.ends_with(pseudo::CLOSE),
                        "{tab:?}: {:?} did not go through the catalog (or is truncated)",
                        item.text
                    );
                } else {
                    assert!(
                        !item.text.contains(pseudo::OPEN),
                        "{tab:?}: {:?} in the pseudo-locale outside en-XA",
                        item.text
                    );
                }
                checked += 1;
            }
        }
        println!("tabs painted in {}: {checked} texts", language.tag());
    }

    /// Reruns [`each_tab_draws_without_changing_anything_in_the_process_language`]
    /// in a child process per language, with `STOKESFORGE_LANG` set: English,
    /// then the pseudo-locale. The child must succeed **and** say that it
    /// painted in the requested language (witness: a child that ran nothing,
    /// or in another language, would not write it).
    #[test]
    fn each_tab_draws_in_english_and_in_the_pseudo_locale() {
        let executable = std::env::current_exe().unwrap();
        let module = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, rest)| rest);
        let name =
            format!("{module}::each_tab_draws_without_changing_anything_in_the_process_language");
        for language in [Language::English, Language::PseudoEnglish] {
            let output = std::process::Command::new(&executable)
                .args([
                    name.as_str(),
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(LANGUAGE_VARIABLE, language.tag())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "{}:\n{stdout}\n{stderr}",
                language.tag()
            );
            assert!(
                stdout.contains(&format!("tabs painted in {}: ", language.tag())),
                "{}: the child did not paint in this language:\n{stdout}",
                language.tag()
            );
        }
    }

    // -- The tab strip and the axes tab -------------------------------------

    /// #19: the properties tabs are the common strip (`tab_strip`);
    /// a click on one of them changes the page (witness: as long as one does not click,
    /// the page stays the same, frame after frame).
    #[test]
    fn a_click_on_a_properties_tab_changes_the_page() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Graph, None);
        let context = test_support::context();
        let axes = tr!("plot-axes-scale-section");
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(!is_painted(&painted, &axes));
        assert_eq!(
            harness.state.plot_settings().unwrap().tab,
            PlotSettingsTab::Graph
        );

        let painted = click_label(&context, &mut harness.state, &PlotSettingsTab::Axes.label());
        assert_eq!(
            harness.state.plot_settings().unwrap().tab,
            PlotSettingsTab::Axes
        );
        assert!(is_painted(&painted, &axes), "the axes page is painted");
    }

    /// The "Axes" tab offers X, Y and the two mirror axes (#176); an
    /// overlaid layer has no mirror. A click on an axis chooses it, without
    /// changing anything.
    #[test]
    fn the_axes_tab_chooses_the_axis_and_offers_the_mirrors() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, None);
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &PlotAxis::Bottom.label()));
        assert!(is_painted(&painted, &PlotAxis::Left.label()));
        assert!(is_painted(&painted, &PlotAxis::Right.label()));
        assert!(is_painted(&painted, &PlotAxis::Top.label()));
        assert_eq!(
            harness.state.plot_settings().unwrap().axis,
            PlotAxis::Bottom
        );

        let original = harness.model();
        click_label(&context, &mut harness.state, &PlotAxis::Left.label());
        assert_eq!(harness.state.plot_settings().unwrap().axis, PlotAxis::Left);
        assert_eq!(harness.model(), original);

        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Right));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &PlotAxis::Right.label()));
    }

    /// S-L9 (#393): the axes are the secondary tabs, a column at the left of
    /// the page (below the Graph Properties tabs), in the order Bottom, Left,
    /// Right, Top.
    #[test]
    fn the_axis_tabs_are_a_column_at_the_left_of_the_page() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, None);
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let at = |axis: PlotAxis| text_exact(&painted, &axis.label()).center;
        let (bottom, left, right, top) = (
            at(PlotAxis::Bottom),
            at(PlotAxis::Left),
            at(PlotAxis::Right),
            at(PlotAxis::Top),
        );
        assert!(left.y > bottom.y + 10.0, "stacked");
        assert!(right.y > left.y + 10.0, "stacked");
        assert!(top.y > right.y + 10.0, "stacked");
        let tabs_right = [bottom, left, right, top]
            .iter()
            .map(|c| c.x)
            .fold(0.0, f32::max);
        let graph = text_exact(&painted, &PlotSettingsTab::Graph.label()).center;
        assert!(graph.y < bottom.y, "the axes are under the main tabs");
        let scale = text_exact(&painted, &tr!("plot-axes-scale-section")).center;
        assert!(scale.x > tabs_right, "page on the right of the tabs");
    }

    /// A graph with the ordinary layer 1 and an overlaid layer 2 (#396) whose
    /// axes are `overlay`; the properties show layer `selected`.
    fn open_with_overlay(
        overlay: stokesforge_plot::OverlaySpec,
        selected: usize,
        axis: Option<PlotAxis>,
        adjust: impl FnOnce(&mut PlotSpec),
    ) -> Harness {
        let mut harness = harness(|spec| {
            let base = spec.panels.first().unwrap().clone();
            spec.panels.push(PanelSpec::overlay_on(&base, overlay));
            adjust(spec);
        });
        harness.open(PlotSettingsTab::Axes, axis);
        harness.state.plot_edit_mut().unwrap().selected_panel = selected;
        harness
    }

    /// Hovers the text `label` of the window and returns the texts painted
    /// with the bubble.
    fn hover_label(context: &egui::Context, state: &mut AppState, label: &str) -> Vec<Text> {
        context.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let painted = paint_window(context, state, Vec::new());
        let pos = text_exact(&painted, label).center;
        paint_window(context, state, vec![egui::Event::PointerMoved(pos)]);
        paint_window(context, state, vec![egui::Event::PointerMoved(pos)])
    }

    /// #396: an overlaid layer shows the four tabs; Bottom and Left belong to
    /// the layer it covers, so they are greyed out, say why in their bubble,
    /// and a click on them does not leave the chosen side.
    #[test]
    fn the_tabs_of_an_overlaid_layer_are_four_and_the_covered_sides_are_greyed() {
        let overlay = stokesforge_plot::OverlaySpec::new(true, true);
        let mut harness = open_with_overlay(overlay, 1, Some(PlotAxis::Right), |_| {});
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        for axis in [
            PlotAxis::Bottom,
            PlotAxis::Left,
            PlotAxis::Right,
            PlotAxis::Top,
        ] {
            assert!(is_painted(&painted, &axis.label()), "{axis:?}");
        }
        let reason = tr!(
            "disabled-axis-side-base-x",
            layer = 1,
            tab = PlotAxis::Top.label()
        );
        assert!(!is_painted(&painted, &reason), "witness: no bubble yet");
        let hovered = hover_label(&context, &mut harness.state, &PlotAxis::Bottom.label());
        assert!(is_painted(&hovered, &reason), "{reason:?}");
        click_label(&context, &mut harness.state, &PlotAxis::Bottom.label());
        assert_eq!(
            harness.state.plot_settings().unwrap().axis,
            PlotAxis::Right,
            "a greyed tab does not select"
        );
        click_label(&context, &mut harness.state, &PlotAxis::Top.label());
        assert_eq!(harness.state.plot_settings().unwrap().axis, PlotAxis::Top);
    }

    /// #396: the Bottom tab being greyed on an overlaid layer, the page opens
    /// on the other side of the same axis, the Top tab.
    #[test]
    fn a_greyed_chosen_tab_falls_back_to_the_other_side_of_the_axis() {
        let overlay = stokesforge_plot::OverlaySpec::new(true, true);
        let mut harness = open_with_overlay(overlay, 1, Some(PlotAxis::Left), |_| {});
        let context = test_support::context();
        paint_window(&context, &mut harness.state, Vec::new());
        assert_eq!(harness.state.plot_settings().unwrap().axis, PlotAxis::Right);
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        paint_window(&context, &mut harness.state, Vec::new());
        assert_eq!(harness.state.plot_settings().unwrap().axis, PlotAxis::Top);
    }

    /// #396: the Top tab of an overlaid layer edits its X axis (the `bottom`
    /// field), never the `top` one; "Show axis" hides it and the layer keeps
    /// its own scale, hence its curves.
    #[test]
    fn the_top_tab_of_an_overlaid_layer_edits_its_x_axis() {
        let overlay = stokesforge_plot::OverlaySpec::new(true, true);
        let mut harness = open_with_overlay(overlay, 1, Some(PlotAxis::Top), |_| {});
        let context = test_support::context();
        let before = harness.draft();
        click_label(&context, &mut harness.state, &tr!("plot-axes-show-axis"));
        let after = harness.draft();
        let layer = after.panels.get(1).unwrap();
        assert!(!layer.bottom.visible, "the X axis is hidden");
        assert_eq!(layer.top, before.panels.get(1).unwrap().top);
        assert_eq!(layer.overlay, Some(overlay), "its scale is kept");
        assert_eq!(harness.model(), after, "committed");
    }

    /// #396: "Own scale" is the layer's `x_top`; unticked, the scale fields and
    /// "Show axis" are greyed with the way out in their bubble.
    #[test]
    fn own_scale_unticked_greys_the_scale_fields_and_says_how_to_tick_it() {
        let overlay = stokesforge_plot::OverlaySpec::new(true, true);
        let mut harness = open_with_overlay(overlay, 1, Some(PlotAxis::Top), |_| {});
        let context = test_support::context();
        click_label(&context, &mut harness.state, &tr!("plot-axes-own-scale"));
        let layer = harness.draft().panels.get(1).unwrap().clone();
        assert_eq!(layer.overlay.map(|o| o.x_top), Some(false));
        assert_eq!(harness.model().panels.get(1), Some(&layer), "committed");
        let reason = tr!("disabled-axis-own-scale-off");
        let hovered = hover_label(&context, &mut harness.state, &tr!("plot-axes-show-axis"));
        assert!(is_painted(&hovered, &reason), "{reason:?}");
        // Greyed: a click on Show axis changes nothing.
        let before = harness.draft();
        click_label(&context, &mut harness.state, &tr!("plot-axes-show-axis"));
        assert_eq!(harness.draft(), before);
    }

    /// #396: the Top of the ordinary layer is greyed while an overlaid layer
    /// draws its X axis there; hiding that axis gives the tab back.
    #[test]
    fn the_top_of_the_covered_layer_is_greyed_while_an_overlay_draws_there() {
        let overlay = stokesforge_plot::OverlaySpec::new(true, false);
        let mut harness = open_with_overlay(overlay, 0, Some(PlotAxis::Top), |_| {});
        let context = test_support::context();
        paint_window(&context, &mut harness.state, Vec::new());
        assert_eq!(
            harness.state.plot_settings().unwrap().axis,
            PlotAxis::Bottom,
            "Top is taken: the other side of the axis"
        );
        let reason = tr!(
            "disabled-axis-side-overlay-x",
            layer = 2,
            tab = PlotAxis::Top.label()
        );
        let hovered = hover_label(&context, &mut harness.state, &PlotAxis::Top.label());
        assert!(is_painted(&hovered, &reason), "{reason:?}");
        // The overlay hides its axis: the Top tab is selectable again.
        harness
            .state
            .plot_edit_mut()
            .unwrap()
            .spec
            .panels
            .get_mut(1)
            .unwrap()
            .bottom
            .visible = false;
        click_label(&context, &mut harness.state, &PlotAxis::Top.label());
        assert_eq!(harness.state.plot_settings().unwrap().axis, PlotAxis::Top);
    }

    /// S-L9 (#393): the axis strip stays fixed, only the page scrolls.
    #[test]
    fn the_axis_strip_does_not_scroll_with_the_page() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let context = test_support::context();
        let before = paint_window(&context, &mut harness.state, Vec::new());
        let tab = text_exact(&before, &PlotAxis::Bottom.label()).center;
        let first_row = tr!("plot-axes-scale-section");
        assert!(is_painted(&before, &first_row));
        let after = scroll_down(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-gridlines"),
        );
        assert_eq!(text_exact(&after, &PlotAxis::Bottom.label()).center, tab);
        assert!(!is_painted(&after, &first_row), "the page moved");
    }

    /// The axes page fills the height of the window: its scroll area is not
    /// squeezed to one row by the row that holds the strip.
    #[test]
    fn the_axes_page_fills_the_height_of_the_window() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let scale = text_exact(&painted, &tr!("plot-axes-scale-section"))
            .center
            .y;
        let grid = text_exact(&painted, &tr!("plot-axes-show-gridlines"))
            .center
            .y;
        assert!(grid > scale + 80.0, "several rows are laid out");
    }

    /// #176: the tab of a mirror axis offers neither title nor color, only
    /// to show it and to keep or not the tick labels.
    #[test]
    fn the_tab_of_a_mirror_axis_offers_only_display() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Top));
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-axes-show-mirror")));
        assert!(is_painted(&painted, &tr!("plot-axes-show-tick-labels")));
        assert!(!is_painted(&painted, &tr!("plot-axes-quantity")));
        assert!(!is_painted(&painted, &tr!("plot-axes-color")));

        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-axes-quantity")));
        assert!(!is_painted(&painted, &tr!("plot-axes-show-mirror")));
    }

    /// #344: the page of an axis no longer has "Reverse axis" or "Auto range"
    /// (the direction is read from the From / To order, the Auto button gives back to the data),
    /// nor "Closed Frame"; it has a "Ticks" section whose
    /// "Advanced" block is closed, then opened on click (and closed again).
    #[test]
    fn an_axis_page_has_its_ticks_and_a_folded_advanced_block() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-axes-from")));
        assert!(is_painted(&painted, &tr!("plot-axes-from-auto")));
        for removed in ["Reverse axis", "Auto range", "Closed Frame"] {
            assert!(
                !is_painted(&painted, removed),
                "{removed:?} no longer exists"
            );
        }
        let painted = scroll_down(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-gridlines"),
        );
        assert!(is_painted(&painted, &tr!("plot-axes-ticks-section")));
        assert!(is_painted(&painted, &tr!("plot-axes-show-minor-ticks")));
        assert!(is_painted(&painted, &tr!("common-advanced")));
        let advanced_labels = [
            tr!("plot-axes-tick-length"),
            tr!("plot-axes-minor-tick-length"),
            tr!("plot-axes-tick-width"),
            tr!("plot-axes-tick-mark-color"),
        ];
        for label in &advanced_labels {
            assert!(!is_painted(&painted, label), "{label:?}: closed by default");
        }
        let click_advanced = |harness: &mut Harness| {
            let pos = text_exact(
                &scroll_down(
                    &context,
                    &mut harness.state,
                    &tr!("plot-axes-show-gridlines"),
                ),
                &tr!("common-advanced"),
            )
            .center;
            click(&context, pos, egui::PointerButton::Primary, |ui| {
                plot_settings_window(ui.ctx(), &mut harness.state)
            });
            scroll_down(
                &context,
                &mut harness.state,
                &tr!("plot-axes-show-gridlines"),
            )
        };
        let painted = click_advanced(&mut harness);
        for label in &advanced_labels {
            assert!(is_painted(&painted, label), "{label:?}: opened on click");
        }
        assert_eq!(harness.model(), harness.draft(), "opening changes nothing");
        let painted = click_advanced(&mut harness);
        assert!(
            !is_painted(&painted, &advanced_labels[0]),
            "second click: closed again"
        );
    }

    /// #344: the "Advanced" block opens exactly once on a click on the
    /// arrow or in the space between the arrow and the title (a single
    /// clickable area: an arrow that toggled by itself, in addition to the title,
    /// would open it then close it in the same pass).
    #[test]
    fn a_click_on_the_advanced_block_arrow_opens_it_once() {
        for offset in [10.0, 2.0] {
            let mut harness = harness(|_| {});
            harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
            let context = test_support::context();
            let title = text_exact(
                &scroll_down(
                    &context,
                    &mut harness.state,
                    &tr!("plot-axes-show-gridlines"),
                ),
                &tr!("common-advanced"),
            )
            .center;
            let width = context.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        tr!("common-advanced"),
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x
            });
            // Left of the title's edge: the arrow, then the space that separates them.
            let point = egui::pos2(title.x - width / 2.0 - offset, title.y);
            click(&context, point, egui::PointerButton::Primary, |ui| {
                plot_settings_window(ui.ctx(), &mut harness.state)
            });
            let painted = scroll_down(
                &context,
                &mut harness.state,
                &tr!("plot-axes-show-gridlines"),
            );
            assert!(
                is_painted(&painted, &tr!("plot-axes-tick-length")),
                "opened at {offset} px from the title"
            );
        }
    }

    /// Publishes the bounds that the draw would have kept for the bottom axis.
    fn publish_bottom_bounds(context: &egui::Context, harness: &Harness, bounds: (f64, f64)) {
        super::super::graph_window::set_published_axis_bounds(
            context,
            harness.graph,
            vec![super::super::graph_window::PanelBounds {
                index: 0,
                bottom: Some(bounds),
                ..Default::default()
            }],
        );
    }

    /// Types `typed` into the numeric field that displays `value`, commits on
    /// Enter.
    fn type_into(context: &egui::Context, harness: &mut Harness, value: &str, typed: &str) {
        focus_field(context, &mut harness.state, value);
        one_frame(
            context,
            &mut harness.state,
            vec![egui::Event::Text(typed.into())],
        );
        one_frame(context, &mut harness.state, vec![key(egui::Key::Enter)]);
        one_frame(context, &mut harness.state, Vec::new());
    }

    /// #394: General › Size shows the plot area width, its height and the size
    /// of the graph in the unit of the Preferences (centimetres: a 392 x 284 pt
    /// plot area is 13.83 x 10.02, the 480 x 360 pt graph it makes with its two
    /// axes 16.93 x 12.7), and typing a width stores points and leaves the height alone, in
    /// the one step of the window.
    #[test]
    fn the_size_fields_show_the_unit_and_store_points() {
        let context = test_support::context();
        let mut harness = harness(|spec| {
            spec.frame_widths = vec![392.0];
            spec.frame_heights = vec![284.0];
        });
        harness.open(PlotSettingsTab::Graph, None);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        for label in [
            tr!("plot-size"),
            tr!("plot-size-width", unit = "cm"),
            tr!("plot-size-height", unit = "cm"),
            tr!("plot-size-total", unit = "cm"),
            "13.8289".to_owned(),
            "10.0189".to_owned(),
            "16.93 × 12.7".to_owned(),
        ] {
            assert!(is_painted(&painted, &label), "{label:?} not painted");
        }
        let history = harness.state.undo_label();

        type_into(&context, &mut harness, "13.8289", "10");
        let model = harness.model();
        assert!(
            (model.frame_widths[0] - 283.464_566_9).abs() < 1e-6,
            "{model:?}"
        );
        assert_eq!(model.frame_heights, [284.0]);
        // The window is one history entry, closed with it.
        harness.state.close_plot_settings();
        assert_ne!(harness.state.undo_label(), history, "an undoable entry");
    }

    /// #394: editing one size of a graph from an old file (no stored frame)
    /// first gives it the frames it is drawn with, so the other axis keeps its
    /// size.
    #[test]
    fn editing_a_size_of_a_legacy_graph_keeps_the_other_axis() {
        let context = test_support::context();
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Graph, None);
        assert!(harness.model().frame_widths.is_empty());
        type_into(&context, &mut harness, "10.0189", "5");
        let model = harness.model();
        assert_eq!(model.frame_widths, [392.0], "drawn width stored as it was");
        assert!(
            (model.frame_heights[0] - 141.732_283).abs() < 1e-5,
            "{model:?}"
        );
    }

    /// #405: the figure size lives in the Advanced block of General, closed by
    /// default (S-L8); opening it changes nothing, and choosing Fixed commits
    /// the draft in one entry, a graph from an old file being given its frames
    /// in the same edit.
    #[test]
    fn the_figure_size_choice_is_in_a_folded_advanced_block_and_commits_the_draft() {
        let context = test_support::context();
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Graph, None);
        assert!(harness.model().frame_widths.is_empty(), "an old graph");
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("common-advanced")));
        for hidden in [
            tr!("plot-figure-size"),
            tr!("plot-figure-size-grows"),
            tr!("plot-figure-size-fixed"),
        ] {
            assert!(!is_painted(&painted, &hidden), "{hidden:?}: closed");
        }
        let painted = scroll_down(&context, &mut harness.state, &tr!("plot-size"));
        let pos = text_exact(&painted, &tr!("common-advanced")).center;
        click(&context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state)
        });
        let painted = scroll_down(&context, &mut harness.state, &tr!("plot-size"));
        assert!(is_painted(&painted, &tr!("plot-figure-size")));
        assert!(is_painted(&painted, &tr!("plot-figure-size-grows")));
        assert_eq!(harness.model(), harness.draft(), "opening changes nothing");
        assert!(!harness.model().fixed_figure);

        let history = harness.state.undo_label();
        let pos = text_exact(&painted, &tr!("plot-figure-size-fixed")).center;
        click(&context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state)
        });
        let model = harness.model();
        assert!(model.fixed_figure, "committed with the click");
        assert_eq!(model.frame_widths, [392.0], "the old graph's frames fixed");
        harness.state.close_plot_settings();
        assert_ne!(harness.state.undo_label(), history, "an undoable entry");
    }

    /// #405: General › Size has Left and Top next to the plot area size; they
    /// store points, a value below what the axes need is raised to it, and Auto
    /// is what the field shows when empty.
    #[test]
    fn left_and_top_store_points_and_are_raised_to_the_minimum() {
        let context = test_support::context();
        let mut harness = harness(|spec| {
            spec.frame_widths = vec![392.0];
            spec.frame_heights = vec![284.0];
            spec.figure_top = Some(200.0);
        });
        harness.open(PlotSettingsTab::Graph, None);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        for label in [
            tr!("plot-size-left", unit = "cm"),
            tr!("plot-size-top", unit = "cm"),
        ] {
            assert!(is_painted(&painted, &label), "{label:?} not painted");
        }
        assert_eq!(harness.model().figure_left, None, "Auto by default");

        // 5 cm = 141.7 pt, above the 76 pt of the left axis.
        type_into(&context, &mut harness, &tr!("common-auto"), "5");
        let model = harness.model();
        let left = model.figure_left.expect("stored");
        assert!((left - 141.732_283).abs() < 1e-5, "{model:?}");
        // 1 cm = 28 pt is below the 76 pt the axis needs: raised to it.
        type_into(&context, &mut harness, "5", "1");
        let left = harness.model().figure_left.expect("stored");
        assert!((left - 76.0).abs() < 0.01, "{left}");
    }

    /// #398: the window opens on the active layer of the graph, on every tab,
    /// and keeps the user's choice afterwards.
    #[test]
    fn the_window_opens_on_the_active_layer() {
        let context = test_support::context();
        for tab in [PlotSettingsTab::Graph, PlotSettingsTab::Legend] {
            let mut harness = harness(|spec| {
                let first = first_panel(spec).clone();
                *spec = spec.clone().with_panels(stokesforge_plot::Panels::new([
                    first,
                    PanelSpec::at(0, 1),
                    PanelSpec::at(0, 2),
                ]));
            });
            let graph = harness.graph;
            context.data_mut(|data| {
                data.insert_temp(super::super::graph_window::active_panel_id(graph), 2usize)
            });
            harness.open(tab, None);
            paint_window(&context, &mut harness.state, Vec::new());
            assert_eq!(harness.state.plot_edit().unwrap().selected_panel, 2);
            harness.state.plot_edit_mut().unwrap().selected_panel = 1;
            paint_window(&context, &mut harness.state, Vec::new());
            assert_eq!(harness.state.plot_edit().unwrap().selected_panel, 1);
        }
    }

    /// A graph of two layers side by side: 300 x 250 pt and 220 x 250 pt plot
    /// areas on the grid, or in free placement shifted far from the stored
    /// origin (so a raw stored coordinate and a distance from the image edge
    /// differ).
    fn two_layers(spec: &mut PlotSpec, free: bool) {
        let first = first_panel(spec).clone();
        *spec = spec
            .clone()
            .with_panels(stokesforge_plot::Panels::new([first, PanelSpec::at(0, 1)]));
        spec.frame_widths = vec![300.0, 220.0];
        spec.frame_heights = vec![250.0];
        if free {
            assert!(stokesforge_app_state::frame_sizes::set_placement_in_draft(
                spec, true
            ));
            for index in 0..2 {
                let panel = spec.panels.get_mut(index).unwrap();
                let frame = panel.free_frame.as_mut().unwrap();
                frame.left += 1000.0;
                frame.top += 700.0;
            }
        }
    }

    /// A length in points as a number field of the window shows it in
    /// centimetres: six significant digits, trailing zeros dropped.
    fn cm(points: f64) -> String {
        let value = stokesforge_plot::LengthUnit::Cm.from_points(points);
        let decimals = if value.abs() < 1.0 {
            6
        } else {
            (5 - value.abs().log10().floor() as i32).max(0) as usize
        };
        let text = format!("{value:.decimals$}");
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text
        }
    }

    /// #398: switching General > Size to Free moves nothing (every plot area and
    /// the image keep their place and size), switching back applies the grid
    /// rules.
    #[test]
    fn the_placement_choice_switches_without_moving_anything_and_back_to_the_grid() {
        let context = test_support::context();
        let mut harness = harness(|spec| two_layers(spec, false));
        harness.open(PlotSettingsTab::Graph, None);
        let grid =
            stokesforge_app_state::frame_sizes::figure_layout(&harness.model()).expect("layout");
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        for label in [
            tr!("plot-size-placement"),
            tr!("plot-size-placement-grid"),
            tr!("plot-size-placement-free"),
        ] {
            assert!(is_painted(&painted, &label), "{label:?} not painted");
        }
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-size-placement-free"),
        );
        let free = harness.model();
        assert_eq!(free.layer_placement, stokesforge_plot::LayerPlacement::Free);
        let layout = stokesforge_app_state::frame_sizes::figure_layout(&free).expect("layout");
        assert_eq!(layout.size, grid.size, "the image does not change");
        for cell in &layout.free_cells {
            assert_eq!(
                Some(cell.frame),
                grid.frame_rect(cell.row, cell.column),
                "plot area of ({}, {})",
                cell.row,
                cell.column
            );
        }

        // The four fields show, the grid ones are gone.
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        for label in [
            tr!("plot-size-left", unit = "cm"),
            tr!("plot-size-top", unit = "cm"),
            tr!("plot-size-width", unit = "cm"),
            tr!("plot-size-height", unit = "cm"),
            tr!("plot-size-total", unit = "cm"),
        ] {
            assert!(is_painted(&painted, &label), "{label:?} not painted");
        }
        assert!(
            !is_painted(&painted, &tr!("plot-size-column-width", n = 1, unit = "cm")),
            "no grid field in Free"
        );

        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-size-placement-grid"),
        );
        let back = harness.model();
        assert_eq!(back.layer_placement, stokesforge_plot::LayerPlacement::Grid);
        assert_eq!(back.frame_widths, [300.0, 220.0]);
        assert_eq!(back.frame_heights, [250.0]);
        assert_eq!((back.panels[1].row, back.panels[1].column), (0, 1));
        harness.state.close_plot_settings();
    }

    /// #398: the four fields belong to the layer chosen in the list, store
    /// points from the unit of the Preferences, and show Left as the distance
    /// from the edge of the image, not the stored coordinate.
    #[test]
    fn the_free_fields_are_those_of_the_selected_layer_in_the_unit_of_the_preferences() {
        let context = test_support::context();
        let mut harness = harness(|spec| two_layers(spec, true));
        harness.open(PlotSettingsTab::Graph, None);
        harness.state.plot_edit_mut().unwrap().selected_panel = 1;
        let draft = harness.draft();
        let layout = stokesforge_app_state::frame_sizes::figure_layout(&draft).expect("layout");
        let fields =
            stokesforge_app_state::frame_sizes::free_fields(&draft, &layout, 1).expect("fields");
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &cm(220.0)), "width of layer 2");
        assert!(
            !is_painted(&painted, &cm(300.0)),
            "not the width of layer 1"
        );
        assert!(is_painted(&painted, &cm(fields.left)), "Left of layer 2");
        let raw = draft.panels[1].free_frame.unwrap().left;
        assert!(
            raw > 1000.0 && !is_painted(&painted, &cm(raw)),
            "not the stored value"
        );

        // 5 cm typed: stored in points, on layer 2 only.
        type_into(&context, &mut harness, &cm(220.0), "5");
        let model = harness.model();
        let width = |index: usize| model.panels[index].free_frame.unwrap().width;
        assert!((width(1) - 141.732_283).abs() < 1e-5, "{}", width(1));
        assert_eq!(width(0), 300.0, "layer 1 untouched");
        harness.state.close_plot_settings();
    }

    /// #344: under Auto, From and To show the computed bounds; typing in one
    /// freezes both (the other is taken from the computation), in **one** history
    /// entry, and the direction follows the order: From > To inverts the axis.
    #[test]
    fn typing_a_bound_under_auto_freezes_both_and_the_direction_follows_the_order() {
        let context = test_support::context();
        for (value, typed, bounds, inverted) in [
            ("5", "3", (3.0, 20.0), false),
            ("20", "1", (5.0, 1.0), true),
            ("5", "30", (30.0, 20.0), true),
        ] {
            let mut harness = harness(|_| {});
            harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
            publish_bottom_bounds(&context, &harness, (5.0, 20.0));
            let original = harness.model();
            assert!(original.panels[0].bottom.auto_range);
            let painted = paint_window(&context, &mut harness.state, Vec::new());
            assert!(
                is_painted(&painted, "5") && is_painted(&painted, "20"),
                "{painted:?}"
            );

            type_into(&context, &mut harness, value, typed);
            let axis = harness.model().panels[0].bottom.clone();
            assert!(!axis.auto_range, "{value}->{typed}: the input freezes");
            assert_eq!(
                (axis.range_min, axis.range_max),
                (Some(bounds.0), Some(bounds.1)),
                "{value}->{typed}"
            );
            assert_eq!(axis.inverted, inverted, "{value}->{typed}: direction");
            harness.state.close_plot_settings();
            harness.state.dispatch(Action::Undo);
            assert_eq!(harness.model(), original, "a single history entry");
        }
    }

    /// #344: the Auto button gives the bounds back to the data and **keeps the direction**;
    /// an inverted axis under Auto shows its upper bound in the From field.
    #[test]
    fn the_auto_button_gives_back_to_the_data_keeping_the_direction() {
        let context = test_support::context();
        let mut harness = harness(|spec| {
            let axis = &mut first_panel(spec).bottom;
            axis.auto_range = false;
            axis.inverted = true;
            axis.range_min = Some(20.0);
            axis.range_max = Some(5.0);
        });
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        publish_bottom_bounds(&context, &harness, (5.0, 20.0));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let button = in_the_row(
            &painted,
            &tr!("plot-axes-from"),
            &tr!("plot-axes-from-auto"),
        )
        .center;
        click(&context, button, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state)
        });
        let axis = harness.model().panels[0].bottom.clone();
        assert!(axis.auto_range);
        assert!(axis.inverted, "the direction is kept");
        assert_eq!((axis.range_min, axis.range_max), (Some(20.0), Some(5.0)));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(
            text_exact(&painted, "20").center.x < text_exact(&painted, "5").center.x,
            "inverted axis: From shows the upper bound"
        );
    }

    /// #344: the "Advanced" settings (length of tick marks, width) are written
    /// into the axis; emptying a field makes it Auto; they are specific to the
    /// chosen axis.
    #[test]
    fn advanced_tick_settings_are_written_per_axis() {
        let context = test_support::context();
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let open = scroll_down(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-gridlines"),
        );
        let pos = text_exact(&open, &tr!("common-advanced")).center;
        click(&context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state)
        });
        let painted = scroll_down(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-gridlines"),
        );
        let field = in_the_row(&painted, &tr!("plot-axes-tick-length"), &tr!("common-auto")).center;
        click(&context, field, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state)
        });
        one_frame(
            &context,
            &mut harness.state,
            vec![egui::Event::Text("9".into())],
        );
        one_frame(&context, &mut harness.state, vec![key(egui::Key::Enter)]);
        one_frame(&context, &mut harness.state, Vec::new());
        let first_panel = &harness.model().panels[0];
        assert_eq!(first_panel.bottom.tick_length, Some(9.0));
        assert_eq!(
            first_panel.left.tick_length, None,
            "specific to the bottom axis"
        );
        assert_eq!(first_panel.bottom.minor_tick_length, None);
        assert_eq!(first_panel.bottom.tick_width, None);
    }

    /// #344: ticks and their direction are set per axis, mirrors included;
    /// a mirror turned off or ticks turned off gray out the direction choice.
    #[test]
    fn ticks_and_their_direction_are_set_per_axis() {
        let context = test_support::context();
        // The full editor (bottom, left, real right axis).
        let mut axis = stokesforge_plot::AxisSpec::default();
        let (_, painted) = paint_axis(&context, &mut axis, None);
        assert!(is_painted(&painted, &tr!("plot-axes-show-ticks")));
        assert!(is_painted(&painted, &tr!("plot-axes-tick-direction")));

        // The top mirror, short page: all the controls are painted.
        let mut harness = harness(|spec| first_panel(spec).top.mirror = true);
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Top));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-axes-show-ticks")));
        assert!(is_painted(&painted, &tr!("plot-axes-tick-direction")));
        click_label(&context, &mut harness.state, &tr!("plot-axes-tick-in"));
        assert_eq!(
            harness.model().panels[0].top.tick_direction,
            TickDirection::In
        );
        click_label(&context, &mut harness.state, &tr!("plot-axes-tick-both"));
        assert_eq!(
            harness.model().panels[0].top.tick_direction,
            TickDirection::Both
        );
        click_label(&context, &mut harness.state, &tr!("plot-axes-show-ticks"));
        assert!(!harness.model().panels[0].top.ticks);
        click_label(&context, &mut harness.state, &tr!("plot-axes-tick-in"));
        assert_eq!(
            harness.model().panels[0].top.tick_direction,
            TickDirection::Both,
            "ticks off: the direction choice is grayed out"
        );
        click_label(&context, &mut harness.state, &tr!("plot-axes-show-ticks"));
        assert!(harness.model().panels[0].top.ticks);
        click_label(&context, &mut harness.state, &tr!("plot-axes-tick-in"));
        assert_eq!(
            harness.model().panels[0].top.tick_direction,
            TickDirection::In
        );

        // Minor ticks: on by default, independent of the major tick marks.
        assert!(is_painted(&painted, &tr!("plot-axes-show-minor-ticks")));
        assert!(harness.model().panels[0].top.minor_ticks);
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-minor-ticks"),
        );
        assert!(!harness.model().panels[0].top.minor_ticks);
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-minor-ticks"),
        );
        assert!(harness.model().panels[0].top.minor_ticks);
        click_label(&context, &mut harness.state, &tr!("plot-axes-show-ticks"));
        assert!(!harness.model().panels[0].top.ticks);
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-axes-show-minor-ticks"),
        );
        assert!(
            !harness.model().panels[0].top.minor_ticks,
            "without major tick marks, the minor ticks can still be set"
        );
    }

    /// #124: the page picks the draft's layer; the first layer
    /// serves as a witness that must stay unchanged after the click on the grid.
    #[test]
    fn the_layer_page_commits_only_the_chosen_layer() {
        let mut harness = harness(|spec| {
            first_panel(spec).bottom.grid = false;
            let mut second = PanelSpec::at(0, 1);
            second.title = "Chosen layer".into();
            second.bottom.grid = false;
            spec.panels.push(second);
        });
        harness.open(PlotSettingsTab::PlotArea, None);
        harness.state.plot_edit_mut().unwrap().selected_panel = usize::MAX;
        let original = harness.model();
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, "Chosen layer"));
        assert_eq!(harness.state.plot_edit().unwrap().selected_panel, 1);
        assert_eq!(
            harness.model(),
            original,
            "the choice alone commits nothing"
        );

        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-area-gridlines-vertical"),
        );
        let after = harness.model();
        assert_eq!(after.panels[0], original.panels[0]);
        assert!(
            after.panels[1].bottom.grid,
            "the click committed the chosen layer"
        );
        harness.state.close_plot_settings();
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original, "the commit stays undoable");
    }

    /// #169: the chosen axis is the active tab of a `tab_strip` (bold
    /// and accent), the chosen series a bold list row, and the
    /// "Line" / "Symbol" sections bold titles — no longer selectable
    /// buttons nor ordinary labels preceded by a rule.
    /// Witnesses: the unchosen axis and series stay in normal weight.
    #[test]
    fn the_chosen_axis_is_a_tab_and_the_chosen_series_a_list_row() {
        let mut harness = harness(|spec| {
            let panel = first_panel(spec);
            panel.series[0].name = "First".into();
            let mut second = panel.series[0].clone();
            second.name = "Second".into();
            panel.series.push(second);
        });
        let context = test_support::context();
        harness.open(PlotSettingsTab::Axes, Some(PlotAxis::Left));
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let left = text_exact(&painted, &PlotAxis::Left.label());
        assert!(left.bold, "the chosen axis is bold");
        assert_eq!(left.color, crate::theme::accent(), "and in the accent");
        let bottom = text_exact(&painted, &PlotAxis::Bottom.label());
        assert!(!bottom.bold, "witness: the unchosen axis");
        assert_ne!(bottom.color, crate::theme::accent());

        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_series = 1;
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let spec = harness.draft();
        let tree = harness.state.tree();
        let label = |index: usize| series_list_label(tree, &spec.panels[0].series[index]);
        let rows = |index: usize| -> Vec<&Text> {
            painted.iter().filter(|t| t.text == label(index)).collect()
        };
        assert!(
            rows(1).iter().any(|t| t.bold),
            "the chosen series is a bold row: {:?}",
            rows(1)
        );
        assert!(
            rows(0).iter().all(|t| !t.bold),
            "witness: the unchosen series"
        );
        for section in [
            tr!("plot-series-line-section"),
            tr!("plot-series-symbol-section"),
        ] {
            assert!(
                painted.iter().any(|t| t.text == section && t.bold),
                "{section}: a bold section title"
            );
        }
    }

    /// #124: the early return on an empty list must leave the
    /// dialog open; a non-empty list bounds the selection without writing
    /// the graph, then its editor changes only the chosen series.
    #[test]
    fn the_series_page_bounds_the_selection_and_keeps_the_empty_case() {
        let mut empty = harness(|spec| first_panel(spec).series.clear());
        empty.open(PlotSettingsTab::Series, None);
        empty.state.plot_edit_mut().unwrap().selected_series = usize::MAX;
        let original = empty.model();
        let painted = paint_window(&test_support::context(), &mut empty.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-series-empty")));
        assert!(!is_painted(&painted, &tr!("plot-series-show")));
        assert_eq!(empty.model(), original);
        assert!(empty.state.plot_settings().is_some());
        assert_eq!(empty.state.plot_edit().unwrap().selected_series, usize::MAX);

        let mut harness = harness(|spec| {
            let panel = first_panel(spec);
            panel.series[0].name = "First".into();
            let mut second = panel.series[0].clone();
            second.name = "Second".into();
            panel.series.push(second);
        });
        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_series = usize::MAX;
        let original = harness.model();
        let context = test_support::context();
        paint_window(&context, &mut harness.state, Vec::new());
        assert_eq!(harness.state.plot_edit().unwrap().selected_series, 1);
        assert_eq!(harness.model(), original);
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-series-show-in-legend"),
        );
        let after = harness.model();
        assert_eq!(after.panels[0].series[0], original.panels[0].series[0]);
        assert!(!after.panels[0].series[1].show_in_legend);
        assert_eq!(after.panels[0].series[1].name, "Second");
    }

    /// #177–#179: the series have their window — the graph's strip no longer
    /// offers them — which shows the group of the chosen series; choosing a
    /// color gradient spreads it at once over its own series only (no
    /// "Apply" button, #213), down to the model.
    #[test]
    fn the_series_window_sets_a_group_and_applies_a_color_gradient_to_it() {
        let mut harness = harness(|spec| {
            let panel = first_panel(spec);
            let model = panel.series[0].clone();
            panel.series.clear();
            panel.push_series_batch(vec![model.clone(), model.clone()]);
            panel.push_series_batch(vec![model]);
        });
        let context = test_support::context();

        harness.open(PlotSettingsTab::Graph, None);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(!is_painted(&painted, &PlotSettingsTab::Series.label()));

        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_series = 1;
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(!is_painted(&painted, &PlotSettingsTab::Graph.label()));
        assert!(is_painted(&painted, &tr!("curve-group-section")));
        assert!(
            !is_painted(&painted, &tr!("curve-scope-group-note")),
            "the scope sentence is under the '?' icon, no longer a note"
        );
        assert!(
            is_painted(&painted, &tr!("plot-series-plot-type")),
            "the type stays settable"
        );
        let original = harness.model();

        // #213: a new group has no color gradient — the list is empty (no
        // gradient name painted), nothing is colored — and, since the group is
        // set as a whole, the individual color is not offered.
        assert!(harness.model().panels[0].group_gradients.is_empty());
        assert!(!is_painted(&painted, &tr!("curve-gradient-blue-red")));
        assert!(!is_painted(&painted, &tr!("plot-series-line-color")));
        assert!(!is_painted(&painted, &tr!("plot-series-symbol-color")));

        // The list has no text to click while it is empty: click it
        // to the left of the Delete button that follows it.
        let open = |context: &egui::Context, state: &mut AppState| {
            let painted = paint_window(context, state, Vec::new());
            let pos = text_exact(&painted, &tr!("curve-gradient-delete")).center;
            click(
                context,
                pos - egui::vec2(70.0, 0.0),
                egui::PointerButton::Primary,
                |ui| plot_settings_window(ui.ctx(), state),
            );
        };
        open(&context, &mut harness.state);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(
            is_painted(&painted, &tr!("curve-gradient-blue-red")),
            "the list opens on its color gradients"
        );
        assert_eq!(harness.model(), original, "opening the list colors nothing");
        click_label(
            &context,
            &mut harness.state,
            &tr!("curve-gradient-black-red"),
        );
        let after = harness.model();
        let colors: Vec<&str> = after.panels[0]
            .series
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(colors[..2], ["#000000", "#ff0000"]);
        assert_eq!(
            after.panels[0].series[2], original.panels[0].series[2],
            "the series of another group is untouched"
        );
        assert_eq!(
            after.panels[0].group_gradient(1).map(<[String]>::len),
            Some(2),
            "the group keeps its color gradient"
        );
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(
            is_painted(&painted, &tr!("curve-gradient-black-red")),
            "the list shows the group's color gradient"
        );

        // #213: the color swatch of an entry is clicked like its name.
        open(&context, &mut harness.state);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let name = text_exact(&painted, &tr!("curve-gradient-black-blue")).center;
        click(
            &context,
            name - egui::vec2(80.0, 0.0),
            egui::PointerButton::Primary,
            |ui| plot_settings_window(ui.ctx(), &mut harness.state),
        );
        let colors: Vec<String> = harness.model().panels[0]
            .series
            .iter()
            .map(|s| s.line.color.clone())
            .collect();
        assert_eq!(colors[..2], ["#000000", "#0000ff"], "click on the swatch");

        // "This series" scope: the individual color comes back.
        let painted = click_label(&context, &mut harness.state, &tr!("curve-scope-series"));
        assert!(is_painted(&painted, &tr!("plot-series-line-color")));

        // A series outside any group has no gradient to set, but its color.
        harness.state.plot_edit_mut().unwrap().selected_series = 2;
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(!is_painted(&painted, &tr!("curve-gradient-section")));
        assert!(is_painted(&painted, &tr!("plot-series-line-color")));
    }

    /// #177: "whole group" scope — the settings specific to a series are
    /// grayed out (the click does nothing); "this series only" scope — they
    /// reopen and touch only the chosen series.
    #[test]
    fn the_scope_grays_out_the_settings_specific_to_a_series() {
        let mut harness = harness(|spec| {
            let panel = first_panel(spec);
            let model = panel.series[0].clone();
            panel.series.clear();
            panel.push_series_batch(vec![model.clone(), model]);
        });
        let context = test_support::context();
        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_series = 1;
        let original = harness.model();

        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-series-show-in-legend"),
        );
        assert_eq!(
            harness.model(),
            original,
            "grayed out: the click changes nothing"
        );

        click_label(&context, &mut harness.state, &tr!("curve-scope-series"));
        click_label(
            &context,
            &mut harness.state,
            &tr!("plot-series-show-in-legend"),
        );
        let after = harness.model();
        assert!(
            after.panels[0].series[0].show_in_legend,
            "the other member is intact"
        );
        assert!(!after.panels[0].series[1].show_in_legend);
        assert_eq!(
            after.panels[0].series[1].group,
            Some(1),
            "stays in its group"
        );
    }

    /// #124: checks the return of the graph page up to the common
    /// commit, in addition to the local test of `formula_field`. A keystroke must
    /// only change the draft, then a click elsewhere in the page
    /// must reach the model without closing the dialog.
    #[test]
    fn the_graph_title_commits_after_focus_loss() {
        let mut harness = harness(|spec| spec.title = "Initial title".into());
        harness.open(PlotSettingsTab::Graph, None);
        let original = harness.model();
        let context = test_support::context();
        click_label(&context, &mut harness.state, "Initial title");
        image(&context, vec![egui::Event::Text("Z".into())], |ui| {
            plot_settings_window(ui.ctx(), &mut harness.state);
        });
        let typed = harness.draft().title;
        assert!(typed.contains('Z'), "the keystroke reached the draft");
        assert_eq!(harness.model(), original, "no commit while typing");
        click_label(&context, &mut harness.state, &tr!("plot-general-dark-note"));
        assert_eq!(harness.model().title, typed);
        assert!(
            harness.state.plot_settings().is_some(),
            "the commit comes from the open page"
        );
        harness.state.close_plot_settings();
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original);
    }

    // -- When the draft is committed ----------------------------------------

    /// Clicks in the numeric field that displays `value` (focus, whole text
    /// selected), then returns the field's center for the rest of the gesture.
    fn focus_field(context: &egui::Context, state: &mut AppState, value: &str) -> egui::Pos2 {
        let painted = paint_window(context, state, Vec::new());
        let center = text_exact(&painted, value).center;
        click(context, center, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        center
    }

    fn one_frame(context: &egui::Context, state: &mut AppState, events: Vec<egui::Event>) {
        image(context, events, |ui| plot_settings_window(ui.ctx(), state));
    }

    /// A typed number commits the draft only on Enter (S-N3) — as long as the
    /// field is open, neither the draft nor the model moves; on Enter,
    /// the model takes the draft. Line width (series) and minimum
    /// bound (axis).
    #[test]
    fn a_typed_number_is_committed_only_on_enter() {
        struct Case {
            tab: PlotSettingsTab,
            axis: Option<PlotAxis>,
            value: &'static str,
            typed: &'static str,
            expected: f64,
            read: fn(&PlotSpec) -> f64,
        }
        let cases = [
            Case {
                tab: PlotSettingsTab::Series,
                axis: None,
                value: "2.5",
                typed: "3.5",
                expected: 3.5,
                read: |s| s.panels[0].series[0].line.width,
            },
            Case {
                tab: PlotSettingsTab::Axes,
                axis: Some(PlotAxis::Left),
                value: "-3.5",
                typed: "-1",
                expected: -1.0,
                read: |s| s.panels[0].left.range_min.unwrap(),
            },
        ];
        for Case {
            tab,
            axis,
            value,
            typed,
            expected,
            read,
        } in cases
        {
            let mut harness = harness(|spec| {
                first_panel(spec).series[0].line.width = 2.5;
                let left = &mut first_panel(spec).left;
                left.auto_range = false;
                left.range_min = Some(-3.5);
                left.range_max = Some(12.5);
            });
            harness.open(tab, axis);
            let before = read(&harness.model());
            assert_eq!(Ok(before), value.parse(), "{tab:?}: the displayed value");
            let context = test_support::context();
            focus_field(&context, &mut harness.state, value);
            one_frame(
                &context,
                &mut harness.state,
                vec![egui::Event::Text(typed.into())],
            );
            one_frame(&context, &mut harness.state, Vec::new());
            assert_eq!(
                read(&harness.draft()),
                before,
                "{tab:?}: committed on keystroke"
            );
            assert_eq!(read(&harness.model()), before, "{tab:?}: model touched");

            one_frame(&context, &mut harness.state, vec![key(egui::Key::Enter)]);
            assert_eq!(read(&harness.draft()), expected, "{tab:?}: Enter");
            assert_eq!(read(&harness.model()), expected, "{tab:?}: model");
        }
    }

    /// Wheel over the list field: one notch = the neighboring proposed value
    /// (2.5 → 3 → 4, then 4 at the ends); three notches in a row make
    /// **one** history entry, a single Undo gives back the previous value.
    #[test]
    fn wheel_notches_make_only_one_history_entry() {
        let mut harness = harness(|spec| first_panel(spec).series[0].line.width = 2.5);
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        let original = harness.model();
        focus_field(&context, &mut harness.state, "2.5");
        let notch = || {
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        for _ in 0..3 {
            one_frame(&context, &mut harness.state, notch());
        }
        one_frame(&context, &mut harness.state, Vec::new());
        let width = |spec: &PlotSpec| spec.panels[0].series[0].line.width;
        assert_eq!(width(&harness.draft()), 4.0);
        harness.state.close_plot_settings();
        assert_eq!(width(&harness.model()), 4.0);
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original, "a single Undo");
    }

    // -- Bins and bar width (#359) -----------------------------------------

    /// The graph of the tests below: one series of type `plot_type`.
    fn typed_harness(plot_type: PlotType) -> Harness {
        let mut harness = harness(|spec| first_panel(spec).series[0].plot_type = plot_type);
        harness.open(PlotSettingsTab::Series, None);
        harness
    }

    /// The "Auto (...)" text of the bar width, if painted.
    fn auto_width_text(painted: &[Text]) -> Option<&Text> {
        painted.iter().find(|t| t.text.starts_with("Auto ("))
    }

    /// Types `typed` in the field showing `value`, then Enter.
    fn type_in(harness: &mut Harness, context: &egui::Context, value: &str, typed: &str) {
        focus_field(context, &mut harness.state, value);
        one_frame(
            context,
            &mut harness.state,
            vec![egui::Event::Text(typed.into())],
        );
        one_frame(context, &mut harness.state, vec![key(egui::Key::Enter)]);
        one_frame(context, &mut harness.state, Vec::new());
    }

    /// Bins (Histogram only) and Width (Bar only): a Line series has neither.
    #[test]
    fn bins_and_width_follow_the_plot_type() {
        let context = test_support::context();
        let bins = tr!("plot-series-bins");
        let width = tr!("plot-series-bar-width");
        for (plot_type, has_bins, has_width) in [
            (PlotType::Line, false, false),
            (PlotType::Bar, false, true),
            (PlotType::Histogram, true, false),
        ] {
            let mut harness = typed_harness(plot_type.clone());
            let painted = paint_window(&context, &mut harness.state, Vec::new());
            assert_eq!(is_painted(&painted, &bins), has_bins, "{plot_type:?}: Bins");
            assert_eq!(
                is_painted(&painted, &width),
                has_width,
                "{plot_type:?}: Width"
            );
        }
    }

    /// Drawing the series page of a Bar or Histogram series writes nothing:
    /// neither Width (still Auto, `None`) nor Bins, in the draft, the model or
    /// the history. (The "Series" tab is not in `PlotSettingsTab::ALL`: the
    /// test of the tabs does not draw it.)
    #[test]
    fn the_bins_and_width_fields_write_nothing_when_only_drawn() {
        for plot_type in [PlotType::Bar, PlotType::Histogram] {
            let mut harness = typed_harness(plot_type.clone());
            let history = harness.state.undo_label();
            let original = harness.model();
            let context = test_support::context();
            let painted = paint_window(&context, &mut harness.state, Vec::new());
            if plot_type == PlotType::Bar {
                assert!(auto_width_text(&painted).is_some(), "Auto (...) painted");
            }
            assert_eq!(harness.draft(), original, "{plot_type:?}: draft");
            assert_eq!(harness.model(), original, "{plot_type:?}: model");
            assert_eq!(
                harness.state.undo_label(),
                history,
                "{plot_type:?}: history"
            );
        }
    }

    /// Typing 25 + Enter sets the bins in the draft and the model, in a single
    /// history entry.
    #[test]
    fn typed_bins_reach_the_model_in_one_history_entry() {
        let mut harness = typed_harness(PlotType::Histogram);
        let context = test_support::context();
        let original = harness.model();
        let default = original.panels[0].series[0]
            .histogram_bins
            .get()
            .to_string();
        type_in(&mut harness, &context, &default, "25");
        let bins = |spec: &PlotSpec| spec.panels[0].series[0].histogram_bins.get();
        assert_eq!(bins(&harness.draft()), 25);
        assert_eq!(bins(&harness.model()), 25);
        harness.state.close_plot_settings();
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original, "a single Undo");
    }

    /// 2.5 is refused (red frame, nothing written); 0 and 65537 are clamped
    /// to the bounds (orange frame).
    #[test]
    fn bins_refuse_a_fraction_and_clamp_to_the_bounds() {
        let frames = |context: &egui::Context, harness: &mut Harness, color| {
            let shapes = image(context, Vec::new(), |ui| {
                plot_settings_window(ui.ctx(), &mut harness.state)
            });
            shapes
                .iter()
                .filter(|p| matches!(&p.shape, egui::Shape::Rect(r) if r.stroke.color == color))
                .count()
        };
        // A fresh context per case: the field keeps its text in it.
        let context = test_support::context();
        let mut harness = typed_harness(PlotType::Histogram);
        let original = harness.model();
        let default = original.panels[0].series[0]
            .histogram_bins
            .get()
            .to_string();
        type_in(&mut harness, &context, &default, "2.5");
        assert_eq!(harness.model(), original, "2.5 writes nothing");
        assert_eq!(
            frames(&context, &mut harness, crate::theme::field_refused_color()),
            1
        );

        let context = test_support::context();
        let mut harness = typed_harness(PlotType::Histogram);
        type_in(&mut harness, &context, &default, "0");
        let bins = |harness: &Harness| harness.model().panels[0].series[0].histogram_bins.get();
        assert_eq!(bins(&harness), 1);
        assert_eq!(
            frames(&context, &mut harness, crate::theme::field_clamped_color()),
            1
        );

        let context = test_support::context();
        let mut harness = typed_harness(PlotType::Histogram);
        type_in(&mut harness, &context, &default, "65537");
        assert_eq!(bins(&harness), stokesforge_plot::MAX_HISTOGRAM_BINS);
        assert_eq!(
            frames(&context, &mut harness, crate::theme::field_clamped_color()),
            1
        );
    }

    /// Width: starts Auto (`None`, gray "Auto (...)"), a typed value fixes it,
    /// `auto` and an emptied field give it back; one history entry each.
    #[test]
    fn the_bar_width_goes_from_auto_to_a_value_and_back() {
        let mut harness = typed_harness(PlotType::Bar);
        let context = test_support::context();
        let width = |spec: &PlotSpec| spec.panels[0].series[0].bar_width;
        assert_eq!(width(&harness.model()), None);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let auto = auto_width_text(&painted).expect("Auto (...) painted");

        // Auto -> 0.5.
        focus_field(&context, &mut harness.state, &auto.text);
        one_frame(
            &context,
            &mut harness.state,
            vec![egui::Event::Text("0.5".into())],
        );
        one_frame(&context, &mut harness.state, vec![key(egui::Key::Enter)]);
        assert_eq!(width(&harness.draft()), Some(0.5));
        assert_eq!(width(&harness.model()), Some(0.5));

        // The word `auto` -> Auto.
        type_in(&mut harness, &context, "0.5", "auto");
        assert_eq!(width(&harness.model()), None, "`auto`");

        // 0.5 again, then an emptied field -> Auto.
        let auto = auto_width_text(&paint_window(&context, &mut harness.state, Vec::new()))
            .expect("Auto (...) painted")
            .text
            .clone();
        type_in(&mut harness, &context, &auto, "0.5");
        assert_eq!(width(&harness.model()), Some(0.5));
        focus_field(&context, &mut harness.state, "0.5");
        one_frame(
            &context,
            &mut harness.state,
            vec![key(egui::Key::Backspace)],
        );
        one_frame(&context, &mut harness.state, vec![key(egui::Key::Enter)]);
        one_frame(&context, &mut harness.state, Vec::new());
        assert_eq!(width(&harness.model()), None, "emptied field");

        // Four commits, four history entries: four Undo give the start back.
        harness.state.close_plot_settings();
        for _ in 0..4 {
            harness.state.dispatch(Action::Undo);
        }
        assert_eq!(width(&harness.model()), None);
        assert!(harness.state.undo_label().is_none(), "history exhausted");
    }

    /// "Whole group": Bins reach the Histogram members only, Width the Bar
    /// members only; the plot type of the members stays what it was.
    #[test]
    fn bins_and_width_are_copied_to_the_members_of_the_matching_type() {
        let make = |first: PlotType| {
            harness(|spec| {
                let panel = first_panel(spec);
                let mut model = panel.series[0].clone();
                panel.series.clear();
                let mut series = Vec::new();
                for plot_type in [first.clone(), first, PlotType::Line] {
                    model.plot_type = plot_type;
                    series.push(model.clone());
                }
                // Third member: the other kind, to prove it is skipped.
                series[2].plot_type = if series[0].plot_type == PlotType::Bar {
                    PlotType::Histogram
                } else {
                    PlotType::Bar
                };
                panel.push_series_batch(series);
            })
        };
        let context = test_support::context();

        let mut histogram = make(PlotType::Histogram);
        histogram.open(PlotSettingsTab::Series, None);
        histogram.state.plot_edit_mut().unwrap().selected_series = 0;
        let default = histogram.model().panels[0].series[1]
            .histogram_bins
            .get()
            .to_string();
        type_in(&mut histogram, &context, &default, "25");
        let after = histogram.model();
        let series = &after.panels[0].series;
        assert_eq!(series[0].histogram_bins.get(), 25);
        assert_eq!(series[1].histogram_bins.get(), 25, "Histogram member");
        assert_eq!(
            series[2].histogram_bins.get().to_string(),
            default,
            "Bar member untouched"
        );
        assert_eq!(series[2].plot_type, PlotType::Bar, "type stays per series");

        let mut bar = make(PlotType::Bar);
        bar.open(PlotSettingsTab::Series, None);
        bar.state.plot_edit_mut().unwrap().selected_series = 0;
        let painted = paint_window(&context, &mut bar.state, Vec::new());
        let auto = auto_width_text(&painted).expect("Auto (...)").text.clone();
        type_in(&mut bar, &context, &auto, "0.5");
        let after = bar.model();
        let series = &after.panels[0].series;
        assert_eq!(series[0].bar_width, Some(0.5));
        assert_eq!(series[1].bar_width, Some(0.5), "Bar member");
        assert_eq!(series[2].bar_width, None, "Histogram member untouched");
    }

    // -- Y axis of a series (#358) ----------------------------------------

    /// A graph whose right axis is hidden (a new panel shows it as a mirror).
    fn hidden_right(spec: &mut PlotSpec) {
        first_panel(spec).right.visible = false;
    }

    fn right_visible(spec: &PlotSpec) -> bool {
        spec.panels[0].right.visible
    }

    /// Clicking "Right" attaches the series to the right axis and shows it, in
    /// one history entry: a single Undo gives back the left axis **and** the
    /// hidden right axis.
    #[test]
    fn choosing_the_right_axis_shows_it_in_one_history_entry() {
        let mut harness = harness(hidden_right);
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        let original = harness.model();
        assert!(!right_visible(&original));
        assert_eq!(original.panels[0].series[0].axis, SeriesAxis::Left);
        click_label(&context, &mut harness.state, &tr!("plot-axis-right"));
        let after = harness.model();
        assert_eq!(after.panels[0].series[0].axis, SeriesAxis::Right);
        assert!(right_visible(&after), "the right axis is shown");
        harness.state.close_plot_settings();
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original, "a single Undo");
        assert!(!right_visible(&harness.model()));
    }

    /// Going back to "Left" hides nothing.
    #[test]
    fn going_back_to_left_keeps_the_right_axis_shown() {
        let mut harness = harness(|spec| {
            hidden_right(spec);
            first_panel(spec).set_series_axis(0, SeriesAxis::Right);
        });
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        click_label(&context, &mut harness.state, &tr!("plot-axis-left"));
        let after = harness.model();
        assert_eq!(after.panels[0].series[0].axis, SeriesAxis::Left);
        assert!(right_visible(&after));
    }

    /// An overlaid layer: the field is painted, grayed out, with its reason;
    /// a click on "Right" changes nothing.
    #[test]
    fn the_axis_field_is_disabled_with_its_reason_on_an_overlaid_layer() {
        let mut harness = harness(|spec| {
            let base = first_panel(spec).clone();
            let mut layer =
                PanelSpec::overlay_on(&base, stokesforge_plot::OverlaySpec::new(true, true));
            layer.series.push(base.series[0].clone());
            spec.panels = stokesforge_plot::Panels::new(vec![base, layer]);
        });
        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_panel = 1;
        let context = test_support::context();
        context.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let original = harness.model();
        let reason = tr!("plot-series-axis", "disabled");
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-series-axis")));
        assert!(
            !is_painted(&painted, &reason),
            "witness: without hovering, no explanation"
        );
        let pos = text_exact(&painted, &tr!("plot-axis-right")).center;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen = texts(&image(
                &context,
                vec![egui::Event::PointerMoved(pos)],
                |ui| plot_settings_window(ui.ctx(), &mut harness.state),
            ));
        }
        assert!(is_painted(&seen, &reason), "hovering says why");
        click_label(&context, &mut harness.state, &tr!("plot-axis-right"));
        assert_eq!(harness.model(), original, "nothing changes");
    }

    /// "Whole group": the axis is copied to every member of the group, and
    /// the right axis is shown.
    #[test]
    fn the_axis_is_copied_to_the_whole_group() {
        let mut harness = harness(|spec| {
            hidden_right(spec);
            let panel = first_panel(spec);
            let model = panel.series[0].clone();
            panel.series.clear();
            panel.push_series_batch(vec![model.clone(), model.clone(), model]);
        });
        harness.open(PlotSettingsTab::Series, None);
        harness.state.plot_edit_mut().unwrap().selected_series = 0;
        let context = test_support::context();
        click_label(&context, &mut harness.state, &tr!("plot-axis-right"));
        let after = harness.model();
        assert!(after.panels[0]
            .series
            .iter()
            .all(|s| s.axis == SeriesAxis::Right));
        assert!(right_visible(&after));
    }

    /// S-F7: the first Escape gives the field its value back (the dialog stays open),
    /// the next one is the dialog's Cancel.
    #[test]
    fn escape_gives_back_the_value_then_cancels_the_dialog() {
        let mut harness = harness(|spec| first_panel(spec).series[0].line.width = 2.5);
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        focus_field(&context, &mut harness.state, "2.5");
        one_frame(
            &context,
            &mut harness.state,
            vec![egui::Event::Text("9".into())],
        );
        one_frame(&context, &mut harness.state, vec![key(egui::Key::Escape)]);
        one_frame(&context, &mut harness.state, Vec::new());
        assert!(
            harness.state.plot_settings().is_some(),
            "first Escape: the dialog stays"
        );
        assert_eq!(harness.draft().panels[0].series[0].line.width, 2.5);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(
            is_painted(&painted, "2.5"),
            "the value is given back to the field"
        );

        one_frame(&context, &mut harness.state, vec![key(egui::Key::Escape)]);
        assert!(
            harness.state.plot_settings().is_none(),
            "second Escape: Cancel"
        );
    }

    /// Enter in a Graph Properties number field (S-N3): commits the field into
    /// the model, the window stays open; a second Enter with no field focused
    /// is the dialog's OK and keeps the change.
    #[test]
    fn enter_in_a_number_field_commits_and_keeps_the_window_open() {
        let context = test_support::context();
        let mut harness = harness(|spec| {
            spec.frame_widths = vec![392.0];
            spec.frame_heights = vec![284.0];
        });
        harness.open(PlotSettingsTab::Graph, None);
        type_into(&context, &mut harness, "13.8289", "10");
        assert!(harness.state.plot_settings().is_some(), "(b) window open");
        assert!(
            (harness.model().frame_widths[0] - 283.46).abs() < 0.01,
            "(a, c)"
        );
        one_frame(&context, &mut harness.state, vec![key(egui::Key::Enter)]);
        one_frame(&context, &mut harness.state, Vec::new());
        // Pre-existing (dialogs.rs, not touched by #398): Enter with no field
        // focused is the dialog's OK; the change is kept, not lost.
        assert!(harness.state.plot_settings().is_none());
        assert!((harness.model().frame_widths[0] - 283.46).abs() < 0.01);
    }

    /// Hovering is enough (#235): without focus, a notch on the list field goes to
    /// the next proposed value.
    #[test]
    fn the_wheel_on_hover_without_focus_follows_the_list() {
        let mut harness = harness(|spec| first_panel(spec).series[0].line.width = 2.5);
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        let center = text_exact(&painted, "2.5").center;
        one_frame(
            &context,
            &mut harness.state,
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 1.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        one_frame(&context, &mut harness.state, Vec::new());
        assert_eq!(harness.draft().panels[0].series[0].line.width, 3.0);
    }

    /// Doc of [`plot_settings_window`]: a checkbox commits right
    /// away, on click.
    #[test]
    fn a_checkbox_commits_on_click() {
        let mut harness = harness(|_| {});
        harness.open(PlotSettingsTab::Legend, None);
        let context = test_support::context();
        let before = harness.model().panels[0].legend.visible;
        paint_window(&context, &mut harness.state, Vec::new());
        assert_eq!(harness.model().panels[0].legend.visible, before);
        click_label(&context, &mut harness.state, &tr!("plot-legend-show"));
        assert_eq!(harness.model().panels[0].legend.visible, !before);
    }

    /// The result of each frame of [`formula_field`] on `value`, and its
    /// texts painted in the last one.
    fn field(
        context: &egui::Context,
        value: &mut String,
        frames: Vec<Vec<egui::Event>>,
    ) -> (Vec<bool>, Vec<Text>) {
        let mut results = Vec::new();
        let mut painted = Vec::new();
        for events in frames {
            // A frame can take two passes (the form grid is
            // measured on the first): its result is that of its passes.
            let mut result = false;
            painted = texts(&image(context, events, |ui| {
                result |= form(ui, "trial", |form| {
                    formula_field(form, "Quantity", "Help", value)
                });
            }));
            results.push(result);
        }
        (results, painted)
    }

    /// Number of red frames ("refused") painted in `shapes`.
    fn red_frames(shapes: &[egui::epaint::ClippedShape]) -> usize {
        let red = crate::theme::field_refused_color();
        shapes
            .iter()
            .filter(|painted| match &painted.shape {
                egui::Shape::Rect(rect) => rect.stroke.color == red,
                _ => false,
            })
            .count()
    }

    /// S-T3: a Typst text that does not compile, once the field is left, is
    /// flagged by the red frame of a refusal; a valid text is not.
    #[test]
    fn a_typst_error_paints_the_fields_red_frame() {
        let context = test_support::context();
        let paint = |value: &mut String| {
            image(&context, vec![], |ui| {
                form(ui, "trial", |form| {
                    formula_field(form, "Quantity", "Help", value)
                });
            })
        };
        let mut valid = String::from("$lambda$ (nm)");
        let mut faulty = String::from("$ (unclosed");
        // Two frames: the form grid is measured on the first.
        paint(&mut valid);
        paint(&mut faulty);
        assert_eq!(red_frames(&paint(&mut valid)), 0);
        assert_eq!(red_frames(&paint(&mut faulty)), 1);
    }

    /// S-T3 in context: a faulty markup is typed, Enter leaves the field, the
    /// red frame appears.
    #[test]
    fn a_typed_then_committed_typst_error_turns_red() {
        let context = test_support::context();
        let mut value = String::from("abc");
        let paint = |value: &mut String, events: Vec<egui::Event>| {
            image(&context, events, |ui| {
                form(ui, "trial", |form| {
                    formula_field(form, "Quantity", "Help", value)
                });
            })
        };
        paint(&mut value, vec![]);
        let frame_shapes = paint(&mut value, vec![]);
        let inside = text_exact(&texts(&frame_shapes), "abc").center;
        paint(&mut value, vec![egui::Event::PointerMoved(inside)]);
        paint(&mut value, vec![click_event(inside, true)]);
        paint(&mut value, vec![click_event(inside, false)]);
        paint(&mut value, vec![egui::Event::Text("$x".into())]);
        assert!(value.contains("$x"), "{value:?}");
        assert_eq!(red_frames(&paint(&mut value, vec![])), 0, "while typing");
        paint(&mut value, vec![key(egui::Key::Enter)]);
        paint(&mut value, vec![]);
        assert_eq!(red_frames(&paint(&mut value, vec![])), 1);
    }

    /// [`formula_field`] commits only on focus loss — Enter or a click
    /// elsewhere —, never on each keystroke: the input reaches the value
    /// (preview) without asking for a commit.
    #[test]
    fn a_text_field_commits_only_on_focus_loss() {
        let context = test_support::context();
        let mut value = String::from("abc");
        let (results, painted) = field(&context, &mut value, vec![vec![], vec![]]);
        assert_eq!(results, [false, false]);
        assert!(is_painted(&painted, "Quantity"), "the label is painted");
        let inside = text_exact(&painted, "abc").center;
        let outside = egui::pos2(1000.0, 800.0);
        let focus_field = vec![
            vec![egui::Event::PointerMoved(inside)],
            vec![click_event(inside, true)],
            vec![click_event(inside, false)],
        ];

        // Enter.
        let (results, _) = field(&context, &mut value, focus_field.clone());
        assert_eq!(results, [false; 3], "taking focus does not commit");
        let (results, _) = field(
            &context,
            &mut value,
            vec![vec![egui::Event::Text("Z".into())], vec![]],
        );
        assert_eq!(results, [false, false], "a keystroke does not commit");
        assert!(
            value.contains('Z'),
            "the keystroke reaches the value: {value:?}"
        );
        // egui 0.36 reports focus loss over two frames (`lost_focus`:
        // focus at the start of this frame *or* the previous one); committing twice
        // has no effect (`AppState::commit_plot_edit` commits only a
        // draft that differs from the model).
        let (results, _) = field(
            &context,
            &mut value,
            vec![vec![key(egui::Key::Enter)], vec![], vec![]],
        );
        assert!(results[0], "Enter commits right away: {results:?}");
        assert!(!results[2], "then nothing more: {results:?}");

        // Click elsewhere.
        let (results, _) = field(&context, &mut value, focus_field);
        assert_eq!(results, [false; 3]);
        let (results, _) = field(
            &context,
            &mut value,
            vec![vec![egui::Event::Text("Y".into())]],
        );
        assert_eq!(results, [false]);
        let (results, _) = field(
            &context,
            &mut value,
            vec![
                vec![egui::Event::PointerMoved(outside)],
                vec![click_event(outside, true)],
                vec![click_event(outside, false)],
                vec![],
                vec![],
            ],
        );
        assert!(
            results.contains(&true),
            "the click elsewhere commits: {results:?}"
        );
        assert_eq!(
            results.last(),
            Some(&false),
            "then nothing more: {results:?}"
        );
        assert!(value.contains('Y'));
    }

    // -- The series tab -------------------------------------------------------

    /// Opens the dropdown list whose displayed text is `displayed`, in
    /// the open tab; returns the texts painted with the list open and the position
    /// of the closed button.
    fn open_list(
        context: &egui::Context,
        state: &mut AppState,
        displayed: &str,
    ) -> (Vec<Text>, egui::Pos2) {
        let painted = paint_window(context, state, Vec::new());
        let button = text_exact(&painted, displayed).center;
        click(context, button, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        (paint_window(context, state, Vec::new()), button)
    }

    /// Each dropdown list of the properties offers **all** its
    /// known variants (the unknown type never, see [`series_editor`]), and
    /// a choice in it is committed right away.
    #[test]
    fn each_dropdown_list_offers_all_its_variants() {
        type Reader = fn(&PlotSpec) -> String;
        let cases: [(PlotSettingsTab, String, Vec<String>, String, Reader); 4] = [
            (
                PlotSettingsTab::Series,
                plot_type_label(&PlotType::Scatter),
                known_types().iter().map(plot_type_label).collect(),
                plot_type_label(&PlotType::Bar),
                |s| format!("{:?}", s.panels[0].series[0].plot_type),
            ),
            (
                PlotSettingsTab::Series,
                line_style_label(LineStyle::Solid),
                styles().into_iter().map(line_style_label).collect(),
                line_style_label(LineStyle::Dash),
                |s| format!("{:?}", s.panels[0].series[0].line.style),
            ),
            (
                PlotSettingsTab::Series,
                symbol_shape_label(SymbolShape::Circle),
                symbol_shapes()
                    .into_iter()
                    .map(symbol_shape_label)
                    .collect(),
                symbol_shape_label(SymbolShape::Diamond),
                |s| format!("{:?}", s.panels[0].series[0].symbol.shape),
            ),
            (
                PlotSettingsTab::Legend,
                legend_position_label(LegendPosition::TopRight),
                positions().into_iter().map(legend_position_label).collect(),
                legend_position_label(LegendPosition::BottomLeft),
                |s| format!("{:?}", s.panels[0].legend.position),
            ),
        ];
        let expected_values = ["Bar", "Dash", "Diamond", "BottomLeft"];
        for ((tab, displayed, offered, choice, read), expected) in
            cases.into_iter().zip(expected_values)
        {
            let mut harness = harness(|spec| {
                let panel = first_panel(spec);
                panel.series[0].plot_type = PlotType::Scatter;
                panel.series[0].line.style = LineStyle::Solid;
                panel.series[0].symbol.shape = SymbolShape::Circle;
                panel.legend.position = LegendPosition::TopRight;
            });
            harness.open(tab, None);
            let context = test_support::context();
            let (painted, button) = open_list(&context, &mut harness.state, &displayed);
            for offer in &offered {
                assert!(
                    painted
                        .iter()
                        .any(|t| &t.text == offer && t.center != button),
                    "{displayed:?}: {offer:?} absent from the open list"
                );
            }
            assert!(!is_painted(&painted, &plot_type_label(&unknown())));
            assert_ne!(read(&harness.model()), expected, "witness: not chosen yet");
            let pos = text_exact(&painted, &choice).center;
            click(&context, pos, egui::PointerButton::Primary, |ui| {
                plot_settings_window(ui.ctx(), &mut harness.state)
            });
            assert_eq!(
                read(&harness.model()),
                expected,
                "{displayed:?}: choice committed"
            );
        }
    }

    /// C5 (`guide-de-correction-robustesse.md` §F3): a series of an unknown
    /// type (newer version) shows it read-only, without a dropdown
    /// list — nothing knows how to rebuild it —, and the rest of the series
    /// stays editable.
    #[test]
    fn an_unknown_plot_type_is_shown_read_only() {
        let mut harness = harness(|spec| first_panel(spec).series[0].plot_type = unknown());
        harness.open(PlotSettingsTab::Series, None);
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-series-type-unknown")));
        assert!(!is_painted(&painted, &plot_type_label(&unknown())));
        // No type list: no known type label is painted
        // (except "Line", which is also the title of the line section).
        let section = tr!("plot-series-line-section");
        for known in known_types() {
            let label = plot_type_label(&known);
            if label != section {
                assert!(!is_painted(&painted, &label), "{known:?}");
            }
        }
        assert!(is_painted(&painted, &tr!("plot-series-show-in-legend")));
        let painted = click_label(
            &context,
            &mut harness.state,
            &tr!("plot-series-show-in-legend"),
        );
        assert!(
            !harness.model().panels[0].series[0].show_in_legend,
            "still editable"
        );
        assert_eq!(harness.model().panels[0].series[0].plot_type, unknown());
        assert!(is_painted(&painted, &tr!("plot-series-type-unknown")));
    }

    // -- The "Theme" tab ------------------------------------------------------

    /// Paints [`theme_editor`] on `spec`; returns the last result and the
    /// texts.
    fn paint_theme(
        context: &egui::Context,
        spec: &mut PlotSpec,
        events: Vec<egui::Event>,
    ) -> (bool, Vec<Text>) {
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let mut commit = true;
        let painted = texts(&image(context, events, |ui| {
            commit = theme_editor(ui, spec, &mut env);
        }));
        assert!(env.requests.is_empty(), "painting saves no color");
        (commit, painted)
    }

    fn paint_axis(
        context: &egui::Context,
        axis: &mut stokesforge_plot::AxisSpec,
        computed: Option<(f64, f64)>,
    ) -> (bool, Vec<Text>) {
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let mut commit = true;
        let painted = texts(&image(context, Vec::new(), |ui| {
            commit = form(ui, "trial", |form| {
                super::axis_editor(form, axis, computed, None, &mut env)
            });
        }));
        (commit, painted)
    }

    /// #221, #344: under Auto, From and To show the computed bounds, in
    /// display order (an inverted axis shows the upper bound first),
    /// without writing them into the spec. Witness: outside Auto, they show the typed
    /// bounds and not the computed ones.
    #[test]
    fn under_auto_from_and_to_show_the_computed_bounds_in_display_order() {
        let context = test_support::context();
        let mut axis = stokesforge_plot::AxisSpec::default();
        assert!(axis.auto_range);
        let (commit, painted) = paint_axis(&context, &mut axis, Some((7.0, 4000.0)));
        assert!(!commit, "painting commits nothing");
        assert!(is_painted(&painted, &tr!("plot-axes-from")));
        assert!(is_painted(&painted, "4000"), "{painted:?}");
        assert!(text_exact(&painted, "7").center.x < text_exact(&painted, "4000").center.x);
        assert_eq!((axis.range_min, axis.range_max), (None, None));

        axis.inverted = true;
        let (commit, painted) = paint_axis(&context, &mut axis, Some((7.0, 4000.0)));
        assert!(!commit);
        assert!(
            text_exact(&painted, "4000").center.x < text_exact(&painted, "7").center.x,
            "inverted axis: the upper bound first"
        );
        assert_eq!((axis.range_min, axis.range_max), (None, None));

        axis.inverted = false;
        axis.auto_range = false;
        axis.range_min = Some(-5.0);
        axis.range_max = Some(8.0);
        let (_, painted) = paint_axis(&context, &mut axis, Some((7.0, 4000.0)));
        assert!(is_painted(&painted, "8"), "{painted:?}");
        assert!(!is_painted(&painted, "4000"), "{painted:?}");
    }

    /// #11: the "Theme" tab draws (themes, background, text, axes) without
    /// changing anything as long as nothing is done — even when the local colors
    /// differ from those it shows (the title of one axis, the scale of
    /// another). Witness: a click on a theme does apply it and commits.
    #[test]
    fn the_theme_tab_draws_without_changing_anything() {
        let mut spec = PlotSpec::empty("Figure").with_panels(PanelSpec::at(0, 0));
        spec.set_text_color("#336699");
        first_panel(&mut spec).left.title_color = "#aa0000".into();
        first_panel(&mut spec).left.tick_color = "#00aa00".into();
        let before = spec.clone();
        let context = test_support::context();
        let (commit, painted) = paint_theme(&context, &mut spec, Vec::new());
        assert!(!commit);
        assert_eq!(spec, before);
        for label in [
            tr!("plot-theme-themes"),
            tr!("plot-theme-global-colors"),
            tr!("plot-theme-background"),
            tr!("plot-theme-text"),
            tr!("plot-theme-axes"),
            tr!("plot-theme-note"),
        ] {
            assert!(is_painted(&painted, &label), "{label:?} missing");
        }
        for label in [tr!("plot-theme-light"), tr!("plot-theme-dark")] {
            assert!(is_painted(&painted, &label), "theme {label:?} missing");
        }

        let dark = &THEMES[1];
        assert_ne!(spec.background, dark.background);
        let pos = text_exact(&painted, &tr!("plot-theme-dark")).center;
        let mut commits = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![click_event(pos, true)],
            vec![click_event(pos, false)],
        ] {
            commits.push(paint_theme(&context, &mut spec, events).0);
        }
        assert_eq!(commits, [false, false, true], "the click commits once");
        assert_eq!(spec.background, dark.background);
        assert_eq!(spec.panels[0].left.title_color, dark.text);
    }

    // -- The formula reminder -------------------------------------------------

    /// Each example of the syntax reminder really compiles in Typst: a reminder
    /// that showed a wrong syntax would teach an error. The effects
    /// described are all distinct.
    #[test]
    fn each_reminder_example_compiles() {
        let fonts =
            formula::resolve_fonts(&stokesforge_render_egui::FontChoice::default()).expect("fonts");
        let rows = formula_memo_rows();
        for (source, _) in &rows {
            for example in source.split("  ") {
                let (sans, math) = fonts.clone();
                formula::compile(example, 14.0, [0, 0, 0, 255], sans, math)
                    .unwrap_or_else(|error| panic!("{example:?} does not compile: {error}"));
            }
        }
        distinct(&rows, |(_, effect)| effect.clone());
        distinct(&rows, |(source, _)| source.to_string());
    }

    /// #33, #197: the syntax reminder is not painted at rest — only the icon
    /// is, at the far right of the field — and hovering the icon shows
    /// all its rows (what is typed, what is obtained) and a footer
    /// that names Typst.
    #[test]
    fn the_formula_reminder_shows_on_icon_hover() {
        let context = test_support::context();
        context.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let icon = std::cell::Cell::new(egui::Rect::NOTHING);
        let field = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut text = String::new();
        let mut paint = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                field.set(ui.text_edit_singleline(&mut text).rect);
                icon.set(hover_help_for_test(ui));
            });
        };
        let painted = texts(&image(&context, Vec::new(), &mut paint));
        let footer = tr!("plot-math-footer", url = "typst.app/docs/reference/math");
        for (source, effect) in formula_memo_rows() {
            assert!(!is_painted(&painted, source), "at rest: {source:?} painted");
            assert!(!is_painted(&painted, &effect));
        }
        assert!(!is_painted(&painted, &footer));
        assert!(
            icon.get().left() >= field.get().right(),
            "the icon is to the right of the field: {:?} vs {:?}",
            icon.get(),
            field.get()
        );

        let hover = vec![egui::Event::PointerMoved(icon.get().center())];
        let mut painted = Vec::new();
        for _ in 0..4 {
            painted = texts(&image(&context, hover.clone(), &mut paint));
        }
        for (source, effect) in formula_memo_rows() {
            assert!(is_painted(&painted, source), "hovered: {source:?} absent");
            assert!(is_painted(&painted, &effect), "hovered: {effect:?} absent");
        }
        assert!(is_painted(&painted, &footer));
        assert!(footer.contains("Typst"), "the reminder names Typst");
    }

    /// Paints the formula help icon and returns its rectangle.
    fn hover_help_for_test(ui: &mut egui::Ui) -> egui::Rect {
        let before = ui.cursor().min;
        formula_help(ui, "");
        egui::Rect::from_min_max(before, ui.min_rect().max)
    }

    /// W6 (#148): width taken by the formula help next to a field, in a given
    /// state of the preference and with a given `hint` of the field.
    fn formula_help_width(hidden: bool, hint: &str) -> f32 {
        let context = test_support::context();
        context.data_mut(|data| {
            data.insert_temp(egui::Id::new(FORMULA_HELP_HIDDEN), hidden);
        });
        let mut width = 0.0;
        image(&context, Vec::new(), |ui| {
            ui.horizontal(|ui| {
                let before = ui.cursor().min.x;
                formula_help(ui, hint);
                width = ui.cursor().min.x - before;
            });
        });
        width
    }

    /// W6: hidden, the formula syntax icon disappears; if it also carried the
    /// field's own hint, a plain "?" keeps it reachable.
    #[test]
    fn the_formula_help_icon_disappears_when_the_preference_is_off() {
        assert!(formula_help_width(false, "") > 0.0, "shown: the icon");
        assert_eq!(formula_help_width(true, ""), 0.0, "hidden: nothing");
        assert!(
            formula_help_width(true, "Hint of the field") > 0.0,
            "hidden but the field has a hint: a plain \"?\""
        );
    }

    /// The window hands the preference to the fields it paints.
    #[test]
    fn the_window_publishes_the_formula_help_preference() {
        let context = test_support::context();
        let mut h = harness(|_| {});
        h.open(PlotSettingsTab::Graph, None);
        let hidden = |context: &egui::Context| {
            context.data(|data| data.get_temp::<bool>(egui::Id::new(FORMULA_HELP_HIDDEN)))
        };
        paint_window(&context, &mut h.state, Vec::new());
        assert_eq!(hidden(&context), Some(false));
        h.state.dispatch(Action::SetShowFormulaHelp(false));
        paint_window(&context, &mut h.state, Vec::new());
        assert_eq!(hidden(&context), Some(true));
    }

    // -- The choice labels ----------------------------------------------------

    fn unknown() -> PlotType {
        PlotType::Unknown(RawVariant {
            name: "Violin".into(),
            payload: Vec::new(),
        })
    }

    /// The plot types offered by the list: all, except the unknown. The
    /// `match` without `_` forces coming back here when a variant is added.
    fn known_types() -> Vec<PlotType> {
        let all = [
            PlotType::Scatter,
            PlotType::Line,
            PlotType::LineScatter,
            PlotType::Bar,
            PlotType::Histogram,
            PlotType::ErrorBar,
        ];
        for plot_type in all.iter().cloned().chain([unknown()]) {
            match plot_type {
                PlotType::Scatter
                | PlotType::Line
                | PlotType::LineScatter
                | PlotType::Bar
                | PlotType::Histogram
                | PlotType::ErrorBar
                | PlotType::Unknown(_) => {}
            }
        }
        all.to_vec()
    }

    fn styles() -> [LineStyle; 5] {
        let all = [
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Dash,
            LineStyle::Dot,
            LineStyle::DashDot,
        ];
        for style in all {
            match style {
                LineStyle::None
                | LineStyle::Solid
                | LineStyle::Dash
                | LineStyle::Dot
                | LineStyle::DashDot => {}
            }
        }
        all
    }

    fn symbol_shapes() -> [SymbolShape; 7] {
        let all = [
            SymbolShape::None,
            SymbolShape::Circle,
            SymbolShape::Square,
            SymbolShape::Triangle,
            SymbolShape::Diamond,
            SymbolShape::Plus,
            SymbolShape::Cross,
        ];
        for shape in all {
            match shape {
                SymbolShape::None
                | SymbolShape::Circle
                | SymbolShape::Square
                | SymbolShape::Triangle
                | SymbolShape::Diamond
                | SymbolShape::Plus
                | SymbolShape::Cross => {}
            }
        }
        all
    }

    fn positions() -> [LegendPosition; 4] {
        let all = [
            LegendPosition::TopLeft,
            LegendPosition::TopRight,
            LegendPosition::BottomLeft,
            LegendPosition::BottomRight,
        ];
        for position in all {
            match position {
                LegendPosition::TopLeft
                | LegendPosition::TopRight
                | LegendPosition::BottomLeft
                | LegendPosition::BottomRight => {}
            }
        }
        all
    }

    /// Each dropdown list of the properties shows choices that can be
    /// told apart, over all variants; the unknown type has its own
    /// label, distinct from the known types.
    #[test]
    fn the_choices_of_one_list_have_distinct_labels() {
        let mut types = known_types();
        types.push(unknown());
        distinct(&types, plot_type_label);
        distinct(&styles(), |style| line_style_label(*style));
        distinct(&symbol_shapes(), |shape| symbol_shape_label(*shape));
        distinct(&positions(), |position| legend_position_label(*position));
        // The label follows the variant, not its content.
        let other = PlotType::Unknown(RawVariant {
            name: "Polar".into(),
            payload: vec![1],
        });
        assert_eq!(plot_type_label(&other), plot_type_label(&unknown()));
    }

    // -- The metadata of a column -------------------------------------------

    /// A column with file metadata: a section, two key / value
    /// rows, a row without a key.
    fn harness_with_metadata(long_name: &str) -> (Session, AppState, NodeId) {
        let mut project = Project::new("Metadata").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let column = project
            .add_column(sheet, None, None, Column::from_f64(&[1.0]))
            .unwrap();
        let line = |key: &str, value: &str| MetaLine {
            key: key.into(),
            value: value.into(),
        };
        project
            .set_column_meta(
                column,
                ColumnMeta {
                    long_name: long_name.into(),
                    file_metadata: vec![
                        line("[Measurement]", ""),
                        line("XUNITS", "NANOMETERS"),
                        line("", "free text line"),
                        line("NPOINTS", "3"),
                    ],
                    ..Default::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        (session, state, column)
    }

    fn paint_metadata(
        context: &egui::Context,
        state: &AppState,
        actions: &mut Vec<Action>,
        events: Vec<egui::Event>,
    ) -> Vec<Text> {
        for _ in 0..3 {
            image(context, Vec::new(), |ui| {
                column_metadata_window(ui.ctx(), state, actions)
            });
        }
        texts(&image(context, events, |ui| {
            column_metadata_window(ui.ctx(), state, actions)
        }))
    }

    /// Issue #5: the metadata window shows, read-only, the key / value
    /// rows, the section in bold and the row without a key, under a
    /// title that names the column. Open, it asks for nothing; Enter
    /// ("OK") like Escape closes it, without anything else. Closed, it paints
    /// nothing.
    #[test]
    fn the_metadata_window_shows_the_rows_and_closes() {
        let context = test_support::context();
        for end_key in [egui::Key::Enter, egui::Key::Escape] {
            let (_session, mut state, column) = harness_with_metadata("Absorbance");
            let mut actions = Vec::new();
            let painted = paint_metadata(&context, &state, &mut actions, Vec::new());
            assert!(
                !is_painted(&painted, "XUNITS"),
                "closed, nothing is painted"
            );

            state.dispatch(Action::ViewColumnMetadata { column });
            let painted = paint_metadata(&context, &state, &mut actions, Vec::new());
            let title = tr!("workbook-metadata-title-named", name = "Absorbance");
            assert!(is_painted(&painted, &title), "title absent");
            for text in ["XUNITS", "NANOMETERS", "free text line", "NPOINTS", "3"] {
                text_exact(&painted, text);
            }
            let visuals = context.global_style().visuals.clone();
            let section = text_exact(&painted, "[Measurement]");
            assert!(section.bold, "bold section");
            assert_eq!(
                section.color,
                visuals.text_color(),
                "bold does not change the color"
            );
            assert!(!text_exact(&painted, "XUNITS").bold, "witness: keyed row");
            assert_eq!(text_exact(&painted, "XUNITS").color, visuals.text_color());
            assert!(actions.is_empty(), "open, it asks for nothing");
            // #164: only a keyed numeric value (NPOINTS) offers the
            // "Use as x'" button; the NANOMETERS text does not.
            assert_eq!(
                painted
                    .iter()
                    .filter(|t| t.text == tr!("workbook-metadata-use-as-x-prime"))
                    .count(),
                1,
                "a single button: the numeric value"
            );

            paint_metadata(&context, &state, &mut actions, vec![key(end_key)]);
            assert_eq!(actions, [Action::CloseColumnMetadata], "{end_key:?}");
        }

        // Without a long name, the generic title.
        let (_session, mut state, column) = harness_with_metadata("");
        state.dispatch(Action::ViewColumnMetadata { column });
        let painted = paint_metadata(
            &test_support::context(),
            &state,
            &mut Vec::new(),
            Vec::new(),
        );
        assert!(is_painted(&painted, &tr!("workbook-metadata-title")));
    }

    // -- Error columns of a series (#360) ----------------------------------

    /// Like [`harness`], with a third column "C" in the sheet of the series;
    /// `adjust` receives the series and that column.
    fn error_harness(adjust: impl FnOnce(&mut SeriesSpec, NodeId)) -> (Harness, NodeId) {
        let mut project = Project::new("Properties").unwrap();
        let root = project.root();
        let (x, y) = xy_columns(&mut project);
        let sheet = project.tree().node(y).and_then(|n| n.parent()).unwrap();
        let extra = project
            .add_column(sheet, None, Some("C"), Column::from_f64(&[0.5, 0.5]))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        let mut series = SeriesSpec::new_xy(x, y);
        adjust(&mut series, extra);
        panel.series.push(series);
        let spec = PlotSpec::default().with_panels(panel);
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut harness = Harness {
            state,
            graph,
            _session: session,
        };
        harness.open(PlotSettingsTab::Series, None);
        (harness, extra)
    }

    /// Clicks the painted text `label` just below the row `row` (the "Y error"
    /// row comes before the "X error" row, both offer the same columns, and
    /// "None" also names the group and the line style).
    fn click_below(context: &egui::Context, state: &mut AppState, row: &str, label: &str) {
        // A first click on the row label takes the focus away from the name
        // field (its `lost_focus` would commit the draft by itself and hide a
        // missing commit of the field under test).
        let painted = paint_window(context, state, Vec::new());
        click(
            context,
            text_exact(&painted, row).center,
            egui::PointerButton::Primary,
            |ui| plot_settings_window(ui.ctx(), state),
        );
        let painted = paint_window(context, state, Vec::new());
        let top = text_exact(&painted, row).center.y;
        let pos = painted
            .iter()
            .filter(|t| t.text == label && t.center.y >= top - 1.0)
            .min_by(|a, b| a.center.y.total_cmp(&b.center.y))
            .unwrap_or_else(|| panic!("{label} not painted below {row}"))
            .center;
        click(context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        paint_window(context, state, Vec::new());
    }

    /// Choosing a column in "Y error" sets it, in one history entry.
    #[test]
    fn choosing_a_y_error_column_sets_it_in_one_history_entry() {
        let (mut harness, extra) = error_harness(|_, _| {});
        let context = test_support::context();
        let original = harness.model();
        assert_eq!(original.panels[0].series[0].y_error, None);
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(is_painted(&painted, &tr!("plot-series-y-error")));
        assert!(is_painted(&painted, &tr!("plot-series-x-error")));
        click_below(
            &context,
            &mut harness.state,
            &tr!("plot-series-y-error"),
            "C",
        );
        let after = harness.model();
        assert_eq!(after.panels[0].series[0].y_error, Some(extra));
        assert_eq!(after.panels[0].series[0].x_error, None);
        harness.state.close_plot_settings();
        harness.state.dispatch(Action::Undo);
        assert_eq!(harness.model(), original, "a single Undo");
    }

    /// "None" removes the error column.
    #[test]
    fn choosing_none_removes_the_y_error_column() {
        let (mut harness, _) = error_harness(|series, extra| series.y_error = Some(extra));
        let context = test_support::context();
        assert!(harness.model().panels[0].series[0].y_error.is_some());
        click_below(
            &context,
            &mut harness.state,
            &tr!("plot-series-y-error"),
            &tr!("plot-series-error-none"),
        );
        assert_eq!(harness.model().panels[0].series[0].y_error, None);
    }

    /// A "row" series (#166) has no error column: the rows are not painted.
    #[test]
    fn a_row_series_has_no_error_fields() {
        let (mut harness, _) = error_harness(|series, _| series.row = Some(0));
        let context = test_support::context();
        let painted = paint_window(&context, &mut harness.state, Vec::new());
        assert!(!is_painted(&painted, &tr!("plot-series-y-error")));
        assert!(!is_painted(&painted, &tr!("plot-series-x-error")));
    }
}
