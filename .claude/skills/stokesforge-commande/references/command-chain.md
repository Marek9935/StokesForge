# The command chain, link by link

Read while walking the chain of `stokesforge-commande`. Each point is a convention already arbitrated: follow it rather than replay it.

## What a command is

- **A value, not a stateful object.** It holds no `Project`, no `Tree`, no `Arc`: that is what lets it cross a thread channel as is. Everything needed to undo is computed **at apply time** by `History`, never carried by the command. Source: `docs/commandes-et-historique.md` § A command is a value.
- **One creation command per node kind** (`AddFolder`, `AddWorkbook`, `AddWorksheet`, `AddGraph`, `AddMatrix`, `AddNote`); `RemoveNode` stays generic.
- **Content edits replace their target as a block** (`SetWindowMeta`, `SetNoteText`, `SetColumnMeta`, `SetPlotSpec`, like `SetCells`). `EditPlotSpec` reuses the `SetPlotSpec` delta and adds only a closed `PlotEditKind` for graph gestures.
- **`expected_version`** is the optimistic condition on cell writes: `None` checks nothing (the UI), `Some(v)` requires `Column::version == v` when applying.
- **Every column data write draws a new version** through `Project` primitives (`next_column_version`, `insert_node` for any node addition), never the `Column` directly: targeted formula recalculation (invariant 11) compares versions. A command that can target a formula column gets a named case in `stokesforge-session/tests/recalc_targeted.rs` (`docs/tests.md` § Targeted recalculation (#193) — and what to test when relying on it).
- **A formula keeps its columns by id** (`ColumnFormula`, #321): inserting, deleting or moving columns never changes what a formula reads. `SetColumnMeta` receives a `ColumnFormula` already built by `Tree::formula_from_letters`, **above** the command.

## Structural or data — the cost question

History is hybrid: structure by tree snapshots, data by deltas (`docs/commandes-et-historique.md` § The history is hybrid, and that is a cost decision). Hence strict LIFO (invariant 8). Classifying a data command as structural costs a tree snapshot per call; the reverse breaks undo. In doubt, look at an arbitrated neighbour: `SetCellsPromoting` changes a column's type and stays "data" because its delta is a **column** snapshot — one history entry brings back type and value.

## History label

The label is **data, not a sentence** (#46): a `HistoryLabel` variant (`src/label.rs`) with its parameters, its `Display` line (English, logs and CLI), and the example in `tests/labels.rs`. A group opened by `begin_macro` is a closed `MacroKind`. Source: `docs/commandes-et-historique.md` § History labels: data, not a sentence.

Trap: a column has two identities and only its **position letter** (A, B, C…) is visible. A label designates a column by position (`ColumnRef`, from `command::column_ref`; letter by `stokesforge_core::spreadsheet_name`); other node kinds keep their real name, shown in the Project Manager.

The sentence is written in `stokesforge-app-state`: a `history-*` key in `i18n/en/stokesforge_app.ftl` (and `fr`), and the branch in `src/history_text.rs` (exhaustive `match`). `tests/history_text.rs` checks that the catalogue and the `Display` agree.

## Delta

A data command that **moves rows** rewrites, in the same delta, the row references that depend on them (row series of graphs, field `plots` of `InsertRows`/`DeleteRows`, #307): specs before/after captured at apply time. Source: `docs/commandes-et-historique.md` § A data command that moves rows rewrites the row references (#307).

## Persistence

`convert.rs` / `schema.rs` have their own transport types: `stokesforge-plot` does not depend on `serde` (invariant 3). An unknown enum variant is kept in a `RawVariant` — loss bounded to the node (invariant 10). Version rule: `docs/persistance.md` § Version rule: every V1 file can be read back up to V3 (additive `#[serde(default)]` field → regenerate `tests/witnesses/v1/project.stf` with an off-default value in `witness_project`; removal, rename or change of meaning → bump and migrate).

## Interface

An `Action` variant, handled in `AppState::dispatch`, which calls `refresh` after a mutation: the UI's snapshot is frozen (persistent tree). A command may exist without an `Action` until a gesture needs it. Lifecycle (#73): `SessionHandle::new_project`, like `load`, replaces identity and clears history; it is not a mutation of the current project. An empty graph is a mutation: `Command::AddGraph`.
