//! Tests of column formulas through `pyst` (#412): remote references, quoted
//! names, column functions, removal of a formula, column type, undo of a
//! script group, duplication, and a differential check of the numeric
//! functions against Python's `math` / `statistics`.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, NodeId, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

fn bench() -> Bench {
    let mut project = Project::new("Formulas").unwrap();
    let root = project.root();
    let book1 = project.add_empty_workbook(root, Some("Book1")).unwrap();
    let sheet1 = project.add_worksheet(book1, Some("Sheet1")).unwrap();
    let sheet3 = project.add_worksheet(book1, Some("Sheet3")).unwrap();
    let blank = project.add_worksheet(book1, Some("Blanc 2")).unwrap();
    let book2 = project.add_empty_workbook(root, Some("Book2")).unwrap();
    let sheet5 = project.add_worksheet(book2, Some("Sheet5")).unwrap();
    let mut add = |sheet: NodeId, name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .unwrap();
    };
    add(sheet1, "A", &[1.0, 2.0, 3.0, 4.0]);
    add(sheet1, "B", &[0.5, 1.0, 2.0, 4.0]);
    add(sheet3, "A", &[10.0, 20.0, 30.0, 40.0]);
    add(sheet3, "B", &[1.0, 2.0, 3.0, 4.0]);
    add(blank, "A", &[7.0, 8.0, 9.0, 10.0]);
    add(blank, "B", &[0.25, 0.5, 0.75, 1.0]);
    add(sheet5, "A", &[100.0, 200.0, 300.0, 400.0]);
    add(sheet5, "B", &[5.0, 6.0, 7.0, 8.0]);
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());
    Bench {
        engine,
        session,
        _actor: actor,
    }
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

const PRELUDE: &str = "
S1 = pyst.resolve('[Book1]Sheet1')
def formula_column(text, sheet=None, name=None):
    col = pyst.create_column(sheet if sheet is not None else S1, name)
    pyst.set_column_meta(col, formula=text)
    return col
";

fn script(bench: &Bench, body: &str) -> ScriptOutcome {
    run(bench, &format!("{PRELUDE}\n{body}"))
}

#[test]
fn remote_quoted_and_column_function_formulas_read_back_in_letters() {
    let bench = bench();
    script(
        &bench,
        r#"
cases = [
    ('max(Sheet3!B)', 'max(Sheet3!B)', 4.0),
    ('sum([Book2]Sheet5!B)', 'sum([Book2]Sheet5!B)', 26.0),
    ("mean('Blanc 2'!A)", "mean('Blanc 2'!A)", 8.5),
    ('A + max(Sheet3!A)', 'A + max(Sheet3!A)', None),
    ('at(Sheet3!B, 2)', 'at(Sheet3!B, 2)', None),
    ('max(B, 1, 3)', 'max(B, 1, 3)', None),
    ('sum(A) - mean(A)', 'sum(A) - mean(A)', 7.5),
]
for text, expected_text, expected in cases:
    col = formula_column(text)
    got = pyst.info(col)['column']['formula']
    vals = pyst.values(col)
    assert got == expected_text, (text, got)
    if expected is not None:
        assert all(v == expected for v in vals), (text, vals)
"#,
    );
}

#[test]
fn removing_a_formula_through_pyst_keeps_the_values() {
    // The interface's "remove formula" drops the values (docs); `pyst` keeps
    // the calculated values in the column as plain data. Pinned here.
    let bench = bench();
    script(
        &bench,
        r#"
col = formula_column('A * 2', name='Twice')
assert pyst.values(col) == (2., 4., 6., 8.)
pyst.set_column_meta(col, formula=None)
assert pyst.info(col)['column']['formula'] in (None, '')
assert pyst.values(col) == (2., 4., 6., 8.), pyst.values(col)
# The column is writable again.
pyst.set_values(col, [1., 1.])
assert pyst.values(col) == (1., 1.)
"#,
    );
}

#[test]
fn a_reader_follows_a_formula_that_is_removed() {
    let bench = bench();
    script(
        &bench,
        r#"
first = formula_column('A * 2')   # column C
second = formula_column('C + 1')    # column D
assert pyst.values(second) == (3., 5., 7., 9.)
pyst.set_column_meta(first, formula=None)
pyst.set_values(first, [10., 20.])
assert pyst.values(second) == (11., 21.), pyst.values(second)
"#,
    );
}

