//! Fenêtre des propriétés du graphe : onglets (thème, axes, séries, légende),
//! éditeurs par famille, mémo des formules Typst, libellés des choix, et la
//! fenêtre des métadonnées d'une colonne (même famille de dialogue).
//! Points d'entrée pour `app.rs` : [`plot_settings_window`] et
//! [`column_metadata_window`].
//!
//! Sorti d'`app.rs` (#119), puis découpé par page (#124). Les fonctions de
//! page empruntent seulement le brouillon utile et rendent la demande de
//! validation ; le corps commun conserve les sous-Ui, leurs identifiants et
//! l'ordre validation → couleurs enregistrées → sélection de page/axe.

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{
    AxisSpec, LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, SymbolShape,
};

use crate::color_picker::{color_field, ColorUsage, PickerEnv, SavedColorEdit};
use crate::number_field::number_field;
use crate::quantities;
use crate::state::{
    parse_x_prime, Action, AppState, PlotAxis, PlotSettings, PlotSettingsTab, SaveTarget,
};
use crate::theme::{bold, section_title};
use crate::tr;

use super::curve_settings::curve_settings_page;
use super::dialogs::{modal_dialog, DialogOutcome};
use super::graph_window::{published_axis_bounds, PanelBounds};
use super::hover_help::hover_help;
use super::tab_strip::{tab_strip, TabStripSide};

/// Largeur par défaut de la fenêtre des propriétés d'un graphe
/// ([`plot_settings_window`]).
const PLOT_SETTINGS_WIDTH: f32 = 380.0;
/// Hauteur par défaut de la fenêtre des propriétés d'un graphe. La fenêtre
/// se redimensionne ensuite librement, y compris plus haut que son contenu
/// (voir [`plot_settings_contents`]).
const PLOT_SETTINGS_HEIGHT: f32 = 460.0;
/// Dimensions par défaut de la fenêtre des séries (#179) : une liste et un
/// éditeur côte à côte.
const CURVE_SETTINGS_WIDTH: f32 = 640.0;
const CURVE_SETTINGS_HEIGHT: f32 = 500.0;

/// Fenêtre des propriétés d'un graphe : un dialogue flottant (même famille
/// que `import_window`), ouvert par le clic droit sur un composant du
/// graphe ou par le menu « Graphique », et organisé en onglets thématiques
/// ([`PlotSettingsTab`]). Elle remplace l'ancien panneau fixe à droite de
/// l'espace de travail, qui prenait de la place en permanence dès qu'un
/// graphe était actif.
///
/// Édite le [`stokesforge_plot::PlotSpec`] au travers d'un brouillon transitoire
/// ([`crate::state::PlotEdit`]), jamais directement.
///
/// **Stratégie anti-inflation d'historique** : chaque widget met à jour le
/// brouillon en direct (retour visuel immédiat), et `commit` — un simple
/// booléen local, mis à `true` par n'importe quel widget qui vient de finir
/// une interaction — ne déclenche [`AppState::commit_plot_edit`] qu'une
/// seule fois, à la fin de la fonction. Trois familles de widgets, trois
/// façons de finir une interaction :
/// - **texte** (titres, libellés) : `lost_focus()`,
///   comme `note_contents`/`cell_widget` — jamais à chaque frappe ;
/// - **nombre** (épaisseur de trait, taille de marqueur, bornes d'axe, brique
///   [`crate::number_field`]) : `NumberResponse::changed` — Entrée, sortie du
///   champ, flèche, cran de molette ou choix dans la liste ;
/// - **case à cocher / menu déroulant** : un seul événement discret par
///   clic, validé tout de suite.
///
/// Fermer la fenêtre valide ce qui diverge encore du modèle
/// ([`AppState::close_plot_settings`]).
///
/// **Un seul panneau, pas de multi-panneaux** : seul `PlotSpec::panels[0]`
/// est éditable ici. Éditer le premier panneau revient, pour l'instant, à
/// éditer *le* graphe.
pub(super) fn plot_settings_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(settings) = state.plot_settings() else {
        return;
    };
    // Cloné avant d'emprunter `state` en écriture pour le brouillon : un
    // `Arc<Tree>` cloné est un compteur de références, pas une copie.
    let tree = state.tree().clone();
    let Some(node) = tree.node(settings.graph) else {
        state.cancel_plot_settings();
        return;
    };
    // Les séries ont leur propre fenêtre (#179) : même brouillon, autre titre,
    // autre taille, autre identité (donc sa propre place à l'écran).
    let curves = settings.tab == PlotSettingsTab::Series;
    let (title, id, size) = if curves {
        (
            tr!("curve-window-title", name = node.name()),
            "stokesforge_curve_settings",
            egui::vec2(CURVE_SETTINGS_WIDTH, CURVE_SETTINGS_HEIGHT),
        )
    } else {
        (
            tr!("plot-window-title", name = node.name()),
            "stokesforge_plot_settings",
            egui::vec2(PLOT_SETTINGS_WIDTH, PLOT_SETTINGS_HEIGHT),
        )
    };
    state.begin_plot_edit(settings.graph);
    let outcome = modal_dialog(ctx, egui::Id::new(id), &title, size, |ui| {
        plot_settings_contents(ui, state, &tree, settings)
    });
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => state.close_plot_settings(),
        DialogOutcome::Cancel => state.cancel_plot_settings(),
    }
}

/// Fenêtre de consultation des métadonnées de fichier d'une colonne (issue
/// #5) : les lignes clé / valeur, sections comprises, en lecture seule. Même
/// brique modale que les propriétés du graphe ; « Valider » et « Annuler »
/// la ferment sans rien changer. Seule exception : le bouton « Utiliser
/// comme x' » d'une valeur numérique, qui l'écrit dans la ligne x' de la
/// colonne (#164).
pub(super) fn column_metadata_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    let Some(column) = state.column_metadata_view() else {
        return;
    };
    let meta = state.column_meta(column);
    let lines = meta.file_metadata;
    let title = if meta.long_name.is_empty() {
        tr!("workbook-metadata-title")
    } else {
        tr!(
            "workbook-metadata-title-named",
            name = meta.long_name.as_str()
        )
    };
    let outcome = modal_dialog(
        ctx,
        egui::Id::new("stokesforge_column_metadata"),
        &title,
        egui::vec2(520.0, 420.0),
        |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("stokesforge_column_metadata_grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for line in &lines {
                            if line.is_section() {
                                ui.label(crate::theme::bold(&line.key));
                                ui.label("");
                            } else if line.key.is_empty() {
                                ui.label("");
                                ui.label(&line.value);
                            } else {
                                ui.label(&line.key);
                                ui.label(&line.value);
                            }
                            // Mini sélecteur d'import (#164) : une valeur
                            // numérique du fichier peut alimenter la ligne x'.
                            if !line.key.is_empty()
                                && matches!(parse_x_prime(&line.value), Ok(Some(_)))
                                && ui
                                    .small_button(tr!("workbook-metadata-use-as-x-prime"))
                                    .on_hover_text(tr!("workbook-metadata-use-as-x-prime", "hint"))
                                    .clicked()
                            {
                                actions.push(Action::SetColumnXPrime {
                                    column,
                                    text: line.value.clone(),
                                });
                            }
                            ui.end_row();
                        }
                    });
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseColumnMetadata);
    }
}

/// Corps de [`plot_settings_window`] : la bande d'onglets, puis la page de
/// l'onglet choisi.
fn plot_settings_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    tree: &Tree,
    settings: PlotSettings,
) {
    let mut chosen_tab = settings.tab;
    let mut chosen_axis = settings.axis;
    // Même bande que les onglets de feuille (`tab_strip`, issue #19) : elle
    // défile au lieu de rétrécir quand la fenêtre est étroite.
    if settings.tab != PlotSettingsTab::Series {
        let tabs: Vec<(PlotSettingsTab, String)> = PlotSettingsTab::ALL
            .iter()
            .map(|tab| (*tab, tab.label()))
            .collect();
        // La bande trace son propre filet sous les onglets (#169) : pas de
        // séparateur en plus, seulement l'espace qu'il laissait.
        tab_strip(
            ui,
            TabStripSide::Top,
            &tabs,
            settings.tab,
            |tab| chosen_tab = tab,
            None,
            || {},
        );
        ui.add_space(ui.spacing().item_spacing.y);
    }

    let mut commit = false;
    let mut gradient_actions: Vec<Action> = Vec::new();
    // Couleurs enregistrées, lues avant d'emprunter le brouillon ; les
    // demandes du sélecteur (enregistrer, retirer) sont traduites en actions
    // après.
    let mut env = PickerEnv {
        project: state.saved_colors(SaveTarget::Project),
        user: state.saved_colors(SaveTarget::User),
        requests: Vec::new(),
    };
    if let Some(edit) = state
        .plot_edit_mut()
        .filter(|edit| edit.graph == settings.graph)
    {
        let spec = &mut edit.spec;
        edit.selected_panel = edit.selected_panel.min(spec.panels.len() - 1);
        if spec.panels.len() > 1 {
            egui::ComboBox::from_id_salt("plot_settings_layer")
                .selected_text(tr!(
                    "object-manager-layer",
                    index = (edit.selected_panel + 1)
                ))
                .show_ui(ui, |ui| {
                    for index in 0..spec.panels.len() {
                        ui.selectable_value(
                            &mut edit.selected_panel,
                            index,
                            tr!("object-manager-layer", index = (index + 1)),
                        );
                    }
                });
        }
        // La page occupe toute la hauteur de la fenêtre, même quand son
        // contenu est plus court : sans `auto_shrink(false)`, une
        // `egui::Window` redimensionnable se recale sur la hauteur de son
        // contenu, et on ne pouvait pas l'agrandir au-delà. Le même réglage
        // la fait défiler quand la fenêtre est plus petite que la page.
        if chosen_tab == PlotSettingsTab::Series {
            // Deux colonnes qui défilent chacune : pas de défilement commun.
            if let Some(panel) = spec.panels.get_mut(edit.selected_panel) {
                commit |= curve_settings_page(
                    ui,
                    panel,
                    edit.selected_panel,
                    tree,
                    &mut edit.selected_series,
                    &mut env,
                    &mut gradient_actions,
                );
            }
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match chosen_tab {
                    PlotSettingsTab::Graph => {
                        commit |= graph_settings_page(ui, spec, &mut env);
                    }
                    PlotSettingsTab::PlotArea => {
                        let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                            return;
                        };
                        commit |= plot_area_settings_page(ui, panel);
                    }
                    PlotSettingsTab::Axes => {
                        let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                            return;
                        };
                        commit |= axes_settings_page(
                            ui,
                            panel,
                            &mut chosen_axis,
                            published_axis_bounds(ui.ctx(), settings.graph, edit.selected_panel),
                            &mut env,
                        );
                    }
                    // Les séries ont leur fenêtre : voir plus haut.
                    PlotSettingsTab::Series => {}
                    PlotSettingsTab::Theme => {
                        commit |= theme_editor(ui, spec, &mut env);
                    }
                    PlotSettingsTab::Legend => {
                        let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                            return;
                        };
                        commit |= legend_editor(ui, &mut panel.legend);
                    }
                });
        }
    }
    if commit {
        state.commit_plot_edit();
    }
    for action in gradient_actions {
        state.dispatch(action);
    }
    for request in env.requests {
        state.dispatch(match request.edit {
            SavedColorEdit::Save => Action::SaveColor {
                usage: request.usage,
                color: request.color,
                target: request.target,
            },
            SavedColorEdit::Remove => Action::RemoveSavedColor {
                usage: request.usage,
                color: request.color,
                target: request.target,
            },
        });
    }
    if let Some(current) = state.plot_settings_mut() {
        current.tab = chosen_tab;
        current.axis = chosen_axis;
    }
}

