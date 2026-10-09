//! Un panneau : une zone de tracé, ses quatre axes, sa légende, ses séries,
//! ses textes libres.

use stokesforge_id::NodeId;

use crate::annotation::AnnotationSpec;
use crate::axis::{AxisSide, AxisSpec};
use crate::legend::LegendSpec;
use crate::series::{SeriesAxis, SeriesSpec};
use crate::text_anchor::TextAnchor;

/// Calque superposé à un autre (#167), à la manière du « New Layer (Axes) »
/// d'Origin : il partage la case de la grille **et la zone de tracé** du calque
/// qu'il recouvre (dimension liée), sur un fond transparent.
///
/// Son axe `bottom` est son axe X, son axe `left` son axe Y. `x_top` le place
/// en haut (échelle **propre** au calque) ; sinon l'axe n'est pas affiché et
/// l'échelle X est celle du calque recouvert (échelle liée). Même règle pour
/// `y_right` et l'axe Y, à droite. Un calque sans axe affiché (`false`,
/// `false`) suit donc entièrement le calque recouvert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlaySpec {
    /// L'axe X du calque est affiché en haut, avec son échelle propre.
    pub x_top: bool,
    /// L'axe Y du calque est affiché à droite, avec son échelle propre.
    pub y_right: bool,
}

/// Un emplacement de la liste des séries d'un panneau (#197), tel que le
/// projette [`PanelSpec::slots`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeriesSlot {
    /// Une série hors groupe : son rang dans [`PanelSpec::series`].
    Single(usize),
    /// Un groupe et les rangs de tous ses membres, dans l'ordre.
    Group {
        /// Numéro du groupe dans le panneau.
        id: u32,
        /// Rangs des séries membres.
        members: Vec<usize>,
    },
}

impl SeriesSlot {
    /// Vrai si la série de rang `index` est dans cet emplacement.
    pub fn contains(&self, index: usize) -> bool {
        match self {
            Self::Single(i) => *i == index,
            Self::Group { members, .. } => members.contains(&index),
        }
    }
}

/// Un panneau et ses quatre axes.
///
/// `row`/`column` placent le panneau dans la grille du [`crate::PlotSpec`] —
/// c'est la description du multi-panneaux, sans rien emprunter au système de
/// disposition d'un moteur donné. Ce sont des `usize` : une position négative
/// n'est pas ramenée à zéro, elle ne s'écrit pas.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    /// Titre du panneau.
    pub title: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    pub title_visible: bool,
    /// Position du titre du panneau s'il a été déplacé (issue #53).
    pub title_anchor: Option<TextAnchor>,
    /// Police et corps du titre du panneau (issue #78).
    pub title_font: crate::TextFont,
    /// Couleur du titre du panneau (issue #90) ; vide = automatique, comme
    /// [`crate::PlotSpec::title_color`].
    pub title_color: String,
    /// Séries tracées, dans l'ordre de superposition.
    pub series: Vec<SeriesSpec>,
    /// Axe du bas.
    pub bottom: AxisSpec,
    /// Axe de gauche.
    pub left: AxisSpec,
    /// Axe de droite (secondaire).
    pub right: AxisSpec,
    /// Axe du haut, masqué par défaut.
    pub top: AxisSpec,
    /// Légende.
    pub legend: LegendSpec,
    /// Calque superposé à un autre de la même case (#167) ; `None` pour un
    /// calque ordinaire. Un calque superposé recouvre le dernier calque
    /// ordinaire qui le précède dans la même case.
    pub overlay: Option<OverlaySpec>,
    /// Ligne du panneau dans la grille.
    pub row: usize,
    /// Colonne du panneau dans la grille.
    pub column: usize,
    /// Textes libres posés sur le panneau, ancrés aux données (issue #35),
    /// dans l'ordre de superposition.
    pub annotations: Vec<AnnotationSpec>,
    /// Objets graphiques (rectangle, ellipse, ligne, flèche — issue #76),
    /// ancrés aux données, du dessous au dessus. Tracés sous les textes
    /// libres.
    pub shapes: Vec<crate::ShapeSpec>,
    /// Noms donnés aux groupes de séries (#197), par numéro de groupe. Un
    /// groupe sans entrée porte son nom automatique (« Group n »). Seules
    /// les entrées de groupes existants comptent : [`PanelSpec::group_label`]
    /// ignore les autres et [`PanelSpec::next_group_id`] ne réemploie pas leur
    /// numéro, donc un nom resté derrière un groupe disparu ne repasse jamais
    /// à un groupe neuf.
    pub group_names: std::collections::BTreeMap<u32, String>,
}

