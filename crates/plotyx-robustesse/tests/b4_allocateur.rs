//! Batterie 4 — l'allocateur d'identifiants à la frontière u64.

use plotyx_core::IdAllocator;
use plotyx_persistence::load;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

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

// --- Schéma minimal dupliqué de `plotyx-persistence`, pour forger un
// fichier `.plx` -----------------------------------------------------------
//
// Même légitimité que dans `b3_fichiers_hostiles.rs` (voir sa note en tête
// de fichier) : le format est auto-descriptif (maps nommées), donc une
// déclaration d'accord sur les noms de champs produit les mêmes octets que
// le vrai schéma. On ne reprend ici que le strict nécessaire — un `Folder`
// racine et un `Folder` enfant — pour porter un identifiant de nœud
// au-delà de la borne de lecture.
#[derive(Serialize)]
struct FileBody {
    root: u64,
    next_id_hint: u64,
    nodes: Vec<NodeRecord>,
}
#[derive(Serialize)]
struct NodeRecord {
    id: u64,
    name: String,
    parent: Option<u64>,
    kind: KindRecord,
}
#[derive(Serialize)]
enum KindRecord {
    Folder { children: Vec<u64> },
}

fn ecrire(dir: &TempDir, nom: &str, body: &FileBody) -> PathBuf {
    let payload = rmp_serde::to_vec_named(body).unwrap();
    let path = dir.path().join(nom);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"PLYX").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    f.sync_all().unwrap();
    path
}

// Pourquoi l'ancienne reproduction de ce fichier a été remplacée
// ----------------------------------------------------------------
//
// Cette place portait `b4_projet_charge_avec_identifiant_max_puis_creation`
// (B1, `guide-de-correction-robustesse.md` §F3/F4) : un `Project::from_tree`
// construit **directement en mémoire** — `Tree::from_nodes`/`Node::new`,
// sans passer par le disque — avec un nœud d'identifiant `u64::MAX`, puis un
// `add_folder` dont on attendait le succès.
//
// Ce scénario est devenu inatteignable par un vrai fichier depuis que B1 est
// corrigé des deux côtés :
// - `IdAllocator::next` (`plotyx-core`, `id.rs:43`) refuse via `checked_add`
//   à la frontière `u64::MAX` plutôt que de déborder ou reculer ;
// - `plotyx_persistence::load` (`lib.rs`, `check_node_id`, ~ligne 327)
//   refuse dès la lecture tout identifiant de nœud (ou `next_id_hint`)
//   au-delà de `MAX_NODE_ID` (10¹²) — très loin de `u64::MAX`, mais bien
//   avant que le fichier ne puisse faire arriver en mémoire un `Project`
//   dont un identifiant frôle la vraie frontière.
//
// Un fichier `.plx` réel ne peut donc plus produire l'état que l'ancien test
// fabriquait à la main : le « refus permanent d'écrire » qu'il observait
// n'était pas un bug — si `u64::MAX` est déjà pris, il ne reste
// mathématiquement plus rien à distribuer. Ce qui reste à garantir, et que
// le test ci-dessous vérifie à la place, c'est que le chemin qui *pourrait*
// amener un tel identifiant en mémoire — un fichier falsifié — est bien
// coupé à la lecture, proprement (une erreur, pas un panic, pas un succès).
#[test]
fn b4_un_identifiant_de_noeud_au_dela_de_la_borne_est_refuse_a_la_lecture() {
    let dir = TempDir::new().unwrap();

    // `MAX_NODE_ID` (`plotyx-persistence/src/lib.rs`) est privée au crate ;
    // on la reproduit ici littéralement — c'est la borne documentée dans le
    // message d'erreur qu'on vérifie plus bas, pas un détail d'implémentation
    // qu'on devine.
    const MAX_NODE_ID: u64 = 1_000_000_000_000;
    let id_hors_borne = MAX_NODE_ID + 1;

    // La racine et `next_id_hint` restent bien en-deçà de la borne : seul le
    // nœud enfant la dépasse, pour isoler précisément ce que `check_node_id`
    // doit attraper sur un identifiant de nœud (pas sur le compteur).
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".into(),
                parent: None,
                kind: KindRecord::Folder {
                    children: vec![id_hors_borne],
                },
            },
            NodeRecord {
                id: id_hors_borne,
                name: "A".into(),
                parent: Some(1),
                kind: KindRecord::Folder { children: vec![] },
            },
        ],
    };
    let path = ecrire(&dir, "id_hors_borne.plx", &body);

    // `load` doit refuser proprement : pas de panic (c'est le fil de
    // l'acteur en session), pas de succès (ce serait rouvrir exactement le
    // scénario que B1 a fermé). Si `MAX_NODE_ID` n'existait pas ou n'était
    // pas appliqué à chaque identifiant de nœud (et pas seulement à
    // `next_id_hint`), ce fichier serait accepté et cette assertion
    // échouerait : c'est ce qui fait de ce test un vrai garde-fou, pas un
    // test qui passe par construction.
    let resultat = std::panic::catch_unwind(|| load(&path));
    assert!(
        resultat.is_ok(),
        "load() a paniqué sur un identifiant de nœud au-delà de MAX_NODE_ID"
    );
    let resultat = resultat.unwrap();
    let erreur = match resultat {
        Err(e) => e,
        Ok(_) => panic!(
            "un identifiant de nœud ({id_hors_borne}) au-delà de MAX_NODE_ID \
             ({MAX_NODE_ID}) a été ACCEPTÉ à la lecture"
        ),
    };
    println!("identifiant hors-borne à la lecture → {erreur}");
    let message = erreur.to_string();
    assert!(
        message.contains(&MAX_NODE_ID.to_string()) && message.contains("borne"),
        "le message d'erreur devrait citer la borne acceptée ({MAX_NODE_ID}) : {message}"
    );
}
