# État du projet et décisions prises

Photographie au **7 septembre 2026**, après la phase 6 rouverte et refermée.
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
| 6 | Tracé + couche `PlotSpec` neutre — **Matplotlib par défaut**, PyQtGraph en visualiseur léger | ✅ |
| 7 | Drag & drop interne | ⬜ suivante |
| 8 → 12 | Analyse, console, export, templates, distribution | ⬜ |

### Chiffres

- **752 tests** verts en profil CI (+1 test `corpus`, désélectionné hors machine de dev)
- **94 % de couverture** globale, branch coverage activée
- ~10 800 lignes de code applicatif, ~9 200 lignes de tests
- Performance : **4 µs/cellule** sur une table de 10⁶ lignes ; premier rendu d'un graphe de 10⁶ points en **141 ms** (Matplotlib, le défaut) et **66 ms** (visualiseur léger) ; réticule blitté à **0,2 ms** par mouvement contre 107 ms pour un dessin complet

### Paquets

| Paquet | Rôle | Dépendances |
|---|---|---|
| `plotyx.core` | Modèle de données | **sans Qt** (test automatique) |
| `plotyx.plot` | Description de graphe `PlotSpec` | **sans Qt, PyQtGraph ni Matplotlib** (test automatique) |
| `plotyx.io` | Parseurs de fichiers | **sans Qt** |
| `plotyx.persistence` | Lecture/écriture HDF5 | sans Qt |
| `plotyx.commands` | Bus d'undo, commandes | Qt (`QUndoStack`) |
| `plotyx.ui` | Widgets, les deux renderers | Qt, Matplotlib, PyQtGraph |

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
| Couche **`PlotSpec` neutre**, dont PyQtGraph était le premier renderer | Deux moteurs de rendu (écran et export publication) divergent inévitablement s'ils ont chacun leur modèle. Conçue en phase 6 et non en phase 10, comme le plan l'exigeait. **Vérifiée à l'usage le 2026-09-07** : changer de moteur par défaut a coûté un renderer et zéro ligne de `plotyx.plot` |
| `PlotSpec` **immuable** | Modifier = calculer un nouveau spec. Une seule commande `SetPlotSpecCommand`, dont le delta d'annulation est l'ancien spec : un graphe de 10 séries d'un million de points coûte à la pile d'undo le prix d'un graphe vide |
| Série = **référence par clé**, jamais une copie des données | Mise à jour automatique quand la donnée change |
| Série orpheline = **état de première classe** | Une colonne supprimée ne fait ni planter ni disparaître la série : elle est signalée, reste modifiable, et se répare à l'`undo` |

> **Rendu par défaut changé le 2026-09-07, et livré le jour même** :
> Matplotlib remplace PyQtGraph comme rendu par défaut (légende, couleurs,
> axes, grille, outils complets) ; PyQtGraph devient le « visualiseur
> léger », minimal, pour l'acquisition live uniquement — bascule manuelle,
> peu visible. Motif : des tests réels ont montré que l'écart de fluidité
> qui justifiait deux moteurs séparés ne se manifeste pas à l'échelle de
> données de l'utilisateur ; le seul avantage de PyQtGraph qui survit au
> constat est un déchargement GPU réel (`useOpenGL`) dont Matplotlib est
> dépourvu. Détail complet :
> `docs/decision-rendu-matplotlib-2026-09-07.md` et la section dédiée de
> `ROADMAP.md`.

Les décisions prises **en écrivant** ce second renderer, qui ne se
déduisent pas de la décision de bascule :

