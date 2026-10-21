//! Colonnes calculées par formule (v1, arithmétique simple intra-feuille).
//!
//! Portée : `+ - * /`, parenthèses, littéraux numériques, références de
//! colonnes par leur **lettre de position** dans la feuille (`A` = première
//! colonne, `B` = deuxième, ... `Z`, `AA`, `AB`, ... façon Origin/Excel).
//! Pas de fonctions (`sin`, `sum`, ...), pas de référence inter-feuilles —
//! hors de portée de cette version, voir la tête de tâche qui a introduit ce
//! module.
//!
//! Module pur : ni accès au `Project`, ni à l'arbre — [`evaluate`] ne prend
//! qu'un slice de [`ColumnValues`] déjà extrait par l'appelant (typiquement
//! `stokesforge-session::recalc`, qui orchestre le recalcul — voir sa doc de tête
//! pour l'ordonnancement par dépendances et la détection de cycle, dont
//! [`topological_order`] ci-dessous est l'outil générique).
//!
//! # Dégradation gracieuse (invariant 10) — jamais de panic
//!
//! Une formule qui ne parse pas, référence une colonne hors bornes, non
//! numérique, ou se référence elle-même, rend un bloc **entièrement `NaN`**
//! de la longueur demandée, accompagné d'un [`FormulaError`] que l'appelant
//! peut afficher — jamais une `Result` avalée en silence, jamais un panic.
//! Une colonne référencée plus courte que la feuille n'est, elle, pas une
//! erreur : [`ColumnValues::number_at`] rend déjà `NaN` au-delà de sa
//! longueur, exactement le comportement voulu ici.
//!
//! Une colonne à types **mixtes** n'est pas « non numérique » : elle est
//! acceptée, et ses cellules texte ou vides valent `NaN` ligne à ligne, comme
//! une valeur manquante ordinaire. Seule une colonne entièrement texte est
//! refusée en bloc.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::column::{ColumnValues, NumericStatus};

/// Pourquoi une formule ne produit pas les valeurs qu'elle décrit. Jamais un
/// panic (invariant 10) : chaque variante est un état de première classe,
/// affichable tel quel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaError {
    /// La chaîne ne s'analyse pas comme une formule valide de la grammaire
    /// v1 (jeton inattendu, parenthèse non refermée, formule vide, ...).
    Parse(String),
    /// La lettre de colonne référencée dépasse le nombre de colonnes de la
    /// feuille.
    UnknownColumn(String),
    /// La formule référence la colonne qui la porte.
    SelfReference(String),
    /// Cycle de dépendance entre colonnes-formule (directement ou par une
    /// colonne dont le calcul dépend lui-même d'un cycle) — détecté par
    /// l'orchestrateur ([`topological_order`]), jamais par une boucle qui
    /// tournerait indéfiniment.
    Cycle,
    /// L'arbre de la formule dépasse [`MAX_DEPTH`] (parenthèses, signes ou
    /// chaîne d'opérateurs trop longs) : refusée au parse plutôt que de
    /// laisser parseur, évaluation et `Drop` récurser jusqu'à l'abandon du
    /// processus (#289).
    TooDeep,
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
            FormulaError::Cycle => write!(f, "dependency cycle between formula columns"),
            FormulaError::TooDeep => write!(f, "formula nested too deeply"),
        }
    }
}

impl std::error::Error for FormulaError {}

// -- lettres de colonne : même convention bijective en base 26 que
// `Project::spreadsheet_name` (A, B, ..., Z, AA, AB, ...), dupliquée ici
// plutôt que partagée : c'est une pure notation, sans rien à synchroniser
// avec la génération de noms de colonnes façon tableur.

/// Position 0-based → lettre(s) façon tableur.
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

/// Lettre(s) façon tableur → position 0-based. `None` si `letters` contient
/// autre chose que des lettres ASCII (ne devrait jamais arriver : le
/// tokenizer ne construit ce jeton qu'à partir d'un run de lettres ASCII).
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
    Column(usize),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(source: &str) -> Result<Vec<Token>, FormulaError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
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
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // Exposant optionnel (`1e-3`, `2.5E6`) : accepté seulement
                // s'il est syntaxiquement complet (au moins un chiffre après
                // le signe optionnel), sinon on laisse le `e` intact — il
                // n'a de toute façon aucun sens comme lettre de colonne
                // isolée juste après un nombre, ce cas restera une erreur de
                // parse plus loin, ce qui est le comportement voulu.
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let mark = i;
                    let mut j = i + 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        while j < chars.len() && chars[j].is_ascii_digit() {
                            j += 1;
                        }
                        i = j;
                    } else {
                        i = mark;
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let value: f64 = text
                    .parse()
                    .map_err(|_| FormulaError::Parse(format!("invalid number: {text}")))?;
                tokens.push(Token::Number(value));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let letters: String = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .to_ascii_uppercase();
                let index = letters_to_index(&letters).ok_or_else(|| {
                    FormulaError::Parse(format!("invalid column reference: {letters}"))
                })?;
                tokens.push(Token::Column(index));
            }
            other => {
                return Err(FormulaError::Parse(format!(
                    "unexpected character: {other}"
                )))
            }
        }
    }
    Ok(tokens)
}

// -- AST --

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Column(usize),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

