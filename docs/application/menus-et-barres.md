# Application : menus, barres d'outils, barre d'état

Suite du sommaire [`README.md`](README.md). Contenu : menus thématiques, barres d'outils, barre d'état. La grammaire des menus est dans [`../menus.md`](../menus.md).

### Menus thématiques (#114)

Les menus du haut et les clics droits rangent les actions partagées sous
les mêmes noms. Les raccourcis et les barres d'outils gardent leurs actions.
Les cases restent réservées aux bascules d'affichage ; elles laissent la
chaîne de menus ouverte. Une action ou un choix exclusif (langue, type,
rôle, résolution) ferme toute la chaîne. `MenuBar` utilise
`CloseOnClickOutside` ; le fond de l'Object Manager et ses sous-menus de réglages
utilisent le même comportement. Les actions ferment explicitement
par `Ui::close`, dont egui propage la demande aux menus parents.

| Menu | Organisation |
|---|---|
| File | New → New Project, Add Workbook, Add Graph ; Import → Import…, Refresh from Source, Reopen Last Import… ; Open…, Save, Save As… ; Language ; Quit |
| Edit | Undo, Redo ; Selection → Copy, Paste, Rename, Duplicate, Delete des objets du Project Manager |
| Workbook (classeur actif) | Sheet → Add Worksheet, Rename, Duplicate, Delete ; Columns → Add Column, Type, Role ; Plot → Line, Scatter, Line + Symbol, Bar ; Show Metadata Row, Show x' Row (ce classeur seul) |
| Graph (graphique actif) | Show Data ; Add Text ; Selection → le menu de l'objet sélectionné (grisé sans sélection) ; Reset Text Positions ; Top / Right Mirror Axis (cases, calque actif ordinaire, #176) ; Reset Zoom ; Export → Export Graph…, Copy Graph, SVG Text, PNG Resolution ; Properties → Graph Properties…, Plot Area…, Axes…, Series…, Legend…, Theme… |
| View | Object Manager → les réglages du fond (Show Axes, Show Objects, Expand by Default, Reset Settings) ; plus de Panels ni de Toolbars (#214) |
| Window | Maximize Window / Restore ; Detach / Reattach Project Manager |

La grammaire des sections, des séparateurs et des sous-menus est dans
[`docs/menus.md`](../menus.md) (#152) ; chaque menu range ses entrées par
`MenuSections::section` (`app/menu_sections.rs`), qui ne pose un trait
qu'entre deux sections présentes. Les peintures partagées peignent le
**contenu** ; le menu du haut l'enveloppe dans un sous-menu qui nomme la
cible, le clic droit l'affiche à plat.

Clics droits : le Project Manager réutilise `object_menu` à plat (une fenêtre :
Show · Hide ─ Copy · Paste ─ Rename · Duplicate ─ Delete ; un dossier : Paste ─
Rename ─ Delete, le renommage s'éditant sur place). Un onglet de feuille
propose Add Worksheet ─ Rename · Duplicate ─ Delete, la bande vide Add
Worksheet. Les lettres de colonnes : Plot ▸ · Add Column ─ presse-papiers ─
Type ▸ · Role ▸ ─ Clear · Delete (les colonnes elles-mêmes, `AppState::delete_columns`), au premier niveau (`column_properties_menu`),
la grille vide Add Column ; les cellules, lignes et champs d'en-tête un seul
groupe presse-papiers puis Clear · Delete, sans Undo/Redo.

Le fond de l'Object Manager affiche à plat les réglages de
`object_manager_settings_contents` (aussi sous View → Object Manager) ; ses
nœuds et la figure partagent `object_node_menu`, **le** menu d'un objet
(courbe : Show Data ─ Rename, Move Up/Down, Hide ─ Delete ─ Properties… ;
texte, axe, objet : voir `docs/menus.md`). Le clic droit de la figure ouvre ce
menu pour l'objet visé — une courbe est sélectionnée par le clic droit — et,
sur le fond ou la zone de tracé, Add Text, Insert Object ▸ (la forme est posée
au point cliqué, `GraphView::insert_shape_at`), Export ▸ et Properties… ; Reset
Zoom n'y figure plus. Les contenus et cibles sont identiques en fenêtres
flottantes et détachées.

Les filtres de l'arbre ne changent pas la visibilité des objets du graphique.
Les cases directement dans l'arbre restent des contrôles de visibilité du
modèle, indépendants de ces filtres. L'inventaire des réglages globaux et les
questions de persistance pour la future fenêtre Preferences sont suivis dans
l'issue #148 ; cette refonte n'ajoute pas cette fenêtre.


### Barres d'outils (#59, #60)

`src/toolbar.rs`. Des barres de boutons-icônes, **fixes** (#108) : leur
contenu et leur place sont figés dans le logiciel, dans le panneau
`stokesforge_toolbars` sous la barre de menu. Ni poignée, ni glisser-déposer,
ni barre flottante ou verticale, ni logique de fenêtre étroite. Disposition en
dur (`ToolbarId::row`) : rangée 0 = Fichier, Édition (Undo/Redo, #238 : libellé d'historique et raccourci en info-bulle, grisé = « Nothing to undo/redo »), Outils ; rangée 1 = Texte, large,
seule sur sa rangée. La barre Object Manager est supprimée (#109) : le panneau
se commande par View et son en-tête. Chaque barre se
masque par le menu View ; seule cette **visibilité** est une préférence
(`preferences.json`), jamais une donnée du `.stf`. Un ancien champ `placement`
est ignoré à la lecture. Le `//!` du module dit pourquoi le panneau est un
`egui::Panel::top` (ni `Window` brute, ni `show_window_in_layer`).

- `ToolbarLayout` (pur, testé) : visibilité par barre et rangées visibles.
  `ToolbarState` n'est que la glu egui.
- Un bouton est un `ToolbarButton` : icône du composant commun, info-bulle
  (libellé + raccourci + `.hint`), grisage. Il pousse la même `Action` que le
  menu — jamais une seconde écriture. Le contenu de chaque barre est
  `toolbar::bar_contents` ; un geste qui n'est pas une `Action` (sélecteur de
  fichier, assistant d'import, export) remonte en `ToolbarRequest`, exécuté par
  `StokesForgeApp::run_toolbar_request` (`app/format.rs`) avec les méthodes
  mêmes des menus. Barre
  Fichier (#61, #73) : nouveau projet, classeur, graphique vide, import,
  ouvrir, enregistrer, enregistrer sous,
  exporter (grisé sans graphe au premier plan).
- Barre contextuelle `Tools` (#62), **toujours affichée** (#93) : son contenu
  suit la fenêtre au premier plan (`ToolsContext`, calculé par
  `StokesForgeApp::tools_context`, `app/format.rs`). Graphe : les outils Pointeur / Zoom / Texte
  (`GraphTool` de la vue, bouton enfoncé pour l'outil actif), réinitialiser le
  zoom, propriétés. Classeur : ajouter feuille, ajouter colonne, tracer la
  sélection (courbe, nuage, barres ; grisé sans colonne sélectionnée). Matrice,
  note, aucune fenêtre : les outils du graphe restent là, **grisés**, avec
  l'info-bulle `toolbar-tools.disabled` (la barre ne change pas de forme).
  Après un séparateur, elle porte le **format de l'objet** sélectionné (#64,
  fusionné avec elle par #93 ; `object_format_contents`), grisé sans objet
  sélectionné. Elle se masque encore par le menu View.
- Barre `Text` (format du texte, #63), **toujours affichée** (#108) sur sa
  propre rangée : sans graphe au premier plan ses boutons sont grisés, avec
  l'info-bulle `toolbar-tools.disabled` (comme `Tools`, #93). Police, corps, couleur, gras,
  italique, indice, exposant, formule. Les textes du graphe étant déjà du
  markup Typst, gras, italique, indice, exposant, formule, couleur et corps
  d'un passage **écrivent des balises dans le texte** (`text_format::apply`,
  pur). La **police** n'est plus une balise (#78) : c'est un champ de la zone
  entière (`TextFont`, `AnnotationSpec::font`), posé par `StokesForgeApp::format_zone`
  (`app/format.rs`) — brouillon de l'éditeur pour un texte libre en édition
  (`pending_zone_font_id`), `Action::SetTextFont` pour un titre ou la légende,
  `Action::SetAnnotation` pour un texte libre sélectionné. Un titre, un titre
  d'axe ou la légende **sélectionné** (`FormatTarget::Zone`, #77) ne prend que
  la police, le corps et la **couleur de la zone entière** (`Action::SetTextColor`, #90 ;
  les autres boutons sont grisés). Cible (`FormatTarget`, `StokesForgeApp::format_target`) : un champ en
  édition sur place (texte libre, titre, titre d'axe, légende) → la demande
  attend dans la mémoire d'egui (`pending_format_id`) et le champ la pose à
  son prochain tracé sur le surlignage qu'egui a gardé, ou insère les balises
  au curseur (`apply_pending_format`) ; sinon un texte libre sélectionné →
  tout le texte, en **une** `Action::SetAnnotation` (couleur et corps sont les
  champs du modèle) ; sinon les boutons sont grisés. Gras, italique, indice,
  exposant et formule se défont en les reposant. Un clic sur la barre ne ferme
  pas l'éditeur (`toolbar::over_format_bar`). Gras et italique agissent aussi
  sur tout un titre, titre d'axe ou titre de calque **sélectionné** hors édition
  (#243, `format_zone` : une `Action::SetTextContent`, bascule comme
  `text_format::apply`) ; la légende, sans texte propre, n'en prend pas.
  Largeur d'un texte libre (#244) : sans largeur, le texte tient sur une ligne ;
  les deux poignées du texte sélectionné (milieux des bords gauche et droit,
  `GraphView::width_handles`) fixent `AnnotationSpec::width`, en fraction de la
  figure, et l'éditeur règle l'alignement (`TextAlign`). Un titre, titre de
  calque ou titre d'axe sélectionné a les mêmes poignées (`TextFont::width`,
  `GraphEvent::TextResized` → `Action::SetTextFont`) ; la légende n'en a pas. Les
  trois boutons d'alignement de la barre de format (`TextFormat::Align`, un champ de la boîte
  ou de la zone, jamais une balise) agissent sur le texte libre (brouillon en
  édition, sinon `SetAnnotation`) et sur la zone d'un titre.
  Les listes affichent la police et le corps de la zone sélectionnée (#97),
  ou ceux du brouillon pendant l'édition, y compris en fenêtre détachée.
  Un corps hérité affiche sa taille effective issue du pipeline partagé ;
  l'entrée « Auto » est alors cochée. La choisir retire le corps explicite
  d'un titre ou de la légende, et rétablit le corps par défaut d'un texte libre.
  L'entrée « Graph font » est cochée pour une police héritée, tandis que le
  champ affiche le nom de la police du graphe. « Auto » réutilise la clé
  commune `common-auto` des deux langues.
  Un réglage local de police ou de corps dans le markup affiche le libellé
  neutre « Font » ou « Size » : la barre résume la zone sans interpréter les
  expressions Typst calculées. Le parseur distingue ces réglages du texte
  brut, des commentaires et des blocs de code brut.
  Le sélecteur affiche aussi la couleur de cette zone (#99), jamais le dernier
  choix fait sur une autre zone. Un passage portant une couleur locale donne
  un échantillon neutre. En édition, le brouillon du sélecteur dure seulement
  jusqu'à la validation du geste, qui pose une seule balise sur le passage.
- Visibilité : plus de menu pour masquer une barre (View ne porte que les réglages des panneaux, #214) ; l'état reste dans `preferences.json`. Emplacement et visibilité
  sont dans `preferences.json` (champ `toolbars`), pas dans le `.stf` ; la
  lecture ignore ce qu'elle ne comprend pas. `StokesForgeApp::new` ne lit pas le
  fichier (les tests), `main` appelle `load_toolbar_layout`.
- Les barres ne sont peintes que dans la fenêtre principale, pas dans les
  fenêtres OS détachées.


## Barre d'état (#230)

`AppState::notice` (information), `warn` (avertissement) et `fail` (erreur,
sauf dans un dialogue modal : S-M7) posent un `StatusMessage` à niveau
(`StatusLevel`) et l'ajoutent à un historique de `STATUS_HISTORY_LEN` = 50
messages. Une information s'efface après `INFO_LIFETIME` (8 s) :
`AppState::expire_status`, appelée à chaque image par
`sync_window_title_and_status` (`app.rs`), efface l'échue et rend le délai
restant — seul cet instant programme un redessin, aucun réveil au repos.
Avertissement et erreur restent jusqu'au message suivant. Un clic sur la
barre (`Action::OpenStatusHistory`) ouvre `status_history_window`. Une
exception de script est une erreur (« Script failed. See the console »). La
barre n'affiche plus « Revision N » ni « Reopen Last Import… ».

Le titre de la fenêtre est `AppState::window_title` (nom du fichier, « • »
tant qu'il y a du travail non enregistré), renvoyé par
`ViewportCommand::Title` quand il change. L'historique est horodaté en UTC
(pas de fuseau local sans dépendance de calendrier).
