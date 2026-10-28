//! Mirror axes (#176) seen in the export: the top and right scale
//! repeats the bottom and left one, without a title, and can be limited to the stroke.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec, TickDirection};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn export(name: &str, rule: impl FnOnce(&mut PanelSpec)) -> String {
    let mut project = Project::new("Mirrors").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add_column = |name: &str, values: Vec<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let x = add_column("X", vec![0.0, 400.0, 800.0]);
    let y = add_column("Y", vec![10.0, 20.0, 30.0]);
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.label = "Time".into();
    panel.series.push(SeriesSpec::new_xy(x, y));
    // Minor ticks are on by default (#344): these tests count
    // tick marks and start from a figure without them.
    for axis in [
        &mut panel.bottom,
        &mut panel.left,
        &mut panel.top,
        &mut panel.right,
    ] {
        axis.minor_ticks = false;
    }
    rule(&mut panel);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.svg")), &svg).ok();
    }
    svg
}

fn occurrences(svg: &str, tick: &str) -> usize {
    svg.matches(&format!("\n{tick}\n")).count()
}

#[test]
fn mirror_axes_repeat_the_ticks_without_a_title() {
    let without = export("mirrors-without", |_| {});
    let with = export("mirrors-with", |p| {
        p.top.mirror = true;
        p.right.mirror = true;
    });
    assert_eq!(occurrences(&with, "800"), 2 * occurrences(&without, "800"));
    assert_eq!(occurrences(&with, "30"), 2 * occurrences(&without, "30"));
    assert_eq!(
        with.matches("Time").count(),
        without.matches("Time").count(),
        "no title on the mirrors"
    );
}

#[test]
fn a_stroke_only_mirror_keeps_the_tick_marks_without_the_numbers() {
    let bare = export("mirrors-bare", |p| {
        p.top.mirror = true;
        p.top.tick_labels = false;
        p.right.mirror = true;
        p.right.tick_labels = false;
    });
    let without = export("mirrors-without", |_| {});
    assert_eq!(occurrences(&bare, "800"), occurrences(&without, "800"));
    assert_eq!(occurrences(&bare, "30"), occurrences(&without, "30"));
}

/// A segment of a two-point stroke: `(x0, y0, x1, y1)`.
type Segment = (i32, i32, i32, i32);

fn segments(svg: &str) -> Vec<Segment> {
    svg.lines()
        .filter(|line| line.starts_with("<polyline"))
        .filter_map(|line| {
            let points = line.split("points=\"").nth(1)?.split('"').next()?;
            let numbers: Vec<i32> = points
                .split([' ', ','])
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse::<f64>().ok().map(|v| v as i32))
                .collect();
            (numbers.len() == 4).then(|| (numbers[0], numbers[1], numbers[2], numbers[3]))
        })
        .collect()
}

/// Axis lines (long horizontal or vertical strokes) and tick marks (short ones).
fn lines_and_tick_marks(svg: &str) -> (Vec<Segment>, Vec<Segment>) {
    segments(svg).into_iter().partition(|(x0, y0, x1, y1)| {
        (x1 - x0).abs().max((y1 - y0).abs()) > 100 && (x0 == x1 || y0 == y1)
    })
}

/// Ordinate of the top axis line, and its starting abscissa.
fn top_line(svg: &str) -> i32 {
    let (lines, _) = lines_and_tick_marks(svg);
    lines
        .iter()
        .filter(|(_, y0, _, y1)| y0 == y1)
        .map(|s| s.1)
        .min()
        .expect("top line")
}

fn closed_frame(name: &str, rule: impl FnOnce(&mut PanelSpec)) -> String {
    export(name, |p| {
        p.top.mirror = true;
        p.right.mirror = true;
        rule(p);
    })
}

