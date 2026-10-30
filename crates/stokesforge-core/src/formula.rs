//! Columns computed by formula (v1, simple arithmetic within a sheet).
//!
//! Scope: `+ - * /` and `^`, parentheses, numeric literals, column
//! references, **row-wise functions** (`sqrt(A)`, `round(B, 2)`, `pi()`, ...
//! the table shared with the Fit, `stokesforge_analysis::functions`, #401),
//! comparisons (`< <= > >= == !=`, true = 1, false = 0) and
//! `if(cond, a, b)`. No cross-sheet reference, no function that reads a whole
//! column — out of scope for this version.
//!
//! Precedence, lowest first: comparisons; `+ -`; `* /`; unary sign; `^`
//! (right-associative, `-2^2 = -4`, `2^3^2 = 512`); call, number, column,
//! parentheses. **A name followed by `(` is a function** (case-insensitive,
//! `EXP(B)` = `exp(B)`); **a lone name is a column**, lowercase included
//! (`b/2` = column B / 2). π is `pi()`. No implicit multiplication:
//! `A(B/C)` is refused, write `A*(B/C)`.
//!
//! A function call adds nothing to a formula's inputs: only the columns it
//! names count (invariant 11, [`Formula::referenced_columns`] walks every
//! node).
//!
//! # A formula names columns, not letters (#321)
//!
//! The user types and reads position **letters** (`A` = first
//! column, `B` = second, ... `Z`, `AA`, `AB`, ... Origin/Excel style), but
//! the **stored** formula holds the column itself: [`ColumnFormula`] is
//! a sequence of text fragments (operators, numbers, spaces,
//! parentheses, as typed) and column identifiers
//! ([`FormulaPart`]). Letters serve only at the boundary:
//! [`ColumnFormula::from_letters`] (input) and [`ColumnFormula::letters`]
//! (display). A column that changes place, or an analysis that inserts its
//! outputs in the middle of the sheet, thus changes the **displayed** letters without
//! ever changing what the formula reads. A formula is never recalculated from
//! its letters (`2C3` would display as `2E3` if C became E): the
//! tokens come from the fragments ([`ColumnFormula::compile`]).
//!
//! An identifier that is no longer in the sheet (column deleted, moved
//! to another sheet) displays as [`REF_TOKEN`] and computes as
//! [`FormulaError::DeletedColumn`]: the identifier stays stored, so
//! an undo that brings the column back brings the formula back too.
//!
//! Pure module: no access to the `Project` or the tree — [`evaluate`] takes
//! only a slice of [`ColumnValues`] already extracted by the caller, with their
//! identifiers (typically `stokesforge-session::recalc`, which orchestrates the
//! recalculation — see its top-level doc for dependency ordering and
//! cycle detection, for which [`topological_order`] below is the generic
//! tool).
//!
//! # Graceful degradation (invariant 10) — never a panic
//!
//! A formula that does not compile, or reads a column absent from the sheet,
//! returns a block **entirely `NaN`** of the requested length, together with a
//! [`FormulaError`] that the caller can display — never a `Result` swallowed
//! silently, never a panic. A referenced column shorter than the
//! sheet is not an error: [`ColumnValues::number_at`] already returns
//! `NaN` beyond its length, exactly the behavior wanted here.
//!
//! A **mixed**-type column is not "non-numeric": it is
//! accepted, and its text or empty cells are `NaN` row by row, like
//! an ordinary missing value. Only an entirely text column is
//! refused as a whole.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use stokesforge_id::NodeId;

use stokesforge_analysis::functions::Func;

use crate::column::{ColumnValues, NumericStatus};

/// What a formula displays in place of a column it can no longer find
/// in its sheet (deleted, or moved to another sheet).
pub const REF_TOKEN: &str = "#REF";

/// Why a formula does not produce the values it describes. Never a
/// panic (invariant 10): each variant is a first-class state,
/// displayable as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaError {
    /// The string does not parse as a valid formula of the v1
    /// grammar (unexpected token, unclosed parenthesis, empty formula, ...).
    Parse(String),
    /// The typed column letter names no column of the sheet
    /// (refusal by [`ColumnFormula::from_letters`]).
    UnknownColumn(String),
    /// The typed formula references the column that holds it (refusal by
    /// [`ColumnFormula::from_letters`]). Loops between several
    /// columns are, for their part, [`FormulaError::Cycle`] at computation.
    SelfReference(String),
    /// The formula reads a column that is no longer in its sheet (deleted,
    /// or moved to another sheet) — displayed [`REF_TOKEN`]. At
    /// computation: empty cells. At input: a text that still contains
    /// `#REF` is refused, it must be replaced by a letter.
    DeletedColumn,
    /// Dependency cycle between formula columns (directly or through a
    /// column whose computation itself depends on a cycle) — detected by
    /// the orchestrator ([`topological_order`]), never by a loop that
    /// would run forever.
    Cycle,
    /// The formula tree exceeds [`MAX_DEPTH`] (parentheses, signs or
    /// operator chain too long): refused at parse rather than letting
    /// parser, evaluation and `Drop` recurse until the process is
    /// aborted (#289).
    TooDeep,
    /// A name followed by `(` is not a function of the table (refusal by
    /// [`ColumnFormula::from_letters`]; a saved text fragment degrades to
    /// [`FormulaError::Parse`] instead, invariant 10).
    UnknownFunction(String),
    /// A function called with a count of arguments it does not accept
    /// (`expected` lists the accepted counts, ascending). Refused at input.
    WrongArgumentCount {
        name: String,
        expected: Vec<usize>,
        found: usize,
    },
    /// A column letter followed by `(` (`A(B/C)`): there is no implicit
    /// multiplication. Refused at input.
    NotAFunction(String),
    /// `pi` alone in a sheet without a `PI` column: the number π is `pi()`.
    /// Refused at input.
    PiNeedsParentheses,
}

impl std::fmt::Display for FormulaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormulaError::Parse(message) => write!(f, "invalid formula: {message}"),
            FormulaError::UnknownColumn(letter) => {
                write!(f, "column {letter} does not exist in the worksheet")
            }
            FormulaError::SelfReference(letter) => {
                write!(f, "the formula refers to its own column ({letter})")
            }
            FormulaError::DeletedColumn => {
                write!(
                    f,
                    "the formula reads a column that no longer exists in the worksheet"
                )
            }
            FormulaError::Cycle => write!(f, "dependency cycle between formula columns"),
            FormulaError::TooDeep => write!(f, "formula nested too deeply"),
            FormulaError::UnknownFunction(name) => write!(f, "unknown function {name}"),
            FormulaError::WrongArgumentCount {
                name,
                expected,
                found,
            } => write!(
                f,
                "function {name} takes {} argument(s), {found} given",
                counts_text(expected)
            ),
            FormulaError::NotAFunction(letter) => write!(
                f,
                "{letter} is not a function: to multiply, write {letter}*(...)"
            ),
            FormulaError::PiNeedsParentheses => {
                write!(f, "pi alone is a column: for the number pi, write pi()")
            }
        }
    }
}

impl std::error::Error for FormulaError {}

/// `1`, `1 or 2`, `1, 2 or 3`.
pub fn counts_text(counts: &[usize]) -> String {
    let words: Vec<String> = counts.iter().map(usize::to_string).collect();
    match words.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

// -- column letters: same bijective base-26 convention as
// `Project::spreadsheet_name` (A, B, ..., Z, AA, AB, ...), duplicated here
// rather than shared: it is a pure notation, with nothing to synchronize
// with the spreadsheet-style generation of column names. Letters live
// only at the boundary (input, display): see the top-level doc.

/// 0-based position → spreadsheet-style letter(s).
fn index_to_letters(index: usize) -> String {
    let mut name = String::new();
    let mut remaining = index as i64;
    loop {
        let offset = (remaining % 26) as u8;
        name.insert(0, char::from(b'A' + offset));
        remaining = remaining / 26 - 1;
        if remaining < 0 {
            break;
        }
    }
    name
}

/// Spreadsheet-style letter(s) → 0-based position. `None` if `letters` contains
/// anything other than ASCII letters, or if the sequence overflows a `u64` (an
/// absurdly long sequence of letters names no column).
fn letters_to_index(letters: &str) -> Option<usize> {
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut value: u64 = 0;
    for c in letters.chars() {
        let digit = (c.to_ascii_uppercase() as u8 - b'A' + 1) as u64;
        value = value.checked_mul(26)?.checked_add(digit)?;
    }
    Some((value - 1) as usize)
}

// -- lexer --

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Column(NodeId),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Comma,
    Cmp(CmpOp),
    Func(Callee),
    LParen,
    RParen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl CmpOp {
    /// Truth value as the formula sees it: 1 or 0.
    fn test(self, a: f64, b: f64) -> bool {
        match self {
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
        }
    }
}

/// What a name followed by `(` can call: a function of the shared table, or
/// `if` (which needs the row status, so it is not in the table).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Callee {
    Table(Func),
    If,
}

impl Callee {
    /// Case-insensitive.
    fn from_name(name: &str) -> Option<Callee> {
        if name.eq_ignore_ascii_case("if") {
            Some(Callee::If)
        } else {
            Func::from_name(name).map(Callee::Table)
        }
    }

    fn name(self) -> &'static str {
        match self {
            Callee::If => "if",
            Callee::Table(f) => f.name(),
        }
    }

    fn arity(self) -> &'static [usize] {
        match self {
            Callee::If => &[3],
            Callee::Table(f) => f.arity(),
        }
    }
}

/// End (exclusive) of the **call name** that starts at `start`, if it is
/// followed by `(` (spaces allowed): letters, then digits (`log10`, `atan2`).
/// A name with no `(` after it is **not** a call: only its letter run is a
/// column (`A2` stays `A` then `2`, a syntax error). Shared by
/// [`tokenize_text`] and [`ColumnFormula::from_letters`]: both must split the
/// same way.
fn call_name_end(chars: &[char], start: usize) -> Option<usize> {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_alphabetic() {
        end += 1;
    }
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    let mut next = end;
    while next < chars.len() && chars[next].is_whitespace() {
        next += 1;
    }
    (end > start && chars.get(next) == Some(&'(')).then_some(end)
}

/// End (exclusive) of the numeric literal that starts at `start`: digits and
/// points, then an optional exponent (`1e-3`, `2.5E6`) accepted only
/// if syntactically complete (at least one digit after the optional
/// sign), otherwise the `e` stays intact — it is then a column letter
/// (`1eA` does not read as an exponent). Shared by [`tokenize_text`] and
/// [`ColumnFormula::from_letters`]: both must split the same way, otherwise
/// `1e-3*A` would bind a column `E` at input.
fn number_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
    }
    if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
        let mut j = i + 1;
        if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
            j += 1;
        }
        if j < chars.len() && chars[j].is_ascii_digit() {
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    i
}

