//! **Le test d'accord** — tâche R4.5, et le critère de fin du plan de rendu
//! tout entier.
//!
//! > Une colonne tracée par `plotyx-render-egui`, la même figure exportée en
//! > SVG par `plotyx-render-svg`, **axes au même endroit** — vérifié par un
//! > test, pas à l'œil.
//!
//! # Ce qui est comparé, et pourquoi pas moins
//!
//! Pas « les deux backends ne plantent pas », pas « les deux rendent un
//! nombre de formes voisin » : **toutes les positions de trait**, au pixel
//! entier, sous forme de multiensembles de segments. Les axes, les
//! graduations, la grille et la courbe elle-même sont dedans. Une
//! comparaison plus faible laisserait passer précisément le défaut que ce
//! test existe pour attraper — un décalage de quelques pixels dû à une
//! mesure de texte divergente, qui ne lève jamais d'erreur
//! (`architecture-choisie.md` §4.4).
//!
//! # Le témoin, sans lequel le test ne prouverait rien
//!
//! Un test d'égalité entre deux chemins peut passer parce que les deux sont
//! justes, ou parce que les deux sont faux de la même façon. Le troisième
//! test de ce fichier trace la même figure avec le `SVGBackend` **nu** —
//! celui qui retombe sur l'heuristique par octets UTF-8 — et exige que les
//! positions soient **différentes**. Sans lui, une enveloppe qui déléguerait
//! par erreur `estimate_text_size` passerait pour correcte.
//!
//! C'est la méthode du spike R0 0.1, qui portait déjà un témoin positif (une
//! famille de police volontairement autre) pour prouver que l'égalité
//! mesurée n'était pas un artefact du harnais.
//!
//! # Où le test vit, et pourquoi pas dans `plotyx-render-svg`
//!
//! Il lui faut les deux backends. Le mettre côté SVG ferait entrer `egui` en
//! dépendance de développement de ce crate, et `cargo tree -p
//! plotyx-render-svg` cesserait de démontrer ce qu'il démontre
//! (`plotyx-render-svg/tests/frontiere.rs`). Il vit donc côté egui, qui a
//! déjà egui.

use egui::{Pos2, Rect, Shape};
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::{NodeId, Project, Tree};
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render::pipeline::{self, DrawnPlot};
use plotyx_render::TextMeasurer;
use plotyx_render_egui::collect_shapes;

/// Dimensions de la figure comparée, en unités de backend.
const SIZE: (u32, u32) = (800, 600);

// ---------------------------------------------------------------- fixture

/// Un projet portant un graphe d'un spectre : une colonne X, une colonne Y,
/// un titre et des libellés d'axe **non ASCII**.
///
/// Le non-ASCII n'est pas de la décoration : « λ », « ⁻¹ » et « é » sont
/// exactement là où l'heuristique par octets de Plotters se trompe le plus
/// (« λ » compte pour deux octets et un seul glyphe). Un jeu de libellés
/// purement ASCII rendrait le témoin plus faible.
struct Fixture {
    project: Project,
    spec: PlotSpec,
}

impl Fixture {
    fn new() -> Self {
        let mut project = Project::new("Accord").expect("projet valide");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("classeur");
        let sheet = project.add_worksheet(book, None).expect("feuille");

        let xs: Vec<f64> = (0..64).map(|i| 400.0 + i as f64 * 5.0).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| {
                let u = (x - 520.0) / 40.0;
                0.9 * (-u * u).exp() + 0.05
            })
            .collect();

        let x = project
            .add_column(sheet, None, Some("Lambda"), numeric(&xs))
            .expect("colonne X");
        let y = project
            .add_column(sheet, None, Some("Absorbance"), numeric(&ys))
            .expect("colonne Y");

        let mut panel = PanelSpec::at(0, 0);
        panel.bottom = AxisSpec {
            label: "λ".to_string(),
            unit: "nm".to_string(),
            grid: true,
            ..AxisSpec::default()
        };
        panel.left = AxisSpec {
            label: "Absorbance mesurée".to_string(),
            unit: "u.a.".to_string(),
            grid: true,
            ..AxisSpec::default()
        };
        let mut series = SeriesSpec::new_xy(x, y);
        series.name = "Échantillon µ1".to_string();
        series.line = LineSpec {
            color: "#4c78a8".to_string(),
            ..LineSpec::default()
        };
        panel.series.push(series);

