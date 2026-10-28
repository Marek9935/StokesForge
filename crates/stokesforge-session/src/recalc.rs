//! Recalculation of formula columns, replayed by the actor after every request
//! that may have changed the project (see `actor::run`) — outside the history.
//!
//! # Recalculating only what a source changed (#193)
//!
//! Until #193, every successful `Execute`/`Undo`/`Redo`/`AbortMacro`
//! recalculated **all** formula columns of all sheets, on all their rows —
//! changing a curve's color included. The [`Recalculator`] now keeps, for each
//! formula column, a [`Fingerprint`] of what it saw at its last calculation,
//! and rewrites a column only if that fingerprint changed.
//!
//! ## Why a comparison, and not "A notifies B"
//!
//! A real subscription (A keeps the list of the formulas that read it) would
//! require maintaining these lists through every undo, move, copy, deletion
//! and load — so many paths where an oversight leaves a wrong value
//! **silently**. The comparison has nothing to maintain: after each command it
//! rereads the current state and checks it against the memo, which costs only
//! comparisons of integers and short texts. No column data is read for a clean
//! column.
//!
//! ## What the comparison rests on
//!
//! On the column version counter ([`stokesforge_core::Column::
//! version`]) and its doctrine: **a number never names two contents of the
//! same column** (A7, `guide-de-correction-robustesse.md` §F2). Every data
//! write, ordinary, undo or recalculation, gets a fresh number from the
//! project's single counter (`Project::next_column_version`); a column
//! inserted with a number (`AddColumn`, paste) pushes that counter past it
//! (`Project::insert_node`); a tree restoration (`Entry::Structural`) brings
//! back only a (version, content) pair that really coexisted. Equal version ⇒
//! equal content: this is the assumption the whole module depends on,
//! rechecked in the actor by battery `b22` and on the history by `b14`. The
//! converse is not required: the same content under a fresh number costs only
//! a useless recalculation.
//!
//! ## What the fingerprint must contain — the rule not to break
//!
//! **Any data that enters the calculation of a formula column enters its
//! fingerprint**: the stored formula (text fragments and the **identifiers**
//! of the columns read, #321: never letters), for each one the version of its
//! content — or `None` when it is no longer in the sheet, because membership
//! of the sheet is read by the calculation —, the length of the result (#194:
//! the longest of the columns read, failing that the longest of the sheet's
//! ordinary columns), the version of the column itself (a direct input into a
//! formula column must be overwritten), and the outcome (unreadable formula,
//! caught in a cycle, calculated). The **displayed** text (the letters) is not
//! read by the calculation: it does not enter the fingerprint, and moving a
//! column rewrites nothing. A calculation that read one more piece of data —
//! a column's name, its unit, another sheet, a `row()` function — without
//! adding it here would be wrongly "clean": the displayed value would stay
//! wrong without an error. What to test then: `docs/tests.md`,
//! "Targeted recalculation".
//!
//! ## The oracle
//!
//! The full recalculation has not disappeared: it is [`FormulaRecalc::Full`],
//! the same loop whose memo is never consulted. Battery `b22` plays random
//! sequences of commands in two sessions, one targeted, the other full, and
//! requires equal trees after each step. The two modes share all the code
//! except the "clean / to recalculate" decision: that decision, and only it, is
//! what the oracle exercises.

//! # Analysis calculations (#181)
//!
//! The same mechanism tracks the **analysis calculations**
//! (`Tree::calculations`), replayed after the formulas, outside the history
//! (`Project::refresh_calculation_from` rewrites the output columns, the
//! results and the staleness). What changes:
//!
//! - the **fingerprint** of a calculation is `CalcFingerprint` — the version of
//!   each column it reads, and its *definition* (operation, flags, output
//!   columns with their version). The rule of the previous paragraph applies
//!   here too: any data read by `stokesforge_core::compute` enters the
//!   fingerprint;
//! - an **automatic** calculation recalculates when the fingerprint changes
//!   (always in `Full` mode); a **failure** keeps the last values and sets
//!   `Staleness::Failed`, with no new attempt as long as the fingerprint does
//!   not move; a vanished input sets `Staleness::InputMissing`;
//! - a **manual** calculation (reconvolution, deconvolution, batches of more
//!   than 20 series) never reruns by itself: an input change marks it `Manual`.
//!   It compares the **content** of its inputs with that of the last
//!   calculation (`CalcMemo::seen`) rather than their versions — the full mode
//!   rewrites the formula columns on every pass, which would change the
//!   versions without changing the content; a change of *definition*
//!   (`UpdateCalculation`, undo) is a user gesture: the result is up to date by
//!   construction (`Trust`). An **automatic** calculation that the file made
//!   `Manual` obeys the same content comparison as long as its inputs do not
//!   really change, in both modes (without it, the full mode erased it and
//!   diverged from the targeted one — latent before #321, found by `b22`);
//! - on load the memo is cleared and **no calculation is rerun** (A1, #316):
//!   automatic and manual ones alike are trusted, the file's values kept
//!   (`Recalculator::run_after_load`, one-shot flag `trust_loaded`, in both
//!   modes). Their staleness marker comes from the file, completed by
//!   persistence: it marks `Manual` a calculation whose fingerprint of the
//!   inputs' content (`Calculation::inputs_digest`) no longer matches. The
//!   formula columns, on the other hand, are still recalculated on load;
//! - formulas and calculations feed each other: `run` alternates the two until
//!   a calculation writes nothing more (at most [`MAX_ROUNDS`] rounds). A loop
//!   that crosses a calculation (its output feeds a formula that feeds its
//!   input) would never stabilize: its calculations are marked `Manual` and
//!   left as they are (`calculations_in_a_cycle`).
//!
//! Tests: `tests/analysis.rs` (named cases), battery `b22` (step
//! `AjouterCalcul`, `RetirerCalcul`, `RecalculerPerimes`), mutants in
//! `docs/tests.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use stokesforge_analysis::ops::Operation;
use stokesforge_analysis::AnalysisError;
use stokesforge_core::{
    compute, evaluate_formula, topological_order, AuxRef, Calculation, ColumnFormula, ColumnType,
    ColumnValues, ComputeError, Formula, FormulaError, NodeId, NumericStatus, Project, SeriesRef,
    Staleness, ValueBlock,
};

