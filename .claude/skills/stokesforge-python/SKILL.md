---
name: stokesforge-python
description: Travailler sur le pont Python de StokesForge — interpréteur CPython embarqué (`stokesforge-python`), fil de script et surface `pyst` offerte aux scripts (`stokesforge-script`), console de script de l'application, tableaux numpy, interruption. À utiliser dès qu'il est question d'un script `pyst`, d'une API exposée aux scripts, de PyO3, du GIL, de numpy, de `PYO3_CONFIG_FILE`, ou d'un problème qui n'apparaît qu'avec la feature `python` — les contrôles Cargo par défaut ne le valident pas ; le travail CI test-python l’exerce explicitement.
---

# Le pont Python de StokesForge

Deux crates, derrière la feature `python`, **absente de `default`** :

- **`stokesforge-python`** — l'interpréteur CPython embarqué. Il ne connaît pas le
  modèle de StokesForge : il ne manipule que des scalaires et des tranches de `f64`.
- **`stokesforge-script`** — le fil de script, l'interruption, et la surface `pyst`
  offerte aux scripts. Lui connaît le modèle, et parle à l'acteur.

Lire `docs/python-et-script.md` et `crates/stokesforge-python/README.md` avant de
commencer.

## La feature change tout

Sans `python`, les deux crates compilent **vide**. `cargo build`,
`cargo test --workspace` et `cargo clippy` restent verts sur une machine
sans interpréteur vendorisé — **y compris si le code Python est cassé, ne
compile pas, ou n'existe pas**.

Donc, toute tâche qui touche ce domaine :

```sh
scripts/python-test.sh                # pose PYO3_CONFIG_FILE et le reste
scripts/python-test.sh -- --ignored   # + les reproductions de la frontière
```

Ce script teste `stokesforge-python`, `stokesforge-script`, `stokesforge-robustesse` (en
série), `stokesforge-app` et `stokesforge-cli`. `test-python` l'exécute automatiquement
en CI et bloque en cas d'échec. Les reproductions ignorées restent explicites.
Appliquer le même environnement (`PYO3_CONFIG_FILE`, rpath,
`STOKESFORGE_PYTHON_HOME`) aux vérifications Clippy ; voir
[`.codex/README.md`](../../../.codex/README.md), § « Fonctionnalités optionnelles ».

`scripts/python-config.sh` produit la `PYO3_CONFIG_FILE` ; sans elle, PyO3 ne
compile pas. L'interpréteur est vendorisé dans `vendor/python/` et **n'est pas
versionné** : un dépôt fraîchement cloné n'en a aucun. Si les scripts échouent
faute d'interpréteur, le dire plutôt que de conclure que le changement est bon.

## Invariant 9 — aucun objet Python ne traverse le canal de l'acteur

Trois règles, dont deux sont tenues par le compilateur :

1. le fil de l'interface ne touche **jamais** à Python ;
2. le fil de script **relâche le GIL** pendant qu'il attend l'acteur — chaque
   appel de la surface `pyst` traverse `Python::detach`, et le marqueur `Ungil`
   de PyO3 interdit à la compilation qu'une valeur liée au GIL entre dans le
   bloc d'attente ou en ressorte ;
3. l'acteur ne prend jamais le GIL — `stokesforge-session` ne dépend pas de
   `stokesforge-python`, le type d'un objet Python n'y serait même pas nommable.

**Le chemin caché que ces règles ferment** : une requête ou une réponse qui
transporterait un objet Python serait détruite sur le fil de l'acteur ; or
détruire un objet Python décrémente son compteur de références, ce qui est une
opération Python — donc exige le GIL. Le blocage mutuel serait structurel : le
script tiendrait le GIL en attendant l'acteur, pendant que l'acteur attendrait
le GIL.

D'où la formulation exacte : **ni à l'aller, ni au retour, ni comme champ**.

## L'asymétrie lecture/écriture est une décision, pas un oubli

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (`column`) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Rust → Python (`values`) | tuple des cellules `float` / `str` / `None` | **oui** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

L'écriture copie par choix, pour deux raisons indépendantes : le modèle doit
posséder ses buffers (adopter de la mémoire allouée par Python ferait dépendre
la durée de vie d'une colonne de celle d'un objet Python) ; et un `Vec<f64>`
copié est du Rust pur, donc destructible sur n'importe quel fil — alors que
détruire un objet Python exige le GIL, ce qui interdirait de faire traverser la
valeur au fil qui possède le modèle.

Ne pas « optimiser » cette copie sans avoir relu ces deux raisons.

## Navigation et lecture du projet (#66)

`root()`, `children(handle=None)` et `info(handle)` lisent les instantanés de
la session, sans état de vue : ils fonctionnent aussi sans interface.
`children` rend les enfants directs ordonnés ; `info` rend nom, nature, parent,
nombre de lignes (feuille/colonne) et métadonnées de colonne. La forme exacte
et les valeurs stables de `kind`, `type` et `role` sont dans
`docs/python-et-script.md` ; ne pas reprendre un libellé humain pour un code
stable de l'API.

