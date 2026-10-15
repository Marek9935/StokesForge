//! Correspondance pixel ↔ données d'un panneau tracé — tâche R7.
//!
//! # Pourquoi ici, et pas dans `stokesforge-render-egui`
//!
//! Rien ci-dessous ne dépend d'egui : c'est de la géométrie pure, au même
//! titre que [`crate::scale`]. Un panneau tracé a une zone de tracé en
//! pixels ([`DrawnPanel::pixel_origin`], [`DrawnPanel::pixel_size`], ajoutés
//! par cette tâche) et un cadrage en données ([`DrawnPanel::x_range`],
//! [`DrawnPanel::y_range`]) ; passer de l'un à l'autre ne regarde aucun
//! backend en particulier, et le placer ici évite exactement la duplication
//! que la règle de `roadmap-rendu.md` (« aucune fonctionnalité n'entre dans
//! un seul backend ») existe pour empêcher. `stokesforge-render-svg` n'en a pas
//! l'usage aujourd'hui (un SVG statique n'a pas de curseur), mais rien ne
//! l'empêcherait d'en avoir un jour — un lecteur SVG interactif, par
//! exemple — sans avoir à redescendre ce module d'un cran.
//!
//! # La transformation, en une phrase
//!
//! Un axe travaille dans un espace **transformé** ([`crate::scale::forward`])
//! qui est linéaire par construction — y compris sur une échelle
//! logarithmique, où c'est log₁₀(valeur) qui l'est. La correspondance
//! pixel ↔ transformé est donc une simple interpolation linéaire ; le
//! passage à l'espace des données se fait par [`crate::scale::inverse`] de
//! part et d'autre. C'est ce qui fait qu'un zoom molette reste correct sur
//! un axe log sans qu'aucun appelant n'ait à le savoir.
//!
//! # L'axe Y est inversé, et ce module porte la seule correction
//!
//! Un pixel croît vers le bas ; une donnée croît vers le haut. `pixel_to_data`
//! et `data_to_pixel` portent ce retournement une fois pour toutes — la
//! dérivation exacte (et le sens du glisser qui s'ensuit) est vérifiée par
//! les tests de ce module plutôt qu'affirmée en commentaire.

use crate::pipeline::DrawnPanel;
use crate::scale::{forward, inverse};
use crate::series;
use stokesforge_core::resolve::resolve_panel;
use stokesforge_core::Tree;
use stokesforge_plot::{AxisScale, PlotSpec, SeriesAxis};

/// La correspondance pixel ↔ données d'un panneau déjà tracé.
///
/// Un geste de cadrage, exprimé dans le repère de la zone de tracé (pixels) :
/// le même pour tous les calques d'une case, qui partagent cette zone (#167).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Zoom d'un `factor` centré sur le pixel `anchor` (voir [`PanelTransform::zoom`]).
    Zoom {
        /// Pixel qui garde sa donnée.
        anchor: (f64, f64),
        /// `< 1` rapproche, `> 1` éloigne.
        factor: f64,
    },
    /// Glisser de `(dx, dy)` pixels (voir [`PanelTransform::pan`]).
    Pan {
        /// Déplacement horizontal.
        dx: f64,
        /// Déplacement vertical.
        dy: f64,
    },
    /// Rectangle de zoom entre deux coins en pixels (voir
    /// [`PanelTransform::range_between`]).
    Box {
        /// Premier coin.
        a: (f64, f64),
        /// Coin opposé.
        b: (f64, f64),
    },
}

/// Construite à partir d'un [`DrawnPanel`] ; se reconstruit à l'identique
/// tant que rien n'a bougé, donc bon marché à refaire à chaque frame plutôt
/// qu'à mettre en cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelTransform {
    origin: (i32, i32),
    size: (u32, u32),
    x_range: (f64, f64),
    y_range: (f64, f64),
    x_scale: AxisScale,
    y_scale: AxisScale,
}

