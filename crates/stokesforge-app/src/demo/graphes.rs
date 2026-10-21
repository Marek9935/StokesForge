//! Les graphes de la démonstration : `Graph1` (celui de toujours) et six
//! graphes qui montrent chacun une famille de réglages du tracé.
//!
//! | Graphe | Ce qu'il montre |
//! |---|---|
//! | `Graph1` | une courbe, deux axes linéaires |
//! | `Panneaux` | une grille 2×2 de panneaux |
//! | `Axes` | axe Y de droite, échelle logarithmique, calque aux axes secondaires |
//! | `TypesTrace` | nuage, courbe et points, barres, histogramme, barres d'erreur |
//! | `Annotations` | texte Typst, les quatre objets graphiques, légende placée |
//! | `Groupes` | un groupe de six courbes et son nuancier |
//! | `Lignes` | des séries « ligne » de la cinétique de `Book1` |
//!
//! Aucun graphe ne porte de titre (#103) : seuls les titres de panneau, d'axe
//! et de série sont posés. Un graphe neuf se construit **ici** sur le `Project`
//! avant `Session::spawn` (invariant 1) ; les groupes de séries passent par les
//! méthodes de `PanelSpec` (`push_series_batch`, `set_group_gradient`), jamais
//! par un numéro de groupe écrit à la main. Le rôle X des colonnes est posé
//! avant que les graphes ne les citent (`classeur.rs`).

use anyhow::Context as _;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{
    AnnotationSpec, AxisScale, AxisSpec, BinCount, LegendPosition, LineSpec, OverlaySpec,
    PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec, ShapeKind, ShapeSpec,
};

use super::classeur::{Book1, Mesures};

/// Un axe titré, avec son unité.
fn axe(titre: &str, unite: &str) -> AxisSpec {
    AxisSpec {
        label: titre.into(),
        unit: unite.into(),
        ..AxisSpec::default()
    }
}

/// Une série de la palette par défaut : `rang` choisit la couleur.
fn courbe(x: Option<NodeId>, y: NodeId, nom: &str, rang: usize, genre: PlotType) -> SeriesSpec {
    SeriesSpec {
        x,
        name: nom.into(),
        plot_type: genre,
        ..SeriesSpec::from_palette(y, rang)
    }
}

/// Un panneau en `(ligne, colonne)` avec ses deux axes titrés.
fn panneau(
    place: (usize, usize),
    bas: (&str, &str),
    gauche: (&str, &str),
    series: Vec<SeriesSpec>,
) -> PanelSpec {
    let mut panneau = PanelSpec::at(place.0, place.1);
    panneau.bottom = axe(bas.0, bas.1);
    panneau.left = axe(gauche.0, gauche.1);
    panneau.series = series;
    panneau
}

/// `Graph1` : une courbe et deux axes linéaires pour juger le rendu à l'œil.
/// Comme les graphes créés par l'utilisateur, la démo n'ajoute aucun titre
/// par défaut (#103) ; les libellés d'axe et le nom de série restent explicites.
pub(super) fn demo_spec(x: NodeId, y: NodeId) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "λ".into(),
        unit: "nm".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Absorbance mesurée".into(),
        unit: "u.a.".into(),
        ..AxisSpec::default()
    };
    let mut serie = SeriesSpec::new_xy(x, y);
    serie.name = "Échantillon µ1".into();
    serie.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(serie);
    PlotSpec::empty("").with_panels(panel)
}

/// Une grille 2×2 : spectre, nuage, histogramme, barres.
fn panneaux(book1: &Book1, m: &Mesures) -> PlotSpec {
    let mut histogramme = courbe(None, m.bruit, "Bruit", 2, PlotType::Histogram);
    histogramme.histogram_bins = BinCount::new(20).unwrap_or(histogramme.histogram_bins);
    PlotSpec::empty("").with_panels(vec![
        panneau(
            (0, 0),
            ("λ", "nm"),
            ("Absorbance", "u.a."),
            vec![courbe(
                Some(book1.spectre_x),
                book1.spectre_y,
                "Spectre",
                0,
                PlotType::Line,
            )],
        ),
        panneau(
            (0, 1),
            ("Temps", "s"),
            ("Signal", "mV"),
            vec![courbe(
                Some(m.temps),
                m.decroissance,
                "Décroissance",
                1,
                PlotType::Scatter,
            )],
        ),
        panneau(
            (1, 0),
            ("Tension", "mV"),
            ("Occurrences", ""),
            vec![histogramme],
        ),
        panneau(
            (1, 1),
            ("Échantillon", ""),
            ("Rendement", "%"),
            vec![courbe(
                Some(m.barres.0),
                m.barres.1,
                "Rendement",
                3,
                PlotType::Bar,
            )],
        ),
    ])
}