`values(handle)` copie les cellules stockées en un tuple Python. **Ne jamais
réanalyser les textes**, ni confondre `MixedCell::Empty` (`None`), `Text("")`
(`""`) et `Number(NaN)` (NaN). Le modèle n'a plus de type homogène `Text` :
les colonnes texte sont `Mixed`. `column` conserve sa vue numpy sans copie sur
les colonnes numériques/date-heure et refuse les mixtes ; ne pas matérialiser
une vue numérique mixte en cachant son coût de copie.

Chaque lecture valide l'époque de la poignée **avant** de chercher le nœud.
Le parent et les enfants rendus héritent de l'époque de **ce même instantané**.
Les dictionnaires, listes et tuples Python sont détachés du modèle ; les
modifier ne constitue pas une écriture. Contrat :
`stokesforge-script/tests/navigation.rs` (dont suppression/undo et chargement d'un
projet aux mêmes identifiants).

L'ancienne API v1 de l'archive n'est plus la cible : le chantier #65 reformule
la surface à partir du code. Étendre le pont existant, toujours GIL relâché.

## L'isolation de l'interpréteur

`PYTHONHOME` seul **n'isole rien** : il laisse entrer `PYTHONPATH`, le
`site-packages` utilisateur et les `.pth` du système. L'initialisation passe
donc par `Py_InitializeFromConfig` avec une `PyConfig` issue de
`PyConfig_InitIsolatedConfig`, en fixant `module_search_paths` et le reste
**explicitement**. PyO3 n'offre aucune API sûre pour ça : le passage par
`pyo3::ffi` est obligatoire, et c'est le **seul `unsafe` du crate**.

