# Application : état, console de script, banc, CLI, tests

Suite du sommaire [`README.md`](README.md). Contenu : ligne de partage état/coquille, règles de conduite, console de script, ce que Qt donnait gratuitement, binaire, tests, `stokesforge-cli`.

### Deux modules, une ligne de partage

| Crate | Contenu | Testable en CI |
|---|---|---|
| `stokesforge-app-state` (`state`…) | **toute la logique**, sans egui, pilotée par un enum d'`Action` | oui (`stokesforge-app-state/tests/shell.rs`) |
| `stokesforge-app` (`app`…) | la glu `eframe` : traduit clavier et souris en actions, peint l'état | non |

La ligne est gardée par le compilateur (#263), qui a remplacé la garde de texte
de #255 (`aucun_module_d_etat_ne_nomme_egui`, retirée) : `stokesforge-app-state`
ne dépend ni d'`egui`, ni d'`eframe`, ni de `winit`, ni de `wgpu`, ni de
`stokesforge-render-egui` ni de `stokesforge-clipboard`
(`stokesforge-app-state/tests/no_ui_dependency.rs`, liste blanche lue par
`stokesforge-arch-guard`). Un type d'egui entre par `stokesforge-app`, jamais par
l'état. Il refuse aussi, de fait, un `crate::app` dans l'état (#128) : l'état
n'a plus de module `app` à nommer. `RenderError` vit dans `stokesforge-render`
(réexportée par `stokesforge-render-egui`) pour que `errors.rs` y reste. Les
modules sont importés un à un (`use stokesforge_app_state::state::Action;`), sans
`pub use` à la racine ; seul `tr!` est exporté à la racine du crate.

Deux conséquences à connaître :

- **Copie de la figure.** L'état ne garde que des octets neutres
  (`state::FigureBytes` : un `ExportFormat` et ses octets, relevés par
  `AppState::take_figure_clipboard`) ; `app/figure_copy.rs` les traduit en types
  MIME (`mime_of`) et met en phrase l'erreur (`clipboard_error_message`, clés
  `error-clipboard-*`) avant de la passer à `AppState::figure_clipboard_failed`.
- **`cfg(test)` ne passe pas la frontière du crate.** Ce que l'état isole sous
  `cfg(test)` — le chemin des préférences (`preferences::default_path`), la langue
  de départ (`i18n::startup_language`), `set_object_manager_open_for_test` — l'est
  aussi sous la feature `test-support` de `stokesforge-app-state`, que la
  dev-dependency de `stokesforge-app` active : sans elle, les tests de l'app
  (qui bâtissent un `AppState` par `new`) liraient et écriraient le vrai
  `preferences.json` (`stokesforge-app/tests/isolation_preferences.rs`). Cette
  feature ne se met jamais dans un `[dependencies]`.

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
boucle de rendu figerait la fenêtre entière, bouton « Stop » compris — le
défaut le plus visible qu'une console de script puisse avoir.

La parade vit dans `state`, pas dans `app`. Trois décisions, prises là et
nulle part ailleurs :

1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
   `AppState::new` ne touche pas à `ScriptEngine` ; seul le premier
   `RunScript` le construit. Le temps de démarrage de l'interpréteur est l'un
   des motifs du portage — le perdre à l'ouverture annulerait le bénéfice.
   Verrouillé par `stokesforge-app-state/tests/script_bridge_lazy.rs`.
2. **`RunScript` ne bloque jamais l'appelant** : le travail part sur un **fil
   détaché** (création paresseuse du moteur comprise) et l'appel revient
   aussitôt.
3. **`poll_script` est à appeler à chaque image** : il sonde le résultat par
   `try_recv` et l'applique dès qu'il est prêt, rafraîchissement de l'instantané
   compris. C'est ce qui rend `dispatch` sûr à appeler depuis la boucle de
   rendu pour **toutes** les actions.

Sans la feature `python`, la console reste visible mais désactivée
(`AppState::script_available`). `ScriptOutput` est une copie **volontairement
indépendante** de `stokesforge_script::ScriptOutcome`, pour que la surface publique
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


### Binaire

`stokesforge-app [chemin]`. Si `chemin` existe, le projet est chargé depuis ce
fichier (et seul ce cas le marque « enregistré » : `AppState::mark_loaded`) ;
sinon un projet **vide** est construit (comme Fichier › Nouveau, sans classeur)
et `chemin` devient la cible d'un futur enregistrement.

**Sans argument (#333)** : la démonstration s'offre **une seule fois**, à la
première ouverture. Elle est écrite dans `demo.stf`, à côté de
`preferences.json` (dossier de configuration), puis ouverte depuis ce fichier,
que l'utilisateur retrouve par « Ouvrir » ; le marqueur `first_run_done` des
préférences est alors posé et les lancements suivants partent d'un projet vide.
Si `demo.stf` ne peut pas s'écrire, la démonstration reste en mémoire, sans
chemin, et le démarrage n'échoue pas. La décision est
`stokesforge_app_state::startup::plan` (pure, testée sans fenêtre) ;
`main.rs` l'exécute. **`STOKESFORGE_DEMO=1`** (développement, captures, pilote
`run-stokesforge`) garde la démonstration en mémoire à chaque lancement, **sans**
chemin (#315 : le titre porte « Untitled project », le premier Ctrl+S ouvre le
sélecteur) et sans poser le marqueur ; un `demo.stf` du dossier courant n'est
jamais pris pour le projet ni écrasé sans contrôle. Le script d'exemple de la
console n'est prérempli qu'avec la démonstration (il lit `[Book1]Sheet1!A`). **File › Save** et le bouton de
la barre sont grisés pendant l'exécution d'un script (`disabled-script-running`).

Les colonnes du projet de démonstration utilisent les noms automatiques
`A`, `B`, `C`… ; le script prérempli lit `[Book1]Sheet1!A`, la première
colonne visible dans la grille. Les noms descriptifs ne sont pas utilisés
comme clés cachées de la démonstration : les titres lisibles sont des noms
longs de colonne.

La démonstration vit dans le code, module `demo/` du binaire (pas de fichier) :
`Book1` + `Graph1` inchangés, un dossier `Classeurs` (`Types` : un exemplaire de
chaque type de colonne et des colonnes à formule ; `Mesures` : les données des
graphes ; une matrice ; la note `Lisez-moi`), un dossier `Analyse` (un classeur
par famille de calcul, dont `Integrate`, `FitPolynome` et `FitGaussienne` avec
leur calcul déjà fait, visible dans le volet Results) et six graphes (`Panneaux`,
`Axes`, `TypesTrace`, `Annotations`, `Groupes`, `Lignes`). Les noms de fenêtre y
sont uniques dans tout le projet. `demo_session` demande un recalcul
complet juste après `Session::spawn` : les colonnes à formule ne portent que
leur formule (invariant 11) et resteraient vides sans lui. Le bruit des mesures
est à graine fixe : la démonstration est la même à chaque lancement.

Le graphe de démonstration suit le même défaut que les créations par le menu,
le tracé de colonnes et `pyst.plot` sans option `title` : aucun titre (#103).
Un titre vide n'est pas dessiné et ne réserve aucune bande au-dessus du tracé.
Le titre reste saisissable dans **Graph › Properties › Graph Properties… › General** ; un titre
présent reste éditable au double-clic sur la figure.

Deux variables d'environnement servent aux tests et aux captures :
`STOKESFORGE_APP_AUTOQUIT_MS` et `STOKESFORGE_APP_SCREENSHOT`. Pour une
interaction (clics, clavier), voir le skill `run-stokesforge`.

### Tests

Dans `stokesforge-app-state/tests/` (sans fenêtre, et sans egui ni wgpu à
compiler) : `shell.rs` (la boucle complète), `script_panel.rs`,
`script_bridge_lazy.rs` (l'interpréteur ne démarre pas tout seul),
`perf_write_under_snapshot.rs`, `analysis.rs`, `analysis_state.rs`,
`project_objects.rs`, `output_names.rs`, `history_text.rs`,
`language_switch.rs` et la garde de frontière `no_ui_dependency.rs`. Dans
`stokesforge-app/tests/` : `libelles.rs` (la garde de traduction et les clés
mortes, voir « Interface multilingue » dans `theme-i18n-systeme.md`), `gardes.rs` (gardes de texte sur les
sources des deux crates), `isolation_preferences.rs`, `wayland_{uri,bridge}.rs`.

## `stokesforge-cli`

Binaire sans fenêtre : `import` construit un projet neuf depuis des fichiers
d'appareil, `recipes` liste les modèles et `run` (feature `python`) exécute
un fichier Python via une session headless. Les mêmes appels `pyst`, commandes
et groupes annulables sont disponibles dans le CLI et la console ; voir
[`python-et-script.md`](../python-et-script.md). Le paquet Linux autonome se
construit et se vérifie par `scripts/cli-dist.sh`.

Les distributions de l'application et du CLI livrent également les licences
de StokesForge et les notices tierces (ressources, patches, CPython et numpy)
par `scripts/licenses-dist.sh`. Installer ses outils avant l'assemblage ;
voir [`licence-et-extensions.md`](../licence-et-extensions.md).

