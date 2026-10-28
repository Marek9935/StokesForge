//! Reading the sources of `stokesforge-app` **and `stokesforge-app-state`**
//! for the guards that read the code (#116, work package #115; #263).
//!
//! # A single list of files
//!
//! [`sources`] reads `src/` **recursively**: a module moved into `src/app/`
//! is still read without declaring anything. Every guard that reads the code
//! goes through it — `tests/labels.rs`, `tests/history_text.rs`, `tests/guards.rs`
//! and, through `src/lib.rs`, the tests of `icons`, `theme` and `i18n`. None of
//! them names a file anymore: moving a function does not take it out of any
//! guard.
//!
//! # A single "production" view
//!
//! [`SourceFile::production`] blanks the comments and **every** `#[cfg(test)]`
//! item — not only what follows the first one: `errors.rs` alternates code
//! and test modules. A test module written in its own file
//! (`#[cfg(test)] mod tests;`, with or without `#[path]`) makes that whole file
//! test code ([`SourceFile::is_test`]), and every module it declares in turn.
//! The blanks keep the line ends: the reported line numbers stay right.
//!
//! # Limits, all loud
//!
//! Not recognized: `#[cfg(all(test, …))]` / `#[cfg(any(test, …))]`, and a
//! `#[cfg(test)] mod x;` declared **inside an inline module** (`mod a { … }`),
//! whose file is looked up in the wrong place. In both cases, test code is read
//! as production: a false positive of a guard, never a hole. Write `#[cfg(test)]`
//! alone, on a module declared at file level.
//!
//! # Inclusion
//!
//! This file is not a test by itself (Cargo only discovers `tests/*.rs`): it is
//! included through `#[path]` where it is used. Its witnesses are in
//! `tests/guards.rs`.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The list of files
// ---------------------------------------------------------------------------

/// A source file of `src/`.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path relative to `src/`, separated by `/`: `app.rs`, `i18n/pseudo.rs`.
    pub path: String,
    pub text: String,
    /// The whole file is test code: it carries a module declared under
    /// `#[cfg(test)]` (or declared by a file that is itself test code).
    pub is_test: bool,
}

impl SourceFile {
    /// The file seen as production code: comments and `#[cfg(test)]` items
    /// blanked; everything blanked if the file is test code.
    pub fn production(&self) -> Mask {
        let mut source = mask_source(&self.text);
        if self.is_test || is_entirely_test(&source.mask) {
            let end = source.mask.len();
            blank_out(&mut source, 0, end);
        } else {
            for zone in test_zones(&source.mask) {
                blank_out(&mut source, zone.0, zone.1);
            }
        }
        source
    }

    /// The text without its comments, strings intact, tests included.
    pub fn code(&self) -> String {
        mask_source(&self.text).code.into_iter().collect()
    }
}

/// The `src/` directory of `stokesforge-app`.
pub fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The `src/` directory of `stokesforge-app-state`, the state without egui (#263).
pub fn state_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("stokesforge-app-state")
        .join("src")
}

/// The prefix of the `stokesforge-app-state` source paths in the list of
/// [`sources`]: `app-state/state.rs`, `app-state/i18n/pseudo.rs`. The sources
/// of `stokesforge-app` have none (`app.rs`, `app/menus.rs`).
pub const STATE_PREFIX: &str = "app-state/";

/// All the application sources, subdirectories included, sorted by path:
/// those of `stokesforge-app` (path relative to its `src/`) **and** those of
/// `stokesforge-app-state` (same path, prefixed with [`STATE_PREFIX`]). The
/// interface text lives in both.
pub fn sources() -> Vec<SourceFile> {
    read_dirs(&[
        (src_dir().as_path(), ""),
        (state_src_dir().as_path(), STATE_PREFIX),
    ])
}

/// All the `.rs` sources under `root`, subdirectories included, sorted by
/// path; the test files are marked.
pub fn read_sources(root: &Path) -> Vec<SourceFile> {
    read_dirs(&[(root, "")])
}

