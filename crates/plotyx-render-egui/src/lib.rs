//! Rendu d'un graphe PlotYx à l'écran — phase R4, tâches 4.2 et 4.3.
//!
//! Trois pièces, dans l'ordre où elles s'emboîtent :
//!
//! - [`backend`] : le `DrawingBackend` de Plotters qui collecte des
//!   `egui::Shape` au lieu de peindre ;
//! - [`cache`] : la clé `(revision, rect, pixels_per_point)` qui fait qu'une
//!   image sans changement ne rejoue pas le pipeline ;
//! - [`feathering`] : le levier d'anticrénelage de la tâche R5.5 bis — le
//!   seul morceau du rendu qui n'a pas d'équivalent côté export, parce que
//!   la tessellation n'existe que pour l'écran ;
//! - [`GraphView`], ici : ce que l'UI tient réellement, un cache plus la
//!   mise en relation d'un `Snapshot` de session et d'un nœud `Graph`.
//!
//! # Le point de contact avec le socle
//!
//! Ce crate ne connaît du modèle que deux choses, toutes deux construites par
//! les phases précédentes : `plotyx_core::resolve` (R3), qui traduit un
//! `PlotSpec` en données prêtes à tracer en gérant les références brisées de
//! première main, et `plotyx_session::Snapshot` (socle), dont la `revision`
//! est la clé de cache. Rien n'est ré-routé, rien n'est recopié : les
//! colonnes sont des `Arc<Vec<f64>>` que la résolution clone par compteur.
//!
//! # Mesurer et dessiner ne passent pas par le même objet, et c'est voulu
//!
//! [`ScreenText`] est la [`TextSource`] de l'écran : elle **dessine** par
//! l'`egui::Context` de la fenêtre et **mesure** par un
//! [`TextMeasurer`] hors écran. Ce n'est pas une duplication de la source de
//! mesure — les deux partent des mêmes [`font_definitions`] — mais la
//! conjonction de deux contraintes qui ne peuvent pas être satisfaites par
//! le même appel :
//!
//! - un `Galley` porte les coordonnées UV de ses glyphes **dans l'atlas qui
//!   l'a mis en page** ; un `Shape::Text` construit sur un `Galley` étranger
//!   serait tesselé contre l'atlas du `Context` et afficherait n'importe
//!   quoi. Le dessin doit donc venir du `Context` ;
//! - la géométrie de la figure ne doit pas dépendre de l'écran, et
//!   l'`egui::Context` met toujours en page au `pixels_per_point` courant —
//!   dont la hauteur de ligne d'`epaint` dépend (voir
//!   `plotyx_render::text`). La mesure doit donc venir d'ailleurs.
//!
//! C'est le cas que la mesure R0 0.1 avait à trancher, et sa réponse est ce
//! qui rend ce découpage sûr : **0,000000 px d'écart** sur 540 comparaisons
//! entre une `Fonts` construite à la main et le chemin `Context`. Partager
//! les `FontDefinitions` suffit ; partager l'objet `Fonts` n'aurait pas été
//! possible de toute façon.
//!
//! Pour que ce ne soit pas une coïncidence, l'application doit poser
//! [`font_definitions`] sur son `Context` — `plotyx-app` le fait au
//! démarrage.
//!
//! # Typst — tâche R8.3
//!
//! Derrière la feature `typst` (absente de `default`,
//! `architecture-choisie.md` §7) : [`typst_screen`] rastérise une formule
//! compilée par `plotyx-render` en RGBA (`typst-render`), la charge en
//! texture `egui`, et [`EguiBackend::draw_text`]/[`estimate_text_size`]
//! l'utilisent à la place du chemin `Shape::Text` pour un label reconnu
//! comme une formule ([`plotyx_render::formula::is_formula`]). Sans la
//! feature, ce module n'existe pas et rien ne change : tout le texte,
//! `$...$` compris, reste du `Shape::Text` brut comme avant R8. Une formule
//! qui ne compile pas dégrade vers ce même chemin plutôt que de faire
//! échouer le rendu (voir `typst_screen`, tête de module).

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod cache;
pub mod feathering;
#[cfg(feature = "typst")]
pub mod typst_screen;
pub mod view;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use egui::{
    Color32, Galley, Id, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke, StrokeKind,
};
use plotyx_core::node::NodeKind;
use plotyx_core::resolve::{resolve_panel, resolve_spec, ResolvedPlot};
use plotyx_core::{NodeId, Tree};
use plotyx_plot::{PlotSpec, SeriesAxis};
use plotyx_render::pipeline::{self};
use plotyx_render::series;
use plotyx_session::Snapshot;

