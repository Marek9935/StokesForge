# Fluent mechanics: keys, `tr!`, pseudo-language, guards

Read from `stokesforge-textes-interface` when writing a key. Overview and the *why*: `docs/application/theme-i18n-systeme.md` § Interface multilingue (Fluent), and the header of `crates/stokesforge-app-state/src/i18n.rs`.

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

- `aucun_libelle_en_dur` (`crates/stokesforge-app/tests/libelles.rs`) reads every `src/` of both crates and refuses a text literal in label-like calls (`label`, `button`, `checkbox`, `on_hover_text`, `heading`, `hint_text`, `set_title`, the visible name of `add_filter`, the free functions of the common look…). **Absolute, no exemption** (#48). It does not see a `&str` passed through a variable nor a `modal_dialog_with` title: the `en-XA` capture covers those. `ZONE_43` forbids any text literal in the graph properties functions.
- `aucune_cle_du_catalogue_n_est_morte`: an `en` key used by no source fails.
- `i18n.rs` tests: `toute_cle_francaise_existe_en_anglais_avec_les_memes_arguments` (blocking), its témoin `la_completude_detecte_une_cle_inconnue_et_un_argument_different`, `la_pseudo_langue_entoure_et_allonge_chaque_message`, `une_preference_corrompue_demarre_en_anglais`. `libelles.rs` also holds the guard's témoin `la_garde_voit_un_litteral_dans_chaque_forme_d_appel` and `les_dependances_de_traduction_restent_dans_stokesforge_app`.
