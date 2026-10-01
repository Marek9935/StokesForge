//! Rendu d'un graphe PlotYx à l'écran — phase R4, tâches 4.2 et 4.3.
//!
//! Trois pièces, dans l'ordre où elles s'emboîtent :
//!
//! - [`backend`] : le `DrawingBackend` de Plotters qui collecte des
//!   `egui::Shape` au lieu de peindre ;
//! - [`cache`] : la clé `(revision, rect, pixels_per_point, view_generation)` qui fait qu'une
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
//! # Typst — tâche R8.3, branché par l'issue #29
//!
//! [`typst_screen`] rastérise une formule compilée par `plotyx-render` en
//! RGBA (`typst-render`), la charge en texture `egui`, et
//! [`EguiBackend::draw_text`]/[`estimate_text_size`] l'utilisent à la place
//! du chemin `Shape::Text` pour tout texte riche — titres, titres d'axe,
//! légende, marqués par leur style ([`plotyx_render::formula::is_rich`],
//! issue #32). Un texte qui ne compile pas
//! dégrade vers le `Shape::Text` brut plutôt que de faire échouer le rendu
//! (voir `typst_screen`, tête de module).
//!
//! Typst est toujours compilé (la feature `typst` a été supprimée, issue
//! #29), et [`GraphView`] — donc l'application — l'active toujours : il
//! tient le cache des textures ([`FormulaCache`]) et les polices résolues
//! une fois pour toutes. Reste optionnel au niveau du **backend** :
//! [`collect_shapes`] trace tout en texte brut, [`collect_shapes_with_typst`]
//! passe les formules par Typst. Le premier n'est pas un reliquat : c'est le
//! témoin des tests qui prouvent que Typst est réellement consulté (une
//! figure avec formule doit différer d'un chemin à l'autre), et le point
//! d'entrée des tests qui n'ont pas de `Context` egui à prêter.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod cache;
pub mod feathering;
pub mod scaled;
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
use plotyx_render::formula::FormulaCache;
use plotyx_render::pipeline::{self};
use plotyx_render::series;
use plotyx_session::Revision;
use plotyx_session::Snapshot;

pub use backend::{EguiBackend, EguiBackendError};
pub use cache::{CacheKey, Outcome, ShapeCache};
pub use feathering::Feathering;
// Réexportés pour que l'UI n'ait pas à déclarer une arête vers
// `plotyx-render` pour lire ce que la figure a tracé : ce crate est déjà sa
// façade de rendu écran.
pub use plotyx_render::pipeline::{DrawnAnnotation, DrawnPanel, DrawnPlot, DEFAULT_FIGURE_SIZE};
// L'adresse et le modèle d'un texte libre : l'application les manipule avec
// les signaux de la vue ([`GraphEvent`]).
pub use plotyx_plot::{AnnotationRef, AnnotationSpec};
pub use plotyx_render::text::{font_definitions, font_definitions_for, TextMeasurer, TextSource};
// Le choix de police (embed du 2026-09-13, `plotyx_render::fonts`) : une UI
// qui veut proposer Fira/Libertinus/Noto ou une police système n'a pas à
// déclarer sa propre arête vers `plotyx-render` pour ça non plus.
pub use plotyx_render::{FontChoice, FontSet, FontSource, ResolvedFonts};
// La correspondance pixel ↔ données (tâche R7) : `plotyx-render` la porte
// (voir `plotyx_render::interact`, générique sur le backend), ce crate ne
// fait que la consommer avec des types egui.
pub use plotyx_render::{component_at, nearest_on_curve, AxisSide, PanelTransform, PlotComponent};
pub use typst_screen::{FormulaTexture, TypstScreen};
// Typst côté modèle de texte (compilation, échappement) : l'interface en a
// besoin pour vérifier ce qu'elle propose d'écrire (mémo de syntaxe des
// propriétés), sans déclarer d'arête vers `plotyx-render`.
pub use plotyx_render::formula;
pub use view::{Framing, ViewState};

