//! The preview of the analysis window (#181, mockups M3 and M4): the series
//! shown, the live result of the calculation (baseline, fit, peaks,
//! residuals) and the markers placed (points, zones, peak positions).
//!
//! The preview paints itself; it does not go through `GraphView`: it has no
//! layer, no legend, no history, and its clicks **write into the form**
//! (`AnalysisForm::add_point`, `add_zone`, `remove_near`), not into the
//! model. Its **painting** stays here; its **geometry** (pixel ↔ data,
//! zoom, ticks, snapping, initial frame) comes from `stokesforge_render`
//! (`PanelTransform`, `axis_ticks`, `snap_to_curve`, `axis_frame`, #261): the
//! application only converts to `Pos2`.
//!
//! # Gestures
//!
//! As on the graph (#270, D3 of #299; matrix: `docs/gestes.md`):
//!
//! - **Ctrl + wheel**: zoom anchored under the pointer; over an axis, that
//!   axis alone. The wheel alone does nothing;
//! - **Ctrl + drag**: moves the view (a drag that starts on an axis moves
//!   only that axis); the modifier is read at the start of the drag;
//! - **double click**: original framing;
//! - **Shift + wheel**: sets the average N of the magnet (M4 b);
//! - *Points* tool: a click places a point; **Shift** snaps it to the curve
//!   with a local average of N points; the snap circle grows with the
//!   averaged zone (width in X converted to pixels, #334);
//! - *Zone* tool: dragging draws a zone or a range;
//! - right click: removes the targeted marker.
//!
//! Zoom and pan are computed by `stokesforge_render` (`PanelTransform::zoom`,
//! `pan`); the preview has no `ViewState` (no layer, no drawn panels): it
//! keeps its frame and only chooses the target of the gesture, the whole plot
//! or one axis.
//!
//! # The calculation is not done in `ui()` (#201)
//!
//! The preview is computed on **a worker thread** (only one at a time, named
//! `stokesforge-analysis-preview`): a multi-peak fit on tens of thousands of
//! points no longer freezes the window. Each frame compares the form and the
//! model revision with those of the last result; if they differ, the request
//! is posted **replacing** the one that was waiting (the obsolete result is
//! dropped, a calculation in progress is not interrupted but its result is
//! ignored if it no longer matches). The frame waits for the thread at most
//! [`INLINE_WAIT`]: a small calculation returns its result in the same frame,
//! without flicker; a big one leaves the old result (marked "in progress")
//! and the thread wakes the viewport **once** when it ends. No periodic
//! wake-up at rest.
//!
//! A frame where nothing changes redoes nothing.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_core::{Computed, Tree};
use stokesforge_plot::{AxisScale, AxisSpec};
use stokesforge_render::scale::axis_ticks;
use stokesforge_render::{
    axis_frame, fit_range, snap_to_curve, snap_to_curve_zone, Gesture, PanelTransform,
};

use crate::theme;
use stokesforge_app_state::analysis::{
    issue_message, series_points, series_rows, AnalysisForm, Issue, PreviewTool, AVERAGE_RANGE,
};
use stokesforge_app_state::tr;

/// The visible data frame: the bounds of X then of Y, **in display order**
/// (a reversed axis has its high bound first), as [`PanelTransform::new`]
/// takes them.
type Frame = ((f64, f64), (f64, f64));

/// The axes (bottom, ordinate) of the graph curve the current series comes
/// from; `None` without a source graph: the preview is then linear and not
/// reversed.
fn source_axes(form: &AnalysisForm) -> Option<(&AxisSpec, &AxisSpec)> {
    let (spec, sources) = form.graph_source.as_ref()?;
    let source = sources.get(form.current)?;
    let panel = spec.panels.get(source.panel)?;
    let curve = panel.series.get(source.index)?;
    Some((&panel.bottom, panel.axis(curve.axis.side())))
}

/// The transform of `rect` for these bounds; `None` for a rectangle with no
/// area. The egui rectangle is fractional: origin and size pass to `f64`
/// without rounding.
fn transform_for(
    rect: egui::Rect,
    x: (f64, f64),
    y: (f64, f64),
    scales: (AxisScale, AxisScale),
) -> Option<PanelTransform> {
    PanelTransform::new(
        (f64::from(rect.min.x), f64::from(rect.min.y)),
        (f64::from(rect.width()), f64::from(rect.height())),
        x,
        y,
        scales.0,
        scales.1,
    )
}

/// The pixel of a data point; `None` if it has no image (outside a
/// logarithmic scale, not finite).
fn to_screen(transform: &PanelTransform, x: f64, y: f64) -> Option<egui::Pos2> {
    let (px, py) = transform.data_to_pixel(x, y)?;
    Some(egui::pos2(px as f32, py as f32))
}

/// Minimal radius of the snap circle: that of an average of a single point.
const SNAP_RADIUS_MIN: f32 = 6.0;

/// Radius in pixels of the snap circle (#334): the larger of the two
/// half-sides of the averaged zone, measured from the snapped point `center`
/// (in X, screen) to the edges `edge_a` and `edge_b`. It does not go below
/// [`SNAP_RADIUS_MIN`] nor above `max`; a non-finite pixel gives the
/// minimum (invariant 10: no panic, no NaN). The distances are absolute: a
/// reversed X axis gives the same radius.
fn snap_radius(center: f32, edge_a: f32, edge_b: f32, max: f32) -> f32 {
    // `f32::max` swallows a NaN: we test each pixel before comparing.
    if ![center, edge_a, edge_b, max].iter().all(|v| v.is_finite()) {
        return SNAP_RADIUS_MIN;
    }
    let half = (edge_a - center).abs().max((edge_b - center).abs());
    half.min(max).max(SNAP_RADIUS_MIN)
}

/// The data under a pixel.
fn to_data(transform: &PanelTransform, pos: egui::Pos2) -> Option<(f64, f64)> {
    transform.pixel_to_data(f64::from(pos.x), f64::from(pos.y))
}

/// What the preview returns to the window: the values of the series shown, or
/// what prevents computing.
#[derive(Debug, Clone, Default)]
pub(super) struct PreviewOutcome {
    pub values: Vec<stokesforge_core::ResultValue>,
    pub warnings: Vec<stokesforge_analysis::ops::Warning>,
    pub skipped: usize,
    pub issue: Option<Issue>,
    /// The calculation is in progress: the values shown are those from before.
    pub pending: bool,
}

/// How long a frame waits for the calculation thread before showing the old
/// result. Tests wait for the result to be deterministic.
#[cfg(not(test))]
const INLINE_WAIT: Duration = Duration::from_millis(40);
#[cfg(test)]
const INLINE_WAIT: Duration = Duration::from_secs(30);

/// A preview result and what it was computed for.
struct PreviewCache {
    form: AnalysisForm,
    revision: u64,
    result: Result<Computed, Issue>,
}

impl PreviewCache {
    fn matches(&self, form: &AnalysisForm, revision: u64) -> bool {
        self.revision == revision && self.form.same_preview(form)
    }
}

/// What the calculation thread must compute.
struct Request {
    form: AnalysisForm,
    revision: u64,
    tree: Arc<Tree>,
}

#[derive(Default)]
struct JobState {
    /// A calculation thread is running.
    running: bool,
    /// The most recent request, posted while a calculation was running.
    next: Option<Request>,
    /// The last finished result.
    finished: Option<Arc<PreviewCache>>,
    /// What was requested most recently: the same thing is not requested
    /// again on every frame.
    requested: Option<(AnalysisForm, u64)>,
}

/// The calculation of a request; a function so that tests can put a slow,
/// countable calculation in it.
type Compute = fn(&Request) -> Result<Computed, Issue>;

