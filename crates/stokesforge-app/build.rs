//! Build script of `stokesforge-app` (#388): the icon of `stokesforge-app.exe`.
//!
//! The icon is a Windows resource, compiled in only when an **official build**
//! asks for it with `STOKESFORGE_EXE_ICON=<path of a .ico>` (set by
//! `scripts/app-dist-windows.sh` after `scripts/logo-dist.sh` generated it:
//! the logo is read nowhere else).
//!
//! - variable unset or empty: nothing happens, the build is the usual one;
//! - variable set and no such file: the build fails, rather than shipping an
//!   executable the packaging believes has its icon;
//! - not a Windows target: the variable is checked and otherwise ignored.

use std::path::PathBuf;
use std::process::exit;

fn main() {
    println!("cargo:rerun-if-env-changed=STOKESFORGE_EXE_ICON");
    println!("cargo:rerun-if-changed=build.rs");
    let Some(icon) = std::env::var_os("STOKESFORGE_EXE_ICON").filter(|value| !value.is_empty())
    else {
        return;
    };
    let icon = PathBuf::from(icon);
    println!("cargo:rerun-if-changed={}", icon.display());
    if !icon.is_file() {
        eprintln!(
            "STOKESFORGE_EXE_ICON is set but {} is not a file",
            icon.display()
        );
        exit(1);
    }
    embed(&icon);
}

#[cfg(windows)]
fn embed(icon: &std::path::Path) {
    let Some(icon) = icon.to_str() else {
        eprintln!("STOKESFORGE_EXE_ICON is not valid Unicode");
        exit(1);
    };
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon);
    // Task Manager shows FileDescription: say StokesForge, not the crate name.
    resource.set("FileDescription", "StokesForge");
    resource.set("ProductName", "StokesForge");
    if let Err(err) = resource.compile() {
        eprintln!("cannot compile the Windows resources: {err}");
        exit(1);
    }
}

#[cfg(not(windows))]
fn embed(_icon: &std::path::Path) {
    println!("cargo:warning=STOKESFORGE_EXE_ICON is only used by a Windows build");
}
