//! Colors saved by the user, filed by usage.
//!
//! A curve, a text and a background do not need the same colors: the
//! application's color picker offers one palette per usage
//! ([`ColorUsage`]), and a saved color only reappears in the
//! palette of the usage it was saved for.
//!
//! This type describes the colors saved **in a project** (they
//! travel with the `.stf` file); the application keeps separately, on the
//! machine, those the user keeps for all their projects — same
//! shape, other storage. As everywhere in this crate, a color is a
//! `#RRGGBB` or `#RRGGBBAA` string, never a structured type.

/// What a color colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorUsage {
    /// Line and marker of a curve.
    Curves,
    /// Text and scales (titles, ticks).
    Text,
    /// Background of the graph.
    Background,
}

/// Maximum number of saved colors per usage. A readability cap, not a
/// memory one: beyond it, a palette can no longer be scanned
/// at a glance. Saving one more color removes the oldest;
/// a file carrying more is truncated on read.
pub const MAX_SAVED_COLORS: usize = 32;

/// A saved color gradient (#178): a name chosen by the user and its
/// `#RRGGBB` stops, at least two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGradient {
    /// Name, unique in its list (case-insensitive).
    pub name: String,
    /// Stops, from first to last.
    pub stops: Vec<String>,
}

/// Saved colors, by usage, from the oldest to the most recent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedColors {
    /// Curve colors.
    pub curves: Vec<String>,
    /// Text colors.
    pub text: Vec<String>,
    /// Background colors.
    pub backgrounds: Vec<String>,
    /// Saved color gradients (#178), from the oldest to the most recent, bounded
    /// like the colors ([`MAX_SAVED_COLORS`]).
    pub gradients: Vec<SavedGradient>,
}

impl SavedColors {
    /// The colors saved for `usage`.
    pub fn get(&self, usage: ColorUsage) -> &[String] {
        match usage {
            ColorUsage::Curves => &self.curves,
            ColorUsage::Text => &self.text,
            ColorUsage::Background => &self.backgrounds,
        }
    }

    fn get_mut(&mut self, usage: ColorUsage) -> &mut Vec<String> {
        match usage {
            ColorUsage::Curves => &mut self.curves,
            ColorUsage::Text => &mut self.text,
            ColorUsage::Background => &mut self.backgrounds,
        }
    }

    /// A copy where `color` is saved for `usage`: at the end of the
    /// list, without duplicate (case-insensitive), the oldest removed
    /// beyond [`MAX_SAVED_COLORS`].
    pub fn with_added(&self, usage: ColorUsage, color: &str) -> Self {
        let mut next = self.with_removed(usage, color);
        let list = next.get_mut(usage);
        list.push(color.trim().to_ascii_lowercase());
        let excess = list.len().saturating_sub(MAX_SAVED_COLORS);
        list.drain(..excess);
        next
    }

    /// A copy where `color` is no longer saved for `usage`.
    pub fn with_removed(&self, usage: ColorUsage, color: &str) -> Self {
        let mut next = self.clone();
        let target = color.trim();
        next.get_mut(usage)
            .retain(|saved| !saved.trim().eq_ignore_ascii_case(target));
        next
    }

    /// A copy where the color gradient `name` is saved with the stops `stops`:
    /// it replaces a color gradient of the same name (case-insensitive, accents included), otherwise goes to the
    /// end, the oldest leaving beyond [`MAX_SAVED_COLORS`]. An empty name
    /// or fewer than two stops change nothing.
    pub fn with_gradient_added(&self, name: &str, stops: &[String]) -> Self {
        let name = name.trim();
        if name.is_empty() || stops.len() < 2 {
            return self.clone();
        }
        let mut next = self.with_gradient_removed(name);
        next.gradients.push(SavedGradient {
            name: name.to_string(),
            stops: stops
                .iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .collect(),
        });
        let excess = next.gradients.len().saturating_sub(MAX_SAVED_COLORS);
        next.gradients.drain(..excess);
        next
    }

    /// A copy without the color gradient `name` (case-insensitive).
    pub fn with_gradient_removed(&self, name: &str) -> Self {
        let mut next = self.clone();
        let name = name.trim();
        next.gradients
            .retain(|saved| saved.name.trim().to_lowercase() != name.to_lowercase());
        next
    }

    /// Brings each list under [`MAX_SAVED_COLORS`], keeping the most
    /// recent — for a list coming from a file, which nothing bounds.
    pub fn bounded(mut self) -> Self {
        for usage in [ColorUsage::Curves, ColorUsage::Text, ColorUsage::Background] {
            let list = self.get_mut(usage);
            let excess = list.len().saturating_sub(MAX_SAVED_COLORS);
            list.drain(..excess);
        }
        let excess = self.gradients.len().saturating_sub(MAX_SAVED_COLORS);
        self.gradients.drain(..excess);
        self
    }

    /// True if no color is saved.
    pub fn is_empty(&self) -> bool {
        self.curves.is_empty()
            && self.text.is_empty()
            && self.backgrounds.is_empty()
            && self.gradients.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_groups_by_usage_without_duplicates() {
        let saved = SavedColors::default()
            .with_added(ColorUsage::Curves, "#FF0000")
            .with_added(ColorUsage::Text, "#202020")
            .with_added(ColorUsage::Curves, "#00ff00")
            .with_added(ColorUsage::Curves, "#ff0000");
        assert_eq!(saved.get(ColorUsage::Curves), ["#00ff00", "#ff0000"]);
        assert_eq!(saved.get(ColorUsage::Text), ["#202020"]);
        assert!(saved.get(ColorUsage::Background).is_empty());
        let saved = saved.with_removed(ColorUsage::Curves, "#FF0000");
        assert_eq!(saved.get(ColorUsage::Curves), ["#00ff00"]);
    }

    #[test]
    fn beyond_the_cap_the_oldest_leaves() {
        let mut saved = SavedColors::default();
        for index in 0..=MAX_SAVED_COLORS {
            saved = saved.with_added(ColorUsage::Background, &format!("#0000{index:02x}"));
        }
        let list = saved.get(ColorUsage::Background);
        assert_eq!(list.len(), MAX_SAVED_COLORS);
        assert_eq!(list[0], "#000001");
        let long = SavedColors {
            curves: vec!["#000000".to_string(); MAX_SAVED_COLORS + 5],
            ..SavedColors::default()
        };
        assert_eq!(long.bounded().curves.len(), MAX_SAVED_COLORS);
    }

    #[test]
    fn a_gradient_is_replaced_by_its_name_and_removed() {
        let stops = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
        let saved = SavedColors::default()
            .with_gradient_added("Café gradient", &stops("#0000FF", "#ff0000"))
            .with_gradient_added("other", &stops("#000000", "#ffffff"))
            .with_gradient_added("CAFÉ GRADIENT", &stops("#00ff00", "#ff0000"));
        assert_eq!(saved.gradients.len(), 2);
        assert_eq!(saved.gradients[1].name, "CAFÉ GRADIENT");
        assert_eq!(saved.gradients[1].stops, ["#00ff00", "#ff0000"]);
        assert!(!saved.is_empty());
        let saved = saved.with_gradient_removed("other");
        assert_eq!(saved.gradients.len(), 1);
        // Without a name or with a single stop: nothing is saved.
        assert_eq!(
            saved.with_gradient_added("  ", &stops("#000000", "#ffffff")),
            saved
        );
        assert_eq!(
            saved.with_gradient_added("x", &["#000000".to_string()]),
            saved
        );
    }
}