/// Deux cases côte à côte. À gauche : la décroissance en échelle logarithmique
/// (ses données sont strictement positives, invariant 10) et la croissance sur
/// l'axe de droite. À droite : un calque superposé aux axes secondaires, avec
/// son propre axe X en haut et son propre axe Y à droite.
fn axes(m: &Mesures) -> PlotSpec {
    let mut gauche = panneau(
        (0, 0),
        ("Temps", "s"),
        ("Décroissance", "mV"),
        vec![courbe(
            Some(m.temps),
            m.decroissance,
            "Décroissance",
            0,
            PlotType::LineScatter,
        )],
    );
    gauche.left.scale = AxisScale::Log;
    gauche.left.grid = true;
    let mut droite = courbe(
        Some(m.temps),
        m.croissance,
        "Croissance",
        1,
        PlotType::LineScatter,
    );
    droite.axis = SeriesAxis::Right;
    gauche.series.push(droite);
    gauche.right = axe("Croissance", "mV");

    let base = panneau(
        (0, 1),
        ("Temps", "s"),
        ("Croissance", "mV"),
        vec![courbe(
            Some(m.temps),
            m.croissance,
            "Croissance",
            1,
            PlotType::Line,
        )],
    );
    let mut calque = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
    calque.bottom.label = "Temps relatif".into();
    calque.bottom.unit = "min".into();
    calque.left.label = "Température".into();
    calque.left.unit = "°C".into();
    calque.series.push(courbe(
        Some(m.temps),
        m.oscillation,
        "Oscillation",
        3,
        PlotType::Line,
    ));
    PlotSpec::empty("").with_panels(vec![gauche, base, calque])
}

/// Un panneau par type de tracé.
fn types_de_trace(m: &Mesures) -> PlotSpec {
    let mut erreurs = courbe(
        Some(m.temps),
        m.decroissance,
        "Décroissance ± σ",
        4,
        PlotType::ErrorBar,
    );
    erreurs.y_error = Some(m.erreur);
    let mut histogramme = courbe(None, m.bruit, "Bruit", 2, PlotType::Histogram);
    histogramme.histogram_bins = BinCount::new(30).unwrap_or(histogramme.histogram_bins);
    let mut cases = vec![
        panneau(
            (0, 0),
            ("Temps", "s"),
            ("Signal", "mV"),
            vec![courbe(
                Some(m.temps),
                m.decroissance,
                "Nuage",
                0,
                PlotType::Scatter,
            )],
        ),
        panneau(
            (0, 1),
            ("Temps", "s"),
            ("Température", "°C"),
            vec![courbe(
                Some(m.temps),
                m.oscillation,
                "Courbe et points",
                1,
                PlotType::LineScatter,
            )],
        ),
        panneau(
            (0, 2),
            ("Échantillon", ""),
            ("Rendement", "%"),
            vec![courbe(
                Some(m.barres.0),
                m.barres.1,
                "Barres",
                3,
                PlotType::Bar,
            )],
        ),
        panneau(
            (1, 0),
            ("Tension", "mV"),
            ("Occurrences", ""),
            vec![histogramme],
        ),
        panneau((1, 1), ("Temps", "s"), ("Signal", "mV"), vec![erreurs]),
    ];
    for (case, nom) in
        cases
            .iter_mut()
            .zip(["Scatter", "LineScatter", "Bar", "Histogram", "ErrorBar"])
    {
        case.title = nom.into();
    }
    PlotSpec::empty("").with_panels(cases)
}

