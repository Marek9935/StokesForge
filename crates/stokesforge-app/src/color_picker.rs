//! Sélecteur de couleur de StokesForge : un bouton-échantillon qui ouvre une
//! fenêtre surgissante avec une **palette** adaptée à ce qu'on colore, les
//! **couleurs enregistrées**, un **nuancier** (carré saturation ×
//! luminosité ou **roue chromatique**), l'**opacité**, et la saisie en
//! **RVB**, **TSL** et **hexadécimal**.
//!
//! # Une palette par usage
//!
//! Une courbe, un texte et un fond n'ont pas besoin des mêmes couleurs
//! ([`ColorUsage`]) :
//!
//! - **courbes** : un outil à part, plus complet — un choix de palettes
//!   nommées ([`stokesforge_plot::CURVE_PALETTES`] : StokesForge, Tableau 10,
//!   Okabe-Ito lisible par les daltoniens, ColorBrewer, et les dégradés
//!   Viridis, Plasma, gris), chacune avec ses nuances foncées et claires.
//!   Les palettes vivent dans `stokesforge-plot` pour qu'une évolution future
//!   puisse colorer plusieurs courbes d'un coup (palette ou dégradé) sans
//!   les déplacer ;
//! - **texte** : des encres lisibles — noirs, gris, blanc, teintes foncées ;
//! - **fonds** : des teintes claires, des fonds **sombres** (le texte
//!   automatique passe alors en blanc, règle de `stokesforge_render::color::
//!   auto_ink`), et **« Aucune »** : un fond transparent, pour intégrer la
//!   figure exportée à une autre présentation.
//!
//! # Couleurs enregistrées
//!
//! « Enregistrer » garde la couleur courante **dans le projet** (par
//! défaut : elle voyage avec le `.stf`, et s'annule par Ctrl+Z) ou **pour
//! l'utilisateur** (sur cette machine, dans tous les projets) — voir
//! [`crate::state::SaveTarget`]. Une couleur enregistrée ne réapparaît que
//! dans la palette de son usage. Clic droit sur une couleur enregistrée : la
//! retirer. Ce module ne touche ni au modèle ni au disque : il dépose des
//! [`SavedColorRequest`] dans le [`PickerEnv`] fourni par l'appelant, qui
//! les traduit en actions.
//!
//! # « Automatique », « Aucune » et l'opacité
//!
//! Dans le modèle, une couleur est une chaîne `#RRGGBB` (opaque) ou
//! `#RRGGBBAA` ; la chaîne vide veut dire « automatique » (encre contrastée
//! avec le fond, couleur du trait pour un marqueur…). « Aucune » est une
//! opacité nulle (`#ffffff00`). Une couleur transparente est montrée sur un
//! damier.
//!
//! # Quand la couleur est-elle validée
//!
//! La chaîne est modifiée en direct (retour visuel immédiat dans le
//! graphe), et [`color_button`] rend `true` quand l'interaction est finie :
//! clic sur un échantillon, fin d'un glisser dans le nuancier, champ
//! validé, fermeture de la fenêtre surgissante. Même règle anti-inflation
//! d'historique que le reste de la fenêtre des propriétés
//! (`crate::app::plot_settings_window`).
//!
//! Les conversions (hexadécimal, RVB ↔ TSL, RVB ↔ TSV) et les palettes sont
//! des fonctions pures, testées sans fenêtre.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
pub use stokesforge_plot::ColorUsage;
use stokesforge_plot::{SavedColors, CURVE_PALETTES};

use crate::state::SaveTarget;
use crate::tr;

/// Une rangée nommée d'échantillons.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteSection {
    /// Titre de la rangée.
    pub title: String,
    /// Couleurs, en RVBA (opacité comprise : « Aucune couleur » est
    /// transparente).
    pub colors: Vec<[u8; 4]>,
}

/// Le nom affiché de la palette de courbes d'indice `index` dans
/// [`CURVE_PALETTES`] (ramené dans les bornes). Les noms de `stokesforge-plot` sont
/// ceux de la description neutre du graphique : l'interface, elle, les écrit
/// par le catalogue (clés `color-palette-*`, dans l'ordre de la table).
pub fn curve_palette_name(index: usize) -> String {
    let index = index.min(CURVE_PALETTES.len() - 1);
    match index {
        0 => tr!("color-palette-stokesforge"),
        1 => tr!("color-palette-tableau"),
        2 => tr!("color-palette-okabe-ito"),
        3 => tr!("color-palette-brewer-set1"),
        4 => tr!("color-palette-brewer-dark2"),
        5 => tr!("color-palette-brewer-paired"),
        6 => tr!("color-palette-viridis"),
        7 => tr!("color-palette-plasma"),
        8 => tr!("color-palette-grayscale"),
        _ => CURVE_PALETTES[index].name.to_string(),
    }
}

/// Encres de la palette « texte ».
const TEXT_INKS: [&str; 12] = [
    "#000000", "#202020", "#404040", "#6b6b6b", "#a0a0a0", "#ffffff", "#1f2a44", "#1f4e79",
    "#8b1a1a", "#1e5631", "#4b2a6b", "#5c3a1e",
];

/// Fonds clairs de la palette « fonds », précédés d'« Aucune couleur »
/// ([`NO_COLOR`]), le fond par défaut d'un graphe.
const LIGHT_BACKGROUNDS: [&str; 9] = [
    NO_COLOR, "#ffffff", "#fafaf7", "#f5f0e1", "#f4ecd8", "#f0f0f0", "#e6e6e6", "#eef4fb",
    "#eef7ee",
];

/// Fonds sombres de la palette « fonds » : texte automatique en blanc.
const DARK_BACKGROUNDS: [&str; 6] = [
    "#000000", "#1e1e1e", "#2b2f36", "#101820", "#14213d", "#102a1f",
];

/// La chaîne d'un fond sans couleur : blanc d'opacité nulle — le fond par
/// défaut d'un graphe (`stokesforge_plot::TRANSPARENT`).
pub const NO_COLOR: &str = stokesforge_plot::TRANSPARENT;

fn parse_all(colors: &[&str]) -> Vec<[u8; 4]> {
    colors.iter().filter_map(|hex| parse_hex(hex)).collect()
}

