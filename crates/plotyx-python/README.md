# plotyx-python — interpréteur CPython embarqué

Ce crate est le **seul** du dépôt autorisé à démarrer CPython et à donner accès
au GIL. Il ne connaît pas le modèle de données de PlotYx : il ne manipule que
des scalaires et des tranches de `f64`.

Cette version livre la tâche **5.1a** : un *hello-world* **empaqueté** qui
prouve qu'une pile Python complète (interpréteur figé + numpy) démarre depuis un
répertoire de distribution autonome, **totalement isolée du Python système**.
C'est le verrou de la phase 5 : l'empaquetage est le risque le plus lourd de
l'architecture, il est levé avant d'écrire le reste du pont.

## Verrou de versions

Une seule source de vérité : [`vendor/python/lock.env`](../../vendor/python/lock.env),
lue par les scripts (bash) **et** par les tests (`include_str!`).

| Élément | Version |
|---|---|
| CPython (python-build-standalone `20260901`, `install_only_stripped`, `x86_64-unknown-linux-gnu` **et** `x86_64-pc-windows-msvc` — § Windows ci-dessous) | `3.12.14` |
| Roue numpy (`cp312`, manylinux x86_64) | `2.5.3` |
| Crate `pyo3` | `0.29.2` |
| Crate `numpy` (rust-numpy) | `0.29.0` |

Ces quatre valeurs sont solidaires (piège n°6) : l'ABI numpy est liée à l'ABI
CPython, rust-numpy est lié à une version précise de PyO3, elle-même compilée
contre cet interpréteur. On ne change jamais l'une sans relancer
`scripts/python-vendor.sh` puis `scripts/python-test.sh`.

## Construire et vérifier

```bash
scripts/python-vendor.sh     # interpréteur + numpy épinglé dans vendor/python/extracted/python
scripts/python-test.sh       # tests d'intégration contre le préfixe vendorisé (embedded.rs + zerocopy.rs)
scripts/python-memcheck.sh   # zerocopy.rs sous valgrind memcheck — no-op explicite si valgrind est absent
scripts/pyhello-dist.sh      # assemble dist/pyhello/ et le VALIDE en environnement nettoyé
```

Sans la feature `python`, rien de tout cela n'est nécessaire :

```bash
cargo build   -p plotyx-python                       # aucune dépendance PyO3 compilée
cargo test    -p plotyx-python
cargo clippy  -p plotyx-python --all-targets -- -D warnings
```

La feature `python` **n'est pas** dans `default` : la CI reste verte sur une
machine sans interpréteur vendorisé.

### Ce que la CI couvre, et ce qu'elle ne couvre pas

Les trois travaux ordinaires de `.gitlab-ci.yml` (`fmt`, `clippy`, `test`)
n'activent **jamais** la feature `python` : ce crate y compile **vide**, et
rien de ce module n'est ni vérifié par clippy ni testé par ces trois travaux.
Un pipeline vert sur ces trois travaux ne dit donc rien du pont Python.

Un quatrième travail, `test-python`, couvre ce pont : il vendorise
l'interpréteur, génère la configuration PyO3 et lance `scripts/python-test.sh`
— exactement la chaîne décrite ci-dessus. Il est **manuel**
(`when: manual`) et **non bloquant** (`allow_failure: true`) : il télécharge
une archive d'une centaine de mégaoctets à froid, et son échec ne doit pas
faire passer au rouge un pipeline qui ne touche pas au pont Python. Voir le
commentaire en tête de `.gitlab-ci.yml` pour le détail.

## Vérifier le socle soi-même

Le critère de fin de **tout** le socle (édition d'une cellule annulée par
`Ctrl+Z` et rétablie par `Ctrl+Y` ; `Ctrl+S` puis `Ctrl+O` à l'identique ; un
script Python qui lit une colonne sans copie, en écrit une autre par le bus,
annulée par `Ctrl+Z`) se rejoue d'une seule commande, **depuis un binaire
empaqueté** :

```bash
scripts/app-dist.sh                    # construit dist/plotyx-app/, y lance plotyx-selftest en environnement piégé, échoue si son code de sortie n'est pas 0
```

Pour relancer seulement l'auto-vérification depuis le paquet déjà construit
(sans reconstruire ni rouvrir la fenêtre) :

