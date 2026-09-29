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

### Débordement des fenêtres de contenu

Toutes les fenêtres de contenu flottantes et détachées partagent un rectangle
extérieur unique : leur contenu est peint dans un sous-espace borné qui ne
peut agrandir le cadre au-delà des poignées de redimensionnement. La règle
intérieure dépend de la nature du contenu. Dans un classeur, la grille et la
bande d'onglets de feuille défilent chacune horizontalement, indépendamment,
lorsqu'elles dépassent. La bande d'onglets est retranchée du bas du classeur
avant la construction de la grille, qui ne peut donc plus la repousser hors du
cadre (`book_contents`) ; ce découpage n'utilise pas `egui::Panel`, qui
remplace le rognage de son parent au lieu de le combiner et dessinait les
onglets sous la vue d'une fenêtre coupée. Les onglets
gardent leur taille naturelle (`sheet_tabs`, `crates/plotyx-app/src/app.rs`) —
ils ne rétrécissent pas et ne se replient pas dans un menu « … » ; sélectionner
un onglet hors vue (au clic, au clavier ou par script) fait défiler la bande
vers lui. Les notes et matrices restent bornées au rectangle commun. Dans un
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
La marge horizontale reste. La barre de défilement horizontale de la grille
(`show_horizontally_scrollable_grid`) utilise un style plein
(`egui::style::ScrollStyle::solid()`) plutôt que la barre flottante par
défaut d'egui, invisible tant qu'elle n'est pas survolée au pixel près —
dans une fenêtre déjà rognée à son rectangle, elle n'était ni visible ni
atteignable.

### Géométrie des fenêtres

Chaque déplacement ou redimensionnement est envoyé à la session et enregistré
dans le fichier `.plotyx`. Au redémarrage, la première ouverture d'un contenu
reprend cette position et cette taille. Une fenêtre détachée utilise aussi ces
valeurs pour son viewport OS ; sous Wayland, le compositeur peut refuser la
position demandée, mais conserve sa taille.

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
`DetachedTabViewModel` porte un champ par nature (`book_state` pour un
classeur, `note_edit` pour une note, `graph_view` pour un
graphe) plutôt que quatre types disjoints à faire cohabiter dans la table des
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
de grille, puis applique les mêmes commandes annulables. La note détachée
conserve son propre `note_edit` et sa fonction de rendu dédiée.

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
« Annuler », pas « Annuler Saisie dans la colonne A ». Le clic droit vient de
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

### Panneau de configuration des graphiques

Chantier 3/3 de « tracer depuis un classeur ». Un `egui::Panel::right`,
peint dans `PlotyxApp::ui` *avant* le `CentralPanel` — comme le
Project Manager à gauche — pour que les fenêtres flottantes ne lui disputent
jamais l'espace (voir § Fenêtres flottantes confinées).
Visible seulement quand l'onglet actif est un `Tab::Graph` : « actif » au
sens le plus large (l'onglet maximisé s'il y en a un, sinon la fenêtre
flottante la plus récemment focalisée — dernier élément de
`PlotyxApp::floating_tabs`), traduit
vers `state::ActiveTabKind` — un miroir minimal et public de `Tab` (privé à
`app.rs`) — au seul point de contact (`app::active_tab_kind`). C'est
`state::plot_panel_graph(ActiveTabKind) -> Option<NodeId>` qui tranche la
visibilité, fonction pure testée sans fenêtre dans `tests/shell.rs` :
`Tab::Book`/`Matrix`/`Note`, ou aucun onglet actif, la masquent.

**Édite le `PlotSpec` au travers d'un brouillon, jamais directement.**
`state::PlotEdit { graph, spec, selected_series }` est un `PlotSpec` entier
tenu hors de l'historique jusqu'à validation — le même principe que
`NoteEdit`/`HeaderEdit` (saisie différée, tampon local, commande envoyée
seulement à la validation), généralisé à un objet entier plutôt qu'à un seul
champ : le panneau mélange une douzaine de petits widgets (titre, axes,
style de chaque série, légende) qui doivent tous voir et modifier le **même**
brouillon. `AppState::begin_plot_edit(graph)`, appelée à chaque image par
`app::plot_config_panel` avant de peindre quoi que ce soit, n'écrase le
brouillon que s'il n'y en a pas encore ou qu'il portait sur un autre graphe
— repeindre la même image sur le même graphe ne doit jamais effacer une
saisie en cours, et changer de graphe ne doit jamais laisser fuiter le
brouillon du précédent. `AppState::commit_plot_edit` envoie
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

Chaque widget de `app::plot_config_panel` (et de ses fonctions auxiliaires
`axis_editor`/`series_editor`/`legend_editor`) accumule dans un simple
booléen local `commit`, mis à `true` par n'importe quel widget qui vient de
finir une interaction ; `commit_plot_edit` n'est appelé qu'une seule fois, à
la toute fin de la fonction, jamais widget par widget.

**Ce qu'édite le panneau**, en ne touchant que des champs qui existent
réellement dans `plotyx-plot` (lu avant d'écrire, comme demandé) :

- **Graphe** : `PlotSpec::title` ;
- **Panneau** — `PlotSpec::panels`[0] seulement, voir plus bas — : `PanelSpec::title`,
  et pour l'axe du bas, celui de gauche, et celui de droite si
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
à ce panneau : proposer `Unknown` dans la liste laisserait croire qu'on peut
y revenir, alors que rien ne sait reconstruire une variante dont ce lecteur
ne connaît que le nom et des octets bruts (C5, niveau 2,
`guide-de-correction-robustesse.md` §F3).

**Ce qui n'est pas fait**, explicitement hors périmètre de ce chantier :

- **pas de multi-panneaux** — `plotyx-plot` autorise plusieurs `PanelSpec`
  par graphe (`row`/`column` dans une grille), mais aucune UI de ce banc
  n'en crée encore un second ; le panneau n'édite donc que
  `PlotSpec::panels`[0], ce qui revient, pour l'instant, à éditer *le*
  graphe ;
- **pas de sélecteur de couleur graphique** — trait et marqueur s'éditent
  comme un texte `#RRGGBB`, pas comme une roue de couleurs : plus simple,
  et ça réutilise la même convention de validation (`lost_focus()`) que
  tous les autres champs texte du panneau plutôt que d'introduire un
  quatrième mécanisme de débounce pour un widget qui déclenche `changed()`
  en continu pendant qu'on le glisse ;
- **pas de largeur de barre ni de nombre de classes d'histogramme** —
  `SeriesSpec::bar_width`/`histogram_bins` existent dans `plotyx-plot` mais
  n'ont pas de widget ici, laissé à une itération suivante ;
- **pas d'ajout/suppression de série ou de panneau depuis le panneau** —
  ça reste le rôle du sous-menu « Tracer » et du glisser-déposer déjà
  décrits plus haut ;
- **zoom/pan n'y entrent pas, et ne doivent jamais y entrer** —
  `plotyx_render_egui::ViewState` vit sur `GraphView`, hors du `PlotSpec`
  (`docs/invariants.md`, `.claude/rules.md`) : ce panneau n'a et n'aura
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
position s'affiche « C »). `command::column_label` les désigne désormais par
leur position : « Saisie dans la colonne A ». Les autres natures de nœud
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
`tests/perf_write_under_snapshot.rs`.

## `plotyx-cli`

Binaire headless de vérification — **pas** l'UI. Il sert à valider que le
workspace se lie de bout en bout sans dépendre d'un choix d'UI. C'est un
squelette : il affiche une ligne et sort.
