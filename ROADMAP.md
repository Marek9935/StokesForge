# PlotYx — Roadmap de développement

Basée sur `architecture-logiciel-origin-like.md`.
Stack : PySide6 + qt-material / PyQtGraph + Matplotlib / numpy-scipy-pandas / HDF5 (h5py) / qtconsole / Briefcase.

**Principe directeur** : le cœur du logiciel n'est ni l'UI ni les graphes, c'est le **modèle de données colonnaire** et le **bus de commandes annulables**. Tout le reste s'y branche. Toute fonctionnalité ajoutée avant ces deux socles devra être réécrite.

**Périmètre plateformes** : **Windows + Linux uniquement**. macOS est hors périmètre de release (décision du 2026-09-04) — voir « Politique macOS » en fin de document. La cible reste *compilable*, jamais *supportée*.

---

## Phase 0 — Fondations projet (≈1 semaine)

**Objectif** : squelette exécutable, packagé, testé, dès le jour 1.

| # | Tâche | Livrable |
|---|---|---|
| 0.1 | Layout `src/plotyx/`, `pyproject.toml` (Briefcase + hatchling), venv Fedora | Projet importable |
| 0.2 | `QMainWindow` vide + qt-material + `QMdiArea` + 3 `QDockWidget` stubs | App qui se lance |
| 0.3 | pytest + `pytest-qt`, un test smoke qui ouvre/ferme la fenêtre | CI verte |
| 0.4 | GitLab CI : lint (ruff) + tests + couverture sur runner Linux partagé | Pipeline |
| 0.5 | `briefcase create/build/package` AppImage (manuel sur `main`, auto sur tag) | AppImage dès la v0.0.1 |
| 0.6 | Jobs Windows / macOS déclarés mais inertes (`tags:`, `manual`) | Intention documentée |

> ⚠️ Point de vigilance #3 du doc : le packaging se valide **maintenant**, pas à la fin. Un projet Qt+numpy qui ne s'empaquette qu'au mois 6 découvre 6 mois de dettes d'un coup.

**Sortie de phase** : un binaire téléchargeable qui affiche une fenêtre vide stylée, sur Windows et Linux.

---

## Phase 1 — Modèle de données (≈2 semaines)

**Objectif** : le noyau métier, sans aucune UI. 100% testable en CLI.

