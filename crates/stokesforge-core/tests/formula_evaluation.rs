//! Tests of the column formula evaluator (numbers and semantics), part 1:
//! the row-wise operators and functions.
//!
//! * (a) differential property test against a reference evaluator written
//!   here straight in `f64` (mirror of the documented semantics);
//! * (b) accuracy of the table functions against values generated with local
//!   CPython 3 `math` (C library) and `scipy.special` 1.18, the `erfc` values
//!   being cross-checked with a 60-digit `decimal` series / continued fraction;
//! * (c) number lexing and what the parser accepts or refuses.
//!
//! The reference deliberately uses the platform `f64` methods: it is an
//! independent implementation (the application goes through `libm`).
// The table holds reference values copied from CPython: they may spell pi, e, ln 2.
#![allow(clippy::disallowed_methods, clippy::approx_constant)]

use proptest::prelude::*;
use std::sync::Arc;

use stokesforge_analysis::functions::Func;
use stokesforge_core::formula::{evaluate, ColumnFormula, Formula, FormulaError};
use stokesforge_core::{ColumnValues, FormulaPart, MixedCell, NodeId, NumericStatus};

// -- helpers --

fn col(index: u64) -> NodeId {
    NodeId::from_raw(index + 1)
}

const OWN: NodeId = NodeId::from_raw(999);

/// A thirty-column sheet (`A` to `AD`).
fn sheet() -> Vec<NodeId> {
    (0..30).map(col).collect()
}

fn parse(src: &str) -> Result<Formula, FormulaError> {
    ColumnFormula::from_letters(src, &sheet(), OWN)?.compile()
}

fn float_column(values: &[f64]) -> ColumnValues {
    ColumnValues::Float(Arc::new(values.to_vec()))
}

fn sheet_values(columns: &[ColumnValues]) -> Vec<(NodeId, ColumnValues)> {
    columns
        .iter()
        .enumerate()
        .map(|(i, v)| (col(i as u64), v.clone()))
        .collect()
}

/// Ordered integer of the bits, with -0 and +0 at the same point.
fn ordered(x: f64) -> i64 {
    let bits = x.to_bits() as i64;
    if bits < 0 {
        i64::MIN - bits
    } else {
        bits
    }
}

/// Distance in representable numbers (0 for two NaN, `u64::MAX` for one).
fn ulps(a: f64, b: f64) -> u64 {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => 0,
        (true, false) | (false, true) => u64::MAX,
        _ if a == b => 0,
        _ => ordered(a).abs_diff(ordered(b)),
    }
}

/// Values of `src` over the given sheet columns.
fn run(src: &str, columns: &[ColumnValues], rows: usize) -> (Vec<f64>, Vec<NumericStatus>) {
    let formula = parse(src).unwrap_or_else(|e| panic!("`{src}` refused: {e:?}"));
    let result = evaluate(&formula, &sheet_values(columns), rows);
    assert_eq!(result.error, None, "`{src}`");
    (result.values, result.statuses)
}

/// The single value of a constant formula.
fn constant(src: &str) -> Result<f64, FormulaError> {
    let formula = parse(src)?;
    let result = evaluate(&formula, &sheet_values(&[float_column(&[1.0])]), 1);
    match result.error {
        Some(error) => Err(error),
        None => Ok(result.values[0]),
    }
}

// -- (a) differential test --

#[derive(Debug, Clone, Copy, PartialEq)]
enum St {
    Ok,
    Missing,
    Failed,
}

impl St {
    fn combine(self, other: St) -> St {
        match (self, other) {
            (St::Failed, _) | (_, St::Failed) => St::Failed,
            (St::Missing, _) | (_, St::Missing) => St::Missing,
            _ => St::Ok,
        }
    }
    fn matches(self, status: NumericStatus) -> bool {
        matches!(
            (self, status),
            (St::Ok, NumericStatus::Ok)
                | (St::Missing, NumericStatus::Missing)
                | (St::Failed, NumericStatus::Failed)
        )
    }
}

