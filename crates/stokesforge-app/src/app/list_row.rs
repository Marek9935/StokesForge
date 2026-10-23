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

/// Hauteur **unique** d'une ligne de liste (#170, point 5) : la même pour une
/// ligne de [`list_row`] avec ou sans icône, sélectionnée ou non, et pour une
/// ligne composée (case de visibilité, échantillon, libellé : groupe, série,
/// objet de l'Object Manager, en-tête de dossier du Project Manager, ligne
/// série des propriétés). Valeur : la hauteur naturelle d'une ligne à icône
/// (ligne de texte de 18 px plus 2 x 3 px de marge, soit 24 px), qui loge la case à cocher
/// (`interact_size.y`) et l'échantillon de 18 px sans les rogner.
pub(crate) const LIST_ROW_HEIGHT: f32 = 24.0;

/// Marges intérieures d'une ligne peinte par [`list_row`].
const LIST_ROW_PADDING: egui::Vec2 = egui::vec2(6.0, 3.0);

/// Écart entre l'icône et le libellé.
const LIST_ROW_GAP: f32 = 5.0;

/// Impose [`LIST_ROW_HEIGHT`] aux widgets d'une ligne composée : `add` s'écrit
/// avec la hauteur d'interaction (`interact_size.y`) relevée à la hauteur de
/// ligne, que lisent le `ui.horizontal` (hauteur de départ de la rangée), la
/// case à cocher, le bouton et le champ de saisie ; chaque widget se centre
/// donc dans une rangée de [`LIST_ROW_HEIGHT`], sans sous-`Ui` neuf (les
/// identifiants egui et le glisser-déposer ne changent pas). Le réglage est
/// rendu à l'appelant dès `add` terminée.
pub(super) fn with_list_row_height<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let before = raise_to_list_row_height(ui);
    let result = add(ui);
    restore_row_height(ui, before);
    result
}

/// La moitié « poser » de [`with_list_row_height`], pour une en-tête repliable
/// dont la réponse (`HeaderResponse`) emprunte l'`Ui` jusqu'à son `.body(…)` :
/// pose [`LIST_ROW_HEIGHT`] et rend la hauteur d'avant, à rendre par
/// [`restore_row_height`] une fois l'en-tête et son corps peints.
pub(super) fn raise_to_list_row_height(ui: &mut egui::Ui) -> f32 {
    let before = ui.spacing().interact_size.y;
    ui.spacing_mut().interact_size.y = LIST_ROW_HEIGHT;
    before
}

/// La moitié « rendre » : voir [`raise_to_list_row_height`].
pub(super) fn restore_row_height(ui: &mut egui::Ui, before: f32) {
    ui.spacing_mut().interact_size.y = before;
}

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

/// Le contour de focus clavier d'une ligne (#170, point 6) : celui de
/// `selection.stroke`, posé en retrait d'un point. Une seule écriture pour
/// toutes les lignes : une ligne de [`list_row`] qui a le focus d'egui, la ligne
/// du curseur clavier du Project Manager (qui suit son propre focus, pas celui
/// d'egui) et celle de l'Object Manager (une ancre de focus pour tout le
/// panneau).
pub(super) fn paint_focus_outline(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        0.0,
        ui.visuals().selection.stroke,
        egui::StrokeKind::Inside,
    );
}

/// Où va le curseur clavier depuis `cursor` quand on appuie sur ↑ (`step` = -1)
/// ou ↓ (+1) dans `visible`, les lignes **visibles** dans l'ordre de lecture :
/// la voisine, sans passer d'un bout à l'autre. Sans curseur (ou si le curseur
/// n'est pas visible), ↓ va à la première ligne et ↑ à la dernière. `None` si
/// rien n'est visible. Partagée par le Project Manager et l'Object Manager
/// (#170, point 6).
pub(super) fn neighbour<T: Copy + PartialEq>(
    visible: &[T],
    cursor: Option<T>,
    step: i32,
) -> Option<T> {
    let last = visible.len().checked_sub(1)?;
    let target = match cursor.and_then(|entry| visible.iter().position(|e| *e == entry)) {
        Some(at) if step < 0 => at.saturating_sub(1),
        Some(at) => (at + 1).min(last),
        None if step < 0 => last,
        None => 0,
    };
    visible.get(target).copied()
}

/// Les touches de navigation d'une liste, lues (et consommées) une fois par
/// image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavKey {
    Up,
    Down,
    Enter,
}

