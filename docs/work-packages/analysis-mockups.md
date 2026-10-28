# Analysis work package (#181) — mockups to choose from before starting

> Report of 2026-10-08, choices made on 2026-10-09. **No code**: screen mockups
> and, for each point still open in #181–#190, two to four options with costs.
> The logic approved on 2026-10-08 (pure Rust, Results panel C', automatic
> recalculation, mouse with magnet, etc.) is **not** discussed again here: the
> mockups give it a shape.

## Decisions (2026-10-09)

The user's choices, to carry into the sub-issues. Where a decision departs
from the mockup or completes it, the decision prevails.

- **M1 B**: submenus by family (`Baseline ▸`, `Fit ▸`,
  `Peaks and Deconvolution ▸`), the method is chosen in the menu. The window
  must still be able to change the method (for `Edit Calculation…`).
- **M2 A**: `Analysis ▸` submenu in the "Create" section of the right-click
  menu.
- **M3 A**: modal with its own clickable preview. This preview will also
  show **visual aids specific to each operation** (background zones, hatched
  areas, components, residuals…).
- **M4 a, A + B**: explicit tools under the preview **and** implicit gestures
  depending on the operation (Ctrl+drag to zoom). If the implicit gestures
  conflict too often, we switch to the explicit tools alone (A).
- **M4 b, C completed**: by default, a point is placed freely; **holding
  Shift** snaps it to the curve, with the local average from A. As long as
  Shift is held over the graph, a tooltip says that **Shift + wheel** sets
  the sensitivity (the number of averaged points). The same setting also
  exists without the wheel, in a regular field of the window
  (`Average over [ 5 ] points`).
- **M5 A completed**: radio button `Show / Keep in Results`, with a **default
  behavior that can be set in the application preferences**, to be handled
  with #148 (Preferences window).
- **M6 A**: long name = source name + short suffix (`Intensity (B-corr)`).
- **M7 a A, b P1, c A**: list + detail; panel between the grid and the tabs;
  `This sheet | Workbook` toggle.
- **M8 C, labels changed**: the confirmation carries
  `[ Delete ]  [ Delete with Columns ]  [ Cancel ]`.
- **M9 a A completed**: the whole column greyed out with an icon in the
  header, **and** the series that plot it greyed out on the graphs, with a
  **distinctive marker** (a triangle carrying a "!") that clearly says the
  data is out of date. **On export** (SVG, PNG, PDF, Ctrl+C) of a figure that
  contains out-of-date series, a confirmation offers to export with or
  without the warning. The graph can also **turn off this warning** (graph
  setting).
- **M9 b C**: `Recalculate automatically` checkbox, default depends on the
  operation.
- **M9 c**: `Recalculate` everywhere the calculation is found (no
  alternative).
- **M9 d A**: input refused, `Detach from Calculation` offered.
- **M10 a B**: handles visible when the calculation is selected.
- **M10 b**: segments or spline, user's choice, segments by default.
- **M10 c**: the same handles for the integration limits and the initial peak
  positions.
- **M11 A**: outside the reference, empty cell, counted.
- **M12 a B, b**: several ranges in one calculation; under the curve, zero or
  a straight line between the limits.
- **M13 a B**: **free model entered by the user from v1** (expression parser
  with functions, numerical derivatives).
- **M13 b**: **global fit from v1** (parameters shared between series,
  `Shared` column).
- **M14 a C, b C**: relative prominence + maximum number; constant, linear or
  no baseline.
- **M15 a A**: IRF interpolated on the signal grid, warning if the steps
  differ by more than 1 %.
- **M15 b B**: **Richardson-Lucy by default**, Wiener as an option.

Cost of the choices on top of the base costed in the sub-issues:
**≈ 13 to 15 days** (detail in the summary), of which ≈ 6.5 days for the free
model and the global fit (M13).

## How to read and answer

- Each point has a number **M1…M15**, the options have a letter; ★ marks the
  recommendation. The cost is given in development days, tests included,
  **on top of** the base already costed in the sub-issues.
- The mockup labels are in **English** (`en` catalog first,
  `docs/ui-texts.md` R1); the new terms and their translation are
  grouped at the end of the report.
- To answer, one line is enough: "all ★ except M7 B and M13 b-yes", or point
  by point. The summary table is at the end of the document.
- Mockup conventions: `( )`/`(•)` radio button, `[ ]`/`[x]` checkbox,
  `[ … ▾]` drop-down list, `[ 123 ]` field, `▌` accent bar of a selected row
  (`list_row`), `▔Sheet1▔` active tab (`tab_strip`, line above for sheets).

## What the mockups revealed

Three findings, found while drawing, that weigh on the choices:

1. **The modal window prevents clicking on the graph.** `modal_dialog_with`
   blocks every click outside the window (the outline flashes red,
   `docs/application/graph.md` § Graph properties). But the common settings
   window is a modal (repository rule) **and** points and ranges are placed
   with the mouse on the graph (decision of 2026-10-08). The two fit together
   only if the window carries **its own clickable preview**, or if it steps
   aside during placement. This is point M3.
2. **A launch from the workbook has no graph** for the preview or for the
   mouse. Same answer: a preview in the window (M3 A) solves both cases at
   once.
