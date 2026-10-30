//! Parser for **free model** expressions (#188, M13 a B): `y = …` typed
//! by the user, with `x`, free parameters and functions.
//!
//! Choice: the parser lives here and not in `stokesforge-core::formula` (the
//! column formulas, which read columns, not parameters) — this crate stays
//! free of workspace dependencies. The two **grammars** stay separate, but
//! the **function table** is shared ([`crate::functions`], #401): a function
//! added there is available in both. Names of functions are case-insensitive
//! here too (`EXP(x)`); parameters stay case-sensitive. `if` and the
//! comparisons of the column formula are not in the Fit.
//!
//! Grammar (increasing precedence): `+ -`; `* /`; unary `-`; `^`
//! (right-associative, `-2^2 = -4`); call `f(a, b)`, number, name,
//! parentheses. The names `x`, `pi`, `e` are reserved (`pi()` also works);
//! any other name outside a call is a **parameter**, in the order of its
//! first appearance.
//! Implicit multiplication (`2x`) does not exist.
//!
//! Logarithms (#287): `log(x)` and `log10(x)` in base 10, `ln(x)` natural,
//! `log2(x)`, `logx(x, b)` in base `b`; x < 0, `b ≤ 0` or `b = 1` give NaN.

use std::fmt;

use crate::functions::Func;
pub use crate::functions::{erf, erfc};

/// A syntax error, with the position (in characters, from 0).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{kind} at position {position}")]
pub struct ExprError {
    pub kind: ExprErrorKind,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprErrorKind {
    Empty,
    UnexpectedChar(char),
    UnexpectedEnd,
    UnexpectedToken,
    UnbalancedParenthesis,
    BadNumber,
    /// Tree depth beyond [`MAX_DEPTH`] (parentheses, signs,
    /// exponents or operator chain too long).
    TooDeep,
    UnknownFunction(String),
    WrongArgumentCount {
        name: String,
        expected: usize,
        found: usize,
    },
}

impl fmt::Display for ExprErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty expression"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character '{c}'"),
            Self::UnexpectedEnd => write!(f, "unexpected end of expression"),
            Self::UnexpectedToken => write!(f, "unexpected token"),
            Self::UnbalancedParenthesis => write!(f, "unbalanced parenthesis"),
            Self::BadNumber => write!(f, "malformed number"),
            Self::TooDeep => write!(f, "expression nested too deeply"),
            Self::UnknownFunction(n) => write!(f, "unknown function '{n}'"),
            Self::WrongArgumentCount {
                name,
                expected,
                found,
            } => write!(f, "'{name}' takes {expected} argument(s), {found} given"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Number(f64),
    X,
    Param(usize),
    Neg(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

/// A compiled expression: parameters resolved to indices.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Node,
    params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            let mut seen_e = false;
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_digit() || d == '.' {
                    i += 1;
                } else if (d == 'e' || d == 'E') && !seen_e {
                    // Exponent only if followed by a digit (with optional
                    // sign): `2e` stays `2` then the name `e`.
                    let mut j = i + 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        seen_e = true;
                        i = j;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let v: f64 = text.parse().map_err(|_| ExprError {
                kind: ExprErrorKind::BadNumber,
                position: start,
            })?;
            out.push((Tok::Num(v), start));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((Tok::Ident(chars[start..i].iter().collect()), start));
        } else if "+-*/^".contains(c) {
            out.push((Tok::Op(c), i));
            i += 1;
        } else if c == '(' {
            out.push((Tok::LParen, i));
            i += 1;
        } else if c == ')' {
            out.push((Tok::RParen, i));
            i += 1;
        } else if c == ',' {
            out.push((Tok::Comma, i));
            i += 1;
        } else {
            return Err(ExprError {
                kind: ExprErrorKind::UnexpectedChar(c),
                position: i,
            });
        }
    }
    Ok(out)
}

/// Cap on tree depth, in "cost": one nesting level
/// (parenthesis, call, sign, exponent) costs [`NEST_COST`], one term of a
/// chain `a+b+c…` or `a*b*c…` costs 1. The parser, `eval_node` and the `Drop`
/// of the tree recurse by that much: bounded here, none can exceed the stack
/// of a 2 MiB thread, even in debug (`docs/archive/rustrewrite/
/// questions-ouvertes.md` §11, #279). Allows 100 nestings or 1 000 terms.
pub const MAX_DEPTH: usize = 1000;
const NEST_COST: usize = 10;

struct Parser {
    toks: Vec<(Tok, usize)>,
    depth: usize,
    pos: usize,
    end: usize,
    params: Vec<String>,
}

impl Parser {
    fn err(&self, kind: ExprErrorKind) -> ExprError {
        let position = self.toks.get(self.pos).map_or(self.end, |t| t.1);
        ExprError { kind, position }
    }

    /// Adds `cost` to the current depth; beyond the cap, `TooDeep`.
    fn charge(&mut self, cost: usize) -> Result<(), ExprError> {
        self.depth += cost;
        if self.depth > MAX_DEPTH {
            return Err(self.err(ExprErrorKind::TooDeep));
        }
        Ok(())
    }