/// Plafond de profondeur de l'arbre, en « coût » : un niveau d'imbrication
/// (parenthèse, signe) coûte [`NEST_COST`], un terme d'une chaîne `a+b+c…` ou
/// `a*b*c…` coûte 1. Le parseur, `eval_expr`/`combine`, `referenced_columns` et
/// le `Drop` de l'arbre récursent d'autant : borné ici, aucun ne peut dépasser
/// la pile d'un thread de 2 MiB, même en debug (`docs/archive/rustrewrite/
/// questions-ouvertes.md` §11, #289). Admet 100 imbrications ou environ 1 000 termes (1 001 passent).
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

    /// Ajoute `cost` à la profondeur courante ; au-delà du plafond, `TooDeep`.
    fn charge(&mut self, cost: usize) -> Result<(), FormulaError> {
        self.depth += cost;
        if self.depth > MAX_DEPTH {
            return Err(FormulaError::TooDeep);
        }
        Ok(())
    }

    /// Exécute `f` un niveau d'imbrication plus bas, puis rend la profondeur.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, FormulaError>,
    ) -> Result<T, FormulaError> {
        self.charge(NEST_COST)?;
        let out = f(self)?;
        self.depth -= NEST_COST;
        Ok(out)
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

    /// `factor := ('+' | '-') factor | primary`
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
            _ => self.parse_primary(),
        }
    }

    /// `primary := number | column | '(' expr ')'`
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
                let inner = self.nested(Self::parse_expr)?;
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
            Some(other) => Err(FormulaError::Parse(format!("unexpected token: {other:?}"))),
            None => Err(FormulaError::Parse("incomplete expression".to_string())),
        }
    }
}

/// Une formule analysée : arithmétique simple sur des colonnes désignées par
/// leur lettre de position dans la feuille. Ne porte aucune référence à une
/// feuille ou un projet précis — c'est [`evaluate`] qui la met en présence de
/// colonnes concrètes.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    expr: Expr,
}

impl Formula {
    /// Analyse `source`. `Err(FormulaError::Parse)` si la chaîne ne
    /// correspond pas à la grammaire v1 — jamais un panic, quelle que soit
    /// l'entrée (c'est un texte libre tapé par l'utilisateur).
    pub fn parse(source: &str) -> Result<Formula, FormulaError> {
        let tokens = tokenize(source)?;
        if tokens.is_empty() {
            return Err(FormulaError::Parse("empty formula".to_string()));
        }
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr()?;
        if parser.pos != tokens.len() {
            return Err(FormulaError::Parse(
                "unexpected tokens after the formula".to_string(),
            ));
        }
        Ok(Formula { expr })
    }

