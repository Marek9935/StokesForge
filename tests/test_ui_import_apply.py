"""`apply_import_result(s)` : traduction d'un `ImportResult` en commandes du bus."""

from __future__ import annotations

import numpy as np

from plotyx.commands.guard import mutation_guard
from plotyx.core.column import ColumnRole
from plotyx.io.result import ColumnImport, ImportResult
from plotyx.ui.import_apply import apply_import_result, apply_import_results, unique_column_name


def _result(names: list[str]) -> ImportResult:
    return ImportResult(
        columns=[
            ColumnImport(
                name=name,
                values=np.array([1.0, 2.0, 3.0]),
                role=ColumnRole.X if i == 0 else ColumnRole.Y,
            )
            for i, name in enumerate(names)
        ]
    )


def test_unique_column_name_returns_proposed_when_free(sheet):
    assert unique_column_name(sheet, "Zed") == "Zed"


def test_unique_column_name_suffixes_on_collision(sheet):
    # `sheet` (fixture conftest) contient déjà une colonne "A".
    assert unique_column_name(sheet, "A") == "A_2"


def test_apply_import_result_adds_columns_via_command(bus, sheet):
    before = bus.count
    result = _result(["Wavelength", "Absorbance"])
    added = apply_import_result(bus, sheet, result)
    assert len(added) == 2
    assert "Wavelength" in sheet.column_names()
    assert "Absorbance" in sheet.column_names()
    # Une commande par colonne, poussée directement (pas de macro ici).
    assert bus.count == before + 2


def test_apply_import_result_dedupes_column_names(bus, sheet):
    """`sheet` a déjà une colonne "A" — l'import ne doit pas planter dessus."""
    result = _result(["A", "B"])
    added = apply_import_result(bus, sheet, result)
    names = [c.name for c in added]
    assert names[0] != "A" or names[0] == "A_2"
    assert len(set(sheet.column_names())) == len(sheet.column_names())


def test_apply_import_results_is_a_single_undo_entry(bus, sheet):
    """Roadmap : un import (même multi-fichiers) tient en une seule entrée d'historique."""
    before = bus.count
    results = {
        "file1.csv": _result(["S1_X", "S1_Y"]),
        "file2.csv": _result(["S2_X", "S2_Y"]),
    }
    added = apply_import_results(bus, sheet, results, macro_text="Import de test")
    assert bus.count == before + 1  # une seule entrée, quel que soit le nombre de fichiers
    assert len(added) == 2
    for cols in added.values():
        for column in cols:
            assert column.name in sheet.column_names()

    bus.undo()
    for cols in added.values():
        for column in cols:
            assert column.name not in sheet.column_names()

    bus.redo()
    for cols in added.values():
        for column in cols:
            assert column.name in sheet.column_names()


def test_apply_import_never_mutates_outside_a_command(bus, sheet):
    """Le garde-fou de la phase 2 doit rester satisfait par l'import."""
    result = _result(["GuardX", "GuardY"])
    with mutation_guard():
        apply_import_result(bus, sheet, result)
    assert "GuardX" in sheet.column_names()


def test_apply_import_results_macro_never_mutates_outside_a_command(bus, sheet):
    results = {"f.csv": _result(["MacroX", "MacroY"])}
    with mutation_guard():
        apply_import_results(bus, sheet, results)
    assert "MacroX" in sheet.column_names()
