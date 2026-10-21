# Catalogue de référence de l'interface de StokesForge (issue #41, chantier #39).
#
# L'anglais est la langue source : une clé absente d'ici n'existe pas, et `tr!`
# (`src/i18n.rs`) refuse à la compilation une clé qui n'y figure pas. Règles de
# rédaction : `docs/textes-interface.md` ; mécanique : skill
# `stokesforge-textes-interface`.
#
# Nommage des clés : `zone-objet[-précision]`, en minuscules et tirets.

## Window

window-title-untitled = Untitled project
window-title-project = { $name } — StokesForge
window-title-project-unsaved = { $name } • — StokesForge
window-title-project-manager = StokesForge — Project Manager
window-title-detached = StokesForge — { $name }

## Boîtes de fichier du projet (#127)

project-open-title = Open Project
project-save-title = Save Project
project-file-filter = StokesForge project

## Menus and actions (#42)
## One label per action, shared by the menu bar, the context menus and the
## tooltips (`action-*`). `.hint` is the tooltip (the effect); `.disabled` is the
## tooltip of a greyed-out entry (why it is unavailable). Shortcuts are drawn by
## the menu itself, never written in the text.

menu-file = File
menu-edit = Edit
menu-workbook = Workbook
menu-graph = Graph
menu-view = View
menu-window = Window
menu-new = New
menu-import = Import
menu-properties = Properties
menu-insert-object = Insert Object
menu-export = Export
menu-sheet = Sheet
menu-columns = Columns
menu-language = Language

action-add-workbook = Add Workbook
    .hint = Adds a workbook with X and Y columns
action-new-project = New Project
    .hint = Creates an empty project in this window
action-add-graph = Add Graph
    .hint = Adds an empty graph with one plot area
unsaved-title = Unsaved Changes
unsaved-new = Save the changes to this project before creating a new one?
unsaved-open = Save the changes to this project before opening another one?
unsaved-quit = Save the changes to this project before quitting?
unsaved-discard = Discard Changes
    .hint = Loses the changes since the last save; this cannot be undone
unsaved-save = Save
    .hint = Saves the project, then goes on (Enter)
alert-save-failed-title = Project Not Saved
alert-save-failed = The project could not be saved. Its changes are kept.
alert-save-as = Save As…
    .hint = Saves the project to another file
alert-open-failed-title = Project Not Opened
alert-open-failed = The project could not be opened. The current project is unchanged.
alert-open-another = Open Another…
    .hint = Chooses another project file
alert-session-lost-title = Session Stopped
alert-session-lost = The project engine stopped unexpectedly. Changes made since your last save are lost. StokesForge will close.
alert-session-lost-recovered = A recovery copy of the last displayed state was saved to { $path }. It may lack your latest change.
alert-session-lost-no-recovery = No recovery copy could be written: { $detail }

alert-low-r2-title = Low R²
alert-low-r2 = Some results follow the data poorly (R² below 0.6). Check the model and the starting values.
low-r2-summary-head = R² < 0.6 for:
low-r2-summary-see-results = See the Results pane for the values.
low-r2-count-baseline =
    { $count ->
        [one] { $count } baseline
       *[other] { $count } baselines
    }
low-r2-count-correct =
    { $count ->
        [one] { $count } correction
       *[other] { $count } corrections
    }
low-r2-count-integrate =
    { $count ->
        [one] { $count } integration
       *[other] { $count } integrations
    }
low-r2-count-fit =
    { $count ->
        [one] { $count } fit
       *[other] { $count } fits
    }
low-r2-count-peaks =
    { $count ->
        [one] { $count } peak decomposition
       *[other] { $count } peak decompositions
    }
low-r2-count-reconvolution =
    { $count ->
        [one] { $count } reconvolution
       *[other] { $count } reconvolutions
    }
low-r2-count-direct-deconvolution =
    { $count ->
        [one] { $count } direct deconvolution
       *[other] { $count } direct deconvolutions
    }
status-low-r2-badge = { $count } low R²
status-low-r2-badge-hint = Some calculations fit the data poorly (R² below 0.6). Click to list them by type.
overwrite-title = Replace File
overwrite-confirm = “{ $name }” already exists. Replace it?
overwrite-replace = Replace
    .hint = Overwrites the existing file; this cannot be undone
discarded-save-title = Replace File
discarded-save-confirm = “{ $name }” was read with { $what } left out. Saving replaces it without them: discarded data will be lost.
discarded-save-replace = Replace
    .hint = Saves the project over the file; the discarded data cannot be recovered
action-import = Import…
    .hint = Reads instrument files into sheets
action-refresh-from-source = Refresh from Source
    .hint = Re-reads the source files of the active sheet's columns
action-reopen-last-import = Reopen Last Import…
    .hint = Reopens the Import Wizard with the last files and template
action-open = Open…
action-save = Save
action-save-as = Save As…
action-quit = Quit


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
menu-graph-svg-text = SVG Text
    .hint = How text is written in SVG exports
menu-graph-svg-text-text = Text
    .hint = Keeps text as editable SVG text; the font must be installed to view it
menu-graph-svg-text-outlines = Outlines
    .hint = Draws text as outlines; the file looks the same everywhere

action-reset-zoom = Reset Zoom
    .hint = Returns to the original axis ranges
action-add-text = Add Text
    .hint = Adds a text in the plot area
    .disabled = Right-click inside a plot area
menu-new-layer = New Layer
    .hint = Adds a layer over the active layer, with its own axes
action-mirror-axis-top = Top Mirror Axis
    .hint = Repeat the bottom scale along the top of the active layer, with no title
action-mirror-axis-right = Right Mirror Axis
    .hint = Repeat the left scale along the right of the active layer, with no title
action-new-layer-top-x = Top X (Linked Y)
    .hint = Adds a layer with its own X axis at the top; its Y scale follows the active layer
action-new-layer-right-y = Right Y (Linked X)
    .hint = Adds a layer with its own Y axis at the right; its X scale follows the active layer
action-new-layer-top-x-right-y = Top X, Right Y
    .hint = Adds a layer with its own X axis at the top and its own Y axis at the right
action-edit-text = Edit Text
action-reset-text-position = Reset Position
    .hint = Puts this title or legend back in its default place
action-reset-text-positions = Reset Text Positions
    .hint = Puts every title and the legend back in their default places
    .disabled = No title or legend has been moved
action-bring-to-front = Bring to Front
    .hint = Puts the selected object above the other objects
    .disabled = Select an object first
action-send-to-back = Send to Back
    .hint = Puts the selected object below the other objects
    .disabled = Select an object first
action-properties = Properties…
    .hint = Opens Graph Properties on this item
action-export-graph = Export Graph…
    .hint = SVG, PDF or PNG, at the current zoom
    .disabled = Bring a graph window to the front first
action-copy-graph = Copy Graph
    .hint = Copies the graph as an image (SVG, PNG and PDF) to paste into another application. Ctrl+C does the same when no object is selected
    .disabled = Bring a graph window to the front first

menu-window-show-metadata-row = Show Metadata Row
    .hint = Shows the metadata of imported files in the column headers
menu-workbook-show-x-prime-row = Show x' Row
    .hint = Shows the x' row in the column headers of this workbook
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
    .disabled = The selected blocks share neither rows nor columns, so they cannot be copied together
action-copy-with-header = Copy with Header
    .hint = Copies the values and the column header
grid-drag-rows =
    { $count ->
        [one] { $count } row
       *[other] { $count } rows
    }
graph-row-series = Row { $number }
status-row-no-x-prime = No column of this sheet has an x' value: nothing to plot
row-drop-title = Plot Rows
row-drop-confirm = This graph already has an X axis. Plot the rows against a new X axis, or against the existing one?
row-drop-new-axis = Create Axis
    .hint = Adds a layer with its own X axis, scaled by the x' values
row-drop-present-axis = Use Present Axis
    .hint = Plots the rows against the X axis of the active layer
grid-drag-columns =
    { $count ->
        [one] { $count } column
       *[other] { $count } columns
    }
