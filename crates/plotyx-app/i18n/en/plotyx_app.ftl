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

## Errors (#47)
## Raw English text of the lower crates stays in their `Display`; each variant
## is mapped to one of these keys in `src/errors.rs`. What happened, then what
## to do (only when useful). `{ $detail }` is the raw cause given by the crate
## or by the system; it is not translated.

# Project model
error-model-invalid-name = Invalid name: { $detail }
error-model-duplicate-name = Name already used: { $detail }
error-model-invalid-path = Invalid path: { $detail }
error-model-invalid-structure = Operation not allowed: { $detail }
error-model-unknown-object = This object no longer exists in the project
error-model-index-out-of-bounds = Position out of range: { $detail }
error-model-incompatible-values = Incompatible values: { $detail }
error-model-ids-exhausted = No more objects can be created in this project
error-model-concurrent-edit = The column changed while it was being edited. Try again
error-model-formula-column-read-only = This column is computed by a formula. Edit the formula to change its values

# Grouped actions and history
error-command-no-group = No grouped action is in progress
error-command-busy = Not available while another action is in progress. Finish it first
error-command-limit-not-empty = The history limit can only be changed when the history is empty
error-command-group-too-deep = Grouped actions are nested too deeply (limit: { $limit })
error-command-foreign-group = Another action is in progress. Wait for it to finish, then try again

# Engine
error-session-closed = The project is no longer available. Restart PlotYx

# Project files
error-file-io = Could not access { $path }: { $detail }
error-file-not-plotyx = { $path } is not a PlotYx file
error-file-too-new = { $path } needs a newer version of PlotYx (file format { $found }, supported up to { $supported })
error-file-corrupt = { $path } is damaged: { $detail }
error-file-invalid-project = { $path } does not contain a valid project: { $detail }
error-file-unknown-feature = { $path } uses features from a newer version of PlotYx ({ $detail })
error-file-changed = { $path } was modified elsewhere since it was opened or saved. Reload it before saving

# Import
error-import-binary = The file is not readable text
error-import-empty = The file is empty
error-import-no-data = No data found after the header. Check the number of header lines
error-import-file = { $path }: { $detail }

# Formulas
error-formula-invalid = Invalid formula: { $detail }
error-formula-unknown-column = Column { $letter } does not exist in this worksheet
error-formula-self-reference = A formula cannot refer to its own column ({ $letter })
error-formula-cycle = Formulas refer to each other in a loop. Remove one of the references

# Graph display and export
error-render-unknown-graph = This graph no longer exists
error-render-not-a-graph = This object is not a graph
error-export-drawing = Could not draw the graph: { $detail }
error-export-write = Could not write { $path }: { $detail }
error-export-fonts = Could not load the fonts: { $detail }
error-export-reread = Could not prepare the figure for export: { $detail }
error-export-pdf = Could not create the PDF: { $detail }
error-export-png-scale = Invalid PNG resolution ({ $scale }). Use a positive number
error-export-png-size = The PNG size is invalid ({ $width }×{ $height } px). Increase the resolution
error-export-png-encode = Could not encode the PNG: { $detail }

# Scripts
error-script-thread = Could not start the script engine: { $detail }
error-script-install = Could not set up the pyx module: { $detail }
error-script-busy = A script is already running
error-script-stopped = The script engine has stopped. Run the script again
error-python-home = Python was not found in { $path }
error-python-init = Could not start Python: { $detail }
error-python-trace =
    Python error:
    { $trace }

## Propriétés du graphe et export (#43)
##
## Zone `plot-*` : fenêtre des propriétés du graphe (six onglets), cibles du
## clic droit, éditeur de texte sur le graphique. `dialog-*` : pied commun des
## dialogues modaux. `export-*` : dialogue d'export. Un attribut `.hint` est
## l'infobulle de son contrôle. Les noms saisis par l'utilisateur, les thèmes et
## les noms de colonnes sont des données, jamais traduits.

# Footer shared by all modal dialogs (`modal_dialog_with`)

dialog-ok =
    OK
    .hint = Keep changes (Enter)
dialog-cancel =
    Cancel
    .hint = Discard changes (Esc)

# Window and tabs

plot-window-title = Graph Properties — { $name }
plot-tab-general = General
plot-tab-plot-area = Plot Area
plot-tab-axes = Axes
plot-tab-series = Series
plot-tab-legend = Legend
plot-tab-theme = Theme

# The three axes of a layer

