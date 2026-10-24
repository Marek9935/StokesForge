# Tri du vrac remonté par l'utilisateur — workbook, fenêtres, drag & drop

18 remarques regroupées en **6 lots de bugs** (avec reproduction et
localisation quand trouvée) et **1 TODO de fonctionnalités** (refonte de
l'en-tête workbook façon Origin). Toutes les localisations pointent vers
`crates/stokesforge-app/src/app.rs` (banc d'intégration egui) sauf mention
contraire.

---

## Partie A — Bugs (comportement qui s'écarte de l'intention du code lui-même)

### A1 — Le glissé de colonne vers un graphe n'affiche pas la courbe, seulement la légende

**Reproduction** (depuis un projet neuf)
1. Ouvrir un classeur avec au moins une colonne numérique et un graphe.
2. Sélectionner une colonne dans le classeur.
3. Saisir la poignée de glissé (bord gauche/droit de la grille) et la
   déposer sur le graphe ouvert.

**Attendu** : la série apparaît dans la légende **et** son tracé est visible
sur le graphe.
**Observé** : la légende se met à jour, le tracé n'apparaît pas.

**Portée** : `AddSeriesToGraph` est poussée par `drop_payload_action` /
`graph_drop_target` (`crates/stokesforge-app/src/app.rs:2688-2719`), mais le
défaut n'a pas été localisé plus précisément côté résolution de série ou
rendu — je n'ai pas remonté la chaîne jusqu'à `stokesforge-render` faute de temps
d'investigation dans cette session, qui était bornée à l'analyse.
**Nature** : incohérence entre deux surfaces (légende à jour, tracé absent) —
la famille la plus révélatrice d'après le skill de rapport : deux chemins de
mise à jour qui auraient dû n'en faire qu'un (mise à jour de la légende vs
invalidation du cache de tracé/résolution des points de la série).
**Invariant potentiellement touché** : aucun identifié avec certitude ;
à vérifier côté invalidation de cache de `stokesforge-render-egui`
(`(revision, rect, pixels_per_point, view_generation)`, voir
`docs/rendu.md`) — si l'ajout de série ne fait pas avancer `revision`, le
cache rendrait l'ancien tracé sans la nouvelle série alors que la légende,
recalculée hors cache, la verrait bien.
**Écarté** : pas un plafond, pas une limite assumée du banc, pas une
dégradation volontaire (aucune série cassée ici, juste absente à l'écran).
**Gravité** : gênant (la fonctionnalité de glissé perd sa seule utilité
visible).

---

### A2 — La zone de prise du glisser-déposer colonne→graphe est en bordure de fenêtre, pas autour de la sélection

Ceci **n'est pas un bug** au sens strict : le commentaire de
`crates/stokesforge-app/src/app.rs:4308-4319` documente explicitement que
`left_rect`/`right_rect` (largeur `COLUMN_GRIP_WIDTH = 10.0`, ligne 514)
sont **volontairement** confinés au bord extérieur gauche/droit de la
grille entière, précisément pour ne pas interférer avec le glissé
d'extension de sélection de `cell_widget` — « ce que la tâche interdit
explicitement ».

L'utilisateur demande maintenant explicitement le changement inverse
(zone de prise positionnée sur les lignes sélectionnées). C'est donc une
**évolution volontaire à trancher**, pas une correction — voir TODO B7.

---

### A3 — Le contour de mise en surbrillance de fenêtre est mal positionné

**Reproduction**
1. Ouvrir deux fenêtres flottantes (ex. deux classeurs) dans la fenêtre
   principale.
2. Faire passer l'une au premier plan (double-clic dans le Project
   Manager, seul déclencheur actuel — voir A4).

**Attendu** : un contour de sélection propre autour de toute la fenêtre
(en-tête compris).
**Observé** : le contour est mal aligné (signalé par l'utilisateur ;
plausible d'après le code : le `rect_stroke` est peint sur `ui.max_rect()`
**à l'intérieur** de la fermeture `.show()` d'`egui::Window`, donc sur le
rectangle de contenu, avant même que `window_header` ne soit dessiné —
`crates/stokesforge-app/src/app.rs:1924-1936`).

**Portée** : `crates/stokesforge-app/src/app.rs:1925-1936`.
**Nature** : résultat visuel faux (positionnement).
**Invariant potentiellement touché** : aucun.
**Écarté** : pas un plafond, pas une limite de banc, pas une dégradation
volontaire — l'utilisateur demande explicitement « trouver une autre
manière propre », ce qui suggère que même l'implémentation actuelle est
reconnue perfectible plutôt que juste buguée : je le classe bug cosmétique
plutôt que feature parce que l'intention (surligner toute la fenêtre) est
déjà codée, juste mal exécutée.
**Gravité** : cosmétique.
**Cause probable** : `ui.max_rect()` est lu **avant** que `window_header`
soit appelé dans le même bloc (ligne 1938), donc le rectangle utilisé pour
le contour ne correspond pas au rectangle final incluant l'en-tête — à
vérifier en comparant avec le rectangle réellement retourné par
`ui.ctx().memory(|memory| memory.area_rect(id))` (ligne 1980), qui lui est
lu **après** coup et sert déjà de source de vérité pour la géométrie
persistée.

*À regrouper avec A4, A5, A6, B-suivant (voir groupe « fenêtres » ci-dessous)
car le correctif propre passe probablement par une seule primitive de
contour partagée plutôt que par un rafistolage au cas par cas — c'est
d'ailleurs ce que demande l'utilisateur pour la fenêtre détachée (« ajouter
un contour... et le garder dans la fenêtre principale »).*

---

### A4 — Pas de surbrillance quand une fenêtre est sélectionnée par clic direct dans la fenêtre principale

**Reproduction**
1. Ouvrir deux fenêtres flottantes dans la fenêtre principale.
2. Cliquer directement sur le corps ou l'en-tête d'une fenêtre (pas de
   passage par le Project Manager).

**Attendu** : la fenêtre cliquée passe au premier plan **et** se surligne.
**Observé** : elle passe au premier plan (comportement natif de z-order
d'egui) mais ne se surligne pas.

**Portée** : `crates/stokesforge-app/src/app.rs:1910` — `is_active` dérive de
`active_tab`, lui-même alimenté par le Project Manager d'après le
commentaire ligne 1927-1929 (« `active_tab` est la même source de vérité
déjà utilisée [...] et le double-clic du Project Manager, pas un nouvel
état ») : rien ne met à jour `active_tab` sur un simple clic dans la
fenêtre elle-même.
**Nature** : résultat faux (état de sélection non synchronisé) — même
famille qu'A3 : « deux chemins qui auraient dû être un seul » (Project
Manager vs clic direct).
**Invariant potentiellement touché** : aucun.
**Écarté** : plafond non applicable ; ce n'est pas une limite documentée
du banc (`docs/application.md` ne mentionne pas cette restriction) ; pas
une dégradation volontaire.
**Gravité** : gênant.

---

### A5 — Pas de surbrillance de fenêtre quand elle est réduite (« masquer le contenu de... »)

**Reproduction**
1. Réduire une fenêtre flottante via son icône de repli (là où elle
   existe, cf. A6).
2. La faire passer au premier plan.

**Attendu** : la fenêtre réduite (en-tête seul visible) se surligne comme
une fenêtre normale.
**Observé** : pas de surbrillance visible.

**Portée** : `crates/stokesforge-app/src/app.rs:1924-1936`, à vérifier en lien
avec A3 : le bloc `is_active` s'exécute indépendamment du repli
(`was_collapsed`/`header_actions.toggle_collapse` sont traités après), donc
le contour devrait en théorie être peint — mais `ui.max_rect()` d'une
fenêtre repliée peut se réduire à la seule zone d'en-tête, ce qui
expliquerait un contour trop petit pour être perçu comme « la
surbrillance ».
**Nature** : incohérence entre deux états de la même fenêtre (dépliée vs
repliée).
**Invariant potentiellement touché** : aucun.
**Écarté** : pas de plafond ni de dégradation volontaire ; à confirmer
que ce n'est pas simplement une conséquence géométrique d'A3 plutôt qu'un
bug indépendant — je ne tranche pas, cause commune plausible mais non
vérifiée par un test.
**Gravité** : cosmétique.

**Ces quatre points (A3, A4, A5, et l'icône manquante A6) forment un même
chantier "surbrillance et repli de fenêtre" à traiter ensemble** : la
correction du positionnement du contour (A3) devrait se faire une seule
fois pour tous les cas (actif/inactif × déplié/replié × docké/détaché),
plutôt que quatre correctifs séparés.

---

### A6 — Icône « masquer le contenu de... » absente sur les fenêtres workbook

**Reproduction**
1. Ouvrir un classeur en fenêtre flottante.
2. Chercher l'icône de repli dans l'en-tête, présente sur les fenêtres de
   graphe.

**Attendu** (d'après le comportement des fenêtres de graphe) : une icône
de repli identique.
**Observé** : absente.

**Portée** confirmée dans le code — `toggle_graph_collapse`
(`crates/stokesforge-app/src/app.rs:2124-2132`) est **explicitement nommée et
commentée** « Les autres types d'onglets n'ont [pas ce repli] » ; l'état
`collapsed_graph_tabs: HashSet<Tab>` (ligne 616) n'est peuplé que pour
`Tab::Graph`.
**Nature** : fonctionnalité manquante plutôt que régression — ce n'est pas
un bug au sens « le code fait le mauvais calcul », c'est un repli qui n'a
jamais été branché sur `Tab::Book`/`Tab::Note`/`Tab::Matrix`.
**Invariant potentiellement touché** : aucun.
**Écarté** : je le range côté bug (extension d'un mécanisme existant
oublié pour un type d'onglet) plutôt que feature parce que le mécanisme
générique (`WindowHeaderActions::toggle_collapse`, `window_header`) existe
déjà et couvre tous les `Tab` — seul le branchement d'état manque pour
Book/Note/Matrix. Si l'équipe préfère le traiter comme une extension de
périmètre, il rejoint alors le TODO B.
**Gravité** : gênant.

---

### A7 — `Tab` en limite de colonne fait un retour à la ligne au lieu d'ajouter une colonne (sauf en dernière ligne d'en-tête `F(x)=`)

**Reproduction**
1. Ouvrir un classeur, se placer dans la dernière colonne, dernière
   cellule du corps.
2. Appuyer sur `Tab`.
3. Comparer avec : se placer dans la dernière colonne de la ligne
   d'en-tête `F(x)=`, puis `Tab`.

**Attendu** : dans les deux cas, `Tab` en bout de grille ajoute une
colonne (comportement déjà correct sur la ligne `F(x)=`).
**Observé** : dans le corps (et, d'après le message, dans le reste de
l'en-tête), `Tab` en bout de grille fait un retour à la ligne au lieu
d'ajouter une colonne.

**Portée** : `Tab` est consommé à au moins deux endroits différents —
`crates/stokesforge-app/src/app.rs:5453` (`cell_widget`, ligne 5375) pour le
corps, et `:4438` pour la ligne `F(x)=` (comportement correct visé comme
référence). La logique correcte existe donc déjà quelque part dans le
fichier ; il s'agit de l'étendre au corps et au reste de l'en-tête plutôt
que de l'inventer.
**Nature** : incohérence entre surfaces (une ligne de l'en-tête se
comporte différemment des autres lignes et du corps).
**Invariant potentiellement touché** : aucun.
**Écarté** : pas de plafond documenté sur le nombre de colonnes qui
expliquerait un refus ; pas une limite assumée du banc (`docs/application.md`
ne mentionne pas cette restriction) ; pas une dégradation volontaire.
**Gravité** : gênant.

*Ce point est le pendant clavier de B1/B2 (ajout de colonne) : même
chantier, à livrer ensemble — cf. TODO.*

---

## Partie B — Fonctionnalités à faire (TODO)

Regroupées par chantier cohérent. Chaque chantier doit passer par
`stokesforge-feature` (choix de couche, menu, raccourci, historique) plutôt
qu'un simple patch UI, car toute mutation du modèle (ajout/suppression de
colonne) doit rester une commande annulable de `stokesforge-commands`.

### TODO — Chantier 1 : refonte de l'ajout/suppression de colonne (remplace les boutons du bandeau workbook)

- [ ] Supprimer les boutons « Importer… » et « Ajouter une colonne » du
      bandeau du classeur (docké **et** détaché — deux implémentations à
      traiter ensemble, `app.rs:3667-3676` et `app.rs:5625-5648`).
- [ ] Supprimer l'affichage « N lignes × M colonnes » au même endroit (même
      double implémentation), donc suppression complète du bandeau dans
      les deux vues.
- [ ] Étendre le mécanisme `Tab` d'ajout de colonne en bout de grille
      (aujourd'hui fonctionnel seulement depuis la ligne `F(x)=`, cf. A7)
      à l'en-tête entier et au corps.
- [ ] Ajouter un menu contextuel clic droit **à droite de la dernière
      colonne** : entrée unique « Ajouter une colonne ».
- [ ] Ajouter, au clic sur l'en-tête d'une colonne, la sélection de toute
      la colonne et une entrée « Supprimer cette colonne » dans son menu
      contextuel.
- [ ] Unifier le menu contextuel : dès qu'une colonne entière est
      sélectionnée, le menu doit être identique que le clic droit ait
      lieu sur l'en-tête `#` ou n'importe où dans la colonne — garder le
      contenu actuel du menu de l'en-tête `#` comme référence.

*Dépendance interne* : la suppression des boutons (items 1-2) ne doit
être livrée qu'une fois le remplacement clavier/souris (Tab + menus
contextuels) fonctionnel, sinon l'ajout/suppression de colonne devient
impossible entre les deux livraisons.

### TODO — Chantier 2 : panneau de raccourcis façon Origin

- [ ] Ajouter un panneau sous le menu « Fichier », avec des icônes pour :
      créer un classeur, créer un graphe, importer un fichier (référence
      capture d'écran fournie : barre d'icônes dense d'OriginLab, sections
      séparées par des poignées verticales).
- [ ] Décider si ce panneau reprend les actions retirées du bandeau
      workbook (chantier 1) pour la création de classeur/graphe/import, ou
      si c'est un panneau indépendant — l'utilisateur ne tranche pas
      explicitement, mais la suppression du bouton « Importer… » du
      bandeau workbook (chantier 1) et l'apparition d'un raccourci
      « importer un fichier » ici (chantier 2) sont manifestement liées :
      probablement le même geste déplacé.

### TODO — Chantier 3 : largeur des colonnes

- [ ] Augmenter la largeur par défaut des colonnes de 1/3. Vérifier
      l'impact sur le nombre de colonnes visibles à l'écran et sur le
      chantier 5 (troncature au redimensionnement) — les deux touchent la
      même géométrie de grille.

### TODO — Chantier 4 : surbrillance et repli de fenêtre (unifie A3, A4, A5, A6)

- [ ] Revoir le mécanisme de contour de surbrillance pour qu'il englobe
      correctement toute la fenêtre (en-tête compris), docquée comme
      détachée.
- [ ] Ajouter un contour à la fenêtre détachée : un trait gris léger,
      angles légèrement arrondis ; le conserver aussi sur la fenêtre
      principale ; supprimer l'effet d'ombre actuel.
- [ ] Brancher la surbrillance sur le clic direct dans une fenêtre de la
      fenêtre principale (pas seulement via le Project Manager) — voir A4.
- [ ] Vérifier/corriger la surbrillance d'une fenêtre repliée — voir A5.
- [ ] Ajouter l'icône « masquer le contenu de... » aux fenêtres workbook
      (aujourd'hui réservée aux graphes, cf. A6) — à faire dans le même
      passage puisque le mécanisme de repli est directement lié à celui
      de surbrillance.

### TODO — Chantier 5 : redimensionnement et défilement horizontal du workbook

- [ ] Revoir toute la logique de redimensionnement des fenêtres workbook :
      au rétrécissement, les colonnes qui dépassent doivent être
      **tronquées** (clip) plutôt que dessinées hors des limites de la
      fenêtre.
- [ ] Ajouter un glisseur (slider) horizontal fonctionnel pour naviguer
      dans les colonnes tronquées.
- [ ] Implémenter Maj+molette pour faire défiler ce glisseur horizontal.

### TODO — Chantier 6 : glisser-déposer colonne → graphe

- [ ] Repositionner la zone de prise (actuellement une bande de 10 px en
      bordure extérieure gauche/droite de toute la grille, voir A2 — c'est
      un choix **délibéré** documenté en commentaire, pas un oubli) pour
      qu'elle entoure la/les lignes sélectionnées plutôt que le bord de
      la fenêtre. Attention : le commentaire de
      `crates/stokesforge-app/src/app.rs:4308-4319` explique pourquoi le choix
      actuel évite d'interférer avec le glissé d'extension de sélection
      de `cell_widget` — toute nouvelle zone de prise devra composer avec
      la même contrainte.
- [ ] Corriger l'affichage de la courbe après un glissé de colonne vers un
      graphe — actuellement seule la légende se met à jour (bug A1, pas
      encore localisé au-delà de `graph_drop_target`,
      `crates/stokesforge-app/src/app.rs:2688-2719`) : nécessitera de creuser
      `stokesforge-render`/`stokesforge-render-egui` pour trouver pourquoi le tracé
      n'est pas invalidé alors que la légende l'est.

---

## Résumé du tri

| Item du message utilisateur | Classement | Groupe |
|---|---|---|
| Supprimer bouton Importer du bandeau workbook | Feature | Chantier 1 |
| Supprimer bouton Ajouter colonne du bandeau workbook | Feature | Chantier 1 |
| Supprimer affichage N lignes × M colonnes | Feature | Chantier 1 |
| Étendre Tab à l'en-tête | Bug (A7) + Feature | Chantier 1 |
| Menu contextuel « ajouter une colonne » à droite | Feature | Chantier 1 |
| Suppression de colonne au clic sur en-tête + menu contextuel unifié | Feature | Chantier 1 |
| Largeur des colonnes +1/3 | Feature | Chantier 3 |
| Panneau d'icônes façon Origin sous Fichier | Feature | Chantier 2 |
| Contour de highlight mal positionné | Bug (A3) | Chantier 4 |
| Contour fenêtre détachée + suppression ombre | Feature | Chantier 4 |
| Tab retour à la ligne au lieu d'ajout de colonne | Bug (A7) | Chantier 1 |
| Highlight manquant au clic direct | Bug (A4) | Chantier 4 |
| Highlight manquant si réduite | Bug (A5) | Chantier 4 |
| Icône « masquer le contenu » absente sur workbook | Bug (A6) | Chantier 4 |
| Resize workbook : colonnes hors fenêtre au lieu de tronquées + slider x + Maj+molette | Bug + Feature | Chantier 5 |
| Zone de prise drag&drop en bordure au lieu des lignes sélectionnées | Feature (comportement actuel délibéré, A2) | Chantier 6 |
| Drag&drop n'affiche pas la courbe, seulement la légende | Bug (A1, non localisé au-delà du point d'entrée) | Chantier 6 |

Aucun test de reproduction `#[ignore]` n'a été écrit dans cette session :
la consigne de la tâche interdisait toute modification de fichier source
du dépôt (seul ce rapport a été produit). À poser au moment de la
correction (`stokesforge-bug-fix`), au bon endroit selon le tableau du skill —
probablement `crates/stokesforge-app/tests/shell.rs` pour A1, A3, A4, A5, A7
(logique d'interface sans fenêtre réelle), et à noter explicitement comme
non testable en CI si un symptôme s'avère dépendre d'un vrai compositeur
(Wayland) lors de l'investigation.
