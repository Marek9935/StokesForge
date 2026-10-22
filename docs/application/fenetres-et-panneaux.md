# Application : fenêtres et panneaux

Suite du sommaire [`README.md`](README.md). Contenu : fenêtres flottantes confinées, débordement, onglets/listes/sélection, géométrie, maximiser et détacher, connecteurs d'arbre, panneaux rétractables et détachés. Le catalogue des fenêtres est dans [`../fenetres.md`](../fenetres.md).

### Fenêtres flottantes confinées

Le Project Manager, l’Object Manager et la console Python sont des panneaux
fixes à gauche, à droite et en bas (#100). Leur visibilité se commande par
View ; aucun en-tête ne porte de bouton Fermer. L'ordre des
panneaux fait la mise en page egui (le premier posé est le plus extérieur) ;
il est décidé dans `StokesForgeApp::ui`, et nulle part ailleurs : barre de
menu (`Panel::top`, `app/menus.rs`), barres d'outils (`toolbar.rs`), barre
d'état (`Panel::bottom`, `app/menus.rs`), console de script (`Panel::bottom`,
`app/script_console.rs`, posée avant le Project Manager pour courir sous lui
sur toute la largeur), Project Manager (`Panel::left`,
`app/project_manager.rs`, sauf détaché), Object Manager (`Panel::right`,
`app/object_manager_ui.rs`, sauf détaché), puis le `CentralPanel`. Le test
`les_panneaux_fixes_se_posent_dans_l_ordre_de_ui` (`app.rs`) tient cet ordre
par les rectangles des panneaux. Les quatre autres
natures de fenêtre de contenu — `Tab::Book` (classeur), `Tab::Graph`,
`Tab::Matrix`, `Tab::Note`, chacune portant un `NodeId` — s'ouvrent par défaut
en fenêtres flottantes à l'intérieur du `CentralPanel`, une par
`StokesForgeApp::floating_tabs` (ordre de pile : `app/window_stack.rs`). Chacune est une `egui::Area` nue
(`show_floating_window`), pas une `egui::Window` : l'application possède seule
sa géométrie, et le cadre, le contour actif, le rognage, les poignées et la
géométrie persistée viennent du même rectangle. Les `egui::Window` d'avant
avaient toutes leurs fonctions coupées puis refaites à la main, et chaque
image devait accorder le rectangle demandé avec celui qu'egui recalait —
d'où les bugs de contour décalé et de débordement sur les panneaux
(retours du 2026-09-24, points 1 et 2 — rapport supprimé, dans l'historique git).

L'aire qui les porte a une taille logique **fixe** (`WORKSPACE_SIZE`,
1920×1080), indépendante de la taille de la fenêtre principale : en dessous,
elle défile (`StokesForgeApp::workspace_scroll`) plutôt que d'écraser les fenêtres
qu'elle contient contre ses bords. `window_positions`/`window_sizes` et la
géométrie persistée dans le projet sont exprimées dans ces coordonnées
logiques, bornées à l'espace de travail par `clamp_to_workspace` (la seule
borne), puis converties en coordonnées écran à la peinture. Le rendu est
rogné à la portion visible de l'espace de travail (hors panneaux fixes et
barres de défilement), pour le dessin comme pour les clics : une fenêtre à
cheval sur le bord de la vue ne peint ni ne capte rien sous le Project
Manager ou la console. Entièrement hors de la vue, elle n'est pas peinte du
tout, et redéfiler la fait réapparaître à sa position logique inchangée. Rouvrir depuis le Project Manager une entrée déjà ouverte
mais défilée hors vue fait défiler l'espace de travail jusqu'à elle. Une
fenêtre nouvellement ouverte apparaît en cascade par rapport à la précédente
plutôt que superposée à elle. La taille fixe est pour l'instant une constante
du code, pas encore réglable par l'utilisateur (persistance, UI de réglage) —
à faire.