/// How the actor recalculates formula columns after every request that may
/// have changed the project (see [`crate::Session::spawn_with_recalc`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormulaRecalc {
    /// Rewrites a formula column only if a piece of data of its calculation
    /// changed since the last one (see the module-level docs). The mode of
    /// every production session.
    #[default]
    Targeted,
    /// Rewrites all formula columns of all sheets, every time — the behavior
    /// before #193. Serves as the **oracle** of the differential test
    /// (`stokesforge-robustness`, battery `b22`): no production client uses
    /// it.
    Full,
}

/// The recalculation memo: what the actor saw at the last calculation of each
/// formula column. Lives in the actor, beside the `Project` — neither in the
/// history, nor in the `.stf`.
pub(crate) struct Recalculator {
    mode: FormulaRecalc,
    memo: HashMap<NodeId, Fingerprint>,
    /// Counterpart of the memo for analysis calculations (#181): see the
    /// "Analysis calculations" section of the module-level docs.
    calc_memo: HashMap<NodeId, CalcMemo>,
    /// Stop requested from outside (#239), read between two columns during
    /// [`Recalculator::run_interruptible`]; `None` the rest of the time.
    stop: Option<Box<dyn Fn() -> bool + Send>>,
    /// True as soon as `stop` has answered yes during the current pass.
    interrupted: bool,
    /// **One-shot** flag armed by `Load` (A1, #316): during the following
    /// pass, a calculation without a memo is trusted as is
    /// (`CalcAction::Trust`), even an automatic one. Disarmed at the end of
    /// that pass: a calculation without a memo after `AddCalculation` or an
    /// undo is still recalculated.
    trust_loaded: bool,
}

/// Maximum number of "formulas → calculations" rounds of a pass: a chain
/// formula → calculation → formula → calculation stabilizes in as many rounds
/// as there are links; a cycle (the output of a calculation that feeds its own
/// input through a formula) never stabilizes and stops here, on the last
/// calculated state.
const MAX_ROUNDS: usize = 8;

impl Recalculator {
    pub(crate) fn new(mode: FormulaRecalc) -> Self {
        Self {
            mode,
            memo: HashMap::new(),
            calc_memo: HashMap::new(),
            stop: None,
            interrupted: false,
            trust_loaded: false,
        }
    }

    /// Like [`Recalculator::run`], but stops between two columns (or two
    /// calculations) as soon as `stop` answers yes (#239). Returns `true` if
    /// the pass ran to completion. An interrupted pass leaves a consistent
    /// state: each column is written as a block and noted in the memo
    /// afterwards, those that were not reached are simply seen as stale by the
    /// next pass.
    pub(crate) fn run_interruptible(
        &mut self,
        project: &mut Project,
        stop: impl Fn() -> bool + Send + 'static,
    ) -> bool {
        self.stop = Some(Box::new(stop));
        self.interrupted = false;
        self.run(project);
        self.stop = None;
        !std::mem::take(&mut self.interrupted)
    }

    /// Stop point: true if the pass must stop (sticky).
    fn should_stop(&mut self) -> bool {
        if !self.interrupted {
            self.interrupted = self.stop.as_ref().is_some_and(|stop| stop());
        }
        self.interrupted
    }

    /// Forgets everything: the next [`Recalculator::run`] recalculates every
    /// formula column. Called when the project is replaced (`Load`,
    /// `NewProject`): a `.stf` carries formula values that are not taken on
    /// trust (see `actor::run`, `Load` branch), and its column versions have
    /// nothing to do with those of the previous project.
    ///
    /// Clears only the memo: the **formula columns** are recalculated by the
    /// next pass, but the **analysis calculations** (A1, #316) no longer are
    /// on open — see [`Recalculator::run_after_load`].
    pub(crate) fn forget(&mut self) {
        self.memo.clear();
        self.calc_memo.clear();
    }

    /// The pass that follows a `Load` (A1, #316): the formula columns are
    /// recalculated as always, but **no analysis calculation** is — its values
    /// are the file's, recalculation on demand ("Recalculate"). A calculation
    /// whose inputs changed outside StokesForge was already marked stale by
    /// persistence (fingerprint of the inputs' content,
    /// `stokesforge_persistence`), without a calculation either. The flag is
    /// single-use: reset to `false` before returning.
    pub(crate) fn run_after_load(&mut self, project: &mut Project) {
        self.trust_loaded = true;
        self.run(project);
        self.trust_loaded = false;
    }

