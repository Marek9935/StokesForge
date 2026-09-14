//! Batterie 13 — ce qu'un `PlotSpec` venu du disque peut demander au rendu.
//!
//! `plotyx-plot` a rendu « zéro classe » inexprimable (`histogram_bins:
//! NonZeroU32`), et `plotyx-persistence` refuse le zéro d'un fichier
//! (`ConvertError::ZeroHistogramBins`). L'autre bout de l'intervalle a
//! longtemps été le seul non borné — ni le type, ni la conversion. B4
//! (`guide-de-correction-robustesse.md` §F4) a fermé les deux :
//! `plotyx_plot::BinCount` rend l'excès inexprimable au niveau du type
//! (`b13_le_type_lui_meme_borne_le_nombre_de_classes`), et
//! `plotyx_persistence::convert::record_to_series_spec` refuse à la lecture
//! une valeur brute qui dépasse la même borne
//! (`b13_histogram_bins_venu_du_disque_est_borne`) — nécessaire séparément,
//! puisqu'un fichier hostile écrit par un autre outil n'est pas tenu de
//! passer par `BinCount` pour exister sur disque.

use std::num::NonZeroU32;

use plotyx_core::{Column, Project};
use plotyx_plot::{BinCount, PanelSpec, Panels, PlotSpec, PlotType, SeriesSpec};
use tempfile::TempDir;

/// Ordre de grandeur au-delà duquel un nombre de classes ne décrit plus un
/// histogramme mais un fichier corrompu. Un écran fait quelques milliers de
/// pixels de large ; 65 536 classes sont déjà dix fois trop — c'est aussi
/// [`plotyx_plot::MAX_HISTOGRAM_BINS`], vérifié plus bas.
const PLAFOND_RAISONNABLE: u32 = 1 << 16;

// -- garde-fous -----------------------------------------------------------

