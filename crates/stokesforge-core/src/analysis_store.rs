//! Le classeur caché des copies d'analyse (#248).
//!
//! Deux sortes de copies de colonnes y vivent, **un seul classeur caché par
//! projet** :
//!
//! - la **référence figée** d'une correction par une colonne de référence (avec
//!   son X) : le calcul lit la copie, pas la colonne d'origine, si bien
//!   que supprimer ou modifier celle-ci, ou son classeur, ne casse rien. Une
//!   copie est partagée par tous les spectres d'un même calcul (une seule
//!   référence par calcul) et par les calculs qui corrigent par la même colonne
//!   dans le même état ;
//! - l'**original** de chaque colonne qu'un remplacement de données a écrasée,
//!   gardé dans le `.stf` pour que « Restore original data » puisse le rendre,
//!   même après fermeture.
//!
//! # Identification et cycle de vie
//!
//! Le classeur est un classeur ordinaire de l'arbre, `hidden` et marqué par
//! [`ANALYSIS_STORE_MARK`] dans le commentaire de sa fenêtre : il voyage donc
//! avec l'arbre (instantané de session, retour arrière, `.stf`) sans schéma à
//! part, et reste hors de `books()`, du Project Manager et de l'Object Manager
//! comme les classeurs cachés qui portent les données d'un graphe copié.
//!
//! Aucune copie n'a de durée de vie propre : une colonne du classeur est gardée
//! tant qu'un calcul (`Calculation::aux`), une correction (`CorrectionRecord`
//! : sa référence ou ses originaux) la cite, et retirée avec la dernière
//! analyse qui l'utilise ([`Project::gc_analysis_store`]). Le classeur lui-même
//! disparaît avec sa dernière copie. Toutes ces écritures font partie d'une
//! commande **structurelle** : annuler ramène l'arbre entier, copies comprises.
//!
//! # Pourquoi pas une table à part dans l'arbre
//!
//! Une `BTreeMap<NodeId, Column>` de plus dans [`Tree`] aurait demandé un champ
//! de schéma `.stf`, une copie dans les instantanés et son propre parcours pour
//! la version de colonne (invariant 11 : toute donnée lue par un calcul entre
//! dans l'empreinte). Une colonne ordinaire, elle, a déjà tout cela : version,
//! persistance, recalcul ciblé.

use std::collections::HashSet;

use crate::calc::AuxRef;
use crate::column::{Column, ColumnType, ColumnValues};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::node::{NodeKind, WindowMeta};
use crate::project::Project;
use crate::tree::Tree;

/// Mêmes type et mêmes cellules (comparaison binaire : `NaN` égale `NaN`).
fn same_content(a: &Column, b: &Column) -> bool {
    a.column_type() == b.column_type()
        && a.len() == b.len()
        && (0..a.len()).all(|row| a.get(row) == b.get(row))
}

/// Le commentaire de fenêtre qui désigne le classeur caché des copies.
pub const ANALYSIS_STORE_MARK: &str = "stokesforge:analysis-store";

/// Préfixe du commentaire de colonne qui dit d'où vient une copie
/// (`copy-of:<id>`) : une même colonne, dans le même état, n'est copiée qu'une
/// fois. Le **contenu** tranche l'« état », jamais un numéro de version : il
/// dépend de l'historique des écritures de la session (le recalcul ciblé en
/// tire moins que le complet), et deux arbres que l'oracle `b22` compare
/// doivent rester égaux.
const SOURCE_MARK: &str = "copy-of:";

/// Nom du classeur caché.
const STORE_NAME: &str = "Analysis Data";

impl Tree {
    /// La feuille du classeur caché des copies, s'il existe.
    pub fn analysis_store_sheet(&self) -> Option<NodeId> {
        self.children(self.root())
            .ok()?
            .iter()
            .find_map(|id| match self.node(*id)?.kind() {
                NodeKind::Workbook { sheets, meta }
                    if meta.hidden && meta.comment == ANALYSIS_STORE_MARK =>
                {
                    sheets.first().copied()
                }
                _ => None,
            })
    }

    /// `column` est-elle une copie du classeur caché ?
    pub fn is_analysis_copy(&self, column: NodeId) -> bool {
        self.analysis_store_sheet()
            .is_some_and(|sheet| self.parent(column).ok().flatten() == Some(sheet))
    }
}

impl Project {
    /// La feuille du classeur caché des copies, s'il existe.
    pub fn analysis_store_sheet(&self) -> Option<NodeId> {
        self.tree().analysis_store_sheet()
    }

    /// `column` est-elle une copie du classeur caché ?
    pub fn is_analysis_copy(&self, column: NodeId) -> bool {
        self.tree().is_analysis_copy(column)
    }

