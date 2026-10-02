# Icônes de l'interface

Les icônes de `stokesforge-app` sont des **Phosphor Icons, variante duotone**
(licence MIT, copiée dans `crates/stokesforge-app/assets/icons/LICENSE`). Le code
qui les affiche est `crates/stokesforge-app/src/icons.rs` ; son `//!` dit le
*pourquoi*, cette page dit le *quoi* et le *comment ajouter*.

## Quand mettre une icône — et quand non

- **Oui** : une action répétée dont la forme se reconnaît d'un coup d'œil
  (fermer, détacher, maximiser, replier, défiler les onglets), et le **type
  d'objet** du projet (dossier, classeur, graphe, matrice, note) partout où il
  apparaît.
- **Non** : là où un mot est plus clair (« # » de la gouttière, un nom de
  colonne, un résumé de métadonnées) ; là où l'icône ne ferait que décorer.
  Pas d'icône dans chaque menu par principe : une icône qui ne distingue
  rien est du bruit.
- **Toujours une info-bulle** : une icône seule ne dit pas ce qu'elle fait.
  Le texte vient de `tr!` (voir `stokesforge-textes-interface`) ; `icon_button`
  l'exige.
- **Jamais un emoji ni un glyphe Unicode** (« × », « ↗ », « 📁 ») comme icône.

## Couleur : neutre par défaut, couleur rare

Le trait est foncé et commun ; la couleur ne vient que du **rôle** (`IconRole`),
et seulement quand elle porte une information que la forme ne porte pas :

| Rôle | Pour | Teinte |
|---|---|---|
| `Neutral` | tout le reste (défaut) | gris-bleu foncé |
| `Workbook`, `Graph`, `Matrix`, `Note`, `Folder` | l'objet du projet de ce type | jaune, bleu, violet, orange, sable |
| `Danger` | l'action destructive d'un contexte | rouge |

Budget indicatif : pas plus d'environ une icône colorée sur quatre dans une
même vue. Pas de couleur par famille d'outils. Le thème de l'application est
clair et fixe (`theme.rs`) : pas de variante sombre.

Le duotone Phosphor a deux calques (fond à 20 % d'opacité, trait
`currentColor`). `Image::tint` multiplie les pixels et ne sait pas les
recolorer séparément : **chaque variante (icône, rôle) est cuite dans le
SVG** par `scripts/icons-build.sh`.

## Ajouter une icône

1. Chercher sur <https://phosphoricons.com/?weight=duotone> ; le nom du fichier
   est celui de `vendor/phosphor-icons/SVGs/duotone/<nom>-duotone.svg`
   (`vendor/` n'est **pas** versionné).
2. `scripts/icons-build.sh --add <nom>` copie le SVG dans `assets/icons/src/`.
3. Ajouter une ligne à `assets/icons/icons.tsv` (`nom<TAB>rôle[,rôle]`) ; un
   rôle nouveau va dans `palette.tsv` (trait, fond), après accord de
   l'utilisateur.
4. `scripts/icons-build.sh --sheet` cuit les variantes
   (`assets/icons/<nom>.<rôle>.svg`) et la **planche-contact**
   (`target/icons/sheet-color.png` et `sheet-gray.png`, via Inkscape). La
   regarder : en niveaux de gris, les formes doivent rester distinctes.
5. Dans `icons.rs` : une variante de `Icon` et ses lignes de `VARIANTS`.
   Le test `le_tableau_des_variantes_suit_icons_tsv` échoue si l'une manque.
6. Les variantes cuites et `src/` sont versionnées ; commiter les deux.

## Garde-fous

- `aucune_icone_hors_du_composant` : aucun module hors `icons.rs` ne cite
  `assets/icons` ni un URI d'icône.
- `chaque_icone_se_charge` : toute variante se rastérise avec le vrai chargeur.
- Largeur : `ICON_SIZE` (16 pt) est la seule taille ; la bande d'une icône
  d'en-tête de fenêtre est `HEADER_ACTION_WIDTH` (`app.rs`, #50).
- Dans un test d'interface, une icône peinte est un `Shape::Rect` à texture ;
  installer `egui_extras::install_image_loaders` et peindre **deux images**
  (la texture est prête à la seconde) — voir `find_icon_centers`.

## Perf

SVG embarqués, rastérisés à la demande par `egui_extras` (resvg) une fois par
taille × `pixels_per_point`, puis gardés en texture : seules les icônes
affichées coûtent.

La réinitialisation du zoom (#74) utilise `arrows-out` : quatre flèches vers
les limites du cadrage, distinctes des coins de maximisation. « New Project »
(#73) utilise `file-plus` ; le nouveau graphique garde le rôle `Graph`.

Object Manager (#82) réutilise l'icône `Graph` en rôle neutre pour son bouton
de barre : son infobulle porte le nom du panneau. Ancrer, faire flotter et
fermer réutilisent `Reattach`, `Detach` et `Close` ; aucune icône ajoutée.
