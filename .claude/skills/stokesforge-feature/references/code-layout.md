# Sheet: where UI code goes

Read when adding or moving code in `stokesforge-app` (new surface, menu, window, building block, test, guard) or splitting a giant function.

## Where the rule lives

- Module → role table: `docs/application/coquille-et-code.md` § Découpage du code.
- Layout rules (one module per surface, explicit imports, `app.rs` = orchestration, tests follow their code): `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, first point `découpage d'app.rs`.
- In-place splitting, témoins: `docs/tests.md` § Redécoupe des fonctions d'interface (#124) ; `CLAUDE.md` § Délégation et choix du modèle.
- Guards that read the code (single reader): `docs/tests.md` § Les garde-fous à ne pas casser.

## Check

- [ ] drawing and events in the surface module; nothing new in `app.rs` beyond orchestration
- [ ] new surface: explicit imports, `//!` header, row added to the table of `docs/application/coquille-et-code.md`
- [ ] no `use ...::*` outside tests; visibility `pub(super)` at most
- [ ] a restructuring is its own commit, without changing egui IDs or event order
- [ ] a test moves with its code; a guard whose function was renamed is updated, not bypassed