    /// Runs `f` one nesting level lower, then gives the depth back.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ExprError>,
    ) -> Result<T, ExprError> {
        self.charge(NEST_COST)?;
        let out = f(self)?;
        self.depth -= NEST_COST;
        Ok(out)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn expr(&mut self) -> Result<Node, ExprError> {
        let base = self.depth;
        let mut left = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            self.charge(1)?;
            let right = self.term()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        self.depth = base;
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let base = self.depth;
        let mut left = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.pos += 1;
            self.charge(1)?;
            let right = self.unary()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        self.depth = base;
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Ok(Node::Neg(Box::new(self.nested(Self::unary)?)))
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.nested(Self::unary)
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Node, ExprError> {
        let base = self.atom()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            // Right-associative; the exponent may carry a sign (`2^-3`).
            let exp = self.nested(Self::unary)?;
            return Ok(Node::Bin('^', Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Node, ExprError> {
        let Some((tok, _)) = self.toks.get(self.pos).cloned() else {
            return Err(self.err(ExprErrorKind::UnexpectedEnd));
        };
        match tok {
            Tok::Num(v) => {
                self.pos += 1;
                Ok(Node::Number(v))
            }
            Tok::LParen => {
                self.pos += 1;
                let inner = self.nested(Self::expr)?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err(self.err(ExprErrorKind::UnbalancedParenthesis)),
                }
            }
            Tok::Ident(name) => {
                let name_pos = self.toks[self.pos].1;
                self.pos += 1;
                if let Some(Tok::LParen) = self.peek() {
                    let Some(func) = Func::from_name(&name) else {
                        return Err(ExprError {
                            kind: ExprErrorKind::UnknownFunction(name),
                            position: name_pos,
                        });
                    };
                    self.pos += 1;
                    let mut args = Vec::new();
                    if let Some(Tok::RParen) = self.peek() {
                        self.pos += 1;
                    } else {
                        loop {
                            args.push(self.nested(Self::expr)?);
                            match self.peek() {
                                Some(Tok::Comma) => self.pos += 1,
                                Some(Tok::RParen) => {
                                    self.pos += 1;
                                    break;
                                }
                                _ => return Err(self.err(ExprErrorKind::UnbalancedParenthesis)),
                            }
                        }
                    }
                    if !func.arity().contains(&args.len()) {
                        return Err(ExprError {
                            kind: ExprErrorKind::WrongArgumentCount {
                                name,
                                expected: func.arity()[0],
                                found: args.len(),
                            },
                            position: name_pos,
                        });
                    }
                    return Ok(Node::Call(func, args));
                }
                Ok(match name.as_str() {
                    "x" => Node::X,
                    "pi" => Node::Number(std::f64::consts::PI),
                    "e" => Node::Number(std::f64::consts::E),
                    _ => {
                        let idx = match self.params.iter().position(|p| *p == name) {
                            Some(i) => i,
                            None => {
                                self.params.push(name);
                                self.params.len() - 1
                            }
                        };
                        Node::Param(idx)
                    }
                })
            }
            _ => Err(self.err(ExprErrorKind::UnexpectedToken)),
        }
    }
}

impl Expr {
    /// Compiles `src`. Surrounding spaces are ignored; an empty text is an
    /// error.
    pub fn parse(src: &str) -> Result<Expr, ExprError> {
        let toks = tokenize(src)?;
        if toks.is_empty() {
            return Err(ExprError {
                kind: ExprErrorKind::Empty,
                position: 0,
            });
        }
        let mut parser = Parser {
            toks,
            depth: 0,
            pos: 0,
            end: src.chars().count(),
            params: Vec::new(),
        };
        let root = parser.expr()?;
        if parser.pos < parser.toks.len() {
            let kind = match parser.peek() {
                Some(Tok::RParen) => ExprErrorKind::UnbalancedParenthesis,
                _ => ExprErrorKind::UnexpectedToken,
            };
            return Err(parser.err(kind));
        }
        Ok(Expr {
            root,
            params: parser.params,
        })
    }

    /// Names of the parameters, in the order of their first appearance.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Value at `x` for the parameters `p` (same order as [`Expr::params`]).
    /// An invalid domain gives `NaN`, never a panic.
    pub fn eval(&self, x: f64, p: &[f64]) -> f64 {
        eval_node(&self.root, x, p)
    }
}

fn eval_node(n: &Node, x: f64, p: &[f64]) -> f64 {
    match n {
        Node::Number(v) => *v,
        Node::X => x,
        Node::Param(i) => p.get(*i).copied().unwrap_or(f64::NAN),
        Node::Neg(a) => -eval_node(a, x, p),
        Node::Bin(op, a, b) => {
            let (a, b) = (eval_node(a, x, p), eval_node(b, x, p));
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => a / b,
                _ => libm::pow(a, b),
            }
        }
        Node::Call(f, args) => {
            // The arity is checked at parse: a few arguments at most. A
            // stack buffer avoids an allocation per point in a fit loop.
            let mut buf = [0.0_f64; 4];
            if args.len() <= buf.len() {
                for (slot, arg) in buf.iter_mut().zip(args) {
                    *slot = eval_node(arg, x, p);
                }
                f.apply(&buf[..args.len()])
            } else {
                let values: Vec<f64> = args.iter().map(|a| eval_node(a, x, p)).collect();
                f.apply(&values)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str, x: f64, p: &[f64]) -> f64 {
        Expr::parse(src).unwrap().eval(x, p)
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(ev("1 + 2 * 3", 0.0, &[]), 7.0);
        assert_eq!(ev("(1 + 2) * 3", 0.0, &[]), 9.0);
        assert_eq!(ev("2 ^ 3 ^ 2", 0.0, &[]), 512.0);
        assert_eq!(ev("-2 ^ 2", 0.0, &[]), -4.0);
        assert_eq!(ev("2 ^ -1", 0.0, &[]), 0.5);
        assert_eq!(ev("10 - 4 - 3", 0.0, &[]), 3.0);
        assert_eq!(ev("8 / 4 / 2", 0.0, &[]), 1.0);
    }

    #[test]
    fn parameters_in_order_of_appearance() {
        let e = Expr::parse("a * exp(-x / tau) + y0 + a").unwrap();
        assert_eq!(e.params(), ["a", "tau", "y0"]);
        let v = e.eval(2.0, &[3.0, 2.0, 1.0]);
        assert!((v - (3.0 * libm::exp(-1.0_f64) + 1.0 + 3.0)).abs() < 1e-15);
    }

    #[test]
    fn constants_and_functions() {
        assert!((ev("pi", 0.0, &[]) - std::f64::consts::PI).abs() < 1e-15);
        assert!((ev("e", 0.0, &[]) - std::f64::consts::E).abs() < 1e-15);
        assert!((ev("ln(e)", 0.0, &[]) - 1.0).abs() < 1e-15);
        assert!((ev("sqrt(x)", 16.0, &[]) - 4.0).abs() < 1e-15);
        assert_eq!(ev("max(x, 3)", 5.0, &[]), 5.0);
        assert_eq!(ev("pow(2, 10)", 0.0, &[]), 1024.0);
        assert!((ev("log10(1000)", 0.0, &[]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn logarithms_base_10_natural_and_chosen_base() {
        // `log` = base 10 (#287), `ln` = natural, `logx(x, b)` = base b.
        assert!((ev("log(100)", 0.0, &[]) - 2.0).abs() < 1e-12);
        assert!((ev("log(1000)", 0.0, &[]) - 3.0).abs() < 1e-12);
        assert!((ev("ln(e)", 0.0, &[]) - 1.0).abs() < 1e-15);
        assert!((ev("logx(8, 2)", 0.0, &[]) - 3.0).abs() < 1e-12);
        assert!((ev("logx(x, 10)", 1000.0, &[]) - 3.0).abs() < 1e-12);
        assert!(ev("logx(-1, 2)", 0.0, &[]).is_nan());
        assert!(ev("logx(x, 2)", -4.0, &[]).is_nan());
        assert!(ev("logx(8, 1)", 0.0, &[]).is_nan());
        assert!(ev("logx(8, 0)", 0.0, &[]).is_nan());
        assert!(ev("logx(8, -2)", 0.0, &[]).is_nan());
        assert!(ev("log(x)", -1.0, &[]).is_nan());
        // Arity: a read error, as for `pow`.
        assert!(matches!(
            Expr::parse("logx(8)").unwrap_err().kind,
            ExprErrorKind::WrongArgumentCount { .. }
        ));
        assert!(matches!(
            Expr::parse("log(8, 2)").unwrap_err().kind,
            ExprErrorKind::WrongArgumentCount { .. }
        ));
    }

    #[test]
    fn scientific_notation_without_confusing_the_constant_e() {
        assert_eq!(ev("1e3", 0.0, &[]), 1000.0);
        assert_eq!(ev("2.5E-2", 0.0, &[]), 0.025);
        // `2e` is not an exponent: error (no implicit multiplication).
        assert!(Expr::parse("2e").is_err());
        assert!(Expr::parse("2 * e").is_ok());
    }

    #[test]
    fn invalid_domain_gives_nan() {
        assert!(ev("sqrt(x)", -1.0, &[]).is_nan());
        assert!(ev("ln(x)", -1.0, &[]).is_nan());
        assert!(ev("1 / x", 0.0, &[]).is_infinite());
    }

    #[test]
    fn sign_min_max_propagate_nan() {
        for f in [
            "sign(sqrt(x))",
            "max(sqrt(x), 0)",
            "min(sqrt(x), 0)",
            "max(0, sqrt(x))",
            "min(0, sqrt(x))",
        ] {
            assert!(ev(f, -1.0, &[]).is_nan(), "{f}");
        }
        // Infinities stay correct.
        assert_eq!(ev("sign(1 / x)", 0.0, &[]), 1.0);
        assert_eq!(ev("sign(-1 / x)", 0.0, &[]), -1.0);
        assert_eq!(ev("max(1 / x, 3)", 0.0, &[]), f64::INFINITY);
        assert_eq!(ev("min(-1 / x, 3)", 0.0, &[]), f64::NEG_INFINITY);
        assert_eq!(ev("min(1 / x, 3)", 0.0, &[]), 3.0);
    }

    #[test]
    fn sign_min_max_nan_in_first_and_second_position() {
        let nan = f64::NAN;
        assert!(Func::Sign.apply(&[nan, 0.0]).is_nan());
        for f in [Func::Min, Func::Max] {
            assert!(f.apply(&[nan, 1.0]).is_nan());
            assert!(f.apply(&[1.0, nan]).is_nan());
            assert!(f.apply(&[nan, nan]).is_nan());
            assert!(f.apply(&[nan, f64::INFINITY]).is_nan());
            assert!(f.apply(&[f64::NEG_INFINITY, nan]).is_nan());
        }
    }

    #[test]
    fn sign_min_max_infinities_zeros_and_equals() {
        let (inf, ninf) = (f64::INFINITY, f64::NEG_INFINITY);
        assert_eq!(Func::Sign.apply(&[inf, 0.0]), 1.0);
        assert_eq!(Func::Sign.apply(&[ninf, 0.0]), -1.0);
        assert_eq!(Func::Sign.apply(&[0.0, 0.0]), 0.0);
        assert_eq!(Func::Sign.apply(&[-0.0, 0.0]), 0.0);
        assert_eq!(Func::Sign.apply(&[1e-300, 0.0]), 1.0);
        assert_eq!(Func::Sign.apply(&[-1e-300, 0.0]), -1.0);
        assert_eq!(Func::Max.apply(&[inf, 3.0]), inf);
        assert_eq!(Func::Max.apply(&[3.0, inf]), inf);
        assert_eq!(Func::Max.apply(&[ninf, 3.0]), 3.0);
        assert_eq!(Func::Max.apply(&[ninf, ninf]), ninf);
        assert_eq!(Func::Min.apply(&[ninf, 3.0]), ninf);
        assert_eq!(Func::Min.apply(&[3.0, ninf]), ninf);
        assert_eq!(Func::Min.apply(&[inf, 3.0]), 3.0);
        assert_eq!(Func::Min.apply(&[inf, inf]), inf);
        assert_eq!(Func::Min.apply(&[2.0, 2.0]), 2.0);
        assert_eq!(Func::Max.apply(&[2.0, 2.0]), 2.0);
        assert_eq!(Func::Min.apply(&[0.0, -0.0]), 0.0);
        assert_eq!(Func::Max.apply(&[-0.0, 0.0]), 0.0);
        // Via the parser.
        assert_eq!(ev("max(1/x, 3)", 0.0, &[]), inf);
        assert_eq!(ev("sign(-1/x)", 0.0, &[]), -1.0);
        assert_eq!(ev("min(-1/x, 3)", 0.0, &[]), ninf);
    }

    #[test]
    fn nested_sign_min_max() {
        // max(min(x,1), sqrt(x)): x = 4 -> max(1,2) = 2; x = 0.25 -> max(0.25,0.5) = 0.5.
        assert_eq!(ev("max(min(x,1),sqrt(x))", 4.0, &[]), 2.0);
        assert_eq!(ev("max(min(x,1),sqrt(x))", 0.25, &[]), 0.5);
        // The NaN of sqrt(-1) goes through min then max.
        assert!(ev("max(min(x,1),sqrt(x))", -1.0, &[]).is_nan());
        assert!(ev("min(max(0,sqrt(x)),5)", -1.0, &[]).is_nan());
        assert!(ev("sign(max(sqrt(x),1))", -1.0, &[]).is_nan());
        assert_eq!(ev("sign(min(x,1)-1)", 3.0, &[]), 0.0);
        // A missing parameter is NaN and propagates too.
        assert!(ev("max(a, 1)", 0.0, &[]).is_nan());
    }

    #[test]
    fn invalid_domain_gives_nan_for_all_functions() {
        for f in [
            "sqrt(-1)",
            "ln(-1)",
            "log(-1)",
            "log10(-1)",
            "log2(-1)",
            "logx(-1, 2)",
            "logx(2, -1)",
            "logx(2, 1)",
            "logx(2, 0)",
            "asin(2)",
            "asin(-2)",
            "acos(2)",
            "acos(-2)",
            "pow(-8, 0.5)",
            "pow(-1, 0.5)",
            "erf(0/0)",
            "erfc(0/0)",
            "0/0",
            "atan2(0/0, 1)",
            "exp(0/0)",
            "abs(0/0)",
            "sin(1/0 - 1/0)",
            "cos(1/0 - 1/0)",
            "tan(1/0 - 1/0)",
            "floor(0/0)",
            "ceil(0/0)",
            "tanh(0/0)",
            "sinh(0/0)",
            "cosh(0/0)",
            "atan(0/0)",
        ] {
            assert!(ev(f, 0.0, &[]).is_nan(), "{f}");
        }
        // Current values pinned: ln(0) = -inf (existing choice), log(0)
        // likewise, logx(0, 2) = -inf, sin(inf) = NaN, exp(inf) = inf.
        assert_eq!(ev("ln(0)", 0.0, &[]), f64::NEG_INFINITY);
        assert_eq!(ev("log(0)", 0.0, &[]), f64::NEG_INFINITY);
        assert_eq!(ev("log2(0)", 0.0, &[]), f64::NEG_INFINITY);
        assert_eq!(ev("logx(0, 2)", 0.0, &[]), f64::NEG_INFINITY);
        assert_eq!(ev("sqrt(0)", 0.0, &[]), 0.0);
        assert_eq!(ev("asin(1)", 0.0, &[]), std::f64::consts::FRAC_PI_2);
        assert_eq!(ev("acos(1)", 0.0, &[]), 0.0);
        assert_eq!(ev("exp(1/0)", 0.0, &[]), f64::INFINITY);
        assert_eq!(ev("exp(-1/0)", 0.0, &[]), 0.0);
        assert_eq!(ev("tanh(1/0)", 0.0, &[]), 1.0);
        assert_eq!(ev("atan(1/0)", 0.0, &[]), std::f64::consts::FRAC_PI_2);
        assert!(ev("sin(1/0)", 0.0, &[]).is_nan());
        assert_eq!(ev("pow(0, 0)", 0.0, &[]), 1.0);
    }

    #[test]
    fn syntax_errors_with_position() {
        assert_eq!(Expr::parse("").unwrap_err().kind, ExprErrorKind::Empty);
        assert_eq!(Expr::parse("   ").unwrap_err().kind, ExprErrorKind::Empty);
        let e = Expr::parse("1 + $").unwrap_err();
        assert_eq!(e.kind, ExprErrorKind::UnexpectedChar('$'));
        assert_eq!(e.position, 4);
        assert_eq!(
            Expr::parse("(1 + 2").unwrap_err().kind,
            ExprErrorKind::UnbalancedParenthesis
        );
        assert_eq!(
            Expr::parse("1 + 2)").unwrap_err().kind,
            ExprErrorKind::UnbalancedParenthesis
        );
        assert_eq!(
            Expr::parse("1 +").unwrap_err().kind,
            ExprErrorKind::UnexpectedEnd
        );
        assert_eq!(
            Expr::parse("foo(1)").unwrap_err().kind,
            ExprErrorKind::UnknownFunction("foo".into())
        );
        assert_eq!(
            Expr::parse("exp(1, 2)").unwrap_err().kind,
            ExprErrorKind::WrongArgumentCount {
                name: "exp".into(),
                expected: 1,
                found: 2
            }
        );
        assert_eq!(
            Expr::parse("1 2").unwrap_err().kind,
            ExprErrorKind::UnexpectedToken
        );
    }

    /// Runs `f` on a 2 MiB thread (that of the session actor);
    /// in debug, this is the tightest case.
    fn on_2_mib<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn too_deep(src: String) -> bool {
        on_2_mib(move || {
            matches!(
                Expr::parse(&src),
                Err(ExprError {
                    kind: ExprErrorKind::TooDeep,
                    ..
                })
            )
        })
    }

    #[test]
    fn deep_formula_refused_without_overflowing_the_stack() {
        // Shapes that aborted the process before the cap (#279).
        assert!(too_deep(format!(
            "{}1{}",
            "(".repeat(100_000),
            ")".repeat(100_000)
        )));
        assert!(too_deep(format!("{}1", "1+".repeat(100_000))));
        assert!(too_deep(format!("{}1", "1*".repeat(100_000))));
        assert!(too_deep(format!("{}1", "-".repeat(100_000))));
        assert!(too_deep(format!("{}1", "1^".repeat(100_000))));
        assert!(too_deep(format!(
            "{}1{}",
            "sin(".repeat(100_000),
            ")".repeat(100_000)
        )));
    }

    #[test]
    fn formula_just_under_the_cap_accepted_and_evaluated() {
        let v = on_2_mib(|| {
            let sum = Expr::parse(&format!("{}1", "1+".repeat(MAX_DEPTH - 2))).unwrap();
            let n = MAX_DEPTH / NEST_COST - 1;
            let parens = Expr::parse(&format!("{}x{}", "(".repeat(n), ")".repeat(n))).unwrap();
            let neg = Expr::parse(&format!("{}x", "-".repeat(n))).unwrap();
            (
                sum.eval(0.0, &[]),
                parens.eval(2.0, &[]),
                neg.eval(3.0, &[]),
            )
        });
        assert_eq!(v, ((MAX_DEPTH - 1) as f64, 2.0, -3.0));
    }

    // ---- Depth cap at the limit (#279) ---------------------------------------
    //
    // Sizes are hard-coded (100 nestings, 1 001 terms) and not
    // derived from `MAX_DEPTH` / `NEST_COST`: a test that recomputes the limit
    // with the constants would not see a changed constant.

    /// Parses, evaluates (x = 1) and frees `src` on a 2 MiB thread.
    fn read_and_eval(src: String) -> Result<f64, ExprError> {
        on_2_mib(move || Expr::parse(&src).map(|e| e.eval(1.0, &[])))
    }

    fn too_deep_position(src: String) -> usize {
        match read_and_eval(src) {
            Err(ExprError {
                kind: ExprErrorKind::TooDeep,
                position,
            }) => position,
            other => panic!("expected TooDeep, got {other:?}"),
        }
    }

    /// `n` nesting levels of `open` around `x`, closed by `close`.
    fn nest(open: &str, close: &str, n: usize) -> String {
        format!("{}x{}", open.repeat(n), close.repeat(n))
    }

    #[test]
    fn chain_of_terms_exactly_at_the_limit() {
        // One term of a chain costs 1: 1 001 terms (1 000 operators) pass,
        // 1 002 do not; the error points at the token after the 1 001st operator.
        for op in ["+", "-", "*", "/"] {
            for n in [999, 1000, 1001] {
                let src = format!("1{}", format!("{op}1").repeat(n - 1));
                assert!(read_and_eval(src).is_ok(), "{n} terms {op}");
            }
            let src = format!("1{}", format!("{op}1").repeat(1001));
            assert_eq!(too_deep_position(src), 2002, "1002 terms {op}");
        }
        // Values: 1 001 ones added up.
        let sum = format!("1{}", "+1".repeat(1000));
        assert_eq!(read_and_eval(sum).unwrap(), 1001.0);
        let product = format!("2{}", "*1".repeat(1000));
        assert_eq!(read_and_eval(product).unwrap(), 2.0);
    }

    #[test]
    fn nesting_exactly_at_the_limit_for_each_shape() {
        // One nesting level costs 10: 100 pass, 101 do not.
        let shapes: [(&str, &str, &str); 6] = [
            ("(", ")", "parentheses"),
            ("-", "", "negations"),
            ("+", "", "unary pluses"),
            ("sin(", ")", "sin calls"),
            ("1^", "", "exponents"),
            ("pow(1,", ")", "nested pow calls"),
        ];
        for (open, close, name) in shapes {
            for n in [99, 100] {
                assert!(read_and_eval(nest(open, close, n)).is_ok(), "{n} {name}");
            }
            // Position: token after the opening of the 101st level.
            let expected = match open {
                "(" | "-" | "+" => 101,
                "sin(" => 404,
                "1^" => 202,
                _ => 6 * 101 - 2,
            };
            assert_eq!(
                too_deep_position(nest(open, close, 101)),
                expected,
                "101 {name}"
            );
        }
    }

    #[test]
    fn values_at_the_nesting_limit() {
        assert_eq!(read_and_eval(nest("(", ")", 100)).unwrap(), 1.0);
        // 100 negations: even, hence x; 99: -x.
        assert_eq!(read_and_eval(nest("-", "", 100)).unwrap(), 1.0);
        assert_eq!(read_and_eval(nest("-", "", 99)).unwrap(), -1.0);
        assert_eq!(read_and_eval(nest("+", "", 100)).unwrap(), 1.0);
        // sin(sin(…sin(1)…)): the fixed point 0 is approached, the value finite.
        let v = read_and_eval(nest("sin(", ")", 100)).unwrap();
        let mut expected = 1.0_f64;
        for _ in 0..100 {
            expected = libm::sin(expected);
        }
        assert_eq!(v, expected);
        // x^x^…^x at 1: always 1.
        assert_eq!(read_and_eval(nest("x^", "", 100)).unwrap(), 1.0);
        // pow(1, pow(1, …(1))) = 1.
        assert_eq!(read_and_eval(nest("pow(1,", ")", 100)).unwrap(), 1.0);
    }

    #[test]
    fn mixed_shapes_at_the_limit() {
        // 50 parentheses (500) + chain of 500 operators: total 1 000.
        let formula =
            |ops: usize| format!("{}1{}{}", "(".repeat(50), "+1".repeat(ops), ")".repeat(50));
        assert_eq!(read_and_eval(formula(500)).unwrap(), 501.0);
        // The 501st operator: token after, 50 + 1 + 2*500 + 1 = 1052.
        assert_eq!(too_deep_position(formula(501)), 1052);

        // Chain of 495 operators, plus the one that introduces the last term
        // (496), then that term in parentheses (10) carrying 494 operators:
        // 496 + 10 + 494 = 1 000.
        let nested_chain =
            |inner: usize| format!("1{}+(1{})", "+1".repeat(495), "+1".repeat(inner));
        assert_eq!(read_and_eval(nested_chain(494)).unwrap(), 991.0);
        let _ = too_deep_position(nested_chain(495));

        // Two chains of 900 in sequence: the depth is given back between
        // the two parentheses, it is not a sum.
        let two = format!("(1{})*(1{})", "+1".repeat(900), "+1".repeat(900));
        assert_eq!(read_and_eval(two).unwrap(), 901.0 * 901.0);

        // Mixed levels: 25 × ( - sin( 1^ ) ) = 100 levels accepted, 101 not.
        let mixed = |turns: usize, extra: &str| {
            format!(
                "{}{}x{}",
                "(-sin(1^".repeat(turns),
                extra,
                "))".repeat(turns)
            )
        };
        // 4 levels per turn: `(`, `-`, `sin(`, `1^`.
        assert!(read_and_eval(mixed(25, "")).is_ok());
        too_deep_position(mixed(25, "-"));
        too_deep_position(mixed(26, ""));
    }

    #[test]
    fn depth_is_given_back_after_each_sub_expression() {
        // 20 blocks of 90 parentheses side by side: 19 operators + 900, the
        // depth of each block is given back before the next one.
        let one = nest("(", ")", 90);
        let src = vec![one; 20].join("+");
        assert!(read_and_eval(src).is_ok());
        // 900 successive calls: 899 operators + 10 for the call.
        let src = vec!["sin(x)"; 900].join("*");
        assert!(read_and_eval(src).is_ok());
        // Multiple arguments: each restarts from the same depth.
        let arg = nest("(", ")", 98); // max( + 98 = 990
        let src = format!("max({arg}, {arg})");
        assert!(read_and_eval(src).is_ok());
    }

    #[test]
    fn quick_refusal_without_walking_a_million_elements() {
        let start = std::time::Instant::now();
        let n = 1_000_000;
        assert!(too_deep(format!("{}1", "1+".repeat(n))));
        assert!(too_deep(format!("{}1{}", "(".repeat(n), ")".repeat(n))));
        assert!(too_deep(format!("{}x", "-".repeat(n))));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "refusal too slow: {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn shapes_that_originally_declared_no_cost_are_all_bounded() {
        // Each of these shapes recursed without counting: 10 000 levels
        // are enough to prove that none escapes the cap.
        let n = 10_000;
        for src in [
            format!("{}1", "1^".repeat(n)),
            format!("{}1", "-".repeat(n)),
            format!("{}1", "+".repeat(n)),
            format!("{}1{}", "(".repeat(n), ")".repeat(n)),
            format!("{}1{}", "f(".repeat(n), ")".repeat(n)).replace("f(", "sin("),
            format!("{}1{}", "pow(1,".repeat(n), ")".repeat(n)),
            format!("{}x", "x^-".repeat(n)),
            format!("{}1", "-(".repeat(n)),
        ] {
            assert!(too_deep(src.clone()), "{}", &src[..src.len().min(24)]);
        }
    }

    #[test]
    fn formula_at_the_limit_fits_in_a_reduced_stack() {
        // Parse + eval + Drop of the most expensive formula of each shape,
        // on 1 MiB (half the stack of the actor). Measured in debug: the worst
        // shape (nestings of 100) passes at 768 KiB and aborts at 512 KiB, i.e.
        // a margin of 2.7 over the real 2 MiB and 1.3 over this test.
        let formulas = [
            format!("1{}", "+1".repeat(1000)),
            format!("1{}", "*x".repeat(1000)),
            nest("(", ")", 100),
            nest("-", "", 100),
            nest("sin(", ")", 100),
            nest("x^", "", 100),
            nest("pow(1,", ")", 100),
            format!("{}1{}{}", "(".repeat(50), "+1".repeat(500), ")".repeat(50)),
            format!("1{}+(1{})", "+1".repeat(495), "+1".repeat(494)),
            format!("{}x{}", "(-sin(1^".repeat(25), "))".repeat(25)),
        ];
        for src in formulas {
            let name = src[..src.len().min(20)].to_string();
            let ok = std::thread::Builder::new()
                .stack_size(1 << 20)
                .spawn(move || {
                    let e = Expr::parse(&src).unwrap();
                    let v = e.eval(1.0, &[]);
                    drop(e);
                    v.is_finite()
                })
                .unwrap()
                .join()
                .unwrap();
            assert!(ok, "{name}");
        }
    }

    #[test]
    fn ordinary_formulas_unchanged() {
        let (x, a, b) = (0.7_f64, 2.0_f64, 0.3_f64);
        let cases: [(&str, f64); 6] = [
            ("a*exp(-x/b) + 1", a * libm::exp(-x / b) + 1.0),
            ("a + b*x + x^2", a + b * x + x * x),
            (
                "a*sin(2*pi*x)^2",
                a * libm::sin(2.0 * std::f64::consts::PI * x).powi(2),
            ),
            ("-(x - a)^2 / (2*b^2)", -((x - a).powi(2)) / (2.0 * b * b)),
            ("a / (1 + exp(-(x - b)))", a / (1.0 + libm::exp(-(x - b)))),
            ("2^3^2 - -x", 512.0 + x),
        ];
        for (src, expected) in cases {
            let e = Expr::parse(src).unwrap();
            let p: Vec<f64> = e
                .params()
                .iter()
                .map(|n| if n == "a" { a } else { b })
                .collect();
            assert!((e.eval(x, &p) - expected).abs() < 1e-12, "{src}");
        }
    }

    #[test]
    fn no_realistic_hundred_term_formula_is_refused() {
        let sum_of_gaussians = (1..=100)
            .map(|i| format!("a{i}*exp(-((x - c{i})/w{i})^2)"))
            .collect::<Vec<_>>()
            .join(" + ");
        let polynomial = (0..100)
            .map(|i| format!("a{i}*x^{i}"))
            .collect::<Vec<_>>()
            .join(" + ");
        let product = (1..=100)
            .map(|i| format!("(1 + x/t{i})"))
            .collect::<Vec<_>>()
            .join(" * ");
        let alternating = (1..=100)
            .map(|i| format!("{}(a{i}*sin(x*{i}))", if i % 2 == 0 { "-" } else { "+" }))
            .collect::<Vec<_>>()
            .join(" ");
        for src in [sum_of_gaussians, polynomial, product, alternating] {
            let e = parse_and_count_params(src.clone());
            assert!(e.is_ok(), "{:?}", &src[..40]);
        }
    }

    fn parse_and_count_params(src: String) -> Result<usize, ExprError> {
        on_2_mib(move || Expr::parse(&src).map(|e| e.params().len()))
    }

    // ---- Logarithms (#287) ---------------------------------------------------

    fn is_close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * b.abs().max(1.0)
    }

    #[test]
    fn logarithms_table_of_values() {
        let e = std::f64::consts::E;
        let table: [(&str, f64); 14] = [
            ("log(1)", 0.0),
            ("log(10)", 1.0),
            ("log(100)", 2.0),
            ("log(0.001)", -3.0),
            ("log(1e6)", 6.0),
            ("ln(1)", 0.0),
            ("ln(e)", 1.0),
            ("ln(e^2)", 2.0),
            ("logx(8, 2)", 3.0),
            ("logx(81, 3)", 4.0),
            ("logx(0.5, 2)", -1.0),
            ("logx(1, 7)", 0.0),
            ("logx(e, e)", 1.0),
            ("logx(100, 10)", 2.0),
        ];
        for (src, expected) in table {
            let v = ev(src, 0.0, &[]);
            assert!(is_close(v, expected), "{src} = {v}, expected {expected}");
        }
        // `log` is not the natural logarithm: log(e) = 1/ln 10.
        assert!(is_close(ev("log(e)", 0.0, &[]), 1.0 / libm::log(10f64)));
        assert!((ev("log(e)", 0.0, &[]) - 1.0).abs() > 0.5);
        // logx(x, b) = ln x / ln b on a grid of bases and x.
        for b in [0.1, 0.5, 2.0, e, 3.0, 10.0, 1e3] {
            for x in [1e-9, 0.01, 0.5, 1.0, 2.0, 7.5, 1e4, 1e12] {
                let v = ev(&format!("logx(x, {b:e})"), x, &[]);
                let expected = libm::log(x) / libm::log(b);
                // The base can also be a free parameter.
                assert_eq!(ev("logx(x, b)", x, &[b]), v);
                assert!(is_close(v, expected), "logx({x}, {b}) = {v} vs {expected}");
            }
        }
    }

    #[test]
    fn logarithms_domain_and_special_values() {
        let (inf, ninf) = (f64::INFINITY, f64::NEG_INFINITY);
        // x < 0: NaN for all forms.
        for f in ["log(x)", "ln(x)", "log2(x)", "log10(x)", "logx(x, 2)"] {
            assert!(ev(f, -1.0, &[]).is_nan(), "{f}");
            assert!(ev(f, ninf, &[]).is_nan(), "{f}(-inf)");
            assert!(ev(f, f64::NAN, &[]).is_nan(), "{f}(NaN)");
            assert_eq!(ev(f, 0.0, &[]), ninf, "{f}(0) = -inf (existing choice)");
            assert_eq!(ev(f, -0.0, &[]), ninf, "{f}(-0) = -inf");
        }
        // x = +inf: infinity.
        for f in ["log(x)", "ln(x)", "log2(x)", "log10(x)", "logx(x, 2)"] {
            assert_eq!(ev(f, inf, &[]), inf, "{f}(inf)");
        }
        // logx(inf, b): +inf for b > 1, -inf for 0 < b < 1.
        assert_eq!(ev("logx(x, 0.5)", inf, &[]), ninf);
        assert_eq!(ev("logx(0, 0.5)", 0.0, &[]), inf);
        // Base ≤ 0, = 1 or NaN: NaN, whatever x is (even valid, even NaN).
        for b in ["0", "-0", "-2", "1", "(0/0)", "-1/0"] {
            for x in [0.5, 1.0, 8.0] {
                let v = ev(&format!("logx({x}, {b})"), 0.0, &[]);
                assert!(v.is_nan(), "logx({x}, {b}) = {v}");
            }
        }
        // x NaN with a valid base: NaN.
        assert!(ev("logx(0/0, 2)", 0.0, &[]).is_nan());
        // Infinite base: x/inf -> 0 (b = +inf is not rejected, ln b = inf).
        assert_eq!(ev("logx(8, 1/0)", 0.0, &[]), 0.0);
    }

    #[test]
    fn logarithms_exact_arity_and_case() {
        let arity = |src: &str| match Expr::parse(src).unwrap_err() {
            ExprError {
                kind:
                    ExprErrorKind::WrongArgumentCount {
                        name,
                        expected,
                        found,
                    },
                position,
            } => (name, expected, found, position),
            other => panic!("{src}: {other:?}"),
        };
        assert_eq!(arity("logx(8)"), ("logx".into(), 2, 1, 0));
        assert_eq!(arity("logx()"), ("logx".into(), 2, 0, 0));
        assert_eq!(arity("logx(1, 2, 3)"), ("logx".into(), 2, 3, 0));
        assert_eq!(arity("log(8, 2)"), ("log".into(), 1, 2, 0));
        assert_eq!(arity("log()"), ("log".into(), 1, 0, 0));
        assert_eq!(arity("ln(8, 2)"), ("ln".into(), 1, 2, 0));
        assert_eq!(arity("ln()"), ("ln".into(), 1, 0, 0));
        assert_eq!(arity("log2(1, 2)"), ("log2".into(), 1, 2, 0));
        assert_eq!(arity("log10()"), ("log10".into(), 1, 0, 0));
        // The position is that of the name, not of the parenthesis.
        assert_eq!(arity("1 + logx(8)"), ("logx".into(), 2, 1, 4));
        // Function names are case-insensitive (shared table, #401).
        for name in ["LOG", "Ln", "LN", "LogX", "Log10", "LOG2"] {
            let upper = Expr::parse(&format!("{name}(8, 2)"));
            let lower = Expr::parse(&format!("{}(8, 2)", name.to_lowercase()));
            assert_eq!(upper.is_ok(), lower.is_ok(), "{name}");
        }
        assert!((ev("LOGX(8, 2)", 0.0, &[]) - 3.0).abs() < 1e-12);
        assert_eq!(
            Expr::parse("LOGY(2)").unwrap_err().kind,
            ExprErrorKind::UnknownFunction("LOGY".into())
        );
        // `pi()` is accepted next to the bare constant; parameters stay
        // case-sensitive.
        assert_eq!(ev("pi()", 0.0, &[]), ev("pi", 0.0, &[]));
        assert_eq!(Expr::parse("Pi + PI").unwrap().params(), ["Pi", "PI"]);
        // Without parenthesis, `log` / `ln` are ordinary parameters.
        assert_eq!(Expr::parse("log + ln").unwrap().params(), ["log", "ln"]);
    }

    #[test]
    fn log2_log10_and_log_are_consistent_with_logx() {
        for x in [1e-6, 0.3, 1.0, 2.0, 5.0, 123.456, 1e9] {
            let (log, log10, log2, ln) = (
                ev("log(x)", x, &[]),
                ev("log10(x)", x, &[]),
                ev("log2(x)", x, &[]),
                ev("ln(x)", x, &[]),
            );
            assert_eq!(log, log10, "log and log10 are the same logarithm (x = {x})");
            assert!(is_close(log10, ev("logx(x, 10)", x, &[])), "x = {x}");
            assert!(is_close(log2, ev("logx(x, 2)", x, &[])), "x = {x}");
            assert!(is_close(ln, ev("logx(x, e)", x, &[])), "x = {x}");
            // Relations between bases.
            assert!(is_close(ln, log * libm::log(10f64)), "x = {x}");
            assert!(is_close(log2, ln / libm::log(2f64)), "x = {x}");
        }
        // logx is antisymmetric when swapping x and the base: logx(x,b)*logx(b,x) = 1.
        for (x, b) in [(8.0, 2.0), (5.0, 3.0), (0.2, 7.0)] {
            let p = ev(&format!("logx({x},{b})*logx({b},{x})"), 0.0, &[]);
            assert!(is_close(p, 1.0), "{x} {b}");
        }
    }

    #[test]
    fn erf_reference_values() {
        // Tabulated values (Abramowitz & Stegun 7.1).
        let cases = [
            (0.0, 0.0),
            (0.5, 0.520_499_877_813_046_5),
            (1.0, 0.842_700_792_949_714_9),
            (2.0, 0.995_322_265_018_952_7),
            (3.5, 0.999_999_256_901_627_7),
        ];
        for (x, want) in cases {
            assert!((erf(x) - want).abs() < 1e-12, "erf({x}) = {}", erf(x));
            assert!((erf(-x) + want).abs() < 1e-12);
        }
        assert!((ev("erfc(1)", 0.0, &[]) - 0.157_299_207_050_285_1).abs() < 1e-12);
    }

    #[test]
    fn erfc_keeps_its_relative_precision_in_the_tail() {
        // erfc(3) and erfc(6): tabulated values; 1 − erf(6) would be 0.
        for (x, want) in [
            (3.0, 2.209_049_699_858_544e-5),
            (6.0, 2.151_973_671_249_892e-17),
        ] {
            assert!(
                (erfc(x) / want - 1.0).abs() < 1e-9,
                "erfc({x}) = {}",
                erfc(x)
            );
        }
        assert!((erfc(-3.0) - (2.0 - 2.209_049_699_858_544e-5)).abs() < 1e-12);
        assert_eq!(erfc(40.0), 0.0);
    }
}
