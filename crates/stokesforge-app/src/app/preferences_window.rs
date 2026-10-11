//! File › Preferences (#148) : la version minimale de la fenêtre des
//! préférences de l'application. Une seule rubrique, **General**, qui ne
//! porte que la langue, seul réglage global déjà persisté et réglable ailleurs
//! (File › Language). Les autres rubriques de l'inventaire de #148 (Display,
//! Export, Folders and Paths) s'ajouteront une à une, avec leurs questions
//! tranchées.
//!
//! Les réglages s'appliquent **tout de suite** (comme le menu Language) : la
//! fenêtre ne change donc rien à valider ou à annuler et passe par
//! `modal_read_only` (Close seul, F2). Elle ne touche ni le projet ni
//! l'historique ; l'état ouvert/fermé est une décision d'interface de
//! `AppState`, l'application de la langue reste à `StokesForgeApp`.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome};
use super::form::{choice, form, ChoiceOption};
use crate::i18n::{self, Language};
use crate::state::{Action, AppState};
use crate::tr;

/// Peint la fenêtre tant qu'elle est ouverte. Rend la langue choisie, s'il y a
/// lieu ; l'appelant l'applique et l'enregistre.
pub(super) fn preferences_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) -> Option<Language> {
    if !state.preferences_open() {
        return None;
    }
    let mut chosen = None;
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_preferences"),
        &tr!("preferences-title"),
        egui::vec2(380.0, 160.0),
        |ui| {
            form(ui, "preferences", |rows| {
                rows.section(tr!("preferences-general"));
                rows.row(tr!("menu-language"), |ui| {
                    let mut language = i18n::language();
                    let options = [Language::English, Language::French]
                        .map(|language| ChoiceOption::new(language, language.native_name()));
                    if choice(ui, "preferences_language", &mut language, &options) {
                        chosen = Some(language);
                    }
                });
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::ClosePreferences);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une image de la fenêtre, ouverte ou non ; rend les textes peints.
    fn textes(open: bool) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Préf").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenPreferences);
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            },
            |_| {
                preferences_window(&ctx, &state, &mut actions);
            },
        );
        output.textures_delta.clear();
        session.shutdown().unwrap();
        output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Fermée, la fenêtre ne peint rien ; ouverte, elle montre son titre, sa
    /// rubrique et la langue.
    #[test]
    fn la_fenetre_montre_la_langue_quand_elle_est_ouverte() {
        assert!(textes(false).is_empty());
        let texts = textes(true);
        assert!(texts.contains(&tr!("preferences-title")), "{texts:?}");
        assert!(texts.contains(&tr!("preferences-general")), "{texts:?}");
        assert!(texts.contains(&tr!("menu-language")), "{texts:?}");
    }
}