| Décision | Raison | Rouvrir si |
|---|---|---|
| Le **mode de rendu n'est jamais persisté** — ni `.plx`, ni QSettings. Schéma HDF5 **inchangé en version 2** | Un fichier ne doit pas imposer un moteur à qui l'ouvre : un projet enregistré sur une machine d'acquisition s'ouvrirait sinon en vue dégradée sur le poste de dépouillement. On y entre explicitement à chaque fois | Un utilisateur travaille durablement en acquisition et se plaint de rebasculer à chaque ouverture |
| Le **panneau de propriétés reste actif** en visualiseur léger, avec un bandeau plutôt qu'un grisage | Il édite le `PlotSpec`, jamais un renderer : l'édition est enregistrée, annulable, persistée, et visible au retour en vue normale. Le griser ferait dépendre le panneau du moteur actif — ce que l'architecture évite partout ailleurs — et interdirait de préparer une mise en forme pendant une acquisition | — |
| Le **suivi de la souris descend dans le renderer** ; la fenêtre reçoit un rappel `set_coords_callback` | Une scène `QGraphicsView` et un canevas Matplotlib ne signalent pas un mouvement de la même façon. C'est ce qui fait tenir `GraphWindow` sans un seul `isinstance` de moteur | — |
| **Réticule Matplotlib blitté** (`animated=True` + `copy_from_bbox`), repli `draw_idle` | Le seul point où le redessin complet par interaction coûterait vraiment. Mesuré à 10⁶ points : 0,2 ms par mouvement contre 107 ms pour un dessin complet — rapport verrouillé par un test de performance | — |
| **La barre standard `NavigationToolbar` de Matplotlib**, pas une barre maison | Demandé le 2026-09-07 après un premier essai avec une barre personnalisée. C'est le vocabulaire que connaît déjà quiconque a utilisé Matplotlib, et il apporte gratuitement l'historique des vues, la sélection rectangulaire et l'enregistrement de la figure — soit l'export d'image avant la phase 10. Deux écarts : `Home` recadre sur les données au lieu de restaurer la vue initiale (qui devient fausse dès que les données changent, ce qui est ici la règle), et un bouton « Curseurs » est ajouté, le réticule n'ayant pas d'équivalent standard | — |
| **La barre appartient au moteur** (`create_toolbar`), la fenêtre n'en construit une que si le moteur n'en a pas | Même raison que le suivi de la souris. Le visualiseur léger n'a pas d'équivalent et lui en écrire une irait contre ce qu'il est — volontairement pauvre : il garde la barre minimale de `GraphWindow` | — |
| **Molette et glissé conservés en plus de la barre**, mais inertes quand un outil de la barre est armé | La barre ne couvre pas le zoom à la molette, et exiger d'armer Pan pour déplacer serait une régression face à PyQtGraph. Les laisser actifs pendant qu'un outil est armé casserait en revanche le rectangle de zoom. Le déplacement délègue à `Axes.start_pan`/`drag_pan`/`end_pan`, l'API publique qu'utilise la barre elle-même — elle traite les axes log et inversés sans cas particulier | — |
| **Ni barres ni barres d'erreur dans le chemin rapide**, des deux côtés | Leurs artistes secondaires ne se réécrivent pas à la volée. Correctif au passage : l'`ErrorBarItem` de PyQtGraph n'était pas mis à jour par `update_data`, laissant des moustaches périmées à côté d'une courbe à jour | — |
| Titre du graphe en **`suptitle`** côté Matplotlib, alors que PyQtGraph le porte sur le premier panneau | Divergence assumée, et cosmétique : le détour PyQtGraph existe parce qu'un label en ligne 0 y décalerait la grille, contrainte que Matplotlib n'a pas. La règle de non-divergence porte sur le *vocabulaire de style*, pas sur le placement | — |

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
| `reload()` synchrone sur `structure_changed` | `PlotPropertiesPanel._on_structure_changed` | `editingFinished` d'un `QLineEdit` (titre du graphe, titre/unité d'un axe) part encore sur la pile C++ de `keyPressEvent` ; le panneau reconstruisait `_body` depuis ce même signal, détruisant le widget qui le traitait — use-after-free, plantage réel constaté en test manuel (2026-09-07). Corrigé en différant d'un tick d'événements, comme `GraphWindow.refresh` |
| Moustaches d'erreur périmées | `PyQtGraphRenderer.update_data` | L'`ErrorBarItem` d'une série est un item distinct de sa courbe ; le chemin rapide ne réécrivait que la courbe. Une saisie dans la colonne d'erreur laissait donc des moustaches obsolètes à côté d'une courbe à jour. Trouvé en écrivant le renderer Matplotlib — l'exercice de faire deux fois la même chose est ce qui l'a rendu visible. Corrigé des deux côtés : barres et barres d'erreur rendent la main à `render` |

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
| **Fragilité Shiboken en test** | **Motif confirmé — troisième signal, le 2026-09-07.** (1) SIGSEGV en construisant un `QDropEvent` réel en Python, contourné par `Mock` ; (2) le SIGSEGV de fin de session attribué à `unraisableexception` ; (3) en suite complète seulement, jamais isolé, un `QMenu` retourné par `addMenu` arrivant comme `QUndoCommand` — confusion de wrapper par réutilisation d'adresse, sur le seul menu dont la référence Python n'était pas retenue. Contre-mesure appliquée : **tout objet Qt créé par une fabrique C++ (`addMenu`, `addToolBar`, …) est retenu côté Python**. Non reproduit depuis (4 exécutions complètes en ordre aléatoire). La règle est peu chère ; s'en écarter est ce qui coûte |
| **Sous-fenêtre MDI orpheline** | Supprimer un `Worksheet`/`Graph` depuis l'explorateur ne ferme pas sa sous-fenêtre si elle est ouverte : `MainWindow` ne s'abonne nulle part à `structure_changed` du projet. Vérifié réel le 2026-09-07, délibérément reporté. À trancher avec le correctif : un `undo` de la suppression doit-il rouvrir la sous-fenêtre ? |
| **Viewport OpenGL du visualiseur léger** | `useOpenGL=True` est **le seul argument** qui a sauvé PyQtGraph de la suppression (§3.5) — et il n'est toujours pas activé : le mode existe, son avantage décisif dort. À armer quand un cas d'acquisition live réel se présentera, pas avant, et à mesurer sur la machine cible |

---

## 7. Répartition des modèles

Opus sur les phases dont une erreur de conception se paie en réécriture : **2** (bus d'undo), **6** (couche `PlotSpec`), **9** (branchement de la console). Sonnet ailleurs.

La phase 9 est un **test d'architecture** : si brancher la console demande de dupliquer de la logique, c'est que l'invariant 1 a fui quelque part.