        let spec = PlotSpec {
            title: "Spectre d'absorption — λ_max".to_string(),
            background: "#ffffff".to_string(),
            ..PlotSpec::empty("")
        }
        .with_panels(panel);

        Self { project, spec }
    }

    fn tree(&self) -> &Tree {
        self.project.tree()
    }
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne numérique valide")
}

// ------------------------------------------------------- extraction

/// Un segment de droite, en coordonnées entières de backend.
type Segment = ((i32, i32), (i32, i32));

/// Les segments d'une figure rendue par le backend egui.
///
/// Les coordonnées sont arrondies à l'entier : le backend egui travaille en
/// `f32` et le SVG en texte décimal ; comparer autrement reviendrait à
/// comparer deux représentations, pas deux géométries. Plotters, lui, ne
/// produit que des entiers (`BackendCoord = (i32, i32)`), donc l'arrondi ne
/// perd rien.
fn segments_egui(shapes: &[Shape]) -> Vec<Segment> {
    let mut out = Vec::new();
    let point = |p: Pos2| (p.x.round() as i32, p.y.round() as i32);
    for shape in shapes {
        match shape {
            Shape::LineSegment { points, .. } => {
                out.push((point(points[0]), point(points[1])));
            }
            Shape::Path(path) => {
                for pair in path.points.windows(2) {
                    out.push((point(pair[0]), point(pair[1])));
                }
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out
}

/// Les segments d'un document SVG produit par `plotters-svg`.
///
/// Analyse à la main plutôt qu'avec un parseur XML : la sortie de
/// `plotters-svg` est un sous-ensemble minuscule et stable du format (pas de
/// `defs`, pas d'`id`, pas de `clipPath` — c'est d'ailleurs ce qui rend la
/// fusion Typst de R8 facile, `architecture-choisie.md` §5.2), et une
/// dépendance de plus dans un test de frontière serait mal placée.
fn segments_svg(document: &str) -> Vec<Segment> {
    let mut out = Vec::new();

    for tag in document.split('<').skip(1) {
        if let Some(rest) = tag.strip_prefix("polyline") {
            let Some(points) = attribute(rest, "points") else {
                continue;
            };
            let parsed: Vec<(i32, i32)> = points
                .split_whitespace()
                .filter_map(|pair| {
                    let (x, y) = pair.split_once(',')?;
                    Some((round(x)?, round(y)?))
                })
                .collect();
            for pair in parsed.windows(2) {
                out.push((pair[0], pair[1]));
            }
        } else if let Some(rest) = tag.strip_prefix("line") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                (get("x1"), get("y1"), get("x2"), get("y2"))
            {
                out.push(((x1, y1), (x2, y2)));
            }
        }
    }
    out.sort_unstable();
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = start + tag[start..].find('"')?;
    Some(tag[start..end].to_string())
}

fn round(value: &str) -> Option<i32> {
    value.trim().parse::<f64>().ok().map(|v| v.round() as i32)
}

// ------------------------------------------------------------- rendus

fn rendu_egui(fixture: &Fixture) -> (Vec<Segment>, DrawnPlot) {
    let plot = resolve_spec(fixture.tree(), &fixture.spec);
    let measurer = TextMeasurer::default();
    let mut shapes = Vec::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0 as f32, SIZE.1 as f32));
    let drawn = collect_shapes(&mut shapes, rect, &measurer, &plot).expect("tracé egui");
    (segments_egui(&shapes), drawn)
}

fn rendu_svg(fixture: &Fixture) -> (Vec<Segment>, DrawnPlot) {
    let plot = resolve_spec(fixture.tree(), &fixture.spec);
    // Volontairement un **second** `TextMeasurer`, pas celui du rendu egui :
    // la tâche R4.1 dit que partager les `FontDefinitions` suffit, et c'est
    // justement ce que ce test doit éprouver.
    let measurer = TextMeasurer::default();
    let (document, drawn) =
        plotyx_render_svg::render_to_string_with_geometry(&plot, SIZE, &measurer)
            .expect("export SVG");
    (segments_svg(&document), drawn)
}

/// Le même rendu, mais avec le `SVGBackend` **nu** : le témoin.
fn rendu_svg_sans_surcharge(fixture: &Fixture) -> Vec<Segment> {
    let plot = resolve_spec(fixture.tree(), &fixture.spec);
    let mut buffer = String::new();
    {
        let backend = plotyx_render_svg::plain_backend(&mut buffer, SIZE);
        pipeline::draw(backend, &plot).expect("tracé SVG nu");
    }
    segments_svg(&buffer)
}

