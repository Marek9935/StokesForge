//! The `PlotSpec` itself: construction, invariants, reading a composed
//! spec. Port of `legacy-python/tests/test_plot_spec.py`.
//!
//! None of these checks needs a renderer — that is the
//! point: the description layer is testable alone, exactly like
//! `stokesforge-core`. It does not even need a `Tree`: a series references
//! `NodeId`s, and a `NodeId` is built without a tree.
//!
//! # What was not carried over, and why
//!
//! Seven checks of the Python suite have no equivalent here, and none
//! disappears because it was in the way:
//!
//! - `test_a_series_must_designate_a_y_column` — `SeriesSpec::y` is a required
//!   `NodeId`; there is no value to refuse;
//! - `test_a_series_only_attaches_to_the_left_or_right_axis` — `SeriesAxis`
//!   has only two variants; the faulty call does not compile;
//! - `test_histogram_bins_and_widths_are_sanitized` — `NonZeroU32` and
//!   `Option<f64>` replace the `max(1, …)` and the `float(…)`; there is no more
//!   sanitizing to observe. What remains checkable is checked below
//!   (`default_histogram_bins_is_forty`, `a_bar_width_of_none_means_deduced`);
//! - `test_string_values_are_coerced_to_enums` — this crate has no textual
//!   vocabulary: enums neither enter nor leave as strings,
//!   the conversion from the on-disk format belongs to
//!   `stokesforge-persistence`;
//! - `test_ids_are_unique` — there is no series identifier (see
//!   `SeriesRef`);
//! - `test_round_trip_through_dict_is_faithful`,
//!   `test_round_trip_survives_json` and
//!   `test_unknown_fields_are_ignored_rather_than_fatal` — no
//!   serialization in this crate, so no round trip to check.
//!
//! The eleven editing tests (`edit.*`) are out of scope for the phase, except
//! for the palette, checked here and in `src/style.rs`.

use stokesforge_id::NodeId;
use stokesforge_plot::{
    palette_color, AxisScale, AxisSide, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle,
    PanelSpec, Panels, PlotSpec, PlotType, SeriesAxis, SeriesRef, SeriesSpec, SymbolShape,
    SymbolSpec, DEFAULT_HISTOGRAM_BINS, DEFAULT_PALETTE,
};

/// Columns of the fixture. A `NodeId` is built without a tree: the spec does not know
/// whether the column exists, and that is precisely what makes a broken
/// reference detectable — by resolution, not here.
const X: NodeId = NodeId::from_raw(10);
const Y1: NodeId = NodeId::from_raw(11);
const Y2: NodeId = NodeId::from_raw(12);
const Y3: NodeId = NodeId::from_raw(13);
const ERR: NodeId = NodeId::from_raw(14);
const ABSENT: NodeId = NodeId::from_raw(99);

// -- construction and invariants -----------------------------------------------

#[test]
fn an_empty_spec_always_has_one_panel() {
    let spec = PlotSpec::empty("Title");
    assert_eq!(spec.title, "Title");
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(spec.series_count(), 0);
    assert_eq!(spec.rows(), 1);
    assert_eq!(spec.columns(), 1);
}

#[test]
fn a_spec_built_without_panels_gets_one_anyway() {
    // A graph without a panel could not be rendered: the type guarantees one.
    let spec = PlotSpec::empty("").with_panels(Vec::new());
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(Panels::new(Vec::new()).len(), 1);
}

#[test]
fn a_series_designates_its_columns_by_identity() {
    // The opposite of the naive port: no name, no path, a NodeId.
    let series = SeriesSpec::new_xy(X, Y1);
    assert_eq!(series.y, Y1);
    assert_eq!(series.x, Some(X));
    assert_eq!(series.x_error, None);
    assert_eq!(series.y_error, None);
}

#[test]
fn a_series_attaches_to_the_left_axis_by_default() {
    let series = SeriesSpec::new(Y1);
    assert_eq!(series.axis, SeriesAxis::Left);
    assert_eq!(series.axis.side(), AxisSide::Left);
    assert_eq!(AxisSide::from(SeriesAxis::Right), AxisSide::Right);
}

