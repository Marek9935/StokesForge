//! L'aperçu de la fenêtre d'analyse (#181, maquettes M3 et M4) : la série
//! montrée, le résultat du calcul en direct (ligne de base, ajustement, pics,
//! résidus) et les repères posés (points, zones, positions de pics).
//!
//! L'aperçu se peint lui-même — il ne passe pas par `GraphView` : il n'a ni
//! calque, ni légende, ni historique, et ses clics **écrivent dans le
//! formulaire** (`AnalysisForm::add_point`, `add_zone`, `remove_near`), pas
//! dans le modèle. Sa géométrie (`View`, `snap`) est pure et testée.
//!
//! # Gestes
//!
//! - outil *Points* : un clic pose un point ; **Maj** l'aimante sur la courbe
//!   avec une moyenne locale de N points, **Maj + molette** règle N (M4 b) ;
//! - outil *Zone* : glisser trace une zone ou une plage ;
//! - outil *Zoom* : glisser cadre un rectangle ; molette : zoom autour du
//!   curseur ; double clic : cadrage d'origine ;
//! - clic droit : retire le repère visé.
//!
//! # Le calcul n'est pas fait dans `ui()` (#201)
//!
//! L'aperçu se calcule sur **un fil de travail** (un seul à la fois, nommé
//! `stokesforge-analysis-preview`) : un ajustement multi-pics sur des dizaines de
//! milliers de points ne gèle plus la fenêtre. Chaque image compare le
//! formulaire et la révision du modèle à ceux du dernier résultat ; s'ils
//! diffèrent, la demande est posée **en remplaçant** celle qui attendait (le
//! résultat obsolète est jeté, un calcul en cours n'est pas interrompu mais son
//! résultat est ignoré s'il ne correspond plus). L'image attend le fil
//! [`INLINE_WAIT`] au plus : un petit calcul rend son résultat dans la même
//! image, sans clignotement ; un gros laisse l'ancien résultat (marqué « en
//! cours ») et le fil réveille le viewport **une fois** à sa fin — aucun réveil
//! périodique au repos.
//!
//! Une image où rien ne change ne refait rien.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_core::{Computed, Tree};

use crate::analysis::{
    issue_message, series_points, series_rows, AnalysisForm, Issue, PreviewTool, AVERAGE_RANGE,
};
use crate::theme;
use crate::tr;

/// Le cadre de données visible : `[x0, x1] × [y0, y1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct View {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    x_log: bool,
    y_log: bool,
    x_inverted: bool,
    y_inverted: bool,
}

impl View {
    /// Le cadre qui contient les valeurs finies de `xs` et `ys`, avec 5 % de
    /// marge ; un cadre d'étendue nulle est élargi.
    pub(super) fn around(xs: &[f64], ys: &[f64]) -> View {
        fn bounds(values: &[f64]) -> (f64, f64) {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for v in values.iter().copied().filter(|v| v.is_finite()) {
                lo = lo.min(v);
                hi = hi.max(v);
            }
            if lo > hi {
                (0.0, 1.0)
            } else if lo == hi {
                (lo - 0.5, hi + 0.5)
            } else {
                let margin = (hi - lo) * 0.05;
                (lo - margin, hi + margin)
            }
        }
        let (x0, x1) = bounds(xs);
        let (y0, y1) = bounds(ys);
        View {
            x0,
            x1,
            y0,
            y1,
            x_log: false,
            y_log: false,
            x_inverted: false,
            y_inverted: false,
        }
    }

    fn fraction(lo: f64, hi: f64, value: f64, log: bool, inverted: bool) -> f64 {
        let transform = |v: f64| if log { v.log10() } else { v };
        let fraction = (transform(value) - transform(lo)) / (transform(hi) - transform(lo));
        if inverted {
            1.0 - fraction
        } else {
            fraction
        }
    }

