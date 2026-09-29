//! Sélecteur de couleur de PlotYx : un bouton-échantillon qui ouvre une
//! fenêtre surgissante avec une **palette** adaptée à ce qu'on colore, un
//! **nuancier** (carré saturation × luminosité et barre de teinte), et la
//! saisie en **RVB**, **TSL** et **hexadécimal**.
//!
//! # Trois palettes, pas une
//!
//! Une courbe, un texte et un fond n'ont pas besoin des mêmes couleurs
//! ([`PaletteKind`]) :
//!
//! - **courbes** : la palette d'attribution automatique des séries
//!   ([`plotyx_plot::DEFAULT_PALETTE`]), des teintes contrastées entre elles,
//!   avec leurs nuances foncées et claires calculées en TSL ;
//! - **texte** : des encres lisibles — noirs, gris, blanc, et des teintes
//!   foncées ;
//! - **fonds** : des teintes claires, et une rangée de fonds **sombres**. Un
//!   fond sombre fait passer d'un coup en blanc tout ce qui est en couleur
//!   automatique (titre, axes, graduations, légende) — la règle vit dans le
//!   rendu (`plotyx_render::color::auto_ink`), commune à l'écran et à
//!   l'export.
//!
//! # « Automatique »
//!
//! Dans le modèle, une couleur est une chaîne `#RRGGBB` ; la chaîne vide veut
//! dire « automatique » (encre contrastée avec le fond, couleur du trait pour
//! un marqueur…). Le bouton propose alors une entrée « Automatique » qui
//! remet la chaîne à vide.
//!
//! # Quand la couleur est-elle validée
//!
//! La chaîne est modifiée en direct (retour visuel immédiat dans le
//! graphe), et [`color_button`] rend `true` quand l'interaction est finie :
//! clic sur un échantillon ou sur « Automatique », fin d'un glisser dans le
//! nuancier, champ numérique ou hexadécimal validé, fermeture de la fenêtre
//! surgissante. C'est la même règle anti-inflation d'historique que le reste
//! de la fenêtre des propriétés (`crate::app::plot_settings_window`) : une
//! seule entrée d'historique par geste, pas une par pixel.
//!
//! Les conversions (hexadécimal, RVB ↔ TSL, RVB ↔ TSV) sont des fonctions
//! pures, testées sans fenêtre.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

/// Ce que la couleur colore — choisit la palette proposée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteKind {
    /// Trait et marqueur d'une courbe.
    Curves,
    /// Titre, titres d'axes, graduations.
    Text,
    /// Fond du graphe.
    Background,
}

/// Une rangée nommée d'échantillons.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteSection {
    /// Titre de la rangée.
    pub title: &'static str,
    /// Couleurs, en RVB.
    pub colors: Vec<[u8; 3]>,
}

/// Encres de la palette « texte ».
const TEXT_INKS: [&str; 12] = [
    "#000000", "#202020", "#404040", "#6b6b6b", "#a0a0a0", "#ffffff", "#1f2a44", "#1f4e79",
    "#8b1a1a", "#1e5631", "#4b2a6b", "#5c3a1e",
];

/// Fonds clairs de la palette « fonds ».
const LIGHT_BACKGROUNDS: [&str; 8] = [
    "#ffffff", "#fafaf7", "#f5f0e1", "#f4ecd8", "#f0f0f0", "#e6e6e6", "#eef4fb", "#eef7ee",
];

/// Fonds sombres de la palette « fonds » : texte automatique en blanc.
const DARK_BACKGROUNDS: [&str; 6] = [
    "#000000", "#1e1e1e", "#2b2f36", "#101820", "#14213d", "#102a1f",
];

