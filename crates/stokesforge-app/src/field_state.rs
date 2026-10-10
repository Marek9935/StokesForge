//! État d'un champ de saisie : **refusé** (cadre rouge) ou **ramené** (cadre
//! orange), une seule brique pour tous les champs (#219, #217 N6/N7/N11, T2, T3).
//!
//! Règles (`docs/standard-interface.md`, § Saisie refusée ou ramenée) :
//!
//! - une saisie **illisible**, ou qui viole une contrainte entre deux champs,
//!   est *refusée* : cadre rouge, bulle qui dit la règle, texte gardé tant que
//!   le champ reste ouvert, ancienne valeur rendue à la sortie ;
//! - une valeur **hors d'une borne fixe** est *ramenée* à la borne : cadre
//!   orange pendant [`CLAMPED_SECONDS`], bulle « Set to the maximum, 20 » ;
//! - jamais d'effacement ni de correction muette.
//!
//! # Trois étages, du plus bas au plus haut
//!
//! 1. [`show_issue`] — peint cadre et bulle d'un [`FieldIssue`] sur la réponse
//!    d'un champ **déjà posé**. Sans mémoire : c'est l'étage d'un champ dont
//!    l'erreur vient d'ailleurs (message Typst d'un titre, T3).
//! 2. [`flag`] / [`active`] / [`clear`] — la mémoire d'egui (par identifiant de
//!    champ) d'un état qui doit survivre à la saisie : le cadre orange de 2 s,
//!    le cadre rouge d'une saisie abandonnée.
//! 3. [`edit_text`] — le cycle complet d'un champ texte validé (brouillon,
//!    Entrée, perte du focus, Échap) autour d'une fonction de lecture
//!    ([`Verdict`]). Les briques « nombre » (#220) et « texte » (#224) sont
//!    des appels d'[`edit_text`] avec leur lecture ; la décision du cycle,
//!    [`decide`], est une fonction pure testée sans fenêtre.
//!
//! Le rouge et l'orange viennent de `theme.rs` (`field_refused_color`,
//! `field_clamped_color`) ; les messages sont des clés `tr!` du catalogue.

use eframe::egui;

use crate::theme;
use crate::tr;

/// Durée d'affichage du cadre orange d'une valeur ramenée, en secondes.
pub const CLAMPED_SECONDS: f64 = 2.0;

/// Épaisseur du cadre d'un champ en défaut, en points.
const FRAME_WIDTH: f32 = 1.5;

/// Quelle borne a ramené la valeur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    Min,
    Max,
}

/// Ce qui ne va pas dans un champ, avec la phrase déjà traduite de la bulle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldIssue {
    /// Saisie refusée : cadre rouge. `message` dit la règle.
    Refused { message: String },
    /// Valeur ramenée à une borne : cadre orange. `message` dit laquelle.
    Clamped { message: String },
}

impl FieldIssue {
    /// Une saisie refusée ; `message` est une phrase `tr!` qui dit la règle.
    pub fn refused(message: impl Into<String>) -> Self {
        Self::Refused {
            message: message.into(),
        }
    }

    /// Une valeur ramenée à `bound`, dont `limit` est la valeur affichée
    /// (« 20 » dans « Set to the maximum, 20 »).
    pub fn clamped(bound: Bound, limit: &str) -> Self {
        let message = match bound {
            Bound::Min => tr!("field-clamped-min", limit = limit),
            Bound::Max => tr!("field-clamped-max", limit = limit),
        };
        Self::Clamped { message }
    }

    /// La phrase de la bulle.
    pub fn message(&self) -> &str {
        match self {
            Self::Refused { message } | Self::Clamped { message } => message,
        }
    }

    /// La couleur du cadre : rouge pour un refus, orange pour une valeur ramenée.
    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Refused { .. } => theme::field_refused_color(),
            Self::Clamped { .. } => theme::field_clamped_color(),
        }
    }
}

