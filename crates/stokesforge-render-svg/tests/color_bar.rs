//! The colour of a curve on a colour bar (#499, step C) is computed at each
//! draw: it reaches the document, and the stored style is never touched.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{ColorBarSegment, ColorBarSpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

const STORED: &str = "#112233";

fn spec_with(project: &mut Project, bar: Option<ColorBarSpec>, x_primes: &[f64]) -> PlotSpec {
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add = |name: &str, x_prime: Option<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![0.0, 1.0, 2.0]),
            ColumnMeta {
                x_prime,
                ..ColumnMeta::default()
            },
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let x = add("X", None);
    let mut panel = PanelSpec::at(0, 0);
    for (name, x_prime) in ["A", "B", "C", "D"]
        .into_iter()
        .zip(x_primes.iter().copied())
    {
        let y = add(name, Some(x_prime));
        let mut series = SeriesSpec::new_xy(x, y);
        series.group = Some(1);
        series.line.color = STORED.to_string();
        series.symbol.color = STORED.to_string();
        panel.series.push(series);
    }
    panel
        .group_gradients
        .insert(1, vec!["#10ff20".to_string(), "#ff1020".to_string()]);
    panel.color_bar = bar;
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

fn export(spec: &PlotSpec, project: &Project) -> String {
    let plot = resolve_spec(project.tree(), spec);
    render_to_string(&plot, (480, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase()
}

fn bar() -> ColorBarSpec {
    ColorBarSpec {
        segments: vec![ColorBarSegment::new(1)],
        ..ColorBarSpec::default()
    }
}

/// Not baked: the curves are painted in the colours their values take
/// (the two ends of the gradient), the stored colour is not painted, and the
/// spec still holds it. Witness: without the bar the stored colour is
/// painted and the gradient ends are not.
#[test]
fn the_value_colour_is_painted_and_never_stored() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), &[0.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#10FF20"), "start colour missing");
    assert!(document.contains("#FF1020"), "end colour missing");
    assert!(
        !document.contains("#112233"),
        "the stored colour is painted"
    );
    assert_eq!(spec.panels[0].series[0].line.color, STORED);
    assert_eq!(spec.panels[0].series[1].symbol.color, STORED);

    let mut other = Project::new("Plain").expect("project");
    let plain = spec_with(&mut other, None, &[0.0, 57.0]);
    let document = export(&plain, &other);
    assert!(document.contains("#112233"), "witness: stored colour");
    assert!(!document.contains("#10FF20"), "witness: no value colour");
}

/// Editing a value recolours at the next draw, the spec staying as it was
/// (T1): the same spec with x' = 57 / 57 paints both curves in the start
/// colour (all values equal).
#[test]
fn equal_values_paint_the_start_colour_for_everyone() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), &[57.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#10FF20"));
    // The bar's own stripes may reach the end colour: compare with a spread
    // of values, where a curve is painted in it as well.
    let mut other = Project::new("Spread").expect("project");
    let spread = export(&spec_with(&mut other, Some(bar()), &[0.0, 57.0]), &other);
    assert!(
        document.matches("#FF1020").count() < spread.matches("#FF1020").count(),
        "no curve in the end colour when equal"
    );
}

/// A hidden bar acts as no bar: the stored colours come back.
#[test]
fn a_hidden_bar_gives_the_stored_colours_back() {
    let mut project = Project::new("Bar").expect("project");
    let mut hidden = bar();
    hidden.visible = false;
    let spec = spec_with(&mut project, Some(hidden), &[0.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#112233"));
    assert!(!document.contains("#10FF20"));
}

/// The tab reports the curves of the bar that have no value (C8): both have
/// an x', neither has a comment; a hidden bar reports nothing.
/// Witness: counting every member (not only those without a color) fails the
/// first assertion.
#[test]
fn the_curves_without_a_value_are_counted() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), &[0.0, 57.0]);
    let count = |spec: &PlotSpec| {
        let plot = resolve_spec(project.tree(), spec);
        stokesforge_render::color_bar::missing_value_count(&plot.panels[0])
    };
    assert_eq!(count(&spec), 0);
    let mut comments = spec.clone();
    comments
        .panels
        .get_mut(0)
        .expect("panel")
        .color_bar
        .as_mut()
        .expect("bar")
        .value = stokesforge_plot::ColorBarValue::Comments;
    assert_eq!(count(&comments), 2);
    comments
        .panels
        .get_mut(0)
        .expect("panel")
        .color_bar
        .as_mut()
        .expect("bar")
        .visible = false;
    assert_eq!(count(&comments), 0);
}