/// Top tick marks: vertical, short, with one end on the top line.
fn top_tick_marks(svg: &str) -> Vec<Segment> {
    let top = top_line(svg);
    let (_, tick_marks) = lines_and_tick_marks(svg);
    tick_marks
        .into_iter()
        .filter(|s| s.0 == s.2 && s.0 > 76)
        .filter(|s| s.1.min(s.3) <= top && s.1.max(s.3) >= top)
        .collect()
}

#[test]
fn a_bare_stroke_keeps_the_line_without_tick_marks_or_numbers() {
    let closed = closed_frame("bare-stroke-ref", |_| {});
    let bare = closed_frame("bare-stroke", |p| {
        for axis in [&mut p.top, &mut p.right] {
            axis.ticks = false;
            axis.tick_labels = false;
        }
    });
    let without = export("bare-stroke-without", |_| {});
    let (bare_lines, bare_tick_marks) = lines_and_tick_marks(&bare);
    let (closed_lines, closed_tick_marks) = lines_and_tick_marks(&closed);
    assert_eq!(
        bare_lines.len(),
        closed_lines.len(),
        "the four lines remain"
    );
    assert_eq!(bare_lines.len(), 4);
    // Only the bottom and left keep their tick marks.
    assert_eq!(
        bare_tick_marks.len(),
        lines_and_tick_marks(&without).1.len()
    );
    assert!(closed_tick_marks.len() > bare_tick_marks.len());
    assert_eq!(occurrences(&bare, "800"), occurrences(&without, "800"));
    assert_eq!(occurrences(&bare, "30"), occurrences(&without, "30"));
}

#[test]
fn inward_and_both_sided_tick_marks_follow_the_plot_area() {
    let default = closed_frame("tick-marks-default", |_| {});
    let outward = top_tick_marks(&default);
    let top = top_line(&default);
    assert!(!outward.is_empty());
    assert!(
        outward.iter().all(|s| s.1.max(s.3) == top),
        "by default, outward: above the top line"
    );

    let inward = closed_frame("tick-marks-inward", |p| {
        p.top.tick_direction = TickDirection::In
    });
    let tick_marks = top_tick_marks(&inward);
    assert_eq!(tick_marks.len(), outward.len());
    assert!(
        tick_marks
            .iter()
            .all(|s| s.1.min(s.3) == top && s.1.max(s.3) > top),
        "inward: below the top line, in the plot area"
    );

    let both = closed_frame("tick-marks-both", |p| {
        p.top.tick_direction = TickDirection::Both
    });
    let tick_marks = top_tick_marks(&both);
    assert_eq!(tick_marks.len(), outward.len());
    assert!(
        tick_marks
            .iter()
            .all(|s| s.1.min(s.3) < top && s.1.max(s.3) > top),
        "both sides: on either side of the line"
    );
    // The outer half is exactly Plotters' tick mark.
    for outgoing in &outward {
        assert!(
            tick_marks
                .iter()
                .any(|s| s.0 == outgoing.0 && s.1.min(s.3) == outgoing.1.min(outgoing.3)),
            "the tick mark {outgoing:?} is found again in \"both sides\""
        );
    }
}

#[test]
fn the_numbers_keep_their_place_when_the_tick_marks_change() {
    let default = closed_frame("numbers-default", |_| {});
    let without = closed_frame("numbers-without-tick-marks", |p| {
        p.top.ticks = false;
        p.right.ticks = false;
    });
    // Same texts, at the same positions: only the tick marks disappear.
    let texts = |svg: &str| -> Vec<String> {
        svg.lines()
            .filter(|l| l.starts_with("<text"))
            .map(String::from)
            .collect()
    };
    assert_eq!(texts(&default), texts(&without));
    assert!(top_tick_marks(&without).is_empty());
}

