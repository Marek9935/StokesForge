"""Aide-écriture UI au-dessus du bus : rien qui mute `core` directement.

## Lignes : macro par colonne, pas la primitive de feuille

`Worksheet.insert_rows`/`delete_rows` sont désormais des inverses strictes
et *pourraient* être enveloppées telles quelles par une seule commande de
feuille. La phase 2 a choisi une granularité plus fine côté noyau —
`test_a_sheet_wide_row_insert_is_a_macro_of_column_commands` — et l'UI
retient la même décision, pour deux raisons concrètes :

1. **Undo homogène.** `InsertRowsCommand`/`DeleteRowsCommand` existent déjà
   par colonne (paramètres seuls, aucun octet retenu) ; les réutiliser
   évite d'ajouter une commande de feuille qui ferait double emploi.
2. **La feuille ignore déjà les colonnes trop courtes** (`at < len(column)`) ;
   reproduire ce filtre ici donne exactement le même ensemble de colonnes
   touchées que `Worksheet.insert_rows`/`delete_rows`, macro pour macro.

Le coût est un peu de bruit dans l'historique (une entrée de macro plutôt
qu'une commande simple) — sans conséquence pour l'utilisateur, qui ne voit
que le texte de la macro.
"""

from __future__ import annotations

from plotyx.commands import CommandBus, DeleteRowsCommand, InsertRowsCommand
from plotyx.core.worksheet import Worksheet

__all__ = ["delete_rows_macro", "insert_rows_macro"]


def insert_rows_macro(
    bus: CommandBus, sheet: Worksheet, at: int, count: int = 1, text: str = "Insertion de lignes"
) -> None:
    with bus.macro(text):
        for column in sheet.columns:
            if at < len(column):
                bus.push(InsertRowsCommand(column, at, count))


def delete_rows_macro(
    bus: CommandBus,
    sheet: Worksheet,
    at: int,
    count: int = 1,
    text: str = "Suppression de lignes",
) -> None:
    with bus.macro(text):
        for column in sheet.columns:
            if at < len(column):
                bus.push(DeleteRowsCommand(column, at, count))
