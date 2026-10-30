//! The **one shared table of scalar functions** (#401): `sqrt`, `ln`, `round`,
//! `mod`... used by the free-model parser of the Fit ([`crate::expr`]) and by
//! the column formulas of `stokesforge-core::formula` (`F(x)=`).
//!
//! A function takes a few `f64` and returns one: no column, no status, no
//! dependency. An invalid domain gives `NaN`, never a panic. Names are looked
//! up **without regard to case** (`EXP` = `exp`); the arity is a **set** of
//! allowed counts ([`Func::arity`]), so that a function can later accept
//! `max(b)` and `max(b, x1, x2)` (#402): the column forms are not in this
//! table (they read a whole column, see [`crate::column_values`] and the
//! `ColumnFunc` callees of the formula); `Func::Min`/`Max` stay the
//! two-argument, row-wise functions. `if` and the comparisons are not here:
//! they need the row status and live in the formula only.
//!
//! [`Func::ALL`] lists every function once (aliases such as `log10` are not
//! entries): the column-formula help of the application is built from it
//! (#404), so a function added here shows up there. [`Func::alternative`]
//! names the function to write instead when the argument count is wrong
//! (`log(B, 2)` -> `logx`), plain data for the error message.
//!
//! The transcendental functions go through `libm`, never through the
//! platform `f64` methods (`clippy.toml`, #332).

/// A function of the shared table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func {
    Exp,
    Ln,
    Log10,
    Log2,
    Logx,
    Sqrt,
    Root,
    Abs,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Erf,
    Erfc,
    Sign,
    Floor,
    Ceil,
    Round,
    Mod,
    Pow,
    Min,
    Max,
    Pi,
}

impl Func {
    /// Every function of the table, once each, in help order (family by
    /// family). A test keeps it complete.
    pub const ALL: [Func; 29] = [
        Func::Exp,
        Func::Ln,
        Func::Log10,
        Func::Log2,
        Func::Logx,
        Func::Sqrt,
        Func::Root,
        Func::Pow,
        Func::Abs,
        Func::Sign,
        Func::Sin,
        Func::Cos,
        Func::Tan,
        Func::Asin,
        Func::Acos,
        Func::Atan,
        Func::Atan2,
        Func::Sinh,
        Func::Cosh,
        Func::Tanh,
        Func::Erf,
        Func::Erfc,
        Func::Floor,
        Func::Ceil,
        Func::Round,
        Func::Mod,
        Func::Min,
        Func::Max,
        Func::Pi,
    ];

    /// The function to write instead when `self` is called with `found`
    /// arguments it does not accept: `log`/2 -> `logx`, `atan`/2 -> `atan2`,
    /// `sqrt`/2 -> `root`. `None` for any other case.
    pub fn alternative(self, found: usize) -> Option<Func> {
        match (self, found) {
            (Func::Log10, 2) => Some(Func::Logx),
            (Func::Atan, 2) => Some(Func::Atan2),
            (Func::Sqrt, 2) => Some(Func::Root),
            _ => None,
        }
    }

    /// The function called `name`, case-insensitive; `None` if unknown.
    pub fn from_name(name: &str) -> Option<Func> {
        Some(match name.to_ascii_lowercase().as_str() {
            "exp" => Func::Exp,
            "ln" => Func::Ln,
            // `log` is in base 10 (chemists' usage, #287); `logx(x, b)`
            // takes the base as second argument.
            "log" | "log10" => Func::Log10,
            "log2" => Func::Log2,
            "logx" => Func::Logx,
            "sqrt" => Func::Sqrt,
            "root" => Func::Root,
            "abs" => Func::Abs,
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "asin" => Func::Asin,
            "acos" => Func::Acos,
            "atan" => Func::Atan,
            "atan2" => Func::Atan2,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" => Func::Tanh,
            "erf" => Func::Erf,
            "erfc" => Func::Erfc,
            "sign" => Func::Sign,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            "round" => Func::Round,
            "mod" => Func::Mod,
            "pow" => Func::Pow,
            "min" => Func::Min,
            "max" => Func::Max,
            "pi" => Func::Pi,
            _ => return None,
        })
    }

