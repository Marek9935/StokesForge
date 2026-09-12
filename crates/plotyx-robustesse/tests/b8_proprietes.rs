//! Batterie 8 — property-based testing (proptest) sur la grammaire de chemins
//! et les primitives de colonne.

use plotyx_core::{format_path, parse_path, Column, ColumnType, ObjectPath, Project, ValueBlock};
use proptest::prelude::*;

/// Un segment de nom valide au sens de `check_name` : non vide, sans espace de
/// bordure, sans caractère réservé.
fn segment() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9éàü _.()+-]{1,12}"
        .prop_map(|s| s.trim().to_string())
        .prop_filter("non vide après détourage", |s| !s.is_empty())
}

fn chemin() -> impl Strategy<Value = ObjectPath> {
    (
        proptest::collection::vec(segment(), 0..4),
        proptest::option::of(segment()),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(folders, window, avec_feuille, avec_colonne, rooted)| {
            // Un chemin qui a une colonne a forcément une feuille, et une
            // feuille suppose une fenêtre : on respecte la grammaire.
            let window_present = window.is_some();
            ObjectPath {
                folders,
                sheet: if window_present && (avec_feuille || avec_colonne) {
                    Some("Feuille1".to_string())
                } else {
                    None
                },
                column: if window_present && avec_colonne {
                    Some("ColA".to_string())
                } else {
                    None
                },
                window,
                rooted,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    /// `parse_path(format_path(p)) == p` pour tout chemin représentable.
    ///
    /// Le chemin entièrement vide (`ObjectPath::default()`) est exclu : il
    /// n'est produit par aucune analyse (`parse_path("")` rend `Err`), sa
    /// forme canonique « / » est donc légitimement relue comme ancrée.
    #[test]
    fn b8_aller_retour_chemin(p in chemin()) {
        prop_assume!(p.window.is_some() || !p.folders.is_empty() || p.rooted);
        let texte = format_path(&p);
        let relu = parse_path(&texte);
        prop_assert!(
            relu.is_ok(),
            "forme canonique {texte:?} du chemin {p:?} refusée à la relecture : {:?}",
            relu.unwrap_err()
        );
        let relu = relu.unwrap();
        prop_assert_eq!(
            &relu, &p,
            "aller-retour infidèle : {:?} -> {} -> {:?}", p, texte, relu
        );
    }

    /// Analyser deux fois le texte canonique donne le même résultat.
    #[test]
    fn b8_forme_canonique_est_stable(p in chemin()) {
        let t1 = format_path(&p);
        if let Ok(q) = parse_path(&t1) {
            let t2 = format_path(&q);
            prop_assert_eq!(&t1, &t2, "la forme canonique n'est pas idempotente");
        }
    }

    /// `parse_path` ne panique jamais, quelle que soit l'entrée.
    #[test]
    fn b8_parse_ne_panique_jamais(texte in ".{0,60}") {
        let _ = parse_path(&texte);
    }

    /// Écrire puis annuler une écriture ramène la colonne à l'identique.
    #[test]
    fn b8_ecriture_puis_annulation_est_fidele(
        initiales in proptest::collection::vec(-1e6f64..1e6, 0..40),
        start in 0usize..60,
        ecrites in proptest::collection::vec(-1e6f64..1e6, 0..40),
    ) {
        use plotyx_commands::{Command, History};
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let col = p.add_column(sheet, None, Some("A"), Column::from_f64(&initiales)).unwrap();
        let avant = std::sync::Arc::clone(p.tree());

        let mut h = History::new();
        h.execute(&mut p, Command::SetCells {
            column: col, start, values: ValueBlock::Float(ecrites.clone()),
            expected_version: None,
        }).unwrap();
        prop_assert!(h.undo(&mut p).unwrap());
        prop_assert!(
            avant.as_ref() == p.tree().as_ref(),
            "annuler SetCells(start={start}, {} valeurs) sur {} valeurs initiales \
             ne rend pas l'arbre d'avant",
            ecrites.len(), initiales.len()
        );
    }

    /// Insérer puis supprimer les mêmes lignes ramène la feuille à l'identique.
    #[test]
    fn b8_insert_puis_delete_lignes_est_neutre(
        colonnes in proptest::collection::vec(
            proptest::collection::vec(-100f64..100.0, 0..15), 1..4),
        at in 0usize..20,
        count in 0usize..10,
    ) {
        use plotyx_commands::{Command, History};
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        for (i, vals) in colonnes.iter().enumerate() {
            p.add_column(sheet, None, Some(&format!("C{i}")), Column::from_f64(vals)).unwrap();
        }
        let avant = std::sync::Arc::clone(p.tree());
        let mut h = History::new();
        h.execute(&mut p, Command::InsertRows { sheet, at, count }).unwrap();
        // B5 (`guide-de-correction-robustesse.md` §F5) — un `InsertRows` qui
        // ne touche aucune colonne (ou `count == 0`) n'empile plus rien :
        // `undo` rend alors `Ok(false)` (rien à annuler), ce qui est tout
        // aussi neutre que défaire un vrai effet. Les deux cas laissent
        // l'arbre égal à `avant` ; seule cette égalité est la propriété
        // testée, pas le résultat de `undo`.
        h.undo(&mut p).unwrap();
        prop_assert!(
            avant.as_ref() == p.tree().as_ref(),
            "InsertRows(at={at}, count={count}) puis annulation n'est pas neutre"
        );
    }

    /// Le changement de type puis son annulation restitue type ET valeurs.
    #[test]
    fn b8_changement_de_type_sannule(
        valeurs in proptest::collection::vec(-1e10f64..1e10, 0..30),
        cible in 0usize..3,
    ) {
        use plotyx_commands::{Command, History};
        let types = [ColumnType::Numeric, ColumnType::DateTime, ColumnType::Text];
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        p.add_column(sheet, None, Some("A"), Column::from_f64(&valeurs)).unwrap();
        let col = p.resolve_path("[Book1]Sheet1!A").unwrap();
        let avant = std::sync::Arc::clone(p.tree());
        let mut h = History::new();
        h.execute(&mut p, Command::SetColumnType { column: col, column_type: types[cible] }).unwrap();
        prop_assert!(h.undo(&mut p).unwrap());
        prop_assert!(
            avant.as_ref() == p.tree().as_ref(),
            "changer le type vers {:?} puis annuler ne restitue pas la colonne",
            types[cible]
        );
    }

    /// Aller-retour disque fidèle pour un contenu quelconque.
    #[test]
    fn b8_aller_retour_disque(
        colonnes in proptest::collection::vec(
            proptest::collection::vec(
                prop_oneof![
                    Just(f64::NAN), Just(-0.0), Just(0.0), Just(f64::INFINITY),
                    Just(f64::NEG_INFINITY), -1e300f64..1e300,
                ], 0..20), 0..5),
    ) {
        let dir = tempfile::TempDir::new().unwrap();
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        for (i, vals) in colonnes.iter().enumerate() {
            p.add_column(sheet, None, Some(&format!("C{i}")), Column::from_f64(vals)).unwrap();
        }
        let chemin = dir.path().join("p.plx");
        plotyx_persistence::save(&p, &chemin).unwrap();
        let relu = plotyx_persistence::load(&chemin).unwrap();
        prop_assert_eq!(p.tree().len(), relu.tree().len());
        for (id, node) in p.tree().iter() {
            let autre = relu.tree().node(id).unwrap();
            if let (Some(c1), Some(c2)) = (node.as_column(), autre.as_column()) {
                prop_assert_eq!(c1.len(), c2.len());
                for r in 0..c1.len() {
                    prop_assert_eq!(
                        c1.get(r), c2.get(r),
                        "colonne {} ligne {} altérée par l'aller-retour disque", node.name(), r
                    );
                }
            }
        }
    }

    /// Le nom automatique de colonne ne collisionne jamais.
    #[test]
    fn b8_noms_automatiques_sont_libres(n in 1usize..200) {
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let mut noms = std::collections::HashSet::new();
        for _ in 0..n {
            let id = p.add_column(sheet, None, None, Column::new(ColumnType::Numeric)).unwrap();
            let nom = p.tree().node(id).unwrap().name().to_string();
            prop_assert!(noms.insert(nom.clone()), "nom automatique répété : {nom}");
        }
    }

    /// `path_of` puis `resolve_path` est l'identité pour tout nœud de l'arbre.
    #[test]
    fn b8_chemin_canonique_se_reresout(
        dossiers in proptest::collection::vec(segment(), 0..3),
        feuilles in 1usize..4,
        colonnes in 0usize..4,
    ) {
        let mut p = Project::new("Racine").unwrap();
        let mut parent = p.root();
        let mut vus = std::collections::HashSet::new();
        for d in &dossiers {
            if !vus.insert(d.clone()) { continue; }
            parent = match p.add_folder(parent, d) { Ok(id) => id, Err(_) => continue };
            vus.clear();
        }
        let book = p.add_empty_workbook(parent, Some("Book1")).unwrap();
        for f in 0..feuilles {
            let sheet = p.add_worksheet(book, Some(&format!("S{f}"))).unwrap();
            for c in 0..colonnes {
                p.add_column(sheet, None, Some(&format!("C{c}")), Column::new(ColumnType::Numeric)).unwrap();
            }
        }
        for id in p.tree().walk_all() {
            let chemin = p.path_of(id).unwrap();
            let retrouve = p.resolve_path(&chemin);
            prop_assert!(
                retrouve.is_ok(),
                "le chemin canonique {chemin:?} du nœud {id} n'est pas re-résolvable : {:?}",
                retrouve.unwrap_err()
            );
            prop_assert_eq!(
                retrouve.unwrap(), id,
                "le chemin canonique {} désigne un autre nœud", chemin
            );
        }
    }
}
