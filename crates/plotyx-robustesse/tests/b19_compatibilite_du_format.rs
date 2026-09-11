//! Batterie 19 — ce qu'un `.plx` écrit par une version **plus récente** fait
//! à ce lecteur-ci.
//!
//! Le format porte deux mécanismes de compatibilité, et il faut les distinguer :
//!
//! - le **numéro de version** de l'en-tête, lu avant le corps : un fichier plus
//!   récent est refusé proprement (`UnsupportedVersion`), sans qu'un octet de
//!   son corps soit décodé ;
//! - les **maps nommées** de MessagePack : un champ inconnu est ignoré, un
//!   champ absent prend son `#[serde(default)]`. C'est ce qui doit permettre
//!   « d'ajouter des données sans casser les fichiers déjà écrits », **sans
//!   toucher au numéro de version** — c'est écrit ainsi dans la doc de tête de
//!   `plotyx-persistence` et de `schema.rs`.
//!
//! Le second mécanisme couvre les **champs**. Il ne couvre pas les
//! **variantes** — et rien ne le dit.
//!
//! Le schéma est privé au crate ; on le redéclare ici, comme `b3`. C'est
//! légitime : le format est auto-descriptif, donc deux déclarations d'accord
//! sur les noms produisent les mêmes octets.

use std::io::Write;
use std::path::PathBuf;

use plotyx_persistence::{load, PersistenceError};
use serde::Serialize;
use tempfile::TempDir;

// -- schéma miroir, dans sa version « future » ----------------------------

#[derive(Serialize)]
struct FileBody<K> {
    root: u64,
    next_id_hint: u64,
    nodes: Vec<NodeRecord<K>>,
}

#[derive(Serialize)]
struct NodeRecord<K> {
    id: u64,
    name: String,
    parent: Option<u64>,
    kind: K,
}

/// Le schéma d'aujourd'hui, **plus un champ** qu'une version future aurait
/// ajouté sur les dossiers.
#[derive(Serialize)]
enum KindAvecChampNeuf {
    Folder {
        children: Vec<u64>,
        /// Ce que la v2 aurait ajouté.
        couleur_detiquette: String,
    },
}

/// Le schéma d'aujourd'hui, **plus une variante** : une version future qui
/// saurait ranger des images dans un dossier.
#[derive(Serialize)]
enum KindAvecVarianteNeuve {
    Folder {
        children: Vec<u64>,
    },
    #[allow(dead_code)]
    Image {
        source: String,
    },
}

