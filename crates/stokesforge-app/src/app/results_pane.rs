//! Le volet Results d'un classeur (#181, maquette M7) : sous la grille, au
//! dessus des onglets de feuille, les valeurs des calculs « Keep in Results ».
//!
//! Replié par défaut (un seul bandeau avec le compte), absent tant que le
//! classeur n'a aucun résultat gardé. Déplié : la liste des calculs à gauche,
//! le détail à droite — une rangée par série, une colonne par valeur — avec
//! `Edit`, `Recalculate` et `Delete`. Le bouton `This sheet | Workbook` choisit
//! ce que la liste montre. L'état d'ouverture et le calcul choisi sont de
//! l'interface (mémoire egui) : ils ne vont ni dans le `.stf` ni dans l'historique.

use eframe::egui;
use stokesforge_core::NodeId;

use super::analysis_dialog::format_value;
use super::list_row::list_row;
use crate::analysis::{kind_title, series_label, value_label, value_unit, with_unit};
use crate::icons::{icon_button, Icon, IconRole};
use crate::state::{Action, AppState};
use crate::{theme, tr};

/// Hauteur du bandeau replié.
pub(super) const COLLAPSED_HEIGHT: f32 = 26.0;
/// Hauteur du volet déplié.
const EXPANDED_HEIGHT: f32 = 210.0;
/// Largeur de la liste des calculs.
const LIST_WIDTH: f32 = 240.0;

#[derive(Clone, Copy, Default)]
struct PaneState {
    open: bool,
    whole_book: bool,
    selected: Option<NodeId>,
}

fn pane_id(book: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_results_pane", book.raw()))
}

/// La hauteur que le volet prend dans le classeur : rien sans résultat gardé.
pub(super) fn pane_height(ui: &egui::Ui, state: &AppState, book: NodeId) -> f32 {
    if state.kept_calculations(book, None).is_empty() {
        return 0.0;
    }
    let open = ui
        .data(|d| d.get_temp::<PaneState>(pane_id(book)))
        .unwrap_or_default()
        .open;
    if open {
        EXPANDED_HEIGHT
    } else {
        COLLAPSED_HEIGHT
    }
}

/// Peint le volet dans `ui`, dont le rectangle a la hauteur de [`pane_height`].
pub(super) fn results_pane(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    book: NodeId,
    sheet: NodeId,
) {
    let id = pane_id(book);
    let mut pane = ui.data(|d| d.get_temp::<PaneState>(id)).unwrap_or_default();
    let total = state.kept_calculations(book, None).len();
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    ui.horizontal(|ui| {
        let (icon, tip) = if pane.open {
            (Icon::CaretDown, tr!("results-collapse"))
        } else {
            (Icon::CaretRight, tr!("results-expand"))
        };
        if icon_button(ui, icon, IconRole::Neutral, tip).clicked() {
            pane.open = !pane.open;
        }
        ui.label(theme::bold(tr!(
            "results-title",
            count = i64::try_from(total).unwrap_or(i64::MAX)
        )));
        if pane.open {
            ui.radio_value(&mut pane.whole_book, false, tr!("results-scope-sheet"));
            ui.radio_value(&mut pane.whole_book, true, tr!("results-scope-book"));
        }
    });
    if pane.open {
        let scope = (!pane.whole_book).then_some(sheet);
        let calculations = state.kept_calculations(book, scope);
        if !calculations.contains(&pane.selected.unwrap_or(NodeId::from_raw(0))) {
            pane.selected = calculations.first().copied();
        }
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(LIST_WIDTH, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("list"))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if calculations.is_empty() {
                                ui.label(egui::RichText::new(tr!("results-empty")).weak());
                            }
                            for calc in &calculations {
                                let mut title = state.calculation_title(*calc);
                                if state
                                    .tree()
                                    .calculation(*calc)
                                    .is_some_and(|c| c.stale.is_some())
                                {
                                    title = tr!("results-stale-suffix", title = title.as_str());
                                }
                                if list_row(ui, None, &title, pane.selected == Some(*calc), false)
                                    .clicked()
                                {
                                    pane.selected = Some(*calc);
                                }
                            }
                        });
                },
            );
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if let Some(calc) = pane.selected {
                        detail(ui, state, actions, id, calc);
                    }
                },
            );
        });
    }
    ui.data_mut(|d| d.insert_temp(id, pane));
}

