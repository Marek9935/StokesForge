//! The specimen of a theme (#543, prototype F of #514): the theme's structure
//! and style drawn on reference data shipped with the application, so that a
//! tile of the theme picker shows what the theme is made of (size, layers,
//! axes on several sides, legend, colour bar, groups) and not only a colour.
//!
//! The data come from a reference project built once ([`reference_tree`]),
//! lazily and deterministically; it is never part of the user's project and is
//! never saved. [`specimen`] keeps every style and structure field of the
//! theme and replaces the data, layer by layer:
//!
//! - ungrouped series: as many reference curves as the layer had, at most
//!   [`MAX_SINGLES`], each taking the theme's series style in order;
//! - groups: the first [`MAX_GROUPS`] groups, each with at most
//!   [`MAX_PER_GROUP`] reference curves, keeping its gradient, its name and
//!   its colour bar segment (the segments of the dropped groups go, and the
//!   bar with them when none is left);
//! - axis ranges are automatic, texts stay the theme's own (a title that
//!   reads the column variables shows the reference columns, whose long names
//!   and units are filled);
//! - a layer with no series gets one reference curve; a theme with no series
//!   at all (a fresh graph, such as the colour-only Light and Dark) gets three
//!   Gaussian bands in its first base layer.
//!
//! Annotations and shapes are anchored to the data of the user and are dropped.
//! The drawing of a specimen (scaled whole, cached) is
//! `stokesforge_render_egui::drawings`.

use std::sync::OnceLock;

use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project, Tree};
use stokesforge_plot::{GraphTheme, PanelSpec, Panels, PlotSpec, SeriesSlot, SeriesSpec};

/// Most reference curves for the ungrouped series of a panel.
pub const MAX_SINGLES: usize = BAND_NAMES.len();
/// Most groups shown in a panel.
pub const MAX_GROUPS: usize = 2;
/// Most curves in one shown group.
pub const MAX_PER_GROUP: usize = 3;

const BAND_NAMES: [&str; 3] = ["Molecule A", "Molecule B", "Molecule C"];

/// The reference columns, in the scratch project.
struct Reference {
    project: Project,
    wavelength: NodeId,
    bands: Vec<NodeId>,
    time: NodeId,
    /// `families[group][curve]`.
    families: Vec<Vec<NodeId>>,
}

fn column(
    project: &mut Project,
    sheet: NodeId,
    values: &[f64],
    (name, unit): (&str, &str),
    role: ColumnRole,
    x_prime: Option<f64>,
) -> Option<NodeId> {
    let mut col = Column::from_f64(values);
    col.set_meta(ColumnMeta {
        role,
        long_name: name.to_string(),
        unit: unit.to_string(),
        x_prime,
        ..ColumnMeta::default()
    });
    project.add_column(sheet, None, None, col).ok()
}

