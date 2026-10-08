//! L'état de la fenêtre d'analyse (#181) : le formulaire ouvert, la
//! confirmation de suppression d'un calcul, et les gestes qui lancent,
//! modifient, recalculent ou retirent un calcul.
//!
//! Tout passe par des commandes annulables (`AddCalculation`,
//! `UpdateCalculation`, `RemoveCalculation`, invariant 1) ou, pour le
//! recalcul, par `SessionHandle::recalculate` — hors historique, comme le
//! recalcul des formules. Le formulaire lui-même est un état d'interface :
//! jamais dans l'historique tant que l'utilisateur n'a pas validé.

use stokesforge_commands::Command;
use stokesforge_core::{Node, NodeId, SeriesRef};
use stokesforge_plot::PlotType;

use super::{series_from_columns, AppState};
use crate::analysis::{issue_message, AnalysisForm, Preset};
use crate::errors;
use crate::tr;

/// La réponse à la confirmation de suppression d'un calcul (M8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculationDelete {
    /// Retire le calcul ; ses colonnes deviennent des colonnes ordinaires.
    Calculation,
    /// Retire le calcul et ses colonnes de sortie.
    WithColumns,
    /// Ne fait rien.
    Cancel,
}

impl AppState {
    /// Le formulaire de la fenêtre d'analyse, s'il est ouvert.
    pub fn analysis_form(&self) -> Option<&AnalysisForm> {
        self.analysis.as_ref()
    }

    /// Le même, modifiable : la fenêtre y écrit ses réglages en direct.
    pub fn analysis_form_mut(&mut self) -> Option<&mut AnalysisForm> {
        self.analysis.as_mut()
    }

    /// Le calcul dont la suppression attend confirmation.
    pub fn pending_calculation_delete(&self) -> Option<NodeId> {
        self.pending_calculation_delete
    }

    /// Les calculs périmés du projet (menu Analysis › Recalculate Out of Date).
    pub fn stale_calculations(&self) -> Vec<NodeId> {
        self.snapshot
            .tree
            .calculations()
            .iter()
            .filter(|(_, calc)| calc.stale.is_some())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Ouvre la fenêtre sur les séries des colonnes sélectionnées de `sheet`
    /// (chaque Y avec le X le plus proche à sa gauche, comme un graphe).
    pub(super) fn open_analysis(&mut self, preset: &Preset, sheet: NodeId, columns: &[NodeId]) {
        let planned = series_from_columns(
            &self.snapshot.tree,
            sheet,
            columns,
            &PlotType::default(),
            0,
            0,
        );
        let series: Vec<SeriesRef> = planned
            .primary
            .iter()
            .chain(&planned.secondary)
            .map(|spec| SeriesRef {
                x: spec.x,
                y: spec.y,
                sigma: spec.y_error,
            })
            .collect();
        if series.is_empty() {
            self.fail(tr!("status-analysis-no-series"));
            return;
        }
        self.analysis = Some(AnalysisForm::new(
            preset,
            series,
            self.analysis_keep_results,
        ));
    }

    /// Rouvre un calcul existant (« Edit Calculation… »).
    pub(super) fn edit_calculation(&mut self, id: NodeId) {
        match self.snapshot.tree.calculation(id) {
            Some(calc) => self.analysis = Some(AnalysisForm::from_calculation(id, calc)),
            None => self.fail(tr!("status-analysis-gone")),
        }
    }

    /// Valide la fenêtre : un calcul neuf ou la modification de l'existant,
    /// en une entrée d'historique. La fenêtre reste ouverte si le calcul
    /// échoue.
    pub(super) fn apply_analysis(&mut self) {
        let Some(form) = self.analysis.clone() else {
            return;
        };
        let draft = match form.draft(&self.snapshot.tree) {
            Ok(draft) => draft,
            Err(issue) => {
                self.fail(issue_message(&issue));
                return;
            }
        };
        let command = match form.editing {
            Some(id) => Command::UpdateCalculation { id, draft },
            None => Command::AddCalculation { draft },
        };
        self.execute(command);
        if self.status.as_ref().is_some_and(|status| !status.is_error) {
            self.analysis_keep_results = form.keep_results;
            self.analysis = None;
        }
    }

    /// Recalcule `ids` ; vide : tous les calculs périmés.
    pub(super) fn recalculate_calculations(&mut self, ids: &[NodeId]) {
        match self.handle.recalculate(ids) {
            Ok(_) => {
                self.notice(tr!("status-analysis-recalculated"));
                self.refresh();
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// « Detach from Calculation » : le calcul est retiré, les colonnes
    /// restent et deviennent ordinaires.
    pub(super) fn detach_calculation(&mut self, id: NodeId) {
        self.execute(Command::RemoveCalculation {
            id,
            with_columns: false,
        });
    }

    /// Demande la suppression d'un calcul : ouvre la confirmation (M8).
    pub(super) fn request_delete_calculation(&mut self, id: NodeId) {
        if self.snapshot.tree.calculation(id).is_some() {
            self.pending_calculation_delete = Some(id);
        }
    }

    /// Réponse à la confirmation de suppression.
    pub(super) fn confirm_delete_calculation(&mut self, choice: CalculationDelete) {
        let Some(id) = self.pending_calculation_delete.take() else {
            return;
        };
        match choice {
            CalculationDelete::Cancel => {}
            CalculationDelete::Calculation => self.execute(Command::RemoveCalculation {
                id,
                with_columns: false,
            }),
            CalculationDelete::WithColumns => self.execute(Command::RemoveCalculation {
                id,
                with_columns: true,
            }),
        }
    }

    /// Le nom d'un calcul pour les menus et les confirmations : l'opération et
    /// la première colonne source.
    pub fn calculation_title(&self, id: NodeId) -> String {
        let Some(calc) = self.snapshot.tree.calculation(id) else {
            return String::new();
        };
        let kind = crate::analysis::kind_title(calc.operation.kind());
        let source = calc
            .series
            .first()
            .and_then(|s| self.snapshot.tree.node(s.y))
            .map(Node::name)
            .unwrap_or_default();
        tr!("analysis-title", kind = kind, column = source)
    }
}
