//! Analyser des lignes (#253) : une ligne se transpose dans une feuille liée,
//! puis sa colonne s'analyse avec les outils des colonnes.
//!
//! Le geste (clic droit d'une courbe « ligne », Object Manager, Graph ›
//! Selection, menu Analysis du classeur sur des lignes entières) ouvre un
//! groupe annulable `AnalyzeRows`, crée ou complète la feuille transposée
//! (`Command::TransposeRows`), puis ouvre la fenêtre d'analyse sur ses
//! colonnes. Le groupe reste ouvert tant que la fenêtre l'est (elle est
//! modale) : Apply le clôt avec le calcul, Cancel l'avorte et la feuille n'a
//! jamais existé. L'analyse elle-même n'a aucune règle propre : le résultat
//! suit exactement le fonctionnement d'une colonne.
//!
//! Le même module porte la remise à jour d'une feuille transposée
//! (`Command::RecalculateTranspose`) : sans confirmation quand elle est
//! seulement périmée, avec confirmation quand une valeur d'une colonne créée
//! par le lien a été éditée (les valeurs éditées seraient écrasées).

use stokesforge_commands::{Command, HistoryLabel, MacroKind};
use stokesforge_core::{NodeId, SeriesRef};

use super::{history_text, AppState};
use crate::analysis::{AnalysisForm, Preset};
use crate::errors;
use crate::tr;

/// Ce que l'interface sait d'une feuille transposée liée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransposeInfo {
    /// La feuille source.
    pub source: NodeId,
    /// Son nom, tel que la feuille transposée le cite.
    pub source_name: String,
    /// La source ne vaut plus ce que le lien a lu.
    pub stale: bool,
    /// Une valeur d'une colonne créée par le lien a été éditée.
    pub modified: bool,
}

impl AppState {
    /// L'état du lien de `sheet`, si c'est une feuille transposée liée.
    pub fn transpose_info(&self, sheet: NodeId) -> Option<TransposeInfo> {
        let tree = &self.snapshot.tree;
        let link = tree.transpose_link(sheet)?;
        let state = link.state(tree);
        Some(TransposeInfo {
            source: link.source,
            source_name: tree.node(link.source)?.name().to_string(),
            stale: state.stale,
            modified: state.modified,
        })
    }

    /// La feuille dont la remise à jour attend la confirmation d'écraser des
    /// valeurs éditées.
    pub fn pending_transpose_relink(&self) -> Option<NodeId> {
        self.pending_transpose_relink
    }

    /// Ouvre l'analyse sur des lignes `(feuille, ligne)` : crée ou complète la
    /// feuille transposée de chaque feuille (un seul groupe annulable), puis
    /// ouvre la fenêtre sur les colonnes de la transposée. Un refus (colonne
    /// sans x', colonne de texte) se dit dans la barre d'état et ne laisse
    /// rien derrière lui.
    pub(super) fn open_row_analysis(&mut self, preset: &Preset, rows: &[(NodeId, usize)]) {
        self.abort_row_macro();
        if rows.is_empty() {
            self.fail(tr!("status-analysis-no-series"));
            return;
        }
        if self.handle.begin_macro(MacroKind::AnalyzeRows).is_err() {
            return;
        }
        let mut sheets: Vec<NodeId> = Vec::new();
        for (sheet, _) in rows {
            if !sheets.contains(sheet) {
                sheets.push(*sheet);
            }
        }
        for sheet in &sheets {
            let wanted: Vec<usize> = rows
                .iter()
                .filter(|(s, _)| s == sheet)
                .map(|(_, row)| *row)
                .collect();
            self.execute(Command::TransposeRows {
                source: *sheet,
                rows: wanted,
            });
            if self.last_action_failed() {
                let _ = self.handle.abort_macro();
                self.refresh();
                return;
            }
        }
        self.refresh();
        let tree = &self.snapshot.tree;
        let series: Option<Vec<SeriesRef>> = rows
            .iter()
            .map(|(sheet, row)| {
                let (_, link) = tree.transposed_from(*sheet)?;
                let column = link.rows.iter().find(|r| r.row == *row)?.column;
                Some(SeriesRef {
                    x: Some(link.x_column),
                    y: column,
                    sigma: None,
                })
            })
            .collect();
        let Some(series) = series else {
            let _ = self.handle.abort_macro();
            self.refresh();
            self.fail(tr!("status-analysis-no-series"));
            return;
        };
        self.analysis = Some(AnalysisForm::new(
            preset,
            series,
            self.analysis_keep_results,
        ));
        self.analysis_rows_macro = true;
    }

    /// Clôt le groupe `AnalyzeRows` : l'analyse vient d'être validée.
    pub(super) fn end_row_macro(&mut self) {
        if !std::mem::take(&mut self.analysis_rows_macro) {
            return;
        }
        match self.handle.end_macro() {
            Ok(_) => self.notice(history_text(&HistoryLabel::Macro(MacroKind::AnalyzeRows))),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
        self.refresh();
    }

    /// Avorte le groupe `AnalyzeRows` : la fenêtre est fermée sans analyse, la
    /// feuille transposée créée pour elle disparaît.
    pub(super) fn abort_row_macro(&mut self) {
        if !std::mem::take(&mut self.analysis_rows_macro) {
            return;
        }
        let _ = self.handle.abort_macro();
        self.refresh();
    }

    /// « Recalculate » d'une feuille transposée : tout de suite si elle est
    /// seulement périmée, après confirmation si des valeurs éditées seraient
    /// écrasées.
    pub(super) fn recalculate_transpose(&mut self, sheet: NodeId) {
        let Some(info) = self.transpose_info(sheet) else {
            self.fail(tr!("status-transpose-not-linked"));
            return;
        };
        if info.modified {
            self.pending_transpose_relink = Some(sheet);
            return;
        }
        self.execute(Command::RecalculateTranspose { sheet });
    }

    /// Réponse à la confirmation de la remise à jour.
    pub(super) fn confirm_transpose_relink(&mut self, accept: bool) {
        let Some(sheet) = self.pending_transpose_relink.take() else {
            return;
        };
        if accept {
            self.execute(Command::RecalculateTranspose { sheet });
        }
    }
}