fn build() -> Option<Reference> {
    let mut project = Project::new("specimen").ok()?;
    let root = project.root();
    let book = project.add_workbook(root, Some("Specimen")).ok()?;
    let sheet = project.add_worksheet(book, Some("Bands")).ok()?;
    let x: Vec<f64> = (0..200)
        .map(|i| 300.0 + 400.0 * f64::from(i) / 199.0)
        .collect();
    let wavelength = column(
        &mut project,
        sheet,
        &x,
        ("Wavelength", "nm"),
        ColumnRole::X,
        None,
    )?;
    // Centre, width, height of the three bands.
    let shapes = [(400.0, 28.0, 1.0), (500.0, 40.0, 0.7), (600.0, 22.0, 0.85)];
    let mut bands = Vec::new();
    for (name, (centre, width, height)) in BAND_NAMES.iter().zip(shapes) {
        let y: Vec<f64> = x
            .iter()
            .map(|x| 0.02 + height * (-((x - centre) / width).powi(2) / 2.0).exp())
            .collect();
        bands.push(column(
            &mut project,
            sheet,
            &y,
            (name, "a.u."),
            ColumnRole::Y,
            None,
        )?);
    }
    let kinetics = project.add_worksheet(book, Some("Kinetics")).ok()?;
    let t: Vec<f64> = (1..=50).map(|i| f64::from(i) * 0.2).collect();
    let time = column(
        &mut project,
        kinetics,
        &t,
        ("Reaction time", "h"),
        ColumnRole::X,
        None,
    )?;
    let mut families = Vec::new();
    for g in 0..MAX_GROUPS {
        let mut group = Vec::new();
        for k in 0..MAX_PER_GROUP {
            let tau = 1.0 + 1.6 * k as f64 + 0.6 * g as f64;
            let plateau = 1.0 + 0.5 * g as f64 - 0.1 * k as f64;
            let y: Vec<f64> = t
                .iter()
                .map(|t| plateau * (1.0 - (-t / tau).exp()) + 0.01)
                .collect();
            group.push(column(
                &mut project,
                kinetics,
                &y,
                ("Conversion", "mol/L"),
                ColumnRole::Y,
                Some((1 + k + 3 * g) as f64 * 10.0),
            )?);
        }
        families.push(group);
    }
    Some(Reference {
        project,
        wavelength,
        bands,
        time,
        families,
    })
}

fn reference() -> Option<&'static Reference> {
    static REFERENCE: OnceLock<Option<Reference>> = OnceLock::new();
    REFERENCE.get_or_init(build).as_ref()
}

/// The scratch tree every specimen is bound to (built once).
pub fn reference_tree() -> Option<&'static Tree> {
    reference().map(|r| r.project.tree().as_ref())
}

fn curve(template: &SeriesSpec, x: NodeId, y: NodeId, name: &str) -> SeriesSpec {
    let mut series = template.clone();
    series.y = y;
    series.x = series.plot_type.needs_x().then_some(x);
    series.x_error = None;
    series.y_error = None;
    series.row = None;
    series.name = name.to_string();
    series.visible = true;
    series
}

fn specimen_panel(theme: &PanelSpec, bare: bool) -> PanelSpec {
    let mut panel = theme.clone();
    let Some(reference) = reference() else {
        return panel;
    };
    panel.annotations.clear();
    panel.shapes.clear();
    for axis in [
        &mut panel.bottom,
        &mut panel.left,
        &mut panel.right,
        &mut panel.top,
    ] {
        axis.auto_range = true;
    }
    let mut series = Vec::new();
    let mut kept_groups: Vec<u32> = Vec::new();
    let mut single = 0usize;
    for slot in theme.slots() {
        match slot {
            SeriesSlot::Single(index) => {
                if single >= MAX_SINGLES {
                    continue;
                }
                if let (Some(template), Some(y)) =
                    (theme.series.get(index), reference.bands.get(single))
                {
                    series.push(curve(
                        template,
                        reference.wavelength,
                        *y,
                        BAND_NAMES[single],
                    ));
                }
                single += 1;
            }
            SeriesSlot::Group { id, members } => {
                if kept_groups.len() >= MAX_GROUPS {
                    continue;
                }
                let g = kept_groups.len();
                kept_groups.push(id);
                let ys = reference.families.get(g).into_iter().flatten();
                for (k, (member, y)) in members.iter().zip(ys).take(MAX_PER_GROUP).enumerate() {
                    if let Some(template) = theme.series.get(*member) {
                        series.push(curve(
                            template,
                            reference.time,
                            *y,
                            &format!("Curve {}", k + 1),
                        ));
                    }
                }
            }
        }
    }
    if series.is_empty() {
        let count = if bare { 3 } else { 1 };
        for (j, (y, name)) in reference
            .bands
            .iter()
            .zip(BAND_NAMES)
            .enumerate()
            .take(count)
        {
            let template = SeriesSpec::from_palette(*y, j);
            series.push(curve(&template, reference.wavelength, *y, name));
        }
    }
    panel.series = series;
    panel.group_names.retain(|id, _| kept_groups.contains(id));
    panel
        .group_gradients
        .retain(|id, _| kept_groups.contains(id));
    if let Some(bar) = panel.color_bar.as_mut() {
        bar.segments.retain(|s| kept_groups.contains(&s.group));
        for segment in &mut bar.segments {
            segment.from = None;
            segment.to = None;
        }
    }
    if panel
        .color_bar
        .as_ref()
        .is_some_and(|b| b.segments.is_empty())
    {
        panel.color_bar = None;
    }
    // Fewer members than the theme's: spread the gradient on those left.
    let gradients: Vec<(u32, Vec<String>)> = panel
        .group_gradients
        .iter()
        .map(|(id, stops)| (*id, stops.clone()))
        .collect();
    for (id, stops) in gradients {
        panel.set_group_gradient(id, &stops);
    }
    panel
}

