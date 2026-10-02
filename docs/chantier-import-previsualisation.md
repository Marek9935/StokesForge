# Chantier « prévisualisation d'import » — note de passation

**Ce document n'est pas une page de référence.** Contrairement au reste de
`docs/`, qui décrit le code tel qu'il est, celui-ci décrit un *chantier* : ce
qu'il visait, ce qu'il a trouvé en chemin, et ce qu'il laisse ouvert. Sa
durée de vie est limitée — quand la colonne « ce qui reste » sera vide, il
rejoint `docs/archive/`. Le cahier des charges complet, verbatim, vit dans
`docs/chantiers/prevision-import.md`.

Le *quoi* et le *pourquoi* du résultat, eux, vivent au plus près du code :
`crates/stokesforge-import/src/preview.rs` (tête de module) pour le budget de
coût de l'aperçu, `crates/stokesforge-import/src/recipe.rs` pour le modèle
(`HeaderLayout`, `SubheaderRoles`), `crates/stokesforge-app/src/app.rs`
(`import_window`, `grid_contents`) pour le panneau et le bug de champ de
texte.

## L'objectif

Rappel du point non négociable du cahier des charges : pendant que
l'utilisateur règle les paramètres d'import, voir **en temps réel** comment
le fichier est découpé — pas seulement une fois l'import validé. Le reste
(widgets précis, vocabulaire OriginLab) était négociable.

## Ce qui est en place

| Crate | Ce qui a changé |
|---|---|
| `stokesforge-import` | `ImportRecipe` gagne `header_layout` (`HeaderLayout` : lignes d'en-tête principal/sous-en-tête distinctes, détection auto), `subheader_roles` (`SubheaderRoles` : plages vers `long_name`/`unit`/`comment`, option même-ligne nom/unité), `skip_chars`, `preview_lines` — tous `#[serde(default)]`. `parse()` applique les rôles et garde, par cellule non numérique, le jeton d'origine (`ParsedColumn::texts`). Nouveau module `preview` : `preview()` classe chaque ligne montrée par rôle sans reparser tout le fichier ; `detect_header_layout()` est l'équivalent du bouton de réévaluation automatique d'OriginLab. |
| `stokesforge-app` | `import_window` : contrôles du découpage structuré, assignation des rôles, caractères à retirer, panneau d'aperçu ligne par ligne coloré par rôle (remplace l'ancienne grille de valeurs figée). `AppState::import_line_preview`, `AppState::auto_detect_import_header`. `AppState::import_files` construit une colonne `Mixed` plutôt que `Numeric` dès qu'une cellule porte un texte conservé, et peuple `unit`/`comment` de `ColumnMeta` en plus de `long_name`. `grid_contents` ne consomme plus Retour arrière/Suppr pendant que la fenêtre d'import est ouverte (le bug de champ de texte, voir plus bas). |
| `stokesforge-core`, `stokesforge-commands`, `stokesforge-render` | **rien** — le chantier consomme `ColumnType::Mixed`/`Column::from_mixed` déjà en place depuis le chantier « colonnes à types mixtes », il ne les étend pas. |

## Le bug de champ de texte : ce que c'était vraiment

Il n'était pas dans la fenêtre d'import elle-même. `grid_contents`
(`crates/stokesforge-app/src/app.rs`) consomme Retour arrière/Suppr via
`ui.input_mut(|input| input.consume_key(...))` pour son propre raccourci
« effacer la sélection ». `input_mut` agit sur l'`InputState` du `Context`
**entier**, pas sur celui d'une `Ui` — donc pas seulement sur cette grille.
Ce bloc était déjà gardé par `state.editing().is_none() &&
state.header_editing().is_none()`, mais pas par
`state.import_state().is_none()`.

