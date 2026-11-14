# Changelog

Format inspired by [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow the numbering `MAJOR.MINOR.PATCH`; the V1 criteria are in
[`docs/v1-criteria.md`](docs/v1-criteria.md).

What counts as compatibility from V1 on: the `.stf` file format (a V1 file can
be read back and migrated up to V3, invariant 12). The `pyst` API is alpha
(`pyst.__api_version__` is `0.x`) and promises nothing until `1.0`
([API version](docs/python-and-scripting.md)). The rest of the code has no
compatibility promise.

One section per version, titled `## [X.Y.Z] - date`. The release job
(`scripts/release-notes.sh`) reads the section of the tag's version and
refuses a tag whose section is missing, empty or still "undated" (tolerated
on an `-rcN`).

Who fills in `[Unreleased]`: every change visible to the user adds a line
there, in the commit that carries it. The version commit renames this section
to `[X.Y.Z] - date` and opens a new empty one. A fix cherry-picked to
`release/N.x` also carries its line there. An additive field of the `.stf`
format is announced there (an older version that saves the file again loses it
without a message, see `docs/persistence.md`).

## [Unreleased]

### Added

- Origin detection by content (#509): the application recognises an Origin
  project by its extension or by its first bytes (`CPYA`/`CPYUA`), like the
  CLI and `pyst`, so a renamed Origin file opens the Import from Origin
  window instead of a "not a text file" refusal. A drop of a text file that
  needs a look and an Origin file opens the Import window first and the status
  bar says that the Origin file will open after it.

- Legend text source (#499): Graph Properties > Legend has a « Text from »
  setting that chooses which worksheet header row gives every legend entry
  (Long Name, Comments, x', Units, Short Name, Long Name (Units), Long Name,
  Comments, or Custom… with a Typst template that reads the variables
  `#long-name`/`#ln`, `#short-name`/`#sn`, `#units`/`#u`, `#comments`/`#c`,
  `#x-prime`/`#xp`). A series name you typed still wins; an empty chosen row
  gives an empty entry. A row series can show the cell of a chosen column.
  New graphs take their default from Preferences > Display > New. Additive
  `.stf` fields (an older version that saves the file again loses them);
  `pyst` API 0.6: `set_graph(legend_text_from=…, legend_template=…)`.

- Legend box (#507): the legend gets a frame (shown or not, colour,
  thickness), a background (automatic or a colour with opacity), several
  columns (entries fill down first, then the next column), row and column
  gaps and the length of the line sample, all in points. A graph you create
  now has **no legend frame**; an existing file keeps its frame and its look.
  The colour bar uses the legend's font. Additive `.stf` fields (an older
  version that saves the file again loses them); `pyst` API 0.6:
  `set_graph(legend_frame=…, legend_frame_color=…, legend_frame_width=…,
  legend_background=…, legend_columns=…, legend_row_gap=…,
  legend_column_gap=…, legend_swatch_length=…)`.
  The **Legend** tab of Graph Properties edits all of it (with the font, size
  and colour of the format toolbar), and choosing a **Position** puts a
  dragged legend back in its corner.

- Color bar (#499): a layer can color the curves of one or more series
  groups by a value (x' by default; also Comments, Long Name, Short Name,
  Units, or the value of a column for a row series) instead of by their rank,
  on a linear or log scale, and shows the scale as a bar that replaces those
  groups in the legend. Graph Properties has a new **Color Bar** tab (groups
  in bar order with a typed range and a Typst break label each, a chain
  option, position in a corner, below or right of the frame, orientation,
  ticks, title); the group page has **On the color bar**; the colors of a
  group on the bar are greyed out in the Series window; the bar is dragged
  like the legend and a double-click opens its tab. The value color is
  computed at each draw, never stored. Additive `.stf` field (an older
  version that saves the file again loses it); `pyst` API 0.6:
  `pyst.color_bar(…)` (with `length=` / `thickness=`), and `apply_gradient`
  now retains the stops. A bar inside the plot area shows resize handles when
  selected, like a drawn rectangle: pulling one changes the length and
  thickness of the strip, the texts keep their size; below / right of the
  frame only the thickness handle exists. Additive stored fields, one undo
  step per pull.

- Origin projects (#490): an `.opj` file dropped on the window, picked in the
  Import window or reached by File > Import opens the "Import from Origin"
  window instead of the wizard. It lists the workbooks and sheets of the
  project (several files at once are listed together) with a check box for
  each; Import creates one workbook per checked Origin workbook, named after
  its long name, with the Horiba information in the columns' metadata. Only
  workbooks are read: graphs and matrices are left out. `.opju` is not read
  yet. The CLI and `pyst.import_file` import the `Data` sheets of an `.opj`.

## [1.1.0] - 2026-11-11

### Added

- Import: the six-step Import Wizard is replaced by **one screen** (#488). StokesForge reads every file once, says in sentences what it recognised (delimiter, decimal, encoding, lines of data, x' line, column names, units, long name, sheet name) and shows the file as a grid: click a line number or a value to give it a role, drag the data handles, **File** and **Result** tabs. What it is not sure of is listed under **To check**, with **Mark as Checked** and **Check Now**; the answers are remembered per kind of file (**Answers remembered** in **Manage Templates…**, **Forget** takes one back). The rare settings (date order, leading characters, blocks, missing values, comment prefix, skipped lines) are in an **Advanced** fold; a short first-use guide shows until **Don't show again** (**Show Guide** brings it back).
  - The detector reads **JCAMP-DX** files (not NMR: an NMR file is refused with a clear message, an NMR block of a mixed file is left out and flagged), recognises the instrument from the content, never picks one of several blocks of numbers silently, and guesses a thousands separator only on strict evidence.
  - **Templates** carry conditions (the instrument, the data type, the number of columns, a header line, the file name, the extension): **Save as Template…** proposes a name and shows what triggers the template; **Manage Templates…** edits them. A template that fits is applied at once; when two fit, the one with the most conditions wins and a point to check offers the other. Templates saved before 1.1 are **kept but no longer applied automatically**: their old Auto-select criteria became conditions that are not active, because those criteria were broad (a `.csv` that starts with `ORIGIN;JASCO` fits every JASCO file) and an old template silently misread a file of another shape (a kinetic whose x' line it read as data). Such a template shows **Not automatic** in **Manage Templates…**, stays usable from the **Template:** menu, and **Use Automatically** turns its conditions back on. A template saved from the new window is automatic. The command line and `pyst.import_file` follow the same rule (a named template still works).
  - **Dropping files** imports straight away when nothing needs a look; if a file has a point to check (or cannot be read), the window opens on all the dropped files. **Reopen Last Import…** reopens every file with its own settings.
  - `stokesforge-cli import` and `pyst.import_file` use the same detector and templates (a named template still comes first); the command line skips a file it cannot read and lists it. A sheet takes the title of the file (a `TITLE` key) when it has one, else the name of the file; the **Sheet name** setting in the **Advanced** fold (**Title from the file** or **File name**, saved with templates) makes it always the file name.
  - **Refresh from Source** reads the file again with the stored settings, without detection, and is not promised to give the same result from one version to the next.
  - The window is now titled **Import** (it was the Import Wizard). The `.stf` format stays version 1 and gains optional fields on the stored import settings (`main_until_marker`, `header_props`, `skip_text_rows`, `format`, `name_unit_slash`, `strip_unit_brackets`, `sheet_name_from_file`): a 1.0 that opens and saves such a file again loses them.

- Import: with **One Workbook per File** (or a single file into **New Workbook**), the workbook takes the title of the file (else its name) and its sheet is called **Raw Data** (#488). The sheet keeps that name while no value of an imported column changes. At the first change (a typed or pasted cell, a cleared cell, rows added, deleted or sorted, a column type changed, an x' changed, a formula put on an imported column, an imported column deleted) the sheet renames itself **Sheet1** (the first free `SheetN`) in the same undoable step: Ctrl+Z gives **Raw Data** back. Editing a header (Long Name, Units, Comments), a role, a display format, the order of the columns or adding a calculated column beside is not a change; a sheet you renamed yourself is never renamed again, and **Refresh from Source** keeps the name. The other destinations are unchanged. The `.stf` format stays version 1 and gains one optional field on a sheet (`raw_modified_base`, written only on a sheet that still carries the mark): a 1.0 that opens and saves such a file again drops it and the sheet keeps its name for good.
- Import: **Long Name** and **Comments** can span several sub-header lines (joined with a space), **Units** and **x'** read one line, and x' takes the first number of its text (`t=12.5 s` gives 12.5) (#488). The recipe gains the optional field `join_lines` (`.stf` stays version 1): a project saved before keeps its first-line reading on Refresh from Source, and an older version that saves it again drops the field. Saved import templates all read the new way, and a sub-header line used as Long Name, Units, Comments or x' is no longer listed as file metadata.
- Analysing whole rows of a sheet that has no x' values now works (#493): the
  rows are analysed with the column numbers 1, 2, 3… as x', a **No x' Values**
  window says so and offers **Cancel Analysis**, and the right-click of those
  rows is the rows menu (Plot and Send to greyed out). It used to analyse, in
  silence, the columns the rows cross.

- A transposed sheet has a **Recalculate automatically** box, checked by
  default (#463). Checked, the sheet follows its source by itself; unchecked, it
  is the snapshot it was (banner, **Recalculate**). A value you type in the
  sheet is never overwritten: the whole sheet waits until **Relink** or
  Ctrl+Z. While the box is checked, the source sheet cannot calculate from the
  transposed sheet (a formula, directly or through another sheet, or an
  analysis whose result lands in the source), and the box is greyed with the
  column to fix while one does. Sheets of files saved before this change open
  unchecked. The `.stf` format keeps version 1 and gains the optional field
  `auto_recalc` on the link of a transposed sheet: an older version that opens
  and saves such a file again loses it (the box comes back unchecked, no number
  is lost).

- A detached window or panel says why a gesture did nothing or failed (#471):
  a band at the bottom of that window, for 5 seconds, orange for a refusal or
  a warning, red for an error. While any dialog is open in the main window, the
  gestures of the detached workbooks, graphs and panels are refused with
  "<Dialog> is open in the main window. Apply or cancel it first" (not only
  for Graph Properties as before). A value typed in a detached cell or header
  stays open with its text until the dialog is closed. The refusal is in the
  status bar and the history of the main window, no longer in the footer of the
  dialog. Warnings of the local view of a detached workbook (an unreadable
  clipboard) now show in its window and in the main history.

- Copy, paste and duplicate a folder in the Project Manager (#391): what is
  inside the copy reads the copy (a graph of the copied folder plots the copied
  workbook), and what is outside is still read from the original. Windows copied
  together in one gesture keep each other the same way. Pasting a folder into
  another project brings what it plots.
- An empty **entry row** (marked `*`) is always shown below the last row of a
  sheet (#440), also in a new workbook, which has no row yet. Typing in it
  creates the row, and one Undo removes it. It is not data: not saved, not
  plotted, not part of Select All.
- User manual (#364, #365, #366, #367): the guide chapters, the Shortcuts, Mouse
  gestures and Interface texts references (generated from the application) and
  reproducible screenshots are written, published with each release.

### Changed

- One rule for the status bar messages (#453). Within one action, the most
  serious message stays on the line (a warning is no longer replaced at once by
  the plain message of the same action, for example when adding a sheet that a
  kept formula names, or on Ctrl+S over the size limit); the others are in the
  message list. A warning fades after 30 seconds, an error stays until a later
  action says something (clicking a sheet tab no longer erases it). A plain
  message that no action caused (script finished, recalculation stopped,
  progress) never replaces a warning or an error. The list keeps, per action,
  its last plain message and its warnings and errors; import progress is no
  longer stored in it. Refiled: columns dropped by a paste, and a layout or
  language choice that could not be saved, are now warnings. Refusing a gesture
  in a detached window shows the band each time. In the message list, a long
  message wraps, and the same message said again directly after is one line with
  a count (`Nothing to undo (×13)`) at the time of the last one, so a repeat no
  longer pushes older messages out of the 50. "Nothing to undo" and "Nothing
  to redo" are plain messages (no colour), so a detached window shows no band
  for them.
- Only the workbook and the graph detach (#473): a note and a matrix stay in
  the main window. Their title bar already had no detach icon; the code now
  refuses it too. The manual no longer says that dragging a title outside the
  application detaches a window (no such gesture exists).

### Fixed

- A number being typed in a field is entered on its own target when another
  target is clicked (#498): typing a From on the Bottom axis, then clicking the
  Left tab, sets Bottom and leaves Left unchanged (before, Left received the
  number).
- Legend entries with the same text all show their text on screen (#500):
  before, only the last one did, the others kept their line and symbol with no
  text. The export was already right.
- The third-party notices (`THIRD-PARTY-LICENSES.txt`) are readable by every
  user (#486): the file was written with mode 600 and an installation by the
  RPM, the DEB or `install.sh --system` kept it, so a normal user could not
  open it. The RPM and the DEB now declare the glibc of the archive (2.35):
  an older system refuses the package instead of failing at start.
- **Recalculate**, **Relink** and Analyze Rows on a checked transposed sheet no
  longer empty a row while a column of the source calculates from that sheet
  (#484). They are refused and say why (Column C of Sheet1 calculates from
  Sheet1ᵀ. Remove that formula to follow): the sheet keeps its last numbers and
  stays out of date. Relink says so before asking for confirmation. An
  unchecked sheet is not concerned.
- Enter in a cell of a detached workbook moves the cursor to the row below, as
  in the main window (#470): the move was undone by the window's own selection.
- Ctrl+Z, Ctrl+Y and Ctrl+S now work in the detached windows (#459): the graph,
  the workbook, the Project Manager, the Object Manager and the console undo,
  redo and save the project, even with the cursor in a cell or a field (the
  typed text stays, the last command is undone, as in the main window). A
  dialog open in the main window refuses them with the band. Ctrl+Shift+S saves
  nothing there. Ctrl+Q in a detached window no longer closes it (a typed cell
  seemed lost): nothing closes and the band says "Quit is in the main window".
  A name field of the Project Manager closes when its folder is removed by
  Undo. Help › Keyboard Shortcuts and the manual say that only Undo, Redo and
  Save work in a detached window.
- A dialog open in the main window now stops the keys of the detached windows
  (#460): with Graph Properties (or any dialog) open, Esc, `G` and the arrows of
  a detached graph in Page Layout Mode, and the keys of a detached workbook, the
  Object Manager, the Project Manager and the console, do nothing, and the band
  of that window says why. Letters typed in an open cell or field keep working.
- A detached window can be grabbed by its title, or resized by its edges, every
  time (#457): before, on Wayland, every second grab did nothing, and moving a
  window could then extend the selected columns of a sheet when the mouse
  passed over the column letters. Present on `main` too.
- Esc closes a dialog that is open over a visible workbook (#406): the grid
  no longer takes the key first. While a dialog is open, the keys of the grid
  and of the Project Manager stand aside.
- Saving includes what you are still typing (#465, #466): Ctrl+S, Save As and
  the **Save** answer of the Unsaved Changes question first enter the open
  cell, header field or note, in a detached window too, and the graph title,
  legend name or free text being edited. A number the cell refuses stays open
  and is left out of the save.
- Undoing or redoing a group of changes over formula columns or calculation
  results no longer stops half way (#448): the columns are made to fit and
  recalculated. A manual calculation whose result had to be refitted is marked
  out of date.
- Inserting or deleting rows beyond the last row of the data, when that moves a
  series plotted by rows, now adds its Undo step (#436).
- The axis line, the edge of the plot area, is a target like the numbers of the
  axis (#385): double-click it to open Graph Properties on that axis, even when
  a curve runs along the edge.
- Tick labels never overlap (#397): the number of labels follows the length of
  the axis, so a short axis writes fewer numbers.
- On Windows, dropping files from Explorer onto the application reads the real
  position of the cursor (#383), so the drop reaches the window under it.

### Changed

- The logo in `assets/logo/` is now under GPL-3.0-or-later, like the code (#450).
- Project Manager (#423): a click selects a window or a folder and opens
  nothing; a double-click opens a window or opens or closes a folder, and
  Enter does the same on the selection. There is no more slow click to rename:
  use F2. Left and Right close and open a folder, Shift with Up or Down
  extends the selection, **Cut** then **Paste** moves entries into the
  selected folder in one Undo step, and a copy keeps its name when the
  destination folder does not use it. While the panel has the focus, Ctrl+C,
  Ctrl+X, Ctrl+V, Delete, F2, Ctrl+D and the arrows act on it, not on the graph
  or workbook in front.
- Layer numbers sit at the top left of the layer's own cell (#395), above its
  plot area, instead of at the corner of the graph window. Overlay layers of one
  cell have their badges side by side.
- **Graph Properties** > **Axes** (#396) lists the four sides, Bottom, Left,
  Right and Top, on every layer, each axis named where it is drawn. A side that
  another layer of the same plot area already uses is greyed, with the reason on
  hover. An overlay layer has an **Own scale** box on its Top and Right sides.
  A mirror axis and an overlay axis cannot share a side.
- Primary tabs are on top and secondary tabs are vertical at the left (#393):
  the Preferences rubrics are across the top and the tabs inside a page (Display,
  the axes of Graph Properties) are listed at the left.
- Secondary text (help, reasons) is one step smaller than body text, 11 pt, and
  empty states or labels beside a result are dimmed at body size (#392). The
  header rows of a sheet (x', Long Name, Units, Comments, F(x)) are no longer
  dimmed.
- In a cell, a refused entry or a formula in error tints the cell instead of
  drawing a red frame (#410). The **?** icon of the F(x)= help sits in the next
  cell, so the field keeps the whole width of its cell.
- Pages that scroll keep a margin under their last control (#347).
- The history entry of **Refresh from Source** reads "Refresh from Source", like
  its menu (#451).
- Opening a menu and moving the pointer over another menu title opens that one
  without a click (#343). With no menu open, hovering opens nothing.
- The Formula Functions help and the manual say that `mod(-1e-20, 3)` is 3,
  like Python and Excel (#412).
- A transposed sheet over a formula column no longer says "Out of date" when
  the formula still gives the same numbers, nor after reopening the file (#439).
  The recalculation now keeps the version of a formula column it recomputes to
  the same values. Pasting the same number in the source, or changing a value
  and undoing it, still says "Out of date" until you recalculate; typing the
  number a cell already holds writes nothing, as before. Transposing
  again or recalculating an up-to-date sheet no longer adds an Undo step.
- Pasting a formula that cannot be read behaves like typing it (#433): it is
  kept, framed in red with the reason, and its column is emptied (the copied
  values are never pasted in its place); the other columns of the paste go
  through, one **Undo** restores everything, and it calculates by itself when
  the missing column or sheet appears. The status bar says "The formula of
  column C does not compute: …". A pasted comma range is rewritten and shown
  in orange exactly like a typed one. Scripts (`pyst`) stay strict.
- Status bar warnings use one amber colour, further from the error red.

### Added

- A free graph can store a page (#426), the minimum size of its figure; it is
  an optional `.stf` field (an older version that saves the file again drops it
  without a message). Nothing is visible yet.

- A detached graph window in the Page Layout Mode (#426, #457) is maximized
  with the mode and restored when you leave it, like a graph window in the main
  window, and its **Maximize the window** button, pressed while it is maximized,
  restores it and leaves the mode. On Wayland a detached
  window covering the main window no longer freezes the application: what you
  do in the detached window still takes effect, about a tenth of a second late
  while the main window is hidden.
- A dialog opened from a detached window (Graph Properties, About, the import
  window, a confirmation...) always appears in the main window (#472, which
  replaces the opposite choice of #457). The detached window shows an orange
  band for 5 seconds that names the dialog, and the main window is asked to come
  forward (it does on Windows and X11; on Wayland use the band, the system's
  notice or Alt+Tab). The rule: a content feature works in every window, a global
  feature stays in the main window; object properties (rectangle, arrow) are the
  one exception and stay in their graph's window. Ctrl+Z in a text field of a
  dialog does not reach the model.
- Page Layout Mode (#426): **Layout ▸ Page Layout Mode**, and a toolbar button,
  show a graph as a page with rulers, a grid, handles and a settings strip
  (figure size with a padlock shared with Graph Properties, position and size of
  the selected graph, Align, Same Size, Distribute, Snap, **Grid mode**, and
  when axes are added). Entering or leaving the mode changes nothing in the
  file; **Grid mode** is the layer placement, one undo step. The padlock of the
  figure size now belongs to the graph for the session instead of to the
  Properties window, and two labels of Graph Properties are renamed: the total
  is **Figure size**, the Advanced choice is **When axes are added**.
  In **General**, **Figure size** now comes first in **Size**; the plot area
  sizes (and Left and Top, or the four fields of a Free layer) moved into the
  closed **Advanced** block. Nothing changes in the file.
- A formula kept because a column or a sheet it names does not exist yet
  computes by itself when that column or sheet appears (#433): add the column,
  move one in, rename a sheet, delete a duplicate workbook, or open the file.
  The binding is part of the history entry of that action (one **Undo** takes
  both) and the status bar says which column the formula now reads. A binding
  that would make a loop stays framed. Once bound, the formula holds the column
  by identifier, so moving it later gives no `#REF`.
- Application icon (#388): the StokesForge logo is the icon of the window, of
  the Windows `stokesforge-app.exe` and of its installer, of the menu entry and
  of the `.stf` files (a landscape sheet with the logo), and sits at the top of
  the About window. The same picture at every size. It is in the official
  packages and installers only: the code builds and runs without it, and its
  notice (`licenses/logo/LICENSE`) allows its use with the unmodified software
  only (the wording is a draft, to be reviewed before the release). All windows,
  detached ones included, now carry the application id `stokesforge` that the
  menu entry matches.
- RPM and DEB packages are published with each release (#390, #388): the
  publication switch `PUBLISH_LINUX_PACKAGES` is now 1, so a failure of the
  package jobs blocks the release. The manual says how to install them and that
  a per-user `install.sh` copy shadows the package. `install.sh` also installs
  and removes the icon of the `.stf` type.

- A new graph plotted from a workbook is named after it (#434): "Book1 plot",
  then "Book1 plot 2"… (numbered after the highest existing number, a deleted
  graph leaves a gap) instead of Graph1, Graph2. `pyst.plot` without `name`
  does the same. A graph without series is still named GraphN.

- Project Manager (#425): drag rows to move them. Drop on a folder to put them
  inside it, between two rows to reorder, below the last row to move them to the
  root. A closed folder opens after a second under the pointer, Escape cancels,
  one Undo gives it all back. Every folder is now shown sorted (folders first,
  then by name, Fit2 before Fit10); dropping between rows puts that folder in
  manual order, and right-click > Sort (on the folder, or on the empty area for
  the root) sorts it again. Copying or duplicating a manual-order folder keeps
  the manual order in the copy. The `.stf` file gains an optional list of
  manual-order folders: an older version that opens and saves the file again
  loses those orders.

- Set the total size of a graph (#407): in Graph Properties > General > Size,
  **Graph size** is now two fields, width and height, in the unit of the
  Preferences. Typing one resizes every plot area in proportion (the axes,
  margins and title keep their size; in Free placement the positions scale too),
  the plot area fields follow, and one typed value is one **Undo** step. A size
  the axes cannot fit is brought back to the nearest possible one (orange field,
  bubble with the minimum). A padlock beside the fields keeps the height / width
  ratio of the image taken when it is closed; it is window state, never stored
  in the file. `pyst`: `figure_width` / `figure_height` in `set_graph` (API 0.4).

- Free placement of graph layers (#398), first batch: Graph Properties >
  General > Size > **Layer placement** switches between **Regular grid** and
  **Free**. In Free each layer has its own plot area, set with **Position, left**,
  **Position, top**, **Plot area width** and **Plot area height** (positions are
  page coordinates: read back as typed, unchanged when another layer moves,
  possibly negative); the image is the bounding box of the layers. Switching to Free moves
  nothing; going back to the grid puts layers side by side in a row and gives
  each column and row its largest plot area. **New Layer Beside** places the new
  layer next to the active one without overlap, **Delete Layer** moves no other
  layer. No mouse gestures yet (they come in a later batch). **Additive `.stf` fields** (`layer_placement`,
  and `free_frame` on each panel): an older version that opens and saves a file
  again drops them without a message and shows the grid.

- Layout menu for free layers (#398), third batch: a new **Layout** menu after
  Graph (and **Layout ▸** in the right-click of the figure) holds **Layer
  Placement** (Regular Grid / Free), **New Layer Beside** (moved from Graph),
  **Align** (Bottom, Top, Vertical Center, Left, Right, Horizontal Center),
  **Same Width**, **Same Height**, **Distribute Horizontally / Vertically**,
  **Bring to Front**, **Send to Back** and **Delete Layer**; in Free placement
  only the arranging entries apply. **Shift + click** a layer (or its number
  badge) to select several layers (a second Shift + click removes it): the
  arranging entries then act on the selection, with the active layer as the
  reference, or on every layer when fewer than two are selected. Each is one
  step of **Undo**. A curve hidden by a layer in front no longer shows selection
  handles under it, a detached graph window follows the image when a layer
  goes left of or above its origin (not on native Wayland, where a window
  cannot be placed), and Ctrl+Z pressed in the very frame a drag is released
  undoes that drag.

- Mouse gestures of free layers (#398), second batch (second part): with
  **Layer placement = Free**, drag a layer's plot area with the Pointer tool to
  move it, and drag the outer boundary of a layer (edges and corners) to resize
  it; left and top edges keep the opposite edge still, sizes stay within 36 and
  2000 pt. Edges snap to the edges of the other layers (6 screen points, a
  dashed guide shows it); hold **Shift** for no magnet. Each gesture is one
  step of **Undo** ("Move Layer", "Resize Layer"), and the graph window follows
  the image when a layer goes left of or above its origin (and on Undo / Redo),
  so the other layers stay still on screen. The regular grid is unchanged.

- Stacking and background of free layers (#398), second batch (first part):
  overlapping layers are drawn in creation order (a new layer starts in
  front); right-click a layer or its Object Manager node for **Bring to Front**
  and **Send to Back** (layer numbers are kept), and set a layer's
  **Layer Background** in General > Size. **Auto** paints only where a layer covers
  another one (white, or the graph's color), so a graph without overlap and
  transparent exports are unchanged. A click on the shared area picks the front
  layer, and what lies under a front layer cannot be picked. **Additive `.stf`
  fields** (`stack` in `free_frame`, `layer_background` on each panel): an older
  version that saves the file again drops them without a message.

- `pyst` graph sizes (#399): `pyst.set_graph` gains `plot_width`, `plot_height`
  (plot area of the column / row of `panel`), `plot_left`, `plot_top` (`None` =
  Auto) and `fixed_size`, with a `unit="cm"|"mm"|"pt"|"in"` keyword (centimetres
  by default, never the Preferences unit); values out of range are refused. The
  new `pyst.graph_size(graph, unit="cm")` reads the image size, the plot areas,
  Left / Top and Fixed. `pyst.__api_version__` is now `0.2`.

- `pyst` total image size (#407): `pyst.set_graph` gains `figure_width` and
  `figure_height` (the whole image, in `unit`): the plot areas follow in
  proportion, the axes, margins and titles keeping their size; a value out of
  the range the graph allows is refused with that range. They cannot be
  combined with `plot_width` / `plot_height` in one call.
  `pyst.__api_version__` is now `0.4`.

- `pyst` free placement (#411): `pyst.set_graph` gains `layer_placement="grid"|"free"`
  (applied first in the call), `layer_left` and `layer_top` (position of the layer of
  `panel` in page coordinates, in `unit`, may be negative); in Free `plot_width` and
  `plot_height` size that layer alone and `plot_left` / `plot_top` are refused.
  `pyst.graph_size` adds `layer_placement` and `layers` (left, top, width, height per
  layer). `pyst.__api_version__` is now `0.3`.

- Column formulas (#401): the `^` operator, comparisons (`<`, `<=`, `>`, `>=`,
  `==`, `!=`, true = 1) and row-wise functions in **F(x)=** (`sqrt`, `root`,
  `exp`, `ln`, `log`, `log2`, `logx`, `abs`, `sign`, trigonometry, `round`,
  `floor`, `ceil`, `mod`, `min`, `max`, `if`, `pi()`). A name followed by `(`
  is a function, a lone name stays a column. The Fit gains `round`, `mod` and
  `root`, accepts `pi()` and no longer cares about the case of function names.

- Column formulas (#403): a formula reads a column of **another sheet or
  workbook**: `max(Sheet3!B)`, `B / at(Sheet3!B, 340)`,
  `integral([Book2]Sheet5!B, 10, 100)` (quotes for names with a space or a
  special character, `'Blanc 2'!B`; a workbook is always followed by its sheet).
  The formula follows the sheet when it is renamed or moved, shows `#REF` when
  it is deleted, and a loop between sheets is refused at typing. Two siblings
  can no longer differ only by case. **Additive `.stf` field**
  (`formula_remote` on a column, written only for such a formula): an older
  version that opens and saves a file again drops it without a message, and the
  formula then shows `#REF`.

- Column formulas (#402): functions that give one value for a whole column,
  usable in a row-by-row formula: `at(B, 340)` (linear interpolation at an X,
  empty outside the data), `max`, `min`, `mean`, `sum`, `std` (n - 1) and
  `integral(B)` (same area as the Integration tool), each with an optional X
  range (`max(B, 330, 350)`, bounds in any order, or columns read row by
  row). Example: `B / at(B, 340)` normalizes a spectrum. The X is the column
  paired with B by the rule of the plot; without one the cells show `---` and
  the **F(x)=** cell is framed in red. `max(B)` is the column, `max(B, C)` stays
  row by row, three values are a range. The first argument must be a column
  letter (`max(B*2)` is refused).

- Graph size (#394): the size of a graph is the size of its **plot area**, one
  width per grid column and one height per row, in points, set in Graph
  Properties > General > Size (in the unit chosen in Preferences, centimetres by
  default). The axes, their numbers and the title are added around it, so adding
  or removing one grows or shrinks the image, never the plot area; the plot
  areas of a column and of a row line up. A new graph is 480 x 360 pt; a layer
  added Beside or Below has a plot area of the same size as its reference layer
  and the figure grows. The screen, the exports and the copy of the figure all
  use the same size. A graph from an older file is drawn as before; its first
  edit fixes the plot areas it was drawn with. New additive `.stf` fields on the
  graph (`frame_widths`, `frame_heights`; absent = the old 480 x 360 pt shared
  equally): an older version that saves the file again loses them without a
  message. The outer right and bottom boundaries of a layer are drag handles that
  resize its column or row (one undo step), and the graph window grows or
  shrinks with the graph at the same display scale.
- Fixed figure size and plot area position (#405): Graph Properties > General >
  **Advanced** (closed by default) has **Figure size**: **Grows with the axes**
  (default) or **Fixed**. With Fixed the image keeps room for one axis on each
  side from the start, so adding or removing an axis, a mirror axis or an
  overlaid layer leaves the image and the plot area exactly as they are (for
  slides that switch from one graph to the next). An axis beyond that room, or a
  title (graph or layer), still makes the image bigger, and says so in the status
  bar and under Size. **Size** also gets **Plot area, from the left edge** and
  **from the top edge** (empty = Auto): where the first plot area sits in the
  image, never below what the axes (and the title, for the top) need. Dragging
  the left or top edge of the figure sets them (one undo step; the window grows
  to the left or upward so the plot area stays still on screen). New additive
  `.stf` fields on the graph (`fixed_figure`, `figure_left`, `figure_top`; absent
  = Grows with the axes, Auto): an older version that saves the file again loses
  them without a message.
- Error bars (#360): the first Y Error (X Error) column to the right of a Y
  column, before the next Y column, is paired with it at plot time, and the
  **Y error** / **X error** fields of Series Properties change or remove it
  ("None"; per series, not copied to a group). The bars are now drawn on every
  plot type (Line, Scatter, Line + Symbol, Bar, Error Bars), not only on Error
  Bars. `pyst.plot` pairs the same way unless `x_error=` / `y_error=` is given
  (`None` goes without). A new fit on a series with an error column starts with
  "Weight by the error column" ticked. In the demo, the Uncertainty column now follows Decay.

- Series Properties (#358): a **Y axis: Left / Right** field attaches a series to
  the right axis of its layer (Right shows the axis; "Whole group" copies it to
  the members). It is grayed out on a layer drawn on top.

- Help for the **F(x)=** cell (#404): a **?** icon at the right of the cell while
  it is edited lists the functions by family with an example each (preference
  **Show F(x)= help** in Preferences > Display > Interface). A misspelled function
  name proposes the nearest one, and a wrong argument count says what the
  function takes in words ("takes 1 or 2 arguments", "takes no arguments") and
  names the function to write instead (`log(B, 2)`: `logx`; `atan(A, B)`:
  `atan2`; `sqrt(A, 2)`: `root`).

### Fixed

- A result column with **Recalculate automatically** off (#458) is now marked out of
  date when rows are inserted or deleted in its source sheet. Undo restores the rows
  and leaves the mark until **Recalculate**, like an edit followed by Undo.

- Window names (#437): a folder named `Book1`, `Graph1`, `MBook1` or `Note1` next to the place where a window is created no longer refuses the new unnamed window; the automatic name moves on to the next free number.

- Column type (#431): the Type entries (right-click on a column letter, Workbook > Columns) are greyed, with the reason as tooltip, on a column computed by a formula or produced by a calculation (they used to answer "Change Type" and be silently undone by the next recalculation). On a selection that also holds other columns, only those change. `pyst.set_column_type` raises `InvalidValueError` on such a column.

- Formulas (#428): `round(x, n)` rounds the shortest decimal representation of
  the stored value, like Excel: `round(1.005, 2)` is 1.01 (it was 1), and
  `round(2.675, 2)` is 2.68.
- A transposed sheet whose source holds a formula column is no longer shown stale
  right after opening a file where it was up to date (#430).
- Pasting a Formulas copy of a column no longer ends on the error "This column is
  computed by a formula": the frozen values are not written into a column whose
  formula was just pasted. Pasting a Formulas copy on a data cell of a column now writes
  the formula into the column, like on its F(x)= field, instead of shifted frozen values (#432).
- Formulas (#429): `logx` of an exact power is exact (`floor(logx(1000, 10))` is
  3, `logx(243, 3)` is 5); `erf` and `erfc` keep their relative precision
  everywhere (within 2 ulps on the Boost.Math tables, `erfc` subnormal up to
  27.2, `erf(-0)` is -0); `mean`, `sum` and `std` of a whole column or a range
  use compensated sums (the mean of ten 0.1 is 0.1, the `std` of a constant
  column is 0) and no longer lose a representable result to an intermediate
  overflow (`mean` of two 1.7e308); a number typed or pasted in a formula or a cell
  beyond the `f64` range (`1e400`) or too small to be stored (`1e-400`) is refused with a
  sentence (the cell stays open; a pasted block holding one is refused as a whole; imported values are not checked),
  while a stored formula holding one still loads.
- A custom Fit model typed with a number beyond the `f64` range (`1e400`) or too
  small to be stored (`1e-400`) is refused at entry with the same sentences as
  formulas (**Create** stays greyed; `pyst.analysis.fit` raises and
  `readiness` says `bad_expression`), while a stored model holding one still
  loads and recalculates as before (#438). Once you leave it, the Custom model field is framed in red
  with the reason for every model error (the text stays, an empty field is not
  framed); a stored calculation holding such a number must be corrected before
  Edit Calculation or a range-handle drag works.
- Peak Decomposition window (#384): the preview no longer overflows the window
  (the last tick read "70" instead of "700") and "Keep in Results" no longer
  wraps; the settings column is wider and the preview takes the width that
  remains.
- Formulas (#427): `at(B, x)` on a stored X now returns the stored value exactly
  (it could be off by a rounding error, and `B / at(B, x)` was not exactly 1);
  the same exact read applies to corrections by a reference, integration
  bounds and segment baselines.

### Changed

- Behavior change (#426): resizing and moving graphs with the mouse now happens
  only in the **Page Layout Mode** (**Layout ▸ Page Layout Mode**). The old
  gestures of the Pointer tool are removed: dragging the outer boundary of a
  cell (#394), dragging the left or top edge of the figure (#405), and moving or
  resizing a layer of a Free graph by dragging its plot area or its edges
  (#398), together with their resize pointers. Outside the mode a plain drag in
  the plot area of a Free graph now does nothing, as on a Regular graph. Texts,
  legends, shapes and annotations are still dragged (and their width handles
  pulled), Shift + click still selects layers, and **Ctrl** or middle-button
  drag still pans. The fields of Graph Properties, **Undo** and `pyst` are
  unchanged, and files that store Left, Top or layer frames open and draw the
  same. The history entry "Position Plot Area" is gone.
- Formulas (#433): `0/0`, infinity minus infinity and zero times infinity are
  a failed calculation (`---`), like `ln(-1)`; `1/0` stays infinity. An infinite
  bound of a range means "to the end" (`max(B, 330, 1/0)`), and `integral(B, 2, 2)`
  is 0 when the column has a point. A sheet that holds only constants or
  formulas of other sheets has one row. A formula column copied alone into its
  own sheet (`pyst.duplicate`) reads the same columns.
- Refresh from Source with a shorter file shrinks a sheet that has formula
  columns; they are recomputed on the new length (#433).
- `pyst.set_column_meta(formula=None)` removes the formula and the values it
  computed, like the interface; `keep_data=True` keeps them. `pyst` API 0.5
  (alpha) (#433).
- Formulas (#433): a formula you type in the **F(x)=** field is now always kept
  (Enter, Tab or a click elsewhere closes the field, Esc cancels). When it
  cannot be calculated (unknown column or function, wrong argument count, loop,
  `#REF`, syntax error such as `A +`, number too large) the text stays as typed,
  the column is empty, the cell is framed in red with the reason, and Ctrl+Z
  brings the old formula back. A column the formula could read still follows
  moves. A loop is kept uncalculated, its columns framed. Unreadable and
  looping formulas leave empty cells instead of `---`. A syntax error has one
  short sentence ("The formula is incomplete or not valid…"). A range written
  with a decimal comma (`max(B,1,5)`) keeps its range, is rewritten
  `max(B, 1, 5)` and shows orange for 10 s with the reason, also in the status
  bar. Pasting a formula that cannot be read writes nothing into its column
  (not the copied values either), the other columns go through and the status
  bar names the column. `pyst` raises `InvalidValueError` for every badly
  written formula, `A +` included, and writes nothing.

- The axes are named **Bottom / Top / Left / Right** everywhere in the interface
  (Bas / Haut / Gauche / Droite in French), instead of "X (bottom)", "Secondary Y
  (right)"...: axis tabs, context menu, Object Manager (#358).
- Preferences window (#148): the rubrics (General, Display, Export) are now
  vertical tabs at the left, and Display has the sub-tabs Interface, Object
  Manager and New. Display › Object Manager sets the same lists and
  expand-by-default groups as the panel's menu; Export sets SVG Text and PNG
  Resolution, as Graph › Export does.
- PNG Resolution (#148) is now remembered from one session to the next (it came
  back to ×4).
- Analysis windows (#148): validating a window no longer changes the default of
  Values (Show / Keep in Results) for every analysis. Each analysis remembers its
  own last choice; one never run starts on the new default in Preferences ›
  General › Analysis.
- Linux archive (#389): it is now built on Ubuntu 22.04 and checked in CI, so it
  runs on glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later).
  RHEL, AlmaLinux and Rocky Linux 9 are not supported.

### Added

- Default folders (#148): **File ▸ Preferences… ▸ Folders** shows the configuration
  folder (with **Open Folder** and its files) and the folder of the recovery copies,
  and sets a default folder for **Open and Save**, **Import files** and **Export
  graphs**, typed or picked with **Browse…**. A file dialog starts in that folder the
  first time you use it in a session, then in the last folder you used; an empty field
  keeps the old behavior, and a folder that no longer exists is skipped (the page says
  "Folder not found"). Save As of a saved project starts in the project's own folder.
- Panels (#148): StokesForge now remembers, for the Project Manager, the Object
  Manager and the Script Console, whether each is collapsed or detached and the
  size and position of its window. At launch, a detached panel comes back in its
  window only if the computer has the same number of screens as when it was
  saved, otherwise it comes back attached. On Wayland the position cannot be
  read or set: the window comes back at its size and the system places it.
- Preferences (#148): **Show formula syntax help** (Display › Interface) hides the
  help icon of the formula fields, and **Decimal comma on tick labels** (Display ›
  New) makes new graphs write 1,5; existing graphs and graphs made by scripts do
  not change.
- Histogram and bar series (#359): **Series Properties** has a **Bins** field
  (1 to 65536) for a Histogram series and a **Width (X units)** field for a Bar
  series, Auto by default (gray "Auto (computed width)"; empty the field or type
  `auto` to go back). In "Whole group" scope both are copied to the members of
  the matching type.
- Grid of layers (#357): **Graph ▸ New Layer Beside ▸ Right / Below** adds a
  layer next to the active one, columns dropped on the right or bottom edge of a
  graph create a layer beside the nearest one, and **Delete** in the Object
  Manager removes a layer (with its overlay layers) or an overlay layer alone.
  One undo step each.

- Version display (#382): **Help ▸ About StokesForge** shows the version, the
  license and the project page, with a **Copy** button for a line to paste in a
  bug report; `stokesforge-app --version` and `stokesforge-cli --version`
  (`-V`) print it.

- `pyst.__api_version__` (#356): the `pyst` API now has its own version,
  `"0.1"`, and is marked alpha (the V1 promise that scripts keep working is
  withdrawn until API `1.0`). CI refuses a surface change without a bump and
  a frozen snapshot `tests/api-witnesses/vX.Y.txt`.

- Linux installer (#387): the archive contains `install.sh`. `./install.sh`
  installs for the current user (program in `~/.local/share/stokesforge/`,
  command `stokesforge`, menu entry, `.stf` in **Open with**), `--system` for
  all users in `/opt/stokesforge` (with `sudo`), `--keep-previous` keeps the
  previous version beside the new one, `--uninstall [X.Y.Z]` removes it. It
  never touches projects, preferences or recovery copies.

- User manual skeleton (#362): `manual/`, built with mdBook, attached to each
  release and published on GitLab Pages for final versions.

### Removed

- No built-in import templates (#378): the five templates the program carried
  (Jasco, AnalytikJena, RPE, generic CSV) are gone from the application, the
  CLI and `pyst`. The first launch starts with an empty template list; only
  the templates you saved exist. `pyst.import_file(recipe=...)` and
  `stokesforge-cli import --recipe` refuse the former names («Jasco
  (point-virgule)», «Jasco (tabulation)», «AnalytikJena (sans en-tête)»,
  «RPE (tabulation)», «CSV avec noms de colonnes»). Templates embedded in
  `.stf` files are unaffected: a file imported with one of them still
  reimports.

### Added

- French guard (#377): `scripts/french-guard.py` (and its tests,
  `scripts/french-guard-test.py`) fails CI on French text outside the exempt
  places or a `lang: fr` marker. Run it locally with
  `python3 scripts/french-guard.py`; contributors mark deliberate French test
  data as described in `docs/tests.md`.
- Language (#380): our own files under `vendor/` (fork notes, comments, notices) are
  translated to English, and the French guard no longer exempts `vendor/`.
- Interaction language setting (#377): one line in `CLAUDE.md` and `AGENTS.md`
  sets the language agents use to talk to the developer; the repository stays
  English.
- Minor ticks on axes (#344): a "Show minor ticks" box per axis (on in a new
  graph, off in an existing file), alone or together with the major ticks, the
  number of minor ticks between two ticks, and "Label every" n to write only
  one tick label out of n. Additive fields of the `.stf` format
  (`minor_ticks`, `minor_tick_count`, `label_every`): an older version that
  saves the file again loses them.
- Collapsible "Advanced" block in the Ticks section (#344), closed by
  default, per axis (mirrors included): length of the major and minor tick
  marks (px), thickness (pt) and color of the tick marks, all Auto until they
  are touched (the Auto minor tick mark is half the major one). Additive
  fields of the `.stf` format (`tick_length`, `minor_tick_length`,
  `tick_width`, `tick_mark_color`): an older version that saves the file again
  loses them.

### Changed

- Drop target (#386): while you drag files or columns, the border of the one window that will receive them turns light blue (no more green line inside the workbook, which overlapped the scrollbar); over the workspace only the main window is outlined.
- Repository in English (#377): docs, skills and scripts are translated, and
  renamed accordingly: `scripts/ci-status.sh`, `versions-aligned.sh`,
  `release-publish.sh`, `release-upload.sh`, `release-tag-increasing.sh`,
  `release-tag-on-main.sh`, `perf/measure-windows.sh`; most `docs/` files
  (`docs/chantiers/` is now `docs/work-packages/`); the skills
  `stokesforge-command`, `-explain`, `-rendering`, `-ui-texts`; the crate
  `stokesforge-robustesse` is now `stokesforge-robustness`.
- The demo project is now in English (#379): folders, workbooks, sheets,
  column titles, units, graph names, axis labels, series names and the
  note (`Workbooks`, `Analysis`, `Spectrum`, `Kinetics`, `Panels`,
  `PlotTypes`, `Groups`, `Rows`, `FitGaussian`…), whatever the interface
  language. Absorbance carries no unit; arbitrary units read `a.u.`.
- Axes page of the properties (#344): the "Reverse axis" and "Auto range"
  boxes disappear. The direction of the axis is read from the order of the
  bounds (From > To inverts the axis); From and To are always editable, follow
  the data until something is typed, and typing freezes both bounds (the
  "Auto" button of the row gives them back to the data, direction kept). The
  ticks have their own "Ticks" section.
- When a `.stf` file is opened, an axis that carries `inverted` next to
  increasing bounds has its bounds swapped (the `inverted` field is still
  written in the file, the figure does not change); the `pyst` surface
  (`x_inverted`, `x_auto_range`…) does not change, but `x_inverted=True` also
  swaps the bounds.
- The "Closed Frame" box (Axes page and layer menu) disappears from the
  interface; its settings are made per axis (mirrors, ticks).
- The default name proposed when exporting a graph without a name is
  "graph" (was "graphe") (#374).

### Fixed

- Texts written in French in the English interface (#369): the "(broken
  reference)" placeholder and the "… and N more" line of the legend, the
  Light and Dark theme names, the import failure messages of the status
  bar and the tooltips of the window header buttons (close, detach,
  reattach, maximize, show/hide contents) now follow the interface language
  (en/fr); the "Row N" name of an
  unnamed row series too. Figures exported by a `pyst` script or the CLI keep
  English.
- English text in the tools (#376): the `pyst` docstrings (`help()` in the
  console), the output of `stokesforge-selftest` and `stokesforge-pyhello`,
  the default recipe name of the CLI and of `pyst` ("Untitled", was "Sans
  titre"), the encoding shown for UTF-16 files in the import window
  ("little-endian"/"big-endian"), and the name given to an imported file
  without a stem ("file"). Names and parameters of the `pyst` API do not
  change.
- The robustness probes are renamed `stokesforge-alloc-probe` and
  `stokesforge-stack-probe` (arguments `build`, `undo`, `clear`, `drop`).

## [1.0.1] - 2026-10-27

License fix only (#348), with no code change: StokesForge is under the pure
GPL-3.0-or-later, with no additional permission. The extension exception for
`pyst` of 1.0.0 is removed (`LICENSE-EXCEPTION` deleted). An additional
permission is asked of the author, case by case.

## [1.0.0] - 2026-10-27

Première version publiée. Portage de StokesForge de Python/Qt vers Rust
(l'ancienne implémentation reste au tag `legacy-python`).

### Ajouté

- Application de bureau (Linux et Windows) : classeurs et feuilles, import de
  fichiers d'appareil avec prévisualisation, formules par colonne, graphes
  (rendu, export SVG, PDF et PNG, copie de la figure au presse-papiers),
  historique annulable.
- Analyse : lignes de base, correction, intégrale, ajustements, pics,
  déconvolution, lissage de données (#324).
- Rôles de colonne supplémentaires X3, X4… et Y3, Y4… : calques superposés
  (#168).
- Scripts Python embarqués par le module `pyst`, dont l'API est figée
  (#331) ; `stokesforge-cli import` et `stokesforge-cli run`.
- Première ouverture : un projet de démonstration est offert, enregistré dans
  `demo.stf` du dossier de configuration ; les lancements suivants démarrent
  sur un projet vide.
- Distribution autonome : archive Linux x86_64 et installeur Windows, avec
  l'interpréteur Python embarqué et les notices de licences (#332).

### Installation sous Windows

- L'installeur n'est pas signé : Windows affiche « éditeur inconnu » (ou « Windows
  a protégé votre ordinateur ») au lancement. C'est attendu ; choisir « Informations
  complémentaires » puis « Exécuter quand même ».

### Format de fichier `.stf`

- Version 1 figée (#323) : tout fichier V1 se relira et se migrera jusqu'en V3.
  Un fichier d'une version plus récente est refusé avec un message qui invite
  à mettre à jour StokesForge.
- Les fichiers écrits avant la V1 (développement) ne sont pas garantis.

### Corrigé

- Fichier `.stf` d'un tiers : chargement quadratique, amplification mémoire et
  recalcul non interruptible (#316).
- Séries « ligne » : la suppression de lignes réécrit le numéro de ligne (#307).
- Une courbe tracée depuis une ligne peut être analysée (#253).
- Project Manager : « Hide » ferme la fenêtre comme la croix (#327).
- Tests d'état de l'application : la langue de la personne qui teste ne fuit
  plus dans les tests (#329).
- Fenêtre d'analyse : libellés qui passent à la ligne, pluriel de l'aperçu,
  rond d'aimantation (#334, #335, #336).
