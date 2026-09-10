//! Même garde-fou que `plotyx-core` et `plotyx-session`, appliqué au fil de
//! script : ce crate a le droit de connaître Python — c'est sa raison d'être —
//! mais **jamais** l'interface. Plutôt que de deviner ce qu'est « une
//! dépendance UI », ce test liste les seules dépendances autorisées : toute
//! addition dans `Cargo.toml` doit être ajoutée ici explicitement, ce qui force
//! une décision consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core",     // le modèle
    "plotyx-commands", // le bus de commandes : tout passe par lui
    "plotyx-session",  // l'acteur, seul détenteur du `&mut Project`
    "plotyx-python",   // l'interpréteur embarqué et le pont de tableaux
    "pyo3",            // liaison CPython, version verrouillée avec plotyx-python
    "numpy",           // liaison de la C-API numpy, liée à celle de pyo3
    "thiserror",       // erreurs typées, sans surface UI
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
             plotyx-script doit rester sans dépendance UI (voir la doc de crate)"
        );
    }
}
