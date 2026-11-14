//! The « Color Bar » tab of the graph properties window (#499, step C): the
//! series groups of a layer with their « on the bar » box, the source and scale of the value, the
//! placement, the ticks and the title.
//!
//! The page edits the draft (`PanelSpec::color_bar`) in place, like the legend
//! tab; a bar edit alone is one history entry `history-plot-color-bar` (see
//! `AppState::commit_plot_edit`). A layer with no bar shows the page with
//! « Show color bar » unticked and the other rows greyed with their reason.

use eframe::egui;
use stokesforge_core::resolve::resolve_panel;
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{
    ColorBarOrientation, ColorBarPosition, ColorBarScale, ColorBarSegment, ColorBarSpec,
    ColorBarTicks, ColorBarValue, PanelSpec, GRADIENT_PRESETS,
};

use crate::number_field::auto_number_field;
use stokesforge_app_state::object_manager::group_title;
use stokesforge_app_state::quantities;
use stokesforge_app_state::tr;

use super::form::{choice, form, ChoiceOption, Form};
use super::plot_settings::formula_field;

/// What the page asks of the panel once the form is painted.
#[derive(Default)]
struct Intent {
    /// A group to put on the bar (`true`) or take off it (`false`).
    toggle: Option<(u32, bool)>,
}

/// Puts `group` on the bar of `panel` (creating or showing the bar) and makes
/// sure its curves have a gradient to be coloured with: a group that retains
/// none gets the first preset, in the same edit (#499). True if anything
/// changed.
pub(super) fn put_group_on_bar(panel: &mut PanelSpec, group: u32) -> bool {
    let mut changed = false;
    let bar = panel.color_bar.get_or_insert_with(ColorBarSpec::default);
    if !bar.visible {
        bar.visible = true;
        changed = true;
    }
    if bar.segment_of(group).is_none() {
        bar.segments.push(ColorBarSegment::new(group));
        changed = true;
    }
    if panel.group_gradient(group).is_none() {
        let first: Vec<&str> = GRADIENT_PRESETS[0].stops.to_vec();
        changed |= panel.set_group_gradient(group, &first);
    }
    changed
}

/// Takes `group` off the bar of `panel`. True if it was on it.
pub(super) fn take_group_off_bar(panel: &mut PanelSpec, group: u32) -> bool {
    panel
        .color_bar
        .as_mut()
        .is_some_and(|bar| bar.remove_group(group))
}

/// A choice among a keyed enum's `CHOICES`, with its unknown stored value kept
/// visible and selected.
fn keyed_choice<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    value: &mut T,
    choices: &[T],
    label: impl Fn(&T) -> String,
    unknown: Option<&T>,
) -> bool {
    let mut options: Vec<ChoiceOption<T>> = choices
        .iter()
        .map(|candidate| ChoiceOption::new(candidate.clone(), label(candidate)))
        .collect();
    if let Some(unknown) = unknown {
        options.push(ChoiceOption::new(
            unknown.clone(),
            tr!("plot-color-bar-unknown"),
        ));
    }
    choice(ui, id_salt, value, &options)
}

fn value_label(value: &ColorBarValue) -> String {
    match value {
        ColorBarValue::XPrime => tr!("legend-text-from-x-prime"),
        ColorBarValue::Comments => tr!("legend-text-from-comments"),
        ColorBarValue::LongName => tr!("legend-text-from-long-name"),
        ColorBarValue::ShortName => tr!("legend-text-from-short-name"),
        ColorBarValue::Units => tr!("legend-text-from-units"),
        ColorBarValue::Unknown(_) => tr!("plot-color-bar-unknown"),
    }
}

fn position_label(position: &ColorBarPosition) -> String {
    match position {
        ColorBarPosition::TopLeft => tr!("plot-legend-top-left"),
        ColorBarPosition::TopRight => tr!("plot-legend-top-right"),
        ColorBarPosition::BottomLeft => tr!("plot-legend-bottom-left"),
        ColorBarPosition::BottomRight => tr!("plot-legend-bottom-right"),
        ColorBarPosition::Below => tr!("plot-color-bar-position-below"),
        ColorBarPosition::Right => tr!("plot-color-bar-position-right"),
        ColorBarPosition::Unknown(_) => tr!("plot-color-bar-unknown"),
    }
}

/// The unknown stored value of a keyed enum, if it is one.
macro_rules! unknown_of {
    ($value:expr, $enum:ident) => {
        match &$value {
            value @ $enum::Unknown(_) => Some(value.clone()),
            _ => None,
        }
    };
}

