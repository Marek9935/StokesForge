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

Le reste du code n'a **aucune** promesse de compatibilité d'une version
majeure à l'autre (voir « Branches et versions » plus bas).

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
   créer le jeton qui laisse GitHub téléverser les paquets dans GitLab et y créer la release : un
   jeton de projet GitLab (rôle Developer, portée `api`), posé comme secret
   GitHub `GITLAB_RELEASE_TOKEN`.
2. Fusionner sur `main` : il n'y a pas de branche de version à créer.
3. Poser le tag `v1.0.0` sur ce commit de `main` et le pousser vers GitLab (le
   miroir le transmet à GitHub).
4. Le tag déclenche, sur GitHub seulement (le miroir le transmet) :
   - les contrôles bloquants, Linux et Windows ;
   - `release-linux` et `dist-windows` : archive
     `StokesForge-1.0.0-linux-x86_64.tar.gz` et installeur
     `StokesForge-1.0.0-windows-setup.exe`, téléversés dans le registre de
     paquets GitLab (aucune release GitHub : GitHub n'est qu'un miroir) ;
   - `publier`, quand tout cela est fait : release GitLab (notes tirées du
     CHANGELOG, liens vers les deux paquets). Aucun runner GitLab n'y sert.
5. Constater : la release GitLab existe, l'installeur et l'archive se
   téléchargent depuis elle.

**Variante pour une version `X.Y.Z` suivante** (`1.0.1`, `1.1.0`, `2.0.0`) : les
étapes sont les mêmes, avec ce qui change :

1. Un commit de version dédié : `version = "X.Y.Z"` dans tous les crates,
   `cargo update --workspace`, section `## [X.Y.Z] - date` du CHANGELOG (la
   section `[Non publié]` y est renommée et datée).
2. CI verte sur ce commit (`scripts/ci-statuts.sh <commit>`), `test-python-windows`
   et `dist-windows` compris.
