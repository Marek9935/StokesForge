"""Traduit un ou plusieurs `ImportResult` en commandes du bus.

Seul point de passage entre la couche `plotyx.io` (sans Qt) et le modèle :
un import, qu'il porte sur un fichier ou sur un lot, tient dans **une**
entrée d'historique (`bus.macro()`), conformément à la roadmap. Rien ici ne
mute `Worksheet` directement — chaque colonne est ajoutée via
`AddColumnCommand`, donc le garde-fou de `plotyx.commands.guard` reste
satisfait.
"""

from __future__ import annotations

from pathlib import Path

from plotyx.commands import AddColumnCommand, CommandBus
from plotyx.core.column import Column
from plotyx.core.worksheet import Worksheet
from plotyx.io.result import ImportResult

__all__ = ["apply_import_result", "apply_import_results", "unique_column_name"]


def unique_column_name(sheet: Worksheet, proposed: str) -> str:
    """`proposed`, ou une variante suffixée si le nom est déjà pris dans `sheet`."""
    if proposed not in sheet.column_names():
        return proposed
    index = 2
    candidate = f"{proposed}_{index}"
    while candidate in sheet.column_names():
        index += 1
        candidate = f"{proposed}_{index}"
    return candidate


def apply_import_result(bus: CommandBus, sheet: Worksheet, result: ImportResult) -> list[Column]:
    """Ajoute les colonnes d'un `ImportResult` à `sheet`, une commande par colonne.

    N'ouvre **pas** de macro elle-même : à appeler à l'intérieur d'un
    `bus.macro()` existant (voir `apply_import_results`), pour qu'un import
    par lots de plusieurs fichiers reste une seule entrée d'historique.
    """
    added: list[Column] = []
    for column_import in result.columns:
        name = unique_column_name(sheet, column_import.name)
        column = Column(
            name,
            column_import.values,
            column_type=column_import.column_type,
            role=column_import.role,
            unit=column_import.unit,
            long_name=column_import.long_name,
            comment=column_import.comment,
        )
        bus.push(AddColumnCommand(sheet, column))
        added.append(column)
    return added


def apply_import_results(
    bus: CommandBus,
    sheet: Worksheet,
    results: dict[Path, ImportResult],
    *,
    macro_text: str = "Import",
) -> dict[Path, list[Column]]:
    """Applique plusieurs résultats d'import dans **une seule** macro."""
    added: dict[Path, list[Column]] = {}
    with bus.macro(macro_text):
        for path, result in results.items():
            added[path] = apply_import_result(bus, sheet, result)
    return added
