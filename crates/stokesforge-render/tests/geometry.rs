//! Axis framing, tested on the cases that cause a division by zero.
//!
//! The pipeline is generic over the backend; this test uses it for what no
//! real backend conveniently allows — stress-testing the framing without
//! producing a single pixel. The mute backend below is also, incidentally,
//! the proof that the genericity is not decorative.

use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{AxisSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::text::{estimate, TextMeasurer};
use stokesforge_render::{pipeline, DrawnPlot};

/// A backend that discards everything — except the measurement, which stays
/// the project's.
///
/// Discarding the measurement too would make this a test of something else:
/// it is what determines the room left for the axes, hence the interval on
/// which the framing is projected.
struct Mute {
    size: (u32, u32),
    measurer: TextMeasurer,
}

#[derive(Debug)]
enum Never {}

impl std::fmt::Display for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}

impl DrawingBackend for Mute {
    type ErrorType = Never;

    fn get_size(&self) -> (u32, u32) {
        self.size
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }

    // A mute backend must also discard paths: the default rasterizes the
    // marked width of sub-pixel paths as a real width.
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        _path: I,
        _style: &S,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }

    fn draw_pixel(
        &mut self,
        _point: BackendCoord,
        _color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }

    /// **Indispensable, and this is not obvious.**
    ///
    /// With `plotters` at `default-features = false`, the default
    /// implementation of `draw_text` ends up calling `FontData::draw` — whose
    /// "naive" version is a `panic!("The font implementation is unable
    /// to draw text")` (`plotters/src/style/font/mod.rs`). In other words:
    /// **any backend that does not override `draw_text` crashes at the first
    /// tick**, and not with an error, with a panic. The two backends of the
    /// work package override it (egui directly, SVG by delegation to
    /// `SVGBackend`), but a third — or this mute backend — would fall into
    /// it. Discovered while writing this test, which is therefore also the
    /// place to note it.
    fn draw_text<S: BackendTextStyle>(
        &mut self,
        _text: &str,
        _style: &S,
        _pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }

    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Never>> {
        Ok(estimate(&self.measurer, text, style))
    }
}

/// Draws a spec on a set of columns and returns the retained geometry.
fn geometry(columns: &[(&str, Vec<f64>)], build: impl Fn(&[NodeId]) -> PlotSpec) -> DrawnPlot {
    let columns: Vec<(&str, Column)> = columns
        .iter()
        .map(|(name, values)| {
            (
                *name,
                Column::from_block(
                    ColumnType::Numeric,
                    ValueBlock::Float(values.clone()),
                    ColumnMeta::default(),
                )
                .expect("column"),
            )
        })
        .collect();
    geometry_from_columns(&columns, build)
}

/// Like [`geometry`], but the columns are supplied as they are — the only
/// way to draw one that is not numeric.
fn geometry_from_columns(
    columns: &[(&str, Column)],
    build: impl Fn(&[NodeId]) -> PlotSpec,
) -> DrawnPlot {
    let mut project = Project::new("Geometry").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let ids: Vec<NodeId> = columns
        .iter()
        .map(|(name, column)| {
            project
                .add_column(sheet, None, Some(name), column.clone())
                .expect("added")
        })
        .collect();

    let spec = build(&ids);
    let plot = resolve_spec(project.tree(), &spec);
    let backend = Mute {
        size: (640, 480),
        measurer: TextMeasurer::default(),
    };
    pipeline::draw(backend, &plot).expect("drawing")
}

fn one_panel(series: SeriesSpec, axis: impl FnOnce(&mut PanelSpec)) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(series);
    axis(&mut panel);
    PlotSpec::empty("Test").with_panels(panel)
}

