//! Test fumée du modèle : construire, cloner, instantanéifier et détruire des
//! projets réalistes sans paniquer ni fuir. C'est le « 0.5 » de la phase 0 : il
//! est écrit pour tourner un jour sous `miri`, donc il n'utilise que l'API
//! publique et reste petit en temps de calcul.

use std::sync::Arc;

use stokesforge_core::{Column, ColumnType, NodeId, Project, Tree, ValueBlock, WindowMeta};

#[test]
fn un_arbre_de_projet_vide_se_clone_et_se_detruit() {
    // B9 (`guide-de-correction-robustesse.md` §F2) — `Project` n'est plus
    // `Clone` : `#[derive(Clone)]` dupliquait `IdAllocator` avec l'arbre, si
    // bien que deux copies d'un même projet attribuaient ensuite les mêmes
    // identifiants à des nœuds différents. Ce que ce test de fumée voulait
    // vraiment couvrir — une structure partagée qui survit à la destruction
    // de ce qui l'a produite — reste vrai pour `Arc<Tree>`, qui est le seul
    // type censé être cloné ici (gratuit, un compteur de références).
    let project = Project::new("Vide").expect("nom valide");
    let tree = Arc::clone(project.tree());
    assert_eq!(tree.len(), project.tree().len());
    drop(project);
    // L'arbre partagé reste utilisable après la destruction du projet.
    assert_eq!(tree.get(tree.root()).expect("racine").name(), "Vide");
}

#[test]
fn un_projet_realiste_supporte_instantanes_et_mutations() {
    let mut project = Project::new("Campagne").expect("nom valide");
    let root = project.root();
    let folder = project.add_folder(root, "Manips").expect("dossier");

    let mut columns: Vec<NodeId> = Vec::new();
    let mut sheets: Vec<NodeId> = Vec::new();
    for _ in 0..3 {
        let book = project.add_empty_workbook(folder, None).expect("classeur");
        for _ in 0..2 {
            let sheet = project.add_worksheet(book, None).expect("feuille");
            sheets.push(sheet);
            for _ in 0..5 {
                let values: Vec<f64> = (0..1000).map(|row| row as f64).collect();
                let column = project
                    .add_column(sheet, None, None, Column::from_f64(&values))
                    .expect("colonne");
                columns.push(column);
            }
        }
    }
    assert_eq!(columns.len(), 30);
    assert_eq!(project.tree().len(), 1 + 1 + 3 + 6 + 30);

    // Dix mutations, un instantané entre chacune.
    let mut snapshots: Vec<Arc<Tree>> = Vec::new();
    for step in 0..10 {
        snapshots.push(project.tree().clone());
        match step % 5 {
            0 => {
                project
                    .set_values(columns[step], 0, ValueBlock::Float(vec![step as f64; 16]))
                    .expect("écriture");
            }
            1 => {
                project
                    .rename(columns[step], &format!("Mesure{step}"))
                    .expect("renommage");
            }
            2 => {
                project
                    .insert_rows(sheets[step % sheets.len()], 10, 5)
                    .expect("insertion de lignes");
            }
            3 => {
                project
                    .add_column(
                        sheets[step % sheets.len()],
                        None,
                        None,
                        Column::new(ColumnType::Numeric),
                    )
                    .expect("colonne");
            }
            _ => {
                let note = project
                    .add_note(root, None, "relevé du jour")
                    .expect("note");
                project
                    .set_window_meta(
                        note,
                        WindowMeta {
                            label: format!("Note {step}"),
                            comment: String::new(),
                        },
                    )
                    .expect("métadonnées");
            }
        }
    }
    assert_eq!(snapshots.len(), 10);

    // Chaque instantané reste lisible et cohérent.
    for snapshot in &snapshots {
        assert!(snapshot.contains(root));
        assert_eq!(snapshot.walk_all().len(), snapshot.len());
    }

    // Le plus ancien instantané est restaurable ; le compteur ne recule pas.
    let hint = project.next_id_hint();
    let oldest = snapshots[0].clone();
    project.replace_tree(oldest);
    assert_eq!(project.next_id_hint(), hint);
    assert!(project.add_folder(root, "Après").expect("dossier").raw() >= hint);

    drop(snapshots);
    drop(project);
}
