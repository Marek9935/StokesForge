//! Rendu d'un graphe StokesForge à l'écran — phase R4, tâches 4.2 et 4.3.
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
//! les phases précédentes : `stokesforge_core::resolve` (R3), qui traduit un
//! `PlotSpec` en données prêtes à tracer en gérant les références brisées de
//! première main, et `stokesforge_session::Snapshot` (socle), dont la `revision`
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
//!   `stokesforge_render::text`). La mesure doit donc venir d'ailleurs.
//!
//! C'est le cas que la mesure R0 0.1 avait à trancher, et sa réponse est ce
//! qui rend ce découpage sûr : **0,000000 px d'écart** sur 540 comparaisons
//! entre une `Fonts` construite à la main et le chemin `Context`. Partager
//! les `FontDefinitions` suffit ; partager l'objet `Fonts` n'aurait pas été
//! possible de toute façon.
//!
//! Pour que ce ne soit pas une coïncidence, l'application doit poser
//! [`font_definitions`] sur son `Context` — `stokesforge-app` le fait au
//! démarrage.
//!
//! # Typst — tâche R8.3, branché par l'issue #29
//!
//! [`typst_screen`] rastérise une formule compilée par `stokesforge-render` en
//! RGBA (`typst-render`), la charge en texture `egui`, et
//! [`EguiBackend::draw_text`]/[`estimate_text_size`] l'utilisent à la place
//! du chemin `Shape::Text` pour tout texte riche — titres, titres d'axe,
//! légende, marqués par leur style ([`stokesforge_render::formula::is_rich`],
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
mod shape_tools;
pub mod typst_screen;
mod view;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use egui::{
    Color32, Galley, Id, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke, StrokeKind,
};
use stokesforge_core::node::NodeKind;
use stokesforge_core::resolve::{resolve_panel, resolve_spec, ResolvedPlot};
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{PlotSpec, SeriesAxis};
use stokesforge_render::formula::FormulaCache;
use stokesforge_render::pipeline::{self};
use stokesforge_render::series;
use stokesforge_session::Revision;
use stokesforge_session::Snapshot;

pub use backend::{EguiBackend, EguiBackendError};
pub use cache::{CacheKey, Outcome, ShapeCache};
pub use feathering::Feathering;
// Réexportés pour que l'UI n'ait pas à déclarer une arête vers
// `stokesforge-render` pour lire ce que la figure a tracé : ce crate est déjà sa
// façade de rendu écran.
pub use stokesforge_render::pipeline::{
    DrawnAnnotation, DrawnLegendRow, DrawnPanel, DrawnPlot, DrawnText, AXIS_DESC_SIZE,
    DEFAULT_FIGURE_SIZE, LABEL_SIZE, PANEL_TITLE_SIZE, TITLE_SIZE,
};
// L'adresse et le modèle d'un texte libre : l'application les manipule avec
// les signaux de la vue ([`GraphEvent`]).
pub use stokesforge_plot::{
    AnnotationRef, AnnotationSpec, ShapeKind, ShapeRef, ShapeSpec, TextAnchor, TextTarget,
};
pub use stokesforge_render::text::{
    font_definitions, font_definitions_for, TextMeasurer, TextSource,
};
// Le choix de police (embed du 2026-09-13, `stokesforge_render::fonts`) : une UI
// qui veut proposer Fira/Libertinus/Noto ou une police système n'a pas à
// déclarer sa propre arête vers `stokesforge-render` pour ça non plus.
pub use stokesforge_render::{
    available_families, FamilyEntry, FontChoice, FontSet, FontSource, ResolvedFonts,
};
// La correspondance pixel ↔ données (tâche R7) : `stokesforge-render` la porte
// (voir `stokesforge_render::interact`, générique sur le backend), ce crate ne
// fait que la consommer avec des types egui.
pub use stokesforge_render::{
    component_at, nearest_on_curve, AxisSide, Gesture, PanelTransform, PlotComponent,
};
pub use typst_screen::{formula_label, FormulaTexture, TypstScreen};
// Typst côté modèle de texte (compilation, échappement) : l'interface en a
// besoin pour vérifier ce qu'elle propose d'écrire (mémo de syntaxe des
// propriétés), sans déclarer d'arête vers `stokesforge-render`.
pub use stokesforge_render::formula;
pub use view::{Framing, ViewState};

/// Ce que la vue signale à l'application, à traduire en actions — textes
/// libres (issue #35). Relevé par [`GraphView::take_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum GraphEvent {
    /// Clic de l'outil Texte (issue #62) : poser un texte libre au point de
    /// données visé et ouvrir son éditeur. L'outil revient au pointeur.
    PlaceText {
        /// Indice du calque dans `PlotSpec::panels`.
        panel: usize,
        /// Abscisse de données.
        x: f64,
        /// Ordonnée de données.
        y: f64,
    },
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
    /// Un objet graphique a été tracé, déplacé ou redimensionné (issue #76) :
    /// sa nouvelle version, à valider en **une** modification du modèle
    /// (`target.index` égal au nombre d'objets du panneau : un ajout).
    /// L'aperçu reste affiché jusqu'à ce que le modèle change
    /// ([`GraphView::hold_preview`]).
    ShapeChanged {
        /// L'objet concerné.
        target: ShapeRef,
        /// Sa nouvelle version.
        shape: ShapeSpec,
    },
    /// Double-clic sur un titre : l'éditer sur place (issue #53).
    EditText(TextTarget),
    /// Double-clic sur une entrée de la légende : renommer la série du
    /// calque `panel` (issue #53).
    EditLegendEntry {
        /// Indice du calque dans `PlotSpec::panels`.
        panel: usize,
        /// Rang de la série dans `PanelSpec::series`.
        series: usize,
    },
    /// Double-clic sur un axe (ligne ou graduations) : ouvrir les
    /// paramètres de cet axe (issue #113).
    OpenAxisProperties {
        /// Indice du panneau dans la figure dessinée.
        panel: usize,
        /// Côté de l'axe.
        side: stokesforge_render::AxisSide,
    },
    /// Double-clic sur une courbe : ouvrir les paramètres de la série
    /// (issue #113).
    OpenSeriesProperties(stokesforge_plot::SeriesRef),
    /// Double-clic sur un objet graphique : ouvrir ses propriétés
    /// (issue #113).
    OpenShapeProperties(stokesforge_plot::ShapeRef),
    /// Un titre ou une légende a été glissé puis relâché : sa nouvelle
    /// ancre, à valider en **une** modification du modèle. L'aperçu reste
    /// affiché jusqu'à ce que le modèle change ([`GraphView::hold_preview`]).
    TextMoved {
        /// Le texte déplacé.
        target: TextTarget,
        /// Sa nouvelle ancre.
        anchor: TextAnchor,
    },
}