fn log_bar() -> ColorBarSpec {
    ColorBarSpec {
        scale: stokesforge_plot::ColorBarScale::Log,
        ..bar()
    }
}

/// One value <= 0 on a log scale (#499, C8/C9): only that curve has no value
/// (it keeps its own colour); the others keep the colours of the range
/// 1..100 and the bar keeps its ticks. Witness: the old rule (every curve of
/// the segment loses its value) paints no gradient end at all.
#[test]
fn a_zero_on_a_log_bar_only_costs_its_own_curve() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(log_bar()), &[0.0, 1.0, 10.0, 100.0]);
    let document = export(&spec, &project);
    assert!(
        document.contains("#112233"),
        "the zero curve keeps its colour"
    );
    // The curves at 1 and 100 are the two ends of the gradient: count them in
    // the curve paths (stroke), not only in the bar stripes.
    let curves = |colour: &str| document.matches(&format!("STROKE=\"{colour}\"")).count();
    assert!(curves("#10FF20") >= 1 && curves("#FF1020") >= 1);
    let has_tick = |label: &str| document.lines().any(|line| line == label);
    assert!(has_tick("100"), "end tick missing");
    assert!(has_tick("1"), "start tick missing");
    let plot = resolve_spec(project.tree(), &spec);
    assert_eq!(
        stokesforge_render::color_bar::missing_value_count(&plot.panels[0]),
        1
    );
}

fn font_sizes(document: &str) -> std::collections::BTreeSet<String> {
    document
        .split("FONT-SIZE=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// The bar's own font (title, ticks): size 30 in the bar reaches the SVG,
/// the default bar has no such size. Witness: a bar font not read leaves 24.19 (30 scaled)
/// out.
#[test]
fn the_bar_font_size_reaches_the_export() {
    let mut project = Project::new("Bar").expect("project");
    let mut big = bar();
    big.font.size = Some(30.0);
    big.title = "Time".to_string();
    let spec = spec_with(&mut project, Some(big), &[0.0, 57.0]);
    let sized = font_sizes(&export(&spec, &project));
    assert!(
        sized.iter().any(|size| size.starts_with("24.19")),
        "{sized:?}"
    );
    let mut other = Project::new("Default").expect("project");
    let plain = spec_with(&mut other, Some(bar()), &[0.0, 57.0]);
    assert!(!font_sizes(&export(&plain, &other))
        .iter()
        .any(|size| size.starts_with("24.19")));
}

/// `(x, y, width, height, fill)` of every rectangle of the document that is a
/// gradient stripe (the strip is 12 high), left to right.
fn stripes(document: &str) -> Vec<(i32, i32, i32, i32, String)> {
    stripes_high(document, 12)
}

/// Same, for a strip of `height` points.
fn stripes_high(document: &str, height: i32) -> Vec<(i32, i32, i32, i32, String)> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let key = format!("{name}=\"");
        let rest = tag.split(&key).nth(1)?;
        Some(rest.split('"').next()?.to_string())
    };
    let mut out: Vec<(i32, i32, i32, i32, String)> = document
        .split("<RECT")
        .skip(1)
        .filter_map(|tag| {
            let tag = tag.split('>').next()?;
            let number = |name: &str| attr(tag, name)?.parse::<f64>().ok().map(|v| v as i32);
            Some((
                number("X")?,
                number("Y")?,
                number("WIDTH")?,
                number("HEIGHT")?,
                attr(tag, "FILL")?,
            ))
        })
        .filter(|stripe| stripe.3 == height)
        .collect();
    out.sort();
    out
}

fn channels(hex: &str) -> [i32; 3] {
    let value = i32::from_str_radix(hex.trim_start_matches('#'), 16).expect("hex");
    [(value >> 16) & 255, (value >> 8) & 255, value & 255]
}

fn two_group_spec(project: &mut Project, chain: bool) -> PlotSpec {
    let mut spec = spec_with(project, Some(bar()), &[0.0, 20.0, 40.0, 57.0]);
    let panel = spec.panels.get_mut(0).expect("panel");
    for series in panel.series.iter_mut().skip(2) {
        series.group = Some(2);
    }
    panel
        .group_gradients
        .insert(2, vec!["#ff1020".to_string(), "#ffa000".to_string()]);
    let bar = panel.color_bar.as_mut().expect("bar");
    bar.segments.push(ColorBarSegment::new(2));
    bar.chain = chain;
    bar.segments[0].label = "switch".to_string();
    spec
}

