//! Dialogues **modaux** : [`modal_dialog`], [`modal_dialog_with`],
//! [`modal_wizard`], [`modal_live_settings`], [`modal_read_only`], [`modal_confirm`],
//! [`modal_save_changes`] et [`modal_alert`], la brique de toute boîte de
//! dialogue et de tout assistant — même cadre, même barre de titre, même
//! blocage du reste de l'interface. Elle pose la fenêtre par la brique des
//! fenêtres de contenu (`window_chrome::show_window_in_layer`).
//!
//! Sorti d'`app.rs` tel quel (#117), puis étendu par #228
//! (`docs/standard-interface.md`, § Fenêtres et dialogues) :
//!
//! - **nature** (#300, S-F11) : [`DialogNature`] dit ce qu'est la fenêtre —
//!   réglages, brouillon, choix, assistant, lecture seule, confirmation,
//!   enregistrement, alerte — et en tire, par des `match` exhaustifs sans `_`,
//!   le pied, la taille par défaut (une [`SizeClass`] : Petite 480 x 170,
//!   Moyenne 480 x 420, Grande 640 x 500), le focus initial et le bouton par
//!   défaut. La taille est un champ de la variante : une fenêtre interactive
//!   ([`InteractiveSize`]) prend une classe ou une taille libre justifiée par
//!   `// TAILLE-LIBRE(#300):` ; une fenêtre sans interaction riche
//!   ([`PlainSize`]) peut prendre la taille de son contenu sans justification ;
//! - **ordre des boutons** (S-F6) : un dialogue range ses boutons secondaires à
//!   gauche, puis Cancel, puis le principal à droite ; un assistant met Cancel
//!   à gauche et groupe Back · Next · Finish à droite ; une fenêtre en lecture
//!   seule n'a qu'un bouton Close (S-F2) ;
//! - **zone de message** du pied, à gauche des boutons (S-M7) : l'erreur d'un
//!   geste du dialogue (jamais dans la barre d'état ; `AppState::dialog_error`,
//!   publiée par [`sync_dialogs`]), sinon la **raison** du bouton principal
//!   grisé (S-F10 → S-R14) — qui est aussi sa bulle. La raison vient de l'appelant :
//!   `AcceptButton::Gated` (#226, `DialogFooter::gated`) quand le bouton est
//!   grisé ; un bouton qui ne l'est jamais est `AcceptButton::Always` ;
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
//! - **focus initial** (S-F9, S-F11) : selon la nature ([`InitialFocus`]), le
//!   premier champ de saisie, le bouton par défaut (confirmation, enregistrement,
//!   alerte) ou rien (lecture seule). Pour le premier champ de saisie :
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

/// Les trois **classes de taille** d'un dialogue (S-F11) : la taille par défaut
/// ne s'invente pas, elle se range dans l'une d'elles. La taille que
/// l'utilisateur a laissée (S-F8) l'emporte ensuite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SizeClass {
    /// 480 x 170 : une question, un message, quelques boutons.
    Small,
    /// 480 x 420 : un formulaire ou des réglages à quelques pages.
    Medium,
    /// 640 x 500 : une liste et son éditeur côte à côte.
    Large,
}

impl SizeClass {
    pub(super) const fn size(self) -> egui::Vec2 {
        match self {
            Self::Small => egui::vec2(480.0, 170.0),
            Self::Medium => egui::vec2(480.0, 420.0),
            Self::Large => egui::vec2(640.0, 500.0),
        }
    }
}

/// La taille d'une fenêtre **interactive** (réglages, brouillon, assistant) :
/// la classe par défaut de sa nature, une autre classe, ou une taille libre
/// que la garde `aucune_taille_libre_sans_justification` ne laisse passer qu'avec
/// un `// TAILLE-LIBRE(#300): <justification>` juste avant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum InteractiveSize {
    /// La classe par défaut de la nature ([`DialogNature::default_class`]).
    Default,
    Class(SizeClass),
    /// Hors classes, justifiée dans le code (S-F11).
    Free(egui::Vec2),
}

impl InteractiveSize {
    /// La taille, `default` étant la classe de la nature.
    fn resolve(self, default: SizeClass) -> egui::Vec2 {
        match self {
            Self::Default => default.size(),
            Self::Class(class) => class.size(),
            Self::Free(size) => size,
        }
    }
}

