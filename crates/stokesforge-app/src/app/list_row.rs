//! **La** ligne de liste sélectionnable de l'application (#169, option A de
//! la maquette des onglets) : une entrée d'arbre ou de liste — fenêtre du
//! Project Manager, objet ou série de l'Object Manager, série de la page
//! « Séries » des propriétés du graphe — se peint comme une ligne pleine
//! largeur, jamais comme un bouton `selectable_label` / `Button::selectable`.
//!
//! Aspect : sélectionnée, fond pâle d'accent sur toute la largeur restante
//! depuis le retrait de la ligne, filet d'accent de [`LIST_ROW_MARK`] points à
//! gauche, libellé en gras (`theme::bold`) ; survolée, fond gris clair ;
//! focus clavier, contour de `selection.stroke`. Les couleurs viennent de
//! `crate::theme`, nulle part ailleurs.
//!
//! Deux usages :
//!
//! - [`list_row`] peint toute la ligne (icône facultative + libellé) et rend
//!   sa réponse ;
//! - une ligne composée de plusieurs widgets (case de visibilité, échantillon,
//!   libellé de l'Object Manager) réserve son fond **avant** ses widgets par
//!   [`ListRowBackground::reserve`], puis le pose une fois la ligne mesurée
//!   ([`ListRowBackground::paint`]) : le fond passe sous les widgets, sans
//!   ajouter de sous-`Ui` ni changer leurs identifiants egui.
//!
//! Un jeu d'**onglets** n'est pas une liste : il passe par `tab_strip`. Un
//! **choix exclusif** dans une liste déroulante ou un menu garde le
//! `selectable_label` d'egui (garde `aucun_bouton_selectionnable_hors_d_un_menu`).

use eframe::egui;

use crate::icons::ICON_SIZE;
use crate::theme;

/// Largeur du filet d'accent à gauche d'une ligne sélectionnée.
pub(super) const LIST_ROW_MARK: f32 = 3.0;

/// Marges intérieures d'une ligne peinte par [`list_row`].
const LIST_ROW_PADDING: egui::Vec2 = egui::vec2(6.0, 3.0);

/// Écart entre l'icône et le libellé.
const LIST_ROW_GAP: f32 = 5.0;

/// Le libellé d'une ligne : gras si elle est sélectionnée.
pub(super) fn list_row_text(text: &str, selected: bool) -> egui::RichText {
    if selected {
        theme::bold(text)
    } else {
        egui::RichText::new(text)
    }
}

/// Couleur du libellé : estompé pour une entrée masquée (`dimmed`), sinon
/// le texte ordinaire, un cran plus foncé quand la ligne est sélectionnée.
pub(super) fn list_row_text_color(ui: &egui::Ui, selected: bool, dimmed: bool) -> egui::Color32 {
    if dimmed {
        ui.visuals().weak_text_color()
    } else if selected {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().text_color()
    }
}

/// Le libellé cliquable d'une ligne composée ([`ListRowBackground`]) : un
/// bouton **sans cadre** — ni fond de sélection ni fond de survol, que porte
/// la ligne entière —, marqué sélectionné pour l'arbre d'accessibilité.
pub(super) fn list_row_label(text: egui::RichText, selected: bool) -> egui::Button<'static> {
    egui::Button::new(text).frame(false).selected(selected)
}

/// [`list_row_label`] suivi d'une zone qui prend tout le reste de la ligne,
/// l'une et l'autre cliquables et glissables : le clic, le double-clic et le
/// glisser-déposer valent sur toute la ligne, hors case de visibilité. Le
/// libellé garde sa place à gauche (un bouton élargi le centrerait).
pub(super) fn list_row_grip(
    ui: &mut egui::Ui,
    text: egui::RichText,
    selected: bool,
) -> egui::Response {
    let label = ui.add(list_row_label(text, selected).sense(egui::Sense::click_and_drag()));
    let rest = ui.allocate_response(
        egui::vec2(ui.available_width().max(0.0), label.rect.height()),
        egui::Sense::click_and_drag(),
    );
    label.union(rest)
}

/// Le fond d'une ligne : fond pâle et filet d'accent si elle est
/// sélectionnée, gris clair si elle est seulement survolée, rien sinon. Deux
/// formes à plat (fond, puis filet), jamais un `Shape::Vec` : les tests et
/// la tessellation les lisent une à une.
fn list_row_background(rect: egui::Rect, selected: bool, hovered: bool) -> [egui::Shape; 2] {
    if selected {
        let mark = egui::Rect::from_min_size(rect.min, egui::vec2(LIST_ROW_MARK, rect.height()));
        [
            egui::Shape::rect_filled(rect, 0.0, theme::list_row_selected_fill()),
            egui::Shape::rect_filled(mark, 0.0, theme::accent()),
        ]
    } else if hovered {
        [
            egui::Shape::rect_filled(rect, 0.0, theme::list_row_hover_fill()),
            egui::Shape::Noop,
        ]
    } else {
        [egui::Shape::Noop, egui::Shape::Noop]
    }
}

/// Fond d'une ligne composée de plusieurs widgets : réservé avant eux, posé
/// après (voir la doc de tête du module).
pub(super) struct ListRowBackground([egui::layers::ShapeIdx; 2]);

impl ListRowBackground {
    /// Réserve la place du fond dans la liste des formes, sous ce qui suit.
    pub(super) fn reserve(ui: &egui::Ui) -> Self {
        let painter = ui.painter();
        Self([
            painter.add(egui::Shape::Noop),
            painter.add(egui::Shape::Noop),
        ])
    }

