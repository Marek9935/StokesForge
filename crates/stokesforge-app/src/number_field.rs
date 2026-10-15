//! La brique « nombre » (#220, N1–N3, N8, F7) : **le seul** champ numérique de
//! l'interface. Plus de `DragValue` ni de `Slider` (garde
//! `aucun_champ_numerique_hors_de_la_brique`, `tests/gardes.rs`).
//!
//! # Ce que fait un champ
//!
//! - un champ texte, large comme [`FIELD_WIDTH`] ; un `▼` seulement si la
//!   [`Quantity`] propose des valeurs ([`Quantity::presets`]) ;
//! - un clic dans le texte sélectionne tout, sans ouvrir la liste ;
//! - la saisie s'applique à Entrée ou en quittant le champ, Échap rend
//!   l'ancienne valeur ; une saisie illisible est **refusée**, une valeur hors
//!   bornes **ramenée** (cadre rouge ou orange, bulle) : tout ce cycle est celui
//!   de [`crate::field_state::edit_text`] ;
//! - liste fermée : ↑/↓ d'un pas, Maj+↑/↓ d'un grand pas ; Alt+↓ ouvre la liste ;
//! - liste ouverte : la molette la fait défiler, ↑/↓ surligne, Entrée choisit,
//!   Échap la ferme (sans toucher à la valeur) ;
//! - molette (#233, #235) : le **survol** suffit, sans focus, et **seulement**
//!   sur un champ à liste : un cran = la valeur proposée voisine (`step_preset`),
//!   une valeur hors liste saute à l'entrée voisine, aux extrémités elle reste.
//!   Un champ sans liste ne réagit jamais : la fenêtre défile ;
//! - aspect (#234) : cadre unique, flèche pleine à l'intérieur à droite, fond de
//!   saisie du thème ; voir [`crate::list_frame`].
//!
//! La liste est une `egui::Area` à elle, pas `egui::Popup` : egui ne retient
//! qu'une fenêtre surgissante ouverte par viewport, donc dans un dialogue modal
//! la liste refermerait la fenêtre parente (issue #9, comme `color_button`).
//!
//! # Auto (#221)
//!
//! Une grandeur a un Auto ([`AutoKind`]) : `Field` — champ vidé ou mot `auto`
//! (casse libre, ou la seule lettre `a`/`A`)
//! rend la valeur automatique ([`auto_number_field`], texte grisé « Auto (…) ») ;
//! `Group` — le mot `auto` coche l'Auto du groupe ([`NumberResponse::auto`]),
//! un champ vidé est refusé. Sans Auto, le mot et le champ vidé sont refusés.
//!
//! # Historique
//!
//! Chaque pas de flèche ou de molette est un changement rendu à l'appelant
//! ([`NumberResponse::changed`]). Des crans d'affilée ne font qu'**une** entrée
//! d'historique parce que l'appelant les écrit dans son brouillon ou dans la
//! macro de son dialogue (propriétés du graphe) : la brique ne touche pas au
//! modèle.
//!
//! Le formateur des nombres affichés est provisoire (#222) : décimales de la
//! grandeur, zéros de fin retirés. La virgule décimale arrive avec #223.

use eframe::egui;
use stokesforge_core::number_format::{format_exact, format_grid};

use crate::app::shortcuts;
use crate::field_state::{self, Bound, Verdict};
use crate::list_frame;
use crate::quantities::{AutoKind, Quantity};
use crate::tr;

/// Largeur d'un champ numérique, en points (la « courte » de L2, #227).
pub const FIELD_WIDTH: f32 = crate::theme::FIELD_WIDTH_SHORT;

/// Une valeur que la brique sait lire et écrire. Les entiers arrondissent.
pub trait Number: Copy {
    fn to_f64(self) -> f64;
    fn from_f64(value: f64) -> Self;
}

impl Number for f64 {
    fn to_f64(self) -> f64 {
        self
    }
    fn from_f64(value: f64) -> Self {
        value
    }
}

impl Number for f32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(value: f64) -> Self {
        value as f32
    }
}

macro_rules! integer_number {
    ($($type:ty),*) => {$(
        impl Number for $type {
            fn to_f64(self) -> f64 {
                self as f64
            }
            fn from_f64(value: f64) -> Self {
                value.round() as $type
            }
        }
    )*};
}
integer_number!(i32, i64, u8, u32, usize);

/// Ce que rend un champ numérique.
pub struct NumberResponse {
    /// Le champ de saisie (survol, aide).
    pub response: egui::Response,
    /// La valeur a changé cette image : saisie validée, pas de flèche, cran de
    /// molette ou choix dans la liste.
    pub changed: bool,
    /// L'utilisateur a demandé l'Auto cette image (champ vidé ou mot `auto`,
    /// selon [`AutoKind`]) : l'appelant rend la valeur ou le groupe automatique.
    pub auto: bool,
}