    fn value(lo: f64, hi: f64, fraction: f64, log: bool, inverted: bool) -> f64 {
        let fraction = if inverted { 1.0 - fraction } else { fraction };
        if log {
            10.0_f64.powf(lo.log10() + fraction * (hi.log10() - lo.log10()))
        } else {
            lo + fraction * (hi - lo)
        }
    }

    pub(super) fn to_screen(self, rect: egui::Rect, x: f64, y: f64) -> egui::Pos2 {
        let u = Self::fraction(self.x0, self.x1, x, self.x_log, self.x_inverted);
        let v = Self::fraction(self.y0, self.y1, y, self.y_log, self.y_inverted);
        egui::pos2(
            rect.left() + u as f32 * rect.width(),
            rect.bottom() - v as f32 * rect.height(),
        )
    }

    pub(super) fn to_data(self, rect: egui::Rect, p: egui::Pos2) -> (f64, f64) {
        let u = f64::from((p.x - rect.left()) / rect.width());
        let v = f64::from((rect.bottom() - p.y) / rect.height());
        (
            Self::value(self.x0, self.x1, u, self.x_log, self.x_inverted),
            Self::value(self.y0, self.y1, v, self.y_log, self.y_inverted),
        )
    }

    /// Le cadre agrandi (`factor` > 1) ou rétréci autour de `anchor`, qui
    /// reste au même endroit de l'écran.
    pub(super) fn zoomed(self, anchor: (f64, f64), factor: f64) -> View {
        let scale = |lo: f64, hi: f64, at: f64, log: bool| {
            if log {
                let (lo, hi, at) = (lo.log10(), hi.log10(), at.log10());
                (
                    10.0_f64.powf(at + (lo - at) * factor),
                    10.0_f64.powf(at + (hi - at) * factor),
                )
            } else {
                (at + (lo - at) * factor, at + (hi - at) * factor)
            }
        };
        let (x0, x1) = scale(self.x0, self.x1, anchor.0, self.x_log);
        let (y0, y1) = scale(self.y0, self.y1, anchor.1, self.y_log);
        View {
            x0,
            x1,
            y0,
            y1,
            ..self
        }
    }

    /// Le cadre dont deux coins sont `a` et `b` (coordonnées de données).
    pub(super) fn between(self, a: (f64, f64), b: (f64, f64)) -> View {
        View {
            x0: a.0.min(b.0),
            x1: a.0.max(b.0),
            y0: a.1.min(b.1),
            y1: a.1.max(b.1),
            ..self
        }
    }
}

/// Le point de la courbe le plus proche de `x`, avec Y moyenné sur `n` points
/// centrés sur lui (aimantation de Maj, M4 b). `None` pour une courbe vide.
pub(super) fn snap(xs: &[f64], ys: &[f64], x: f64, n: usize) -> Option<(f64, f64)> {
    let nearest = xs
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))?
        .0;
    let half = n.max(1) / 2;
    let lo = nearest.saturating_sub(half);
    let hi = (nearest + half + 1).min(ys.len());
    let window = &ys[lo..hi];
    Some((
        xs[nearest],
        window.iter().sum::<f64>() / window.len() as f64,
    ))
}