/// Tokens of all fragments, in order: a text fragment is
/// split, a column identifier is one token. **No letter is
/// read** — a letter in a text fragment (crafted file) is an
/// unexpected character, not a column (invariant 10: a syntax
/// error, never a panic).
fn tokenize(parts: &[FormulaPart]) -> Result<Vec<Token>, FormulaError> {
    let mut tokens = Vec::new();
    for part in parts {
        match part {
            FormulaPart::Text(text) => tokenize_text(text, &mut tokens)?,
            FormulaPart::Column(id) => tokens.push(Token::Column(*id)),
        }
    }
    Ok(tokens)
}

fn tokenize_text(source: &str, tokens: &mut Vec<Token>) -> Result<(), FormulaError> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '<' | '>' | '=' | '!' => {
                let with_equal = chars.get(i + 1) == Some(&'=');
                let op = match (c, with_equal) {
                    ('<', false) => CmpOp::Lt,
                    ('<', true) => CmpOp::Le,
                    ('>', false) => CmpOp::Gt,
                    ('>', true) => CmpOp::Ge,
                    ('=', true) => CmpOp::Eq,
                    ('!', true) => CmpOp::Ne,
                    // A lone `=` or `!`: not an operator.
                    _ => {
                        return Err(FormulaError::Parse(format!("unexpected character: {c}")));
                    }
                };
                tokens.push(Token::Cmp(op));
                i += if with_equal { 2 } else { 1 };
            }
            c if c.is_ascii_alphabetic() => {
                // Only a known function followed by `(` is read; any other
                // letter (crafted file) stays an unexpected character, never
                // a column (invariant 10).
                let callee = call_name_end(&chars, i).and_then(|end| {
                    let name: String = chars[i..end].iter().collect();
                    Callee::from_name(&name).map(|callee| (callee, end))
                });
                match callee {
                    Some((callee, end)) => {
                        tokens.push(Token::Func(callee));
                        i = end;
                    }
                    None => {
                        return Err(FormulaError::Parse(format!("unexpected character: {c}")));
                    }
                }
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                i = number_end(&chars, start);
                let text: String = chars[start..i].iter().collect();
                let value: f64 = text
                    .parse()
                    .map_err(|_| FormulaError::Parse(format!("invalid number: {text}")))?;
                tokens.push(Token::Number(value));
            }
            other => {
                return Err(FormulaError::Parse(format!(
                    "unexpected character: {other}"
                )))
            }
        }
    }
    Ok(())
}

// -- AST --

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Column(NodeId),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

/// Depth cap of the tree, in "cost": one nesting level
/// (parenthesis, sign, call, `^`) costs [`NEST_COST`], one term of a chain
/// `a+b+c…` or `a*b*c…`, and one comma of an argument list, costs 1. The parser, `eval_expr`/`combine`, `referenced_columns` and
/// the `Drop` of the tree recurse by that much: bounded here, none can exceed
/// the stack of a 2 MiB thread, even in debug (`docs/archive/rustrewrite/
/// questions-ouvertes.md` §11, #289). Allows 100 nestings or about 1,000 terms (1,001 pass).
pub const MAX_DEPTH: usize = 1000;
const NEST_COST: usize = 10;

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Adds `cost` to the current depth; beyond the cap, `TooDeep`.
    fn charge(&mut self, cost: usize) -> Result<(), FormulaError> {
        self.depth += cost;
        if self.depth > MAX_DEPTH {
            return Err(FormulaError::TooDeep);
        }
        Ok(())
    }

    /// Runs `f` one nesting level deeper, then gives the depth back.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, FormulaError>,
    ) -> Result<T, FormulaError> {
        self.charge(NEST_COST)?;
        let out = f(self)?;
        self.depth -= NEST_COST;
        Ok(out)
    }

    /// `cmp := expr (('<' | '<=' | '>' | '>=' | '==' | '!=') expr)*`
    /// (left-associative; true = 1, false = 0).
    fn parse_cmp(&mut self) -> Result<Expr, FormulaError> {
        let base = self.depth;
        let mut node = self.parse_expr()?;
        while let Some(Token::Cmp(op)) = self.peek() {
            let op = *op;
            self.pos += 1;
            self.charge(1)?;
            let rhs = self.parse_expr()?;
            node = Expr::Cmp(op, Box::new(node), Box::new(rhs));
        }
        self.depth = base;
        Ok(node)
    }

    /// `expr := term (('+' | '-') term)*`
    fn parse_expr(&mut self) -> Result<Expr, FormulaError> {
        let base = self.depth;
        let mut node = self.parse_term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.pos += 1;
                    self.charge(1)?;
                    let rhs = self.parse_term()?;
                    node = Expr::Add(Box::new(node), Box::new(rhs));
                }
                Some(Token::Minus) => {
                    self.pos += 1;
                    self.charge(1)?;
                    let rhs = self.parse_term()?;
                    node = Expr::Sub(Box::new(node), Box::new(rhs));
                }
                _ => break,
            }
        }
        self.depth = base;
        Ok(node)
    }

    /// `term := factor (('*' | '/') factor)*`
    fn parse_term(&mut self) -> Result<Expr, FormulaError> {
        let base = self.depth;
        let mut node = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.pos += 1;
                    self.charge(1)?;
                    let rhs = self.parse_factor()?;
                    node = Expr::Mul(Box::new(node), Box::new(rhs));
                }
                Some(Token::Slash) => {
                    self.pos += 1;
                    self.charge(1)?;
                    let rhs = self.parse_factor()?;
                    node = Expr::Div(Box::new(node), Box::new(rhs));
                }
                _ => break,
            }
        }
        self.depth = base;
        Ok(node)
    }

    /// `factor := ('+' | '-') factor | power`
    fn parse_factor(&mut self) -> Result<Expr, FormulaError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.pos += 1;
                let inner = self.nested(Self::parse_factor)?;
                Ok(Expr::Neg(Box::new(inner)))
            }
            Some(Token::Plus) => {
                self.pos += 1;
                self.nested(Self::parse_factor)
            }
            _ => self.parse_power(),
        }
    }

    /// `power := primary ('^' factor)?` — right-associative, and above the
    /// unary sign on its left (`-2^2 = -4`), below it on its right (`2^-3`).
    fn parse_power(&mut self) -> Result<Expr, FormulaError> {
        let base = self.parse_primary()?;
        if let Some(Token::Caret) = self.peek() {
            self.pos += 1;
            let exponent = self.nested(Self::parse_factor)?;
            return Ok(Expr::Pow(Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    /// `args := (cmp (',' cmp)*)?`, up to the closing parenthesis (consumed).
    fn parse_args(&mut self) -> Result<Vec<Expr>, FormulaError> {
        let base = self.depth;
        let mut args = Vec::new();
        if let Some(Token::RParen) = self.peek() {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            args.push(self.parse_cmp()?);
            match self.peek() {
                Some(Token::Comma) => {
                    self.pos += 1;
                    self.charge(1)?;
                }
                Some(Token::RParen) => {
                    self.pos += 1;
                    break;
                }
                _ => {
                    return Err(FormulaError::Parse(
                        "missing closing parenthesis".to_string(),
                    ))
                }
            }
        }
        self.depth = base;
        Ok(args)
    }

    /// `primary := number | column | name '(' args ')' | '(' cmp ')'`
    fn parse_primary(&mut self) -> Result<Expr, FormulaError> {
        match self.tokens.get(self.pos) {
            Some(Token::Number(value)) => {
                self.pos += 1;
                Ok(Expr::Number(*value))
            }
            Some(Token::Column(index)) => {
                self.pos += 1;
                Ok(Expr::Column(*index))
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let inner = self.nested(Self::parse_cmp)?;
                match self.tokens.get(self.pos) {
                    Some(Token::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err(FormulaError::Parse(
                        "missing closing parenthesis".to_string(),
                    )),
                }
            }
            Some(Token::Func(callee)) => {
                let callee = *callee;
                self.pos += 1;
                // The tokenizer only emits a function before its `(`.
                if self.peek() != Some(&Token::LParen) {
                    return Err(FormulaError::Parse(
                        "missing opening parenthesis".to_string(),
                    ));
                }
                self.pos += 1;
                let args = self.nested(Self::parse_args)?;
                if !callee.arity().contains(&args.len()) {
                    return Err(FormulaError::WrongArgumentCount {
                        name: callee.name().to_string(),
                        expected: callee.arity().to_vec(),
                        found: args.len(),
                    });
                }
                match callee {
                    // The arity (3) was just checked.
                    Callee::If => match <[Expr; 3]>::try_from(args) {
                        Ok([cond, then, otherwise]) => Ok(Expr::If(
                            Box::new(cond),
                            Box::new(then),
                            Box::new(otherwise),
                        )),
                        Err(_) => Err(FormulaError::Parse("if takes three arguments".to_string())),
                    },
                    Callee::Table(func) => Ok(Expr::Call(func, args)),
                }
            }
            Some(other) => Err(FormulaError::Parse(format!("unexpected token: {other:?}"))),
            None => Err(FormulaError::Parse("incomplete expression".to_string())),
        }
    }
}

/// A **stored** formula: the typed text, where each column letter is
/// replaced by the identifier of the column it named at the time of
/// input. See the module's top-level doc.
///
/// The [`FormulaPart::Text`] fragments keep what the user typed
/// (operators, numbers, spaces, parentheses). No fragment is
/// normalized: a round trip through the file gives back the same fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnFormula {
    parts: Vec<FormulaPart>,
}

/// A piece of a [`ColumnFormula`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaPart {
    /// Typed text, with no column letter (nor, at input, `#REF`).
    Text(String),
    /// The column read, by its identifier.
    Column(NodeId),
}

impl ColumnFormula {
    /// Rebuilds a formula from its fragments (reading a file).
    /// Nothing is checked here: a text that does not compile, or an
    /// identifier outside the sheet, degrades at computation
    /// ([`ColumnFormula::compile`], [`evaluate`]).
    pub fn from_parts(parts: Vec<FormulaPart>) -> Self {
        Self { parts }
    }

    /// The fragments, in text order.
    pub fn parts(&self) -> &[FormulaPart] {
        &self.parts
    }

    /// Reads a typed text: each sequence of letters is the **position** of a
    /// column of the sheet (`sheet_columns`, in the order of the sheet's
    /// children) and becomes its identifier; `own` is the column that will carry
    /// the formula.
    ///
    /// A name followed by `(` is a function and is kept as typed (the shared
    /// table, case-insensitive); refused if unknown
    /// ([`FormulaError::UnknownFunction`]), if it is a column letter
    /// ([`FormulaError::NotAFunction`]) or if the argument count is wrong
    /// ([`FormulaError::WrongArgumentCount`]); a lone `pi` with no `PI` column
    /// is [`FormulaError::PiNeedsParentheses`].
    ///
    /// Refusals (nothing is built): a letter that names no
    /// column ([`FormulaError::UnknownColumn`]), the column `own`
    /// ([`FormulaError::SelfReference`]), `#REF` ([`FormulaError::DeletedColumn`]:
    /// it is not a letter, it must be replaced). A syntax error
    /// (`A*`) is **not** refused here: the text is stored as is and
    /// [`ColumnFormula::compile`] returns it, as before identifiers.
    ///
    /// The splitting follows the lexer: a number with an exponent (`1e-3`) is a
    /// text fragment, not a column `E`.
    pub fn from_letters(
        text: &str,
        sheet_columns: &[NodeId],
        own: NodeId,
    ) -> Result<Self, FormulaError> {
        let chars: Vec<char> = text.chars().collect();
        let mut parts = Vec::new();
        let mut buffer = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '#'
                && chars[i + 1..]
                    .iter()
                    .take(3)
                    .collect::<String>()
                    .eq_ignore_ascii_case("ref")
            {
                return Err(FormulaError::DeletedColumn);
            } else if c.is_ascii_digit() || c == '.' {
                let end = number_end(&chars, i);
                buffer.extend(&chars[i..end]);
                i = end;
            } else if c.is_ascii_alphabetic() {
                let start = i;
                // A name followed by `(` is a function, not a column.
                if let Some(end) = call_name_end(&chars, start) {
                    let name: String = chars[start..end].iter().collect();
                    if Callee::from_name(&name).is_none() {
                        let letters = name.to_ascii_uppercase();
                        let names_a_column = letters_to_index(&letters)
                            .is_some_and(|index| index < sheet_columns.len());
                        return Err(if names_a_column {
                            FormulaError::NotAFunction(letters)
                        } else {
                            FormulaError::UnknownFunction(name)
                        });
                    }
                    buffer.push_str(&name);
                    i = end;
                    continue;
                }
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let letters: String = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .to_ascii_uppercase();
                let Some(&id) =
                    letters_to_index(&letters).and_then(|index| sheet_columns.get(index))
                else {
                    // `pi` is the number only with parentheses.
                    return Err(if letters == "PI" {
                        FormulaError::PiNeedsParentheses
                    } else {
                        FormulaError::UnknownColumn(letters)
                    });
                };
                if id == own {
                    return Err(FormulaError::SelfReference(letters));
                }
                if !buffer.is_empty() {
                    parts.push(FormulaPart::Text(std::mem::take(&mut buffer)));
                }
                parts.push(FormulaPart::Column(id));
            } else {
                buffer.push(c);
                i += 1;
            }
        }
        if !buffer.is_empty() {
            parts.push(FormulaPart::Text(buffer));
        }
        let formula = Self { parts };
        // A wrong argument count is refused here; any other syntax error is
        // stored and returned by `compile`, as before.
        if let Err(error @ FormulaError::WrongArgumentCount { .. }) = formula.compile() {
            return Err(error);
        }
        Ok(formula)
    }

    /// The text to display: each column read is written by its **current
    /// position letter** in `sheet_columns` (letters follow the
    /// columns), or [`REF_TOKEN`] if it is no longer there. For display
    /// only: never reread to compute or to modify another field
    /// (see the top-level doc).
    pub fn letters(&self, sheet_columns: &[NodeId]) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                FormulaPart::Text(text) => out.push_str(text),
                FormulaPart::Column(id) => match sheet_columns.iter().position(|c| c == id) {
                    Some(index) => out.push_str(&index_to_letters(index)),
                    None => out.push_str(REF_TOKEN),
                },
            }
        }
        out
    }

    /// True if a column read is no longer in `sheet_columns`: the formula
    /// displays with [`REF_TOKEN`] and computes as
    /// [`FormulaError::DeletedColumn`].
    pub fn has_deleted_reference(&self, sheet_columns: &[NodeId]) -> bool {
        self.parts.iter().any(|part| match part {
            FormulaPart::Column(id) => !sheet_columns.contains(id),
            FormulaPart::Text(_) => false,
        })
    }

    /// Identifiers of all columns read, with no judgment on their
    /// presence in the sheet.
    pub fn columns(&self) -> BTreeSet<NodeId> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                FormulaPart::Column(id) => Some(*id),
                FormulaPart::Text(_) => None,
            })
            .collect()
    }

    /// Same formula, each identifier passed through `map` (copy of a sheet
    /// or workbook: the copy reads its own columns). Text fragments
    /// are unchanged.
    pub fn remap(&self, mut map: impl FnMut(NodeId) -> NodeId) -> Self {
        Self {
            parts: self
                .parts
                .iter()
                .map(|part| match part {
                    FormulaPart::Column(id) => FormulaPart::Column(map(*id)),
                    FormulaPart::Text(text) => FormulaPart::Text(text.clone()),
                })
                .collect(),
        }
    }

    /// Parses the formula: the tokens come from the fragments, never from the
    /// displayed text. `Err(FormulaError::Parse)` if it does not match the
    /// v1 grammar — never a panic, whatever the input (a free
    /// typed text, or a crafted file).
    pub fn compile(&self) -> Result<Formula, FormulaError> {
        let tokens = tokenize(&self.parts)?;
        if tokens.is_empty() {
            return Err(FormulaError::Parse("empty formula".to_string()));
        }
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_cmp()?;
        if parser.pos != tokens.len() {
            return Err(FormulaError::Parse(
                "unexpected tokens after the formula".to_string(),
            ));
        }
        Ok(Formula { expr })
    }
}

