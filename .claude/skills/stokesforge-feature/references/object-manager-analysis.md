# Sheet: Object Manager, content selection, analysis corrections

Read when the task touches the Object Manager, a graph selection, object drag-and-drop, or a definitive analysis correction.

## Where the rule lives

- Object Manager (#81-85), view settings (#139), shared selection, `with_graph_view`: `.claude/rules/menus-panels.md` § Object Manager, fixed panels, menus ; `docs/application/managers.md` § Object Manager (#81–85).
- Series, group, drag by the handle: `.claude/rules/series.md` § Series and groups.
- Preference isolation in tests: `.claude/rules/app.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards, pitfall `cfg(test)`.
- Definitive corrections (#212), `ReplaceAnalysisData`: `.claude/rules/analysis.md` § Analysis (#181) ; tests: `docs/tests.md` § Final replacement of a correction (#212).

## Check

- [ ] no parallel selection in the panel: the selection stays in `GraphView`
- [ ] one action on drag release (`PlaceSeries`, `PlaceSeriesGroup`, `ReorderObject`)
- [ ] `AppState` test built with `new_with_preferences`, never the real configuration
- [ ] correction: versions of every input required, trace undone with the values
