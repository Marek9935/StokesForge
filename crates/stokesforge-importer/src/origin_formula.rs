//! Origin column formula text -> **pieces** of the StokesForge formula language
//! (#556, lot C).
//!
//! Pure and data-only: the result is either a list of [`FormulaPiece`]s (text
//! fragments of the target language and column references that still carry the
//! Origin **short name**) or an [`UntranslatableReason`]. No sentence, no tree
//! identifier, no dependency on the model: a later lot maps each reference to the
//! imported column.
//!
//! Only forms whose meaning is **exactly** the same are translated; anything
//! else is refused rather than approximated. Notable rewrites: `col(B)` is the
//! reference `B`; `col(B)(645)` is `at(B, 645)`; `table(x, y, v)` is
//! `at(y, v, x)`; `max(c - col(B))` is `(c - min(B))`; a decimal comma between
//! digits at call depth 0 is a point. Short names are greedy identifiers
//! (`S1cR1c`, `B2`); a name followed by `(` is a function.
//!
//! The input is bounded ([`MAX_LEN`], [`MAX_DEPTH`]); the work is linear.

/// Longest accepted formula, in characters.
pub const MAX_LEN: usize = 4096;
/// Deepest accepted nesting of parentheses and calls.
pub const MAX_DEPTH: usize = 32;

/// A column named in an Origin formula, by its short name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaReference {
    /// Sheet prefix (`Raw!B`), as written.
    pub sheet: Option<String>,
    /// Workbook prefix (`[Book]Sheet!B`), as written.
    pub book: Option<String>,
    /// The Origin short name of the column (`B`, `S1cR1c`).
    pub short_name: String,
    /// Set on the X column of a translated `table(...)` when it is read in the
    /// same remote sheet as the first argument of the produced `at(...)`: the
    /// target language writes that argument unprefixed.
    pub same_sheet_as_first: bool,
}

/// One piece of a translated formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaPiece {
    /// Text of the StokesForge formula language (operators, numbers, calls).
    Text(String),
    /// A column reference.
    Reference(FormulaReference),
}

/// Why a formula cannot be translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntranslatableReason {
    /// Empty or blank text.
    Blank,
    /// Longer than [`MAX_LEN`].
    TooLong,
    /// Nested deeper than [`MAX_DEPTH`].
    TooDeep,
    /// An assignment (`=`).
    Assignment,
    /// Several statements (`;` or several lines).
    MultipleStatements,
    /// A string literal.
    StringLiteral,
    /// A character with no meaning in a formula (script syntax, operators
    /// outside the language).
    UnexpectedCharacter,
    /// Not a well-formed expression.
    Syntax,
    /// Access to a row (`col(Y)[477]`).
    RowAccess,
    /// `sum(B)`: cumulative in LabTalk, a total in StokesForge.
    CumulativeSum,
    /// A function outside the allowlist.
    UnknownFunction,
    /// The argument of a column function is not a column (nor `c - column`).
    ColumnFunctionArgument,
    /// The two columns of a `table(...)` are not read in the same sheet.
    TableSheetMismatch,
    /// A reserved name (`t0`).
    ReservedName,
}

/// The verdict on a formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginFormula {
    /// Translated: the pieces, in order.
    Pieces(Vec<FormulaPiece>),
    /// Not translated.
    Untranslatable(UntranslatableReason),
}

/// Translates the text of an Origin column formula.
pub fn translate_origin_formula(text: &str) -> OriginFormula {
    match translate(text) {
        Ok(p) => OriginFormula::Pieces(p),
        Err(r) => OriginFormula::Untranslatable(r),
    }
}

type R<T> = Result<T, UntranslatableReason>;
use UntranslatableReason as U;

