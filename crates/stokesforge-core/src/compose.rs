//! Composition d'un graphe à partir des colonnes d'un classeur (#257) : les
//! règles qui lisent le [`Tree`] et fabriquent des [`SeriesSpec`] ou lisent un
//! [`PlotSpec`], sans interface ni phrase.
//!
//! Elles vivaient dans `stokesforge-app/src/state.rs`, d'où `pyst` et le CLI
//! n'y voyaient rien et en refaisaient une partie à la main (la palette, la
//! recherche de l'abscisse). Elles sont ici pour que **l'interface, `pyst` et
//! les analyses appliquent la même règle** :
//!
//! - **appariement**, façon OriginLab : chaque colonne Y est appariée à la
//!   colonne X la plus proche à sa **gauche** dans la feuille (rôle
//!   [`ColumnRole::X`]), en incluant les colonnes X non sélectionnées ; un Yn
//!   (Y2, Y3, Y4) suit la même règle avec les colonnes Xn ([`x_for`],
//!   [`series_from_columns`]). Sans X à gauche, l'abscisse reste implicite
//!   (numéro de ligne) ;
//! - **palette** : la couleur d'une série est celle de son rang
//!   ([`SeriesSpec::from_palette`]), jamais une écriture à la main ;
//! - **calques** : le calque ordinaire que recouvre un calque superposé
//!   ([`overlay_base`]) et le calque de rôle Xn/Yn d'un calque de base
//!   ([`role_layer_of`]) ;
//! - **lecture du spec** : [`current_plot_spec`], [`panel_plot_type`],
//!   [`spec_has_x`].
//!
//! Ce module ne mute rien : il construit des valeurs que l'appelant envoie par
//! une commande annulable (invariant 1). Il ne produit aucun texte :
//! l'interface passe le nom traduit des séries « ligne » en paramètre
//! ([`row_series`]).

use stokesforge_id::NodeId;
use stokesforge_plot::{OverlaySpec, PanelSpec, PlotSpec, PlotType, SeriesSpec};

use crate::column::ColumnRole;
use crate::node::{Node, NodeKind};
use crate::tree::Tree;

/// Les séries à tracer pour une sélection de colonnes, réparties par calque
/// (#167, #168) : le calque principal et les calques superposés des rôles
/// X2/Y2, X3/Y3 et X4/Y4.
#[derive(Debug, Default)]
pub struct PlannedSeries {
    /// Séries de rôle Y, sur les axes principaux.
    pub primary: Vec<SeriesSpec>,
    /// Séries de rôle Y2, Y3 et Y4 (dans cet ordre) : chacune sur le calque
    /// superposé de son numéro (axe X en haut, axe Y à droite).
    pub overlays: [Vec<SeriesSpec>; OVERLAY_ROLES],
}

/// Nombre de calques superposés qu'alimentent les rôles Xn/Yn (X2…X4).
pub const OVERLAY_ROLES: usize =
    (OverlaySpec::LAST_ROLE_LAYER - OverlaySpec::FIRST_ROLE_LAYER + 1) as usize;

impl PlannedSeries {
    /// Aucune série, ni principale ni superposée.
    pub fn is_empty(&self) -> bool {
        self.primary.is_empty() && self.overlays.iter().all(Vec::is_empty)
    }

    /// Les séries de tous les calques superposés, du calque 2 au calque 4.
    pub fn overlay_series(&self) -> impl Iterator<Item = &SeriesSpec> {
        self.overlays.iter().flatten()
    }
}

/// Les colonnes de `sheet`, dans l'ordre de la feuille (vide si `sheet` n'est
/// pas un nœud de l'arbre).
fn sheet_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    tree.node(sheet)
        .map(Node::children)
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|id| {
            tree.node(*id)
                .is_some_and(|node| node.as_column().is_some())
        })
        .collect()
}

fn role_of(tree: &Tree, id: NodeId) -> Option<ColumnRole> {
    tree.node(id)
        .and_then(Node::as_column)
        .map(|column| column.meta().role)
}

