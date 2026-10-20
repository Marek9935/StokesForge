//! L'état de la fenêtre d'analyse (#181) : le formulaire ouvert, la
//! confirmation de suppression d'un calcul, et les gestes qui lancent,
//! modifient, recalculent ou retirent un calcul.
//!
//! Tout passe par des commandes annulables (`AddCalculation`,
//! `UpdateCalculation`, `RemoveCalculation`, invariant 1) ou, pour le
//! recalcul, par `SessionHandle::recalculate` — hors historique, comme le
//! recalcul des formules. Le formulaire lui-même est un état d'interface :
//! jamais dans l'historique tant que l'utilisateur n'a pas validé.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_commands::HistoryLabel;
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{Node, NodeId, NodeKind, SeriesRef};
use stokesforge_plot::PlotType;

use stokesforge_core::compose::{current_plot_spec, series_from_columns};

use super::{history_text, Action, AppState};
use crate::analysis::{issue_message, AnalysisForm, Preset, RecentTarget};
use crate::analysis_recent::{remember, RecentAnalysis};
use crate::errors;
use crate::r_squared;
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

/// La réponse à la confirmation d'un export qui montre des séries pas à jour
/// (#198, M9 a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleExport {
    /// Exporte la figure telle qu'elle s'affiche : séries grisées et marqueur.
    Anyway,
    /// Exporte la figure sans l'avertissement (couleurs normales, pas de marqueur).
    WithoutWarning,
    /// Recalcule d'abord les calculs pas à jour, puis exporte.
    Recalculate,
    /// N'exporte rien.
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

    /// Le graphe désigne ses propres X/Y : aucun appariement de feuille ici.
    pub(super) fn open_graph_analysis(
        &mut self,
        preset: &Preset,
        graph: NodeId,
        sources: &[stokesforge_plot::SeriesRef],
        spec: stokesforge_plot::PlotSpec,
    ) {
        let series: Option<Vec<SeriesRef>> = sources
            .iter()
            .map(|source| {
                let curve = spec.panels.get(source.panel)?.series.get(source.index)?;
                // Une série de ligne a une autre sémantique que les colonnes ;
                // elle ne doit jamais analyser silencieusement toute sa colonne.
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
        let mut form = AnalysisForm::new(preset, series, self.analysis_keep_results);
        form.graph = Some(graph);
        form.add_to_graph = true;
        form.graph_source = Some((spec, sources.to_vec()));
        self.analysis = Some(form);
    }

    /// Les dernières analyses lancées, la plus récente en tête (#249).
    pub fn analysis_recent(&self) -> &[RecentAnalysis] {
        &self.analysis_recent
    }

    /// Rouvre une des dernières analyses (#249) sur la sélection `target`,
    /// avec ses réglages. Avec `run` (Ctrl+clic), la lance tout de suite quand
    /// rien ne manque — points, zones et colonne de référence ne sont pas
    /// retenus : alors la fenêtre reste ouverte, réglages en place, pour les
    /// poser. « Replace original data » n'est repris que si la sélection le
    /// permet (colonnes ordinaires).
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

    /// Retient l'analyse que le formulaire vient de lancer (#249). Un fichier
    /// de préférences illisible ne doit pas défaire un calcul réussi.
    fn remember_analysis(&mut self, form: &AnalysisForm) {
        let Some(recent) = RecentAnalysis::from_form(form) else {
            return;
        };
        remember(&mut self.analysis_recent, recent);
        let _ = self
            .object_manager_preferences
            .save_analysis_recent(&self.analysis_recent);
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
        // L'échec va dans le pied de la fenêtre (S-M7, #228) : elle reste
        // ouverte.
        if !self.last_action_failed() {
            self.analysis_keep_results = form.keep_results;
            self.remember_analysis(&form);
            // Le dernier choix devient le défaut des prochaines sessions (#200).
            // Un fichier de préférences illisible ne doit pas défaire un calcul réussi.
            let _ = self
                .object_manager_preferences
                .save_analysis_keep_results(form.keep_results);
            self.analysis = None;
            // Un calcul « Show » ne garde pas ses valeurs : le recalcul ne les
            // verra jamais, c'est donc ici qu'un R² très faible se dit (#288).
            // Un calcul qui garde ses valeurs est vu par `watch_low_r2`.
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

    /// Ouvre la boîte « R² très faible » (#288) pour `found` (par calcul, son
    /// type et son nombre de séries en défaut), sauf si la préférence de
    /// l'application la coupe, s'il n'y a rien à dire, ou si une alerte attend
    /// déjà d'être lue (une seule case : une alerte d'enregistrement ou
    /// d'ouverture ne s'écrase pas). Le texte est un compteur par type (#290).
    /// Ce qui n'a pas pu s'ouvrir n'est pas perdu : la pastille de la barre
    /// d'état (`low_r2_groups`) le dit tant que les R² restent faibles.
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

    /// Recalcule les R² faibles du projet (#290) à partir de l'instantané
    /// courant : un **état**, pas un évènement. Appelée à chaque instantané
    /// neuf, avant `watch_low_r2` ; le groupe disparaît dès que les R² remontent.
    pub(super) fn refresh_low_r2_groups(&mut self) {
        self.low_r2_groups = r_squared::project_groups(&self.snapshot.tree);
        if self.low_r2_groups.is_empty() {
            self.low_r2_list_open = false;
        }
    }

    /// Les séries à R² < 0,6 de tout le projet, par type de calcul, dans
    /// l'ordre fixe des types (#290). Compte aussi les calculs dont la boîte
    /// est coupée ; pas les calculs « Show », qui ne gardent pas leurs valeurs.
    pub fn low_r2_groups(&self) -> &[r_squared::LowRSquaredGroup] {
        &self.low_r2_groups
    }

    /// Le nombre de séries à R² < 0,6 du projet : la pastille de la barre
    /// d'état n'existe que s'il est positif.
    pub fn low_r2_total(&self) -> usize {
        r_squared::total(&self.low_r2_groups)
    }

    /// La liste de la pastille est-elle ouverte ?
    pub fn low_r2_list_open(&self) -> bool {
        self.low_r2_list_open
    }

    /// Le texte de la liste de la pastille : les mêmes compteurs que la boîte.
    pub fn low_r2_list_text(&self) -> String {
        r_squared::summary_text(&self.low_r2_groups)
    }

    /// Prend l'empreinte des R² très faibles des calculs de `tree`, sans rien
    /// dire : l'ouverture d'un projet n'est pas un recalcul.
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

    /// Appelée à chaque instantané neuf : un calcul qui garde ses valeurs et
    /// dont un R² très faible **vient de changer** (recalcul manuel ou
    /// automatique, application, annulation) ouvre la boîte (#288). Un calcul
    /// dont le résultat n'a pas bougé ne la rouvre pas à chaque image.
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

    /// Le résultat rejoint exactement le calque et le groupe de sa source,
    /// avec son X explicite (l'ordre de la feuille ne sert pas d'appariement).
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
            // Aucun nouveau groupe : le numéro appartient au calque source.
            panel.series.push(result);
        }
        self.execute(Command::SetPlotSpec { graph, spec });
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
    /// Les traces de correction du classeur, dans leur ordre de création.
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

impl AppState {
    /// Les calculs pas à jour dont une colonne est tracée par les séries
    /// visibles de `spec` ; vide quand le graphe a désactivé l'avertissement.
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

    /// L'export (ou la copie) en attente de confirmation, avec le nombre de
    /// calculs pas à jour qu'il montre.
    pub fn pending_stale_export(&self) -> Option<usize> {
        let spec = match self.pending_stale_export.as_ref()? {
            Action::ExportGraph { spec, .. } | Action::CopyGraphFigure { spec } => spec,
            _ => return None,
        };
        Some(self.stale_calculations_in(spec).len())
    }

    /// Exporte ou copie tout de suite si la figure est à jour ; sinon met
    /// l'action en attente et laisse la fenêtre demander (#198).
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

    /// Réponse à la confirmation d'export.
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

/// Le R² d'une série qui franchit le palier de la boîte, s'il y en a un (le plus
/// bas si plusieurs : un ajustement global n'en a qu'un par série).
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

    fn valeur(key: &str, value: f64) -> ResultValue {
        ResultValue {
            key: key.to_string(),
            index: None,
            value,
            error: None,
        }
    }

    #[test]
    fn low_r2_of_rend_le_plus_bas_sous_le_palier_de_la_boite() {
        let values = [
            valeur("stat.r_squared", 0.4),
            valeur("stat.r_squared", -2.0),
            valeur("stat.r_squared", 0.5),
            valeur("param.tau", -9.0),
        ];
        assert_eq!(low_r2_of(&values), Some(-2.0));
    }

    #[test]
    fn low_r2_of_ne_rend_rien_hors_du_palier_ou_hors_de_la_cle() {
        assert_eq!(low_r2_of(&[]), None);
        assert_eq!(low_r2_of(&[valeur("stat.r_squared", 0.6)]), None);
        assert_eq!(low_r2_of(&[valeur("stat.r_squared", f64::NAN)]), None);
        assert_eq!(low_r2_of(&[valeur("param.tau", 0.1)]), None);
        assert_eq!(
            low_r2_of(&[valeur("stat.r_squared", f64::NEG_INFINITY)]),
            Some(f64::NEG_INFINITY)
        );
    }
}
