//! The length unit of the interface (#394): how a size stored in points is
//! shown and typed. A preference (centimetres by default), never in the `.stf`,
//! which stores points.
//!
//! Pure conversion, no text: the unit goes in the label of a field through a
//! catalog key (S-N10), the symbol being the `{ $unit }` argument.

/// Points per inch (the PostScript / PDF point).
const POINTS_PER_INCH: f64 = 72.0;

/// A unit of length offered for the size of a graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    /// Centimetres (default).
    #[default]
    Cm,
    /// Millimetres.
    Mm,
    /// Points, the unit the sizes are stored in.
    Pt,
    /// Inches.
    In,
}

impl LengthUnit {
    /// The units offered, in the order of the preference.
    pub const ALL: [Self; 4] = [Self::Cm, Self::Mm, Self::Pt, Self::In];

    /// Points in one of this unit.
    pub fn points_per_unit(self) -> f64 {
        match self {
            Self::Cm => POINTS_PER_INCH / 2.54,
            Self::Mm => POINTS_PER_INCH / 25.4,
            Self::Pt => 1.0,
            Self::In => POINTS_PER_INCH,
        }
    }

    /// A length in points, in this unit.
    pub fn from_points(self, points: f64) -> f64 {
        points / self.points_per_unit()
    }

    /// A length in this unit, in points.
    pub fn to_points(self, value: f64) -> f64 {
        value * self.points_per_unit()
    }

    /// The symbol written after a value and in a field label: `cm`, `mm`,
    /// `pt`, `in`. Also the value stored in the preferences file.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::Pt => "pt",
            Self::In => "in",
        }
    }

    /// The unit written in the preferences file; anything else (unknown,
    /// from a newer version) is `None`, and the caller takes the default.
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.symbol() == symbol)
    }

    /// Decimals shown for a size in this unit.
    pub fn decimals(self) -> usize {
        match self {
            Self::Cm | Self::In => 2,
            Self::Mm | Self::Pt => 1,
        }
    }

    /// One notch of arrow or wheel on a size in this unit.
    pub fn step(self) -> f64 {
        match self {
            Self::Cm => 0.1,
            Self::In => 0.05,
            Self::Mm => 1.0,
            Self::Pt => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_centimetres_are_283_46_points() {
        let points = LengthUnit::Cm.to_points(10.0);
        assert!((points - 283.464_566_9).abs() < 1e-6, "{points}");
    }

    #[test]
    fn the_units_agree_with_the_inch() {
        assert_eq!(LengthUnit::In.to_points(1.0), 72.0);
        assert!((LengthUnit::Mm.to_points(25.4) - 72.0).abs() < 1e-9);
        assert_eq!(LengthUnit::Pt.to_points(480.0), 480.0);
    }

    #[test]
    fn a_conversion_round_trips_in_every_unit() {
        for unit in LengthUnit::ALL {
            let back = unit.to_points(unit.from_points(360.0));
            assert!((back - 360.0).abs() < 1e-9, "{unit:?}");
        }
    }

    #[test]
    fn the_default_is_centimetres_and_an_unknown_symbol_is_none() {
        assert_eq!(LengthUnit::default(), LengthUnit::Cm);
        assert_eq!(LengthUnit::from_symbol("furlong"), None);
        for unit in LengthUnit::ALL {
            assert_eq!(LengthUnit::from_symbol(unit.symbol()), Some(unit));
        }
    }
}