    /// The canonical (lowercase) name, as shown in messages.
    pub fn name(self) -> &'static str {
        match self {
            Func::Exp => "exp",
            Func::Ln => "ln",
            Func::Log10 => "log",
            Func::Log2 => "log2",
            Func::Logx => "logx",
            Func::Sqrt => "sqrt",
            Func::Root => "root",
            Func::Abs => "abs",
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Asin => "asin",
            Func::Acos => "acos",
            Func::Atan => "atan",
            Func::Atan2 => "atan2",
            Func::Sinh => "sinh",
            Func::Cosh => "cosh",
            Func::Tanh => "tanh",
            Func::Erf => "erf",
            Func::Erfc => "erfc",
            Func::Sign => "sign",
            Func::Floor => "floor",
            Func::Ceil => "ceil",
            Func::Round => "round",
            Func::Mod => "mod",
            Func::Pow => "pow",
            Func::Min => "min",
            Func::Max => "max",
            Func::Pi => "pi",
        }
    }

    /// The allowed argument counts (a set, ascending).
    pub fn arity(self) -> &'static [usize] {
        match self {
            Func::Pi => &[0],
            Func::Logx
            | Func::Root
            | Func::Atan2
            | Func::Round
            | Func::Mod
            | Func::Pow
            | Func::Min
            | Func::Max => &[2],
            _ => &[1],
        }
    }

    /// Value of the function. `args` is expected to have an allowed count
    /// ([`Func::arity`], checked by the parsers); a missing argument reads as
    /// `NaN`, never a panic.
    pub fn apply(self, args: &[f64]) -> f64 {
        let a = args.first().copied().unwrap_or(f64::NAN);
        let b = args.get(1).copied().unwrap_or(f64::NAN);
        match self {
            Func::Exp => libm::exp(a),
            Func::Ln => libm::log(a),
            Func::Log10 => libm::log10(a),
            Func::Log2 => libm::log2(a),
            Func::Sqrt => a.sqrt(),
            Func::Abs => a.abs(),
            Func::Sin => libm::sin(a),
            Func::Cos => libm::cos(a),
            Func::Tan => libm::tan(a),
            Func::Asin => libm::asin(a),
            Func::Acos => libm::acos(a),
            Func::Atan => libm::atan(a),
            Func::Sinh => libm::sinh(a),
            Func::Cosh => libm::cosh(a),
            Func::Tanh => libm::tanh(a),
            Func::Erf => erf(a),
            Func::Erfc => erfc(a),
            // `f64::min`/`max` ignore a NaN and `sign` would read it as 0:
            // a NaN operand must stay NaN (contract "invalid domain").
            Func::Sign => {
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
            Func::Floor => a.floor(),
            Func::Ceil => a.ceil(),
            Func::Pow => libm::pow(a, b),
            Func::Min if a.is_nan() || b.is_nan() => f64::NAN,
            Func::Max if a.is_nan() || b.is_nan() => f64::NAN,
            Func::Min => a.min(b),
            Func::Max => a.max(b),
            Func::Atan2 => libm::atan2(a, b),
            // Base ≤ 0 or = 1: NaN (ln b = 0 would give ±inf or 0/0); x ≤ 0:
            // `ln` already gives NaN (−inf for 0, which we keep like `ln`).
            Func::Logx if b.is_nan() || b <= 0.0 || b == 1.0 => f64::NAN,
            Func::Logx => libm::log(a) / libm::log(b),
            Func::Root => root(a, b),
            Func::Round => round(a, b),
            Func::Mod => modulo(a, b),
            Func::Pi => std::f64::consts::PI,
        }
    }
}

/// `n`-th root. Odd integer `n` with `x < 0` keeps the real root
/// (`root(-8, 3) = -2`); any other negative `x`, and `n = 0`, give `NaN`.
fn root(x: f64, n: f64) -> f64 {
    if x.is_nan() || n.is_nan() || n == 0.0 {
        return f64::NAN;
    }
    if x < 0.0 {
        let odd_integer = n.is_finite() && n.fract() == 0.0 && libm::fmod(n, 2.0).abs() == 1.0;
        return if odd_integer { -root(-x, n) } else { f64::NAN };
    }
    if n == 2.0 {
        x.sqrt()
    } else if n == 3.0 {
        libm::cbrt(x)
    } else {
        libm::pow(x, 1.0 / n)
    }
}