    /// La feuille des copies, créée au besoin (classeur caché, un par projet).
    fn ensure_analysis_store(&mut self) -> CoreResult<NodeId> {
        if let Some(sheet) = self.analysis_store_sheet() {
            return Ok(sheet);
        }
        let root = self.root();
        // Un classeur de l'utilisateur peut déjà porter ce nom : le nôtre se
        // distingue par son marqueur, pas par son nom.
        let mut name = STORE_NAME.to_string();
        let mut n = 1;
        while self
            .tree()
            .children(root)?
            .iter()
            .any(|c| self.tree().node(*c).is_some_and(|node| node.name() == name))
        {
            n += 1;
            name = format!("{STORE_NAME} {n}");
        }
        let book = self.add_empty_workbook(root, Some(&name))?;
        self.set_window_meta(
            book,
            WindowMeta {
                label: String::new(),
                comment: ANALYSIS_STORE_MARK.to_string(),
                hidden: true,
            },
        )?;
        self.add_worksheet(book, Some("Copies"))
    }

    /// Copie `source` dans le classeur caché. `share` : réutilise la copie déjà
    /// faite de cette colonne dans ce même état (référence figée) ; sinon la
    /// copie est neuve (original d'un remplacement).
    fn copy_into_store(&mut self, source: NodeId, share: bool) -> CoreResult<NodeId> {
        let column = self
            .tree()
            .get(source)?
            .as_column()
            .ok_or_else(|| CoreError::Structure("only a column can be copied".into()))?
            .clone();
        let mark = format!("{SOURCE_MARK}{}", source.raw());
        if share {
            if let Some(sheet) = self.analysis_store_sheet() {
                let found = self.tree().children(sheet)?.iter().copied().find(|c| {
                    self.tree()
                        .node(*c)
                        .and_then(|n| n.as_column())
                        .is_some_and(|col| col.meta().comment == mark && same_content(col, &column))
                });
                if let Some(found) = found {
                    return Ok(found);
                }
            }
        }
        let label = self.path_of(source).unwrap_or_default();
        let sheet = self.ensure_analysis_store()?;
        let mut meta = column.meta().clone();
        // La copie est une photo : ni formule à recalculer, ni rôle de tracé.
        meta.formula = None;
        meta.long_name = label;
        meta.comment = mark;
        let mut copy = column;
        copy.set_meta(meta);
        let id = self.add_column(sheet, None, None, copy)?;
        // Un numéro de version que ce projet n'a jamais attribué à un autre
        // contenu (A7) : la copie n'hérite pas de celui de sa source.
        let version = self.next_column_version();
        self.restamp_column_version(id, version)?;
        Ok(id)
    }

    /// Fige la colonne de référence d'une correction (et son X) : le calcul lira
    /// ces copies. Une référence déjà figée est rendue telle quelle.
    pub(crate) fn freeze_reference(&mut self, aux: &AuxRef) -> CoreResult<AuxRef> {
        if self.is_analysis_copy(aux.y) {
            return Ok(aux.clone());
        }
        let y = self.copy_into_store(aux.y, true)?;
        let x = match aux.x {
            Some(x) if self.is_analysis_copy(x) => Some(x),
            Some(x) => Some(self.copy_into_store(x, true)?),
            None => None,
        };
        Ok(AuxRef { x, y })
    }

    /// Garde l'original de `column` avant un remplacement de données.
    pub(crate) fn keep_original(&mut self, column: NodeId) -> CoreResult<NodeId> {
        self.copy_into_store(column, false)
    }

    /// Rend à `column` le type et les valeurs gardés dans `copy`.
    pub(crate) fn restore_from_copy(&mut self, column: NodeId, copy: NodeId) -> CoreResult<()> {
        let source = self
            .tree()
            .get(copy)?
            .as_column()
            .ok_or_else(|| CoreError::Structure("an original copy must be a column".into()))?;
        let column_type: ColumnType = source.column_type();
        let values: ColumnValues = source.values().clone();
        self.restore_column(column, column_type, values)
    }

    /// Retire les copies que plus aucune analyse ne cite, puis le classeur
    /// caché s'il est vide.
    pub(crate) fn gc_analysis_store(&mut self) {
        let Some(sheet) = self.analysis_store_sheet() else {
            return;
        };
        let mut used: HashSet<NodeId> = HashSet::new();
        let tree = self.tree();
        for calc in tree.calculations().values() {
            if let Some(aux) = &calc.aux {
                used.insert(aux.y);
                used.extend(aux.x);
            }
        }
        for trace in tree.correction_traces().values() {
            if let Some(aux) = &trace.aux {
                used.insert(aux.y);
                used.extend(aux.x);
            }
            used.extend(trace.originals.iter().map(|(_, copy)| *copy));
        }
        let dead: Vec<NodeId> = tree
            .children(sheet)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter(|c| !used.contains(c))
                    .collect()
            })
            .unwrap_or_default();
        for column in dead {
            // Une copie qui ne se retire pas reste : un ménage raté ne défait
            // pas l'analyse qui l'a déclenché.
            let _ = self.remove_without_gc(column);
        }
        let empty = self
            .tree()
            .children(sheet)
            .map(<[NodeId]>::is_empty)
            .unwrap_or(false);
        if empty {
            if let Some(book) = self.tree().parent(sheet).ok().flatten() {
                let _ = self.remove_without_gc(book);
            }
        }
    }
}
