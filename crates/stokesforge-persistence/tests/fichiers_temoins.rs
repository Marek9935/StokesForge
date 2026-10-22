//! Fichiers témoins du format `.stf` (S4, #323).
//!
//! `temoins/v1/projet.stf` est ce que la V1 écrit pour le projet de
//! [`projet_temoin`], qui exerce tous les types de nœuds, de colonnes et de
//! séries, avec des valeurs hors défaut partout (un champ renommé qui retombe
//! sur sa valeur par défaut fait échouer la comparaison). Deux gardes :
//!
//! - **relecture** : le fichier se relit et rend le projet attendu, avec ses
//!   tables de vue et un rapport propre ;
//! - **dérive** : enregistrer le projet donne exactement les octets du
//!   fichier. Un changement du format écrit passe donc par une décision : le
//!   numéro de version monte (l'ancien témoin se fige dans son dossier
//!   `vN/`), ou le changement est additif et le témoin se régénère.
//!
//! Régénérer (geste explicite) :
//! `STOKESFORGE_REGENERER_TEMOINS=1 cargo test -p stokesforge-persistence --test fichiers_temoins regenerer -- --ignored`

mod commun;

use std::collections::HashMap;
use std::path::PathBuf;

use commun::{fixture_project, non_trivial_spec, recipe_hors_defaut};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::{Operation, Warning};
use stokesforge_core::{
    AuxRef, CalculationDraft, Column, ColumnMeta, ColumnRole, ImportSource, MetaLine, NodeId,
    OutputDraft, Project, ResultValue, SeriesRef, SeriesResult,
};
use stokesforge_persistence::{
    load_with_report, save_with_geometries, ViewTables, WindowGeometry, WindowMode, WindowView,
};
use stokesforge_plot::{ColorUsage, PanelSpec, PlotSpec, PlotType, SavedColors, SeriesSpec};

fn dossier_temoins() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/temoins")
}

fn fichier_v1() -> PathBuf {
    dossier_temoins().join("v1/projet.stf")
}

