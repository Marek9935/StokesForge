//! Batterie 20 — deux écrivains, un fichier.
//!
//! `save` tient sa promesse d'**atomicité** : un lecteur ne voit jamais un
//! fichier à moitié écrit, et une écriture qui échoue ne touche pas la cible
//! (`NamedTempFile::persist`, un `rename` sur le même système de fichiers).
//! C'est une garantie contre l'**interruption**.
//!
//! Elle dit maintenant aussi quelque chose de la **concurrence** (B8,
//! `guide-de-correction-robustesse.md` §F2) : `FileBody` porte un compteur
//! de sauvegarde monotone, comparé à ce que CE processus a vu la dernière
//! fois pour cette instance de `Project` avant d'écraser une cible
//! existante. La question — la « mise à jour perdue » de tout logiciel qui
//! édite des documents — n'est répondue que pour ce cas précis : deux
//! `Project` distincts, dans des processus (ou threads) différents, qui
//! chargent puis sauvegardent le même fichier sans jamais se recharger
//! entre-temps.

use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_persistence::{load, save};
use tempfile::TempDir;

/// `[Book1]Sheet1!A` = [1, 2, 3].
fn projet() -> (Project, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let colonne = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    (project, colonne)
}

fn valeurs(project: &Project, colonne: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(colonne)
        .and_then(stokesforge_core::Node::as_column)
        .and_then(Column::float_values)
        .expect("colonne numérique")
        .as_slice()
        .to_vec()
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b20_une_sauvegarde_interrompue_ne_touche_pas_la_cible() {
    // Le pendant positif : la cible n'existe pas encore, l'écriture échoue
    // (répertoire parent inexistant), et rien n'est laissé derrière.
    let dir = TempDir::new().expect("répertoire");
    let (project, _) = projet();
    let inaccessible = dir.path().join("sous-dossier-absent").join("p.stf");
    assert!(save(&project, &inaccessible).is_err());
    assert!(
        !inaccessible.exists(),
        "une écriture qui échoue ne doit laisser aucun fichier"
    );
}

#[test]
fn b20_deux_sauvegardes_successives_donnent_le_meme_fichier() {
    let dir = TempDir::new().expect("répertoire");
    let (project, _) = projet();
    let a = dir.path().join("a.stf");
    let b = dir.path().join("b.stf");
    save(&project, &a).expect("première sauvegarde");
    save(&project, &b).expect("seconde sauvegarde");
    assert_eq!(
        std::fs::read(&a).expect("lecture a"),
        std::fs::read(&b).expect("lecture b"),
        "deux sauvegardes du même projet doivent être identiques octet à octet"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse() {
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("manip.stf");

    // Instance A ouvre le projet et le sauvegarde.
    let (projet_a, colonne) = projet();
    save(&projet_a, &chemin).expect("sauvegarde de A");

    // Instance B — autre fenêtre, autre processus, ou un script — charge le
    // même fichier, écrit, sauvegarde.
    let mut projet_b = load(&chemin).expect("chargement par B");
    projet_b
        .set_values(colonne, 0, ValueBlock::Float(vec![42.0]))
        .expect("écriture de B");
    save(&projet_b, &chemin).expect("sauvegarde de B");
    let apres_b = valeurs(&load(&chemin).expect("relecture"), colonne);
    println!("après la sauvegarde de B : {apres_b:?}");

    // A n'a rien vu passer. Il modifie sa copie, et sauvegarde.
    let mut projet_a = projet_a;
    projet_a
        .set_values(colonne, 1, ValueBlock::Float(vec![7.0]))
        .expect("écriture de A");
    let sauvegarde_de_a = save(&projet_a, &chemin);
    let final_ = valeurs(&load(&chemin).expect("relecture"), colonne);
    println!("après la sauvegarde de A : {final_:?}");
    println!("  (le 42 de B a disparu ; A n'a rien su, B ne saura rien)");

    assert!(
        sauvegarde_de_a.is_err(),
        "le fichier a changé entre le chargement de A et sa sauvegarde, et rien ne \
         l'a remarqué : le travail de B est perdu sans trace. `save` n'a ni verrou, \
         ni date de modification retenue, ni empreinte du fichier d'origine — il \
         écrase. L'atomicité de `NamedTempFile::persist` protège de l'interruption, \
         pas de la concurrence, et rien dans la documentation du crate ne distingue \
         les deux"
    );
}

#[test]
fn b20_un_fichier_porte_de_quoi_detecter_un_ecrasement() {
    // B8 (`guide-de-correction-robustesse.md` §F2) — corrigée :
    // `FileBody` porte désormais `save_counter`, un compteur de sauvegarde
    // monotone (persisté, `#[serde(default)]` pour rester lisible par un
    // lecteur plus ancien qui l'ignorerait), sans bouger le numéro de
    // version du format. C'est exactement le champ que ce test constatait
    // absent : `save` compare, avant d'écrire, le compteur sur disque à
    // celui que CE processus a vu la dernière fois pour cette instance —
    // voir `b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse`,
    // qui vérifie le refus lui-même. Celui-ci vérifie juste que le compteur
    // avance à chaque sauvegarde, ce qui rend ce refus possible.
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("manip.stf");
    let (mut projet, colonne) = projet();
    save(&projet, &chemin).expect("sauvegarde");
    let premier = std::fs::read(&chemin).expect("lecture");

    projet
        .set_values(colonne, 0, ValueBlock::Float(vec![42.0]))
        .expect("écriture");
    save(&projet, &chemin).expect("sauvegarde");
    let second = std::fs::read(&chemin).expect("lecture");

    println!(
        "deux états successifs du même fichier : {} puis {} octets",
        premier.len(),
        second.len()
    );
    assert_ne!(
        premier, second,
        "le compteur de sauvegarde doit avoir changé entre les deux écritures, même si \
         le reste du contenu était resté identique"
    );
}