/// Le détail d'un calcul : ses gestes, puis le tableau série × valeur.
fn detail(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    id: egui::Id,
    calc_id: NodeId,
) {
    let Some(calc) = state.tree().calculation(calc_id) else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(theme::bold(kind_title(calc.operation.kind())));
        if ui
            .button(tr!("action-calculation-edit"))
            .on_hover_text(tr!("action-calculation-edit", "hint"))
            .clicked()
        {
            actions.push(Action::EditCalculation(calc_id));
        }
        if ui
            .add_enabled(
                calc.stale.is_some(),
                egui::Button::new(tr!("action-calculation-recalculate")),
            )
            .on_hover_text(tr!("action-calculation-recalculate", "hint"))
            .on_disabled_hover_text(tr!("action-calculation-recalculate", "disabled"))
            .clicked()
        {
            actions.push(Action::RecalculateCalculations(vec![calc_id]));
        }
        if ui
            .button(tr!("action-calculation-delete"))
            .on_hover_text(tr!("action-calculation-delete", "hint"))
            .clicked()
        {
            actions.push(Action::RequestDeleteCalculation(calc_id));
        }
    });
    // Les colonnes du tableau : chaque (clé, rang) vu, dans l'ordre d'apparition.
    let mut columns: Vec<(String, Option<usize>)> = Vec::new();
    for result in &calc.results {
        for value in &result.values {
            let entry = (value.key.clone(), value.index);
            if !columns.contains(&entry) {
                columns.push(entry);
            }
        }
    }
    if columns.is_empty() {
        ui.label(egui::RichText::new(tr!("results-no-values")).weak());
        return;
    }
    egui::ScrollArea::both()
        .id_salt(id.with(("table", calc_id.raw())))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new(id.with(("grid", calc_id.raw())))
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for (key, index) in &columns {
                        ui.label(theme::bold(value_label(key, *index)));
                    }
                    ui.end_row();
                    for (rank, series) in calc.series.iter().enumerate() {
                        ui.label(series_label(state.tree(), series));
                        let values = calc.results.get(rank);
                        for (key, index) in &columns {
                            let text = values
                                .and_then(|r| {
                                    r.values.iter().find(|v| &v.key == key && v.index == *index)
                                })
                                .map(|v| {
                                    with_unit(
                                        format_value(v.value, v.error),
                                        &value_unit(state.tree(), series, key),
                                    )
                                })
                                .unwrap_or_default();
                            ui.label(text);
                        }
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{cliquer, contexte, image_de, painted, texte_exact, textes};
    use super::*;
    use crate::analysis::Preset;
    use stokesforge_core::{Column, ColumnMeta, ColumnRole, Project};
    use stokesforge_session::Session;

    /// Un classeur avec une intégrale gardée dans les résultats.
    fn banc() -> (Session, AppState, NodeId, NodeId) {
        let mut project = Project::new("Résultats").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let xs: Vec<f64> = (0..11).map(f64::from).collect();
        let x = project
            .add_column(sheet, None, Some("A"), Column::from_f64(&xs))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("B"), Column::from_f64(&xs))
            .unwrap();
        project
            .set_column_meta(
                x,
                ColumnMeta {
                    role: ColumnRole::X,
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".into()).unwrap();
        state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet,
            columns: vec![x, y],
        });
        {
            let form = state.analysis_form_mut().unwrap();
            form.keep_results = true;
            form.setup.integrate.cumulative = true;
        }
        state.dispatch(Action::ApplyAnalysis);
        (session, state, book, sheet)
    }

    fn frame(
        ctx: &egui::Context,
        state: &AppState,
        book: NodeId,
        sheet: NodeId,
        actions: &mut Vec<Action>,
    ) -> egui::FullOutput {
        image_de(ctx, egui::vec2(900.0, 400.0), Vec::new(), |ui| {
            results_pane(ui, state, actions, book, sheet);
        })
    }

    #[test]
    fn le_volet_est_replie_avec_son_compte_puis_se_deplie_en_tableau() {
        let ctx = contexte();
        let (session, state, book, sheet) = banc();
        assert_eq!(state.kept_calculations(book, None).len(), 1);
        assert_eq!(state.kept_calculations(book, Some(sheet)).len(), 1);
        assert_eq!(
            state
                .kept_calculations(book, Some(NodeId::from_raw(999_999)))
                .len(),
            0
        );
        let mut actions = Vec::new();
        frame(&ctx, &state, book, sheet, &mut actions);
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        assert!(painted(&output, &tr!("results-title", count = 1)));
        assert!(!painted(&output, &tr!("results-scope-book")), "replié");

        // Le bouton de dépliage est à gauche du titre.
        let title = texte_exact(&textes(&output.shapes), &tr!("results-title", count = 1)).centre;
        cliquer(
            &ctx,
            egui::pos2(title.x - 40.0, title.y),
            egui::PointerButton::Primary,
            |ui| results_pane(ui, &state, &mut actions, book, sheet),
        );
        let output = frame(&ctx, &state, book, sheet, &mut actions);
        assert!(painted(&output, &tr!("results-scope-book")), "déplié");
        assert!(painted(&output, &value_label("area.total", None)));
        let edit = texte_exact(&textes(&output.shapes), &tr!("action-calculation-edit")).centre;
        cliquer(&ctx, edit, egui::PointerButton::Primary, |ui| {
            results_pane(ui, &state, &mut actions, book, sheet)
        });
        let calc = *state.tree().calculations().keys().next().unwrap();
        assert!(
            actions.contains(&Action::EditCalculation(calc)),
            "{actions:?}"
        );
        session.shutdown().unwrap();
    }
}
