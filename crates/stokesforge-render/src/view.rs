//! L'état de vue — zoom et pan, tâche R7.1.
//!
//! # Ce que ce module n'est pas
//!
//! Ce n'est **pas** une mutation du modèle. `CLAUDE.md` est catégorique :
//! « toute mutation du modèle passe par une commande annulable de
//! `stokesforge-commands` » — mais un zoom molette n'est pas une mutation du
//! modèle, c'est un cadrage de vue éphémère, et le faire passer par une
//! commande annulable en ferait une entrée d'historique au même titre qu'un
//! renommage de colonne. Personne ne veut annuler un zoom avec `Ctrl+Z`.
//! [`ViewState`] ne touche donc jamais au `PlotSpec` stocké dans le
//! [`stokesforge_core::Tree`] : il vit à côté, dans `stokesforge-render`
//! (#262 : rien ici ne dépend d'egui), et `GraphView`, de
//! `stokesforge-render-egui`, le porte comme champ.
//!
//! # Le mécanisme : un cadrage explicite, appliqué à un clone
//!
//! `stokesforge-plot` a déjà tout ce qu'il faut pour qu'un cadrage explicite
//! prime sur les données calculées — c'est
//! [`stokesforge_plot::AxisSpec::has_explicit_range`], exercé par
//! `crates/stokesforge-render/tests/geometrie.rs::un_cadrage_explicite_prime_sur_les_donnees`.
//! [`ViewState::apply`] pose `auto_range = false` avec les deux bornes sur
//! un **clone** du `PlotSpec`, juste avant `resolve_spec` — jamais sur
//! l'original. Rien de neuf n'a donc été inventé pour le cadrage lui-même :
//! seul le déclenchement (molette, glisser, rectangle élastique — R7.1/7.4)
//! est nouveau, dans `GraphView` ; le calcul du cadrage qu'un geste donne
//! ([`ViewState::commit_gesture`], [`ViewState::commit_axis_gesture`]) est
//! ici, sans egui, donc testable sans fenêtre.
//!
//! # Identité d'un panneau : son indice dans le spec
//!
//! [`stokesforge_render::DrawnPlot::panels`] omet les panneaux trop petits pour
//! être tracés ([`stokesforge_render::pipeline::MIN_CELL`]) : son indice n'est
//! donc pas stable d'une image à l'autre si la fenêtre change de taille. Le
//! cadrage est donc rangé par l'indice du calque dans
//! [`stokesforge_plot::PlotSpec::panels`] (`DrawnPanel::index`), stable et
//! distinct même quand plusieurs calques se superposent dans la même case.
//!
//! # La clé de cache : un compteur, pas les bornes
//!
//! Le zoom/pan change la géométrie tracée — contrairement au *feathering*,
//! qui ne change que la tessellation (`stokesforge_render_egui::feathering`, et
//! `architecture-choisie.md` §4.2). Il **doit** donc entrer dans la clé de
//! cache (`stokesforge_render_egui::CacheKey::view_generation`).
//! [`ViewState::generation`] est ce qui y entre : un entier qui monte
//! à chaque cadrage commis, plutôt que les bornes elles-mêmes (variables en
//! nombre, par panneau, en `f64` — rien de bien `Copy`).

use std::collections::HashMap;

use stokesforge_plot::PlotSpec;

use crate::hit::AxisSide;
use crate::interact::{Gesture, PanelTransform};
use crate::pipeline::DrawnPanel;

/// Le cadrage explicite des **échelles propres** d'un calque : les bornes que
/// le zoom/pan ont calculées, en espace de données (pas transformé — comme
/// [`stokesforge_plot::AxisSpec::range_min`]/`range_max`, dont c'est directement
/// la valeur). Une échelle absente n'est pas cadrée : une échelle liée à un
/// autre calque (#167) n'a pas de cadrage à elle, elle suit celle du calque
/// qu'elle recouvre.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Framing {
    /// Bornes de l'axe des abscisses. L'ordre n'importe pas : la résolution
    /// les trie de toute façon (`pipeline::axis_range`).
    pub x: Option<(f64, f64)>,
    /// Bornes de l'ordonnée principale.
    pub y: Option<(f64, f64)>,
    /// Bornes de l'ordonnée de droite.
    pub y2: Option<(f64, f64)>,
}