#[test]
fn default_styles_match_the_python_defaults() {
    // The default values are a contract: they decide what an unconfigured
    // graph looks like.
    let series = SeriesSpec::new(Y1);
    assert_eq!(series.plot_type, PlotType::Line);
    assert!(series.visible && series.show_in_legend);
    assert!(series.name.is_empty());
    assert_eq!(series.line, LineSpec::default());
    assert_eq!(series.line.style, LineStyle::Solid);
    assert!((series.line.width - 1.5).abs() < f64::EPSILON);
    assert_eq!(series.symbol.shape, SymbolShape::Circle);
    assert!((series.symbol.size - 7.0).abs() < f64::EPSILON);
    assert_eq!(AxisSpec::default().scale, AxisScale::Linear);
    assert_eq!(LegendSpec::default().position, LegendPosition::TopRight);
    assert!(LegendSpec::default().visible);
}

#[test]
fn default_histogram_bins_is_forty() {
    // B4 (`guide-de-correction-robustesse.md` §F4) — `histogram_bins` is
    // now a `BinCount` (bounded at both ends), no longer a bare `NonZeroU32`
    // (bounded below only).
    assert_eq!(DEFAULT_HISTOGRAM_BINS.get(), 40);
    assert_eq!(SeriesSpec::new(Y1).histogram_bins, DEFAULT_HISTOGRAM_BINS);
}

#[test]
fn a_bar_width_of_none_means_deduced() {
    // The sentinel `0.0` of the Python version becomes `None`: zero was a
    // perfectly writable bar width, and therefore ambiguous.
    assert_eq!(SeriesSpec::new(Y1).bar_width, None);
    let explicit = SeriesSpec {
        bar_width: Some(0.8),
        ..SeriesSpec::new(Y1)
    };
    assert_eq!(explicit.bar_width, Some(0.8));
}

#[test]
fn series_columns_list_only_the_references_that_exist() {
    let series = SeriesSpec {
        x: Some(X),
        y_error: Some(ERR),
        ..SeriesSpec::new(Y1)
    };
    assert_eq!(series.referenced_columns(), vec![X, Y1, ERR]);
    assert!(series.references(ERR));
    assert!(!series.references(ABSENT));
}

#[test]
fn histogram_does_not_need_an_x_column() {
    assert!(!PlotType::Histogram.needs_x());
    assert!(PlotType::Bar.needs_x());
}

#[test]
fn plot_type_draw_flags() {
    assert!(PlotType::Line.draws_line() && !PlotType::Line.draws_symbols());
    assert!(PlotType::Scatter.draws_symbols() && !PlotType::Scatter.draws_line());
    assert!(PlotType::LineScatter.draws_line() && PlotType::LineScatter.draws_symbols());
    assert!(PlotType::ErrorBar.draws_line() && PlotType::ErrorBar.draws_symbols());
}

#[test]
fn axis_display_label_never_shows_empty_parentheses() {
    let axis = |label: &str, unit: &str| AxisSpec {
        label: label.to_string(),
        unit: unit.to_string(),
        ..AxisSpec::default()
    };
    assert_eq!(axis("Time", "s").display_label(), "Time (s)");
    assert_eq!(axis("Time", "").display_label(), "Time");
    assert_eq!(axis("", "s").display_label(), "(s)");
    assert_eq!(axis("", "").display_label(), "");
}

#[test]
fn explicit_range_needs_both_bounds_and_auto_off() {
    let bounded = AxisSpec {
        range_min: Some(0.0),
        range_max: Some(1.0),
        ..AxisSpec::default()
    };
    // `auto_range` is `true`: the bounds are remembered, not applied.
    assert!(!bounded.has_explicit_range());
    assert!(!AxisSpec {
        auto_range: false,
        range_max: None,
        ..bounded.clone()
    }
    .has_explicit_range());
    assert!(AxisSpec {
        auto_range: false,
        ..bounded
    }
    .has_explicit_range());
}

#[test]
fn palette_cycles() {
    assert_eq!(palette_color(0), DEFAULT_PALETTE[0]);
    assert_eq!(palette_color(DEFAULT_PALETTE.len()), DEFAULT_PALETTE[0]);
}

// -- reading a composed spec --------------------------------------------------

/// The fixture of the Python suite (`two_panel_spec`), built by hand: the
/// editing functions that assembled it there are out of scope for the
/// phase. Two series on the first panel, one of them on the right axis;
/// a lone histogram on the second, stacked below.
fn two_panel_spec() -> PlotSpec {
    let first = PanelSpec {
        series: vec![
            SeriesSpec::new_xy(X, Y1),
            SeriesSpec {
                axis: SeriesAxis::Right,
                ..SeriesSpec::new_xy(X, Y2)
            },
        ],
        ..PanelSpec::default()
    };
    let second = PanelSpec {
        series: vec![SeriesSpec {
            plot_type: PlotType::Histogram,
            ..SeriesSpec::new(Y3)
        }],
        ..PanelSpec::at(1, 0)
    };
    PlotSpec::empty("Two panels").with_panels(vec![first, second])
}