/// Ce que la vue signale à l'application, à traduire en actions — textes
/// libres (issue #35). Relevé par [`GraphView::take_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum GraphEvent {
    /// Double-clic sur un texte libre : l'éditer sur place.
    EditAnnotation(AnnotationRef),
    /// Un texte libre a été glissé puis relâché : sa nouvelle version, à
    /// valider en **une** modification du modèle. L'aperçu reste affiché
    /// jusqu'à ce que le modèle change ([`GraphView::hold_preview`]).
    AnnotationMoved {
        /// Le texte déplacé.
        target: AnnotationRef,
        /// Sa version déplacée.
        annotation: AnnotationSpec,
    },
}

/// Un glisser de texte libre en cours.
#[derive(Debug, Clone)]
struct AnnotationDrag {
    target: AnnotationRef,
    original: AnnotationSpec,
    /// Centre tracé du texte au début du geste, en repère de la figure.
    start_center: Pos2,
    /// Point d'appui du geste, en repère de la figure.
    press: Pos2,
}

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
/// graphe ont chacune leur cache et, depuis R7, leur propre cadrage de
/// zoom/pan, ce qui est le comportement attendu
/// (zoomer dans une fenêtre ne doit pas zoomer l'autre).
#[derive(Debug)]
pub struct GraphView {
    cache: ShapeCache,
    /// Les formules Typst déjà rastérisées (issue #29) : vit aussi longtemps
    /// que la vue, pour qu'une reconstruction de la figure (zoom, révision)
    /// ne recompile pas des titres inchangés. Borné par
    /// [`FORMULA_CACHE_LIMIT`].
    formulas: FormulaCache<FormulaTexture>,
    /// Le choix de police des formules — la valeur par défaut, comme le
    /// [`TextMeasurer`] de la vue.
    font_choice: FontChoice,
    /// Les polices Typst (texte, Math) de `font_choice`, résolues une fois à
    /// la création de la vue. `None` si la résolution a échoué — alors tout
    /// le texte reste brut, sans faire échouer le rendu (invariant 10).
    typst_fonts: Option<(typst::text::Font, typst::text::Font)>,
    /// Les formes du cache mises à l'échelle d'affichage, texte (issue #18)
    /// et formules Typst (issue #31) compris : voir [`scaled`].
    scaled: scaled::ScaledShapes,
    last: Option<DrawnPlot>,
    measurer: TextMeasurer,
    view: ViewState,
    id: Id,
    hover: Option<PointHit>,
    /// Rectangle visible du dernier rendu ; le cache garde, lui, le repère
    /// logique de référence quand l'application agrandit ou réduit la figure.
    display_rect: Option<Rect>,
    /// Coins du rectangle de zoom élastique en cours (tâche 7.4), en pixels
    /// d'écran. `None` hors glissé secondaire.
    drag_box: Option<(Pos2, Pos2)>,
    /// Réponse d'interaction de la figure au dernier rendu : l'application
    /// y accroche son menu contextuel (clic droit **sans** glisser — le
    /// glisser secondaire reste le zoom élastique).
    response: Option<egui::Response>,
    /// Aperçu d'un texte libre en cours d'édition ou de glisser (issue
    /// #35) : posé sur le clone du spec à la construction de la figure,
    /// comme le zoom — jamais dans le modèle tant que l'application n'a pas
    /// validé.
    annotation_preview: Option<(AnnotationRef, AnnotationSpec)>,
    /// Compteur de l'aperçu, ajouté à la génération de vue dans la clé du
    /// cache.
    preview_generation: u64,
    /// Révision du modèle à laquelle l'aperçu a été validé : il s'efface dès
    /// que le modèle a changé, pour ne pas montrer une image de l'ancienne
    /// position entre la validation et la mise à jour du modèle.
    preview_until: Option<Revision>,
    annotation_drag: Option<AnnotationDrag>,
    /// Texte libre sélectionné (clic), cible de Suppr.
    selected_annotation: Option<AnnotationRef>,
    events: Vec<GraphEvent>,
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

/// Nombre de formules au-delà duquel [`GraphView`] vide son cache de
/// textures avant de reconstruire la figure.
///
/// Chaque frappe dans un titre à formule crée une entrée (et une texture)
/// qui ne resservira plus. Une figure réelle compte quelques dizaines de
/// textes au plus — un titre, trois titres d'axe, vingt-quatre entrées de
/// légende (`legend::MAX_ENTRIES`) —, donc la limite ne se déclenche
/// qu'après une longue édition, et coûte alors une recompilation de ce qui
/// est affiché (0,7 ms par formule).
pub const FORMULA_CACHE_LIMIT: usize = 256;

impl GraphView {
    /// Une vue neuve, sans rien en cache et sans zoom.
    ///
    /// Construit sa propre source de mesure : c'est le seul coût notable de
    /// la vue (le décodage des polices embarquées), payé une fois à
    /// l'ouverture d'une fenêtre de graphe.
    pub fn new() -> Self {
        let font_choice = FontChoice::default();
        Self {
            cache: ShapeCache::new(),
            formulas: FormulaCache::new(),
            typst_fonts: plotyx_render::formula::resolve_fonts(&font_choice).ok(),
            font_choice,
            scaled: scaled::ScaledShapes::default(),
            last: None,
            measurer: TextMeasurer::default(),
            view: ViewState::new(),
            id: Id::new((
                "plotyx_render_egui::GraphView",
                NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            )),
            hover: None,
            display_rect: None,
            drag_box: None,
            response: None,
            annotation_preview: None,
            preview_generation: 0,
            preview_until: None,
            annotation_drag: None,
            selected_annotation: None,
            events: Vec::new(),
        }
    }

