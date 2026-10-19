# Gestes à la souris : la matrice (#299)

Équivalent pour la souris de [`menus.md`](menus.md) : **ce que fait chaque
geste sur chaque surface**. Une surface nouvelle, ou un geste nouveau sur une
surface existante, se range ici ; un geste qui diffère de la même action
ailleurs se signale contre cette page. Les raccourcis clavier sont dans
`app/shortcuts.rs` (registre) et `docs/application.md`, pas ici.

Cette page décrit l'**état réel du code** (relevé le 2026-10-20). Ce qui est
décidé mais pas encore écrit est marqué **À VENIR** et ne doit pas être lu
comme en place : la décision D3 de #299 (aperçu d'analyse aligné sur le
graphe) en est l'unique source aujourd'hui.

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
geste ne fait rien sur cette surface. **À VENIR** : décidé, pas en place.

## Le graphe (figure)

Source : `render-egui/lib.rs` (navigation, objets), `render-egui/shape_tools.rs`
(formes) ; le menu est posé par `app/graph_window.rs`. Le glisser simple dépend
de l'**outil actif** de la barre Tools.

| Geste | Sur | Effet |
|---|---|---|
| Clic | courbe, texte libre, forme, titre, légende | sélectionne l'objet (l'outil Pointeur seul sélectionne une courbe) |
| Clic | zone nue | désélectionne ; active le calque cliqué ; outil Texte : pose un texte |
| Glisser | zone de tracé, outil Pointeur | déplace la vue (pan) |
| Glisser | un axe, outil Pointeur | ne déplace que l'échelle de cet axe |
| Glisser | texte libre, forme, titre ou légende, outil Pointeur | déplace l'objet (une poignée de largeur le redimensionne) |
| Glisser | zone de tracé, outil Zoom | rectangle de zoom (au-delà de 4 px) |
| Glisser | zone de tracé, outil de forme | trace la forme ; un clic sans mouvement pose la taille par défaut |
| Double-clic | texte libre, titre, entrée de légende | édition sur place |
| Double-clic | courbe, forme, axe | ouvre ses paramètres |
| Double-clic | zone nue, marges | recadre (réinitialise le zoom) |
| Molette | — | — (la molette seule ne zoome pas, #62) |
| Ctrl + molette (ou pincement) | zone de tracé | zoom ancré sous le pointeur |
| Ctrl + molette | un axe | zoom de cette seule échelle |
| Ctrl + glisser | — | — |
| Clic droit | tout objet ou la zone | sélectionne la cible et ouvre son menu |
| Maj + molette | — | — |

## L'aperçu de la fenêtre d'analyse

Source : `app/analysis_preview.rs` (aperçu), `app/analysis_handles.rs`
(poignées sur le graphe). Le glisser simple dépend de l'outil choisi dans le
sélecteur de la fenêtre (Points / Zone / Zoom, exception justifiée de #299).

| Geste | Sur | Effet aujourd'hui | Cible D3 |
|---|---|---|---|
| Clic | aperçu, outil Points | ajoute un point (Maj : aimanté à la courbe) | inchangé (D2) |
| Glisser | aperçu, outil Zone | trace une zone | inchangé (D2) |
| Glisser | aperçu, outil Zoom | rectangle de zoom | **À VENIR** : disparaît avec le bouton Zoom |
| Double-clic | aperçu | recadre | inchangé |
| Molette | aperçu | zoom ancré sous le pointeur | **À VENIR** : ne zoome plus |
| Ctrl + molette | aperçu | — | **À VENIR** : zoom |
| Ctrl + glisser | aperçu | — | **À VENIR** : pan |
| Maj + molette | aperçu, outil Points | change la moyenne de l'aimant (2 points par cran) | inchangé |
| Clic droit | aperçu | retire le point ou la zone le plus proche | inchangé |
| Glisser | une poignée sur le graphe (`app/analysis_handles.rs`) | déplace la poignée du calcul ; aperçu immédiat | inchangé |

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
| Glisser | bord d'une sélection de colonnes (ou de lignes) | dépose un tracé sur un graphe |
| Clic droit | cellule, en-tête, lettre, ligne | si hors sélection, la remplace par la cible ; ouvre le menu de la sélection |

## Les arbres : Object Manager et Project Manager

Sources : `app/object_manager_ui.rs`, `app/list_row.rs`, `app/reorder_list.rs`,
`app/project_manager.rs`. Toute la ligne répond, hors case de visibilité.

| Geste | Sur | Effet |
|---|---|---|
| Clic | ligne (OM) | sélectionne l'objet (et l'active, pour un calque) |
| Clic | ligne (PM) | sélectionne et ouvre la fenêtre |
| Ctrl + clic | ligne (PM) | bascule la ligne dans la sélection |
| Maj + clic | ligne (PM) | plage ancre → cible (Ctrl + Maj : ajoute la plage) |
| Clic lent (second clic sur l'entrée seule sélectionnée) | ligne (PM) | renomme, comme F2 |
| Double-clic | ligne (OM) | ouvre les paramètres de l'objet |
| Double-clic | ligne (PM) | active ou restaure la fenêtre, jamais ne la ferme (sans modificateur) |
| Glisser | ligne (OM) | réordonne ; sur un graphe, déplace l'objet |
| Glisser | ligne (PM) | glisser-déposer d'objets (dossier, graphe) |
| Clic droit | ligne | si hors sélection, la remplace ; menu de l'objet (un seul menu, `menus.md`) |
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
| Clic | fenêtre derrière | l'amène devant |
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