plot-axis-bottom = X (bottom)
plot-axis-left = Y (left)
plot-axis-right = Secondary Y (right)

# What a right-click designates (head of the context menu)

plot-target-graph = Graph
plot-target-title = Title
plot-target-plot-area = Plot Area
plot-target-axis = { $axis } Axis
plot-target-axis-title = { $axis } Axis Title
plot-target-text = Text

# Automatic values of the color fields

plot-color-auto-contrast = Auto (contrasts with background)
plot-color-auto-white = Auto (white)
plot-color-same-as-line = Same as line

# General tab

plot-general-title =
    Title
    .hint = Shown above the graph
plot-general-title-color = Title color
plot-general-background = Background
plot-general-dark-note = Auto colors turn white on a dark background

# Plot Area tab

plot-area-layer-title =
    Layer title
    .hint = Shown above the plot area
plot-area-gridlines = Gridlines
plot-area-gridlines-vertical = Vertical (X ticks)
plot-area-gridlines-horizontal = Horizontal (Y ticks)

# Axes tab

plot-axes-title-section = Axis title
plot-axes-quantity =
    Quantity
    .hint = Name or symbol, e.g. Wavenumber
plot-axes-units =
    Units
    .hint = Shown after the quantity, in parentheses
plot-axes-title-color = Title color
plot-axes-scale-section = Scale
plot-axes-log-scale =
    Log scale
    .hint = Base-10 logarithm
plot-axes-reverse =
    Reverse axis
    .hint = Values decrease in the reading direction
plot-axes-auto-range =
    Auto range
    .hint = Computed from the data
plot-axes-min = Min
plot-axes-max = Max
plot-axes-display-section = Display
plot-axes-show-axis = Show axis
plot-axes-show-gridlines = Show gridlines
plot-axes-color = Axis color
plot-axes-color-note = Axis line, ticks and tick labels. Gridlines use it, faded

# Series tab

plot-series-empty = This graph has no series
plot-series-name =
    Name
    .hint = Empty: uses the plotted column's name as is
plot-series-plot-type = Plot type
plot-series-type-unknown = Unknown type (from a newer version)
plot-series-show = Show series
plot-series-show-in-legend = Show in legend
plot-series-line-section = Line
plot-series-line-color = Color
plot-series-line-style = Line style
plot-series-line-width = Line width (pt)
plot-series-symbol-section = Symbol
plot-series-symbol-shape = Shape
plot-series-symbol-size = Size (pt)
plot-series-symbol-color = Symbol color

plot-type-scatter = Scatter
plot-type-line = Line
plot-type-line-symbol = Line + Symbol
plot-type-bar = Bar
plot-type-histogram = Histogram
plot-type-error-bars = Line + Error Bars
plot-type-unknown = Unknown type

plot-line-style-none = None
plot-line-style-solid = Solid
plot-line-style-dashed = Dashed
plot-line-style-dotted = Dotted
plot-line-style-dash-dot = Dash-dot

plot-symbol-none = None
plot-symbol-circle = Circle
plot-symbol-square = Square
plot-symbol-triangle = Triangle
plot-symbol-diamond = Diamond
plot-symbol-plus = Plus (+)
plot-symbol-cross = Cross (×)

# Legend tab

plot-legend-show = Show legend
plot-legend-position = Position
plot-legend-top-left = Top Left
plot-legend-top-right = Top Right
plot-legend-bottom-left = Bottom Left
plot-legend-bottom-right = Bottom Right

# Theme tab

plot-theme-themes = Themes
plot-theme-global-colors = Global colors
plot-theme-background = Background
plot-theme-text = Text
plot-theme-axes = Axes
plot-theme-note = Applies to all titles, axes, ticks and gridlines. Replaces colors set in other tabs

# Memo of the Typst syntax, under every title field

plot-formula-memo-header = Formula syntax
plot-formula-greek = Greek letters λ, Δ
plot-formula-subscript = Subscript
plot-formula-superscript = Superscript
plot-formula-full = Full formula
plot-formula-upright = Upright text in a formula
plot-formula-format = Text formatting
plot-formula-escape = The character itself
plot-formula-footer = Text between $ … $ is a formula, the rest is plain text. Full syntax: { $url }

# Text on the graph (editor in place) and view of the graph

plot-text-size = Font size (pt)
plot-text-rotation = Rotation (°)
plot-text-ok =
    OK
    .hint = Apply (Enter)
plot-text-placeholder = Text, $formula$
plot-view-reset-zoom = Reset Zoom

# Export

export-dialog-title = Export Graph
