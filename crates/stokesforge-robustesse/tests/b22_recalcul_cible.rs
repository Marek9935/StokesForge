//! Batterie 22 — recalcul ciblé des colonnes-formule (#193) : **le ciblé
//! rend exactement ce que rend le complet**, après chaque pas d'une séquence
//! tirée au sort.
//!
//! Depuis #193, l'acteur ne recalcule une colonne-formule que si une donnée
//! de son calcul a changé (`stokesforge-session/src/recalc.rs`). Le risque
//! de ce genre d'optimisation est unique et silencieux : une invalidation
//! oubliée laisse une valeur fausse **sans erreur**. Le filet est un oracle,
//! le recalcul complet d'avant #193, gardé tel quel dans le même code
//! ([`FormulaRecalc::Full`]) : deux sessions jumelles, l'une ciblée, l'autre
//! complète, reçoivent les mêmes requêtes, et leurs arbres doivent rester
//! égaux à chaque pas (l'égalité des colonnes ignore les compteurs de
//! version, qui divergent forcément : le ciblé écrit moins).
//!
//! Trois propriétés, vérifiées après chaque pas :
//!
//! 1. **oracle** — mêmes réponses (succès ou refus) et mêmes arbres dans les
//!    deux sessions ;
//! 2. **version ↔ contenu**, dans chaque session — l'hypothèse dont dépend
//!    tout le recalcul ciblé : un numéro de version ne désigne jamais deux
//!    contenus d'une même colonne. `b14` la vérifie sur `History` seul ;
//!    celle-ci la vérifie dans l'acteur, écritures du recalcul et macros
//!    comprises. Sa portée est une instance en mémoire : un `Load` repart
//!    des versions du fichier (`Project::from_tree`), donc la table repart
//!    de zéro à chaque chargement — c'est aussi pour cela que l'acteur vide
//!    son mémo au chargement ;
//! 3. **ciblé** — une commande sans rapport (un dossier de plus) ne change
//!    la version d'aucune colonne de la session ciblée. Sans cette propriété,
//!    un « ciblé » qui recalculerait tout passerait la première sans
//!    broncher : c'est le témoin de la batterie.
//!
//! Le tirage couvre ce qui peut changer une donnée d'une formule : saisies
//! (nombres, textes qui promeuvent en mixte), formules posées, changées,
//! illisibles, auto-référentes, en cycle, retirées ; colonnes ajoutées (y
//! compris avec une version déjà portée, ou clonées d'un instantané),
//! supprimées, déplacées dans la feuille ou vers une autre ; types changés ;
//! lignes insérées, supprimées ; cellules supprimées (y compris dans une
//! colonne-formule) ; feuilles dupliquées, collées, supprimées ; classeurs
//! collés ; annuler, rétablir, et des séries d'annulations suivies d'autant
//! de rétablissements (le rétablissement d'une entrée structurelle est le
//! seul chemin qui ne réestampille pas les versions) ; macros fermées ou
//! avortées ; sauvegarde et rechargement, y compris d'un état plus ancien.
//! Un contrôle final exige que chaque nature de pas ait lancé au moins vingt
//! passes : un geste toujours refusé ne prouverait rien.
//!
//! **Une fonctionnalité qui ajoute une donnée au calcul d'une formule doit
//! ajouter ici le pas qui la fait changer** — voir `docs/tests.md`,
//! « Targeted recalculation ». Sans ce pas, l'oracle ne peut pas voir l'oubli.
//!
//! Le tirage est **déterministe** (`TestRng::deterministic_rng`), comme
//! celui de `b14` : un échec se reproduit à l'identique et aucun fichier de
//! régression n'est écrit. Pour le rester, rien ici ne choisit une cible
//! dans l'ordre de `Tree::iter` (une table de hachage à graine aléatoire) :
//! les feuilles se prennent dans l'ordre du parcours depuis la racine.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    compute, AuxRef, Calculation, CalculationDraft, Cell, Column, ColumnMeta, ColumnType,
    ColumnValues, MixedCell, NodeCopy, NodeId, OutputDraft, Project, SeriesRef, Tree, ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};
use tempfile::TempDir;

// -- tirage ---------------------------------------------------------------

/// Formules tirées, **en lettres** (#321) : références dans et hors de la
/// feuille (jusqu'à `G`), auto-références et cycles selon la colonne qui les
/// reçoit, division (des zéros sont saisis), constante pure, texte illisible.
/// Le pas les convertit en identifiants (`Tree::formula_from_letters`) comme le
/// fait l'interface ; un refus (lettre hors de la feuille, auto-référence)
/// rend un pas sans commande, compté « refusé ».
const FORMULES: [&str; 13] = [
    "A*2", "B+1", "A+B", "C-A", "D/2", "A*", "F+1", "1+2", "A+B*C", "E", "(A+B)/D", "G-1", "B*B",
];

