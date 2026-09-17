//! Batterie 1 — valeurs limites de la logique métier (hors allocation géante,
//! qui vit dans b6 pour ne pas tuer la machine de test).

use plotyx_core::{Cell, Column, ColumnType, ColumnValues, MixedCell, Project, ValueBlock};
use plotyx_plot::{PanelSpec, PlotSpec};
use std::num::NonZeroU32;

/// Fabrique un projet [Book1]Sheet1 avec une colonne A de `values`.
fn projet_avec_colonne(values: &[f64]) -> (Project, plotyx_core::NodeId) {
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let col = project
        .add_column(sheet, None, Some("A"), Column::from_f64(values))
        .unwrap();
    (project, col)
}

// -- histogram --

#[test]
fn b1_histogram_intervalle_infini_ne_produit_pas_de_nan() {
    // Deux valeurs finies dont l'écart déborde f64 : width = +inf.
    let values = [-f64::MAX, f64::MAX];
    let (edges, counts) = plotyx_core::histogram(&values, NonZeroU32::new(4).unwrap());
    println!("edges = {edges:?}");
    println!("counts = {counts:?}");
    assert!(
        edges.iter().all(|e| e.is_finite()),
        "des bornes d'histogramme non finies : {edges:?}"
    );
    assert_eq!(
        counts.iter().sum::<f64>(),
        values.len() as f64,
        "des points ont été perdus ou comptés deux fois : {counts:?}"
    );
}

#[test]
fn b1_histogram_compte_tous_les_points_finis() {
    let values = [1.0, 2.0, 3.0, 4.0, f64::NAN, f64::INFINITY];
    let (_edges, counts) = plotyx_core::histogram(&values, NonZeroU32::new(3).unwrap());
    assert_eq!(counts.iter().sum::<f64>(), 4.0, "counts = {counts:?}");
}

#[test]
fn b1_histogram_valeurs_vides() {
    let (edges, counts) = plotyx_core::histogram(&[], NonZeroU32::new(5).unwrap());
    println!("vide -> edges={edges:?} counts={counts:?}");
    // Documenté : (vec![0.0], vec![]). On vérifie juste l'absence de panique
    // et la cohérence edges.len() == counts.len() + 1 attendue d'un histogramme.
    assert_eq!(
        edges.len(),
        counts.len() + 1,
        "invariant d'histogramme rompu : edges={edges:?}, counts={counts:?}"
    );
}

#[test]
fn b1_histogram_grand_nombre_de_classes_ne_panique_pas() {
    // bins énorme : le vecteur de comptage est alloué à cette taille.
    let values = [1.0, 2.0];
    let (edges, counts) = plotyx_core::histogram(&values, NonZeroU32::new(1_000_000).unwrap());
    assert_eq!(edges.len(), 1_000_001);
    assert_eq!(counts.iter().sum::<f64>(), 2.0);
}

// -- bar_width --

#[test]
fn b1_bar_width_demandee_absurde_est_filtree() {
    // La doc dit « celle demandée si elle est positive, sinon 80 % du pas
    // médian ». On vérifie que le contrat documenté est tenu.
    let x = [0.0, 1.0, 2.0, 3.0];
    for demandee in [-5.0, 0.0, f64::NAN, f64::INFINITY] {
        let w = plotyx_core::bar_width(Some(&x), Some(demandee));
        println!("bar_width(demandee={demandee}) = {w}");
        assert!(
            w.is_finite() && w > 0.0,
            "largeur de barre inutilisable ({w}) pour une demande de {demandee}"
        );
    }
}

// -- PlotSpec grid --

#[test]
fn b1_plotspec_rows_columns_sur_position_extreme() {
    let spec = PlotSpec {
        panels: vec![PanelSpec::at(usize::MAX, usize::MAX)].into(),
        ..PlotSpec::default()
    };
    // B2 — `saturating_add(1)` : ni panique, ni débordement vers 0.
    println!("rows = {}", spec.rows());
    println!("columns = {}", spec.columns());
    assert_eq!(spec.rows(), usize::MAX);
    assert_eq!(spec.columns(), usize::MAX);
}

// -- colonne --

