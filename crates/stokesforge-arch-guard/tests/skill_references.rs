//! The project skills (`.claude/skills/stokesforge-*` and `run-stokesforge`)
//! do not copy the rules: they point to them with "`file.md` § Title"
//! into `docs/` or `.claude/conventions.md`. This guard checks that every
//! reference, in every `SKILL.md` and every `references/*.md`, names a file and
//! a heading that still exist. It also guards the `](references/x.md)` links of
//! each `SKILL.md`: they must point to an existing file, and no sheet
//! of `references/` may stay unlinked (orphan).
//!
//! A third check guards the frontmatter: an unquoted `description:` holding
//! ` #` (a YAML comment) or `: ` (a mapping indicator) would be cut by any
//! YAML parser, so it must be quoted; a missing or empty one is refused too.
//!
//! Why: the headings of `docs/` change when the documentation is
//! translated into English; a renamed or removed section must break the test
//! instead of leaving a mute sheet. Third-party skills (apex, skill-manager,
//! audit-test…) are ignored. The sheets no longer have a fixed count: only a
//! sanity floor (at least 3 skills, more than 30 references) signals a format
//! that would have changed.
//!
//! Form read: a path in backticks ending in `.md`, then
//! ` § `, then the title, up to ` ; ` or the end of the line. The title is
//! accepted if it is contained in a heading of the file, or if it starts with a
//! heading of the file (the reference may add ", point `x`" or "(paragraph… )").
//! A `§ ` with no path just before it is refused: it could not be verified.
//!
//! The checks take a root and return their errors; the witnesses
//! (fake trees in the temp directory) prove that they ring.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn headings(file: &Path) -> Vec<String> {
    fs::read_to_string(file)
        .unwrap_or_else(|e| panic!("reading {}: {e}", file.display()))
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .collect()
}

/// References `(path, title)` of a line; `Err` for a `§ ` with no path.
fn references(line: &str) -> Result<Vec<(String, String)>, String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find(" § ") {
        let before = &rest[..i];
        let path = before
            .strip_suffix("`")
            .and_then(|a| a.rfind('`').map(|j| &a[j + 1..]))
            .filter(|c| c.ends_with(".md"))
            .ok_or_else(|| format!("\"§\" without a .md path before it: {line}"))?;
        let after = &rest[i + " § ".len()..];
        let end = after.find(" ; ").unwrap_or(after.len());
        let title = after[..end].trim().trim_end_matches('.').trim();
        found.push((path.to_string(), title.to_string()));
        rest = &after[end..];
    }
    Ok(found)
}

fn valid_title(title: &str, within: &[String]) -> bool {
    within.iter().any(|t| {
        t.contains(title)
            || (title.starts_with(t.as_str())
                && title[t.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric()))
    })
}

/// Project skills: `stokesforge-*` and `run-stokesforge`, sorted.
fn project_skills(root: &Path) -> Vec<PathBuf> {
    let mut skills: Vec<PathBuf> = fs::read_dir(root.join(".claude/skills"))
        .expect(".claude/skills directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("stokesforge-") || n == "run-stokesforge")
        })
        .collect();
    skills.sort();
    skills
}

/// The `.md` files to scan in a skill: `SKILL.md` and `references/*.md`.
fn markdown_files(skill: &Path) -> Vec<PathBuf> {
    let mut files = vec![skill.join("SKILL.md")];
    if let Ok(dir) = fs::read_dir(skill.join("references")) {
        let mut refs: Vec<PathBuf> = dir
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        refs.sort();
        files.extend(refs);
    }
    files
}