fn compute_preview(request: &Request) -> Result<Computed, Issue> {
    request.form.preview(&request.tree)
}

/// The calculation thread of an analysis window, kept in egui memory.
struct PreviewJobs {
    state: Mutex<JobState>,
    done: Condvar,
    compute: Compute,
}

impl PreviewJobs {
    fn new(compute: Compute) -> Self {
        Self {
            state: Mutex::new(JobState::default()),
            done: Condvar::new(),
            compute,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, JobState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Requests the calculation of `form` at `revision`, unless already requested.
    fn request(
        self: &Arc<Self>,
        context: &egui::Context,
        form: &AnalysisForm,
        tree: &Arc<Tree>,
        revision: u64,
    ) {
        let mut state = self.lock();
        if state
            .requested
            .as_ref()
            .is_some_and(|(asked, rev)| *rev == revision && asked.same_preview(form))
        {
            return;
        }
        state.requested = Some((form.clone(), revision));
        let request = Request {
            form: form.clone(),
            revision,
            tree: tree.clone(),
        };
        if state.running {
            state.next = Some(request);
            return;
        }
        state.running = true;
        drop(state);
        let (jobs, ctx) = (self.clone(), context.clone());
        let fallback = Request {
            form: request.form.clone(),
            revision,
            tree: request.tree.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name("stokesforge-analysis-preview".into())
            .spawn(move || jobs.work(&ctx, request));
        if spawned.is_err() {
            // No thread available: the calculation is done on the caller, as
            // before #201, rather than never returning a result.
            self.work(context, fallback);
        }
    }

    /// The thread body: computes the request, publishes, chains onto the most
    /// recent one if there is one, then stops.
    fn work(&self, context: &egui::Context, mut request: Request) {
        loop {
            let result = (self.compute)(&request);
            let mut state = self.lock();
            state.finished = Some(Arc::new(PreviewCache {
                form: request.form,
                revision: request.revision,
                result,
            }));
            self.done.notify_all();
            context.request_repaint();
            match state.next.take() {
                Some(next) => request = next,
                None => {
                    state.running = false;
                    return;
                }
            }
        }
    }

    /// The last finished result, whatever it is.
    fn latest(&self) -> Option<Arc<PreviewCache>> {
        self.lock().finished.clone()
    }

    /// Waits at most `timeout` for the result of `form` at `revision`.
    fn wait_for(
        &self,
        form: &AnalysisForm,
        revision: u64,
        timeout: Duration,
    ) -> Option<Arc<PreviewCache>> {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if let Some(done) = state
                .finished
                .as_ref()
                .filter(|c| c.matches(form, revision))
            {
                return Some(done.clone());
            }
            if !state.running {
                return None;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            state = self
                .done
                .wait_timeout(state, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

#[derive(Clone)]
struct StoredView {
    key: (usize, u64),
    source: Option<(stokesforge_plot::PlotSpec, Vec<stokesforge_plot::SeriesRef>)>,
    frame: Frame,
}

/// What a navigation gesture targets: the whole plot, or a single scale
/// (the X tick band under the plot, the Y one on its left).
#[derive(Clone, Copy, Debug, PartialEq)]
enum NavTarget {
    Plot,
    AxisX,
    AxisY,
}

/// What the drag in progress does, decided on its first frame (like
/// `nav_drag` of the graph): draw a zone, or move the view (Ctrl held at the
/// start), or nothing (started outside the plot and the axes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DragMode {
    Draw,
    Pan(NavTarget),
    #[default]
    Ignore,
}

/// The target of a navigation gesture at `pos`: the plot, the X axis (below
/// the plot, residuals included), the Y axis (on its left), or nothing.
fn nav_target(pos: egui::Pos2, plot_rect: egui::Rect, outer: egui::Rect) -> Option<NavTarget> {
    if plot_rect.contains(pos) {
        Some(NavTarget::Plot)
    } else if pos.x >= plot_rect.left()
        && pos.x <= plot_rect.right()
        && pos.y > plot_rect.bottom()
        && pos.y <= outer.bottom()
    {
        Some(NavTarget::AxisX)
    } else if pos.x >= outer.left()
        && pos.x < plot_rect.left()
        && pos.y >= plot_rect.top()
        && pos.y <= plot_rect.bottom()
    {
        Some(NavTarget::AxisY)
    } else {
        None
    }
}

/// The frame after `gesture` on `target`. On an axis, the other component of
/// the gesture is neutralized at the center of the plot (like
/// `ViewState::commit_axis_gesture`) and the other bound of the frame does
/// not move. With no image (logarithmic bound ≤ 0), the frame stays.
fn navigate(
    transform: &PanelTransform,
    plot_rect: egui::Rect,
    frame: Frame,
    target: NavTarget,
    gesture: Gesture,
) -> Frame {
    let center = plot_rect.center();
    let (cx, cy) = (f64::from(center.x), f64::from(center.y));
    let gesture = match (gesture, target) {
        (Gesture::Zoom { anchor, factor }, NavTarget::AxisX) => Gesture::Zoom {
            anchor: (anchor.0, cy),
            factor,
        },
        (Gesture::Zoom { anchor, factor }, NavTarget::AxisY) => Gesture::Zoom {
            anchor: (cx, anchor.1),
            factor,
        },
        (Gesture::Pan { dx, .. }, NavTarget::AxisX) => Gesture::Pan { dx, dy: 0.0 },
        (Gesture::Pan { dy, .. }, NavTarget::AxisY) => Gesture::Pan { dx: 0.0, dy },
        (other, _) => other,
    };
    match transform.apply(&gesture) {
        Some((x, y)) => match target {
            NavTarget::Plot => (x, y),
            NavTarget::AxisX => (x, frame.1),
            NavTarget::AxisY => (frame.0, y),
        },
        None => frame,
    }
}

/// The typing pause after which a preview is recomputed (S-T1, #224), in
/// seconds. Text **and** numbers: each keystroke in a field does not restart
/// the calculation.
pub(super) const PREVIEW_PAUSE: f64 = 0.3;

/// Beyond this time without a painted frame, the window was closed then
/// reopened: the form is not being typed in.
const REOPEN_GAP: f64 = 1.0;

/// Has the form stayed unchanged since [`PREVIEW_PAUSE`]? The form of the
/// previous frame, the instant of its last change and that of the last frame
/// are kept in egui memory; while it moves, a frame is requested at the end
/// of the pause (no periodic wake-up at rest). When a window opens, the
/// preview is computed at once: there is no typing to wait for.
fn settled(ui: &egui::Ui, id: egui::Id, form: &AnalysisForm) -> bool {
    let key = id.with("preview_pause");
    let now = ui.input(|input| input.time);
    let since = ui.data_mut(|data| {
        let (last, since, seen) = data
            .get_temp::<(AnalysisForm, f64, f64)>(key)
            .unwrap_or_else(|| (form.clone(), f64::NEG_INFINITY, now));
        let since = if now - seen > REOPEN_GAP {
            f64::NEG_INFINITY
        } else if last.same_preview(form) {
            since
        } else {
            now
        };
        data.insert_temp(key, (form.clone(), since, now));
        since
    });
    let waited = now - since;
    if waited < PREVIEW_PAUSE {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(
                PREVIEW_PAUSE - waited + 0.01,
            ));
        return false;
    }
    true
}

/// The wheel notches of this frame (positive upward), all axes together: one
/// raw event per notch, whatever the egui smoothing.
fn wheel_notches(ui: &egui::Ui) -> i32 {
    let total: f32 = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } => {
                    let amount = delta.x + delta.y;
                    Some(match unit {
                        egui::MouseWheelUnit::Line => amount,
                        egui::MouseWheelUnit::Page => amount * 10.0,
                        egui::MouseWheelUnit::Point => amount / 50.0,
                    })
                }
                _ => None,
            })
            .sum()
    });
    if total == 0.0 {
        return 0;
    }
    total.abs().round().max(1.0) as i32 * total.signum() as i32
}

