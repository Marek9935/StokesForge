//! La description complète d'un graphe, et l'adressage de ses séries.

use std::ops::Deref;

use stokesforge_id::NodeId;

use crate::panel::PanelSpec;
use crate::series::SeriesSpec;
use crate::text_anchor::TextAnchor;
use crate::text_font::TextFont;

/// Version du *format* du spec, indépendante de la version de schéma du
/// fichier projet : un [`PlotSpec`] peut aussi voyager par presse-papiers ou
/// par modèle sans passer par un fichier.
pub const SPEC_FORMAT_VERSION: u32 = 1;

/// Les panneaux d'un graphe : **au moins un, toujours**.
///
/// C'est le seul invariant du crate que le système de types ne donne pas
/// gratuitement, alors il est tenu par construction plutôt que vérifié : le
/// `Vec` est privé, et aucune opération publique ne peut le vider. Un graphe
/// sans panneau ne serait pas rendable, et la version Python le rattrapait à
/// chaque construction (`tuple(panels) or (PanelSpec(),)`) — ici il n'y a
/// rien à rattraper parce qu'il n'y a pas d'état à rattraper.
#[derive(Debug, Clone, PartialEq)]
pub struct Panels(Vec<PanelSpec>);

impl Panels {
    /// Un panneau unique.
    pub fn one(panel: PanelSpec) -> Self {
        Self(vec![panel])
    }

    /// Les panneaux donnés, dans l'ordre. **Une suite vide donne un panneau
    /// par défaut** plutôt qu'une erreur : c'est le comportement de la version
    /// Python, et le seul qui ait un sens — refuser obligerait tout appelant à
    /// traiter un cas qui n'a pas de réparation intéressante.
    pub fn new(panels: impl IntoIterator<Item = PanelSpec>) -> Self {
        let panels: Vec<PanelSpec> = panels.into_iter().collect();
        if panels.is_empty() {
            Self::default()
        } else {
            Self(panels)
        }
    }

    /// Ajoute un panneau à la fin.
    pub fn push(&mut self, panel: PanelSpec) {
        self.0.push(panel);
    }

    /// Accès modifiable au panneau `index`.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut PanelSpec> {
        self.0.get_mut(index)
    }

    /// Les panneaux, modifiables un à un (leur nombre ne change pas : la
    /// liste reste non vide).
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, PanelSpec> {
        self.0.iter_mut()
    }

    /// Les panneaux, en lecture.
    pub fn as_slice(&self) -> &[PanelSpec] {
        &self.0
    }
}

impl Default for Panels {
    fn default() -> Self {
        Self(vec![PanelSpec::default()])
    }
}

impl Deref for Panels {
    type Target = [PanelSpec];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<PanelSpec> for Panels {
    fn from(panel: PanelSpec) -> Self {
        Self::one(panel)
    }
}

impl From<Vec<PanelSpec>> for Panels {
    fn from(panels: Vec<PanelSpec>) -> Self {
        Self::new(panels)
    }
}

/// Adresse d'une série dans un [`PlotSpec`] : le panneau, puis le rang dans ce
/// panneau.
///
/// C'est ce qui remplace l'identifiant opaque que la version Python attachait
/// à chaque série. Une adresse n'est valable que pour la valeur de `PlotSpec`
/// dont elle sort : le spec étant immuable et reconstruit à chaque
/// modification, il n'y a pas d'adresse à faire survivre à une édition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeriesRef {
    /// Indice du panneau dans [`PlotSpec::panels`].
    pub panel: usize,
    /// Rang de la série dans ce panneau.
    pub index: usize,
}

impl SeriesRef {
    /// Adresse de la série `index` du panneau `panel`.
    pub const fn new(panel: usize, index: usize) -> Self {
        Self { panel, index }
    }
}

impl std::fmt::Display for SeriesRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "série {} du panneau {}", self.index, self.panel)
    }
}

/// Aucune couleur : blanc d'opacité nulle. Le fond par défaut d'un graphe.
pub const TRANSPARENT: &str = "#ffffff00";

/// La description complète d'un graphe : le seul objet qu'un renderer lira.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotSpec {
    /// Titre du graphe.
    pub title: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    pub title_visible: bool,
    /// Panneaux, au moins un.
    pub panels: Panels,
    /// Fond du graphe (`#RRGGBB` ou `#RRGGBBAA`). Par défaut **aucune
    /// couleur** ([`TRANSPARENT`]) : la figure exportée n'a pas de fond et
    /// s'intègre telle quelle à une autre présentation. Vide = fond blanc
    /// (ce que portent les fichiers écrits avant ce choix).
    pub background: String,
    /// Couleur du titre du graphe (`#RRGGBB`). Vide = automatique : l'encre
    /// qui contraste avec le fond (sombre sur fond clair, blanche sur fond
    /// sombre), comme tout le texte et les axes laissés en automatique.
    pub title_color: String,
    /// Position du titre du graphe s'il a été déplacé (issue #53), dans le
    /// repère de données du premier panneau ; `None` = sa place par défaut.
    pub title_anchor: Option<TextAnchor>,
    /// Police et corps du titre du graphe (issue #78).
    pub title_font: TextFont,
    /// Version du format de ce spec.
    pub format_version: u32,
}