```bash
dist/plotyx-app/plotyx-selftest        # rejoue le critère de fin depuis dist/plotyx-app/ ; --quiet pour n'afficher que les étapes en échec et le verdict
```

Et depuis l'arbre de sources, sans empaquetage, sous forme de test `cargo`
ordinaire (la même logique que le binaire ci-dessus, exercée par
`crates/plotyx-script/tests/selftest.rs`) :

```bash
scripts/python-vendor.sh               # une fois : interpréteur + numpy vendorisés (si ce n'est pas déjà fait)
scripts/python-test.sh                 # lance tests/selftest.rs (et tests/script.rs) contre le préfixe vendorisé
```

Toute la logique de la vérification vit dans `plotyx_script::run_selftest`
(`crates/plotyx-script/src/selftest.rs`) : le binaire `plotyx-selftest` et le
test `tests/selftest.rs` en sont deux coquilles minces autour du même code,
ce qui garantit que ce que le paquet prouve et ce que `cargo test` couvre ne
divergent jamais.

## Les sept pièges et leur traitement ici

### 1. PyO3 ne trouve pas `libpython` à la compilation

Le `sysconfig` de python-build-standalone rend `LIBDIR = /install/lib` — le
chemin de la machine de build d'Astral, inexistant ici. La détection automatique
de PyO3 est donc court-circuitée par un `PYO3_CONFIG_FILE` écrit à la main.

* Gabarit versionné : [`pyo3-config.template.txt`](pyo3-config.template.txt).
* Génération : [`scripts/python-config.sh`](../../scripts/python-config.sh) substitue
  le chemin absolu du préfixe (dépendant de la machine, donc non versionné) et
  écrit `target/pyo3-config.txt`, dont il affiche le chemin sur `stdout` :
  `export PYO3_CONFIG_FILE="$(scripts/python-config.sh)"`.
* Le script **retire les lignes de commentaire** du gabarit : `pyo3-build-config`
  ne lit que des paires `clé=valeur`.

### 2. Le binaire ne trouve pas `libpython` à l'exécution

`rpath` **relatif** posé à l'édition de liens, jamais de `LD_LIBRARY_PATH` :

* distribution — `scripts/pyhello-dist.sh` : `RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'`
  (guillemets **simples** : `$ORIGIN` doit arriver littéralement à l'éditeur de liens) ;
* tests et développement — `scripts/python-test.sh` : même mécanisme avec un rpath
  **absolu** vers `vendor/python/extracted/python/lib`, puisque le binaire de test
  n'est pas distribué.

Les flags passent par `RUSTFLAGS` et non par `.cargo/config.toml` : la section
`[target.x86_64-unknown-linux-gnu]` y serait **globale au workspace** et
imposerait ce rpath à tous les autres binaires du dépôt.

Preuve, dans la sortie de `scripts/pyhello-dist.sh` :

```
libpython3.12.so.1.0 => …/dist/pyhello/lib/libpython3.12.so.1.0
```

### 3. Liaison statique de `libpython`

Écartée : les modules d'extension compilés (ceux de numpy, ceux de `lib-dynload`)
attendent une `libpython` **partagée**. On prend la variante
`install_only_stripped` de python-build-standalone, qui fournit une
`libpython3.12.so.1.0` **partagée** (comme `install_only`, dont elle ne diffère
que par l'absence des symboles de débogage natifs), et on l'embarque. Pas de
binaire unique — c'est un choix, pas un accident (`shared=true` dans le
gabarit de config PyO3).

**`install_only` → `install_only_stripped`** : `libpython3.12.so.1.0` non
strippée pèse 209 Mo, l'essentiel des 307 Mo de `dist/pyhello/` d'origine (§
Mesures). La variante strippée la ramène à 31 Mo (mesuré), sans rien changer
au fait qu'elle reste partagée. Ce qui est perdu : le débogage **natif** de
CPython — vérifié ici avec `readelf -S` : les sections `.debug_*` (DWARF,
lignes/types) ont disparu, donc `gdb` ne peut plus faire de correspondance
adresse → ligne source dans `libpython`. Nuance mesurée, plus fine que
« plus aucun nom de fonction » : la table de symboles ELF (`.symtab`/`.dynsym`)
est, elle, **conservée** — `nm` y liste toujours les noms de fonctions C — donc
une pile d'appels native garde ses noms de fonctions, seulement plus leurs
lignes ni leurs types. Ce qui n'est **pas** perdu, vérifié en exécutant un
script Python qui lève une exception sous l'interpréteur strippé :

```
Traceback (most recent call last):
  File "…/preuve_traceback.py", line 7, in <module>
    niveau_un()
  File "…/preuve_traceback.py", line 5, in niveau_un
    niveau_deux()
  File "…/preuve_traceback.py", line 2, in niveau_deux
    raise ValueError("preuve : traceback complet apres stripping de libpython")
ValueError: preuve : traceback complet apres stripping de libpython
```

Fichier, numéro de ligne, nom de fonction et type d'exception sont tous
présents : les tracebacks Python viennent de l'interpréteur (les objets code
CPython portent leur propre `co_filename`/`co_firstlineno`), pas des symboles
natifs ELF — le stripping des binaires ne les touche pas.