impl PanelTransform {
    /// La transformation d'un panneau tracé, ou `None` si sa zone de tracé
    /// est dégénérée (largeur ou hauteur nulle — ne devrait pas arriver pour
    /// un panneau qui a franchi [`crate::pipeline::MIN_CELL`], mais une
    /// division par zéro n'a pas à compter sur cette garantie indirecte).
    pub fn of(panel: &DrawnPanel) -> Option<Self> {
        if panel.pixel_size.0 == 0 || panel.pixel_size.1 == 0 {
            return None;
        }
        Some(Self {
            origin: panel.pixel_origin,
            size: panel.pixel_size,
            x_range: panel.x_range,
            y_range: panel.y_range,
            x_scale: panel.x_scale,
            y_scale: panel.y_scale,
        })
    }

    /// La transformation d'un panneau pour une ordonnée donnée : celle de
    /// droite si `right` et que le panneau en a une (`y2_range`), sinon
    /// celle de gauche — la règle des séries et des textes libres (issue
    /// #35).
    pub fn of_axis(panel: &DrawnPanel, right: bool) -> Option<Self> {
        let mut transform = Self::of(panel)?;
        if let (true, Some(range)) = (right, panel.y2_range) {
            transform.y_range = range;
            transform.y_scale = panel.y2_scale;
        }
        Some(transform)
    }

    /// Vrai si le pixel donné tombe dans la zone de tracé de ce panneau.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (ox, oy) = (self.origin.0 as f64, self.origin.1 as f64);
        let (w, h) = (self.size.0 as f64, self.size.1 as f64);
        (ox..=ox + w).contains(&x) && (oy..=oy + h).contains(&y)
    }

    /// Donnée sous un pixel. `None` si l'une des deux bornes de l'un des
    /// deux axes n'a pas d'image sur son échelle (ex. borne négative sur un
    /// axe log) — un cas déjà possible pour un cadrage explicite incohérent
    /// (`pipeline::axis_range`), donc à laisser remonter plutôt qu'à
    /// paniquer.
    pub fn pixel_to_data(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let fx = (x - self.origin.0 as f64) / self.size.0 as f64;
        // Le pixel croît vers le bas, la donnée vers le haut : `1.0 - fy`.
        let fy = 1.0 - (y - self.origin.1 as f64) / self.size.1 as f64;
        let data_x = axis_from_frac(self.x_scale, self.x_range, fx)?;
        let data_y = axis_from_frac(self.y_scale, self.y_range, fy)?;
        Some((data_x, data_y))
    }

    /// Pixel d'une donnée. Réciproque de [`Self::pixel_to_data`] là où
    /// celle-ci est définie.
    pub fn data_to_pixel(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let fx = axis_to_frac(self.x_scale, self.x_range, x)?;
        let fy = axis_to_frac(self.y_scale, self.y_range, y)?;
        let px = self.origin.0 as f64 + fx * self.size.0 as f64;
        let py = self.origin.1 as f64 + (1.0 - fy) * self.size.1 as f64;
        Some((px, py))
    }

    /// Le cadrage `(x, y)` que ce geste donne à ces deux axes.
    pub fn apply(&self, gesture: &Gesture) -> Option<((f64, f64), (f64, f64))> {
        match *gesture {
            Gesture::Zoom { anchor, factor } => self.zoom(anchor, factor),
            Gesture::Pan { dx, dy } => self.pan(dx, dy),
            Gesture::Box { a, b } => self.range_between(a, b),
        }
    }

    /// Nouveau cadrage après un zoom centré sur `anchor` (en pixel), d'un
    /// `factor` (`< 1` rapproche, `> 1` éloigne). La donnée sous `anchor`
    /// reste sous `anchor` après le zoom — c'est la propriété que
    /// [`tests::zoomer_fixe_le_point_sous_le_curseur`] vérifie.
    pub fn zoom(&self, anchor: (f64, f64), factor: f64) -> Option<((f64, f64), (f64, f64))> {
        let (ax, ay) = self.pixel_to_data(anchor.0, anchor.1)?;
        let x_range = zoom_range(self.x_scale, self.x_range, ax, factor)?;
        let y_range = zoom_range(self.y_scale, self.y_range, ay, factor)?;
        Some((x_range, y_range))
    }

    /// Nouveau cadrage après un glisser de `(dx, dy)` pixels — la donnée qui
    /// était sous le curseur au début du geste reste sous le curseur à la
    /// fin (`tests::glisser_fait_suivre_le_contenu_au_curseur`).
    pub fn pan(&self, dx: f64, dy: f64) -> Option<((f64, f64), (f64, f64))> {
        let fx = dx / self.size.0 as f64;
        // Le pixel Y est inversé par rapport à la donnée : le signe change.
        let fy = -dy / self.size.1 as f64;
        let x_range = pan_range(self.x_scale, self.x_range, fx)?;
        let y_range = pan_range(self.y_scale, self.y_range, fy)?;
        Some((x_range, y_range))
    }

    /// Le cadrage délimité par deux coins en pixel — le rectangle de zoom
    /// élastique de la tâche 7.4. L'ordre des coins n'importe pas : chaque
    /// paire est remise dans le sens de l'axe affiché, car un cadrage
    /// « de 800 à 400 » trace l'axe à l'envers (`pipeline::axis_range`).
    pub fn range_between(
        &self,
        corner_a: (f64, f64),
        corner_b: (f64, f64),
    ) -> Option<((f64, f64), (f64, f64))> {
        let (ax, ay) = self.pixel_to_data(corner_a.0, corner_a.1)?;
        let (bx, by) = self.pixel_to_data(corner_b.0, corner_b.1)?;
        let oriented = |(a, b): (f64, f64), shown: (f64, f64)| {
            if (a < b) == (shown.0 < shown.1) {
                (a, b)
            } else {
                (b, a)
            }
        };
        Some((
            oriented((ax, bx), self.x_range),
            oriented((ay, by), self.y_range),
        ))
    }
}

