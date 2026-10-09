# Python et scripts

`pyst` est l'interface d'extension publique couverte par la permission
supplémentaire de `LICENSE-EXCEPTION`. Un script qui l'utilise sans
incorporer ni lier le code de StokesForge dans son propre programme peut
être distribué sous la licence choisie par son auteur. La liste fermée des
interfaces et la portée de cette permission sont dans
[`licence-et-extensions.md`](licence-et-extensions.md).

Deux crates, derrière la feature `python` (absente de `default`) :

- **`stokesforge-python`** — l'interpréteur CPython **embarqué**. Il ne connaît pas
  le modèle de StokesForge : il ne manipule que des scalaires et des tranches de
  `f64`.
- **`stokesforge-script`** — le fil de script, l'interruption, et la surface `pyst`
  offerte aux scripts. Lui connaît le modèle, et parle à l'acteur.

Sans la feature, les deux compilent vide : `cargo build` et `cargo test`
fonctionnent sans interpréteur vendorisé. Le travail CI `test-python` prépare
son runtime et exerce ce chemin automatiquement. Avec, la compilation
exige `PYO3_CONFIG_FILE`, produite par `scripts/python-config.sh`.

## Ce que ces crates rendent vrai

> Un script Python lit une colonne **sans copie**, en écrit une autre **via le
> bus**, et `Ctrl+Z` annule ce que le script vient d'écrire.

C'est le critère de fin du socle, et `stokesforge-script::run_selftest` le rejoue en
entier — depuis une distribution empaquetée pour le binaire `stokesforge-selftest`,
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

`stokesforge-pyhello` existe pour prouver que cette isolation tient une fois
empaquetée (`scripts/pyhello-dist.sh`, environnement nettoyé).

### `pyst.analysis` : lots et ajustement global (#201)

`y` peut être une liste de poignées : un seul calcul multi-séries, comme dans la
fenêtre d'analyse (un Ctrl+Z, une ligne de Results). `x` et `sigma` valent pour
toutes les séries (une poignée) ou une par série (liste de même longueur ;
sinon `InvalidValueError`). Le résultat d'un lot est
`{"series": [{columns, values, errors, warnings}, …]}`, dans l'ordre de `y` ;
une poignée seule garde le dictionnaire à plat. `fit(shared=['tau'])` ajuste
globalement. Témoins : `un_lot_est_un_seul_calcul_multi_series_annulable_d_un_coup`,
`un_lot_mal_forme_est_refuse_avec_un_message`.

### `pyst.analysis.fit` : modèles (#204)

`model=` accepte `linear`, `polynomial` (`degree`), `exponential`
(`A·exp(−x/τ)`), `exponential_offset` (avec `y0`), `exponential2` (deux
composantes et `y0`), `logarithm`, `power`, `gaussian`, `lorentzian`, ou une
formule. `pyst.analysis.peaks(center_window=…)` borne chaque centre à ± cette
distance de sa position de départ. Témoin :
`les_trois_exponentielles_et_la_fenetre_de_centre_ont_leur_nom`.

### scipy dans le préfixe embarqué (#191)

La pile embarquée porte numpy **et scipy** (`SCIPY_VERSION` de
`vendor/python/lock.env`, roue cp312 manylinux, installée par
`scripts/python-vendor.sh` avec `--no-deps --only-binary=:all:` comme numpy).
Les deux sont solidaires de CPython et entre eux (la roue scipy exige
`numpy>=2.0,<2.8`) : changer l'un se rejoue par `scripts/python-vendor.sh` puis
`scripts/python-test.sh`. scipy sert aux **scripts** `pyst` (par exemple
`scipy.optimize.curve_fit` pour un modèle que `pyst.analysis.fit` ne propose
pas) ; l'interface et les menus n'en dépendent jamais, tout le traitement des
menus est du Rust pur (`stokesforge-analysis`). Poids : ≈ 140 Mo installés
(`scipy` 109 Mo + `scipy.libs` 30 Mo, OpenBLAS, libgfortran et libquadmath
inclus), à compter dans le paquet. Les notices vont dans `licenses/scipy/`
(`scripts/licenses-dist.sh`). Témoin : `stokesforge-script/tests/analysis.rs`,
`scipy_ajuste_une_exponentielle_depuis_un_script` (travail CI `test-python`).
Windows : la pile n'est pas vendorisée aujourd'hui (tâche 5.1b) ; la roue
`win_amd64` s'y installera de la même façon, à vérifier alors.

