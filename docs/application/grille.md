# Application : grille

Suite du sommaire [`README.md`](README.md). Contenu : lignes Metadata et x', menus contextuels et sélection multiple de la grille, gouttière, Tab et flèches, alignement, saisie.

### Lignes « Metadata » et x' de l'en-tête

Deux rangées facultatives, **réglées par classeur** : Classeur › Afficher la
ligne Metadata / Afficher la ligne x', le clic droit de la case « # » (sous la
section de la sélection), et, pour Metadata, le clic droit de son libellé. Le
réglage d'un classeur (`grid::HeaderRows`, `header_rows` / `set_header_rows`)
est gardé dans la mémoire d'egui sous l'identifiant du classeur, partagé par
ses fenêtres flottante et détachée, jamais dans le modèle ni le `.stf` ; il se
perd à la fermeture de l'application.

Le réglage des **nouveaux** classeurs est une préférence (Préférences ›
Display ; `preferences.json`, `workbook_show_metadata` affichée par défaut,
`workbook_show_x_prime` cachée par défaut ; `Action::SetWorkbookHeaderDefaults`).
`app.rs` la publie à chaque image dans le `Context`
(`grid::publish_header_row_defaults`) : un classeur que l'utilisateur n'a pas
réglé la suit, un classeur réglé la garde. La hauteur de l'en-tête perd une
sous-ligne par rangée masquée (`stacked_header_height`). Limite connue : la
navigation clavier peut encore atteindre la rangée x' masquée.


### Menus contextuels de la grille