/// The specimen of `theme`: the same graph drawn with the reference data;
/// resolve it against [`reference_tree`].
///
/// A colour-only theme (Light, Dark) holds an empty graph in its colours: it
/// has no series, so it comes out as a fresh new graph with three Gaussians.
pub fn specimen(theme: &GraphTheme) -> PlotSpec {
    let style = &theme.style;
    let no_series = style.panels.iter().all(|p| p.series.is_empty());
    let first_base = style.panels.iter().position(|p| p.overlay.is_none());
    let panels: Vec<PanelSpec> = style
        .panels
        .iter()
        .enumerate()
        .map(|(i, panel)| specimen_panel(panel, no_series && Some(i) == first_base))
        .collect();
    let mut out = style.clone();
    out.panels = Panels::from(panels);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{ColorBarSegment, ColorBarSpec};

    fn plain(n: usize) -> PanelSpec {
        let mut panel = PanelSpec::at(0, 0);
        panel.series = (0..n)
            .map(|i| SeriesSpec::from_palette(NodeId::from_raw(100 + i as u64), i))
            .collect();
        panel
    }

    fn spec_of(panels: Vec<PanelSpec>) -> GraphTheme {
        GraphTheme {
            name: "test".into(),
            style: PlotSpec::empty("").with_panels(panels),
            colors_only: false,
        }
    }

    #[test]
    fn one_two_five_series_give_one_two_three_curves() {
        for (given, shown) in [(1, 1), (2, 2), (5, 3)] {
            let out = specimen(&spec_of(vec![plain(given)]));
            assert_eq!(out.panels[0].series.len(), shown, "{given} series");
        }
    }

    #[test]
    fn the_curves_take_the_style_of_the_theme_series_in_order() {
        let mut panel = plain(2);
        panel.series[0].line.color = "#111111".into();
        panel.series[1].line.color = "#222222".into();
        let out = specimen(&spec_of(vec![panel]));
        assert_eq!(out.panels[0].series[0].line.color, "#111111");
        assert_eq!(out.panels[0].series[1].line.color, "#222222");
    }

    #[test]
    fn a_fresh_graph_gets_three_bands_and_a_series_less_panel_one() {
        let out = specimen(&spec_of(vec![PanelSpec::at(0, 0)]));
        assert_eq!(out.panels[0].series.len(), 3);
        let out = specimen(&spec_of(vec![plain(2), {
            let mut p = PanelSpec::at(0, 1);
            p.series.clear();
            p
        }]));
        assert_eq!(out.panels[1].series.len(), 1);
    }

    fn grouped() -> PanelSpec {
        let mut panel = PanelSpec::at(0, 0);
        panel.push_series_batch(
            (0..5)
                .map(|i| SeriesSpec::from_palette(NodeId::from_raw(100 + i), i as usize))
                .collect(),
        );
        panel.push_series_batch(
            (0..2)
                .map(|i| SeriesSpec::from_palette(NodeId::from_raw(200 + i), i as usize))
                .collect(),
        );
        panel.push_series_batch(
            (0..2)
                .map(|i| SeriesSpec::from_palette(NodeId::from_raw(300 + i), i as usize))
                .collect(),
        );
        let ids = panel.group_ids();
        for id in &ids {
            assert!(panel.set_group_gradient(*id, &["#2c7bb6", "#d7191c"]));
        }
        panel.color_bar = Some(ColorBarSpec {
            segments: ids.iter().map(|g| ColorBarSegment::new(*g)).collect(),
            ..ColorBarSpec::default()
        });
        panel
    }

    #[test]
    fn groups_keep_their_gradient_and_their_colour_bar_segment() {
        let theme = grouped();
        let ids = theme.group_ids();
        let out = specimen(&spec_of(vec![theme]));
        let panel = &out.panels[0];
        assert_eq!(panel.group_ids(), ids[..MAX_GROUPS].to_vec());
        assert_eq!(panel.group_members(ids[0]).len(), MAX_PER_GROUP);
        assert_eq!(panel.group_members(ids[1]).len(), 2);
        assert!(panel.group_gradient(ids[0]).is_some());
        assert!(!panel.group_gradients.contains_key(&ids[2]));
        let bar = panel.color_bar.as_ref().expect("the bar stays");
        let groups: Vec<u32> = bar.segments.iter().map(|s| s.group).collect();
        assert_eq!(groups, ids[..MAX_GROUPS].to_vec());
    }

    #[test]
    fn a_colour_bar_left_without_a_shown_group_is_dropped() {
        let mut panel = plain(2);
        panel.color_bar = Some(ColorBarSpec {
            segments: vec![ColorBarSegment::new(77)],
            ..ColorBarSpec::default()
        });
        let out = specimen(&spec_of(vec![panel]));
        assert!(out.panels[0].color_bar.is_none());
    }

    #[test]
    fn two_panels_keep_their_frames_and_ranges_become_automatic() {
        let mut theme = spec_of(vec![plain(1), {
            let mut p = plain(1);
            p.row = 0;
            p.column = 1;
            p.bottom.auto_range = false;
            p.bottom.range_min = Some(5.0);
            p
        }]);
        theme.style.frame_widths = vec![300.0, 150.0];
        let out = specimen(&theme);
        assert_eq!(out.panels.len(), 2);
        assert_eq!(out.frame_widths, theme.style.frame_widths);
        assert!(out.panels[1].bottom.auto_range);
        assert_eq!(out.panels[1].column, 1);
    }

    #[test]
    fn a_colour_only_theme_is_a_fresh_graph_with_three_gaussians_in_its_colours() {
        let colors = stokesforge_plot::THEMES[1];
        let theme = GraphTheme::from_colors("Dark", &colors);
        let out = specimen(&theme);
        assert_eq!(out.panels.len(), theme.style.panels.len());
        assert_eq!(out.panels[0].series.len(), 3);
        assert_eq!(out.background, theme.style.background);
        let names: Vec<&str> = out.panels[0]
            .series
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, BAND_NAMES);
    }

    #[test]
    fn the_reference_data_are_named_and_the_tree_is_built_once() {
        let tree = reference_tree().expect("built");
        assert!(std::ptr::eq(tree, reference_tree().unwrap()));
        let out = specimen(&spec_of(vec![plain(3), grouped()]));
        let resolved = stokesforge_core::resolve::resolve_spec(tree, &out);
        let x = out.panels[0].series[0].x.expect("a curve with an X");
        let fields = stokesforge_core::resolve::column_fields(tree, x).expect("column");
        assert_eq!(
            (fields.long_name.as_str(), fields.units.as_str()),
            ("Wavelength", "nm")
        );
        assert!(resolved
            .panels
            .iter()
            .flat_map(|p| &p.series)
            .all(|s| !s.stale));
    }

    #[test]
    fn the_specimen_resolves_against_the_scratch_tree() {
        let out = specimen(&spec_of(vec![grouped()]));
        let resolved = stokesforge_core::resolve::resolve_spec(reference_tree().unwrap(), &out);
        assert!(resolved.panels[0].series.iter().all(|s| !s.stale));
        assert_eq!(resolved.panels[0].series.len(), 5);
    }
}
