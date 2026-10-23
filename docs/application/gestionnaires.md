# Application : Project Manager et Object Manager

Suite du sommaire [`README.md`](README.md). Contenu : Project Manager et fenêtres par contenu, graphique collé entre projets, Object Manager.

### Project Manager et fenêtres par contenu

Le menu contextuel des fenêtres (à plat) et le menu *Edit* › Selection proposent
Copier, Coller, Renommer, Dupliquer et Supprimer (#105) ; un dossier propose
Add Folder, Coller, Renommer (sur place) et Supprimer (#157), et son Copier et
son Dupliquer sont grisés (voir plus bas). Un clic sélectionne et ouvre l'objet ; Ctrl
bascule une cible sans ouvrir, Maj sélectionne une plage dans l'ordre de
l'arbre, y compris les fenêtres des dossiers repliés (#327).

**Dossiers sélectionnables (#170, point 4).** La sélection du Project Manager
est un ensemble d'entrées `PmEntry` (`app/project_manager.rs`) : un dossier ou
une fenêtre (son `Tab`). Un dossier se peint comme une fenêtre
(`list_row` : fond pâle, filet d'accent, survol). Un clic le **sélectionne** et
le déplie s'il est fermé, sans ouvrir de fenêtre (`open_requests` reste réservé
aux fenêtres) ; un clic sur un dossier ouvert le sélectionne **sans le
replier** ; un double-clic le replie, le triangle aussi. Suppr et F2 agissent
sur lui (une entrée d'historique, `DeleteObjects` / `RenameObjects`). Coller
avec ce seul dossier sélectionné colle **dans** ce dossier (`paste_parent`),
sinon à la racine. Un dossier supprimé quitte la sélection. **Copier et
Dupliquer un dossier sont refusés** : une copie locale garderait ses références
vers les classeurs d'origine, qui sont hors du dossier. Le menu grise les deux
entrées (`disabled-copy-folder`), et `AppState::refuse_folder_copy` refuse le
geste clavier avec la même phrase dans la barre d'état. Reste : une issue à
part pour copier un dossier avec ses dépendances.
F2, Ctrl+D, Ctrl+C, Ctrl+V et Suppr s'appliquent à cette sélection quand le
Project Manager a le focus, aussi en fenêtre détachée. Les renommages multiples
saisissent les noms successivement, puis valident une seule entrée annulable ;
Échap abandonne l'ensemble. Le champ se valide par Entrée ou perte du focus.
Open et Close sont dans le menu contextuel d'une entrée et agissent sur
toute la sélection : Open ouvre ou active (comme un clic), Close ferme par le
même chemin que la croix (fenêtres flottantes et détachées). Une fenêtre
fermée est atténuée dans l'arbre ; un clic sur sa ligne l'ouvre. L'arbre commence directement sous l'en-tête du panneau,
sans bandeau de compteur ni boutons intermédiaires.

La sélection utilise la forme arrondie des boutons, sans fond rectangulaire
ajouté autour des entrées (#144), y compris dans le Project Manager détaché.

La sélection simple suit les changements de fenêtre de contenu active (#146),
qu'elle soit flottante, maximisée ou détachée. Une sélection multiple est
conservée pendant les changements passifs et reste la cible des actions du panneau.
Un clic simple sur une entrée, ou un clic dans une fenêtre du workspace
(flottante ou maximisée), remplace la sélection multiple par la fenêtre visée
et réactive le suivi (#154), même si cette fenêtre est déjà active. Le survol,
le clic dans le vide et les panneaux recouvrant le contenu ne la remplacent pas.
Quand la fenêtre active reprend la sélection, elle devient aussi l'ancre de
Maj-clic dans le panneau fixe et détaché : la prochaine plage part donc de
cette fenêtre, et non de la dernière entrée cliquée dans Project Manager.
`follow_project_manager_active_tab` assure le suivi passif avant les menus ;
`window_stack` traite le clic explicite et demande la republication au Project
Manager détaché.

Les noms sont uniques dans le dossier parent. Les copies portent le premier
suffixe « copy N » / « copie N » libre et de nouveaux identifiants pour toute
leur descendance. Un graphe copié dans son projet garde ses références aux
colonnes originales. Un classeur dupliqué conserve son état ouvert ou fermé ;
les graphes qui l'utilisent ne sont pas dupliqués. Supprimer ferme les fenêtres
flottantes et détachées ; annuler restaure les objets sans rouvrir les fenêtres.
Rien ne demande confirmation (un seul Ctrl+Z rend tout, #229) ; le message
nomme les graphes restants qui en utilisaient les données.

Copier capture le contenu au moment du geste, puis l'écrit dans le presse-papiers
système ; le collage lit celui-ci, sans relecture des objets source. Les classeurs,
notes et matrices se transfèrent entre projets et entre processus ; les graphes
entre projets emportent leurs colonnes dans un classeur caché (#107).
Coller dans le menu d'un dossier y place les copies. La grille refuse ce transport d'objets au lieu de le
transformer en cellules. `Action::MoveObjects` prépare le déplacement entre
futurs dossiers en un seul groupe annulable ; le geste Couper reste à câbler lors
de leur introduction.



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
- **`Tab::Note`** est réellement éditable : `stokesforge-core` expose déjà le texte
  d'une note (`NodeKind::Note::text`) et une commande annulable
  (`Command::SetNoteText`). L'éditeur suit le même principe de saisie
  différée qu'une cellule de grille (`AppState::NoteEdit`, symétrique de
  `CellEdit` : le tampon n'atteint le modèle qu'à la perte de focus).
- **`Tab::Matrix`** est un **espace réservé documenté** : la fenêtre s'ouvre et
  se focalise comme les trois autres, mais ne montre qu'un message explicite
  — `stokesforge-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu à éditer, et
  l'éditeur de matrice complet est explicitement hors périmètre de cette
  tâche.

### Données d’un graphique collé entre projets (#107)

Le geste Copier fige aussi les colonnes X, Y, erreur X et erreur Y référencées
par toutes les séries et tous les panneaux. Un collage dans un autre projet
crée un classeur caché distinct par graphique et par collage, sans fusion ;
les feuilles source sont conservées avec des noms distincts si nécessaire.
Les colonnes non utilisées ne sont pas emportées. Les colonnes calculées
transportent leurs résultats figés, sans leur formule : le nouvel agencement
des colonnes et les dépendances omises ne doivent pas recalculer ces valeurs. Un collage dans le même
projet conserve ses références d’origine et ne crée aucun classeur.

Graphique › Afficher les données (Show Data), également au clic droit du
graphique, révèle et ouvre les classeurs référencés. Le masquage des données
embarquées est une métadonnée persistée du classeur, distincte des fenêtres
masquées de l’interface ; la révélation est annulable. Un collage entier
s’annule en un geste. Une référence déjà rompue reste rompue, même si la
destination comporte une colonne de même identifiant.

Le texte du presse-papiers est limité à 64 Mio (hexadécimal compris) ; le
sérialiseur s’arrête avant le dépassement et l’erreur est affichée sans
remplacer le presse-papiers. Le collage d’un transport trop grand est refusé.


## Gestionnaire d'objets (Object Manager, #81–85)

Affichage › Gestionnaire d'objets › Afficher ouvre l'arbre du
graphique actif : graphique › calques (`PlotSpec::panels`) › groupe de tracés ›
séries, ainsi que les objets rares, textes libres et objets graphiques
(rectangle, ellipse, ligne, flèche), quand il y en a. Les **textes
liés** (titre du graphique et des calques, titres d'axes, légende) n'y sont
jamais listés (#214) : ils s'éditent par double-clic sur la figure ou dans la
fenêtre des propriétés, et ne comptent pas dans les cases parentes. Le groupe
« Objects » ne liste que les objets libres, est replié par défaut et disparaît
quand il est vide. « Objects » a sa bascule « Afficher les objets » (`show_objects`, listé par défaut) ; décoché, ses objets ne comptent pas dans les cases parentes. Les **axes** (#111, #147) suivent le même
motif avec leur propre bascule, « Afficher les axes »
(`Action::SetObjectManagerShowScales`, préférence `object_manager_show_scales`,
même sous-menu Affichage et dans le menu du fond) : non listés par défaut, ils
ajoutent sous chaque calque, après le groupe de tracés, un groupe « Axes »
(`VisibilityTarget::Axes`, `LayerRow::axes`) qui réunit une ligne par axe — les
axes que la figure trace : bas, gauche, droite et haut, ces deux derniers
étant, sur un calque ordinaire, des **axes miroirs** (#176) — avec
une seule case qui commande `AxisSpec::visible` (ligne d'axe, graduations et
libellés d'un coup ; le titre garde son `title_visible`) ; pour le haut, et la
droite quand aucune série n'y est rattachée, elle commande `AxisSpec::mirror`
(`AxisRef::is_mirror`), et se grise quand un calque superposé de la case
occupe ce côté (`AxisRef::blocked`). La case du groupe
« Axes » commande ses axes et reste mixte tant qu'un miroir est décoché ; ses
Propriétés ouvrent l'onglet Axes (pages Haut et Droite : `mirror_axis_editor`,
« afficher l'axe miroir » et « afficher les étiquettes de graduation », ni
titre ni couleur). Un **calque superposé** (#171)
ne liste que les axes qu'il affiche (`object_manager::axis_sides`) : son axe X
nommé « X (haut) » si `x_top`, son axe Y nommé « Y (droite) » si `y_right`,
chacun commandant l'`AxisSpec` `bottom` / `left` qui le porte ; ses lignes
`right` / `top`, mortes, n'existent pas. Le groupe est **replié par
défaut** (#138) : un clic sur sa flèche déplie les lignes d'axe, sans toucher
au modèle. Les cases du graphique et du
calque ne comptent que les **enfants listés** (#173), regroupés dans `Shown` :
les axes sans « Show Axes » ; un enfant non listé garde son état. Axes listés, ils comptent
(calque et graphique mixtes si un miroir est décoché) ; la case du groupe de
tracés n'a que ses séries (`VisibilityTarget::contents_shown`,
`visibility_shown`, `set_shown`). Activer une bascule recalcule les parents. `Action::SetPlotVisibility` lit la bascule dans l'état. Le panneau est redimensionnable à droite ;
son bouton Détacher ouvre une fenêtre système, Rattacher le replace à droite. Il est
visible par défaut ; l'ouverture ou la fermeture choisie se mémorise dans
`preferences.json`, hors du projet et de l'historique.

Les réglages de vue (#139) sont regroupés dans **Affichage › Gestionnaire d’objets**,
aussi accessibles à plat au clic droit sur le fond (#152), même sans
graphique actif. Les deux filtres sont au premier niveau ; **Déplier par
défaut** règle graphique, calques, groupes de tracés, axes et objets. Les objets
sont regroupés par graphique ou calque, sans modifier leur visibilité.
Le menu est grisé lorsque le panneau est fermé. **Réinitialiser** rétablit les
filtres cachés, les axes repliés et les autres groupes dépliés ; les replis déjà
choisis restent libres et conservés. Les valeurs initiales ne s'appliquent
qu'aux groupes sans état egui, jamais à un groupe déjà peint ou manipulé.
`preferences::ObjectManagerSettings` est la table unique des défauts ; les
réglages vont dans `preferences.json`, hors `.stf` et historique. Les anciens
champs des deux filtres restent lus et écrits pour la compatibilité.
`object_manager_settings_menu` et `object_manager_contents` servent aussi le
viewport détaché ; une modification des réglages le réveille même si le
projet et la sélection n'ont pas changé.

Chaque série garde son nom dérivé de colonne et son échantillon de légende
(trait, couleur, épaisseur, marqueur ou barre). Les cases des parents modifient
ensemble leurs contenus ; le groupe commande seulement les séries. Une case
indéterminée signale une visibilité partielle. Masquer conserve données,
style, texte, position et rang ; une série masquée sort du tracé, de la légende
et des bornes automatiques. Les cases du graphique et du calque commandent toujours le contenu complet,
textes cachés compris. Les titres ont leur propre `title_visible` :
masquer un titre d'axe laisse l'axe en place. Une bascule écrit une seule
commande `EditPlotSpec`, avec le motif d'historique `PlotEditKind::Visibility`.

Cliquer un contenu dans l'arbre ou sur le tracé synchronise la sélection, en
gras dans l'arbre ; la série est surlignée sur la figure. Un contenu masqué
reste sélectionnable dans l'arbre. La partie format d'objet de la barre `Tools` règle le trait
d'une série ou d'un objet ; celle de texte règle le texte sélectionné. Le
panneau et ces barres suivent aussi une fenêtre de graphique détachée.

Le clic droit dans l'arbre ou la figure et Graphique › Sélection partagent
le même menu (`object_node_menu`, #152) : Afficher les données (courbe),
Renommer (F2), Déplacer vers le haut/bas (Ctrl+Haut/Bas), Afficher/Masquer,
Supprimer (Suppr) et Propriétés… en dernier, selon la cible. Un clic simple sur une ligne donne
le focus clavier au panneau : F2, Ctrl+flèches et Suppr agissent dès ce clic,
sans glisser préalable (#126). Le trait coloré et le nom de la série offrent
les mêmes gestes (sélection, clic droit, double-clic et glisser, #151).
F2 sur un groupe sélectionné dans la figure ouvre l'éditeur de son nom dans la figure (sur la légende), arbre masqué ou replié compris (#206).
Supprimer une série ne supprime aucune colonne de données ni les autres
courbes ; Annuler/Rétablir restaure sa place et ses réglages. Un clic ailleurs
rend ce focus.

Le glisser-déposer d'une série change son rang dans le groupe de son calque, avec un repère avant/après la ligne
visée ; un dépôt sur un autre calque est refusé. Le relâchement envoie une
seule action. La sélection suit le contenu déplacé, y compris après
Annuler/Rétablir, et une suppression décale les adresses des voisins sélectionnés.

Les propriétés choisissent la série et le calque exacts, même masqués ; le
sélecteur de calque est commun aux pages de la fenêtre ; il est grisé, avec
une info-bulle, sur « Graph » et « Theme » qui valent pour tout le graphique (#180). Un texte libre
réutilise son éditeur sur place. Un objet graphique ouvre un brouillon de
propriétés (coordonnées, trait, remplissage) : Valider envoie une action,
Annuler ne change rien. Les parents ont leurs propriétés et leur visibilité ;
la suppression concerne les feuilles, et le renommage les séries.

`object_manager.rs` porte la projection, la visibilité et les règles de
réordonnancement ; `app/object_manager_ui.rs` (#121) peint le contenu commun
ancré/flottant (`object_manager_contents`, menus, cases, glisser, raccourcis)
et porte la glu de sélection vers `GraphView` (`select_manager_target`,
`selected_manager_target`) ; ce qu'il demande à la vue remonte en
`ObjectManagerRequest`, exécuté après la peinture par
`StokesForgeApp::run_object_manager_request` (même module, #122, qui porte
aussi le panneau `object_manager_panel`) — le menu « Graph › Selection » passe par le même chemin. Les décisions de mutation restent dans
`AppState::dispatch`, avec les commandes de la session.