impl PanelSpec {
    /// Panneau par défaut placé en `(row, column)`.
    pub fn at(row: usize, column: usize) -> Self {
        Self {
            row,
            column,
            ..Self::default()
        }
    }

    /// Un calque superposé (#167) au calque `base` : même case, sans titre ni
    /// légende, seuls les axes que `overlay` affiche sont visibles, sans
    /// quadrillage.
    pub fn overlay_on(base: &PanelSpec, overlay: OverlaySpec) -> Self {
        let axis = |visible| AxisSpec {
            visible,
            grid: false,
            ..AxisSpec::default()
        };
        let legend = LegendSpec {
            visible: false,
            ..LegendSpec::default()
        };
        Self {
            title_visible: false,
            bottom: axis(overlay.x_top),
            left: axis(overlay.y_right),
            right: AxisSpec::hidden(),
            legend,
            overlay: Some(overlay),
            ..Self::at(base.row, base.column)
        }
    }

    /// L'axe d'un côté donné.
    pub fn axis(&self, side: AxisSide) -> &AxisSpec {
        match side {
            AxisSide::Bottom => &self.bottom,
            AxisSide::Left => &self.left,
            AxisSide::Right => &self.right,
            AxisSide::Top => &self.top,
        }
    }

    /// L'axe d'un côté donné, modifiable.
    pub fn axis_mut(&mut self, side: AxisSide) -> &mut AxisSpec {
        match side {
            AxisSide::Bottom => &mut self.bottom,
            AxisSide::Left => &mut self.left,
            AxisSide::Right => &mut self.right,
            AxisSide::Top => &mut self.top,
        }
    }

    /// Vrai si au moins une série est rattachée à l'axe de droite — c'est ce
    /// qui décide de l'afficher.
    pub fn uses_right_axis(&self) -> bool {
        self.series.iter().any(|s| s.axis == SeriesAxis::Right)
    }

    /// L'axe du haut est un miroir de celui du bas (#176).
    pub fn mirrors_top(&self) -> bool {
        self.overlay.is_none() && self.top.mirror
    }

    /// L'axe de droite est un miroir de celui de gauche (#176) : seulement
    /// quand aucune série n'y est rattachée, sinon c'est un vrai axe.
    pub fn mirrors_right(&self) -> bool {
        self.overlay.is_none() && self.right.mirror && !self.uses_right_axis()
    }

