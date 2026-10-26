# Critères de la V1 (#333)

La V1 sort quand **chaque case ci-dessous est vraie**, constatée sur le commit
qui porte le tag `v1.0.0`. Cette page dit ce qui est exigé et comment le
constater ; le contenu de la version est dans [`CHANGELOG.md`](../CHANGELOG.md).

## Ce que la V1 promet

| Promesse | Tenue par |
|---|---|
| Un fichier `.stf` écrit par la V1 se relit et se migre jusqu'en V3 ; un fichier d'un **format** plus récent (numéro de format supérieur) est refusé avec un message clair | [`persistance.md`](persistance.md) § règle de version, [`invariants.md`](invariants.md) n° 12, fichier témoin `crates/stokesforge-persistence/tests/temoins/v1/` |
| Un script `pyst` écrit contre la V1 continue de marcher | [`python-et-script.md`](python-et-script.md) § « API figée V1 », test `crates/stokesforge-script/tests/surface_figee.rs` |
| L'application et son Python embarqué s'installent et démarrent sur Linux x86_64 et Windows x86_64 | `scripts/app-dist.sh`, `scripts/app-dist-windows.sh` ([`architecture.md`](architecture.md), § « Où sont les binaires ») |

Le reste du code n'a **aucune** promesse de compatibilité : `main` prépare la
version majeure suivante (voir « Branches et versions » plus bas).

## Critères

Chaque ligne se constate par une commande ou une page ; aucune ne se déclare
« faite » sans l'avoir constatée.

### Contenu

- [ ] Toutes les issues du jalon V1 sont fermées (`glab issue list --milestone V1`
  ne rend rien).
- [ ] Aucune issue ouverte étiquetée `bug` ne touche la perte de données, le
  format `.stf` ou le pont Python (`glab issue list --label bug`).

### Qualité

- [ ] Sur le commit du tag, les travaux bloquants sont verts : `fmt`, `licenses`,
  `clippy`, `test`, `test-python` (`scripts/ci-statuts.sh <commit>`).
- [ ] `test-python-windows` et `dist-windows` (automatiques sur `main` et sur un tag) sont verts sur le même commit.
- [ ] `cargo test -p stokesforge-robustesse -- --ignored` ne rend aucun échec
  (zéro rouge : plus aucun `#[ignore]` ne reste, voir `docs/tests.md`).

### Distribution

- [ ] `scripts/app-dist.sh` sort 0 sur Linux : la fenêtre s'ouvre et se ferme,
  `stokesforge-selftest` passe, depuis le paquet. La CI le fait sous écran
  virtuel (xvfb) au tag ; avant le tag, l'essayer aussi à la main sur un bureau
  Wayland réel. Essai à la main fait le 2026-10-26 sur Ubuntu 26.04 (GNOME,
  Wayland, VM, rendu logiciel) plutôt que sur Fedora, pour sortir de la
  distribution de développement : paquet construit sur la machine de dev,
  copié hors du dépôt, `stokesforge-selftest` code 0, application, console
  `pyst`/numpy/scipy, import, `.stf`, exports et glisser-déposer
  fonctionnent. Reste, pour cocher : la CI au tag, et refaire le même essai
  avec l'archive du tag `v1.0.0-rc1` (construite sur Ubuntu par la CI).
- [ ] `scripts/app-dist-windows.sh` sort 0 sur le runner Windows et l'installeur
  `StokesForge-<version>-windows-setup.exe` s'installe, démarre et se désinstalle
  sur un poste neuf : au moins une fois à la main, sur un autre ordinateur
  Windows, avec l'installeur produit par le tag `v1.0.0-rc1` (le runner n'a pas
  de bureau).
- [ ] Les notices de licences sont livrées dans les deux paquets, et celles de
  Windows sont recoupées avec l'archive python-build-standalone Windows complète
  (`vendor/python/licenses-windows/README.md`).
