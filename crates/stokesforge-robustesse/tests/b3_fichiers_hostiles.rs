//! Batterie 3 — fichiers .stf syntaxiquement valides (MessagePack bien formé,
//! schéma respecté) mais sémantiquement hostiles.
//!
//! Le schéma est privé à `stokesforge-persistence` ; on le redéclare ici à
//! l'identique. C'est légitime : le format est auto-descriptif (maps nommées),
//! donc deux déclarations d'accord sur les noms produisent les mêmes octets.

use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use stokesforge_persistence::load;
use tempfile::TempDir;

#[derive(Serialize)]
struct FileBody {
    root: u64,
    next_id_hint: u64,
    nodes: Vec<NodeRecord>,
}
#[derive(Serialize)]
struct NodeRecord {
    id: u64,
    name: String,
    parent: Option<u64>,
    kind: KindRecord,
}
#[derive(Serialize)]
enum KindRecord {
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
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
    },
    Graph {
        label: String,
        comment: String,
        spec: PlotSpecRecord,
    },
    #[allow(dead_code)]
    Matrix {
        label: String,
        comment: String,
    },
    #[allow(dead_code)]
    Note {
        label: String,
        comment: String,
        text: String,
    },
}
#[derive(Serialize)]
enum TypeRecord {
    Numeric,
    #[allow(dead_code)]
    DateTime,
}
#[derive(Serialize)]
enum RoleRecord {
    #[allow(dead_code)]
    X,
    Y,
}
#[derive(Serialize)]
enum ValuesRecord {
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    Mixed {
        #[serde(with = "serde_bytes")]
        tags: Vec<u8>,
        #[serde(with = "serde_bytes")]
        numbers: Vec<u8>,
        texts: Vec<String>,
    },
}
#[derive(Serialize, Default)]
struct PlotSpecRecord {
    title: String,
    panels: Vec<PanelSpecRecord>,
    background: String,
}
#[derive(Serialize, Default)]
struct PanelSpecRecord {
    title: String,
    series: Vec<SeriesSpecRecord>,
    bottom: AxisSpecRecord,
    left: AxisSpecRecord,
    right: AxisSpecRecord,
    top: AxisSpecRecord,
    legend: LegendSpecRecord,
    row: u64,
    column: u64,
}
// `Default` est dérivé : ces enregistrements servent à fabriquer des fichiers
// hostiles, pas à répliquer les valeurs par défaut de `stokesforge-plot` (l'axe du
// haut masqué, notamment, n'a pas à l'être ici).
#[derive(Serialize)]
struct AxisSpecRecord {
    label: String,
    unit: String,
    scale: AxisScaleRecord,
    auto_range: bool,
    range_min: Option<f64>,
    range_max: Option<f64>,
    grid: bool,
    visible: bool,
    inverted: bool,
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
enum AxisScaleRecord {
    Linear,
    #[allow(dead_code)]
    Log,
}
#[derive(Serialize)]
struct SeriesSpecRecord {
    y: u64,
    x: Option<u64>,
    x_error: Option<u64>,
    y_error: Option<u64>,
    plot_type: PlotTypeRecord,
    name: String,
    line: LineSpecRecord,
    symbol: SymbolSpecRecord,
    axis: SeriesAxisRecord,
    visible: bool,
    show_in_legend: bool,
    bar_width: Option<f64>,
    histogram_bins: u32,
}
impl Default for SeriesSpecRecord {
    fn default() -> Self {
        Self {
            y: 1,
            x: None,
            x_error: None,
            y_error: None,
            plot_type: PlotTypeRecord::Line,
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
enum PlotTypeRecord {
    #[allow(dead_code)]
    Scatter,
    Line,
    #[allow(dead_code)]
    Bar,
}
#[derive(Serialize)]
enum SeriesAxisRecord {
    Left,
    #[allow(dead_code)]
    Right,
}
#[derive(Serialize)]
struct LineSpecRecord {
    color: String,
    style: LineStyleRecord,
    width: f64,
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
enum LineStyleRecord {
    #[allow(dead_code)]
    None,
    Solid,
}
#[derive(Serialize)]
struct SymbolSpecRecord {
    shape: SymbolShapeRecord,
    size: f64,
    color: String,
    edge_color: String,
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
enum SymbolShapeRecord {
    #[allow(dead_code)]
    None,
    Circle,
}
#[derive(Serialize)]
struct LegendSpecRecord {
    visible: bool,
    position: LegendPositionRecord,
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
enum LegendPositionRecord {
    #[allow(dead_code)]
    TopLeft,
    TopRight,
}

fn ecrire(dir: &TempDir, nom: &str, body: &FileBody) -> PathBuf {
    let payload = rmp_serde::to_vec_named(body).unwrap();
    let path = dir.path().join(nom);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"STKF").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    f.sync_all().unwrap();
    path
}

fn dossier(id: u64, nom: &str, parent: Option<u64>, enfants: Vec<u64>) -> NodeRecord {
    NodeRecord {
        id,
        name: nom.into(),
        parent,
        kind: KindRecord::Folder { children: enfants },
    }
}
fn colonne(id: u64, nom: &str, parent: u64, valeurs: Vec<f64>) -> NodeRecord {
    let mut bytes = Vec::new();
    for v in &valeurs {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    NodeRecord {
        id,
        name: nom.into(),
        parent: Some(parent),
        kind: KindRecord::Column {
            column_type: TypeRecord::Numeric,
            role: RoleRecord::Y,
            unit: String::new(),
            long_name: String::new(),
            comment: String::new(),
            display_format: String::new(),
            values: ValuesRecord::Float(bytes),
            version: 0,
        },
    }
}

#[test]
fn b3_cycle_de_parente_est_refuse() {
    let dir = TempDir::new().unwrap();
    // racine 1 -> 2, et 2 -> 3 -> 2 (cycle)
    let body = FileBody {
        root: 1,
        next_id_hint: 4,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            dossier(2, "A", Some(3), vec![3]),
            dossier(3, "B", Some(2), vec![2]),
        ],
    };
    let path = ecrire(&dir, "cycle.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panique (ou boucle) sur un cycle de parenté");
    let r = r.unwrap();
    assert!(r.is_err(), "un cycle de parenté a été ACCEPTÉ");
    println!("cycle -> {}", r.unwrap_err());
}

#[test]
fn b3_auto_parente_est_refusee() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            dossier(2, "A", Some(2), vec![2]),
        ],
    };
    let path = ecrire(&dir, "self.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(
        r.is_ok(),
        "panique (ou boucle) sur un nœud son propre parent"
    );
    assert!(r.unwrap().is_err(), "auto-parenté ACCEPTÉE");
}

#[test]
fn b3_identifiant_zero_et_max() {
    let dir = TempDir::new().unwrap();
    // NodeId 0 n'est jamais attribué par l'allocateur ; u64::MAX force
    // `resume(hint, highest)` à saturer.
    let body = FileBody {
        root: 0,
        next_id_hint: 1,
        nodes: vec![
            dossier(0, "R", None, vec![u64::MAX]),
            dossier(u64::MAX, "A", Some(0), vec![]),
        ],
    };
    let path = ecrire(&dir, "ids.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panique sur des identifiants extrêmes");
    match r.unwrap() {
        Ok(p) => {
            println!("ACCEPTÉ : next_id_hint = {}", p.next_id_hint());
            // Le prochain identifiant attribué doit être libre.
            assert!(
                !p.tree()
                    .contains(stokesforge_core::NodeId::from_raw(p.next_id_hint())),
                "le prochain identifiant à attribuer ({}) est DÉJÀ pris : \
                 la prochaine création écrasera un nœud existant",
                p.next_id_hint()
            );
        }
        Err(e) => println!("refusé : {e}"),
    }
}

#[test]
fn b3_panneau_en_position_extreme() {
    let dir = TempDir::new().unwrap();
    let panneau = PanelSpecRecord {
        row: u64::MAX,
        column: u64::MAX,
        ..PanelSpecRecord::default()
    };
    let spec = PlotSpecRecord {
        title: "T".into(),
        panels: vec![panneau],
        background: String::new(),
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "G1".into(),
                parent: Some(1),
                kind: KindRecord::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };
    let path = ecrire(&dir, "grille.stf", &body);
    // B2 (`guide-de-correction-robustesse.md` §F3/F4) — corrigée : un
    // panneau en (row, column) au-delà de `MAX_PANEL_POSITION` (10 000) est
    // désormais refusé AU CHARGEMENT, avant que `PlotSpec::rows()`/
    // `columns()` n'aient la moindre chance de déborder sur la position
    // qu'il portait.
    let resultat = load(&path);
    println!("chargement d'un panneau en (u64::MAX, u64::MAX) → {resultat:?}");
    assert!(
        resultat.is_err(),
        "un panneau en position extrême doit être refusé au chargement, pas laissé à \
         PlotSpec::rows()/columns() : {resultat:?}"
    );
}

#[test]
fn b3_serie_referencant_un_noeud_inexistant_ou_non_colonne() {
    let dir = TempDir::new().unwrap();
    // 999 ne désigne aucun nœud ; 1 est la racine, un dossier et non une colonne.
    let serie = SeriesSpecRecord {
        y: 999,
        ..SeriesSpecRecord::default()
    };
    let serie2 = SeriesSpecRecord {
        y: 1,
        ..SeriesSpecRecord::default()
    };
    let panneau = PanelSpecRecord {
        series: vec![serie, serie2],
        ..PanelSpecRecord::default()
    };
    let spec = PlotSpecRecord {
        title: String::new(),
        panels: vec![panneau],
        background: String::new(),
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "G1".into(),
                parent: Some(1),
                kind: KindRecord::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };
    let path = ecrire(&dir, "brisee.stf", &body);
    let projet = load(&path).expect("référence pendante : acceptée par contrat");
    let tree = projet.tree();
    let node = tree.node(stokesforge_core::NodeId::from_raw(2)).unwrap();
    let spec = match node.kind() {
        stokesforge_core::NodeKind::Graph { spec, .. } => spec,
        _ => panic!(),
    };
    let resolu = stokesforge_core::resolve_spec(tree, spec);
    assert_eq!(
        resolu.broken_series().count(),
        2,
        "les deux séries doivent être marquées brisées"
    );
    assert_eq!(resolu.point_count(), 0);
}

#[test]
fn b3_valeurs_flottantes_longueur_non_multiple_de_huit() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "A".into(),
                parent: Some(3),
                kind: KindRecord::Column {
                    column_type: TypeRecord::Numeric,
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(vec![0u8; 13]), // 13 = pas un multiple de 8
                    version: 0,
                },
            },
        ],
    };
    let path = ecrire(&dir, "mauvaise_taille.stf", &body);
    let r = load(&path);
    assert!(
        r.is_err(),
        "un buffer de 13 octets a été accepté comme des f64"
    );
    println!("13 octets -> {}", r.unwrap_err());
}

