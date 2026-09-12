//! Propriété centrale du bus : **quoi qu'on lui fasse subir, annuler tout
//! ramène l'arbre initial bit à bit, et tout rétablir ramène l'arbre final**.
//!
//! C'est le test qui vaut le plus cher ici, parce que l'historique est hybride :
//! des instantanés de structure et des deltas de données s'entremêlent dans la
//! même pile, et seule une séquence aléatoire un peu longue met en évidence les
//! ordres d'annulation qui ne marchent pas.
//!
//! Le tirage est **déterministe** (`TestRng::deterministic_rng`) : un échec se
//! reproduit à l'identique, et aucun fichier de régression n'est écrit.
//!
//! Une commande refusée par le modèle (nom déjà pris, colonne disparue entre
//! temps...) est un **rejet de tirage**, pas un échec : on passe au pas suivant.
//! C'est la seule façon de tirer des séquences longues sans faire du générateur
//! une deuxième implémentation des règles du cœur.

use std::sync::Arc;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};

use plotyx_commands::{Command, History};
use plotyx_core::{Column, ColumnType, Node, NodeId, Project, ValueBlock};

// -- tirage ---------------------------------------------------------------

/// Valeurs tirées : des `NaN` en quantité (l'égalité du modèle est binaire,
/// `NaN == NaN`), un zéro négatif (binairement distinct de `0.0`), et de quoi
/// éprouver la conversion numérique ↔ texte.
const SAMPLE_VALUES: &[f64] = &[
    f64::NAN,
    f64::NAN,
    -f64::NAN,
    0.0,
    -0.0,
    1.0,
    -1.5,
    42.0,
    1e300,
    f64::INFINITY,
];

/// Un pas de tirage : les indices sont **relatifs** (ils seront ramenés à
/// l'état courant au moment de jouer), pour qu'un tirage reste jouable même
/// après des ajouts et des suppressions de colonnes.
#[derive(Debug, Clone)]
enum Step {
    Cells {
        column: usize,
        start: usize,
        values: Vec<f64>,
    },
    AddColumn {
        sheet: usize,
        index: usize,
    },
    DeleteColumn {
        column: usize,
    },
    Rename {
        column: usize,
    },
    Move {
        column: usize,
        sheet: usize,
        index: usize,
    },
    SetType {
        column: usize,
        kind: usize,
    },
    InsertRows {
        sheet: usize,
        at: usize,
        count: usize,
    },
    DeleteRows {
        sheet: usize,
        at: usize,
        count: usize,
    },
}

/// Un pas isolé, ou un groupe de 2-3 pas joués dans une macro.
#[derive(Debug, Clone)]
enum Script {
    Single(Step),
    Group(Vec<Step>),
}

fn value() -> impl Strategy<Value = f64> {
    prop::sample::select(SAMPLE_VALUES.to_vec())
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0usize..8, 0usize..6, prop::collection::vec(value(), 0..4)).prop_map(
            |(column, start, values)| Step::Cells {
                column,
                start,
                values
            }
        ),
        (0usize..2, 0usize..6).prop_map(|(sheet, index)| Step::AddColumn { sheet, index }),
        (0usize..8).prop_map(|column| Step::DeleteColumn { column }),
        (0usize..8).prop_map(|column| Step::Rename { column }),
        (0usize..8, 0usize..2, 0usize..6).prop_map(|(column, sheet, index)| Step::Move {
            column,
            sheet,
            index
        }),
        (0usize..8, 0usize..3).prop_map(|(column, kind)| Step::SetType { column, kind }),
        (0usize..2, 0usize..6, 1usize..4).prop_map(|(sheet, at, count)| Step::InsertRows {
            sheet,
            at,
            count
        }),
        (0usize..2, 0usize..6, 1usize..4).prop_map(|(sheet, at, count)| Step::DeleteRows {
            sheet,
            at,
            count
        }),
    ]
}

/// ~15 % de macros de 2 à 3 commandes.
fn script() -> impl Strategy<Value = Script> {
    prop_oneof![
        17 => step().prop_map(Script::Single),
        3 => prop::collection::vec(step(), 2..=3).prop_map(Script::Group),
    ]
}

/// Runner à graine fixe : reproductible, et sans fichier de régression sur le
/// disque du dépôt.
fn deterministic_runner() -> TestRunner {
    let config = Config {
        cases: 32,
        failure_persistence: None,
        ..Config::default()
    };
    TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha))
}

// -- interprétation -------------------------------------------------------

/// Le monde sur lequel un tirage est joué : la fixture des critères
/// d'acceptation, plus un compteur qui fabrique des noms de renommage jamais
/// pris (un renommage doit échouer pour de bonnes raisons, pas parce que le
/// générateur retire toujours le même nom du chapeau).
struct World {
    project: Project,
    sheets: Vec<NodeId>,
    next_name: u32,
}

