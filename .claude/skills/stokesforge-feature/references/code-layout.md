# Sheet: where UI code goes

Read when adding or moving code in `stokesforge-app` (new surface, menu, window, building block, test, guard) or splitting a giant function.

## Where the rule lives

- Module → role table: `docs/application/coquille-et-code.md` § Code split.
- Layout rules (one module per surface, explicit imports, `app.rs` = orchestration, tests follow their code): `.claude/conventions.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards, first point "`app.rs` split".
- In-place splitting, witnesses: `docs/tests.md` § Re-split of interface functions (#124) ; `CLAUDE.md` § Delegation and model choice.
- Guards that read the code (single reader): `docs/tests.md` § Guards not to break.

## Check

- [ ] drawing and events in the surface module; nothing new in `app.rs` beyond orchestration
- [ ] new surface: explicit imports, `//!` header, row added to the table of `docs/application/coquille-et-code.md`
- [ ] no `use ...::*` outside tests; visibility `pub(super)` at most
- [ ] a restructuring is its own commit, without changing egui IDs or event order
- [ ] a test moves with its code; a guard whose function was renamed is updated, not bypassed