/// Lit ↑, ↓ et Entrée dans le registre des touches. Les variantes Maj (↑ ↓) et
/// Alt+↓ sont consommées **d'abord** et ne font rien : `consume_shortcut`
/// ignore Maj et Alt en trop et les prendrait pour ↑ ou ↓ simples. Ctrl+↑ et
/// Ctrl+↓ (déplacer une série) ne correspondent pas : le modificateur Ctrl, lui,
/// doit être exact.
pub(super) fn read_navigation(input: &mut egui::InputState) -> Option<NavKey> {
    use super::shortcuts::{
        KEY_ALT_ARROW_DOWN, KEY_ARROW_DOWN, KEY_ARROW_UP, KEY_ENTER, KEY_SHIFT_ARROW_DOWN,
        KEY_SHIFT_ARROW_UP,
    };
    let swallowed = [
        input.consume_shortcut(&KEY_SHIFT_ARROW_UP),
        input.consume_shortcut(&KEY_SHIFT_ARROW_DOWN),
        input.consume_shortcut(&KEY_ALT_ARROW_DOWN),
    ]
    .contains(&true);
    if swallowed {
        return None;
    }
    if input.consume_shortcut(&KEY_ARROW_UP) {
        Some(NavKey::Up)
    } else if input.consume_shortcut(&KEY_ARROW_DOWN) {
        Some(NavKey::Down)
    } else if input.consume_shortcut(&KEY_ENTER) {
        Some(NavKey::Enter)
    } else {
        None
    }
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
    /// l'`Ui` (la ligne court sur toute la largeur restante), et le contour de
    /// focus clavier quand elle porte le curseur (`focused`, #170 point 6).
    pub(super) fn paint(self, ui: &egui::Ui, row: egui::Rect, selected: bool, focused: bool) {
        let rect = egui::Rect::from_min_max(
            row.min,
            egui::pos2(
                row.right().max(ui.max_rect().right()),
                row.bottom().max(row.top() + LIST_ROW_HEIGHT),
            ),
        );
        let hovered = ui.rect_contains_pointer(rect);
        for (index, shape) in self
            .0
            .into_iter()
            .zip(list_row_background(rect, selected, hovered))
        {
            ui.painter().set(index, shape);
        }
        if focused {
            paint_focus_outline(ui, rect);
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
    list_row_sensed(
        ui,
        icon,
        text,
        selected,
        dimmed,
        false,
        egui::Sense::click(),
    )
}

/// [`list_row`] dont le contour de focus est peint aussi quand l'appelant sait
/// que la ligne porte le curseur clavier sans avoir le focus d'egui (le Project
/// Manager suit son focus lui-même, #170 point 6).
pub(super) fn list_row_focused(
    ui: &mut egui::Ui,
    icon: Option<egui::Image<'static>>,
    text: &str,
    selected: bool,
    dimmed: bool,
    focused: bool,
) -> egui::Response {
    list_row_sensed(
        ui,
        icon,
        text,
        selected,
        dimmed,
        focused,
        egui::Sense::click(),
    )
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
    focused: bool,
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
    let size = egui::vec2(ui.available_width().max(natural), LIST_ROW_HEIGHT);
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
        if focused || response.has_focus() {
            paint_focus_outline(ui, rect);
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

    /// #170, point 5 : une ligne de liste a toujours `LIST_ROW_HEIGHT`, avec ou
    /// sans icône, sélectionnée (gras) ou non. Témoin : la hauteur d'une ligne
    /// n'est pas celle d'un simple libellé.
    #[test]
    fn toute_ligne_de_liste_a_la_hauteur_unique() {
        let context = contexte();
        let mut hauteurs = Vec::new();
        image(&context, |ui| {
            for (icone, selected) in [(false, false), (false, true), (true, false), (true, true)] {
                let icon = icone.then(|| {
                    crate::icons::icon_image(
                        crate::icons::Icon::Folder,
                        crate::icons::IconRole::Folder,
                    )
                });
                hauteurs.push(list_row(ui, icon, "Série", selected, false).rect.height());
            }
            hauteurs.push(list_row_draggable(ui, "Série", false).rect.height());
            hauteurs.push(ui.label("Série").rect.height());
        });
        assert_eq!(hauteurs[..5], [LIST_ROW_HEIGHT; 5], "{hauteurs:?}");
        assert!(
            hauteurs[5] < LIST_ROW_HEIGHT,
            "témoin : un libellé nu est plus bas : {hauteurs:?}"
        );
    }

    /// `with_list_row_height` loge case, bouton et champ dans une rangée de
    /// `LIST_ROW_HEIGHT`, centrés sur son milieu, puis rend la hauteur
    /// d'interaction d'avant. Témoin : sans lui, la rangée est plus basse.
    #[test]
    fn la_rangee_composee_prend_la_hauteur_unique_et_la_rend() {
        let context = contexte();
        let mut mesures = Vec::new();
        image(&context, |ui| {
            let avant = ui.spacing().interact_size.y;
            for imposer in [true, false] {
                let mut case = true;
                let mut rangee = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        let c = ui.checkbox(&mut case, "").rect.center().y;
                        let b = ui.button("Nom").rect.center().y;
                        (c, b)
                    })
                };
                let sortie = if imposer {
                    with_list_row_height(ui, rangee)
                } else {
                    rangee(ui)
                };
                let (c, b) = sortie.inner;
                mesures.push((sortie.response.rect, c, b));
            }
            assert_eq!(ui.spacing().interact_size.y, avant, "hauteur rendue");
        });
        let (avec, c, b) = mesures[0];
        assert_eq!(avec.height(), LIST_ROW_HEIGHT);
        assert!((c - avec.center().y).abs() < 0.5 && (b - avec.center().y).abs() < 0.5);
        assert!(
            mesures[1].0.height() < LIST_ROW_HEIGHT,
            "témoin : {mesures:?}"
        );
    }

    /// Le fond d'une ligne composée est au moins `LIST_ROW_HEIGHT` de haut,
    /// même si la rangée mesurée est plus basse.
    #[test]
    fn le_fond_reserve_impose_la_hauteur_minimale() {
        let context = contexte();
        let output = image(&context, |ui| {
            let background = ListRowBackground::reserve(ui);
            let row = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 18.0));
            background.paint(ui, row, true, false);
        });
        let rows = rects(&output, theme::list_row_selected_fill());
        assert_eq!(rows[0].height(), LIST_ROW_HEIGHT);
        let output = image(&context, |ui| {
            let background = ListRowBackground::reserve(ui);
            let row = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 40.0));
            background.paint(ui, row, true, false);
        });
        assert_eq!(
            rects(&output, theme::list_row_selected_fill())[0].height(),
            40.0,
            "témoin : une rangée plus haute n'est pas rognée"
        );
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
                background.paint(ui, row, selected, false);
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