pub use backend::{EguiBackend, EguiBackendError};
pub use cache::{CacheKey, Outcome, ShapeCache};
pub use feathering::Feathering;
// Réexportés pour que l'UI n'ait pas à déclarer une arête vers
// `plotyx-render` pour lire ce que la figure a tracé : ce crate est déjà sa
// façade de rendu écran.
pub use plotyx_render::pipeline::{DrawnPanel, DrawnPlot, DEFAULT_FIGURE_SIZE};
pub use plotyx_render::text::{font_definitions, font_definitions_for, TextMeasurer, TextSource};
// Le choix de police (embed du 2026-09-13, `plotyx_render::fonts`) : une UI
// qui veut proposer Fira/Libertinus/Noto ou une police système n'a pas à
// déclarer sa propre arête vers `plotyx-render` pour ça non plus.
pub use plotyx_render::{FontChoice, FontSet, FontSource, ResolvedFonts};
// La correspondance pixel ↔ données (tâche R7) : `plotyx-render` la porte
// (voir `plotyx_render::interact`, générique sur le backend), ce crate ne
// fait que la consommer avec des types egui.
pub use plotyx_render::{nearest_on_curve, PanelTransform};
#[cfg(feature = "typst")]
pub use typst_screen::{FormulaTexture, TypstScreen};
pub use view::{Framing, ViewState};

/// Distance maximale, en pixels, à laquelle un point est encore « désigné »
/// par le curseur (tâche R7.2). Au-delà, personne n'est sous la souris.
pub const HIT_RADIUS: f32 = 12.0;

/// Ce qui peut rater à l'affichage d'un graphe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// Le nœud demandé n'existe pas dans l'instantané.
    UnknownNode(NodeId),
    /// Le nœud existe mais n'est pas un graphe.
    NotAGraph(NodeId),
    /// Plotters a refusé de tracer. Le message est celui de Plotters : le
    /// reformuler perdrait la seule information utile.
    Drawing(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(f, "aucun nœud {id:?} dans cet instantané"),
            Self::NotAGraph(id) => write!(f, "le nœud {id:?} n'est pas un graphe"),
            Self::Drawing(message) => write!(f, "échec du tracé : {message}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// La source de texte de l'écran : dessin par le `Context`, mesure hors
/// écran.
///
/// Les deux moitiés partent des mêmes [`font_definitions`] ; la raison de
/// les séparer est en tête de module.
pub struct ScreenText<'a> {
    /// Le contexte qui possède l'atlas de glyphes dans lequel le texte sera
    /// tesselé.
    pub context: &'a egui::Context,
    /// La source de mesure, indépendante de l'écran.
    pub measurer: &'a TextMeasurer,
}

impl<'a> ScreenText<'a> {
    /// Assemble les deux moitiés.
    pub fn new(context: &'a egui::Context, measurer: &'a TextMeasurer) -> Self {
        Self { context, measurer }
    }
}

impl TextSource for ScreenText<'_> {
    fn measure(&self, text: &str, font_id: egui::FontId) -> egui::Vec2 {
        self.measurer.measure(text, font_id)
    }

    fn layout(&self, text: &str, font_id: egui::FontId, color: Color32) -> Arc<Galley> {
        self.context
            .fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font_id, color))
    }
}