/// The « Color bar » section at the bottom of the Legend tab: a collapsing
/// header, **closed** unless the layer's bar is shown (it then opens by
/// itself, also when the bar becomes visible while the page is up).
/// `unfold` forces it open and scrolls it into view (opened from the bar's
/// double-click). Returns `true` if the draft must be committed.
pub(super) fn color_bar_section(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    row_columns: &[(NodeId, String)],
    unfold: bool,
) -> bool {
    let shown = panel.color_bar.as_ref().is_some_and(|bar| bar.visible);
    let was_shown_id = ui.make_persistent_id("color_bar_section_was_shown");
    let was_shown = ui.data(|data| data.get_temp::<bool>(was_shown_id));
    ui.data_mut(|data| data.insert_temp(was_shown_id, shown));
    ui.add_space(ui.spacing().item_spacing.y * 2.0);
    let mut header = egui::CollapsingHeader::new(tr!("plot-color-bar-section"))
        .id_salt("color_bar_section")
        .default_open(shown);
    if unfold || (shown && was_shown == Some(false)) {
        header = header.open(Some(true));
    }
    let mut commit = false;
    let response = header.show(ui, |ui| {
        commit = color_bar_editor(ui, panel, tree, row_columns);
    });
    if unfold {
        response
            .header_response
            .scroll_to_me(Some(egui::Align::TOP));
    }
    commit
}

/// Edits the colour bar of `panel`. Returns `true` if the draft must be
/// committed — see the head doc of `plot_settings_window`.
pub(super) fn color_bar_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    row_columns: &[(NodeId, String)],
) -> bool {
    // A layer with no bar shows a hidden, empty one.
    let baseline = panel.color_bar.clone().unwrap_or_else(|| ColorBarSpec {
        visible: false,
        ..ColorBarSpec::default()
    });
    let mut bar = baseline.clone();
    let groups: Vec<(u32, String)> = panel
        .group_ids()
        .into_iter()
        .map(|id| (id, group_title(panel, id)))
        .collect();
    let missing = if bar.visible {
        stokesforge_render::color_bar::missing_value_count(&resolve_panel(tree, panel))
    } else {
        0
    };
    let mut intent = Intent::default();

    let commit = form(ui, "plot_color_bar_page", |form| {
        let mut commit = form.checkbox(
            &mut bar.visible,
            tr!("plot-color-bar-show"),
            Some(tr!("plot-color-bar-show", "hint")),
        );
        let shown = bar.visible;
        let rows = form.enabled(shown, |form| {
            groups_section(form, &mut bar, &groups, &mut intent, &mut commit);
            value_section(form, &mut bar, row_columns, missing, &mut commit);
            layout_section(form, &mut bar, &mut commit);
            commit
        });
        if !shown {
            rows.response
                .on_disabled_hover_text(tr!("disabled-color-bar-off"));
        }
        commit |= rows.inner;
        commit
    });

    let mut commit = commit;
    // The segments follow the graph's order, whatever the file stored.
    let order: Vec<u32> = groups.iter().map(|(id, _)| *id).collect();
    let bar = bar.in_graph_order(&order).into_owned();
    if bar != baseline {
        panel.color_bar = Some(bar);
    }
    if let Some((group, on)) = intent.toggle {
        if on {
            put_group_on_bar(panel, group);
        } else {
            take_group_off_bar(panel, group);
        }
        commit = true;
    }
    commit
}

/// The groups of the bar that are followed by another one, in graph order
/// (`order`): each carries the label of the break between the two. None for
/// the last group on the bar; the label of a group is kept in its segment, so
/// it comes back when a group removed after it is ticked again (#499).
pub(super) fn groups_with_a_break(bar: &ColorBarSpec, order: &[u32]) -> Vec<u32> {
    let on_bar: Vec<u32> = order
        .iter()
        .copied()
        .filter(|group| bar.segment_of(*group).is_some())
        .collect();
    on_bar
        .split_last()
        .map(|(_, rest)| rest.to_vec())
        .unwrap_or_default()
}