/// Row-wise functions with the same meaning in both languages (Origin `log` is
/// base 10, like `stokesforge_analysis::functions`).
const ROW_FUNCTIONS: &[&str] = &[
    "sqrt", "exp", "ln", "log", "log10", "abs", "sin", "cos", "tan", "asin", "acos", "atan",
    "sinh", "cosh", "tanh", "floor", "ceil", "round",
];
/// Column functions (one value for a whole column).
const COLUMN_FUNCTIONS: &[&str] = &["max", "min", "mean"];

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Num(String),
    Ident(String),
    Quoted(String),
    Bracket(String),
    LParen,
    RParen,
    Comma,
    Bang,
    Op(char),
}

#[derive(Debug, Clone)]
struct Tok {
    lead: String,
    kind: Kind,
}

fn lex(s: &str) -> R<Vec<Tok>> {
    let b: Vec<char> = s.chars().collect();
    let n = b.len();
    let mut i = 0;
    let mut toks = Vec::new();
    loop {
        let start = i;
        while i < n && b[i].is_whitespace() {
            i += 1;
        }
        let lead: String = b[start..i].iter().collect();
        if i >= n {
            break;
        }
        if lead.contains(['\n', '\r']) {
            return Err(U::MultipleStatements);
        }
        let c = b[i];
        let kind = if c.is_ascii_digit() || (c == '.' && i + 1 < n && b[i + 1].is_ascii_digit()) {
            let s0 = i;
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < n && b[i] == '.' {
                i += 1;
                while i < n && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < n && (b[i] == 'e' || b[i] == 'E') {
                let mut j = i + 1;
                if j < n && (b[j] == '+' || b[j] == '-') {
                    j += 1;
                }
                if j < n && b[j].is_ascii_digit() {
                    while j < n && b[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            Kind::Num(b[s0..i].iter().collect())
        } else if c.is_ascii_alphabetic() || c == '_' {
            let s0 = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            Kind::Ident(b[s0..i].iter().collect())
        } else if c == '\'' {
            i += 1;
            let mut name = String::new();
            loop {
                if i >= n {
                    return Err(U::Syntax);
                }
                if b[i] == '\'' {
                    if i + 1 < n && b[i + 1] == '\'' {
                        name.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                name.push(b[i]);
                i += 1;
            }
            Kind::Quoted(name)
        } else if c == '[' {
            let s0 = i + 1;
            i = s0;
            while i < n && b[i] != ']' {
                i += 1;
            }
            if i >= n {
                return Err(U::Syntax);
            }
            let name: String = b[s0..i].iter().collect();
            i += 1;
            Kind::Bracket(name)
        } else {
            i += 1;
            match c {
                '(' => Kind::LParen,
                ')' => Kind::RParen,
                ',' => Kind::Comma,
                '!' => Kind::Bang,
                '+' | '-' | '*' | '/' | '^' => Kind::Op(c),
                '=' => return Err(U::Assignment),
                ';' => return Err(U::MultipleStatements),
                '"' => return Err(U::StringLiteral),
                _ => return Err(U::UnexpectedCharacter),
            }
        };
        toks.push(Tok { lead, kind });
    }
    Ok(toks)
}

fn translate(text: &str) -> R<Vec<FormulaPiece>> {
    if text.chars().count() > MAX_LEN {
        return Err(U::TooLong);
    }
    let t = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if t.is_empty() {
        return Err(U::Blank);
    }
    let mut p = Parser {
        toks: lex(t)?,
        pos: 0,
        out: Vec::new(),
    };
    p.expr(0, 0)?;
    if p.pos < p.toks.len() {
        return Err(U::Syntax);
    }
    Ok(p.out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    out: Vec<FormulaPiece>,
}

type Ref3 = (FormulaReference, usize, bool);

impl Parser {
    fn kind(&self, p: usize) -> Option<&Kind> {
        self.toks.get(p).map(|t| &t.kind)
    }

    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if let Some(FormulaPiece::Text(last)) = self.out.last_mut() {
            last.push_str(s);
        } else {
            self.out.push(FormulaPiece::Text(s.to_string()));
        }
    }

    fn lead(&self, p: usize) -> String {
        self.toks[p].lead.clone()
    }

    fn capture(&mut self, f: impl FnOnce(&mut Self) -> R<()>) -> R<Vec<FormulaPiece>> {
        let saved = std::mem::take(&mut self.out);
        let r = f(self);
        let got = std::mem::replace(&mut self.out, saved);
        r.map(|()| got)
    }

    fn expr(&mut self, depth: usize, call: usize) -> R<()> {
        if depth > MAX_DEPTH {
            return Err(U::TooDeep);
        }
        loop {
            while let Some(Kind::Op(c @ ('+' | '-'))) = self.kind(self.pos).cloned() {
                let l = self.lead(self.pos);
                self.text(&format!("{l}{c}"));
                self.pos += 1;
            }
            self.primary(depth, call)?;
            if let Some(Kind::Bracket(_)) = self.kind(self.pos) {
                return Err(U::RowAccess);
            }
            match self.kind(self.pos).cloned() {
                Some(Kind::Op(c)) => {
                    let l = self.lead(self.pos);
                    self.text(&format!("{l}{c}"));
                    self.pos += 1;
                }
                _ => return Ok(()),
            }
        }
    }

    fn primary(&mut self, depth: usize, call: usize) -> R<()> {
        let Some(kind) = self.kind(self.pos).cloned() else {
            return Err(U::Syntax);
        };
        let l = self.lead(self.pos);
        match kind {
            Kind::Num(n1) => {
                let comma = call == 0
                    && n1.bytes().all(|b| b.is_ascii_digit())
                    && matches!(self.kind(self.pos + 1), Some(Kind::Comma))
                    && self.toks[self.pos + 1].lead.is_empty()
                    && matches!(self.kind(self.pos + 2), Some(Kind::Num(n2))
                        if n2.bytes().all(|b| b.is_ascii_digit()) && self.toks[self.pos + 2].lead.is_empty());
                if comma {
                    let Some(Kind::Num(n2)) = self.kind(self.pos + 2).cloned() else {
                        return Err(U::Syntax);
                    };
                    self.text(&format!("{l}{n1}.{n2}"));
                    self.pos += 3;
                } else {
                    self.text(&format!("{l}{n1}"));
                    self.pos += 1;
                }
                Ok(())
            }
            Kind::LParen => {
                self.text(&format!("{l}("));
                self.pos += 1;
                self.expr(depth + 1, call)?;
                self.close()
            }
            Kind::Ident(_) | Kind::Quoted(_) | Kind::Bracket(_) => {
                if let Some((r, np, via_col)) = self.try_reference(self.pos)? {
                    if via_col && matches!(self.kind(np), Some(Kind::LParen)) {
                        self.text(&format!("{l}at("));
                        self.out.push(FormulaPiece::Reference(r));
                        self.text(", ");
                        self.pos = np + 1;
                        if let Some(t) = self.toks.get_mut(self.pos) {
                            t.lead.clear();
                        }
                        if depth + 1 > MAX_DEPTH {
                            return Err(U::TooDeep);
                        }
                        self.expr(depth + 1, call + 1)?;
                        if !matches!(self.kind(self.pos), Some(Kind::RParen)) {
                            return Err(U::Syntax);
                        }
                        self.pos += 1;
                        self.text(")");
                    } else {
                        self.text(&l);
                        self.out.push(FormulaPiece::Reference(r));
                        self.pos = np;
                    }
                    return Ok(());
                }
                self.function(depth, call, l)
            }
            _ => Err(U::Syntax),
        }
    }

    /// Consumes a `)` (with its leading space).
    fn close(&mut self) -> R<()> {
        if !matches!(self.kind(self.pos), Some(Kind::RParen)) {
            return Err(U::Syntax);
        }
        let l = self.lead(self.pos);
        self.text(&format!("{l})"));
        self.pos += 1;
        Ok(())
    }

    fn reference(sheet: Option<String>, book: Option<String>, name: String) -> R<FormulaReference> {
        if name.eq_ignore_ascii_case("t0") {
            return Err(U::ReservedName);
        }
        Ok(FormulaReference {
            sheet,
            book,
            short_name: name,
            same_sheet_as_first: false,
        })
    }

    /// A column reference starting at token `p`, if there is one: the
    /// reference, the next position, and whether it was written `col(..)`.
    fn try_reference(&self, p: usize) -> R<Option<Ref3>> {
        let k = |q: usize| self.kind(q);
        match k(p) {
            Some(Kind::Ident(n))
                if n.eq_ignore_ascii_case("col") && k(p + 1) == Some(&Kind::LParen) =>
            {
                match (k(p + 2), k(p + 3)) {
                    (Some(Kind::Ident(name)), Some(Kind::RParen)) => Ok(Some((
                        Self::reference(None, None, name.clone())?,
                        p + 4,
                        true,
                    ))),
                    _ => Err(U::Syntax),
                }
            }
            Some(Kind::Bracket(book)) => match (k(p + 1), k(p + 2), k(p + 3)) {
                (
                    Some(Kind::Ident(s) | Kind::Quoted(s)),
                    Some(Kind::Bang),
                    Some(Kind::Ident(name)),
                ) => Ok(Some((
                    Self::reference(Some(s.clone()), Some(book.clone()), name.clone())?,
                    p + 4,
                    false,
                ))),
                _ => Err(U::Syntax),
            },
            Some(Kind::Ident(s) | Kind::Quoted(s)) if k(p + 1) == Some(&Kind::Bang) => {
                match k(p + 2) {
                    Some(Kind::Ident(name)) => Ok(Some((
                        Self::reference(Some(s.clone()), None, name.clone())?,
                        p + 3,
                        false,
                    ))),
                    _ => Err(U::Syntax),
                }
            }
            Some(Kind::Ident(n)) if k(p + 1) != Some(&Kind::LParen) => Ok(Some((
                Self::reference(None, None, n.clone())?,
                p + 1,
                false,
            ))),
            Some(Kind::Quoted(_)) => Err(U::Syntax),
            _ => Ok(None),
        }
    }

    /// Arguments after the first: `, expr` repeated, up to and including `)`.
    fn rest_args(&mut self, depth: usize, call: usize) -> R<()> {
        loop {
            match self.kind(self.pos) {
                Some(Kind::Comma) => {
                    let l = self.lead(self.pos);
                    self.text(&format!("{l},"));
                    self.pos += 1;
                    self.expr(depth + 1, call + 1)?;
                }
                Some(Kind::RParen) => return self.close(),
                _ => return Err(U::Syntax),
            }
        }
    }

    fn function(&mut self, depth: usize, call: usize, l: String) -> R<()> {
        let Some(Kind::Ident(name)) = self.kind(self.pos).cloned() else {
            return Err(U::Syntax);
        };
        if !matches!(self.kind(self.pos + 1), Some(Kind::LParen)) {
            return Err(U::Syntax);
        }
        if depth + 1 > MAX_DEPTH {
            return Err(U::TooDeep);
        }
        let lname = name.to_ascii_lowercase();
        self.pos += 2;
        if lname == "sum" {
            return Err(U::CumulativeSum);
        }
        if lname == "table" {
            return self.table(depth, call, &l);
        }
        if COLUMN_FUNCTIONS.contains(&lname.as_str()) {
            return self.column_function(depth, call, &l, &lname);
        }
        if !ROW_FUNCTIONS.contains(&lname.as_str()) {
            return Err(U::UnknownFunction);
        }
        self.text(&format!("{l}{lname}("));
        self.expr(depth + 1, call + 1)?;
        self.rest_args(depth, call)
    }

    fn column_function(&mut self, depth: usize, call: usize, l: &str, name: &str) -> R<()> {
        // `max(c - col(B))` = `c - min(B)`, exact for this affine form only.
        if matches!(name, "max" | "min") {
            if let (Some(Kind::Num(c)), Some(Kind::Op('-'))) =
                (self.kind(self.pos), self.kind(self.pos + 1))
            {
                if let Some((r, np, _)) = self.try_reference(self.pos + 2)? {
                    if matches!(self.kind(np), Some(Kind::RParen)) {
                        let inner = if name == "max" { "min" } else { "max" };
                        let c = c.clone();
                        self.text(&format!("{l}({c} - {inner}("));
                        self.out.push(FormulaPiece::Reference(r));
                        self.text("))");
                        self.pos = np + 1;
                        return Ok(());
                    }
                }
            }
        }
        match self.try_reference(self.pos)? {
            Some((r, np, _)) if matches!(self.kind(np), Some(Kind::Comma | Kind::RParen)) => {
                self.text(&format!("{l}{name}("));
                self.out.push(FormulaPiece::Reference(r));
                self.pos = np;
                self.rest_args(depth, call)
            }
            _ => Err(U::ColumnFunctionArgument),
        }
    }

    /// `table(x, y, v)` -> `at(y, v, x)`.
    fn table(&mut self, depth: usize, call: usize, l: &str) -> R<()> {
        let Some((mut x, np, _)) = self.try_reference(self.pos)? else {
            return Err(U::ColumnFunctionArgument);
        };
        if !matches!(self.kind(np), Some(Kind::Comma)) {
            return Err(U::Syntax);
        }
        let Some((y, np, _)) = self.try_reference(np + 1)? else {
            return Err(U::ColumnFunctionArgument);
        };
        if !matches!(self.kind(np), Some(Kind::Comma)) {
            return Err(U::Syntax);
        }
        let same = |a: &Option<String>, b: &Option<String>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        };
        if !same(&x.sheet, &y.sheet) || !same(&x.book, &y.book) {
            return Err(U::TableSheetMismatch);
        }
        x.same_sheet_as_first = x.sheet.is_some() || x.book.is_some();
        self.pos = np + 1;
        if self.pos >= self.toks.len() {
            return Err(U::Syntax);
        }
        self.toks[self.pos].lead.clear();
        let v = self.capture(|s| s.expr(depth + 1, call + 1))?;
        if !matches!(self.kind(self.pos), Some(Kind::RParen)) {
            return Err(U::Syntax);
        }
        self.pos += 1;
        self.text(&format!("{l}at("));
        self.out.push(FormulaPiece::Reference(y));
        self.text(", ");
        for piece in v {
            match piece {
                FormulaPiece::Text(t) => self.text(&t),
                r => self.out.push(r),
            }
        }
        self.text(", ");
        self.out.push(FormulaPiece::Reference(x));
        self.text(")");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pieces as a string: references as `{name}` (prefixes shown, `*` when
    /// read in the sheet of the first argument).
    fn ok(input: &str) -> String {
        match translate_origin_formula(input) {
            OriginFormula::Pieces(ps) => ps
                .iter()
                .map(|p| match p {
                    FormulaPiece::Text(t) => t.clone(),
                    FormulaPiece::Reference(r) => format!(
                        "{{{}{}{}{}}}",
                        r.book
                            .as_ref()
                            .map(|b| format!("[{b}]"))
                            .unwrap_or_default(),
                        r.sheet
                            .as_ref()
                            .map(|s| format!("{s}!"))
                            .unwrap_or_default(),
                        r.short_name,
                        if r.same_sheet_as_first { "*" } else { "" }
                    ),
                })
                .collect(),
            OriginFormula::Untranslatable(r) => panic!("{input:?} refused: {r:?}"),
        }
    }

    fn no(input: &str) -> UntranslatableReason {
        match translate_origin_formula(input) {
            OriginFormula::Untranslatable(r) => r,
            OriginFormula::Pieces(p) => panic!("{input:?} accepted: {p:?}"),
        }
    }

    #[test]
    fn simple_arithmetic() {
        assert_eq!(ok("B-C"), "{B}-{C}");
        assert_eq!(ok("B*0.5+60"), "{B}*0.5+60");
        assert_eq!(ok("(D+E)/2"), "({D}+{E})/2");
        assert_eq!(ok("(B+C+D)/9*1.009"), "({B}+{C}+{D})/9*1.009");
        assert_eq!(ok("B"), "{B}");
        assert_eq!(ok("  B - C \0"), "{B} - {C}");
    }

    #[test]
    fn col_forms_are_references() {
        assert_eq!(ok("col(B)"), "{B}");
        assert_eq!(ok("Col(Y)"), "{Y}");
        assert_eq!(ok("COL(B)"), "{B}");
    }

    #[test]
    fn callee_is_lowercased() {
        assert_eq!(ok("B/Max(B)"), "{B}/max({B})");
    }

    #[test]
    fn short_names_are_greedy() {
        assert_eq!(ok("S1cR1c*2"), "{S1cR1c}*2");
        assert_eq!(ok("B2+B"), "{B2}+{B}");
    }

    #[test]
    fn call_on_col_is_at() {
        assert_eq!(ok("B/col(B)(645)"), "{B}/at({B}, 645)");
    }

    #[test]
    fn table_swaps_arguments() {
        assert_eq!(ok("table(A, B, 5)"), "at({B}, 5, {A})");
        assert_eq!(ok("Table(A, B, C*2)"), "at({B}, {C}*2, {A})");
    }

    #[test]
    fn prefixed_references() {
        assert_eq!(ok("Raw!B"), "{Raw!B}");
        assert_eq!(ok("'My sheet'!B"), "{My sheet!B}");
        assert_eq!(ok("[Book]Sheet!B"), "{[Book]Sheet!B}");
        assert_eq!(ok("'it''s'!B"), "{it's!B}");
    }

    #[test]
    fn table_remote_marks_the_third_argument() {
        assert_eq!(
            ok("table(Raw!A, Raw!CM, 800)"),
            "at({Raw!CM}, 800, {Raw!A*})"
        );
        assert_eq!(
            ok("(Raw!X - table(Raw!A, Raw!X, 800))/table(Raw!A, Raw!X, 800)"),
            "({Raw!X} - at({Raw!X}, 800, {Raw!A*}))/at({Raw!X}, 800, {Raw!A*})"
        );
        assert_eq!(no("table(Raw!A, Other!CM, 800)"), U::TableSheetMismatch);
        assert_eq!(no("table(A, Raw!CM, 800)"), U::TableSheetMismatch);
    }

    #[test]
    fn decimal_comma_at_depth_zero_only() {
        assert_eq!(ok("B/0,37783*23000"), "{B}/0.37783*23000");
        assert_eq!(ok("(B)*(0,5)"), "({B})*(0.5)");
        assert_eq!(ok("max(B, 1,5)"), "max({B}, 1,5)");
        assert_eq!(ok("round(B, 2)"), "round({B}, 2)");
        // not between digits / with a space: a stray comma
        assert_eq!(no("B*0, 5"), U::Syntax);
        assert_eq!(no("B*0 ,5"), U::Syntax);
        assert_eq!(no("B,C"), U::Syntax);
    }

    #[test]
    fn affine_max_is_rewritten() {
        assert_eq!(ok("max(100 - Col(B))"), "(100 - min({B}))");
        assert_eq!(ok("max(5-B)"), "(5 - min({B}))");
        assert_eq!(ok("min(5 - B)"), "(5 - max({B}))");
        assert_eq!(
            ok("100-((100-Col(B))/max(100-Col(B))*100)"),
            "100-((100-{B})/(100 - min({B}))*100)"
        );
    }

    #[test]
    fn other_column_function_arguments_are_refused() {
        assert_eq!(no("max(B-C)"), U::ColumnFunctionArgument);
        assert_eq!(no("max(B*2)"), U::ColumnFunctionArgument);
        assert_eq!(no("max(sqrt(B))"), U::ColumnFunctionArgument);
        assert_eq!(no("min(B+1)"), U::ColumnFunctionArgument);
        assert_eq!(no("mean(2*B)"), U::ColumnFunctionArgument);
        assert_eq!(no("max(B-C)/2"), U::ColumnFunctionArgument);
    }

    #[test]
    fn row_access_is_refused() {
        assert_eq!(no("col(Y)[477]"), U::RowAccess);
        assert_eq!(no("col(B)[i-1]"), U::RowAccess);
        assert_eq!(no("col(Y)/col(Y)[477]"), U::RowAccess);
    }

    #[test]
    fn i_e_pi_are_plain_names() {
        assert_eq!(ok("i+e*pi"), "{i}+{e}*{pi}");
    }

    #[test]
    fn row_wise_functions_and_unknown_ones() {
        assert_eq!(ok("SQRT(B)+Log(C)"), "sqrt({B})+log({C})");
        assert_eq!(ok("ln(abs(B))"), "ln(abs({B}))");
        assert_eq!(no("foo(B)"), U::UnknownFunction);
        assert_eq!(no("erf(B)"), U::UnknownFunction);
        assert_eq!(no("B(3)"), U::UnknownFunction);
    }

    #[test]
    fn sum_is_cumulative() {
        assert_eq!(no("sum(B)"), U::CumulativeSum);
        assert_eq!(no("B/Sum(B)"), U::CumulativeSum);
    }

    #[test]
    fn not_formulas() {
        assert_eq!(no("t0"), U::ReservedName);
        assert_eq!(no(""), U::Blank);
        assert_eq!(no("  \0\0 "), U::Blank);
        assert_eq!(no("B=C"), U::Assignment);
        assert_eq!(no("col(B)=1"), U::Assignment);
        assert_eq!(no("B+1;C+2"), U::MultipleStatements);
        assert_eq!(no("B+1\nC+2"), U::MultipleStatements);
        assert_eq!(no("if (B>1) {C}"), U::UnexpectedCharacter);
        assert_eq!(no("\"text\""), U::StringLiteral);
        assert_eq!(no("B C"), U::Syntax);
        assert_eq!(no("2B"), U::Syntax);
        assert_eq!(no("(B"), U::Syntax);
        assert_eq!(no("B)"), U::Syntax);
        assert_eq!(no("B+"), U::Syntax);
        assert_eq!(no("max("), U::ColumnFunctionArgument);
    }

    #[test]
    fn bounded_and_total() {
        assert_eq!(no(&"(".repeat(10_000)), U::TooLong);
        assert_eq!(
            no(&format!("{}B{}", "(".repeat(100), ")".repeat(100))),
            U::TooDeep
        );
        assert_eq!(
            no(&format!("{}B{}", "sqrt(".repeat(100), ")".repeat(100))),
            U::TooDeep
        );
        assert_eq!(no(&"B+".repeat(MAX_LEN)), U::TooLong);
        let long = "B+".repeat(1500) + "B";
        assert!(matches!(
            translate_origin_formula(&long),
            OriginFormula::Pieces(_)
        ));
        // deterministic noise: never panics
        let alphabet: Vec<char> = "B C1.,()[]'!;=\"+-*/^colmax table\n\0é{".chars().collect();
        let mut s = 12345u64;
        for _ in 0..3000 {
            let mut t = String::new();
            for _ in 0..(s % 40) {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                t.push(alphabet[(s >> 33) as usize % alphabet.len()]);
            }
            let _ = translate_origin_formula(&t);
        }
    }
}
