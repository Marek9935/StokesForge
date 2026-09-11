//! Batterie 20 — deux écrivains, un fichier.
//!
//! `save` tient sa promesse d'**atomicité** : un lecteur ne voit jamais un
//! fichier à moitié écrit, et une écriture qui échoue ne touche pas la cible
//! (`NamedTempFile::persist`, un `rename` sur le même système de fichiers).
//! C'est une garantie contre l'**interruption**.
//!
//! Elle ne dit rien de la **concurrence**. Rien dans le dépôt ne remarque
//! qu'un fichier a changé depuis qu'on l'a lu : ni verrou, ni date de
//! modification retenue, ni empreinte. La question est ancienne et connue —
//! c'est la « mise à jour perdue » de tout logiciel qui édite des documents —
//! et sa réponse n'est écrite nulle part ici.

use plotyx_core::{Column, NodeId, Project, ValueBlock};
use plotyx_persistence::{load, save};
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
        .and_then(plotyx_core::Node::as_column)
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
    let inaccessible = dir.path().join("sous-dossier-absent").join("p.plx");
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
    let a = dir.path().join("a.plx");
    let b = dir.path().join("b.plx");
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
#[ignore = "reproduction B8 — rien ne remarque qu'un fichier a changé depuis qu'on l'a lu : la seconde sauvegarde efface la première, sans un mot"]
fn b20_sauvegarder_sur_un_fichier_modifie_ailleurs_est_refuse() {
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("manip.plx");

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
#[ignore = "reproduction B8 — le format ne porte aucune identité d'écrivain ni aucun horodatage : même a posteriori, rien ne permet de savoir qu'une écriture a été écrasée"]
fn b20_un_fichier_porte_de_quoi_detecter_un_ecrasement() {
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("manip.plx");
    let (mut projet, colonne) = projet();
    save(&projet, &chemin).expect("sauvegarde");
    let premier = std::fs::read(&chemin).expect("lecture");

    projet
        .set_values(colonne, 0, ValueBlock::Float(vec![42.0]))
        .expect("écriture");
    save(&projet, &chemin).expect("sauvegarde");
    let second = std::fs::read(&chemin).expect("lecture");

    // Le corps porte `root`, `next_id_hint` et les nœuds. Rien d'autre.
    println!(
        "deux états successifs du même fichier : {} puis {} octets",
        premier.len(),
        second.len()
    );
    println!(
        "l'en-tête tient en 8 octets — signature « PLYX » + numéro de version — et le \
         corps ne porte ni date, ni compteur de sauvegarde, ni identité d'écrivain"
    );
    // `Column::version` est bien persisté, mais il est **par colonne** et
    // repart d'un état restauré (anomalie A7) : il ne peut pas servir
    // d'estampille de fichier.
    panic!(
        "aucun champ du format ne permet à un lecteur de dire « ce fichier n'est plus \
         celui que j'ai chargé ». Un compteur de sauvegarde monotone dans `FileBody` \
         (quatre octets, `#[serde(default)]`, donc sans bouger le numéro de version) \
         suffirait à transformer l'écrasement silencieux de B8 en un refus explicite, \
         et à rendre possible un « le fichier a changé, recharger ou écraser ? »"
    );
}
