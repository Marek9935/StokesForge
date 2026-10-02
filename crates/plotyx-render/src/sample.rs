//! Deux graphes de démonstration, construits en code.
//!
//! # Pourquoi ils vivent dans un crate de production
//!
//! La sortie de la phase R5 demande que le critère de fin soit vrai pour un
//! graphe **réaliste**, et que le test d'accord porte sur celui-là. Il
//! demande aussi, depuis R4, qu'on puisse *regarder* la figure
//! (`cargo run -p plotyx-render-svg --example figure`). Ces deux besoins
//! portent sur le **même** graphe : « ce qu'on regarde est exactement ce que
//! le test compare » est ce que R4 avait noté en tête de son exemple.
//!
//! Or le test d'accord vit dans `plotyx-render-egui` (il lui faut les deux
//! backends) et l'exemple dans `plotyx-render-svg` (il ne doit pas voir
//! egui). Deux crates qui ne peuvent pas dépendre l'un de l'autre : le
//! graphe commun descend donc d'un cran, exactement comme la mesure de texte
//! en R4 et `NodeId` en R3. Le dupliquer aurait garanti qu'il divergerait, et
//! la divergence aurait porté précisément sur ce que le test prétend
//! vérifier.
//!
//! Ce n'est pas une dépendance de développement parce qu'un `[dev-dependency]`
//! ne traverse pas les crates ; c'est du code public, sans coût pour qui ne
//! l'appelle pas, et il n'y a **aucun fichier `.plx` d'exemple dans le
//! dépôt** dont on pourrait le tirer à la place.
//!
//! # Ce que le graphe réaliste exerce
//!
//! Tout ce que R5 ajoute, en un seul spec : cinq panneaux dans une grille
//! 2 × 3 (donc une cellule vide), les six types de tracé, une échelle
//! logarithmique, un axe secondaire, trois styles de trait, quatre formes de
//! marqueur, des barres d'erreur, une légende dans chaque coin, et une
//! **référence brisée** — qui doit se voir, pas disparaître.

use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::{CoreResult, NodeId, Project};
use plotyx_plot::{
    AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle, PanelSpec, Panels,
    PlotSpec, PlotType, SeriesAxis, SeriesSpec, SymbolShape, SymbolSpec,
};

/// Un projet et le graphe qu'il porte.
pub struct Sample {
    /// Le projet, qui possède les colonnes référencées par le spec.
    pub project: Project,
    /// Le graphe.
    pub spec: PlotSpec,
}

/// Le graphe minimal de la phase R4 : une courbe, deux axes linéaires.
///
/// Conservé tel quel — il reste le cas le plus simple, celui qui doit
/// continuer de passer quoi qu'ajoutent les phases suivantes.
pub fn minimal() -> CoreResult<Sample> {
    let mut project = Project::new("Accord")?;
    let sheet = worksheet(&mut project, "Book1", "Sheet1")?;

    let xs: Vec<f64> = (0..64).map(|i| 400.0 + i as f64 * 5.0).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| gaussienne(*x, 520.0, 40.0, 0.9) + 0.05)
        .collect();
    let x = column(&mut project, sheet, "Lambda", &xs)?;
    let y = column(&mut project, sheet, "Absorbance", &ys)?;

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = axis("λ", "nm");
    panel.left = axis("Absorbance mesurée", "u.a.");
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

    Ok(Sample { project, spec })
}

