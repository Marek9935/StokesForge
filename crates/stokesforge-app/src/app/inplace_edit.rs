//! **Le** contrat de l'édition sur place (#224, `docs/standard-interface.md`,
//! § Text fields and in-place editing, S-T4) : grille, onglets de feuille,
//! Project Manager, Object Manager et titres du graphe l'appellent tous.
//!
//! Le contrat, identique partout :
//!
//! - à l'ouverture le champ prend le focus et **tout le texte est
//!   sélectionné** (sauf quand la frappe qui l'ouvre est déjà son contenu :
//!   la grille, [`Opening::KeepCursor`]) ;
//! - Entrée et un clic ailleurs **valident**, Échap **annule**, Tab **valide et
//!   passe au suivant** (Maj+Tab au précédent) ;
//! - une saisie refusée ([`Resolution::Stay`]) garde le champ ouvert et son
//!   texte, cadre rouge de `field_state` ; un clic ailleurs sur une saisie
//!   refusée rend l'ancienne valeur.
//!
//! La brique ne possède ni le tampon ni le modèle : l'appelant garde son
//! `String` et son état, pose sa mise en page (la brique ne crée aucun `Ui`,
//! donc ne change aucun identifiant egui) et traduit [`Resolution`] en
//! action. Entrée, Tab et Échap sont **consommés** ici : aucun raccourci
//! plus bas dans l'image ne les revoit (le piège du double déplacement de
//! la grille, #38).
//!
//! Pourquoi une fonction pure ([`resolve`]) : la décision « valider, rester,
//! rendre l'ancienne valeur » est testée sans fenêtre ; [`show`] n'est que la
//! lecture des touches et du focus.

use eframe::egui;

use crate::field_state::{show_issue, FieldIssue};

/// Les touches que le champ garde pour lui : Tab ne doit pas faire sortir le
/// focus vers le reste de l'interface (le champ le lit, #38). Échap reste à
/// egui, qui rend le focus : [`show`] le lit avant.
pub(super) const KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// Comment s'ouvre le champ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    /// Tout le texte est sélectionné : taper le remplace.
    SelectAll,
    /// Le curseur reste où egui l'a mis (à la fin) : le texte vient d'être
    /// tapé, il ne faut pas le sélectionner pour le remplacer.
    KeepCursor,
}

/// Ce que fait la perte du focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusLoss {
    /// Un clic ailleurs termine la saisie ([`EditEnd::Elsewhere`]) : le cas
    /// général.
    Ends,
    /// Le champ peut perdre le focus sans que la saisie finisse : un éditeur
    /// qui porte d'autres réglages (taille, rotation, couleur) et décide lui-même
    /// de ce qu'est un « ailleurs » (titres du graphe, textes libres). Le champ
    /// ne reprend pas le focus de force.
    Ignored,
}

/// Ce qui a terminé la saisie cette image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditEnd {
    /// Entrée ; `shift` si Maj était tenue (la grille remonte d'une ligne).
    Enter { shift: bool },
    /// Tab ; `shift` pour Maj+Tab (le champ précédent).
    Tab { shift: bool },
    /// Un clic ailleurs, ou le focus pris par un autre champ.
    Elsewhere,
    /// Échap.
    Cancel,
}

/// Ce que l'appelant fait de la fin de saisie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Resolution {
    /// Le champ reste ouvert avec son texte (Entrée ou Tab sur une saisie
    /// refusée).
    Stay,
    /// Le champ se ferme. `write` : le texte est à appliquer (il a changé et
    /// il est lisible) ; sinon l'ancienne valeur revient. `end` dit comment
    /// continuer (Tab passe au suivant, Entrée avance dans la grille).
    Close { write: bool, end: EditEnd },
}

/// La décision du cycle, sans egui. `unchanged` : le texte vaut encore
/// l'original ; `refused` : l'appelant refuse le texte (nom vide, déjà pris…).
pub(super) fn resolve(end: EditEnd, unchanged: bool, refused: bool) -> Resolution {
    match end {
        EditEnd::Cancel => Resolution::Close { write: false, end },
        EditEnd::Enter { .. } | EditEnd::Tab { .. } if refused => Resolution::Stay,
        EditEnd::Elsewhere if refused => Resolution::Close { write: false, end },
        _ => Resolution::Close {
            write: !unchanged,
            end,
        },
    }
}

/// Ce que rend [`show`].
pub(super) struct Shown {
    pub(super) response: egui::Response,
    /// La fin de saisie de cette image, s'il y en a une.
    pub(super) end: Option<EditEnd>,
}

/// Dernière image où le champ `id` a été peint : de quoi reconnaître son
/// ouverture (le `TextEdit` d'un identifiant stable garde son curseur d'une
/// édition à l'autre, il ne dit donc pas s'il est neuf).
#[derive(Clone, Copy)]
struct Seen(u64);