/// Page du graphe : titre et fond, sans ajouter de sous-Ui aux widgets.
fn graph_settings_page(ui: &mut egui::Ui, spec: &mut PlotSpec, env: &mut PickerEnv) -> bool {
    let mut commit = false;
    commit |= formula_field(
        ui,
        &tr!("plot-general-title"),
        &tr!("plot-general-title", "hint"),
        &mut spec.title,
    );
    commit |= color_field(
        ui,
        &tr!("plot-title-color"),
        &mut spec.title_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );
    ui.add_space(6.0);
    commit |= color_field(
        ui,
        &tr!("plot-general-background"),
        &mut spec.background,
        ColorUsage::Background,
        Some(&tr!("plot-color-auto-white")),
        env,
    );
    ui.weak(tr!("plot-general-dark-note"));
    ui.add_space(6.0);
    commit |= ui
        .checkbox(&mut spec.stale_warning, tr!("plot-stale-warning"))
        .on_hover_text(tr!("plot-stale-warning", "hint"))
        .changed();
    commit
}

/// Page du calque : titre et lignes de grille.
fn plot_area_settings_page(ui: &mut egui::Ui, panel: &mut PanelSpec) -> bool {
    let mut commit = false;
    commit |= formula_field(
        ui,
        &tr!("plot-area-layer-title"),
        &tr!("plot-area-layer-title", "hint"),
        &mut panel.title,
    );
    section_title(ui, tr!("plot-area-gridlines"));
    commit |= ui
        .checkbox(&mut panel.bottom.grid, tr!("plot-area-gridlines-vertical"))
        .changed();
    commit |= ui
        .checkbox(&mut panel.left.grid, tr!("plot-area-gridlines-horizontal"))
        .changed();
    commit
}

/// Page des axes : choix local de l'axe, puis éditeur partagé.
fn axes_settings_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    chosen_axis: &mut PlotAxis,
    bounds: Option<PanelBounds>,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    let mut axes = vec![PlotAxis::Bottom, PlotAxis::Left];
    if panel.overlay.is_none() {
        // Haut et droite : les axes miroirs (#176), toujours proposés.
        axes.extend([PlotAxis::Right, PlotAxis::Top]);
    } else if panel.uses_right_axis() || *chosen_axis == PlotAxis::Right {
        axes.push(PlotAxis::Right);
    }
    // #169 : les axes sont un jeu d'onglets (une page par axe), donc la
    // bande commune, pas une rangée de boutons sélectionnables ; son filet
    // remplace l'ancien séparateur.
    let tabs: Vec<(PlotAxis, String)> = axes.into_iter().map(|axis| (axis, axis.label())).collect();
    let mut picked = None;
    tab_strip(
        ui,
        TabStripSide::Top,
        &tabs,
        *chosen_axis,
        |axis| picked = Some(axis),
        None,
        || {},
    );
    if let Some(axis) = picked {
        *chosen_axis = axis;
    }
    ui.add_space(ui.spacing().item_spacing.y);
    // Le haut est toujours un miroir ; la droite l'est tant qu'aucune série n'y
    // est rattachée (sinon c'est un vrai axe, éditeur complet).
    let mirror = panel.overlay.is_none()
        && (*chosen_axis == PlotAxis::Top
            || (*chosen_axis == PlotAxis::Right && !panel.uses_right_axis()));
    let axis = match *chosen_axis {
        PlotAxis::Bottom => &mut panel.bottom,
        PlotAxis::Left => &mut panel.left,
        PlotAxis::Right => &mut panel.right,
        PlotAxis::Top => &mut panel.top,
    };
    commit |= if mirror {
        mirror_axis_editor(ui, axis)
    } else {
        let computed = bounds.and_then(|bounds| match *chosen_axis {
            PlotAxis::Bottom => bounds.bottom,
            PlotAxis::Left => bounds.left,
            _ => bounds.right,
        });
        axis_editor(ui, axis, computed, env)
    };
    commit
}

/// Onglet « Thème » : fond, couleur du texte et couleur des axes, réglés
/// globalement (issue #11), plus des thèmes prédéfinis. Le dernier réglage
/// écrase ce qui avait été réglé une à une : voir [`stokesforge_plot::theme`].
/// Rend `true` si le brouillon doit être validé.
fn theme_editor(
    ui: &mut egui::Ui,
    spec: &mut stokesforge_plot::PlotSpec,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    section_title(ui, tr!("plot-theme-themes"));
    ui.horizontal(|ui| {
        for theme in &stokesforge_plot::THEMES {
            if ui.button(theme.name).clicked() {
                spec.apply_theme(theme);
                commit = true;
            }
        }
    });
    section_title(ui, tr!("plot-theme-global-colors"));
    commit |= color_field(
        ui,
        &tr!("plot-theme-background"),
        &mut spec.background,
        ColorUsage::Background,
        Some(&tr!("plot-color-auto-white")),
        env,
    );
    let mut text = spec.theme_text_color().to_string();
    let text_committed = color_field(
        ui,
        &tr!("plot-theme-text"),
        &mut text,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );
    if text != spec.theme_text_color() {
        spec.set_text_color(&text);
    }
    commit |= text_committed;
    let mut axes = spec.theme_axes_color().to_string();
    let axes_committed = color_field(
        ui,
        &tr!("plot-theme-axes"),
        &mut axes,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );
    if axes != spec.theme_axes_color() {
        spec.set_axes_color(&axes);
    }
    commit |= axes_committed;
    ui.weak(tr!("plot-theme-note"));
    commit
}

/// Le rappel de syntaxe des textes du graphe (issues #33, #197), en bulle de
/// survol sur l'icône d'aide posée à droite du champ.
///
/// Ces textes sont composés par Typst et lus comme du **markup** (issue
/// #32) : un chimiste venu d'Origin n'en connaît pas la syntaxe, et un `_`
/// ou un `$` isolé change le rendu sans prévenir. Le rappel nomme Typst et
/// donne la page de référence. Une icône au bout du champ plutôt qu'un mémo
/// repliable dessous : le lien avec le champ se voit, et l'aide ne prend plus
/// de hauteur dans la fenêtre (voir [`super::hover_help`]).
fn formula_help(ui: &mut egui::Ui) {
    hover_help(ui, |ui| {
        ui.label(bold(tr!("plot-formula-memo-header")));
        egui::Grid::new("plot_formula_help")
            .num_columns(2)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for (source, effect) in formula_memo_rows() {
                    ui.monospace(source);
                    ui.weak(effect);
                    ui.end_row();
                }
            });
        ui.weak(tr!(
            "plot-formula-footer",
            url = "typst.app/docs/reference/math"
        ));
    });
}

/// Les lignes du rappel de [`formula_help`] : ce qu'on tape (de la syntaxe
/// Typst, jamais traduite), ce qu'on obtient (traduit).
fn formula_memo_rows() -> [(&'static str, String); 7] {
    [
        ("$lambda$  $Delta$", tr!("plot-formula-greek")),
        ("$x_2$  $\"Ru(bpy)\"_3$", tr!("plot-formula-subscript")),
        ("$x^2$  $\"cm\"^(-1)$", tr!("plot-formula-superscript")),
        ("$Delta H = sum_i n_i$", tr!("plot-formula-full")),
        ("$\"L mol\"^(-1)$", tr!("plot-formula-upright")),
        ("*bold*  _italic_", tr!("plot-formula-format")),
        ("\\$  \\_  \\*  \\#", tr!("plot-formula-escape")),
    ]
}

/// Un champ texte libellé qui accepte une formule (#197), validé à la perte de
/// focus ; `hint` est son infobulle. Tout à droite, l'icône d'aide à la syntaxe
/// Typst ([`formula_help`]). Rend `true` si le brouillon doit être validé —
/// voir la doc de tête de [`plot_settings_window`].
fn formula_field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let lost = ui
            .text_edit_singleline(value)
            .on_hover_text(hint)
            .lost_focus();
        formula_help(ui);
        lost
    })
    .inner
}

/// Édite un axe miroir (#176) : il reprend l'échelle de l'axe opposé et n'a ni
/// titre ni couleur propres ; on choisit seulement de l'afficher, avec ou sans
/// étiquettes de graduation.
fn mirror_axis_editor(ui: &mut egui::Ui, axis: &mut AxisSpec) -> bool {
    section_title(ui, tr!("plot-axes-display-section"));
    let mut commit = ui
        .checkbox(&mut axis.mirror, tr!("plot-axes-show-mirror"))
        .changed();
    commit |= ui
        .add_enabled(
            axis.mirror,
            egui::Checkbox::new(&mut axis.tick_labels, tr!("plot-axes-show-tick-labels")),
        )
        .changed();
    ui.weak(tr!("plot-axes-mirror-note"));
    commit
}