#[derive(Debug, Clone)]
enum Pas {
    /// Correction définitive d’une colonne ordinaire (#212).
    RemplacerCorrection {
        feuille: usize,
        colonne: usize,
    },
    Saisir {
        feuille: usize,
        colonne: usize,
        debut: usize,
        combien: usize,
        graine: usize,
    },
    SaisirTexte {
        feuille: usize,
        colonne: usize,
        debut: usize,
    },
    PoserFormule {
        feuille: usize,
        colonne: usize,
        formule: usize,
    },
    RetirerFormule {
        feuille: usize,
        colonne: usize,
    },
    /// `version` non nulle : une colonne venue d'ailleurs (fichier, autre
    /// projet) qui porte déjà un numéro, peut-être au-delà du compteur.
    AjouterColonne {
        feuille: usize,
        index: usize,
        longueur: usize,
        version: u64,
    },
    /// Ajoute, par `AddColumn`, le clone d'une colonne de l'instantané —
    /// version comprise, comme le ferait un client qui garde une colonne lue
    /// avant un chargement.
    AjouterClone {
        feuille: usize,
        colonne: usize,
        vers: usize,
    },
    SupprimerColonne {
        feuille: usize,
        colonne: usize,
    },
    DeplacerColonne {
        feuille: usize,
        colonne: usize,
        vers: usize,
        index: usize,
    },
    ChangerType {
        feuille: usize,
        colonne: usize,
        type_: usize,
    },
    InsererLignes {
        feuille: usize,
        at: usize,
        combien: usize,
    },
    SupprimerLignes {
        feuille: usize,
        at: usize,
        combien: usize,
    },
    SupprimerCellules {
        feuille: usize,
        colonne: usize,
        at: usize,
        combien: usize,
    },
    DupliquerFeuille {
        feuille: usize,
    },
    /// Copier-coller (`InsertCopy`) d'une feuille dans son classeur.
    CollerFeuille {
        feuille: usize,
    },
    /// Copier-coller (`InsertCopy`) du classeur d'une feuille à la racine.
    CollerClasseur {
        feuille: usize,
    },
    SupprimerFeuille {
        feuille: usize,
    },
    /// Un calcul d'analyse (#181) sur deux colonnes de la feuille : un
    /// ajustement linéaire, ou une correction par deux autres colonnes.
    AjouterCalcul {
        feuille: usize,
        x: usize,
        y: usize,
        correction: bool,
        automatique: bool,
    },
    /// Retire le `rang`-ième calcul (par identifiant), colonnes comprises ou non.
    RetirerCalcul {
        rang: usize,
        avec_colonnes: bool,
    },
    /// « Recalculate Out of Date » : relance les calculs périmés.
    RecalculerPerimes,
    Annuler,
    Retablir,
    /// `k` annulations puis `k` rétablissements : sans lui, un rétablissement
    /// d'entrée structurelle (le seul chemin qui ne réestampille pas les
    /// versions) n'arrive presque jamais.
    AnnulerPuisRetablir {
        k: usize,
    },
    OuvrirMacro,
    FermerMacro,
    AvorterMacro,
    /// Sauvegarde puis recharge l'état courant.
    Recharger,
    /// Recharge un état sauvegardé plus tôt dans la séquence.
    RevenirA {
        sauvegarde: usize,
    },
    SansRapport,
}