/// Every series group of the layer, in graph order, each with its « on the
/// bar » box; under a ticked group its range; between two ticked
/// groups the label of their break; then the chain option. Draws into any
/// form, so the block can be moved to another page.
fn groups_section(
    form: &mut Form<'_>,
    bar: &mut ColorBarSpec,
    groups: &[(u32, String)],
    intent: &mut Intent,
    commit: &mut bool,
) {
    form.section(tr!("plot-color-bar-groups-section"));
    if groups.is_empty() {
        form.note_dimmed(tr!("plot-color-bar-no-series-groups"));
        return;
    }
    let order: Vec<u32> = groups.iter().map(|(id, _)| *id).collect();
    let with_break = groups_with_a_break(bar, &order);
    // The group of the previous ticked row, whose break is painted before the
    // next ticked one.
    let mut previous: Option<u32> = None;
    let hint_from = tr!("common-auto");
    for (group, title) in groups {
        let group = *group;
        let on_bar = bar.segment_of(group).is_some();
        if on_bar {
            if let Some(left) = previous.filter(|left| with_break.contains(left)) {
                if let Some(segment) = bar.segments.iter_mut().find(|s| s.group == left) {
                    *commit |= formula_field(
                        form,
                        &tr!("plot-color-bar-group-break"),
                        &tr!("plot-color-bar-group-break", "hint"),
                        &mut segment.label,
                    );
                }
            }
        }
        let mut ticked = on_bar;
        let changed = form.row(title.as_str(), |ui| {
            super::hover_help::help_beside(ui, tr!("plot-color-bar-group-on", "hint"), |ui| {
                ui.checkbox(&mut ticked, tr!("plot-color-bar-group-on"))
                    .changed()
            })
        });
        if changed {
            intent.toggle = Some((group, ticked));
        }
        if on_bar {
            previous = Some(group);
            if let Some(segment) = bar.segments.iter_mut().find(|s| s.group == group) {
                *commit |= form.row(tr!("plot-color-bar-range"), |ui| {
                    ui.label(tr!("plot-axes-from"));
                    let first = auto_number_field(
                        ui,
                        ("color_bar_from", group),
                        &mut segment.from,
                        &quantities::COLOR_BAR_BOUND,
                        &hint_from,
                    );
                    ui.label(tr!("analysis-field-to"));
                    let second = auto_number_field(
                        ui,
                        ("color_bar_to", group),
                        &mut segment.to,
                        &quantities::COLOR_BAR_BOUND,
                        &hint_from,
                    );
                    first.changed || second.changed
                });
            }
        }
    }
    *commit |= form.checkbox(
        &mut bar.chain,
        tr!("plot-color-bar-chain"),
        Some(tr!("plot-color-bar-chain", "hint")),
    );
}

/// Where the value comes from, how it maps onto the gradient, and the curves
/// left without one.
fn value_section(
    form: &mut Form<'_>,
    bar: &mut ColorBarSpec,
    row_columns: &[(NodeId, String)],
    missing: usize,
    commit: &mut bool,
) {
    form.section(tr!("plot-color-bar-value-section"));
    let unknown = unknown_of!(bar.value, ColorBarValue);
    *commit |= form.row(tr!("plot-color-bar-value-from"), |ui| {
        let changed = keyed_choice(
            ui,
            "color_bar_value_from",
            &mut bar.value,
            ColorBarValue::CHOICES,
            value_label,
            unknown.as_ref(),
        );
        super::hover_help::help_text(ui, tr!("plot-color-bar-value-from", "hint"));
        changed
    });
    if !row_columns.is_empty() {
        #[derive(Clone, Copy, PartialEq)]
        enum RowValue {
            None,
            Column,
        }
        let mut mode = if bar.row_column.is_some() {
            RowValue::Column
        } else {
            RowValue::None
        };
        let changed = form.row(tr!("plot-legend-row-text"), |ui| {
            choice(
                ui,
                "color_bar_row_value",
                &mut mode,
                &[
                    ChoiceOption::new(RowValue::None, tr!("plot-color-bar-row-none")),
                    ChoiceOption::new(RowValue::Column, tr!("plot-legend-row-column-value")),
                ],
            )
        });
        if changed {
            bar.row_column = match mode {
                RowValue::None => None,
                RowValue::Column => bar.row_column.or(row_columns.first().map(|(id, _)| *id)),
            };
            *commit = true;
        }
        if let Some(chosen) = bar.row_column.as_mut() {
            let selected = row_columns
                .iter()
                .find(|(id, _)| id == chosen)
                .map_or_else(String::new, |(_, name)| name.clone());
            *commit |= form.row(tr!("plot-legend-row-column"), |ui| {
                let mut picked = false;
                egui::ComboBox::from_id_salt("color_bar_row_column")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (id, name) in row_columns {
                            picked |= ui.selectable_value(chosen, *id, name).clicked();
                        }
                    });
                picked
            });
        }
    }
    let unknown = unknown_of!(bar.scale, ColorBarScale);
    *commit |= form.row(tr!("plot-color-bar-scale"), |ui| {
        let changed = keyed_choice(
            ui,
            "color_bar_scale",
            &mut bar.scale,
            ColorBarScale::CHOICES,
            |scale| match scale {
                ColorBarScale::Log => tr!("plot-color-bar-scale-log"),
                ColorBarScale::Linear => tr!("plot-color-bar-scale-linear"),
                ColorBarScale::Unknown(_) => tr!("plot-color-bar-unknown"),
            },
            unknown.as_ref(),
        );
        super::hover_help::help_text(ui, tr!("plot-color-bar-scale-log", "hint"));
        changed
    });
    if missing > 0 {
        form.note_dimmed(tr!("plot-color-bar-no-value", count = missing));
    }
}

