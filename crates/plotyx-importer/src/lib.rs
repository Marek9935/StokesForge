//! Du résultat de [`plotyx_import`] aux **colonnes du modèle** : quels fichiers
//! (et quels blocs) vont sur quelles feuilles ([`plan_import`]), et comment se
//! construit chaque colonne ([`make_import_column`] : type numérique,
//! date-heure ou mixte, nom long, unité, rôle, métadonnées du fichier, source
//! d'import).
//!
//! Ce crate **n'écrit dans aucun projet** : il rend des `Column` et des plans.
//! Chaque consommateur décide de la façon d'écrire — l'application par des
//! commandes annulables dans une macro « Importer », le CLI directement dans un
//! `Project` neuf, les scripts `pyx` par le bus de commandes de leur session.
//! C'est ce qui permet à une seule écriture de la conversion de servir partout
//! (`plotyx-import` reste, lui, indépendant du modèle).

use std::path::{Path, PathBuf};

use plotyx_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, ImportSource, MixedCell, ValueBlock,
};
use plotyx_import::{ColumnKind, ColumnRoleChoice, FilesLayout, ImportRecipe, ParsedTable};

/// Un fichier (ou un bloc de fichier) à importer : son chemin, la recette qui
/// le lit, et le résultat de l'analyse (ou son erreur).
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub path: PathBuf,
    pub recipe: ImportRecipe,
    pub table: Result<ParsedTable, String>,
    /// Rang du bloc quand le fichier en contient plusieurs
    /// (`ImportRecipe::block_split`) ; `None` pour un fichier à un seul bloc.
    pub block: Option<usize>,
}

/// Lit un fichier et rend **un élément par bloc** de données (un seul pour un
/// fichier ordinaire) ; une erreur de lecture ou d'analyse donne un élément en
/// erreur.
pub fn import_items(path: &Path, recipe: &ImportRecipe) -> Vec<ImportItem> {
    let failed = |message: String| {
        vec![ImportItem {
            path: path.to_path_buf(),
            recipe: recipe.clone(),
            table: Err(message),
            block: None,
        }]
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => return failed(format!("{}: {err}", path.display())),
    };
    match plotyx_import::parse_blocks(&bytes, recipe) {
        Ok(tables) => {
            let several = tables.len() > 1;
            tables
                .into_iter()
                .enumerate()
                .map(|(index, table)| ImportItem {
                    path: path.to_path_buf(),
                    recipe: recipe.clone(),
                    table: Ok(table),
                    block: several.then_some(index),
                })
                .collect()
        }
        Err(err) => failed(format!("{}: {err}", path.display())),
    }
}

/// Une colonne à créer : la colonne `column` de l'élément `item`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedColumn {
    pub item: usize,
    pub column: usize,
}

/// Une feuille à remplir : son nom, ses colonnes, les éléments qui y
/// contribuent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    pub name: String,
    pub columns: Vec<PlannedColumn>,
    pub items: Vec<usize>,
}

/// Répartit les colonnes des éléments lisibles en feuilles selon `layout` : une
/// feuille par élément, ou tous côte à côte, ou tous avec le X du premier
/// partagé. Les colonnes « Ignorer » sont exclues partout.
pub fn plan_import(items: &[ImportItem], layout: FilesLayout) -> Vec<ImportPlan> {
    // Nom de la feuille : le fichier, suivi du rang du bloc s'il y en a plusieurs.
    let stem = |item: usize| {
        let base = items[item]
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fichier");
        match items[item].block {
            Some(block) => format!("{base}_{}", block + 1),
            None => base.to_string(),
        }
    };
    let kept = |item: usize| -> Vec<usize> {
        let ImportItem { recipe, table, .. } = &items[item];
        let count = table.as_ref().map_or(0, |t| t.columns.len());
        (0..count)
            .filter(|&c| recipe.column_role(c) != ColumnRoleChoice::Ignore)
            .collect()
    };
    let readable: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].table.is_ok())
        .collect();
    match layout {
        FilesLayout::SheetPerFile => readable
            .iter()
            .map(|&item| ImportPlan {
                name: stem(item),
                columns: kept(item)
                    .into_iter()
                    .map(|column| PlannedColumn { item, column })
                    .collect(),
                items: vec![item],
            })
            .collect(),
        FilesLayout::SideBySide | FilesLayout::SharedX => {
            let Some(&first) = readable.first() else {
                return Vec::new();
            };
            let mut columns = Vec::new();
            for &item in &readable {
                for column in kept(item) {
                    // X partagé : celui du premier ; les suivants n'en apportent pas.
                    let is_x = items[item].recipe.column_role(column) == ColumnRoleChoice::X;
                    if layout == FilesLayout::SharedX && item != first && is_x {
                        continue;
                    }
                    columns.push(PlannedColumn { item, column });
                }
            }
            vec![ImportPlan {
                name: stem(first),
                columns,
                items: readable,
            }]
        }
    }
}