action-select-all = Select All
    .hint = Selects every column of the sheet, headers included
action-paste = Paste
    .disabled = Select a single block of cells or contiguous columns to paste into
action-paste-in-place = Paste in Place
    .hint = Pastes at the position the cells were copied from
    .disabled = Copy from the grid first. Text from elsewhere has no original position
action-clear = Clear
    .hint = Empties the selected cells
action-remove-formula-keep-data = Remove Formula, Keep Data
    .hint = Removes the formula and keeps the calculated values as ordinary data
    .disabled = Select a column that has a formula
action-delete-columns = Delete
    .hint = Removes the selected columns from the sheet. Undo brings them back with their values
action-delete-shift-up = Delete and Shift Up
    .hint = Deletes now and shifts cells up. The Del key first clears, then deletes

shortcut-backspace = Backspace
shortcut-delete = Del

tabs-previous-worksheets = Show previous sheets
tabs-next-worksheets = Show next sheets

detached-workbook-unavailable = Workbook unavailable in this window. Reattach it to the main window
detached-note-unavailable = Note unavailable in this window. Reattach it to the main window


window-kind-workbook = Workbook
window-kind-graph = Graph
window-kind-matrix = Matrix
window-kind-note = Note
window-deleted = { $kind } { $id } (deleted)

status-ready = Ready
status-history = Show the last 50 messages
status-history-title = Messages
status-history-empty = No messages yet. The messages of your actions will appear here
status-history-time = { $time } UTC
status-level-warning = Warning
status-level-error = Error
status-saved = Saved to { $path }
status-saved-over-limit = Saved to { $path }, but the file ({ $size } MB) is over the { $limit } MB opening limit. Raise the limit in File > Preferences > General to reopen it
status-loaded = Loaded from { $path }
status-load-discarded-calculations = { $count } { $count ->
        [one] calculation
       *[other] calculations
    }
status-load-discarded-corrections = { $count } { $count ->
        [one] correction record
       *[other] correction records
    }
status-load-discarded-and = { $first } and { $second }
status-load-discarded = { $what } { $total ->
        [one] was
       *[other] were
    } discarded from the project ({ $ids })
status-load-discarded-corrections-only = { $what } { $total ->
        [one] was
       *[other] were
    } discarded from the project ({ $ids })
status-nothing-to-undo = Nothing to undo
status-nothing-to-redo = Nothing to redo
status-clipboard-unreadable = Clipboard is empty or unreadable
status-paste-non-contiguous = Cannot paste into a non-contiguous selection
status-paste-columns-dropped =
    { $count ->
        [one] One column did not fit and was not pasted
       *[other] { $count } columns did not fit and were not pasted
    }
status-cell-decimal-comma = "{ $text }" was read as { $value } (decimal comma)
status-paste-fields-dropped =
    { $count ->
        [one] One header field did not fit and was not pasted
       *[other] { $count } header fields did not fit and were not pasted
    }
status-paste-columns-fields-dropped = { $columns } column(s) and { $fields } header field(s) did not fit and were not pasted
status-graph-copied = Graph copied as { $formats }
status-graph-copied-partly = Graph copied as { $formats }; { $missing } could not be prepared
status-language-not-saved = Could not save the language choice: { $detail }

## Assistant d'import (#44)
## Texts of the Import Wizard: six steps, templates, metadata and preview.
## Template names and file contents are data and are never translated.

import-window-title = Import Wizard
import-button-back = Back
import-button-next = Next
import-button-finish = Finish
    .hint = Imports with the current settings (Enter)

import-step-files-title = Files and Template
import-step-format-title = Format
import-step-header-title = Header and Data End
import-step-columns-title = Columns

# Values shared by several lists.
common-auto = Auto
import-option-none = None
import-option-comma = Comma
import-option-dot = Dot
import-option-space = Space
import-option-apostrophe = Apostrophe
import-option-other = Other

# Column types, shared by the "Read As" and "Type" columns.
column-type = Type
column-type-numeric = Numeric
column-type-text = Text
column-type-mixed = Mixed
column-type-datetime = Date/Time

# Step 1: files and template.
import-template-untitled = Untitled
import-files-template = Template
import-files-template-modified = { $name } (modified)
import-files-reset = Reset
    .hint = Restarts from the default settings
import-files-delete = Delete…
    .hint = Deletes this saved template
import-files-delete-title = Delete Template
import-files-delete-confirm = Delete the template “{ $name }”? This cannot be undone.
import-files-delete-yes = Delete
    .hint = Deletes the saved template; this cannot be undone
import-files-export = Export Template…
    .hint = Saves this template to a JSON file
import-files-export-title = Export Template
import-files-import = Import Template…
    .hint = Loads a template from a JSON file
import-files-import-title = Import Template
import-files-template-filter = Import template
import-files-unsaved = Unsaved changes. Save the template in the Destination step
import-files-add = Add Files…
    .hint = Picks the files to import
import-files-add-title = Choose Files to Import
import-files-drop = or drop files in the window
import-files-section = Files
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
import-format-date-order = Date order
import-date-day-first = Day, then month
import-date-month-first = Month, then day
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
    .hint = Recounts the header and subheader lines from the file
import-header-combined = Combined line
    .hint = Long name and units share the same subheader line
import-header-role-long-name = Long Name
    .hint = Subheader lines that give the long names
import-header-role-units = Units
    .hint = Subheader lines that give the units
import-header-role-comments = Comments
    .hint = Subheader lines that give the comments
import-header-role-x-prime = x'
    .hint = Subheader line that gives the x' value of each column (e.g. its time)
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
import-column-names = Column names
import-header-name-file = File name
import-header-name-key = Header key
import-header-key = Key

# Step 4: columns.
import-columns-name = Name
import-columns-read-as = Read As
import-columns-type = Type
import-columns-values = Values
import-columns-choose-file = Choose a file in step 1

# Step 5: metadata.
import-metadata-keep-header = Keep header
    .hint = Stores the file header as metadata
import-metadata-keep-footer = Keep footer
    .hint = Stores the file footer as metadata
import-metadata-uncheck = Uncheck a key or section to drop it
import-metadata-lines-section = File metadata
import-metadata-none = No metadata with the current settings

# Step 6: destination and template.
import-destination = Destination
import-destination-active = Active workbook (one sheet per file)
import-destination-active-none = Active workbook (none)
import-destination-new-per-file = New workbook per file
import-destination-new-all = New workbook for all files
import-destination-mode = In active workbook
import-mode-new-sheet = New sheet
import-mode-replace = Replace active sheet
import-mode-add-columns = Add columns to active sheet
import-mode-add-rows = Add rows to active sheet
import-destination-multiple = Multiple files
import-layout-per-file = One sheet per file
import-layout-side-by-side = One sheet, side by side (X Y X Y…)
import-layout-shared-x = One sheet, X of first file shared
import-destination-template-name = Template name
import-destination-template-section = Save as template
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
import-legend-data = Data
import-legend-footer = Footer
import-legend-comment-line = Comment line
import-legend-skipped = Skipped line
import-line-data-start = Start Data Here
import-line-units = Use as Units Line
import-line-data-end = End Data Here
import-line-no-auto-end = Disable Auto Data End

## History entries (issue #46)
## One key per `HistoryLabel` variant (`stokesforge-commands/src/label.rs`). Commands:
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
history-add-worksheet = Add Sheet
history-duplicate-worksheet = Duplicate Sheet
history-duplicate-worksheet-named = Duplicate Sheet “{ $name }”
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
history-macro-add-worksheet = Add Sheet
history-macro-script = Run Script
history-macro-set-columns-type = Change Column Types
history-macro-set-columns-role = Change Column Roles
history-macro-analysis-to-graph = Analyze and Plot
history-macro-remove-formula = Remove Formula
history-macro-delete-columns = Delete Columns

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
error-session-closed = The project is no longer available. Restart StokesForge