/// Édite un [`AxisSpec`], en trois groupes : le titre de l'axe (libellé,
/// unité), son échelle (logarithmique, inversée, bornes) et son affichage
/// (axe, grille). Rend `true` si le brouillon doit être validé — voir la doc
/// de tête de [`plot_settings_window`].
fn axis_editor(
    ui: &mut egui::Ui,
    axis: &mut AxisSpec,
    computed: Option<(f64, f64)>,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    section_title(ui, tr!("plot-axes-title-section"));
    commit |= formula_field(
        ui,
        &tr!("plot-axes-quantity"),
        &tr!("plot-axes-quantity", "hint"),
        &mut axis.label,
    );
    commit |= formula_field(
        ui,
        &tr!("plot-axes-units"),
        &tr!("plot-axes-units", "hint"),
        &mut axis.unit,
    );
    commit |= color_field(
        ui,
        &tr!("plot-title-color"),
        &mut axis.title_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );

    section_title(ui, tr!("plot-axes-scale-section"));
    let mut log_scale = axis.scale == stokesforge_plot::AxisScale::Log;
    if ui
        .checkbox(&mut log_scale, tr!("plot-axes-log-scale"))
        .on_hover_text(tr!("plot-axes-log-scale", "hint"))
        .changed()
    {
        axis.scale = if log_scale {
            stokesforge_plot::AxisScale::Log
        } else {
            stokesforge_plot::AxisScale::Linear
        };
        commit = true;
    }
    commit |= ui
        .checkbox(&mut axis.inverted, tr!("plot-axes-reverse"))
        .on_hover_text(tr!("plot-axes-reverse", "hint"))
        .changed();
    let auto_changed = ui
        .checkbox(&mut axis.auto_range, tr!("plot-axes-auto-range"))
        .on_hover_text(tr!("plot-axes-auto-range", "hint"))
        .changed();
    commit |= auto_changed;
    if auto_changed && !axis.auto_range {
        // Décocher reprend les bornes calculées comme valeurs (G4).
        if let Some((low, high)) = computed {
            axis.range_min = Some(low);
            axis.range_max = Some(high);
        }
    }
    // Min et Max restent visibles : grisés sous « Auto range », avec les
    // bornes calculées (#221, #170 §1).
    let auto = axis.auto_range;
    let (mut min, mut max) = if auto {
        computed
            .or(axis.range_min.zip(axis.range_max))
            .unwrap_or((0.0, 0.0))
    } else {
        (axis.range_min.unwrap_or(0.0), axis.range_max.unwrap_or(0.0))
    };
    let mut changed = false;
    let mut auto_asked = false;
    ui.add_enabled_ui(!auto, |ui| {
        for (label, salt, value) in [
            (tr!("plot-axes-min"), "axis_min", &mut min),
            (tr!("plot-axes-max"), "axis_max", &mut max),
        ] {
            let response = ui
                .horizontal(|ui| {
                    ui.label(label);
                    number_field(ui, salt, value, &quantities::AXIS_BOUND)
                })
                .inner;
            changed |= response.changed;
            auto_asked |= response.auto;
        }
    })
    .response
    .on_disabled_hover_text(tr!("plot-axes-auto-range", "disabled"));
    if auto_asked {
        axis.auto_range = true;
        commit = true;
    } else if !auto && changed {
        axis.range_min = Some(min);
        axis.range_max = Some(max);
        commit = true;
    }

    section_title(ui, tr!("plot-axes-display-section"));
    commit |= ui
        .checkbox(&mut axis.visible, tr!("plot-axes-show-axis"))
        .changed();
    commit |= ui
        .checkbox(&mut axis.tick_labels, tr!("plot-axes-show-tick-labels"))
        .changed();
    commit |= ui
        .checkbox(&mut axis.grid, tr!("plot-axes-show-gridlines"))
        .changed();
    commit |= color_field(
        ui,
        &tr!("plot-axes-color"),
        &mut axis.tick_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );
    ui.weak(tr!("plot-axes-color-note"));
    commit
}

/// Édite la série sélectionnée : nom, type de tracé, visibilité, présence en
/// légende, style de trait, style de marqueur. Rend `true` si le brouillon
/// doit être validé — voir la doc de tête de [`plot_settings_window`].
///
/// [`PlotType::Unknown`] (C5, `guide-de-correction-robustesse.md` §F3) ne
/// figure **jamais** dans la liste proposée : c'est le type de tracé d'une
/// version plus récente de StokesForge, et le proposer laisserait croire qu'on
/// peut y revenir alors que rien ne sait le reconstruire. Une série qui le
/// porte s'affiche quand même, en lecture seule pour ce seul champ — le
/// reste (nom, visibilité...) reste éditable normalement.
///
/// `locked` (#177) : on règle le **groupe** de la série, pas la série seule.
/// Les réglages propres à une série (nom, type, visibilité, présence en
/// légende, couleurs) sont alors grisés, avec une info-bulle qui dit pourquoi ;
/// les autres (style du trait, forme et taille du symbole) restent actifs et la
/// page les recopie dans tout le groupe.
pub(super) fn series_editor(
    ui: &mut egui::Ui,
    series: &mut stokesforge_plot::SeriesSpec,
    env: &mut PickerEnv,
    locked: bool,
) -> bool {
    let mut commit = false;

    commit |= per_series(ui, locked, |ui| {
        formula_field(
            ui,
            &tr!("plot-series-name"),
            &tr!("plot-series-name", "hint"),
            &mut series.name,
        )
    });

    commit |= per_series(ui, locked, |ui| {
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.label(tr!("plot-series-plot-type"));
            match &series.plot_type {
                PlotType::Unknown(_) => {
                    ui.weak(tr!("plot-series-type-unknown"));
                }
                current => {
                    let mut selected = current.clone();
                    egui::ComboBox::new("plot_config_series_type", "")
                        .selected_text(plot_type_label(&selected))
                        .show_ui(ui, |ui| {
                            for candidate in [
                                PlotType::Scatter,
                                PlotType::Line,
                                PlotType::LineScatter,
                                PlotType::Bar,
                                PlotType::Histogram,
                                PlotType::ErrorBar,
                            ] {
                                let text = plot_type_label(&candidate);
                                let is_selected = candidate == selected;
                                if ui.selectable_label(is_selected, text).clicked() {
                                    selected = candidate;
                                }
                            }
                        });
                    if selected != series.plot_type {
                        series.plot_type = selected;
                        commit = true;
                    }
                }
            }
        });
        commit |= ui
            .checkbox(&mut series.visible, tr!("plot-series-show"))
            .changed();
        commit |= ui
            .checkbox(
                &mut series.show_in_legend,
                tr!("plot-series-show-in-legend"),
            )
            .changed();
        commit
    });

    section_title(ui, tr!("plot-series-line-section"));
    commit |= per_series(ui, locked, |ui| {
        color_field(
            ui,
            &tr!("plot-series-line-color"),
            &mut series.line.color,
            ColorUsage::Curves,
            None,
            env,
        )
    });

    ui.horizontal(|ui| {
        ui.label(tr!("plot-series-line-style"));
        let mut style = series.line.style;
        egui::ComboBox::new("plot_config_line_style", "")
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
        if style != series.line.style {
            series.line.style = style;
            commit = true;
        }
    });

    commit |= ui
        .horizontal(|ui| {
            ui.label(tr!("plot-series-line-width"));
            number_field(
                ui,
                "series_line_width",
                &mut series.line.width,
                &quantities::LINE_WIDTH,
            )
            .changed
        })
        .inner;

    section_title(ui, tr!("plot-series-symbol-section"));
    ui.horizontal(|ui| {
        ui.label(tr!("plot-series-symbol-shape"));
        let mut shape = series.symbol.shape;
        egui::ComboBox::new("plot_config_symbol_shape", "")
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
        if shape != series.symbol.shape {
            series.symbol.shape = shape;
            commit = true;
        }
    });

    commit |= ui
        .horizontal(|ui| {
            ui.label(tr!("plot-series-symbol-size"));
            number_field(
                ui,
                "series_symbol_size",
                &mut series.symbol.size,
                &quantities::SYMBOL_SIZE,
            )
            .changed
        })
        .inner;

    commit |= per_series(ui, locked, |ui| {
        color_field(
            ui,
            &tr!("plot-series-symbol-color"),
            &mut series.symbol.color,
            ColorUsage::Curves,
            Some(&tr!("plot-color-same-as-line")),
            env,
        )
    });

    commit
}

/// Un réglage propre à une série : grisé, avec l'info-bulle qui dit pourquoi,
/// quand on règle le groupe entier (`locked`, voir [`series_editor`]).
fn per_series(ui: &mut egui::Ui, locked: bool, add: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    let out = ui.add_enabled_ui(!locked, add);
    if locked {
        out.response.on_hover_text(tr!("curve-scope-locked"));
    }
    out.inner
}

/// Édite un [`stokesforge_plot::LegendSpec`] : visibilité, coin d'ancrage. Rend
/// `true` si le brouillon doit être validé — voir la doc de tête de
/// [`plot_settings_window`].
fn legend_editor(ui: &mut egui::Ui, legend: &mut stokesforge_plot::LegendSpec) -> bool {
    let mut commit = false;
    commit |= ui
        .checkbox(&mut legend.visible, tr!("plot-legend-show"))
        .changed();

    ui.horizontal(|ui| {
        ui.label(tr!("plot-legend-position"));
        let mut position = legend.position;
        egui::ComboBox::new("plot_config_legend_position", "")
            .selected_text(legend_position_label(position))
            .show_ui(ui, |ui| {
                for candidate in [
                    LegendPosition::TopLeft,
                    LegendPosition::TopRight,
                    LegendPosition::BottomLeft,
                    LegendPosition::BottomRight,
                ] {
                    ui.selectable_value(&mut position, candidate, legend_position_label(candidate));
                }
            });
        if position != legend.position {
            legend.position = position;
            commit = true;
        }
    });

    commit
}

/// Libellé d'un [`PlotType`], y compris [`PlotType::Unknown`] (par
/// prudence : ce bras existe pour que la fonction reste totale, mais
/// `series_editor` ne l'affiche jamais — un type inconnu ne passe pas par le
/// menu déroulant qui appelle cette fonction).
fn plot_type_label(plot_type: &PlotType) -> String {
    match plot_type {
        PlotType::Scatter => tr!("plot-type-scatter"),
        PlotType::Line => tr!("plot-type-line"),
        PlotType::LineScatter => tr!("plot-type-line-symbol"),
        PlotType::Bar => tr!("plot-type-bar"),
        PlotType::Histogram => tr!("plot-type-histogram"),
        PlotType::ErrorBar => tr!("plot-type-error-bars"),
        PlotType::Unknown(_) => tr!("plot-type-unknown"),
    }
}

/// Libellé d'un [`LineStyle`].
pub(super) fn line_style_label(style: LineStyle) -> String {
    match style {
        LineStyle::None => tr!("plot-line-style-none"),
        LineStyle::Solid => tr!("plot-line-style-solid"),
        LineStyle::Dash => tr!("plot-line-style-dashed"),
        LineStyle::Dot => tr!("plot-line-style-dotted"),
        LineStyle::DashDot => tr!("plot-line-style-dash-dot"),
    }
}

/// Libellé d'un [`SymbolShape`].
pub(super) fn symbol_shape_label(shape: SymbolShape) -> String {
    match shape {
        SymbolShape::None => tr!("plot-symbol-none"),
        SymbolShape::Circle => tr!("plot-symbol-circle"),
        SymbolShape::Square => tr!("plot-symbol-square"),
        SymbolShape::Triangle => tr!("plot-symbol-triangle"),
        SymbolShape::Diamond => tr!("plot-symbol-diamond"),
        SymbolShape::Plus => tr!("plot-symbol-plus"),
        SymbolShape::Cross => tr!("plot-symbol-cross"),
    }
}