/// La taille d'une fenêtre **sans interaction riche** (choix, lecture seule,
/// confirmation, enregistrement, alerte) : pas de formulaire à ranger, une
/// taille mal choisie y coûte peu. Elle peut donc prendre la taille de son
/// contenu **sans justification** (S-F11) — ce que la fenêtre interactive ne
/// peut pas : le type ne l'offre pas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PlainSize {
    /// La classe par défaut de la nature ([`DialogNature::default_class`]).
    Default,
    Class(SizeClass),
    /// La taille que demande le contenu.
    Content(egui::Vec2),
}

impl PlainSize {
    /// La taille, `default` étant la classe de la nature.
    fn resolve(self, default: SizeClass) -> egui::Vec2 {
        match self {
            Self::Default => default.size(),
            Self::Class(class) => class.size(),
            Self::Content(size) => size,
        }
    }
}

/// La **nature** d'une fenêtre modale (S-F11) : ce qu'elle est fixe son pied
/// (ordre des boutons, S-F2, S-F4, S-F6), sa taille par défaut, son focus
/// initial (S-F9) et le bouton par défaut. Chaque `match` sur la nature est
/// exhaustif, sans `_` : une nature nouvelle ne compile pas avant d'avoir dit
/// tout cela.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DialogNature {
    /// Réglages en direct d'un objet visible : `[Cancel] [OK]`.
    Settings(InteractiveSize),
    /// Création sur brouillon : `[secondaires] [message] [Cancel] [verbe]`.
    Draft(InteractiveSize),
    /// Choix sans réponse « oui » : même pied, secondaires = autres réponses.
    Choice(PlainSize),
    /// Assistant : `[Cancel] [message] [Back · Next] [Finish]`.
    Wizard(InteractiveSize),
    /// Réglages **appliqués en direct**, sans rien à valider ni annuler :
    /// `[message] [Close]` (S-F2). Interactive (des champs à remplir : focus
    /// sur le premier, taille par classe), mais au pied d'une lecture seule.
    LiveSettings(InteractiveSize),
    /// Lecture seule : `[secondaires] [message] [Close]`.
    ReadOnly(PlainSize),
    /// Confirmation : `[message] [Cancel] [verbe]`, Cancel par défaut.
    Confirm(PlainSize),
    /// Enregistrer ? : `[Discard Changes] [message] [Cancel] [Save]`.
    SaveChanges(PlainSize),
    /// Échec grave : `[action] [message] [Close]`.
    Alert(PlainSize),
}

/// Où le focus va à l'ouverture (S-F9, S-F11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitialFocus {
    /// Le premier champ de saisie, trouvé par la brique.
    FirstField,
    /// Le bouton par défaut du pied : Cancel d'une confirmation, Save, Close.
    DefaultButton,
    /// Rien : la fenêtre se lit, Entrée ferme.
    Nothing,
}

/// La disposition des boutons du pied d'une nature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FooterShape {
    /// `[secondaires] [message] [Cancel] [principal]`.
    Dialog,
    /// `[Cancel] [message] [Back · Next] [Finish]`.
    Wizard,
    /// `[secondaires] [message] [Close]`.
    ReadOnly,
    /// `[message] [Cancel] [verbe]`.
    Confirm,
    /// `[Discard Changes] [message] [Cancel] [Save]`.
    SaveChanges,
}

impl DialogNature {
    /// La disposition du pied (S-F2, S-F4, S-F6).
    pub(super) fn footer(self) -> FooterShape {
        match self {
            Self::Settings(_) | Self::Draft(_) | Self::Choice(_) => FooterShape::Dialog,
            Self::Wizard(_) => FooterShape::Wizard,
            Self::LiveSettings(_) | Self::ReadOnly(_) | Self::Alert(_) => FooterShape::ReadOnly,
            Self::Confirm(_) => FooterShape::Confirm,
            Self::SaveChanges(_) => FooterShape::SaveChanges,
        }
    }

    /// La classe de taille quand l'appelant n'en dit pas.
    pub(super) fn default_class(self) -> SizeClass {
        match self {
            Self::Settings(_) | Self::Draft(_) | Self::LiveSettings(_) | Self::ReadOnly(_) => {
                SizeClass::Medium
            }
            Self::Wizard(_) => SizeClass::Large,
            Self::Choice(_) | Self::Confirm(_) | Self::SaveChanges(_) | Self::Alert(_) => {
                SizeClass::Small
            }
        }
    }