#[test]
fn the_framing_follows_the_data_with_a_margin() {
    let drawn = geometry(
        &[("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])],
        |ids| one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |_| {}),
    );
    // 5% margin on both sides.
    assert!(
        (drawn.x_range().0 - -0.1).abs() < 1e-9,
        "{:?}",
        drawn.x_range()
    );
    assert!(
        (drawn.x_range().1 - 2.1).abs() < 1e-9,
        "{:?}",
        drawn.x_range()
    );
    assert!(
        (drawn.y_range().0 - 9.0).abs() < 1e-9,
        "{:?}",
        drawn.y_range()
    );
    assert!(
        (drawn.y_range().1 - 31.0).abs() < 1e-9,
        "{:?}",
        drawn.y_range()
    );
    assert_eq!(drawn.drawn_series, 1);
}

#[test]
fn a_constant_series_does_not_give_an_empty_interval() {
    // Without spreading, `build_cartesian_2d` would receive `5.0..5.0` and
    // Plotters would divide by zero when projecting. A single point is the
    // same case.
    let drawn = geometry(
        &[("X", vec![1.0, 2.0, 3.0]), ("Y", vec![5.0, 5.0, 5.0])],
        |ids| one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |_| {}),
    );
    assert!(
        drawn.y_range().0 < drawn.y_range().1,
        "{:?}",
        drawn.y_range()
    );

    let zero = geometry(&[("Y", vec![0.0])], |ids| {
        one_panel(SeriesSpec::new(ids[0]), |_| {})
    });
    assert!(zero.y_range().0 < zero.y_range().1, "{:?}", zero.y_range());
    assert!(zero.x_range().0 < zero.x_range().1, "{:?}", zero.x_range());
}

#[test]
fn a_series_that_is_entirely_non_finite_does_not_make_the_drawing_fail() {
    // `NaN` is the core's missing value, and a column can be nothing but
    // that. The rendering must produce an empty figure, not an error.
    let drawn = geometry(
        &[("Y", vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY])],
        |ids| one_panel(SeriesSpec::new(ids[0]), |_| {}),
    );
    assert_eq!(drawn.drawn_series, 0);
    assert_eq!(drawn.y_range(), (0.0, 1.0));
}

/// A mixed-type column is drawn **exactly like** the numeric column obtained
/// by blanking its text and empty cells.
///
/// This is the property that says the feature entered no backend
/// (invariant 5): nothing here knows `ColumnType::Mixed`. The translation
/// happens once, in `stokesforge_core::Column::numeric_view`, and the
/// pipeline — hence both backends — learned nothing new: it already
/// discarded non-finite points.
#[test]
fn a_mixed_column_draws_only_its_numeric_cells() {
    use stokesforge_core::MixedCell;

    let mixed = Column::from_mixed(&[
        MixedCell::Number(10.0),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Empty,
        MixedCell::Number(30.0),
    ]);
    let numeric_equivalent = Column::from_f64(&[10.0, f64::NAN, f64::NAN, 30.0]);

    let draw = |column: Column| {
        geometry_from_columns(&[("Y", column)], |ids| {
            one_panel(SeriesSpec::new(ids[0]), |_| {})
        })
    };
    let a = draw(mixed);
    let b = draw(numeric_equivalent);

    assert_eq!(a.drawn_series, 1, "the mixed series is indeed drawn");
    assert_eq!(a.y_range(), b.y_range(), "same vertical framing");
    assert_eq!(
        a.x_range(),
        b.x_range(),
        "same implicit abscissa: non-numeric rows still count \
         as rows, they do not shift the index of the following ones"
    );
    // And the framing comes only from the two numbers: neither "n/a" nor the
    // empty cell produced a point (5% margin on both sides of
    // [10, 30]).
    assert!((a.y_range().0 - 9.0).abs() < 1e-9, "{:?}", a.y_range());
    assert!((a.y_range().1 - 31.0).abs() < 1e-9, "{:?}", a.y_range());
}

/// A mixed column with no numeric cell can be drawn while drawing nothing —
/// like an entirely `NaN` numeric column, and unlike a **text** column,
/// which is a broken reference.
#[test]
fn a_mixed_column_without_a_number_gives_an_empty_figure_not_an_error() {
    use stokesforge_core::MixedCell;

    let drawn = geometry_from_columns(
        &[(
            "Y",
            Column::from_mixed(&[MixedCell::Text("a".to_string()), MixedCell::Empty]),
        )],
        |ids| one_panel(SeriesSpec::new(ids[0]), |_| {}),
    );
    assert_eq!(drawn.drawn_series, 0);
    assert_eq!(drawn.y_range(), (0.0, 1.0));
}