/// Les valeurs d'une colonne planifiée, en bloc prêt à écrire : des nombres, ou
/// des cellules mixtes dès qu'un texte est conservé (colonne forcée en texte
/// comprise).
pub fn import_block(items: &[ImportItem], planned: PlannedColumn) -> Option<ValueBlock> {
    let ImportItem { recipe, table, .. } = items.get(planned.item)?;
    let parsed = table.as_ref().ok()?.columns.get(planned.column)?;
    let forced_text = recipe.column_kind(planned.column) == ColumnKind::Text;
    Some(if forced_text || parsed.texts.iter().any(Option::is_some) {
        ValueBlock::Mixed(
            parsed
                .values
                .iter()
                .zip(&parsed.texts)
                .map(|(value, text)| match text {
                    Some(text) => MixedCell::Text(text.clone()),
                    None => MixedCell::from_f64(*value),
                })
                .collect(),
        )
    } else {
        ValueBlock::Float(parsed.values.clone())
    })
}

/// Construit la colonne `planned` : valeurs typées (numérique, date-heure,
/// mixte), nom long, unité, commentaire, rôle, métadonnées du fichier et source
/// d'import. `None` pour une colonne « Ignorer ». `prefix` cases vides sont
/// ajoutées en tête (colonne neuve d'un import « ajouter des lignes »).
pub fn make_import_column(
    items: &[ImportItem],
    planned: PlannedColumn,
    prefix: usize,
) -> Option<Column> {
    let ImportItem {
        path,
        recipe,
        table,
        block: item_block,
    } = items.get(planned.item)?;
    let table = table.as_ref().ok()?;
    let index = planned.column;
    let parsed_column = table.columns.get(index)?;
    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("fichier");
    // Rôle choisi à l'étape « Colonnes » de l'assistant : une colonne « Ignorer »
    // n'est pas importée.
    let role = match recipe.column_role(index) {
        ColumnRoleChoice::Ignore => return None,
        ColumnRoleChoice::X => ColumnRole::X,
        ColumnRoleChoice::XError => ColumnRole::XError,
        ColumnRoleChoice::YError => ColumnRole::YError,
        ColumnRoleChoice::Y | ColumnRoleChoice::Auto => ColumnRole::Y,
    };
    // Une colonne dont au moins une cellule porte un texte conservé (`n/a`, un
    // code d'instrument…) devient une colonne à types mixtes plutôt qu'une colonne
    // numérique qui aurait tout aplati en `NaN` ; une colonne entièrement
    // numérique garde le chemin `Numeric`.
    let mut column = match import_block(items, planned)? {
        ValueBlock::Mixed(mut cells) => {
            if prefix > 0 {
                let mut padded = vec![MixedCell::Empty; prefix];
                padded.append(&mut cells);
                cells = padded;
            }
            Column::from_mixed(&cells)
        }
        ValueBlock::Float(mut values) => {
            if prefix > 0 {
                let mut padded = vec![f64::NAN; prefix];
                padded.append(&mut values);
                values = padded;
            }
            let mut numeric = Column::from_f64(&values);
            if parsed_column.datetime {
                // Dates et heures : les mêmes `f64` (timestamps POSIX), typés
                // `DateTime` pour l'affichage et les axes.
                numeric.set_column_type(ColumnType::DateTime);
            }
            numeric
        }
    };
    // Le nom long privilégie le rôle de sous-en-tête `long_name`, quand la
    // recette en assigne un — sinon la ligne de noms de colonnes, puis « X » pour
    // la première colonne, puis le nom de la série (fichier ou clé d'en-tête).
    let long_name =
        parsed_column
            .long_name
            .clone()
            .unwrap_or_else(|| match (index, &parsed_column.name) {
                (_, Some(name)) => name.clone(),
                (0, None) => "X".to_string(),
                (_, None) => plotyx_import::resolve_series_name(recipe, table, file_stem),
            });
    column.set_meta(ColumnMeta {
        long_name,
        unit: parsed_column.unit.clone().unwrap_or_default(),
        comment: parsed_column.comment.clone().unwrap_or_default(),
        // Métadonnées du fichier, copiées sur chaque colonne : plusieurs fichiers
        // dans une même feuille gardent chacun les leurs.
        file_metadata: table
            .metadata()
            .iter()
            .map(|line| plotyx_core::MetaLine {
                key: line.key.clone(),
                value: line.value.clone(),
            })
            .collect(),
        // Par défaut la première colonne importée est l'abscisse
        // (`ImportRecipe::column_role`).
        role,
        // De quoi actualiser la colonne depuis son fichier.
        import_source: Some(ImportSource {
            path: path.display().to_string(),
            recipe_json: recipe.to_json(),
            column: index,
            block: item_block.unwrap_or(0),
        }),
        ..Default::default()
    });
    Some(column)
}