/// Construit les formes d'un graphe résolu dans `shapes`.
///
/// Le point d'entrée bas niveau : sans `Context`, sans cache, sans session.
/// C'est lui que les tests appellent — le rendu se vérifie ainsi **sans
/// serveur graphique**, ce qui est l'argument du §9 de `roadmap-rendu.md`
/// pour faire le renderer avant l'UI.
pub fn collect_shapes(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, RenderError> {
    let backend = EguiBackend::in_rect(shapes, rect, text);
    pipeline::draw(backend, plot).map_err(|error| RenderError::Drawing(error.to_string()))
}

/// Comme [`collect_shapes`], avec le chemin Typst de la tâche R8.3 activé.
///
/// Point d'entrée bas niveau équivalent, pour tester le chemin écran de
/// Typst sans passer par un [`GraphView`] complet ni une session.
#[cfg(feature = "typst")]
pub fn collect_shapes_with_typst(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    typst: typst_screen::TypstScreen<'_>,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, RenderError> {
    let backend = EguiBackend::in_rect(shapes, rect, text).with_typst(typst);
    pipeline::draw(backend, plot).map_err(|error| RenderError::Drawing(error.to_string()))
}

/// Un point désigné sur une courbe — tâche R7.2.
///
/// **Limité à l'axe principal** (bas, gauche) : une série de l'axe
/// secondaire vivrait sur un second `PanelTransform` (bornes `y2_range`,
/// échelle `right.scale`) qu'il faudrait construire à côté, et rien
/// aujourd'hui n'en a l'usage (`plotyx-app` n'affiche encore qu'un banc,
/// sans axe secondaire visible). Écart assumé, pas une omission tue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointHit {
    /// Indice du panneau désigné dans [`DrawnPlot::panels`] — donc parmi les
    /// panneaux effectivement tracés (R5.4), pas dans le spec d'origine.
    pub panel: usize,
    /// Indice de la série dans `PanelSpec::series`, résolu dans le même
    /// ordre (`ResolvedPanel::series`).
    pub series: usize,
    /// Indice du point désigné dans sa série.
    pub point_index: usize,
    /// Position du point, en données.
    pub data: (f64, f64),
    /// Position du point, à l'écran.
    pub pixel: Pos2,
    /// Distance du curseur au point, en pixels.
    pub distance: f32,
}

/// Un graphe affiché : son cache, son état de vue, et de quoi le repeindre.
///
/// Un `GraphView` par vue, pas un par graphe : deux fenêtres sur le même
/// graphe ont deux rectangles, donc deux caches — et, depuis R7, deux
/// cadrages de zoom/pan indépendants, ce qui est le comportement attendu
/// (zoomer dans une fenêtre ne doit pas zoomer l'autre).
#[derive(Debug)]
pub struct GraphView {
    cache: ShapeCache,
    last: Option<DrawnPlot>,
    measurer: TextMeasurer,
    view: ViewState,
    id: Id,
    hover: Option<PointHit>,
    /// Coins du rectangle de zoom élastique en cours (tâche 7.4), en pixels
    /// d'écran. `None` hors glissé secondaire.
    drag_box: Option<(Pos2, Pos2)>,
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

/// Compteur pour donner à chaque `GraphView` un `egui::Id` d'interaction qui
/// lui est propre, indépendant du chemin d'`Ui` où elle est peinte.
///
/// Sans lui, deux vues peintes depuis le même point du code (une boucle sur
/// plusieurs graphes, par exemple) partageraient le même `Id` et donc le
/// même geste de glisser — l'`Id` dérivé de `ui.id()` seul ne les
/// distinguerait pas.
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(0);

impl GraphView {
    /// Une vue neuve, sans rien en cache et sans zoom.
    ///
    /// Construit sa propre source de mesure : c'est le seul coût notable de
    /// la vue (le décodage des polices embarquées), payé une fois à
    /// l'ouverture d'une fenêtre de graphe.
    pub fn new() -> Self {
        Self {
            cache: ShapeCache::new(),
            last: None,
            measurer: TextMeasurer::default(),
            view: ViewState::new(),
            id: Id::new((
                "plotyx_render_egui::GraphView",
                NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            )),
            hover: None,
            drag_box: None,
        }
    }

    /// Le cache, pour l'inspecter (compteurs de la tâche R4.3).
    pub fn cache(&self) -> &ShapeCache {
        &self.cache
    }

    /// La géométrie de la dernière figure construite.
    pub fn last_drawn(&self) -> Option<&DrawnPlot> {
        self.last.as_ref()
    }

    /// L'état de vue (zoom/pan) de cette vue — tâche R7.1.
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// Efface tout cadrage explicite : la vue revient au cadrage sur les
    /// données, pour tous les panneaux.
    pub fn reset_view(&mut self) {
        self.view.reset();
    }

    /// Oublie les formes et repères liés à la surface où la vue était peinte.
    ///
    /// Une même vue peut migrer entre une fenêtre flottante et un viewport OS
    /// séparé. Même si leurs rectangles coïncident par hasard, leurs
    /// coordonnées écran ne désignent pas la même surface : conserver les
    /// formes ou les transformations précédentes décalerait le rendu et le
    /// réticule.
    pub fn invalidate_surface(&mut self) {
        self.cache.invalidate();
        self.last = None;
        self.hover = None;
        self.drag_box = None;
    }

    /// Le point désigné par le curseur à la dernière image peinte, s'il y en
    /// a un à moins de [`HIT_RADIUS`] pixels — tâche R7.2.
    pub fn hovered_point(&self) -> Option<&PointHit> {
        self.hover.as_ref()
    }