#[test]
fn b13_un_nombre_de_classes_ordinaire_traverse_le_disque_a_lidentique() {
    let bins = BinCount::new(40).expect("40 est dans les bornes");
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
    let relu = plotyx_persistence::load(&chemin).expect("rechargement");
    let fig = relu.resolve_path("[Fig1]").expect("graphe présent");
    let node = relu.tree().node(fig).expect("nœud présent");
    let bins_relus = match node.kind() {
        plotyx_core::NodeKind::Graph { spec, .. } => spec.panels[0].series[0].histogram_bins,
        autre => panic!("attendu un graphe, obtenu {}", autre.type_name()),
    };
    assert_eq!(bins_relus, bins);
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

#[test]
fn b13_le_plafond_du_type_est_bien_celui_documente() {
    assert_eq!(plotyx_plot::MAX_HISTOGRAM_BINS, PLAFOND_RAISONNABLE);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b13_le_type_lui_meme_borne_le_nombre_de_classes() {
    // B4 — `BinCount` (contrairement au `NonZeroU32` nu d'avant ce correctif)
    // rend un excès inexprimable : la construction elle-même échoue, il n'y a
    // plus de valeur absurde à inspecter après coup.
    let trop_grand = BinCount::new(u32::MAX);
    println!("BinCount::new(u32::MAX) = {trop_grand:?}");
    assert!(
        trop_grand.is_none(),
        "le type doit refuser de représenter {} classes, au-delà du plafond {}",
        u32::MAX,
        PLAFOND_RAISONNABLE
    );
    let limite = BinCount::new(PLAFOND_RAISONNABLE);
    assert!(
        limite.is_some(),
        "le plafond lui-même doit rester constructible"
    );
    assert!(
        BinCount::new(PLAFOND_RAISONNABLE + 1).is_none(),
        "un classe de plus que le plafond doit être refusée"
    );
}

/// Schéma sur disque redéclaré à l'identique pour fabriquer un fichier
/// hostile — voir `b3_fichiers_hostiles.rs`, qui documente pourquoi c'est
/// légitime (format auto-descriptif : deux déclarations d'accord sur les
/// noms produisent les mêmes octets). Réduit aux seuls champs qu'un graphe à
/// un histogramme nécessite.
mod hostile {
    use serde::Serialize;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[derive(Serialize)]
    pub(super) struct FileBody {
        pub root: u64,
        pub next_id_hint: u64,
        pub nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    pub(super) struct NodeRecord {
        pub id: u64,
        pub name: String,
        pub parent: Option<u64>,
        pub kind: KindRecord,
    }
    #[derive(Serialize)]
    pub(super) enum KindRecord {
        Folder {
            children: Vec<u64>,
        },
        Workbook {
            sheets: Vec<u64>,
            label: String,
            comment: String,
        },
        Worksheet {
            columns: Vec<u64>,
        },
        Graph {
            label: String,
            comment: String,
            spec: PlotSpecRecord,
        },
    }
    #[derive(Serialize, Default)]
    pub(super) struct PlotSpecRecord {
        pub title: String,
        pub panels: Vec<PanelSpecRecord>,
        pub background: String,
        pub format_version: u32,
    }
    #[derive(Serialize, Default)]
    pub(super) struct PanelSpecRecord {
        pub title: String,
        pub series: Vec<SeriesSpecRecord>,
        pub bottom: AxisSpecRecord,
        pub left: AxisSpecRecord,
        pub right: AxisSpecRecord,
        pub top: AxisSpecRecord,
        pub legend: LegendSpecRecord,
        pub row: u64,
        pub column: u64,
    }
    #[derive(Serialize)]
    pub(super) struct AxisSpecRecord {
        pub label: String,
        pub unit: String,
        pub scale: AxisScaleRecord,
        pub auto_range: bool,
        pub range_min: Option<f64>,
        pub range_max: Option<f64>,
        pub grid: bool,
        pub visible: bool,
        pub inverted: bool,
    }
    impl Default for AxisSpecRecord {
        fn default() -> Self {
            Self {
                label: String::new(),
                unit: String::new(),
                scale: AxisScaleRecord::Linear,
                auto_range: true,
                range_min: None,
                range_max: None,
                grid: false,
                visible: true,
                inverted: false,
            }
        }
    }
    #[derive(Serialize)]
    pub(super) enum AxisScaleRecord {
        Linear,
    }
    #[derive(Serialize)]
    pub(super) struct SeriesSpecRecord {
        pub y: u64,
        pub x: Option<u64>,
        pub x_error: Option<u64>,
        pub y_error: Option<u64>,
        pub plot_type: PlotTypeRecord,
        pub name: String,
        pub line: LineSpecRecord,
        pub symbol: SymbolSpecRecord,
        pub axis: SeriesAxisRecord,
        pub visible: bool,
        pub show_in_legend: bool,
        pub bar_width: Option<f64>,
        pub histogram_bins: u32,
    }
    impl Default for SeriesSpecRecord {
        fn default() -> Self {
            Self {
                y: 1,
                x: None,
                x_error: None,
                y_error: None,
                plot_type: PlotTypeRecord::Histogram,
                name: String::new(),
                line: LineSpecRecord::default(),
                symbol: SymbolSpecRecord::default(),
                axis: SeriesAxisRecord::Left,
                visible: true,
                show_in_legend: true,
                bar_width: None,
                histogram_bins: 10,
            }
        }
    }
    #[derive(Serialize)]
    pub(super) enum PlotTypeRecord {
        Histogram,
    }
    #[derive(Serialize)]
    pub(super) enum SeriesAxisRecord {
        Left,
    }
    #[derive(Serialize)]
    pub(super) struct LineSpecRecord {
        pub color: String,
        pub style: LineStyleRecord,
        pub width: f64,
    }
    impl Default for LineSpecRecord {
        fn default() -> Self {
            Self {
                color: String::new(),
                style: LineStyleRecord::Solid,
                width: 1.0,
            }
        }
    }
    #[derive(Serialize)]
    pub(super) enum LineStyleRecord {
        Solid,
    }
    #[derive(Serialize)]
    pub(super) struct SymbolSpecRecord {
        pub shape: SymbolShapeRecord,
        pub size: f64,
        pub color: String,
        pub edge_color: String,
    }
    impl Default for SymbolSpecRecord {
        fn default() -> Self {
            Self {
                shape: SymbolShapeRecord::Circle,
                size: 5.0,
                color: String::new(),
                edge_color: String::new(),
            }
        }
    }
    #[derive(Serialize)]
    pub(super) enum SymbolShapeRecord {
        Circle,
    }
    #[derive(Serialize)]
    pub(super) struct LegendSpecRecord {
        pub visible: bool,
        pub position: LegendPositionRecord,
    }
    impl Default for LegendSpecRecord {
        fn default() -> Self {
            Self {
                visible: true,
                position: LegendPositionRecord::TopRight,
            }
        }
    }
    #[derive(Serialize)]
    pub(super) enum LegendPositionRecord {
        TopRight,
    }

    pub(super) fn ecrire(dir: &TempDir, nom: &str, body: &FileBody) -> PathBuf {
        let payload = rmp_serde::to_vec_named(body).unwrap();
        let path = dir.path().join(nom);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"PLYX").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap();
        f.write_all(&payload).unwrap();
        f.sync_all().unwrap();
        path
    }

    /// Un fichier valide portant un seul graphe, dont l'unique série est un
    /// histogramme à `bins` classes brutes — pas de `BinCount` ici, ce module
    /// simule un fichier écrit par un outil qui n'en a jamais entendu parler.
    pub(super) fn projet_avec_histogram_bins(dir: &TempDir, bins: u32) -> PathBuf {
        let serie = SeriesSpecRecord {
            histogram_bins: bins,
            ..SeriesSpecRecord::default()
        };
        let panneau = PanelSpecRecord {
            series: vec![serie],
            ..PanelSpecRecord::default()
        };
        let spec = PlotSpecRecord {
            format_version: 1,
            panels: vec![panneau],
            ..PlotSpecRecord::default()
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 5,
            nodes: vec![
                NodeRecord {
                    id: 1,
                    name: "R".to_string(),
                    parent: None,
                    kind: KindRecord::Folder {
                        children: vec![2, 4],
                    },
                },
                NodeRecord {
                    id: 2,
                    name: "Book1".to_string(),
                    parent: Some(1),
                    kind: KindRecord::Workbook {
                        sheets: vec![3],
                        label: String::new(),
                        comment: String::new(),
                    },
                },
                NodeRecord {
                    id: 3,
                    name: "Sheet1".to_string(),
                    parent: Some(2),
                    kind: KindRecord::Worksheet { columns: vec![] },
                },
                NodeRecord {
                    id: 4,
                    name: "Fig1".to_string(),
                    parent: Some(1),
                    kind: KindRecord::Graph {
                        label: String::new(),
                        comment: String::new(),
                        spec,
                    },
                },
            ],
        };
        ecrire(dir, "histogramme_hostile.plx", &body)
    }
}

#[test]
fn b13_histogram_bins_venu_du_disque_est_borne() {
    let dir = TempDir::new().expect("répertoire temporaire");
    // `BinCount` empêche désormais d'écrire ce scénario via l'API publique
    // (voir `b13_le_type_lui_meme_borne_le_nombre_de_classes`) : un fichier
    // hostile écrit par un outil tiers, lui, n'est pas tenu de passer par
    // `BinCount` pour exister — d'où le fichier hostile forgé ici.
    let chemin = hostile::projet_avec_histogram_bins(&dir, u32::MAX);
    let resultat = plotyx_persistence::load(&chemin);
    println!(
        "chargement d'un fichier avec histogram_bins = {} → {resultat:?}",
        u32::MAX
    );
    assert!(
        resultat.is_err(),
        "un histogram_bins de {} (bien au-delà du plafond {PLAFOND_RAISONNABLE}) doit être \
         refusé au chargement plutôt que de laisser le rendu demander l'allocation qui \
         ferait abort() le processus (`cargo run --bin plotyx-sonde-alloc -- histogram_bins \
         4294967295`)",
        u32::MAX
    );
}
