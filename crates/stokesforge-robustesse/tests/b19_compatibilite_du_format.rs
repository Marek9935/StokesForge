//! Batterie 19 — ce qu'un `.stf` écrit par une version **plus récente** fait
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
//!   `stokesforge-persistence` et de `schema.rs`.
//!
//! Le second mécanisme couvre les **champs**. Il ne couvre pas les
//! **variantes** — et rien ne le dit.
//!
//! Le schéma est privé au crate ; on le redéclare ici, comme `b3`. C'est
//! légitime : le format est auto-descriptif, donc deux déclarations d'accord
//! sur les noms produisent les mêmes octets.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use stokesforge_core::NodeKind;
use stokesforge_persistence::{load, save, PersistenceError};
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

/// `Violin` n'existe pas dans `stokesforge-plot` : c'est le type de tracé qu'une
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

/// Écrit un corps derrière l'en-tête `STKF` + `version`.
fn ecrire<K: Serialize>(dir: &TempDir, nom: &str, version: u32, body: &FileBody<K>) -> PathBuf {
    let chemin = dir.path().join(nom);
    let mut fichier = std::fs::File::create(&chemin).expect("fichier créé");
    fichier.write_all(b"STKF").expect("signature");
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
    let project = load(&ecrire(&dir, "champ.stf", 1, &body)).expect("champ inconnu ignoré");
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
    let erreur = load(&ecrire(&dir, "v2.stf", 2, &body)).expect_err("version future refusée");
    assert!(
        matches!(
            erreur,
            PersistenceError::UnsupportedVersion { found: 2, .. }
        ),
        "{erreur:?}"
    );
}

// -- garde-fous (C5, niveau 2) ---------------------------------------------

#[test]
// C5 (niveau 2) — corrigée : `KindRecord`/`PlotTypeRecord` (`schema.rs`)
// n'ont plus `Serialize`/`Deserialize` dérivés, mais écrits à la main
// (`unknown.rs`). Le décodage capture d'abord la valeur MessagePack qui
// suit le nom de variante dans un arbre générique (`rmpv::Value`), la
// réencode, puis tente de la relire comme l'une des variantes connues avec
// le même décodeur `rmp_serde` que pour le reste du fichier. Un nom de
// variante inconnu — ou une variante connue dont le contenu échoue plus
// profondément (un type de tracé inconnu enfoui dans le spec d'un graphe,
// `b19_un_type_de_trace_inconnu_ne_perd_pas_les_donnees` ci-dessous) — ne
// remonte plus l'erreur : le nom et le contenu bruts sont conservés dans
// `KindRecord::Unknown`/`PlotTypeRecord::Unknown`, qui portent
// `stokesforge_plot::RawVariant` jusque dans le modèle en mémoire
// (`NodeKind::Unknown`, `PlotType::Unknown`) pour qu'une sauvegarde
// suivante réécrive le nœud à l'identique (voir `b20_fichier_partage.rs`,
// inchangé et toujours vert). La perte est bornée à ce seul nœud, jamais au
// fichier entier.
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
    let chemin = ecrire(&dir, "image.stf", 1, &body);
    let project = load(&chemin).expect(
        "un nœud d'une nature inconnue ne doit plus emporter le fichier entier — les \
         autres, parfaitement lisibles, ne doivent pas être perdus avec lui",
    );
    println!("chargé : {} nœuds", project.tree().len());
    assert_eq!(
        project.tree().len(),
        2,
        "la racine ET le nœud inconnu doivent survivre"
    );

    let photo = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(2))
        .expect("nœud « Photo » présent malgré sa nature inconnue");
    match photo.kind() {
        NodeKind::Unknown(raw) => assert_eq!(
            raw.name, "Image",
            "le nom de la variante d'origine doit survivre"
        ),
        other => panic!("attendu NodeKind::Unknown, obtenu {other:?}"),
    }

    // Deux sauvegardes successives du même projet doivent produire le même
    // fichier (B20, `b20_deux_sauvegardes_successives_donnent_le_meme_fichier`,
    // inchangé et toujours vert) : c'est la garantie qui borne la perte au
    // nœud inconnu, jamais à sa fidélité de ré-écriture.
    let a = dir.path().join("image_a.stf");
    let b = dir.path().join("image_b.stf");
    save(&project, &a).expect("première sauvegarde d'un projet portant un nœud inconnu");
    save(&project, &b).expect("seconde sauvegarde du même projet");
    assert_eq!(
        std::fs::read(&a).expect("lecture a"),
        std::fs::read(&b).expect("lecture b"),
        "le nœud d'une nature inconnue doit survivre à deux sauvegardes identiques, \
         pas être perdu ni renormalisé"
    );
}