/// Ce qu'une saisie dit : un nombre, ou l'Auto (#221).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Value(f64),
    Auto,
}

/// L'état de la liste de valeurs d'un champ.
#[derive(Clone, Copy, Debug, Default)]
struct Presets {
    open: bool,
    highlighted: usize,
}

/// Pose un champ numérique sur la grandeur `quantity`, dans la rangée en cours.
/// `id_salt` identifie le champ (brouillon, liste) ; la valeur est écrite dans
/// `value` dès qu'elle change.
pub fn number_field<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut T,
    quantity: &Quantity,
) -> NumberResponse {
    number_field_core(ui, id_salt, value, quantity, None)
}

/// Un champ dont la valeur peut être automatique (#221) : `None` montre `hint`
/// en gris (« Auto (0–4000) », la valeur calculée) ; vider le champ ou taper
/// `auto` la remet à `None` (grandeur [`AutoKind::Field`]). Une saisie lisible
/// la fixe à `Some`.
pub fn auto_number_field<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut Option<T>,
    quantity: &Quantity,
    hint: &str,
) -> NumberResponse {
    let mut scratch = value.map_or(0.0, |v| v.to_f64());
    let shown_auto = value.is_none().then_some(hint);
    let mut response = number_field_core(ui, id_salt, &mut scratch, quantity, shown_auto);
    if response.auto {
        *value = None;
        response.changed = true;
    } else if response.changed {
        *value = Some(T::from_f64(scratch));
    }
    response
}