    /// Pose le fond sous la ligne `row`, prolongée jusqu'au bord droit de
    /// l'`Ui` : la ligne court sur toute la largeur restante.
    pub(super) fn paint(self, ui: &egui::Ui, row: egui::Rect, selected: bool) {
        let rect = egui::Rect::from_min_max(
            row.min,
            egui::pos2(row.right().max(ui.max_rect().right()), row.bottom()),
        );
        let hovered = ui.rect_contains_pointer(rect);
        for (index, shape) in self
            .0
            .into_iter()
            .zip(list_row_background(rect, selected, hovered))
        {
            ui.painter().set(index, shape);
        }
    }
}

/// Une ligne de liste : icône facultative et libellé, sur toute la largeur
/// restante depuis son retrait. `dimmed` estompe le libellé (fenêtre
/// masquée du Project Manager). La réponse est un clic ; l'arbre
/// d'accessibilité la voit comme un bouton sélectionné ou non.
pub(super) fn list_row(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
) -> egui::Response {
    list_row_sensed(ui, icon, text, selected, dimmed, egui::Sense::click())
}

/// [`list_row`] qui se glisse aussi : toute la ligne saisit (glisser-déposer
/// des séries, `object_dnd.rs`).
pub(super) fn list_row_draggable(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    list_row_sensed(
        ui,
        None,
        text,
        selected,
        false,
        egui::Sense::click_and_drag(),
    )
}

fn list_row_sensed(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
    sense: egui::Sense,
) -> egui::Response {
    let galley = egui::WidgetText::from(list_row_text(text, selected)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let icon_width = if icon.is_some() {
        ICON_SIZE + LIST_ROW_GAP
    } else {
        0.0
    };
    let natural = 2.0 * LIST_ROW_PADDING.x + icon_width + galley.size().x;
    let size = egui::vec2(
        ui.available_width().max(natural),
        galley.size().y.max(ICON_SIZE) + 2.0 * LIST_ROW_PADDING.y,
    );
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let response = super::cursors::clickable(response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, text)
    });
    if ui.is_rect_visible(rect) {
        ui.painter()
            .extend(list_row_background(rect, selected, response.hovered()));
        let mut text_left = rect.left() + LIST_ROW_PADDING.x;
        if let Some(icon) = icon {
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(text_left, rect.center().y - ICON_SIZE / 2.0),
                egui::Vec2::splat(ICON_SIZE),
            );
            icon.paint_at(ui, icon_rect);
            text_left = icon_rect.right() + LIST_ROW_GAP;
        }
        let text_pos = egui::pos2(text_left, rect.center().y - galley.size().y / 2.0);
        let color = list_row_text_color(ui, selected, dimmed);
        ui.painter().galley(text_pos, galley, color);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                0.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::test_support::{contexte, textes};

    fn rects(output: &egui::FullOutput, fill: egui::Color32) -> Vec<egui::Rect> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                _ => None,
            })
            .collect()
    }

    fn image(context: &egui::Context, paint: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let mut paint = paint;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| paint(ui));
        output.textures_delta.clear();
        output
    }

    /// Une ligne sélectionnée : fond pâle pleine largeur, filet d'accent à
    /// gauche, libellé gras, et jamais le fond `selection.bg_fill` d'un
    /// bouton. Témoin : non sélectionnée, rien de tout cela.
    #[test]
    fn une_ligne_selectionnee_porte_fond_filet_et_gras() {
        let context = contexte();
        let output = image(&context, |ui| {
            list_row(ui, None, "Série A", true, false);
            list_row(ui, None, "Série B", false, false);
        });
        let rows = rects(&output, theme::list_row_selected_fill());
        let marks = rects(&output, theme::accent());
        assert_eq!(rows.len(), 1);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].left(), rows[0].left());
        assert_eq!(marks[0].height(), rows[0].height());
        assert_eq!(marks[0].width(), LIST_ROW_MARK);
        assert!(rows[0].width() > 300.0, "pleine largeur : {:?}", rows[0]);
        let button_fill = context.global_style().visuals.selection.bg_fill;
        assert!(rects(&output, button_fill).is_empty());
        let peints = textes(&output.shapes);
        assert!(peints.iter().any(|t| t.texte == "Série A" && t.gras));
        assert!(peints.iter().any(|t| t.texte == "Série B" && !t.gras));
    }

    /// Le fond réservé d'une ligne composée passe **sous** ses widgets et
    /// court jusqu'au bord droit ; sans sélection ni survol, il ne peint rien.
    #[test]
    fn le_fond_reserve_passe_sous_les_widgets_de_la_ligne() {
        let context = contexte();
        for selected in [true, false] {
            let output = image(&context, |ui| {
                let background = ListRowBackground::reserve(ui);
                let row = ui
                    .horizontal(|ui| {
                        ui.label("case");
                        ui.label(list_row_text("Objet", selected));
                    })
                    .response
                    .rect;
                background.paint(ui, row, selected);
            });
            let fill = output.shapes.iter().position(|s| {
                matches!(&s.shape, egui::Shape::Rect(r) if r.fill == theme::list_row_selected_fill())
            });
            let texte = output
                .shapes
                .iter()
                .position(|s| matches!(&s.shape, egui::Shape::Text(_)));
            if selected {
                let (fill, texte) = (fill.expect("fond"), texte.expect("texte"));
                assert!(fill < texte, "le fond est peint avant les widgets");
                let rows = rects(&output, theme::list_row_selected_fill());
                assert!(rows[0].right() >= 399.0, "jusqu'au bord : {:?}", rows[0]);
                assert_eq!(rects(&output, theme::accent()).len(), 1);
            } else {
                assert!(fill.is_none(), "témoin : rien sans sélection");
                assert!(rects(&output, theme::accent()).is_empty());
            }
        }
    }
}