/// A parsed formula: simple arithmetic on columns named by
/// their identifier. Carries no reference to a specific sheet or
/// project — [`evaluate`] puts it in presence of concrete columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    expr: Expr,
}

impl Formula {
    /// Identifiers of all the columns this formula references, with no
    /// judgment on their validity — [`evaluate`] checks that they
    /// are in the sheet. Also used by the orchestrator to build the
    /// dependency graph between formula columns (see
    /// [`topological_order`]).
    pub fn referenced_columns(&self) -> BTreeSet<NodeId> {
        fn walk(expr: &Expr, out: &mut BTreeSet<NodeId>) {
            match expr {
                Expr::Number(_) => {}
                Expr::Column(id) => {
                    out.insert(*id);
                }
                Expr::Neg(inner) => walk(inner, out),
                Expr::Add(a, b)
                | Expr::Sub(a, b)
                | Expr::Mul(a, b)
                | Expr::Div(a, b)
                | Expr::Pow(a, b)
                | Expr::Cmp(_, a, b) => {
                    walk(a, out);
                    walk(b, out);
                }
                Expr::Call(_, args) => args.iter().for_each(|arg| walk(arg, out)),
                Expr::If(c, a, b) => {
                    walk(c, out);
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        let mut set = BTreeSet::new();
        walk(&self.expr, &mut set);
        set
    }
}

/// **Always complete** result of an evaluation: `values` has the requested length
/// `target_len`, whether the evaluation succeeded or not — degraded to
/// `NaN` everywhere on error (invariant 10). `error` carries the reason,
/// so that the caller (the orchestrator of `stokesforge-session`, tomorrow the UI)
/// knows there is a problem and which, rather than discovering a
/// silently empty column.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaEvaluation {
    /// Computed values, or `NaN` everywhere if `error.is_some()`.
    pub values: Vec<f64>,
    /// Row-by-row status, parallel to `values` (same length), which
    /// distinguishes a cell with nothing to compute
    /// ([`NumericStatus::Missing`]) from a real computation failure due to a
    /// non-numeric text in a referenced column ([`NumericStatus::Failed`])
    /// — see [`ColumnValues::status_at`]. Always `Missing` everywhere if
    /// `error.is_some()`: a formula error (column absent from the
    /// sheet, ...) degrades everything as a block, with no row-by-row
    /// distinction to make.
    pub statuses: Vec<NumericStatus>,
    /// `None` if the evaluation succeeded.
    pub error: Option<FormulaError>,
}

impl FormulaEvaluation {
    fn degraded(target_len: usize, error: FormulaError) -> Self {
        Self {
            values: vec![f64::NAN; target_len],
            statuses: vec![NumericStatus::Missing; target_len],
            error: Some(error),
        }
    }
}

/// Evaluates `formula` row by row against the columns of a sheet.
///
/// `columns` are the columns of the sheet, each with its identifier:
/// a column read is looked up **by identifier** in this list, never
/// by position, and never elsewhere (a column of another sheet is not
/// in it). `target_len` is the wanted length of the result, chosen by
/// the caller (the actor takes the longest of the columns read, #194) — it
/// may exceed the length of some referenced columns:
/// [`ColumnValues::get`] already returns `NaN`/`None` beyond their own
/// length, so a shorter column naturally pads with `NaN`, without
/// it being an error.
///
/// A column read that is not in `columns` degrades the **whole** result
/// to `NaN` ([`FormulaError::DeletedColumn`]) rather than computing
/// partially: a formula with a single bad reference has
/// no partially reliable value to return. Self-reference is not
/// judged here: input refuses it, and a formula that reads itself in
/// a crafted file is a cycle for the orchestrator.
pub fn evaluate(
    formula: &Formula,
    columns: &[(NodeId, ColumnValues)],
    target_len: usize,
) -> FormulaEvaluation {
    let mut resolved: BTreeMap<NodeId, &ColumnValues> = BTreeMap::new();
    for id in formula.referenced_columns() {
        match columns.iter().find(|(column, _)| *column == id) {
            None => {
                return FormulaEvaluation::degraded(target_len, FormulaError::DeletedColumn);
            }
            // A **mixed** column — including an entirely
            // text column, there is no longer a separate homogeneous `Text` type — is
            // accepted: it carries numbers, just not
            // necessarily on every row. Its text and
            // empty cells are `NaN` (`ColumnValues::number_at`), exactly
            // like a missing value of a numeric column — degrade
            // the row, not the whole formula (invariant 10).
            Some((_, values)) => {
                resolved.insert(id, values);
            }
        }
    }
    let (values, statuses) = (0..target_len)
        .map(|row| eval_expr(&formula.expr, row, &resolved))
        .unzip();
    FormulaEvaluation {
        values,
        statuses,
        error: None,
    }
}

/// Evaluates `expr` at row `row`, returning both the floating-point value
/// (ordinary IEEE arithmetic, unchanged) and the combined [`NumericStatus`]
/// of its operands — **`Failed` always wins over `Missing`**
/// ([`NumericStatus::combine`]): a non-numeric text anywhere in the
/// expression must show, even drowned in an addition with an empty
/// cell.
fn eval_expr(
    expr: &Expr,
    row: usize,
    columns: &BTreeMap<NodeId, &ColumnValues>,
) -> (f64, NumericStatus) {
    match expr {
        Expr::Number(value) => (*value, NumericStatus::Ok),
        // `number_at`/`status_at` rather than `get`: the read without
        // allocation, which already returns `NaN` for anything that is not a
        // number (text or empty cell of a mixed column, row out of
        // bounds). `get` would clone a `String` per row only to throw it away.
        // A column absent from `columns` cannot happen
        // here (`evaluate` refused it): `NaN`, never a panic.
        Expr::Column(id) => match columns.get(id) {
            Some(values) => (values.number_at(row), values.status_at(row)),
            None => (f64::NAN, NumericStatus::Missing),
        },
        Expr::Neg(inner) => {
            let (value, status) = eval_expr(inner, row, columns);
            (-value, status)
        }
        Expr::Add(a, b) => combine(a, b, row, columns, |x, y| x + y),
        Expr::Sub(a, b) => combine(a, b, row, columns, |x, y| x - y),
        Expr::Mul(a, b) => combine(a, b, row, columns, |x, y| x * y),
        Expr::Div(a, b) => combine(a, b, row, columns, |x, y| x / y),
        Expr::Pow(a, b) => {
            let (value, status) = combine(a, b, row, columns, |x, y| Func::Pow.apply(&[x, y]));
            domain_checked(nan_unless_ok(value, status), status)
        }
        Expr::Cmp(op, a, b) => {
            let (value, status) = combine(a, b, row, columns, |x, y| {
                if x.is_nan() || y.is_nan() {
                    f64::NAN
                } else if op.test(x, y) {
                    1.0
                } else {
                    0.0
                }
            });
            (nan_unless_ok(value, status), status)
        }
        Expr::Call(func, args) => {
            let mut status = NumericStatus::Ok;
            let values: Vec<f64> = args
                .iter()
                .map(|arg| {
                    let (value, arg_status) = eval_expr(arg, row, columns);
                    status = status.combine(arg_status);
                    value
                })
                .collect();
            domain_checked(nan_unless_ok(func.apply(&values), status), status)
        }
        // Only the chosen branch is evaluated, status included: an empty cell
        // in the other branch does not matter.
        Expr::If(cond, then, otherwise) => {
            let (c, c_status) = eval_expr(cond, row, columns);
            if c.is_nan() {
                (f64::NAN, c_status)
            } else if c != 0.0 {
                eval_expr(then, row, columns)
            } else {
                eval_expr(otherwise, row, columns)
            }
        }
    }
}

/// A function of an empty or failed operand is empty (`pow(NaN, 0)` would be
/// 1 for `libm`).
fn nan_unless_ok(value: f64, status: NumericStatus) -> f64 {
    if status == NumericStatus::Ok {
        value
    } else {
        f64::NAN
    }
}

/// A `NaN` out of operands that were all fine is an invalid domain
/// (`ln(-1)`, `sqrt(-1)`, `(-8)^(1/3)`): the row failed, it is not merely
/// empty.
fn domain_checked(value: f64, status: NumericStatus) -> (f64, NumericStatus) {
    if value.is_nan() && status == NumericStatus::Ok {
        (value, NumericStatus::Failed)
    } else {
        (value, status)
    }
}

/// Factor common to the four binary operators: computes each operand
/// (value and status), combines the values with `op` (unchanged IEEE
/// arithmetic) and the statuses with [`NumericStatus::combine`].
fn combine(
    a: &Expr,
    b: &Expr,
    row: usize,
    columns: &BTreeMap<NodeId, &ColumnValues>,
    op: impl Fn(f64, f64) -> f64,
) -> (f64, NumericStatus) {
    let (av, astatus) = eval_expr(a, row, columns);
    let (bv, bstatus) = eval_expr(b, row, columns);
    (op(av, bv), astatus.combine(bstatus))
}

/// Orders a set of nodes by dependency (Kahn's topological sort),
/// **without ever looping forever** — it is the generic tool the
/// recalculation orchestrator (`stokesforge-session::recalc`) uses to
/// recalculate a formula column only once all those it depends on are
/// already up to date, and to detect a cycle between formula columns without risk
/// of an infinite loop.
///
/// `edges[k]` = the set of nodes that `k` must see already resolved before
/// itself. A value of `edges[k]` that is **not** itself a key of
/// `edges` is not counted as a dependency to order: this is the case
/// of a reference to an ordinary column (already available, nothing to
/// wait for), not to be confused with a dependency between two
/// formula columns.
///
/// Returns `(order, blocked)`: `order` contains each key of `edges`
/// **exactly once**, in an order where every dependency precedes its
/// dependent, for every node not involved in any cycle;
/// `blocked` contains the rest — the nodes of a cycle **and** every node that
/// depends on one, directly or transitively, since its computation order
/// is not definable anyway as long as the cycle is not resolved.
pub fn topological_order<K: Ord + Copy>(edges: &BTreeMap<K, BTreeSet<K>>) -> (Vec<K>, BTreeSet<K>) {
    let mut in_degree: BTreeMap<K, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<K, Vec<K>> = BTreeMap::new();
    for (&node, deps) in edges {
        let relevant = deps.iter().filter(|dep| edges.contains_key(dep)).count();
        in_degree.insert(node, relevant);
        for dep in deps {
            if edges.contains_key(dep) {
                dependents.entry(*dep).or_default().push(node);
            }
        }
    }

    let mut queue: VecDeque<K> = in_degree
        .iter()
        .filter(|&(_, &degree)| degree == 0)
        .map(|(&node, _)| node)
        .collect();

    let mut order = Vec::with_capacity(edges.len());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        if let Some(waiting) = dependents.get(&node) {
            for &dependent in waiting {
                if let Some(degree) = in_degree.get_mut(&dependent) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent);
                    }
                }
            }
        }
    }

    let resolved: BTreeSet<K> = order.iter().copied().collect();
    let blocked: BTreeSet<K> = edges
        .keys()
        .copied()
        .filter(|k| !resolved.contains(k))
        .collect();
    (order, blocked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::MixedCell;
    use std::sync::Arc;

    /// Identifier of the column at position `index` in the test sheet
    /// (`A` = 1, `B` = 2, ...).
    fn col(index: u64) -> NodeId {
        NodeId::from_raw(index + 1)
    }

    /// Test sheet: thirty columns (`A` to `AD`); the column that carries
    /// the formula is outside the sheet (`OWN`), so that input refuses nothing there.
    fn sheet() -> Vec<NodeId> {
        (0..30).map(col).collect()
    }

    const OWN: NodeId = NodeId::from_raw(999);

    /// Enters `src` as letters on the test sheet and compiles it — the
    /// user's path. A syntax error comes out of `compile`.
    fn parse(src: &str) -> Result<Formula, FormulaError> {
        ColumnFormula::from_letters(src, &sheet(), OWN)?.compile()
    }

    /// The values of the sheet's columns, in order, with their
    /// identifier (`A` = 1, ...).
    fn sheet_values(values: &[ColumnValues]) -> Vec<(NodeId, ColumnValues)> {
        values
            .iter()
            .enumerate()
            .map(|(index, values)| (col(index as u64), values.clone()))
            .collect()
    }

    fn float_column(values: &[f64]) -> ColumnValues {
        ColumnValues::Float(Arc::new(values.to_vec()))
    }

    fn text_column(values: &[Option<&str>]) -> ColumnValues {
        ColumnValues::Mixed(Arc::new(
            values
                .iter()
                .map(|v| match v {
                    None => MixedCell::Empty,
                    Some(text) => MixedCell::Text(text.to_string()),
                })
                .collect(),
        ))
    }

    /// Test mixed column: `None` = empty, `Some(Ok(n))` = number,
    /// `Some(Err(text))` = non-numeric text.
    fn mixed_column(cells: &[Option<Result<f64, &str>>]) -> ColumnValues {
        use crate::column::MixedCell;
        ColumnValues::Mixed(Arc::new(
            cells
                .iter()
                .map(|cell| match cell {
                    None => MixedCell::Empty,
                    Some(Ok(value)) => MixedCell::Number(*value),
                    Some(Err(text)) => MixedCell::Text((*text).to_string()),
                })
                .collect(),
        ))
    }

    // -- parsing --

    #[test]
    fn parses_simple_addition() {
        let formula = parse("A+B").expect("parse");
        assert_eq!(
            formula.referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
    }

    #[test]
    fn parses_parentheses_and_precedence() {
        let formula = parse("(A-B)*2").expect("parse");
        let columns = [float_column(&[10.0]), float_column(&[3.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert_eq!(result.values, vec![14.0]);
        assert_eq!(result.error, None);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let formula = parse("A+B*2").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[3.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert_eq!(result.values, vec![7.0]);
    }

    #[test]
    fn parses_division() {
        let formula = parse("A/B").expect("parse");
        let columns = [float_column(&[10.0]), float_column(&[4.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert_eq!(result.values, vec![2.5]);
    }

    #[test]
    fn parses_multi_letter_column_and_unary_minus() {
        let formula = parse("-AA + 1").expect("parse");
        assert_eq!(formula.referenced_columns(), BTreeSet::from([col(26)]));
    }

    #[test]
    fn column_letters_are_case_insensitive() {
        let formula = parse("a+b").expect("parse");
        assert_eq!(
            formula.referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
    }

    #[test]
    fn ignores_whitespace() {
        let formula = parse("  A   +  B  ").expect("parse");
        assert_eq!(
            formula.referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
    }

    #[test]
    fn rejects_empty_formula() {
        assert!(matches!(parse(""), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn rejects_unmatched_parenthesis() {
        assert!(matches!(parse("(A+B"), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(matches!(parse("A+B)"), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn rejects_unknown_character() {
        assert!(matches!(parse("A+B#2"), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn rejects_dangling_operator() {
        assert!(matches!(parse("A+"), Err(FormulaError::Parse(_))));
    }

    // -- evaluation, degradation --

    #[test]
    fn evaluate_degrades_to_nan_on_parse_failure_when_caller_uses_parse_first() {
        // This is the contract that `evaluate` documents: a formula that does not
        // parse never reaches `evaluate` (the caller keeps
        // the `Err(FormulaError::Parse)` of `Formula::parse`); this test pins
        // only that `Formula::parse` does fail for this case, the
        // NaN degradation being tested by the orchestrator (stokesforge-session).
        assert!(parse("A@B").is_err());
    }

    #[test]
    fn missing_column_pads_with_nan() {
        let formula = parse("A+B").expect("parse");
        let columns = [float_column(&[1.0, 2.0, 3.0]), float_column(&[10.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 3);
        assert_eq!(result.error, None);
        assert_eq!(result.values[0], 11.0);
        assert!(result.values[1].is_nan());
        assert!(result.values[2].is_nan());
    }

    #[test]
    fn a_column_absent_from_the_sheet_degrades_everything_to_nan() {
        // `C` is in the input sheet, but not in the columns
        // supplied to the evaluation (deleted, or in another sheet).
        let formula = parse("A+C").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[2.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 3);
        assert_eq!(result.error, Some(FormulaError::DeletedColumn));
        assert!(result.values.iter().all(|v| v.is_nan()));
        assert_eq!(result.values.len(), 3);
    }

    #[test]
    fn evaluate_looks_columns_up_by_identifier_not_by_position() {
        // The permuted sheet: `B` (2) first. The formula `A*2` reads
        // identifier 1, which is no longer at position 0.
        let formula = parse("A*2").expect("parse");
        let permuted = [
            (col(1), float_column(&[100.0])),
            (col(0), float_column(&[5.0])),
        ];
        let result = evaluate(&formula, &permuted, 1);
        assert_eq!(result.error, None);
        assert_eq!(result.values, vec![10.0]);
    }

    #[test]
    fn a_column_with_text_cells_degrades_only_the_affected_rows() {
        // There is no longer a homogeneous `Text` column type to refuse as
        // a block: a column some of whose cells are text is a
        // `Mixed` column, accepted by the formula — only the rows whose
        // referenced cell is not a number become `NaN`, not
        // the whole formula (invariant 10).
        let formula = parse("A+B").expect("parse");
        let columns = [
            float_column(&[1.0, 2.0]),
            text_column(&[Some("10"), Some("x")]),
        ];
        let result = evaluate(&formula, &sheet_values(&columns), 2);
        assert_eq!(result.error, None);
        assert!(
            result.values[0].is_nan(),
            "\"10\" as text is never reparsed as a number"
        );
        assert!(result.values[1].is_nan());
    }

    #[test]
    fn division_by_zero_is_ieee_not_an_error() {
        // Ordinary floating-point behavior, not a degradation case
        // specific to the module: 0/0 = NaN, x/0 = ±inf, exactly like
        // any missing or non-finite value already tolerated by the model.
        let formula = parse("A/B").expect("parse");
        let columns = [float_column(&[1.0, 0.0]), float_column(&[0.0, 0.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 2);
        assert_eq!(result.error, None);
        assert!(result.values[0].is_infinite());
        assert!(result.values[1].is_nan());
    }

    // -- row-by-row status (Ok / Missing / Failed) --

    #[test]
    fn numeric_status_combine_failed_always_wins() {
        assert_eq!(
            NumericStatus::Failed.combine(NumericStatus::Missing),
            NumericStatus::Failed
        );
        assert_eq!(
            NumericStatus::Missing.combine(NumericStatus::Failed),
            NumericStatus::Failed
        );
        assert_eq!(
            NumericStatus::Failed.combine(NumericStatus::Ok),
            NumericStatus::Failed
        );
        assert_eq!(
            NumericStatus::Missing.combine(NumericStatus::Ok),
            NumericStatus::Missing
        );
        assert_eq!(
            NumericStatus::Ok.combine(NumericStatus::Ok),
            NumericStatus::Ok
        );
    }

    #[test]
    fn evaluate_tags_failed_and_missing_row_by_row() {
        // The task's example: A = [1,2,3,4,""], B = [1,1,"oops",1,1],
        // C = A+B must return, row by row: Ok(2), Ok(3), Failed, Ok(5),
        // Missing (A empty, B present: the result cell still exists
        // with the value of B, as today — only the status is
        // new, not the value).
        let formula = parse("A+B").expect("parse");
        let a = float_column(&[1.0, 2.0, 3.0, 4.0, f64::NAN]);
        let b = mixed_column(&[
            Some(Ok(1.0)),
            Some(Ok(1.0)),
            Some(Err("oops")),
            Some(Ok(1.0)),
            Some(Ok(1.0)),
        ]);
        let columns = [a, b];
        let result = evaluate(&formula, &sheet_values(&columns), 5);
        assert_eq!(result.error, None);

        assert_eq!(result.values[0], 2.0);
        assert_eq!(result.statuses[0], NumericStatus::Ok);

        assert_eq!(result.values[1], 3.0);
        assert_eq!(result.statuses[1], NumericStatus::Ok);

        assert!(result.values[2].is_nan());
        assert_eq!(result.statuses[2], NumericStatus::Failed);

        assert_eq!(result.values[3], 5.0);
        assert_eq!(result.statuses[3], NumericStatus::Ok);

        // A missing (NaN), B = 1.0: the value follows the propagation
        // convention already in place (NaN + 1.0 = NaN), and the status is
        // Missing (not Failed: nothing failed, an operand is just
        // missing).
        assert!(result.values[4].is_nan());
        assert_eq!(result.statuses[4], NumericStatus::Missing);
    }

    #[test]
    fn evaluate_status_is_ok_for_plain_arithmetic() {
        let formula = parse("A*B").expect("parse");
        let columns = [float_column(&[2.0]), float_column(&[3.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert_eq!(result.values, vec![6.0]);
        assert_eq!(result.statuses, vec![NumericStatus::Ok]);
    }

    #[test]
    fn evaluate_status_is_missing_when_both_operands_are_missing() {
        let formula = parse("A+B").expect("parse");
        let columns = [mixed_column(&[None]), mixed_column(&[None])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses[0], NumericStatus::Missing);
    }

    #[test]
    fn evaluate_status_failed_wins_over_missing() {
        // A empty, B non-numeric text: Failed must win over
        // Missing (never a real failure hidden behind an empty cell).
        let formula = parse("A+B").expect("parse");
        let columns = [mixed_column(&[None]), mixed_column(&[Some(Err("x"))])];
        let result = evaluate(&formula, &sheet_values(&columns), 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses[0], NumericStatus::Failed);
    }

    #[test]
    fn degraded_evaluation_reports_missing_status_everywhere() {
        let formula = parse("A+C").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[2.0])];
        let result = evaluate(&formula, &sheet_values(&columns), 3);
        assert!(result.error.is_some());
        assert_eq!(result.statuses, vec![NumericStatus::Missing; 3]);
    }

    // -- letters --

    #[test]
    fn letters_round_trip() {
        for index in [0usize, 1, 25, 26, 27, 51, 52, 701, 702] {
            let letters = index_to_letters(index);
            assert_eq!(
                letters_to_index(&letters),
                Some(index),
                "index {index} -> {letters}"
            );
        }
    }

    // -- input and display: letters <-> identifiers --

    fn abc() -> [NodeId; 3] {
        [col(0), col(1), col(2)]
    }

    #[test]
    fn from_letters_keeps_the_typed_text_around_the_columns() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("A+B*2", &[a, b, c], c).expect("input");
        assert_eq!(
            formula.parts(),
            [
                FormulaPart::Column(a),
                FormulaPart::Text("+".into()),
                FormulaPart::Column(b),
                FormulaPart::Text("*2".into()),
            ]
        );
        assert_eq!(formula.letters(&[a, b, c]), "A+B*2");
        // Spaces and parentheses are kept as typed.
        let spaced = ColumnFormula::from_letters(" ( a - B ) ", &[a, b, c], c).expect("input");
        assert_eq!(spaced.letters(&[a, b, c]), " ( A - B ) ");
    }

    #[test]
    fn from_letters_refuses_a_letter_that_names_no_column() {
        let sheet = [col(0), col(1), col(2), col(3)];
        assert_eq!(
            ColumnFormula::from_letters("E*2", &sheet, OWN),
            Err(FormulaError::UnknownColumn("E".into()))
        );
        // An absurdly long sequence of letters names nothing either, without a panic.
        let long = "Z".repeat(40);
        assert_eq!(
            ColumnFormula::from_letters(&long, &sheet, OWN),
            Err(FormulaError::UnknownColumn(long.clone()))
        );
    }

    #[test]
    fn from_letters_refuses_the_column_that_carries_the_formula() {
        let [a, b, c] = abc();
        assert_eq!(
            ColumnFormula::from_letters("C*2", &[a, b, c], c),
            Err(FormulaError::SelfReference("C".into()))
        );
        // The same input on another column is accepted.
        assert!(ColumnFormula::from_letters("C*2", &[a, b, c], a).is_ok());
    }

    #[test]
    fn from_letters_refuses_the_ref_token() {
        let [a, b, c] = abc();
        for text in ["#REF*2", "A+#ref", "#REF"] {
            assert_eq!(
                ColumnFormula::from_letters(text, &[a, b, c], c),
                Err(FormulaError::DeletedColumn),
                "{text}"
            );
        }
        // A `#` that is not `#REF` is not refused here: syntax error.
        let stored = ColumnFormula::from_letters("A#2", &[a, b, c], c).expect("stored");
        assert!(matches!(stored.compile(), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn from_letters_does_not_read_an_exponent_as_a_column() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("1e-3*A", &[a, b, c], c).expect("input");
        assert_eq!(formula.columns(), BTreeSet::from([a]));
        assert_eq!(formula.letters(&[a, b, c]), "1e-3*A");
        let compiled = formula.compile().expect("compile");
        assert_eq!(compiled.referenced_columns(), BTreeSet::from([a]));
        // An `e` that does not open a complete exponent is a column letter.
        let letter =
            ColumnFormula::from_letters("1e*A", &[a, b, c, col(3), col(4)], c).expect("input");
        assert_eq!(letter.columns(), BTreeSet::from([a, col(4)]));
    }

    #[test]
    fn a_syntax_error_is_stored_and_reported_by_compile() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("A*", &[a, b, c], c).expect("stored");
        assert!(matches!(formula.compile(), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn letters_follow_the_columns_when_the_sheet_is_permuted() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("A+B*2", &[a, b, c], c).expect("input");
        // B moves to the front: the stored text does not change, the letters do.
        assert_eq!(formula.letters(&[b, a, c]), "B+A*2");
        assert_eq!(formula.parts().len(), 4);
        assert!(!formula.has_deleted_reference(&[b, a, c]));
    }

    #[test]
    fn a_column_outside_the_sheet_is_written_ref() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("A+B*2", &[a, b, c], c).expect("input");
        assert_eq!(formula.letters(&[b, c]), "#REF+A*2");
        assert!(formula.has_deleted_reference(&[b, c]));
        assert_eq!(formula.letters(&[]), "#REF+#REF*2");
        // The identifier stays stored: the column that comes back brings the reading back.
        assert_eq!(formula.columns(), BTreeSet::from([a, b]));
        assert_eq!(formula.letters(&[a, b, c]), "A+B*2");
    }

    #[test]
    fn the_stored_fragments_compile_even_when_the_displayed_letters_would_not() {
        // `2C3`: if C became E, the display would be `2E3` (a number), but
        // it is the fragments that compile: never a return through the
        // letters.
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Text("2".into()),
            FormulaPart::Column(a),
            FormulaPart::Text("3".into()),
        ]);
        assert_eq!(formula.letters(&[b, c, col(3), col(4), a]), "2E3");
        assert!(matches!(formula.compile(), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn letters_in_a_text_fragment_are_a_syntax_error_not_a_column() {
        // Crafted file (invariant 10): never a panic, never a column.
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Text("A+".into()),
            FormulaPart::Column(col(1)),
        ]);
        assert!(matches!(formula.compile(), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn remap_renumbers_the_columns_and_keeps_the_text() {
        let [a, b, c] = abc();
        let formula = ColumnFormula::from_letters("A+B*2", &[a, b, c], c).expect("input");
        let moved = formula.remap(|id| NodeId::from_raw(id.raw() + 100));
        assert_eq!(
            moved.parts(),
            [
                FormulaPart::Column(NodeId::from_raw(101)),
                FormulaPart::Text("+".into()),
                FormulaPart::Column(NodeId::from_raw(102)),
                FormulaPart::Text("*2".into()),
            ]
        );
    }

    // -- topological order / cycles --

    #[test]
    fn topological_order_respects_dependencies() {
        // C depends on B, B depends on A: A must come out before B, B before C.
        let mut edges = BTreeMap::new();
        edges.insert("C", BTreeSet::from(["B"]));
        edges.insert("B", BTreeSet::from(["A"]));
        edges.insert("A", BTreeSet::new());
        let (order, blocked) = topological_order(&edges);
        assert_eq!(order, vec!["A", "B", "C"]);
        assert!(blocked.is_empty());
    }

    #[test]
    fn topological_order_detects_direct_cycle() {
        // A depends on B, B depends on A.
        let mut edges = BTreeMap::new();
        edges.insert("A", BTreeSet::from(["B"]));
        edges.insert("B", BTreeSet::from(["A"]));
        let (order, blocked) = topological_order(&edges);
        assert!(order.is_empty());
        assert_eq!(blocked, BTreeSet::from(["A", "B"]));
    }

    #[test]
    fn topological_order_blocks_transitive_dependents_of_a_cycle() {
        // A <-> B (cycle), C depends on B.
        let mut edges = BTreeMap::new();
        edges.insert("A", BTreeSet::from(["B"]));
        edges.insert("B", BTreeSet::from(["A"]));
        edges.insert("C", BTreeSet::from(["B"]));
        let (order, blocked) = topological_order(&edges);
        assert!(order.is_empty());
        assert_eq!(blocked, BTreeSet::from(["A", "B", "C"]));
    }

    #[test]
    fn topological_order_ignores_references_to_non_formula_columns() {
        // "A" depends on "X", which is not a key of `edges` (ordinary
        // column, not a formula column): it is not a dependency
        // to order, "A" waits for nothing.
        let mut edges = BTreeMap::new();
        edges.insert("A", BTreeSet::from(["X"]));
        let (order, blocked) = topological_order(&edges);
        assert_eq!(order, vec!["A"]);
        assert!(blocked.is_empty());
    }

    #[test]
    fn topological_order_never_loops_on_self_dependency() {
        let mut edges = BTreeMap::new();
        edges.insert("A", BTreeSet::from(["A"]));
        let (order, blocked) = topological_order(&edges);
        assert!(order.is_empty());
        assert_eq!(blocked, BTreeSet::from(["A"]));
    }

    // -- depth cap (#289) --

    /// Runs `f` on a 2 MiB thread (that of the session actor);
    /// in debug, it is the tightest case.
    fn on_2_mib_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn too_deep(src: String) -> bool {
        on_2_mib_stack(move || parse(&src) == Err(FormulaError::TooDeep))
    }

    #[test]
    fn a_deep_formula_is_refused_without_overflowing_the_stack() {
        // Shapes that aborted the process before the cap.
        assert!(too_deep(format!(
            "{}A{}",
            "(".repeat(100_000),
            ")".repeat(100_000)
        )));
        assert!(too_deep(format!("{}A", "A+".repeat(100_000))));
        assert!(too_deep(format!("{}A", "A*".repeat(100_000))));
        assert!(too_deep(format!("{}A", "-".repeat(100_000))));
    }

    #[test]
    fn a_formula_just_under_the_cap_is_accepted_and_evaluated() {
        let v = on_2_mib_stack(|| {
            let columns = [float_column(&[2.0])];
            let sum = parse(&format!("{}A", "A+".repeat(MAX_DEPTH - 2))).unwrap();
            let n = MAX_DEPTH / NEST_COST - 1;
            let parens = parse(&format!("{}A{}", "(".repeat(n), ")".repeat(n))).unwrap();
            let neg = parse(&format!("{}A", "-".repeat(n))).unwrap();
            let val = |f: &Formula| {
                let r = evaluate(f, &sheet_values(&columns), 1);
                assert_eq!(r.error, None);
                r.values[0]
            };
            (val(&sum), val(&parens), val(&neg))
        });
        assert_eq!(v, (2.0 * (MAX_DEPTH - 1) as f64, 2.0, -2.0));
    }

    // -- cap hardening (#289): limit sizes hard-coded, never
    // derived from `MAX_DEPTH`, so that a change of the cap or the costs
    // breaks these tests instead of following them. `FormulaError::TooDeep` is a
    // variant with no field: there is no position to check (unlike
    // `ExprErrorKind::TooDeep` of `analysis::expr`).

    /// Runs `f` on a thread whose stack is `bytes` long; on
    /// overflow the whole process aborts (no recoverable panic).
    fn on_stack<R: Send + 'static>(bytes: usize, f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    const ONE_MIB: usize = 1 << 20;
    const TWO_MIB: usize = 2 << 20;

    /// `n` terms `A` joined by `op` (hence `n - 1` operators).
    fn chain(op: &str, terms: usize) -> String {
        vec!["A"; terms].join(op)
    }

    fn parens(n: usize, middle: &str) -> String {
        format!("{}{middle}{}", "(".repeat(n), ")".repeat(n))
    }

    fn parse_ok(src: &str) -> bool {
        match parse(src) {
            Ok(_) => true,
            Err(FormulaError::TooDeep) => false,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn exact_cap_of_an_operator_chain() {
        // 1001 terms = 1000 operators = cost 1000: accepted; 1002: refused.
        for op in ["+", "-", "*", "/"] {
            for (terms, expected) in [(999, true), (1000, true), (1001, true), (1002, false)] {
                assert_eq!(
                    parse_ok(&chain(op, terms)),
                    expected,
                    "{terms} terms `{op}`"
                );
            }
        }
    }

    #[test]
    fn exact_cap_of_nested_parentheses() {
        for (n, expected) in [(99, true), (100, true), (101, false)] {
            assert_eq!(parse_ok(&parens(n, "A")), expected, "{n} parentheses");
        }
    }

    #[test]
    fn exact_cap_of_stacked_signs() {
        for sign in ["-", "+"] {
            for (n, expected) in [(99, true), (100, true), (101, false)] {
                let src = format!("{}A", sign.repeat(n));
                assert_eq!(parse_ok(&src), expected, "{n} signs `{sign}`");
            }
        }
        // Mixed signs: 100 signs accepted, 101 refused.
        assert!(parse_ok(&format!("{}A", "-+".repeat(50))));
        assert!(!parse_ok(&format!("{}A", "-+".repeat(50) + "-")));
        assert!(!parse_ok(&format!("{}A", "-+".repeat(51))));
    }

    #[test]
    fn costs_add_up_between_parentheses_and_chain() {
        // 99 parentheses (990) + 10 operators (10) = 1000: accepted; 11: refused.
        assert!(parse_ok(&parens(99, &chain("+", 11))));
        assert!(!parse_ok(&parens(99, &chain("+", 12))));
        // 98 parentheses (980) + 20 operators.
        assert!(parse_ok(&parens(98, &chain("*", 21))));
        assert!(!parse_ok(&parens(98, &chain("*", 22))));
        // A sign at the head of a chain costs 10 while reading `A`
        // only, then is given back: the chain keeps its 1000.
        assert!(parse_ok(&format!("-{}", chain("+", 1001))));
        assert!(!parse_ok(&format!("-{}", chain("+", 1002))));
        // Sign applied to the last term: `A+A+…+-A` -> 990 operators + 10.
        assert!(parse_ok(&format!("{}-A", "A+".repeat(990))));
        assert!(!parse_ok(&format!("{}-A", "A+".repeat(991))));
        // Likewise multiplicative: `A*-A*-…*-A`.
        assert!(parse_ok(&format!("{}A", "A*-".repeat(990))));
        assert!(!parse_ok(&format!("{}A", "A*-".repeat(991))));
    }

    #[test]
    fn depth_is_given_back_after_each_group_and_each_term() {
        // Sequential groups: 991 groups = 990 operators + 10 for the last.
        // Without `nested` giving back the 10, 101 groups would already be
        // refused.
        let groups = |n: usize| vec!["(A)"; n].join("+");
        assert!(parse_ok(&groups(991)));
        assert!(!parse_ok(&groups(992)));
        // A parenthesized sum gives back its `+` on exit: 120 groups of
        // 10 terms (9 `+` each) leave no remainder, otherwise 120 * 9 > 1000.
        let group = format!("({})", chain("+", 10));
        assert!(parse_ok(&vec![group.as_str(); 120].join("+")));
        // The terms of a product reset to zero at each `+`: 600
        // chained `A*A` products = 599 `+` + 1 `*` = 600, not 1199.
        let products = vec!["A*A"; 600].join("+");
        assert!(parse_ok(&products));
        // 500 `+` then a product of 500 `*` = 1000: right at the limit.
        let somme_puis_produit =
            |stars: usize| format!("{}+{}", chain("+", 500), chain("*", stars));
        assert!(parse_ok(&somme_puis_produit(501)));
        assert!(!parse_ok(&somme_puis_produit(502)));
        // A product of 1000 `*` followed by sums: it does not add to the `+`.
        assert!(parse_ok(&format!(
            "{}+{}",
            chain("*", 1001),
            chain("+", 500)
        )));
    }

    #[test]
    fn any_shape_without_a_declared_cost_is_refused_at_10_000() {
        let n = 10_000;
        let shapes = [
            ("closed parentheses", parens(n, "A")),
            ("parentheses never closed", "(".repeat(n)),
            ("stacked minuses", format!("{}A", "-".repeat(n))),
            ("stacked pluses", format!("{}A", "+".repeat(n))),
            ("alternating signs", format!("{}A", "-+".repeat(n / 2))),
            ("sum", chain("+", n)),
            ("difference", chain("-", n)),
            ("product", chain("*", n)),
            ("quotient", chain("/", n)),
            ("alternating sum/product", chain("+A*", n / 2) + "A"),
            (
                "right nesting",
                format!("{}A{}", "A+(".repeat(n), ")".repeat(n)),
            ),
            (
                "sign then parenthesis",
                format!("{}A{}", "-(".repeat(n), ")".repeat(n)),
            ),
            ("product of signs", format!("{}A", "A*-".repeat(n))),
        ];
        for (name, src) in shapes {
            on_stack(ONE_MIB, move || {
                assert_eq!(parse(&src), Err(FormulaError::TooDeep), "{name}");
            });
        }
    }

    #[test]
    fn a_million_elements_are_refused_fast() {
        let start = std::time::Instant::now();
        on_stack(ONE_MIB, || {
            for src in [
                "(".repeat(1_000_000),
                "-".repeat(1_000_000),
                "A+".repeat(500_000),
                parens(500_000, "A"),
            ] {
                assert_eq!(parse(&src), Err(FormulaError::TooDeep));
            }
        });
        // Linear (a few tens of ms in debug); the margin is only there
        // to catch a quadratic traversal or a full sweep.
        assert!(start.elapsed().as_secs() < 10, "{:?}", start.elapsed());
    }

    /// The worst accepted formulas, one per recursion shape: operator
    /// chain (deep tree of 1000), parentheses (parser), signs
    /// (`Neg` tree and parser), mixes at the exact cost of the cap.
    fn worst_formulas() -> Vec<(&'static str, String)> {
        vec![
            ("sum", chain("+", 1001)),
            ("difference", chain("-", 1001)),
            ("product", chain("*", 1001)),
            ("quotient", chain("/", 1001)),
            ("parentheses", parens(100, "A")),
            ("stacked minuses", format!("{}A", "-".repeat(100))),
            ("stacked pluses", format!("{}A", "+".repeat(100))),
            ("parentheses and sum", parens(99, &chain("+", 11))),
            (
                "right nesting",
                format!("{}A{}", "A+(".repeat(90), ")".repeat(90)),
            ),
            (
                "sign then parenthesis",
                format!("{}A{}", "-(".repeat(50), ")".repeat(50)),
            ),
            ("sum ending with a sign", format!("{}-A", "A+".repeat(990))),
            ("product of signs", format!("{}A", "A*-".repeat(990))),
            ("sequential groups", vec!["(A)"; 991].join("+")),
        ]
    }

    /// Parses, reads the references, evaluates, clones, compares and drops the formula.
    fn full_walk(src: &str) -> f64 {
        let formula = parse(src).expect("worst accepted formula");
        let columns = [float_column(&[2.0, 3.0])];
        let copy = formula.clone();
        assert_eq!(formula, copy);
        assert_eq!(formula.referenced_columns(), BTreeSet::from([col(0)]));
        let r = evaluate(&formula, &sheet_values(&columns), 2);
        assert_eq!(r.error, None);
        r.values[0]
    }

    #[test]
    fn the_worst_accepted_formulas_fit_in_1_mib_and_2_mib() {
        // A stack overflow aborts the process: this test does not "fail",
        // it kills the test binary, which makes it impossible to miss.
        // Margin measured in debug (stack reduced until abort, worst
        // shape = `+`/`-`/`*`/`/` chain of 1001 terms): about
        // 560 KiB are needed; 1 MiB gives a factor 1.8, 2 MiB (the actor) a factor 3.6.
        // Parentheses alone need ~425 KiB (parser), stacked
        // signs ~95 KiB.
        for bytes in [TWO_MIB, ONE_MIB] {
            for (name, src) in worst_formulas() {
                let value = on_stack(bytes, move || full_walk(&src));
                assert!(!value.is_nan(), "{name} on {bytes} bytes");
            }
        }
    }

    #[test]
    fn the_limit_formulas_have_the_right_value() {
        // A = 2.0 on the first row.
        let expected = [
            ("sum", 2.0 * 1001.0),
            ("difference", 2.0 - 2.0 * 1000.0),
            ("product", 2f64.powi(1001)),
            ("quotient", 2.0 / 2f64.powi(1000)),
            ("parentheses", 2.0),
            ("stacked minuses", 2.0), // 100 negations
            ("stacked pluses", 2.0),
            ("parentheses and sum", 22.0),
            ("right nesting", 2.0 * 91.0),
            ("sign then parenthesis", 2.0), // 50 negations
            ("sum ending with a sign", 2.0 * 990.0 - 2.0),
            ("product of signs", 2f64.powi(991)),
            ("sequential groups", 2.0 * 991.0),
        ];
        let worst = worst_formulas();
        assert_eq!(worst.len(), expected.len());
        for ((name, src), (expected_name, value)) in worst.into_iter().zip(expected) {
            assert_eq!(name, expected_name);
            let obtained = on_stack(TWO_MIB, move || full_walk(&src));
            assert_eq!(obtained, value, "{name}");
        }
        // One negation fewer: inverted sign.
        let odd = on_stack(TWO_MIB, || full_walk(&format!("{}A", "-".repeat(99))));
        assert_eq!(odd, -2.0);
    }

    #[test]
    fn ordinary_formulas_are_unchanged() {
        let columns = [float_column(&[2.0, 8.0]), float_column(&[4.0, 2.0])];
        let values = |src: &str| {
            let f = parse(src).unwrap();
            let r = evaluate(&f, &sheet_values(&columns), 2);
            assert_eq!(r.error, None, "{src}");
            r.values
        };
        assert_eq!(values("A+B"), [6.0, 10.0]);
        assert_eq!(values("(A+B)*2-1"), [11.0, 19.0]);
        // Left associativity: (10-4)-3, (100/10)/5, not 10-(4-3).
        assert_eq!(values("10-4-3"), [3.0, 3.0]);
        assert_eq!(values("100/10/5"), [2.0, 2.0]);
        assert_eq!(values("2+3*4"), [14.0, 14.0]);
        assert_eq!(values("-A"), [-2.0, -8.0]);
        assert_eq!(values("--A"), [2.0, 8.0]);
        assert_eq!(values("+A"), [2.0, 8.0]);
        assert_eq!(values("A*-B"), [-8.0, -16.0]);
        assert_eq!(values("((((A))))"), [2.0, 8.0]);
        assert_eq!(values(" A / B "), [0.5, 4.0]);
        assert_eq!(values("1e1+.5"), [10.5, 10.5]);
    }

    #[test]
    fn the_referenced_columns_of_a_limit_formula_are_exact() {
        // Chain of 1001 terms cycling over A, B, C and AA.
        let letters = ["A", "B", "C", "AA"];
        let terms: Vec<&str> = (0..1001).map(|i| letters[i % 4]).collect();
        let src = terms.join("+");
        let refs = on_stack(ONE_MIB, move || parse(&src).unwrap().referenced_columns());
        assert_eq!(refs, BTreeSet::from([col(0), col(1), col(2), col(26)]));
        // A single reference at the very bottom of a limit stack.
        let deep = format!("{}C{}", "(-".repeat(50), ")".repeat(50));
        let refs = on_stack(ONE_MIB, move || parse(&deep).unwrap().referenced_columns());
        assert_eq!(refs, BTreeSet::from([col(2)]));
        // Nothing to reference: empty set.
        let refs = on_stack(ONE_MIB, || {
            parse(&vec!["1"; 1001].join("+"))
                .unwrap()
                .referenced_columns()
        });
        assert!(refs.is_empty());
    }

    #[test]
    fn a_too_deep_formula_degrades_evaluate_after_a_refused_parse() {
        // `evaluate` receives a `Formula` only after `parse`: a refusal
        // leaves no formula to evaluate; the error is that of the parse.
        let error = parse(&chain("+", 1002)).unwrap_err();
        assert_eq!(error, FormulaError::TooDeep);
        assert_eq!(error.to_string(), "formula nested too deeply");
    }

    // -- functions, `^`, comparisons, `if` (#401) --

    /// A mixed column of numbers, `None` = empty cell.
    fn opt_column(values: &[Option<f64>]) -> ColumnValues {
        let cells: Vec<Option<Result<f64, &str>>> = values.iter().map(|v| v.map(Ok)).collect();
        mixed_column(&cells)
    }

    /// Enters `src`, evaluates it on `columns` over `len` rows.
    fn run(src: &str, columns: &[ColumnValues], len: usize) -> FormulaEvaluation {
        let formula = parse(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        evaluate(&formula, &sheet_values(columns), len)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn power_is_right_associative() {
        assert_eq!(run("2^3^2", &[], 1).values, vec![512.0]);
        assert_eq!(run("2^-1", &[], 1).values, vec![0.5]);
    }

    #[test]
    fn power_binds_tighter_than_the_unary_sign() {
        assert_eq!(run("-2^2", &[], 1).values, vec![-4.0]);
        assert_eq!(run("2*3^2", &[], 1).values, vec![18.0]);
    }

    #[test]
    fn a_function_reads_its_columns() {
        let formula = parse("sqrt(A)").unwrap();
        assert_eq!(formula.referenced_columns(), BTreeSet::from([col(0)]));
        let result = run("sqrt(A)", &[float_column(&[16.0, 2.25])], 2);
        assert_eq!(result.values, vec![4.0, 1.5]);
        assert_eq!(result.statuses, vec![NumericStatus::Ok; 2]);
        // Every node walks: power, comparison, `if`, several arguments.
        assert_eq!(
            parse("A^B").unwrap().referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
        assert_eq!(
            parse("A<B").unwrap().referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
        assert_eq!(
            parse("if(A>0,B,C)").unwrap().referenced_columns(),
            BTreeSet::from([col(0), col(1), col(2)])
        );
        assert_eq!(
            parse("max(A, B)").unwrap().referenced_columns(),
            BTreeSet::from([col(0), col(1)])
        );
    }

    #[test]
    fn root_keeps_the_real_odd_root_and_refuses_the_even_one() {
        let result = run("root(A,3)", &[float_column(&[-8.0])], 1);
        assert_eq!(result.values, vec![-2.0]);
        let result = run("root(-8,2)", &[], 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses, vec![NumericStatus::Failed]);
    }

    #[test]
    fn an_invalid_domain_is_nan_and_failed() {
        let result = run("ln(A)", &[float_column(&[-1.0, 1.0])], 2);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses[0], NumericStatus::Failed);
        assert_eq!(result.values[1], 0.0);
        assert_eq!(result.statuses[1], NumericStatus::Ok);
        let result = run("(-8)^(1/3)", &[], 1);
        assert_eq!(result.statuses, vec![NumericStatus::Failed]);
    }

    #[test]
    fn an_empty_operand_gives_a_missing_result() {
        let result = run("sqrt(A)", &[opt_column(&[None, Some(4.0)])], 2);
        assert!(result.values[0].is_nan());
        assert_eq!(
            result.statuses,
            vec![NumericStatus::Missing, NumericStatus::Ok]
        );
        // `pow(NaN, 0)` is 1 for libm: an empty operand must still win.
        let result = run("A^0", &[opt_column(&[None])], 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses, vec![NumericStatus::Missing]);
    }

    #[test]
    fn comparisons_give_one_or_zero_and_nan_stays_nan() {
        let a = opt_column(&[Some(-1.0), Some(0.0), Some(2.0), None]);
        let result = run("A<0", std::slice::from_ref(&a), 4);
        assert_eq!(result.values[..3], [1.0, 0.0, 0.0]);
        assert!(result.values[3].is_nan());
        assert_eq!(result.statuses[3], NumericStatus::Missing);
        for (src, want) in [
            ("A<=0", [1.0, 1.0, 0.0]),
            ("A>0", [0.0, 0.0, 1.0]),
            ("A>=0", [0.0, 1.0, 1.0]),
            ("A==0", [0.0, 1.0, 0.0]),
            ("A!=0", [1.0, 0.0, 1.0]),
        ] {
            assert_eq!(run(src, std::slice::from_ref(&a), 3).values, want, "{src}");
        }
        // A NaN that is not an empty cell (0/0) stays NaN in a comparison.
        assert!(run("0/0<1", &[], 1).values[0].is_nan());
        assert!(run("0/0==0/0", &[], 1).values[0].is_nan());
        // Comparisons are the loosest operator.
        assert_eq!(run("1+1==2", &[], 1).values, vec![1.0]);
    }

    #[test]
    fn if_picks_a_branch_and_keeps_an_empty_cell() {
        let a = opt_column(&[Some(-1.0), Some(2.0), None]);
        let result = run("if(A<0,0,A)", std::slice::from_ref(&a), 3);
        assert_eq!(result.values[..2], [0.0, 2.0]);
        assert!(result.values[2].is_nan());
        assert_eq!(result.statuses[2], NumericStatus::Missing);
    }

    #[test]
    fn if_reads_the_status_of_the_chosen_branch_only() {
        // A > 0 is true: C is empty but not chosen.
        let columns = [
            float_column(&[1.0]),
            float_column(&[5.0]),
            opt_column(&[None]),
        ];
        let result = run("if(A>0,B,C)", &columns, 1);
        assert_eq!(result.values, vec![5.0]);
        assert_eq!(result.statuses, vec![NumericStatus::Ok]);
        // And the other way round: the empty branch is chosen.
        let result = run("if(A<0,B,C)", &columns, 1);
        assert_eq!(result.statuses, vec![NumericStatus::Missing]);
    }

    #[test]
    fn round_follows_excel() {
        let a = [float_column(&[7.14159])];
        assert_eq!(run("round(A,2)", &a, 1).values, vec![7.14]);
        assert_eq!(run("round(-2.5,0)", &[], 1).values, vec![-3.0]);
        assert_eq!(run("round(1234,-2)", &[], 1).values, vec![1200.0]);
        assert_eq!(run("mod(-7,3)", &[], 1).values, vec![2.0]);
    }

    #[test]
    fn minus_log_gives_the_absorbance() {
        let result = run("-log(A)", &[float_column(&[0.1])], 1);
        assert!(close(result.values[0], 1.0));
    }

    #[test]
    fn pi_is_a_call() {
        assert!(close(run("pi()", &[], 1).values[0], std::f64::consts::PI));
        assert!(close(run("pi ( )", &[], 1).values[0], std::f64::consts::PI));
        let result = run("A*pi()", &[float_column(&[2.0])], 1);
        assert!(close(result.values[0], 2.0 * std::f64::consts::PI));
    }

    #[test]
    fn a_lone_pi_is_refused_unless_a_pi_column_exists() {
        let short = [col(0), col(1)];
        assert_eq!(
            ColumnFormula::from_letters("pi", &short, OWN),
            Err(FormulaError::PiNeedsParentheses)
        );
        assert_eq!(
            ColumnFormula::from_letters("2*PI", &short, OWN),
            Err(FormulaError::PiNeedsParentheses)
        );
        // With 425 columns, `pi` is the column PI (index 424) as before.
        let wide: Vec<NodeId> = (0..425).map(col).collect();
        let formula = ColumnFormula::from_letters("pi", &wide, OWN).unwrap();
        assert_eq!(formula.parts(), [FormulaPart::Column(col(424))]);
    }

    #[test]
    fn a_name_followed_by_a_parenthesis_is_a_function_otherwise_a_column() {
        // Not a function: a column letter before `(`.
        assert_eq!(
            ColumnFormula::from_letters("A(B/C)", &sheet(), OWN),
            Err(FormulaError::NotAFunction("A".into()))
        );
        // A function named like no column is accepted, case-insensitive.
        let sqrt = ColumnFormula::from_letters("sqrt(A)", &sheet(), OWN).unwrap();
        assert_eq!(sqrt.parts().len(), 3);
        assert_eq!(
            run("EXP(A)", &[float_column(&[1.0])], 1).values,
            run("exp(A)", &[float_column(&[1.0])], 1).values
        );
        // A lone lowercase name is a column, unchanged.
        let half = ColumnFormula::from_letters("i*0.5", &sheet(), OWN).unwrap();
        assert_eq!(half.parts()[0], FormulaPart::Column(col(8)));
        let b = ColumnFormula::from_letters("b/2", &sheet(), OWN).unwrap();
        assert_eq!(b.parts()[0], FormulaPart::Column(col(1)));
        // `A2` is still `A` then `2`: a syntax error, not an unknown column.
        let a2 = ColumnFormula::from_letters("A2", &sheet(), OWN).unwrap();
        assert!(matches!(a2.compile(), Err(FormulaError::Parse(_))));
        // The exponent of a number is not a call.
        assert!(parse("1e-3*A").is_ok());
    }

    #[test]
    fn names_with_digits_are_accepted_at_input() {
        for src in ["log10(A)", "log2(A)", "atan2(A, B)"] {
            assert!(
                ColumnFormula::from_letters(src, &sheet(), OWN).is_ok(),
                "{src}"
            );
        }
        let thousand = [float_column(&[1000.0])];
        assert!(close(run("log10(A)", &thousand, 1).values[0], 3.0));
        let eight = [float_column(&[8.0])];
        assert!(close(run("log2(A)", &eight, 1).values[0], 3.0));
    }

    #[test]
    fn an_unknown_function_or_a_wrong_count_is_refused_at_input() {
        assert_eq!(
            ColumnFormula::from_letters("foo(A)", &sheet(), OWN),
            Err(FormulaError::UnknownFunction("foo".into()))
        );
        assert_eq!(
            ColumnFormula::from_letters("sqrt(A, B)", &sheet(), OWN),
            Err(FormulaError::WrongArgumentCount {
                name: "sqrt".into(),
                expected: vec![1],
                found: 2
            })
        );
        assert_eq!(
            ColumnFormula::from_letters("if(A, B)", &sheet(), OWN),
            Err(FormulaError::WrongArgumentCount {
                name: "if".into(),
                expected: vec![3],
                found: 2
            })
        );
        assert_eq!(
            ColumnFormula::from_letters("pi(A)", &sheet(), OWN),
            Err(FormulaError::WrongArgumentCount {
                name: "pi".into(),
                expected: vec![0],
                found: 1
            })
        );
    }

    #[test]
    fn counts_are_put_into_words() {
        assert_eq!(counts_text(&[1]), "1");
        assert_eq!(counts_text(&[1, 2]), "1 or 2");
        assert_eq!(counts_text(&[1, 2, 3]), "1, 2 or 3");
    }

    #[test]
    fn a_saved_function_text_compiles_and_a_lone_name_stays_a_syntax_error() {
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Text("sqrt(".into()),
            FormulaPart::Column(col(0)),
            FormulaPart::Text(")".into()),
        ]);
        let compiled = formula.compile().unwrap();
        assert_eq!(compiled.referenced_columns(), BTreeSet::from([col(0)]));
        // A name that is not a function, saved in a text fragment: syntax
        // error at compute (invariant 10), never a column or a panic.
        for text in ["foo(", "sqrtx", "pi", "A"] {
            let crafted = ColumnFormula::from_parts(vec![
                FormulaPart::Text(text.into()),
                FormulaPart::Column(col(0)),
                FormulaPart::Text(")".into()),
            ]);
            assert!(
                matches!(crafted.compile(), Err(FormulaError::Parse(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn functions_keep_their_columns_as_dependencies() {
        // `referenced_columns` is what the orchestrator builds its edges from.
        let b = parse("sqrt(C)").unwrap().referenced_columns();
        let c = parse("abs(B)").unwrap().referenced_columns();
        assert_eq!(b, BTreeSet::from([col(2)]));
        assert_eq!(c, BTreeSet::from([col(1)]));
    }

    #[test]
    fn calls_and_argument_lists_count_towards_the_depth_cap() {
        let nested = |n: usize| format!("{}A{}", "sqrt(".repeat(n), ")".repeat(n));
        assert!(on_2_mib_stack(move || parse(&nested(100)).is_ok()));
        assert!(too_deep(nested(101)));
        assert!(too_deep(nested(100_000)));
        // One comma costs 1: 1 001 arguments are 1 000 commas plus the call.
        assert!(too_deep(format!("max({}A)", "A,".repeat(1000))));
        // `^` chain: 100 exponents pass, 101 do not.
        assert!(on_2_mib_stack(
            || parse(&format!("{}A", "A^".repeat(100))).is_ok()
        ));
        assert!(too_deep(format!("{}A", "A^".repeat(101))));
        assert!(too_deep(format!("{}A", "A^".repeat(100_000))));
        // A call at the limit evaluates without overflowing the stack.
        let value = on_2_mib_stack(move || {
            let formula = parse(&nested(100)).unwrap();
            let columns = [(col(0), float_column(&[16.0]))];
            let value = evaluate(&formula, &columns, 1).values[0];
            drop(formula);
            value
        });
        assert!(close(value, 1.0));
        // Comparison chains are bounded too.
        assert!(too_deep(format!("{}A", "A<".repeat(100_000))));
    }
}
