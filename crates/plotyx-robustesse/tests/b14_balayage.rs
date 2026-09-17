//! Batterie 14 — balayage : trois invariants vérifiés **après chaque pas**
//! d'une séquence tirée au sort.
//!
//! Les batteries 1 à 13 éprouvent des endroits choisis. Celle-ci ne choisit
//! rien : elle joue des séquences de commandes (écritures, structure,
//! conversions, lignes, annulations, rétablissements) et revérifie à chaque
//! pas trois propriétés que le dépôt énonce ailleurs :
//!
//! 1. **version ↔ contenu** — deux états différents d'une même colonne ne
//!    partagent jamais un numéro de [`plotyx_core::Column::version`] ;
//! 2. **`path_of` ∘ `resolve_path` = identité** — la propriété énoncée par
//!    `b8_proprietes`, ici sur des arbres que ce tirage-là construit ;
//! 3. **aller-retour disque** — `save` puis `load` rend un arbre égal, et des
//!    compteurs de version égaux.
//!
//! Le tirage est **déterministe** (`TestRng::deterministic_rng`), comme celui
//! de `plotyx-commands/tests/undo_random.rs` : un échec se reproduit à
//! l'identique et aucun fichier de régression n'est écrit.
//!
//! C'est ce balayage qui a trouvé A7 et A8 sans qu'on lui dise où regarder ;
//! il reste ici comme filet, pas seulement comme preuve.

use std::collections::HashMap;

