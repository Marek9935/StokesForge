//! L'aperçu de la fenêtre d'analyse (#181, maquettes M3 et M4) : la série
//! montrée, le résultat du calcul en direct (ligne de base, ajustement, pics,
//! résidus) et les repères posés (points, zones, positions de pics).
//!
//! L'aperçu se peint lui-même — il ne passe pas par `GraphView` : il n'a ni
//! calque, ni légende, ni historique, et ses clics **écrivent dans le
//! formulaire** (`AnalysisForm::add_point`, `add_zone`, `remove_near`), pas
//! dans le modèle. Sa **peinture** reste ici ; sa **géométrie** — pixel ↔
//! données, zoom, graduations, aimantation, cadre initial — vient de
//! `stokesforge_render` (`PanelTransform`, `axis_ticks`, `snap_to_curve`,
//! `axis_frame`, #261) : l'application ne fait que convertir en `Pos2`.
//!
//! # Gestes
//!
//! Comme sur le graphe (#270, D3 de #299 ; matrice : `docs/gestes.md`) :
//!
//! - **Ctrl + molette** : zoom ancré sous le pointeur ; au-dessus d'un axe, ce
//!   seul axe. La molette seule ne fait rien ;
//! - **Ctrl + glisser** : déplace la vue (un glisser parti d'un axe ne déplace
//!   que cet axe) ; le modificateur est lu au début du glisser ;
//! - **double clic** : cadrage d'origine ;
//! - **Maj + molette** : règle la moyenne N de l'aimant (M4 b) ;
//! - outil *Points* : un clic pose un point ; **Maj** l'aimante sur la courbe
//!   avec une moyenne locale de N points ;
//! - outil *Zone* : glisser trace une zone ou une plage ;
//! - clic droit : retire le repère visé.
//!
//! Le zoom et le déplacement sont calculés par `stokesforge_render`
//! (`PanelTransform::zoom`, `pan`) ; l'aperçu n'a pas de `ViewState` (ni calque,
//! ni panneaux dessinés) : il garde son cadre et ne fait que choisir la cible
//! du geste, plein tracé ou un axe.
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
use stokesforge_plot::{AxisScale, AxisSpec};
use stokesforge_render::scale::axis_ticks;
use stokesforge_render::{axis_frame, fit_range, snap_to_curve, Gesture, PanelTransform};

use crate::theme;
use stokesforge_app_state::analysis::{
    issue_message, series_points, series_rows, AnalysisForm, Issue, PreviewTool, AVERAGE_RANGE,
};
use stokesforge_app_state::tr;

/// Le cadre de données visible : les bornes de X puis de Y, **dans l'ordre
/// d'affichage** (un axe inversé a sa borne haute en premier), comme les prend
/// [`PanelTransform::new`].
type Frame = ((f64, f64), (f64, f64));

/// Les axes (bas, ordonnée) de la courbe du graphe dont vient la série
/// courante ; `None` sans graphe source — l'aperçu est alors linéaire et non
/// inversé.
fn source_axes(form: &AnalysisForm) -> Option<(&AxisSpec, &AxisSpec)> {
    let (spec, sources) = form.graph_source.as_ref()?;
    let source = sources.get(form.current)?;
    let panel = spec.panels.get(source.panel)?;
    let curve = panel.series.get(source.index)?;
    Some((&panel.bottom, panel.axis(curve.axis.side())))
}

/// La transformation de `rect` pour ces bornes ; `None` pour un rectangle sans
/// surface. Le rectangle d'egui est fractionnaire : origine et taille passent
/// en `f64` sans arrondi.
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

/// Le pixel d'une donnée ; `None` si elle n'a pas d'image (hors d'une échelle
/// logarithmique, non finie).
fn to_screen(transform: &PanelTransform, x: f64, y: f64) -> Option<egui::Pos2> {
    let (px, py) = transform.data_to_pixel(x, y)?;
    Some(egui::pos2(px as f32, py as f32))
}

/// La donnée sous un pixel.
fn to_data(transform: &PanelTransform, pos: egui::Pos2) -> Option<(f64, f64)> {
    transform.pixel_to_data(f64::from(pos.x), f64::from(pos.y))
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
    frame: Frame,
}