    /// Peint le graphe `graph` de `snapshot` dans `rect`, et traite les
    /// gestes de navigation (molette, glissé, rectangle élastique — R7.1 à
    /// R7.4) survenus sur cette zone depuis la dernière image.
    ///
    /// Rend `true` si l'image a été servie depuis le cache — c'est-à-dire si
    /// le pipeline Plotters n'a **pas** été rejoué. `ui` plutôt qu'un
    /// `Painter` nu (comme avant R7) : il faut désormais un `Response` pour
    /// recevoir la molette et le glissé, donc un `Sense` plus large que
    /// `hover()`.
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let spec = graph_spec(&snapshot.tree, graph)?;
        let context = ui.ctx().clone();
        let response = ui.interact(rect, self.id, egui::Sense::click_and_drag());

        // Le câblage prévu par R5.5 bis, à l'appel près (`architecture-choisie.md`
        // §4.2, `feathering` R7.3) : coupé pendant tout glissé — pan ou
        // rectangle de zoom, les deux sont un `dragged()` —, rétabli au repos.
        // Un réglage de `Context`, donc pour toute la fenêtre : sans effet
        // tant qu'un seul graphe est affiché à la fois (`plotyx-app`
        // aujourd'hui), à revoir si plusieurs vues cohabitent un jour.
        feathering::apply(&context, Feathering::during_drag(response.dragged()));

        self.handle_navigation(&response, &context);

        let key = CacheKey {
            revision: snapshot.revision,
            rect,
            pixels_per_point: context.pixels_per_point(),
            view_generation: self.view.generation(),
        };
        let tree = snapshot.tree.clone();
        let measurer = &self.measurer;
        let view = &self.view;
        let outcome = self.cache.get_or_build(key, |shapes| {
            // R7.1 : la surcharge de cadrage vit sur un clone, jamais sur le
            // `PlotSpec` d'origine — voir `crate::view` pour la raison.
            let mut framed = spec.clone();
            view.apply(&mut framed);
            let plot = resolve_spec(&tree, &framed);
            let text = ScreenText::new(&context, measurer);
            collect_shapes(shapes, rect, &text, &plot)
        })?;
        if let Some(drawn) = outcome.built {
            self.last = Some(drawn);
        }
        // Le cœur de la tâche R4.3 : une image sans changement, c'est
        // exactement cette ligne — pas de résolution, pas de `ChartBuilder`,
        // pas de parcours de points.
        let painter = ui.painter();
        painter.extend(self.cache.shapes().iter().cloned());

        // R7.2/7.3 : réticule, désignation de point et rectangle élastique,
        // peints **par-dessus** ce qui précède — jamais dans le `Vec<Shape>`
        // mis en cache ci-dessus, sous peine de rejouer tout le pipeline à
        // chaque mouvement de souris (`crate::cache`, tête de module).
        self.hover = self.paint_overlay(painter, &response, &snapshot.tree, &spec);

        Ok(outcome.cached)
    }

    /// Le panneau (et sa transformation pixel ↔ données) sous le pixel
    /// `pos`, s'il y en a un — cherché dans la **dernière** figure tracée.
    ///
    /// Un cran de retard sur la figure du jour : la géométrie ne bouge
    /// qu'au redimensionnement ou à un zoom déjà commis, donc c'est
    /// imperceptible, et c'est le prix normal du mode immédiat (interpréter
    /// l'entrée de cette image avec la géométrie de la précédente).
    fn panel_at(&self, pos: Pos2) -> Option<(usize, PanelTransform)> {
        let last = self.last.as_ref()?;
        last.panels.iter().enumerate().find_map(|(index, panel)| {
            let transform = PanelTransform::of(panel)?;
            transform
                .contains(pos.x as f64, pos.y as f64)
                .then_some((index, transform))
        })
    }

    /// Pose le cadrage `(x_range, y_range)` calculé pour le panneau `index`
    /// (dans [`DrawnPlot::panels`]) sur l'état de vue, identifié par sa
    /// position déclarée — voir `crate::view` pour la raison de cette
    /// indirection.
    fn commit_framing(&mut self, panel_index: usize, x_range: (f64, f64), y_range: (f64, f64)) {
        let Some(last) = &self.last else { return };
        let Some(panel) = last.panels.get(panel_index) else {
            return;
        };
        let (row, column) = (panel.row, panel.column);
        self.view.set_framing(
            row,
            column,
            Framing {
                x: x_range,
                y: y_range,
            },
        );
    }

