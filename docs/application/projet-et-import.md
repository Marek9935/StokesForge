# Application : projet neuf, dépôt de fichiers, assistant d'import

Suite du sommaire [`README.md`](README.md). Contenu : nouveau projet et graphique vide, glisser-déposer de fichiers et son backend, assistant d'import.

### Nouveau projet et graphique vide (#73)

**File › New › New Project** (« Fichier › Nouveau projet », Ctrl+N) remplace le
projet courant dans la même fenêtre. La barre Fichier offre le même geste.
Un projet contenant du travail non enregistré, y compris une saisie locale
dans une fenêtre détachée ou un texte du graphique, demande « Discard Changes ·
Cancel · Save » (#229 ; Entrée = Save, qui enregistre puis poursuit) ; Cancel,
Échap ou la croix conservent le projet. File › Open…, File › Quit et la
fermeture de la fenêtre posent la même question (`Action::OpenProject`,
`Action::Quit`, `app/confirmations.rs`) ; un projet illisible ou un
enregistrement raté s'annonce par un dialogue bloquant (S-M8, `AppState::alert`)
et garde le projet et le chemin courants. Le repère est conservateur : toute
révision depuis l'enregistrement, y compris une annulation, ou une géométrie
modifiée demande confirmation. Une commande appliquée compte comme une
modification **même pendant un recalcul long**, avant que l'instantané soit
relu (`AppState::applied_revision`, #239) : fermer ou ouvrir pose la question.
Il ne conserve aucun ancien arbre en mémoire.
Un nouveau projet encore vide ne demande pas de confirmation.

Si l'acteur de session meurt (panic, #310), `AppState::check_session` (appelé
à chaque image par `poll_recalc` et par `refresh`) le détecte une seule fois :
l'indicateur de recalcul est abandonné, une copie de secours du dernier
instantané est écrite dans `recovery/` du dossier de données (jamais sur le
`.stf` de l'utilisateur, et elle peut précéder sa dernière commande), et un
dialogue bloquant (`AlertKind::SessionLost`) ne laisse que Close, qui ferme
l'application. Quit ne demande alors rien.

**Sauvegarde automatique et récupération (#317)** — `state/autosave.rs`,
`recovery.rs`. Activée par défaut, toutes les 5 minutes (préférences
`autosave`, `autosave_minutes`, bornes 1 à 60 ; File › Preferences › General,
section Files). `AppState::poll_autosave(now)`, appelé par `logic()` de la
fenêtre racine, écrit par `SessionHandle::save` le projet dans **un seul
fichier par processus** de `recovery/` (`<nom|Untitled>-<pid>.stf`, écrasé,
avec sa fiche `.json` et son verrou `.lock`), seulement si le projet a changé
depuis la dernière copie, le dernier enregistrement ou le démarrage. Ni
historique, ni `saved_revision`, ni chemin, ni titre « modifié » ne bougent, et
le `.stf` de l'utilisateur n'est jamais écrit. L'échéance saute (et
recommence 5 s plus tard) pendant une macro, un script, un import ou un
recalcul ; un échec d'écriture dit un seul avertissement. La copie est
supprimée à l'enregistrement, à la fermeture propre et pour New, Open d'un
autre projet et Discard ; une session perdue la garde. Au démarrage (`main`,
fenêtre racine), `scan_recovery` liste les copies sans verrou tenu et plus
récentes que leur `.stf` (copies de #310 comprises) dans la fenêtre de
récupération (Open, Ignore, Delete avec confirmation ; `docs/fenetres.md`) :
Open charge dans la fenêtre courante (derrière « Enregistrer ? » s'il y a du
travail), sous le chemin d'origine (vide pour Untitled), comme travail non
enregistré, et la liste reste ouverte ; Ignore laisse la copie, reproposée au
démarrage suivant.

Le remplacement passe par `SessionHandle::new_project`, vide l'historique et
les géométries et change l'époque pour invalider les anciennes références
Python. Les fenêtres de contenu, caches et sélections du projet précédent
sont oubliés, et ses fenêtres détachées se ferment. La visibilité des barres
et la langue sont conservées. Le geste est grisé pendant un script, un import
en cours ou une macro ouverte. Le premier **Save** (« Enregistrer ») demande
une destination, puis les suivants utilisent ce fichier.

**File › New › Add Graph** (« Fichier › Ajouter un graphique ») et la barre Fichier
créent un graphique vide à la racine : une zone de tracé, aucune série ni titre.
Sa fenêtre s'ouvre au premier plan. Il passe par la commande `AddGraph` existante :
Annuler le retire, Rétablir retrouve la même identité, et le `.stf` le conserve.
Ce geste n'a pas de raccourci supplémentaire ; il reste accessible au menu.

### Glisser-déposer de fichiers

Déposer des fichiers sur la fenêtre principale émet `Action::DropFiles`. Le
classeur flottant sous le curseur réclame le dépôt en se peignant
(`app::file_drop::claim_file_drop`) : il reçoit une feuille par fichier, sinon un nouveau
classeur est créé. Les `.stf` ne sont jamais importés : `AppState` les met en
file (`take_projects_to_open`) et `app.rs` lance un second processus
(`file_drop::open_project_in_new_window`)
`stokesforge-app <chemin>`. Chaque fichier utilise le modèle qui le reconnaît,
sinon le dernier modèle utilisé dans cette session, sinon la recette permissive.
Lecture, choix du modèle et analyse se font sur le fil de fond de l'import de
menu (#266) : `drop_files` ne garde sur le fil de l'interface que les refus et
la mise en file des `.stf`, puis passe par `start_import`, entrée unique de
l'import (indicateur et Stop comme File › Import ; un dépôt pendant un import
en cours est refusé). `poll_import` applique le résultat — une
seule entrée « Importer » — et pose le message des modèles utilisés ; un fichier qui ne donne aucune valeur numérique
ouvre à la place la fenêtre d'import pré-remplie, au retour du fil. Décision pure :
`state::file_drop_plan`. Le dépôt crée toujours de nouvelles feuilles, même
si la recette du dernier import remplaçait ou fusionnait une feuille. Le lien
File › Import › Reopen Last
Import… reprend ses fichiers dans l'assistant.

Le même geste est câblé dans les fenêtres détachées : sur un classeur,
la destination reste ce classeur ; sur un graphe, une note, une matrice ou le
Project Manager, un nouveau classeur. Les dépôts sont isolés par viewport et
leurs actions reviennent à la session principale. Un dépôt est refusé tant
que les propriétés du graphe sont ouvertes, pour garder leur macro
d'historique indépendante de l'import ; si elles s'ouvrent pendant que le fil
lit, `poll_import` garde le résultat et l'applique après leur fermeture. Les événements de dépôt sont
testés sans fenêtre. Le dépôt réel depuis Fichiers en Wayland natif est
confirmé par l'utilisateur le 2026-10-02 ; les fenêtres OS détachées et
les autres cas du protocole ci-dessous restent à valider manuellement.

### Assistant d'import

« Importer… » ouvre un assistant en six étapes façon Origin (`import_window`,
`ImportStep` ; module `src/app/import_wizard.rs`, #118, seul point d'entrée
appelé par `app.rs`), **dialogue modal de la brique commune** (`modal_dialog_with` :
même cadre, même barre de titre, même blocage du reste de l'interface que les
propriétés du graphe ; *Finish* = bouton par défaut, *Back* et *Next* dans le
pied de page) : fichiers et modèle, format, en-tête et fin des données,
colonnes, métadonnées, destination. Tous ses textes passent par `tr!` (clés
`import-*` du catalogue `en`, rédigées selon `docs/textes-interface.md` § 2.4 :
*Delimiter*, *Template*, *Subheader*…) ; les noms de modèles et le contenu du
fichier restent des données, jamais traduites. L'aperçu du fichier reste à droite à
toutes les étapes ; un clic droit sur une ligne règle l'import à cet endroit
(`AppState::import_line_action`). *Finish* importe dès l'étape 1 avec le
modèle tel quel. Les octets des fichiers sont lus une fois (cache par chemin,
`ImportState::bytes`) ; chaque fichier montre avant l'import ses lignes lues ou
son erreur. L'en-tête est un seul réglage (`ImportRecipe::unify_header` ramène
`HeaderSkip::Lines` au découpage structuré) ; les rôles de colonne choisis à
l'étape 4 vivent dans la recette (`column_roles`). La section *Column names*
est grisée et inactive quand *Names in first line* est coché : les noms
viennent alors de cette ligne. Sinon, les choix *Filename* et *Header key*
sont actifs ; leurs valeurs sont conservées pendant le grisage. Un dépôt de fichiers pendant
que l'assistant est ouvert les ajoute à sa liste. *Finish* lance
`Action::StartImport` : l'analyse des fichiers tourne sur un fil de fond (avec
progression « Import en cours… k/n », `AppState::poll_import` appelé à chaque
image) et les feuilles se créent à la fin, en une seule entrée « Importer ».
Raccourci : **Ctrl+"** (`SHORTCUT_IMPORT` : touche du 3 sans Maj sur AZERTY,
`SHORTCUT_IMPORT_QWERTY` : Ctrl+Maj+' sur QWERTY). Fichiers récents et dossier
mémorisé : `stokesforge_import::RecentFiles` (`import_recent.json`, à côté des
recettes). « File › Import › Reopen Last Import… » (`Action::ReopenLastImport`)
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

**Fin des données.** Par défaut aucune coupure (les données vont jusqu'à la fin
du fichier) : la coupure automatique perdait sans avertissement les lignes qui
suivaient une mesure manquante (« n/a ») en X. Elle reste un choix de l'assistant,
et les modèles Jasco la demandent. Elle s'arme au premier X numérique,
puis coupe au premier X non numérique (ligne vide comprise). Le texte et les
lignes vides qui précèdent ce premier nombre sont conservés ; un texte dans Y
ne coupe pas. Le préfixe `skip_chars` est retiré avant ce test ; avec le
séparateur espaces, X est le premier jeton non vide. L'étape « En-tête et fin
des données » propose aussi « Aucune coupure », première ligne vide, marqueur,
N lignes après l'en-tête ou N lignes avant la fin. Le clic droit sur l'aperçu
permet d'annuler la coupure automatique ; l'aperçu et l'import utilisent le
même découpage (`stokesforge_import::layout`, `data_end`).

**Format des nombres, dates et encodage (#314).** Un séparateur « espaces »
(choisi, ou résolu depuis Auto) avec l'espace pour milliers est refusé, à
l'aperçu (message dans la fenêtre) comme à l'import : `1 234,5` serait coupé en
deux colonnes ; `;` et tabulation acceptent ces milliers. Le réglage « Ordre des
dates » (jour d'abord par défaut, ou mois d'abord) règle `03/04/2026` ; l'ISO ne
change pas. En encodage Auto, un fichier presque UTF-8 est décodé en mixte : les
séquences UTF-8 valides sont gardées, chaque octet invalide est lu en
Windows-1252 ; un fichier Windows-1252 dont deux octets forment par hasard une
séquence UTF-8 (`Ã©`) est lu `é` — forcer l'encodage dans ce cas.

**Plusieurs blocs.** `BlockSplit` découpe un fichier qui enchaîne plusieurs
spectres (à chaque ligne commençant par un texte comme `TITLE`, ou aux lignes
vides) : `parse_blocks` rend un tableau par bloc, chacun avec son en-tête, et
l'import crée une feuille par bloc (`fichier_1`, `fichier_2`…) ou les répartit
selon `FilesLayout`. L'aperçu montre le premier bloc et le nombre de blocs.

**Actualiser.** Chaque colonne importée garde sa source
(`ColumnMeta::import_source` : chemin, recette en JSON, index de colonne).
« File › Import › Refresh from Source » (`Action::RefreshFromSource`)
relit les fichiers et réécrit les valeurs **en place** : les colonnes gardent
leur identité, les graphes restent valides. Les lignes de la feuille suivent le
plus long des nouveaux contenus ; elles ne raccourcissent que si toutes les
colonnes sont actualisées.

### Glisser-déposer : le backend de fenêtre

Sous Linux, winit choisit désormais **Wayland natif par défaut** lorsqu'il
est disponible (#72). `STOKESFORGE_X11=1` force X11/XWayland en secours.
`STOKESFORGE_WAYLAND=1` n'est plus nécessaire. eframe/egui-winit restent en
0.36.2 ; les patches Cargo sélectionnent winit 0.30.13 et smithay-clipboard
0.7.3 dans `vendor/` en attendant le support amont stable.

Le module transitoire reçoit `text/uri-list` sur **le même récepteur que le
presse-papiers**. Ce worker est partagé par tous les viewports : créer un
second récepteur vole le dépôt au premier sur Mutter, comme l'a montré le
premier prototype. Un canal faible remet à winit les chemins et la position,
sans objet Wayland ni nouveau fil. Il émet les événements habituels pour chaque
fenêtre native, donc le chemin de dépôt de l'application et des fenêtres
détachées reste commun. Le pipe est lu sans attente bloquante, avec une liste
d'URI limitée à 1 Mio et un délai de 10 secondes. La position au lâcher reste
celle utilisée pour choisir le classeur. Voir
[`vendor/winit/STOKESFORGE.md`](../vendor/winit/STOKESFORGE.md) pour le choix
technique, les alternatives, le protocole manuel GNOME/KDE/wlroots et le
retrait lors du passage à winit 0.31.

Le défaut du premier prototype a été reproduit avec un dépôt réel depuis
Fichiers sur GNOME. Après correction, l'utilisateur confirme le fonctionnement
du dépôt en Wayland natif le 2026-10-02.
**Les fenêtres détachées et KDE/wlroots restent à valider manuellement** ;
les tests automatiques couvrent le parseur, le canal, sa durée de vie et la
décision d'import, pas le compositeur.