3. **The per-sheet Results panel scatters batches.** The current import puts
   one file per sheet ("Active workbook (one sheet per file)"); a fit run on
   20 curves of a graph therefore stores its 20 results on 20 sheets, and
   comparing the 20 τ means opening 20 tabs. Decision C' stands; M7 offers a
   way to see a batch at once.

---

## M1 — Analysis menu in the bar (#182)

Decided place: between Graph and View. Its shape remains to be fixed.
Grammar (`docs/menus.md`): a submenu only for an exclusive choice or at least
3 entries, depth 2, bare verbs, "…" if a window follows.

### A ★ — One entry per operation, the method is chosen in the window

```
 File  Edit  Workbook  Graph  Analysis  View  Window
                             ┌──────────────────────────────┐
                             │ Subtract Baseline…           │
                             │ Correct by Reference…        │
                             ├──────────────────────────────┤
                             │ Integrate…                   │
                             │ Fit…                         │
                             │ Decompose Peaks…             │
                             │ Deconvolve by IRF…           │
                             ├──────────────────────────────┤
                             │ Recalculate Out of Date      │
                             └──────────────────────────────┘
```

The method (linear, polynomial, points; fit model; reconvolution or direct)
is a list in the window: you can change it **while watching the preview**,
without closing. Short menu, no submenu.

### B — Submenus by family, the method is chosen in the menu

```
                             ┌──────────────────────────────┐
                             │ Baseline                   ▸ │──┬───────────────────┐
                             │ Correct by Reference…        │  │ Linear…           │
                             ├──────────────────────────────┤  │ Polynomial…       │
                             │ Integrate…                   │  │ Anchor Points…    │
                             │ Fit                        ▸ │  └───────────────────┘
                             │ Peaks and Deconvolution    ▸ │
                             ├──────────────────────────────┤
                             │ Recalculate Out of Date      │
                             └──────────────────────────────┘
       Fit ▸ : Linear… · Polynomial… · Exponential… · Logarithm… · Power…
               · Gaussian… · Lorentzian…
       Peaks and Deconvolution ▸ : Decompose Peaks… · Reconvolution…
               · Direct Deconvolution…
```

Closer to Origin, one click less for those who know what they want; but
changing the method means cancelling and reopening, and the window must
still be able to change the method (for "Edit Calculation…").

### C — A, with the fit models in a submenu

A everywhere, except `Fit ▸` (seven models), which prefills the model. A
compromise, but two paths to the same window.

| | A ★ | B | C |
|---|---|---|---|
| Change method without closing | yes | no | yes except Fit |
| Menu length | 7 entries | 5 + 13 in submenus | 6 + 7 |
| Cost | ref. | +0.5 d (labels, grammar tests) | +0.3 d |

**"Recalculate Out of Date" entry**: present in all three options, disabled
(tooltip "Everything is up to date") when nothing is waiting; it serves the
manual recalculation of heavy calculations (M9 c). No shortcut proposed.

---

## M2 — Right-click: curve, Object Manager, column header (#182)

The same entries as the bar menu (one shared painting,
`analysis_menu_contents`, wrapped in the bar, flat or as a submenu in the
right-click menu depending on the option).

### A ★ — `Analysis ▸` submenu in the "Create" section

```
 Curve (graph, Object Manager)            Y column header
 ┌──────────────────────────────┐         ┌───────────────────────────────┐
 │ Series: B                    │ (greyed) │ Plot                        ▸ │
 │ Show Data                    │         │ Add Column                    │
 ├──────────────────────────────┤         │ Analysis                    ▸ │
 │ Analysis                   ▸ │         ├───────────────────────────────┤
 ├──────────────────────────────┤         │ Cut · Copy · …                │
 │ Rename                       │         ├───────────────────────────────┤
 │ Move Up                      │         │ Type ▸ · Role ▸               │
 │ Move Down                    │         ├───────────────────────────────┤
 │ Hide                         │         │ Clear · Delete and Shift Up   │
 ├──────────────────────────────┤         └───────────────────────────────┘
 │ Delete                       │
 ├──────────────────────────────┤
 │ Properties…                  │
 └──────────────────────────────┘
```

The curve menu grows by only one line. Depth 2 respected.

### B — Flat entries in the "Create" section

The six operations directly in the curve menu: one click less, but a menu of
14 lines, and the grammar asks for short context menus.

| | A ★ | B |
|---|---|---|
| Curve menu length | 9 | 14 |
| Cost | ref. | same cost |

**On a computed column** (header, or curve that plots it), the same menu
gains at the top of the "Organize" section: `Edit Calculation…` (reopens
the window on this calculation), `Recalculate` (disabled if up to date),
`Detach from Calculation` (see M9 d). Proposed for all options.

---

## M3 — Settings window: where the preview and the mouse live (#182)

See finding 1. This is **the** structuring choice of the base.

### A ★ — Modal with its own clickable preview