    /// Recalculates the project's formula columns that need it — **direct**
    /// access to `Project` (`Project::set_length`/`Project::set_values`),
    /// never via `History::apply`/`Command::execute`: it is a silent side
    /// effect replayed afterwards, not an undoable mutation (see the
    /// crate-level docs, `docs/commands-and-history.md`) — exactly the same
    /// status as `actor::purge_orphaned_geometries`. The formula text
    /// (`Command::SetColumnMeta`) and the source cells (`Command::SetCells`,
    /// `InsertRows`, `DeleteRows`, ...) stay normally undoable: only the
    /// recalculation never pushes its own entry.
    ///
    /// # Scope of the trigger
    ///
    /// Called after **every** successful `Execute`/`Undo`/`Redo`/`AbortMacro`
    /// and after every `Load` (see `actor::run`, for each call site and its
    /// own justification).
    ///
    /// The whole tree is **inspected**, not only "the sheet the command just
    /// touched": `Undo`/`Redo`/`AbortMacro` carry no command to inspect (only
    /// `History` knows what it just restored, and it does not expose it), and
    /// `Command::Move` can move a column from one sheet to another: the
    /// formulas of the old sheet that read it no longer find it (`#REF`).
    /// Inspecting is not recalculating: a column whose fingerprint did not move
    /// is neither read nor rewritten. The cost of a pass with nothing to
    /// recalculate is linear in the number of nodes, plus the parsing of each
    /// formula text — never the size of the data.
    pub(crate) fn run(&mut self, project: &mut Project) {
        self.run_formulas(project);
        self.run_calculation_rounds(project, None);
    }

    /// Reruns analysis calculations, whatever their state ("Recalculate"
    /// action). Empty `ids`: all those that carry a staleness marker
    /// ("Recalculate Out of Date"). Outside the history, like
    /// [`Recalculator::run`], whose propagation it reuses.
    pub(crate) fn recalculate(&mut self, project: &mut Project, ids: &[NodeId]) {
        let forced: HashSet<NodeId> = if ids.is_empty() {
            project
                .tree()
                .calculations()
                .iter()
                .filter(|(_, calc)| calc.stale.is_some())
                .map(|(id, _)| *id)
                .collect()
        } else {
            ids.iter().copied().collect()
        };
        self.run_formulas(project);
        self.run_calculation_rounds(project, Some(forced));
    }

    fn run_formulas(&mut self, project: &mut Project) {
        let sheets: Vec<NodeId> = project
            .tree()
            .iter()
            .filter(|(_, node)| node.kind().is_worksheet())
            .map(|(id, _)| id)
            .collect();
        let mut seen = HashSet::new();
        for sheet in sheets {
            if self.should_stop() {
                // Interrupted pass: `seen` is incomplete, do not prune.
                return;
            }
            self.recalc_sheet(project, sheet, &mut seen);
        }
        // A deleted column, or one that lost its formula, leaves the memo:
        // hygiene, so that the memo stays bounded by the formula columns
        // present. This is not what makes the memo correct — an identifier is
        // never reassigned within a project (invariant 7), and `forget` covers
        // the replaced project; a stale entry could therefore never be taken
        // for another column.
        self.memo.retain(|id, _| seen.contains(id));
    }

    /// True if `column` must be (re)written: full mode, column absent from the
    /// memo, or different fingerprint.
    fn is_stale(&self, column: NodeId, fingerprint: &Fingerprint) -> bool {
        match self.mode {
            FormulaRecalc::Full => true,
            FormulaRecalc::Targeted => self.memo.get(&column) != Some(fingerprint),
        }
    }

    /// Notes what was just written into `column`: the fingerprint receives the
    /// version **after** writing, the one the next pass must find again if
    /// nobody else touched it.
    fn record(&mut self, project: &Project, column: NodeId, mut fingerprint: Fingerprint) {
        if self.mode == FormulaRecalc::Full {
            return;
        }
        match column_version(project, column) {
            Some(version) => {
                fingerprint.own_version = version;
                self.memo.insert(column, fingerprint);
            }
            None => {
                self.memo.remove(&column);
            }
        }
    }

