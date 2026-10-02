# Rapport de tri — retours workbook / fenêtres / drag&drop

Contexte : lot de retours en vrac (correctifs + demandes de fonctionnalité) sur
l'app `stokesforge-app` (branche `rewriterust`), classés et regroupés ci-dessous
par sujet, dans un ordre qui suit les dépendances (le chrome de fenêtre avant
le highlight qui en dépend, le mécanisme tab avant le menu contextuel qui le
complète, etc.). Fichiers de référence repérés dans le dépôt :
`crates/stokesforge-app/src/app.rs` (6606 lignes, contient le rendu des fenêtres
MDI/détachées, `floating_resize_handles`, `detached_resize_handles`,
`COLUMN_GRIP_WIDTH`) et `crates/stokesforge-app/src/state.rs` (état, dont
`masquer_et_afficher_agissent_sur_toute_la_selection` déjà testé pour le repli
de fenêtre).

Aucun code n'a été modifié : ce document est uniquement le tri demandé.

---

## Groupe 1 — Bandeau du classeur (workbook) : suppression et remplacement par le mécanisme tab/menu contextuel

Ces points forment un seul chantier cohérent : supprimer le bandeau actuel
(boutons + compteurs) et le remplacer intégralement par les mécanismes
d'ajout/suppression de colonne déjà en germe (tab, menu contextuel). Les
traiter séparément romprait le comportement le temps de la transition.

1. **Supprimer le bouton « Importer » et le bouton « Ajouter une colonne »**
   de la fenêtre workbook.
2. **Supprimer l'affichage du nombre de colonnes et de lignes** dans le
   workbook — conséquence directe du point 1 : tout le bandeau disparaît.
3. **Étendre le mécanisme tab d'ajout de colonne à l'en-tête.** Il existe déjà
   pour le corps de la grille (taber en fin de ligne ajoute une colonne) mais
   pas depuis l'en-tête.
4. **Corriger le bug du tab en limite de colonne** : tab en fin de ligne fait
   aujourd'hui un retour à la ligne au lieu d'ajouter une colonne, *sauf*
   quand on est dans la dernière ligne de l'en-tête (`F(x)=`), où ça marche.
   Ce point est un bug dans le même mécanisme que le point 3 — à corriger en
   même temps, pas comme un ticket séparé, sous peine de refaire deux fois le
   même chemin de code.
5. **Ajouter un menu contextuel clic droit** à droite de la dernière colonne,
   avec l'entrée « Ajouter une colonne ».
6. **Ajouter au clic sur l'en-tête d'une colonne la possibilité de supprimer
   cette colonne.** Une fois qu'une colonne entière est sélectionnée (que la
   sélection parte de l'en-tête `#` ou d'ailleurs dans la colonne), le menu
   contextuel doit être identique — en gardant celui actuellement affiché
   depuis l'en-tête comme référence.

