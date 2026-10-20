//! Les **calculs d'analyse** du projet (chantier #181) : le lien entre des
//! colonnes calculées, les colonnes dont elles sortent et l'opération qui les a
//! produites.
//!
//! # Où ça vit, et pourquoi
//!
//! Un [`Calculation`] n'est pas un nœud de l'arbre — il n'a ni nom, ni parent,
//! ni fenêtre — mais un enregistrement porté par [`Tree`] à côté des couleurs
//! enregistrées (`Tree::saved_colors`) : il voyage donc avec l'instantané de
//! session, le retour arrière et le fichier `.stf` sans chemin à part. Son
//! identité est un [`NodeId`] du même allocateur que les nœuds (jamais
//! réutilisé, invariant 7), mais ne désigne aucun nœud de la carte.
//!
//! La trace de l'opération d'une colonne calculée (« cette colonne vient de tel
//! calcul ») n'est **pas** stockée sur la colonne : elle se lit dans
//! `Calculation::outputs` ([`Tree::calculation_of_column`]). Une source de
//! vérité unique évite qu'une copie de colonne ou un déplacement laisse
//! deux traces en désaccord.
//!
//! # Liens pendants
//!
//! Supprimer une colonne n'édite pas les calculs qui la citent : la référence
//! devient pendante, et c'est la lecture qui la traite. Une **entrée**
//! supprimée rend le calcul périmé ([`Staleness::InputMissing`]) ; une
//! **sortie** supprimée reste dans `outputs` avec `column` pointant un nœud
//! absent, et se relit « sortie supprimée » (M8 C). Une seule exception, pour
//! que rien ne traîne : `Project::remove` retire le calcul dont **toutes** les
//! colonnes Y d'entrée ont disparu (annuler la suppression le ramène avec
//! l'arbre).
//!
//! # Ce qui est écrit hors historique
//!
//! Le recalcul (`stokesforge-session::recalc`) réécrit les colonnes de sortie,
//! les résultats et l'état périmé par [`Project::refresh_calculation`] et
//! [`Project::set_calculation_stale`], comme il réécrit les colonnes-formule :
//! jamais une entrée d'historique. La création, la modification des
//! paramètres et la suppression d'un calcul sont, elles, des commandes
//! annulables (`Command::AddCalculation`, `UpdateCalculation`,
//! `RemoveCalculation`).

use std::sync::Arc;

use stokesforge_analysis::ops::{
    self, AuxInput, Operation, OperationKind, SeriesInput, Value, Warning,
};
use stokesforge_analysis::AnalysisError;

use crate::column::{Column, ColumnMeta, ColumnType, MixedCell, ValueBlock};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::project::Project;
use crate::tree::Tree;

/// Les colonnes d'une courbe traitée : son Y, son X apparié et, pour un
/// ajustement pondéré, sa colonne d'écart-type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesRef {
    pub x: Option<NodeId>,
    pub y: NodeId,
    pub sigma: Option<NodeId>,
}

/// La colonne annexe d'un calcul : la référence d'une correction, l'IRF d'une
/// déconvolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxRef {
    pub x: Option<NodeId>,
    pub y: NodeId,
}

/// Une colonne de sortie : de quelle série elle vient, quelle sortie de
/// l'opération elle porte (`OutColumn::key` + `index`), et où elle est.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRef {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    /// La colonne écrite. Un nœud qui n'existe plus : sortie supprimée.
    pub column: Option<NodeId>,
}

/// Une valeur calculée gardée dans le projet.
///
/// Même contenu que [`stokesforge_analysis::ops::Value`], mais l'égalité traite
/// deux `NaN` comme égaux : un arbre restauré par annulation doit être
/// **identique** à celui d'avant, même quand un résultat vaut `NaN`.
#[derive(Debug, Clone)]
pub struct ResultValue {
    pub key: String,
    pub index: Option<usize>,
    pub value: f64,
    pub error: Option<f64>,
}