#[test]
fn an_axis_with_default_tick_marks_keeps_plotters_next_to_a_modified_axis() {
    let default = closed_frame("neighbor-default", |_| {});
    let mixed = closed_frame("neighbor-mixed", |p| {
        p.top.tick_direction = TickDirection::In
    });
    // The right tick marks (short horizontal strokes in the right band)
    // are plotted by the pipeline in one case and by Plotters in the other:
    // they must overlap to the pixel.
    let right = |svg: &str| -> Vec<Segment> {
        let (_, tick_marks) = lines_and_tick_marks(svg);
        let mut v: Vec<Segment> = tick_marks
            .into_iter()
            .filter(|s| s.1 == s.3 && s.0 >= 400)
            .collect();
        v.sort_unstable();
        v
    };
    assert!(!right(&default).is_empty());
    assert_eq!(right(&default), right(&mixed));
}

/// Length of a tick mark.
fn length(s: &Segment) -> i32 {
    (s.2 - s.0).abs().max((s.3 - s.1).abs())
}

#[test]
fn minor_ticks_add_shorter_tick_marks_without_moving_the_main_ones() {
    let without = closed_frame("minor-without", |_| {});
    let with = closed_frame("minor-with", |p| p.top.minor_ticks = true);
    let main = top_tick_marks(&without);
    let all = top_tick_marks(&with);
    assert!(all.len() > main.len(), "minor ticks are added");
    for tick in &main {
        assert!(
            all.contains(tick),
            "the main tick mark {tick:?} does not move"
        );
    }
    let main_length = main.iter().map(length).max().expect("main");
    for tick in all.iter().filter(|t| !main.contains(t)) {
        assert!(
            length(tick) < main_length,
            "{tick:?} is shorter than the main tick marks"
        );
        assert!(
            !main.iter().any(|p| p.0 == tick.0),
            "a minor tick does not cover a main tick"
        );
    }
    // Only the top changed: the bottom and left keep their tick marks.
    let top = top_line(&with);
    let (_, before) = lines_and_tick_marks(&without);
    let (_, after) = lines_and_tick_marks(&with);
    for tick in after.iter().filter(|t| !before.contains(t)) {
        assert!(
            tick.1.min(tick.3) <= top && tick.1.max(tick.3) >= top,
            "{tick:?} is not on the top line"
        );
    }
}

#[test]
fn bottom_minor_ticks_are_plotted_with_plotters_tick_marks() {
    let without = export("minor-bottom-without", |_| {});
    let with = export("minor-bottom-with", |p| p.bottom.minor_ticks = true);
    let (_, before) = lines_and_tick_marks(&without);
    let (_, after) = lines_and_tick_marks(&with);
    assert!(after.len() > before.len());
    for tick in &before {
        assert!(after.contains(tick), "{tick:?} is kept");
    }
}

#[test]
fn minor_ticks_follow_the_tick_mark_direction() {
    let top = top_line(&closed_frame("minor-direction-ref", |_| {}));
    let inward = closed_frame("minor-direction-inward", |p| {
        p.top.minor_ticks = true;
        p.top.tick_direction = TickDirection::In;
    });
    assert!(
        top_tick_marks(&inward)
            .iter()
            .all(|s| s.1.min(s.3) == top && s.1.max(s.3) > top),
        "all of them, minor ticks included, inward"
    );
    let both = closed_frame("minor-direction-both", |p| {
        p.top.minor_ticks = true;
        p.top.tick_direction = TickDirection::Both;
    });
    assert!(top_tick_marks(&both)
        .iter()
        .all(|s| s.1.min(s.3) < top && s.1.max(s.3) > top));
}

#[test]
fn minor_ticks_are_plotted_alone_when_the_main_tick_marks_are_off() {
    let bare = closed_frame("minor-alone-ref", |p| p.top.ticks = false);
    let alone = closed_frame("minor-alone", |p| {
        p.top.ticks = false;
        p.top.minor_ticks = true;
    });
    let main = top_tick_marks(&closed_frame("minor-alone-full", |_| {}));
    let main_length = main.iter().map(length).max().expect("main");
    assert!(
        top_tick_marks(&bare).is_empty(),
        "neither main nor minor tick marks"
    );
    let small = top_tick_marks(&alone);
    assert!(!small.is_empty(), "the minor ticks remain");
    assert!(small.iter().all(|t| length(t) < main_length));
}

