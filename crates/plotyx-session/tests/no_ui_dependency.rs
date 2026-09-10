//! Garde-fou : `plotyx-session` ne doit jamais gagner de dépendance UI ou
//! Python. L'acteur détient le seul `&mut Project` du processus et doit
//! pouvoir servir aussi bien l'UI qu'un futur thread de script Python sans
//! connaître ni l'un ni l'autre — voir la doc de crate. Plutôt que de
//! deviner ce qu'est « une dépendance UI » ou « une dépendance Python »,
//! ce test liste les seules dépendances autorisées : toute addition dans
//! `Cargo.toml` doit être ajoutée ici explicitement, ce qui force une
//! décision consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core",
    "plotyx-commands",
    "plotyx-persistence",
    "thiserror",
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
             plotyx-session doit rester sans dépendance UI ni Python (voir la doc de crate)"
        );
    }
}
