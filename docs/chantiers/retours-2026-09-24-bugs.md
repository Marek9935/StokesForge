# Retours bugs — 2026-09-24

Rapport de tri terminé. Les points 1 à 3 ont été reproduits et confirmés à
l'écran lors de l'enquête initiale ; leurs captures temporaires ne sont plus
disponibles. Le point 4 consigne une évolution demandée, sans conception. Le
point 5 est confirmé par comparaison des deux chemins de code et par une
[capture de la fenêtre détachée](captures-2026-09-24/classeur-detache.png).

## Tableau de tri

| # | Remarque (résumé utilisateur) | Classement | Lot |
|---|---|---|---|
| 1 | Contour vert décalé quand la fenêtre est bloquée contre le PM | **bug, corrigé** | fenêtres flottantes |
| 2 | Le contenu d'une fenêtre flottante passe devant les panneaux fixes une fois l'espace de travail défilé | **bug, corrigé** | espace de travail défilable |
| 3 | Le redimensionnement du graphe ne scale pas correctement (le texte des axes se chevauche en petit) | **bug, confirmé** | rendu du graphe (F11) |
| 4 | Bandeau d'actions du classeur → menus contextuels + menu Fichier, option classeur actif/nouveau à l'import | **évolution**, bien spécifiée par l'utilisateur | classeur / import |
| 5 | Fenêtre de classeur détachée : disposition et grille différentes de la flottante | **bug, confirmé à l'écran et par le code** | duplication flottant/détaché |

---

## 1. Contour actif décalé du cadre réel quand la fenêtre est clampée [corrigé]

**Correctif (2026-09-29)** : les fenêtres flottantes ne sont plus des
`egui::Window` mais des `egui::Area` (`show_floating_window`,
`crates/plotyx-app/src/app.rs`). La position n'est plus bornée que par
`clamp_to_workspace`, et le cadre, le contour, les poignées et la géométrie
persistée partent de ce seul rectangle. Tests :
`une_position_hors_de_lespace_de_travail_est_bornee_par_une_seule_fonction`,
`le_cadre_le_contour_et_la_zone_cliquable_suivent_le_meme_rectangle`.

**Reproduction** (sans interaction — géométrie de fenêtre forcée dans un
`.plotyx` construit ad hoc, cf. méthode ci-dessous)
1. Classeur `Book1` sans feuille, géométrie persistée `x: -300, y: 80, width:
   500, height: 400` (donc une position dont l'abscisse est hors de l'espace
   de travail, ce qui force `.constrain_to(...)` à la recaler).
2. Ouvrir ce projet.

**Attendu** : le contour vert de surbrillance longe exactement le cadre gris
réellement peint (recalé par `.constrain_to`, flush contre le Project
Manager).

**Observé** (capture prise, `/tmp/plotyx3.png` — perdue avec le conteneur,
à reproduire) : le cadre gris est bien recalé flush contre le PM. Le contour
vert, lui, est peint à la position **non recalée** (`x: -300`) : il flotte
à gauche, chevauchant le panneau du Project Manager, complètement détaché du
cadre réel.

**Portée** : `crates/plotyx-app/src/app.rs`, boucle des fenêtres flottantes
dans `PlotyxApp::ui`.
**Cause probable** : `let requested_rect = egui::Rect::from_min_size(current_pos, display_size);`
(≈ ligne 2212, juste avant `.constrain_to(...)`) est calculé à partir de
`current_pos` **avant** que `.constrain_to(...)` ne soit appliqué par `egui`
sur le `egui::Window` lui-même. `requested_rect` sert ensuite à la fois à
`paint_active_window_outline` (≈ ligne 2271) et à `floating_resize_handles` :
les deux utilisent donc une position non recalée dès que le clamp entre en
jeu. Piste : ne peindre le contour/poser les poignées qu'après avoir lu la
position **réellement** peinte par `egui` cette image (`memory.area_rect(id)`,
déjà utilisé plus bas pour la géométrie persistée), pas `requested_rect`.
**Nature** : incohérence entre deux surfaces (le cadre et son contour).
**Écarté** : ni plafond, ni limite assumée, ni dégradation volontaire —
aucune mention de ce cas dans le code ou la doc.
**Gravité** : cosmétique mais très visible, et peut induire en erreur sur
quelle fenêtre est réellement active.

