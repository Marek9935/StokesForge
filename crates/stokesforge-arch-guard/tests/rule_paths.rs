//! Guards of the always-loaded rules and of the path-scoped rule files (#516).
//!
//! `.claude/rules/*.md` are loaded by Claude Code only when a file matching
//! their `paths:` frontmatter is read, edited or written. A rule file whose
//! frontmatter is broken is loaded at every start (bad YAML) or never (a glob
//! that matches nothing), silently. This guard checks that each file has a valid
//! `paths:` list (one glob per line, a plain YAML list, no braces) and that each
//! glob matches at least one file of the repository.
//!
//! Second part: the size budget. `CLAUDE.md` is read at every start by every
//! agent: it stays under 200 lines and within 10 % of the size it landed with,
//! and the whole of `.claude/rules/` within 10 % of its landed size. A rule
//! added is a rule removed or turned into a script, test or setting; raise a
//! constant below only together with that review.
//!
//! The checks take a root and return their errors; the witnesses (fake trees in
//! the temp directory) prove that they ring.

use std::fs;
use std::path::{Path, PathBuf};

/// Size of `CLAUDE.md` and of `.claude/rules/` (bytes) when #516 landed.
const LANDED_CLAUDE_BYTES: usize = 13_200;
const LANDED_RULES_BYTES: usize = 70_000;
const MAX_CLAUDE_LINES: usize = 200;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir`, as `/`-separated paths relative to `root`;
/// `target` and `.git` are skipped.
fn all_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && name != ".git" {
                all_files(root, &path, out);
            }
        } else {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy();
            out.push(rel.replace('\\', "/"));
        }
    }
}

/// `*` matches any run of characters inside one path segment.
fn segment_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(&p, &t)
}

/// Glob over `/`-separated paths: `**` matches any number of segments
/// (including none), `*` any run inside a segment.
fn glob_match(pattern: &str, path: &str) -> bool {
    fn go(p: &[&str], s: &[&str]) -> bool {
        match p.split_first() {
            None => s.is_empty(),
            Some((&"**", rest)) => (0..=s.len()).any(|i| go(rest, &s[i..])),
            Some((seg, rest)) => {
                s.first().is_some_and(|x| segment_match(seg, x)) && go(rest, &s[1..])
            }
        }
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    go(&p, &s)
}

/// The globs of a rule file's frontmatter, or why it is invalid.
fn parse_paths(text: &str) -> Result<Vec<String>, String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Err("no frontmatter (the file would load at every start)".into());
    }
    let front: Vec<&str> = lines
        .by_ref()
        .take_while(|l| l.trim_end() != "---")
        .collect();
    if front.first().map(|l| l.trim_end()) != Some("paths:") {
        return Err("frontmatter must start with `paths:`".into());
    }
    let mut globs = Vec::new();
    for line in &front[1..] {
        let Some(item) = line.strip_prefix("  - ") else {
            return Err(format!("not a plain list item `  - glob`: {line:?}"));
        };
        let glob = item.trim().trim_matches('"').to_string();
        if glob.is_empty() || glob.contains('{') || glob.contains('}') {
            return Err(format!("empty glob or braces (not portable): {item:?}"));
        }
        globs.push(glob);
    }
    if globs.is_empty() {
        return Err("`paths:` lists no glob".into());
    }
    Ok(globs)
}

/// Errors of the `.claude/rules/*.md` frontmatters and of their globs.
fn path_errors(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    all_files(root, root, &mut files);
    let mut errors = Vec::new();
    let Ok(entries) = fs::read_dir(root.join(".claude/rules")) else {
        return vec![".claude/rules: directory missing".into()];
    };
    let mut rules: Vec<PathBuf> = entries
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    rules.sort();
    if rules.is_empty() {
        errors.push(".claude/rules: no rule file".into());
    }
    for rule in rules {
        let name = rule.file_name().unwrap().to_string_lossy().to_string();
        let text = fs::read_to_string(&rule).expect("rule file");
        match parse_paths(&text) {
            Err(e) => errors.push(format!("{name}: {e}")),
            Ok(globs) => {
                for glob in globs {
                    if !files.iter().any(|f| glob_match(&glob, f)) {
                        errors.push(format!("{name}: glob matches no file: {glob}"));
                    }
                }
            }
        }
    }
    errors
}

/// Errors of the budgets: `CLAUDE.md` lines and bytes, `.claude/rules/` bytes.
fn budget_errors(root: &Path, claude_bytes_max: usize, rules_bytes_max: usize) -> Vec<String> {
    let mut errors = Vec::new();
    let claude = fs::read_to_string(root.join("CLAUDE.md")).expect("CLAUDE.md");
    if claude.lines().count() > MAX_CLAUDE_LINES {
        errors.push(format!(
            "CLAUDE.md has {} lines, the limit is {MAX_CLAUDE_LINES}",
            claude.lines().count()
        ));
    }
    if claude.len() > claude_bytes_max {
        errors.push(format!(
            "CLAUDE.md is {} bytes, the budget is {claude_bytes_max}",
            claude.len()
        ));
    }
    let rules: usize = fs::read_dir(root.join(".claude/rules"))
        .map(|d| {
            d.map(|e| e.expect("entry").path())
                .filter_map(|p| fs::metadata(p).ok())
                .map(|m| m.len() as usize)
                .sum()
        })
        .unwrap_or(0);
    if rules > rules_bytes_max {
        errors.push(format!(
            ".claude/rules/ is {rules} bytes, the budget is {rules_bytes_max}"
        ));
    }
    errors
}

#[test]
fn rule_files_have_valid_paths_that_match_files() {
    let errors = path_errors(&root());
    assert!(errors.is_empty(), "rule files:\n{}", errors.join("\n"));
}

#[test]
fn always_loaded_rules_stay_within_their_budget() {
    let errors = budget_errors(
        &root(),
        LANDED_CLAUDE_BYTES * 11 / 10,
        LANDED_RULES_BYTES * 11 / 10,
    );
    assert!(
        errors.is_empty(),
        "budget (one rule added = one rule removed or turned into a script, test or setting):\n{}",
        errors.join("\n")
    );
}

// ---- Witnesses ----

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "stokesforge-rulepaths-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("tree");
        Tree(dir)
    }

    fn write(&self, path: &str, content: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const GOOD: &str = "---\npaths:\n  - \"crates/a/**\"\n---\n\n## A\n";

#[test]
fn witness_matcher_handles_star_and_double_star() {
    assert!(glob_match("crates/a/**", "crates/a/src/x.rs"));
    assert!(glob_match("crates/a/**", "crates/a/x.rs"));
    assert!(!glob_match("crates/a/**", "crates/b/x.rs"));
    assert!(glob_match(
        "crates/*/src/error.rs",
        "crates/core/src/error.rs"
    ));
    assert!(!glob_match(
        "crates/*/src/error.rs",
        "crates/core/x/src/error.rs"
    ));
    assert!(glob_match("a/src/analysis*.rs", "a/src/analysis_recent.rs"));
    assert!(glob_match("**/x.rs", "x.rs"));
}