#[test]
fn an_explicit_framing_takes_precedence_over_data() {
    let drawn = geometry(&[("X", vec![0.0, 1.0]), ("Y", vec![0.0, 1.0])], |ids| {
        one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
            panel.left = AxisSpec {
                auto_range: false,
                range_min: Some(-5.0),
                range_max: Some(5.0),
                ..AxisSpec::default()
            };
        })
    });
    assert_eq!(drawn.y_range(), (-5.0, 5.0));
}

#[test]
fn an_inconsistent_explicit_framing_falls_back_to_the_data() {
    // Nothing validates `range_min`/`range_max` in `stokesforge-plot` (they are
    // two free `Option<f64>`) and a file can carry anything: equal bounds,
    // `NaN`, reversed. The rendering must degrade, not panic.
    for (min, max) in [
        (Some(3.0), Some(3.0)),
        (Some(f64::NAN), Some(1.0)),
        (Some(1.0), Some(f64::INFINITY)),
    ] {
        let drawn = geometry(&[("Y", vec![0.0, 10.0])], |ids| {
            one_panel(SeriesSpec::new(ids[0]), |panel| {
                panel.left = AxisSpec {
                    auto_range: false,
                    range_min: min,
                    range_max: max,
                    ..AxisSpec::default()
                };
            })
        });
        assert!(
            drawn.y_range().0 < drawn.y_range().1,
            "framing {min:?}..{max:?} gave {:?}",
            drawn.y_range()
        );
    }

    // Reversed bounds: accepted, and the axis is drawn upside down ("from 9 to
    // -9") — a complete and consistent framing, not a faulty datum.
    let drawn = geometry(&[("Y", vec![0.0, 10.0])], |ids| {
        one_panel(SeriesSpec::new(ids[0]), |panel| {
            panel.left = AxisSpec {
                auto_range: false,
                range_min: Some(9.0),
                range_max: Some(-9.0),
                ..AxisSpec::default()
            };
        })
    });
    assert_eq!(drawn.y_range(), (9.0, -9.0));
}

#[test]
fn an_inverted_axis_returns_its_bounds_reversed() {
    let drawn = geometry(&[("Y", vec![0.0, 10.0])], |ids| {
        one_panel(SeriesSpec::new(ids[0]), |panel| {
            panel.left = AxisSpec {
                inverted: true,
                ..AxisSpec::default()
            };
        })
    });
    assert!(
        drawn.y_range().0 > drawn.y_range().1,
        "an inverted axis must return (max, min): {:?}",
        drawn.y_range()
    );
}

#[test]
fn an_invisible_series_counts_neither_in_the_drawing_nor_in_the_framing() {
    let drawn = geometry(&[("Y", vec![0.0, 1.0]), ("Huge", vec![0.0, 1e9])], |ids| {
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new(ids[0]));
        let mut hidden = SeriesSpec::new(ids[1]);
        hidden.visible = false;
        panel.series.push(hidden);
        PlotSpec::empty("Test").with_panels(panel)
    });
    assert_eq!(drawn.drawn_series, 1);
    assert_eq!(drawn.panels[0].legend_entries, 1);
    assert_eq!(drawn.panels[0].series_geometry.len(), 1);
    assert!(
        drawn.y_range().1 < 2.0,
        "the hidden series widened the framing: {:?}",
        drawn.y_range()
    );
}

