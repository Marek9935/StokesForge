//! Dialogues **modaux** : [`modal_dialog`], [`modal_dialog_with`],
//! [`modal_wizard`] et [`modal_read_only`], la brique de toute boîte de
//! dialogue et de tout assistant — même cadre, même barre de titre, même
//! blocage du reste de l'interface. Elle pose la fenêtre par la brique des
//! fenêtres de contenu (`window_chrome::show_window_in_layer`).
//!
//! Sorti d'`app.rs` tel quel (#117), puis étendu par #228
//! (`docs/standard-interface.md`, § Fenêtres et dialogues) :
//!
//! - **ordre des boutons** (S-F6) : un dialogue range ses boutons secondaires à
//!   gauche, puis Cancel, puis le principal à droite ; un assistant met Cancel
//!   à gauche et groupe Back · Next · Finish à droite ; une fenêtre en lecture
//!   seule n'a qu'un bouton Close (S-F2) ;
//! - **zone de message** du pied, à gauche des boutons (S-M7) : l'erreur d'un
//!   geste du dialogue (jamais dans la barre d'état ; `AppState::dialog_error`,
//!   publiée par [`sync_dialogs`]), sinon la **raison** du bouton principal
//!   grisé (S-F10 → S-R14) — qui est aussi sa bulle. La raison vient de l'appelant :
//!   `DialogFooter::accept_disabled` (#226) quand `accept_enabled` est faux ;
//! - **Échap** (S-F7) : celui qui sort d'un champ texte ne ferme pas le
//!   dialogue, le suivant fait Cancel ;
//! - **taille et position** mémorisées par sorte de dialogue (S-F8), dans les
//!   préférences : reprises à l'ouverture, rendues à la fermeture par
//!   [`sync_dialogs`] (`Action::RememberDialogGeometry`). La sorte est tirée
//!   de l'identifiant du dialogue ;
//! - **confirmations** (#229, S-F4, S-F5 → S-R4, S-R5) : [`modal_confirm`] — Cancel
//!   est le bouton par défaut (encadré ; Entrée et Échap l'actionnent), le
//!   verbe (Delete…) a le style normal et rougit au survol **et** au focus
//!   clavier ; [`modal_save_changes`] — « Discard Changes · Cancel · Save »,
//!   seule exception écrite : Entrée = Save ; [`modal_alert`] — l'échec grave
//!   (S-M8), un bouton Close et une action possible ;
//! - **focus initial** (S-F9) : le premier champ de saisie l'a à l'ouverture.
//!   egui ne dit pas quels widgets sont des champs texte : à l'ouverture, une
//!   ancre invisible prend le focus en tête du contenu, puis le focus avance
//!   d'un widget par passe, toutes dans la même image (`request_discard` :
//!   rien n'est peint entre deux pas ; la navigation de Tab d'egui) jusqu'au premier
//!   champ texte du dialogue (un `TextEdit`, reconnu à l'état qu'il garde
//!   dans la mémoire d'egui). Il s'arrête — focus rendu — au pied, hors du
//!   dialogue, au bout de [`FOCUS_WALK_STEPS`] widgets, ou dès que
//!   l'utilisateur tape ou clique (le geste s'applique alors sans focus).

use std::collections::BTreeMap;

use eframe::egui;

use crate::theme;
use stokesforge_app_state::preferences::DialogGeometry as SavedGeometry;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::grid::keyboard_focus_outside_grid;
use super::window_chrome::{
    floating_resize_handles, show_window_contents_bounded, show_window_in_layer, window_header,
    HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
};

/// Issue d'une image de [`modal_dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogOutcome {
    /// Toujours ouvert.
    Open,
    /// « Valider » (bouton par défaut : Entrée), ou Close d'une fenêtre en
    /// lecture seule.
    Accept,
    /// « Annuler », croix ou Échap.
    Cancel,
}

/// Hauteur de la bande des boutons « Valider » / « Annuler ».
const DIALOG_FOOTER_HEIGHT: f32 = 38.0;

/// Taille minimale d'un bouton du pied.
pub(super) const FOOTER_BUTTON_SIZE: egui::Vec2 = egui::vec2(84.0, 24.0);

/// Durée d'une alerte de clic bloqué ([`modal_dialog`]) : deux impulsions.
const MODAL_FLASH_SECONDS: f64 = 0.6;

/// Nombre de widgets que le focus initial (S-F9) parcourt au plus avant de
/// renoncer.
const FOCUS_WALK_STEPS: u8 = 32;

/// Position et taille d'un dialogue modal, gardées en mémoire egui le temps
/// qu'il est ouvert.
#[derive(Clone, Copy)]
struct DialogGeometry {
    pos: egui::Pos2,
    size: egui::Vec2,
}

/// La disposition du pied (S-F2, S-F4, S-F6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FooterLayout {
    /// `[secondaires] [message] [Cancel] [principal]`.
    Dialog,
    /// `[Cancel] [message] [Back · Next] [Finish]`.
    Wizard,
    /// `[secondaires] [message] [Close]` : lecture seule (S-F2), échec grave
    /// (S-M8, l'action possible en secondaire).
    ReadOnly,
    /// `[message] [Cancel] [verbe]` : confirmation (S-F4, S-F5) ; Cancel est le
    /// bouton par défaut, Entrée l'actionne ; le verbe est destructeur.
    Confirm,
    /// `[Discard Changes] [message] [Cancel] [Save]` (S-F4) : Entrée = Save,
    /// Discard Changes destructeur.
    SaveChanges,
}

/// Issue d'une image du corps commun : celle d'un [`DialogOutcome`], plus le
/// bouton secondaire de la brique (Discard Changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameOutcome {
    Open,
    Accept,
    Cancel,
    Secondary,
}

impl FrameOutcome {
    /// Pour les dialogues sans secondaire de brique.
    fn dialog(self) -> DialogOutcome {
        match self {
            Self::Open | Self::Secondary => DialogOutcome::Open,
            Self::Accept => DialogOutcome::Accept,
            Self::Cancel => DialogOutcome::Cancel,
        }
    }
}