/// L'outil actif d'un graphe (issue #62), façon Origin. Un réglage de **vue**,
/// comme le cadrage : jamais dans le modèle, jamais annulable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GraphTool {
    /// Le défaut : glisser fait défiler la vue, les textes se saisissent et
    /// se déplacent, Ctrl+molette zoome.
    #[default]
    Pointer,
    /// Glisser trace un rectangle de zoom.
    ZoomBox,
    /// Un clic dans la zone de tracé pose un texte libre.
    Text,
    /// Glisser trace un rectangle (issue #76).
    Rectangle,
    /// Glisser trace une ellipse (issue #76).
    Ellipse,
    /// Glisser trace une ligne (issue #76).
    Line,
    /// Glisser trace une flèche, pointe au relâchement (issue #76).
    Arrow,
}

impl GraphTool {
    /// La sorte d'objet que l'outil trace, s'il en trace une.
    pub fn shape_kind(self) -> Option<ShapeKind> {
        match self {
            Self::Rectangle => Some(ShapeKind::Rectangle),
            Self::Ellipse => Some(ShapeKind::Ellipse),
            Self::Line => Some(ShapeKind::Line),
            Self::Arrow => Some(ShapeKind::Arrow),
            Self::Pointer | Self::ZoomBox | Self::Text => None,
        }
    }
}

/// Un glisser de titre ou de légende en cours (issue #53).
#[derive(Debug, Clone)]
struct TextDrag {
    target: TextTarget,
    /// L'ancre au début du geste (`None` : le texte était à sa place par
    /// défaut).
    original: Option<TextAnchor>,
    /// Centre tracé du texte au début du geste, en repère de la figure.
    start_center: Pos2,
    /// Point d'appui du geste, en repère de la figure.
    press: Pos2,
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
    /// Le geste tire une poignée de largeur (#244) au lieu de déplacer.
    resize: bool,
}

/// Distance maximale, en points egui, à laquelle une poignée de largeur est
/// saisie.
const WIDTH_HANDLE_RADIUS: f32 = 7.0;

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
            Self::UnknownNode(id) => write!(f, "no node {id:?} in this snapshot"),
            Self::NotAGraph(id) => write!(f, "node {id:?} is not a graph"),
            Self::Drawing(message) => write!(f, "drawing failed: {message}"),
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

/// Échantillon partagé avec la légende, pour Object Manager (#82).
pub fn series_swatch(tree: &Tree, spec: &stokesforge_plot::SeriesSpec, rect: Rect) -> Vec<Shape> {
    use plotters::drawing::IntoDrawingArea;
    // Vérifier les références par leurs métadonnées, sans convertir les
    // cellules mixtes ni allouer la géométrie de la série pour une vignette.
    let references = [
        Some(spec.y),
        spec.x.filter(|_| spec.plot_type.needs_x()),
        spec.x_error,
        spec.y_error,
    ];
    let broken = references
        .into_iter()
        .flatten()
        .any(|id| stokesforge_core::resolve::resolve_column(tree, id).is_none());
    let bars = !broken
        && matches!(
            spec.plot_type,
            stokesforge_plot::PlotType::Bar | stokesforge_plot::PlotType::Histogram
        );
    let mut shapes = Vec::new();
    static TEXT: std::sync::OnceLock<TextMeasurer> = std::sync::OnceLock::new();
    let text = TEXT.get_or_init(TextMeasurer::default);
    let area = EguiBackend::in_rect(&mut shapes, rect, text).into_drawing_area();
    let _ = stokesforge_render::legend::series_key(
        &area,
        spec,
        broken,
        bars,
        (0, rect.height() as i32 / 2),
        stokesforge_render::paint::INK,
    );
    drop(area);
    shapes
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
/// aujourd'hui n'en a l'usage (`stokesforge-app` n'affiche encore qu'un banc,
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
    /// d'écran. `None` hors glissé de l'outil Zoom.
    drag_box: Option<(Pos2, Pos2)>,
    /// L'outil actif (issue #62).
    tool: GraphTool,
    /// Réponse d'interaction de la figure au dernier rendu : l'application
    /// y accroche son menu contextuel (clic droit).
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
    /// Aperçu d'un titre ou d'une légende en cours de glisser (issue #53),
    /// sur le même principe que `annotation_preview`.
    text_preview: Option<(TextTarget, TextAnchor)>,
    text_drag: Option<TextDrag>,
    /// Texte libre sélectionné (clic), cible de Suppr.
    selected_annotation: Option<AnnotationRef>,
    /// Le titre, titre d'axe ou la légende sélectionné (issue #77).
    selected_text: Option<TextTarget>,
    /// Aperçu d'un objet graphique en cours de tracé, de déplacement ou de
    /// redimensionnement (issue #76), sur le même principe que
    /// `annotation_preview`.
    shape_preview: Option<(ShapeRef, ShapeSpec)>,
    shape_drag: Option<shape_tools::ShapeDrag>,
    /// Objet graphique sélectionné (clic), cible de Suppr et des poignées.
    selected_shape: Option<ShapeRef>,
    /// Série sélectionnée, commune au graphe et à Object Manager (#83).
    selected_series: Option<stokesforge_plot::SeriesRef>,
    /// Le groupe de séries sélectionné (#197), avec la série qui le désigne :
    /// il ne vaut que tant que `selected_series` est cette série, donc un clic
    /// sur une autre courbe le lève sans rien à nettoyer ailleurs.
    selected_group: Option<(stokesforge_plot::SeriesRef, u32)>,
    /// Où est parti le glisser de navigation en cours : décidé à la première
    /// image du geste, pour que le curseur puisse en sortir sans changer de
    /// cible.
    nav_drag: Option<NavDrag>,
    /// Calque actif (#161) : le dernier calque sélectionné, cible par défaut
    /// des ajouts. État d'interface, hors modèle ; jamais dessiné dans la
    /// figure. Lu par [`Self::active_panel`], qui le revalide.
    active_panel: usize,
    /// Sélection vue à l'image précédente : le calque actif suit un
    /// changement de sélection, pas la sélection elle-même (un clic sur un
    /// repère de calque reste donc valable tant que rien d'autre n'est
    /// sélectionné).
    seen_selection: SelectionKey,
    events: Vec<GraphEvent>,
}

/// La cible d'un glisser de navigation : toute la zone de tracé, ou une seule
/// échelle (glisser sur un axe, #167).
#[derive(Debug, Clone, Copy, PartialEq)]
enum NavDrag {
    Plot,
    Axis { panel: usize, side: AxisSide },
}