```
┌ Subtract Baseline ──────────────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────────┐ │
│ Series   [ Sheet1 › B (+3)        ▾]  │ │ ▲                                │ │
│ X range  (•) All                      │ │ │        ╱╲                      │ │
│          ( ) From [  420  ] to [ 680 ]│ │ │       ╱  ╲         ╱╲            │ │
│          ( ) Current zoom             │ │ │  ____╱    ╲___    ╱  ╲____      │ │
│                                       │ │ │ ●- - - - - - -●- - - - - -●    │ │
│ Baseline                              │ │ └─────────────────────────────▶  │ │
│ Method   [ Anchor points          ▾]  │ │  400        500        600   λ   │ │
│ Points   3 · click to add,            │ ├──────────────────────────────────┤ │
│          right-click to remove        │ │ [Add Points] [Zone] [Zoom] [Fit] │ │
│ Shape    (•) Segments  ( ) Spline     │ │ Showing: B · 1 of 4  [◀] [▶]     │ │
│ Apply    (•) Subtract  ( ) Divide     │ └──────────────────────────────────┘ │
│                                       │                                      │
│ Output                                │                                      │
│ [x] Corrected column                  │                                      │
│ [ ] Baseline column                   │                                      │
│ [x] Add to graph   (Graph1)           │                                      │
│ Values  ( ) Show  (•) Keep in Results │                                      │
│ [x] Recalculate automatically         │                                      │
├───────────────────────────────────────┴──────────────────────────────────────┤
│                                                           [ OK ]  [ Cancel ] │
└──────────────────────────────────────────────────────────────────────────────┘
```

- The window stays a `modal_dialog_with` (repository rule kept).
- The preview is a small interactive graph (same rendering as the graphs,
  `nearest_on_curve` magnet, wheel zoom): it also works from the workbook,
  where there is no graph.
- Launched from a graph, the preview takes over its styles and its zoom; the
  graph behind also shows the result live (like Properties).
- In a batch, `[◀] [▶]` moves from one series to the next; the placed points
  apply to all (per-series option: later).
- Cost: a `GraphView` in the window, ≈ +1.5 d on the base.

### B — Modal that steps aside during placement ("Pick on Graph", Origin style)

```
┌ Subtract Baseline ───────────────────────── ✕ ┐
│ …                                             │
│ Points   3   [ Pick on Graph ]                │   ← the window hides
│ …                                             │
└───────────────────────────────────────────────┘

  Graph1 ─────────────────────────────────────────────────────────────────
  │ Picking anchor points · Enter to finish · Esc to cancel             │
  │                ╱╲                                                   │
  │   ●___________╱  ╲____●______╱╲_____●                               │
```

The real graph, large; but back-and-forth, an exception to the modal
blocking to write and test, and nothing to click from the workbook (a
temporary graph is needed). Cost ≈ +1 d, plus the "workbook" case ≈ +1 d.

### C — Non-modal tool anchored to the graph (Peak Analyzer style)

```
  Graph1 ──────────────────────────────────────────┬──────────────────────
  │                                                │ Subtract Baseline   │
  │         ╱╲             ╱╲                      │ Method [Anchor ▾]   │
  │   ●____╱  ╲_____●_____╱  ╲____●                │ Points 3            │
  │                                                │ …                   │
  │                                                │ [ OK ] [ Cancel ]   │
```

Fully interactive graph, but a **new building block** for windows (outside
`modal_dialog_with`, so against the rule and the guard
`no_window_is_a_raw_egui_window` to extend), a macro left open
while the user can click elsewhere, and still nothing from the workbook.
Cost ≈ +3 d.

| | A ★ | B | C |
|---|---|---|---|
| Rule "every window through the common block" | kept | exception | new block |
| From the workbook | yes | temporary graph | no |
| Size of the clickable area | medium (resizable window) | large | large |
| Cost | +1.5 d | +2 d | +3 d |

