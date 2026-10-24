# Sheet: Object Manager, content selection, analysis corrections

Read when the task touches the Object Manager, a graph selection, object drag-and-drop, or a definitive analysis correction.

## Where the rule lives

- Object Manager (#81-85), view settings (#139), shared selection, `with_graph_view`: `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application/gestionnaires.md` § Gestionnaire d'objets (Object Manager.
- Series, group, drag by the handle: `.claude/conventions.md` § Séries et groupes.
- Preference isolation in tests: `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, piège `cfg(test)`.
- Definitive corrections (#212), `ReplaceAnalysisData`: `.claude/conventions.md` § Analyse (#181) ; tests: `docs/tests.md` § Remplacement définitif d'une correction (#212).

## Check

- [ ] no parallel selection in the panel: the selection stays in `GraphView`
- [ ] one action on drag release (`PlaceSeries`, `PlaceSeriesGroup`, `ReorderObject`)
- [ ] `AppState` test built with `new_with_preferences`, never the real configuration
- [ ] correction: versions of every input required, trace undone with the values
