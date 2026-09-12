//! Ce que l'export doit produire, indépendamment du test d'accord.
//!
//! Le test d'accord (`plotyx-render-egui/tests/accord.rs`) prouve que les
//! deux backends sont *d'accord*. Il ne prouve pas que ce sur quoi ils
//! s'accordent est un document SVG valide et complet — deux figures vides
//! seraient parfaitement d'accord. D'où ce fichier.

use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::{NodeId, Project, Tree};
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render::TextMeasurer;
use plotyx_render_svg::{naive_estimate, render_spec, render_to_file, render_to_string};

struct Projet {
    project: Project,
    spec: PlotSpec,
}

fn projet() -> Projet {
    let mut project = Project::new("Export").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let xs: Vec<f64> = (0..40).map(|i| i as f64 * 0.25).collect();
    let ys: Vec<f64> = xs.iter().map(|x| x.exp() / 10.0).collect();
    let x = project
        .add_column(sheet, None, Some("X"), numeric(&xs))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), numeric(&ys))
        .expect("Y");

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "Temps".into(),
        unit: "s".into(),
        grid: true,
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Réponse".into(),
        unit: "µV".into(),
        grid: true,
        ..AxisSpec::default()
    };
    let mut series = SeriesSpec::new_xy(x, y);
    series.line = LineSpec {
        color: "#e45756".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(series);

    let spec = PlotSpec {
        title: "Réponse impulsionnelle".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);
    Projet { project, spec }
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne")
}

fn tree(projet: &Projet) -> &Tree {
    projet.project.tree()
}

#[test]
fn le_document_porte_la_figure_entiere() {
    let projet = projet();
    let document = render_spec(
        tree(&projet),
        &projet.spec,
        (800, 600),
        &TextMeasurer::default(),
    )
    .expect("export");

    assert!(document.starts_with("<svg"), "en-tête SVG absent");
    assert!(
        document.trim_end().ends_with("</svg>"),
        "document non refermé"
    );
    assert!(document.contains("width=\"800\""));
    assert!(document.contains("height=\"600\""));

    // Le titre, les libellés d'axe et la couleur de la série. Le non-ASCII
    // est vérifié tel quel : `plotters-svg` échappe `<`, `>` et `&`, pas
    // l'UTF-8, et un jour où il le ferait il faudrait s'en apercevoir.
    assert!(document.contains("Réponse impulsionnelle"));
    assert!(document.contains("Réponse (µV)"));
    assert!(document.contains("Temps (s)"));
    assert!(
        document.contains("#E45756"),
        "la couleur de la série n'est pas dans le document"
    );
    assert!(
        document.matches("<polyline").count() > 5,
        "trop peu de polylignes : la grille ou la courbe manque"
    );
}

#[test]
fn une_serie_brisee_ne_vide_pas_le_document() {
    // §11.1 : une référence brisée est un état, pas une panne. L'export doit
    // sortir la figure quand même.
    let mut projet = projet();
    if let Some(panel) = projet.spec.panels.get_mut(0) {
        panel
            .series
            .push(SeriesSpec::new(NodeId::from_raw(u64::MAX)));
    }
    let plot = resolve_spec(tree(&projet), &projet.spec);
    assert_eq!(plot.broken_series().count(), 1);

    let document = render_to_string(&plot, (640, 480), &TextMeasurer::default()).expect("export");
    assert!(document.contains("Réponse impulsionnelle"));
    assert!(document.contains("#E45756"), "la série valide est tracée");
}

#[test]
fn ecriture_sur_disque() {
    let projet = projet();
    let plot = resolve_spec(tree(&projet), &projet.spec);
    let dossier = tempfile::tempdir().expect("dossier temporaire");
    let cible = dossier.path().join("figure.svg");
    render_to_file(&cible, &plot, (400, 300), &TextMeasurer::default()).expect("écriture");
    let contenu = std::fs::read_to_string(&cible).expect("relecture");
    assert!(contenu.starts_with("<svg"));
    assert!(contenu.len() > 500, "document suspicieusement court");
}

#[test]
fn le_temoin_naif_reproduit_bien_lheuristique_de_plotters() {
    // `naive_estimate` n'existe que pour servir de témoin. Un témoin faux
    // est pire qu'aucun témoin : il ferait croire à une différence là où il
    // n'y en a pas. Les deux propriétés qui le caractérisent sont donc
    // vérifiées ici — largeur proportionnelle au **nombre d'octets**, et
    // insensibilité au dessin réel des glyphes.
    assert_eq!(naive_estimate("111", 20.0), naive_estimate("WWW", 20.0));
    assert_eq!(
        naive_estimate("λ", 20.0),
        naive_estimate("ab", 20.0),
        "« λ » fait deux octets : l'heuristique doit le compter comme deux \
         caractères, c'est tout son défaut"
    );
    let (largeur, hauteur) = naive_estimate("0,5", 20.0);
    assert!(largeur > 0 && hauteur > 0);
}