/// On a real drawing (title "Test", titled axes), the pixel picks the right
/// component: the geometry of [`stokesforge_render::hit`] is indeed the one
/// Plotters allocated, not a reconstruction.
#[test]
fn a_pixel_picks_the_graph_component_underneath() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextTarget};
    use stokesforge_render::{component_at, AxisSide, PlotComponent};
    let drawn = geometry(
        &[("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])],
        |ids| {
            one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
                panel.bottom.label = "Time".to_string();
                panel.left.label = "Signal".to_string();
            })
        },
    );
    assert!(drawn.title_height > 0, "the title \"Test\" occupies a band");
    let panel = drawn.panel(0).expect("a panel");
    let (ox, oy) = (panel.pixel_origin.0 as f64, panel.pixel_origin.1 as f64);
    let (w, h) = (panel.pixel_size.0 as f64, panel.pixel_size.1 as f64);
    let bottom = panel.axis_areas.bottom.size as f64;
    let left = panel.axis_areas.left.size as f64;
    let cx = ox + w / 2.0;
    let cy = oy + h / 2.0;
    assert_eq!(component_at(&drawn, cx, 2.0), Some(PlotComponent::Title));
    assert_eq!(
        component_at(&drawn, cx, cy),
        Some(PlotComponent::PlotArea { panel: 0 })
    );
    // #385: the axis lines, inside the edge of the plot area.
    assert_eq!(
        component_at(&drawn, cx, oy + h - 1.0),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Bottom
        })
    );
    assert_eq!(
        component_at(&drawn, ox + 1.0, cy),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Left
        })
    );
    assert_eq!(
        component_at(&drawn, cx, oy + h - 10.0),
        Some(PlotComponent::PlotArea { panel: 0 }),
        "away from the edge"
    );
    assert_eq!(
        component_at(&drawn, cx, oy + h + 4.0),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Bottom
        })
    );
    assert_eq!(
        component_at(&drawn, cx, oy + h + bottom - 2.0),
        Some(PlotComponent::Text(TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Bottom
        })),
        "the axis title text (issue #53) goes before its band"
    );
    // In the band but far from the text: the axis properties, as before.
    assert_eq!(
        component_at(&drawn, ox + 2.0, oy + h + bottom - 2.0),
        Some(PlotComponent::AxisTitle {
            panel: 0,
            side: AxisSide::Bottom
        })
    );
    assert_eq!(
        component_at(&drawn, ox - 4.0, cy),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Left
        })
    );
    assert_eq!(
        component_at(&drawn, ox - left + 2.0, cy),
        Some(PlotComponent::Text(TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Left
        }))
    );
}

/// Issue #53: a moved title is drawn centered on its anchor, without changing
/// the layout — and, without an anchor, at its default place.
#[test]
fn a_moved_title_is_drawn_on_its_anchor_without_changing_the_layout() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextAnchor, TextTarget};
    use stokesforge_render::PanelTransform;
    let columns = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let build = |anchors: bool| {
        move |ids: &[NodeId]| {
            one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
                panel.title = "Panel".to_string();
                panel.bottom.label = "Time".to_string();
                panel.left.label = "Signal".to_string();
                if anchors {
                    panel.title_anchor = Some(TextAnchor::new(1.5, 25.0));
                    panel.bottom.title_anchor = Some(TextAnchor::new(0.5, 12.0));
                    panel.left.title_anchor = Some(TextAnchor::new(0.25, 20.0));
                    panel.legend.anchor = Some(TextAnchor::new(1.0, 15.0));
                }
            })
        }
    };
    let default = geometry(&columns, build(false));
    let moved = geometry(&columns, build(true));

    let panel = moved.panel(0).expect("a panel");
    let transform = PanelTransform::of(panel).expect("plot area");
    let expected = |x: f64, y: f64| transform.data_to_pixel(x, y).expect("projectable");
    let text = |target: TextTarget| {
        panel
            .texts
            .iter()
            .find(|t| t.target == target)
            .copied()
            .unwrap_or_else(|| panic!("{target:?} is not in {:?}", panel.texts))
    };
    for (target, (x, y)) in [
        (TextTarget::PanelTitle { panel: 0 }, (1.5, 25.0)),
        (
            TextTarget::AxisTitle {
                panel: 0,
                side: PlotAxisSide::Bottom,
            },
            (0.5, 12.0),
        ),
        (
            TextTarget::AxisTitle {
                panel: 0,
                side: PlotAxisSide::Left,
            },
            (0.25, 20.0),
        ),
        (TextTarget::Legend { panel: 0 }, (1.0, 15.0)),
    ] {
        let drawn = text(target);
        let (px, py) = expected(x, y);
        assert!(drawn.anchored, "{target:?} is set on its anchor");
        assert!(
            (drawn.center.0 - px).abs() <= 1.5 && (drawn.center.1 - py).abs() <= 1.5,
            "{target:?} centered at {:?}, expected {:?}",
            drawn.center,
            (px, py)
        );
    }
    // The layout does not depend on the title's place: the plot area is the
    // same as with the titles in their place (otherwise the title would jump
    // on release).
    assert_eq!(
        moved.panel(0).unwrap().pixel_size,
        default.panel(0).unwrap().pixel_size
    );
    assert_eq!(
        moved.panel(0).unwrap().pixel_origin,
        default.panel(0).unwrap().pixel_origin
    );
    // Without an anchor, the titles are at their default place, in their band.
    let by_default = default.panel(0).expect("a panel");
    assert!(by_default.texts.iter().all(|t| !t.anchored));
    assert!(by_default.axis_areas.bottom.titled && !panel.axis_areas.bottom.titled);
    assert_eq!(by_default.texts.len(), 4, "title, two axes, legend");
    // The legend rows point at the series.
    assert_eq!(by_default.legend_rows.len(), 1);
    assert_eq!(by_default.legend_rows[0].series, 0);
}