/// Le message du pied (S-M7, S-F10) : l'erreur passe devant la raison du bouton
/// principal grisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FooterMessage<'a> {
    Error(&'a str),
    Reason(&'a str),
}

/// La décision du pied, pure : une erreur s'il y en a une ; sinon, bouton
/// principal grisé, sa raison ; sinon rien.
fn footer_message<'a>(
    error: Option<&'a str>,
    accept_enabled: bool,
    reason: &'a str,
) -> Option<FooterMessage<'a>> {
    match error.filter(|error| !error.is_empty()) {
        Some(error) => Some(FooterMessage::Error(error)),
        None if !accept_enabled && !reason.is_empty() => Some(FooterMessage::Reason(reason)),
        None => None,
    }
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
            accept_disabled: None,
        },
        |_| {},
        add_contents,
    )
}

/// Le pied de page d'un [`modal_dialog_with`] : libellé et grisage du bouton par
/// défaut. [`modal_dialog`] y met « OK », toujours actif — ce que les propriétés
/// du graphe emploient. `accept_hint` est la bulle du bouton actif ; quand
/// `accept_enabled` est faux, `accept_disabled` est la **raison** du grisage,
/// qui dit quoi faire (S-G1) : elle s'écrit dans la zone de message du pied et
/// dans la bulle du bouton grisé (S-F10).
pub(super) struct DialogFooter<'a> {
    pub(super) accept_label: &'a str,
    pub(super) accept_hint: &'a str,
    pub(super) accept_enabled: bool,
    /// Pourquoi le bouton principal est grisé et quoi faire (#226, S-G1) ;
    /// `None` quand il ne l'est jamais.
    pub(super) accept_disabled: Option<&'a str>,
}

impl DialogFooter<'_> {
    /// La raison du grisage ; à défaut, la bulle du bouton (#226).
    fn disabled_reason(&self) -> &str {
        self.accept_disabled.unwrap_or(self.accept_hint)
    }
}

/// [`modal_dialog`] avec un bouton par défaut personnalisé (`footer`) et des
/// boutons **secondaires** (`extra_buttons`), posés à gauche du pied, de
/// gauche à droite, avant la zone de message ; Cancel puis le bouton principal
/// restent à droite (S-F6).
pub(super) fn modal_dialog_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::Dialog,
        footer_spec,
        extra_buttons,
        add_contents,
    )
    .dialog()
}

/// Un **assistant** (S-F6) : Cancel à gauche, les boutons d'étape
/// (`step_buttons`, ajoutés **de droite à gauche** : Next puis Back) groupés à
/// droite contre le bouton final (`footer`, Finish). Même fenêtre que
/// [`modal_dialog_with`] ; l'assistant d'import en est l'usage.
pub(super) fn modal_wizard(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    footer_spec: DialogFooter<'_>,
    step_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::Wizard,
        footer_spec,
        step_buttons,
        add_contents,
    )
    .dialog()
}

/// Une fenêtre modale en **lecture seule** (S-F2) : un seul bouton, Close
/// (Entrée), qui rend [`DialogOutcome::Accept`] ; Échap et la croix rendent
/// [`DialogOutcome::Cancel`]. Les deux ferment sans rien changer.
pub(super) fn modal_read_only(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::ReadOnly,
        DialogFooter {
            accept_label: &label,
            accept_hint: &hint,
            accept_enabled: true,
            accept_disabled: None,
        },
        |_| {},
        add_contents,
    )
    .dialog()
}

/// Issue d'une image de [`modal_confirm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfirmOutcome {
    /// Toujours ouvert.
    Open,
    /// Le verbe (Delete…) : seulement par son bouton, jamais par Entrée.
    Confirm,
    /// Cancel (bouton par défaut, Entrée), croix ou Échap.
    Cancel,
}

/// Une **confirmation** (S-F3–S-F5 → S-R4, S-R5) : seulement pour ce que Ctrl+Z ne
/// rattrape pas. Pied `[Cancel] [verbe]` : Cancel est le bouton par défaut,
/// encadré, et Entrée comme Échap l'actionnent ; le verbe (`verb`, sa bulle
/// `verb_hint`) a le style normal et rougit au survol et au focus clavier.
pub(super) fn modal_confirm(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    verb: &str,
    verb_hint: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> ConfirmOutcome {
    match modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::Confirm,
        DialogFooter {
            accept_label: verb,
            accept_hint: verb_hint,
            accept_enabled: true,
            accept_disabled: None,
        },
        |_| {},
        add_contents,
    ) {
        FrameOutcome::Open | FrameOutcome::Secondary => ConfirmOutcome::Open,
        FrameOutcome::Accept => ConfirmOutcome::Confirm,
        FrameOutcome::Cancel => ConfirmOutcome::Cancel,
    }
}

/// Issue d'une image de [`modal_save_changes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveChangesOutcome {
    /// Toujours ouvert.
    Open,
    /// Discard Changes.
    Discard,
    /// Cancel, croix ou Échap.
    Cancel,
    /// Save (bouton par défaut : Entrée).
    Save,
}

/// Le dialogue des changements non enregistrés (S-F4) : « Discard Changes ·
/// Cancel · Save ». Exception écrite à S-R4 : Entrée = Save, Échap = Cancel ;
/// Discard Changes, à gauche, rougit au survol et au focus clavier.
pub(super) fn modal_save_changes(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> SaveChangesOutcome {
    let label = tr!("unsaved-save");
    let hint = tr!("unsaved-save", "hint");
    match modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::SaveChanges,
        DialogFooter {
            accept_label: &label,
            accept_hint: &hint,
            accept_enabled: true,
            accept_disabled: None,
        },
        |_| {},
        add_contents,
    ) {
        FrameOutcome::Open => SaveChangesOutcome::Open,
        FrameOutcome::Accept => SaveChangesOutcome::Save,
        FrameOutcome::Cancel => SaveChangesOutcome::Cancel,
        FrameOutcome::Secondary => SaveChangesOutcome::Discard,
    }
}

/// Issue d'une image de [`modal_alert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AlertOutcome {
    /// Toujours ouvert.
    Open,
    /// Close (Entrée), croix ou Échap : le message est lu.
    Close,
    /// L'action proposée.
    Action,
}

/// Un **échec grave** hors fenêtre (S-M8) : dialogue bloquant, le message dans
/// le contenu, un bouton Close (Entrée, Échap) et l'action possible
/// (`action` : libellé et bulle), à gauche.
pub(super) fn modal_alert(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    action: (&str, &str),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> AlertOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    let mut acted = false;
    let outcome = modal_frame(
        ctx,
        id,
        title,
        default_size,
        FooterLayout::ReadOnly,
        DialogFooter {
            accept_label: &label,
            accept_hint: &hint,
            accept_enabled: true,
            accept_disabled: None,
        },
        |ui| {
            acted = ui
                .add(egui::Button::new(action.0).min_size(FOOTER_BUTTON_SIZE))
                .on_hover_text(action.1)
                .clicked();
        },
        add_contents,
    );
    match outcome {
        _ if acted => AlertOutcome::Action,
        FrameOutcome::Open | FrameOutcome::Secondary => AlertOutcome::Open,
        FrameOutcome::Accept | FrameOutcome::Cancel => AlertOutcome::Close,
    }
}

