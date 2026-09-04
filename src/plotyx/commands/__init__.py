"""Bus de commandes annulables — phase 2.

**Pourquoi un paquet séparé de `plotyx.core`.** `QUndoStack` et
`QUndoCommand` sont des classes Qt, et le README interdit tout import de Qt
sous `plotyx/core/` (principe non négociable, vérifié par
`tests/test_core_no_qt.py`). La couche d'undo vit donc ici : elle a le droit
d'importer Qt, `core` reste pur, et la dépendance ne va que dans un sens —
`plotyx.commands` connaît `plotyx.core`, jamais l'inverse.

**Ce que la couche enveloppe.** Les *primitives* de mutation documentées en
tête de `core/project.py`, pas les façades `Project.add/remove/rename/move`.
Chaque primitive retourne son delta d'inversion et émet exactement une
notification ; c'est ce contrat qui rend les commandes réversibles sans
snapshot.

**Stratégie mémoire.** Delta ou paramètres d'abord ; une seule commande
recourt au snapshot, `SetColumnTypeCommand`, et le documente. Les objets
supprimés ne sont pas copiés mais *retenus* par la commande, qui les lâche
quand elle sort de la pile.

Exemple :

```python
project = Project()
book = project.add_workbook("Book1")
sheet = book.sheet("Sheet1")
bus = CommandBus(project, undo_limit=100)

bus.push(AddColumnCommand(sheet, Column("A", [1.0, 2.0, 3.0])))
with bus.macro("Import CSV"):          # une seule entrée d'historique
    bus.push(AddColumnCommand(sheet, Column("B", [4.0, 5.0, 6.0])))
    bus.push(SetCellsCommand(sheet.column("A"), 0, [9.0]))
bus.undo()                             # tout l'import d'un coup
```
"""

from plotyx.commands.base import CommandReleasedError, ModelCommand
from plotyx.commands.bus import CommandBus
from plotyx.commands.context import executing, execution_depth, is_executing
from plotyx.commands.data import (
    DeleteRowsCommand,
    InsertRowsCommand,
    SetCellsCommand,
    SetColumnLengthCommand,
    SetColumnMetadataCommand,
    SetColumnTypeCommand,
)
from plotyx.commands.guard import (
    MutationOutsideCommandError,
    command_guard,
    mutation_guard,
)
from plotyx.commands.plot import (
    SetPlotSpecCommand,
    add_panel_command,
    add_series_command,
    current_spec,
    move_series_command,
    remove_panel_command,
    remove_series_command,
    set_graph_title_command,
    update_axis_command,
    update_legend_command,
    update_panel_command,
    update_series_command,
)
from plotyx.commands.structure import (
    AddColumnCommand,
    AddObjectCommand,
    DeleteColumnCommand,
    DeleteObjectCommand,
    MoveCommand,
    RenameCommand,
)

__all__ = [
    "AddColumnCommand",
    "AddObjectCommand",
    "CommandBus",
    "CommandReleasedError",
    "DeleteColumnCommand",
    "DeleteObjectCommand",
    "DeleteRowsCommand",
    "InsertRowsCommand",
    "ModelCommand",
    "MoveCommand",
    "MutationOutsideCommandError",
    "RenameCommand",
    "SetCellsCommand",
    "SetColumnLengthCommand",
    "SetColumnMetadataCommand",
    "SetColumnTypeCommand",
    "SetPlotSpecCommand",
    "add_panel_command",
    "add_series_command",
    "command_guard",
    "current_spec",
    "executing",
    "execution_depth",
    "is_executing",
    "move_series_command",
    "mutation_guard",
    "remove_panel_command",
    "remove_series_command",
    "set_graph_title_command",
    "update_axis_command",
    "update_legend_command",
    "update_panel_command",
    "update_series_command",
]