### 4. `PYTHONHOME` seul n'isole rien

Il laisse entrer `PYTHONPATH`, le `site-packages` utilisateur
(`~/.local/lib/python3.12/…`) et les `.pth` du système. L'initialisation se fait
donc par `PyConfig` en mode isolé — `src/interpreter.rs`, fonction
`start_interpreter` :

| Champ | Valeur | Effet |
|---|---|---|
| `isolated` | `1` | `sys.flags.isolated == 1` |
| `use_environment` | `0` | aucune variable `PYTHON*` n'est lue |
| `user_site_directory` | `0` | pas de `~/.local/lib/python3.12` |
| `install_signal_handlers` | `0` | CPython ne touche pas aux signaux du processus |
| `parse_argv` | `0` | `argv` du binaire Rust jamais interprété par Python |
| `home` | préfixe embarqué | fixe `sys.prefix` / `sys.exec_prefix` |
| `module_search_paths_set` | `1` | CPython ne calcule plus `sys.path` |
| `module_search_paths` | `lib/python3.12`, `…/lib-dynload`, `…/site-packages` | `sys.path` **exhaustif**, dans cet ordre |

Résultat mesuré dans `dist/pyhello/`, environnement piégé : `sys.path` contient
**exactement 3 entrées**, toutes sous le préfixe embarqué. `site.py` n'ajoute
rien. Le binaire `pyhello` s'auto-contrôle : il sort en erreur si une seule
entrée de `sys.path` sort du préfixe.

### 5. `Py_SetPythonHome`/`Py_SetProgramName` dépréciés (3.11), supprimés (3.13)

Aucun de ces setters n'est utilisé. Le chemin retenu est
`PyConfig_InitIsolatedConfig` → `PyConfig_SetBytesString` (pour `home`) →
`Py_DecodeLocale` + `PyWideStringList_Append` (pour chaque chemin de modules) →
`Py_InitializeFromConfig` → `PyConfig_Clear`, via `pyo3::ffi`
(`src/interpreter.rs`, fonctions `start_interpreter` et `append_search_path`).

**PyO3 0.29 n'offre aucune API sûre pour une configuration d'initialisation
personnalisée** : `Python::initialize()` appelle `Py_InitializeEx(0)`, qui lit
l'environnement. Le passage par `pyo3::ffi` est donc obligatoire — c'est le seul
`unsafe` du crate, confiné à deux fonctions.

Détail non listé dans les pièges mais indispensable : après
`Py_InitializeFromConfig`, le fil appelant **détient le GIL**. `start_interpreter`
le relâche par `PyEval_SaveThread()` — sans quoi tout `Python::attach` depuis un
autre fil se bloquerait. C'est le même geste que `Python::initialize` de PyO3.

### 6. numpy est lié à une version d'interpréteur précise

Traité par le verrou de versions ci-dessus. Deux garde-fous automatiques :

* `scripts/python-vendor.sh` refuse de continuer si l'interpréteur extrait n'est
  pas la version du verrou, et revérifie `numpy.__version__` après installation ;
* `tests/embedded.rs` lit `lock.env` par `include_str!` et compare
  `python_version()` et `numpy_version()` aux valeurs verrouillées — un
  décalage casse les tests, il ne passe pas en silence.

