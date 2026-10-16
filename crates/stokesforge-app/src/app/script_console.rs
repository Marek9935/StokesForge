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
//! Un contenu unique lit AppState en mode attaché et `ScriptConsoleModel`
//! en mode détaché (#100) : aucun moteur ne migre dans le callback. Le tampon
//! de saisie remonte avant les actions, la sortie reste celle de la racine.

use eframe::egui;

use stokesforge_app_state::state::{Action, AppState, ScriptOutput};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::shortcuts::SHORTCUT_RUN_SCRIPT;
use super::window_chrome::{
    panel_header, show_window_contents_bounded, PanelPlacement, WindowHeaderActions,
};
use super::StokesForgeApp;

/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;

/// Barre de titre de la console de script, avec le bouton qui la replie ou la
/// déplie (issue #79), et celui qui la détache (#100).
pub(super) fn script_header(
    ui: &mut egui::Ui,
    state: &AppState,
    collapsed: bool,
) -> WindowHeaderActions {
    let header = panel_header(ui, &tr!("script-title"), PanelPlacement::Bottom, collapsed);
    if !collapsed && !state.script_available() {
        ui.weak(tr!("script-unavailable"));
    }
    header
}

/// Lecture de la console pour le callback différé ; le moteur et son travail
/// restent dans AppState à la racine. Seule la saisie remonte comme requête.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ScriptConsoleModel {
    pub(super) source: String,
    pub(super) output: Option<ScriptOutput>,
    pub(super) running: bool,
    pub(super) available: bool,
}

impl ScriptConsoleModel {
    pub(super) fn from_state(state: &AppState) -> Self {
        Self {
            source: state.script_source().to_owned(),
            output: state.script_output().cloned(),
            running: state.script_running(),
            available: state.script_available(),
        }
    }
}

pub(super) fn script_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let mut source = state.script_source().to_owned();
    script_fields_contents(
        ui,
        ctx,
        &mut source,
        state.script_output(),
        state.script_running(),
        state.script_available(),
        actions,
    );
    if source != state.script_source() {
        state.set_script_source(source);
    }
}

pub(super) fn script_model_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    model: &mut ScriptConsoleModel,
    actions: &mut Vec<Action>,
) {
    script_fields_contents(
        ui,
        ctx,
        &mut model.source,
        model.output.as_ref(),
        model.running,
        model.available,
        actions,
    );
}

/// La console attachée emprunte la sortie : pas de copie de tout un résultat
/// à chaque image. La lecture détachée en possède déjà sa copie publiée.
fn script_fields_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    source: &mut String,
    output: Option<&ScriptOutput>,
    running: bool,
    available: bool,
    actions: &mut Vec<Action>,
) {
    if running {
        // Sans ça, egui ne redessine que sur événement d'entrée : la fin du
        // script n'apparaîtrait qu'au prochain geste de l'utilisateur.
        ctx.request_repaint();
    }

    let unavailable_reason = if running {
        tr!("disabled-script-running")
    } else {
        tr!("disabled-script-unavailable")
    };
    let available = available && !running;
    ui.add_enabled(
        available,
        egui::TextEdit::multiline(source)
            .code_editor()
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    )
    .on_disabled_hover_text(&unavailable_reason);

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                egui::Button::new(tr!("script-run"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .on_disabled_hover_text(&unavailable_reason)
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new(tr!("script-stop")))
            .on_disabled_hover_text(tr!("disabled-script-not-running"))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            super::busy::indicator(ui, &tr!("script-running"));
        }
    });

    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match output {
            None => {
                empty_state(ui, tr!("script-not-run"));
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
    /// hauteur. Le détachement utilise la même lecture de console et laisse
    /// le moteur à la racine ; le masquage View ne suspend pas son travail.
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
        self.drain_script_console(ctx, actions);
        if !self.script_open {
            return;
        }
        if self.script_detached {
            self.show_detached_script_console(ctx);
            return;
        }
        let collapsed = self.script_collapsed;
        let panel = egui::Panel::bottom(if collapsed {
            "stokesforge_script_collapsed"
        } else {
            "stokesforge_script"
        })
        .resizable(!collapsed);
        let panel = if collapsed {
            panel
                .exact_size(super::window_chrome::PANEL_STRIP_THICKNESS)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
                .show_separator_line(false)
        } else {
            panel.default_size(SCRIPT_PANEL_HEIGHT)
        };
        panel.show(ui, |ui| {
            let header = script_header(ui, &self.state, collapsed);
            self.script_collapsed ^= header.toggle_collapse;
            if header.detach {
                self.detach_script_console();
            }
            if !collapsed {
                // Le contenu seul peut être plus court que la hauteur
                // choisie : occuper la place réservée la conserve (#100).
                show_window_contents_bounded(ui, |ui| {
                    script_contents(ui, ctx, &mut self.state, actions);
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_session::Session;

    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::test_support::painted;
    use super::super::window_chrome::tests::find_icon_centers;

    fn state() -> (Session, AppState) {
        let project = Project::new("Console").unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state)
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
                    toggled
                        .set(toggled.get() | script_header(ui, &state, collapsed).toggle_collapse);
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
                !collapsed && !state.script_available(),
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
