//! The « Color Bar » tab of the graph properties window (#499, step C): the
//! groups on the bar of a layer, the source and scale of the value, the
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
    /// A group to put on the bar, at the end.
    add: Option<u32>,
    /// A segment to move: (rank, up).
    shift: Option<(usize, bool)>,
    /// A segment to take off the bar.
    remove: Option<usize>,
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
    // The gestures on the list of segments.
    if let Some(rank) = intent.remove {
        if rank < bar.segments.len() {
            bar.segments.remove(rank);
            commit = true;
        }
    }
    if let Some((rank, up)) = intent.shift {
        let other = if up {
            rank.checked_sub(1)
        } else {
            Some(rank + 1)
        };
        if let Some(other) = other.filter(|other| *other < bar.segments.len()) {
            bar.segments.swap(rank, other);
            commit = true;
        }
    }
    if bar != baseline {
        panel.color_bar = Some(bar);
    }
    if let Some(group) = intent.add {
        put_group_on_bar(panel, group);
        commit = true;
    }
    commit
}

/// The groups on the bar, in bar order, with their range and break label,
/// then the « Add a group » list and the chain option.
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
    let count = bar.segments.len();
    for rank in 0..count {
        let group = bar.segments[rank].group;
        let title = groups
            .iter()
            .find(|(id, _)| *id == group)
            .map_or_else(|| group.to_string(), |(_, title)| title.clone());
        form.row(title, |ui| {
            for (up, label) in [
                (true, tr!("plot-color-bar-move-up")),
                (false, tr!("plot-color-bar-move-down")),
            ] {
                let movable = if up { rank > 0 } else { rank + 1 < count };
                if ui
                    .add_enabled(movable, egui::Button::new(label))
                    .on_disabled_hover_text(if up {
                        tr!("disabled-move-first")
                    } else {
                        tr!("disabled-move-last")
                    })
                    .clicked()
                {
                    intent.shift = Some((rank, up));
                }
            }
            if ui
                .button(tr!("plot-color-bar-remove"))
                .on_hover_text(tr!("plot-color-bar-remove", "hint"))
                .clicked()
            {
                intent.remove = Some(rank);
            }
        });
        let segment = &mut bar.segments[rank];
        let hint_from = tr!("common-auto");
        *commit |= form.row(tr!("plot-axes-from"), |ui| {
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
        *commit |= formula_field(
            form,
            &tr!("plot-color-bar-group-break"),
            &tr!("plot-color-bar-group-break", "hint"),
            &mut segment.label,
        );
    }
    if count == 0 {
        form.note_dimmed(tr!("plot-color-bar-no-groups"));
    }
    let free: Vec<&(u32, String)> = groups
        .iter()
        .filter(|(id, _)| bar.segment_of(*id).is_none())
        .collect();
    form.field(|ui| {
        let list = ui.add_enabled_ui(!free.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("color_bar_add_group")
                .selected_text(tr!("plot-color-bar-add-group"))
                .show_ui(ui, |ui| {
                    for (id, title) in &free {
                        if ui.selectable_label(false, title).clicked() {
                            intent.add = Some(*id);
                        }
                    }
                });
        });
        if free.is_empty() {
            list.response
                .on_disabled_hover_text(tr!("plot-color-bar-add-group", "disabled"));
        }
    });
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
    let unknown = unknown_of!(bar.position, ColorBarPosition);
    *commit |= form.row(tr!("plot-color-bar-position"), |ui| {
        keyed_choice(
            ui,
            "color_bar_position",
            &mut bar.position,
            ColorBarPosition::CHOICES,
            position_label,
            unknown.as_ref(),
        )
    });
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