fn pas() -> impl Strategy<Value = Pas> {
    let f = 0usize..4;
    let c = 0usize..8;
    prop_oneof![
        6 => (f.clone(), c.clone(), 0usize..6, 1usize..4, 0usize..50).prop_map(
            |(feuille, colonne, debut, combien, graine)| Pas::Saisir {
                feuille,
                colonne,
                debut,
                combien,
                graine
            }
        ),
        1 => (f.clone(), c.clone(), 0usize..5).prop_map(|(feuille, colonne, debut)| {
            Pas::SaisirTexte {
                feuille,
                colonne,
                debut,
            }
        }),
        6 => (f.clone(), c.clone(), 0..FORMULES.len()).prop_map(|(feuille, colonne, formule)| {
            Pas::PoserFormule {
                feuille,
                colonne,
                formule,
            }
        }),
        1 => (f.clone(), c.clone())
            .prop_map(|(feuille, colonne)| Pas::RetirerFormule { feuille, colonne }),
        2 => (
            f.clone(),
            0usize..8,
            0usize..7,
            prop_oneof![Just(0u64), 1u64..120]
        )
            .prop_map(|(feuille, index, longueur, version)| Pas::AjouterColonne {
                feuille,
                index,
                longueur,
                version,
            }),
        1 => (f.clone(), c.clone(), f.clone()).prop_map(|(feuille, colonne, vers)| {
            Pas::AjouterClone {
                feuille,
                colonne,
                vers,
            }
        }),
        1 => (f.clone(), c.clone())
            .prop_map(|(feuille, colonne)| Pas::SupprimerColonne { feuille, colonne }),
        3 => (f.clone(), c.clone(), f.clone(), 0usize..8).prop_map(
            |(feuille, colonne, vers, index)| Pas::DeplacerColonne {
                feuille,
                colonne,
                vers,
                index
            }
        ),
        1 => (f.clone(), c.clone(), 0usize..3).prop_map(|(feuille, colonne, type_)| {
            Pas::ChangerType {
                feuille,
                colonne,
                type_,
            }
        }),
        2 => (f.clone(), 0usize..6, 1usize..3).prop_map(|(feuille, at, combien)| {
            Pas::InsererLignes {
                feuille,
                at,
                combien,
            }
        }),
        2 => (f.clone(), 0usize..6, 1usize..3).prop_map(|(feuille, at, combien)| {
            Pas::SupprimerLignes {
                feuille,
                at,
                combien,
            }
        }),
        1 => (f.clone(), c, 0usize..5, 1usize..3).prop_map(|(feuille, colonne, at, combien)| {
            Pas::SupprimerCellules {
                feuille,
                colonne,
                at,
                combien,
            }
        }),
        1 => f.clone().prop_map(|feuille| Pas::DupliquerFeuille { feuille }),
        1 => f.clone().prop_map(|feuille| Pas::CollerFeuille { feuille }),
        1 => f.clone().prop_map(|feuille| Pas::CollerClasseur { feuille }),
        1 => f.clone().prop_map(|feuille| Pas::SupprimerFeuille { feuille }),
        3 => (f.clone(), 0usize..8).prop_map(|(feuille, colonne)| Pas::RemplacerCorrection { feuille, colonne }),
        4 => (f, 0usize..8, 0usize..8, any::<bool>(), any::<bool>()).prop_map(
            |(feuille, x, y, correction, automatique)| Pas::AjouterCalcul {
                feuille,
                x,
                y,
                correction,
                automatique,
            }
        ),
        1 => (0usize..4, any::<bool>())
            .prop_map(|(rang, avec_colonnes)| Pas::RetirerCalcul { rang, avec_colonnes }),
        2 => Just(Pas::RecalculerPerimes),
        4 => Just(Pas::Annuler),
        2 => Just(Pas::Retablir),
        2 => (1usize..5).prop_map(|k| Pas::AnnulerPuisRetablir { k }),
        1 => Just(Pas::OuvrirMacro),
        1 => Just(Pas::FermerMacro),
        1 => Just(Pas::AvorterMacro),
        1 => Just(Pas::Recharger),
        1 => (0usize..8).prop_map(|sauvegarde| Pas::RevenirA { sauvegarde }),
        3 => Just(Pas::SansRapport),
    ]
}

/// Toutes les natures de pas, pour le contrôle de couverture.
const NOMS_DE_PAS: &[&str] = &[
    "RemplacerCorrection",
    "Saisir",
    "SaisirTexte",
    "PoserFormule",
    "RetirerFormule",
    "AjouterColonne",
    "AjouterClone",
    "SupprimerColonne",
    "DeplacerColonne",
    "ChangerType",
    "InsererLignes",
    "SupprimerLignes",
    "SupprimerCellules",
    "DupliquerFeuille",
    "CollerFeuille",
    "CollerClasseur",
    "SupprimerFeuille",
    "AjouterCalcul",
    "RetirerCalcul",
    "RecalculerPerimes",
    "Annuler",
    "Retablir",
    "AnnulerPuisRetablir",
    "OuvrirMacro",
    "FermerMacro",
    "AvorterMacro",
    "Recharger",
    "RevenirA",
    "SansRapport",
];

/// Nom de la nature d'un pas (une entrée de [`NOMS_DE_PAS`]).
fn nom(pas: &Pas) -> &'static str {
    match pas {
        Pas::RemplacerCorrection { .. } => "RemplacerCorrection",
        Pas::Saisir { .. } => "Saisir",
        Pas::SaisirTexte { .. } => "SaisirTexte",
        Pas::PoserFormule { .. } => "PoserFormule",
        Pas::RetirerFormule { .. } => "RetirerFormule",
        Pas::AjouterColonne { .. } => "AjouterColonne",
        Pas::AjouterClone { .. } => "AjouterClone",
        Pas::SupprimerColonne { .. } => "SupprimerColonne",
        Pas::DeplacerColonne { .. } => "DeplacerColonne",
        Pas::ChangerType { .. } => "ChangerType",
        Pas::InsererLignes { .. } => "InsererLignes",
        Pas::SupprimerLignes { .. } => "SupprimerLignes",
        Pas::SupprimerCellules { .. } => "SupprimerCellules",
        Pas::DupliquerFeuille { .. } => "DupliquerFeuille",
        Pas::CollerFeuille { .. } => "CollerFeuille",
        Pas::CollerClasseur { .. } => "CollerClasseur",
        Pas::SupprimerFeuille { .. } => "SupprimerFeuille",
        Pas::AjouterCalcul { .. } => "AjouterCalcul",
        Pas::RetirerCalcul { .. } => "RetirerCalcul",
        Pas::RecalculerPerimes => "RecalculerPerimes",
        Pas::Annuler => "Annuler",
        Pas::Retablir => "Retablir",
        Pas::AnnulerPuisRetablir { .. } => "AnnulerPuisRetablir",
        Pas::OuvrirMacro => "OuvrirMacro",
        Pas::FermerMacro => "FermerMacro",
        Pas::AvorterMacro => "AvorterMacro",
        Pas::Recharger => "Recharger",
        Pas::RevenirA { .. } => "RevenirA",
        Pas::SansRapport => "SansRapport",
    }
}

