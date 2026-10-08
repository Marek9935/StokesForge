//! Analyseur d'expressions du **modèle libre** (#188, M13 a B) : `y = …` saisi
//! par l'utilisateur, avec `x`, des paramètres libres et des fonctions.
//!
//! Choix : l'analyseur vit ici et non dans `stokesforge-core::formula` (qui ne
//! fait que `+ - * /` sur des colonnes) — ce crate reste sans dépendance du
//! workspace. Les deux grammaires restent distinctes ; les partager demanderait
//! de descendre un analyseur commun sous `core`, à décider si la formule de
//! colonne apprend un jour les fonctions.
//!
//! Grammaire (précédence croissante) : `+ -` ; `* /` ; `-` unaire ; `^`
//! (associatif à droite, `-2^2 = -4`) ; appel `f(a, b)`, nombre, nom,
//! parenthèses. Les noms `x`, `pi`, `e` sont réservés ; tout autre nom hors
//! appel est un **paramètre**, dans l'ordre de sa première apparition.
//! La multiplication implicite (`2x`) n'existe pas.

use std::fmt;

/// Une erreur de syntaxe, avec la position (en caractères, à partir de 0).
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
            Self::UnknownFunction(n) => write!(f, "unknown function '{n}'"),
            Self::WrongArgumentCount {
                name,
                expected,
                found,
            } => write!(f, "'{name}' takes {expected} argument(s), {found} given"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Exp,
    Ln,
    Log10,
    Log2,
    Sqrt,
    Abs,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Erf,
    Erfc,
    Sign,
    Floor,
    Ceil,
    Pow,
    Min,
    Max,
    Atan2,
}

impl Func {
    fn from_name(name: &str) -> Option<(Func, usize)> {
        Some(match name {
            "exp" => (Func::Exp, 1),
            "ln" | "log" => (Func::Ln, 1),
            "log10" => (Func::Log10, 1),
            "log2" => (Func::Log2, 1),
            "sqrt" => (Func::Sqrt, 1),
            "abs" => (Func::Abs, 1),
            "sin" => (Func::Sin, 1),
            "cos" => (Func::Cos, 1),
            "tan" => (Func::Tan, 1),
            "asin" => (Func::Asin, 1),
            "acos" => (Func::Acos, 1),
            "atan" => (Func::Atan, 1),
            "sinh" => (Func::Sinh, 1),
            "cosh" => (Func::Cosh, 1),
            "tanh" => (Func::Tanh, 1),
            "erf" => (Func::Erf, 1),
            "erfc" => (Func::Erfc, 1),
            "sign" => (Func::Sign, 1),
            "floor" => (Func::Floor, 1),
            "ceil" => (Func::Ceil, 1),
            "pow" => (Func::Pow, 2),
            "min" => (Func::Min, 2),
            "max" => (Func::Max, 2),
            "atan2" => (Func::Atan2, 2),
            _ => return None,
        })
    }