/// Les rangées de la palette de `usage`. Pour les courbes, celles de la
/// palette nommée d'indice `curve_palette` dans
/// [`stokesforge_plot::CURVE_PALETTES`] (ramené dans les bornes).
pub fn palette(usage: ColorUsage, curve_palette: usize) -> Vec<PaletteSection> {
    match usage {
        ColorUsage::Curves => {
            let named = CURVE_PALETTES[curve_palette.min(CURVE_PALETTES.len() - 1)];
            let base = parse_all(named.colors);
            let shade = |factor: fn(f32) -> f32| {
                base.iter()
                    .map(|&[r, g, b, a]| {
                        let (h, s, l) = rgb_to_hsl([r, g, b]);
                        let [r, g, b] = hsl_to_rgb(h, s, factor(l));
                        [r, g, b, a]
                    })
                    .collect()
            };
            vec![
                PaletteSection {
                    title: curve_palette_name(curve_palette),
                    colors: base.clone(),
                },
                PaletteSection {
                    title: tr!("color-shades-dark"),
                    colors: shade(|l| l * 0.62),
                },
                PaletteSection {
                    title: tr!("color-shades-light"),
                    colors: shade(|l| l + (100.0 - l) * 0.45),
                },
            ]
        }
        ColorUsage::Text => vec![PaletteSection {
            title: tr!("color-section-text"),
            colors: parse_all(&TEXT_INKS),
        }],
        ColorUsage::Background => vec![
            PaletteSection {
                title: tr!("color-section-light-backgrounds"),
                colors: parse_all(&LIGHT_BACKGROUNDS),
            },
            PaletteSection {
                title: tr!("color-section-dark-backgrounds"),
                colors: parse_all(&DARK_BACKGROUNDS),
            },
        ],
    }
}

/// Les composantes d'une couleur `#RRGGBB` (opaque) ou `#RRGGBBAA` —
/// casse indifférente, `#` facultatif, espaces de bordure ignorés —, ou
/// `None`. La syntaxe est celle de [`stokesforge_plot::parse_hex`], source unique
/// partagée avec le rendu (issue #25).
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    stokesforge_plot::parse_hex(text)
}

/// La chaîne d'une couleur : `#rrggbb` si elle est opaque, `#rrggbbaa`
/// sinon.
pub fn to_hex([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// RVB → TSL : teinte en degrés (0–360), saturation et luminosité en
/// pourcentage (0–100).
pub fn rgb_to_hsl([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    (hue_of(r, g, b, max, delta), s * 100.0, l * 100.0)
}

/// TSL → RVB, inverse de [`rgb_to_hsl`]. Les entrées hors bornes sont
/// ramenées dans leurs bornes.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    from_chroma(h, c, l - c / 2.0)
}

/// RVB → TSV : teinte, saturation et valeur, chacune entre 0 et 1.
pub fn rgb_to_hsv([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let s = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    let h = if delta <= f32::EPSILON {
        0.0
    } else {
        hue_of(r, g, b, max, delta) / 360.0
    };
    (h, s, max)
}

/// TSV → RVB, inverse de [`rgb_to_hsv`].
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = (h * 360.0).rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    from_chroma(h, c, v - c)
}

fn hue_of(r: f32, g: f32, b: f32, max: f32, delta: f32) -> f32 {
    let h = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    h * 60.0
}

fn from_chroma(h: f32, c: f32, m: f32) -> [u8; 3] {
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let channel = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [channel(r), channel(g), channel(b)]
}

/// Point d'une roue chromatique de rayon 1 centrée à l'origine → (teinte,
/// saturation), chacune entre 0 et 1 ; la teinte 0 (rouge) est à droite et
/// tourne dans le sens trigonométrique, la saturation croît vers le bord.
pub fn wheel_to_hue_saturation(dx: f32, dy: f32) -> (f32, f32) {
    let hue = (-dy).atan2(dx) / std::f32::consts::TAU;
    (hue.rem_euclid(1.0), (dx * dx + dy * dy).sqrt().min(1.0))
}

/// Inverse de [`wheel_to_hue_saturation`].
pub fn hue_saturation_to_wheel(hue: f32, saturation: f32) -> (f32, f32) {
    let angle = hue * std::f32::consts::TAU;
    (saturation * angle.cos(), -saturation * angle.sin())
}

/// Enregistrer ou retirer une couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedColorEdit {
    /// Enregistrer.
    Save,
    /// Retirer.
    Remove,
}

/// Une demande du sélecteur sur les couleurs enregistrées, que l'appelant
/// traduit en action (`Action::SaveColor`/`RemoveSavedColor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedColorRequest {
    /// Enregistrer ou retirer.
    pub edit: SavedColorEdit,
    /// Usage.
    pub usage: ColorUsage,
    /// Projet ou utilisateur.
    pub target: SaveTarget,
    /// La couleur.
    pub color: String,
}

impl SavedColorRequest {
    /// L'action qui exécute la demande.
    pub fn into_action(self) -> crate::state::Action {
        use crate::state::Action;
        match self.edit {
            SavedColorEdit::Save => Action::SaveColor {
                usage: self.usage,
                color: self.color,
                target: self.target,
            },
            SavedColorEdit::Remove => Action::RemoveSavedColor {
                usage: self.usage,
                color: self.color,
                target: self.target,
            },
        }
    }
}

/// Ce que le sélecteur lit et écrit hors de la couleur éditée : les couleurs
/// enregistrées, et les demandes qui les modifient.
#[derive(Debug, Clone, Default)]
pub struct PickerEnv {
    /// Couleurs enregistrées dans le projet.
    pub project: SavedColors,
    /// Couleurs enregistrées pour l'utilisateur.
    pub user: SavedColors,
    /// Demandes déposées pendant cette image.
    pub requests: Vec<SavedColorRequest>,
}

/// Un libellé suivi de [`color_button`], sur une ligne.
pub fn color_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        color_button(ui, label, value, usage, auto_label, env)
    })
    .inner
}

