//! Fenêtre des propriétés du graphe : onglets (thème, axes, séries, légende),
//! éditeurs par famille, mémo des formules Typst, libellés des choix, et la
//! fenêtre des métadonnées d'une colonne (même famille de dialogue).
//! Points d'entrée pour `app.rs` : [`plot_settings_window`] et
//! [`column_metadata_window`].
//!
//! Sorti d'`app.rs` tel quel (#119).

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{AxisSpec, LegendPosition, LineStyle, PlotType, SymbolShape};

use crate::color_picker::{color_field, ColorUsage, PickerEnv, SavedColorEdit};
use crate::state::{
    series_list_label, Action, AppState, PlotAxis, PlotSettings, PlotSettingsTab, SaveTarget,
};
use crate::tr;

use super::dialogs::{modal_dialog, DialogOutcome};
use super::tab_strip::tab_strip;

/// Largeur par défaut de la fenêtre des propriétés d'un graphe
/// ([`plot_settings_window`]).
const PLOT_SETTINGS_WIDTH: f32 = 380.0;
/// Hauteur par défaut de la fenêtre des propriétés d'un graphe. La fenêtre
/// se redimensionne ensuite librement, y compris plus haut que son contenu
/// (voir [`plot_settings_contents`]).
const PLOT_SETTINGS_HEIGHT: f32 = 460.0;

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
/// - **glissé numérique** (épaisseur de trait, taille de marqueur, bornes
///   d'axe) : `drag_stopped() || lost_focus()` ;
/// - **case à cocher / menu déroulant** : un seul événement discret par
///   clic, validé tout de suite.
///
/// Fermer la fenêtre valide ce qui diverge encore du modèle
/// ([`AppState::close_plot_settings`]).
///
/// **Un seul panneau, pas de multi-panneaux** : seul `PlotSpec::panels`[0]
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
    let title = tr!("plot-window-title", name = node.name());
    state.begin_plot_edit(settings.graph);
    let outcome = modal_dialog(
        ctx,
        egui::Id::new("stokesforge_plot_settings"),
        &title,
        egui::vec2(PLOT_SETTINGS_WIDTH, PLOT_SETTINGS_HEIGHT),
        |ui| plot_settings_contents(ui, state, &tree, settings),
    );
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => state.close_plot_settings(),
        DialogOutcome::Cancel => state.cancel_plot_settings(),
    }
}

