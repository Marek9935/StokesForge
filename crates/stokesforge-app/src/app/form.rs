//! La brique « formulaire » (#227, `docs/standard-interface.md`, § Choix,
//! disposition) : une grille à **deux colonnes** — libellés à gauche, champs
//! alignés sur une même verticale, cases à cocher et notes dans la colonne des
//! champs (S-L1) — et le **choix exclusif** qui se peint en boutons radio pour
//! 2 ou 3 options, en liste déroulante dès 4 (S-C1), radios côte à côte si
//! chaque libellé tient en 3 mots, l'un sous l'autre sinon (S-C2).
//!
//! Pourquoi pas `egui::Grid` : une grille d'egui n'a pas de cellule qui
//! s'étend sur deux colonnes, donc un titre de section (S-L3) y élargirait la
//! colonne des libellés, et `add_space` y est interdit (assertion d'egui) — or
//! le titre de section pose son écart. La brique garde des rangées ordinaires
//! (`ui.horizontal`), où l'icône « ? » de `hover_help` se pose comme ailleurs,
//! et aligne la colonne des champs sur le libellé le plus large, mesuré à
//! l'image précédente et gardé dans la mémoire d'egui sous l'identifiant du
//! formulaire (comme `egui::Grid` le fait de ses colonnes). Quand cette
//! largeur change (première image, autre page), l'image est refaite
//! (`request_discard`) : on ne voit jamais un formulaire désaligné.
//!
//! L'ordre de Tab suit l'ordre de lecture (S-K6) parce que les rangées se
//! peignent de haut en bas et, dans une rangée, de gauche à droite : un
//! formulaire n'a pas à le régler.
//!
//! Pas de section repliable dans un formulaire (S-L5) : trop long, il passe en
//! onglets (`tab_strip`). Les largeurs de champ sont celles de
//! [`theme::FieldWidth`] (S-L2) ; les écarts, [`theme::SPACE_ITEM`] et
//! [`theme::SPACE_SECTION`] (S-L4).

use std::hash::Hash;

use eframe::egui;

use crate::color_picker::{color_button, ColorUsage, PickerEnv};
use crate::theme;

use super::hover_help::help_beside;

/// Un formulaire en cours de peinture : voir [`form`].
pub(super) struct Form<'a> {
    ui: &'a mut egui::Ui,
    /// Largeur de la colonne des libellés retenue pour cette image (le plus
    /// large de l'image précédente).
    label_width: f32,
    /// Le libellé le plus large peint dans cette image.
    widest: f32,
}

/// Peint un formulaire en grille à deux colonnes (S-L1) : `add` pose ses
/// rangées par [`Form::row`], [`Form::field`], [`Form::note`] et
/// [`Form::section`]. `id_salt` distingue deux formulaires d'un même `Ui`.
pub(super) fn form<R>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Form<'_>) -> R,
) -> R {
    let id = ui.make_persistent_id(("stokesforge_form", id_salt));
    let label_width = ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0);
    let (inner, widest) = {
        let mut form = Form {
            ui: &mut *ui,
            label_width,
            widest: 0.0,
        };
        let inner = add(&mut form);
        (inner, form.widest)
    };
    if (widest - label_width).abs() > 0.5 {
        ui.data_mut(|data| data.insert_temp(id, widest));
        ui.ctx()
            .request_discard("colonne des libellés d'un formulaire");
    }
    inner
}