/// Runner à graine fixe, sans fichier de régression sur le disque du dépôt.
fn runner(cas: u32) -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases: cas,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    )
}

// -- le monde joué --------------------------------------------------------

/// `Book1` à deux feuilles de longueurs différentes, déjà munies de
/// formules (dont une chaîne et une constante), pour que les premiers pas
/// aient de quoi recalculer.
fn projet() -> Project {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("classeur");
    let s1 = project.resolve_path("[Book1]Sheet1").expect("feuille 1");
    let s2 = project.add_worksheet(book, None).expect("feuille 2");
    let colonnes: [(NodeId, &[f64], Option<&str>); 7] = [
        (s1, &[1.0, 2.0, 3.0], None),
        (s1, &[10.0, 0.0, 30.0, 40.0], None),
        (s1, &[], Some("A*2")),
        (s1, &[], Some("C+B")),
        (s2, &[1.0, 2.0, 3.0, 4.0, 5.0], None),
        (s2, &[], Some("A/2")),
        (s2, &[], Some("1+2")),
    ];
    let mut avec_formule = Vec::new();
    for (sheet, valeurs, formule) in colonnes {
        let id = project
            .add_column(sheet, None, None, Column::from_f64(valeurs))
            .expect("colonne");
        if let Some(texte) = formule {
            avec_formule.push((id, texte));
        }
    }
    // Les formules se saisissent en lettres une fois les colonnes de leur
    // feuille en place (#321) : lettres vers identifiants.
    for (id, texte) in avec_formule {
        let formula = project
            .tree()
            .formula_from_letters(id, texte)
            .expect("formule de départ");
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .expect("formule posée");
    }
    project
}

struct Jumelle {
    /// Gardée vivante : lâcher la `Session` arrêterait l'acteur.
    _session: Session,
    handle: SessionHandle,
    /// `(NodeId, version) → (type, valeurs)`, pour la propriété 2. Les
    /// métadonnées (la formule elle-même, l'unité…) n'en font pas partie :
    /// la version ne les a jamais couvertes, et le texte de la formule a sa
    /// propre composante dans l'empreinte du recalcul.
    vu: HashMap<(NodeId, u64), (ColumnType, ColumnValues)>,
}

impl Jumelle {
    fn new(mode: FormulaRecalc) -> Self {
        let session = Session::spawn_with_recalc(projet(), None, mode).expect("acteur démarré");
        let handle = session.handle();
        Self {
            _session: session,
            handle,
            vu: HashMap::new(),
        }
    }

    fn instantane(&self) -> Snapshot {
        self.handle.snapshot().expect("instantané")
    }
}

/// Les feuilles de l'arbre, dans l'ordre du parcours depuis la racine —
/// jamais celui de `Tree::iter`, adossé à une table de hachage dont la
/// graine change à chaque processus : le tirage ne serait plus rejouable.
fn feuilles(tree: &Tree) -> Vec<NodeId> {
    tree.walk_all()
        .into_iter()
        .filter(|&id| tree.node(id).is_some_and(|node| node.kind().is_worksheet()))
        .collect()
}

fn choisir<T: Copy>(liste: &[T], i: usize) -> Option<T> {
    (!liste.is_empty()).then(|| liste[i % liste.len()])
}

fn colonnes(tree: &Tree, feuille: Option<NodeId>) -> Vec<NodeId> {
    feuille
        .and_then(|f| tree.children(f).ok())
        .map(<[NodeId]>::to_vec)
        .unwrap_or_default()
}