/// Côté de l'échantillon du bouton : un petit carré (#89).
const SWATCH_SIDE: f32 = 12.0;
/// Place réservée au bouton : le carré, plus un peu d'air sur les côtés.
const BUTTON_SWATCH: Vec2 = Vec2::new(14.5, SWATCH_SIDE);
/// Le trait en travers d'une couleur non définie (automatique), même rouge
/// que le rôle `Danger` des icônes.
const AUTO_SLASH: Color32 = Color32::from_rgb(0xe5, 0x45, 0x3b);
/// Taille d'un échantillon de palette.
const PALETTE_SWATCH: f32 = 18.0;
/// Côté du carré du nuancier (et diamètre de la roue).
const PICKER_SIDE: f32 = 200.0;
/// Hauteur des barres (teinte, valeur, opacité).
const BAR_HEIGHT: f32 = 16.0;
/// Finesse des maillages du nuancier.
const MESH_STEPS: usize = 24;
/// Côté d'une case du damier des couleurs transparentes.
const CHECKER: f32 = 5.0;

/// Nuancier affiché : carré ou roue. Choix mémorisé pour la session, commun
/// à tous les sélecteurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PickerShape {
    #[default]
    Square,
    Wheel,
}

/// État du nuancier entre deux images : la couleur en TSV, gardée à part de
/// la chaîne pour ne pas perdre la teinte sur un gris (où elle n'est pas
/// définie) pendant qu'on glisse.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PickerMemory {
    /// La couleur à laquelle `hsv` correspond ; si le modèle change (autre
    /// champ, annulation), la mémoire est recalculée depuis la chaîne.
    rgb: [u8; 3],
    hsv: (f32, f32, f32),
}

/// Peint `rgba` dans `rect`, sur un damier si elle n'est pas opaque.
fn paint_swatch(painter: &egui::Painter, rect: Rect, [r, g, b, a]: [u8; 4]) {
    if a < 255 {
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        let columns = (rect.width() / CHECKER).ceil() as i32;
        let rows = (rect.height() / CHECKER).ceil() as i32;
        for row in 0..rows {
            for column in 0..columns {
                if (row + column) % 2 == 0 {
                    continue;
                }
                let min = rect.min + Vec2::new(column as f32 * CHECKER, row as f32 * CHECKER);
                let cell = Rect::from_min_size(min, Vec2::splat(CHECKER)).intersect(rect);
                painter.rect_filled(cell, 0.0, Color32::from_gray(0xcc));
            }
        }
    }
    painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(r, g, b, a));
}

/// Le bouton-échantillon et sa fenêtre surgissante. `value` est la chaîne
/// du modèle (vide = automatique quand `auto_label` est donné). Rend `true`
/// quand un geste vient de se terminer — voir la doc du module.
pub fn color_button(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    let current = parse_hex(value);
    let (cell, response) = ui.allocate_exact_size(BUTTON_SWATCH, Sense::click());
    let rect = Rect::from_center_size(cell.center(), Vec2::splat(SWATCH_SIDE));
    let response = response.on_hover_text(match current {
        Some([_, _, _, 0]) => tr!("color-none"),
        Some(rgba) => to_hex(rgba),
        None => auto_label.map_or_else(|| tr!("common-auto"), str::to_string),
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let stroke = ui.visuals().widgets.inactive.fg_stroke;
        match current {
            Some(rgba) => paint_swatch(painter, rect, rgba),
            // Couleur non définie = automatique : carré clair barré d'un trait
            // rouge en travers (#89).
            None => {
                painter.rect_filled(rect, 2.0, Color32::WHITE);
                let inner = rect.shrink(1.5);
                painter.line_segment(
                    [inner.left_bottom(), inner.right_top()],
                    egui::Stroke::new(1.5, AUTO_SLASH),
                );
            }
        }
        painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    }

    let popup_id = ui.make_persistent_id(("stokesforge_color_popup", id_salt));
    let was_open = picker_is_open(ui.ctx(), popup_id);
    if response.clicked() {
        set_picker_open(ui.ctx(), popup_id, !was_open);
    }
    let mut commit = false;
    // Une `Area` à nous, pas `egui::Popup` : egui ne retient qu'une fenêtre
    // surgissante ouverte par viewport, donc la liste d'un `ComboBox` placé
    // dans le sélecteur le refermerait (issue #9). La fermeture (Échap, clic
    // extérieur) est écrite ici, en épargnant les couches surgissantes filles.
    if picker_is_open(ui.ctx(), popup_id) {
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 4.0))
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(PICKER_SIDE + 40.0);
                    commit |= picker_contents(ui, popup_id, value, usage, auto_label, env);
                });
            });
        // Échap est consommé : le dialogue qui contient le sélecteur ne doit
        // pas se fermer avec lui.
        let escape =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let pressed_at = ui.input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        let list_open = egui::Popup::is_any_open(ui.ctx());
        let outside = pressed_at.is_some_and(|pos| {
            !response.rect.contains(pos)
                && !ui.ctx().layer_id_at(pos).is_some_and(|layer| {
                    // Une liste de menu déroulant (couche surgissante fille)
                    // compte comme « dedans » ; le dialogue et son calque
                    // bloquant, eux, sont au même ordre mais sont « dehors ».
                    layer == area.response.layer_id
                        || (list_open && layer.order == egui::Order::Foreground)
                })
        });
        if escape || outside {
            set_picker_open(ui.ctx(), popup_id, false);
        }
    }
    let is_open = picker_is_open(ui.ctx(), popup_id);
    // Refermer la fenêtre surgissante valide ce qui a pu être modifié sans
    // geste de fin explicite (champ encore en saisie).
    commit || (was_open && !is_open)
}

/// Une icône qui dit *quelle* couleur on règle, suivie de son échantillon
/// compact : la forme des barres d'outils (#89). Les formulaires, eux, gardent
/// un libellé devant l'échantillon (`color_field`). `tooltip` vient de `tr!`.
pub fn color_icon_button(
    ui: &mut egui::Ui,
    (icon, tooltip): (crate::icons::Icon, &str),
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add(
            crate::icons::icon_image(icon, crate::icons::IconRole::Neutral).sense(Sense::hover()),
        )
        .on_hover_text(tooltip);
        color_button(ui, id_salt, value, usage, auto_label, env)
    })
    .inner
}

fn picker_is_open(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false)
}

fn set_picker_open(ctx: &egui::Context, id: egui::Id, open: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(id, open);
        if !open {
            // Le sélecteur se rouvre toujours replié sur la palette (#10).
            data.remove::<bool>(custom_id(id));
        }
    });
}

