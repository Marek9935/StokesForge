# StokesForge documentation — `main` branch

Port of StokesForge (analysis and plotting of scientific data, OriginLab style)
from Python/Qt to Rust. All the code lives in the Cargo workspace at the root.

## What this documentation is, and what it is not

It **describes the code as it is**. It plans nothing, specifies nothing
and arbitrates nothing: there is no requirements document here.

The detailed reasoning — *why* this decision rather than that one,
what was measured, what was discarded — lives in the **module head comments**
(`//!`), as close as possible to the code it justifies. These pages do not copy
it: they say **where to find it**, and carry only what fits in no module
because it crosses all of them.

The reading rule that follows: these pages guide, the code is authoritative. A
page that contradicts the code is wrong.

## Where to start

| I want to… | Read |
|---|---|
| understand the split into crates and why it is so | [`architecture.md`](architecture.md) |
| know what I am not allowed to break | [`invariants.md`](invariants.md) |
| touch the project tree, the columns, the `PlotSpec` | [`modele-de-donnees.md`](modele-de-donnees.md) |
| touch undo/redo, macros | [`commandes-et-historique.md`](commandes-et-historique.md) |
| touch the `.stf` format, `save`/`load` | [`persistance.md`](persistance.md) |
| touch the actor, concurrency, snapshots | [`session.md`](session.md) |
| touch plotting, axes, SVG/PDF/PNG export, formulas | [`rendu.md`](rendu.md) |
| touch the PyO3 bridge or the `pyst` surface of scripts | [`python-et-script.md`](python-et-script.md) |
| reproduce the off-screen measurements of the Matplotlib prototype | [`python-matplotlib-spike.md`](python-matplotlib-spike.md) |
| touch the window, shortcuts, the grid | [`application/`](application/README.md) |
| add or display an icon (color roles, contact sheet) | [`icones.md`](icones.md) |
| add or place a menu or right-click entry | [`menus.md`](menus.md) (grammar), then [`application/menus-et-barres.md`](application/menus-et-barres.md) § Themed menus (#114) |
| write or change visible text (label, tooltip, message) | [`textes-interface.md`](textes-interface.md) |
| add a field, a message, a dialog, a confirmation, a greyed-out state, a keyboard or mouse gesture | [`standard-interface.md`](standard-interface.md) (behavior, #217) |
| create a new window or dialog, or add a gesture (wheel, double-click) | [`fenetres.md`](fenetres.md) (catalog of windows), [`principes-ui.md`](principes-ui.md) (interface principles), [`gestes.md`](gestes.md) (gesture matrix) |
| write or run a test, understand the CI | [`tests.md`](tests.md) |
| measure the smoothness of the interface (frame time, running in release) | [`mesure-performance.md`](mesure-performance.md) |
| license, contributions, extension interfaces and third-party notices | [`licence-et-extensions.md`](licence-et-extensions.md) |
| know what V1 requires, or release a version | [`criteres-v1.md`](criteres-v1.md) |

`.claude/rules.md` completes this table on the **context loading** side: which
crates to open, and above all which not to open, for each domain.

An exception to the rule "these pages plan nothing":
[`chantier-colonnes-mixtes.md`](chantier-colonnes-mixtes.md) is a
**handover note**, not a reference page — what a work package found along the
way and what it leaves open, especially on the test side. It will join
`docs/archive/` when its "remaining to do" column is empty.

`docs/chantiers/` gathers the same notes for **pending** work:

| Note | Contents |
|---|---|
| [`retours-2026-09-23-features.md`](chantiers/retours-2026-09-23-features.md) | nine features to develop, with the decisions already made |
| [`prevision-import.md`](chantiers/prevision-import.md) | import preview |
| [`analyse-maquettes.md`](chantiers/analyse-maquettes.md) | mathematical processing (#181): mockups M1–M15 and choices of 2026-10-09, carried over into #181–#190 |

**Bugs do not live there**: their state is tracked in the project's GitLab
issues (`glab issue list --label bug`). The old `retours-*-bugs.md` reports
were deleted once their bugs were fixed; the code comments that cite them
resolve in the git history.

Like the other handover notes, they **plan**: the code is not yet
authoritative there, and a "remaining to do" must be checked against the code
before being taken up.

## Commands

```sh
cargo build --workspace
cargo test --workspace                        # inclut stokesforge-robustesse, sauf #[ignore]
cargo test -p <crate> <nom_test>              # un seul test
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The last two are mandatory: the CI (`.github/workflows/ci.yml`) makes them
fail at the first deviation.

## Conventions

- Code, comments and commits: commits are **in English** since #368. Code and comments are **still in French** for now and are translated crate by crate (#372–#376). Documentation in `docs/` is **in English**; `docs/archive/` stays in French (#368).
- A module head comment explains **why**, not *what*: the
  *what* can be read in the signature just below.
- Every mutation of the model goes through an undoable command of
  `stokesforge-commands`, never through a primitive of `stokesforge-core` directly.

## `docs/archive/` — the history, not the reference

`docs/archive/` keeps the earlier design documents: technical
requirements document, phased roadmaps (foundation 0-5, rendering R0-R8),
robustness reports, explored options. They produced the current code and keep
their archive value — notably because **the code comments cite them
by their file name** (`architecture-choisie.md` §4.3,
`guide-de-correction-robustesse.md` §F6, `roadmap-rendu.md`…). Every citation
of this kind resolves in `docs/archive/`.

They are not maintained. Where they diverge from the code, the code is
right.