/// Issue #53: the moved graph title keeps its (empty) band, and a
/// non-finite anchor leaves it in place.
#[test]
fn the_moved_graph_title_keeps_its_empty_band() {
    use stokesforge_plot::{TextAnchor, TextTarget};
    let columns = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let with = |anchor: Option<TextAnchor>| {
        geometry(&columns, move |ids| {
            let mut spec = one_panel(SeriesSpec::new_xy(ids[0], ids[1]), |_| {});
            spec.title_anchor = anchor;
            spec
        })
    };
    let default = with(None);
    assert!(default.title_height > 0);
    let title = default.title.expect("title drawn");
    assert_eq!(title.target, TextTarget::Title);
    assert!(!title.anchored);

    let moved = with(Some(TextAnchor::new(1.0, 30.0)));
    assert_eq!(moved.title_height, default.title_height, "band reserved");
    assert!(moved.title.expect("title drawn").anchored);

    let damaged = with(Some(TextAnchor::new(f64::NAN, 30.0)));
    assert_eq!(damaged.title_height, default.title_height, "anchor ignored");
    assert!(!damaged.title.expect("title drawn").anchored);
}

/// Issues #77 and #78: the font size set on a text box changes the size it
/// occupies — graph title, axis title, legend — and nothing else moves
/// (the plot area shrinks only by what the text takes).
#[test]
fn the_font_size_of_a_text_box_changes_its_size() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextFont, TextTarget};
    let columns = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let build = |big: bool| {
        move |ids: &[NodeId]| {
            let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
            series.name = "Curve".to_string();
            one_panel(series, |panel| {
                panel.bottom.label = "Time".to_string();
                if big {
                    let font = TextFont {
                        family: String::new(),
                        size: Some(32.0),
                        ..TextFont::default()
                    };
                    panel.bottom.title_font = font.clone();
                    panel.legend.font = font;
                }
            })
        }
    };
    let default = geometry(&columns, build(false));
    let big = geometry(&columns, build(true));
    let size = |g: &stokesforge_render::DrawnPlot, target: TextTarget| {
        g.panel(0)
            .unwrap()
            .texts
            .iter()
            .find(|t| t.target == target)
            .unwrap_or_else(|| panic!("{target:?} missing"))
            .size
    };
    for target in [
        TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Bottom,
        },
        TextTarget::Legend { panel: 0 },
    ] {
        let (small, large) = (size(&default, target), size(&big, target));
        assert!(
            large.1 > small.1,
            "{target:?}: size 32 ({large:?}) not taller than the default ({small:?})"
        );
    }
}

/// Selection handles (#98): a negative bar carries its own at the bottom,
/// on the value side, not on the baseline; a positive bar at the top.
#[test]
fn the_free_end_of_a_bar_is_on_the_side_of_its_value() {
    use stokesforge_plot::PlotType;
    let drawn = geometry(&[("X", vec![1.0, 2.0]), ("Y", vec![10.0, -10.0])], |ids| {
        let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
        series.plot_type = PlotType::Bar;
        one_panel(series, |_| {})
    });
    let bars = &drawn.panels[0].series_geometry[0];
    assert_eq!(bars.bars.len(), 2);
    assert_eq!(bars.bar_tips.len(), 2);
    let (positive, negative) = (bars.bars[0], bars.bars[1]);
    // Pixels: y grows downward.
    assert_eq!(bars.bar_tips[0].1, positive.0 .1, "{bars:?}");
    assert_eq!(bars.bar_tips[1].1, negative.1 .1, "{bars:?}");
    assert_eq!(bars.bar_tips[0].0, (positive.0 .0 + positive.1 .0) / 2);
}

