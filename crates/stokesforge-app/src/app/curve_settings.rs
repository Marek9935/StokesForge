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
//! Une série d'un groupe se règle selon une **portée** choisie en clair
//! ([`EditScope`]) : « cette série » (rien ne touche les autres, c'est
//! l'exception) ou « tout le groupe » (style du trait et du symbole recopiés
//! dans chaque membre, nuancier appliqué ; les réglages propres à une série —
//! nom, type, visibilité, couleurs — sont grisés avec une info-bulle). Le
//! style commun s'**écrit dans chaque série** : aucun drapeau d'exception à
//! tenir, et « Quitter le groupe » rend une série à elle-même.
//!
//! Nuanciers (#178) : fournis, enregistrés dans le projet ou pour
//! l'utilisateur (même motif que les couleurs, `SavedColors::gradients`),
//! ou personnalisés ; ils se suppriment depuis la même liste.
//!
//! [`PlotSettingsTab::Series`]: crate::state::PlotSettingsTab::Series

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{gradient_colors, PanelSpec, SavedColors, SeriesSpec, GRADIENT_PRESETS};

use crate::color_picker::{color_field, gradient_strip, ColorUsage, PickerEnv};
use crate::state::{series_list_label, Action, SaveTarget};
use crate::theme::section_title;
use crate::tr;

use super::list_row::list_row;
use super::plot_settings::series_editor;

/// Largeur de la liste des séries, à gauche.
const LIST_WIDTH: f32 = 190.0;
/// Hauteur retirée à la liste pour la rangée Monter / Descendre.
const LIST_FOOTER: f32 = 34.0;
/// Arrêts de départ d'un nuancier personnalisé : du bleu au rouge.
const CUSTOM_STOPS: [&str; 2] = ["#0000ff", "#ff0000"];
/// Nombre d'échantillons de l'aperçu d'un nuancier.
const PREVIEW_STEPS: usize = 24;

/// Sur quoi portent les réglages d'une série qui est dans un groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditScope {
    /// Cette série seulement : les autres membres ne bougent pas.
    Series,
    /// Tout le groupe.
    Group,
}

/// Le nuancier choisi dans la liste (état de fenêtre, jamais écrit au modèle).
#[derive(Debug, Clone, PartialEq, Eq)]
enum GradientChoice {
    /// Un nuancier fourni : rang dans `GRADIENT_PRESETS`.
    Preset(usize),
    /// Un nuancier enregistré : où et sous quel nom.
    Saved(SaveTarget, String),
    /// Les arrêts saisis dans la fenêtre.
    Custom,
}

/// Page des séries du calque `panel` : la liste, puis le groupe et les
/// réglages de la série `selected`. Rend `true` si le brouillon doit être
/// validé — voir la doc de tête de `plot_settings_window`. Les enregistrements
/// et suppressions de nuanciers sortent en `actions`.
pub(super) fn curve_settings_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    selected: &mut usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
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
                    commit |= series_page(ui, panel, *selected, env, actions);
                });
        });
    });
    commit
}

/// Tout le groupe : recopie dans les membres de `group` ce que la série `index`
/// vient de changer dans son style de trait et de symbole par rapport à
/// `before` (seulement ce qui a changé : les exceptions déjà posées sur les
/// autres champs sont gardées).
fn propagate_style(panel: &mut PanelSpec, group: u32, before: &SeriesSpec, index: usize) {
    let edited = panel.series[index].clone();
    for member in panel.group_members(group) {
        let other = &mut panel.series[member];
        if edited.line.style != before.line.style {
            other.line.style = edited.line.style;
        }
        if edited.line.width != before.line.width {
            other.line.width = edited.line.width;
        }
        if edited.symbol.shape != before.symbol.shape {
            other.symbol.shape = edited.symbol.shape;
        }
        if edited.symbol.size != before.symbol.size {
            other.symbol.size = edited.symbol.size;
        }
    }
}

/// Le nom d'un groupe : « Group 2 ».
pub(crate) fn group_name(group: u32) -> String {
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

/// Le côté droit : groupe, portée, nuancier, puis les réglages de la série.
fn series_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    index: usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let scope_id = egui::Id::new("curve_edit_scope");
    let mut scope = ui
        .data(|d| d.get_temp(scope_id))
        .unwrap_or(EditScope::Group);
    let mut commit = membership_editor(ui, panel, index, &mut scope);
    let group = panel.series[index].group;
    let whole_group = group.is_some() && scope == EditScope::Group;
    commit |= gradient_editor(ui, panel, group, whole_group, env, actions);

    let before = panel.series[index].clone();
    commit |= series_editor(ui, &mut panel.series[index], env, whole_group);
    if let (true, Some(group)) = (whole_group, group) {
        propagate_style(panel, group, &before, index);
    }
    ui.data_mut(|d| d.insert_temp(scope_id, scope));
    commit
}