/// Schéma miroir d'un `PlotSpec`, **plus un type de tracé** que ce lecteur ne
/// connaît pas. Seuls les champs comptent : les valeurs sont celles par
/// défaut, le fichier doit être valide partout ailleurs.
#[derive(Serialize, Default)]
struct PlotSpecRecord {
    title: String,
    panels: Vec<PanelSpecRecord>,
    background: String,
    format_version: u32,
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

#[derive(Serialize, Default)]
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

#[derive(Serialize, Default)]
enum AxisScaleRecord {
    #[default]
    Linear,
}

#[derive(Serialize, Default)]
struct LegendSpecRecord {
    visible: bool,
    position: LegendPositionRecord,
}

#[derive(Serialize, Default)]
enum LegendPositionRecord {
    #[default]
    TopRight,
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

/// `Violin` n'existe pas dans `plotyx-plot` : c'est le type de tracé qu'une
/// version future aurait ajouté.
#[derive(Serialize)]
enum PlotTypeRecord {
    #[allow(dead_code)]
    Line,
    Violin,
}

#[derive(Serialize, Default)]
struct LineSpecRecord {
    color: String,
    style: LineStyleRecord,
    width: f64,
}

#[derive(Serialize, Default)]
enum LineStyleRecord {
    #[default]
    Solid,
}

#[derive(Serialize, Default)]
struct SymbolSpecRecord {
    shape: SymbolShapeRecord,
    size: f64,
    color: String,
    edge_color: String,
}

#[derive(Serialize, Default)]
enum SymbolShapeRecord {
    #[default]
    Circle,
}

#[derive(Serialize, Default)]
enum SeriesAxisRecord {
    #[default]
    Left,
}

/// Un arbre valide de bout en bout, dont un graphe porte le spec fautif.
#[derive(Serialize)]
enum KindAvecTraceNeuve {
    Folder {
        children: Vec<u64>,
    },
    Worksheet {
        columns: Vec<u64>,
    },
    Workbook {
        sheets: Vec<u64>,
        label: String,
        comment: String,
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
}

#[derive(Serialize)]
enum TypeRecord {
    Numeric,
}

#[derive(Serialize)]
enum RoleRecord {
    Y,
}

#[derive(Serialize)]
enum ValuesRecord {
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
}

/// Écrit un corps derrière l'en-tête `PLYX` + `version`.
fn ecrire<K: Serialize>(dir: &TempDir, nom: &str, version: u32, body: &FileBody<K>) -> PathBuf {
    let chemin = dir.path().join(nom);
    let mut fichier = std::fs::File::create(&chemin).expect("fichier créé");
    fichier.write_all(b"PLYX").expect("signature");
    fichier
        .write_all(&version.to_le_bytes())
        .expect("numéro de version");
    fichier
        .write_all(&rmp_serde::to_vec_named(body).expect("encodage"))
        .expect("corps");
    chemin
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b19_un_champ_inconnu_est_bien_ignore() {
    // La promesse tenue : c'est le mécanisme sur lequel toute l'évolution du
    // format repose, et il fonctionne.
    let dir = TempDir::new().expect("répertoire");
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![NodeRecord {
            id: 1,
            name: "R".to_string(),
            parent: None,
            kind: KindAvecChampNeuf::Folder {
                children: Vec::new(),
                couleur_detiquette: "#ff0000".to_string(),
            },
        }],
    };
    let project = load(&ecrire(&dir, "champ.plx", 1, &body)).expect("champ inconnu ignoré");
    assert_eq!(project.tree().len(), 1);
}

#[test]
fn b19_un_numero_de_version_superieur_est_refuse_proprement() {
    let dir = TempDir::new().expect("répertoire");
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![NodeRecord {
            id: 1,
            name: "R".to_string(),
            parent: None,
            kind: KindAvecVarianteNeuve::Folder {
                children: Vec::new(),
            },
        }],
    };
    let erreur = load(&ecrire(&dir, "v2.plx", 2, &body)).expect_err("version future refusée");
    assert!(
        matches!(
            erreur,
            PersistenceError::UnsupportedVersion { found: 2, .. }
        ),
        "{erreur:?}"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
#[ignore = "reproduction C5 — une variante d'enum inconnue rend le fichier ENTIER illisible, et l'erreur annonce « corrompu »"]
fn b19_une_variante_inconnue_ne_perd_pas_tout_le_fichier() {
    let dir = TempDir::new().expect("répertoire");
    // Un projet parfaitement lisible : une racine, et **un seul** nœud d'une
    // nature que ce lecteur ne connaît pas.
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindAvecVarianteNeuve::Folder { children: vec![2] },
            },
            NodeRecord {
                id: 2,
                name: "Photo".to_string(),
                parent: Some(1),
                kind: KindAvecVarianteNeuve::Image {
                    source: "manip.png".to_string(),
                },
            },
        ],
    };
    // Version 1 dans l'en-tête, exprès : c'est le scénario que la
    // documentation encourage — ajouter des données sans toucher au numéro.
    let chemin = ecrire(&dir, "image.plx", 1, &body);
    let resultat = load(&chemin);
    match &resultat {
        Ok(project) => println!("chargé : {} nœuds", project.tree().len()),
        Err(erreur) => println!("refusé : {erreur}"),
    }
    assert!(
        resultat.is_ok(),
        "un nœud d'une nature inconnue emporte le fichier entier — les autres, \
         parfaitement lisibles, sont perdus avec lui. Et l'erreur rendue est \
         `Corrupt` (« fichier corrompu »), qui envoie l'utilisateur chercher une \
         sauvegarde alors que son fichier est intact et que la bonne réponse est \
         « mettez PlotYx à jour ». Les deux mécanismes du format ne disent pas la \
         même chose sur le numéro de version : un champ neuf n'exige pas de le \
         bouger, une variante neuve l'exige — et rien ne l'écrit nulle part"
    );
}

#[test]
#[ignore = "reproduction C5 — même une variante inconnue enfouie dans le spec d'un seul graphe emporte tout le projet, mesures comprises"]
fn b19_un_type_de_trace_inconnu_ne_perd_pas_les_donnees() {
    // Le cas réaliste : un collègue trace un violon avec une version plus
    // récente et envoie le fichier. Tout le reste — le classeur, la feuille,
    // les mille mesures — est parfaitement lisible par ce lecteur-ci.
    let dir = TempDir::new().expect("répertoire");
    let mesures: Vec<u8> = (0..1000u32)
        .flat_map(|i| f64::from(i).to_le_bytes())
        .collect();
    let octets_de_donnees = mesures.len();

    let serie = SeriesSpecRecord {
        y: 4,
        x: None,
        x_error: None,
        y_error: None,
        plot_type: PlotTypeRecord::Violin, // le seul champ que ce lecteur ignore
        name: String::new(),
        line: LineSpecRecord::default(),
        symbol: SymbolSpecRecord::default(),
        axis: SeriesAxisRecord::default(),
        visible: true,
        show_in_legend: true,
        bar_width: None,
        histogram_bins: 40,
    };
    let spec = PlotSpecRecord {
        panels: vec![PanelSpecRecord {
            series: vec![serie],
            ..PanelSpecRecord::default()
        }],
        format_version: 1,
        ..PlotSpecRecord::default()
    };

    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindAvecTraceNeuve::Folder {
                    children: vec![2, 5],
                },
            },
            NodeRecord {
                id: 2,
                name: "Book1".to_string(),
                parent: Some(1),
                kind: KindAvecTraceNeuve::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Sheet1".to_string(),
                parent: Some(2),
                kind: KindAvecTraceNeuve::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "Signal".to_string(),
                parent: Some(3),
                kind: KindAvecTraceNeuve::Column {
                    column_type: TypeRecord::Numeric,
                    role: RoleRecord::Y,
                    unit: "V".to_string(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(mesures),
                    version: 7,
                },
            },
            NodeRecord {
                id: 5,
                name: "Fig1".to_string(),
                parent: Some(1),
                kind: KindAvecTraceNeuve::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };

    let chemin = ecrire(&dir, "violon.plx", 1, &body);
    let resultat = load(&chemin);
    match &resultat {
        Ok(project) => println!("chargé : {} nœuds", project.tree().len()),
        Err(erreur) => println!("refusé : {erreur}"),
    }
    assert!(
        resultat.is_ok(),
        "un seul champ `plot_type` inconnu, dans une seule série d'un seul graphe, \
         emporte les {octets_de_donnees} octets de mesures du même fichier. Le même \
         raisonnement vaut pour `ColumnType`, `LineStyle`, `SymbolShape`, \
         `LegendPosition` et `AxisScale` : six enums de plus dans le même fichier, six \
         façons de le rendre entièrement illisible. Un lecteur devrait dégrader \
         gracieusement — garder ce qu'il comprend, marquer le reste — ou, au minimum, \
         rendre `UnsupportedVersion` plutôt que `Corrupt`, pour que le message dise la \
         vérité : « mettez PlotYx à jour », et non « votre fichier est endommagé »"
    );
}