## Tableaux : l'asymétrie est une décision

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (`column`) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Rust → Python (`values`) | tuple de cellules `float` / `str` / `None` | **oui** |
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

1. un **drapeau atomique** vérifié par chaque appel de la surface `pyst` — il
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
moitié — la première le rattrape dès qu'il touche à `pyst`.

## La surface `pyst`

Le chantier Python (#65) reformule l'API à partir du code ; la spécification
v1 de l'archive est historique. Les lots #66 à #68 ajoutent navigation,
lecture des cellules, manipulation du projet et graphes natifs par commandes
annulables. Aucun
appel ne dépend d'une feuille active ni d'une fenêtre : la surface s'emploie
avec toute session, y compris une session sans interface.

| Appel | Résultat / effet |
|---|---|
| `root()` | poignée du dossier racine du projet courant |
| `children(handle=None)` | liste des poignées des enfants directs, dans l'ordre du modèle ; sans poignée, enfants de la racine |
| `info(handle)` | dictionnaire détaché : nom, nature, parent, nombre de lignes et métadonnées de colonne |
| `values(handle)` | tuple copié des cellules stockées, en `float`, `str` ou `None` |
| `resolve(chemin)` | poignée opaque résolue par la grammaire de chemins du modèle |
| `column(poignee)` | tableau numpy numérique/date-heure, sans copie et en lecture seule |
| `set_column(poignee, tab)` | écriture numérique de même longueur par les commandes, annulable |
| `create_workbook(name=None, parent=None)` | classeur vide dans le dossier demandé, racine par défaut ; poignée |
| `create_worksheet(workbook, name=None)` | feuille vide en fin de classeur ; poignée |
| `create_column(worksheet, name=None, column_type="numeric")` | colonne vide en fin de feuille ; poignée ; types `numeric`, `datetime`, `mixed` |
| `rename(handle, name)` | renomme le nœud en conservant sa poignée |
| `remove(handle)` | supprime le nœud et ses descendants, sauf la racine |
| `set_values(handle, values)` | remplace les valeurs et la longueur de cette seule colonne |
| `set_column_meta(handle, **metadata)` | modifie les seuls champs nommés, en préservant la provenance |
| `plot(y, x=None, *, graph=None, name=None, parent=None, **options)` | crée un graphe natif ou ajoute une série à un graphe ; poignée |
| `add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis="left", line_color="", line_width=1.5, fill_color="")` | ajoute un objet graphique en coordonnées de données ; rang dans le panneau |
| `remove_shape(graph, index, *, panel=(0, 0))` | retire l'objet graphique de ce rang |
| `undo()` | termine puis annule le groupe courant, ou annule le dernier groupe déjà terminé ; `False` si l'historique est vide |
| `import_file(path, recipe=None)` | import dans un classeur neuf, une macro annulable ; poignées des feuilles créées |

### `pyst.analysis` (#181)

Sous-module des calculs du menu Analysis : `baseline`, `correct`, `integrate`,
`fit`, `peaks`, `reconvolve`, `deconvolve`. Chaque appel décrit une opération
sur **une** colonne Y (poignée, X facultatif) et l'enregistre comme un calcul du
projet par `Command::AddCalculation`, dans le groupe d'écritures du script : un
seul `pyst.undo()` l'annule avec ses colonnes. Même `compute` que l'interface,
même recalcul automatique (`auto_recalc`) et même option `keep_results`.
Retour : un dictionnaire `{columns, values, errors, warnings}` aux clés stables
du crate de calcul (`"area.total"`, `"param.tau"`, `"peak.height.1"`…) ;
les avertissements sont en anglais. Les noms longs des colonnes de sortie sont
anglais aussi. **Limites** : une série par appel (ni lot, ni ajustement global).
Contrat : `stokesforge-script/tests/analysis.rs`. La dépendance de `script` à
`stokesforge-analysis` est déclarée dans sa liste blanche (`no_ui_dependency.rs`).

### Écritures et annulation d'un script (#67)

Les écritures passent par les commandes déjà utilisées par l'application.
La première mutation ouvre une macro exclusive `MacroKind::Script` ; les
mutations suivantes, imports compris, rejoignent ce groupe. Au succès, un
seul Ctrl+Z annule toute l'exécution (libellé « Exécuter le script »). Une
exception non rattrapée ou une interruption demandée annule les mutations du
groupe en cours et préserve l'historique précédent, y compris le rétablissement.
Une erreur rattrapée permet au script de poursuivre et de valider ses autres
écritures.

Un script de lecture n'ouvre aucune macro. Dès la première mutation, les
autres clients ne peuvent plus écrire, sauvegarder ou charger pendant le
groupe ; les lectures restent possibles. Le moteur possède toujours une
identité de client distincte de son appelant, même si celui-ci lui transmet
un clone de sa poignée. Les mutations revalident l'époque après acquisition
de la macro ; les écritures de cellules portent encore `expected_version`.

`pyst.undo()` reste utilisable pendant un script : il termine le groupe en
cours puis l'annule entièrement. C'est un point de validation explicite ;
une écriture ultérieure ouvre un nouveau groupe, et un échec ultérieur
n'annule pas les opérations d'historique déjà demandées. Deux écritures
successives avant `undo()` s'annulent donc ensemble.

`set_column` conserve son contrat numpy strict : float64, une dimension,
longueur inchangée, tableaux non contigus acceptés et masque non vide refusé.
`set_values` remplace aussi la longueur, par composition de `SetCells` puis
`DeleteCells` pour retirer la queue ; l'identité, le type, les métadonnées et
les autres colonnes restent inchangés. Aucune suppression de queue ne
précède la validation de l'écriture.

`set_values` accepte un tableau numpy float64 1D (mêmes contrôles) ou un
itérable de `float`, `int`, `str`, `None`. Les entiers Python hors de
[-2⁵³, 2⁵³] sont refusés : convertir explicitement si la perte de précision
est voulue. Une colonne numérique/date-heure refuse les textes et traduit
`None` en NaN ; une colonne mixte conserve chaque cellule telle quelle
(`None`, `""`, `"12"`, NaN et -0.0 sont distincts). Les entrées sont copiées.
Les dates sont des timestamps POSIX en secondes.

Les noms automatiques viennent du modèle (`BookN`, `SheetN`, lettres de
colonne). La longueur des noms (255 caractères), les caractères réservés,
l'unicité et les types de parent gardent les règles du modèle. Le plafond
de 100 000 000 lignes est partagé avec le modèle et vérifié avant la copie
d'un tableau numpy ; un itérable est également borné pendant sa copie.
Les créations rendent des objets vides ; pour des données texte, créer une
colonne `mixed`, puis appeler `set_values`.

`set_column_meta` accepte `role`, `long_name`, `unit`, `comment`,
`display_format` et `formula`. Les valeurs sont des chaînes, sauf
`formula=None` qui retire la formule ; les rôles sont ceux de `info`.
`type` et `version` restent en lecture seule. Une colonne-formule refuse
les écritures directes. Poser une formule déclenche le recalcul existant,
qui peut changer le type de la colonne pour porter les échecs de calcul ;
retirer la formule ne reconvertit pas ce type automatiquement.

```python
import numpy as np

x = pyst.column(pyst.resolve("[Book1]Sheet1!A"))
book = pyst.create_workbook("Résultats")
sheet = pyst.create_worksheet(book, "Calcul")
result = pyst.create_column(sheet, "Signal")
pyst.set_values(result, np.asarray(x * 2, dtype=np.float64))
pyst.set_column_meta(result, role="y", long_name="Signal doublé", unit="mV")
# Un seul Ctrl+Z retire ce classeur et toutes ses données.
```

Contrat : `crates/stokesforge-script/tests/mutations.rs` (création, longueurs,
cellules exactes, undo/redo, refus, époques, interruption et imports).

### Graphes natifs (#68)

`pyst.plot(y, x)` référence les colonnes du modèle et crée un graphe à la
racine ; sans `x`, l'abscisse est l'indice de ligne. `parent` accepte un
dossier, `name` choisit le nom (automatique `GraphN` sinon). `graph=g` ajoute
une série au premier panneau d'un graphe existant et conserve ses autres
panneaux, séries et réglages ; `name` et `parent` sont alors refusés. Un appel
ajoute une série, ou plusieurs si `y` est une liste de colonnes : ces séries forment un groupe (§ Groupes de séries, #196).
Sans option `title`, un graphe neuf n'a pas de titre ; le nom du graphe
sert à identifier sa fenêtre, pas à titrer la figure (#103).

| Option | Valeurs |
|---|---|
| `plot_type` | `line`, `scatter`, `line_scatter`, `bar`, `histogram`, `error_bar` |
| `color` | `#RRGGBB` ou `#RRGGBBAA`, trait et symbole ; palette partagée sinon |
| `line_style`, `line_width` | `none`, `solid`, `dash`, `dot`, `dash_dot` ; largeur finie positive |
| `symbol`, `symbol_size` | `none`, `circle`, `square`, `triangle`, `diamond`, `plus`, `cross` ; taille finie positive |
| `label`, `title`, `x_title`, `y_title` | chaînes avec markup Typst explicite |
| `x_scale`, `y_scale` | `linear` ou `log` |
| `x_range`, `y_range` | couple `(min, max)` fini croissant, positif sur un axe log ; plage automatique sinon sur un graphe neuf |
| `axis` | `left` ou `right` pour la série et ses options Y ; rend l'axe droit visible |
| `x_error`, `y_error` | poignées des colonnes d'erreurs |
| `histogram_bins` | entier de 1 à 65 536 |

Le label automatique vient de la colonne et reste du texte littéral : une
métadonnée contenant du markup n'est pas interprétée. Les titres et labels
fournis explicitement utilisent Typst, comme les propriétés de l'application.
Une option inconnue, une plage invalide ou une cible incorrecte est refusée.
Les poignées et les paramètres sont validés avant la mutation ; les références
cassées sont détectées avec leur époque. Les commandes `AddGraph` et
`SetPlotSpec` rejoignent le même groupe `Script` que les écritures de colonnes.

```python
x = pyst.resolve('[Book1]Sheet1!A')
y = pyst.resolve('[Book1]Sheet1!B')
g = pyst.plot(y, x, name='Graph1', title='$x^2$',
             plot_type='line_scatter', color='#123456', x_title='Temps (s)')
pyst.plot(y, x, graph=g, line_style='dash', label='Deuxième série')
```

Contrat : `tests/graphs.rs` compare le `PlotSpec` exact et le SVG partagé,
avec un témoin différent et du Typst ; il couvre aussi ajout, rollback,
persistance et références invalidées. Matplotlib est évalué séparément par
le [spike #69](python-matplotlib-spike.md), sans changer le rendu natif.

`pyst.add_shape(g, "arrow", 0, 0, 5, 5, line_color="#aa3311")` pose un objet
(`rectangle`, `ellipse`, `line` ou `arrow`, pointe en `(x2, y2)`) en coordonnées
de données du calque de base de la case `panel=(ligne, colonne)` (le calque ordinaire, jamais un calque superposé, #167), au-dessus des autres, et rend
son rang. `axis` vaut `left` ou `right`. La couleur du trait vide est
automatique, celle du remplissage vide signifie aucun remplissage.
`remove_shape` retire ce rang ; les objets suivants descendent d'un rang.

Ces appels de #76 rejoignent la même macro `Script`, même s'ils sont la
première mutation de l'exécution. Une erreur ou interruption annule donc aussi
les ajouts et retraits d'objets. Nature, époque et panneau sont revalidés après
réservation ; aucun client concurrent ne peut écraser le spec pendant cette
écriture. Des coordonnées non finies, un type, axe, style ou rang invalide
sont refusés sans mutation. Contrat : `tests/script.rs` et `tests/graphs.rs`.

### Groupes de séries (#196)

`pyst.plot([y1, y2, …], x)` crée **un groupe**, comme les séries ajoutées
ensemble depuis l'interface (`PanelSpec::push_series_batch`) ; une liste d'une
seule colonne n'en crée pas. `x` est `None` (indice de ligne), une colonne
partagée ou une liste de même longueur que `y`. `label` est une chaîne pour
une série, une liste (un nom par série) pour un lot ; `group_name` nomme le
groupe (refusé pour une série seule) ; `x_error`/`y_error` ne servent qu'à une
série seule. Les couleurs automatiques suivent la palette, série après série.

| Appel | Effet |
|---|---|
| `series(g, *, panel=(0, 0))` | liste de dictionnaires `index`, `name`, `group` (`None` si seule), `color`, `visible`, `y` (poignée) |
| `series_groups(g, *, panel)` | liste de `id`, `name` (`None` = nom automatique), `members` (rangs) |
| `group_series(g, indices, *, name=None, panel)` | groupe neuf à la place de la première série ; rend son numéro. Les séries quittent leur ancien groupe, qui disparaît avec son nom s'il est vide |
| `set_series_group(g, index, group, *, panel)` | entre à la suite des membres, ou sort du groupe avec `None` ; rend le nouveau rang |
| `rename_group(g, group, name, *, panel)` | `""` rend le nom automatique |
| `ungroup(g, group, *, panel)` | dissout : les séries restent, style gardé |
| `remove_group(g, group, *, panel)` | supprime le groupe **et ses séries** |
| `set_group_style(g, group, *, panel, line_style, line_width, symbol, symbol_size)` | pose chaque option donnée sur tous les membres |
| `apply_gradient(g, group, gradient, *, panel)` | `gradient` : identifiant d'un nuancier fourni (`blue-red`…), nom d'un nuancier enregistré **dans le projet**, ou liste d'arrêts `#RRGGBB` |

Un numéro de groupe est celui du spec : stable tant que le groupe existe. Les
gestes rejouent ceux de l'interface (`PanelSpec::group_series`,
`set_series_group`, `dissolve_group`, `remove_group`, `apply_group_gradient`) ;
les groupes restent contigus. Les nuanciers enregistrés au niveau de
l'utilisateur (hors projet) ne sont pas visibles d'un script, qui reste
reproductible. Toutes ces écritures sont des `SetPlotSpec` du groupe `Script`
(motif d'`add_shape` : validation avant et après réservation). Contrat :
`tests/graphs.rs` (groupes en un appel, lecture, gestes et annulation d'un seul
`undo`, refus sans mutation, poignées périmées).

### CLI sans fenêtre (#71)

```sh
stokesforge-cli run --out resultat.stf calcul.pyst
stokesforge-cli run --project resultat.stf --out suite.stf suite.pyst
```

La feature `python` est requise ; sans elle, `run` explique comment l'activer.
Sans `--project`, le script reçoit un projet vide ; sans `--out`, il n'y a
pas de sauvegarde finale. Le fichier Python est lu en UTF-8. stdout et stderr
sont préservées ; une exception affiche sa trace complète et rend un code
non nul. `--out` n'est écrit qu'après succès. Les scripts passent par le
même `ScriptEngine`, l'acteur et les commandes que la console.

`scripts/cli-dist.sh` construit le paquet Linux `dist/stokesforge-cli/`, avec le
runtime de base (sans Matplotlib), un rpath relatif et aucun pip. Il vérifie
numpy, écritures, graphe, sauvegarde/rechargement et undo depuis ce paquet,
sans écran ni `STOKESFORGE_PYTHON_HOME`, avec `PYTHONHOME`/`PYTHONPATH` piégés.

### Navigation et métadonnées

`children` suit les dossiers, classeurs puis feuilles ; une colonne, un graphe,
une note ou une matrice rend une liste vide. Une liste rendue ne se met pas à
jour toute seule. Chaque appel lit un seul instantané du projet, et toute
poignée rendue conserve l'époque de cet instantané.

`info` rend toujours les cinq clés suivantes :

| Clé | Valeur |
|---|---|
| `name` | nom court du nœud, ou nom du projet pour sa racine |
| `kind` | `folder`, `workbook`, `worksheet`, `column`, `graph`, `matrix`, `note` ou `unknown` |
| `parent` | poignée du parent ; `None` à la racine |
| `row_count` | longueur d'une colonne ; maximum des longueurs pour une feuille (zéro si vide) ; `None` ailleurs |
| `column` | dictionnaire ci-dessous pour une colonne ; `None` ailleurs |

Les métadonnées `column` portent `type` (`numeric`, `datetime`, `mixed`),
`role` (`x`, `y`, `x2`, `y2`, `z`, `x_error`, `y_error`, `label`, `none`), `long_name`,
`unit`, `comment`, `x_prime` (nombre ou `None`, #164, lecture seule), `display_format`,
`formula` (texte ou `None`) et `version`
(compteur de modification). Modifier ce dictionnaire ne touche jamais au
modèle. Relire `info` après une modification pour obtenir l'état courant.

```python
for objet in pyst.children():
    print(pyst.info(objet)["name"], pyst.info(objet)["kind"])

feuille = pyst.resolve("[Book1]Sheet1")
for colonne in pyst.children(feuille):
    description = pyst.info(colonne)
    print(description["name"], description["column"]["unit"],
          description["row_count"])
```

### Cellules texte et mixtes

Le modèle actuel ne porte plus de type homogène `Text` : une colonne texte
est une colonne `mixed` contenant des cellules texte. `values` préserve le
type stocké de chaque cellule : un texte `"12.5"` reste une chaîne, une cellule
vide devient `None`, une chaîne vide reste `""`, et un nombre NaN reste un
`float` NaN. Les dates restent des timestamps POSIX en secondes.

```python
cellules = pyst.values(pyst.resolve("[Book1]Sheet1!Etiquette"))
# Exemple : ("échantillon A", None, "", "12.5")

import numpy as np
numerique = pyst.column(pyst.resolve("[Book1]Sheet1!A"))  # sans copie
mixte = np.array(cellules, dtype=object)                # copie explicite
```

`values` copie en temps et mémoire proportionnels au nombre de cellules,
y compris pour une colonne numérique ; son tuple est un instantané immuable.
`column` conserve son contrat numérique sans copie et refuse une colonne
mixte avec `InvalidValueError`. Les lectures ne changent ni la révision ni
l'historique. Une cible autre qu'une colonne fait échouer `values` avec
`InvalidValueError` ; toute poignée supprimée ou d'un projet rechargé est
refusée avec `BrokenReferenceError`, avant de regarder sa nature.

`pyst.import_file("spectre.csv", recipe="Jasco (point-virgule)")` accepte un
chemin texte ou `pathlib.Path` et rend la liste des poignées des feuilles
créées. Sans nom de modèle, il applique le choix automatique, sinon la recette
permissive. Il respecte les réglages du modèle (colonnes ignorées, types,
métadonnées, blocs et répartition en feuilles) et crée toujours un classeur
neuf ; les modes de modification d'une feuille active concernent l'assistant.
Un seul `pyst.undo()` ou Ctrl+Z retire tout l'import. Un modèle inconnu ou un
fichier illisible lève `InvalidValueError` avant toute mutation ; une erreur
pendant la création avorte la macro. L'analyse et les commandes se déroulent
GIL relâché, comme les autres appels au modèle.

Une poignée (`Handle`) est **opaque** : pas de constructeur Python, aucun entier
exposé ; la résolution, la navigation, les créations et l'import en produisent. Elle reste
valide après un renommage, un déplacement, et même après une suppression
**suivie d'une annulation** — c'est
le contrat de l'identité, assuré par le modèle (identifiants monotones, jamais
réutilisés). Elle est invalidée par un `Load` ou un `NewProject`, détecté via l'`epoch` de
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

La résolution de chemin n'est plus réécrite dans le pont : `stokesforge-core` expose
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
- `stokesforge-script/tests/navigation.rs` vérifie le contrat du lot #66 : ordre,
  métadonnées, longueurs, valeurs exactes, absence de mutation et identité des
  poignées après renommage, déplacement, suppression/annulation et chargement.
- Lancer les tests Python : `scripts/python-test.sh`, qui pose les trois
  variables d'environnement nécessaires et teste pont, scripts, robustesse,
  application et CLI. Le parcours `script_panel` lit numpy, crée les résultats
  et un graphe, puis annule/rétablit l'ensemble depuis `AppState`. Côté robustesse, `b7` (frontière
  Rust ↔ Python dans les deux sens) et `b9` (script sous contrainte) sont
  derrière la même feature.

**À lire** : `stokesforge-script/src/lib.rs`, `src/engine.rs`, `src/surface.rs`,
`src/bridge.rs`, `stokesforge-python/src/interpreter.rs`, `src/array.rs` (têtes).
