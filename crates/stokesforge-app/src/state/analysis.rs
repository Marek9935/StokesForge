//! L'état de la fenêtre d'analyse (#181) : le formulaire ouvert, la
//! confirmation de suppression d'un calcul, et les gestes qui lancent,
//! modifient, recalculent ou retirent un calcul.
//!
//! Tout passe par des commandes annulables (`AddCalculation`,
//! `UpdateCalculation`, `RemoveCalculation`, invariant 1) ou, pour le
//! recalcul, par `SessionHandle::recalculate` — hors historique, comme le
//! recalcul des formules. Le formulaire lui-même est un état d'interface :
//! jamais dans l'historique tant que l'utilisateur n'a pas validé.

use stokesforge_commands::HistoryLabel;
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{Node, NodeId, NodeKind, SeriesRef};
use stokesforge_plot::PlotType;

use super::{current_plot_spec, history_text, series_from_columns, AppState};
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

    /// Un glisser de poignée (#199) : les réglages du calcul changent comme
    /// dans la fenêtre d'analyse, et c'est la même commande, donc un pas
    /// d'historique par geste. Rien ne change (et rien n'entre dans
    /// l'historique) si la poignée n'a pas bougé.
    pub(super) fn move_calculation_handle(
        &mut self,
        id: NodeId,
        slot: crate::analysis::HandleSlot,
        x: f64,
        y: f64,
    ) {
        let Some(calc) = self.snapshot.tree.calculation(id) else {
            self.fail(tr!("status-analysis-gone"));
            return;
        };
        let mut form = AnalysisForm::from_calculation(id, calc);
        if !form.move_handle(slot, x, y) {
            return;
        }
        match form.draft(&self.snapshot.tree) {
            Ok(draft) => self.execute(Command::UpdateCalculation { id, draft }),
            Err(issue) => self.fail(issue_message(&issue)),
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
        let target = match (form.editing, form.add_to_graph, form.graph) {
            (None, true, Some(graph)) => Some(graph),
            _ => None,
        };
        match target {
            Some(graph) => self.add_calculation_to_graph(command, graph, &form),
            None => self.execute(command),
        }
        if self.status.as_ref().is_some_and(|status| !status.is_error) {
            self.analysis_keep_results = form.keep_results;
            // Le dernier choix devient le défaut des prochaines sessions (#200).
            // Un fichier de préférences illisible ne doit pas défaire un calcul réussi.
            let _ = self
                .object_manager_preferences
                .save_analysis_keep_results(form.keep_results);
            self.analysis = None;
        }
    }

    /// Les graphiques qui tracent au moins une des colonnes `ys` : ceux où
    /// « Add to graph » a un sens (#200).
    pub fn graphs_plotting(&self, ys: &[NodeId]) -> Vec<(NodeId, String)> {
        self.graphs()
            .into_iter()
            .filter(|(id, _)| {
                matches!(
                    self.snapshot.tree.node(*id).map(Node::kind),
                    Some(NodeKind::Graph { spec, .. })
                        if spec.panels.iter().any(|panel| {
                            panel.series.iter().any(|series| ys.contains(&series.y))
                        })
                )
            })
            .collect()
    }

    /// Crée le calcul **et** trace ses colonnes sur `graph`, en une seule
    /// entrée d'historique (#200). Les résidus ne sont pas tracés : leur échelle
    /// écraserait la courbe. Un échec à n'importe quelle étape avorte tout.
    fn add_calculation_to_graph(&mut self, command: Command, graph: NodeId, form: &AnalysisForm) {
        if self.handle.begin_macro(MacroKind::AnalysisToGraph).is_err() {
            return;
        }
        let before: Vec<NodeId> = self.snapshot.tree.calculations().keys().copied().collect();
        self.execute(command);
        let failed = |state: &Self| state.status.as_ref().is_some_and(|s| s.is_error);
        if failed(self) {
            let _ = self.handle.abort_macro();
            self.refresh();
            return;
        }
        let tree = &self.snapshot.tree;
        let columns: Vec<NodeId> = tree
            .calculations()
            .iter()
            .find(|(id, _)| !before.contains(id))
            .map(|(_, calc)| {
                calc.live_outputs(tree)
                    .filter(|output| output.key != "residuals")
                    .filter_map(|output| output.column)
                    .collect()
            })
            .unwrap_or_default();
        let ys: Vec<NodeId> = form.series.iter().map(|s| s.y).collect();
        let panel = current_plot_spec(tree, graph)
            .and_then(|spec| {
                spec.panels
                    .iter()
                    .position(|p| p.series.iter().any(|s| ys.contains(&s.y)))
            })
            .unwrap_or(0);
        self.add_series_to_graph(graph, panel, columns, None);
        if failed(self) {
            let _ = self.handle.abort_macro();
            self.refresh();
            return;
        }
        match self.handle.end_macro() {
            Ok(_) => self.notice(history_text(&HistoryLabel::Macro(
                MacroKind::AnalysisToGraph,
            ))),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
        self.refresh();
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

impl AppState {
    /// Les calculs qui gardent leurs résultats (« Keep in Results ») pour le
    /// classeur `book` — ceux dont la première série vit dans une de ses
    /// feuilles —, restreints à `sheet` quand elle est donnée (volet Results,
    /// M7).
    pub fn kept_calculations(&self, book: NodeId, sheet: Option<NodeId>) -> Vec<NodeId> {
        let tree = &self.snapshot.tree;
        tree.calculations()
            .iter()
            .filter(|(_, calc)| calc.keep_results)
            .filter(|(_, calc)| {
                let Some(first) = calc.series.first() else {
                    return false;
                };
                let Some(sheet_of) = tree.node(first.y).and_then(Node::parent) else {
                    return false;
                };
                tree.node(sheet_of).and_then(Node::parent) == Some(book)
                    && sheet.is_none_or(|wanted| wanted == sheet_of)
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

impl AppState {
    /// Pourquoi une colonne de sortie n'est pas à jour, si elle appartient à
    /// un calcul périmé (M9 a).
    pub fn column_staleness(&self, column: NodeId) -> Option<&stokesforge_core::Staleness> {
        let (id, _) = self.snapshot.tree.calculation_of_column(column)?;
        self.snapshot.tree.calculation(id)?.stale.as_ref()
    }

    /// La phrase qui dit pourquoi `column` est périmée, pour l'info-bulle de
    /// son en-tête ; `None` si la colonne est à jour.
    pub fn column_stale_text(&self, column: NodeId) -> Option<String> {
        use stokesforge_core::Staleness;
        Some(match self.column_staleness(column)? {
            Staleness::Manual => tr!("analysis-stale-manual"),
            Staleness::InputMissing => tr!("analysis-stale-input-missing"),
            Staleness::Failed(error) => tr!(
                "analysis-stale-failed",
                detail = errors::analysis::error_message(error)
            ),
        })
    }
}
