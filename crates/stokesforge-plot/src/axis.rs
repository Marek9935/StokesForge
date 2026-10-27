//! Les quatre axes d'un panneau : échelle, limites, grille.

use crate::text_anchor::TextAnchor;

/// Échelle d'un axe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AxisScale {
    /// Échelle linéaire (défaut).
    #[default]
    Linear,
    /// Échelle logarithmique.
    Log,
}

/// Côté du panneau où vit un axe. Les quatre existent ; une série, elle, ne
/// se rattache qu'à deux d'entre eux — voir [`crate::SeriesAxis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscisse, en bas.
    Bottom,
    /// Ordonnée principale, à gauche.
    Left,
    /// Ordonnée secondaire, à droite.
    Right,
    /// Abscisse secondaire, en haut.
    Top,
}

/// Sens des tirets d'un axe (#344) : hors de la zone de tracé (défaut, comme
/// Plotters), vers l'intérieur, ou des deux côtés de la ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TickDirection {
    /// Vers l'extérieur du cadre (défaut).
    #[default]
    Out,
    /// Vers l'intérieur de la zone de tracé.
    In,
    /// De part et d'autre de la ligne.
    Both,
}

/// Un axe : titre, unité, échelle, limites.
///
/// [`AxisSpec::auto_range`] prime sur `range_min`/`range_max` : les bornes
/// restent mémorisées quand on repasse en automatique, pour qu'un aller-retour
/// dans le panneau de propriétés ne les perde pas. C'est pour ça que les
/// bornes ne sont pas fondues dans un unique `Option<(f64, f64)>` — la
/// représentation serait plus serrée mais perdrait ce que l'utilisateur avait
/// saisi.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisSpec {
    /// Titre de l'axe, sans l'unité.
    pub label: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    pub title_visible: bool,
    /// Unité affichée entre parenthèses après le titre.
    pub unit: String,
    /// Échelle.
    pub scale: AxisScale,
    /// Cadrage automatique sur les données.
    pub auto_range: bool,
    /// Borne basse mémorisée.
    pub range_min: Option<f64>,
    /// Borne haute mémorisée.
    pub range_max: Option<f64>,
    /// Grille sur cet axe.
    pub grid: bool,
    /// Axe affiché.
    pub visible: bool,
    /// Axe inversé (valeurs décroissantes).
    pub inverted: bool,
    /// Couleur du titre de l'axe (`#RRGGBB`). Vide = automatique, voir
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Position du titre de l'axe s'il a été déplacé (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Police et corps du titre de l'axe (issue #78).
    pub title_font: crate::TextFont,
    /// Couleur de l'échelle — ligne d'axe, graduations et leurs libellés, et
    /// la grille, atténuée (`#RRGGBB`). Vide = automatique.
    pub tick_color: String,
    /// Axe miroir (#176) : sur un calque ordinaire, l'axe du haut ou de droite
    /// reprend l'échelle, les bornes et les graduations de celui du bas ou de
    /// gauche, sans titre. Sans effet sur les autres axes, ni sur l'axe de
    /// droite quand une série y est rattachée (c'est alors un vrai axe).
    pub mirror: bool,
    /// Libellés des graduations affichés ; faux = la ligne et les tirets
    /// seulement (#176).
    pub tick_labels: bool,
    /// Tirets des graduations tracés ; faux = la ligne et les nombres
    /// seulement, l'écart des nombres est conservé (#344).
    pub ticks: bool,
    /// Sens des tirets (#344).
    pub tick_direction: TickDirection,
    /// Sous-graduations tracées entre les graduations principales (#344),
    /// avec ou sans les tirets principaux. Vrai pour un axe neuf ; un ancien
    /// fichier, qui n'en avait pas, se relit faux.
    pub minor_ticks: bool,
    /// Sous-graduations entre deux graduations principales (#344) ; `None` =
    /// automatique (4 ou 5 subdivisions selon le pas). Sans effet en échelle
    /// logarithmique, où ce sont les multiples 2 à 9.
    pub minor_tick_count: Option<u8>,
    /// Un nombre sur `label_every` est écrit, le premier toujours (#344) ; 1 =
    /// tous. 0 se lit comme 1.
    pub label_every: u8,
    /// Longueur des grands tirets en pixels de figure (#344, section
    /// « Avancé ») ; `None` = automatique. La valeur brute est gardée telle
    /// quelle : le rendu la borne.
    pub tick_length: Option<f64>,
    /// Longueur des petits tirets en pixels de figure ; `None` = la moitié du
    /// grand tiret effectif.
    pub minor_tick_length: Option<f64>,
    /// Épaisseur des tirets en points ; `None` = automatique (1).
    pub tick_width: Option<f64>,
    /// Couleur des tirets (`#RRGGBB`), distincte de [`AxisSpec::tick_color`]
    /// (ligne et nombres). Vide = la couleur de l'échelle.
    pub tick_mark_color: String,
}

