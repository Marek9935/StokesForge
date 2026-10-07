//! Recalcul des colonnes-formule, rejoué par l'acteur après chaque requête
//! qui a pu changer le projet (voir `actor::run`) — hors historique.

use std::collections::{BTreeMap, BTreeSet};

use stokesforge_core::{
    evaluate_formula, topological_order, ColumnType, Formula, NodeId, NumericStatus, Project,
    ValueBlock,
};

/// Recalcule toutes les colonnes-formule du projet — accès **direct** à
/// `Project` (`Project::set_length`/`Project::set_values`), jamais via
/// `History::apply`/`Command::execute` : c'est un effet de bord silencieux
/// rejoué après coup, pas une mutation annulable (voir la doc de tête de
/// crate, `docs/commandes-et-historique.md`) — exactement le même statut que
/// `actor::purge_orphaned_geometries`, qui rejoue déjà la purge de géométrie
/// après toute exécution structurelle. Le texte de la formule
/// (`Command::SetColumnMeta`) et les cellules sources (`Command::SetCells`,
/// `InsertRows`, `DeleteRows`, ...) restent, eux, annulables normalement :
/// c'est seulement le recalcul qui ne pousse jamais sa propre entrée.
///
/// # Portée du déclenchement
///
/// Appelée après **tout** `Execute`/`Undo`/`Redo`/`AbortMacro` qui a réussi
/// (donc a pu changer le projet) et après tout `Load` (voir `run`,
/// ci-dessus, pour chaque site d'appel et sa justification propre — en
/// particulier `Load`, qui recalcule plutôt que de faire confiance aux
/// valeurs lues du disque).
///
/// À l'intérieur de cette fonction, la portée est volontairement **large** :
/// tout l'arbre est balayé, pas seulement « la feuille que la commande vient
/// de toucher ». Deux raisons à ce choix plutôt qu'un déclenchement plus fin
/// piloté par `Command::is_structural()` ou par les variantes précises de
/// `Command` :
///
/// - `Undo`/`Redo`/`AbortMacro` ne portent aucune commande à inspecter ici —
///   seule `History` sait ce qu'elle vient de restaurer, et elle ne l'expose
///   pas (elle n'a d'ailleurs pas à le faire : c'est un détail
///   d'implémentation d'`Entry`) ; il faudrait lui ajouter une surface
///   dédiée rien que pour ce besoin.
/// - Même côté `Execute`, `Command::Move` peut déplacer une **colonne**
///   d'une feuille à une autre : les lettres de position changent à la fois
///   dans la feuille de départ et dans celle d'arrivée, alors que `Move` ne
///   distingue en rien ce cas d'un déplacement de dossier sans le moindre
///   effet sur une formule.
///
/// Le coût de ce balayage est celui de `actor::purge_orphaned_geometries` :
/// linéaire dans le nombre de nœuds pour trouver les feuilles, puis borné
/// par le nombre de colonnes-formule effectivement présentes (le cas
/// courant : aucune) — jamais par la taille des données des colonnes qui
/// n'en portent pas.
pub(crate) fn recalc_formulas(project: &mut Project) {
    let sheets: Vec<NodeId> = project
        .tree()
        .iter()
        .filter(|(_, node)| node.kind().is_worksheet())
        .map(|(id, _)| id)
        .collect();
    for sheet in sheets {
        recalc_sheet(project, sheet);
    }
}