/// Peint le cadre de `issue` autour du champ `response`, et sa bulle tant que le
/// champ est survolé ou a le focus.
pub fn show_issue(ui: &egui::Ui, response: &egui::Response, issue: &FieldIssue) {
    let corner = ui.visuals().widgets.inactive.corner_radius;
    ui.painter().rect_stroke(
        response.rect.expand(1.0),
        corner,
        egui::Stroke::new(FRAME_WIDTH, issue.color()),
        egui::StrokeKind::Outside,
    );
    if response.hovered() || response.has_focus() {
        response.show_tooltip_text(issue.message());
    }
}

/// L'état gardé en mémoire pour un champ : ce qu'il faut encore montrer, et
/// jusqu'à quand (temps d'egui).
#[derive(Clone, Debug)]
struct Flag {
    issue: FieldIssue,
    until: f64,
}

/// Mémorise `issue` pour le champ `id` pendant [`CLAMPED_SECONDS`] à partir de
/// `now`, et réveille l'interface à son échéance.
pub fn flag(ctx: &egui::Context, id: egui::Id, issue: FieldIssue, now: f64) {
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            Flag {
                issue,
                until: now + CLAMPED_SECONDS,
            },
        );
    });
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(CLAMPED_SECONDS));
}

/// L'état encore à montrer pour le champ `id` à l'instant `now`. Un état échu
/// est oublié ; sinon l'interface est réveillée à son échéance, sans quoi le
/// cadre resterait peint jusqu'au prochain événement.
pub fn active(ctx: &egui::Context, id: egui::Id, now: f64) -> Option<FieldIssue> {
    let flag = ctx.data(|data| data.get_temp::<Flag>(id))?;
    if now >= flag.until {
        clear(ctx, id);
        return None;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(flag.until - now));
    Some(flag.issue)
}

/// Oublie l'état mémorisé du champ `id`.
pub fn clear(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| data.remove::<Flag>(id));
}

/// Ce que la lecture d'un texte saisi a décidé.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict<T> {
    /// Lisible et dans les bornes.
    Valid(T),
    /// Lisible mais hors d'une borne fixe : `value` est la valeur ramenée.
    Clamped {
        value: T,
        bound: Bound,
        /// La borne telle qu'elle s'affiche dans la bulle.
        limit: String,
    },
    /// Illisible ou contraire à une contrainte : `message` dit la règle.
    Refused(String),
}

/// Ce que le cycle d'un champ fait d'une image.
#[derive(Clone, Debug, PartialEq)]
pub enum Step<T> {
    /// Rien à valider : la saisie continue, ou le texte n'a pas changé.
    Keep,
    /// Échap, ou perte du focus sans changement : l'ancienne valeur revient.
    Revert,
    /// Valeur validée.
    Commit(T),
    /// Valeur validée après avoir été ramenée : cadre orange.
    CommitClamped(T, FieldIssue),
    /// Saisie refusée. À Entrée le champ reste ouvert avec son texte ; à la
    /// perte du focus l'ancienne valeur revient, le refus restant signalé.
    Reject { issue: FieldIssue, stay_open: bool },
}

/// Le cycle d'un champ, sans egui : `draft` est le texte saisi, `current` la
/// valeur affichée avant la saisie, `lost_focus` la sortie du champ (Entrée
/// comprise, `enter` la distingue d'un clic ailleurs), `escape` Échap.
pub fn decide<T>(
    draft: &str,
    current: &str,
    enter: bool,
    escape: bool,
    lost_focus: bool,
    parse: impl FnOnce(&str) -> Verdict<T>,
) -> Step<T> {
    if escape {
        return Step::Revert;
    }
    if !lost_focus {
        return Step::Keep;
    }
    if draft == current {
        return Step::Revert;
    }
    match parse(draft) {
        Verdict::Valid(value) => Step::Commit(value),
        Verdict::Clamped {
            value,
            bound,
            limit,
        } => Step::CommitClamped(value, FieldIssue::clamped(bound, &limit)),
        Verdict::Refused(message) => Step::Reject {
            issue: FieldIssue::refused(message),
            stay_open: enter,
        },
    }
}

/// Le brouillon d'un champ pendant la saisie.
#[derive(Clone, Debug)]
struct Draft(String);