/// The `.rs` sources of each `(root, prefix)`, subdirectories included, whose
/// path is preceded by its prefix; sorted by path, test files marked.
pub fn read_dirs(dirs: &[(&Path, &str)]) -> Vec<SourceFile> {
    let mut paths: Vec<(PathBuf, String)> = Vec::new();
    for (root, prefix) in dirs {
        let mut relatives = Vec::new();
        walk(root, "", &mut relatives);
        for relative in relatives {
            paths.push((root.join(&relative), format!("{prefix}{relative}")));
        }
    }
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    let mut files: Vec<SourceFile> = paths
        .into_iter()
        .map(|(full, path)| {
            let text = std::fs::read_to_string(full).expect("readable source");
            SourceFile {
                path,
                text,
                is_test: false,
            }
        })
        .collect();
    mark_test_files(&mut files);
    files
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("sources directory").flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = format!("{prefix}{name}");
        if path.is_dir() {
            walk(&path, &format!("{relative}/"), out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(relative);
        }
    }
}

// ---------------------------------------------------------------------------
// The test files
// ---------------------------------------------------------------------------

/// A `mod name;` declaration whose module lives in another file.
struct Declaration {
    position: usize,
    name: String,
    /// The value of a `#[path = "…"]` placed on the declaration.
    path: Option<String>,
}

/// Marks, up to a fixed point, the files of the modules declared under
/// `#[cfg(test)]` and those declared by a file that is already test code.
fn mark_test_files(files: &mut [SourceFile]) {
    let known: BTreeSet<String> = files.iter().map(|f| f.path.clone()).collect();
    for file in files.iter_mut() {
        file.is_test = file.is_test || is_entirely_test(&mask_source(&file.text).mask);
    }
    loop {
        let mut nouveaux = BTreeSet::new();
        for file in files.iter() {
            let source = mask_source(&file.text);
            let zones = test_zones(&source.mask);
            for declaration in declarations(&source) {
                let under_test = file.is_test
                    || zones
                        .iter()
                        .any(|(start, end)| (*start..*end).contains(&declaration.position));
                if !under_test {
                    continue;
                }
                if let Some(target) = module_file(&file.path, &declaration)
                    .into_iter()
                    .find(|candidate| known.contains(candidate))
                {
                    nouveaux.insert(target);
                }
            }
        }
        let mut change = false;
        for file in files.iter_mut() {
            if !file.is_test && nouveaux.contains(&file.path) {
                file.is_test = true;
                change = true;
            }
        }
        if !change {
            return;
        }
    }
}

/// The `mod name;` declarations (not the inline module `mod name { … }`).
fn declarations(source: &Mask) -> Vec<Declaration> {
    let mask = &source.mask;
    let mut found = Vec::new();
    for i in 0..mask.len() {
        if !word_at(mask, i, "mod") {
            continue;
        }
        let mut j = i + 3;
        while mask.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        let name = ident_at(mask, j);
        if name.is_empty() {
            continue;
        }
        j += name.chars().count();
        while mask.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if mask.get(j) != Some(&';') {
            continue;
        }
        let start = item_start(mask, i);
        found.push(Declaration {
            position: i,
            name,
            path: path_attribute(source, start, i),
        });
    }
    found
}

/// The value of the `#[path = "…"]` written between `start` and `end`, if any.
fn path_attribute(source: &Mask, start: usize, end: usize) -> Option<String> {
    let mask = &source.mask;
    let attribute = (start..end).find(|k| starts_with_at(mask, *k, "#[path"))?;
    let open_quote = (attribute..end).find(|k| mask[*k] == '"')?;
    let close_quote = (open_quote + 1..end).find(|k| mask[*k] == '"')?;
    Some(
        source.original[open_quote + 1..close_quote]
            .iter()
            .collect(),
    )
}

