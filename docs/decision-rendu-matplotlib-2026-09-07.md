# Rendu par défaut : de PyQtGraph à Matplotlib — 2026-09-07

Rapport d'une session de test manuel du jalon v0.3 (import → tracé → édition
des propriétés), qui a produit trois correctifs et une décision d'architecture
revenant sur un choix posé en phase 6. Ce document sert de mémoire à cette
décision — la conversation qui y a mené ne sera pas rejouable telle quelle.

## 1. Trois correctifs livrés le même jour

Remontés en testant l'application à la main :

| # | Constat | Correctif | Commit |
|---|---|---|---|
| 1 | Thème sombre par défaut | `dark_teal.xml` → `light_teal.xml`, posé comme axe de développement principal ; PyQtGraph aligné (fond blanc/texte noir), sans quoi les graphes seraient restés sur fond noir sous un thème clair | `da69317` |
| 2 | Plantage à l'édition du titre d'un graphe ou d'un axe (dock Properties) | `editingFinished` part depuis `QLineEdit::keyPressEvent`, encore sur la pile C++ ; le panneau reconstruisait `_body` de façon synchrone depuis ce même signal et détruisait le widget qui le traitait encore (use-after-free). Reconstruction différée d'un tick d'événements, comme le fait déjà `GraphWindow.refresh` pour une raison voisine. Reproduit et vérifié : les deux nouveaux tests échouent sur l'ancien code (`RuntimeError: already deleted`), passent avec le correctif | `bddac60` |
| 3 | Un classeur déroulé jusqu'aux colonnes, multiplié par plusieurs classeurs ouverts, rend l'explorateur de projet illisible | Le déroulement est conservé (utile pour retrouver une colonne), ajout d'une action « Réduire tout » (`Ctrl+Shift+R`, menu Affichage, menu contextuel de l'arbre) | `e80c3a7` |

Les trois sont commités localement, non poussés (politique du projet : jamais de push sans instruction explicite). Suite complète verte (664 tests) et `ruff` propre après chaque correctif.

## 2. La question posée : Matplotlib peut-il remplacer PyQtGraph ?

Point de départ : l'utilisateur voulait pouvoir fine-tuner ses figures en
Python après coup, plus facile selon lui avec Matplotlib qu'avec PyQtGraph.
L'architecture d'origine (posée avant la phase 6) séparait déjà les deux
usages — PyQtGraph pour l'écran, Matplotlib pour l'export publication en
phase 10 — précisément pour éviter la divergence entre les deux rendus. La
question soulevée ici était plus radicale : et si Matplotlib devenait le
rendu **par défaut**, PyQtGraph ne restant que pour un usage secondaire ?

### Le raisonnement initial (erroné sur un point, corrigé ensuite)

Argument de départ : Matplotlib redessine toute la figure à chaque
interaction (pas de scène retenue comme `QGraphicsView`), donc coûteux à
haute fréquence d'événements (pan continu, réticule, acquisition live) —
indépendamment du nombre de points. Argument correct en théorie, mais qui
posait un couplage volume de données / fluidité qui ne s'est pas vérifié à
l'usage.

### Les tests empiriques qui ont fait bouger la conclusion

Effectués par l'utilisateur sur sa machine (Core Ultra 7 155H) :

- **700 courbes × 400 points**, interaction (zoom, glissé) : peu fluide,
  **des deux côtés**, Matplotlib comme PyQtGraph.
- **20 courbes** : fluide des deux côtés, PyQtGraph légèrement meilleur mais
  pas de façon décisive.

Conclusion tirée : le décrochage à 700 courbes n'est pas un avantage
architectural de PyQtGraph qui s'effondrerait plus vite — c'est un coût
**par nombre d'objets graphiques** (700 items séparés dans la scène, que ce
soit des `Line2D` ou des `QGraphicsPathItem`), symétrique aux deux moteurs.
L'écart d'interactivité qui justifiait deux moteurs séparés ne se manifeste
pas à l'échelle réelle des données de l'utilisateur.

### Points techniques passés en revue

- **Langage** : les deux rasterisent en C++ sous le capot (Matplotlib via
  Anti-Grain Geometry embarqué, PyQtGraph via `QPainter`/Qt). Ce n'est pas
  le différenciateur.
- **Simplification de tracé** : `path.simplify`/`path.simplify_threshold`
  de Matplotlib (actif par défaut) explique probablement pourquoi les gros
  volumes de points sont restés bon marché dans les tests — un équivalent
  approximatif du downsampling de PyQtGraph.
