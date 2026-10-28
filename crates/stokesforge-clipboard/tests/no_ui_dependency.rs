//! `stokesforge-clipboard` is a leaf: no dependency on the rest of the
//! workspace, nor on egui. It only receives bytes labeled with a MIME type
//! and a `HasDisplayHandle` (`raw-window-handle`, the window
//! standard); the interface drives it, never the reverse.

#[test]
fn the_crate_depends_on_no_workspace_crate_nor_egui() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        &[
            "raw-window-handle",
            "thiserror",
            "smithay-clipboard",
            "x11rb",
            "clipboard-win",
            "objc2-app-kit",
            "objc2-foundation",
            "stokesforge-arch-guard",
        ],
    );
}