# Project files
error-file-io = Could not access { $path }: { $detail }
error-file-not-stokesforge = { $path } is not a StokesForge file
error-file-too-new = { $path } needs a newer version of StokesForge (file format { $found }, supported up to { $supported })
error-file-too-large = { $path } is too large to open ({ $size } MB, limit { $limit } MB). Raise the limit in File > Preferences > General, then open it again
error-file-corrupt = { $path } is damaged: { $detail }
error-file-invalid-project = { $path } does not contain a valid project: { $detail }
error-file-unknown-feature = { $path } uses features from a newer version of StokesForge ({ $detail })
error-file-changed = { $path } was modified elsewhere since it was opened or saved. Reload it before saving
error-file-unreadable-target = Cannot save over { $path }: it is a StokesForge file that cannot be read ({ $detail }). Use Save As with another name, or move that file first

# Import
error-import-binary = The file is not readable text
error-import-empty = The file is empty
error-import-no-data = No data found after the header. Check the number of header lines
error-import-whitespace-thousands = A space cannot both separate the columns and group the thousands. Choose another delimiter or another thousands separator
error-import-file = { $path }: { $detail }

# Formulas
error-formula-invalid = Invalid formula: { $detail }
error-formula-unknown-column = Column { $letter } does not exist in this sheet
error-formula-self-reference = A formula cannot refer to its own column ({ $letter })
error-formula-cycle = Formulas refer to each other in a loop. Remove one of the references
error-formula-too-deep = The formula is nested too deeply. Simplify it (fewer nested parentheses or chained terms)

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
error-script-install = Could not set up the pyst module: { $detail }
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
    .hint = Keeps the changes (Enter)
dialog-cancel =
    Cancel
    .hint = Discards the changes (Esc)
dialog-close =
    Close
    .hint = Closes this window (Enter)

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
plot-axis-top = Secondary X (top)
plot-axis-overlay-x = X (top)
plot-axis-overlay-y = Y (right)

# What a right-click designates (head of the context menu)

plot-target-graph = Graph
plot-target-title = Title
plot-target-plot-area = Plot Area
plot-target-axis = { $axis } Axis
plot-target-axis-title = { $axis } Axis Title
plot-target-text = Text
plot-target-object = Object
plot-target-legend = Legend

# Automatic values of the color fields

plot-color-auto-contrast = Auto (contrasts with background)
plot-color-auto-white = Auto (white)
plot-color-same-as-line = Same as line

# General tab

plot-general-title =
    Title
    .hint = Shown above the graph
plot-title-color = Title color
plot-general-background = Background
plot-general-dark-note = Auto colors turn white on a dark background
plot-stale-warning = Warn about out-of-date data
    .hint = Greys out the curves of calculations that are out of date and marks them, on screen and in exports

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
    .disabled = Uncheck Auto range to set the bounds
plot-axes-from = From
plot-axes-display-section = Display
plot-axes-decimal-comma = Decimal comma on tick labels
    .hint = Writes 1,5 instead of 1.5 on every axis of this graph, on screen and in exports
plot-axes-show-axis = Show axis
plot-axes-show-gridlines = Show gridlines
plot-axes-show-tick-labels = Show tick labels
plot-axes-show-mirror = Show mirror axis
plot-axes-mirror-note = Repeats the scale of the opposite axis (bottom or left), with no title
plot-axes-color = Axis color
plot-axes-color-note = Axis line, ticks and tick labels. Gridlines use it, faded

# Series tab

plot-series-empty = This graph has no series. Drag columns from a workbook onto it
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

# Series window and groups (#177–#179)

curve-window-title = Series Properties — { $name }
curve-group-name = Group { $number }
curve-group-section = Group
curve-group-field = Group
curve-group-none = None
curve-group-new = New group
curve-group-entry = { $group } ({ $count })
curve-group-leave =
    Leave Group
    .hint = Detaches this series from its group; it keeps its current style
    .disabled = This series is not in a group
curve-group-name-field =
    Name
    .hint = Name of the group in the Object Manager and the series list. Leave empty for the automatic name.
curve-group-ungroup =
    Ungroup
    .hint = Turns every series of the group into a single series; each keeps its style
curve-group-delete =
    Delete Group
    .hint = Deletes the group and all its series from the graph
curve-group-move-up =
    Move Group Up
    .hint = Moves the whole group up in the drawing order
curve-group-move-down =
    Move Group Down
    .hint = Moves the whole group down in the drawing order
curve-scope-field = Edit
curve-scope-series = This series only
curve-scope-group = Whole group ({ $count } series)
curve-scope-group-note = Line style, line width, symbol shape and size apply to every series of the group. Name, type, visibility and colors stay per series; spread colors with the gradient below.
curve-scope-series-note = Changes apply to this series only. It stays in its group; use Leave Group to detach it.
curve-scope-locked = Per-series setting: switch to "This series only" to change it
curve-gradient-locked = Switch to "Whole group" to apply a gradient
curve-gradient-saved = { $name } ({ $target })
curve-gradient-delete =
    Delete
    .hint = Deletes this saved gradient
    .disabled = Only a saved gradient can be deleted
curve-gradient-name-field =
    Name
    .hint = Name of the gradient
curve-gradient-save =
    Save
    .hint = Saves the custom gradient in the project or for the user
    .disabled = Give the gradient a name first
curve-gradient-section = Color gradient
curve-gradient-field = Gradient
curve-gradient-custom = Custom
curve-gradient-stop = Color { $number }
curve-gradient-add-stop = Add Color
curve-gradient-remove-stop = Remove Color
curve-gradient-blue-red = Blue to Red
curve-gradient-black-red = Black to Red
curve-gradient-black-blue = Black to Blue
curve-gradient-blue-yellow = Blue to Yellow
curve-gradient-green-red = Green to Red
curve-gradient-blue-white-red = Blue to White to Red
curve-gradient-viridis = Viridis
curve-gradient-plasma = Plasma

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
plot-legend-top-left = Top left
plot-legend-top-right = Top right
plot-legend-bottom-left = Bottom left
plot-legend-bottom-right = Bottom right

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
plot-formula-footer = Text between $ … $ is a formula, the rest is plain text. This is Typst syntax; full reference: { $url }

# Text on the graph (editor in place) and view of the graph

plot-text-size = Font size (pt)
plot-text-rotation = Rotation (°)
plot-text-align =
    Alignment
    .hint = Aligns the text in its box (set a width with the handles)
plot-text-align-left = Left
plot-text-align-center = Center
plot-text-align-right = Right
plot-text-ok =
    OK
    .hint = Applies the text (Enter)
plot-text-placeholder = Text, $formula$
plot-view-reset-zoom = Reset Zoom
plot-layer-badge = { $n }
    .hint = Makes layer { $n } the target of additions to this graph
plot-layer-choice-disabled = This page applies to the whole graph, not to a layer

# Export

export-dialog-title = Export Graph

## Workbook, Project Manager, notes, colors, console (#45)

# Project Manager
project-manager-title = Project Manager
project-manager-show = Show
    .hint = Opens or unhides the selected windows
project-manager-hide = Hide
    .hint = Hides the selected windows without closing them
project-folder-empty = (empty)

# Workbook: stacked column header (left gutter)
column-x-prime = x'
status-x-prime-invalid = x' must be a finite number
column-long-name = Long Name
column-units = Units
column-comments = Comments
workbook-header-formula = F(x)=
common-metadata = Metadata
workbook-metadata-hide = Hide Metadata Row
workbook-metadata-summary =
    { $count ->
        [one] { $count } key
       *[other] { $count } keys
    }
    .hint = Shows the file metadata
workbook-metadata-use-as-x-prime = Use as x'
    .hint = Writes this value into the x' row of the column
workbook-metadata-title = File Metadata
workbook-metadata-title-named = Metadata — { $name }

# Workbook: column menu (type, role, plot)
column-role = Role
workbook-role-x = X
workbook-role-y = Y
workbook-role-x2 = X2
workbook-role-y2 = Y2
workbook-role-z = Z
column-role-x-error = X Error
column-role-y-error = Y Error
workbook-role-label = Label
workbook-role-none = None
workbook-plot-menu = Plot
workbook-send-to-graph = Send to { $graph }
    .hint = Adds the selected columns to an open graph