    /// Positions (0-based, lettre de la feuille) de toutes les colonnes que
    /// cette formule référence, sans jugement sur leur validité — c'est
    /// [`evaluate`] qui vérifie qu'elles existent, sont numériques, et ne
    /// désignent pas la colonne-formule elle-même. Sert aussi à
    /// l'orchestrateur pour construire le graphe de dépendance entre
    /// colonnes-formule (voir [`topological_order`]).
    pub fn referenced_columns(&self) -> BTreeSet<usize> {
        fn walk(expr: &Expr, out: &mut BTreeSet<usize>) {
            match expr {
                Expr::Number(_) => {}
                Expr::Column(index) => {
                    out.insert(*index);
                }
                Expr::Neg(inner) => walk(inner, out),
                Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) => {
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

/// Résultat **toujours complet** d'une évaluation : `values` a la longueur
/// `target_len` demandée, que l'évaluation ait réussi ou non — dégradée à
/// `NaN` partout en cas d'erreur (invariant 10). `error` porte la raison,
/// pour que l'appelant (l'orchestrateur de `stokesforge-session`, demain l'UI)
/// sache qu'il y a un problème et lequel, plutôt que de découvrir une
/// colonne silencieusement vide.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaEvaluation {
    /// Valeurs calculées, ou `NaN` partout si `error.is_some()`.
    pub values: Vec<f64>,
    /// Statut ligne à ligne, parallèle à `values` (même longueur), qui
    /// distingue une case sans rien à calculer
    /// ([`NumericStatus::Missing`]) d'un vrai échec de calcul dû à un texte
    /// non numérique dans une colonne référencée ([`NumericStatus::Failed`])
    /// — voir [`ColumnValues::status_at`]. Toujours `Missing` partout si
    /// `error.is_some()` : une erreur de formule (colonne inconnue,
    /// auto-référence, cycle, ...) dégrade tout en bloc, sans qu'il y ait de
    /// distinction ligne à ligne à faire.
    pub statuses: Vec<NumericStatus>,
    /// `None` si l'évaluation a réussi.
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

/// Évalue `formula` ligne par ligne contre les colonnes d'une feuille.
///
/// `columns[i]` doit être la colonne de lettre `index_to_letters(i)` de la
/// feuille (l'ordre des enfants du nœud `Worksheet`) ; `self_index` est la
/// position de la colonne-formule elle-même dans cette même feuille, pour
/// détecter l'auto-référence. `target_len` est la longueur voulue du
/// résultat, choisie par l'appelant (l'acteur prend la plus longue des
/// colonnes lues, #194) — elle peut dépasser la longueur de certaines
/// colonnes référencées : [`ColumnValues::get`] rend déjà `NaN`/`None`
/// au-delà de leur longueur propre, donc une colonne plus courte complète
/// naturellement à `NaN`, sans que ce soit une erreur.
///
/// Toute référence invalide (colonne hors bornes, non numérique,
/// auto-référence) dégrade **tout** le résultat à `NaN` plutôt que de
/// calculer partiellement : une formule dont une seule référence est
/// mauvaise n'a pas de valeur partiellement fiable à rendre.
pub fn evaluate(
    formula: &Formula,
    self_index: usize,
    columns: &[ColumnValues],
    target_len: usize,
) -> FormulaEvaluation {
    for index in formula.referenced_columns() {
        if index == self_index {
            return FormulaEvaluation::degraded(
                target_len,
                FormulaError::SelfReference(index_to_letters(index)),
            );
        }
        match columns.get(index) {
            None => {
                return FormulaEvaluation::degraded(
                    target_len,
                    FormulaError::UnknownColumn(index_to_letters(index)),
                )
            }
            // Une colonne **mixte** — y compris une colonne entièrement
            // textuelle, il n'y a plus de type `Text` homogène à part — est
            // acceptée : elle porte des nombres, simplement pas
            // nécessairement à toutes les lignes. Ses cellules texte et
            // vides valent `NaN` (`ColumnValues::number_at`), exactement
            // comme une valeur manquante d'une colonne numérique — dégrader
            // la ligne, pas toute la formule (invariant 10).
            Some(ColumnValues::Float(_)) | Some(ColumnValues::Mixed(_)) => {}
        }
    }
    let (values, statuses) = (0..target_len)
        .map(|row| eval_expr(&formula.expr, row, columns))
        .unzip();
    FormulaEvaluation {
        values,
        statuses,
        error: None,
    }
}

/// Évalue `expr` à la ligne `row`, en rendant à la fois la valeur flottante
/// (arithmétique IEEE ordinaire, inchangée) et le [`NumericStatus`] combiné
/// de ses opérandes — **`Failed` l'emporte toujours sur `Missing`**
/// ([`NumericStatus::combine`]) : un texte non numérique quelque part dans
/// l'expression doit se voir, même noyé dans une addition avec une case
/// vide.
fn eval_expr(expr: &Expr, row: usize, columns: &[ColumnValues]) -> (f64, NumericStatus) {
    match expr {
        Expr::Number(value) => (*value, NumericStatus::Ok),
        // `number_at`/`status_at` plutôt que `get` : la lecture sans
        // allocation, qui rend déjà `NaN` pour tout ce qui n'est pas un
        // nombre (colonne texte référencée — `evaluate` l'a de toute façon
        // déjà refusée —, cellule texte ou vide d'une colonne mixte, ligne
        // hors bornes). `get` clonerait une `String` par ligne pour la jeter
        // aussitôt.
        Expr::Column(index) => (
            columns[*index].number_at(row),
            columns[*index].status_at(row),
        ),
        Expr::Neg(inner) => {
            let (value, status) = eval_expr(inner, row, columns);
            (-value, status)
        }
        Expr::Add(a, b) => combine(a, b, row, columns, |x, y| x + y),
        Expr::Sub(a, b) => combine(a, b, row, columns, |x, y| x - y),
        Expr::Mul(a, b) => combine(a, b, row, columns, |x, y| x * y),
        Expr::Div(a, b) => combine(a, b, row, columns, |x, y| x / y),
    }
}

/// Facteur commun aux quatre opérateurs binaires : calcule chaque opérande
/// (valeur et statut), combine les valeurs par `op` (arithmétique IEEE
/// inchangée) et les statuts par [`NumericStatus::combine`].
fn combine(
    a: &Expr,
    b: &Expr,
    row: usize,
    columns: &[ColumnValues],
    op: impl Fn(f64, f64) -> f64,
) -> (f64, NumericStatus) {
    let (av, astatus) = eval_expr(a, row, columns);
    let (bv, bstatus) = eval_expr(b, row, columns);
    (op(av, bv), astatus.combine(bstatus))
}

/// Ordonne un ensemble de nœuds par dépendance (tri topologique de Kahn),
/// **sans jamais boucler indéfiniment** — c'est l'outil générique dont
/// l'orchestrateur de recalcul (`stokesforge-session::recalc`) se sert pour ne
/// recalculer une colonne-formule qu'une fois toutes celles dont elle dépend
/// déjà à jour, et pour détecter un cycle entre colonnes-formule sans risque
/// de boucle infinie.
///
/// `edges[k]` = l'ensemble des nœuds que `k` doit voir déjà résolus avant
/// lui. Une valeur de `edges[k]` qui n'est **pas** elle-même une clé de
/// `edges` n'est pas comptée comme une dépendance à ordonner : c'est le cas
/// d'une référence vers une colonne ordinaire (déjà disponible, rien à
/// attendre), à ne pas confondre avec une dépendance entre deux
/// colonnes-formule.
///
/// Rend `(ordre, bloqués)` : `ordre` contient chaque clé de `edges`
/// **exactement une fois**, dans un ordre où toute dépendance précède son
/// dépendant, pour tout nœud qui n'est impliqué dans aucun cycle ;
/// `bloqués` contient le reste — les nœuds d'un cycle **et** tout nœud qui
/// en dépend, directement ou transitivement, puisque son ordre de calcul
/// n'est de toute façon pas définissable tant que le cycle n'est pas résolu.
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

    /// Colonne mixte de test : `None` = vide, `Some(Ok(n))` = nombre,
    /// `Some(Err(texte))` = texte non numérique.
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
        let formula = Formula::parse("A+B").expect("parse");
        assert_eq!(formula.referenced_columns(), BTreeSet::from([0, 1]));
    }

    #[test]
    fn parses_parentheses_and_precedence() {
        let formula = Formula::parse("(A-B)*2").expect("parse");
        let columns = [float_column(&[10.0]), float_column(&[3.0])];
        let result = evaluate(&formula, 2, &columns, 1);
        assert_eq!(result.values, vec![14.0]);
        assert_eq!(result.error, None);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let formula = Formula::parse("A+B*2").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[3.0])];
        let result = evaluate(&formula, 2, &columns, 1);
        assert_eq!(result.values, vec![7.0]);
    }

