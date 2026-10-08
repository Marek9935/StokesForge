//! Couleurs enregistrées par l'utilisateur, rangées par usage.
//!
//! Une courbe, un texte et un fond n'ont pas besoin des mêmes couleurs : le
//! sélecteur de couleur de l'application propose une palette par usage
//! ([`ColorUsage`]), et une couleur enregistrée ne réapparaît que dans la
//! palette de l'usage pour lequel elle a été enregistrée.
//!
//! Ce type décrit les couleurs enregistrées **dans un projet** (elles
//! voyagent avec le fichier `.stf`) ; l'application tient à part, sur la
//! machine, celles que l'utilisateur garde pour tous ses projets — même
//! forme, autre stockage. Comme partout dans ce crate, une couleur est une
//! chaîne `#RRGGBB` ou `#RRGGBBAA`, jamais un type structuré.

/// Ce que colore une couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorUsage {
    /// Trait et marqueur d'une courbe.
    Curves,
    /// Texte et échelles (titres, graduations).
    Text,
    /// Fond du graphe.
    Background,
}

/// Nombre maximal de couleurs enregistrées par usage. Un plafond de
/// lisibilité, pas de mémoire : au-delà, une palette ne se parcourt plus
/// d'un coup d'œil. Enregistrer une couleur de plus retire la plus ancienne ;
/// un fichier qui en porterait davantage est tronqué à la lecture.
pub const MAX_SAVED_COLORS: usize = 32;

/// Un nuancier enregistré (#178) : un nom choisi par l'utilisateur et ses
/// arrêts `#RRGGBB`, au moins deux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGradient {
    /// Nom, unique dans sa liste (casse indifférente).
    pub name: String,
    /// Arrêts, du premier au dernier.
    pub stops: Vec<String>,
}

/// Couleurs enregistrées, par usage, de la plus ancienne à la plus récente.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedColors {
    /// Couleurs de courbes.
    pub curves: Vec<String>,
    /// Couleurs de texte.
    pub text: Vec<String>,
    /// Couleurs de fond.
    pub backgrounds: Vec<String>,
    /// Nuanciers enregistrés (#178), du plus ancien au plus récent, bornés
    /// comme les couleurs ([`MAX_SAVED_COLORS`]).
    pub gradients: Vec<SavedGradient>,
}

impl SavedColors {
    /// Les couleurs enregistrées pour `usage`.
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

    /// Une copie où `color` est enregistrée pour `usage` : à la fin de la
    /// liste, sans doublon (casse indifférente), la plus ancienne retirée
    /// au-delà de [`MAX_SAVED_COLORS`].
    pub fn with_added(&self, usage: ColorUsage, color: &str) -> Self {
        let mut next = self.with_removed(usage, color);
        let list = next.get_mut(usage);
        list.push(color.trim().to_ascii_lowercase());
        let excess = list.len().saturating_sub(MAX_SAVED_COLORS);
        list.drain(..excess);
        next
    }

    /// Une copie où `color` n'est plus enregistrée pour `usage`.
    pub fn with_removed(&self, usage: ColorUsage, color: &str) -> Self {
        let mut next = self.clone();
        let target = color.trim();
        next.get_mut(usage)
            .retain(|saved| !saved.trim().eq_ignore_ascii_case(target));
        next
    }

    /// Une copie où le nuancier `name` est enregistré avec les arrêts `stops` :
    /// il remplace un nuancier de même nom (casse indifférente, accents compris), sinon va à la
    /// fin, la plus ancienne partant au-delà de [`MAX_SAVED_COLORS`]. Un nom
    /// vide ou moins de deux arrêts ne changent rien.
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

    /// Une copie sans le nuancier `name` (casse indifférente).
    pub fn with_gradient_removed(&self, name: &str) -> Self {
        let mut next = self.clone();
        let name = name.trim();
        next.gradients
            .retain(|saved| saved.name.trim().to_lowercase() != name.to_lowercase());
        next
    }

    /// Ramène chaque liste sous [`MAX_SAVED_COLORS`] en gardant les plus
    /// récentes — pour une liste venue d'un fichier, que rien ne borne.
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

    /// Vrai si aucune couleur n'est enregistrée.
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
    fn enregistrer_range_par_usage_sans_doublon() {
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
    fn au_dela_du_plafond_la_plus_ancienne_part() {
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
    fn un_nuancier_se_remplace_par_son_nom_et_se_supprime() {
        let stops = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
        let saved = SavedColors::default()
            .with_gradient_added("Mon dégradé", &stops("#0000FF", "#ff0000"))
            .with_gradient_added("autre", &stops("#000000", "#ffffff"))
            .with_gradient_added("MON DÉGRADÉ", &stops("#00ff00", "#ff0000"));
        assert_eq!(saved.gradients.len(), 2);
        assert_eq!(saved.gradients[1].name, "MON DÉGRADÉ");
        assert_eq!(saved.gradients[1].stops, ["#00ff00", "#ff0000"]);
        assert!(!saved.is_empty());
        let saved = saved.with_gradient_removed("autre");
        assert_eq!(saved.gradients.len(), 1);
        // Sans nom ou avec un seul arrêt : rien n'est enregistré.
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
