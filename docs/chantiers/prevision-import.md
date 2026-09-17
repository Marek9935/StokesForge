# Cahier des charges — chantier 2 : prévisualisation d'import façon Origin

## Objectif

**Le point important de ce chantier, et le seul qui ne doit pas se perdre en
route** : pendant que l'utilisateur règle les paramètres d'import, il doit
voir **en temps réel comment le fichier est effectivement découpé** —
quelles lignes sont de l'en-tête, lesquelles alimentent quel rôle de
colonne, où commencent les données — pas seulement une fois l'import validé.
Tout le reste de ce document (widgets précis, rôles nommés comme chez
OriginLab, options de configuration détaillées) est secondaire à ce
principe et négociable ; ce principe ne l'est pas.

Concrètement : refaire le panneau de prévisualisation de la fenêtre d'import
(`import_window`, `crates/plotyx-app/src/app.rs`) pour qu'il affiche ce
découpage en direct, à mesure que l'utilisateur ajuste les paramètres
d'import, sur le modèle de l'assistant d'import ASCII d'OriginLab.

C'est le point le plus important des deux chantiers notés le 2026-09-15 : il
englobe et corrige au passage le bug connu du champ de texte du module
d'import (impossible d'y supprimer du texte sans d'abord le sélectionner),
puisque cette zone est entièrement refaite ici.

## Référence : capture de l'assistant d'import ASCII d'OriginLab

Capture fournie par l'utilisateur le 2026-09-15 (assistant d'import ASCII
d'OriginLab), décrite ci-dessous faute de pouvoir l'intégrer telle quelle à
la documentation versionnée.

Le panneau qu'elle montre se découpe en trois zones :

1. **Comptage des lignes d'en-tête**, en haut :
   - `Number of main header lines (exclude subheader lines)` — nombre de
     lignes d'en-tête « principal », dans un menu déroulant, avec un bouton
     de réévaluation automatique à côté (icône de cycle) et une case
     `Line number start from bottom` pour compter depuis la fin du fichier.
   - `Number of subheader lines` — nombre de lignes de « sous-en-tête »
     (celles qui portent les métadonnées de colonne : nom long, unité,
     commentaire…), même genre de menu déroulant + bouton de réévaluation.
   - `Auto determine header lines` — case à cocher qui laisse l'outil
     déduire ces deux comptes du contenu du fichier plutôt que de les saisir
     à la main.

2. **Assignation des rôles de ligne** (« Column Label Assignment from
   Subheader Lines ») : pour chacun des rôles `Long Names`, `Units`,
   `Comments`, `System Parameters`, `User Parameters`, une paire de menus
   déroulants « from … to … » qui choisit **quelle plage de lignes de
   sous-en-tête** alimente ce rôle (une seule ligne dans la capture : `18` à
   `18` pour `Comments`, par exemple), chacune avec son propre bouton de
   réévaluation. Une case `Extract Long Names and Units from Same Line`
   permet de lire les deux rôles sur une ligne partagée plutôt que sur deux
   lignes séparées. À côté, `Characters to skip on each line` retire un
   préfixe fixe de chaque ligne de données avant parsing.

3. **Aperçu texte en direct**, en bas, avec deux réglages qui le pilotent
   (`Preview Font`, `Preview Lines` — nombre de lignes affichées) : le
   contenu brut du fichier, numéroté, avec pour chaque ligne un **code de
   préfixe** qui dit comment cette ligne va être interprétée avec les
   réglages actuels — légende donnée en toutes lettres au-dessus du panneau :
   `S=Short Name, L=Long Name, U=Units, P=Parameters, C=Comment,
   MH=Main Header, SH=Subheader`. Les lignes sont colorées selon leur rôle
   (bleu pour les rôles d'en-tête reconnus, magenta pour un commentaire,
   gris pour les lignes de données pas encore atteintes par `Preview
   Lines`…). **Tout changement dans les deux zones du dessus retexte cet
   aperçu immédiatement**, sans étape de validation intermédiaire — c'est le
   comportement central à reproduire, plus que tel ou tel widget précis.

## Ce qui existe déjà côté PlotYx (point de départ, pas à rejeter en bloc)