#[test]
fn b3_type_de_colonne_incoherent_avec_ses_valeurs() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "A".into(),
                parent: Some(3),
                kind: KindRecord::Column {
                    column_type: TypeRecord::Numeric, // annoncé numérique
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Mixed {
                        tags: vec![2],
                        numbers: Vec::new(),
                        texts: vec!["x".into()],
                    }, // mais texte
                    version: 0,
                },
            },
        ],
    };
    let path = ecrire(&dir, "incoherent.stf", &body);
    let r = load(&path);
    assert!(
        r.is_err(),
        "une colonne numérique contenant du texte a été acceptée"
    );
    println!("type incohérent -> {}", r.unwrap_err());
}

#[test]
fn b3_noms_de_fenetres_dupliques_sont_refuses() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 4,
        nodes: vec![
            dossier(1, "R", None, vec![2, 3]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![],
                    label: String::new(),
                    comment: String::new(),
                },
            },
        ],
    };
    let path = ecrire(&dir, "doublon.stf", &body);
    let r = load(&path);
    assert!(
        r.is_err(),
        "deux fenêtres homonymes acceptées : [Book1] devient ambigu"
    );
    println!("fenêtres homonymes -> {}", r.unwrap_err());
}

#[test]
fn b3_enfant_declare_deux_fois_est_refuse() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            dossier(1, "R", None, vec![2, 2]),
            dossier(2, "A", Some(1), vec![]),
        ],
    };
    let path = ecrire(&dir, "double.stf", &body);
    let r = load(&path);
    assert!(r.is_err(), "un enfant listé deux fois a été accepté");
    println!("enfant doublé -> {}", r.unwrap_err());
}