/// The files where the module `declaration`, declared in `path`, can live —
/// rustc rules: next to a `lib.rs` / `main.rs` / `mod.rs`, otherwise in the
/// directory of the same name; a `#[path]` is relative to the file's directory.
fn module_file(path: &str, declaration: &Declaration) -> Vec<String> {
    let (dir, file) = match path.rsplit_once('/') {
        Some((dir, file)) => (format!("{dir}/"), file),
        None => (String::new(), path),
    };
    if let Some(attribute) = &declaration.path {
        // A path that leaves `src/` designates no source read.
        return normalize(&format!("{dir}{attribute}"))
            .into_iter()
            .collect();
    }
    let base = if ["lib.rs", "main.rs", "mod.rs"].contains(&file) {
        dir
    } else {
        format!("{dir}{}/", file.trim_end_matches(".rs"))
    };
    let name = &declaration.name;
    vec![format!("{base}{name}.rs"), format!("{base}{name}/mod.rs")]
}

/// `a/./b/../c.rs` → `a/c.rs`; `None` if the path goes above `src/`
/// (`../tests/support/sources.rs`).
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

// ---------------------------------------------------------------------------
// The test items
// ---------------------------------------------------------------------------

/// The file opens with an inner attribute `#![cfg(test)]` (among its leading
/// inner attributes): it is entirely test code.
fn is_entirely_test(mask: &[char]) -> bool {
    let mut k = 0;
    loop {
        while mask.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        if starts_with_at(mask, k, "#![cfg(test)]") {
            return true;
        }
        if !starts_with_at(mask, k, "#![") {
            return false;
        }
        match closing(mask, k + 2) {
            Some(end) => k = end + 1,
            None => return false,
        }
    }
}

/// The ranges `[start, end)` of the `#[cfg(test)]` elements: their attributes,
/// then the item up to its `;` or to the brace that closes its body — or, for
/// a field, a `match` arm, an argument, up to its comma.
pub fn test_zones(mask: &[char]) -> Vec<(usize, usize)> {
    const ATTRIBUTE: &str = "#[cfg(test)]";
    let mut zones = Vec::new();
    let mut i = 0;
    while i < mask.len() {
        if !starts_with_at(mask, i, ATTRIBUTE) {
            i += 1;
            continue;
        }
        let start = item_start(mask, i);
        let end = item_end(mask, i + ATTRIBUTE.len());
        zones.push((start, end));
        i = end.max(i + 1);
    }
    zones
}

/// The start of the item whose attribute or keyword is at `position`: steps
/// back over whitespace, outer attributes `#[…]` and visibility.
fn item_start(mask: &[char], position: usize) -> usize {
    let mut start = position;
    loop {
        let mut k = start;
        while k > 0 && mask[k - 1].is_whitespace() {
            k -= 1;
        }
        if k > 0 && mask[k - 1] == ']' {
            match opening(mask, k - 1) {
                Some(bracket) if bracket > 0 && mask[bracket - 1] == '#' => {
                    start = bracket - 1;
                    continue;
                }
                _ => {}
            }
        }
        if k > 0 && mask[k - 1] == ')' {
            if let Some(paren) = opening(mask, k - 1) {
                if paren >= 3 && word_at(mask, paren - 3, "pub") {
                    start = paren - 3;
                    continue;
                }
            }
        }
        if k >= 3 && word_at(mask, k - 3, "pub") {
            start = k - 3;
            continue;
        }
        return start;
    }
}

/// The words that open an item or a statement ended by `;` or by a body
/// between braces.
const ITEM_KEYWORDS: &[&str] = &[
    "fn",
    "mod",
    "use",
    "impl",
    "struct",
    "enum",
    "union",
    "const",
    "static",
    "type",
    "trait",
    "macro_rules",
    "pub",
    "unsafe",
    "async",
    "extern",
    "let",
];