Le classeur n'a plus de bandeau d'actions au-dessus de la grille. Un clic droit
dans l'espace vide à droite de la dernière colonne propose « Ajouter une
colonne » ; un clic droit sur un onglet de feuille propose « Ajouter une
feuille », « Renommer », « Dupliquer » et « Supprimer », sur la partie vide de
sa bande « Ajouter une feuille » seule — directement, sans sous-menu (#158). La bande reste sous la grille dans les
fenêtres flottantes et détachées. « Importer… » se trouve dans le menu File.
Son dialogue choisit entre le classeur actif, un nouveau classeur par fichier
et un seul nouveau classeur pour tout le lot. Chaque fichier valide crée sa
propre feuille ; les nouveaux classeurs ne sont créés qu'après analyse
réussie. Le lot est une seule entrée d'historique annulable.

Les cinq zones de la grille — cellule de données, champ d'en-tête d'une seule
colonne, rangée des lettres, gouttière des numéros de ligne, libellés de champ
entier dans cette même gouttière — portent le **même** bloc d'édition au clic
droit (`app::grid::grid_edit_menu`) : un groupe presse-papiers (Couper, Copier,
Copier avec l'en-tête, Coller, Coller à l'identique), puis Vider et Supprimer
et décaler. Undo et Redo n'y sont plus (Edit et le clavier, #158) ; chaque
entrée garde son équivalent au clavier, miroir du bloc de raccourcis de
`grid_contents`. La rangée des lettres y entoure ce bloc de ce qui lui est
propre : Tracer ▸ et Ajouter une colonne avant, Déplacer ▸, Type ▸ et Rôle ▸
après le presse-papiers (ils décrivent la colonne plutôt qu'ils n'agissent sur
son contenu).

**Déplacer ▸** (#319, `grid::move_columns_menu`, peint par
`column_type_role_menus` donc aussi dans Workbook › Columns) : quatre entrées,
Move to First · Move Left · Move Right · Move to Last, qui font bouger les
colonnes sélectionnées **dans leur feuille** (jamais d'une feuille à l'autre).
Left et Right avancent chaque colonne d'une place, l'écart entre colonnes non
contiguës reste (`A [B] C [D] E` → `B A D C E`) ; First et Last regroupent la
sélection en tête ou en fin, dans son ordre. Tout ou rien : une entrée est
**grisée**, raison en bulle, quand le geste ne changerait rien (Left si la
première colonne sélectionnée est déjà en A, Right symétrique, First et Last si
l'ordre ne change pas) ; une sélection de lignes entières, qui couvre toutes les
colonnes, grise les quatre. Une colonne : une entrée `Move “B”` ; plusieurs : une
macro `MacroKind::MoveColumns`, un seul Ctrl+Z. **Aucune** fenêtre « Colonne
utilisée » : depuis #321 une formule suit sa colonne. Les graphes déjà tracés ne
bougent pas ; seuls les tracés suivants apparient les Y aux X qui sont à leur
nouvelle gauche. La logique d'ordre est pure (`stokesforge-app-state`,
`column_move.rs`) et sert au grisage comme à l'exécution ; un enfant `Unknown`
sous la feuille garde sa place (l'index se traduit dans la liste complète des
enfants). La largeur d'une colonne suit son identifiant, donc sa colonne
(#325).

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
fenêtre dans `stokesforge-app-state/tests/shell.rs`, comme le reste de la matrice copier/coller.
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

### Sélection multiple de la grille (#159)

`selection.rs` porte la sélection de chaque feuille — état d'interface pur,
hors historique. Elle est **d'une seule sorte** à la fois (colonnes, lignes
ou cases), et la frontière en-tête/données reste infranchissable ; chaque
sorte est un **ensemble** et non plus une plage `{ ancre, curseur }` :

| Sorte | Contenu | Gestes |
|---|---|---|
| colonnes (`Columns`) | `base` (colonnes posées une à une) ∪ plage `ancre..=curseur` | clic · Ctrl+clic bascule la colonne · Maj+clic remplace tout par la plage ancre → cible · Ctrl+Maj+clic et glisser ajoutent la plage |
| lignes (`FullRows`) | intervalles de lignes (même modèle) | mêmes gestes sur les numéros de ligne |
| cases (`Rows`) | rectangles déjà posés (`extra`) + dernier rectangle `ancre`–`curseur` | Ctrl+clic ajoute ou retire une case (le rectangle est découpé autour) · Ctrl+glisser ajoute un rectangle · Maj+clic étend le dernier rectangle |

Un Ctrl+clic d'une autre sorte, ou de l'autre côté de la frontière
en-tête/données, repose une sélection simple. Les champs d'en-tête entiers
(`HeaderRows`) restent des plages simples. Retirer le dernier élément lève la
sélection.

Les consommateurs raisonnent par **blocs** (`Selection::blocks`, un bloc =
colonnes de la feuille × plage de rangs) ; l'ordre est toujours celui de la
feuille (gauche → droite, haut → bas), jamais celui des clics.

- **Case du coin « # »** et **Ctrl+A** (grille focalisée, hors édition ;
  Edit › Select All) : `Action::SelectAll`, toutes les colonnes visibles,
  en-tête compris, même sans ligne ; un second clic sur la case, ou Échap dans
  la grille, lève la sélection. La case a une info-bulle et le curseur « main ».
- **Copier / Couper** : colonnes non contiguës bout à bout dans l'ordre de la
  feuille ; blocs de cases qui partagent leurs lignes (côte à côte) ou leurs
  colonnes (empilés) — sinon Copy et Cut sont grisés (`GridMenu::copy_enabled`,
  `Selection::copy_layout`). Copy with Header suit les colonnes touchées.
- **Coller / Coller à l'identique** : grisés sur une sélection non contiguë
  (`GridMenu::paste_enabled`, cible ambiguë) ; le collage au clavier est refusé
  avec un message.