#[test]
fn b3_colonne_hors_dune_feuille_est_refusee() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            colonne(2, "A", 1, vec![1.0]), // une colonne directement sous un dossier
        ],
    };
    let path = ecrire(&dir, "malrange.stf", &body);
    let r = load(&path);
    assert!(
        r.is_err(),
        "une colonne rangée sous un dossier a été acceptée"
    );
    println!("colonne mal rangée -> {}", r.unwrap_err());
}

#[test]
fn b3_nom_invalide_est_refuse() {
    let dir = TempDir::new().unwrap();
    for mauvais in ["", " A", "A ", "A!B", "[A]", "a/b"] {
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                dossier(1, "R", None, vec![2]),
                dossier(2, mauvais, Some(1), vec![]),
            ],
        };
        let path = ecrire(&dir, "nom.stf", &body);
        let r = load(&path);
        assert!(r.is_err(), "le nom {mauvais:?} a été accepté au chargement");
    }
}

/// A2 (#316) : un fichier plus gros que le plafond est refusé avant d'être lu
/// (la taille du fichier est sa taille décompressée : le `.stf` n'est pas
/// compressé), sans même regarder s'il est bien formé.
#[test]
fn b3_fichier_plus_gros_que_le_plafond_est_refuse_avant_lecture() {
    use stokesforge_persistence::{load_with_limit, PersistenceError};
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![dossier(1, "R", None, vec![])],
    };
    let path = ecrire(&dir, "limite.stf", &body);
    let taille = std::fs::metadata(&path).unwrap().len();
    assert!(load_with_limit(&path, taille).is_ok());
    assert!(
        matches!(
            load_with_limit(&path, taille - 1),
            Err(PersistenceError::TooLarge { .. })
        ),
        "un fichier au-dessus du plafond doit être refusé"
    );
    // Du bruit plus gros que le plafond : refusé pour sa taille, pas décodé.
    let bruit = dir.path().join("bruit.stf");
    std::fs::write(&bruit, vec![0xC1u8; 4096]).unwrap();
    assert!(matches!(
        load_with_limit(&bruit, 1024),
        Err(PersistenceError::TooLarge { size: 4096, .. })
    ));
}