/// The whiskers of an error-bar series can be picked, but do not form
/// a segment of the line: they carry no handle.
#[test]
fn whiskers_are_not_segments_of_the_line() {
    use stokesforge_plot::PlotType;
    let drawn = geometry(
        &[
            ("X", vec![1.0, 2.0, 3.0]),
            ("Y", vec![10.0, 20.0, 15.0]),
            ("E", vec![1.0, 2.0, 1.0]),
        ],
        |ids| {
            let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
            series.y_error = Some(ids[2]);
            series.plot_type = PlotType::ErrorBar;
            one_panel(series, |_| {})
        },
    );
    let series = &drawn.panels[0].series_geometry[0];
    assert_eq!(series.paths.len(), 1, "{series:?}");
    assert_eq!(series.paths[0].len(), 3);
    assert!(!series.whiskers.is_empty());
    let handles = series.handle_centers(|p| p);
    assert!(!handles.is_empty());
    // All the handles are on the line, none on a whisker.
    for handle in handles {
        let on_line = series.paths[0].windows(2).any(|s| {
            let (a, b) = (s[0], s[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let cross = dx * (handle.1 - a.1) - dy * (handle.0 - a.0);
            (cross / dx.hypot(dy)).abs() < 1e-6
                && handle.0 >= a.0.min(b.0) - 1e-6
                && handle.0 <= a.0.max(b.0) + 1e-6
        });
        assert!(on_line, "handle off the line: {handle:?}");
    }
}

/// The handle ink is the automatic ink of the figure's background:
/// dark on the default background (transparent, judged against white), white
/// on a dark background.
#[test]
fn the_handle_ink_follows_the_figure_background() {
    use stokesforge_render::color::{auto_ink, DARK_INK, LIGHT_INK};
    use stokesforge_render::pipeline::figure_ink;
    let mut spec = PlotSpec::empty("Ink");
    assert_eq!(figure_ink(&spec), DARK_INK);
    spec.background = "#101828".to_owned();
    assert_eq!(figure_ink(&spec), LIGHT_INK);
    assert_eq!(figure_ink(&spec), auto_ink((0x10, 0x18, 0x28)));
    spec.background = "#ffffff".to_owned();
    assert_eq!(figure_ink(&spec), DARK_INK);
}

/// #167: an overlaid layer shares the plot area of the covered layer
/// (linked dimension), keeps its own scale when its axis is shown, and
/// follows that of the covered layer otherwise.
#[test]
fn an_overlaid_layer_shares_the_plot_area_and_keeps_its_scale() {
    use stokesforge_plot::{OverlaySpec, Panels};
    let columns = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![400.0, 500.0, 800.0]),
        ("Y2", vec![0.0, 1.0, 2.0]),
    ];
    let build = |overlay: OverlaySpec| {
        move |ids: &[NodeId]| {
            let mut base = PanelSpec::at(0, 0);
            base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
            let mut top = PanelSpec::overlay_on(&base, overlay);
            top.series.push(SeriesSpec::new_xy(ids[2], ids[3]));
            PlotSpec::empty("Test").with_panels(Panels::new(vec![base, top]))
        }
    };

    let drawn = geometry(&columns, build(OverlaySpec::new(true, true)));
    assert_eq!(drawn.panels.len(), 2);
    let (base, top) = (&drawn.panels[0], &drawn.panels[1]);
    assert_eq!((top.index, top.row, top.column), (1, 0, 0));
    assert_eq!(base.pixel_origin, top.pixel_origin, "same plot area");
    assert_eq!(base.pixel_size, top.pixel_size);
    assert!(top.axis_areas.top.size > 0 && top.axis_areas.right.size > 0);
    assert_eq!(
        (top.axis_areas.bottom.size, top.axis_areas.left.size),
        (0, 0)
    );
    assert!(base.axis_areas.top.size == 0, "the top axis is the layer's");
    assert!(top.x_range.0 > 300.0, "own X scale: {:?}", top.x_range);
    assert!(base.x_range.1 < 10.0, "{:?}", base.x_range);

    // Axes not shown: the scale is that of the covered layer.
    let linked = geometry(&columns, build(OverlaySpec::new(false, false)));
    assert_eq!(linked.panels[1].x_range, linked.panels[0].x_range);
    assert_eq!(linked.panels[1].y_range, linked.panels[0].y_range);
    assert_eq!(linked.panels[1].pixel_size, linked.panels[0].pixel_size);
    assert!(
        drawn.panels[0].pixel_size.0 < linked.panels[0].pixel_size.0,
        "the top and right axes reserve their room for all the layers of the cell"
    );
}