    /// La taille par défaut de la fenêtre.
    pub(super) fn size(self) -> egui::Vec2 {
        let default = self.default_class();
        match self {
            Self::Settings(size)
            | Self::Draft(size)
            | Self::LiveSettings(size)
            | Self::Wizard(size) => size.resolve(default),
            Self::Choice(size)
            | Self::ReadOnly(size)
            | Self::Confirm(size)
            | Self::SaveChanges(size)
            | Self::Alert(size) => size.resolve(default),
        }
    }

    /// Le focus initial (S-F9) : un brouillon, des réglages, un assistant ou un
    /// choix ont des champs à remplir ; une confirmation, un enregistrement ou
    /// une alerte attendent un bouton ; une lecture seule n'a rien à saisir.
    pub(super) fn initial_focus(self) -> InitialFocus {
        match self {
            Self::Settings(_)
            | Self::Draft(_)
            | Self::LiveSettings(_)
            | Self::Wizard(_)
            | Self::Choice(_) => InitialFocus::FirstField,
            Self::Confirm(_) | Self::SaveChanges(_) | Self::Alert(_) => InitialFocus::DefaultButton,
            Self::ReadOnly(_) => InitialFocus::Nothing,
        }
    }

    /// Cancel est-il le bouton par défaut (S-F5) ? Sinon c'est le principal.
    fn cancel_is_default(self) -> bool {
        match self {
            Self::Confirm(_) => true,
            Self::Settings(_)
            | Self::Draft(_)
            | Self::Choice(_)
            | Self::Wizard(_)
            | Self::LiveSettings(_)
            | Self::ReadOnly(_)
            | Self::SaveChanges(_)
            | Self::Alert(_) => false,
        }
    }
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
    size: InteractiveSize,
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
        DialogNature::Settings(size),
        DialogFooter::always(&accept_label, &accept_hint),
        |_| {},
        add_contents,
    )
}

/// Le bouton principal d'un pied : ou bien **toujours actif** (il n'a qu'une
/// bulle), ou bien **peut être grisé** — et alors la bulle du bouton actif et
/// la raison du grisage sont toutes deux obligatoires (S-F10, S-G1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptButton<'a> {
    /// Jamais grisé.
    Always { hint: &'a str },
    /// Grisé quand `enabled` est faux. `reason` dit pourquoi et quoi faire ; elle
    /// s'écrit dans la zone de message du pied **et** en bulle du bouton grisé.
    Gated {
        enabled: bool,
        hint: &'a str,
        reason: &'a str,
    },
}

/// Le pied de page d'un [`modal_dialog_with`] : libellé et état du bouton par
/// défaut. [`modal_dialog`] y met « OK », toujours actif — ce que les propriétés
/// du graphe emploient.
pub(super) struct DialogFooter<'a> {
    pub(super) accept_label: &'a str,
    pub(super) accept: AcceptButton<'a>,
}

impl<'a> DialogFooter<'a> {
    /// Un bouton principal qui n'est jamais grisé.
    pub(super) fn always(label: &'a str, hint: &'a str) -> Self {
        Self {
            accept_label: label,
            accept: AcceptButton::Always { hint },
        }
    }

    /// Un bouton principal qui peut être grisé, avec sa raison (#226).
    pub(super) fn gated(label: &'a str, hint: &'a str, enabled: bool, reason: &'a str) -> Self {
        debug_assert!(
            enabled || !reason.is_empty(),
            "un bouton grisé sans raison (S-G1)"
        );
        Self {
            accept_label: label,
            accept: AcceptButton::Gated {
                enabled,
                hint,
                reason,
            },
        }
    }

    fn enabled(&self) -> bool {
        match self.accept {
            AcceptButton::Always { .. } => true,
            AcceptButton::Gated { enabled, .. } => enabled,
        }
    }

    /// La bulle du bouton actif.
    fn hint(&self) -> &str {
        match self.accept {
            AcceptButton::Always { hint } | AcceptButton::Gated { hint, .. } => hint,
        }
    }