**Editing afterwards** (#185): whichever the option, the baseline and the
limits stay editable **on the real graph** through their handles (M10 a); M3
concerns only creation and "Edit Calculation…".

---

## M4 — Mouse gestures in the preview (#182, #185)

### a. Active tool

| Option | Gesture | For | Against | Cost |
|---|---|---|---|---|
| **A ★ — Explicit tools** under the preview (`[Add Points] [Zone] [Zoom]`), the default one depends on the operation | click = point; drag = zone; drag a point = move it; right-click or Del = remove it | you see what the click will do; zoom without conflict | one more click to change tool | ref. |
| B — Implicit tool depending on the operation | same gestures, Ctrl+drag = zoom | no bar | zoom hidden behind a key | −0.2 d |

### b. Where a point snaps

| Option | Effect | Cost |
|---|---|---|
| **A ★ — Magnet on the curve, local average as an option** (`Average over [ 5 ] points`, tooltip "Less sensitive to noise") | the point lands on a measured point; the average smooths the noise | +0.3 d |
| B — Magnet alone | simpler, sensitive to noise | ref. |
| C — Free point, Shift to snap | allows a point off the curve (extrapolated background) | +0.2 d |

---

## M5 — "Output" block: show or keep the values (#183)

Decided: always the choice between **show** (nothing in the project) and
**keep** (Results panel).

### A ★ — Radio button in the window, remembered per operation

`Values  ( ) Show  (•) Keep in Results` (mockup M3 A). Predictable, suited to
batches. The "Show" window still offers `Keep in Results` to change one's
mind.

### B — Two confirm buttons

`[ Show Values ]  [ OK ]  [ Cancel ]`: the choice is made by confirming.
Shorter, but two action buttons side by side get confused (R14: one button
= one verb).

### C — Always show, then keep if needed

OK calculates and always opens the values window, with `Keep in Results`.
You decide while seeing; one more window at every calculation, tiresome in a
batch.

### The values window (common to all three)

```
┌ Fit Results ─────────────────────────────────────────────────── ✕ ┐
│ Exponential decay, 1 term + offset · Sheet1 › C                   │
│ 512 points used, 3 ignored (empty or text)                        │
│ ┌───────────┬──────────────┬──────────────┬──────────┐            │
│ │ Parameter │ Value        │ Std. error   │ Unit     │            │
│ ├───────────┼──────────────┼──────────────┼──────────┤            │
│ │ y0        │ 12.4         │ 0.8          │ counts   │            │
│ │ A1        │ 9 831        │ 41           │ counts   │            │
│ │ τ1        │ 4.213        │ 0.031        │ ns       │            │
│ └───────────┴──────────────┴──────────────┴──────────┘            │
│ R² 0.9981 · Reduced χ² 1.07                                       │
├───────────────────────────────────────────────────────────────────┤
│                       [ Copy ]  [ Keep in Results ]  [ Close ]    │
└───────────────────────────────────────────────────────────────────┘
```

`Copy` puts the table on the clipboard as tab-separated text (pastes into a
sheet, a spreadsheet, a notebook). In a batch: one row per series, one
column per parameter (see M7 c).

| | A ★ | B | C |
|---|---|---|---|
| Batches | good | good | one window to close |
| Cost | ref. | −0.2 d | same cost |

---

## M6 — Produced columns: place and name (#182)

Decided: right after the source Y column, a long name that says the
operation, detail in the comment, persisted trace.

```
┌ Book1 ──────────────────────────────────────────────────────────────────┐
│       │ A(X)    │ B(Y)      │ C(Y)               │ D(Y)               │ │
│ Long  │ λ       │ Intensity │ Intensity (B-corr) │ Baseline of B      │ │
│ Units │ nm      │ counts    │ counts             │ counts             │ │
│ Comm. │         │           │ Anchor points, 3,  │ Anchor points, 3,  │ │
│       │         │           │ subtracted         │ segments           │ │
│ Meta  │         │           │ ⤷ Baseline 1       │ ⤷ Baseline 1       │ │
│ 1     │ 400.0   │ 152.3     │ 1.8                │ 150.5              │ │
│ 2     │ 400.5   │ 151.9     │ 1.5                │ 150.4              │ │
```

`⤷ Baseline 1` in the Metadata row is a link: a click unfolds the Results
panel and selects the calculation there (#183).

### Shape of the long name

| Option | Example | For | Against |
|---|---|---|---|
| **A ★ — Source name + short suffix** | `Intensity (B-corr)`, `Intensity (fit)`, `Intensity (resid.)`, `Peak 1 of B` | reads like the axis title it feeds (Long Name / Units); the source stays first | abbreviations to fix in the glossary |
| B — Operation + source letter | `Baseline corrected B`, `Fit of B`, `Residuals of B` | no abbreviation | the quantity (Intensity) disappears from the axis title and the legend |
| C — Free name typed in "Output" | field prefilled by A | total control | one more field per output |

Same cost (C: +0.3 d).

---

## M7 — Results panel under the grid (#183)

Decided: C', a collapsible panel under the grid, that follows the active
sheet; collapsed by default, counter, absent without results.

### a. Panel content

**A ★ — List + detail**

```
┌ Book1 ──────────────────────────────────────────────────────────────────┐
│ (grille…)                                                             ▲ │
│                                                                       │ │
├═════════════════════════════════════════════════════════════════════ ═══┤ ← poignée
│ ▾ Results (3)                                  [This sheet | Workbook]  │
│ ┌──────────────────────────────┬──────────────────────────────────────┐ │
│ │▌Baseline 1      B            │ Fit 1 · Exponential decay, 1 term    │ │
│ │ Integrate 1     C   1.28e4   │ Input   Sheet1 › C, X 0 – 50 ns      │ │
│ │ Fit 1           C   τ 4.21   │ Output  E, F                         │ │
│ │                  (!) out of  │ y0   12.4   ± 0.8     counts         │ │
│ │                      date    │ A1   9 831  ± 41      counts         │ │
│ │                              │ τ1   4.213  ± 0.031   ns             │ │
│ │                              │ R² 0.9981 · Reduced χ² 1.07          │ │
│ │                              │ [Edit…] [Recalculate] [Copy] [Delete]│ │
│ └──────────────────────────────┴──────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────┤
│ ▔Sheet1▔   Sheet2   Sheet3                                              │
│ ◄═══════════════════════════════════════════════════════►              │
└─────────────────────────────────────────────────────────────────────────┘

 Replié :  │ ▸ Results (3)                                                 │
```

List by `list_row`, detail on the right, title by `theme::section_title`.
Height adjustable by the handle, remembered per workbook (egui memory, like
the Metadata row, not in the `.stf`).

**B — Stacked cards**: one card per calculation, all unfolded one under the
other (no detail on the right). Readable for 2 or 3 calculations, long to
scroll beyond that.

**C — Tree** (`tree_lines`): `Fit 1 ▸` unfolds its parameters into rows.
Compact, but comparing two calculations side by side is impossible.

### b. Panel position

| Option | Order from top to bottom | For | Against |
|---|---|---|---|
| **P1 ★** | grid · **Results** · tabs · horizontal bar | the tabs stay at the edge, like a spreadsheet; they drive grid and panel | the grid's horizontal bar moves away from the grid (it already is, under the tabs) |
| P2 | grid · tabs · bar · **Results** | the grid keeps its bars attached | the panel seems to depend on no sheet |

### c. Seeing a batch at once (finding 3)

| Option | Effect | Cost |
|---|---|---|
| **A ★ — `This sheet | Workbook` toggle** in the panel header | "Workbook" lists the calculations of all sheets of the workbook, with an extra "Sheet" column; a batch launched together is laid out there as a series × parameter table, copyable | +1 d |
| B — Nothing more | a batch is compared through the values window (M5) at calculation time, or through `pyst` | 0 |
| C — `Copy Batch Results` command on a calculation of a batch | a series × parameter table on the clipboard, without changing the panel | +0.5 d |

"Workbook" creates neither a sheet nor an object: it is a filter of the same
panel.

---

## M8 — Deleting a result or a produced column (#183)

| Option | Delete a **result** | Delete a **produced column** | Cost |
|---|---|---|---|
| A — Linked | also deletes its columns (confirmation that names them) | deletes the result if it was its only output, otherwise the result no longer writes it | ref. |
| B — Independent | the columns stay and become ordinary (trace removed, no more recalculation) | the result stays, the output is marked "deleted" there | ref. |
| **C ★ — Chosen at delete time** | confirmation with two verbs: `[ Delete Result ]  [ Delete with Columns ]  [ Cancel ]` | as B | +0.3 d |

```
┌ Delete Result ───────────────────────────────────────────── ✕ ┐
│ Fit 1 wrote columns E (Intensity (fit)) and F (resid.).       │
│ Keep them as ordinary columns, or delete them too?            │
├───────────────────────────────────────────────────────────────┤
│        [ Delete Result ]  [ Delete with Columns ]  [ Cancel ] │
└───────────────────────────────────────────────────────────────┘
```

A single history entry in all cases.

---

## M9 — Recalculation: disabled, heavy calculations, manual editing (#184, #185)

### a. An out-of-date column (failure or waiting for manual recalculation)

```
│       │ B(Y)      │ C(Y)  (!)          │ ← icon tooltip:
│ Long  │ Intensity │ Intensity (B-corr) │   « Out of date: the fit no longer
│ 1     │ 152.3     │ 1.8                │     converges (B changed). Showing
│ 2     │ 151.9     │ 1.5                │     the last valid values »
          normal      texte grisé
```

| Option | Effect | Cost |
|---|---|---|
| **A ★ — Whole column greyed out**, warning icon in the header, reason in a tooltip | a calculation is a whole: all its values come from the same pass | ref. |
| B — Only the changed cells | looks precise, but a baseline or a fit changes all the values: nothing to distinguish | +0.5 d |

Same greying and same icon on the row of the Results panel (mockup M7).

### b. Who decides that a calculation is "heavy" (manual recalculation)

| Option | Effect | Cost |
|---|---|---|
| A — The operation | reconvolution and deconvolution always manual | ref. |
| B — A threshold | beyond N series × points, manual | +0.3 d, threshold to tune |
| **C ★ — The user, with a default per operation** | `Recalculate automatically` checkbox in "Output" (M3 A), checked by default except for reconvolution, direct deconvolution and batches of more than 20 series; editable afterwards through `Edit Calculation…` | +0.2 d |

### c. Where to restart a manual recalculation

**★ Everywhere the calculation is found** (the grammar wants nothing to be
only in the right-click menu): `Recalculate` button of the detail (M7),
`Recalculate` entry in the right-click menu of a produced column (M2), and
`Analysis › Recalculate Out of Date` for the whole project (M1). No
alternative proposed.

### d. Manual input in a produced column

| Option | Effect | Cost |
|---|---|---|
| **A ★ — Refused**, message that offers to detach | "This column is computed by Baseline 1. Detach it from the calculation to edit it" + `Detach from Calculation` (the column becomes ordinary, the result keeps its values) | ref. |
| B — Accepted, the link breaks after confirmation | more flexible, a confirmation every time | +0.2 d |
| C — Accepted, overwritten at the next recalculation | like Origin; the input can vanish without warning | ref. |

---

## M10 — Baseline (#185)

Window: mockup M3 A. Depending on `Method` the block changes:

```
Method   [ Linear               ▾]       Method   [ Polynomial           ▾]
From     (•) Two points ( ) Zones        Degree   [  3  ]
Points   2 · click to add                Zones    4 · drag to add
Apply    (•) Subtract ( ) Divide         Apply    (•) Subtract ( ) Divide
```

"Zones": background ranges placed by dragging, on which the line or the
polynomial is fitted; they are shown as pale bands in the preview.

### a. Handles on the real graph after creation

```
  Graph1
  │            ╱╲                ╱╲
  │   ◆_______╱  ╲______◆______╱  ╲______◆      ◆ baseline handles
  │   - - - - - - - - - - - - - - - - - - -     (dashed, color of B)
```

| Option | When are the baseline and its handles seen | Cost |
|---|---|---|
| A — Always | clutters a figure meant for export; it would have to be excluded from export | +0.5 d |
| **B ★ — When the calculation is selected**: click on the corrected curve, row of the Results panel, Object Manager node | nothing on the figure at rest or on export; dragging a handle = one history step | ref. |
| C — "Show baseline" checkbox per calculation | fine choice, one more toggle to manage and persist | +0.5 d |

### b. Line by points: segments or smooth curve

**★ User's choice** (`Shape (•) Segments ( ) Spline`, segments by default).
Alternative: segments only (−0.3 d).

### c. Same principle for the other parameters placed on the graph

**★ Yes**: integration limits (M12) and initial peak positions (M14) have
the same handles, visible when the calculation is selected. Alternative:
handles for the baseline only, the rest through `Edit Calculation…` (−1 d).

---

## M11 — Correction by a reference column (#186)

```
┌ Correct by Reference ───────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────┐ │
│ Series     [ Sheet1 › B         ▾]    │ │  data ───  reference ┄┄┄   │ │
│                                       │ │      ╱╲   ┄┄┄┄┄┄┄┄           │ │
│ Reference                             │ │ ┄┄┄┄╱  ╲┄┄        ┄┄┄┄       │ │
│ Column     [ Detector › B (QE)  ▾]    │ │ ___╱    ╲____________        │ │
│ X          Detector › A (λ / nm)      │ │ ░░░                     ░░░░ │ │
│ Operation  (•) Divide ( ) Multiply    │ │ ↑ outside the reference     │ │
│            ( ) Subtract               │ └──────────────────────────────┘ │
│ Outside    32 points outside the      │                                  │
│ reference  reference range            │                                  │
│ Output  …                             │                                  │
```

Order kept in the application (#252): Input, Analysis type, then the
Reference column at the head of the family block (Response for
deconvolution), then the operation and Output — as for the Baseline.

The reference is brought onto the X grid of the data by linear interpolation
(decided). Points outside the reference domain:

| Option | Effect | Cost |
|---|---|---|
| **A ★ — Empty cell (NaN), counted** | honest: nothing is invented; the count is shown (mockup) and in the result | ref. |
| B — Refusal if the reference does not cover everything | safe, but blocks the common case of a slightly shorter reference | ref. |
| C — Last value extended | continuous curve, but an invented correction at the edges | ref. |

The correction follows the changes of the reference (automatic
recalculation, dependency between sheets): a direct consequence of the
decisions, only to be confirmed.

---

## M12 — Integration (#187)

```
┌ Integrate ──────────────────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────┐ │
│ Series     [ Sheet1 › C         ▾]    │ │        ╱╲           ╱╲       │ │
│                                       │ │       ╱▓▓╲         ╱▒▒╲      │ │
│ Ranges     drag on the preview        │ │  ____╱▓▓▓▓╲_______╱▒▒▒▒╲___  │ │
│ ┌────┬──────────┬──────────┬────────┐ │ │      |  1  |     |  2   |    │ │
│ │ #  │ From     │ To       │ Area   │ │ └──────────────────────────────┘ │
│ │ 1  │ 455.0    │ 505.0    │ 8 412  │ │                                  │
│ │ 2  │ 590.0    │ 650.0    │ 4 377  │ │                                  │
│ └────┴──────────┴──────────┴────────┘ │                                  │
│ Unit       nm·counts (X × Y, as is)   │                                  │
│ Under      (•) Zero  ( ) Straight line│                                  │
│            between range ends         │                                  │
│ Output                                │                                  │
│ [ ] Cumulative integral column        │                                  │
│ [ ] Normalized column (area = 1)      │                                  │
│ Values  ( ) Show  (•) Keep in Results │                                  │
```

The unit of the area is the product of the column units, without conversion
(decided), written only if both units are filled in.

### a. Several ranges in one calculation

| Option | Effect | Cost |
|---|---|---|
| A — One range per calculation | simple; three bands = three calculations | ref. |
| **B ★ — Several ranges, one table** | the areas of an emission spectrum together, a single result row, area ratios made easy | +0.5 d |

### b. Under the curve

**★ `Zero` or `Straight line between range ends`**, plus "an existing
baseline" if the series is already a corrected column (it then is, in
effect). Alternative: `Zero` alone (−0.3 d), the baseline always going
through M10.

---

## M13 — Fit (#188)

```
┌ Fit ────────────────────────────────────────────────────────────────── ✕ ┐
│ Input                                    │ ┌───────────────────────────┐ │
│ Series   [ Sheet1 › C (+19)       ▾]     │ │ ●                         │ │
│ X range  (•) All ( ) From … ( ) Zoom     │ │  ●●                       │ │
│ Weight   [ None / Sheet1 › D (yErr) ▾]   │ │    ●●●___   données ●     │ │
│                                          │ │          ‾‾‾───────  fit  │ │
│ Model                                    │ ├───────────────────────────┤ │
│ ┌─────────────────────┐                  │ │ résidus  ·  ·· · ·  · ·   │ │
│ │ Linear              │ y = y0 + A e^-x/τ│ └───────────────────────────┘ │
│ │ Polynomial          │                  │                               │
│ │▌Exponential decay   │ Terms  [ 1 ▾]    │                               │
│ │ Exponential growth  │ [x] Offset y0    │                               │
│ │ Logarithm           │                  │                               │
│ │ Power               │                  │                               │
│ │ Gaussian            │                  │                               │
│ │ Lorentzian          │                  │                               │
│ └─────────────────────┘                  │                               │
│ Parameters                       [ Auto ]│                               │
│ ┌──────┬──────────┬───────┬──────┬──────┬────────────┐                   │
│ │ Name │ Initial  │ Fixed │ Min  │ Max  │ Fitted     │                   │
│ │ y0   │ 12       │ [ ]   │      │      │ 12.4 ± 0.8 │                   │
│ │ A1   │ 9 800    │ [ ]   │ 0    │      │ 9 831 ± 41 │                   │
│ │ τ1   │ 4        │ [ ]   │ 0    │      │ 4.21 ± 0.03│                   │
│ └──────┴──────────┴───────┴──────┴──────┴────────────┘                   │
│ R² 0.9981 · Reduced χ² 1.07 · converged in 7 iterations                  │
│ Output  [x] Fitted column [x] Residuals column [x] Add to graph …        │
```

The model formula is composed by Typst (like the titles). The calculation
restarts at every change (preview), `Auto` re-estimates the initial values.

### a. Free model entered by the user in v1

| Option | Effect | Cost |
|---|---|---|
| **A ★ — Not in v1** | the listed models; the free case goes through `pyst` + scipy (#191) | ref. |
| B — Yes | "y = …" field; needs an expression parser with functions (`exp`, `ln`, `sqrt`…), absent today (`formula.rs` only does `+ - * /`), and numerical derivatives | +3 to 4 d |

### b. Global fit (several series, shared parameters)

**★ Later**, in a separate sub-issue. Alternative: in v1, a `Shared` column
in the parameter table (+3 d).

---

## M14 — Peak decomposition (#189)

```
┌ Decompose Peaks ────────────────────────────────────────────────────── ✕ ┐
│ Input   [ Sheet1 › C          ▾]         │ ┌───────────────────────────┐ │
│ Shape   [ Gaussian            ▾]         │ │      ╱╲      ╱‾╲          │ │
│ Peaks   [ Find ]  Prominence [ 5 % ]     │ │     ╱  ╲    ╱   ╲         │ │
│         Max. number [ 6 ]                │ │   ▲╱ 1  ╲ ▲╱  2  ╲▲       │ │
│         click to add, right-click        │ │  _╱      ╲╱       ╲╲_     │ │
│         to remove                        │ │  composantes ┄┄ somme ──  │ │
│ Baseline (•) Constant ( ) Linear         │ └───────────────────────────┘ │
│          ( ) None (already corrected)    │                               │
│ [ ] Same width for all peaks             │                               │
│ ┌───┬──────────┬──────────┬──────────┬──────────┐                        │
│ │ # │ Position │ Height   │ FWHM     │ Area     │                        │
│ │ 1 │ 478.2 ±… │ 1 204 ±… │ 21.3 ±…  │ 27 310   │                        │
│ │ 2 │ 612.0 ±… │   655 ±… │ 38.9 ±…  │ 27 120   │                        │
│ │ 3 │ 640.5 ±… │   210 ±… │ 15.0 ±…  │  3 350   │                        │
│ └───┴──────────┴──────────┴──────────┴──────────┘                        │
│ Output  [x] One column per peak  [x] Sum  [ ] Residuals  [x] Add to graph│
```

The components added to the graph form a series group (#177), recolored by a
palette.

### a. Automatic detection

| Option | Effect | Cost |
|---|---|---|
| A — Imposed number of peaks | predictable, but you need to know the answer | ref. |
| B — Relative height threshold | simple, sensitive to the baseline | ref. |
| **C ★ — Relative prominence + maximum number** | robust to a rising background; the maximum avoids 40 peaks on a noisy spectrum | +0.3 d |

### b. Baseline

| Option | Effect | Cost |
|---|---|---|
| A — Always subtracted beforehand (through M10) | a single path, but two calculations to chain | ref. |
| B — Always inside the fit | one gesture, but no free shape | ref. |
| **C ★ — `Constant`, `Linear` inside the fit, or `None` for an already corrected series** | covers both uses | +0.3 d |

---

## M15 — Deconvolution by a measured response (#190)

```
┌ Deconvolve by IRF ──────────────────────────────────────────────────── ✕ ┐
│ Input     [ Decays › B          ▾]       │ ┌───────────────────────────┐ │
│ IRF       [ IRF › B             ▾]       │ │ IRF ┄┄   déclin ●  fit ── │ │
│ Method    (•) Reconvolution              │ │  ┆╲●                      │ │
│           ( ) Direct deconvolution       │ │  ┆ ●●╲                    │ │
│                                          │ │ ┄┘   ●●●╲___              │ │
│ Reconvolution                            │ │            ‾‾‾●●●───────  │ │
│ Model    [ Exponential decay ▾] Terms [2]│ ├───────────────────────────┤ │
│ Shift     [ 0.00 ] ns  [x] Fit the shift │ │ résidus · ·  ·· · · ·     │ │
│ Background [x] Fit constant background   │ └───────────────────────────┘ │
│ Parameters (tableau comme M13)           │                               │
│                                          │                               │
│ Direct deconvolution (si choisie)        │                               │
│ Algorithm [ Wiener              ▾]       │                               │
│ Noise level [ Auto ]  ──●────────        │                               │
```

Decided: both methods; adjustable shift with a checkbox (shown).

### a. IRF and signal on different X grids

| Option | Effect | Cost |
|---|---|---|
| **A ★ — IRF interpolated on the signal grid**, warning if the steps differ by more than 1 % | works with files from different instruments | ref. |
| B — Refusal if the grids differ | no hidden interpolation; forces resampling beforehand | ref. |

### b. Direct deconvolution: default method and exposed setting

| Option | Setting shown | For | Against |
|---|---|---|---|
| **A ★ — Wiener by default, Richardson-Lucy as an option** | a noise level (`Auto` estimated, slider); for Richardson-Lucy, a number of iterations | immediate, a single setting | Wiener can give negative values (flagged) |
| B — Richardson-Lucy by default | number of iterations | stays positive, suited to counts | slower, the stopping is empirical |
| C — Wiener alone in v1 | noise level | −1 d | no positive alternative |

---

## New terms (to validate with `docs/ui-texts.md`)

The already reserved terms (†) are used as they are: *Baseline*, *Peak*,
*Peak position*, *Height*, *FWHM*, *Area*, *Normalize*, *Integrate*, *Fit*.
To add to the glossary at launch, proposed translation:

| English | French | Note |
|---|---|---|
| Analysis | Analyse | menu |
| Results | Résultats | panel under the grid |
| Calculation | Calcul | an entry of the panel; "Edit Calculation…", "Detach from Calculation" |
| Out of date | Pas à jour | greyed-out state (M9) |
| Recalculate | Recalculer | |
| Anchor point | Point d'ancrage | baseline by points |
| Zone | Zone | background range of a baseline (≠ *Range*, axis or integration range) |
| Reference | Référence | correction column |
| Instrument response (IRF) | Réponse instrumentale (IRF) | |
| Reconvolution / Deconvolve | Reconvolution / Déconvoluer | |
| Residuals | Résidus | |
| Fitted | Ajusté | fitted column and value; *Series* remains the word for the plotted data (not *curve*) |
| Std. error | Erreur type | "±" column |
| Prominence | Proéminence | peak detection |
| Shift | Décalage | IRF/signal; to distinguish from *Offset (stack)* |

---

## Summary of choices

| # | Subject | ★ recommended | **Chosen (2026-10-09)** | Cost of the choice on top of the base |
|---|---|---|---|---|
| M1 | Analysis menu | A one entry per operation | **B** submenus by family | +0.5 d |
| M2 | Right-click | A `Analysis ▸` submenu | **A** | 0 |
| M3 | Window and mouse | A modal with clickable preview | **A**, preview also a support for visual aids per operation | +1.5 d |
| M4 a | Mouse tool | A explicit tools | **A + B**, fall back to A if conflicts | +0.2 d |
| M4 b | Point snapping | A magnet + optional average | **C** free, Shift = magnet + average; Shift + wheel sets the sensitivity (tooltip); regular field too | +0.6 d |
| M5 | Show / keep | A remembered radio | **A** + default in Preferences (#148) | +0.3 d |
| M6 | Produced long name | A source + suffix | **A** | 0 |
| M7 a | Panel content | A list + detail | **A** | 0 |
| M7 b | Panel position | P1 above the tabs | **P1** | 0 |
| M7 c | Seeing a batch | A `This sheet / Workbook` toggle | **A** | +1 d |
| M8 | Delete | C chosen at delete time | **C**, buttons `Delete` / `Delete with Columns` / `Cancel` | +0.3 d |
| M9 a | Greying | A whole column | **A** + greyed series + triangle "!" marker; confirmation on export; warning can be turned off per graph | +1.5 d |
| M9 b | Heavy calculation | C checkbox, default per operation | **C** | +0.2 d |
| M9 c | Manual restart | everywhere | **everywhere** | 0 |
| M9 d | Manual input | A refused, "Detach" offered | **A** | 0 |
| M10 a | Handles after creation | B when selected | **B** | 0 |
| M10 b | Line by points | segments or spline, user's choice | **user's choice**, segments by default | 0 |
| M10 c | Handles for limits and peaks | yes | **yes** | 0 |
| M11 | Outside the reference | A empty, counted | **A** | 0 |
| M12 a | Integration ranges | B several | **B** | +0.5 d |
| M12 b | Under the curve | zero or straight line between limits | **zero or straight line** | 0 |
| M13 a | Free model v1 | A no (scipy) | **B yes** | +3 to 4 d |
| M13 b | Global fit | later | **v1** | +3 d |
| M14 a | Peak detection | C prominence + max | **C** | +0.3 d |
| M14 b | Baseline of peaks | C constant, linear or none | **C** | +0.3 d |
| M15 a | IRF/signal grids | A interpolation + warning | **A** | 0 |
| M15 b | Direct default | A Wiener, R-L as an option | **B** Richardson-Lucy, Wiener as an option | 0 |

Total of the choices on top of the base costed in the sub-issues:
**≈ 13 to 15 d** (≈ 4.4 d for the ★ alone).

The choices are carried into the relevant sub-issues (section "Decisions");
this report keeps the reference mockups and can be archived at the end of
the work package.