Or `import_window` (un `egui::Window` flottant) est peinte *après* le dock
dans `StokesForgeApp::update` — donc *après* que `grid_contents` (qui continue de
tourner en dessous, à chaque image, tant qu'un classeur est ouvert) ait déjà
consommé l'événement clavier. Résultat : taper Retour arrière ou Suppr dans
un champ texte de la fenêtre d'import ne faisait plus rien, la touche ayant
déjà été retirée de la file d'événements avant que le `TextEdit` de la
fenêtre d'import ne la voie. Sélectionner du texte puis taper un caractère
de remplacement fonctionnait (c'est un `Event::Text`, jamais consommé ici) —
d'où le symptôme exact rapporté : « impossible d'y supprimer du texte sans
d'abord le sélectionner ».

Correctif : `grid_contents` inclut maintenant `state.import_state().is_none()`
dans la garde. Pas de test automatisé pour ce correctif précis — `AppState`
ne dépend pas d'egui et la saisie clavier réelle n'est testée nulle part
dans ce dépôt (voir la doc de tête de `shell.rs`, critère 13 : « ce qu'elle
ajoute n'est pas testable automatiquement »), donc pas une régression de
couverture propre à ce chantier.

## Décisions sur les questions ouvertes

### `System Parameters`/`User Parameters` d'OriginLab

**Ignorés, pas de champ ajouté.** `ColumnMeta` porte déjà `role`, `unit`,
`long_name`, `comment`, `display_format`, `formula` — rien de plus proche
d'un « paramètre système/utilisateur » libre que `comment`, et le fondre
dedans aurait mélangé deux informations distinctes (un commentaire de
colonne vs. un paramètre d'acquisition de l'appareil) sans bénéfice clair.
Étendre `ColumnMeta` avec deux nouveaux champs aurait touché la persistance
`.stf` pour un besoin que personne n'a encore exprimé côté StokesForge — pas
vérifié dans `docs/persistance.md` en détail parce que ce n'était pas
nécessaire : `ImportRecipe`/`RecipeStore` ne touchent de toute façon jamais
au format `.stf` (voir plus bas), la question ne se posait donc que pour
`ColumnMeta`, tranchée en amont. À rouvrir si un usage réel réclame ces deux
rôles.

### Coût de l'aperçu en direct sur un gros fichier

**Bornage à deux niveaux**, voir la doc de tête de
`crates/stokesforge-import/src/preview.rs` :

- découper le fichier en lignes (`str::split('\n')`) reste `O(taille du
  fichier)` — incompressible, et déjà le cas de l'ancien `import_preview`
  (qui, lui, analysait **toutes** les lignes de données cellule par
  cellule, à chaque image) ;
- construire une ligne possédée et l'analyser cellule à cellule est borné à
  l'en-tête/pied de page (des comptes petits, fixés par l'utilisateur) plus
  `recipe.preview_lines` lignes de données — jamais proportionnel au reste
  du fichier, aussi long soit-il.

Non fait, documenté comme limite restante : le découpage en lignes lui-même
n'est pas paresseux (il matérialise un `Vec<&str>` pour tout le fichier,
sans copie de contenu mais avec une allocation par ligne pour le vecteur de
pointeurs). Sur un fichier de plusieurs millions de lignes, ce `Vec`
resterait le coût dominant de l'aperçu. Pas mesuré, pas traité ici — un
`str::lines().skip(n).take(m)` paresseux serait la suite naturelle si un cas
réel le réclame.

### Détection automatique des lignes d'en-tête

Heuristique en deux passes (voir `detect_header_layout`, tête de
`preview.rs`) : en-tête principal = lignes initiales dont le nombre de
champs diffère de la dernière ligne du fichier (supposée être une ligne de
données) ; sous-en-tête = lignes suivantes qui ont le bon nombre de champs
mais dont au moins un jeton ne s'analyse pas en nombre. Plafonnée aux 500
premières lignes non blanches — un plafond documenté dans le code, pas un
bug, même esprit que les six plafonds de
`docs/archive/rustrewrite/questions-ouvertes.md` §11 (non modifié : ce
plafond est nouveau, propre à ce chantier, il n'avait pas sa place dans une
page qui ne documente que les six déjà tranchés ailleurs).

Limite connue et non corrigée : un fichier sans aucune ligne de données
parmi les 500 examinées fait dégénérer la détection à « tout en-tête
principal, rien en sous-en-tête ». Pas un panic, une détection peu utile —
à corriger à la main comme n'importe quelle détection automatique qui se
trompe, cohérent avec l'esprit du bouton « Réévaluer » d'OriginLab (une
action explicite, jamais appliquée en continu).

`HeaderLayout::auto_detect` (la case à cocher) ne déclenche rien tout seul
— c'est `AppState::auto_detect_import_header`, appelé sur clic du bouton
« Réévaluer maintenant », qui lit le fichier et réécrit
`main_header_lines`/`subheader_lines`. Décision délibérée : recalculer à
chaque image aurait écrasé en continu une valeur que l'utilisateur viendrait
de corriger à la main, l'empêchant de jamais s'en écarter — même défaut que
si OriginLab appliquait sa détection en continu plutôt que sur un bouton.

### Persistance des nouveaux réglages dans `ImportRecipe`

**Non concerné par le format `.stf`** — `ImportRecipe`/`RecipeStore`
persistent dans un fichier JSON séparé
(`ProjectDirs::config_dir()/import_recipes.json`, voir
`crates/stokesforge-import/src/store.rs`), jamais dans le fichier de projet.
`docs/persistance.md` ne les mentionne pas et n'avait pas besoin d'être lu
en détail pour cette raison. La seule compatibilité qui comptait était
JSON→JSON : tous les nouveaux champs portent `#[serde(default)]`
(`header_layout`, `subheader_roles`, `skip_chars`, `preview_lines`), vérifié
par un test dédié
(`recette_json_anterieure_au_chantier_se_charge_avec_les_defauts`,
`crates/stokesforge-import/src/recipe.rs`) qui désérialise un JSON écrit avant ce
chantier et affirme que les nouveaux champs prennent leurs valeurs par
défaut plutôt que de faire échouer le chargement.

## Décisions à ne pas rouvrir sans raison nouvelle

- **Le modèle structuré (`header_layout`) ne remplace `HeaderSkip` que si
  l'utilisateur s'en sert** (`main_header_lines + subheader_lines > 0`).
  Une recette qui n'y touche jamais garde le comportement `HeaderSkip`
  d'avant ce chantier à l'identique — minimiser la casse, même politique que
  le chantier « colonnes à types mixtes ».
- **Une plage de rôle de sous-en-tête n'est lue que sur sa première ligne**
  (`SubheaderRoles::long_name`/`unit`/`comment`, champ `from`). Une plage
  `18` à `18` (l'exemple de référence d'OriginLab) n'en souffre pas ; une
  vraie plage multi-lignes répartie entre colonnes différentes n'est pas
  implémentée — aucun format d'instrument rencontré dans les fixtures de
  test n'en a eu besoin. À étendre si un cas réel l'exige, pas avant.
- **`count_from_bottom` du cahier des charges (« Line number start from
  bottom ») n'a pas été implémenté.** Contrairement aux autres champs, une
  implémentation incorrecte aurait été pire que son absence (compter depuis
  la fin d'un fichier dont l'en-tête est censé être en tête n'a de sens que
  pour un usage qu'OriginLab ne documente pas clairement dans la capture
  fournie). Absent de `HeaderLayout` plutôt que présent et non branché — pas
  un champ mort dans le modèle.
- **Une colonne entièrement numérique reste `Numeric`, jamais promue en
  `Mixed`.** Seule une colonne qui rencontre réellement un jeton non
  numérique devient `Mixed` — vérifié par
  `importing_a_non_numeric_cell_produces_a_mixed_column_with_the_original_text`
  (`crates/stokesforge-app/tests/shell.rs`), qui affirme aussi l'inverse sur la
  colonne numérique voisine dans le même fichier.
- **Le correctif du bug de champ de texte vit dans `grid_contents`, pas dans
  `import_window`.** C'est là que le problème naissait (`consume_key` sur
  l'`InputState` global) — le corriger dans la fenêtre d'import elle-même
  (par exemple en essayant de reconsommer/republier l'événement) aurait été
  un correctif local à un problème structurel, fragile au prochain widget
  flottant qui ouvrirait un champ texte par-dessus la grille.

## Ce qui reste

- L'allocation `Vec<&str>` du découpage en lignes de `preview()` n'est pas
  paresseuse — voir « coût de l'aperçu » ci-dessus. Pas un problème
  rencontré en pratique (pas de fixture de plusieurs millions de lignes
  dans ce dépôt), documenté par prudence.
- Pas de test automatisé pour la disparition du bug de champ de texte
  lui-même (la correction est structurelle et vérifiée par lecture +
  build/clippy, pas par un scénario de frappe simulée — voir la section
  dédiée ci-dessus).
- `System Parameters`/`User Parameters` d'OriginLab restent sans équivalent
  StokesForge, par choix (voir plus haut) — à revisiter si un usage réel les
  réclame.
- Les six plafonds de `docs/archive/rustrewrite/questions-ouvertes.md` §11
  ont été relus avant ce chantier ; aucun n'est concerné (`MAX_COLUMN_ROWS`
  continue de s'appliquer aux colonnes mixtes importées par la même
  fonction générique qu'avant, rien de nouveau à vérifier ici).


## Fin des données (issue #4)

`ImportRecipe::data_end` (`DataEnd`, `#[serde(default)]` = `Auto`) décide où
s'arrêtent les données ; le reste est le pied de page (`LineRole::Footer`,
`ParsedTable::footer_lines`). `Auto` s'arme à la première ligne dont la
première colonne est un nombre, puis coupe à la première dont le X n'en est
pas un (ligne vide comprise) ; un texte dans une autre colonne ne coupe pas.
Autres options : aucune coupure, première ligne vide, marqueur, N lignes après
l'en-tête, N lignes avant la fin. La règle vit dans `data_end.rs`, appelée par
le découpage partagé `layout`, pour que `parse` **et** `preview` coupent au
même endroit. `footer_skip_lines` s'applique d'abord. Les options et le clic
droit sur la ligne de coupure sont disponibles dans l'assistant ; voir
`docs/application.md` § « Assistant d'import ». Le pied peut être conservé
dans les métadonnées avec `keep_footer_metadata`.

## Métadonnées de fichier par colonne (issue #5)

`ImportRecipe::keep_header_metadata` (vrai), `keep_footer_metadata` (faux) et
`excluded_metadata` (clés ou sections écartées) pilotent `ParsedTable::metadata()`
(`crate::metadata`, lignes clé / valeur, sections `[...]`, lignes sans clé
gardées telles quelles). `stokesforge-app` les copie dans `ColumnMeta::file_metadata`
de chaque colonne importée — dans la macro « Importer », sans commande
d'édition : lecture seule. Affichage : ligne « Metadata » de l'en-tête (« 📋 N
clés »), clic → fenêtre modale de consultation (`column_metadata_window`).
La ligne se masque (menu Fenêtre) ; la sélection par clé est dans l'étape
« Métadonnées » de l'assistant ; aucun remplissage automatique des unités
(décidé).

## Assistant en étapes (issue #6)

`import_window` est maintenant un assistant en six étapes (voir
`docs/application.md` § « Assistant d'import »). Côté `stokesforge-import` :
`ImportRecipe::column_roles` (`ColumnRoleChoice`, Ignorer retire la colonne à
l'import), `unify_header`, `detect_encoding`. Le réglage d'en-tête n'expose
plus la case « Détection automatique » : seul le bouton « Détecter depuis le
fichier » agit (`HeaderLayout::auto_detect` reste dans la recette, sans effet).