workbook-send-to-graph-menu = Send to Graph
    .hint = Adds the selected columns to an open graph

# Workbook: grid, sheet tabs, empty states
workbook-column-add = Add Column
workbook-grid-no-columns = No columns in this sheet. Right-click here and choose Add Column
workbook-no-worksheets = This workbook has no sheets. Use Add Sheet in the sheet tabs
workbook-worksheet-add = Add Sheet
action-rename-sheet = Rename
    .hint = Edit the sheet name
action-duplicate-sheet = Duplicate
    .hint = Add a copy of this sheet, with its columns
action-delete-sheet = Delete
    .hint = Removes this sheet and its columns; Undo brings them back
    .disabled = A workbook must keep at least one sheet
sheet-copy-name = { $name } copy { $number }
sheet-invalid = { $id } is not a sheet of this project
sheet-rename-empty = A sheet name cannot be empty; “{ $name }” kept
sheet-rename-taken = A sheet named “{ $name }” already exists in this workbook; “{ $old }” kept
sheet-delete-last = A workbook must keep at least one sheet
sheet-delete-used = Sheet deleted; it was plotted by { $graphs }. Undo brings it back
matrix-unavailable = Matrix editing is not available yet
note-deleted = This note was deleted

# Script console
script-title = Python Script
script-unavailable = — Not available in this build
script-run = Run
script-stop = Stop
script-running = Running…
script-not-run = Not run yet. Write a script, then click Run
script-stdout = Standard output
script-stderr = Error output
script-traceback = Traceback
script-no-output = (no output)

# Color picker
color-none = No color (transparent)
color-none-swatch = No color (transparent, no background in exports)
color-custom = Custom color
    .hint = Picks any color and its opacity, and saves it
color-shades-dark = Darker shades
color-shades-light = Lighter shades
color-section-text = Text colors
color-section-light-backgrounds = No color (default) and light backgrounds
color-section-dark-backgrounds = Dark backgrounds (auto text is white)
color-saved-project = Saved in project
color-saved-user = My colors (all projects)
color-saved-remove-hint = Right-click to remove
color-save = Save Color
    .hint = Adds the current color to the saved colors
color-target-project = In project
color-target-user = In my colors
color-shape-square = Square
color-shape-wheel = Wheel
color-opacity = Opacity (%)
color-rgb = RGB
color-hsl = HSL
color-hex = Hex
    .hint = #RRGGBB, or #RRGGBBAA with opacity
color-channel-red = R
color-channel-green = G
color-channel-blue = B
color-channel-hue = H (°)
color-channel-saturation = S (%)
color-channel-lightness = L (%)
color-palette-stokesforge = StokesForge (default)
color-palette-tableau = Tableau 10
color-palette-okabe-ito = Okabe-Ito (colorblind-safe)
color-palette-brewer-set1 = ColorBrewer Set1
color-palette-brewer-dark2 = ColorBrewer Dark2
color-palette-brewer-paired = ColorBrewer Paired
color-palette-viridis = Viridis (gradient)
color-palette-plasma = Plasma (gradient)
color-palette-grayscale = Grayscale (gradient)

## Toolbars (#60)

toolbar-file = File
    .hint = Shows or hides the File toolbar
status-toolbars-not-saved = Could not save the toolbar layout: { $detail }

toolbar-tools = Tools
    .hint = Shows or hides the toolbar of the active window
    .disabled = Bring a graph window to the front first
tool-pointer = Pointer
    .hint = Drag to move and resize texts and objects; Ctrl + drag or middle-click + drag pans the view; Ctrl + wheel zooms
tool-zoom = Zoom
    .hint = Drag a rectangle to zoom in on it; Ctrl + drag or middle-click + drag pans the view
tool-text = Text
    .hint = Click in the plot area to add a text
tool-rectangle = Rectangle
    .hint = Drag in the plot area to draw a rectangle
tool-ellipse = Ellipse
    .hint = Drag in the plot area to draw an ellipse
tool-line = Line
    .hint = Drag in the plot area to draw a line
tool-arrow = Arrow
    .hint = Drag in the plot area to draw an arrow; the head is where you release
toolbar-plot-selection = Plot Selection
    .hint = Plots the selected columns in a new graph
    .disabled = Select columns in the sheet first
toolbar-text = Text format
    .hint = Shows or hides the text format toolbar of the active graph
toolbar-edit = Edit
    .hint = Shows or hides the Edit toolbar
format-disabled = Select a text in the graph, or edit one, to format it. Titles and the legend only take a font and a size
format-object-disabled = Select an object in the graph to format it
format-object-line-color = Line color
    .hint = Sets the color of the selected object's line
format-object-fill-color = Fill color
    .hint = Sets the fill of the selected rectangle or ellipse; the opacity is set in the color picker
format-object-no-fill = No fill
format-object-line-width = Line width (pt)
    .hint = Sets the width, in points, of the selected object's line
format-bold = Bold
    .hint = Wraps the selected text in bold tags; press again to remove them
format-italic = Italic
    .hint = Wraps the selected text in italic tags; press again to remove them
format-subscript = Subscript
    .hint = Lowers the selected text
format-superscript = Superscript
    .hint = Raises the selected text
format-formula = Formula
    .hint = Writes the selected text as a math formula
format-font = Font
    .hint = Sets the font of the whole text box, math included. A system font changes the figure on a computer that lacks it
format-font-default = Graph font
format-font-embedded = Included with StokesForge
format-font-system = Installed on this computer
format-size = Size
    .hint = Sets the size, in points, of the selected text
format-color = Text color

action-object-manager = Object Manager
    .hint = Show or hide the objects of the active graph.
object-manager-no-graph = No graph is active. Click a graph to see its objects
object-manager-visibility = Show or hide this content. Hidden series are excluded from the legend and automatic axis limits.
object-manager-layer = Layer { $index }
object-manager-group = Series
object-manager-axes = Axes
object-manager-show-axes = Show Axes
    .hint = Lists the axes of each layer in the Object Manager. Does not change their visibility on the graph. Not listed by default.

object-manager-show-objects = Show Objects
    .hint = Lists the free objects of each layer (free texts, shapes) in the Object Manager. Does not change their visibility on the graph. Listed by default.

menu-selected-content = Selection
    .disabled = Select a curve, a text or an object first
action-rename-series = Rename
action-rename-group = Rename
action-ungroup-series =
    Ungroup
    .hint = Turns every series of this group into a single series; each keeps its style
action-select-group-series =
    Select Members
    .hint = Selects the group and shows the handles of all its series
action-move-series-up = Move Up
action-move-series-down = Move Down
action-show-content = Show
action-hide-content = Hide
action-delete-content = Delete
object-properties-title = Object Properties
object-coordinate = { $coordinate }
history-plot-visibility = Change Visibility
history-plot-rename-series = Rename Series
history-plot-reorder-series = Reorder Series
history-plot-remove-series = Delete Series
history-plot-remove-text = Delete Text
history-plot-remove-object = Delete Object
history-plot-reorder-object = Reorder Object
history-plot-rename-group = Rename Group
history-plot-dissolve-group = Ungroup Series
history-plot-remove-group = Delete Group
history-plot-reorder-group = Reorder Group

# Panneaux fixes et fenêtres détachées (#100)
panel-expand = Expand the panel
panel-collapse = Collapse the panel
panel-detach = Detach into a separate window
panel-reattach = Reattach to the main window

history-macro-paste-objects = Paste Objects
history-macro-duplicate-objects = Duplicate Objects
history-macro-delete-objects = Delete Objects
history-macro-move-objects = Move Objects

object-rename-empty = The name is empty; the previous name was kept
object-clipboard-invalid = The clipboard contains no valid StokesForge objects
object-delete-used = Deleted; its data was used by { $graphs }. Undo brings it back

action-rename-project-object = Rename
action-duplicate-project-object = Duplicate
action-delete-project-object = Delete

history-macro-rename-objects = Rename Objects

object-clipboard-use-manager = Select a folder or object in Project Manager to paste these objects

