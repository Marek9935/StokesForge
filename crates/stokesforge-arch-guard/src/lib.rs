//! Lecture **commune** des manifestes pour les garde-fous de dépendances
//! (`tests/no_ui_dependency.rs` des crates du socle, d'`stokesforge-import`…).
//! Chaque crate garde sa propre liste blanche — c'est une décision par crate —
//! mais la façon de lire un `Cargo.toml` est écrite ici une seule fois, pour ne
//! pas reproduire le même angle mort six fois (issue #24).
//!
//! # Politique
//!
//! - **Comptent** : `[dependencies]`, `[build-dependencies]` et leurs variantes
//!   conditionnelles `[target.'cfg(…)'.dependencies]` /
//!   `[target.….build-dependencies]` — une dépendance conditionnelle est une
//!   dépendance ; les dépendances optionnelles (features) comptent aussi.
//! - **Ne comptent pas** : `[dev-dependencies]` (outillage de test, jamais
//!   livré) — y compris conditionnelles.
//! - **Nom réel** : une dépendance renommée (`alias = { package = "réel" }`)
//!   est jugée sur son paquet **réel**, pas sur sa clé locale ; l'alias n'ouvre
//!   aucun droit.

use toml::Value;

/// Les sections de dépendances qui comptent, avec leur chemin lisible.
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

/// Nom du paquet réel derrière une entrée de dépendance.
fn real_package_name<'a>(key: &'a str, spec: &'a Value) -> &'a str {
    spec.get("package").and_then(Value::as_str).unwrap_or(key)
}

/// Les dépendances de `manifest` (texte TOML) qui ne figurent pas dans
/// `allowed`, sous la forme `section: clé` (ou `section: clé (paquet réel)`
/// pour un alias). Liste vide : le manifeste respecte la liste blanche.
pub fn undeclared_dependencies(manifest: &str, allowed: &[&str]) -> Vec<String> {
    let parsed: Value = manifest.parse().expect("Cargo.toml valide");
    let mut found = Vec::new();
    for (section, table) in dependency_tables(&parsed) {
        for (key, spec) in table {
            let package = real_package_name(key, spec);
            if !allowed.contains(&package) {
                found.push(if package == key {
                    format!("{section}: {key}")
                } else {
                    format!("{section}: {key} (paquet réel : {package})")
                });
            }
        }
    }
    found
}

/// Vérifie le `Cargo.toml` du crate situé dans `manifest_dir` (passer
/// `env!("CARGO_MANIFEST_DIR")`) : panique, avec la liste des dépendances
/// hors liste blanche, si l'une d'elles n'est pas dans `allowed`.
pub fn assert_only_allowed_dependencies(manifest_dir: &str, allowed: &[&str]) {
    let path = std::path::Path::new(manifest_dir).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).expect("Cargo.toml lisible");
    let offending = undeclared_dependencies(&manifest, allowed);
    assert!(
        offending.is_empty(),
        "dépendance(s) non listée(s) dans ALLOWED_DEPENDENCIES de {} : {offending:?}. \
         Une addition se décide consciemment : l'ajouter à la liste blanche du crate, \
         jamais élargir la liste pour faire passer le test.",
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

    /// Témoin : un manifeste conforme ne remonte rien.
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

    /// Témoin négatif 1 : la section normale.
    #[test]
    fn a_normal_forbidden_dependency_is_caught() {
        let manifest = "[dependencies]\neframe = \"0.36\"\n";
        assert_eq!(offending(manifest), vec!["dependencies: eframe"]);
    }

    /// Témoin négatif 2 : la section `target` (l'angle mort de l'audit).
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

    /// Témoin négatif 3 : un alias sur une clé autorisée ne masque pas le
    /// paquet réel.
    #[test]
    fn a_renamed_package_is_judged_by_its_real_name() {
        let manifest = r#"
            [dependencies]
            thiserror = { package = "eframe", version = "0.36" }
        "#;
        let found = offending(manifest);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("paquet réel : eframe"));
    }

    /// Un alias dont le paquet réel est autorisé est accepté.
    #[test]
    fn an_alias_of_an_allowed_package_passes() {
        let manifest = "[dependencies]\nerrors = { package = \"thiserror\", version = \"2\" }\n";
        assert!(offending(manifest).is_empty());
    }

    /// Témoin négatif 4 : une dépendance optionnelle (feature) compte.
    #[test]
    fn an_optional_dependency_is_caught() {
        let manifest = "[dependencies]\neframe = { version = \"0.36\", optional = true }\n";
        assert_eq!(offending(manifest), vec!["dependencies: eframe"]);
    }

    /// Les dépendances de build comptent ; celles de test, non.
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