/// La sorte d'un dialogue, clé de sa géométrie dans les préférences (S-F8) :
/// son identifiant egui, stable d'une session à l'autre.
fn dialog_kind(id: egui::Id) -> String {
    format!("{:016x}", id.value())
}

/// Les géométries mémorisées, recopiées des préférences dans la mémoire d'egui.
#[derive(Clone, Default)]
struct RememberedGeometries(BTreeMap<String, SavedGeometry>);

fn remembered_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_geometries")
}

/// Les géométries des dialogues fermés, à rendre aux préférences.
#[derive(Clone, Default)]
struct ClosedGeometries(Vec<(String, SavedGeometry)>);

fn closed_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_geometries_closed")
}

/// L'erreur du dialogue ouvert (S-M7), publiée par [`sync_dialogs`].
fn dialog_error_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_error")
}

/// Le lien entre les dialogues et l'état, une fois par image après les
/// dialogues : publie l'erreur du dialogue ouvert pour son pied (S-M7), recopie
/// une fois les géométries mémorisées (S-F8) et rend en actions celles des
/// dialogues qui viennent de se fermer.
pub(super) fn sync_dialogs(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let error = state.dialog_error().map(str::to_owned);
    let closed = ctx.data_mut(|data| {
        data.insert_temp(dialog_error_id(), error);
        if data
            .get_temp::<RememberedGeometries>(remembered_id())
            .is_none()
        {
            data.insert_temp(
                remembered_id(),
                RememberedGeometries(state.dialog_geometries().clone()),
            );
        }
        data.remove_temp::<ClosedGeometries>(closed_id())
    });
    for (kind, geometry) in closed.map(|closed| closed.0).unwrap_or_default() {
        actions.push(Action::RememberDialogGeometry { kind, geometry });
    }
}

/// Le focus initial en cours (S-F9) : combien de widgets il a déjà passés, et
/// les boutons du pied de l'image précédente (il s'arrête sur eux).
#[derive(Clone, Default)]
struct FocusWalk {
    steps: u8,
    footer: Vec<egui::Id>,
}

/// `id` est-il un champ texte posé dans `layer` ?
fn text_field_in(ctx: &egui::Context, id: egui::Id, layer: egui::LayerId) -> bool {
    egui::TextEdit::load_state(ctx, id).is_some()
        && ctx
            .read_response(id)
            .is_some_and(|response| response.layer_id == layer)
}

/// L'image a-t-elle un geste (touche, texte, clic) ?
fn any_gesture(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { pressed: true, .. }
                    | egui::Event::Text(_)
                    | egui::Event::PointerButton { pressed: true, .. }
            )
        })
    })
}

/// Le corps commun des dialogues modaux.
#[expect(clippy::too_many_arguments)]
fn modal_frame(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    default_size: egui::Vec2,
    layout: FooterLayout,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> FrameOutcome {
    let screen = ctx.input(|input| input.content_rect());
    let geometry_id = id.with("geometry");
    let flash_id = id.with("flash_start");
    let walk_id = id.with("initial_focus");
    let anchor_id = id.with("focus_anchor");
    let kind = dialog_kind(id);
    let stored = ctx.data(|data| data.get_temp::<DialogGeometry>(geometry_id));
    let just_opened = stored.is_none();
    let mut geometry = stored.unwrap_or_else(|| {
        let remembered = ctx
            .data(|data| data.get_temp::<RememberedGeometries>(remembered_id()))
            .and_then(|remembered| remembered.0.get(&kind).copied())
            .filter(SavedGeometry::is_valid);
        match remembered {
            Some(saved) => DialogGeometry {
                pos: egui::pos2(saved.x, saved.y),
                size: egui::vec2(saved.width, saved.height),
            },
            None => {
                let size = default_size.min(screen.size());
                DialogGeometry {
                    pos: screen.center() - size / 2.0,
                    size,
                }
            }
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

    // S-F9 : un pas du focus initial, avant que les widgets se posent (egui
    // ne suit une direction de focus que pendant la passe où elle est donnée).
    if just_opened {
        ctx.data_mut(|data| data.insert_temp(walk_id, FocusWalk::default()));
    } else {
        advance_initial_focus(ctx, walk_id, anchor_id, layer);
    }
    // S-F7 : l'Échap qui sort d'un champ texte du dialogue ne le ferme pas.
    // egui rend le focus sur Échap dès le début de la passe : on lit donc
    // celui de la fin de l'image précédente.
    let text_focus_id = id.with("text_focus");
    let editing_before = ctx
        .data(|data| data.get_temp::<bool>(text_focus_id))
        .unwrap_or(false);

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

    let error = ctx
        .data(|data| data.get_temp::<Option<String>>(dialog_error_id()))
        .flatten();
    let popup_open = egui::Popup::is_any_open(ctx);
    let window_rect = egui::Rect::from_min_size(geometry.pos, geometry.size);
    let mut header_actions = WindowHeaderActions::default();
    let mut clicks = FooterClicks::default();
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
                // L'ancre du focus initial (S-F9) : invisible, en tête du contenu.
                body.interact(
                    egui::Rect::from_min_size(body.max_rect().min, egui::Vec2::ZERO),
                    anchor_id,
                    egui::Sense::focusable_noninteractive(),
                );
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
                let message = footer_message(
                    error.as_deref(),
                    footer_spec.accept_enabled,
                    footer_spec.disabled_reason(),
                );
                clicks = paint_footer(&mut footer, layout, &footer_spec, message, extra_buttons);
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
    let editing_now = ctx
        .memory(|memory| memory.focused())
        .is_some_and(|focused| text_field_in(ctx, focused, layer));
    ctx.data_mut(|data| {
        data.insert_temp(geometry_id, geometry);
        data.insert_temp(text_focus_id, editing_now);
    });
    if let Some(mut walk) = ctx.data(|data| data.get_temp::<FocusWalk>(walk_id)) {
        // L'ancre prend le focus à l'ouverture ; le pied borne la recherche.
        if just_opened {
            ctx.memory_mut(|memory| memory.request_focus(anchor_id));
        }
        walk.footer = clicks.ids.clone();
        ctx.data_mut(|data| data.insert_temp(walk_id, walk));
        // Les pas du focus s'enchaînent dans la même image : egui refait la
        // passe (sans la peindre) au lieu de montrer le focus qui saute de
        // widget en widget. Le plafond de passes est relevé en conséquence.
        ctx.options_mut(|options| {
            options.max_passes = std::num::NonZeroUsize::new(usize::from(FOCUS_WALK_STEPS) + 2)
                .unwrap_or(options.max_passes);
        });
        ctx.request_discard("initial focus");
        ctx.request_repaint();
    }

    // Échap : consommé par le sélecteur de couleur ou une liste ouverts (ils
    // le lisent avant), sinon il annule — sauf celui qui vient de sortir d'un
    // champ texte (S-F7). Entrée actionne le bouton par défaut, sauf pendant
    // une saisie de texte (où elle termine la saisie) : le principal, ou
    // Cancel d'une confirmation (S-R4). Un bouton du pied focalisé au clavier
    // reçoit Entrée lui-même : son clic passe devant.
    let escape = !popup_open
        && ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ESCAPE))
        && !editing_before;
    let enter = !popup_open
        && !keyboard_focus_outside_grid(ctx)
        && ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ENTER));
    let outcome = if header_actions.close || escape {
        FrameOutcome::Cancel
    } else if clicks.accept {
        FrameOutcome::Accept
    } else if clicks.secondary {
        FrameOutcome::Secondary
    } else if clicks.cancel || (enter && layout == FooterLayout::Confirm) {
        FrameOutcome::Cancel
    } else if enter && footer_spec.accept_enabled {
        FrameOutcome::Accept
    } else {
        FrameOutcome::Open
    };
    if outcome != FrameOutcome::Open {
        let saved = SavedGeometry {
            x: geometry.pos.x,
            y: geometry.pos.y,
            width: geometry.size.x,
            height: geometry.size.y,
        };
        ctx.data_mut(|data| {
            data.remove::<DialogGeometry>(geometry_id);
            data.remove::<f64>(flash_id);
            data.remove::<FocusWalk>(walk_id);
            data.remove::<bool>(text_focus_id);
            let remembered = data.get_temp_mut_or_default::<RememberedGeometries>(remembered_id());
            remembered.0.insert(kind.clone(), saved);
            data.get_temp_mut_or_default::<ClosedGeometries>(closed_id())
                .0
                .push((kind, saved));
        });
    }
    outcome
}