Un onglet par fenêtre **ouverte**, quelle que soit sa nature : `StokesForgeApp::
open_tab` est le seul endroit qui décide « ouvrir » vs « focaliser » (cliquer
une seconde fois la même entrée du Project Manager ne duplique jamais son
onglet, elle est ramenée au premier plan). Un onglet `Tab::Graph` par graphe
ouvert garde son propre `stokesforge_render_egui::GraphView` (donc son propre
cache de formes — `StokesForgeApp::graph_views` est une table indexée par
`NodeId`). Son contenu réserve la hauteur des commandes sous la figure : le
rectangle rendu reste stable au survol, donc le
cache ne se reconstruit pas et la fenêtre ne dérive pas en hauteur. Une
nouvelle fenêtre de graphe part assez grande pour contenir la figure de
référence (480×360), ces contrôles et son en-tête ; si l'utilisateur la
rétrécit, la figure conserve sa taille et sa géométrie. Elle reste centrée,
et ses parties hors de la zone allouée sont tronquées par le découpage
d'egui, pour le dessin comme pour les interactions. Elle ne recouvre donc
ni l'en-tête ni les commandes placées sous elle. Les compteurs de cache et
le résumé technique de la figure ne sont plus affichés.

La barre de titre maison centre le nom du contenu ; il n'est rogné que
symétriquement, par la plus large des réserves réellement occupées — flèche de
repli à gauche, icônes peintes à droite (un dialogue ne réserve que la croix,
#50). Celle d'un graphe garde à
gauche la flèche de repli d'une fenêtre egui : elle masque ou réaffiche le
seul contenu de ce graphe, sans modifier le projet, la géométrie enregistrée
ni les autres fenêtres.

Chaque fenêtre flottante peut aussi être maximisée (plein cadre du
`CentralPanel`, `StokesForgeApp::maximized_tab`) ou détachée dans une vraie fenêtre
OS séparée (`StokesForgeApp::detach_tab`/`reattach_tab`, pour le multi-écran).

Le menu File propose la création d'un nouveau classeur (Add Workbook), d'un dossier
à la racine (Add Folder, #326) et l'import de fichiers. La position et
la taille des fenêtres de contenu sont enregistrées dans le projet (voir
« Géométrie des fenêtres »). L'éditeur de matrice reste un espace réservé.


### Débordement des fenêtres de contenu

Toutes les fenêtres de contenu flottantes et détachées partagent un rectangle
extérieur unique : leur contenu est peint dans un sous-espace borné qui ne
peut agrandir le cadre au-delà des poignées de redimensionnement. La règle
intérieure dépend de la nature du contenu. Dans un classeur, la grille et la
bande d'onglets de feuille défilent chacune horizontalement, indépendamment,
lorsqu'elles dépassent. Du haut vers le bas : grille, onglets de feuille,
barre horizontale de la grille ; la barre verticale occupe le bord droit sur
toute la hauteur. La bande d'onglets est retranchée du bas du classeur
avant la construction de la grille, qui ne peut donc plus la repousser hors du
cadre (`book_contents`) ; ce découpage n'utilise pas `egui::Panel`, qui
remplace le rognage de son parent au lieu de le combiner et dessinait les
onglets sous la vue d'une fenêtre coupée. Les onglets
gardent leur taille naturelle (`sheet_tabs`, enveloppe de `tab_strip`, `crates/stokesforge-app/src/app/workbook.rs`) —
ils ne rétrécissent pas et ne se replient pas dans un menu « … ». La bande n'a
**pas** de barre de défilement : quand des onglets sont cachés, deux boutons
« < » et « > » apparaissent à sa gauche et font apparaître l'onglet caché
suivant de ce côté (chacun grisé s'il n'y a rien à montrer de son côté) ; ils
disparaissent quand tout tient. Changer de feuille (au clic, au clavier ou
par script) fait défiler la bande **du plus petit décalage** qui montre
entièrement l'onglet sélectionné et ses deux voisins — cliquer sur le dernier
onglet visible ne révèle qu'un onglet de plus (`scroll_tab_strip_to_selected`,
issue #1). Aspect (#169) : onglets plats, sans cadre ; l'actif est écrit
en gras et dans l'accent et porte un trait d'accent du côté du contenu (`TabStripSide` :
sous le libellé pour les pages des propriétés, au-dessus pour les feuilles),
un filet gris sépare la bande du contenu. Les axes de la page « Axes » des
propriétés sont aussi un jeu d'onglets (`tab_strip`, `Top`).

#### Onglets, listes et sélection : quatre aspects, quatre rôles (#169)

| Rôle | Brique | Aspect |
|---|---|---|
| jeu d'onglets (feuilles, pages, axes) | `tab_strip` / `tab_strip_with` + `TabStripSide` | plat, actif en gras et accent, trait du côté du contenu |
| entrée de liste ou d'arbre (Project Manager, Object Manager, liste des séries des propriétés) | `app/list_row.rs` : `list_row`, ou `ListRowBackground` + `list_row_label` pour une ligne composée | ligne pleine largeur, fond pâle d'accent, filet d'accent à gauche, gras |
| choix exclusif dans un menu, une liste déroulante ou le sélecteur de couleur | `selectable_label` / `selectable_value` d'egui | fond de bouton sélectionné |
| bascule d'outil d'une barre | `Button::selected` (`toolbar::tool_button`) | bouton enfoncé |

Garde `aucun_bouton_selectionnable_hors_d_un_menu` (`tests/gardes.rs`) : un
bouton sélectionnable d'egui hors d'un menu, d'un `show_ui` ou d'une fonction
`*picker*`/`*popup*` fait échouer les tests.

Le gras est **toujours** `theme::bold` (axe `wght` de Noto Sans Variable,
graisse 600) : `RichText::strong` d'egui ne graisse rien, il est interdit
(`aucun_strong_d_egui_hors_du_theme`). Un titre de section de formulaire
(propriétés du graphe, assistant d'import) passe par `theme::section_title`
(espace de 6 pt puis gras) et sépare seul ses groupes, sans `ui.separator()`
juste avant (`aucun_separateur_avant_un_titre_de_section`) ; l'étape courante
de l'assistant d'import et l'élément sélectionné d'une liste sont aussi en
gras. Une police sans axe `wght` (paire Fira ou Libertinus, police système)
reste en graisse normale, sans erreur. Toute couleur d'état ou de chrome
vient de `theme.rs` (`accent`, `tab_rule_color`, `tab_hover_mark_color`,
`list_row_selected_fill`, `list_row_hover_fill`, `file_drop_outline_color`…)
ou des `Visuals` qu'il pose (`error_fg_color`, `warn_fg_color`,
`selection`) : aucun `Color32::from_…` ni teinte nommée hors de `theme.rs` et
`color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`). Les notes
et matrices restent bornées au rectangle commun. Dans un
graphe (`graph_view_contents`,
`crates/stokesforge-app/src/app/graph_window.rs`), la figure garde le ratio de
`DEFAULT_FIGURE_SIZE` (`stokesforge-render::pipeline`, 4:3) mais se met à l'échelle
« contain » dans l'espace disponible — maximisée en prenant la dimension
limitante, largeur ou hauteur, aussi bien en agrandissant qu'en rétrécissant
(F11). Aucun rognage ni défilement pour le graphe : la figure entière reste
toujours visible. C'est un **zoom** : la disposition (graduations, marges,
légende, mesures de texte) est toujours calculée à la taille de référence
`DEFAULT_FIGURE_SIZE` (480×360), puis le résultat entier, texte et épaisseurs
compris, est agrandi ou réduit uniformément (`GraphView::paint_scaled`,
`stokesforge-render-egui`). Redimensionner la fenêtre ne redispose donc rien, et
ne reconstruit pas le cache de formes. La règle est identique en fenêtre
flottante et détachée, qui partagent la même fonction.