#[test]
fn set_column_type_on_a_formula_column_is_refused() {
    let bench = bench();
    script(
        &bench,
        r#"
col = formula_column('A * 2', name='Twice')
"#,
    );
    let before = bench.session.snapshot().unwrap();
    script(
        &bench,
        r#"
col = pyst.children(S1)[-1]
for kind in ('mixed', 'datetime', 'numeric'):
    try:
        pyst.set_column_type(col, kind)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('formula column converted to ' + kind)
assert pyst.info(col)['column']['formula'] == 'A * 2'
assert pyst.values(col) == (2., 4., 6., 8.)
"#,
    );
    // The refusals left no history entry and no change.
    let after = bench.session.snapshot().unwrap();
    assert_eq!(after.history.undo_label, before.history.undo_label);
    assert_eq!(*after.tree, *before.tree);
}

#[test]
fn a_script_that_sets_formulas_in_a_loop_is_undone_in_one_step() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    script(
        &bench,
        r#"
cols = []
for i in range(8):
    cols.append(formula_column('A * %d' % (i + 1), name='F%d' % i))
pyst.set_column_meta(cols[0], formula='A + max(Sheet3!A)')
dep = formula_column('J + C', name='Dep')   # columns C..J are the loop's
assert pyst.values(dep) == (49., 58., 67., 76.), pyst.values(dep)
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert!(after.tree.iter().count() > before.tree.iter().count());
    bench.session.undo().unwrap();
    let undone = bench.session.snapshot().unwrap();
    assert_eq!(undone.tree.iter().count(), before.tree.iter().count());
    assert_eq!(*undone.tree, *before.tree);
    // Redo brings the formulas and their values back in one step.
    bench.session.redo().unwrap();
    let redone = bench.session.snapshot().unwrap();
    assert_eq!(redone.tree.iter().count(), after.tree.iter().count());
    assert_eq!(*redone.tree, *after.tree);
}

/// Design question (#412): a lone formula column duplicated in its own sheet
/// reads `#REF` for the local columns it read (`Project::insert_copies`
/// reserves an id with no node for any reference outside the copy), while a
/// remote reference to a sheet that was not copied keeps reading the original.
/// Pinned as it is today.
#[test]
fn duplicating_a_lone_formula_column_voids_its_local_references() {
    let bench = bench();
    script(
        &bench,
        r#"
col = formula_column('A + max(Sheet3!B)', name='Mix')
copy = pyst.duplicate(col)
assert pyst.values(col) == (5., 6., 7., 8.)
assert pyst.info(copy)['column']['formula'] == '#REF + max(Sheet3!B)', pyst.info(copy)['column']['formula']
assert all(v is None for v in pyst.values(copy)), pyst.values(copy)
# A purely remote formula keeps reading the original sheet.
far = formula_column('max(Sheet3!B)', name='Far')
far_copy = pyst.duplicate(far)
assert pyst.info(far_copy)['column']['formula'] == 'max(Sheet3!B)'
assert pyst.values(far_copy) == pyst.values(far)
"#,
    );
}

#[test]
fn duplicating_a_sheet_makes_its_formulas_read_the_copy() {
    let bench = bench();
    script(
        &bench,
        r#"
col = formula_column('A * 2', name='Twice')
other = formula_column('max(Sheet3!A) + A', name='Far')
sheet_copy = pyst.duplicate(S1, name='Copy')
cols = {pyst.info(c)['name']: c for c in pyst.children(sheet_copy)}
a_copy = cols['A']
pyst.set_values(a_copy, [100., 200., 300., 400.])
assert pyst.values(cols['Twice']) == (200., 400., 600., 800.), pyst.values(cols['Twice'])
assert pyst.values(col) == (2., 4., 6., 8.)
assert pyst.values(cols['Far']) == (140., 240., 340., 440.), pyst.values(cols['Far'])
assert pyst.info(cols['Far'])['column']['formula'] == 'max(Sheet3!A) + A'
"#,
    );
}