/// La colonne de rôle `x_role` la plus proche **à gauche** de la colonne de
/// rang `position` dans `columns`.
fn nearest_left(
    tree: &Tree,
    columns: &[NodeId],
    position: usize,
    x_role: ColumnRole,
) -> Option<NodeId> {
    columns[..position]
        .iter()
        .rev()
        .copied()
        .find(|id| role_of(tree, *id) == Some(x_role))
}

/// La colonne X appariée à la colonne `y` : le X (Xn si `y` est de rôle Yn) le
/// plus proche à sa gauche dans sa feuille, `None` sans X à gauche ou si `y`
/// n'est pas une colonne d'une feuille. C'est la règle de
/// [`series_from_columns`], pour qui ne veut que l'abscisse (annexe d'une
/// analyse, `pyst.plot(y)` sans `x=`).
pub fn x_for(tree: &Tree, y: NodeId) -> Option<NodeId> {
    let sheet = tree.node(y).and_then(Node::parent)?;
    let columns = sheet_columns(tree, sheet);
    let position = columns.iter().position(|id| *id == y)?;
    let x_role = role_of(tree, y)?.paired_x().unwrap_or(ColumnRole::X);
    nearest_left(tree, &columns, position, x_role)
}

/// Construit les séries de la sélection `columns` de `sheet` : les colonnes de
/// rôle Y (ou Y2, Y3, Y4) présentes dans `columns`, dans l'ordre où elles apparaissent
/// dans la feuille, chacune appariée à son X ([`x_for`]) — y compris un X non
/// sélectionné. Les Yn vont dans [`PlannedSeries::overlays`].
///
/// Chaque série prend la couleur de palette de son rang, `start_index` inclus
/// (`overlay_starts` pour les calques superposés) : décalage utile à l'ajout
/// de séries à un graphe, qui prolonge la palette après les séries déjà
/// présentes plutôt que de la reprendre à zéro. Rend des listes vides si
/// `columns` ne contient aucun Y ni Y2 — c'est ce que les appelants lisent
/// pour refuser un graphe ou un ajout vide.
pub fn series_from_columns(
    tree: &Tree,
    sheet: NodeId,
    columns: &[NodeId],
    plot_type: &PlotType,
    start_index: usize,
    overlay_starts: [usize; OVERLAY_ROLES],
) -> PlannedSeries {
    let ordered = sheet_columns(tree, sheet);
    let mut planned = PlannedSeries::default();
    for (position, column) in ordered.iter().enumerate() {
        if !columns.contains(column) {
            continue;
        }
        let Some(role) = role_of(tree, *column) else {
            continue;
        };
        let (list, start) = match role {
            ColumnRole::Y => (&mut planned.primary, start_index),
            ColumnRole::Y2 | ColumnRole::Y3 | ColumnRole::Y4 => {
                let slot = role.layer().map_or(0, |layer| {
                    usize::from(layer - OverlaySpec::FIRST_ROLE_LAYER)
                });
                (&mut planned.overlays[slot], overlay_starts[slot])
            }
            _ => continue,
        };
        let x_role = role.paired_x().unwrap_or(ColumnRole::X);
        let mut spec = SeriesSpec::from_palette(*column, start + list.len());
        spec.x = nearest_left(tree, &ordered, position, x_role);
        spec.plot_type = plot_type.clone();
        list.push(spec);
    }
    planned
}

/// Les séries « ligne » (#166) de `rows`, ancrées sur `anchor`, avec les
/// couleurs de palette à partir du rang `start`. `name` donne le nom de la
/// série d'une ligne (0 = la première) : une phrase d'interface, que ce crate
/// ne sait pas écrire.
pub fn row_series(
    anchor: NodeId,
    rows: &[usize],
    start: usize,
    plot_type: &PlotType,
    name: impl Fn(usize) -> String,
) -> Vec<SeriesSpec> {
    rows.iter()
        .enumerate()
        .map(|(offset, &row)| {
            let mut series = SeriesSpec::from_palette(anchor, start + offset);
            series.row = Some(row);
            series.name = name(row);
            series.plot_type = plot_type.clone();
            series
        })
        .collect()
}

/// Les colonnes de `sheet` qui portent une valeur x' finie, dans l'ordre de la
/// feuille (#164, #166).
pub fn x_prime_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    sheet_columns(tree, sheet)
        .into_iter()
        .filter(|id| {
            tree.node(*id)
                .and_then(Node::as_column)
                .is_some_and(|column| column.meta().x_prime.is_some_and(f64::is_finite))
        })
        .collect()
}

