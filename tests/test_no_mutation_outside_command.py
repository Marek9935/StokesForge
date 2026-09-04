"""Test anti-régression de la règle d'or : aucune mutation hors commande.

C'est le premier risque du tableau de la roadmap — « console qui contourne
le bus d'undo », impact « réécriture de la couche métier ». Le test doit donc
attraper les **vrais** contournements, pas seulement l'oubli évident.

Trois niveaux, du plus large au plus fin :

1. **Le jeu fermé des primitives.** `core/project.py` documente la liste
   exhaustive des mutations possibles du modèle. Chacune est appelée
   directement ici, sous garde-fou armé, et chacune doit lever. Le contrôle
   de complétude `test_the_primitive_table_matches_the_docstring` compare la
   table de test à celle du docstring : une primitive ajoutée en phase 3 et
   oubliée ici fera tomber le test, plutôt que de créer un angle mort.
2. **Les façades.** `Project.add/remove/rename/move` aiguillent vers ces
   primitives : elles doivent lever aussi, sans exception.
3. **Le contournement mémoire.** Une écriture directe dans `Column._values`
   n'émet aucune notification et échappe par construction au hook. Le test
   le constate explicitement — plutôt que de laisser croire à une couverture
   qu'on n'a pas — et montre le filet complémentaire : la comparaison d'état
   exhaustive de `tests/model_state.py`.

Note d'implémentation : le hook est appelé par la notification, donc *après*
la mutation. Une primitive gardée lève bien, mais son effet a déjà eu lieu ;
chaque primitive est donc essayée sur un projet neuf.
"""

from __future__ import annotations

import re
from pathlib import Path

import numpy as np
import pytest

import plotyx.core
from plotyx.commands import (
    AddColumnCommand,
    MutationOutsideCommandError,
    SetCellsCommand,
    mutation_guard,
)
from plotyx.core import Column, ColumnType, Folder, Graph, Project, Worksheet
from plotyx.plot import PlotSpec
from tests.model_state import assert_same_state, snapshot


def _fresh_project() -> Project:
    project = Project("Demo")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("A", [1.0, 2.0, np.nan, 4.0])
    sheet.add_column("B", [10.0, 20.0])
    book.add_sheet("Sheet2")
    project.add_folder("Autre")  # deux enfants à la racine : `move_child` a un effet
    project.add_graph("GraphA")  # phase 6 : `Graph.set_plot_spec` est une primitive
    return project


def _primitive_calls(project: Project) -> dict[str, object]:
    """`Classe.primitive` → appel direct de la primitive sur `project`."""
    root = project.root
    book = project.resolve_path("[Book1]")
    sheet = project.resolve_path("[Book1]Sheet1")
    column = sheet.column("A")
    graph = project.resolve_path("[GraphA]")
    return {
        "Column.set_values": lambda: column.set_values(0, [0.0]),
        "Column.set_length": lambda: column.set_length(1),
        "Column.insert_rows": lambda: column.insert_rows(0, 1),
        "Column.delete_rows": lambda: column.delete_rows(0, 1),
        "Column.set_metadata": lambda: column.set_metadata(unit="V"),
        "Column.set_column_type": lambda: column.set_column_type(ColumnType.TEXT),
        "Worksheet.insert_column": lambda: sheet.insert_column(0, Column("Neuve")),
        "Worksheet.remove_column": lambda: sheet.remove_column("B"),
        "Worksheet.move_column": lambda: sheet.move_column("B", 0),
        "Worksheet.rename_column": lambda: sheet.rename_column("B", "Bis"),
        "Worksheet.insert_rows": lambda: sheet.insert_rows(0, 1),
        "Worksheet.delete_rows": lambda: sheet.delete_rows(0, 1),
        "Workbook.insert_sheet": lambda: book.insert_sheet(0, Worksheet("Neuve")),
        "Workbook.remove_sheet": lambda: book.remove_sheet("Sheet2"),
        "Workbook.move_sheet": lambda: book.move_sheet("Sheet2", 0),
        "Workbook.rename_sheet": lambda: book.rename_sheet("Sheet2", "Autre"),
        "Folder.insert_child": lambda: root.insert_child(0, Graph("G1")),
        "Folder.remove_child": lambda: root.remove_child(book),
        "Folder.move_child": lambda: root.move_child(book, 1),
        "Folder.rename_child": lambda: root.rename_child(book, "Book9"),
        "Graph.set_plot_spec": lambda: graph.set_plot_spec(PlotSpec.empty()),
        "ModelObject._set_name": lambda: root._set_name("Racine"),
    }


def _documented_primitives() -> set[str]:
    """Le jeu fermé, tel qu'écrit dans le docstring de `core/project.py`."""
    source = Path(plotyx.core.project.__file__).read_text()
    block = source.split("se limite\nà ceci :", 1)[1].split("Chacune est réversible", 1)[0]
    documented: set[str] = set()
    for owner, body in re.findall(r"\* `(\w+)` : (.*?)(?=\n\* |\Z)", block, re.S):
        documented.update(f"{owner}.{name}" for name in re.findall(r"`(\w+)`", body))
    return documented


