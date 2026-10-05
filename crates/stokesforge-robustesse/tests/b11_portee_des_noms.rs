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
//! Cette batterie vérifie qu’ils disent la même chose. Depuis #105, la portée
//! est le dossier parent, avec refus des chemins courts ambigus.

use stokesforge_core::{CoreError, Node, NodeId, NodeKind, Project, Tree, WindowMeta};

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
fn b11_homonymes_entre_dossiers_acceptes_et_chemins_canoniques_non_ambigus() {
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    let ailleurs = project.add_folder(root, "Ailleurs").expect("dossier");
    let first = project.add_note(root, Some("N"), "racine").unwrap();
    let second = project.add_note(ailleurs, Some("N"), "ailleurs").unwrap();
    assert!(matches!(
        project.add_note(root, Some("N"), ""),
        Err(CoreError::DuplicateName(_))
    ));
    assert!(matches!(
        project.add_note(ailleurs, Some("N"), ""),
        Err(CoreError::DuplicateName(_))
    ));
    assert!(
        project.resolve_path("[N]").is_err(),
        "le nom court ne choisit pas arbitrairement une note"
    );
    assert_eq!(project.resolve_path("/[N]").unwrap(), first);
    assert_eq!(project.resolve_path("/Ailleurs/[N]").unwrap(), second);
    for id in [first, second] {
        assert_eq!(
            project.resolve_path(&project.path_of(id).unwrap()).unwrap(),
            id
        );
    }
    let before = project.tree().clone();
    assert!(
        project.move_node(second, root, 0).is_err(),
        "le déplacement ne crée pas d’homonymes frères"
    );
    assert_eq!(project.tree(), &before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("homonyms.stf");
    stokesforge_persistence::save(&project, &path).unwrap();
    let loaded = stokesforge_persistence::load(&path).unwrap();
    assert_eq!(loaded.tree(), &before);
    for id in [first, second] {
        assert_eq!(
            loaded.resolve_path(&loaded.path_of(id).unwrap()).unwrap(),
            id
        );
    }
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
fn b11_path_of_se_re_resout_toujours_sur_le_meme_noeud() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — corrigée à la racine :
    // `check_name_available` refuse maintenant qu'une fenêtre prenne le nom
    // d'un dossier frère (voir `b11_une_fenetre_ne_prend_pas_le_nom_dun_
    // dossier_frere`), donc le scénario que ce test mettait en place —
    // « note X » acceptée à côté d'un « dossier X » — n'est plus
    // constructible du tout via l'API de mutation. La propriété
    // `path_of ∘ resolve_path = identité` (`b8_proprietes`) n'a donc plus
    // besoin d'être défendue après coup contre ce cas précis : il est
    // devenu impossible à atteindre, ce que ce test vérifie directement.
    let mut project = Project::new("R").expect("projet");
    let root = project.root();
    let dossier = project.add_folder(root, "X").expect("dossier X");
    assert!(
        matches!(
            project.add_note(root, Some("X"), "texte"),
            Err(CoreError::DuplicateName(_))
        ),
        "une note ne doit plus pouvoir prendre le nom d'un dossier frère"
    );

    // Le dossier, lui, reste normalement résoluble.
    assert_eq!(project.resolve_path("/X"), Ok(dossier));
    let chemin = project.path_of(dossier).expect("chemin du dossier");
    assert_eq!(
        project.resolve_path(&chemin),
        Ok(dossier),
        "le chemin canonique d'un nœud doit toujours redésigner ce nœud"
    );
}

#[test]
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