/// L'ouverture du champ `id` est-elle cette image ? À appeler **avant** de
/// peindre le `TextEdit`, une fois par image et par champ.
pub(super) fn begin(ctx: &egui::Context, id: egui::Id) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let seen_id = id.with("inplace_seen");
    let fresh = ctx
        .data(|data| data.get_temp::<Seen>(seen_id))
        .is_none_or(|Seen(last)| pass.saturating_sub(last) > 2);
    ctx.data_mut(|data| data.insert_temp(seen_id, Seen(pass)));
    fresh
}

/// Lit la fin de saisie d'un champ déjà peint (`response`) : focus et
/// sélection à l'ouverture, Entrée, Tab, Échap, perte du focus. `fresh` vient
/// de [`begin`], le `TextEdit` portait [`KEYS`] et l'identifiant `response.id`.
/// `refusal` : la phrase qui dit pourquoi le texte actuel est refusé, peinte en
/// cadre rouge.
pub(super) fn finish(
    ui: &egui::Ui,
    response: &egui::Response,
    fresh: bool,
    opening: Opening,
    loss: FocusLoss,
    refusal: Option<&str>,
) -> Option<EditEnd> {
    let ctx = ui.ctx().clone();
    let id = response.id;
    // Lu avant toute demande de focus : redemander le focus effacerait la perte
    // que signale Entrée (le piège de `cell_widget`).
    let lost = response.lost_focus();
    if fresh {
        response.request_focus();
        if opening == Opening::SelectAll {
            if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                let end = state_text_len(&ctx, id);
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(end),
                    )));
                state.store(&ctx, id);
            }
        }
    }
    if let Some(message) = refusal {
        show_issue(ui, response, &FieldIssue::refused(message));
    }

    let focused = response.has_focus();
    let (escape, enter, tab, shift) = ctx.input(|input| {
        (
            input.key_pressed(egui::Key::Escape),
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Tab),
            input.modifiers.shift,
        )
    });
    let consume = |key: &egui::KeyboardShortcut| {
        ctx.input_mut(|input| input.consume_shortcut(key));
    };
    let end = if !fresh && (focused || lost) && escape {
        consume(&super::shortcuts::KEY_ESCAPE);
        Some(EditEnd::Cancel)
    } else if !fresh && focused && tab {
        consume(&super::shortcuts::KEY_TAB);
        Some(EditEnd::Tab { shift })
    } else if !fresh && lost && (enter || loss == FocusLoss::Ends) {
        if enter {
            consume(&super::shortcuts::KEY_ENTER);
            Some(EditEnd::Enter { shift })
        } else {
            Some(EditEnd::Elsewhere)
        }
    } else {
        // Un champ ouvert sans focus (après un changement de fenêtre) le
        // reprend : il n'y a pas d'état « ouvert mais inerte ».
        if !fresh && !focused && !lost && loss == FocusLoss::Ends {
            response.request_focus();
        }
        None
    };
    if end.is_some() {
        response.surrender_focus();
        ctx.data_mut(|data| data.remove::<Seen>(id.with("inplace_seen")));
        ctx.request_repaint();
    }
    end
}

/// Le texte du champ, mesuré en caractères, pour tout sélectionner.
fn state_text_len(ctx: &egui::Context, id: egui::Id) -> usize {
    ctx.data(|data| data.get_temp::<TextLen>(id.with("inplace_len")))
        .map_or(0, |TextLen(len)| len)
}

/// Longueur du texte vue par [`show`] ou [`measured`] cette image.
#[derive(Clone, Copy)]
struct TextLen(usize);

/// Retient la longueur du texte d'un champ posé hors de [`show`] (avant
/// [`finish`], qui sélectionne tout à l'ouverture).
pub(super) fn measured(ctx: &egui::Context, id: egui::Id, text: &str) {
    ctx.data_mut(|data| data.insert_temp(id.with("inplace_len"), TextLen(text.chars().count())));
}