def test_the_primitive_table_matches_the_docstring():
    documented = _documented_primitives()
    assert len(documented) >= 20, "le jeu de primitives n'est plus lisible dans le docstring"
    assert set(_primitive_calls(_fresh_project())) == documented


@pytest.mark.parametrize("name", sorted(_primitive_calls(_fresh_project())))
def test_every_primitive_is_caught_by_the_guard(name):
    """Chaque mutation primitive, appelée hors commande, doit lever."""
    call = _primitive_calls(_fresh_project())[name]
    with mutation_guard(), pytest.raises(MutationOutsideCommandError):
        call()


@pytest.mark.parametrize(
    "call",
    [
        lambda p: p.add_workbook("Book2"),
        lambda p: p.add_folder("Sous"),
        lambda p: p.add_graph("Graph1"),
        lambda p: p.add_matrix("MBook1"),
        lambda p: p.add_note("Note1"),
        lambda p: p.remove(p.resolve_path("[Book1]")),
        lambda p: p.rename(p.resolve_path("[Book1]"), "Book9"),
        lambda p: p.move(p.resolve_path("[Book1]Sheet1!A"), p.resolve_path("[Book1]Sheet2")),
    ],
    ids=["workbook", "folder", "graph", "matrix", "note", "remove", "rename", "move"],
)
def test_the_project_facades_are_caught_too(call):
    project = _fresh_project()
    with mutation_guard(), pytest.raises(MutationOutsideCommandError):
        call(project)


def test_a_command_pushed_on_the_bus_passes(bus, sheet):
    with mutation_guard():
        bus.push(SetCellsCommand(sheet.column("A"), 0, [7.0]))
        bus.push(AddColumnCommand(sheet, Column("D", [1.0])))
        bus.undo()
        bus.redo()
    assert sheet.column("A").values[0] == 7.0


def test_a_macro_pushed_on_the_bus_passes(bus, sheet):
    with mutation_guard():
        with bus.macro("Import"):
            bus.push(AddColumnCommand(sheet, Column("D", [1.0])))
            bus.push(SetCellsCommand(sheet.column("A"), 0, [7.0]))
        bus.undo()
        bus.redo()


def test_undo_triggered_directly_on_the_qt_stack_passes(bus, sheet):
    """La phase 3 branchera `createUndoAction()` : le bus n'est pas sur le chemin."""
    bus.push(SetCellsCommand(sheet.column("A"), 0, [7.0]))
    with mutation_guard():
        bus.stack.undo()
        bus.stack.redo()


def test_non_strict_mode_collects_without_raising():
    project = _fresh_project()
    with mutation_guard(strict=False) as breaches:
        project.resolve_path("[Book1]Sheet1!A").set_values(0, [1.0])
        project.add_folder("Sous")
    assert len(breaches) == 2
    assert "mutation hors commande" in breaches[0]


def test_the_guard_is_off_by_default_and_restored_afterwards():
    project = _fresh_project()
    assert plotyx.core.get_mutation_hook() is None
    with mutation_guard():
        assert plotyx.core.get_mutation_hook() is not None
    assert plotyx.core.get_mutation_hook() is None
    project.add_folder("Sous")  # aucune erreur : le garde-fou est bien retiré


def test_the_hook_can_be_set_and_read_directly():
    seen = []
    previous = plotyx.core.set_mutation_hook(lambda src, event: seen.append(event))
    try:
        assert previous is None
        _fresh_project().add_folder("Sous")
        assert seen
    finally:
        plotyx.core.set_mutation_hook(None)


def test_a_direct_array_write_escapes_the_hook_but_not_the_state_check():
    """Limite assumée du hook, et filet qui la couvre.

    Écrire dans `Column._values` ne passe par aucune primitive et n'émet
    aucune notification : le hook ne peut pas le voir. La comparaison
    d'état, elle, le voit — c'est elle qui garantit le test aléatoire.
    """
    project = _fresh_project()
    column = project.resolve_path("[Book1]Sheet1!A")
    before = snapshot(project)
    with mutation_guard():
        column._values[0] = 99.0  # contournement délibéré, non détecté

    with pytest.raises(AssertionError, match="valeurs de la colonne"):
        assert_same_state(before, snapshot(project))


def test_the_read_only_view_makes_the_obvious_bypass_noisy():
    column = _fresh_project().resolve_path("[Book1]Sheet1!A")
    with pytest.raises(ValueError):
        column.values[0] = 99.0


def test_the_state_snapshot_detects_a_structural_bypass():
    project = _fresh_project()
    before = snapshot(project)
    Folder("Pirate")  # objet neuf hors arbre : n'affecte rien
    assert_same_state(before, snapshot(project))

    project.root._children.append(Folder("Pirate2"))  # contournement structurel
    with pytest.raises(AssertionError):
        assert_same_state(before, snapshot(project))


def test_nan_is_compared_as_a_value_not_as_a_float():
    """`nan != nan` : sans `assert_array_equal`, ce test serait toujours vert."""
    project = _fresh_project()
    column = project.resolve_path("[Book1]Sheet1!A")
    assert np.isnan(column.values[2])
    assert_same_state(snapshot(project), snapshot(project), "identité")