Deux conséquences assumées : l'initialisation est explicite et paresseuse
(`auto-initialize` n'est pas activée), et `Py_Finalize` n'est **jamais** appelé
— source connue de plantages en sortie, sans bénéfice pour un processus qui se
termine ; les `atexit` Python ne s'exécutent donc pas.

`stokesforge-pyhello` existe pour prouver que cette isolation tient **une fois
empaquetée** (`scripts/pyhello-dist.sh`, environnement nettoyé). Un changement
de configuration d'initialisation se vérifie là, pas seulement sous `cargo test`.

## Écrire dans le modèle depuis un script

Un script passe par les **mêmes commandes annulables** que l'interface :
`Ctrl+Z` annule ce qu'un script vient d'écrire. C'est le critère de fin du
socle, rejoué en entier par `stokesforge-script::run_selftest`.

Le lot #67 ajoute `create_workbook`, `create_worksheet`, `create_column`,
`rename`, `remove`, `set_values` et `set_column_meta` : réutiliser les
commandes, pas un second mutateur. `set_column` garde sa longueur fixe ;
`set_values` remplace longueur et cellules sans changer le type ni la
provenance, par `SetCells` puis `DeleteCells`. Ne pas normaliser NaN en Empty
pour une entrée numpy écrite dans une colonne mixte.

Le moteur donne une identité de client distincte à son contexte. La première
écriture réserve une macro `Script` ; les lectures seules n'en ouvrent pas.
Revalider la poignée après cette acquisition (un Load a pu passer entre les
deux instantanés). Succès = fermer ; erreur non rattrapée ou interruption
demandée = avorter, GIL relâché et exception asynchrone désarmée. Les imports
imbriquent leur macro. `undo()` termine puis annule le groupe courant ; les
écritures ultérieures commencent un autre groupe. Contrat détaillé et bords :
`docs/python-et-script.md`, `stokesforge-script/tests/mutations.rs`.

`pyst.import_file(path, recipe=None)` rend les poignées des feuilles d'un
classeur neuf. Réutiliser `stokesforge-importer` (plans et conversion communs au CLI
et à l'application), puis les commandes dans une macro « Importer ». Aucun
type Python dans le pont hôte : analyse et écriture sont GIL relâché. Un refus
avant création ne modifie rien ; une erreur de commande avorte la macro.

`expected_version` est la condition optimiste que `stokesforge-script` remplit :
elle exige que la version de la colonne n'ait pas bougé **au moment
d'appliquer**, ce qui ferme la fenêtre entre l'instantané qu'un script a lu et
la commande qu'il envoie. Une nouvelle écriture exposée aux scripts doit la
porter aussi.

## Tracé et exécution sans fenêtre

`pyst.plot(y, x=None, *, graph=None, name=None, parent=None, **options)` crée
un `PlotSpec` par `AddGraph`, ou ajoute une série au premier panneau par
`SetPlotSpec`. Réutiliser `stokesforge-plot` : palette, couleurs, axes et styles ;
pas de dépendance de production à un moteur de rendu. Valider les poignées
sur un seul instantané, puis de nouveau après acquisition de la macro.
`add_shape` et `remove_shape` (#76) passent eux aussi par le contexte :
valider le spec avant réservation puis le relire sous macro, même si l'objet
est la première mutation. Aucun mutateur de graphe ne doit contourner la macro
`Script`, sinon une exception laisserait son objet derrière elle.
Conserver les autres panneaux et les options non fournies. Les titres et
labels explicites sont du markup Typst ; les labels automatiques restent
littéraux. Contrat et SVG témoins : `tests/graphs.rs`.

`stokesforge-cli run` utilise le même moteur et la même session, jamais une API
Python parallèle. Il ne sauve `--out` qu'au succès ; erreurs et sorties du
script sont préservées. Vérifier le paquet Linux par `scripts/cli-dist.sh`.
Le spike Matplotlib est un exemple auto-vérifié, lancé uniquement par
`scripts/python-matplotlib-spike.sh`, dans une copie isolée du runtime.
Il ne modifie ni la surface native `pyst` ni le paquet habituel.

## Côté application : la console ne fige pas la fenêtre

`ScriptEngine::run` **bloque** jusqu'au terme du script. Trois décisions
vivent dans `stokesforge-app/src/state.rs`, pas dans `app.rs` :

1. **l'interpréteur ne démarre jamais à l'ouverture de la fenêtre** — seul le
   premier `RunScript` le construit. Le temps de démarrage de l'interpréteur
   est l'un des motifs du portage ; le payer à l'ouverture annulerait le
   bénéfice. Verrouillé par `stokesforge-app/tests/script_bridge_lazy.rs` ;
2. **`RunScript` ne bloque jamais l'appelant** : le travail part sur un fil
   détaché et l'appel revient aussitôt ;
3. **`poll_script` est à appeler à chaque image** : il sonde le résultat et
   l'applique dès qu'il est prêt.

Un fil d'interruption minuscule existe pour que `ScriptEngine::interrupt` ne
bloque jamais — lever une exception asynchrone dans le fil de script exige le
GIL, et le prendre depuis l'interface figerait la fenêtre.

Une **exception Python n'est pas une erreur d'application** : sa trace complète
s'affiche, sans marquer le statut en erreur. Et sans la feature, la console
reste visible mais désactivée (`AppState::script_available`) — `ScriptOutput`
est une copie volontairement indépendante de `stokesforge_script::ScriptOutcome`
pour que la surface publique de `state` soit la même avec et sans la feature.
Ajouter un champ à l'une oblige à le refléter dans l'autre.

## Messages de la surface `pyst` : en anglais

Tout texte que voit un script — message des exceptions `pyst` (`UnknownPathError`,
`BrokenReferenceError`, `InvalidValueError`, `ConcurrentWriteError`), erreurs de
numpy (`array.rs`), de l'interpréteur (`PythonError`) et du fil de script
(`ScriptError`) — est de l'**anglais brut**, jamais traduit : il est lu par
l'utilisateur de `pyst`, pas par l'interface (#47). La console de l'application,
elle, met `ScriptError` en phrase par `errors::script::error_message` (feature
`python`) ; la trace d'une exception de script reste celle de Python. Un message
nouveau suit la même rédaction (ce qui s'est passé, puis quoi faire) et un test qui
l'asserte compare l'anglais.

La console peinte (barre de titre repliable, zone de saisie, Run/Stop,
sortie) vit dans `stokesforge-app/src/app/script_console.rs` (#121 :
`script_header`, `script_contents`, testés sans fenêtre avec et sans
interpréteur) ; le panneau qui la porte, `StokesForgeApp::script_panel`, y
vit aussi depuis #122 (repliée au démarrage, dépliée par une exécution). Les textes de la console (titre, Run/Stop,
sorties, trace d'appels) passent par `tr!` (clés `script-*`) ; les messages de la surface `pyst` — ce que
voit un script — restent en anglais, ce ne sont pas des textes d'interface.

## Tests

- `stokesforge-script/tests/selftest.rs` — le critère de fin du socle, en entier ;
- `stokesforge-script/tests/graphs.rs` et `tests/mutations.rs` — tracé natif,
  cellules exactes, commandes, rollback et undo/redo ;
- `stokesforge-cli/tests/run.rs` — exécution, `.stf`, erreurs et absence de feature ;
- `stokesforge-app/tests/script_panel.rs` — parcours numpy → colonnes → graphe ;
- `stokesforge-script/tests/navigation.rs` — navigation, métadonnées et lecture
  exacte des cellules (lot #66), sans mutation ni dépendance à l'interface ;
- `stokesforge-app/tests/script_bridge_lazy.rs` — l'interpréteur ne démarre pas tout
  seul ;
- `stokesforge-robustesse` : `b7` (frontière Rust ↔ Python), `b9` (script sous
  contrainte, qui garde le seul `#[ignore]` restant du banc, motif écrit dans
  l'attribut).

## Checklist

- [ ] `docs/python-et-script.md` lu
- [ ] `scripts/python-test.sh` réellement lancé (un `cargo test` vert ne prouve rien ici)
- [ ] aucun objet Python dans une requête ou une réponse de l'acteur
- [ ] le GIL relâché pendant l'attente de l'acteur (`Python::detach`)
- [ ] écriture exposée aux scripts : `expected_version` portée
- [ ] surface publique de `state` identique avec et sans la feature
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts **en plus** du script Python