    #[test]
    fn parses_division() {
        let formula = Formula::parse("A/B").expect("parse");
        let columns = [float_column(&[10.0]), float_column(&[4.0])];
        let result = evaluate(&formula, 2, &columns, 1);
        assert_eq!(result.values, vec![2.5]);
    }

    #[test]
    fn parses_multi_letter_column_and_unary_minus() {
        let formula = Formula::parse("-AA + 1").expect("parse");
        assert_eq!(formula.referenced_columns(), BTreeSet::from([26]));
    }

    #[test]
    fn column_letters_are_case_insensitive() {
        let formula = Formula::parse("a+b").expect("parse");
        assert_eq!(formula.referenced_columns(), BTreeSet::from([0, 1]));
    }

    #[test]
    fn ignores_whitespace() {
        let formula = Formula::parse("  A   +  B  ").expect("parse");
        assert_eq!(formula.referenced_columns(), BTreeSet::from([0, 1]));
    }

    #[test]
    fn rejects_empty_formula() {
        assert!(matches!(Formula::parse(""), Err(FormulaError::Parse(_))));
    }

    #[test]
    fn rejects_unmatched_parenthesis() {
        assert!(matches!(
            Formula::parse("(A+B"),
            Err(FormulaError::Parse(_))
        ));
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(matches!(
            Formula::parse("A+B)"),
            Err(FormulaError::Parse(_))
        ));
    }

    #[test]
    fn rejects_unknown_character() {
        assert!(matches!(
            Formula::parse("A+B#2"),
            Err(FormulaError::Parse(_))
        ));
    }

    #[test]
    fn rejects_dangling_operator() {
        assert!(matches!(Formula::parse("A+"), Err(FormulaError::Parse(_))));
    }

    // -- évaluation, dégradation --

    #[test]
    fn evaluate_degrades_to_nan_on_parse_failure_when_caller_uses_parse_first() {
        // C'est le contrat que `evaluate` documente : une formule qui ne
        // parse pas n'atteint jamais `evaluate` (l'appelant garde
        // l'`Err(FormulaError::Parse)` de `Formula::parse`) ; ce test fige
        // seulement que `Formula::parse` échoue bien pour ce cas, la
        // dégradation NaN étant testée par l'orchestrateur (stokesforge-session).
        assert!(Formula::parse("A@B").is_err());
    }

    #[test]
    fn missing_column_pads_with_nan() {
        let formula = Formula::parse("A+B").expect("parse");
        let columns = [float_column(&[1.0, 2.0, 3.0]), float_column(&[10.0])];
        let result = evaluate(&formula, 2, &columns, 3);
        assert_eq!(result.error, None);
        assert_eq!(result.values[0], 11.0);
        assert!(result.values[1].is_nan());
        assert!(result.values[2].is_nan());
    }

