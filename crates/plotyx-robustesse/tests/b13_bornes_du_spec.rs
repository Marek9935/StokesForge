//! Batterie 13 — ce qu'un `PlotSpec` venu du disque peut demander au rendu.
//!
//! `plotyx-plot` a rendu « zéro classe » inexprimable (`histogram_bins:
//! NonZeroU32`), et `plotyx-persistence` refuse le zéro d'un fichier
//! (`ConvertError::ZeroHistogramBins`). L'autre bout de l'intervalle, lui,
//! n'est borné nulle part : ni le type, ni la conversion, ni la fonction qui
//! le consomme.
//!
//! Le plafond dont il est question ici n'existe pas encore ; la constante
//! ci-dessous dit ce qu'il devrait valoir, pas ce que le dépôt applique.

use std::num::NonZeroU32;

use plotyx_core::{Column, NodeKind, Project};
use plotyx_plot::{PanelSpec, Panels, PlotSpec, PlotType, SeriesSpec};
use tempfile::TempDir;

/// Ordre de grandeur au-delà duquel un nombre de classes ne décrit plus un
/// histogramme mais un fichier corrompu. Un écran fait quelques milliers de
/// pixels de large ; 65 536 classes sont déjà dix fois trop.
const PLAFOND_RAISONNABLE: u32 = 1 << 16;

/// Un projet d'un graphe, dont l'unique série est un histogramme à `bins`
/// classes, passé par le disque.
fn aller_retour(bins: NonZeroU32) -> (Project, u64) {
    let dir = TempDir::new().expect("répertoire temporaire");
    let chemin = dir.path().join("histogramme.plx");

    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne Y");

    let serie = SeriesSpec {
        plot_type: PlotType::Histogram,
        histogram_bins: bins,
        ..SeriesSpec::new(y)
    };
    let panneau = PanelSpec {
        series: vec![serie],
        ..PanelSpec::default()
    };
    project
        .add_graph(
            root,
            Some("Fig1"),
            PlotSpec::default().with_panels(Panels::one(panneau)),
        )
        .expect("graphe");

    plotyx_persistence::save(&project, &chemin).expect("sauvegarde");
    let octets = std::fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
    let relu = plotyx_persistence::load(&chemin).expect("rechargement");
    (relu, octets)
}

/// Nombre de classes de la première série du graphe `[Fig1]`.
fn bins_de(project: &Project) -> NonZeroU32 {
    let fig = project.resolve_path("[Fig1]").expect("graphe présent");
    let node = project.tree().node(fig).expect("nœud présent");
    match node.kind() {
        NodeKind::Graph { spec, .. } => spec.panels[0].series[0].histogram_bins,
        autre => panic!("attendu un graphe, obtenu {}", autre.type_name()),
    }
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b13_un_nombre_de_classes_ordinaire_traverse_le_disque_a_lidentique() {
    let bins = NonZeroU32::new(40).expect("non nul");
    let (relu, _) = aller_retour(bins);
    assert_eq!(bins_de(&relu), bins);
}

#[test]
fn b13_un_histogramme_ordinaire_se_calcule_sans_incident() {
    let (edges, counts) = plotyx_core::resolve::histogram(
        &[1.0, 2.0, 3.0, 4.0],
        NonZeroU32::new(4).expect("non nul"),
    );
    assert_eq!(edges.len(), counts.len() + 1);
    assert_eq!(counts.iter().sum::<f64>(), 4.0);
}

// -- reproductions --------------------------------------------------------

#[test]
#[ignore = "reproduction B4 — histogram_bins n'est borné ni par le type, ni par la conversion depuis le fichier : le rendu allouera 2 × 34 Gio et le processus fera abort()"]
fn b13_histogram_bins_venu_du_disque_est_borne() {
    let (relu, octets) = aller_retour(NonZeroU32::MAX);
    let bins = bins_de(&relu);
    let demande = u64::from(bins.get()) * 8 * 2; // `edges` (bins + 1) + `counts`
    println!("fichier de {octets} octets, relu sans le moindre avertissement");
    println!("histogram_bins = {bins} → {demande} octets demandés par `resolve::histogram`");
    println!(
        "l'abort() lui-même est hors test (un échec d'allocation Rust n'est pas \
         rattrapable) : `cargo run --bin plotyx-sonde-alloc -- histogram_bins 4294967295`"
    );
    assert!(
        bins.get() <= PLAFOND_RAISONNABLE,
        "un fichier de {octets} octets fait demander {demande} octets au moteur de rendu ; \
         `ConvertError::ZeroHistogramBins` montre que le réflexe de borner ce champ à la \
         lecture est déjà en place — il ne regarde qu'une des deux extrémités"
    );
}

#[test]
#[ignore = "reproduction B4 — même sans disque, rien n'empêche de construire un spec dont le rendu tuera le processus"]
fn b13_le_type_lui_meme_borne_le_nombre_de_classes() {
    // `NonZeroU32` retire le zéro et rien d'autre. La décision R2 (« les états
    // illégaux ne s'écrivent pas ») s'arrête à la borne basse.
    let serie = SeriesSpec {
        histogram_bins: NonZeroU32::MAX,
        ..SeriesSpec::new(plotyx_core::NodeId::from_raw(1))
    };
    println!(
        "histogram_bins constructible en mémoire : {}",
        serie.histogram_bins
    );
    assert!(
        serie.histogram_bins.get() <= PLAFOND_RAISONNABLE,
        "le type laisse écrire {} classes ; un `BinCount` borné le rendrait \
         inexprimable, exactement comme `NonZeroU32` a rendu le zéro inexprimable",
        serie.histogram_bins
    );
}