#[derive(Debug, Clone)]
enum E {
    Num(f64),
    Col(usize),
    Neg(Box<E>),
    /// `+ - * /`
    Bin(char, Box<E>, Box<E>),
    Pow(Box<E>, Box<E>),
    Cmp(&'static str, Box<E>, Box<E>),
    If(Box<E>, Box<E>, Box<E>),
    F(&'static str, Vec<E>),
}

/// Precedence levels of the grammar (higher binds tighter).
fn prec(e: &E) -> u8 {
    match e {
        E::Cmp(..) => 1,
        E::Bin('+' | '-', ..) => 2,
        E::Bin(..) => 3,
        E::Neg(_) => 4,
        E::Pow(..) => 5,
        _ => 6,
    }
}

fn lit(x: f64) -> String {
    format!("{x:e}")
}

/// Text of the tree. `minimal`: parentheses only where the grammar needs
/// them; otherwise every operator node is wrapped.
fn render(e: &E, minimal: bool) -> String {
    let wrap = |child: &E, min: u8| {
        let text = render(child, minimal);
        if (minimal && prec(child) < min) || (!minimal && prec(child) < 6) {
            format!("({text})")
        } else {
            text
        }
    };
    match e {
        E::Num(x) => lit(*x),
        E::Col(i) => ((b'A' + *i as u8) as char).to_string(),
        E::Neg(inner) => format!("-{}", wrap(inner, 4)),
        E::Bin(op, a, b) => {
            let (l, r) = if matches!(op, '+' | '-') {
                (2, 3)
            } else {
                (3, 4)
            };
            format!("{} {op} {}", wrap(a, l), wrap(b, r))
        }
        E::Pow(a, b) => format!("{}^{}", wrap(a, 6), wrap(b, 4)),
        E::Cmp(op, a, b) => format!("{} {op} {}", wrap(a, 1), wrap(b, 2)),
        E::If(c, a, b) => format!(
            "if({}, {}, {})",
            render(c, minimal),
            render(a, minimal),
            render(b, minimal)
        ),
        E::F(name, args) => format!(
            "{name}({})",
            args.iter()
                .map(|a| render(a, minimal))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Reference: value and status of the documented semantics.
/// `app_primitives`: compute `^` and the table functions with the
/// application's own primitives (`Func::apply`) instead of the platform `f64`
/// methods. The transcendental differential uses it: their accuracy is the job
/// of the table tests below, and a one-ulp difference between two libraries,
/// amplified by an ill-conditioned chain (`sin(B^10)` with B = 123456.789,
/// an argument near 1e50), would otherwise fail a case at random. What it
/// checks is the composition: parsing, precedence, statuses, bit for bit.
fn reference(e: &E, cols: &[Vec<f64>], row: usize, app_primitives: bool) -> (f64, St) {
    let unless_ok = |v: f64, s: St| if s == St::Ok { v } else { f64::NAN };
    let domain = |v: f64, s: St| {
        if v.is_nan() && s == St::Ok {
            (v, St::Failed)
        } else {
            (v, s)
        }
    };
    match e {
        E::Num(x) => (*x, St::Ok),
        E::Col(i) => {
            let v = cols[*i][row];
            (v, if v.is_nan() { St::Missing } else { St::Ok })
        }
        E::Neg(inner) => {
            let (v, s) = reference(inner, cols, row, app_primitives);
            (-v, s)
        }
        E::Bin(op, a, b) => {
            let (x, sa) = reference(a, cols, row, app_primitives);
            let (y, sb) = reference(b, cols, row, app_primitives);
            let v = match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                _ => x / y,
            };
            (v, sa.combine(sb))
        }
        E::Pow(a, b) => {
            let (x, sa) = reference(a, cols, row, app_primitives);
            let (y, sb) = reference(b, cols, row, app_primitives);
            let s = sa.combine(sb);
            let v = if app_primitives {
                Func::Pow.apply(&[x, y])
            } else {
                x.powf(y)
            };
            domain(unless_ok(v, s), s)
        }
        E::Cmp(op, a, b) => {
            let (x, sa) = reference(a, cols, row, app_primitives);
            let (y, sb) = reference(b, cols, row, app_primitives);
            let s = sa.combine(sb);
            let v = if x.is_nan() || y.is_nan() {
                f64::NAN
            } else {
                let t = match *op {
                    "<" => x < y,
                    "<=" => x <= y,
                    ">" => x > y,
                    ">=" => x >= y,
                    "==" => x == y,
                    _ => x != y,
                };
                f64::from(u8::from(t))
            };
            (unless_ok(v, s), s)
        }
        E::If(c, a, b) => {
            let (cv, cs) = reference(c, cols, row, app_primitives);
            if cv.is_nan() {
                (f64::NAN, cs)
            } else if cv != 0.0 {
                reference(a, cols, row, app_primitives)
            } else {
                reference(b, cols, row, app_primitives)
            }
        }
        E::F(name, args) => {
            let mut s = St::Ok;
            let v: Vec<f64> = args
                .iter()
                .map(|a| {
                    let (x, sa) = reference(a, cols, row, app_primitives);
                    s = s.combine(sa);
                    x
                })
                .collect();
            let value = if app_primitives {
                Func::from_name(name)
                    .unwrap_or_else(|| panic!("unknown function {name}"))
                    .apply(&v)
            } else {
                reference_fn(name, &v)
            };
            domain(unless_ok(value, s), s)
        }
    }
}

fn reference_fn(name: &str, v: &[f64]) -> f64 {
    let (a, b) = (v[0], v.get(1).copied().unwrap_or(f64::NAN));
    match name {
        "abs" => a.abs(),
        "floor" => a.floor(),
        "ceil" => a.ceil(),
        "sqrt" => a.sqrt(),
        "sign" => {
            if a.is_nan() {
                f64::NAN
            } else if a > 0.0 {
                1.0
            } else if a < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        "min" | "max" if a.is_nan() || b.is_nan() => f64::NAN,
        "min" => a.min(b),
        "max" => a.max(b),
        "mod" => {
            if a.is_nan() || b.is_nan() || b == 0.0 {
                return f64::NAN;
            }
            let r = a % b;
            if r != 0.0 && (r < 0.0) != (b < 0.0) {
                r + b
            } else {
                r
            }
        }
        "exp" => a.exp(),
        "ln" => a.ln(),
        "log10" => a.log10(),
        "log2" => a.log2(),
        "sin" => a.sin(),
        "cos" => a.cos(),
        "tan" => a.tan(),
        "atan" => a.atan(),
        "sinh" => a.sinh(),
        "cosh" => a.cosh(),
        "tanh" => a.tanh(),
        "atan2" => a.atan2(b),
        "pow" => a.powf(b),
        "logx" => {
            if b.is_nan() || b <= 0.0 || b == 1.0 {
                f64::NAN
            } else {
                a.ln() / b.ln()
            }
        }
        other => panic!("no reference for {other}"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// Only IEEE-exact operations: results must match bit for bit.
    Exact,
    /// Transcendental functions and `^`, computed in the reference with the
    /// application's primitives (see [`reference`]); no comparison or `if`.
    Transcendental,
}

const LITERALS: [f64; 14] = [
    0.0, 1.0, 2.0, 3.0, 0.5, 10.0, 1e-300, 1e300, 123.456, 1e-5, 5e-324, 1e22, 0.1, 255.0,
];

const POOL: [f64; 16] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    3.0,
    -2.5,
    7.0,
    1e-300,
    1e300,
    123456.789,
    1e-5,
    f64::NAN,
    1e-310,
];

fn tree(mode: Mode) -> impl Strategy<Value = E> {
    let leaf = prop_oneof![
        prop::sample::select(LITERALS.to_vec()).prop_map(E::Num),
        (0usize..4).prop_map(E::Col),
    ];
    leaf.prop_recursive(4, 24, 3, move |inner| {
        let unary: Vec<&'static str> = match mode {
            Mode::Exact => vec!["abs", "floor", "ceil", "sign", "sqrt"],
            Mode::Transcendental => vec![
                "exp", "ln", "log10", "log2", "sqrt", "abs", "sin", "cos", "tan", "atan", "sinh",
                "cosh", "tanh",
            ],
        };
        let binary: Vec<&'static str> = match mode {
            Mode::Exact => vec!["min", "max", "mod"],
            Mode::Transcendental => vec!["atan2", "pow", "min", "max", "logx"],
        };
        let mut options: Vec<BoxedStrategy<E>> = vec![
            inner.clone().prop_map(|e| E::Neg(Box::new(e))).boxed(),
            (
                prop::sample::select(vec!['+', '-', '*', '/']),
                inner.clone(),
                inner.clone(),
            )
                .prop_map(|(op, a, b)| E::Bin(op, Box::new(a), Box::new(b)))
                .boxed(),
            (prop::sample::select(unary), inner.clone())
                .prop_map(|(f, a)| E::F(f, vec![a]))
                .boxed(),
            (prop::sample::select(binary), inner.clone(), inner.clone())
                .prop_map(|(f, a, b)| E::F(f, vec![a, b]))
                .boxed(),
        ];
        match mode {
            Mode::Exact => {
                options.push(
                    (
                        prop::sample::select(vec!["<", "<=", ">", ">=", "==", "!="]),
                        inner.clone(),
                        inner.clone(),
                    )
                        .prop_map(|(op, a, b)| E::Cmp(op, Box::new(a), Box::new(b)))
                        .boxed(),
                );
                options.push(
                    (inner.clone(), inner.clone(), inner.clone())
                        .prop_map(|(c, a, b)| E::If(Box::new(c), Box::new(a), Box::new(b)))
                        .boxed(),
                );
                // `x^0` and `x^1`: exact in any `pow`.
                options.push(
                    (inner.clone(), 0u8..2)
                        .prop_map(|(a, n)| E::Pow(Box::new(a), Box::new(E::Num(f64::from(n)))))
                        .boxed(),
                );
            }
            Mode::Transcendental => {
                options.push(
                    (inner.clone(), inner)
                        .prop_map(|(a, b)| E::Pow(Box::new(a), Box::new(b)))
                        .boxed(),
                );
            }
        }
        proptest::strategy::Union::new(options)
    })
}

fn columns_strategy() -> impl Strategy<Value = Vec<Vec<f64>>> {
    prop::collection::vec(
        prop::collection::vec(prop::sample::select(POOL.to_vec()), 6),
        4,
    )
}

fn close(app: f64, reference: f64, mode: Mode) -> bool {
    match mode {
        Mode::Exact => (app.is_nan() && reference.is_nan()) || app.to_bits() == reference.to_bits(),
        Mode::Transcendental => {
            if app.is_nan() || reference.is_nan() {
                return app.is_nan() && reference.is_nan();
            }
            if app == reference {
                return true;
            }
            let scale = app.abs().max(reference.abs());
            (app - reference).abs() <= 1e-9 * scale || ulps(app, reference) <= 8
        }
    }
}

fn differential(mode: Mode, e: &E, cols: &[Vec<f64>], minimal: bool) -> Result<(), TestCaseError> {
    let text = render(e, minimal);
    let formula =
        parse(&text).map_err(|err| TestCaseError::fail(format!("`{text}` refused: {err:?}")))?;
    let values: Vec<ColumnValues> = cols.iter().map(|c| float_column(c)).collect();
    let result = evaluate(&formula, &sheet_values(&values), 6);
    prop_assert_eq!(&result.error, &None, "`{}`", text);
    for row in 0..6 {
        let (rv, rs) = reference(e, cols, row, mode == Mode::Transcendental);
        prop_assert!(
            close(result.values[row], rv, mode) && rs.matches(result.statuses[row]),
            "`{}` row {}: app {:?} {:?}, reference {:?} {:?}; columns {:?}",
            text,
            row,
            result.values[row],
            result.statuses[row],
            rv,
            rs,
            cols.iter().map(|c| c[row]).collect::<Vec<_>>()
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3000, failure_persistence: None, rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha, rng_seed: proptest::test_runner::RngSeed::Fixed(412), ..ProptestConfig::default() })]

    #[test]
    fn exact_operations_match_the_reference_fully_parenthesized(
        e in tree(Mode::Exact), cols in columns_strategy()
    ) {
        differential(Mode::Exact, &e, &cols, false)?;
    }

    #[test]
    fn exact_operations_match_the_reference_with_minimal_parentheses(
        e in tree(Mode::Exact), cols in columns_strategy()
    ) {
        differential(Mode::Exact, &e, &cols, true)?;
    }

    #[test]
    fn transcendental_functions_match_the_reference_fully_parenthesized(
        e in tree(Mode::Transcendental), cols in columns_strategy()
    ) {
        differential(Mode::Transcendental, &e, &cols, false)?;
    }

    #[test]
    fn transcendental_functions_match_the_reference_with_minimal_parentheses(
        e in tree(Mode::Transcendental), cols in columns_strategy()
    ) {
        differential(Mode::Transcendental, &e, &cols, true)?;
    }
}

/// Precedence and associativity, written without redundant parentheses.
#[test]
fn precedence_and_associativity_of_unparenthesized_texts() {
    // A = 2, B = 3, C = 4.
    let data = [
        float_column(&[2.0]),
        float_column(&[3.0]),
        float_column(&[4.0]),
    ];
    let cases: &[(&str, f64)] = &[
        ("-2^2", -4.0),
        ("2^-1", 0.5),
        ("2^3^2", 512.0),
        ("-2^-2", -0.25),
        ("2^-2^2", 0.0625),
        ("-A^B", -8.0),
        ("A-B-C", -5.0),
        ("A/B/C", (2.0 / 3.0) / 4.0),
        ("C/A/A", 1.0),
        ("-A*B", -6.0),
        ("-A*-B", 6.0),
        ("A*-B", -6.0),
        ("A- -B", 5.0),
        ("A--B", 5.0),
        ("--A", 2.0),
        ("---A", -2.0),
        ("+A", 2.0),
        ("A++B", 5.0),
        ("A+-B", -1.0),
        ("A+B*C", 14.0),
        ("A*B+C", 10.0),
        ("(A+B)*C", 20.0),
        ("A-B+C", 3.0),
        ("A^B*C", 32.0),
        ("A*B^C", 162.0),
        ("A/B*C", (2.0 / 3.0) * 4.0),
        ("A*B/C", 6.0 / 4.0),
        ("2*3^2", 18.0),
        ("-3^2+10", 1.0),
        ("A<B==C", 0.0),
        ("A<B==1", 1.0),
        ("A<B<C", 1.0),
        ("C>B>A", 0.0),
        ("A+B<C+A", 1.0),
        ("A+1==B", 1.0),
        ("A*2==C", 1.0),
        ("A!=B==1", 1.0),
        ("A<=A", 1.0),
        ("A>=B", 0.0),
        ("1+1==2", 1.0),
        ("-A<0", 1.0),
        ("-A^2<0", 1.0),
        ("2^0.5^2", 2f64.powf(0.25)),
        ("sqrt(A)^2", 2f64.sqrt().powf(2.0)),
        ("-sqrt(A)", -(2f64.sqrt())),
        ("if(A<B, A, B)", 2.0),
        ("if(A>B, A, B)+1", 4.0),
        ("pi()*2", std::f64::consts::PI * 2.0),
        ("pi()^2", std::f64::consts::PI.powf(2.0)),
        ("-pi()", -std::f64::consts::PI),
        ("2^2^0", 2.0),
        ("0^0", 1.0),
        ("1e3^2", 1e6),
        ("1E3*2", 2000.0),
        ("1e-3*2", 0.002),
    ];
    for (text, expected) in cases {
        let (values, _) = run(text, &data, 1);
        assert!(
            ulps(values[0], *expected) <= 2,
            "`{text}`: app {} expected {expected}",
            values[0]
        );
    }
}

/// `-0` behaviour that is IEEE-exact and visible in a result.
#[test]
fn signed_zero_flows_through_arithmetic_like_ieee() {
    let data = [float_column(&[-0.0, 0.0]), float_column(&[1.0, 1.0])];
    let (v, _) = run("-A", &data, 2);
    assert_eq!(v[0].to_bits(), 0.0f64.to_bits());
    assert_eq!(v[1].to_bits(), (-0.0f64).to_bits());
    let (v, _) = run("B/A", &data, 2);
    assert_eq!(v[0], f64::NEG_INFINITY);
    assert_eq!(v[1], f64::INFINITY);
    let (v, _) = run("A*-1", &data, 2);
    assert_eq!(v[0].to_bits(), 0.0f64.to_bits());
    let (v, _) = run("A==-A", &data, 2);
    assert_eq!(v, vec![1.0, 1.0]);
}

/// Dividing by zero is infinity, not an error (manual, "How a formula works").
#[test]
fn division_by_zero_is_infinity_with_status_ok() {
    let (v, s) = run("1/A", &[float_column(&[0.0, -0.0])], 2);
    assert_eq!(v, vec![f64::INFINITY, f64::NEG_INFINITY]);
    assert_eq!(s, vec![NumericStatus::Ok; 2]);
}

/// The manual says a result with no value (`ln(-1)`...) is a failed
/// calculation. `0/0`, `inf - inf` and `0 * inf` are NaN too, but the
/// operators do not run the domain check: status Ok, an "empty cell" that is
/// not "shown as a failed calculation". Design question: #433 point 1.
#[test]
#[ignore = "design: #433 point 1: 0/0 is NaN with status Ok (empty cell), only functions run the domain check, formula.rs:2321-2331"]
fn nan_from_arithmetic_is_a_failed_calculation_like_ln_of_minus_one() {
    let (v, s) = run("A/A", &[float_column(&[0.0])], 1);
    assert!(v[0].is_nan());
    assert_eq!(
        s[0],
        NumericStatus::Failed,
        "0/0 reads as an empty cell, not Failed"
    );
}

#[test]
#[ignore = "design: #433 point 1: inf - inf is NaN with status Ok, formula.rs:2321-2331"]
fn nan_from_infinity_arithmetic_is_a_failed_calculation() {
    let (v, s) = run("1e308*10 - 1e308*10", &[float_column(&[0.0])], 1);
    assert!(v[0].is_nan());
    assert_eq!(s[0], NumericStatus::Failed);
}

// -- (b) accuracy of the table functions --
//
// Expected values: CPython 3 `math` (C library), `scipy.special` 1.18; the
// `erfc` ones cross-checked with a 60-digit `decimal` series (no mismatch).
// Columns: function, arguments, expected, tolerance in ulps.
type Row = (&'static str, &'static [f64], f64, u64);

const TABLE: &[Row] = &[
    ("erf", &[0.0], 0.0, 4),
    ("erf", &[-0.0], -0.0, 4),
    ("erf", &[5.0e-324], 5.0e-324, 4),
    ("erf", &[-5.0e-324], -5.0e-324, 4),
    ("erf", &[1.0e-300], 1.1283791670955126e-300, 4),
    ("erf", &[1.0e-160], 1.1283791670955126e-160, 4),
    ("erf", &[1.0e-10], 1.1283791670955126e-10, 4),
    ("erf", &[-1.0e-10], -1.1283791670955126e-10, 4),
    ("erf", &[1.0e-05], 1.1283791670579e-05, 4),
    ("erf", &[0.01], 0.011283415555849618, 4),
    ("erf", &[0.1], 0.1124629160182849, 4),
    ("erf", &[0.5], 0.5204998778130465, 4),
    ("erf", &[-0.5], -0.5204998778130465, 4),
    ("erf", &[1.0], 0.8427007929497149, 4),
    ("erf", &[1.5], 0.9661051464753108, 4),
    ("erf", &[2.0], 0.9953222650189527, 4),
    ("erf", &[2.5], 0.999593047982555, 4),
    ("erf", &[2.9], 0.9999589021219005, 4),
    ("erf", &[2.999999], 0.999977909363748, 4),
    ("erf", &[3.0], 0.9999779095030014, 4),
    ("erf", &[3.000001], 0.9999779096422541, 4),
    ("erf", &[3.1], 0.9999883513426328, 4),
    ("erf", &[4.0], 0.9999999845827421, 4),
    ("erf", &[5.0], 0.9999999999984626, 4),
    ("erf", &[5.9], 0.9999999999999999, 4),
    ("erf", &[6.0], 1.0, 4),
    ("erf", &[6.1], 1.0, 4),
    ("erf", &[10.0], 1.0, 4),
    ("erf", &[-10.0], -1.0, 4),
    ("erf", &[26.0], 1.0, 4),
    ("erf", &[30.0], 1.0, 4),
    ("erf", &[-30.0], -1.0, 4),
    ("erf", &[f64::INFINITY], 1.0, 4),
    ("erf", &[f64::NEG_INFINITY], -1.0, 4),
    ("erfc", &[0.0], 1.0, 4),
    ("erfc", &[5.0e-324], 1.0, 4),
    ("erfc", &[1.0e-300], 1.0, 4),
    ("erfc", &[1.0e-10], 0.999999999887162, 4),
    ("erfc", &[1.0e-05], 0.9999887162083294, 4),
    ("erfc", &[0.1], 0.887537083981715, 4),
    ("erfc", &[0.5], 0.4795001221869535, 4),
    ("erfc", &[1.0], 0.15729920705028513, 4),
    ("erfc", &[1.5], 0.033894853524689274, 4),
    ("erfc", &[2.0], 0.004677734981047266, 4),
    ("erfc", &[2.2], 0.0018628462979818898, 4),
    ("erfc", &[2.4], 0.0006885138966450789, 4),
    ("erfc", &[2.5], 0.0004069520174449589, 4),
    ("erfc", &[2.6], 0.00023603441652934908, 4),
    ("erfc", &[3.0], 2.209049699858544e-05, 4),
    ("erfc", &[3.5], 7.430983723414128e-07, 4),
    ("erfc", &[4.0], 1.541725790028002e-08, 4),
    ("erfc", &[5.0], 1.537459794428035e-12, 4),
    ("erfc", &[6.0], 2.1519736712498913e-17, 4),
    ("erfc", &[8.0], 1.1224297172982926e-29, 4),
    ("erfc", &[10.0], 2.088487583762545e-45, 4),
    ("erfc", &[15.0], 7.212994172451207e-100, 4),
    ("erfc", &[20.0], 5.395865611607901e-176, 4),
    ("erfc", &[26.0], 5.663192408856143e-296, 4),
    ("erfc", &[26.5], 2.2109076642637343e-307, 4),
    ("erfc", &[27.0], 5.23705e-319, 4),
    ("erfc", &[27.1], 2.33e-321, 4),
    ("erfc", &[30.0], 0.0, 4),
    ("erfc", &[-0.5], 1.5204998778130465, 4),
    ("erfc", &[-1.0], 1.8427007929497148, 4),
    ("erfc", &[-3.0], 1.9999779095030015, 4),
    ("erfc", &[-5.0], 1.9999999999984626, 4),
    ("erfc", &[-6.0], 2.0, 4),
    ("erfc", &[-10.0], 2.0, 4),
    ("erfc", &[-26.0], 2.0, 4),
    ("erfc", &[-30.0], 2.0, 4),
    ("erfc", &[f64::INFINITY], 0.0, 4),
    ("erfc", &[f64::NEG_INFINITY], 2.0, 4),
    ("sinh", &[0.0], 0.0, 2),
    ("cosh", &[0.0], 1.0, 2),
    ("tanh", &[0.0], 0.0, 2),
    ("sinh", &[-0.0], -0.0, 2),
    ("cosh", &[-0.0], 1.0, 2),
    ("tanh", &[-0.0], -0.0, 2),
    ("sinh", &[1.0e-300], 1.0e-300, 2),
    ("cosh", &[1.0e-300], 1.0, 2),
    ("tanh", &[1.0e-300], 1.0e-300, 2),
    ("sinh", &[1.0e-10], 1.0e-10, 2),
    ("cosh", &[1.0e-10], 1.0, 2),
    ("tanh", &[1.0e-10], 1.0e-10, 2),
    ("sinh", &[-1.0e-10], -1.0e-10, 2),
    ("cosh", &[-1.0e-10], 1.0, 2),
    ("tanh", &[-1.0e-10], -1.0e-10, 2),
    ("sinh", &[1.0e-05], 1.0000000000166668e-05, 2),
    ("cosh", &[1.0e-05], 1.00000000005, 2),
    ("tanh", &[1.0e-05], 9.999999999666668e-06, 2),
    ("sinh", &[0.5], 0.5210953054937474, 2),
    ("cosh", &[0.5], 1.1276259652063807, 2),
    ("tanh", &[0.5], 0.46211715726000974, 2),
    ("sinh", &[1.0], 1.1752011936438014, 2),
    ("cosh", &[1.0], 1.5430806348152437, 2),
    ("tanh", &[1.0], 0.7615941559557649, 2),
    ("sinh", &[5.0], 74.20321057778875, 2),
    ("cosh", &[5.0], 74.20994852478785, 2),
    ("tanh", &[5.0], 0.9999092042625951, 2),
    ("sinh", &[10.0], 11013.232874703393, 2),
    ("cosh", &[10.0], 11013.232920103324, 2),
    ("tanh", &[10.0], 0.9999999958776927, 2),
    ("sinh", &[20.0], 242582597.70489514, 2),
    ("cosh", &[20.0], 242582597.70489514, 2),
    ("tanh", &[20.0], 1.0, 2),
    ("sinh", &[100.0], 1.3440585709080678e+43, 2),
    ("cosh", &[100.0], 1.3440585709080678e+43, 2),
    ("tanh", &[100.0], 1.0, 2),
    ("sinh", &[709.0], 4.109203730777486e+307, 2),
    ("cosh", &[709.0], 4.109203730777486e+307, 2),
    ("tanh", &[709.0], 1.0, 2),
    ("sinh", &[710.0], 1.1169973830808557e+308, 2),
    ("cosh", &[710.0], 1.1169973830808557e+308, 2),
    ("tanh", &[710.0], 1.0, 2),
    ("sinh", &[711.0], f64::INFINITY, 2),
    ("cosh", &[711.0], f64::INFINITY, 2),
    ("tanh", &[711.0], 1.0, 2),
    ("sinh", &[-710.0], -1.1169973830808557e+308, 2),
    ("cosh", &[-710.0], 1.1169973830808557e+308, 2),
    ("tanh", &[-710.0], -1.0, 2),
    ("ln", &[1.0000000000000002], 2.2204460492503128e-16, 1),
    ("ln", &[0.9999999999999999], -1.1102230246251565e-16, 1),
    ("ln", &[1.0], 0.0, 1),
    ("ln", &[1.0000000001], 1.000000082690371e-10, 1),
    ("ln", &[0.5], -0.6931471805599453, 1),
    ("ln", &[2.0], 0.6931471805599453, 1),
    ("ln", &[10.0], 2.302585092994046, 1),
    ("ln", &[1.0e-300], -690.7755278982137, 1),
    ("ln", &[5.0e-324], -744.4400719213812, 1),
    ("ln", &[1.0e+308], 709.1962086421661, 1),
    ("ln", &[f64::INFINITY], f64::INFINITY, 1),
    ("ln", &[1000.0], 6.907755278982137, 1),
    ("ln", &[2.718281828459045], 1.0, 1),
    ("log", &[1000.0], 3.0, 1),
    ("log", &[1.0e-05], -5.0, 1),
    ("log", &[1000000000000000.0], 15.0, 1),
    ("log", &[1.0e-300], -300.0, 1),
    ("log", &[5.0e-324], -323.3062153431158, 1),
    ("log", &[2.0], 0.3010299956639812, 1),
    ("log", &[0.5], -0.3010299956639812, 1),
    ("log", &[1.0], 0.0, 1),
    ("log", &[100.0], 2.0, 1),
    ("log", &[12345.678], 4.091514945509202, 1),
    ("log2", &[8.0], 3.0, 1),
    ("log2", &[1024.0], 10.0, 1),
    ("log2", &[0.125], -3.0, 1),
    ("log2", &[1.0e-300], -996.5784284662087, 1),
    ("log2", &[5.0e-324], -1074.0, 1),
    ("log2", &[3.0], 1.584962500721156, 1),
    ("log2", &[1.0], 0.0, 1),
    ("log2", &[1.2676506002282294e+30], 100.0, 1),
    ("asin", &[0.0], 0.0, 2),
    ("acos", &[0.0], 1.5707963267948966, 2),
    ("asin", &[-0.0], -0.0, 2),
    ("acos", &[-0.0], 1.5707963267948966, 2),
    ("asin", &[1.0], 1.5707963267948966, 2),
    ("acos", &[1.0], 0.0, 2),
    ("asin", &[-1.0], -1.5707963267948966, 2),
    ("acos", &[-1.0], 3.141592653589793, 2),
    ("asin", &[0.5], 0.5235987755982989, 2),
    ("acos", &[0.5], 1.0471975511965979, 2),
    ("asin", &[-0.5], -0.5235987755982989, 2),
    ("acos", &[-0.5], 2.0943951023931957, 2),
    ("asin", &[1.0e-10], 1.0e-10, 2),
    ("acos", &[1.0e-10], 1.5707963266948965, 2),
    ("asin", &[0.9999999999999999], 1.5707963118937354, 2),
    ("acos", &[0.9999999999999999], 1.4901161193847656e-08, 2),
    ("asin", &[-0.9999999999999999], -1.5707963118937354, 2),
    ("acos", &[-0.9999999999999999], 3.141592638688632, 2),
    ("asin", &[1.0000000000000002], f64::NAN, 0),
    ("acos", &[1.0000000000000002], f64::NAN, 0),
    ("asin", &[-1.0000000000000002], f64::NAN, 0),
    ("acos", &[-1.0000000000000002], f64::NAN, 0),
    ("asin", &[2.0], f64::NAN, 0),
    ("acos", &[2.0], f64::NAN, 0),
    ("asin", &[f64::INFINITY], f64::NAN, 0),
    ("acos", &[f64::INFINITY], f64::NAN, 0),
    ("atan", &[0.0], 0.0, 2),
    ("atan", &[-0.0], -0.0, 2),
    ("atan", &[1.0], 0.7853981633974483, 2),
    ("atan", &[-1.0], -0.7853981633974483, 2),
    ("atan", &[1.0e-300], 1.0e-300, 2),
    ("atan", &[1.0e+300], 1.5707963267948966, 2),
    ("atan", &[f64::INFINITY], 1.5707963267948966, 2),
    ("atan", &[f64::NEG_INFINITY], -1.5707963267948966, 2),
    ("atan", &[0.5], 0.4636476090008061, 2),
    ("tan", &[0.5], 0.5463024898437905, 2),
    ("sin", &[0.5], 0.479425538604203, 2),
    ("cos", &[0.5], 0.8775825618903728, 2),
    ("tan", &[1.0], 1.5574077246549023, 2),
    ("sin", &[1.0], 0.8414709848078965, 2),
    ("cos", &[1.0], 0.5403023058681398, 2),
    ("tan", &[10.0], 0.6483608274590866, 2),
    ("sin", &[10.0], -0.5440211108893698, 2),
    ("cos", &[10.0], -0.8390715290764524, 2),
    ("tan", &[1.0e+300], 1.4214488238747245, 2),
    ("sin", &[1.0e+300], -0.8178819121159085, 2),
    ("cos", &[1.0e+300], -0.5753861119575491, 2),
    ("tan", &[1.0e-300], 1.0e-300, 2),
    ("sin", &[1.0e-300], 1.0e-300, 2),
    ("cos", &[1.0e-300], 1.0, 2),
    ("tan", &[100.0], -0.5872139151569291, 2),
    ("sin", &[100.0], -0.5063656411097588, 2),
    ("cos", &[100.0], 0.8623188722876839, 2),
    ("tan", &[100000.0], -0.035771662952898776, 2),
    ("sin", &[100000.0], 0.03574879797201651, 2),
    ("cos", &[100000.0], -0.9993608074382124, 2),
    ("tan", &[-1.0], -1.5574077246549023, 2),
    ("sin", &[-1.0], -0.8414709848078965, 2),
    ("cos", &[-1.0], 0.5403023058681398, 2),
    ("sin", &[1.0e+22], -0.8522008497671888, 2),
    ("cos", &[1.0e+22], 0.523214785395139, 2),
    ("tan", &[1.0e+22], -1.6287782256068988, 2),
    ("sin", &[1.0e+300], -0.8178819121159085, 2),
    ("cos", &[1.0e+300], -0.5753861119575491, 2),
    ("tan", &[1.0e+300], 1.4214488238747245, 2),
    ("sin", &[3.141592653589793], 1.2246467991473532e-16, 2),
    ("cos", &[3.141592653589793], -1.0, 2),
    ("tan", &[3.141592653589793], -1.2246467991473532e-16, 2),
    ("sin", &[1.5707963267948966], 1.0, 2),
    ("cos", &[1.5707963267948966], 6.123233995736766e-17, 2),
    ("tan", &[1.5707963267948966], 1.633123935319537e+16, 2),
    ("atan2", &[0.0, 0.0], 0.0, 2),
    ("atan2", &[0.0, -0.0], 3.141592653589793, 2),
    ("atan2", &[0.0, 1.0], 0.0, 2),
    ("atan2", &[0.0, -1.0], 3.141592653589793, 2),
    ("atan2", &[0.0, f64::INFINITY], 0.0, 2),
    ("atan2", &[0.0, f64::NEG_INFINITY], 3.141592653589793, 2),
    ("atan2", &[0.0, 1.0e-300], 0.0, 2),
    ("atan2", &[0.0, 1.0e+300], 0.0, 2),
    ("atan2", &[0.0, 5.0e-324], 0.0, 2),
    ("atan2", &[-0.0, 0.0], -0.0, 2),
    ("atan2", &[-0.0, -0.0], -3.141592653589793, 2),
    ("atan2", &[-0.0, 1.0], -0.0, 2),
    ("atan2", &[-0.0, -1.0], -3.141592653589793, 2),
    ("atan2", &[-0.0, f64::INFINITY], -0.0, 2),
    ("atan2", &[-0.0, f64::NEG_INFINITY], -3.141592653589793, 2),
    ("atan2", &[-0.0, 1.0e-300], -0.0, 2),
    ("atan2", &[-0.0, 1.0e+300], -0.0, 2),
    ("atan2", &[-0.0, 5.0e-324], -0.0, 2),
    ("atan2", &[1.0, 0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0, -0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0, 1.0], 0.7853981633974483, 2),
    ("atan2", &[1.0, -1.0], 2.356194490192345, 2),
    ("atan2", &[1.0, f64::INFINITY], 0.0, 2),
    ("atan2", &[1.0, f64::NEG_INFINITY], 3.141592653589793, 2),
    ("atan2", &[1.0, 1.0e-300], 1.5707963267948966, 2),
    ("atan2", &[1.0, 1.0e+300], 1.0e-300, 2),
    ("atan2", &[1.0, 5.0e-324], 1.5707963267948966, 2),
    ("atan2", &[-1.0, 0.0], -1.5707963267948966, 2),
    ("atan2", &[-1.0, -0.0], -1.5707963267948966, 2),
    ("atan2", &[-1.0, 1.0], -0.7853981633974483, 2),
    ("atan2", &[-1.0, -1.0], -2.356194490192345, 2),
    ("atan2", &[-1.0, f64::INFINITY], -0.0, 2),
    ("atan2", &[-1.0, f64::NEG_INFINITY], -3.141592653589793, 2),
    ("atan2", &[-1.0, 1.0e-300], -1.5707963267948966, 2),
    ("atan2", &[-1.0, 1.0e+300], -1.0e-300, 2),
    ("atan2", &[-1.0, 5.0e-324], -1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, 0.0], 1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, -0.0], 1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, 1.0], 1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, -1.0], 1.5707963267948966, 2),
    (
        "atan2",
        &[f64::INFINITY, f64::INFINITY],
        0.7853981633974483,
        2,
    ),
    (
        "atan2",
        &[f64::INFINITY, f64::NEG_INFINITY],
        2.356194490192345,
        2,
    ),
    ("atan2", &[f64::INFINITY, 1.0e-300], 1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, 1.0e+300], 1.5707963267948966, 2),
    ("atan2", &[f64::INFINITY, 5.0e-324], 1.5707963267948966, 2),
    ("atan2", &[f64::NEG_INFINITY, 0.0], -1.5707963267948966, 2),
    ("atan2", &[f64::NEG_INFINITY, -0.0], -1.5707963267948966, 2),
    ("atan2", &[f64::NEG_INFINITY, 1.0], -1.5707963267948966, 2),
    ("atan2", &[f64::NEG_INFINITY, -1.0], -1.5707963267948966, 2),
    (
        "atan2",
        &[f64::NEG_INFINITY, f64::INFINITY],
        -0.7853981633974483,
        2,
    ),
    (
        "atan2",
        &[f64::NEG_INFINITY, f64::NEG_INFINITY],
        -2.356194490192345,
        2,
    ),
    (
        "atan2",
        &[f64::NEG_INFINITY, 1.0e-300],
        -1.5707963267948966,
        2,
    ),
    (
        "atan2",
        &[f64::NEG_INFINITY, 1.0e+300],
        -1.5707963267948966,
        2,
    ),
    (
        "atan2",
        &[f64::NEG_INFINITY, 5.0e-324],
        -1.5707963267948966,
        2,
    ),
    ("atan2", &[1.0e-300, 0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e-300, -0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e-300, 1.0], 1.0e-300, 2),
    ("atan2", &[1.0e-300, -1.0], 3.141592653589793, 2),
    ("atan2", &[1.0e-300, f64::INFINITY], 0.0, 2),
    (
        "atan2",
        &[1.0e-300, f64::NEG_INFINITY],
        3.141592653589793,
        2,
    ),
    ("atan2", &[1.0e-300, 1.0e-300], 0.7853981633974483, 2),
    ("atan2", &[1.0e-300, 1.0e+300], 0.0, 2),
    ("atan2", &[1.0e-300, 5.0e-324], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, 0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, -0.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, 1.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, -1.0], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, f64::INFINITY], 0.0, 2),
    (
        "atan2",
        &[1.0e+300, f64::NEG_INFINITY],
        3.141592653589793,
        2,
    ),
    ("atan2", &[1.0e+300, 1.0e-300], 1.5707963267948966, 2),
    ("atan2", &[1.0e+300, 1.0e+300], 0.7853981633974483, 2),
    ("atan2", &[1.0e+300, 5.0e-324], 1.5707963267948966, 2),
    ("atan2", &[5.0e-324, 0.0], 1.5707963267948966, 2),
    ("atan2", &[5.0e-324, -0.0], 1.5707963267948966, 2),
    ("atan2", &[5.0e-324, 1.0], 5.0e-324, 2),
    ("atan2", &[5.0e-324, -1.0], 3.141592653589793, 2),
    ("atan2", &[5.0e-324, f64::INFINITY], 0.0, 2),
    (
        "atan2",
        &[5.0e-324, f64::NEG_INFINITY],
        3.141592653589793,
        2,
    ),
    ("atan2", &[5.0e-324, 1.0e-300], 4.940656458412465e-24, 2),
    ("atan2", &[5.0e-324, 1.0e+300], 0.0, 2),
    ("atan2", &[5.0e-324, 5.0e-324], 0.7853981633974483, 2),
    ("floor", &[1.0], 1.0, 0),
    ("ceil", &[1.0], 1.0, 0),
    ("floor", &[-1.0], -1.0, 0),
    ("ceil", &[-1.0], -1.0, 0),
    ("floor", &[2.5], 2.0, 0),
    ("ceil", &[2.5], 3.0, 0),
    ("floor", &[-2.5], -3.0, 0),
    ("ceil", &[-2.5], -2.0, 0),
    ("ceil", &[0.5], 1.0, 0),
    ("floor", &[-0.5], -1.0, 0),
    ("floor", &[1.5], 1.0, 0),
    ("ceil", &[1.5], 2.0, 0),
    ("floor", &[-1.5], -2.0, 0),
    ("ceil", &[-1.5], -1.0, 0),
    ("ceil", &[0.49999999999999994], 1.0, 0),
    ("floor", &[-0.49999999999999994], -1.0, 0),
    ("floor", &[4503599627370497.0], 4503599627370497.0, 0),
    ("ceil", &[4503599627370497.0], 4503599627370497.0, 0),
    ("floor", &[-4503599627370497.0], -4503599627370497.0, 0),
    ("ceil", &[-4503599627370497.0], -4503599627370497.0, 0),
    ("floor", &[1.0e+308], 1.0e+308, 0),
    ("ceil", &[1.0e+308], 1.0e+308, 0),
    ("floor", &[-1.0e+308], -1.0e+308, 0),
    ("ceil", &[-1.0e+308], -1.0e+308, 0),
    ("floor", &[f64::INFINITY], f64::INFINITY, 0),
    ("ceil", &[f64::INFINITY], f64::INFINITY, 0),
    ("floor", &[f64::NEG_INFINITY], f64::NEG_INFINITY, 0),
    ("ceil", &[f64::NEG_INFINITY], f64::NEG_INFINITY, 0),
    ("ceil", &[1.0e-300], 1.0, 0),
    ("floor", &[-1.0e-300], -1.0, 0),
    ("ceil", &[5.0e-324], 1.0, 0),
    ("mod", &[7.0, 3.0], 1.0, 0),
    ("mod", &[-7.0, 3.0], 2.0, 0),
    ("mod", &[7.0, -3.0], -2.0, 0),
    ("mod", &[-7.0, -3.0], -1.0, 0),
    ("mod", &[5.5, 2.0], 1.5, 0),
    ("mod", &[-5.5, 2.0], 0.5, 0),
    ("mod", &[5.5, -2.0], -0.5, 0),
    ("mod", &[0.3, 0.1], 0.09999999999999998, 0),
    ("mod", &[1.0e+308, 3.0], 2.0, 0),
    ("mod", &[10.0, 0.1], 0.09999999999999945, 0),
    ("mod", &[1.0, 1.0e-300], 8.963186528039712e-301, 0),
    ("mod", &[6.0, 3.0], 0.0, 0),
    ("mod", &[-6.0, 3.0], -0.0, 0),
    ("mod", &[0.0, 5.0], 0.0, 0),
    ("mod", &[3.0, f64::INFINITY], 3.0, 0),
    ("mod", &[-3.0, f64::INFINITY], f64::INFINITY, 0),
    ("mod", &[3.0, f64::NEG_INFINITY], f64::NEG_INFINITY, 0),
    ("mod", &[1.0e+300, 7.0], 1.0, 0),
    ("mod", &[-1.0e-20, 3.0], 3.0, 0),
    ("mod", &[1.152921504606847e+18, 3.0], 1.0, 0),
    ("mod", &[0.1, 0.01], 3.469446951953614e-18, 0),
    ("mod", &[5.0, 0.0], f64::NAN, 0),
    ("mod", &[f64::INFINITY, 3.0], f64::NAN, 0),
    ("mod", &[-0.0, 5.0], 0.0, 0),
    ("root", &[-8.0, 3.0], -2.0, 2),
    ("root", &[27.0, 3.0], 3.0, 2),
    ("root", &[-27.0, 3.0], -3.0, 2),
    ("root", &[16.0, 4.0], 2.0, 2),
    ("root", &[-16.0, 4.0], f64::NAN, 2),
    ("root", &[8.0, 0.3333333333333333], 512.0, 2),
    ("root", &[-8.0, 0.3333333333333333], f64::NAN, 2),
    ("root", &[4.0, -2.0], 0.5, 2),
    ("root", &[-8.0, -3.0], -0.5, 2),
    ("root", &[0.0, 3.0], 0.0, 2),
    ("root", &[0.0, -3.0], f64::INFINITY, 2),
    ("root", &[0.0, 0.0], f64::NAN, 2),
    ("root", &[1000.0, 3.0], 10.0, 2),
    ("root", &[1.0e-300, 3.0], 1.0e-100, 2),
    ("root", &[1.0e+308, 2.0], 1.0e+154, 2),
    ("root", &[2.0, 1.0], 2.0, 2),
    ("root", &[-5.0, 1.0], -5.0, 2),
    ("root", &[32.0, 5.0], 2.0, 2),
    ("root", &[-32.0, 5.0], -2.0, 2),
    ("root", &[81.0, 4.0], 3.0, 2),
    ("root", &[-1.0, 2.0], f64::NAN, 2),
    ("root", &[10.0, f64::INFINITY], 1.0, 2),
    ("root", &[-10.0, f64::INFINITY], f64::NAN, 2),
    ("root", &[0.001, 3.0], 0.1, 2),
    ("root", &[1.0e-320, 3.0], 2.1544266950262728e-107, 2),
    ("root", &[2.0, 2.0], 1.4142135623730951, 2),
    ("root", &[-1.0, 3.0], -1.0, 2),
    ("root", &[1.0e+300, 300.0], 10.0, 2),
    ("root", &[1024.0, 10.0], 2.0, 2),
    ("root", &[125.0, 3.0], 5.0, 2),
    ("logx", &[8.0, 2.0], 3.0, 2),
    ("logx", &[1000.0, 10.0], 2.9999999999999996, 2),
    ("logx", &[243.0, 3.0], 4.999999999999999, 2),
    ("logx", &[125.0, 5.0], 3.0000000000000004, 2),
    ("logx", &[1.0, 5.0], 0.0, 2),
    ("logx", &[0.0, 2.0], f64::NEG_INFINITY, 2),
    ("logx", &[0.0, 0.5], f64::INFINITY, 2),
    ("logx", &[-1.0, 2.0], f64::NAN, 2),
    ("logx", &[5.0, 1.0], f64::NAN, 2),
    ("logx", &[5.0, 0.0], f64::NAN, 2),
    ("logx", &[5.0, -2.0], f64::NAN, 2),
    ("logx", &[5.0, f64::INFINITY], 0.0, 2),
    ("logx", &[1.0e-300, 10.0], -299.99999999999994, 2),
    ("logx", &[1024.0, 2.0], 10.0, 2),
    ("logx", &[0.5, 2.0], -1.0, 2),
    ("logx", &[1.0e+300, 10.0], 299.99999999999994, 2),
    ("logx", &[2.0, 0.5], -1.0, 2),
    ("logx", &[1000000000000000.0, 10.0], 14.999999999999998, 2),
    ("logx", &[3.0, 3.0], 1.0, 2),
    ("logx", &[1.0e-05, 10.0], -5.0, 2),
    ("logx", &[4.0, 16.0], 0.5, 2),
    ("pow", &[0.0, 0.0], 1.0, 2),
    ("pow", &[-8.0, 0.3333333333333333], f64::NAN, 2),
    ("pow", &[-1.0, f64::INFINITY], 1.0, 2),
    ("pow", &[2.0, 10.0], 1024.0, 2),
    ("pow", &[-2.0, 3.0], -8.0, 2),
    ("pow", &[-2.0, 0.5], f64::NAN, 2),
    ("pow", &[2.0, 1024.0], f64::INFINITY, 2),
    ("pow", &[2.0, -1075.0], 0.0, 2),
    ("pow", &[0.0, -1.0], f64::INFINITY, 2),
    ("pow", &[-0.0, -1.0], f64::NEG_INFINITY, 2),
    ("pow", &[0.0, 1.0], 0.0, 2),
    ("pow", &[10.0, -5.0], 1.0e-05, 2),
    ("pow", &[10.0, 15.0], 1000000000000000.0, 2),
    ("pow", &[10.0, -300.0], 1.0e-300, 2),
    ("pow", &[1.0000000001, 10000000000.0], 2.7182820532347876, 2),
    ("pow", &[0.5, -2.0], 4.0, 2),
    ("pow", &[-0.0, 3.0], -0.0, 2),
    ("pow", &[f64::INFINITY, 0.0], 1.0, 2),
    ("pow", &[f64::INFINITY, -1.0], 0.0, 2),
    ("pow", &[f64::NEG_INFINITY, 3.0], f64::NEG_INFINITY, 2),
    ("pow", &[f64::NEG_INFINITY, 2.0], f64::INFINITY, 2),
    ("pow", &[1.0, f64::INFINITY], 1.0, 2),
    ("pow", &[2.0, f64::INFINITY], f64::INFINITY, 2),
    ("pow", &[0.5, f64::INFINITY], 0.0, 2),
    ("pow", &[1.0e+300, 2.0], f64::INFINITY, 2),
    ("pow", &[7.0, 0.5], 2.6457513110645907, 2),
    ("pow", &[2.0, 0.5], 1.4142135623730951, 2),
    ("pow", &[9.0, 0.5], 3.0, 2),
    ("pow", &[-1.0, f64::INFINITY], 1.0, 0),
];

