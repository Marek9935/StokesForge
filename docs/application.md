# Application

## `plotyx-app` — la coquille UI

**Ce que ce crate n'est pas** : une ébauche de l'interface finale. La grille
éditable à délégués, le panneau de propriétés et un système de graphes complet
(sélection multiple, réorganisation, fermeture persistée…) seront repris
entièrement plus tard ; rien ici ne doit les préempter.

Le banc est volontairement pauvre. Son rôle reste de prouver que
l'intégration bout en bout tient — modèle → commande → annulation → sauvegarde
→ rechargement, dans une vraie fenêtre — **tant qu'une intégration ratée ne
coûte qu'une réécriture de ce crate**, et non celle de tout ce qu'on aurait
bâti par-dessus. Deux fondations techniques sont posées au-dessus de ce
principe (voir plus bas) : la disposition est en **fenêtres flottantes
confinées** (`egui::Window`), et Save/Load passent par un vrai sélecteur de
fichiers natif (`rfd`) — ce ne sont que des charpentes, pas les panneaux
définitifs qu'elles accueilleront.

Toolkit : `eframe` / `egui` (disposition et rendu), `rfd` (sélecteur de
fichiers natif).

### Fenêtres flottantes confinées

Le Project Manager et la console de script sont deux panneaux fixes
(`egui::Panel::left`/`egui::Panel::bottom`), jamais fermés. Les quatre autres
natures de fenêtre de contenu — `Tab::Book` (classeur), `Tab::Graph`,
`Tab::Matrix`, `Tab::Note`, chacune portant un `NodeId` — s'ouvrent par défaut
en fenêtres flottantes à l'intérieur du `CentralPanel`, une par
`PlotyxApp::floating_tabs`, dans `app.rs`. Chacune est une `egui::Area` nue
(`show_floating_window`), pas une `egui::Window` : l'application possède seule
sa géométrie, et le cadre, le contour actif, le rognage, les poignées et la
géométrie persistée viennent du même rectangle. Les `egui::Window` d'avant
avaient toutes leurs fonctions coupées puis refaites à la main, et chaque
image devait accorder le rectangle demandé avec celui qu'egui recalait —
d'où les bugs de contour décalé et de débordement sur les panneaux
(retours du 2026-09-24, points 1 et 2 — rapport supprimé, dans l'historique git).

L'aire qui les porte a une taille logique **fixe** (`WORKSPACE_SIZE`,
1920×1080), indépendante de la taille de la fenêtre principale : en dessous,
elle défile (`PlotyxApp::workspace_scroll`) plutôt que d'écraser les fenêtres
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

Un onglet par fenêtre **ouverte**, quelle que soit sa nature : `PlotyxApp::
open_tab` est le seul endroit qui décide « ouvrir » vs « focaliser » (cliquer
une seconde fois la même entrée du Project Manager ne duplique jamais son
onglet, elle est ramenée au premier plan). Un onglet `Tab::Graph` par graphe
ouvert garde son propre `plotyx_render_egui::GraphView` (donc son propre
cache de formes — `PlotyxApp::graph_views` est une table indexée par
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

La barre de titre maison centre le nom du contenu. Celle d'un graphe garde à
gauche la flèche de repli d'une fenêtre egui : elle masque ou réaffiche le
seul contenu de ce graphe, sans modifier le projet, la géométrie enregistrée
ni les autres fenêtres.

Chaque fenêtre flottante peut aussi être maximisée (plein cadre du
`CentralPanel`, `PlotyxApp::maximized_tab`) ou détachée dans une vraie fenêtre
OS séparée (`PlotyxApp::detach_tab`/`reattach_tab`, pour le multi-écran).

Le menu Fichier propose la création d'un nouveau classeur et l'import de
fichiers. La position et
la taille des fenêtres de contenu sont enregistrées dans le projet (voir
« Géométrie des fenêtres »). L'éditeur de matrice reste un espace réservé.

### Glisser-déposer de fichiers

Déposer des fichiers sur la fenêtre principale émet `Action::DropFiles`. Le
classeur flottant sous le curseur réclame le dépôt en se peignant
(`claim_file_drop`) : il reçoit une feuille par fichier, sinon un nouveau
classeur est créé. Les `.plx` ne sont jamais importés : `AppState` les met en
file (`take_projects_to_open`) et `app.rs` lance un second processus
`plotyx-app <chemin>`. Chaque fichier utilise le modèle qui le reconnaît,
sinon le dernier modèle utilisé dans cette session, sinon la recette permissive.
Il passe par la conversion et l'application communes à l'assistant — une
seule entrée « Importer » ; un fichier qui ne donne aucune valeur numérique
ouvre à la place la fenêtre d'import pré-remplie. Décision pure :
`state::file_drop_plan`. Le dépôt crée toujours de nouvelles feuilles, même
si la recette du dernier import remplaçait ou fusionnait une feuille. Le lien
« Régler l'import… » dans la barre d'état et Fichier › Régler le dernier
import… reprennent ses fichiers dans l'assistant.

Le même geste est câblé dans les fenêtres détachées : sur un classeur,
la destination reste ce classeur ; sur un graphe, une note, une matrice ou le
Project Manager, un nouveau classeur. Les dépôts sont isolés par viewport et
leurs actions reviennent à la session principale. Un dépôt est refusé tant
que les propriétés du graphe sont ouvertes, pour garder leur macro
d'historique indépendante de l'import. Les événements de dépôt sont
testés sans fenêtre ; le glisser-déposer réel (X11/Wayland, fenêtres OS
détachées) reste à valider manuellement.

### Assistant d'import

« Importer… » ouvre un assistant en six étapes façon Origin (`import_window`,
`ImportStep`), **dialogue modal de la brique commune** (`modal_dialog_with` :
même cadre, même barre de titre, même blocage du reste de l'interface que les
propriétés du graphe ; « Terminer » = bouton par défaut, « < Précédent » et
« Suivant > » dans le pied de page) : fichiers et modèle, format, en-tête et fin des données,
colonnes, métadonnées, destination. L'aperçu du fichier reste à droite à
toutes les étapes ; un clic droit sur une ligne règle l'import à cet endroit
(`AppState::import_line_action`). « Terminer » importe dès l'étape 1 avec le
modèle tel quel. Les octets des fichiers sont lus une fois (cache par chemin,
`ImportState::bytes`) ; chaque fichier montre avant l'import ses lignes lues ou
son erreur. L'en-tête est un seul réglage (`ImportRecipe::unify_header` ramène
`HeaderSkip::Lines` au découpage structuré) ; les rôles de colonne choisis à
l'étape 4 vivent dans la recette (`column_roles`). Un dépôt de fichiers pendant
que l'assistant est ouvert les ajoute à sa liste. « Terminer » lance
`Action::StartImport` : l'analyse des fichiers tourne sur un fil de fond (avec
progression « Import en cours… k/n », `AppState::poll_import` appelé à chaque
image) et les feuilles se créent à la fin, en une seule entrée « Importer ».
Raccourci : **Ctrl+"** (`SHORTCUT_IMPORT` : touche du 3 sans Maj sur AZERTY,
`SHORTCUT_IMPORT_QWERTY` : Ctrl+Maj+' sur QWERTY). Fichiers récents et dossier
mémorisé : `plotyx_import::RecentFiles` (`import_recent.json`, à côté des
recettes). « Fichier › Régler le dernier import… » (`Action::ReopenLastImport`)
rouvre l'assistant sur les fichiers et le modèle du dernier import — en
l'annulant d'abord s'il est encore au sommet de l'historique, jamais un travail
postérieur.

Le témoin « modifié » du modèle compare les réglages au modèle chargé ou au
dernier enregistrement réussi (`AppState::import_recipe_modified`). Enregistrer
le modèle retire ce témoin ; un échec d'écriture conserve la référence
précédente et signale l'erreur.

**Où vont les colonnes.** Dans le classeur actif, le mode d'import
(`ImportMode`) est : nouvelle feuille, remplacer la feuille active, ajouter des
colonnes, ou ajouter des lignes (la colonne *j* du fichier prolonge la colonne
*j* de la feuille). Plusieurs fichiers se répartissent selon `FilesLayout` : une
feuille par fichier, une seule feuille côte à côte (X Y X Y…), ou une seule
feuille avec le X du premier fichier partagé. Le plan (`plan_import`) est
calculé puis exécuté dans la même macro « Importer ». Par défaut, la première
colonne de chaque fichier reçoit le rôle X et les suivantes le rôle Y ; ces
rôles sont conservés lors du rétablissement de l'import après annulation.

