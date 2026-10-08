//! Page des propriétés des séries (#177–#179) : la fenêtre à part, ouverte
//! par le double-clic sur une courbe ou sur sa ligne de l'Object Manager,
//! qui règle les séries d'un calque — liste à gauche, réglages à droite — et
//! leurs **groupes** : un groupe est l'ensemble des séries tracées ensemble
//! ([`stokesforge_plot::PanelSpec::push_series_batch`]) ou rassemblées à la
//! main ; il règle leur style commun et leur répartit un nuancier.
//!
//! La fenêtre est celle de [`super::plot_settings`] (même brouillon, même
//! validation, même macro d'historique) : [`PlotSettingsTab::Series`] ne
//! figure plus dans la bande d'onglets du graphe, elle choisit cette page.
//! Le style commun d'un groupe s'**écrit dans chaque série** au moment du
//! réglage : une série qu'on règle ensuite seule garde son exception, sans
//! drapeau à tenir.
//!
//! [`PlotSettingsTab::Series`]: crate::state::PlotSettingsTab::Series

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{gradient_colors, LineStyle, PanelSpec, SymbolShape, GRADIENT_PRESETS};

use crate::color_picker::{color_field, gradient_strip, ColorUsage, PickerEnv};
use crate::state::series_list_label;
use crate::theme::section_title;
use crate::tr;

use super::list_row::list_row;
use super::plot_settings::{line_style_label, series_editor, symbol_shape_label};

/// Largeur de la liste des séries, à gauche.
const LIST_WIDTH: f32 = 190.0;
/// Hauteur retirée à la liste pour la rangée Monter / Descendre.
const LIST_FOOTER: f32 = 34.0;
/// Arrêts de départ d'un nuancier personnalisé : du bleu au rouge.
const CUSTOM_STOPS: [&str; 2] = ["#0000ff", "#ff0000"];
/// Nombre d'échantillons de l'aperçu d'un nuancier.
const PREVIEW_STEPS: usize = 24;

/// Page des séries du calque `panel` : la liste, puis le groupe et les
/// réglages de la série `selected`. Rend `true` si le brouillon doit être
/// validé — voir la doc de tête de `plot_settings_window`.
pub(super) fn curve_settings_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    selected: &mut usize,
    env: &mut PickerEnv,
) -> bool {
    if panel.series.is_empty() {
        ui.weak(tr!("plot-series-empty"));
        return false;
    }
    *selected = (*selected).min(panel.series.len() - 1);
    let mut commit = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            commit |= series_list(ui, panel, tree, selected);
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("curve_settings_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    commit |= group_editor(ui, panel, *selected, env);
                    if let Some(series) = panel.series.get_mut(*selected) {
                        commit |= series_editor(ui, series, env);
                    }
                });
        });
    });
    commit
}

/// Le nom d'un groupe : « Group 2 ».
fn group_name(group: u32) -> String {
    tr!("curve-group-name", number = group)
}

/// La liste des séries (une ligne chacune, groupe en suffixe) et la rangée
/// qui change leur ordre. Rend `true` si l'ordre a changé.
fn series_list(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    selected: &mut usize,
) -> bool {
    egui::ScrollArea::vertical()
        .id_salt("curve_settings_list")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - LIST_FOOTER).max(60.0))
        .show(ui, |ui| {
            // #169 : une liste, donc des lignes de liste (`list_row`).
            for (index, series) in panel.series.iter().enumerate() {
                let name = series_list_label(tree, series);
                let label = match series.group {
                    Some(group) => tr!(
                        "curve-list-label",
                        name = name.as_str(),
                        group = group_name(group)
                    ),
                    None => name,
                };
                if ui
                    .push_id(index, |ui| {
                        list_row(ui, None, &label, *selected == index, false)
                    })
                    .inner
                    .clicked()
                {
                    *selected = index;
                }
            }
        });
    let mut commit = false;
    ui.horizontal(|ui| {
        let up = ui.add_enabled(
            *selected > 0,
            egui::Button::new(tr!("action-move-series-up")),
        );
        if up.clicked() {
            panel.series.swap(*selected, *selected - 1);
            *selected -= 1;
            commit = true;
        }
        let down = ui.add_enabled(
            *selected + 1 < panel.series.len(),
            egui::Button::new(tr!("action-move-series-down")),
        );
        if down.clicked() {
            panel.series.swap(*selected, *selected + 1);
            *selected += 1;
            commit = true;
        }
    });
    commit
}