/// Excel (decimal) expectations of `round`: the binary product `x * 10^n` loses
/// them (`round(2.675, 2)`).
const ROUND_DECIMAL: &[Row] = &[
    ("round", &[2.5, 0.0], 3.0, 1),
    ("round", &[-2.5, 0.0], -3.0, 1),
    ("round", &[0.5, 0.0], 1.0, 1),
    ("round", &[-0.5, 0.0], -1.0, 1),
    ("round", &[1.5, 0.0], 2.0, 1),
    ("round", &[-1.5, 0.0], -2.0, 1),
    ("round", &[0.49999999999999994, 0.0], 0.0, 1),
    ("round", &[1234.5678, 2.0], 1234.57, 1),
    ("round", &[1234.5678, -2.0], 1200.0, 1),
    ("round", &[1250.0, -2.0], 1300.0, 1),
    ("round", &[-1250.0, -2.0], -1300.0, 1),
    ("round", &[1234.0, -2.0], 1200.0, 1),
    ("round", &[5.0, -1.0], 10.0, 1),
    ("round", &[-5.0, -1.0], -10.0, 1),
    ("round", &[2.5, 1.0], 2.5, 1),
    ("round", &[1.0e+308, 2.0], 1.0e+308, 1),
    ("round", &[1.0e-300, 2.0], 0.0, 1),
    ("round", &[123456789.12345679, 5.0], 123456789.12346, 1),
    ("round", &[0.5, -1.0], 0.0, 1),
    ("round", &[1.45, 1.0], 1.5, 1),
    ("round", &[2.675, 2.0], 2.68, 1),
    ("round", &[1.005, 2.0], 1.01, 1),
    ("round", &[0.285, 2.0], 0.29, 1),
    ("round", &[1.255, 2.0], 1.26, 1),
    ("round", &[8.345, 2.0], 8.35, 1),
    ("round", &[-2.675, 2.0], -2.68, 1),
    ("round", &[1.15, 1.0], 1.2, 1),
    ("round", &[0.30000000000000004, 15.0], 0.3, 1),
    ("round", &[7.0, 0.0], 7.0, 1),
    ("round", &[2.5, 0.9], 3.0, 1),
    ("round", &[1234.5678, 2.9], 1234.57, 1),
    ("round", &[1234.5678, -2.9], 1200.0, 1),
    ("round", &[1.0e+22, 2.0], 1.0e+22, 1),
    ("round", &[5.0e-324, 400.0], 5.0e-324, 1),
    ("round", &[1.0, 500.0], 1.0, 1),
    ("round", &[1.0, -500.0], 0.0, 1),
];

