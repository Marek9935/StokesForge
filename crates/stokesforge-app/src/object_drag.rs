//! Les décisions pures du glisser-déposer des séries, des groupes et des
//! objets libres (Object Manager, fenêtre des séries).
//!
//! Même geste que la liste des formats de copie (`app/reorder_list.rs`) : la
//! ligne saisie par sa poignée **change de place tout de suite** quand le
//! pointeur en survole une autre, sans trait de repère ; au relâchement, la
//! liste est déjà dans son nouvel ordre. Ce module dit seulement *où* va
//! l'élément glissé selon la ligne survolée — l'interface lui passe l'élément
//! (`DragItem`) et le survol (`DropOver`), il rend le nouvel état.
//!
//! Règles, toutes dans [`step`] :
//! - **une série sur une série** : dans le même conteneur (le groupe, ou les
//!   séries seules), elle prend la place de la ligne survolée ; venant d'un
//!   autre conteneur, elle s'insère avant la ligne si le pointeur est dans sa
//!   moitié haute, après sinon. Survoler un membre d'un groupe y fait donc
//!   **entrer** la série ; survoler une série seule la rend seule (**sortie**) ;
//! - **une série sur l'en-tête d'un groupe** : moitié haute, elle se pose
//!   seule juste avant le groupe ; moitié basse, elle en devient le premier
//!   membre ;
//! - **une série sur la zone de fin** : seule, tout en bas de la liste (la
//!   seule sortie possible d'un groupe qui finit la liste) ;
//! - **un groupe** (saisi par son en-tête) : il prend la place de
//!   l'emplacement survolé, en bloc ;
//! - **un objet libre** (texte, forme) : il prend la place de l'objet survolé
//!   de **sa catégorie** ; un texte ne passe jamais parmi les formes.
//!
//! Un déplacement entre calques n'existe pas : il changerait les axes de
//! rattachement des données. Les axes ne se réordonnent pas.

use stokesforge_plot::{PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSlot};

use crate::object_manager::VisibilityTarget;

/// Ce que la poignée a saisi. `origin` est l'adresse au début du geste (celle
/// du spec réel), `current` celle de l'élément dans l'aperçu : la liste se
/// réordonne pendant le glisser, l'élément change donc de rang.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragItem {
    Series {
        panel: usize,
        origin: usize,
        current: usize,
    },
    Group {
        panel: usize,
        group: u32,
    },
    Text {
        panel: usize,
        origin: usize,
        current: usize,
    },
    Shape {
        panel: usize,
        origin: usize,
        current: usize,
    },
}

impl DragItem {
    /// Le calque dont l'élément ne sort pas.
    pub fn panel(self) -> usize {
        match self {
            Self::Series { panel, .. }
            | Self::Group { panel, .. }
            | Self::Text { panel, .. }
            | Self::Shape { panel, .. } => panel,
        }
    }

    /// L'élément saisi, tel que l'aperçu le montre : la cible à garder
    /// sélectionnée pendant le geste.
    pub fn target(self) -> VisibilityTarget {
        use stokesforge_plot::{AnnotationRef, ShapeRef};
        match self {
            Self::Series { panel, current, .. } => {
                VisibilityTarget::Series(SeriesRef::new(panel, current))
            }
            Self::Group { panel, group } => {
                VisibilityTarget::SeriesGroup(SeriesGroupRef::new(panel, group))
            }
            Self::Text { panel, current, .. } => VisibilityTarget::Annotation(AnnotationRef {
                panel,
                index: current,
            }),
            Self::Shape { panel, current, .. } => VisibilityTarget::Shape(ShapeRef {
                panel,
                index: current,
            }),
        }
    }
}

/// Ce que le pointeur survole pendant le glisser. `after` : le pointeur est
/// dans la moitié basse de la ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOver {
    Series {
        panel: usize,
        index: usize,
        after: bool,
    },
    Header {
        panel: usize,
        group: u32,
        after: bool,
    },
    /// La zone vide sous la dernière série du calque.
    End {
        panel: usize,
    },
    Text {
        panel: usize,
        index: usize,
    },
    Shape {
        panel: usize,
        index: usize,
    },
}

