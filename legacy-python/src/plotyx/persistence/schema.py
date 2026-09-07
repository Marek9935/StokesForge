"""Constantes du schéma HDF5 `/project` — la partie qui ne bouge pas souvent.

## Arborescence retenue

```
/ (fichier)
  attrs: plotyx_format = "plotyx-project"
  attrs: plotyx_schema_version = 1            # <- premier octet écrit, non négociable
  attrs: plotyx_autosave_source = "..."       # présent seulement sur un autosave

  /project                                     # = Project.root (un Folder)
    attrs: key, name, type="folder"
    attrs: children_order = [nom, ...]         # ordre des enfants, indépendant
                                                # de l'ordre d'itération HDF5
    /<nom d'enfant>                            # un sous-groupe par enfant
      # Folder imbriqué : mêmes attrs que ci-dessus, récursif
      # Workbook :
      attrs: key, name, type="workbook", label, comment, metadata_json
      attrs: children_order = [nom de feuille, ...]
      /<nom de feuille>                        # Worksheet
        attrs: key, name, type="worksheet"
        attrs: children_order = [nom de colonne, ...]
        /<nom de colonne>                      # Column : voir plus bas
      # Graph | Matrix | Note :
      attrs: key, name, type="graph"|"matrix"|"note", label, comment,
             metadata_json, text (Note seulement)
      attrs: plot_spec_json   # Graph uniquement, schéma >= 2 : le PlotSpec
                              # sérialisé (voir plotyx.plot.spec). Absent
                              # sur un graphe encore vide, et sur tout
                              # fichier de schéma 1.

  /history                                     # RÉSERVÉ, phase 4 ne le crée
                                                # pas — voir docs/architecture.md §4
```

## Colonnes

Une colonne numérique ou datetime est un **dataset** `float64` (NaN = valeur
manquante, comme dans `plotyx.core.column`) portant ses métadonnées en
attributs HDF5. Une colonne texte est un **groupe** de deux datasets :
`values` (chaînes UTF-8 de longueur variable, "" pour un `None`) et
`missing` (booléens, la marque exacte des `None`) — une colonne texte peut
légitimement contenir une chaîne vide *distincte* d'une valeur manquante, ce
qu'un simple dataset de chaînes ne pourrait pas coder sans ambiguïté.

Attributs communs (dataset ou groupe) : `key`, `name`, `type="column"`,
`column_type`, `role`, `unit`, `long_name`, `comment`, `display_format`.

Jamais de table rectangulaire : chaque colonne porte sa propre longueur
(la forme du dataset), sans padding — voir la décision de la phase 1.
"""

from __future__ import annotations

__all__ = [
    "ATTR_AUTOSAVE_SOURCE",
    "ATTR_COLUMN_TYPE",
    "ATTR_COMMENT",
    "ATTR_DISPLAY_FORMAT",
    "ATTR_FORMAT",
    "ATTR_KEY",
    "ATTR_LABEL",
    "ATTR_LONG_NAME",
    "ATTR_METADATA_JSON",
    "ATTR_NAME",
    "ATTR_PLOT_SPEC_JSON",
    "ATTR_ORDER",
    "ATTR_ROLE",
    "ATTR_SCHEMA_VERSION",
    "ATTR_TEXT",
    "ATTR_TYPE",
    "ATTR_UNIT",
    "CURRENT_SCHEMA_VERSION",
    "DATASET_MISSING",
    "DATASET_VALUES",
    "FORMAT_MARKER",
    "GROUP_HISTORY",
    "GROUP_PROJECT",
    "TYPE_COLUMN",
    "TYPE_FOLDER",
    "TYPE_GRAPH",
    "TYPE_MATRIX",
    "TYPE_NOTE",
    "TYPE_WORKBOOK",
    "TYPE_WORKSHEET",
    "WINDOW_TYPES",
]

#: Premier octet écrit dans tout fichier `.plx` — non négociable (roadmap
#: phase 4). Toute évolution du schéma incrémente ce nombre et ajoute une
#: étape dans `plotyx.persistence.migration.migrate`.
#: Schéma 2 (phase 6) : ajout de l'attribut `plot_spec_json` sur les
#: groupes de type `graph`. Un fichier de schéma 1 reste lisible tel quel —
#: ses graphes sont simplement vides — d'où une migration qui n'a rien à
#: réécrire, voir `plotyx.persistence.migration`.
CURRENT_SCHEMA_VERSION = 2

#: Extension de fichier officielle des projets PlotYx (décision 2026-09-04) :
#: `.pyx` collisionne avec l'extension Cython, `.plx` est libre.
FILE_EXTENSION = ".plx"

FORMAT_MARKER = "plotyx-project"

# -- attributs du fichier ---------------------------------------------------

ATTR_FORMAT = "plotyx_format"
ATTR_SCHEMA_VERSION = "plotyx_schema_version"
ATTR_AUTOSAVE_SOURCE = "plotyx_autosave_source"

# -- groupes de premier niveau -----------------------------------------------

GROUP_PROJECT = "project"
#: Réservé par l'architecture (§4) : journal d'opérations, hors périmètre de
#: cette phase. On ne l'écrit pas, mais le nom est fixé pour ne pas entrer
#: en collision avec un enfant de projet nommé "history" (impossible de
#: toute façon : `history` n'est stocké qu'à la racine du fichier).
GROUP_HISTORY = "history"

# -- attributs communs aux nœuds ---------------------------------------------

ATTR_KEY = "key"
ATTR_NAME = "name"
ATTR_TYPE = "type"
ATTR_ORDER = "children_order"
ATTR_LABEL = "label"
ATTR_COMMENT = "comment"
ATTR_METADATA_JSON = "metadata_json"
ATTR_TEXT = "text"  # Note.text
#: `Graph.plot_spec` sérialisé en JSON (schéma 2). Un seul attribut plutôt
#: qu'une arborescence de groupes : le spec ne contient aucune donnée, il
#: pèse quelques kilo-octets au plus, et le garder opaque pour HDF5 évite
#: d'avoir à faire évoluer le schéma du fichier à chaque nouvelle option de
#: style — c'est `PlotSpec.from_dict` qui ignore les champs inconnus.
ATTR_PLOT_SPEC_JSON = "plot_spec_json"

# -- attributs de colonne -----------------------------------------------------

ATTR_COLUMN_TYPE = "column_type"
ATTR_ROLE = "role"
ATTR_UNIT = "unit"
ATTR_LONG_NAME = "long_name"
ATTR_DISPLAY_FORMAT = "display_format"

DATASET_VALUES = "values"
DATASET_MISSING = "missing"

# -- valeurs de `type` --------------------------------------------------------

TYPE_FOLDER = "folder"
TYPE_WORKBOOK = "workbook"
TYPE_WORKSHEET = "worksheet"
TYPE_GRAPH = "graph"
TYPE_MATRIX = "matrix"
TYPE_NOTE = "note"
TYPE_COLUMN = "column"

WINDOW_TYPES = (TYPE_WORKBOOK, TYPE_GRAPH, TYPE_MATRIX, TYPE_NOTE)