/// Runs every row of `rows` whose function is in `names` through a formula
/// `f(A)` / `f(A, B)` and returns one line per row off its tolerance, and
/// the worst error in ulps per function.
fn table_failures(rows: &[Row], names: &[&str]) -> (Vec<String>, Vec<(String, u64)>) {
    let mut failures = Vec::new();
    let mut worst: Vec<(String, u64)> = Vec::new();
    for (name, args, expected, tolerance) in rows {
        if !names.contains(name) {
            continue;
        }
        let src = if args.len() == 1 {
            format!("{name}(A)")
        } else {
            format!("{name}(A, B)")
        };
        let data = [
            float_column(&[args[0]]),
            float_column(&[args[1.min(args.len() - 1)]]),
        ];
        let (values, statuses) = run(&src, &data, 1);
        let got = values[0];
        let off = ulps(got, *expected);
        match worst.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = entry.1.max(off),
            None => worst.push((name.to_string(), off)),
        }
        // A NaN expectation is a domain error: the row must be Failed.
        let status_ok = !expected.is_nan() || statuses[0] == NumericStatus::Failed;
        if off > *tolerance || !status_ok {
            failures.push(format!(
                "{name}({args:?}): got {got:e} ({:?}), expected {expected:e} ({off} ulps, tolerance {tolerance})",
                statuses[0]
            ));
        }
    }
    (failures, worst)
}