/// Recalcule les colonnes-formule d'une seule feuille, dans l'ordre de leurs
/// dépendances (tri topologique, [`stokesforge_core::topological_order`]) — une
/// colonne-formule qui en référence une autre lit sa valeur **déjà
/// recalculée**, jamais celle d'avant cette passe. Un cycle de dépendance
/// (direct ou transitif) est détecté sans boucler : chaque colonne qui y
/// participe reçoit un bloc entièrement `NaN` et
/// `FormulaError::Cycle` plutôt qu'une valeur qui n'aurait de toute façon
/// aucun sens à choisir.
fn recalc_sheet(project: &mut Project, sheet: NodeId) {
    let Ok(columns) = project.tree().children(sheet).map(<[NodeId]>::to_vec) else {
        return;
    };
    // Longueur cible commune à toute la feuille : le maximum des longueurs
    // actuelles de ses colonnes (formule comprises), au moment où cette
    // passe commence — la même notion que `Tree::row_count`, qui gouverne
    // déjà `InsertRows`/`DeleteRows`.
    let target_len = project.tree().row_count(sheet).unwrap_or(0);

    // Formules analysées à l'avance : une erreur de *parse* est déjà un
    // résultat final (bloc `NaN`, `FormulaError::Parse`), elle ne participe
    // pas au graphe de dépendance — une colonne qui la référencerait la
    // trouve immédiatement disponible (à `NaN`), pas « en attente ».
    let mut parsed: BTreeMap<NodeId, Formula> = BTreeMap::new();
    for &id in &columns {
        let Some(source) = project
            .tree()
            .node(id)
            .and_then(|node| node.as_column())
            .and_then(|column| column.meta().formula.clone())
        else {
            continue;
        };
        match Formula::parse(&source) {
            Ok(formula) => {
                parsed.insert(id, formula);
            }
            Err(_error) => write_formula_column(
                project,
                id,
                vec![f64::NAN; target_len],
                &vec![NumericStatus::Missing; target_len],
            ),
        }
    }

    // Type de sortie d'une colonne-formule : `Numeric` si toutes les
    // colonnes qu'elle référence sont `Numeric`/`DateTime` (l'échec est alors
    // impossible, seules des cases manquantes peuvent apparaître), `Mixed`
    // dès qu'une seule est `Mixed` (l'échec devient possible — voir
    // `write_formula_column`, qui a besoin de ce type pour savoir où écrire
    // `"---"`). Recalculé à chaque passe plutôt que figé à la création : une
    // formule référence potentiellement une colonne dont le type change
    // ensuite (conversion manuelle). Appliqué directement au modèle plutôt
    // que par une commande annulable, comme `write_formula_column`
    // ci-dessous : c'est la même dérivation « hors historique » que le
    // recalcul des valeurs (voir la doc de tête de `recalc_sheet`), pas une
    // saisie utilisateur à elle seule annulable.
    for (&id, formula) in &parsed {
        if let Some(desired) = desired_formula_column_type(formula, &columns, project) {
            let current = project
                .tree()
                .node(id)
                .and_then(|node| node.as_column())
                .map(stokesforge_core::Column::column_type);
            if current != Some(desired) {
                let _ = project.set_column_type(id, desired);
            }
        }
    }

    // Graphe de dépendance entre colonnes-formule uniquement : une référence
    // vers une colonne ordinaire n'est pas une arête à ordonner, elle est
    // déjà disponible (voir la doc de `stokesforge_core::topological_order`).
    let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> =
        parsed.keys().map(|&id| (id, BTreeSet::new())).collect();
    for (&id, formula) in &parsed {
        for index in formula.referenced_columns() {
            if let Some(&dep_id) = columns.get(index) {
                if parsed.contains_key(&dep_id) {
                    if let Some(deps) = edges.get_mut(&id) {
                        deps.insert(dep_id);
                    }
                }
            }
        }
    }
    let (order, blocked) = topological_order(&edges);

    for id in blocked {
        write_formula_column(
            project,
            id,
            vec![f64::NAN; target_len],
            &vec![NumericStatus::Missing; target_len],
        );
    }
    for id in order {
        let Some(formula) = parsed.get(&id) else {
            continue;
        };
        let Some(self_index) = columns.iter().position(|&c| c == id) else {
            continue;
        };
        // Relu à chaque colonne, pas une fois pour toute la feuille : les
        // colonnes-formule déjà traitées dans cette boucle (celles dont `id`
        // dépend, forcément plus tôt dans `order`) doivent être vues à jour.
        let values: Vec<_> = columns
            .iter()
            .map(|&column_id| {
                project
                    .tree()
                    .node(column_id)
                    .and_then(|node| node.as_column())
                    .map(|column| column.values().clone())
            })
            .collect();
        // Une colonne de `columns` a disparu entre la lecture de `columns`
        // et maintenant : ne peut pas arriver (cette fonction ne touche
        // jamais la structure de l'arbre), garde défensive plutôt qu'un
        // panic si l'hypothèse venait à être fausse un jour (invariant 10).
        if values.iter().any(Option::is_none) {
            continue;
        }
        let values: Vec<_> = values.into_iter().flatten().collect();
        let evaluation = evaluate_formula(formula, self_index, &values, target_len);
        write_formula_column(project, id, evaluation.values, &evaluation.statuses);
    }
}

