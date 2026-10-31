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

- Peak Decomposition window (#384): the preview no longer overflows the window
  (the last tick read "70" instead of "700") and "Keep in Results" no longer
  wraps; the settings column is wider and the preview takes the width that
  remains.

### Changed

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
  release and published on GitLab Pages for final versions. Chapters are
  still to be written.

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
