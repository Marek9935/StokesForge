//! The state of the analysis window (#181): the open form, the confirmation
//! of the deletion of a calculation, and the gestures that launch, modify,
//! recalculate or remove a calculation.
//!
//! Everything goes through undoable commands (`AddCalculation`,
//! `UpdateCalculation`, `RemoveCalculation`, invariant 1) or, for
//! recalculation, through `SessionHandle::recalculate`, outside the history,
//! like formula recalculation. The form itself is an interface state: never
//! in the history until the user has validated.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_commands::HistoryLabel;
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{Node, NodeId, NodeKind, SeriesRef};
use stokesforge_plot::PlotType;

use stokesforge_core::compose::{current_plot_spec, series_from_columns, OVERLAY_ROLES};

use super::{history_text, Action, AppState};
use crate::analysis::{issue_message, AnalysisForm, Preset, RecentTarget};
use crate::analysis_recent::{remember, RecentAnalysis};
use crate::errors;
use crate::r_squared;
use crate::tr;

/// The answer to the confirmation of the deletion of a calculation (M8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculationDelete {
    /// Removes the calculation; its columns become ordinary columns.
    Calculation,
    /// Removes the calculation and its output columns.
    WithColumns,
    /// Does nothing.
    Cancel,
}

/// The answer to the confirmation of an export that shows out-of-date series
/// (#198, M9 a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleExport {
    /// Exports the figure as it is displayed: greyed series and marker.
    Anyway,
    /// Exports the figure without the warning (normal colors, no marker).
    WithoutWarning,
    /// First recalculates the out-of-date calculations, then exports.
    Recalculate,
    /// N'exporte rien.
    Cancel,
}

impl AppState {
    /// The form of the analysis window, if it is open.
    pub fn analysis_form(&self) -> Option<&AnalysisForm> {
        self.analysis.as_ref()
    }

    /// The same, mutable: the window writes its settings into it live.
    pub fn analysis_form_mut(&mut self) -> Option<&mut AnalysisForm> {
        self.analysis.as_mut()
    }

    /// The calculation whose deletion is waiting for confirmation.
    pub fn pending_calculation_delete(&self) -> Option<NodeId> {
        self.pending_calculation_delete
    }

