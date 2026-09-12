//! Le cadrage des axes, éprouvé sur les cas qui font diviser par zéro.
//!
//! Le pipeline est générique sur le backend ; ce test s'en sert pour ce
//! qu'aucun backend réel ne permet de faire commodément — mettre le cadrage
//! sous contrainte sans produire un seul pixel. Le backend muet ci-dessous
//! est aussi, incidemment, la preuve que la généricité n'est pas décorative.

use plotters_backend::{
    BackendColor, BackendCoord, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::{NodeId, Project};
use plotyx_plot::{AxisSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render::text::{estimate, TextMeasurer};
use plotyx_render::{pipeline, DrawnPlot};

/// Un backend qui jette tout — sauf la mesure, qui reste celle du projet.
///
/// Jeter la mesure aussi ferait de ce test un test d'autre chose : c'est
/// elle qui détermine la place laissée aux axes, donc l'intervalle sur
/// lequel le cadrage se projette.
struct Muet {
    size: (u32, u32),
    measurer: TextMeasurer,
}

#[derive(Debug)]
enum Jamais {}

impl std::fmt::Display for Jamais {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Jamais {}

impl DrawingBackend for Muet {
    type ErrorType = Jamais;

    fn get_size(&self) -> (u32, u32) {
        self.size
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Jamais>> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Jamais>> {
        Ok(())
    }

    fn draw_pixel(
        &mut self,
        _point: BackendCoord,
        _color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Jamais>> {
        Ok(())
    }

    /// **Indispensable, et ce n'est pas évident.**
    ///
    /// Avec `plotters` en `default-features = false`, l'implémentation par
    /// défaut de `draw_text` finit par appeler `FontData::draw` — dont la
    /// version « naive » est un `panic!("The font implementation is unable
    /// to draw text")` (`plotters/src/style/font/mod.rs`). Autrement dit :
    /// **tout backend qui ne surcharge pas `draw_text` plante à la première
    /// graduation**, et pas avec une erreur, avec une panique. Les deux
    /// backends du chantier la surchargent (egui directement, SVG par
    /// délégation à `SVGBackend`), mais un troisième — ou ce backend muet —
    /// tomberait dedans. Découvert en écrivant ce test, qui est donc aussi
    /// l'endroit où le noter.
    fn draw_text<S: BackendTextStyle>(
        &mut self,
        _text: &str,
        _style: &S,
        _pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Jamais>> {
        Ok(())
    }

    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Jamais>> {
        Ok(estimate(&self.measurer, text, style))
    }
}

/// Trace un spec sur un jeu de colonnes et rend la géométrie retenue.
fn geometrie(
    colonnes: &[(&str, Vec<f64>)],
    construire: impl Fn(&[NodeId]) -> PlotSpec,
) -> DrawnPlot {
    let mut project = Project::new("Géométrie").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let ids: Vec<NodeId> = colonnes
        .iter()
        .map(|(nom, valeurs)| {
            project
                .add_column(
                    sheet,
                    None,
                    Some(nom),
                    Column::from_block(
                        ColumnType::Numeric,
                        ValueBlock::Float(valeurs.clone()),
                        ColumnMeta::default(),
                    )
                    .expect("colonne"),
                )
                .expect("ajout")
        })
        .collect();

    let spec = construire(&ids);
    let plot = resolve_spec(project.tree(), &spec);
    let backend = Muet {
        size: (640, 480),
        measurer: TextMeasurer::default(),
    };
    pipeline::draw(backend, &plot).expect("tracé")
}

fn un_panneau(series: SeriesSpec, axe: impl FnOnce(&mut PanelSpec)) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(series);
    axe(&mut panel);
    PlotSpec::empty("Test").with_panels(panel)
}

#[test]
fn le_cadrage_suit_les_donnees_avec_une_marge() {
    let drawn = geometrie(
        &[("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])],
        |ids| un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |_| {}),
    );
    // 5 % de marge de part et d'autre.
    assert!(
        (drawn.x_range().0 - -0.1).abs() < 1e-9,
        "{:?}",
        drawn.x_range()
    );
    assert!(
        (drawn.x_range().1 - 2.1).abs() < 1e-9,
        "{:?}",
        drawn.x_range()
    );
    assert!(
        (drawn.y_range().0 - 9.0).abs() < 1e-9,
        "{:?}",
        drawn.y_range()
    );
    assert!(
        (drawn.y_range().1 - 31.0).abs() < 1e-9,
        "{:?}",
        drawn.y_range()
    );
    assert_eq!(drawn.drawn_series, 1);
}

#[test]
fn une_serie_constante_ne_donne_pas_un_intervalle_vide() {
    // Sans écartement, `build_cartesian_2d` recevrait `5.0..5.0` et Plotters
    // diviserait par zéro en projetant. Un seul point est le même cas.
    let drawn = geometrie(
        &[("X", vec![1.0, 2.0, 3.0]), ("Y", vec![5.0, 5.0, 5.0])],
        |ids| un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |_| {}),
    );
    assert!(
        drawn.y_range().0 < drawn.y_range().1,
        "{:?}",
        drawn.y_range()
    );

    let zero = geometrie(&[("Y", vec![0.0])], |ids| {
        un_panneau(SeriesSpec::new(ids[0]), |_| {})
    });
    assert!(zero.y_range().0 < zero.y_range().1, "{:?}", zero.y_range());
    assert!(zero.x_range().0 < zero.x_range().1, "{:?}", zero.x_range());
}

#[test]
fn une_serie_entierement_non_finie_ne_fait_pas_echouer_le_trace() {
    // `NaN` est la valeur manquante du socle, et une colonne peut n'être que
    // ça. Le rendu doit sortir une figure vide, pas une erreur.
    let drawn = geometrie(
        &[("Y", vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY])],
        |ids| un_panneau(SeriesSpec::new(ids[0]), |_| {}),
    );
    assert_eq!(drawn.drawn_series, 0);
    assert_eq!(drawn.y_range(), (0.0, 1.0));
}

#[test]
fn un_cadrage_explicite_prime_sur_les_donnees() {
    let drawn = geometrie(&[("X", vec![0.0, 1.0]), ("Y", vec![0.0, 1.0])], |ids| {
        un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
            panel.left = AxisSpec {
                auto_range: false,
                range_min: Some(-5.0),
                range_max: Some(5.0),
                ..AxisSpec::default()
            };
        })
    });
    assert_eq!(drawn.y_range(), (-5.0, 5.0));
}

#[test]
fn un_cadrage_explicite_incoherent_retombe_sur_les_donnees() {
    // Rien ne valide `range_min`/`range_max` dans `plotyx-plot` (ce sont
    // deux `Option<f64>` libres) et un fichier peut porter n'importe quoi :
    // bornes égales, `NaN`, inversées. Le rendu doit dégrader, pas paniquer.
    for (min, max) in [
        (Some(3.0), Some(3.0)),
        (Some(f64::NAN), Some(1.0)),
        (Some(1.0), Some(f64::INFINITY)),
    ] {
        let drawn = geometrie(&[("Y", vec![0.0, 10.0])], |ids| {
            un_panneau(SeriesSpec::new(ids[0]), |panel| {
                panel.left = AxisSpec {
                    auto_range: false,
                    range_min: min,
                    range_max: max,
                    ..AxisSpec::default()
                };
            })
        });
        assert!(
            drawn.y_range().0 < drawn.y_range().1,
            "cadrage {min:?}..{max:?} a donné {:?}",
            drawn.y_range()
        );
    }

    // Bornes à l'envers : acceptées, remises à l'endroit — c'est un cadrage
    // complet et cohérent une fois trié, pas une donnée fautive.
    let drawn = geometrie(&[("Y", vec![0.0, 10.0])], |ids| {
        un_panneau(SeriesSpec::new(ids[0]), |panel| {
            panel.left = AxisSpec {
                auto_range: false,
                range_min: Some(9.0),
                range_max: Some(-9.0),
                ..AxisSpec::default()
            };
        })
    });
    assert_eq!(drawn.y_range(), (-9.0, 9.0));
}

#[test]
fn un_axe_inverse_rend_ses_bornes_a_lenvers() {
    let drawn = geometrie(&[("Y", vec![0.0, 10.0])], |ids| {
        un_panneau(SeriesSpec::new(ids[0]), |panel| {
            panel.left = AxisSpec {
                inverted: true,
                ..AxisSpec::default()
            };
        })
    });
    assert!(
        drawn.y_range().0 > drawn.y_range().1,
        "un axe inversé doit rendre (max, min) : {:?}",
        drawn.y_range()
    );
}

#[test]
fn une_serie_invisible_ne_compte_ni_dans_le_trace_ni_dans_le_cadrage() {
    let drawn = geometrie(
        &[("Y", vec![0.0, 1.0]), ("Enorme", vec![0.0, 1e9])],
        |ids| {
            let mut panel = PanelSpec::at(0, 0);
            panel.series.push(SeriesSpec::new(ids[0]));
            let mut cachee = SeriesSpec::new(ids[1]);
            cachee.visible = false;
            panel.series.push(cachee);
            PlotSpec::empty("Test").with_panels(panel)
        },
    );
    assert_eq!(drawn.drawn_series, 1);
    assert!(
        drawn.y_range().1 < 2.0,
        "la série cachée a élargi le cadrage : {:?}",
        drawn.y_range()
    );
}
