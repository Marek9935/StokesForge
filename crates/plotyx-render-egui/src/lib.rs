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
//! # Typst
//!
//! La feature `typst` existe et ne fait rien ; voir le `Cargo.toml` pour la
//! raison (§7 d'`architecture-choisie.md` : le droit d'abandonner Typst sans
//! rien démonter se garde en compilant sans lui **dès le premier jour**). Le
//! texte ordinaire des graduations passe par `Shape::Text`, jamais par
//! Typst, feature activée ou non.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod cache;
pub mod feathering;

use std::sync::Arc;

use egui::{Color32, Galley, Painter, Rect, Shape};
use plotyx_core::node::NodeKind;
use plotyx_core::resolve::{resolve_spec, ResolvedPlot};
use plotyx_core::{NodeId, Tree};
use plotyx_plot::PlotSpec;
use plotyx_render::pipeline::{self, DrawnPlot};
use plotyx_session::Snapshot;

pub use backend::{EguiBackend, EguiBackendError};
pub use cache::{CacheKey, Outcome, ShapeCache};
pub use feathering::Feathering;
pub use plotyx_render::text::{font_definitions, TextMeasurer, TextSource};

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

/// Un graphe affiché : son cache, et de quoi le repeindre.
///
/// Un `GraphView` par vue, pas un par graphe : deux fenêtres sur le même
/// graphe ont deux rectangles, donc deux caches. Le partage se ferait au prix
/// d'un cache qui se viderait à chaque alternance.
#[derive(Debug)]
pub struct GraphView {
    cache: ShapeCache,
    last: Option<DrawnPlot>,
    measurer: TextMeasurer,
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphView {
    /// Une vue neuve, sans rien en cache.
    ///
    /// Construit sa propre source de mesure : c'est le seul coût notable de
    /// la vue (le décodage des polices embarquées), payé une fois à
    /// l'ouverture d'une fenêtre de graphe.
    pub fn new() -> Self {
        Self {
            cache: ShapeCache::new(),
            last: None,
            measurer: TextMeasurer::default(),
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

    /// Peint le graphe `graph` de `snapshot` dans `rect`.
    ///
    /// Rend `true` si l'image a été servie depuis le cache — c'est-à-dire si
    /// le pipeline Plotters n'a **pas** été rejoué.
    pub fn paint(
        &mut self,
        painter: &Painter,
        rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let spec = graph_spec(&snapshot.tree, graph)?;
        let context = painter.ctx().clone();
        let key = CacheKey {
            revision: snapshot.revision,
            rect,
            pixels_per_point: context.pixels_per_point(),
        };
        let tree = snapshot.tree.clone();
        let measurer = &self.measurer;
        let outcome = self.cache.get_or_build(key, |shapes| {
            let plot = resolve_spec(&tree, &spec);
            let text = ScreenText::new(&context, measurer);
            collect_shapes(shapes, rect, &text, &plot)
        })?;
        if let Some(drawn) = outcome.built {
            self.last = Some(drawn);
        }
        // Le cœur de la tâche R4.3 : une image sans changement, c'est
        // exactement cette ligne — pas de résolution, pas de `ChartBuilder`,
        // pas de parcours de points.
        painter.extend(self.cache.shapes().iter().cloned());
        Ok(outcome.cached)
    }
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
