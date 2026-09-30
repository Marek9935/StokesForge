---
name: plotyx-python
description: Travailler sur le pont Python de PlotYx — interpréteur CPython embarqué (`plotyx-python`), fil de script et surface `pyx` offerte aux scripts (`plotyx-script`), console de script de l'application, tableaux numpy, interruption. À utiliser dès qu'il est question d'un script `pyx`, d'une API exposée aux scripts, de PyO3, du GIL, de numpy, de `PYO3_CONFIG_FILE`, ou d'un problème qui n'apparaît qu'avec la feature `python` — que `cargo test --workspace` ne compile même pas, ce qui rend ce domaine le seul où un pipeline vert ne prouve strictement rien.
---

# Le pont Python de PlotYx

Deux crates, derrière la feature `python`, **absente de `default`** :

- **`plotyx-python`** — l'interpréteur CPython embarqué. Il ne connaît pas le
  modèle de PlotYx : il ne manipule que des scalaires et des tranches de `f64`.
- **`plotyx-script`** — le fil de script, l'interruption, et la surface `pyx`
  offerte aux scripts. Lui connaît le modèle, et parle à l'acteur.

Lire `docs/python-et-script.md` et `crates/plotyx-python/README.md` avant de
commencer.

## La feature change tout

Sans `python`, les deux crates compilent **vide**. `cargo build`,
`cargo test --workspace`, `cargo clippy` et la CI restent verts sur une machine
sans interpréteur vendorisé — **y compris si le code Python est cassé, ne
compile pas, ou n'existe pas**.

Donc, toute tâche qui touche ce domaine :

```sh
scripts/python-test.sh                # pose PYO3_CONFIG_FILE et le reste
scripts/python-test.sh -- --ignored   # + les reproductions de la frontière
```

Ce script teste `plotyx-python`, `plotyx-script` et `plotyx-robustesse` ; il
ne lance pas les tests propres à `plotyx-app`. Si la console ou son câblage
change, tester aussi ce crate avec `--features python` en reprenant le même
environnement (`PYO3_CONFIG_FILE`, rpath, `PLOTYX_PYTHON_HOME`) ; voir
[`.codex/README.md`](../../../.codex/README.md), § « Fonctionnalités
optionnelles ». Appliquer cet environnement aux vérifications Clippy.

`scripts/python-config.sh` produit la `PYO3_CONFIG_FILE` ; sans elle, PyO3 ne
compile pas. L'interpréteur est vendorisé dans `vendor/python/` et **n'est pas
versionné** : un dépôt fraîchement cloné n'en a aucun. Si les scripts échouent
faute d'interpréteur, le dire plutôt que de conclure que le changement est bon.

## Invariant 9 — aucun objet Python ne traverse le canal de l'acteur

Trois règles, dont deux sont tenues par le compilateur :

1. le fil de l'interface ne touche **jamais** à Python ;
2. le fil de script **relâche le GIL** pendant qu'il attend l'acteur — chaque
   appel de la surface `pyx` traverse `Python::detach`, et le marqueur `Ungil`
   de PyO3 interdit à la compilation qu'une valeur liée au GIL entre dans le
   bloc d'attente ou en ressorte ;
3. l'acteur ne prend jamais le GIL — `plotyx-session` ne dépend pas de
   `plotyx-python`, le type d'un objet Python n'y serait même pas nommable.

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
| Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

L'écriture copie par choix, pour deux raisons indépendantes : le modèle doit
posséder ses buffers (adopter de la mémoire allouée par Python ferait dépendre
la durée de vie d'une colonne de celle d'un objet Python) ; et un `Vec<f64>`
copié est du Rust pur, donc destructible sur n'importe quel fil — alors que
détruire un objet Python exige le GIL, ce qui interdirait de faire traverser la
valeur au fil qui possède le modèle.

Ne pas « optimiser » cette copie sans avoir relu ces deux raisons.

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

`plotyx-pyhello` existe pour prouver que cette isolation tient **une fois
empaquetée** (`scripts/pyhello-dist.sh`, environnement nettoyé). Un changement
de configuration d'initialisation se vérifie là, pas seulement sous `cargo test`.

## Écrire dans le modèle depuis un script

Un script passe par les **mêmes commandes annulables** que l'interface :
`Ctrl+Z` annule ce qu'un script vient d'écrire. C'est le critère de fin du
socle, rejoué en entier par `plotyx-script::run_selftest`.

`expected_version` est la condition optimiste que `plotyx-script` remplit :
elle exige que la version de la colonne n'ait pas bougé **au moment
d'appliquer**, ce qui ferme la fenêtre entre l'instantané qu'un script a lu et
la commande qu'il envoie. Une nouvelle écriture exposée aux scripts doit la
porter aussi.

## Côté application : la console ne fige pas la fenêtre

`ScriptEngine::run` **bloque** jusqu'au terme du script. Trois décisions
vivent dans `plotyx-app/src/state.rs`, pas dans `app.rs` :

1. **l'interpréteur ne démarre jamais à l'ouverture de la fenêtre** — seul le
   premier `RunScript` le construit. Le temps de démarrage de l'interpréteur
   est l'un des motifs du portage ; le payer à l'ouverture annulerait le
   bénéfice. Verrouillé par `plotyx-app/tests/script_bridge_lazy.rs` ;
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
est une copie volontairement indépendante de `plotyx_script::ScriptOutcome`
pour que la surface publique de `state` soit la même avec et sans la feature.
Ajouter un champ à l'une oblige à le refléter dans l'autre.

## Tests

- `plotyx-script/tests/selftest.rs` — le critère de fin du socle, en entier ;
- `plotyx-app/tests/script_bridge_lazy.rs` — l'interpréteur ne démarre pas tout
  seul ;
- `plotyx-robustesse` : `b7` (frontière Rust ↔ Python), `b9` (script sous
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