impl PlotSpec {
    /// Un graphe vide mais valide : un panneau, aucune série.
    pub fn empty(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Nombre de lignes de la grille de panneaux.
    ///
    /// B2 (`guide-de-correction-robustesse.md` §F3/F4/rendu) —
    /// `saturating_add(1)` : une position de panneau venue d'un fichier peut
    /// valoir `usize::MAX` (rien ne la borne à la lecture, voir C5/le
    /// lecteur), et `+ 1` panique en debug, déborde sur 0 en release.
    pub fn rows(&self) -> usize {
        self.panels
            .iter()
            .map(|p| p.row)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Nombre de colonnes de la grille de panneaux. À ne pas confondre avec
    /// [`PlotSpec::referenced_columns`] : ici ce sont des colonnes de grille,
    /// là des colonnes de données. Voir [`PlotSpec::rows`] pour le
    /// `saturating_add`.
    pub fn columns(&self) -> usize {
        self.panels
            .iter()
            .map(|p| p.column)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    /// Toutes les séries du graphe, avec leur adresse, panneau par panneau.
    pub fn iter_series(&self) -> impl Iterator<Item = (SeriesRef, &SeriesSpec)> {
        self.panels.iter().enumerate().flat_map(|(panel, spec)| {
            spec.series
                .iter()
                .enumerate()
                .map(move |(index, series)| (SeriesRef::new(panel, index), series))
        })
    }

    /// La série à cette adresse, si elle existe.
    pub fn series_at(&self, at: SeriesRef) -> Option<&SeriesSpec> {
        self.panels.get(at.panel)?.series.get(at.index)
    }

    /// La série à cette adresse, modifiable.
    pub fn series_at_mut(&mut self, at: SeriesRef) -> Option<&mut SeriesSpec> {
        self.panels.get_mut(at.panel)?.series.get_mut(at.index)
    }

    /// Nombre total de séries, tous panneaux confondus.
    pub fn series_count(&self) -> usize {
        self.panels.iter().map(|p| p.series.len()).sum()
    }

    /// Colonnes référencées par l'ensemble des séries, sans doublon, dans
    /// l'ordre de première apparition.
    ///
    /// C'est la liste que la résolution ira chercher dans l'arbre, et celle
    /// qu'une suppression de colonne devra consulter pour savoir quels graphes
    /// elle casse.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        for (_, series) in self.iter_series() {
            for column in series.referenced_columns() {
                if !seen.contains(&column) {
                    seen.push(column);
                }
            }
        }
        seen
    }

    /// Vrai si une série du graphe référence cette colonne.
    pub fn references(&self, column: NodeId) -> bool {
        self.panels.iter().any(|p| p.references(column))
    }

    /// Le même graphe avec d'autres panneaux.
    pub fn with_panels(self, panels: impl Into<Panels>) -> Self {
        Self {
            panels: panels.into(),
            ..self
        }
    }
}

impl Default for PlotSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            panels: Panels::default(),
            background: TRANSPARENT.to_string(),
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFont::default(),
            format_version: SPEC_FORMAT_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un graphe neuf n'a pas de fond : « aucune couleur » est le défaut.
    #[test]
    fn le_fond_par_defaut_est_sans_couleur() {
        assert_eq!(PlotSpec::default().background, TRANSPARENT);
        assert_eq!(PlotSpec::empty("Titre").background, TRANSPARENT);
        assert!(TRANSPARENT.ends_with("00"), "opacité nulle");
    }

    #[test]
    fn panels_are_never_empty_even_when_asked_to_be() {
        assert_eq!(Panels::new(Vec::new()).len(), 1);
        assert_eq!(Panels::default().len(), 1);
    }

    #[test]
    fn panels_keep_the_order_they_were_given() {
        let panels = Panels::new([PanelSpec::at(0, 0), PanelSpec::at(1, 0)]);
        assert_eq!(panels.len(), 2);
        assert_eq!(panels[1].row, 1);
    }

    #[test]
    fn a_series_ref_reads_as_french() {
        assert_eq!(SeriesRef::new(1, 0).to_string(), "série 0 du panneau 1");
    }
}
