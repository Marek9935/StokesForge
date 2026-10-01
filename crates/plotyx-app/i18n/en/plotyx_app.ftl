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
window-title-project-manager = PlotYx — Project Manager
window-title-detached = PlotYx — { $name }

## Menus and actions (#42)
## One label per action, shared by the menu bar, the context menus and the
## tooltips (`action-*`). `.hint` is the tooltip (the effect); `.disabled` is the
## tooltip of a greyed-out entry (why it is unavailable). Shortcuts are drawn by
## the menu itself, never written in the text.

menu-file = File
menu-edit = Edit
menu-graph = Graph
menu-window = Window

action-add-workbook = Add Workbook
    .hint = Adds a workbook with X and Y columns
action-import = Import…
    .hint = Reads instrument files into worksheets
action-refresh-from-source = Refresh from Source
    .hint = Re-reads the source files of the active worksheet's columns
action-reopen-last-import = Reopen Last Import…
    .hint = Reopens the Import Wizard with the last files and template
action-open = Open…
action-save = Save
action-save-as = Save As…
action-quit = Quit

action-undo = Undo
action-redo = Redo

menu-graph-properties = Graph Properties…
    .hint = Edits the title and background
menu-graph-plot-area = Plot Area…
    .hint = Edits the plot area title and gridlines
menu-graph-axes = Axes…
    .hint = Edits axis titles, scales and ranges
menu-graph-series = Series…
    .hint = Edits the plot type, line and symbol of each series
menu-graph-legend = Legend…
    .hint = Edits legend visibility and position
menu-graph-theme = Theme…
    .hint = Edits global colors and applies predefined themes
menu-graph-png-resolution = PNG Resolution
    .hint = Scale factor of PNG exports
menu-graph-png-resolution-option = ×{ $scale } ({ $width } × { $height } px)

action-reset-zoom = Reset Zoom
    .hint = Returns to the original axis ranges
action-add-text = Add Text
    .hint = Adds a text in the plot area
    .disabled = Right-click inside a plot area
action-edit-text = Edit Text
action-delete-text = Delete Text
action-properties = Properties…
    .hint = Opens Graph Properties on this item
action-export-graph = Export Graph…
    .hint = SVG, PDF or PNG, at the current zoom

menu-window-show-metadata-row = Show Metadata Row
    .hint = Shows the metadata of imported files in the column headers
action-maximize-window = Maximize Window
    .hint = Fills the workspace with the active window
action-restore-window = Restore “{ $title }”
    .hint = Shows all windows again
action-detach-project-manager = Detach Project Manager
    .hint = Moves the Project Manager to its own window
action-reattach-project-manager = Reattach Project Manager
    .hint = Puts the Project Manager back in the main window

action-cut = Cut
action-copy = Copy
action-copy-with-header = Copy with Header
    .hint = Copies the values and the column header
action-paste = Paste
action-paste-in-place = Paste in Place
    .hint = Pastes at the position the cells were copied from
    .disabled = Copy from the grid first. Text from elsewhere has no original position
action-clear = Clear
    .hint = Empties the selected cells
action-delete-shift-up = Delete and Shift Up
    .hint = Deletes now and shifts cells up. The Del key first clears, then deletes

shortcut-backspace = Backspace
shortcut-delete = Del

tabs-previous-worksheets = Show previous worksheets
tabs-next-worksheets = Show next worksheets

detached-workbook-unavailable = Workbook unavailable in this window. Reattach it to the main window
detached-note-unavailable = Note unavailable in this window. Reattach it to the main window

graph-text-default = Text

window-kind-workbook = Workbook
window-kind-graph = Graph
window-kind-matrix = Matrix
window-kind-note = Note
window-deleted = { $kind } { $id } (deleted)

status-ready = Ready
status-revision = Revision { $revision }
status-saved = Saved to { $path }
status-loaded = Loaded from { $path }
status-nothing-to-undo = Nothing to undo
status-nothing-to-redo = Nothing to redo
status-clipboard-unreadable = Clipboard is empty or unreadable

## Assistant d'import (#44)
## Texts of the Import Wizard: six steps, templates, metadata and preview.
## Template names and file contents are data and are never translated.

import-window-title = Import Wizard
import-button-back = Back
import-button-next = Next
import-button-finish = Finish
    .hint = Import with the current settings (Enter)

import-step-files-title = Files and Template
import-step-format-title = Format
import-step-header-title = Header and Data End
import-step-columns-title = Columns
import-step-metadata-title = Metadata
import-step-destination-title = Destination

# Values shared by several lists.
import-option-auto = Auto
import-option-none = None
import-option-comma = Comma
import-option-dot = Dot
import-option-space = Space
import-option-apostrophe = Apostrophe
import-option-other = Other

# Column types, shared by the "Read As" and "Type" columns.
import-type-numeric = Numeric
import-type-text = Text
import-type-mixed = Mixed
import-type-datetime = Date/Time

# Step 1: files and template.
import-template-untitled = Untitled
import-files-template = Template
import-files-template-modified = { $name } (modified)
import-files-reset = Reset
    .hint = Start again from default settings
import-files-delete = Delete…
    .hint = Delete this saved template
import-files-delete-confirm = Delete template “{ $name }”?
import-files-delete-yes = Delete
import-files-delete-keep = Keep
import-files-export = Export Template…
    .hint = Save this template to a JSON file