    /// Le cache, pour l'inspecter (compteurs de la tâche R4.3).
    pub fn cache(&self) -> &ShapeCache {
        &self.cache
    }

    /// Nombre de formules Typst gardées en cache par cette vue — pour les
    /// tests (issue #29).
    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }

    /// La géométrie de la dernière figure construite.
    pub fn last_drawn(&self) -> Option<&DrawnPlot> {
        self.last.as_ref()
    }

    /// Rectangle réellement occupé à l'écran lors du dernier rendu.
    pub fn display_rect(&self) -> Option<Rect> {
        self.display_rect
    }

    /// L'état de vue (zoom/pan) de cette vue — tâche R7.1.
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// L'identifiant d'interaction de cette vue — propre à la vue, stable,
    /// de quoi y accrocher un état d'interface (l'éditeur sur place d'un
    /// texte libre, `plotyx-app`).
    pub fn id(&self) -> Id {
        self.id
    }

    /// Les signaux de la vue depuis le dernier relevé (issue #35).
    pub fn take_events(&mut self) -> Vec<GraphEvent> {
        std::mem::take(&mut self.events)
    }

    /// Le texte libre sélectionné, s'il y en a un.
    pub fn selected_annotation(&self) -> Option<AnnotationRef> {
        self.selected_annotation
    }

    /// Désélectionne le texte libre (après sa suppression, par exemple).
    pub fn clear_annotation_selection(&mut self) {
        self.selected_annotation = None;
    }

    /// L'aperçu d'un texte libre, s'il y en a un.
    pub fn annotation_preview(&self) -> Option<&(AnnotationRef, AnnotationSpec)> {
        self.annotation_preview.as_ref()
    }