/// La fraction (0 = `range.0`, 1 = `range.1`) de la valeur `v` sur l'axe,
/// dans l'espace transformé.
fn axis_to_frac(scale: AxisScale, (r0, r1): (f64, f64), v: f64) -> Option<f64> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let t = forward(scale, v)?;
    if t1 == t0 {
        return None;
    }
    Some((t - t0) / (t1 - t0))
}

/// La donnée à la fraction `frac` (0 = `range.0`, 1 = `range.1`) de l'axe.
fn axis_from_frac(scale: AxisScale, (r0, r1): (f64, f64), frac: f64) -> Option<f64> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    Some(inverse(scale, t0 + frac * (t1 - t0)))
}

/// Un cadrage zoomé d'un `factor` autour de la valeur `anchor`, dans
/// l'espace transformé — la dérivation qui garantit que `anchor` garde la
/// même fraction avant et après (donc le même pixel) est en tête de module.
fn zoom_range(
    scale: AxisScale,
    (r0, r1): (f64, f64),
    anchor: f64,
    factor: f64,
) -> Option<(f64, f64)> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let ta = forward(scale, anchor)?;
    let n0 = ta + (t0 - ta) * factor;
    let n1 = ta + (t1 - ta) * factor;
    Some((inverse(scale, n0), inverse(scale, n1)))
}

/// Un cadrage décalé d'une fraction `frac` de sa propre étendue, dans
/// l'espace transformé. `frac > 0` déplace le cadrage vers le haut de
/// l'axe (donc, en pixel, fait apparaître comme si le contenu descendait) ;
/// [`PanelTransform::pan`] choisit le signe de `frac` pour chaque axe.
fn pan_range(scale: AxisScale, (r0, r1): (f64, f64), frac: f64) -> Option<(f64, f64)> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let shift = frac * (t1 - t0);
    Some((inverse(scale, t0 - shift), inverse(scale, t1 - shift)))
}

