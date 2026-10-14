# Menus et clics droits : la grammaire (#152, #155)

Source **unique** des règles d'organisation des menus de la barre et des
clics droits. Une entrée nouvelle se range dans une section de cette
grammaire ; un menu qui la viole se signale contre cette page. Le *comment*
(brique, peintures partagées) est dans `crates/stokesforge-app/src/app/menu_sections.rs`
et `docs/application.md` (§ Menus thématiques) ; les termes et les majuscules
sont dans `docs/textes-interface.md`.

## Le principe : contenu et enveloppe

Un sous-menu qui **porte le nom de la cible** (*Selection*, *Sheet*,
*Columns*, *Text*, *Objects*) est utile dans la barre de menus, qui doit dire
sur quoi elle agit, et un niveau de trop au clic droit, qui a déjà désigné la
cible. Les peintures partagées (`object_menu`, `object_node_menu`,
`object_manager_settings_contents`, `columns_menu_contents` /
`column_type_role_menus` / `plot_menu`, `graph_properties_menu`) peignent donc
**le contenu** ; le menu du haut l'enveloppe dans son sous-menu, le clic
droit l'affiche à plat. Il n'y a pas de seconde voie : seul l'emballage change.
**Un objet a un seul menu** (`object_node_menu`), qu'on le désigne dans la
figure ou dans l'Object Manager ; Graph › Selection l'enveloppe.

## Ordre canonique des sections

Une section vide disparaît avec ses séparateurs.

1. **Action principale** de la cible : Show Data, Edit Text, Show / Hide d'une fenêtre…
2. **Créer / insérer** : Add Text, Insert Object ▸, Add Column, Add Worksheet, Plot ▸.
3. **Presse-papiers**, un seul groupe : Cut, Copy et variantes, Paste et variantes.
4. **Organiser** : Rename, Duplicate, Move Up / Down, Bring to Front…, Hide, Type ▸, Role ▸.
5. **Supprimer** : Clear, Delete…
6. **Sortie** : Export ▸.
7. **Properties…**, toujours en dernier, action directe qui ouvre la fenêtre
   existante sur la page de la cible.

## Règles

- **Séparateurs** : un trait fin entre deux sections présentes, jamais en tête,
  en fin ni en double. Il se pose **uniquement** par `MenuSections::section(ui,
  présente, contenu)` ; l'appelant dit si la section existe pour la cible, le
  trait suit. Garde `aucun_separateur_brut_dans_un_menu` (`tests/gardes.rs`),
  sans dérogation : aucun `ui.separator()` dans un menu.
- **Sous-menu** seulement pour un choix exclusif (Type, Role, type de tracé)
  ou au moins **3** entrées secondaires d'une même famille. Jamais 1 entrée ;
  2 entrées se posent à plat. Les actions importantes ou fréquentes sont
  directes.