/// Position, orientation, ticks and title.
fn layout_section(form: &mut Form<'_>, bar: &mut ColorBarSpec, commit: &mut bool) {
    form.section(tr!("plot-color-bar-layout-section"));
    // A dragged bar's anchor wins over the position: the list then shows
    // « Dragged », and choosing any place (even the one it names) drops it.
    let mut shown: Option<ColorBarPosition> = if bar.anchor.is_some() {
        None
    } else {
        Some(bar.position.clone())
    };
    let mut options: Vec<ChoiceOption<Option<ColorBarPosition>>> = ColorBarPosition::CHOICES
        .iter()
        .map(|place| ChoiceOption::new(Some(place.clone()), position_label(place)))
        .collect();
    if bar.anchor.is_some() {
        options.insert(
            0,
            ChoiceOption::new(None, tr!("plot-color-bar-position-dragged")),
        );
    }
    if matches!(bar.position, ColorBarPosition::Unknown(_)) && bar.anchor.is_none() {
        options.push(ChoiceOption::new(
            Some(bar.position.clone()),
            tr!("plot-color-bar-unknown"),
        ));
    }
    let picked = form.row(tr!("plot-color-bar-position"), |ui| {
        choice(ui, "color_bar_position", &mut shown, &options)
    });
    if picked {
        if let Some(place) = shown {
            bar.position = place;
            bar.anchor = None;
            *commit = true;
        }
    }
    // Below / Right force the orientation (unless the bar was dragged):
    // the choice shows what is drawn, greyed with its reason.
    let forced = forced_orientation(bar);
    let mut shown = forced.clone().unwrap_or_else(|| bar.orientation.clone());
    let unknown = unknown_of!(shown, ColorBarOrientation);
    let row = form.enabled(forced.is_none(), |form| {
        form.row(tr!("plot-color-bar-orientation"), |ui| {
            keyed_choice(
                ui,
                "color_bar_orientation",
                &mut shown,
                ColorBarOrientation::CHOICES,
                |orientation| match orientation {
                    ColorBarOrientation::Vertical => tr!("plot-color-bar-orientation-vertical"),
                    ColorBarOrientation::Horizontal => {
                        tr!("plot-color-bar-orientation-horizontal")
                    }
                    ColorBarOrientation::Unknown(_) => tr!("plot-color-bar-unknown"),
                },
                unknown.as_ref(),
            )
        })
    });
    if forced.is_some() {
        row.response
            .on_disabled_hover_text(tr!("plot-color-bar-orientation", "disabled"));
    } else if row.inner {
        bar.orientation = shown;
        *commit = true;
    }
    let unknown = unknown_of!(bar.ticks, ColorBarTicks);
    *commit |= form.row(tr!("plot-color-bar-ticks"), |ui| {
        keyed_choice(
            ui,
            "color_bar_ticks",
            &mut bar.ticks,
            ColorBarTicks::CHOICES,
            |ticks| match ticks {
                ColorBarTicks::Auto => tr!("plot-color-bar-ticks-auto"),
                ColorBarTicks::Breaks => tr!("plot-color-bar-ticks-breaks"),
                ColorBarTicks::Unknown(_) => tr!("plot-color-bar-unknown"),
            },
            unknown.as_ref(),
        )
    });
    *commit |= formula_field(
        form,
        &tr!("plot-color-bar-title"),
        &tr!("plot-color-bar-title", "hint"),
        &mut bar.title,
    );
}

/// The orientation the place forces, if it does: Below is horizontal, Right
/// vertical, unless the bar was dragged (its place is then its own).
pub(super) fn forced_orientation(bar: &ColorBarSpec) -> Option<ColorBarOrientation> {
    if bar.anchor.is_some() {
        return None;
    }
    match bar.position.effective() {
        ColorBarPosition::Below => Some(ColorBarOrientation::Horizontal),
        ColorBarPosition::Right => Some(ColorBarOrientation::Vertical),
        _ => None,
    }
}