/// Choix dans l’ordre de la feuille, sans dépendre de la table de hachage.
/// Le calcul et la commande restent les mêmes que dans les clients réels.
fn commande_correction(tree: &Tree, colonnes: &[NodeId], rang: usize) -> Option<Command> {
    let candidates: Vec<_> = colonnes
        .iter()
        .copied()
        .filter(|id| {
            tree.node(*id).and_then(|n| n.as_column()).is_some_and(|c| {
                c.column_type() == ColumnType::Numeric
                    && c.meta().formula.is_none()
                    && tree.calculation_of_column(*id).is_none()
            })
        })
        .collect();
    let y = *candidates.get(rang % candidates.len().max(1))?;
    let version = tree.node(y)?.as_column()?.version();
    let operation = Operation::Baseline {
        method: BaselineMethod::Constant { point: (0.0, 1.5) },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let series = vec![SeriesRef {
        x: None,
        y,
        sigma: None,
    }];
    let result = compute(tree, &operation, &series, None).ok()?;
    Some(Command::ReplaceAnalysisData {
        draft: CalculationDraft {
            operation,
            series,
            aux: None,
            outputs: result
                .columns
                .into_iter()
                .map(|c| OutputDraft {
                    series: c.series,
                    key: c.key,
                    index: c.index,
                    meta: ColumnMeta::default(),
                    data: c.data,
                })
                .collect(),
            results: result.results,
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: true,
        },
        expected_versions: vec![(y, version)],
    })
}

/// Les versions de toutes les colonnes de l'arbre.
fn versions(tree: &Tree) -> HashMap<NodeId, u64> {
    tree.iter()
        .filter_map(|(id, node)| node.as_column().map(|c| (id, c.version())))
        .collect()
}

/// Traduit un pas en commande, sur l'arbre (commun) des deux jumelles.
/// `None` : le pas n'est pas une commande, ou n'a pas de cible.
fn commande(pas: &Pas, tree: &Tree, rang: usize) -> Option<Command> {
    let sheets = feuilles(tree);
    let feuille = |i: usize| choisir(&sheets, i);
    let colonne = |f: usize, c: usize| choisir(&colonnes(tree, feuille(f)), c);
    match *pas {
        Pas::RemplacerCorrection {
            feuille: f,
            colonne: c,
        } => commande_correction(tree, &colonnes(tree, feuille(f)), c),
        Pas::Saisir {
            feuille: f,
            colonne: c,
            debut,
            combien,
            graine,
        } => colonne(f, c).map(|column| Command::SetCells {
            column,
            start: debut,
            // Des zéros et des négatifs : la division et le signe comptent.
            values: ValueBlock::Float(
                (0..combien)
                    .map(|k| ((graine * 7 + k * 3) % 11) as f64 - 3.0)
                    .collect(),
            ),
            expected_version: None,
        }),
        Pas::SaisirTexte {
            feuille: f,
            colonne: c,
            debut,
        } => colonne(f, c).map(|column| Command::SetCellsPromoting {
            column,
            start: debut,
            values: ValueBlock::Mixed(vec![MixedCell::Text("x".into())]),
            expected_version: None,
        }),
        Pas::PoserFormule {
            feuille: f,
            colonne: c,
            formule,
        } => colonne(f, c).and_then(|column| {
            // Lettres vers identifiants sur l'instantané (les deux jumelles
            // portent les mêmes identifiants). Un refus — lettre hors de la
            // feuille, colonne qui se lit elle-même — rend `None` : le pas
            // n'est pas joué, comme tout pas sans cible (#321).
            let formula = tree.formula_from_letters(column, FORMULES[formule]).ok()?;
            Some(Command::SetColumnMeta {
                column,
                meta: ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            })
        }),
        Pas::RetirerFormule {
            feuille: f,
            colonne: c,
        } => colonne(f, c).map(|column| Command::SetColumnMeta {
            column,
            meta: ColumnMeta::default(),
        }),
        Pas::AjouterColonne {
            feuille: f,
            index,
            longueur,
            version,
        } => feuille(f).and_then(|sheet| {
            Some(Command::AddColumn {
                sheet,
                index: Some(index % (colonnes(tree, Some(sheet)).len() + 1)),
                name: None,
                column: Column::from_block_with_version(
                    ColumnType::Numeric,
                    ValueBlock::Float((0..longueur).map(|k| k as f64 + 0.5).collect()),
                    ColumnMeta::default(),
                    version,
                )
                .ok()?,
            })
        }),
        Pas::AjouterClone {
            feuille: f,
            colonne: c,
            vers,
        } => colonne(f, c).zip(feuille(vers)).and_then(|(id, sheet)| {
            let clone = tree.node(id)?.as_column()?.clone();
            Some(Command::AddColumn {
                sheet,
                index: None,
                name: None,
                column: clone,
            })
        }),
        Pas::SupprimerColonne {
            feuille: f,
            colonne: c,
        } => colonne(f, c).map(|column| Command::DeleteColumn { column }),
        Pas::DeplacerColonne {
            feuille: f,
            colonne: c,
            vers,
            index,
        } => colonne(f, c)
            .zip(feuille(vers))
            .map(|(id, new_parent)| Command::Move {
                id,
                new_parent,
                // Un index au-delà de la feuille est refusé : le ramener dans
                // ses bornes, sans quoi trois déplacements sur quatre l'étaient.
                index: index % (colonnes(tree, Some(new_parent)).len() + 1),
            }),
        Pas::ChangerType {
            feuille: f,
            colonne: c,
            type_,
        } => colonne(f, c).map(|column| Command::SetColumnType {
            column,
            column_type: [ColumnType::Numeric, ColumnType::Mixed, ColumnType::DateTime][type_],
        }),
        Pas::InsererLignes {
            feuille: f,
            at,
            combien,
        } => feuille(f).map(|sheet| Command::InsertRows {
            sheet,
            at,
            count: combien,
        }),
        Pas::SupprimerLignes {
            feuille: f,
            at,
            combien,
        } => feuille(f).map(|sheet| Command::DeleteRows {
            sheet,
            at,
            count: combien,
        }),
        Pas::SupprimerCellules {
            feuille: f,
            colonne: c,
            at,
            combien,
        } => colonne(f, c).map(|column| Command::DeleteCells {
            columns: vec![column],
            at,
            count: combien,
        }),
        Pas::DupliquerFeuille { feuille: f } => {
            feuille(f).map(|sheet| Command::DuplicateWorksheet {
                sheet,
                name: format!("Copie{rang}"),
            })
        }
        Pas::CollerFeuille { feuille: f } => feuille(f).and_then(|sheet| {
            Some(Command::InsertCopy {
                parent: tree.parent(sheet).ok()??,
                copy: NodeCopy::capture(tree, sheet).ok()?,
                name: format!("Collee{rang}"),
            })
        }),
        Pas::CollerClasseur { feuille: f } => feuille(f).and_then(|sheet| {
            let book = tree.parent(sheet).ok()??;
            Some(Command::InsertCopy {
                parent: tree.root(),
                copy: NodeCopy::capture(tree, book).ok()?,
                name: format!("Colle{rang}"),
            })
        }),
        Pas::SupprimerFeuille { feuille: f } => {
            feuille(f).map(|sheet| Command::RemoveNode { id: sheet })
        }
        Pas::AjouterCalcul {
            feuille: f,
            x,
            y,
            correction,
            automatique,
        } => {
            let cols = colonnes(tree, feuille(f));
            let (cx, cy) = (choisir(&cols, x)?, choisir(&cols, y)?);
            if cx == cy {
                return None;
            }
            let (operation, aux) = if correction {
                (
                    Operation::Correct {
                        op: CorrectOp::Divide,
                    },
                    Some(AuxRef {
                        x: choisir(&cols, x + 1),
                        y: choisir(&cols, y + 2)?,
                    }),
                )
            } else {
                (
                    Operation::Fit {
                        spec: FitSpec {
                            model: Model::Linear,
                            params: vec![],
                        },
                        range: None,
                        residuals: false,
                    },
                    None,
                )
            };
            let series = vec![SeriesRef {
                x: Some(cx),
                y: cy,
                sigma: None,
            }];
            let calcule = compute(tree, &operation, &series, aux.as_ref()).ok()?;
            Some(Command::AddCalculation {
                draft: CalculationDraft {
                    operation,
                    series,
                    aux,
                    outputs: calcule
                        .columns
                        .iter()
                        .map(|c| OutputDraft {
                            series: c.series,
                            key: c.key.clone(),
                            index: c.index,
                            meta: ColumnMeta::default(),
                            data: c.data.clone(),
                        })
                        .collect(),
                    results: calcule.results,
                    keep_results: true,
                    auto_recalc: automatique,
                    low_r2_dialog: true,
                },
            })
        }
        Pas::RetirerCalcul {
            rang,
            avec_colonnes,
        } => {
            let ids: Vec<NodeId> = tree.calculations().keys().copied().collect();
            choisir(&ids, rang).map(|id| Command::RemoveCalculation {
                id,
                with_columns: avec_colonnes,
            })
        }
        Pas::SansRapport => Some(Command::AddFolder {
            parent: tree.root(),
            name: format!("Temoin{rang}"),
        }),
        Pas::RecalculerPerimes
        | Pas::Annuler
        | Pas::Retablir
        | Pas::AnnulerPuisRetablir { .. }
        | Pas::OuvrirMacro
        | Pas::FermerMacro
        | Pas::AvorterMacro
        | Pas::Recharger
        | Pas::RevenirA { .. } => None,
    }
}

/// Réponse d'une session à un pas : c'est elle que les deux jumelles
/// doivent partager.
/// Ordonnée du moins au plus : la réponse d'un pas composé est la plus
/// forte de ses requêtes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reponse {
    Refuse,
    /// Accepté sans passe de recalcul : ouvrir ou fermer une macro, annuler
    /// ou rétablir sans rien à défaire.
    AccepteSansPasse,
    /// Accepté, et l'acteur a lancé une passe de recalcul.
    AccepteAvecPasse,
}