import-files-export-title = Export Template
import-files-import = Import Template…
    .hint = Load a template from a JSON file
import-files-import-title = Import Template
import-files-template-filter = Import template
import-files-unsaved = Unsaved changes. Save the template in the Destination step
import-files-add = Add Files…
    .hint = Choose the files to import
import-files-add-title = Choose Files to Import
import-files-drop = or drop files in the window
import-files-recent = Recent Files…
import-files-none = No files chosen
import-files-row-count =
    { $count ->
        [one] { $count } row
       *[other] { $count } rows
    }
import-files-remove = Remove

# Step 2: format.
import-format-delimiter = Delimiter
    .hint = Character that separates the values on a line
import-delimiter-semicolon = Semicolon
import-delimiter-tab = Tab
import-delimiter-whitespace = Whitespace
import-delimiter-fixed = Fixed width
import-format-widths-hint = Width of each column but the last, e.g. 10,8,8
import-format-decimal = Decimal separator
import-format-thousands = Thousands separator
import-format-missing = Missing values
    .hint = Comma-separated. Read as empty cells: --, n/a, *, NaN…
import-format-comment-prefix = Comment prefix
    .hint = Data lines starting with it are skipped (#, ;, //…)
import-format-blocks = Blocks
    .hint = How a file holding several data blocks is split
import-blocks-single = Single block
import-blocks-line-start = Line starting with
import-blocks-blank = Blank lines
import-format-encoding = Encoding
import-format-skip-chars = Remove leading characters
    .hint = Removed at the start of every data line
import-format-skip-lines = Skip data lines
    .hint = Ignored at the start of the data

# Step 3: header and data end.
import-header-mode = Header
import-header-mode-lines = Line count
import-header-mode-marker = Until marker
import-header-marker = Marker
    .hint = The header ends at the line equal to this text
import-header-lines = Header lines
import-header-subheader-lines = Subheader lines
    .hint = Counted in the header lines
import-header-detect = Detect Header
    .hint = Recount the header and subheader lines from the file
import-header-combined = Combined line
    .hint = Long name and units share the same subheader line
import-header-role-long-name = Long name
    .hint = Subheader lines that give the long names
import-header-role-units = Units
    .hint = Subheader lines that give the units
import-header-role-comments = Comments
    .hint = Subheader lines that give the comments
import-header-range-from = from
import-header-range-to = to
import-header-names-first-line = Names in first line
    .hint = The first data line gives the column names
import-dataend = Data end
import-dataend-auto = Auto
    .hint = Stops at the first non-numeric X
import-dataend-blank = First blank line
import-dataend-marker = End marker
import-dataend-after-header = Lines after header
import-dataend-before-end = Lines before end
import-header-skip-footer = Skip footer lines
    .hint = Ignored at the end of the file
import-header-column-names = Column names
import-header-name-file = File name
import-header-name-key = Header key
import-header-key = Key

# Step 4: columns.
import-columns-name = Name
import-columns-long-name = Long Name
import-columns-units = Units
import-columns-read-as = Read As
import-columns-type = Type
import-columns-role = Role
import-columns-values = Values
import-role-x-error = X Error
import-role-y-error = Y Error
import-columns-choose-file = Choose a file in step 1

# Step 5: metadata.
import-metadata-keep-header = Keep header
    .hint = Keep the file header as metadata
import-metadata-keep-footer = Keep footer
    .hint = Keep the file footer as metadata
import-metadata-uncheck = Uncheck a key or section to drop it
import-metadata-none = No metadata with the current settings

# Step 6: destination and template.
import-destination = Destination
import-destination-active = Active workbook (one worksheet per file)
import-destination-active-none = Active workbook (none)
import-destination-new-per-file = New workbook per file
import-destination-new-all = New workbook for all files
import-destination-mode = In active workbook
import-mode-new-sheet = New worksheet
import-mode-replace = Replace active worksheet
import-mode-add-columns = Add columns to active worksheet
import-mode-add-rows = Add rows to active worksheet
import-destination-multiple = Multiple files
import-layout-per-file = One worksheet per file
import-layout-side-by-side = One worksheet, side by side (X Y X Y…)
import-layout-shared-x = One worksheet, X of first file shared
import-destination-template-name = Template name
import-destination-save = Save Template
import-destination-saved-note = Saved templates are offered at the next import
import-destination-auto = Auto-select
    .hint = Picks this template for a file that matches every filled criterion
import-auto-extensions = Extensions
    .hint = csv, txt, asc…
import-auto-name = Name contains
import-auto-content = File start contains
    .hint = A signature, such as ORIGIN;JASCO

# Preview pane.
import-preview-title = Preview
import-preview-encoding = Encoding { $encoding }
import-preview-blocks = { $count } blocks, showing the first
import-preview-tip = Right-click a line to set the import there
import-preview-empty = Choose a file to see a preview
import-preview-showing =
    { $total ->
        [one] Showing { $shown } of { $total } data line
       *[other] Showing { $shown } of { $total } data lines
    }
import-legend-header = Header
import-legend-long-name = Long name
import-legend-units = Units
import-legend-comments = Comments
import-legend-column-names = Column names
import-legend-data = Data
import-legend-footer = Footer
import-legend-comment-line = Comment line
import-legend-skipped = Skipped line
import-line-data-start = Start Data Here
import-line-units = Use as Units Line
import-line-data-end = End Data Here
import-line-no-auto-end = Disable Auto Data End

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