- `crates/plotyx-import` : crate pur (zéro dépendance UI/core), qui modélise
  déjà une **recette** (`ImportRecipe`) inspirée du même assistant OriginLab
  — voir le commentaire de tête de `crates/plotyx-import/src/lib.rs`. Champs
  actuels : `separator` (`Separator`), `decimal` (`DecimalStyle`), `header`
  (`HeaderSkip` : aucun saut, `N` lignes fixes, ou jusqu'à un marqueur),
  `footer_skip_lines`, `has_column_header_row`, `name_source` (nom de fichier
  ou clé lue en en-tête).
- `crates/plotyx-core/src/column.rs` : `ColumnMeta { unit, long_name,
  comment }` existe déjà par colonne — les rôles `Long Names`/`Units`/
  `Comments` d'OriginLab ont donc une destination naturelle dans le modèle
  actuel. Rien d'équivalent à `System Parameters`/`User Parameters`
  d'OriginLab n'existe : à trancher (les ignorer, ou les faire atterrir dans
  `comment` faute de mieux — voir « Questions ouvertes »).
- `crates/plotyx-app/src/app.rs`, fonction `import_window` : la fenêtre
  d'import actuelle (dialogue transitoire tant que `AppState::import_state`
  est `Some`), avec ses champs de texte simples (dont le bug noté) et pas
  d'aperçu en direct du découpage.

L'écart principal avec OriginLab : le modèle actuel de `HeaderSkip` traite
l'en-tête comme un bloc à sauter, sans distinguer un « en-tête principal »
d'un « sous-en-tête », et sans assigner des **plages de lignes** à des rôles
sémantiques par colonne — c'est tout l'apport attendu de ce chantier.

## Fonctionnalités attendues

1. **Deux compteurs de lignes d'en-tête** distincts (principal / sous-en-tête)
   avec option de comptage depuis le bas et une détection automatique
   raisonnable (heuristique à documenter — ex. première ligne où le nombre
   de champs façon `separator` redevient stable et numérique).
2. **Assignation de plages de sous-en-tête aux rôles** `long_name`, `unit`,
   `comment` de `ColumnMeta` (plus, si retenu, un traitement pour les
   paramètres système/utilisateur d'OriginLab — voir questions ouvertes),
   avec une option pour lire nom long et unité sur une même ligne.
3. **Nombre de caractères à retirer en tête de chaque ligne** avant parsing
   (utile pour les préfixes fixes de certains formats d'instrument).
4. **Aperçu en direct, texte brut coloré/annoté par rôle de ligne**,
   recalculé à chaque changement de paramètre, avec un nombre de lignes
   affichées configurable. Doit rester utilisable sur de gros fichiers (pas
   de re-parsing complet coûteux à chaque frappe — voir « Points de
   vigilance »).
5. **Correction du bug d'édition de texte** : tous les champs de texte du
   panneau (nom de recette, marqueur d'en-tête, clé d'en-tête…) doivent
   permettre suppression/insertion normales au clavier, pas seulement un
   remplacement par sélection.

## Ce qui ne doit *pas* être fait ici

- Ne pas toucher au chantier 1 (colonnes à types mixtes) au-delà de
  consommer son résultat : ce panneau doit refléter dans son aperçu le
  typage par cellule maintenant que ce chantier est mergé (une valeur non
  numérique dans une colonne par ailleurs numérique doit s'annoter dans
  l'aperçu, pas faire échouer le parsing).
- Ne pas copier le vocabulaire d'OriginLab tel quel si un rôle n'a pas
  d'équivalent dans le modèle PlotYx (`System Parameters`/`User Parameters`
  notamment) sans décision explicite — voir ci-dessous.
- Rien de tout ceci ne passe par une commande annulable de
  `plotyx-commands` **tant que l'import n'est pas validé** : le panneau de
  prévisualisation travaille sur un état local transitoire de la fenêtre
  d'import, comme aujourd'hui — seule l'application finale du résultat au
  classeur doit passer par une commande (c'est déjà le cas, à ne pas
  régresser).

## Questions ouvertes à trancher avant/pendant l'implémentation

- **`System Parameters`/`User Parameters` d'OriginLab** : les ignorer (pas
  de champ équivalent dans `ColumnMeta`), les fondre dans `comment`, ou
  étendre `ColumnMeta` ? Impact sur le format `.plx` si extension — lire
  `docs/persistance.md` avant de trancher.
- **Coût de l'aperçu en direct** sur un gros fichier (des dizaines de
  milliers de lignes) : reparser tout le fichier à chaque frappe dans un
  champ, ou seulement la fenêtre visible (`Preview Lines`) + les lignes
  d'en-tête ? OriginLab semble limiter explicitement via `Preview Lines`.
- **Détection automatique des lignes d'en-tête** (`Auto determine header
  lines`) : heuristique à définir et à documenter, avec ses limites
  (`docs/archive/rustrewrite/questions-ouvertes.md` §11 recense déjà des
  plafonds/heuristiques similaires ailleurs dans le projet — vérifier qu'un
  comportement de bord qui semble étrange n'est pas déjà un choix assumé).
- **Persistance de ces nouveaux réglages dans `ImportRecipe`** : rétro-
  compatibilité du format JSON des recettes déjà enregistrées par
  `RecipeStore` (`crates/plotyx-import/src/store.rs`) à vérifier.