action-show-graph-data = Show Data

## Copie du graphe vers une autre application (#149)

error-clipboard-empty = There is nothing to copy
error-clipboard-unsupported = This display cannot receive a copied graph ({ $display })
error-clipboard-backend = Could not copy the graph: { $detail }

# Réglages de vue de l’Object Manager (#139).
object-manager-default-expansion = Expand by Default
    .hint = Only sets the initial state of new groups; groups already expanded or collapsed keep their state
object-manager-expand-layers = Layers
object-manager-expand-plots = Plot groups
object-manager-objects = Objects
object-manager-reset = Reset Settings
    .hint = Restores the Object Manager settings to their defaults without changing the current state of groups

project-manager-selection-needed = Select at least one object first

history-add-calculation = Add { $kind } Calculation
history-edit-calculation = Edit { $kind } Calculation
history-delete-calculation = Delete { $kind } Calculation
history-detach-calculation = Detach { $kind } Calculation
history-delete-calculation-any = Delete Calculation
history-detach-calculation-any = Detach Calculation
analysis-kind-baseline = Baseline
analysis-kind-correct = Correction
analysis-kind-integrate = Integration
analysis-kind-fit = Fit
analysis-kind-peaks = Peak Decomposition
analysis-kind-reconvolution = Reconvolution
analysis-kind-direct-deconvolution = Direct Deconvolution
error-model-calculated-column-read-only = This column is produced by a calculation. Detach it from the calculation to edit its values

## Analysis — names of the columns an operation produces (#181)
analysis-output-baseline-corrected = { $name } ({ $column }-corr)
analysis-output-corrected = { $name } (corrected)
analysis-output-baseline = Baseline of { $column }
analysis-output-cumulative = Cumulative integral of { $column }
analysis-output-normalized = Normalized integral of { $column }
analysis-output-fit = { $name } (fit)
analysis-output-residuals = { $name } (resid.)
analysis-output-peak = Peak { $index } of { $column }
analysis-output-background = Background of { $column }
analysis-output-peaks-sum = Sum of peaks of { $column }
analysis-output-deconvolved = { $name } (deconvolved)

## Analysis — errors of a calculation (#181)
error-analysis-not-enough-points = Only { $found } usable points, at least { $needed } are needed. Check the range and the data
error-analysis-duplicate-x = Two points share the same X value. Remove or average the duplicates first
error-analysis-bad-expression = The model expression is invalid: { $detail }
error-analysis-no-free-parameter = Every parameter is fixed. Free at least one parameter
error-analysis-empty-bounds = The bounds of { $name } are inverted (minimum { $min } is above maximum { $max }). Fix the bounds
error-analysis-non-finite-model = The model gives no valid value with the starting values. Change the starting values
error-analysis-model-not-evaluable = The model cannot be evaluated around the current values: its slope is not a number. Check the expression, the bounds and the starting values
error-analysis-singular-system = The data do not determine the parameters. Fix a parameter or widen the range
error-analysis-not-converged = The fit did not converge after { $iterations } iterations. Change the starting values or the model
error-analysis-not-enough-anchors = The baseline needs at least { $needed } anchor points, { $found } given
error-analysis-zones-too-small = The background zones hold { $found } points, { $needed } are needed for degree { $degree }. Widen the zones or lower the degree
error-analysis-duplicate-anchor = Two anchor points share the same X value. Move one of them
error-analysis-empty-reference = The reference column has no usable value. Choose another column
error-analysis-empty-response = The instrument response has no usable signal. Choose another column
error-analysis-empty-range = The range holds no point. Widen it
error-analysis-invalid-parameter = The value of { $name } is invalid
error-analysis-length-mismatch = A series and its X column do not have the same number of rows
error-analysis-output-unwritable = A result column no longer accepts the result. Delete that column or detach the calculation
error-analysis-no-peak = No peak to fit. Place a peak on the preview or lower the minimum prominence
error-analysis-bad-sigma = The error column holds a missing, zero or negative value. Fix it or turn weighting off
error-analysis-bad-signal = This signal cannot be deconvolved: its mean is not positive or its X steps are too uneven. Check the data
error-analysis-bad-components = The number of components must be between 1 and 4
error-analysis-bad-degree = The degree must be between 1 and 9
error-analysis-bad-iterations = The number of iterations must be between 1 and 5000
error-analysis-bad-noise = The noise ratio must be a positive number
error-analysis-bad-max-peaks = The maximum number of peaks is 100
error-analysis-expr-empty = The expression is empty
error-analysis-expr-unexpected-char = Unexpected character '{ $symbol }' at position { $position }
error-analysis-expr-unexpected-end = The expression ends too early (position { $position })
error-analysis-expr-unexpected-token = Unexpected symbol at position { $position }
error-analysis-expr-unbalanced = Unbalanced parentheses (position { $position })
error-analysis-expr-bad-number = Malformed number at position { $position }
error-analysis-expr-too-deep = Expression nested too deeply (position { $position })
error-analysis-expr-unknown-function = Unknown function { $name } at position { $position }
error-analysis-expr-wrong-arguments = { $name } takes { $expected } argument(s), { $found } given (position { $position })

## Analysis — status messages and what is missing (#181)
status-analysis-no-series = Select at least one Y column to analyze
status-analysis-gone = This calculation no longer exists
status-analysis-recalculated = Calculations recalculated
analysis-title = { $kind } of { $column }
analysis-issue-no-series = Select at least one Y column
analysis-issue-needs-reference = Choose a reference column
analysis-issue-needs-response = Choose the instrument response column
analysis-issue-needs-points = Place at least { $needed } points ({ $have } placed)
analysis-issue-needs-zone = Draw at least one background zone
analysis-issue-no-parameter = The model has no parameter
analysis-issue-bad-model = The model is invalid: { $detail }
analysis-issue-missing-column = A column of this calculation no longer exists

## Analysis — menu (#181)
menu-analysis = Analysis
analysis-block-type = Analysis type
action-analysis-open-data-manipulation = Data Manipulation…
    .hint = Opens the Data Manipulation window: baselines and correction by a reference, which can replace the original data
action-analysis-open-fitting = Fitting…
    .hint = Opens the Fitting window: fits a model to the data
action-analysis-open-peaks = Peaks and Deconvolution…
    .hint = Opens the Peaks and Deconvolution window: peak decomposition, reconvolution and direct deconvolution
action-analysis-open-integrate = Integrate…
    .hint = Opens the Integrate window: areas over ranges and the cumulative integral
analysis-category-data-manipulation = Data Manipulation
analysis-category-fitting = Fitting
analysis-category-peaks = Peaks and Deconvolution
analysis-category-integrate = Integrate
analysis-needs-y = Select at least one Y column first
analysis-baseline-linear = Linear…
    .hint = Subtracts or divides by a straight line through two points
analysis-baseline-polynomial = Polynomial…
    .hint = Fits a polynomial on background zones, then subtracts or divides it
analysis-baseline-anchors = Anchor Points…
    .hint = Subtracts or divides by a curve through anchor points you place
action-analysis-correct = Correct by Reference…
    .hint = Divides, multiplies or subtracts a reference column, interpolated on the data
action-analysis-integrate = Integrate…
    .hint = Computes areas over one or several ranges, and the cumulative integral
analysis-fit-linear = Linear…
    .hint = Fits a straight line
analysis-fit-polynomial = Polynomial…
    .hint = Fits a polynomial
analysis-fit-exponential = Exponential…
    .hint = Fits a decaying exponential
analysis-fit-exponential-offset = Exponential with Plateau…
    .hint = Fits a decaying exponential that settles on a constant plateau
analysis-fit-exponential2 = Two Exponentials…
    .hint = Fits the sum of two decaying exponentials and a constant plateau
analysis-fit-logarithm = Logarithm…
    .hint = Fits a logarithm
analysis-fit-power = Power…
    .hint = Fits a power law
analysis-fit-gaussian = Gaussian…
    .hint = Fits a Gaussian peak
