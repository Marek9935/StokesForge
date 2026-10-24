# Sheet: layer choice, graph gestures, layers

Read when the feature touches a graph (continuous gesture, free text, adding series, export) or when hesitating between `state.rs` and `src/app/`.

## Where the rule lives

- State / shell split: `docs/application/coquille-et-code.md` § Deux crates : l'état et la coquille ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes.
- Action that needs the view state (zoom): `docs/application/graphe.md` § Propriétés des graphiques (`GraphView::framed_spec`, `Action::ExportGraph`).
- Continuous gesture, in-place text editing: `docs/application/graphe.md` § Propriétés des graphiques (`annotation_interactions`) ; `docs/rendu.md` § Couleurs du texte et des échelles : l'encre automatique.
- Layer by index, active layer, adding series: `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue) ; `docs/application/graphe.md` § Glisser une sélection de colonnes vers un graphe.
- New graph without a title: `.claude/conventions.md` § Graphes neufs et fenêtres détachées.
- Composing series from columns: `.claude/conventions.md` § Graphes neufs et fenêtres détachées (point `Composition d'un graphe (#257)`).

## Check

- [ ] an action that reads the view state receives it in the `Action` (never read in `state.rs`)
- [ ] a continuous gesture emits one action, at validation; the preview lives in `GraphView`
- [ ] the layer is addressed by its index in `PlotSpec::panels` (active layer when nothing was clicked)
- [ ] adding series reuses `drop_payload_action` / `drop_rows_action`
- [ ] new graph: `PlotSpec::title` empty
- [ ] no business rule in `app`: it would be unverifiable in CI
