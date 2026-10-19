//! Brique « travail en cours » (#232, S-P1/S-P2) : le même indicateur partout où
//! un travail dure.
//!
//! - **S-P1** : en dessous de [`SHOW_AFTER`] (≈ 0,5 s) rien ne s'affiche — un
//!   travail bref ne doit pas clignoter ; au-delà, un indicateur tournant et
//!   un mot (« Importing… »), avec une barre de progression quand le nombre
//!   d'étapes est connu ;
//! - **S-P2** : au-delà de [`STOP_AFTER`] (quelques secondes), un bouton Stop
//!   qui pousse l'`Action` donnée. Le travail dit lui-même ce que Stop
//!   défait (voir `AppState::stop_import`).
//!
//! La brique ne mesure rien : l'appelant donne la durée écoulée, lue sur le
//! travail (`AppState::import_work`). Elle demande elle-même l'image suivante
//! tant qu'elle est visible ou sur le point de l'être.

use std::time::Duration;

use eframe::egui;

use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

/// Durée avant laquelle un travail reste invisible (S-P1).
pub(super) const SHOW_AFTER: Duration = Duration::from_millis(500);
/// Durée à partir de laquelle Stop est proposé (S-P2).
pub(super) const STOP_AFTER: Duration = Duration::from_secs(3);

/// Un travail en cours, tel que la brique le peint.
pub(super) struct Work {
    /// Le mot : « Importing… ».
    pub(super) label: String,
    /// Depuis combien de temps il dure.
    pub(super) elapsed: Duration,
    /// `(faites, total)` quand le nombre d'étapes est connu.
    pub(super) progress: Option<(usize, usize)>,
    /// Ce que pousse Stop ; `None` si le travail ne s'interrompt pas.
    pub(super) stop: Option<Action>,
}

/// L'indicateur seul : tourniquet et mot. Pour les surfaces qui ont déjà leur
/// propre Stop (la console de script).
pub(super) fn indicator(ui: &mut egui::Ui, label: &str) {
    ui.spinner();
    ui.weak(label);
}

/// Peint le travail s'il dure depuis assez longtemps ; rend vrai si quelque
/// chose a été peint.
pub(super) fn work_in_progress(ui: &mut egui::Ui, work: &Work, actions: &mut Vec<Action>) -> bool {
    if work.elapsed < SHOW_AFTER {
        // Rien à voir encore, mais l'image où il apparaîtra doit venir.
        ui.ctx().request_repaint_after(SHOW_AFTER - work.elapsed);
        return false;
    }
    indicator(ui, &work.label);
    if let Some((done, total)) = work.progress {
        ui.add(
            egui::ProgressBar::new(if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            })
            .desired_width(120.0),
        );
    }
    match (&work.stop, work.elapsed >= STOP_AFTER) {
        (Some(action), true) => {
            if ui
                .button(tr!("busy-stop"))
                .on_hover_text(tr!("busy-stop", "hint"))
                .clicked()
            {
                actions.push(action.clone());
            }
        }
        (Some(_), false) => {
            ui.ctx().request_repaint_after(STOP_AFTER - work.elapsed);
        }
        (None, _) => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Peint `work` et rend (peint ?, textes, actions).
    fn paint(work: &Work) -> (bool, Vec<String>, Vec<Action>) {
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut actions = Vec::new();
        let mut drawn = false;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| drawn = work_in_progress(ui, work, &mut actions),
        );
        output.textures_delta.clear();
        let texts = output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        (drawn, texts, actions)
    }

    fn work(seconds: f32) -> Work {
        Work {
            label: "Importing…".into(),
            elapsed: Duration::from_secs_f32(seconds),
            progress: Some((1, 4)),
            stop: Some(Action::StopImport),
        }
    }

    /// S-P1 : rien avant 0,5 s ; le mot ensuite. S-P2 : Stop seulement après
    /// quelques secondes, et seulement si le travail s'interrompt.
    #[test]
    fn l_indicateur_et_stop_arrivent_chacun_a_leur_heure() {
        let (drawn, texts, _) = paint(&work(0.2));
        assert!(!drawn && texts.is_empty(), "{texts:?}");

        let (drawn, texts, _) = paint(&work(1.0));
        assert!(drawn);
        assert!(texts.contains(&"Importing…".to_string()), "{texts:?}");
        assert!(!texts.contains(&tr!("busy-stop")), "Stop trop tôt");

        let (_, texts, _) = paint(&work(4.0));
        assert!(texts.contains(&tr!("busy-stop")), "{texts:?}");

        let mut sans_stop = work(4.0);
        sans_stop.stop = None;
        let (_, texts, _) = paint(&sans_stop);
        assert!(!texts.contains(&tr!("busy-stop")), "{texts:?}");
    }
}