**Fin des données.** La coupure automatique s'arme au premier X numérique,
puis coupe au premier X non numérique (ligne vide comprise). Le texte et les
lignes vides qui précèdent ce premier nombre sont conservés ; un texte dans Y
ne coupe pas. Le préfixe `skip_chars` est retiré avant ce test ; avec le
séparateur espaces, X est le premier jeton non vide. L'étape « En-tête et fin
des données » propose aussi « Aucune coupure », première ligne vide, marqueur,
N lignes après l'en-tête ou N lignes avant la fin. Le clic droit sur l'aperçu
permet d'annuler la coupure automatique ; l'aperçu et l'import utilisent le
même découpage (`plotyx_import::layout`, `data_end`).

**Plusieurs blocs.** `BlockSplit` découpe un fichier qui enchaîne plusieurs
spectres (à chaque ligne commençant par un texte comme `TITLE`, ou aux lignes
vides) : `parse_blocks` rend un tableau par bloc, chacun avec son en-tête, et
l'import crée une feuille par bloc (`fichier_1`, `fichier_2`…) ou les répartit
selon `FilesLayout`. L'aperçu montre le premier bloc et le nombre de blocs.

**Actualiser.** Chaque colonne importée garde sa source
(`ColumnMeta::import_source` : chemin, recette en JSON, index de colonne).
« Fichier › Actualiser depuis le fichier source » (`Action::RefreshFromSource`)
relit les fichiers et réécrit les valeurs **en place** : les colonnes gardent
leur identité, les graphes restent valides. Les lignes de la feuille suivent le
plus long des nouveaux contenus ; elles ne raccourcissent que si toutes les
colonnes sont actualisées.

### Glisser-déposer : le backend de fenêtre

**winit 0.30 n'émet aucun dépôt de fichier sous Wayland** (seulement X11,
Windows, macOS) : sous une session Wayland, rien ne se passait au lâcher.
`main.rs` (`prefer_x11`) demande donc le backend X11 (XWayland) sous Linux dès
que `DISPLAY` existe ; `PLOTYX_WAYLAND=1` garde le Wayland natif (sans dépôt de
fichiers). La logique de dépôt elle-même est testée sans fenêtre
(`un_depot_est_reclame_par_le_classeur_sous_le_curseur`) ; le geste réel depuis
un gestionnaire de fichiers reste à valider à la main.

### Ligne « Metadata » de l'en-tête

Elle se masque et se réaffiche (Fenêtre › Afficher la ligne Metadata des
colonnes, ou clic droit sur son libellé) : réglage de vue gardé dans la mémoire
d'egui, partagé par les classeurs flottants et détachés, jamais dans le modèle.

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
gardent leur taille naturelle (`sheet_tabs`, enveloppe de `tab_strip`, `crates/plotyx-app/src/app.rs`) —
ils ne rétrécissent pas et ne se replient pas dans un menu « … ». La bande n'a
**pas** de barre de défilement : quand des onglets sont cachés, deux boutons
« < » et « > » apparaissent à sa gauche et font apparaître l'onglet caché
suivant de ce côté (chacun grisé s'il n'y a rien à montrer de son côté) ; ils
disparaissent quand tout tient. Changer de feuille (au clic, au clavier ou
par script) fait défiler la bande **du plus petit décalage** qui montre
entièrement l'onglet sélectionné et ses deux voisins — cliquer sur le dernier
onglet visible ne révèle qu'un onglet de plus (`scroll_tab_strip_to_selected`,
issue #1). Les notes et matrices restent bornées au rectangle commun. Dans un
graphe (`graph_view_contents`,
`crates/plotyx-app/src/app.rs`), la figure garde le ratio de
`DEFAULT_FIGURE_SIZE` (`plotyx-render::pipeline`, 4:3) mais se met à l'échelle
« contain » dans l'espace disponible — maximisée en prenant la dimension
limitante, largeur ou hauteur, aussi bien en agrandissant qu'en rétrécissant
(F11). Aucun rognage ni défilement pour le graphe : la figure entière reste
toujours visible. C'est un **zoom** : la disposition (graduations, marges,
légende, mesures de texte) est toujours calculée à la taille de référence
`DEFAULT_FIGURE_SIZE` (480×360), puis le résultat entier, texte et épaisseurs
compris, est agrandi ou réduit uniformément (`GraphView::paint_scaled`,
`plotyx-render-egui`). Redimensionner la fenêtre ne redispose donc rien, et
ne reconstruit pas le cache de formes. La règle est identique en fenêtre
flottante et détachée, qui partagent la même fonction.

Pour une nouvelle nature de fenêtre, conserver cette borne commune puis
définir avec l'utilisateur quelles zones défilent et quelles zones sont
rognées avant de brancher son contenu.

Le `Frame` de ces fenêtres n'a pas de marge verticale
(`WINDOW_CONTENT_MARGIN`, `crates/plotyx-app/src/app.rs`) : la bande d'en-tête
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

Pour chaque fenêtre de contenu, la session retient (et le `.plotyx`
enregistre) sa **géométrie flottante** — position et taille dans l'espace de
travail — et son **état** : ouverte, maximisée ou fermée, repliée, rang
d'empilement (`WindowGeometry::view`, voir `docs/persistance.md`). Les deux
partent ensemble à la fin de chaque image (`PlotyxApp::sync_window_views`),
seulement quand ils changent. La géométrie flottante ne bouge jamais quand la
fenêtre est fermée, maximisée ou détachée : `PlotyxApp::floating_rects` la
conserve et prime sur l'instantané, que `AppState::set_window_geometry` ne
rafraîchit pas.

Le **détachement ne se retient pas** : une fenêtre détachée est enregistrée
ouverte et revient flottante au rechargement ; la position et la taille de sa
fenêtre OS ne sont ni écrites ni relues. Au démarrage, `PlotyxApp::new`
rétablit les fenêtres ouvertes (dans l'ordre d'empilement), la maximisée et
les repliées ; sans état enregistré, il ouvre le premier classeur et le
premier graphe.

### Project Manager et fenêtres par contenu

Le Project Manager affiche l'arbre du projet **arrêté** à `Workbook`,
`Graph`, `Matrix`, `Note` : ces quatre natures sont des feuilles cliquables de
ce panneau, jamais développées davantage (pas de descente dans les feuilles
d'un classeur, pas de descente dans le contenu d'un graphe) ; les dossiers
sont des nœuds dépliables, respectant la hiérarchie réelle de l'arbre.
`AppState::project_tree` construit cette vue en généralisant le principe déjà
posé par `AppState::graphs`/`AppState::sheets` (interroger l'instantané, ne
jamais dupliquer l'accès à l'arbre) plutôt que de le réinventer : elle rend
une structure légère (`ProjectTreeNode`, dossiers et fenêtres typées avec leur
`NodeId` et leur nom), sans dépendance à egui.

Cliquer une fenêtre l'ouvre ou la focalise, pour les quatre natures :

- **`Tab::Book`** remplace l'ancien `Tab::Grid` unique et global : une fenêtre
  par classeur ouvert, avec une sous-navigation interne entre ses feuilles
  (`AppState::worksheets_of`, `AppState::selected_sheet_in` — une carte
  `Workbook -> Worksheet` plutôt qu'une sélection globale unique, pour que
  deux classeurs ouverts simultanément n'interfèrent jamais l'un avec
  l'autre). La grille elle-même (virtualisation, une seule cellule éditable à
  la fois, `MISSING_TEXT_DISPLAY`…) est inchangée dans son principe ; elle est
  seulement scopée à « la feuille actuellement choisie dans ce classeur »
  plutôt qu'à une sélection globale — `AppState::visible_columns` et
  `AppState::row_count` prennent désormais un `NodeId` de feuille explicite.
- **`Tab::Note`** est réellement éditable : `plotyx-core` expose déjà le texte
  d'une note (`NodeKind::Note::text`) et une commande annulable
  (`Command::SetNoteText`). L'éditeur suit le même principe de saisie
  différée qu'une cellule de grille (`AppState::NoteEdit`, symétrique de
  `CellEdit` : le tampon n'atteint le modèle qu'à la perte de focus).