L'installation se fait avec `--no-deps --only-binary=:all:` : jamais de
compilation locale, jamais de dépendance non prévue.

### 7. Pas de `Py_Finalize`

`Py_Finalize` n'est **jamais** appelé, à aucun endroit. C'est une source connue
de plantages en sortie (modules d'extension qui survivent à la finalisation) et
n'apporte rien à un processus qui se termine : le système récupère la mémoire.

**Conséquence assumée : les `atexit` Python ne s'exécutent pas.** Tout ce qui
doit être vidé sur disque à la fermeture doit l'être côté Rust, explicitement.
Aucun script utilisateur ne peut compter sur `atexit` — à documenter dans l'aide
de la console de script (tâche 5.4).

## Disposition de `dist/pyhello/`

```
dist/pyhello/
├── pyhello                                  binaire release (0,7 Mo)
└── lib/
    ├── libpython3.12.so.1.0                 31 Mo (strippée), cible du rpath $ORIGIN/lib
    ├── libpython3.12.so -> libpython3.12.so.1.0
    └── python/                              = préfixe embarqué (97 Mo), $PYTHONHOME logique
        └── lib/
            ├── libtcl9.0.so, …              bibliothèques annexes du préfixe
            └── python3.12/
                ├── …                        bibliothèque standard (+ __pycache__ livrés)
                ├── lib-dynload/             modules d'extension du cœur
                └── site-packages/
                    ├── numpy/               43 Mo
                    └── numpy.libs/          27 Mo (OpenBLAS)
```

`libpython3.12.so.1.0` est la seule pièce affectée par le passage à
`install_only_stripped` (209 Mo → 31 Mo) : la bibliothèque standard, numpy et
`numpy.libs` sont hors du périmètre du *flavor* et gardent leur taille.

`EmbeddedPython::locate()` cherche `<dir de l'exécutable>/lib/python`, ce qui
donne exactement cette disposition. En développement, la variable
`PLOTYX_PYTHON_HOME` court-circuite cette résolution — **uniquement** pour les
tests et le développement, jamais comme mécanisme utilisateur.

Trois choix d'assemblage, tous dans `scripts/pyhello-dist.sh` :

* `bin/` est **exclu** (98 Mo) : l'initialisation par `PyConfig` n'a besoin que
  de `home` et des chemins de modules, jamais d'un exécutable `python3` ;
* `pip` est **retiré** de `site-packages` (12 Mo) : décision d'architecture, pip
  n'est pas exposé à l'utilisateur ; il ne sert qu'au build (`python-vendor.sh`) ;
* `libpython3.12.so.1.0` n'est copiée **qu'une fois**, dans `lib/`, pas dans
  `lib/python/lib/` : la dupliquer ajouterait 209 Mo pour rien, les modules de
  `lib-dynload` la référencent par SONAME et la trouvent déjà chargée ;
* les `__pycache__` livrés par python-build-standalone sont **conservés** : les
  supprimer fait passer l'initialisation de ~50 ms à ~250 ms (recompilation de la
  bibliothèque standard), et une distribution installée en lecture seule ne
  pourrait jamais reconstituer ce cache.

## Mesures

Fedora, x86_64, `dist/pyhello/pyhello` lancé sous
`env -i PATH=/usr/bin:/bin HOME=/tmp PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2`,
binaire `--release`, moyenne de trois exécutions à cache chaud :

| Mesure | Temps |
|---|---|
| (a) initialisation : démarrage CPython + `import numpy` | **≈ 55 ms** (54–55) |
| (b) aller 1 M `f64` : copie Rust → `ndarray` numpy | **≈ 0,7 ms** |
| (c) retour 1 M `f64` : `ndarray` numpy → `Vec<f64>` | **≈ 0,4 ms** |

Taille de `dist/pyhello/` : **128 Mo** (`install_only_stripped`), contre
**307 Mo** avec la distribution `install_only` d'origine — la décision
« passer au *stripped* », prise après 5.1a, a été exécutée : voir § 3
ci-dessus pour ce qu'elle coûte et ce qu'elle ne coûte pas. Ces mesures de
temps ne bougent pas avec le *flavor* : le stripping retire des sections
ELF de débogage, pas du code exécuté.