- `Column` : `np.ndarray` + métadonnées (nom, unité, label long, commentaire, rôle X/Y/yErr, format d'affichage, type)
- `Worksheet` : liste ordonnée de colonnes, longueurs hétérogènes tolérées, opérations insert/delete/move/rename
- `Project` : arbre `Folder → Workbook → Worksheet → Column`, plus `Graph | Matrix | Note` comme nœuds frères du `Workbook` ; chemins uniques type `[Book1]Sheet1!ColA`
- `ObjectRegistry` : clé stable ↔ objet, indispensable pour le drag & drop par référence (§5 du doc) et pour la console

> **Correction apportée à ce plan (2026-09-04)** : le niveau `Workbook` (« classeur ») manquait à l'arbre initialement écrit `Folder → Worksheet`. La grammaire `[Book1]Sheet1!ColA`, héritée d'Origin, l'exige : sans classeur, la partie entre crochets n'a pas de référent. Écart validé, structurant pour le schéma HDF5 de la phase 4.
- Signaux Qt (`dataChanged`, `structureChanged`) émis par le modèle → prépare la vue

**Décisions tranchées ici** (2026-09-04, coûteuses à revenir dessus) :
1. Valeurs manquantes : **`ndarray` `float64` + `NaN`**, pas de `np.ma`. Plus rapide, suffisant en usage scientifique.
2. **Colonnes de longueur variable**, pas de worksheet rectangulaire (comme Origin ; l'alternative impose du padding NaN partout). La lecture hors bornes rend la valeur manquante sans rien stocker, ce qui rend `row_count = max(len)` suffisant.
3. **Notifications en Python pur** dans `core/`, adaptées vers les signaux Qt par la couche `ui/`. La roadmap demandait des « signaux Qt émis par le modèle », en contradiction avec le principe « aucun Qt dans `core/` » ; c'est le principe qui gagne, et un test le vérifie automatiquement.
4. **Clés du registre = UUID de naissance**, jamais fonction du nom ni du chemin : renommer ou déplacer ne les invalide pas. Références faibles — un objet supprimé sort du registre et y revient sous la même clé au `undo`, ce qui reporte la responsabilité de sa durée de vie sur la `QUndoCommand` de la phase 2.

**Sortie de phase** : `pytest` couvre le modèle à >85 %, aucune dépendance UI dans `plotyx/core/`.

---

## Phase 2 — Bus de commandes & Undo (≈1,5 semaine)

**Objectif** : la règle d'or du §3 — **aucune mutation du modèle en dehors d'une `QUndoCommand`**.

- `QUndoStack` par projet, `setUndoLimit(N)` configurable
- Commandes de base : `SetCellsCommand`, `AddColumnCommand`, `DeleteColumnCommand`, `RenameCommand`, `MoveCommand`
- Stratégie mémoire : **delta/paramètres d'abord**, snapshot uniquement en dernier recours (grosse opération irréversible) ; copy-on-write sur les arrays non touchés
- `beginMacro()/endMacro()` pour les opérations composites (ex. un import = 1 seule entrée d'historique)
- `cleanChanged` → astérisque titre + prompt de sauvegarde
- Test anti-régression : un test qui **échoue si une mutation du modèle est faite hors commande** (hook de mutation en mode debug)

**Décisions tranchées ici** (2026-09-04) :

1. **Paquet séparé `plotyx/commands/`.** `QUndoStack`/`QUndoCommand` sont des classes Qt et `core/` n'a pas le droit d'importer Qt. La couche d'undo vit donc à côté, avec une dépendance à sens unique : `commands` connaît `core`, jamais l'inverse. `tests/test_core_no_qt.py` reste vert sans modification.
2. **Les commandes enveloppent les primitives, pas les façades `Project`.** Un module d'aiguillage (`commands/dispatch.py`) traduit `(conteneur, opération)` en appel de la primitive du conteneur. Les façades `Project.add/remove/rename/move` restent de l'ergonomie d'API ; `Project.move`, composite, masque justement les deux deltas dont l'annulation a besoin.
3. **Application avant empilement.** `QUndoStack.push()` appelle `redo()` de l'autre côté de la frontière C++, où une exception Python n'a pas de comportement garanti. `CommandBus.push()` applique donc la commande lui-même : un échec (nom en double, index hors bornes) remonte proprement et la pile n'a pas bougé.
4. **Durée de vie des objets supprimés.** Une commande de suppression garde une référence forte sur l'objet retiré tant qu'elle est sur la pile. Le bus tient un miroir de la pile et appelle `release()` sur toute commande qui en sort — éviction par `setUndoLimit`, troncature de la branche de redo, `clear()`. Testé pour de vrai avec `gc` et `weakref`, y compris sur un tableau de 2 Mo.
5. **Un seul snapshot, documenté.** `SetColumnTypeCommand` conserve le tableau précédent : la conversion de type est destructrice dans les deux sens, aucun delta ne la reconstruit. Toutes les autres commandes s'annulent par delta ou par simples paramètres.
6. **Abandon de macro.** `QUndoStack` n'a pas de `cancelMacro()`. `CommandBus.abort_macro()` ferme la macro, l'annule, puis pousse une commande qui se déclare obsolète pendant son `redo()` : la branche de redo est purgée et la commande disparaît, la pile retrouve exactement son état d'avant.

**Écart constaté sur la phase 1, corrigé** : `Worksheet.insert_rows` rabattait son index en `min(at, len(column))`, ce qui allongeait d'un NaN toute colonne trop courte pour atteindre `at`, alors que `delete_rows` ignorait purement cette colonne — les deux primitives n'étaient donc pas inverses. Corrigé en alignant l'insertion sur la suppression : les deux sautent désormais le même ensemble de colonnes.

L'invariant sous-jacent découle du choix de colonnes de longueur variable : **une colonne de longueur L n'a pas de ligne au-delà de L**, ces lignes étant déjà manquantes implicitement ; une insertion ou une suppression hors de sa portée est sans effet sur elle. Le jeu de primitives est ainsi refermé, sans exception à la règle d'or.

Les commandes de lignes restent volontairement à l'échelle de la **colonne**, une opération de feuille s'exprimant en macro (`test_a_sheet_wide_row_insert_is_a_macro_of_column_commands`) — non plus par contrainte, mais parce que la granularité est plus fine. Les primitives de feuille peuvent désormais être enveloppées telles quelles si la phase 3 y trouve intérêt.

**Sortie de phase** : script de test qui applique 50 commandes aléatoires, undo tout, et vérifie l'égalité bit à bit avec l'état initial.

---

## Phase 3 — Worksheet UI (≈2 semaines)

- `QAbstractTableModel` branché sur `Worksheet` (jamais de copie des données dans la vue)
- `QTableView` en sous-fenêtre MDI, en-têtes riches (nom + unité + label sur plusieurs lignes, façon Origin)
- Édition cellule → `SetCellsCommand` ; copier/coller multi-cellules ; sélection colonne/plage
- Dialogue « Column Properties » (nom, unité, rôle, format)
- Dock « Project Explorer » : `QTreeView` sur `Project`
- Perf : vue virtualisée, test avec 1 M lignes × 20 colonnes → scroll fluide

> Le MDI façon Origin est idiomatique sur Windows et acceptable sur Linux. C'est le seul choix d'UI que macOS aurait remis en cause — le couper lève cette contrainte de conception.

**Sortie de phase** : on peut saisir/coller des données à la main, les annuler, naviguer dans l'arbre projet.

---

## Phase 4 — Persistance HDF5 (≈1 semaine)

- Schéma `/project` : groupes = folders, datasets = colonnes, attributs HDF5 = métadonnées
- **Numéro de version de schéma dès le premier octet écrit** + fonction de migration (stub) — non négociable pour un format de projet
- Save / Save As / Open ; état final uniquement (§4)
- Autosave périodique vers fichier temporaire + recovery au démarrage après crash
- Optionnel plus tard : groupe `/history` (journal d'opérations, traçabilité, non rejouable)

**Sortie de phase** : round-trip complet — créer, sauver, fermer, rouvrir, égalité stricte.

---

## Phase 5 — Import de fichiers (≈2,5 semaines)

- `FileParser` abstrait : `can_parse(path) -> score` + `parse(path, options) -> ImportResult(data, metadata, warnings)`
- `ParserRegistry` : sélection par score, jamais d'import monolithique
- `CsvParser` : `chardet` (encodage), `csv.Sniffer` (délimiteur), séparateur décimal, lignes d'en-tête/métadonnées à sauter
- **Wizard d'import** avec preview tableau + overrides manuels — jamais d'import à l'aveugle
- Gros fichiers : `pandas.read_csv(chunksize=...)`, barre de progression, annulable
- Drag & drop OS → app (`hasUrls()`) branché sur le même wizard
- Warnings structurés remontés en UI, l'import ne lève pas d'exception fatale
- `JcampParser` (lib `jcamp`) comme deuxième parseur, pour valider que le registre tient

**Sortie de phase** : jeu de ~15 fichiers réels hétérogènes importés sans intervention manuelle sur au moins 12.

---

## Phase 6 — Plots PyQtGraph (≈3 semaines)

- `GraphWindow` MDI : `PlotWidget`, axes, légende, curseurs, zoom/pan
- Modèle de plot : une série = **référence** `(worksheet_key, x_col, y_col)`, pas une copie → mise à jour auto quand la donnée change
- Types : scatter, line, scatter+line, barres, histogramme, error bars
- Panneau de propriétés (dock) : styles, couleurs, échelles log, titres, limites d'axes
- Perf : `downsample`/`clipToView` sur gros datasets, tests à 10⁶ points
- Multi-panneaux / axes secondaires

**Sortie de phase** : un graphe présentable à l'écran, interactif, à partir d'un import réel.

---

## Phase 7 — Drag & Drop interne (≈1 semaine)

Dépend de 3 + 6. Le §5 du doc en entier :

- MIME custom `application/x-plotyx-columns`, payload = **clés du registre**, jamais les données
- Zones de dépose distinctes (axe X / axe Y / nouvelle série) via `dragMoveEvent`
- Auto-switch d'onglet MDI au survol prolongé
- `dropEvent` → `QUndoCommand`, jamais d'action directe
- **Test Wayland Fedora dès le premier jour de la phase**, fallback documenté `QT_QPA_PLATFORM=xcb`
- Matrice de test réduite à Windows + X11 + Wayland (3 combinaisons au lieu de 4)

---

## Phase 8 — Analyse & calcul (≈3 semaines)

- Couche `plotyx.analysis` pure numpy/scipy, sans Qt, testable seule
- Statistiques descriptives, lissage, dérivée/intégrale, FFT, interpolation, baseline
- Fit non-linéaire (`scipy.optimize.curve_fit`) : bibliothèque de modèles + modèle utilisateur, rapport de fit (params, σ, R², résidus), courbe de fit ajoutée comme série
- **Toute opération longue en `QThread`** (point de vigilance #2) : worker + progression + annulation. Pattern unique réutilisé partout.
- `numba` seulement si un profil le justifie — pas avant mesure

---

## Phase 9 — Console Python embarquée (≈1,5 semaine)

- `qtconsole` dans un dock, kernel in-process
- API publique `plotyx.api` exposée (`pyx.active_sheet()`, `pyx.plot(...)`, …)
- **Contrainte structurante du §3** : chaque appel API passe par les mêmes `QUndoCommand` que l'UI → tout script devient annulable gratuitement. Si l'API contourne le bus, c'est un bug bloquant.
- Historique, complétion, chargement/exécution de scripts `.py`

> Cette phase est un **test d'architecture** : si brancher la console demande de dupliquer de la logique, c'est que les phases 1–2 ont fui. Prévoir une marge.

---

## Phase 10 — Export publication (≈1,5 semaine)

- Backend Matplotlib parallèle à PyQtGraph : mêmes séries, rendu qualité publication
- Export PNG/SVG/PDF/EPS + **pgf/LaTeX**, DPI et taille physique paramétrables
- Thèmes de rendu (police, tailles, épaisseurs) sauvegardés dans le projet
- Piège identifié : deux moteurs de rendu = risque de divergence. Mitiger avec une couche `PlotSpec` neutre dont PyQtGraph et Matplotlib sont deux *renderers*. **À concevoir en phase 6, pas ici.**

---

## Phase 11 — Templates, mise en page, finitions (≈2 semaines)

- Templates de graphe et de worksheet réutilisables
- Page de layout multi-graphes pour figure d'article
- Préférences utilisateur, thème clair/sombre, raccourcis
- Sessions récentes, restauration de la disposition des fenêtres

---

## Phase 12 — Distribution (≈1 semaine)

- **Mise en place de la VM Windows auto-hébergée** : runner GitLab enregistré sur le projet, et machine propre pour tester l'installeur (point de vigilance #3). C'est ici que la validation continue du packaging Windows démarre réellement.
- Signature Authenticode Windows (évite SmartScreen)
- AppImage Linux `manylinux_2_28`, pas de signature obligatoire
- Réduction du binaire (~150–300 Mo attendu, normal pour la catégorie)
- **Test d'installation sur machines propres**, pas la machine de dev
- Mécanisme de mise à jour, crash reporting, docs utilisateur

> Phase ramenée de 2 semaines à 1 : la notarisation Apple (compte développeur, `notarytool`, stapling, rejets opaques) était le poste dominant et disparaît.

---

## Chemin critique

```
0 → 1 → 2 → 3 → 4 ─┬→ 5 ──┐
                   └→ 6 ──┴→ 7 → 8 → 9 → 10 → 11 → 12
```

Phases 5 et 6 parallélisables une fois la 4 finie. Tout le reste est séquentiel : 2 dépend de 1, et 9 dépend de la propreté de 2.

**Total estimé** : **~21 semaines** à temps plein pour une v1.0 vendable (24 avant la coupe macOS). Le gain se répartit en ~1 semaine sur la phase 12, ~0,5 semaine sur la phase 0, et ~1,5 semaine diffuse de QA sur les phases UI (3, 6, 7, 11).

---

## Jalons de démo

| Jalon | Contenu | Fin de phase |
|---|---|---|
| **v0.1** | Fenêtre + worksheet éditable + undo | 3 |
| **v0.2** | Ouvrir/sauver un projet `.pyx` | 4 |
| **v0.3** | Importer un CSV réel et le tracer | 6 |
| **v0.4** | Drag & drop colonne → graphe | 7 |
| **v0.5** | Fit + console scriptable | 9 |
| **v0.9** | Export publication + templates | 11 |
| **v1.0** | Installeurs signés, Windows + Linux | 12 |

---

## Risques classés

| Risque | Impact | Mitigation |
|---|---|---|
| Console qui contourne le bus d'undo | Réécriture de la couche métier | Règle « aucune mutation hors commande » testée automatiquement dès phase 2 |
| Divergence PyQtGraph / Matplotlib | Rendu écran ≠ rendu export | Couche `PlotSpec` neutre conçue en phase 6 |
| Drag & drop Wayland | Fonction cassée sur Fedora, la machine cible | Test dès le début de la phase 7, fallback xcb documenté |
| Format HDF5 non versionné | Projets utilisateurs illisibles après update | Version de schéma + migration dès phase 4 |
| Packaging découvert tard | Semaines perdues en fin de projet | Build Windows + Linux en CI dès la phase 0 |
| Dérive Windows-only pendant l'absence de macOS | Retour macOS à 6 semaines au lieu de 1 | Job CI macOS non bloquant + 4 règles de la Politique macOS |
| Undo par snapshot sur gros datasets | Explosion RAM | Undo par delta, COW, `setUndoLimit` |

---

## Politique macOS (décision 2026-09-04)

**macOS est retiré du périmètre de release.** Motifs : notarisation Apple (compte 99 $/an, `notarytool`, stapling, rejets opaques), runners CI facturés 10× le tarif Linux sur repo privé, matériel de test dédié, MDI sans équivalent natif — et OriginLab lui-même n'existe pas sur macOS, le marché cible est Windows avec du Linux en labo.

### La cible reste compilable, jamais supportée

Le job CI macOS est conservé en **`continue-on-error: true`**, sans publication d'artefact. Il coûte quelques minutes de runner par push et signale gratuitement le jour où quelque chose casse. C'est ce qui garde le retour à ~1–2 semaines au lieu de ~5–8.

### Les 4 règles à ne pas relâcher

Elles sont déjà exigées par le support Windows + Linux ; macOS n'en ajoute aucune. Les tenir est gratuit, les relâcher est le seul vrai risque.

1. **Zéro API OS-spécifique** — pas de `win32`, pas de `ctypes` système, pas de registre. Tout passe par Qt.
2. **Chemins** — `pathlib` partout, `QStandardPaths` pour config/cache/documents, jamais de séparateur en dur, jamais d'hypothèse d'insensibilité à la casse.
3. **Raccourcis** — `QKeySequence.StandardKey` uniquement (mappe `Ctrl`/`Cmd` tout seul), pas de `"Ctrl+Z"` littéral.
4. **Polices d'export** (phase 10) — le rendu Matplotlib ne dépend d'aucune police installée par Windows ; embarquer les polices utilisées.

### Coût de réactivation

| Scénario | Coût |
|---|---|
| Les 4 règles tenues + job CI conservé | **1–2 semaines** (essentiellement notarisation + universal2/arm64 + revue UI du MDI) |
| Règles relâchées, CI macOS supprimée | **5–8 semaines** — le coût n'est pas Apple, c'est de déterrer des hypothèses Windows disséminées dans 30 000 lignes |

À rouvrir sur signal marché réel (demandes utilisateurs), pas par principe.

---

## Hébergement et CI (décision 2026-09-04)

**Dépôt public sur GitLab, tout le CI/CD sur GitLab.** Pas de miroir GitHub Actions : le gain (minutes illimitées sur dépôt public, runners `windows-latest` gratuits) ne justifie pas d'entretenir une seconde plateforme, des secrets de synchronisation et une ambiguïté sur l'endroit où vivent les issues — pour un besoin qui ne se matérialise qu'à la phase 12.

**Public ≠ open source.** La licence reste propriétaire (« tous droits réservés ») : le code est lisible, aucun droit n'est cédé. C'est du *source-available*, compatible avec la vente closed-source visée par l'architecture. Conséquence concrète unique : le programme « GitLab for Open Source » (quota de minutes étendu) exige une licence approuvée OSI et reste donc inaccessible.

### Conséquence sur le quota de minutes

Le tier gratuit GitLab.com limite les minutes de calcul sur runners partagés, et ne fournit que du Linux. D'où le découpage retenu :

| Job | Déclenchement |
|---|---|
| `lint:ruff`, `test:linux` (+ couverture) | à chaque push — léger, c'est le filet des phases 1 à 11 |
| `package:linux:appimage` | **manuel** sur `main`, **automatique sur tag** — lourd (~544 Mo), poste de consommation principal |
| `package:windows:msi`, `package:macos` | inertes, en attente d'un runner auto-hébergé (`tags:`) |

À surveiller dans *Settings → Usage Quotas*.

### Coût assumé

**La validation du packaging Windows n'est pas continue** d'ici la phase 12 — contrairement au principe posé en phase 0. C'est un écart délibéré : monter l'infra Windows à la phase 1 pour un besoin de la phase 12 coûte plus cher que le risque encouru, d'autant qu'une machine Windows propre sera de toute façon nécessaire pour tester l'installeur. À réévaluer si un bug de packaging Windows apparaît avant la phase 12.

### Reste ouvert

- **Texte définitif du `LICENSE`** : le fichier actuel est un placeholder propriétaire. À trancher avant la première release publique.
