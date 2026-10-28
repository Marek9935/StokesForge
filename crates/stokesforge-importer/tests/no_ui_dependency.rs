//! Guard: `stokesforge-importer` bridges `stokesforge-import` (files) and
//! `stokesforge-core` (model), used by the application, the CLI and the scripts. It must
//! know no interface, no commands and no session: each of its
//! consumers decides **how** to write into the project.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-import", // reading the files
    "stokesforge-core",   // the model columns
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