- **Format du presse-papiers (#259)** : le texte système est du TSV au format
  Excel/LibreOffice, écrit et lu par `stokesforge_core::tsv` (`encode`,
  `decode`, `encode_cell`), sans egui : une case qui porte une tabulation, un
  saut de ligne ou un guillemet est entourée de guillemets (guillemet doublé),
  donc un texte multiligne ne décale rien au collage, ici comme dans un
  tableur. À la lecture, `\r\n` et une fin de ligne terminale sont admis
  (collage depuis un tableur sous Windows). Une case **manquante** se copie
  vide : `cell_copy_text_in` la traite avant la mise en texte, jamais par un
  `replace` de `MISSING_TEXT_DISPLAY` après coup, qui effacerait aussi les
  espaces insécables vraiment tapés. L'app ne garde que le choix des cases.
- **Saisie et collage (#258)** : une seule règle de lecture « texte → valeur de
  cellule », `ValueBlock::from_texts(kind, &[&str])` (`stokesforge-core`,
  `column.rs`), appelée par `commit_cell` (une case) et par
  `value_block_from_tsv_column` (une colonne collée) : une date tapée ou collée
  donne la même cellule. Chaque case est rognée ; vide = valeur manquante ;
  `Numeric` lit un nombre fini (virgule décimale admise), `DateTime` lit d'abord
  une date ou une heure ; `inf`, `nan` et `1e400` restent du **texte** (dans les
  trois types, colonne `Mixed` déjà ouverte comprise — seule la conversion de
  type par l'en-tête garde `inf` écrit = infini, règle C2). Un texte illisible
  dans `Numeric`/`DateTime` rend un bloc `Mixed` et le drapeau « élargit » :
  écriture par `SetCellsPromoting` (inchangé), les cases lues restant des
  nombres, la date comprise. Après un collage qui écrase une colonne `Mixed`,
  si toutes ses cases présentes sont des nombres, elle repasse en `Numeric`
  (`SetColumnType`, comme après une saisie) ; la repromotion se fait **avant**
  la clôture du groupe `PasteCells`, et un collage d'une seule colonne n'ouvre ce
  groupe que lorsqu'une repromotion a lieu : un collage reste une entrée
  d'historique, un seul Annuler. Une colonne `DateTime` élargie en `Mixed`
  affiche ensuite la date comme un horodatage numérique, comme pour une date
  tapée (comportement existant, non modifié).
- **Effacer** vide chaque bloc, en un seul groupe annulable. **Supprimer et
  remonter** n'est offert que pour un bloc unique (des trous rendraient la
  remontée ambiguë) ; Suppr se contente alors de vider.
- **Clic droit** : dans la sélection, le menu agit sur toute la sélection, même
  non contiguë ; hors de la sélection, il la remplace par la case visée (#152).
- **Tracer** reçoit la liste ordonnée des colonnes sélectionnées (A en X, C et
  E en Y…). Type ▸ et Role ▸ passent par `Action::SetColumnsType`/`SetColumnsRole` : un seul
  pas d'annulation (`MacroKind::SetColumnsType`/`SetColumnsRole`) pour toutes les colonnes.

Pendant un glisser, l'extension au survol ne s'applique qu'aux cases où
l'appui n'a **pas** commencé (`press_started_in`) : sinon un Ctrl+clic, qui
bascule au relâchement, aurait déjà étendu la sélection pendant l'appui.


### Largeur des colonnes : par colonne, pas par position (#325)

La grille est une `egui_extras::TableBuilder`, dont la mémoire garde les
largeurs **par position** et les jette dès que le nombre de colonnes change :
supprimer ou ajouter une colonne remettait toute la feuille à zéro, et un
déplacement ne l'aurait pas suivie. La largeur est donc un fait de la
**colonne** : `NodeId -> f32` dans l'acteur (`SessionHandle::set_column_width`,
`Snapshot::column_widths`, `AppState::column_width` / `Action::SetColumnWidth`),
enregistrée dans le `.stf` (`docs/persistance.md`). Hors du modèle, hors de
l'historique : redimensionner **ne s'annule pas**, mais Ctrl+Z qui ramène une
colonne supprimée lui rend sa largeur (l'identifiant n'est jamais réutilisé,
invariant 7, et la table n'est pas purgée à la mutation).

`grid_contents` redonne à egui, à chaque image, `TableColumn::initial(largeur de
la colonne ou DEFAULT_COLUMN_WIDTH)` par colonne, et appelle
`TableBuilder::reset()` quand la liste ordonnée des identifiants (avec l'époque
du projet) diffère de celle de l'image précédente — jamais un `id_salt`
variable, dont dériverait l'identifiant de la zone de défilement verticale (le
défilement serait perdu). Une colonne neuve (ajout, collage, import, analyse)
prend la largeur par défaut. Pas d'ajustement au contenu par double-clic.

La largeur n'est poussée qu'à la **fin d'un redimensionnement fait à la main**
(`finished_column_resizes` : poignée relâchée, largeur lue sur l'étendue de
l'en-tête) : pousser la largeur rendue à chaque image marquerait le projet
modifié à chaque ouverture. La vue détachée a son propre `AppState` ; sa
largeur passe aussi par l'acteur, l'état principal la relit à son
rafraîchissement.

**Colonnes rognées (#330).** Les colonnes de données sont déclarées
`.clip(true)` : sans cela egui_extras fait grandir une colonne non rognée
jusqu'au contenu mesuré de ses cellules (`table.rs`, `max_used_widths`), et la
largeur par défaut (54) dépendait de ce qui était peint. Elle vaut désormais 54
partout, un contenu trop large est coupé. Le test
`une_colonne_jamais_reglee_garde_54_quelle_que_soit_la_fenetre` garde ce
comportement ; le « 57 puis 62 px au maximisage » remonté sous #330 n'a pas pu
être reproduit dans le banc (voir le message du commit).

**Remettre au défaut (#330).** Clic droit sur une lettre ou Workbook ▸ Columns :
« Reset Width » (après Role ▸), sur toute la sélection ; Workbook ▸ Sheet et
clic droit de l'onglet : « Reset Column Widths », toutes les colonnes de la
feuille. `Action::ResetColumnWidths { columns }` →
`SessionHandle::reset_column_widths` **retire les clés** de la table (le défaut
est la constante `DEFAULT_COLUMN_WIDTH` de l'application, rien n'est écrit dans
le `.stf`). Non annulable, sans confirmation, comme la pose. Entrée grisée
(« Columns are already at the default width ») quand aucune colonne visée n'a de
largeur retenue. La clé de mise en page de `grid_contents` contient les
largeurs retenues des colonnes visibles : sans elles, `TableColumn::initial`
n'est lu qu'après un `reset` et la remise ne changerait rien à l'écran.
`TableBuilder::reset` remet **toutes** les colonnes, gouttière comprise : la
largeur rendue de la gouttière est mémorisée par feuille en mémoire temporaire
d'egui (`gutter_width_id`) et redonnée en `initial`, sinon chaque glisser la
ramènerait à 90. La remise d'une colonne n'y touche pas ; celle de toute la
feuille l'oublie (`forget_gutter_width`) : retour à 90, jamais enregistré.

### Une colonne a deux identités, une seule est visible

Son **nom de nœud** (`Node::name` — « Temps », « Signal », ou le « A »/« B »
automatique de `Project::next_column_name`) est une clé du modèle : il résout
une colonne depuis un script (`pyst.resolve("[Book1]Sheet1!Temps")`), l'adresse
dans une spec de graphe, la retrouve dans un `.stf`. La grille ne l'affiche
**nulle part** : elle montre la lettre de **position** (A, B, C…) et les
quatre champs de `ColumnMeta` (Nom long, Unité, Commentaire, Formule), qui
sont encore autre chose.

**Ligne Formule (#321).** Une formule retient les colonnes qu'elle lit **par
identifiant** ; les lettres que la case montre et que l'on tape ne sont qu'un
affichage, relu à chaque image : insérer, supprimer ou déplacer des colonnes
(dont les sorties d'une analyse) ne change jamais ce que la formule lit, la
lettre affichée suit la colonne (`a*2` s'écrit `A*2`). Si une colonne lue n'est
plus dans la feuille, la case affiche `#REF` et peint un cadre rouge avec la
bulle « Colonne supprimée. Remplacer #REF par une lettre de colonne ». Trois
saisies sont **refusées** : une lettre qui ne désigne aucune colonne de la
feuille, la colonne elle-même, et `#REF` modifié. Entrée ou Tab sur un texte
refusé laissent le champ ouvert (cadre rouge, bulle qui dit pourquoi) ; un clic
ailleurs le ferme sans rien écrire. Valider le texte affiché tel quel (même avec
`#REF`) n'écrit rien. Une **erreur de syntaxe** n'est pas un refus : elle
s'enregistre et la colonne se remplit de `---`. Au collage d'un rectangle, la
formule refusée est la seule à ne pas être collée : les autres champs le sont, et
la barre d'état dit le refus après les écritures. La décision se prend dans
`stokesforge-app-state` (`header_edit_refusal`, `formula_issue`), la grille ne
fait que la peindre.

`stokesforge_commands::Command::label` construisait ses libellés d'historique sur
le nom de nœud, et annonçait donc « Saisie dans Temps » pour une colonne que
l'utilisateur ne connaît que comme « A » — un mot introuvable à l'écran, et
carrément trompeur quand les deux divergent (un nœud nommé `T` en troisième
position s'affiche « C »). Le libellé désigne désormais la colonne par sa
position (`ColumnRef`, depuis #46 une donnée et non une phrase) : « Edit Cells
in Column A ». Les autres natures de nœud
(feuille, classeur, graphe, note) gardent leur nom réel — celui-là *est*
affiché, dans le Project Manager.

La correspondance position → lettre est `stokesforge_core::spreadsheet_name`,
rendue publique pour l'occasion : c'est la même fonction qui nomme les
colonnes créées sans nom explicite, qui peint l'en-tête de la grille
(`app::grid::column_letter` n'en est plus qu'un alias) et qui écrit les libellés
d'historique. Trois copies de cet algorithme valaient trois façons d'appeler
la même colonne.

La gouttière (`app::grid::row_number_widget`) est le miroir exact de la rangée des
lettres (`app::grid::column_header_widget`) : toute sa largeur sélectionne, au clic,
**toute la ligne** — les colonnes visibles de la feuille, à cette ligne, quelle
que soit leur longueur propre (une colonne plus courte que la ligne visée
reste touchée par la sélection, seule sa portion réellement écrite est
affectée par Vider/Supprimer). `AppState::select_row`/`extend_row_selection`/
`is_row_selected` en sont les pendants de `select_column`/
`extend_column_selection`/`is_column_selected`, et `SelectionKind::FullRows`
(`stokesforge_app_state::selection`) le pendant de `SelectionKind::Columns` — même règle de
non-franchissement de frontière : un glisser qui part de la gouttière et croise
une cellule ou la rangée des lettres repose une sélection simple plutôt que
d'étendre silencieusement sans effet, et réciproquement.

### La gouttière côté en-tête : homogène, et elle aussi sélectionnable

Côté en-tête empilée (`app::grid::row_number_header_widget`), la gouttière portait
un « # » décoratif sur fond `header_letter_row_fill`, puis quatre rangées de
libellés (« Long Name », « Units », « Comments », « F(x)= ») restées
**transparentes** et inertes — la colonne de gauche n'était donc homogène ni
visuellement (le fond s'arrêtait au « # ») ni fonctionnellement (rien n'y
était cliquable). Les deux sont corrigés ensemble : chaque rangée de libellé
porte désormais le même fond que le « # » (ou le surlignage de sélection),
et un clic sur un libellé (`app::grid::header_row_label_widget`) sélectionne ce
champ pour **toutes** les colonnes visibles de la feuille — symétrique de
`row_number_widget` côté données. `AppState::select_header_row`/
`extend_header_row_selection`/`is_header_row_selected` en sont les pendants
de `select_row`/`extend_row_selection`/`is_row_selected`, et
`SelectionKind::HeaderRows` (`stokesforge_app_state::selection`) le pendant de `FullRows`
pour la zone d'en-tête : elle touche toutes les colonnes et, symétriquement,
ne touche jamais les données (`row_range()` y vaut toujours `None`).

Ce n'est **pas** la même chose qu'un champ sélectionné sur une seule colonne
(`app::grid::header_field_widget`, `Selection::header_field`) : celui-ci reste
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

Les libellés de la gouttière restent sur une seule ligne, même en français,
en pseudo-langue ou après réduction de sa largeur (#125). Si la place
manque, ils sont tronqués avec des points de suspension ; le survol affiche
le texte complet. Ils ne débordent pas sur les rangées voisines.

### Une palette à trois paliers pour la gouttière, et une teinte propre à son texte

La gouttière (rangée des lettres, « # », numéros de ligne, libellés de champ)
utilisait le même vert que le reste de l'accent de marque, à pleine
saturation pour son texte (`Visuals::strong_text_color()`, qui vaut
`widgets.active.fg_stroke.color`, donc littéralement `ACCENT`) — trop
appuyé pour une zone censée rester en retrait des données. `theme::
gutter_text_color` lui donne une teinte propre, nettement plus sombre,
indépendante de la couleur globale des éléments « actifs » pour ne pas
déteindre sur des boutons ou onglets sans rapport.

Le fond, lui, gagne un troisième palier. `app::grid::gutter_fill(selected,
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
l'image, voir `StokesForgeApp::ui`, `app.rs`) est sûr — la ligne existera à temps.
Une colonne neuve, elle, n'a pas encore de `NodeId` tant que `Command::
AddColumn` n'a pas tourné : la sélectionner à l'avance n'a pas de sens.
`Action::AddColumnAndSelect { sheet, row }` fait donc les deux d'un coup
dans son propre traitement (`AppState::add_column_and_select`) : ajoute la
colonne, *puis* — une fois `self.snapshot` rafraîchi par `execute` — sélectionne
`row` dans la dernière colonne visible. Câblé aux deux mêmes endroits que Tab
normal : le raccourci clavier direct de `grid_contents`, et la validation
d'édition de `cell_widget` — `app::grid::is_last_visible_column` factorise la
condition commune aux deux.

Câblé aussi **côté en-tête** (`Action::AddColumnAndSelectHeaderField`,
`AppState::add_column_and_select_header_field`), pour **chacun** des quatre
champs (Long Name, Units, Comments, Formule) de la dernière colonne, en
édition comme hors édition (#52) : Tab y ajoute une colonne et sélectionne le
même champ dans la colonne neuve. Jusqu'à #52, seul Formule le faisait — Tab
circulait d'abord colonne par colonne pour un même champ ; l'utilisateur a levé
cette décision, au prix du retour automatique au champ suivant de la première
colonne quand on remplit l'en-tête champ par champ.

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

F2 ouvre le même éditeur qu'un double-clic, avec le contenu initial conservé
(#143), uniquement lorsque l'ancrage de la grille a le focus. Sur une plage
de données ou de champs d'en-tête, il édite la case en haut à gauche, dans
l'ordre visible des colonnes. Nom long, Unité, Commentaire et Formule utilisent
leur éditeur existant ; les colonnes entières ne déclenchent pas cette édition.
Le renommage par F2 reste propre aux gestionnaires focalisés.

Limite : le filtre ne s'applique qu'à l'image qui suit la prise de focus
(`set_focus_lock_filter` exige `had_focus_last_frame`) — une touche tombant
dans l'image même du clic n'est pas une frappe réaliste. Verrouillé par
les tests de `app/grid.rs` (`app::grid::tests`). Le focus d'egui ne se voit pas dans une capture
(`STOKESFORGE_APP_SCREENSHOT`) : à confirmer sur une vraie fenêtre
(`run-stokesforge` en X11 aide, sans valoir Wayland natif).

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
  presse-papier avant de pousser l'`Event::Paste`), donc `app::grid::
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
gardent leurs nombres ([`commandes-et-historique.md`](../commandes-et-historique.md)).
Conversion et écriture tiennent dans une seule entrée d'historique : un
`Ctrl+Z` ramène le type *et* la valeur d'avant.

Avant les colonnes à types mixtes, la saisie était refusée avec un message
d'erreur, et noter un « n/a » sur une ligne obligeait à convertir toute la
colonne en texte — donc à rendre tout le reste intraçable. Sur une colonne
déjà mixte, plus rien n'est promu : chaque cellule décide pour elle-même, et
un nombre tapé y reste un nombre.