    /// Recalculates the formula columns of a single sheet that need it, in the
    /// order of their dependencies (topological sort,
    /// [`stokesforge_core::topological_order`]) — a formula column that
    /// references another reads its **already recalculated** value, never the
    /// one from before this pass. A dependency cycle (direct or transitive) is
    /// detected without looping: each column that takes part gets an entirely
    /// `NaN` block rather than a value that would make no sense to choose
    /// anyway.
    ///
    /// Propagation is automatic: recalculating B writes B, so changes its
    /// version, and C = B + 1, which comes after B in the order, sees its
    /// fingerprint change in the same pass. The order of the steps below
    /// (unreadable, then types, then cycles, then the topological order) means
    /// that no column is written after a column that reads it took its
    /// fingerprint: an immediate second pass rewrites nothing.
    fn recalc_sheet(&mut self, project: &mut Project, sheet: NodeId, seen: &mut HashSet<NodeId>) {
        let Ok(columns) = project.tree().children(sheet).map(<[NodeId]>::to_vec) else {
            return;
        };
        // Length of a result (#194): that of the longest of the columns the
        // formula reads (calculated below, column by column). Failing that —
        // unreadable formula, caught in a cycle, or without any resolved
        // reference — that of the longest **ordinary** column of the sheet.
        // Never a length that counts the formula columns themselves: before
        // #194, `Tree::row_count` counted them, and a formula column once
        // lengthened sustained itself at that length (undoing the lengthening
        // of a source no longer shortened it). Ordinary columns are never
        // written by the pass: this length is fixed during the whole pass.
        let ordinary_len = columns
            .iter()
            .filter_map(|&id| project.tree().node(id).and_then(|node| node.as_column()))
            .filter(|column| column.meta().formula.is_none())
            .map(stokesforge_core::Column::len)
            .max()
            .unwrap_or(0);

        // Formulas parsed in advance: a *parse* error is already a final
        // result (`NaN` block), it takes no part in the dependency graph — a
        // column that referenced it finds it immediately available (as `NaN`),
        // not "pending".
        let mut parsed: BTreeMap<NodeId, (ColumnFormula, Formula)> = BTreeMap::new();
        for &id in &columns {
            let Some(source) = project
                .tree()
                .node(id)
                .and_then(|node| node.as_column())
                .and_then(|column| column.meta().formula.clone())
            else {
                continue;
            };
            seen.insert(id);
            match source.compile() {
                Ok(formula) => {
                    parsed.insert(id, (source, formula));
                }
                Err(_error) => {
                    let fingerprint =
                        Fingerprint::new(project, id, source, Outcome::Unparsable, ordinary_len);
                    if self.is_stale(id, &fingerprint) {
                        write_formula_column(
                            project,
                            id,
                            vec![f64::NAN; ordinary_len],
                            &vec![NumericStatus::Missing; ordinary_len],
                        );
                        self.record(project, id, fingerprint);
                    }
                }
            }
        }

        // Output type of a formula column: see
        // `desired_formula_column_type`. Recalculated on every pass rather than
        // fixed at creation: a formula may reference a column that appears or
        // disappears later. Applied directly to the model rather than through
        // an undoable command, like `write_formula_column` below: it is the
        // same "outside the history" derivation as the recalculation of the
        // values, not a user input that is undoable on its own. No shortcut
        // through the memo here: comparing the types reads no data, and a type
        // change changes the column's version, hence its fingerprint — it will
        // be rewritten below.
        for (&id, (_, formula)) in &parsed {
            if let Some(desired) = desired_formula_column_type(formula, &columns, project) {
                let current = project
                    .tree()
                    .node(id)
                    .and_then(|node| node.as_column())
                    .map(stokesforge_core::Column::column_type);
                if current != Some(desired) {
                    let _ = project.set_column_type(id, desired);
                }
            }
        }

        // Dependency graph between formula columns only: a reference to an
        // ordinary column is not an edge to order, it is already available (see
        // the docs of `stokesforge_core::topological_order`).
        let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> =
            parsed.keys().map(|&id| (id, BTreeSet::new())).collect();
        // `parsed` holds only columns of this sheet: a reference to another
        // sheet is never an edge.
        for (&id, (_, formula)) in &parsed {
            for dep_id in formula.referenced_columns() {
                if parsed.contains_key(&dep_id) {
                    if let Some(deps) = edges.get_mut(&id) {
                        deps.insert(dep_id);
                    }
                }
            }
        }
        let (order, blocked) = topological_order(&edges);

        for id in blocked {
            let Some((source, _)) = parsed.get(&id) else {
                continue;
            };
            // A `NaN` block depends only on the length and the type of the
            // column, already covered by its own version (the type is fixed
            // just above): the sources do not enter the fingerprint. Putting
            // their versions there would make the members of a cycle rewrite
            // each other on every pass, each changing the other's version.
            // Nor their length: in a cycle, it would be defined by itself; it
            // is that of the ordinary columns.
            let fingerprint =
                Fingerprint::new(project, id, source.clone(), Outcome::Blocked, ordinary_len);
            if self.is_stale(id, &fingerprint) {
                write_formula_column(
                    project,
                    id,
                    vec![f64::NAN; ordinary_len],
                    &vec![NumericStatus::Missing; ordinary_len],
                );
                self.record(project, id, fingerprint);
            }
        }
        for id in order {
            if self.should_stop() {
                return;
            }
            let Some((source, formula)) = parsed.get(&id) else {
                continue;
            };
            // Read here, not before the loop: the formula columns already
            // processed in this loop (those `id` depends on, necessarily
            // earlier in `order`) may have been rewritten, hence changed
            // version — this is what propagates the recalculation along a
            // chain. Each identifier is resolved among the columns of the
            // **sheet** only (never in the whole tree): `None` means "outside
            // the sheet", and enters the fingerprint.
            let sources: Vec<(NodeId, Option<u64>)> = formula
                .referenced_columns()
                .into_iter()
                .map(|column_id| {
                    let version = columns
                        .contains(&column_id)
                        .then(|| column_version(project, column_id))
                        .flatten();
                    (column_id, version)
                })
                .collect();
            // The longest of the columns read, also read after the formula
            // columns `id` depends on; their length follows their version,
            // already in the fingerprint.
            let target_len = sources
                .iter()
                .filter(|(_, version)| version.is_some())
                .filter_map(|(column_id, _)| column_len(project, *column_id))
                .max()
                .unwrap_or(ordinary_len);
            let fingerprint = Fingerprint::new(
                project,
                id,
                source.clone(),
                Outcome::Evaluated { sources },
                target_len,
            );
            if !self.is_stale(id, &fingerprint) {
                continue;
            }
            let values: Vec<_> = columns
                .iter()
                .map(|&column_id| {
                    project
                        .tree()
                        .node(column_id)
                        .and_then(|node| node.as_column())
                        .map(|column| (column_id, column.values().clone()))
                })
                .collect();
            // A column of `columns` vanished between reading `columns` and
            // now: cannot happen (this function never touches the tree's
            // structure), a defensive guard rather than a panic if the
            // assumption were ever wrong (invariant 10). Nothing is written,
            // so nothing is noted: the memo forgets it, and the next pass will
            // retry.
            if values.iter().any(Option::is_none) {
                self.memo.remove(&id);
                continue;
            }
            let values: Vec<_> = values.into_iter().flatten().collect();
            let evaluation = evaluate_formula(formula, &values, target_len);
            // A column read that is no longer in the sheet (`#REF`) leaves
            // **empty** cells, not `---`: no calculation failed, the formula
            // has nothing left to read. Empty statuses = no row marked
            // (`write_formula_column`).
            let statuses: &[NumericStatus] = match evaluation.error {
                Some(FormulaError::DeletedColumn) => &[],
                _ => &evaluation.statuses,
            };
            write_formula_column(project, id, evaluation.values, statuses);
            self.record(project, id, fingerprint);
        }
    }
}

