//! Dialogues **modaux** : [`modal_dialog`] et [`modal_dialog_with`], la brique
//! de toute boîte de dialogue et de tout assistant — même cadre, même barre
//! de titre, même blocage du reste de l'interface. Elle pose la fenêtre par
//! la brique des fenêtres de contenu (`window_chrome::show_window_in_layer`).
//!
//! Sorti d'`app.rs` tel quel (#117).

use eframe::egui;

use crate::{theme, tr};

use super::keyboard_focus_outside_grid;
use super::window_chrome::{
    floating_resize_handles, show_window_contents_bounded, show_window_in_layer, window_header,
    HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
};

/// Issue d'une image de [`modal_dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogOutcome {
    /// Toujours ouvert.
    Open,
    /// « Valider » (bouton par défaut : Entrée).
    Accept,
    /// « Annuler », croix ou Échap.
    Cancel,
}

/// Hauteur de la bande des boutons « Valider » / « Annuler ».
const DIALOG_FOOTER_HEIGHT: f32 = 38.0;

/// Durée d'une alerte de clic bloqué ([`modal_dialog`]) : deux impulsions.
const MODAL_FLASH_SECONDS: f64 = 0.6;

/// Position et taille d'un dialogue modal, gardées en mémoire egui le temps
/// qu'il est ouvert.
#[derive(Clone, Copy)]
struct DialogGeometry {
    pos: egui::Pos2,
    size: egui::Vec2,
}

/// Dialogue **modal** : au premier plan, le reste de l'interface ne reçoit
/// plus aucun clic, et un clic ailleurs fait clignoter le contour du
/// dialogue en rouge foncé ([`theme::blocked_flash_color`]) pour le
/// désigner. Il se ferme par « Valider » (bouton par défaut, Entrée hors
/// saisie de texte) ou par « Annuler » (croix, Échap) : voir [`DialogOutcome`].
///
/// Même brique que les fenêtres de contenu : [`show_window_in_layer`] (cadre,
/// contour actif, rognage), [`window_header`] (placement
/// [`HeaderPlacement::Dialog`]) et [`floating_resize_handles`]. Ne diffère
/// que par l'ordre de calque (`Foreground`, au-dessus des fenêtres en
/// `Middle`) et par le calque plein écran qui capte les clics. Les menus
/// déroulants et le sélecteur de couleur ouverts depuis le dialogue restent
/// utilisables : ils sont créés après lui, donc au-dessus.
///
/// Limite : les fenêtres OS détachées sont d'autres viewports, que ce calque
/// ne couvre pas.
pub(super) fn modal_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    // Le pied par défaut : « OK », toujours actif — ce que les propriétés du
    // graphe emploient. Les textes viennent du catalogue ; le pied les emprunte.
    let accept_label = tr!("dialog-ok");
    let accept_hint = tr!("dialog-ok", "hint");
    modal_dialog_with(
        ctx,
        id,
        title,
        default_size,
        DialogFooter {
            accept_label: &accept_label,
            accept_hint: &accept_hint,
            accept_enabled: true,
        },
        |_| {},
        add_contents,
    )
}

/// Le pied de page d'un [`modal_dialog_with`] : libellé et grisage du bouton par
/// défaut. [`modal_dialog`] y met « OK », toujours actif — ce que les propriétés
/// du graphe emploient.
pub(super) struct DialogFooter<'a> {
    pub(super) accept_label: &'a str,
    pub(super) accept_hint: &'a str,
    pub(super) accept_enabled: bool,
}

