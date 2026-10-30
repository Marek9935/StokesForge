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

- Graph size (#394): each grid column has a width and each row a height, in
  points, set in Graph Properties > General > Size (in the unit chosen in
  Preferences, centimetres by default). The figure is their sum plus the title
  band, and the screen, the exports and the copy of the figure all use it. A new
  graph is 480 x 360 pt; a layer added Beside or Below keeps the size of its
  reference layer and the figure grows. New additive `.stf` fields on the graph
  (`column_widths`, `row_heights`; absent = the old 480 x 360 pt shared
  equally): an older version that saves the file again loses them without a
  message.
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