3. Tag `vX.Y.Z` sur ce commit de `main` (ou de `release/N.x` pour un correctif
   d'une ancienne majeure), **sur ordre explicite de l'utilisateur**. Un
   `-rcN` d'abord seulement dans les cas de « Branches et versions ».

Les travaux de publication refusent un tag incohérent avant tout téléversement :
forme `vX.Y.Z`, versions des crates, section du CHANGELOG (`scripts/release-notes.sh`).

**Non éprouvé.** Ces travaux n'ont jamais tourné sur un vrai tag : seuls les
scripts ont été essayés en local (`release-notes.sh`, `release-linux.sh` sur une
distribution existante, `release-publier.sh` en `STOKESFORGE_RELEASE_DRYRUN=1`).
Un tag `vX.Y.Z-rcN` lance la CI GitHub complète et, aussi sur
GitHub, `test-python-windows` et `dist-windows` (runners hébergés) ;
`release-linux` construit l'archive Linux et la téléverse dans le registre de
paquets GitLab (`release-notes.sh` n'y vérifie que les versions des crates) ;
`publier` crée la release GitLab puis la supprime aussitôt (essai, le tag
reste). Le tag `-rcN` éprouve donc les runners Windows et Linux, les deux
téléversements (d'où l'on prend l'installeur pour l'essai sur un autre poste
Windows) et l'appel de création de la release. Seul le contenu final des notes
(section du CHANGELOG) n'est lu qu'au vrai tag ; `release-televerser.sh` et
`release-publier.sh` s'essaient aussi à vide avec `STOKESFORGE_RELEASE_DRYRUN=1`.

## Branches et versions

Principe : **une seule ligne tant que rien ne diverge**, une branche de plus le
jour où un gros chantier empêcherait de livrer un correctif. Une branche se
crée à tout moment depuis un tag ; il est inutile d'en ouvrir une d'avance.

```
 Phase 1 : une seule ligne            Phase 2 : un chantier qui casse
 main ──●──●──●──●──●──►              main ──●──●──●──►   V1 continue
       ▼1.0.0 ▼1.0.1 ▼1.1.0                  ↓ fusion  ↓ fusion
                                       v2 ───●──●──●──●──►  chantier cassant
```

- **`main`** : la ligne livrable. Les corrections, les petits ajouts sans
  risque et leurs tags (`v1.0.1`, `v1.1.0`…) y sont posés directement. Elle est
  protégée (pas de force-push, voir `docs/tests.md` § CI).
- **Numéros** : `Z` (`1.0.1`) = correction, ou champ additif du format `.stf`
  (voir `persistance.md` : risque et annonce au CHANGELOG) ; `Y` (`1.1.0`) =
  fonctionnalité ajoutée sans rien casser ; `X` (`2.0.0`) = rupture de
  compatibilité, dont tout changement cassant du format (`FORMAT_VERSION`
  monte).
- **Branches de fonctionnalité** : `feature/<n>-<sujet>`, une par ticket, courtes
  (quelques jours). Avant de fusionner, on fusionne `main` dedans. Elle entre
  dans `main` par fusion `--no-ff`, commit « Fusion #N : … (Closes #N) ».
  **Jamais** de squash ni de rebase d'une branche partagée.
- **Ouvrir `vN`** (ex. `v2`) quand un chantier laisserait le code inutilisable
  pendant des semaines (changement de bibliothèque d'interface, refonte du
  modèle). Critère : *peut-on encore sortir un correctif de `main` pendant les
  travaux ?* Sinon, branche. Un ajout de fonctionnalité n'en demande pas.
  **Une seule branche `vN` à la fois.** Tant qu'elle est ouverte, `main` ne
  reçoit que des correctifs et de petits ajouts ; les fonctionnalités vont dans
  `vN`. Dans `vN`, les commits disent « Refs #N » : « Closes » ne ferme
  l'issue qu'à la fusion dans `main`.
- **Le bug se corrige là où il est né** : un bug présent sur `main` se corrige
  sur `main`, dans un commit isolé, et arrive dans `vN` par **fusion** de `main`
  (pas par copie à la main) ; un bug de `release/1.x` absent de `main` se
  corrige directement sur `release/1.x`, sans cherry-pick. Fusionner `main`
  dans `vN` souvent, chaque semaine environ : une branche longue sans fusion
  donne des conflits énormes. `main` → `vN` se fait toujours par fusion.
- **Chaque correctif porte son test de régression.** À la fusion `main` → `vN`
  en conflit, on garde le test et on réécrit la correction dans le code de `vN`.
- **Format `.stf` gelé** : dès que le format N+1 existe sur une branche, le
  format N est gelé partout ailleurs (rien n'y change plus), sinon un fichier
  réenregistré perdrait des données sans message.
- **Fin du chantier** : `vN` est fusionnée dans `main`, tag `vN.0.0`. Si la
  version précédente doit rester maintenue, créer alors `release/(N-1).x` depuis
  son dernier tag ; les correctifs nés sur `main` y arrivent par cherry-pick
  (la ligne du CHANGELOG avec).
- **Tags `-rcN`** : après la V1, seulement avant une version `X` ou quand la CI
  ou les scripts de distribution ont changé ; toujours sur ordre explicite de
  l'utilisateur. Un tag n'est posé que sur `main` ou sur `release/N.x` : la CI
  refuse sinon.
- Aucune contrainte de rétrocompatibilité du code ; seuls les fichiers `.stf`
  V1 restent relisibles (invariant 12) et l'API `pyst` ne change que par
  décision explicite.
- À partir de la V1, **la règle « pas de rétrocompatibilité du format en
  développement » ne s'applique plus** : tout changement de ce qui est écrit
  dans un `.stf` suit la règle de version de `persistance.md`.

## Hors V1

Compression et somme de contrôle du `.stf`, repli `Unknown` sur les autres
enums du format, signature de code Windows, distribution macOS : ils n'ont pas
de critère ici. Voir #323 pour ce qui peut attendre la V2.