impl Form<'_> {
    /// Le `Ui` du formulaire, pour ce qui s'étend sur toute la largeur (un
    /// tableau, un aperçu, une liste de lignes).
    pub(super) fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// Un titre de section (S-L3), sur toute la largeur : [`theme::section_title`].
    pub(super) fn section(&mut self, title: impl Into<String>) -> egui::Response {
        theme::section_title(self.ui, title)
    }

    /// Une rangée : `label` dans la colonne des libellés, `field` dans celle
    /// des champs (le champ, puis s'il y a lieu son icône « ? »).
    pub(super) fn row<R>(
        &mut self,
        label: impl Into<egui::WidgetText>,
        field: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let label_width = self.label_width;
        let (inner, width) = self
            .ui
            .horizontal(|ui| {
                let width = ui.label(label).rect.width();
                ui.add_space((label_width - width).max(0.0));
                (field(ui), width)
            })
            .inner;
        self.widest = self.widest.max(width);
        inner
    }

    /// Une rangée sans libellé : `field` dans la colonne des champs (case à
    /// cocher, bouton, groupe de radios sous son libellé).
    pub(super) fn field<R>(&mut self, field: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let indent = self.indent();
        self.ui
            .horizontal(|ui| {
                ui.add_space(indent);
                field(ui)
            })
            .inner
    }

    /// Une case à cocher dans la colonne des champs, son rappel en icône
    /// « ? » à droite si `hint` est donné. Rend `true` si elle a changé.
    pub(super) fn checkbox(
        &mut self,
        value: &mut bool,
        text: impl Into<egui::WidgetText>,
        hint: Option<String>,
    ) -> bool {
        self.field(|ui| match hint {
            Some(hint) => help_beside(ui, hint, |ui| ui.checkbox(value, text).changed()),
            None => ui.checkbox(value, text).changed(),
        })
    }

    /// Une rangée de couleur : `label` à gauche, l'échantillon du sélecteur
    /// commun ([`color_button`], identifié par son libellé comme
    /// `color_field`) dans la colonne des champs. Rend `true` quand un geste
    /// vient de se terminer.
    pub(super) fn color(
        &mut self,
        label: &str,
        value: &mut String,
        usage: ColorUsage,
        auto_label: Option<&str>,
        env: &mut PickerEnv,
    ) -> bool {
        self.row(label, |ui| {
            color_button(ui, label, value, usage, auto_label, env)
        })
    }

    /// Un texte en clair estompé (effet de bord, portée : S-M1) dans la colonne
    /// des champs, qui revient à la ligne dans sa colonne.
    pub(super) fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        let indent = self.indent();
        self.ui.horizontal_top(|ui| {
            ui.add_space(indent);
            ui.vertical(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
            });
        });
    }

    /// Les rangées de `add`, grisées si `enabled` est faux (S-G4 : valeurs
    /// visibles et conservées). Rend la réponse du groupe, pour sa raison.
    pub(super) fn enabled<R>(
        &mut self,
        enabled: bool,
        add: impl FnOnce(&mut Form<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let label_width = self.label_width;
        let mut widest = self.widest;
        let response = self.ui.add_enabled_ui(enabled, |ui| {
            let mut inner_form = Form {
                ui,
                label_width,
                widest,
            };
            let inner = add(&mut inner_form);
            widest = inner_form.widest;
            inner
        });
        self.widest = widest;
        response
    }

    /// Le retrait de la colonne des champs : la colonne des libellés et
    /// l'écart entre les deux colonnes.
    fn indent(&self) -> f32 {
        if self.label_width > 0.0 {
            self.label_width + self.ui.spacing().item_spacing.x
        } else {
            0.0
        }
    }
}

/// Comment se peint un choix exclusif (S-C1, S-C2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChoiceLayout {
    /// 2 ou 3 options aux libellés de 3 mots au plus : radios côte à côte.
    RadiosRow,
    /// 2 ou 3 options dont un libellé dépasse 3 mots : radios l'un sous
    /// l'autre.
    RadiosColumn,
    /// 4 options ou plus : liste déroulante.
    List,
}

/// Le nombre d'options à partir duquel un choix passe en liste (S-C1).
pub(super) const LIST_FROM: usize = 4;

/// Le nombre de mots au-delà duquel les radios passent l'un sous l'autre (S-C2).
const RADIO_ROW_WORDS: usize = 3;

/// La décision de S-C1 et S-C2, pure : la forme d'un choix d'après ses libellés.
pub(super) fn choice_layout<S: AsRef<str>>(labels: &[S]) -> ChoiceLayout {
    if labels.len() >= LIST_FROM {
        ChoiceLayout::List
    } else if labels
        .iter()
        .all(|label| label.as_ref().split_whitespace().count() <= RADIO_ROW_WORDS)
    {
        ChoiceLayout::RadiosRow
    } else {
        ChoiceLayout::RadiosColumn
    }
}

/// Une option d'un [`choice`] : sa valeur, son libellé, et le rappel
/// facultatif posé en icône « ? » à côté de son radio (ou en bulle de son
/// entrée de liste).
pub(super) struct ChoiceOption<T> {
    pub(super) value: T,
    pub(super) label: String,
    pub(super) hint: Option<String>,
    pub(super) disabled: Option<String>,
}

impl<T> ChoiceOption<T> {
    /// Une option sans rappel.
    pub(super) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            hint: None,
            disabled: None,
        }
    }

    /// Grise une option avec la raison du refus affichée au survol.
    pub(super) fn disabled(mut self, reason: Option<String>) -> Self {
        self.disabled = reason;
        self
    }

    /// La même option, avec son rappel.
    pub(super) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Un choix exclusif parmi des options fixes, peint selon [`choice_layout`] :
