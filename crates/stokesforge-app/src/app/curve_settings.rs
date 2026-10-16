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
//! [`PlotSettingsTab::Series`]: stokesforge_app_state::state::PlotSettingsTab::Series

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{
    gradient_colors, PanelSpec, SavedColors, SeriesSlot, SeriesSpec, GRADIENT_PRESETS,
};

use crate::color_picker::{color_field, gradient_strip, gradient_swatch, ColorUsage, PickerEnv};
use crate::icons::ICON_SIZE;
use crate::theme;
use stokesforge_app_state::object_drag::{step_panel, DragItem, DropOver};
use stokesforge_app_state::object_manager::group_title;
use stokesforge_app_state::state::{series_list_label, Action, SaveTarget};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::form::{choice, form, ChoiceOption, Form};
use super::hover_help::help_text;
use super::list_row::list_row_draggable;
use super::object_dnd::{self, Hover};
use super::plot_settings::series_editor;
use super::reorder_list::reorder_row_with;

/// Largeur de la liste des séries, à gauche.
const LIST_WIDTH: f32 = 190.0;
/// Hauteur retirée à la liste pour la rangée Monter / Descendre.
const LIST_FOOTER: f32 = 34.0;
/// Arrêts de départ d'un nuancier personnalisé : du bleu au rouge.
const CUSTOM_STOPS: [&str; 2] = ["#0000ff", "#ff0000"];
/// Nombre d'échantillons de l'aperçu d'un nuancier.
const PREVIEW_STEPS: usize = 24;
/// Nombre d'échantillons de la pastille d'une entrée de la liste.
const ENTRY_STEPS: usize = 12;

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
    layer: usize,
    tree: &Tree,
    selected: &mut usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    if panel.series.is_empty() {
        empty_state(ui, tr!("plot-series-empty"));
        return false;
    }
    *selected = (*selected).min(panel.series.len() - 1);
    let mut commit = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            commit |= series_list(ui, panel, layer, tree, selected);
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("curve_settings_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    commit |= series_page(ui, panel, layer, selected, env, actions);
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

/// Le glisser-déposer de la liste : l'élément saisi (le brouillon se modifie
/// tout de suite, sans aperçu) et la première ligne survolée de l'image.
struct ListDnd {
    layer: usize,
    item: Option<DragItem>,
    hover: Hover,
}

/// Une ligne de série de la liste ; rend le centre vertical de la ligne. Sa
/// poignée la glisse : dans son groupe, vers un autre groupe, hors du groupe,
/// ou parmi les séries seules.
fn series_row(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    index: usize,
    selected: &mut usize,
    dnd: &ListDnd,
    commit: &mut bool,
) -> f32 {
    let label = series_list_label(tree, &panel.series[index]);
    let row = ui
        .push_id(index, |ui| {
            ui.horizontal(|ui| {
                let mut clicked = false;
                let outcome = reorder_row_with(
                    ui,
                    ui.id().with("row"),
                    matches!(
                        dnd.item,
                        Some(DragItem::Series { current, .. }) if current == index
                    ),
                    |ui| {
                        // « Show series » : la case de la série, en tête de ligne.
                        let mut visible = panel.series[index].visible;
                        if ui
                            .add(egui::Checkbox::new(&mut visible, ""))
                            .on_hover_text(tr!("plot-series-show"))
                            .changed()
                        {
                            panel.series[index].visible = visible;
                            *commit = true;
                        }
                        // Toute la ligne, hors case, se clique et se glisse.
                        let response = list_row_draggable(ui, &label, *selected == index);
                        clicked = response.clicked();
                        Some(response)
                    },
                );
                (outcome, clicked)
            })
        })
        .inner;
    let (outcome, clicked) = row.inner;
    if clicked {
        *selected = index;
    }
    if outcome.grabbed {
        *selected = index;
        object_dnd::begin(
            ui.ctx(),
            None,
            DragItem::Series {
                panel: dnd.layer,
                origin: index,
                current: index,
            },
        );
    }
    if let Some(rect) = outcome.hover {
        dnd.hover.offer(DropOver::Series {
            panel: dnd.layer,
            index,
            after: object_dnd::pointer_after(ui, rect),
        });
    }
    row.response.rect.center().y
}

/// La liste des séries, rangée comme l'Object Manager (#197) : une série seule
/// est une ligne, un groupe est un nœud repliable (réduire ne masque aucune
/// courbe) dont les membres sont en branche. Dessous, la rangée qui change
/// l'ordre. Rend `true` si l'ordre a changé.
fn series_list(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    selected: &mut usize,
) -> bool {
    let mut commit = false;
    let dnd = ListDnd {
        layer,
        item: object_dnd::live(ui.ctx())
            .filter(|live| live.graph.is_none())
            .map(|live| live.item),
        hover: Hover::default(),
    };
    egui::ScrollArea::vertical()
        .id_salt("curve_settings_list")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - LIST_FOOTER).max(60.0))
        .show(ui, |ui| {
            crate::tree_lines::begin_tree(ui);
            // #169 : une liste, donc des lignes de liste (`list_row`).
            for slot in panel.slots() {
                match slot {
                    SeriesSlot::Single(index) => {
                        series_row(ui, panel, tree, index, selected, &dnd, &mut commit);
                    }
                    SeriesSlot::Group { id, members } => {
                        let state = ui.make_persistent_id(("curve_list_group", layer, id));
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            state,
                            true,
                        )
                        .show_header(ui, |ui| {
                            // Comme dans l'Object Manager (#208) : l'en-tête est
                            // surligné quand le groupe est ce qu'on règle, et un
                            // clic choisit le groupe (sa première série, portée
                            // « tout le groupe »).
                            let scope_id = egui::Id::new("curve_edit_scope");
                            let scope = ui
                                .data(|d| d.get_temp(scope_id))
                                .unwrap_or(EditScope::Group);
                            let on_group = scope == EditScope::Group && members.contains(selected);
                            let title = group_title(panel, id);
                            let mut clicked = false;
                            let outcome = reorder_row_with(
                                ui,
                                ui.id().with("row"),
                                matches!(
                                    dnd.item,
                                    Some(DragItem::Group { group, .. }) if group == id
                                ),
                                |ui| {
                                    let response = list_row_draggable(ui, &title, on_group);
                                    clicked = response.clicked();
                                    Some(response)
                                },
                            );
                            if clicked || outcome.grabbed {
                                if let Some(first) = members.first() {
                                    *selected = *first;
                                }
                                ui.data_mut(|d| d.insert_temp(scope_id, EditScope::Group));
                            }
                            if outcome.grabbed {
                                object_dnd::begin(
                                    ui.ctx(),
                                    None,
                                    DragItem::Group {
                                        panel: layer,
                                        group: id,
                                    },
                                );
                            }
                            if let Some(rect) = outcome.hover {
                                dnd.hover.offer(DropOver::Header {
                                    panel: layer,
                                    group: id,
                                    after: object_dnd::pointer_after(ui, rect),
                                });
                            }
                        })
                        .body(|ui| {
                            crate::tree_lines::tree_children(ui, |ui, branch| {
                                for &member in &members {
                                    let y = series_row(
                                        ui,
                                        panel,
                                        tree,
                                        member,
                                        selected,
                                        &dnd,
                                        &mut commit,
                                    );
                                    branch.mark(y);
                                }
                            })
                        });
                    }
                }
            }
            // La sortie d'un groupe qui finit la liste, et le dépôt en fin de
            // liste : la zone n'existe que pendant un glisser.
            if dnd.item.is_some() {
                let row = ui.horizontal(|ui| ui.weak(tr!("object-drag-end"))).response;
                if ui.rect_contains_pointer(row.rect) {
                    dnd.hover.offer(DropOver::End { panel: layer });
                }
            }
        });
    if let (Some(item), Some(over)) = (dnd.item, dnd.hover.take()) {
        if let Some((item, order)) = step_panel(panel, item, over) {
            object_dnd::set_item(ui.ctx(), item);
            // La série réglée suit sa place dans le nouvel ordre.
            if let Some(rank) = order.iter().position(|old| old == selected) {
                *selected = rank;
            }
            commit = true;
        }
    }
    ui.horizontal(|ui| {
        for (label, up) in [
            (tr!("action-move-series-up"), true),
            (tr!("action-move-series-down"), false),
        ] {
            let movable = panel.can_move_series(*selected, up);
            if ui
                .add_enabled(movable, egui::Button::new(label))
                .on_disabled_hover_text(if up {
                    tr!("disabled-move-first")
                } else {
                    tr!("disabled-move-last")
                })
                .clicked()
            {
                if let Some(index) = panel.move_series(*selected, up) {
                    *selected = index;
                    commit = true;
                }
            }
        }
    });
    commit
}

