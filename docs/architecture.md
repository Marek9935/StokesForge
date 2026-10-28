# Architecture

Twenty crates, one direction of dependency, and one optional feature
(`python`). This page gives the map; the detail of each tier is in
its own page.

## The graph, from bottom to top

Arrow = "depends on" (`Cargo.toml` manifests, checked with
`cargo metadata --no-deps --format-version 1 --offline`).

```text
stokesforge-id            NodeId seul, zéro dépendance
   ↑
stokesforge-analysis      calcul pur (lignes de base, intégrales, ajustements, pics,
                          déconvolution) : f64 en entrée et en sortie, aucune
                          dépendance stokesforge — lu par core, commands,
                          persistence, session, script et app
   ↑
stokesforge-plot          PlotSpec : quoi tracer, sans savoir comment
   ↑
stokesforge-core          arbre de projet, colonnes, résolution d'un spec
   ↑            ↑            ↑
stokesforge-commands  stokesforge-persistence  stokesforge-render ← stokesforge-render-svg
(core, plot)     (core, plot, import)  (core, plot)     (core, plot, render)
   ↑       ↑
stokesforge-session       l'acteur : le &mut Project de la session
(commands, persistence, core)
   ↑            ↑
stokesforge-script   stokesforge-render-egui (core, plot, render, session)
(analysis, commands, core, import, importer, plot, python, render-svg, session)
+ stokesforge-python (aucune dépendance stokesforge)

stokesforge-cli    (core, commands, import, importer, persistence)
               import : un `Project` neuf construit directement, puis `.stf`
               run (python) : script, python, session ; même acteur que la console
stokesforge-import (aucune dépendance stokesforge) — lecture des fichiers d'appareil ;
               lu par persistence : le miroir de la recette dans le `.stf` (S2, #323)
stokesforge-importer (core, import) — plan de feuilles et colonnes du modèle, sans
               commandes : partagé par l'application, le CLI et les scripts
stokesforge-clipboard (aucune dépendance stokesforge) — offre multi-format au
               presse-papiers système (SVG, PNG, PDF), un backend par affichage
stokesforge-app-state (analysis, commands, core, import, importer, persistence, plot,
               render, render-svg, session ; script et python en option) — l'état de
               l'application SANS interface : AppState, sélection, préférences,
               formulaires d'analyse, textes (Fluent) et erreurs mises en phrase
stokesforge-app    (app-state, analysis, clipboard, commands, core, import, plot,
               render, render-egui, render-svg, session ; python en option) — la
               coquille eframe : peint l'état, traduit clavier et souris en actions
```

`stokesforge-robustesse` stands apart: it depends on the foundation and the Python bridge, and
contains no production code ([`tests.md`](tests.md)). `stokesforge-arch-guard`
is a test tool: the shared reading of manifests for the dependency
guards.

## What each crate holds