/// [`modal_dialog`] avec un bouton par défaut personnalisé (`footer`) et des
/// boutons supplémentaires à gauche d'« Annuler » (`extra_buttons`, ajoutés
/// de droite à gauche comme les autres) : c'est ainsi que l'assistant d'import
/// garde « Précédent / Suivant » tout en étant **la même fenêtre** que les
/// propriétés du graphe — même cadre, même barre de titre, même blocage du
/// reste de l'interface.
pub(super) fn modal_dialog_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let screen = ctx.input(|input| input.content_rect());
    let geometry_id = id.with("geometry");
    let flash_id = id.with("flash_start");
    let stored = ctx.data(|data| data.get_temp::<DialogGeometry>(geometry_id));
    let just_opened = stored.is_none();
    let mut geometry = stored.unwrap_or_else(|| {
        let size = default_size.min(screen.size());
        DialogGeometry {
            pos: screen.center() - size / 2.0,
            size,
        }
    });
    geometry.size = geometry
        .size
        .max(MIN_CONTENT_WINDOW_SIZE)
        .min(screen.size());
    geometry.pos = geometry
        .pos
        .clamp(screen.min, (screen.max - geometry.size).max(screen.min));
    let layer = egui::LayerId::new(egui::Order::Foreground, id);

    // Le calque qui capte les clics : tout le reste de l'interface est sous lui.
    // Il est **invisible** : pas de voile sombre sur le reste de l'application,
    // pour qu'on continue de voir le rendu du graphe pendant qu'on règle ses
    // propriétés (issue #20). Le clignotement du contour dit déjà que le
    // reste est bloqué.
    let backdrop = egui::Area::new(id.with("backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .sense(egui::Sense::click_and_drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        });
    let blocked_click =
        backdrop.response.contains_pointer() && ctx.input(|input| input.pointer.any_pressed());
    let now = ctx.input(|input| input.time);
    if blocked_click {
        ctx.data_mut(|data| data.insert_temp(flash_id, now));
    }
    if blocked_click || just_opened {
        ctx.move_to_top(layer);
    }
    let flash = ctx
        .data(|data| data.get_temp::<f64>(flash_id))
        .map(|start| now - start)
        .filter(|elapsed| *elapsed < MODAL_FLASH_SECONDS)
        .map(|elapsed| {
            ctx.request_repaint();
            // Deux impulsions sur la durée : 0 → 1 → 0, deux fois.
            0.5 * (1.0 - (std::f64::consts::TAU * elapsed / (MODAL_FLASH_SECONDS / 2.0)).cos())
        });

    let popup_open = egui::Popup::is_any_open(ctx);
    let window_rect = egui::Rect::from_min_size(geometry.pos, geometry.size);
    let mut header_actions = WindowHeaderActions::default();
    let mut button_outcome = DialogOutcome::Open;
    let mut position = geometry.pos;
    let mut size = geometry.size;
    show_window_in_layer(
        ctx,
        id,
        egui::Order::Foreground,
        window_rect,
        screen,
        true,
        |ui| {
            header_actions = window_header(ui, id, title, None, false, HeaderPlacement::Dialog);
            show_window_contents_bounded(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let split = (full.bottom() - DIALOG_FOOTER_HEIGHT).max(full.top());
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            full.min,
                            egui::pos2(full.right(), split),
                        ))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                body.set_clip_rect(ui.clip_rect().intersect(body.max_rect()));
                add_contents(&mut body);
                let mut footer = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            egui::pos2(full.left(), split),
                            full.max,
                        ))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                footer.painter().hline(
                    full.x_range(),
                    split,
                    footer.visuals().widgets.noninteractive.bg_stroke,
                );
                // Premier ajouté = le plus à droite : « Valider », par défaut.
                let accept = egui::Button::new(footer_spec.accept_label)
                    .fill(footer.visuals().selection.bg_fill)
                    .min_size(egui::vec2(84.0, 24.0));
                if footer
                    .add_enabled(footer_spec.accept_enabled, accept)
                    .on_hover_text(footer_spec.accept_hint)
                    .clicked()
                {
                    button_outcome = DialogOutcome::Accept;
                }
                let cancel =
                    egui::Button::new(tr!("dialog-cancel")).min_size(egui::vec2(84.0, 24.0));
                if footer
                    .add(cancel)
                    .on_hover_text(tr!("dialog-cancel", "hint"))
                    .clicked()
                {
                    button_outcome = DialogOutcome::Cancel;
                }
                extra_buttons(&mut footer);
            });
        },
        |ui, rect| {
            floating_resize_handles(ui, id, rect, &mut position, &mut size);
            if let Some(intensity) = flash {
                let style = ui.style();
                ui.painter().rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    egui::Stroke::new(
                        2.5,
                        theme::blocked_flash_color().gamma_multiply(intensity as f32),
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        },
    );
    position += header_actions.drag_delta;
    geometry = DialogGeometry {
        pos: position,
        size,
    };
    ctx.data_mut(|data| data.insert_temp(geometry_id, geometry));

    // Échap : consommé par le sélecteur de couleur ou une liste ouverts (ils
    // le lisent avant), sinon il annule. Entrée valide, sauf pendant une
    // saisie de texte (où elle termine la saisie).
    let escape = !popup_open
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let enter = !popup_open
        && !keyboard_focus_outside_grid(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let outcome = if header_actions.close || escape {
        DialogOutcome::Cancel
    } else if enter && footer_spec.accept_enabled {
        DialogOutcome::Accept
    } else {
        button_outcome
    };
    if outcome != DialogOutcome::Open {
        ctx.data_mut(|data| {
            data.remove::<DialogGeometry>(geometry_id);
            data.remove::<f64>(flash_id);
        });
    }
    outcome
}