- [ ] Windows affiche « éditeur inconnu » (pas de signature de code) : assumé
  (décision de la V1) et écrit dans les notes de version, section « Installation
  sous Windows » de `CHANGELOG.md`.

### Version et documentation

- [ ] Tous les crates sont en `1.0.0` (`scripts/release-notes.sh v1.0.0` le vérifie).
- [ ] `CHANGELOG.md` a une section `## [1.0.0]` datée, sans « à dater ».
- [ ] Les pages de `docs/` citées par les promesses ci-dessus décrivent le code
  tel qu'il est (relecture, pas de renvoi à une issue encore ouverte).

## Publier

1. Constater les critères ci-dessus ; dater la section `## [1.0.0]` du CHANGELOG ;
   créer le jeton qui laisse GitHub téléverser l'archive Linux dans GitLab : un
   jeton de projet GitLab (rôle Developer, portée `api`), posé comme secret
   GitHub `GITLAB_RELEASE_TOKEN`.
2. Fusionner sur `main`, puis créer la branche `release/1.x` à ce commit.
3. Poser le tag `v1.0.0` sur `release/1.x` et le pousser vers GitLab (le miroir
   le transmet à GitHub).
4. Le tag déclenche, en parallèle :
   - GitLab `dist-windows` puis `publier` : installeur Windows et release GitLab
     (notes tirées du CHANGELOG) ;
   - GitHub `release-linux`, après les contrôles bloquants : archive
     `StokesForge-1.0.0-linux-x86_64.tar.gz`, téléversée dans le registre de
     paquets GitLab (aucune release GitHub : GitHub n'est qu'un miroir).
     `publier` attend l'archive (une heure au plus) avant de créer la release.
5. Constater : la release GitLab existe, l'installeur et l'archive se
   téléchargent depuis elle.

Les travaux de publication refusent un tag incohérent avant tout téléversement :
forme `vX.Y.Z`, versions des crates, section du CHANGELOG (`scripts/release-notes.sh`).

**Non éprouvé.** Ces travaux n'ont jamais tourné sur un vrai tag : seuls les
scripts ont été essayés en local (`release-notes.sh`, `release-linux.sh` sur une
distribution existante, `release-publier.sh` en `STOKESFORGE_RELEASE_DRYRUN=1`).
Un tag `vX.Y.Z-rcN` lance la CI GitHub complète et, sur GitLab,
`test-python-windows` et `dist-windows` (leur règle accepte le suffixe) ;
`release-linux` construit l'archive Linux et la téléverse dans le registre de
paquets GitLab (`release-notes.sh` n'y vérifie que les versions des crates) ;
la release GitLab, elle, ignore le tag (`publier` n'accepte que `vX.Y.Z`). Le
tag `-rcN` éprouve donc le runner Windows, l'installeur (artefact de
`dist-windows`, pour l'essai sur un autre poste Windows) et le téléversement de
l'archive Linux. Seul l'appel qui crée la release GitLab (formulaire, liens
vers l'installeur et l'archive) n'a lieu qu'au premier vrai tag ; `release-linux-televerser.sh` et
`release-publier.sh` s'essaient à vide avec `STOKESFORGE_RELEASE_DRYRUN=1`.

## Branches et versions

- `main` : travail vers la version majeure suivante. Aucune contrainte de
  rétrocompatibilité du code ; seuls les fichiers `.stf` V1 restent relisibles
  (invariant 12) et l'API `pyst` ne change que par décision explicite.
- `release/N.x` : une branche par version majeure, reçoit les correctifs
  (cherry-pick depuis `main`). Tags `v1.0.0`, `v1.0.1`…
- À partir de la V1, **la règle « pas de rétrocompatibilité du format en
  développement » ne s'applique plus** : tout changement de ce qui est écrit
  dans un `.stf` suit la règle de version de `persistance.md`.

## Hors V1

Compression et somme de contrôle du `.stf`, repli `Unknown` sur les autres
enums du format, signature de code Windows, distribution macOS : ils n'ont pas
de critère ici. Voir #323 pour ce qui peut attendre la V2.