/// Les quatre sélections d'une vue, pour détecter qu'elles ont changé.
type SelectionKey = (
    Option<stokesforge_plot::SeriesRef>,
    Option<AnnotationRef>,
    Option<ShapeRef>,
    Option<TextTarget>,
);

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
            typst_fonts: stokesforge_render::formula::resolve_fonts(&font_choice).ok(),
            font_choice,
            scaled: scaled::ScaledShapes::default(),
            last: None,
            measurer: TextMeasurer::default(),
            view: ViewState::new(),
            id: Id::new((
                "stokesforge_render_egui::GraphView",
                NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            )),
            hover: None,
            display_rect: None,
            drag_box: None,
            tool: GraphTool::default(),
            response: None,
            annotation_preview: None,
            preview_generation: 0,
            preview_until: None,
            annotation_drag: None,
            text_preview: None,
            text_drag: None,
            selected_annotation: None,
            selected_text: None,
            shape_preview: None,
            shape_drag: None,
            selected_shape: None,
            selected_series: None,
            selected_group: None,
            nav_drag: None,
            active_panel: 0,
            seen_selection: (None, None, None, None),
            events: Vec::new(),
        }
    }

    /// Le calque actif de `spec` (#161) : le dernier sélectionné, ou le
    /// premier s'il a disparu. Toujours un indice valide tant que le graphe
    /// a un calque.
    pub fn active_panel(&self, spec: &PlotSpec) -> usize {
        if self.active_panel < spec.panels.len() {
            self.active_panel
        } else {
            0
        }
    }

    /// Rend le calque `panel` actif (sans effet s'il n'existe pas). Repère
    /// numéroté de la fenêtre, nœud du calque dans Object Manager.
    pub fn set_active_panel(&mut self, panel: usize, spec: &PlotSpec) {
        if panel < spec.panels.len() {
            self.active_panel = panel;
        }
    }

    /// Le calque de l'objet sélectionné, s'il y en a un.
    fn selection_panel(&self, spec: &PlotSpec) -> Option<usize> {
        if let Some(series) = self.selected_series {
            return Some(series.panel);
        }
        let position = self
            .selected_annotation
            .map(|t| t.panel)
            .or_else(|| self.selected_shape.map(|t| t.panel))
            .or_else(|| self.selected_text.and_then(|t| t.panel_index_of()))?;
        (position < spec.panels.len()).then_some(position)
    }

    /// Le calque actif suit la sélection quand elle change.
    fn follow_selection(&mut self, spec: &PlotSpec) {
        let key = (
            self.selected_series,
            self.selected_annotation,
            self.selected_shape,
            self.selected_text,
        );
        if key != self.seen_selection {
            self.seen_selection = key;
            if let Some(panel) = self.selection_panel(spec) {
                self.active_panel = panel;
            }
        }
    }

    fn paint_series_selection(&self, painter: &egui::Painter, spec: &PlotSpec) {
        let Some(selected) = self.selected_series else {
            return;
        };
        // Un groupe sélectionné (#197) montre les poignées de tous ses membres.
        let members = self.selected_group().and_then(|group| {
            spec.panels
                .get(group.panel)
                .map(|panel| panel.group_members(group.group))
        });
        let targets = match members {
            Some(members) => members
                .into_iter()
                .map(|index| stokesforge_plot::SeriesRef::new(selected.panel, index))
                .collect(),
            None => vec![selected],
        };
        for target in targets {
            self.paint_series_handles(painter, spec, target);
        }
    }

    fn paint_series_handles(
        &self,
        painter: &egui::Painter,
        spec: &PlotSpec,
        target: stokesforge_plot::SeriesRef,
    ) {
        if target.panel >= spec.panels.len() {
            return;
        }
        let Some(panel) = self
            .last
            .as_ref()
            .and_then(|plot| plot.panels.iter().find(|p| p.index == target.panel))
        else {
            return;
        };
        let Some(series) = panel
            .series_geometry
            .iter()
            .find(|s| s.index == target.index)
        else {
            return;
        };
        let Some(rect) = self.display_rect else {
            return;
        };
        let Some(scale) = self.display_scale() else {
            return;
        };
        // Poignées de sélection (#98) : placées et peintes par
        // `stokesforge_render::series_selection`, en pixels d'affichage pour
        // que leur taille et leur espacement ne suivent pas l'agrandissement
        // de la figure. Peintes par-dessus la figure en cache, jamais dedans :
        // sélectionner ne reconstruit rien, et l'export n'en sait rien.
        let scale = f64::from(scale);
        let centers = series.handle_centers(|(x, y)| (x * scale, y * scale));
        let mut shapes = Vec::with_capacity(centers.len());
        let size = (rect.width().max(1.0) as u32, rect.height().max(1.0) as u32);
        let mut backend = EguiBackend::new(&mut shapes, rect.min, size, &self.measurer);
        // `draw_rect` de ce backend n'échoue jamais (`EguiBackendError` est
        // vide, et un carré ne passe pas par les polices).
        let _ = stokesforge_render::series_selection::paint_handles(
            &mut backend,
            &centers,
            stokesforge_render::pipeline::figure_ink(spec),
        );
        painter.extend(shapes);
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
    /// texte libre, `stokesforge-app`).
    pub fn id(&self) -> Id {
        self.id
    }

    /// Les signaux de la vue depuis le dernier relevé (issue #35).
    pub fn take_events(&mut self) -> Vec<GraphEvent> {
        std::mem::take(&mut self.events)
    }

    /// La série sélectionnée, visible ou masquée (#83).
    pub fn selected_series(&self) -> Option<stokesforge_plot::SeriesRef> {
        self.selected_series
    }

    /// Sélection d'interface : elle ne change ni le modèle, ni son historique.
    pub fn select_series(&mut self, target: Option<stokesforge_plot::SeriesRef>, spec: &PlotSpec) {
        self.selected_group = None;
        self.selected_series = target.filter(|target| spec.series_at(*target).is_some());
        if self.selected_series.is_some() {
            self.selected_annotation = None;
            self.selected_shape = None;
            self.selected_text = None;
            self.tool = GraphTool::Pointer;
        }
    }

    /// Le groupe de séries sélectionné (#197), s'il y en a un.
    pub fn selected_group(&self) -> Option<stokesforge_plot::SeriesGroupRef> {
        let (series, group) = self.selected_group?;
        (self.selected_series == Some(series))
            .then(|| stokesforge_plot::SeriesGroupRef::new(series.panel, group))
    }

    /// Sélectionne un groupe de séries depuis Object Manager (#197) : la série
    /// qui le désigne est la première de ses membres.
    pub fn select_series_group(
        &mut self,
        target: Option<stokesforge_plot::SeriesGroupRef>,
        spec: &PlotSpec,
    ) {
        let first = target.and_then(|group| {
            let index = *spec
                .panels
                .get(group.panel)?
                .group_members(group.group)
                .first()?;
            Some((
                stokesforge_plot::SeriesRef::new(group.panel, index),
                group.group,
            ))
        });
        self.select_series(first.map(|(series, _)| series), spec);
        if self.selected_series.is_some() {
            self.selected_group = first;
        }
    }

    /// Sélection d'un texte libre depuis Object Manager, y compris masqué.
    pub fn select_annotation(&mut self, target: Option<AnnotationRef>, spec: &PlotSpec) {
        self.select_series(None, spec);
        self.selected_shape = None;
        self.selected_text = None;
        self.selected_annotation = target.filter(|t| t.get(spec).is_some());
        self.tool = GraphTool::Pointer;
    }

    /// Sélection d'un titre ou d'une légende depuis Object Manager.
    pub fn select_text(&mut self, target: Option<TextTarget>, spec: &PlotSpec) {
        self.select_series(None, spec);
        self.selected_shape = None;
        self.selected_annotation = None;
        self.selected_text = target.filter(|t| t.visible(spec).is_some());
        self.tool = GraphTool::Pointer;
    }

    /// Série sous le pointeur, par sa géométrie réellement dessinée.
    pub fn series_at(&self, pos: Pos2, spec: &PlotSpec) -> Option<stokesforge_plot::SeriesRef> {
        let pos = self.figure_position(pos)?;
        let last = self.last.as_ref()?;
        let (panel, index) =
            stokesforge_render::series_selection::series_at(last, (pos.x as f64, pos.y as f64))?;
        (panel < spec.panels.len()).then(|| stokesforge_plot::SeriesRef::new(panel, index))
    }

    /// Le texte libre sélectionné, s'il y en a un.
    pub fn selected_annotation(&self) -> Option<AnnotationRef> {
        self.selected_annotation
    }

    /// Le titre, titre d'axe ou la légende sélectionné, s'il y en a un
    /// (issue #77) : un clic le sélectionne, le glisser le déplace, la barre
    /// de format règle sa police et son corps.
    pub fn selected_text(&self) -> Option<TextTarget> {
        self.selected_text
    }

    /// Le titre ou la légende sous le pixel egui `pos`. Une entrée de la
    /// légende désigne la légende entière : on ne sélectionne ni ne déplace
    /// une ligne à part.
    pub fn text_target_at(&self, pos: Pos2) -> Option<TextTarget> {
        match self.component_at(pos)? {
            PlotComponent::Text(target) => Some(target),
            PlotComponent::LegendEntry { panel, .. } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(TextTarget::Legend { panel: drawn.index })
            }
            _ => None,
        }
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
            None => self.clear_previews(),
        }
    }

    /// Efface les aperçus (texte libre et titre déplacé).
    fn clear_previews(&mut self) {
        self.set_annotation_preview(None);
        self.set_text_preview(None);
        self.set_shape_preview(None);
    }

    /// L'aperçu d'un titre ou d'une légende déplacé, s'il y en a un.
    pub fn text_preview(&self) -> Option<&(TextTarget, TextAnchor)> {
        self.text_preview.as_ref()
    }

    /// Pose (ou retire) l'aperçu d'un titre ou d'une légende déplacé : la
    /// figure le montre à la prochaine image, sans toucher au modèle.
    pub fn set_text_preview(&mut self, preview: Option<(TextTarget, TextAnchor)>) {
        self.preview_until = None;
        if self.text_preview != preview {
            self.text_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// Le texte libre sous le pixel egui `pos`, cherché dans la dernière
    /// figure tracée.
    pub fn annotation_at(&self, pos: Pos2) -> Option<AnnotationRef> {
        match self.component_at(pos)? {
            PlotComponent::Annotation { panel, index } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(AnnotationRef {
                    panel: drawn.index,
                    index,
                })
            }
            _ => None,
        }
    }

    /// Où un titre ou la légende est tracé à l'écran : centre, taille du
    /// texte droit et rotation (degrés, antihoraire) — pour poser l'éditeur
    /// sur place (issue #53).
    pub fn text_screen_box(&self, target: TextTarget) -> Option<(Pos2, egui::Vec2, f32)> {
        let drawn = self.drawn_text(target)?;
        let (center, size) = self.figure_box_to_screen(drawn.center, drawn.size)?;
        Some((center, size, drawn.rotation as f32))
    }

    /// Où la ligne de légende de la série `series` (rang dans
    /// `PanelSpec::series`) du calque `panel` est tracée à l'écran :
    /// centre et taille de la ligne, sur toute la largeur du cadre.
    pub fn legend_entry_screen_box(
        &self,
        panel: usize,
        series: usize,
    ) -> Option<(Pos2, egui::Vec2)> {
        let panel = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|drawn| drawn.index == panel)?;
        let frame = panel
            .texts
            .iter()
            .find(|text| matches!(text.target, TextTarget::Legend { .. }))?;
        let line = panel
            .legend_rows
            .iter()
            .find(|line| line.series == series)?;
        self.figure_box_to_screen(
            (frame.center.0, line.center_y),
            (frame.size.0, line.height.round() as u32),
        )
    }

    fn figure_box_to_screen(
        &self,
        center: (f64, f64),
        size: (u32, u32),
    ) -> Option<(Pos2, egui::Vec2)> {
        let display = self.display_rect?;
        let scale = self.display_scale()?;
        Some((
            display.min + egui::vec2(center.0 as f32, center.1 as f32) * scale,
            egui::vec2(size.0 as f32, size.1 as f32) * scale,
        ))
    }

    /// Le titre ou la légende tracé pour `target`, dans la dernière figure.
    fn drawn_text(&self, target: TextTarget) -> Option<&DrawnText> {
        let last = self.last.as_ref()?;
        match target.panel_index_of() {
            None => last.title.as_ref(),
            Some(index) => last
                .panels
                .iter()
                .find(|panel| panel.index == index)?
                .texts
                .iter()
                .find(|text| text.target == target),
        }
    }

    /// L'outil actif.
    pub fn tool(&self) -> GraphTool {
        self.tool
    }

    /// Change d'outil ; un geste de rectangle en cours est abandonné.
    pub fn set_tool(&mut self, tool: GraphTool) {
        if self.tool != tool {
            self.tool = tool;
            self.drag_box = None;
            self.abort_shape_drag();
        }
    }

    /// Le point de données sous le pixel egui `pos` (ordonnée de gauche), et
    /// l'indice de son calque.
    pub fn data_at(&self, pos: Pos2) -> Option<(usize, f64, f64)> {
        let (index, transform) = self.panel_at(pos)?;
        let figure = self.figure_position(pos)?;
        let (x, y) = transform.pixel_to_data(figure.x as f64, figure.y as f64)?;
        let panel = self.last.as_ref()?.panels.get(index)?;
        Some((panel.index, x, y))
    }

    /// Le pixel egui d'un point de données `(x, y)` du calque `panel`
    /// (#199) — réciproque de [`Self::data_at`] — et le rectangle de sa zone
    /// de tracé à l'écran. Cherché dans la **dernière** figure tracée.
    pub fn data_to_screen(&self, panel: usize, x: f64, y: f64) -> Option<Pos2> {
        let (transform, display, scale) = self.panel_screen(panel)?;
        let (px, py) = transform.data_to_pixel(x, y)?;
        Some(display.min + egui::vec2(px as f32, py as f32) * scale)
    }

    /// La zone de tracé du calque `panel` à l'écran (#199).
    pub fn plot_rect_screen(&self, panel: usize) -> Option<Rect> {
        let drawn = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|p| p.index == panel)?;
        let (display, scale) = (self.display_rect?, self.display_scale()?);
        let min = display.min
            + egui::vec2(drawn.pixel_origin.0 as f32, drawn.pixel_origin.1 as f32) * scale;
        let size = egui::vec2(drawn.pixel_size.0 as f32, drawn.pixel_size.1 as f32) * scale;
        Some(Rect::from_min_size(min, size))
    }

    fn panel_screen(&self, panel: usize) -> Option<(PanelTransform, Rect, f32)> {
        let drawn = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|p| p.index == panel)?;
        Some((
            PanelTransform::of(drawn)?,
            self.display_rect?,
            self.display_scale()?,
        ))
    }

    /// Le point de données au centre de la zone de tracé du calque actif
    /// (#161), ou du premier panneau tracé si celui-ci ne l'est pas — où
    /// poser un texte ajouté par la barre de menus.
    pub fn panel_center_data(&self, spec: &PlotSpec) -> Option<(usize, f64, f64)> {
        let last = self.last.as_ref()?;
        let active_index = self.active_panel(spec);
        let panel = last
            .panels
            .iter()
            .find(|p| p.index == active_index)
            .or_else(|| last.panels.first())?;
        let transform = PanelTransform::of(panel)?;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let (x, y) = transform.pixel_to_data(center.0, center.1)?;
        Some((panel.index, x, y))
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

    /// Les deux poignées de largeur du texte libre `target`, au milieu de
    /// ses bords gauche et droit (repère tourné), en points egui (#244).
    fn width_handles(&self, target: AnnotationRef) -> Option<[Pos2; 2]> {
        let (center, size, rotation) = self.annotation_screen_box(target)?;
        let (sin, cos) = rotation.to_radians().sin_cos();
        let half = size.x / 2.0 + 3.0;
        let axis = egui::vec2(cos, -sin);
        Some([center - axis * half, center + axis * half])
    }

    /// Vrai si `pos` (points egui) tombe sur une poignée de largeur.
    fn width_handle_at(&self, target: AnnotationRef, pos: Pos2) -> bool {
        self.width_handles(target).is_some_and(|handles| {
            handles
                .iter()
                .any(|handle| handle.distance(pos) <= WIDTH_HANDLE_RADIUS)
        })
    }

    fn drawn_annotation(&self, target: AnnotationRef) -> Option<&DrawnAnnotation> {
        self.last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.index == target.panel)?
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
        // tant qu'un seul graphe est affiché à la fois (`stokesforge-app`
        // aujourd'hui), à revoir si plusieurs vues cohabitent un jour.
        feathering::apply(&context, Feathering::during_drag(response.dragged()));

        // Un aperçu validé s'efface dès que le modèle l'a rattrapé.
        if self
            .preview_until
            .is_some_and(|revision| revision != snapshot.revision)
        {
            self.clear_previews();
        }

        self.selected_series = self
            .selected_series
            .filter(|target| spec.series_at(*target).is_some());
        self.selected_annotation = self.selected_annotation.filter(|t| t.get(&spec).is_some());
        self.selected_shape = self.selected_shape.filter(|t| t.get(&spec).is_some());
        self.selected_text = self.selected_text.filter(|t| t.visible(&spec).is_some());
        self.follow_selection(&spec);
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
        let text_preview = &self.text_preview;
        let shape_preview = &self.shape_preview;
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
            if let Some((target, shape)) = shape_preview {
                target.set(&mut framed, shape.clone());
            }
            if let Some((target, anchor)) = text_preview {
                target.set_anchor(&mut framed, Some(*anchor));
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

    /// Le rapport entre un pixel de la figure et un point egui à l'écran
    /// (1 tant qu'elle n'est ni agrandie ni réduite). `None` avant la
    /// première figure tracée.
    pub fn display_scale(&self) -> Option<f32> {
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
        let hit = |(index, panel): (usize, &DrawnPanel)| {
            let transform = PanelTransform::of(panel)?;
            transform
                .contains(pos.x as f64, pos.y as f64)
                .then_some((index, transform))
        };
        // Des calques superposés se recouvrent : le calque actif prend le
        // geste (#161), à défaut le premier tracé.
        last.panels
            .iter()
            .enumerate()
            .filter(|(_, panel)| panel.index == self.active_panel)
            .find_map(hit)
            .or_else(|| last.panels.iter().enumerate().find_map(hit))
    }

    /// Applique un geste de cadrage (zoom, glisser, rectangle) à **toutes les
    /// échelles propres de tous les calques de la case** du calque
    /// `panel_index` (dans [`DrawnPlot::panels`]) (#167). Les calques d'une
    /// case partagent la même zone de tracé : la même transformation en
    /// pixels garde donc leurs graduations alignées, y compris celles d'un
    /// axe secondaire. Une échelle liée à un autre calque n'a pas de cadrage
    /// à elle et suit celle qu'elle recouvre.
    fn commit_gesture(&mut self, panel_index: usize, gesture: Gesture) {
        let Some(last) = &self.last else { return };
        let Some(hit) = last.panels.get(panel_index) else {
            return;
        };
        let cell = (hit.row, hit.column);
        let mut updates = Vec::new();
        for panel in last.panels.iter().filter(|p| (p.row, p.column) == cell) {
            let (own_x, own_y) = match panel.overlay {
                Some(overlay) => (overlay.x_top, overlay.y_right),
                None => (true, true),
            };
            let mut framing = Framing::default();
            if let Some((x, y)) = PanelTransform::of(panel).and_then(|t| t.apply(&gesture)) {
                framing.x = own_x.then_some(x);
                framing.y = own_y.then_some(y);
            }
            if panel.overlay.is_none() && panel.y2_range.is_some() {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
            updates.push((panel.index, framing));
        }
        for (index, framing) in updates {
            self.view.set_framing(index, framing);
        }
    }

    /// L'axe (ligne et graduations) sous le pixel egui `pos`, par l'indice du
    /// panneau dans [`DrawnPlot::panels`] et le côté **du spec**.
    fn axis_at(&self, pos: Pos2) -> Option<(usize, AxisSide)> {
        match self.component_at(pos)? {
            PlotComponent::Axis { panel, side } => Some((panel, side)),
            _ => None,
        }
    }

    /// Applique un geste à **une seule échelle** (#167) : celle de l'axe
    /// `side` du panneau `panel_index` (dans [`DrawnPlot::panels`]). Les
    /// autres échelles, liées ou non, ne bougent pas. Le geste n'agit que
    /// selon l'axe : l'autre composante est neutralisée au centre de la zone
    /// de tracé.
    fn commit_axis_gesture(&mut self, panel_index: usize, side: AxisSide, gesture: Gesture) {
        let Some(panel) = self.last.as_ref().and_then(|l| l.panels.get(panel_index)) else {
            return;
        };
        let horizontal = side == AxisSide::Bottom;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let gesture = match gesture {
            Gesture::Zoom { anchor, factor } => Gesture::Zoom {
                anchor: if horizontal {
                    (anchor.0, center.1)
                } else {
                    (center.0, anchor.1)
                },
                factor,
            },
            Gesture::Pan { dx, dy } => {
                if horizontal {
                    Gesture::Pan { dx, dy: 0.0 }
                } else {
                    Gesture::Pan { dx: 0.0, dy }
                }
            }
            other => other,
        };
        let mut framing = Framing::default();
        match side {
            AxisSide::Bottom => {
                framing.x = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(x, _)| x);
            }
            AxisSide::Left => {
                framing.y = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y)| y);
            }
            AxisSide::Right => {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
        }
        let index = panel.index;
        self.view.set_framing(index, framing);
    }

    /// Ctrl+molette (zoom), glissé principal (pan avec l'outil Pointeur,
    /// rectangle de zoom élastique avec l'outil Zoom), clic de l'outil Texte
    /// et double-clic (réinitialisation) — tâches 7.1 et 7.4, issue #62. Ne fait rien tant qu'aucune figure n'a encore été tracée.
    fn handle_navigation(&mut self, response: &Response, context: &egui::Context, spec: &PlotSpec) {
        if self.last.is_none() {
            return;
        }

        // Issue #35 : un clic choisit le texte libre sous le pointeur (ou
        // aucun), un double-clic sur un texte l'édite au lieu de
        // réinitialiser le zoom.
        let pointer = response.interact_pointer_pos();
        // Un geste commencé dans la zone de tracé d'un calque (clic, clic
        // droit, début de glisser d'un outil) l'active (#161), même sans
        // objet visé.
        if response.clicked() || response.secondary_clicked() || response.drag_started() {
            let hit = pointer
                .and_then(|pos| self.panel_at(pos))
                .and_then(|(i, _)| Some(self.last.as_ref()?.panels.get(i)?.index));
            if let Some(panel) = hit {
                self.set_active_panel(panel, spec);
            }
        }
        if response.clicked() {
            self.selected_series = None;
            self.selected_annotation = pointer.and_then(|pos| self.annotation_at(pos));
            self.selected_shape = match self.selected_annotation {
                Some(_) => None,
                None => pointer.and_then(|pos| self.shape_at(pos)),
            };
            // Issue #77 : un titre ou la légende se sélectionne au clic,
            // quand ni un texte libre ni un objet n'est visé.
            self.selected_text = match (self.selected_annotation, self.selected_shape) {
                (None, None) => pointer.and_then(|pos| self.text_target_at(pos)),
                _ => None,
            };
            if self.tool == GraphTool::Pointer
                && self.selected_annotation.is_none()
                && self.selected_shape.is_none()
                && self.selected_text.is_none()
            {
                self.selected_series = pointer.and_then(|pos| self.series_at(pos, spec));
            }
        }
        if response.double_clicked() {
            let component = pointer.and_then(|pos| self.component_at(pos));
            if let Some(PlotComponent::Text(target)) = component {
                self.selected_text = Some(target);
            }
            match (pointer.and_then(|pos| self.annotation_at(pos)), component) {
                (Some(target), _) => {
                    self.selected_annotation = Some(target);
                    self.events.push(GraphEvent::EditAnnotation(target));
                }
                // Issue #53 : un titre s'édite sur place, comme un texte libre.
                (None, Some(PlotComponent::Text(target))) => {
                    self.events.push(match target {
                        // La légende n'a pas de texte à elle : ce sont les
                        // noms des séries, un par ligne.
                        TextTarget::Legend { .. } => return,
                        _ => GraphEvent::EditText(target),
                    });
                }
                (None, Some(PlotComponent::LegendEntry { panel, series })) => {
                    if let Some(drawn) = self.last.as_ref().and_then(|last| last.panels.get(panel))
                    {
                        self.events.push(GraphEvent::EditLegendEntry {
                            panel: drawn.index,
                            series,
                        });
                    }
                }
                // Issue #113 : sur un objet, une courbe ou un axe, le
                // double-clic ouvre leurs paramètres. Ailleurs (zone de
                // tracé nue, marges), il réinitialise toujours le zoom.
                _ => {
                    let shape = pointer.and_then(|pos| self.shape_at(pos));
                    let series = pointer.and_then(|pos| self.series_at(pos, spec));
                    if let Some(shape) = shape {
                        self.events.push(GraphEvent::OpenShapeProperties(shape));
                    } else if let Some(series) = series {
                        self.selected_series = Some(series);
                        self.events.push(GraphEvent::OpenSeriesProperties(series));
                    } else if let Some(PlotComponent::Axis { panel, side }) = component {
                        self.events
                            .push(GraphEvent::OpenAxisProperties { panel, side });
                    } else {
                        self.view.reset();
                    }
                }
            }
        }
        if self.tool == GraphTool::Pointer
            && (self.handle_annotation_drag(response, spec)
                || self.handle_shape_drag(response, spec)
                || self.handle_text_drag(response, spec))
        {
            return;
        }
        self.handle_shape_draw(response, spec);

        // Outil Texte : un clic dans la zone de tracé pose le texte.
        if self.tool == GraphTool::Text && response.clicked() {
            if let Some((panel, x, y)) = pointer.and_then(|pos| self.data_at(pos)) {
                self.events.push(GraphEvent::PlaceText { panel, x, y });
            }
        }

        if response.hovered() {
            if let Some(pos) = response
                .hover_pos()
                .filter(|pos| response.interact_rect.contains(*pos))
                .and_then(|pos| self.figure_position(pos).map(|position| (pos, position)))
            {
                // Ctrl+molette (ou pincement) : egui range ce geste dans
                // `zoom_delta`, un facteur multiplicatif (> 1 : vers l'avant,
                // on zoome). La molette seule ne zoome plus (issue #62).
                // Le facteur de cadrage est l'inverse, borné pour qu'un
                // défilement continu (pavé tactile) ne fasse pas s'effondrer
                // le cadrage d'un coup.
                let zoom = context.input(|input| input.zoom_delta());
                if zoom != 1.0 {
                    let factor = (1.0 / zoom as f64).clamp(0.5, 2.0);
                    let gesture = Gesture::Zoom {
                        anchor: (pos.1.x as f64, pos.1.y as f64),
                        factor,
                    };
                    if let Some((panel_index, side)) = self.axis_at(pos.0) {
                        // Au-dessus d'un axe, le zoom ne cadre que cette échelle.
                        self.commit_axis_gesture(panel_index, side, gesture);
                    } else if let Some((panel_index, _)) = self.panel_at(pos.0) {
                        self.commit_gesture(panel_index, gesture);
                    }
                }
            }
        }

        if !response.dragged_by(PointerButton::Primary) {
            self.nav_drag = None;
        }
        if self.tool == GraphTool::Pointer && response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if let Some(pos) = response.interact_pointer_pos() {
                // La cible est celle du point de départ du geste, décidée à
                // la première image : une fois qu'on glisse, le curseur peut
                // sortir de la zone de tracé (paner jusqu'au bord d'un
                // panneau voisin) ou de l'axe sans que la cible ne change en
                // cours de route. Un glisser parti d'un axe ne déplace que
                // cette échelle (#167).
                let origin = pos - delta;
                let target = match self.nav_drag {
                    Some(target) => target,
                    None => {
                        let target = match self.axis_at(origin) {
                            Some((panel, side)) => NavDrag::Axis { panel, side },
                            None => NavDrag::Plot,
                        };
                        self.nav_drag = Some(target);
                        target
                    }
                };
                let scale = self.display_scale().unwrap_or(1.0);
                let pan = Gesture::Pan {
                    dx: (delta.x / scale) as f64,
                    dy: (delta.y / scale) as f64,
                };
                match target {
                    NavDrag::Axis { panel, side } => self.commit_axis_gesture(panel, side, pan),
                    NavDrag::Plot => {
                        if let Some((panel_index, _)) =
                            self.panel_at(origin).or_else(|| self.panel_at(pos))
                        {
                            self.commit_gesture(panel_index, pan);
                        }
                    }
                }
            }
        }

        if self.tool == GraphTool::ZoomBox && response.dragged_by(PointerButton::Primary) {
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
        if self.tool == GraphTool::ZoomBox && response.drag_stopped_by(PointerButton::Primary) {
            if let Some((start, end)) = self.drag_box.take() {
                // En dessous de quelques pixels, c'est un tremblement de
                // main, pas un rectangle : un rectangle de zoom d'un pixel
                // de large produirait un cadrage quasi dégénéré, que le
                // pipeline redresserait silencieusement en revenant aux
                // données (`pipeline::axis_range`) — un comportement correct
                // mais surprenant pour un geste qui se voulait un zoom.
                if start.distance(end) > 4.0 {
                    if let Some((panel_index, _)) = self.panel_at(start) {
                        if let (Some(start), Some(end)) =
                            (self.figure_position(start), self.figure_position(end))
                        {
                            self.commit_gesture(
                                panel_index,
                                Gesture::Box {
                                    a: (start.x as f64, start.y as f64),
                                    b: (end.x as f64, end.y as f64),
                                },
                            );
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
            let handle = self
                .selected_annotation
                .filter(|target| self.width_handle_at(*target, origin));
            let Some(target) = handle.or_else(|| self.annotation_at(origin)) else {
                return false;
            };
            let (Some(original), Some(drawn), Some(press)) = (
                target.get(spec).cloned(),
                self.drawn_annotation(target).copied(),
                self.figure_position(origin),
            ) else {
                return false;
            };
            self.selected_series = None;
            self.selected_annotation = Some(target);
            self.selected_text = None;
            self.selected_shape = None;
            self.annotation_drag = Some(AnnotationDrag {
                target,
                original,
                start_center: Pos2::new(drawn.center.0 as f32, drawn.center.1 as f32),
                press,
                resize: handle.is_some(),
            });
        }
        let Some(drag) = self.annotation_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        if drag.resize {
            // La poignée tirée fixe la demi-largeur : sa distance au centre,
            // mesurée le long du texte (tourné), en fraction de la figure.
            let Some(figure) = self.cache.key().map(|key| key.rect.width()) else {
                return true;
            };
            let (sin, cos) = drag.original.effective_rotation().to_radians().sin_cos();
            let along = (current - drag.start_center).dot(egui::vec2(cos as f32, -sin as f32));
            let fraction =
                f64::from(2.0 * along.abs() / figure).clamp(AnnotationSpec::MIN_WIDTH, 1.0);
            self.set_annotation_preview(Some((
                drag.target,
                AnnotationSpec {
                    width: Some(fraction),
                    ..drag.original
                },
            )));
            return true;
        }
        let center = drag.start_center + (current - drag.press);
        let right = drag.original.axis == SeriesAxis::Right;
        let moved = self
            .last
            .as_ref()
            .and_then(|last| {
                last.panels
                    .iter()
                    .find(|panel| panel.index == drag.target.panel)
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

    /// Glisser un titre ou la légende (issue #53) : un glisser principal
    /// **parti d'un titre** le déplace au lieu de faire défiler la vue.
    /// L'aperçu suit le pointeur ; au relâchement, un seul
    /// [`GraphEvent::TextMoved`]. Même principe que
    /// [`Self::handle_annotation_drag`] — le centre du texte suit le
    /// pointeur, converti en données du panneau (donc ancré aux données, il
    /// suit ensuite le zoom).
    fn handle_text_drag(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(drag) = self.text_drag.take() {
                match self.text_preview {
                    Some((target, anchor)) if drag.original != Some(anchor) => {
                        self.events.push(GraphEvent::TextMoved { target, anchor });
                        self.hold_preview();
                    }
                    _ => self.set_text_preview(None),
                }
                return true;
            }
            return false;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.text_drag.is_some();
        };
        if self.text_drag.is_none() {
            let origin = pos - response.drag_delta();
            let Some(target) = self.text_target_at(origin) else {
                return false;
            };
            self.selected_series = None;
            self.selected_text = Some(target);
            self.selected_annotation = None;
            self.selected_shape = None;
            let (Some(drawn), Some(press)) = (
                self.drawn_text(target).copied(),
                self.figure_position(origin),
            ) else {
                return false;
            };
            self.text_drag = Some(TextDrag {
                target,
                original: target.anchor(spec),
                start_center: Pos2::new(drawn.center.0 as f32, drawn.center.1 as f32),
                press,
            });
        }
        let Some(drag) = self.text_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        let center = drag.start_center + (current - drag.press);
        // Le repère de l'ancre : le panneau du texte, ou le premier panneau
        // tracé pour le titre du graphe ; l'ordonnée de droite si l'ancre
        // existante y était déjà.
        let right = drag
            .original
            .is_some_and(|anchor| anchor.axis == SeriesAxis::Right);
        let panel = self
            .last
            .as_ref()
            .and_then(|last| match drag.target.panel_index_of() {
                None => last.panels.first(),
                Some(index) => last.panels.iter().find(|panel| panel.index == index),
            });
        let moved = panel
            .and_then(|panel| PanelTransform::of_axis(panel, right))
            .and_then(|transform| transform.pixel_to_data(center.x as f64, center.y as f64));
        if let Some((x, y)) = moved {
            let axis = if right {
                SeriesAxis::Right
            } else {
                SeriesAxis::Left
            };
            self.set_text_preview(Some((drag.target, TextAnchor { x, y, axis })));
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
            // Les poignées de largeur (#244), aux milieux des bords gauche
            // et droit.
            for handle in self
                .selected_annotation
                .and_then(|target| self.width_handles(target))
                .into_iter()
                .flatten()
            {
                let square = Rect::from_center_size(handle, egui::vec2(7.0, 7.0));
                painter.rect_filled(square, 1.0, Color32::WHITE);
                painter.rect_stroke(
                    square,
                    1.0,
                    Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
                    StrokeKind::Inside,
                );
            }
        }

        if let Some((center, size, rotation)) = self
            .selected_text
            .and_then(|target| self.text_screen_box(target))
        {
            let (sin, cos) = rotation.to_radians().sin_cos();
            let half = size / 2.0 + egui::vec2(3.0, 3.0);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
                let (u, v) = (sx * half.x, sy * half.y);
                center + egui::vec2(u * cos + v * sin, -u * sin + v * cos)
            });
            painter.add(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
            ));
        }

        self.paint_shape_selection(painter);
        self.paint_series_selection(painter, spec);

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
/// `stokesforge_render::nearest_on_curve`, réutilisé série par série).
///
/// Recalcule la géométrie plutôt que de la garder en mémoire : une série
/// résolue ne porte que des `Arc<Vec<f64>>` clonés par compteur
/// (`stokesforge-core::resolve`), donc le refaire à chaque survol coûte un
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
    let spec_panel = spec.panels.get(drawn_panel.index)?;
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

/// Le champ de saisie d'un texte libre en cours d'édition (issue #37) : où
/// il se pose et à quelle taille d'écriture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnotationField {
    /// Le rectangle du champ, à l'écran. Toujours **droit** : la rotation du
    /// texte n'y entre pas (voir [`annotation_field`]).
    pub rect: egui::Rect,
    /// La taille de police du champ, en points egui — celle du texte rendu.
    pub font_size: f32,
}

/// Réserve de largeur d'un champ à l'écran, en multiples de son corps, pour
/// qu'un texte très court (ou vide) laisse place à la saisie.
const FIELD_MIN_WIDTH_EM: f32 = 6.0;
/// Marge autour du texte, en multiples du corps : la hauteur d'une ligne de
/// saisie, un peu plus que le texte seul.
const FIELD_PADDING_EM: f32 = 0.3;

/// Où poser le champ de saisie d'un texte libre, pour qu'on écrive **dans le
/// cadre du rendu** (issue #37).
///
/// - **centré** sur `center`, le centre du texte rendu ;
/// - à la **taille de police** du texte : son corps (`annotation_size`, au
///   sens de Plotters, comme `AnnotationSpec::effective_size`) ramené au
///   corps d'epaint, fois `scale`, l'échelle d'affichage de la figure ;
/// - **sans rotation** : le champ reste droit, quelle que soit celle du
///   texte, donc rien ici ne la prend en paramètre ;
/// - au moins aussi grand que le texte droit rendu (`text_size`, à l'écran),
///   plus une marge et une largeur minimale pour qu'un texte vide reste
///   saisissable.
///
/// Fonction pure : la taille finale diffère un peu après recompilation par
/// Typst (une formule `$…$` se compose autrement que sa source), le champ
/// n'est pas censé la prédire.
pub fn annotation_field(
    center: Pos2,
    text_size: egui::Vec2,
    annotation_size: f64,
    scale: f32,
) -> AnnotationField {
    let font_size = stokesforge_render::text::font_size(annotation_size) * scale;
    let padding = font_size * FIELD_PADDING_EM;
    let size = egui::vec2(
        (text_size.x + 2.0 * padding).max(font_size * FIELD_MIN_WIDTH_EM),
        (text_size.y + 2.0 * padding).max(font_size + 2.0 * padding),
    );
    AnnotationField {
        rect: egui::Rect::from_center_size(center, size),
        font_size,
    }
}

/// Le `PlotSpec` d'un nœud, cloné.
///
/// Cloné et non emprunté : le spec ne porte que des identités et du style
/// (`stokesforge-plot` : « aucune donnée »), donc la copie est bon marché, et
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
    use stokesforge_plot::{PanelSpec, Panels};

    /// Issue #37 : le champ de saisie est centré sur le texte rendu, à son
    /// corps (proportionnel à la taille et à l'échelle d'affichage), et au
    /// moins aussi grand que lui — la rotation n'y entre pas.
    #[test]
    fn le_champ_de_saisie_est_centre_et_a_la_taille_du_texte() {
        let centre = Pos2::new(240.0, 130.0);
        let texte = egui::vec2(90.0, 20.0);
        let champ = annotation_field(centre, texte, 16.0, 1.0);
        assert_eq!(champ.rect.center(), centre);
        assert!(champ.rect.width() >= texte.x && champ.rect.height() >= texte.y);
        assert_eq!(champ.font_size, stokesforge_render::text::font_size(16.0));

        // Le corps suit la taille du texte…
        let grand = annotation_field(centre, texte, 32.0, 1.0);
        assert!((grand.font_size - 2.0 * champ.font_size).abs() < 1e-4);
        // … et l'échelle d'affichage de la figure.
        let agrandi = annotation_field(centre, texte * 2.0, 16.0, 2.0);
        assert!((agrandi.font_size - 2.0 * champ.font_size).abs() < 1e-4);
        assert_eq!(agrandi.rect.center(), centre);
    }

    /// Un texte vide (ou très court) laisse une largeur de saisie ; un texte
    /// long fait grandir le champ des deux côtés, toujours centré.
    #[test]
    fn le_champ_reste_saisissable_vide_et_suit_un_texte_long() {
        let centre = Pos2::new(100.0, 50.0);
        let vide = annotation_field(centre, egui::Vec2::ZERO, 16.0, 1.0);
        assert!(vide.rect.width() >= 6.0 * vide.font_size);
        assert!(vide.rect.height() > vide.font_size);
        let long = annotation_field(centre, egui::vec2(600.0, 20.0), 16.0, 1.0);
        assert!(long.rect.width() >= 600.0);
        assert_eq!(long.rect.center(), centre);
    }

    /// Issue #34 : `framed_spec` rend le spec cadré par le zoom — ce que la
    /// figure montre — sur un clone ; le spec d'origine, celui du modèle,
    /// ne change pas. Sans zoom, le spec est rendu tel quel.
    #[test]
    fn framed_spec_applique_le_zoom_sur_un_clone() {
        let spec = PlotSpec::empty("Zoom").with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
        let mut view = GraphView::new();
        assert_eq!(view.framed_spec(&spec), spec, "sans zoom, rien ne change");

        view.view
            .set_framing(0, Framing::xy((2.0, 3.0), (-1.0, 1.0)));
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
