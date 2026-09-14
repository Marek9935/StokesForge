//! **Le test d'accord** — tâche R4.5, élargie par R5.6, et le critère de fin
//! du plan de rendu tout entier.
//!
//! > Une colonne tracée par `plotyx-render-egui`, la même figure exportée en
//! > SVG par `plotyx-render-svg`, **axes au même endroit** — vérifié par un
//! > test, pas à l'œil.
//!
//! # Ce qui est comparé, et pourquoi pas moins
//!
//! Pas « les deux backends ne plantent pas », pas « les deux rendent un
//! nombre de formes voisin » : **toutes les primitives**, au pixel entier,
//! sous forme de multiensembles triés. R4 comparait les segments — axes,
//! graduations, grille, courbe. R5 ajoute ce qu'elle a introduit :
//! les **cercles** et les **polygones** (marqueurs), les **rectangles**
//! (barres, histogrammes, cadre de légende) et le **texte**. Une comparaison
//! plus faible laisserait passer précisément le défaut que ce test existe
//! pour attraper — un décalage de quelques pixels dû à une mesure de texte
//! divergente, qui ne lève jamais d'erreur (`architecture-choisie.md` §4.4).
//!
//! Le texte est comparé par son **contenu** et non par sa position : côté
//! SVG, `<text x y dy text-anchor>` porte le point d'ancrage brut que
//! Plotters a donné ; côté egui, `Shape::Text` porte le coin supérieur
//! gauche du galley, déjà décalé de l'ancrage. Les deux sont corrects et
//! incomparables — c'est ce que `tests/formes.rs` vérifie séparément. La
//! *position* des graduations, elle, est bien comparée : ce sont leurs
//! traits de repère, qui sont des segments.
//!
//! # Les témoins, sans lesquels le test ne prouverait rien
//!
//! Un test d'égalité entre deux chemins peut passer parce que les deux sont
//! justes, ou parce que les deux sont faux de la même façon. Deux familles
//! de témoins l'écartent :
//!
//! - celui de R4 : la même figure tracée avec le `SVGBackend` **nu** — celui
//!   qui retombe sur l'heuristique par octets UTF-8 — doit se placer
//!   **ailleurs**. Sans lui, une enveloppe qui déléguerait par erreur
//!   `estimate_text_size` passerait pour correcte ;
//! - celui de R5 : pour **chaque** fonctionnalité que la phase ajoute, une
//!   variante du graphe réaliste qui la change doit produire une figure
//!   **différente**. Sans eux, une comparaison qui ne regarderait pas les
//!   cercles, ou un style de trait qui ne serait pas appliqué, passerait
//!   aussi pour correct — et l'accord serait celui de deux figures
//!   également amputées.
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
use plotyx_plot::{
    AxisScale, LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec,
    SymbolShape,
};
use plotyx_render::pipeline::{self, DrawnPlot};
use plotyx_render::sample::{self, Sample};
use plotyx_render::TextMeasurer;
use plotyx_render_egui::collect_shapes;

/// Dimensions de la figure comparée, en unités de backend.
const SIZE: (u32, u32) = (800, 600);
/// Le graphe réaliste a cinq panneaux : il lui faut de la place.
const LARGE: (u32, u32) = (1280, 860);

// -- fixtures --

/// Le graphe minimal de R4 et le graphe réaliste de R5 viennent tous deux de
/// `plotyx_render::sample`, et non d'ici : l'exemple qu'on regarde
/// (`cargo run -p plotyx-render-svg --example figure`) doit être exactement
/// la figure que ce test compare. La raison complète est en tête de ce
/// module-là.
fn minimal() -> Sample {
    sample::minimal().expect("graphe minimal")
}

fn realiste() -> Sample {
    sample::realistic().expect("graphe réaliste")
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne numérique valide")
}

// -- extraction --

/// Un point en coordonnées entières de backend.
type Point = (i32, i32);

/// Ce qu'une figure a posé, réduit à ce que les deux backends décrivent de
/// la même façon.
///
/// Les coordonnées sont arrondies à l'entier : le backend egui travaille en
/// `f32` et le SVG en texte décimal ; comparer autrement reviendrait à
/// comparer deux représentations, pas deux géométries. Plotters, lui, ne
/// produit que des entiers (`BackendCoord = (i32, i32)`), donc l'arrondi ne
/// perd rien.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Primitive {
    /// Un segment de droite.
    Segment(Point, Point),
    /// Un polygone plein (un marqueur triangulaire ou losange).
    Polygon(Vec<Point>),
    /// Un rectangle — barre, cadre de légende, fond.
    Rect(Point, Point),
    /// Un disque, plein ou non.
    Circle(Point, i32),
    /// Un texte, par son contenu seulement (voir la doc de tête).
    Text(String),
}

