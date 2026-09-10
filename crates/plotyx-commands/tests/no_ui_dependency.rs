//! Même garde-fou que `plotyx-core` : `plotyx-commands` ne doit jamais gagner
//! de dépendance UI. Plutôt que de deviner ce qu'est "une dépendance UI", ce
//! test liste les seules dépendances autorisées — toute addition dans
//! `Cargo.toml` doit être ajoutée ici explicitement, ce qui force une décision
//! consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core", // le modèle : dépendance à sens unique, sans surface UI
    "thiserror",   // erreurs typées, sans surface UI
];

#[test]
fn only_allowed_dependencies_are_declared() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml lisible");
    let parsed: toml::Value = manifest.parse().expect("Cargo.toml valide");

    let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_table()) else {
        return; // pas de section [dependencies] du tout : rien à vérifier
    };

    for name in deps.keys() {
        assert!(
            ALLOWED_DEPENDENCIES.contains(&name.as_str()),
            "dépendance `{name}` non listée dans ALLOWED_DEPENDENCIES : \
             plotyx-commands doit rester sans dépendance UI (voir roadmap phase 2)"
        );
    }
}