/// Appartenance de la série `index` : liste déroulante, « Quitter le groupe »,
/// et, si elle est dans un groupe, le choix de portée avec sa phrase
/// d'explication.
fn membership_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    index: usize,
    scope: &mut EditScope,
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
        if ui
            .add_enabled(group.is_some(), egui::Button::new(tr!("curve-group-leave")))
            .on_hover_text(tr!("curve-group-leave", "hint"))
            .on_disabled_hover_text(tr!("curve-group-leave", "disabled"))
            .clicked()
        {
            chosen = None;
        }
    });
    if new_group {
        chosen = Some(panel.next_group_id());
    }
    if chosen != group {
        panel.series[index].group = chosen;
        commit = true;
    }
    if let Some(group) = chosen {
        let count = panel.group_members(group).len();
        ui.horizontal(|ui| {
            ui.label(tr!("curve-scope-field"));
            ui.radio_value(scope, EditScope::Series, tr!("curve-scope-series"));
            ui.radio_value(
                scope,
                EditScope::Group,
                tr!("curve-scope-group", count = count),
            );
        });
        ui.weak(match scope {
            EditScope::Group => tr!("curve-scope-group-note"),
            EditScope::Series => tr!("curve-scope-series-note"),
        });
    }
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

/// Les couleurs enregistrées de la cible `target`.
fn saved_of(env: &PickerEnv, target: SaveTarget) -> &SavedColors {
    match target {
        SaveTarget::Project => &env.project,
        SaveTarget::User => &env.user,
    }
}

/// Les arrêts du nuancier choisi ; `custom` pour un nuancier personnalisé (ou
/// un nuancier enregistré qui a disparu).
fn choice_stops(choice: &GradientChoice, env: &PickerEnv, custom: &[String]) -> Vec<String> {
    match choice {
        GradientChoice::Preset(index) => GRADIENT_PRESETS.get(*index).map_or_else(
            || custom.to_vec(),
            |preset| preset.stops.iter().map(|s| (*s).to_string()).collect(),
        ),
        GradientChoice::Saved(target, name) => saved_of(env, *target)
            .gradients
            .iter()
            .find(|gradient| &gradient.name == name)
            .map_or_else(|| custom.to_vec(), |gradient| gradient.stops.clone()),
        GradientChoice::Custom => custom.to_vec(),
    }
}