/// Rounding to `n` decimals like Excel: half away from zero, `n` negative
/// allowed (`round(1234, -2) = 1200`), `n` truncated toward zero.
fn round(x: f64, n: f64) -> f64 {
    if x.is_nan() || n.is_nan() {
        return f64::NAN;
    }
    let n = n.trunc();
    if !x.is_finite() || n > 400.0 {
        return x;
    }
    if n < -400.0 {
        return 0.0;
    }
    if n >= 0.0 {
        let factor = libm::pow(10.0, n);
        let scaled = x * factor;
        if !scaled.is_finite() {
            return x;
        }
        libm::round(scaled) / factor
    } else {
        // Divide then multiply back: exact for 1234 -> 1200 (0.01 is not).
        let factor = libm::pow(10.0, -n);
        libm::round(x / factor) * factor
    }
}

/// Remainder with the **sign of the divisor** (Python, Excel):
/// `mod(-7, 3) = 2`. A zero divisor gives `NaN`.
fn modulo(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() || b == 0.0 {
        return f64::NAN;
    }
    let r = libm::fmod(a, b);
    if r != 0.0 && (r < 0.0) != (b < 0.0) {
        r + b
    } else {
        r
    }
}

/// Complementary error function, precise in the tail: `1 − erf(x)` would lose
/// all its digits from `x ≈ 6` (`erfc(6) ≈ 2e-17`).
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < 2.5 {
        return 1.0 - erf(x);
    }
    if x > 27.0 {
        return 0.0;
    }
    // Same continued fraction as `erf`, without subtracting it from 1.
    let mut f = 0.0;
    for k in (1..=60).rev() {
        f = (k as f64 / 2.0) / (x + f);
    }
    libm::exp(-x * x) / (std::f64::consts::PI.sqrt() * (x + f))
}

