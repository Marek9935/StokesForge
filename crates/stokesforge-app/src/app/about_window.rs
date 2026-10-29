//! Help › About StokesForge (#382): version, license, project page and a Copy
//! button for the line a bug report cites. Read-only, hence `modal_read_only`
//! (Close only, S-F2). The line itself is built by
//! `stokesforge_app_state::startup::version_line`; the clipboard write is done
//! by the shell when it applies `Action::CopyVersionLine`.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use stokesforge_app_state::startup::{LICENSE, REPOSITORY_URL, VERSION};
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Paints the window while it is open.
pub(super) fn about_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if !state.about_open() {
        return;
    }
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_about"),
        &tr!("about-title"),
        PlainSize::Content(egui::vec2(360.0, 140.0)),
        |ui| {
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
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        let run = |events: Vec<egui::Event>, actions: &mut Vec<Action>| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: screen,
                    events,
                    ..Default::default()
                },
                |_| about_window(&ctx, &state, actions),
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
        (painted, actions)
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
}