impl Reponse {
    fn de<T, E>(resultat: Result<T, E>, passe: impl FnOnce(&T) -> bool) -> Self {
        match resultat {
            Err(_) => Reponse::Refuse,
            Ok(valeur) if passe(&valeur) => Reponse::AccepteAvecPasse,
            Ok(_) => Reponse::AccepteSansPasse,
        }
    }
}

/// Joue un pas sur une jumelle. `sauvegardes` : les fichiers de cette
/// jumelle, dans l'ordre où ils ont été écrits.
fn jouer(
    handle: &SessionHandle,
    pas: &Pas,
    commande: Option<&Command>,
    sauvegardes: &mut Vec<PathBuf>,
    dossier: &TempDir,
    nom: &str,
) -> Reponse {
    match pas {
        Pas::RemplacerCorrection { .. } => {
            let Some(Command::ReplaceAnalysisData {
                draft,
                expected_versions,
            }) = commande
            else {
                return Reponse::Refuse;
            };
            let Ok(snapshot) = handle.snapshot() else {
                return Reponse::Refuse;
            };
            // Les deux compteurs divergent lorsque le mode complet réécrit
            // des formules inchangées. Le résultat commun ne change pas ;
            // seule sa garde de concurrence prend les versions de sa jumelle.
            let expected_versions: Option<Vec<_>> = expected_versions
                .iter()
                .map(|(id, _)| Some((*id, snapshot.tree.node(*id)?.as_column()?.version())))
                .collect();
            let Some(expected_versions) = expected_versions else {
                return Reponse::Refuse;
            };
            Reponse::de(
                handle.execute(Command::ReplaceAnalysisData {
                    draft: draft.clone(),
                    expected_versions,
                }),
                |_| true,
            )
        }
        Pas::RecalculerPerimes => Reponse::de(handle.recalculate(&[]), |_| true),
        Pas::Annuler => Reponse::de(handle.undo(), Option::is_some),
        Pas::Retablir => Reponse::de(handle.redo(), Option::is_some),
        Pas::AnnulerPuisRetablir { k } => {
            let mut reponses = Vec::new();
            for _ in 0..*k {
                reponses.push(Reponse::de(handle.undo(), Option::is_some));
            }
            for _ in 0..*k {
                reponses.push(Reponse::de(handle.redo(), Option::is_some));
            }
            reponses.into_iter().max().unwrap_or(Reponse::Refuse)
        }
        Pas::OuvrirMacro => Reponse::de(handle.begin_macro(MacroKind::PasteCells), |_| false),
        Pas::FermerMacro => Reponse::de(handle.end_macro(), |_| false),
        Pas::AvorterMacro => Reponse::de(handle.abort_macro(), |_| true),
        Pas::Recharger => {
            let chemin = dossier
                .path()
                .join(format!("{nom}-{}.stf", sauvegardes.len()));
            if handle.save(&chemin).is_err() {
                return Reponse::Refuse;
            }
            sauvegardes.push(chemin.clone());
            Reponse::de(handle.load(&chemin), |_| true)
        }
        Pas::RevenirA { sauvegarde } => {
            match sauvegardes.get(*sauvegarde % sauvegardes.len().max(1)) {
                Some(chemin) => Reponse::de(handle.load(chemin), |_| true),
                None => Reponse::Refuse,
            }
        }
        _ => match commande {
            Some(commande) => Reponse::de(handle.execute(commande.clone()), |_| true),
            None => Reponse::Refuse,
        },
    }
}

