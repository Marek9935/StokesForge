//! Feuille transposée liée (#253) : la bande qui la signale en haut de la
//! grille, avec son état (pas à jour, valeurs modifiées) et le bouton qui la
//! remet à jour, et la confirmation qui précède l'écrasement de valeurs
//! éditées. La création et le calcul sont dans `stokesforge-app-state`
//! (`state/transpose.rs`) ; ici, seulement la peinture et les gestes.

use eframe::egui;
use stokesforge_core::NodeId;

use super::dialogs::{modal_confirm, ConfirmOutcome, PlainSize};
use super::tab_strip::TAB_STRIP_HEIGHT;
use crate::theme;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Hauteur de la bande, la même que celle des onglets de feuille.
pub(super) const BANNER_HEIGHT: f32 = TAB_STRIP_HEIGHT;

/// La bande de `sheet` si c'est une feuille transposée liée : d'où elle vient,
/// ce qui a changé, et Recalculate (Relink quand des valeurs ont été éditées).
/// L'appelant a réservé sa place quand `state.transpose_info(sheet)` existe.
pub(super) fn transpose_banner(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(info) = state.transpose_info(sheet) else {
        return;
    };
    ui.horizontal_centered(|ui| {
        ui.add_space(theme::SPACE_ITEM);
        ui.label(tr!("transpose-banner", source = info.source_name.as_str()));
        let warn = theme::stale_color(ui.visuals().dark_mode);
        if info.stale {
            ui.label(egui::RichText::new(tr!("transpose-state-stale")).color(warn));
        }
        if info.modified {
            ui.label(egui::RichText::new(tr!("transpose-state-modified")).color(warn));
        }
        if info.stale || info.modified {
            let (label, hint) = if info.modified {
                (
                    tr!("transpose-relink"),
                    tr!("transpose-relink-hint", source = info.source_name.as_str()),
                )
            } else {
                (
                    tr!("transpose-recalculate"),
                    tr!(
                        "transpose-recalculate-hint",
                        source = info.source_name.as_str()
                    ),
                )
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                actions.push(Action::RecalculateTranspose { sheet });
            }
        }
    });
}

