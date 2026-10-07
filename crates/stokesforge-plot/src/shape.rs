//! Les objets graphiques d'un panneau — rectangle, ellipse, ligne, flèche
//! (issue #76), à côté des textes libres ([`crate::AnnotationSpec`]).
//!
//! # Décisions du 2026-10-02
//!
//! - **Ancrage aux données**, comme un texte libre : les deux points définissant
//!   l'objet sont en coordonnées de données du panneau (axe gauche ou droit),
//!   donc l'objet suit le zoom et le déplacement de la vue ;
//! - **une seule liste ordonnée** ([`crate::PanelSpec::shapes`]) : l'ordre
//!   d'empilement est l'ordre de création (le dernier est dessus), modifiable
//!   par [`ShapeRef::bring_to_front`] / [`ShapeRef::send_to_back`]. Les textes
//!   libres sont toujours tracés au-dessus des formes ;
//! - deux points `(x1, y1)` / `(x2, y2)` pour les quatre types : les coins
//!   opposés d'un rectangle ou de la boîte d'une ellipse, les extrémités d'une
//!   ligne, la queue et la pointe d'une flèche. Le sens du glisser est donc
//!   conservé (une flèche tracée de droite à gauche pointe à gauche).
//!
//! Les couleurs suivent la convention du dépôt : `#RRGGBB` ou `#RRGGBBAA` (donc
//! l'opacité du remplissage est l'alpha de [`ShapeSpec::fill_color`]) ; vide =
//! automatique pour le trait (contraste avec le fond), **aucun remplissage**
//! pour le remplissage.

use crate::series::SeriesAxis;
use crate::spec::PlotSpec;

/// La nature d'un objet graphique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// Rectangle aligné aux axes de la figure, défini par deux coins opposés.
    Rectangle,
    /// Ellipse alignée aux axes de la figure, inscrite dans ce rectangle.
    Ellipse,
    /// Segment de droite.
    Line,
    /// Segment de droite terminé par une pointe en `(x2, y2)`.
    Arrow,
}

impl ShapeKind {
    /// Vrai si le type a une surface, donc un remplissage.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse)
    }
}

/// Un objet graphique posé sur un panneau, ancré aux données.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    /// Sa nature.
    pub kind: ShapeKind,
    /// Visible au rendu ; masquer conserve le contenu et son adresse (#81).
    pub visible: bool,
    /// Abscisse du premier point (coin, queue), en données.
    pub x1: f64,
    /// Ordonnée du premier point, en données de l'axe [`Self::axis`].
    pub y1: f64,
    /// Abscisse du second point (coin opposé, pointe), en données.
    pub x2: f64,
    /// Ordonnée du second point, en données de l'axe [`Self::axis`].
    pub y2: f64,
    /// Ordonnée de rattachement : gauche ou droite, comme une série.
    pub axis: SeriesAxis,
    /// Couleur du trait ; vide = automatique.
    pub line_color: String,
    /// Épaisseur du trait, comme [`crate::LineSpec::width`].
    pub line_width: f64,
    /// Couleur de remplissage (rectangle et ellipse) ; vide = aucun.
    pub fill_color: String,
}

impl ShapeSpec {
    /// Épaisseur de trait par défaut.
    pub const DEFAULT_WIDTH: f64 = 1.5;
    /// Épaisseur acceptée au rendu ; au-delà, ramenée dans ces bornes.
    pub const WIDTH_RANGE: (f64, f64) = (0.5, 20.0);

    /// Un objet de `kind` entre `(x1, y1)` et `(x2, y2)`, de l'ordonnée de
    /// gauche, trait automatique, sans remplissage.
    pub fn new(kind: ShapeKind, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            kind,
            visible: true,
            x1,
            y1,
            x2,
            y2,
            axis: SeriesAxis::Left,
            line_color: String::new(),
            line_width: Self::DEFAULT_WIDTH,
            fill_color: String::new(),
        }
    }

    /// L'épaisseur effectivement tracée : [`Self::line_width`] ramenée dans
    /// [`Self::WIDTH_RANGE`], et le défaut pour une valeur non finie.
    pub fn effective_width(&self) -> f64 {
        if self.line_width.is_finite() {
            self.line_width
                .clamp(Self::WIDTH_RANGE.0, Self::WIDTH_RANGE.1)
        } else {
            Self::DEFAULT_WIDTH
        }
    }

    /// Vrai si les quatre coordonnées sont finies ; sinon l'objet est ignoré
    /// au rendu (fichier abîmé) plutôt que de projeter un `NaN`.
    pub fn is_finite(&self) -> bool {
        self.x1.is_finite() && self.y1.is_finite() && self.x2.is_finite() && self.y2.is_finite()
    }
}

/// L'adresse d'un objet graphique dans un [`PlotSpec`] : son calque, par son
/// indice dans [`PlotSpec::panels`], et son rang dans [`crate::PanelSpec::shapes`]
/// (l'ordre d'empilement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeRef {
    /// Indice du calque dans [`PlotSpec::panels`].
    pub panel: usize,
    /// Rang de l'objet dans son panneau.
    pub index: usize,
}