/// Number of references read and errors of all project skills.
fn read_references(root: &Path) -> (usize, Vec<String>) {
    let mut total = 0;
    let mut errors = Vec::new();
    for skill in project_skills(root) {
        for file in markdown_files(&skill) {
            let text = match fs::read_to_string(&file) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("{}: cannot read: {e}", file.display()));
                    continue;
                }
            };
            for line in text.lines() {
                match references(line) {
                    Err(e) => errors.push(format!("{}: {e}", file.display())),
                    Ok(list) => {
                        for (path, title) in list {
                            total += 1;
                            let target = root.join(&path);
                            if !target.is_file() {
                                errors.push(format!("{}: file missing {path}", file.display()));
                            } else if !valid_title(&title, &headings(&target)) {
                                errors.push(format!(
                                    "{}: no heading \"{title}\" in {path}",
                                    file.display()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    (total, errors)
}

/// Errors of the "`path.md` § Title" references of all project skills.
fn reference_errors(root: &Path) -> Vec<String> {
    read_references(root).1
}

/// Errors of the `](references/x)` links: dead link or orphan sheet.
fn link_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for skill in project_skills(root) {
        let Ok(text) = fs::read_to_string(skill.join("SKILL.md")) else {
            errors.push(format!("{}: unreadable SKILL.md", skill.display()));
            continue;
        };
        for piece in text.split("](references/").skip(1) {
            let name = piece.split(')').next().unwrap_or("");
            if !skill.join("references").join(name).is_file() {
                errors.push(format!(
                    "{}: sheet listed but missing: {name}",
                    skill.display()
                ));
            }
        }
        let Ok(dir) = fs::read_dir(skill.join("references")) else {
            continue;
        };
        for entry in dir {
            let entry = entry.expect("entry");
            if !entry.path().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !text.contains(&format!("](references/{name})")) {
                errors.push(format!(
                    "{}: sheet not listed in SKILL.md: {name}",
                    skill.display()
                ));
            }
        }
    }
    errors
}

/// Errors of the frontmatter `description:`: missing, empty, or unquoted
/// with ` #` or `: ` (a YAML parser would cut it).
fn description_errors(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for skill in project_skills(root) {
        let file = skill.join("SKILL.md");
        let text =
            fs::read_to_string(&file).unwrap_or_else(|e| panic!("reading {}: {e}", file.display()));
        let name = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        let value = text
            .lines()
            .skip_while(|l| l.trim_end() != "---")
            .skip(1)
            .take_while(|l| l.trim_end() != "---")
            .find_map(|l| l.strip_prefix("description:"))
            .map(str::trim);
        match value {
            None | Some("") => errors.push(format!("{name}: missing or empty description")),
            Some(v) if v.starts_with('"') || v.starts_with('\'') => {}
            Some(v) if v.contains(" #") || v.contains(": ") || v.ends_with(':') => {
                errors.push(format!(
                    "{name}: unquoted description contains \" #\" or \": \", \
                     it would be cut by YAML, quote it"
                ));
            }
            Some(_) => {}
        }
    }
    errors
}

#[test]
fn skill_descriptions_survive_yaml() {
    let errors = description_errors(&root());
    assert!(
        errors.is_empty(),
        "descriptions cut by YAML:\n{}",
        errors.join("\n")
    );
}

#[test]
fn skill_references_point_to_existing_sections() {
    let root = root();
    let skills = project_skills(&root).len();
    assert!(skills >= 3, "project skills not found: {skills}");
    let (total, errors) = read_references(&root);
    assert!(
        total > 30,
        "too few references read ({total}): format changed?"
    );
    assert!(
        errors.is_empty(),
        "broken references:\n{}",
        errors.join("\n")
    );
}

#[test]
fn skills_list_existing_sheets_with_no_orphan() {
    let errors = link_errors(&root());
    assert!(
        errors.is_empty(),
        "broken sheet links:\n{}",
        errors.join("\n")
    );
}

// ---- Witnesses: fake trees ----

/// Fake tree in the temp directory, removed at the end.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "stokesforge-references-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("tree creation");
        Tree(dir)
    }

    fn write(&self, path: &str, content: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().expect("parent")).expect("dossiers");
        fs::write(p, content).expect("write");
    }

    /// Correct tree: one doc, three skills, one of them linked to a sheet.
    fn correct(name: &str) -> Self {
        let a = Self::new(name);
        a.write("docs/a.md", "# Doc\n\n## Real section\n");
        a.write(
            ".claude/skills/stokesforge-one/SKILL.md",
            "See `docs/a.md` § Real section\n[f](references/f.md)\n",
        );
        a.write(
            ".claude/skills/stokesforge-one/references/f.md",
            "See `docs/a.md` § Real section ; `docs/a.md` § Doc\n",
        );
        a.write(".claude/skills/stokesforge-two/SKILL.md", "Nothing.\n");
        a.write(".claude/skills/run-stokesforge/SKILL.md", "Nothing.\n");
        a
    }

    fn skill(&self) -> &'static str {
        ".claude/skills/stokesforge-one"
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn contains_error(errors: &[String], fragment: &str) -> bool {
    errors.iter().any(|e| e.contains(fragment))
}

#[test]
fn witness_correct_tree_has_no_error() {
    let a = Tree::correct("correct");
    assert_eq!(project_skills(&a.0).len(), 3);
    let (total, errors) = read_references(&a.0);
    assert_eq!(total, 3);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(link_errors(&a.0).is_empty());
}

fn with_description(a: &Tree, line: &str) {
    a.write(
        &format!("{}/SKILL.md", a.skill()),
        &format!("---\nname: x\n{line}\n---\nBody.\n"),
    );
}

#[test]
fn witness_unquoted_description_with_hash_is_reported() {
    let a = Tree::correct("deschash");
    with_description(&a, "description: Use for « traite #N » now");
    assert!(contains_error(&description_errors(&a.0), "cut by YAML"));
}

#[test]
fn witness_unquoted_description_with_colon_is_reported() {
    let a = Tree::correct("desccolon");
    with_description(&a, "description: Use when: something");
    assert!(contains_error(&description_errors(&a.0), "cut by YAML"));
}

#[test]
fn witness_quoted_description_with_hash_is_accepted() {
    let a = Tree::correct("descquoted");
    with_description(&a, "description: \"Use for « traite #N »: now\"");
    assert!(!contains_error(
        &description_errors(&a.0),
        "stokesforge-one"
    ));
}

#[test]
fn witness_missing_description_is_reported() {
    let a = Tree::correct("descmissing");
    with_description(&a, "other: x");
    assert!(contains_error(
        &description_errors(&a.0),
        "missing or empty"
    ));
}

#[test]
fn witness_missing_heading_is_reported() {
    let a = Tree::correct("heading");
    a.write(
        &format!("{}/references/f.md", a.skill()),
        "See `docs/a.md` § Vanished section\n",
    );
    assert!(contains_error(
        &reference_errors(&a.0),
        "no heading \"Vanished section\""
    ));
}

#[test]
fn witness_missing_file_is_reported() {
    let a = Tree::correct("file");
    a.write(
        &format!("{}/SKILL.md", a.skill()),
        "See `docs/unknown.md` § Doc\n[f](references/f.md)\n",
    );
    assert!(contains_error(
        &reference_errors(&a.0),
        "file missing docs/unknown.md"
    ));
}

#[test]
fn witness_paragraph_without_path_is_reported() {
    let a = Tree::correct("nopath");
    a.write(
        &format!("{}/references/f.md", a.skill()),
        "See the section § Doc\n",
    );
    assert!(contains_error(
        &reference_errors(&a.0),
        "without a .md path"
    ));
}

#[test]
fn witness_link_to_missing_sheet_is_reported() {
    let a = Tree::correct("deadlink");
    a.write(
        &format!("{}/SKILL.md", a.skill()),
        "[f](references/f.md) [g](references/g.md)\n",
    );
    let errors = link_errors(&a.0);
    assert!(
        contains_error(&errors, "sheet listed but missing: g.md"),
        "{errors:?}"
    );
}

#[test]
fn witness_orphan_sheet_is_reported() {
    let a = Tree::correct("orphan");
    a.write(&format!("{}/references/orphan.md", a.skill()), "Nothing.\n");
    let errors = link_errors(&a.0);
    assert!(
        contains_error(&errors, "sheet not listed in SKILL.md: orphan.md"),
        "{errors:?}"
    );
}

#[test]
fn witness_third_party_skill_is_ignored() {
    let a = Tree::correct("thirdparty");
    a.write(
        ".claude/skills/apex/SKILL.md",
        "See `docs/unknown.md` § Nothing and § without path\n[x](references/absent.md)\n",
    );
    a.write(".claude/skills/apex/references/orphan.md", "Nothing.\n");
    assert!(reference_errors(&a.0).is_empty());
    assert!(link_errors(&a.0).is_empty());
    a.write(
        ".claude/skills/apex/SKILL.md",
        "---\ndescription: bad #1\n---\n",
    );
    assert!(!contains_error(&description_errors(&a.0), "apex"));
    assert_eq!(project_skills(&a.0).len(), 3);
}