/// Un champ texte d'une ligne au cycle du standard : `current` est la valeur
/// affichée hors saisie, `parse` la lit. Rend la valeur lue quand elle est
/// validée (Entrée ou sortie du champ), `None` sinon.
///
/// - la saisie est un brouillon gardé en mémoire d'egui sous `id_salt` ; rien
///   n'en sort avant la validation ;
/// - un brouillon illisible peint le cadre rouge dès la frappe, avec la règle
///   en bulle ;
/// - Échap rend l'ancienne valeur ; la sortie sans changement aussi.
pub fn edit_text<T>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    parse: impl Fn(&str) -> Verdict<T>,
) -> Option<T> {
    let id = ui.make_persistent_id(id_salt);
    let ctx = ui.ctx().clone();
    let now = ui.input(|input| input.time);

    let draft = ctx.data(|data| data.get_temp::<Draft>(id));
    // Une sortie du champ ne vaut que pour une saisie ouverte : egui peut
    // annoncer la perte du focus sur deux images de suite, et la seconde ne
    // doit ni revalider ni effacer l'état qu'a posé la première.
    let editing = draft.is_some();
    let mut buffer = draft.map_or_else(|| current.to_owned(), |draft| draft.0);
    let response = ui.add(egui::TextEdit::singleline(&mut buffer).id(id));
    let focused = response.has_focus();

    // egui rend lui-même le focus sur Échap : le champ « a eu » le focus aussi
    // quand il vient de le perdre.
    let escape = editing
        && (focused || response.lost_focus())
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let left = response.lost_focus() && editing;
    let enter = left && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let step = decide(&buffer, current, enter, escape, left || escape, &parse);

    // La frappe a déjà été peinte avec le brouillon : une étape qui le rend
    // n'est visible qu'à l'image suivante, qu'il faut donc demander.
    if !matches!(step, Step::Keep) {
        ctx.request_repaint();
    }
    let mut committed = None;
    match step {
        Step::Keep => {
            if focused || response.changed() {
                ctx.data_mut(|data| data.insert_temp(id, Draft(buffer.clone())));
            }
        }
        Step::Revert => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            clear(&ctx, id);
            if escape {
                response.surrender_focus();
            }
        }
        Step::Commit(value) => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            clear(&ctx, id);
            committed = Some(value);
        }
        Step::CommitClamped(value, issue) => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            flag(&ctx, id, issue, now);
            committed = Some(value);
        }
        Step::Reject { issue, stay_open } => {
            if stay_open {
                ctx.data_mut(|data| data.insert_temp(id, Draft(buffer.clone())));
                response.request_focus();
            } else {
                ctx.data_mut(|data| data.remove::<Draft>(id));
                flag(&ctx, id, issue, now);
            }
        }
    }

    // Ce qui se montre cette image : un brouillon illisible tant que le champ
    // est ouvert (la règle s'apprend à la frappe), sinon l'état mémorisé.
    let live = (focused && buffer != current)
        .then(|| match parse(&buffer) {
            Verdict::Refused(message) => Some(FieldIssue::refused(message)),
            _ => None,
        })
        .flatten();
    if let Some(issue) = live.or_else(|| active(&ctx, id, now)) {
        show_issue(ui, &response, &issue);
    }
    committed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un contexte au thème de l'application (les polices de l'interface n'y
    /// changent rien : on lit des couleurs et des textes, pas des positions).
    fn contexte() -> egui::Context {
        let context = egui::Context::default();
        theme::install(&context);
        context
    }

    fn touche(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Un texte contenant `needle` a-t-il été peint ?
    fn painted(output: &egui::FullOutput, needle: &str) -> bool {
        output.shapes.iter().any(|peint| {
            matches!(&peint.shape,
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
        })
    }

    fn lire(texte: &str) -> Verdict<f64> {
        match texte.trim().parse::<f64>() {
            Ok(v) if v > 20.0 => Verdict::Clamped {
                value: 20.0,
                bound: Bound::Max,
                limit: "20".into(),
            },
            Ok(v) => Verdict::Valid(v),
            Err(_) => Verdict::Refused("Enter a number".into()),
        }
    }

    #[test]
    fn la_saisie_en_cours_n_est_pas_lue() {
        let pas = decide("abc", "1", false, false, false, lire);
        assert_eq!(pas, Step::Keep);
    }

    #[test]
    fn echap_rend_l_ancienne_valeur_meme_pour_un_texte_illisible() {
        assert_eq!(decide("abc", "1", false, true, false, lire), Step::Revert);
    }

    #[test]
    fn une_sortie_sans_changement_ne_valide_rien() {
        assert_eq!(decide("1", "1", false, false, true, lire), Step::Revert);
    }

    #[test]
    fn un_nombre_lisible_est_valide() {
        assert_eq!(decide("5", "1", true, false, true, lire), Step::Commit(5.0));
    }

    #[test]
    fn hors_borne_la_valeur_est_ramenee_avec_sa_bulle() {
        let Step::CommitClamped(valeur, issue) = decide("99", "1", true, false, true, lire) else {
            panic!("une valeur hors borne est ramenée");
        };
        assert_eq!(valeur, 20.0);
        assert_eq!(issue, FieldIssue::clamped(Bound::Max, "20"));
        assert!(issue.message().contains("20"), "{}", issue.message());
    }

    /// Entrée sur un texte illisible garde le champ ouvert ; un clic ailleurs
    /// rend l'ancienne valeur. Dans les deux cas le refus est signalé.
    #[test]
    fn un_texte_illisible_est_refuse_ouvert_a_entree_et_ferme_a_la_sortie() {
        let attendu = FieldIssue::refused("Enter a number");
        assert_eq!(
            decide("abc", "1", true, false, true, lire),
            Step::Reject {
                issue: attendu.clone(),
                stay_open: true
            }
        );
        assert_eq!(
            decide("abc", "1", false, false, true, lire),
            Step::Reject {
                issue: attendu,
                stay_open: false
            }
        );
    }

    #[test]
    fn le_rouge_et_l_orange_sont_distincts() {
        assert_ne!(
            FieldIssue::refused("x").color(),
            FieldIssue::clamped(Bound::Min, "0").color()
        );
    }

    #[test]
    fn un_etat_memorise_s_eteint_apres_deux_secondes() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("champ");
        flag(&ctx, id, FieldIssue::clamped(Bound::Max, "20"), 10.0);
        assert!(active(&ctx, id, 11.9).is_some());
        assert!(active(&ctx, id, 12.0).is_none());
        assert!(active(&ctx, id, 12.1).is_none(), "l'oubli est définitif");
    }

    // — Les formes peintes : le cadre et la bulle se lisent dans les shapes. —

    /// Le nombre de cadres de couleur `couleur` peints dans `shapes`.
    fn cadres(output: &egui::FullOutput, couleur: egui::Color32) -> usize {
        output
            .shapes
            .iter()
            .filter(|peint| match &peint.shape {
                egui::Shape::Rect(rect) => rect.stroke.color == couleur,
                _ => false,
            })
            .count()
    }

    /// Une image à l'instant `temps` où `evenements` arrivent ; le champ est
    /// l'unique contenu, valeur affichée « 1 ».
    fn image(
        ctx: &egui::Context,
        temps: f64,
        evenements: Vec<egui::Event>,
        sortie: &mut Option<f64>,
    ) -> egui::FullOutput {
        image_avec(ctx, temps, evenements, sortie, None)
    }

    /// Comme [`image`], et `focus` donne (`Some(true)`) ou retire (`Some(false)`)
    /// le focus au champ avant sa pose.
    fn image_avec(
        ctx: &egui::Context,
        temps: f64,
        evenements: Vec<egui::Event>,
        sortie: &mut Option<f64>,
        focus: Option<bool>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            events: evenements,
            time: Some(temps),
            ..Default::default()
        };
        input.focused = true;
        let mut output = ctx.run_ui(input, |ui| {
            let id = ui.make_persistent_id("champ");
            match focus {
                Some(true) => ui.memory_mut(|m| m.request_focus(id)),
                Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
                None => {}
            }
            if let Some(valeur) = edit_text(ui, "champ", "1", lire) {
                *sortie = Some(valeur);
            }
        });
        output.textures_delta.clear();
        output
    }

    fn taper(texte: &str) -> egui::Event {
        egui::Event::Text(texte.into())
    }

    /// Donne le focus au champ en le demandant dans une image, puis tape.
    fn ouvrir_et_taper(ctx: &egui::Context, texte: &str, sortie: &mut Option<f64>) {
        image_avec(ctx, 0.0, vec![], sortie, Some(true));
        image(ctx, 0.1, vec![], sortie);
        image(ctx, 0.2, vec![taper(texte)], sortie);
    }

    #[test]
    fn un_brouillon_illisible_peint_le_cadre_rouge_et_la_regle() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "abc", &mut sortie);
        let sortie_image = image(&ctx, 0.3, vec![], &mut sortie);
        assert_eq!(cadres(&sortie_image, theme::field_refused_color()), 1);
        assert!(painted(&sortie_image, "Enter a number"));
        assert_eq!(sortie, None, "rien ne sort avant la validation");
    }

    /// Témoin : une saisie valide ne peint aucun cadre de défaut.
    #[test]
    fn une_saisie_valide_ne_peint_aucun_cadre() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "5", &mut sortie);
        let sortie_image = image(&ctx, 0.3, vec![], &mut sortie);
        assert_eq!(cadres(&sortie_image, theme::field_refused_color()), 0);
        assert_eq!(cadres(&sortie_image, theme::field_clamped_color()), 0);
    }

    #[test]
    fn entree_valide_un_nombre_ramene_et_le_cadre_orange_s_eteint() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "9", &mut sortie); // « 19 » : valide
        image(&ctx, 0.3, vec![taper("9")], &mut sortie); // « 199 » : hors borne
        image(&ctx, 0.4, vec![touche(egui::Key::Enter)], &mut sortie);
        assert_eq!(sortie, Some(20.0), "ramené à la borne");

        let avant = image(&ctx, 1.0, vec![], &mut sortie);
        assert_eq!(cadres(&avant, theme::field_clamped_color()), 1);
        let apres = image(&ctx, 3.0, vec![], &mut sortie);
        assert_eq!(cadres(&apres, theme::field_clamped_color()), 0);
    }

    #[test]
    fn la_sortie_d_un_texte_illisible_rend_l_ancienne_valeur() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "abc", &mut sortie);
        // Le focus part ailleurs sans Entrée.
        image_avec(&ctx, 0.4, vec![], &mut sortie, Some(false));
        let apres = image(&ctx, 0.5, vec![], &mut sortie);
        assert_eq!(sortie, None);
        assert!(!painted(&apres, "abc"), "le brouillon est rendu");
        assert_eq!(
            cadres(&apres, theme::field_refused_color()),
            1,
            "le refus reste signalé un instant"
        );
        let plus_tard = image(&ctx, 3.0, vec![], &mut sortie);
        assert_eq!(cadres(&plus_tard, theme::field_refused_color()), 0);
    }

    /// Entrée sur un texte illisible : le champ reste ouvert, le texte gardé,
    /// le cadre rouge peint (T2) ; rien ne sort.
    #[test]
    fn entree_sur_un_texte_illisible_garde_le_champ_ouvert() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "abc", &mut sortie);
        image(&ctx, 0.3, vec![touche(egui::Key::Enter)], &mut sortie);
        let apres = image(&ctx, 0.4, vec![], &mut sortie);
        let encore = image(&ctx, 0.5, vec![], &mut sortie);
        assert_eq!(sortie, None);
        assert!(painted(&apres, "abc") && painted(&encore, "abc"));
        assert_eq!(cadres(&encore, theme::field_refused_color()), 1);
    }

    #[test]
    fn echap_rend_l_ancienne_valeur_sans_cadre() {
        let ctx = contexte();
        let mut sortie = None;
        ouvrir_et_taper(&ctx, "abc", &mut sortie);
        image(&ctx, 0.3, vec![touche(egui::Key::Escape)], &mut sortie);
        let apres = image(&ctx, 0.4, vec![], &mut sortie);
        assert!(!painted(&apres, "abc"));
        assert_eq!(cadres(&apres, theme::field_refused_color()), 0);
    }
}