use plotyx_commands::{Command, History};
use plotyx_core::{Column, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use tempfile::TempDir;

// -- tirage ---------------------------------------------------------------

/// Noms volontairement partagés entre dossiers, notes et colonnes : c'est ce
/// qui met en présence les trois portées d'unicité du modèle.
const NOMS: [&str; 4] = ["P", "Q", "R", "S"];

#[derive(Debug, Clone)]
enum Pas {
    Cellules {
        colonne: usize,
        debut: usize,
        combien: usize,
    },
    AjouterColonne {
        index: usize,
    },
    SupprimerColonne {
        colonne: usize,
    },
    RenommerColonne {
        colonne: usize,
        nom: usize,
    },
    AjouterDossier {
        nom: usize,
    },
    AjouterNote {
        nom: usize,
    },
    DeplacerAlaRacine {
        enfant: usize,
        index: usize,
    },
    ChangerDeType {
        colonne: usize,
        type_: usize,
    },
    InsererLignes {
        at: usize,
        combien: usize,
    },
    SupprimerLignes {
        at: usize,
        combien: usize,
    },
    Annuler,
    Retablir,
}

fn pas() -> impl Strategy<Value = Pas> {
    prop_oneof![
        (0usize..6, 0usize..6, 0usize..4).prop_map(|(colonne, debut, combien)| Pas::Cellules {
            colonne,
            debut,
            combien
        }),
        (0usize..6).prop_map(|index| Pas::AjouterColonne { index }),
        (0usize..6).prop_map(|colonne| Pas::SupprimerColonne { colonne }),
        (0usize..6, 0usize..4).prop_map(|(colonne, nom)| Pas::RenommerColonne { colonne, nom }),
        (0usize..4).prop_map(|nom| Pas::AjouterDossier { nom }),
        (0usize..4).prop_map(|nom| Pas::AjouterNote { nom }),
        (0usize..5, 0usize..5).prop_map(|(enfant, index)| Pas::DeplacerAlaRacine { enfant, index }),
        (0usize..6, 0usize..3).prop_map(|(colonne, type_)| Pas::ChangerDeType { colonne, type_ }),
        (0usize..5, 1usize..3).prop_map(|(at, combien)| Pas::InsererLignes { at, combien }),
        (0usize..5, 1usize..3).prop_map(|(at, combien)| Pas::SupprimerLignes { at, combien }),
        Just(Pas::Annuler),
        Just(Pas::Retablir),
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

struct Monde {
    project: Project,
    history: History,
    sheet: NodeId,
}

impl Monde {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("projet valide");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
            .expect("colonne A");
        project
            .add_column(sheet, None, Some("B"), Column::from_f64(&[3.0]))
            .expect("colonne B");
        Self {
            project,
            history: History::with_limit(0),
            sheet,
        }
    }

    fn colonnes(&self) -> Vec<NodeId> {
        self.project
            .tree()
            .children(self.sheet)
            .map(<[NodeId]>::to_vec)
            .unwrap_or_default()
    }

    /// Joue un pas. Un refus du modèle est un rejet de tirage, pas un échec —
    /// même convention que `undo_random.rs`.
    fn jouer(&mut self, pas: &Pas) {
        let colonnes = self.colonnes();
        let choisir = |i: usize| colonnes.get(i % colonnes.len().max(1)).copied();
        let commande = match pas {
            Pas::Cellules {
                colonne,
                debut,
                combien,
            } => choisir(*colonne).map(|c| Command::SetCells {
                column: c,
                start: *debut,
                values: ValueBlock::Float((0..*combien).map(|k| k as f64 + 0.5).collect()),
                expected_version: None,
            }),
            Pas::AjouterColonne { index } => Some(Command::AddColumn {
                sheet: self.sheet,
                index: Some(*index),
                name: None,
                column: Column::from_f64(&[7.0, 8.0]),
            }),
            Pas::SupprimerColonne { colonne } => {
                choisir(*colonne).map(|c| Command::DeleteColumn { column: c })
            }
            Pas::RenommerColonne { colonne, nom } => choisir(*colonne).map(|c| Command::Rename {
                id: c,
                name: NOMS[*nom].to_string(),
            }),
            Pas::AjouterDossier { nom } => {
                let root = self.project.root();
                let _ = self.project.add_folder(root, NOMS[*nom]);
                None
            }
            Pas::AjouterNote { nom } => {
                let root = self.project.root();
                let _ = self.project.add_note(root, Some(NOMS[*nom]), "");
                None
            }
            Pas::DeplacerAlaRacine { enfant, index } => {
                let root = self.project.root();
                let enfants = self
                    .project
                    .tree()
                    .children(root)
                    .map(<[NodeId]>::to_vec)
                    .unwrap_or_default();
                enfants
                    .get(*enfant % enfants.len().max(1))
                    .map(|id| Command::Move {
                        id: *id,
                        new_parent: root,
                        index: *index,
                    })
            }
            Pas::ChangerDeType { colonne, type_ } => {
                choisir(*colonne).map(|c| Command::SetColumnType {
                    column: c,
                    column_type: [ColumnType::Numeric, ColumnType::Mixed, ColumnType::DateTime]
                        [*type_],
                })
            }
            Pas::InsererLignes { at, combien } => Some(Command::InsertRows {
                sheet: self.sheet,
                at: *at,
                count: *combien,
            }),
            Pas::SupprimerLignes { at, combien } => Some(Command::DeleteRows {
                sheet: self.sheet,
                at: *at,
                count: *combien,
            }),
            Pas::Annuler => {
                let _ = self.history.undo(&mut self.project);
                None
            }
            Pas::Retablir => {
                let _ = self.history.redo(&mut self.project);
                None
            }
        };
        if let Some(commande) = commande {
            let _ = self.history.execute(&mut self.project, commande);
        }
    }
}

/// `NodeId → (version, contenu binaire)` pour toutes les colonnes de l'arbre.
/// Le contenu est réduit à des bits (`to_bits` pour les flottants) pour que
/// `NaN == NaN`, comme partout ailleurs dans le modèle.
fn empreintes(project: &Project) -> HashMap<NodeId, (u64, Vec<u64>)> {
    project
        .tree()
        .iter()
        .filter_map(|(id, node)| {
            let colonne = node.as_column()?;
            let bits: Vec<u64> = match colonne.values() {
                ColumnValues::Float(v) => v.iter().map(|x| x.to_bits()).collect(),
                // Une empreinte, pas une sérialisation : l'étiquette de la
                // cellule suffit à distinguer les trois variantes, et le
                // motif binaire du nombre est ce qui compte pour `NaN`.
                ColumnValues::Mixed(v) => v
                    .iter()
                    .map(|c| match c {
                        plotyx_core::MixedCell::Empty => 0,
                        plotyx_core::MixedCell::Number(x) => x.to_bits(),
                        plotyx_core::MixedCell::Text(t) => t.len() as u64 + 1,
                    })
                    .collect(),
            };
            Some((id, (colonne.version(), bits)))
        })
        .collect()
}

// -- garde-fou ------------------------------------------------------------

#[test]
fn b14_laller_retour_disque_tient_a_chaque_pas() {
    let mut runner = runner(8);
    runner
        .run(&prop::collection::vec(pas(), 40..60), |tirage| {
            let mut monde = Monde::new();
            let dir = TempDir::new().expect("répertoire temporaire");
            let chemin = dir.path().join("balayage.plx");
            for (rang, pas) in tirage.iter().enumerate() {
                monde.jouer(pas);
                plotyx_persistence::save(&monde.project, &chemin).expect("sauvegarde");
                let relu = plotyx_persistence::load(&chemin).expect("rechargement");
                prop_assert!(
                    **relu.tree() == **monde.project.tree(),
                    "pas {rang} ({pas:?}) : l'arbre relu diffère de l'arbre en mémoire"
                );
                prop_assert!(
                    empreintes(&relu) == empreintes(&monde.project),
                    "pas {rang} ({pas:?}) : les compteurs de version ne traversent pas le disque"
                );
            }
            Ok(())
        })
        .expect("aller-retour disque vérifié");
}

// -- reproductions --------------------------------------------------------

#[test]
fn b14_une_version_ne_designe_jamais_deux_contenus() {
    let mut runner = runner(16);
    runner
        .run(&prop::collection::vec(pas(), 60..80), |tirage| {
            let mut monde = Monde::new();
            let mut vu: HashMap<(NodeId, u64), Vec<u64>> = HashMap::new();
            for (rang, pas) in tirage.iter().enumerate() {
                monde.jouer(pas);
                for (id, (version, bits)) in empreintes(&monde.project) {
                    if let Some(anciens) = vu.get(&(id, version)) {
                        prop_assert!(
                            *anciens == bits,
                            "pas {rang} ({pas:?}) : {id} est à la version {version} avec un \
                             contenu différent de celui déjà vu sous ce numéro — un lecteur \
                             qui compare les versions se croira à jour"
                        );
                    }
                    vu.insert((id, version), bits);
                }
            }
            Ok(())
        })
        .expect("version ↔ contenu vérifié");
}

#[test]
fn b14_le_chemin_canonique_redesigne_toujours_son_noeud() {
    let mut runner = runner(16);
    runner
        .run(&prop::collection::vec(pas(), 60..80), |tirage| {
            let mut monde = Monde::new();
            for (rang, pas) in tirage.iter().enumerate() {
                monde.jouer(pas);
                for id in monde.project.tree().walk_all() {
                    let Ok(chemin) = monde.project.path_of(id) else {
                        continue;
                    };
                    prop_assert!(
                        monde.project.resolve_path(&chemin) == Ok(id),
                        "pas {rang} ({pas:?}) : path_of({id}) = {chemin:?}, qui se résout sur \
                         {:?}",
                        monde.project.resolve_path(&chemin)
                    );
                }
            }
            Ok(())
        })
        .expect("path_of ∘ resolve_path vérifié");
}