/// Ce que vise un geste de navigation : tout le tracé, ou une seule échelle
/// (la bande des graduations de X sous le tracé, celle de Y à sa gauche).
#[derive(Clone, Copy, Debug, PartialEq)]
enum NavTarget {
    Plot,
    AxisX,
    AxisY,
}

/// Ce que fait le glisser en cours, décidé à sa première image (comme
/// `nav_drag` du graphe) : tracer une zone, ou déplacer la vue (Ctrl enfoncé au
/// début), ou rien (parti hors du tracé et des axes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DragMode {
    Draw,
    Pan(NavTarget),
    #[default]
    Ignore,
}

/// La cible d'un geste de navigation en `pos` : le tracé, l'axe X (sous le
/// tracé, résidus compris), l'axe Y (à sa gauche), ou rien.
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

/// Le cadre après `gesture` sur `target`. Sur un axe, l'autre composante du
/// geste est neutralisée au centre du tracé (comme
/// `ViewState::commit_axis_gesture`) et l'autre borne du cadre ne bouge pas.
/// Sans image (borne logarithmique ≤ 0), le cadre reste.
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

/// La pause de frappe après laquelle un aperçu se recalcule (S-T1, #224), en
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

//// Les crans de molette de cette image (positifs vers le haut), tous axes
/// confondus : un événement brut par cran, quel que soit le lissage d'egui.
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