analysis-fit-lorentzian = Lorentzian…
    .hint = Fits a Lorentzian peak
analysis-fit-custom = Custom Model…
    .hint = Fits a formula you write, with its own parameters
action-analysis-peaks = Decompose Peaks…
    .hint = Fits several peaks at once, found automatically or placed by hand
action-analysis-reconvolution = Reconvolution…
    .hint = Fits exponentials convolved with the instrument response
action-analysis-direct = Direct Deconvolution…
    .hint = Recovers the signal before the instrument response, without a model
action-analysis-recalculate-stale = Recalculate Out of Date
    .hint = Recalculates every calculation whose data changed
    .disabled = Everything is up to date

## Analysis — recent analyses (#249)
analysis-recent-entry = { $rank } { $label }…
analysis-recent-hint = Opens the window with the same settings. Ctrl+click runs it on the current selection
analysis-recent-hint-replace = Opens the window with the same settings. Ctrl+click replaces the original data of the current selection; Ctrl+Z undoes it
analysis-recent-baseline-constant = Constant Baseline
analysis-recent-baseline-linear = Linear Baseline
analysis-recent-baseline-polynomial = Polynomial Baseline
analysis-recent-baseline-anchors = Anchor Points Baseline
analysis-recent-correct = Correct by Reference
analysis-recent-integrate = Integrate
analysis-recent-fit = { $model } Fit
analysis-recent-peaks = Decompose Peaks
analysis-recent-reconvolution = Reconvolution
analysis-recent-direct = Direct Deconvolution
analysis-recent-subtract = Subtract
analysis-recent-divide = Divide
analysis-recent-multiply = Multiply
analysis-recent-degree = degree { $degree }, { $op }
analysis-recent-degree-only = degree { $degree }
analysis-recent-detail = { $name } ({ $detail })
status-analysis-replaced = Original data replaced by { $label } · Ctrl+Z to undo
action-calculation-edit = Edit Calculation…
    .hint = Reopens the calculation that produced this column
action-calculation-recalculate = Recalculate
    .hint = Refreshes the output columns from the current data
    .disabled = This calculation is up to date
action-calculation-detach = Detach from Calculation
    .hint = Keeps the columns as ordinary columns and removes the calculation
action-calculation-delete = Delete Calculation…
    .hint = Deletes the calculation, with or without its columns

## Analysis — names of calculated values (#181)
analysis-value-area = Area { $index }
analysis-value-area-from = Area { $index }: from
analysis-value-area-to = Area { $index }: to
analysis-value-area-total = Total area
analysis-value-out-of-reference = Points outside the reference
analysis-value-points = Points used
analysis-value-r-squared = R²
analysis-value-reduced-chi2 = Reduced χ²
analysis-value-dof = Degrees of freedom
analysis-value-tau-amplitude = τ (amplitude-weighted)
analysis-value-tau-intensity = τ (intensity-weighted)
analysis-value-noise-ratio = Noise ratio
analysis-value-baseline-points = Baseline points
analysis-value-baseline-slope = Baseline slope
analysis-value-baseline-intercept = Baseline intercept
analysis-value-peak-height = Peak { $index }: height
analysis-value-peak-center = Peak { $index }: center
analysis-value-peak-fwhm = Peak { $index }: FWHM
analysis-value-peak-fwhm-gauss = Peak { $index }: Gaussian FWHM
analysis-value-peak-fwhm-lorentz = Peak { $index }: Lorentzian FWHM
analysis-value-peak-area = Peak { $index }: area
analysis-value-peak-area-percent = Peak { $index }: area (%)
analysis-value-background-y0 = Background: offset
analysis-value-background-slope = Background: slope

## Analysis — preview (#181)
analysis-preview-residuals = Residuals
analysis-preview-average = Average over { $count } points

## Analysis — window (#181)
analysis-window-new = New analysis: { $kind }
analysis-window-edit = Edit calculation: { $kind }
analysis-apply-create = Create
    .hint = Runs the calculation and writes its columns
analysis-apply-update = Update
    .hint = Applies the new settings and rewrites the columns
analysis-block-input = Input
analysis-block-output = Output
analysis-series-previous = Previous series
analysis-series-next = Next series
analysis-series-position = Series { $current } of { $total }
analysis-field-reference = Reference
analysis-field-response = Instrument response
analysis-field-choose-column = Choose a column
analysis-field-no-candidate = No other Y column in this workbook
analysis-field-auto = auto
analysis-field-to = to
analysis-field-range = Range
analysis-field-remove = Remove
analysis-field-clear-range = Clear the range (use all data)
analysis-field-add-range = Add Range
    .hint = Adds a range you can type; you can also drag across the preview with the Zone tool
analysis-baseline-shape-line = Straight line through two points
analysis-baseline-shape-zones = Polynomial on background zones
analysis-baseline-shape-anchors = Curve through anchor points
analysis-baseline-hint-line = Click two points on the preview
analysis-baseline-hint-anchors = Click anchor points on the preview; right-click removes one. Hold Shift to snap to the curve
analysis-baseline-hint-zones = Drag across the preview to mark the zones where there is only background
analysis-baseline-segments = Segments
analysis-baseline-spline = Spline
analysis-baseline-degree = Degree
analysis-baseline-subtract = Subtract
analysis-baseline-divide = Divide
analysis-correct-hint = The reference is interpolated on the X values of the data; points outside it stay empty
analysis-correct-divide = Divide
analysis-correct-multiply = Multiply
analysis-correct-subtract = Subtract
analysis-integrate-hint = Without a range, the whole curve is integrated. Drag across the preview to add ranges
analysis-integrate-zero = Area down to zero
analysis-integrate-line = Area above the line between the bounds
analysis-integrate-cumulative = Write the cumulative integral
analysis-integrate-normalize = Write the normalized integral
analysis-model-linear = Linear
analysis-model-polynomial = Polynomial
analysis-model-exponential = Exponential
analysis-model-exponential-offset = Exponential with Plateau
analysis-model-exponential2 = Two Exponentials
analysis-model-logarithm = Logarithm
analysis-model-power = Power
analysis-model-gaussian = Gaussian
analysis-model-lorentzian = Lorentzian
analysis-model-custom = Custom model
analysis-fit-auto = Auto
    .hint = Fills the starting values with an estimate from the data shown
analysis-fit-model = Model
analysis-fit-degree = Degree
analysis-fit-custom-hint = Write y as a function of x; every other name becomes a parameter
analysis-fit-weighted = Weight by the error column
    .hint = Uses the error column of each series as the standard deviation of its points
analysis-fit-residuals = Write the residuals
analysis-param-initial = Start
analysis-param-fixed = Fixed
analysis-param-min = Min
analysis-param-max = Max
analysis-param-shared = Shared
    .hint = A shared parameter has one value for all series (global fit)
analysis-peaks-shape = Peak shape
analysis-peaks-voigt = Voigt
analysis-peaks-background = Background
analysis-peaks-bg-none = None
analysis-peaks-bg-constant = Constant
analysis-peaks-bg-linear = Linear
analysis-peaks-link-widths = Same width for all peaks
analysis-peaks-center-window = Keep positions within ±
    .hint = Each peak center stays within this distance of the position you placed or that was detected
analysis-peaks-hint = Click the preview to place the peaks yourself; without any, they are detected
analysis-peaks-seed = Peak { $index } at
analysis-peaks-prominence = Minimum prominence (%)
    .hint = Share of the data range a peak must stand out by to be detected
analysis-peaks-max = Maximum peaks
analysis-reconv-hint = Fits a sum of exponentials convolved with the instrument response
analysis-reconv-components = Exponentials
analysis-reconv-shift = Response shift
analysis-reconv-fit-shift = Fit the shift
analysis-direct-hint = Recovers the signal before the instrument response, without a model
analysis-direct-richardson-lucy = Richardson-Lucy
analysis-direct-wiener = Wiener
analysis-direct-iterations = Iterations
analysis-direct-noise = Noise ratio
analysis-output-baseline-column = Write the baseline
analysis-output-peak-columns = Write one column per peak
analysis-output-sum-column = Write the sum of the peaks
analysis-output-to-graph = Add to graph
    .hint = Plots the new columns on the graph that shows this curve, in the same undo step
