//! Batterie 11 — la portée d'unicité des noms, des deux côtés de l'arbre.
//!
//! Un dossier est le seul parent du modèle qui accueille deux **catégories**
//! d'enfants (`NodeKind::accepts_child` : des dossiers *et* des fenêtres). La
//! question « ce nom est-il libre ici ? » y a donc deux réponses possibles, et
//! trois endroits du dépôt y répondent :
//!
//! - `Tree::check_name_available` — à la mutation ;
//! - `Tree::validate_loaded_names` — au chargement d'un fichier ;
//! - `Tree::child_by_name`, filtré par `is_folder` — à la résolution d'un
//!   chemin de dossier.
//!
//! Cette batterie vérifie qu'ils disent la même chose. Ils ne la disent pas.

use plotyx_core::{CoreError, Node, NodeId, NodeKind, Project, Tree, WindowMeta};

fn dossier_kind(enfants: Vec<NodeId>) -> NodeKind {
    NodeKind::Folder { children: enfants }
}

fn note_kind() -> NodeKind {
    NodeKind::Note {
        meta: WindowMeta::default(),
        text: String::new(),
    }
}

/// Arbre à plat : une racine et ses enfants, dans l'ordre donné.
fn arbre(enfants: Vec<(u64, &str, NodeKind)>) -> Result<Tree, CoreError> {
    let ids: Vec<NodeId> = enfants
        .iter()
        .map(|(id, _, _)| NodeId::from_raw(*id))
        .collect();
    let mut noeuds = vec![Node::new(NodeId::from_raw(1), "R", dossier_kind(ids))?];
    for (id, nom, kind) in enfants {
        noeuds.push(Node::with_parent(
            NodeId::from_raw(id),
            nom,
            Some(NodeId::from_raw(1)),
            kind,
        )?);
    }
    Tree::from_nodes(NodeId::from_raw(1), noeuds)
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b11_deux_dossiers_freres_homonymes_sont_refuses() {
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    project.add_folder(root, "X").expect("premier dossier");
    assert!(
        matches!(
            project.add_folder(root, "X"),
            Err(CoreError::DuplicateName(_))
        ),
        "deux dossiers frères ne peuvent pas porter le même nom"
    );
}

#[test]
fn b11_deux_fenetres_homonymes_sont_refusees_ou_quelles_soient() {
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    let ailleurs = project.add_folder(root, "Ailleurs").expect("dossier");
    project
        .add_note(root, Some("N"), "")
        .expect("première note");
    assert!(
        matches!(
            project.add_note(ailleurs, Some("N"), ""),
            Err(CoreError::DuplicateName(_))
        ),
        "un nom de fenêtre est unique à l'échelle du projet, pas seulement entre frères"
    );
}

#[test]
fn b11_un_dossier_ne_peut_pas_prendre_le_nom_dune_fenetre_soeur() {
    // Ce sens-ci est bien gardé : `check_name_available` compare un dossier à
    // TOUS ses frères, quelle que soit leur catégorie. C'est l'autre sens qui
    // ne l'est pas (voir la reproduction A8 ci-dessous).
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    project.add_note(root, Some("X"), "").expect("note");
    assert!(
        matches!(
            project.add_folder(root, "X"),
            Err(CoreError::DuplicateName(_))
        ),
        "un dossier ne peut pas prendre le nom d'une note sœur"
    );
}

#[test]
fn b11_le_chargeur_refuse_deux_fenetres_homonymes() {
    let erreur = arbre(vec![(2, "X", note_kind()), (3, "X", note_kind())])
        .expect_err("deux notes homonymes doivent être refusées");
    assert!(matches!(erreur, CoreError::Structure(_)), "{erreur:?}");
}

// -- reproductions --------------------------------------------------------

#[test]
#[ignore = "reproduction A8 — check_name_available sort par la branche « fenêtre » sans jamais regarder les frères : une fenêtre peut prendre le nom d'un dossier frère"]
fn b11_une_fenetre_ne_prend_pas_le_nom_dun_dossier_frere() {
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    project.add_folder(root, "X").expect("dossier X");

    let note = project.add_note(root, Some("X"), "texte");
    println!("add_note(\"X\") à côté du dossier « X » → {note:?}");
    assert!(
        matches!(note, Err(CoreError::DuplicateName(_))),
        "l'ordre de création ne devrait rien changer : dossier puis note est accepté, \
         note puis dossier est refusé (voir le garde-fou b11_un_dossier_ne_peut_pas…)"
    );
}

#[test]
#[ignore = "reproduction A8 — un dossier précédé d'une fenêtre homonyme devient introuvable par son chemin : path_of ∘ resolve_path n'est plus l'identité"]
fn b11_path_of_se_re_resout_toujours_sur_le_meme_noeud() {
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    let dossier = project.add_folder(root, "X").expect("dossier X");
    let note = project
        .add_note(root, Some("X"), "texte")
        .expect("note X acceptée — c'est déjà l'anomalie A8");

    // Tant que le dossier vient en premier dans la liste d'enfants, tout va
    // bien : `child_by_name` tombe sur lui.
    assert_eq!(project.resolve_path("/X"), Ok(dossier));

    // `Move` est l'une des huit primitives du socle, et elle accepte ce
    // déplacement (la branche « fenêtre » de `check_name_available` ne
    // regarde toujours pas les frères).
    project
        .move_node(note, root, 0)
        .expect("déplacement accepté");

    let chemin = project.path_of(dossier).expect("chemin du dossier");
    let retour = project.resolve_path(&chemin);
    println!("path_of({dossier}) = {chemin:?} → resolve_path = {retour:?}");
    println!(
        "le dossier est pourtant toujours là : {}",
        project.tree().contains(dossier)
    );
    assert_eq!(
        retour,
        Ok(dossier),
        "le chemin canonique d'un nœud doit toujours redésigner ce nœud — c'est la \
         propriété que `b8_proprietes` énonce (path_of ∘ resolve_path = identité), et \
         `child_by_name` la perd en rendant le PREMIER frère homonyme, que le filtre \
         `is_folder` transforme ensuite en « dossier introuvable »"
    );
}

#[test]
#[ignore = "reproduction A8 — validate_loaded_names ne compare qu'à catégorie égale : le chargeur accepte des arbres que l'API de mutation refuse"]
fn b11_le_chargeur_refuse_ce_que_lapi_de_mutation_refuse() {
    // Une note puis un dossier homonymes : `Project::add_folder` refuse
    // exactement cette paire (garde-fou b11_un_dossier_ne_peut_pas…), le
    // chargeur l'accepte.
    let resultat = arbre(vec![
        (2, "X", note_kind()),
        (3, "X", dossier_kind(Vec::new())),
    ]);
    match &resultat {
        Ok(tree) => println!(
            "chargé : {} nœuds, resolve(\"/X\") = {:?}",
            tree.len(),
            tree.resolve("/X")
        ),
        Err(e) => println!("refusé : {e}"),
    }
    assert!(
        resultat.is_err(),
        "le chargeur accepte un arbre que l'API de mutation ne sait pas construire : \
         les deux jeux d'invariants doivent coïncider, sinon un fichier ouvre un projet \
         qu'aucune commande ne pourra reproduire ni réparer"
    );
}