/// Le point d'une courbe déjà projetée le plus proche du curseur, en pixel.
///
/// L'équivalent du couple `ReverseCoordTranslate` + `min_by_key`
/// d'`architecture-choisie.md` §6 — sans le premier, dont ce module est déjà
/// l'implémentation. Rend l'indice dans `points` et la distance en pixels.
pub fn nearest_on_curve(
    transform: &PanelTransform,
    points: &[(f64, f64)],
    cursor: (f64, f64),
) -> Option<(usize, f64)> {
    points
        .iter()
        .enumerate()
        .filter_map(|(index, &(x, y))| {
            let (px, py) = transform.data_to_pixel(x, y)?;
            let squared = (px - cursor.0).powi(2) + (py - cursor.1).powi(2);
            Some((index, squared))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, squared)| (index, squared.sqrt()))
}

/// Un point désigné sur une courbe : ce que [`nearest_point`] rend, en
/// `f64` et sans type d'interface (la conversion en pixels egui est celle de
/// `stokesforge-render-egui`, qui en fait son `PointHit`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestPoint {
    /// Indice de la série dans `PanelSpec::series`, résolu dans le même
    /// ordre (`ResolvedPanel::series`).
    pub series: usize,
    /// Indice du point désigné dans sa série.
    pub point_index: usize,
    /// Position du point, dans l'espace de la géométrie préparée
    /// ([`series::prepare`]).
    pub data: (f64, f64),
    /// Position du point, en pixels de la figure.
    pub pixel: (f64, f64),
    /// Distance du curseur au point, en pixels.
    pub distance: f64,
}

