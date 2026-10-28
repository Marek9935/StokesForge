# Application: analysis

Continuation of the table of contents [`README.md`](README.md). Contents: analysis window, Analysis menu, Results panel.

### Analysis: window, menu, Results panel (#181)

The mathematical processing interface is split as follows:

- `stokesforge-app-state/src/analysis.rs` — **the form model** `AnalysisForm` (no egui):
  family, settings of each family (all kept when the family
  changes), series, auxiliary (reference, response), `issues()` (what blocks
  Apply: `Operation::readiness()` of `stokesforge-analysis` for the points, the
  zone and the model, translated by `issue_of`; the `Tree` for the rest, #260),
  `operation_for(for_preview)`, `preview(tree)`, `draft(tree)` (long
  names of the output columns, M6: the rule is `stokesforge_core::OutputName`,
  translated here by `output_name_text_in`, #256), `from_calculation` (editing), the preview
  gestures (`add_point`, `add_zone`, `remove_near`). Tested without a window
  (`stokesforge-app-state/tests/analysis.rs`).
- `stokesforge-app-state/src/state/analysis.rs` — the state and the `Action`s (`OpenAnalysis`,
  `EditCalculation`, `ApplyAnalysis`, `RecalculateCalculations`,
  `DetachCalculation`, `RequestDeleteCalculation`/`ConfirmDeleteCalculation`);
  Apply goes through `AddCalculation`/`UpdateCalculation` and keeps the window
  open on error.
- The surfaces of `src/app/`: `analysis_menu`, `analysis_dialog`,
  `analysis_forms`, `analysis_preview`, `results_pane` (module table above).
- Handles on the graph (#199): `AnalysisForm::handles` / `move_handle` (model,
  no egui) list and move anchor points, bounds of zones/ranges and
  peak positions; `app/analysis_handles.rs` paints them over the figure
  (never in `stokesforge-render`, so never at export) for the calculation
  of which a curve is selected or whose row in the Results panel is
  chosen. During the drag, the baseline is recalculated on the fly;
  on release, `Action::MoveCalculationHandle` reopens the calculation like the window
  and pushes an `UpdateCalculation`: **one history step per gesture**, none
  if the handle did not move. The Object Manager node does not exist (a
  calculation is not a node).
- The preview is painted by hand (no project graph) but **its geometry does not
  belong to it** (#261): pixel ↔ data, zoom, pan, initial
  frame (5% margin, remembered bounds, inversion), snapping
  to the curve (the orange circle covers the averaged zone: radius = largest
  half-side of the X zone converted to pixels by the screen transform,
  6 px at minimum, the plot diagonal at most; the text "Average over N
  points" gives the real number of points, #334) and ticks come from `stokesforge-render`
  (`PanelTransform::new`, `zoom`, `pan`, `axis_frame`, `snap_to_curve_zone`,
  `scale::axis_ticks`);
  the application builds one transform per frame (a second one for the
  residuals band) and converts to `Pos2`. The ticks therefore have the step and
  the format of those of the graph (decimal point, no comma), and follow
  the logarithmic scale and the axis inversion of the source graph. A zoom or
  a pan with no image (logarithmic bound ≤ 0) keeps the frame. The
  **navigation is that of the graph** (#270, D3 of #299): Ctrl + wheel zooms
  (above an axis, that scale only), Ctrl + drag pans the view,
  double-click reframes, the wheel alone does nothing; there is no Zoom tool anymore.
  The **gesture** (point or zone) is that of the analysis (`natural_tool`); only
  peak detection offers the choice (`tool_choices`), in two boxed texts with a
  toggle (`form::toggle_texts`). Matrix: `docs/gestures.md`. It is
  **computed on a worker
  thread** (#201, `analysis_preview.rs`, `PreviewJobs`): only one thread at a
  time, the most recent request replaces the one that waits, the frame waits
  40 ms at most then shows the old result marked "Calculating…"; the thread
  wakes the viewport once at its end, never a periodic wake-up. A
  calculation already started is not interrupted: its stale result is ignored.
- Stale columns: greyed out in the grid, tinted letter (`theme::stale_color`)
  and tooltip; the Results panel (collapsed by default) lists the kept
  calculations (Sheet / Workbook scope) with Edit / Recalculate / Delete. A
  22 px centered banner, background one step lighter (`theme::results_pane_fill`); the list
  (≤ 45% of the width) and the detail are two bounded rectangles, the
  detail text wraps; a list row is "Operation · Sheet/Column"
  (`row_title`), with no category.
- A category of the `Analysis ▸` submenu of the right-click reopens **the last
  method run in that category** (taken from the recent analyses, so from the last
  three only), otherwise `Category::default_preset`. `publish_last_presets`
  publishes it every frame in the `Context`, `category_preset` reads it: the graph
  menus do not have the state at hand. Only the method is taken back, not its
  settings.
- Series not up to date on the graph (#198): the core resolves the flag
  (`ResolvedSeries.stale`: one of the X, Y or error columns comes from a calculation
  not up to date) and the rendering applies it knowing nothing of the model — greyed line
  (`#9a9a9a`) and warning triangle just above its last point; the
  series row of the Object Manager carries the `Icon::Stale` icon. The setting lives in the
  graph (`PlotSpec::stale_warning`, "Warn about out-of-date data" checkbox of the
  Graph page of the properties, default on, old file = on). Exporting or
  copying the figure of a graph that shows such a calculation asks first
  (`app/stale_export.rs`, `AppState::pending_stale_export`): Export Anyway,
  Export Without Warning (same figure, without grey or marker), Recalculate and
  Export, Cancel. Neither command nor history for the confirmation itself; the
  recalculation goes through the usual commands.
- Errors come from `errors::analysis::error_message` (exhaustive match).
- Fit window (#204): three distinct exponentials (`Exponential`,
  `Exponential with Plateau`, `Two Exponentials`, same names as `pyst`:
  `exponential`, `exponential_offset`, `exponential2`); the model formula
  is composed by Typst (`model_formula_source` →
  `stokesforge_render_egui::formula_label`, cache and fonts kept in
  egui's memory); `Auto` fills the starting values with
  `CompiledModel::guess` (for the free model, a sweep per parameter that
  follows the scale of the data); an area is displayed with the product of the units of
  X and Y when both exist (`value_unit`); the peaks window bounds
  the centers (`center_window`, also `pyst.analysis.peaks`).
- Finishing touches (#200): "Add to graph" (checkbox of the output, visible when a
  graph shows the curve; `AppState::graphs_plotting`) creates the calculation **and**
  plots its columns (except residuals, `PanelSpec::push_series_batch` via
  `AddSeriesToGraph`) in a single macro `MacroKind::AnalysisToGraph`;
  Show / Keep in Results remembers the last choice from one session to the next
  (`preferences.json`, `analysis_keep_results`); the header of a column not up to
  date carries the `Icon::Stale` icon (Phosphor `warning`, neutral role) to the left of
  its tinted letter.
- Analysis from a curve (#210): the right-click of the figure and of the
  Object Manager share the "Analysis…" entry. The top Analysis menu
  also uses the selected curve or group when the graph is active.
  The form keeps the X/Y references of each series, its style and its
  framing; it does not pair the columns again according to their position in the
  sheet. A group opens a batch, the data of a hidden workbook stay
  analyzable. "Add to graph" is checked at the start and the produced columns
  join the layer and the group of their source curve.
  The preview also keeps the logarithmic scale and the inversion of the axes,
  with gestures in data coordinates. For bars or a
  histogram, it links the raw Y to analyze with the source style;
  it does not compute a histogram of the results.
- Constant baseline (#211): "Constant…" is the first method of the
  Baseline menu. The flat background is set by a single point (free, or snapped with
  Shift and local average) editable in X/Y, or by the average of an inclusive zone
  of X. A new zone replaces the previous one; only finite X/Y pairs
  enter its average. Subtract and Divide reuse the same operations
  as the other baselines.
- Smoothing (#324): three methods in the Data Manipulation category —
  adjacent average, Savitzky-Golay (order 2 to 5) and moving median —,
  `stokesforge-analysis/src/smooth.rs` (the `//!` header says why: neighbors
  **by rank** after sorting by X, reflected edges, odd window). The window is
  set in points (odd: the field steps by 2, `SmoothSetup::set_points`
  raises an even input) or in X units (converted by the **median** step); 
  caps `MAX_WINDOW`, `MIN_ORDER`/`MAX_ORDER` read by `quantities` and by
  `Operation::validate`. The output is the `smoothed` column (`OutputName::Smoothed`),
  of the length of the source, and can **replace** the Y like a correction
  (`replacement_key` in `core::calc`). An X step that is irregular by more than 1%
  warns (`SignalStepNotUniform`) only for Savitzky-Golay, whose
  coefficients assume a regular step. No FFT filter (rejected: an FFT to
  write without a dependency, and the handling of edges and irregular steps).
- Replacing the original data (#212): the output of a baseline,
  of a correction by reference or of a smoothing can overwrite its ordinary Y columns.
  The column keeps its length and its unusable cells; the
  curves that plot it follow the new values. Columns with a formula
  or computed columns stay protected. Several curves that share a source
  trigger a confirmation. The `ReplaceAnalysisData` command writes the
  data and an immutable `CorrectionRecord` trace in one undoable
  entry, with no calculation link nor produced column; the settings do not
  reopen. Results keeps the successive corrections, including
  after a manual input, and the `.stf` file saves them. The small
  neutral pencil in the header points to these settings through its tooltip;
  Undo removes the corrected values and the trace of the gesture together.
- The choice of the analysis type is made in the window ("Analysis type" list,
  `family_chooser`) as long as it creates a calculation; a reopened calculation keeps its
  family (#202).
- Rows skipped and counted (`skipped`): non-finite X or Y, and, in weighted
  fit, empty, zero or negative σ (#205) — never a failure of the whole fit.
  A parameter that the data do not determine (zero Jacobian column or
  combination of the previous ones) has no uncertainty and raises
  `Warning::Undetermined(n)`; if no free parameter is determined,
  the fit is refused (`SingularSystem`, #203).
- Calculation warnings and errors (#313): `Warning::NotConverged` also covers
  a solver that accepts no step while the residual slope stays
  above 1e-6 (the starting point is returned with its R², never
  silently); a parameter stuck to its bound stays converged. A non-finite
  slope (e.g. `sqrt(a)` at a = 0) is the `ModelNotEvaluable` error. In
  deconvolution, `Warning::IrfTruncated` signals a response cut above
  1% of its peak at the chosen shift (the kernel stays renormalized). In
  correction by reference, an empty or non-finite cell in the middle of the
  reference is bridged by interpolation and the data points that fall
  there are counted (`Warning::BridgedReferenceGap(n)`); a reference row
  without X, or a `NaN` at the end of the reference, does not count.
  `Warning::DivisionByZero(n)` counts the divisions by zero (correction by
  reference, baseline in Divide) returned as `NaN`; an overflow with a
  non-zero divisor is not counted. The mean durations of the reconvolution
  are `NaN` (never ±∞) when the sum of the amplitudes is zero.
- Length of a series (#313): a calculation reads **the length of the X column**
  if given, otherwise that of Y; Y and σ are truncated or padded with
  empties to this length. The Y rows beyond X have no abscissa:
  the output stops at X and a replacement of the original data keeps
  their value.