/// #325 : le bloc des largeurs de colonne d'un fichier hostile — NaN, ∞,
/// zéro, négatif, clé orpheline, identifiant hors borne. Une entrée illisible
/// est écartée seule (invariant 10), jamais le fichier ; un identifiant hors
/// borne est refusé comme pour les géométries.
#[test]
fn b3_largeurs_de_colonne_hostiles() {
    use std::collections::BTreeMap;
    use stokesforge_core::NodeId;
    use stokesforge_persistence::load_with_report;

    #[derive(Serialize)]
    struct FileBodyLargeurs {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        column_widths: BTreeMap<u64, f32>,
    }
    let ecrire_largeurs = |dir: &TempDir, nom: &str, largeurs: &[(u64, f32)]| {
        let body = FileBodyLargeurs {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                dossier(1, "R", None, vec![2]),
                dossier(2, "A", Some(1), vec![]),
            ],
            column_widths: largeurs.iter().copied().collect(),
        };
        let payload = rmp_serde::to_vec_named(&body).unwrap();
        let path = dir.path().join(nom);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"STKF").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap();
        f.write_all(&payload).unwrap();
        path
    };
    let dir = TempDir::new().unwrap();
    let path = ecrire_largeurs(
        &dir,
        "melange.stf",
        &[
            (1, f32::NAN),
            (2, 48.5),
            (3, 60.0), // sans nœud
            (4, f32::INFINITY),
            (5, 0.0),
            (6, -12.0),
        ],
    );
    let r = std::panic::catch_unwind(|| load_with_report(&path));
    assert!(r.is_ok(), "panique sur des largeurs hostiles");
    let (_, vue, _) = r
        .unwrap()
        .expect("des largeurs hostiles ne font pas échouer le fichier");
    assert_eq!(
        vue.column_widths,
        [(NodeId::from_raw(2), 48.5)].into_iter().collect(),
        "seule l'entrée saine voisine d'entrées hostiles doit rester"
    );
    let hors_borne = ecrire_largeurs(&dir, "hors_borne.stf", &[(u64::MAX, 50.0)]);
    assert!(load_with_report(&hors_borne).is_err());
}