impl World {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("projet Demo");
        let root = project.root();
        let book = project
            .add_workbook(root, Some("Book1"))
            .expect("classeur Book1");
        let sheet1 = project
            .resolve_path("[Book1]Sheet1")
            .expect("feuille Sheet1");
        project
            .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("colonne A");
        project
            .add_column(sheet1, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
            .expect("colonne B");
        project
            .add_column(
                sheet1,
                None,
                Some("T"),
                Column::from_text(&[Some("a".to_string()), None, Some("c".to_string())]),
            )
            .expect("colonne T");
        let sheet2 = project
            .add_worksheet(book, Some("Sheet2"))
            .expect("feuille Sheet2");
        project
            .add_column(sheet2, None, Some("X"), Column::from_f64(&[0.5, 1.5]))
            .expect("colonne X");
        Self {
            project,
            sheets: vec![sheet1, sheet2],
            next_name: 0,
        }
    }

    fn children_of(&self, sheet: NodeId) -> Vec<NodeId> {
        self.project
            .tree()
            .children(sheet)
            .map(<[NodeId]>::to_vec)
            .unwrap_or_default()
    }

    /// Toutes les colonnes vivantes, les deux feuilles à la suite.
    fn columns(&self) -> Vec<NodeId> {
        self.sheets
            .iter()
            .flat_map(|sheet| self.children_of(*sheet))
            .collect()
    }

    fn column_type(&self, column: NodeId) -> Option<ColumnType> {
        self.project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .map(Column::column_type)
    }

    /// Traduit un pas en commande jouable sur l'état courant, ou `None` si le
    /// pas n'a pas de cible (plus aucune colonne, par exemple).
    fn command_for(&mut self, step: &Step) -> Option<Command> {
        let columns = self.columns();
        match step {
            Step::Cells {
                column,
                start,
                values,
            } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let block = match self.column_type(target)? {
                    ColumnType::Text => ValueBlock::Text(
                        values
                            .iter()
                            .map(|v| {
                                if v.is_nan() {
                                    None
                                } else {
                                    Some(v.to_string())
                                }
                            })
                            .collect(),
                    ),
                    ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(values.clone()),
                };
                Some(Command::SetCells {
                    column: target,
                    start: start % 8,
                    values: block,
                })
            }
            Step::AddColumn { sheet, index } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                let count = self.children_of(sheet).len();
                Some(Command::AddColumn {
                    sheet,
                    index: Some(index % (count + 1)),
                    name: None,
                    column: Column::from_f64(&[7.0, f64::NAN]),
                })
            }
            Step::DeleteColumn { column } => {
                // Toujours garder de quoi jouer les pas suivants.
                if columns.len() <= 1 {
                    return None;
                }
                Some(Command::DeleteColumn {
                    column: *columns.get(column % columns.len())?,
                })
            }
            Step::Rename { column } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let name = format!("K{}", self.next_name);
                self.next_name += 1;
                Some(Command::Rename { id: target, name })
            }
            Step::Move {
                column,
                sheet,
                index,
            } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                let count = self.children_of(sheet).len();
                Some(Command::Move {
                    id: target,
                    new_parent: sheet,
                    index: index % (count + 1),
                })
            }
            Step::SetType { column, kind } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let column_type = match kind % 3 {
                    0 => ColumnType::Numeric,
                    1 => ColumnType::DateTime,
                    _ => ColumnType::Text,
                };
                Some(Command::SetColumnType {
                    column: target,
                    column_type,
                })
            }
            Step::InsertRows { sheet, at, count } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                Some(Command::InsertRows {
                    sheet,
                    at: *at,
                    count: *count,
                })
            }
            Step::DeleteRows { sheet, at, count } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                Some(Command::DeleteRows {
                    sheet,
                    at: *at,
                    count: *count,
                })
            }
        }
    }

    /// Joue un pas. Rend `true` s'il a été accepté ; un refus est un rejet de
    /// tirage, silencieux.
    fn play_step(&mut self, history: &mut History, step: &Step) -> bool {
        match self.command_for(step) {
            Some(command) => history.execute(&mut self.project, command).is_ok(),
            None => false,
        }
    }

    /// Joue un scénario complet et rend le nombre de commandes acceptées.
    fn play(&mut self, history: &mut History, scripts: &[Script]) -> usize {
        let mut accepted = 0;
        for script in scripts {
            match script {
                Script::Single(step) => {
                    if self.play_step(history, step) {
                        accepted += 1;
                    }
                }
                Script::Group(steps) => {
                    // B7 : un refus (profondeur d'imbrication au plafond) est
                    // un rejet de tirage, silencieux comme les autres — voir
                    // la doc de `play_step`. Non-imbriqué ici (un groupe ne
                    // s'ouvre jamais avant que le précédent ne se soit
                    // refermé), donc jamais atteint en pratique.
                    if history.begin_macro("Groupe").is_err() {
                        return accepted;
                    }
                    for step in steps {
                        if self.play_step(history, step) {
                            accepted += 1;
                        }
                    }
                    // Une macro vide ne laisse aucune entrée : rien à traiter à
                    // part si toutes les commandes du groupe ont été refusées.
                    if history.end_macro(&mut self.project).is_err() {
                        return accepted;
                    }
                }
            }
        }
        accepted
    }

    /// Joue des pas isolés jusqu'à `target` commandes acceptées.
    fn play_until(&mut self, history: &mut History, steps: &[Step], target: usize) -> usize {
        let mut accepted = 0;
        for step in steps {
            if accepted >= target {
                break;
            }
            if self.play_step(history, step) {
                accepted += 1;
            }
        }
        accepted
    }
}

