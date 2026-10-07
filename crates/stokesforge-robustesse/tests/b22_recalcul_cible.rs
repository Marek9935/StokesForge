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
//! illisibles, auto-référentes, en cycle, retirées ; colonnes ajoutées,
//! supprimées, déplacées dans la feuille ou vers une autre ; types changés ;
//! lignes insérées, supprimées ; cellules supprimées (y compris dans une
//! colonne-formule) ; feuilles dupliquées ; annuler, rétablir ; macros
//! fermées ou avortées ; sauvegarde et rechargement, y compris d'un état
//! plus ancien.
//!
//! **Une fonctionnalité qui ajoute une donnée au calcul d'une formule doit
//! ajouter ici le pas qui la fait changer** — voir `docs/tests.md`,
//! « Recalcul ciblé ». Sans ce pas, l'oracle ne peut pas voir l'oubli.
//!
//! Le tirage est **déterministe** (`TestRng::deterministic_rng`), comme
//! celui de `b14` : un échec se reproduit à l'identique et aucun fichier de
//! régression n'est écrit.

use std::collections::HashMap;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnType, ColumnValues, MixedCell, NodeId, Project, Tree,
    ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};
use tempfile::TempDir;

// -- tirage ---------------------------------------------------------------

/// Formules tirées : références dans et hors de la feuille (jusqu'à `G`),
/// auto-références et cycles selon la colonne qui les reçoit, division (des
/// zéros sont saisis), constante pure, texte illisible.
const FORMULES: [&str; 13] = [
    "A*2", "B+1", "A+B", "C-A", "D/2", "A*", "F+1", "1+2", "A+B*C", "E", "(A+B)/D", "G-1", "B*B",
];

#[derive(Debug, Clone)]
enum Pas {
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
    AjouterColonne {
        feuille: usize,
        index: usize,
        longueur: usize,
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
    Annuler,
    Retablir,
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
        2 => (f.clone(), 0usize..8, 0usize..7).prop_map(|(feuille, index, longueur)| {
            Pas::AjouterColonne {
                feuille,
                index,
                longueur,
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
        1 => f.prop_map(|feuille| Pas::DupliquerFeuille { feuille }),
        4 => Just(Pas::Annuler),
        2 => Just(Pas::Retablir),
        1 => Just(Pas::OuvrirMacro),
        1 => Just(Pas::FermerMacro),
        1 => Just(Pas::AvorterMacro),
        1 => Just(Pas::Recharger),
        1 => (0usize..8).prop_map(|sauvegarde| Pas::RevenirA { sauvegarde }),
        3 => Just(Pas::SansRapport),
    ]
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
    let formule = |texte: &str| ColumnMeta {
        formula: Some(texte.to_string()),
        ..Default::default()
    };
    let colonnes: [(NodeId, &[f64], Option<ColumnMeta>); 7] = [
        (s1, &[1.0, 2.0, 3.0], None),
        (s1, &[10.0, 0.0, 30.0, 40.0], None),
        (s1, &[], Some(formule("A*2"))),
        (s1, &[], Some(formule("C+B"))),
        (s2, &[1.0, 2.0, 3.0, 4.0, 5.0], None),
        (s2, &[], Some(formule("A/2"))),
        (s2, &[], Some(formule("1+2"))),
    ];
    for (sheet, valeurs, meta) in colonnes {
        let mut colonne = Column::from_f64(valeurs);
        if let Some(meta) = meta {
            colonne.set_meta(meta);
        }
        project
            .add_column(sheet, None, None, colonne)
            .expect("colonne");
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

/// Les feuilles de l'arbre, dans l'ordre du parcours.
fn feuilles(tree: &Tree) -> Vec<NodeId> {
    tree.iter()
        .filter(|(_, node)| node.kind().is_worksheet())
        .map(|(id, _)| id)
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
        } => colonne(f, c).map(|column| Command::SetColumnMeta {
            column,
            meta: ColumnMeta {
                formula: Some(FORMULES[formule].to_string()),
                ..Default::default()
            },
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
        } => feuille(f).map(|sheet| Command::AddColumn {
            sheet,
            index: Some(index),
            name: None,
            column: Column::from_f64(&(0..longueur).map(|k| k as f64 + 0.5).collect::<Vec<_>>()),
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
                index,
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
        Pas::SansRapport => Some(Command::AddFolder {
            parent: tree.root(),
            name: format!("Temoin{rang}"),
        }),
        Pas::Annuler
        | Pas::Retablir
        | Pas::OuvrirMacro
        | Pas::FermerMacro
        | Pas::AvorterMacro
        | Pas::Recharger
        | Pas::RevenirA { .. } => None,
    }
}

/// Réponse d'une session à un pas : c'est elle que les deux jumelles
/// doivent partager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        Pas::Annuler => Reponse::de(handle.undo(), Option::is_some),
        Pas::Retablir => Reponse::de(handle.redo(), Option::is_some),
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
#[derive(Debug, Default, Clone, Copy)]
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
}

impl Couverture {
    fn ajouter(&mut self, autre: Couverture) {
        self.calculees += autre.calculees;
        self.non_calculees += autre.non_calculees;
        self.epargnees += autre.epargnees;
        self.acceptes += autre.acceptes;
    }

    fn compter(&mut self, avant: &Tree, apres: &Tree) {
        let versions_avant = versions(avant);
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
            "pas {rang} ({pas:?}) : le recalcul ciblé diverge du recalcul complet"
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
            couverture.compter(&avant.tree, &s_cible.tree);
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
    let total = std::cell::Cell::new(Couverture::default());
    runner(128)
        .run(&prop::collection::vec(pas(), 40..90), |tirage| {
            let couverture = derouler(&tirage)?;
            let mut cumul = total.get();
            cumul.ajouter(couverture);
            total.set(cumul);
            Ok(())
        })
        .expect("le recalcul ciblé suit l'oracle à chaque pas");
    let total = total.get();
    eprintln!("b22 : {total:?}");
    assert!(total.calculees > 1000, "{total:?}");
    assert!(total.non_calculees > 100, "{total:?}");
    assert!(total.epargnees > 1000, "{total:?}");
}
