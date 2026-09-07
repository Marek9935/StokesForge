"""Photographie exhaustive de l'état d'un projet, pour les tests d'égalité.

Sert à deux tests de la phase 2 :

* le test aléatoire (N commandes, tout annuler, comparer à l'état initial) ;
* le test anti-régression, où toute divergence d'état non expliquée par une
  commande est une infraction — y compris celles qui échappent au hook de
  mutation, comme une écriture directe dans `Column._values`.

Deux principes :

* **l'identité compte** — les objets sont repérés par leur clé de registre,
  pas par leur nom ni leur position. Une annulation qui remettrait une
  *copie* de la colonne supprimée, avec une clé neuve, serait détectée ;
* **`NaN` est une valeur comme une autre** — la comparaison passe par
  `np.testing.assert_array_equal`, qui traite `nan` comme égal à `nan`. Un
  `==` naïf ferait passer ce test pour toujours vert.
"""

from __future__ import annotations

from typing import Any

import numpy as np

from plotyx.core import Column, ModelObject, Project

__all__ = ["assert_same_state", "columns_of", "snapshot"]


def columns_of(project: Project) -> list[Column]:
    return [node for node in project.walk() if isinstance(node, Column)]


def _index_in_parent(node: ModelObject) -> int | None:
    parent = node.parent
    if parent is None:
        return None
    for index, child in enumerate(parent.children()):
        if child is node:
            return index
    return -1  # pragma: no cover - incohérence d'arbre, jamais atteint


def snapshot(project: Project) -> dict[str, Any]:
    """État complet : arbre, métadonnées, valeurs, contenu du registre."""
    tree = [
        (
            node.key,
            type(node).__name__,
            node.name,
            node.parent.key if node.parent is not None else None,
            _index_in_parent(node),
        )
        for node in project.walk()
    ]
    values: dict[str, np.ndarray] = {}
    meta: dict[str, tuple[Any, ...]] = {}
    for column in columns_of(project):
        values[column.key] = np.array(column.values)
        meta[column.key] = (
            str(column.column_type),
            str(column.role),
            column.unit,
            column.long_name,
            column.comment,
            column.display_format,
        )
    return {
        "tree": tree,
        "meta": meta,
        "values": values,
        "registry": sorted(project.registry.keys()),
    }


def assert_same_state(first: dict[str, Any], second: dict[str, Any], context: str = "") -> None:
    """Égalité stricte de deux photographies, `NaN` compris."""
    prefix = f"{context} : " if context else ""
    assert first["tree"] == second["tree"], f"{prefix}arbre différent"
    assert first["meta"] == second["meta"], f"{prefix}métadonnées différentes"
    assert first["registry"] == second["registry"], f"{prefix}registre différent"
    assert first["values"].keys() == second["values"].keys(), f"{prefix}colonnes différentes"
    for key, array in first["values"].items():
        np.testing.assert_array_equal(
            array, second["values"][key], err_msg=f"{prefix}valeurs de la colonne {key}"
        )
