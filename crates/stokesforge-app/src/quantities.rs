//! La table des grandeurs (#220, N8) : pour chaque grandeur numérique de
//! l'interface, une seule définition de ses bornes, de ses pas, de ses
//! décimales et des valeurs qu'on propose.
//!
//! Un champ numérique ne réécrit jamais `0.0..=20.0` ni `speed(0.1)` : il
//! nomme sa grandeur ici et la brique [`crate::number_field`] fait le reste.
//! Changer la plage de l'épaisseur du trait, c'est changer une ligne de ce
//! fichier, pour tous les champs d'épaisseur à la fois.
//!
//! L'**unité** n'est pas dans la table : elle se dit dans le libellé du champ
//! (« Line width (pt) »), clé du catalogue (N10, `docs/textes-interface.md`).
//! Un `▼` n'apparaît que si la grandeur a des [`Quantity::presets`] ; une borne
//! d'axe ou un paramètre d'ajustement n'en ont pas.
//!
//! Les plafonds d'un réglage d'analyse (degré, itérations, pics, composantes)
//! ne s'écrivent pas ici : ils sont lus dans `stokesforge_analysis::ops`
//! (#260), la source que `Operation::validate` applique aussi pour `pyst` et le
//! fichier.

use stokesforge_analysis::ops as limits;

/// Ce qu'une grandeur fait du mot `auto` et d'un champ vidé (#221, C4/R10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoKind {
    /// Pas d'Auto : `auto` et un champ vidé sont refusés (N6).
    None,
    /// La valeur elle-même a un Auto : un champ vidé ou `auto` la rend
    /// automatique, et le champ affiche alors « Auto (…) » en gris.
    Field,
    /// L'Auto est celui d'un groupe lié (Min/Max d'axe, case « Auto range ») :
    /// `auto` l'active pour tout le groupe, un champ vidé est refusé.
    Group,
}

/// La définition d'une grandeur numérique.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    /// Plus petite valeur permise (`f64::NEG_INFINITY` : aucune borne).
    pub min: f64,
    /// Plus grande valeur permise (`f64::INFINITY` : aucune borne).
    pub max: f64,
    /// Un cran de flèche ou de molette.
    pub step: f64,
    /// Un grand cran (Maj + flèche).
    pub big_step: f64,
    /// Décimales les plus longues affichées (la saisie peut en porter plus).
    pub decimals: usize,
    /// Nombre entier : une saisie à décimales est refusée.
    pub integer: bool,
    /// Valeurs proposées par le `▼`, dans l'ordre d'affichage.
    pub presets: &'static [f64],
    /// Ce que la grandeur fait de `auto` et d'un champ vidé.
    pub auto: AutoKind,
}

impl Quantity {
    /// Un réel non borné.
    pub const fn real(step: f64, big_step: f64, decimals: usize) -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step,
            big_step,
            decimals,
            integer: false,
            presets: &[],
            auto: AutoKind::None,
        }
    }

    /// Un entier non borné, de pas 1.
    pub const fn whole(big_step: f64) -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: 1.0,
            big_step,
            decimals: 0,
            integer: true,
            presets: &[],
            auto: AutoKind::None,
        }
    }

    /// Le nombre est entier, positif et tient en trois chiffres, sans liste de
    /// valeurs : le champ prend la largeur « quelques chiffres » (L2, #237).
    pub fn is_compact(&self) -> bool {
        self.integer && self.min >= 0.0 && self.max < 1000.0 && self.presets.is_empty()
    }

    /// Le nombre est un réel signé borné à trois chiffres, sans liste de
    /// valeurs (rotation en degrés) : « -180.5 » tient dans un champ un peu
    /// plus large que [`Self::is_compact`] (#246).
    pub fn is_compact_signed(&self) -> bool {
        !self.integer
            && self.min > -1000.0
            && self.max < 1000.0
            && self.presets.is_empty()
            && self.auto == AutoKind::None
    }

    /// La même grandeur bornée à `min..=max`.
    pub const fn between(self, min: f64, max: f64) -> Self {
        Self { min, max, ..self }
    }

    /// La même grandeur, avec l'Auto donné.
    pub const fn with_auto(self, auto: AutoKind) -> Self {
        Self { auto, ..self }
    }

    /// La même grandeur avec ses valeurs proposées.
    pub const fn with_presets(self, presets: &'static [f64]) -> Self {
        Self { presets, ..self }
    }
}