/// #174: the scales of an overlaid layer are joined to those of the covered
/// layer — same axis fractions for the ticks, hence the top 0 facing the
/// bottom 0 — while enclosing its own data.
#[test]
fn the_scales_of_an_overlaid_layer_are_joined_to_those_of_the_covered_layer() {
    use stokesforge_plot::{OverlaySpec, Panels};
    use stokesforge_render::scale::{linear_key_points, DEFAULT_LABELS};
    let columns = [
        ("X", vec![0.0, 400.0, 800.0]),
        ("Y", vec![0.0, 5.0, 10.0]),
        ("X2", vec![0.0, 60.0, 120.0]),
        ("Y2", vec![0.0, 0.4, 0.9]),
    ];
    let drawn = geometry(&columns, |ids: &[NodeId]| {
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
        let mut top = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        top.series.push(SeriesSpec::new_xy(ids[2], ids[3]));
        PlotSpec::empty("Test").with_panels(Panels::new(vec![base, top]))
    });
    let (base, top) = (&drawn.panels[0], &drawn.panels[1]);
    let fractions = |range: (f64, f64)| -> Vec<f64> {
        linear_key_points(range, DEFAULT_LABELS)
            .iter()
            .map(|t| (t - range.0) / (range.1 - range.0))
            .collect()
    };
    for (own, other, data) in [
        (base.x_range, top.x_range, (0.0, 120.0)),
        (base.y_range, top.y_range, (0.0, 0.9)),
    ] {
        let (a, b) = (fractions(own), fractions(other));
        assert_eq!(a.len(), b.len(), "{own:?} / {other:?}");
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6, "{own:?} / {other:?}");
        }
        assert!(other.0 <= data.0 && other.1 >= data.1, "{other:?}");
    }
}

/// #172: the legend of the ordinary layer also lists the curves of the
/// overlaid layers; that of the overlaid layer, hidden at creation, does not
/// make a second one, and showing it limits it to its own curves.
#[test]
fn the_ordinary_layer_legend_lists_the_curves_of_the_overlaid_layers() {
    use stokesforge_plot::{OverlaySpec, Panels};
    let columns = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("Y2", vec![0.0, 1.0, 2.0]),
    ];
    let build = |top_legend: bool| {
        move |ids: &[NodeId]| {
            let mut base = PanelSpec::at(0, 0);
            base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
            let mut top = PanelSpec::overlay_on(&base, OverlaySpec::new(false, true));
            top.legend.visible = top_legend;
            top.series.push(SeriesSpec::new_xy(ids[0], ids[2]));
            PlotSpec::empty("Test").with_panels(Panels::new(vec![base, top]))
        }
    };
    let drawn = geometry(&columns, build(false));
    assert_eq!(drawn.panels[0].legend_entries, 2, "all the layers");
    assert_eq!(drawn.panels[1].legend_entries, 0, "no second legend");
    let drawn = geometry(&columns, build(true));
    assert_eq!(drawn.panels[0].legend_entries, 2);
    assert_eq!(drawn.panels[1].legend_entries, 1, "its own curves");
}