/// Le graphe a-t-il déjà un X : une série dont l'abscisse vient d'une colonne
/// de rôle X, ou une série « ligne » (abscisse = x') ? Décide si le dépôt de
/// lignes pose la question de l'axe (#166).
pub fn spec_has_x(spec: &PlotSpec) -> bool {
    spec.panels
        .iter()
        .flat_map(|panel| &panel.series)
        .any(|series| series.x.is_some() || series.row.is_some())
}

/// Le calque ordinaire que `panel` recouvre (lui-même s'il est ordinaire) :
/// le dernier calque ordinaire qui le précède dans sa case.
pub fn overlay_base(spec: &PlotSpec, panel: usize) -> usize {
    let Some(target) = spec.panels.get(panel) else {
        return panel;
    };
    if target.overlay.is_none() {
        return panel;
    }
    (0..panel)
        .rev()
        .find(|index| {
            spec.panels.get(*index).is_some_and(|candidate| {
                candidate.overlay.is_none()
                    && (candidate.row, candidate.column) == (target.row, target.column)
            })
        })
        .unwrap_or(panel)
}

/// L'indice du calque de rôle de numéro `layer` (2 à 4 : axe X en haut, axe Y
/// à droite, [`OverlaySpec::role_layer`]) posé sur le calque ordinaire `base`,
/// s'il existe.
pub fn role_layer_of(spec: &PlotSpec, base: usize, layer: u8) -> Option<usize> {
    let cell = spec
        .panels
        .get(base)
        .map(|panel| (panel.row, panel.column))?;
    (base + 1..spec.panels.len()).find(|index| {
        spec.panels.get(*index).is_some_and(|panel| {
            panel.overlay == Some(OverlaySpec::role_layer(layer))
                && (panel.row, panel.column) == cell
        })
    })
}

/// Nombre maximal de calques superposés d'une case que « New Layer » laisse
/// poser : autant que de rôles Xn/Yn (#168), les calques « ligne » compris.
pub const MAX_OVERLAYS: usize = OVERLAY_ROLES;

/// Les calques superposés de la case du calque `panel`.
fn overlays_in_cell(spec: &PlotSpec, panel: usize) -> impl Iterator<Item = &PanelSpec> {
    let cell = spec
        .panels
        .get(panel)
        .map(|target| (target.row, target.column));
    spec.panels.iter().filter(move |candidate| {
        candidate.overlay.is_some() && Some((candidate.row, candidate.column)) == cell
    })
}

/// La case du calque `panel` porte-t-elle déjà [`MAX_OVERLAYS`] calques
/// superposés ? Alors « New Layer » est grisé.
pub fn overlays_full(spec: &PlotSpec, panel: usize) -> bool {
    overlays_in_cell(spec, panel).count() >= MAX_OVERLAYS
}

/// Le plus petit numéro de calque de rôle (2 à 4) qu'aucun calque de la case
/// du calque `panel` ne porte : celui d'un nouveau calque à deux axes (#168).
/// `None` si les trois sont pris.
pub fn free_role_layer(spec: &PlotSpec, panel: usize) -> Option<u8> {
    (OverlaySpec::FIRST_ROLE_LAYER..=OverlaySpec::LAST_ROLE_LAYER).find(|layer| {
        !overlays_in_cell(spec, panel).any(|candidate| {
            candidate
                .overlay
                .is_some_and(|overlay| overlay.layer == *layer)
        })
    })
}

/// Le [`PlotSpec`] courant de `graph`, ou `None` si ce n'est pas un graphe de
/// ce projet.
pub fn current_plot_spec(tree: &Tree, graph: NodeId) -> Option<PlotSpec> {
    match tree.node(graph)?.kind() {
        NodeKind::Graph { spec, .. } => Some(spec.clone()),
        _ => None,
    }
}

