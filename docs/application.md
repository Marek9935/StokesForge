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
porte le bouton de bascule, grisé quand l'onglet actif est le Project Manager
lui-même (le maximiser n'aurait pas de sens : il est déjà toujours visible).

### Détachement du Project Manager

Deux mécanismes, tous deux disponibles :

- **Flottant dans la fenêtre mère** : glisser l'onglet Project Manager hors du
  dock le fait flotter en `egui::Window`, via le mécanisme natif d'`egui_dock`
  (`Surface::Window`) — gratuit, aucune configuration ne le restreint pour cet
  onglet (`TabViewer::allowed_in_windows` reste à sa valeur par défaut,
  `true`).
- **Vraie fenêtre OS séparée** : un bouton du menu « Fenêtre » ouvre le
  contenu du Project Manager dans un vrai viewport natif `eframe`
  (`egui::Context::show_viewport_deferred`), déplaçable sur un autre écran —
  ce que le flottant `egui_dock` ne peut pas faire, lui qui reste borné à la
  fenêtre OS principale. Dans ce mode, `Tab::ProjectManager` est retiré du
  `DockState` (`PlotyxApp::pm_detached`) pour ne jamais être montré à deux
  endroits à la fois ; fermer la fenêtre OS ou cliquer « Réattacher » le
  repousse proprement dans le dock.

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
- **Une seule cellule éditable à la fois**, toutes grilles de tous les
  classeurs ouverts confondues. La grille est virtualisée
  (`TableBuilder::body().rows()`) : une feuille peut avoir un million de
  lignes, il n'est pas question de garder un tampon par cellule. Le même
  principe vaut pour l'édition d'une note (`AppState::NoteEdit`), pour la même
  raison de cohérence plutôt que de nécessité stricte.
- **La disposition dockable ne persiste pas d'un lancement à l'autre** :
  `PlotyxApp::new` la reconstruit à chaque démarrage (Project Manager, script,
  premier classeur, premier graphe). Ce que l'utilisateur redispose en
  session — y compris la maximisation et le détachement du Project Manager —
  tient jusqu'à la fermeture de la fenêtre, pas au-delà.

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
