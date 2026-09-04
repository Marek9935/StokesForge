# Architecture — Logiciel type OriginLab (from scratch)

## 1. Stack technique globale

| Couche | Techno | Licence | Rôle |
|---|---|---|---|
| UI | **PySide6** | LGPLv3 (OK commercial) | Framework fenêtres/widgets |
| Thème | **qt-material** | MIT | Look moderne flat (dark/light) |
| Plots | **PyQtGraph** | MIT | Graphes interactifs, gros datasets, temps réel |
| Plots export | **Matplotlib** | BSD | Export publication (compatible LaTeX/pgf) |
| Calcul | **numpy / scipy / pandas** | BSD | Traitement vectorisé |
| Calcul (fallback) | **numba** | BSD | JIT pour boucles non-vectorisables |
| Format projet | **HDF5** (`h5py`) | BSD | Données + métadonnées, un seul fichier |
| Scripting | Console Python embarquée (qtconsole) | — | Même API que l'UI, pas de langage séparé |
| Packaging | **Briefcase** (BeeWare) | BSD | Build exe / dmg / AppImage depuis un seul projet |

**Verdict licences** : stack 100% compatible vente closed-source sans royalties. Seul point historiquement bloquant (QFluentWidgets, GPLv3) a été écarté au profit de qt-material (MIT).

---

## 2. Architecture des fenêtres

- **`QMainWindow`** : coquille principale (menu, toolbar, statusbar)
- **`QMdiArea`** : worksheets/graphes en sous-fenêtres façon Origin (mode libre ou onglets)
- **`QDockWidget`** : panneaux ancrables (explorateur de projet, propriétés, console de script)
- Fenêtres top-level indépendantes possibles pour détacher un graphe sur un second écran

---

## 3. Undo/Redo — `QUndoStack`

- Command Pattern natif Qt : chaque action = une classe `QUndoCommand` (`redo()` / `undo()`)
- Raccourcis clavier gérés nativement et cross-platform via `QKeySequence.Undo` / `.Redo`
- **Point clé** : faire passer les appels de la console Python par les mêmes `QUndoCommand` que l'UI → tout script devient annulable gratuitement
- **Gestion mémoire sur gros datasets** :
  - Préférer undo par delta/paramètres (fonction inverse) plutôt que snapshot complet
  - Copy-on-write sur les arrays numpy non modifiés
  - `setUndoLimit(N)` pour plafonner l'historique
- Regrouper les opérations composites : `beginMacro()` / `endMacro()`
- Lier l'état "modifié" du document : `undo_stack.cleanChanged` → astérisque titre fenêtre / prompt de sauvegarde

---

## 4. Format de fichier (HDF5)

- Capture uniquement **l'état final** au moment du save (pas l'historique undo)
- Optionnel : autosave/recovery périodique vers fichier temporaire indépendant
- Optionnel (avancé) : journal d'opérations dans un groupe `/history` (nom, timestamp, params) pour la traçabilité scientifique — pas rejouable au Ctrl+Z, mais utile pour la reproductibilité

---

## 5. Drag & Drop

### Système → app
`setAcceptDrops(True)` + `dragEnterEvent`/`dropEvent` sur `mimeData().hasUrls()` — import de fichiers par glisser-déposer depuis l'explorateur.

### In-app (ex. worksheet → plot, entre fenêtres MDI)
- Ne jamais sérialiser les données dans le `QMimeData` — transporter une **référence** (registre d'objets + clé) puisque tout tourne dans le même process Python
- `QDrag` + `QMimeData` avec type MIME custom (`application/x-tonapp-columns`)
- Points d'attention :
  - Mode onglets MDI : gérer le changement d'onglet au survol prolongé
  - Distinguer la zone de dépose (axe X / axe Y / nouvelle série) via `dragMoveEvent`
  - Pousser une `QUndoCommand` depuis le `dropEvent` (pas d'action directe) pour rester annulable
- **Wayland (Fedora)** : tester tôt, Qt6.5+ nettement plus stable que Qt5 ; fallback `QT_QPA_PLATFORM=xcb` si besoin

---

## 6. Parsing de fichiers hétérogènes

**Architecture** : registre de parseurs pluggables (`FileParser` avec `can_parse()` scoré + `parse()`), pas d'import monolithique.

- **CSV/texte générique** : détection auto encodage (`chardet`), dialecte/délimiteur (`csv.Sniffer`), séparateur décimal, lignes de métadonnées à skip
- **Formats instrument** : JCAMP-DX via lib `jcamp` pour la spectro ; parseurs dédiés par signature de fichier pour formats propriétaires récurrents
- **UI** : wizard d'import avec preview (aperçu tableau + overrides manuels), jamais d'import "à l'aveugle"
- **Gros fichiers** : parsing en streaming/chunks (`pandas.read_csv(chunksize=...)`), pas de chargement complet en RAM
- **Robustesse** : retour de warnings structurés (`ImportResult` avec `data`, `metadata`, `warnings`) plutôt que des exceptions qui plantent l'import

---

## 7. Packaging & distribution

```toml
[tool.briefcase.app.tonlogiciel]
sources = ["src/tonlogiciel"]
requires = ["PySide6", "qt-material", "pyqtgraph", "numpy", "scipy", "pandas", "h5py"]
```

```bash
briefcase create && briefcase build && briefcase package
```

- **Windows** : MSI, signature Authenticode recommandée (évite l'alerte SmartScreen)
- **macOS** : .dmg, nécessite notarisation (compte Apple Developer) pour distribution large
- **Linux** : AppImage, pas de signature obligatoire, viser `manylinux_2_28`
- **CI recommandée dès le début du projet** : GitHub Actions, matrice `windows-latest / macos-latest / ubuntu-latest`, build à chaque push

---

## Points de vigilance transverses

1. Poids binaire final ~150-300 Mo (normal pour la catégorie, comparable à Origin lui-même)
2. Calcul lourd toujours déporté en `QThread`/`multiprocessing` pour ne jamais geler l'UI
3. Tester le packaging sur machine propre (pas la machine de dev) dès les premières itérations
4. Modèle de données worksheet orienté colonnes avec métadonnées par colonne (nom, unité, labels) — pattern central à répliquer, comme chez Origin