fn ajouter(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> NodeId {
    project
        .add_column(sheet, None, Some(name), Column::from_f64(values))
        .expect("colonne ajoutée")
}

fn versions(project: &Project, ids: &[NodeId]) -> Vec<(NodeId, u64)> {
    ids.iter()
        .map(|id| {
            let column = project.tree().get(*id).ok().and_then(|n| n.as_column());
            (*id, column.expect("colonne présente").version())
        })
        .collect()
}

/// Le projet témoin et ses tables de vue. Toute modification de cette
/// fonction change les octets attendus : à régénérer.
fn projet_temoin() -> (Project, ViewTables) {
    let mut project = fixture_project();
    let root = project.root();

    // Un classeur de mesures : un rôle par colonne, une formule, une source
    // d'import, des métadonnées de fichier.
    let book = project
        .add_workbook(root, Some("Mesures"))
        .expect("classeur");
    let sheet = project.tree().children(book).expect("feuille")[0];
    let x = ajouter(&mut project, sheet, "X", &[0.0, 1.0, 2.0]);
    let y1 = ajouter(&mut project, sheet, "Y1", &[1.0, 2.0, 3.0]);
    let y2 = ajouter(&mut project, sheet, "Y2", &[4.0, 5.0, 6.0]);
    let roles = [
        ColumnRole::X2,
        ColumnRole::Y2,
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
        ColumnRole::Z,
        ColumnRole::XError,
        ColumnRole::YError,
        ColumnRole::Label,
        ColumnRole::None,
    ];
    for (i, role) in roles.into_iter().enumerate() {
        let id = ajouter(&mut project, sheet, &format!("R{i}"), &[i as f64]);
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    role,
                    ..Default::default()
                },
            )
            .expect("rôle posé");
    }
    let formule = project
        .tree()
        .formula_from_letters(y2, "A*2+B")
        .expect("formule saisie");
    let calculee = ajouter(&mut project, sheet, "Calculee", &[0.0, 0.0, 0.0]);
    project
        .set_column_meta(
            calculee,
            ColumnMeta {
                formula: Some(formule),
                unit: "u".into(),
                long_name: "Calculée".into(),
                comment: "par formule".into(),
                display_format: "0.00".into(),
                x_prime: Some(-1.5),
                file_metadata: vec![
                    MetaLine {
                        key: "[Section]".into(),
                        value: String::new(),
                    },
                    MetaLine {
                        key: "XUNITS".into(),
                        value: "NANOMETERS".into(),
                    },
                    MetaLine {
                        key: String::new(),
                        value: "ligne libre".into(),
                    },
                ],
                import_source: Some(ImportSource {
                    path: "/data/mesure.asc".into(),
                    recipe_json: recipe_hors_defaut().to_json(),
                    column: 3,
                    block: 2,
                }),
                ..Default::default()
            },
        )
        .expect("métadonnées posées");

    // Graphes : le spec non trivial, et un graphe avec chaque type de série.
    project
        .add_graph(root, Some("Fig1"), non_trivial_spec(x, y1, y2))
        .expect("graphe");
    let series = [
        PlotType::Scatter,
        PlotType::Line,
        PlotType::LineScatter,
        PlotType::Bar,
        PlotType::Histogram,
        PlotType::ErrorBar,
    ]
    .into_iter()
    .map(|plot_type| SeriesSpec {
        plot_type,
        ..SeriesSpec::new_xy(x, y1)
    })
    .collect::<Vec<_>>();
    let tous_les_types = PlotSpec::empty("Tous les types").with_panels(vec![PanelSpec {
        series,
        ..PanelSpec::at(0, 0)
    }]);
    project
        .add_graph(root, Some("Fig2"), tous_les_types)
        .expect("graphe");

    // Un calcul actif, puis une correction qui remplace ses données par une
    // référence figée (classeur caché, trace, original).
    let calcul_book = project
        .add_workbook(root, Some("Calculs"))
        .expect("classeur");
    let calcul_sheet = project.tree().children(calcul_book).expect("feuille")[0];
    let cx = ajouter(&mut project, calcul_sheet, "X", &[1.0, 2.0, 3.0]);
    let cy = ajouter(&mut project, calcul_sheet, "Y", &[10.0, 20.0, 30.0]);
    let rx = ajouter(&mut project, calcul_sheet, "RX", &[1.0, 2.0, 3.0]);
    let ry = ajouter(&mut project, calcul_sheet, "RY", &[2.0, 2.0, 2.0]);
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: Some(cx),
                y: cy,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![9.0, 18.0, f64::NAN],
            }],
            results: vec![SeriesResult {
                values: vec![
                    ResultValue {
                        key: "count.outside".into(),
                        index: None,
                        value: 1.0,
                        error: None,
                    },
                    ResultValue {
                        key: "param.a".into(),
                        index: Some(2),
                        value: 0.25,
                        error: Some(0.5),
                    },
                ],
                skipped: 3,
                warnings: vec![Warning::NegativeClipped(4), Warning::IrfStepMismatch],
            }],
            keep_results: true,
            auto_recalc: true,
            low_r2_dialog: false,
        })
        .expect("calcul");
    let remplacees = versions(&project, &[rx, ry, cx, cy]);
    project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Divide,
                },
                series: vec![SeriesRef {
                    x: Some(rx),
                    y: ry,
                    sigma: None,
                }],
                aux: Some(AuxRef { x: Some(cx), y: cy }),
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![5.0, 10.0, 15.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &remplacees,
        )
        .expect("correction");

    // Une feuille transposée liée.
    let source_book = project
        .add_workbook(root, Some("Spectres"))
        .expect("classeur");
    let source = project.tree().children(source_book).expect("feuille")[0];
    for i in 0..3 {
        let mut column = Column::from_f64(&[1.0 + f64::from(i), 5.0]);
        column.set_meta(ColumnMeta {
            x_prime: Some(f64::from(i) * 10.0),
            ..ColumnMeta::default()
        });
        project
            .add_column(source, None, Some(&format!("C{i}")), column)
            .expect("colonne");
    }
    project.transpose_rows(source, &[0, 1]).expect("transposée");

    project.set_saved_colors(
        SavedColors::default()
            .with_added(ColorUsage::Curves, "#ff0000")
            .with_added(ColorUsage::Text, "#20202080")
            .with_added(ColorUsage::Background, "#ffffff00")
            .with_gradient_added("Chaud", &["#000000".to_string(), "#ff0000".to_string()]),
    );

    // Tables de vue : plus de deux fenêtres, pour que l'ordre compte.
    let mut window_geometries = HashMap::new();
    let mut column_widths = HashMap::new();
    for (rang, id) in project.tree().walk_all().into_iter().take(20).enumerate() {
        window_geometries.insert(
            id,
            WindowGeometry {
                x: rang as f64 * 3.5,
                y: rang as f64 + 0.25,
                width: 420.0,
                height: 310.5,
                view: Some(WindowView {
                    mode: [WindowMode::Open, WindowMode::Maximized, WindowMode::Closed][rang % 3],
                    collapsed: rang % 2 == 0,
                    stack: rang as u32,
                }),
            },
        );
        column_widths.insert(id, 40.0 + rang as f32 * 1.5);
    }
    (
        project,
        ViewTables {
            window_geometries,
            column_widths,
        },
    )
}

fn octets_du_projet_temoin() -> Vec<u8> {
    let (project, views) = projet_temoin();
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("projet.stf");
    save_with_geometries(&project, &views, &path).expect("enregistrement");
    std::fs::read(&path).expect("lecture")
}

#[test]
fn le_temoin_v1_se_relit_et_rend_le_projet_attendu() {
    let (attendu, vues_attendues) = projet_temoin();
    let (relu, vues, rapport) = load_with_report(&fichier_v1()).expect("le témoin V1 se relit");
    assert!(rapport.is_clean(), "{rapport:?}");
    assert!(
        *relu.tree() == *attendu.tree(),
        "l'arbre relu diffère du projet attendu"
    );
    assert_eq!(relu.next_id_hint(), attendu.next_id_hint());
    assert_eq!(vues.window_geometries, vues_attendues.window_geometries);
    assert_eq!(vues.column_widths, vues_attendues.column_widths);
}

#[test]
fn le_format_ecrit_ne_derive_pas_sans_changement_de_version() {
    let fichier = std::fs::read(fichier_v1()).expect("témoin présent");
    assert!(
        octets_du_projet_temoin() == fichier,
        "le format écrit a changé : si c'est voulu, monter le numéro de version \
         (retrait, renommage, changement de sens) ou régénérer le témoin \
         (ajout), voir docs/persistance.md, « Règle de version »"
    );
}

#[test]
#[ignore = "geste explicite : régénère le témoin du format courant"]
fn regenerer_le_temoin_courant() {
    assert!(
        std::env::var_os("STOKESFORGE_REGENERER_TEMOINS").is_some(),
        "poser STOKESFORGE_REGENERER_TEMOINS=1"
    );
    let chemin = fichier_v1();
    std::fs::create_dir_all(chemin.parent().expect("dossier")).expect("création");
    std::fs::write(&chemin, octets_du_projet_temoin()).expect("écriture");
}