// Paints the preview in the remaining space of `ui` and applies its gestures to the
/// form.
pub(super) fn preview(
    ui: &mut egui::Ui,
    id: egui::Id,
    form: &mut AnalysisForm,
    tree: &Arc<Tree>,
    revision: u64,
) -> PreviewOutcome {
    form.clamp_current();
    let jobs_id = id.with("jobs");
    let jobs = match ui.data_mut(|d| d.get_temp::<Arc<PreviewJobs>>(jobs_id)) {
        Some(jobs) => jobs,
        None => {
            let jobs = Arc::new(PreviewJobs::new(compute_preview));
            ui.data_mut(|d| d.insert_temp(jobs_id, jobs.clone()));
            jobs
        }
    };
    let up_to_date = jobs.latest().filter(|c| c.matches(form, revision));
    let cache = match (up_to_date, jobs.latest()) {
        (Some(cache), _) => cache,
        // A preview is recomputed after a typing pause (S-T1, #224): while the
        // form moves, the old result is kept, marked in progress.
        (None, Some(latest)) if !settled(ui, id, form) => latest,
        (None, _) => {
            jobs.request(ui.ctx(), form, tree, revision);
            jobs.wait_for(form, revision, INLINE_WAIT)
                .or_else(|| jobs.latest())
                .unwrap_or_else(|| {
                    // Nothing has been computed yet (thread unavailable or first
                    // long calculation): an empty result, marked in progress.
                    Arc::new(PreviewCache {
                        form: form.clone(),
                        revision: u64::MAX,
                        result: Ok(Computed::default()),
                    })
                })
        }
    };
    let pending = !cache.matches(form, revision);
    let rank = form.preview_rank();
    let outcome = match &cache.result {
        Ok(computed) => PreviewOutcome {
            pending,
            values: computed
                .results
                .get(rank)
                .map(|r| r.values.clone())
                .unwrap_or_default(),
            warnings: computed
                .results
                .get(rank)
                .map(|r| r.warnings.clone())
                .unwrap_or_default(),
            skipped: computed.results.get(rank).map_or(0, |r| r.skipped),
            issue: None,
        },
        Err(issue) => PreviewOutcome {
            pending,
            issue: Some(issue.clone()),
            ..PreviewOutcome::default()
        },
    };

    let Some(series) = form.series.get(form.current).cloned() else {
        return outcome;
    };
    let (xs, ys) = series_points(tree, &series);
    let (rows_x, _) = series_rows(tree, &series);
    let computed = cache.result.as_ref().ok();
    let column = |key: &str, index: Option<usize>| -> Option<&Vec<f64>> {
        computed?
            .columns
            .iter()
            .find(|c| c.series == rank && c.key == key && c.index == index)
            .map(|c| &c.data)
    };

    // The frame: remembered per series, reframed on double click. The scales and the
    // direction of the axes come from the source graph; the initial framing and the
    // geometry, from `stokesforge_render`.
    let axes = source_axes(form);
    let scales = (
        axes.map_or(AxisScale::Linear, |(x, _)| x.scale),
        axes.map_or(AxisScale::Linear, |(_, y)| y.scale),
    );
    let view_id = id.with("view");
    let key = (form.current, series.y.raw());
    let mut frame: Frame = ui
        .data_mut(|d| d.get_temp::<StoredView>(view_id))
        .filter(|s| s.key == key && s.source == form.graph_source)
        .map(|s| s.frame)
        .unwrap_or_else(|| {
            let mut all_y = ys.clone();
            for k in ["corrected", "smoothed", "deconvolved"] {
                if let Some(c) = column(k, None) {
                    all_y.extend(c.iter().copied());
                }
            }
            match axes {
                Some((x, y)) => (axis_frame(&xs, x), axis_frame(&all_y, y)),
                None => (
                    fit_range(&xs, AxisScale::Linear),
                    fit_range(&all_y, AxisScale::Linear),
                ),
            }
        });

    let dark = ui.visuals().dark_mode;
    let size = ui.available_size().max(egui::vec2(120.0, 120.0));
    let (outer, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let residuals = column("residuals", None);
    let plot_rect = {
        let mut r = outer.shrink2(egui::vec2(8.0, 4.0));
        r.min.x += 44.0;
        r.max.y -= 18.0;
        if residuals.is_some() {
            r.max.y -= r.height() * 0.22;
        }
        r
    };
    let painter = ui.painter_at(outer);
    painter.rect_filled(outer, 2.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        plot_rect,
        0.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    // A rectangle with no area, or bounds with no image: nothing to place.
    let Some(transform) = transform_for(plot_rect, frame.0, frame.1, scales) else {
        return outcome;
    };
    let text_color = ui.visuals().text_color();
    let font = egui::FontId::proportional(11.0);

    // Ticks: the step and the format of the graph (`axis_ticks`), the decimal
    // point as before.
    for (x, label) in axis_ticks(scales.0, frame.0, false) {
        if let Some(p) = to_screen(&transform, x, frame.1 .0) {
            painter.text(
                egui::pos2(p.x, plot_rect.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                label,
                font.clone(),
                text_color,
            );
        }
    }
    for (y, label) in axis_ticks(scales.1, frame.1, false) {
        if let Some(p) = to_screen(&transform, frame.0 .0, y) {
            painter.text(
                egui::pos2(plot_rect.left() - 3.0, p.y),
                egui::Align2::RIGHT_CENTER,
                label,
                font.clone(),
                text_color,
            );
        }
    }

    let plot = painter.with_clip_rect(plot_rect);
    let curve = |plot: &egui::Painter, ys: &[f64], stroke: egui::Stroke| {
        let mut run: Vec<egui::Pos2> = Vec::new();
        let flush = |run: &mut Vec<egui::Pos2>| {
            if run.len() > 1 {
                plot.add(egui::Shape::line(std::mem::take(run), stroke));
            } else {
                run.clear();
            }
        };
        for (x, y) in rows_x.iter().zip(ys) {
            let point = (x.is_finite() && y.is_finite())
                .then(|| to_screen(&transform, *x, *y))
                .flatten()
                .filter(|point| point.x.is_finite() && point.y.is_finite());
            match point {
                Some(point) => run.push(point),
                None => flush(&mut run),
            }
        }
        flush(&mut run);
    };

    // Zones and ranges.
    let base = frame.1 .0;
    let band = |plot: &egui::Painter, a: f64, b: f64| {
        let (Some(l), Some(r)) = (
            to_screen(&transform, a, base),
            to_screen(&transform, b, base),
        ) else {
            return;
        };
        plot.rect_filled(
            egui::Rect::from_x_y_ranges(l.x..=r.x, plot_rect.y_range()),
            0.0,
            theme::preview_zone_fill(),
        );
    };
    for (a, b) in zones_of(form) {
        band(&plot, a, b);
    }

    // Data, then results.
    let data_stroke = egui::Stroke::new(1.2, text_color);
    {
        let (_, all_y) = series_rows(tree, &series);
        let source = form.graph_source.as_ref().and_then(|(spec, sources)| {
            let source = sources.get(form.current)?;
            spec.panels.get(source.panel)?.series.get(source.index)
        });
        if let Some(source) = source {
            let pixels: Vec<_> = rows_x
                .iter()
                .zip(&all_y)
                .map(|(x, y)| {
                    if !x.is_finite() || !y.is_finite() {
                        return (f64::NAN, f64::NAN);
                    }
                    match to_screen(&transform, *x, *y) {
                        Some(p) => {
                            let p = p - plot_rect.min;
                            (f64::from(p.x), f64::from(p.y))
                        }
                        None => (f64::NAN, f64::NAN),
                    }
                })
                .collect();
            plot.extend(stokesforge_render_egui::analysis_curve(
                source, plot_rect, &pixels,
            ));
        } else {
            curve(&plot, &all_y, data_stroke);
        }
    }
    let helper = egui::Stroke::new(1.5, theme::preview_helper_color(dark));
    let result = egui::Stroke::new(1.8, theme::preview_result_color(dark));
    for k in ["baseline", "background"] {
        if let Some(c) = column(k, None) {
            curve(&plot, c, helper);
        }
    }
    for k in ["fit", "corrected", "smoothed", "peaks_sum", "deconvolved"] {
        if let Some(c) = column(k, None) {
            curve(&plot, c, result);
        }
    }
    if let Some(Ok(computed)) = Some(&cache.result) {
        for c in computed
            .columns
            .iter()
            .filter(|c| c.series == rank && c.key == "peak")
        {
            let index = c.index.unwrap_or(1);
            curve(
                &plot,
                &c.data,
                egui::Stroke::new(1.0, theme::preview_peak_color(index)),
            );
        }
    }

    // Markers placed.
    let marker = theme::preview_marker_color();
    for (px, py) in marks_of(form) {
        if let Some(p) = to_screen(&transform, px, py) {
            plot.circle(p, 4.0, marker, egui::Stroke::new(1.0, text_color));
        }
    }
    for cx in seeds_of(form) {
        if let (Some(top), Some(bottom)) = (
            to_screen(&transform, cx, frame.1 .1),
            to_screen(&transform, cx, frame.1 .0),
        ) {
            plot.line_segment([top, bottom], egui::Stroke::new(1.0, marker));
        }
    }

    // Residuals under the curve.
    if let Some(res) = residuals {
        let strip = egui::Rect::from_min_max(
            egui::pos2(plot_rect.left(), plot_rect.bottom() + 18.0),
            egui::pos2(plot_rect.right(), outer.bottom() - 4.0),
        );
        let bound = res
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(0.0_f64, |m, v| m.max(v.abs()))
            .max(f64::MIN_POSITIVE);
        // Same X as the curve; Y linear, not reversed, centered on zero.
        let sub = transform_for(
            strip,
            frame.0,
            (-bound * 1.1, bound * 1.1),
            (scales.0, AxisScale::Linear),
        );
        painter.rect_stroke(
            strip,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        if let Some(sub) = sub {
            let clip = painter.with_clip_rect(strip);
            if let Some(zero) = to_screen(&sub, frame.0 .0, 0.0) {
                clip.hline(strip.x_range(), zero.y, egui::Stroke::new(0.5, text_color));
            }
            let mut run: Vec<egui::Pos2> = Vec::new();
            for (x, r) in rows_x.iter().zip(res) {
                let point = (x.is_finite() && r.is_finite())
                    .then(|| to_screen(&sub, *x, *r))
                    .flatten();
                match point {
                    Some(point) => run.push(point),
                    None if run.len() > 1 => {
                        clip.add(egui::Shape::line(std::mem::take(&mut run), result));
                    }
                    None => run.clear(),
                }
            }
            if run.len() > 1 {
                clip.add(egui::Shape::line(run, result));
            }
        }
        painter.text(
            egui::pos2(strip.left() + 3.0, strip.top() + 1.0),
            egui::Align2::LEFT_TOP,
            tr!("analysis-preview-residuals"),
            font.clone(),
            text_color,
        );
    }

    // What prevents computing: said at the center of the preview.
    if let Some(issue) = &outcome.issue {
        plot.text(
            plot_rect.center_top() + egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_TOP,
            issue_message(issue),
            egui::FontId::proportional(13.0),
            theme::preview_result_color(dark),
        );
    }

    // Gestures.
    let (shift, command) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
    let hover = response.hover_pos().filter(|p| plot_rect.contains(*p));
    let hover_data = hover.and_then(|pos| to_data(&transform, pos));
    if let Some((hx, hy)) = hover_data {
        let mut label = format!("x = {hx:.6}\ny = {hy:.6}");
        if shift && form.tool == PreviewTool::Points {
            if let Some(snap) = snap_to_curve_zone(&xs, &ys, hx, form.average) {
                let (sx, sy) = (snap.x, snap.y);
                if let Some(p) = to_screen(&transform, sx, sy) {
                    // The circle covers the averaged zone (#334); an edge with no
                    // image (log scale, negative value): minimal radius.
                    let edge = |x: f64| to_screen(&transform, x, sy).map_or(f32::NAN, |q| q.x);
                    let max = plot_rect.size().length();
                    let radius = snap_radius(p.x, edge(snap.x_min), edge(snap.x_max), max);
                    plot.circle_stroke(p, radius, egui::Stroke::new(1.5, marker));
                }
                label = format!(
                    "x = {sx:.6}\ny = {sy:.6}\n{}",
                    tr!(
                        "analysis-preview-average",
                        count = i64::try_from(snap.count).unwrap_or(i64::MAX)
                    )
                );
            }
        }
        painter.text(
            plot_rect.right_top() + egui::vec2(-4.0, 4.0),
            egui::Align2::RIGHT_TOP,
            label,
            font.clone(),
            text_color,
        );
        // Shift + wheel sets the average, one point per notch. We count the
        // raw events, one per notch: `smooth_scroll_delta` spreads a notch
        // over several frames and added dozens of points. Under Shift,
        // egui puts the scroll on the horizontal axis, but the raw event
        // keeps the direction of the device: so we add both axes.
        if shift {
            let notches = wheel_notches(ui);
            if notches != 0 {
                let next = (form.average as i64 + i64::from(notches))
                    .clamp(*AVERAGE_RANGE.start() as i64, *AVERAGE_RANGE.end() as i64);
                form.average = next as usize;
            }
        }
    }

    // Ctrl + wheel (or pinch): zoom anchored under the pointer, same factor
    // as the graph; over an axis, that scale alone. The wheel alone does not
    // zoom (#62).
    let mut transform = transform;
    if let Some(pos) = response.hover_pos() {
        let zoom = ui.input(|i| i.zoom_delta());
        if zoom != 1.0 {
            if let Some(target) = nav_target(pos, plot_rect, outer) {
                let gesture = Gesture::Zoom {
                    anchor: (f64::from(pos.x), f64::from(pos.y)),
                    factor: (1.0 / f64::from(zoom)).clamp(0.5, 2.0),
                };
                frame = navigate(&transform, plot_rect, frame, target, gesture);
                transform = transform_for(plot_rect, frame.0, frame.1, scales).unwrap_or(transform);
            }
        }
    }

    // The origin of the drag is kept in memory (#251): egui clears
    // `press_origin` on the frame of the release, the one where the zone is placed.
    let origin_id = id.with("drag_origin");
    let origin = match ui.input(|i| i.pointer.press_origin()) {
        Some(pos) => {
            ui.data_mut(|d| d.insert_temp(origin_id, pos));
            Some(pos)
        }
        None => ui.data_mut(|d| d.remove_temp::<egui::Pos2>(origin_id)),
    };
    // What the drag in progress does, decided on its first frame: Ctrl
    // held then → move the view, otherwise draw (Zone tool); a user who
    // presses or releases Ctrl along the way does not change gesture midway.
    let mode_id = id.with("drag_mode");
    let primary = egui::PointerButton::Primary;
    let mut mode = ui.data(|d| d.get_temp::<DragMode>(mode_id));
    if response.dragged_by(primary) && mode.is_none() {
        let start = origin.or_else(|| {
            response
                .interact_pointer_pos()
                .map(|pos| pos - response.drag_delta())
        });
        let decided = if command {
            start
                .and_then(|pos| nav_target(pos, plot_rect, outer))
                .map_or(DragMode::Ignore, DragMode::Pan)
        } else {
            DragMode::Draw
        };
        ui.data_mut(|d| d.insert_temp(mode_id, decided));
        mode = Some(decided);
    }
    if let (Some(DragMode::Pan(target)), true) = (mode, response.dragged_by(primary)) {
        let delta = response.drag_delta();
        let gesture = Gesture::Pan {
            dx: f64::from(delta.x),
            dy: f64::from(delta.y),
        };
        frame = navigate(&transform, plot_rect, frame, target, gesture);
        transform = transform_for(plot_rect, frame.0, frame.1, scales).unwrap_or(transform);
    }
    if mode == Some(DragMode::Draw) && form.tool == PreviewTool::Zone {
        if let (Some(from), Some(to)) = (origin, response.interact_pointer_pos()) {
            if let (Some(a), Some(b)) = (to_data(&transform, from), to_data(&transform, to)) {
                if response.dragged_by(primary) {
                    band(&plot, a.0, b.0);
                } else if response.drag_stopped_by(primary) {
                    form.add_zone(a.0, b.0);
                }
            }
        }
    }
    if !response.dragged_by(primary) {
        ui.data_mut(|d| d.remove_temp::<DragMode>(mode_id));
    }
    // Ctrl + click: nothing (reserved for navigation).
    if response.clicked_by(primary) && form.tool == PreviewTool::Points && !command {
        if let Some((cx, cy)) = hover_data {
            let (px, py) = if shift {
                snap_to_curve(&xs, &ys, cx, form.average).unwrap_or((cx, cy))
            } else {
                (cx, cy)
            };
            form.add_point(px, py);
        }
    }
    if response.secondary_clicked() {
        if let Some((cx, _)) = hover_data {
            // The X extent is negative when the axis is reversed: the
            // tolerance is a distance.
            let tolerance = (frame.0 .1 - frame.0 .0).abs() * 8.0 / f64::from(plot_rect.width());
            form.remove_near(cx, tolerance);
        }
    }
    if response.double_clicked() {
        ui.data_mut(|d| d.remove::<StoredView>(view_id));
    } else {
        ui.data_mut(|d| {
            d.insert_temp(
                view_id,
                StoredView {
                    key,
                    source: form.graph_source.clone(),
                    frame,
                },
            )
        });
    }
    outcome
}

/// The zones, ranges and windows of the form to tint.
fn zones_of(form: &AnalysisForm) -> Vec<(f64, f64)> {
    use stokesforge_analysis::ops::OperationKind as K;
    let s = &form.setup;
    match form.kind {
        K::Baseline
            if s.baseline.shape == stokesforge_app_state::analysis::BaselineShape::ConstantZone =>
        {
            s.baseline.constant_zone.into_iter().collect()
        }
        K::Baseline
            if s.baseline.shape == stokesforge_app_state::analysis::BaselineShape::Zones =>
        {
            s.baseline.zones.clone()
        }
        K::Integrate => s.integrate.ranges.clone(),
        K::Fit => s.fit.range.into_iter().collect(),
        K::Peaks => s.peaks.range.into_iter().collect(),
        K::Reconvolution => s.reconv.spec.window.into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The points placed (line, anchor points).
fn marks_of(form: &AnalysisForm) -> Vec<(f64, f64)> {
    use stokesforge_analysis::ops::OperationKind as K;
    use stokesforge_app_state::analysis::BaselineShape;
    match form.kind {
        K::Baseline if form.setup.baseline.shape == BaselineShape::Constant => {
            form.setup.baseline.constant_point.into_iter().collect()
        }
        K::Baseline
            if !matches!(
                form.setup.baseline.shape,
                BaselineShape::Zones | BaselineShape::ConstantZone
            ) =>
        {
            form.setup.baseline.points.clone()
        }
        _ => Vec::new(),
    }
}

/// The starting positions of the peaks.
fn seeds_of(form: &AnalysisForm) -> Vec<f64> {
    use stokesforge_analysis::ops::OperationKind as K;
    match form.kind {
        K::Peaks => form.setup.peaks.seeds.iter().map(|s| s.center).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use stokesforge_app_state::analysis::Preset;

    fn form_and_tree() -> (AnalysisForm, Arc<Tree>) {
        let project = stokesforge_core::Project::new("Preview").unwrap();
        let form = AnalysisForm::new(
            &Preset::Fit(stokesforge_analysis::model::Model::Linear),
            Vec::new(),
            false,
        );
        (form, project.tree().clone())
    }

    static ORDER: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    static COMPUTED: AtomicUsize = AtomicUsize::new(0);

    fn slow_and_recorded(request: &Request) -> Result<Computed, Issue> {
        std::thread::sleep(Duration::from_millis(150));
        ORDER.lock().unwrap().push(request.revision);
        COMPUTED.fetch_add(1, Ordering::SeqCst);
        Ok(Computed::default())
    }

    /// Requesting does not block the frame; during a calculation, the
    /// intermediate requests are replaced by the most recent one, and the
    /// result of a stale request is never taken for the right one (#201).
    #[test]
    fn the_preview_calculation_does_not_block_and_keeps_the_most_recent_request() {
        let (form, tree) = form_and_tree();
        let context = egui::Context::default();
        let jobs = Arc::new(PreviewJobs::new(slow_and_recorded));

        let started = Instant::now();
        jobs.request(&context, &form, &tree, 1);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "requesting does not compute: {:?}",
            started.elapsed()
        );
        assert!(
            jobs.wait_for(&form, 1, Duration::from_millis(10)).is_none(),
            "the frame does not wait for a long calculation"
        );
        // Two requests during the calculation: only the last one will be computed.
        jobs.request(&context, &form, &tree, 2);
        jobs.request(&context, &form, &tree, 3);
        let done = jobs
            .wait_for(&form, 3, Duration::from_secs(10))
            .expect("the most recent request is computed");
        assert_eq!(done.revision, 3);
        assert_eq!(*ORDER.lock().unwrap(), vec![1, 3], "request 2 is dropped");
        assert_eq!(COMPUTED.load(Ordering::SeqCst), 2);
        // Witness: the result of request 2 does not exist, it is not confused
        // with that of 3.
        assert!(!jobs.latest().unwrap().matches(&form, 2));
        // Requesting the same thing again restarts nothing.
        jobs.request(&context, &form, &tree, 3);
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(COMPUTED.load(Ordering::SeqCst), 2);
    }

    /// The preview is recomputed after a pause of 0.3 s without change (S-T1,
    /// #224): the first calculation is immediate, a change restarts the
    /// countdown, the elapsed pause allows it.
    #[test]
    fn the_preview_waits_for_a_typing_pause_before_recalculating() {
        let (mut form, _) = form_and_tree();
        let context = egui::Context::default();
        let id = egui::Id::new("preview");
        let at = |time: f64, form: &AnalysisForm| {
            let mut settled_now = false;
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| settled_now = settled(ui, id, form));
            output.textures_delta.clear();
            settled_now
        };
        assert!(at(1.0, &form), "the first calculation does not wait");
        assert!(at(1.05, &form), "nothing changed");
        form.current += 1;
        assert!(!at(2.0, &form), "a change restarts the countdown");
        assert!(
            !at(2.0 + PREVIEW_PAUSE - 0.05, &form),
            "the pause has not elapsed"
        );
        assert!(at(2.0 + PREVIEW_PAUSE + 0.01, &form), "pause elapsed");
        form.current += 1;
        assert!(!at(3.0, &form), "each keystroke pushes back the deadline");
        form.current += 1;
        assert!(
            at(10.0, &form),
            "a reopened window computes at once, with no typing to wait for"
        );
    }
}

/// #251: a Zone-tool drag places the zone on release, even though
/// egui has already cleared `press_origin` on that frame.
#[cfg(test)]
mod zone_tests {
    use super::super::test_support::{context, image, xy_columns};
    use super::*;
    use stokesforge_app_state::analysis::{BaselineShape, Preset};
    use stokesforge_core::SeriesRef;

    /// Drags on the preview from left to right, at mid height.
    fn drag(shape: Option<BaselineShape>, preset: Preset) -> AnalysisForm {
        let mut project = stokesforge_core::Project::new("Zone").unwrap();
        let (x, y) = xy_columns(&mut project);
        let tree = project.tree().clone();
        let series = vec![SeriesRef {
            x: Some(x),
            y,
            sigma: None,
        }];
        let mut form = AnalysisForm::new(&preset, series, false);
        if let Some(shape) = shape {
            form.setup.baseline.shape = shape;
            form.tool = form.natural_tool();
        }
        let context = context();
        let id = egui::Id::new("zone");
        let ev = |pos: egui::Pos2, pressed: Option<bool>| {
            let mut v = vec![egui::Event::PointerMoved(pos)];
            if let Some(pressed) = pressed {
                v.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            v
        };
        let (a, b) = (egui::pos2(500.0, 400.0), egui::pos2(900.0, 400.0));
        for events in [
            vec![],
            ev(a, None),
            ev(a, Some(true)),
            ev(egui::pos2(700.0, 400.0), None),
            ev(b, None),
            ev(b, Some(false)),
            vec![],
        ] {
            image(&context, events, |ui| {
                preview(ui, id, &mut form, &tree, 0);
            });
        }
        form
    }

    #[test]
    fn dragging_draws_the_zone_of_the_average_constant() {
        let form = drag(Some(BaselineShape::ConstantZone), Preset::BaselineConstant);
        assert_eq!(form.tool, PreviewTool::Zone);
        assert!(
            form.setup.baseline.constant_zone.is_some(),
            "{:?}",
            form.setup.baseline
        );
    }

    #[test]
    fn dragging_draws_a_polynomial_zone() {
        let form = drag(None, Preset::BaselinePolynomial);
        assert_eq!(form.setup.baseline.zones.len(), 1);
    }

    /// Places a point with a left click then removes it with a right click at
    /// the same pixel; returns the point after each of the two clicks. With `inverted`,
    /// the X axis of the source graph is reversed: the X extent is then negative
    /// and the tolerance of `remove_near` must stay a distance.
    fn place_then_remove(inverted: bool) -> [Option<(f64, f64)>; 2] {
        let mut project = stokesforge_core::Project::new("Right click").unwrap();
        let (x, y) = xy_columns(&mut project);
        let tree = project.tree().clone();
        let series = vec![SeriesRef {
            x: Some(x),
            y,
            sigma: None,
        }];
        let mut form = AnalysisForm::new(&Preset::BaselineConstant, series, false);
        let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
        panel.bottom.inverted = inverted;
        panel
            .series
            .push(stokesforge_plot::SeriesSpec::new_xy(x, y));
        let spec = stokesforge_plot::PlotSpec::empty("Source").with_panels(panel);
        form.graph_source = Some((spec, vec![stokesforge_plot::SeriesRef::new(0, 0)]));
        assert_eq!(form.tool, PreviewTool::Points);
        let context = context();
        let id = egui::Id::new("right_click");
        let pos = egui::pos2(700.0, 400.0);
        let button = |button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let primary = egui::PointerButton::Primary;
        let secondary = egui::PointerButton::Secondary;
        let mut after_left = None;
        let steps = [
            vec![],
            vec![egui::Event::PointerMoved(pos)],
            vec![button(primary, true)],
            vec![button(primary, false)],
            vec![],
            vec![button(secondary, true)],
            vec![button(secondary, false)],
            vec![],
        ];
        for (rank, events) in steps.into_iter().enumerate() {
            image(&context, events, |ui| {
                preview(ui, id, &mut form, &tree, 0);
            });
            if rank == 4 {
                after_left = form.setup.baseline.constant_point;
            }
        }
        [after_left, form.setup.baseline.constant_point]
    }

    /// The right click removes the targeted marker, whatever the direction of the X axis:
    /// the tolerance `(x1 - x0) · 8 / width` would be negative on a reversed
    /// axis without `abs`.
    #[test]
    fn right_click_removes_the_placed_point_even_on_a_reversed_x_axis() {
        for inverted in [false, true] {
            let [posed, removed] = place_then_remove(inverted);
            assert!(
                posed.is_some(),
                "the left click places the point ({inverted})"
            );
            assert_eq!(
                removed, None,
                "the right click removes it (reversed: {inverted})"
            );
        }
    }

    /// A preview driven frame by frame: modifier keys, mouse, wheel.
    struct Bench {
        context: egui::Context,
        id: egui::Id,
        tree: Arc<Tree>,
        form: AnalysisForm,
        modifiers: egui::Modifiers,
        pointer: egui::Pos2,
    }

    /// Ctrl as winit sends it under Linux: `command` follows `ctrl`.
    const CTRL: egui::Modifiers = egui::Modifiers {
        alt: false,
        ctrl: true,
        shift: false,
        mac_cmd: false,
        command: true,
    };
    /// In the plot, toward the middle.
    const MIDDLE: egui::Pos2 = egui::pos2(700.0, 400.0);
    /// On the X tick band (below the plot).
    const ON_X: egui::Pos2 = egui::pos2(700.0, 890.0);
    /// On the Y tick band (left of the plot).
    const ON_Y: egui::Pos2 = egui::pos2(20.0, 400.0);

    impl Bench {
        fn new(preset: Preset, inverted: bool) -> Self {
            Self::on_series(preset, inverted, None)
        }

        /// Like `new`, on a series of `points` points (x = 0, 1, 2…).
        fn on_series(preset: Preset, inverted: bool, points: Option<usize>) -> Self {
            let mut project = stokesforge_core::Project::new("Navigation").unwrap();
            let (x, y) = match points {
                None => xy_columns(&mut project),
                Some(n) => {
                    let root = project.root();
                    let book = project.add_empty_workbook(root, None).unwrap();
                    let sheet = project.add_worksheet(book, None).unwrap();
                    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
                    let ys: Vec<f64> = xs.iter().map(|x| (x / 7.0).sin()).collect();
                    let x = project
                        .add_column(sheet, None, None, stokesforge_core::Column::from_f64(&xs))
                        .unwrap();
                    let y = project
                        .add_column(sheet, None, None, stokesforge_core::Column::from_f64(&ys))
                        .unwrap();
                    (x, y)
                }
            };
            let series = vec![SeriesRef {
                x: Some(x),
                y,
                sigma: None,
            }];
            let mut form = AnalysisForm::new(&preset, series, false);
            if inverted {
                let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
                panel.bottom.inverted = true;
                panel
                    .series
                    .push(stokesforge_plot::SeriesSpec::new_xy(x, y));
                let spec = stokesforge_plot::PlotSpec::empty("Source").with_panels(panel);
                form.graph_source = Some((spec, vec![stokesforge_plot::SeriesRef::new(0, 0)]));
            }
            let mut bench = Self {
                context: context(),
                id: egui::Id::new("navigation"),
                tree: project.tree().clone(),
                form,
                modifiers: egui::Modifiers::NONE,
                pointer: MIDDLE,
            };
            bench.image(vec![]);
            bench.image(vec![]);
            bench
        }

        /// A frame where the mouse is at `pointer` and the modifiers those of the bench.
        fn image(&mut self, mut events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
            let mut all = vec![
                egui::Event::ModifiersChanged(self.modifiers),
                egui::Event::PointerMoved(self.pointer),
            ];
            all.append(&mut events);
            let (id, form, tree) = (self.id, &mut self.form, &self.tree);
            image(&self.context, all, |ui| {
                preview(ui, id, form, tree, 0);
            })
        }

        fn move_pointer(&mut self, pointer: egui::Pos2) {
            self.pointer = pointer;
            self.image(vec![]);
        }

        fn set_button(&mut self, pressed: bool) {
            let event = egui::Event::PointerButton {
                pos: self.pointer,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: self.modifiers,
            };
            self.image(vec![event]);
        }

        /// Presses at `from`, drags to `to`, releases.
        fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
            self.move_pointer(from);
            self.set_button(true);
            self.move_pointer(from + (to - from) * 0.5);
            self.move_pointer(to);
            self.set_button(false);
            self.image(vec![]);
        }

        fn frame(&self) -> Frame {
            self.context
                .data(|d| d.get_temp::<StoredView>(self.id.with("view")))
                .expect("remembered frame")
                .frame
        }

        fn wheel(&mut self, delta: egui::Vec2) {
            let event = egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                phase: egui::TouchPhase::Move,
                modifiers: self.modifiers,
            };
            self.image(vec![event]);
        }

        fn with_modifiers(&mut self, modifiers: egui::Modifiers) -> &mut Self {
            self.modifiers = modifiers;
            self
        }
    }

    fn width_of((a, b): (f64, f64)) -> f64 {
        (b - a).abs()
    }

    /// The wheel alone does nothing in the preview any more (D3 of #299).
    #[test]
    fn the_wheel_alone_does_not_change_the_frame() {
        let mut bench = Bench::new(Preset::Integrate, false);
        let before = bench.frame();
        bench.wheel(egui::vec2(0.0, 120.0));
        bench.wheel(egui::vec2(0.0, -120.0));
        assert_eq!(bench.frame(), before);
    }

    /// Ctrl + wheel (or pinch) tightens the frame around the pointer: the
    /// data under the cursor stays there, and the frame tightens on both axes.
    #[test]
    fn ctrl_wheel_tightens_the_frame_around_the_pointer() {
        for gesture in ["pinch", "ctrl_wheel"] {
            let mut bench = Bench::new(Preset::Integrate, false);
            let before = bench.frame();
            let pointeur = egui::pos2(400.0, 300.0);
            bench.move_pointer(pointeur);
            // The data under the pointer, before.
            let plot_rect = plot_area();
            let under_the_pointer = |frame: Frame| {
                let t = transform_for(
                    plot_rect,
                    frame.0,
                    frame.1,
                    (AxisScale::Linear, AxisScale::Linear),
                )
                .unwrap();
                to_data(&t, pointeur).unwrap()
            };
            let data_point = under_the_pointer(before);
            if gesture == "pinch" {
                bench.image(vec![egui::Event::Zoom(1.25)]);
            } else {
                bench.with_modifiers(CTRL).wheel(egui::vec2(0.0, 60.0));
            }
            let after = bench.frame();
            assert!(
                width_of(after.0) < width_of(before.0),
                "{gesture}: X tightened"
            );
            assert!(
                width_of(after.1) < width_of(before.1),
                "{gesture}: Y tightened"
            );
            let again = under_the_pointer(after);
            assert!(
                (again.0 - data_point.0).abs() < 1e-9 && (again.1 - data_point.1).abs() < 1e-9,
                "{gesture}: the data under the pointer stays under the pointer"
            );
        }
    }

    /// The plot rectangle as `preview` computes it for a frame of
    /// 1400 × 900 without residuals.
    fn plot_area() -> egui::Rect {
        let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut r = outer.shrink2(egui::vec2(8.0, 4.0));
        r.min.x += 44.0;
        r.max.y -= 18.0;
        r
    }

    /// Over an axis, Ctrl + wheel frames only that scale.
    #[test]
    fn ctrl_wheel_on_an_axis_zooms_only_that_scale() {
        let mut bench = Bench::new(Preset::Integrate, false);
        let before = bench.frame();
        bench.move_pointer(ON_X);
        bench.image(vec![egui::Event::Zoom(1.25)]);
        let after_x = bench.frame();
        assert!(width_of(after_x.0) < width_of(before.0), "X tightened");
        assert_eq!(after_x.1, before.1, "Y does not move");
        bench.move_pointer(ON_Y);
        bench.image(vec![egui::Event::Zoom(1.25)]);
        let after_y = bench.frame();
        assert_eq!(after_y.0, after_x.0, "X no longer moves");
        assert!(width_of(after_y.1) < width_of(before.1), "Y tightened");
    }

    /// Shift + wheel sets the magnet's average, by 1 per notch, without touching
    /// the frame.
    #[test]
    fn shift_wheel_sets_the_average_without_zooming() {
        let mut bench = Bench::new(Preset::BaselineAnchors, false);
        let (average, frame) = (bench.form.average, bench.frame());
        bench
            .with_modifiers(egui::Modifiers::SHIFT)
            .wheel(egui::vec2(0.0, 40.0));
        assert_eq!(bench.form.average, average + 1);
        bench.wheel(egui::vec2(0.0, -40.0));
        assert_eq!(bench.form.average, average);
        assert_eq!(bench.frame(), frame, "the frame does not move");
    }

    /// One notch = one point, even if egui spreads the scroll: the following
    /// frames, with no wheel event, change nothing any more.
    #[test]
    fn one_wheel_notch_is_worth_one_point_even_across_several_frames() {
        let mut bench = Bench::new(Preset::BaselineAnchors, false);
        let average = bench.form.average;
        bench
            .with_modifiers(egui::Modifiers::SHIFT)
            .wheel(egui::vec2(0.0, 50.0));
        for _ in 0..5 {
            bench.image(vec![]);
        }
        assert_eq!(bench.form.average, average + 1);
        bench.wheel(egui::vec2(50.0, 0.0));
        assert_eq!(
            bench.form.average,
            average + 2,
            "horizontal axis under Shift"
        );
    }

    /// The circles of the magnet color painted in a frame.
    fn snap_circles(shapes: &[egui::epaint::ClippedShape]) -> Vec<f32> {
        let marker = theme::preview_marker_color();
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Circle(c) if c.stroke.color == marker => Some(c.radius),
                _ => None,
            })
            .collect()
    }

    /// The painted text, put end to end.
    fn texts(shapes: &[egui::epaint::ClippedShape]) -> String {
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The snap circle grows with N (#334): 6 px for one point,
    /// strictly more for 21 points of a series of 101.
    #[test]
    fn the_snap_circle_grows_with_the_average() {
        let mut bench = Bench::on_series(Preset::BaselineAnchors, false, Some(101));
        bench.with_modifiers(egui::Modifiers::SHIFT);
        bench.form.average = 1;
        let single = snap_circles(&bench.image(vec![]));
        bench.form.average = 21;
        let wide = snap_circles(&bench.image(vec![]));
        assert_eq!(single.len(), 1, "one snap circle");
        assert_eq!(wide.len(), 1, "one snap circle");
        assert_eq!(single[0], SNAP_RADIUS_MIN);
        assert!(wide[0] > single[0], "{} > {}", wide[0], single[0]);
    }

    /// At the edge, the text gives the real number of points averaged, not N.
    #[test]
    fn the_average_text_gives_the_real_number_of_points_at_the_edge() {
        let mut bench = Bench::on_series(Preset::BaselineAnchors, false, Some(101));
        bench.with_modifiers(egui::Modifiers::SHIFT);
        bench.pointer = egui::pos2(plot_area().left() + 1.0, 400.0);
        bench.form.average = 21;
        let text = texts(&bench.image(vec![]));
        assert!(text.contains("11 points"), "{text}");
        assert!(!text.contains("21 points"), "{text}");
    }

    /// The circle radius: largest half-side, clamped, never NaN.
    #[test]
    fn the_circle_radius_is_the_largest_clamped_half_side() {
        let max = 1000.0;
        // One point: the minimum.
        assert_eq!(snap_radius(300.0, 300.0, 300.0, max), SNAP_RADIUS_MIN);
        // Centered zone of 200 px: 100 px.
        assert_eq!(snap_radius(300.0, 200.0, 400.0, max), 100.0);
        // Asymmetric: 20 on the left, 60 on the right, and the reverse.
        assert_eq!(snap_radius(300.0, 280.0, 360.0, max), 60.0);
        assert_eq!(snap_radius(300.0, 240.0, 320.0, max), 60.0);
        // Reversed X axis: edges swapped, same radius.
        assert_eq!(snap_radius(300.0, 400.0, 200.0, max), 100.0);
        // Cap.
        assert_eq!(snap_radius(300.0, -5000.0, 400.0, max), max);
        // Not finite: the minimum, even if only one pixel is.
        assert_eq!(snap_radius(300.0, f32::NAN, 500.0, max), SNAP_RADIUS_MIN);
        assert_eq!(
            snap_radius(300.0, 100.0, f32::INFINITY, max),
            SNAP_RADIUS_MIN
        );
        assert_eq!(snap_radius(f32::NAN, 100.0, 500.0, max), SNAP_RADIUS_MIN);
    }

    /// Ctrl + drag moves the view: the frame translates (width kept,
    /// Y intact) and no zone is placed, despite the Integrate Zone tool.
    /// On a reversed X axis, the data still follow the cursor: the bounds
    /// go the other way.
    #[test]
    fn ctrl_drag_translates_the_frame_without_placing_a_zone() {
        for inverted in [false, true] {
            let mut bench = Bench::new(Preset::Integrate, inverted);
            assert_eq!(bench.form.tool, PreviewTool::Zone);
            let before = bench.frame();
            bench.with_modifiers(CTRL);
            bench.drag(egui::pos2(500.0, 400.0), egui::pos2(700.0, 400.0));
            let after = bench.frame();
            assert!(
                bench.form.setup.integrate.ranges.is_empty(),
                "Ctrl + drag draws no zone ({inverted})"
            );
            assert!(
                (width_of(after.0) - width_of(before.0)).abs() < 1e-9,
                "{inverted}"
            );
            assert_eq!(after.1, before.1, "no vertical movement ({inverted})");
            let offset = after.0 .0 - before.0 .0;
            assert!(offset != 0.0, "the frame moved ({inverted})");
            // The content follows the cursor to the right: the left edge sees
            // data further left in display direction, so smaller
            // without reversal and larger with.
            assert_eq!(
                offset < 0.0,
                !inverted,
                "direction of the movement ({inverted})"
            );
            // The data under the start of the drag is under its arrival.
            let plot_rect = plot_area();
            let t = |frame: Frame| {
                transform_for(
                    plot_rect,
                    frame.0,
                    frame.1,
                    (AxisScale::Linear, AxisScale::Linear),
                )
                .unwrap()
            };
            let (from, to) = (egui::pos2(500.0, 400.0), egui::pos2(700.0, 400.0));
            let before_x = to_data(&t(before), from).unwrap().0;
            let after_x = to_data(&t(after), to).unwrap().0;
            // The reversed axis is drawn by the source graph, not by this linear
            // `t`: so we compare the gap relative to the width of the frame.
            if !inverted {
                assert!(
                    (before_x - after_x).abs() < 1e-9,
                    "the content follows the cursor"
                );
            }
        }
    }

    /// Ctrl + drag starting on an axis moves only that axis.
    #[test]
    fn ctrl_drag_on_an_axis_moves_only_that_axis() {
        let mut bench = Bench::new(Preset::Integrate, false);
        let before = bench.frame();
        bench.with_modifiers(CTRL);
        bench.drag(ON_X, ON_X + egui::vec2(150.0, 60.0));
        let after = bench.frame();
        assert_ne!(after.0, before.0, "X moved");
        assert_eq!(after.1, before.1, "Y intact");
        let mut bench = Bench::new(Preset::Integrate, false);
        bench.with_modifiers(CTRL);
        bench.drag(ON_Y, ON_Y + egui::vec2(10.0, 80.0));
        assert_eq!(bench.frame().0, before.0, "X intact");
        assert_ne!(bench.frame().1, before.1, "Y moved");
    }

    /// Double click reframes: the original frame, after a zoom as after a
    /// movement.
    #[test]
    fn double_click_restores_the_original_frame() {
        let mut bench = Bench::new(Preset::Integrate, false);
        let origine = bench.frame();
        bench.image(vec![egui::Event::Zoom(1.5)]);
        bench.with_modifiers(CTRL);
        bench.drag(egui::pos2(500.0, 400.0), egui::pos2(600.0, 450.0));
        bench.with_modifiers(egui::Modifiers::NONE);
        assert_ne!(bench.frame(), origine);
        bench.move_pointer(MIDDLE);
        for pressed in [true, false, true, false] {
            bench.set_button(pressed);
        }
        bench.image(vec![]);
        assert_eq!(bench.frame(), origine);
    }

    /// A plain Ctrl + click places nothing; the click alone places the point.
    #[test]
    fn ctrl_click_does_not_place_a_point() {
        let mut bench = Bench::new(Preset::BaselineConstant, false);
        assert_eq!(bench.form.tool, PreviewTool::Points);
        bench.with_modifiers(CTRL);
        for pressed in [true, false] {
            bench.set_button(pressed);
        }
        bench.image(vec![]);
        assert_eq!(bench.form.setup.baseline.constant_point, None);
        bench.with_modifiers(egui::Modifiers::NONE);
        bench.move_pointer(MIDDLE + egui::vec2(30.0, 0.0));
        for pressed in [true, false] {
            bench.set_button(pressed);
        }
        bench.image(vec![]);
        assert!(bench.form.setup.baseline.constant_point.is_some());
    }

    /// Peak detection: the drag (Zone tool) gives the search range,
    /// the click (Points tool) the seed of a peak; a drag with the Points tool
    /// places nothing.
    #[test]
    fn peaks_are_placed_by_click_and_by_drag() {
        let mut bench = Bench::new(Preset::Peaks, false);
        assert_eq!(bench.form.tool, PreviewTool::Points);
        bench.drag(egui::pos2(500.0, 400.0), egui::pos2(900.0, 400.0));
        assert_eq!(bench.form.setup.peaks.range, None, "Points tool: nothing");
        assert!(bench.form.setup.peaks.seeds.is_empty());
        bench.form.tool = PreviewTool::Zone;
        bench.drag(egui::pos2(500.0, 400.0), egui::pos2(900.0, 400.0));
        assert!(bench.form.setup.peaks.range.is_some(), "Zone tool: range");
        bench.form.tool = PreviewTool::Points;
        bench.move_pointer(MIDDLE);
        for pressed in [true, false] {
            bench.set_button(pressed);
        }
        bench.image(vec![]);
        assert_eq!(bench.form.setup.peaks.seeds.len(), 1, "Points tool: seed");
    }
}