#[test]
fn iter_series_walks_every_panel() {
    let spec = two_panel_spec();
    let panels: Vec<usize> = spec.iter_series().map(|(at, _)| at.panel).collect();
    assert_eq!(panels, vec![0, 0, 1]);
    assert_eq!(spec.series_count(), 3);
}

#[test]
fn columns_are_deduplicated_and_ordered() {
    let spec = two_panel_spec();
    assert_eq!(spec.referenced_columns(), vec![X, Y1, Y2, Y3]);
    assert_eq!(spec.panels[0].referenced_columns(), vec![X, Y1, Y2]);
}

#[test]
fn a_panel_knows_it_uses_the_secondary_axis() {
    let spec = two_panel_spec();
    assert!(spec.panels[0].uses_right_axis());
    assert!(!spec.panels[1].uses_right_axis());
}

#[test]
fn rows_and_columns_come_from_the_panel_positions() {
    let spec = two_panel_spec();
    assert_eq!(spec.rows(), 2);
    assert_eq!(spec.columns(), 1);
    assert_eq!(spec.panels[1].row, 1);

    // Two panels side by side, rather than stacked.
    let side_by_side =
        PlotSpec::empty("").with_panels(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]);
    assert_eq!(side_by_side.rows(), 1);
    assert_eq!(side_by_side.columns(), 2);
}

#[test]
fn a_series_is_addressed_by_its_position() {
    // What replaces `find_series(id)` / `panel_index(id)` of the Python
    // version: the address is the position, and it holds for that spec.
    let spec = two_panel_spec();
    let at = SeriesRef::new(1, 0);
    assert_eq!(spec.series_at(at), Some(&spec.panels[1].series[0]));
    assert_eq!(spec.series_at(SeriesRef::new(9, 0)), None);
    assert_eq!(spec.series_at(SeriesRef::new(1, 9)), None);
    assert_eq!(
        spec.iter_series().map(|(at, _)| at).last(),
        Some(SeriesRef::new(1, 0))
    );
}

#[test]
fn panel_axis_lookup_by_side() {
    let spec = two_panel_spec();
    let panel = &spec.panels[0];
    assert_eq!(panel.axis(AxisSide::Left), &panel.left);
    assert_eq!(panel.axis(AxisSide::Bottom), &panel.bottom);
    assert_eq!(panel.axis(AxisSide::Right), &panel.right);
    // The top axis is the only one hidden by default.
    assert_eq!(panel.axis(AxisSide::Top), &panel.top);
    assert!(!panel.top.visible);
    assert!(panel.bottom.visible && panel.left.visible && panel.right.visible);
}

#[test]
fn a_spec_knows_which_columns_would_break_it() {
    // The question that deleting a column will ask (§11.1): a broken
    // reference must be detectable, never silent.
    let spec = two_panel_spec();
    assert!(spec.references(Y3));
    assert!(!spec.references(ABSENT));
}

// -- immutability ------------------------------------------------------------------

#[test]
fn editing_a_spec_never_touches_the_one_it_came_from() {
    // The Python version held this with `frozen=True`; here it is the property
    // of values. The test stays useful: it locks in the fact that a spec is
    // modified by cloning, which undo depends on (the old spec is the
    // undo delta).
    let spec = two_panel_spec();
    let mut modified = spec.clone();
    modified
        .series_at_mut(SeriesRef::new(0, 0))
        .expect("the series (0, 0) exists in the fixture")
        .line
        .width = 4.0;
    assert!((spec.panels[0].series[0].line.width - 1.5).abs() < f64::EPSILON);
    assert!((modified.panels[0].series[0].line.width - 4.0).abs() < f64::EPSILON);
    assert_ne!(spec, modified);
}

#[test]
fn the_symbol_of_a_series_can_be_restyled_without_rebuilding_it() {
    let base = SeriesSpec::from_palette(Y1, 1);
    assert_eq!(base.line.color, DEFAULT_PALETTE[1]);
    assert_eq!(base.symbol.color, DEFAULT_PALETTE[1]);
    let hollow = SeriesSpec {
        symbol: SymbolSpec {
            edge_color: "#000000".to_string(),
            ..base.symbol.clone()
        },
        ..base.clone()
    };
    assert_eq!(hollow.symbol.effective_edge_color(), "#000000");
    assert_eq!(base.symbol.effective_edge_color(), DEFAULT_PALETTE[1]);
}