- **`Tab::Matrix`** est un **espace réservé documenté** : la fenêtre s'ouvre et
  se focalise comme les trois autres, mais ne montre qu'un message explicite
  — `plotyx-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu à éditer, et
  l'éditeur de matrice complet est explicitement hors périmètre de cette
  tâche.

### Maximiser et détacher une fenêtre

Les fenêtres de contenu proposent dans leur en-tête des icônes pour
maximiser, détacher et fermer, accompagnées d'infobulles. Le classeur et le
graphe utilisent les mêmes commandes. La maximisation remplit le
`CentralPanel` ; le Project Manager et la console restent accessibles.
Restaurer rend à la fenêtre sa disposition flottante. Le menu Fenêtre
permet aussi de maximiser ou restaurer la fenêtre active.

### Détachement du Project Manager

Le Project Manager est un panneau fixe à gauche. Le menu Fenêtre permet de
le détacher dans une fenêtre OS puis de le réattacher. Sa fermeture OS le
réattache, puisque ce panneau de navigation n'est jamais fermé.

Les fenêtres séparées utilisent `show_viewport_deferred` : leur rendu est
indépendant de celui de la fenêtre principale. Le callback échange un
instantané de lecture et des requêtes avec celle-ci, via deux
`Arc<Mutex<_>>`, sans déplacer `AppState` ni créer une seconde voie de
mutation métier. Les requêtes réveillent explicitement la fenêtre
principale pour être traitées même quand elle est inactive.

### Détachement générique des onglets de contenu

`PlotyxApp::detach_tab`/`reattach_tab`/`show_detached_tabs` généralisent le
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
`tab_contents` (partagée par les fenêtres flottantes et la zone de maximisation)
prend `&mut AppState`, que la fermeture `'static`, `Send + Sync` de
`show_viewport_deferred` ne peut pas capturer — le même problème déjà résolu
pour le PM (mettre `AppState` entière derrière un verrou déborderait le
problème à résoudre, pour un onglet comme pour l'autre). Le contournement :
`DetachedTabViewModel::snapshot`, un clone de `Snapshot` (bon marché),
resynchronisé depuis `self.state` à chaque image avant l'appel — même schéma
que `ProjectManagerViewModel::tree`. Toute lecture pure passe par des
fonctions libres de `crate::state`, factorisées sur un `&Tree` pour
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
déjà scopés par graphe, dans `PlotyxApp::graph_views`) — `graph_view_contents`
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

### Thème

L'appli **ne suit pas** le thème clair/sombre du système : `plotyx_app::theme`
pose, une seule fois au démarrage (`main.rs`, dans la fermeture de création
d'`eframe::run_native`, aux côtés du réglage des polices), une palette maison
dérivée de `egui::Visuals::light()` — jamais du sombre. `Context::set_theme`
verrouille la préférence sur `Theme::Light` (empêche egui de re-suivre le
système à la moindre notification `winit`), puis `Context::set_visuals`
applique la palette (`theme::visuals`) : accent vert forêt (sélection,
liens, contours des widgets actifs/survolés), coins légèrement moins arrondis
que le défaut egui. Le reste (gris de fond, contrastes de texte) vient tel
quel du thème clair d'egui, déjà calibré pour la lisibilité.

Deux raisons à ce choix, documentées en tête de `theme.rs` : `plotyx-render`
dessine les tracés sur un fond clair implicite (axes, grilles, texte noir) —
un thème sombre romprait ce contraste sans qu'aucun code de rendu n'en soit
informé ; et un thème qui change avec le système est un thème qu'on ne peut
pas garantir en capture d'écran ni en support.

**Barres de défilement : un seul standard.** Toutes les barres de
l'application (espace de travail, grille, panneaux, fenêtres d'import et de
script, fenêtres détachées) ont le même aspect : 10 points d'épaisseur,
poignée gris foncé atténuée, qui passe à la couleur de survol puis à l'accent
pendant le glisser. `theme::scroll_style` le définit, `theme::install` le pose
une fois sur le style global d'egui ; changer les constantes `SCROLL_BAR_*` de
`theme.rs` change toutes les barres d'un coup. Règle pour le code futur : ne
jamais construire ni modifier un `ScrollStyle` hors de `theme.rs` ; une zone
qui doit réserver la place d'une barre la lit par
`theme::scroll_bar_allocated_width`. Le test `aucun_scroll_style_hors_du_theme`
fait échouer la CI sinon.

### Interface multilingue (Fluent)

L'interface s'écrit **d'abord en anglais** ; le français en est une traduction
(chantier #39). Seule l'application traduit : Fluent (`i18n-embed`,
`i18n-embed-fl`, `rust-embed`, plus `fluent-syntax` pour la pseudo-langue) est
une dépendance de `plotyx-app` et d'aucun autre crate — `core`, `commands`,
`session` et les autres rendent des données structurées que l'application met
en phrase (#46, #47). Le pourquoi du choix de Fluent (clé et arguments vérifiés
à la compilation) est en tête de `src/i18n.rs`.

| Où | Quoi |
|---|---|
| `crates/plotyx-app/i18n/en/plotyx_app.ftl` | le catalogue de référence ; une clé absente d'ici n'existe pas |
| `crates/plotyx-app/i18n/fr/plotyx_app.ftl` | la traduction, vide au départ ; une clé absente reprend l'anglais |
| `crates/plotyx-app/i18n.toml` | `fallback_language = "en"`, dossier des catalogues |
| `src/i18n.rs` | chargeur, langue courante, macro `tr!` ; `src/i18n/pseudo.rs` : la pseudo-langue |
| `src/preferences.rs` | `preferences.json`, même dossier de configuration que `couleurs.json` |
| `src/history_text.rs` | les libellés d'historique (`HistoryLabel`) en phrases, clés `history-*` (#46) |
| `tests/libelles.rs` | la garde `aucun_libelle_en_dur` et sa liste de dérogations |

**`tr!` est l'unique point d'entrée** : `tr!("menu-file")`, ou avec arguments
nommés `tr!("witness-count", count = n)`. Il enveloppe `fl!` ; une clé ou un
argument inconnu de `en` ne compile pas. Aucun littéral dans un libellé. Un test
qui compare une langue précise passe son chargeur : `tr!(in loader; "clé")`
(`build_loader`), pour ne pas dépendre de `PLOTYX_LANG`.