/// Nuanciers (#178) : choisir (fourni, enregistré, personnalisé), voir l'aperçu,
/// enregistrer ou supprimer un nuancier enregistré, et « Appliquer au
/// groupe », qui répartit les couleurs régulièrement sur les séries du groupe
/// dans l'ordre de la liste. Grisé tant que la portée n'est pas « tout le
/// groupe » (`enabled`), avec l'info-bulle qui le dit. Le choix en cours n'est
/// qu'un état de fenêtre ; seules les couleurs appliquées entrent dans le
/// graphe.
fn gradient_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    group: Option<u32>,
    enabled: bool,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let choice_id = egui::Id::new("curve_gradient_choice");
    let stops_id = egui::Id::new("curve_gradient_stops");
    let name_id = egui::Id::new("curve_gradient_name");
    let target_id = egui::Id::new("curve_gradient_target");
    let mut choice: GradientChoice = ui
        .data(|d| d.get_temp(choice_id))
        .unwrap_or(GradientChoice::Preset(0));
    let mut custom: Vec<String> = ui
        .data(|d| d.get_temp(stops_id))
        .unwrap_or_else(|| CUSTOM_STOPS.iter().map(|s| (*s).to_string()).collect());
    let mut name: String = ui.data(|d| d.get_temp(name_id)).unwrap_or_default();
    let mut target: SaveTarget = ui.data(|d| d.get_temp(target_id)).unwrap_or_default();

    section_title(ui, tr!("curve-gradient-section"));
    let mut commit = false;
    let out = ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label(tr!("curve-gradient-field"));
            let selected_text = match &choice {
                GradientChoice::Preset(index) => GRADIENT_PRESETS
                    .get(*index)
                    .map_or_else(String::new, |preset| gradient_label(preset.id)),
                GradientChoice::Saved(target, name) => tr!(
                    "curve-gradient-saved",
                    name = name.as_str(),
                    target = target.label()
                ),
                GradientChoice::Custom => tr!("curve-gradient-custom"),
            };
            egui::ComboBox::from_id_salt("curve_gradient_combo")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (index, preset) in GRADIENT_PRESETS.iter().enumerate() {
                        ui.selectable_value(
                            &mut choice,
                            GradientChoice::Preset(index),
                            gradient_label(preset.id),
                        );
                    }
                    for saved_target in [SaveTarget::Project, SaveTarget::User] {
                        for gradient in &saved_of(env, saved_target).gradients {
                            ui.selectable_value(
                                &mut choice,
                                GradientChoice::Saved(saved_target, gradient.name.clone()),
                                tr!(
                                    "curve-gradient-saved",
                                    name = gradient.name.as_str(),
                                    target = saved_target.label()
                                ),
                            );
                        }
                    }
                    ui.selectable_value(
                        &mut choice,
                        GradientChoice::Custom,
                        tr!("curve-gradient-custom"),
                    );
                });
            let saved = match &choice {
                GradientChoice::Saved(target, name) => Some((*target, name.clone())),
                _ => None,
            };
            if ui
                .add_enabled(
                    saved.is_some(),
                    egui::Button::new(tr!("curve-gradient-delete")),
                )
                .on_hover_text(tr!("curve-gradient-delete", "hint"))
                .on_disabled_hover_text(tr!("curve-gradient-delete", "disabled"))
                .clicked()
            {
                if let Some((target, name)) = saved {
                    actions.push(Action::RemoveSavedGradient { name, target });
                    choice = GradientChoice::Preset(0);
                }
            }
        });
        if choice == GradientChoice::Custom {
            let mut remove = None;
            for (index, stop) in custom.iter_mut().enumerate() {
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
                    let last = custom.last().cloned().unwrap_or_default();
                    custom.push(last);
                }
                if ui
                    .add_enabled(
                        custom.len() > 2,
                        egui::Button::new(tr!("curve-gradient-remove-stop")),
                    )
                    .clicked()
                {
                    remove = Some(custom.len() - 1);
                }
            });
            if let Some(index) = remove {
                custom.remove(index);
            }
            ui.horizontal(|ui| {
                ui.label(tr!("curve-gradient-name-field"));
                ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text(tr!("curve-gradient-name-field", "hint"))
                        .desired_width(120.0),
                );
                egui::ComboBox::from_id_salt("curve_gradient_target")
                    .selected_text(target.label())
                    .show_ui(ui, |ui| {
                        for candidate in [SaveTarget::Project, SaveTarget::User] {
                            ui.selectable_value(&mut target, candidate, candidate.label());
                        }
                    });
                if ui
                    .add_enabled(
                        !name.trim().is_empty(),
                        egui::Button::new(tr!("curve-gradient-save")),
                    )
                    .on_hover_text(tr!("curve-gradient-save", "hint"))
                    .on_disabled_hover_text(tr!("curve-gradient-save", "disabled"))
                    .clicked()
                {
                    actions.push(Action::SaveGradient {
                        name: name.trim().to_string(),
                        stops: custom.clone(),
                        target,
                    });
                    choice = GradientChoice::Saved(target, name.trim().to_string());
                }
            });
        }
        let active = choice_stops(&choice, env, &custom);
        gradient_strip(ui, &gradient_colors(&active, PREVIEW_STEPS));
        if ui
            .add_enabled(
                group.is_some(),
                egui::Button::new(tr!("curve-gradient-apply")),
            )
            .on_hover_text(tr!("curve-gradient-apply", "hint"))
            .clicked()
        {
            if let Some(group) = group {
                apply_gradient(panel, group, &active);
                commit = true;
            }
        }
    });
    if !enabled {
        let reason = if group.is_some() {
            tr!("curve-gradient-locked")
        } else {
            tr!("curve-gradient-needs-group")
        };
        out.response.on_hover_text(reason);
    }
    ui.data_mut(|d| {
        d.insert_temp(choice_id, choice);
        d.insert_temp(stops_id, custom);
        d.insert_temp(name_id, name);
        d.insert_temp(target_id, target);
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
    use stokesforge_plot::LineStyle;

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
    fn le_style_modifie_se_recopie_dans_le_groupe_sans_ecraser_le_reste() {
        let mut panel = panneau();
        panel.series[2].line.width = 4.0; // exception posée à la main
        let avant = panel.series[1].clone();
        panel.series[1].line.style = LineStyle::Dash;
        panel.series[1].symbol.size = 11.0;
        propagate_style(&mut panel, 1, &avant, 1);
        for member in [1, 2, 3] {
            assert_eq!(panel.series[member].line.style, LineStyle::Dash);
            assert_eq!(panel.series[member].symbol.size, 11.0);
        }
        assert_eq!(panel.series[2].line.width, 4.0, "champ non touché : gardé");
        assert_eq!(panel.series[0].line.style, LineStyle::Solid, "hors groupe");
        assert_eq!(
            panel.series[0].symbol.size,
            SeriesSpec::new(NodeId::from_raw(1)).symbol.size
        );
    }

    #[test]
    fn un_nuancier_illisible_ne_change_rien() {
        let mut panel = panneau();
        let avant = panel.clone();
        apply_gradient(&mut panel, 1, &["pas une couleur".to_string()]);
        assert_eq!(panel, avant);
    }
}
