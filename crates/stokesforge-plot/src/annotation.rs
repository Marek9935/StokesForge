//! Les textes libres d'un panneau — l'outil Texte d'Origin (issue #35).

use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// Un texte posé sur un panneau, **ancré aux données** : il suit le zoom et
/// le déplacement de la vue comme un point de série.
///
/// Le texte est du markup Typst, comme un titre (`$lambda$`, `*gras*`) ; son
/// **centre** est posé au point `(x, y)`, en coordonnées de données du
/// panneau (décision du 2026-10-01). Pas de cadre ni de fond dans cette
/// première version.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationSpec {
    /// Le texte, en markup Typst.
    pub text: String,
    /// Abscisse du centre du texte, en données.
    pub x: f64,
    /// Ordonnée du centre du texte, en données de l'axe [`Self::axis`].
    pub y: f64,
    /// Ordonnée de rattachement : gauche ou droite, comme une série.
    pub axis: SeriesAxis,
    /// Couleur `#RRGGBB` ou `#RRGGBBAA` ; vide = automatique (contraste avec
    /// le fond).
    pub color: String,
    /// Corps du texte, au sens de Plotters (comme les tailles du pipeline :
    /// 16 = titre d'axe).
    pub size: f64,
    /// Rotation en degrés, sens trigonométrique (antihoraire), autour du
    /// centre du texte.
    pub rotation: f64,
    /// Nom de famille de la police (issue #78) ; vide = la police du graphe.
    pub font: String,
}

impl AnnotationSpec {
    /// Corps par défaut : celui des titres d'axe.
    pub const DEFAULT_SIZE: f64 = 16.0;
    /// Corps minimal et maximal accepté au rendu ; au-delà, le corps est
    /// ramené dans ces bornes plutôt que de rastériser une texture démesurée.
    pub const SIZE_RANGE: (f64, f64) = (4.0, 200.0);

    /// Un texte au point `(x, y)` de l'ordonnée de gauche, dans le style
    /// par défaut.
    pub fn new(text: impl Into<String>, x: f64, y: f64) -> Self {
        Self {
            text: text.into(),
            x,
            y,
            axis: SeriesAxis::Left,
            color: String::new(),
            size: Self::DEFAULT_SIZE,
            rotation: 0.0,
            font: String::new(),
        }
    }

    /// Le corps effectivement tracé : [`Self::size`] ramené dans
    /// [`Self::SIZE_RANGE`], et le défaut pour une valeur non finie.
    pub fn effective_size(&self) -> f64 {
        if self.size.is_finite() {
            self.size.clamp(Self::SIZE_RANGE.0, Self::SIZE_RANGE.1)
        } else {
            Self::DEFAULT_SIZE
        }
    }

    /// La rotation effectivement tracée, ramenée dans `]-180, 180]` ; 0 pour
    /// une valeur non finie.
    pub fn effective_rotation(&self) -> f64 {
        if !self.rotation.is_finite() {
            return 0.0;
        }
        let turned = self.rotation.rem_euclid(360.0);
        if turned > 180.0 {
            turned - 360.0
        } else {
            turned
        }
    }
}

/// L'adresse d'un texte libre dans un [`PlotSpec`] : son panneau, par sa
/// position dans la grille (comme le cadrage de vue, qui survit ainsi à un
/// panneau omis faute de place), et son rang dans
/// [`crate::PanelSpec::annotations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnnotationRef {
    /// Ligne du panneau.
    pub row: usize,
    /// Colonne du panneau.
    pub column: usize,
    /// Rang du texte dans son panneau.
    pub index: usize,
}

impl AnnotationRef {
    fn panel<'a>(&self, spec: &'a PlotSpec) -> Option<&'a crate::PanelSpec> {
        spec.panels
            .iter()
            .find(|panel| panel.row == self.row && panel.column == self.column)
    }

    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        spec.panels
            .iter()
            .position(|panel| panel.row == self.row && panel.column == self.column)
    }

    /// Le texte désigné, s'il existe.
    pub fn get<'a>(&self, spec: &'a PlotSpec) -> Option<&'a AnnotationSpec> {
        self.panel(spec)?.annotations.get(self.index)
    }

    /// Pose `annotation` à cette adresse : remplace le texte existant, ou
    /// l'ajoute si `index` est le rang qui suit le dernier. Rend `false` sans
    /// rien changer si le panneau n'existe pas ou si `index` est plus loin.
    pub fn set(&self, spec: &mut PlotSpec, annotation: AnnotationSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        match self.index.cmp(&panel.annotations.len()) {
            std::cmp::Ordering::Less => panel.annotations[self.index] = annotation,
            std::cmp::Ordering::Equal => panel.annotations.push(annotation),
            std::cmp::Ordering::Greater => return false,
        }
        true
    }

    /// Retire le texte désigné ; rend `false` s'il n'existe pas.
    pub fn remove(&self, spec: &mut PlotSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        if self.index >= panel.annotations.len() {
            return false;
        }
        panel.annotations.remove(self.index);
        true
    }

    /// L'adresse du prochain texte ajouté au panneau `(row, column)`, ou
    /// `None` si ce panneau n'existe pas.
    pub fn next_in(spec: &PlotSpec, row: usize, column: usize) -> Option<Self> {
        let panel = spec
            .panels
            .iter()
            .find(|panel| panel.row == row && panel.column == column)?;
        Some(Self {
            row,
            column,
            index: panel.annotations.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_adresse_ajoute_remplace_et_retire() {
        let mut spec = PlotSpec::empty("A");
        let new = AnnotationRef::next_in(&spec, 0, 0).expect("panneau 0,0");
        assert_eq!(new.index, 0);
        assert!(new.set(&mut spec, AnnotationSpec::new("a", 1.0, 2.0)));
        assert_eq!(new.get(&spec).map(|a| a.text.as_str()), Some("a"));
        assert!(new.set(&mut spec, AnnotationSpec::new("b", 1.0, 2.0)));
        assert_eq!(spec.panels[0].annotations.len(), 1, "remplacé, pas ajouté");
        let loin = AnnotationRef { index: 5, ..new };
        assert!(!loin.set(&mut spec, AnnotationSpec::new("c", 0.0, 0.0)));
        assert!(AnnotationRef::next_in(&spec, 3, 3).is_none());
        assert!(new.remove(&mut spec));
        assert!(!new.remove(&mut spec));
        assert!(spec.panels[0].annotations.is_empty());
    }

    #[test]
    fn corps_et_rotation_sont_ramenes_dans_leurs_bornes() {
        let mut a = AnnotationSpec::new("x", 0.0, 0.0);
        a.size = 1e9;
        assert_eq!(a.effective_size(), AnnotationSpec::SIZE_RANGE.1);
        a.size = f64::NAN;
        assert_eq!(a.effective_size(), AnnotationSpec::DEFAULT_SIZE);
        a.rotation = 270.0;
        assert_eq!(a.effective_rotation(), -90.0);
        a.rotation = -540.0;
        assert_eq!(a.effective_rotation(), 180.0);
        a.rotation = f64::INFINITY;
        assert_eq!(a.effective_rotation(), 0.0);
    }
}