Lecture : la copie de 8 Mo de `f64` coûte **moins d'une milliseconde par sens**,
soit ~10 Go/s. Le zéro-copie (tâche 5.2) ne se justifiera donc **pas** par le
temps CPU mais par la **mémoire** : chaque aller-retour duplique la colonne.

## Zéro-copie : lecture sans copie, écriture par copie (tâche 5.2)

Le second verrou de la phase : l'aller-retour de tableau entre Rust et numpy,
avec la **lecture sans copie**. Implémenté dans [`src/array.rs`](src/array.rs),
vérifié par les 19 tests de [`tests/zerocopy.rs`](tests/zerocopy.rs).

### L'asymétrie est une décision

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

L'écriture copie **par choix** : le modèle doit posséder ses buffers — adopter
de la mémoire allouée par l'allocateur Python ferait dépendre la durée de vie
d'une colonne de celle d'un objet Python. Seconde raison, indépendante : un
`Vec<f64>` copié est du Rust pur, il se détruit sur n'importe quel fil — alors
que détruire un objet Python exige le GIL, ce qui interdirait de faire
traverser la valeur au fil qui possède le modèle (tâche 5.4).

### Le mécanisme d'emprunt et son contrat de sûreté

[`borrowed_readonly_array`] rend une vue numpy qui pointe **exactement** sur
`data.as_ptr()`, via l'unique `unsafe` du module,
`PyArray::borrow_from_array`. Sa clause de sûreté demande que le buffer emprunté
ne soit ni réalloué ni libéré tant que le `container` posé comme `base` du
tableau est vivant.

Le `container`, c'est [`FloatBufferOwner`] : un `#[pyclass(frozen)]` qui détient
un **clone** de l'`Arc`. C'est ce clone, et lui seul, qui rend l'écriture
concurrente sûre : tant qu'une vue existe, le compteur de références vaut
**≥ 2**, donc toute écriture côté modèle passe par `Arc::make_mut`, qui — avec
un compteur ≥ 2 — **alloue un buffer neuf** et laisse l'ancien intact. Ni
mutation en place, ni réallocation (croissance du `Vec`), ni libération ne
peuvent atteindre le buffer sous la vue. La clause de sûreté est honorée **par
construction, pas par vigilance** : aucune discipline d'appel n'est requise de
qui utilise `borrowed_readonly_array`.

`writeable = False` est un garde-fou, pas la frontière de sûreté : il attrape
la faute d'inattention (`col[0] = 3` dans un script) mais ne résiste pas à un
script déterminé — ce n'est pas son rôle, le modèle de confiance donne déjà les
pleins pouvoirs à l'interpréteur embarqué.

### La politique de la vue périmée

Une vue prise **avant** une écriture reste sur l'**ancien** buffer : le script
continue d'y lire des valeurs périmées, silencieusement. C'est le comportement
**retenu**, pas un défaut :

```python
v = pyx.column(col)       # v regarde directement la mémoire du modèle
print(v[0])               # 1.0
pyx.set_column(col, autres_valeurs)
print(v[0])               # 1.0  ← toujours l'ancienne valeur
print(pyx.column(col)[0]) # 2.0  ← la nouvelle
```

C'est le seul choix sûr : muter le buffer sous les pieds de numpy serait un
plantage, pas une valeur surprenante. Les deux options écartées, pour mémoire :
*invalider la vue* (numpy ne permet pas d'invalider proprement un `ndarray` ; il
faudrait rendre un objet enveloppant, donc perdre le fait que ce soit un vrai
`ndarray`, ce que toute la bibliothèque scientifique attend) ; *vue vivante qui
se met à jour* (impossible sans copie ni verrou — précisément ce que le
zéro-copie interdit).

**Règle simple qui évite tout le problème : ne gardez pas une vue plus
longtemps qu'un calcul ; relisez la colonne après toute écriture.**

Ce choix est délibérément documenté plutôt que corrigé — voir le test
`vue_perimee_reste_sur_l_ancien_buffer` dans `tests/zerocopy.rs`, qui grave la
politique : ancien pointeur inchangé, ancienne valeur relue sans plantage,
nouvel emprunt sur un pointeur différent rendant la valeur neuve.

### Mesures