fn same_float(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

impl PartialEq for ResultValue {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.index == other.index
            && same_float(self.value, other.value)
            && match (self.error, other.error) {
                (Some(a), Some(b)) => same_float(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl From<&Value> for ResultValue {
    fn from(v: &Value) -> Self {
        Self {
            key: v.key.clone(),
            index: v.index,
            value: v.value,
            error: v.error,
        }
    }
}

/// Les valeurs d'une série traitée.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesResult {
    pub values: Vec<ResultValue>,
    /// Lignes écartées (X ou Y non fini).
    pub skipped: usize,
    pub warnings: Vec<Warning>,
}

/// Pourquoi les données d'un calcul ne sont pas à jour (M9 a).
#[derive(Debug, Clone, PartialEq)]
pub enum Staleness {
    /// Recalcul manuel : une source a changé et personne n'a relancé.
    Manual,
    /// Une colonne d'entrée n'existe plus.
    InputMissing,
    /// Le recalcul a échoué ; les dernières valeurs valides sont gardées.
    Failed(AnalysisError),
}

/// Un calcul d'analyse enregistré dans le projet.
#[derive(Debug, Clone, PartialEq)]
pub struct Calculation {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    pub aux: Option<AuxRef>,
    pub outputs: Vec<OutputRef>,
    /// Une entrée par série, ou vide quand on ne garde pas les résultats.
    pub results: Vec<SeriesResult>,
    /// « Keep in Results » (M5) : les valeurs sont gardées et montrées dans le
    /// volet Results. Sinon la trace ne sert qu'aux colonnes.
    pub keep_results: bool,
    /// Relancé automatiquement quand une source change (M9 b).
    pub auto_recalc: bool,
    /// Ouvre une boîte de dialogue quand R² d'un ajustement est très faible
    /// (#288) ; propre au calcul, annulable et sauvegardé. Actif par défaut.
    pub low_r2_dialog: bool,
    pub stale: Option<Staleness>,
}

impl Calculation {
    /// Les colonnes qu'il lit : séries et annexe.
    pub fn input_columns(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        for s in &self.series {
            out.extend(s.x);
            out.push(s.y);
            out.extend(s.sigma);
        }
        if let Some(a) = &self.aux {
            out.extend(a.x);
            out.push(a.y);
        }
        out
    }

    /// Empreinte **stable du contenu** des colonnes d'entrée (A1, #316) : type
    /// et valeurs de chacune, dans l'ordre de [`Calculation::input_columns`].
    /// `None` si une colonne manque ou n'est pas une colonne.
    ///
    /// Écrite dans le fichier à l'enregistrement et comparée à la lecture,
    /// sans recalcul, pour marquer périmé un calcul dont l'entrée a changé
    /// hors de StokesForge. Elle ne contient **jamais** de version de colonne
    /// (non persistante) ni d'identifiant : le même contenu sous deux
    /// `NodeId` donne la même valeur. FNV-1a écrit à la main, parce que
    /// `DefaultHasher` ne garantit aucune stabilité d'une version à l'autre.
    pub fn inputs_digest(&self, tree: &Tree) -> Option<u64> {
        let mut hash = Fnv1a::new();
        let inputs = self.input_columns();
        hash.write(&(inputs.len() as u64).to_le_bytes());
        for id in inputs {
            let column = tree.node(id)?.as_column()?;
            hash.write(&[match column.column_type() {
                ColumnType::Numeric => 0,
                ColumnType::DateTime => 1,
                ColumnType::Mixed => 2,
            }]);
            match column.values() {
                crate::column::ColumnValues::Float(values) => {
                    hash.write(&[0]);
                    hash.write(&(values.len() as u64).to_le_bytes());
                    for v in values.iter() {
                        hash.write(&v.to_bits().to_le_bytes());
                    }
                }
                crate::column::ColumnValues::Mixed(cells) => {
                    hash.write(&[1]);
                    hash.write(&(cells.len() as u64).to_le_bytes());
                    for cell in cells.iter() {
                        match cell {
                            MixedCell::Empty => hash.write(&[0]),
                            MixedCell::Number(v) => {
                                hash.write(&[1]);
                                hash.write(&v.to_bits().to_le_bytes());
                            }
                            MixedCell::Text(t) => {
                                hash.write(&[2]);
                                hash.write(&(t.len() as u64).to_le_bytes());
                                hash.write(t.as_bytes());
                            }
                        }
                    }
                }
            }
        }
        Some(hash.finish())
    }

    /// Les colonnes de sortie qui existent encore dans `tree`.
    pub fn live_outputs<'a>(
        &'a self,
        tree: &'a crate::tree::Tree,
    ) -> impl Iterator<Item = &'a OutputRef> + 'a {
        self.outputs
            .iter()
            .filter(move |o| o.column.is_some_and(|c| tree.node(c).is_some()))
    }
}

/// FNV-1a 64 bits : l'empreinte de [`Calculation::inputs_digest`] est écrite
/// dans un fichier, son algorithme ne doit donc jamais changer sous nos pieds.
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Une colonne à créer ou à réécrire pour un calcul.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDraft {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    /// Métadonnées de la colonne (nom long, commentaire, unité).
    pub meta: ColumnMeta,
    pub data: Vec<f64>,
}

/// Tout ce qu'il faut pour enregistrer un calcul : l'enregistrement et les
/// colonnes de sortie avec leurs valeurs.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculationDraft {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    pub aux: Option<AuxRef>,
    pub outputs: Vec<OutputDraft>,
    pub results: Vec<SeriesResult>,
    pub keep_results: bool,
    pub auto_recalc: bool,
    /// Voir [`Calculation::low_r2_dialog`].
    pub low_r2_dialog: bool,
}

/// Trace immuable d'une correction qui a remplacé ses données sources (#212).
/// Elle ne crée aucun lien de calcul : les colonnes restent ordinaires.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionRecord {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    /// La référence figée d'une correction par une colonne (#248) : une copie
    /// du classeur caché, jamais la colonne d'origine.
    pub aux: Option<AuxRef>,
    pub results: Vec<SeriesResult>,
    /// `(colonne remplacée, copie de son original)` : de quoi la restaurer
    /// (« Restore original data », #248). Les copies vivent dans le classeur
    /// caché des analyses.
    pub originals: Vec<(NodeId, NodeId)>,
}

/// Plafond de séries d'un même calcul (un lot de spectres).
pub const MAX_CALCULATION_SERIES: usize = 10_000;

/// Forme d'un calcul : seule source des règles structurelles, appelée à
/// l'écriture (`CalculationDraft`) comme à la lecture d'un `.stf` (#309).
/// `output_series` : le rang de série de chaque sortie ; `results` : le
/// nombre de résultats gardés.
pub fn validate_calculation_shape(
    series: usize,
    output_series: impl IntoIterator<Item = usize>,
    results: usize,
) -> CoreResult<()> {
    if series == 0 || series > MAX_CALCULATION_SERIES {
        return Err(CoreError::Structure(format!(
            "a calculation needs between 1 and {MAX_CALCULATION_SERIES} series, got {series}"
        )));
    }
    if output_series.into_iter().any(|s| s >= series) {
        return Err(CoreError::Structure(
            "a calculation output refers to a series that does not exist".to_string(),
        ));
    }
    if results != 0 && results != series {
        return Err(CoreError::Structure(
            "a calculation keeps one result per series".to_string(),
        ));
    }
    Ok(())
}

impl Calculation {
    /// Vérifie la forme (voir [`validate_calculation_shape`]).
    pub fn validate_shape(&self) -> CoreResult<()> {
        validate_calculation_shape(
            self.series.len(),
            self.outputs.iter().map(|o| o.series),
            self.results.len(),
        )
    }
}

impl CalculationDraft {
    fn validate(&self) -> CoreResult<()> {
        validate_calculation_shape(
            self.series.len(),
            self.outputs.iter().map(|o| o.series),
            self.results.len(),
        )?;
        // Une opération invalide (non-fini, plafond) ne s'enregistre pas : elle
        // serait écrite puis perdue à la relecture du `.stf` (#278).
        self.operation
            .validate()
            .map_err(|e| CoreError::Structure(e.to_string()))?;
        Ok(())
    }
}