impl Framing {
    /// Cadrage des seuls axes principaux.
    pub fn xy(x: (f64, f64), y: (f64, f64)) -> Self {
        Self {
            x: Some(x),
            y: Some(y),
            y2: None,
        }
    }
}

/// L'état de vue d'un `GraphView` (`stokesforge-render-egui`) : les cadrages explicites en
/// cours, par panneau.
///
/// Vide par défaut — un graphe neuf s'affiche cadré sur ses données, comme
/// avant R7. Limité à l'axe principal (bas et gauche) : la désignation de
/// point (tâche 7.2) l'est aussi, pour la même raison — voir sa
/// documentation dans `stokesforge_render_egui::PointHit`.
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    framings: HashMap<usize, Framing>,
    generation: u64,
}

impl ViewState {
    /// Un état de vue neuf, sans aucun cadrage explicite.
    pub fn new() -> Self {
        Self::default()
    }

    /// Le compteur à poser dans `stokesforge_render_egui::CacheKey::view_generation`.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Vrai si aucun panneau n'a de cadrage explicite — le cas courant, où
    /// [`Self::apply`] n'a rien à faire.
    pub fn is_empty(&self) -> bool {
        self.framings.is_empty()
    }

    /// Le cadrage explicite du calque `panel`, s'il y en a un.
    pub fn framing(&self, panel: usize) -> Option<Framing> {
        self.framings.get(&panel).copied()
    }

    /// Pose le cadrage explicite d'un calque, échelle par échelle : une
    /// échelle absente de `framing` garde son cadrage précédent. Fait avancer
    /// la génération — donc invalide le cache.
    pub fn set_framing(&mut self, panel: usize, framing: Framing) {
        let entry = self.framings.entry(panel).or_default();
        entry.x = framing.x.or(entry.x);
        entry.y = framing.y.or(entry.y);
        entry.y2 = framing.y2.or(entry.y2);
        self.generation += 1;
    }

    /// Efface tous les cadrages explicites : retour au cadrage sur les
    /// données pour tous les panneaux. Sans effet — et donc sans invalider
    /// le cache — si aucun n'était posé.
    pub fn reset(&mut self) {
        if !self.framings.is_empty() {
            self.framings.clear();
            self.generation += 1;
        }
    }

