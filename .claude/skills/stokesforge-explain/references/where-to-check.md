# Where to check, and the plain-language map

Read from `stokesforge-explain` before answering. Pages route; the code decides.

## By question family

| The question is about | Page | Code |
|---|---|---|
| grid, typing, selection, copy/paste, context menus, shortcuts, windows, theme, console | `docs/application/README.md` (table by need) | `stokesforge-app-state/src/state.rs`, `stokesforge-app/src/app/` |
| mouse gestures, double-clicks, drags | `docs/gestures.md` § The model | the surface module in `src/app/` |
| column types, missing values, mixed columns, header fields | `docs/data-model.md` | `stokesforge-core` |
| undo, macros, what Ctrl+Z brings back | `docs/commands-and-history.md` § A command is a value | `stokesforge-commands/src/command.rs` |
| saving, file format, compatibility | `docs/persistence.md` § Version rule: every V1 file can be read back up to V3 | `stokesforge-persistence` |
| plotting, axes, legend, SVG/PDF/PNG export, formulas | `docs/rendering.md` | `stokesforge-render` |
| CSV/TXT/ASC import, preview | `docs/application/project-and-import.md` § Import wizard | `stokesforge-import`, `stokesforge-importer` |
| menus, right-clicks, what is greyed or absent | `docs/menus.md` § Tree by surface | `stokesforge-app/src/app/menus.rs` |
| analysis (baseline, fit, peaks) | `docs/application/analysis.md` | `stokesforge-analysis`, `stokesforge-app-state/src/analysis.rs` |
| Python scripts, `pyst` | `docs/python-and-scripting.md` § The `pyst` surface | `stokesforge-script`, `stokesforge-python` |
| interface words, translation | `docs/ui-texts.md` § 2. Glossary | catalogues `crates/stokesforge-app-state/i18n/{en,fr}/` |
| the split into crates and its reasons | `docs/architecture.md` § What each crate holds | `cargo metadata --no-deps --format-version 1 --offline` |
| what must never break | `docs/invariants.md` | the « Ce qui la tient » column of its table |
| licence (plain GPL-3.0-or-later; extra permission only on written request, #348) | `docs/license-and-extensions.md` § Additional permission: withdrawn, can be requested | — |
| the *why* of a precise decision | the module's `//!` header | — |
| an option ruled out and its reason | `docs/archive/` (not maintained, written before the PlotYx → StokesForge renaming: `CLAUDE.md` § Context) | — |

A « oui » is checked by an `rg` in the crate or by the test that exercises it; a « non » too.

## Four behaviours that look like gaps and are not

- Greek letter, subscript, superscript in a title, axis label or series name: Typst syntax between dollars — `$lambda$`, `$x_2$`, `$x^2$`, `$"cm"^(-1)$`; `\$` for a real dollar. The « ? » icon beside these fields recalls it. A column name reused in the legend is shown as is.
- Ctrl+Z while typing in a cell undoes the last project change, not the last keystroke — spreadsheet behaviour, assumed.
- A series whose column was deleted stays, grey, annotated in the legend: missing data degrades the display, never removes the graph (invariant 10).
- Error bars: the first column with the Y Error (X Error) role to the right of a Y column, before the next Y, is paired with it at plot time (`compose::errors_for`); the series fields « Y error » / « X error » change or remove it; the bars are drawn on every plot type (histogram excepted), `pyst.plot` pairs the same way (`y_error=None` goes without).
- A cap (name length, cell count, number of bins…) is an arbitrated, documented limit (`docs/archive/rustrewrite/questions-ouvertes.md` §11), not a failure.

## Plain-language map (to build an explanation)

**The central principle**: StokesForge is split into packages (*crates*) stacked from the most abstract to the most concrete; each floor knows only the floors below. A package that describes data does not know a window exists — so the interface changes without touching the data, and data is tested without opening a window. Exact graph: `docs/architecture.md` § The graph, from bottom to top.

**Three ideas that explain almost everything**:

1. **One writer.** One component (« l'acteur ») holds the project; every other one has a frozen photo it reads at will. To change anything, it sends a request. Python analogy: one thread owns the object, the others get a read copy — where it breaks: the photo is not copied each time, it is shared and never modified.
2. **Every change is an undoable command.** Typing a value, renaming a sheet, adding a graph: each is a « commande » that also knows how to undo itself — why Ctrl+Z works the same everywhere. Exceptions: view state (zoom, window positions), loading a project, formula values recomputed by the actor as a consequence of a command (targeted: only formulas whose inputs changed are recomputed, like an `lru_cache` keyed on the version numbers of the columns read).
3. **Drawing is written once.** A curve on screen and the same curve exported to PDF go through the **same** code; only the last step (« poser un pixel » vs « écrire une balise SVG ») differs — what you export is what you see.

**The invariants** (`docs/invariants.md`) are the rules no change may break. What matters is the « what holds it » of each: a rule held by the compiler or a test needs no thought; a rule held only by review gets lost.