/// Les rangées de la palette `kind`.
pub fn palette(kind: PaletteKind) -> Vec<PaletteSection> {
    let parse_all = |colors: &[&str]| colors.iter().filter_map(|hex| parse_hex(hex)).collect();
    match kind {
        PaletteKind::Curves => {
            let base: Vec<[u8; 3]> = parse_all(&plotyx_plot::DEFAULT_PALETTE);
            let shade = |factor: fn(f32) -> f32| {
                base.iter()
                    .map(|&rgb| {
                        let (h, s, l) = rgb_to_hsl(rgb);
                        hsl_to_rgb(h, s, factor(l))
                    })
                    .collect()
            };
            vec![
                PaletteSection {
                    title: "Couleurs des courbes",
                    colors: base.clone(),
                },
                PaletteSection {
                    title: "Nuances foncées",
                    colors: shade(|l| l * 0.62),
                },
                PaletteSection {
                    title: "Nuances claires",
                    colors: shade(|l| l + (100.0 - l) * 0.45),
                },
            ]
        }
        PaletteKind::Text => vec![PaletteSection {
            title: "Encres",
            colors: parse_all(&TEXT_INKS),
        }],
        PaletteKind::Background => vec![
            PaletteSection {
                title: "Fonds clairs",
                colors: parse_all(&LIGHT_BACKGROUNDS),
            },
            PaletteSection {
                title: "Fonds sombres (texte automatique en blanc)",
                colors: parse_all(&DARK_BACKGROUNDS),
            },
        ],
    }
}

/// Les composantes d'une couleur `#RRGGBB` (casse indifférente, `#`
/// facultatif, espaces de bordure ignorés), ou `None`.
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let text = text.trim();
    let text = text.strip_prefix('#').unwrap_or(text);
    if text.len() != 6 || !text.is_ascii() {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(&text[index..index + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// La chaîne `#rrggbb` d'une couleur.
pub fn to_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
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

/// Un libellé suivi de [`color_button`], sur une ligne.
pub fn color_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    kind: PaletteKind,
    auto_label: Option<&str>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        color_button(ui, label, value, kind, auto_label)
    })
    .inner
}

/// Taille de l'échantillon du bouton.
const BUTTON_SWATCH: Vec2 = Vec2::new(44.0, 18.0);
/// Taille d'un échantillon de palette.
const PALETTE_SWATCH: f32 = 18.0;
/// Côté du carré du nuancier.
const SQUARE_SIDE: f32 = 200.0;
/// Hauteur de la barre de teinte.
const HUE_BAR_HEIGHT: f32 = 16.0;
/// Finesse du maillage du nuancier (sommets par côté).
const MESH_STEPS: usize = 24;

/// État du nuancier entre deux images : la couleur en TSV, gardée à part de
/// la chaîne pour ne pas perdre la teinte sur un gris (où elle n'est pas
/// définie) pendant qu'on glisse dans le carré.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PickerMemory {
    /// La chaîne à laquelle `hsv` correspond ; si le modèle change (autre
    /// champ, annulation), la mémoire est recalculée depuis la chaîne.
    hex: [u8; 3],
    hsv: (f32, f32, f32),
}

/// Le bouton-échantillon et sa fenêtre surgissante. `value` est la chaîne
/// `#RRGGBB` du modèle (vide = automatique quand `auto_label` est donné).
/// Rend `true` quand un geste vient de se terminer — voir la doc du module.
pub fn color_button(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    kind: PaletteKind,
    auto_label: Option<&str>,
) -> bool {
    let current = parse_hex(value);
    let (rect, response) = ui.allocate_exact_size(BUTTON_SWATCH, Sense::click());
    let response = response.on_hover_text(match current {
        Some(rgb) => to_hex(rgb),
        None => auto_label.unwrap_or("Automatique").to_string(),
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let stroke = ui.visuals().widgets.inactive.fg_stroke;
        match current {
            Some([r, g, b]) => {
                painter.rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            }
            None => {
                painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Auto",
                    egui::FontId::proportional(11.0),
                    stroke.color,
                );
            }
        }
        painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    }

    let popup_id = ui.make_persistent_id(("plotyx_color_popup", id_salt));
    let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let mut commit = false;
    egui::Popup::from_toggle_button_response(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(SQUARE_SIDE + 24.0);
            commit |= picker_contents(ui, popup_id, value, kind, auto_label);
        });
    let is_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    // Refermer la fenêtre surgissante valide ce qui a pu être modifié sans
    // geste de fin explicite (champ encore en saisie).
    commit || (was_open && !is_open)
}