/// What the actor saw at the last calculation of a formula column: **all** the
/// data on which what is written there depends (see the module-level docs,
/// "What the fingerprint must contain"). Two equal fingerprints guarantee that
/// recalculating would write exactly the same content.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    /// The stored formula: text fragments and identifiers of the columns read
    /// (#321). Never the displayed letters, which the calculation does not
    /// read.
    source: ColumnFormula,
    /// The outcome of the calculation, and the sources it read.
    outcome: Outcome,
    /// Length of the result: the longest of the columns read, or failing that
    /// the longest ordinary column of the sheet (#194). For a calculated
    /// formula, it follows from the versions of the sources; for the other
    /// outcomes, it is the only trace of the sheet in the fingerprint.
    target_len: usize,
    /// Version of the formula column itself. Compared with the current version
    /// before writing, replaced by the after-write version in the memo
    /// ([`Recalculator::record`]): a direct input into the column, the undo of
    /// such an input or a type change makes it change, and the column is
    /// rewritten.
    own_version: u64,
}

impl Fingerprint {
    fn new(
        project: &Project,
        column: NodeId,
        source: ColumnFormula,
        outcome: Outcome,
        target_len: usize,
    ) -> Self {
        Self {
            source,
            outcome,
            target_len,
            own_version: column_version(project, column).unwrap_or(0),
        }
    }
}

/// The outcome of a formula calculation, with what it read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// The formula does not parse: `NaN` everywhere, nothing read.
    Unparsable,
    /// Caught in a cycle, or dependent on a cycle: `NaN` everywhere, nothing
    /// read. The output type, which depends on the columns named, is fixed
    /// before writing and changing type changes the column's version.
    Blocked,
    /// Calculated. Each column read, with the version of its content — or
    /// `None`: it is not in the sheet (deleted, or in another sheet), the
    /// calculation degrades (`FormulaError::DeletedColumn`, empty cells).
    /// Membership of the sheet is read by the calculation: it therefore enters
    /// the fingerprint, and a column that comes back (undo) triggers a
    /// recalculation.
    Evaluated { sources: Vec<(NodeId, Option<u64>)> },
}

/// Current length of `column`, `None` if it is not (or no longer) a column of
/// the tree.
fn column_len(project: &Project, column: NodeId) -> Option<usize> {
    project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(stokesforge_core::Column::len)
}

/// Current version of `column`, `None` if it is not (or no longer) a column of
/// the tree.
fn column_version(project: &Project, column: NodeId) -> Option<u64> {
    project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(stokesforge_core::Column::version)
}

/// Type that the result column of `formula` should carry, from the columns it
/// references, or `None` when it cannot be decided (reference outside the
/// sheet, or to a column that vanished) — in that case, the caller leaves the
/// column's type unchanged rather than choosing at random.
///
/// `Mixed` as soon as the formula references at least one column — not only if
/// one of them is already `Mixed`. Any column, even a purely `Numeric` one, can
/// have an empty cell on some row; yet a `Numeric` column can store only
/// numbers, never the text `"---"` that [`write_formula_column`] now writes for
/// [`NumericStatus::Failed`] as well as for [`NumericStatus::Missing`] (missing
/// cell, not only failure) — otherwise a missing operand would stay a mute
/// cell rather than a result marked as not calculated. Only a formula without
/// any reference (a pure constant, which can never be missing nor fail) stays
/// `Numeric`.
fn desired_formula_column_type(
    formula: &Formula,
    columns: &[NodeId],
    project: &Project,
) -> Option<ColumnType> {
    let mut any_reference = false;
    for column_id in formula.referenced_columns() {
        if !columns.contains(&column_id) {
            return None;
        }
        project
            .tree()
            .node(column_id)
            .and_then(|node| node.as_column())?;
        any_reference = true;
    }
    Some(if any_reference {
        ColumnType::Mixed
    } else {
        ColumnType::Numeric
    })
}

/// Writes `values`/`statuses` into `column`, first truncating/extending it to
/// its length (`Project::set_length`) since `Project::set_values` never
/// overwrites a column tail longer than the written block. Silently ignores a
/// failure (`column` vanished meanwhile, which nothing in this function
/// causes) rather than panicking — invariant 10.
///
/// For a **mixed** formula column, a row with status [`NumericStatus::Failed`]
/// **or** [`NumericStatus::Missing`] is written as `MixedCell::Text("---")`
/// rather than `MixedCell::Empty`: a result not calculated, whether because a
/// referenced column holds non-numeric text (`Failed`) or because an operand
/// is simply missing (`Missing`), is always flagged rather than leaving a mute
/// cell that would be mistaken for a value not yet entered. `statuses` must
/// have the same length as `values`; otherwise the values alone govern (no row
/// is marked), as before this parameter was added.
fn write_formula_column(
    project: &mut Project,
    column: NodeId,
    values: Vec<f64>,
    statuses: &[NumericStatus],
) {
    // The block must match the column's type, otherwise `set_values` refuses
    // and the recalculation would be silently lost. A formula column without
    // reference (pure constant) stays numeric, but as soon as it references a
    // column, `desired_formula_column_type` has already retyped it as mixed:
    // `ValueBlock::into_mixed` then turns the `NaN`s into empty cells, which is
    // what the formula meant (invariant 10 — degrade, do not swallow); a
    // `Failed` or `Missing` row is written there as `"---"` rather than an
    // empty cell, see the docs above.
    let is_mixed = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .is_some_and(|target| target.column_type() == ColumnType::Mixed);
    let length = values.len();
    let block = if is_mixed {
        let mut cells = ValueBlock::Float(values).into_mixed();
        if statuses.len() == cells.len() {
            for (cell, status) in cells.iter_mut().zip(statuses) {
                if matches!(status, NumericStatus::Failed | NumericStatus::Missing) {
                    *cell = stokesforge_core::MixedCell::Text("---".to_string());
                }
            }
        }
        ValueBlock::Mixed(cells)
    } else {
        ValueBlock::Float(values)
    };
    if project.set_length(column, length).is_err() {
        return;
    }
    let _ = project.set_values(column, 0, block);
}