/// Le graphe réaliste de la phase R5 — cinq panneaux, six types de tracé.
///
/// Les libellés sont volontairement non ASCII (« λ », « ⁻¹ », « µ », « é ») :
/// c'est là que l'heuristique par octets de Plotters se trompe le plus, donc
/// là où le témoin du test d'accord mord le mieux (`architecture-choisie.md`
/// §4.4).
pub fn realistic() -> CoreResult<Sample> {
    let mut project = Project::new("Campagne")?;
    let sheet = worksheet(&mut project, "Mesures", "Spectres")?;

    // -- (0,0) deux spectres superposés, traits de styles différents.
    let lambda: Vec<f64> = (0..=140).map(|i| 400.0 + i as f64 * 2.5).collect();
    let spectre_a: Vec<f64> = lambda
        .iter()
        .map(|x| gaussienne(*x, 520.0, 38.0, 0.85) + gaussienne(*x, 610.0, 22.0, 0.32) + 0.04)
        .collect();
    let spectre_b: Vec<f64> = lambda
        .iter()
        .map(|x| gaussienne(*x, 545.0, 55.0, 0.62) + 0.06)
        .collect();
    let id_lambda = column(&mut project, sheet, "Lambda", &lambda)?;
    let id_a = column(&mut project, sheet, "AbsA", &spectre_a)?;
    let id_b = column(&mut project, sheet, "AbsB", &spectre_b)?;

    let mut spectres = PanelSpec::at(0, 0);
    spectres.title = "Spectres".to_string();
    spectres.bottom = axis("λ", "nm");
    spectres.left = axis("Absorbance", "u.a.");
    spectres.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopRight,
        anchor: None,
    };
    spectres.series.push(SeriesSpec {
        name: "Échantillon µ1".to_string(),
        line: ligne("#4c78a8", LineStyle::Solid, 2.0),
        ..SeriesSpec::new_xy(id_lambda, id_a)
    });
    spectres.series.push(SeriesSpec {
        name: "Témoin".to_string(),
        line: ligne("#f58518", LineStyle::Dash, 2.0),
        ..SeriesSpec::new_xy(id_lambda, id_b)
    });

    // -- (0,1) cinétique : une courbe à gauche, un nuage à droite.
    let temps: Vec<f64> = (0..=60).map(|i| i as f64 * 0.5).collect();
    let concentration: Vec<f64> = temps.iter().map(|t| 1.0 * (-t / 8.0).exp()).collect();
    let temperature: Vec<f64> = temps
        .iter()
        .map(|t| 20.0 + 6.0 * (t / 4.0).sin() + t / 12.0)
        .collect();
    let id_temps = column(&mut project, sheet, "Temps", &temps)?;
    let id_conc = column(&mut project, sheet, "Concentration", &concentration)?;
    let id_temp = column(&mut project, sheet, "Temperature", &temperature)?;

    let mut cinetique = PanelSpec::at(0, 1);
    cinetique.title = "Cinétique".to_string();
    cinetique.bottom = axis("t", "min");
    cinetique.left = axis("c", "mol·L⁻¹");
    cinetique.right = axis("θ", "°C");
    cinetique.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopRight,
        anchor: None,
    };
    cinetique.series.push(SeriesSpec {
        name: "Concentration".to_string(),
        line: ligne("#54a24b", LineStyle::Solid, 2.0),
        ..SeriesSpec::new_xy(id_temps, id_conc)
    });
    cinetique.series.push(SeriesSpec {
        name: "Température".to_string(),
        plot_type: PlotType::Scatter,
        axis: SeriesAxis::Right,
        symbol: marqueur("#e45756", SymbolShape::Diamond, 6.0),
        line: ligne("#e45756", LineStyle::None, 1.0),
        ..SeriesSpec::new_xy(id_temps, id_temp)
    });

    // -- (0,2) étalonnage : ordonnée logarithmique et barres d'erreur.
    let dose: Vec<f64> = (0..9).map(|i| 1.0 + i as f64 * 2.0).collect();
    let reponse: Vec<f64> = dose.iter().map(|d| 2.0 * 10f64.powf(d / 8.0)).collect();
    let incertitude: Vec<f64> = reponse.iter().map(|r| r * 0.18).collect();
    let id_dose = column(&mut project, sheet, "Dose", &dose)?;
    let id_reponse = column(&mut project, sheet, "Reponse", &reponse)?;
    let id_incertitude = column(&mut project, sheet, "Incertitude", &incertitude)?;

    let mut etalonnage = PanelSpec::at(0, 2);
    etalonnage.title = "Étalonnage".to_string();
    etalonnage.bottom = axis("dose", "µg");
    etalonnage.left = AxisSpec {
        scale: AxisScale::Log,
        ..axis("réponse", "cps")
    };
    etalonnage.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopLeft,
        anchor: None,
    };
    etalonnage.series.push(SeriesSpec {
        name: "Série A".to_string(),
        plot_type: PlotType::ErrorBar,
        y_error: Some(id_incertitude),
        line: ligne("#72b7b2", LineStyle::Solid, 1.0),
        symbol: marqueur("#72b7b2", SymbolShape::Square, 7.0),
        ..SeriesSpec::new_xy(id_dose, id_reponse)
    });

    // -- (1,0) répartition : un histogramme.
    let residus: Vec<f64> = (0..400)
        .map(|i| {
            // Trois suites équiréparties de pas irrationnels **distincts**,
            // sommées : une cloche grossière, parfaitement déterministe.
            // Aucun générateur aléatoire ne doit entrer dans un graphe dont
            // un test compare la sortie au pixel près. Les pas ne doivent
            // pas non plus se compléter à un entier, faute de quoi leurs
            // parties fractionnaires se compensent et tout l'échantillon
            // tombe dans une seule classe — erreur commise puis corrigée
            // ici, et visible d'un coup d'œil sur l'histogramme.
            let i = i as f64;
            let a = (i * 0.754_877_666_2) % 1.0;
            let b = (i * 0.569_840_290_9) % 1.0;
            let c = (i * 0.324_717_957_2) % 1.0;
            (a + b + c - 1.5) * 2.5
        })
        .collect();
    let id_residus = column(&mut project, sheet, "Residus", &residus)?;

    let mut repartition = PanelSpec::at(1, 0);
    repartition.title = "Répartition".to_string();
    repartition.bottom = axis("écart", "u.a.");
    repartition.left = axis("effectif", "");
    repartition.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopLeft,
        anchor: None,
    };
    repartition.series.push(SeriesSpec {
        name: "Résidus".to_string(),
        plot_type: PlotType::Histogram,
        line: ligne("#b279a2", LineStyle::Solid, 1.0),
        ..SeriesSpec::new(id_residus)
    });

    // -- (1,1) rendement : des barres, un nuage relié, et une référence
    //    brisée qui doit se voir.
    let lot: Vec<f64> = (1..=7).map(|i| i as f64).collect();
    let rendement: Vec<f64> = [72.0, 81.0, 68.0, 90.0, 77.0, 85.0, 79.0].to_vec();
    let id_lot = column(&mut project, sheet, "Lot", &lot)?;
    let id_rendement = column(&mut project, sheet, "Rendement", &rendement)?;
    let disparue = column(&mut project, sheet, "Disparue", &[1.0, 2.0])?;
    project.remove(disparue)?;

    let mut rendements = PanelSpec::at(1, 1);
    rendements.title = "Rendement par lot".to_string();
    rendements.bottom = axis("lot", "");
    rendements.left = axis("rendement", "%");
    rendements.legend = LegendSpec {
        visible: true,
        position: LegendPosition::BottomRight,
        anchor: None,
    };
    rendements.series.push(SeriesSpec {
        name: "Rendement".to_string(),
        plot_type: PlotType::Bar,
        line: ligne("#eeca3b", LineStyle::Solid, 1.0),
        ..SeriesSpec::new_xy(id_lot, id_rendement)
    });
    rendements.series.push(SeriesSpec {
        name: "Campagne précédente".to_string(),
        plot_type: PlotType::LineScatter,
        line: ligne("#9d755d", LineStyle::DashDot, 1.0),
        symbol: marqueur("#9d755d", SymbolShape::Triangle, 7.0),
        ..SeriesSpec::new(disparue)
    });

    let spec = PlotSpec {
        title: "Campagne du 12 septembre — série µ".to_string(),
        background: "#ffffff".to_string(),
        ..PlotSpec::empty("")
    }
    .with_panels(Panels::new([
        spectres,
        cinetique,
        etalonnage,
        repartition,
        rendements,
    ]));

    Ok(Sample { project, spec })
}

/// Une gaussienne, pour fabriquer des spectres qui ressemblent à des
/// spectres.
fn gaussienne(x: f64, centre: f64, largeur: f64, hauteur: f64) -> f64 {
    let u = (x - centre) / largeur;
    hauteur * (-u * u).exp()
}

fn axis(label: &str, unit: &str) -> AxisSpec {
    AxisSpec {
        label: label.to_string(),
        unit: unit.to_string(),
        grid: true,
        ..AxisSpec::default()
    }
}

fn ligne(color: &str, style: LineStyle, width: f64) -> LineSpec {
    LineSpec {
        color: color.to_string(),
        style,
        width,
    }
}

fn marqueur(color: &str, shape: SymbolShape, size: f64) -> SymbolSpec {
    SymbolSpec {
        shape,
        size,
        color: color.to_string(),
        edge_color: String::new(),
    }
}

fn worksheet(project: &mut Project, book: &str, sheet: &str) -> CoreResult<NodeId> {
    let root = project.root();
    let book = project.add_empty_workbook(root, Some(book))?;
    project.add_worksheet(book, Some(sheet))
}

fn column(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> CoreResult<NodeId> {
    let column = Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )?;
    project.add_column(sheet, None, Some(name), column)
}