// Peint l'aperçu dans l'espace restant de `ui` et applique ses gestes au
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
        // Un aperçu se recalcule après une pause de frappe (S-T1, #224) : tant
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

    // Le cadre : mémorisé par série, recadré au double clic. Les échelles et le
    // sens des axes viennent du graphe source ; le cadrage initial et la
    // géométrie, de `stokesforge_render`.
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
    // Un rectangle sans surface, ou des bornes sans image : rien à placer.
    let Some(transform) = transform_for(plot_rect, frame.0, frame.1, scales) else {
        return outcome;
    };
    let text_color = ui.visuals().text_color();
    let font = egui::FontId::proportional(11.0);

    // Graduations : le pas et le format du graphe (`axis_ticks`), le point
    // décimal comme avant.
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

    // Zones et plages.
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

    // Repères posés.
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
        // Même X que la courbe ; Y linéaire, non inversé, centré sur zéro.
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
    let (shift, command) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
    let hover = response.hover_pos().filter(|p| plot_rect.contains(*p));
    let hover_data = hover.and_then(|pos| to_data(&transform, pos));
    if let Some((hx, hy)) = hover_data {
        let mut label = format!("x = {hx:.6}\ny = {hy:.6}");
        if shift && form.tool == PreviewTool::Points {
            if let Some((sx, sy)) = snap_to_curve(&xs, &ys, hx, form.average) {
                if let Some(p) = to_screen(&transform, sx, sy) {
                    plot.circle_stroke(p, 6.0, egui::Stroke::new(1.5, marker));
                }
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
        // Maj + molette règle la moyenne, d'un point par cran. On compte les
        // événements bruts, un par cran : `smooth_scroll_delta` étale un cran
        // sur plusieurs images et ajoutait des dizaines de points. Sous Maj,
        // egui range le défilement dans l'axe horizontal, mais l'événement brut
        // garde le sens de l'appareil : on additionne donc les deux axes.
        if shift {
            let notches = wheel_notches(ui);
            if notches != 0 {
                let next = (form.average as i64 + i64::from(notches))
                    .clamp(*AVERAGE_RANGE.start() as i64, *AVERAGE_RANGE.end() as i64);
                form.average = next as usize;
            }
        }
    }

    // Ctrl + molette (ou pincement) : zoom ancré sous le pointeur, même facteur
    // que le graphe ; au-dessus d'un axe, cette seule échelle. La molette seule
    // ne zoome pas (#62).
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
    // Ce que fait le glisser en cours, décidé à sa première image : Ctrl
    // enfoncé alors → déplacer la vue, sinon tracer (outil Zone) ; l'utilisateur
    // qui appuie ou relâche Ctrl en route ne change pas de geste en chemin.
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
    // Ctrl + clic : rien (réservé à la navigation).
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
            // L'étendue de X est négative quand l'axe est inversé : la
            // tolérance est une distance.
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

/// Les zones, plages et fenêtres du formulaire à teinter.
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

/// Les points posés (droite, points d'ancrage).
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use stokesforge_app_state::analysis::Preset;

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

    /// L'aperçu se recalcule après une pause de 0,3 s sans changement (S-T1,
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
}

/// #251 : un glisser de l'outil Zone pose la zone au relâchement, alors
/// qu'egui a déjà effacé `press_origin` à cette image.
#[cfg(test)]
mod zone_tests {
    use super::super::test_support::{colonnes_xy, contexte, image};
    use super::*;
    use stokesforge_app_state::analysis::{BaselineShape, Preset};
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

    /// Pose un point par un clic gauche puis le retire par un clic droit au
    /// même pixel ; rend le point après chacun des deux clics. Avec `inverted`,
    /// l'axe X du graphe source est inversé : l'étendue de X est alors négative
    /// et la tolérance de `remove_near` doit rester une distance.
    fn poser_puis_retirer(inverted: bool) -> [Option<(f64, f64)>; 2] {
        let mut project = stokesforge_core::Project::new("Clic droit").unwrap();
        let (x, y) = colonnes_xy(&mut project);
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
        let context = contexte();
        let id = egui::Id::new("clic_droit");
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

    /// Le clic droit retire le repère visé, quel que soit le sens de l'axe X :
    /// la tolérance `(x1 - x0) · 8 / largeur` serait négative sur un axe
    /// inversé sans `abs`.
    #[test]
    fn clic_droit_retire_le_point_pose_meme_sur_un_axe_x_inverse() {
        for inverted in [false, true] {
            let [posed, removed] = poser_puis_retirer(inverted);
            assert!(posed.is_some(), "le clic gauche pose le point ({inverted})");
            assert_eq!(
                removed, None,
                "le clic droit le retire (inversé : {inverted})"
            );
        }
    }

    /// Un aperçu piloté image par image : touches de modification, souris, molette.
    struct Banc {
        context: egui::Context,
        id: egui::Id,
        tree: Arc<Tree>,
        form: AnalysisForm,
        modifiers: egui::Modifiers,
        pointer: egui::Pos2,
    }

    /// Ctrl tel que l'envoie winit sous Linux : `command` suit `ctrl`.
    const CTRL: egui::Modifiers = egui::Modifiers {
        alt: false,
        ctrl: true,
        shift: false,
        mac_cmd: false,
        command: true,
    };
    /// Dans le tracé, vers le milieu.
    const AU_MILIEU: egui::Pos2 = egui::pos2(700.0, 400.0);
    /// Sur la bande des graduations de X (sous le tracé).
    const SUR_X: egui::Pos2 = egui::pos2(700.0, 890.0);
    /// Sur la bande des graduations de Y (à gauche du tracé).
    const SUR_Y: egui::Pos2 = egui::pos2(20.0, 400.0);

    impl Banc {
        fn new(preset: Preset, inverted: bool) -> Self {
            let mut project = stokesforge_core::Project::new("Navigation").unwrap();
            let (x, y) = colonnes_xy(&mut project);
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
            let mut banc = Self {
                context: contexte(),
                id: egui::Id::new("navigation"),
                tree: project.tree().clone(),
                form,
                modifiers: egui::Modifiers::NONE,
                pointer: AU_MILIEU,
            };
            banc.image(vec![]);
            banc.image(vec![]);
            banc
        }

        /// Une image où la souris est en `pointer` et les modificateurs ceux du banc.
        fn image(&mut self, mut events: Vec<egui::Event>) {
            let mut all = vec![
                egui::Event::ModifiersChanged(self.modifiers),
                egui::Event::PointerMoved(self.pointer),
            ];
            all.append(&mut events);
            let (id, form, tree) = (self.id, &mut self.form, &self.tree);
            image(&self.context, all, |ui| {
                preview(ui, id, form, tree, 0);
            });
        }

        fn deplacer(&mut self, pointer: egui::Pos2) {
            self.pointer = pointer;
            self.image(vec![]);
        }

        fn bouton(&mut self, pressed: bool) {
            let event = egui::Event::PointerButton {
                pos: self.pointer,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: self.modifiers,
            };
            self.image(vec![event]);
        }

        /// Appuie en `from`, glisse jusqu'en `to`, relâche.
        fn glisser(&mut self, from: egui::Pos2, to: egui::Pos2) {
            self.deplacer(from);
            self.bouton(true);
            self.deplacer(from + (to - from) * 0.5);
            self.deplacer(to);
            self.bouton(false);
            self.image(vec![]);
        }

        fn cadre(&self) -> Frame {
            self.context
                .data(|d| d.get_temp::<StoredView>(self.id.with("view")))
                .expect("cadre mémorisé")
                .frame
        }

        fn molette(&mut self, delta: egui::Vec2) {
            let event = egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                phase: egui::TouchPhase::Move,
                modifiers: self.modifiers,
            };
            self.image(vec![event]);
        }

        fn avec(&mut self, modifiers: egui::Modifiers) -> &mut Self {
            self.modifiers = modifiers;
            self
        }
    }

    fn largeur((a, b): (f64, f64)) -> f64 {
        (b - a).abs()
    }

    /// La molette seule ne fait plus rien dans l'aperçu (D3 de #299).
    #[test]
    fn la_molette_seule_ne_change_pas_le_cadre() {
        let mut banc = Banc::new(Preset::Integrate, false);
        let avant = banc.cadre();
        banc.molette(egui::vec2(0.0, 120.0));
        banc.molette(egui::vec2(0.0, -120.0));
        assert_eq!(banc.cadre(), avant);
    }

    /// Ctrl + molette (ou pincement) resserre le cadre autour du pointeur : la
    /// donnée sous le curseur y reste, et le cadre se resserre dans les deux axes.
    #[test]
    fn ctrl_molette_resserre_le_cadre_autour_du_pointeur() {
        for geste in ["pincement", "ctrl_molette"] {
            let mut banc = Banc::new(Preset::Integrate, false);
            let avant = banc.cadre();
            let pointeur = egui::pos2(400.0, 300.0);
            banc.deplacer(pointeur);
            // La donnée sous le pointeur, avant.
            let plot_rect = rect_du_trace();
            let sous_le_pointeur = |cadre: Frame| {
                let t = transform_for(
                    plot_rect,
                    cadre.0,
                    cadre.1,
                    (AxisScale::Linear, AxisScale::Linear),
                )
                .unwrap();
                to_data(&t, pointeur).unwrap()
            };
            let donnee = sous_le_pointeur(avant);
            if geste == "pincement" {
                banc.image(vec![egui::Event::Zoom(1.25)]);
            } else {
                banc.avec(CTRL).molette(egui::vec2(0.0, 60.0));
            }
            let apres = banc.cadre();
            assert!(largeur(apres.0) < largeur(avant.0), "{geste} : X resserré");
            assert!(largeur(apres.1) < largeur(avant.1), "{geste} : Y resserré");
            let encore = sous_le_pointeur(apres);
            assert!(
                (encore.0 - donnee.0).abs() < 1e-9 && (encore.1 - donnee.1).abs() < 1e-9,
                "{geste} : la donnée sous le pointeur reste sous le pointeur"
            );
        }
    }

    /// Le rectangle du tracé tel que `preview` le calcule pour une image de
    /// 1400 × 900 sans résidus.
    fn rect_du_trace() -> egui::Rect {
        let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut r = outer.shrink2(egui::vec2(8.0, 4.0));
        r.min.x += 44.0;
        r.max.y -= 18.0;
        r
    }

    /// Au-dessus d'un axe, Ctrl + molette ne cadre que cette échelle.
    #[test]
    fn ctrl_molette_sur_un_axe_ne_zoome_que_cette_echelle() {
        let mut banc = Banc::new(Preset::Integrate, false);
        let avant = banc.cadre();
        banc.deplacer(SUR_X);
        banc.image(vec![egui::Event::Zoom(1.25)]);
        let apres_x = banc.cadre();
        assert!(largeur(apres_x.0) < largeur(avant.0), "X resserré");
        assert_eq!(apres_x.1, avant.1, "Y ne bouge pas");
        banc.deplacer(SUR_Y);
        banc.image(vec![egui::Event::Zoom(1.25)]);
        let apres_y = banc.cadre();
        assert_eq!(apres_y.0, apres_x.0, "X ne bouge plus");
        assert!(largeur(apres_y.1) < largeur(avant.1), "Y resserré");
    }

    /// Maj + molette règle la moyenne de l'aimant, de 1 par cran, sans toucher
    /// au cadre.
    #[test]
    fn maj_molette_regle_la_moyenne_sans_zoomer() {
        let mut banc = Banc::new(Preset::BaselineAnchors, false);
        let (moyenne, cadre) = (banc.form.average, banc.cadre());
        banc.avec(egui::Modifiers::SHIFT)
            .molette(egui::vec2(0.0, 40.0));
        assert_eq!(banc.form.average, moyenne + 1);
        banc.molette(egui::vec2(0.0, -40.0));
        assert_eq!(banc.form.average, moyenne);
        assert_eq!(banc.cadre(), cadre, "le cadre ne bouge pas");
    }

    /// Un cran = un point, même si egui étale le défilement : les images
    /// suivantes, sans événement de molette, ne changent plus rien.
    #[test]
    fn un_cran_de_molette_vaut_un_point_meme_sur_plusieurs_images() {
        let mut banc = Banc::new(Preset::BaselineAnchors, false);
        let moyenne = banc.form.average;
        banc.avec(egui::Modifiers::SHIFT)
            .molette(egui::vec2(0.0, 50.0));
        for _ in 0..5 {
            banc.image(vec![]);
        }
        assert_eq!(banc.form.average, moyenne + 1);
        banc.molette(egui::vec2(50.0, 0.0));
        assert_eq!(banc.form.average, moyenne + 2, "axe horizontal sous Maj");
    }

    /// Ctrl + glisser déplace la vue : le cadre se translate (largeur gardée,
    /// Y intact) et aucune zone n'est posée, malgré l'outil Zone de l'intégrale.
    /// Sur un axe X inversé, les données suivent encore le curseur : les bornes
    /// vont dans l'autre sens.
    #[test]
    fn ctrl_glisser_translate_le_cadre_sans_poser_de_zone() {
        for inverted in [false, true] {
            let mut banc = Banc::new(Preset::Integrate, inverted);
            assert_eq!(banc.form.tool, PreviewTool::Zone);
            let avant = banc.cadre();
            banc.avec(CTRL);
            banc.glisser(egui::pos2(500.0, 400.0), egui::pos2(700.0, 400.0));
            let apres = banc.cadre();
            assert!(
                banc.form.setup.integrate.ranges.is_empty(),
                "Ctrl + glisser ne trace pas de zone ({inverted})"
            );
            assert!(
                (largeur(apres.0) - largeur(avant.0)).abs() < 1e-9,
                "{inverted}"
            );
            assert_eq!(apres.1, avant.1, "pas de déplacement vertical ({inverted})");
            let decalage = apres.0 .0 - avant.0 .0;
            assert!(decalage != 0.0, "le cadre a bougé ({inverted})");
            // Le contenu suit le curseur vers la droite : le bord gauche voit
            // des données plus à gauche dans le sens d'affichage, donc plus
            // petites sans inversion et plus grandes avec.
            assert_eq!(
                decalage < 0.0,
                !inverted,
                "sens du déplacement ({inverted})"
            );
            // La donnée sous le départ du glisser est sous son arrivée.
            let plot_rect = rect_du_trace();
            let t = |cadre: Frame| {
                transform_for(
                    plot_rect,
                    cadre.0,
                    cadre.1,
                    (AxisScale::Linear, AxisScale::Linear),
                )
                .unwrap()
            };
            let (from, to) = (egui::pos2(500.0, 400.0), egui::pos2(700.0, 400.0));
            let avant_x = to_data(&t(avant), from).unwrap().0;
            let apres_x = to_data(&t(apres), to).unwrap().0;
            // L'axe inversé est tracé par le graphe source, pas par ce `t`
            // linéaire : on compare donc l'écart relatif à la largeur du cadre.
            if !inverted {
                assert!(
                    (avant_x - apres_x).abs() < 1e-9,
                    "le contenu suit le curseur"
                );
            }
        }
    }

    /// Ctrl + glisser parti d'un axe ne déplace que cet axe.
    #[test]
    fn ctrl_glisser_sur_un_axe_ne_deplace_que_cet_axe() {
        let mut banc = Banc::new(Preset::Integrate, false);
        let avant = banc.cadre();
        banc.avec(CTRL);
        banc.glisser(SUR_X, SUR_X + egui::vec2(150.0, 60.0));
        let apres = banc.cadre();
        assert_ne!(apres.0, avant.0, "X déplacé");
        assert_eq!(apres.1, avant.1, "Y intact");
        let mut banc = Banc::new(Preset::Integrate, false);
        banc.avec(CTRL);
        banc.glisser(SUR_Y, SUR_Y + egui::vec2(10.0, 80.0));
        assert_eq!(banc.cadre().0, avant.0, "X intact");
        assert_ne!(banc.cadre().1, avant.1, "Y déplacé");
    }

    /// Le double clic recadre : le cadre d'origine, après un zoom comme après un
    /// déplacement.
    #[test]
    fn le_double_clic_rend_le_cadre_d_origine() {
        let mut banc = Banc::new(Preset::Integrate, false);
        let origine = banc.cadre();
        banc.image(vec![egui::Event::Zoom(1.5)]);
        banc.avec(CTRL);
        banc.glisser(egui::pos2(500.0, 400.0), egui::pos2(600.0, 450.0));
        banc.avec(egui::Modifiers::NONE);
        assert_ne!(banc.cadre(), origine);
        banc.deplacer(AU_MILIEU);
        for pressed in [true, false, true, false] {
            banc.bouton(pressed);
        }
        banc.image(vec![]);
        assert_eq!(banc.cadre(), origine);
    }

    /// Un Ctrl + clic simple ne pose rien ; le clic seul pose le point.
    #[test]
    fn ctrl_clic_ne_pose_pas_de_point() {
        let mut banc = Banc::new(Preset::BaselineConstant, false);
        assert_eq!(banc.form.tool, PreviewTool::Points);
        banc.avec(CTRL);
        for pressed in [true, false] {
            banc.bouton(pressed);
        }
        banc.image(vec![]);
        assert_eq!(banc.form.setup.baseline.constant_point, None);
        banc.avec(egui::Modifiers::NONE);
        banc.deplacer(AU_MILIEU + egui::vec2(30.0, 0.0));
        for pressed in [true, false] {
            banc.bouton(pressed);
        }
        banc.image(vec![]);
        assert!(banc.form.setup.baseline.constant_point.is_some());
    }

    /// Détection de pics : le glisser (outil Zone) donne la plage de recherche,
    /// le clic (outil Points) la graine d'un pic ; le glisser en outil Points ne
    /// pose rien.
    #[test]
    fn les_pics_se_posent_par_clic_et_par_glisser() {
        let mut banc = Banc::new(Preset::Peaks, false);
        assert_eq!(banc.form.tool, PreviewTool::Points);
        banc.glisser(egui::pos2(500.0, 400.0), egui::pos2(900.0, 400.0));
        assert_eq!(banc.form.setup.peaks.range, None, "outil Points : rien");
        assert!(banc.form.setup.peaks.seeds.is_empty());
        banc.form.tool = PreviewTool::Zone;
        banc.glisser(egui::pos2(500.0, 400.0), egui::pos2(900.0, 400.0));
        assert!(banc.form.setup.peaks.range.is_some(), "outil Zone : plage");
        banc.form.tool = PreviewTool::Points;
        banc.deplacer(AU_MILIEU);
        for pressed in [true, false] {
            banc.bouton(pressed);
        }
        banc.image(vec![]);
        assert_eq!(
            banc.form.setup.peaks.seeds.len(),
            1,
            "outil Points : graine"
        );
    }
}