    fn apply(self, a: f64, b: f64) -> f64 {
        match self {
            Func::Exp => a.exp(),
            Func::Ln => a.ln(),
            Func::Log10 => a.log10(),
            Func::Log2 => a.log2(),
            Func::Sqrt => a.sqrt(),
            Func::Abs => a.abs(),
            Func::Sin => a.sin(),
            Func::Cos => a.cos(),
            Func::Tan => a.tan(),
            Func::Asin => a.asin(),
            Func::Acos => a.acos(),
            Func::Atan => a.atan(),
            Func::Sinh => a.sinh(),
            Func::Cosh => a.cosh(),
            Func::Tanh => a.tanh(),
            Func::Erf => erf(a),
            Func::Erfc => 1.0 - erf(a),
            Func::Sign => {
                if a > 0.0 {
                    1.0
                } else if a < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Func::Floor => a.floor(),
            Func::Ceil => a.ceil(),
            Func::Pow => a.powf(b),
            Func::Min => a.min(b),
            Func::Max => a.max(b),
            Func::Atan2 => a.atan2(b),
        }
    }
}

/// Fonction d'erreur (Abramowitz & Stegun 7.1.26 raffinée par une série pour
/// les petits arguments) : erreur absolue < 1e-12 sur ℝ.
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let ax = x.abs();
    let v = if ax < 3.0 {
        // Série de Maclaurin : erf(x) = 2/√π Σ (-1)^n x^(2n+1) / (n! (2n+1)).
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
        // Fraction continue de erfc pour les grands arguments.
        let mut f = 0.0;
        for k in (1..=60).rev() {
            f = (k as f64 / 2.0) / (ax + f);
        }
        1.0 - (-ax * ax).exp() / (std::f64::consts::PI.sqrt() * (ax + f))
    };
    if x < 0.0 {
        -v
    } else {
        v
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

/// Une expression compilée : paramètres résolus en indices.
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
                    // Exposant seulement s'il est suivi d'un chiffre (avec signe
                    // facultatif) : `2e` reste `2` puis le nom `e`.
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

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    end: usize,
    params: Vec<String>,
}

impl Parser {
    fn err(&self, kind: ExprErrorKind) -> ExprError {
        let position = self.toks.get(self.pos).map_or(self.end, |t| t.1);
        ExprError { kind, position }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn expr(&mut self) -> Result<Node, ExprError> {
        let mut left = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            let right = self.term()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.pos += 1;
            let right = self.unary()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Ok(Node::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Node, ExprError> {
        let base = self.atom()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            // Associatif à droite ; l'exposant peut porter un signe (`2^-3`).
            let exp = self.unary()?;
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
                let inner = self.expr()?;
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
                    let Some((func, arity)) = Func::from_name(&name) else {
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
                            args.push(self.expr()?);
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
                    if args.len() != arity {
                        return Err(ExprError {
                            kind: ExprErrorKind::WrongArgumentCount {
                                name,
                                expected: arity,
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
    /// Compile `src`. Les espaces autour sont ignorés ; un texte vide est une
    /// erreur.
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

    /// Noms des paramètres, dans l'ordre de leur première apparition.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Valeur en `x` pour les paramètres `p` (même ordre que [`Expr::params`]).
    /// Un domaine invalide donne `NaN`, jamais une panique.
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
                _ => a.powf(b),
            }
        }
        Node::Call(f, args) => {
            let a = eval_node(&args[0], x, p);
            let b = args.get(1).map_or(0.0, |n| eval_node(n, x, p));
            f.apply(a, b)
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
    fn precedence_et_associativite() {
        assert_eq!(ev("1 + 2 * 3", 0.0, &[]), 7.0);
        assert_eq!(ev("(1 + 2) * 3", 0.0, &[]), 9.0);
        assert_eq!(ev("2 ^ 3 ^ 2", 0.0, &[]), 512.0);
        assert_eq!(ev("-2 ^ 2", 0.0, &[]), -4.0);
        assert_eq!(ev("2 ^ -1", 0.0, &[]), 0.5);
        assert_eq!(ev("10 - 4 - 3", 0.0, &[]), 3.0);
        assert_eq!(ev("8 / 4 / 2", 0.0, &[]), 1.0);
    }

    #[test]
    fn parametres_dans_l_ordre_d_apparition() {
        let e = Expr::parse("a * exp(-x / tau) + y0 + a").unwrap();
        assert_eq!(e.params(), ["a", "tau", "y0"]);
        let v = e.eval(2.0, &[3.0, 2.0, 1.0]);
        assert!((v - (3.0 * (-1.0_f64).exp() + 1.0 + 3.0)).abs() < 1e-15);
    }

    #[test]
    fn constantes_et_fonctions() {
        assert!((ev("pi", 0.0, &[]) - std::f64::consts::PI).abs() < 1e-15);
        assert!((ev("e", 0.0, &[]) - std::f64::consts::E).abs() < 1e-15);
        assert!((ev("ln(e)", 0.0, &[]) - 1.0).abs() < 1e-15);
        assert!((ev("sqrt(x)", 16.0, &[]) - 4.0).abs() < 1e-15);
        assert_eq!(ev("max(x, 3)", 5.0, &[]), 5.0);
        assert_eq!(ev("pow(2, 10)", 0.0, &[]), 1024.0);
        assert!((ev("log10(1000)", 0.0, &[]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn notation_scientifique_sans_confondre_la_constante_e() {
        assert_eq!(ev("1e3", 0.0, &[]), 1000.0);
        assert_eq!(ev("2.5E-2", 0.0, &[]), 0.025);
        // `2e` n'est pas un exposant : erreur (pas de multiplication implicite).
        assert!(Expr::parse("2e").is_err());
        assert!(Expr::parse("2 * e").is_ok());
    }

    #[test]
    fn domaine_invalide_donne_nan() {
        assert!(ev("sqrt(x)", -1.0, &[]).is_nan());
        assert!(ev("ln(x)", -1.0, &[]).is_nan());
        assert!(ev("1 / x", 0.0, &[]).is_infinite());
    }

    #[test]
    fn erreurs_de_syntaxe_avec_position() {
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

    #[test]
    fn erf_valeurs_de_reference() {
        // Valeurs tabulées (Abramowitz & Stegun 7.1).
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
}