impl DropOver {
    fn panel(self) -> usize {
        match self {
            Self::Series { panel, .. }
            | Self::Header { panel, .. }
            | Self::End { panel }
            | Self::Text { panel, .. }
            | Self::Shape { panel, .. } => panel,
        }
    }
}

/// Le rang, parmi `slots`, de l'emplacement qui contient la série `index`.
fn slot_rank(slots: &[SeriesSlot], index: usize) -> Option<usize> {
    slots.iter().position(|slot| slot.contains(index))
}

/// Le rang de l'emplacement `rank` une fois la série `dragged` retirée : son
/// emplacement disparaît si elle y était seule, et décale ce qui le suit.
fn rank_without(slots: &[SeriesSlot], dragged: usize, rank: usize) -> usize {
    let vanishes = slots.iter().position(|slot| match slot {
        SeriesSlot::Single(i) => *i == dragged,
        SeriesSlot::Group { members, .. } => members.as_slice() == [dragged],
    });
    match vanishes {
        Some(gone) if gone < rank => rank - 1,
        _ => rank,
    }
}

/// Où va la série `dragged` quand le pointeur survole `over` : le conteneur
/// (`None` : seule) et le rang dans ce conteneur, compté série retirée.
fn series_destination(
    panel: &PanelSpec,
    dragged: usize,
    over: DropOver,
) -> Option<(Option<u32>, usize)> {
    let own = panel.series.get(dragged)?.group;
    let slots = panel.slots();
    match over {
        DropOver::Series { index, after, .. } => {
            if index == dragged {
                return None;
            }
            match panel.series.get(index)?.group {
                None => {
                    let rank = slot_rank(&slots, index)?;
                    Some(if own.is_none() {
                        (None, rank)
                    } else {
                        (
                            None,
                            rank_without(&slots, dragged, rank) + usize::from(after),
                        )
                    })
                }
                Some(group) => {
                    let place = panel
                        .group_members(group)
                        .iter()
                        .position(|member| *member == index)?;
                    Some(if own == Some(group) {
                        (Some(group), place)
                    } else {
                        (Some(group), place + usize::from(after))
                    })
                }
            }
        }
        DropOver::Header { group, after, .. } => {
            let rank = slots
                .iter()
                .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
            if !after {
                return Some((None, rank_without(&slots, dragged, rank)));
            }
            (own != Some(group)).then_some((Some(group), 0))
        }
        DropOver::End { .. } => Some((None, usize::MAX)),
        DropOver::Text { .. } | DropOver::Shape { .. } => None,
    }
}

/// Où va le groupe `group` quand le pointeur survole `over` : le rang parmi
/// les emplacements, groupe retiré.
fn group_destination(panel: &PanelSpec, group: u32, over: DropOver) -> Option<usize> {
    let slots = panel.slots();
    let own = slots
        .iter()
        .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
    let rank = match over {
        DropOver::Series { index, .. } => slot_rank(&slots, index)?,
        DropOver::Header { group: other, .. } => slots
            .iter()
            .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == other))?,
        DropOver::End { .. } => return Some(usize::MAX),
        DropOver::Text { .. } | DropOver::Shape { .. } => return None,
    };
    (rank != own).then_some(rank)
}

/// Remet l'élément `from` au rang `to` d'une liste d'objets libres.
fn shift<T>(list: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= list.len() || to >= list.len() || from == to {
        return false;
    }
    let item = list.remove(from);
    list.insert(to, item);
    true
}

/// Applique à `spec` ce que le survol `over` demande de l'élément glissé
/// `item`. Rend l'élément à sa nouvelle place, `None` si rien ne change (autre
/// calque, mauvaise catégorie, élément déjà là).
pub fn step(spec: &mut PlotSpec, item: DragItem, over: DropOver) -> Option<DragItem> {
    if item.panel() != over.panel() {
        return None;
    }
    let panel = spec.panels.get_mut(item.panel())?;
    step_panel(panel, item, over).map(|(item, _)| item)
}

