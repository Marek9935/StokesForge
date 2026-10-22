# Gestes à la souris : la matrice (#299)

Équivalent pour la souris de [`menus.md`](menus.md) : **ce que fait chaque
geste sur chaque surface**. Une surface nouvelle, ou un geste nouveau sur une
surface existante, se range ici ; un geste qui diffère de la même action
ailleurs se signale contre cette page. Les raccourcis clavier sont dans
`app/shortcuts.rs` (registre) et `docs/application/README.md`, pas ici.

Cette page décrit l'**état réel du code** (relevé le 2026-10-20, puis rejoué
dans l'application réelle : voir « Vérification »). L'aperçu d'analyse est
aligné sur le graphe depuis #270 (D3 de #299).

## Le modèle

- **L'objet d'abord** : on clique ou on glisse l'objet lui-même (une courbe,
  une cellule, une poignée), on ne passe pas par un mode.
- **Les outils vivent dans la barre Tools** (Pointeur, Texte, Zoom, formes) et
  nulle part ailleurs. Seule exception justifiée : les outils d'analyse de la
  fenêtre d'analyse (voir « Garde » plus bas).
- **Un même geste, la même action**, d'une surface à l'autre : Ctrl + molette
  zoome, double-clic ouvre les paramètres ou recadre, clic droit ouvre le menu
  de l'objet visé (et le sélectionne s'il ne l'était pas, cf. `menus.md`).

## Légende

`Ctrl` s'entend comme `Command` sous macOS (`modifiers.command`). « — » : le
geste ne fait rien sur cette surface.

## Le graphe (figure)

Source : `render-egui/lib.rs` (navigation, objets), `render-egui/shape_tools.rs`
(formes) ; le menu est posé par `app/graph_window.rs`. Le glisser simple dépend
de l'**outil actif** de la barre Tools.

