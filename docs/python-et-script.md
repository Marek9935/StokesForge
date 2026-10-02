# Python et scripts

Deux crates, derrière la feature `python` (absente de `default`) :

- **`plotyx-python`** — l'interpréteur CPython **embarqué**. Il ne connaît pas
  le modèle de PlotYx : il ne manipule que des scalaires et des tranches de
  `f64`.
- **`plotyx-script`** — le fil de script, l'interruption, et la surface `pyx`
  offerte aux scripts. Lui connaît le modèle, et parle à l'acteur.

Sans la feature, les deux compilent vide : `cargo build`, `cargo test` et la CI
restent verts sur une machine sans interpréteur vendorisé. Avec, la compilation
exige `PYO3_CONFIG_FILE`, produite par `scripts/python-config.sh`.

## Ce que ces crates rendent vrai

> Un script Python lit une colonne **sans copie**, en écrit une autre **via le
> bus**, et `Ctrl+Z` annule ce que le script vient d'écrire.

C'est le critère de fin du socle, et `plotyx-script::run_selftest` le rejoue en
entier — depuis une distribution empaquetée pour le binaire `plotyx-selftest`,
sous `cargo test` pour `tests/selftest.rs`. Toute défaillance, y compris le
démarrage de l'interpréteur, devient une étape en échec : jamais un panic.

## L'interpréteur embarqué

`PYTHONHOME` seul **n'isole rien** : il laisse entrer `PYTHONPATH`, le
`site-packages` utilisateur et les `.pth` du système. L'initialisation passe
donc par `Py_InitializeFromConfig` avec une `PyConfig` issue de
`PyConfig_InitIsolatedConfig`, en fixant `isolated`, `use_environment`,
`user_site_directory`, `install_signal_handlers` et surtout
`module_search_paths` **explicitement**. Aucune variable d'environnement, aucun
setter déprécié.

PyO3 n'offre **aucune** API sûre pour une configuration d'initialisation
personnalisée — `Python::initialize()` appelle `Py_InitializeEx(0)`, qui lit
l'environnement. Le passage par `pyo3::ffi` est donc obligatoire, et c'est le
seul `unsafe` du crate.

L'initialisation est **explicite et paresseuse** (`auto-initialize` n'est pas
activée). `Py_Finalize` n'est **jamais** appelé : source connue de plantages en
sortie, sans bénéfice pour un processus qui se termine. Conséquence assumée :
les `atexit` Python ne s'exécutent pas.

`plotyx-pyhello` existe pour prouver que cette isolation tient une fois
empaquetée (`scripts/pyhello-dist.sh`, environnement nettoyé).

## Tableaux : l'asymétrie est une décision

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

L'écriture copie **par choix**, pour deux raisons indépendantes :

- le modèle doit posséder ses buffers — adopter de la mémoire allouée par
  l'allocateur Python ferait dépendre la durée de vie d'une colonne de celle
  d'un objet Python ;
- un `Vec<f64>` copié est du Rust pur : il se détruit sur **n'importe quel
  fil**, alors que détruire un objet Python exige le GIL — ce qui interdirait de
  faire traverser la valeur au fil qui possède le modèle.

## Deux fils, pas un

- le **fil de script** démarre CPython, en devient le fil principal, et exécute
  les scripts l'un après l'autre ;
- un **fil d'interruption** minuscule existe pour que `ScriptEngine::interrupt`
  ne bloque **jamais**. Lever une exception asynchrone dans le fil de script
  exige le GIL ; le prendre depuis le fil qui demande l'arrêt (l'interface)
  pourrait l'y faire attendre. L'interface pose un drapeau et envoie un
  message ; ce petit fil attend le GIL à sa place.

**Deux drapeaux, pas un**, également : `busy` est posé par `run` **avant**
l'envoi du travail (il sert au refus `ScriptError::Busy`) ; `running` est posé
par le fil de script une fois qu'il est **vraiment entré** dans le script. La
distinction garantit qu'une interruption demandée pendant que `is_running()` est
vrai ne peut plus être effacée par la remise à zéro de début d'exécution.

## L'interruption, en deux moitiés

L'interpréteur démarre **sans gestion des signaux** : aucun `KeyboardInterrupt`
ne sera levé tout seul.

1. un **drapeau atomique** vérifié par chaque appel de la surface `pyx` — il
   attrape un script qui boucle en appelant l'API, sans surcoût ;
2. **`PyThreadState_SetAsyncExc`** depuis le fil d'interruption, qui lève un
   `KeyboardInterrupt` au prochain point de contrôle de l'interpréteur — il
   attrape une boucle Python **pure**.

*Écarté* : un hook `sys.settrace`, qui coûterait à chaque ligne exécutée, en
permanence, pour une granularité dont on n'a pas besoin.

**Limite dite honnêtement** : ni l'un ni l'autre n'interrompt un appel **natif**
long. Une opération numpy sur dix millions de points ira à son terme ;
l'exception ne sera levée qu'en revenant dans l'interpréteur. Un script qui
avale les exceptions (`except: pass` en boucle pure) résiste à la seconde
moitié — la première le rattrape dès qu'il touche à `pyx`.

## La surface `pyx`

Ce n'est **pas** l'API v1 destinée aux utilisateurs (spécifiée dans
`docs/archive/rustrewrite/cahier-des-charges-api-script.md`). Les quatre appels du socle
ne sont pas choisis pour leur utilité mais parce qu'**ensemble ils rendent le
critère de fin vérifiable** :

