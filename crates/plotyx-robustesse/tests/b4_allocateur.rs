//! Batterie 4 — l'allocateur d'identifiants à la frontière u64.

use plotyx_core::{IdAllocator, Node, NodeId, NodeKind, Project, Tree};
use std::sync::Arc;

#[test]
fn b4_allocateur_sature_puis_refuse() {
    // B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — `next()` rend
    // désormais `CoreResult<NodeId>` : à la frontière `u64::MAX`, il refuse
    // au lieu de déborder silencieusement (`self.next += 1` sur `u64::MAX`,
    // qui paniquait en debug et faisait **reculer** le compteur en release —
    // deux nœuds sous le même identifiant).
    //
    // `resume` sature volontairement (test unitaire existant du crate :
    // `resume_does_not_panic_at_the_u64_boundary`, qui n'appelle que `peek`).
    let mut alloc = IdAllocator::resume(u64::MAX, u64::MAX);
    assert_eq!(alloc.peek(), u64::MAX);
    let premier = alloc.next();
    println!("next() à la frontière u64::MAX → {premier:?}");
    assert!(
        premier.is_err(),
        "l'allocateur doit refuser plutôt que déborder ou reculer : {premier:?}"
    );
    // Refuse à nouveau, sans avoir bougé : jamais de régression, jamais de
    // réattribution.
    let second = alloc.next();
    assert!(second.is_err(), "un allocateur épuisé le reste : {second:?}");
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
