//! Format d'objet : ce que la partie « format d'objet » de la barre « Tools » règle sur un objet
//! graphique sélectionné (issue #64, après #76), ou le trait d’une série
//! sélectionnée depuis Object Manager (#83).
//!
//! Contrairement au format de texte ([`crate::text_format`]), qui écrit des
//! balises dans un texte, ici chaque réglage est un **champ du modèle**
//! ([`stokesforge_plot::ShapeSpec`]) : couleur de trait, couleur de remplissage
//! (dont l'alpha est l'opacité, `#RRGGBBAA`), épaisseur. La barre n'envoie donc
//! que la valeur choisie ; l'application la pose sur l'objet sélectionné par
//! une seule `Action::SetShape`, annulable.
//!
//! Tout est pur, testé sans fenêtre.

use stokesforge_plot::{SeriesSpec, ShapeSpec};

/// Un réglage que la barre sait poser.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectFormat {
    /// Couleur du trait ; vide = automatique (contraste avec le fond).
    LineColor(String),
    /// Couleur de remplissage ; vide = aucun. Sans effet sur une ligne ou une
    /// flèche, qui n'ont pas de surface.
    FillColor(String),
    /// Épaisseur du trait, en points.
    LineWidth(f64),
}

impl ObjectFormat {
    /// Pose les réglages de trait sur la série sélectionnée (#83).
    pub fn apply_series(&self, series: &mut SeriesSpec) -> bool {
        let before = series.clone();
        match self {
            Self::LineColor(color) => series.line.color = color.clone(),
            Self::LineWidth(width) if width.is_finite() && *width > 0.0 => {
                series.line.width = *width
            }
            _ => {}
        }
        *series != before
    }

    /// Pose le réglage sur `shape`. Rend `true` si l'objet a changé.
    pub fn apply(&self, shape: &mut ShapeSpec) -> bool {
        let before = shape.clone();
        match self {
            ObjectFormat::LineColor(color) => shape.line_color = color.clone(),
            ObjectFormat::FillColor(color) => {
                if shape.kind.is_closed() {
                    shape.fill_color = color.clone();
                }
            }
            ObjectFormat::LineWidth(width) => {
                if width.is_finite() && *width > 0.0 {
                    shape.line_width = *width;
                }
            }
        }
        *shape != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::ShapeKind;

    fn shape(kind: ShapeKind) -> ShapeSpec {
        ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0)
    }

    #[test]
    fn chaque_reglage_pose_son_champ_et_dit_s_il_a_change() {
        let mut rect = shape(ShapeKind::Rectangle);
        assert!(ObjectFormat::LineColor("#aa3311".into()).apply(&mut rect));
        assert!(ObjectFormat::FillColor("#33669980".into()).apply(&mut rect));
        assert!(ObjectFormat::LineWidth(3.0).apply(&mut rect));
        assert_eq!(rect.line_color, "#aa3311");
        assert_eq!(rect.fill_color, "#33669980");
        assert_eq!(rect.line_width, 3.0);
        // Le même réglage ne change rien.
        assert!(!ObjectFormat::LineWidth(3.0).apply(&mut rect));
        assert!(!ObjectFormat::LineColor("#aa3311".into()).apply(&mut rect));
    }

    #[test]
    fn une_ligne_ou_une_fleche_n_a_pas_de_remplissage() {
        for kind in [ShapeKind::Line, ShapeKind::Arrow] {
            let mut line = shape(kind);
            assert!(!ObjectFormat::FillColor("#336699".into()).apply(&mut line));
            assert!(line.fill_color.is_empty());
        }
    }

    #[test]
    fn une_epaisseur_invalide_ne_pose_rien() {
        let mut rect = shape(ShapeKind::Rectangle);
        for width in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            assert!(!ObjectFormat::LineWidth(width).apply(&mut rect));
        }
        assert_eq!(rect.line_width, ShapeSpec::DEFAULT_WIDTH);
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;
    #[test]
    fn le_trait_de_la_serie_se_regle_sans_changer_les_symboles() {
        let mut spec = SeriesSpec::new(stokesforge_core::NodeId::from_raw(1));
        let symbol = spec.symbol.clone();
        assert!(ObjectFormat::LineColor("#cc3300".into()).apply_series(&mut spec));
        assert!(ObjectFormat::LineWidth(4.0).apply_series(&mut spec));
        assert_eq!(spec.line.color, "#cc3300");
        assert_eq!(spec.line.width, 4.0);
        assert_eq!(spec.symbol, symbol);
        assert!(!ObjectFormat::FillColor("#cc3300".into()).apply_series(&mut spec));
        assert!(!ObjectFormat::LineWidth(f64::NAN).apply_series(&mut spec));
    }
}