#[test]
// C5 (niveau 2) — corrigée, même mécanisme que ci-dessus appliqué à
// `PlotTypeRecord`/`PlotType` : un type de tracé inconnu enfoui dans le
// spec d'un graphe ne fait plus échouer le `Graph` qui le porte — ni,
// a fortiori, le classeur et la colonne qui n'ont rien à voir avec lui.
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

    let chemin = ecrire(&dir, "violon.stf", 1, &body);
    let project = load(&chemin).expect(
        "un seul champ `plot_type` inconnu, dans une seule série d'un seul graphe, ne \
         doit plus emporter les mesures du même fichier",
    );
    println!("chargé : {} nœuds", project.tree().len());
    assert_eq!(
        project.tree().len(),
        5,
        "le classeur, la feuille, la colonne de {octets_de_donnees} octets et le graphe \
         doivent tous survivre"
    );

    // Le type de tracé inconnu est capturé à l'échelle de la série, pas de
    // tout le graphe : `PlotTypeRecord`/`PlotType` ont le même mécanisme
    // `Unknown` que `KindRecord`/`NodeKind` (voir la doc au-dessus du
    // test). Le graphe lui-même reste un `NodeKind::Graph` en bonne et due
    // forme, avec son panneau et sa série — seul `plot_type` est opaque.
    let graphe = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(5))
        .expect("nœud « Fig1 » présent");
    match graphe.kind() {
        NodeKind::Graph { spec, .. } => {
            let serie = &spec.panels[0].series[0];
            match &serie.plot_type {
                stokesforge_plot::PlotType::Unknown(raw) => {
                    assert_eq!(raw.name, "Violin", "le nom du type de tracé doit survivre");
                }
                other => panic!("attendu PlotType::Unknown, obtenu {other:?}"),
            }
        }
        other => panic!("attendu NodeKind::Graph (dégradation à l'échelle de la série, pas du nœud), obtenu {other:?}"),
    }

    // Les mesures elles-mêmes, portées par la colonne « Signal », doivent
    // être intactes : ce nœud n'a rien à voir avec le type de tracé inconnu.
    let colonne = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(4))
        .expect("nœud « Signal » présent")
        .as_column()
        .expect("« Signal » est une colonne");
    assert_eq!(
        colonne
            .float_values()
            .expect("colonne numérique")
            .as_slice()
            .len(),
        1000,
        "les 1000 mesures doivent survivre intactes"
    );

    // Même vérification de fidélité qu'au test précédent (B20) : deux
    // sauvegardes successives du même projet sont identiques.
    let a = dir.path().join("violon_a.stf");
    let b = dir.path().join("violon_b.stf");
    save(&project, &a).expect("première sauvegarde d'un projet portant un type de tracé inconnu");
    save(&project, &b).expect("seconde sauvegarde du même projet");
    assert_eq!(
        std::fs::read(&a).expect("lecture a"),
        std::fs::read(&b).expect("lecture b"),
        "le type de tracé inconnu doit survivre à deux sauvegardes identiques"
    );
}

// -- la compatibilité *descendante* des colonnes à types mixtes -------------

/// Le schéma d'aujourd'hui, **plus un type de colonne** : ce que ce lecteur
/// verrait d'un fichier écrit par une version plus récente.
///
/// Le cas réel qui motive ce test est l'inverse dans le temps — un lecteur
/// d'**avant** les colonnes à types mixtes ouvrant un fichier qui en porte
/// une, donc rencontrant le nom de variante `"Mixed"` qu'il ne connaît pas.
/// C'est exactement le même mécanisme ; ce test l'exerce avec un nom
/// qu'*aucune* version n'attribue, pour qu'il reste valable maintenant que
/// `Mixed` est connu de ce lecteur-ci.
#[derive(Serialize)]
enum KindAvecTypeDeColonneNeuf {
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
        column_type: TypeRecordFutur,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecordFutur,
        version: u64,
    },
}

#[derive(Serialize)]
enum TypeRecordFutur {
    #[allow(dead_code)]
    Numeric,
    /// Un type de colonne qu'aucune version n'a jamais écrit — il tient ici
    /// le rôle que `Mixed` tiendrait pour un lecteur d'avant.
    Complex,
}

