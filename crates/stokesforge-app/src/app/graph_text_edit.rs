//! Éditeurs sur place d'un graphe : le texte libre (issue #35, avec sa barre
//! couleur / taille / rotation), le titre et le nom de légende d'une série
//! (issue #53), et la mise en forme demandée par la barre de format pour le
//! champ en édition (`pending_format_*`, `apply_pending_*`, #77/#78).
//!
//! Sorti d'`app.rs` tel quel (#121).
//!
//! L'édition sur place et le glisser ne touchent pas le modèle tant qu'ils
//! durent : l'aperçu vit dans la vue (`GraphView::set_annotation_preview`),
//! comme le zoom, et la validation envoie **une** `Action::SetAnnotation` —
//! une entrée d'historique, Échap jette l'aperçu. L'état de l'éditeur est
//! rangé dans la mémoire d'egui sous l'identifiant de la vue : la même
//! écriture sert la fenêtre flottante et la fenêtre détachée, sans rien
//! faire transiter par `AppState`.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::{GraphView, TextTarget};

use crate::color_picker::{ColorUsage, PickerEnv};
use crate::state::{series_list_label, Action};
use crate::tr;

use crate::color_picker::{saved_colors_id, SavedColorRequest};

/// L'éditeur sur place d'un texte libre, ouvert sur une vue.
#[derive(Debug, Clone)]
pub(super) struct AnnotationEditor {
    pub(super) target: stokesforge_render_egui::AnnotationRef,
    pub(super) draft: stokesforge_render_egui::AnnotationSpec,
    /// Le texte tel qu'il est dans le modèle ; `None` pour un texte nouveau.
    pub(super) original: Option<stokesforge_render_egui::AnnotationSpec>,
    /// Donner le focus au champ à la prochaine image (ouverture).
    pub(super) focus: bool,
    /// Le dernier centre et la dernière taille (écran) du texte droit rendu :
    /// un texte vidé n'est plus tracé, mais le champ doit rester où il est.
    pub(super) anchor: Option<(egui::Pos2, egui::Vec2)>,
}

/// Un titre ou une entrée de légende visé par le clic droit (issue #53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextText {
    /// Un titre, un titre d'axe ou la légende entière.
    Title(TextTarget),
    /// La ligne de légende de la série `series` du calque `panel`.
    LegendEntry { panel: usize, series: usize },
}

/// Ce que l'éditeur sur place d'un titre modifie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TextEditTarget {
    /// Le texte saisi d'un titre.
    Title(TextTarget),
    /// Le nom, en légende, d'une série.
    LegendEntry(stokesforge_plot::SeriesRef),
}

/// L'éditeur sur place d'un titre ou d'un nom de légende (issue #53) : un
/// champ posé sur le texte. Entrée ou clic ailleurs valide, Échap annule —
/// comme l'éditeur d'un texte libre, sans barre de réglages (couleur et
/// taille d'un titre se règlent dans les propriétés du graphe).
#[derive(Debug, Clone)]
pub(super) struct TextEditor {
    pub(super) target: TextEditTarget,
    pub(super) buffer: String,
    /// Le texte tel qu'il était à l'ouverture : valider sans l'avoir changé
    /// n'envoie rien (et ne fige pas un nom dérivé de la colonne).
    original: String,
    /// Donner le focus au champ à la prochaine image (ouverture).
    focus: bool,
    /// Centre et taille (écran) du texte tracé à l'ouverture.
    anchor: (egui::Pos2, egui::Vec2),
}

/// La mise en forme demandée par la barre, en attente du champ en édition.
pub(super) fn pending_format_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_text_format")
}

/// La police demandée par la barre pour le texte libre en édition : elle
/// s'applique au brouillon de l'éditeur, pas au contenu (issue #78).
pub(super) fn pending_zone_font_id() -> egui::Id {
    egui::Id::new("stokesforge_pending_zone_font")
}