| Geste | Sur | Effet |
|---|---|---|
| Clic | courbe, texte libre, forme, titre, légende | sélectionne l'objet (l'outil Pointeur seul sélectionne une courbe) |
| Clic | zone nue | désélectionne ; active le calque cliqué ; outil Texte : pose un texte |
| Glisser | zone de tracé, outil Pointeur ou Texte | — (rien : plus de pan au glisser simple, #318) |
| Glisser | un axe | — (rien ; le pan d'une échelle se fait sous Ctrl ou au clic milieu) |
| Glisser | texte libre, forme, titre ou légende, outil Pointeur | déplace l'objet (une poignée de largeur le redimensionne) |
| Glisser | zone de tracé, outil Zoom | rectangle de zoom (au-delà de 4 px) |
| Glisser | zone de tracé, outil de forme | trace la forme ; un clic sans mouvement pose la taille par défaut |
| Double-clic | texte libre, titre, entrée de légende | édition sur place |
| Double-clic | courbe, forme, axe | ouvre ses paramètres |
| Double-clic | zone nue, marges | recadre (réinitialise le zoom) |
| Molette | — | — (la molette seule ne zoome pas, #62) |
| Ctrl + molette (ou pincement) | zone de tracé | zoom ancré sous le pointeur |
| Ctrl + molette | un axe | zoom de cette seule échelle |
| Ctrl + glisser, ou clic milieu + glisser | zone de tracé, tout outil, y compris au-dessus d'un texte, d'un titre, d'une forme, de la légende ou d'une poignée d'analyse | déplace la vue (pan) ; ni rectangle de zoom, ni forme tracée, ni objet déplacé ; hors du cadre du graphe : — |
| Ctrl + glisser, ou clic milieu + glisser | un axe | ne déplace que l'échelle de cet axe (même geste que l'aperçu d'analyse, #318) |
| Ctrl + clic | zone de tracé, outil Texte ou de forme | — (aucun texte posé, aucune forme, pas de retour au Pointeur) |
| Clic droit | tout objet ou la zone | sélectionne la cible et ouvre son menu |
| Maj + molette | — | — |

## L'aperçu de la fenêtre d'analyse

Source : `app/analysis_preview.rs` (aperçu), `app/analysis_handles.rs`
(poignées sur le graphe). Le geste du clic et du glisser simples vient de
l'analyse : la fenêtre ne montre un sélecteur **Point / Zone** (deux textes
entourés à bascule, exception justifiée de #299) que pour la détection de
pics ; les autres analyses ont un seul geste, sans rien à choisir. Il n'y a
plus d'outil Zoom ni de bouton de recadrage.

| Geste | Sur | Effet |
|---|---|---|
| Clic | aperçu, geste Point | ajoute un point, une graine de pic ou une borne selon l'analyse (Maj : aimanté à la courbe) |
| Glisser | aperçu, geste Zone | trace une zone ou une plage |
| Double-clic | aperçu | recadre (cadre d'origine) |
| Molette | — | — (la molette seule ne zoome plus) |
| Ctrl + molette (ou pincement) | tracé | zoom ancré sous le pointeur, même facteur que le graphe |
| Ctrl + molette | un axe (graduations de X sous le tracé, de Y à gauche) | zoom de cette seule échelle |
| Ctrl + glisser | tracé | déplace la vue (le contenu suit le curseur, axe inversé compris) ; ne trace pas de zone ; le modificateur est lu au début du glisser |
| Ctrl + glisser | un axe | ne déplace que cette échelle |
| Ctrl + clic | — | — (rien) |
| Maj + molette | tracé, quel que soit le geste | change la moyenne de l'aimant (1 point par cran, un événement de molette brut par cran) ; le cadre ne bouge pas |
| Clic droit | aperçu | retire le point ou la zone le plus proche |
| Glisser | une poignée sur le graphe (`app/analysis_handles.rs`) | déplace la poignée du calcul ; aperçu immédiat |

## La grille (classeur)

Source : `app/grid.rs`. La molette défile la feuille (barres d'egui) ;
Ctrl + molette n'est pas pris.

| Geste | Sur | Effet |
|---|---|---|
| Clic | cellule | sélectionne la cellule |
| Ctrl + clic | cellule | ajoute ou retire la cellule |
| Maj + clic | cellule | étend le dernier rectangle |
| Glisser | cellule | sélection par rectangle |
| Ctrl + glisser | cellule | ajoute un rectangle, les précédents restent |
| Double-clic | cellule ou champ d'en-tête | édition sur place |
| Clic, Ctrl, Maj, glisser | lettre de colonne | sélectionne, bascule, étend ou ajoute des colonnes (mêmes modificateurs) |
| Clic, Maj + clic, glisser | numéro de ligne | sélectionne les lignes |
| Clic | case du coin « # » | sélectionne tout |
| Glisser | bord gauche ou droit d'une sélection de colonnes, sous la rangée des lettres (ou bord haut ou bas d'un bloc de lignes, si la feuille porte une ligne x') | dépose un tracé sur un graphe |
| Clic droit | cellule, en-tête, lettre, ligne | si hors sélection, la remplace par la cible ; ouvre le menu de la sélection |
| Clic droit › Move ▸ | lettre de colonne (ou Workbook › Columns) | déplace les colonnes sélectionnées dans leur feuille : au début, à la fin, ou d'une place à gauche ou à droite chacune (l'écart entre colonnes non contiguës reste) ; un seul Ctrl+Z ; pas de glisser (le glisser sur les lettres sélectionne déjà) |

## Les arbres : Object Manager et Project Manager

Sources : `app/object_manager_ui.rs`, `app/list_row.rs`, `app/reorder_list.rs`,
`app/project_manager.rs`. Toute la ligne répond, hors case de visibilité.

| Geste | Sur | Effet |
|---|---|---|
| Clic | ligne (OM) | sélectionne l'objet (et l'active, pour un calque) |
| Clic | ligne (PM) | sélectionne et ouvre la fenêtre |
| Ctrl + clic | ligne (PM) | bascule la ligne dans la sélection |
| Maj + clic | ligne (PM) | plage ancre → cible dans l'ordre de l'arbre, dossiers repliés compris : leurs fenêtres entrent dans la sélection et dans l'action (Copy, Delete, Open, Close) (Ctrl + Maj : ajoute la plage) |
| Clic lent (clic sur l'entrée déjà seule sélectionnée, puis délai écoulé sans second clic) | ligne (PM) | renomme, comme F2 ; un double-clic rapide ne renomme pas |
| Double-clic | ligne (OM) | ouvre les paramètres de l'objet |
| Double-clic | ligne (PM) | active ou restaure la fenêtre, jamais ne la ferme (sans modificateur) |
| Glisser | ligne (OM) | réordonne ; sur un graphe, déplace l'objet |
| Glisser | ligne (PM) | — (les lignes du PM ne se glissent pas : clic seul) |
| Clic droit | ligne | si hors sélection, la remplace ; menu de l'objet (un seul menu, `menus.md`) |
| Clic droit › Add Folder | dossier (PM) | crée un dossier « FolderN » dans ce dossier, le déplie et met le nom du dossier neuf en édition sur place (création et nom = deux entrées d'historique) |
| Clic droit | fond de l'OM | réglages du panneau |
| Molette, Ctrl + molette | — | défile la liste ; Ctrl + molette n'est pas pris |

## Onglets (bandes d'onglets)

Source : `app/tab_strip.rs`.

| Geste | Sur | Effet |
|---|---|---|
| Clic | onglet | active |
| Double-clic | onglet (bande qui renomme) | renomme sur place |
| Clic droit | onglet | menu de l'onglet |
| Clic droit | bande vide ou bouton d'ajout | menu d'ajout |

## Fenêtres

Sources : `app/window_chrome.rs`, `app/dialogs.rs`, `app/window_detach.rs`,
`app/window_stack.rs`.

| Geste | Sur | Effet |
|---|---|---|
| Glisser | barre de titre | déplace la fenêtre ; hors de l'application, la détache |
| Glisser | bord ou coin | redimensionne |
| Clic | icône de la barre de titre | réduit, détache, ferme (selon l'icône) |
| Clic | fenêtre derrière | l'amène devant (le clic agit aussi sur ce qu'il vise dans la fenêtre) |
| Clic hors d'un dialogue modal | fond | bloqué ; le contour clignote |

## Champs et sélecteurs

| Geste | Sur | Effet |
|---|---|---|
| Molette | champ nombre survolé (`number_field.rs`) | change la valeur d'un pas, ou d'une entrée de la liste proposée ; la molette est consommée (la fenêtre ne défile pas) |
| Ctrl + molette | champ nombre | pas pris (réservé au zoom) |
| Clic | flèche du champ | ouvre la liste |
| Clic, glisser | carré, roue, barre du sélecteur de couleur (`color_picker.rs`) | choisit la couleur |
| Clic droit | pastille d'une couleur enregistrée | la retire |
| Clic droit | numéro de ligne de l'aperçu d'import (`app/import_wizard.rs`) | menu de la ligne |

## La garde

Deux gardes de `crates/stokesforge-app/tests/gardes.rs` tiennent cette page.

- **`aucun_selecteur_d_outil_hors_de_la_barre_tools`** (#299, D1, D5) : un
  choix (`choice`, bouton sélectionnable, radio) dont les arguments nomment un
  outil (`tool`) est refusé hors de `toolbar.rs`, sauf s'il porte, dans les
  douze lignes qui le précèdent, le marqueur d'exception
  `// OUTIL-EN-FENETRE(#299): <justification>` avec une justification non
  vide. Une sortie de la guideline se justifie donc dans le code, et ici.
- **`aucun_geste_hors_de_la_matrice`** (#299, D4) : tout fichier de production
  qui lit une molette (`scroll_delta`, `zoom_delta`, `MouseWheel`) ou un
  double-clic (`double_clicked`) doit être cité ci-dessous ; un fichier cité
  qui n'existe plus fait aussi échouer la garde. Elle ne juge pas le geste, elle
  force à le ranger dans la matrice.

Surfaces lues (chemins relatifs à `src/` ; `render-egui/` pour
`stokesforge-render-egui`, `app-state/` pour `stokesforge-app-state`) :

- `render-egui/lib.rs`, `render-egui/shape_tools.rs` : le graphe ;
- `app/analysis_preview.rs`, `app/analysis_handles.rs` : l'aperçu d'analyse ;
- `app/grid.rs` : la grille ;
- `app/object_manager_ui.rs`, `app/project_manager.rs` : les arbres ;
- `app/tab_strip.rs` : les onglets ;
- `app/window_chrome.rs` : les fenêtres ;
- `number_field.rs`, `color_picker.rs` : les champs ;
- `app/import_wizard.rs` : l'aperçu d'import.

## Vérification (2026-10-20)

**Graphe, avant #318** : les lignes ci-dessous qui disent « glisser = pan » et
« glisser un axe » décrivent le comportement d'**avant #318** (le Pointeur
déplaçait la vue au glisser simple, sans lire Ctrl). Aujourd'hui : Ctrl ou
clic milieu + glisser, couvert par les tests d'interaction et de session de
stokesforge-render-egui (voir docs/tests.md), rejoué dans l'application après #318. Le geste est lu à la
pression : Ctrl relâché en route, le pan continue ; pressé en route, rien.

Rejoué dans l'application réelle (pilote `run-stokesforge`, X11 sous
Xwayland, souris et molette injectées par XTEST). Une ligne par geste de la
matrice ; « lu dans le code » = non rejoué.

**Aperçu d'analyse** : les lignes ci-dessous qui le concernent (molette = zoom,
outil Zoom, Ctrl + glisser = zone) décrivent le comportement **d'avant #270** ;
le tableau de l'aperçu plus haut est celui d'aujourd'hui, couvert par les tests
de `app/analysis_preview.rs` (`zone_tests`) et vérifié dans l'application après
#270. Maj + molette : egui range la molette sous Maj dans l'axe horizontal
(`smooth_scroll_delta.x`) ; l'aperçu lit les deux axes des événements bruts (un par cran ; `smooth_scroll_delta` étalait un cran sur plusieurs images), sinon la moyenne ne
changeait jamais en vrai.

- **Confirmés** : clic sur une fenêtre derrière ; graphe : Molette seule sans
  zoom, Ctrl + molette (zone, ancré), Ctrl + molette sur un axe (cette échelle
  seule), glisser = pan (avant #318), glisser un axe (avant #318), outil Zoom (rectangle), double-clic
  courbe (paramètres), double-clic zone nue (recadre), clic et glisser de la
  légende ; grille : clic, Ctrl + clic, Maj + clic, glisser, Ctrl + glisser,
  lettre de colonne (clic, Maj, Ctrl), numéro de ligne (clic, Maj), case « # »,
  double-clic cellule (édition), clic droit, glisser le bord d'une colonne
  vers un graphe (série ajoutée) ; PM : clic, Ctrl + clic, clic lent
  (renomme) ; onglets : clic, double-clic (renomme) ; fenêtres : glisser la
  barre de titre, redimensionner par le coin, clic hors d'un dialogue modal
  (bloqué ; le clignotement du contour n'a pas été observé) ; aperçu
  d'analyse (fenêtre Intégrale) : Molette (zoom), Ctrl + molette (sans
  effet), glisser en outil Zone, glisser en outil Zoom, double-clic (recadre),
  clic droit (retire la zone).
- **Corrigés** : Ctrl + glisser du graphe (pan, pas « — ») ; Ctrl + glisser de
  l'aperçu (trace une zone) ; glisser une ligne du PM (rien, pas de
  glisser-déposer) ; bord de glisser de la grille (bords gauche/droit sous les
  lettres) ; clic lent (déclenché par le délai, pas par un second clic).
- **Non rejoués, lus dans le code** : Maj + molette du graphe ; clic
  Texte et outils de forme ; double-clic d'un texte, du titre, d'une entrée de
  légende, d'une forme, d'un axe ; poignée de largeur ; glisser des poignées
  d'analyse sur le graphe ; aperçu : outil Points (grisé dans Intégrale),
  Maj + clic aimanté, Maj + molette ; Ctrl + molette de la grille (feuille
  trop courte pour défiler) ; double-clic et clic droit des lignes du PM ;
  tout l'Object Manager (glisser, double-clic, clic droit) ; clic droit sur
  onglet, bande, bouton d'ajout ; icônes de barre de titre, détachement ;
  Maj + clic PM ; champs nombre (molette) ; sélecteur de couleur ; aperçu
  d'import.
