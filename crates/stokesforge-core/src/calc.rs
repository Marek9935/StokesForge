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
//! absent, et se relit « sortie supprimée » (M8 C).
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

use stokesforge_analysis::ops::{self, AuxInput, Operation, SeriesInput, Value, Warning};
use stokesforge_analysis::AnalysisError;

use crate::column::{Column, ColumnMeta, ValueBlock};
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
}

/// Plafond de séries d'un même calcul (un lot de spectres).
pub const MAX_CALCULATION_SERIES: usize = 10_000;

impl CalculationDraft {
    fn validate(&self) -> CoreResult<()> {
        if self.series.is_empty() || self.series.len() > MAX_CALCULATION_SERIES {
            return Err(CoreError::Structure(format!(
                "a calculation needs between 1 and {MAX_CALCULATION_SERIES} series, got {}",
                self.series.len()
            )));
        }
        if self.outputs.iter().any(|o| o.series >= self.series.len()) {
            return Err(CoreError::Structure(
                "a calculation output refers to a series that does not exist".to_string(),
            ));
        }
        if !self.results.is_empty() && self.results.len() != self.series.len() {
            return Err(CoreError::Structure(
                "a calculation keeps one result per series".to_string(),
            ));
        }
        Ok(())
    }
}

impl Project {
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
            aux: draft.aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
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
                    self.remove(col)?;
                }
            }
        }
        let calc = Calculation {
            operation: draft.operation,
            series: draft.series,
            aux: draft.aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
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
                            self.remove(col)?;
                        }
                    }
                }
            }
            Arc::make_mut(self.tree_arc_mut())
                .calculations_mut()
                .remove(&id);
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