/// #168: the axes of several overlaid layers stack outward, in layer order,
/// on a common plot area; a layer with no axis shown leaves no empty band.
#[test]
fn the_axes_of_several_overlaid_layers_stack_outward() {
    use stokesforge_plot::{OverlaySpec, Panels};
    let columns = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![400.0, 500.0, 800.0]),
        ("Y2", vec![0.0, 1.0, 2.0]),
        ("X3", vec![5.0, 6.0, 7.0]),
        ("Y3", vec![1.0, 2.0, 3.0]),
        ("X4", vec![50.0, 60.0, 70.0]),
        ("Y4", vec![1.0, 2.0, 3.0]),
    ];
    let build = |axes: Vec<OverlaySpec>| {
        move |ids: &[NodeId]| {
            let mut base = PanelSpec::at(0, 0);
            base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
            let mut panels = vec![base.clone()];
            for (rank, axes) in axes.iter().copied().enumerate() {
                let mut layer = PanelSpec::overlay_on(&base, axes);
                layer
                    .series
                    .push(SeriesSpec::new_xy(ids[2 + 2 * rank], ids[3 + 2 * rank]));
                panels.push(layer);
            }
            PlotSpec::empty("Test").with_panels(Panels::new(panels))
        }
    };

    let three = geometry(&columns, build(vec![OverlaySpec::SECOND_AXES; 3]));
    assert_eq!(three.panels.len(), 4);
    for layer in &three.panels[1..] {
        assert_eq!(layer.pixel_origin, three.panels[0].pixel_origin);
        assert_eq!(layer.pixel_size, three.panels[0].pixel_size);
    }
    let top: Vec<_> = three.panels[1..]
        .iter()
        .map(|p| (p.axis_areas.top.offset, p.axis_areas.top.size))
        .collect();
    let right: Vec<_> = three.panels[1..]
        .iter()
        .map(|p| (p.axis_areas.right.offset, p.axis_areas.right.size))
        .collect();
    let band = top[0].1;
    assert!(band > 0);
    assert_eq!(top, vec![(0, band), (band, band), (2 * band, band)]);
    assert_eq!(right[1].0, right[0].1, "{right:?}");
    assert!(right[2].0 > right[1].0, "{right:?}");

    // A single overlaid layer: the plot area is taller than under three
    // stories of axes.
    let one = geometry(&columns[..4], build(vec![OverlaySpec::SECOND_AXES]));
    assert!(three.panels[0].pixel_size.1 < one.panels[0].pixel_size.1);

    // The second layer with no axis shown: the third comes against the area.
    let without = geometry(
        &columns,
        build(vec![
            OverlaySpec::SECOND_AXES,
            OverlaySpec::new(false, false),
            OverlaySpec::SECOND_AXES,
        ]),
    );
    assert_eq!(without.panels[1].axis_areas.top.offset, 0);
    assert_eq!(
        without.panels[3].axis_areas.top.offset,
        without.panels[3].axis_areas.top.size
    );
}

/// `AxisSpec::orient` does not change the figure: the drawn framing of an axis
/// is that of its oriented equivalent, whatever the auto, direction and order
/// of the bounds (#344). Witness: the cases are not all at the same framing.
#[test]
fn an_oriented_axis_is_drawn_like_the_original_axis() {
    let bounds = [
        (None, None),
        (Some(1.0), Some(5.0)),
        (Some(5.0), Some(1.0)),
        (Some(3.0), Some(3.0)),
        (Some(2.0), None),
    ];
    let mut framings = Vec::new();
    for auto_range in [true, false] {
        for inverted in [false, true] {
            for (range_min, range_max) in bounds {
                let axis = AxisSpec {
                    auto_range,
                    inverted,
                    range_min,
                    range_max,
                    ..AxisSpec::default()
                };
                let drawn_for = |axis: AxisSpec| {
                    let d = geometry(&[("Y", vec![0.0, 10.0])], |ids| {
                        one_panel(SeriesSpec::new(ids[0]), |p| p.left = axis.clone())
                    });
                    d.y_range()
                };
                let before = drawn_for(axis.clone());
                let after = drawn_for(axis.orient());
                assert_eq!(before, after, "{axis:?}");
                framings.push(format!("{before:?}"));
            }
        }
    }
    framings.sort();
    framings.dedup();
    assert!(framings.len() >= 4, "the witness covers several framings");
}
