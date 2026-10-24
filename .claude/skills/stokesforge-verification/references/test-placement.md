# Where a test goes, and what makes it prove something

Read at step 1 of `stokesforge-verification`. Families and guards: `docs/tests.md` § Trois familles ; `docs/tests.md` § Les garde-fous à ne pas casser ; before writing: `docs/tests.md` § Ce qu'il faut savoir avant d'écrire un test.

## Placement

| What is tested | Where |
|---|---|
| local behaviour | `#[cfg(test)]` in `src/`, next to its code |
| crate property: public contract, boundary, agreement between paths | `crates/*/tests/` |
| the core at its limits | `stokesforge-robustesse/tests/b1`…`b22` (index in its `src/lib.rs`) |
| an `AppState` decision | `crates/stokesforge-app-state/tests/shell.rs` (`cargo test -p stokesforge-app-state`, no egui to compile) |
| a painted gesture (click, key, layout read from shapes) | the `tests` module of the surface (`src/app/<surface>.rs`), without a window |
| orchestration (panel order, deferred application, confirmations, global shortcuts) | the `tests` module of `app.rs`, and nothing else |

A decision that must live near the UI is extracted as a **pure function** and tested (`app::grid::column_grip_bands`, `state::active_graph`, `state::drop_payload_action`). Tests follow their code: moving a function moves its tests. A test helper shared between `app` modules is `pub(in crate::app)` in the test module that owns it. A target painted by hand also checks its name, role and state in the published AccessKit tree.

## The témoin

A test comparing two paths can pass because both are right — or because **both are wrong the same way**. It needs a case that must stay **different**: `docs/tests.md` § Le témoin, sans lequel un test d'égalité ne prouve rien. Under `CLAUDE.md` § Délégation et choix du modèle, each témoin is checked by mutation (break the code, see the test fail, restore).

## Reproduction vs guard

A **reproduction** fails on purpose and carries `#[ignore = "<reason>"]` (with the issue number). When it starts passing, remove `#[ignore]`: it becomes the guard.

## Measure, do not time

`b17` and `b21` count **allocated bytes**, not milliseconds, so the assertion holds on any machine. Follow that for any new cost test.

## Interface standard building blocks

Which guards and building blocks are *in place* or *to come*: `docs/standard-interface.md` § Briques et gardes. A guard "to come" does not exist yet: never cite it as green. The commit that writes one switches it to *in place* there and in the skills.