/// Le côté droit : groupe, portée, nuancier, puis les réglages de la série.
fn series_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    index: &mut usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let scope_id = egui::Id::new("curve_edit_scope");
    let mut scope = ui
        .data(|d| d.get_temp(scope_id))
        .unwrap_or(EditScope::Group);
    let mut commit = membership_editor(ui, panel, layer, index, &mut scope);
    ui.data_mut(|d| d.insert_temp(scope_id, scope));
    // Supprimer un groupe peut vider le calque : rien à régler ensuite.
    if panel.series.is_empty() {
        return commit;
    }
    *index = (*index).min(panel.series.len() - 1);
    let group = panel.series[*index].group;
    let whole_group = group.is_some() && scope == EditScope::Group;
    // Hors d'un groupe, il n'y a rien à répartir : pas de nuancier (#213).
    if let Some(group) = group {
        commit |= gradient_editor(ui, panel, group, whole_group, env, actions);
    }

    let before = panel.series[*index].clone();
    commit |= series_editor(ui, &mut panel.series[*index], env, whole_group);
    if let (true, Some(group)) = (whole_group, group) {
        propagate_style(panel, group, &before, *index);
    }
    commit
}

/// Appartenance de la série `index` : liste déroulante, « Quitter le groupe »,
/// puis, si elle est dans un groupe, ses gestes (nom, ordre, dissoudre,
/// supprimer) et le choix de portée avec sa phrase d'explication. Un groupe
/// reste contigu (#197) : changer d'appartenance déplace la série, `index`
/// la suit.
fn membership_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    index: &mut usize,
    scope: &mut EditScope,
) -> bool {
    let mut commit = false;
    let group = panel.series[*index].group;
    let mut chosen = group;
    let mut new_group = false;
    form(ui, "curve_membership", |f| {
        f.section(tr!("curve-group-section"));
        f.row(tr!("curve-group-field"), |ui| {
            let entry = |id: u32| {
                tr!(
                    "curve-group-entry",
                    group = group_title(panel, id),
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
            if let Some(moved) = panel.set_series_group(*index, chosen) {
                *index = moved;
                commit = true;
            }
        }
        if let Some(group) = panel.series[*index].group {
            commit |= group_editor(f, panel, layer, group, index);
            let count = panel.group_members(group).len();
            f.row(tr!("curve-scope-field"), |ui| {
                // Les deux options sont l'une sous l'autre : le « ? » se pose
                // au niveau de la dernière, « Whole group ».
                ui.horizontal(|ui| {
                    let options = ui.vertical(|ui| {
                        choice(
                            ui,
                            "curve_scope",
                            scope,
                            &[
                                ChoiceOption::new(EditScope::Series, tr!("curve-scope-series")),
                                ChoiceOption::new(
                                    EditScope::Group,
                                    tr!("curve-scope-group", count = count),
                                ),
                            ],
                        );
                    });
                    // Ce qui s'applique selon la portée : sous le « ? » (#213),
                    // plus en note grisée.
                    let text = match scope {
                        EditScope::Group => tr!("curve-scope-group-note"),
                        EditScope::Series => tr!("curve-scope-series-note"),
                    };
                    let rect = options.response.rect;
                    let at = egui::pos2(
                        ui.cursor().min.x,
                        rect.bottom() - ICON_SIZE.max(ui.spacing().interact_size.y),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            at,
                            egui::vec2(
                                ICON_SIZE * 2.0,
                                ICON_SIZE.max(ui.spacing().interact_size.y),
                            ),
                        )),
                        |ui| {
                            ui.centered_and_justified(|ui| {
                                help_text(ui, text);
                            });
                        },
                    );
                });
            });
        }
        commit
    })
}

/// Les gestes du groupe `group` (#197) : son nom, son rang dans l'ordre de
/// tracé, sa dissolution (les séries restent) et sa suppression avec ses
/// séries. `index` est la série réglée : il suit son contenu quand le groupe
/// change de rang, et retombe sur la place du groupe quand il est supprimé.
/// Rend `true` si le groupe a changé.
fn group_editor(
    f: &mut Form<'_>,
    panel: &mut PanelSpec,
    layer: usize,
    group: u32,
    index: &mut usize,
) -> bool {
    let mut commit = false;
    // Le texte en cours de frappe ne vit que le temps du focus : le reste du
    // temps, le champ relit le modèle (nom changé ailleurs, autre groupe).
    let name_id = egui::Id::new(("curve_group_name", layer, group));
    let mut name: String = f
        .ui()
        .data(|d| d.get_temp(name_id))
        .unwrap_or_else(|| panel.group_label(group).unwrap_or_default().to_string());
    f.row(tr!("curve-group-name-field"), |ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text(tr!("curve-group-name", number = group))
                .desired_width(theme::FieldWidth::Medium.points(ui)),
        );
        help_text(ui, tr!("curve-group-name-field", "hint"));
        if field.lost_focus() && panel.rename_group(group, &name) {
            commit = true;
        }
        if field.has_focus() {
            ui.data_mut(|d| d.insert_temp(name_id, name.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(name_id));
        }
    });
    f.field(|ui| {
        for up in [true, false] {
            let (label, hint) = if up {
                (
                    tr!("curve-group-move-up"),
                    tr!("curve-group-move-up", "hint"),
                )
            } else {
                (
                    tr!("curve-group-move-down"),
                    tr!("curve-group-move-down", "hint"),
                )
            };
            if ui
                .add_enabled(panel.can_move_group(group, up), egui::Button::new(label))
                .on_hover_text(hint)
                .on_disabled_hover_text(if up {
                    tr!("disabled-move-first")
                } else {
                    tr!("disabled-move-last")
                })
                .clicked()
            {
                let place = panel
                    .group_members(group)
                    .iter()
                    .position(|member| member == index);
                if panel.move_group(group, up) {
                    if let Some(member) =
                        place.and_then(|p| panel.group_members(group).get(p).copied())
                    {
                        *index = member;
                    }
                    commit = true;
                }
            }
        }
    });
    f.field(|ui| {
        if ui
            .button(tr!("curve-group-ungroup"))
            .on_hover_text(tr!("curve-group-ungroup", "hint"))
            .clicked()
            && panel.dissolve_group(group)
        {
            commit = true;
        }
        if ui
            .button(tr!("curve-group-delete"))
            .on_hover_text(tr!("curve-group-delete", "hint"))
            .clicked()
        {
            let first = panel.group_members(group).first().copied().unwrap_or(0);
            if panel.remove_group(group) {
                *index = first.min(panel.series.len().saturating_sub(1));
                commit = true;
            }
        }
    });
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

/// Le choix qui correspond aux `stops` retenus par le groupe (#213) : celui que
/// la fenêtre se rappelle s'il donne ces arrêts, sinon un nuancier fourni ou
/// enregistré qui les porte, sinon le nuancier personnalisé.
fn identify_choice(
    stops: &[String],
    remembered: Option<&GradientChoice>,
    env: &PickerEnv,
    custom: &[String],
) -> GradientChoice {
    let same = |other: &[String]| {
        other.len() == stops.len()
            && other
                .iter()
                .zip(stops)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    if let Some(choice) = remembered {
        if same(&choice_stops(choice, env, custom)) {
            return choice.clone();
        }
    }
    if let Some(index) = GRADIENT_PRESETS.iter().position(|preset| {
        let preset: Vec<String> = preset.stops.iter().map(|s| (*s).to_string()).collect();
        same(&preset)
    }) {
        return GradientChoice::Preset(index);
    }
    for target in [SaveTarget::Project, SaveTarget::User] {
        if let Some(gradient) = saved_of(env, target)
            .gradients
            .iter()
            .find(|gradient| same(&gradient.stops))
        {
            return GradientChoice::Saved(target, gradient.name.clone());
        }
    }
    GradientChoice::Custom
}

/// Une entrée de la liste des nuanciers : ses couleurs, puis son nom (#213).
fn gradient_popup_entry(
    ui: &mut egui::Ui,
    choice: &mut Option<GradientChoice>,
    value: GradientChoice,
    label: String,
    stops: &[String],
) {
    ui.horizontal(|ui| {
        // Toute la ligne se clique (#213) : la pastille comme le nom.
        let swatch = gradient_swatch(ui, &gradient_colors(stops, ENTRY_STEPS));
        if swatch.clicked() {
            *choice = Some(value.clone());
        }
        ui.selectable_value(choice, Some(value), label);
    });
}

/// Nuanciers (#178, #213) : choisir (fourni, enregistré, personnalisé), voir
/// ses couleurs dans la liste et en bande, enregistrer ou supprimer un nuancier
/// enregistré. Il répartit les couleurs régulièrement sur les séries du groupe
/// dans l'ordre de la liste, et le groupe le **retient** : la liste montre
/// celui du groupe, vide tant qu'aucun n'a été choisi (la bande est alors
/// grisée et rien n'est appliqué). Une série qui rejoint le groupe en reçoit la
/// couleur. Grisé tant que la portée n'est pas « tout le groupe » (`enabled`),
/// avec l'info-bulle qui le dit. Le nuancier s'applique dès qu'on le choisit ou
/// qu'on change un arrêt (brouillon, validé par OK). Hors d'un groupe, la page
/// ne l'appelle pas : il n'y a rien à répartir.
fn gradient_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    group: u32,
    enabled: bool,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let choice_id = egui::Id::new("curve_gradient_choice");
    let stops_id = egui::Id::new("curve_gradient_stops");
    let name_id = egui::Id::new("curve_gradient_name");
    let target_id = egui::Id::new("curve_gradient_target");
    let remembered: Option<GradientChoice> = ui.data(|d| d.get_temp(choice_id));
    let mut custom: Vec<String> = ui
        .data(|d| d.get_temp(stops_id))
        .unwrap_or_else(|| CUSTOM_STOPS.iter().map(|s| (*s).to_string()).collect());
    let mut name: String = ui.data(|d| d.get_temp(name_id)).unwrap_or_default();
    let mut target: SaveTarget = ui.data(|d| d.get_temp(target_id)).unwrap_or_default();

    // Ce que montre la liste vient du groupe, pas de la fenêtre : vide s'il n'a
    // reçu aucun nuancier.
    let retained: Option<Vec<String>> = panel.group_gradient(group).map(<[String]>::to_vec);
    let mut choice: Option<GradientChoice> = retained
        .as_deref()
        .map(|stops| identify_choice(stops, remembered.as_ref(), env, &custom));
    if let (Some(GradientChoice::Custom), Some(stops)) = (&choice, &retained) {
        custom.clone_from(stops);
    }

    let choice_before = choice.clone();
    let custom_before = custom.clone();
    let mut commit = false;
    let out = ui.add_enabled_ui(enabled, |ui| {
        form(ui, "curve_gradient_form", |f| {
            f.section(tr!("curve-gradient-section"));
            f.row(tr!("curve-gradient-field"), |ui| {
                let selected_text = match &choice {
                    None => String::new(),
                    Some(GradientChoice::Preset(index)) => GRADIENT_PRESETS
                        .get(*index)
                        .map_or_else(String::new, |preset| gradient_label(preset.id)),
                    Some(GradientChoice::Saved(target, name)) => tr!(
                        "curve-gradient-saved",
                        name = name.as_str(),
                        target = target.label()
                    ),
                    Some(GradientChoice::Custom) => tr!("curve-gradient-custom"),
                };
                egui::ComboBox::from_id_salt("curve_gradient_combo")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for (index, preset) in GRADIENT_PRESETS.iter().enumerate() {
                            let stops: Vec<String> =
                                preset.stops.iter().map(|s| (*s).to_string()).collect();
                            gradient_popup_entry(
                                ui,
                                &mut choice,
                                GradientChoice::Preset(index),
                                gradient_label(preset.id),
                                &stops,
                            );
                        }
                        for saved_target in [SaveTarget::Project, SaveTarget::User] {
                            for gradient in &saved_of(env, saved_target).gradients {
                                gradient_popup_entry(
                                    ui,
                                    &mut choice,
                                    GradientChoice::Saved(saved_target, gradient.name.clone()),
                                    tr!(
                                        "curve-gradient-saved",
                                        name = gradient.name.as_str(),
                                        target = saved_target.label()
                                    ),
                                    &gradient.stops,
                                );
                            }
                        }
                        gradient_popup_entry(
                            ui,
                            &mut choice,
                            GradientChoice::Custom,
                            tr!("curve-gradient-custom"),
                            &custom,
                        );
                    });
                let saved = match &choice {
                    Some(GradientChoice::Saved(target, name)) => Some((*target, name.clone())),
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
                        // Les couleurs déjà posées restent : la liste les relit
                        // comme un nuancier personnalisé.
                        choice = Some(GradientChoice::Custom);
                    }
                }
            });
            if choice == Some(GradientChoice::Custom) {
                let mut remove = None;
                for (index, stop) in custom.iter_mut().enumerate() {
                    f.ui().push_id(index, |ui| {
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
                f.field(|ui| {
                    if ui.button(tr!("curve-gradient-add-stop")).clicked() {
                        let last = custom.last().cloned().unwrap_or_default();
                        custom.push(last);
                    }
                    if ui
                        .add_enabled(
                            custom.len() > 2,
                            egui::Button::new(tr!("curve-gradient-remove-stop")),
                        )
                        .on_disabled_hover_text(tr!("disabled-gradient-min-colors"))
                        .clicked()
                    {
                        remove = Some(custom.len() - 1);
                    }
                });
                if let Some(index) = remove {
                    custom.remove(index);
                }
                f.row(tr!("curve-gradient-name-field"), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text(tr!("curve-gradient-name-field", "hint"))
                            .desired_width(theme::FieldWidth::Short.points(ui)),
                    );
                    super::form::choice(
                        ui,
                        "curve_gradient_target",
                        &mut target,
                        &[
                            ChoiceOption::new(SaveTarget::Project, SaveTarget::Project.label()),
                            ChoiceOption::new(SaveTarget::User, SaveTarget::User.label()),
                        ],
                    );
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
                        choice = Some(GradientChoice::Saved(target, name.trim().to_string()));
                    }
                });
            }
            // Pas de bouton « Appliquer » : comme tous les autres réglages,
            // le nuancier s'écrit dans le brouillon dès qu'on le change
            // (choix, arrêt de couleur) ; Cancel l'efface, OK le valide (#213).
            let active = choice
                .as_ref()
                .map(|choice| choice_stops(choice, env, &custom));
            if enabled && (choice != choice_before || custom != custom_before) {
                if let Some(active) = &active {
                    commit |= panel.set_group_gradient(group, active);
                }
            }
            // La bande montre le nuancier du groupe ; grisée, vide, tant qu'il
            // n'en a reçu aucun.
            let strip = active
                .as_deref()
                .map(|stops| gradient_colors(stops, PREVIEW_STEPS))
                .unwrap_or_default();
            gradient_strip(f.ui(), &strip);
        })
    });
    if !enabled {
        out.response.on_hover_text(tr!("curve-gradient-locked"));
    }
    ui.data_mut(|d| {
        if let Some(choice) = choice {
            d.insert_temp(choice_id, choice);
        } else {
            d.remove::<GradientChoice>(choice_id);
        }
        d.insert_temp(stops_id, custom);
        d.insert_temp(name_id, name);
        d.insert_temp(target_id, target);
    });
    commit
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

    /// Un calque « A, [B C D] » dont le groupe s'appelle « Lots » ; les séries
    /// portent le nom de leur lettre.
    fn calque_avec_groupe() -> PanelSpec {
        let mut panel = PanelSpec::default();
        for (n, name) in [(1, "A"), (2, "B"), (3, "C"), (4, "D")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            panel.series.push(series);
        }
        for member in 1..=3 {
            panel.series[member].group = Some(1);
        }
        assert!(panel.rename_group(1, "Lots"));
        panel
    }

    /// Une image de la page des séries : rend ce qui a été peint et si elle
    /// demande la validation.
    fn page(
        ctx: &egui::Context,
        panel: &mut PanelSpec,
        selected: &mut usize,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        let project = stokesforge_core::Project::new("Séries").unwrap();
        let mut env = PickerEnv::default();
        let mut actions = Vec::new();
        let mut commit = false;
        let output =
            super::super::test_support::image_de(ctx, egui::vec2(900.0, 700.0), events, |ui| {
                commit |= curve_settings_page(
                    ui,
                    panel,
                    0,
                    project.tree(),
                    selected,
                    &mut env,
                    &mut actions,
                );
            });
        (output, commit)
    }

    fn centre(output: &egui::FullOutput, texte: &str) -> egui::Pos2 {
        super::super::test_support::textes(&output.shapes)
            .into_iter()
            .find(|t| t.texte == texte)
            .unwrap_or_else(|| panic!("texte absent : {texte}"))
            .centre
    }

    /// Bord gauche du texte peint : l'alignement d'une ligne, quelle que soit
    /// la longueur de son libellé.
    fn gauche(output: &egui::FullOutput, texte: &str) -> f32 {
        output
            .shapes
            .iter()
            .find_map(|peint| match &peint.shape {
                egui::Shape::Text(shape) if shape.galley.text() == texte => {
                    Some(shape.visual_bounding_rect().left())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("texte absent : {texte}"))
    }

    fn clic(
        ctx: &egui::Context,
        panel: &mut PanelSpec,
        selected: &mut usize,
        pos: egui::Pos2,
    ) -> bool {
        let mut commit = page(ctx, panel, selected, vec![egui::Event::PointerMoved(pos)]).1;
        for pressed in [true, false] {
            commit |= page(
                ctx,
                panel,
                selected,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .1;
        }
        commit
    }

    /// #197 : la liste des séries range un groupe comme l'Object Manager — un
    /// en-tête qui porte son nom, ses membres en retrait dessous — et ne le
    /// montre plus en suffixe de chaque ligne.
    #[test]
    fn la_liste_range_un_groupe_sous_son_en_tete_nomme() {
        let ctx = super::super::test_support::contexte();
        let mut panel = calque_avec_groupe();
        let mut selected = 0;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let (seule, en_tete, membre) = (
            centre(&output, "A"),
            centre(&output, "Lots"),
            centre(&output, "B"),
        );
        assert!(seule.y < en_tete.y && en_tete.y < membre.y);
        assert!(
            gauche(&output, "B") > gauche(&output, "A") + 10.0,
            "les membres sont en retrait d'une série seule"
        );
        assert!(
            super::super::test_support::textes(&output.shapes)
                .iter()
                .all(|t| !t.texte.contains('·')),
            "plus de groupe en suffixe"
        );
    }

    /// #197 : Dissoudre rend ses séries simples (style gardé), Supprimer le
    /// groupe les retire ; les deux demandent la validation du brouillon.
    #[test]
    fn dissoudre_et_supprimer_un_groupe_depuis_la_fenetre() {
        let ctx = super::super::test_support::contexte();
        let mut panel = calque_avec_groupe();
        panel.series[1].line.width = 4.5;
        let mut selected = 1;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = centre(&output, &tr!("curve-group-ungroup"));
        assert!(clic(&ctx, &mut panel, &mut selected, pos));
        assert!(panel.series.iter().all(|s| s.group.is_none()));
        assert_eq!(panel.series[1].line.width, 4.5, "le style reste");
        assert_eq!(panel.series.len(), 4);

        let mut panel = calque_avec_groupe();
        let mut selected = 3;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = centre(&output, &tr!("curve-group-delete"));
        assert!(clic(&ctx, &mut panel, &mut selected, pos));
        assert_eq!(panel.series.len(), 1, "A reste seule");
        assert!(panel.group_names.is_empty());
        let (_, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        assert_eq!(selected, 0, "la sélection retombe sur une série existante");
    }

    /// #197 : rejoindre un groupe place la série à la suite de ses membres, et
    /// la sélection suit la série.
    #[test]
    fn rejoindre_un_groupe_deplace_la_serie_et_la_selection_la_suit() {
        let mut panel = calque_avec_groupe();
        let mut ui_selected = 0;
        let ctx = super::super::test_support::contexte();
        // « A » (rang 0) rejoint le groupe 1 par la même voie que la liste déroulante.
        let moved = panel.set_series_group(ui_selected, Some(1)).unwrap();
        ui_selected = moved;
        assert_eq!(ui_selected, 3);
        assert_eq!(
            panel
                .series
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["B", "C", "D", "A"]
        );
        let (output, _) = page(&ctx, &mut panel, &mut ui_selected, Vec::new());
        assert!(centre(&output, "A").y > centre(&output, "D").y);
    }

    /// #209 : rejoindre un groupe **par la liste déroulante** de la fenêtre —
    /// et non par `set_series_group` appelé directement — range la série à la
    /// suite des membres, et l'éditeur garde la même série (`*index = moved`
    /// dans `membership_editor`). Témoin : avant le choix, la série réglée est
    /// au rang 0, qu'elle quitte.
    #[test]
    fn la_liste_deroulante_fait_rejoindre_un_groupe_et_l_editeur_suit_la_serie() {
        let ctx = super::super::test_support::contexte();
        let mut panel = calque_avec_groupe();
        let mut selected = 0; // « A », hors groupe
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let liste = centre(&output, &tr!("curve-group-none"));
        assert!(
            !clic(&ctx, &mut panel, &mut selected, liste),
            "ouvrir la liste ne change rien"
        );
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = centre(
            &output,
            &tr!("curve-group-entry", group = "Lots", count = 3),
        );
        assert!(
            clic(&ctx, &mut panel, &mut selected, pos),
            "le choix demande la validation"
        );
        let noms: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(noms, ["B", "C", "D", "A"], "A rejoint la fin du groupe");
        assert_eq!(panel.series[3].group, Some(1));
        assert_eq!(selected, 3);
        assert_eq!(panel.series[selected].name, "A", "l'éditeur suit la série");
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        assert!(
            super::super::test_support::textes(&output.shapes)
                .iter()
                .any(|t| t.texte == tr!("curve-group-entry", group = "Lots", count = 4)),
            "la liste montre désormais le groupe de la série"
        );
    }

    /// #197 : Descendre le groupe garde la série réglée (la même, pas le même
    /// rang) dans l'éditeur.
    #[test]
    fn descendre_le_groupe_garde_la_serie_reglee() {
        let ctx = super::super::test_support::contexte();
        let mut panel = calque_avec_groupe();
        let mut suite = SeriesSpec::new(NodeId::from_raw(5));
        suite.name = "E".into();
        panel.series.push(suite);
        let mut selected = 1; // « B », premier membre du groupe
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = centre(&output, &tr!("curve-group-move-down"));
        assert!(clic(&ctx, &mut panel, &mut selected, pos));
        let noms: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            noms,
            ["A", "E", "B", "C", "D"],
            "témoin : le groupe a bougé"
        );
        assert_eq!(panel.series[selected].name, "B");
    }

    /// La page des séries vivante, pour piloter un glisser-déposer image par
    /// image : le brouillon se modifie tout de suite, sans aperçu.
    struct Banc {
        project: stokesforge_core::Project,
        panel: PanelSpec,
        selected: usize,
        committed: bool,
    }

    fn banc(panel: PanelSpec, selected: usize) -> egui_kittest::Harness<'static, Banc> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .build_ui_state(
                |ui, banc: &mut Banc| {
                    let mut env = PickerEnv::default();
                    let mut actions = Vec::new();
                    banc.committed |= curve_settings_page(
                        ui,
                        &mut banc.panel,
                        0,
                        banc.project.tree(),
                        &mut banc.selected,
                        &mut env,
                        &mut actions,
                    );
                },
                Banc {
                    project: stokesforge_core::Project::new("Séries").unwrap(),
                    panel,
                    selected,
                    committed: false,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn poignee(harness: &egui_kittest::Harness<'static, Banc>, nom: &str) -> egui::Rect {
        use egui_kittest::kittest::Queryable;
        harness.get_by_label(nom).rect()
    }

    /// Saisit la poignée de `nom` et la mène sur le haut de la ligne de `vers`.
    fn glisser(harness: &mut egui_kittest::Harness<'static, Banc>, nom: &str, vers: &str) {
        let from = poignee(harness, nom).center();
        let target = poignee(harness, vers);
        let to = egui::pos2(target.center().x, target.top() + 1.0);
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from + egui::vec2(14.0, 0.0));
        harness.step();
        harness.hover_at(from + egui::vec2(20.0, 0.0));
        harness.step();
        for _ in 0..3 {
            harness.hover_at(to);
            harness.step();
        }
        harness.drop_at(to);
        harness.run();
    }

    fn noms(panel: &PanelSpec) -> Vec<&str> {
        panel.series.iter().map(|s| s.name.as_str()).collect()
    }

    /// Une série seule glissée sur un membre rejoint le groupe ; le brouillon
    /// est modifié tout de suite et la série réglée la suit.
    #[test]
    fn glisser_une_serie_seule_sur_un_membre_la_fait_rejoindre_le_groupe() {
        let mut harness = banc(calque_avec_groupe(), 0);
        glisser(&mut harness, "A", "B");
        let banc = harness.state();
        assert!(banc.committed, "le brouillon est à valider");
        let a = banc
            .panel
            .series
            .iter()
            .position(|s| s.name == "A")
            .unwrap();
        assert_eq!(banc.panel.series[a].group, Some(1));
        assert_eq!(banc.panel.group_members(1).len(), 4);
        assert_eq!(banc.selected, a, "la série réglée suit sa place");
    }

    /// Un membre glissé sur une série seule quitte son groupe.
    #[test]
    fn glisser_un_membre_sur_une_serie_seule_le_fait_sortir_du_groupe() {
        let mut harness = banc(calque_avec_groupe(), 1);
        glisser(&mut harness, "B", "A");
        let banc = harness.state();
        assert_eq!(noms(&banc.panel), ["B", "A", "C", "D"]);
        assert_eq!(banc.panel.series[0].group, None);
        assert_eq!(banc.panel.group_members(1), [2, 3]);
        assert_eq!(banc.selected, 0);
    }

    /// Une série seule et un groupe s'échangent ; le groupe se glisse en bloc.
    #[test]
    fn glisser_un_groupe_le_deplace_en_bloc_et_la_selection_le_suit() {
        let mut harness = banc(calque_avec_groupe(), 2);
        glisser(&mut harness, "Lots", "A");
        let banc = harness.state();
        assert_eq!(noms(&banc.panel), ["B", "C", "D", "A"]);
        assert_eq!(banc.panel.group_members(1), [0, 1, 2]);
        // Saisir l'en-tête choisit le groupe (sa première série, « B »), qui suit.
        assert_eq!(banc.panel.series[banc.selected].name, "B");
    }

    /// Entre deux séries indépendantes, deux séries s'échangent.
    #[test]
    fn glisser_une_serie_independante_sur_une_autre_les_echange() {
        let mut panel = PanelSpec::default();
        for (n, name) in [(1, "A"), (2, "B"), (3, "C")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            panel.series.push(series);
        }
        let mut harness = banc(panel, 0);
        glisser(&mut harness, "A", "C");
        assert_eq!(noms(&harness.state().panel), ["B", "C", "A"]);
        assert_eq!(harness.state().selected, 2);
    }
}