impl Project {
    /// Remplace les Y par leur correction et garde la trace du geste, atomiquement.
    /// Le client fournit les versions de toutes les entrées lues : un résultat
    /// préparé sur un instantané dépassé ne doit jamais écraser des données neuves.
    pub fn replace_analysis_data(
        &mut self,
        draft: CalculationDraft,
        expected_versions: &[(NodeId, u64)],
    ) -> CoreResult<NodeId> {
        draft.validate()?;
        let mut inputs: Vec<NodeId> = draft
            .series
            .iter()
            .flat_map(|s| [Some(s.y), s.x, s.sigma].into_iter().flatten())
            .collect();
        if let Some(aux) = &draft.aux {
            inputs.push(aux.y);
            inputs.extend(aux.x);
        }
        if inputs
            .iter()
            .any(|id| !expected_versions.iter().any(|(read, _)| read == id))
        {
            return Err(CoreError::Structure(
                "a correction needs the version of every input".into(),
            ));
        }
        if !matches!(
            draft.operation,
            Operation::Baseline { .. } | Operation::Correct { .. }
        ) {
            return Err(CoreError::Structure(
                "only a curve correction can replace original data".into(),
            ));
        }
        for &(id, expected) in expected_versions {
            let column = self.tree().get(id)?.as_column().ok_or_else(|| {
                CoreError::Structure("a correction input must be a column".into())
            })?;
            if column.version() != expected {
                return Err(CoreError::ConcurrentModification {
                    column: id,
                    expected,
                    found: column.version(),
                });
            }
        }
        let mut writes: std::collections::BTreeMap<NodeId, ValueBlock> =
            std::collections::BTreeMap::new();
        for (rank, source) in draft.series.iter().enumerate() {
            let column = self.tree().get(source.y)?.as_column().ok_or_else(|| {
                CoreError::Structure("a correction source must be a column".into())
            })?;
            if column.meta().formula.is_some() {
                return Err(CoreError::FormulaColumnReadOnly { column: source.y });
            }
            if let Some((calculation, _)) = self.tree().calculation_of_column(source.y) {
                return Err(CoreError::CalculatedColumnReadOnly {
                    column: source.y,
                    calculation,
                });
            }
            let output = draft
                .outputs
                .iter()
                .find(|o| o.series == rank && o.key == "corrected")
                .ok_or_else(|| {
                    CoreError::Structure("a correction needs its corrected output".into())
                })?;
            if output.data.len() < column.len() {
                return Err(CoreError::Structure(
                    "a correction must preserve the source length".into(),
                ));
            }
            // Les lignes non exploitables gardent leur cellule originale. Un
            // résultat NaN sur une ligne valide (division par zéro, référence
            // hors plage) reste NaN : ce n'est pas une valeur manquante d'entrée.
            let cells: Vec<MixedCell> = (0..column.len())
                .map(|row| {
                    let x_ok = source.x.is_none_or(|x| {
                        self.tree()
                            .node(x)
                            .and_then(|n| n.as_column())
                            .is_some_and(|c| c.values().number_at(row).is_finite())
                    });
                    if !column.values().number_at(row).is_finite() || !x_ok {
                        column.cell(row)
                    } else {
                        MixedCell::Number(output.data[row])
                    }
                })
                .collect();
            let values = if column.column_type() == ColumnType::Mixed {
                ValueBlock::Mixed(cells)
            } else {
                ValueBlock::Float(
                    cells
                        .iter()
                        .enumerate()
                        .map(|(row, cell)| {
                            if !column.values().number_at(row).is_finite() {
                                column.values().number_at(row)
                            } else {
                                cell.as_number()
                            }
                        })
                        .collect(),
                )
            };
            if let Some(previous) = writes.get(&source.y) {
                if previous != &values {
                    return Err(CoreError::Structure(
                        "one source has conflicting corrections".into(),
                    ));
                }
            } else {
                writes.insert(source.y, values);
            }
        }
        let before = self.tree().clone();
        let result = (|| {
            let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
            let mut originals = Vec::with_capacity(writes.len());
            for &id in writes.keys() {
                // Une colonne déjà corrigée garde sa donnée brute d'origine
                // (#308) : la copie du premier remplacement est réutilisée, on
                // ne photographie pas l'état déjà corrigé.
                let copy = match self.first_original_of(id) {
                    Some(copy) => copy,
                    None => self.keep_original(id)?,
                };
                originals.push((id, copy));
            }
            for (id, values) in writes {
                self.set_values(id, 0, values)?;
            }
            let id = self.allocate_id()?;
            Arc::make_mut(self.tree_arc_mut())
                .correction_traces_mut()
                .insert(
                    id,
                    Arc::new(CorrectionRecord {
                        operation: draft.operation,
                        series: draft.series,
                        aux,
                        results: draft.results,
                        originals,
                    }),
                );
            self.gc_analysis_store();
            Ok(id)
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// La copie de la donnée brute de `column`, si une correction antérieure
    /// l'a déjà gardée : celle de la trace au plus petit identifiant, donc la
    /// plus ancienne. `None` pour une colonne jamais corrigée.
    fn first_original_of(&self, column: NodeId) -> Option<NodeId> {
        self.tree().correction_traces().values().find_map(|trace| {
            trace
                .originals
                .iter()
                .find(|(replaced, copy)| {
                    *replaced == column
                        && self
                            .tree()
                            .node(*copy)
                            .and_then(|n| n.as_column())
                            .is_some()
                })
                .map(|(_, copy)| *copy)
        })
    }

    /// « Restore original data » (#248) : rend à chaque colonne remplacée par
    /// la correction `trace` son type et ses valeurs **brutes** (celles d'avant
    /// la toute première correction, #308), **écrase** ce qui y a été saisi
    /// depuis, retire la trace et les copies qui ne servent plus. Une colonne
    /// restaurée sort aussi de toutes les autres traces qui la citaient (une
    /// trace qui n'a plus aucune colonne disparaît) : restaurer B après A rend
    /// la donnée brute, il n'y a plus de correction « A » à restaurer.
    /// Une colonne supprimée depuis est laissée de côté. Atomique.
    pub fn restore_original_data(&mut self, trace: NodeId) -> CoreResult<()> {
        let record = self
            .tree()
            .correction_traces()
            .get(&trace)
            .cloned()
            .ok_or(CoreError::UnknownNode(trace))?;
        let before = self.tree().clone();
        let result = (|| {
            for (column, copy) in &record.originals {
                if self
                    .tree()
                    .node(*column)
                    .and_then(|n| n.as_column())
                    .is_some()
                {
                    self.restore_from_copy(*column, *copy)?;
                }
            }
            let restored: Vec<NodeId> = record.originals.iter().map(|(c, _)| *c).collect();
            let traces = Arc::make_mut(self.tree_arc_mut()).correction_traces_mut();
            traces.remove(&trace);
            let others: Vec<NodeId> = traces.keys().copied().collect();
            for other in others {
                let Some(existing) = traces.get(&other).cloned() else {
                    continue;
                };
                if !existing
                    .originals
                    .iter()
                    .any(|(column, _)| restored.contains(column))
                {
                    continue;
                }
                let mut stripped = (*existing).clone();
                stripped
                    .originals
                    .retain(|(column, _)| !restored.contains(column));
                if stripped.originals.is_empty() {
                    traces.remove(&other);
                } else {
                    traces.insert(other, Arc::new(stripped));
                }
            }
            self.gc_analysis_store();
            Ok(())
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// La référence que le calcul lira : figée dans le classeur caché pour une
    /// correction par une colonne (#248), telle quelle pour les autres
    /// opérations.
    fn frozen_aux(
        &mut self,
        operation: &Operation,
        aux: Option<&AuxRef>,
    ) -> CoreResult<Option<AuxRef>> {
        match (operation, aux) {
            (Operation::Correct { .. }, Some(aux)) => self.freeze_reference(aux).map(Some),
            (_, aux) => Ok(aux.cloned()),
        }
    }

    /// Enregistre un calcul : crée ses colonnes de sortie **juste après la
    /// colonne Y de chaque série** (dans l'ordre des sorties), puis
    /// l'enregistrement. Atomique : un échec rend l'arbre intact.
    pub fn add_calculation(&mut self, draft: CalculationDraft) -> CoreResult<NodeId> {
        draft.validate()?;
        let before = self.tree().clone();
        let result = self.add_calculation_inner(draft);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn add_calculation_inner(&mut self, draft: CalculationDraft) -> CoreResult<NodeId> {
        let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
        let mut last_after: Vec<NodeId> = draft.series.iter().map(|s| s.y).collect();
        let mut outputs = Vec::with_capacity(draft.outputs.len());
        for out in &draft.outputs {
            let column = self.insert_output_column(&mut last_after, out)?;
            outputs.push(OutputRef {
                series: out.series,
                key: out.key.clone(),
                index: out.index,
                column: Some(column),
            });
        }
        let id = self.allocate_id()?;
        let calc = Calculation {
            operation: draft.operation,
            series: draft.series,
            aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            low_r2_dialog: draft.low_r2_dialog,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
        self.gc_analysis_store();
        Ok(id)
    }

    /// Crée la colonne de `out`, juste après `last_after[out.series]`.
    fn insert_output_column(
        &mut self,
        last_after: &mut [NodeId],
        out: &OutputDraft,
    ) -> CoreResult<NodeId> {
        let anchor = last_after[out.series];
        let sheet = self
            .tree()
            .parent(anchor)?
            .ok_or_else(|| CoreError::Structure("the source column has no sheet".to_string()))?;
        let position = self
            .tree()
            .children(sheet)?
            .iter()
            .position(|&c| c == anchor)
            .ok_or(CoreError::UnknownNode(anchor))?;
        let mut column = Column::from_f64(&out.data);
        column.set_meta(out.meta.clone());
        let id = self.add_column(sheet, Some(position + 1), None, column)?;
        last_after[out.series] = id;
        Ok(id)
    }

    /// Remplace les paramètres, les colonnes et les résultats d'un calcul
    /// existant. Une sortie déjà présente (même série, même clé, même rang) est
    /// **réécrite en place** ; une nouvelle est créée ; une sortie qui n'existe
    /// plus dans `draft` voit sa colonne supprimée.
    pub fn update_calculation(&mut self, id: NodeId, draft: CalculationDraft) -> CoreResult<()> {
        draft.validate()?;
        let current = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let before = self.tree().clone();
        let result = self.update_calculation_inner(id, &current, draft);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn update_calculation_inner(
        &mut self,
        id: NodeId,
        current: &Calculation,
        draft: CalculationDraft,
    ) -> CoreResult<()> {
        let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
        let mut last_after: Vec<NodeId> = draft.series.iter().map(|s| s.y).collect();
        let mut outputs = Vec::with_capacity(draft.outputs.len());
        let mut kept: Vec<NodeId> = Vec::new();
        for out in &draft.outputs {
            let existing = current
                .outputs
                .iter()
                .find(|o| o.series == out.series && o.key == out.key && o.index == out.index)
                .and_then(|o| o.column)
                .filter(|c| {
                    self.tree()
                        .node(*c)
                        .is_some_and(|n| n.as_column().is_some())
                });
            let column = match existing {
                Some(col) => {
                    self.write_column_data(col, &out.data)?;
                    self.set_column_meta(col, out.meta.clone())?;
                    last_after[out.series] = col;
                    col
                }
                None => self.insert_output_column(&mut last_after, out)?,
            };
            kept.push(column);
            outputs.push(OutputRef {
                series: out.series,
                key: out.key.clone(),
                index: out.index,
                column: Some(column),
            });
        }
        for old in &current.outputs {
            if let Some(col) = old.column {
                if !kept.contains(&col) && self.tree().node(col).is_some() {
                    self.remove_without_gc(col)?;
                }
            }
        }
        let calc = Calculation {
            operation: draft.operation,
            series: draft.series,
            aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            low_r2_dialog: draft.low_r2_dialog,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
        self.gc_analysis_store();
        Ok(())
    }

    /// Retire un calcul. `with_columns` supprime aussi ses colonnes de sortie ;
    /// sinon elles deviennent des colonnes ordinaires (« Detach from
    /// Calculation »).
    pub fn remove_calculation(&mut self, id: NodeId, with_columns: bool) -> CoreResult<()> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let before = self.tree().clone();
        let result = (|| {
            if with_columns {
                for out in &calc.outputs {
                    if let Some(col) = out.column {
                        if self.tree().node(col).is_some() {
                            self.remove_without_gc(col)?;
                        }
                    }
                }
            }
            Arc::make_mut(self.tree_arc_mut())
                .calculations_mut()
                .remove(&id);
            self.gc_analysis_store();
            Ok(())
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// Réécrit les valeurs d'une colonne si elles diffèrent (version neuve du
    /// projet à chaque écriture réelle ; rien si le contenu est identique, pour
    /// ne pas relancer en cascade les calculs qui la lisent).
    fn write_column_data(&mut self, column: NodeId, data: &[f64]) -> CoreResult<bool> {
        let same = self
            .tree()
            .get(column)?
            .as_column()
            .and_then(Column::numeric_view)
            .is_some_and(|cur| {
                cur.len() == data.len() && cur.iter().zip(data).all(|(a, b)| same_float(*a, *b))
            });
        if same {
            return Ok(false);
        }
        self.set_length(column, data.len())?;
        self.set_values(column, 0, ValueBlock::Float(data.to_vec()))?;
        Ok(true)
    }

    /// **Hors historique** — appelée par le recalcul de la session. Réécrit les
    /// colonnes de sortie (`data` : une entrée par sortie de `Calculation::
    /// outputs`, dans leur ordre ; `None` laisse la colonne telle quelle),
    /// remplace les résultats si le calcul les garde, et lève l'état périmé.
    /// Rend `true` si quelque chose a réellement changé (une colonne, les
    /// résultats ou l'état) : c'est ce qui dit au recalcul de la session de
    /// repasser sur les formules et les calculs qui lisent ces colonnes.
    pub fn refresh_calculation(
        &mut self,
        id: NodeId,
        data: &[Option<Vec<f64>>],
        results: Vec<SeriesResult>,
    ) -> CoreResult<bool> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let mut changed = false;
        for (out, d) in calc.outputs.iter().zip(data) {
            if let (Some(col), Some(d)) = (out.column, d) {
                if self
                    .tree()
                    .node(col)
                    .is_some_and(|n| n.as_column().is_some())
                {
                    changed |= self.write_column_data(col, d)?;
                }
            }
        }
        let mut next = (*calc).clone();
        if next.keep_results {
            next.results = results;
        }
        next.stale = None;
        if next != *calc {
            Arc::make_mut(self.tree_arc_mut())
                .calculations_mut()
                .insert(id, Arc::new(next));
            changed = true;
        }
        Ok(changed)
    }

    /// **Hors historique** — comme [`Project::refresh_calculation`], à partir
    /// du résultat de [`compute`]. Chaque colonne calculée retrouve sa sortie
    /// par (série, clé, rang) ; une sortie que le calcul ne produit plus (moins
    /// de pics détectés) est vidée en `NaN` — le recalcul ne crée ni ne
    /// supprime de colonne, cela reste le fait de `UpdateCalculation`.
    pub fn refresh_calculation_from(&mut self, id: NodeId, computed: Computed) -> CoreResult<bool> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let data: Vec<Option<Vec<f64>>> =
            calc.outputs
                .iter()
                .map(|out| {
                    let found = computed.columns.iter().find(|c| {
                        c.series == out.series && c.key == out.key && c.index == out.index
                    });
                    Some(match found {
                        Some(c) => c.data.clone(),
                        None => {
                            let len = out
                                .column
                                .and_then(|c| self.tree().node(c))
                                .and_then(|n| n.as_column())
                                .map_or(0, Column::len);
                            vec![f64::NAN; len]
                        }
                    })
                })
                .collect();
        self.refresh_calculation(id, &data, computed.results)
    }

    /// **Hors historique** — marque un calcul périmé (ou à jour avec `None`).
    pub fn set_calculation_stale(
        &mut self,
        id: NodeId,
        stale: Option<Staleness>,
    ) -> CoreResult<()> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        if calc.stale == stale {
            return Ok(());
        }
        let mut next = (*calc).clone();
        next.stale = stale;
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(next));
        Ok(())
    }

    /// Remplace un enregistrement entier (la persistance et les tests). Rend
    /// l'ancien.
    pub fn restore_calculation(
        &mut self,
        id: NodeId,
        calc: Option<Calculation>,
    ) -> Option<Arc<Calculation>> {
        let map = Arc::make_mut(self.tree_arc_mut()).calculations_mut();
        match calc {
            Some(c) => map.insert(id, Arc::new(c)),
            None => map.remove(&id),
        }
    }
}

// ── Calculer ────────────────────────────────────────────────────────────

/// Pourquoi un calcul n'a pas pu être exécuté.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ComputeError {
    /// Une colonne d'entrée n'existe plus (ou n'est plus une colonne).
    #[error("input column {0} no longer exists")]
    MissingInput(NodeId),
    /// L'opération elle-même a échoué.
    #[error(transparent)]
    Analysis(#[from] AnalysisError),
}

/// Une colonne calculée, avant d'être posée dans le classeur.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedColumn {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    pub data: Vec<f64>,
}

/// Le résultat d'une opération sur les colonnes du projet : de quoi bâtir un
/// [`CalculationDraft`] (création) ou rafraîchir un calcul (recalcul).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Computed {
    pub columns: Vec<ComputedColumn>,
    /// Une entrée par série.
    pub results: Vec<SeriesResult>,
}

/// Les nombres d'une colonne, `NaN` pour une cellule vide ou textuelle, prolongés
/// de `NaN` jusqu'à `len`.
fn numbers(tree: &Tree, column: NodeId, len: usize) -> Result<Vec<f64>, ComputeError> {
    let col = tree
        .node(column)
        .and_then(|n| n.as_column())
        .ok_or(ComputeError::MissingInput(column))?;
    let values = col.values();
    Ok((0..len).map(|r| values.number_at(r)).collect())
}

fn column_len(tree: &Tree, column: NodeId) -> Result<usize, ComputeError> {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(Column::len)
        .ok_or(ComputeError::MissingInput(column))
}

/// X, Y et, au besoin, une colonne d'incertitude, lus ligne à ligne.
type ReadColumns = (Vec<f64>, Vec<f64>, Option<Vec<f64>>);

/// Une paire (X, Y) lue dans `tree`. Sans colonne X, l'abscisse est le numéro
/// de ligne (à partir de 1). Les deux colonnes sont prolongées de `NaN` à la
/// plus longue : une ligne incomplète est écartée par l'opération.
fn read_pair(
    tree: &Tree,
    x: Option<NodeId>,
    y: NodeId,
    extra: Option<NodeId>,
) -> Result<ReadColumns, ComputeError> {
    let mut len = column_len(tree, y)?;
    if let Some(x) = x {
        len = len.max(column_len(tree, x)?);
    }
    if let Some(e) = extra {
        len = len.max(column_len(tree, e)?);
    }
    let xs = match x {
        Some(x) => numbers(tree, x, len)?,
        None => (1..=len).map(|r| r as f64).collect(),
    };
    let ys = numbers(tree, y, len)?;
    let es = extra.map(|e| numbers(tree, e, len)).transpose()?;
    Ok((xs, ys, es))
}

/// Exécute `op` sur les colonnes citées de `tree`. C'est la lecture du modèle
/// que partagent la création (application, `pyst`) et le recalcul de la
/// session ; l'opération elle-même est `stokesforge_analysis::ops::run`.
pub fn compute(
    tree: &Tree,
    op: &Operation,
    series: &[SeriesRef],
    aux: Option<&AuxRef>,
) -> Result<Computed, ComputeError> {
    let mut inputs = Vec::with_capacity(series.len());
    for s in series {
        let (x, y, sigma) = read_pair(tree, s.x, s.y, s.sigma)?;
        inputs.push(SeriesInput { x, y, sigma });
    }
    let aux_input = match aux {
        Some(a) => {
            let (x, y, _) = read_pair(tree, a.x, a.y, None)?;
            Some(AuxInput { x, y })
        }
        None => None,
    };
    let outcomes = ops::run(op, &inputs, aux_input.as_ref())?;
    let mut computed = Computed::default();
    for (rank, outcome) in outcomes.iter().enumerate() {
        for c in &outcome.columns {
            computed.columns.push(ComputedColumn {
                series: rank,
                key: c.key.to_string(),
                index: c.index,
                data: c.data.clone(),
            });
        }
        computed.results.push(SeriesResult {
            values: outcome.values.iter().map(ResultValue::from).collect(),
            skipped: outcome.skipped,
            warnings: outcome.warnings.clone(),
        });
    }
    Ok(computed)
}

/// Le nom d'une colonne de sortie d'analyse, **comme donnée** : les crates du
/// dessous ne mettent rien en phrase. Une seule règle, partagée par l'interface
/// et `pyst` (#256) : [`OutputName::of`] choisit la variante depuis la famille,
/// la clé de sortie et la source ; [`OutputName::english`] en donne la forme
/// anglaise brute (scripts, journaux) ; l'application la traduit par un `match`
/// exhaustif vers les clés `analysis-output-*` du catalogue `en`
/// (`tests/output_names.rs` garde l'accord des deux).
///
/// `name` est le nom long de la source, à défaut son nom court ; `column` est
/// toujours son nom court (« B »).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputName {
    /// Le Y corrigé d'une ligne de base : `Intensity (B-corr)`.
    BaselineCorrected { name: String, column: String },
    /// Le Y corrigé d'une correction : `Intensity (corrected)`.
    Corrected { name: String },
    /// `Baseline of B`.
    Baseline { column: String },
    /// `Cumulative integral of B`.
    Cumulative { column: String },
    /// `Normalized integral of B`.
    Normalized { column: String },
    /// `Intensity (fit)`.
    Fit { name: String },
    /// `Intensity (resid.)`.
    Residuals { name: String },
    /// `Peak 2 of B` (rang à partir de 1).
    Peak { index: usize, column: String },
    /// `Background of B`.
    Background { column: String },
    /// `Sum of peaks of B`.
    PeaksSum { column: String },
    /// `Intensity (deconvolved)`.
    Deconvolved { name: String },
    /// Repli d'une clé que cette version ne connaît pas : le nom de la source.
    Source { name: String },
}

/// Le nom court d'une colonne (« B »), à défaut le vide.
fn short_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .map(|n| n.name().to_string())
        .unwrap_or_default()
}

/// Le nom long d'une colonne, ou à défaut son nom court.
fn long_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().long_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| short_name(tree, column))
}

impl OutputName {
    /// Le nom de la sortie `key` (et `index`, pour un pic) de `kind` sur `series`.
    pub fn of(
        tree: &Tree,
        kind: OperationKind,
        series: &SeriesRef,
        key: &str,
        index: Option<usize>,
    ) -> Self {
        let column = short_name(tree, series.y);
        let name = long_name(tree, series.y);
        match (kind, key) {
            (OperationKind::Baseline, "corrected") => Self::BaselineCorrected { name, column },
            (_, "corrected") => Self::Corrected { name },
            (_, "baseline") => Self::Baseline { column },
            (_, "cumulative") => Self::Cumulative { column },
            (_, "normalized") => Self::Normalized { column },
            (_, "fit") => Self::Fit { name },
            (_, "residuals") => Self::Residuals { name },
            (_, "peak") => Self::Peak {
                index: index.unwrap_or(1),
                column,
            },
            (_, "background") => Self::Background { column },
            (_, "peaks_sum") => Self::PeaksSum { column },
            (_, "deconvolved") => Self::Deconvolved { name },
            _ => Self::Source { name },
        }
    }

    /// La forme anglaise brute, identique au catalogue `en` de l'application.
    pub fn english(&self) -> String {
        match self {
            Self::BaselineCorrected { name, column } => format!("{name} ({column}-corr)"),
            Self::Corrected { name } => format!("{name} (corrected)"),
            Self::Baseline { column } => format!("Baseline of {column}"),
            Self::Cumulative { column } => format!("Cumulative integral of {column}"),
            Self::Normalized { column } => format!("Normalized integral of {column}"),
            Self::Fit { name } => format!("{name} (fit)"),
            Self::Residuals { name } => format!("{name} (resid.)"),
            Self::Peak { index, column } => format!("Peak {index} of {column}"),
            Self::Background { column } => format!("Background of {column}"),
            Self::PeaksSum { column } => format!("Sum of peaks of {column}"),
            Self::Deconvolved { name } => format!("{name} (deconvolved)"),
            Self::Source { name } => name.clone(),
        }
    }
}

/// L'unité d'une colonne de sortie : **toujours celle de la source** pour les
/// dix clés connues, vide pour une clé inconnue.
pub fn output_unit(tree: &Tree, series: &SeriesRef, key: &str) -> String {
    match key {
        "corrected" | "baseline" | "cumulative" | "normalized" | "fit" | "residuals" | "peak"
        | "background" | "peaks_sum" | "deconvolved" => tree
            .node(series.y)
            .and_then(|n| n.as_column())
            .map(|c| c.meta().unit.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Le nom anglais brut d'une famille d'opération : le commentaire des colonnes
/// de sortie écrites par `pyst` et le `Display` des libellés d'historique. Le
/// catalogue `analysis-kind-*` de l'application en est la traduction.
pub fn family_english(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Baseline => "Baseline",
        OperationKind::Correct => "Correction",
        OperationKind::Integrate => "Integration",
        OperationKind::Fit => "Fit",
        OperationKind::Peaks => "Peak Decomposition",
        OperationKind::Reconvolution => "Reconvolution",
        OperationKind::DirectDeconvolution => "Direct Deconvolution",
    }
}

/// Les deux réglages de sortie d'une décomposition en pics qui ne sont pas dans
/// l'[`Operation`] (ni dans le fichier) : les colonnes « un pic + fond » et la
/// colonne « somme des pics ». Les cases de la fenêtre et les options de
/// `pyst.peaks` les portent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeakOutputs {
    /// Une colonne par pic et la colonne de fond.
    pub peak_columns: bool,
    /// La colonne de la somme des pics.
    pub sum_column: bool,
}

impl Default for PeakOutputs {
    /// Les valeurs par défaut de la fenêtre : tout est écrit.
    fn default() -> Self {
        Self {
            peak_columns: true,
            sum_column: true,
        }
    }
}

/// La sortie `key` de `op` est-elle à écrire ? Une seule liste pour la fenêtre
/// d'analyse et `pyst` : les colonnes d'aide (ligne de base, intégrales,
/// résidus) suivent l'option de l'opération ; celles d'une décomposition en pics
/// suivent `peaks`. Toute autre sortie est écrite.
pub fn wants_output(op: &Operation, key: &str, peaks: PeakOutputs) -> bool {
    match (op, key) {
        (Operation::Baseline { keep_baseline, .. }, "baseline") => *keep_baseline,
        (Operation::Integrate { spec }, "cumulative") => spec.cumulative,
        (Operation::Integrate { spec }, "normalized") => spec.normalize,
        (Operation::Fit { residuals, .. }, "residuals")
        | (Operation::Peaks { residuals, .. }, "residuals")
        | (Operation::Reconvolution { residuals, .. }, "residuals") => *residuals,
        (Operation::Peaks { .. }, "peak" | "background") => peaks.peak_columns,
        (Operation::Peaks { .. }, "peaks_sum") => peaks.sum_column,
        _ => true,
    }
}

#[cfg(test)]
mod output_name_tests {
    use super::*;
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};

    /// Une feuille avec la colonne « B » (nom long « Intensity », unité
    /// « counts ») et la colonne « C » (aucun nom long ni unité).
    fn fixture() -> (Project, SeriesRef, SeriesRef) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let b = project
            .add_column(sheet, None, Some("B"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        let c = project
            .add_column(sheet, None, Some("C"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        project
            .set_column_meta(
                b,
                ColumnMeta {
                    long_name: "Intensity".into(),
                    unit: "counts".into(),
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let series = |y| SeriesRef {
            x: None,
            y,
            sigma: None,
        };
        (project, series(b), series(c))
    }

    fn name(tree: &Tree, kind: OperationKind, s: &SeriesRef, key: &str) -> OutputName {
        OutputName::of(tree, kind, s, key, None)
    }

    #[test]
    fn a_baseline_correction_is_named_after_the_long_name_and_the_short_one() {
        let (project, b, _) = fixture();
        let n = name(project.tree(), OperationKind::Baseline, &b, "corrected");
        assert_eq!(
            n,
            OutputName::BaselineCorrected {
                name: "Intensity".into(),
                column: "B".into()
            }
        );
        assert_eq!(n.english(), "Intensity (B-corr)");
        // Hors famille Baseline : le corrigé d'une correction.
        let n = name(project.tree(), OperationKind::Correct, &b, "corrected");
        assert_eq!(n.english(), "Intensity (corrected)");
    }

    #[test]
    fn without_a_long_name_the_short_name_stands_in() {
        let (project, _, c) = fixture();
        let n = name(project.tree(), OperationKind::Baseline, &c, "corrected");
        assert_eq!(
            n,
            OutputName::BaselineCorrected {
                name: "C".into(),
                column: "C".into()
            }
        );
        assert_eq!(n.english(), "C (C-corr)");
    }

    #[test]
    fn peaks_are_counted_from_one_and_named_after_the_short_name() {
        let (project, b, _) = fixture();
        let tree = project.tree();
        let peak = |index| OutputName::of(tree, OperationKind::Peaks, &b, "peak", index);
        assert_eq!(peak(Some(2)).english(), "Peak 2 of B");
        assert_eq!(peak(None).english(), "Peak 1 of B");
        let sum = name(tree, OperationKind::Peaks, &b, "peaks_sum");
        assert_eq!(sum.english(), "Sum of peaks of B");
    }

    #[test]
    fn every_known_key_has_its_english_form() {
        let (project, b, _) = fixture();
        let tree = project.tree();
        let of = |kind, key| name(tree, kind, &b, key).english();
        assert_eq!(of(OperationKind::Baseline, "baseline"), "Baseline of B");
        assert_eq!(
            of(OperationKind::Integrate, "cumulative"),
            "Cumulative integral of B"
        );
        assert_eq!(
            of(OperationKind::Integrate, "normalized"),
            "Normalized integral of B"
        );
        assert_eq!(of(OperationKind::Fit, "fit"), "Intensity (fit)");
        assert_eq!(of(OperationKind::Fit, "residuals"), "Intensity (resid.)");
        assert_eq!(of(OperationKind::Peaks, "background"), "Background of B");
        assert_eq!(
            of(OperationKind::Reconvolution, "deconvolved"),
            "Intensity (deconvolved)"
        );
    }

    #[test]
    fn an_unknown_key_falls_back_on_the_source_name() {
        let (project, b, _) = fixture();
        let n = name(project.tree(), OperationKind::Fit, &b, "something_new");
        assert_eq!(
            n,
            OutputName::Source {
                name: "Intensity".into()
            }
        );
        assert_eq!(n.english(), "Intensity");
    }

    #[test]
    fn the_unit_is_the_one_of_the_source_for_the_ten_keys() {
        let (project, b, c) = fixture();
        let tree = project.tree();
        for key in [
            "corrected",
            "baseline",
            "cumulative",
            "normalized",
            "fit",
            "residuals",
            "peak",
            "background",
            "peaks_sum",
            "deconvolved",
        ] {
            assert_eq!(output_unit(tree, &b, key), "counts", "{key}");
            assert_eq!(output_unit(tree, &c, key), "", "{key} sans unité");
        }
        assert_eq!(output_unit(tree, &b, "something_new"), "");
    }

    #[test]
    fn each_family_has_its_english_title() {
        let titles: Vec<&str> = OperationKind::ALL.map(family_english).to_vec();
        assert_eq!(
            titles,
            [
                "Baseline",
                "Correction",
                "Integration",
                "Fit",
                "Peak Decomposition",
                "Reconvolution",
                "Direct Deconvolution"
            ]
        );
    }

    fn peaks_op() -> Operation {
        Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Gaussian,
                background: PeakBackground::Constant,
                seeds: Vec::new(),
                link_widths: false,
                center_window: None,
            },
            auto: None,
            range: None,
            residuals: false,
        }
    }

    #[test]
    fn the_peak_options_rule_the_peak_columns_and_the_sum() {
        let op = peaks_op();
        for peak_columns in [false, true] {
            for sum_column in [false, true] {
                let options = PeakOutputs {
                    peak_columns,
                    sum_column,
                };
                assert_eq!(wants_output(&op, "peak", options), peak_columns);
                assert_eq!(wants_output(&op, "background", options), peak_columns);
                assert_eq!(wants_output(&op, "peaks_sum", options), sum_column);
                // Ce que les options ne règlent pas reste écrit ou suit l'opération.
                assert!(wants_output(&op, "fit", options));
                assert!(!wants_output(&op, "residuals", options));
            }
        }
        assert!(PeakOutputs::default().peak_columns);
        assert!(PeakOutputs::default().sum_column);
    }

    #[test]
    fn the_helper_columns_follow_the_option_of_their_operation() {
        let options = PeakOutputs::default();
        let baseline = |keep_baseline| Operation::Baseline {
            method: BaselineMethod::Constant { point: (0.0, 0.0) },
            op: BaselineOp::Subtract,
            keep_baseline,
        };
        assert!(wants_output(&baseline(true), "baseline", options));
        assert!(!wants_output(&baseline(false), "baseline", options));
        assert!(wants_output(&baseline(false), "corrected", options));
        let integrate = |cumulative, normalize| Operation::Integrate {
            spec: IntegrateSpec {
                ranges: Vec::new(),
                under: Under::Zero,
                cumulative,
                normalize,
            },
        };
        let op = integrate(true, false);
        assert!(wants_output(&op, "cumulative", options));
        assert!(!wants_output(&op, "normalized", options));
    }
}

#[cfg(test)]
mod inputs_digest_tests {
    use super::*;
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};

    fn calc_on(y: NodeId) -> Calculation {
        Calculation {
            operation: Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: Vec::new(),
                    under: Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            },
            series: vec![SeriesRef {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: Vec::new(),
            results: Vec::new(),
            keep_results: false,
            auto_recalc: false,
            low_r2_dialog: true,
            stale: None,
        }
    }

    fn sheet_with(columns: &[Column]) -> (Project, Vec<NodeId>) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let ids = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                project
                    .add_column(sheet, None, Some(&format!("C{i}")), c.clone())
                    .unwrap()
            })
            .collect();
        (project, ids)
    }

    fn digest(columns: &[Column]) -> Option<u64> {
        let (project, ids) = sheet_with(columns);
        calc_on(ids[0]).inputs_digest(project.tree())
    }

    #[test]
    fn the_same_content_under_two_ids_gives_the_same_digest() {
        let c = Column::from_f64(&[1.0, 2.0, f64::NAN]);
        // Deux feuilles, donc deux jeux d'identifiants : seul le contenu compte.
        let (mut project, ids) = sheet_with(std::slice::from_ref(&c));
        let extra = project.tree().children(project.root()).unwrap()[0];
        let sheet = project.tree().children(extra).unwrap()[0];
        let other = project
            .add_column(sheet, None, Some("Z"), c.clone())
            .unwrap();
        let a = calc_on(ids[0]).inputs_digest(project.tree());
        let b = calc_on(other).inputs_digest(project.tree());
        assert!(a.is_some());
        assert_eq!(a, b);
    }

    #[test]
    fn a_changed_cell_or_type_changes_the_digest() {
        let base = digest(&[Column::from_f64(&[1.0, 2.0, 3.0])]);
        assert_ne!(base, digest(&[Column::from_f64(&[1.0, 2.5, 3.0])]));
        assert_ne!(base, digest(&[Column::from_f64(&[1.0, 2.0])]));
        let mixed = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Number(3.0),
        ]);
        assert_ne!(base, digest(std::slice::from_ref(&mixed)));
        let text = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Text("3".into()),
        ]);
        assert_ne!(digest(&[mixed]), digest(&[text]));
        // Le motif binaire compte : -0.0 n'est pas 0.0.
        assert_ne!(
            digest(&[Column::from_f64(&[0.0])]),
            digest(&[Column::from_f64(&[-0.0])])
        );
    }

    #[test]
    fn a_missing_column_gives_no_digest() {
        let (project, _) = sheet_with(&[Column::from_f64(&[1.0])]);
        assert_eq!(
            calc_on(NodeId::from_raw(9999)).inputs_digest(project.tree()),
            None
        );
    }
}
