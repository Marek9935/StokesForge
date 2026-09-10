//! Colonne du modèle de données : un vecteur de valeurs nommé (le nom vit
//! dans le nœud de l'arbre, pas ici) de longueur libre, plus ses métadonnées
//! d'affichage et de tracé.
//!
//! Deux décisions d'architecture structurent ce module : les valeurs
//! manquantes sont `f64::NAN` pour les colonnes numériques/date-heure et
//! `None` pour les colonnes texte (distinct de `Some(String::new())`) ; et
//! les buffers sont des `Vec` contigus (jamais chunkés) partagés derrière un
//! `Arc`, mutés via `Arc::make_mut` pour permettre au pont Python de tenir
//! une vue numpy sans copie tant qu'il détient un clone de l'`Arc`.

use std::sync::Arc;

use crate::error::{CoreError, CoreResult};

/// Type de données stocké. `Numeric` et `DateTime` partagent la représentation
/// `f64` (pour `DateTime`, un timestamp POSIX en secondes) ; `Text` stocke des
/// chaînes optionnelles dont la valeur manquante est `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnType {
    /// Valeurs numériques (`f64`), manquante = `NaN`.
    Numeric,
    /// Timestamps POSIX en secondes (`f64`), manquante = `NaN`.
    DateTime,
    /// Chaînes optionnelles, manquante = `None`.
    Text,
}

/// Rôle de la colonne dans un tracé, façon Origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColumnRole {
    /// Abscisse.
    X,
    /// Ordonnée (rôle par défaut d'une colonne nouvellement créée).
    #[default]
    Y,
    /// Troisième axe (surfaces, matrices affichées en colonnes).
    Z,
    /// Barre d'erreur en X.
    XError,
    /// Barre d'erreur en Y.
    YError,
    /// Étiquette textuelle associée aux points.
    Label,
    /// Aucun rôle de tracé (colonne auxiliaire).
    None,
}

/// Métadonnées d'affichage et de tracé d'une colonne (hors nom et type).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColumnMeta {
    /// Rôle de tracé (X, Y, Z, barre d'erreur, étiquette, ...).
    pub role: ColumnRole,
    /// Unité affichée (`"ms"`, `"°C"`, ...), libre.
    pub unit: String,
    /// Nom long / description, distinct du nom court porté par le nœud.
    pub long_name: String,
    /// Commentaire libre.
    pub comment: String,
    /// Format d'affichage (chaîne libre interprétée par la couche UI).
    pub display_format: String,
}

/// Bloc de valeurs typé, tel qu'on l'écrit dans une colonne ou qu'on le
/// récupère en delta d'annulation.
#[derive(Debug, Clone)]
pub enum ValueBlock {
    /// Valeurs numériques ou date-heure.
    Float(Vec<f64>),
    /// Valeurs texte, `None` pour une cellule manquante.
    Text(Vec<Option<String>>),
}

impl PartialEq for ValueBlock {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ValueBlock::Float(a), ValueBlock::Float(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ValueBlock::Text(a), ValueBlock::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ValueBlock {}

impl ValueBlock {
    /// Nombre de valeurs du bloc.
    pub fn len(&self) -> usize {
        match self {
            ValueBlock::Float(values) => values.len(),
            ValueBlock::Text(values) => values.len(),
        }
    }

    /// Vrai si le bloc ne contient aucune valeur.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bloc vide du type de colonne donné.
    pub fn empty_for(column_type: ColumnType) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(Vec::new()),
            ColumnType::Text => ValueBlock::Text(Vec::new()),
        }
    }

    /// `count` valeurs manquantes du type donné.
    pub fn missing_for(column_type: ColumnType, count: usize) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(vec![f64::NAN; count]),
            ColumnType::Text => ValueBlock::Text(vec![None; count]),
        }
    }
}

/// Une cellule lue.
#[derive(Debug, Clone)]
pub enum Cell {
    /// Valeur numérique ou date-heure.
    Float(f64),
    /// Valeur texte ; `None` = manquante.
    Text(Option<String>),
}

