# État du projet et décisions prises

Photographie au **5 septembre 2026**, après la phase 6.
Le plan de développement est dans [`../ROADMAP.md`](../ROADMAP.md) ; ce document dit **où en est le code** et **ce qui a été tranché en chemin**, avec les raisons. Il sert à ne pas rejuger deux fois la même chose, et à savoir quoi rouvrir si une hypothèse tombe.

---

## 1. Où en est le développement

**Phases 0 à 6 terminées.** Jalon **v0.3** atteint : importer un fichier d'instrument réel et le tracer.

| Phase | Contenu | État |
|---|---|---|
| 0 | Squelette applicatif, packaging Briefcase, CI | ✅ |
| 1 | Modèle de données (`Project`/`Workbook`/`Worksheet`/`Column`) | ✅ |
| 2 | Bus de commandes & undo | ✅ |
| 3 | UI worksheet (table, en-têtes riches, explorateur) | ✅ |
| 4 | Persistance HDF5 `.plx`, autosave, récupération | ✅ |
| 5 | Import de fichiers (registre de parseurs, wizard) | ✅ |
| 6 | Tracé PyQtGraph + couche `PlotSpec` neutre | ✅ |
| 7 | Drag & drop interne | ⬜ suivante |
| 8 → 12 | Analyse, console, export, templates, distribution | ⬜ |

### Chiffres

- **662 tests** verts en profil CI (+1 test `corpus`, désélectionné hors machine de dev)
- **94 % de couverture** globale, branch coverage activée
- ~10 000 lignes de code applicatif, ~8 300 lignes de tests
- Performance : **4 µs/cellule** sur une table de 10⁶ lignes ; premier rendu d'un graphe de 10⁶ points en **88 ms**

### Paquets

| Paquet | Rôle | Dépendances |
|---|---|---|
| `plotyx.core` | Modèle de données | **sans Qt** (test automatique) |
| `plotyx.plot` | Description de graphe `PlotSpec` | **sans Qt, PyQtGraph ni Matplotlib** (test automatique) |
| `plotyx.io` | Parseurs de fichiers | **sans Qt** |
| `plotyx.persistence` | Lecture/écriture HDF5 | sans Qt |
| `plotyx.commands` | Bus d'undo, commandes | Qt (`QUndoStack`) |
| `plotyx.ui` | Widgets, renderer PyQtGraph | Qt, PyQtGraph |

Les dépendances vont dans un seul sens : `ui → commands → core`, et `plot → core` sans retour.

---

## 2. Les invariants

Quatre règles qui structurent tout le reste. Les enfreindre coûte une réécriture, pas un correctif.

1. **Aucune mutation du modèle en dehors d'une commande du bus.** C'est ce qui rendra la console de script (phase 9) annulable sans une ligne de code supplémentaire. Outillé par `plotyx/commands/guard.py` et `core/guard.py`.
2. **`plotyx.core` n'importe jamais Qt**, et `plotyx.plot` n'importe ni Qt ni aucun moteur de rendu. Vérifié par des tests dédiés qui échouent si la règle est enfreinte.
3. **Le packaging se valide en continu**, pas à la fin.
4. **Aucun calcul lourd sur le thread UI.**

---

## 3. Décisions de conception

### 3.1 Modèle de données

| Décision | Raison | Rouvrir si |
|---|---|---|
| Valeurs manquantes = `float64` + `NaN`, pas `np.ma` | Plus rapide, suffisant en usage scientifique | Un besoin de masque explicite apparaît |
| **Colonnes de longueur variable**, pas de feuille rectangulaire | Comme Origin ; l'alternative impose du padding NaN partout. Cas d'usage réel : des spectres de longueurs différentes cohabitent dans une feuille | — |
| Invariant : **une colonne de longueur L n'a pas de ligne au-delà de L** | Les lignes hors portée sont manquantes implicitement. Rend `insert_rows`/`delete_rows` strictement inverses | — |
| Niveau **`Workbook`** entre `Folder` et `Worksheet` | La grammaire `[Book1]Sheet1!ColA` l'exige : sans classeur, les crochets n'ont pas de référent. Écart assumé au plan initial | — |
| Notifications en **Python pur** dans `core`, adaptées vers Qt par `ui` | La roadmap demandait des « signaux Qt émis par le modèle », en contradiction avec l'invariant 2. C'est l'invariant qui gagne | — |
| Clés de registre = **UUID de naissance**, en références faibles | Renommer ou déplacer n'invalide pas une clé — indispensable au drag & drop (7) et à la console (9) | — |

### 3.2 Undo