    /// Pose (ou retire) l'aperçu d'un texte libre : la figure le montre à la
    /// prochaine image, sans toucher au modèle. Un aperçu identique ne
    /// reconstruit rien.
    pub fn set_annotation_preview(&mut self, preview: Option<(AnnotationRef, AnnotationSpec)>) {
        self.preview_until = None;
        if self.annotation_preview != preview {
            self.annotation_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// Garde l'aperçu affiché jusqu'à ce que le modèle change — à appeler
    /// quand l'application vient de valider l'aperçu (une modification du
    /// modèle est en route). Sans révision connue, l'aperçu s'efface.
    pub fn hold_preview(&mut self) {
        match self.cache.key() {
            Some(key) => self.preview_until = Some(key.revision),
            None => self.set_annotation_preview(None),
        }
    }

    /// Le texte libre sous le pixel egui `pos`, cherché dans la dernière
    /// figure tracée.
    pub fn annotation_at(&self, pos: Pos2) -> Option<AnnotationRef> {
        match self.component_at(pos)? {
            PlotComponent::Annotation { panel, index } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(AnnotationRef {
                    row: drawn.row,
                    column: drawn.column,
                    index,
                })
            }
            _ => None,
        }
    }

    /// Le point de données sous le pixel egui `pos` (ordonnée de gauche), et
    /// la position `(row, column)` de son panneau.
    pub fn data_at(&self, pos: Pos2) -> Option<(usize, usize, f64, f64)> {
        let (index, transform) = self.panel_at(pos)?;
        let figure = self.figure_position(pos)?;
        let (x, y) = transform.pixel_to_data(figure.x as f64, figure.y as f64)?;
        let panel = self.last.as_ref()?.panels.get(index)?;
        Some((panel.row, panel.column, x, y))
    }

    /// Le point de données au centre de la zone de tracé du premier panneau
    /// tracé — où poser un texte ajouté par la barre de menus.
    pub fn panel_center_data(&self) -> Option<(usize, usize, f64, f64)> {
        let panel = self.last.as_ref()?.panels.first()?;
        let transform = PanelTransform::of(panel)?;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let (x, y) = transform.pixel_to_data(center.0, center.1)?;
        Some((panel.row, panel.column, x, y))
    }

    /// Où un texte libre est tracé à l'écran : centre, taille du texte droit
    /// et rotation (degrés, antihoraire) — pour poser l'éditeur sur place.
    pub fn annotation_screen_box(&self, target: AnnotationRef) -> Option<(Pos2, egui::Vec2, f32)> {
        let drawn = self.drawn_annotation(target)?;
        let display = self.display_rect?;
        let scale = self.display_scale()?;
        let center = display.min + egui::vec2(drawn.center.0 as f32, drawn.center.1 as f32) * scale;
        let size = egui::vec2(drawn.size.0 as f32, drawn.size.1 as f32) * scale;
        Some((center, size, drawn.rotation as f32))
    }

    fn drawn_annotation(&self, target: AnnotationRef) -> Option<&DrawnAnnotation> {
        self.last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.row == target.row && panel.column == target.column)?
            .annotations
            .iter()
            .find(|annotation| annotation.index == target.index)
    }

    /// `spec` tel que cette vue le montre : un clone, cadré par le zoom
    /// courant ([`ViewState::apply`], le même appel que la construction de
    /// la figure). C'est ce qu'exporte l'application (issue #34) — on
    /// exporte ce qu'on voit — sans que le zoom touche le `PlotSpec` du
    /// modèle. Sans zoom, `spec` inchangé.
    pub fn framed_spec(&self, spec: &PlotSpec) -> PlotSpec {
        let mut framed = spec.clone();
        self.view.apply(&mut framed);
        framed
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
        self.display_rect = None;
        self.drag_box = None;
        self.response = None;
    }

    /// Réponse d'interaction de la figure au dernier rendu, pour y accrocher
    /// un menu contextuel (`Response::context_menu`). Réutiliser celle-ci
    /// plutôt qu'en créer une seconde sur le même rectangle : une autre
    /// réponse superposée disputerait le glisser à la navigation.
    pub fn response(&self) -> Option<&egui::Response> {
        self.response.as_ref()
    }

    /// Le composant du graphe (titre, zone de tracé, axe, titre d'axe) sous
    /// le pixel egui `pos`, cherché dans la **dernière** figure tracée — même
    /// cran de retard, imperceptible, que [`Self::panel_at`].
    pub fn component_at(&self, pos: Pos2) -> Option<PlotComponent> {
        let pos = self.figure_position(pos)?;
        component_at(self.last.as_ref()?, pos.x as f64, pos.y as f64)
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
        self.paint_with_layout(ui, rect, rect, snapshot, graph)
    }