analysis-output-show = Show
    .hint = Writes the columns only; the values are not kept
analysis-output-keep = Keep in Results
    .hint = Also keeps the values in the Results pane of the workbook
analysis-output-auto = Recalculate automatically
    .hint = Recalculates whenever the source data change; otherwise the columns are marked out of date
analysis-output-low-r2 = Warn when R² is low
    .hint = Opens a dialog after a calculation when the R² of a fit is very low
analysis-r2-low-hint = Low R²: the model may not describe the data well
analysis-output-heavy = A large batch: recalculating each change can be slow
analysis-tool-points = Point
    .hint = Click to place a point; Shift snaps to the curve, Shift + wheel sets the averaging
analysis-tool-zone = Zone
    .hint = Drag to mark a zone or a range
analysis-preview-navigation = Ctrl + wheel zooms, Ctrl + drag moves the view, double-click resets
analysis-warning-step-not-uniform = The X step is not constant: the result may be inaccurate
analysis-warning-irf-step = The response has a different X step than the data
analysis-warning-negative-clipped = { $count } negative points were set to zero
analysis-warning-not-converged = The fit did not converge: the model does not reach the data, check the starting values
analysis-warning-undetermined = { $count } parameter(s) are not determined by the data: no uncertainty is given
analysis-warning-irf-truncated = The response is cut off above 1% of its peak: the missing tail is taken as zero, the lifetimes may be biased
analysis-warning-bridged-reference-gap = { $count } point(s) were interpolated across an empty cell of the reference column
analysis-warning-division-by-zero = { $count } division(s) by zero were left empty
analysis-warning-shifted-x = The constrained polynomial was fitted on X values far from 0: precision may be low. Subtract the mean of X first
stale-series-hint = Out of date: its calculation needs to be recalculated
stale-export-title = Out-of-date data
stale-export-message = This graph shows { $count } calculation(s) that are out of date.
stale-export-anyway = Export Anyway
    .hint = Exports the graph as shown, with grey curves and a warning mark
stale-export-without-warning = Export Without Warning
    .hint = Exports the graph with normal colors and no warning mark
stale-export-recalculate = Recalculate and Export
    .hint = Brings the calculations up to date first, then exports
analysis-calculating = Calculating…
analysis-skipped-rows = { $count } rows without a valid X, Y or error were ignored
analysis-delete-title = Delete calculation
analysis-delete-confirm = Delete the calculation “{ $name }”?
analysis-delete-columns = Its columns can stay as ordinary columns ({ $count } now).
analysis-delete-accept = Delete
    .hint = Deletes the calculation; its columns stay as ordinary columns
analysis-delete-with-columns = Delete with Columns
    .hint = Deletes the calculation and the columns it wrote
analysis-field-x = X
analysis-field-y = Y
analysis-fit-custom-example = a*exp(-x/b)+c

## Results pane (#181)
results-title = Results ({ $count })
results-expand = Show the results
results-collapse = Hide the results
results-scope-sheet = This sheet
results-scope-book = Workbook
results-empty = No kept results here. Run a calculation from the Analysis menu
results-no-values = This calculation produced no values
results-stale-suffix = { $title } (out of date)
results-replace-hint = The correction overwrites the original data.
results-row = { $kind } · { $place }

## Analysis — out-of-date columns (#181)
analysis-stale-manual = Out of date: the source data changed. Recalculate to update this column
analysis-stale-input-missing = Out of date: a source column was deleted. Edit the calculation or detach this column
analysis-stale-failed = Out of date: the last recalculation failed ({ $detail })

## Erreurs du système de fichiers (#216)

error-io-not-found = File or folder not found
error-io-permission-denied = Access denied
error-io-already-exists = It already exists
error-io-storage-full = The disk is full
error-io-read-only = The location is read-only
error-io-invalid-data = The content is invalid
error-io-invalid-input = Invalid location or name
error-io-interrupted = The operation was interrupted
error-io-unexpected-eof = The file ends unexpectedly
error-io-is-directory = This is a folder, not a file
error-io-not-directory = This is a file, not a folder
error-io-timed-out = The operation timed out
error-io-other = { $detail }

## Messages de la barre d'état (#216)

error-window-geometry = Could not update the window position: { $cause }
error-plot-settings-cancel = Could not cancel the graph properties: { $cause }
error-plot-settings-undo-group = Changes to the graph properties cannot be undone together: { $cause }
error-plot-settings-block-detached = Graph properties are open. Apply or cancel them before changing a detached window
error-preferences-not-saved = Could not save the preferences: { $cause }
error-colors-session-only = Saved colors will last for this session only: { $cause }
error-script-vanished = The script engine stopped unexpectedly. Run the script again
error-save-script-running = Wait for the script to finish before saving
error-drop-plot-settings-open = Apply or cancel the graph properties before dropping files
error-import-running = An import is already running
status-import-progress = Importing… { $done } of { $total } { $total ->
    [one] file
   *[other] files
}
error-import-start = Could not start the import: { $cause }
error-import-stopped = The import stopped before the end
error-refresh-no-imported-column = This sheet has no column imported from a file
error-refresh-failed = Could not refresh: { $failures }
error-refresh-group = Could not refresh the columns
error-refresh-close-group = the refresh could not be completed as a single undo step
status-refresh-done = { $count ->
    [one] { $count } column refreshed from its file
   *[other] { $count } columns refreshed from their files
}
error-refresh-partial = { $count ->
    [one] { $count } column refreshed from its file
   *[other] { $count } columns refreshed from their files
}: { $failures }
error-import-nothing-to-reopen = There is no import to reopen
status-import-reopened-undone = Last import undone. Adjust its settings, then run it again
status-import-reopened-kept = Adjust the last import settings, then run it again. The previous import stays in place
status-template-chosen = Template “{ $name }” chosen from the file
status-template-exported = Template exported to { $path }
error-template-export = Could not export the template: { $cause }
status-template-imported = Template “{ $name }” imported
error-template-import-not-saved = Template imported but not saved: { $cause }
error-template-name-required = Give the template a name to save it
error-template-save = Could not save the template: { $cause }
status-template-saved = Template “{ $name }” saved
error-template-delete = Could not delete the template: { $cause }
status-template-deleted = Template “{ $name }” deleted
error-import-choose-file-first = Choose a file before detecting the header
error-export-unknown-format = Unknown export format for { $path }. Choose .svg, .pdf or .png
status-graph-exported = Graph exported to { $path }
error-sheet-gone = This sheet is no longer in the project
error-sheet-no-workbook = This sheet is not in a workbook
error-column-gone = This column is no longer in the project
error-workbook-no-folder = This workbook is not in a folder
error-graph-gone = This graph is no longer in the project
error-layer-gone = This layer is no longer in the graph
error-clear-group = Could not clear the selection
error-paste-group = Could not paste
error-columns-group = Could not change the columns
error-create-group = Could not create the objects
error-import-group = Could not import the files
error-group-close = Could not complete the operation as a single undo step: { $cause }
error-paste-no-cell = Select a cell to paste into
status-paste-absolute-empty = Nothing to paste as absolute: copy from the grid first
status-import-done = { $imported ->
    [one] { $imported } file imported
   *[other] { $imported } files imported
} of { $total }
error-import-partial = { $imported ->
    [one] { $imported } file imported
   *[other] { $imported } files imported
} of { $total }: { $failures }
error-plot-no-y = No Y column selected. Select a Y column to plot
error-add-no-y = No Y column selected. Select a Y column to add
error-script-unavailable = Scripts are not available in this build of StokesForge
error-script-running = A script is already running
error-script-no-python = Python was not found. Set STOKESFORGE_PYTHON_HOME or reinstall StokesForge
status-script-running = Running script…
error-script-start = Could not start the script: { $cause }
status-script-interrupted = Script interrupted
status-script-exception = Script failed. See the console
status-script-done = Script finished
status-script-interrupt-requested = Interrupt requested
status-script-none-running = No script is running
error-import-not-open = The import window is closed
error-template-import-read = Could not read the template { $path }: { $cause }
error-template-import-invalid = The template { $path } is not valid: { $detail }
status-import-dropped = { $count ->
    [one] { $count } file imported with template “{ $models }”
   *[other] { $count } files imported with template “{ $models }”
}