    /// Molette (zoom), glissé principal (pan), glissé secondaire (rectangle
    /// de zoom élastique) et double-clic (réinitialisation) — tâches 7.1 et
    /// 7.4. Ne fait rien tant qu'aucune figure n'a encore été tracée.
    fn handle_navigation(&mut self, response: &Response, context: &egui::Context) {
        if self.last.is_none() {
            return;
        }

        if response.double_clicked() {
            self.view.reset();
        }

        if response.hovered() {
            if let Some(pos) = response.hover_pos() {
                let scroll = context.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    if let Some((panel_index, transform)) = self.panel_at(pos) {
                        // Un cran de molette classique vaut plusieurs
                        // dizaines d'unités de défilement ; 0,2 % par unité
                        // donne un zoom perceptible sans à-coups, et le
                        // `clamp` évite qu'un défilement continu (pavé
                        // tactile) ne fasse s'effondrer le cadrage d'un
                        // coup.
                        let factor = (-scroll as f64 * 0.002).exp().clamp(0.5, 2.0);
                        if let Some((x_range, y_range)) =
                            transform.zoom((pos.x as f64, pos.y as f64), factor)
                        {
                            self.commit_framing(panel_index, x_range, y_range);
                        }
                    }
                }
            }
        }

        if response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if let Some(pos) = response.interact_pointer_pos() {
                // Le panneau visé est celui sous le point de départ du
                // geste : une fois qu'on glisse, le curseur peut sortir de
                // la zone de tracé (paner jusqu'au bord d'un panneau voisin)
                // sans que la cible ne change en cours de route.
                let origin = pos - delta;
                if let Some((panel_index, transform)) =
                    self.panel_at(origin).or_else(|| self.panel_at(pos))
                {
                    if let Some((x_range, y_range)) = transform.pan(delta.x as f64, delta.y as f64)
                    {
                        self.commit_framing(panel_index, x_range, y_range);
                    }
                }
            }
        }

        if response.dragged_by(PointerButton::Secondary) {
            if let Some(pos) = response.interact_pointer_pos() {
                // **Ne pas** se fier à `drag_started_by` pour capturer le
                // point de départ : sur un widget sensible au clic *et* au
                // glissé, `egui` diffère la décision « clic ou glissé » tant
                // que le mouvement ne la tranche pas
                // (`egui::interaction::interact`, commentaire *"the mouse
                // first is pressed, it could be either [...] so we postpone
                // the decision"*) — `drag_started_by` ne devient vrai qu'à
                // l'image où la décision tombe, et `interact_pointer_pos()`
                // porte déjà la position **courante** à ce moment-là, pas
                // celle de la pression initiale. `drag_delta()` reste fiable
                // (c'est le mouvement de **cette** image, quelle que soit la
                // date de la décision) : `pos - drag_delta()` retrouve donc
                // le point de départ réel la première fois qu'on voit
                // `dragged_by`, sans dépendre de `drag_started_by` du tout.
                let start = self
                    .drag_box
                    .map_or_else(|| pos - response.drag_delta(), |(start, _)| start);
                self.drag_box = Some((start, pos));
            }
        }
        if response.drag_stopped_by(PointerButton::Secondary) {
            if let Some((start, end)) = self.drag_box.take() {
                // En dessous de quelques pixels, c'est un tremblement de
                // main, pas un rectangle : un rectangle de zoom d'un pixel
                // de large produirait un cadrage quasi dégénéré, que le
                // pipeline redresserait silencieusement en revenant aux
                // données (`pipeline::axis_range`) — un comportement correct
                // mais surprenant pour un geste qui se voulait un zoom.
                if start.distance(end) > 4.0 {
                    if let Some((panel_index, transform)) = self.panel_at(start) {
                        if let Some((x_range, y_range)) = transform.range_between(
                            (start.x as f64, start.y as f64),
                            (end.x as f64, end.y as f64),
                        ) {
                            self.commit_framing(panel_index, x_range, y_range);
                        }
                    }
                }
            }
        }
    }

    /// Réticule, désignation de point et rectangle de zoom élastique —
    /// tâches 7.2 à 7.4. Peint directement sur `painter`, **jamais** dans le
    /// `Vec<Shape>` du cache : c'est ce qui remplace le blitting
    /// `copy_from_bbox` de la version Python (voir `crate::cache`).
    fn paint_overlay(
        &self,
        painter: &Painter,
        response: &Response,
        tree: &Tree,
        spec: &PlotSpec,
    ) -> Option<PointHit> {
        if let Some((start, end)) = self.drag_box {
            let rect = Rect::from_two_pos(start, end);
            painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(90, 150, 255, 40));
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
                StrokeKind::Outside,
            );
        }

        let pos = response.hover_pos()?;
        let (panel_index, transform) = self.panel_at(pos)?;
        let last = self.last.as_ref()?;
        let panel = last.panels.get(panel_index)?;
        let panel_rect = Rect::from_min_size(
            Pos2::new(panel.pixel_origin.0 as f32, panel.pixel_origin.1 as f32),
            egui::vec2(panel.pixel_size.0 as f32, panel.pixel_size.1 as f32),
        );
        if !panel_rect.contains(pos) {
            return None;
        }

        let stroke = Stroke::new(1.0, Color32::from_gray(150));
        painter.line_segment(
            [
                Pos2::new(pos.x, panel_rect.top()),
                Pos2::new(pos.x, panel_rect.bottom()),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(panel_rect.left(), pos.y),
                Pos2::new(panel_rect.right(), pos.y),
            ],
            stroke,
        );

        let hit = nearest_point(tree, spec, panel_index, panel, &transform, pos)?;
        painter.circle_stroke(
            hit.pixel,
            5.0,
            Stroke::new(1.5, Color32::from_rgb(220, 90, 90)),
        );
        Some(hit)
    }
}

