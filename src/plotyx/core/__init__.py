"""Cœur métier de PlotYx : modèle de données colonnaire.

Principe non négociable (voir README) : **aucun import de Qt ici**, ni dans
aucun sous-module de `plotyx.core`. Les notifications du modèle passent par
un mécanisme d'observateur en Python pur (`plotyx.core.signals`) ; la couche
`plotyx.ui` fera l'adaptation vers les signaux Qt. Le test
`tests/test_core_no_qt.py` vérifie automatiquement cette frontière.

Phase 1 — arbre du modèle :

```
Project
└── Folder (racine)
    ├── Folder ...
    ├── Workbook ─── Worksheet ─── Column
    ├── Graph        \\
    ├── Matrix        > nœuds minimaux, contenu en phases 6+
    └── Note         /
```
"""

from plotyx.core.column import Column, ColumnRole, ColumnType, ValuesDelta
from plotyx.core.errors import (
    DuplicateNameError,
    InvalidNameError,
    PathError,
    PlotyxError,
    StructureError,
    UnknownKeyError,
)
from plotyx.core.node import DataChanged, ModelObject, StructureChanged
from plotyx.core.nodes import Folder, Graph, Matrix, Note, WindowNode, Workbook
from plotyx.core.paths import ObjectPath, format_path, parse_path
from plotyx.core.project import Project
from plotyx.core.registry import ObjectRegistry
from plotyx.core.signals import Connection, Signal
from plotyx.core.worksheet import Worksheet

__all__ = [
    "Column",
    "ColumnRole",
    "ColumnType",
    "Connection",
    "DataChanged",
    "DuplicateNameError",
    "Folder",
    "Graph",
    "InvalidNameError",
    "Matrix",
    "ModelObject",
    "Note",
    "ObjectPath",
    "ObjectRegistry",
    "PathError",
    "PlotyxError",
    "Project",
    "Signal",
    "StructureChanged",
    "StructureError",
    "UnknownKeyError",
    "ValuesDelta",
    "WindowNode",
    "Workbook",
    "Worksheet",
    "format_path",
    "parse_path",
]