/// radios (2–3 options) ou liste déroulante (4 et plus). `same` dit si une
/// valeur est l'option courante (égalité, ou même variante quand l'option
/// porte un paramètre). Rend `true` si l'utilisateur a choisi une **autre**
/// option, écrite dans `value`.
pub(super) fn choice_by<T: Clone>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
    same: impl Fn(&T, &T) -> bool,
) -> bool {
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    let current = options.iter().position(|option| same(&option.value, value));
    let mut picked = None;
    match choice_layout(&labels) {
        ChoiceLayout::List => {
            let shown = current.map(|index| labels[index]).unwrap_or_default();
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        let mut entry = ui
                            .add_enabled(
                                option.disabled.is_none(),
                                egui::Button::selectable(
                                    current == Some(index),
                                    option.label.as_str(),
                                ),
                            )
                            .on_disabled_hover_text(option.disabled.as_deref().unwrap_or_default());
                        if let Some(hint) = &option.hint {
                            entry = entry.on_hover_text(hint);
                        }
                        if entry.clicked() {
                            picked = Some(index);
                        }
                    }
                });
        }
        layout => {
            let radios = |ui: &mut egui::Ui| {
                // Un libellé qui ne tient pas passe à la ligne (#335), il n'est
                // pas rogné par le bord de la colonne.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.push_id(id_salt, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        let radio = |ui: &mut egui::Ui| {
                            ui.add_enabled(
                                option.disabled.is_none(),
                                egui::RadioButton::new(
                                    current == Some(index),
                                    option.label.as_str(),
                                ),
                            )
                            .on_disabled_hover_text(option.disabled.as_deref().unwrap_or_default())
                        };
                        let clicked = match &option.hint {
                            Some(hint) => help_beside(ui, hint.clone(), radio).clicked(),
                            None => radio(ui).clicked(),
                        };
                        if clicked {
                            picked = Some(index);
                        }
                    }
                });
            };
            if layout == ChoiceLayout::RadiosRow {
                ui.horizontal_wrapped(radios);
            } else {
                ui.vertical(radios);
            }
        }
    }
    match picked {
        Some(index) if current != Some(index) => {
            *value = options[index].value.clone();
            true
        }
        _ => false,
    }
}

/// [`choice_by`] pour des valeurs comparables par égalité.
pub(super) fn choice<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
) -> bool {
    choice_by(ui, id_salt, value, options, |a, b| a == b)
}

/// Le rayon des coins d'un texte entouré de [`toggle_texts`].
const TOGGLE_TEXT_RADIUS: u8 = 6;

