"""Sortie de phase 2 : N commandes aléatoires, tout annuler, tout refaire.

La roadmap demande « un script de test qui applique 50 commandes aléatoires,
undo tout, et vérifie l'égalité bit à bit avec l'état initial ». Ce fichier
l'implémente, avec quatre précautions qui font la différence entre un test
vert par chance et un test qui trouve des bugs :

* **graine fixe** — reproductible ; un échec se rejoue à l'identique ;
* **`NaN` traité comme une valeur** — `nan != nan`, donc la comparaison
  passe par `np.testing.assert_array_equal` (voir `tests/model_state.py`).
  Un `==` naïf rendrait ce test toujours vert, et les données scientifiques
  sont pleines de `NaN` par décision de la phase 1 ;
* **identité, pas ressemblance** — l'état comparé indexe les colonnes par
  leur clé de registre. Une annulation qui remettrait une *copie* de la
  colonne supprimée serait détectée ;
* **le redo aussi** — annuler puis refaire doit ramener exactement l'état
  final. C'est ce qui attrape les commandes dont `_apply` n'est pas
  idempotent après un `_revert`.

Le garde-fou de mutation est armé pendant tout le tirage : une commande qui
muterait le modèle hors du contexte d'exécution ferait tomber ce test aussi.
"""

from __future__ import annotations

import random

import numpy as np
import pytest

from plotyx.commands import (
    AddColumnCommand,
    CommandBus,
    DeleteColumnCommand,
    DeleteRowsCommand,
    InsertRowsCommand,
    MoveCommand,
    RenameCommand,
    SetCellsCommand,
    SetColumnLengthCommand,
    SetColumnMetadataCommand,
    SetColumnTypeCommand,
    mutation_guard,
)
from plotyx.core import Column, ColumnRole, ColumnType, PlotyxError, Project
from tests.model_state import assert_same_state, columns_of, snapshot

#: Un tirage peut proposer une opération invalide sur l'état courant (nom déjà
#: pris, colonne entre-temps déplacée). C'est un rejet de tirage, pas un bug :
#: on repioche. Toute autre exception fait tomber le test.
INVALID_DRAW = (PlotyxError, KeyError, IndexError)

SEED = 20260904
OPERATIONS = 50


def _build_project() -> Project:
    project = Project("Aléatoire")
    book = project.add_workbook("Book1")
    first = book.sheet("Sheet1")
    first.add_column("A", [1.0, 2.0, np.nan, 4.0, 5.0], role=ColumnRole.X, unit="s")
    first.add_column("B", [np.nan, 20.0, 30.0])
    first.add_column("T", ["a", None, "c"], column_type=ColumnType.TEXT)
    second = book.add_sheet("Sheet2")
    second.add_column("X", [0.5, 1.5])
    return project


def _random_values(rng: random.Random, count: int) -> list[float]:
    """Des flottants, et des `NaN` en quantité — c'est là que ça casse."""
    return [
        np.nan if rng.random() < 0.3 else round(rng.uniform(-100, 100), 6)
        for _ in range(count)
    ]


