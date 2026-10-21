//! « Colonne utilisée par un calcul » (#306) : après la suppression de
//! colonnes (ou d'une feuille, d'un classeur) que lit un calcul, une fenêtre
//! d'information dit lesquels, avec deux réponses : `Close` ne fait rien,
//! `Undo` annule le geste.
//!
//! Le geste n'est jamais bloqué ni confirmé d'avance : il reste annulable
//! (invariant 1), la liste se calcule sur l'arbre d'**avant** (une colonne
//! supprimée n'a plus de nom) et la fenêtre s'ouvre après, seulement si le
//! geste a réussi. Les textes sont des `tr!` ici, jamais dans les crates du
//! dessous.
//!
//! Quatre sortes de lecteurs, et une seule exclusion :
//! - une colonne **à formule** de la même feuille, quand une de ses lettres
//!   désigne la colonne supprimée **ou une colonne qui la suit** (les lettres
//!   suivantes glissent : `C = B*2` désigne une autre colonne une fois A
//!   supprimée) ;
//! - un **calcul d'analyse** dont une série (X, Y, σ) est une colonne
//!   supprimée. La référence cachée d'une correction (`aux`) ne compte pas :
//!   c'est une copie. Un calcul dont une colonne de sortie disparaît avec le
//!   geste ne compte pas non plus : il disparaît avec elle.
//!
//! Les formules ne sont jamais réécrites (`docs/modele-de-donnees.md`).

use std::collections::BTreeSet;

use stokesforge_analysis::ops::OperationKind;
use stokesforge_commands::HistoryLabel;
use stokesforge_core::formula::Formula;
use stokesforge_core::{Node, NodeId, NodeKind, Tree};

use super::AppState;
use crate::analysis::kind_title;
use crate::tr;

/// La réponse à la question de Ctrl+Maj+C sur des colonnes à formule (#306).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullColumnsCopy {
    /// Copie les valeurs ; la ligne Formule reste vide.
    Values,
    /// Copie aussi les formules.
    Formulas,
    /// Ne copie rien.
    Cancel,
}

/// Un lecteur de colonne supprimée, en donnée : le texte se fait à l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ColumnUser {
    /// Une colonne à formule, nommée par son nom court (même feuille).
    Formula { name: String },
    /// Un calcul d'analyse : sa famille et sa première colonne de sortie,
    /// nommée `[Classeur]Feuille!Colonne` (ou par son nom court dans la
    /// feuille d'une colonne supprimée).
    Calculation {
        kind: OperationKind,
        output: Option<String>,
    },
}

/// La fenêtre ouverte : ce qu'elle liste, et l'étiquette d'annulation de
/// **ce** geste, pour que `Undo` n'annule rien d'autre.
#[derive(Debug, Clone)]
pub(super) struct ColumnUseNotice {
    users: Vec<ColumnUser>,
    undo_label: Option<HistoryLabel>,
}

/// Les colonnes (feuilles de données) sous `ids`, `ids` compris.
pub(super) fn columns_under(tree: &Tree, ids: &[NodeId]) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = ids.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(node) = tree.node(id) else {
            continue;
        };
        if node.as_column().is_some() {
            out.push(id);
        }
        if let Ok(children) = tree.children(id) {
            stack.extend(children.iter().rev().copied());
        }
    }
    out
}

