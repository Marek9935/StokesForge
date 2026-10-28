# Application: theme, multilingual interface, file picker

Continuation of the table of contents [`README.md`](README.md). Contents: theme, multilingual interface (Fluent), native file picker.

### Theme

The app **does not follow** the system light/dark theme: `stokesforge-app::theme`
sets, once at startup (`main.rs`, in the creation closure of
`eframe::run_native`, next to the font setup), a house palette derived from
`egui::Visuals::light()` — never from the dark one. `Context::set_theme`
locks the preference on `Theme::Light` (this stops egui from following the
system again at the slightest `winit` notification), then `Context::set_visuals`
applies the palette (`theme::visuals`): forest-green accent (selection,
links, outlines of active/hovered widgets), corners slightly less rounded
than the egui default. The rest (background grey, text contrast) comes
as is from egui's light theme, already tuned for readability.

There are two reasons for this choice, documented at the top of `theme.rs`: `stokesforge-render`
draws plots on an implicit light background (axes, grids, black text) —
a dark theme would break this contrast without any rendering code being
told; and a theme that changes with the system is a theme that cannot
be guaranteed in a screenshot or in support.

**Scroll bars: one standard.** All the bars of the
application (workspace, grid, panels, import and script
windows, detached windows) look the same: 10 points thick,
muted dark grey handle, which turns to the hover color then to the accent
during a drag. `theme::scroll_style` defines it, `theme::install` sets it
once on egui's global style; changing the `SCROLL_BAR_*` constants of
`theme.rs` changes all the bars at once. Rule for future code: never
build or modify a `ScrollStyle` outside `theme.rs`; an area
that must reserve room for a bar reads it through
`theme::scroll_bar_allocated_width`. The test `no_scroll_style_outside_the_theme`
fails the CI otherwise.

**Icons.** Phosphor duotone, a single component (`icons.rs`: `icon_image`,
`icon_button`, single size, mandatory tooltip), color "neutral by
default, rare afterwards": rules and procedure in [`icones.md`](../icones.md).
The test `no_icon_outside_the_component` forbids loading any icon
elsewhere.

### Multilingual interface (Fluent)

The interface is written **in English first**; French is a translation of it
(work package #39). Only the application translates: Fluent (`i18n-embed`,
`i18n-embed-fl`, `rust-embed`, plus `fluent-syntax` for the pseudo-language) is
a dependency of `stokesforge-app-state` (catalogs, loader, `tr!`) and of `stokesforge-app` (the compile-time check of `fl!`) and of no other crate — `core`, `commands`,
`session` and the others return structured data that the application turns
into sentences (#46, #47). The reason for choosing Fluent (key and arguments checked
at compile time) is at the top of `stokesforge-app-state/src/i18n.rs`.