Gestes sur une feuille (#106, retrait de F2 selon #130) : double-clic sur l'onglet ou menu
Renommer (édition en place, `AppState::renaming_sheet`, validée par Entrée ou
en quittant le champ, abandonnée par Échap) ; Ctrl+D ou menu Dupliquer (« … copie
N », premier numéro libre, la copie est affichée) ; Supprimer par le menu seul,
sans confirmation (annulable, #229 ; le message nomme les graphes qui en
traçaient des colonnes), grisé sur la dernière feuille. Nom vide ou déjà pris : ancien nom conservé et message,
aucune entrée d'historique. Mêmes gestes dans le menu *Workbook* (miroir du clic
droit, `workbook_menu`) ; la bande les offre par `tab_strip_with`
(`TabOps`/`TabEvent`), qui ne touche jamais au modèle.

Pour une nouvelle nature de fenêtre, conserver cette borne commune puis
définir avec l'utilisateur quelles zones défilent et quelles zones sont
rognées avant de brancher son contenu.

Le `Frame` de ces fenêtres n'a pas de marge verticale
(`WINDOW_CONTENT_MARGIN`, `crates/stokesforge-app/src/app/window_chrome.rs`) : la bande d'en-tête
et les séparateurs du contenu fournissent déjà leur propre espacement, une
marge par défaut n'ajoutait qu'un bandeau blanc mort au-dessus et en dessous.
La marge horizontale reste. Toutes les barres de défilement suivent le
standard de `theme::scroll_style` (voir § Thème).

