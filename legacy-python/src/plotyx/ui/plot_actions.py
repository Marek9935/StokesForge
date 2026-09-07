"""Création d'un graphe à partir d'une sélection de colonnes.

Le pont entre « l'utilisateur a sélectionné des colonnes dans une feuille »
et « un `Graph` porteur d'un `PlotSpec` existe dans le projet ». Isolé de
`MainWindow` pour être testable sans fenêtre, et réutilisable tel quel par
le drag & drop de la phase 7 (qui transportera exactement la même chose :
des **clés de registre**) puis par la console de la phase 9.

Tout tient dans **une seule entrée d'historique** (`bus.macro`) : créer le
graphe et lui ajouter ses séries est une action unique du point de vue de
l'utilisateur, un seul Ctrl+Z doit la défaire.

## Choix de l'abscisse

Dans l'ordre : une colonne de rôle X **dans la sélection**, sinon la
première colonne de rôle X de la feuille (`Worksheet.x_column`), sinon
aucune — la série retombe alors sur l'abscisse implicite (l'indice de
ligne), comme Origin. Les colonnes de rôle `X_ERROR` / `Y_ERROR` présentes
dans la sélection sont attachées comme barres d'erreur plutôt que tracées
pour elles-mêmes.
"""

from __future__ import annotations

from collections.abc import Sequence

from plotyx.commands import AddObjectCommand, CommandBus
from plotyx.commands.plot import add_series_command, current_spec, update_axis_command
from plotyx.core.column import Column, ColumnRole, ColumnType
from plotyx.core.nodes import Folder, Graph
from plotyx.core.project import Project
from plotyx.plot.edit import series_for_column
from plotyx.plot.spec import AxisSide, PlotType

__all__ = ["create_graph_from_columns", "plottable_columns"]


def plottable_columns(columns: Sequence[Column]) -> list[Column]:
    """Les colonnes réellement traçables : numériques, hors rôles X et erreur."""
    return [
        column
        for column in columns
        if column.column_type is not ColumnType.TEXT
        and column.role not in (ColumnRole.X, ColumnRole.X_ERROR, ColumnRole.Y_ERROR)
    ]


def _pick(columns: Sequence[Column], role: ColumnRole) -> Column | None:
    for column in columns:
        if column.role is role:
            return column
    return None


def create_graph_from_columns(
    project: Project,
    bus: CommandBus,
    columns: Sequence[Column],
    *,
    plot_type: PlotType = PlotType.LINE,
    folder: Folder | None = None,
    name: str | None = None,
) -> Graph:
    """Crée un graphe traçant `columns`, en une seule entrée d'historique.

    Lève `ValueError` si la sélection ne contient aucune colonne traçable —
    l'appelant (l'UI) le remonte à l'utilisateur plutôt que de créer un
    graphe vide qu'il n'a pas demandé.
    """
    targets = plottable_columns(columns)
    if not targets:
        raise ValueError("aucune colonne numérique traçable dans la sélection")

    x_column = _pick(columns, ColumnRole.X)
    if x_column is None:
        sheet = targets[0].parent
        x_column = sheet.x_column() if sheet is not None else None
    x_key = x_column.key if x_column is not None else None
    y_error = _pick(columns, ColumnRole.Y_ERROR)
    x_error = _pick(columns, ColumnRole.X_ERROR)

    graph = Graph(name or _next_graph_name(project))
    destination = folder if folder is not None else project.root
    with bus.macro(f"Nouveau graphe {graph.name}"):
        bus.push(AddObjectCommand(destination, graph))
        for column in targets:
            series = series_for_column(
                # Le spec est relu du graphe à chaque itération : la couleur
                # de palette suit ainsi le nombre de séries déjà ajoutées.
                current_spec(graph),
                y_key=column.key,
                x_key=x_key,
                plot_type=(
                    PlotType.ERROR_BAR
                    if (y_error is not None or x_error is not None)
                    else plot_type
                ),
                y_error_key=y_error.key if y_error is not None else None,
                x_error_key=x_error.key if x_error is not None else None,
            )
            bus.push(add_series_command(graph, series))
        _label_axes(bus, graph, x_column, targets)
    return graph


def _label_axes(
    bus: CommandBus, graph: Graph, x_column: Column | None, targets: Sequence[Column]
) -> None:
    """Titre les axes d'après les colonnes, comme le ferait Origin.

    Les libellés sont écrits **une fois**, à la création : ce sont ensuite
    des propriétés du graphe que l'utilisateur peut modifier, et qu'un
    renommage de colonne n'a pas à écraser derrière lui.
    """
    changes: list[tuple[AxisSide, dict[str, str]]] = []
    if x_column is not None:
        changes.append(
            (AxisSide.BOTTOM, {"label": x_column.long_name or x_column.name, "unit": x_column.unit})
        )
    first = targets[0]
    same_unit = all(column.unit == first.unit for column in targets)
    changes.append(
        (
            AxisSide.LEFT,
            {
                "label": (first.long_name or first.name) if len(targets) == 1 else "",
                "unit": first.unit if same_unit else "",
            },
        )
    )
    for side, fields in changes:
        if any(fields.values()):
            bus.push(update_axis_command(graph, 0, side, **fields))


def _next_graph_name(project: Project) -> str:
    index = 1
    while project.find_window(f"Graph{index}") is not None:
        index += 1
    return f"Graph{index}"
