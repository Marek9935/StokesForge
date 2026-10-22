//! Feuilles transposées liées (#253) : analyser une **ligne** d'une feuille,
//! c'est analyser une colonne d'une feuille transposée.
//!
//! L'analyse reste entièrement en colonnes. Une feuille « `<Source>ᵀ` » porte
//! une colonne x' (les x' des colonnes de la source) puis une colonne par ligne
//! demandée (« Row 10 »). Le lien est un **instantané recalculable**, pas un
//! lien vivant : il retient ce qu'il a lu ([`TransposeSourceColumn`]) et la
//! feuille est dite *périmée* quand la source ne le vaut plus, *modifiée*
//! quand une valeur d'une colonne créée par le lien a été éditée. Aucun des
//! deux états n'est stocké : ils se dérivent de l'arbre (comme le péremption
//! d'un calcul), et ne passent pas par le recalcul ciblé des formules.
//!
//! Le lien est porté par l'arbre ([`Tree::transposes`]), comme les calculs :
//! il voyage avec les instantanés d'historique et le fichier.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::column::{Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, MixedCell};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::node::NodeKind;
use crate::project::{spreadsheet_name, Project};
use crate::tree::Tree;

/// Le suffixe du nom d'une feuille transposée.
pub const TRANSPOSE_MARK: char = '\u{1D40}';

/// Nom de la colonne des x' dans la feuille transposée.
pub const TRANSPOSE_X_NAME: &str = "x'";

/// Ce qu'une colonne de la source avait au moment où le lien l'a lue : son
/// identifiant, sa version (type et valeurs) et son x' (en bits, `NaN` si
/// absent). Une version ne couvre pas les métadonnées : le x' y est donc
/// retenu à part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransposeSourceColumn {
    /// La colonne de la source.
    pub column: NodeId,
    /// Sa version lue.
    pub version: u64,
    /// Son x' lu, en bits de `f64` (`NaN` : aucun).
    pub x_prime_bits: u64,
}

/// Une colonne « ligne » de la feuille transposée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransposedRow {
    /// La ligne de la source, 0 = la première.
    pub row: usize,
    /// La colonne de la feuille transposée qui la porte.
    pub column: NodeId,
    /// L'empreinte de ses valeurs à la dernière écriture par le lien.
    pub stamp: u64,
}

/// Le lien d'une feuille transposée avec sa source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransposeLink {
    /// La feuille source.
    pub source: NodeId,
    /// La colonne des x' dans la feuille transposée.
    pub x_column: NodeId,
    /// L'empreinte de ses valeurs à la dernière écriture par le lien.
    pub x_stamp: u64,
    /// Les lignes demandées, dans l'ordre où elles l'ont été.
    pub rows: Vec<TransposedRow>,
    /// Les colonnes de la source telles que le lien les a lues, dans l'ordre
    /// de la feuille.
    pub inputs: Vec<TransposeSourceColumn>,
}

/// L'état dérivé d'une feuille transposée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransposeState {
    /// Ce que la source contient ne vaut plus ce que le lien a lu.
    pub stale: bool,
    /// Une valeur d'une colonne créée par le lien a été éditée.
    pub modified: bool,
}

/// Ce que le lien d'une feuille transposée contient, pour l'écrire dans un
/// fichier (la persistance ne touche pas aux champs du lien un par un).
impl TransposeLink {
    /// L'état du lien dans `tree`.
    pub fn state(&self, tree: &Tree) -> TransposeState {
        let current = read_inputs(tree, self.source);
        let stale = current != self.inputs;
        let changed = |column: NodeId, stamp: u64| {
            tree.node(column)
                .and_then(|n| n.as_column())
                .is_some_and(|c| content_stamp(c) != stamp)
        };
        let modified = changed(self.x_column, self.x_stamp)
            || self.rows.iter().any(|r| changed(r.column, r.stamp));
        TransposeState { stale, modified }
    }
}