#[derive(Serialize)]
enum ValuesRecordFutur {
    #[allow(dead_code)]
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    /// La forme de valeurs qui va avec : des tableaux parallèles, comme
    /// `ValuesRecord::Mixed` dans le schéma d'aujourd'hui.
    Complex {
        #[serde(with = "serde_bytes")]
        reels: Vec<u8>,
        #[serde(with = "serde_bytes")]
        imaginaires: Vec<u8>,
    },
}

#[test]
// Ce qui rend la stratégie de migration des colonnes à types mixtes tenable
// **sans toucher au numéro de version du format** : un lecteur plus ancien
// qui rencontre un type de colonne qu'il ne connaît pas ne perd que cette
// colonne, jamais le fichier. C'est le mécanisme C5 (niveau 2) de
// `unknown.rs`, déjà en place pour les natures de nœud — une colonne est un
// `KindRecord::Column`, donc elle en bénéficie sans qu'une ligne lui soit
// propre. Ce test le vérifie explicitement, parce que c'est sur lui que
// repose la décision de ne pas incrémenter `FORMAT_VERSION`.
fn b19_un_type_de_colonne_inconnu_ne_perd_que_cette_colonne() {
    let dir = TempDir::new().expect("répertoire");
    let mesures: Vec<u8> = (0..500u32)
        .flat_map(|i| f64::from(i).to_le_bytes())
        .collect();

    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindAvecTypeDeColonneNeuf::Folder { children: vec![2] },
            },
            NodeRecord {
                id: 2,
                name: "Book1".to_string(),
                parent: Some(1),
                kind: KindAvecTypeDeColonneNeuf::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Sheet1".to_string(),
                parent: Some(2),
                kind: KindAvecTypeDeColonneNeuf::Worksheet {
                    columns: vec![4, 5],
                },
            },
            NodeRecord {
                id: 4,
                name: "Signal".to_string(),
                parent: Some(3),
                kind: KindAvecTypeDeColonneNeuf::Column {
                    column_type: TypeRecordFutur::Numeric,
                    role: RoleRecord::Y,
                    unit: "V".to_string(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecordFutur::Float(mesures),
                    version: 3,
                },
            },
            NodeRecord {
                id: 5,
                name: "Exotique".to_string(),
                parent: Some(3),
                kind: KindAvecTypeDeColonneNeuf::Column {
                    column_type: TypeRecordFutur::Complex,
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecordFutur::Complex {
                        reels: 1.0f64.to_le_bytes().to_vec(),
                        imaginaires: 2.0f64.to_le_bytes().to_vec(),
                    },
                    version: 9,
                },
            },
        ],
    };

    let chemin = ecrire(&dir, "complexe.stf", 1, &body);
    let project = load(&chemin).expect(
        "un type de colonne inconnu ne doit pas emporter le fichier : c'est la \
         condition pour qu'un nouveau type de colonne s'ajoute sans incrémenter \
         FORMAT_VERSION",
    );
    assert_eq!(
        project.tree().len(),
        5,
        "les quatre nœuds lisibles ET la colonne inconnue doivent survivre"
    );

    // La colonne voisine, parfaitement lisible, garde ses 500 mesures.
    let signal = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(4))
        .expect("nœud « Signal » présent")
        .as_column()
        .expect("« Signal » est une colonne");
    assert_eq!(
        signal
            .float_values()
            .expect("colonne numérique")
            .as_slice()
            .len(),
        500,
        "les 500 mesures de la colonne voisine doivent survivre intactes"
    );

    // La colonne exotique, elle, devient un nœud opaque.
    let exotique = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(5))
        .expect("nœud « Exotique » présent malgré son type de colonne inconnu");
    match exotique.kind() {
        NodeKind::Unknown(raw) => assert_eq!(raw.name, "Column"),
        other => panic!("attendu NodeKind::Unknown, obtenu {other:?}"),
    }

    // Et la fidélité de ré-écriture (B20) : ce que ce lecteur ne comprend
    // pas, il le rend tel quel — donc un aller-retour par une version plus
    // ancienne ne détruit pas la colonne mixte d'une version plus récente.
    let a = dir.path().join("complexe_a.stf");
    let b = dir.path().join("complexe_b.stf");
    save(&project, &a).expect("première sauvegarde");
    save(&project, &b).expect("seconde sauvegarde");
    assert_eq!(
        std::fs::read(&a).expect("lecture a"),
        std::fs::read(&b).expect("lecture b"),
        "la colonne d'un type inconnu doit survivre à deux sauvegardes identiques"
    );
}
