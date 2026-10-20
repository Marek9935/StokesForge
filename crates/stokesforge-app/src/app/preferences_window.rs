//! File › Preferences (#148) : la fenêtre des préférences de l'application,
//! en trois onglets : **General** (la langue, réglable aussi par
//! File › Language, et l'analyse), **Display** (les classeurs neufs) et
//! **Export** (les formats de Copy Graph : l'ordre se change au glisser,
//! `reorder_list.rs`, chaque format se coche). Les autres rubriques de
//! l'inventaire de #148 (Folders and Paths) s'ajouteront une à une, avec
//! leurs questions tranchées.
//!
//! Les réglages s'appliquent **tout de suite** (comme le menu Language) : la
//! fenêtre n'a rien à valider ni à annuler et passe par `modal_live_settings`
//! (Close seul, S-F2 ; interactive, donc focus sur le premier champ et taille
//! par classe, S-F9, S-F11). Elle ne touche ni le projet ni
//! l'historique ; l'état ouvert/fermé est une décision d'interface de
//! `AppState`, l'application de la langue reste à `StokesForgeApp`.

use eframe::egui;

use super::dialogs::{modal_live_settings, DialogOutcome, InteractiveSize, SizeClass};
use super::form::{choice, form, ChoiceOption};
use super::reorder_list::{move_before, reorder_move, reorder_row};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::preferences::{CopyFormat, CopyFormatEntry};
use stokesforge_app_state::quantities;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Les rubriques, dans l'ordre de #148 déjà livrées.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Page {
    #[default]
    General,
    Display,
    Export,
}

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
    // La rubrique ouverte n'est qu'une commodité de la fenêtre : ni `AppState`,
    // ni le fichier des préférences.
    let page_id = egui::Id::new("stokesforge_preferences_page");
    let mut page: Page = ctx.data(|data| data.get_temp(page_id)).unwrap_or_default();
    let outcome = modal_live_settings(
        ctx,
        egui::Id::new("stokesforge_preferences"),
        &tr!("preferences-title"),
        InteractiveSize::Class(SizeClass::Medium),
        |ui| {
            let tabs = [
                (Page::General, tr!("preferences-general")),
                (Page::Display, tr!("preferences-display")),
                (Page::Export, tr!("preferences-export")),
            ];
            tab_strip(
                ui,
                TabStripSide::Top,
                &tabs,
                page,
                |picked| page = picked,
                None,
                || {},
            );
            match page {
                Page::General => chosen = general_page(ui, state, actions),
                Page::Display => display_page(ui, state, actions),
                Page::Export => export_page(ui, state, actions),
            }
        },
    );
    ctx.data_mut(|data| data.insert_temp(page_id, page));
    if outcome != DialogOutcome::Open {
        actions.push(Action::ClosePreferences);
    }
    chosen
}

/// Le sigle d'un format, par le catalogue comme tout texte d'interface.
fn format_name(format: CopyFormat) -> String {
    match format {
        CopyFormat::Svg => tr!("preferences-copy-svg"),
        CopyFormat::Png => tr!("preferences-copy-png"),
        CopyFormat::Pdf => tr!("preferences-copy-pdf"),
    }
}

fn general_page(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
) -> Option<Language> {
    let mut chosen = None;
    let mut low_r2 = state.analysis_low_r2_dialog();
    let before = low_r2;
    let mut max_file_mb = state.max_file_mb();
    let max_file_before = max_file_mb;
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
        // Plafond de taille d'un fichier à l'ouverture (A2, #316).
        rows.section(tr!("preferences-files"));
        rows.row(tr!("preferences-max-file-size"), |ui| {
            number_field(
                ui,
                "preferences_max_file_mb",
                &mut max_file_mb,
                &quantities::MAX_FILE_MB,
            );
        });
        rows.note(tr!("preferences-max-file-size-note"));
        // Préférence de l'application (#288) ; chaque calcul a la sienne.
        rows.section(tr!("preferences-analysis"));
        rows.checkbox(
            &mut low_r2,
            tr!("preferences-low-r2-dialog"),
            Some(tr!("preferences-low-r2-dialog", "hint")),
        );
    });
    if low_r2 != before {
        actions.push(Action::SetAnalysisLowR2Dialog(low_r2));
    }
    if max_file_mb != max_file_before {
        actions.push(Action::SetMaxFileMb(max_file_mb));
    }
    chosen
}

/// Les rangées d'en-tête des **nouveaux** classeurs ; les classeurs ouverts
/// gardent leur réglage propre (menu Classeur, case #).
fn display_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let (mut metadata, mut x_prime) = state.workbook_header_defaults();
    let before = (metadata, x_prime);
    form(ui, "preferences_display", |rows| {
        rows.section(tr!("preferences-new-workbooks"));
        rows.note(tr!("preferences-new-workbooks-note"));
        rows.field(|ui| {
            ui.checkbox(&mut metadata, tr!("preferences-show-metadata-row"));
        });
        rows.field(|ui| {
            ui.checkbox(&mut x_prime, tr!("preferences-show-x-prime-row"));
        });
    });
    if (metadata, x_prime) != before {
        actions.push(Action::SetWorkbookHeaderDefaults { metadata, x_prime });
    }
}