/// Pose sur `font` la police demandée par la barre de format, s'il y en a une.
fn apply_pending_zone_font(ctx: &egui::Context, font: &mut String) {
    if let Some(Some(name)) =
        ctx.data_mut(|data| data.remove_temp::<Option<String>>(pending_zone_font_id()))
    {
        *font = name;
    }
}

/// Pose la mise en forme demandée par la barre de format sur `buffer`, selon
/// la sélection que le champ `field` a gardée (egui la conserve quand le
/// champ perd le focus au clic sur la barre), puis rend le focus au champ avec
/// la sélection à l'intérieur des balises.
fn apply_pending_format(ctx: &egui::Context, field: egui::Id, buffer: &mut String) {
    use egui::text::{CCursor, CCursorRange};
    let Some(format) = ctx
        .data_mut(|data| {
            data.remove_temp::<Option<crate::text_format::TextFormat>>(pending_format_id())
        })
        .flatten()
    else {
        return;
    };
    let mut state = egui::TextEdit::load_state(ctx, field).unwrap_or_default();
    let length = buffer.chars().count();
    let selection = state.cursor.char_range().map_or(length..length, |range| {
        let [start, end] = range.sorted_cursors();
        start.index.0..end.index.0
    });
    let done = crate::text_format::apply(buffer, selection, &format);
    *buffer = done.text;
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(done.selection.start),
        CCursor::new(done.selection.end),
    )));
    state.store(ctx, field);
    ctx.memory_mut(|memory| memory.request_focus(field));
}

pub(super) fn text_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("text_editor")
}

/// Ouvre l'éditeur sur le texte d'un titre ; sans effet si le titre n'est
/// pas tracé ou n'existe plus (la légende n'a pas de texte à elle).
pub(super) fn open_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    target: TextTarget,
) {
    let Some(original) =
        crate::state::current_plot_spec(tree, graph).and_then(|spec| target.text(&spec))
    else {
        return;
    };
    let Some((center, size, _)) = view.text_screen_box(target) else {
        return;
    };
    start_text_editor(
        ctx,
        view,
        TextEditTarget::Title(target),
        original,
        (center, size),
    );
}

/// Ouvre l'éditeur sur le nom, en légende, d'une série.
pub(super) fn open_legend_entry_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    panel: usize,
    series: usize,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let address = stokesforge_plot::SeriesRef::new(panel, series);
    let Some(spec_series) = spec.series_at(address) else {
        return;
    };
    let Some(anchor) = view.legend_entry_screen_box(panel, series) else {
        return;
    };
    let original = crate::state::series_list_label(tree, spec_series);
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(address),
        original,
        anchor,
    );
}

pub(super) fn open_series_name_editor(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let Some(value) = spec.series_at(series) else {
        return;
    };
    let anchor = view
        .legend_entry_screen_box(series.panel, series.index)
        .unwrap_or((
            view.display_rect()
                .map_or(ctx.content_rect().center(), |r| r.center()),
            egui::vec2(180.0, 24.0),
        ));
    start_text_editor(
        ctx,
        view,
        TextEditTarget::LegendEntry(series),
        series_list_label(tree, value),
        anchor,
    );
}

fn start_text_editor(
    ctx: &egui::Context,
    view: &GraphView,
    target: TextEditTarget,
    original: String,
    anchor: (egui::Pos2, egui::Vec2),
) {
    let editor = TextEditor {
        target,
        buffer: original.clone(),
        original,
        focus: true,
        anchor,
    };
    ctx.data_mut(|data| data.insert_temp(text_editor_id(view), editor));
}