// ── Analysis calculations (#181) ────────────────────────────────────────

/// What the recalculation saw of an analysis calculation at its last pass: the
/// versions of the columns it **reads**, and what **defines** it (operation,
/// output columns and their version).
///
/// The separation serves the manual calculations: an input change makes them
/// stale; a change of definition (`UpdateCalculation`, undo) is a user gesture
/// whose result is up to date by construction.
#[derive(Debug, Clone, PartialEq)]
struct CalcFingerprint {
    inputs: Vec<(NodeId, u64)>,
    definition: CalcDefinition,
}

#[derive(Debug, Clone, PartialEq)]
struct CalcDefinition {
    operation: Operation,
    /// The structure of the inputs, not only the flat list of their columns:
    /// `{x: None, y: A, σ: B}` and `{x: A, y: B, σ: None}` read the same
    /// columns in different roles.
    series: Vec<SeriesRef>,
    aux: Option<AuxRef>,
    auto_recalc: bool,
    keep_results: bool,
    outputs: Vec<(NodeId, Option<u64>)>,
}

/// The fingerprint of `calc`, or the first input column that no longer exists.
fn calc_fingerprint(project: &Project, calc: &Calculation) -> Result<CalcFingerprint, NodeId> {
    let mut inputs = Vec::new();
    for column in calc.input_columns() {
        match column_version(project, column) {
            Some(version) => inputs.push((column, version)),
            None => return Err(column),
        }
    }
    Ok(CalcFingerprint {
        inputs,
        definition: CalcDefinition {
            operation: calc.operation.clone(),
            series: calc.series.clone(),
            aux: calc.aux.clone(),
            auto_recalc: calc.auto_recalc,
            keep_results: calc.keep_results,
            outputs: calc
                .outputs
                .iter()
                .filter_map(|o| o.column)
                .map(|c| (c, column_version(project, c)))
                .collect(),
        },
    })
}

/// The memo of a calculation: its fingerprint and, for a **manual** calculation
/// only, the content of its inputs at its last calculation (a cloned `Arc`, not
/// a copy as long as the column is not modified). Without it, a column rewritten
/// identically — which the full recalculation of the formulas does on every
/// pass — would make stale a calculation that nothing changed.
struct CalcMemo {
    fingerprint: CalcFingerprint,
    seen: Vec<ColumnValues>,
}

/// Is the content of each input the one that was seen?
fn same_contents(project: &Project, inputs: &[(NodeId, u64)], seen: &[ColumnValues]) -> bool {
    inputs.len() == seen.len()
        && inputs.iter().zip(seen).all(|((column, _), before)| {
            project
                .tree()
                .node(*column)
                .and_then(|n| n.as_column())
                .is_some_and(|c| c.values() == before)
        })
}

fn snapshot_inputs(project: &Project, inputs: &[(NodeId, u64)]) -> Vec<ColumnValues> {
    inputs
        .iter()
        .filter_map(|(column, _)| {
            project
                .tree()
                .node(*column)
                .and_then(|n| n.as_column())
                .map(|c| c.values().clone())
        })
        .collect()
}

enum CalcAction {
    Skip,
    /// The versions changed, not the content: the new ones are kept.
    Renumber,
    /// Trusts the data present (manual calculation never seen, or redefined):
    /// notes the fingerprint, calculates nothing.
    Trust,
    /// An input changed under a manual calculation.
    MarkStale,
    Recompute,
}

impl Recalculator {
    /// Walks the calculations in the order of their dependencies (the output of
    /// one can be the input of another); if one rewrote data, goes over the
    /// formulas again — which can read these outputs — then starts over, within
    /// the limit of [`MAX_ROUNDS`]. With no calculation in the project, costs
    /// nothing.
    fn run_calculation_rounds(&mut self, project: &mut Project, forced: Option<HashSet<NodeId>>) {
        if project.tree().calculations().is_empty() {
            self.calc_memo.clear();
            return;
        }
        let mut forced = forced;
        for _ in 0..MAX_ROUNDS {
            if self.should_stop() {
                return;
            }
            let changed = self.run_calculations(project, forced.take().as_ref());
            if !changed {
                break;
            }
            self.run_formulas(project);
        }
        let present = project.tree().calculations();
        self.calc_memo.retain(|id, _| present.contains_key(id));
    }

    /// One round over all calculations. Returns `true` if data changed.
    fn run_calculations(
        &mut self,
        project: &mut Project,
        forced: Option<&HashSet<NodeId>>,
    ) -> bool {
        // Who produces which column; a calculation depends on those whose output it reads.
        let mut producer: HashMap<NodeId, NodeId> = HashMap::new();
        for (&id, calc) in project.tree().calculations() {
            for column in calc.outputs.iter().filter_map(|o| o.column) {
                producer.insert(column, id);
            }
        }
        let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for (&id, calc) in project.tree().calculations() {
            let deps = calc
                .input_columns()
                .into_iter()
                .filter_map(|c| producer.get(&c).copied())
                .filter(|&p| p != id)
                .collect();
            edges.insert(id, deps);
        }
        let (order, blocked) = topological_order(&edges);
        let mut blocked: HashSet<NodeId> = blocked.into_iter().collect();
        blocked.extend(calculations_in_a_cycle(project));

        let mut changed = false;
        for id in order {
            if blocked.contains(&id) {
                continue;
            }
            if self.should_stop() {
                return changed;
            }
            changed |=
                self.update_calculation(project, id, forced.is_some_and(|f| f.contains(&id)));
        }
        // A cycle (between calculations, or calculation → formula →
        // calculation) has no calculation order, and rerunning it would never
        // stabilize: they are flagged stale without touching them.
        for id in blocked {
            let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
        }
        changed
    }

