//! Help › About StokesForge (#382): version, license, project page and a Copy
//! button for the line a bug report cites. Read-only, hence `modal_read_only`
//! (Close only, S-F2). The line itself is built by
//! `stokesforge_app_state::startup::version_line`; the clipboard write is done
//! by the shell when it applies `Action::CopyVersionLine`. The logo of an
//! official build (#388) sits at the top, centred; a build without logo has no
//! gap where it would be.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use crate::app_logo::ABOUT_LOGO_SIZE;
use stokesforge_app_state::startup::{LICENSE, REPOSITORY_URL, VERSION};
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Room taken by the logo: its side, the space below it and the item spacing
/// after it (a window too short hides the Copy button under the footer).
const LOGO_ROOM: f32 = ABOUT_LOGO_SIZE + 8.0 + 8.0;

/// Paints the window while it is open. `logo` is the texture of the logo, `None`
/// for a build without one.
pub(super) fn about_window(
    ctx: &egui::Context,
    state: &AppState,
    logo: Option<egui::TextureId>,
    actions: &mut Vec<Action>,
) {
    if !state.about_open() {
        return;
    }
    let height = 140.0 + if logo.is_some() { LOGO_ROOM } else { 0.0 };
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_about"),
        &tr!("about-title"),
        PlainSize::Content(egui::vec2(360.0, height)),
        |ui| {
            if let Some(texture) = logo {
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((
                        texture,
                        egui::vec2(ABOUT_LOGO_SIZE, ABOUT_LOGO_SIZE),
                    )));
                    ui.add_space(8.0);
                });
            }
            ui.label(tr!("about-version", version = VERSION));
            ui.label(tr!("about-license", license = LICENSE));
            ui.hyperlink_to(tr!("about-repository"), REPOSITORY_URL);
            if ui
                .button(tr!("action-copy-version"))
                .on_hover_text(tr!("action-copy-version", "hint"))
                .clicked()
            {
                actions.push(Action::CopyVersionLine);
            }
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseAbout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_app_state::startup::version_line;

    /// Runs two frames of the window (a window is painted on the second),
    /// the second one with `events`; returns the painted texts and the
    /// actions requested.
    fn frame(open: bool, click_copy: bool) -> (Vec<String>, Vec<Action>) {
        let (texts, actions, _) = frame_with_logo(open, click_copy, false);
        (texts, actions)
    }

    /// What the logo painted: the screen rects of the images that use the
    /// texture given to the window, and the rect of the version text.
    struct LogoPaint {
        images: Vec<egui::Rect>,
        version: Option<egui::Rect>,
    }

    /// [`frame`], with a logo texture handed to the window when `logo`.
    fn frame_with_logo(
        open: bool,
        click_copy: bool,
        logo: bool,
    ) -> (Vec<String>, Vec<Action>, LogoPaint) {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("About").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenAbout);
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let screen = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        ));
        let texture = logo.then(|| {
            ctx.load_texture(
                "test-logo",
                egui::ColorImage::filled([4, 4], egui::Color32::RED),
                egui::TextureOptions::LINEAR,
            )
        });
        let texture_id = texture.as_ref().map(egui::TextureHandle::id);
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        let run = |events: Vec<egui::Event>, actions: &mut Vec<Action>| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: screen,
                    events,
                    ..Default::default()
                },
                |_| about_window(&ctx, &state, texture_id, actions),
            )
        };
        for _ in 0..2 {
            output = run(Vec::new(), &mut actions);
            output.textures_delta.clear();
        }
        let texts = |output: &egui::FullOutput| -> Vec<String> {
            output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                    _ => None,
                })
                .collect()
        };
        let painted = texts(&output);
        let version = tr!("about-version", version = VERSION);
        let paint = LogoPaint {
            images: output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect) if Some(rect.fill_texture_id()) == texture_id => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect(),
            version: output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == version => {
                        Some(shape.visual_bounding_rect())
                    }
                    _ => None,
                }),
        };
        if click_copy {
            // The Copy button is laid out below the three lines of text.
            let button = tr!("action-copy-version");
            let rect = output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == button => {
                        Some(shape.visual_bounding_rect())
                    }
                    _ => None,
                })
                .expect("the Copy button is painted");
            let pos = rect.center();
            let events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ];
            actions.clear();
            // Press and release in two frames, as a pointer does.
            let _ = run(events, &mut actions);
            let _ = run(Vec::new(), &mut actions);
        }
        session.shutdown().unwrap();
        (painted, actions, paint)
    }

    /// Closed, the window paints nothing; open, it shows its title, the
    /// version, the license and the project page.
    #[test]
    fn the_window_shows_version_license_and_page_when_it_is_open() {
        assert!(frame(false, false).0.is_empty());
        let (texts, _) = frame(true, false);
        assert!(texts.contains(&tr!("about-title")), "{texts:?}");
        assert!(
            texts.contains(&tr!("about-version", version = env!("CARGO_PKG_VERSION"))),
            "{texts:?}"
        );
        assert!(
            texts.contains(&tr!("about-license", license = "GPL-3.0-or-later")),
            "{texts:?}"
        );
        assert!(texts.contains(&tr!("about-repository")), "{texts:?}");
    }

    /// The Copy button asks for the version line to be copied.
    #[test]
    fn the_copy_button_requests_the_version_line() {
        let (_, actions) = frame(true, true);
        assert!(actions.contains(&Action::CopyVersionLine), "{actions:?}");
        assert!(version_line().contains(env!("CARGO_PKG_VERSION")));
    }

    /// Without a logo (build without `assets/logo/`), no image is painted and
    /// the texts are those of today.
    #[test]
    fn without_a_logo_nothing_is_painted_above_the_texts() {
        let (texts, _, paint) = frame_with_logo(true, false, false);
        assert!(paint.images.is_empty());
        assert_eq!(texts, frame(true, false).0);
        assert!(texts.contains(&tr!("about-repository")), "{texts:?}");
    }

    /// With a logo, one image is painted, centred above the version text, and
    /// the Copy button still works.
    #[test]
    fn the_logo_is_painted_once_above_the_version_text() {
        let (_, _, paint) = frame_with_logo(true, false, true);
        assert_eq!(paint.images.len(), 1, "{:?}", paint.images);
        let (logo, version) = (paint.images[0], paint.version.expect("the version text"));
        assert!(logo.max.y <= version.min.y, "{logo:?} above {version:?}");
        assert!((logo.width() - crate::app_logo::ABOUT_LOGO_SIZE).abs() < 1.0);
        let (_, actions, _) = frame_with_logo(true, true, true);
        assert!(actions.contains(&Action::CopyVersionLine), "{actions:?}");
    }
}