| Appel | Ce qu'il éprouve |
|---|---|
| `resolve` | poignées opaques, grammaire de chemins |
| `column` | vue numpy **sans copie**, `Arc` maintenu vivant |
| `set_column` | mutation via l'acteur et le bus, GIL relâché pendant l'attente |
| `undo` | délégation à la pile d'historique depuis Python |
| `import_file(path, recipe=None)` | import dans un classeur neuf, conversion commune à l'application et au CLI, une macro annulable |
| `add_shape(graph, kind, x1, y1, x2, y2, *, panel, axis, line_color, line_width, fill_color)` / `remove_shape(graph, index, *, panel)` | objets graphiques (issue #76) : lecture du spec puis `SetPlotSpec`, un Ctrl+Z par appel |

`pyx.add_shape(g, "arrow", 0, 0, 5, 5, line_color="#aa3311")` pose l'objet
(`"rectangle"`, `"ellipse"`, `"line"` ou `"arrow"`, pointe en `(x2, y2)`) en
**coordonnées de données** du panneau `panel=(ligne, colonne)`, au-dessus des
autres, et rend son rang. `g` est la poignée d'un graphe (`pyx.resolve("[Graph1]")`).
Genre, axe, couleur ou épaisseur invalides, coordonnée non finie, panneau ou
rang absent, nœud qui n'est pas un graphe : `InvalidValueError`, rien n'est
modifié. Le spec entier est relu puis réécrit (`SetPlotSpec` n'a pas de
version attendue) : un autre client qui éditerait le même graphe dans la même
fraction de seconde verrait sa modification écrasée, comme dans l'interface.

`pyx.import_file("spectre.csv", recipe="Jasco (point-virgule)")` accepte un
chemin texte ou `pathlib.Path` et rend la liste des poignées des feuilles
créées. Sans nom de modèle, il applique le choix automatique, sinon la recette
permissive. Il respecte les réglages du modèle (colonnes ignorées, types,
métadonnées, blocs et répartition en feuilles) et crée toujours un classeur
neuf ; les modes de modification d'une feuille active concernent l'assistant.
Un seul `pyx.undo()` ou Ctrl+Z retire tout l'import. Un modèle inconnu ou un
fichier illisible lève `InvalidValueError` avant toute mutation ; une erreur
pendant la création avorte la macro. L'analyse et les commandes se déroulent
GIL relâché, comme les autres appels au modèle.

Une poignée (`Handle`) est **opaque** : pas de constructeur Python, aucun entier
exposé ; `resolve` et `import_file` en produisent. Elle reste valide après un
renommage, un déplacement, et même après une suppression **suivie d'une annulation** — c'est
le contrat de l'identité, assuré par le modèle (identifiants monotones, jamais
réutilisés). Elle est invalidée par un `Load`, détecté via l'`epoch` de
l'instantané avant même de chercher le `NodeId` dans l'arbre courant
([`session.md`](session.md)).

Quatre exceptions, et seulement quatre : `UnknownPathError`,
`BrokenReferenceError`, `InvalidValueError`, `ConcurrentWriteError`. Les trois
premières viennent telles quelles de la spécification v1, pour ne pas avoir à
les renommer ensuite ; la quatrième distingue l'épuisement des tentatives de
réécriture (cinq refus consécutifs pour cause de concurrence) des autres refus.
Toute erreur de l'hôte — `SessionError`, `CommandError`, `CoreError` — est
**traduite** en l'une d'elles, jamais transportée telle quelle à travers le
canal.

## Le GIL — la partie que le compilateur tient

Les trois règles et leur chemin caché sont dans [`invariants.md`](invariants.md)
(invariant 9). Leur trace dans le code : tout `bridge.rs` s'exécute à
l'intérieur d'un `Python::detach`, GIL relâché, et **rien n'y nomme un type
PyO3** — c'est ce qui permet au marqueur `Ungil` de vérifier à la compilation
qu'aucune valeur liée au GIL ne traverse l'attente de l'acteur.

La résolution de chemin n'est plus réécrite dans le pont : `plotyx-core` expose
`Tree::resolve`, seule implémentation de la grammaire `[Book1]Sheet1!ColA` du
dépôt, et le pont l'appelle sur l'arbre de son instantané.

## Capture des sorties

`sys.stdout` est **global à l'interpréteur**. La recette habituelle — sauver,
remplacer, restaurer — n'est correcte que si un seul acteur s'en sert à la
fois ; deux moteurs de script dans le même processus se voleraient mutuellement
leur restauration.

On pose donc **une seule fois** un objet-relais dans `sys.stdout`/`sys.stderr`,
et c'est lui qui aiguille : vers le tampon du **fil courant** s'il y en a un,
sinon vers la sortie réelle du processus. L'objet est partagé et sans état ; le
seul état vit dans un `thread_local!`. Conséquence voulue : un `print` exécuté
hors script continue d'aller sur la sortie du processus.

## Ce qu'il faut savoir avant de modifier

- Une erreur *du script* n'est pas une erreur du moteur : elle atterrit dans
  `ScriptOutcome::error` avec sa trace Python. `ScriptError` est réservé aux
  pannes de l'**hôte** (démarrage du fil, moteur occupé).
- Lancer les tests Python : `scripts/python-test.sh`, qui pose les trois
  variables d'environnement nécessaires. Côté robustesse, `b7` (frontière
  Rust ↔ Python dans les deux sens) et `b9` (script sous contrainte) sont
  derrière la même feature.

**À lire** : `plotyx-script/src/lib.rs`, `src/engine.rs`, `src/surface.rs`,
`src/bridge.rs`, `plotyx-python/src/interpreter.rs`, `src/array.rs` (têtes).
