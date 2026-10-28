# Fluent mechanics: keys, `tr!`, pseudo-language, guards

Read from `stokesforge-ui-texts` when writing a key. Overview and the *why*: `docs/application/theme-i18n-system.md` § Multilingual interface (Fluent), and the header of `crates/stokesforge-app-state/src/i18n.rs`.

## Writing a key

- Name in lower case and hyphens, `zone-element[-precision]` (R16): `menu-file`, `plot-axis-log-scale`. A control's tooltip is an attribute of its key (`.hint`, read by `tr!("key", "hint")`); a greyed entry's reason is `.disabled`. An action shared by menu, context menu and status bar has **one** `action-*` key.
- Use it with `tr!("menu-file")` or named arguments `tr!("witness-count", count = n)`; it returns a `String`. Import: `use stokesforge_app_state::tr;` (explicit, never a glob). `tr!` wraps `fl!` and is the **only** entry point: never call `fl!` or the loader directly.
- A user-chosen name is passed as an argument, never put in the catalogue (R15). A test looking for a painted text gets it through `tr!`, never a literal; a test comparing one language builds its loader and writes `tr!(in loader; "key", arg = …)`.
- A number that changes the sentence is a Fluent selector in the `.ftl` (`{ $count -> [one] … *[other] … }`), never an `if` in Rust nor two keys.
- French (`fr/stokesforge_app.ftl`): infinitive for instructions, only the first letter capitalised, non-breaking space (U+00A0) before « : », « ? » and inside « », same arguments as `en`, French CLDR plural (singular for 0 and 1). A key removed from `en` is removed from `fr`.
- **History labels** (#46): one `history-*` key per `HistoryLabel` variant (and per form), wired in `stokesforge-app-state/src/history_text.rs`; Title Case, verb first; a user name between “ ”; a column is « Column A ». `stokesforge-app-state/tests/history_text.rs` checks the `en` catalogue and the `Display` of `stokesforge-commands` agree.
- Status bar messages (`AppState::notice` / `warn` / `fail`): `status-*` for information or warning, `error-*` for a failure.

## What the compiler checks, and what it does not

`fl!` reads the `en` catalogue at compile time (through the calling crate's `i18n.toml`; both crates set `domain = "stokesforge_app"`): **an unknown key or an undeclared argument does not compile**. It does not see a key present in one language only, different arguments between languages (the `i18n.rs` tests), nor a hard-coded literal (the guard below).

## Language choice

`STOKESFORGE_LANG` (`en`, `fr`, `en-XA`), then the `language` preference of `preferences.json`, otherwise **English**; the system language is not followed (user's decision). File › Language switches without restart (`choose_language` → `i18n::set_language`, then `Preferences::save_language`). A test comparing an exact text pins English; a test that changes the global language has its own binary (`stokesforge-app-state/tests/language_switch.rs`).

## Pseudo-language `en-XA`

Built on the fly from `en` (`stokesforge-app-state/src/i18n/pseudo.rs`): accented letters, text 30 % longer, message framed by `[[ ]]`; arguments untouched. The frame is ASCII because Noto Sans lacks `⟦ ⟧`, written as the Fluent literal `{ "[[" }`.

## Guards

- `no_hardcoded_label` (`crates/stokesforge-app/tests/labels.rs`) reads every `src/` of both crates and refuses a text literal in label-like calls (`label`, `button`, `checkbox`, `on_hover_text`, `heading`, `hint_text`, `set_title`, the visible name of `add_filter`, the free functions of the common look…). **Absolute, no exemption** (#48). It does not see a `&str` passed through a variable nor a `modal_dialog_with` title: the `en-XA` capture covers those. `ZONE_43` forbids any text literal in the graph properties functions.
- `no_catalog_key_is_dead`: an `en` key used by no source fails.
- `i18n.rs` tests: `every_french_key_exists_in_english_with_the_same_arguments` (blocking), its witness `completeness_detects_an_unknown_key_and_a_different_argument`, `the_pseudo_locale_frames_and_lengthens_every_message`, `a_corrupted_preference_starts_in_english`. `labels.rs` also holds the guard's witness `the_guard_sees_a_literal_in_every_call_form` and `translation_dependencies_stay_in_stokesforge_app`.
