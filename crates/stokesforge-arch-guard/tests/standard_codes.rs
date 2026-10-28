//! The `S-XN` codes of the interface standard (`docs/interface-standard.md`,
//! "Rule index") are cited everywhere: comments, docs, skills,
//! test messages. The `S-` prefix was added by script (#301, F3/F4);
//! nothing checked the names. This guard reads the index, then refuses any
//! `S-XN` cited in the repository that is not in it (typo, removed rule,
//! code of another family wrongly prefixed).
//!
//! It does not detect an *existing* but badly chosen code, nor a citation
//! left without a prefix: those two cases stay for review.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A code `S-` + an uppercase letter + digits, at the start of `s`.
fn leading_code(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("S-")?;
    if !rest.as_bytes().first()?.is_ascii_uppercase() {
        return None;
    }
    let digits = rest.as_bytes()[1..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    (digits > 0).then(|| &s[..3 + digits])
}

/// The codes cited in `text`, excluding longer words (`ABS-C1`).
fn cited_codes(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    text.match_indices("S-")
        .filter(|(i, _)| *i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        .filter_map(|(i, _)| leading_code(&text[i..]))
        .collect()
}

/// The codes of the index: table rows `| S-XN | … |`.
fn index(standard: &str) -> BTreeSet<String> {
    standard
        .lines()
        .filter_map(|l| l.strip_prefix("| "))
        .filter_map(leading_code)
        .map(str::to_string)
        .collect()
}

fn unknown<'a>(text: &'a str, known: &BTreeSet<String>) -> Vec<&'a str> {
    cited_codes(text)
        .into_iter()
        .filter(|c| !known.contains(*c))
        .collect()
}

fn files(folder: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("folder read") {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if !matches!(
                name.as_str(),
                "target" | ".git" | "archive" | "worktrees" | "vendor"
            ) {
                files(&path, output);
            }
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("md" | "rs" | "ftl" | "html" | "toml" | "py" | "sh" | "yml")
        ) {
            output.push(path);
        }
    }
}

#[test]
fn cited_s_codes_exist_in_the_standard_index() {
    let root = root();
    let standard = fs::read_to_string(root.join("docs/interface-standard.md")).expect("standard");
    let known = index(&standard);
    assert!(known.len() >= 89, "index badly read: {} codes", known.len());
    assert!(known.contains("S-N1") && known.contains("S-R16"));

    let mut list = vec![root.join("CLAUDE.md")];
    for folder in ["docs", ".claude", "crates"] {
        files(&root.join(folder), &mut list);
    }
    let (mut citations, mut errors) = (0, Vec::new());
    for file in &list {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        citations += cited_codes(&text).len();
        for (n, line) in text.lines().enumerate() {
            for code in unknown(line, &known) {
                errors.push(format!("{}:{}: {code}", file.display(), n + 1));
            }
        }
    }
    assert!(citations > 300, "too few citations read: {citations}");
    assert!(
        errors.is_empty(),
        "codes missing from the standard index:\n{}",
        errors.join("\n")
    );
}

#[test]
fn the_guard_sees_an_unknown_code_and_ignores_longer_words() {
    let known: BTreeSet<String> = ["S-N1".to_string()].into();
    assert_eq!(unknown("rule S-N1 and S-N2.", &known), ["S-N2"]);
    let fake = format!("S-{}99", 'Z');
    assert_eq!(unknown(&format!("{fake}, (S-N1)"), &known), [fake.as_str()]);
    assert!(unknown("ABS-N7 and S-n3 and S-12", &known).is_empty());
}