/// Les boutons du pied cliqués dans l'image, et leurs identifiants (le focus
/// initial s'arrête au pied).
#[derive(Default)]
struct FooterClicks {
    accept: bool,
    cancel: bool,
    /// Discard Changes (seul secondaire posé par la brique).
    secondary: bool,
    ids: Vec<egui::Id>,
}

/// Le pied d'un dialogue, dans `footer` (disposition de droite à gauche) :
/// boutons rangés selon `layout` (S-F2, S-F6) et zone de message à gauche des
/// boutons (S-M7, S-F10).
fn paint_footer(
    footer: &mut egui::Ui,
    layout: FooterLayout,
    spec: &DialogFooter<'_>,
    message: Option<FooterMessage<'_>>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
) -> FooterClicks {
    let mut clicks = FooterClicks::default();
    let default_fill = footer.visuals().selection.bg_fill;
    // Premier ajouté = le plus à droite : le bouton principal — par défaut,
    // sauf dans une confirmation, où il est destructeur et Cancel l'est (S-R4).
    let confirm = layout == FooterLayout::Confirm;
    let response = footer
        .scope(|ui| {
            let mut accept = egui::Button::new(spec.accept_label).min_size(FOOTER_BUTTON_SIZE);
            if confirm {
                theme::destructive_button_visuals(ui.visuals_mut());
            } else {
                accept = accept.fill(default_fill);
            }
            ui.add_enabled(spec.accept_enabled, accept)
                .on_hover_text(spec.accept_hint)
                .on_disabled_hover_text(spec.disabled_reason())
        })
        .inner;
    clicks.ids.push(response.id);
    clicks.accept = response.clicked();
    let cancel = |ui: &mut egui::Ui, clicks: &mut FooterClicks| {
        let mut button = egui::Button::new(tr!("dialog-cancel")).min_size(FOOTER_BUTTON_SIZE);
        if confirm {
            button = button.fill(default_fill);
        }
        let response = ui.add(button).on_hover_text(tr!("dialog-cancel", "hint"));
        clicks.ids.push(response.id);
        clicks.cancel = response.clicked();
    };
    match layout {
        FooterLayout::Dialog => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                extra_buttons(ui);
                paint_footer_message(ui, message);
            });
        }
        FooterLayout::Confirm => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                paint_footer_message(ui, message);
            });
        }
        FooterLayout::SaveChanges => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let response = ui
                    .scope(|ui| {
                        theme::destructive_button_visuals(ui.visuals_mut());
                        ui.add(
                            egui::Button::new(tr!("unsaved-discard")).min_size(FOOTER_BUTTON_SIZE),
                        )
                    })
                    .inner
                    .on_hover_text(tr!("unsaved-discard", "hint"));
                clicks.ids.push(response.id);
                clicks.secondary = response.clicked();
                paint_footer_message(ui, message);
            });
        }
        FooterLayout::Wizard => {
            extra_buttons(footer);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                cancel(ui, &mut clicks);
                paint_footer_message(ui, message);
            });
        }
        FooterLayout::ReadOnly => {
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                extra_buttons(ui);
                paint_footer_message(ui, message);
            });
        }
    }
    clicks
}

/// La zone de message du pied : l'erreur en couleur d'erreur, la raison d'un
/// grisage estompée ; coupée à la place qui reste (texte complet au survol).
fn paint_footer_message(ui: &mut egui::Ui, message: Option<FooterMessage<'_>>) {
    let text = match message {
        Some(FooterMessage::Error(error)) => {
            egui::RichText::new(error).color(ui.visuals().error_fg_color)
        }
        Some(FooterMessage::Reason(reason)) => egui::RichText::new(reason).weak(),
        None => return,
    };
    ui.add(egui::Label::new(text).truncate());
}