/// Qui lit les colonnes `deleted` (à calculer **avant** la suppression).
pub(super) fn column_users(tree: &Tree, deleted: &[NodeId]) -> Vec<ColumnUser> {
    let gone: BTreeSet<NodeId> = deleted.iter().copied().collect();
    let mut users = Vec::new();
    // Colonnes à formule : par feuille, à partir du plus petit rang supprimé.
    let sheets: BTreeSet<NodeId> = deleted
        .iter()
        .filter_map(|&c| tree.node(c).and_then(Node::parent))
        .collect();
    for sheet in sheets {
        let Ok(columns) = tree.children(sheet) else {
            continue;
        };
        let Some(first) = columns.iter().position(|c| gone.contains(c)) else {
            continue;
        };
        for &column in columns {
            if gone.contains(&column) {
                continue;
            }
            let Some(node) = tree.node(column) else {
                continue;
            };
            let Some(text) = node.as_column().and_then(|c| c.meta().formula.clone()) else {
                continue;
            };
            let Ok(formula) = Formula::parse(&text) else {
                continue;
            };
            if formula.referenced_columns().iter().any(|&i| i >= first) {
                users.push(ColumnUser::Formula {
                    name: node.name().to_string(),
                });
            }
        }
    }
    // Calculs d'analyse.
    for calculation in tree.calculations().values() {
        let reads = calculation.series.iter().any(|s| {
            s.x.iter()
                .chain(std::iter::once(&s.y))
                .chain(s.sigma.iter())
                .any(|c| gone.contains(c))
        });
        let output_gone = calculation
            .outputs
            .iter()
            .any(|o| o.column.is_some_and(|c| gone.contains(&c)));
        if !reads || output_gone {
            continue;
        }
        let output = calculation
            .outputs
            .iter()
            .find_map(|o| o.column)
            .and_then(|c| column_path(tree, c));
        users.push(ColumnUser::Calculation {
            kind: calculation.operation.kind(),
            output,
        });
    }
    users
}

/// `[Classeur]Feuille!Colonne`, `None` si la colonne n'a pas de feuille.
fn column_path(tree: &Tree, column: NodeId) -> Option<String> {
    let node = tree.node(column)?;
    let sheet = tree.node(node.parent()?)?;
    let book = tree.node(sheet.parent()?)?;
    (matches!(sheet.kind(), NodeKind::Worksheet { .. }))
        .then(|| format!("[{}]{}!{}", book.name(), sheet.name(), node.name()))
}

impl AppState {
    /// Les colonnes lues, calculées avant le geste : `ids` sont les nœuds
    /// supprimés (colonnes, feuilles, classeurs).
    pub(super) fn column_users_before(&self, ids: &[NodeId]) -> Vec<ColumnUser> {
        let tree = &self.snapshot.tree;
        column_users(tree, &columns_under(tree, ids))
    }

    /// Ouvre la fenêtre après un geste réussi qui a rendu `users` orphelins.
    /// Rien si la liste est vide ou si le geste a échoué.
    pub(super) fn open_column_use(&mut self, users: Vec<ColumnUser>) {
        if users.is_empty() || self.last_action_failed() {
            return;
        }
        self.column_use = Some(ColumnUseNotice {
            users,
            undo_label: self.snapshot.history.undo_label.clone(),
        });
    }

    /// Les lignes de la fenêtre « colonne utilisée », ouverte ou non.
    pub fn column_use_lines(&self) -> Option<Vec<String>> {
        let notice = self.column_use.as_ref()?;
        Some(
            notice
                .users
                .iter()
                .map(|user| match user {
                    ColumnUser::Formula { name } => {
                        tr!("column-use-formula", name = name.as_str())
                    }
                    ColumnUser::Calculation {
                        kind,
                        output: Some(output),
                    } => tr!(
                        "column-use-calculation",
                        kind = kind_title(*kind),
                        output = output.as_str()
                    ),
                    ColumnUser::Calculation { kind, output: None } => {
                        tr!("column-use-calculation-bare", kind = kind_title(*kind))
                    }
                })
                .collect(),
        )
    }

    /// `Close` : ne fait rien.
    pub(super) fn close_column_use(&mut self) {
        self.column_use = None;
    }

    /// `Undo` : annule le geste, et seulement lui — si la pile n'a plus son
    /// étiquette en tête, on ferme sans rien annuler.
    pub(super) fn undo_column_use(&mut self) {
        let Some(notice) = self.column_use.take() else {
            return;
        };
        if self.snapshot.history.undo_label == notice.undo_label {
            self.undo();
        }
    }

    /// Réponse à la question de Ctrl+Maj+C : la copie se fait maintenant, et
    /// son texte part au presse-papiers système par la voie des objets.
    pub(super) fn confirm_full_columns_copy(&mut self, choice: FullColumnsCopy) {
        let Some(sheet) = self.pending_full_copy.take() else {
            return;
        };
        let keep_formula = match choice {
            FullColumnsCopy::Values => false,
            FullColumnsCopy::Formulas => true,
            FullColumnsCopy::Cancel => return,
        };
        if let Some(text) = self.copy_full_columns_with(sheet, keep_formula) {
            self.object_clipboard_output = Some(text);
        }
    }
}