#[test]
fn b1_conversion_texte_vers_numerique_puis_retour() {
    let (mut project, col) = projet_avec_colonne(&[]);
    let textes: Vec<Option<String>> = vec![
        Some("1.5".into()),
        Some(" 2.5 ".into()),
        Some("".into()),
        None,
        Some("pas un nombre".into()),
        Some("1e400".into()), // déborde f64
        Some("-0".into()),
        Some("inf".into()),
        Some("NaN".into()),
    ];
    project.set_column_type(col, ColumnType::Mixed).unwrap();
    let cellules: Vec<MixedCell> = textes
        .iter()
        .cloned()
        .map(|value| match value {
            None => MixedCell::Empty,
            Some(text) => MixedCell::Text(text),
        })
        .collect();
    project
        .set_values(col, 0, ValueBlock::Mixed(cellules))
        .unwrap();
    project.set_column_type(col, ColumnType::Numeric).unwrap();
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    let vals: Vec<f64> = match column.values() {
        ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("type inattendu"),
    };
    println!("texte -> numérique : {vals:?}");
    assert_eq!(vals.len(), textes.len(), "des lignes ont disparu");
    // C2 — décidé : un texte fini en toutes lettres dont la valeur déborde
    // f64 ("1e400") est refusé, jamais silencieusement +inf ; il retombe sur
    // la valeur manquante du modèle, exactement comme "pas un nombre".
    assert!(
        vals[5].is_nan(),
        "« 1e400 » déborde et doit devenir la valeur manquante (NaN), pas {}",
        vals[5]
    );
    // Une non-finitude *explicitement demandée*, elle, n'est pas un
    // débordement : elle est prise au mot.
    assert_eq!(
        vals[6].to_bits(),
        (-0.0_f64).to_bits(),
        "« -0 » doit rester -0.0"
    );
    assert_eq!(vals[7], f64::INFINITY, "« inf » doit rester +inf");
    assert!(vals[8].is_nan(), "« NaN » doit devenir la valeur manquante");
}

// `b1_aller_retour_numerique_texte_numerique_preserve_les_valeurs` a été
// retiré : il vérifiait qu'un aller-retour `Numeric → Text → Numeric`
// (formatage par `format_f64_for_display` puis reparsing par
// `parse_f64_text`) était fidèle bit à bit. Il n'y a plus de conversion vers
// une forme textuelle formatée : `Numeric → Mixed` est sans perte
// (`MixedCell::Number`, jamais une chaîne), donc `Numeric → Mixed →
// Numeric` préserve les valeurs *trivialement*, sans passer par un
// formatage — l'invariant qui donnait son intérêt à ce test (la fidélité
// d'un aller-retour texte) n'a plus de chemin de code à couvrir.

#[test]
fn b1_cellule_texte_vide_est_distincte_de_manquante() {
    let (mut project, col) = projet_avec_colonne(&[]);
    project.set_column_type(col, ColumnType::Mixed).unwrap();
    project
        .set_values(
            col,
            0,
            ValueBlock::Mixed(vec![MixedCell::Text(String::new()), MixedCell::Empty]),
        )
        .unwrap();
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(column.get(0), Cell::Text(Some(String::new())));
    assert_eq!(column.get(1), Cell::Text(None));
    // Documenté : les deux comptent comme « manquantes » à l'affichage.
    assert!(column.is_missing(0));
    assert!(column.is_missing(1));
}

#[test]
fn b1_lecture_hors_bornes_rend_manquant_jamais_erreur() {
    let (project, col) = projet_avec_colonne(&[1.0]);
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(column.get(usize::MAX), Cell::Float(f64::NAN));
    assert!(column.is_missing(usize::MAX));
}

#[test]
fn b1_delete_rows_count_max_ne_deborde_pas() {
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let removed = project.delete_rows(sheet, 1, usize::MAX).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].1.len(), 2);
    assert_eq!(project.tree().row_count(sheet).unwrap(), 1);
}

#[test]
fn b1_insert_rows_at_egal_longueur_ajoute_en_fin_de_feuille() {
    // B5 (`guide-de-correction-robustesse.md` §F5) — `at == len` est
    // désormais une cible pour `insert_rows` (pas pour `delete_rows`, qui ne
    // désigne aucune ligne existante à cette position) : c'est la seule
    // façon d'ajouter une ligne en fin de feuille, la plage que
    // `Column::insert_rows` accepte déjà par colonne (`at ∈ 0..=len`). Cette
    // reproduction couvrait auparavant le bug inverse (« ignoré ») ; elle
    // couvre maintenant la garantie qui le remplace.
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let touched = project.insert_rows(sheet, 2, 3).unwrap();
    assert_eq!(touched, vec![a]);
    assert_eq!(project.tree().row_count(sheet).unwrap(), 5);
}

#[test]
fn b1_compteur_de_version_avance_a_chaque_ecriture() {
    let (mut project, col) = projet_avec_colonne(&[1.0]);
    let v0 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    project
        .set_values(col, 0, ValueBlock::Float(vec![2.0]))
        .unwrap();
    let v1 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    assert_ne!(v0, v1, "le compteur n'a pas bougé après une écriture");
    // Un refus ne doit pas l'incrémenter.
    let _ = project.set_values(col, 0, ValueBlock::Mixed(vec![MixedCell::Empty]));
    let v2 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    assert_eq!(v1, v2, "une écriture refusée a fait avancer le compteur");
}
