# Application : graphes

Suite du sommaire [`README.md`](README.md). Contenu : tracer depuis un classeur, glisser des colonnes ou des lignes vers un graphe, propriétés des graphiques, règles communes avec le rendu.

### Tracer depuis un classeur

La rangée des lettres offre, en plus des types (Numeric, Mixed) et du
sous-menu « Role », un sous-menu « Plot » (Line, Scatter, Line + Symbol, Bar) qui crée un graphe à
partir de la sélection de colonnes courante — même cible que les deux autres
sous-menus (`target_columns` : toute la plage sélectionnée si la colonne
cliquée en fait partie, sinon cette colonne seule). Le graphe créé est
ajouté sans titre dans le dossier qui contient le classeur (`Command::AddGraph`,
pas d'entrée séparée pour la feuille) et s'ouvre aussitôt dans son propre onglet.

L'appariement des colonnes en séries suit la convention Origin plutôt qu'un
choix arbitraire : parmi les colonnes visées, chacune de rôle Y
(`ColumnRole::Y`, `ColumnMeta::role`) devient une série, appariée à la
colonne X la plus proche à sa **gauche** dans la feuille — en comptant aussi
les colonnes X non sélectionnées, puisque l'abscisse commune à plusieurs
courbes n'a généralement pas de raison d'être re-cliquée à chaque tracé. Une
colonne Y sans X à sa gauche garde une abscisse implicite (l'indice de
ligne) ; une colonne de tout autre rôle (X, Z, barre d'erreur, étiquette,
aucun) n'engendre jamais de série à elle seule. Une sélection sans aucune
colonne Y ne crée donc aucun graphe — un clic droit sur une plage de
colonnes purement X, par exemple, ne fait rien plutôt que d'ouvrir un graphe
vide. Chaque série prend la couleur de palette de son rang
(`SeriesSpec::from_palette`).

Cette règle vit dans `stokesforge_core::compose::series_from_columns` (#257 :
`pyst.plot` et l'annexe d'une analyse l'appliquent aussi, `x_for`) ;
`AppState::plot_columns` (`stokesforge-app-state/src/state.rs`) n'émet que la
commande et les messages d'échec, et le menu contextuel ne fait que choisir le
type de tracé et pousser `Action::PlotColumns`. Une seconde
action, `Action::AddSeriesToGraph`, applique la même règle d'appariement mais
relit le `PlotSpec` courant d'un graphe existant pour y **ajouter** des
séries (`Command::SetPlotSpec`) sans toucher à celles déjà présentes — elle
sert le glisser-déposer d'un classeur vers un graphe ouvert, décrit
ci-dessous.

### Glisser une sélection de colonnes vers un graphe

Second chemin vers `Action::AddSeriesToGraph`, complémentaire du sous-menu
« Plot » : on **attrape le bord** d'un bloc de colonnes sélectionnées, comme la
sélection d'Excel, et on le lâche sur un graphe ouvert (#160).

**Précondition.** Une sélection de **colonnes entières** (rangée des lettres,
case du coin, Ctrl+A ; non contiguë comprise, #159) :
`AppState::column_drag_payload` rend `None` pour des cellules, des lignes ou un
champ d'en-tête. Le geste emporte **toutes** les colonnes sélectionnées, dans
l'ordre de la feuille.

**Prise.** `app::grid::column_grip_bands` (fonction pure, testée seule) pose
une bande fine (`COLUMN_GRIP_HALF_WIDTH`, 3 pt de part et d'autre) sur chaque
bord extérieur d'un bloc de colonnes sélectionnées qui se suivent — deux
paires pour deux blocs séparés —, du premier champ d'en-tête au bas de la
grille. La **rangée des lettres en est exclue** : ses bords de colonne servent
au redimensionnement (`egui_extras`), qui garde la priorité. Les bandes sont
posées après la table (donc au-dessus des cellules) mais n'existent que sur ces
bords : le glisser d'extension de sélection de `cell_widget` garde toute la
surface des cellules. Curseur `Grab` au survol, `Grabbing` pendant le glisser ;
un repère (pictogramme du composant d'icônes + nombre de colonnes) suit le
pointeur ; Échap annule (`egui::DragAndDrop::clear_payload`).

**Charge utile.** `state::ColumnDragPayload { sheet, columns }` typée — pas une
chaîne —, portée par le mécanisme natif d'egui 0.36
(`egui::Response::dnd_set_drag_payload`, `egui::DragAndDrop`).

**Cible.** Chaque `Tab::Graph` est enrobé par `app::graph_window::graph_drop_target`
(appelée par `tab_contents`, seul point d'entrée commun au dock et aux
onglets détachés) : un survol avec une `ColumnDragPayload` active dessine un
contour sobre autour du graphe ; un relâchement au-dessus convertit la charge
en `Action::AddSeriesToGraph` via `state::drop_payload_action`. Relâcher
ailleurs ne fait rien. L'action porte le **calque** visé (`panel`) et
**n'impose pas de type de tracé** (`plot_type: None`) : les séries prennent
celui de la **dernière série** du calque, `Line` pour un calque vide
(`compose::panel_plot_type`). Une seule commande annulable (`SetPlotSpec`).
Le calque visé est le **calque actif** du graphe (#161, ci-dessous) :
`graph_drop_target` et « Send to » lisent `published_active_panel`.

### Ligne x' et glisser de lignes vers un graphe (#164, #166)

**Ligne x'.** Cinquième champ de l'en-tête empilé, **au-dessus** du nom long
(`HeaderField::XPrime`, rang -5 ; `HeaderField::COUNT` donne le nombre de
champs, `rank`/`from_rank` la correspondance avec l'axe en-tête/données) : une
valeur **numérique** par colonne (`ColumnMeta::x_prime`), saisie comme les
autres champs — un texte vide l'efface, un texte qui n'est pas un nombre fini
est refusé (barre d'état, rien n'est écrit). `Action::SetColumnXPrime`, copier,
coller, vider et Tab suivent les autres champs ; la mutation reste
`Command::SetColumnMeta` (un pas d'annulation). **Import** : rôle de
sous-en-tête `x_prime` (`SubheaderRoles::x_prime`, ligne « x' » de l'assistant,
code `X'` de l'aperçu), lu avec le format numérique de la recette.
**Metadata** : la fenêtre des métadonnées d'une colonne propose, devant chaque
valeur numérique du fichier, « Utiliser comme x' » (le mini sélecteur : la clé
choisie alimente la ligne x' de cette colonne).

**Glisser de lignes.** Même geste que pour les colonnes, sur des **lignes
entières** (gouttière des numéros) : `AppState::row_drag_payload` ne l'offre que
si la feuille porte au moins une valeur x'. Prise par le bord haut ou bas d'un
bloc (`app::grid::row_grip_bands`, pure, les bords se lisent sur la sélection
et non sur les lignes peintes ; la gouttière reste libre), curseurs `Grab` /
`Grabbing`, repère « n lignes », Échap. Charge `state::RowDragPayload`, cible
`graph_drop_target` → `Action::AddRowsToGraph { axis: RowAxis::Ask }`. **Une
ligne = une série** (`SeriesSpec::row`), nommée « Row n » à la création. Axe :
un graphe **sans X** (aucune série à abscisse de colonne ni série ligne) reçoit
les séries sur l'axe X principal du calque actif, sans question ; un graphe
**avec X** pose le dialogue de choix de `app/confirmations.rs`
(`modal_dialog_with`, appelé par `StokesForgeApp::blocking_confirmations`) — « Create Axis » (défaut) ajoute un calque superposé à
axe X en haut (`OverlaySpec { x_top, y_right: false }`), « Use Present Axis »
garde le calque visé, « Cancel » abandonne (`Action::ConfirmRowDrop`). Une
seule `SetPlotSpec` : un seul pas d'annulation, calque compris. **Menus** : sur
une sélection de lignes entières d'une feuille à x' (clic droit sur un numéro de
ligne, menu Classeur), Plot crée un graphe de lignes (`Action::PlotRows`, mêmes
types de tracé) et Send to envoie les lignes (même question d'axe qu'au dépôt) ;
le menu des colonnes n'est plus proposé dans ce cas. Sans x', une sélection de
lignes garde le menu des colonnes qu'elles traversent.

**Analyser une ligne (#253).** `Analysis ▸` est actif sur une courbe « ligne » (clic
droit de la courbe, Object Manager, Graph › Selection, même menu partout) et sur des
lignes entières de la gouttière (menu Analysis du classeur, clic droit des numéros). Le
geste n’analyse pas la ligne : il crée ou complète la **feuille transposée liée**
(« Sheet1ᵀ ») puis ouvre la fenêtre sur sa colonne « Row N » ; le résultat suit une colonne
(classeur des résultats, « Replace original data » agit sur la colonne de la transposée,
aucune série n'est ajoutée au graphe de lignes). Un groupe qui mêle courbes de colonnes et
de lignes est grisé (`analysis-needs-same-curves`). Une colonne Y de la feuille sans x', ou
de texte, fait refuser le geste en la nommant par sa lettre. La feuille transposée porte
une bande en haut de la grille (`app/transpose_ui.rs`) : origine, « Out of date » quand la
source a changé, « Values edited » quand une valeur d'une colonne liée a été éditée, et
Recalculate (Relink avec confirmation dans ce dernier cas). Détail du modèle :
[`../modele-de-donnees.md`](../modele-de-donnees.md), « Feuilles transposées liées ».

**Calques superposés (#167).** Graph › New Layer (Top X, Right Y, Top X Right Y)
ajoute, par `Action::AddLayer`, un `PanelSpec::overlay_on` au calque de base du
calque actif ; les colonnes de rôle **Y2** (apparié au **X2** le plus proche à
sa gauche) partent, à la création du graphe (Plot) comme au dépôt ou à « Send
to », sur le second calque (axe X en haut, axe Y à droite, échelles propres),
réutilisé s'il existe. Le zoom, le déplacement et le rectangle de zoom portent sur les
**échelles** : le geste, exprimé en pixels de la zone de tracé (commune aux
calques d'une case), s'applique à toutes les échelles propres de tous les
calques de la case (X, Y et y2 du calque ordinaire ; X en haut / Y à droite
d'un calque superposé), si bien que les graduations restent alignées. Une
échelle liée n'a pas de cadrage à elle (`Framing` par échelle, `Gesture`,
`GraphView::commit_gesture`). **Un seul axe** : la molette (Ctrl) et le glisser
au-dessus d'un axe (ligne et graduations, pas son titre) ne cadrent que cette
échelle (`commit_axis_gesture`) ; la cible d'un glisser est figée à sa première
image (`NavDrag`), le curseur peut donc quitter l'axe. Les autres échelles, liées
ou non, ne bougent pas. Le rectangle de zoom reste un geste de zone de tracé. Rendu : `docs/rendu.md`.

**Calque actif (#161).** `GraphView::active_panel(spec)` : le dernier calque
sélectionné, état d'interface hors `.stf` et hors historique, porté par la vue
(donc identique en fenêtre détachée), revalidé à la lecture (calque disparu :
le premier). Il change par un clic, un clic droit ou un début de glisser dans
la zone de tracé d'un calque, par la sélection d'un objet du calque (figure ou
Object Manager, `follow_selection`), par un clic sur le nœud du calque, de son
groupe ou de ses axes dans l'Object Manager, et par le **repère numéroté** peint
en haut à gauche de la fenêtre (`layer_badges`, jamais dans l'export ni la
copie). La fenêtre publie le calque actif dans la mémoire d'egui
(`published_active_panel`) pour les menus qui n'ont pas la vue. Suivent le
calque actif : le dépôt de colonnes, « Send to » et Graph › Add Text ; les
gestes qui désignent un point (clic droit, outils) visent leur calque.

**Entre deux fenêtres système : pas de glisser (#165).** Le dépôt marche au sein
d'une même fenêtre OS (dock ou fenêtre détachée), pas de l'une à l'autre : sous
Wayland la fenêtre source garde le pointeur tant que le bouton est pris, le
graphique de l'autre fenêtre ne voit ni survol ni relâchement, et un vrai
glisser-déposer `wl_data_device` coûterait plusieurs jours. Décision : aucun
glisser inter-fenêtres, remplacé par **« Send to <graphique> »**
(`app/graph_targets.rs`), la même `Action::AddSeriesToGraph` que le dépôt, vers
**chaque graphique ouvert**, attaché ou détaché. Il se trouve à côté de « Plot »,
dans le clic droit des lettres de colonnes, dans celui de toute case d'une
sélection de **colonnes ou de lignes entières** (cellule, numéro de ligne,
champ d'en-tête, coin : même menu que les lettres, appliqué aux colonnes de la
sélection, `selection_context_menu`) et dans le menu Classeur. À partir de trois
graphiques, un sous-menu « Send to Graph » ; sans graphique ouvert, l'entrée est
absente.

### Propriétés des graphiques

Les réglages d'un graphe vivent dans une **fenêtre flottante à onglets**
(`app::plot_settings::plot_settings_window`), un **dialogue modal** (`modal_dialog`, `src/app/dialogs.rs`) :
au premier plan, le reste de l'interface ne reçoit plus aucun clic, et un
clic ailleurs fait clignoter son contour en rouge foncé
(`theme::blocked_flash_color`). Deux boutons en bas : **OK** (par défaut,
Entrée hors saisie de texte) et **Cancel** (aussi la croix et Échap) ; leurs
textes viennent du catalogue (`dialog-ok`, `dialog-cancel`). Les
gestes s'appliquent en direct au graphe (aperçu) mais dans une **macro
d'historique** ouverte à l'ouverture de la fenêtre : OK la ferme — une
seule entrée, un Ctrl+Z défait tout (`AppState::close_plot_settings`) ;
Cancel l'avorte et restaure le graphe d'avant
(`AppState::cancel_plot_settings`). Pendant la macro, annuler/rétablir et
enregistrer sont refusés (`stokesforge-commands`), ainsi que les écritures d'une
fenêtre détachée (autre identité d'émetteur : refusées par la session) et les
actions qu'elle remonte à la racine (`reject_detached_actions`). Toute
opération groupée se clôt par `AppState::end_grouped_macro`, qui rafraîchit
l'instantané (sinon `can_undo`/libellé restent périmés, issue #22). Elle est construite
par la même brique que les fenêtres de contenu (`show_window_in_layer`,
`window_header`, `floating_resize_handles`) — même cadre, même barre de
titre — et non par une `egui::Window` brute ; seuls l'ordre de calque et le
calque plein écran qui capte les clics la distinguent. Ce calque est
**invisible** : aucun voile sombre sur le reste de l'application, pour continuer
à voir le rendu du graphe pendant le réglage (issue #20). Ses onglets sont la
même bande que ceux des feuilles (`tab_strip`, issue #19) : c'est le standard
du projet pour tout jeu d'onglets. Les fenêtres OS
détachées, autres viewports, ne sont pas couvertes par ce blocage. Elle
n'occupe aucune place tant qu'elle est fermée.
Elle remplace l'ancien panneau fixe à droite de l'espace de travail, affiché
en permanence dès qu'un graphe était actif. Son état (quel graphe, quel
onglet, quel axe) est `state::PlotSettings`, dans `AppState` ; on l'ouvre
par `Action::OpenPlotSettings { graph, tab, axis }`.

**Double-clic sur la figure** (#113) : sur un axe, une courbe ou un objet
graphique, il ouvre leurs paramètres (`GraphEvent::OpenAxisProperties`,
`OpenSeriesProperties`, `OpenShapeProperties`, traduits par
`graph_window::annotation_interactions`) ; sur un texte, il garde l'édition sur place ;
dans la zone de tracé nue, il réinitialise le zoom. Dans l'Object Manager,
toute ligne (graphe, calque, groupes, séries, axes, objets) ouvre ses
paramètres au double-clic. Sur un graphe à plusieurs calques, le double-clic
sur un axe et son menu contextuel ouvrent l'onglet Axes du calque visé,
avec le même `Action::OpenObjectProperties` que l'Object Manager. L'adresse
est celle du calque dans la grille, indépendante du rang des panneaux tracés.

**Titres, titres d'axe et légende** (issue #53) : se désignent comme un texte
libre. Double-clic sur un titre ou un titre d'axe → éditeur sur place
(`app::graph_text_edit::text_editor_ui`, un champ ; Entrée ou clic ailleurs valide, Échap
annule) ; sur une entrée de légende → renomme la série (`SetLegendEntryName`,
vide = nom dérivé de la colonne) ; sur le cadre de la légende, rien (elle n'a
pas de texte à elle). Un glisser déplace le texte (ancré aux données : il suit
le zoom) ; un clic droit propose « Edit Text » et « Reset Position » (grisé tant
que le texte est à sa place), et « Graph » → « Text » → « Reset Text Positions » remet tout
d'un coup. Chaque geste validé est **une** `Action` (`SetTextAnchor`,
`ResetTextAnchors`, `SetTextContent`, `SetLegendEntryName`), donc un
`SetPlotSpec`, une entrée d'historique ; la position est enregistrée dans le
`.stf` et vaut à l'export.

**Textes libres** (issue #35) : clic droit dans une zone de tracé →
« Add Text » au point cliqué, ou « Graph » → « Text » → « Add Text »
(au centre du premier panneau). Le texte s'édite **sur place** : un champ posé
sur le texte, et au-dessus une barre couleur / taille / rotation
(`app::graph_text_edit::annotation_editor_ui`) ; Entrée ou clic ailleurs valide, Échap annule.
Le champ s'écrit **dans le cadre du rendu** (issue #37) : centré sur le centre
du texte rendu, à la taille de police du texte (corps × échelle d'affichage de
la figure), droit même si le texte est tourné — son placement est la fonction
pure `stokesforge_render_egui::annotation_field`. Un texte vidé n'étant plus tracé,
l'éditeur garde le dernier cadre connu. Tant qu'on tape, le champ a un léger
fond ; dès qu'il perd le focus (on règle la rotation ou la taille), il devient
transparent et l'aperçu **tourné** se voit (issue #36) ; un clic dans son cadre
redonne la main à la saisie. La taille finale diffère un peu après
recompilation par Typst (une formule `$…$` se compose autrement que sa source).
Double-clic ou clic droit → « Modifier le texte » rouvrent l'éditeur ; un clic
sélectionne (contour bleu), Suppr ou « Supprimer » le retire ; un glisser le
déplace. L'aperçu vit dans la vue (`GraphView::set_annotation_preview`) et
l'état de l'éditeur dans la mémoire d'egui, sous l'identifiant de la vue : même
écriture pour la fenêtre flottante et la fenêtre détachée, rien dans
`AppState`. Chaque geste validé est **une** `Action::SetAnnotation` ou
`RemoveAnnotation`, donc un `SetPlotSpec` ou `EditPlotSpec`, une entrée d'historique. Les couleurs
enregistrées sont publiées chaque image dans la mémoire d'egui
(`color_picker::publish_saved_colors`, lue par `color_picker::saved_colors_id`) pour que le sélecteur de l'éditeur les propose aussi
dans une fenêtre détachée.

**Objets graphiques** (issue #76) : quatre outils de la barre « Outils »
(`GraphTool::Rectangle`, `Ellipse`, `Line`, `Arrow`) ; un clic insère une
forme de taille par défaut (#153) : rectangle carré et ellipse circulaire de
48 × 48 px, ligne ou flèche horizontale de 80 px, en pixels de figure à partir
du point cliqué. Glisser trace librement l'objet (aperçu dans la vue) ; le
clic ou relâchement rend la main à l'outil Pointeur et sélectionne l'objet.
Avec le Pointeur : un clic sélectionne (contour et poignées),
glisser le corps déplace (Ctrl + glisser ou clic milieu + glisser déplace la vue
au lieu de l'objet, #318), glisser une poignée redimensionne, Suppr retire.
Maintenir Maj pendant le redimensionnement conserve les proportions à l'écran
au début du geste : le coin opposé reste fixe, une poignée de côté conserve
le côté opposé et le centre de l'autre dimension. Une ligne ou flèche garde
sa direction initiale. Sans Maj, les dimensions restent libres ; une forme
initialement plate reste libre faute de rapport largeur/hauteur défini.
Ces gestes sont communs aux graphes flottants et détachés. Menu
« Graph » et clic droit sur l'objet : « Bring to Front », « Send to Back »,
« Delete Object » (`graph_window::shape_order_buttons`). Chaque geste validé est **une**
`Action::SetShape` / `RemoveShape` / `ReorderShape`, donc un `SetPlotSpec` ou `EditPlotSpec`. La
partie « format d'objet » de la barre `Tools` (issue #64, fusionnée par #93 ;
`object_format.rs`) règle l'objet sélectionné : couleur de trait, couleur de remplissage (grisée pour
une ligne ou une flèche ; l'opacité est l'alpha du sélecteur, `#RRGGBBAA`),
épaisseur. Chaque réglage est un champ du modèle posé par une `Action::SetShape`
(`StokesForgeApp::format_object`, `app/format.rs`). Côté script : `pyst.add_shape` / `pyst.remove_shape` (`docs/python-et-script.md`).

**Exporter le graphe** (issue #34) : menu « Graph » → « Export » → « Export
Graph… », Ctrl+E (graphe actif seulement), ou clic droit sur la figure (y
compris dans une fenêtre détachée). Un sélecteur `rfd` (SVG, PDF, PNG) propose
le nom du graphe (`state::export_file_name`) ; le format suit l'extension,
`.svg` si elle manque. `app::graph_window::request_graph_export` lit le zoom de la vue
(`GraphView::framed_spec`) et pousse `Action::ExportGraph { path, spec }` ;
`AppState` écrit le fichier (`stokesforge-render-svg`, texte Typst compris) et le dit
dans la barre d'état. Aucune commande : le modèle n'est pas touché. La
résolution PNG (×1, ×2, ×4 ; ×4 par défaut, soit 1920 × 1440 px) se règle dans
« Graph » → « Export » → « PNG Resolution » (`Action::SetPngExportScale`,
préférence de session). Le texte des SVG exportés (#162) se règle dans
« Graph » → « Export » → « SVG Text » : **Texte** (défaut ; `<text>`
sélectionnable, police nommée mais non embarquée) ou **Contours** ; le choix est
retenu d'un lancement à l'autre (`preferences.json`, `Action::SetSvgTextMode`,
ni commande ni historique ni `.stf`) et ne vaut que pour le SVG.

**Copier le graphe vers une autre application** (issue #149) : menu « Graph »
→ « Export » → « Copy Graph », ou **Ctrl+C** quand une fenêtre de graphique est active au
premier plan (flottante ou détachée), qu'**aucun objet** n'y est sélectionné
(série, texte libre, forme, titre ou légende : `selected_manager_target`, la
sélection de l'Object Manager est celle de la vue) et qu'aucun champ de saisie
n'a le focus (`keyboard_focus_outside_grid`). Le presse-papiers reçoit la
**même figure en trois représentations**, offertes ensemble — SVG, PNG, PDF,
dans cet ordre de préférence ; c'est l'application destinataire qui choisit, et
beaucoup préfèrent le PNG. Le cadrage est celui de la vue affichée (zoom et
déplacement, `GraphView::framed_spec`), la taille, les polices et le fond ceux de
l'export vers un fichier, et le PNG suit « PNG Resolution ». Le Ctrl+C d'une
cellule, d'une feuille ou du Project Manager est inchangé : le Project Manager
retire lui-même son `Event::Copy` quand il a le focus, et `figure_copy` passe
avant les panneaux et les contenus, de sorte qu'une grille visible derrière la
fenêtre de graphique ne reçoit pas le même événement.

Chaîne : `app::figure_copy::request_figure_copy` (la racine, depuis `ui()`,
ou le viewport d'un graphique détaché) pousse `Action::CopyGraphFigure { spec }` ;
`AppState::copy_graph_figure` compose la figure **une fois**
(`stokesforge_render_svg::export_bytes_many`) et prépare les représentations
(`take_figure_clipboard`) ; la fenêtre racine les offre ensuite
(`StokesForgeApp::offer_figure`, qui seule connaît son affichage par
`eframe::Frame`) par `stokesforge-clipboard`. Un format qui échoue seul est omis
et nommé dans le message ; si aucun n'est prêt, ou si le système refuse, la barre
d'état donne la cause. Aucune commande : ni le modèle ni l'historique ne
bougent.

Pourquoi un crate à part : ni egui (`Context::copy_text`) ni `arboard` ne savent
offrir `image/svg+xml` ni `application/pdf`, ni plusieurs types à la fois.
`stokesforge-clipboard` porte un backend par affichage : **Wayland** par
`Clipboard::store_formats` du patch `vendor/smithay-clipboard` (le worker déjà
partagé avec egui-winit, voir `vendor/winit/STOKESFORGE.md`), **X11** par un
propriétaire de la sélection `CLIPBOARD` (`TARGETS`, un type par
représentation, pas d'`INCR` : une représentation plus grosse qu'une requête X
n'est pas offerte), **Windows** (un format enregistré par représentation :
`PNG`, `image/svg+xml`, `application/pdf`) et **macOS** (`NSPasteboard`, UTI
`public.svg-image`, `public.png`, `com.adobe.pdf`). Sous Wayland le
compositeur n'accepte la sélection que de la fenêtre focalisée qui vient de
recevoir une entrée : l'offre part donc du geste, et sinon elle est perdue sans
erreur, comme le texte d'egui-winit.

Vérifié : la composition et l'ordre des trois formats, le geste (sélection,
focus, onglet actif, événement consommé) et l'entrée de menu par des tests ;
X11 contre un vrai serveur (`cargo test -p stokesforge-clipboard --test x11 --
--ignored`, XWayland suffit). **Non exécutés par la CI ni ici** : Wayland réel,
Windows et macOS (ces deux derniers compilent seulement,
`cargo check -p stokesforge-clipboard --target x86_64-pc-windows-gnu` et
`aarch64-apple-darwin`). Contrôle manuel Wayland : lancer l'application,
sélectionner un graphique au premier plan sans rien y sélectionner, Ctrl+C, puis
`wl-paste -l` doit lister `image/svg+xml`, `image/png` et `application/pdf`
(`wl-paste -t image/png > figure.png` pour lire une représentation).

**Onglets** (`state::PlotSettingsTab`, clés `plot-tab-*`, en anglais) :
« General » (titre et sa couleur, fond), « Plot Area » (titre du calque,
quadrillage), « Axes » (un axe à la fois, choisi en tête de page : titre de
l'axe — grandeur et unités —, échelle, affichage, couleur de l'axe),
« Legend » et « Theme ». Les séries n'ont plus d'onglet (#179) : elles ont leur
propre fenêtre, **Series Properties**, décrite plus bas. Les textes de toute la fenêtre sont des clés `plot-*`
du catalogue `en` (infobulle en attribut `.hint`), selon le glossaire
`docs/textes-interface.md` ; les noms saisis (titres, noms de série, thèmes)
sont des données, jamais traduits.

**Textes du graphe** : titre, titre du panneau, libellé et unité d'axe, nom de
série sont composés par Typst et lus comme du **markup** (`docs/rendu.md`
§ Typst). Chacun de ces champs (`formula_field`) porte à sa droite une petite
icône d'aide grisée (#197, `app/hover_help.rs`) dont la bulle, au survol, rappelle
la syntaxe (`formula_help`, issue #33) : `$lambda$`, `$x_2$`, `$"cm"^(-1)$`,
`*bold*`, `\$` pour un dollar, et nomme Typst avec l'adresse de sa référence.
Elle remplace le mémo repliable « Math syntax » : le lien avec le champ se
voit et l'aide ne prend plus de hauteur. Tout champ futur qui accepte du
texte Typst (côté interface : « Math », #322) passe par `formula_field` ; une autre aide au survol, par `hover_help`.
Le test `chaque_exemple_du_memo_compile` vérifie que chaque exemple compile. Un
nom de série laissé vide reprend celui de la colonne, affiché tel quel.

**Onglet « Thème »** : deux thèmes prédéfinis (`stokesforge_plot::THEMES` : Clair,
Sombre) et trois couleurs globales — fond, texte, axes. Une
couleur globale est **écrite dans tous les champs locaux** (titre du graphe,
titre et échelle de chaque axe de chaque panneau ;
`PlotSpec::set_text_color`/`set_axes_color`/`apply_theme`) : le dernier
réglage écrase ce qui avait été réglé une à une, et rien n'est mémorisé en
plus (le format `.stf` ne change pas). Le tout reste dans le brouillon de la
fenêtre : un seul pas d'historique à la validation. Les thèmes laissent le
texte et les axes en automatique, donc ils suivent le fond.

**Sélecteur de couleur** (`crate::color_picker`) : toute couleur de la
fenêtre se choisit par un bouton-échantillon (`color_field`/`color_button`)
qui ouvre une fenêtre surgissante **repliée** : palette, « Auto »
quand la couleur peut l'être, couleurs enregistrées et saisie hexadécimale
(`#RRGGBB`, ou `#RRGGBBAA` avec l'opacité). La case **« Custom color »**
(une case à cocher depuis S-C5, #227) déplie le reste — Save Color et sa destination, nuancier (carré saturation ×
luminosité avec barre de teinte, ou **roue chromatique** avec barre de
valeur), **opacité**, saisie RGB et HSL. Le sélecteur se rouvre toujours
replié (`color_picker::custom_id`). Cette fenêtre est une `egui::Area` gérée
par `color_button` (et non un `egui::Popup`, dont egui n'autorise qu'un
exemplaire ouvert : la liste d'un menu déroulant le refermerait) ; elle
se ferme sur Échap ou sur un clic hors d'elle et de ses listes. Une couleur transparente se montre sur un
damier (dans l'échantillon du sélecteur). **Une palette par usage** (`stokesforge_plot::ColorUsage`) :

- **courbes** : un outil à part, avec un choix de palettes nommées
  (`stokesforge_plot::CURVE_PALETTES` : StokesForge, Tableau 10, Okabe-Ito lisible par
  les daltoniens, ColorBrewer Set1/Dark2/Paired, dégradés Viridis, Plasma,
  gris), chacune avec ses nuances foncées et claires. Les palettes vivent
  dans `stokesforge-plot`, pour qu'une évolution future colore plusieurs courbes
  d'un coup (palette ou dégradé) ;
- **texte** : encres lisibles ;
- **fonds** : en premier **« No color »** (`#ffffff00`,
  `stokesforge_plot::TRANSPARENT`), qui est aussi le **fond par défaut** d'un
  graphe neuf (`PlotSpec::default`) : la figure exportée n'a pas de fond,
  pour s'intégrer à une autre présentation ; à l'écran, elle se montre sur
  un fond **blanc** peint par l'application sous la figure
  (`app::graph_window::display_backdrop`), jamais exporté. Puis des fonds clairs, et des fonds
  sombres (le texte automatique passe en blanc, voir `docs/rendu.md`). Un
  fond vide (fichiers écrits avant ce défaut) reste blanc.

**Couleurs enregistrées** : « Enregistrer » garde la couleur courante
**dans le projet** (par défaut — commande annulable `SetSavedColors`,
enregistrée dans le `.stf`) ou **pour l'utilisateur** (fichier
`couleurs.json` du dossier de configuration de StokesForge, à côté des recettes
d'import, hors historique — `stokesforge_app_state::user_colors`), au choix
(`state::SaveTarget`). Une couleur enregistrée ne réapparaît que dans la
palette de son usage ; clic droit dessus pour la retirer
(`Action::SaveColor`/`RemoveSavedColor`). Le sélecteur ne touche ni au
modèle ni au disque : il dépose des demandes dans un `PickerEnv`, que
`plot_settings_contents` traduit en actions.

La couleur est validée à la fin du geste (clic sur un échantillon, fin de
glisser, champ validé, fermeture), jamais pixel par pixel. Tout nouveau
réglage de couleur passe par ce sélecteur, avec l'usage qui lui correspond.

**Trois façons de l'ouvrir** :

- **clic droit sur un composant du graphe** (`app::graph_window::graph_context_menu`) :
  titre, zone de tracé, axe, titre d'axe, courbe, légende. Le menu est celui
  de l'objet visé (`object_node_menu`), dont « Properties… » ouvre la page qui
  règle ce composant (`state::PlotTarget::settings_page`) ; sur le fond et la
  zone de tracé : Add Text, Insert Object, Export, Properties…. Le composant
  sous le pointeur est trouvé par `stokesforge_render::hit::component_at`, sur la
  géométrie réellement allouée par Plotters (zone de tracé, aires de
  graduation, bande du titre), pas sur une reconstitution. Le clic droit
  **sans** glisser ouvre le menu ; le glisser secondaire reste le zoom
  élastique. Même chemin en fenêtre flottante et détachée
  (`graph_view_contents`) ;
- **menu « Graph »** de la barre de menus, présent seulement quand
  l'onglet actif est un graphe (`state::active_graph`, testée sans fenêtre) :
  un sous-menu « Properties » avec une entrée par onglet, et « Reset Zoom ». Aucun réglage n'est
  donc atteignable **seulement** au clic droit ;
- rouvrir la fenêtre sur un autre graphe valide d'abord le brouillon du
  précédent.

Fermer la fenêtre valide ce qui diverge encore du modèle
(`AppState::close_plot_settings`), par exemple un champ texte qui n'a pas
encore perdu le focus.

**Édite le `PlotSpec` au travers d'un brouillon, jamais directement.**
`state::PlotEdit { graph, spec, selected_series }` est un `PlotSpec` entier
tenu hors de l'historique jusqu'à validation — le même principe que
`NoteEdit`/`HeaderEdit` (saisie différée, tampon local, commande envoyée
seulement à la validation), généralisé à un objet entier plutôt qu'à un seul
champ : le panneau mélange une douzaine de petits widgets (titre, axes,
style de chaque série, légende) qui doivent tous voir et modifier le **même**
brouillon. `AppState::begin_plot_edit(graph)`, appelée à chaque image par
`app::plot_settings::plot_settings_window` avant de peindre quoi que ce soit, n'écrase le
brouillon que s'il n'y en a pas encore ou qu'il portait sur un autre graphe
— repeindre la même image sur le même graphe ne doit jamais effacer une
saisie en cours, et changer de graphe ne doit jamais laisser fuiter le
brouillon du précédent. Un brouillon **sans saisie en cours** (égal à
`PlotEdit::base`, le spec du modèle dont il est parti) suit en revanche le
modèle : après Annuler, Rétablir, un script ou un glisser-déposer de
colonne, la fenêtre montre le spec à jour au lieu de valeurs périmées
qu'une validation suivante réécrirait par-dessus l'annulation. `AppState::commit_plot_edit` envoie
`Action::SetPlotSpec { graph, spec }` → `Command::SetPlotSpec` — mais
seulement si le brouillon diverge du spec courant du modèle (comparaison
`PartialEq` sur `PlotSpec`, qui dérive proprement jusqu'aux feuilles) :
sans cette garde, relâcher un curseur qui n'a pas bougé ou perdre le focus
d'un champ texte non modifié empilerait une entrée d'historique identique à
la précédente.

**Stratégie anti-inflation d'historique**, trois familles de widgets liées
chacune à un déclencheur de validation différent, plutôt qu'un commit à
chaque frappe/pixel :

- **texte** (titres, libellés d'axe, couleurs en `#RRGGBB`) : `lost_focus()`
  — le champ est modifié en direct dans le brouillon à chaque frappe (retour
  visuel immédiat), mais `commit_plot_edit` n'est appelé qu'à la perte de
  focus, exactement comme `note_contents`/`cell_widget` ;
- **glissé numérique** (épaisseur de trait, taille de marqueur, bornes
  d'axe manuelles, `egui::DragValue`) : `drag_stopped() || lost_focus()` —
  la valeur change à chaque pixel de glisser dans le brouillon, mais ne
  s'envoie qu'au relâchement de la souris ou à la validation clavier ;
- **case à cocher / menu déroulant** (échelle, visibilité, grille, type de
  tracé, position de légende...) : un seul événement discret par clic,
  validé tout de suite — rien à débouncer, il n'y a par construction jamais
  de rafale d'événements `changed()` pour ces widgets-là.

Chaque widget de `app::plot_settings::plot_settings_contents` (et de ses fonctions auxiliaires
`axis_editor`/`series_editor`/`legend_editor`) accumule dans un simple
booléen local `commit`, mis à `true` par n'importe quel widget qui vient de
finir une interaction ; `commit_plot_edit` n'est appelé qu'une seule fois, à
la toute fin de la fonction, jamais widget par widget.

**Ce qu'édite la fenêtre**, en ne touchant que des champs qui existent
réellement dans `stokesforge-plot` (lu avant d'écrire, comme demandé) :

- **Graphe** : `PlotSpec::title`, `background` ;
- **Zone de tracé** — `PlotSpec::panels`[0] seulement, voir plus bas — :
  `PanelSpec::title`, grille des axes du bas et de gauche ;
- **Axes** : pour l'axe du bas, celui de gauche, et celui de droite si
  `PanelSpec::uses_right_axis()` (l'axe du haut, masqué par défaut et sans
  UI dans ce banc pour l'activer, n'a pas de section) : `AxisSpec::label`,
  `unit`, `scale` (`AxisScale::Linear`/`Log`), `visible`, `grid`,
  `inverted`, `auto_range`, et `range_min`/`range_max` quand `auto_range`
  est décoché ;
- **Séries** (#179 : fenêtre à part, `app/curve_settings.rs` ; `PlotSettingsTab::Series`
  n'est plus dans `PlotSettingsTab::ALL` mais choisit cette page dans
  `plot_settings_window` — même brouillon, autre titre `curve-window-title`, autre
  identité egui et autre taille ; ouverte par le double-clic sur une courbe de la
  figure ou sur sa ligne de l'Object Manager, `Properties…` et Graph › Properties ›
  Series…). Liste à gauche rangée comme l'Object Manager (#197, `PanelSpec::slots`) :
  une série seule est une ligne `list_row`, un groupe est un **nœud repliable**
  (réduire ne masque aucune courbe) dont les membres sont en branche
  `tree_lines` ; Move Up / Move Down changent le rang dans le groupe (ou sautent
  un groupe entier), réglages à droite : le **groupe** de la série
  (liste déroulante Aucun / nom du groupe / New group, et « Leave Group » qui la
  détache ; groupe créé à l'ajout simultané, #177 ; entrer dans un groupe place
  la série à la suite de ses membres), puis les gestes du groupe (`group_editor`) :
  son **nom** (champ validé à la perte de focus, vide = « Group n »), Move Group
  Up / Down, **Ungroup** (les séries restent, simples, avec leur style) et
  **Delete Group** (le groupe et ses séries) ; et, pour une série
  groupée, la **portée** (`EditScope`) en clair — « This series only » ou
  « Whole group (n series) » (par défaut) — avec une phrase qui dit ce qui
  s'applique. Portée groupe : style et épaisseur du trait, forme et taille du
  symbole se recopient dans chaque membre (`propagate_style`) ; les réglages
  propres à une série (nom, visibilité, légende) sont **grisés**
  avec l'info-bulle `curve-scope-locked`, ses **couleurs sont masquées** (le
  nuancier les donne, #213) et le **type de tracé** reste réglable, pour la
  seule série choisie ; la phrase qui dit ce que la portée applique est sous un
  « ? » à côté du choix, plus en note ; le **nuancier** (#178) est actif.
  Portée série : seule la série bouge (c'est l'exception), ses couleurs
  reviennent, le nuancier est grisé (info-bulle). Série sans groupe : tout est
  actif, ses couleurs sont là et le **nuancier n'est pas affiché**. Nuanciers :
  fournis (`GRADIENT_PRESETS`), enregistrés (projet ou utilisateur, suffixe
  « (Project) » / « (User) », bouton Delete grisé hors nuancier enregistré) ou
  personnalisé à N couleurs (nom + cible + Save). Chaque entrée de la liste
  montre ses couleurs (`gradient_swatch`) avant son nom, et la pastille se clique comme le nom ; la bande dessous
  montre celui du groupe. Il est appliqué **dès qu'on le choisit** (ou qu'on
  change un arrêt), sans bouton : il répartit les couleurs dans l'ordre de la
  liste du brouillon, que OK valide et Cancel défait. Le **groupe retient** ses
  arrêts (`PanelSpec::group_gradients`, dans le `.stf`) : la liste en relit le
  choix (`identify_choice`), **vide** — et la bande grisée — tant qu'aucun n'a
  été choisi ; une série qui rejoint le groupe (`set_series_group`) en reçoit la
  couleur. Dans l'Object Manager, le nœud sous le calque s'appelle « Series » ; un
  groupe y est un **nœud parent comme les autres** (#197) : triangle de repli
  (repli initial = celui de « Series », #139), case de visibilité mixte qui ne
  commande que ses membres (`VisibilityTarget::SeriesGroup`), nom éditable sur
  place (F2 ou Rename, Entrée valide, Échap annule), sélection (la `GraphView`
  garde `selected_group`, valable tant que la série qui le désigne est
  sélectionnée ; les poignées de toutes ses courbes sont peintes), double-clic
  = fenêtre des séries ouverte sur sa première série, clic droit
  (`object_node_menu` : Select Members, Rename, Move Up / Down, Ungroup, Hide,
  Delete, Properties…). Dans la fenêtre des séries, l'en-tête du groupe suit la
  même règle (#208) : surligné quand le groupe est réglé (portée « tout le
  groupe »), un clic le choisit (première série, portée groupe). Suppr supprime le groupe **et ses séries** (le geste
  distinct de la dissolution) ; Ctrl+↑/↓ le déplace en bloc. Actions :
  `RenameSeriesGroup`, `UngroupSeries`, `MoveSeriesGroup`, `MoveSeries`,
  `RemovePlotObject`.
  Ancienne description de la liste : (`egui::ScrollArea` bornée en hauteur,
  `selectable_label` par série, libellé `state::series_list_label` : le nom
  de la série s'il est renseigné, sinon le nom long de la colonne Y, sinon
  son nom de nœud, sinon « (référence rompue) » — le même calcul que
  `label_for` en privé dans `stokesforge-core/src/resolve.rs`, dupliqué plutôt
  que rendu public parce que c'est une règle d'affichage, pas une règle de
  résolution) — puis, pour la série choisie : `SeriesSpec::name`,
  `plot_type` (`PlotType`, toutes les variantes sauf `Unknown`, voir plus
  bas), `visible`, `show_in_legend`, `line` (`LineSpec::color`, `style`,
  `width`) et `symbol` (`SymbolSpec::shape`, `size`, `color`) ;
- **Légende** : `LegendSpec::visible`, `position`.

**`PlotType::Unknown` ne figure jamais dans le menu déroulant**, mais une
série qui le porte s'affiche quand même — le champ type de tracé devient un
texte en lecture seule (« type inconnu de cette version, non modifiable »),
le reste (nom, visibilité, style...) reste éditable normalement. C'est
l'invariant 10 (« un défaut de donnée dégrade, il ne plante pas ») appliqué
à cette fenêtre : proposer `Unknown` dans la liste laisserait croire qu'on peut
y revenir, alors que rien ne sait reconstruire une variante dont ce lecteur
ne connaît que le nom et des octets bruts (C5, niveau 2,
`guide-de-correction-robustesse.md` §F3).

**Ce qui n'est pas fait**, explicitement hors périmètre de ce chantier :

- **pas de largeur de barre ni de nombre de classes d'histogramme** —
  `SeriesSpec::bar_width`/`histogram_bins` existent dans `stokesforge-plot` mais
  n'ont pas de widget ici, laissé à une itération suivante ;
- **pas d'ajout/suppression de série ou de panneau depuis la fenêtre** —
  ça reste le rôle du sous-menu « Plot » et du glisser-déposer déjà
  décrits plus haut ;
- **zoom/pan n'y entrent pas, et ne doivent jamais y entrer** —
  `stokesforge_render_egui::ViewState` vit sur `GraphView`, hors du `PlotSpec`
  (`docs/invariants.md`, `.claude/rules.md`) : cette fenêtre n'a et n'aura
  aucun champ de navigation, une confusion entre les deux romprait la
  frontière rendu/modèle que tout le reste du chantier de rendu tient.


## Règles communes avec le rendu

Deux règles ne sont **pas** réécrites dans l'interface (issue #25) :
la syntaxe des couleurs `#RRGGBB[AA]` (`stokesforge_plot::parse_hex`, dont
`color_picker::parse_hex` n'est qu'un adaptateur) et le nom affiché d'une série
(`stokesforge_core::series_label`, appelée par la légende **et** par la liste des
séries des propriétés du graphe via `series_list_label`). Les transformations
RGB/HSL restent propres au sélecteur.