/// [`step`] sur un seul calque (le brouillon de la fenêtre des séries), sans
/// contrôle de calque. Rend aussi l'ordre des séries obtenu
/// (`ordre[nouveau rang] = ancien rang`, vide pour un objet libre), qui permet
/// de suivre une série sélectionnée.
pub fn step_panel(
    panel: &mut PanelSpec,
    item: DragItem,
    over: DropOver,
) -> Option<(DragItem, Vec<usize>)> {
    match item {
        DragItem::Series {
            panel: p,
            origin,
            current,
        } => {
            let (group, position) = series_destination(panel, current, over)?;
            let order = panel.place_series(current, group, position)?;
            let current = order.iter().position(|old| *old == current)?;
            Some((
                DragItem::Series {
                    panel: p,
                    origin,
                    current,
                },
                order,
            ))
        }
        DragItem::Group { panel: p, group } => {
            let position = group_destination(panel, group, over)?;
            let order = panel.place_group(group, position)?;
            Some((DragItem::Group { panel: p, group }, order))
        }
        DragItem::Text {
            panel: p,
            origin,
            current,
        } => {
            let DropOver::Text { index, .. } = over else {
                return None;
            };
            shift(&mut panel.annotations, current, index).then_some((
                DragItem::Text {
                    panel: p,
                    origin,
                    current: index,
                },
                Vec::new(),
            ))
        }
        DragItem::Shape {
            panel: p,
            origin,
            current,
        } => {
            let DropOver::Shape { index, .. } = over else {
                return None;
            };
            shift(&mut panel.shapes, current, index).then_some((
                DragItem::Shape {
                    panel: p,
                    origin,
                    current: index,
                },
                Vec::new(),
            ))
        }
    }
}

/// Place l'objet libre `target` (texte ou forme) au rang `index` de sa
/// catégorie, dans son calque. Faux si la cible n'est pas un objet libre,
/// n'existe pas ou ne bouge pas.
pub fn reorder_object(spec: &mut PlotSpec, target: VisibilityTarget, index: usize) -> bool {
    match target {
        VisibilityTarget::Annotation(t) => spec
            .panels
            .get_mut(t.panel)
            .is_some_and(|panel| shift(&mut panel.annotations, t.index, index)),
        VisibilityTarget::Shape(t) => spec
            .panels
            .get_mut(t.panel)
            .is_some_and(|panel| shift(&mut panel.shapes, t.index, index)),
        _ => false,
    }
}

/// Ce que le relâchement demande au modèle, déduit de l'aperçu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Series {
        source: SeriesRef,
        group: Option<u32>,
        position: usize,
    },
    Group {
        group: SeriesGroupRef,
        position: usize,
    },
    Object {
        target: VisibilityTarget,
        index: usize,
    },
}