/// Clé du dépli « Personnalisée » du sélecteur `popup_id` (#10).
fn custom_id(popup_id: egui::Id) -> egui::Id {
    popup_id.with("custom")
}

/// Contenu de la fenêtre surgissante.
fn picker_contents(
    ui: &mut egui::Ui,
    popup_id: egui::Id,
    value: &mut String,
    usage: ColorUsage,
    auto_label: Option<&str>,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
    let mut close = false;
    let mut chosen: Option<[u8; 4]> = None;

    // -- Palette -------------------------------------------------------
    let curve_palette_id = egui::Id::new("stokesforge_curve_palette");
    let mut curve_palette: usize = ui
        .memory(|memory| memory.data.get_temp(curve_palette_id))
        .unwrap_or(0);
    if usage == ColorUsage::Curves {
        // L'outil des courbes : un choix de palettes nommées.
        egui::ComboBox::from_id_salt(popup_id.with("curve_palette"))
            .width(PICKER_SIDE + 20.0)
            .selected_text(curve_palette_name(curve_palette))
            .show_ui(ui, |ui| {
                for index in 0..CURVE_PALETTES.len() {
                    ui.selectable_value(&mut curve_palette, index, curve_palette_name(index));
                }
            });
        ui.memory_mut(|memory| memory.data.insert_temp(curve_palette_id, curve_palette));
    }
    let current = parse_hex(value);
    for section in palette(usage, curve_palette) {
        ui.label(egui::RichText::new(section.title).small().strong());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            for rgba in section.colors {
                let response = palette_swatch(ui, rgba, current == Some(rgba));
                if response.clicked() {
                    chosen = Some(rgba);
                    close = true;
                }
            }
        });
        ui.add_space(2.0);
    }
    // Le nuancier, l'opacité, RVB/TSL et l'enregistrement sont cachés : le
    // « + » en fin de palette les déplie (#10, comme dans Office).
    let mut custom = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(custom_id(popup_id)))
        .unwrap_or(false);
    ui.horizontal_wrapped(|ui| {
        if let Some(auto_label) = auto_label {
            if ui
                .selectable_label(value.trim().is_empty(), auto_label)
                .clicked()
            {
                value.clear();
                commit = true;
                close = true;
            }
        }
        ui.toggle_value(&mut custom, tr!("color-custom"))
            .on_hover_text(tr!("color-custom", "hint"));
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(custom_id(popup_id), custom));

    // -- Couleurs enregistrées ------------------------------------------
    ui.separator();
    for (target, title) in [
        (SaveTarget::Project, tr!("color-saved-project")),
        (SaveTarget::User, tr!("color-saved-user")),
    ] {
        let saved = match target {
            SaveTarget::Project => env.project.get(usage).to_vec(),
            SaveTarget::User => env.user.get(usage).to_vec(),
        };
        if saved.is_empty() {
            continue;
        }
        ui.label(egui::RichText::new(title).small().strong());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            for color in saved {
                let Some(rgba) = parse_hex(&color) else {
                    continue;
                };
                let response = palette_swatch(ui, rgba, current == Some(rgba))
                    .on_hover_text(tr!("color-saved-remove-hint"));
                if response.clicked() {
                    chosen = Some(rgba);
                    close = true;
                }
                if response.secondary_clicked() {
                    env.requests.push(SavedColorRequest {
                        edit: SavedColorEdit::Remove,
                        usage,
                        target,
                        color,
                    });
                }
            }
        });
    }
    if custom {
        let target_id = egui::Id::new("stokesforge_save_color_target");
        let mut target: SaveTarget = ui
            .memory(|memory| memory.data.get_temp(target_id))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(current.is_some(), egui::Button::new(tr!("color-save")))
                .on_hover_text(tr!("color-save", "hint"))
                .clicked()
            {
                if let Some(rgba) = current {
                    env.requests.push(SavedColorRequest {
                        edit: SavedColorEdit::Save,
                        usage,
                        target,
                        color: to_hex(rgba),
                    });
                }
            }
            egui::ComboBox::from_id_salt(popup_id.with("save_target"))
                .selected_text(target.label())
                .show_ui(ui, |ui| {
                    for candidate in [SaveTarget::Project, SaveTarget::User] {
                        ui.selectable_value(&mut target, candidate, candidate.label());
                    }
                });
        });
        ui.memory_mut(|memory| memory.data.insert_temp(target_id, target));
        ui.separator();
    }

    // -- Nuancier --------------------------------------------------------
    let [r0, g0, b0, a0] = current.unwrap_or([0x80, 0x80, 0x80, 255]);
    let rgb = [r0, g0, b0];
    let memory_id = popup_id.with("hsv");
    let mut memory = ui
        .memory(|memory| memory.data.get_temp::<PickerMemory>(memory_id))
        .filter(|memory| memory.rgb == rgb)
        .unwrap_or(PickerMemory {
            rgb,
            hsv: rgb_to_hsv(rgb),
        });
    let mut new_rgb: Option<[u8; 3]> = None;
    let mut alpha = a0;
    if custom {
        let shape_id = egui::Id::new("stokesforge_picker_shape");
        let mut shape: PickerShape = ui
            .memory(|memory| memory.data.get_temp(shape_id))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut shape, PickerShape::Square, tr!("color-shape-square"));
            ui.selectable_value(&mut shape, PickerShape::Wheel, tr!("color-shape-wheel"));
        });
        ui.memory_mut(|m| m.data.insert_temp(shape_id, shape));

        let (h, s, v) = &mut memory.hsv;
        let gestures = match shape {
            PickerShape::Square => [sv_square(ui, *h, s, v), hue_bar(ui, h)],
            PickerShape::Wheel => [color_wheel(ui, h, s, *v), value_bar(ui, *h, *s, v)],
        };
        if gestures.iter().any(|gesture| gesture.changed) {
            new_rgb = Some(hsv_to_rgb(memory.hsv.0, memory.hsv.1, memory.hsv.2));
        }
        commit |= gestures.iter().any(|gesture| gesture.finished);

        // -- Opacité ---------------------------------------------------------
        let mut opacity = f32::from(a0) / 255.0 * 100.0;
        let opacity_response = ui
            .horizontal(|ui| {
                ui.label(tr!("color-opacity"));
                ui.add(
                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                        .suffix(" %")
                        .max_decimals(0),
                )
            })
            .inner;
        if opacity_response.changed() {
            alpha = (opacity / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        commit |= opacity_response.drag_stopped() || opacity_response.lost_focus();

        // -- RVB ---------------------------------------------------------------
        ui.add_space(4.0);
        let shown = new_rgb.unwrap_or(rgb);
        let mut channels = shown.map(i32::from);
        let mut rgb_edit = NumericEdit::default();
        ui.horizontal(|ui| {
            ui.label(tr!("color-rgb"));
            let prefixes = [
                tr!("color-channel-red"),
                tr!("color-channel-green"),
                tr!("color-channel-blue"),
            ];
            for (channel, prefix) in channels.iter_mut().zip(prefixes) {
                rgb_edit.track(
                    ui.add(
                        egui::DragValue::new(channel)
                            .range(0..=255)
                            .speed(0.5)
                            .prefix(format!("{prefix} ")),
                    ),
                );
            }
        });
        if rgb_edit.changed {
            let edited = channels.map(|c| c.clamp(0, 255) as u8);
            new_rgb = Some(edited);
            memory.hsv = rgb_to_hsv(edited);
        }
        commit |= rgb_edit.finished;

        // -- TSL ---------------------------------------------------------------
        let (mut th, mut ts, mut tl) = rgb_to_hsl(new_rgb.unwrap_or(rgb));
        let mut hsl_edit = NumericEdit::default();
        ui.horizontal(|ui| {
            ui.label(tr!("color-hsl"));
            hsl_edit.track(
                ui.add(
                    egui::DragValue::new(&mut th)
                        .range(0.0..=360.0)
                        .speed(1.0)
                        .max_decimals(0)
                        .prefix(format!("{} ", tr!("color-channel-hue")))
                        .suffix("°"),
                ),
            );
            hsl_edit.track(
                ui.add(
                    egui::DragValue::new(&mut ts)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .max_decimals(0)
                        .prefix(format!("{} ", tr!("color-channel-saturation")))
                        .suffix(" %"),
                ),
            );
            hsl_edit.track(
                ui.add(
                    egui::DragValue::new(&mut tl)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .max_decimals(0)
                        .prefix(format!("{} ", tr!("color-channel-lightness")))
                        .suffix(" %"),
                ),
            );
        });
        if hsl_edit.changed {
            let edited = hsl_to_rgb(th, ts, tl);
            new_rgb = Some(edited);
            let (_, s, v) = rgb_to_hsv(edited);
            memory.hsv = (th / 360.0, s, v);
        }
        commit |= hsl_edit.finished;
    }

    // -- Hexadécimal : appliqué seulement lisible, à la validation ---------
    let hex_id = popup_id.with("hex");
    let [r1, g1, b1] = new_rgb.unwrap_or(rgb);
    let mut hex_text: String = ui
        .memory(|memory| memory.data.get_temp(hex_id))
        .unwrap_or_else(|| to_hex([r1, g1, b1, alpha]));
    let hex_response = ui
        .horizontal(|ui| {
            ui.label(tr!("color-hex"));
            ui.add(egui::TextEdit::singleline(&mut hex_text).desired_width(90.0))
                .on_hover_text(tr!("color-hex", "hint"))
        })
        .inner;
    if hex_response.has_focus() {
        ui.memory_mut(|memory| memory.data.insert_temp(hex_id, hex_text.clone()));
    } else {
        ui.memory_mut(|memory| memory.data.remove::<String>(hex_id));
    }
    if hex_response.lost_focus() {
        if let Some([r, g, b, a]) = parse_hex(&hex_text) {
            new_rgb = Some([r, g, b]);
            alpha = a;
            memory.hsv = rgb_to_hsv([r, g, b]);
            commit = true;
        }
    }

    // -- Application -------------------------------------------------------
    if let Some(rgba) = chosen {
        *value = to_hex(rgba);
        commit = true;
    } else if new_rgb.is_some() || alpha != a0 {
        let [r, g, b] = new_rgb.unwrap_or(rgb);
        *value = to_hex([r, g, b, alpha]);
        memory.rgb = [r, g, b];
    }
    ui.memory_mut(|m| m.data.insert_temp(memory_id, memory));
    if close {
        set_picker_open(ui.ctx(), popup_id, false);
    }
    commit
}