**Langue** : la variable `PLOTYX_LANG` (`en`, `fr`, `en-XA`), puis la
préférence `language` de `preferences.json`, sinon l'anglais. La langue du
système n'est **pas** suivie tant que le catalogue français n'est pas complet
(#49). Une valeur inconnue ou un fichier corrompu passe au niveau suivant, sans
panique (invariant 10). Choisie une fois, à la première lecture du chargeur.

**Pseudo-langue `en-XA`** : fabriquée à la volée depuis `en` (accents, `[[ ]]`,
+30 %), sans fichier. `PLOTYX_LANG=en-XA` : un texte sans `[[ ]]` n'est pas passé
par le catalogue, un `]]` manquant signale un texte tronqué.

**Garde `aucun_libelle_en_dur`** (`tests/libelles.rs`) : aucun littéral textuel
dans `label`, `button`, `menu_button`, `checkbox`, `radio_value`,
`selectable_value`, `selectable_label`, `on_hover_text`, `heading`, ni dans les
`Button::new(…)`/`RichText::new(…)`, ni dans les tables `fn label()`/`fn title()`.
L'existant d'avant #41 (213 littéraux, 14 zones) est recensé par fichier et par
zone, chaque zone portant le ticket qui la convertit (#42-#45) : la liste se
réduit, ne grossit pas, et doit être vide à #48. La zone #43 (propriétés du
graphe, éditeur de texte, pied des dialogues, export) est convertie : elle n'a
plus de dérogation et reste gardée plus strictement (`ZONE_43` : aucun littéral
textuel dans ses fonctions, quelle que soit la forme de l'appel). Mécanique détaillée : skill
`plotyx-textes-interface`.

**Erreurs affichées (#47).** Les crates du dessous rendent des erreurs
structurées dont le `Display` (`#[error(...)]`) est en **anglais brut** — ce que
lisent le CLI, `pyx` et les journaux — et que personne ne traduit. L'application
les met en phrase par `src/errors.rs` : une fonction `error_message` par
famille (`model` pour `CoreError`, `commands`, `session`, `file` pour
`PersistenceError`, `import`, `formula`, `render`, `export`, et `script` avec la
feature `python`), qui associe chaque variante à une clé `error-*` du catalogue
`en` avec ses champs en arguments. Le `match` est exhaustif, sans `_` : une
variante nouvelle ne compile pas tant qu'elle n'est pas associée (ou renvoyée
volontairement à son `Display` par `errors::fallback`). Le test de chaque
famille construit une valeur de chaque variante et vérifie que la phrase n'est
pas vide, sans jargon interne, sans point final. Toute erreur montrée à
l'utilisateur passe par ces fonctions, jamais par `to_string()` ni `{err}`.
Les champs `detail` (cause décrite par le crate d'origine) et les messages du
système restent en anglais dans la phrase. `plotyx-importer` rend un
`ImportFailure` structuré (fichier + cause) pour la même raison.

### Sélecteur de fichiers natif

Le bandeau de fichier a été supprimé : Ouvrir, Enregistrer et Enregistrer
sous sont réunis dans le menu Fichier. Les clics et leurs raccourcis
partagent le même routage. Ouvrir (Ctrl+O) affiche un `rfd::FileDialog` ;
Enregistrer sous (Ctrl+Maj+S) demande une destination ; Enregistrer
(Ctrl+S) utilise la destination courante.

`rfd` (feature par défaut `xdg-portal` + `wayland`) compile et se lie sans
portail système présent : le portail (`xdg-desktop-portal`, D-Bus) n'est
qu'une dépendance d'exécution. Vérifié par une compilation `cargo build` et
`cargo build --release` réussies dans un bac à sable sans session graphique
garantie. L'ouverture réelle du sélecteur sur un vrai bureau Wayland reste à
valider interactivement — ce qu'aucun outil non interactif de ce dépôt ne
peut faire à la place d'un humain devant un écran.

### Deux modules, une ligne de partage

| Module | Contenu | Testable en CI |
|---|---|---|
| `state` | **toute la logique**, sans egui, pilotée par un enum d'`Action` | oui (`tests/shell.rs`) |
| `app` | la glu `eframe` : traduit clavier et souris en actions, peint l'état | non |

Une fenêtre ne se teste pas en intégration continue. C'est toute la raison du
découpage : **rien de ce que contient `app` n'est une décision de produit**.

`Action` : `Undo`, `Redo`, `Save`, `Load`, `SelectSheet`, `CommitCell`,
`AddWorkbook`, `AddColumn`, `InsertRows`, `DeleteRows`, `SetNoteText`, `RunScript`,
`InterruptScript`. `AddColumn`/`InsertRows`/`DeleteRows` portent désormais un
`NodeId` de feuille explicite (`sheet`) : depuis que la grille est scopée par
classeur (`Tab::Book`), il n'y a plus de « la » feuille affichée sur laquelle
une action implicite pourrait agir sans ambiguïté quand plusieurs classeurs
sont ouverts en même temps.

### Deux règles de conduite

1. **`AppState::dispatch` ne rend jamais d'erreur.** Toute erreur devient un
   `StatusMessage` affiché à l'utilisateur. Un banc d'essai qui meurt sur une
   saisie invalide ne prouve rien. Nuance portée par le type : « impossible
   pour l'instant » (annuler à vide) n'est **pas** une erreur ; un chemin
   illisible en est une.
2. **L'instantané détenu est figé** (l'arbre est persistant). Il faut le
   redemander à l'acteur après chaque mutation — ce que fait `refresh`, appelé
   par `dispatch` après toute action ayant pu changer le modèle.

### La console de script ne fige pas la fenêtre

`ScriptEngine::run` **bloque** jusqu'au terme du script. L'appeler depuis la
boucle de rendu figerait la fenêtre entière, bouton « Arrêter » compris — le
défaut le plus visible qu'une console de script puisse avoir.

La parade vit dans `state`, pas dans `app`. Trois décisions, prises là et
nulle part ailleurs :

1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
   `AppState::new` ne touche pas à `ScriptEngine` ; seul le premier
   `RunScript` le construit. Le temps de démarrage de l'interpréteur est l'un
   des motifs du portage — le perdre à l'ouverture annulerait le bénéfice.
   Verrouillé par `tests/script_bridge_lazy.rs`.
2. **`RunScript` ne bloque jamais l'appelant** : le travail part sur un **fil
   détaché** (création paresseuse du moteur comprise) et l'appel revient
   aussitôt.
3. **`poll_script` est à appeler à chaque image** : il sonde le résultat par
   `try_recv` et l'applique dès qu'il est prêt, rafraîchissement de l'instantané
   compris. C'est ce qui rend `dispatch` sûr à appeler depuis la boucle de
   rendu pour **toutes** les actions.

Sans la feature `python`, la console reste visible mais désactivée
(`AppState::script_available`). `ScriptOutput` est une copie **volontairement
indépendante** de `plotyx_script::ScriptOutcome`, pour que la surface publique
de `state` soit la même avec et sans la feature.

Une exception de script n'est pas une erreur d'application : sa trace Python
complète s'affiche, sans jamais marquer le statut en erreur.

### Ce que Qt donnait gratuitement, et qu'il faut écrire ici

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci clavier,
l'entrée de menu, son grisage et son libellé (« Annuler *Renommer* »). Ici les
quatre sont séparés : le raccourci est déclaré et consommé à la main
(`SHORTCUT_UNDO` et compagnie), le grisage passe par `add_enabled`, le libellé
vient de `AppState::undo_label` — et **rien ne garantit leur cohérence sinon la
relecture**. C'est le point à vérifier en premier quand une action est ajoutée.

### Menus contextuels de la grille

Le classeur n'a plus de bandeau d'actions au-dessus de la grille. Un clic droit
dans l'espace vide à droite de la dernière colonne propose « Ajouter une
colonne » ; un clic droit sur un onglet de feuille ou sur la partie vide de sa
bande propose « Ajouter une feuille ». La bande reste sous la grille dans les
fenêtres flottantes et détachées. « Importer… » se trouve dans le menu Fichier.
Son dialogue choisit entre le classeur actif, un nouveau classeur par fichier
et un seul nouveau classeur pour tout le lot. Chaque fichier valide crée sa
propre feuille ; les nouveaux classeurs ne sont créés qu'après analyse
réussie. Le lot est une seule entrée d'historique annulable.

Les cinq zones de la grille — cellule de données, champ d'en-tête d'une seule
colonne, rangée des lettres, gouttière des numéros de ligne, libellés de champ
entier dans cette même gouttière — portent le **même** bloc d'édition au clic
droit (`app::grid_edit_menu`) : Couper, Copier, Copier avec l'en-tête, Coller,
Coller à l'identique, Vider, Supprimer et décaler, Annuler, Rétablir. Aucun
n'offre d'action qui n'ait pas d'équivalent au clavier : ce bloc est le miroir
exact du bloc de raccourcis de `grid_contents`. La rangée des lettres y ajoute
ensuite ce qui lui est propre (type et rôle de colonne), qui décrit la colonne
plutôt qu'il n'agit sur son contenu.

Les entrées sont des verbes **nus** : « Copier », pas « Copier la cellule » ;
« Annuler », pas « Annuler Edit Cells in Column A ». Le clic droit vient de
se poser sur ce qu'il vise, et le raccourci est déjà rappelé à droite de
l'entrée — nommer la cible en plus allongeait chaque ligne sans rien
apprendre. `GridMenu::target_label`/`copy_full_columns_label` et
`AppState::undo_label`/`redo_label` restent **disponibles et testés** pour les
surfaces où l'on ne voit *pas* ce qu'on vise (la barre de menus les utilise
déjà pour Annuler/Rétablir, et une ligne de statut ou une confirmation en
auront besoin) ; ils ne sont simplement pas appelés par le menu contextuel.

La **décision** (quels gestes ont un sens ici, lesquels griser, comment
nommer la cible le jour où c'est utile) vit dans `AppState::grid_menu` →
`GridMenu`, pas dans le code egui : c'est ce qui la rend vérifiable sans
fenêtre dans `tests/shell.rs`, comme le reste de la matrice copier/coller.
Deux contextualisations s'y jouent :

- l'accord en nombre de `target_label` (« la cellule » / « les champs » /
  « les colonnes » / « les lignes »), avec deux pluriels distincts — celui des
  **cases** (`GridMenu::plural`) et celui des **colonnes**
  (`GridMenu::multi_column`) : trois cellules empilées sont plurielles sans que
  « Copier la colonne entière » doive passer au pluriel.
  `GridZone::Rows` (la gouttière) fait exception à `plural` : ses libellés
  parlent de **lignes**, pas de cases, donc une seule ligne sélectionnée reste
  « la ligne » même sur une feuille à plusieurs colonnes — `plural` n'y compte
  que le nombre de lignes couvertes, jamais `multi_column` ;
- « Supprimer et décaler » **n'apparaît pas dans l'en-tête** : un champ n'est
  pas répété par ligne, Suppr s'y contente de vider, et l'offrir ferait deux
  entrées pour un seul effet — vrai aussi bien pour un champ d'une seule
  colonne que pour un champ entier sur toutes les colonnes (`GridZone::
  Header` couvre les deux, voir plus bas). Il apparaît en revanche pour une
  plage de lignes entières : supprimer une ligne fait remonter ce qui suit
  dans toutes les colonnes, exactement comme un rectangle de cellules.

### Tracer depuis un classeur

La rangée des lettres offre, en plus de « Type » et « Rôle », un sous-menu
« Tracer » (Ligne, Points, Ligne + points, Barres) qui crée un graphe à
partir de la sélection de colonnes courante — même cible que les deux autres
sous-menus (`target_columns` : toute la plage sélectionnée si la colonne
cliquée en fait partie, sinon cette colonne seule). Le graphe créé est
ajouté dans le dossier qui contient le classeur (`Command::AddGraph`, pas
d'entrée séparée pour la feuille) et s'ouvre aussitôt dans son propre onglet.

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

Cette règle vit dans `AppState::plot_columns`/`series_from_columns`
(`plotyx-app/src/state.rs`), pas dans `app.rs` : le menu contextuel ne fait
que choisir le type de tracé et pousser `Action::PlotColumns`. Une seconde
action, `Action::AddSeriesToGraph`, applique la même règle d'appariement mais
relit le `PlotSpec` courant d'un graphe existant pour y **ajouter** des
séries (`Command::SetPlotSpec`) sans toucher à celles déjà présentes — elle
sert le glisser-déposer d'un classeur vers un graphe ouvert, décrit
ci-dessous.

### Glisser une sélection de colonnes vers un graphe

Second chemin vers `Action::AddSeriesToGraph`, complémentaire du sous-menu
« Tracer » : glisser une sélection de colonnes depuis la grille jusqu'à un
graphe déjà ouvert y ajoute des séries, en `PlotType::Line` toujours (le
glisser ne propose pas de choisir le type — contrairement au sous-menu
contextuel, qui lui le demande).

**Zone de prise.** Le glisser ne part pas de n'importe où dans la grille —
ça casserait le glisser d'extension de sélection existant
(`cell_widget`/`extend_selection`), qui occupe déjà toute la surface des
cellules. La zone de prise est une mince bande (`COLUMN_GRIP_WIDTH`, 10px) le
long du bord *extérieur* gauche ou droit de la grille entière — pas de
chaque colonne individuellement — et n'apparaît que si la feuille a déjà une
sélection couvrant au moins une colonne (`AppState::column_drag_payload`).
Le survol y affiche un curseur « attraper » et un petit pictogramme (une
grille de points, peinte à la main plutôt qu'un glyphe de police — voir
`app::paint_drag_handle_icon`) à côté du pointeur. `app::grid_grip_side` est
la fonction pure qui décide si un point est dans cette bande, testée dans
`tests/shell.rs` sans fenêtre ; `grid_contents` s'en sert pour peindre
l'icône et interagit séparément sur deux rectangles confinés (gauche, droite)
pour démarrer le glisser, afin de ne jamais intercepter le reste de la
grille.

**Charge utile.** `AppState::column_drag_payload` construit une
`state::ColumnDragPayload { sheet, columns }` typée — pas une chaîne —
portée par le mécanisme natif d'egui 0.36
(`egui::Response::dnd_set_drag_payload`/`dnd_hover_payload`/
`dnd_release_payload`, `Any + Send + Sync`).

**Cible.** Chaque `Tab::Graph` est enrobé par `app::graph_drop_target`
(appelée par `tab_contents`, seul point d'entrée commun au dock et aux
onglets détachés — voir sa doc de tête) : un survol avec une
`ColumnDragPayload` active dessine un contour sobre autour du graphe, à la
teinte de sélection de `theme::header_letter_row_selected_fill` (aucune
palette inventée) ; un relâchement au-dessus convertit la charge utile en
`Action::AddSeriesToGraph` via `state::drop_payload_action`, une fonction
pure elle aussi testée directement. Relâcher ailleurs qu'au-dessus d'un
graphe ne fait rien — `dnd_release_payload` ne répond que si le pointeur est
contenu dans le rectangle du graphe au moment du relâchement.

**Onglet détaché.** Comme `graph_drop_target` opère via `tab_contents`,
identique pour le dock et pour une fenêtre détachée (voir la doc de tête de
`PlotyxApp::show_detached_tabs`), le dépôt marche dans les deux cas sans code
séparé à maintenir — testé manuellement, `egui::DragAndDrop` étant un état
global du `Context` plutôt que scopé à une fenêtre OS.

### Propriétés des graphiques

Les réglages d'un graphe vivent dans une **fenêtre flottante à onglets**
(`app::plot_settings_window`), un **dialogue modal** (`app::modal_dialog`) :
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
enregistrer sont refusés (`plotyx-commands`), ainsi que les écritures d'une
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

**Textes libres** (issue #35) : clic droit dans une zone de tracé →
« Ajouter un texte » au point cliqué, ou « Graphique » → « Ajouter un texte »
(au centre du premier panneau). Le texte s'édite **sur place** : un champ posé
sur le texte, et au-dessus une barre couleur / taille / rotation
(`app::annotation_editor_ui`) ; Entrée ou clic ailleurs valide, Échap annule.
Le champ s'écrit **dans le cadre du rendu** (issue #37) : centré sur le centre
du texte rendu, à la taille de police du texte (corps × échelle d'affichage de
la figure), droit même si le texte est tourné — son placement est la fonction
pure `plotyx_render_egui::annotation_field`. Un texte vidé n'étant plus tracé,
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
`RemoveAnnotation`, donc un `SetPlotSpec`, une entrée d'historique. Les couleurs
enregistrées sont publiées chaque image dans la mémoire d'egui
(`publish_saved_colors`) pour que le sélecteur de l'éditeur les propose aussi
dans une fenêtre détachée.

**Exporter le graphe** (issue #34) : menu « Graphique » → « Exporter le
graphe… », Ctrl+E (graphe actif seulement), ou clic droit sur la figure (y
compris dans une fenêtre détachée). Un sélecteur `rfd` (SVG, PDF, PNG) propose
le nom du graphe (`state::export_file_name`) ; le format suit l'extension,
`.svg` si elle manque. `app::request_graph_export` lit le zoom de la vue
(`GraphView::framed_spec`) et pousse `Action::ExportGraph { path, spec }` ;
`AppState` écrit le fichier (`plotyx-render-svg`, texte Typst compris) et le dit
dans la barre d'état. Aucune commande : le modèle n'est pas touché. La
résolution PNG (×1, ×2, ×4 ; ×4 par défaut, soit 1920 × 1440 px) se règle dans
« Graphique » → « Résolution des exports PNG » (`Action::SetPngExportScale`,
préférence de session).

**Onglets** (`state::PlotSettingsTab`, clés `plot-tab-*`, en anglais) :
« General » (titre et sa couleur, fond), « Plot Area » (titre du calque,
quadrillage), « Axes » (un axe à la fois, choisi en tête de page : titre de
l'axe — grandeur et unités —, échelle, affichage, couleur de l'axe), « Series »,
« Legend » et « Theme ». Les textes de toute la fenêtre sont des clés `plot-*`
du catalogue `en` (infobulle en attribut `.hint`), selon le glossaire
`docs/textes-interface.md` ; les noms saisis (titres, noms de série, thèmes)
sont des données, jamais traduits.

**Textes du graphe** : titre, titre du panneau, libellé et unité d'axe, nom de
série sont composés par Typst et lus comme du **markup** (`docs/rendu.md`
§ Typst). Sous chacun de ces champs, une section repliée « Formula syntax »
(`app::formula_memo`, issue #33) rappelle la syntaxe : `$lambda$`, `$x_2$`,
`$"cm"^(-1)$`, `*bold*`, `\$` pour un dollar. Repliable plutôt qu'en bulle
d'aide : la fenêtre est un dialogue modal (issue #9). Le test
`chaque_exemple_du_memo_compile` vérifie que chaque exemple compile. Un nom de
série laissé vide reprend celui de la colonne, affiché tel quel.

**Onglet « Thème »** : deux thèmes prédéfinis (`plotyx_plot::THEMES` : Clair,
Sombre) et trois couleurs globales — fond, texte, axes. Une
couleur globale est **écrite dans tous les champs locaux** (titre du graphe,
titre et échelle de chaque axe de chaque panneau ;
`PlotSpec::set_text_color`/`set_axes_color`/`apply_theme`) : le dernier
réglage écrase ce qui avait été réglé une à une, et rien n'est mémorisé en
plus (le format `.plx` ne change pas). Le tout reste dans le brouillon de la
fenêtre : un seul pas d'historique à la validation. Les thèmes laissent le
texte et les axes en automatique, donc ils suivent le fond.

**Sélecteur de couleur** (`crate::color_picker`) : toute couleur de la
fenêtre se choisit par un bouton-échantillon (`color_field`/`color_button`)
qui ouvre une fenêtre surgissante **repliée** : palette, « Automatique »
quand la couleur peut l'être, couleurs enregistrées et saisie hexadécimale
(`#RRGGBB`, ou `#RRGGBBAA` avec l'opacité). Le bouton **« + Personnalisée… »**
déplie le reste — Enregistrer et sa destination, nuancier (carré saturation ×
luminosité avec barre de teinte, ou **roue chromatique** avec barre de
valeur), **opacité**, saisie RVB et TSL. Le sélecteur se rouvre toujours
replié (`color_picker::custom_id`). Cette fenêtre est une `egui::Area` gérée
par `color_button` (et non un `egui::Popup`, dont egui n'autorise qu'un
exemplaire ouvert : la liste d'un menu déroulant le refermerait) ; elle
se ferme sur Échap ou sur un clic hors d'elle et de ses listes. Une couleur transparente se montre sur un
damier (dans l'échantillon du sélecteur). **Une palette par usage** (`plotyx_plot::ColorUsage`) :

- **courbes** : un outil à part, avec un choix de palettes nommées
  (`plotyx_plot::CURVE_PALETTES` : PlotYx, Tableau 10, Okabe-Ito lisible par
  les daltoniens, ColorBrewer Set1/Dark2/Paired, dégradés Viridis, Plasma,
  gris), chacune avec ses nuances foncées et claires. Les palettes vivent
  dans `plotyx-plot`, pour qu'une évolution future colore plusieurs courbes
  d'un coup (palette ou dégradé) ;
- **texte** : encres lisibles ;
- **fonds** : en premier **« Aucune couleur »** (`#ffffff00`,
  `plotyx_plot::TRANSPARENT`), qui est aussi le **fond par défaut** d'un
  graphe neuf (`PlotSpec::default`) : la figure exportée n'a pas de fond,
  pour s'intégrer à une autre présentation ; à l'écran, elle se montre sur
  un fond **blanc** peint par l'application sous la figure
  (`app::display_backdrop`), jamais exporté. Puis des fonds clairs, et des fonds
  sombres (le texte automatique passe en blanc, voir `docs/rendu.md`). Un
  fond vide (fichiers écrits avant ce défaut) reste blanc.

**Couleurs enregistrées** : « Enregistrer » garde la couleur courante
**dans le projet** (par défaut — commande annulable `SetSavedColors`,
enregistrée dans le `.plx`) ou **pour l'utilisateur** (fichier
`couleurs.json` du dossier de configuration de PlotYx, à côté des recettes
d'import, hors historique — `crate::user_colors`), au choix
(`state::SaveTarget`). Une couleur enregistrée ne réapparaît que dans la
palette de son usage ; clic droit dessus pour la retirer
(`Action::SaveColor`/`RemoveSavedColor`). Le sélecteur ne touche ni au
modèle ni au disque : il dépose des demandes dans un `PickerEnv`, que
`plot_settings_contents` traduit en actions.

La couleur est validée à la fin du geste (clic sur un échantillon, fin de
glisser, champ validé, fermeture), jamais pixel par pixel. Tout nouveau
réglage de couleur passe par ce sélecteur, avec l'usage qui lui correspond.

**Trois façons de l'ouvrir** :

- **clic droit sur un composant du graphe** (`app::graph_context_menu`) :
  titre, zone de tracé, axe, titre d'axe. Le menu propose « Propriétés… »,
  qui ouvre l'onglet qui règle ce composant (`state::PlotTarget::
  settings_page`), et « Réinitialiser le zoom ». Le composant sous le
  pointeur est trouvé par `plotyx_render::hit::component_at`, sur la
  géométrie réellement allouée par Plotters (zone de tracé, aires de
  graduation, bande du titre), pas sur une reconstitution. Le clic droit
  **sans** glisser ouvre le menu ; le glisser secondaire reste le zoom
  élastique. Même chemin en fenêtre flottante et détachée
  (`graph_view_contents`) ;
- **menu « Graphique »** de la barre de menus, présent seulement quand
  l'onglet actif est un graphe (`state::active_graph`, testée sans fenêtre) :
  une entrée par onglet, et « Réinitialiser le zoom ». Aucun réglage n'est
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
`app::plot_settings_window` avant de peindre quoi que ce soit, n'écrase le
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

Chaque widget de `app::plot_settings_contents` (et de ses fonctions auxiliaires
`axis_editor`/`series_editor`/`legend_editor`) accumule dans un simple
booléen local `commit`, mis à `true` par n'importe quel widget qui vient de
finir une interaction ; `commit_plot_edit` n'est appelé qu'une seule fois, à
la toute fin de la fonction, jamais widget par widget.

**Ce qu'édite la fenêtre**, en ne touchant que des champs qui existent
réellement dans `plotyx-plot` (lu avant d'écrire, comme demandé) :

- **Graphe** : `PlotSpec::title`, `background` ;
- **Zone de tracé** — `PlotSpec::panels`[0] seulement, voir plus bas — :
  `PanelSpec::title`, grille des axes du bas et de gauche ;
- **Axes** : pour l'axe du bas, celui de gauche, et celui de droite si
  `PanelSpec::uses_right_axis()` (l'axe du haut, masqué par défaut et sans
  UI dans ce banc pour l'activer, n'a pas de section) : `AxisSpec::label`,
  `unit`, `scale` (`AxisScale::Linear`/`Log`), `visible`, `grid`,
  `inverted`, `auto_range`, et `range_min`/`range_max` quand `auto_range`
  est décoché ;
- **Séries** — une liste (`egui::ScrollArea` bornée en hauteur,
  `selectable_label` par série, libellé `state::series_list_label` : le nom
  de la série s'il est renseigné, sinon le nom long de la colonne Y, sinon
  son nom de nœud, sinon « (référence rompue) » — le même calcul que
  `label_for` en privé dans `plotyx-core/src/resolve.rs`, dupliqué plutôt
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

- **pas de multi-panneaux** — `plotyx-plot` autorise plusieurs `PanelSpec`
  par graphe (`row`/`column` dans une grille), mais aucune UI de ce banc
  n'en crée encore un second ; la fenêtre n'édite donc que
  `PlotSpec::panels`[0], ce qui revient, pour l'instant, à éditer *le*
  graphe ;
- **pas de sélecteur de couleur graphique** — trait et marqueur s'éditent
  comme un texte `#RRGGBB`, pas comme une roue de couleurs : plus simple,
  et ça réutilise la même convention de validation (`lost_focus()`) que
  tous les autres champs texte de la fenêtre plutôt que d'introduire un
  quatrième mécanisme de débounce pour un widget qui déclenche `changed()`
  en continu pendant qu'on le glisse ;
- **pas de largeur de barre ni de nombre de classes d'histogramme** —
  `SeriesSpec::bar_width`/`histogram_bins` existent dans `plotyx-plot` mais
  n'ont pas de widget ici, laissé à une itération suivante ;
- **pas d'ajout/suppression de série ou de panneau depuis la fenêtre** —
  ça reste le rôle du sous-menu « Tracer » et du glisser-déposer déjà
  décrits plus haut ;
- **zoom/pan n'y entrent pas, et ne doivent jamais y entrer** —
  `plotyx_render_egui::ViewState` vit sur `GraphView`, hors du `PlotSpec`
  (`docs/invariants.md`, `.claude/rules.md`) : cette fenêtre n'a et n'aura
  aucun champ de navigation, une confusion entre les deux romprait la
  frontière rendu/modèle que tout le reste du chantier de rendu tient.

### Une colonne a deux identités, une seule est visible

Son **nom de nœud** (`Node::name` — « Temps », « Signal », ou le « A »/« B »
automatique de `Project::next_column_name`) est une clé du modèle : il résout
une colonne depuis un script (`pyx.resolve("[Book1]Sheet1!Temps")`), l'adresse
dans une spec de graphe, la retrouve dans un `.plx`. La grille ne l'affiche
**nulle part** : elle montre la lettre de **position** (A, B, C…) et les
quatre champs de `ColumnMeta` (Nom long, Unité, Commentaire, Formule), qui
sont encore autre chose.

`plotyx_commands::Command::label` construisait ses libellés d'historique sur
le nom de nœud, et annonçait donc « Saisie dans Temps » pour une colonne que
l'utilisateur ne connaît que comme « A » — un mot introuvable à l'écran, et
carrément trompeur quand les deux divergent (un nœud nommé `T` en troisième
position s'affiche « C »). Le libellé désigne désormais la colonne par sa
position (`ColumnRef`, depuis #46 une donnée et non une phrase) : « Edit Cells
in Column A ». Les autres natures de nœud
(feuille, classeur, graphe, note) gardent leur nom réel — celui-là *est*
affiché, dans le Project Manager.

La correspondance position → lettre est `plotyx_core::spreadsheet_name`,
rendue publique pour l'occasion : c'est la même fonction qui nomme les
colonnes créées sans nom explicite, qui peint l'en-tête de la grille
(`app::column_letter` n'en est plus qu'un alias) et qui écrit les libellés
d'historique. Trois copies de cet algorithme valaient trois façons d'appeler
la même colonne.

La gouttière (`app::row_number_widget`) est le miroir exact de la rangée des
lettres (`app::column_header_widget`) : toute sa largeur sélectionne, au clic,
**toute la ligne** — les colonnes visibles de la feuille, à cette ligne, quelle
que soit leur longueur propre (une colonne plus courte que la ligne visée
reste touchée par la sélection, seule sa portion réellement écrite est
affectée par Vider/Supprimer). `AppState::select_row`/`extend_row_selection`/
`is_row_selected` en sont les pendants de `select_column`/
`extend_column_selection`/`is_column_selected`, et `SelectionKind::FullRows`
(`crate::selection`) le pendant de `SelectionKind::Columns` — même règle de
non-franchissement de frontière : un glisser qui part de la gouttière et croise
une cellule ou la rangée des lettres repose une sélection simple plutôt que
d'étendre silencieusement sans effet, et réciproquement.

### La gouttière côté en-tête : homogène, et elle aussi sélectionnable

Côté en-tête empilée (`app::row_number_header_widget`), la gouttière portait
un « # » décoratif sur fond `header_letter_row_fill`, puis quatre rangées de
libellés (« Long Name », « Units », « Comments », « F(x)= ») restées
**transparentes** et inertes — la colonne de gauche n'était donc homogène ni
visuellement (le fond s'arrêtait au « # ») ni fonctionnellement (rien n'y
était cliquable). Les deux sont corrigés ensemble : chaque rangée de libellé
porte désormais le même fond que le « # » (ou le surlignage de sélection),
et un clic sur un libellé (`app::header_row_label_widget`) sélectionne ce
champ pour **toutes** les colonnes visibles de la feuille — symétrique de
`row_number_widget` côté données. `AppState::select_header_row`/
`extend_header_row_selection`/`is_header_row_selected` en sont les pendants
de `select_row`/`extend_row_selection`/`is_row_selected`, et
`SelectionKind::HeaderRows` (`crate::selection`) le pendant de `FullRows`
pour la zone d'en-tête : elle touche toutes les colonnes et, symétriquement,
ne touche jamais les données (`row_range()` y vaut toujours `None`).

Ce n'est **pas** la même chose qu'un champ sélectionné sur une seule colonne
(`app::header_field_widget`, `Selection::header_field`) : celui-ci reste
strictement à une colonne, glisser peut l'étendre à d'autres colonnes mais
jamais d'un seul geste à *toutes* les colonnes de la feuille — les deux
widgets, les deux constructeurs et les deux gestes restent distincts, même
si les deux finissent dans la même zone de menu (`GridZone::Header`) une fois
posés : `cleared_header_fields`/`copy_selection`/`paste` sont tous les trois
écrits en termes de `full_rank_range()`/`columns()`, qui rendent déjà la
bonne réponse pour les deux sortes de sélection sans code dédié
supplémentaire.

Le « # » lui-même sélectionne tout le tableau au clic (`AppState::
select_all`), comme la case en coin de la plupart des tableurs : une
sélection `SelectionKind::FullRows` couvrant `0..=row_count - 1` (`Selection::
all_rows`), qui touche déjà toutes les colonnes visibles sans qu'il faille de
sorte de sélection dédiée — `is_all_selected` en teste la couverture exacte
pour teindre le « # » en retour.

### Une palette à trois paliers pour la gouttière, et une teinte propre à son texte

La gouttière (rangée des lettres, « # », numéros de ligne, libellés de champ)
utilisait le même vert que le reste de l'accent de marque, à pleine
saturation pour son texte (`Visuals::strong_text_color()`, qui vaut
`widgets.active.fg_stroke.color`, donc littéralement `ACCENT`) — trop
appuyé pour une zone censée rester en retrait des données. `theme::
gutter_text_color` lui donne une teinte propre, nettement plus sombre,
indépendante de la couleur globale des éléments « actifs » pour ne pas
déteindre sur des boutons ou onglets sans rapport.

Le fond, lui, gagne un troisième palier. `app::gutter_fill(selected,
touched)` choisit entre `theme::header_letter_row_fill` (repos),
`header_letter_row_touched_fill` (la colonne ou la ligne d'une cellule de
données sélectionnée, sans être elle-même cliquée) et
`header_letter_row_selected_fill` (sélectionnée directement) — le repère
« ta ligne/colonne est concernée » d'un tableur, porté par `Selection::
touches_column`/`touches_row` (`AppState::column_touched_by_selection`/
`row_touched_by_selection`), écrites en termes de `columns()`/`row_range()`
donc valables pour **toute** sorte de sélection sans code dédié
supplémentaire. Les 3 rangées de la gouttière qui en profitent :
`column_header_widget`, `row_number_widget`, et le « # » de
`row_number_header_widget` (`touched` toujours faux pour lui — rien ne
« touche » le tableau entier sans le sélectionner en entier).

Enfin, `ui.spacing_mut().item_spacing.y = 0.0` avant de construire la table
et dans les deux empilements verticaux de la gouttière côté en-tête
(`column_header_widget`, `row_number_header_widget`) : sans ça, l'espacement
par défaut d'egui entre rangées empilées laissait un mince filet blanc entre
chaque case peinte d'un aplat, qui la faisait paraître flotter plutôt que
remplir vraiment sa case.

### Tab en dernière colonne : même geste qu'Entrée en dernière ligne

Entrée sur la dernière ligne d'une feuille ajoute une ligne et y déplace la
sélection (`Action::CommitCellAndAdvance` en édition, ou le raccourci
clavier direct hors édition) — Tab sur la dernière colonne d'une ligne de
données fait maintenant de même pour les colonnes plutôt que de revenir au
début de la ligne suivante (l'ancien comportement de `move_selection_tab`
aux bords).

L'asymétrie entre les deux axes tient à l'identité de la cible : une ligne
n'est qu'un indice, donc sélectionner `(colonne existante, row + 1)` *avant*
que la commande d'ajout ne soit appliquée (elle ne l'est qu'après le rendu de
l'image, voir `app::PlotyxApp::update`) est sûr — la ligne existera à temps.
Une colonne neuve, elle, n'a pas encore de `NodeId` tant que `Command::
AddColumn` n'a pas tourné : la sélectionner à l'avance n'a pas de sens.
`Action::AddColumnAndSelect { sheet, row }` fait donc les deux d'un coup
dans son propre traitement (`AppState::add_column_and_select`) : ajoute la
colonne, *puis* — une fois `self.snapshot` rafraîchi par `execute` — sélectionne
`row` dans la dernière colonne visible. Câblé aux deux mêmes endroits que Tab
normal : le raccourci clavier direct de `grid_contents`, et la validation
d'édition de `cell_widget` — `app::is_last_visible_column` factorise la
condition commune aux deux.

Câblé aussi **côté en-tête** (`Action::AddColumnAndSelectHeaderField`,
`AppState::add_column_and_select_header_field`), mais seulement au champ
Formule — le dernier des quatre — de la dernière colonne : c'est le seul
point où Tab, jusqu'ici, quittait l'en-tête (`GridCursor::Header(_,
Formula)` → `Data(première colonne, 0)`, voir `move_selection_tab`). Les
trois autres champs (Long Name, Units, Comments) gardent leur circulation
normale même sur la dernière colonne — Tab y avance d'abord colonne par
colonne pour un même champ ; les intercepter aussi aurait empêché d'atteindre
les champs suivants de cette colonne, en ajoutant une colonne à chaque
pression plutôt que d'y circuler.

### Tab et flèches appartiennent à la grille (#38)

egui traite Tab et les flèches **au début de l'image** (`Focus::begin_pass`)
pour déplacer son propre focus de widget en widget, sauf si le widget
focalisé les réclame par un `EventFilter`. La grille les lisait par
`key_pressed`, qui ne retire rien : egui donnait le focus au premier widget
focalisable venu (souvent hors de la grille), et dès qu'un widget avait le
focus la garde des raccourcis (`egui_wants_keyboard_input`) coupait net la
sélection de la grille.
Même chose en édition : le `TextEdit` d'une cellule ou d'un champ d'en-tête
perdait le focus sur Tab.

La règle, dans `grid_contents` (donc la même pour le classeur flottant et le
classeur détaché, tous deux par `book_contents`) :

- **Un ancrage de focus par grille** (`grid_focus_id`) : un widget posé sur le
  rectangle de la grille, focalisable *seulement tant qu'il a le focus* (sinon
  un Tab sans focus y entrerait de lui-même), qui porte le filtre
  `GRID_KEYS` (Tab, flèches ; pas Échap, qui rend le focus). La grille le
  réclame (`claim_grid_focus`) à un appui ou un relâchement dans son
  rectangle, et à la fin d'une édition qui a rendu le focus à personne.
  egui le retire seul à un clic ailleurs : Tab y reprend alors son rôle
  habituel, sans toucher la sélection.
- **En édition**, les champs de cellule et d'en-tête posent le même filtre
  (`TextEdit::event_filter(GRID_KEYS)`) : Tab ne les fait plus perdre le
  focus, ils le lisent eux-mêmes (`tab_pressed`), valident, rendent le focus
  et laissent la grille le reprendre.
- **Garde des raccourcis** : `focus_free` (aucun focus, ou celui de cette
  grille). Le reste de l'application, qui testait `focused().is_some()` pour
  savoir si l'on tape dans un champ (Entrée d'un dialogue, Suppr d'une
  annotation), passe par `keyboard_focus_outside_grid`.

Limite : le filtre ne s'applique qu'à l'image qui suit la prise de focus
(`set_focus_lock_filter` exige `had_focus_last_frame`) — une touche tombant
dans l'image même du clic n'est pas une frappe réaliste. Verrouillé par
`app::grid_keyboard_tests`. Le focus d'egui ne se voit pas dans une capture
(`PLOTYX_APP_SCREENSHOT`) : à confirmer sur une vraie fenêtre.

### Alignement du corps : nombres à droite, texte à gauche

`cell_widget` aligne chaque cellule du **corps** de la grille comme un
tableur — jamais la gouttière ni l'en-tête, qui n'affichent pas de valeur de
colonne. `AppState::cell_is_numeric` tranche par cellule, pas par colonne :
sur une colonne `Mixed`, deux lignes voisines peuvent s'aligner
différemment, l'une à droite (nombre saisi), l'autre à gauche (texte). Une
valeur manquante (`NaN` ou texte vide/absent) compte comme non numérique et
reste à gauche — un vide n'a pas de raison d'être poussé à droite, et ça
évite qu'une colonne clairsemée ait l'air de sauter d'un bord à l'autre
d'une ligne à l'autre.

`Label::halign` seul ne suffit **pas** ici, malgré ce que sa doc laisse
penser : `ui.put` place le `Label` dans un `Layout::centered_and_justified`,
qui justifie le texte (l'étire pour remplir la largeur) avant que `halign`
n'ait voix au chapitre — le résultat restait visuellement centré, quel que
soit `halign`. Un vrai `Layout` directionnel (`right_to_left`/
`left_to_right`, `Align::Center` pour l'axe vertical), posé via
`ui.scope_builder` sur un `max_rect` légèrement rétréci — le même mécanisme
que celui déjà posé sur toute la table (`cell_layout` du `TableBuilder`) —
aligne réellement le texte contre le bord voulu.

Un clic droit **hors** de la sélection la réduit d'abord à la case visée, un
clic droit **dedans** la laisse intacte — pour que le menu agisse toujours sur
ce qu'on vient de désigner — même garde (`response.secondary_clicked() &&
!selected`) dans les quatre zones. `column_header_widget` ne l'avait pas :
son clic droit réduisait **toujours** la plage à la lettre visée, ce qui
rendait impossible de changer d'un coup le type ou le rôle de plusieurs
colonnes sélectionnées, alors même que le commentaire de son `target_columns`
supposait déjà l'inverse. Corrigé en même temps que l'arrivée de la
gouttière, qui a rendu l'écart visible.

Deux écarts délibérés entre le menu et le clavier :

- **« Supprimer et décaler » supprime tout de suite**, là où la touche Suppr
  vide à la première pression et ne supprime qu'à la seconde. La double
  pression n'existe que pour protéger d'une frappe réflexe sur une touche
  facile d'accès ; un menu contextuel est un geste explicite et désigné.
- **« Coller » n'est jamais grisé.** egui n'expose aucune API de *lecture* du
  presse-papier (Ctrl+V ne marche que parce qu'`egui-winit` lit lui-même le
  presse-papier avant de pousser l'`Event::Paste`), donc `app::
  system_clipboard_text` le lit directement via `arboard` — la bibliothèque
  qu'`egui-winit` emploie déjà en interne. Savoir s'il y a quelque chose à
  coller exigerait cette lecture à *chaque image* où le menu est ouvert : on
  tente au clic, et on pose une notice si ça ne donne rien. « Coller à
  l'identique », lui, est grisable sans coût — il rejoue le presse-papier
  **interne**, dont on connaît l'état.

Enfin, les cinq constantes `SHORTCUT_COPY`/`SHORTCUT_CUT`/`SHORTCUT_PASTE`/
`SHORTCUT_COPY_FULL_COLUMNS`/`SHORTCUT_PASTE_ABSOLUTE` sont **purement
décoratives** : elles n'existent que pour écrire le rappel de raccourci à
droite d'une entrée de menu. Rien ne les consomme, et il ne faut pas essayer —
`egui-winit` intercepte Ctrl+C/X/V à la traduction des événements de la
fenêtre et n'en laisse jamais passer de `Event::Key` (un essai précédent,
fondé sur l'hypothèse inverse, produisait des raccourcis qui ne se
déclenchaient jamais).

### Limites assumées du banc

- **Les raccourcis sont consommés au niveau du contexte**, donc *avant* les
  widgets. Ctrl+Z pendant la saisie d'une cellule annule la dernière **commande
  du modèle**, pas la dernière frappe : c'est le comportement attendu d'un
  tableur, et c'est aussi la limite — le banc ne sait pas distinguer les deux
  intentions.
- **Une seule cellule éditable à la fois**, toutes grilles **dockées**
  confondues (`AppState::editing`). La grille est virtualisée
  (`TableBuilder::body().rows()`) : une feuille peut avoir un million de
  lignes, il n'est pas question de garder un tampon par cellule. Le même
  principe vaut pour l'édition d'une note (`AppState::NoteEdit`), pour la même
  raison de cohérence plutôt que de nécessité stricte. Un onglet **détaché**
  a son propre tampon, indépendant de celui du dock et des autres onglets
  détachés (voir « Détachement générique des onglets de contenu ») —
  contrainte technique de la fermeture `'static` de `show_viewport_deferred`,
  pas une extension délibérée de la limite.
- **Les fenêtres ouvertes et leur mode ne sont pas restaurés** : le
  démarrage ouvre le premier classeur et le premier graphe. Les positions
  et tailles enregistrées sont réutilisées pour les contenus, mais la
  maximisation et le détachement ne constituent pas un plan de disposition
  persisté.

### Un détail qui a l'air d'un détail

`MISSING_TEXT_DISPLAY` vaut `\u{00A0}` — un espace insécable — pour afficher une
valeur texte **absente**. Il occupe la cellule comme un blanc ordinaire (rien ne
saute aux yeux) tout en restant, en mémoire, une chaîne différente de `""`, donc
distincte d'une chaîne vide **présente**. Un caractère visible (`—`, `∅`, `?`)
aurait été une réponse légitime à « qu'est-ce qu'un utilisateur pourrait vouloir
taper ici ? » ; un espace insécable n'est accessible à la frappe sur aucun
clavier usuel.

Il ne vaut que pour les colonnes **texte**, où une chaîne vide présente existe
et doit rester indiscernable à l'œil. Une colonne **mixte** porte
l'information dans le type de sa cellule : `cell_text_in` la lit par
`Column::cell` (et non `Column::get`, qui confondrait les deux cas) et affiche
une cellule vide comme une cellule numérique vide, `""`.

### Taper ce qui n'est pas un nombre

Une saisie non numérique dans une colonne numérique **élargit la colonne**
plutôt que d'être refusée : `commit_cell` envoie `Command::SetCellsPromoting`,
la colonne devient `ColumnType::Mixed`, la cellule porte le texte, les autres
gardent leurs nombres ([`commandes-et-historique.md`](commandes-et-historique.md)).
Conversion et écriture tiennent dans une seule entrée d'historique : un
`Ctrl+Z` ramène le type *et* la valeur d'avant.

Avant les colonnes à types mixtes, la saisie était refusée avec un message
d'erreur, et noter un « n/a » sur une ligne obligeait à convertir toute la
colonne en texte — donc à rendre tout le reste intraçable. Sur une colonne
déjà mixte, plus rien n'est promu : chaque cellule décide pour elle-même, et
un nombre tapé y reste un nombre.

### Binaire

`plotyx-app [chemin]`. Si `chemin` existe, le projet est chargé depuis ce
fichier ; sinon un projet de démonstration est construit et `chemin` devient la
cible d'un futur enregistrement.

Deux variables d'environnement servent aux tests et aux captures :
`PLOTYX_APP_AUTOQUIT_MS` et `PLOTYX_APP_SCREENSHOT`.

### Tests

`tests/shell.rs` (la boucle complète, sans fenêtre), `tests/script_panel.rs`,
`tests/script_bridge_lazy.rs` (l'interpréteur ne démarre pas tout seul),
`tests/perf_write_under_snapshot.rs`, `tests/libelles.rs` (la garde de
traduction, voir « Interface multilingue »).

## `plotyx-cli`

Binaire headless de vérification — **pas** l'UI. Il sert à valider que le
workspace se lie de bout en bout sans dépendre d'un choix d'UI. C'est un
squelette : il affiche une ligne et sort.

## Règles communes avec le rendu

Deux règles ne sont **pas** réécrites dans l'interface (issue #25) :
la syntaxe des couleurs `#RRGGBB[AA]` (`plotyx_plot::parse_hex`, dont
`color_picker::parse_hex` n'est qu'un adaptateur) et le nom affiché d'une série
(`plotyx_core::series_label`, appelée par la légende **et** par la liste des
séries des propriétés du graphe via `series_list_label`). Les transformations
RVB/TSL restent propres au sélecteur.