impl PartialEq for Cell {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Cell::Float(a), Cell::Float(b)) => a.to_bits() == b.to_bits(),
            (Cell::Text(a), Cell::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Cell {}

impl Cell {
    /// Vrai si la valeur est manquante : `NaN` pour `Float`, `None` **ou `Some("")`** pour
    /// `Text` (une chaîne vide est stockée distinctement mais compte comme manquante à
    /// l'affichage — c'est le comportement de la version Python).
    pub fn is_missing(&self) -> bool {
        match self {
            Cell::Float(value) => value.is_nan(),
            Cell::Text(None) => true,
            Cell::Text(Some(text)) => text.is_empty(),
        }
    }
}

/// Buffer sous-jacent, partagé par `Arc`. C'est cet `Arc` que le pont Python
/// clonera pour tenir une vue numpy vivante.
#[derive(Debug, Clone)]
pub enum ColumnValues {
    /// Buffer numérique ou date-heure partagé.
    Float(Arc<Vec<f64>>),
    /// Buffer texte partagé.
    Text(Arc<Vec<Option<String>>>),
}

impl PartialEq for ColumnValues {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ColumnValues::Float(a), ColumnValues::Float(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ColumnValues::Text(a), ColumnValues::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ColumnValues {}

impl ColumnValues {
    /// Longueur du buffer.
    pub fn len(&self) -> usize {
        match self {
            ColumnValues::Float(values) => values.len(),
            ColumnValues::Text(values) => values.len(),
        }
    }

    /// Vrai si le buffer est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cellule à la ligne `row` ; valeur manquante au-delà de la longueur (jamais une erreur).
    pub fn get(&self, row: usize) -> Cell {
        match self {
            ColumnValues::Float(values) => {
                Cell::Float(values.get(row).copied().unwrap_or(f64::NAN))
            }
            ColumnValues::Text(values) => Cell::Text(values.get(row).cloned().unwrap_or(None)),
        }
    }
}

/// Ce qu'il faut pour annuler un `set_values` sans copier la colonne : les
/// valeurs écrasées et la longueur précédente. Ne contient pas de `f64` en
/// propre (ils sont dans `previous: ValueBlock`) : dériver `PartialEq`/`Eq`
/// reste correct puisqu'il délègue à l'égalité bit à bit de `ValueBlock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuesDelta {
    /// Ligne de départ de l'écriture qui a produit ce delta.
    pub start: usize,
    /// Valeurs écrasées par l'écriture (partie recouvrant des lignes déjà présentes).
    pub previous: ValueBlock,
    /// Longueur de la colonne avant l'écriture.
    pub previous_length: usize,
}

/// Vecteur de données de longueur libre plus ses métadonnées. Le nom vit
/// dans le nœud de l'arbre.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    column_type: ColumnType,
    values: ColumnValues,
    meta: ColumnMeta,
}

/// Écrit `new_values` dans `vec` à partir de `start`, en étendant `vec` avec
/// des valeurs manquantes (`missing`) si besoin. Rend les valeurs écrasées
/// (uniquement la portion de `new_values` qui recouvre des lignes déjà
/// présentes) ainsi que la longueur précédente de `vec`.
fn write_block<T: Clone>(
    vec: &mut Vec<T>,
    start: usize,
    new_values: Vec<T>,
    missing: impl Fn() -> T,
) -> (Vec<T>, usize) {
    let previous_length = vec.len();
    let overlap_start = start.min(previous_length);
    let overlap_end = previous_length.min(start.saturating_add(new_values.len()));
    let previous = vec
        .get(overlap_start..overlap_end)
        .map(|slice| slice.to_vec())
        .unwrap_or_default();

    if vec.len() < start {
        vec.resize_with(start, missing);
    }

    let mut incoming = new_values.into_iter();
    if let Some(tail) = vec.get_mut(start..) {
        for slot in tail.iter_mut() {
            match incoming.next() {
                Some(value) => *slot = value,
                None => break,
            }
        }
    }
    vec.extend(incoming);

    (previous, previous_length)
}

impl Column {
    /// Colonne vide du type donné, métadonnées par défaut.
    pub fn new(column_type: ColumnType) -> Self {
        let values = match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ColumnValues::Float(Arc::new(Vec::new())),
            ColumnType::Text => ColumnValues::Text(Arc::new(Vec::new())),
        };
        Self {
            column_type,
            values,
            meta: ColumnMeta::default(),
        }
    }

    /// Colonne numérique initialisée (copie le slice).
    pub fn from_f64(values: &[f64]) -> Self {
        Self {
            column_type: ColumnType::Numeric,
            values: ColumnValues::Float(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
        }
    }

    /// Colonne texte initialisée (copie).
    pub fn from_text(values: &[Option<String>]) -> Self {
        Self {
            column_type: ColumnType::Text,
            values: ColumnValues::Text(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
        }
    }

    /// Colonne depuis un bloc typé et des métadonnées. `Err(TypeMismatch)` si le bloc ne
    /// correspond pas au type (`Float` pour `Numeric`/`DateTime`, `Text` pour `Text`).
    pub fn from_block(
        column_type: ColumnType,
        block: ValueBlock,
        meta: ColumnMeta,
    ) -> CoreResult<Self> {
        let values = match (column_type, block) {
            (ColumnType::Numeric | ColumnType::DateTime, ValueBlock::Float(values)) => {
                ColumnValues::Float(Arc::new(values))
            }
            (ColumnType::Text, ValueBlock::Text(values)) => ColumnValues::Text(Arc::new(values)),
            _ => {
                return Err(CoreError::TypeMismatch(
                    "le bloc de valeurs ne correspond pas au type de la colonne".to_string(),
                ))
            }
        };
        Ok(Self {
            column_type,
            values,
            meta,
        })
    }

    /// Type de données de la colonne.
    pub fn column_type(&self) -> ColumnType {
        self.column_type
    }

    /// Buffer de valeurs partagé.
    pub fn values(&self) -> &ColumnValues {
        &self.values
    }

    /// Métadonnées d'affichage et de tracé.
    pub fn meta(&self) -> &ColumnMeta {
        &self.meta
    }

    /// Nombre de lignes de la colonne.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Vrai si la colonne ne contient aucune ligne.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Cellule à `row` ; manquante hors bornes, jamais d'erreur.
    pub fn get(&self, row: usize) -> Cell {
        self.values.get(row)
    }

    /// Vrai si la cellule à `row` est manquante (voir `Cell::is_missing`).
    pub fn is_missing(&self, row: usize) -> bool {
        self.get(row).is_missing()
    }

    /// Le buffer `f64` partagé, ou `None` pour une colonne texte. C'est l'`Arc` que le pont
    /// Python clone pour tenir une vue sans copie.
    pub fn float_values(&self) -> Option<&Arc<Vec<f64>>> {
        match &self.values {
            ColumnValues::Float(values) => Some(values),
            ColumnValues::Text(_) => None,
        }
    }

    /// Écrit `block` à partir de la ligne `start`, en étendant la colonne si besoin ;
    /// l'extension remplit les trous éventuels avec la valeur manquante. Rend les valeurs
    /// écrasées (uniquement la partie de `block` qui recouvre des lignes existantes) et la
    /// longueur précédente. `Err(TypeMismatch)` si la variante du bloc ne correspond pas au type.
    pub fn set_values(&mut self, start: usize, block: ValueBlock) -> CoreResult<ValuesDelta> {
        match (&mut self.values, block) {
            (ColumnValues::Float(arc), ValueBlock::Float(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) = write_block(vec, start, new_values, || f64::NAN);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                })
            }
            (ColumnValues::Text(arc), ValueBlock::Text(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) = write_block(vec, start, new_values, || None);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Text(previous),
                    previous_length,
                })
            }
            _ => Err(CoreError::TypeMismatch(format!(
                "bloc de valeurs incompatible avec le type de la colonne ({:?})",
                self.column_type
            ))),
        }
    }

    /// Tronque ou complète à `length` lignes. Le delta porte la queue retirée (vide si extension
    /// ou inchangé), `start = length`, `previous` = valeurs retirées (vide si aucune),
    /// `previous_length` = ancienne longueur.
    pub fn set_length(&mut self, length: usize) -> ValuesDelta {
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                let previous_length = vec.len();
                let previous = if length < previous_length {
                    vec.split_off(length)
                } else {
                    if length > previous_length {
                        vec.resize(length, f64::NAN);
                    }
                    Vec::new()
                };
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                }
            }
            ColumnValues::Text(arc) => {
                let vec = Arc::make_mut(arc);
                let previous_length = vec.len();
                let previous = if length < previous_length {
                    vec.split_off(length)
                } else {
                    if length > previous_length {
                        vec.resize(length, None);
                    }
                    Vec::new()
                };
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Text(previous),
                    previous_length,
                }
            }
        }
    }

    /// Insère `count` lignes manquantes à partir de `at`. `at` doit être dans `0..=len`
    /// (l'égalité insère en fin) sinon `Err(IndexOutOfBounds)`.
    pub fn insert_rows(&mut self, at: usize, count: usize) -> CoreResult<()> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "ligne {at} hors bornes (longueur {len})"
            )));
        }
        if count == 0 {
            return Ok(());
        }
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(f64::NAN, count));
            }
            ColumnValues::Text(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(None, count));
            }
        }
        Ok(())
    }

    /// Supprime `count` lignes à partir de `at`, rabattu à la fin (`at + count > len` supprime
    /// jusqu'à la fin, sans erreur). `at` doit être dans `0..=len` sinon `Err(IndexOutOfBounds)`.
    /// Rend les valeurs retirées.
    pub fn delete_rows(&mut self, at: usize, count: usize) -> CoreResult<ValueBlock> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "ligne {at} hors bornes (longueur {len})"
            )));
        }
        let stop = len.min(at.saturating_add(count));
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Float(vec.drain(at..stop).collect()))
            }
            ColumnValues::Text(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Text(vec.drain(at..stop).collect()))
            }
        }
    }

    /// Remplace les métadonnées. Rend les précédentes.
    pub fn set_meta(&mut self, meta: ColumnMeta) -> ColumnMeta {
        std::mem::replace(&mut self.meta, meta)
    }

    /// Change le type et convertit les données. Rend `(ancien type, anciennes valeurs)` —
    /// **snapshot** volontaire : la conversion est destructrice dans les deux sens.
    /// Conversions : `Numeric ↔ DateTime` sans changement de valeurs ; `Float → Text` :
    /// `NaN → None`, sinon `Some(v.to_string())` ; `Text → Float` : `None → NaN`, `Some(s)` →
    /// `s.trim().parse::<f64>()` ou `NaN` si l'analyse échoue. Même type → aucun changement,
    /// rend quand même `(type, clone des valeurs)`.
    pub fn set_column_type(&mut self, column_type: ColumnType) -> (ColumnType, ColumnValues) {
        let old_type = self.column_type;
        let old_values = self.values.clone();

        if column_type == old_type {
            return (old_type, old_values);
        }

        let new_values = match (&self.values, column_type) {
            (ColumnValues::Float(values), ColumnType::Numeric | ColumnType::DateTime) => {
                ColumnValues::Float(Arc::clone(values))
            }
            (ColumnValues::Float(values), ColumnType::Text) => {
                let converted = values
                    .iter()
                    .map(|value| {
                        if value.is_nan() {
                            None
                        } else {
                            Some(value.to_string())
                        }
                    })
                    .collect();
                ColumnValues::Text(Arc::new(converted))
            }
            (ColumnValues::Text(values), ColumnType::Numeric | ColumnType::DateTime) => {
                let converted = values
                    .iter()
                    .map(|value| match value {
                        None => f64::NAN,
                        Some(text) => text.trim().parse::<f64>().unwrap_or(f64::NAN),
                    })
                    .collect();
                ColumnValues::Float(Arc::new(converted))
            }
            (ColumnValues::Text(values), ColumnType::Text) => {
                ColumnValues::Text(Arc::clone(values))
            }
        };

        self.column_type = column_type;
        self.values = new_values;

        (old_type, old_values)
    }

    /// Remplace entièrement le type et les valeurs (utilisé pour annuler `set_column_type`).
    /// `Err(TypeMismatch)` si `values` ne correspond pas à `column_type`.
    pub fn restore(&mut self, column_type: ColumnType, values: ColumnValues) -> CoreResult<()> {
        let matches_type = matches!(
            (column_type, &values),
            (
                ColumnType::Numeric | ColumnType::DateTime,
                ColumnValues::Float(_)
            ) | (ColumnType::Text, ColumnValues::Text(_))
        );
        if !matches_type {
            return Err(CoreError::TypeMismatch(format!(
                "les valeurs ne correspondent pas au type {column_type:?}"
            )));
        }
        self.column_type = column_type;
        self.values = values;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values_of(column: &Column) -> Vec<f64> {
        column
            .float_values()
            .expect("colonne numérique")
            .as_slice()
            .to_vec()
    }

    #[test]
    fn new_numeric_column_is_empty_with_default_metadata() {
        let column = Column::new(ColumnType::Numeric);
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.len(), 0);
        assert!(column.float_values().is_some());
        assert_eq!(column.meta().role, ColumnRole::Y);
    }

    #[test]
    fn get_reports_missing_for_nan_and_present_otherwise() {
        let column = Column::from_f64(&[1.0, f64::NAN, 3.0]);
        assert!(!column.get(0).is_missing());
        assert!(column.get(1).is_missing());
        assert!(column.is_missing(1));
    }

    #[test]
    fn reading_past_the_end_yields_missing_without_growing_length() {
        let column = Column::from_f64(&[1.0, 2.0]);
        assert!(column.get(99).is_missing());
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn from_f64_copies_the_source_slice() {
        let mut source = vec![1.0, 2.0, 3.0];
        let column = Column::from_f64(&source);
        source[0] = 99.0;
        assert_eq!(column.get(0), Cell::Float(1.0));
    }

    #[test]
    fn text_columns_distinguish_empty_string_from_missing() {
        let column = Column::from_text(&[Some("a".to_string()), Some(String::new()), None]);
        assert_eq!(column.get(0), Cell::Text(Some("a".to_string())));
        assert_eq!(column.get(1), Cell::Text(Some(String::new())));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Text(None));
        assert_eq!(column.get(9), Cell::Text(None));
        assert!(column.float_values().is_none());
    }

    #[test]
    fn from_block_rejects_mismatched_block_variant() {
        let err = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Text(vec![None]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));

        let err = Column::from_block(
            ColumnType::Text,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn set_values_overwrites_and_reports_delta_for_replay() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column
            .set_values(1, ValueBlock::Float(vec![20.0, 30.0]))
            .expect("écriture valide");
        assert_eq!(values_of(&column), vec![1.0, 20.0, 30.0]);
        assert_eq!(delta.start, 1);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);

        column
            .set_values(delta.start, delta.previous.clone())
            .expect("rejeu valide");
        column.set_length(delta.previous_length);
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_extends_and_fills_gap_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column
            .set_values(3, ValueBlock::Float(vec![4.0]))
            .expect("écriture valide");
        assert_eq!(column.len(), 4);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(4.0));
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(delta.previous_length, 1);
    }

    #[test]
    fn set_values_rejects_mismatched_block_and_leaves_column_untouched() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let err = column.set_values(0, ValueBlock::Text(vec![None]));
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_copies_on_write_when_a_reader_holds_the_arc() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);

        // Simule une vue numpy vivante côté pont Python : refcount >= 2.
        let held = column.float_values().cloned().expect("colonne numérique");
        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("écriture valide");

        assert!(!Arc::ptr_eq(
            &held,
            column.float_values().expect("colonne numérique")
        ));
        assert_eq!(held.as_slice(), &[1.0, 2.0, 3.0]);
        assert_eq!(values_of(&column), vec![9.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_keeps_the_same_arc_without_a_concurrent_reader() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let before_ptr = Arc::as_ptr(column.float_values().expect("colonne numérique"));

        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("écriture valide");

        let after_ptr = Arc::as_ptr(column.float_values().expect("colonne numérique"));
        assert!(std::ptr::eq(before_ptr, after_ptr));
    }

    #[test]
    fn set_length_truncates_and_reports_removed_tail() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column.set_length(1);
        assert_eq!(values_of(&column), vec![1.0]);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);
    }

    #[test]
    fn set_length_extends_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column.set_length(3);
        assert_eq!(column.len(), 3);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
    }

    #[test]
    fn set_length_to_same_length_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let delta = column.set_length(2);
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn insert_then_delete_rows_round_trips() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        column.insert_rows(1, 2).expect("insertion valide");
        assert_eq!(column.len(), 5);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(2.0));

        let removed = column.delete_rows(1, 2).expect("suppression valide");
        match removed {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Text(_) => panic!("bloc attendu de type Float"),
        }
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn delete_rows_clamps_to_end_of_column() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(1, 99).expect("suppression valide");
        assert_eq!(removed, ValueBlock::Float(vec![2.0]));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn insert_rows_rejects_out_of_bounds_but_accepts_end_position() {
        let mut column = Column::from_f64(&[1.0]);
        assert!(matches!(
            column.insert_rows(5, 1),
            Err(CoreError::IndexOutOfBounds(_))
        ));
        column
            .insert_rows(1, 1)
            .expect("insertion en fin de colonne valide");
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn delete_rows_of_zero_count_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(0, 0).expect("suppression valide");
        assert_eq!(removed, ValueBlock::Float(Vec::new()));
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
    }

    #[test]
    fn text_column_insert_and_delete_rows() {
        let mut column = Column::from_text(&[Some("a".to_string()), Some("b".to_string())]);
        column.insert_rows(1, 1).expect("insertion valide");
        assert_eq!(column.get(1), Cell::Text(None));

        let removed = column.delete_rows(0, 1).expect("suppression valide");
        assert_eq!(removed, ValueBlock::Text(vec![Some("a".to_string())]));
    }

    #[test]
    fn set_meta_replaces_and_returns_the_previous_metadata() {
        let mut column = Column::new(ColumnType::Numeric);
        let new_meta = ColumnMeta {
            role: ColumnRole::X,
            unit: "ms".to_string(),
            ..ColumnMeta::default()
        };
        let previous = column.set_meta(new_meta.clone());
        assert_eq!(previous.unit, "");
        assert_eq!(previous.role, ColumnRole::Y);
        assert_eq!(column.meta(), &new_meta);
    }

    #[test]
    fn set_column_type_text_to_numeric_parses_values() {
        let mut column = Column::from_text(&[
            Some("1.5".to_string()),
            Some("abc".to_string()),
            Some("3".to_string()),
        ]);
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_eq!(old_type, ColumnType::Text);
        assert_eq!(
            old_values,
            ColumnValues::Text(Arc::new(vec![
                Some("1.5".to_string()),
                Some("abc".to_string()),
                Some("3".to_string()),
            ]))
        );
        assert_eq!(column.get(0), Cell::Float(1.5));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Float(3.0));
    }

    #[test]
    fn set_column_type_numeric_to_text_formats_values() {
        let mut column = Column::from_f64(&[1.0, f64::NAN]);
        column.set_column_type(ColumnType::Text);
        assert_eq!(column.get(0), Cell::Text(Some("1".to_string())));
        assert_eq!(column.get(1), Cell::Text(None));
    }

    #[test]
    fn set_column_type_to_same_type_is_a_no_op_snapshot() {
        let mut column = Column::from_f64(&[1.0]);
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_eq!(old_type, ColumnType::Numeric);
        assert_eq!(old_values, ColumnValues::Float(Arc::new(vec![1.0])));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn set_column_type_numeric_to_datetime_keeps_values() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        column.set_column_type(ColumnType::DateTime);
        assert_eq!(column.column_type(), ColumnType::DateTime);
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
        assert!(column.float_values().is_some());
    }

    #[test]
    fn restore_undoes_set_column_type_exactly() {
        let original = Column::from_f64(&[1.0, 2.0]);
        let mut column = original.clone();
        let (old_type, old_values) = column.set_column_type(ColumnType::Text);
        column
            .restore(old_type, old_values)
            .expect("restauration valide");
        assert_eq!(column, original);

        let err = column.restore(
            ColumnType::Numeric,
            ColumnValues::Text(Arc::new(vec![None])),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn float_equality_is_bit_for_bit_not_ieee() {
        // Deux colonnes identiques, `NaN` compris, sont égales.
        assert_eq!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, f64::NAN])
        );
        assert_ne!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, 2.0])
        );

        assert_eq!(Cell::Float(f64::NAN), Cell::Float(f64::NAN));
        assert_eq!(
            ValueBlock::Float(vec![f64::NAN]),
            ValueBlock::Float(vec![f64::NAN])
        );
        assert_eq!(
            ColumnValues::Float(Arc::new(vec![f64::NAN])),
            ColumnValues::Float(Arc::new(vec![f64::NAN]))
        );

        // Motif binaire distinct : `0.0` et `-0.0` restent différents.
        assert_ne!(Cell::Float(0.0), Cell::Float(-0.0));

        // Les types texte gardent une comparaison normale, non affectée.
        assert_eq!(
            Cell::Text(Some("a".to_string())),
            Cell::Text(Some("a".to_string()))
        );
        assert_ne!(Cell::Text(None), Cell::Text(Some(String::new())));
    }

    #[test]
    fn value_block_helpers_build_missing_and_empty_blocks() {
        assert_eq!(
            ValueBlock::missing_for(ColumnType::Text, 2),
            ValueBlock::Text(vec![None, None])
        );
        match ValueBlock::missing_for(ColumnType::Numeric, 2) {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Text(_) => panic!("bloc attendu de type Float"),
        }
        assert!(ValueBlock::empty_for(ColumnType::Numeric).is_empty());
        assert!(ValueBlock::empty_for(ColumnType::Text).is_empty());
    }
}