    /// Dispose toujours la figure à `DEFAULT_FIGURE_SIZE`, puis applique une
    /// transformation uniforme à ses formes. La densité des graduations,
    /// l'emplacement des légendes et les mesures de texte ne changent donc
    /// pas avec la taille de la fenêtre ; seule leur taille affichée change.
    pub fn paint_scaled(
        &mut self,
        ui: &egui::Ui,
        display_rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let (width, height) = DEFAULT_FIGURE_SIZE;
        let logical_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32));
        if display_rect.width() <= 0.0 || display_rect.height() <= 0.0 {
            self.hover = None;
            return Ok(true);
        }
        self.paint_with_layout(ui, logical_rect, display_rect, snapshot, graph)
    }

    fn paint_with_layout(
        &mut self,
        ui: &egui::Ui,
        logical_rect: Rect,
        display_rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let spec = graph_spec(&snapshot.tree, graph)?;
        let context = ui.ctx().clone();
        let response = ui.interact(display_rect, self.id, egui::Sense::click_and_drag());
        self.display_rect = Some(display_rect);
        self.response = Some(response.clone());

        // Le câblage prévu par R5.5 bis, à l'appel près (`architecture-choisie.md`
        // §4.2, `feathering` R7.3) : coupé pendant tout glissé — pan ou
        // rectangle de zoom, les deux sont un `dragged()` —, rétabli au repos.
        // Un réglage de `Context`, donc pour toute la fenêtre : sans effet
        // tant qu'un seul graphe est affiché à la fois (`plotyx-app`
        // aujourd'hui), à revoir si plusieurs vues cohabitent un jour.
        feathering::apply(&context, Feathering::during_drag(response.dragged()));

        // Un aperçu validé s'efface dès que le modèle l'a rattrapé.
        if self
            .preview_until
            .is_some_and(|revision| revision != snapshot.revision)
        {
            self.set_annotation_preview(None);
        }

        self.handle_navigation(&response, &context, &spec);

        let key = CacheKey {
            revision: snapshot.revision,
            rect: logical_rect,
            pixels_per_point: context.pixels_per_point(),
            // Deux compteurs qui ne font que monter : leur somme change à
            // chaque zoom comme à chaque aperçu de texte libre.
            view_generation: self.view.generation() + self.preview_generation,
        };
        let tree = snapshot.tree.clone();
        let measurer = &self.measurer;
        let view = &self.view;
        let preview = &self.annotation_preview;
        let formulas = &self.formulas;
        let font_choice = &self.font_choice;
        let typst_fonts = &self.typst_fonts;
        let outcome = self.cache.get_or_build(key, |shapes| {
            // R7.1 : la surcharge de cadrage vit sur un clone, jamais sur le
            // `PlotSpec` d'origine — voir `crate::view` pour la raison.
            let mut framed = spec.clone();
            view.apply(&mut framed);
            if let Some((target, annotation)) = preview {
                target.set(&mut framed, annotation.clone());
            }
            let plot = resolve_spec(&tree, &framed);
            let text = ScreenText::new(&context, measurer);
            let Some(fonts) = typst_fonts.clone() else {
                return collect_shapes(shapes, logical_rect, &text, &plot);
            };
            // Vidé ici seulement : les formes que ce cache désignait sont
            // justement en train d'être remplacées (voir `FormulaCache::clear`).
            if formulas.len() > FORMULA_CACHE_LIMIT {
                formulas.clear();
            }
            let typst = TypstScreen::with_fonts(&context, formulas, font_choice, fonts);
            collect_shapes_with_typst(shapes, logical_rect, &text, typst, &plot)
        })?;
        if let Some(drawn) = outcome.built {
            self.last = Some(drawn);
        }
        // Le cœur de la tâche R4.3 : une image sans changement, c'est
        // exactement cette ligne — pas de résolution, pas de `ChartBuilder`,
        // pas de parcours de points.
        let painter = ui.painter().with_clip_rect(display_rect);
        let scale = display_rect.width() / logical_rect.width();
        if logical_rect == display_rect {
            painter.extend(self.cache.shapes().iter().cloned());
        } else {
            let transform = egui::emath::TSTransform::new(
                display_rect.min.to_vec2() - logical_rect.min.to_vec2() * scale,
                scale,
            );
            let scaled_key = scaled::ScaledKey {
                source: key,
                display: display_rect,
            };
            painter.extend(
                self.scaled
                    .get(
                        &context,
                        scaled_key,
                        self.cache.shapes(),
                        transform,
                        &self.formulas,
                    )
                    .iter()
                    .cloned(),
            );
        }

        // R7.2/7.3 : réticule, désignation de point et rectangle élastique,
        // peints **par-dessus** ce qui précède — jamais dans le `Vec<Shape>`
        // mis en cache ci-dessus, sous peine de rejouer tout le pipeline à
        // chaque mouvement de souris (`crate::cache`, tête de module).
        self.hover = self.paint_overlay(&painter, &response, &snapshot.tree, &spec);

        Ok(outcome.cached)
    }

    /// Coordonnée de `pos` dans le repère local de la figure mise en cache.
    ///
    /// Le pipeline de rendu produit les [`DrawnPanel`] dans ce repère ; egui,
    /// lui, livre les positions du pointeur dans le repère de la fenêtre.
    /// Une figure peut être centrée dans une fenêtre flottante, donc les deux
    /// origines ne coïncident pas en général.
    fn figure_position(&self, pos: Pos2) -> Option<Pos2> {
        let display = self.display_rect?;
        let logical = self.cache.key()?.rect;
        let scale = display.width() / logical.width();
        (scale > 0.0).then(|| Pos2::ZERO + (pos - display.min) / scale)
    }

    fn display_scale(&self) -> Option<f32> {
        Some(self.display_rect?.width() / self.cache.key()?.rect.width())
    }

    /// Le panneau (et sa transformation pixel ↔ données) sous le pixel egui
    /// `pos`, s'il y en a un — cherché dans la **dernière** figure tracée.
    ///
    /// Un cran de retard sur la figure du jour : la géométrie ne bouge
    /// qu'au redimensionnement ou à un zoom déjà commis, donc c'est
    /// imperceptible, et c'est le prix normal du mode immédiat (interpréter
    /// l'entrée de cette image avec la géométrie de la précédente).
    fn panel_at(&self, pos: Pos2) -> Option<(usize, PanelTransform)> {
        let pos = self.figure_position(pos)?;
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
    fn handle_navigation(&mut self, response: &Response, context: &egui::Context, spec: &PlotSpec) {
        if self.last.is_none() {
            return;
        }

        // Issue #35 : un clic choisit le texte libre sous le pointeur (ou
        // aucun), un double-clic sur un texte l'édite au lieu de
        // réinitialiser le zoom.
        let pointer = response.interact_pointer_pos();
        if response.clicked() {
            self.selected_annotation = pointer.and_then(|pos| self.annotation_at(pos));
        }
        if response.double_clicked() {
            match pointer.and_then(|pos| self.annotation_at(pos)) {
                Some(target) => {
                    self.selected_annotation = Some(target);
                    self.events.push(GraphEvent::EditAnnotation(target));
                }
                None => self.view.reset(),
            }
        }
        if self.handle_annotation_drag(response, spec) {
            return;
        }

        if response.hovered() {
            if let Some(pos) = response
                .hover_pos()
                .filter(|pos| response.interact_rect.contains(*pos))
                .and_then(|pos| self.figure_position(pos).map(|position| (pos, position)))
            {
                let scroll = context.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    if let Some((panel_index, transform)) = self.panel_at(pos.0) {
                        // Un cran de molette classique vaut plusieurs
                        // dizaines d'unités de défilement ; 0,2 % par unité
                        // donne un zoom perceptible sans à-coups, et le
                        // `clamp` évite qu'un défilement continu (pavé
                        // tactile) ne fasse s'effondrer le cadrage d'un
                        // coup.
                        let factor = (-scroll as f64 * 0.002).exp().clamp(0.5, 2.0);
                        if let Some((x_range, y_range)) =
                            transform.zoom((pos.1.x as f64, pos.1.y as f64), factor)
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
                    let scale = self.display_scale().unwrap_or(1.0);
                    if let Some((x_range, y_range)) =
                        transform.pan((delta.x / scale) as f64, (delta.y / scale) as f64)
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
                        if let (Some(start), Some(end)) =
                            (self.figure_position(start), self.figure_position(end))
                        {
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
    }

    /// Glisser un texte libre (issue #35) : un glisser principal **parti
    /// d'un texte** le déplace au lieu de faire défiler la vue. L'aperçu suit
    /// le pointeur ; au relâchement, un seul [`GraphEvent::AnnotationMoved`].
    /// Rend `true` si le geste est un glisser de texte (la navigation ne
    /// doit alors rien en faire).
    fn handle_annotation_drag(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(drag) = self.annotation_drag.take() {
                if let Some((target, annotation)) = self.annotation_preview.clone() {
                    if annotation != drag.original {
                        self.events
                            .push(GraphEvent::AnnotationMoved { target, annotation });
                        self.hold_preview();
                    } else {
                        self.set_annotation_preview(None);
                    }
                }
                return true;
            }
            return false;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.annotation_drag.is_some();
        };
        if self.annotation_drag.is_none() {
            // Le point de départ réel, comme pour le rectangle élastique.
            let origin = pos - response.drag_delta();
            let Some(target) = self.annotation_at(origin) else {
                return false;
            };
            let (Some(original), Some(drawn), Some(press)) = (
                target.get(spec).cloned(),
                self.drawn_annotation(target).copied(),
                self.figure_position(origin),
            ) else {
                return false;
            };
            self.selected_annotation = Some(target);
            self.annotation_drag = Some(AnnotationDrag {
                target,
                original,
                start_center: Pos2::new(drawn.center.0 as f32, drawn.center.1 as f32),
                press,
            });
        }
        let Some(drag) = self.annotation_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        let center = drag.start_center + (current - drag.press);
        let right = drag.original.axis == SeriesAxis::Right;
        let moved = self
            .last
            .as_ref()
            .and_then(|last| {
                last.panels.iter().find(|panel| {
                    panel.row == drag.target.row && panel.column == drag.target.column
                })
            })
            .and_then(|panel| PanelTransform::of_axis(panel, right))
            .and_then(|transform| transform.pixel_to_data(center.x as f64, center.y as f64));
        if let Some((x, y)) = moved {
            let annotation = AnnotationSpec {
                x,
                y,
                ..drag.original.clone()
            };
            self.set_annotation_preview(Some((drag.target, annotation)));
        }
        true
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

        // Le texte libre sélectionné : son rectangle tourné (issue #35).
        if let Some((center, size, rotation)) = self
            .selected_annotation
            .and_then(|target| self.annotation_screen_box(target))
        {
            let (sin, cos) = rotation.to_radians().sin_cos();
            let half = size / 2.0 + egui::vec2(3.0, 3.0);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
                let (u, v) = (sx * half.x, sy * half.y);
                // Rotation antihoraire à l'écran (axe y vers le bas).
                center + egui::vec2(u * cos + v * sin, -u * sin + v * cos)
            });
            painter.add(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
            ));
        }

        let pos = response.hover_pos()?;
        let figure_pos = self.figure_position(pos)?;
        let (panel_index, transform) = self.panel_at(pos)?;
        let last = self.last.as_ref()?;
        let panel = last.panels.get(panel_index)?;
        let figure_rect = self.display_rect?;
        let scale = self.display_scale()?;
        let panel_rect = Rect::from_min_size(
            figure_rect.min
                + egui::vec2(panel.pixel_origin.0 as f32, panel.pixel_origin.1 as f32) * scale,
            egui::vec2(panel.pixel_size.0 as f32, panel.pixel_size.1 as f32) * scale,
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

        let mut hit = nearest_point(tree, spec, panel_index, panel, &transform, figure_pos)?;
        hit.pixel = figure_rect.min + hit.pixel.to_vec2() * scale;
        hit.distance *= scale;
        painter.circle_stroke(
            hit.pixel,
            5.0 * scale,
            Stroke::new(1.5 * scale, Color32::from_rgb(220, 90, 90)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use plotyx_plot::{PanelSpec, Panels};

    /// Issue #34 : `framed_spec` rend le spec cadré par le zoom — ce que la
    /// figure montre — sur un clone ; le spec d'origine, celui du modèle,
    /// ne change pas. Sans zoom, le spec est rendu tel quel.
    #[test]
    fn framed_spec_applique_le_zoom_sur_un_clone() {
        let spec = PlotSpec::empty("Zoom").with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
        let mut view = GraphView::new();
        assert_eq!(view.framed_spec(&spec), spec, "sans zoom, rien ne change");

        view.view.set_framing(
            0,
            0,
            Framing {
                x: (2.0, 3.0),
                y: (-1.0, 1.0),
            },
        );
        let framed = view.framed_spec(&spec);
        let bottom = &framed.panels[0].bottom;
        assert_eq!((bottom.range_min, bottom.range_max), (Some(2.0), Some(3.0)));
        assert!(!bottom.auto_range);
        assert!(
            spec.panels[0].bottom.auto_range,
            "le spec d'origine a changé"
        );
    }
}
