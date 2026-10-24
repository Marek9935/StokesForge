# Import: modules and frequent symptoms

Read from `stokesforge-import` to find where a piece of the import lives. Each module's header doc is the authority; this map only routes.

## Modules of `crates/stokesforge-import/src/`

| Module | Role |
|---|---|
| `recipe` | the recipe model: a frozen, serialisable description of an instrument format; knows nothing of parsing |
| `parse` | applies a recipe to a file |
| `layout` | **the** split of a file into zones (header, sub-header, names, data, footer, comments, skipped lines), shared by `parse` and `preview`; `split_lines` is the only line splitter |
| `data_end` | the single rule for where data stops (`DataEnd`), shared by `parse` and `preview` |
| `numbers`, `datetime` | numbers (decimal `Auto`, thousands, missing values; a non-finite result is `NaN`, token kept as text) and dates (`date_order`, `parse_datetime_with`) |
| `builtin`, `choose` | built-in templates, and automatic choice (extension, name, content signature) |
| `preview` | classifies each shown line by role for the live preview, with the same split as `parse` |
| `store` | saves and reloads user recipes (JSON), never panics on a corrupt file |
| `decode` | bytes → text before any split (UTF-8, or the Windows code page of instrument software; mixed decoding in `Auto`) |
| `metadata` | header (and footer if asked) as key / value lines |
| `recent` | recently imported files and remembered folder (`import_recent.json`) |
| `config_dir` | the single rule for the configuration folder (`.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point « Dossier de configuration ») |

`crates/stokesforge-importer/src/lib.rs`: from parsed tables to model columns (`plan_import`, `make_import_column`, `MetaLine`). `crates/stokesforge-persistence/src/recipe_record.rs`: the recipe as written in the `.stf`.

## Symptom → where to look

| Symptom | Look at |
|---|---|
| columns merged into one | the recipe's `Separator` (tab vs multiple spaces vs `;`) |
| numbers read as text | `DecimalStyle` — a European instrument writes `1,234` |
| first data lines swallowed | `HeaderSkip` / `HeaderLayout` |
| trailing junk lines (sums, notes) | `footer_skip_lines`, or `data_end` (`DataEnd::Auto` cuts at the first non-numeric X once armed) |
| column names missing or wrong | `has_column_header_row`, `NameSource` |
| long name / unit / comment / x' misplaced (#164) | `SubheaderRoles` |
| parasitic prefix on every line | the leading-character removal of data lines |
| `°C` garbled | `decode` (forced encoding vs `Auto`) |