| Crate | Role | Page |
|---|---|---|
| `stokesforge-id` | `NodeId`, and nothing else | [model](modele-de-donnees.md) |
| `stokesforge-analysis` | pure mathematical processing (#181): baselines, correction by reference, integrals, fits, peaks, deconvolution; `f64` where `NaN` means "empty", results with stable keys; transcendental functions through `libm`, same bits on all platforms (#332); no UI, no Python, no other workspace crate | [model](modele-de-donnees.md) |
| `stokesforge-plot` | `PlotSpec`: the description of a graph, neutral toward the renderer | [model](modele-de-donnees.md) |
| `stokesforge-core` | `Project` / `Tree` / `Node` / `Column`, plus `resolve` (spec → data) and `compose` (columns → series of a graph, #257: X/Y pairing, palette by rank, layers; reading of the `PlotSpec`; no sentence, no mutation) | [model](modele-de-donnees.md) |
| `stokesforge-commands` | the commands, the hybrid undo stack, the macros | [commands](commandes-et-historique.md) |
| `stokesforge-persistence` | `save`/`load`, transport schema, `.stf` format | [persistence](persistance.md) |
| `stokesforge-session` | the actor, the `Snapshot`s, the `revision` | [session](session.md) |
| `stokesforge-import` | reading of instrument files (CSV/TXT/ASC), recipes, preview; independent of core/UI | [application](application/projet-et-import.md) |
| `stokesforge-render` | text measurement, Plotters pipeline, **all the rendering** | [rendering](rendu.md) |
| `stokesforge-render-egui` | screen backend (`DrawingBackend` → `egui::Shape`), shape cache, reading of egui inputs (wheel, drag), feathering; the computation of zoom/pan and of the nearest point is in `stokesforge-render` (#262) | [rendering](rendu.md) |
| `stokesforge-render-svg` | SVG export, and PDF and PNG on top of it | [rendering](rendu.md) |
| `stokesforge-python` | embedded CPython interpreter, numpy round trip | [python](python-et-script.md) |
| `stokesforge-script` | the script thread, the `pyst` surface, the interruption | [python](python-et-script.md) |
| `stokesforge-app-state` | the application state **without egui** (#263): `AppState` and its `Action`s, selection, preferences, analysis forms, Object Manager, interface text (`tr!`, Fluent catalogs `en`/`fr`) and errors turned into sentences; testable without a window (`tests/shell.rs`). No `egui`, no `eframe`, no `winit`, no `wgpu`: the compiler holds the boundary | [application](application/etat-et-console.md) |
| `stokesforge-app` | the eframe UI shell (test bench): windows, grid, menus, theme, icons, copy of the figure to the clipboard; paints the state of `stokesforge-app-state`. Together with it, the **only** crates that translate the interface (Fluent: `i18n-embed`, `i18n-embed-fl`, `rust-embed`, `fluent-syntax`) | [application](application/coquille-et-code.md) |
| `stokesforge-cli` | import and Python scripts without an interface | [application](application/etat-et-console.md) |
| `stokesforge-importer` | from the result of `stokesforge-import` to the columns of the model (sheet plan, column construction) — no commands, no interface, shared by the application, the CLI and the scripts | [application](application/projet-et-import.md) |
| `stokesforge-clipboard` | offers the same content to the system clipboard under several MIME types (the figure of a graph: SVG, PNG, PDF); Wayland, X11, Windows, macOS; independent of the rest of the workspace | [application](application/graphe.md) |
| `stokesforge-robustesse` | 22 test batteries at the limits, 2 out-of-process probes | [tests](tests.md) |
| `stokesforge-arch-guard` | shared reading of the `Cargo.toml` files for the dependency guards (dev-dependency) | [tests](tests.md) |

## The three separation lines

The split is not decorative; each of these three boundaries is
**checked by a test** and not by review.

1. **No UI dependency below `stokesforge-session`, nor in the application
   state.** `stokesforge-analysis`, `stokesforge-core`,
   `stokesforge-plot`, `stokesforge-commands`, `stokesforge-persistence`, `stokesforge-session`,
   `stokesforge-script`, `stokesforge-import`, `stokesforge-importer`, `stokesforge-clipboard` and
   `stokesforge-app-state` (#263: the application state, without egui; the allowlist
   there refuses `winit` and `stokesforge-clipboard`, which pulls it in) each carry a
   `tests/no_ui_dependency.rs`: an allowlist of dependencies, which forces
   a conscious decision for every new edge instead of relying on
   review. The reading of the manifest is shared (`stokesforge-arch-guard`):
   normal, build, conditional (`[target.…]`) and renamed dependencies
   (`package = …`, judged by their real name) count; `dev-dependencies`
   do not. `stokesforge-render` has its own `tests/no_ui_dependency.rs`, which relies on the
   same reading.
2. **The `PlotSpec` knows no renderer.** `stokesforge-plot` depends
   only on `stokesforge-id`: no `plotters`, no `egui`, no `typst`, not even
   `serde`. This is why changing the renderer does not touch the
   description of graphs.
3. **A plotting feature never goes into a single backend.** It
   goes into `stokesforge-render`, generic over `DrawingBackend`. The two backends
   differ only in how they put pixels down. The only living exception
   is `stokesforge-render-egui::feathering`, and it is justified where
   it lives: tessellation exists only for the screen.

The detail of these rules and of what holds them: [`invariants.md`](invariants.md).

## A Cargo cycle avoided, and the trace it left

`stokesforge-id` exists for a single reason, and it explains a visible oddity
of the graph. Making `NodeKind::Graph` carry a `stokesforge_plot::PlotSpec`
creates the edge `stokesforge-core → stokesforge-plot`; `stokesforge-plot` already depended on
the whole of `stokesforge-core`, for the `NodeId` type alone. The two edges together
are a cycle that Cargo refuses (*cyclic package dependency* — checked, not
assumed).

`NodeId` was therefore moved down into a minimal crate with no dependency, below
both. `stokesforge-core` re-exports it unchanged (`pub use stokesforge_id::NodeId`):
no caller of `stokesforge_core::NodeId` had to change. The full reasoning
is at the top of `crates/stokesforge-id/src/lib.rs`.

## Features

Only one feature is outside `default`, `python`: the workspace builds and
tests entirely without it.

| Feature | Crates | What it adds | What it requires |
|---|---|---|---|
| `python` | `stokesforge-python`, `stokesforge-script`, `stokesforge-app-state`, `stokesforge-app`, `stokesforge-cli`, `stokesforge-robustesse` | PyO3, embedded interpreter, script thread, script console | `PYO3_CONFIG_FILE` (see `scripts/python-config.sh`) and the vendored interpreter in `vendor/python/` |

Without the `python` feature, the crates concerned compile empty — this is the
condition for the right to drop Python without taking anything apart. Practical
consequence: `cargo build --workspace` **does not compile** this path; to
exercise it, `scripts/python-test.sh` sets the environment variables
(reference commands: [`.codex/README.md`](../.codex/README.md)).

**Typst is not a feature.** Since issue #29, `typst`, `typst-layout`,
`typst-render` and `typst-svg` (all `=0.15.1`) are hard dependencies of
`stokesforge-render`, `stokesforge-render-egui` and `stokesforge-render-svg`, always
compiled: `cargo build` and `cargo test --workspace` cover them. What
remains optional is the **execution path** of a backend (formulas through Typst
or in plain text), detailed in [`rendu.md`](rendu.md#typst-r8).

## Where the binaries are

| Binary | Crate | What it is for |
|---|---|---|
| `stokesforge-app` | `stokesforge-app` | the window — `stokesforge-app [chemin]` |
| `stokesforge-cli` | `stokesforge-cli` | `import`: instruments → `.stf`, new project without a session; `recipes`; `run` (python): script through a session without a window |
| `stokesforge-selftest` | `stokesforge-script` | replay the end criterion of the foundation from a packaged distribution |
| `stokesforge-pyhello` | `stokesforge-python` | prove the isolation of the packaged Python stack |
| `stokesforge-sonde-alloc`, `stokesforge-sonde-pile` | `stokesforge-robustesse` | two out-of-process probes (allocation failure, stack overflow) |

The `scripts/*.sh` at the root assemble and rerun the distributions
(`dist/`) in a clean environment.

**Release (#333).** `scripts/release-linux.sh` packages `dist/stokesforge-app`
(already proven by `app-dist.sh`) as `StokesForge-<version>-linux-x86_64.tar.gz`;
`scripts/release-notes.sh` checks that a `vX.Y.Z` tag matches the versions of the
crates (alignment by `scripts/versions-alignees.sh`, also run on every push by `fmt`: crates, internal dependencies, `Cargo.lock`, format witness; refusal of a tag outside `main` or not increasing by `scripts/release-tag-sur-main.sh` and `scripts/release-tag-croissant.sh`) and a section of `CHANGELOG.md`, and prints the notes;
`scripts/release-televerser.sh linux|windows` uploads the archive or the installer to
the GitLab package registry; `scripts/release-publier.sh` creates the GitLab
release, which links the two. Linux and Windows are built on GitHub
(hosted runners), and everything is published on GitLab (the release is created by the GitHub `publier` job): [`criteres-v1.md`](criteres-v1.md).

**Windows (#332).**
`scripts/app-dist-windows.sh` (Git Bash) is the counterpart of `app-dist.sh`: it
assembles a staging folder, proves it, then delivers it as an **Inno
Setup installer** (`scripts/stokesforge.iss`, `ISCC.exe` required; per user without
administrator rights, "Open with" association of `.stf` files, full
uninstall). No code signing for now: Windows shows "unknown
publisher". The Visual C++ DLLs are shipped next to the executable;
tkinter/Tcl/Tk, the tests and pip are removed from the embedded Python. Notices per
platform (`vendor/python/licenses-linux`, `licenses-windows`). The GitHub CI jobs
`dist-windows` and `test-python-windows` run automatically on `main` and
on a `vX.Y.Z` tag. In release, `stokesforge-app` is of the
windowed subsystem. The scripts
`python-vendor.sh`, `python-config.sh` (template
`pyo3-config.windows.template.txt`) and `python-test.sh` detect Windows.
The embedded prefix there has the python-build-standalone layout
(`lib/python/{Lib,DLLs}`, `python312.dll` next to the executable), handled
by `EmbeddedPython` (`cfg(windows)`). Left to lift before V1:
cross-check the Windows notices against the full python-build-standalone
Windows archive (`licenses-windows/README.md`). The distribution is proven on a
GitHub hosted runner (`dist-windows` and `test-python-windows`; tools:
Git Bash, Rust MSVC, cargo-deny, cargo-about, Inno Setup).

## Vocabulary

- **Node** — an entry of the project tree: folder, workbook, sheet,
  column, graph, matrix, note.
- **Window** — a node that can be displayed in a subwindow: workbook,
  graph, matrix, note. `Tree::windows()` lists them; their names are unique
  across the whole project, not only within the parent.
- **Revision** — the monotonic counter of `stokesforge-session`, incremented on every
  accepted mutation. A client knows its snapshot is stale by comparing
  the revision, never the trees.
- **Spec** — a `stokesforge_plot::PlotSpec`: what to plot.
- **Snapshot** — a frozen, immutable view of the project, returned
  by the actor: an `Arc<Tree>` plus the revision and the history state.
- **Macro** — a group of commands that is undone with a single `Ctrl+Z`.
- **Phases** — the comments cite work package markers: phases 0-5
  for the foundation, **R0-R8** for rendering (R0 measurement, R3 anchoring of the spec in
  the model, R4 the two backends, R5 the widening, R6 PDF, R7 interaction,
  R8 Typst). All closed; the corresponding plans are in
  `docs/archive/`.