    /// La raison du grisage, qui est aussi la bulle du bouton grisé ; un bouton
    /// qui ne l'est jamais n'en a pas.
    fn disabled_reason(&self) -> &str {
        match self.accept {
            AcceptButton::Always { .. } => "",
            AcceptButton::Gated { reason, .. } => reason,
        }
    }
}

/// [`modal_dialog`] avec un bouton par défaut personnalisé (`footer`) et des
/// boutons **secondaires** (`extra_buttons`), posés à gauche du pied, de
/// gauche à droite, avant la zone de message ; Cancel puis le bouton principal
/// restent à droite (S-F6). `nature` est celle d'un pied de dialogue :
/// réglages, brouillon ou choix.
pub(super) fn modal_dialog_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    nature: DialogNature,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        nature,
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
    size: InteractiveSize,
    footer_spec: DialogFooter<'_>,
    step_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::Wizard(size),
        footer_spec,
        step_buttons,
        add_contents,
    )
    .dialog()
}

/// Une fenêtre de **réglages appliqués en direct** (S-F2, S-F11) : rien à
/// valider ni à annuler, donc un seul bouton, Close, comme la lecture seule ;
/// mais elle a des champs, donc le focus initial va au premier (S-F9) et la
/// taille se range dans une classe ([`InteractiveSize`]). Mêmes issues que
/// [`modal_read_only`].
pub(super) fn modal_live_settings(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: InteractiveSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::LiveSettings(size),
        DialogFooter::always(&label, &hint),
        |_| {},
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
    size: PlainSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::ReadOnly(size),
        DialogFooter::always(&label, &hint),
        |_| {},
        add_contents,
    )
    .dialog()
}

