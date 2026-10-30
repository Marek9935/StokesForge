---
name: stokesforge-command
description: Add or change an undoable StokesForge command, the only way to mutate the model. Use when a task creates, deletes, renames, moves or edits anything in the project (« ajoute une colonne » / "add a column", « renomme » / rename, « supprime » / delete), or adds a pub mutating method to stokesforge-core.
---

Invariant 1: no model mutation outside a command (`docs/invariants.md` § 1. No model mutation outside a command). A `pub` mutating method added to `stokesforge-core` without its command opens a non-undoable write path. No test catches that today (`docs/invariants.md` names `b21`, which measures cost and identity instead): review is the only guard.

## Read first

`docs/commands-and-history.md` § What to know before modifying, then the header of `stokesforge-commands/src/command.rs` and `Command::is_structural` — the code is the list that counts. `.stf` touched: `docs/persistence.md` § What to know before modifying, then `docs/archive/rustrewrite/format-fichier-plx.md` for options already ruled out.

## Walk the chain

Stop at the links the command really touches; detail and traps of each link: [references/command-chain.md](references/command-chain.md).

1. `stokesforge-core`: the mutation primitive (`&mut self` on `Project`). Search first: it may exist.
2. `stokesforge-commands/src/command.rs`: the `Command` variant, its `is_structural` class, its `HistoryLabel`.
3. `stokesforge-commands/src/history.rs`: the delta computed at apply time and its undo.
4. `stokesforge-persistence`: only if the state must survive reload.
5. `stokesforge-session`: usually nothing; check whether a new response type is needed.
6. `stokesforge-app-state`: an `Action`, its `dispatch`, the `history-*` sentence; then the UI surfaces with `stokesforge-feature`.

Object copy (#105–107) and definitive analysis corrections (#212) reuse existing commands: `.claude/conventions.md` § Copying objects and figures ; `.claude/conventions.md` § Analysis (#181).

A size or layout edit of a graph (#394) is an `EditPlotSpec` (`PlotEditKind::ResizeCell`, `AddCell`, `RemoveCell`): a graph without stored cell sizes is made explicit **inside the same command** (`stokesforge_app_state::cell_sizes`), so one Undo returns to the legacy file.

## Tests

| Link | Test |
|---|---|
| undo / redo | full round trip: state before == state after undo |
| structural nature | LIFO holds in a macro (`stokesforge-commands/tests/history.rs`) |
| history label | the example in `stokesforge-commands/tests/labels.rs` (exhaustive) |
| persistence | `.stf` round trip (`stokesforge-persistence/tests/roundtrip.rs`); additive field → regenerate the witness file (`tests/witness_files.rs`) |
| column data write | the gesture added to the draws of `b14` and `b22` |
| memory | `stokesforge-commands/tests/memory_pinning.rs` |
| interface | `stokesforge-app-state/tests/shell.rs` for the `Action` |

Extend the dedicated batteries rather than duplicating them: `b5` (macros), `b12` (accepted no-op commands), `b14` (random sweep), `b18` (open macro); index in `stokesforge-robustness/src/lib.rs`.

Done when every touched link has its test, the structural/data class is justified by the delta's cost, the label is data (`HistoryLabel`) with its `history-*` sentence, and `stokesforge-verification` is green.