/// Un pas du focus initial (S-F9), au début d'une image : voir la doc du
/// module. Un geste l'interrompt avant que les widgets voient le focus ; un
/// focus posé en route sur autre chose qu'un champ texte est alors rendu.
fn advance_initial_focus(
    ctx: &egui::Context,
    walk_id: egui::Id,
    anchor_id: egui::Id,
    layer: egui::LayerId,
) {
    let Some(walk) = ctx.data(|data| data.get_temp::<FocusWalk>(walk_id)) else {
        return;
    };
    let focused = ctx.memory(|memory| memory.focused());
    let stop = |ctx: &egui::Context, release: Option<egui::Id>| {
        if let Some(focused) = release {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
        ctx.data_mut(|data| data.remove::<FocusWalk>(walk_id));
    };
    match focused {
        Some(focused) if text_field_in(ctx, focused, layer) => stop(ctx, None),
        _ if any_gesture(ctx) => stop(ctx, focused),
        Some(focused)
            if walk.steps < FOCUS_WALK_STEPS
                && (focused == anchor_id
                    || (!walk.footer.contains(&focused)
                        && ctx
                            .read_response(focused)
                            .is_some_and(|response| response.layer_id == layer))) =>
        {
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::Next));
            ctx.data_mut(|data| {
                data.insert_temp(
                    walk_id,
                    FocusWalk {
                        steps: walk.steps + 1,
                        ..walk
                    },
                );
            });
        }
        other => stop(ctx, other),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{image_de, textes, touche, Texte};
    use super::*;

    /// Le centre du texte peint `cherche`.
    fn centre(peints: &[Texte], cherche: &str) -> egui::Pos2 {
        peints
            .iter()
            .find(|t| t.texte == cherche)
            .unwrap_or_else(|| panic!("{cherche:?} non peint"))
            .centre
    }

    /// [`modal_frame`] rendu en [`DialogOutcome`].
    #[expect(clippy::too_many_arguments)]
    fn dialog_frame(
        ctx: &egui::Context,
        id: egui::Id,
        title: &str,
        default_size: egui::Vec2,
        layout: FooterLayout,
        footer_spec: DialogFooter<'_>,
        extra_buttons: impl FnOnce(&mut egui::Ui),
        add_contents: impl FnOnce(&mut egui::Ui),
    ) -> DialogOutcome {
        modal_frame(
            ctx,
            id,
            title,
            default_size,
            layout,
            footer_spec,
            extra_buttons,
            add_contents,
        )
        .dialog()
    }

    /// Un dialogue de `layout` dont le bouton principal est `principal` (grisé
    /// avec la raison `raison` si `actif` est faux), un bouton supplémentaire
    /// « Autre » ; rend l'issue de l'image et ses textes.
    fn pied(
        context: &egui::Context,
        layout: FooterLayout,
        actif: bool,
        events: Vec<egui::Event>,
        mut contenu: impl FnMut(&mut egui::Ui),
    ) -> (DialogOutcome, Vec<Texte>) {
        let mut issue = DialogOutcome::Open;
        let sortie = image_de(context, egui::vec2(800.0, 600.0), events, |ui| {
            issue = dialog_frame(
                ui.ctx(),
                egui::Id::new("dialogue_pied"),
                "Titre",
                egui::vec2(600.0, 300.0),
                layout,
                DialogFooter {
                    accept_label: "Principal",
                    accept_hint: "Bulle",
                    accept_enabled: actif,
                    accept_disabled: Some("Raison"),
                },
                |ui| {
                    let _ = ui.button("Autre");
                },
                |ui| {
                    contenu(ui);
                },
            );
        });
        (issue, textes(&sortie.shapes))
    }

    /// S-F6 : un dialogue range le secondaire à gauche, puis Cancel, puis le
    /// principal à droite ; un assistant met Cancel tout à gauche et groupe ses
    /// boutons d'étape contre le principal. Lu dans les formes peintes.
    #[test]
    fn l_ordre_des_boutons_suit_la_sorte_de_dialogue() {
        let annuler = tr!("dialog-cancel");
        let context = egui::Context::default();
        let mut peints = Vec::new();
        for _ in 0..3 {
            peints = pied(&context, FooterLayout::Dialog, true, Vec::new(), |_| {}).1;
        }
        let (autre, cancel, principal) = (
            centre(&peints, "Autre"),
            centre(&peints, &annuler),
            centre(&peints, "Principal"),
        );
        assert!(autre.x < cancel.x && cancel.x < principal.x, "dialogue");
        assert!((autre.y - principal.y).abs() < 1.0, "une seule rangée");

        let context = egui::Context::default();
        for _ in 0..3 {
            peints = pied(&context, FooterLayout::Wizard, true, Vec::new(), |_| {}).1;
        }
        let (autre, cancel, principal) = (
            centre(&peints, "Autre"),
            centre(&peints, &annuler),
            centre(&peints, "Principal"),
        );
        assert!(cancel.x < autre.x && autre.x < principal.x, "assistant");
        assert!(
            principal.x - autre.x < autre.x - cancel.x,
            "les boutons d'étape sont groupés contre le principal"
        );
    }

    /// S-F2 : une fenêtre en lecture seule n'a qu'un bouton, Close, qu'Entrée
    /// actionne ; pas de Cancel.
    #[test]
    fn une_fenetre_en_lecture_seule_n_a_que_close() {
        let context = egui::Context::default();
        let id = egui::Id::new("lecture_seule");
        let mut issue = DialogOutcome::Open;
        let mut peints = Vec::new();
        for events in [vec![], vec![], vec![], vec![touche(egui::Key::Enter)]] {
            let sortie = image_de(&context, egui::vec2(800.0, 600.0), events, |ui| {
                issue = modal_read_only(ui.ctx(), id, "Titre", egui::vec2(300.0, 200.0), |ui| {
                    ui.label("contenu");
                });
            });
            if issue == DialogOutcome::Open {
                peints = textes(&sortie.shapes);
            }
        }
        assert!(peints.iter().any(|t| t.texte == tr!("dialog-close")));
        assert!(!peints.iter().any(|t| t.texte == tr!("dialog-cancel")));
        assert_eq!(issue, DialogOutcome::Accept, "Entrée ferme");
    }

    /// S-M7, S-F10 (décision pure) : l'erreur passe devant la raison ; la raison
    /// n'est dite que bouton grisé ; sans l'une ni l'autre, rien.
    #[test]
    fn l_erreur_passe_devant_la_raison_du_grisage() {
        assert_eq!(
            footer_message(Some("Erreur"), false, "Raison"),
            Some(FooterMessage::Error("Erreur"))
        );
        assert_eq!(
            footer_message(None, false, "Raison"),
            Some(FooterMessage::Reason("Raison"))
        );
        assert_eq!(footer_message(None, true, "Raison"), None);
        assert_eq!(footer_message(Some(""), false, ""), None);
    }

    /// S-F10, S-M7 peints : bouton principal grisé, sa raison est dans le pied ;
    /// une erreur publiée la remplace, et la raison revient quand l'erreur
    /// disparaît. Témoin : bouton actif, la raison n'est pas peinte.
    #[test]
    fn le_pied_montre_l_erreur_puis_la_raison_du_grisage() {
        let context = egui::Context::default();
        let mut peints = Vec::new();
        for _ in 0..3 {
            peints = pied(&context, FooterLayout::Dialog, false, Vec::new(), |_| {}).1;
        }
        assert!(peints.iter().any(|t| t.texte == "Raison"));
        context.data_mut(|data| {
            data.insert_temp(dialog_error_id(), Some("Échec du calcul".to_string()));
        });
        peints = pied(&context, FooterLayout::Dialog, false, Vec::new(), |_| {}).1;
        assert!(peints.iter().any(|t| t.texte == "Échec du calcul"));
        assert!(!peints.iter().any(|t| t.texte == "Raison"));
        context.data_mut(|data| data.insert_temp(dialog_error_id(), None::<String>));
        peints = pied(&context, FooterLayout::Dialog, false, Vec::new(), |_| {}).1;
        assert!(
            peints.iter().any(|t| t.texte == "Raison"),
            "la raison revient"
        );

        let actif = egui::Context::default();
        for _ in 0..3 {
            peints = pied(&actif, FooterLayout::Dialog, true, Vec::new(), |_| {}).1;
        }
        assert!(!peints.iter().any(|t| t.texte == "Raison"), "témoin");
    }

    /// Un dialogue dont le contenu est une case puis un champ texte.
    fn dialogue_a_champ(
        context: &egui::Context,
        texte: &mut String,
        events: Vec<egui::Event>,
    ) -> (DialogOutcome, Option<egui::Id>) {
        let mut champ = None;
        let mut case = false;
        let issue = pied(context, FooterLayout::Dialog, true, events, |ui| {
            ui.checkbox(&mut case, "case");
            champ = Some(ui.text_edit_singleline(texte).id);
        })
        .0;
        (issue, champ)
    }

    /// S-F9 : à l'ouverture, le premier champ de saisie prend le focus, même
    /// derrière une case ; S-F7 : le premier Échap sort du champ sans fermer, le
    /// second fait Cancel.
    #[test]
    fn le_premier_champ_a_le_focus_et_le_premier_echap_n_en_fait_que_sortir() {
        let context = egui::Context::default();
        let mut texte = String::from("abc");
        let mut champ = None;
        for _ in 0..8 {
            champ = dialogue_a_champ(&context, &mut texte, Vec::new()).1;
        }
        let champ = champ.expect("champ peint");
        assert_eq!(context.memory(|memory| memory.focused()), Some(champ));
        let (issue, _) = dialogue_a_champ(&context, &mut texte, vec![touche(egui::Key::Escape)]);
        assert_eq!(issue, DialogOutcome::Open, "le premier Échap sort du champ");
        assert_ne!(context.memory(|memory| memory.focused()), Some(champ));
        let (issue, _) = dialogue_a_champ(&context, &mut texte, vec![touche(egui::Key::Escape)]);
        assert_eq!(issue, DialogOutcome::Cancel, "le second annule");
    }

    /// S-F9 : les pas du focus s'enchaînent dans l'image (passes jetées), pas
    /// un widget par image — ici deux images suffisent derrière une case.
    #[test]
    fn le_focus_initial_ne_prend_pas_une_image_par_widget() {
        let context = egui::Context::default();
        let mut texte = String::from("abc");
        let mut champ = None;
        for _ in 0..2 {
            champ = dialogue_a_champ(&context, &mut texte, Vec::new()).1;
        }
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(champ.expect("champ peint"))
        );
    }

    /// S-F9, sans champ de saisie : le focus initial ne laisse rien de
    /// focalisé (Entrée valide toujours). Un geste pendant la recherche
    /// l'interrompt : la touche agit sans focus.
    #[test]
    fn sans_champ_le_focus_initial_ne_laisse_rien_de_focalise() {
        let context = egui::Context::default();
        for _ in 0..8 {
            pied(&context, FooterLayout::Dialog, true, Vec::new(), |ui| {
                ui.label("contenu");
            });
        }
        assert_eq!(context.memory(|memory| memory.focused()), None);

        let context = egui::Context::default();
        pied(&context, FooterLayout::Dialog, true, Vec::new(), |_| {});
        let (issue, _) = pied(
            &context,
            FooterLayout::Dialog,
            true,
            vec![touche(egui::Key::Enter)],
            |_| {},
        );
        assert_eq!(
            issue,
            DialogOutcome::Accept,
            "Entrée agit pendant la recherche"
        );
    }

    /// S-F8 : fermé, un dialogue rend sa géométrie (pour les préférences) et la
    /// reprend à sa réouverture, même avec une autre taille par défaut.
    #[test]
    fn un_dialogue_rouvert_reprend_sa_taille_et_sa_place() {
        let context = egui::Context::default();
        let id = egui::Id::new("dialogue_memorise");
        let mut taille = egui::vec2(300.0, 200.0);
        let ouvrir = |events: Vec<egui::Event>, taille: egui::Vec2| {
            let mut issue = DialogOutcome::Open;
            image_de(&context, egui::vec2(800.0, 600.0), events, |ui| {
                issue = modal_dialog(ui.ctx(), id, "Titre", taille, |ui| {
                    ui.label("contenu");
                });
            });
            issue
        };
        ouvrir(Vec::new(), taille);
        assert_eq!(
            ouvrir(vec![touche(egui::Key::Escape)], taille),
            DialogOutcome::Cancel
        );
        let fermees = context
            .data(|data| data.get_temp::<ClosedGeometries>(closed_id()))
            .unwrap_or_default()
            .0;
        assert_eq!(fermees.len(), 1);
        let (sorte, rendue) = &fermees[0];
        assert_eq!(sorte, &dialog_kind(id));
        assert_eq!((rendue.width, rendue.height), (300.0, 200.0));

        // Une géométrie mémorisée (venue des préférences) l'emporte sur la
        // taille par défaut.
        let memorisee = SavedGeometry {
            x: 40.0,
            y: 50.0,
            width: 420.0,
            height: 260.0,
        };
        context.data_mut(|data| {
            data.insert_temp(
                remembered_id(),
                RememberedGeometries(BTreeMap::from([(dialog_kind(id), memorisee)])),
            );
        });
        taille = egui::vec2(500.0, 500.0);
        ouvrir(Vec::new(), taille);
        let ouverte = context
            .data(|data| data.get_temp::<DialogGeometry>(id.with("geometry")))
            .expect("ouvert");
        assert_eq!(ouverte.pos, egui::pos2(40.0, 50.0));
        assert_eq!(ouverte.size, egui::vec2(420.0, 260.0));
    }

    /// Une confirmation (#229) : une image avec `events` ; rend l'issue et
    /// les formes peintes.
    fn confirmation(
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (ConfirmOutcome, egui::FullOutput) {
        let mut issue = ConfirmOutcome::Open;
        let sortie = image_de(context, egui::vec2(800.0, 600.0), events, |ui| {
            issue = modal_confirm(
                ui.ctx(),
                egui::Id::new("confirmation"),
                "Supprimer le modèle",
                egui::vec2(400.0, 160.0),
                "Supprimer",
                "Irréversible",
                |ui| {
                    ui.label("Supprimer « A » ?");
                },
            );
        });
        (issue, sortie)
    }

    /// Le trio « Discard Changes · Cancel · Save » : une image avec `events`.
    fn trio(
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (SaveChangesOutcome, egui::FullOutput) {
        let mut issue = SaveChangesOutcome::Open;
        let sortie = image_de(context, egui::vec2(800.0, 600.0), events, |ui| {
            issue = modal_save_changes(
                ui.ctx(),
                egui::Id::new("trio"),
                "Modifications",
                egui::vec2(400.0, 160.0),
                |ui| {
                    ui.label("Enregistrer ?");
                },
            );
        });
        (issue, sortie)
    }

    /// Clique `cherche` (un texte peint par `peindre`) en trois images ; rend
    /// la première issue autre que `ouvert`.
    fn cliquer_texte<T: PartialEq + Copy>(
        context: &egui::Context,
        cherche: &str,
        ouvert: T,
        peindre: impl Fn(&egui::Context, Vec<egui::Event>) -> (T, egui::FullOutput),
    ) -> T {
        let (_, sortie) = peindre(context, Vec::new());
        let pos = centre(&textes(&sortie.shapes), cherche);
        let bouton = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        [
            vec![egui::Event::PointerMoved(pos)],
            vec![bouton(true)],
            vec![bouton(false)],
        ]
        .into_iter()
        .map(|events| peindre(context, events).0)
        .find(|issue| *issue != ouvert)
        .unwrap_or(ouvert)
    }

    /// S-R4 (#229) : dans une confirmation, Entrée **et** Échap font Cancel —
    /// le bouton par défaut ; le verbe ne s'obtient que par son bouton
    /// (témoin : le clic confirme).
    #[test]
    fn une_confirmation_s_annule_par_entree_comme_par_echap() {
        for key in [egui::Key::Enter, egui::Key::Escape] {
            let context = egui::Context::default();
            for _ in 0..3 {
                confirmation(&context, Vec::new());
            }
            assert_eq!(
                confirmation(&context, vec![touche(key)]).0,
                ConfirmOutcome::Cancel,
                "{key:?}"
            );
        }
        let context = egui::Context::default();
        for _ in 0..3 {
            confirmation(&context, Vec::new());
        }
        assert_eq!(
            cliquer_texte(&context, "Supprimer", ConfirmOutcome::Open, confirmation),
            ConfirmOutcome::Confirm
        );
    }

    /// S-F4, exception écrite à S-R4 : dans « Discard Changes · Cancel · Save »,
    /// Entrée = Save, Échap = Cancel ; Discard Changes par son bouton, à
    /// gauche de Cancel et Save.
    #[test]
    fn le_trio_enregistre_par_entree_et_s_annule_par_echap() {
        for (key, attendu) in [
            (egui::Key::Enter, SaveChangesOutcome::Save),
            (egui::Key::Escape, SaveChangesOutcome::Cancel),
        ] {
            let context = egui::Context::default();
            for _ in 0..3 {
                trio(&context, Vec::new());
            }
            assert_eq!(trio(&context, vec![touche(key)]).0, attendu, "{key:?}");
        }
        let context = egui::Context::default();
        let mut sortie = None;
        for _ in 0..3 {
            sortie = Some(trio(&context, Vec::new()).1);
        }
        let peints = textes(&sortie.expect("peint").shapes);
        let (abandon, annuler, enregistrer) = (
            centre(&peints, &tr!("unsaved-discard")),
            centre(&peints, &tr!("dialog-cancel")),
            centre(&peints, &tr!("unsaved-save")),
        );
        assert!(abandon.x < annuler.x && annuler.x < enregistrer.x);
        assert_eq!(
            cliquer_texte(
                &context,
                &tr!("unsaved-discard"),
                SaveChangesOutcome::Open,
                trio
            ),
            SaveChangesOutcome::Discard
        );
    }

    /// Un rectangle plein de `couleur` a-t-il été peint ?
    fn rectangle_de(sortie: &egui::FullOutput, couleur: egui::Color32) -> bool {
        sortie
            .shapes
            .iter()
            .any(|peint| matches!(&peint.shape, egui::Shape::Rect(rect) if rect.fill == couleur))
    }

    /// S-R5 (#229) : le verbe destructeur a le style normal au repos, et rougit
    /// au survol **et** au focus clavier (Tab), pointeur ailleurs.
    #[test]
    fn le_verbe_destructeur_rougit_au_survol_et_au_focus() {
        let context = egui::Context::default();
        let rouge = context.global_style().visuals.error_fg_color;
        let mut sortie = None;
        for _ in 0..3 {
            sortie = Some(confirmation(&context, Vec::new()).1);
        }
        let sortie = sortie.expect("peint");
        assert!(!rectangle_de(&sortie, rouge), "au repos : style normal");
        let verbe = centre(&textes(&sortie.shapes), "Supprimer");
        let survol = confirmation(&context, vec![egui::Event::PointerMoved(verbe)]).1;
        assert!(rectangle_de(&survol, rouge), "survol");
        let ailleurs = egui::pos2(5.0, 5.0);
        let repos = confirmation(&context, vec![egui::Event::PointerMoved(ailleurs)]).1;
        assert!(!rectangle_de(&repos, rouge), "témoin : pointeur parti");
        let focus = (0..6).any(|_| {
            let sortie = confirmation(&context, vec![touche(egui::Key::Tab)]).1;
            rectangle_de(&confirmation(&context, Vec::new()).1, rouge)
                || rectangle_de(&sortie, rouge)
        });
        assert!(focus, "focus clavier");
    }

    /// S-M8 : l'échec grave se ferme par Entrée (Close) ; son action est un
    /// bouton à part.
    #[test]
    fn un_echec_grave_se_ferme_par_entree_et_propose_son_action() {
        let alerte = |context: &egui::Context, events: Vec<egui::Event>| {
            let mut issue = AlertOutcome::Open;
            let sortie = image_de(context, egui::vec2(800.0, 600.0), events, |ui| {
                issue = modal_alert(
                    ui.ctx(),
                    egui::Id::new("alerte"),
                    "Échec",
                    egui::vec2(400.0, 160.0),
                    ("Réessayer", "Aide"),
                    |ui| {
                        ui.label("Raté");
                    },
                );
            });
            (issue, sortie)
        };
        let context = egui::Context::default();
        for _ in 0..3 {
            alerte(&context, Vec::new());
        }
        assert_eq!(
            alerte(&context, vec![touche(egui::Key::Enter)]).0,
            AlertOutcome::Close
        );
        let context = egui::Context::default();
        for _ in 0..3 {
            alerte(&context, Vec::new());
        }
        assert_eq!(
            cliquer_texte(&context, "Réessayer", AlertOutcome::Open, alerte),
            AlertOutcome::Action
        );
    }

    /// Une image de 800 × 600 avec `events`, où `ui` dessine l'arrière-plan.
    fn image(context: &egui::Context, events: Vec<egui::Event>, ui: impl FnMut(&mut egui::Ui)) {
        image_de(context, egui::vec2(800.0, 600.0), events, ui);
    }

    /// #14 : tant qu'un dialogue modal est ouvert, un clic sur le reste de
    /// l'interface n'a aucun effet et déclenche le clignotement du contour ;
    /// Échap le ferme ; sans dialogue, le même clic agit (témoin). Un clic
    /// **dans** le dialogue, lui, ne clignote pas.
    #[test]
    fn un_dialogue_modal_bloque_l_arriere_plan_et_clignote() {
        let id = egui::Id::new("dialogue_test");
        let clicks = std::cell::Cell::new(0);
        let closed = std::cell::Cell::new(DialogOutcome::Open);
        let run = |context: &egui::Context, modal: bool, events: Vec<egui::Event>| {
            image(context, events, |ui| {
                if ui.button("fond").clicked() {
                    clicks.set(clicks.get() + 1);
                }
                if modal {
                    let close =
                        modal_dialog(ui.ctx(), id, "Test", egui::vec2(300.0, 200.0), |ui| {
                            ui.label("contenu");
                        });
                    if close != DialogOutcome::Open {
                        closed.set(close);
                    }
                }
            });
        };
        let click = |context: &egui::Context, modal: bool, at: egui::Pos2| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            run(context, modal, vec![egui::Event::PointerMoved(at)]);
            run(context, modal, vec![button(true)]);
            run(context, modal, vec![button(false)]);
        };
        let fond = egui::pos2(20.0, 14.0);
        let flash = |context: &egui::Context| {
            context.data(|data| data.get_temp::<f64>(id.with("flash_start")))
        };

        let free = egui::Context::default();
        run(&free, false, vec![]);
        click(&free, false, fond);
        assert_eq!(clicks.get(), 1, "sans dialogue, le bouton répond");

        let context = egui::Context::default();
        for _ in 0..3 {
            run(&context, true, vec![]);
        }
        // Le dialogue est centré à l'écran : son milieu est dans son corps.
        click(&context, true, egui::pos2(400.0, 300.0));
        assert!(
            flash(&context).is_none(),
            "un clic dans le dialogue n'est pas un clic bloqué"
        );
        assert_eq!(closed.get(), DialogOutcome::Open);
        click(&context, true, fond);
        assert_eq!(clicks.get(), 1, "le dialogue a bloqué le clic");
        assert!(
            flash(&context).is_some(),
            "le clic bloqué déclenche le clignotement"
        );
        assert_eq!(closed.get(), DialogOutcome::Open);
        run(&context, true, vec![touche(egui::Key::Escape)]);
        assert_eq!(closed.get(), DialogOutcome::Cancel, "Échap annule");
        assert!(
            flash(&context).is_none(),
            "fermé, le dialogue oublie son clignotement"
        );

        // Valider est le bouton par défaut : Entrée (hors saisie de texte).
        closed.set(DialogOutcome::Open);
        let again = egui::Context::default();
        for _ in 0..3 {
            run(&again, true, vec![]);
        }
        run(&again, true, vec![touche(egui::Key::Enter)]);
        assert_eq!(closed.get(), DialogOutcome::Accept, "Entrée valide");
    }

    /// Le bouton par défaut grisé (assistant à une étape incomplète) : Entrée
    /// ne valide plus, Échap annule toujours.
    #[test]
    fn entree_ne_valide_pas_un_dialogue_dont_le_bouton_par_defaut_est_grise() {
        let id = egui::Id::new("dialogue_grise");
        for (accept_enabled, attendu) in
            [(false, DialogOutcome::Open), (true, DialogOutcome::Accept)]
        {
            let context = egui::Context::default();
            let mut issue = DialogOutcome::Open;
            let mut run = |events: Vec<egui::Event>| {
                image(&context, events, |ui| {
                    issue = modal_dialog_with(
                        ui.ctx(),
                        id,
                        "Test",
                        egui::vec2(300.0, 200.0),
                        DialogFooter {
                            accept_label: "Suivant",
                            accept_hint: "",
                            accept_enabled,
                            accept_disabled: Some("raison"),
                        },
                        |_| {},
                        |ui| {
                            ui.label("contenu");
                        },
                    );
                });
                issue
            };
            for _ in 0..3 {
                run(Vec::new());
            }
            assert_eq!(
                run(vec![touche(egui::Key::Enter)]),
                attendu,
                "accept_enabled={accept_enabled}"
            );
            if !accept_enabled {
                assert_eq!(run(vec![touche(egui::Key::Escape)]), DialogOutcome::Cancel);
            }
        }
    }
}