/// Des **textes entourés** qui font office de boutons à bascule (#270) : un
/// cadre arrondi autour de chaque libellé, l'option courante en fond plein de
/// la couleur d'accent, les autres en contour seul. Un clic sur une autre
/// option la rend courante (rend `true`) ; un clic sur la courante ne change
/// rien. C'est un choix exclusif, mais pour la rangée d'outils de l'aperçu
/// d'analyse, où des radios alourdiraient une barre qui change selon l'analyse ;
/// partout ailleurs, [`choice`]. Peint à la main : le fond sélectionné d'egui
/// est réservé aux menus (`aucun_bouton_selectionnable_hors_d_un_menu`).
pub(super) fn toggle_texts<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: impl Hash + std::fmt::Debug,
    value: &mut T,
    options: &[ChoiceOption<T>],
) -> bool {
    let mut picked = None;
    ui.push_id(id_salt, |ui| {
        ui.horizontal(|ui| {
            for (index, option) in options.iter().enumerate() {
                let active = *value == option.value;
                let font = egui::TextStyle::Button.resolve(ui.style());
                // Couleur choisie à la peinture (`galley` la prend en repli).
                let galley = ui.painter().layout_no_wrap(
                    option.label.clone(),
                    font,
                    egui::Color32::PLACEHOLDER,
                );
                let size = galley.size() + egui::vec2(16.0, 6.0);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        active,
                        option.label.as_str(),
                    )
                });
                let radius = egui::CornerRadius::same(TOGGLE_TEXT_RADIUS);
                let (text_color, outline) = if active {
                    ui.painter().rect_filled(rect, radius, theme::accent());
                    (theme::text_on_accent(), theme::accent())
                } else {
                    let line = if response.hovered() {
                        ui.visuals().widgets.hovered.bg_stroke
                    } else {
                        ui.visuals().widgets.inactive.bg_stroke
                    };
                    (ui.visuals().text_color(), line.color)
                };
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(1.0, outline),
                    egui::StrokeKind::Inside,
                );
                ui.painter()
                    .galley(rect.center() - galley.size() / 2.0, galley, text_color);
                if response.clicked() {
                    picked = Some(index);
                }
                if let Some(hint) = &option.hint {
                    response.on_hover_text(hint);
                }
            }
        });
    });
    match picked {
        Some(index) if options[index].value != *value => {
            *value = options[index].value.clone();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    use super::super::test_support::{cliquer, contexte, image, textes};
    use super::{choice, choice_layout, form, toggle_texts, ChoiceLayout, ChoiceOption};
    use crate::theme;

    /// S-C1, S-C2 : 2–3 options en radios, côte à côte tant que chaque libellé
    /// tient en 3 mots ; 4 et plus en liste, quelle que soit leur longueur.
    #[test]
    fn la_forme_d_un_choix_suit_le_nombre_d_options_et_de_mots() {
        assert_eq!(choice_layout(&["Dot", "Comma"]), ChoiceLayout::RadiosRow);
        assert_eq!(
            choice_layout(&["None", "Constant", "Linear"]),
            ChoiceLayout::RadiosRow
        );
        assert_eq!(
            choice_layout(&["One sheet per file", "Side by side"]),
            ChoiceLayout::RadiosColumn,
            "un libellé de 4 mots met les radios l'un sous l'autre"
        );
        assert_eq!(
            choice_layout(&["a b c", "d e f"]),
            ChoiceLayout::RadiosRow,
            "3 mots tiennent encore sur la rangée"
        );
        assert_eq!(choice_layout(&["a", "b", "c", "d"]), ChoiceLayout::List);
        assert_eq!(
            choice_layout(&["un deux trois quatre", "b", "c", "d"]),
            ChoiceLayout::List,
            "dès 4 options, la liste, même avec un libellé long"
        );
    }

    /// S-L1 : la colonne des champs est une même verticale, quel que soit le
    /// libellé de la rangée ; une case sans libellé s'y range aussi. Témoin :
    /// les libellés, eux, commencent tous à gauche, à des largeurs différentes.
    #[test]
    fn les_champs_d_un_formulaire_sont_alignes_sur_une_meme_verticale() {
        let context = contexte();
        let mut case = false;
        let mut formes = Vec::new();
        for _ in 0..3 {
            formes = image(&context, Vec::new(), |ui| {
                form(ui, "essai", |form| {
                    form.row("Court", |ui| ui.label("champ un"));
                    form.row("Un libellé bien plus long", |ui| ui.label("champ deux"));
                    form.checkbox(&mut case, "case", None);
                    form.note("note");
                });
            });
        }
        // Le rectangle peint d'un texte (sa galée posée à l'écran).
        let rect = |cherche: &str| {
            formes
                .iter()
                .find_map(|forme| match &forme.shape {
                    egui::Shape::Text(texte) if texte.galley.job.text == cherche => {
                        Some(texte.galley.rect.translate(texte.pos.to_vec2()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{cherche:?} non peint"))
        };
        let champ_un = rect("champ un").left();
        assert!((champ_un - rect("champ deux").left()).abs() < 0.5);
        assert!(
            rect("case").left() > champ_un,
            "la case est dans la colonne des champs (après sa coche)"
        );
        assert!((rect("note").left() - champ_un).abs() < 0.5);
        // Témoin : les libellés partent du même bord gauche, et le plus long
        // pousse la colonne des champs au-delà de sa fin.
        let long = rect("Un libellé bien plus long");
        assert!((rect("Court").left() - long.left()).abs() < 0.5);
        assert!(champ_un > long.right());
        assert!(rect("Court").right() < long.right());
    }

    /// Un clic sur un radio choisit sa valeur ; un clic sur l'option déjà
    /// choisie ne rend pas de changement.
    #[test]
    fn un_clic_sur_un_radio_choisit_son_option() {
        let context = contexte();
        let mut valeur = 1;
        let options = [ChoiceOption::new(1, "Un"), ChoiceOption::new(2, "Deux")];
        let change = std::cell::Cell::new(false);
        let peindre = |events: Vec<egui::Event>, valeur: &mut i32| {
            textes(&image(&context, events, |ui| {
                change.set(choice(ui, "radios", valeur, &options));
            }))
        };
        let peints = peindre(Vec::new(), &mut valeur);
        let deux = peints.iter().find(|t| t.texte == "Deux").unwrap().centre;
        let clic = |pressed| egui::Event::PointerButton {
            pos: deux,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        peindre(vec![egui::Event::PointerMoved(deux)], &mut valeur);
        peindre(vec![clic(true)], &mut valeur);
        peindre(vec![clic(false)], &mut valeur);
        assert_eq!(valeur, 2);
        assert!(change.get());
        peindre(vec![clic(true)], &mut valeur);
        peindre(vec![clic(false)], &mut valeur);
        assert!(!change.get(), "l'option déjà choisie ne change rien");
    }
    /// #335 : dans une colonne étroite, des radios côte à côte ou l'un sous
    /// l'autre passent à la ligne au lieu de dépasser (et d'être rognés).
    #[test]
    fn des_radios_dans_une_colonne_etroite_ne_depassent_pas() {
        let context = contexte();
        let mut valeur = 1;
        let options = [
            ChoiceOption::new(1, "Mathematical Correction"),
            ChoiceOption::new(2, "Constant (point)"),
        ];
        let largeur = 120.0;
        let formes = image(&context, Vec::new(), |ui| {
            ui.allocate_ui(egui::vec2(largeur, 400.0), |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        choice(ui, "etroit", &mut valeur, &options);
                    });
                });
            });
        });
        for forme in &formes {
            if let egui::Shape::Text(texte) = &forme.shape {
                let droite = texte.pos.x + texte.galley.rect.right();
                assert!(
                    droite <= largeur + 1.0,
                    "{:?} dépasse la colonne ({droite} > {largeur})",
                    texte.galley.job.text
                );
            }
        }
    }

    /// Le même refus doit tenir dans les radios et dans une liste déroulante.
    #[test]
    fn une_option_grisee_ne_se_choisit_ni_en_radio_ni_en_liste() {
        use super::super::test_support::cliquer;
        for count in [2, 4] {
            let context = contexte();
            let mut value = 1;
            let options: Vec<_> = (1..=count)
                .map(|n| {
                    ChoiceOption::new(n, n.to_string()).disabled((n == 2).then(|| "refus".into()))
                })
                .collect();
            let mut peindre = |ui: &mut egui::Ui| {
                choice(ui, "refus", &mut value, &options);
            };
            let mut painted = image(&context, vec![], &mut peindre);
            if count == 4 {
                let position = textes(&painted)
                    .iter()
                    .find(|t| t.texte == "1")
                    .unwrap()
                    .centre;
                cliquer(
                    &context,
                    position,
                    egui::PointerButton::Primary,
                    &mut peindre,
                );
                painted = image(&context, vec![], &mut peindre);
            }
            let position = textes(&painted)
                .iter()
                .find(|t| t.texte == "2")
                .unwrap()
                .centre;
            cliquer(
                &context,
                position,
                egui::PointerButton::Primary,
                &mut peindre,
            );
            assert_eq!(value, 1);
        }
    }

    /// #270 : les textes entourés sont un choix exclusif à bascule. L'option
    /// courante a le fond plein de l'accent et son texte clair ; un clic sur
    /// l'autre la rend courante ; un clic sur la courante ne change rien.
    #[test]
    fn les_textes_entoures_basculent_et_marquent_l_option_courante() {
        let context = contexte();
        let value = std::cell::Cell::new(1);
        let changes = std::cell::Cell::new(0);
        let options = vec![ChoiceOption::new(1, "Point"), ChoiceOption::new(2, "Zone")];
        let mut peindre = |ui: &mut egui::Ui| {
            let mut courante = value.get();
            if toggle_texts(ui, "bascule", &mut courante, &options) {
                changes.set(changes.get() + 1);
            }
            value.set(courante);
        };
        let painted = image(&context, vec![], &mut peindre);
        let remplis = |painted: &[egui::epaint::ClippedShape]| {
            painted
                .iter()
                .filter(|p| matches!(&p.shape, egui::Shape::Rect(r) if r.fill == theme::accent()))
                .count()
        };
        assert_eq!(remplis(&painted), 1, "une seule option est pleine");
        let texte = |painted: &[egui::epaint::ClippedShape], cherche: &str| {
            textes(painted)
                .into_iter()
                .find(|t| t.texte == cherche)
                .unwrap()
        };
        assert_eq!(texte(&painted, "Point").couleur, theme::text_on_accent());
        assert_ne!(texte(&painted, "Zone").couleur, theme::text_on_accent());
        let (point, zone) = (
            texte(&painted, "Point").centre,
            texte(&painted, "Zone").centre,
        );
        cliquer(&context, point, egui::PointerButton::Primary, &mut peindre);
        assert_eq!(
            (value.get(), changes.get()),
            (1, 0),
            "la courante : rien ne change"
        );
        cliquer(&context, zone, egui::PointerButton::Primary, &mut peindre);
        assert_eq!((value.get(), changes.get()), (2, 1));
        let painted = image(&context, vec![], &mut peindre);
        assert_eq!(remplis(&painted), 1);
        assert_eq!(
            texte(&painted, "Zone").couleur,
            theme::text_on_accent(),
            "Zone est pleine"
        );
    }
}