fn number_field_core<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut T,
    quantity: &Quantity,
    auto_hint: Option<&str>,
) -> NumberResponse {
    let ctx = ui.ctx().clone();
    let id = field_state::field_id(ui, id_salt);
    let list_id = id.with("presets");
    let mut presets = ctx
        .data(|data| data.get_temp::<Presets>(list_id))
        .unwrap_or_default();
    let has_presets = !quantity.presets.is_empty();
    let mut current = value.to_f64();
    let mut changed = false;
    let mut auto = false;
    // Un Auto de la valeur elle-même (`AutoKind::Field`) ouvre la liste par une
    // entrée « Auto » ; les valeurs proposées sont alors décalées d'un cran.
    let offset = usize::from(quantity.auto == AutoKind::Field);
    let focused = ctx.memory(|memory| memory.has_focus(id));

    // — Clavier, avant le champ : le `TextEdit` prendrait ↑/↓ et Entrée. —
    // Échap ferme la liste sans rendre la valeur (F7). egui retire lui-même le
    // focus au début de l'image où Échap est pressée : la liste ouverte, pas le
    // focus, dit que l'Échap est pour elle, et le champ le reprend.
    let close_list = presets.open && key(ui, &shortcuts::KEY_ESCAPE);
    if close_list {
        presets.open = false;
    }
    if focused && !close_list {
        // Alt+↓ est toujours pris : un pas ne doit pas le lire (egui fait
        // correspondre « aucun modificateur » à n'importe quelle combinaison).
        let alt_down = key(ui, &shortcuts::KEY_ALT_ARROW_DOWN);
        if alt_down && has_presets && !presets.open {
            presets.open = true;
            presets.highlighted = nearest_preset(quantity, current) + offset;
        } else if presets.open {
            if key(ui, &shortcuts::KEY_ARROW_DOWN) {
                presets.highlighted =
                    (presets.highlighted + 1).min(quantity.presets.len() - 1 + offset);
            }
            if key(ui, &shortcuts::KEY_ARROW_UP) {
                presets.highlighted = presets.highlighted.saturating_sub(1);
            }
            if key(ui, &shortcuts::KEY_ENTER) {
                if presets.highlighted < offset {
                    auto = true;
                } else {
                    current = quantity.presets[presets.highlighted - offset];
                    changed = true;
                }
                presets.open = false;
            }
        } else {
            let steps = i32::from(key(ui, &shortcuts::KEY_SHIFT_ARROW_UP)) * 10
                - i32::from(key(ui, &shortcuts::KEY_SHIFT_ARROW_DOWN)) * 10
                + i32::from(key(ui, &shortcuts::KEY_ARROW_UP))
                - i32::from(key(ui, &shortcuts::KEY_ARROW_DOWN));
            if steps != 0 {
                current = step_value(quantity, current, steps);
                changed = true;
            }
        }
    }

    // — Le champ. —
    let shown = match auto_hint {
        Some(_) => String::new(),
        // Hors saisie : 6 chiffres significatifs ; dans le champ : la valeur
        // entière, pour qu'un Tab sans frappe n'en perde aucun (#222).
        None if focused => format_exact(current),
        None => format_grid(current),
    };
    let arrow_room = if has_presets {
        list_frame::ARROW_ROOM
    } else {
        0.0
    };
    let edited = field_state::edit_text_with_arrow(
        ui,
        id_salt,
        if quantity.is_compact() {
            crate::theme::FIELD_WIDTH_DIGITS
        } else if quantity.is_compact_signed() {
            crate::theme::FIELD_WIDTH_SIGNED_DIGITS
        } else {
            FIELD_WIDTH
        },
        &shown,
        auto_hint.unwrap_or(""),
        arrow_room,
        |text| read(quantity, text),
    );
    let response = edited.response;
    if close_list {
        response.request_focus();
    }
    let typed = edited.value.is_some();
    match edited.value {
        Some(Entry::Value(value)) => {
            current = value;
            changed = true;
        }
        Some(Entry::Auto) => auto = true,
        None => {}
    }
    if response.gained_focus() {
        select_all(&ctx, id, shown.chars().count());
    }

    // — Molette (#233, #235) : le survol suffit, sans focus ; un cran = la
    // valeur proposée voisine. Un champ sans liste n'y réagit jamais : la
    // fenêtre défile. —
    if has_presets && response.hovered() && !presets.open {
        let notches = wheel_notches(ui);
        if notches != 0 {
            let moved = step_preset(quantity, current, notches);
            if moved != current {
                current = moved;
                changed = true;
            }
        }
    }

    // — La liste de valeurs. —
    if has_presets {
        // La flèche pleine est dans le cadre du champ (#234), cliquable.
        let arrow = list_frame::arrow_rect(response.rect);
        let button = ui
            .interact(arrow, list_id.with("arrow"), egui::Sense::click())
            .on_hover_text(tr!("number-presets"));
        list_frame::paint_arrow(ui, arrow, &button);
        if button.clicked() {
            presets.open = !presets.open;
            presets.highlighted = if auto_hint.is_some() {
                0
            } else {
                nearest_preset(quantity, current) + offset
            };
        }
        if presets.open {
            let anchor = response.rect.left_bottom() + egui::vec2(0.0, 2.0);
            let mut picked = None;
            let area = egui::Area::new(list_id)
                .order(egui::Order::Foreground)
                .fixed_pos(anchor)
                .constrain(true)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        picked = preset_popup_contents(ui, quantity, presets.highlighted);
                    });
                });
            let pressed_at = ui.input(|input| {
                input
                    .pointer
                    .any_pressed()
                    .then(|| input.pointer.interact_pos())
                    .flatten()
            });
            let outside = pressed_at.is_some_and(|pos| {
                !response.rect.contains(pos)
                    && !button.rect.contains(pos)
                    && !area.response.rect.contains(pos)
            });
            if let Some(choice) = picked {
                match choice {
                    Some(preset) => {
                        current = preset;
                        changed = true;
                    }
                    None => auto = true,
                }
                presets.open = false;
                response.request_focus();
            } else if outside {
                presets.open = false;
            }
        }
    }
    ctx.data_mut(|data| data.insert_temp(list_id, presets));

    if changed {
        *value = T::from_f64(current);
        // Un pas, un cran ou un choix repart de la valeur : plus de brouillon,
        // plus de cadre. Une saisie validée garde le sien (valeur ramenée).
        if !typed {
            field_state::discard_draft(&ctx, id);
        }
    }
    NumberResponse {
        response,
        changed,
        auto,
    }
}

/// Consomme la touche `shortcut` du registre.
fn key(ui: &egui::Ui, shortcut: &egui::KeyboardShortcut) -> bool {
    ui.input_mut(|input| input.consume_shortcut(shortcut))
}

/// Sélectionne tout le texte du champ `id` (`len` caractères).
fn select_all(ctx: &egui::Context, id: egui::Id, len: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(len),
            )));
        state.store(ctx, id);
    }
}

/// Les crans de molette de cette image (positifs vers le haut), et la molette
/// consommée : la fenêtre sous le champ ne défile pas. Ctrl+molette est le zoom
/// (X1) et n'est pas pris.
fn wheel_notches(ui: &mut egui::Ui) -> i32 {
    let total: f32 = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if !modifiers.command => Some(match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                }),
                _ => None,
            })
            .sum()
    });
    if total == 0.0 {
        return 0;
    }
    ui.input_mut(|input| {
        input.smooth_scroll_delta = egui::Vec2::ZERO;
    });
    let magnitude = total.abs().round().max(1.0) as i32;
    magnitude * total.signum() as i32
}