def _make_command(rng: random.Random, project: Project, counter: list[int]):
    """Tire une commande valide sur l'état courant du projet."""
    sheets = [project.resolve_path("[Book1]Sheet1"), project.resolve_path("[Book1]Sheet2")]
    numeric = [c for c in columns_of(project) if c.column_type is not ColumnType.TEXT]
    every = columns_of(project)
    choices = [
        "set_cells",
        "set_length",
        "insert_rows",
        "delete_rows",
        "metadata",
        "add_column",
        "delete_column",
        "rename",
        "move",
        "column_type",
    ]
    kind = rng.choice(choices)

    if kind == "set_cells" and numeric:
        column = rng.choice(numeric)
        start = rng.randrange(0, max(len(column), 1) + 2)
        return SetCellsCommand(column, start, _random_values(rng, rng.randint(1, 4)))
    if kind == "set_length" and every:
        column = rng.choice(every)
        return SetColumnLengthCommand(column, rng.randrange(0, 8))
    if kind == "insert_rows" and every:
        column = rng.choice(every)
        return InsertRowsCommand(column, rng.randrange(0, len(column) + 1), rng.randint(1, 3))
    if kind == "delete_rows" and every:
        column = rng.choice(every)
        if len(column):
            return DeleteRowsCommand(column, rng.randrange(0, len(column)), rng.randint(1, 3))
        return None
    if kind == "metadata" and every:
        column = rng.choice(every)
        return SetColumnMetadataCommand(
            column,
            unit=rng.choice(["", "s", "V", "m/s"]),
            role=rng.choice(list(ColumnRole)),
            comment=f"c{counter[0]}",
        )
    if kind == "add_column":
        sheet = rng.choice(sheets)
        counter[0] += 1
        column = Column(f"N{counter[0]}", _random_values(rng, rng.randint(0, 5)))
        return AddColumnCommand(sheet, column, rng.randrange(0, len(sheet) + 1))
    if kind == "delete_column":
        sheet = rng.choice(sheets)
        if len(sheet):
            return DeleteColumnCommand(sheet, rng.randrange(0, len(sheet)))
        return None
    if kind == "rename" and every:
        counter[0] += 1
        return RenameCommand(rng.choice(every), f"R{counter[0]}")
    if kind == "move" and every:
        column = rng.choice(every)
        target = rng.choice(sheets)
        if target is column.parent:
            return MoveCommand(column, index=rng.randrange(0, len(target)))
        return MoveCommand(column, target, rng.randrange(0, len(target) + 1))
    if kind == "column_type" and every:
        column = rng.choice(every)
        return SetColumnTypeCommand(column, rng.choice(list(ColumnType)))
    return None


class _EmptyDraw(Exception):
    """Tirage sans commande applicable : abandonne la macro en cours."""


def _apply_random_operations(bus: CommandBus, rng: random.Random, count: int) -> int:
    """Empile `count` entrées d'historique, dont quelques macros."""
    counter = [0]
    applied = 0
    guard = 0
    while applied < count:
        guard += 1
        if guard > count * 20:  # pragma: no cover - filet anti-boucle infinie
            pytest.fail("le tirage ne produit plus de commandes valides")
        if rng.random() < 0.15:
            size = rng.randint(2, 3)
            try:
                # Les commandes sont construites *dans* la macro : chacune doit
                # voir l'état laissé par la précédente.
                with bus.macro(f"Macro {applied}"):
                    for _ in range(size):
                        command = _make_command(rng, bus.project, counter)
                        if command is None:
                            raise _EmptyDraw
                        bus.push(command)
            except (*INVALID_DRAW, _EmptyDraw):
                continue  # la macro partielle a été annulée et effacée
            applied += 1
            continue
        command = _make_command(rng, bus.project, counter)
        if command is None:
            continue
        try:
            bus.push(command)
        except INVALID_DRAW:
            continue
        applied += 1
    return applied


def test_fifty_random_commands_undo_to_the_initial_state(qapp):
    rng = random.Random(SEED)
    project = _build_project()
    bus = CommandBus(project)  # historique illimité : on doit pouvoir tout annuler
    initial = snapshot(project)

    with mutation_guard():
        assert _apply_random_operations(bus, rng, OPERATIONS) == OPERATIONS
        assert bus.count == OPERATIONS
        final = snapshot(project)
        assert final != initial, "le tirage n'a rien modifié : le test ne prouve rien"

        while bus.can_undo:
            bus.undo()
        assert_same_state(initial, snapshot(project), "après annulation complète")

        while bus.can_redo:
            bus.redo()
        assert_same_state(final, snapshot(project), "après rétablissement complet")


def test_the_random_run_is_reproducible(qapp):
    """Même graine, même historique — sinon un échec ne se rejoue pas."""
    texts = []
    for _ in range(2):
        project = _build_project()
        bus = CommandBus(project)
        _apply_random_operations(bus, random.Random(SEED), 12)
        texts.append([bus.stack.command(i).text() for i in range(bus.count)])
    assert texts[0] == texts[1]


def test_random_commands_survive_a_bounded_history(qapp):
    """Avec `setUndoLimit`, on n'annule que le sommet — sans rien casser."""
    rng = random.Random(SEED + 1)
    project = _build_project()
    bus = CommandBus(project, undo_limit=5)
    _apply_random_operations(bus, rng, 30)

    assert bus.count == 5
    reference = snapshot(project)
    for _ in range(3):
        bus.undo()
    for _ in range(3):
        bus.redo()
    assert_same_state(reference, snapshot(project), "aller-retour partiel")
