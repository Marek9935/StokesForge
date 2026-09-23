# Retours bugs — 2026-09-24

Rapport **temporaire, inachevé** : session interrompue faute de crédit. Repris
partiellement (points 1-3 investigués et confirmés à l'écran, 4 est une
évolution déjà bien spécifiée par l'utilisateur, 5 confirmé par lecture de
code seulement, pas encore à l'écran). Ce fichier tient lieu de mémoire —
à charger avant de reprendre, puis à basculer dans `docs/chantiers/` normal
(ou `docs/archive/`) une fois traité.

## Tableau de tri

| # | Remarque (résumé utilisateur) | Classement | Lot |
|---|---|---|---|
| 1 | Contour vert décalé quand la fenêtre est bloquée contre le PM | **bug, confirmé** | fenêtres flottantes |
| 2 | Le contenu d'une fenêtre flottante passe devant les panneaux fixes une fois l'espace de travail défilé | **bug, confirmé** | espace de travail défilable |
| 3 | Le redimensionnement du graphe ne scale pas correctement (le texte des axes se chevauche en petit) | **bug, confirmé** | rendu du graphe (F11) |
| 4 | Bandeau d'actions du classeur → menus contextuels + menu Fichier, option classeur actif/nouveau à l'import | **évolution**, bien spécifiée par l'utilisateur | classeur / import |
| 5 | Fenêtre de classeur détachée : disposition différente de la flottante | **bug, confirmé par le code** (pas encore à l'écran) | duplication docké/détaché |

---

## 1. Contour actif décalé du cadre réel quand la fenêtre est clampée

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

## 2. Le contenu d'une fenêtre flottante passe devant les panneaux fixes

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
- La fenêtre d'import gagne une option : importer chaque fichier dans le
  classeur actif, ou créer un nouveau classeur (par fichier, ou un seul pour
  tous les fichiers importés ensemble) — portée exacte de cette option à
  préciser avec l'utilisateur avant de concevoir (`plotyx-feature`).

À faire dans le crate `plotyx-app`, potentiellement `plotyx-import` pour
l'option de destination. Compter les **deux écritures** (dockée/détachée)
partout où c'est pertinent.

---

## 5. Fenêtre de classeur détachée : disposition différente de la flottante

**Pas encore vérifié à l'écran** (un classeur détaché exige un clic sur
l'icône ↗, une interaction qu'aucun outil du dépôt ne peut scripter ; l'état
détaché n'est pas persisté dans le `.plotyx` donc pas reproductible par un
fichier construit à la main non plus). **À demander à l'utilisateur** : une
capture de la fenêtre détachée suffirait à confirmer/affiner ce qui suit.

**Confirmé par lecture directe du code**, comparaison ligne à ligne :

`book_contents` (`app.rs:4307`, flottant) :
1. bandeau (Ajouter une colonne / Ajouter une feuille / Importer… + compteur)
2. séparateur
3. grille (défilement horizontal)
4. séparateur
5. **onglets de feuille en bas**

`detached_book_contents` (`app.rs:6250`, détaché) :
1. **onglets de feuille en haut**
2. séparateur
3. bandeau (Ajouter une colonne / Ajouter une feuille / **Insérer 1 ligne** /
   **Supprimer 1 ligne** + compteur)
4. séparateur
5. grille (défilement horizontal)

Deux divergences structurelles, pas juste une question de pixels :
- **Ordre inversé** : les onglets de feuille sont en haut côté détaché, en
  bas côté flottant.
- **Bandeau différent** : la version détachée a deux boutons de plus
  (« Insérer 1 ligne », « Supprimer 1 ligne ») absents du bandeau flottant.

**Portée** : `crates/plotyx-app/src/app.rs:4307` et `:6250`.
**Nature** : incohérence entre deux surfaces (dockée/détachée) — la famille
de symptôme la plus révélatrice signalée par le skill de triage.
**Écarté** : rien dans la doc ne documente cette différence comme voulue ;
`docs/application.md` dit au contraire qu'il s'agit du « même contenu »,
juste sur deux sources de données différentes (`&mut AppState` vs `&Tree`).
**Gravité** : gênant — pas de perte de données, mais contredit l'attente
explicite de l'utilisateur (disposition identique).

À noter pour la suite : le lot 4 (bandeau → menus contextuels) va de toute
façon réécrire ces deux fonctions — traiter 4 et 5 **ensemble** évite de
resynchroniser deux fois la même paire dockée/détachée.

---

## Notes pour la reprise

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
- Un fichier de test scratch (`crates/plotyx-app/tests/_scratch_repro.rs`,
  préfixé `_` pour signaler qu'il est temporaire) a servi à générer ces
  `.plotyx` — **non committé**, à supprimer ou à garder pour la reprise selon
  ce qui est utile.
- Point 5 : demander une capture de la fenêtre détachée à l'utilisateur avant
  de conclure davantage — le diagnostic ci-dessus vient uniquement du code.