| Décision | Raison |
|---|---|
| Undo **par delta**, jamais par snapshot (une exception documentée) | Des colonnes de millions de valeurs ; copy-on-write sur les arrays non modifiés |
| Les commandes enveloppent les **primitives**, pas les façades `Project` | `Project.move` est un composite qui masque les deux deltas dont l'annulation a besoin |
| Le bus tient un **miroir de la pile** `QUndoStack` | Qt ne signale ni l'éviction par `setUndoLimit`, ni la troncature de la branche de redo, ni `clear()`. Sans ce miroir, une commande de suppression ne sait jamais quand relâcher l'objet qu'elle retient — fuite mémoire d'un côté, clé morte de l'autre |
| Un objet supprimé **sort** du registre et y revient sous la même clé à l'`undo` | La durée de vie est portée par la commande. Conséquence exploitée en phase 6 : une série orpheline se répare toute seule |

### 3.3 Persistance

| Décision | Raison |
|---|---|
| Extension **`.plx`** | `.pyx` (prévu initialement) est l'extension de Cython — collision avec les éditeurs et les outils de build. `.plx` : aucune association dans la base MIME du système |
| **Version de schéma dès le premier octet**, avec fonction de migration | Sans elle, les projets des utilisateurs deviennent illisibles à la première mise à jour du format |
| Un **dataset HDF5 par colonne** | Une table rectangulaire trahirait les longueurs variables et gonflerait les fichiers |
| **UUID persistés**, pas régénérés à la lecture | Les séries de graphes référencent les colonnes par clé : régénérer casserait toutes les références à chaque ouverture |
| Compression **gzip niveau 4 + `shuffle`** | Mesuré sur du signal spectroscopique : −42 % avec `shuffle` contre −15 % sans. Le filtre fait l'essentiel du travail sur des flottants lentement variables |
| **État final uniquement**, pas d'historique d'undo | §4 de l'architecture. Le groupe `/history` reste réservé, non implémenté |