/// Suivi d'un groupe de champs numériques : modifié cette image, et geste
/// terminé (fin de glisser, ou saisie validée).
#[derive(Default)]
struct NumericEdit {
    changed: bool,
    finished: bool,
}

impl NumericEdit {
    fn track(&mut self, response: egui::Response) {
        self.changed |= response.changed();
        self.finished |= response.drag_stopped() || response.lost_focus();
    }
}

/// Infobulle d'un échantillon : son code, ou « Aucune couleur » s'il est
/// entièrement transparent.
fn swatch_label(rgba: [u8; 4]) -> String {
    if rgba[3] == 0 {
        tr!("color-none-swatch")
    } else {
        to_hex(rgba)
    }
}

/// Un échantillon de palette cliquable.
fn palette_swatch(ui: &mut egui::Ui, rgba: [u8; 4], selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PALETTE_SWATCH), Sense::click());
    let response = response.on_hover_text(swatch_label(rgba));
    let painter = ui.painter();
    paint_swatch(painter, rect, rgba);
    let stroke = if selected {
        Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke
    } else {
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    response
}

/// Résultat d'un élément du nuancier cette image.
struct PickerGesture {
    changed: bool,
    finished: bool,
}

/// Position du pointeur pendant un appui ou un clic sur `response`.
fn pressed_pos(response: &egui::Response) -> Option<Pos2> {
    response
        .interact_pointer_pos()
        .filter(|_| response.is_pointer_button_down_on() || response.clicked())
}

fn gesture(response: &egui::Response) -> PickerGesture {
    PickerGesture {
        changed: false,
        finished: response.drag_stopped() || response.clicked(),
    }
}