/// Differential check of the numeric functions against Python's own `math`
/// and `statistics`, on seeded random data. The bound is on ULP distance.
/// What is known not to hold yet is in
/// `erf_erfc_and_the_aggregates_meet_their_target_tolerance` (#429).
#[test]
fn numeric_functions_agree_with_python_math_within_a_few_ulps() {
    let bench = bench();
    script(
        &bench,
        r#"
import math, random, statistics, struct

def ordered(x):
    bits = struct.unpack('<q', struct.pack('<d', x))[0]
    return -(bits & 0x7fffffffffffffff) if bits < 0 else bits

def ulps(a, b):
    if math.isnan(a) and math.isnan(b):
        return 0
    if math.isnan(a) or math.isnan(b):
        return 10**9
    if a == b:
        return 0
    return abs(ordered(a) - ordered(b))

def add_column(name, data):
    col = pyst.create_column(S1, name)
    pyst.set_values(col, data)
    return col

def letter(name):
    names = [pyst.info(c)['name'] for c in pyst.children(S1)]
    return chr(ord('A') + names.index(name))

random.seed(412)
data = [random.uniform(-3.0, 3.0) for _ in range(500)]
data += [1e-300, -1e-300, 0.0, 1e10, 123456.789]
add_column('Rand', data)
x = letter('Rand')

rowwise = {
    'exp(%s)': lambda v: math.exp(v) if v < 700 else math.inf,
    'sin(%s)': math.sin,
    'cos(%s)': math.cos,
    'tan(%s)': math.tan,
    'atan(%s)': math.atan,
    'sinh(%s)': lambda v: math.sinh(v) if abs(v) < 700 else math.copysign(math.inf, v),
    'cosh(%s)': lambda v: math.cosh(v) if abs(v) < 700 else math.inf,
    'tanh(%s)': math.tanh,
    'abs(%s)': abs,
    '%s ^ 2': lambda v: v * v,
}
# erf and erfc: documented absolute error < 1e-12 (functions.rs). Their
# relative error is the subject of the ignored test (#429).
for text, fn in (('erf(%s)', math.erf), ('erfc(%s)', math.erfc)):
    got = pyst.values(formula_column(text % x))
    worst = max(abs(g - fn(v)) for g, v in zip(got, data))
    assert worst < 1e-12, (text, worst)
worst = {}
for text, fn in rowwise.items():
    got = pyst.values(formula_column(text % x))
    worst[text] = max(ulps(g, fn(v)) for g, v in zip(got, data))
# Aggregates: naive summation may differ from fsum by a few hundred ulps.
agg = {
    'sum(%s)': math.fsum(data),
    'mean(%s)': math.fsum(data) / len(data),
    'std(%s)': statistics.stdev(data),
    'max(%s)': max(data),
    'min(%s)': min(data),
}
for text, expected in agg.items():
    got = pyst.values(formula_column(text % x))
    assert len(got) >= 1
    worst[text] = max(ulps(g, expected) for g in got)
loose = {'sum(%s)': 2000, 'mean(%s)': 2000, 'std(%s)': 2000}
for text, distance in worst.items():
    assert distance <= loose.get(text, 2), (text, distance, sorted(worst.items(), key=lambda kv: -kv[1]))
"#,
    );
}

/// Row-wise functions not covered above, against Python's `math`, with one
/// tolerance per function. Tolerances hold on any libm (Windows included): 4
/// ulps for the transcendental ones, exact for what IEEE makes exact.
/// Left out on purpose: `at` on a grid point (#427), `logx` of powers and
/// `erf` (#429).
#[test]
fn more_row_wise_functions_agree_with_python_math() {
    let bench = bench();
    script(
        &bench,
        r#"
import math, random, statistics, struct

def ordered(x):
    bits = struct.unpack('<q', struct.pack('<d', x))[0]
    return -(bits & 0x7fffffffffffffff) if bits < 0 else bits

def ulps(a, b):
    if math.isnan(a) and math.isnan(b):
        return 0
    if math.isnan(a) or math.isnan(b):
        return 10**9
    if a == b:
        return 0
    return abs(ordered(a) - ordered(b))

def add_column(name, data):
    col = pyst.create_column(S1, name)
    pyst.set_values(col, data)
    return col

def letter(name):
    names = [pyst.info(c)['name'] for c in pyst.children(S1)]
    return chr(ord('A') + names.index(name))

random.seed(7)
pos = [random.uniform(1e-3, 50.0) for _ in range(300)] + [0.5, 2.0, 1e-300, 1e10]
unit = [random.uniform(-0.999, 0.999) for _ in range(300)] + [0.0, 0.5, -0.5]
anyx = [random.uniform(-50.0, 50.0) for _ in range(300)] + [0.0, -0.0, 2.5, -2.5, 1e10, -1e10]
other = [random.uniform(0.5, 4.0) for _ in range(len(anyx))]
signed = [random.choice([-1.0, 1.0]) * random.uniform(0.5, 4.0) for _ in range(len(anyx))]
add_column('Pos', pos)
add_column('Unit', unit)
add_column('Any', anyx)
add_column('Other', other)
add_column('Signed', signed)

# (formula, expected values, tolerance in ulps)
cases = [
    ('sqrt(%s)' % letter('Pos'), [math.sqrt(v) for v in pos], 0),
    ('ln(%s)' % letter('Pos'), [math.log(v) for v in pos], 4),
    ('log10(%s)' % letter('Pos'), [math.log10(v) for v in pos], 4),
    ('log2(%s)' % letter('Pos'), [math.log2(v) for v in pos], 4),
    ('asin(%s)' % letter('Unit'), [math.asin(v) for v in unit], 4),
    ('acos(%s)' % letter('Unit'), [math.acos(v) for v in unit], 4),
    ('atan2(%s, %s)' % (letter('Any'), letter('Signed')),
     [math.atan2(y, x) for y, x in zip(anyx, signed)], 4),
    ('pow(%s, %s)' % (letter('Other'), letter('Signed')),
     [math.pow(a, b) for a, b in zip(other, signed)], 4),
    ('floor(%s)' % letter('Any'), [float(math.floor(v)) for v in anyx], 0),
    ('ceil(%s)' % letter('Any'), [float(math.ceil(v)) for v in anyx], 0),
    ('sign(%s)' % letter('Any'), [float((v > 0) - (v < 0)) for v in anyx], 0),
    ('mod(%s, %s)' % (letter('Any'), letter('Signed')),
     [a % b for a, b in zip(anyx, signed)], 0),
]
failures = []
for text, expected, tolerance in cases:
    got = pyst.values(formula_column(text))
    assert len(got) == len(expected), (text, len(got), len(expected))
    worst = max(ulps(g, e) for g, e in zip(got, expected))
    if worst > tolerance:
        failures.append((text, worst, tolerance))
assert not failures, failures
"#,
    );
}