/// Le cadre de la zone de tracé : les extrêmes de tous les segments, ce qui
/// donne un message d'échec lisible là où comparer deux listes de 200
/// segments n'en donnerait aucun.
fn cadre(segments: &[Segment]) -> (i32, i32, i32, i32) {
    let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for ((x1, y1), (x2, y2)) in segments {
        for (x, y) in [(x1, y1), (x2, y2)] {
            bounds.0 = bounds.0.min(*x);
            bounds.1 = bounds.1.min(*y);
            bounds.2 = bounds.2.max(*x);
            bounds.3 = bounds.3.max(*y);
        }
    }
    bounds
}

// -------------------------------------------------------------- tests

#[test]
fn les_deux_backends_placent_les_axes_au_meme_endroit() {
    let fixture = Fixture::new();
    let (egui_segments, egui_drawn) = rendu_egui(&fixture);
    let (svg_segments, svg_drawn) = rendu_svg(&fixture);

    assert!(
        !egui_segments.is_empty(),
        "le backend egui n'a produit aucun segment : la figure est vide, \
         le test ne compare rien"
    );

    // D'abord le cadre, pour que l'échec soit lisible.
    assert_eq!(
        cadre(&egui_segments),
        cadre(&svg_segments),
        "la zone de tracé n'est pas au même endroit — c'est exactement le \
         défaut silencieux du §4.4 : une mesure de texte divergente décale \
         la figure sans lever d'erreur"
    );

    // Puis l'égalité complète, segment par segment.
    assert_eq!(
        egui_segments,
        svg_segments,
        "{} segments côté egui, {} côté SVG : les deux backends ne tracent \
         pas la même figure",
        egui_segments.len(),
        svg_segments.len()
    );

    // Et la géométrie logique, qui n'est pas déductible des pixels.
    assert_eq!(egui_drawn, svg_drawn);
    assert_eq!(egui_drawn.drawn_series, 1);
    assert_eq!(egui_drawn.broken_series, 0);
}

#[test]
fn laccord_tient_a_plusieurs_dimensions_de_figure() {
    // Une seule taille pourrait tomber juste par hasard — deux marges qui se
    // compensent, un arrondi qui masque un décalage.
    let fixture = Fixture::new();
    for size in [(400_u32, 300_u32), (800, 600), (1280, 720), (640, 900)] {
        let plot = resolve_spec(fixture.tree(), &fixture.spec);
        let measurer = TextMeasurer::default();
        let mut shapes = Vec::new();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32));
        collect_shapes(&mut shapes, rect, &measurer, &plot).expect("tracé egui");

        let autre = TextMeasurer::default();
        let document =
            plotyx_render_svg::render_to_string(&plot, size, &autre).expect("export SVG");

        assert_eq!(
            segments_egui(&shapes),
            segments_svg(&document),
            "désaccord à {size:?}"
        );
    }
}

#[test]
fn temoin_sans_la_surcharge_de_estimate_text_size_les_axes_bougent() {
    let fixture = Fixture::new();
    let (avec, _) = rendu_svg(&fixture);
    let sans = rendu_svg_sans_surcharge(&fixture);

    assert_ne!(
        cadre(&avec),
        cadre(&sans),
        "le SVGBackend nu place la figure exactement comme le nôtre : soit \
         l'enveloppe délègue estimate_text_size par erreur, soit le témoin \
         ne mord plus — dans les deux cas le test d'accord ne prouve plus \
         rien (architecture §4.4)"
    );
}

#[test]
fn une_reference_brisee_ne_casse_ni_le_rendu_ni_laccord() {
    // `architecture-choisie.md` §11.1 : une référence brisée est un état de
    // première classe, jamais un plantage ni une série silencieusement vide.
    // Les deux backends doivent s'en accommoder *de la même façon*.
    let mut fixture = Fixture::new();
    let fantome = NodeId::from_raw(u64::MAX);
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel.series.push(SeriesSpec::new(fantome));
    }

    let (egui_segments, egui_drawn) = rendu_egui(&fixture);
    let (svg_segments, svg_drawn) = rendu_svg(&fixture);

    assert_eq!(egui_segments, svg_segments);
    assert_eq!(egui_drawn, svg_drawn);
    assert_eq!(
        egui_drawn.broken_series, 1,
        "la référence brisée est comptée"
    );
    assert_eq!(
        egui_drawn.drawn_series, 1,
        "l'autre série est tracée quand même"
    );
}