    /// Colonnes référencées par les séries du panneau, sans doublon, dans
    /// l'ordre de première apparition.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        for series in &self.series {
            for column in series.referenced_columns() {
                if !seen.contains(&column) {
                    seen.push(column);
                }
            }
        }
        seen
    }

    /// Le premier numéro de groupe (#177) qu'aucune série du panneau ne porte
    /// et qu'aucun nom de groupe ne réserve (#197).
    pub fn next_group_id(&self) -> u32 {
        self.series
            .iter()
            .filter_map(|s| s.group)
            .chain(self.group_names.keys().copied())
            .max()
            .map_or(1, |max| max.saturating_add(1))
    }

    /// Ajoute des séries **ensemble** (#177) : à deux ou plus, elles forment un
    /// nouveau groupe ; une série seule n'en a pas. Les numéros de groupe que
    /// les séries portaient déjà sont écartés.
    pub fn push_series_batch(&mut self, series: Vec<SeriesSpec>) {
        let group = (series.len() > 1).then(|| self.next_group_id());
        self.series.extend(series.into_iter().map(|mut s| {
            s.group = group;
            s
        }));
    }

    /// Rangs, dans [`PanelSpec::series`], des séries du groupe `group`.
    pub fn group_members(&self, group: u32) -> Vec<usize> {
        self.series
            .iter()
            .enumerate()
            .filter_map(|(index, s)| (s.group == Some(group)).then_some(index))
            .collect()
    }

    /// Les numéros de groupe présents, dans l'ordre de première apparition.
    pub fn group_ids(&self) -> Vec<u32> {
        let mut ids = Vec::new();
        for group in self.series.iter().filter_map(|s| s.group) {
            if !ids.contains(&group) {
                ids.push(group);
            }
        }
        ids
    }

    /// Le nom donné au groupe `group` (#197), s'il existe et n'est pas vide.
    /// `None` : le groupe porte son nom automatique, ou n'existe pas.
    pub fn group_label(&self, group: u32) -> Option<&str> {
        self.group_names
            .get(&group)
            .map(String::as_str)
            .filter(|name| !name.is_empty() && self.series.iter().any(|s| s.group == Some(group)))
    }

    /// Nomme le groupe `group` ; un nom vide rend le nom automatique. Faux si
    /// le groupe n'existe pas ou si rien ne change.
    pub fn rename_group(&mut self, group: u32, name: &str) -> bool {
        if !self.series.iter().any(|s| s.group == Some(group)) {
            return false;
        }
        let name = name.trim();
        let before = self.group_names.get(&group).map(String::as_str);
        if before.unwrap_or("") == name {
            return false;
        }
        if name.is_empty() {
            self.group_names.remove(&group);
        } else {
            self.group_names.insert(group, name.to_string());
        }
        true
    }

    /// Les emplacements de la liste des séries (#197), dans l'ordre : une
    /// série seule, ou un groupe avec **tous** ses membres. Un groupe prend la
    /// place de son premier membre — c'est la seule projection que l'Object
    /// Manager et la fenêtre des séries se partagent.
    pub fn slots(&self) -> Vec<SeriesSlot> {
        let mut slots: Vec<SeriesSlot> = Vec::new();
        for (index, series) in self.series.iter().enumerate() {
            let Some(id) = series.group else {
                slots.push(SeriesSlot::Single(index));
                continue;
            };
            let existing = slots.iter_mut().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => Some(members),
                _ => None,
            });
            match existing {
                Some(members) => members.push(index),
                None => slots.push(SeriesSlot::Group {
                    id,
                    members: vec![index],
                }),
            }
        }
        slots
    }

    /// Range les séries dans l'ordre des emplacements `slots`, puis oublie les
    /// noms de groupes disparus. Rend le rang final de la série `index`
    /// (rang d'avant l'appel), qu'un déplacement fait bouger.
    fn apply_slots(&mut self, slots: &[SeriesSlot], index: usize) -> usize {
        let order: Vec<usize> = slots
            .iter()
            .flat_map(|slot| match slot {
                SeriesSlot::Single(i) => vec![*i],
                SeriesSlot::Group { members, .. } => members.clone(),
            })
            .collect();
        debug_assert_eq!(order.len(), self.series.len());
        let mut taken: Vec<Option<SeriesSpec>> = std::mem::take(&mut self.series)
            .into_iter()
            .map(Some)
            .collect();
        self.series = order.iter().filter_map(|i| taken[*i].take()).collect();
        self.group_names
            .retain(|id, _| self.series.iter().any(|s| s.group == Some(*id)));
        order.iter().position(|i| *i == index).unwrap_or(index)
    }

    /// Place la série `index` dans le groupe `group`, **à la suite** de ses
    /// membres (un groupe est toujours contigu, #197) ; `None` la sort de son
    /// groupe, juste derrière lui. Un numéro que personne ne porte crée le
    /// groupe. Rend le rang final de la série, `None` si rien ne change.
    pub fn set_series_group(&mut self, index: usize, group: Option<u32>) -> Option<usize> {
        if index >= self.series.len() || self.series[index].group == group {
            return None;
        }
        let mut slots = self.slots();
        let rank = slots.iter().position(|slot| slot.contains(index))?;
        // Où se poserait la série seule : à la place d'un emplacement qu'elle
        // vide, sinon juste derrière le groupe qu'elle quitte.
        let emptied = match &mut slots[rank] {
            SeriesSlot::Single(_) => true,
            SeriesSlot::Group { members, .. } => {
                members.retain(|m| *m != index);
                members.is_empty()
            }
        };
        let here = if emptied {
            slots.remove(rank);
            rank
        } else {
            rank + 1
        };
        let joined = group.and_then(|id| {
            slots.iter_mut().find_map(|slot| match slot {
                SeriesSlot::Group { id: other, members } if *other == id => Some(members),
                _ => None,
            })
        });
        match (group, joined) {
            (Some(_), Some(members)) => members.push(index),
            (Some(id), None) => slots.insert(
                here,
                SeriesSlot::Group {
                    id,
                    members: vec![index],
                },
            ),
            (None, _) => slots.insert(here, SeriesSlot::Single(index)),
        }
        self.series[index].group = group;
        Some(self.apply_slots(&slots, index))
    }

    /// Sort la série `index` de son groupe (#197).
    pub fn leave_group(&mut self, index: usize) -> Option<usize> {
        self.series.get(index)?.group?;
        self.set_series_group(index, None)
    }

    /// Fait de la série `index` le premier membre d'un groupe neuf.
    pub fn start_group(&mut self, index: usize) -> Option<u32> {
        let alone = self
            .series
            .get(index)?
            .group
            .is_some_and(|group| self.group_members(group).len() == 1);
        if alone {
            return None;
        }
        let id = self.next_group_id();
        self.set_series_group(index, Some(id))?;
        Some(id)
    }

    /// Dissout le groupe `group` (#197) : ses séries deviennent simples, à la
    /// même place, chacune gardant son style. Faux si le groupe n'existe pas.
    pub fn dissolve_group(&mut self, group: u32) -> bool {
        let mut changed = false;
        for series in &mut self.series {
            if series.group == Some(group) {
                series.group = None;
                changed = true;
            }
        }
        self.group_names.remove(&group);
        changed
    }

    /// Supprime le groupe `group` et ses séries (#197). Faux s'il n'existe pas.
    pub fn remove_group(&mut self, group: u32) -> bool {
        let before = self.series.len();
        self.series.retain(|s| s.group != Some(group));
        self.group_names.remove(&group);
        self.series.len() != before
    }

    /// Remonte (`up`) ou descend la série `index` d'un cran (#197). Une série
    /// d'un groupe reste dans son groupe ; une série seule saute par-dessus
    /// un groupe entier. Rend son rang final, `None` en butée.
    pub fn move_series(&mut self, index: usize, up: bool) -> Option<usize> {
        let mut slots = self.slots();
        let rank = slots.iter().position(|slot| slot.contains(index))?;
        match &mut slots[rank] {
            SeriesSlot::Group { members, .. } => {
                let at = members.iter().position(|m| *m == index)?;
                let other = if up { at.checked_sub(1)? } else { at + 1 };
                if other >= members.len() {
                    return None;
                }
                members.swap(at, other);
            }
            SeriesSlot::Single(_) => {
                let next = if up { rank.checked_sub(1)? } else { rank + 1 };
                if next >= slots.len() {
                    return None;
                }
                slots.swap(rank, next);
            }
        }
        Some(self.apply_slots(&slots, index))
    }

    /// Répartit le nuancier d'arrêts `stops` sur les séries du groupe `group`
    /// (#178), dans l'ordre du panneau : le trait et le remplissage des
    /// symboles prennent la couleur. Faux, sans rien changer, si le groupe
    /// n'existe pas ou si aucun arrêt n'est lisible.
    pub fn apply_group_gradient<S: AsRef<str>>(&mut self, group: u32, stops: &[S]) -> bool {
        let members = self.group_members(group);
        let colors = crate::gradient_colors(stops, members.len());
        let mut changed = false;
        for (&member, color) in members.iter().zip(colors) {
            let series = &mut self.series[member];
            changed |= series.line.color != color || series.symbol.color != color;
            series.line.color.clone_from(&color);
            series.symbol.color = color;
        }
        changed
    }

    /// Vrai si [`PanelSpec::move_series`] déplacerait la série `index`.
    pub fn can_move_series(&self, index: usize, up: bool) -> bool {
        self.clone().move_series(index, up).is_some()
    }

    /// Vrai si [`PanelSpec::move_group`] déplacerait le groupe `group`.
    pub fn can_move_group(&self, group: u32, up: bool) -> bool {
        self.clone().move_group(group, up)
    }

    /// Remonte (`up`) ou descend le groupe `group` d'un emplacement (#197).
    /// Faux en butée ou si le groupe n'existe pas.
    pub fn move_group(&mut self, group: u32, up: bool) -> bool {
        let mut slots = self.slots();
        let Some(rank) = slots
            .iter()
            .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))
        else {
            return false;
        };
        let next = if up {
            rank.checked_sub(1)
        } else {
            Some(rank + 1)
        };
        let Some(next) = next.filter(|n| *n < slots.len()) else {
            return false;
        };
        slots.swap(rank, next);
        self.apply_slots(&slots, 0);
        true
    }

    /// Vrai si une série du panneau référence cette colonne.
    pub fn references(&self, column: NodeId) -> bool {
        self.series.iter().any(|s| s.references(column))
    }
}