| Where | What |
|---|---|
| `crates/stokesforge-app-state/i18n/en/stokesforge_app.ftl` | the reference catalog; a key missing from here does not exist |
| `crates/stokesforge-app-state/i18n/fr/stokesforge_app.ftl` | the French translation, complete (#49): same set of keys as `en`, same commit |
| `crates/stokesforge-app-state/i18n.toml`, `crates/stokesforge-app/i18n.toml` | `fallback_language = "en"`, catalog folder; `fl!` reads the `i18n.toml` of the **calling** crate, so both set `domain = "stokesforge_app"` and the app's one points to the `app-state` catalog |
| `stokesforge-app-state/src/i18n.rs` | loader, current language, `tr!` macro; `src/i18n/pseudo.rs`: the pseudo-language |
| `stokesforge-app-state/src/preferences.rs` | `preferences.json`, same configuration folder as `couleurs.json` |
| `stokesforge-import/src/config_dir.rs` | the single rule for the configuration folder (`STOKESFORGE_CONFIG_DIR`, #332) |
| `stokesforge-app-state/src/history_text.rs` | the history labels (`HistoryLabel`) as sentences, `history-*` keys (#46) |
| `stokesforge-app/tests/labels.rs` | the `no_hardcoded_label` guard (absolute: no exemption), which reads the sources of both crates, and the dead keys |

**`tr!` is the single entry point**: `tr!("menu-file")`, or with named
arguments `tr!("history-insert-rows", count = n)`. It wraps `fl!`; a key or an
argument unknown to `en` does not compile. No literal in a label. For the tooltip of a control (`.hint` attribute of its key): `tr!("project-manager-open", "hint")`. A test
that compares a specific language passes its loader: `tr!(in loader; "clé")`
(`build_loader`), so as not to depend on `STOKESFORGE_LANG`; a test that looks for a painted text asks `tr!` for it, never a literal.

**Menus and actions (#42)**: an action has **one** label, the `action-*` key, shared
by the menu bar, the context menus, the status bar and the detached
window button (`action-reopen-last-import`, `action-reset-zoom`…); the
menus themselves are `menu-*`. The tooltip (the effect, not the shortcut, which the
menu already draws) is the `.hint` attribute (`tr!("action-save", "hint")`);
the tooltip of a disabled entry, which says why, is `.disabled`. The key
names (`shortcut-backspace`, `shortcut-delete`) are keys too. 
The menu bar and the status bar are `StokesForgeApp::menu_bar` and
`StokesForgeApp::status_bar` (`app/menus.rs`, #122); their tests open
every top-level menu for real and read labels, disabled state and checkboxes
in the accessibility tree that egui publishes (AccessKit), without
assuming any position.

**Language**: the `STOKESFORGE_LANG` variable (`en`, `fr`, `en-XA`), then the
`language` preference of `preferences.json`, otherwise English. The system
language is **not** followed (decision of 2026-10-01). An unknown value or
a corrupted file falls to the next level, without panic (invariant 10).
The menu **File › Language** changes the language right away (`set_language`, texts
are redrawn at the next frame) and writes it to `preferences.json`;
`STOKESFORGE_LANG` still wins at the next startup.

**Development variable `STOKESFORGE_DEMO=1`** (#333): opens the in-memory
demonstration at every launch, instead of only at the first opening (see
`etat-et-console.md`, "Binary"). Set by the `run-stokesforge` driver. Closing
then does **not** ask to save (`set_quit_without_prompt`); New and Open… still ask, and an
existing project passed as an argument keeps the question.

**Configuration folder** (#332): the `STOKESFORGE_CONFIG_DIR` variable,
if set and non-empty, designates the folder as is (no suffix);
otherwise the system folder (`directories::ProjectDirs`, `…\StokesForge\config`
on Windows). `preferences.json`, `couleurs.json`, `import_recipes.json` and
`import_recent.json` live there; the backup copies in
`<STOKESFORGE_CONFIG_DIR>/recovery`, otherwise `recovery/` of the data folder.
Useful for a portable installation, and this is how tests
isolate themselves from the real profile (`docs/tests.md`, "Rule (#250)"). Rule and functions:
`stokesforge_import::config_dir`.

**Pseudo-language `en-XA`**: built on the fly from `en` (accents, `[[ ]]`,
+30%), without a file. With `STOKESFORGE_LANG=en-XA`: a text without `[[ ]]` has not gone
through the catalog, a missing `]]` signals a truncated text.

**Guard `no_hardcoded_label`** (`tests/labels.rs`): no text literal
in `label`, `button`, `menu_button`, `checkbox`, `radio_value`,
`selectable_value`, `selectable_label`, `on_hover_text`, `heading`, nor in the
`Button::new(…)`/`RichText::new(…)`, nor in the `fn label()`/`fn title()` tables.
It is **absolute**: no exemption (the 213 literals from before #41 were
converted, #42 to #45, and the list was removed at #48). It also sees `strong`,
`weak`, `small`, `monospace`, `colored_label`, and ignores format fields
(`{code}`). The native file boxes are guarded too (#127):
`set_title` for the title, `add_filter` for the visible name of the filter; the
extensions and the format acronyms (`SVG`, `PDF`, `PNG`) stay technical.
The open and save titles as well as the project filter
go through `tr!`, in the current language at the moment the box opens.
Zone #43 (graph properties, text editor, dialog footer,
export) is guarded more strictly (`ZONE_43`: no text
literal in its functions, whatever the form of the call). A key of the
`en` catalog that no source in `src/` uses makes
`no_catalog_key_is_dead` fail (`tests/labels.rs`, which reads `src/` of both crates). Detailed mechanics: skill
`stokesforge-textes-interface`.

**Displayed errors (#47).** The crates below return structured errors
whose `Display` (`#[error(...)]`) is in **raw English** — what
the CLI, `pyst` and the logs read — and which nobody translates. The application
turns them into sentences through `stokesforge-app-state/src/errors.rs`: one `error_message` function per
family (`model` for `CoreError`, `commands`, `session`, `file` for
`PersistenceError`, `import`, `formula`, `render`, `export`, and `script` with the
`python` feature), which maps each variant to an `error-*` key of the `en`
catalog with its fields as arguments. The `match` is exhaustive, without `_`: a
new variant does not compile until it is mapped (or deliberately sent back
to its `Display` by `errors::fallback`). The test of each
family builds one value of each variant and checks that the sentence
is not empty, has no internal jargon, and has no final period. Every error shown to the
user goes through these functions, never through `to_string()` or `{err}`.
The `detail` fields (cause described by the originating crate) and the system
messages stay in English in the sentence. `stokesforge-importer` returns a structured
`ImportFailure` (file + cause) for the same reason.

Rule of conduct (full chain, turned into a sentence): `.claude/conventions.md`, section "Errors: raw English below, catalog sentence in the application".

### Native file picker

The file banner was removed: Open, Save and Save As
are gathered in the File menu. Clicks and their shortcuts
share the same routing. Open (Ctrl+O) shows an `rfd::FileDialog`;
Save As (Ctrl+Shift+S) asks for a destination; Save
(Ctrl+S) uses the current destination.

**Separate thread (#192).** All the pickers (Open, Save As, graph export,
import wizard) go through `Action::PickFile(PickRequest)` and
`file_picker.rs` (in `stokesforge-app-state`): `rfd` runs on a dedicated thread, the answer comes back through a
channel polled at every frame (`StokesForgeApp::poll_file_picker`). Before, the
synchronous call on the egui thread froze the application when the picker closed
without the portal answering (`wait_for_response` of `rfd` has no timeout);
now, at worst, the request stays pending. Only one picker is tracked at a time: a new request replaces
the old one, otherwise a picker closed without an answer from the portal would block
Open afterwards (the abandoned thread stays blocked in `rfd`, with no effect).
Guard: `no_rfd_picker_outside_the_module`. To be validated on a real GNOME/Wayland
desktop (click on the application while the picker is open); on macOS, the
pickers off the main thread remain to be checked. The picker is not yet
attached to the window (`set_parent`), not needed for the unfreeze.

`rfd` (default feature `xdg-portal` + `wayland`) compiles and links without a
system portal present: the portal (`xdg-desktop-portal`, D-Bus) is
only a runtime dependency. Verified by successful `cargo build` and
`cargo build --release` in a sandbox without a guaranteed graphical session.
The real opening of the picker on a real Wayland desktop remains to be
validated interactively — something no tool of this repository can do
in place of a human in front of a screen: `run-stokesforge` drives the application
in X11 (Xwayland), not in native Wayland, and does not open the `rfd` picker.