/// Error function (Abramowitz & Stegun 7.1.26 refined by a series for
/// small arguments): absolute error < 1e-12 on ℝ.
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let ax = x.abs();
    let v = if ax < 3.0 {
        // Maclaurin series: erf(x) = 2/√π Σ (-1)^n x^(2n+1) / (n! (2n+1)).
        let mut term = ax;
        let mut sum = ax;
        let x2 = ax * ax;
        let mut n = 0.0_f64;
        loop {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() < 1e-17 * sum.abs() || n > 200.0 {
                break;
            }
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else if ax > 6.0 {
        1.0
    } else {
        // Continued fraction of erfc for large arguments.
        let mut f = 0.0;
        for k in (1..=60).rev() {
            f = (k as f64 / 2.0) / (ax + f);
        }
        1.0 - libm::exp(-ax * ax) / (std::f64::consts::PI.sqrt() * (ax + f))
    };
    if x < 0.0 {
        -v
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: &[f64]) -> f64 {
        Func::from_name(name).expect(name).apply(args)
    }

    /// Position of each variant: an exhaustive `match`, so that a new
    /// variant does not compile until it is counted here (and so listed in
    /// [`Func::ALL`]).
    fn index(f: Func) -> usize {
        match f {
            Func::Exp => 0,
            Func::Ln => 1,
            Func::Log10 => 2,
            Func::Log2 => 3,
            Func::Logx => 4,
            Func::Sqrt => 5,
            Func::Root => 6,
            Func::Abs => 7,
            Func::Sin => 8,
            Func::Cos => 9,
            Func::Tan => 10,
            Func::Asin => 11,
            Func::Acos => 12,
            Func::Atan => 13,
            Func::Atan2 => 14,
            Func::Sinh => 15,
            Func::Cosh => 16,
            Func::Tanh => 17,
            Func::Erf => 18,
            Func::Erfc => 19,
            Func::Sign => 20,
            Func::Floor => 21,
            Func::Ceil => 22,
            Func::Round => 23,
            Func::Mod => 24,
            Func::Pow => 25,
            Func::Min => 26,
            Func::Max => 27,
            Func::Pi => 28,
        }
    }

    #[test]
    fn all_lists_every_function_once_and_each_name_round_trips() {
        let mut seen = [0usize; 29];
        for f in Func::ALL {
            seen[index(f)] += 1;
            assert_eq!(Func::from_name(f.name()), Some(f), "{}", f.name());
        }
        assert!(seen.iter().all(|&n| n == 1), "{seen:?}");
    }

    #[test]
    fn the_alternative_is_plain_data_for_three_known_mistakes() {
        assert_eq!(Func::Log10.alternative(2), Some(Func::Logx));
        assert_eq!(Func::Atan.alternative(2), Some(Func::Atan2));
        assert_eq!(Func::Sqrt.alternative(2), Some(Func::Root));
        assert_eq!(Func::Exp.alternative(2), None);
        assert_eq!(Func::Sqrt.alternative(3), None);
    }

    #[test]
    fn names_are_case_insensitive_and_unknown_is_none() {
        assert_eq!(Func::from_name("EXP"), Some(Func::Exp));
        assert_eq!(Func::from_name("LogX"), Some(Func::Logx));
        assert_eq!(Func::from_name("log"), Func::from_name("log10"));
        assert_eq!(Func::from_name("foo"), None);
        assert_eq!(Func::from_name(""), None);
    }

    #[test]
    fn arity_is_a_set_of_counts() {
        assert_eq!(Func::Pi.arity(), &[0]);
        assert_eq!(Func::Sqrt.arity(), &[1]);
        assert_eq!(Func::Mod.arity(), &[2]);
    }

    #[test]
    fn mod_takes_the_sign_of_the_divisor() {
        assert_eq!(call("mod", &[-7.0, 3.0]), 2.0);
        assert_eq!(call("mod", &[7.0, 3.0]), 1.0);
        assert_eq!(call("mod", &[7.0, -3.0]), -2.0);
        assert_eq!(call("mod", &[-6.0, 3.0]), 0.0);
        assert!(call("mod", &[1.0, 0.0]).is_nan());
    }

    #[test]
    fn round_is_excel_like() {
        assert_eq!(call("round", &[-2.5, 0.0]), -3.0);
        assert_eq!(call("round", &[2.5, 0.0]), 3.0);
        assert_eq!(call("round", &[7.14159, 2.0]), 7.14);
        assert_eq!(call("round", &[1234.0, -2.0]), 1200.0);
        // A non-integer `n` is truncated toward zero.
        assert_eq!(call("round", &[7.14159, 2.9]), 7.14);
        assert_eq!(call("round", &[1250.0, -2.7]), 1300.0);
        assert!(call("round", &[f64::NAN, 1.0]).is_nan());
    }

    #[test]
    fn root_keeps_the_real_odd_root() {
        assert_eq!(call("root", &[-8.0, 3.0]), -2.0);
        assert_eq!(call("root", &[8.0, 3.0]), 2.0);
        assert!(call("root", &[-8.0, 2.0]).is_nan());
        assert!(call("root", &[-8.0, 0.5]).is_nan());
        assert!(call("root", &[8.0, 0.0]).is_nan());
        assert!((call("root", &[-32.0, 5.0]) + 2.0).abs() < 1e-12);
    }

    #[test]
    fn pi_takes_no_argument_and_a_missing_one_is_nan() {
        assert_eq!(call("pi", &[]), std::f64::consts::PI);
        assert!(Func::Sqrt.apply(&[]).is_nan());
    }

    #[test]
    fn invalid_domains_give_nan() {
        assert!(call("sqrt", &[-1.0]).is_nan());
        assert!(call("ln", &[-1.0]).is_nan());
        assert!(call("logx", &[8.0, 1.0]).is_nan());
        assert!(call("min", &[f64::NAN, 1.0]).is_nan());
        assert_eq!(call("logx", &[8.0, 2.0]).round(), 3.0);
    }
}