/// B1 (#316) : un fichier dont les colonnes déclarent plus de cellules que le
/// budget global est refusé avant toute allocation de colonne. Les colonnes
/// forgées sont sous `MAX_COLUMN_ROWS` chacune : seul le cumul les arrête.
#[test]
fn b3_budget_global_de_cellules_refuse_le_cumul() {
    use stokesforge_core::{Column, Project};
    use stokesforge_persistence::{load_with_limits, save, LoadLimits, PersistenceError};
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("P").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("B")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    for nom in ["A", "B", "C"] {
        project
            .add_column(sheet, None, Some(nom), Column::from_f64(&[0.0; 1000]))
            .unwrap();
    }
    let path = dir.path().join("cumul.stf");
    save(&project, &path).unwrap();
    let avec = |max_cells| LoadLimits {
        max_cells,
        ..LoadLimits::default()
    };
    assert!(load_with_limits(&path, avec(3000)).is_ok());
    assert!(matches!(
        load_with_limits(&path, avec(2999)),
        Err(PersistenceError::TooManyCells { .. })
    ));
}

/// #253 : le bloc des feuilles transposées liées d'un fichier hostile — lien
/// vers une feuille absente, vers un classeur, vers lui-même, identifiant hors
/// borne, colonne inexistante. Un lien illisible est écarté seul (invariant 10) :
/// jamais le fichier, et la feuille reste, ordinaire ; le lien sain voisin vit.
#[test]
fn b3_feuilles_transposees_hostiles() {
    use stokesforge_core::NodeId;

    #[derive(Serialize)]
    struct Ligne {
        row: u64,
        column: u64,
        stamp: u64,
    }
    #[derive(Serialize)]
    struct Lien {
        sheet: u64,
        source: u64,
        x_column: u64,
        x_stamp: u64,
        rows: Vec<Ligne>,
        inputs: Vec<u8>,
    }
    #[derive(Serialize)]
    struct FileBodyLiens {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        transposes: Vec<Lien>,
    }
    let lien = |sheet: u64, source: u64, colonne: u64| Lien {
        sheet,
        source,
        x_column: colonne,
        x_stamp: 7,
        rows: vec![Ligne {
            row: 0,
            column: colonne,
            stamp: 9,
        }],
        inputs: Vec::new(),
    };
    let dir = TempDir::new().unwrap();
    let body = FileBodyLiens {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            dossier(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3, 4],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![] },
            },
            NodeRecord {
                id: 4,
                name: "S1T".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![] },
            },
        ],
        transposes: vec![
            lien(999, 3, 50),         // feuille absente
            lien(2, 3, 50),           // « feuille » qui est un classeur
            lien(3, 3, 50),           // une feuille liée à elle-même
            lien(4, 3, u64::MAX / 2), // identifiant de colonne hors borne
            lien(4, 3, 50),           // sain, colonne inexistante tolérée
        ],
    };
    let payload = rmp_serde::to_vec_named(&body).unwrap();
    let path = dir.path().join("liens.stf");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"STKF").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    drop(f);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panique sur des liens hostiles");
    let projet = r
        .unwrap()
        .expect("des liens hostiles ne font pas échouer le fichier");
    let liens = projet.tree().transposes();
    assert_eq!(
        liens.keys().copied().collect::<Vec<_>>(),
        vec![NodeId::from_raw(4)],
        "seul le lien sain survit"
    );
    assert!(
        projet.tree().node(NodeId::from_raw(4)).is_some(),
        "la feuille d'un lien écarté reste"
    );
}
