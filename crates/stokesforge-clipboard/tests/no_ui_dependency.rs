//! `stokesforge-clipboard` est une feuille : aucune dépendance au reste du
//! workspace, ni à egui. Il ne reçoit que des octets étiquetés par un type
//! MIME et un `HasDisplayHandle` (`raw-window-handle`, la norme des
//! fenêtres) ; l'interface le pilote, jamais l'inverse.

#[test]
fn le_crate_ne_depend_d_aucun_crate_du_workspace_ni_d_egui() {
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