/// Les primitives d'une figure rendue par le backend egui.
fn primitives_egui(shapes: &[Shape]) -> Vec<Primitive> {
    let mut out = Vec::new();
    let point = |p: Pos2| (p.x.round() as i32, p.y.round() as i32);
    for shape in shapes {
        match shape {
            Shape::LineSegment { points, .. } => {
                out.push(Primitive::Segment(point(points[0]), point(points[1])));
            }
            Shape::Path(path) if path.closed => {
                out.push(Primitive::Polygon(
                    path.points.iter().map(|p| point(*p)).collect(),
                ));
            }
            Shape::Path(path) => {
                for pair in path.points.windows(2) {
                    out.push(Primitive::Segment(point(pair[0]), point(pair[1])));
                }
            }
            Shape::Rect(rect) => {
                out.push(Primitive::Rect(point(rect.rect.min), point(rect.rect.max)));
            }
            Shape::Circle(circle) => {
                out.push(Primitive::Circle(
                    point(circle.center),
                    circle.radius.round() as i32,
                ));
            }
            Shape::Text(text) => {
                out.push(Primitive::Text(text.galley.job.text.clone()));
            }
            _ => {}
        }
    }
    out.sort();
    out
}

/// Les primitives d'un document SVG produit par `plotters-svg`.
///
/// Analyse à la main plutôt qu'avec un parseur XML : la sortie de
/// `plotters-svg` est un sous-ensemble minuscule et stable du format (pas de
/// `defs`, pas d'`id`, pas de `clipPath` — c'est d'ailleurs ce qui rend la
/// fusion Typst de R8 facile, `architecture-choisie.md` §5.2), et une
/// dépendance de plus dans un test de frontière serait mal placée.
fn primitives_svg(document: &str) -> Vec<Primitive> {
    let mut out = Vec::new();

    for tag in document.split('<').skip(1) {
        if let Some(rest) = tag.strip_prefix("polyline") {
            for pair in points_of(rest).windows(2) {
                out.push(Primitive::Segment(pair[0], pair[1]));
            }
        } else if let Some(rest) = tag.strip_prefix("polygon") {
            out.push(Primitive::Polygon(points_of(rest)));
        } else if let Some(rest) = tag.strip_prefix("line") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                (get("x1"), get("y1"), get("x2"), get("y2"))
            {
                out.push(Primitive::Segment((x1, y1), (x2, y2)));
            }
        } else if let Some(rest) = tag.strip_prefix("rect") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(x), Some(y), Some(w), Some(h)) =
                (get("x"), get("y"), get("width"), get("height"))
            {
                out.push(Primitive::Rect((x, y), (x + w, y + h)));
            }
        } else if let Some(rest) = tag.strip_prefix("circle") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(cx), Some(cy), Some(r)) = (get("cx"), get("cy"), get("r")) {
                out.push(Primitive::Circle((cx, cy), r));
            }
        } else if let Some(rest) = tag.strip_prefix("text") {
            if let Some(contenu) = rest.split_once('>').map(|(_, after)| after) {
                out.push(Primitive::Text(unescape(contenu.trim())));
            }
        }
    }
    out.sort();
    out
}

fn points_of(tag: &str) -> Vec<Point> {
    let Some(points) = attribute(tag, "points") else {
        return Vec::new();
    };
    points
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some((round(x)?, round(y)?))
        })
        .collect()
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

/// L'inverse de l'échappement de `plotters-svg` (`svg.rs::escape_and_push`).
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// -- rendus --

fn rendu_egui(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> (Vec<Primitive>, DrawnPlot) {
    let plot = resolve_spec(tree, spec);
    let measurer = TextMeasurer::default();
    let mut shapes = Vec::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32));
    let drawn = collect_shapes(&mut shapes, rect, &measurer, &plot).expect("tracé egui");
    (primitives_egui(&shapes), drawn)
}