/// Le pas « rond » d'une graduation pour couvrir `span` en environ 5 pas.
pub(super) fn tick_step(span: f64) -> f64 {
    let raw = span / 5.0;
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    let fraction = raw / magnitude;
    let nice = if fraction < 1.5 {
        1.0
    } else if fraction < 3.5 {
        2.0
    } else if fraction < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

fn format_tick(value: f64, step: f64) -> String {
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    stokesforge_core::number_format::format_tick(value, decimals)
}

/// Ce que l'aperçu rend à la fenêtre : les valeurs de la série montrée, ou ce
/// qui empêche de calculer.
#[derive(Debug, Clone, Default)]
pub(super) struct PreviewOutcome {
    pub values: Vec<stokesforge_core::ResultValue>,
    pub warnings: Vec<stokesforge_analysis::ops::Warning>,
    pub skipped: usize,
    pub issue: Option<Issue>,
    /// Le calcul est en cours : les valeurs montrées sont celles d'avant.
    pub pending: bool,
}

/// Combien de temps une image attend le fil de calcul avant d'afficher
/// l'ancien résultat. Les tests attendent le résultat pour être déterministes.
#[cfg(not(test))]
const INLINE_WAIT: Duration = Duration::from_millis(40);
#[cfg(test)]
const INLINE_WAIT: Duration = Duration::from_secs(30);

/// Un résultat d'aperçu et ce pour quoi il a été calculé.
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

/// Ce que le fil de calcul doit calculer.
struct Request {
    form: AnalysisForm,
    revision: u64,
    tree: Arc<Tree>,
}

#[derive(Default)]
struct JobState {
    /// Un fil de calcul tourne.
    running: bool,
    /// La demande la plus récente, posée pendant qu'un calcul tournait.
    next: Option<Request>,
    /// Le dernier résultat terminé.
    finished: Option<Arc<PreviewCache>>,
    /// Ce qui a été demandé le plus récemment : on ne redemande pas la même
    /// chose à chaque image.
    requested: Option<(AnalysisForm, u64)>,
}

/// Le calcul d'une demande ; une fonction pour que les tests y mettent un calcul
/// lent et comptable.
type Compute = fn(&Request) -> Result<Computed, Issue>;

fn compute_preview(request: &Request) -> Result<Computed, Issue> {
    request.form.preview(&request.tree)
}

/// Le fil de calcul d'une fenêtre d'analyse, gardé dans la mémoire d'egui.
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

    /// Demande le calcul de `form` à `revision`, sauf s'il a déjà été demandé.
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
            // Aucun fil disponible : le calcul se fait sur l'appelant, comme
            // avant #201, plutôt que de ne jamais rendre de résultat.
            self.work(context, fallback);
        }
    }

    /// Le corps du fil : calcule la demande, publie, enchaîne sur la plus
    /// récente s'il y en a une, puis s'arrête.
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

    /// Le dernier résultat terminé, quel qu'il soit.
    fn latest(&self) -> Option<Arc<PreviewCache>> {
        self.lock().finished.clone()
    }

    /// Attend au plus `timeout` le résultat de `form` à `revision`.
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
    view: View,
}

/// Recadre l'aperçu sur ses données (bouton « Reset View »).
pub(super) fn reset_view(ui: &mut egui::Ui, id: egui::Id) {
    ui.data_mut(|d| d.remove::<StoredView>(id.with("view")));
}

/// La pause de frappe après laquelle un aperçu se recalcule (T1, #224), en
/// secondes. Texte **et** nombres : chaque frappe d'un champ ne relance pas le
/// calcul.
pub(super) const PREVIEW_PAUSE: f64 = 0.3;

/// Au-delà de ce temps sans image peinte, la fenêtre a été fermée puis
/// rouverte : le formulaire n'est pas en cours de frappe.
const REOPEN_GAP: f64 = 1.0;

/// Le formulaire est-il resté inchangé depuis [`PREVIEW_PAUSE`] ? Le formulaire
/// de l'image précédente, l'instant de son dernier changement et celui de la
/// dernière image sont gardés en mémoire d'egui ; tant qu'il bouge, une image
/// est demandée à la fin de la pause (aucun réveil périodique au repos). À
/// l'ouverture d'une fenêtre, l'aperçu se calcule tout de suite : il n'y a pas
/// de frappe à attendre.
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