#[test]
fn witness_correct_tree_has_no_error() {
    let a = Tree::new("good");
    a.write("crates/a/src/x.rs", "");
    a.write(".claude/rules/a.md", GOOD);
    assert!(path_errors(&a.0).is_empty(), "{:?}", path_errors(&a.0));
}

#[test]
fn witness_missing_frontmatter_is_reported() {
    let a = Tree::new("nofront");
    a.write("crates/a/src/x.rs", "");
    a.write(".claude/rules/a.md", "## A\n");
    assert!(path_errors(&a.0)
        .iter()
        .any(|e| e.contains("no frontmatter")));
}

#[test]
fn witness_glob_matching_nothing_is_reported() {
    let a = Tree::new("nope");
    a.write("crates/a/src/x.rs", "");
    a.write(
        ".claude/rules/a.md",
        "---\npaths:\n  - \"crates/nope/**\"\n---\n",
    );
    assert!(path_errors(&a.0)
        .iter()
        .any(|e| e.contains("matches no file: crates/nope/**")));
}

#[test]
fn witness_braces_and_empty_list_are_reported() {
    let a = Tree::new("braces");
    a.write("crates/a/src/x.rs", "");
    a.write(
        ".claude/rules/a.md",
        "---\npaths:\n  - \"crates/{a,b}/**\"\n---\n",
    );
    a.write(".claude/rules/b.md", "---\npaths:\n---\n");
    let errors = path_errors(&a.0);
    assert!(errors
        .iter()
        .any(|e| e.starts_with("a.md") && e.contains("braces")));
    assert!(errors
        .iter()
        .any(|e| e.starts_with("b.md") && e.contains("no glob")));
}

#[test]
fn witness_budget_refuses_a_201_line_claude_md() {
    let a = Tree::new("long");
    a.write(".claude/rules/a.md", GOOD);
    a.write("CLAUDE.md", &"x\n".repeat(201));
    assert!(budget_errors(&a.0, 100_000, 100_000)
        .iter()
        .any(|e| e.contains("201 lines")));
    a.write("CLAUDE.md", &"x\n".repeat(200));
    assert!(budget_errors(&a.0, 100_000, 100_000).is_empty());
    assert!(budget_errors(&a.0, 100, 100_000)
        .iter()
        .any(|e| e.contains("budget")));
    assert!(budget_errors(&a.0, 100_000, 10)
        .iter()
        .any(|e| e.contains(".claude/rules/")));
}