    #[test]
    fn unknown_column_letter_degrades_everything_to_nan() {
        let formula = Formula::parse("A+C").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[2.0])];
        let result = evaluate(&formula, 5, &columns, 3);
        assert_eq!(
            result.error,
            Some(FormulaError::UnknownColumn("C".to_string()))
        );
        assert!(result.values.iter().all(|v| v.is_nan()));
        assert_eq!(result.values.len(), 3);
    }

    #[test]
    fn a_column_with_text_cells_degrades_only_the_affected_rows() {
        // Il n'y a plus de type de colonne `Text` homogène à refuser d'un
        // bloc : une colonne dont certaines cellules sont du texte est une
        // colonne `Mixed`, acceptée par la formule — seules les lignes dont
        // la cellule référencée ne vaut pas un nombre deviennent `NaN`, pas
        // toute la formule (invariant 10).
        let formula = Formula::parse("A+B").expect("parse");
        let columns = [
            float_column(&[1.0, 2.0]),
            text_column(&[Some("10"), Some("x")]),
        ];
        let result = evaluate(&formula, 5, &columns, 2);
        assert_eq!(result.error, None);
        assert!(
            result.values[0].is_nan(),
            "\"10\" en texte n'est jamais réanalysé en nombre"
        );
        assert!(result.values[1].is_nan());
    }

    #[test]
    fn self_reference_degrades_to_nan() {
        let formula = Formula::parse("A+B").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[2.0])];
        // La colonne-formule est en position 0 (« A ») : elle se référence
        // elle-même.
        let result = evaluate(&formula, 0, &columns, 2);
        assert_eq!(
            result.error,
            Some(FormulaError::SelfReference("A".to_string()))
        );
        assert!(result.values.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn division_by_zero_is_ieee_not_an_error() {
        // Comportement flottant ordinaire, pas un cas de dégradation
        // spécifique au module : 0/0 = NaN, x/0 = ±inf, exactement comme
        // toute valeur manquante ou non finie déjà tolérée par le modèle.
        let formula = Formula::parse("A/B").expect("parse");
        let columns = [float_column(&[1.0, 0.0]), float_column(&[0.0, 0.0])];
        let result = evaluate(&formula, 5, &columns, 2);
        assert_eq!(result.error, None);
        assert!(result.values[0].is_infinite());
        assert!(result.values[1].is_nan());
    }

    // -- statut ligne à ligne (Ok / Missing / Failed) --

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
        // L'exemple de la tâche : A = [1,2,3,4,""], B = [1,1,"zut",1,1],
        // C = A+B doit rendre, ligne à ligne : Ok(2), Ok(3), Failed, Ok(5),
        // Missing (A vide, B présent : la case résultat existe quand même
        // avec la valeur de B, comme aujourd'hui — seul le statut est
        // nouveau, pas la valeur).
        let formula = Formula::parse("A+B").expect("parse");
        let a = float_column(&[1.0, 2.0, 3.0, 4.0, f64::NAN]);
        let b = mixed_column(&[
            Some(Ok(1.0)),
            Some(Ok(1.0)),
            Some(Err("zut")),
            Some(Ok(1.0)),
            Some(Ok(1.0)),
        ]);
        let columns = [a, b];
        let result = evaluate(&formula, 5, &columns, 5);
        assert_eq!(result.error, None);

        assert_eq!(result.values[0], 2.0);
        assert_eq!(result.statuses[0], NumericStatus::Ok);

        assert_eq!(result.values[1], 3.0);
        assert_eq!(result.statuses[1], NumericStatus::Ok);

        assert!(result.values[2].is_nan());
        assert_eq!(result.statuses[2], NumericStatus::Failed);

        assert_eq!(result.values[3], 5.0);
        assert_eq!(result.statuses[3], NumericStatus::Ok);

        // A manquant (NaN), B = 1.0 : la valeur suit la convention de
        // propagation déjà en place (NaN + 1.0 = NaN), et le statut est
        // Missing (pas Failed : rien n'a échoué, il manque juste une
        // opérande).
        assert!(result.values[4].is_nan());
        assert_eq!(result.statuses[4], NumericStatus::Missing);
    }

    #[test]
    fn evaluate_status_is_ok_for_plain_arithmetic() {
        let formula = Formula::parse("A*B").expect("parse");
        let columns = [float_column(&[2.0]), float_column(&[3.0])];
        let result = evaluate(&formula, 5, &columns, 1);
        assert_eq!(result.values, vec![6.0]);
        assert_eq!(result.statuses, vec![NumericStatus::Ok]);
    }

    #[test]
    fn evaluate_status_is_missing_when_both_operands_are_missing() {
        let formula = Formula::parse("A+B").expect("parse");
        let columns = [mixed_column(&[None]), mixed_column(&[None])];
        let result = evaluate(&formula, 5, &columns, 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses[0], NumericStatus::Missing);
    }

    #[test]
    fn evaluate_status_failed_wins_over_missing() {
        // A vide, B texte non numérique : Failed doit l'emporter sur
        // Missing (jamais un vrai échec caché derrière une case vide).
        let formula = Formula::parse("A+B").expect("parse");
        let columns = [mixed_column(&[None]), mixed_column(&[Some(Err("x"))])];
        let result = evaluate(&formula, 5, &columns, 1);
        assert!(result.values[0].is_nan());
        assert_eq!(result.statuses[0], NumericStatus::Failed);
    }

    #[test]
    fn degraded_evaluation_reports_missing_status_everywhere() {
        let formula = Formula::parse("A+C").expect("parse");
        let columns = [float_column(&[1.0]), float_column(&[2.0])];
        let result = evaluate(&formula, 5, &columns, 3);
        assert!(result.error.is_some());
        assert_eq!(result.statuses, vec![NumericStatus::Missing; 3]);
    }

    // -- lettres --

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

    // -- ordre topologique / cycles --

    #[test]
    fn topological_order_respects_dependencies() {
        // C dépend de B, B dépend de A : A doit sortir avant B, B avant C.
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
        // A dépend de B, B dépend de A.
        let mut edges = BTreeMap::new();
        edges.insert("A", BTreeSet::from(["B"]));
        edges.insert("B", BTreeSet::from(["A"]));
        let (order, blocked) = topological_order(&edges);
        assert!(order.is_empty());
        assert_eq!(blocked, BTreeSet::from(["A", "B"]));
    }

    #[test]
    fn topological_order_blocks_transitive_dependents_of_a_cycle() {
        // A <-> B (cycle), C dépend de B.
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
        // "A" dépend de "X", qui n'est pas une clé de `edges` (colonne
        // ordinaire, pas une colonne-formule) : ce n'est pas une dépendance
        // à ordonner, "A" n'attend rien.
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

    // -- plafond de profondeur (#289) --

    /// Exécute `f` sur un thread de 2 MiB (celui de l'acteur de session) ;
    /// en debug, c'est le cas le plus serré.
    fn sur_2_mib<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn trop_profonde(src: String) -> bool {
        sur_2_mib(move || Formula::parse(&src) == Err(FormulaError::TooDeep))
    }

    #[test]
    fn formule_profonde_refusee_sans_depasser_la_pile() {
        // Formes qui avortaient le processus avant le plafond.
        assert!(trop_profonde(format!(
            "{}A{}",
            "(".repeat(100_000),
            ")".repeat(100_000)
        )));
        assert!(trop_profonde(format!("{}A", "A+".repeat(100_000))));
        assert!(trop_profonde(format!("{}A", "A*".repeat(100_000))));
        assert!(trop_profonde(format!("{}A", "-".repeat(100_000))));
    }

    #[test]
    fn formule_juste_sous_le_plafond_acceptee_et_evaluee() {
        let v = sur_2_mib(|| {
            let columns = [float_column(&[2.0])];
            let somme = Formula::parse(&format!("{}A", "A+".repeat(MAX_DEPTH - 2))).unwrap();
            let n = MAX_DEPTH / NEST_COST - 1;
            let parens = Formula::parse(&format!("{}A{}", "(".repeat(n), ")".repeat(n))).unwrap();
            let neg = Formula::parse(&format!("{}A", "-".repeat(n))).unwrap();
            let val = |f: &Formula| {
                let r = evaluate(f, 5, &columns, 1);
                assert_eq!(r.error, None);
                r.values[0]
            };
            (val(&somme), val(&parens), val(&neg))
        });
        assert_eq!(v, (2.0 * (MAX_DEPTH - 1) as f64, 2.0, -2.0));
    }

    // -- blindage du plafond (#289) : tailles limites codées en dur, jamais
    // dérivées de `MAX_DEPTH`, pour qu'un changement du plafond ou des coûts
    // casse ces tests au lieu de les suivre. `FormulaError::TooDeep` est une
    // variante sans champ : il n'y a pas de position à vérifier (contrairement
    // à `ExprErrorKind::TooDeep` de `analysis::expr`).

    /// Exécute `f` sur un thread dont la pile fait `octets` ; en cas de
    /// débordement le processus entier avorte (pas de panic récupérable).
    fn sur_pile<R: Send + 'static>(octets: usize, f: impl FnOnce() -> R + Send + 'static) -> R {
        std::thread::Builder::new()
            .stack_size(octets)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    const UN_MIB: usize = 1 << 20;
    const DEUX_MIB: usize = 2 << 20;

    /// `n` termes `A` joints par `op` (donc `n - 1` opérateurs).
    fn chaine(op: &str, termes: usize) -> String {
        vec!["A"; termes].join(op)
    }

    fn parens(n: usize, milieu: &str) -> String {
        format!("{}{milieu}{}", "(".repeat(n), ")".repeat(n))
    }

    fn parse_ok(src: &str) -> bool {
        match Formula::parse(src) {
            Ok(_) => true,
            Err(FormulaError::TooDeep) => false,
            Err(autre) => panic!("erreur inattendue : {autre:?}"),
        }
    }

    #[test]
    fn plafond_exact_d_une_chaine_d_operateurs() {
        // 1001 termes = 1000 opérateurs = coût 1000 : accepté ; 1002 : refusé.
        for op in ["+", "-", "*", "/"] {
            for (termes, attendu) in [(999, true), (1000, true), (1001, true), (1002, false)] {
                assert_eq!(
                    parse_ok(&chaine(op, termes)),
                    attendu,
                    "{termes} termes `{op}`"
                );
            }
        }
    }

    #[test]
    fn plafond_exact_des_parentheses_imbriquees() {
        for (n, attendu) in [(99, true), (100, true), (101, false)] {
            assert_eq!(parse_ok(&parens(n, "A")), attendu, "{n} parenthèses");
        }
    }

    #[test]
    fn plafond_exact_des_signes_empiles() {
        for signe in ["-", "+"] {
            for (n, attendu) in [(99, true), (100, true), (101, false)] {
                let src = format!("{}A", signe.repeat(n));
                assert_eq!(parse_ok(&src), attendu, "{n} signes `{signe}`");
            }
        }
        // Signes mêlés : 100 signes acceptés, 101 refusés.
        assert!(parse_ok(&format!("{}A", "-+".repeat(50))));
        assert!(!parse_ok(&format!("{}A", "-+".repeat(50) + "-")));
        assert!(!parse_ok(&format!("{}A", "-+".repeat(51))));
    }

    #[test]
    fn les_couts_se_cumulent_entre_parentheses_et_chaine() {
        // 99 parenthèses (990) + 10 opérateurs (10) = 1000 : accepté ; 11 : refusé.
        assert!(parse_ok(&parens(99, &chaine("+", 11))));
        assert!(!parse_ok(&parens(99, &chaine("+", 12))));
        // 98 parenthèses (980) + 20 opérateurs.
        assert!(parse_ok(&parens(98, &chaine("*", 21))));
        assert!(!parse_ok(&parens(98, &chaine("*", 22))));
        // Un signe en tête de chaîne coûte 10 pendant la lecture de `A`
        // seulement, puis est restitué : la chaîne garde ses 1000.
        assert!(parse_ok(&format!("-{}", chaine("+", 1001))));
        assert!(!parse_ok(&format!("-{}", chaine("+", 1002))));
        // Signe appliqué au dernier terme : `A+A+…+-A` -> 990 opérateurs + 10.
        assert!(parse_ok(&format!("{}-A", "A+".repeat(990))));
        assert!(!parse_ok(&format!("{}-A", "A+".repeat(991))));
        // Idem multiplicatif : `A*-A*-…*-A`.
        assert!(parse_ok(&format!("{}A", "A*-".repeat(990))));
        assert!(!parse_ok(&format!("{}A", "A*-".repeat(991))));
    }

    #[test]
    fn la_profondeur_est_rendue_apres_chaque_groupe_et_chaque_terme() {
        // Groupes séquentiels : 991 groupes = 990 opérateurs + 10 du dernier.
        // Sans la restitution des 10 par `nested`, 101 groupes seraient déjà
        // refusés.
        let groupes = |n: usize| vec!["(A)"; n].join("+");
        assert!(parse_ok(&groupes(991)));
        assert!(!parse_ok(&groupes(992)));
        // Une somme entre parenthèses rend ses `+` en sortant : 120 groupes de
        // 10 termes (9 `+` chacun) ne laissent aucun reste, sinon 120 * 9 > 1000.
        let groupe = format!("({})", chaine("+", 10));
        assert!(parse_ok(&vec![groupe.as_str(); 120].join("+")));
        // Les termes d'un produit se remettent à zéro à chaque `+` : 600
        // produits `A*A` enchaînés = 599 `+` + 1 `*` = 600, pas 1199.
        let produits = vec!["A*A"; 600].join("+");
        assert!(parse_ok(&produits));
        // 500 `+` puis un produit de 500 `*` = 1000 : pile à la limite.
        let somme_puis_produit =
            |etoiles: usize| format!("{}+{}", chaine("+", 500), chaine("*", etoiles));
        assert!(parse_ok(&somme_puis_produit(501)));
        assert!(!parse_ok(&somme_puis_produit(502)));
        // Un produit de 1000 `*` suivi de sommes : il ne s'ajoute pas aux `+`.
        assert!(parse_ok(&format!(
            "{}+{}",
            chaine("*", 1001),
            chaine("+", 500)
        )));
    }

    #[test]
    fn toute_forme_sans_cout_declare_est_refusee_a_10_000() {
        let n = 10_000;
        let formes = [
            ("parenthèses fermées", parens(n, "A")),
            ("parenthèses jamais refermées", "(".repeat(n)),
            ("moins empilés", format!("{}A", "-".repeat(n))),
            ("plus empilés", format!("{}A", "+".repeat(n))),
            ("signes alternés", format!("{}A", "-+".repeat(n / 2))),
            ("somme", chaine("+", n)),
            ("différence", chaine("-", n)),
            ("produit", chaine("*", n)),
            ("quotient", chaine("/", n)),
            ("somme/produit alternés", chaine("+A*", n / 2) + "A"),
            (
                "imbrication à droite",
                format!("{}A{}", "A+(".repeat(n), ")".repeat(n)),
            ),
            (
                "signe puis parenthèse",
                format!("{}A{}", "-(".repeat(n), ")".repeat(n)),
            ),
            ("produit de signes", format!("{}A", "A*-".repeat(n))),
        ];
        for (nom, src) in formes {
            sur_pile(UN_MIB, move || {
                assert_eq!(Formula::parse(&src), Err(FormulaError::TooDeep), "{nom}");
            });
        }
    }

    #[test]
    fn un_million_d_elements_est_refuse_vite() {
        let debut = std::time::Instant::now();
        sur_pile(UN_MIB, || {
            for src in [
                "(".repeat(1_000_000),
                "-".repeat(1_000_000),
                "A+".repeat(500_000),
                parens(500_000, "A"),
            ] {
                assert_eq!(Formula::parse(&src), Err(FormulaError::TooDeep));
            }
        });
        // Linéaire (quelques dizaines de ms en debug) ; la marge n'est là que
        // pour attraper un parcours quadratique ou un balayage complet.
        assert!(debut.elapsed().as_secs() < 10, "{:?}", debut.elapsed());
    }

    /// Les pires formules acceptées, une par forme de récursion : chaîne
    /// d'opérateurs (arbre profond de 1000), parenthèses (parseur), signes
    /// (arbre `Neg` et parseur), mélanges au coût exact du plafond.
    fn pires_formules() -> Vec<(&'static str, String)> {
        vec![
            ("somme", chaine("+", 1001)),
            ("différence", chaine("-", 1001)),
            ("produit", chaine("*", 1001)),
            ("quotient", chaine("/", 1001)),
            ("parenthèses", parens(100, "A")),
            ("moins empilés", format!("{}A", "-".repeat(100))),
            ("plus empilés", format!("{}A", "+".repeat(100))),
            ("parenthèses et somme", parens(99, &chaine("+", 11))),
            (
                "imbrication à droite",
                format!("{}A{}", "A+(".repeat(90), ")".repeat(90)),
            ),
            (
                "signe puis parenthèse",
                format!("{}A{}", "-(".repeat(50), ")".repeat(50)),
            ),
            (
                "somme finie par un signe",
                format!("{}-A", "A+".repeat(990)),
            ),
            ("produit de signes", format!("{}A", "A*-".repeat(990))),
            ("groupes séquentiels", vec!["(A)"; 991].join("+")),
        ]
    }

    /// Parse, lit les références, évalue, clone, compare et lâche la formule.
    fn parcours_complet(src: &str) -> f64 {
        let formule = Formula::parse(src).expect("pire formule acceptée");
        let colonnes = [float_column(&[2.0, 3.0])];
        let copie = formule.clone();
        assert_eq!(formule, copie);
        assert_eq!(formule.referenced_columns(), BTreeSet::from([0]));
        let r = evaluate(&formule, 5, &colonnes, 2);
        assert_eq!(r.error, None);
        r.values[0]
    }

    #[test]
    fn les_pires_formules_acceptees_tiennent_sur_1_mib_et_2_mib() {
        // Un débordement de pile avorte le processus : ce test n'« échoue »
        // pas, il tue le binaire de test, ce qui le rend impossible à rater.
        // Marge mesurée en debug (pile réduite jusqu'à l'avortement, pire
        // forme = chaîne `+`/`-`/`*`/`/` de 1001 termes) : il faut environ
        // 560 Kio ; 1 Mio donne un facteur 1,8, 2 Mio (l'acteur) un facteur 3,6.
        // Les parenthèses seules demandent ~425 Kio (parseur), les signes
        // empilés ~95 Kio.
        for octets in [DEUX_MIB, UN_MIB] {
            for (nom, src) in pires_formules() {
                let valeur = sur_pile(octets, move || parcours_complet(&src));
                assert!(!valeur.is_nan(), "{nom} sur {octets} octets");
            }
        }
    }

    #[test]
    fn les_formules_limites_ont_la_bonne_valeur() {
        // A = 2.0 sur la première ligne.
        let attendu = [
            ("somme", 2.0 * 1001.0),
            ("différence", 2.0 - 2.0 * 1000.0),
            ("produit", 2f64.powi(1001)),
            ("quotient", 2.0 / 2f64.powi(1000)),
            ("parenthèses", 2.0),
            ("moins empilés", 2.0), // 100 négations
            ("plus empilés", 2.0),
            ("parenthèses et somme", 22.0),
            ("imbrication à droite", 2.0 * 91.0),
            ("signe puis parenthèse", 2.0), // 50 négations
            ("somme finie par un signe", 2.0 * 990.0 - 2.0),
            ("produit de signes", 2f64.powi(991)),
            ("groupes séquentiels", 2.0 * 991.0),
        ];
        let pires = pires_formules();
        assert_eq!(pires.len(), attendu.len());
        for ((nom, src), (nom_attendu, valeur)) in pires.into_iter().zip(attendu) {
            assert_eq!(nom, nom_attendu);
            let obtenu = sur_pile(DEUX_MIB, move || parcours_complet(&src));
            assert_eq!(obtenu, valeur, "{nom}");
        }
        // Une négation de moins : signe inversé.
        let impair = sur_pile(DEUX_MIB, || {
            parcours_complet(&format!("{}A", "-".repeat(99)))
        });
        assert_eq!(impair, -2.0);
    }

    #[test]
    fn les_formules_ordinaires_sont_inchangees() {
        let colonnes = [float_column(&[2.0, 8.0]), float_column(&[4.0, 2.0])];
        let valeurs = |src: &str| {
            let f = Formula::parse(src).unwrap();
            let r = evaluate(&f, 9, &colonnes, 2);
            assert_eq!(r.error, None, "{src}");
            r.values
        };
        assert_eq!(valeurs("A+B"), [6.0, 10.0]);
        assert_eq!(valeurs("(A+B)*2-1"), [11.0, 19.0]);
        // Associativité à gauche : (10-4)-3, (100/10)/5, pas 10-(4-3).
        assert_eq!(valeurs("10-4-3"), [3.0, 3.0]);
        assert_eq!(valeurs("100/10/5"), [2.0, 2.0]);
        assert_eq!(valeurs("2+3*4"), [14.0, 14.0]);
        assert_eq!(valeurs("-A"), [-2.0, -8.0]);
        assert_eq!(valeurs("--A"), [2.0, 8.0]);
        assert_eq!(valeurs("+A"), [2.0, 8.0]);
        assert_eq!(valeurs("A*-B"), [-8.0, -16.0]);
        assert_eq!(valeurs("((((A))))"), [2.0, 8.0]);
        assert_eq!(valeurs(" A / B "), [0.5, 4.0]);
        assert_eq!(valeurs("1e1+.5"), [10.5, 10.5]);
    }

    #[test]
    fn les_colonnes_referencees_d_une_formule_limite_sont_exactes() {
        // Chaîne de 1001 termes tournant sur A, B, C et AA.
        let lettres = ["A", "B", "C", "AA"];
        let termes: Vec<&str> = (0..1001).map(|i| lettres[i % 4]).collect();
        let src = termes.join("+");
        let refs = sur_pile(UN_MIB, move || {
            Formula::parse(&src).unwrap().referenced_columns()
        });
        assert_eq!(refs, BTreeSet::from([0, 1, 2, 26]));
        // Une référence unique tout au fond d'un empilement limite.
        let profond = format!("{}C{}", "(-".repeat(50), ")".repeat(50));
        let refs = sur_pile(UN_MIB, move || {
            Formula::parse(&profond).unwrap().referenced_columns()
        });
        assert_eq!(refs, BTreeSet::from([2]));
        // Rien à référencer : ensemble vide.
        let refs = sur_pile(UN_MIB, || {
            Formula::parse(&vec!["1"; 1001].join("+"))
                .unwrap()
                .referenced_columns()
        });
        assert!(refs.is_empty());
    }

    #[test]
    fn une_formule_trop_profonde_degrade_evaluate_apres_un_parse_refuse() {
        // `evaluate` ne reçoit une `Formula` qu'après `parse` : un refus ne
        // laisse donc aucune formule à évaluer ; l'erreur est celle du parse.
        let erreur = Formula::parse(&chaine("+", 1002)).unwrap_err();
        assert_eq!(erreur, FormulaError::TooDeep);
        assert_eq!(erreur.to_string(), "formula nested too deeply");
    }
}