/// Le point désigné du panneau `panel_index` sous le pixel `pos` — tâche
/// R7.2, l'équivalent local de `ReverseCoordTranslate` + `min_by_key`
/// d'`architecture-choisie.md` §6 (`min_by_key` vit dans
/// `plotyx_render::nearest_on_curve`, réutilisé série par série).
///
/// Recalcule la géométrie plutôt que de la garder en mémoire : une série
/// résolue ne porte que des `Arc<Vec<f64>>` clonés par compteur
/// (`plotyx-core::resolve`), donc le refaire à chaque survol coûte un
/// parcours, pas une copie — moins cher que de le garder à jour à travers
/// chaque zoom.
fn nearest_point(
    tree: &Tree,
    spec: &PlotSpec,
    panel_index: usize,
    drawn_panel: &DrawnPanel,
    transform: &PanelTransform,
    pos: Pos2,
) -> Option<PointHit> {
    let spec_panel = spec
        .panels
        .iter()
        .find(|panel| panel.row == drawn_panel.row && panel.column == drawn_panel.column)?;
    let resolved = resolve_panel(tree, spec_panel);
    let cursor = (pos.x as f64, pos.y as f64);

    let mut best: Option<PointHit> = None;
    for (series_index, resolved_series) in resolved.series.iter().enumerate() {
        let series_spec = resolved_series.spec;
        // Désignation limitée à l'axe principal — voir la doc de `PointHit`.
        if !series_spec.visible || series_spec.axis == SeriesAxis::Right {
            continue;
        }
        let prepared = series::prepare(
            resolved_series,
            spec_panel.bottom.scale,
            spec_panel.left.scale,
        );
        if prepared.broken || prepared.unknown_type {
            continue;
        }
        let series::Geometry::Curve { points, .. } = &prepared.geometry else {
            continue;
        };
        let Some((point_index, distance)) = nearest_on_curve(transform, points, cursor) else {
            continue;
        };
        if best.is_some_and(|hit| hit.distance as f64 <= distance) {
            continue;
        }
        let Some(pixel) = transform.data_to_pixel(points[point_index].0, points[point_index].1)
        else {
            continue;
        };
        best = Some(PointHit {
            panel: panel_index,
            series: series_index,
            point_index,
            data: points[point_index],
            pixel: Pos2::new(pixel.0 as f32, pixel.1 as f32),
            distance: distance as f32,
        });
    }
    best.filter(|hit| hit.distance <= HIT_RADIUS)
}

/// Le `PlotSpec` d'un nœud, cloné.
///
/// Cloné et non emprunté : le spec ne porte que des identités et du style
/// (`plotyx-plot` : « aucune donnée »), donc la copie est bon marché, et
/// l'emprunt obligerait l'`Arc<Tree>` à survivre à la fermeture de la
/// `FnOnce` du cache.
fn graph_spec(tree: &Tree, graph: NodeId) -> Result<PlotSpec, RenderError> {
    let node = tree.node(graph).ok_or(RenderError::UnknownNode(graph))?;
    match node.kind() {
        NodeKind::Graph { spec, .. } => Ok(spec.clone()),
        _ => Err(RenderError::NotAGraph(graph)),
    }
}
