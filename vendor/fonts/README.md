# Polices embarquées de StokesForge

Trois paires de polices « texte sans-serif + Math », sous licence **SIL Open
Font License 1.1** (vérifiée sur chaque fichier `OFL.txt` ci-dessous, pas
supposée). Embarquées dans le binaire par `crates/stokesforge-render/src/fonts.rs`
via `include_bytes!`, pour que StokesForge sache toujours dessiner du texte lisible
sans dépendre de ce qui est installé sur la machine — voir
`docs/archive/newarchitecture/architecture-choisie.md` §7 pour la décision et sa
date (2026-09-13).

Noto Sans / Noto Sans Math a un double rôle dans ce mécanisme : c'est la
police par défaut (rien demandé ⇒ Noto) et le repli silencieux (police
système demandée introuvable ⇒ Noto). Les deux autres paires ne sont
utilisées que sur demande explicite.

## Pourquoi ces fichiers sont commités tels quels (pas un `lock.env` + fetch)

`vendor/python/` (voir `vendor/python/lock.env` et `.gitignore`) verrouille
une version et télécharge ~300 Mo à la compilation, parce que la distribution
est volumineuse, liée à une plateforme et à une ABI précises (CPython +
numpy + PyO3), et changerait souvent de contenu réel sans changer de
version déclarée. Rien de tout cela ne s'applique ici : six fichiers,
4,5 Mo au total, statiques, indépendants de la plateforme, qui ne changeront
plus une fois choisis (une mise à jour de version serait un choix conscient,
pas un défaut de build). Un mécanisme de fetch aurait ajouté une dépendance
réseau à la compilation pour un gain quasi nul, et aurait cassé la
robustesse hors-ligne que `stokesforge-robustesse` (et la CI) tiennent pour
acquise. D'où le choix : commiter les fichiers directement.

## Provenance

| Répertoire | Fichier | Famille (déclarée dans le fichier) | Source | Version / commit | Récupéré le |
|---|---|---|---|---|---|
| `libertinus-sans/` | `LibertinusSans-Regular.otf` | Libertinus Sans | [alerque/libertinus](https://github.com/alerque/libertinus), release `v7.051` | 7.051 (2024-09-26) | 2026-09-13 |
| `libertinus-math/` | `LibertinusMath-Regular.otf` | Libertinus Math | [alerque/libertinus](https://github.com/alerque/libertinus), release `v7.051` (même archive) | 7.051 (2024-09-26) | 2026-09-13 |
| `fira-sans/` | `FiraSans-Regular.ttf` | Fira Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/firasans) (distribution Google Fonts canonique ; origine Mozilla/Telefónica) | commit `ed7143b8f0c9` (2016-12-02) | 2026-09-13 |
| `fira-math/` | `FiraMath-Regular.otf` | Fira Math | [firamath/firamath](https://github.com/firamath/firamath), release `v0.3.4` | 0.3.4 (2020-10-15, confirmée courante sur [CTAN](https://ctan.org/pkg/firamath)) | 2026-09-13 |
| `noto-sans/` | `NotoSans-Variable.ttf` (renommé depuis `NotoSans[wdth,wght].ttf`) | Noto Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosans) | commit `2984c575fdce` (2024-11-20) | 2026-09-13 |
| `noto-sans/` | `NotoSans-Italic-Variable.ttf` (renommé depuis `NotoSans-Italic[wdth,wght].ttf`) | Noto Sans (italique) | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosans) | commit `2984c575fdce` (2024-11-20) | 2026-10-02 |
| `noto-sans-math/` | `NotoSansMath-Regular.ttf` | Noto Sans Math | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosansmath) | commit `dbd1ab6e65dc` (2024-06-27) | 2026-09-13 |

Chaque répertoire porte son propre `OFL.txt` (texte de licence intégral),
copié depuis la même source que le fichier de police — c'est l'usage
standard de distribution de polices, au même niveau de soin que les licences
vendorisées de `vendor/python/`.
Les scripts de distribution copient chaque texte dans
`licenses/fonts/<famille>/OFL.txt` par `scripts/licenses-dist.sh` (#129).

## Notes

- **Libertinus** : régulier statique choisi (pas la variable) — `alerque/libertinus`
  publie les deux formats dans la même release ; le statique suffit puisque
  StokesForge n'a besoin que du poids régulier.
- **Fira Sans** : statique régulier, pris sur `google/fonts` plutôt que sur le
  dépôt `googlefonts/FiraGFVersion` (archivé, sans release) — `google/fonts`
  est la distribution effectivement publiée et sert la licence à côté.
- **Fira Math** : dernière version stable publiée (`v0.3.4` ; il existe des
  pré-releases `v0.4-beta-*` plus récentes mais explicitement bêta, non
  retenues).
- **Noto Sans** : police variable (axes `wdth`, `wght`) plutôt qu'une coupe
  statique — choisi pour rester sur la distribution `google/fonts`
  canonique, qui sert `OFL.txt` au même endroit ; l'instance par défaut du
  fichier correspond au poids régulier. Renommé `NotoSans-Variable.ttf` pour
  éviter les crochets `[...]` du nom de fichier d'origine dans le dépôt.
- **Noto Sans italique** (issue #63) : Typst ne fabrique pas d'italique ; sans cette face, le bouton « italique » de la barre de format de texte restait sans effet. Le gras, lui, vient de l'axe `wght` de la variable. Fira et Libertinus n'ont que le régulier : gras et italique n'y changent rien.
- **Noto Sans Math** : déjà distribuée en statique régulier par `google/fonts`.

## Mettre à jour une police

1. Vérifier la licence OFL à la nouvelle source (ne pas supposer qu'elle n'a
   pas changé).
2. Remplacer le fichier de police et `OFL.txt` dans le répertoire concerné.
3. Mettre à jour la ligne correspondante de ce tableau (source, version ou
   commit, date de récupération).
4. Si le nom de famille interne au fichier a changé, mettre à jour
   `FontSet::sans_family_name`/`math_family_name` dans
   `crates/stokesforge-render/src/fonts.rs` — un test
   (`family_names_match_the_file_content`) échouera
   sinon.
