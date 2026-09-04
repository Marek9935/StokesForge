# PlotYx

Logiciel d'analyse et de tracé de données scientifiques, façon OriginLab, écrit de zéro en Python.

## État

**Pré-alpha — phase 3.** Le squelette applicatif (fenêtre, packaging, CI), le
**modèle de données**, le **bus de commandes annulables** et l'**UI
worksheet** sont en place : saisie/collage dans une table, undo/redo,
explorateur de projet.

`plotyx.core` fournit `Project` / `Folder` / `Workbook` / `Worksheet` / `Column`
(plus les nœuds minimaux `Graph`, `Matrix`, `Note`), la résolution de chemins
`[Book1]Sheet1!ColA`, un registre de clés stables et un mécanisme de
notification en Python pur.

`plotyx.commands` fournit le `CommandBus` (une `QUndoStack` par projet, limite
d'historique, macros, état « document modifié ») et les commandes qui
enveloppent les primitives de mutation du modèle. C'est le seul des trois
paquets ci-dessous qui importait Qt jusqu'à cette phase : `core` reste pur.

`plotyx.ui` adapte ces deux couches en widgets Qt : un `QAbstractTableModel`
sur `Worksheet` (en-têtes riches nom/unité/label, aucune copie des données),
une `QTableView` avec copier/coller et menus contextuels, un dialogue
« Column Properties », et un `QAbstractItemModel` façon explorateur de
fichiers sur `Project` pour le dock « Project Explorer ». Toute écriture y
passe par une commande du bus — jamais par une primitive de `core` en
direct. Prochaine étape : la persistance HDF5 (phase 4).

## Stack

| Couche | Techno |
|---|---|
| UI | PySide6 + qt-material |
| Plots interactifs | PyQtGraph |
| Export publication | Matplotlib (PNG/SVG/PDF/pgf) |
| Calcul | numpy · scipy · pandas (numba si profilage le justifie) |
| Format projet | HDF5 (`h5py`) |
| Scripting | console Python embarquée (qtconsole) |
| Packaging | Briefcase |

Licences 100 % compatibles avec une distribution closed-source, sans royalties.

## Dépôt

Public, mais **pas open source** : la licence est propriétaire (« tous droits réservés »), le code est lisible sans qu'aucun droit ne soit cédé. CI/CD entièrement sur GitLab — voir la section « Hébergement et CI » de la roadmap.

## Plateformes

**Windows et Linux.** macOS est hors périmètre de release ; la cible reste compilable en CI mais n'est ni publiée ni supportée — voir la section « Politique macOS » de la roadmap.

Sous Linux, `xcb` est forcé par défaut en session Wayland : un `QDockWidget` détaché y est immobile, Wayland refusant qu'une application positionne ses propres fenêtres. Forcer `QT_QPA_PLATFORM=wayland` reste possible. Détails et condition de retour dans la roadmap, « Wayland et géométrie des fenêtres ».

## Documentation

- [`ROADMAP.md`](ROADMAP.md) — plan de développement en 13 phases, jalons, risques
- [`docs/architecture.md`](docs/architecture.md) — architecture technique de référence

## Principes non négociables

1. **Aucune mutation du modèle de données en dehors d'une `QUndoCommand`.** C'est ce qui rend la console de script annulable sans code supplémentaire. Vérifié automatiquement : `plotyx.core.guard` expose un hook de mutation (désactivé par défaut, coût nul en production) que `plotyx.commands.mutation_guard` arme pour faire échouer toute mutation hors commande — voir `tests/test_no_mutation_outside_command.py`.
2. **Le modèle de données ne dépend pas de Qt.** `plotyx/core/` et `plotyx/analysis/` sont testables sans UI. Le modèle émet des signaux Python purs (`plotyx.core.signals`) que la couche `ui/` adapte en signaux Qt ; `tests/test_core_no_qt.py` vérifie automatiquement cette frontière.
3. **Le packaging se valide en continu**, dès la première version, pas à la fin.
4. **Aucun calcul lourd sur le thread UI.**
