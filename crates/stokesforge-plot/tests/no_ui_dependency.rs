//! Same guard as `stokesforge-core` and `stokesforge-commands`: `stokesforge-plot` must
//! never gain a drawing dependency. Rather than guess what "a rendering
//! dependency" is, this test lists the only allowed dependencies —
//! any addition to `Cargo.toml` must be added here explicitly, which
//! forces a conscious decision rather than a sneaking import.
//!
//! This is the exit criterion of phase R2 made executable: `cargo tree -p
//! stokesforge-plot` must show neither `plotters`, nor `egui`, nor `typst`
//! (`docs/archive/newarchitecture/roadmap-rendu.md` §2.1).
//!
//! Since phase R3, the listed dependency is `stokesforge-id`, not
//! `stokesforge-core`: `stokesforge-core` had to start depending on `stokesforge-plot`
//! (to carry a `PlotSpec` on `NodeKind::Graph`), which would have made a direct
//! Cargo cycle with the reverse direction. `NodeId` was therefore moved down into
//! `stokesforge-id`, a minimal crate with no dependency below both — see
//! `crates/stokesforge-id/src/lib.rs` and `src/lib.rs` of this crate. The R2 exit
//! criterion still holds, and more tightly than before: this crate no longer
//! even sees `Tree` or `Column`, only `NodeId`.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-id", // node identity only: one-way dependency
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Shared reading (`stokesforge-arch-guard`): normal, build,
    // conditional (`target`) and renamed (`package`) dependencies — issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