/// Copy Graph : les formats offerts ensemble, dans l'ordre où les applications
/// les voient. Un réglage change tout de suite (`Action::SetCopyFormats`).
fn export_page(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let mut entries: Vec<CopyFormatEntry> = state.copy_formats().to_vec();
    let before = entries.clone();
    let offered = entries.iter().filter(|entry| entry.on).count();
    form(ui, "preferences_export", |rows| {
        rows.section(tr!("preferences-copy-graph"));
        rows.note(tr!("preferences-copy-graph-note"));
        let ids: Vec<egui::Id> = entries
            .iter()
            .map(|entry| egui::Id::new(("preferences_copy_format", entry.format)))
            .collect();
        let mut over = None;
        for (index, id) in ids.iter().enumerate() {
            let entry = &mut entries[index];
            let name = format_name(entry.format);
            // Le dernier format offert ne se décoche pas : il faut en offrir un.
            let last_offered = entry.on && offered == 1;
            rows.field(|ui| {
                let hovered = reorder_row(
                    ui,
                    *id,
                    tr!("preferences-copy-move", format = name.as_str()),
                    &tr!("preferences-copy-drag"),
                    |ui| {
                        ui.set_min_width(220.0);
                        ui.weak(format!("{}.", index + 1));
                        ui.add_enabled(
                            !last_offered,
                            egui::Checkbox::new(&mut entry.on, name.as_str()),
                        )
                        .on_disabled_hover_text(tr!("disabled-copy-last-format"));
                    },
                );
                if hovered {
                    over = Some(index);
                }
            });
        }
        if let Some((from, to)) = reorder_move(&ctx, &ids, over) {
            move_before(&mut entries, from, to);
        }
    });
    if entries != before {
        actions.push(Action::SetCopyFormats(entries));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    /// Une image de la fenêtre, ouverte ou non ; rend les textes peints.
    fn textes(open: bool) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Préf").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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

    /// Le banc de la fenêtre réelle : un état d'application sans fichier de
    /// préférences, les actions rejouées à chaque image.
    struct Banc {
        _session: stokesforge_session::Session,
        state: AppState,
    }

    fn banc() -> egui_kittest::Harness<'static, Banc> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Préf").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            "test.stf".to_string(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        state.dispatch(Action::OpenPreferences);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(760.0, 600.0))
            .build_ui_state(
                |ui, banc: &mut Banc| {
                    let mut actions = Vec::new();
                    preferences_window(ui.ctx(), &banc.state, &mut actions);
                    for action in actions {
                        banc.state.dispatch(action);
                    }
                },
                Banc {
                    _session: session,
                    state,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn ordre(harness: &egui_kittest::Harness<'static, Banc>) -> Vec<(CopyFormat, bool)> {
        harness
            .state()
            .state
            .copy_formats()
            .iter()
            .map(|entry| (entry.format, entry.on))
            .collect()
    }

    fn vers_export(harness: &mut egui_kittest::Harness<'static, Banc>) {
        harness.get_by_label(&tr!("preferences-export")).click();
        harness.run();
    }

    /// Décocher un format l'ôte de l'offre ; le dernier offert reste coché.
    #[test]
    fn decocher_un_format_le_retire_et_le_dernier_reste() {
        let mut harness = banc();
        vers_export(&mut harness);
        harness.get_by_label("PNG").click();
        harness.run();
        harness.get_by_label("PDF").click();
        harness.run();
        assert_eq!(
            ordre(&harness),
            [
                (CopyFormat::Svg, true),
                (CopyFormat::Png, false),
                (CopyFormat::Pdf, false)
            ]
        );
        harness.get_by_label("SVG").click();
        harness.run();
        assert!(harness.state().state.copy_formats()[0].on, "dernier offert");
    }

    /// Glisser « 3. PDF » au-dessus de « 1. SVG » : l'ordre offert change.
    #[test]
    fn glisser_une_ligne_change_l_ordre_offert() {
        let mut harness = banc();
        vers_export(&mut harness);
        let from = harness
            .get_by_label(&tr!("preferences-copy-move", format = "PDF"))
            .rect()
            .center();
        let first = harness
            .get_by_label(&tr!("preferences-copy-move", format = "SVG"))
            .rect();
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from - egui::vec2(0.0, 12.0));
        harness.step();
        harness.hover_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.step();
        harness.step();
        harness.drop_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.run();
        let formats: Vec<CopyFormat> = ordre(&harness).into_iter().map(|(f, _)| f).collect();
        assert_eq!(formats, [CopyFormat::Pdf, CopyFormat::Svg, CopyFormat::Png]);
    }

    /// Rubrique Display : Metadata est cochée et x' décochée par défaut ; cocher
    /// x' règle les nouveaux classeurs.
    #[test]
    fn la_rubrique_affichage_regle_les_rangees_des_nouveaux_classeurs() {
        let mut harness = banc();
        harness.get_by_label(&tr!("preferences-display")).click();
        harness.run();
        assert_eq!(
            harness.state().state.workbook_header_defaults(),
            (true, false)
        );
        harness
            .get_by_label(&tr!("preferences-show-x-prime-row"))
            .click();
        harness.run();
        harness
            .get_by_label(&tr!("preferences-show-metadata-row"))
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.workbook_header_defaults(),
            (false, true)
        );
    }
}