fn rendu_svg(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> (Vec<Primitive>, DrawnPlot) {
    let plot = resolve_spec(tree, spec);
    // Volontairement un **second** `TextMeasurer`, pas celui du rendu egui :
    // la tâche R4.1 dit que partager les `FontDefinitions` suffit, et c'est
    // justement ce que ce test doit éprouver.
    let measurer = TextMeasurer::default();
    let (document, drawn) =
        plotyx_render_svg::render_to_string_with_geometry(&plot, size, &measurer)
            .expect("export SVG");
    (primitives_svg(&document), drawn)
}

/// Le même rendu, mais avec le `SVGBackend` **nu** : le témoin de R4.
fn rendu_svg_sans_surcharge(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> Vec<Primitive> {
    let plot = resolve_spec(tree, spec);
    let mut buffer = String::new();
    {
        let backend = plotyx_render_svg::plain_backend(&mut buffer, size);
        pipeline::draw(backend, &plot).expect("tracé SVG nu");
    }
    primitives_svg(&buffer)
}

/// Le cadre de la figure : les extrêmes des **segments**, ce qui donne un
/// message d'échec lisible là où comparer deux listes de 500 primitives n'en
/// donnerait aucun.
///
/// Les segments et eux seuls : le fond de la figure est un rectangle qui
/// couvre toute la zone par construction, donc l'inclure rendrait ce cadre
/// égal aux dimensions demandées quoi qu'il arrive — et le témoin du §4.4,
/// qui repose sur le déplacement de la zone de tracé, ne mordrait plus
/// jamais.
fn cadre(primitives: &[Primitive]) -> (i32, i32, i32, i32) {
    let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for primitive in primitives {
        let Primitive::Segment(a, b) = primitive else {
            continue;
        };
        for (x, y) in [a, b] {
            bounds.0 = bounds.0.min(*x);
            bounds.1 = bounds.1.min(*y);
            bounds.2 = bounds.2.max(*x);
            bounds.3 = bounds.3.max(*y);
        }
    }
    bounds
}

/// Compare les deux backends sur un graphe donné, et rend la géométrie.
fn accord(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    quoi: &str,
) -> (Vec<Primitive>, DrawnPlot) {
    let (egui_primitives, egui_drawn) = rendu_egui(tree, spec, size);
    let (svg_primitives, svg_drawn) = rendu_svg(tree, spec, size);

    assert!(
        !egui_primitives.is_empty(),
        "{quoi} : le backend egui n'a produit aucune primitive, le test ne \
         compare rien"
    );
    // D'abord le cadre, pour que l'échec soit lisible.
    assert_eq!(
        cadre(&egui_primitives),
        cadre(&svg_primitives),
        "{quoi} : la zone de tracé n'est pas au même endroit — c'est \
         exactement le défaut silencieux du §4.4 : une mesure de texte \
         divergente décale la figure sans lever d'erreur"
    );
    // Puis l'égalité complète, primitive par primitive.
    assert_eq!(
        egui_primitives,
        svg_primitives,
        "{quoi} : {} primitives côté egui, {} côté SVG — les deux backends \
         ne tracent pas la même figure",
        egui_primitives.len(),
        svg_primitives.len()
    );
    // Et la géométrie logique, qui n'est pas déductible des pixels.
    assert_eq!(
        egui_drawn, svg_drawn,
        "{quoi} : géométries logiques distinctes"
    );
    (egui_primitives, egui_drawn)
}

// -- R4 : le graphe minimal --

#[test]
fn les_deux_backends_placent_les_axes_au_meme_endroit() {
    let fixture = minimal();
    let (_, drawn) = accord(
        fixture.project.tree(),
        &fixture.spec,
        SIZE,
        "graphe minimal",
    );
    assert_eq!(drawn.drawn_series, 1);
    assert_eq!(drawn.broken_series, 0);
    assert_eq!(drawn.panels.len(), 1);
}

#[test]
fn laccord_tient_a_plusieurs_dimensions_de_figure() {
    // Une seule taille pourrait tomber juste par hasard — deux marges qui se
    // compensent, un arrondi qui masque un décalage.
    let fixture = minimal();
    for size in [(400_u32, 300_u32), (800, 600), (1280, 720), (640, 900)] {
        accord(
            fixture.project.tree(),
            &fixture.spec,
            size,
            &format!("graphe minimal à {size:?}"),
        );
    }
}

#[test]
fn temoin_sans_la_surcharge_de_estimate_text_size_les_axes_bougent() {
    let fixture = minimal();
    let (avec, _) = rendu_svg(fixture.project.tree(), &fixture.spec, SIZE);
    let sans = rendu_svg_sans_surcharge(fixture.project.tree(), &fixture.spec, SIZE);

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
    let mut fixture = minimal();
    let fantome = NodeId::from_raw(u64::MAX);
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel.series.push(SeriesSpec::new(fantome));
    }

    let (primitives, drawn) = accord(
        fixture.project.tree(),
        &fixture.spec,
        SIZE,
        "référence brisée",
    );
    assert_eq!(drawn.broken_series, 1, "la référence brisée est comptée");
    assert_eq!(drawn.drawn_series, 1, "l'autre série est tracée quand même");
    // R5 va plus loin que R4, qui se contentait de compter : la série
    // brisée **se voit**, en légende, annotée.
    assert!(
        primitives.iter().any(|p| matches!(
            p,
            Primitive::Text(texte) if texte.contains("référence rompue")
        )),
        "la référence brisée n'apparaît nulle part dans la figure : elle est \
         tue, ce que le §11.1 interdit"
    );
    assert_eq!(drawn.panels[0].legend_entries, 2);
}

// -- R5 : le graphe réaliste --

#[test]
fn le_graphe_realiste_exerce_bien_ce_quil_pretend_exercer() {
    // Un test d'accord sur un graphe qui n'exercerait rien passerait tout
    // seul. Celui-ci vérifie d'abord que la figure de référence contient
    // réellement chacune des fonctionnalités de la phase.
    let fixture = realiste();
    let (_, drawn) = rendu_egui(fixture.project.tree(), &fixture.spec, LARGE);

    assert_eq!(
        drawn.panels.len(),
        5,
        "cinq panneaux, dans une grille 2 × 3"
    );
    assert_eq!(drawn.omitted_panels, 0, "aucun panneau ne doit être omis");
    assert_eq!(drawn.broken_series, 1, "une référence brisée, exprès");
    assert_eq!(
        drawn.drawn_series, 7,
        "sept séries tracées : deux spectres, deux en cinétique, \
         l'étalonnage, l'histogramme et les barres"
    );

    let etalonnage = drawn
        .panels
        .iter()
        .find(|panel| panel.y_scale == AxisScale::Log)
        .expect("un panneau à ordonnée logarithmique");
    assert!(etalonnage.y_range.0 > 0.0, "{:?}", etalonnage.y_range);

    let cinetique = drawn
        .panels
        .iter()
        .find(|panel| panel.y2_range.is_some())
        .expect("un panneau à axe secondaire");
    assert_ne!(
        cinetique.y2_range.map(|r| r.0),
        Some(cinetique.y_range.0),
        "l'axe de droite a le même cadrage que celui de gauche : il n'est pas \
         vraiment secondaire"
    );

    assert!(
        drawn.panels.iter().all(|panel| panel.legend_entries > 0),
        "chaque panneau doit porter une légende"
    );
}

#[test]
fn laccord_tient_sur_le_graphe_realiste() {
    let fixture = realiste();
    for size in [LARGE, (900, 600), (1600, 1000)] {
        accord(
            fixture.project.tree(),
            &fixture.spec,
            size,
            &format!("graphe réaliste à {size:?}"),
        );
    }
}

#[test]
fn temoin_le_svgbackend_nu_deplace_aussi_le_graphe_realiste() {
    let fixture = realiste();
    let (avec, _) = rendu_svg(fixture.project.tree(), &fixture.spec, LARGE);
    let sans = rendu_svg_sans_surcharge(fixture.project.tree(), &fixture.spec, LARGE);
    assert_ne!(cadre(&avec), cadre(&sans));
}

/// Une fonctionnalité de R5, et de quoi la débrancher : le nom sert au
/// message d'échec, la fonction modifie le graphe réaliste.
type Variante = (&'static str, fn(&mut PlotSpec));

/// **Le témoin de R5.** Pour chaque fonctionnalité ajoutée par la phase, une
/// variante du graphe réaliste qui la change doit produire une figure
/// différente — et les deux backends doivent rester d'accord *sur la
/// variante aussi*.
///
/// Sans ce test, une fonctionnalité qui ne serait pas réellement appliquée
/// (un style de trait ignoré, une échelle log sans effet) laisserait le test
/// d'accord au vert : deux figures identiquement amputées s'accordent
/// parfaitement.
#[test]
fn temoin_chaque_fonctionnalite_de_r5_change_la_figure() {
    let fixture = realiste();
    let (reference, _) = accord(
        fixture.project.tree(),
        &fixture.spec,
        LARGE,
        "graphe réaliste (référence)",
    );

    let variantes: Vec<Variante> = vec![
        ("style de trait", |spec| {
            if let Some(panel) = spec.panels.get_mut(0) {
                panel.series[1].line.style = LineStyle::Solid;
            }
        }),
        ("forme de marqueur", |spec| {
            if let Some(panel) = spec.panels.get_mut(1) {
                panel.series[1].symbol.shape = SymbolShape::Cross;
            }
        }),
        ("échelle logarithmique", |spec| {
            if let Some(panel) = spec.panels.get_mut(2) {
                panel.left.scale = AxisScale::Linear;
            }
        }),
        ("axe secondaire", |spec| {
            if let Some(panel) = spec.panels.get_mut(1) {
                panel.series[1].axis = SeriesAxis::Left;
            }
        }),
        ("légende", |spec| {
            for index in 0..5 {
                if let Some(panel) = spec.panels.get_mut(index) {
                    panel.legend.visible = false;
                }
            }
        }),
        ("position de légende", |spec| {
            if let Some(panel) = spec.panels.get_mut(0) {
                panel.legend.position = LegendPosition::BottomLeft;
            }
        }),
        ("type de tracé", |spec| {
            if let Some(panel) = spec.panels.get_mut(4) {
                panel.series[0].plot_type = PlotType::Line;
            }
        }),
        ("nombre de classes de l'histogramme", |spec| {
            if let Some(panel) = spec.panels.get_mut(3) {
                panel.series[0].histogram_bins = plotyx_plot::BinCount::new(7).expect("7 classes");
            }
        }),
        ("grille de panneaux", |spec| {
            if let Some(panel) = spec.panels.get_mut(4) {
                panel.column = 2;
            }
        }),
    ];

    for (quoi, modifier) in variantes {
        let mut variante = realiste();
        modifier(&mut variante.spec);
        let (primitives, _) = accord(
            variante.project.tree(),
            &variante.spec,
            LARGE,
            &format!("variante « {quoi} »"),
        );
        assert_ne!(
            primitives, reference,
            "changer « {quoi} » n'a rien changé à la figure : la \
             fonctionnalité n'est pas appliquée, ou le test ne la regarde pas"
        );
    }
}

// -- R5.5 : décimation --

#[test]
fn la_decimation_sapplique_et_les_deux_backends_deciment_pareil() {
    // Le LTTB dépend de la **largeur en pixels** de la zone de tracé : c'est
    // le seul endroit du pipeline où la géométrie dépend de la taille de la
    // figure autrement que par une homothétie. Si les deux backends
    // n'obtenaient pas la même largeur, ils ne décimeraient pas de la même
    // façon — et rien d'autre ne le dirait.
    let mut project = Project::new("Acquisition").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let n = 40_000usize;
    let xs: Vec<f64> = (0..n).map(|i| i as f64 * 1e-4).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| (x * 37.0).sin() + 0.2 * (x * 311.0).sin())
        .collect();
    let x = project
        .add_column(sheet, None, Some("t"), numeric(&xs))
        .expect("t");
    let y = project
        .add_column(sheet, None, Some("signal"), numeric(&ys))
        .expect("signal");

    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(x, y));
    let spec = PlotSpec::empty("Acquisition").with_panels(panel);

    let (_, drawn) = accord(project.tree(), &spec, SIZE, "série longue");
    assert!(
        drawn.decimated_points > 0,
        "40 000 points sur 800 px devraient être décimés"
    );
    assert!(
        drawn.decimated_points > n / 2,
        "seulement {} points retirés sur {n} : le seuil n'est pas celui \
         qu'on croit",
        drawn.decimated_points
    );

    // Le cadrage, lui, reste celui des données entières : il est calculé
    // avant la décimation, et le LTTB ne choisit que des points réels — donc
    // les deux resteraient cohérents même si l'ordre changeait un jour.
    let min = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let marge = (max - min) * 0.05;
    let (bas, haut) = drawn.y_range();
    assert!(
        (bas - (min - marge)).abs() < 1e-9 && (haut - (max + marge)).abs() < 1e-9,
        "le cadrage {:?} ne correspond plus à l'étendue des 40 000 points \
         ({min}..{max}) : la décimation a perdu un extremum",
        drawn.y_range()
    );
}