/// Le groupe de la série `index` : son appartenance, puis, si elle en a un, le
/// style commun et le nuancier de tout le groupe.
fn group_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    index: usize,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    let group = panel.series[index].group;
    section_title(ui, tr!("curve-group-section"));
    let mut chosen = group;
    let mut new_group = false;
    ui.horizontal(|ui| {
        ui.label(tr!("curve-group-field"));
        let entry = |id: u32| {
            tr!(
                "curve-group-entry",
                group = group_name(id),
                count = panel.group_members(id).len()
            )
        };
        egui::ComboBox::from_id_salt("curve_group_choice")
            .selected_text(match group {
                Some(id) => entry(id),
                None => tr!("curve-group-none"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, tr!("curve-group-none"));
                for id in panel.group_ids() {
                    ui.selectable_value(&mut chosen, Some(id), entry(id));
                }
                if ui.selectable_label(false, tr!("curve-group-new")).clicked() {
                    new_group = true;
                }
            });
    });
    if new_group {
        chosen = Some(panel.next_group_id());
    }
    if chosen != group {
        panel.series[index].group = chosen;
        commit = true;
    }
    if let Some(group) = chosen {
        commit |= group_style_editor(ui, panel, index, group);
        commit |= gradient_editor(ui, panel, group, env);
    }
    commit
}

/// Style commun : trait et symbole, écrits dans **toutes** les séries du groupe
/// dès qu'on y touche (la couleur a son nuancier, plus bas). Les valeurs
/// affichées sont celles de la série `index`.
fn group_style_editor(ui: &mut egui::Ui, panel: &mut PanelSpec, index: usize, group: u32) -> bool {
    let members = panel.group_members(group);
    let reference = panel.series[index].clone();
    let mut commit = false;
    ui.weak(tr!("curve-group-style-note", count = members.len()));

    let mut style = reference.line.style;
    ui.horizontal(|ui| {
        ui.label(tr!("plot-series-line-style"));
        egui::ComboBox::from_id_salt("curve_group_line_style")
            .selected_text(line_style_label(style))
            .show_ui(ui, |ui| {
                for candidate in [
                    LineStyle::None,
                    LineStyle::Solid,
                    LineStyle::Dash,
                    LineStyle::Dot,
                    LineStyle::DashDot,
                ] {
                    ui.selectable_value(&mut style, candidate, line_style_label(candidate));
                }
            });
    });
    if style != reference.line.style {
        for &member in &members {
            panel.series[member].line.style = style;
        }
        commit = true;
    }

    let mut width = reference.line.width;
    let response = ui
        .horizontal(|ui| {
            ui.label(tr!("plot-series-line-width"));
            ui.add(
                egui::DragValue::new(&mut width)
                    .speed(0.1)
                    .range(0.0..=20.0),
            )
        })
        .inner;
    if response.changed() {
        for &member in &members {
            panel.series[member].line.width = width;
        }
    }
    commit |= response.drag_stopped() || response.lost_focus();

    let mut shape = reference.symbol.shape;
    ui.horizontal(|ui| {
        ui.label(tr!("plot-series-symbol-shape"));
        egui::ComboBox::from_id_salt("curve_group_symbol_shape")
            .selected_text(symbol_shape_label(shape))
            .show_ui(ui, |ui| {
                for candidate in [
                    SymbolShape::None,
                    SymbolShape::Circle,
                    SymbolShape::Square,
                    SymbolShape::Triangle,
                    SymbolShape::Diamond,
                    SymbolShape::Plus,
                    SymbolShape::Cross,
                ] {
                    ui.selectable_value(&mut shape, candidate, symbol_shape_label(candidate));
                }
            });
    });
    if shape != reference.symbol.shape {
        for &member in &members {
            panel.series[member].symbol.shape = shape;
        }
        commit = true;
    }

    let mut size = reference.symbol.size;
    let response = ui
        .horizontal(|ui| {
            ui.label(tr!("plot-series-symbol-size"));
            ui.add(egui::DragValue::new(&mut size).speed(0.1).range(0.0..=40.0))
        })
        .inner;
    if response.changed() {
        for &member in &members {
            panel.series[member].symbol.size = size;
        }
    }
    commit |= response.drag_stopped() || response.lost_focus();
    commit
}

/// Le nom d'un nuancier fourni.
fn gradient_label(id: &str) -> String {
    match id {
        "blue-red" => tr!("curve-gradient-blue-red"),
        "black-red" => tr!("curve-gradient-black-red"),
        "black-blue" => tr!("curve-gradient-black-blue"),
        "blue-yellow" => tr!("curve-gradient-blue-yellow"),
        "green-red" => tr!("curve-gradient-green-red"),
        "blue-white-red" => tr!("curve-gradient-blue-white-red"),
        "viridis" => tr!("curve-gradient-viridis"),
        "plasma" => tr!("curve-gradient-plasma"),
        _ => id.to_string(),
    }
}