#[test]
fn a_logarithmic_axis_has_its_minor_ticks_at_the_multiples() {
    let without = export("minor-log-without", |p| {
        p.left.scale = stokesforge_plot::AxisScale::Log
    });
    let with = export("minor-log-with", |p| {
        p.left.scale = stokesforge_plot::AxisScale::Log;
        p.left.minor_ticks = true;
    });
    let added = lines_and_tick_marks(&with).1.len() - lines_and_tick_marks(&without).1.len();
    assert!(added >= 8, "at least 2 to 9 over a decade: {added}");
}

#[test]
fn a_fresh_axis_has_its_minor_ticks() {
    let off = export("minor-default-off", |_| {});
    let fresh = export("minor-default-fresh", |p| {
        p.bottom.minor_ticks = PanelSpec::at(0, 0).bottom.minor_ticks;
    });
    assert!(PanelSpec::at(0, 0).bottom.minor_ticks);
    assert!(lines_and_tick_marks(&fresh).1.len() > lines_and_tick_marks(&off).1.len());
}

#[test]
fn a_minor_tick_count_sets_the_number_of_small_tick_marks() {
    let tick_marks = |count: Option<u8>| {
        lines_and_tick_marks(&export("minor-count", |p| {
            p.bottom.minor_ticks = true;
            p.bottom.minor_tick_count = count;
        }))
        .1
        .len()
    };
    let without = lines_and_tick_marks(&export("minor-count-without", |_| {}))
        .1
        .len();
    let one = tick_marks(Some(1)) - without;
    let nine = tick_marks(Some(9)) - without;
    assert!(
        one > 0 && nine > 5 * one,
        "1 per interval: {one}, 9: {nine}"
    );
}

/// Bottom tick marks: vertical, short, with one end on the bottom line.
fn bottom_tick_marks(svg: &str) -> Vec<Segment> {
    let (lines, tick_marks) = lines_and_tick_marks(svg);
    let bottom = lines
        .iter()
        .filter(|(_, y0, _, y1)| y0 == y1)
        .map(|s| s.1)
        .max()
        .expect("bottom line");
    tick_marks
        .into_iter()
        .filter(|s| s.0 == s.2 && s.1.min(s.3) <= bottom && s.1.max(s.3) >= bottom)
        .collect()
}

fn longest(tick_marks: &[Segment]) -> i32 {
    tick_marks.iter().map(length).max().expect("tick marks")
}

#[test]
fn a_hand_set_long_tick_mark_changes_the_figure_and_the_length() {
    let default = export("advanced-length-default", |_| {});
    let long = export("advanced-length-10", |p| p.bottom.tick_length = Some(10.0));
    assert_ne!(default, long, "the length changes the figure");
    assert_eq!(longest(&bottom_tick_marks(&default)), 5);
    assert_eq!(longest(&bottom_tick_marks(&long)), 10);
}

#[test]
fn an_automatic_small_tick_mark_is_half_the_effective_long_tick_mark() {
    let minor = |rule: fn(&mut PanelSpec)| {
        let svg = export("advanced-small", |p| {
            p.bottom.minor_ticks = true;
            p.bottom.tick_length = Some(10.0);
            rule(p);
        });
        let main = bottom_tick_marks(&svg)
            .into_iter()
            .filter(|t| length(t) == 10)
            .count();
        assert!(main > 0);
        bottom_tick_marks(&svg)
            .iter()
            .map(length)
            .filter(|&l| l != 10)
            .max()
            .expect("small tick marks")
    };
    assert_eq!(minor(|_| {}), 5, "long 10, small Auto: 5");
    assert_eq!(
        minor(|p| p.bottom.minor_tick_length = Some(3.0)),
        3,
        "small set by hand"
    );
}

