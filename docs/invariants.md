# Invariants

The rules we do not break. Each one is here because it crosses several
crates — so no module comment can enforce it alone.

The column that matters is the last one: **what holds it**. A rule held by the
compiler or by a test needs no one to think about it; a rule held by review
gets lost.

| # | Rule | What holds it |
|---|---|---|
| 1 | No model mutation outside a command, for the clients of a session | ownership: the actor holds the `&mut Project` of the session (construction, tests and the CLI go through the primitives of `core`); review for a mutating `pub` method added to `core` (no test) |
| 2 | No UI dependency under `stokesforge-session`, nor in the application state (`stokesforge-app-state`, #263) | `tests/no_ui_dependency.rs`, one allowlist per crate |
| 3 | `stokesforge-plot` knows no renderer | its only dependency is `stokesforge-id` |
| 4 | A single source of text measurement | `stokesforge-render/tests/measurement.rs` + the agreement test |
| 5 | A plot feature does not enter a single backend | `stokesforge-render` is generic over `DrawingBackend` |
| 6 | *Feathering* does not enter the cache key | `stokesforge-render-egui/tests/feathering.rs` |
| 7 | Identifiers never reused | the allocator lives outside the tree, in `Project` |
| 8 | Strict LIFO on the history stack | the structure of `History`, checked by `tests/history.rs` |
| 9 | No Python object crosses the actor channel | PyO3's `Ungil` marker, at compile time |
| 10 | A data defect degrades, it does not crash | first-class states, not errors |
| 11 | A targeted recalculation gives exactly the full recalculation | the fingerprint contains every piece of data of the calculation; `b22` (twin oracle) and `stokesforge-session/tests/recalc_targeted.rs` |
| 12 | A `.stf` file written by V1 can be read back up to V3, and an older version refuses a file of a newer format (number) | `version::decode_body` (one arm per version), the witness files (`stokesforge-persistence/tests/witness_files.rs`), `UnsupportedVersion` |

---

## 1. No model mutation outside a command

`stokesforge-core::Project` is the only public facade for mutating the model,
and all its methods take `&mut self`. The actor of `stokesforge-session` holds
the `&mut Project` **of the session**: a client has only a frozen `Arc<Tree>`.

So the rule is not a convention to watch but a property of ownership. To
mutate, one must go through `SessionHandle::execute(Command)`, and therefore
through the history stack — and so be undoable.

Corollary: adding a mutating `pub` method to `stokesforge-core` without a
matching command in `stokesforge-commands` opens a non-undoable write path. No
test checks this today (`b21` measures the cost and identity on the write path,
not this corollary): review is what holds it.

**To read**: `stokesforge-core/src/project.rs` (head), `stokesforge-session/src/lib.rs`
(head).

## 2. No UI dependency under `stokesforge-session`

Rather than guess what "a UI dependency" is, each crate of the foundation
carries a `tests/no_ui_dependency.rs` that **lists the allowed dependencies**
and reads its own `Cargo.toml`. Every addition must be added to the list
explicitly: the decision becomes conscious instead of slipping in through a
`use`.

Crates guarded this way: `stokesforge-core`, `stokesforge-plot`, `stokesforge-commands`,
`stokesforge-persistence`, `stokesforge-session`, `stokesforge-script`, `stokesforge-import`,
`stokesforge-importer`, `stokesforge-clipboard`, `stokesforge-analysis` and
`stokesforge-app-state` (#263: the application state, its texts and its errors
put into sentences; neither `egui`, nor `eframe`, nor `winit`, nor `stokesforge-clipboard`
which pulls it in — the allowlist refuses them, and the compiler replaces the
text guard of #255). The
reading of the manifest is **shared** (`stokesforge-arch-guard`): normal,
build, conditional (`[target.…]`), optional and renamed dependencies
(`package = …`, judged on their real name); `dev-dependencies` are out of
scope (test tooling). Its negative witnesses live in this crate.

`stokesforge-render-svg` carries the same test
(`tests/no_ui_dependency.rs`): it must see neither `egui`, nor `eframe`, nor `winit` —
this is what makes **rendering testable without a graphical server**, and so in
continuous integration.

## 3. `stokesforge-plot` knows no renderer

A `PlotSpec` says *what to plot*, never *how*. Its only dependency is
`stokesforge-id`: no `plotters`, no `egui`, no `typst`, no `serde`
(persistence has its own transport types and does the conversion itself).

Concrete consequence, and this is the reason for the rule: the vocabulary of
the spec is its own. No `'o'`, no `linestyle='--'`, no
`PenStyle` — named enumerations, whose translation table lives in the
renderer. The equivalent layer of the Python version survived a complete change
of renderer without a line moving.

**To read**: `stokesforge-plot/src/lib.rs` and `src/style.rs` (heads).

## 4. A single source of text measurement

This is the **only place in the work package where a bug is silent**, and this
is what justifies the weight we put on it.

Everything that answers "how much room does this string take?" goes through
`stokesforge_render::text`, whichever backend asks the question. What is shared
is the `epaint::FontDefinitions` — not the `epaint::Fonts` instance, which is
neither `Clone` nor usable through `&self` anyway. The danger is not "two
instances", it is "two sets of fonts".

The precise trap to know: `plotters` is taken **without its fonts**
(`default-features = false`). A `DrawingBackend` that does not override
`draw_text` **panics** at the first tick; one that does not override
`estimate_text_size` measures the text by its **number of UTF-8 bytes** — "λ"
counts as 2, "111" measures like "WWW" — and signals nothing. The margins
reserved for labels would be wrong, so the plot area would differ from the one
on screen: the exported figure would not be the figure seen.

Two known blind spots, both handled: Plotters' `configure_series_labels` and
`MultiLineText` measure the text **themselves**, through
`FontData::estimate_layout`, hence through the naive heuristic — never through
`estimate_text_size`. This is the precise reason why the legend is
written by hand in `stokesforge_render::legend`.

Two engines measure, each for one role, never for the same text:
`epaint` (through `stokesforge_render::text`) for the ticks, Typst
(`stokesforge_render::formula`) for the titles, axis titles and the legend (issue
#32). The role is decided **once**, by the pipeline, in the text style
(`formula::rich_style`); both backends switch on `formula::is_rich`
in `estimate_text_size` **and** in `draw_text` — a measurement by one engine and
a drawing by the other would be the same silent defect. The only case where a
rich text is measured by `epaint` is its fallback when it does not compile
(invariant 10), and the fallback then applies to the measurement as well as to
the drawing.

Three tests hold the rule, and the third is the one without which the other two
would prove nothing:

- `stokesforge-render/tests/measurement.rs`: two measurers measure identically; the
  measurement does not depend on `pixels_per_point`; and it is **different**
  from the naive heuristic — the witness, without which two equally wrong
  paths would pass;
- `stokesforge-render-egui/tests/agreement.rs`: the same figure, screen against
  SVG, all primitives compared to the whole pixel, plus two families of
  witnesses;
- `stokesforge-render-egui/tests/shapes.rs`: the anchoring conventions specific
  to each backend, which the agreement test cannot compare directly.

**To read**: `stokesforge-render/src/text.rs` (head), then
`stokesforge-render-svg/src/backend.rs` (head) for the trap seen from the
export side.

## 5. A plot feature does not enter a single backend

The Plotters pipeline is written **once**, generic over
`DB: DrawingBackend` (`stokesforge_render::pipeline`). The two backends now
differ only in how they put pixels down.

A test can note a disagreement; it cannot prevent one from arising. If each
backend called its own `ChartBuilder`, the slightest divergence in margin,
font size or call order would become a disagreement to find afterwards.

A single exception lives today: `stokesforge-render-egui::feathering`. It
is not a breach — `TessellationOptions` is a setting of the egui stack,
downstream of the collection of `Shape`s, and an SVG triangulates nothing.
There is literally nothing to put on the other side.

## 6. *Feathering* does not enter the cache key

Cutting the anti-aliasing fringe changes the **tessellation**, not the `Shape`s —
and tessellation happens at every frame anyway. Adding a term to the key
`(revision, rect, pixels_per_point, view_generation)` would rebuild the whole
figure at every button press, that is, pay dearly at exactly the moment we
wanted to lighten (the lever is worth 78% of the tessellation cost, measured).

Compare with `view_generation`, which **must** be in it: a zoom changes the
plotted geometry, not only its finish.

Locked by `stokesforge-render-egui/tests/feathering.rs`.

## 7. Identifiers never reused

A `NodeId` is a monotonic counter, never reassigned over the whole life of a
project, **reload included**. The allocator lives in `Project`, **outside the
tree**, precisely so that an undo that restores an old tree
(`replace_tree`) does not move it back — otherwise two nodes could
carry the same identifier.

At the `u64::MAX` boundary, `IdAllocator::next` **refuses** (`checked_add`)
rather than overflowing or moving back. And `stokesforge_persistence::load` refuses from
the read any identifier beyond `MAX_NODE_ID` (10¹²): very far from
`u64::MAX`, but early enough that a forged file cannot lead to a project
unusable in memory.

## 8. Strict LIFO on the history stack

The history is hybrid: **structure** is kept by tree snapshots, **data** by
deltas ([`commands-and-history.md`](commands-and-history.md)).
From this asymmetry follows a non-negotiable constraint: restoring a whole tree
overwrites everything written after it, so a structural entry
can be undone only once all later entries have been undone.

The stack guarantees it by construction, and **inside a macro the entries are
undone strictly in reverse order of insertion**, whatever their
natures. This is what makes the hybrid hold.

## 9. No Python object crosses the actor channel

Three rules, two of which are held by the compiler:

1. the interface thread never touches Python;
2. the script thread releases the GIL while it waits for the actor — each
   call of the `pyst` surface goes through `Python::detach`, and PyO3's `Ungil`
   marker **forbids at compile time** that a GIL-bound value enters the
   waiting block or comes out of it;
3. the actor never takes the GIL — `stokesforge-session` does not depend on
   `stokesforge-python`, the type of a Python object could not even be named there.

The hidden path that these three rules close: a request or a response that
carried a Python object would be **destroyed on the actor thread**; and
destroying a Python object decrements its reference count, which is a
Python operation. Hence the chosen wording — no Python object crosses
the channel, neither on the way there, nor on the way back, nor as a field.

Without these rules, the deadlock is structural: the script would hold the GIL
while waiting for the actor, while the actor would wait for the GIL.

## 10. A data defect degrades, it does not crash

The same policy, applied everywhere a piece of data can be missing, unknown
or invalid. It is worth stating once because it comes back in
four similar forms:

| Situation | What happens |
|---|---|
| a series references a deleted column | `ResolvedSeries` still exists, `broken()` true, plotted in grey and annotated in the legend |
| an unknown enumeration variant in a `.stf` | captured as is in a `RawVariant`; the loss is bounded to this node (or this series), not to the file — **including under a sheet or a workbook**, which `accepts_child` accepts for this precise reason |
| a title, an axis title or a legend entry that does not compile in Typst | the raw text, `$…$` included, is displayed as is by `epaint` (the only case where a rich text is not composed by Typst); the graph does not disappear |
| a missing value | `f64::NAN` (numeric, date-time), `None` (text, distinct from `Some("")`) or `MixedCell::Empty` (mixed, distinct from `Number(NaN)` and `Text("")`) |
| a text cell in a plotted column | a mixed column plots only its numeric cells, the others being `NaN` — never a refusal to plot the column |

The common thread: **a visible and localized defect, never a panic that comes
up**, and never a silently empty series. Renaming or moving a referenced column
breaks nothing at all, by the way — a reference is a `NodeId`,
not a name.

If a panic escapes the actor thread anyway, we do not promise to
avoid it but to **make it visible** (#310): `SessionHandle::actor_alive`
is false, the recalculation "in progress" is abandoned, a backup copy is
written and a blocking dialog announces it — never an indicator that spins
forever.

Corollary on the UI side: `AppState::dispatch` (`stokesforge-app-state`) **never** returns an error; every
error becomes a displayed status message.

## 11. A targeted recalculation gives exactly the full recalculation

Since #193, the actor rewrites a formula column only if a piece of data of its
calculation has changed; it knows this by comparing a **fingerprint** (the
stored formula — text fragments and column **identifiers**, #321 —, the version
of each column read or its absence from the sheet, length of the result,
own version, outcome) with that of its last calculation. What the rule protects is the only
possible failure of such an optimization, and it is silent: a
piece of data read by the calculation but absent from the fingerprint leaves a wrong value
displayed, without any error.

Two consequences that cross the crates:

- **every piece of data the calculation reads enters the fingerprint**
  (`stokesforge-session/src/recalc.rs`) — a formula function, a
  cross-sheet reference or an analysis that reads a new piece of data adds it
  there. The reference of a correction is read on a **frozen copy** of the
  hidden workbook of copies (#248), never on the original column: the copy
  is deduplicated by **content**, never by version number, which depends on the
  history and would distort the `b22` oracle. A formula reads a column **of
  its sheet**: membership in the sheet (`None` in the fingerprint: the
  column was deleted or moved elsewhere) is read by the calculation, so it is in
  the fingerprint; the **displayed letters**, which the calculation does not read, are not
  in it (they follow the columns);
- **every data write to a column draws a fresh version number**
  (`Project::next_column_version`, `stokesforge-core`), **and every insertion
  of a column that already carries a number moves the counter beyond it**
  (`Project::insert_node`): the comparison relies on "a number never designates
  two contents of the same column". A primitive of `core` that
  wrote without this, or an insertion that left the counter below
  (found in review: `AddColumn` of a column coming from a file or from a
  snapshot from before a load), would make a stale formula pass for
  up to date.

The full recalculation has not disappeared: it is the oracle (`FormulaRecalc::Full`),
against which `b22` compares the targeted one after each step of a randomly
drawn sequence. What to test when extending the mechanism:
[`tests.md`](tests.md), "Targeted recalculation (#193) — and what to test when relying on it".

**To read**: `stokesforge-session/src/recalc.rs` (head).

## 12. A V1 file can be read back up to V3

Two things cannot be fixed after a version is released: **what it writes** and
**what it does with a newer file**. The second point is held by
`UnsupportedVersion` (no degraded reading, message "update StokesForge"); the
first by the witness files: the file of the version is read back giving the
expected project, and saving gives back its bytes. Removing, renaming or
changing the meaning of a field **raises the version number** and adds a
migrating arm to `version::decode_body`; adding a `#[serde(default)]` field
stays additive and regenerates the witness. Full rule:
[`persistence.md`](persistence.md), "Version rule: every V1 file can be read back up to V3".

**To read**: `stokesforge-persistence/src/version.rs`,
`stokesforge-persistence/tests/witness_files.rs`.