/// The end (excluded) of the element that follows the attribute ending at
/// `position`, following attributes included. An item (`fn`, `mod`, `struct`…)
/// ends after its top-level `;`, or after the closing brace of its body. Any
/// other element (field, `match` arm, argument) ends at its comma, or before
/// the closing brace or parenthesis that encloses it: it never swallows the
/// code that follows it.
fn item_end(mask: &[char], position: usize) -> usize {
    let mut k = position;
    loop {
        while mask.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        if !starts_with_at(mask, k, "#[") {
            break;
        }
        match closing(mask, k + 1) {
            Some(end) => k = end + 1,
            None => return mask.len(),
        }
    }
    let item = ITEM_KEYWORDS.iter().any(|word| word_at(mask, k, word));
    let mut depth = 0usize;
    while k < mask.len() {
        match mask[k] {
            '(' | '[' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return k,
            ')' | ']' => depth -= 1,
            ';' if depth == 0 => return k + 1,
            ',' if depth == 0 && !item => return k + 1,
            '{' if depth == 0 => {
                let end = closing(mask, k).map_or(mask.len(), |end| end + 1);
                if item {
                    return end;
                }
                k = end;
                continue;
            }
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        k += 1;
    }
    mask.len()
}

fn blank_out(source: &mut Mask, start: usize, end: usize) {
    for k in start..end.min(source.mask.len()) {
        if source.original[k] != '\n' {
            source.mask[k] = ' ';
            source.code[k] = ' ';
        }
    }
}

// ---------------------------------------------------------------------------
// The mask
// ---------------------------------------------------------------------------

/// A source under three views of the same length (in characters), hence at the
/// same positions.
#[derive(Debug, Clone)]
pub struct Mask {
    /// Comments blanked, string contents replaced: `A` for a letter, `_` for
    /// the rest. A pattern searched here is never in a comment or in a string.
    pub mask: Vec<char>,
    /// Comments blanked, strings intact.
    pub code: Vec<char>,
    pub original: Vec<char>,
}

impl Mask {
    pub fn code_as_text(&self) -> String {
        self.code.iter().collect()
    }
}

/// The mask of the text, without a file view: tests included.
pub fn mask_source(text: &str) -> Mask {
    let original: Vec<char> = text.chars().collect();
    let mut mask = original.clone();
    let mut code = original.clone();
    let n = original.len();
    let mut i = 0;
    while i < n {
        let c = original[i];
        let following = original.get(i + 1).copied();
        // Line comment.
        if c == '/' && following == Some('/') {
            while i < n && original[i] != '\n' {
                mask[i] = ' ';
                code[i] = ' ';
                i += 1;
            }
            continue;
        }
        // Block comment, nested.
        if c == '/' && following == Some('*') {
            let mut depth = 0;
            while i < n {
                if original[i] == '/' && original.get(i + 1) == Some(&'*') {
                    depth += 1;
                    for k in [i, i + 1] {
                        mask[k] = ' ';
                        code[k] = ' ';
                    }
                    i += 2;
                } else if original[i] == '*' && original.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    for k in [i, i + 1] {
                        mask[k] = ' ';
                        code[k] = ' ';
                    }
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if original[i] != '\n' {
                        mask[i] = ' ';
                        code[i] = ' ';
                    }
                    i += 1;
                }
            }
            continue;
        }
        let prev_is_ident = i > 0 && is_ident(original[i - 1]);
        // Raw string: r"…", r#"…"#, br#"…"#.
        if (c == 'r' || (c == 'b' && following == Some('r'))) && !prev_is_ident {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut hashes = 0;
            while original.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if original.get(j) == Some(&'"') {
                j += 1;
                while j < n {
                    if original[j] == '"'
                        && (0..hashes).all(|k| original.get(j + 1 + k) == Some(&'#'))
                    {
                        break;
                    }
                    mask_char(&original, &mut mask, j);
                    j += 1;
                }
                i = (j + 1 + hashes).min(n);
                continue;
            }
        }
        // Ordinary string.
        if c == '"' {
            let mut j = i + 1;
            while j < n && original[j] != '"' {
                if original[j] == '\\' {
                    mask[j] = '_';
                    j += 1;
                    if j < n {
                        let escaped = original[j];
                        mask_neutral(&original, &mut mask, j);
                        j += 1;
                        if escaped == 'u' && original.get(j) == Some(&'{') {
                            while j < n && original[j] != '}' {
                                mask_neutral(&original, &mut mask, j);
                                j += 1;
                            }
                            if j < n {
                                mask_neutral(&original, &mut mask, j);
                                j += 1;
                            }
                        }
                    }
                    continue;
                }
                // A format field (`{code}`, `{label:>4}`) is not text: its content
                // is neutral. `{{` is a literal brace.
                if original[j] == '{' && original.get(j + 1) != Some(&'{') {
                    while j < n && original[j] != '}' && original[j] != '"' {
                        mask_neutral(&original, &mut mask, j);
                        j += 1;
                    }
                    continue;
                }
                mask_char(&original, &mut mask, j);
                j += 1;
            }
            i = (j + 1).min(n);
            continue;
        }
        // Character literal (and not a lifetime).
        if c == '\'' {
            if following == Some('\\') {
                let mut j = i + 2;
                while j < n && original[j] != '\'' {
                    mask[j] = '_';
                    j += 1;
                }
                mask[i + 1] = '_';
                i = (j + 1).min(n);
                continue;
            }
            if original.get(i + 2) == Some(&'\'') {
                mask[i + 1] = '_';
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    Mask {
        mask,
        code,
        original,
    }
}

/// In a string: a letter becomes `A`, the rest `_`, except line ends.
fn mask_char(original: &[char], mask: &mut [char], i: usize) {
    mask[i] = if original[i] == '\n' {
        '\n'
    } else if original[i].is_alphabetic() {
        'A'
    } else {
        '_'
    };
}

/// In an escape sequence: never a text letter.
fn mask_neutral(original: &[char], mask: &mut [char], i: usize) {
    mask[i] = if original[i] == '\n' { '\n' } else { '_' };
}

// ---------------------------------------------------------------------------
// Tools to read the mask
// ---------------------------------------------------------------------------

pub fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Position of the closing delimiter that matches the one opened at `opening`.
pub fn closing(mask: &[char], opening: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in mask.iter().enumerate().skip(opening) {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Position of the opening delimiter that matches the one closed at `closing`.
pub fn opening(mask: &[char], closing: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..=closing).rev() {
        match mask[i] {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn starts_with_at(mask: &[char], position: usize, pattern: &str) -> bool {
    pattern
        .chars()
        .enumerate()
        .all(|(k, c)| mask.get(position + k) == Some(&c))
}

/// `pattern` written at `position` as a whole word: neither preceded nor followed
/// by an identifier character.
pub fn word_at(mask: &[char], position: usize, pattern: &str) -> bool {
    starts_with_at(mask, position, pattern)
        && (position == 0 || !is_ident(mask[position - 1]))
        && !mask
            .get(position + pattern.chars().count())
            .is_some_and(|c| is_ident(*c))
}

pub fn ident_at(mask: &[char], position: usize) -> String {
    mask[position.min(mask.len())..]
        .iter()
        .take_while(|c| is_ident(**c))
        .collect()
}

pub fn line_of(original: &[char], position: usize) -> usize {
    1 + original[..position].iter().filter(|c| **c == '\n').count()
}

// ---------------------------------------------------------------------------
// The functions
// ---------------------------------------------------------------------------

/// The functions of the mask: position of the `fn`, name qualified by the
/// `impl` block that contains it (`Type::name`, `name` outside an `impl`).
pub fn functions(mask: &[char]) -> Vec<(usize, String)> {
    let impls = impl_blocks(mask);
    let mut found = Vec::new();
    for i in 0..mask.len() {
        if starts_with_at(mask, i, "fn ") && (i == 0 || !is_ident(mask[i - 1])) {
            let name = ident_at(mask, i + 3);
            if name.is_empty() {
                continue;
            }
            let qualified = impls
                .iter()
                .find(|(start, end, _)| *start < i && i < *end)
                .map_or(name.clone(), |(_, _, target)| format!("{target}::{name}"));
            found.push((i, qualified));
        }
    }
    found
}

/// The last segment of a qualified name: `name` for `Type::name`.
pub fn short_name(qualified: &str) -> &str {
    qualified.rsplit("::").next().unwrap_or(qualified)
}

/// The range `[opening brace, closing brace]` of the body of the function whose
/// `fn` is at `position`; `None` for a declaration without a body.
pub fn body(mask: &[char], position: usize) -> Option<(usize, usize)> {
    // The `;` of an array type (`[u8; 4]`) in the signature is not the end of
    // a declaration: only top-level ones count.
    let mut depth = 0usize;
    for k in position..mask.len() {
        match mask[k] {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => return None,
            '{' if depth == 0 => return Some((k, closing(mask, k)?)),
            _ => {}
        }
    }
    None
}

/// A production function found by [`functions_named`].
#[derive(Debug, Clone)]
pub struct Function {
    pub path: String,
    pub qualified: String,
    pub line: usize,
    /// The body, comments blanked, strings intact.
    pub body: String,
}

/// The production functions named `name` (last segment of the qualified name:
/// a `Type::name` method counts), wherever they are in `sources`.
pub fn functions_named(sources: &[SourceFile], name: &str) -> Vec<Function> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        for (position, qualified) in functions(&source.mask) {
            if short_name(&qualified) != name {
                continue;
            }
            let Some((start, end)) = body(&source.mask, position) else {
                continue;
            };
            found.push(Function {
                path: file.path.clone(),
                qualified,
                line: line_of(&source.original, position),
                body: source.code[start..=end].iter().collect(),
            });
        }
    }
    found
}

/// The only production function named `name` — free or `Type::name` method:
/// a function that became a method is still found (#122). Fails, saying so,
/// if it is not found or if there are several.
pub fn unique_function(sources: &[SourceFile], name: &str) -> Function {
    let mut found = functions_named(sources, name);
    assert!(
        found.len() == 1,
        "`fn {name}` expected once in the production sources of src/, found {} times: {:?}",
        found.len(),
        found
            .iter()
            .map(|f| format!("{}:{}", f.path, f.line))
            .collect::<Vec<_>>()
    );
    found.remove(0)
}

/// The `impl` blocks: (start, end, target type).
pub fn impl_blocks(mask: &[char]) -> Vec<(usize, usize, String)> {
    let mut blocks = Vec::new();
    for i in 0..mask.len() {
        if !starts_with_at(mask, i, "impl")
            || (i > 0 && is_ident(mask[i - 1]))
            || mask.get(i + 4).is_some_and(|c| is_ident(*c))
        {
            continue;
        }
        let Some(brace) = (i..mask.len()).find(|k| mask[*k] == '{' || mask[*k] == ';') else {
            continue;
        };
        if mask[brace] != '{' {
            continue;
        }
        let Some(end) = closing(mask, brace) else {
            continue;
        };
        let header: String = mask[i + 4..brace].iter().collect();
        blocks.push((i, end, impl_target(&header)));
    }
    blocks
}

/// The type targeted by the header of an `impl`: `Type` for `impl Type`,
/// `impl<T> Type<T>`, `impl Trait for Type`.
fn impl_target(header: &str) -> String {
    let mut rest = header.trim();
    if let Some((_, after)) = rest.rsplit_once(" for ") {
        rest = after.trim();
    } else if rest.starts_with('<') {
        let mut depth = 0usize;
        for (k, c) in rest.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        rest = rest[k + 1..].trim();
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    rest.chars().take_while(|c| is_ident(*c)).collect()
}