/// L'empreinte du **contenu numérique** d'une colonne (FNV-1a 64 bits, stable
/// d'une version de Rust à l'autre : elle est écrite dans le fichier). Un
/// changement de type qui garde les mêmes nombres ne la change pas ; une
/// cellule vide vaut `NaN`, un texte se hache avec une marque.
pub fn content_stamp(column: &Column) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    fn feed(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(PRIME);
        }
    }
    let canonical = |v: f64| {
        if v.is_nan() {
            f64::NAN.to_bits()
        } else {
            v.to_bits()
        }
    };
    let mut hash = OFFSET;
    match column.values() {
        ColumnValues::Float(values) => {
            for v in values.iter() {
                feed(&mut hash, &canonical(*v).to_le_bytes());
            }
        }
        ColumnValues::Mixed(cells) => {
            for cell in cells.iter() {
                match cell {
                    MixedCell::Empty => feed(&mut hash, &f64::NAN.to_bits().to_le_bytes()),
                    MixedCell::Number(v) => feed(&mut hash, &canonical(*v).to_le_bytes()),
                    MixedCell::Text(text) => {
                        feed(&mut hash, &[0xFF]);
                        feed(&mut hash, text.as_bytes());
                    }
                }
            }
        }
    }
    hash
}

/// Les colonnes de `source` telles qu'un lien les lirait maintenant.
fn read_inputs(tree: &Tree, source: NodeId) -> Vec<TransposeSourceColumn> {
    let Ok(children) = tree.children(source) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|id| {
            let column = tree.node(*id)?.as_column()?;
            // Seules les colonnes Y se transposent (#253) : un X, une barre
            // d'erreur ou une étiquette n'est pas une courbe de la ligne.
            if column.meta().role != ColumnRole::Y {
                return None;
            }
            Some(TransposeSourceColumn {
                column: *id,
                version: column.version(),
                x_prime_bits: column
                    .meta()
                    .x_prime
                    .map_or(f64::NAN.to_bits(), |v| v.to_bits()),
            })
        })
        .collect()
}

fn is_numeric(column: &Column) -> bool {
    match column.values() {
        ColumnValues::Float(_) => true,
        ColumnValues::Mixed(cells) => !cells.iter().any(|c| matches!(c, MixedCell::Text(_))),
    }
}

/// Le nom d'une feuille transposée de la feuille nommée `source_name`.
pub fn transposed_name(source_name: &str) -> String {
    format!("{source_name}{TRANSPOSE_MARK}")
}