/// Une valeur libre : abscisse, ordonnée, borne d'axe, paramètre d'ajustement.
pub const FREE: Quantity = Quantity::real(0.1, 1.0, 8);

/// Une borne d'axe : libre, et liée à la case « Auto range » de son axe.
pub const AXIS_BOUND: Quantity = FREE.with_auto(AutoKind::Group);

/// Épaisseur d'un trait de série, en points : 0 à 20, décimales au clavier.
pub const LINE_WIDTH: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 20.0)
    .with_presets(&[0.5, 1.0, 1.5, 2.0, 3.0, 4.0]);

/// Épaisseur du trait d'une forme du graphe, en points ; valeurs de la barre
/// de format des objets.
pub const SHAPE_LINE_WIDTH: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 100.0)
    .with_presets(&[0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0]);

/// Taille d'un symbole de série, en points.
pub const SYMBOL_SIZE: Quantity = Quantity::real(0.1, 1.0, 2)
    .between(0.0, 40.0)
    .with_presets(&[2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);

/// Corps d'un texte du graphe, en points ; bornes de `AnnotationSpec::SIZE_RANGE`.
pub const TEXT_SIZE: Quantity = Quantity::real(0.5, 2.0, 1)
    .between(4.0, 200.0)
    .with_presets(&[8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 24.0, 36.0]);

/// Corps proposé par la barre de format du texte : mêmes bornes que
/// [`TEXT_SIZE`], liste plus longue et entrée « Auto » (C4, N9).
pub const TEXT_FORMAT_SIZE: Quantity = TEXT_SIZE
    .with_presets(&[
        6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0,
    ])
    .with_auto(AutoKind::Field);

/// Rotation d'un texte, en degrés.
pub const ROTATION: Quantity = Quantity::real(1.0, 15.0, 1).between(-180.0, 180.0);

/// Canal rouge, vert ou bleu, de 0 à 255.
pub const COLOR_CHANNEL: Quantity = Quantity::whole(16.0).between(0.0, 255.0);

/// Opacité, en pour cent.
pub const OPACITY_PERCENT: Quantity = Quantity::whole(10.0).between(0.0, 100.0);

/// Teinte, en degrés.
pub const HUE_DEGREES: Quantity = Quantity::whole(15.0).between(0.0, 360.0);

/// Saturation ou luminosité, en pour cent.
pub const PERCENT: Quantity = Quantity::whole(10.0).between(0.0, 100.0);

/// Degré d'un polynôme.
pub const POLYNOMIAL_DEGREE: Quantity =
    Quantity::whole(1.0).between(1.0, limits::MAX_DEGREE as f64);

/// Proéminence minimale d'un pic, en pour cent du signal.
pub const PROMINENCE_PERCENT: Quantity = Quantity::real(0.5, 5.0, 2).between(0.1, 100.0);

/// Nombre maximal de pics cherchés.
pub const MAX_PEAKS: Quantity = Quantity::whole(5.0).between(1.0, limits::MAX_PEAKS as f64);

/// Nombre de composantes d'une déconvolution.
pub const COMPONENTS: Quantity = Quantity::whole(1.0).between(1.0, limits::MAX_COMPONENTS as f64);

/// Nombre d'itérations d'un solveur.
pub const ITERATIONS: Quantity = Quantity::whole(100.0).between(1.0, limits::MAX_ITERATIONS as f64);

/// Un nombre de lignes ou de caractères à sauter à l'import (0 et plus).
pub const COUNT: Quantity = Quantity::whole(10.0).between(0.0, 9_007_199_254_740_991.0);

/// Un numéro de ligne de l'import (1 et plus).
pub const LINE_NUMBER: Quantity = Quantity::whole(10.0).between(1.0, 9_007_199_254_740_991.0);