    /// Applique un geste de cadrage (zoom, glisser, rectangle) à **toutes les
    /// échelles propres de tous les calques de la case** du calque
    /// `panel_index` (dans `panels`, les panneaux tracés — `DrawnPlot::panels`)
    /// (#167). Les calques d'une case partagent la même zone de tracé : la
    /// même transformation en pixels garde donc leurs graduations alignées, y
    /// compris celles d'un axe secondaire. Une échelle liée à un autre calque
    /// n'a pas de cadrage à elle et suit celle qu'elle recouvre.
    ///
    /// Sans effet si `panel_index` n'est pas un panneau tracé.
    pub fn commit_gesture(&mut self, panels: &[DrawnPanel], panel_index: usize, gesture: Gesture) {
        let Some(hit) = panels.get(panel_index) else {
            return;
        };
        let cell = (hit.row, hit.column);
        let mut updates = Vec::new();
        for panel in panels.iter().filter(|p| (p.row, p.column) == cell) {
            let (own_x, own_y) = match panel.overlay {
                Some(overlay) => (overlay.x_top, overlay.y_right),
                None => (true, true),
            };
            let mut framing = Framing::default();
            if let Some((x, y)) = PanelTransform::of(panel).and_then(|t| t.apply(&gesture)) {
                framing.x = own_x.then_some(x);
                framing.y = own_y.then_some(y);
            }
            if panel.overlay.is_none() && panel.y2_range.is_some() {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
            updates.push((panel.index, framing));
        }
        for (index, framing) in updates {
            self.set_framing(index, framing);
        }
    }

    /// Applique un geste à **une seule échelle** (#167) : celle de l'axe
    /// `side` du panneau `panel_index` (dans `panels`). Les autres échelles,
    /// liées ou non, ne bougent pas. Le geste n'agit que selon l'axe :
    /// l'autre composante est neutralisée au centre de la zone de tracé.
    ///
    /// Sans effet si `panel_index` n'est pas un panneau tracé.
    pub fn commit_axis_gesture(
        &mut self,
        panels: &[DrawnPanel],
        panel_index: usize,
        side: AxisSide,
        gesture: Gesture,
    ) {
        let Some(panel) = panels.get(panel_index) else {
            return;
        };
        let horizontal = side == AxisSide::Bottom;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let gesture = match gesture {
            Gesture::Zoom { anchor, factor } => Gesture::Zoom {
                anchor: if horizontal {
                    (anchor.0, center.1)
                } else {
                    (center.0, anchor.1)
                },
                factor,
            },
            Gesture::Pan { dx, dy } => {
                if horizontal {
                    Gesture::Pan { dx, dy: 0.0 }
                } else {
                    Gesture::Pan { dx: 0.0, dy }
                }
            }
            other => other,
        };
        let mut framing = Framing::default();
        match side {
            AxisSide::Bottom => {
                framing.x = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(x, _)| x);
            }
            AxisSide::Left => {
                framing.y = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y)| y);
            }
            AxisSide::Right => {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
        }
        self.set_framing(panel.index, framing);
    }

    /// Applique les cadrages en cours à un clone du `PlotSpec`, juste avant
    /// résolution. Ne touche à rien si aucun panneau n'a de cadrage
    /// explicite — le chemin rapide du cas courant, sans allocation ni
    /// parcours.
    pub fn apply(&self, spec: &mut PlotSpec) {
        if self.framings.is_empty() {
            return;
        }
        // `Panels` ne prête son contenu qu'en `&[PanelSpec]` (`Deref`, pas
        // `DerefMut` : le champ est privé, l'invariant « au moins un
        // panneau » se tient par construction) — d'où la boucle sur
        // l'indice et `get_mut` plutôt qu'une itération directe.
        for index in 0..spec.panels.len() {
            let Some(panel) = spec.panels.get_mut(index) else {
                continue;
            };
            let Some(framing) = self.framing(index) else {
                continue;
            };
            for (axis, range) in [
                (&mut panel.bottom, framing.x),
                (&mut panel.left, framing.y),
                (&mut panel.right, framing.y2),
            ] {
                if let Some(range) = range {
                    axis.auto_range = false;
                    axis.range_min = Some(range.0);
                    axis.range_max = Some(range.1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panel;
    use stokesforge_plot::{AxisScale, OverlaySpec, PanelSpec, Panels};

    fn spec_a_deux_panneaux() -> PlotSpec {
        PlotSpec::empty("Vue")
            .with_panels(Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]))
    }

    #[test]
    fn vide_ne_touche_a_rien() {
        let view = ViewState::new();
        let mut spec = spec_a_deux_panneaux();
        let avant = spec.clone();
        view.apply(&mut spec);
        assert_eq!(spec, avant);
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn poser_un_cadrage_ne_touche_que_son_panneau() {
        let mut view = ViewState::new();
        view.set_framing(0, Framing::xy((1.0, 2.0), (3.0, 4.0)));
        let mut spec = spec_a_deux_panneaux();
        view.apply(&mut spec);

        let premier = &spec.panels[0];
        assert!(!premier.bottom.auto_range);
        assert_eq!(premier.bottom.range_min, Some(1.0));
        assert_eq!(premier.bottom.range_max, Some(2.0));
        assert!(!premier.left.auto_range);
        assert_eq!(premier.left.range_min, Some(3.0));
        assert_eq!(premier.left.range_max, Some(4.0));

        let second = &spec.panels[1];
        assert!(second.bottom.auto_range, "le second panneau n'a pas bougé");
    }

    #[test]
    fn chaque_cadrage_pose_fait_avancer_la_generation() {
        let mut view = ViewState::new();
        assert_eq!(view.generation(), 0);
        view.set_framing(0, Framing::xy((0.0, 1.0), (0.0, 1.0)));
        assert_eq!(view.generation(), 1);
        view.set_framing(0, Framing::xy((0.0, 2.0), (0.0, 1.0)));
        assert_eq!(view.generation(), 2);
    }

    #[test]
    fn reset_sans_cadrage_ninvalide_rien() {
        let mut view = ViewState::new();
        view.reset();
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn reset_efface_et_fait_avancer_la_generation_une_fois() {
        let mut view = ViewState::new();
        view.set_framing(0, Framing::xy((0.0, 1.0), (0.0, 1.0)));
        view.reset();
        assert!(view.is_empty());
        assert_eq!(view.generation(), 2);
        let mut spec = spec_a_deux_panneaux();
        let avant = spec.clone();
        view.apply(&mut spec);
        assert_eq!(spec, avant, "plus rien à appliquer après un reset");
    }

    fn proche(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn un_glisser_decale_le_cadrage_dans_le_sens_du_curseur() {
        let panels = [panel(0, 0, 0)];
        let mut view = ViewState::new();
        view.commit_gesture(&panels, 0, Gesture::Pan { dx: 15.0, dy: 8.0 });
        let framing = view.framing(0).expect("cadrage posé");
        // Le contenu suit le curseur : vers la droite, le cadrage recule en X ;
        // vers le bas, il monte en Y (le pixel Y croît vers le bas).
        assert!(proche(framing.x.expect("x"), (-15.0, 85.0)), "{framing:?}");
        assert!(proche(framing.y.expect("y"), (8.0, 108.0)), "{framing:?}");
        assert_eq!(framing.y2, None);
        assert_eq!(view.generation(), 1);
    }

    #[test]
    fn un_zoom_garde_la_donnee_sous_le_curseur() {
        let panels = [panel(0, 0, 0)];
        let mut view = ViewState::new();
        view.commit_gesture(
            &panels,
            0,
            Gesture::Zoom {
                anchor: (20.0, 50.0),
                factor: 0.5,
            },
        );
        let framing = view.framing(0).expect("cadrage posé");
        assert!(proche(framing.x.expect("x"), (10.0, 60.0)), "{framing:?}");
        assert!(proche(framing.y.expect("y"), (25.0, 75.0)), "{framing:?}");
    }

    #[test]
    fn un_zoom_sur_axe_log_se_fait_sur_les_logarithmes() {
        let mut log = panel(0, 0, 0);
        log.x_range = (1.0, 1_000.0);
        log.x_scale = AxisScale::Log;
        let mut view = ViewState::new();
        view.commit_gesture(
            &[log],
            0,
            Gesture::Zoom {
                anchor: (50.0, 50.0),
                factor: 0.5,
            },
        );
        let (min, max) = view.framing(0).and_then(|f| f.x).expect("x cadré");
        // 10^0,75 .. 10^2,25 : centré sur la moyenne géométrique, pas sur 500.
        assert!((min - 10f64.powf(0.75)).abs() < 1e-6, "{min}");
        assert!((max - 10f64.powf(2.25)).abs() < 1e-6, "{max}");
    }

    #[test]
    fn un_axe_inverse_garde_son_sens_apres_un_rectangle() {
        let mut inverse = panel(0, 0, 0);
        inverse.x_range = (100.0, 0.0);
        let mut view = ViewState::new();
        view.commit_gesture(
            &[inverse],
            0,
            Gesture::Box {
                a: (10.0, 10.0),
                b: (60.0, 40.0),
            },
        );
        // Les pixels 10 et 60 sont les données 90 et 40 : la paire reste de
        // 100 vers 0, sinon la figure se retournerait au zoom.
        let x = view.framing(0).and_then(|f| f.x).expect("x cadré");
        assert!(proche(x, (90.0, 40.0)), "{x:?}");
    }

    #[test]
    fn un_geste_cadre_les_calques_de_la_case_et_pas_les_autres_cases() {
        let mut recouvre = panel(1, 0, 0);
        recouvre.overlay = Some(OverlaySpec {
            x_top: false,
            y_right: true,
        });
        recouvre.y_range = (0.0, 10.0);
        let mut base = panel(0, 0, 0);
        base.y2_range = Some((0.0, 1.0));
        let voisin = panel(2, 0, 1);
        let mut abscisse_propre = panel(3, 0, 0);
        abscisse_propre.overlay = Some(OverlaySpec {
            x_top: true,
            y_right: false,
        });
        let panels = [base, recouvre, voisin, abscisse_propre];

        let mut view = ViewState::new();
        view.commit_gesture(&panels, 0, Gesture::Pan { dx: 10.0, dy: 0.0 });

        let base = view.framing(0).expect("calque de base");
        assert!(proche(base.x.expect("x"), (-10.0, 90.0)));
        assert!(base.y.is_some());
        assert!(base.y2.is_some(), "l'ordonnée de droite suit le geste");
        // Le calque superposé n'a que son ordonnée propre : son abscisse est
        // liée à celle du calque recouvert.
        let haut = view.framing(1).expect("calque superposé");
        assert_eq!(haut.x, None);
        assert!(haut.y.is_some());
        assert_eq!(haut.y2, None);
        assert_eq!(view.framing(2), None, "une autre case ne bouge pas");
        // L'inverse : abscisse propre, ordonnée liée au calque recouvert.
        let propre = view.framing(3).expect("calque à abscisse propre");
        assert!(propre.x.is_some());
        assert_eq!((propre.y, propre.y2), (None, None));
    }

    #[test]
    fn un_geste_sur_un_panneau_inconnu_ne_fait_rien() {
        let mut view = ViewState::new();
        view.commit_gesture(&[panel(0, 0, 0)], 3, Gesture::Pan { dx: 1.0, dy: 1.0 });
        view.commit_axis_gesture(
            &[panel(0, 0, 0)],
            3,
            AxisSide::Bottom,
            Gesture::Pan { dx: 1.0, dy: 1.0 },
        );
        assert!(view.is_empty());
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn un_geste_sur_un_axe_ne_cadre_que_cette_echelle() {
        let mut base = panel(0, 0, 0);
        base.y2_range = Some((0.0, 1.0));
        let panels = [base];
        let glisser = Gesture::Pan { dx: 10.0, dy: 20.0 };

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Bottom, glisser);
        let bas = view.framing(0).expect("cadrage");
        assert!(proche(bas.x.expect("x"), (-10.0, 90.0)), "{bas:?}");
        assert_eq!((bas.y, bas.y2), (None, None));

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Left, glisser);
        let gauche = view.framing(0).expect("cadrage");
        assert!(proche(gauche.y.expect("y"), (20.0, 120.0)), "{gauche:?}");
        assert_eq!((gauche.x, gauche.y2), (None, None));

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Right, glisser);
        let droite = view.framing(0).expect("cadrage");
        assert!(proche(droite.y2.expect("y2"), (0.2, 1.2)), "{droite:?}");
        assert_eq!((droite.x, droite.y), (None, None));
    }

    #[test]
    fn un_zoom_sur_un_axe_se_centre_sur_le_curseur_de_cet_axe() {
        let panels = [panel(0, 0, 0)];
        let zoom = Gesture::Zoom {
            anchor: (20.0, 90.0),
            factor: 0.5,
        };
        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Bottom, zoom);
        let x = view.framing(0).and_then(|f| f.x).expect("x");
        assert!(proche(x, (10.0, 60.0)), "{x:?}");

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Left, zoom);
        // Le pixel 90 est la donnée 10 : (0-10)*0,5+10 .. (100-10)*0,5+10.
        let y = view.framing(0).and_then(|f| f.y).expect("y");
        assert!(proche(y, (5.0, 55.0)), "{y:?}");
    }

    #[test]
    fn un_cadrage_pose_par_un_geste_passe_dans_le_spec() {
        let mut view = ViewState::new();
        view.commit_gesture(&[panel(0, 0, 0)], 0, Gesture::Pan { dx: 10.0, dy: 0.0 });
        let mut spec = PlotSpec::empty("Vue").with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
        view.apply(&mut spec);
        let bas = &spec.panels[0].bottom;
        assert!(!bas.auto_range);
        assert_eq!((bas.range_min, bas.range_max), (Some(-10.0), Some(90.0)));
    }
}
