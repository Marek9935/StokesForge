//! Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
//! sortie. Contenu de [`StokesForgeApp::script_panel`].
//!
//! Ne bloque jamais la fenêtre : voir la doc de tête de ce module.
//! `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
//! quelle autre action, appliquées après le rendu par la boucle
//! principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
//! bloquent pas.
//!
//! Sorti d'`app.rs` tel quel (#121).

use eframe::egui;

use crate::icons::Icon;
use crate::state::{Action, AppState};
use crate::tr;

use super::shortcuts::SHORTCUT_RUN_SCRIPT;
use super::window_chrome::{header_icon, HEADER_ACTION_WIDTH};

/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;

/// Barre de titre de la console de script, avec le bouton qui la replie ou la
/// déplie (issue #79). Rend `true` si ce bouton a été cliqué.
pub(super) fn script_header(ui: &mut egui::Ui, state: &AppState, collapsed: bool) -> bool {
    ui.horizontal(|ui| {
        let (icon, tooltip) = if collapsed {
            (Icon::CaretUp, tr!("script-expand"))
        } else {
            (Icon::CaretDown, tr!("script-collapse"))
        };
        // Même cellule de `HEADER_ACTION_WIDTH` que les icônes des fenêtres,
        // l'icône centrée dedans et la cellule centrée sur la ligne du titre
        // (#95) : un `icon_button` collait le chevron à gauche, sans cellule.
        let row_height = ui.text_style_height(&egui::TextStyle::Heading);
        let (cell, _) = ui.allocate_exact_size(
            egui::vec2(HEADER_ACTION_WIDTH, row_height),
            egui::Sense::hover(),
        );
        let toggled = header_icon(ui, ui.id(), "script-toggle", cell, icon, &tooltip);
        ui.heading(tr!("script-title"));
        if !state.script_available() {
            ui.weak(tr!("script-unavailable"));
        }
        toggled
    })
    .inner
}

pub(super) fn script_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let running = state.script_running();
    if running {
        // Sans ça, egui ne redessine que sur événement d'entrée : la fin du
        // script n'apparaîtrait qu'au prochain geste de l'utilisateur.
        ctx.request_repaint();
    }

    let available = state.script_available() && !running;
    let mut source = state.script_source().to_string();
    let edit = ui.add_enabled(
        available,
        egui::TextEdit::multiline(&mut source)
            .code_editor()
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    if edit.changed() {
        state.set_script_source(source);
    }

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                egui::Button::new(tr!("script-run"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new(tr!("script-stop")))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            ui.spinner();
            ui.weak(tr!("script-running"));
        }
    });

    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match state.script_output() {
            None => {
                ui.weak(tr!("script-not-run"));
            }
            Some(output) => {
                let mut empty = true;
                if !output.stdout.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stdout"));
                    ui.monospace(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stderr"));
                    ui.monospace(&output.stderr);
                }
                if let Some(error) = &output.error {
                    empty = false;
                    ui.colored_label(ui.visuals().warn_fg_color, tr!("script-traceback"));
                    ui.monospace(error);
                }
                if empty {
                    ui.weak(tr!("script-no-output"));
                }
            }
        });
}