/// Le spectre de `Book1` annoté : texte Typst, rectangle, ellipse, ligne,
/// flèche, et une légende déplacée dans un coin.
fn annotations(book1: &Book1) -> PlotSpec {
    let mut panel = panneau(
        (0, 0),
        ("λ", "nm"),
        ("Absorbance", "u.a."),
        vec![courbe(
            Some(book1.spectre_x),
            book1.spectre_y,
            "Spectre",
            0,
            PlotType::Line,
        )],
    );
    panel.legend.position = LegendPosition::TopLeft;
    panel
        .annotations
        .push(AnnotationSpec::new("Maximum $lambda_1$", 580.0, 0.8));
    panel
        .annotations
        .push(AnnotationSpec::new("Épaulement $lambda_2$", 668.0, 0.74));

    let mut zone = ShapeSpec::new(ShapeKind::Rectangle, 500.0, 0.0, 540.0, 0.9);
    zone.line_color = "#4c78a8".into();
    zone.fill_color = "#4c78a833".into();
    let mut cercle = ShapeSpec::new(ShapeKind::Ellipse, 585.0, 0.15, 635.0, 0.5);
    cercle.line_color = "#e45756".into();
    let mut fond = ShapeSpec::new(ShapeKind::Line, 400.0, 0.04, 700.0, 0.04);
    fond.line_color = "#888888".into();
    let mut fleche = ShapeSpec::new(ShapeKind::Arrow, 662.0, 0.7, 618.0, 0.36);
    fleche.line_color = "#54a24b".into();
    fleche.line_width = 2.0;
    panel.shapes = vec![zone, cercle, fond, fleche];
    PlotSpec::empty("").with_panels(panel)
}

/// Six courbes ajoutées **ensemble** : elles forment un groupe, auquel on
/// applique un nuancier.
fn groupes(m: &Mesures) -> anyhow::Result<PlotSpec> {
    let mut panel = panneau(
        (0, 0),
        ("Temps de réaction", "h"),
        ("Conversion", "mol/L"),
        Vec::new(),
    );
    let series = m
        .familles_y
        .iter()
        .enumerate()
        .map(|(rang, y)| {
            courbe(
                Some(m.familles_x),
                *y,
                &format!("Courbe {}", rang + 1),
                rang,
                PlotType::Line,
            )
        })
        .collect();
    panel.push_series_batch(series);
    let groupe = *panel
        .group_ids()
        .first()
        .context("le lot de courbes forme un groupe")?;
    anyhow::ensure!(
        panel.set_group_gradient(groupe, &["#2c7bb6", "#fdae61", "#d7191c"]),
        "nuancier du groupe de courbes"
    );
    Ok(PlotSpec::empty("").with_panels(panel))
}

/// Trois lignes de la cinétique, tracées contre les x' des colonnes.
fn lignes(book1: &Book1) -> PlotSpec {
    let series = [0usize, 3, 7]
        .into_iter()
        .enumerate()
        .map(|(rang, ligne)| SeriesSpec {
            row: Some(ligne),
            ..courbe(
                None,
                book1.cinetique,
                &format!("Ligne {}", ligne + 1),
                rang,
                PlotType::LineScatter,
            )
        })
        .collect();
    PlotSpec::empty("").with_panels(panneau(
        (0, 0),
        ("Temps", "s"),
        ("Absorbance", "u.a."),
        series,
    ))
}

/// `Graph1`, juste après `Book1` dans l'arbre.
pub(super) fn graph1(project: &mut Project, book1: &Book1) -> anyhow::Result<()> {
    let racine = project.root();
    project
        .add_graph(
            racine,
            Some("Graph1"),
            demo_spec(book1.spectre_x, book1.spectre_y),
        )
        .context("graphe de démonstration")?;
    Ok(())
}

/// Les six autres graphes, à la racine.
pub(super) fn autres(project: &mut Project, book1: &Book1, m: &Mesures) -> anyhow::Result<()> {
    let racine = project.root();
    for (nom, spec) in [
        ("Panneaux", panneaux(book1, m)),
        ("Axes", axes(m)),
        ("TypesTrace", types_de_trace(m)),
        ("Annotations", annotations(book1)),
        ("Groupes", groupes(m)?),
        ("Lignes", lignes(book1)),
    ] {
        project
            .add_graph(racine, Some(nom), spec)
            .with_context(|| format!("graphe {nom}"))?;
    }
    Ok(())
}