    /// Notes the fingerprint seen of `calc` (and the content of its inputs if it is manual).
    fn remember(
        &mut self,
        project: &Project,
        id: NodeId,
        calc: &Calculation,
        fingerprint: CalcFingerprint,
    ) {
        // The content of the inputs is kept for a manual calculation, and for
        // an automatic one that the file made stale (`Manual`, or
        // `InputMissing` that `Trust` brings back to `Manual`): see
        // `update_calculation`.
        let waits_for_a_gesture = !calc.auto_recalc
            || matches!(
                calc.stale,
                Some(Staleness::Manual) | Some(Staleness::InputMissing)
            );
        let seen = if waits_for_a_gesture {
            snapshot_inputs(project, &fingerprint.inputs)
        } else {
            Vec::new()
        };
        self.calc_memo.insert(id, CalcMemo { fingerprint, seen });
    }

    fn update_calculation(&mut self, project: &mut Project, id: NodeId, force: bool) -> bool {
        let Some(calc) = project.tree().calculation(id).cloned() else {
            return false;
        };
        let fingerprint = match calc_fingerprint(project, &calc) {
            Ok(fingerprint) => fingerprint,
            Err(_missing) => {
                // A column read vanished: the last values stay, the calculation
                // is stale. Finding the column again (undo) restarts from
                // zero.
                let _ = project.set_calculation_stale(id, Some(Staleness::InputMissing));
                self.calc_memo.remove(&id);
                return false;
            }
        };
        let previous = self.calc_memo.get(&id);
        let action = if force {
            CalcAction::Recompute
        } else if self.trust_loaded && previous.is_none() {
            // First pass after a `Load`, in both modes (otherwise the `Full`
            // oracle would diverge): the file's values are authoritative.
            CalcAction::Trust
        } else if calc.auto_recalc {
            match (self.mode, previous) {
                (FormulaRecalc::Targeted, Some(p)) if p.fingerprint == fingerprint => {
                    CalcAction::Skip
                }
                // An automatic calculation marked `Manual` (read from a file
                // where it was stale or failed, A1 #316) waits for a gesture:
                // as long as the **content** of its inputs is the one it saw,
                // recalculating it would move it from `Manual` to `Failed` or
                // to "up to date" without anybody asking — and the full mode,
                // which rewrites the formula columns (hence their versions) on
                // every pass, would diverge from the targeted one (battery
                // `b22`, latent before #321). A real modification of an input
                // recalculates it like any automatic one.
                (_, Some(p))
                    if calc.stale == Some(Staleness::Manual)
                        && p.fingerprint.definition == fingerprint.definition
                        && p.fingerprint.inputs == fingerprint.inputs =>
                {
                    CalcAction::Skip
                }
                (_, Some(p))
                    if calc.stale == Some(Staleness::Manual)
                        && p.fingerprint.definition == fingerprint.definition
                        && same_contents(project, &fingerprint.inputs, &p.seen) =>
                {
                    CalcAction::Renumber
                }
                _ => CalcAction::Recompute,
            }
        } else {
            // A manual calculation does not obey the mode: its inputs are
            // compared by content, in both modes (see `CalcMemo`).
            match previous {
                None => CalcAction::Trust,
                Some(p) if p.fingerprint.definition != fingerprint.definition => CalcAction::Trust,
                Some(p) if p.fingerprint.inputs == fingerprint.inputs => CalcAction::Skip,
                Some(p) if same_contents(project, &fingerprint.inputs, &p.seen) => {
                    CalcAction::Renumber
                }
                Some(_) => CalcAction::MarkStale,
            }
        };
        match action {
            CalcAction::Skip => false,
            CalcAction::Renumber => {
                if let Some(memo) = self.calc_memo.get_mut(&id) {
                    memo.fingerprint = fingerprint;
                }
                false
            }
            CalcAction::Trust => {
                if calc.stale == Some(Staleness::InputMissing) {
                    // The columns came back but nothing says the kept values
                    // correspond to them.
                    let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
                }
                self.remember(project, id, &calc, fingerprint);
                false
            }
            CalcAction::MarkStale => {
                if !matches!(calc.stale, Some(Staleness::Failed(_))) {
                    let _ = project.set_calculation_stale(id, Some(Staleness::Manual));
                }
                false
            }
            CalcAction::Recompute => {
                match compute(
                    project.tree(),
                    &calc.operation,
                    &calc.series,
                    calc.aux.as_ref(),
                ) {
                    Ok(computed) => {
                        let changed = match project.refresh_calculation_from(id, computed) {
                            Ok(changed) => changed,
                            Err(_) => {
                                // An output refuses the result (type changed,
                                // protected column): say so, instead of
                                // memorizing a fingerprint and leaving the
                                // column frozen with no sign.
                                let _ = project.set_calculation_stale(
                                    id,
                                    Some(Staleness::Failed(AnalysisError::OutputUnwritable)),
                                );
                                self.remember(project, id, &calc, fingerprint);
                                return false;
                            }
                        };
                        // The after-write fingerprint: the one the next pass
                        // must find again if nobody touches it.
                        if let Some((after, fresh)) = project
                            .tree()
                            .calculation(id)
                            .cloned()
                            .and_then(|c| Some((calc_fingerprint(project, &c).ok()?, c)))
                        {
                            self.remember(project, id, &fresh, after);
                        }
                        changed
                    }
                    Err(ComputeError::MissingInput(_)) => {
                        let _ = project.set_calculation_stale(id, Some(Staleness::InputMissing));
                        self.calc_memo.remove(&id);
                        false
                    }
                    Err(ComputeError::Analysis(error)) => {
                        // The last valid values stay; no new attempt as long
                        // as no input changes.
                        let _ = project.set_calculation_stale(id, Some(Staleness::Failed(error)));
                        self.remember(project, id, &calc, fingerprint);
                        false
                    }
                }
            }
        }
    }
}

