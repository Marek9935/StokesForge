//! Banc de robustesse : ce crate ne contient pas de code de production.
//!
//! Il porte deux choses, et rien d'autre :
//!
//! - `tests/` — vingt-deux batteries qui éprouvent le socle aux limites :
//!   valeurs numériques extrêmes, fichiers `.stf` malformés puis
//!   sémantiquement hostiles, macros de l'historique, concurrence entre
//!   clients de l'acteur, propriétés tirées au sort (proptest), la frontière
//!   Rust ↔ Python dans les deux sens, puis — passe du 2026-09-11 après-midi,
//!   Rust pur — le contrat du compteur de version (`b10`), la portée
//!   d'unicité des noms (`b11`), les commandes acceptées sans effet (`b12`),
//!   les bornes d'un `PlotSpec` venu du disque (`b13`) et un balayage qui
//!   revérifie trois invariants après chaque pas d'une séquence tirée au sort
//!   (`b14`), la traduction cellule ↔ texte (`b15`), les séries dont les
//!   colonnes n'ont pas la même longueur (`b16`), le coût des opérations
//!   courantes quand le projet grandit (`b17`), ce qui reste possible pendant
//!   qu'une macro est ouverte (`b18`), ce qu'un fichier écrit par une version
//!   plus récente fait à ce lecteur-ci (`b19`), deux écrivains sur un même
//!   fichier (`b20`), le coût comme l'identité sur le chemin d'écriture
//!   (`b21`) et — #193 — le recalcul ciblé des formules confronté, pas à
//!   pas, au recalcul complet d'une session jumelle (`b22`) ;
//! - `src/bin/sonde_alloc.rs` et `src/bin/sonde_pile.rs` — deux sondes
//!   hors-processus, parce que les cas qu'elles éprouvent se terminent par un
//!   `abort()` que rien ne rattrape : un échec d'allocation pour la première,
//!   un débordement de pile pour la seconde.
//!
//! # Deux natures de test, et comment les distinguer
//!
//! **Les garde-fous** passent, et doivent continuer à passer : ce sont des
//! tests de non-régression ordinaires, qui entrent dans `cargo test
//! --workspace` comme les autres.
//!
//! **Les reproductions** échouent, délibérément. Chacune reproduit une
//! anomalie constatée le 2026-09-11 et consignée dans l'un des deux rapports
//! de ce jour — `docs/archive/rustrewrite/rapport-robustesse-2026-09-11.md` (A1 à C2,
//! batteries 1 à 9) et `docs/archive/rustrewrite/rapport-robustesse-2026-09-11-suite.md`
//! (A7 à D2, batteries 10 à 20) — sous l'identifiant (A1, B2, …) que porte son
//! attribut `#[ignore]`. Elles sont marquées
//! `#[ignore]` pour que le pipeline garde sa valeur de signal — un CI rouge
//! en permanence ne dit plus rien — et se lancent par :
//!
//! ```text
//! cargo test --workspace -- --ignored
//! ```
//!
//! **Une reproduction qui se met à passer est le signal qu'on attend** : le
//! correctif est bon, et c'est le moment de retirer son `#[ignore]` pour que
//! le garde-fou prenne le relais.
//!
//! # Par où commencer
//!
//! `docs/archive/rustrewrite/guide-de-correction-robustesse.md` range les trente
//! anomalies **par fichier**, sépare les correctifs mécaniques des décisions
//! à prendre, donne un ordre qui minimise le travail refait, et liste six
//! pièges de correction vérifiés sur le dépôt (dont celui-ci : un correctif
//! de C3 qui ferait *échouer* un bloc vide casserait l'annulation).
//!
//! # La feature `python`
//!
//! Les batteries `b7` et `b9` sont derrière `#![cfg(feature = "python")]` :
//! sans la feature, elles compilent vides. Elles exigent les trois variables
//! d'environnement de `scripts/python-test.sh`, qui les lance déjà.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
