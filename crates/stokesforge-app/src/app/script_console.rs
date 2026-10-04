//! Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
//! sortie. Contenu de [`StokesForgeApp::script_panel`].
//!
//! Ne bloque jamais la fenêtre : voir la doc de tête d'`app.rs`, § « La
//! console de script ne fige pas la fenêtre ».
//! `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
//! quelle autre action, appliquées après le rendu par la boucle
//! principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
//! bloquent pas.
//!
//! Sorti d'`app.rs` tel quel (#121) ; le panneau qui la porte,
//! [`StokesForgeApp::script_panel`], l'a rejoint (#122).

use eframe::egui;

use crate::icons::Icon;
use crate::state::{Action, AppState};
use crate::tr;

use super::shortcuts::SHORTCUT_RUN_SCRIPT;
use super::window_chrome::{header_icon, HEADER_ACTION_WIDTH};
use super::StokesForgeApp;

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

/// Hauteur par défaut du panneau fixe de la console de script — point de
/// départ seulement, l'utilisateur peut ensuite le redimensionner en
/// glissant son bord (`.resizable(true)`, voir [`StokesForgeApp::script_panel`]).
const SCRIPT_PANEL_HEIGHT: f32 = 220.0;

impl StokesForgeApp {
    /// Peint la console de script dans son panneau fixe, ancré en bas — voir
    /// la doc de tête d'`app.rs`, § Ce qui reste ici (l'ordre des panneaux).
    /// Appelé par
    /// [`StokesForgeApp::ui`](eframe::App::ui) *avant* [`StokesForgeApp::project_manager_panel`]
    /// (`egui::Panel::left`) : avec `egui::Panel`, le premier panneau ajouté
    /// est le plus extérieur, donc celui-ci, posé en premier, réserve sa
    /// bande sur toute la largeur de la fenêtre — y compris sous le Project
    /// Manager, pas seulement sous la zone du `CentralPanel` — plutôt que
    /// d'être resserré entre lui et le bord droit. `egui::Panel::bottom`
    /// n'est **pas** redimensionnable par défaut (à la différence de
    /// `left`/`right`) : `.resizable(true)` restaure le glisser du bord en
    /// hauteur. Contrairement au Project
    /// Manager, aucun mode détaché en fenêtre OS séparée ni bascule de
    /// position : la console de script a toujours été en bas, et le reste.
    ///
    /// Repliée (issue #79), la console se réduit à sa barre de titre — un
    /// panneau distinct, non redimensionnable, pour que la hauteur mémorisée
    /// du panneau déplié ne soit pas écrasée.
    pub(super) fn script_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
    ) {
        if actions
            .iter()
            .any(|action| matches!(action, Action::RunScript))
        {
            self.script_collapsed = false;
        }
        if self.script_collapsed {
            egui::Panel::bottom("stokesforge_script_collapsed")
                .resizable(false)
                .show(ui, |ui| {
                    if script_header(ui, &self.state, true) {
                        self.script_collapsed = false;
                    }
                });
            return;
        }
        egui::Panel::bottom("stokesforge_script")
            .resizable(true)
            .default_size(SCRIPT_PANEL_HEIGHT)
            .show(ui, |ui| {
                if script_header(ui, &self.state, false) {
                    self.script_collapsed = true;
                }
                ui.separator();
                script_contents(ui, ctx, &mut self.state, actions);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_session::Session;

    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::window_chrome::tests::find_icon_centers;

    fn state() -> (Session, AppState) {
        let project = Project::new("Console").unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state)
    }

    fn painted(output: &egui::FullOutput, needle: &str) -> bool {
        output.shapes.iter().any(|painted| {
            matches!(&painted.shape,
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
        })
    }

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 400.0),
            )),
            events,
            ..Default::default()
        }
    }

    /// La barre de titre (#79) : un clic sur son chevron le signale, une image
    /// sans clic non ; elle dit quand l'interpréteur manque à ce binaire.
    #[test]
    fn la_barre_de_titre_signale_le_clic_du_chevron_et_l_absence_d_interpreteur() {
        let (_session, state) = state();
        for collapsed in [true, false] {
            let context = egui::Context::default();
            egui_extras::install_image_loaders(&context);
            let toggled = std::cell::Cell::new(false);
            let paint = |context: &egui::Context, events| {
                let mut output = context.run_ui(input(events), |ui| {
                    toggled.set(toggled.get() | script_header(ui, &state, collapsed));
                });
                output.textures_delta.clear();
                output
            };
            paint(&context, Vec::new());
            let output = paint(&context, Vec::new());
            assert!(!toggled.get(), "sans clic, pas de bascule");
            assert!(painted(&output, &tr!("script-title")));
            assert_eq!(
                painted(&output, &tr!("script-unavailable")),
                !state.script_available(),
                "l'absence d'interpréteur se dit, et seulement elle"
            );
            let chevron = find_icon_centers(&output.shapes)[0];
            click_at(&context, chevron, egui::Modifiers::NONE, paint);
            assert!(
                toggled.get(),
                "le clic sur le chevron bascule (replié : {collapsed})"
            );
        }
    }

    /// « Exécuter » n'émet `RunScript` que si un interpréteur est là ;
    /// « Arrêter » est grisé tant qu'aucun script ne tourne. Avant toute
    /// exécution, la sortie le dit.
    #[test]
    fn executer_n_emet_que_si_un_interpreteur_est_la_et_arreter_reste_grise() {
        let (_session, mut state) = state();
        let context = egui::Context::default();
        let actions = std::cell::RefCell::new(Vec::new());
        let state = std::cell::RefCell::new(&mut state);
        let paint = |context: &egui::Context, events| {
            let mut output = context.run_ui(input(events), |ui| {
                let ctx = ui.ctx().clone();
                script_contents(ui, &ctx, &mut state.borrow_mut(), &mut actions.borrow_mut());
            });
            output.textures_delta.clear();
            output
        };
        paint(&context, Vec::new());
        let output = paint(&context, Vec::new());
        assert!(painted(&output, &tr!("script-not-run")));
        let available = state.borrow().script_available();

        click_at(
            &context,
            find_text_center(&output.shapes, &tr!("script-stop")),
            egui::Modifiers::NONE,
            paint,
        );
        assert!(
            actions.borrow().is_empty(),
            "Arrêter est grisé sans script en cours"
        );

        click_at(
            &context,
            find_text_center(&output.shapes, &tr!("script-run")),
            egui::Modifiers::NONE,
            paint,
        );
        let expected = if available {
            vec![Action::RunScript]
        } else {
            Vec::new()
        };
        assert_eq!(
            *actions.borrow(),
            expected,
            "interpréteur présent : {available}"
        );
    }

    /// Repliée au démarrage, la console n'a que sa barre de titre (#79) ;
    /// une exécution demandée dans l'image la déplie, pour qu'on en voie la
    /// sortie, et le panneau déplié peint le contenu.
    #[test]
    fn lancer_un_script_deplie_la_console() {
        let (_session, state) = state();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        let paint = |app: &mut StokesForgeApp, mut actions: Vec<Action>| {
            let mut output = context.run_ui(input(Vec::new()), |ui| {
                let ctx = ui.ctx().clone();
                app.script_panel(ui, &ctx, &mut actions);
            });
            output.textures_delta.clear();
            output
        };
        assert!(app.script_collapsed, "repliée au démarrage");
        let output = paint(&mut app, Vec::new());
        assert!(app.script_collapsed, "sans exécution, elle reste repliée");
        assert!(painted(&output, &tr!("script-title")));
        assert!(
            !painted(&output, &tr!("script-not-run")),
            "repliée : pas de contenu"
        );

        let output = paint(&mut app, vec![Action::RunScript]);
        assert!(!app.script_collapsed, "une exécution la déplie");
        assert!(painted(&output, &tr!("script-not-run")));
    }
}