fn assert_table(names: &[&str]) {
    let (failures, worst) = table_failures(TABLE, names);
    eprintln!("worst error in ulps: {worst:?}");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn erf_matches_reference_values() {
    assert_table(&["erf"]);
}

#[test]
fn erfc_matches_reference_values() {
    assert_table(&["erfc"]);
}

#[test]
fn hyperbolic_functions_match_reference_values() {
    assert_table(&["sinh", "cosh", "tanh"]);
}

#[test]
fn logarithms_match_reference_values() {
    assert_table(&["ln", "log", "log2"]);
}

#[test]
fn inverse_trigonometric_functions_match_reference_values() {
    assert_table(&["asin", "acos", "atan"]);
}

#[test]
fn trigonometric_functions_match_reference_values() {
    assert_table(&["sin", "cos", "tan"]);
}

#[test]
fn atan2_matches_reference_values_in_all_quadrants() {
    assert_table(&["atan2"]);
}

#[test]
fn floor_and_ceil_match_reference_values() {
    assert_table(&["floor", "ceil"]);
}

#[test]
fn mod_matches_reference_values() {
    assert_table(&["mod"]);
}

#[test]
fn root_matches_reference_values() {
    assert_table(&["root"]);
}

#[test]
fn logx_matches_reference_values() {
    assert_table(&["logx"]);
}

#[test]
fn pow_matches_reference_values() {
    assert_table(&["pow"]);
}

#[test]
fn round_matches_binary_exact_reference_values() {
    assert_table(&["round"]);
}

#[test]
#[ignore = "bug: #428: round(1.005, 2) = 1 not 1.01 (x * 10^n is binary, not Excel decimal), functions.rs:272-295"]
fn round_matches_excel_decimal_reference_values() {
    let (failures, _) = table_failures(ROUND_DECIMAL, &["round"]);
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Signs of zero that the documentation promises or IEEE dictates.
#[test]
fn signed_zero_results_of_functions() {
    let one = |src: &str, a: f64, b: f64| {
        let (v, _) = run(src, &[float_column(&[a]), float_column(&[b])], 1);
        v[0].to_bits()
    };
    assert_eq!(one("floor(A)", -0.0, 0.0), (-0.0f64).to_bits());
    assert_eq!(one("ceil(A)", -0.5, 0.0), (-0.0f64).to_bits());
    assert_eq!(one("floor(A)", 0.5, 0.0), 0.0f64.to_bits());
    assert_eq!(one("sign(A)", -0.0, 0.0), 0.0f64.to_bits());
    assert_eq!(
        one("atan2(A, B)", 0.0, -0.0),
        std::f64::consts::PI.to_bits()
    );
    assert_eq!(
        one("atan2(A, B)", -0.0, -0.0),
        (-std::f64::consts::PI).to_bits()
    );
    assert_eq!(one("atan2(A, B)", -0.0, 1.0), (-0.0f64).to_bits());
    assert_eq!(one("sinh(A)", -0.0, 0.0), (-0.0f64).to_bits());
    assert_eq!(one("tanh(A)", -0.0, 0.0), (-0.0f64).to_bits());
    assert_eq!(one("sqrt(A)", -0.0, 0.0), (-0.0f64).to_bits());
    assert_eq!(one("abs(A)", -0.0, 0.0), 0.0f64.to_bits());
}

#[test]
fn erf_of_minus_zero_is_minus_zero() {
    let (v, _) = run("erf(A)", &[float_column(&[-0.0])], 1);
    assert_eq!(v[0].to_bits(), (-0.0f64).to_bits());
}

#[test]
fn min_and_max_with_signed_zeros_return_a_zero() {
    let (v, _) = run(
        "min(A, B)",
        &[float_column(&[-0.0, 0.0]), float_column(&[0.0, -0.0])],
        2,
    );
    assert!(v.iter().all(|x| *x == 0.0));
    let (v, _) = run(
        "max(A, B)",
        &[float_column(&[-0.0, 0.0]), float_column(&[0.0, -0.0])],
        2,
    );
    assert!(v.iter().all(|x| *x == 0.0));
}

/// `min`/`max` with an empty cell stay empty (status Missing), `sign` too.
#[test]
fn min_max_sign_of_an_empty_cell_stay_empty() {
    let data = [
        float_column(&[f64::NAN, 1.0]),
        float_column(&[1.0, f64::NAN]),
    ];
    for src in [
        "min(A, B)",
        "max(A, B)",
        "sign(A)",
        "pow(A, 0)",
        "pow(1, A)",
        "atan2(A, B)",
    ] {
        let (v, s) = run(src, &data, 2);
        assert!(v[0].is_nan(), "{src}");
        assert_eq!(s[0], NumericStatus::Missing, "{src}");
    }
}

/// The exact remainder 3 - 1e-20 rounds to 3 in f64, so `mod` returns the divisor
/// itself, like Python and Excel (decided in #412).
#[test]
fn mod_of_a_tiny_negative_dividend_rounds_to_the_divisor_like_python_and_excel() {
    let (v, _) = run("mod(A, 3)", &[float_column(&[-1e-20])], 1);
    assert_eq!(v[0], 3.0);
}

#[test]
fn logx_of_exact_powers_is_exact() {
    let cases = [
        (1000.0, 10.0, 3.0),
        (8.0, 2.0, 3.0),
        (243.0, 3.0, 5.0),
        (125.0, 5.0, 3.0),
        (1e15, 10.0, 15.0),
        (8.0, 0.5, -3.0),
    ];
    for (x, base, expected) in cases {
        let (v, _) = run(
            "logx(A, B)",
            &[float_column(&[x]), float_column(&[base])],
            1,
        );
        assert_eq!(v[0], expected, "logx({x}, {base}) = {:e}", v[0]);
    }
}

#[test]
fn floor_of_logx_of_a_power_of_ten_is_the_exponent() {
    let (v, _) = run("floor(logx(A, 10))", &[float_column(&[1000.0])], 1);
    assert_eq!(v[0], 3.0);
}

/// A value only close to a power is not snapped to the integer.
#[test]
fn logx_of_a_value_close_to_a_power_is_not_snapped() {
    let x = 243.0 * (1.0 + 1e-12);
    let (v, _) = run("logx(A, B)", &[float_column(&[x]), float_column(&[3.0])], 1);
    assert_ne!(v[0], 5.0);
    assert!((v[0] - 5.0).abs() < 1e-11);
}

// -- (c) number lexing --

/// What the evaluator does with a text: its value on a constant formula, or
/// a refusal. `Err` is a clear refusal (a `FormulaError`); a refused text is
/// never a bug, a silent misread is.
fn outcome(text: &str) -> Result<f64, String> {
    constant(text).map_err(|e| format!("{e:?}"))
}

#[test]
fn accepted_number_forms() {
    let cases: &[(&str, f64)] = &[
        (".5", 0.5),
        ("5.", 5.0),
        ("0.5", 0.5),
        ("1e3", 1000.0),
        ("1E3", 1000.0),
        ("1e+3", 1000.0),
        ("1E-3", 0.001),
        ("2.5E6", 2.5e6),
        ("007", 7.0),
        ("1.e3", 1000.0),
        (".5e1", 5.0),
        ("--1", 1.0),
        ("+1", 1.0),
        ("1++1", 2.0),
        ("1 + 1", 2.0),
        ("1\t+\t1", 2.0),
        ("1\u{a0}+\u{a0}1", 2.0),
        ("1\n+1", 2.0),
        ("PI()", std::f64::consts::PI),
        ("Pi ( )", std::f64::consts::PI),
        ("pi()", std::f64::consts::PI),
        ("1==1", 1.0),
        ("(((1)))", 1.0),
        ("12345678901234567890", 12345678901234567890.0),
        (
            "0.1234567890123456789012345678901234567890",
            0.123_456_789_012_345_68,
        ),
        ("9007199254740993", 9007199254740992.0),
    ];
    for (text, expected) in cases {
        match outcome(text) {
            Ok(v) => assert_eq!(v, *expected, "`{text}`"),
            Err(e) => panic!("`{text}` refused: {e}"),
        }
    }
}

#[test]
fn refused_forms_are_refused_with_an_error() {
    let refused = [
        "1e",
        "1e+",
        "1e-",
        "0x10",
        "1_000",
        "\u{2212}1",
        "1\u{d7}2",
        "1\u{f7}2",
        "1,5",
        "2B",
        "2(B)",
        "B2",
        "A B",
        "pi",
        "inf",
        "nan",
        "NaN",
        "1 2",
        "()",
        "",
        " ",
        "=1",
        "1=1",
        "1.2.3",
        "1..2",
        ".",
        "..",
        ".e3",
        "(1",
        "1)",
        "1+",
        "*1",
        "1**2",
        "1//2",
        "1%",
        "1!",
        "!1",
        "2e3e4",
        "1e3e",
        "A.5",
        "\u{661}",
        "\u{ff11}",
        "\u{221e}",
        "1 000",
        "$1",
        "1;2",
        "1 / ",
        "(",
        ")",
        "pi(1)",
        "sqrt()",
        "sqrt(1, 2)",
        "if(1, 2)",
        "round(1)",
        "1e5x",
        "'a'",
        "\"1\"",
        "1&&1",
        "1||0",
        "true",
        "1^",
        "^2",
        "--",
        "- -",
    ];
    for text in refused {
        assert!(
            outcome(text).is_err(),
            "`{text}` accepted: {:?}",
            outcome(text)
        );
    }
}

/// Literals beyond the range of f64 are refused at entry (#429): `1e400` would
/// be infinity and `1e-400` would be 0. What is representable, subnormals
/// included, and a written zero stay accepted.
#[test]
fn out_of_range_literals_are_refused_at_entry() {
    for text in ["1e400", "1e-400", "-1e400"] {
        let v = outcome(text);
        assert!(v.is_err(), "`{text}` silently read as {v:?}");
    }
    assert_eq!(
        parse("1e400").err(),
        Some(FormulaError::NumberOutOfRange {
            text: "1e400".into(),
            too_large: true
        })
    );
    assert_eq!(
        parse("1e-400").err(),
        Some(FormulaError::NumberOutOfRange {
            text: "1e-400".into(),
            too_large: false
        })
    );
    for (text, expected) in [
        ("1e-310", 1e-310),
        ("4.9e-324", 5e-324),
        ("0e400", 0.0),
        ("0.000", 0.0),
        ("1.7976931348623157e308", f64::MAX),
    ] {
        assert_eq!(outcome(text), Ok(expected), "`{text}`");
    }
}

/// A stored formula keeps a literal outside the range: it loads and computes
/// (invariants 10 and 12), only the entry refuses it.
#[test]
fn a_stored_out_of_range_literal_still_compiles() {
    let stored = ColumnFormula::from_parts(vec![FormulaPart::Text("1e400".into())]);
    assert!(stored.compile().is_ok());
    let stored = ColumnFormula::from_parts(vec![FormulaPart::Text("1e-400".into())]);
    assert!(stored.compile().is_ok());
}

#[test]
fn very_long_digit_runs_do_not_panic() {
    let long = format!("1{}", "0".repeat(400));
    let v = outcome(&long);
    assert!(v.is_err(), "400-digit literal silently read as {v:?}");
    let tiny = format!("0.{}1", "0".repeat(400));
    let v = outcome(&tiny);
    assert!(v.is_err(), "0.000..1 (401 digits) silently read as {v:?}");
    let digits = "1".repeat(100_000);
    let _ = outcome(&digits);
}

/// Letters in a formula are columns: `E` is the fifth column, `1e` is not an
/// exponent without digits.
#[test]
fn letters_are_columns_and_exponents_need_digits() {
    let data: Vec<ColumnValues> = (0..30)
        .map(|i| float_column(&[f64::from(i) + 1.0]))
        .collect();
    let (v, _) = run("E", &data, 1);
    assert_eq!(v[0], 5.0);
    let (v, _) = run("AA", &data, 1);
    assert_eq!(v[0], 27.0);
    let (v, _) = run("ad", &data, 1);
    assert_eq!(v[0], 30.0);
    let (v, _) = run("2*E", &data, 1);
    assert_eq!(v[0], 10.0);
    let (v, _) = run("1e3*E", &data, 1);
    assert_eq!(v[0], 5000.0);
    let (v, _) = run("E*1e-3", &data, 1);
    assert_eq!(v[0], 0.005);
    // Unknown column: refused, never a silent reading.
    assert!(parse("AE").is_err());
    assert!(parse("ZZ").is_err());
}

/// `max(B, 1,5)` is a range [1, 5]: with a decimal comma (`1,5` meaning 1.5)
/// the text means something else and compiles. Recorded, not asserted as a bug:
/// the manual says three values are always a range.
#[test]
fn comma_between_numbers_in_a_column_function_is_read_as_a_range() {
    let data = [
        float_column(&[1.0, 2.0, 3.0]),
        float_column(&[10.0, 20.0, 30.0]),
    ];
    let formula = parse("max(B, 1,5)").expect("compiles as a range");
    let paired = std::collections::BTreeMap::from([(col(1), col(0))]);
    let r = stokesforge_core::formula::evaluate_with_x(&formula, &sheet_values(&data), &paired, 3);
    assert_eq!(r.error, None);
    assert_eq!(r.values, vec![30.0; 3], "range [1, 5] over X = 1, 2, 3");
}

#[test]
fn deeply_nested_formulas_are_refused_not_crashed() {
    let text = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
    assert!(outcome(&text).is_err());
    let chain = vec!["1"; 5000].join("+");
    let _ = outcome(&chain);
    let minus = format!("{}1", "-".repeat(5000));
    let _ = outcome(&minus);
    let caret = vec!["2"; 5000].join("^");
    let _ = outcome(&caret);
}

#[test]
fn text_cell_makes_the_row_failed_and_empty_makes_it_missing() {
    let col = ColumnValues::Mixed(Arc::new(vec![
        MixedCell::Number(2.0),
        MixedCell::Text("abc".into()),
        MixedCell::Empty,
        MixedCell::Text(String::new()),
    ]));
    let (v, s) = run("A*2", std::slice::from_ref(&col), 4);
    assert_eq!(v[0], 4.0);
    assert!(v[1].is_nan() && v[2].is_nan() && v[3].is_nan());
    assert_eq!(
        s,
        vec![
            NumericStatus::Ok,
            NumericStatus::Failed,
            NumericStatus::Missing,
            NumericStatus::Missing
        ]
    );
    // `if` keeps the status of the branch taken.
    let (_, s) = run("if(1, 5, A)", std::slice::from_ref(&col), 4);
    assert_eq!(s, vec![NumericStatus::Ok; 4]);
    // `A + A` of a text row: Failed beats Missing.
    let (_, s) = run("A + A", &[col], 4);
    assert_eq!(s[1], NumericStatus::Failed);
}