La barre horizontale de la grille est **déportée sous les onglets de
feuille**, comme dans un tableur : la zone de défilement de la grille
(`show_horizontally_scrollable_grid`) masque sa propre barre, et
`show_grid_horizontal_bar` pose dans la bande réservée en bas de la fenêtre
une `ScrollArea` horizontale vide qui la reflète ; un glisser sur cette barre
est appliqué à la grille à l'image suivante.

La barre verticale de la grille est **déportée** : la table défile elle-même
(ce qui fige son en-tête) mais sa barre est masquée, car dans la zone de
défilement horizontale elle suivrait la largeur des colonnes et sortirait de
la vue. `book_contents` réserve une bande au bord droit de la fenêtre, sur
toute sa hauteur (en-tête et onglets de feuille compris), et
`show_grid_vertical_bar` y pose une `ScrollArea` verticale vide qui reflète la
table, contenu et décalage mis à l'échelle de la barre
(`grid_vertical_bar_scale`) ; un glisser sur cette barre est appliqué à la
table à l'image suivante (`grid_contents`, `pending_vertical_offset`).

### Géométrie et état des fenêtres

Pour chaque fenêtre de contenu, la session retient (et le `.stf`
enregistre) sa **géométrie flottante** — position et taille dans l'espace de
travail — et son **état** : ouverte, maximisée ou fermée, repliée, rang
d'empilement (`WindowGeometry::view`, voir `docs/persistance.md`). Les deux
partent ensemble à la fin de chaque image (`StokesForgeApp::sync_window_views`),
seulement quand ils changent. La géométrie flottante ne bouge jamais quand la
fenêtre est fermée, maximisée ou détachée : `StokesForgeApp::floating_rects` la
conserve et prime sur l'instantané, que `AppState::set_window_geometry` ne
rafraîchit pas.

Le **détachement ne se retient pas** : une fenêtre détachée est enregistrée
ouverte et revient flottante au rechargement ; la position et la taille de sa
fenêtre OS ne sont ni écrites ni relues. Au démarrage, `StokesForgeApp::new`
rétablit les fenêtres ouvertes (dans l'ordre d'empilement), la maximisée et
les repliées ; sans état enregistré, il ouvre le premier classeur et le
premier graphe.


### Maximiser et détacher une fenêtre

Les fenêtres de contenu proposent dans leur en-tête des icônes pour
maximiser, détacher et fermer, accompagnées d'infobulles. Le classeur et le
graphe utilisent les mêmes commandes. La maximisation remplit le
`CentralPanel` ; le Project Manager et la console restent accessibles.
Restaurer rend à la fenêtre sa disposition flottante. Le menu Window
permet aussi de maximiser ou restaurer la fenêtre active.

### Connecteurs d'arbre en L (#112)

Les arbres du Project Manager et de l'Object Manager n'utilisent plus la ligne
d'indentation d'egui (qui descendait jusqu'en bas du corps d'un nœud). Ils
passent par un seul composant, `tree_lines.rs` : `begin_tree` coupe
`indent_has_left_vline` à la racine, chaque corps de nœud s'écrit dans
`tree_children`, et chaque enfant direct déclare le centre de sa rangée
(`TreeBranch::mark`). Le trait vertical va du haut du corps au dernier enfant
direct (`└`), chaque enfant reçoit un trait horizontal (`├`/`└`), et le trait
continue à travers le sous-arbre d'un enfant tant qu'un frère suit. Les
traits sont peints au `Painter` (couleur `noninteractive.bg_stroke`, alignés
sur le pixel). Un nœud qui contient un arbre doit renvoyer le centre de sa
rangée à son parent ; limite : le vertical part du haut du corps, pas du bas
de la flèche du parent.

### Panneaux rétractables et détachés (#100)

