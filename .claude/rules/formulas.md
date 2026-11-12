---
paths:
  - "crates/stokesforge-session/**"
  - "crates/stokesforge-core/src/formula.rs"
  - "crates/stokesforge-commands/**"
---

## Formula recalculation (#193)

- Formula recalculation is targeted by fingerprint (`stokesforge-session/src/recalc.rs`, invariant 11). **Any data read by a formula computation enters the fingerprint**: a column's version covers only type and values, not name, meta or position; membership of a read column in the sheet enters it, the displayed letters do not (a formula keeps its columns by identifier, #321).
- **Any command that writes data to a column draws a fresh version** per `Project`, even over identical values, while the recalculation pass draws one only when type or values change (#439).
- Full recalculation remains as the oracle (`FormulaRecalc::Full`, batteries `b22` and `b23`).
- To extend the mechanism (formula function, #184, #186): the tests to write are in `docs/tests.md`, "Targeted recalculation".