/// Peint l'éditeur d'un titre ou d'un nom de légende, et traduit sa
/// validation en **une** action.
pub(super) fn text_editor_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    mut editor: TextEditor,
    actions: &mut Vec<Action>,
) {
    let id = text_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.buffer);
    let (center, size) = editor.anchor;
    let field = egui::Rect::from_center_size(
        center,
        egui::vec2((size.x + 32.0).max(180.0), (size.y + 8.0).max(24.0)),
    );
    let mut accept = false;
    let field_id = id.with("field");
    let area = egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.min)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut editor.buffer)
                        .id(field_id)
                        .desired_width(field.width())
                        .hint_text(tr!("plot-text-placeholder")),
                );
                if editor.focus {
                    response.request_focus();
                    editor.focus = false;
                }
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    accept = true;
                }
            });
        });
    let cancel = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let outside = ctx
        .input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        })
        .is_some_and(|pos| {
            !area.response.rect.contains(pos)
                && !crate::toolbar::over_format_bar(ctx, pos)
                && ctx
                    .layer_id_at(pos)
                    .is_none_or(|layer| layer.order != egui::Order::Foreground)
        });
    if cancel {
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    if accept || outside {
        if editor.buffer != editor.original {
            actions.push(match editor.target {
                TextEditTarget::Title(target) => Action::SetTextContent {
                    graph,
                    target,
                    text: editor.buffer.clone(),
                },
                TextEditTarget::LegendEntry(series) => Action::SetLegendEntryName {
                    graph,
                    series,
                    name: editor.buffer.clone(),
                },
            });
        }
        ctx.data_mut(|data| data.remove::<TextEditor>(id));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

pub(super) fn annotation_editor_id(view: &GraphView) -> egui::Id {
    view.id().with("annotation_editor")
}

/// Ouvre l'éditeur sur un texte existant ; sans effet si le texte n'existe
/// plus dans le modèle.
pub(super) fn open_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    target: stokesforge_render_egui::AnnotationRef,
) {
    let Some(original) =
        crate::state::current_plot_spec(tree, graph).and_then(|spec| target.get(&spec).cloned())
    else {
        return;
    };
    start_annotation_editor(ctx, view, target, original.clone(), Some(original));
}

/// Ajoute un texte au point de données `(panel, x, y)` et ouvre
/// l'éditeur dessus : rien n'entre dans le modèle avant validation.
pub(super) fn add_annotation_at(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    (panel, x, y): (usize, f64, f64),
) {
    let Some(target) = crate::state::current_plot_spec(tree, graph)
        .and_then(|spec| stokesforge_render_egui::AnnotationRef::next_in(&spec, panel))
    else {
        return;
    };
    let draft = stokesforge_render_egui::AnnotationSpec::new(String::new(), x, y);
    start_annotation_editor(ctx, view, target, draft, None);
}

fn start_annotation_editor(
    ctx: &egui::Context,
    view: &mut GraphView,
    target: stokesforge_render_egui::AnnotationRef,
    draft: stokesforge_render_egui::AnnotationSpec,
    original: Option<stokesforge_render_egui::AnnotationSpec>,
) {
    view.set_annotation_preview(Some((target, draft.clone())));
    let editor = AnnotationEditor {
        target,
        draft,
        original,
        focus: true,
        anchor: None,
    };
    ctx.data_mut(|data| data.insert_temp(annotation_editor_id(view), editor));
}

/// Peint l'éditeur sur place : le champ de texte posé sur le texte, et
/// au-dessus une barre couleur / taille / rotation. Entrée ou clic ailleurs
/// valide, Échap annule.
pub(super) fn annotation_editor_ui(
    ctx: &egui::Context,
    view: &mut GraphView,
    graph: NodeId,
    mut editor: AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let id = annotation_editor_id(view);
    apply_pending_format(ctx, id.with("field"), &mut editor.draft.text);
    apply_pending_zone_font(ctx, &mut editor.draft.font);
    // Le cadre du texte rendu ; à défaut (texte neuf, ou vidé donc plus
    // tracé), le dernier connu, puis le centre de la figure.
    if let Some((center, size, _)) = view.annotation_screen_box(editor.target) {
        editor.anchor = Some((center, size));
    }
    let (center, size) = editor.anchor.unwrap_or_else(|| {
        let center = view
            .display_rect()
            .map_or(egui::Pos2::ZERO, |rect| rect.center());
        (center, egui::Vec2::ZERO)
    });
    // Le champ s'écrit dans le cadre du rendu (#37) : centré, au corps du
    // texte, droit même si le texte est tourné.
    let field = stokesforge_render_egui::annotation_field(
        center,
        size,
        editor.draft.effective_size(),
        view.display_scale().unwrap_or(1.0),
    );

    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };

    let mut accept = false;
    let toolbar = egui::Area::new(id.with("toolbar"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min - egui::vec2(0.0, 36.0))
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::color_picker::color_icon_button(
                        ui,
                        (crate::icons::Icon::TextColor, &tr!("format-color")),
                        ("annotation_color", id),
                        &mut editor.draft.color,
                        ColorUsage::Text,
                        Some(&tr!("plot-color-auto-contrast")),
                        &mut env,
                    );
                    ui.label(tr!("plot-text-size"));
                    let (low, high) = stokesforge_render_egui::AnnotationSpec::SIZE_RANGE;
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.size)
                            .range(low..=high)
                            .speed(0.2),
                    );
                    ui.label(tr!("plot-text-rotation"));
                    ui.add(
                        egui::DragValue::new(&mut editor.draft.rotation)
                            .range(-180.0..=180.0)
                            .speed(1.0),
                    );
                    if ui
                        .button(tr!("plot-text-ok"))
                        .on_hover_text(tr!("plot-text-ok", "hint"))
                        .clicked()
                    {
                        accept = true;
                    }
                });
            });
        });
    // Tant qu'on tape, le champ droit couvre le texte ; dès qu'il perd le
    // focus (on règle la rotation ou la taille), il devient transparent et
    // l'aperçu — tourné — se voit (#36).
    let field_id = id.with("field");
    let typing = editor.focus || ctx.memory(|memory| memory.has_focus(field_id));
    let text = egui::Area::new(id.with("text"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.min)
        .constrain(true)
        .show(ctx, |ui| {
            let visuals = ui.visuals().clone();
            let outline = if typing {
                visuals.selection.stroke
            } else {
                visuals.widgets.inactive.bg_stroke
            };
            if typing {
                ui.painter()
                    .rect_filled(field.rect, 2.0, visuals.extreme_bg_color);
            }
            ui.painter()
                .rect_stroke(field.rect, 2.0, outline, egui::StrokeKind::Inside);
            let padding = (field.font_size * 0.3).round() as i8;
            let response = ui.put(
                field.rect,
                egui::TextEdit::singleline(&mut editor.draft.text)
                    .id(field_id)
                    .font(egui::FontId::proportional(field.font_size))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::same(padding))
                    .desired_width(field.rect.width())
                    .horizontal_align(egui::Align::Center)
                    .vertical_align(egui::Align::Center)
                    .text_color(if typing {
                        visuals.text_color()
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .hint_text(tr!("plot-text-placeholder")),
            );
            if editor.focus {
                response.request_focus();
                editor.focus = false;
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                accept = true;
            }
        });

    actions.extend(env.requests.into_iter().map(SavedColorRequest::into_action));

    // Échap : le sélecteur de couleur ouvert le consomme d'abord pour lui.
    let cancel = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    // Un clic hors de l'éditeur (et hors de toute couche surgissante, comme
    // le sélecteur de couleur) valide.
    let pressed_at = ctx.input(|input| {
        input
            .pointer
            .any_pressed()
            .then(|| input.pointer.interact_pos())
            .flatten()
    });
    let outside = pressed_at.is_some_and(|pos| {
        !toolbar.response.rect.contains(pos)
            && !text.response.rect.contains(pos)
            && !crate::toolbar::over_format_bar(ctx, pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order != egui::Order::Foreground)
    });

    if cancel {
        view.set_annotation_preview(None);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    if accept || outside {
        commit_annotation_editor(view, graph, &editor, actions);
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        return;
    }
    view.set_annotation_preview(Some((editor.target, editor.draft.clone())));
    ctx.data_mut(|data| data.insert_temp(id, editor));
}

/// Valide l'éditeur : une seule action, et seulement si quelque chose a
/// changé. Un texte vidé est retiré (un texte vide n'est pas tracé, il ne
/// pourrait plus être désigné).
fn commit_annotation_editor(
    view: &mut GraphView,
    graph: NodeId,
    editor: &AnnotationEditor,
    actions: &mut Vec<Action>,
) {
    let empty = editor.draft.text.trim().is_empty();
    match (&editor.original, empty) {
        (None, true) => view.set_annotation_preview(None),
        (Some(_), true) => {
            actions.push(Action::RemoveAnnotation {
                graph,
                target: editor.target,
            });
            view.hold_preview();
        }
        (Some(original), false) if *original == editor.draft => {
            view.set_annotation_preview(None);
        }
        _ => {
            actions.push(Action::SetAnnotation {
                graph,
                target: editor.target,
                annotation: Box::new(editor.draft.clone()),
            });
            view.hold_preview();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_format::TextFormat;
    use egui::text::{CCursor, CCursorRange};
    use stokesforge_plot::SeriesRef;
    use stokesforge_render_egui::{AnnotationRef, AnnotationSpec};

    fn context_with_selection(field: egui::Id, range: std::ops::Range<usize>) -> egui::Context {
        let ctx = egui::Context::default();
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(range.start),
            CCursor::new(range.end),
        )));
        state.store(&ctx, field);
        ctx
    }

    /// #63 : la mise en forme demandée par la barre s'applique au surlignage
    /// que le champ a gardé, puis la sélection se place dans les balises.
    #[test]
    fn la_mise_en_forme_en_attente_s_applique_au_surlignage_du_champ() {
        let field = egui::Id::new("champ");
        let ctx = context_with_selection(field, 4..7);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Bold)));
        let mut buffer = "Abs 280 nm".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
        let state = egui::text_edit::TextEditState::load(&ctx, field).unwrap();
        let [start, end] = state.cursor.char_range().unwrap().sorted_cursors();
        assert_eq!((start.index.0, end.index.0), (12, 15));
        assert!(
            ctx.memory(|memory| memory.has_focus(field)),
            "le champ retrouve le focus après le clic sur la barre"
        );
        // La demande est consommée : une seconde image ne la rejoue pas.
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "Abs #strong[280] nm");
    }

    #[test]
    fn sans_surlignage_le_curseur_recoit_les_balises() {
        let field = egui::Id::new("champ");
        let ctx = context_with_selection(field, 3..3);
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(TextFormat::Subscript)));
        let mut buffer = "H2O".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O#sub[]");
    }

    /// Sans demande en attente, le texte du champ reste tel quel (le témoin des
    /// deux tests précédents).
    #[test]
    fn sans_demande_en_attente_le_texte_reste_tel_quel() {
        let field = egui::Id::new("champ");
        let ctx = context_with_selection(field, 0..3);
        let mut buffer = "H2O".to_owned();
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O");
        ctx.data_mut(|data| data.insert_temp(pending_format_id(), None::<TextFormat>));
        apply_pending_format(&ctx, field, &mut buffer);
        assert_eq!(buffer, "H2O");
    }

    /// #78 : la police demandée par la barre se pose une fois sur le brouillon
    /// du texte libre, puis la demande est consommée.
    #[test]
    fn la_police_en_attente_se_pose_une_fois_sur_le_brouillon() {
        let ctx = egui::Context::default();
        let mut font = "Fira Sans".to_owned();
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Fira Sans", "rien en attente, rien ne change");
        ctx.data_mut(|data| {
            data.insert_temp(pending_zone_font_id(), Some("Libertinus Serif".to_owned()))
        });
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Libertinus Serif");
        font = "Fira Sans".to_owned();
        apply_pending_zone_font(&ctx, &mut font);
        assert_eq!(font, "Fira Sans", "la demande est consommée");
    }

    fn editor(
        target: AnnotationRef,
        draft: AnnotationSpec,
        original: Option<AnnotationSpec>,
    ) -> AnnotationEditor {
        AnnotationEditor {
            target,
            draft,
            original,
            focus: false,
            anchor: None,
        }
    }

    /// Valider l'éditeur d'un texte libre n'envoie qu'une action, et
    /// seulement si quelque chose a changé ; un texte vidé est retiré.
    #[test]
    fn valider_un_texte_libre_n_envoie_qu_une_action_et_seulement_s_il_a_change() {
        let graph = NodeId::from_raw(5);
        let target = AnnotationRef { panel: 0, index: 0 };
        let pic = AnnotationSpec::new("Pic", 1.0, 2.0);
        let blank = AnnotationSpec::new("   ", 1.0, 2.0);
        let renamed = AnnotationSpec::new("Pic 2", 1.0, 2.0);
        for (case, draft, original, expected) in [
            ("neuf et vide", blank.clone(), None, vec![]),
            (
                "neuf",
                pic.clone(),
                None,
                vec![Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(pic.clone()),
                }],
            ),
            (
                "vidé",
                blank.clone(),
                Some(pic.clone()),
                vec![Action::RemoveAnnotation { graph, target }],
            ),
            ("inchangé", pic.clone(), Some(pic.clone()), vec![]),
            (
                "modifié",
                renamed.clone(),
                Some(pic.clone()),
                vec![Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(renamed.clone()),
                }],
            ),
        ] {
            let mut view = GraphView::new();
            view.set_annotation_preview(Some((target, draft.clone())));
            let mut actions = Vec::new();
            commit_annotation_editor(
                &mut view,
                graph,
                &editor(target, draft, original),
                &mut actions,
            );
            assert_eq!(actions, expected, "{case}");
            // Sans figure tracée (pas de révision à attendre), l'aperçu
            // s'efface dans tous les cas.
            assert_eq!(view.annotation_preview(), None, "{case}");
        }
    }

    /// Une image de l'éditeur de titre ouvert sur `view`, s'il y en a un.
    fn run_text_editor(
        ctx: &egui::Context,
        view: &GraphView,
        graph: NodeId,
        events: Vec<egui::Event>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let editor: Option<TextEditor> =
                ui.ctx().data(|data| data.get_temp(text_editor_id(view)));
            if let Some(editor) = editor {
                text_editor_ui(ui.ctx(), view, graph, editor, &mut actions);
            }
        });
        output.textures_delta.clear();
        actions
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_far() -> Vec<egui::Event> {
        let pos = egui::pos2(700.0, 500.0);
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    /// L'éditeur sur place d'un titre ou d'un nom de légende (#53) : il reste
    /// ouvert tant qu'on n'agit pas, Échap le ferme sans rien écrire, Entrée
    /// ou un clic ailleurs valide — par une action, et seulement si le texte
    /// a changé.
    #[test]
    fn l_editeur_d_un_titre_valide_par_entree_ou_un_clic_ailleurs_et_annule_par_echap() {
        let graph = NodeId::from_raw(5);
        let view = GraphView::new();
        let id = text_editor_id(&view);
        let open = |ctx: &egui::Context, target: TextEditTarget, original: &str| {
            start_text_editor(
                ctx,
                &view,
                target,
                original.to_owned(),
                (egui::pos2(200.0, 200.0), egui::vec2(80.0, 20.0)),
            );
        };
        let type_text = |ctx: &egui::Context, text: &str| {
            ctx.data_mut(|data| {
                let mut editor: TextEditor = data.get_temp(id).unwrap();
                editor.buffer = text.to_owned();
                data.insert_temp(id, editor);
            });
        };
        let open_now =
            |ctx: &egui::Context| ctx.data(|data| data.get_temp::<TextEditor>(id)).is_some();
        let title = TextEditTarget::Title(TextTarget::Title);

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectre");
        assert!(run_text_editor(&ctx, &view, graph, Vec::new()).is_empty());
        assert!(open_now(&ctx), "sans geste, l'éditeur reste ouvert");
        type_text(&ctx, "Autre");
        assert!(run_text_editor(&ctx, &view, graph, vec![key(egui::Key::Escape)]).is_empty());
        assert!(!open_now(&ctx), "Échap ferme l'éditeur sans rien écrire");

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectre");
        run_text_editor(&ctx, &view, graph, Vec::new());
        assert!(
            run_text_editor(&ctx, &view, graph, click_far()).is_empty(),
            "valider un texte inchangé n'envoie rien"
        );
        assert!(!open_now(&ctx));

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectre");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Absorbance");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, click_far()),
            vec![Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: "Absorbance".to_owned(),
            }]
        );
        assert!(!open_now(&ctx));

        let ctx = egui::Context::default();
        open(&ctx, title, "Spectre");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Absorbance");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, vec![key(egui::Key::Enter)]),
            vec![Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: "Absorbance".to_owned(),
            }],
            "Entrée valide aussi"
        );

        let ctx = egui::Context::default();
        let series = SeriesRef::new(0, 1);
        open(&ctx, TextEditTarget::LegendEntry(series), "B");
        run_text_editor(&ctx, &view, graph, Vec::new());
        type_text(&ctx, "Témoin");
        assert_eq!(
            run_text_editor(&ctx, &view, graph, click_far()),
            vec![Action::SetLegendEntryName {
                graph,
                series,
                name: "Témoin".to_owned(),
            }]
        );
    }

    /// L'éditeur d'un texte libre (#35) : l'aperçu montre le brouillon, Échap
    /// le jette sans rien écrire, un clic ailleurs valide par une action.
    #[test]
    fn l_editeur_d_un_texte_libre_valide_par_un_clic_ailleurs_et_jette_l_apercu_sur_echap() {
        let graph = NodeId::from_raw(5);
        let target = AnnotationRef { panel: 0, index: 0 };
        let draft = AnnotationSpec::new("Pic", 1.0, 2.0);
        let run = |ctx: &egui::Context, view: &mut GraphView, events: Vec<egui::Event>| {
            let mut actions = Vec::new();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let editor: Option<AnnotationEditor> = ui
                    .ctx()
                    .data(|data| data.get_temp(annotation_editor_id(view)));
                if let Some(editor) = editor {
                    annotation_editor_ui(ui.ctx(), view, graph, editor, &mut actions);
                }
            });
            output.textures_delta.clear();
            actions
        };
        let open_now = |ctx: &egui::Context, view: &GraphView| {
            ctx.data(|data| data.get_temp::<AnnotationEditor>(annotation_editor_id(view)))
                .is_some()
        };

        let ctx = egui::Context::default();
        let mut view = GraphView::new();
        start_annotation_editor(&ctx, &mut view, target, draft.clone(), None);
        assert_eq!(
            view.annotation_preview(),
            Some(&(target, draft.clone())),
            "l'aperçu montre le brouillon avant toute validation"
        );
        assert!(run(&ctx, &mut view, Vec::new()).is_empty());
        assert!(open_now(&ctx, &view));
        assert!(run(&ctx, &mut view, vec![key(egui::Key::Escape)]).is_empty());
        assert_eq!(view.annotation_preview(), None, "Échap jette l'aperçu");
        assert!(!open_now(&ctx, &view));

        let ctx = egui::Context::default();
        let mut view = GraphView::new();
        start_annotation_editor(&ctx, &mut view, target, draft.clone(), None);
        run(&ctx, &mut view, Vec::new());
        assert_eq!(
            run(&ctx, &mut view, click_far()),
            vec![Action::SetAnnotation {
                graph,
                target,
                annotation: Box::new(draft.clone()),
            }]
        );
        assert!(!open_now(&ctx, &view));
    }
}
