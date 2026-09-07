# PlotYx

Logiciel d'analyse et de tracé de données scientifiques, façon OriginLab, écrit de zéro en Python.

## État

**Pré-alpha — phase 6.** Le squelette applicatif (fenêtre, packaging, CI), le
**modèle de données**, le **bus de commandes annulables**, l'**UI worksheet**,
la **persistance HDF5**, l'**import de fichiers** et les **graphes
Matplotlib** sont en place : saisie et collage dans une table, undo/redo,
explorateur de projet, projets `.plx` avec autosave et récupération, import
par registre de parseurs avec aperçu, et tracé interactif (nuage, ligne,
points+ligne, barres, histogramme, barres d'erreur) en sous-fenêtre MDI,
avec multi-panneaux, axes secondaires et légende. La barre d'outils est
celle de Matplotlib — Home, historique des vues, déplacement, zoom
rectangle, éditeur d'axes, enregistrement de la figure — augmentée d'un
bouton « Curseurs » ; la molette zoome et le glissé déplace sans avoir à
armer d'outil.

Un second moteur, **PyQtGraph**, reste accessible comme « visualiseur
léger » — minimal, destiné à l'acquisition live, jamais enregistré dans le
projet. Il était le rendu par défaut jusqu'au 2026-09-07 ; le changement et
ses raisons sont dans
[`docs/decision-rendu-matplotlib-2026-09-07.md`](docs/decision-rendu-matplotlib-2026-09-07.md).

`plotyx.core` fournit `Project` / `Folder` / `Workbook` / `Worksheet` / `Column`
(plus les nœuds minimaux `Graph`, `Matrix`, `Note`), la résolution de chemins
`[Book1]Sheet1!ColA`, un registre de clés stables et un mécanisme de
notification en Python pur.

`plotyx.commands` fournit le `CommandBus` (une `QUndoStack` par projet, limite
d'historique, macros, état « document modifié ») et les commandes qui
enveloppent les primitives de mutation du modèle. C'est le seul des trois
paquets ci-dessous qui importait Qt jusqu'à cette phase : `core` reste pur.

`plotyx.plot` décrit un graphe **sans connaître aucun moteur de rendu** :
`PlotSpec` / `PanelSpec` / `SeriesSpec` / `AxisSpec` sont des dataclasses
immuables de types standard, où une série désigne ses colonnes par **clé de
registre** et jamais par copie des données. Deux renderers la peignent —
Matplotlib (`plotyx.ui.mpl_renderer`, le défaut) et PyQtGraph
(`plotyx.ui.plot_renderer`, le visualiseur léger) — sans que la description
connaisse ni l'un ni l'autre. Basculer le rendu par défaut de l'un à
l'autre n'a demandé aucune modification de `plotyx.plot` : c'est ce que
cette couche est là pour garantir. `tests/test_plot_spec_is_engine_neutral.py`
vérifie qu'un `PlotSpec` se construit et s'inspecte dans un interpréteur où
Qt, PyQtGraph et Matplotlib sont introuvables.

`plotyx.ui` adapte ces deux couches en widgets Qt : un `QAbstractTableModel`
sur `Worksheet` (en-têtes riches nom/unité/label, aucune copie des données),
une `QTableView` avec copier/coller et menus contextuels, un dialogue
« Column Properties », et un `QAbstractItemModel` façon explorateur de
fichiers sur `Project` pour le dock « Project Explorer ». Toute écriture y
passe par une commande du bus — jamais par une primitive de `core` en
direct. Depuis la phase 6, le dock « Properties » édite le graphe actif, et
chaque style, échelle ou limite d'axe est une commande annulable.
Prochaine étape : le glisser-déposer interne (phase 7).

## Stack

| Couche | Techno |
|---|---|
| UI | PySide6 + qt-material |
| Plots interactifs | Matplotlib (défaut) · PyQtGraph (visualiseur léger) |
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

- [`docs/etat-du-projet.md`](docs/etat-du-projet.md) — **où en est le code et ce qui a été tranché**, avec les raisons
- [`ROADMAP.md`](ROADMAP.md) — plan de développement en 13 phases, jalons, risques
- [`docs/architecture.md`](docs/architecture.md) — architecture technique de référence

## Principes non négociables

1. **Aucune mutation du modèle de données en dehors d'une `QUndoCommand`.** C'est ce qui rend la console de script annulable sans code supplémentaire. Vérifié automatiquement : `plotyx.core.guard` expose un hook de mutation (désactivé par défaut, coût nul en production) que `plotyx.commands.mutation_guard` arme pour faire échouer toute mutation hors commande — voir `tests/test_no_mutation_outside_command.py`.
2. **Le modèle de données ne dépend pas de Qt.** `plotyx/core/` et `plotyx/analysis/` sont testables sans UI. Le modèle émet des signaux Python purs (`plotyx.core.signals`) que la couche `ui/` adapte en signaux Qt ; `tests/test_core_no_qt.py` vérifie automatiquement cette frontière.
3. **Le packaging se valide en continu**, dès la première version, pas à la fin.
4. **Aucun calcul lourd sur le thread UI.**
