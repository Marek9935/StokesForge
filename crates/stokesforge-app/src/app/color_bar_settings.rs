//! The « Color bar » section of the Legend tab of the graph properties window
//! (#499, step C): the source and scale of the value, the placement, the ticks
//! and the title, the names of the groups on the bar and a link to the Series
//! window. What belongs to one group (its box, range, break label and the
//! chain option) is edited on its page of the Series window (#510): the look
//! of the bar is a theme, the data it maps is not.
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

/// Where a link button between the two windows (#510) sends the user, stashed
/// in the egui context and turned into an action by the properties window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarLink {
    /// Series window, on the first group on the bar.
    ToSeries,
    /// Graph Properties, Legend tab, Color bar section unfolded.
    ToBarLook,
}

fn link_id() -> egui::Id {
    egui::Id::new("color_bar_link_request")
}

/// Asks the properties window to follow `link` at the end of the frame. A link
/// is a target pick like a tab (#498): a draft being typed is validated on its
/// own page first, and refused, it keeps the page.
pub(super) fn request_link(ctx: &egui::Context, link: BarLink) {
    crate::field_state::pick_pending(ctx);
    ctx.data_mut(|d| d.insert_temp(link_id(), link));
}

/// Takes the pending link request, if any.
pub(super) fn take_link(ctx: &egui::Context) -> Option<BarLink> {
    ctx.data_mut(|d| {
        let link = d.get_temp::<BarLink>(link_id());
        d.remove::<BarLink>(link_id());
        link
    })
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

    let ui_ctx = ui.ctx().clone();
    let commit = form(ui, "plot_color_bar_page", |form| {
        let mut commit = form.checkbox(
            &mut bar.visible,
            tr!("plot-color-bar-show"),
            Some(tr!("plot-color-bar-show", "hint")),
        );
        let shown = bar.visible;
        let rows = form.enabled(shown, |form| {
            groups_summary(form, &bar, &groups, ui_ctx);
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

    // The segments follow the graph's order, whatever the file stored.
    let order: Vec<u32> = groups.iter().map(|(id, _)| *id).collect();
    let bar = bar.in_graph_order(&order).into_owned();
    if bar != baseline {
        panel.color_bar = Some(bar);
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

/// The groups on the bar, named on one line, and the link to the Series
/// window where their range, break label and chaining are edited (#510).
fn groups_summary(
    form: &mut Form<'_>,
    bar: &ColorBarSpec,
    groups: &[(u32, String)],
    ctx: egui::Context,
) {
    form.section(tr!("plot-color-bar-groups-section"));
    let on_bar: Vec<&(u32, String)> = groups
        .iter()
        .filter(|(group, _)| bar.segment_of(*group).is_some())
        .collect();
    if groups.is_empty() {
        form.note_dimmed(tr!("plot-color-bar-no-series-groups"));
    } else if on_bar.is_empty() {
        form.note_dimmed(tr!("plot-color-bar-groups-none"));
    } else {
        let names = on_bar
            .iter()
            .map(|(_, title)| title.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        form.note(tr!("plot-color-bar-groups-on", names = names));
    }
    form.field(|ui| {
        if ui
            .link(tr!("plot-color-bar-to-series"))
            .on_hover_text(tr!("plot-color-bar-to-series", "hint"))
            .clicked()
        {
            request_link(&ctx, BarLink::ToSeries);
        }
    });
}

/// The colour-bar rows of one group on the bar, for the page of the group in
/// the Series window (#510): its range, the label of its break with the next
/// group, and the chain option. `order` is the graph's group order. True if
/// the draft must be committed.
pub(super) fn group_bar_rows(
    form: &mut Form<'_>,
    bar: &mut ColorBarSpec,
    group: u32,
    order: &[u32],
) -> bool {
    let mut commit = false;
    let has_break = groups_with_a_break(bar, order).contains(&group);
    let hint_from = tr!("common-auto");
    if let Some(segment) = bar.segments.iter_mut().find(|s| s.group == group) {
        commit |= form.row(tr!("plot-color-bar-range"), |ui| {
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
        if has_break {
            commit |= formula_field(
                form,
                &tr!("plot-color-bar-group-break"),
                &tr!("plot-color-bar-group-break", "hint"),
                &mut segment.label,
            );
        }
    }
    commit |= form.checkbox(
        &mut bar.chain,
        tr!("plot-color-bar-chain"),
        Some(tr!("plot-color-bar-chain", "hint")),
    );
    commit
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