impl Project {
    /// Crée ou complète la feuille transposée de `source` avec les lignes
    /// `rows` (0 = la première) et rend cette feuille. Atomique.
    ///
    /// - La feuille se nomme « `<Source>ᵀ` » ; si ce nom est pris par une
    ///   feuille qui n'est pas liée à `source`, « `<Source>ᵀ 2` », etc. Seule
    ///   une feuille déjà liée à `source` est réutilisée.
    /// - Une ligne déjà présente réutilise sa colonne ; sinon une colonne
    ///   « Row N » est ajoutée en fin de feuille.
    /// - Seules les colonnes de rôle Y se transposent (les autres sont sautées).
    /// - Chaque colonne Y de `source` doit être numérique et porter un x' : sinon
    ///   le refus nomme la colonne (`CoreError::TransposeNotNumeric`,
    ///   `TransposeNoXPrime`). Une colonne plus courte que la ligne donne
    ///   `NaN`.
    /// - Si la feuille est périmée mais pas modifiée, ses colonnes liées sont
    ///   d'abord remises à jour. Modifiée, elle n'est pas écrasée : les
    ///   colonnes neuves sont lues dans l'état actuel de la source, et le lien
    ///   garde sa lecture (elle reste périmée).
    pub fn transpose_rows(&mut self, source: NodeId, rows: &[usize]) -> CoreResult<NodeId> {
        let before = self.tree().clone();
        let result = self.transpose_rows_inner(source, rows);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn transpose_rows_inner(&mut self, source: NodeId, rows: &[usize]) -> CoreResult<NodeId> {
        if rows.is_empty() {
            return Err(CoreError::Structure("no row to transpose".to_string()));
        }
        let inputs = self.checked_transpose_inputs(source)?;
        let existing = self
            .tree()
            .transposed_from(source)
            .map(|(sheet, link)| (sheet, link.clone()));
        let (sheet, mut link, fresh_sheet) = match existing {
            Some((sheet, link)) => (sheet, link, false),
            None => {
                let sheet = self.create_transposed_sheet(source)?;
                let link = TransposeLink {
                    source,
                    x_column: sheet,
                    x_stamp: 0,
                    rows: Vec::new(),
                    inputs: Vec::new(),
                };
                (sheet, link, true)
            }
        };
        let state = if fresh_sheet {
            TransposeState::default()
        } else {
            link.state(self.tree())
        };
        // Une feuille périmée et intacte se remet d'abord à jour : elle serait
        // sinon incohérente (colonnes neuves plus longues que les anciennes).
        let refresh = fresh_sheet || (state.stale && !state.modified);
        let mut seen = Vec::new();
        for row in rows {
            if !seen.contains(row) {
                seen.push(*row);
            }
        }
        for row in &seen {
            if !link.rows.iter().any(|r| {
                r.row == *row
                    && self
                        .tree()
                        .node(r.column)
                        .is_some_and(|n| n.as_column().is_some())
            }) {
                link.rows.retain(|r| r.row != *row);
                let placeholder = TransposedRow {
                    row: *row,
                    column: sheet,
                    stamp: 0,
                };
                link.rows.push(placeholder);
            }
        }
        self.write_transpose(sheet, &mut link, &inputs, refresh, &seen)?;
        if refresh {
            link.inputs = inputs;
        }
        Arc::make_mut(self.tree_arc_mut())
            .transposes_mut()
            .insert(sheet, Arc::new(link));
        Ok(sheet)
    }

    /// Remet à jour la feuille transposée `sheet` depuis sa source : réécrit
    /// les valeurs de toutes ses colonnes liées (celles que l'utilisateur a
    /// éditées perdent leurs modifications), recrée celles qui ont disparu et
    /// relit la source. Atomique.
    pub fn recalculate_transpose(&mut self, sheet: NodeId) -> CoreResult<()> {
        let before = self.tree().clone();
        let result = self.recalculate_transpose_inner(sheet);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn recalculate_transpose_inner(&mut self, sheet: NodeId) -> CoreResult<()> {
        let mut link =
            self.tree().transpose_link(sheet).cloned().ok_or_else(|| {
                CoreError::Structure(format!("{sheet} is not a transposed sheet"))
            })?;
        let inputs = self.checked_transpose_inputs(link.source)?;
        let rows: Vec<usize> = link.rows.iter().map(|r| r.row).collect();
        self.write_transpose(sheet, &mut link, &inputs, true, &rows)?;
        link.inputs = inputs;
        Arc::make_mut(self.tree_arc_mut())
            .transposes_mut()
            .insert(sheet, Arc::new(link));
        Ok(())
    }

    /// Lit les colonnes de `source` et refuse la première qui n'est pas
    /// transposable.
    fn checked_transpose_inputs(&self, source: NodeId) -> CoreResult<Vec<TransposeSourceColumn>> {
        let node = self.tree().get(source)?;
        if !node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{source} is not a worksheet but {}",
                node.kind().with_article()
            )));
        }
        if node.children().is_empty() {
            return Err(CoreError::Structure(
                "the worksheet has no column to transpose".to_string(),
            ));
        }
        for (position, id) in node.children().iter().enumerate() {
            let Some(child) = self.tree().node(*id) else {
                continue;
            };
            let Some(column) = child.as_column() else {
                continue;
            };
            if column.meta().role != ColumnRole::Y {
                continue;
            }
            let letter = spreadsheet_name(position);
            let name = child.name().to_string();
            if !is_numeric(column) {
                return Err(CoreError::TransposeNotNumeric { letter, name });
            }
            if column.meta().x_prime.is_none_or(|v| !v.is_finite()) {
                return Err(CoreError::TransposeNoXPrime { letter, name });
            }
        }
        let inputs = read_inputs(self.tree(), source);
        if inputs.is_empty() {
            return Err(CoreError::Structure(
                "the worksheet has no Y column to transpose".to_string(),
            ));
        }
        Ok(inputs)
    }

    /// Crée la feuille transposée de `source`, vide, juste après elle.
    fn create_transposed_sheet(&mut self, source: NodeId) -> CoreResult<NodeId> {
        let book = self
            .tree()
            .parent(source)?
            .ok_or_else(|| CoreError::Structure("the worksheet has no workbook".to_string()))?;
        let base = transposed_name(self.tree().get(source)?.name());
        let name = self.free_child_name(book, &base)?;
        let index = self
            .tree()
            .children(book)?
            .iter()
            .position(|c| *c == source)
            .map_or(usize::MAX, |i| i + 1);
        let index = index.min(self.tree().children(book)?.len());
        self.insert_node(
            book,
            index,
            &name,
            NodeKind::Worksheet {
                columns: Vec::new(),
            },
        )
    }

    /// `base`, ou « `base` 2 », « `base` 3 »… : le premier nom libre.
    fn free_child_name(&self, parent: NodeId, base: &str) -> CoreResult<String> {
        let taken = |name: &str| {
            self.tree().children(parent).is_ok_and(|kids| {
                kids.iter()
                    .filter_map(|k| self.tree().node(*k))
                    .any(|n| n.name() == name)
            })
        };
        let mut candidate = base.to_string();
        let mut n = 2u32;
        while taken(&candidate) {
            candidate = format!("{base} {n}");
            n += 1;
        }
        Ok(candidate)
    }

    /// Écrit les colonnes du lien. `refresh` : toutes (x' et lignes déjà là),
    /// sinon seulement celles de `fresh_rows` qui n'ont pas encore de colonne.
    /// Les colonnes absentes sont créées. Remet à jour les empreintes écrites.
    fn write_transpose(
        &mut self,
        sheet: NodeId,
        link: &mut TransposeLink,
        inputs: &[TransposeSourceColumn],
        refresh: bool,
        fresh_rows: &[usize],
    ) -> CoreResult<()> {
        let source_name = self.tree().get(link.source)?.name().to_string();
        let x_values: Vec<f64> = inputs
            .iter()
            .map(|i| f64::from_bits(i.x_prime_bits))
            .collect();
        let columns: Vec<Arc<Vec<f64>>> = inputs
            .iter()
            .map(|i| {
                self.tree()
                    .node(i.column)
                    .and_then(|n| n.as_column())
                    .and_then(Column::numeric_view)
                    .unwrap_or_default()
            })
            .collect();
        let unit = common_unit(self.tree(), inputs);

        // La colonne des x'.
        let x_exists = link.x_column != sheet
            && self
                .tree()
                .node(link.x_column)
                .is_some_and(|n| n.as_column().is_some());
        if !x_exists {
            let mut column = Column::from_f64(&x_values);
            column.set_meta(ColumnMeta {
                role: ColumnRole::X,
                long_name: format!("x' of {source_name}"),
                ..ColumnMeta::default()
            });
            let name = self.free_child_name(sheet, TRANSPOSE_X_NAME)?;
            link.x_column = self.add_column(sheet, Some(0), Some(&name), column)?;
            link.x_stamp = self.stamp_of(link.x_column)?;
        } else if refresh {
            self.overwrite(link.x_column, x_values)?;
            link.x_stamp = self.stamp_of(link.x_column)?;
        }

        // Les lignes.
        for index in 0..link.rows.len() {
            let TransposedRow { row, column, .. } = link.rows[index];
            let present = column != sheet
                && self
                    .tree()
                    .node(column)
                    .is_some_and(|n| n.as_column().is_some());
            let data: Vec<f64> = columns
                .iter()
                .map(|values| values.get(row).copied().unwrap_or(f64::NAN))
                .collect();
            if !present {
                if !fresh_rows.contains(&row) && !refresh {
                    continue;
                }
                let mut created = Column::from_f64(&data);
                created.set_meta(ColumnMeta {
                    role: ColumnRole::Y,
                    unit: unit.clone(),
                    long_name: format!("Row {} of {source_name}", row.saturating_add(1)),
                    ..ColumnMeta::default()
                });
                let name =
                    self.free_child_name(sheet, &format!("Row {}", row.saturating_add(1)))?;
                let id = self.add_column(sheet, None, Some(&name), created)?;
                link.rows[index].column = id;
                link.rows[index].stamp = self.stamp_of(id)?;
            } else if refresh {
                self.overwrite(column, data)?;
                link.rows[index].stamp = self.stamp_of(column)?;
            }
        }
        Ok(())
    }

    /// Remplace type et valeurs de `column` par ces nombres (version neuve).
    fn overwrite(&mut self, column: NodeId, data: Vec<f64>) -> CoreResult<()> {
        self.restore_column(
            column,
            ColumnType::Numeric,
            ColumnValues::Float(Arc::new(data)),
        )
    }

    fn stamp_of(&self, column: NodeId) -> CoreResult<u64> {
        self.tree()
            .node(column)
            .and_then(|n| n.as_column())
            .map(content_stamp)
            .ok_or(CoreError::UnknownNode(column))
    }

    /// Fait suivre le nom des feuilles transposées d'une feuille renommée :
    /// « Nouveauᵀ » pour la feuille qui portait « Ancienᵀ » (ou « Ancienᵀ 2 »).
    /// Une feuille que l'utilisateur a renommée à sa façon garde son nom. Un
    /// nom qui n'est pas libre ou pas valide laisse la feuille telle quelle.
    pub(crate) fn follow_source_rename(&mut self, source: NodeId, old_name: &str) {
        let Some((sheet, _)) = self.tree().transposed_from(source) else {
            return;
        };
        let Some(current) = self.tree().node(sheet).map(|n| n.name().to_string()) else {
            return;
        };
        let old_base = transposed_name(old_name);
        let Some(suffix) = current.strip_prefix(&old_base) else {
            return;
        };
        let Some(new_name) = self.tree().node(source).map(|n| n.name().to_string()) else {
            return;
        };
        let wanted = format!("{}{suffix}", transposed_name(&new_name));
        let Ok(Some(book)) = self.tree().parent(sheet) else {
            return;
        };
        let free = self.tree().children(book).is_ok_and(|kids| {
            !kids
                .iter()
                .filter(|k| **k != sheet)
                .filter_map(|k| self.tree().node(*k))
                .any(|n| n.name() == wanted)
        });
        if free {
            let _ = self.rename_without_follow(sheet, &wanted);
        }
    }

    /// Retire les liens dont la source ou la feuille transposée n'existe plus :
    /// la feuille qui reste devient ordinaire, rien n'est perdu.
    pub(crate) fn prune_transposes(&mut self) {
        let dead: Vec<NodeId> = self
            .tree()
            .transposes()
            .iter()
            .filter(|(sheet, link)| {
                self.tree().node(**sheet).is_none() || self.tree().node(link.source).is_none()
            })
            .map(|(sheet, _)| *sheet)
            .collect();
        if dead.is_empty() {
            return;
        }
        let tree = Arc::make_mut(self.tree_arc_mut());
        for sheet in dead {
            tree.transposes_mut().remove(&sheet);
        }
    }
}

/// L'unité commune des colonnes de la source, vide si elles diffèrent.
fn common_unit(tree: &Tree, inputs: &[TransposeSourceColumn]) -> String {
    let mut units = inputs.iter().filter_map(|i| {
        tree.node(i.column)?
            .as_column()
            .map(|c| c.meta().unit.clone())
    });
    let Some(first) = units.next() else {
        return String::new();
    };
    if units.all(|u| u == first) {
        first
    } else {
        String::new()
    }
}

/// Les liens, tels que l'arbre les porte.
pub type TransposeLinks = BTreeMap<NodeId, Arc<TransposeLink>>;