/// Le point désigné du panneau tracé `drawn_panel` sous le curseur `cursor`
/// (pixels de la figure) — tâche R7.2, l'équivalent local de
/// `ReverseCoordTranslate` + `min_by_key` d'`architecture-choisie.md` §6
/// (`min_by_key` vit dans [`nearest_on_curve`], réutilisé série par série).
/// `None` si aucun point n'est à `radius` pixels ou moins.
///
/// **Limité à l'axe principal** (bas, gauche) : une série de l'axe secondaire
/// vivrait sur un second [`PanelTransform`] (`of_axis`) qu'il faudrait
/// construire à côté, et rien aujourd'hui n'en a l'usage. Écart assumé, pas
/// une omission tue. Les séries masquées, brisées ou de type inconnu, et
/// celles qui ne sont pas une courbe (histogramme, barres), sont ignorées.
///
/// Recalcule la géométrie plutôt que de la garder en mémoire : une série
/// résolue ne porte que des `Arc<Vec<f64>>` clonés par compteur
/// (`stokesforge-core::resolve`), donc le refaire à chaque survol coûte un
/// parcours, pas une copie — moins cher que de le garder à jour à travers
/// chaque zoom.
pub fn nearest_point(
    tree: &Tree,
    spec: &PlotSpec,
    drawn_panel: &DrawnPanel,
    transform: &PanelTransform,
    cursor: (f64, f64),
    radius: f64,
) -> Option<NearestPoint> {
    let spec_panel = spec.panels.get(drawn_panel.index)?;
    let resolved = resolve_panel(tree, spec_panel);

    let mut best: Option<NearestPoint> = None;
    for (series_index, resolved_series) in resolved.series.iter().enumerate() {
        let series_spec = resolved_series.spec;
        if !series_spec.visible || series_spec.axis == SeriesAxis::Right {
            continue;
        }
        let prepared = series::prepare(
            resolved_series,
            spec_panel.bottom.scale,
            spec_panel.left.scale,
        );
        if prepared.broken || prepared.unknown_type {
            continue;
        }
        let series::Geometry::Curve { points, .. } = &prepared.geometry else {
            continue;
        };
        let Some((point_index, distance)) = nearest_on_curve(transform, points, cursor) else {
            continue;
        };
        if best.is_some_and(|hit| hit.distance <= distance) {
            continue;
        }
        let Some(pixel) = transform.data_to_pixel(points[point_index].0, points[point_index].1)
        else {
            continue;
        };
        best = Some(NearestPoint {
            series: series_index,
            point_index,
            data: points[point_index],
            pixel,
            distance,
        });
    }
    best.filter(|hit| hit.distance <= radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panel;
    use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::{PanelSpec, SeriesSpec};

    /// Un panneau de 100×100 px, cadré sur `0..100` des deux côtés, en
    /// linéaire — assez pour vérifier le sens des deux axes sans que
    /// l'échelle log ne mélange la lecture des chiffres.
    fn carre() -> PanelTransform {
        PanelTransform {
            origin: (0, 0),
            size: (100, 100),
            x_range: (0.0, 100.0),
            y_range: (0.0, 100.0),
            x_scale: AxisScale::Linear,
            y_scale: AxisScale::Linear,
        }
    }

    #[test]
    fn le_coin_haut_gauche_est_x_min_y_max() {
        let t = carre();
        assert_eq!(t.pixel_to_data(0.0, 0.0), Some((0.0, 100.0)));
        assert_eq!(t.pixel_to_data(100.0, 100.0), Some((100.0, 0.0)));
    }

    #[test]
    fn aller_retour_pixel_donnee_est_lidentite() {
        let t = carre();
        for (x, y) in [(0.0, 0.0), (37.0, 61.0), (100.0, 100.0)] {
            let (dx, dy) = t.pixel_to_data(x, y).expect("pixel dans le panneau");
            let (px, py) = t.data_to_pixel(dx, dy).expect("donnée représentable");
            assert!((px - x).abs() < 1e-9 && (py - y).abs() < 1e-9, "{px},{py}");
        }
    }

    #[test]
    fn glisser_fait_suivre_le_contenu_au_curseur() {
        // La donnée sous le pixel (20, 30) doit se retrouver sous le pixel
        // (20 + 15, 30 + 8) après un glisser de (15, 8) px — c'est la
        // définition même de « faire glisser le graphe ».
        let t = carre();
        let (dx, dy) = t.pixel_to_data(20.0, 30.0).expect("dans le panneau");
        let (x_range, y_range) = t.pan(15.0, 8.0).expect("pan linéaire toujours défini");
        let after = PanelTransform {
            x_range,
            y_range,
            ..t
        };
        let (px, py) = after
            .data_to_pixel(dx, dy)
            .expect("la même donnée reste représentable");
        assert!((px - 35.0).abs() < 1e-9, "px = {px}");
        assert!((py - 38.0).abs() < 1e-9, "py = {py}");
    }

    #[test]
    fn zoomer_fixe_le_point_sous_le_curseur() {
        let t = carre();
        let anchor = (70.0, 20.0);
        let anchor_data = t
            .pixel_to_data(anchor.0, anchor.1)
            .expect("dans le panneau");
        let (x_range, y_range) = t.zoom(anchor, 0.5).expect("zoom linéaire toujours défini");
        let after = PanelTransform {
            x_range,
            y_range,
            ..t
        };
        let (px, py) = after
            .data_to_pixel(anchor_data.0, anchor_data.1)
            .expect("le point sous le curseur reste représentable");
        assert!((px - anchor.0).abs() < 1e-9, "px = {px}");
        assert!((py - anchor.1).abs() < 1e-9, "py = {py}");
        // Et le cadrage a bien rétréci de moitié.
        assert!((x_range.1 - x_range.0 - 50.0).abs() < 1e-9, "{x_range:?}");
    }

    #[test]
    fn zoomer_sur_une_echelle_log_reste_coherent() {
        let t = PanelTransform {
            origin: (0, 0),
            size: (100, 100),
            x_range: (1.0, 1_000.0),
            y_range: (0.0, 100.0),
            x_scale: AxisScale::Log,
            y_scale: AxisScale::Linear,
        };
        // Le milieu du panneau, en x, est la moyenne géométrique 1..1000,
        // soit environ 31,6 — pas 500,5 comme en linéaire.
        let (mid_x, _) = t.pixel_to_data(50.0, 100.0).expect("dans le panneau");
        assert!((mid_x - 31.622_776_6).abs() < 1e-4, "{mid_x}");

        let (x_range, _) = t.zoom((50.0, 50.0), 0.5).expect("zoom log toujours défini");
        // Toujours centré sur le même point après le zoom.
        let after = PanelTransform { x_range, ..t };
        let (px, _) = after.data_to_pixel(mid_x, 50.0).expect("représentable");
        assert!((px - 50.0).abs() < 1e-6, "px = {px}");
    }

    #[test]
    fn le_rectangle_elastique_lit_les_deux_coins() {
        let t = carre();
        let (x_range, y_range) = t.range_between((10.0, 10.0), (60.0, 40.0)).unwrap();
        assert_eq!(x_range, (10.0, 60.0));
        // Y croît vers le haut : le coin haut (pixel 10) est la donnée la plus
        // grande, mais la paire reste dans le sens de l'axe affiché (bas → haut).
        assert_eq!(y_range, (60.0, 90.0));
        // Les coins pris dans l'autre ordre donnent le même cadrage.
        let swapped = t.range_between((60.0, 40.0), (10.0, 10.0)).unwrap();
        assert_eq!(swapped, (x_range, y_range));
    }

    #[test]
    fn hors_du_panneau_nest_plus_dans_le_panneau() {
        let t = carre();
        assert!(t.contains(0.0, 0.0));
        assert!(t.contains(100.0, 100.0));
        assert!(!t.contains(-1.0, 50.0));
        assert!(!t.contains(50.0, 101.0));
    }

    #[test]
    fn le_point_le_plus_proche_est_bien_le_plus_proche() {
        let t = carre();
        let points = [(0.0, 0.0), (50.0, 50.0), (100.0, 0.0)];
        // Curseur près du pixel de (50, 50) (data), soit pixel (50, 50).
        let (index, distance) = nearest_on_curve(&t, &points, (52.0, 51.0)).unwrap();
        assert_eq!(index, 1);
        assert!(distance < 3.0, "{distance}");
    }

    /// Un projet à une feuille dont les colonnes sont `colonnes`, et leurs
    /// identifiants dans le même ordre.
    fn projet(colonnes: &[Vec<f64>]) -> (Project, Vec<NodeId>) {
        let mut project = Project::new("Désignation").expect("projet");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("classeur");
        let sheet = project.add_worksheet(book, None).expect("feuille");
        let ids = colonnes
            .iter()
            .map(|valeurs| {
                let colonne = Column::from_block(
                    ColumnType::Numeric,
                    ValueBlock::Float(valeurs.clone()),
                    ColumnMeta::default(),
                )
                .expect("colonne");
                project
                    .add_column(sheet, None, None, colonne)
                    .expect("ajout")
            })
            .collect();
        (project, ids)
    }

    /// Désigne le point sous `cursor` d'un panneau 100×100 px cadré sur
    /// `0..100`, dont `series` sont les courbes.
    fn designer(
        colonnes: &[Vec<f64>],
        series: impl Fn(&[NodeId]) -> Vec<SeriesSpec>,
        cursor: (f64, f64),
        radius: f64,
    ) -> Option<NearestPoint> {
        let (project, ids) = projet(colonnes);
        let mut spec_panel = PanelSpec::at(0, 0);
        spec_panel.series.extend(series(&ids));
        let spec = PlotSpec::empty("Désignation").with_panels(spec_panel);
        let drawn = panel(0, 0, 0);
        let transform = PanelTransform::of(&drawn).expect("zone de tracé");
        nearest_point(project.tree(), &spec, &drawn, &transform, cursor, radius)
    }

    fn diagonale() -> Vec<Vec<f64>> {
        vec![vec![0.0, 50.0, 100.0], vec![0.0, 50.0, 100.0]]
    }

    #[test]
    fn le_point_designe_est_le_plus_proche_du_curseur() {
        // Le point (50, 50) est au pixel (50, 50) : l'ordonnée est retournée.
        let hit = designer(
            &diagonale(),
            |ids| vec![SeriesSpec::new_xy(ids[0], ids[1])],
            (52.0, 51.0),
            12.0,
        )
        .expect("un point à portée");
        assert_eq!((hit.series, hit.point_index), (0, 1));
        assert_eq!(hit.data, (50.0, 50.0));
        assert_eq!(hit.pixel, (50.0, 50.0));
        assert!((hit.distance - 5f64.sqrt()).abs() < 1e-9, "{hit:?}");
    }

    #[test]
    fn hors_de_portee_rien_n_est_designe() {
        let colonnes = diagonale();
        let series = |ids: &[NodeId]| vec![SeriesSpec::new_xy(ids[0], ids[1])];
        // Le coin haut gauche est à plus de 12 px de la diagonale (le point
        // le plus proche, (0, 0), est au pixel (0, 100)).
        assert_eq!(designer(&colonnes, series, (10.0, 10.0), 12.0), None);
        // À 20 px du point (50, 50) : hors d'un rayon de 12, dans un de 25.
        assert_eq!(designer(&colonnes, series, (70.0, 50.0), 12.0), None);
        let hit = designer(&colonnes, series, (70.0, 50.0), 25.0).expect("à portée");
        assert_eq!(hit.point_index, 1);
    }

    #[test]
    fn une_serie_masquee_ou_de_droite_n_est_pas_designee() {
        let colonnes = vec![
            vec![0.0, 50.0, 100.0],
            vec![0.0, 50.0, 100.0],
            // Une seconde courbe, loin de la première (décalée de 10 px).
            vec![0.0, 40.0, 90.0],
        ];
        let curseur = (50.0, 50.0);
        let deux = |modifier: fn(&mut SeriesSpec)| {
            move |ids: &[NodeId]| {
                let mut proche = SeriesSpec::new_xy(ids[0], ids[1]);
                modifier(&mut proche);
                vec![proche, SeriesSpec::new_xy(ids[0], ids[2])]
            }
        };
        let libre = designer(&colonnes, deux(|_| {}), curseur, 12.0).expect("à portée");
        assert_eq!((libre.series, libre.point_index), (0, 1));

        let masquee = designer(&colonnes, deux(|s| s.visible = false), curseur, 12.0)
            .expect("l'autre courbe est à portée");
        assert_eq!((masquee.series, masquee.point_index), (1, 1));
        assert_eq!(masquee.data, (50.0, 40.0));

        let droite = designer(
            &colonnes,
            deux(|s| s.axis = stokesforge_plot::SeriesAxis::Right),
            curseur,
            12.0,
        )
        .expect("l'autre courbe est à portée");
        assert_eq!(droite.series, 1);

        // Seule, une série masquée ne désigne rien.
        let seule = |ids: &[NodeId]| {
            let mut s = SeriesSpec::new_xy(ids[0], ids[1]);
            s.visible = false;
            vec![s]
        };
        assert_eq!(designer(&colonnes, seule, curseur, 12.0), None);
    }

    #[test]
    fn un_panneau_inconnu_du_spec_ne_designe_rien() {
        let (project, _) = projet(&[]);
        let spec = PlotSpec::empty("Vide").with_panels(PanelSpec::at(0, 0));
        let drawn = panel(5, 0, 0);
        let transform = PanelTransform::of(&drawn).expect("zone de tracé");
        assert_eq!(
            nearest_point(project.tree(), &spec, &drawn, &transform, (1.0, 1.0), 12.0),
            None
        );
    }

    #[test]
    fn le_panneau_designe_est_celui_du_calque_trace() {
        // Le calque tracé est le second du spec : c'est ses séries qu'on lit,
        // pas celles du premier.
        let (project, ids) = projet(&diagonale());
        let mut second = PanelSpec::at(0, 1);
        second.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
        let spec = PlotSpec::empty("Deux").with_panels(stokesforge_plot::Panels::new(vec![
            PanelSpec::at(0, 0),
            second,
        ]));
        let drawn = panel(1, 0, 1);
        let transform = PanelTransform::of(&drawn).expect("zone de tracé");
        let hit = nearest_point(
            project.tree(),
            &spec,
            &drawn,
            &transform,
            (50.0, 50.0),
            12.0,
        )
        .expect("un point à portée");
        assert_eq!(hit.point_index, 1);
    }
}