# Champs : valeur ramenée à une borne fixe (#219).
field-clamped-min = Set to the minimum, { $limit }
field-clamped-max = Set to the maximum, { $limit }
field-refused-number = Enter a number
field-refused-integer = Enter a whole number
field-refused-use-auto = Use Auto range
field-refused-name-empty = The name cannot be empty
field-refused-name-taken = This name is already used

# Champ numérique : bulle du ▼ (#220).
number-presets = Common values

## Raisons de grisage (#226) : un élément grisé dit pourquoi et quoi faire

disabled-object-gone = This item no longer exists
disabled-move-first = Already first
disabled-move-last = Already last
disabled-visibility-empty = Nothing to show or hide
disabled-axis-blocked = A layer above already draws this axis
disabled-axis-on-overlay = Not available on a layer drawn on top
disabled-text-not-moved = This text has not been moved
disabled-no-graph-data = This graph has no data to show
disabled-nothing-to-undo = Nothing to undo
disabled-copy-last-format = At least one format must be offered
disabled-nothing-to-redo = Nothing to redo
disabled-new-project-busy = Finish or cancel the running script, import or edit first
disabled-no-source-file = No column of this sheet comes from a file
disabled-no-last-import = No finished import to reopen
disabled-needs-sheet = Bring a workbook sheet to the front first
disabled-no-window = No window to maximize
disabled-import-not-data-line = Only a data or footer line can end the data
disabled-import-auto-end-off = Automatic data end is already off
disabled-import-builtin-template = Only a template you saved can be deleted
disabled-add-file-first = Add a file first
disabled-import-no-active-book = Bring a workbook to the front first
disabled-import-last-step = This is the last step
disabled-import-first-step = This is the first step
disabled-script-running = Wait for the script to finish
disabled-script-unavailable = Python is not available in this build
disabled-script-not-running = No script is running
disabled-color-invalid = Enter a valid color first
disabled-gradient-min-colors = A gradient needs at least two colors
disabled-mirror-off = Turn on the mirror axis first
disabled-analysis-issues = Fix the problem shown in the form first

## Help › Keyboard Shortcuts (#231, S-K5). The lists are generated from the key
## registry (`app/shortcuts.rs`, `HELP`).

menu-help = Help
action-keyboard-shortcuts = Keyboard Shortcuts…
    .hint = Lists the keys and gestures of the application
keyboard-shortcuts-title = Keyboard Shortcuts
action-preferences = Preferences…
    .hint = Sets the application, not the project
preferences-title = Preferences
preferences-general = General
preferences-export = Export
preferences-display = Display
preferences-files = Files
preferences-max-file-size = Maximum file size (MB)
preferences-max-file-size-note = Largest StokesForge file opened. Larger files are refused before they are read, which protects the computer from a file received from someone else.
preferences-analysis = Analysis
preferences-low-r2-dialog = Warn when R² is low
    .hint = Opens a dialog after a calculation when R² is below 0.6. Each calculation can also turn it off from its Output settings.
preferences-new-workbooks = New workbooks
preferences-new-workbooks-note = Rows of the column header shown when a workbook opens. Each workbook can then change its own from its Workbook menu or from its # cell.
preferences-show-metadata-row = Show Metadata Row
preferences-show-x-prime-row = Show x' Row
preferences-copy-graph = Copy Graph
preferences-copy-graph-note = Formats offered together to other applications, in this order. Drag a row to reorder.
preferences-copy-move = Move { $format }
preferences-copy-svg = SVG
preferences-copy-png = PNG
preferences-copy-pdf = PDF
preferences-copy-drag = Drag to change the order.
help-area-general = General
help-area-workbook = Workbook
help-area-graph = Graph
help-area-managers = Project Manager and Object Manager
help-area-fields = Fields and dialogs
help-keys-either = { $first } or { $second }
help-keys-ctrl-wheel = Ctrl+Wheel
help-keys-enter = Enter
help-keys-escape = Esc
help-keys-tab = Tab
help-keys-arrows = Up / Down
help-keys-shift-arrows = Shift+Up / Shift+Down
help-undo = Undo
help-redo = Redo
help-new-project = New project
help-open = Open a project
help-save = Save
help-save-as = Save as
help-import = Import files
help-export-graph = Export the graph
help-run-script = Run the script
help-edit-cell = Edit the cell or the header field
help-select-all = Select all
help-copy = Copy
help-copy-full-columns = Copy the whole columns
help-cut = Cut
help-paste = Paste
help-paste-absolute = Paste at the same place
help-clear-cells = Clear the selected cells, then delete them
help-duplicate-sheet = Duplicate the sheet
help-copy-graph = Copy the graph as an image (nothing selected)
help-delete-object = Delete the selected object
help-move-series-up = Move the selected series up
help-move-series-down = Move the selected series down
help-zoom = Zoom
help-rename = Rename
help-delete-selection = Delete the selection
help-duplicate = Duplicate
help-validate = Confirm
help-cancel = Cancel
help-next-field = Confirm and go to the next field
help-step-value = Change the number by one step
help-step-value-ten = Change the number by ten steps
help-open-list = Open the list of values

## Work in progress (#232)
busy-stop = Stop
    .hint = Stops the work in progress
status-import-stopped = Import stopped. Nothing was imported
status-recalculating = Recalculating…
status-recalc-stopped = Recalculation stopped. The change was undone

analysis-baseline-constant = Constant…
    .hint = Remove a flat background from one point or the mean of a zone
analysis-baseline-shape-constant = Constant (point)
analysis-baseline-shape-constant-zone = Constant (zone mean)
analysis-baseline-hint-constant = Click one point. Shift snaps to the curve using the local mean. Edit X and Y below.
analysis-baseline-hint-constant-zone = Drag one X range. Its mean Y defines a flat background; a new range replaces the previous one.

history-replace-analysis-data = Replace Original Data ({ $kind })
history-restore-original-data = Restore Original Data ({ $kind })
history-restore-original-data-any = Restore Original Data

analysis-output-new-column = New column
analysis-output-replace-original = Replace original data
action-restore-original-data = Restore Original Data
    .hint = Puts back the values from before the correction and overwrites what was typed since. Ctrl+Z undoes it
    .disabled = This correction kept no original values
analysis-reference-copy = copy of { $name }
analysis-reference-line = Reference: { $reference }
analysis-output-replace-hint = The correction replaces the source values. Its settings remain in Results. Undo restores the original data.
disabled-analysis-replace-operation = Only a new baseline or reference correction can replace original data.
disabled-analysis-replace-readonly = Calculated columns and columns with a formula cannot be replaced.
analysis-replace-title = Replace original data
analysis-replace-confirm = Several curves use these data. They will all show the corrected values.
analysis-replace-accept = Replace
analysis-correction-trace = Modified data: { $kind }

# Repère discret des corrections définitives (#212).
grid-column-modified = Original data replaced by a correction. See Results for the correction settings

analysis-needs-column-curve = Analysis requires a curve drawn from a column

# Glisser-déposer des séries, des groupes et des objets libres.
object-drag-end = End of the list

# Column in use by a calculation (#306)
column-use-title = Column in use
column-use-message = This deletion affects { $count } calculation(s):
column-use-formula = { $name } (formula)
column-use-calculation = { $kind } → { $output }
column-use-calculation-bare = { $kind }
dialog-undo =
    Undo
    .hint = Reverses the deletion

# Copy with header on formula columns (#306)
full-copy-title = Copy with Header
full-copy-message = The copied columns contain formulas. Copy their values or their formulas?
full-copy-values = Values
    .hint = Copies the values; the Formula row stays empty
full-copy-formulas = Formulas
    .hint = Copies the formulas too