impl ShapeRef {
    fn panel_index(&self, spec: &PlotSpec) -> Option<usize> {
        (self.panel < spec.panels.len()).then_some(self.panel)
    }

    /// L'objet désigné, s'il existe.
    pub fn get<'a>(&self, spec: &'a PlotSpec) -> Option<&'a ShapeSpec> {
        spec.panels
            .get(self.panel_index(spec)?)?
            .shapes
            .get(self.index)
    }

    /// Pose `shape` à cette adresse : remplace l'objet existant, ou l'ajoute
    /// si `index` est le rang qui suit le dernier. Rend `false` sans rien
    /// changer si le panneau n'existe pas ou si `index` est plus loin.
    pub fn set(&self, spec: &mut PlotSpec, shape: ShapeSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        match self.index.cmp(&panel.shapes.len()) {
            std::cmp::Ordering::Less => panel.shapes[self.index] = shape,
            std::cmp::Ordering::Equal => panel.shapes.push(shape),
            std::cmp::Ordering::Greater => return false,
        }
        true
    }

    /// Retire l'objet désigné ; rend `false` s'il n'existe pas.
    pub fn remove(&self, spec: &mut PlotSpec) -> bool {
        let Some(panel) = self
            .panel_index(spec)
            .and_then(|index| spec.panels.get_mut(index))
        else {
            return false;
        };
        if self.index >= panel.shapes.len() {
            return false;
        }
        panel.shapes.remove(self.index);
        true
    }

    /// Met l'objet au-dessus de tous les autres de son panneau. Rend sa
    /// nouvelle adresse, ou `None` s'il n'existe pas.
    pub fn bring_to_front(&self, spec: &mut PlotSpec) -> Option<Self> {
        let panel = spec.panels.get_mut(self.panel_index(spec)?)?;
        if self.index >= panel.shapes.len() {
            return None;
        }
        let shape = panel.shapes.remove(self.index);
        panel.shapes.push(shape);
        Some(Self {
            index: panel.shapes.len() - 1,
            ..*self
        })
    }

    /// Met l'objet sous tous les autres de son panneau. Rend sa nouvelle
    /// adresse, ou `None` s'il n'existe pas.
    pub fn send_to_back(&self, spec: &mut PlotSpec) -> Option<Self> {
        let panel = spec.panels.get_mut(self.panel_index(spec)?)?;
        if self.index >= panel.shapes.len() {
            return None;
        }
        let shape = panel.shapes.remove(self.index);
        panel.shapes.insert(0, shape);
        Some(Self { index: 0, ..*self })
    }

    /// L'adresse du prochain objet ajouté au calque `panel`, ou `None` si ce
    /// calque n'existe pas.
    pub fn next_in(spec: &PlotSpec, panel: usize) -> Option<Self> {
        let source = spec.panels.get(panel)?;
        Some(Self {
            panel,
            index: source.shapes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64) -> ShapeSpec {
        ShapeSpec::new(ShapeKind::Rectangle, x, 0.0, x + 1.0, 1.0)
    }

    #[test]
    fn une_adresse_ajoute_remplace_et_retire() {
        let mut spec = PlotSpec::empty("A");
        let new = ShapeRef::next_in(&spec, 0).expect("panneau 0,0");
        assert_eq!(new.index, 0);
        assert!(new.set(&mut spec, rect(0.0)));
        assert!(new.set(&mut spec, rect(5.0)));
        assert_eq!(spec.panels[0].shapes.len(), 1, "remplacé, pas ajouté");
        assert_eq!(new.get(&spec).map(|s| s.x1), Some(5.0));
        assert!(!ShapeRef { index: 5, ..new }.set(&mut spec, rect(0.0)));
        assert!(ShapeRef::next_in(&spec, 3).is_none());
        assert!(new.remove(&mut spec));
        assert!(!new.remove(&mut spec));
    }

    #[test]
    fn premier_et_arriere_plan_reordonnent_et_rendent_la_nouvelle_adresse() {
        let mut spec = PlotSpec::empty("A");
        for x in [0.0, 1.0, 2.0] {
            let at = ShapeRef::next_in(&spec, 0).unwrap();
            at.set(&mut spec, rect(x));
        }
        let first = ShapeRef { panel: 0, index: 0 };
        let moved = first.bring_to_front(&mut spec).unwrap();
        assert_eq!(moved.index, 2);
        let xs: Vec<f64> = spec.panels[0].shapes.iter().map(|s| s.x1).collect();
        assert_eq!(xs, [1.0, 2.0, 0.0]);
        let back = moved.send_to_back(&mut spec).unwrap();
        assert_eq!(back.index, 0);
        let xs: Vec<f64> = spec.panels[0].shapes.iter().map(|s| s.x1).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0]);
        assert!(ShapeRef { index: 9, ..first }
            .bring_to_front(&mut spec)
            .is_none());
    }

    #[test]
    fn l_epaisseur_est_ramenee_dans_ses_bornes() {
        let mut shape = rect(0.0);
        shape.line_width = 1e9;
        assert_eq!(shape.effective_width(), ShapeSpec::WIDTH_RANGE.1);
        shape.line_width = f64::NAN;
        assert_eq!(shape.effective_width(), ShapeSpec::DEFAULT_WIDTH);
    }
}