/// Type de tracé des séries ajoutées à `panel` quand le geste n'en désigne
/// pas : celui de sa **dernière** série (un calque qui en mélange plusieurs
/// donne le dernier), [`PlotType::Line`] pour un calque vide.
pub fn panel_plot_type(panel: &PanelSpec) -> PlotType {
    panel
        .series
        .last()
        .map_or(PlotType::Line, |series| series.plot_type.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, ColumnMeta, Project};
    use stokesforge_plot::{palette_color, Panels, DEFAULT_PALETTE};

    /// Un classeur dont les colonnes, dans l'ordre, portent les rôles donnés
    /// (nommées `c0`, `c1`…).
    fn sheet_with(roles: &[ColumnRole]) -> (Project, NodeId, Vec<NodeId>) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let columns: Vec<NodeId> = roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                let id = project
                    .add_column(
                        sheet,
                        None,
                        Some(&format!("c{i}")),
                        Column::from_f64(&[1.0, 2.0, 3.0]),
                    )
                    .unwrap();
                project
                    .set_column_meta(
                        id,
                        ColumnMeta {
                            role: *role,
                            ..ColumnMeta::default()
                        },
                    )
                    .unwrap();
                id
            })
            .collect();
        (project, sheet, columns)
    }

    use ColumnRole::{None as NoRole, X, X2, X3, X4, Y, Y2, Y3, Y4};

    #[test]
    fn chaque_y_prend_le_x_le_plus_proche_a_sa_gauche_meme_non_selectionne() {
        // X0 Y1 X2 Y3 Y4 : Y1 -> X0, Y3 et Y4 -> X2 (le plus proche à gauche).
        let (project, sheet, c) = sheet_with(&[X, Y, X, Y, Y]);
        let tree = project.tree();
        // Seuls les Y sont sélectionnés : les X ne le sont pas, ils servent quand même.
        let planned = series_from_columns(
            tree,
            sheet,
            &[c[1], c[3], c[4]],
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        let pairs: Vec<(Option<NodeId>, NodeId)> =
            planned.primary.iter().map(|s| (s.x, s.y)).collect();
        assert_eq!(
            pairs,
            [(Some(c[0]), c[1]), (Some(c[2]), c[3]), (Some(c[2]), c[4])]
        );
        assert!(planned.overlays.iter().all(Vec::is_empty));
        // Le témoin : un appariement « à droite » donnerait un autre X.
        assert_ne!(planned.primary[0].x, Some(c[2]));
    }

    #[test]
    fn un_y2_s_apparie_a_x2_et_va_au_second_calque() {
        // X0 Y1 X2_2 Y2_3 : Y2 ignore X0 et prend X2.
        let (project, sheet, c) = sheet_with(&[X, Y, X2, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[3]],
            &PlotType::Scatter,
            0,
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.primary.len(), 1);
        assert_eq!(planned.primary[0].x, Some(c[0]));
        assert_eq!(planned.overlays[0].len(), 1);
        assert_eq!(planned.overlays[0][0].x, Some(c[2]));
        assert_eq!(planned.overlays[0][0].y, c[3]);
        assert!(planned
            .primary
            .iter()
            .chain(&planned.overlays[0])
            .all(|s| s.plot_type == PlotType::Scatter));
    }

    /// #168 : Y3 et Y4 s'apparient à X3 et X4 (jamais à X ni à X2) et vont à
    /// leur propre calque ; un Y3 sans Y2 ne remplit pas le calque 2.
    #[test]
    fn y3_et_y4_s_apparient_a_leur_x_et_vont_a_leur_calque() {
        let (project, sheet, c) = sheet_with(&[X, Y, X2, Y2, X3, Y3, X4, Y4]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        let paires = |i: usize| -> Vec<(Option<NodeId>, NodeId)> {
            planned.overlays[i].iter().map(|s| (s.x, s.y)).collect()
        };
        assert_eq!(paires(0), [(Some(c[2]), c[3])]);
        assert_eq!(paires(1), [(Some(c[4]), c[5])]);
        assert_eq!(paires(2), [(Some(c[6]), c[7])]);
        assert_eq!(x_for(project.tree(), c[7]), Some(c[6]));

        let (project, sheet, c) = sheet_with(&[X, Y, X3, Y3]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[3]],
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert!(planned.overlays[0].is_empty(), "pas de Y2 : calque 2 vide");
        assert_eq!(planned.overlays[1].len(), 1);
        assert_eq!(planned.overlay_series().count(), 1);
    }

    #[test]
    fn sans_x_a_gauche_l_abscisse_reste_implicite_et_sans_y_rien_n_est_planifie() {
        // Y0 X1 Y2 : Y0 n'a pas de X à sa gauche, Y2 a X1.
        let (project, sheet, c) = sheet_with(&[Y, X, Y]);
        let tree = project.tree();
        let planned = series_from_columns(tree, sheet, &c, &PlotType::Line, 0, [0; OVERLAY_ROLES]);
        assert_eq!(planned.primary[0].x, None);
        assert_eq!(planned.primary[1].x, Some(c[1]));
        // Un Y2 sans X2 à gauche reste implicite aussi, même avec un X.
        let (project, sheet, c) = sheet_with(&[X, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.overlays[0][0].x, None);
        // Aucune colonne Y ni Y2 sélectionnée : vide.
        let (project, sheet, c) = sheet_with(&[X, NoRole]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert!(planned.is_empty());
    }

    #[test]
    fn x_for_suit_la_meme_regle_que_series_from_columns() {
        let (project, _sheet, c) = sheet_with(&[Y, X, Y, X2, Y2, Y]);
        let tree = project.tree();
        assert_eq!(x_for(tree, c[0]), None, "pas de X à gauche");
        assert_eq!(x_for(tree, c[2]), Some(c[1]));
        assert_eq!(x_for(tree, c[4]), Some(c[3]), "un Y2 prend X2");
        assert_eq!(x_for(tree, c[5]), Some(c[1]), "un Y ignore X2");
        // Pas une colonne : rien.
        assert_eq!(x_for(tree, tree.root()), None);
    }

    #[test]
    fn la_palette_suit_le_rang_avec_le_decalage_de_chaque_calque() {
        let (project, sheet, c) = sheet_with(&[X, Y, Y, X2, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[2], c[4]],
            &PlotType::Line,
            3,
            [7, 0, 0],
        );
        let colors: Vec<&str> = planned
            .primary
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(colors, [palette_color(3), palette_color(4)]);
        assert_eq!(planned.overlays[0][0].line.color, palette_color(7));
        // Trait et marqueurs ensemble.
        assert!(planned
            .primary
            .iter()
            .chain(&planned.overlays[0])
            .all(|s| s.symbol.color == s.line.color));
        // Le rang boucle sur la palette.
        let (project, sheet, c) = sheet_with(&[Y]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            DEFAULT_PALETTE.len(),
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.primary[0].line.color, DEFAULT_PALETTE[0]);
    }

    #[test]
    fn les_series_ligne_prennent_la_palette_du_rang_et_le_nom_donne() {
        let (_project, _sheet, c) = sheet_with(&[Y]);
        let series = row_series(c[0], &[4, 9], 2, &PlotType::Scatter, |row| {
            format!("ligne {}", row + 1)
        });
        assert_eq!(series.len(), 2);
        assert_eq!((series[0].row, series[1].row), (Some(4), Some(9)));
        assert_eq!(
            (series[0].name.as_str(), series[1].name.as_str()),
            ("ligne 5", "ligne 10")
        );
        assert_eq!(series[0].line.color, palette_color(2));
        assert_eq!(series[1].line.color, palette_color(3));
        assert_eq!(series[1].symbol.color, palette_color(3));
        assert!(series.iter().all(|s| s.y == c[0] && s.x.is_none()));
        assert!(series.iter().all(|s| s.plot_type == PlotType::Scatter));
    }

    #[test]
    fn les_colonnes_x_prime_sont_celles_dont_la_valeur_est_finie() {
        let (mut project, sheet, c) = sheet_with(&[Y, Y, Y, Y]);
        for (id, x_prime) in [
            (c[0], None),
            (c[1], Some(1.5)),
            (c[2], Some(f64::NAN)),
            (c[3], Some(0.0)),
        ] {
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        x_prime,
                        ..ColumnMeta::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(x_prime_columns(project.tree(), sheet), [c[1], c[3]]);
    }

    /// Un spec à trois calques : ordinaire, X2/Y2 superposé, ordinaire en case (0, 1).
    fn spec_a_calques() -> PlotSpec {
        let base = PanelSpec::default();
        let second = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
        let rows = PanelSpec::overlay_on(&base, OverlaySpec::TOP_X);
        let ailleurs = PanelSpec::at(0, 1);
        PlotSpec {
            panels: Panels::new(vec![base, rows, second, ailleurs]),
            ..PlotSpec::default()
        }
    }

    #[test]
    fn un_calque_superpose_recouvre_le_dernier_calque_ordinaire_de_sa_case() {
        let spec = spec_a_calques();
        assert_eq!(overlay_base(&spec, 0), 0, "ordinaire : lui-même");
        assert_eq!(overlay_base(&spec, 1), 0);
        assert_eq!(overlay_base(&spec, 2), 0);
        assert_eq!(overlay_base(&spec, 3), 3);
        assert_eq!(overlay_base(&spec, 9), 9, "inexistant : inchangé");
    }

    #[test]
    fn le_second_calque_est_celui_des_deux_axes_pas_celui_des_lignes() {
        let spec = spec_a_calques();
        // Le calque 1 (X en haut seul) n'est pas le second calque : c'est le 2.
        assert_eq!(role_layer_of(&spec, 0, 2), Some(2));
        assert_eq!(role_layer_of(&spec, 0, 3), None, "pas de calque 3");
        assert_eq!(role_layer_of(&spec, 3, 2), None, "autre case");
        assert_eq!(role_layer_of(&spec, 9, 2), None);
    }

    /// #168 : un calque de rôle se retrouve par son **numéro**, pas par sa
    /// position : le calque 3 est celui du Y3 même sans calque 2 avant lui.
    #[test]
    fn un_calque_de_role_se_retrouve_par_son_numero() {
        let base = PanelSpec::default();
        let troisieme = PanelSpec::overlay_on(&base, OverlaySpec::role_layer(3));
        let spec = PlotSpec {
            panels: Panels::new(vec![base, troisieme]),
            ..PlotSpec::default()
        };
        assert_eq!(role_layer_of(&spec, 0, 3), Some(1));
        assert_eq!(role_layer_of(&spec, 0, 2), None);
    }

    #[test]
    fn le_graphe_a_un_x_par_une_colonne_x_ou_une_serie_ligne() {
        let (_project, _sheet, c) = sheet_with(&[X, Y]);
        let mut spec = PlotSpec::default();
        assert!(!spec_has_x(&spec));
        spec.panels
            .get_mut(0)
            .unwrap()
            .series
            .push(SeriesSpec::new(c[1]));
        assert!(!spec_has_x(&spec), "abscisse implicite : pas de X");
        spec.panels
            .get_mut(0)
            .unwrap()
            .series
            .push(SeriesSpec::new_xy(c[0], c[1]));
        assert!(spec_has_x(&spec));
        let mut rows = PlotSpec::default();
        let mut series = SeriesSpec::new(c[1]);
        series.row = Some(0);
        rows.panels.get_mut(0).unwrap().series.push(series);
        assert!(spec_has_x(&rows));
    }

    #[test]
    fn le_type_de_tracage_d_un_calque_est_celui_de_sa_derniere_serie() {
        let (_project, _sheet, c) = sheet_with(&[Y]);
        let mut panel = PanelSpec::default();
        assert_eq!(panel_plot_type(&panel), PlotType::Line, "calque vide");
        let mut a = SeriesSpec::new(c[0]);
        a.plot_type = PlotType::Scatter;
        let b = SeriesSpec::new(c[0]);
        panel.series = vec![a, b];
        assert_eq!(panel_plot_type(&panel), PlotType::Line);
        panel.series.reverse();
        assert_eq!(panel_plot_type(&panel), PlotType::Scatter);
    }

    #[test]
    fn le_spec_courant_est_celui_du_graphe_et_rien_d_autre() {
        let (mut project, sheet, _c) = sheet_with(&[Y]);
        let spec = spec_a_calques();
        let graph = project
            .add_graph(project.root(), Some("G"), spec.clone())
            .unwrap();
        let tree = project.tree();
        assert_eq!(current_plot_spec(tree, graph), Some(spec));
        assert_eq!(current_plot_spec(tree, sheet), None, "une feuille");
        assert_eq!(current_plot_spec(tree, NodeId::from_raw(987_654)), None);
    }
}