fn undo_all(history: &mut History, project: &mut Project) -> Result<(), TestCaseError> {
    loop {
        match history.undo(project) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(err) => return Err(TestCaseError::fail(format!("annulation refusée : {err}"))),
        }
    }
}

fn redo_all(history: &mut History, project: &mut Project) -> Result<(), TestCaseError> {
    loop {
        match history.redo(project) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(err) => {
                return Err(TestCaseError::fail(format!(
                    "rétablissement refusé : {err}"
                )))
            }
        }
    }
}

// -- 22. aller-retour complet --------------------------------------------

#[test]
fn undoing_everything_restores_the_initial_tree_bit_for_bit() {
    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(script(), 70..=90), |scripts| {
            let mut world = World::new();
            let mut history = History::with_limit(0);
            let initial = Arc::clone(world.project.tree());

            let accepted = world.play(&mut history, &scripts);
            prop_assert!(
                accepted >= 50,
                "tirage trop dégénéré : seulement {accepted} commandes acceptées"
            );

            let last = Arc::clone(world.project.tree());
            prop_assert!(
                *last != *initial,
                "le tirage n'a rien changé : le test ne prouverait rien"
            );

            undo_all(&mut history, &mut world.project)?;
            // Égalité binaire, `NaN` compris — et les `NodeId` vivent dans
            // les nœuds, donc cette comparaison couvre aussi le retour d'un
            // nœud supprimé sous son identifiant d'origine.
            prop_assert!(
                **world.project.tree() == *initial,
                "après annulation complète, l'arbre diffère de l'initial"
            );

            redo_all(&mut history, &mut world.project)?;
            prop_assert!(
                **world.project.tree() == *last,
                "après rétablissement complet, l'arbre diffère du final"
            );
            Ok(())
        })
        .expect("propriété d'aller-retour vérifiée");
}

// -- 23. déterminisme ----------------------------------------------------

#[test]
fn replaying_the_same_draw_yields_the_same_history_labels() {
    /// Joue un tirage et rend les libellés de la pile, du sommet vers le fond.
    fn labels(scripts: &[Script]) -> Vec<String> {
        let mut world = World::new();
        let mut history = History::with_limit(0);
        world.play(&mut history, scripts);
        let mut collected = Vec::new();
        while let Some(label) = history.undo_label().map(str::to_string) {
            collected.push(label);
            if history.undo(&mut world.project) != Ok(true) {
                break;
            }
        }
        collected
    }

    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(script(), 70..=90), |scripts| {
            prop_assert_eq!(labels(&scripts), labels(&scripts));
            Ok(())
        })
        .expect("libellés déterministes");
}

// -- 24. sous limite -----------------------------------------------------

#[test]
fn a_bounded_history_still_round_trips_within_its_window() {
    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(step(), 60..=80), |steps| {
            let mut world = World::new();
            let mut history = History::with_limit(5);

            let accepted = world.play_until(&mut history, &steps, 30);
            prop_assert!(
                accepted == 30,
                "tirage trop dégénéré : {accepted} commandes acceptées sur 30 attendues"
            );
            prop_assert_eq!(history.len(), 5);

            let before = Arc::clone(world.project.tree());
            for _ in 0..3 {
                prop_assert_eq!(history.undo(&mut world.project), Ok(true));
            }
            for _ in 0..3 {
                prop_assert_eq!(history.redo(&mut world.project), Ok(true));
            }
            prop_assert!(
                **world.project.tree() == *before,
                "trois annulations puis trois rétablissements doivent être neutres"
            );
            Ok(())
        })
        .expect("propriété sous limite vérifiée");
}
