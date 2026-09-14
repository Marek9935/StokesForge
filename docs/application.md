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
principe (voir plus bas) : la disposition est **dockable** (`egui_dock`), et
Save/Load passent par un vrai sélecteur de fichiers natif (`rfd`) — ce ne sont
que des charpentes, pas les panneaux définitifs qu'elles accueilleront.

Toolkit : `eframe` / `egui`, `egui_dock` (disposition), `rfd` (sélecteur de
fichiers natif).

### Disposition dockable

Tous les contenus de ce banc sont des onglets d'un `egui_dock::DockState<Tab>`
peint par une `DockArea`, dans `app.rs`. `Tab` a six variantes : le Project
Manager, la console de script — tous deux structurels, jamais fermés — et
quatre natures de fenêtre portant chacune un `NodeId` : `Book` (classeur),
`Graph`, `Matrix`, `Note`. La disposition initiale (Project Manager au centre,
script amarré en bas, premier classeur du projet puis premier graphe à droite
s'ils existent) n'est qu'un point de départ : l'utilisateur redispose,
redimensionne et referme les onglets de fenêtre librement, `egui_dock` retient
la disposition d'une image à l'autre.

Un onglet par fenêtre **ouverte**, quelle que soit sa nature : `PlotyxApp::
open_tab` est le seul endroit qui décide « ouvrir » vs « focaliser » (cliquer
une seconde fois la même entrée du Project Manager ne duplique jamais son
onglet). Un onglet `Tab::Graph` par graphe ouvert garde son propre
`plotyx_render_egui::GraphView` (donc son propre cache de formes —
`PlotyxApp::graph_views` est une table indexée par `NodeId`).

Ce qui n'est **pas** fait : pas de bouton « nouveau panneau », pas de
persistance de la disposition entre deux lancements, pas de panneau
Properties ni de grille à délégués, pas d'éditeur de matrice — hors périmètre
de cette fondation (voir « Project Manager et fenêtres par contenu »
ci-dessous pour le détail de ce qui est, et n'est pas, câblé).

### Project Manager et fenêtres par contenu

`Tab::ProjectManager` affiche l'arbre du projet **arrêté** à `Workbook`,
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

### Maximiser un onglet

`egui_dock` 0.21 n'expose aucune fonction « maximiser » native. Plutôt que de
faire mentir le `DockState` lui-même, `PlotyxApp::maximized_tab` est un état
**à côté** du plan de disposition, qui ne le touche jamais : quand il vaut
`Some`, la `DockArea` normale n'est pas peinte du tout cette image-là — une
zone séparée montre uniquement l'onglet maximisé, plein cadre, à côté d'un
panneau fixe qui garde le Project Manager visible et accessible (le principe :
l'utilisateur doit toujours pouvoir y naviguer, onglet maximisé ou non).
Restaurer revient à repeindre la `DockArea` normalement : la disposition
qu'elle retient n'a jamais bougé pendant la maximisation, donc « restaurer »
est exactement la disposition d'avant, pas un recalcul. Un menu « Fenêtre »
porte le bouton de bascule.

Le Project Manager est maximisable comme n'importe quel autre onglet — plus
d'exclusion particulière pour lui. Deux endroits proposent l'action : le
menu « Fenêtre » (« Maximiser l'onglet actif », qui grise seulement quand
aucun onglet n'est focalisé) et le clic droit sur l'onglet PM lui-même
(« Maximiser »/« Restaurer », voir la section suivante). Un détail de mise en
œuvre en découle : quand le PM *est* l'onglet maximisé, le panneau fixe qui
garde d'ordinaire le PM visible à côté d'un autre onglet maximisé serait un
doublon de lui-même — `PlotyxApp::ui` ne le peint donc que si l'onglet
maximisé n'est pas le PM, et peint son arbre cliquable directement dans la
zone de maximisation dans le cas contraire (`tab_contents`, la fonction
partagée dock/maximisation, ne sait délibérément pas peindre
`Tab::ProjectManager` — il lui manquerait `open_requests` — donc ce n'est pas
elle qui est appelée dans ce cas précis).

### Attacher le Project Manager à gauche ou à droite

`PlotyxApp::pm_side` (`PmSide::Left` par défaut, comme la disposition
initiale) mémorise le dernier côté choisi pour le PM **dans le dock** —
orthogonal au détachement en fenêtre OS ci-dessous, qui est un troisième état
combinable avec n'importe quel côté mémorisé. Deux actions explicites le font
basculer, jamais une détection de bord pendant un glisser (délibérément
rejeté) :

- le menu « Fenêtre » du haut (« Attacher à gauche »/« Attacher à droite »,
  grisées sur le côté déjà actif, et pendant un détachement en fenêtre OS —
  rien à réordonner dans le dock tant qu'il n'y est pas) ;
- le clic droit sur l'onglet PM lui-même (`AppTabViewer::context_menu`), même
  grisage.

`PlotyxApp::set_project_manager_side` retire l'onglet du `dock_state` (où
qu'il soit) puis le réinsère par un split gauche/droite du nœud que
`PlotyxApp::top_region_node` désigne comme « la région du haut » — celle qui
contient les onglets de contenu, au-dessus de la console de script, qui garde
toujours toute la largeur en bas. Ce nœud est retrouvé à chaque appel plutôt
que mémorisé : les indices d'`egui_dock` 0.21 (`NodeIndex`) désignent une
position dans un arbre binaire indexé par formule, pas un identifiant stable —
`remove_tab` peut réindexer les nœuds restants (`Tree::remove_leaf`) quand un
nœud se retrouve vide, silencieusement. `top_region_node` part à la place du
nœud de `Tab::Script` (structurel, toujours présent) et remonte à son parent,
dont l'autre enfant est par construction la région du haut telle que
`PlotyxApp::new` l'a créée (`split_below` place l'ancien contenu à
`parent.left()`, la nouvelle feuille Script à `parent.right()`) — un repère
qui résiste aux redispositions habituelles à l'intérieur de cette région,
tant que la console elle-même n'est pas déplacée hors de son propre nœud
(aucune action de ce module ne le fait). `reattach_project_manager`
(ci-dessous) utilise le même mécanisme pour revenir au côté mémorisé, plutôt
que `push_to_focused_leaf` (au hasard) comme avant cette fonctionnalité.

Ce mécanisme ne restreint jamais le glisser-déposer natif d'`egui_dock` :
l'onglet PM reste `draggable` comme les autres (le désactiver casserait aussi
son détachement en fenêtre OS, constaté pendant une bisection antérieure).
Le rectangle/l'icône de survol qu'`egui_dock` peint pendant un glisser pour
indiquer où l'onglet atterrirait (`egui_dock::style::OverlayStyle`) est en
revanche rendu **entièrement transparent, globalement, pour tous les
onglets** (`selection_color`, `button_color`, `button_border_stroke`,
`hovered_leaf_highlight.color`/`.stroke`, tous mis à `Color32::TRANSPARENT`/
`Stroke::NONE` sur un `Style` dérivé de `Style::from_egui` avant de le passer
à `DockArea::style`) : cette version d'`egui_dock` n'offre aucun crochet par
onglet pour ce réglage, et le laisser actif ferait doublon visuel avec les
actions explicites ci-dessus tout en suggérant, à tort, qu'un « bon » endroit
de dépôt serait détecté pendant le glisser. Le glisser-déposer lui-même
continue de fonctionner à l'identique — seul son rendu de zone de dépôt
disparaît.

### Détachement du Project Manager

Deux mécanismes, tous deux disponibles :

- **Flottant dans la fenêtre mère** : glisser l'onglet Project Manager hors du
  dock le fait flotter en `egui::Window`, via le mécanisme natif d'`egui_dock`
  (`Surface::Window`) — gratuit, aucune configuration ne le restreint pour cet
  onglet (`TabViewer::allowed_in_windows` reste à sa valeur par défaut,
  `true`).
- **Vraie fenêtre OS séparée** : le menu « Fenêtre » du haut, **et** depuis
  cette tâche le clic droit sur l'onglet PM lui-même (« Détacher en fenêtre
  OS », utile pour un usage double-écran sans lâcher la souris pour la barre
  de menu), ouvrent le contenu du Project Manager dans un vrai viewport natif
  `eframe` (`egui::Context::show_viewport_deferred`), déplaçable sur un autre
  écran — ce que le flottant `egui_dock` ne peut pas faire, lui qui reste
  borné à la fenêtre OS principale. Dans ce mode, `Tab::ProjectManager` est
  retiré du `DockState` (`PlotyxApp::pm_detached`) pour ne jamais être montré
  à deux endroits à la fois ; fermer la fenêtre OS ou cliquer « Réattacher »
  (menu du haut ou clic droit) le repousse proprement dans le dock, **au côté
  mémorisé par `PlotyxApp::pm_side`** (voir la section précédente) plutôt
  qu'au hasard.

  **`show_viewport_deferred`, pas `_immediate`** : ce mécanisme utilisait
  `show_viewport_immediate` jusqu'à ce que l'utilisateur constate,
  interactivement sur Fedora/GNOME/Wayland (compositeur Mutter), que la
  fenêtre détachée restait draggable mais visuellement **confinée au
  rectangle de la fenêtre mère**, même celle-ci réduite — pas seulement
  l'absence de positionnement absolu déjà connue pour Wayland (voir
  ci-dessous), autre chose empêchait la fenêtre de sortir du cadre.
  `show_viewport_immediate` documente lui-même un rendu couplé entre les
  deux fenêtres (« pause the current viewport and render the child viewport
  in its own window ») ; `egui` recommande explicitement
  `show_viewport_deferred` pour une vraie fenêtre indépendante, repeinte
  indépendamment de la fenêtre parente — un candidat plausible pour ce
  symptôme, **pas une preuve**. Si le symptôme persiste malgré ce
  changement, ce sera à documenter ici comme limite connue plutôt qu'à
  retenter une autre bascule sans piste nouvelle.

  Contrainte technique de cette forme : le callback est `Fn(&mut Ui,
  ViewportClass) + Send + Sync + 'static`, donc sans capture de `&mut self`
  ni d'emprunt classique de `AppState` (il peut être rappelé seul, à une
  image où la fenêtre mère ne peint pas). Plutôt que de mettre `AppState`
  entière derrière un verrou, `PlotyxApp` porte deux petites structures
  dédiées derrière un `Arc<Mutex<_>>` chacune, décrites dans la doc de tête
  de `PlotyxApp::show_detached_project_manager` (`crates/plotyx-app/src/
  app.rs`) : `ProjectManagerViewModel` (une copie de l'arbre du projet,
  resynchronisée depuis `self.state` à chaque image avant l'appel) pour ce
  que la fenêtre détachée lit, et `DetachedProjectManagerRequests` (clics
  d'ouverture, demande de réattachement) pour ce qu'elle écrit — drainée par
  `PlotyxApp::ui` juste après, qui produit à partir de là les mêmes
  `open_requests`/`Action` qu'avant, sans dupliquer `PlotyxApp::open_tab` ni
  appeler `reattach_project_manager` depuis l'intérieur du callback.

  **Réserve Wayland connue**, sur le même modèle que celle déjà documentée
  pour `rfd`/`xdg-portal` : le compositeur peut ignorer une position de
  fenêtre demandée par l'application. Cette tâche ne demande explicitement
  aucune position pour la fenêtre détachée (elle laisse le compositeur la
  placer), donc n'en souffre pas directement — la limite vaut néanmoins d'être
  connue avant d'y ajouter un jour une position mémorisée entre deux
  détachements.

### Détachement générique des onglets de contenu

`PlotyxApp::detach_tab`/`reattach_tab`/`show_detached_tabs` généralisent le
mécanisme ci-dessus à `Tab::Book`, `Tab::Graph`, `Tab::Matrix` et `Tab::Note` :
un clic droit sur l'onglet (`egui_dock::TabViewer::context_menu`, le seul
point d'extension de ce genre en 0.21.1) propose « Détacher dans une
fenêtre », symétrique du bouton « Réattacher au dock principal » côté fenêtre
détachée. Plusieurs onglets peuvent être détachés en même temps — une entrée
par onglet dans `PlotyxApp::detached_tabs`, chaque `Tab` portant déjà un
`NodeId` donne un `ViewportId::from_hash_of(tab)` unique par construction,
sans collision possible entre deux classeurs détachés simultanément par
exemple. `Tab::ProjectManager` garde son propre menu contextuel, avec ses
propres actions (voir les deux sections précédentes) plutôt que celui-ci ;
`Tab::Script` n'a aucune entrée de menu contextuel, n'étant ni détachable en
fenêtre de contenu ni le PM (voir plus bas).

**Tailles par défaut des fenêtres détachées** (`show_detached_tabs`) :
`Tab::Graph` garde un ratio 4:3 (800×600). Le reste (`Book`/`Matrix`/`Note`)
partage une taille uniforme de 420×360, calculée pour montrer 3 colonnes de
grille : `grid_contents`/`matrix_contents` utilisent
`TableColumn::exact(56.0)` pour la colonne de numéro de ligne puis
`TableColumn::initial(110.0)` par colonne de données, soit 56 + 3×110 = 386 ;
420 ajoute une marge pour les bordures et la scrollbar verticale de la
`ScrollArea`. `Tab::Note` n'a pas de grille à proprement parler mais reçoit la
même taille que « les autres » pour rester cohérente avec eux — voulu, pas un
oubli.

**Une seule structure de `ViewModel`, pas une par nature de contenu.** Le PM
n'a qu'une forme de contenu (un arbre en lecture seule) ; les quatre natures
détachables ici sont franchement différentes (grille éditable virtualisée,
graphe interactif avec cache de formes, texte éditable, espace réservé).
`DetachedTabViewModel` porte un champ par nature (`selected_sheet`/
`cell_edit` pour un classeur, `note_edit` pour une note, `graph_view` pour un
graphe) plutôt que quatre types disjoints à faire cohabiter dans la table des
onglets détachés — les champs inutilisés pour la nature du `Tab` servi
restent à leur valeur par défaut.

**La lecture du modèle est factorisée, l'édition transitoire ne l'est pas.**
`tab_contents` (déjà partagée par le dock normal et la zone de maximisation)
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

Le tampon de saisie transitoire (`AppState::editing`/`note_editing`), en
revanche, n'est **pas** partagé avec `AppState` : un onglet détaché a son
propre tampon (`DetachedTabViewModel::cell_edit`/`note_edit`), pas le verrou
unique app-wide du dock. La limite déjà documentée plus bas (« une seule
cellule éditable à la fois, toutes grilles confondues ») devient, pour un
onglet détaché, « une seule cellule éditable à la fois **dans cette fenêtre
détachée** » — conséquence assumée de la même contrainte technique que pour
le PM, pas une nécessité produit. `detached_cell_widget`/
`detached_note_contents` réimplémentent donc la petite machine à états
(`gained_focus`/`lost_focus`/Échap) de `cell_widget`/`note_contents` sur ce
tampon local plutôt que sur celui d'`AppState` — quelques lignes dupliquées
plutôt qu'une abstraction supplémentaire pour un couplage aussi mince.
Valider une saisie produit la **même** `Action::CommitCell`/`Action::
SetNoteText`, appliquée par `AppState::dispatch` : le chemin de commande
annulable ne connaît aucune différence entre un onglet docké et un onglet
détaché.

`Tab::Graph` échappe à cette limite : `GraphView` n'a pas de verrou global à
`AppState` à respecter (son cache de formes et sa navigation zoom/pan sont
déjà scopés par graphe, dans `PlotyxApp::graph_views`) — `graph_view_contents`
est donc **littéralement** la même fonction pour le dock et pour une fenêtre
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
applique la palette (`theme::visuals`) : accent bleu-pétrole (sélection,
liens, contours des widgets actifs/survolés), coins légèrement moins arrondis
que le défaut egui. Le reste (gris de fond, contrastes de texte) vient tel
quel du thème clair d'egui, déjà calibré pour la lisibilité.

Deux raisons à ce choix, documentées en tête de `theme.rs` : `plotyx-render`
dessine les tracés sur un fond clair implicite (axes, grilles, texte noir) —
un thème sombre romprait ce contraste sans qu'aucun code de rendu n'en soit
informé ; et un thème qui change avec le système est un thème qu'on ne peut
pas garantir en capture d'écran ni en support.

### Sélecteur de fichiers natif

`tool_bar` n'a plus de champ de texte pour taper le chemin à la main : les
boutons « Enregistrer sous… » et « Ouvrir… » ouvrent un `rfd::FileDialog`
natif, modal. Ctrl+S/Ctrl+O (et les entrées de menu équivalentes) continuent
d'agir directement sur le chemin déjà connu, sans dialogue — le geste
« enregistrer »/« recharger » rapide d'un tableur, distinct de « enregistrer
sous ».

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
`AddColumn`, `InsertRows`, `DeleteRows`, `SetNoteText`, `RunScript`,
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
- **La disposition dockable ne persiste pas d'un lancement à l'autre** :
  `PlotyxApp::new` la reconstruit à chaque démarrage (Project Manager, script,
  premier classeur, premier graphe). Ce que l'utilisateur redispose en
  session — y compris la maximisation et le détachement, du Project Manager
  ou de tout autre onglet — tient jusqu'à la fermeture de la fenêtre, pas
  au-delà.

### Un détail qui a l'air d'un détail

`MISSING_TEXT_DISPLAY` vaut `\u{00A0}` — un espace insécable — pour afficher une
valeur texte **absente**. Il occupe la cellule comme un blanc ordinaire (rien ne
saute aux yeux) tout en restant, en mémoire, une chaîne différente de `""`, donc
distincte d'une chaîne vide **présente**. Un caractère visible (`—`, `∅`, `?`)
aurait été une réponse légitime à « qu'est-ce qu'un utilisateur pourrait vouloir
taper ici ? » ; un espace insécable n'est accessible à la frappe sur aucun
clavier usuel.

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
