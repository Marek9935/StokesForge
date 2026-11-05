//! The application's identity in the window system (#388): the icon and the
//! application id, and the logo of the About window.
//!
//! The logo exists only in **official builds** (`assets/logo/` is not under the
//! GPL, see its `LICENSE`). It is never compiled in: the packaging scripts
//! install `share/icons/hicolor/256x256/apps/stokesforge.png` next to the
//! executable's `bin` folder (`<exe dir>/share/...`) and this module reads that
//! file at run time. A build without the file runs the same, with the system's
//! generic icon and no logo in About. An unreadable file does the same, with one
//! line on stderr (invariant 10: degrade, never abort, raw English, no `tr!`).
//!
//! Every window of the application is built through [`viewport`] so that all
//! of them carry the same `app_id`: the desktop entry (`stokesforge.desktop`)
//! is matched to the window by it. The guard `no_viewport_without_identity`
//! (`tests/guards.rs`) refuses a bare `ViewportBuilder::default()` elsewhere.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

/// The application id: the Wayland `app_id` and X11 `WM_CLASS` of every
/// window. It is the name of the desktop entry written by `install-linux.sh`
/// (`stokesforge.desktop`) and of the icon (`stokesforge.png`).
pub const APP_ID: &str = "stokesforge";

/// Where the logo is looked for, relative to the executable's folder.
const LOGO_RELATIVE_PATH: &str = "share/icons/hicolor/256x256/apps/stokesforge.png";

/// Side, in points, of the logo at the top of the About window.
pub const ABOUT_LOGO_SIZE: f32 = 96.0;

/// The path of the logo for an executable in `exe_dir`.
#[must_use]
pub fn logo_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join(LOGO_RELATIVE_PATH)
}

/// The logo next to the running executable, if the build has one.
#[must_use]
pub fn load() -> Option<egui::IconData> {
    let exe = std::env::current_exe().ok()?;
    load_from(exe.parent()?)
}

/// The logo of an executable in `exe_dir`: `None` when there is no file (a
/// build without logo) or when it cannot be decoded (one line on stderr).
#[must_use]
pub fn load_from(exe_dir: &Path) -> Option<egui::IconData> {
    let path = logo_path(exe_dir);
    let bytes = std::fs::read(&path).ok()?;
    match eframe::icon_data::from_png_bytes(&bytes) {
        Ok(icon) => Some(icon),
        Err(err) => {
            eprintln!(
                "[stokesforge-app] cannot decode the logo {}: {err}",
                path.display()
            );
            None
        }
    }
}

/// The window icon: the logo, or an **empty** icon. The empty one is what keeps
/// the system's generic icon: without an icon at all, egui sets its own
/// default one on Windows. Windows opened later inherit the same `Arc`, so no
/// command is sent per frame.
#[must_use]
pub fn window_icon(logo: Option<&egui::IconData>) -> Arc<egui::IconData> {
    Arc::new(logo.cloned().unwrap_or_default())
}

/// The one door to a `ViewportBuilder`: every window starts from it, so that
/// every window has the application id. A window carries its icon either by
/// inheritance from the main window (egui copies it to child viewports) or
/// through [`main_viewport`].
#[must_use]
pub fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default().with_app_id(APP_ID)
}

/// The builder of the main window: identity and icon.
#[must_use]
pub fn main_viewport(logo: Option<&egui::IconData>) -> egui::ViewportBuilder {
    viewport().with_icon(window_icon(logo))
}

/// The logo as an egui image. `IconData` holds straight alpha, whereas
/// `ColorImage::from(IconData)` assumes premultiplied colour (a soft edge would
/// come out too bright).
#[must_use]
pub fn texture_image(icon: &egui::IconData) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 128]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn exe_dir_with(bytes: Option<&[u8]>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(bytes) = bytes {
            // The literal layout the packaging scripts install.
            let path = dir
                .path()
                .join("share/icons/hicolor/256x256/apps/stokesforge.png");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        dir
    }

    #[test]
    fn without_a_file_the_window_icon_is_empty_and_there_is_no_logo() {
        let dir = exe_dir_with(None);
        let logo = load_from(dir.path());
        assert!(logo.is_none());
        let builder = main_viewport(logo.as_ref());
        let icon = builder
            .icon
            .expect("an icon, so egui keeps none of its own");
        assert!(icon.rgba.is_empty());
    }

    #[test]
    fn a_valid_png_gives_the_logo_and_the_window_icon() {
        let dir = exe_dir_with(Some(&png(256, 256)));
        let logo = load_from(dir.path()).expect("the logo");
        assert_eq!((logo.width, logo.height), (256, 256));
        let icon = main_viewport(Some(&logo)).icon.unwrap();
        assert_eq!((icon.width, icon.height), (256, 256));
    }

    #[test]
    fn corrupt_bytes_degrade_like_an_absent_file() {
        let dir = exe_dir_with(Some(b"not a png"));
        assert!(load_from(dir.path()).is_none());
    }

    #[test]
    fn the_main_window_has_the_icon_and_the_application_id() {
        let builder = main_viewport(None);
        assert!(builder.icon.is_some());
        assert_eq!(builder.app_id.as_deref(), Some("stokesforge"));
    }

    #[test]
    fn the_texture_keeps_straight_alpha_colour_unscaled_by_alpha() {
        let icon = egui::IconData {
            rgba: vec![255, 0, 0, 128],
            width: 1,
            height: 1,
        };
        let image = texture_image(&icon);
        // Premultiplied storage: red scaled by alpha (about 128), not 255.
        assert!((126..=130).contains(&image.pixels[0].r()));
        assert_eq!(image.pixels[0].a(), 128);
    }
}