- **Choix exclusif** (C3, #227) : une **coche seule** devant l'option
  courante, jamais un bouton radio ni le fond d'un bouton sélectionné ; il se
  peint par `menu_sections::menu_choice`. Les cases à cocher restent aux
  bascules d'affichage.
- **Profondeur 2** : le menu et un sous-menu. Au clic droit c'est strict ;
  dans la barre, le menu du haut compte comme le menu (une seule exception
  assumée : View › Object Manager › Expand by Default, enveloppe d'un contenu
  partagé).
- **Absent ou grisé** : sans objet pour cette cible → **absent** (Show Data
  hors d'une courbe, Copy sur un dossier) ; pertinent mais momentanément
  impossible → **grisé avec une info-bulle `.disabled`** qui dit pourquoi.
- **Cible** : un clic droit hors de la sélection la remplace par l'élément
  visé (une courbe cliquée au clic droit est sélectionnée) ; dedans, il agit
  sur toute la sélection.
- **Libellés** : verbes nus (« Copy », « Rename », « Delete »). L'en-tête grisé
  qui nomme la cible (`PlotTarget::label`) est **réservé au graphe**, où la
  cible n'est pas évidente.
- **Raccourci** affiché à droite dès qu'il existe, toujours par
  `format_shortcut` d'une constante `SHORTCUT_*`, jamais en dur.
- **Pas d'icône** dans les menus.
- **Undo / Redo absents des clics droits** : Edit et le clavier seulement.
- **Fond d'un panneau** : ses réglages au premier niveau, sans enveloppe
  « Settings ▸ ».
- **Rien n'est atteignable seulement au clic droit** : toute entrée a son
  équivalent dans la barre de menus, un raccourci ou une barre d'outils.
- **Fenêtres détachées** : mêmes peintures, mêmes cibles ; les actions
  remontent à la racine comme les autres.

## Arborescence par surface

### Graphe (en-tête grisé = cible)

| Cible | Menu |
|---|---|
| Fond, zone de tracé | Add Text · Insert Object ▸ (Rectangle, Ellipse, Line, Arrow, posé au point cliqué à la taille par défaut) ─ Export ▸ (Export Graph…, Copy Graph) ─ Properties… |
| Courbe | Show Data (le classeur de **cette** courbe) ─ Rename · Move Up · Move Down · Hide ─ Delete ─ Properties… |
| Titre, titre d'axe | Edit Text · Reset Position ─ Hide ─ Properties… |
| Entrée de légende | Edit Text ─ Properties… |
| Légende | Reset Position ─ Hide ─ Properties… |
| Axe | Hide ─ Properties… |
| Texte libre | Edit Text ─ Delete ─ Properties… |
| Objet | Bring to Front · Send to Back ─ Delete ─ Properties… |

Reset Zoom n'est pas au clic droit : il reste dans la barre d'outils, le menu
Graph, le bouton du graphe zoomé et le double-clic. Export ▸ n'est que sur le
fond et la zone de tracé, jamais sur un objet (cohérent avec Ctrl+C, qui ne
copie la figure que sans sélection, #149).

### Object Manager

| Cible | Menu |
|---|---|
| Nœud | le menu de l'objet de la figure (`object_node_menu`) |
| Fond | Show Axes ☐ · Show Objects ☑ · Expand by Default ▸ (5 cases) ─ Reset Settings |

View ne contient que « Object Manager ▸ » (plus de Panels ni de Toolbars : panneaux et barres ne se masquent pas par menu ; les réglages du Project Manager et de la console viendront s'y ajouter) ; il enveloppe le même contenu (`object_manager_settings_contents`).
Les cases gardent le menu ouvert (#114).

### Project Manager

| Cible | Menu |
|---|---|
| Fenêtre(s) | Show · Hide ─ Copy · Paste ─ Rename · Duplicate ─ Delete |
| Dossier | Paste (dans ce dossier) ─ Rename (sur place) ─ Delete |

Selection ▸ ne subsiste que dans Edit.

### Grille

| Cible | Menu |
|---|---|
| Cellule, ligne, champ d'en-tête | Cut · Copy · Copy with Header · Paste · Paste in Place ─ Remove Formula, Keep Data (champ Formule portant une formule, #236 ; aussi Workbook › Columns) · Clear · Delete and Shift Up |
| En-tête de colonne | Plot ▸ · Add Column ─ presse-papiers ─ Type ▸ · Role ▸ ─ Clear · Delete and Shift Up |
| Grille vide, zone blanche | Add Column |
| Onglet de feuille, bande vide | Add Worksheet ─ Rename · Duplicate ─ Delete |

### Colonnes et courbes : Analysis (#181, #210)

Le menu du haut (`analysis_menu_contents`) range les méthodes en
**catégories** (#248), une fenêtre par catégorie : Data Manipulation ▸
(Constant… · Linear… · Polynomial… · Anchor Points… · Correct by Reference…,
à plat) · Fitting ▸ (modèles, Custom Model…) · Peaks and Deconvolution ▸
(Decompose Peaks… · Reconvolution… · Direct Deconvolution…) · Integrate… ─
Recalculate Out of Date ─ les **trois dernières analyses lancées** (#249),
numérotées, sous un filet. Un clic sur une dernière analyse ouvre la fenêtre
pré-remplie ; Ctrl+clic la relance sur la sélection courante (la fenêtre
s'ouvre si un réglage manque). Le clic droit d'un en-tête de colonne a un
sous-menu `Analysis ▸` à quatre entrées, une par catégorie, dans la section
« créer » (après `Plot ▸` et Add Column) — une entrée par méthode aurait dépassé
la profondeur 2. Sur une colonne que produit un calcul, une section de gestes
du calcul (Edit · Recalculate · Detach · Delete) suit Type/Role. Les entrées
sont **grisées** (info-bulle `.disabled`) tant qu'aucune colonne Y n'est
visée, jamais absentes.

Le clic droit d'une courbe ou d'un groupe, dans le graphique comme dans
l'Object Manager, propose le même sous-menu `Analysis ▸`, parmi ses actions principales.
Le menu Analysis du haut prend la courbe ou le groupe sélectionné quand un
graphique est actif. Un groupe analyse tous ses membres, dans l'ordre du
tracé ; les séries construites depuis une ligne de feuille sont grisées.
Les X/Y sont ceux du tracé, y compris dans un classeur caché : l'appariement
des colonnes de la feuille n'intervient pas. L'aperçu reprend le cadrage et
les styles de la source ; Add to graph est coché et les résultats rejoignent
son calque et son groupe.

### Barre de menus

Les menus du haut rangent leurs sections dans le même ordre ; ce qu'ils
enveloppent est le contenu des clics droits : Edit › Selection (menu des
fenêtres du Project Manager), Graph › Selection (menu de l'objet),
Workbook › Sheet / Columns / Plot, View › Object Manager.

## Ajouter une entrée

1. Choisir sa section dans l'ordre canonique ; si aucune ne convient, la
   grammaire est à discuter, pas à contourner.
2. Peindre dans la **peinture partagée** de la surface (une seule écriture
   pour la barre et le clic droit), derrière `MenuSections::section`.
3. Décider *absente* ou *grisée* (info-bulle `.disabled` pour un grisage).
4. Libellé : verbe nu, clé du catalogue `en` **et** `fr` ; raccourci par
   `SHORTCUT_*`.
5. Tester dans le module de la surface : l'entrée, son ordre, son absence
   sur une cible qui n'en veut pas.