/// [`modal_read_only`] avec des boutons secondaires à gauche de Close
/// (`[secondaires] [Close]`) : une lecture seule qui propose aussi un geste
/// (#306 : « Undo »).
pub(super) fn modal_read_only_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::ReadOnly(size),
        DialogFooter::always(&label, &hint),
        extra_buttons,
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
    size: PlainSize,
    verb: &str,
    verb_hint: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> ConfirmOutcome {
    match modal_frame(
        ctx,
        id,
        title,
        DialogNature::Confirm(size),
        DialogFooter::always(verb, verb_hint),
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
    size: PlainSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> SaveChangesOutcome {
    let label = tr!("unsaved-save");
    let hint = tr!("unsaved-save", "hint");
    match modal_frame(
        ctx,
        id,
        title,
        DialogNature::SaveChanges(size),
        DialogFooter::always(&label, &hint),
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
    size: PlainSize,
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
        DialogNature::Alert(size),
        DialogFooter::always(&label, &hint),
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
fn modal_frame(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    nature: DialogNature,
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
                let size = nature.size().min(screen.size());
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
    let focus = nature.initial_focus();
    if just_opened {
        if focus == InitialFocus::FirstField {
            ctx.data_mut(|data| data.insert_temp(walk_id, FocusWalk::default()));
        }
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
                if focus == InitialFocus::FirstField {
                    body.interact(
                        egui::Rect::from_min_size(body.max_rect().min, egui::Vec2::ZERO),
                        anchor_id,
                        egui::Sense::focusable_noninteractive(),
                    );
                }
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
                    footer_spec.enabled(),
                    footer_spec.disabled_reason(),
                );
                let focus_default = just_opened && focus == InitialFocus::DefaultButton;
                clicks = paint_footer(
                    &mut footer,
                    nature,
                    &footer_spec,
                    message,
                    focus_default,
                    extra_buttons,
                );
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
    } else if clicks.cancel || (enter && nature.cancel_is_default()) {
        FrameOutcome::Cancel
    } else if enter && footer_spec.enabled() {
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
    nature: DialogNature,
    spec: &DialogFooter<'_>,
    message: Option<FooterMessage<'_>>,
    focus_default: bool,
    extra_buttons: impl FnOnce(&mut egui::Ui),
) -> FooterClicks {
    let mut clicks = FooterClicks::default();
    let default_fill = footer.visuals().selection.bg_fill;
    // Premier ajouté = le plus à droite : le bouton principal — par défaut,
    // sauf dans une confirmation, où il est destructeur et Cancel l'est (S-R4).
    let confirm = nature.cancel_is_default();
    let response = footer
        .scope(|ui| {
            let mut accept = egui::Button::new(spec.accept_label).min_size(FOOTER_BUTTON_SIZE);
            if confirm {
                theme::destructive_button_visuals(ui.visuals_mut());
            } else {
                accept = accept.fill(default_fill);
            }
            ui.add_enabled(spec.enabled(), accept)
                .on_hover_text(spec.hint())
                .on_disabled_hover_text(spec.disabled_reason())
        })
        .inner;
    // S-F11 : le bouton par défaut prend le focus à l'ouverture (le principal ;
    // Cancel dans une confirmation, plus bas).
    if focus_default && !confirm {
        response.request_focus();
    }
    clicks.ids.push(response.id);
    clicks.accept = response.clicked();
    let cancel = |ui: &mut egui::Ui, clicks: &mut FooterClicks| {
        let mut button = egui::Button::new(tr!("dialog-cancel")).min_size(FOOTER_BUTTON_SIZE);
        if confirm {
            button = button.fill(default_fill);
        }
        let response = ui.add(button).on_hover_text(tr!("dialog-cancel", "hint"));
        if focus_default && confirm {
            response.request_focus();
        }
        clicks.ids.push(response.id);
        clicks.cancel = response.clicked();
    };
    match nature.footer() {
        FooterShape::Dialog => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                extra_buttons(ui);
                paint_footer_message(ui, message);
            });
        }
        FooterShape::Confirm => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                paint_footer_message(ui, message);
            });
        }
        FooterShape::SaveChanges => {
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
        FooterShape::Wizard => {
            extra_buttons(footer);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                cancel(ui, &mut clicks);
                paint_footer_message(ui, message);
            });
        }
        FooterShape::ReadOnly => {
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

    /// Une nature de brouillon, de la taille libre des tests.
    fn brouillon() -> DialogNature {
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(600.0, 300.0)))
    }

    /// Une nature d'assistant, de la taille libre des tests.
    fn assistant() -> DialogNature {
        DialogNature::Wizard(InteractiveSize::Free(egui::vec2(600.0, 300.0)))
    }

    /// Un dialogue de `nature` dont le bouton principal est `principal` (grisé
    /// avec la raison `raison` si `actif` est faux), un bouton supplémentaire
    /// « Autre » ; rend l'issue de l'image et ses textes.
    fn pied(
        context: &egui::Context,
        nature: DialogNature,
        actif: bool,
        events: Vec<egui::Event>,
        mut contenu: impl FnMut(&mut egui::Ui),
    ) -> (DialogOutcome, Vec<Texte>) {
        let mut issue = DialogOutcome::Open;
        let sortie = image_de(context, egui::vec2(800.0, 600.0), events, |ui| {
            issue = modal_frame(
                ui.ctx(),
                egui::Id::new("dialogue_pied"),
                "Titre",
                nature,
                DialogFooter::gated("Principal", "Bulle", actif, "Raison"),
                |ui| {
                    let _ = ui.button("Autre");
                },
                |ui| {
                    contenu(ui);
                },
            )
            .dialog();
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
            peints = pied(&context, brouillon(), true, Vec::new(), |_| {}).1;
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
            peints = pied(&context, assistant(), true, Vec::new(), |_| {}).1;
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
                issue = modal_read_only(
                    ui.ctx(),
                    id,
                    "Titre",
                    PlainSize::Content(egui::vec2(300.0, 200.0)),
                    |ui| {
                        ui.label("contenu");
                    },
                );
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
            peints = pied(&context, brouillon(), false, Vec::new(), |_| {}).1;
        }
        assert!(peints.iter().any(|t| t.texte == "Raison"));
        context.data_mut(|data| {
            data.insert_temp(dialog_error_id(), Some("Échec du calcul".to_string()));
        });
        peints = pied(&context, brouillon(), false, Vec::new(), |_| {}).1;
        assert!(peints.iter().any(|t| t.texte == "Échec du calcul"));
        assert!(!peints.iter().any(|t| t.texte == "Raison"));
        context.data_mut(|data| data.insert_temp(dialog_error_id(), None::<String>));
        peints = pied(&context, brouillon(), false, Vec::new(), |_| {}).1;
        assert!(
            peints.iter().any(|t| t.texte == "Raison"),
            "la raison revient"
        );

        let actif = egui::Context::default();
        for _ in 0..3 {
            peints = pied(&actif, brouillon(), true, Vec::new(), |_| {}).1;
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
        let issue = pied(context, brouillon(), true, events, |ui| {
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
            pied(&context, brouillon(), true, Vec::new(), |ui| {
                ui.label("contenu");
            });
        }
        assert_eq!(context.memory(|memory| memory.focused()), None);

        let context = egui::Context::default();
        pied(&context, brouillon(), true, Vec::new(), |_| {});
        let (issue, _) = pied(
            &context,
            brouillon(),
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
                issue = modal_dialog(ui.ctx(), id, "Titre", InteractiveSize::Free(taille), |ui| {
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
                PlainSize::Content(egui::vec2(400.0, 160.0)),
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
                PlainSize::Content(egui::vec2(400.0, 160.0)),
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
            // Cancel a le focus à l'ouverture (S-F11) : le verbe est son
            // voisin d'avant, Maj + Tab y mène.
            let retour = egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            };
            let sortie = confirmation(&context, vec![retour]).1;
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
                    PlainSize::Content(egui::vec2(400.0, 160.0)),
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

    /// S-F11 : chaque nature dit son pied, sa taille par défaut (une classe),
    /// son focus initial ; la taille est celle du champ quand l'appelant en
    /// donne une. Classes : Petite 480 x 170, Moyenne 480 x 420, Grande 640 x 500.
    #[test]
    fn chaque_nature_fixe_son_pied_sa_taille_et_son_focus() {
        use InitialFocus::{DefaultButton, FirstField, Nothing};
        let petite = egui::vec2(480.0, 170.0);
        let moyenne = egui::vec2(480.0, 420.0);
        let grande = egui::vec2(640.0, 500.0);
        let interactive = InteractiveSize::Default;
        let simple = PlainSize::Default;
        for (nature, pied, taille, focus) in [
            (
                DialogNature::Settings(interactive),
                FooterShape::Dialog,
                moyenne,
                FirstField,
            ),
            (
                DialogNature::Draft(interactive),
                FooterShape::Dialog,
                moyenne,
                FirstField,
            ),
            (
                DialogNature::Choice(simple),
                FooterShape::Dialog,
                petite,
                FirstField,
            ),
            (
                DialogNature::Wizard(interactive),
                FooterShape::Wizard,
                grande,
                FirstField,
            ),
            (
                DialogNature::LiveSettings(interactive),
                FooterShape::ReadOnly,
                moyenne,
                FirstField,
            ),
            (
                DialogNature::ReadOnly(simple),
                FooterShape::ReadOnly,
                moyenne,
                Nothing,
            ),
            (
                DialogNature::Confirm(simple),
                FooterShape::Confirm,
                petite,
                DefaultButton,
            ),
            (
                DialogNature::SaveChanges(simple),
                FooterShape::SaveChanges,
                petite,
                DefaultButton,
            ),
            (
                DialogNature::Alert(simple),
                FooterShape::ReadOnly,
                petite,
                DefaultButton,
            ),
        ] {
            assert_eq!(nature.footer(), pied, "{nature:?} : pied");
            assert_eq!(nature.size(), taille, "{nature:?} : taille par défaut");
            assert_eq!(nature.initial_focus(), focus, "{nature:?} : focus");
        }
        // Le champ de la variante l'emporte sur la classe par défaut.
        assert_eq!(
            DialogNature::Choice(PlainSize::Class(SizeClass::Large)).size(),
            grande
        );
        assert_eq!(
            DialogNature::Choice(PlainSize::Content(egui::vec2(560.0, 170.0))).size(),
            egui::vec2(560.0, 170.0)
        );
        assert_eq!(
            DialogNature::Draft(InteractiveSize::Free(egui::vec2(1000.0, 660.0))).size(),
            egui::vec2(1000.0, 660.0)
        );
        assert_eq!(
            DialogNature::Settings(InteractiveSize::Class(SizeClass::Small)).size(),
            petite
        );
    }

    /// Le widget focalisé est-il le bouton peint `texte` ?
    fn focus_sur_le_bouton(context: &egui::Context, peints: &[Texte], texte: &str) -> bool {
        let Some(focused) = context.memory(|memory| memory.focused()) else {
            return false;
        };
        context
            .read_response(focused)
            .is_some_and(|response| response.rect.contains(centre(peints, texte)))
    }

    /// S-F11 : à l'ouverture, une confirmation donne le focus à **Cancel**, un
    /// enregistrement et une alerte à leur bouton principal — jamais à un champ,
    /// même si le contenu en a un ; une lecture seule ne le donne à rien.
    /// Témoin : un brouillon, lui, le donne au champ.
    #[test]
    fn le_focus_initial_suit_la_nature() {
        let mut texte = String::from("abc");
        let ouvrir = |nature: DialogNature, texte: &mut String| {
            let context = egui::Context::default();
            let mut peints = Vec::new();
            let mut champ = None;
            for _ in 0..6 {
                let sortie = image_de(&context, egui::vec2(800.0, 600.0), Vec::new(), |ui| {
                    modal_frame(
                        ui.ctx(),
                        egui::Id::new("nature_focus"),
                        "Titre",
                        nature,
                        DialogFooter::always("Principal", "Bulle"),
                        |_| {},
                        |ui| {
                            champ = Some(ui.text_edit_singleline(texte).id);
                        },
                    );
                });
                peints = textes(&sortie.shapes);
            }
            (context, peints, champ)
        };
        let simple = PlainSize::Default;

        let (context, peints, champ) = ouvrir(DialogNature::Confirm(simple), &mut texte);
        assert!(
            focus_sur_le_bouton(&context, &peints, &tr!("dialog-cancel")),
            "Cancel"
        );
        assert_ne!(context.memory(|memory| memory.focused()), champ);

        for nature in [
            DialogNature::SaveChanges(simple),
            DialogNature::Alert(simple),
        ] {
            let (context, peints, champ) = ouvrir(nature, &mut texte);
            assert!(
                focus_sur_le_bouton(&context, &peints, "Principal"),
                "{nature:?}"
            );
            assert_ne!(context.memory(|memory| memory.focused()), champ);
        }

        let (context, _, _) = ouvrir(DialogNature::ReadOnly(simple), &mut texte);
        assert_eq!(context.memory(|memory| memory.focused()), None, "lecture");

        // Des réglages en direct ont des champs : focus sur le premier, comme
        // un brouillon, malgré le pied « Close seul » d'une lecture.
        let (context, _, champ) = ouvrir(
            DialogNature::LiveSettings(InteractiveSize::Default),
            &mut texte,
        );
        assert_eq!(
            context.memory(|memory| memory.focused()),
            champ,
            "réglages en direct : le premier champ"
        );

        let (context, _, champ) = ouvrir(DialogNature::Draft(InteractiveSize::Default), &mut texte);
        assert_eq!(
            context.memory(|memory| memory.focused()),
            champ,
            "témoin : le brouillon va au champ"
        );
    }

    /// S-F10, S-F11 : un pied « peut être grisé » grisé montre sa **raison**,
    /// jamais la bulle du bouton actif ; un pied « toujours actif » n'a pas de
    /// raison. (La bulle n'est peinte qu'au survol : on lit donc le pied.)
    #[test]
    fn un_pied_grise_montre_sa_raison_et_non_sa_bulle() {
        let context = egui::Context::default();
        let mut peints = Vec::new();
        for _ in 0..3 {
            peints = pied(&context, brouillon(), false, Vec::new(), |_| {}).1;
        }
        assert!(peints.iter().any(|t| t.texte == "Raison"));
        assert!(!peints.iter().any(|t| t.texte == "Bulle"));
        let grise = DialogFooter::gated("P", "Bulle", false, "Raison");
        assert!(!grise.enabled());
        assert_eq!(grise.disabled_reason(), "Raison");
        assert_eq!(grise.hint(), "Bulle");
        let toujours = DialogFooter::always("P", "Bulle");
        assert!(toujours.enabled());
        assert_eq!(toujours.disabled_reason(), "", "témoin : jamais de raison");
        assert_eq!(
            footer_message(None, toujours.enabled(), toujours.disabled_reason()),
            None
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
                    let close = modal_dialog(
                        ui.ctx(),
                        id,
                        "Test",
                        InteractiveSize::Free(egui::vec2(300.0, 200.0)),
                        |ui| {
                            ui.label("contenu");
                        },
                    );
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
                        brouillon(),
                        DialogFooter::gated("Suivant", "", accept_enabled, "raison"),
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