/// Écraser des valeurs éditées dans les colonnes liées : confirmation avant
/// (Ctrl+Z rattrape le geste, mais on prévient d'abord).
pub(super) fn transpose_relink_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(sheet) = state.pending_transpose_relink() else {
        return;
    };
    let name = state
        .tree()
        .node(sheet)
        .map(|node| node.name().to_string())
        .unwrap_or_default();
    let source = state
        .transpose_info(sheet)
        .map(|info| info.source_name)
        .unwrap_or_default();
    let outcome = modal_confirm(
        ctx,
        egui::Id::new("stokesforge_transpose_relink"),
        &tr!("transpose-relink-title"),
        PlainSize::Default,
        &tr!("transpose-relink"),
        &tr!("transpose-relink-hint", source = source.as_str()),
        |ui| {
            ui.label(tr!("transpose-relink-confirm", sheet = name.as_str()));
        },
    );
    match outcome {
        ConfirmOutcome::Confirm => actions.push(Action::ConfirmTransposeRelink(true)),
        ConfirmOutcome::Cancel => actions.push(Action::ConfirmTransposeRelink(false)),
        ConfirmOutcome::Open => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{
        cliquer, contexte, image, image_de, painted, texte_exact, textes,
    };
    use super::*;
    use stokesforge_core::{Column, ColumnMeta, Project, ValueBlock, TRANSPOSE_MARK};
    use stokesforge_session::Session;

    /// Le nom « Sheet1ᵀ » s'écrit avec un caractère que la police embarquée
    /// doit porter : sinon un carré vide au milieu de l'interface.
    #[test]
    fn la_police_embarquee_affiche_le_t_en_exposant() {
        let ctx = contexte();
        image_de(&ctx, egui::vec2(100.0, 100.0), Vec::new(), |_| {});
        // La police proportionnelle porte les noms de feuille (onglets,
        // gestionnaires, titres). La police à chasse fixe (console) ne le porte pas.
        let has = ctx
            .fonts_mut(|fonts| fonts.has_glyph(&egui::FontId::proportional(14.0), TRANSPOSE_MARK));
        assert!(has, "pas de glyphe pour {TRANSPOSE_MARK}");
    }

    /// Une feuille source à x' et sa transposée ; `source` est ensuite modifiée
    /// pour la périmer.
    fn banc() -> (Session, AppState, NodeId, NodeId, NodeId) {
        let mut project = Project::new("T").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let source = project.tree().children(book).unwrap()[0];
        let mut first = None;
        for i in 0..2 {
            let mut column = Column::from_f64(&[1.0, 2.0]);
            column.set_meta(ColumnMeta {
                x_prime: Some(f64::from(i)),
                ..ColumnMeta::default()
            });
            let id = project
                .add_column(source, None, Some(&format!("S{i}")), column)
                .unwrap();
            first.get_or_insert(id);
        }
        let sheet = project.transpose_rows(source, &[0]).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "t.stf".into()).unwrap();
        (session, state, source, sheet, first.unwrap())
    }

    #[test]
    fn la_bande_dit_l_origine_et_n_offre_recalculer_que_si_besoin() {
        let (session, mut state, _source, sheet, first) = banc();
        let ctx = contexte();
        let mut actions = Vec::new();
        let output = image_de(&ctx, egui::vec2(700.0, 60.0), Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert!(painted(&output, "Transposed from Sheet1"));
        assert!(!painted(&output, "Recalculate"), "à jour : pas de bouton");

        session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: first,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = textes(&shapes);
        texte_exact(&found, &tr!("transpose-state-stale"));
        let button = texte_exact(&found, &tr!("transpose-recalculate")).centre;
        cliquer(&ctx, button, egui::PointerButton::Primary, |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        assert_eq!(actions, vec![Action::RecalculateTranspose { sheet }]);
    }

    #[test]
    fn une_valeur_editee_offre_relier_de_nouveau() {
        let (session, mut state, _source, sheet, _first) = banc();
        let row = state.tree().children(sheet).unwrap()[1];
        session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: row,
                start: 0,
                values: ValueBlock::Float(vec![-1.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        let ctx = contexte();
        let mut actions = Vec::new();
        let shapes = image(&ctx, Vec::new(), |ui| {
            transpose_banner(ui, &state, sheet, &mut actions);
        });
        let found = textes(&shapes);
        texte_exact(&found, &tr!("transpose-state-modified"));
        texte_exact(&found, &tr!("transpose-relink"));
        assert!(found
            .iter()
            .all(|t| t.texte != tr!("transpose-recalculate")));
    }

    #[test]
    fn la_confirmation_ne_s_ouvre_que_sur_demande() {
        let (_session, mut state, _source, sheet, _first) = banc();
        let ctx = contexte();
        let mut actions = Vec::new();
        let output = image_de(&ctx, egui::vec2(900.0, 600.0), Vec::new(), |ui| {
            transpose_relink_window(ui.ctx(), &state, &mut actions);
        });
        assert!(!painted(&output, &tr!("transpose-relink-title")));
        // Édite une valeur liée puis demande la remise à jour.
        let row = state.tree().children(sheet).unwrap()[1];
        _session
            .handle()
            .execute(stokesforge_commands::Command::SetCells {
                column: row,
                start: 0,
                values: ValueBlock::Float(vec![-1.0]),
                expected_version: None,
            })
            .unwrap();
        state.refresh();
        state.dispatch(Action::RecalculateTranspose { sheet });
        let mut shown = false;
        for _ in 0..3 {
            let shapes = image(&ctx, Vec::new(), |ui| {
                transpose_relink_window(ui.ctx(), &state, &mut actions);
            });
            shown |= textes(&shapes)
                .iter()
                .any(|t| t.texte == tr!("transpose-relink-title"));
        }
        assert!(shown, "la confirmation s'ouvre");
    }
}