/// Fenêtre de consultation des métadonnées de fichier d'une colonne (issue
/// #5) : les lignes clé / valeur, sections comprises, en lecture seule. Même
/// brique modale que les propriétés du graphe ; « Valider » et « Annuler »
/// la ferment sans rien changer.
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
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for line in &lines {
                            if line.is_section() {
                                ui.label(egui::RichText::new(&line.key).strong());
                                ui.label("");
                            } else if line.key.is_empty() {
                                ui.label("");
                                ui.label(&line.value);
                            } else {
                                ui.label(&line.key);
                                ui.label(&line.value);
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
    let tabs: Vec<(PlotSettingsTab, String)> = PlotSettingsTab::ALL
        .iter()
        .map(|tab| (*tab, tab.label()))
        .collect();
    tab_strip(ui, &tabs, settings.tab, |tab| chosen_tab = tab, None, || {});
    ui.separator();

    let mut commit = false;
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match chosen_tab {
                PlotSettingsTab::Graph => {
                    commit |= text_field(
                        ui,
                        &tr!("plot-general-title"),
                        &tr!("plot-general-title", "hint"),
                        &mut spec.title,
                    );
                    formula_memo(ui, "plot_settings_memo_graph");
                    commit |= color_field(
                        ui,
                        &tr!("plot-title-color"),
                        &mut spec.title_color,
                        ColorUsage::Text,
                        Some(&tr!("plot-color-auto-contrast")),
                        &mut env,
                    );
                    ui.add_space(6.0);
                    commit |= color_field(
                        ui,
                        &tr!("plot-general-background"),
                        &mut spec.background,
                        ColorUsage::Background,
                        Some(&tr!("plot-color-auto-white")),
                        &mut env,
                    );
                    ui.weak(tr!("plot-general-dark-note"));
                }
                PlotSettingsTab::PlotArea => {
                    let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                        return;
                    };
                    commit |= text_field(
                        ui,
                        &tr!("plot-area-layer-title"),
                        &tr!("plot-area-layer-title", "hint"),
                        &mut panel.title,
                    );
                    formula_memo(ui, "plot_settings_memo_panel");
                    ui.add_space(4.0);
                    ui.strong(tr!("plot-area-gridlines"));
                    commit |= ui
                        .checkbox(&mut panel.bottom.grid, tr!("plot-area-gridlines-vertical"))
                        .changed();
                    commit |= ui
                        .checkbox(&mut panel.left.grid, tr!("plot-area-gridlines-horizontal"))
                        .changed();
                }
                PlotSettingsTab::Axes => {
                    let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                        return;
                    };
                    let mut axes = vec![PlotAxis::Bottom, PlotAxis::Left];
                    if panel.uses_right_axis() || chosen_axis == PlotAxis::Right {
                        axes.push(PlotAxis::Right);
                    }
                    ui.horizontal(|ui| {
                        for axis in axes {
                            if ui
                                .selectable_label(chosen_axis == axis, axis.label())
                                .clicked()
                            {
                                chosen_axis = axis;
                            }
                        }
                    });
                    ui.separator();
                    let axis = match chosen_axis {
                        PlotAxis::Bottom => &mut panel.bottom,
                        PlotAxis::Left => &mut panel.left,
                        PlotAxis::Right => &mut panel.right,
                    };
                    commit |= axis_editor(ui, axis, &mut env);
                }
                PlotSettingsTab::Series => {
                    let Some(panel) = spec.panels.get_mut(edit.selected_panel) else {
                        return;
                    };
                    if panel.series.is_empty() {
                        ui.weak(tr!("plot-series-empty"));
                        return;
                    }
                    if edit.selected_series >= panel.series.len() {
                        edit.selected_series = panel.series.len() - 1;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("plot_settings_series_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (index, series) in panel.series.iter().enumerate() {
                                let label = series_list_label(tree, series);
                                if ui
                                    .selectable_label(edit.selected_series == index, label)
                                    .clicked()
                                {
                                    edit.selected_series = index;
                                }
                            }
                        });
                    ui.separator();
                    if let Some(series) = panel.series.get_mut(edit.selected_series) {
                        commit |= series_editor(ui, series, &mut env);
                    }
                }
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
    if commit {
        state.commit_plot_edit();
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

/// Onglet « Thème » : fond, couleur du texte et couleur des axes, réglés
/// globalement (issue #11), plus des thèmes prédéfinis. Le dernier réglage
/// écrase ce qui avait été réglé une à une : voir [`stokesforge_plot::theme`].
/// Rend `true` si le brouillon doit être validé.
pub(super) fn theme_editor(
    ui: &mut egui::Ui,
    spec: &mut stokesforge_plot::PlotSpec,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    ui.strong(tr!("plot-theme-themes"));
    ui.horizontal(|ui| {
        for theme in &stokesforge_plot::THEMES {
            if ui.button(theme.name).clicked() {
                spec.apply_theme(theme);
                commit = true;
            }
        }
    });
    ui.add_space(6.0);
    ui.strong(tr!("plot-theme-global-colors"));
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

/// Le mémo de syntaxe des textes du graphe (issue #33), replié par défaut,
/// sous les champs de titre, de libellé et de nom de série.
///
/// Ces textes sont composés par Typst et lus comme du **markup** (issue
/// #32) : un chimiste venu d'Origin n'en connaît pas la syntaxe, et un `_`
/// ou un `$` isolé change le rendu sans prévenir. Une section repliable
/// plutôt qu'une bulle d'aide : la fenêtre des propriétés est un dialogue
/// modal, où une fenêtre surgissante de plus se disputerait l'unique place
/// qu'egui leur réserve (issue #9).
fn formula_memo(ui: &mut egui::Ui, id_salt: &str) {
    egui::CollapsingHeader::new(tr!("plot-formula-memo-header"))
        .id_salt(id_salt)
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new(id_salt)
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

/// Les lignes du mémo de [`formula_memo`] : ce qu'on tape (de la syntaxe
/// Typst, jamais traduite), ce qu'on obtient (traduit).
pub(super) fn formula_memo_rows() -> [(&'static str, String); 7] {
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

/// Un champ texte libellé, validé à la perte de focus ; `hint` est son
/// infobulle. Rend `true` si le brouillon doit être validé — voir la doc de
/// tête de [`plot_settings_window`].
fn text_field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        ui.text_edit_singleline(value).on_hover_text(hint)
    })
    .inner
    .lost_focus()
}

/// Édite un [`AxisSpec`], en trois groupes : le titre de l'axe (libellé,
/// unité), son échelle (logarithmique, inversée, bornes) et son affichage
/// (axe, grille). Rend `true` si le brouillon doit être validé — voir la doc
/// de tête de [`plot_settings_window`].
fn axis_editor(ui: &mut egui::Ui, axis: &mut AxisSpec, env: &mut PickerEnv) -> bool {
    let mut commit = false;
    ui.strong(tr!("plot-axes-title-section"));
    commit |= text_field(
        ui,
        &tr!("plot-axes-quantity"),
        &tr!("plot-axes-quantity", "hint"),
        &mut axis.label,
    );
    commit |= text_field(
        ui,
        &tr!("plot-axes-units"),
        &tr!("plot-axes-units", "hint"),
        &mut axis.unit,
    );
    formula_memo(ui, "plot_settings_memo_axis");
    commit |= color_field(
        ui,
        &tr!("plot-title-color"),
        &mut axis.title_color,
        ColorUsage::Text,
        Some(&tr!("plot-color-auto-contrast")),
        env,
    );

    ui.add_space(6.0);
    ui.strong(tr!("plot-axes-scale-section"));
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
    commit |= ui
        .checkbox(&mut axis.auto_range, tr!("plot-axes-auto-range"))
        .on_hover_text(tr!("plot-axes-auto-range", "hint"))
        .changed();
    if !axis.auto_range {
        let mut min = axis.range_min.unwrap_or(0.0);
        let mut max = axis.range_max.unwrap_or(0.0);
        let min_response = ui.horizontal(|ui| {
            ui.label(tr!("plot-axes-min"));
            ui.add(egui::DragValue::new(&mut min).speed(0.1))
        });
        let max_response = ui.horizontal(|ui| {
            ui.label(tr!("plot-axes-max"));
            ui.add(egui::DragValue::new(&mut max).speed(0.1))
        });
        axis.range_min = Some(min);
        axis.range_max = Some(max);
        let min_inner = min_response.inner;
        let max_inner = max_response.inner;
        commit |= min_inner.drag_stopped() || min_inner.lost_focus();
        commit |= max_inner.drag_stopped() || max_inner.lost_focus();
    }

    ui.add_space(6.0);
    ui.strong(tr!("plot-axes-display-section"));
    commit |= ui
        .checkbox(&mut axis.visible, tr!("plot-axes-show-axis"))
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
fn series_editor(
    ui: &mut egui::Ui,
    series: &mut stokesforge_plot::SeriesSpec,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;

    commit |= text_field(
        ui,
        &tr!("plot-series-name"),
        &tr!("plot-series-name", "hint"),
        &mut series.name,
    );
    formula_memo(ui, "plot_settings_memo_series");

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

    ui.separator();
    ui.label(tr!("plot-series-line-section"));
    commit |= color_field(
        ui,
        &tr!("plot-series-line-color"),
        &mut series.line.color,
        ColorUsage::Curves,
        None,
        env,
    );

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

    let width_response = ui.horizontal(|ui| {
        ui.label(tr!("plot-series-line-width"));
        ui.add(
            egui::DragValue::new(&mut series.line.width)
                .speed(0.1)
                .range(0.0..=20.0),
        )
    });
    let width_inner = width_response.inner;
    commit |= width_inner.drag_stopped() || width_inner.lost_focus();

    ui.separator();
    ui.label(tr!("plot-series-symbol-section"));
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

    let size_response = ui.horizontal(|ui| {
        ui.label(tr!("plot-series-symbol-size"));
        ui.add(
            egui::DragValue::new(&mut series.symbol.size)
                .speed(0.1)
                .range(0.0..=40.0),
        )
    });
    let size_inner = size_response.inner;
    commit |= size_inner.drag_stopped() || size_inner.lost_focus();

    commit |= color_field(
        ui,
        &tr!("plot-series-symbol-color"),
        &mut series.symbol.color,
        ColorUsage::Curves,
        Some(&tr!("plot-color-same-as-line")),
        env,
    );

    commit
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
fn line_style_label(style: LineStyle) -> String {
    match style {
        LineStyle::None => tr!("plot-line-style-none"),
        LineStyle::Solid => tr!("plot-line-style-solid"),
        LineStyle::Dash => tr!("plot-line-style-dashed"),
        LineStyle::Dot => tr!("plot-line-style-dotted"),
        LineStyle::DashDot => tr!("plot-line-style-dash-dot"),
    }
}

/// Libellé d'un [`SymbolShape`].
fn symbol_shape_label(shape: SymbolShape) -> String {
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