impl AxisSpec {
    /// Axe masqué — la valeur par défaut de l'axe du haut d'un panneau.
    pub fn hidden() -> Self {
        Self {
            visible: false,
            ..Self::default()
        }
    }

    /// Titre affiché : `label (unité)`, sans parenthèses vides.
    pub fn display_label(&self) -> String {
        match (self.label.is_empty(), self.unit.is_empty()) {
            (false, false) => format!("{} ({})", self.label, self.unit),
            (false, true) => self.label.clone(),
            (true, false) => format!("({})", self.unit),
            (true, true) => String::new(),
        }
    }

    /// L'axe dans sa forme canonique : le sens se lit sur l'ordre des bornes
    /// (De > À = axe inversé), le champ [`AxisSpec::inverted`] n'est que sa
    /// trace dans le fichier.
    ///
    /// `inverted` devient vrai quand les bornes sont **utilisées** et que
    /// `range_min > range_max` ; un axe inversé remet ses bornes mémorisées
    /// dans l'ordre décroissant, que `auto_range` soit vrai ou non (en
    /// automatique, les bornes mémorisées sont inertes, mais elles reprennent
    /// le sens de l'axe). Des bornes mémorisées décroissantes sous
    /// `auto_range` n'inversent donc rien : l'axe d'origine ne les lisait
    /// pas. Une borne manquante ou non finie ne bouge pas. **Le rendu est
    /// identique** à celui de l'axe d'origine (`axis_range` lit « l'un ou
    /// l'autre »).
    #[must_use]
    pub fn orient(&self) -> Self {
        let mut axis = self.clone();
        if let (Some(min), Some(max)) = (axis.range_min, axis.range_max) {
            if !(min.is_finite() && max.is_finite()) {
                return axis;
            }
            if min > max {
                axis.inverted |= !axis.auto_range;
            } else if axis.inverted && min < max {
                axis.range_min = Some(max);
                axis.range_max = Some(min);
            }
        }
        axis
    }

    /// Vrai quand le cadrage manuel est à la fois demandé et complet : les
    /// deux bornes sont là et l'automatique est coupé.
    pub fn has_explicit_range(&self) -> bool {
        !self.auto_range && self.range_min.is_some() && self.range_max.is_some()
    }
}

impl Default for AxisSpec {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScale::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            tick_color: String::new(),
            mirror: false,
            tick_labels: true,
            ticks: true,
            tick_direction: TickDirection::Out,
            minor_ticks: true,
            minor_tick_count: None,
            label_every: 1,
            tick_length: None,
            minor_tick_length: None,
            tick_width: None,
            tick_mark_color: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(auto: bool, inverted: bool, bounds: (Option<f64>, Option<f64>)) -> AxisSpec {
        AxisSpec {
            auto_range: auto,
            inverted,
            range_min: bounds.0,
            range_max: bounds.1,
            ..AxisSpec::default()
        }
    }

    /// Table : (auto, inversé, bornes) -> (inversé, bornes) après `orient`.
    #[test]
    fn orient_lit_le_sens_sur_l_ordre_des_bornes() {
        let b = |a, z| (Some(a), Some(z));
        let table = [
            // explicite ou automatique : mêmes règles sur les bornes mémorisées.
            (false, false, b(1.0, 5.0), false, b(1.0, 5.0)),
            (false, false, b(5.0, 1.0), true, b(5.0, 1.0)),
            (false, true, b(1.0, 5.0), true, b(5.0, 1.0)),
            (false, true, b(5.0, 1.0), true, b(5.0, 1.0)),
            (true, false, b(1.0, 5.0), false, b(1.0, 5.0)),
            (true, false, b(5.0, 1.0), false, b(5.0, 1.0)),
            (true, true, b(1.0, 5.0), true, b(5.0, 1.0)),
            (true, true, b(5.0, 1.0), true, b(5.0, 1.0)),
            // bornes égales, absentes ou partielles : le sens ne change pas.
            (false, true, b(3.0, 3.0), true, b(3.0, 3.0)),
            (true, true, (None, None), true, (None, None)),
            (true, false, (None, None), false, (None, None)),
            (false, true, (Some(1.0), None), true, (Some(1.0), None)),
            (
                false,
                false,
                (Some(f64::INFINITY), Some(1.0)),
                false,
                (Some(f64::INFINITY), Some(1.0)),
            ),
        ];
        for (auto, inverted, bounds, want_inverted, want_bounds) in table {
            let got = axis(auto, inverted, bounds).orient();
            assert_eq!(
                (got.inverted, (got.range_min, got.range_max)),
                (want_inverted, want_bounds),
                "auto={auto} inversé={inverted} bornes={bounds:?}"
            );
            assert_eq!(got.auto_range, auto, "orient ne touche pas auto_range");
            assert_eq!(got.orient(), got, "idempotente");
        }
    }

    #[test]
    fn hidden_only_flips_visibility() {
        let hidden = AxisSpec::hidden();
        assert!(!hidden.visible);
        assert_eq!(
            AxisSpec {
                visible: true,
                ..hidden
            },
            AxisSpec::default()
        );
    }
}