/// Nuanciers (#178) : un nuancier fourni ou personnalisé (N couleurs), aperçu,
/// et « Appliquer », qui répartit les couleurs régulièrement sur les séries du
/// groupe, dans l'ordre de la liste. Le choix en cours n'est qu'un état de
/// fenêtre ; seules les couleurs appliquées entrent dans le graphe.
fn gradient_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    group: u32,
    env: &mut PickerEnv,
) -> bool {
    let choice_id = egui::Id::new("curve_gradient_choice");
    let stops_id = egui::Id::new("curve_gradient_stops");
    let custom = GRADIENT_PRESETS.len();
    let mut choice: usize = ui.data(|d| d.get_temp(choice_id)).unwrap_or(0);
    let mut stops: Vec<String> = ui
        .data(|d| d.get_temp(stops_id))
        .unwrap_or_else(|| CUSTOM_STOPS.iter().map(|s| (*s).to_string()).collect());

    section_title(ui, tr!("curve-gradient-section"));
    ui.horizontal(|ui| {
        ui.label(tr!("curve-gradient-field"));
        egui::ComboBox::from_id_salt("curve_gradient_combo")
            .selected_text(match GRADIENT_PRESETS.get(choice) {
                Some(preset) => gradient_label(preset.id),
                None => tr!("curve-gradient-custom"),
            })
            .show_ui(ui, |ui| {
                for (index, preset) in GRADIENT_PRESETS.iter().enumerate() {
                    ui.selectable_value(&mut choice, index, gradient_label(preset.id));
                }
                ui.selectable_value(&mut choice, custom, tr!("curve-gradient-custom"));
            });
    });
    if choice == custom {
        let mut remove = None;
        for (index, stop) in stops.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                color_field(
                    ui,
                    &tr!("curve-gradient-stop", number = (index + 1)),
                    stop,
                    ColorUsage::Curves,
                    None,
                    env,
                );
            });
        }
        ui.horizontal(|ui| {
            if ui.button(tr!("curve-gradient-add-stop")).clicked() {
                let last = stops.last().cloned().unwrap_or_default();
                stops.push(last);
            }
            if ui
                .add_enabled(
                    stops.len() > 2,
                    egui::Button::new(tr!("curve-gradient-remove-stop")),
                )
                .clicked()
            {
                remove = Some(stops.len() - 1);
            }
        });
        if let Some(index) = remove {
            stops.remove(index);
        }
    }
    let active: Vec<String> = match GRADIENT_PRESETS.get(choice) {
        Some(preset) => preset.stops.iter().map(|s| (*s).to_string()).collect(),
        None => stops.clone(),
    };
    gradient_strip(ui, &gradient_colors(&active, PREVIEW_STEPS));

    let mut commit = false;
    if ui
        .button(tr!("curve-gradient-apply"))
        .on_hover_text(tr!("curve-gradient-apply", "hint"))
        .clicked()
    {
        apply_gradient(panel, group, &active);
        commit = true;
    }
    ui.data_mut(|d| {
        d.insert_temp(choice_id, choice);
        d.insert_temp(stops_id, stops);
    });
    commit
}

/// Répartit le nuancier d'arrêts `stops` sur les séries du groupe `group`, dans
/// l'ordre du calque : trait et remplissage des symboles prennent la couleur.
/// Rien ne change si aucun arrêt n'est lisible.
fn apply_gradient(panel: &mut PanelSpec, group: u32, stops: &[String]) {
    let members = panel.group_members(group);
    for (&member, color) in members.iter().zip(gradient_colors(stops, members.len())) {
        panel.series[member].line.color = color.clone();
        panel.series[member].symbol.color = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::NodeId;
    use stokesforge_plot::SeriesSpec;

    fn panneau() -> PanelSpec {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(1))]);
        panel.push_series_batch(
            (2..=4)
                .map(|n| SeriesSpec::new(NodeId::from_raw(n)))
                .collect(),
        );
        panel
    }

    #[test]
    fn le_nuancier_colore_les_series_du_groupe_dans_l_ordre_et_elles_seules() {
        let mut panel = panneau();
        let seule = panel.series[0].clone();
        let stops = ["#000000".to_string(), "#ff0000".to_string()];
        apply_gradient(&mut panel, 1, &stops);
        assert_eq!(panel.series[0], seule, "la série hors groupe est intacte");
        let couleurs: Vec<&str> = panel.series[1..]
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(couleurs, ["#000000", "#800000", "#ff0000"]);
        assert!(panel.series[1..]
            .iter()
            .all(|s| s.symbol.color == s.line.color));
    }

    #[test]
    fn un_nuancier_illisible_ne_change_rien() {
        let mut panel = panneau();
        let avant = panel.clone();
        apply_gradient(&mut panel, 1, &["pas une couleur".to_string()]);
        assert_eq!(panel, avant);
    }
}