/// The SVG's polylines: `(stroke, stroke-width, segment)`.
fn strokes(svg: &str) -> Vec<(String, String, Segment)> {
    let attribute = |line: &str, name: &str| {
        line.split(&format!("{name}=\""))
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or_default()
            .to_string()
    };
    svg.lines()
        .filter(|l| l.starts_with("<polyline"))
        .filter_map(|l| {
            let seg = segments(l).first().copied()?;
            Some((attribute(l, "stroke"), attribute(l, "stroke-width"), seg))
        })
        .collect()
}

#[test]
fn the_tick_mark_color_touches_neither_the_line_nor_the_numbers() {
    let svg = export("advanced-color", |p| {
        p.bottom.tick_color = "#0000ff".into();
        p.bottom.tick_mark_color = "#ff0000".into();
    });
    let (lines, tick_marks) = lines_and_tick_marks(&svg);
    let bottom = bottom_tick_marks(&svg);
    assert!(!bottom.is_empty());
    for (color, _, seg) in strokes(&svg) {
        if bottom.contains(&seg) {
            assert_eq!(color, "#FF0000", "bottom tick mark {seg:?}");
        } else if lines.contains(&seg) && seg.1 == seg.3 && seg.1 > 200 {
            assert_eq!(color, "#0000FF", "bottom line {seg:?}");
        }
    }
    assert!(!tick_marks.is_empty());
    assert!(
        svg.contains("fill=\"#0000FF\""),
        "the numbers keep the scale's color"
    );
    assert!(!svg.contains("fill=\"#FF0000\""), "no red number");
}

#[test]
fn the_tick_mark_thickness_does_not_touch_the_axis_line() {
    let svg = export("advanced-thickness", |p| p.bottom.tick_width = Some(3.0));
    let bottom = bottom_tick_marks(&svg);
    for (_, thickness, seg) in strokes(&svg) {
        if bottom.contains(&seg) {
            assert_eq!(thickness, "3", "bottom tick mark {seg:?}");
        } else if length(&seg) > 100 && seg.1 == seg.3 {
            assert_eq!(thickness, "1", "line {seg:?}");
        }
    }
}

#[test]
fn out_of_range_tick_mark_settings_do_not_crash_and_are_clamped() {
    // Lengths and thicknesses coming from a file or a script: anything goes.
    for (name, requested_length, thickness) in [
        ("nan", f64::NAN, f64::NAN),
        ("negative", -3.0, 0.0),
        ("huge", 1e30, 1e9),
        ("infinite", f64::INFINITY, f64::NEG_INFINITY),
    ] {
        let svg = export(&format!("advanced-bound-{name}"), |p| {
            p.bottom.minor_ticks = true;
            p.bottom.tick_length = Some(requested_length);
            p.bottom.minor_tick_length = Some(requested_length);
            p.bottom.tick_width = Some(thickness);
        });
        for (_, width, _) in strokes(&svg) {
            let width: u32 = width.parse().expect("integer thickness");
            assert!((1..=64).contains(&width), "{name}: thickness {width}");
        }
        for seg in bottom_tick_marks(&svg) {
            assert!(length(&seg) <= 50, "{name}: tick mark {seg:?} too long");
        }
    }
    // A negative length is not "Auto" but zero: no tick mark.
    let zero = export("advanced-bound-zero", |p| p.bottom.tick_length = Some(-3.0));
    assert!(bottom_tick_marks(&zero).is_empty());
    // NaN reads as Auto.
    let default = export("advanced-bound-default", |_| {});
    let nan = export("advanced-bound-nan", |p| {
        p.bottom.tick_length = Some(f64::NAN)
    });
    assert_eq!(bottom_tick_marks(&nan), bottom_tick_marks(&default));
}