`measure_borrow_vs_copy(1_000_000)`, binaire de tests `--release`, Fedora
x86_64, plusieurs exécutions consécutives (le poste est partagé, d'où la
variance) :

| Mesure | Temps |
|---|---|
| emprunt de 1 M `f64` (deux allocations Python, aucun octet copié) | **≈ 5–9 µs** |
| copie de 1 M `f64` (`PyArray1::from_slice`) | **≈ 1,8–2,2 ms** |
| rapport observé | **×240 à ×370** |

Le test `emprunt_au_moins_vingt_fois_plus_rapide_que_la_copie` exige un rapport
**≥ ×20** ; la marge observée est un ordre de grandeur au-dessus. L'emprunt est
en temps **constant** (il n'alloue que le `FloatBufferOwner` et l'objet
`ndarray`) là où la copie est **linéaire** en la taille de la colonne — l'écart
ne peut donc que croître avec des colonnes plus grandes que 1 M valeurs.

Ces chiffres, mesurés à l'intérieur du binaire de tests via `cargo test`, sont
plus élevés côté copie que les ≈ 0,8 ms mesurés en 5.1a par `dist/pyhello`
lancé en dehors de tout harnais (§ Mesures ci-dessus) : la méthodologie diffère
(harnais `libtest`, une seule mesure par exécution plutôt qu'une moyenne de
trois lancers de processus). Le rapport, pas la valeur absolue, est ce que ce
verrou vérifie.

### Contrôle instrumenté : exécuté, résultat excellent

[`scripts/python-memcheck.sh`](../../scripts/python-memcheck.sh) lance le
binaire de tests de `tests/zerocopy.rs` sous `valgrind --error-exitcode=1`
(sans `--leak-check` : les allocations « encore joignables » de l'allocateur
CPython, jamais finalisé, ne sont pas ce qu'on cherche — on cherche des lectures
ou écritures **invalides**), avec `PYTHONMALLOC=malloc` posé pour éviter le
torrent de faux positifs que produit l'allocateur de petits objets de CPython
sous memcheck.

`valgrind` est maintenant installé sur la machine de développement. Résultat :

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

==ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)
```

**Zéro erreur, sur les 19 tests.** Le contrat de durée de vie de la vue numpy
sans copie (§ ci-dessus) n'est plus vérifié seulement par des assertions
Rust : `valgrind` confirme, à l'instrumentation binaire, qu'aucune lecture ni
écriture n'atteint une zone mémoire invalide à la frontière Rust/PyO3/numpy.

#### Un faux négatif corrigé : le rapport ×20 n'a pas de sens sous instrumentation

Le test `emprunt_au_moins_vingt_fois_plus_rapide_que_la_copie` a d'abord
**échoué** sous valgrind :

```
emprunt de 1000000 f64 : 1170881 ns
copie   de 1000000 f64 : 10496668 ns
rapport : ×9.0
panicked: l'emprunt doit être au moins 20× plus rapide
```

Hors instrumentation, l'emprunt coûte 5 à 9 µs contre ≈ 2 ms pour la copie
(rapport ×240 à ×370, § Mesures ci-dessus). `valgrind` ralentit tout, mais
**pas uniformément** : l'emprunt, dominé par quelques appels (allocation du
`FloatBufferOwner`, construction du `ndarray`), est proportionnellement bien
plus pénalisé que la copie, dominée par un transfert de mémoire linéaire — le
rapport mesuré sous instrumentation n'a donc aucun rapport avec le rapport
réel.

Le test est maintenant **conscient de son environnement** :
`scripts/python-memcheck.sh` pose `PLOTYX_UNDER_VALGRIND=1` avant de lancer le
binaire sous valgrind ; en la voyant, le test affiche ses deux mesures et
**n'affirme pas le rapport** (avec un message expliquant pourquoi), au lieu
d'échouer sur un faux négatif. L'assertion `borrow_ns > 0` (horloge assez fine)
reste vérifiée dans tous les cas, et l'assertion de rapport `× 20` reste
**active en exécution ordinaire** (`scripts/python-test.sh`, sans la variable) —
c'est elle qui protège le zéro-copie contre une régression silencieuse en
copie. Mesures affichées sous instrumentation, avec `--nocapture` (sans quoi
libtest avale la sortie d'un test qui réussit) :

```
emprunt de 1000000 f64 : 1085997 ns
copie   de 1000000 f64 : 9853264 ns
rapport : ×9.1
PLOTYX_UNDER_VALGRIND posée : rapport non affirmé (valgrind ralentit
l'emprunt et la copie de façon non uniforme, le rapport mesuré ici
n'a pas de sens)
```

`miri` n'est **pas** une alternative à `valgrind` ici : il n'exécute pas
d'appel de bibliothèque étrangère (FFI), donc rien de ce qui traverse PyO3 —
soit tout ce module. Il reste l'outil pertinent pour le reste du modèle
PlotYx, qui est du Rust pur sans FFI.

## Volontairement hors de cette tâche

* **Cycle de vie et propriété des objets Python** (`Py<T>` conservés entre deux
  prises du GIL, libération) — tâche **5.3**.
* **Fil de script dédié** (console utilisateur, interruption, `detach` pendant
  les calculs longs) — tâche **5.4**. Ici tout tourne sur le fil appelant.

## Windows (tâche 5.1b)

**Faite.** Le critère de fin du socle (§1.1) rejoue vert sur Windows :
`scripts/python-test.sh` (lui-même OS-aware) lance les mêmes suites
(`embedded.rs`, `zerocopy.rs`, `plotyx-script`, `plotyx-robustesse` b7/b8/b9)
contre un CPython 3.12.14 `python-build-standalone` vendorisé pour
`x86_64-pc-windows-msvc` — même verrou de versions que Linux
(`vendor/python/lock.env`, `PBS_TRIPLE_WINDOWS`), aucune n'a bougé.

Ce qui différait vraiment de Linux, et comment c'est traité :

* **Disposition du préfixe.** `python-build-standalone` livre un layout
  différent par OS : `<home>/lib/python3.12` + `lib-dynload/` sur Unix,
  `<home>/Lib` + `<home>/DLLs` sur Windows (pas de `lib-dynload` dans cette
  disposition). `EmbeddedPython::stdlib_dir`/`search_paths`
  (`src/interpreter.rs`) branchent sur `cfg(windows)` — la seule différence
  entre les deux chemins de code, le reste de `start_interpreter` (PyConfig
  isolée, `module_search_paths`) est identique sur les deux OS.
* **Configuration PyO3 (piège n°1).** Gabarit dédié
  [`pyo3-config.windows.template.txt`](pyo3-config.windows.template.txt) :
  `lib_name=python312` (nom du `.lib` d'import, pas `python3.12`) et
  `lib_dir=<prefix>/libs` (pas `<prefix>/lib`). `scripts/python-config.sh`
  choisit le bon gabarit sur `$OS`.
* **`libpython` à l'exécution (piège n°2).** Pas de rpath sous Windows (le
  chargeur ne connaît pas la notion, et `-Wl,-rpath` ferait échouer
  `link.exe`) : `scripts/python-test.sh` préfixe `PATH` avec le préfixe
  embarqué à la place — `python312.dll` y vit directement, à la racine.
* **`Py_DecodeLocale`/`wchar_t` 16 bits.** Aucun contournement nécessaire :
  ce sont des détails internes de la C-API que `pyo3::ffi` expose déjà de
  façon portable ; `start_interpreter`/`append_search_path` n'ont pas changé
  d'une ligne entre les deux OS.
* **`link.exe` de Git for Windows.** Piège d'environnement, pas de code :
  `Git\usr\bin\link.exe` (un outil POSIX homonyme, sans rapport avec MSVC)
  peut passer avant celui de Visual Studio sur le `PATH` — voir la section
  Windows du `CLAUDE.md` racine pour l'ordre à respecter, et pourquoi
  compiler directement en PowerShell plutôt que via `bash -lc`.

Un seul test du dépôt reposait sur une hypothèse Unix en dehors du pont
Python lui-même : `tests/embedded.rs::sys_path_ne_contient_que_le_prefixe_embarque`
comparait `sys.path` à `Path::canonicalize()`, qui sur Windows rend un chemin
**verbatim** (`\\?\C:\...`) que `sys.path` ne porte jamais — corrigé en
retirant ce préfixe avant comparaison, sans rapport avec le pont Rust/Python.