/// Peint l'aperçu dans l'espace restant de `ui` et applique ses gestes au
/// formulaire.
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
        // Un aperçu se recalcule après une pause de frappe (T1, #224) : tant
        // que le formulaire bouge, on garde l'ancien résultat, marqué en cours.
        (None, Some(latest)) if !settled(ui, id, form) => latest,
        (None, _) => {
            jobs.request(ui.ctx(), form, tree, revision);
            jobs.wait_for(form, revision, INLINE_WAIT)
                .or_else(|| jobs.latest())
                .unwrap_or_else(|| {
                    // Rien n'a encore été calculé (fil indisponible ou premier
                    // calcul long) : un résultat vide, marqué en cours.
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

    // Le cadre : mémorisé par série, recadré au double clic.
    let view_id = id.with("view");
    let key = (form.current, series.y.raw());
    let mut view = ui
        .data_mut(|d| d.get_temp::<StoredView>(view_id))
        .filter(|s| s.key == key && s.source == form.graph_source)
        .map(|s| s.view)
        .unwrap_or_else(|| {
            let mut all_y = ys.clone();
            for k in ["corrected", "deconvolved"] {
                if let Some(c) = column(k, None) {
                    all_y.extend(c.iter().copied());
                }
            }
            let mut frame = View::around(&xs, &all_y);
            if let Some((spec, sources)) = &form.graph_source {
                if let Some(source) = sources.get(form.current) {
                    if let Some(panel) = spec.panels.get(source.panel) {
                        let Some(curve) = panel.series.get(source.index) else {
                            return frame;
                        };
                        let y = panel.axis(curve.axis.side());
                        frame.x_log = panel.bottom.scale == stokesforge_plot::AxisScale::Log;
                        frame.y_log = y.scale == stokesforge_plot::AxisScale::Log;
                        frame.x_inverted = panel.bottom.inverted;
                        frame.y_inverted = y.inverted;
                        let positive_bounds = |values: &[f64]| {
                            let values: Vec<_> = values
                                .iter()
                                .copied()
                                .filter(|v| v.is_finite() && *v > 0.0)
                                .map(f64::log10)
                                .collect();
                            let bounds = View::around(&values, &values);
                            (10.0_f64.powf(bounds.x0), 10.0_f64.powf(bounds.x1))
                        };
                        if frame.x_log {
                            (frame.x0, frame.x1) = positive_bounds(&xs);
                        }
                        if frame.y_log {
                            (frame.y0, frame.y1) = positive_bounds(&all_y);
                        }
                        if let (Some(lo), Some(hi)) =
                            (panel.bottom.range_min, panel.bottom.range_max)
                        {
                            if !panel.bottom.auto_range {
                                frame.x0 = lo;
                                frame.x1 = hi;
                            }
                        }
                        if let (Some(lo), Some(hi)) = (y.range_min, y.range_max) {
                            if !y.auto_range {
                                frame.y0 = lo;
                                frame.y1 = hi;
                            }
                        }
                    }
                }
            }
            frame
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
    let text_color = ui.visuals().text_color();
    let font = egui::FontId::proportional(11.0);

    // Graduations.
    let step_x = tick_step(view.x1 - view.x0);
    let step_y = tick_step(view.y1 - view.y0);
    let mut x = (view.x0 / step_x).ceil() * step_x;
    while x <= view.x1 {
        let p = view.to_screen(plot_rect, x, view.y0);
        painter.text(
            egui::pos2(p.x, plot_rect.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            format_tick(x, step_x),
            font.clone(),
            text_color,
        );
        x += step_x;
    }
    let mut y = (view.y0 / step_y).ceil() * step_y;
    while y <= view.y1 {
        let p = view.to_screen(plot_rect, view.x0, y);
        painter.text(
            egui::pos2(plot_rect.left() - 3.0, p.y),
            egui::Align2::RIGHT_CENTER,
            format_tick(y, step_y),
            font.clone(),
            text_color,
        );
        y += step_y;
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
            if x.is_finite() && y.is_finite() {
                let point = view.to_screen(plot_rect, *x, *y);
                if point.x.is_finite() && point.y.is_finite() {
                    run.push(point);
                } else {
                    flush(&mut run);
                }
            } else {
                flush(&mut run);
            }
        }
        flush(&mut run);
    };

    // Zones et plages.
    let frame = view;
    let band = move |plot: &egui::Painter, a: f64, b: f64| {
        let l = frame.to_screen(plot_rect, a, frame.y0).x;
        let r = frame.to_screen(plot_rect, b, frame.y0).x;
        plot.rect_filled(
            egui::Rect::from_x_y_ranges(l..=r, plot_rect.y_range()),
            0.0,
            theme::preview_zone_fill(),
        );
    };
    for (a, b) in zones_of(form) {
        band(&plot, a, b);
    }

    // Données, puis résultats.
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
                    let p = view.to_screen(plot_rect, *x, *y) - plot_rect.min;
                    (f64::from(p.x), f64::from(p.y))
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
    for k in ["fit", "corrected", "peaks_sum", "deconvolved"] {
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

    // Repères posés.
    let marker = theme::preview_marker_color();
    for (px, py) in marks_of(form) {
        let p = view.to_screen(plot_rect, px, py);
        plot.circle(p, 4.0, marker, egui::Stroke::new(1.0, text_color));
    }
    for cx in seeds_of(form) {
        let top = view.to_screen(plot_rect, cx, view.y1);
        let bottom = view.to_screen(plot_rect, cx, view.y0);
        plot.line_segment([top, bottom], egui::Stroke::new(1.0, marker));
    }

    // Résidus sous la courbe.
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
        let sub = View {
            x0: view.x0,
            x1: view.x1,
            y0: -bound * 1.1,
            y1: bound * 1.1,
            y_log: false,
            y_inverted: false,
            ..view
        };
        painter.rect_stroke(
            strip,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let clip = painter.with_clip_rect(strip);
        let zero = sub.to_screen(strip, view.x0, 0.0).y;
        clip.hline(strip.x_range(), zero, egui::Stroke::new(0.5, text_color));
        let mut run: Vec<egui::Pos2> = Vec::new();
        for (x, r) in rows_x.iter().zip(res) {
            if x.is_finite() && r.is_finite() {
                run.push(sub.to_screen(strip, *x, *r));
            } else if run.len() > 1 {
                clip.add(egui::Shape::line(std::mem::take(&mut run), result));
            } else {
                run.clear();
            }
        }
        if run.len() > 1 {
            clip.add(egui::Shape::line(run, result));
        }
        painter.text(
            egui::pos2(strip.left() + 3.0, strip.top() + 1.0),
            egui::Align2::LEFT_TOP,
            tr!("analysis-preview-residuals"),
            font.clone(),
            text_color,
        );
    }

    // Ce qui empêche de calculer : dit au centre de l'aperçu.
    if let Some(issue) = &outcome.issue {
        plot.text(
            plot_rect.center_top() + egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_TOP,
            issue_message(issue),
            egui::FontId::proportional(13.0),
            theme::preview_result_color(dark),
        );
    }

    // Gestes.
    let shift = ui.input(|i| i.modifiers.shift);
    let hover = response.hover_pos().filter(|p| plot_rect.contains(*p));
    if let Some(pos) = hover {
        let (hx, hy) = view.to_data(plot_rect, pos);
        let mut label = format!("x = {hx:.6}\ny = {hy:.6}");
        if shift && form.tool == PreviewTool::Points {
            if let Some((sx, sy)) = snap(&xs, &ys, hx, form.average) {
                let p = view.to_screen(plot_rect, sx, sy);
                plot.circle_stroke(p, 6.0, egui::Stroke::new(1.5, marker));
                label = format!(
                    "x = {sx:.6}\ny = {sy:.6}\n{}",
                    tr!(
                        "analysis-preview-average",
                        count = i64::try_from(form.average).unwrap_or(i64::MAX)
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
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if shift {
                let step = if scroll > 0.0 { 2 } else { -2 };
                let next = (form.average as i64 + step)
                    .clamp(*AVERAGE_RANGE.start() as i64, *AVERAGE_RANGE.end() as i64);
                form.average = next as usize;
            } else {
                let factor = f64::from(-scroll * 0.004).exp();
                view = view.zoomed((hx, hy), factor);
            }
        }
    }

    // L'origine du glisser est gardée en mémoire (#251) : egui efface
    // `press_origin` à l'image du relâchement, celle où la zone se pose.
    let origin_id = id.with("drag_origin");
    let origin = match ui.input(|i| i.pointer.press_origin()) {
        Some(pos) => {
            ui.data_mut(|d| d.insert_temp(origin_id, pos));
            Some(pos)
        }
        None => ui.data_mut(|d| d.remove_temp::<egui::Pos2>(origin_id)),
    };
    if response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(from), Some(to)) = (origin, response.interact_pointer_pos()) {
            match form.tool {
                PreviewTool::Zone => {
                    band(
                        &plot,
                        view.to_data(plot_rect, from).0,
                        view.to_data(plot_rect, to).0,
                    );
                }
                PreviewTool::Zoom => {
                    plot.rect_stroke(
                        egui::Rect::from_two_pos(from, to),
                        0.0,
                        egui::Stroke::new(1.0, marker),
                        egui::StrokeKind::Inside,
                    );
                }
                PreviewTool::Points => {}
            }
        }
    }
    if response.drag_stopped_by(egui::PointerButton::Primary) {
        if let (Some(from), Some(to)) = (origin, response.interact_pointer_pos()) {
            let a = view.to_data(plot_rect, from);
            let b = view.to_data(plot_rect, to);
            match form.tool {
                PreviewTool::Zone => form.add_zone(a.0, b.0),
                PreviewTool::Zoom if (from - to).abs().max_elem() > 4.0 => {
                    view = view.between(a, b);
                }
                _ => {}
            }
        }
    }
    if response.clicked_by(egui::PointerButton::Primary) && form.tool == PreviewTool::Points {
        if let Some(pos) = hover {
            let (cx, cy) = view.to_data(plot_rect, pos);
            let (px, py) = if shift {
                snap(&xs, &ys, cx, form.average).unwrap_or((cx, cy))
            } else {
                (cx, cy)
            };
            form.add_point(px, py);
        }
    }
    if response.secondary_clicked() {
        if let Some(pos) = hover {
            let (cx, _) = view.to_data(plot_rect, pos);
            let tolerance = (view.x1 - view.x0) * 8.0 / f64::from(plot_rect.width());
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
                    view,
                },
            )
        });
    }
    outcome
}

/// Les zones, plages et fenêtres du formulaire à teinter.
fn zones_of(form: &AnalysisForm) -> Vec<(f64, f64)> {
    use stokesforge_analysis::ops::OperationKind as K;
    let s = &form.setup;
    match form.kind {
        K::Baseline if s.baseline.shape == crate::analysis::BaselineShape::ConstantZone => {
            s.baseline.constant_zone.into_iter().collect()
        }
        K::Baseline if s.baseline.shape == crate::analysis::BaselineShape::Zones => {
            s.baseline.zones.clone()
        }
        K::Integrate => s.integrate.ranges.clone(),
        K::Fit => s.fit.range.into_iter().collect(),
        K::Peaks => s.peaks.range.into_iter().collect(),
        K::Reconvolution => s.reconv.spec.window.into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Les points posés (droite, points d'ancrage).
fn marks_of(form: &AnalysisForm) -> Vec<(f64, f64)> {
    use crate::analysis::BaselineShape;
    use stokesforge_analysis::ops::OperationKind as K;
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

/// Les positions de départ des pics.
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
    use crate::analysis::Preset;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn form_and_tree() -> (AnalysisForm, Arc<Tree>) {
        let project = stokesforge_core::Project::new("Aperçu").unwrap();
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

    /// Demander ne bloque pas l'image ; pendant un calcul, les demandes
    /// intermédiaires sont remplacées par la plus récente, et le résultat d'une
    /// demande périmée n'est jamais pris pour le bon (#201).
    #[test]
    fn le_calcul_de_l_apercu_ne_bloque_pas_et_garde_la_demande_la_plus_recente() {
        let (form, tree) = form_and_tree();
        let context = egui::Context::default();
        let jobs = Arc::new(PreviewJobs::new(slow_and_recorded));

        let started = Instant::now();
        jobs.request(&context, &form, &tree, 1);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "demander ne calcule pas : {:?}",
            started.elapsed()
        );
        assert!(
            jobs.wait_for(&form, 1, Duration::from_millis(10)).is_none(),
            "l'image n'attend pas un calcul long"
        );
        // Deux demandes pendant le calcul : seule la dernière sera calculée.
        jobs.request(&context, &form, &tree, 2);
        jobs.request(&context, &form, &tree, 3);
        let done = jobs
            .wait_for(&form, 3, Duration::from_secs(10))
            .expect("la demande la plus récente est calculée");
        assert_eq!(done.revision, 3);
        assert_eq!(*ORDER.lock().unwrap(), vec![1, 3], "la demande 2 est jetée");
        assert_eq!(COMPUTED.load(Ordering::SeqCst), 2);
        // Témoin : le résultat de la demande 2 n'existe pas, il ne se confond pas
        // avec celui de la 3.
        assert!(!jobs.latest().unwrap().matches(&form, 2));
        // Redemander la même chose ne relance rien.
        jobs.request(&context, &form, &tree, 3);
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(COMPUTED.load(Ordering::SeqCst), 2);
    }

    /// L'aperçu se recalcule après une pause de 0,3 s sans changement (T1,
    /// #224) : le premier calcul est immédiat, un changement relance le
    /// compte à rebours, la pause écoulée l'autorise.
    #[test]
    fn l_apercu_attend_une_pause_de_frappe_avant_de_se_recalculer() {
        let (mut form, _) = form_and_tree();
        let context = egui::Context::default();
        let id = egui::Id::new("apercu");
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
        assert!(at(1.0, &form), "le premier calcul n'attend pas");
        assert!(at(1.05, &form), "rien n'a changé");
        form.current += 1;
        assert!(!at(2.0, &form), "un changement relance le compte à rebours");
        assert!(
            !at(2.0 + PREVIEW_PAUSE - 0.05, &form),
            "la pause n'est pas écoulée"
        );
        assert!(at(2.0 + PREVIEW_PAUSE + 0.01, &form), "pause écoulée");
        form.current += 1;
        assert!(!at(3.0, &form), "chaque frappe repousse l'échéance");
        form.current += 1;
        assert!(
            at(10.0, &form),
            "une fenêtre rouverte se calcule tout de suite, sans frappe à attendre"
        );
    }

    #[test]
    fn le_cadre_entoure_les_valeurs_finies_avec_une_marge() {
        let view = View::around(&[0.0, 10.0, f64::NAN], &[1.0, 3.0]);
        assert_eq!((view.x0, view.x1), (-0.5, 10.5));
        assert!((view.y0 - 0.9).abs() < 1e-12 && (view.y1 - 3.1).abs() < 1e-12);
        let flat = View::around(&[2.0, 2.0], &[]);
        assert_eq!((flat.x0, flat.x1), (1.5, 2.5));
        assert_eq!(
            (flat.y0, flat.y1),
            (0.0, 1.0),
            "aucune donnée : cadre unité"
        );
    }

    #[test]
    fn echelle_log_inversee_et_zoom_gardent_la_correspondance_des_gestes() {
        let view = View {
            x0: 1.0,
            x1: 100.0,
            y0: 10.0,
            y1: 1000.0,
            x_log: true,
            y_log: true,
            x_inverted: true,
            y_inverted: false,
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0));
        let center = view.to_screen(rect, 10.0, 100.0);
        assert_eq!(center, rect.center());
        let point = view.to_data(rect, center);
        assert!((point.0 - 10.0).abs() < 1e-6);
        assert!((point.1 - 100.0).abs() < 1e-6);
        assert_eq!(view.to_screen(rect, 100.0, 10.0), rect.left_bottom());
        assert_eq!(view.to_screen(rect, 1.0, 1000.0), rect.right_top());
        let zoom = view.zoomed((10.0, 100.0), 0.5);
        assert_eq!(zoom.to_screen(rect, 10.0, 100.0), center);
        assert!(zoom.x_log && zoom.y_log && zoom.x_inverted);
    }

    #[test]
    fn ecran_et_donnees_sont_des_fonctions_inverses() {
        let view = View {
            x0: 0.0,
            x1: 10.0,
            y0: -1.0,
            y1: 1.0,
            ..View::around(&[], &[])
        };
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 100.0));
        let p = view.to_screen(rect, 2.5, 0.5);
        let (x, y) = view.to_data(rect, p);
        assert!((x - 2.5).abs() < 1e-4 && (y - 0.5).abs() < 1e-4);
        assert_eq!(view.to_screen(rect, 0.0, -1.0), rect.left_bottom());
        assert_eq!(view.to_screen(rect, 10.0, 1.0), rect.right_top());
    }

    #[test]
    fn le_zoom_garde_le_point_ancre_fixe() {
        let view = View {
            x0: 0.0,
            x1: 10.0,
            y0: 0.0,
            y1: 10.0,
            ..View::around(&[], &[])
        };
        let zoomed = view.zoomed((2.0, 4.0), 0.5);
        assert_eq!((zoomed.x0, zoomed.x1), (1.0, 6.0));
        assert_eq!((zoomed.y0, zoomed.y1), (2.0, 7.0));
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let before = view.to_screen(rect, 2.0, 4.0);
        let after = zoomed.to_screen(rect, 2.0, 4.0);
        assert!((before - after).length() < 1e-3, "l'ancre reste à l'écran");
        // L'ancre garde sa position **relative** : (2-0)/10 == (2-1)/5.
        assert!(
            ((2.0 - view.x0) / (view.x1 - view.x0) - (2.0 - zoomed.x0) / (zoomed.x1 - zoomed.x0))
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn l_aimantation_prend_le_point_le_plus_proche_et_moyenne_autour() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let ys = [10.0, 20.0, 60.0, 20.0, 10.0];
        assert_eq!(snap(&xs, &ys, 2.2, 1), Some((2.0, 60.0)));
        let (x, y) = snap(&xs, &ys, 2.2, 3).unwrap();
        assert_eq!(x, 2.0);
        assert!((y - 100.0 / 3.0).abs() < 1e-12);
        // Au bord, la fenêtre se tronque au lieu de déborder.
        assert_eq!(snap(&xs, &ys, -5.0, 3), Some((0.0, 15.0)));
        assert_eq!(snap(&[], &[], 1.0, 3), None);
    }

    #[test]
    fn les_graduations_sont_rondes() {
        assert_eq!(tick_step(10.0), 2.0);
        assert_eq!(tick_step(1.0), 0.2);
        assert_eq!(tick_step(0.0), 1.0);
        assert_eq!(format_tick(0.4, 0.2), "0.4");
        assert_eq!(format_tick(6.0, 2.0), "6");
    }
}

/// #251 : un glisser de l'outil Zone pose la zone au relâchement, alors
/// qu'egui a déjà effacé `press_origin` à cette image.
#[cfg(test)]
mod zone_tests {
    use super::super::test_support::{colonnes_xy, contexte, image};
    use super::*;
    use crate::analysis::{BaselineShape, Preset};
    use stokesforge_core::SeriesRef;

    /// Glisse sur l'aperçu de gauche à droite, au milieu de la hauteur.
    fn glisser(shape: Option<BaselineShape>, preset: Preset) -> AnalysisForm {
        let mut project = stokesforge_core::Project::new("Zone").unwrap();
        let (x, y) = colonnes_xy(&mut project);
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
        let context = contexte();
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
    fn glisser_trace_la_zone_de_la_constante_par_moyenne() {
        let form = glisser(Some(BaselineShape::ConstantZone), Preset::BaselineConstant);
        assert_eq!(form.tool, PreviewTool::Zone);
        assert!(
            form.setup.baseline.constant_zone.is_some(),
            "{:?}",
            form.setup.baseline
        );
    }

    #[test]
    fn glisser_trace_une_zone_du_polynome() {
        let form = glisser(None, Preset::BaselinePolynomial);
        assert_eq!(form.setup.baseline.zones.len(), 1);
    }
}