- **Multithreading** : ni l'un ni l'autre ne parallélise le tracé d'une
  figure sur plusieurs cœurs. Plus de cœurs aide la réactivité perçue (sortir
  la préparation des données dans un `QThread`, pattern déjà utilisé dans le
  projet), pas la vitesse de tracé elle-même — pour les deux moteurs à
  égalité.
- **GPU — la vraie asymétrie trouvée** : PyQtGraph a un viewport OpenGL
  optionnel (`useOpenGL=True`), un déchargement GPU réel. Matplotlib, en
  usage standard intégré à Qt (backend Agg), n'a aucun chemin GPU. Sur une
  machine à CPU faible mais iGPU correct (le cas de la quasi-totalité du
  matériel récent), c'est un levier que Matplotlib ne peut pas rattraper par
  construction. **C'est cet argument, pas la fluidité générale, qui a
  justifié de garder PyQtGraph plutôt que de l'abandonner.**

## 3. Décision

- **Matplotlib devient le rendu par défaut**, complet : légende, couleurs
  fines, axes, grille, outils — tout ce qu'expose déjà le panneau
  Properties.
- **PyQtGraph devient un mode secondaire, « vue directe »**, réservé à
  l'acquisition live/streaming. Volontairement minimal (pas de gestion fine
  de la légende, des couleurs, des axes, de la grille, pas d'outils) —
  usage estimé à 0,5 % des cas.
- Bascule **manuelle**, dans le menu options, délibérément peu visible (pas
  de bouton de barre d'outils).
- À construire **avant la phase 7** (drag & drop), pas reporté à la phase
  10 comme prévu initialement.

### Asymétrie d'effort à ne pas sous-estimer

Le repli PyQtGraph est **déjà construit** — c'est tout le travail de la
phase 6 (`plot_renderer.py`, `graph_window.py`, réticule, intégration MDI),
testé, et vient d'être débogué (§1.2 ci-dessus). Le rendu Matplotlib
interactif par défaut (pan/zoom, réticule, rebranchement live sur les
éditions du panneau Properties), lui, n'existe pas encore. La partie
« repli à faible effort » est donc déjà acquise ; le chantier réel est du
côté du nouveau défaut, pas l'inverse de ce que la formulation initiale
laissait penser.

### Ce que ça change pour le reste du roadmap

Voir la mise à jour de `ROADMAP.md` (phases 6 et 10, section dédiée). En
résumé : la phase 6 est rouverte pour cibler Matplotlib ; la phase 10
(export publication) devrait largement se réduire à un dialogue
d'export (format, DPI, taille) par-dessus un rendu déjà en place, plutôt
qu'un second moteur à écrire.

### Questions ouvertes, à trancher avec l'implémentation

- Le panneau Properties grise-t-il les contrôles inutilisables en vue
  directe, ou les garde-t-il actifs mais sans effet visible tant qu'on n'est
  pas revenu en vue normale ?
- Le mode d'affichage (direct/normal) est-il persisté dans le `.plx`, ou
  purement une préférence d'exécution ? Piste : jamais persisté — à
  confirmer explicitement avant de toucher au schéma.

## 4. Bug trouvé en cours de route, sans rapport avec le rendu

**Vérifié, réel, délibérément reporté** (« à traiter plus tard ») :
supprimer un `Worksheet` ou un `Graph` depuis l'explorateur de projet ne
ferme pas sa sous-fenêtre MDI si elle est ouverte. `MainWindow` ne s'abonne
nulle part à `structure_changed` du projet ; `_open_sheets`/`_open_graphs`
ne servent qu'à éviter de rouvrir un nœud déjà ouvert, pas à réagir à sa
suppression. La fenêtre reste affichée avec des données qui n'existent plus
dans le modèle, sans aucun signalement — contrairement au cas déjà géré
d'une série orpheline dans le panneau Properties, marquée « ⚠ ». Aucun test
existant n'exerçait ce cas (`test_explorer_context_menu_delete` ne supprime
qu'un dossier vide).

Correctif attendu : `MainWindow` (ou chaque sous-fenêtre) écoute
`structure_changed` pour le nœud qu'elle affiche et ferme/retire la
sous-fenêtre correspondante. Question à trancher avec le correctif, pas
après : un `undo` de la suppression doit-il rouvrir la sous-fenêtre, ou la
laisser fermée ?