/// Le carré saturation (horizontale) × valeur (verticale) à la teinte `h`.
fn sv_square(ui: &mut egui::Ui, h: f32, s: &mut f32, v: &mut f32) -> PickerGesture {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(PICKER_SIDE), Sense::click_and_drag());
    let mut result = gesture(&response);
    if let Some(pos) = pressed_pos(&response) {
        let new_s = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let new_v = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        if (new_s, new_v) != (*s, *v) {
            (*s, *v) = (new_s, new_v);
            result.changed = true;
        }
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for row in 0..=MESH_STEPS {
            for column in 0..=MESH_STEPS {
                let fs = column as f32 / MESH_STEPS as f32;
                let fv = 1.0 - row as f32 / MESH_STEPS as f32;
                let [r, g, b] = hsv_to_rgb(h, fs, fv);
                let pos = Pos2::new(
                    rect.left() + fs * rect.width(),
                    rect.top() + (1.0 - fv) * rect.height(),
                );
                mesh.colored_vertex(pos, Color32::from_rgb(r, g, b));
                if row < MESH_STEPS && column < MESH_STEPS {
                    let i = (row * (MESH_STEPS + 1) + column) as u32;
                    let next_row = i + MESH_STEPS as u32 + 1;
                    mesh.add_triangle(i, i + 1, next_row);
                    mesh.add_triangle(i + 1, next_row + 1, next_row);
                }
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let marker = Pos2::new(
            rect.left() + *s * rect.width(),
            rect.top() + (1.0 - *v) * rect.height(),
        );
        let contrast = if *v > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        painter.circle_stroke(marker, 5.0, Stroke::new(1.5, contrast));
        painter.rect_stroke(
            rect,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
    result
}

/// La roue chromatique : teinte selon l'angle, saturation selon la distance
/// au centre, à la valeur `v`.
fn color_wheel(ui: &mut egui::Ui, h: &mut f32, s: &mut f32, v: f32) -> PickerGesture {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(PICKER_SIDE), Sense::click_and_drag());
    let center = rect.center();
    let radius = rect.width() / 2.0;
    let mut result = gesture(&response);
    if let Some(pos) = pressed_pos(&response) {
        let (new_h, new_s) =
            wheel_to_hue_saturation((pos.x - center.x) / radius, (pos.y - center.y) / radius);
        if (new_h, new_s) != (*h, *s) {
            (*h, *s) = (new_h, new_s);
            result.changed = true;
        }
    }
    if ui.is_rect_visible(rect) {
        const RINGS: usize = 12;
        const SEGMENTS: usize = 72;
        let mut mesh = egui::Mesh::default();
        let [r, g, b] = hsv_to_rgb(0.0, 0.0, v);
        mesh.colored_vertex(center, Color32::from_rgb(r, g, b));
        for ring in 1..=RINGS {
            let saturation = ring as f32 / RINGS as f32;
            for segment in 0..SEGMENTS {
                let hue = segment as f32 / SEGMENTS as f32;
                let (dx, dy) = hue_saturation_to_wheel(hue, saturation);
                let [r, g, b] = hsv_to_rgb(hue, saturation, v);
                mesh.colored_vertex(
                    center + Vec2::new(dx, dy) * radius,
                    Color32::from_rgb(r, g, b),
                );
            }
        }
        let vertex = |ring: usize, segment: usize| -> u32 {
            if ring == 0 {
                0
            } else {
                (1 + (ring - 1) * SEGMENTS + segment % SEGMENTS) as u32
            }
        };
        for segment in 0..SEGMENTS {
            mesh.add_triangle(0, vertex(1, segment), vertex(1, segment + 1));
            for ring in 2..=RINGS {
                let (a, b) = (vertex(ring - 1, segment), vertex(ring - 1, segment + 1));
                let (c, d) = (vertex(ring, segment), vertex(ring, segment + 1));
                mesh.add_triangle(a, c, d);
                mesh.add_triangle(a, d, b);
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let (dx, dy) = hue_saturation_to_wheel(*h, *s);
        let marker = center + Vec2::new(dx, dy) * radius;
        let contrast = if v > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        painter.circle_stroke(marker, 5.0, Stroke::new(1.5, contrast));
        painter.circle_stroke(
            center,
            radius,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
    result
}

/// Une barre horizontale peinte d'un dégradé `color(f)`, f de 0 à 1, avec
/// un curseur en `position`. Rend la nouvelle position si on l'a bougée.
fn gradient_bar(
    ui: &mut egui::Ui,
    position: f32,
    color: impl Fn(f32) -> Color32,
) -> (Option<f32>, PickerGesture) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(PICKER_SIDE, BAR_HEIGHT), Sense::click_and_drag());
    let result = gesture(&response);
    let moved = pressed_pos(&response)
        .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
        .filter(|&f| f != position);
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for step in 0..=MESH_STEPS {
            let f = step as f32 / MESH_STEPS as f32;
            let x = rect.left() + f * rect.width();
            mesh.colored_vertex(Pos2::new(x, rect.top()), color(f));
            mesh.colored_vertex(Pos2::new(x, rect.bottom()), color(f));
            if step < MESH_STEPS {
                let i = (step * 2) as u32;
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i + 1, i + 3, i + 2);
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let x = rect.left() + moved.unwrap_or(position) * rect.width();
        painter.rect_stroke(
            Rect::from_center_size(
                Pos2::new(x, rect.center().y),
                Vec2::new(4.0, rect.height() + 2.0),
            ),
            1.0,
            Stroke::new(1.5, Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    }
    (moved, result)
}

/// La barre de teinte, de 0 à 360°.
fn hue_bar(ui: &mut egui::Ui, h: &mut f32) -> PickerGesture {
    let (moved, mut result) = gradient_bar(ui, *h, |f| {
        let [r, g, b] = hsv_to_rgb(f, 1.0, 1.0);
        Color32::from_rgb(r, g, b)
    });
    if let Some(new_h) = moved {
        *h = new_h;
        result.changed = true;
    }
    result
}

/// La barre de valeur (du noir à la couleur pleine), sous la roue.
fn value_bar(ui: &mut egui::Ui, h: f32, s: f32, v: &mut f32) -> PickerGesture {
    let (moved, mut result) = gradient_bar(ui, *v, |f| {
        let [r, g, b] = hsv_to_rgb(h, s, f);
        Color32::from_rgb(r, g, b)
    });
    if let Some(new_v) = moved {
        *v = new_v;
        result.changed = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_hexadecimal_se_lit_et_s_ecrit_opacite_comprise() {
        assert_eq!(parse_hex("#4C78a8"), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex(" 4c78a8 "), Some([0x4c, 0x78, 0xa8, 255]));
        assert_eq!(parse_hex("#4c78a880"), Some([0x4c, 0x78, 0xa8, 0x80]));
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#abc"), None);
        assert_eq!(parse_hex("#12345g"), None);
        assert_eq!(
            to_hex([0x4c, 0x78, 0xa8, 255]),
            "#4c78a8",
            "opaque : 6 chiffres"
        );
        assert_eq!(to_hex([0x4c, 0x78, 0xa8, 0x80]), "#4c78a880");
        assert_eq!(
            parse_hex(NO_COLOR).map(|c| c[3]),
            Some(0),
            "« Aucune » est transparente"
        );
    }

    #[test]
    fn tsl_des_couleurs_de_reference() {
        assert_eq!(rgb_to_hsl([255, 0, 0]), (0.0, 100.0, 50.0));
        assert_eq!(rgb_to_hsl([255, 255, 255]), (0.0, 0.0, 100.0));
        assert_eq!(hsl_to_rgb(120.0, 100.0, 50.0), [0, 255, 0]);
        assert_eq!(hsl_to_rgb(240.0, 100.0, 25.0), [0, 0, 128]);
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.0), [0, 0, 0]);
    }

    /// Un aller-retour RVB → TSL → RVB et RVB → TSV → RVB rend la couleur de
    /// départ, sur un échantillon de tout le cube.
    #[test]
    fn les_conversions_font_l_aller_retour() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(85) {
                    let rgb = [r as u8, g as u8, b as u8];
                    let (h, s, l) = rgb_to_hsl(rgb);
                    assert_eq!(hsl_to_rgb(h, s, l), rgb, "TSL de {rgb:?}");
                    let (h, s, v) = rgb_to_hsv(rgb);
                    assert_eq!(hsv_to_rgb(h, s, v), rgb, "TSV de {rgb:?}");
                }
            }
        }
    }

    /// La roue : rouge à droite, vert (1/3) en haut à gauche, saturation
    /// nulle au centre et pleine au bord — et l'aller-retour tient.
    #[test]
    fn la_roue_place_les_teintes_comme_attendu() {
        assert_eq!(wheel_to_hue_saturation(1.0, 0.0), (0.0, 1.0));
        let (h, s) = wheel_to_hue_saturation(0.0, -1.0);
        assert!(
            (h - 0.25).abs() < 1e-6 && (s - 1.0).abs() < 1e-6,
            "haut : 90°"
        );
        assert_eq!(wheel_to_hue_saturation(0.0, 0.0).1, 0.0);
        assert_eq!(wheel_to_hue_saturation(3.0, 4.0).1, 1.0, "bornée au bord");
        for (hue, saturation) in [(0.1, 0.5), (0.6, 0.9), (0.95, 0.2)] {
            let (x, y) = hue_saturation_to_wheel(hue, saturation);
            let (h, s) = wheel_to_hue_saturation(x, y);
            assert!((h - hue).abs() < 1e-5 && (s - saturation).abs() < 1e-5);
        }
    }

    /// Dessine, sans fenêtre, un sélecteur ouvert — pour chaque usage et
    /// chaque forme de nuancier — et vérifie qu'il peint son contenu (dont
    /// les couleurs enregistrées) sans paniquer. La capture d'écran ne sait
    /// pas ouvrir une fenêtre surgissante ; ce test en est le filet.
    #[test]
    fn un_selecteur_ouvert_se_dessine_pour_chaque_usage_et_chaque_nuancier() {
        for shape in [PickerShape::Square, PickerShape::Wheel] {
            for usage in [ColorUsage::Curves, ColorUsage::Text, ColorUsage::Background] {
                let context = egui::Context::default();
                context.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(egui::Id::new("stokesforge_picker_shape"), shape)
                });
                let mut env = PickerEnv {
                    project: SavedColors::default().with_added(usage, "#123456"),
                    user: SavedColors::default().with_added(usage, "#22aa4480"),
                    requests: Vec::new(),
                };
                let mut value = "#ff000080".to_string();
                let mut shapes = 0;
                for frame in 0..3 {
                    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                        let popup_id =
                            ui.make_persistent_id(("stokesforge_color_popup", "Couleur"));
                        if frame == 0 {
                            set_picker_open(ui.ctx(), popup_id, true);
                            // Le nuancier est replié par défaut (#10) : on le
                            // déplie pour éprouver tout le contenu.
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(custom_id(popup_id), true));
                        }
                        color_button(ui, "Couleur", &mut value, usage, Some("Auto"), &mut env);
                    });
                    output.textures_delta.clear();
                    shapes = output.shapes.len();
                }
                assert!(
                    shapes > 40,
                    "{usage:?}/{shape:?} : {shapes} formes seulement"
                );
                assert_eq!(value, "#ff000080", "rien n'a été modifié sans geste");
                assert!(env.requests.is_empty());
            }
        }
    }

    /// Position du texte `needle` parmi les formes peintes.
    fn find_text(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text().contains(needle) => {
                    Some(text.visual_bounding_rect().center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, needle)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|clipped| walk(&clipped.shape, needle))
    }

    /// Bug #9 : le menu déroulant de destination de « Enregistrer » s'ouvre
    /// sans refermer le sélecteur, on y choisit « utilisateur », et un clic
    /// réellement extérieur referme bien le sélecteur.
    #[test]
    fn un_menu_deroulant_du_selecteur_ne_le_ferme_pas() {
        let context = egui::Context::default();
        let mut env = PickerEnv {
            project: SavedColors::default(),
            user: SavedColors::default(),
            requests: Vec::new(),
        };
        let mut value = "#ff0000".to_string();
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                color_button(
                    ui,
                    "Couleur",
                    &mut value,
                    ColorUsage::Background,
                    Some("Auto"),
                    &mut env,
                );
            });
            output.textures_delta.clear();
            output
        };
        // Ouvre le sélecteur puis laisse la mise en page se stabiliser.
        let mut output = run(vec![]);
        let button = egui::pos2(14.0, 14.0);
        let click = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(vec![egui::Event::PointerMoved(button)]);
        run(vec![click(button, true)]);
        run(vec![click(button, false)]);
        for _ in 0..2 {
            output = run(vec![]);
        }
        let plus = find_text(&output, &tr!("color-custom")).expect("« + » peint");
        run(vec![egui::Event::PointerMoved(plus)]);
        run(vec![click(plus, true)]);
        run(vec![click(plus, false)]);
        for _ in 0..2 {
            output = run(vec![]);
        }
        let combo = find_text(&output, &tr!("color-target-project")).expect("menu déroulant peint");
        run(vec![egui::Event::PointerMoved(combo)]);
        run(vec![click(combo, true)]);
        run(vec![click(combo, false)]);
        output = run(vec![]);
        let entry = find_text(&output, &tr!("color-target-user")).expect("liste du menu ouverte");
        run(vec![egui::Event::PointerMoved(entry)]);
        run(vec![click(entry, true)]);
        run(vec![click(entry, false)]);
        output = run(vec![]);
        assert!(
            find_text(&output, &tr!("color-save")).is_some(),
            "le sélecteur est resté ouvert après le choix dans la liste"
        );
        let away = egui::pos2(700.0, 500.0);
        run(vec![egui::Event::PointerMoved(away)]);
        run(vec![click(away, true)]);
        run(vec![click(away, false)]);
        output = run(vec![]);
        assert!(
            find_text(&output, &tr!("color-save")).is_none(),
            "un clic extérieur referme le sélecteur"
        );
    }

    /// #10 : le sélecteur s'ouvre replié sur la palette (avec l'hexadécimal) ;
    /// le dépli « + Personnalisée… » montre nuancier, opacité, RVB/TSL et
    /// enregistrement ; refermé, il se rouvre replié.
    #[test]
    fn le_selecteur_est_replie_par_defaut_et_se_deplie_a_la_demande() {
        let context = egui::Context::default();
        let mut env = PickerEnv {
            project: SavedColors::default(),
            user: SavedColors::default(),
            requests: Vec::new(),
        };
        let mut value = "#ff0000".to_string();
        let popup_id = std::cell::Cell::new(egui::Id::NULL);
        let mut run = |open: Option<bool>, custom: Option<bool>| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                popup_id.set(ui.make_persistent_id(("stokesforge_color_popup", "Couleur")));
                if let Some(open) = open {
                    set_picker_open(ui.ctx(), popup_id.get(), open);
                }
                if let Some(custom) = custom {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(custom_id(popup_id.get()), custom));
                }
                color_button(
                    ui,
                    "Couleur",
                    &mut value,
                    ColorUsage::Background,
                    Some("Auto"),
                    &mut env,
                );
            });
            output.textures_delta.clear();
            output
        };
        let settle = |run: &mut dyn FnMut(Option<bool>, Option<bool>) -> egui::FullOutput| {
            run(None, None);
            run(None, None)
        };
        run(Some(true), None);
        let replie = settle(&mut run);
        assert!(
            find_text(&replie, &tr!("color-hex")).is_some(),
            "hexa toujours visible"
        );
        assert!(find_text(&replie, &tr!("color-custom")).is_some());
        let derriere_le_plus = [
            tr!("color-opacity"),
            tr!("color-rgb"),
            tr!("color-hsl"),
            tr!("color-save"),
            tr!("color-shape-square"),
        ];
        for cache in &derriere_le_plus {
            assert!(find_text(&replie, cache).is_none(), "{cache} est replié");
        }
        run(None, Some(true));
        let deplie = settle(&mut run);
        for montre in std::iter::once(&tr!("color-hex")).chain(&derriere_le_plus) {
            assert!(find_text(&deplie, montre).is_some(), "{montre} est déplié");
        }
        // Fermé puis rouvert : de nouveau replié.
        run(Some(false), None);
        run(Some(true), None);
        let rouvert = settle(&mut run);
        assert!(find_text(&rouvert, &tr!("color-opacity")).is_none());
    }

    fn luminance([r, g, b]: [u8; 3]) -> f32 {
        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
    }

    /// Les palettes sont distinctes par usage ; celle des courbes suit la
    /// palette nommée choisie, avec ses nuances ; celle des fonds sépare
    /// clairs et sombres.
    #[test]
    fn chaque_palette_de_courbes_a_son_nom_dans_le_catalogue() {
        // Une clé `color-palette-*` par palette de `stokesforge-plot`, dans l'ordre :
        // une palette ajoutée à la table sans sa clé retomberait sur le nom
        // de `stokesforge-plot`, qui n'est pas traduit.
        assert_eq!(
            CURVE_PALETTES.len(),
            9,
            "`curve_palette_name` écrit les neuf palettes : en ajouter une demande sa clé              `color-palette-*` et sa branche"
        );
        for index in 0..CURVE_PALETTES.len() {
            assert!(!curve_palette_name(index).is_empty());
        }
    }

    #[test]
    fn chaque_usage_a_sa_palette() {
        let courbes = palette(ColorUsage::Curves, 0);
        let texte = palette(ColorUsage::Text, 0);
        let fonds = palette(ColorUsage::Background, 0);
        assert_ne!(courbes, texte);
        assert_ne!(texte, fonds);
        assert_eq!(
            courbes[0].colors,
            parse_all(&stokesforge_plot::DEFAULT_PALETTE)
        );
        assert_eq!(courbes.len(), 3, "teintes, foncées, claires");
        let okabe = palette(ColorUsage::Curves, 2);
        assert_eq!(okabe[0].colors, parse_all(CURVE_PALETTES[2].colors));
        assert_eq!(
            palette(ColorUsage::Curves, 999)[0].title,
            curve_palette_name(CURVE_PALETTES.len() - 1),
            "indice ramené dans les bornes"
        );
        assert_eq!(
            fonds[0].colors[0],
            [0xff, 0xff, 0xff, 0],
            "« Aucune couleur » en premier"
        );
        assert!(
            fonds[0].colors[1..]
                .iter()
                .all(|&[r, g, b, _]| luminance([r, g, b]) > 0.85),
            "fonds clairs"
        );
        assert!(
            fonds[1]
                .colors
                .iter()
                .all(|&[r, g, b, _]| luminance([r, g, b]) < 0.2),
            "fonds sombres"
        );
    }
}