Probablement **préexistant** à cette session (le calcul n'a pas changé de
nature avec le chantier « espace de travail défilable », seulement de
coordonnées), mais bien plus facile à déclencher maintenant qu'une fenêtre
peut être clampée en pratique (contre le PM, la console, ou les bords de
`WORKSPACE_SIZE`) sans avoir eu un écran plus grand au moment de la sauvegarde.

---

## 2. Le contenu d'une fenêtre flottante passe devant les panneaux fixes [corrigé]

**Correctif (2026-09-29)** : même changement que le point 1, direction (a)
retenue. Le dessin est rogné à la portion visible de l'espace de travail (hors
panneaux et barres de défilement), et la zone cliquable de la fenêtre est
réduite à cette même portion. Un clic sur la partie cachée va donc au panneau
du dessous. Le bug se produisait aussi **sans** défilement, en bas, sur la
console de script : une fenêtre plus haute que la vue la recouvrait. Test :
`une_fenetre_a_cheval_sur_le_bord_de_la_vue_est_rognee_et_laisse_passer_les_clics`.

**Reproduction** — nécessite un défilement de l'espace de travail
(`PlotyxApp::workspace_scroll ≠ 0`), donc une interaction (glisser la barre)
qu'aucun outil du dépôt ne peut scripter. Confirmé ici en modifiant
temporairement (non committé, déjà annulé) la valeur par défaut de
`workspace_scroll` dans `PlotyxApp::new` pour simuler un défilement, avec
`Book1` à la position logique `x: 50, y: 80` :
1. `workspace_scroll = (150, 0)` (temporaire, pour la capture seulement).
2. Ouvrir le projet, `Book1` de largeur 500 à `x: 50`.

**Attendu** : rien ne doit jamais passer devant le Project Manager ni la
console de script (invariant explicite de la doc de tête du module).

**Observé** (capture prise, `/tmp/plotyx4.png` — perdue avec le conteneur) :
la fenêtre `Book1`, dont le rectangle écran vaut `[50+390-150, …] =
[290, 790]`ish, chevauche la colonne du Project Manager (`[0, 390]`ish) sur
sa moitié gauche. Vérifié pixel par pixel : à `x = 388` (la colonne du
séparateur du PM), sous le haut de la fenêtre, la couleur est celle du fond
de la fenêtre flottante, pas celle du PM — la fenêtre peint bien **par-dessus**
le panneau.

**Portée** : `crates/plotyx-app/src/app.rs`, culling de visibilité de la
boucle des fenêtres flottantes (`if !requested_rect.intersects(viewport_rect)
{ continue; }`, chantier « espace de travail défilable », commit `ea93b3c`).
**Cause probable** : le culling n'écarte que les fenêtres **entièrement**
hors du viewport visible. Une fenêtre à cheval sur la frontière (une partie
dans le viewport, une partie dans la zone que couvrent les panneaux une fois
l'espace de travail défilé) continue d'être peinte **en entier** — `egui::
Window` peint sur `Order::Foreground`, indépendant de toute zone de
défilement ambiante, donc rien ne rogne la portion qui déborde côté panneau.
C'est exactement la limite anticipée (mais volontairement pas traitée, faute
de solution bon marché) à la conception de ce chantier : soit cliper
réellement le contenu de la fenêtre à la frontière du viewport, soit peindre
un masque opaque par-dessus les zones de panneau après les fenêtres.
**Nature** : incohérence entre deux surfaces (confinement horizontal des
classeurs vs confinement du culling de visibilité).
**Écarté** : pas un plafond, pas une limite assumée — l'invariant « rien ne
recouvre les panneaux fixes » est explicite et documenté, ce chantier le
casse par effet de bord non anticipé.
**Gravité** : bloquant pour l'usage réel du défilement (le point même du
chantier), pas juste cosmétique.