impl Default for PanelSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: crate::TextFont::default(),
            title_color: String::new(),
            series: Vec::new(),
            bottom: AxisSpec::default(),
            left: AxisSpec::default(),
            right: AxisSpec::default(),
            top: AxisSpec::hidden(),
            legend: LegendSpec::default(),
            overlay: None,
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
            group_names: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    fn serie(n: u64) -> SeriesSpec {
        SeriesSpec::new(NodeId::from_raw(n))
    }

    #[test]
    fn des_series_ajoutees_ensemble_forment_un_groupe_une_seule_non() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![serie(1)]);
        assert_eq!(panel.series[0].group, None);
        panel.push_series_batch(vec![serie(2), serie(3)]);
        assert_eq!(panel.group_members(1), [1, 2]);
        panel.push_series_batch(vec![serie(4), serie(5)]);
        assert_eq!(panel.group_ids(), [1, 2]);
        assert_eq!(panel.next_group_id(), 3);
    }

    #[test]
    fn le_nuancier_colore_les_series_du_groupe_dans_l_ordre_et_elles_seules() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![serie(1)]);
        panel.push_series_batch((2..=4).map(serie).collect());
        let seule = panel.series[0].clone();
        let stops = ["#000000", "#ff0000"];
        assert!(panel.apply_group_gradient(1, &stops));
        assert_eq!(panel.series[0], seule, "la série hors groupe est intacte");
        let couleurs: Vec<&str> = panel.series[1..]
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(couleurs, ["#000000", "#800000", "#ff0000"]);
        assert!(panel.series[1..]
            .iter()
            .all(|s| s.symbol.color == s.line.color));
        assert!(!panel.apply_group_gradient(1, &stops), "déjà appliqué");
    }

    #[test]
    fn un_nuancier_illisible_ne_change_rien() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch((1..=3).map(serie).collect());
        let avant = panel.clone();
        assert!(!panel.apply_group_gradient(1, &["pas une couleur"]));
        assert_eq!(panel, avant);
        assert!(!panel.apply_group_gradient(9, &["#000000", "#ffffff"]));
    }

    /// Un panneau « 1, [2 3], 4, [5 6] », les colonnes valant leur rang.
    fn panneau() -> PanelSpec {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![serie(1)]);
        panel.push_series_batch(vec![serie(2), serie(3)]);
        panel.push_series_batch(vec![serie(4)]);
        panel.push_series_batch(vec![serie(5), serie(6)]);
        panel
    }

    /// Les colonnes Y dans l'ordre du panneau.
    fn colonnes(panel: &PanelSpec) -> Vec<u64> {
        panel.series.iter().map(|s| s.y.raw()).collect()
    }

    #[test]
    fn un_groupe_se_projette_en_un_emplacement_avec_tous_ses_membres() {
        let slots = panneau().slots();
        assert_eq!(
            slots,
            [
                SeriesSlot::Single(0),
                SeriesSlot::Group {
                    id: 1,
                    members: vec![1, 2]
                },
                SeriesSlot::Single(3),
                SeriesSlot::Group {
                    id: 2,
                    members: vec![4, 5]
                },
            ]
        );
    }

    #[test]
    fn un_groupe_aux_membres_disperses_se_rassemble_a_la_place_du_premier() {
        let mut panel = panneau();
        panel.series.swap(2, 3); // 1, 2, 4, 3, …
        assert_eq!(
            panel.slots()[1],
            SeriesSlot::Group {
                id: 1,
                members: vec![1, 3]
            }
        );
    }

    #[test]
    fn un_nom_de_groupe_se_pose_se_lit_et_se_vide() {
        let mut panel = panneau();
        assert_eq!(panel.group_label(1), None);
        assert!(panel.rename_group(1, "  Blancs  "));
        assert_eq!(panel.group_label(1), Some("Blancs"));
        assert!(
            !panel.rename_group(1, "Blancs"),
            "même nom : rien ne change"
        );
        assert!(panel.rename_group(1, ""));
        assert_eq!(panel.group_label(1), None);
        assert!(!panel.rename_group(9, "Fantôme"), "groupe inexistant");
        assert!(panel.group_names.is_empty());
    }

    #[test]
    fn le_nom_d_un_groupe_disparu_ne_revient_pas_a_un_groupe_neuf() {
        let mut panel = panneau();
        panel.rename_group(2, "Rouges");
        assert!(panel.dissolve_group(2));
        assert!(panel.group_names.is_empty(), "dissoudre oublie le nom");
        // Un nom resté seul (fichier édité) réserve son numéro sans nommer.
        panel.group_names.insert(7, "Reste".into());
        assert_eq!(panel.next_group_id(), 8);
        assert_eq!(panel.group_label(7), None);
    }

    #[test]
    fn entrer_dans_un_groupe_place_la_serie_a_la_suite_de_ses_membres() {
        let mut panel = panneau();
        // La série seule « 4 » (rang 3) rejoint le groupe 1 : 1, [2 3 4], [5 6].
        assert_eq!(panel.set_series_group(3, Some(1)), Some(3));
        assert_eq!(colonnes(&panel), [1, 2, 3, 4, 5, 6]);
        // La première série seule rejoint le groupe 2 : [2 3 4], [5 6 1].
        assert_eq!(panel.set_series_group(0, Some(2)), Some(5));
        assert_eq!(colonnes(&panel), [2, 3, 4, 5, 6, 1]);
        assert_eq!(panel.group_members(2), [3, 4, 5]);
    }

    #[test]
    fn sortir_d_un_groupe_place_la_serie_juste_derriere_lui() {
        let mut panel = panneau();
        // « 2 » quitte le groupe 1 : 1, [3], 2, 4, [5 6].
        assert_eq!(panel.leave_group(1), Some(2));
        assert_eq!(colonnes(&panel), [1, 3, 2, 4, 5, 6]);
        assert_eq!(panel.series[2].group, None);
        // « 3 » est le dernier : le groupe 1 disparaît, la série reste en place.
        assert_eq!(panel.leave_group(1), Some(1));
        assert_eq!(panel.group_ids(), [2]);
        assert_eq!(panel.leave_group(0), None, "déjà seule");
    }

    #[test]
    fn un_groupe_vide_par_depart_perd_son_nom() {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![serie(1), serie(2)]);
        panel.rename_group(1, "Éphémère");
        panel.leave_group(0);
        panel.leave_group(0);
        assert!(panel.group_names.is_empty());
    }

    #[test]
    fn commencer_un_groupe_donne_un_numero_neuf() {
        let mut panel = panneau();
        assert_eq!(panel.start_group(0), Some(3));
        assert_eq!(panel.group_members(3), [0]);
        assert_eq!(panel.start_group(0), None, "déjà seule dans son groupe");
    }

    #[test]
    fn dissoudre_garde_les_series_et_leur_style() {
        let mut panel = panneau();
        panel.series[1].line.width = 3.5;
        let avant = colonnes(&panel);
        assert!(panel.dissolve_group(1));
        assert_eq!(colonnes(&panel), avant);
        assert_eq!(panel.series[1].group, None);
        assert_eq!(panel.series[1].line.width, 3.5);
        assert!(!panel.dissolve_group(1));
    }

    #[test]
    fn supprimer_le_groupe_retire_ses_series_et_son_nom() {
        let mut panel = panneau();
        panel.rename_group(1, "À jeter");
        assert!(panel.remove_group(1));
        assert_eq!(colonnes(&panel), [1, 4, 5, 6]);
        assert!(panel.group_names.is_empty());
        assert!(!panel.remove_group(1));
    }

    #[test]
    fn monter_et_descendre_une_serie_restent_dans_son_groupe() {
        let mut panel = panneau();
        assert_eq!(panel.move_series(1, true), None, "premier du groupe");
        assert_eq!(panel.move_series(1, false), Some(2));
        assert_eq!(colonnes(&panel), [1, 3, 2, 4, 5, 6]);
        assert_eq!(panel.move_series(2, false), None, "dernier du groupe");
    }

    #[test]
    fn une_serie_seule_saute_par_dessus_un_groupe_entier() {
        let mut panel = panneau();
        assert_eq!(panel.move_series(3, true), Some(1));
        assert_eq!(colonnes(&panel), [1, 4, 2, 3, 5, 6]);
        assert_eq!(panel.move_series(1, true), Some(0));
        assert_eq!(panel.move_series(0, true), None);
        assert_eq!(panel.move_series(0, false), Some(1));
    }

    #[test]
    fn un_groupe_se_deplace_en_bloc() {
        let mut panel = panneau();
        assert!(panel.move_group(2, true));
        assert_eq!(colonnes(&panel), [1, 2, 3, 5, 6, 4]);
        assert!(panel.move_group(1, false));
        assert_eq!(colonnes(&panel), [1, 5, 6, 2, 3, 4]);
        assert!(panel.move_group(2, true));
        assert_eq!(colonnes(&panel), [5, 6, 1, 2, 3, 4]);
        assert!(!panel.move_group(2, true), "butée");
        assert!(!panel.move_group(9, true), "inexistant");
    }
}