/// `round(x, n)` against Python's `decimal`: round half up (away from zero on
/// the magnitude) of the shortest representation `repr(x)`, exact equality.
#[test]
fn round_agrees_with_python_decimal_on_the_shortest_representation() {
    let bench = bench();
    script(
        &bench,
        r#"
import random
from decimal import Decimal, ROUND_HALF_UP

def add_column(name, data):
    col = pyst.create_column(S1, name)
    pyst.set_values(col, data)
    return col

random.seed(428)
xs, ns = [], []
for _ in range(400):
    n = random.randint(-3, 12)
    kind = random.randint(0, 2)
    if kind == 0:
        x = random.uniform(-1000.0, 1000.0)
    elif kind == 1:
        # Ties in decimal: k.5 scaled by 10^-n (1.005, 2.675...)
        x = (random.randint(-5000, 5000) * 10 + 5) / 10.0 ** (n + 1)
    else:
        x = round(random.uniform(-100.0, 100.0), random.randint(1, 6))
    xs.append(x)
    ns.append(float(n))
cx = add_column('X', xs)
cn = add_column('N', ns)
names = [pyst.info(c)['name'] for c in pyst.children(S1)]
lx, ln = chr(ord('A') + names.index('X')), chr(ord('A') + names.index('N'))
got = pyst.values(formula_column('round(%s, %s)' % (lx, ln)))
assert len(got) == len(xs), (len(got), len(xs))
bad = []
for x, n, g in zip(xs, ns, got):
    expected = float(Decimal(repr(x)).quantize(Decimal(1).scaleb(-int(n)), rounding=ROUND_HALF_UP))
    if g != expected:
        bad.append((x, n, g, expected))
assert not bad, bad[:10]
"#,
    );
}

/// What the differential check holds to a tighter bound: the relative error of
/// `erf` and `erfc` in the tail, and `sum`, `mean`, `std` at the tolerance a
/// compensated summation gives (2 ulps from `math.fsum`) (#429).
#[test]
fn erf_erfc_and_the_aggregates_meet_their_target_tolerance() {
    let bench = bench();
    script(
        &bench,
        r#"
import math, random, statistics, struct

def ordered(x):
    bits = struct.unpack('<q', struct.pack('<d', x))[0]
    return -(bits & 0x7fffffffffffffff) if bits < 0 else bits

def ulps(a, b):
    if math.isnan(a) and math.isnan(b):
        return 0
    if math.isnan(a) or math.isnan(b):
        return 10**9
    if a == b:
        return 0
    return abs(ordered(a) - ordered(b))

def add_column(name, data):
    col = pyst.create_column(S1, name)
    pyst.set_values(col, data)
    return col

def letter(name):
    names = [pyst.info(c)['name'] for c in pyst.children(S1)]
    return chr(ord('A') + names.index(name))

random.seed(412)
data = [random.uniform(-3.0, 3.0) for _ in range(500)]
data += [1e-300, -1e-300, 0.0, 1e10, 123456.789]
add_column('Rand', data)
x = letter('Rand')
for text, fn in (('erf(%s)', math.erf), ('erfc(%s)', math.erfc)):
    got = pyst.values(formula_column(text % x))
    assert max(ulps(g, fn(v)) for g, v in zip(got, data)) <= 4, text
agg = {
    'sum(%s)': math.fsum(data),
    'mean(%s)': math.fsum(data) / len(data),
    'std(%s)': statistics.stdev(data),
}
for text, expected in agg.items():
    got = pyst.values(formula_column(text % x))
    assert max(ulps(g, expected) for g in got) <= 2, text
"#,
    );
}