/// Peint le champ et lit sa fin de saisie. `edit` est le `TextEdit` de
/// l'appelant (largeur, police, cadre, texte d'aide) : la brique lui pose
/// seulement son identifiant et le filtre de touches. Pour un champ posé par
/// `ui.put` ou dans une mise en page particulière : [`begin`] puis [`finish`].
pub(super) fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    edit: egui::TextEdit<'_>,
    opening: Opening,
    loss: FocusLoss,
    refusal: Option<&str>,
) -> Shown {
    let fresh = begin(ui.ctx(), id);
    let output = edit.id(id).event_filter(KEYS).show(ui);
    measured(ui.ctx(), id, output.galley.text());
    let response = output.response.response;
    let end = finish(ui, &response, fresh, opening, loss, refusal);
    Shown { response, end }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{contexte, image, touche};
    use super::*;

    const ENTREE: EditEnd = EditEnd::Enter { shift: false };
    const TAB: EditEnd = EditEnd::Tab { shift: false };

    #[test]
    fn echap_annule_toujours() {
        for refused in [false, true] {
            assert_eq!(
                resolve(EditEnd::Cancel, false, refused),
                Resolution::Close {
                    write: false,
                    end: EditEnd::Cancel
                }
            );
        }
    }

    #[test]
    fn entree_et_tab_valident_ce_qui_a_change() {
        for end in [ENTREE, TAB] {
            assert_eq!(
                resolve(end, false, false),
                Resolution::Close { write: true, end }
            );
            // Sans changement, rien n'est écrit mais le geste continue (Tab
            // passe au suivant).
            assert_eq!(
                resolve(end, true, false),
                Resolution::Close { write: false, end }
            );
        }
    }

    #[test]
    fn entree_et_tab_sur_une_saisie_refusee_gardent_le_champ_ouvert() {
        for end in [ENTREE, TAB] {
            assert_eq!(resolve(end, false, true), Resolution::Stay);
        }
    }

    #[test]
    fn un_clic_ailleurs_valide_ou_rend_l_ancienne_valeur_si_refuse() {
        assert_eq!(
            resolve(EditEnd::Elsewhere, false, false),
            Resolution::Close {
                write: true,
                end: EditEnd::Elsewhere
            }
        );
        assert_eq!(
            resolve(EditEnd::Elsewhere, false, true),
            Resolution::Close {
                write: false,
                end: EditEnd::Elsewhere
            }
        );
    }

    /// Un champ ouvert sur « Spectre », piloté image par image.
    struct Banc {
        context: egui::Context,
        texte: String,
        fins: Vec<EditEnd>,
        opening: Opening,
        refusal: Option<String>,
    }

    impl Banc {
        fn nouveau(opening: Opening) -> Self {
            Self {
                context: contexte(),
                texte: "Spectre".to_string(),
                fins: Vec::new(),
                opening,
                refusal: None,
            }
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            let Self {
                context,
                texte,
                fins,
                opening,
                refusal,
            } = self;
            image(context, events, |ui| {
                let shown = show(
                    ui,
                    egui::Id::new("champ"),
                    egui::TextEdit::singleline(texte),
                    *opening,
                    FocusLoss::Ends,
                    refusal.as_deref(),
                );
                fins.extend(shown.end);
            });
        }

        fn ouvrir(&mut self) {
            self.image(Vec::new());
            self.image(Vec::new());
        }
    }

    #[test]
    fn l_ouverture_selectionne_tout_et_la_frappe_remplace() {
        let mut banc = Banc::nouveau(Opening::SelectAll);
        banc.ouvrir();
        banc.image(vec![egui::Event::Text("Neuf".to_string())]);
        assert_eq!(banc.texte, "Neuf");
        assert!(banc.fins.is_empty());
    }

    #[test]
    fn garder_le_curseur_n_efface_pas_le_texte_tape() {
        let mut banc = Banc::nouveau(Opening::KeepCursor);
        banc.ouvrir();
        banc.image(vec![egui::Event::Text("!".to_string())]);
        assert_eq!(banc.texte, "Spectre!");
    }

    #[test]
    fn entree_valide_et_est_consommee() {
        let mut banc = Banc::nouveau(Opening::SelectAll);
        banc.ouvrir();
        banc.image(vec![touche(egui::Key::Enter)]);
        assert_eq!(banc.fins, [ENTREE]);
    }

    #[test]
    fn echap_annule() {
        let mut banc = Banc::nouveau(Opening::SelectAll);
        banc.ouvrir();
        banc.image(vec![touche(egui::Key::Escape)]);
        assert_eq!(banc.fins, [EditEnd::Cancel]);
    }

    #[test]
    fn tab_valide_et_passe_au_suivant_sans_quitter_le_focus_par_egui() {
        let mut banc = Banc::nouveau(Opening::SelectAll);
        banc.ouvrir();
        banc.image(vec![touche(egui::Key::Tab)]);
        assert_eq!(banc.fins, [TAB]);
    }

    #[test]
    fn un_champ_refuse_se_peint_en_rouge() {
        let mut banc = Banc::nouveau(Opening::SelectAll);
        banc.refusal = Some("Name already used".to_string());
        banc.ouvrir();
        let rouge = crate::theme::field_refused_color();
        let mut peint = false;
        let context = banc.context.clone();
        let Banc {
            texte,
            opening,
            refusal,
            ..
        } = &mut banc;
        let formes = image(&context, Vec::new(), |ui| {
            show(
                ui,
                egui::Id::new("champ"),
                egui::TextEdit::singleline(texte),
                *opening,
                FocusLoss::Ends,
                refusal.as_deref(),
            );
        });
        for forme in formes {
            if let egui::Shape::Rect(rect) = forme.shape {
                peint |= rect.stroke.color == rouge;
            }
        }
        assert!(peint, "un cadre rouge entoure le champ refusé");
    }
}
