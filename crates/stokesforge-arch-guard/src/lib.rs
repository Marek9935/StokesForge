//! **Shared** reading of manifests for the dependency guards
//! (`tests/no_ui_dependency.rs` of the foundation crates, of `stokesforge-import`…).
//! Each crate keeps its own allowlist — that is a per-crate decision —
//! but the way to read a `Cargo.toml` is written here once, so as not to
//! repeat the same blind spot six times (issue #24).
//!
//! # Policy
//!
//! - **Count**: `[dependencies]`, `[build-dependencies]` and their conditional
//!   variants `[target.'cfg(…)'.dependencies]` /
//!   `[target.….build-dependencies]` — a conditional dependency is a
//!   dependency; optional dependencies (features) count too.
//! - **Do not count**: `[dev-dependencies]` (test tooling, never
//!   shipped) — conditional ones included.
//! - **Real name**: a renamed dependency (`alias = { package = "real" }`)
//!   is judged on its **real** package, not on its local key; the alias grants
//!   no right.

use toml::Value;

/// The dependency sections that count, with their readable path.
fn dependency_tables(manifest: &Value) -> Vec<(String, &toml::map::Map<String, Value>)> {
    let mut tables = Vec::new();
    for kind in ["dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(kind).and_then(Value::as_table) {
            tables.push((kind.to_string(), table));
        }
    }
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for (target, content) in targets {
            for kind in ["dependencies", "build-dependencies"] {
                if let Some(table) = content.get(kind).and_then(Value::as_table) {
                    tables.push((format!("target.{target}.{kind}"), table));
                }
            }
        }
    }
    tables
}

/// Name of the real package behind a dependency entry.
fn real_package_name<'a>(key: &'a str, spec: &'a Value) -> &'a str {
    spec.get("package").and_then(Value::as_str).unwrap_or(key)
}

/// The dependencies of `manifest` (TOML text) that are not in
/// `allowed`, as `section: key` (or `section: key (real package)`
/// for an alias). Empty list: the manifest respects the allowlist.
pub fn undeclared_dependencies(manifest: &str, allowed: &[&str]) -> Vec<String> {
    let parsed: Value = manifest.parse().expect("valid Cargo.toml");
    let mut found = Vec::new();
    for (section, table) in dependency_tables(&parsed) {
        for (key, spec) in table {
            let package = real_package_name(key, spec);
            if !allowed.contains(&package) {
                found.push(if package == key {
                    format!("{section}: {key}")
                } else {
                    format!("{section}: {key} (real package: {package})")
                });
            }
        }
    }
    found
}

/// Checks the `Cargo.toml` of the crate located in `manifest_dir` (pass
/// `env!("CARGO_MANIFEST_DIR")`): panics, with the list of dependencies
/// outside the allowlist, if any of them is not in `allowed`.
pub fn assert_only_allowed_dependencies(manifest_dir: &str, allowed: &[&str]) {
    let path = std::path::Path::new(manifest_dir).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).expect("readable Cargo.toml");
    let offending = undeclared_dependencies(&manifest, allowed);
    assert!(
        offending.is_empty(),
        "dependencies not listed in ALLOWED_DEPENDENCIES of {}: {offending:?}. \
         An addition is a conscious decision: add it to the crate's allowlist, \
         never widen the list just to make the test pass.",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::undeclared_dependencies;

    const ALLOWED: &[&str] = &["thiserror", "stokesforge-id"];

    fn offending(manifest: &str) -> Vec<String> {
        undeclared_dependencies(manifest, ALLOWED)
    }

    /// Witness: a conforming manifest reports nothing.
    #[test]
    fn a_conforming_manifest_passes() {
        let manifest = r#"
            [dependencies]
            thiserror = "2"
            stokesforge-id = { path = "../stokesforge-id" }
            [dev-dependencies]
            proptest = "1"
        "#;
        assert!(offending(manifest).is_empty());
    }

    /// Negative witness 1: the normal section.
    #[test]
    fn a_normal_forbidden_dependency_is_caught() {
        let manifest = "[dependencies]\neframe = \"0.36\"\n";
        assert_eq!(offending(manifest), vec!["dependencies: eframe"]);
    }

    /// Negative witness 2: the `target` section (the audit's blind spot).
    #[test]
    fn a_target_specific_dependency_is_caught() {
        let manifest = r#"
            [target.'cfg(target_os = "linux")'.dependencies]
            eframe = "0.36"
        "#;
        let found = offending(manifest);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("eframe"));
    }

    /// Negative witness 3: an alias on an allowed key does not hide the
    /// real package.
    #[test]
    fn a_renamed_package_is_judged_by_its_real_name() {
        let manifest = r#"
            [dependencies]
            thiserror = { package = "eframe", version = "0.36" }
        "#;
        let found = offending(manifest);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("real package: eframe"));
    }

    /// An alias whose real package is allowed is accepted.
    #[test]
    fn an_alias_of_an_allowed_package_passes() {
        let manifest = "[dependencies]\nerrors = { package = \"thiserror\", version = \"2\" }\n";
        assert!(offending(manifest).is_empty());
    }

    /// Negative witness 4: an optional dependency (feature) counts.
    #[test]
    fn an_optional_dependency_is_caught() {
        let manifest = "[dependencies]\neframe = { version = \"0.36\", optional = true }\n";
        assert_eq!(offending(manifest), vec!["dependencies: eframe"]);
    }

    /// Build dependencies count; test dependencies do not.
    #[test]
    fn build_dependencies_count_and_dev_dependencies_do_not() {
        let manifest = r#"
            [build-dependencies]
            cc = "1"
            [dev-dependencies]
            eframe = "0.36"
            [target.'cfg(unix)'.dev-dependencies]
            egui = "0.36"
        "#;
        assert_eq!(offending(manifest), vec!["build-dependencies: cc"]);
    }
}