    /// The out-of-date calculations of the project (Analysis › Recalculate Out of Date menu).
    pub fn stale_calculations(&self) -> Vec<NodeId> {
        self.snapshot
            .tree
            .calculations()
            .iter()
            .filter(|(_, calc)| calc.stale.is_some())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Opens the window on the series of the selected columns of `sheet`
    /// (each Y with the nearest X on its left, like a graph).
    pub(super) fn open_analysis(&mut self, preset: &Preset, sheet: NodeId, columns: &[NodeId]) {
        let planned = series_from_columns(
            &self.snapshot.tree,
            sheet,
            columns,
            &PlotType::default(),
            0,
            [0; OVERLAY_ROLES],
        );
        let series: Vec<SeriesRef> = planned
            .primary
            .iter()
            .chain(planned.overlay_series())
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
        let keep = self.keep_results_for(preset.kind());
        self.analysis = Some(AnalysisForm::new(preset, series, keep));
    }

    /// "Keep in Results" an analysis window of `kind` opens with: the last
    /// choice validated for this operation, else the Preferences default (M5).
    pub(super) fn keep_results_for(&self, kind: OperationKind) -> bool {
        self.object_manager_preferences
            .analysis_keep_results_for(kind)
    }

    /// The Preferences default of "Keep in Results" (General › Analysis).
    pub fn analysis_keep_results_default(&self) -> bool {
        self.object_manager_preferences
            .analysis_keep_results_default()
    }

    /// The graph designates its own X/Y: no sheet pairing here.
    pub(super) fn open_graph_analysis(
        &mut self,
        preset: &Preset,
        graph: NodeId,
        sources: &[stokesforge_plot::SeriesRef],
        spec: stokesforge_plot::PlotSpec,
    ) {
        // "Row" curves (#253): the columns of the linked transposed sheet are
        // analyzed, never the row itself.
        let curves: Vec<_> = sources
            .iter()
            .filter_map(|source| spec.panels.get(source.panel)?.series.get(source.index))
            .collect();
        if curves.iter().any(|curve| curve.row.is_some()) {
            let rows: Option<Vec<(NodeId, usize)>> = curves
                .iter()
                .map(|curve| {
                    let sheet = self.snapshot.tree.parent(curve.y).ok().flatten()?;
                    Some((sheet, curve.row?))
                })
                .collect();
            match rows {
                Some(rows) if rows.len() == sources.len() => self.open_row_analysis(preset, &rows),
                _ => self.fail(tr!("status-analysis-mixed-curves")),
            }
            return;
        }
        let series: Option<Vec<SeriesRef>> = sources
            .iter()
            .map(|source| {
                let curve = spec.panels.get(source.panel)?.series.get(source.index)?;
                // A row series has other semantics than columns; it must never
                // silently analyze its whole column.
                if curve.row.is_some() || self.snapshot.tree.node(curve.y).is_none() {
                    return None;
                }
                Some(SeriesRef {
                    x: curve.x,
                    y: curve.y,
                    sigma: curve.y_error,
                })
            })
            .collect();
        let Some(series) = series.filter(|s| !s.is_empty()) else {
            self.fail(tr!("status-analysis-no-series"));
            return;
        };
        let keep = self.keep_results_for(preset.kind());
        let mut form = AnalysisForm::new(preset, series, keep);
        form.graph = Some(graph);
        form.add_to_graph = true;
        form.graph_source = Some((spec, sources.to_vec()));
        self.analysis = Some(form);
    }

    /// The latest analyses launched, the most recent first (#249).
    pub fn analysis_recent(&self) -> &[RecentAnalysis] {
        &self.analysis_recent
    }

    /// Reopens one of the latest analyses (#249) on the selection `target`,
    /// with its settings. With `run` (Ctrl+click), launches it right away when
    /// nothing is missing; points, zones and reference column are not
    /// remembered: then the window stays open, settings in place, to set
    /// them. "Replace original data" is taken back only if the selection
    /// allows it (ordinary columns).
    pub(super) fn open_recent_analysis(&mut self, index: usize, run: bool, target: RecentTarget) {
        let Some(recent) = self.analysis_recent.get(index).cloned() else {
            return;
        };
        let preset = recent.preset();
        match target {
            RecentTarget::Columns { sheet, columns } => {
                self.open_analysis(&preset, sheet, &columns)
            }
            RecentTarget::Graph {
                graph,
                series,
                spec,
            } => self.open_graph_analysis(&preset, graph, &series, *spec),
            RecentTarget::Rows { rows } => self.open_row_analysis(&preset, &rows),
        }
        let tree = self.snapshot.tree.clone();
        let Some(form) = self.analysis.as_mut() else {
            return;
        };
        recent.apply(form);
        form.replace_original =
            recent.replace_original && form.replacement_disabled(&tree).is_none();
        if !run || !form.issues().is_empty() {
            return;
        }
        let replaced = form.replace_original;
        self.apply_analysis();
        if replaced && self.analysis.is_none() && !self.pending_analysis_replacement {
            self.notice(tr!("status-analysis-replaced", label = recent.label()));
        }
    }

    /// Remembers the analysis that the form has just launched (#249). An
    /// unreadable preferences file must not undo a successful calculation.
    fn remember_analysis(&mut self, form: &AnalysisForm) {
        let Some(recent) = RecentAnalysis::from_form(form) else {
            return;
        };
        remember(&mut self.analysis_recent, recent);
        let _ = self
            .object_manager_preferences
            .save_analysis_recent(&self.analysis_recent);
    }

    /// Reopens an existing calculation ("Edit Calculation…").
    pub(super) fn edit_calculation(&mut self, id: NodeId) {
        match self.snapshot.tree.calculation(id) {
            Some(calc) => self.analysis = Some(AnalysisForm::from_calculation(id, calc)),
            None => self.fail(tr!("status-analysis-gone")),
        }
    }

    /// A handle drag (#199): the settings of the calculation change as in the
    /// analysis window, and it is the same command, hence one history step
    /// per gesture. Nothing changes (and nothing enters the history) if the
    /// handle did not move.
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

    /// Validates the window: a new calculation or the modification of the
    /// existing one, in a single history entry. The window stays open if the
    /// calculation fails.
    pub fn pending_analysis_replacement(&self) -> bool {
        self.pending_analysis_replacement
    }

    pub(super) fn confirm_analysis_replacement(&mut self, accept: bool) {
        if !self.pending_analysis_replacement {
            return;
        }
        self.pending_analysis_replacement = false;
        if accept {
            self.apply_analysis_confirmed(true);
        }
    }

    pub(super) fn apply_analysis(&mut self) {
        self.apply_analysis_confirmed(false);
    }

    fn apply_analysis_confirmed(&mut self, confirmed: bool) {
        let Some(form) = self.analysis.clone() else {
            return;
        };
        if form.replace_original {
            if let Some(reason) = form.replacement_disabled(&self.snapshot.tree) {
                self.fail(reason);
                return;
            }
            let ys: Vec<NodeId> = form.series.iter().map(|s| s.y).collect();
            let graphs: Vec<_> = self
                .graphs()
                .iter()
                .filter_map(|(id, _)| current_plot_spec(&self.snapshot.tree, *id))
                .collect();
            let shared = ys.iter().any(|y| {
                graphs
                    .iter()
                    .map(|s| {
                        s.panels
                            .iter()
                            .flat_map(|p| p.series.iter())
                            .filter(|s| s.y == *y)
                            .count()
                    })
                    .sum::<usize>()
                    > 1
            });
            if shared && !confirmed {
                self.pending_analysis_replacement = true;
                return;
            }
        }
        let (mut draft, all_values) = match form.draft_with_values(&self.snapshot.tree) {
            Ok(drafted) => drafted,
            Err(issue) => {
                self.fail(issue_message(&issue));
                return;
            }
        };
        let command = if form.replace_original {
            draft.complete_for_replacement(&self.snapshot.tree);
            let mut inputs: Vec<NodeId> = draft
                .series
                .iter()
                .flat_map(|s| [Some(s.y), s.x, s.sigma].into_iter().flatten())
                .collect();
            if let Some(aux) = &draft.aux {
                inputs.push(aux.y);
                inputs.extend(aux.x);
            }
            inputs.sort();
            inputs.dedup();
            let expected_versions = inputs
                .into_iter()
                .filter_map(|id| {
                    self.snapshot
                        .tree
                        .node(id)
                        .and_then(|n| n.as_column())
                        .map(|c| (id, c.version()))
                })
                .collect();
            Command::ReplaceAnalysisData {
                draft,
                expected_versions,
            }
        } else {
            match form.editing {
                Some(id) => Command::UpdateCalculation { id, draft },
                None => Command::AddCalculation { draft },
            }
        };
        let target = match (
            form.editing,
            form.add_to_graph && !form.replace_original,
            form.graph,
        ) {
            (None, true, Some(graph)) => Some(graph),
            _ => None,
        };
        match target {
            Some(graph) => self.add_calculation_to_graph(command, graph, &form),
            None => self.execute(command),
        }
        // The failure goes into the footer of the window (S-M7, #228): it
        // stays open.
        if !self.last_action_failed() {
            self.remember_analysis(&form);
            // The choice is remembered for this operation only: the Preferences
            // default is not changed by validating a window (#148, M5).
            // An unreadable preferences file must not undo a successful calculation.
            let _ = self
                .object_manager_preferences
                .save_analysis_keep_results_for(form.kind, form.keep_results);
            self.analysis = None;
            self.end_row_macro();
            // A "Show" calculation does not keep its values: recalculation will
            // never see them, so this is where a very low R² is reported (#288).
            // A calculation that keeps its values is seen by `watch_low_r2`.
            if !form.keep_results && !form.replace_original && form.low_r2_dialog {
                let kind = form.kind;
                let count = all_values
                    .iter()
                    .filter(|result| low_r2_of(&result.values).is_some())
                    .count();
                self.raise_low_r2(&[(kind, count)]);
            }
        }
    }

    /// Opens the "Low R²" box (#288) for `found` (per calculation, its type
    /// and its number of series at fault), unless the application preference
    /// turns it off, there is nothing to say, or an alert is already waiting
    /// to be read (a single slot: a save or open alert is not overwritten).
    /// The text is a counter per type (#290). What could not open is not
    /// lost: the badge of the status bar (`low_r2_groups`) says it as long as
    /// the R² stay low.
    fn raise_low_r2(&mut self, found: &[(OperationKind, usize)]) {
        let groups = r_squared::group_by_kind(found.iter().copied());
        if groups.is_empty()
            || self.alert.is_some()
            || !self.object_manager_preferences.analysis_low_r2_dialog()
        {
            return;
        }
        self.alert = Some(super::Alert {
            kind: super::AlertKind::LowRSquared,
            message: r_squared::summary_text(&groups),
        });
    }

    /// Recomputes the low R² of the project (#290) from the current
    /// snapshot: a **state**, not an event. Called at every new snapshot,
    /// before `watch_low_r2`; the group disappears as soon as the R² go back up.
    pub(super) fn refresh_low_r2_groups(&mut self) {
        self.low_r2_groups = r_squared::project_groups(&self.snapshot.tree);
        if self.low_r2_groups.is_empty() {
            self.low_r2_list_open = false;
        }
    }

    /// The series with R² < 0.6 of the whole project, per type of calculation,
    /// in the fixed order of the types (#290). Also counts the calculations
    /// whose box is turned off; not the "Show" calculations, which do not keep
    /// their values.
    pub fn low_r2_groups(&self) -> &[r_squared::LowRSquaredGroup] {
        &self.low_r2_groups
    }

    /// The number of series with R² < 0.6 in the project: the badge of the
    /// status bar exists only if it is positive.
    pub fn low_r2_total(&self) -> usize {
        r_squared::total(&self.low_r2_groups)
    }

    /// Is the badge list open?
    pub fn low_r2_list_open(&self) -> bool {
        self.low_r2_list_open
    }

    /// The text of the badge list: the same counters as the box.
    pub fn low_r2_list_text(&self) -> String {
        r_squared::summary_text(&self.low_r2_groups)
    }

    /// Takes the fingerprint of the very low R² of the calculations of `tree`,
    /// saying nothing: opening a project is not a recalculation.
    pub(super) fn seed_low_r2(&mut self) {
        self.low_r2_seen = self
            .snapshot
            .tree
            .calculations()
            .iter()
            .filter_map(|(id, calc)| {
                let print = r_squared::dialog_fingerprint(calc);
                (!print.is_empty()).then_some((*id, print))
            })
            .collect();
    }

    /// Called at every new snapshot: a calculation that keeps its values and
    /// whose very low R² **has just changed** (manual or automatic
    /// recalculation, apply, undo) opens the box (#288). A calculation whose
    /// result has not moved does not reopen it at every frame.
    pub(super) fn watch_low_r2(&mut self) {
        let tree = self.snapshot.tree.clone();
        let mut found = Vec::new();
        let mut seen = std::mem::take(&mut self.low_r2_seen);
        seen.retain(|id, _| tree.calculation(*id).is_some());
        for (id, calc) in tree.calculations() {
            let print = r_squared::dialog_fingerprint(calc);
            if print.is_empty() {
                seen.remove(id);
            } else if seen.get(id) != Some(&print) {
                if calc.low_r2_dialog {
                    found.push((calc.operation.kind(), r_squared::low_series_count(calc)));
                }
                seen.insert(*id, print);
            }
        }
        self.low_r2_seen = seen;
        self.raise_low_r2(&found);
    }

    /// The graphs that plot at least one of the columns `ys`: those where
    /// "Add to graph" makes sense (#200).
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

    /// Creates the calculation **and** plots its columns on `graph`, in a
    /// single history entry (#200). The residuals are not plotted: their scale
    /// would crush the curve. A failure at any step aborts everything.
    fn add_calculation_to_graph(&mut self, command: Command, graph: NodeId, form: &AnalysisForm) {
        if self.handle.begin_macro(MacroKind::AnalysisToGraph).is_err() {
            return;
        }
        let before: Vec<NodeId> = self.snapshot.tree.calculations().keys().copied().collect();
        self.execute(command);
        if self.last_action_failed() {
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
        if form.graph == Some(graph) && form.graph_source.is_some() {
            self.add_graph_analysis_outputs(graph, form, &before);
        } else {
            self.add_series_to_graph(graph, panel, columns, None);
        }
        if self.last_action_failed() {
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

    /// The result joins exactly the layer and the group of its source,
    /// with its explicit X (the order of the sheet is not used for pairing).
    fn add_graph_analysis_outputs(
        &mut self,
        graph: NodeId,
        form: &AnalysisForm,
        before: &[NodeId],
    ) {
        let Some((source_spec, sources)) = &form.graph_source else {
            return;
        };
        let Some(mut spec) = current_plot_spec(&self.snapshot.tree, graph) else {
            self.fail(tr!("error-graph-gone"));
            return;
        };
        let Some((_, calc)) = self
            .snapshot
            .tree
            .calculations()
            .iter()
            .find(|(id, _)| !before.contains(id))
        else {
            return;
        };
        for output in calc
            .live_outputs(&self.snapshot.tree)
            .filter(|o| o.key != "residuals")
        {
            let (Some(column), Some(source)) = (output.column, sources.get(output.series)) else {
                continue;
            };
            let Some(curve) = source_spec
                .panels
                .get(source.panel)
                .and_then(|p| p.series.get(source.index))
            else {
                continue;
            };
            let Some(panel) = spec.panels.get_mut(source.panel) else {
                continue;
            };
            let mut result = curve.clone();
            result.y = column;
            result.y_error = None;
            result.name.clear();
            result.visible = true;
            // No new group: the number belongs to the source layer.
            panel.series.push(result);
        }
        self.execute(Command::SetPlotSpec { graph, spec });
    }

    /// Recalculates `ids`; empty: all the out-of-date calculations.
    pub(super) fn recalculate_calculations(&mut self, ids: &[NodeId]) {
        match self.handle.recalculate(ids) {
            Ok(_) => {
                self.notice(tr!("status-analysis-recalculated"));
                self.refresh();
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// "Detach from Calculation": the calculation is removed, the columns
    /// stay and become ordinary.
    pub(super) fn detach_calculation(&mut self, id: NodeId) {
        self.execute(Command::RemoveCalculation {
            id,
            with_columns: false,
        });
    }

    /// Asks for the deletion of a calculation: opens the confirmation (M8).
    pub(super) fn request_delete_calculation(&mut self, id: NodeId) {
        if self.snapshot.tree.calculation(id).is_some() {
            self.pending_calculation_delete = Some(id);
        }
    }

    /// Answer to the deletion confirmation.
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

    /// The name of a calculation for menus and confirmations: the operation
    /// and the first source column.
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
    /// The correction traces of the workbook, in their creation order.
    pub fn correction_traces(&self, book: NodeId, sheet: Option<NodeId>) -> Vec<NodeId> {
        let tree = &self.snapshot.tree;
        tree.correction_traces()
            .iter()
            .filter(|(_, trace)| {
                trace.series.iter().any(|s| {
                    let parent = tree.node(s.y).and_then(Node::parent);
                    parent.is_some_and(|p| {
                        tree.node(p).and_then(Node::parent) == Some(book)
                            && sheet.is_none_or(|wanted| wanted == p)
                    })
                })
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// The calculations that keep their results ("Keep in Results") for the
    /// workbook `book`: those whose first series lives in one of its
    /// sheets, restricted to `sheet` when it is given (Results pane,
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
    /// Why an output column is not up to date, if it belongs to an
    /// out-of-date calculation (M9 a).
    pub fn column_staleness(&self, column: NodeId) -> Option<&stokesforge_core::Staleness> {
        let (id, _) = self.snapshot.tree.calculation_of_column(column)?;
        self.snapshot.tree.calculation(id)?.stale.as_ref()
    }

    /// The sentence that says why `column` is out of date, for the tooltip of
    /// its header; `None` if the column is up to date.
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

impl AppState {
    /// The out-of-date calculations one of whose columns is plotted by the
    /// visible series of `spec`; empty when the graph turned the warning off.
    pub fn stale_calculations_in(&self, spec: &stokesforge_plot::PlotSpec) -> Vec<NodeId> {
        if !spec.stale_warning {
            return Vec::new();
        }
        let tree = &self.snapshot.tree;
        let resolved = stokesforge_core::resolve::resolve_spec(tree, spec);
        let mut calculations = Vec::new();
        for series in resolved
            .panels
            .iter()
            .flat_map(|panel| panel.series.iter())
            .filter(|series| series.stale && series.spec.visible)
        {
            let spec = series.spec;
            for column in [Some(spec.y), spec.x, spec.x_error, spec.y_error]
                .into_iter()
                .flatten()
            {
                if let Some((id, _)) = tree.calculation_of_column(column) {
                    if tree.calculation(id).is_some_and(|c| c.stale.is_some())
                        && !calculations.contains(&id)
                    {
                        calculations.push(id);
                    }
                }
            }
        }
        calculations
    }

    /// The export (or copy) waiting for confirmation, with the number of
    /// out-of-date calculations it shows.
    pub fn pending_stale_export(&self) -> Option<usize> {
        let spec = match self.pending_stale_export.as_ref()? {
            Action::ExportGraph { spec, .. } | Action::CopyGraphFigure { spec } => spec,
            _ => return None,
        };
        Some(self.stale_calculations_in(spec).len())
    }

    /// Exports or copies right away if the figure is up to date; otherwise
    /// puts the action on hold and lets the window ask (#198).
    pub(super) fn export_unless_stale(&mut self, action: Action) {
        let stale = match &action {
            Action::ExportGraph { spec, .. } | Action::CopyGraphFigure { spec } => {
                !self.stale_calculations_in(spec).is_empty()
            }
            _ => false,
        };
        if stale {
            self.pending_stale_export = Some(action);
        } else {
            self.run_export(action);
        }
    }

    fn run_export(&mut self, action: Action) {
        match action {
            Action::ExportGraph { path, spec } => self.export_graph(path, &spec),
            Action::CopyGraphFigure { spec } => self.copy_graph_figure(&spec),
            _ => {}
        }
    }

    /// Answer to the export confirmation.
    pub(super) fn confirm_stale_export(&mut self, choice: StaleExport) {
        let Some(mut action) = self.pending_stale_export.take() else {
            return;
        };
        match choice {
            StaleExport::Cancel => return,
            StaleExport::Anyway => {}
            StaleExport::WithoutWarning => {
                if let Action::ExportGraph { spec, .. } | Action::CopyGraphFigure { spec } =
                    &mut action
                {
                    spec.stale_warning = false;
                }
            }
            StaleExport::Recalculate => {
                let stale = match &action {
                    Action::ExportGraph { spec, .. } | Action::CopyGraphFigure { spec } => {
                        self.stale_calculations_in(spec)
                    }
                    _ => Vec::new(),
                };
                self.recalculate_calculations(&stale);
                if self.status.as_ref().is_some_and(|s| s.is_error()) {
                    return;
                }
            }
        }
        self.run_export(action);
    }
}

/// The R² of a series that crosses the threshold of the box, if there is one
/// (the lowest if several: a global fit has only one per series).
fn low_r2_of(values: &[stokesforge_core::ResultValue]) -> Option<f64> {
    values
        .iter()
        .filter(|v| r_squared::level_of_value(&v.key, v.value) == r_squared::RSquaredLevel::Dialog)
        .map(|v| v.value)
        .min_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::low_r2_of;
    use stokesforge_core::ResultValue;

    fn value_of(key: &str, value: f64) -> ResultValue {
        ResultValue {
            key: key.to_string(),
            index: None,
            value,
            error: None,
        }
    }

    #[test]
    fn low_r2_of_returns_the_lowest_below_the_box_threshold() {
        let values = [
            value_of("stat.r_squared", 0.4),
            value_of("stat.r_squared", -2.0),
            value_of("stat.r_squared", 0.5),
            value_of("param.tau", -9.0),
        ];
        assert_eq!(low_r2_of(&values), Some(-2.0));
    }

    #[test]
    fn low_r2_of_returns_nothing_outside_the_threshold_or_the_key() {
        assert_eq!(low_r2_of(&[]), None);
        assert_eq!(low_r2_of(&[value_of("stat.r_squared", 0.6)]), None);
        assert_eq!(low_r2_of(&[value_of("stat.r_squared", f64::NAN)]), None);
        assert_eq!(low_r2_of(&[value_of("param.tau", 0.1)]), None);
        assert_eq!(
            low_r2_of(&[value_of("stat.r_squared", f64::NEG_INFINITY)]),
            Some(f64::NEG_INFINITY)
        );
    }
}
