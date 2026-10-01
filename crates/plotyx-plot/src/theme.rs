//! Thèmes : régler d'un geste les couleurs globales d'un graphe (issue #11).
//!
//! Un thème n'est **pas** un champ du [`PlotSpec`] : appliquer un thème ou une
//! couleur globale écrit la même couleur dans tous les champs locaux
//! (titre du graphe, titre et échelle de chaque axe de chaque panneau, fond).
//! Le dernier réglage écrase donc ce qui avait été réglé une à une, et rien
//! n'est mémorisé au-delà des champs eux-mêmes — pas de changement du format
//! `.plx`. Une couleur vide reste « automatique » : le rendu la résout par
//! `plotyx_render::color::auto_ink` (blanc sur fond sombre).

use crate::{AxisSide, PlotSpec};

/// Un thème prédéfini : un fond, une couleur de texte, une couleur d'axes.
/// Une chaîne vide veut dire « automatique » (`""`) pour le texte et les
/// axes, et « blanc » pour le fond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Nom affiché.
    pub name: &'static str,
    /// Fond du graphe (`#RRGGBB[AA]`).
    pub background: &'static str,
    /// Couleur du texte : titres du graphe et des axes.
    pub text: &'static str,
    /// Couleur des axes : ligne, graduations, libellés et grille.
    pub axes: &'static str,
}

/// Les thèmes prédéfinis. Texte et axes restent en automatique : ils suivent
/// le fond (encre sombre sur fond clair, blanche sur fond sombre).
pub const THEMES: [Theme; 2] = [
    Theme {
        name: "Clair",
        background: "#ffffff",
        text: "",
        axes: "",
    },
    Theme {
        name: "Sombre",
        background: "#1e1e1e",
        text: "",
        axes: "",
    },
];

const SIDES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

impl PlotSpec {
    /// Couleur globale du texte telle que l'onglet « Thème » la montre : celle
    /// du titre du graphe.
    pub fn theme_text_color(&self) -> &str {
        &self.title_color
    }

    /// Couleur globale des axes telle que l'onglet « Thème » la montre : celle
    /// de l'échelle de l'axe du bas du premier panneau.
    pub fn theme_axes_color(&self) -> &str {
        self.panels
            .first()
            .map_or("", |panel| panel.bottom.tick_color.as_str())
    }

    /// Écrit `color` comme couleur du texte : titre du graphe, titre de
    /// chaque axe et textes libres (issue #35) de chaque panneau.
    pub fn set_text_color(&mut self, color: &str) {
        self.title_color = color.to_string();
        for panel in self.panels.iter_mut() {
            for side in SIDES {
                panel.axis_mut(side).title_color = color.to_string();
            }
            for annotation in &mut panel.annotations {
                annotation.color = color.to_string();
            }
        }
    }

    /// Écrit `color` comme couleur des axes : échelle (ligne, graduations,
    /// libellés, grille) de chaque axe de chaque panneau.
    pub fn set_axes_color(&mut self, color: &str) {
        for panel in self.panels.iter_mut() {
            for side in SIDES {
                panel.axis_mut(side).tick_color = color.to_string();
            }
        }
    }

    /// Applique un thème prédéfini en entier.
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.background = theme.background.to_string();
        self.set_text_color(theme.text);
        self.set_axes_color(theme.axes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanelSpec;

    fn spec_a_deux_panneaux() -> PlotSpec {
        let mut spec = PlotSpec::empty("Figure").with_panels(PanelSpec::at(0, 0));
        spec.panels.push(PanelSpec::at(0, 1));
        spec
    }

    #[test]
    fn une_couleur_globale_ecrase_les_reglages_locaux_partout() {
        let mut spec = spec_a_deux_panneaux();
        spec.panels.get_mut(1).unwrap().left.title_color = "#ff0000".to_string();
        spec.panels.get_mut(0).unwrap().bottom.tick_color = "#00ff00".to_string();
        spec.set_text_color("#123456");
        spec.set_axes_color("#654321");
        assert_eq!(spec.title_color, "#123456");
        assert_eq!(spec.theme_text_color(), "#123456");
        assert_eq!(spec.theme_axes_color(), "#654321");
        for panel in spec.panels.as_slice() {
            for side in SIDES {
                assert_eq!(panel.axis(side).title_color, "#123456");
                assert_eq!(panel.axis(side).tick_color, "#654321");
            }
        }
    }

    #[test]
    fn un_theme_regle_fond_texte_et_axes() {
        let mut spec = spec_a_deux_panneaux();
        spec.set_text_color("#ff0000");
        spec.apply_theme(&THEMES[1]);
        assert_eq!(spec.background, "#1e1e1e");
        assert_eq!(spec.theme_text_color(), "", "texte automatique");
        assert_eq!(spec.panels.as_slice()[1].right.title_color, "");
        spec.apply_theme(&THEMES[0]);
        assert_eq!(spec.background, "#ffffff");
    }
}