/// The calculations caught in a loop that crosses formula columns: the output
/// of a calculation feeds a formula that feeds (step by step) an input of the
/// same calculation. A loop made of formulas alone is already handled by
/// `recalc_sheet` (`NaN` columns); here we look for those that go through a
/// calculation — the full graph minus the graph of the formulas.
fn calculations_in_a_cycle(project: &Project) -> HashSet<NodeId> {
    let tree = project.tree();
    let mut formula_edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for (sheet, node) in tree.iter() {
        if !node.kind().is_worksheet() {
            continue;
        }
        let Ok(columns) = tree.children(sheet) else {
            continue;
        };
        for &id in columns {
            let deps = formula_edges.entry(id).or_default();
            let source = tree
                .node(id)
                .and_then(|n| n.as_column())
                .and_then(|c| c.meta().formula.clone());
            if let Some(Ok(formula)) = source.as_ref().map(ColumnFormula::compile) {
                // The columns of **this sheet** only, like `recalc_sheet`.
                deps.extend(
                    formula
                        .referenced_columns()
                        .into_iter()
                        .filter(|column| columns.contains(column)),
                );
            }
        }
    }
    let mut full_edges = formula_edges.clone();
    for calc in tree.calculations().values() {
        let inputs = calc.input_columns();
        for output in calc.outputs.iter().filter_map(|o| o.column) {
            full_edges
                .entry(output)
                .or_default()
                .extend(inputs.iter().copied());
        }
    }
    let (_, blocked_full) = topological_order(&full_edges);
    let (_, blocked_formulas) = topological_order(&formula_edges);
    tree.calculations()
        .iter()
        .filter(|(_, calc)| {
            calc.outputs
                .iter()
                .filter_map(|o| o.column)
                .any(|c| blocked_full.contains(&c) && !blocked_formulas.contains(&c))
        })
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use stokesforge_core::{Column, ColumnMeta, ColumnType, ColumnValues};

    use super::*;

    /// Numeric cells of a column; `None` for what is not numeric (the "---" of
    /// a formula column not yet calculated).
    fn numbers(project: &Project, column: NodeId) -> Vec<Option<f64>> {
        let column = project
            .tree()
            .node(column)
            .and_then(|n| n.as_column())
            .expect("column");
        match column.values() {
            ColumnValues::Float(values) => values.iter().map(|v| Some(*v)).collect(),
            ColumnValues::Mixed(cells) => cells
                .iter()
                .map(|cell| match cell {
                    stokesforge_core::MixedCell::Number(v) => Some(*v),
                    _ => None,
                })
                .collect(),
        }
    }

    fn values_of(expected: &[f64]) -> Vec<Option<f64>> {
        expected.iter().map(|v| Some(*v)).collect()
    }

    /// `A [1,2,3]` then `B = A*2`, `C = B+1`, `D = C+1` (a chain: each column
    /// waits for the previous one).
    fn chain() -> (Project, [NodeId; 3]) {
        let mut project = Project::new("Demo").expect("project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("A");
        let mut formulas = Vec::new();
        for (name, text) in [("B", "A*2"), ("C", "B+1"), ("D", "C+1")] {
            let column = project
                .add_column(sheet, None, Some(name), Column::new(ColumnType::Numeric))
                .expect("column");
            let formula = project
                .tree()
                .formula_from_letters(column, text)
                .expect("formula entered");
            project
                .set_column_meta(
                    column,
                    ColumnMeta {
                        formula: Some(formula),
                        ..Default::default()
                    },
                )
                .expect("formula");
            formulas.push(column);
        }
        (project, [formulas[0], formulas[1], formulas[2]])
    }

    fn version(project: &Project, column: NodeId) -> u64 {
        column_version(project, column).expect("version")
    }

    /// #239: the stop falls between two columns. What was written stays, the
    /// rest waits; the next pass completes without rewriting what is already
    /// written (the memo stayed correct) and ends up at the complete result.
    #[test]
    fn a_stop_between_two_columns_leaves_a_state_that_the_next_pass_completes() {
        let (mut project, [c, d, e]) = chain();
        let polls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&polls);
        let mut recalc = Recalculator::new(FormulaRecalc::Targeted);
        // 1st check: before the sheet; 2nd: before C; 3rd: before D.
        let completed = recalc.run_interruptible(&mut project, move || {
            counter.fetch_add(1, Ordering::SeqCst) + 1 >= 3
        });
        assert!(!completed, "the pass should have been interrupted");
        assert_eq!(
            numbers(&project, c),
            values_of(&[2.0, 4.0, 6.0]),
            "C is written"
        );
        assert!(
            numbers(&project, d).iter().all(Option::is_none)
                && numbers(&project, e).iter().all(Option::is_none),
            "D and E were not reached"
        );
        let c_version = version(&project, c);

        recalc.run(&mut project);
        assert_eq!(numbers(&project, d), values_of(&[3.0, 5.0, 7.0]));
        assert_eq!(numbers(&project, e), values_of(&[4.0, 6.0, 8.0]));
        assert_eq!(
            version(&project, c),
            c_version,
            "C, already up to date, is not rewritten"
        );
    }

    /// Witness: without a stop request, the interruptible pass runs to the end.
    #[test]
    fn without_a_stop_the_interruptible_pass_runs_to_the_end() {
        let (mut project, [_, _, e]) = chain();
        let mut recalc = Recalculator::new(FormulaRecalc::Targeted);
        assert!(recalc.run_interruptible(&mut project, || false));
        assert_eq!(numbers(&project, e), values_of(&[4.0, 6.0, 8.0]));
    }
}