/// L'action unique du relâchement : `preview` est le spec tel que le geste l'a
/// laissé, `item` l'élément à sa place finale. `None` si l'aperçu n'a rien
/// changé par rapport à `base`.
pub fn finish(base: &PlotSpec, preview: &PlotSpec, item: DragItem) -> Option<Drop> {
    if preview == base {
        return None;
    }
    match item {
        DragItem::Series {
            panel,
            origin,
            current,
        } => {
            let (group, position) = preview.panels.get(panel)?.series_position(current)?;
            Some(Drop::Series {
                source: SeriesRef::new(panel, origin),
                group,
                position,
            })
        }
        DragItem::Group { panel, group } => {
            let slots = preview.panels.get(panel)?.slots();
            let position = slots
                .iter()
                .position(|slot| matches!(slot, SeriesSlot::Group { id, .. } if *id == group))?;
            Some(Drop::Group {
                group: SeriesGroupRef::new(panel, group),
                position,
            })
        }
        DragItem::Text {
            panel,
            origin,
            current,
        } => Some(Drop::Object {
            target: VisibilityTarget::Annotation(stokesforge_plot::AnnotationRef {
                panel,
                index: origin,
            }),
            index: current,
        }),
        DragItem::Shape {
            panel,
            origin,
            current,
        } => Some(Drop::Object {
            target: VisibilityTarget::Shape(stokesforge_plot::ShapeRef {
                panel,
                index: origin,
            }),
            index: current,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::NodeId;
    use stokesforge_plot::{Panels, SeriesSpec};

    /// A, [B C], D, [E F] — colonnes 1 à 6.
    fn spec() -> PlotSpec {
        let mut panel = PanelSpec::at(0, 0);
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(1))]);
        panel.push_series_batch(vec![
            SeriesSpec::new(NodeId::from_raw(2)),
            SeriesSpec::new(NodeId::from_raw(3)),
        ]);
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(4))]);
        panel.push_series_batch(vec![
            SeriesSpec::new(NodeId::from_raw(5)),
            SeriesSpec::new(NodeId::from_raw(6)),
        ]);
        PlotSpec::empty("Test").with_panels(Panels::new(vec![panel]))
    }

    fn colonnes(spec: &PlotSpec) -> Vec<u64> {
        spec.panels[0].series.iter().map(|s| s.y.raw()).collect()
    }

    fn serie(index: usize) -> DragItem {
        DragItem::Series {
            panel: 0,
            origin: index,
            current: index,
        }
    }

    fn sur(index: usize, after: bool) -> DropOver {
        DropOver::Series {
            panel: 0,
            index,
            after,
        }
    }

    #[test]
    fn deux_series_seules_s_echangent_en_se_touchant() {
        let mut spec = spec();
        // A survole D : A prend le rang de D parmi les emplacements.
        let item = step(&mut spec, serie(0), sur(3, false)).unwrap();
        assert_eq!(colonnes(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(
            item,
            DragItem::Series {
                panel: 0,
                origin: 0,
                current: 3
            }
        );
        // Puis elle remonte en survolant D (désormais au-dessus).
        let item = step(&mut spec, item, sur(2, false)).unwrap();
        assert_eq!(colonnes(&spec), [2, 3, 1, 4, 5, 6]);
        assert_eq!(
            item.target(),
            VisibilityTarget::Series(SeriesRef::new(0, 2))
        );
    }

    #[test]
    fn survoler_un_membre_fait_entrer_dans_le_groupe_selon_la_moitie() {
        // D sur B, moitié haute : D entre avant B.
        let mut haut = spec();
        step(&mut haut, serie(3), sur(1, false)).unwrap();
        assert_eq!(colonnes(&haut), [1, 4, 2, 3, 5, 6]);
        assert_eq!(haut.panels[0].series[1].group, Some(1));
        // D sur C, moitié basse : D entre après C.
        let mut bas = spec();
        step(&mut bas, serie(3), sur(2, true)).unwrap();
        assert_eq!(colonnes(&bas), [1, 2, 3, 4, 5, 6]);
        assert_eq!(bas.panels[0].series[3].group, Some(1));
    }

    #[test]
    fn survoler_une_serie_seule_fait_sortir_du_groupe() {
        let mut spec = spec();
        // B (groupe 1) sur A, moitié haute : B passe seule en tête.
        step(&mut spec, serie(1), sur(0, false)).unwrap();
        assert_eq!(colonnes(&spec), [2, 1, 3, 4, 5, 6]);
        assert_eq!(spec.panels[0].series[0].group, None);
        assert_eq!(spec.panels[0].group_members(1), [2]);
    }

    #[test]
    fn l_entete_d_un_groupe_se_partage_en_deux_moities() {
        let en_tete = |after| DropOver::Header {
            panel: 0,
            group: 2,
            after,
        };
        // Moitié haute : D se pose seule juste avant le groupe 2 (il y est déjà).
        assert!(step(&mut spec(), serie(3), en_tete(false)).is_none());
        // A, moitié haute : A passe juste avant le groupe 2.
        let mut spec = spec();
        step(&mut spec, serie(0), en_tete(false)).unwrap();
        assert_eq!(colonnes(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(spec.panels[0].series[3].group, None);
        // Moitié basse : A devient le premier membre.
        let mut spec = self::spec();
        step(&mut spec, serie(0), en_tete(true)).unwrap();
        assert_eq!(colonnes(&spec), [2, 3, 4, 1, 5, 6]);
        assert_eq!(spec.panels[0].group_members(2), [3, 4, 5]);
    }

    #[test]
    fn la_zone_de_fin_est_la_sortie_d_un_groupe_qui_finit_la_liste() {
        let mut spec = spec();
        // F quitte le groupe 2, qui finit la liste.
        let item = step(&mut spec, serie(5), DropOver::End { panel: 0 }).unwrap();
        assert_eq!(colonnes(&spec), [1, 2, 3, 4, 5, 6]);
        assert_eq!(spec.panels[0].series[5].group, None);
        assert_eq!(
            item,
            DragItem::Series {
                panel: 0,
                origin: 5,
                current: 5
            }
        );
        // Déjà seule en fin de liste : rien ne bouge.
        assert!(step(&mut spec, item, DropOver::End { panel: 0 }).is_none());
    }

    #[test]
    fn un_groupe_se_deplace_en_bloc_sur_l_emplacement_survole() {
        let mut spec = spec();
        let groupe = DragItem::Group { panel: 0, group: 2 };
        step(&mut spec, groupe, sur(0, false)).unwrap();
        assert_eq!(colonnes(&spec), [5, 6, 1, 2, 3, 4]);
        // Survoler un de ses propres membres ne fait rien.
        assert!(step(&mut spec, groupe, sur(0, true)).is_none());
        // La zone de fin le renvoie en bas.
        step(&mut spec, groupe, DropOver::End { panel: 0 }).unwrap();
        assert_eq!(colonnes(&spec), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn jamais_d_un_calque_a_l_autre_ni_d_une_categorie_a_l_autre() {
        let mut spec = spec();
        let ailleurs = DropOver::Series {
            panel: 1,
            index: 0,
            after: false,
        };
        assert!(step(&mut spec, serie(0), ailleurs).is_none());
        assert!(step(&mut spec, serie(0), DropOver::Text { panel: 0, index: 0 }).is_none());
        let texte = DragItem::Text {
            panel: 0,
            origin: 0,
            current: 0,
        };
        assert!(step(&mut spec, texte, DropOver::Shape { panel: 0, index: 0 }).is_none());
        assert!(step(&mut spec, texte, sur(1, false)).is_none());
    }

    #[test]
    fn les_objets_libres_gardent_leur_categorie() {
        use stokesforge_plot::{AnnotationSpec, ShapeKind, ShapeSpec};
        let mut spec = spec();
        let panel = spec.panels.get_mut(0).unwrap();
        for texte in ["a", "b", "c"] {
            panel.annotations.push(AnnotationSpec::new(texte, 0.0, 0.0));
        }
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Line, 0.0, 0.0, 1.0, 1.0));
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Arrow, 0.0, 0.0, 1.0, 1.0));
        let texte = DragItem::Text {
            panel: 0,
            origin: 0,
            current: 0,
        };
        let item = step(&mut spec, texte, DropOver::Text { panel: 0, index: 2 }).unwrap();
        let ordre: Vec<_> = spec.panels[0]
            .annotations
            .iter()
            .map(|a| a.text.as_str())
            .collect();
        assert_eq!(ordre, ["b", "c", "a"]);
        assert_eq!(spec.panels[0].shapes.len(), 2, "les formes n'ont pas bougé");
        let drop = finish(&self::spec_with(&spec), &spec, item);
        assert!(
            matches!(drop, Some(Drop::Object { index: 2, .. })),
            "{drop:?}"
        );
    }

    /// Le spec d'avant le geste : les objets libres de `spec` dans leur ordre
    /// d'origine (a, b, c).
    fn spec_with(apres: &PlotSpec) -> PlotSpec {
        let mut avant = apres.clone();
        avant.panels.get_mut(0).unwrap().annotations.rotate_right(1);
        avant
    }

    #[test]
    fn le_relachement_se_resume_en_une_destination() {
        let base = spec();
        let mut preview = base.clone();
        let item = step(&mut preview, serie(3), sur(1, false)).unwrap();
        let drop = finish(&base, &preview, item).unwrap();
        assert_eq!(
            drop,
            Drop::Series {
                source: SeriesRef::new(0, 3),
                group: Some(1),
                position: 0
            }
        );
        // Rejouée sur le spec réel, la destination redonne l'aperçu.
        let mut rejoue = base.clone();
        let (group, position) = match drop {
            Drop::Series {
                group, position, ..
            } => (group, position),
            _ => unreachable!(),
        };
        rejoue
            .panels
            .get_mut(0)
            .unwrap()
            .place_series(3, group, position)
            .unwrap();
        assert_eq!(rejoue, preview);
        // Sans changement, rien à demander.
        assert!(finish(&base, &base, item).is_none());
    }

    #[test]
    fn reorder_object_refuse_ce_qui_n_est_pas_un_objet_libre() {
        let mut spec = spec();
        assert!(!reorder_object(
            &mut spec,
            VisibilityTarget::Series(SeriesRef::new(0, 0)),
            1
        ));
        assert!(!reorder_object(&mut spec, VisibilityTarget::Panel(0), 0));
    }
}