/// Libellé d'une [`LegendPosition`].
fn legend_position_label(position: LegendPosition) -> String {
    match position {
        LegendPosition::TopLeft => tr!("plot-legend-top-left"),
        LegendPosition::TopRight => tr!("plot-legend-top-right"),
        LegendPosition::BottomLeft => tr!("plot-legend-bottom-left"),
        LegendPosition::BottomRight => tr!("plot-legend-bottom-right"),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use stokesforge_core::{Column, ColumnMeta, MetaLine, NodeId, Project};
    use stokesforge_plot::{
        LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, RawVariant, SeriesSpec,
        SymbolShape, THEMES,
    };
    use stokesforge_render_egui::formula;
    use stokesforge_session::Session;

    use super::{
        column_metadata_window, formula_field, formula_help, formula_memo_rows,
        legend_position_label, line_style_label, plot_settings_window, plot_type_label,
        symbol_shape_label, theme_editor,
    };
    use crate::color_picker::PickerEnv;
    use crate::i18n::{self, pseudo, Language, LANGUAGE_VARIABLE};
    use crate::state::{series_list_label, Action, AppState, PlotAxis, PlotSettingsTab};
    use crate::tr;

    use super::super::test_support::{
        cliquer, colonnes_xy, contexte, distincts, image, texte_exact, textes, touche, Texte,
    };

    // -- Outillage --------------------------------------------------------

    fn est_peint(textes: &[Texte], cherche: &str) -> bool {
        textes.iter().any(|t| t.texte == cherche)
    }

    fn clic(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// Le premier panneau, modifiable (`Panels` ne s'indexe qu'en lecture).
    fn panneau(spec: &mut PlotSpec) -> &mut PanelSpec {
        spec.panels.get_mut(0).unwrap()
    }

    /// Un graphe d'une série (B en fonction de A) dans une session, la
    /// fenêtre des propriétés fermée. `regler` ajuste le graphe avant que la
    /// session le prenne ; les champs gardent en vie ce dont l'état dépend.
    struct Banc {
        state: AppState,
        graph: NodeId,
        _session: Session,
    }

    fn banc(regler: impl FnOnce(&mut PlotSpec)) -> Banc {
        let mut project = Project::new("Propriétés").unwrap();
        let root = project.root();
        let (x, y) = colonnes_xy(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let mut spec = PlotSpec::default().with_panels(panel);
        regler(&mut spec);
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        Banc {
            state,
            graph,
            _session: session,
        }
    }

    impl Banc {
        /// Ouvre la fenêtre des propriétés sur `tab` (et `axis`), comme le
        /// clic droit ou le menu « Graphique ».
        fn ouvrir(&mut self, tab: PlotSettingsTab, axis: Option<PlotAxis>) {
            self.state.dispatch(Action::OpenPlotSettings {
                graph: self.graph,
                tab,
                axis,
            });
            assert_eq!(self.state.plot_settings().map(|s| s.tab), Some(tab));
        }

        /// Le graphe tel que le modèle le tient (pas le brouillon).
        fn modele(&self) -> PlotSpec {
            self.state.plot_spec(self.graph).unwrap()
        }

        /// Le brouillon de la fenêtre.
        fn brouillon(&self) -> PlotSpec {
            self.state.plot_edit().unwrap().spec.clone()
        }
    }

    /// Peint la fenêtre des propriétés trois fois (mise en page stabilisée,
    /// dialogue centré) puis une quatrième avec `events` ; rend les textes de
    /// la dernière image.
    fn peindre_fenetre(
        context: &egui::Context,
        state: &mut AppState,
        events: Vec<egui::Event>,
    ) -> Vec<Texte> {
        for _ in 0..3 {
            image(context, Vec::new(), |ui| {
                plot_settings_window(ui.ctx(), state)
            });
        }
        textes(&image(context, events, |ui| {
            plot_settings_window(ui.ctx(), state)
        }))
    }

    /// Clique sur le texte `libelle` de la fenêtre (une seule occurrence),
    /// puis rend les textes de l'image suivante.
    fn cliquer_libelle(context: &egui::Context, state: &mut AppState, libelle: &str) -> Vec<Texte> {
        let peints = peindre_fenetre(context, state, Vec::new());
        let pos = texte_exact(&peints, libelle).centre;
        cliquer(context, pos, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        peindre_fenetre(context, state, Vec::new())
    }

    // -- Chaque onglet, dans chaque langue ----------------------------------

    /// Un libellé propre à chaque onglet : peint sur sa page, absent des
    /// autres. Choisis pour ne coïncider avec aucun autre texte des six pages
    /// (« Axes » est à la fois l'onglet et une couleur du thème, « Line » à la
    /// fois une section et un type de tracé).
    fn libelles_propres() -> [(PlotSettingsTab, String); 6] {
        [
            (PlotSettingsTab::Graph, tr!("plot-general-dark-note")),
            (
                PlotSettingsTab::PlotArea,
                tr!("plot-area-gridlines-vertical"),
            ),
            (PlotSettingsTab::Axes, tr!("plot-axes-scale-section")),
            (PlotSettingsTab::Series, tr!("plot-series-line-width")),
            (PlotSettingsTab::Legend, tr!("plot-legend-show")),
            (PlotSettingsTab::Theme, tr!("plot-theme-global-colors")),
        ]
    }

    /// Issue #119 (captures avant/après) : chaque onglet se peint sans
    /// paniquer, montre la bande commune des six onglets, **son** contenu et
    /// pas celui des autres, et ne modifie rien — ni le modèle, ni le
    /// brouillon, ni l'historique, ni l'onglet choisi. En pseudo-langue
    /// (`STOKESFORGE_LANG=en-XA`), tout texte peint qui n'est pas une donnée
    /// passe par le catalogue : il est encadré de `[[ ]]` (un `]]` absent
    /// dirait un texte tronqué par la mise en page).
    ///
    /// La langue est celle du processus : c'est
    /// [`chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue`] qui le relance
    /// en sous-processus avec `en` puis `en-XA` (la langue est un état global :
    /// la changer ici troublerait les tests voisins). Il lit la ligne
    /// « onglets peints en … » pour savoir que les vérifications ont eu lieu
    /// dans la bonne langue.
    #[test]
    #[ignore = "lancé en sous-processus par chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue"]
    fn chaque_onglet_se_peint_sans_rien_modifier_dans_la_langue_du_processus() {
        let langue = i18n::language();
        let mut verifies = 0;
        for tab in PlotSettingsTab::ALL {
            // Une ordonnée de titre coloré différemment du titre du graphe :
            // l'onglet « Thème » ne doit pas les fondre en une seule couleur
            // par le seul fait de se peindre.
            let mut banc = banc(|spec| {
                spec.set_text_color("#336699");
                panneau(spec).left.title_color = "#aa0000".into();
                panneau(spec).left.tick_color = "#00aa00".into();
            });
            let historique = banc.state.undo_label();
            banc.ouvrir(tab, None);
            let original = banc.modele();
            let context = contexte();
            let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());

            // La bande défile quand elle ne tient pas (#19) : en anglais, les
            // six onglets tiennent ; en pseudo-langue, allongée de 30 %, seul
            // l'onglet choisi est sûr d'être visible.
            assert!(
                est_peint(&peints, &tab.label()),
                "{tab:?} : l'onglet choisi n'est pas visible dans la bande"
            );
            if langue == Language::English {
                for onglet in PlotSettingsTab::ALL {
                    assert!(
                        est_peint(&peints, &onglet.label()),
                        "{tab:?} : onglet {onglet:?} absent de la bande"
                    );
                }
            }
            for (proprietaire, libelle) in libelles_propres() {
                assert_eq!(
                    est_peint(&peints, &libelle),
                    proprietaire == tab,
                    "{tab:?} : {libelle:?} appartient à {proprietaire:?}"
                );
            }
            assert_eq!(banc.modele(), original, "{tab:?} : le modèle a changé");
            assert_eq!(
                banc.brouillon(),
                original,
                "{tab:?} : le brouillon a changé"
            );
            assert_eq!(banc.state.undo_label(), historique, "{tab:?} : historique");
            let settings = banc.state.plot_settings().expect("fenêtre restée ouverte");
            assert_eq!(settings.tab, tab);

            // Les données ne passent pas par le catalogue : noms des thèmes
            // (`stokesforge_plot::THEMES`, des données comme les noms saisis,
            // `docs/application.md` § Propriétés des graphiques),
            // libellé de la série tiré de ses colonnes, valeurs numériques,
            // champs de saisie vides (titres non renseignés).
            let tree = banc.state.tree().clone();
            let serie = series_list_label(&tree, &original.panels[0].series[0]);
            let donnee = |texte: &str| {
                texte.is_empty()
                    || THEMES.iter().any(|theme| theme.name == texte)
                    || texte == serie
                    || texte.parse::<f64>().is_ok()
            };
            for peint in &peints {
                if langue == Language::PseudoEnglish && !donnee(&peint.texte) {
                    assert!(
                        peint.texte.starts_with(pseudo::OPEN)
                            && peint.texte.ends_with(pseudo::CLOSE),
                        "{tab:?} : {:?} n'est pas passé par le catalogue (ou est tronqué)",
                        peint.texte
                    );
                } else {
                    assert!(
                        !peint.texte.contains(pseudo::OPEN),
                        "{tab:?} : {:?} en pseudo-langue hors de en-XA",
                        peint.texte
                    );
                }
                verifies += 1;
            }
        }
        println!("onglets peints en {} : {verifies} textes", langue.tag());
    }

    /// Relance [`chaque_onglet_se_peint_sans_rien_modifier_dans_la_langue_du_processus`]
    /// dans un processus fils par langue, `STOKESFORGE_LANG` posé : l'anglais,
    /// puis la pseudo-langue. Le fils doit réussir **et** dire qu'il a peint
    /// dans la langue demandée (témoin : un fils qui n'aurait rien lancé, ou
    /// dans une autre langue, ne l'écrirait pas).
    #[test]
    fn chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue() {
        let executable = std::env::current_exe().unwrap();
        let module = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, reste)| reste);
        let nom = format!(
            "{module}::chaque_onglet_se_peint_sans_rien_modifier_dans_la_langue_du_processus"
        );
        for langue in [Language::English, Language::PseudoEnglish] {
            let sortie = std::process::Command::new(&executable)
                .args([
                    nom.as_str(),
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(LANGUAGE_VARIABLE, langue.tag())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&sortie.stdout);
            let stderr = String::from_utf8_lossy(&sortie.stderr);
            assert!(
                sortie.status.success(),
                "{} :\n{stdout}\n{stderr}",
                langue.tag()
            );
            assert!(
                stdout.contains(&format!("onglets peints en {} : ", langue.tag())),
                "{} : le fils n'a pas peint dans cette langue :\n{stdout}",
                langue.tag()
            );
        }
    }

    // -- La bande d'onglets et l'onglet des axes ----------------------------

    /// #19 : les onglets des propriétés sont la bande commune (`tab_strip`) ;
    /// un clic sur l'un d'eux change la page (témoin : tant qu'on ne clique
    /// pas, la page reste la même, image après image).
    #[test]
    fn un_clic_sur_un_onglet_des_proprietes_change_de_page() {
        let mut banc = banc(|_| {});
        banc.ouvrir(PlotSettingsTab::Graph, None);
        let context = contexte();
        let axes = tr!("plot-axes-scale-section");
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(!est_peint(&peints, &axes));
        assert_eq!(
            banc.state.plot_settings().unwrap().tab,
            PlotSettingsTab::Graph
        );

        let peints = cliquer_libelle(&context, &mut banc.state, &PlotSettingsTab::Axes.label());
        assert_eq!(
            banc.state.plot_settings().unwrap().tab,
            PlotSettingsTab::Axes
        );
        assert!(est_peint(&peints, &axes), "la page des axes est peinte");
    }

    /// L'onglet « Axes » propose X, Y et les deux axes miroirs (#176) ; un
    /// calque superposé n'a pas de miroir. Un clic sur un axe le choisit, sans
    /// rien modifier.
    #[test]
    fn l_onglet_des_axes_choisit_l_axe_et_propose_les_miroirs() {
        let mut banc = banc(|_| {});
        banc.ouvrir(PlotSettingsTab::Axes, None);
        let context = contexte();
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, &PlotAxis::Bottom.label()));
        assert!(est_peint(&peints, &PlotAxis::Left.label()));
        assert!(est_peint(&peints, &PlotAxis::Right.label()));
        assert!(est_peint(&peints, &PlotAxis::Top.label()));
        assert_eq!(banc.state.plot_settings().unwrap().axis, PlotAxis::Bottom);

        let original = banc.modele();
        cliquer_libelle(&context, &mut banc.state, &PlotAxis::Left.label());
        assert_eq!(banc.state.plot_settings().unwrap().axis, PlotAxis::Left);
        assert_eq!(banc.modele(), original);

        banc.ouvrir(PlotSettingsTab::Axes, Some(PlotAxis::Right));
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, &PlotAxis::Right.label()));
    }

    /// #176 : l'onglet d'un axe miroir ne propose ni titre ni couleur, seulement
    /// de l'afficher et de garder ou non les étiquettes de graduation.
    #[test]
    fn l_onglet_d_un_axe_miroir_ne_propose_que_l_affichage() {
        let mut banc = banc(|_| {});
        banc.ouvrir(PlotSettingsTab::Axes, Some(PlotAxis::Top));
        let context = contexte();
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, &tr!("plot-axes-show-mirror")));
        assert!(est_peint(&peints, &tr!("plot-axes-show-tick-labels")));
        assert!(!est_peint(&peints, &tr!("plot-axes-quantity")));
        assert!(!est_peint(&peints, &tr!("plot-axes-color")));

        banc.ouvrir(PlotSettingsTab::Axes, Some(PlotAxis::Bottom));
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, &tr!("plot-axes-quantity")));
        assert!(!est_peint(&peints, &tr!("plot-axes-show-mirror")));
    }

    /// #124 : la page choisit le calque du brouillon ; le premier calque
    /// sert de témoin qui doit rester inchangé après le clic sur la grille.
    #[test]
    fn la_page_du_calque_valide_uniquement_le_calque_choisi() {
        let mut banc = banc(|spec| {
            panneau(spec).bottom.grid = false;
            let mut second = PanelSpec::at(0, 1);
            second.title = "Calque choisi".into();
            second.bottom.grid = false;
            spec.panels.push(second);
        });
        banc.ouvrir(PlotSettingsTab::PlotArea, None);
        banc.state.plot_edit_mut().unwrap().selected_panel = usize::MAX;
        let original = banc.modele();
        let context = contexte();
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, "Calque choisi"));
        assert_eq!(banc.state.plot_edit().unwrap().selected_panel, 1);
        assert_eq!(banc.modele(), original, "le choix seul ne valide rien");

        cliquer_libelle(
            &context,
            &mut banc.state,
            &tr!("plot-area-gridlines-vertical"),
        );
        let apres = banc.modele();
        assert_eq!(apres.panels[0], original.panels[0]);
        assert!(
            apres.panels[1].bottom.grid,
            "le clic a validé le calque choisi"
        );
        banc.state.close_plot_settings();
        banc.state.dispatch(Action::Undo);
        assert_eq!(banc.modele(), original, "la validation reste annulable");
    }

    /// #169 : l'axe choisi est l'onglet actif d'une bande `tab_strip` (gras
    /// et accent), la série choisie une ligne de liste en gras, et les
    /// sections « Line » / « Symbol » des titres en gras — plus des boutons
    /// sélectionnables ni des libellés ordinaires précédés d'un filet.
    /// Témoins : l'axe et la série non choisis restent en graisse normale.
    #[test]
    fn l_axe_choisi_est_un_onglet_et_la_serie_choisie_une_ligne_de_liste() {
        let mut banc = banc(|spec| {
            let panel = panneau(spec);
            panel.series[0].name = "Première".into();
            let mut seconde = panel.series[0].clone();
            seconde.name = "Seconde".into();
            panel.series.push(seconde);
        });
        let context = contexte();
        banc.ouvrir(PlotSettingsTab::Axes, Some(PlotAxis::Left));
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        let gauche = texte_exact(&peints, &PlotAxis::Left.label());
        assert!(gauche.gras, "l'axe choisi est en gras");
        assert_eq!(gauche.couleur, crate::theme::accent(), "et dans l'accent");
        let bas = texte_exact(&peints, &PlotAxis::Bottom.label());
        assert!(!bas.gras, "témoin : l'axe non choisi");
        assert_ne!(bas.couleur, crate::theme::accent());

        banc.ouvrir(PlotSettingsTab::Series, None);
        banc.state.plot_edit_mut().unwrap().selected_series = 1;
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        let spec = banc.brouillon();
        let tree = banc.state.tree();
        let libelle = |index: usize| series_list_label(tree, &spec.panels[0].series[index]);
        let lignes = |index: usize| -> Vec<&Texte> {
            peints
                .iter()
                .filter(|t| t.texte == libelle(index))
                .collect()
        };
        assert!(
            lignes(1).iter().any(|t| t.gras),
            "la série choisie est une ligne en gras : {:?}",
            lignes(1)
        );
        assert!(
            lignes(0).iter().all(|t| !t.gras),
            "témoin : la série non choisie"
        );
        for section in [
            tr!("plot-series-line-section"),
            tr!("plot-series-symbol-section"),
        ] {
            assert!(
                peints.iter().any(|t| t.texte == section && t.gras),
                "{section} : un titre de section en gras"
            );
        }
    }

    /// #124 : le retour anticipé sur une liste vide doit laisser le
    /// dialogue ouvert ; une liste non vide borne la sélection sans écrire
    /// le graphe, puis son éditeur ne modifie que la série choisie.
    #[test]
    fn la_page_des_series_borne_la_selection_et_garde_le_cas_vide() {
        let mut vide = banc(|spec| panneau(spec).series.clear());
        vide.ouvrir(PlotSettingsTab::Series, None);
        vide.state.plot_edit_mut().unwrap().selected_series = usize::MAX;
        let original = vide.modele();
        let peints = peindre_fenetre(&contexte(), &mut vide.state, Vec::new());
        assert!(est_peint(&peints, &tr!("plot-series-empty")));
        assert!(!est_peint(&peints, &tr!("plot-series-show")));
        assert_eq!(vide.modele(), original);
        assert!(vide.state.plot_settings().is_some());
        assert_eq!(vide.state.plot_edit().unwrap().selected_series, usize::MAX);

        let mut banc = banc(|spec| {
            let panel = panneau(spec);
            panel.series[0].name = "Première".into();
            let mut seconde = panel.series[0].clone();
            seconde.name = "Seconde".into();
            panel.series.push(seconde);
        });
        banc.ouvrir(PlotSettingsTab::Series, None);
        banc.state.plot_edit_mut().unwrap().selected_series = usize::MAX;
        let original = banc.modele();
        let context = contexte();
        peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert_eq!(banc.state.plot_edit().unwrap().selected_series, 1);
        assert_eq!(banc.modele(), original);
        cliquer_libelle(&context, &mut banc.state, &tr!("plot-series-show"));
        let apres = banc.modele();
        assert_eq!(apres.panels[0].series[0], original.panels[0].series[0]);
        assert!(!apres.panels[0].series[1].visible);
        assert_eq!(apres.panels[0].series[1].name, "Seconde");
    }

    /// #177–#179 : les séries ont leur fenêtre — la bande du graphe ne les
    /// propose plus — qui montre le groupe de la série choisie, et « Appliquer
    /// au groupe » répartit le nuancier sur ses seules séries, jusqu'au modèle.
    #[test]
    fn la_fenetre_des_series_regle_un_groupe_et_lui_applique_un_nuancier() {
        let mut banc = banc(|spec| {
            let panel = panneau(spec);
            let modele = panel.series[0].clone();
            panel.series.clear();
            panel.push_series_batch(vec![modele.clone(), modele.clone()]);
            panel.push_series_batch(vec![modele]);
        });
        let context = contexte();

        banc.ouvrir(PlotSettingsTab::Graph, None);
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(!est_peint(&peints, &PlotSettingsTab::Series.label()));

        banc.ouvrir(PlotSettingsTab::Series, None);
        banc.state.plot_edit_mut().unwrap().selected_series = 1;
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(!est_peint(&peints, &PlotSettingsTab::Graph.label()));
        assert!(est_peint(&peints, &tr!("curve-group-section")));
        assert!(est_peint(&peints, &tr!("curve-scope-group-note")));
        let original = banc.modele();

        cliquer_libelle(&context, &mut banc.state, &tr!("curve-gradient-apply"));
        let apres = banc.modele();
        let couleurs: Vec<&str> = apres.panels[0]
            .series
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(couleurs[..2], ["#0000ff", "#ff0000"]);
        assert_eq!(
            apres.panels[0].series[2], original.panels[0].series[2],
            "la série d'un autre groupe n'est pas touchée"
        );
    }

    /// #177 : portée « tout le groupe » — les réglages propres à une série sont
    /// grisés (le clic ne fait rien) ; portée « cette série seulement » — ils
    /// se rouvrent et ne touchent que la série choisie.
    #[test]
    fn la_portee_grise_les_reglages_propres_a_une_serie() {
        let mut banc = banc(|spec| {
            let panel = panneau(spec);
            let modele = panel.series[0].clone();
            panel.series.clear();
            panel.push_series_batch(vec![modele.clone(), modele]);
        });
        let context = contexte();
        banc.ouvrir(PlotSettingsTab::Series, None);
        banc.state.plot_edit_mut().unwrap().selected_series = 1;
        let original = banc.modele();

        cliquer_libelle(&context, &mut banc.state, &tr!("plot-series-show"));
        assert_eq!(banc.modele(), original, "grisé : le clic ne change rien");

        cliquer_libelle(&context, &mut banc.state, &tr!("curve-scope-series"));
        cliquer_libelle(&context, &mut banc.state, &tr!("plot-series-show"));
        let apres = banc.modele();
        assert!(
            apres.panels[0].series[0].visible,
            "l'autre membre est intact"
        );
        assert!(!apres.panels[0].series[1].visible);
        assert_eq!(
            apres.panels[0].series[1].group,
            Some(1),
            "reste dans son groupe"
        );
    }

    /// #124 : vérifie le retour de la page du graphe jusqu'à la validation
    /// commune, en plus du test local de `formula_field`. Une frappe doit
    /// seulement changer le brouillon, puis un clic ailleurs dans la page
    /// doit atteindre le modèle sans fermer le dialogue.
    #[test]
    fn le_titre_du_graphe_se_valide_apres_la_perte_de_focus() {
        let mut banc = banc(|spec| spec.title = "Titre initial".into());
        banc.ouvrir(PlotSettingsTab::Graph, None);
        let original = banc.modele();
        let context = contexte();
        cliquer_libelle(&context, &mut banc.state, "Titre initial");
        image(&context, vec![egui::Event::Text("Z".into())], |ui| {
            plot_settings_window(ui.ctx(), &mut banc.state);
        });
        let saisi = banc.brouillon().title;
        assert!(saisi.contains('Z'), "la frappe a atteint le brouillon");
        assert_eq!(
            banc.modele(),
            original,
            "aucune validation pendant la frappe"
        );
        cliquer_libelle(&context, &mut banc.state, &tr!("plot-general-dark-note"));
        assert_eq!(banc.modele().title, saisi);
        assert!(
            banc.state.plot_settings().is_some(),
            "la validation vient de la page ouverte"
        );
        banc.state.close_plot_settings();
        banc.state.dispatch(Action::Undo);
        assert_eq!(banc.modele(), original);
    }

    // -- Quand le brouillon est validé --------------------------------------

    /// Clique dans le champ numérique qui affiche `valeur` (focus, tout le texte
    /// sélectionné), puis rend le centre du champ pour la suite du geste.
    fn focaliser(context: &egui::Context, state: &mut AppState, valeur: &str) -> egui::Pos2 {
        let peints = peindre_fenetre(context, state, Vec::new());
        let centre = texte_exact(&peints, valeur).centre;
        cliquer(context, centre, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        centre
    }

    fn une_image(context: &egui::Context, state: &mut AppState, events: Vec<egui::Event>) {
        image(context, events, |ui| plot_settings_window(ui.ctx(), state));
    }

    /// Un nombre tapé ne valide le brouillon qu'à Entrée (N3) — tant que le
    /// champ est ouvert, ni le brouillon ni le modèle ne bougent ; à Entrée,
    /// le modèle reprend le brouillon. Épaisseur du trait (série) et borne
    /// minimale (axe).
    #[test]
    fn un_nombre_tape_n_est_valide_qu_a_entree() {
        struct Cas {
            tab: PlotSettingsTab,
            axis: Option<PlotAxis>,
            valeur: &'static str,
            saisi: &'static str,
            attendu: f64,
            lire: fn(&PlotSpec) -> f64,
        }
        let cas = [
            Cas {
                tab: PlotSettingsTab::Series,
                axis: None,
                valeur: "2.5",
                saisi: "3.5",
                attendu: 3.5,
                lire: |s| s.panels[0].series[0].line.width,
            },
            Cas {
                tab: PlotSettingsTab::Axes,
                axis: Some(PlotAxis::Left),
                valeur: "-3.5",
                saisi: "-1",
                attendu: -1.0,
                lire: |s| s.panels[0].left.range_min.unwrap(),
            },
        ];
        for Cas {
            tab,
            axis,
            valeur,
            saisi,
            attendu,
            lire,
        } in cas
        {
            let mut banc = banc(|spec| {
                panneau(spec).series[0].line.width = 2.5;
                let left = &mut panneau(spec).left;
                left.auto_range = false;
                left.range_min = Some(-3.5);
                left.range_max = Some(12.5);
            });
            banc.ouvrir(tab, axis);
            let avant = lire(&banc.modele());
            assert_eq!(Ok(avant), valeur.parse(), "{tab:?} : la valeur affichée");
            let context = contexte();
            focaliser(&context, &mut banc.state, valeur);
            une_image(
                &context,
                &mut banc.state,
                vec![egui::Event::Text(saisi.into())],
            );
            une_image(&context, &mut banc.state, Vec::new());
            assert_eq!(
                lire(&banc.brouillon()),
                avant,
                "{tab:?} : validé à la frappe"
            );
            assert_eq!(lire(&banc.modele()), avant, "{tab:?} : modèle touché");

            une_image(&context, &mut banc.state, vec![touche(egui::Key::Enter)]);
            assert_eq!(lire(&banc.brouillon()), attendu, "{tab:?} : Entrée");
            assert_eq!(lire(&banc.modele()), attendu, "{tab:?} : modèle");
        }
    }

    /// Molette sur le champ (focus et curseur dessus) : un pas par cran ; trois
    /// crans d'affilée ne font qu'**une** entrée d'historique, un seul Annuler
    /// rend la valeur d'avant.
    #[test]
    fn des_crans_de_molette_ne_font_qu_une_entree_d_historique() {
        let mut banc = banc(|spec| panneau(spec).series[0].line.width = 2.5);
        banc.ouvrir(PlotSettingsTab::Series, None);
        let context = contexte();
        let original = banc.modele();
        focaliser(&context, &mut banc.state, "2.5");
        let cran = || {
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        for _ in 0..3 {
            une_image(&context, &mut banc.state, cran());
        }
        une_image(&context, &mut banc.state, Vec::new());
        let largeur = |spec: &PlotSpec| spec.panels[0].series[0].line.width;
        assert!((largeur(&banc.brouillon()) - 2.8).abs() < 1e-9);
        banc.state.close_plot_settings();
        assert!((largeur(&banc.modele()) - 2.8).abs() < 1e-9);
        banc.state.dispatch(Action::Undo);
        assert_eq!(banc.modele(), original, "un seul Annuler");
    }

    /// F7 : le premier Échap rend au champ sa valeur (le dialogue reste ouvert),
    /// le suivant fait Annuler du dialogue.
    #[test]
    fn echap_rend_la_valeur_puis_annule_le_dialogue() {
        let mut banc = banc(|spec| panneau(spec).series[0].line.width = 2.5);
        banc.ouvrir(PlotSettingsTab::Series, None);
        let context = contexte();
        focaliser(&context, &mut banc.state, "2.5");
        une_image(
            &context,
            &mut banc.state,
            vec![egui::Event::Text("9".into())],
        );
        une_image(&context, &mut banc.state, vec![touche(egui::Key::Escape)]);
        une_image(&context, &mut banc.state, Vec::new());
        assert!(
            banc.state.plot_settings().is_some(),
            "premier Échap : le dialogue reste"
        );
        assert_eq!(banc.brouillon().panels[0].series[0].line.width, 2.5);
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, "2.5"), "la valeur est rendue au champ");

        une_image(&context, &mut banc.state, vec![touche(egui::Key::Escape)]);
        assert!(
            banc.state.plot_settings().is_none(),
            "second Échap : Annuler"
        );
    }

    /// Sans focus, la molette ne change pas le champ : elle reste à la fenêtre.
    #[test]
    fn la_molette_sans_focus_ne_change_pas_le_champ() {
        let mut banc = banc(|spec| panneau(spec).series[0].line.width = 2.5);
        banc.ouvrir(PlotSettingsTab::Series, None);
        let context = contexte();
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        let centre = texte_exact(&peints, "2.5").centre;
        une_image(
            &context,
            &mut banc.state,
            vec![
                egui::Event::PointerMoved(centre),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 1.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        une_image(&context, &mut banc.state, Vec::new());
        assert_eq!(banc.brouillon().panels[0].series[0].line.width, 2.5);
    }

    /// Doc de [`plot_settings_window`] : une case à cocher valide tout de
    /// suite, au clic.
    #[test]
    fn une_case_a_cocher_valide_au_clic() {
        let mut banc = banc(|_| {});
        banc.ouvrir(PlotSettingsTab::Legend, None);
        let context = contexte();
        let avant = banc.modele().panels[0].legend.visible;
        peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert_eq!(banc.modele().panels[0].legend.visible, avant);
        cliquer_libelle(&context, &mut banc.state, &tr!("plot-legend-show"));
        assert_eq!(banc.modele().panels[0].legend.visible, !avant);
    }

    /// Le résultat de chaque image de [`formula_field`] sur `valeur`, et ses
    /// textes peints à la dernière.
    fn champ(
        context: &egui::Context,
        valeur: &mut String,
        images: Vec<Vec<egui::Event>>,
    ) -> (Vec<bool>, Vec<Texte>) {
        let mut rendus = Vec::new();
        let mut peints = Vec::new();
        for events in images {
            peints = textes(&image(context, events, |ui| {
                rendus.push(formula_field(ui, "Grandeur", "Aide", valeur));
            }));
        }
        (rendus, peints)
    }

    /// [`formula_field`] ne valide qu'à la perte de focus — Entrée ou clic
    /// ailleurs —, jamais à chaque frappe : la saisie arrive dans la valeur
    /// (aperçu) sans demander de validation.
    #[test]
    fn un_champ_texte_ne_valide_qu_a_la_perte_de_focus() {
        let context = contexte();
        let mut valeur = String::from("abc");
        let (rendus, peints) = champ(&context, &mut valeur, vec![vec![], vec![]]);
        assert_eq!(rendus, [false, false]);
        assert!(est_peint(&peints, "Grandeur"), "le libellé est peint");
        let dedans = texte_exact(&peints, "abc").centre;
        let dehors = egui::pos2(1000.0, 800.0);
        let focaliser = vec![
            vec![egui::Event::PointerMoved(dedans)],
            vec![clic(dedans, true)],
            vec![clic(dedans, false)],
        ];

        // Entrée.
        let (rendus, _) = champ(&context, &mut valeur, focaliser.clone());
        assert_eq!(rendus, [false; 3], "prendre le focus ne valide pas");
        let (rendus, _) = champ(
            &context,
            &mut valeur,
            vec![vec![egui::Event::Text("Z".into())], vec![]],
        );
        assert_eq!(rendus, [false, false], "une frappe ne valide pas");
        assert!(
            valeur.contains('Z'),
            "la frappe atteint la valeur : {valeur:?}"
        );
        // egui 0.36 signale la perte de focus sur deux images (`lost_focus` :
        // focus au début de cette image *ou* de la précédente) ; valider deux
        // fois est sans effet (`AppState::commit_plot_edit` ne valide qu'un
        // brouillon qui diverge du modèle).
        let (rendus, _) = champ(
            &context,
            &mut valeur,
            vec![vec![touche(egui::Key::Enter)], vec![], vec![]],
        );
        assert!(rendus[0], "Entrée valide tout de suite : {rendus:?}");
        assert!(!rendus[2], "puis plus rien : {rendus:?}");

        // Clic ailleurs.
        let (rendus, _) = champ(&context, &mut valeur, focaliser);
        assert_eq!(rendus, [false; 3]);
        let (rendus, _) = champ(
            &context,
            &mut valeur,
            vec![vec![egui::Event::Text("Y".into())]],
        );
        assert_eq!(rendus, [false]);
        let (rendus, _) = champ(
            &context,
            &mut valeur,
            vec![
                vec![egui::Event::PointerMoved(dehors)],
                vec![clic(dehors, true)],
                vec![clic(dehors, false)],
                vec![],
                vec![],
            ],
        );
        assert!(
            rendus.contains(&true),
            "le clic ailleurs valide : {rendus:?}"
        );
        assert_eq!(rendus.last(), Some(&false), "puis plus rien : {rendus:?}");
        assert!(valeur.contains('Y'));
    }

    // -- L'onglet des séries --------------------------------------------------

    /// Ouvre la liste déroulante dont le texte affiché est `affiche`, dans
    /// l'onglet ouvert ; rend les textes peints liste ouverte et la position
    /// du bouton fermé.
    fn ouvrir_liste(
        context: &egui::Context,
        state: &mut AppState,
        affiche: &str,
    ) -> (Vec<Texte>, egui::Pos2) {
        let peints = peindre_fenetre(context, state, Vec::new());
        let bouton = texte_exact(&peints, affiche).centre;
        cliquer(context, bouton, egui::PointerButton::Primary, |ui| {
            plot_settings_window(ui.ctx(), state)
        });
        (peindre_fenetre(context, state, Vec::new()), bouton)
    }

    /// Chaque liste déroulante des propriétés propose **toutes** ses
    /// variantes connues (le type inconnu jamais, voir [`series_editor`]), et
    /// un choix y est validé tout de suite.
    #[test]
    fn chaque_liste_deroulante_propose_toutes_ses_variantes() {
        type Lire = fn(&PlotSpec) -> String;
        let cas: [(PlotSettingsTab, String, Vec<String>, String, Lire); 4] = [
            (
                PlotSettingsTab::Series,
                plot_type_label(&PlotType::Scatter),
                types_connus().iter().map(plot_type_label).collect(),
                plot_type_label(&PlotType::Bar),
                |s| format!("{:?}", s.panels[0].series[0].plot_type),
            ),
            (
                PlotSettingsTab::Series,
                line_style_label(LineStyle::Solid),
                styles().into_iter().map(line_style_label).collect(),
                line_style_label(LineStyle::Dash),
                |s| format!("{:?}", s.panels[0].series[0].line.style),
            ),
            (
                PlotSettingsTab::Series,
                symbol_shape_label(SymbolShape::Circle),
                formes().into_iter().map(symbol_shape_label).collect(),
                symbol_shape_label(SymbolShape::Diamond),
                |s| format!("{:?}", s.panels[0].series[0].symbol.shape),
            ),
            (
                PlotSettingsTab::Legend,
                legend_position_label(LegendPosition::TopRight),
                positions().into_iter().map(legend_position_label).collect(),
                legend_position_label(LegendPosition::BottomLeft),
                |s| format!("{:?}", s.panels[0].legend.position),
            ),
        ];
        let attendus = ["Bar", "Dash", "Diamond", "BottomLeft"];
        for ((tab, affiche, proposes, choix, lire), attendu) in cas.into_iter().zip(attendus) {
            let mut banc = banc(|spec| {
                let panel = panneau(spec);
                panel.series[0].plot_type = PlotType::Scatter;
                panel.series[0].line.style = LineStyle::Solid;
                panel.series[0].symbol.shape = SymbolShape::Circle;
                panel.legend.position = LegendPosition::TopRight;
            });
            banc.ouvrir(tab, None);
            let context = contexte();
            let (peints, bouton) = ouvrir_liste(&context, &mut banc.state, &affiche);
            for propose in &proposes {
                assert!(
                    peints
                        .iter()
                        .any(|t| &t.texte == propose && t.centre != bouton),
                    "{affiche:?} : {propose:?} absent de la liste ouverte"
                );
            }
            assert!(!est_peint(&peints, &plot_type_label(&inconnu())));
            assert_ne!(lire(&banc.modele()), attendu, "témoin : pas encore choisi");
            let pos = texte_exact(&peints, &choix).centre;
            cliquer(&context, pos, egui::PointerButton::Primary, |ui| {
                plot_settings_window(ui.ctx(), &mut banc.state)
            });
            assert_eq!(lire(&banc.modele()), attendu, "{affiche:?} : choix validé");
        }
    }

    /// C5 (`guide-de-correction-robustesse.md` §F3) : une série d'un type
    /// inconnu (version plus récente) le montre en lecture seule, sans liste
    /// déroulante — rien ne sait le reconstruire —, et le reste de la série
    /// reste éditable.
    #[test]
    fn un_type_de_trace_inconnu_s_affiche_en_lecture_seule() {
        let mut banc = banc(|spec| panneau(spec).series[0].plot_type = inconnu());
        banc.ouvrir(PlotSettingsTab::Series, None);
        let context = contexte();
        let peints = peindre_fenetre(&context, &mut banc.state, Vec::new());
        assert!(est_peint(&peints, &tr!("plot-series-type-unknown")));
        assert!(!est_peint(&peints, &plot_type_label(&inconnu())));
        // Aucune liste des types : aucun libellé de type connu n'est peint
        // (hors « Line », qui est aussi le titre de la section du trait).
        let section = tr!("plot-series-line-section");
        for connu in types_connus() {
            let libelle = plot_type_label(&connu);
            if libelle != section {
                assert!(!est_peint(&peints, &libelle), "{connu:?}");
            }
        }
        assert!(est_peint(&peints, &tr!("plot-series-show")));
        let peints = cliquer_libelle(&context, &mut banc.state, &tr!("plot-series-show"));
        assert!(
            !banc.modele().panels[0].series[0].visible,
            "toujours éditable"
        );
        assert_eq!(banc.modele().panels[0].series[0].plot_type, inconnu());
        assert!(est_peint(&peints, &tr!("plot-series-type-unknown")));
    }

    // -- L'onglet « Thème » ---------------------------------------------------

    /// Peint [`theme_editor`] sur `spec` ; rend le dernier résultat et les
    /// textes.
    fn peindre_theme(
        context: &egui::Context,
        spec: &mut PlotSpec,
        events: Vec<egui::Event>,
    ) -> (bool, Vec<Texte>) {
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let mut commit = true;
        let peints = textes(&image(context, events, |ui| {
            commit = theme_editor(ui, spec, &mut env);
        }));
        assert!(
            env.requests.is_empty(),
            "peindre n'enregistre aucune couleur"
        );
        (commit, peints)
    }

    fn peindre_axe(
        context: &egui::Context,
        axis: &mut stokesforge_plot::AxisSpec,
        calcule: Option<(f64, f64)>,
    ) -> (bool, Vec<Texte>) {
        let mut env = PickerEnv {
            project: Default::default(),
            user: Default::default(),
            requests: Vec::new(),
        };
        let mut commit = true;
        let peints = textes(&image(context, Vec::new(), |ui| {
            commit = super::axis_editor(ui, axis, calcule, &mut env);
        }));
        (commit, peints)
    }

    /// #221 : sous « Auto range », Min et Max restent visibles (grisés) et
    /// montrent les bornes calculées, sans les écrire dans le spec. Témoin :
    /// hors Auto, ils montrent les bornes saisies et non les calculées.
    #[test]
    fn sous_auto_range_min_et_max_montrent_les_bornes_calculees() {
        let context = contexte();
        let mut axe = stokesforge_plot::AxisSpec::default();
        assert!(axe.auto_range);
        let (commit, peints) = peindre_axe(&context, &mut axe, Some((0.0, 4000.0)));
        assert!(!commit, "peindre ne valide rien");
        assert!(est_peint(&peints, &tr!("plot-axes-min")));
        assert!(est_peint(&peints, "4000"), "{peints:?}");
        assert_eq!((axe.range_min, axe.range_max), (None, None));

        axe.auto_range = false;
        axe.range_min = Some(-5.0);
        axe.range_max = Some(7.0);
        let (_, peints) = peindre_axe(&context, &mut axe, Some((0.0, 4000.0)));
        assert!(est_peint(&peints, "7"), "{peints:?}");
        assert!(!est_peint(&peints, "4000"), "{peints:?}");
    }

    /// #11 : l'onglet « Thème » se dessine (thèmes, fond, texte, axes) sans
    /// rien modifier tant qu'on n'agit pas — même quand les couleurs locales
    /// divergent de celles qu'il montre (le titre d'un axe, l'échelle d'un
    /// autre). Témoin : un clic sur un thème, lui, l'applique et valide.
    #[test]
    fn l_onglet_theme_se_dessine_sans_rien_modifier() {
        let mut spec = PlotSpec::empty("Figure").with_panels(PanelSpec::at(0, 0));
        spec.set_text_color("#336699");
        panneau(&mut spec).left.title_color = "#aa0000".into();
        panneau(&mut spec).left.tick_color = "#00aa00".into();
        let avant = spec.clone();
        let context = contexte();
        let (commit, peints) = peindre_theme(&context, &mut spec, Vec::new());
        assert!(!commit);
        assert_eq!(spec, avant);
        for libelle in [
            tr!("plot-theme-themes"),
            tr!("plot-theme-global-colors"),
            tr!("plot-theme-background"),
            tr!("plot-theme-text"),
            tr!("plot-theme-axes"),
            tr!("plot-theme-note"),
        ] {
            assert!(est_peint(&peints, &libelle), "{libelle:?} absent");
        }
        for theme in &THEMES {
            assert!(
                est_peint(&peints, theme.name),
                "thème {:?} absent",
                theme.name
            );
        }

        let sombre = &THEMES[1];
        assert_ne!(spec.background, sombre.background);
        let pos = texte_exact(&peints, sombre.name).centre;
        let mut commits = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![clic(pos, true)],
            vec![clic(pos, false)],
        ] {
            commits.push(peindre_theme(&context, &mut spec, events).0);
        }
        assert_eq!(commits, [false, false, true], "le clic valide une fois");
        assert_eq!(spec.background, sombre.background);
        assert_eq!(spec.panels[0].left.title_color, sombre.text);
    }

    // -- Le mémo des formules -------------------------------------------------

    /// Chaque exemple du mémo de syntaxe compile vraiment en Typst : un mémo
    /// qui montre une syntaxe fausse enseignerait une erreur. Les effets
    /// décrits sont tous distincts.
    #[test]
    fn chaque_exemple_du_memo_compile() {
        let fonts = formula::resolve_fonts(&stokesforge_render_egui::FontChoice::default())
            .expect("polices");
        let lignes = formula_memo_rows();
        for (source, _) in &lignes {
            for example in source.split("  ") {
                let (sans, math) = fonts.clone();
                formula::compile(example, 14.0, [0, 0, 0, 255], sans, math)
                    .unwrap_or_else(|error| panic!("{example:?} ne compile pas : {error}"));
            }
        }
        distincts(&lignes, |(_, effet)| effet.clone());
        distincts(&lignes, |(source, _)| source.to_string());
    }

    /// #33, #197 : le rappel de syntaxe n'est pas peint au repos — l'icône
    /// seule l'est, tout à droite du champ — et le survol de l'icône montre
    /// toutes ses lignes (ce qu'on tape, ce qu'on obtient) et un pied de page
    /// qui nomme Typst.
    #[test]
    fn le_rappel_des_formules_s_affiche_au_survol_de_l_icone() {
        let context = contexte();
        context.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let icone = std::cell::Cell::new(egui::Rect::NOTHING);
        let champ = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut texte = String::new();
        let mut peindre = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                champ.set(ui.text_edit_singleline(&mut texte).rect);
                icone.set(hover_help_pour_test(ui));
            });
        };
        let peints = textes(&image(&context, Vec::new(), &mut peindre));
        let pied = tr!("plot-formula-footer", url = "typst.app/docs/reference/math");
        for (source, effet) in formula_memo_rows() {
            assert!(!est_peint(&peints, source), "au repos : {source:?} peint");
            assert!(!est_peint(&peints, &effet));
        }
        assert!(!est_peint(&peints, &pied));
        assert!(
            icone.get().left() >= champ.get().right(),
            "l'icône est à droite du champ : {:?} contre {:?}",
            icone.get(),
            champ.get()
        );

        let survol = vec![egui::Event::PointerMoved(icone.get().center())];
        let mut peints = Vec::new();
        for _ in 0..4 {
            peints = textes(&image(&context, survol.clone(), &mut peindre));
        }
        for (source, effet) in formula_memo_rows() {
            assert!(est_peint(&peints, source), "survolé : {source:?} absent");
            assert!(est_peint(&peints, &effet), "survolé : {effet:?} absent");
        }
        assert!(est_peint(&peints, &pied));
        assert!(pied.contains("Typst"), "le rappel nomme Typst");
    }

    /// Peint l'icône d'aide des formules et rend son rectangle.
    fn hover_help_pour_test(ui: &mut egui::Ui) -> egui::Rect {
        let avant = ui.cursor().min;
        formula_help(ui);
        egui::Rect::from_min_max(avant, ui.min_rect().max)
    }

    // -- Les libellés des choix ---------------------------------------------

    fn inconnu() -> PlotType {
        PlotType::Unknown(RawVariant {
            name: "Violin".into(),
            payload: Vec::new(),
        })
    }

    /// Les types de tracé proposés par la liste : tous, sauf l'inconnu. Le
    /// `match` sans `_` oblige à revenir ici quand une variante s'ajoute.
    fn types_connus() -> Vec<PlotType> {
        let tous = [
            PlotType::Scatter,
            PlotType::Line,
            PlotType::LineScatter,
            PlotType::Bar,
            PlotType::Histogram,
            PlotType::ErrorBar,
        ];
        for plot_type in tous.iter().cloned().chain([inconnu()]) {
            match plot_type {
                PlotType::Scatter
                | PlotType::Line
                | PlotType::LineScatter
                | PlotType::Bar
                | PlotType::Histogram
                | PlotType::ErrorBar
                | PlotType::Unknown(_) => {}
            }
        }
        tous.to_vec()
    }

    fn styles() -> [LineStyle; 5] {
        let tous = [
            LineStyle::None,
            LineStyle::Solid,
            LineStyle::Dash,
            LineStyle::Dot,
            LineStyle::DashDot,
        ];
        for style in tous {
            match style {
                LineStyle::None
                | LineStyle::Solid
                | LineStyle::Dash
                | LineStyle::Dot
                | LineStyle::DashDot => {}
            }
        }
        tous
    }

    fn formes() -> [SymbolShape; 7] {
        let toutes = [
            SymbolShape::None,
            SymbolShape::Circle,
            SymbolShape::Square,
            SymbolShape::Triangle,
            SymbolShape::Diamond,
            SymbolShape::Plus,
            SymbolShape::Cross,
        ];
        for forme in toutes {
            match forme {
                SymbolShape::None
                | SymbolShape::Circle
                | SymbolShape::Square
                | SymbolShape::Triangle
                | SymbolShape::Diamond
                | SymbolShape::Plus
                | SymbolShape::Cross => {}
            }
        }
        toutes
    }

    fn positions() -> [LegendPosition; 4] {
        let toutes = [
            LegendPosition::TopLeft,
            LegendPosition::TopRight,
            LegendPosition::BottomLeft,
            LegendPosition::BottomRight,
        ];
        for position in toutes {
            match position {
                LegendPosition::TopLeft
                | LegendPosition::TopRight
                | LegendPosition::BottomLeft
                | LegendPosition::BottomRight => {}
            }
        }
        toutes
    }

    /// Chaque liste déroulante des propriétés montre des choix qu'on
    /// distingue, sur toutes les variantes ; le type inconnu a son propre
    /// libellé, distinct des types connus.
    #[test]
    fn les_choix_d_une_meme_liste_ont_des_libelles_distincts() {
        let mut types = types_connus();
        types.push(inconnu());
        distincts(&types, plot_type_label);
        distincts(&styles(), |style| line_style_label(*style));
        distincts(&formes(), |forme| symbol_shape_label(*forme));
        distincts(&positions(), |position| legend_position_label(*position));
        // Le libellé suit la variante, pas son contenu.
        let autre = PlotType::Unknown(RawVariant {
            name: "Polar".into(),
            payload: vec![1],
        });
        assert_eq!(plot_type_label(&autre), plot_type_label(&inconnu()));
    }

    // -- Les métadonnées d'une colonne --------------------------------------

    /// Une colonne aux métadonnées de fichier : une section, deux lignes
    /// clé / valeur, une ligne sans clé.
    fn banc_metadonnees(long_name: &str) -> (Session, AppState, NodeId) {
        let mut project = Project::new("Métadonnées").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let column = project
            .add_column(sheet, None, None, Column::from_f64(&[1.0]))
            .unwrap();
        let ligne = |key: &str, value: &str| MetaLine {
            key: key.into(),
            value: value.into(),
        };
        project
            .set_column_meta(
                column,
                ColumnMeta {
                    long_name: long_name.into(),
                    file_metadata: vec![
                        ligne("[Measurement]", ""),
                        ligne("XUNITS", "NANOMETERS"),
                        ligne("", "free text line"),
                        ligne("NPOINTS", "3"),
                    ],
                    ..Default::default()
                },
            )
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        (session, state, column)
    }

    fn peindre_metadonnees(
        context: &egui::Context,
        state: &AppState,
        actions: &mut Vec<Action>,
        events: Vec<egui::Event>,
    ) -> Vec<Texte> {
        for _ in 0..3 {
            image(context, Vec::new(), |ui| {
                column_metadata_window(ui.ctx(), state, actions)
            });
        }
        textes(&image(context, events, |ui| {
            column_metadata_window(ui.ctx(), state, actions)
        }))
    }

    /// Issue #5 : la fenêtre des métadonnées montre, en lecture seule, les
    /// lignes clé / valeur, la section en gras et la ligne sans clé, sous un
    /// titre qui nomme la colonne. Ouverte, elle ne demande rien ; Entrée
    /// (« OK ») comme Échap la ferment, sans rien d'autre. Fermée, elle ne
    /// peint rien.
    #[test]
    fn la_fenetre_des_metadonnees_montre_les_lignes_et_se_ferme() {
        let context = contexte();
        for touche_de_fin in [egui::Key::Enter, egui::Key::Escape] {
            let (_session, mut state, column) = banc_metadonnees("Absorbance");
            let mut actions = Vec::new();
            let peints = peindre_metadonnees(&context, &state, &mut actions, Vec::new());
            assert!(!est_peint(&peints, "XUNITS"), "fermée, rien n'est peint");

            state.dispatch(Action::ViewColumnMetadata { column });
            let peints = peindre_metadonnees(&context, &state, &mut actions, Vec::new());
            let titre = tr!("workbook-metadata-title-named", name = "Absorbance");
            assert!(est_peint(&peints, &titre), "titre absent");
            for texte in ["XUNITS", "NANOMETERS", "free text line", "NPOINTS", "3"] {
                texte_exact(&peints, texte);
            }
            let visuals = context.global_style().visuals.clone();
            let section = texte_exact(&peints, "[Measurement]");
            assert!(section.gras, "section en gras");
            assert_eq!(
                section.couleur,
                visuals.text_color(),
                "le gras ne change pas la couleur"
            );
            assert!(!texte_exact(&peints, "XUNITS").gras, "témoin : ligne à clé");
            assert_eq!(texte_exact(&peints, "XUNITS").couleur, visuals.text_color());
            assert!(actions.is_empty(), "ouverte, elle ne demande rien");
            // #164 : seule une valeur numérique à clé (NPOINTS) offre le
            // bouton « Utiliser comme x' » ; le texte NANOMETERS non.
            assert_eq!(
                peints
                    .iter()
                    .filter(|t| t.texte == tr!("workbook-metadata-use-as-x-prime"))
                    .count(),
                1,
                "un seul bouton : la valeur numérique"
            );

            peindre_metadonnees(&context, &state, &mut actions, vec![touche(touche_de_fin)]);
            assert_eq!(actions, [Action::CloseColumnMetadata], "{touche_de_fin:?}");
        }

        // Sans nom long, le titre générique.
        let (_session, mut state, column) = banc_metadonnees("");
        state.dispatch(Action::ViewColumnMetadata { column });
        let peints = peindre_metadonnees(&contexte(), &state, &mut Vec::new(), Vec::new());
        assert!(est_peint(&peints, &tr!("workbook-metadata-title")));
    }
}