/// `value` déplacée de `steps` pas (un grand pas vaut dix crans), ramenée dans
/// les bornes et arrondie aux décimales du pas pour que 0,1 + 0,2 reste 0,3.
fn step_value(quantity: &Quantity, value: f64, steps: i32) -> f64 {
    let unit = if steps.abs() >= 10 {
        quantity.big_step * f64::from(steps / 10)
    } else {
        quantity.step * f64::from(steps)
    };
    let moved = (value + unit).clamp(quantity.min, quantity.max);
    (moved * 1e9).round() / 1e9
}

/// `value` déplacée de `notches` entrées de la liste proposée (positif = vers
/// le haut). Une valeur hors liste saute à l'entrée voisine dans le sens du
/// cran ; aux extrémités, elle reste.
fn step_preset(quantity: &Quantity, value: f64, notches: i32) -> f64 {
    const EPS: f64 = 1e-9;
    let mut current = value;
    for _ in 0..notches.unsigned_abs() {
        let next = if notches > 0 {
            quantity
                .presets
                .iter()
                .copied()
                .find(|p| *p > current + EPS)
        } else {
            quantity
                .presets
                .iter()
                .copied()
                .rev()
                .find(|p| *p < current - EPS)
        };
        match next {
            Some(preset) => current = preset,
            None => break,
        }
    }
    current
}

/// L'indice de la valeur proposée la plus proche de `value`.
fn nearest_preset(quantity: &Quantity, value: f64) -> usize {
    quantity
        .presets
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map_or(0, |(index, _)| index)
}

/// Le contenu de la liste de valeurs (le nom contient « popup » : c'est un
/// choix exclusif, `aucun_bouton_selectionnable_hors_d_un_menu`). Rend la
/// valeur cliquée.
fn preset_popup_contents(
    ui: &mut egui::Ui,
    quantity: &Quantity,
    highlighted: usize,
) -> Option<Option<f64>> {
    let offset = usize::from(quantity.auto == AutoKind::Field);
    let mut picked = None;
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            ui.set_min_width(FIELD_WIDTH);
            if offset == 1
                && ui
                    .selectable_label(highlighted == 0, tr!("common-auto"))
                    .clicked()
            {
                picked = Some(None);
            }
            for (index, preset) in quantity.presets.iter().enumerate() {
                let index = index + offset;
                let entry = ui.selectable_label(
                    index == highlighted,
                    format_value(*preset, quantity.decimals),
                );
                if entry.clicked() {
                    picked = Some(Some(*preset));
                }
                if index == highlighted
                    && ui.input(|input| {
                        input.key_pressed(egui::Key::ArrowDown)
                            || input.key_pressed(egui::Key::ArrowUp)
                    })
                {
                    entry.scroll_to_me(None);
                }
            }
        });
    picked
}

/// Lit une saisie : un nombre fini, entier si la grandeur l'est, ramené aux
/// bornes.
fn read(quantity: &Quantity, text: &str) -> Verdict<Entry> {
    let trimmed = text.trim();
    // L'Auto : champ vidé (la valeur elle-même en a un) ou le mot `auto` (la
    // valeur ou son groupe). Ailleurs, ni l'un ni l'autre n'est lisible.
    if quantity.auto == AutoKind::Field && trimmed.is_empty() {
        return Verdict::Valid(Entry::Auto);
    }
    if quantity.auto != AutoKind::None
        && (trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("a"))
    {
        return Verdict::Valid(Entry::Auto);
    }
    if quantity.auto == AutoKind::Group && trimmed.is_empty() {
        return Verdict::Refused(tr!("field-refused-use-auto"));
    }
    let Some(parsed) = stokesforge_core::number_format::parse_input(trimmed) else {
        return refused(quantity);
    };
    if quantity.integer && parsed.fract() != 0.0 {
        return refused(quantity);
    }
    if parsed < quantity.min {
        return Verdict::Clamped {
            value: Entry::Value(quantity.min),
            bound: Bound::Min,
            limit: format_value(quantity.min, quantity.decimals),
        };
    }
    if parsed > quantity.max {
        return Verdict::Clamped {
            value: Entry::Value(quantity.max),
            bound: Bound::Max,
            limit: format_value(quantity.max, quantity.decimals),
        };
    }
    Verdict::Valid(Entry::Value(parsed))
}

fn refused(quantity: &Quantity) -> Verdict<Entry> {
    Verdict::Refused(if quantity.integer {
        tr!("field-refused-integer")
    } else {
        tr!("field-refused-number")
    })
}