/// Propriété 2 sur une jumelle : un `(colonne, version)` déjà vu porte le
/// même contenu. Rend la première colonne fautive.
fn verifier_versions(jumelle: &mut Jumelle, tree: &Tree) -> Option<(NodeId, u64)> {
    for (id, node) in tree.iter() {
        let Some(colonne) = node.as_column() else {
            continue;
        };
        let cle = (id, colonne.version());
        let contenu = (colonne.column_type(), colonne.values().clone());
        match jumelle.vu.get(&cle) {
            Some(deja) if *deja != contenu => return Some(cle),
            Some(_) => {}
            None => {
                jumelle.vu.insert(cle, contenu);
            }
        }
    }
    None
}

/// Ce que le tirage a réellement atteint, compté sur la session ciblée
/// après chaque pas qui a lancé une passe de recalcul, colonne-formule non
/// vide par colonne-formule non vide.
#[derive(Debug, Default, Clone)]
struct Couverture {
    /// Au moins une cellule calculée.
    calculees: usize,
    /// Aucune cellule calculée : illisible, cycle, source absente ou vide.
    non_calculees: usize,
    /// Présentes avant et après une passe, avec la même version : épargnées
    /// par le ciblé.
    epargnees: usize,
    /// Pas acceptés par les deux sessions, passe de recalcul ou non.
    acceptes: usize,
    /// Calculs d'analyse présents après une passe, dont les périmés.
    calculs: usize,
    calculs_perimes: usize,
    /// Par nature de pas : combien ont été acceptés **avec** une passe de
    /// recalcul. Un geste qui ne passe jamais ne prouve rien à l'oracle.
    par_pas: BTreeMap<&'static str, usize>,
}

impl Couverture {
    fn ajouter(&mut self, autre: Couverture) {
        self.calculees += autre.calculees;
        self.non_calculees += autre.non_calculees;
        self.epargnees += autre.epargnees;
        self.acceptes += autre.acceptes;
        self.calculs += autre.calculs;
        self.calculs_perimes += autre.calculs_perimes;
        for (nom, n) in autre.par_pas {
            *self.par_pas.entry(nom).or_default() += n;
        }
    }

    /// `chargement` : un `Load` repart des versions du fichier, et une
    /// colonne réécrite peut retomber par hasard sur le numéro qu'elle
    /// portait avant — comparer les versions de part et d'autre ne dit
    /// alors rien, et le compte dépendrait de la numérotation.
    fn compter(&mut self, avant: &Tree, apres: &Tree, chargement: bool) {
        let versions_avant = if chargement {
            HashMap::new()
        } else {
            versions(avant)
        };
        for calcul in apres.calculations().values() {
            self.calculs += 1;
            self.calculs_perimes += usize::from(Calculation::clone(calcul).stale.is_some());
        }
        for (id, node) in apres.iter() {
            let Some(colonne) = node.as_column() else {
                continue;
            };
            if colonne.meta().formula.is_none() || colonne.is_empty() {
                continue;
            }
            let calculee = (0..colonne.len()).any(|row| match colonne.get(row) {
                Cell::Float(v) => !v.is_nan(),
                Cell::Text(Some(t)) => t != "---",
                Cell::Text(None) => false,
            });
            if calculee {
                self.calculees += 1;
            } else {
                self.non_calculees += 1;
            }
            if versions_avant.get(&id) == Some(&colonne.version()) {
                self.epargnees += 1;
            }
        }
    }
}

