---
paths:
  - "crates/*/src/error.rs"
  - "crates/stokesforge-app-state/src/errors.rs"
  - "crates/stokesforge-app-state/i18n/**"
---

## Errors: raw English below, catalog sentence in the application

The `Display` (`#[error(...)]`) of the lower crates (core, commands,
persistence, import, script, render-svg…) is **raw English**, concise and
readable by a user of the CLI or of `pyst`: it is never translated and none
of these crates depends on Fluent. The application phrases every displayed error
through `crates/stokesforge-app-state/src/errors.rs`:

- one function `error_message(&Erreur) -> String` per family
  (`errors::model`, `commands`, `session`, `file`, `import`, `formula`,
  `number` (a typed number out of the `f64` range, shared by formulas, cells and
  custom Fit models, #429, #438: the model check lives in `Operation::readiness` only,
  never in `Expr::parse`, `Model::compile` or `validate`, which stay lenient for stored models),
  `render`, `export`, `script` under the `python` feature);
- each variant → one key `error-<family>-<cause>` of the `en` catalog, its
  fields as named arguments (`tr!("error-file-corrupt", path = …, detail = …)`);
- the `match` is **exhaustive, without `_`**: a new variant does not compile
  until it is mapped. With no sentence, `errors::fallback` returns
  its English `Display` (nothing disappears);
- the test of the family builds a value of each variant and checks
  non-empty sentence, no jargon (S-R9), no final period: add one for the
  new variant;
- a `detail` (cause described by the originating crate) or a system message
  is taken as is, in English;
- **never** `to_string()` nor `{err}` of an error shown to the user.
- a `std::io::Error` goes through `errors::io::error_message(&err)` (keys
  `error-io-*`), and the context message receives it as a variable
  (`tr!("error-template-export", cause = …)`). `ErrorKind` is
  `#[non_exhaustive]`: this family keeps a fallback `_` (`error-io-other`),
  the only exception to "without `_`".
- a formula the F(x)= field **keeps** (#433) says why through the same
  `errors::formula::error_message`, as a failure on the status bar and as the
  tooltip of the frame (`AppState::formula_issue`, memoised by revision and
  epoch, the error kept and the sentence built afterwards so that a language
  change follows). `FormulaError::Parse` is mapped to `error-formula-syntax`,
  which holds **no `detail`**: the parser's raw text stays in the `Display` for
  the CLI and `pyst`;
- the status bar messages (`AppState::notice` / `warn` / `fail`) go through
  `tr!` (keys `status-*` for information or a warning, `error-*`
  for a failure); the guard `no_hardcoded_label` covers `notice`, `warn`, `fail` and
  `progress`.
  **One rule for the line (S-R18, #453)**: red = not done, orange = done but not as asked or needs your eyes, plain = done as asked. A message takes the line by that rule (most serious of the gesture, last on a tie; a plain message with no gesture never replaces orange or red), so a gesture says its messages in any order: no "set it last" trick, no `paste_warnings`-style buffer. A new emitter outside `dispatch` that says several messages wraps its body in `AppState::gesture`; a poll or timer in `AppState::background`; a progress line is `progress`, never `notice`. Orange fades after 30 s, red stays. A script exception reads "Script failed. See the console" (S-M10).

Same pattern for the **names of analysis output columns** (#256): the core
returns a data item (`stokesforge_core::OutputName`, with `english()`), the application
phrases it through `analysis::output_name_text_in` (exhaustive `match` without `_`
to `analysis-output-*`), and `tests/output_names.rs` guards the agreement of
the catalog's English with `english()`: changing one of these texts means changing
both.

Wording: what happened, then what to do only if useful;
Sentence case, no final period on a single line, no "please", no
apology, no internal jargon (*node, session, macro, command, tree*). A wrong
or ambiguous message is fixed in the catalog (`en`), not in the lower
crate — unless the raw English text itself is wrong.