/// Type que devrait porter la colonne-résultat de `formula` d'après les
/// colonnes qu'elle référence, ou `None` quand ça ne peut pas se décider
/// (référence hors bornes, ou vers une colonne qui a disparu) — dans ce cas,
/// l'appelant laisse le type de la colonne inchangé plutôt que de choisir au
/// hasard.
///
/// `Mixed` dès que la formule référence au moins une colonne — pas
/// seulement si l'une d'elles est déjà `Mixed`. N'importe quelle colonne,
/// même purement `Numeric`, peut avoir une case vide sur une ligne
/// quelconque ; or une colonne `Numeric` ne peut stocker que des nombres,
/// jamais le texte `"---"` qu'[`write_formula_column`] écrit désormais
/// aussi bien pour [`NumericStatus::Failed`] que pour
/// [`NumericStatus::Missing`] (case manquante, pas seulement en échec) —
/// sans quoi une opérande manquante resterait une case muette plutôt qu'un
/// résultat marqué comme non calculé. Seule une formule sans la moindre
/// référence (une constante pure, qui ne peut jamais manquer ni échouer)
/// reste `Numeric`.
fn desired_formula_column_type(
    formula: &Formula,
    columns: &[NodeId],
    project: &Project,
) -> Option<ColumnType> {
    let mut any_reference = false;
    for index in formula.referenced_columns() {
        let &column_id = columns.get(index)?;
        project
            .tree()
            .node(column_id)
            .and_then(|node| node.as_column())?;
        any_reference = true;
    }
    Some(if any_reference {
        ColumnType::Mixed
    } else {
        ColumnType::Numeric
    })
}

/// Écrit `values`/`statuses` dans `column`, en le tronquant/étendant d'abord
/// à sa longueur (`Project::set_length`) puisque `Project::set_values`
/// n'écrase jamais une queue de colonne plus longue que le bloc écrit.
/// Ignore silencieusement un échec (`column` disparu entre-temps, ce que
/// rien dans cette fonction ne provoque) plutôt que de paniquer — invariant
/// 10.
///
/// Pour une colonne-formule **mixte**, une ligne au statut
/// [`NumericStatus::Failed`] **ou** [`NumericStatus::Missing`] s'écrit
/// `MixedCell::Text("---")` plutôt que `MixedCell::Empty` : un résultat non
/// calculé, que ce soit parce qu'une colonne référencée porte du texte non
/// numérique (`Failed`) ou parce qu'une opérande manque tout simplement
/// (`Missing`), se voit toujours signalé plutôt que de laisser une case
/// muette qu'on confondrait avec une valeur pas encore saisie. `statuses`
/// doit avoir la même longueur que `values` ; sinon ce sont les valeurs
/// seules qui gouvernent (aucune ligne n'est marquée), comme avant l'ajout
/// de ce paramètre.
fn write_formula_column(
    project: &mut Project,
    column: NodeId,
    values: Vec<f64>,
    statuses: &[NumericStatus],
) {
    // Le bloc doit correspondre au type de la colonne, sans quoi
    // `set_values` refuse et le recalcul serait silencieusement perdu. Une
    // colonne-formule sans référence (constante pure) reste numérique, mais
    // dès qu'elle référence une colonne, `desired_formula_column_type` l'a
    // déjà retypée en mixte : `ValueBlock::into_mixed` fait alors des `NaN`
    // des cellules vides, ce que la formule voulait dire (invariant 10 —
    // dégrader, pas avaler) ; une ligne `Failed` ou `Missing` s'y écrit
    // `"---"` plutôt qu'une case vide, voir la doc ci-dessus.
    let is_mixed = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .is_some_and(|target| target.column_type() == ColumnType::Mixed);
    let length = values.len();
    let block = if is_mixed {
        let mut cells = ValueBlock::Float(values).into_mixed();
        if statuses.len() == cells.len() {
            for (cell, status) in cells.iter_mut().zip(statuses) {
                if matches!(status, NumericStatus::Failed | NumericStatus::Missing) {
                    *cell = stokesforge_core::MixedCell::Text("---".to_string());
                }
            }
        }
        ValueBlock::Mixed(cells)
    } else {
        ValueBlock::Float(values)
    };
    if project.set_length(column, length).is_err() {
        return;
    }
    let _ = project.set_values(column, 0, block);
}
