# Catalogue de référence de l'interface de PlotYx (issue #41, chantier #39).
#
# L'anglais est la langue source : une clé absente d'ici n'existe pas, et `tr!`
# (`src/i18n.rs`) refuse à la compilation une clé qui n'y figure pas. Règles de
# rédaction : `docs/textes-interface.md` ; mécanique : skill
# `plotyx-textes-interface`.
#
# Nommage des clés : `zone-objet[-précision]`, en minuscules et tirets.

## Window

window-title = PlotYx

## Menu bar

menu-file = File

## Witness of the machinery
## (read by the tests of `i18n.rs` only: one argument and one CLDR plural; to be
## removed once a real key with arguments exists)

witness-count =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }

## History entries (issue #46)
## One key per `HistoryLabel` variant (`plotyx-commands/src/label.rs`). Commands:
## Title Case. A name chosen by the user is quoted; a column is its grid letter.
## A variant with an optional target has a key for each form (named / generic).

history-set-cells = Edit Cells
history-set-cells-in-column = Edit Cells in Column { $column }
history-add-column = Add Column
history-delete-column = Delete Column
history-delete-column-named = Delete Column { $column }
history-rename = Rename “{ $from }” to “{ $to }”
history-rename-unnamed = Rename to “{ $to }”
history-move = Move
history-move-named = Move “{ $name }”
history-set-column-type = Change Column Type
history-set-column-type-of = Change Type of Column { $column }
history-insert-rows =
    Insert { $count } { $count ->
        [one] Row
       *[other] Rows
    }
history-delete-rows =
    Delete { $count } { $count ->
        [one] Row
       *[other] Rows
    }
history-delete-cells =
    Delete Cells and Shift Up ({ $count } { $count ->
        [one] Row
       *[other] Rows
    })
history-add-folder = Add Folder “{ $name }”
history-add-workbook = Add Workbook
history-add-empty-workbook = Add Empty Workbook
history-add-worksheet = Add Worksheet
history-add-graph = Add Graph
history-add-matrix = Add Matrix
history-add-note = Add Note
history-remove-node = Delete
history-remove-node-named = Delete “{ $name }”
history-set-window-meta = Edit Window Properties
history-set-window-meta-named = Edit Properties of “{ $name }”
history-set-note-text = Edit Note
history-set-note-text-named = Edit Note “{ $name }”
history-set-column-meta = Edit Column Properties
history-set-column-meta-of = Edit Properties of Column { $column }
history-set-plot-spec = Edit Graph
history-set-plot-spec-named = Edit Graph “{ $name }”
history-set-saved-colors = Edit Saved Colors
history-macro-paste-cells = Paste
history-macro-clear-selection = Clear Selection
history-macro-graph-properties = Edit Graph Properties
history-macro-refresh-from-file = Refresh from File
history-macro-import = Import
history-macro-add-workbook = Add Workbook
history-macro-add-worksheet = Add Worksheet

## Undo and Redo

history-undo = Undo
history-undo-entry = Undo { $label }
history-redo = Redo
history-redo-entry = Redo { $label }
history-undone = Undone: { $label }
history-undone-last = Undone last action
history-redone = Redone: { $label }
history-redone-last = Redone last action
