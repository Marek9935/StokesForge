# Rapport de tri — retours du 2026-09-23

Quinze remarques en vrac, plus une capture d'OriginLab (barre de menus + barre
d'outils dense en icônes, pour le point 7). Tri d'abord, détail ensuite,
conformément au gabarit `plotyx-bug-report`.

## 0. Ce qui n'a pas pu être vérifié à l'œil

Plusieurs remarques sont purement visuelles (contour de highlight mal
positionné, ombre de fenêtre, largeur de colonne, resize qui déborde). Cette
tâche tournait en arrière-plan, sans capture d'écran possible à la demande et
sans utilisateur à qui la réclamer en cours de route. Les causes ci-dessous
sont donc établies **à partir du code seul** là où c'est indiqué — avec le
risque que cela comporte (§0 du skill) — et marquées « à confirmer à l'écran »
quand le code ne suffit pas à trancher seul.

## 1. Tableau de tri

| # | Remarque (reprise) | Classement | Lot |
|---|---|---|---|
| 1 | Supprimer le bouton « Importer… » et « Ajouter une colonne » du bandeau du classeur | évolution | Lot A — bandeau du classeur |
| 2 | Supprimer l'affichage « N lignes × N colonnes » (bandeau entier disparaît) | évolution | Lot A |
| 3 | Étendre le mécanisme Tab-ajoute-une-colonne à toutes les lignes d'en-tête (pas seulement Formule) | évolution (extension d'un comportement **délibéré**, voir §3.1) | Lot B — Tab en en-tête |
| 10 | Tab en limite de colonne revient à la ligne, sauf sur la ligne F(x)= | même remarque que #3, formulée autrement | Lot B |
| 4 | Menu contextuel « Ajouter une colonne » au clic droit à droite de la dernière colonne | évolution | Lot A (remplace le bouton supprimé en #1) |
| 5 | Clic sur l'en-tête d'une colonne → sélectionne + peut la supprimer ; même menu contextuel qu'à la rangée des lettres | à trancher — l'essentiel existe déjà, voir §3.2 | Lot C — sélection/suppression de colonne |
| 6 | Largeur des colonnes × 4/3 | évolution (réglage simple) | Lot D — ajustements visuels seuls |
| 7 | Panneau d'icônes façon Origin sous « Fichier » (nouveau classeur / graphe / import) | évolution, à préciser | Todo dédiée (§2) |
| 8 | Contour de highlight mal positionné autour des fenêtres | bug non évident, localisé | §3.3 |
| 9 | Fenêtre détachée : ajouter un contour fin, angles arrondis ; le garder aussi en fenêtre principale ; supprimer l'ombre | évolution (style), à préciser | Lot D |
| 11 | Pas de highlight quand une fenêtre est sélectionnée à la souris **dans la fenêtre principale** (dockée) | bug non évident, même famille que #8 | §3.3 |
| 12 | Pas de highlight quand la fenêtre est réduite via « Masquer le contenu de… » | bug non évident, même famille que #8 | §3.3 |
| 13 | Les fenêtres classeur n'ont pas l'icône « Masquer le contenu de… » | bug localisé avec certitude | §3.4 |
| 14 | Resize d'un classeur : les colonnes qui débordent sortent de la fenêtre au lieu d'être tronquées ; ajouter un slider horizontal, Maj+molette pour le piloter | évolution, périmètre à trancher (« revoir toute la logique » — cf. règle du skill sur le périmètre flou) | Todo dédiée (§2) |
| 15 | Zone de prise du glisser-déposer colonne→graphe : 10 px devrait longer la/les lignes **sélectionnées**, pas le bord de la fenêtre | à trancher — c'est le cas déjà documenté dans le skill lui-même, voir §3.5 | isolé |
| 16 | Le glisser-déposer colonne→graphe affiche la légende mais pas la courbe | bug non évident, désaccord entre deux surfaces | §3.6 |

Lots proposés (regroupement par zone de code, pas par ordre d'arrivée) :

- **Lot A** — bandeau du classeur : retirer boutons + compteur (#1, #2),
  les remplacer par le menu contextuel de fin de grille (#4). Un seul
  chantier : le bandeau actuel disparaît complètement à `book_contents`
  (`crates/plotyx-app/src/app.rs:3653`) et sa jumelle détachée
  `detached_book_contents` (`app.rs:5592`) — **les deux écritures**, sans quoi
  la fenêtre détachée garde l'ancien bandeau.
- **Lot B** — Tab en en-tête (#3, #10) : une seule extension de la même
  condition, à deux endroits (`app.rs:3395-3415` dans `header_field_widget`,
  et `app.rs:4460-4484` dans le raccourci clavier global de `grid_contents` —
  vérifier aussi le duplicata détaché signalé en `app.rs:5453-5484`).
- **Lot C** — sélection/suppression de colonne (#5), voir §3.2 : ce qui reste
  réellement à faire est plus étroit que la remarque ne le suggère.
- **Lot D** — ajustements visuels sans logique nouvelle (#6, #9) : largeur de
  colonne, contour/ombre de fenêtre. Bas risque, à grouper avec le lot qui
  touchera `window_header`/`floating_resize_handles` pour #8/#11/#12/#13 si
  possible, pour ne pas repeindre deux fois la même zone.

## 2. Demandes d'évolution — todo (reformulée fidèlement, non conçue ici)

- [ ] **Bandeau du classeur** : supprimer le bouton « Importer… », le bouton
  « Ajouter une colonne » et l'affichage « N lignes × N colonnes » de
  `book_contents` et `detached_book_contents`. *(#1, #2 — Lot A)*
- [ ] **Menu contextuel de fin de grille** : au clic droit dans la zone vide
  à droite de la dernière colonne, proposer « Ajouter une colonne ».
  Remplace le bouton supprimé ci-dessus. *(#4 — Lot A)*
- [ ] **Tab en en-tête, toutes lignes** : à la dernière colonne, Tab doit
  ajouter une colonne depuis n'importe quelle ligne d'en-tête (Long Name,
  Units, Comments), pas seulement Formule. *Point d'attention* : le
  comportement actuel est documenté comme volontaire
  (`app.rs:4459-4467`, commentaire « seul Formule marque la vraie fin de
  l'en-tête ») — l'extension change ce choix, pas un oubli à réparer.
  *(#3, #10 — Lot B)*
- [ ] **Panneau d'icônes façon Origin** sous la barre de menus, avec des
  raccourcis illustrés (icônes) pour : nouveau classeur, nouveau graphe,
  importer un fichier. La capture jointe montre la référence OriginLab
  (barre d'outils dense, sections séparées par des poignées). *Précisions
  à obtenir avant conception (plotyx-feature)* : liste exacte des actions à
  inclure au-delà des trois citées, jeu d'icônes (existe-t-il déjà des
  glyphes utilisés ailleurs dans `plotyx-app`, ou faut-il en choisir ?),
  et si ce panneau doit être permanent ou repliable. *(#7)*
- [ ] **Largeur des colonnes** : l'augmenter d'un tiers (× 4/3) par rapport
  à la largeur actuelle. *(#6 — Lot D)*
- [ ] **Fenêtre détachée** : ajouter un contour — trait gris fin, angles
  légèrement arrondis — et appliquer le même contour à la fenêtre
  principale ; supprimer l'effet d'ombre actuel. *(#9 — Lot D)*
- [ ] **Resize des classeurs** : quand la fenêtre rétrécit, tronquer les
  colonnes qui dépassent au lieu de les laisser déborder hors fenêtre ;
  ajouter un slider horizontal, avec Maj+molette pour le déplacer. *Portée
  à confirmer avant de chiffrer* (voir §4, question 3) : la remarque dit
  « toute la logique de resize des workbook est à revoir », ce qui peut
  vouloir dire soit corriger ce seul débordement, soit une refonte plus
  large du redimensionnement de grille — l'écart de coût est important.
  *(#14)*

## 3. Bugs non évidents — détail

### 3.1 — Tab en en-tête ne prolonge pas au-delà de la ligne Formule (#3, #10)

**Ce n'est pas un bug.** `app.rs:4459-4467` documente explicitement la règle :
Tab sur la dernière colonne ajoute une colonne uniquement depuis une cellule
de données ou depuis le champ Formule (dernier des quatre champs
d'en-tête) ; les trois autres champs (Long Name, Units, Comments) font
circuler Tab colonne par colonne sur le même champ, y compris en dernière
colonne, par choix. Reclassé en évolution — §2.

### 3.2 — Sélection/suppression depuis l'en-tête (#5)

Vérifié dans `column_header_widget` (`app.rs:3095` et suivantes) : la rangée
des lettres (« # ») **sélectionne déjà** la colonne entière au clic (simple,
Maj-étend, glisser), et son menu contextuel appelle déjà `grid_edit_menu` —
le même bloc d'édition partagé par les cellules de données et les champs
d'en-tête (voir le commentaire de tête de `grid_edit_menu`, `app.rs:3145` :
« le même bloc, dans le même ordre, sur les trois zones »). Si `grid_edit_menu`
porte déjà une entrée de suppression de colonne, alors une bonne partie de la
demande est déjà satisfaite pour la rangée des lettres — reste à vérifier à
l'écran ce qui manque précisément :

- Est-ce que `header_field_widget` (les 4 lignes de nom/unité/commentaire/
  formule, `app.rs:3350`) sélectionne bien **la colonne entière** au clic
  simple (pas seulement le champ), pour ouvrir le même menu que la rangée
  des lettres ? Le commentaire à `app.rs:3423` mentionne un champ « surligné
  si ce champ précis est sélectionné, ou si c'est toute la colonne qui
  l'est » — donc la lecture de sélection existe, mais l'**écriture** (le
  clic sur un champ d'en-tête sélectionne-t-il la colonne, ou seulement ce
  champ ?) reste à vérifier.
- La demande « une fois une colonne entière sélectionnée, le menu contextuel
  doit être le même peu importe où on clique droit (en gardant celui de
  l'en-tête) » est probablement déjà vraie via `grid_edit_menu` partagé — à
  confirmer par un test manuel plutôt qu'un correctif à l'aveugle.

**Portée** : `crates/plotyx-app/src/app.rs:3095-3260` (`column_header_widget`),
`app.rs:3350-3494` (`header_field_widget`), `app.rs:3145` (`grid_edit_menu`).

### 3.3 — Highlight de fenêtre : positionnement et cas manquants (#8, #11, #12)

Localisé : le highlight de fenêtre active existe **uniquement pour les
fenêtres flottantes/détachées**, à `app.rs:1925-1937` :

```rust
if is_active {
    // Surlignage de la fenêtre au premier plan (§5)…
    ui.painter().rect_stroke(
        ui.max_rect(),
        0.0,
        ui.visuals().selection.stroke,
        egui::StrokeKind::Outside,
    );
}
```

Trois observations qui expliquent les trois remarques :

- Ce bloc tourne uniquement dans la boucle des fenêtres **flottantes**
  (`egui::Window` avec `.title_bar(false)` etc., autour de `app.rs:1908-1945`
  ). Aucune trace équivalente pour les fenêtres **dockées** dans la zone
  principale — ce qui correspond exactement à #11 (« manque la logique »,
  pas « ça ne se déclenche pas » : `rg "is_active" crates/plotyx-app/src/app.rs`
  ne montre pas d'occurrence côté zone dockée).
- `ui.max_rect()` est demandé **avant** que `window_header` ait posé la barre
  de titre custom (`.title_bar(false)`, gérée à la main juste après) — le
  contour peut donc inclure ou exclure la barre de titre selon comment egui
  calcule `max_rect()` à ce stade, ce qui est la piste la plus probable pour
  #8. À confirmer à l'écran : je n'ai pas pu observer si le contour déborde,
  est en retrait, ou coupe la barre de titre — un raisonnement depuis le
  code seul ne suffit pas ici (§0 du skill).
- Rien dans ce bloc ne dépend de `was_collapsed` : à première lecture le
  contour devrait continuer à s'afficher (autour d'une fenêtre réduite plus
  petite) plutôt que disparaître — donc #12 n'est probablement pas « le
  highlight disparaît » mais plutôt un sous-cas de #11/#8 à vérifier à
  l'écran une fois la fenêtre réduite.

**Portée** : `crates/plotyx-app/src/app.rs:1908-1945` (highlight flottant,
existant) ; absence confirmée côté dock (grep négatif) ; interaction avec
`window_header`/`floating_resize_handles` (`app.rs:2233`, `app.rs:2467`) à
vérifier pour #8.
**Nature** : incohérence entre surfaces (dock vs détaché) + positionnement à
confirmer visuellement.
**Écarté** : pas de plafond, pas de limite de banc documentée pour ce point.
**Gravité** : cosmétique.

### 3.4 — Icône « Masquer le contenu de… » absente sur les classeurs (#13)

Cause certaine, sans ambiguïté :

```rust
let collapse_width = if matches!(tab, Tab::Graph(_)) {
    COLLAPSE_WIDTH
} else {
    0.0
};
```

`window_header` (`app.rs:2486-2492`) réserve l'icône de repli explicitement
aux `Tab::Graph`, quel que soit le `HeaderPlacement`. `Tab::Book` en est donc
exclu par construction — pas une régression, une restriction jamais levée
depuis que ce mécanisme existe. Le repli lui-même (`toggle_graph_collapse`,
`collapsed_graph_tabs: HashSet<Tab>`, `app.rs:2126`) est générique sur `Tab`
et ne semble pas intrinsèquement lié aux graphes — mais je n'ai pas vérifié
ce que `book_contents`/`detached_book_contents` font quand un classeur est
dans `collapsed_graph_tabs` (la peinture du contenu replié n'a été vérifiée
que côté graphe). C'est le point à trancher avant de lever la restriction :
le mécanisme de repli du contenu lui-même gère-t-il déjà un classeur, ou
seulement l'icône est-elle manquante ?

**Portée** : `crates/plotyx-app/src/app.rs:2486-2492` (`window_header`) ;
`app.rs:2126-2151` (`toggle_graph_collapse`/`toggle_graph_collapse_to`).
**Nature** : incohérence entre surfaces (graphe vs classeur).
**Écarté** : aucune décision documentée n'exclut les classeurs du repli — pas
de commentaire justifiant cette restriction, contrairement au cas §3.1/§3.5.
**Gravité** : gênant.

### 3.5 — Zone de prise du glisser-déposer colonne→graphe (#15) — à trancher

C'est très exactement le cas de figure que le skill `plotyx-bug-report`
utilise lui-même comme exemple canonique de « ne pas trancher seul » (§2,
nuance) : la zone de prise (`COLUMN_GRIP_WIDTH = 10.0`, `app.rs:514`) est
confinée au bord *extérieur* de la grille entière, et le commentaire
(`app.rs:4308-4321`) explique que couvrir tout `grid_rect` écraserait le
glisser d'extension de sélection de `cell_widget`. L'utilisateur redemande
maintenant précisément la troisième option que ce commentaire n'examine
pas : une bande de 10 px autour de la ou des lignes **sélectionnées**,
plutôt qu'au bord de la fenêtre.

Le commentaire ne rejette pas cette option — il rejette seulement
« `ui.interact` sur tout `grid_rect` ». Une bande limitée aux lignes
sélectionnées reste, en première lecture, compatible avec la contrainte
énoncée (elle ne couvre pas la grille entière). Mais c'est un changement de
comportement, pas une simple réparation, et il touche un geste déjà
fonctionnel (le glisser d'extension de sélection) : je ne tranche pas seul —
c'est un des points à poser à l'utilisateur (§4).

**Portée** : `crates/plotyx-app/src/app.rs:514` (`COLUMN_GRIP_WIDTH`),
`app.rs:4304-4340` (zone de prise actuelle).
**Écarté** : ni un plafond, ni une limite de banc — une décision de tracé de
zone d'interaction, dont l'alternative proposée par l'utilisateur diffère de
celle déjà rejetée dans le commentaire.

### 3.6 — Glisser-déposer colonne→graphe : légende affichée, courbe absente (#16)

Désaccord entre deux surfaces qui auraient dû partager un seul chemin
(légende vs tracé) — famille de symptôme la plus révélatrice d'après le
skill. Je n'ai pas retracé le code de résolution de série jusqu'au bout dans
cette passe (effort porté sur les quinze autres points) : à investiguer en
priorité, en partant de l'endroit où la légende obtient son libellé de série
d'un côté, et où le `PlotSpec`/la série est construite de l'autre — ces deux
chemins sont probablement déjà proches dans `plotyx-app`, cf. la
recommandation du skill de chercher un `Command`/`Action` qui existe déjà
plutôt que d'assumer qu'il manque.

**Portée** : non localisée précisément — point de départ recommandé,
`rg "column_drag_payload|drop.*graph|drag.*graph" crates/plotyx-app/src/app.rs`.
**Nature** : incohérence entre surfaces (légende vs tracé).
**Écarté** : pas de dégradation volontaire connue pour ce cas — rien dans
`docs/invariants.md` §10 ne couvre le glisser-déposer.
**Gravité** : gênant (fonctionnalité visiblement à moitié câblée).

## 4. Points à trancher avant d'enchaîner sur un correctif ou une conception

1. **#15 — zone de prise du glisser-déposer.** L'utilisateur veut une bande
   de 10 px autour de la sélection de lignes plutôt qu'au bord de la
   fenêtre. C'est faisable sans rouvrir le conflit déjà écarté dans le
   commentaire (`ui.interact` sur tout `grid_rect`), mais ça change un geste
   qui fonctionne aujourd'hui (le glisser d'extension de sélection sur les
   bords de la grille) : la zone de prise se retrouverait à un endroit où
   l'utilisateur peut aussi vouloir étendre sa sélection avec la souris.
   Faut-il ce compromis, ou une autre zone de prise (ex. une poignée dédiée
   au coin de la sélection) ?
2. **#14 — resize des classeurs.** La remarque dit « toute la logique de
   resize... est à revoir » : un correctif borné (tronquer les colonnes qui
   débordent + slider horizontal) est un chantier d'une taille ; une refonte
   plus large du redimensionnement de grille en est une autre, nettement
   plus coûteuse. Le tronquage + slider suffit-il, ou y a-t-il d'autres
   symptômes de resize non listés ici ?
3. **#13 — repli des classeurs.** Avant de lever la restriction
   `matches!(tab, Tab::Graph(_))`, faut-il vérifier que le mécanisme de
   contenu replié (peinture, geometrie) gère déjà un classeur, ou est-ce
   à construire aussi ? (Question technique à trancher côté implémentation,
   pas côté utilisateur — mentionnée ici pour mémoire, pas à poser tel quel.)

Aucune de ces trois questions n'a été tranchée dans ce rapport, conformément
au skill : diagnostiquer et documenter, pas décider à la place de
l'utilisateur.