Schéma actuel : **version 2** (la v2 ajoute le `PlotSpec` sur les nœuds `graph` ; migration additive qui n'invente pas de spec vide, ce qui confondrait « jamais tracé » et « explicitement vide »).

### 3.4 Import

| Décision | Raison |
|---|---|
| Registre de parseurs à **score**, jamais d'aiguillage par extension | Un `.txt` peut être du Jasco, du JCAMP-like ou une matrice large |
| Le **comptage de cohérence prime sur `csv.Sniffer`** | Le Sniffer se fait avoir par la virgule décimale, qui forme à elle seule un « délimiteur » cohérent |
| Séparateur décimal déduit des **champs numériques d'en-tête**, pas de `LOCALE` | `LOCALE` est un indice fort, pas une règle : d'autres locales utilisent le point |
| **Warnings structurés** dans `ImportResult`, pas d'exception fatale | Un fichier partiellement lisible vaut mieux qu'un import qui plante |
| Fixtures versionnées **synthétiques uniquement** | Le dépôt est public ; les fichiers de thèse sont des données non publiées. Le corpus réel sert à un test local marqué `corpus`, skippé en CI |

Taux mesuré sur le corpus réel : **202/209 (96,7 %)**. Les 7 échecs sont des `.dat` à extension texte mais contenu binaire, rejetés proprement.

### 3.5 Tracé

| Décision | Raison |
|---|---|
| Couche **`PlotSpec` neutre**, dont PyQtGraph est le premier renderer | Deux moteurs de rendu (écran et export publication) divergent inévitablement s'ils ont chacun leur modèle. Conçue en phase 6 et non en phase 10, comme le plan l'exigeait |
| `PlotSpec` **immuable** | Modifier = calculer un nouveau spec. Une seule commande `SetPlotSpecCommand`, dont le delta d'annulation est l'ancien spec : un graphe de 10 séries d'un million de points coûte à la pile d'undo le prix d'un graphe vide |
| Série = **référence par clé**, jamais une copie des données | Mise à jour automatique quand la donnée change |
| Série orpheline = **état de première classe** | Une colonne supprimée ne fait ni planter ni disparaître la série : elle est signalée, reste modifiable, et se répare à l'`undo` |

---

## 4. Décisions de plateforme et d'outillage

### macOS retiré du périmètre de release

Motifs : notarisation Apple (compte annuel, `notarytool`, rejets opaques), runners CI facturés 10× le tarif Linux, matériel de test dédié, MDI sans équivalent natif — et OriginLab lui-même n'existe pas sur macOS.

**La cible reste compilable, jamais supportée.** Quatre règles, déjà exigées par Windows + Linux, gardent le retour à 1–2 semaines au lieu de 5–8 : zéro API OS-spécifique, `pathlib`/`QStandardPaths` partout, `QKeySequence.StandardKey` uniquement, polices d'export embarquées.

### `xcb` forcé par défaut sous Wayland

Un `QDockWidget` détaché est **immobile** sous Wayland : le protocole interdit à une application de positionner ses propres fenêtres de premier niveau, ce dont Qt a besoin pour déplacer un dock flottant.

Ce n'est pas un incident isolé mais un motif, qui touche aussi le détachement d'un graphe sur un second écran et la restauration de la disposition des fenêtres (phase 11).

Non réparable côté application : le protocole `xdg_toplevel_drag_v1` existe et **les deux extrémités sont prêtes** (`libmutter-18` l'implémente, `libQt6WaylandClient` embarque les bindings) — mais le câblage manque dans `QDockWidget`, dont les classes Qt sont privées et inatteignables depuis PySide6.

**Condition de retour** : lancer avec `QT_QPA_PLATFORM=wayland`, détacher un dock, le déplacer. Si ça marche, inverser le défaut dans `select_linux_platform()` — une ligne.

### Dépôt public, CI entièrement sur GitLab

Public **≠ open source** : la licence reste propriétaire, le code est lisible sans qu'aucun droit ne soit cédé. Conséquence concrète : le programme « GitLab for Open Source » (quota de minutes étendu) exige une licence OSI et reste inaccessible.

Pas de miroir GitHub Actions : le gain ne justifie pas d'entretenir une seconde plateforme pour un besoin de la phase 12. Le tier gratuit ne fournissant que des runners Linux avec un quota limité, le build AppImage est **manuel sur `main`, automatique sur tag** ; seuls les jobs légers tournent à chaque push.

**Coût assumé** : la validation du packaging Windows n'est pas continue d'ici la phase 12, en écart délibéré avec le principe de la phase 0.

### Le plugin pytest `unraisableexception` reste armé

Il avait été désarmé pour un SIGSEGV de fin de session attribué au `gc.collect()` qu'il force. Le plantage ne s'est pas reproduit (5 exécutions sur 5). Le masque a donc été retiré : ce plugin signale les exceptions avalées dans `__del__` et pendant le ramasse-miettes, soit exactement la classe de bug de durée de vie d'objets sur laquelle repose la rétention du bus. **S'il réapparaît, capturer une reproduction avant de remasquer.**

---

## 5. Bugs trouvés et corrigés en chemin

| Bug | Où | Nature |
|---|---|---|
| `folder or self._root` | `Project.add` | `Folder` définit `__len__` : un dossier **vide est falsy**, donc tout objet rangé dedans atterrissait à la racine |
| `if sheet` / `if column` | `Project.path_of` | Même piège, deuxième occurrence : une feuille ou colonne vide n'était pas adressable par chemin |
| `min(at, len(column))` | `Worksheet.insert_rows` | Allongeait d'un NaN toute colonne trop courte, quand `delete_rows` l'ignorait : les deux primitives n'étaient pas inverses, ce qui les tenait hors du bus |

Le piège du conteneur *falsy* s'est manifesté **deux fois**, malgré un commentaire d'avertissement dans le fichier même. À traiter comme un motif connu, pas comme deux accidents.

---

## 6. Ce qui reste ouvert

| Sujet | Décision attendue |
|---|---|
| **Texte du `LICENSE`** | Placeholder « tous droits réservés » sur un dépôt public. À trancher avant la première release |
| **Runner Windows** | VM auto-hébergée, prévue phase 12. Aucun build MSI vérifié d'ici là |
| **Portabilité de l'AppImage** | Construit avec `--no-docker` faute de Docker sur la machine de dev (podman seulement) : il est lié aux bibliothèques système de Fedora, donc pas garanti portable sur une distro plus ancienne. À traiter avant la v1.0 |
| **Poids du binaire** | 544 Mo mesurés, contre 150–300 Mo annoncés dans le document d'architecture |
| **Détachement d'un graphe sur un second écran** | Piste retenue : une seconde `QMainWindow` en plein écran sur un `QScreen` choisi, plutôt qu'un arrachage à la souris — `showFullScreen()` n'exige pas de positionner la fenêtre. À traiter avec la phase 11 |
| **Fragilité Shiboken en test** | Un SIGSEGV reproductible en construisant un `QDropEvent` réel en Python (code de production hors de cause), contourné par `Mock`. Deuxième signal du genre ; un troisième ferait un motif |

---

## 7. Répartition des modèles

Opus sur les phases dont une erreur de conception se paie en réécriture : **2** (bus d'undo), **6** (couche `PlotSpec`), **9** (branchement de la console). Sonnet ailleurs.

La phase 9 est un **test d'architecture** : si brancher la console demande de dupliquer de la logique, c'est que l'invariant 1 a fui quelque part.
