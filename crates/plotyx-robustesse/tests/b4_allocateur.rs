//! Batterie 4 — l'allocateur d'identifiants à la frontière u64.

use plotyx_core::{IdAllocator, Node, NodeId, NodeKind, Project, Tree};
use std::sync::Arc;

#[test]
#[ignore = "reproduction B1 — IdAllocator::next() déborde sur u64::MAX : panique en debug, compteur qui recule en release"]
fn b4_allocateur_sature_puis_deborde() {
    // `resume` sature volontairement (test unitaire existant du crate :
    // `resume_does_not_panic_at_the_u64_boundary`, qui n'appelle que `peek`).
    let mut alloc = IdAllocator::resume(u64::MAX, u64::MAX);
    assert_eq!(alloc.peek(), u64::MAX);
    // Mais le premier `next()` fait `self.next += 1` sur u64::MAX.
    let premier = alloc.next();
    assert_eq!(premier.raw(), u64::MAX);
    println!("après le premier next(), peek = {}", alloc.peek());
    let second = alloc.next();
    println!("second identifiant rendu : {}", second.raw());
    assert!(
        second.raw() > premier.raw(),
        "l'allocateur a reculé : {} après {} — un identifiant sera réattribué",
        second.raw(),
        premier.raw()
    );
}

#[test]
#[ignore = "reproduction B1 — créer un nœud dans un projet chargé portant u64::MAX tue le fil de l'acteur"]
fn b4_projet_charge_avec_identifiant_max_puis_creation() {
    // Reproduit ce qu'un fichier contenant un nœud d'identifiant u64::MAX
    // produit, sans passer par le disque.
    let racine = Node::new(
        NodeId::from_raw(1),
        "R",
        NodeKind::Folder {
            children: vec![NodeId::from_raw(u64::MAX)],
        },
    )
    .unwrap();
    let enfant = Node::with_parent(
        NodeId::from_raw(u64::MAX),
        "A",
        Some(NodeId::from_raw(1)),
        NodeKind::Folder { children: vec![] },
    )
    .unwrap();
    let tree = Tree::from_nodes(NodeId::from_raw(1), vec![racine, enfant]).unwrap();
    let mut project = Project::from_tree(Arc::new(tree), 2);
    println!("next_id_hint après chargement = {}", project.next_id_hint());

    let root = project.root();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        project.add_folder(root, "Nouveau")
    }));
    match r {
        Err(_) => panic!(
            "la création d'un dossier PANIQUE sur un projet dont un nœud porte \
             l'identifiant u64::MAX — en session, c'est le fil de l'acteur qui meurt"
        ),
        Ok(Ok(id)) => println!("créé sous {id}"),
        Ok(Err(e)) => panic!(
            "la création d'un dossier est refusée à jamais : {e} — le projet \
             chargé ne peut plus accueillir aucun nœud"
        ),
    }
}