/// Joue `tirage` dans les deux jumelles en vérifiant les trois propriétés.
fn derouler(tirage: &[Pas]) -> Result<Couverture, TestCaseError> {
    let mut couverture = Couverture::default();
    let mut cible = Jumelle::new(FormulaRecalc::Targeted);
    let mut complet = Jumelle::new(FormulaRecalc::Full);
    let dossier = TempDir::new().expect("répertoire temporaire");
    let (mut fichiers_cible, mut fichiers_complet) = (Vec::new(), Vec::new());

    // Le projet de départ porte des formules jamais calculées : la première
    // passe de recalcul les calcule toutes, et un chargement vide le mémo.
    // La propriété 3 ne vaut qu'une fois une passe faite depuis l'un ou
    // l'autre.
    let mut memo_amorce = false;
    for (rang, pas) in tirage.iter().enumerate() {
        let avant = cible.instantane();
        let commande = commande(pas, &avant.tree, rang);

        let reponse_cible = jouer(
            &cible.handle,
            pas,
            commande.as_ref(),
            &mut fichiers_cible,
            &dossier,
            "cible",
        );
        let reponse_complet = jouer(
            &complet.handle,
            pas,
            commande.as_ref(),
            &mut fichiers_complet,
            &dossier,
            "complet",
        );
        prop_assert_eq!(
            reponse_cible,
            reponse_complet,
            "pas {} ({:?}) : les deux sessions ne répondent pas pareil",
            rang,
            pas
        );
        couverture.acceptes += usize::from(reponse_cible != Reponse::Refuse);

        let (s_cible, s_complet) = (cible.instantane(), complet.instantane());
        // 1. oracle
        prop_assert!(
            *s_cible.tree == *s_complet.tree,
            "pas {rang} ({pas:?}) : le recalcul ciblé diverge du recalcul complet\ncalculs ciblé : {:?}\ncalculs complet : {:?}",
            s_cible.tree.calculations(),
            s_complet.tree.calculations()
        );

        // 2. version ↔ contenu, dans chaque session.
        if matches!(pas, Pas::Recharger | Pas::RevenirA { .. })
            && reponse_cible == Reponse::AccepteAvecPasse
        {
            cible.vu.clear();
            complet.vu.clear();
        }
        for (jumelle, tree, quoi) in [
            (&mut cible, &s_cible.tree, "ciblée"),
            (&mut complet, &s_complet.tree, "complète"),
        ] {
            if let Some((id, version)) = verifier_versions(jumelle, tree) {
                prop_assert!(
                    false,
                    "pas {rang} ({pas:?}), session {quoi} : {id} porte la version {version} \
                     avec un contenu différent de celui déjà vu sous ce numéro"
                );
            }
        }

        // 3. ciblé : une commande sans rapport ne réécrit aucune colonne.
        if matches!(pas, Pas::SansRapport)
            && reponse_cible == Reponse::AccepteAvecPasse
            && memo_amorce
        {
            prop_assert_eq!(
                versions(&avant.tree),
                versions(&s_cible.tree),
                "pas {} : une commande sans rapport a réécrit une colonne",
                rang
            );
        }
        let passe = reponse_cible == Reponse::AccepteAvecPasse;
        if passe {
            couverture.compter(
                &avant.tree,
                &s_cible.tree,
                matches!(pas, Pas::Recharger | Pas::RevenirA { .. }),
            );
            *couverture.par_pas.entry(nom(pas)).or_default() += 1;
        }
        memo_amorce |= passe;
    }
    Ok(couverture)
}

// -- garde-fou --------------------------------------------------------------

/// L'oracle, puis le contrôle de ce que le tirage a atteint : sans ce
/// dernier, un tirage qui ne poserait jamais de formule, ou dont toutes les
/// formules seraient en erreur, passerait l'oracle trivialement.
#[test]
fn b22_le_recalcul_cible_rend_exactement_le_recalcul_complet() {
    let total = std::cell::RefCell::new(Couverture::default());
    runner(128)
        .run(&prop::collection::vec(pas(), 40..90), |tirage| {
            let couverture = derouler(&tirage)?;
            total.borrow_mut().ajouter(couverture);
            Ok(())
        })
        .expect("le recalcul ciblé suit l'oracle à chaque pas");
    let total = total.into_inner();
    eprintln!("b22 : {total:?}");
    assert!(total.calculees > 1000, "{total:?}");
    assert!(total.non_calculees > 100, "{total:?}");
    assert!(total.epargnees > 1000, "{total:?}");
    assert!(total.calculs > 200, "{total:?}");
    assert!(total.calculs_perimes > 30, "{total:?}");
    // Chaque geste qui lance une passe l'a fait assez souvent pour que
    // l'oracle ait eu l'occasion de voir ce qu'il casserait. Ouvrir et
    // fermer une macro ne lancent jamais de passe.
    for geste in NOMS_DE_PAS {
        if matches!(*geste, "OuvrirMacro" | "FermerMacro") {
            continue;
        }
        let n = total.par_pas.get(geste).copied().unwrap_or(0);
        assert!(n >= 20, "{geste} : {n} passes seulement — {total:?}");
    }
}