/// Un nombre tel qu'il s'affiche : au plus `decimals` décimales, sans zéros de
/// fin, jamais « -0 ».
pub fn format_value(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantities;

    fn touche(key: egui::Key) -> egui::Event {
        touche_avec(key, egui::Modifiers::NONE)
    }

    #[test]
    fn un_entier_de_trois_chiffres_sans_liste_prend_la_largeur_compacte() {
        for compact in [
            &quantities::COLOR_CHANNEL,
            &quantities::HUE_DEGREES,
            &quantities::PERCENT,
            &quantities::OPACITY_PERCENT,
        ] {
            assert!(compact.is_compact());
        }
        // Plus de trois chiffres, réel ou signé : largeur courte.
        assert!(quantities::ROTATION.is_compact_signed());
        assert!(!quantities::ROTATION.is_compact());
        assert!(!quantities::FREE.is_compact_signed());
        assert!(!quantities::TEXT_FORMAT_SIZE.is_compact_signed());
        assert!(!quantities::ITERATIONS.is_compact());
        assert!(!quantities::FREE.is_compact());
        assert!(!quantities::COUNT.is_compact());
    }

    #[test]
    fn le_format_retire_les_zeros_de_fin_et_le_moins_zero() {
        assert_eq!(format_value(2.5, 2), "2.5");
        assert_eq!(format_value(2.0, 2), "2");
        assert_eq!(format_value(-0.0, 2), "0");
        assert_eq!(format_value(-0.001, 2), "0");
        assert_eq!(format_value(100.0, 0), "100");
        assert_eq!(format_value(0.3, 8), "0.3");
    }

    #[test]
    fn la_virgule_decimale_est_lue_comme_le_point() {
        // #223 : « 2,5 » vaut 2.5 ; une virgule ambiguë reste refusée.
        assert_eq!(
            read(&quantities::AXIS_BOUND, "2,5"),
            Verdict::Valid(Entry::Value(2.5))
        );
        assert!(matches!(
            read(&quantities::AXIS_BOUND, "1,2,3"),
            Verdict::Refused(_)
        ));
    }

    #[test]
    fn une_saisie_illisible_est_refusee() {
        for texte in ["abc", "1..2", "", "inf", "nan", "1e999"] {
            assert!(
                matches!(read(&quantities::LINE_WIDTH, texte), Verdict::Refused(_)),
                "{texte:?}"
            );
        }
    }

    #[test]
    fn le_corps_de_la_barre_de_format_accepte_auto() {
        // #245 : la barre de format doit pouvoir remettre le corps en Auto,
        // par le mot, par le champ vidé ou par l'entrée de la liste.
        let corps = quantities::TEXT_FORMAT_SIZE;
        assert_eq!(corps.auto, AutoKind::Field);
        for texte in ["auto", ""] {
            assert_eq!(read(&corps, texte), Verdict::Valid(Entry::Auto));
        }
        // Le corps de l'éditeur sur place n'a pas d'Auto : ce n'est pas lui.
        assert_eq!(quantities::TEXT_SIZE.auto, AutoKind::None);
    }

    #[test]
    fn auto_est_accepte_selon_la_grandeur() {
        let champ = quantities::FREE.with_auto(AutoKind::Field);
        for texte in ["", "  ", "auto", "Auto", " AUTO ", "a", "A"] {
            assert_eq!(
                read(&champ, texte),
                Verdict::Valid(Entry::Auto),
                "{texte:?}"
            );
        }
        assert_eq!(read(&champ, "5"), Verdict::Valid(Entry::Value(5.0)));
        // Une borne de groupe accepte le mot, pas le champ vidé.
        let groupe = &quantities::AXIS_BOUND;
        assert_eq!(read(groupe, "auto"), Verdict::Valid(Entry::Auto));
        assert!(matches!(read(groupe, ""), Verdict::Refused(_)));
        assert!(matches!(read(groupe, "  "), Verdict::Refused(_)));
    }

    /// Témoin : une grandeur sans Auto refuse le mot comme le champ vidé.
    #[test]
    fn sans_auto_le_mot_et_le_champ_vide_sont_refuses() {
        for texte in ["auto", "Auto", "a", "A", ""] {
            assert!(
                matches!(read(&quantities::LINE_WIDTH, texte), Verdict::Refused(_)),
                "{texte:?}"
            );
        }
    }

    #[test]
    fn un_entier_refuse_les_decimales_mais_pas_un_point_zero() {
        assert!(matches!(
            read(&quantities::MAX_PEAKS, "2.5"),
            Verdict::Refused(_)
        ));
        assert_eq!(
            read(&quantities::MAX_PEAKS, "3.0"),
            Verdict::Valid(Entry::Value(3.0))
        );
    }

    #[test]
    fn hors_bornes_la_valeur_est_ramenee_a_la_borne() {
        let Verdict::Clamped {
            value,
            bound,
            limit,
        } = read(&quantities::LINE_WIDTH, "35")
        else {
            panic!("35 dépasse 20");
        };
        assert_eq!(
            (value, bound, limit.as_str()),
            (Entry::Value(20.0), Bound::Max, "20")
        );
        assert!(matches!(
            read(&quantities::LINE_WIDTH, "-1"),
            Verdict::Clamped {
                bound: Bound::Min,
                ..
            }
        ));
    }

    /// #260 : les plafonds des champs d'analyse sont ceux de
    /// `stokesforge-analysis`, que `Operation::validate` applique aussi. Le
    /// champ « Max peaks » monte de 50 à 100 pics.
    #[test]
    fn les_champs_d_analyse_suivent_les_plafonds_du_crate_d_analyse() {
        use stokesforge_analysis::ops::{MAX_COMPONENTS, MAX_DEGREE, MAX_ITERATIONS, MAX_PEAKS};
        assert_eq!(quantities::POLYNOMIAL_DEGREE.max, f64::from(MAX_DEGREE));
        assert_eq!(quantities::ITERATIONS.max, MAX_ITERATIONS as f64);
        assert_eq!(quantities::MAX_PEAKS.max, MAX_PEAKS as f64);
        assert_eq!(quantities::COMPONENTS.max, MAX_COMPONENTS as f64);
        assert_eq!(MAX_PEAKS, 100, "décision du 2026-10-15 : 100 pics partout");

        assert_eq!(
            read(&quantities::MAX_PEAKS, "100"),
            Verdict::Valid(Entry::Value(100.0))
        );
        let Verdict::Clamped {
            value,
            bound,
            limit,
        } = read(&quantities::MAX_PEAKS, "101")
        else {
            panic!("101 dépasse 100");
        };
        assert_eq!(
            (value, bound, limit.as_str()),
            (Entry::Value(100.0), Bound::Max, "100")
        );
    }

    #[test]
    fn des_decimales_au_clavier_sont_gardees() {
        assert_eq!(
            read(&quantities::LINE_WIDTH, "2.75"),
            Verdict::Valid(Entry::Value(2.75))
        );
    }

    #[test]
    fn un_pas_est_arrondi_et_borne() {
        let largeur = &quantities::LINE_WIDTH;
        assert_eq!(step_value(largeur, 0.2, 1), 0.3);
        assert_eq!(step_value(largeur, 19.95, 1), 20.0, "borne atteinte");
        assert_eq!(step_value(largeur, 0.0, -1), 0.0);
        assert_eq!(step_value(largeur, 1.0, 10), 2.0, "un grand pas");
        assert_eq!(step_value(largeur, 5.0, -10), 4.0);
    }

    #[test]
    fn la_valeur_proposee_la_plus_proche_est_surlignee() {
        assert_eq!(nearest_preset(&quantities::LINE_WIDTH, 1.4), 2);
        assert_eq!(nearest_preset(&quantities::LINE_WIDTH, 99.0), 5);
    }

    #[test]
    fn les_grandeurs_sont_coherentes() {
        for quantite in [
            quantities::FREE,
            quantities::LINE_WIDTH,
            quantities::SHAPE_LINE_WIDTH,
            quantities::SYMBOL_SIZE,
            quantities::TEXT_SIZE,
            quantities::TEXT_FORMAT_SIZE,
            quantities::ROTATION,
            quantities::COLOR_CHANNEL,
            quantities::OPACITY_PERCENT,
            quantities::HUE_DEGREES,
            quantities::PERCENT,
            quantities::POLYNOMIAL_DEGREE,
            quantities::PROMINENCE_PERCENT,
            quantities::MAX_PEAKS,
            quantities::COMPONENTS,
            quantities::ITERATIONS,
            quantities::COUNT,
            quantities::LINE_NUMBER,
        ] {
            assert!(quantite.min < quantite.max, "{quantite:?}");
            assert!(quantite.step > 0.0 && quantite.big_step >= quantite.step);
            for preset in quantite.presets {
                assert!(
                    (quantite.min..=quantite.max).contains(preset),
                    "{preset} hors de {quantite:?}"
                );
            }
        }
    }

    /// La plage du corps de texte est celle du modèle : deux définitions
    /// divergeraient en silence.
    #[test]
    fn le_corps_de_texte_suit_la_plage_du_modele() {
        let (bas, haut) = stokesforge_render_egui::AnnotationSpec::SIZE_RANGE;
        assert_eq!(
            (quantities::TEXT_SIZE.min, quantities::TEXT_SIZE.max),
            (bas, haut)
        );
    }

    // — Le champ peint : clavier, liste, molette. —

    fn touche_avec(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn cran(delta: f32) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, delta),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Un banc : un champ de l'épaisseur du trait (avec liste de valeurs) ou
    /// de `quantite`, sa valeur, l'horloge.
    struct Banc {
        ctx: egui::Context,
        valeur: f64,
        quantite: Quantity,
        temps: f64,
        id: Option<egui::Id>,
        champ: Option<egui::Rect>,
    }

    impl Banc {
        fn new(valeur: f64, quantite: Quantity) -> Self {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            Self {
                ctx,
                valeur,
                quantite,
                temps: 0.0,
                id: None,
                champ: None,
            }
        }

        /// Une image avec `events` ; `focus` donne le focus au champ avant sa
        /// pose. Rend ce qui a été peint et si la valeur a changé.
        fn image(&mut self, events: Vec<egui::Event>, focus: bool) -> (egui::FullOutput, bool) {
            self.temps += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                events,
                time: Some(self.temps),
                focused: true,
                ..Default::default()
            };
            let (valeur, quantite, id_champ, rect_champ) = (
                &mut self.valeur,
                self.quantite,
                &mut self.id,
                &mut self.champ,
            );
            let mut changed = false;
            let mut sortie = self.ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| {
                    let id = field_state::field_id(ui, "champ");
                    *id_champ = Some(id);
                    if focus {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                    let reponse = number_field(ui, "champ", valeur, &quantite);
                    *rect_champ = Some(reponse.response.rect);
                    changed = reponse.changed;
                });
            });
            sortie.textures_delta.clear();
            (sortie, changed)
        }

        /// Le champ prend le focus et se stabilise.
        fn focaliser(&mut self) {
            self.image(Vec::new(), true);
            self.image(Vec::new(), false);
        }

        fn peint(&mut self, texte: &str) -> bool {
            let (sortie, _) = self.image(Vec::new(), false);
            sortie.shapes.iter().any(|peint| {
                matches!(&peint.shape,
                    egui::Shape::Text(forme) if forme.galley.job.text == texte)
            })
        }

        fn cadres(sortie: &egui::FullOutput, couleur: egui::Color32) -> usize {
            sortie
                .shapes
                .iter()
                .filter(|peint| matches!(&peint.shape, egui::Shape::Rect(r) if r.stroke.color == couleur))
                .count()
        }
    }

    #[test]
    fn les_fleches_changent_d_un_pas_et_maj_d_un_grand_pas() {
        let mut banc = Banc::new(2.0, quantities::LINE_WIDTH);
        banc.focaliser();
        banc.image(
            vec![touche_avec(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert!((banc.valeur - 2.1).abs() < 1e-9, "{}", banc.valeur);
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        assert!((banc.valeur - 2.0).abs() < 1e-9);
        banc.image(
            vec![touche_avec(egui::Key::ArrowUp, egui::Modifiers::SHIFT)],
            false,
        );
        assert!((banc.valeur - 3.0).abs() < 1e-9, "{}", banc.valeur);
    }

    #[test]
    fn sans_focus_les_fleches_ne_font_rien() {
        let mut banc = Banc::new(2.0, quantities::LINE_WIDTH);
        banc.image(
            vec![touche_avec(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(banc.valeur, 2.0);
    }

    #[test]
    fn alt_bas_ouvre_la_liste_entree_choisit_et_echap_la_ferme() {
        let mut banc = Banc::new(1.4, quantities::LINE_WIDTH);
        banc.focaliser();
        assert!(!banc.peint("3"), "liste fermée : aucune valeur proposée");
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        assert!(banc.peint("3"), "liste ouverte");
        assert_eq!(banc.valeur, 1.4, "ouvrir ne change rien");
        // La valeur proposée la plus proche de 1,4 est 1,5 ; deux crans en bas.
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(banc.valeur, 1.4, "surligner ne change rien");
        let (_, changed) = banc.image(vec![touche(egui::Key::Enter)], false);
        assert!(changed);
        assert_eq!(banc.valeur, 3.0);
        assert!(!banc.peint("4"), "la liste est fermée après le choix");

        // Échap ferme la liste sans rendre ni changer la valeur.
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        assert!(banc.peint("4"));
        let (_, changed) = banc.image(vec![touche(egui::Key::Escape)], false);
        assert!(!changed);
        assert!(!banc.peint("4"), "Échap ferme la liste");
        assert_eq!(banc.valeur, 3.0);
    }

    #[test]
    fn une_grandeur_sans_valeurs_proposees_n_ouvre_pas_de_liste() {
        let mut banc = Banc::new(2.0, quantities::FREE);
        banc.focaliser();
        banc.image(
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        // Alt+↓ n'est pas un pas : la valeur ne bouge pas, rien ne s'ouvre.
        assert_eq!(banc.valeur, 2.0);
    }

    #[test]
    fn le_clic_selectionne_tout_le_texte() {
        let mut banc = Banc::new(12.5, quantities::LINE_WIDTH);
        banc.image(Vec::new(), false);
        let centre = egui::pos2(20.0, 12.0);
        for pressed in [None, Some(true), Some(false)] {
            let events = match pressed {
                None => vec![egui::Event::PointerMoved(centre)],
                Some(pressed) => vec![egui::Event::PointerButton {
                    pos: centre,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            };
            banc.image(events, false);
        }
        banc.image(Vec::new(), false);
        let etat =
            egui::TextEdit::load_state(&banc.ctx, banc.id.expect("id")).expect("état du champ");
        let plage = etat.cursor.char_range().expect("sélection");
        let (bas, haut) = (
            plage.primary.index.min(plage.secondary.index),
            plage.primary.index.max(plage.secondary.index),
        );
        assert_eq!((bas.0, haut.0), (0, 4), "« 12.5 » entier");
    }

    #[test]
    fn la_molette_suit_la_liste_au_survol_sans_focus() {
        let mut banc = Banc::new(2.0, quantities::LINE_WIDTH);
        let centre = egui::pos2(20.0, 12.0);
        // Curseur ailleurs : la molette reste à la fenêtre.
        banc.image(
            vec![egui::Event::PointerMoved(egui::pos2(300.0, 250.0))],
            false,
        );
        banc.image(vec![cran(1.0)], false);
        assert_eq!(banc.valeur, 2.0);
        // Survol, sans focus : un cran = l'entrée voisine (2 → 3, puis 1,5).
        banc.image(vec![egui::Event::PointerMoved(centre)], false);
        banc.image(vec![cran(1.0)], false);
        assert_eq!(banc.valeur, 3.0);
        banc.image(vec![cran(-2.0)], false);
        assert_eq!(banc.valeur, 1.5);
    }

    #[test]
    fn step_preset_saute_hors_liste_et_s_arrete_aux_extremites() {
        let q = quantities::LINE_WIDTH; // 0,5 · 1 · 1,5 · 2 · 3 · 4
        assert_eq!(step_preset(&q, 2.3, 1), 3.0);
        assert_eq!(step_preset(&q, 2.3, -1), 2.0);
        assert_eq!(step_preset(&q, 4.0, 1), 4.0);
        assert_eq!(step_preset(&q, 0.5, -3), 0.5);
        assert_eq!(step_preset(&q, 7.0, 1), 7.0);
        assert_eq!(step_preset(&q, 7.0, -1), 4.0);
        assert_eq!(step_preset(&q, 1.0, 3), 3.0);
    }

    #[test]
    fn un_champ_sans_liste_ignore_la_molette() {
        let mut banc = Banc::new(2.0, quantities::FREE);
        banc.image(
            vec![egui::Event::PointerMoved(egui::pos2(20.0, 12.0))],
            false,
        );
        banc.image(vec![cran(1.0)], false);
        assert_eq!(banc.valeur, 2.0);
        banc.focaliser();
        let (_, changed) = banc.image(vec![cran(1.0)], false);
        assert!(!changed);
        assert_eq!(banc.valeur, 2.0);
    }

    #[test]
    fn la_fleche_pleine_est_dans_le_cadre_du_champ_a_liste() {
        let mut banc = Banc::new(2.0, quantities::LINE_WIDTH);
        let (sortie, _) = banc.image(Vec::new(), false);
        let champ = banc.champ.expect("rect du champ");
        let fleche = sortie.shapes.iter().find_map(|peint| match &peint.shape {
            egui::Shape::Path(path) if path.fill != egui::Color32::TRANSPARENT => {
                Some(egui::Rect::from_points(&path.points))
            }
            _ => None,
        });
        let fleche = fleche.expect("triangle plein");
        assert!(champ.contains_rect(fleche), "{fleche:?} hors de {champ:?}");
    }

    #[test]
    fn une_saisie_hors_borne_est_ramenee_et_une_illisible_refusee() {
        let mut banc = Banc::new(2.0, quantities::LINE_WIDTH);
        banc.focaliser();
        banc.image(vec![egui::Event::Text("35".into())], false);
        let (_, changed) = banc.image(vec![touche(egui::Key::Enter)], false);
        assert!(changed);
        assert_eq!(banc.valeur, 20.0);
        let (sortie, _) = banc.image(Vec::new(), false);
        assert_eq!(
            Banc::cadres(&sortie, crate::theme::field_clamped_color()),
            1
        );

        banc.focaliser();
        banc.image(vec![egui::Event::Text("abc".into())], false);
        let (sortie, _) = banc.image(Vec::new(), false);
        assert_eq!(
            Banc::cadres(&sortie, crate::theme::field_refused_color()),
            1
        );
        assert_eq!(banc.valeur, 20.0);
    }

    #[test]
    fn un_nombre_entier_ecrit_un_entier() {
        let mut banc = Banc::new(3.0, quantities::POLYNOMIAL_DEGREE);
        banc.focaliser();
        banc.image(
            vec![touche_avec(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(banc.valeur, 4.0);
        assert!(banc.peint("4"));
    }
}
