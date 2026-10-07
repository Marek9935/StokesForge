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
//! [`stokesforge_core::Tree`] : il vit à côté, dans `stokesforge-render-egui`, sur
//! [`crate::GraphView`].
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
//! est nouveau, dans [`crate::GraphView`].
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
//! qui ne change que la tessellation (`crate::feathering`, et
//! `architecture-choisie.md` §4.2). Il **doit** donc entrer dans la clé de
//! cache. [`ViewState::generation`] est ce qui y entre : un entier qui monte
//! à chaque cadrage commis, plutôt que les bornes elles-mêmes (variables en
//! nombre, par panneau, en `f64` — rien de bien `Copy`).

use std::collections::HashMap;

use stokesforge_plot::PlotSpec;

/// Un cadrage explicite pour un panneau : les bornes que le zoom/pan ont
/// calculées, en espace de données (pas transformé — comme
/// [`stokesforge_plot::AxisSpec::range_min`]/`range_max`, dont c'est directement
/// la valeur).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    /// Bornes de l'axe des abscisses. L'ordre n'importe pas : la résolution
    /// les trie de toute façon (`pipeline::axis_range`).
    pub x: (f64, f64),
    /// Bornes de l'ordonnée principale.
    pub y: (f64, f64),
}

/// L'état de vue d'un [`crate::GraphView`] : les cadrages explicites en
/// cours, par panneau.
///
/// Vide par défaut — un graphe neuf s'affiche cadré sur ses données, comme
/// avant R7. Limité à l'axe principal (bas et gauche) : la désignation de
/// point (tâche 7.2) l'est aussi, pour la même raison — voir sa
/// documentation dans `crate::lib`.
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

    /// Le compteur à poser dans [`crate::cache::CacheKey::view_generation`].
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

    /// Pose (ou remplace) le cadrage explicite d'un panneau, et fait avancer
    /// la génération — donc invalide le cache.
    pub fn set_framing(&mut self, panel: usize, framing: Framing) {
        self.framings.insert(panel, framing);
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
            panel.bottom.auto_range = false;
            panel.bottom.range_min = Some(framing.x.0);
            panel.bottom.range_max = Some(framing.x.1);
            panel.left.auto_range = false;
            panel.left.range_min = Some(framing.y.0);
            panel.left.range_max = Some(framing.y.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PanelSpec, Panels};

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
        view.set_framing(
            0,
            Framing {
                x: (1.0, 2.0),
                y: (3.0, 4.0),
            },
        );
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
        view.set_framing(
            0,
            Framing {
                x: (0.0, 1.0),
                y: (0.0, 1.0),
            },
        );
        assert_eq!(view.generation(), 1);
        view.set_framing(
            0,
            Framing {
                x: (0.0, 2.0),
                y: (0.0, 1.0),
            },
        );
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
        view.set_framing(
            0,
            Framing {
                x: (0.0, 1.0),
                y: (0.0, 1.0),
            },
        );
        view.reset();
        assert!(view.is_empty());
        assert_eq!(view.generation(), 2);
        let mut spec = spec_a_deux_panneaux();
        let avant = spec.clone();
        view.apply(&mut spec);
        assert_eq!(spec, avant, "plus rien à appliquer après un reset");
    }
}