/// The gradient is smooth, not a few blocks (#499): across a wide bar the
/// stripes take more than a hundred distinct colours, two neighbours never
/// differ by more than a few levels, and the stripes leave no hole in a
/// segment. Witness: one stripe per member colour (4 curves) or a 12-pixel
/// stripe fails the count and the neighbour difference.
#[test]
fn the_gradient_is_smooth_and_has_no_hole() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), &[0.0, 20.0, 40.0, 57.0]);
    let plot = resolve_spec(project.tree(), &spec);
    let document = render_to_string(&plot, (900, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase();
    let mut body: Vec<_> = stripes(&document)
        .into_iter()
        .filter(|stripe| stripe.2 >= 3)
        .collect();
    body.dedup_by_key(|stripe| stripe.0);
    let distinct: std::collections::BTreeSet<&String> =
        body.iter().map(|stripe| &stripe.4).collect();
    assert!(distinct.len() >= 100, "{} colours", distinct.len());
    for pair in body.windows(2) {
        let (a, b) = (channels(&pair[0].4), channels(&pair[1].4));
        let worst = (0..3).map(|c| (a[c] - b[c]).abs()).max().expect("channels");
        assert!(worst <= 6, "jump of {worst} levels at x = {}", pair[1].0);
        assert!(
            pair[1].0 <= pair[0].0 + pair[0].2,
            "hole between x = {} and x = {}",
            pair[0].0,
            pair[1].0
        );
    }
}

/// A break (#499): the two segments are separate bars with an empty gap, the
/// marker is centred on the gap and touches the top of the bar, the break
/// tick starts at the bar's bottom edge (nothing dark crosses the colours),
/// and the value label is centred on the gap.
#[test]
fn the_break_is_centred_on_a_clean_gap() {
    let mut project = Project::new("Bar").expect("project");
    let spec = two_group_spec(&mut project, true);
    let plot = resolve_spec(project.tree(), &spec);
    let document = render_to_string(&plot, (900, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase();
    let strip = stripes(&document);
    let top = strip.iter().map(|stripe| stripe.1).min().expect("strip");
    let bottom = top + 12;
    // The gap: the widest empty interval between two stripes.
    let mut spans: Vec<(i32, i32)> = strip
        .iter()
        .map(|stripe| (stripe.0, stripe.0 + stripe.2))
        .collect();
    spans.sort();
    let mut reach = spans[0].1;
    let mut gap = (0, 0);
    for (from, to) in &spans {
        if *from - reach > gap.1 - gap.0 {
            gap = (reach, *from);
        }
        reach = reach.max(*to);
    }
    assert!(gap.1 - gap.0 >= 4, "no clean gap: {gap:?}");
    let centre = (gap.0 + gap.1) / 2;
    // The marker: apex on the bar's top edge, centred on the gap.
    let polygon = document
        .split("<POLYGON")
        .nth(1)
        .and_then(|rest| rest.split("POINTS=\"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .expect("marker");
    let points: Vec<(i32, i32)> = polygon
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        })
        .collect();
    let apex = points.iter().max_by_key(|point| point.1).expect("apex");
    assert_eq!(apex.1, top, "the marker touches the bar");
    assert!(
        (apex.0 - centre).abs() <= 1,
        "marker {} gap {centre}",
        apex.0
    );
    // No dark mark over the colours: every polyline (tick dash) lies above
    // the bar's top or below its bottom edge.
    for line in document.split("<POLYLINE").skip(1) {
        let Some(points) = line
            .split("POINTS=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        for pair in points.split_whitespace() {
            let Some((x, y)) = pair.split_once(',') else {
                continue;
            };
            let (x, y): (i32, i32) = (x.parse().unwrap_or(0), y.parse().unwrap_or(0));
            let over_bar = y > top && y < bottom && x >= spans[0].0 && x <= reach;
            // Plot curves cross other areas: only marks near the bar count.
            let near = y >= top - 2 && y <= bottom + 2;
            assert!(!(over_bar && near), "a mark crosses the bar at ({x}, {y})");
        }
    }
    // The value label is centred on the gap.
    let label = document
        .split("<TEXT")
        .skip(1)
        .find(|text| {
            text.split('>')
                .nth(1)
                .is_some_and(|content| content.trim_start().starts_with("20"))
        })
        .expect("break value label");
    let x: f64 = label
        .split(" X=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .and_then(|value| value.parse().ok())
        .expect("x");
    assert!((x as i32 - centre).abs() <= 1, "label {x} gap {centre}");
}

/// `(x, anchor, content)` of every text of the document.
fn texts(document: &str) -> Vec<(f64, String, String)> {
    document
        .split("<TEXT")
        .skip(1)
        .filter_map(|text| {
            let (head, rest) = text.split_once('>')?;
            let attr = |name: &str| -> Option<String> {
                let key = format!(" {name}=\"");
                Some(head.split(&key).nth(1)?.split('"').next()?.to_string())
            };
            Some((
                attr("X")?.parse().ok()?,
                attr("TEXT-ANCHOR").unwrap_or_else(|| "start".to_string()),
                rest.split('<').next()?.trim().to_string(),
            ))
        })
        .collect()
}

/// A break between values that differ (no chain, 20 then 40) has two ticks,
/// one on each edge of the gap, the first label ending at its tick and the
/// second starting at its own, so the boxes never overlap; a chain (or data
/// that meet) keeps one label centred on the gap. Witness: both labels
/// centred (the old look) put the "20" and the "40" boxes on each other.
#[test]
fn a_break_between_different_values_has_two_labels_apart() {
    let mut project = Project::new("Bar").expect("project");
    let spec = two_group_spec(&mut project, false);
    let plot = resolve_spec(project.tree(), &spec);
    let document = render_to_string(&plot, (900, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase();
    let all = texts(&document);
    let find = |content: &str| {
        all.iter()
            .find(|text| text.2 == content)
            .unwrap_or_else(|| panic!("no label {content}: {all:?}"))
    };
    let (before, after) = (find("20"), find("40"));
    assert_eq!(before.1, "END", "end label right-aligned: {before:?}");
    assert_eq!(after.1, "START", "start label left-aligned: {after:?}");
    assert!(after.0 - before.0 >= 6.0, "{before:?} {after:?}");

    let mut other = Project::new("Chain").expect("project");
    let chained = two_group_spec(&mut other, true);
    let plot = resolve_spec(other.tree(), &chained);
    let document = render_to_string(&plot, (900, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase();
    let all = texts(&document);
    assert_eq!(all.iter().filter(|text| text.2 == "20").count(), 1);
    assert!(!all.iter().any(|text| text.2 == "40"), "{all:?}");
}

/// A bar in a corner with a custom length and thickness (#499): the strip is
/// as long and as thick as asked, in the export as on screen (one drawing
/// code), and **no text changes size**: the same font sizes appear with and
/// without the custom size. The automatic size is back with `None`.
/// Witness: a font scaled with the strip, a length not read, or a thickness
/// stuck at 12 fails the assertions.
#[test]
fn a_custom_size_resizes_the_strip_and_not_the_texts() {
    let mut project = Project::new("Bar").expect("project");
    let mut plain = bar();
    plain.position = stokesforge_plot::ColorBarPosition::TopRight;
    plain.title = "Time".to_string();
    let automatic = spec_with(&mut project, Some(plain.clone()), &[0.0, 57.0]);
    let mut sized = plain;
    sized.length = Some(210.0);
    sized.thickness = Some(20.0);
    let custom = spec_with(&mut project, Some(sized), &[0.0, 57.0]);

    let (before, after) = (export(&automatic, &project), export(&custom, &project));
    assert!(stripes_high(&before, 20).is_empty());
    let strip = stripes_high(&after, 20);
    assert!(strip.len() > 50, "{} stripes of 20 points", strip.len());
    let left = strip.iter().map(|s| s.0).min().expect("stripes");
    let right = strip.iter().map(|s| s.0 + s.2).max().expect("stripes");
    // The body leaves the rounded caps (4 points each end) to other shapes.
    assert!((right - left - 202).abs() <= 1, "length {}", right - left);
    assert_eq!(
        font_sizes(&before),
        font_sizes(&after),
        "texts keep their size"
    );
}

/// Below the frame the length keeps following the frame, the thickness
/// applies. Witness: a stored length that shortens the strip below the frame.
#[test]
fn below_the_frame_only_the_thickness_applies() {
    let mut project = Project::new("Bar").expect("project");
    let mut below = bar();
    below.position = stokesforge_plot::ColorBarPosition::Below;
    let automatic = spec_with(&mut project, Some(below.clone()), &[0.0, 57.0]);
    below.length = Some(40.0);
    below.thickness = Some(20.0);
    let custom = spec_with(&mut project, Some(below), &[0.0, 57.0]);
    let (before, after) = (
        stripes(&export(&automatic, &project)),
        stripes_high(&export(&custom, &project), 20),
    );
    let width = |s: &[(i32, i32, i32, i32, String)]| {
        s.iter().map(|s| s.0 + s.2).max().unwrap_or(0) - s.iter().map(|s| s.0).min().unwrap_or(0)
    };
    assert!(!after.is_empty(), "the thickness applies");
    assert_eq!(
        width(&before),
        width(&after),
        "the length follows the frame"
    );
}