**À trancher** (ne pas décider seul, deux directions valables) :
- (a) cliper réellement le contenu peint d'une fenêtre à l'intersection avec
  le viewport (pas de solution native `egui::Window` triviale, demande un
  clip_rect posé à la main sur le calque de la fenêtre) ;
- (b) peindre un masque opaque (couleur de fond des panneaux) par-dessus les
  zones PM/console, dans un ordre de calque au-dessus de `Order::Foreground`,
  après la boucle des fenêtres — plus simple, mais un hack visuel plutôt
  qu'une vraie occlusion (interactions résiduelles à vérifier : un clic sur
  la zone masquée doit-il traverser jusqu'au panneau dessous, ou rester capté
  par la fenêtre masquée ?).

---

## 3. Le graphe ne « scale » pas correctement au redimensionnement

**Reproduction**
1. Graphe `Graph1` (une série `Y = X²`, 21 points), fenêtre flottante très
   petite (`260×200`) puis très grande (`1200×900`).
2. Ouvrir chaque projet séparément.

**Attendu** (relu contre `docs/application.md` § Débordement des fenêtres de
contenu, et contre la demande de l'utilisateur) : le graphe garde une
géométrie de référence **fixe** — celle qu'aurait donné son rendu à
`DEFAULT_FIGURE_SIZE` (480×360) — et le redimensionnement de la fenêtre ne
fait que **mettre à l'échelle** ce rendu (zoom visuel uniforme), sans jamais
recalculer indépendamment la densité des graduations/étiquettes pour la
nouvelle taille.

**Observé** (captures prises, `/tmp/graphe_petit.png` et
`/tmp/graphe_grand.png` — perdues avec le conteneur, à reproduire) :
- Petite fenêtre (260×200, figure mise à l'échelle ≈130×97 px) : les
  étiquettes des deux axes se **chevauchent totalement**, illisibles
  (`"400.0 300.0 200.0 100.0 0.0"` toutes superposées verticalement,
  graduations X collées horizontalement) — la police ne rapetisse pas avec la
  figure, et surtout le nombre de graduations n'est **pas réduit** pour la
  taille réduite.
- Grande fenêtre (1200×900) : les graduations sont recalculées pour la
  nouvelle taille et restent lisibles — pas de symptôme visible dans ce sens
  (mais ce n'est pas ce qui est demandé non plus : ici aussi la figure
  devrait rester à sa taille de référence, juste zoomée, pas re-disposée).

**Portée** : `crates/plotyx-app/src/app.rs::graph_view_contents` (le
mécanisme « contain-fit » posé pour F11, commit `3e78404`) — passe
`rect` (déjà mis à l'échelle, ex. 130×97) directement à `view.paint(&figure_ui,
rect, …)`, qui délègue à `plotyx-render` pour tout redisposer **à cette
taille en pixels**. Le pipeline de rendu (`plotyx_render::pipeline`,
génération des graduations/étiquettes) n'a pas de notion de « rendu de
référence + zoom » : il recalcule sa disposition à chaque taille de `rect`
donnée, y compris des tailles où l'algorithme de densité de graduation ne
tient plus.
**Nature** : résultat visuellement faux (illisible), pas juste imparfait —
la doc et la demande de l'utilisateur convergent sur un comportement précis
que le code actuel ne respecte pas.
**Écarté** : le F11 documenté (contain-fit, ratio 4:3 préservé) n'est pas
remis en cause dans son principe — c'est la **mécanique** de mise à l'échelle
qui est fausse (redisposer à la taille cible plutôt que zoomer un rendu de
référence).
**Gravité** : bloquant pour toute fenêtre de graphe plus petite que la
référence — usage courant (fenêtres flottantes redimensionnées à la main).

**À trancher** (question d'implémentation, pas seulement de constat) :
comment obtenir un vrai « zoom » avec ce pipeline ? Deux pistes, coût très
différent — pas à choisir sans regarder `plotyx-render`/`plotyx-render-egui`
de près (hors budget de cette session) :
- rendre toujours au `rect` de référence (480×360) puis appliquer une
  transformation d'échelle sur les `egui::Shape` produits avant de les
  peindre (si `plotyx-render-egui` s'y prête) ;
- ou changer uniquement l'échelle/`pixels_per_point` effective passée au
  pipeline sans changer les dimensions logiques du `rect`.

---

## 4. Bandeau d'actions du classeur → menus contextuels (évolution)

Reformulation fidèle de la demande, **pas conçue ici** :

- Retirer la ligne de boutons « Ajouter une colonne » / « Ajouter une
  feuille » / « Importer… » (+ compteur lignes×colonnes) au-dessus de la
  grille, dans `book_contents` **et** `detached_book_contents`
  (`crates/plotyx-app/src/app.rs`).
- « Ajouter une colonne » → nouvelle entrée de menu contextuel (à créer) au
  clic droit dans l'espace vide à droite des colonnes de la grille.
- « Ajouter une feuille » → nouvelle entrée de menu contextuel (à créer) au
  clic droit sur la bande des onglets de feuille (`sheet_tabs`).
- « Importer… » → menu Fichier plutôt qu'un bouton du bandeau.
- La fenêtre d'import gagne les trois destinations demandées : le classeur
  actif, un nouveau classeur par fichier, ou un seul nouveau classeur pour
  tous les fichiers importés ensemble. Le sélecteur actuel accepte déjà
  plusieurs fichiers (`pick_files`, `app.rs:4767`).

À faire dans le crate `plotyx-app` (`app.rs` pour les menus et la fenêtre,
`state.rs` pour la destination et les commandes d'import). Compter les **deux
écritures** (flottante/détachée) partout où c'est pertinent. La destination
actuelle est figée à l'ouverture dans `AppState::open_import` (`state.rs:2781`).

---

## 5. Le classeur détaché ne reprend pas la disposition du classeur flottant

**Reproduction** (depuis un projet neuf)
1. Ouvrir un classeur contenant une feuille d'au moins deux colonnes.
2. Observer la place des onglets de feuille, la bande d'actions, l'en-tête
   des colonnes et leur largeur dans la fenêtre flottante.
3. Cliquer sur ↗ dans son en-tête pour détacher ce même classeur. Comparer
   ces zones à largeur de fenêtre comparable ; réattacher pour confirmer le
   retour à la première disposition.

**Attendu** : le contenu garde la même disposition dans les deux fenêtres ;
seuls le cadre et les contrôles propres à la fenêtre OS changent.

**Observé** : la [capture du viewport détaché](captures-2026-09-24/classeur-detache.png)
montre les onglets au-dessus de la barre d'actions, puis une grille dont
l'en-tête n'a qu'une ligne (`#`, `Temps num`, `Signal num`, `Note mixte`). Le
bouton « Insérer 1 ligne » y apparaît, et le bouton de suppression déborde
hors de la largeur utile. La capture montre cinq lignes suivies d'un grand
espace vide ; le code détaché n'emploie pas la réservation de hauteur de la
grille flottante. La comparaison des deux chemins de code précise ces écarts :

| Zone | Fenêtre flottante | Fenêtre détachée |
|---|---|---|
| Onglets de feuille | Sous la grille, avec hauteur réservée en bas (`book_contents`, `app.rs:4339-4351`) | Au-dessus du bandeau et de la grille (`detached_book_contents`, `app.rs:6267-6271`) |
| Bande d'actions | Ajouter colonne, ajouter feuille, importer (`app.rs:4320-4337`) | Ajouter colonne, ajouter feuille, insérer/supprimer une ligne ; pas d'import (`app.rs:6272-6309`) |
| En-tête et gouttière | Cinq rangées : lettre/rôle, nom long, unité, commentaire, formule ; sélection et menus (`grid_contents`, `app.rs:4886-4943`) | Une seule rangée : `#`, nom et type de colonne (`detached_grid_contents`, `app.rs:6317-6353`) |
| Largeurs initiales | Gouttière 90 points, colonnes 54 points, redimensionnables (`app.rs:481-495`, `:4908-4915`) | Gouttière fixe 56 points, colonnes 110 points (`app.rs:6334-6337`) |

La grille détachée emploie aussi `detached_cell_widget` (`app.rs:6371`),
alors que la flottante emploie `cell_widget` (`app.rs:6033`) : leurs gestes
et styles ont évolué séparément. C'est une divergence de comportement à
vérifier lors du correctif, au-delà du seul placement signalé ici.

**Portée** : `crates/plotyx-app/src/app.rs:3311-3319` aiguille la fenêtre
flottante vers `book_contents` puis `grid_contents` ; `app.rs:1668-1680`
aiguille le viewport OS vers `detached_book_contents` puis
`detached_grid_contents`. `sheet_tabs` (`app.rs:2657`) est déjà commun ; sa
position diffère selon l'appelant. La sélection de feuille et le tampon de
saisie locaux de `DetachedTabViewModel` ne sont pas, en eux-mêmes, la cause
de la disposition : ce sont les chemins de rendu distincts.

**Cause établie pour l'écart structurel** : la fenêtre détachée conserve une
seconde implémentation du classeur et une grille simplifiée. Les quatre
différences du tableau sont directement inscrites dans ces fonctions.
**Nature** : incohérence entre deux surfaces.
**Écarté** : `docs/application.md` (§ Détachement générique) justifie un
instantané et un tampon de saisie propres au viewport détaché, pas une
disposition différente. Aucun choix de produit n'établit cette divergence.
**Gravité** : gênant ; aucune perte de données constatée.

**Validation et limite** : la capture utilisateur confirme l'apparence du
viewport détaché ; la disposition flottante décrite ci-dessus est vérifiée
dans `book_contents`/`grid_contents` mais n'a pas été photographiée dans le
même état. Un futur correctif demandera une comparaison visuelle avant/après
et une vérification des gestes dans les deux modes ; un test de géométrie
seul ne prouverait pas l'identité visuelle.

---

## Provenance des reproductions

- Le mécanisme de capture (`PLOTYX_APP_SCREENSHOT` + `PLOTYX_APP_AUTOQUIT_MS`,
  voir `.claude/skills/plotyx-bug-report/references/voir-l-interface.md`)
  **fonctionne dans cet environnement distant** malgré l'absence de serveur
  d'affichage (rendu logiciel Mesa) — contrairement à ce qui avait été dit
  dans une session précédente. À utiliser systématiquement pour tout retour
  visuel.
- Les captures prises pendant cette session (`/tmp/plotyx1..4.png`,
  `/tmp/graphe_petit.png`, `/tmp/graphe_grand.png`) sont volatiles au
  conteneur et ne survivront pas — à refaire si besoin de les revoir. Les
  `.plotyx` de reproduction aussi (`/tmp/repro_contour.plotyx`,
  `/tmp/repro_graphe_petit.plotyx`, `/tmp/repro_graphe_grand.plotyx`) ; la
  méthode pour les reconstruire est dans les sections 1 et 3 ci-dessus
  (géométrie de fenêtre forcée via `Session::handle().set_window_geometry`
  puis `.save(...)`, sans avoir besoin d'interaction).
- Un fichier de test temporaire a servi à générer ces `.plotyx` pendant
  l'enquête initiale ; il n'est plus présent dans le dépôt.
- Point 5 : capture fournie par l'utilisateur et conservée dans
  `captures-2026-09-24/classeur-detache.png`.