/// Contenu de la fenêtre surgissante : palette, « Automatique », nuancier,
/// champs numériques et hexadécimal.
fn picker_contents(
    ui: &mut egui::Ui,
    popup_id: egui::Id,
    value: &mut String,
    kind: PaletteKind,
    auto_label: Option<&str>,
) -> bool {
    let mut commit = false;
    let mut close = false;

    for section in palette(kind) {
        ui.label(egui::RichText::new(section.title).small().strong());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(3.0);
            for rgb in section.colors {
                let selected = parse_hex(value) == Some(rgb);
                if palette_swatch(ui, rgb, selected).clicked() {
                    *value = to_hex(rgb);
                    commit = true;
                    close = true;
                }
            }
        });
        ui.add_space(2.0);
    }
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
    ui.separator();

    // Le nuancier part de la couleur courante, ou d'un gris moyen pour une
    // couleur automatique (rien à montrer).
    let rgb = parse_hex(value).unwrap_or([0x80, 0x80, 0x80]);
    let memory_id = popup_id.with("hsv");
    let mut memory = ui
        .memory(|memory| memory.data.get_temp::<PickerMemory>(memory_id))
        .filter(|memory| memory.hex == rgb)
        .unwrap_or(PickerMemory {
            hex: rgb,
            hsv: rgb_to_hsv(rgb),
        });

    let (h, s, v) = &mut memory.hsv;
    let square = sv_square(ui, *h, s, v);
    let hue = hue_bar(ui, h);
    let mut new_rgb = None;
    if square.changed || hue.changed {
        new_rgb = Some(hsv_to_rgb(memory.hsv.0, memory.hsv.1, memory.hsv.2));
    }
    commit |= square.finished || hue.finished;

    ui.add_space(4.0);
    let shown = new_rgb.unwrap_or(rgb);
    // RVB.
    let mut channels = shown.map(i32::from);
    let mut rgb_edit = NumericEdit::default();
    ui.horizontal(|ui| {
        ui.label("RVB");
        for (channel, prefix) in channels.iter_mut().zip(["R ", "V ", "B "]) {
            rgb_edit.track(
                ui.add(
                    egui::DragValue::new(channel)
                        .range(0..=255)
                        .speed(0.5)
                        .prefix(prefix),
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

    // TSL.
    let (mut th, mut ts, mut tl) = rgb_to_hsl(new_rgb.unwrap_or(rgb));
    let mut hsl_edit = NumericEdit::default();
    ui.horizontal(|ui| {
        ui.label("TSL");
        hsl_edit.track(
            ui.add(
                egui::DragValue::new(&mut th)
                    .range(0.0..=360.0)
                    .speed(1.0)
                    .max_decimals(0)
                    .prefix("T ")
                    .suffix("°"),
            ),
        );
        hsl_edit.track(
            ui.add(
                egui::DragValue::new(&mut ts)
                    .range(0.0..=100.0)
                    .speed(0.5)
                    .max_decimals(0)
                    .prefix("S ")
                    .suffix(" %"),
            ),
        );
        hsl_edit.track(
            ui.add(
                egui::DragValue::new(&mut tl)
                    .range(0.0..=100.0)
                    .speed(0.5)
                    .max_decimals(0)
                    .prefix("L ")
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

    // Hexadécimal : le texte saisi n'est appliqué que lisible, à la
    // validation.
    let hex_id = popup_id.with("hex");
    let mut hex_text: String = ui
        .memory(|memory| memory.data.get_temp(hex_id))
        .unwrap_or_else(|| to_hex(new_rgb.unwrap_or(rgb)));
    let hex_response = ui
        .horizontal(|ui| {
            ui.label("Hexa");
            ui.add(egui::TextEdit::singleline(&mut hex_text).desired_width(80.0))
        })
        .inner;
    if hex_response.has_focus() {
        ui.memory_mut(|memory| memory.data.insert_temp(hex_id, hex_text.clone()));
    } else {
        ui.memory_mut(|memory| memory.data.remove::<String>(hex_id));
    }
    if hex_response.lost_focus() {
        if let Some(parsed) = parse_hex(&hex_text) {
            new_rgb = Some(parsed);
            memory.hsv = rgb_to_hsv(parsed);
            commit = true;
        }
    }

    if let Some(edited) = new_rgb {
        *value = to_hex(edited);
        memory.hex = edited;
    }
    ui.memory_mut(|m| m.data.insert_temp(memory_id, memory));
    if close {
        egui::Popup::close_id(ui.ctx(), popup_id);
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

/// Un échantillon de palette cliquable.
fn palette_swatch(ui: &mut egui::Ui, [r, g, b]: [u8; 3], selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PALETTE_SWATCH), Sense::click());
    let response = response.on_hover_text(to_hex([r, g, b]));
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
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

fn gesture(response: &egui::Response) -> PickerGesture {
    PickerGesture {
        changed: false,
        finished: response.drag_stopped() || response.clicked(),
    }
}

/// Le carré saturation (horizontale) × valeur (verticale) à la teinte `h`.
fn sv_square(ui: &mut egui::Ui, h: f32, s: &mut f32, v: &mut f32) -> PickerGesture {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(SQUARE_SIDE), Sense::click_and_drag());
    let mut result = gesture(&response);
    if let Some(pos) = response.interact_pointer_pos() {
        if response.is_pointer_button_down_on() || response.clicked() {
            let new_s = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new_v = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            if (new_s, new_v) != (*s, *v) {
                (*s, *v) = (new_s, new_v);
                result.changed = true;
            }
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

/// La barre de teinte, de 0 à 360°.
fn hue_bar(ui: &mut egui::Ui, h: &mut f32) -> PickerGesture {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(SQUARE_SIDE, HUE_BAR_HEIGHT),
        Sense::click_and_drag(),
    );
    let mut result = gesture(&response);
    if let Some(pos) = response.interact_pointer_pos() {
        if response.is_pointer_button_down_on() || response.clicked() {
            let new_h = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if new_h != *h {
                *h = new_h;
                result.changed = true;
            }
        }
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        for step in 0..=MESH_STEPS {
            let f = step as f32 / MESH_STEPS as f32;
            let [r, g, b] = hsv_to_rgb(f, 1.0, 1.0);
            let color = Color32::from_rgb(r, g, b);
            let x = rect.left() + f * rect.width();
            mesh.colored_vertex(Pos2::new(x, rect.top()), color);
            mesh.colored_vertex(Pos2::new(x, rect.bottom()), color);
            if step < MESH_STEPS {
                let i = (step * 2) as u32;
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i + 1, i + 3, i + 2);
            }
        }
        let painter = ui.painter();
        painter.add(egui::Shape::mesh(mesh));
        let x = rect.left() + *h * rect.width();
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
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_hexadecimal_se_lit_et_s_ecrit() {
        assert_eq!(parse_hex("#4C78a8"), Some([0x4c, 0x78, 0xa8]));
        assert_eq!(parse_hex(" 4c78a8 "), Some([0x4c, 0x78, 0xa8]));
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#abc"), None);
        assert_eq!(parse_hex("#12345g"), None);
        assert_eq!(to_hex([0x4c, 0x78, 0xa8]), "#4c78a8");
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

    fn luminance([r, g, b]: [u8; 3]) -> f32 {
        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
    }

    /// Les trois palettes sont distinctes, et chacune est faite pour son
    /// usage : la palette des courbes commence par les couleurs
    /// d'attribution automatique, celle des fonds sépare clairs et sombres.
    #[test]
    fn chaque_usage_a_sa_palette() {
        let courbes = palette(PaletteKind::Curves);
        let texte = palette(PaletteKind::Text);
        let fonds = palette(PaletteKind::Background);
        assert_ne!(courbes, texte);
        assert_ne!(texte, fonds);
        assert_eq!(
            courbes[0].colors,
            plotyx_plot::DEFAULT_PALETTE
                .iter()
                .filter_map(|hex| parse_hex(hex))
                .collect::<Vec<_>>()
        );
        assert!(courbes.iter().all(|section| section.colors.len() == 10));
        assert!(
            fonds[0].colors.iter().all(|&c| luminance(c) > 0.85),
            "fonds clairs"
        );
        assert!(
            fonds[1].colors.iter().all(|&c| luminance(c) < 0.2),
            "fonds sombres"
        );
    }
}
