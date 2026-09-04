# PlotYx

Logiciel d'analyse et de tracé de données scientifiques, façon OriginLab, écrit de zéro en Python.

## État

**Pré-alpha — phase 0.** Aucun code applicatif pour l'instant : le dépôt porte l'architecture et la roadmap.

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

## Plateformes

**Windows et Linux.** macOS est hors périmètre de release ; la cible reste compilable en CI mais n'est ni publiée ni supportée — voir la section « Politique macOS » de la roadmap.

## Documentation

- [`ROADMAP.md`](ROADMAP.md) — plan de développement en 13 phases, jalons, risques
- [`docs/architecture.md`](docs/architecture.md) — architecture technique de référence

## Principes non négociables

1. **Aucune mutation du modèle de données en dehors d'une `QUndoCommand`.** C'est ce qui rend la console de script annulable sans code supplémentaire.
2. **Le modèle de données ne dépend pas de Qt.** `plotyx/core/` et `plotyx/analysis/` sont testables sans UI.
3. **Le packaging se valide en continu**, dès la première version, pas à la fin.
4. **Aucun calcul lourd sur le thread UI.**