Les trois panneaux partagent `panel_header` (`app/window_chrome.rs`) : titre
compact, Détacher, Replier/Déployer. L'ordre des boutons dépend du côté :
Replier/Déployer vers l'espace de travail, Détacher vers le bord extérieur
pour les gestionnaires. La console garde Replier/Déployer puis Détacher
côte à côte à gauche quand elle est déployée (#142). Les trois panneaux
repliés ne montrent que Déployer : Détacher est masqué et sa place est rendue
au titre. Il faut déployer le panneau pour accéder à son bouton Détacher.
La bande repliée de la console porte une séparation supérieure sur toute la largeur, du même style que celles
des bandes des gestionnaires.
Les bandes verticales repliées gardent Déployer en haut et le titre dessous.
PM et OM se replient en une bande latérale avec titre vertical centré dans
l’espace disponible sous le bouton ; la console se replie en une barre
horizontale. Les trois bandes font 16 points d’épaisseur, comme leurs icônes,
centrées sans marge de panneau supplémentaire. Le panneau déployé reste
redimensionnable
(largeur de PM/OM, hauteur de la console). Les bandes de repli ont des
identifiants egui distincts : leur taille n’écrase pas la dimension déployée.
Le repli laisse le panneau visible ; View le masque ou le réaffiche. Le
masquage conserve également le repli et le détachement. La visibilité de
l’OM reste une préférence ; celles du PM et de la console sont locales à
l’application, comme leur repli. Le mode flottant interne de l’OM est retiré.

Détacher ouvre une vraie fenêtre système sans décoration native, avec le
même cadre maison et un seul bouton Rattacher. Son titre permet le glisser,
ses bords le redimensionnement. Une demande de fermeture du compositeur
est annulée puis traitée comme un rattachement ; elle ne masque pas le
panneau. Le menu Window garde aussi Detach / Reattach Project Manager.

Les fenêtres séparées utilisent `show_viewport_deferred` : leur rendu est
indépendant de celui de la fenêtre principale. Le callback échange un
instantané de lecture et des requêtes avec celle-ci, via deux
`Arc<Mutex<_>>`, sans déplacer `AppState` ni créer une seconde voie de
mutation métier. Les requêtes réveillent explicitement la fenêtre
principale pour être traitées même quand elle est inactive.
Dans l'autre sens, publier un instantané modifié réveille explicitement le
viewport lecteur (#104) : époque du projet, révision du modèle ou révision
des géométries. Le Project Manager est aussi réveillé si sa sélection, son
masquage ou l’onglet actif change. L’OM reçoit le graphe actif, sa sélection
et les filtres à chaque publication : il suit aussi un graphe détaché et
se réveille sans interaction dans sa propre fenêtre. Les gestes remontent
avec l’identifiant du graphe qui était peint. La console reçoit la saisie,
la sortie et l’état d’exécution (`ScriptConsoleModel`) ; les nouvelles frappes
sont drainées avant publication. Le moteur et le travail Python restent dans
l’`AppState` principal : détacher, rattacher, replier ou masquer la console
ne les interrompt pas. La racine sonde le résultat tant qu’un script tourne,
y compris quand la console est masquée. Une commande appliquée en fin d'image
programme l'image racine qui publiera son résultat ; aucun réveil périodique
n'est ajouté au repos. Les tests sans fenêtre vérifient la demande ciblée
et le retour au repos ; les tests transversaux des trois panneaux sont dans
`app/panel_tests.rs` (clics, tailles, callbacks différés, console sous Python).
Le redessin réel des fenêtres OS reste à vérifier manuellement.

### Détachement générique des onglets de contenu

`StokesForgeApp::detach_tab`/`reattach_tab`/`show_detached_tabs` généralisent le
mécanisme ci-dessus à `Tab::Book`, `Tab::Graph`, `Tab::Matrix` et `Tab::Note` :
l'icône de détachement ouvre une fenêtre OS ; celle-ci propose une icône de
réattachement. Fermer une fenêtre de contenu détachée ferme uniquement sa
vue, sans la réattacher ni supprimer les données du projet. Elle reste
réouvrable depuis le Project Manager. Plusieurs contenus peuvent être
détachés simultanément, chacun dans son propre `ViewportId`.

**Tailles par défaut des fenêtres détachées** (`show_detached_tabs`) :
`Tab::Graph` ouvre en 560×540, ce qui contient la figure de référence, ses
commandes et son en-tête. Le reste (`Book`/`Matrix`/`Note`) partage une taille
uniforme de 420×360. La grille du classeur conserve les mêmes largeurs et
en-têtes dans les fenêtres flottantes et détachées ; une fenêtre étroite
défile horizontalement. La note et la matrice gardent la même taille par
cohérence avec les autres contenus.

**Une seule structure de `ViewModel`, pas une par nature de contenu.** Le PM
n'a qu'une forme de contenu (un arbre en lecture seule) ; les quatre natures
détachables ici sont franchement différentes (grille éditable virtualisée,
graphe interactif avec cache de formes, texte éditable, espace réservé).
`DetachedTabViewModel` porte un champ par nature (`view_state` pour un
classeur ou une note, `graph_view` pour un graphe) plutôt que quatre types disjoints à faire cohabiter dans la table des
onglets détachés — les champs inutilisés pour la nature du `Tab` servi
restent à leur valeur par défaut.

**La lecture du modèle est factorisée, l'édition transitoire ne l'est pas.**
`tab_contents` (`app/contents.rs`, avec `note_contents` et `matrix_contents` ;
partagée par les fenêtres flottantes et la zone de maximisation)
prend `&mut AppState`, que la fermeture `'static`, `Send + Sync` de
`show_viewport_deferred` ne peut pas capturer — le même problème déjà résolu
pour le PM (mettre `AppState` entière derrière un verrou déborderait le
problème à résoudre, pour un onglet comme pour l'autre). Le contournement :
`DetachedTabViewModel::snapshot`, un clone de `Snapshot` (bon marché),
resynchronisé depuis `self.state` à chaque image avant l'appel — même schéma
que `ProjectManagerViewModel::tree`. Toute lecture pure passe par des
fonctions libres de `stokesforge_app_state::state`, factorisées sur un `&Tree` pour
l'occasion (`cell_text_in`, `visible_columns_in`, `row_count_in`,
`worksheets_of_in`, `note_text_in`, `node_title_in`) : le corps exact de la
méthode `AppState` correspondante, pas une réécriture — le dock et une
fenêtre détachée affichent donc littéralement la même règle.

Le classeur détaché a un `AppState` de vue local, synchronisé avec l'instantané
de la racine. Il appelle les mêmes `book_contents`/`grid_contents` que le
classeur flottant ; disposition, en-tête, sélection, menus et raccourcis de
grille partagent ainsi le même code. Sa sélection et sa saisie restent locales
à la fenêtre. Avant de traiter ses actions, la racine reprend leur contexte
de grille, puis applique les mêmes commandes annulables. La note suit le même
motif : sa fenêtre détachée est une vue `AppState` locale (construite par
`fork_for_detached_view`) et appelle `note_contents`, comme la fenêtre
flottante ; seul le tampon de saisie (`note_editing`) est local à la fenêtre,
et la validation part en `Action::SetNoteText`, donc en commande annulable.
L'espace réservé de la matrice, `matrix_contents`, ne lit que l'arbre et sert
tel quel aux deux fenêtres : plus aucune fonction de contenu n'est propre au
détaché.

`Tab::Graph` échappe à cette limite : `GraphView` n'a pas de verrou global à
`AppState` à respecter (son cache de formes et sa navigation zoom/pan sont
déjà scopés par graphe, dans `StokesForgeApp::graph_views`) — `graph_view_contents`
est donc **littéralement** la même fonction pour une fenêtre flottante et une fenêtre
détachée ; la vue elle-même migre simplement hors de `graph_views` le temps
du détachement pour rester capturable par la fermeture sans dupliquer le
cache. `Tab::Matrix` n'a aucun état à faire voyager (espace réservé).

`Tab::Script` n'est pas détachable : sa saisie (`AppState::script_source`)
est un champ direct d'`AppState`, pas un tampon différé comme
`CellEdit`/`NoteEdit`, et son cycle de vie (fil détaché, `poll_script` à
appeler image après image) est bien plus couplé à `AppState` que les quatre
autres natures — le généraliser demanderait de faire voyager aussi l'état
d'exécution du script, hors périmètre de cette tâche.