→ Le point 4 (bug tab) doit être résolu *avant* ou *avec* le point 3
(extension à l'en-tête), sinon l'extension hérite du même défaut.

---

## Groupe 2 — Largeur des colonnes

7. **Augmenter la largeur des colonnes de 1/3.** Correctif isolé, sans
   dépendance avec le reste (probablement `TableColumn::initial(110.0)` ou
   équivalent, vu dans `app.rs` autour de la ligne 1485 — largeur par défaut
   des colonnes de grille).

---

## Groupe 3 — Chrome des fenêtres (contour, ombre, highlight)

Ces quatre points touchent tous au même sous-système visuel — le cadre/le
contour des fenêtres MDI et détachées — et doivent être traités comme un seul
chantier : changer le contour sans revoir le highlight qui s'appuie dessus
laisserait le highlight cassé une deuxième fois.

8. **Le contour de highlight n'est pas bien positionné** autour des
   fenêtres — trouver une autre manière de le poser proprement (a priori pas
   au-dessus du rendu actuel mais lié à la géométrie réelle de la fenêtre,
   MDI ou détachée).
9. **Ajouter un contour à la fenêtre détachée** (le garder aussi sur la
   fenêtre principale) et **supprimer l'effet d'ombre** : un léger trait gris
   avec des angles légèrement arrondis suffit. Voir `detached_resize_handles`
   et le rendu de bordure côté MDI (`floating_resize_handles`,
   `app.rs:2233` et `app.rs:2374`) comme points d'entrée probables.
10. **Highlight manquant quand une fenêtre est sélectionnée à la souris**
    directement dans la fenêtre principale (MDI).
11. **Highlight manquant quand une fenêtre est réduite** via « Masquer le
    contenu de… ».

→ Ordre logique : 9 (redéfinir le contour/l'ombre) puis 8 (recaler le
highlight sur ce nouveau contour) puis 10 et 11 (brancher le highlight sur les
deux états où il manque aujourd'hui — sélection directe et repli). Un test
existe déjà côté repli (`masquer_et_afficher_agissent_sur_toute_la_selection`
dans `app.rs`) : vérifier qu'il couvre bien l'état visuel avant de considérer
11 comme acquis, ou l'étendre.

---

## Groupe 4 — Icône « masquer le contenu » sur les fenêtres workbook

12. **Il manque l'icône « masquer le contenu de… »** sur les fenêtres
    workbook (elle existe apparemment ailleurs, ex. graph, d'après
    `collapsed_graph_tabs` repéré dans `app.rs:2137`). Rattaché au groupe 3
    dans la mesure où le point 11 (highlight en repli) n'a de sens visible que
    si cette icône existe aussi sur workbook — mais l'ajout de l'icône
    lui-même est un correctif indépendant et local (probablement une simple
    extension de ce qui existe déjà pour les fenêtres de graphe).

---

## Groupe 5 — Redimensionnement du workbook (troncature + slider horizontal)

13. **Revoir toute la logique de resize des workbook.** Aujourd'hui, réduire
    la fenêtre ne tronque pas les colonnes qui dépassent : elles continuent à
    s'afficher en dehors de la fenêtre. Il faut :
    - tronquer l'affichage des colonnes à la largeur de la fenêtre,
    - ajouter un slider horizontal (slider x),
    - câbler Maj+molette sur ce slider pour permettre le défilement
      horizontal à la molette.
    Un seul chantier : la troncature et le slider sont deux faces du même
    problème (sans troncature du viewport, le slider n'a rien à faire
    défiler ; sans slider, la troncature rend les colonnes en trop
    inaccessibles).

---

## Groupe 6 — Drag & drop colonne → graphe

14. **La zone de détection du drag & drop (limite de 10 px) est mal placée** :
    elle est actuellement en bordure de fenêtre, alors qu'elle devrait être
    autour de la ou des lignes sélectionnées. Voir `COLUMN_GRIP_WIDTH: f32 =
    10.0` dans `app.rs:514` — probablement la même constante ou une logique
    voisine à corriger.
15. **Le drag & drop n'affiche pas la courbe sur le graphe**, alors que la
    légende s'affiche bien — signe que le tracé de la série et
    l'alimentation de la légende empruntent deux chemins distincts dans le
    pipeline de rendu, et que seul le second est branché correctement lors
    d'un drop.

→ Ces deux points sont liés par leur cause commune probable (mauvais calage
de la zone active *et* mise à jour incomplète du graphe cible au drop) mais
peuvent être corrigés indépendamment si le diagnostic le confirme ; à
regrouper au moins dans le même ticket d'investigation initiale.

---

## Feature à part — panneau de raccourcis façon Origin

16. **Créer un petit panneau de raccourcis** (façon OriginLab, capture jointe :
    barre de menus + barre d'icônes dense juste en dessous, sections séparées
    par des poignées verticales) placé sous la barre de menu « Fichier »,
    avec au minimum trois icônes : créer un workbook, créer un graphe,
    importer un fichier.

C'est une fonctionnalité nouvelle, indépendante des correctifs ci-dessus
(elle peut réutiliser les actions déjà câblées ailleurs — les mêmes qui
alimentent aujourd'hui les entrées de menu « Nouveau classeur », « Nouveau
graphe », « Importer »). Cependant, comme le groupe 1 supprime le bouton
« Importer » et le bouton « Ajouter une colonne » du bandeau workbook, ce
nouveau panneau devient le remplaçant naturel pour l'action d'import globale
(pas l'ajout de colonne, qui reste porté par le tab/menu contextuel du
groupe 1) : à séquencer après le groupe 1 pour ne pas faire disparaître
l'import de l'interface entre les deux chantiers.

### TODO — panneau de raccourcis (Origin-like)

- [ ] Choisir l'emplacement exact (bande fixe sous la barre de menu,
      largeur pleine fenêtre) et la hauteur cible.
- [ ] Définir le jeu d'icônes minimal : nouveau workbook, nouveau graphe,
      importer un fichier (extensible plus tard à d'autres actions, comme
      chez Origin, mais ne pas sur-scoper au-delà de la demande).
- [ ] Réutiliser les actions existantes (celles des entrées de menu
      correspondantes) plutôt que dupliquer la logique de création/import.
- [ ] Prévoir des tooltips (nom de l'action) puisque les icônes seules sont
      peu explicites au premier lancement.
- [ ] Vérifier l'accessibilité clavier/focus egui du nouveau panneau, dans
      la continuité des raccourcis déjà documentés côté `application.md`.
- [ ] Séquencer après le groupe 1 (retrait du bouton « Importer » du
      bandeau workbook) pour ne pas laisser l'import sans point d'entrée UI
      entre les deux livraisons.
- [ ] Mettre à jour `docs/application.md` si cette doc décrit la disposition
      actuelle de la fenêtre principale.

---

## Résumé de l'ordre de traitement suggéré

1. Groupe 3 (chrome : contour + ombre) → 2. Groupe 3 (highlight, 3 sous-cas)
   → 3. Groupe 4 (icône masquer sur workbook, dépend du chrome/highlight)
2. Groupe 1 (bandeau workbook + tab en-tête + menu contextuel colonnes),
   avec le bug tab (point 4) corrigé dans la même passe que l'extension
   (point 3)
3. Groupe 16 (panneau de raccourcis), après le groupe 1 pour ne pas perdre
   l'accès à l'import entre-temps
4. Groupe 5 (resize workbook + slider x) — indépendant, peut être mené en
   parallèle des groupes ci-dessus
5. Groupe 6 (drag & drop vers graphe) — indépendant, à investiguer en premier
   pour confirmer si les deux symptômes (zone 10px, pas de courbe tracée)
   partagent une cause commune
6. Groupe 2 (largeur des colonnes +1/3) — correctif isolé, peut être fait à
   tout moment, y compris en premier comme échauffement
