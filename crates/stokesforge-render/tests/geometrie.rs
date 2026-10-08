//! Le cadrage des axes, éprouvé sur les cas qui font diviser par zéro.
//!
//! Le pipeline est générique sur le backend ; ce test s'en sert pour ce
//! qu'aucun backend réel ne permet de faire commodément — mettre le cadrage
//! sous contrainte sans produire un seul pixel. Le backend muet ci-dessous
//! est aussi, incidemment, la preuve que la généricité n'est pas décorative.

use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{AxisSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::text::{estimate, TextMeasurer};
use stokesforge_render::{pipeline, DrawnPlot};

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

    // Un backend muet doit aussi jeter les chemins : le défaut rastérise
    // l'épaisseur marquée des chemins sous-pixel comme une largeur réelle.
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        _path: I,
        _style: &S,
    ) -> Result<(), DrawingErrorKind<Jamais>> {
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
    let colonnes: Vec<(&str, Column)> = colonnes
        .iter()
        .map(|(nom, valeurs)| {
            (
                *nom,
                Column::from_block(
                    ColumnType::Numeric,
                    ValueBlock::Float(valeurs.clone()),
                    ColumnMeta::default(),
                )
                .expect("colonne"),
            )
        })
        .collect();
    geometrie_de_colonnes(&colonnes, construire)
}

/// Comme [`geometrie`], mais les colonnes sont fournies telles quelles — le
/// seul moyen d'en tracer une qui ne soit pas numérique.
fn geometrie_de_colonnes(
    colonnes: &[(&str, Column)],
    construire: impl Fn(&[NodeId]) -> PlotSpec,
) -> DrawnPlot {
    let mut project = Project::new("Géométrie").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let ids: Vec<NodeId> = colonnes
        .iter()
        .map(|(nom, colonne)| {
            project
                .add_column(sheet, None, Some(nom), colonne.clone())
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

/// Une colonne à types mixtes se trace **exactement comme** la colonne
/// numérique obtenue en blanchissant ses cellules texte et vides.
///
/// C'est la propriété qui dit que la fonctionnalité n'est entrée dans aucun
/// backend (invariant 5) : rien ici ne connaît `ColumnType::Mixed`. La
/// traduction a lieu une fois, dans `stokesforge_core::Column::numeric_view`, et
/// le pipeline — donc les deux backends — n'a rien appris de nouveau : il
/// écartait déjà les points non finis.
#[test]
fn une_colonne_mixte_ne_trace_que_ses_cellules_numeriques() {
    use stokesforge_core::MixedCell;

    let mixte = Column::from_mixed(&[
        MixedCell::Number(10.0),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Empty,
        MixedCell::Number(30.0),
    ]);
    let equivalent_numerique = Column::from_f64(&[10.0, f64::NAN, f64::NAN, 30.0]);

    let dessine = |colonne: Column| {
        geometrie_de_colonnes(&[("Y", colonne)], |ids| {
            un_panneau(SeriesSpec::new(ids[0]), |_| {})
        })
    };
    let a = dessine(mixte);
    let b = dessine(equivalent_numerique);

    assert_eq!(a.drawn_series, 1, "la série mixte est bien tracée");
    assert_eq!(a.y_range(), b.y_range(), "même cadrage vertical");
    assert_eq!(
        a.x_range(),
        b.x_range(),
        "même abscisse implicite : les lignes non numériques comptent toujours \
         comme des lignes, elles ne décalent pas l'indice des suivantes"
    );
    // Et le cadrage ne vient que des deux nombres : ni « n/a » ni la cellule
    // vide n'ont produit de point (5 % de marge de part et d'autre de
    // [10, 30]).
    assert!((a.y_range().0 - 9.0).abs() < 1e-9, "{:?}", a.y_range());
    assert!((a.y_range().1 - 31.0).abs() < 1e-9, "{:?}", a.y_range());
}

/// Une colonne mixte dont aucune cellule n'est un nombre est traçable sans
/// rien tracer — comme une colonne numérique entièrement `NaN`, et
/// contrairement à une colonne **texte**, qui est une référence rompue.
#[test]
fn une_colonne_mixte_sans_nombre_donne_une_figure_vide_pas_une_erreur() {
    use stokesforge_core::MixedCell;

    let drawn = geometrie_de_colonnes(
        &[(
            "Y",
            Column::from_mixed(&[MixedCell::Text("a".to_string()), MixedCell::Empty]),
        )],
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
    // Rien ne valide `range_min`/`range_max` dans `stokesforge-plot` (ce sont
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
    assert_eq!(drawn.panels[0].legend_entries, 1);
    assert_eq!(drawn.panels[0].series_geometry.len(), 1);
    assert!(
        drawn.y_range().1 < 2.0,
        "la série cachée a élargi le cadrage : {:?}",
        drawn.y_range()
    );
}

/// Sur un vrai tracé (titre « Test », axes titrés), le pixel désigne le bon
/// composant : la géométrie de [`stokesforge_render::hit`] est bien celle que
/// Plotters a allouée, pas une reconstitution.
#[test]
fn un_pixel_designe_le_composant_du_graphe_qui_est_dessous() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextTarget};
    use stokesforge_render::{component_at, AxisSide, PlotComponent};
    let drawn = geometrie(
        &[("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])],
        |ids| {
            un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
                panel.bottom.label = "Temps".to_string();
                panel.left.label = "Signal".to_string();
            })
        },
    );
    assert!(drawn.title_height > 0, "le titre « Test » occupe une bande");
    let panel = drawn.panel(0).expect("un panneau");
    let (ox, oy) = (panel.pixel_origin.0 as f64, panel.pixel_origin.1 as f64);
    let (w, h) = (panel.pixel_size.0 as f64, panel.pixel_size.1 as f64);
    let bottom = panel.axis_areas.bottom.size as f64;
    let left = panel.axis_areas.left.size as f64;
    let cx = ox + w / 2.0;
    let cy = oy + h / 2.0;
    assert_eq!(component_at(&drawn, cx, 2.0), Some(PlotComponent::Title));
    assert_eq!(
        component_at(&drawn, cx, cy),
        Some(PlotComponent::PlotArea { panel: 0 })
    );
    assert_eq!(
        component_at(&drawn, cx, oy + h + 4.0),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Bottom
        })
    );
    assert_eq!(
        component_at(&drawn, cx, oy + h + bottom - 2.0),
        Some(PlotComponent::Text(TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Bottom
        })),
        "le texte du titre d'axe (issue #53) passe avant sa bande"
    );
    // Dans la bande mais loin du texte : les propriétés de l'axe, comme avant.
    assert_eq!(
        component_at(&drawn, ox + 2.0, oy + h + bottom - 2.0),
        Some(PlotComponent::AxisTitle {
            panel: 0,
            side: AxisSide::Bottom
        })
    );
    assert_eq!(
        component_at(&drawn, ox - 4.0, cy),
        Some(PlotComponent::Axis {
            panel: 0,
            side: AxisSide::Left
        })
    );
    assert_eq!(
        component_at(&drawn, ox - left + 2.0, cy),
        Some(PlotComponent::Text(TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Left
        }))
    );
}

/// Issue #53 : un titre déplacé est tracé centré sur son ancre, sans changer
/// la mise en page — et, sans ancre, à sa place par défaut.
#[test]
fn un_titre_deplace_est_trace_sur_son_ancre_sans_changer_la_mise_en_page() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextAnchor, TextTarget};
    use stokesforge_render::PanelTransform;
    let colonnes = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let construire = |ancres: bool| {
        move |ids: &[NodeId]| {
            un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |panel| {
                panel.title = "Panneau".to_string();
                panel.bottom.label = "Temps".to_string();
                panel.left.label = "Signal".to_string();
                if ancres {
                    panel.title_anchor = Some(TextAnchor::new(1.5, 25.0));
                    panel.bottom.title_anchor = Some(TextAnchor::new(0.5, 12.0));
                    panel.left.title_anchor = Some(TextAnchor::new(0.25, 20.0));
                    panel.legend.anchor = Some(TextAnchor::new(1.0, 15.0));
                }
            })
        }
    };
    let defaut = geometrie(&colonnes, construire(false));
    let deplace = geometrie(&colonnes, construire(true));

    let panneau = deplace.panel(0).expect("un panneau");
    let transform = PanelTransform::of(panneau).expect("zone de tracé");
    let attendu = |x: f64, y: f64| transform.data_to_pixel(x, y).expect("projetable");
    let texte = |cible: TextTarget| {
        panneau
            .texts
            .iter()
            .find(|t| t.target == cible)
            .copied()
            .unwrap_or_else(|| panic!("{cible:?} n'est pas dans {:?}", panneau.texts))
    };
    for (cible, (x, y)) in [
        (TextTarget::PanelTitle { panel: 0 }, (1.5, 25.0)),
        (
            TextTarget::AxisTitle {
                panel: 0,
                side: PlotAxisSide::Bottom,
            },
            (0.5, 12.0),
        ),
        (
            TextTarget::AxisTitle {
                panel: 0,
                side: PlotAxisSide::Left,
            },
            (0.25, 20.0),
        ),
        (TextTarget::Legend { panel: 0 }, (1.0, 15.0)),
    ] {
        let drawn = texte(cible);
        let (px, py) = attendu(x, y);
        assert!(drawn.anchored, "{cible:?} est posé sur son ancre");
        assert!(
            (drawn.center.0 - px).abs() <= 1.5 && (drawn.center.1 - py).abs() <= 1.5,
            "{cible:?} centré en {:?}, attendu {:?}",
            drawn.center,
            (px, py)
        );
    }
    // La mise en page ne dépend pas de la place du titre : la zone de tracé
    // est la même qu'avec les titres à leur place (sinon le titre sauterait
    // au lâcher).
    assert_eq!(
        deplace.panel(0).unwrap().pixel_size,
        defaut.panel(0).unwrap().pixel_size
    );
    assert_eq!(
        deplace.panel(0).unwrap().pixel_origin,
        defaut.panel(0).unwrap().pixel_origin
    );
    // Sans ancre, les titres sont à leur place par défaut, dans leur bande.
    let par_defaut = defaut.panel(0).expect("un panneau");
    assert!(par_defaut.texts.iter().all(|t| !t.anchored));
    assert!(par_defaut.axis_areas.bottom.titled && !panneau.axis_areas.bottom.titled);
    assert_eq!(par_defaut.texts.len(), 4, "titre, deux axes, légende");
    // Les lignes de la légende pointent la série.
    assert_eq!(par_defaut.legend_rows.len(), 1);
    assert_eq!(par_defaut.legend_rows[0].series, 0);
}

/// Issue #53 : le titre du graphe déplacé garde sa bande (vide), et une
/// ancre non finie le laisse à sa place.
#[test]
fn le_titre_du_graphe_deplace_garde_sa_bande_vide() {
    use stokesforge_plot::{TextAnchor, TextTarget};
    let colonnes = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let avec = |ancre: Option<TextAnchor>| {
        geometrie(&colonnes, move |ids| {
            let mut spec = un_panneau(SeriesSpec::new_xy(ids[0], ids[1]), |_| {});
            spec.title_anchor = ancre;
            spec
        })
    };
    let defaut = avec(None);
    assert!(defaut.title_height > 0);
    let titre = defaut.title.expect("titre tracé");
    assert_eq!(titre.target, TextTarget::Title);
    assert!(!titre.anchored);

    let deplace = avec(Some(TextAnchor::new(1.0, 30.0)));
    assert_eq!(deplace.title_height, defaut.title_height, "bande réservée");
    assert!(deplace.title.expect("titre tracé").anchored);

    let abime = avec(Some(TextAnchor::new(f64::NAN, 30.0)));
    assert_eq!(abime.title_height, defaut.title_height, "ancre ignorée");
    assert!(!abime.title.expect("titre tracé").anchored);
}

/// Issues #77 et #78 : le corps réglé sur une zone de texte change la taille
/// qu'elle occupe — titre du graphe, titre d'axe, légende — et rien d'autre ne
/// bouge (la zone de tracé rétrécit seulement de ce que le texte prend).
#[test]
fn le_corps_d_une_zone_de_texte_change_sa_taille() {
    use stokesforge_plot::{AxisSide as PlotAxisSide, TextFont, TextTarget};
    let colonnes = [("X", vec![0.0, 1.0, 2.0]), ("Y", vec![10.0, 20.0, 30.0])];
    let construire = |gros: bool| {
        move |ids: &[NodeId]| {
            let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
            series.name = "Courbe".to_string();
            un_panneau(series, |panel| {
                panel.bottom.label = "Temps".to_string();
                if gros {
                    let font = TextFont {
                        family: String::new(),
                        size: Some(32.0),
                    };
                    panel.bottom.title_font = font.clone();
                    panel.legend.font = font;
                }
            })
        }
    };
    let defaut = geometrie(&colonnes, construire(false));
    let gros = geometrie(&colonnes, construire(true));
    let taille = |g: &stokesforge_render::DrawnPlot, cible: TextTarget| {
        g.panel(0)
            .unwrap()
            .texts
            .iter()
            .find(|t| t.target == cible)
            .unwrap_or_else(|| panic!("{cible:?} absent"))
            .size
    };
    for cible in [
        TextTarget::AxisTitle {
            panel: 0,
            side: PlotAxisSide::Bottom,
        },
        TextTarget::Legend { panel: 0 },
    ] {
        let (petit, grand) = (taille(&defaut, cible), taille(&gros, cible));
        assert!(
            grand.1 > petit.1,
            "{cible:?} : corps 32 ({grand:?}) pas plus haut que le défaut ({petit:?})"
        );
    }
}

/// Poignées de sélection (#98) : une barre négative porte la sienne en bas,
/// côté valeur, pas sur la ligne de base ; une barre positive en haut.
#[test]
fn l_extremite_libre_d_une_barre_est_du_cote_de_sa_valeur() {
    use stokesforge_plot::PlotType;
    let drawn = geometrie(&[("X", vec![1.0, 2.0]), ("Y", vec![10.0, -10.0])], |ids| {
        let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
        series.plot_type = PlotType::Bar;
        un_panneau(series, |_| {})
    });
    let bars = &drawn.panels[0].series_geometry[0];
    assert_eq!(bars.bars.len(), 2);
    assert_eq!(bars.bar_tips.len(), 2);
    let (positive, negative) = (bars.bars[0], bars.bars[1]);
    // Pixels : y croît vers le bas.
    assert_eq!(bars.bar_tips[0].1, positive.0 .1, "{bars:?}");
    assert_eq!(bars.bar_tips[1].1, negative.1 .1, "{bars:?}");
    assert_eq!(bars.bar_tips[0].0, (positive.0 .0 + positive.1 .0) / 2);
}

/// Les moustaches d'une série à barres d'erreur se désignent, mais ne
/// forment pas un tronçon de la ligne : elles ne portent pas de poignée.
#[test]
fn les_moustaches_ne_sont_pas_des_troncons_de_la_ligne() {
    use stokesforge_plot::PlotType;
    let drawn = geometrie(
        &[
            ("X", vec![1.0, 2.0, 3.0]),
            ("Y", vec![10.0, 20.0, 15.0]),
            ("E", vec![1.0, 2.0, 1.0]),
        ],
        |ids| {
            let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
            series.y_error = Some(ids[2]);
            series.plot_type = PlotType::ErrorBar;
            un_panneau(series, |_| {})
        },
    );
    let series = &drawn.panels[0].series_geometry[0];
    assert_eq!(series.paths.len(), 1, "{series:?}");
    assert_eq!(series.paths[0].len(), 3);
    assert!(!series.whiskers.is_empty());
    let handles = series.handle_centers(|p| p);
    assert!(!handles.is_empty());
    // Toutes les poignées sont sur la ligne, aucune sur une moustache.
    for handle in handles {
        let on_line = series.paths[0].windows(2).any(|s| {
            let (a, b) = (s[0], s[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let cross = dx * (handle.1 - a.1) - dy * (handle.0 - a.0);
            (cross / dx.hypot(dy)).abs() < 1e-6
                && handle.0 >= a.0.min(b.0) - 1e-6
                && handle.0 <= a.0.max(b.0) + 1e-6
        });
        assert!(on_line, "poignée hors de la ligne : {handle:?}");
    }
}

/// L'encre des poignées est l'encre automatique du fond de la figure :
/// sombre sur le fond par défaut (transparent, jugé sur du blanc), blanche
/// sur un fond sombre.
#[test]
fn l_encre_des_poignees_suit_le_fond_de_la_figure() {
    use stokesforge_render::color::{auto_ink, DARK_INK, LIGHT_INK};
    use stokesforge_render::pipeline::figure_ink;
    let mut spec = PlotSpec::empty("Encre");
    assert_eq!(figure_ink(&spec), DARK_INK);
    spec.background = "#101828".to_owned();
    assert_eq!(figure_ink(&spec), LIGHT_INK);
    assert_eq!(figure_ink(&spec), auto_ink((0x10, 0x18, 0x28)));
    spec.background = "#ffffff".to_owned();
    assert_eq!(figure_ink(&spec), DARK_INK);
}

/// #167 : un calque superposé partage la zone de tracé du calque recouvert
/// (dimension liée), garde sa propre échelle quand son axe est affiché, et
/// suit celle du calque recouvert sinon.
#[test]
fn un_calque_superpose_partage_la_zone_de_trace_et_garde_son_echelle() {
    use stokesforge_plot::{OverlaySpec, Panels};
    let colonnes = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![400.0, 500.0, 800.0]),
        ("Y2", vec![0.0, 1.0, 2.0]),
    ];
    let construire = |overlay: OverlaySpec| {
        move |ids: &[NodeId]| {
            let mut base = PanelSpec::at(0, 0);
            base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
            let mut haut = PanelSpec::overlay_on(&base, overlay);
            haut.series.push(SeriesSpec::new_xy(ids[2], ids[3]));
            PlotSpec::empty("Test").with_panels(Panels::new(vec![base, haut]))
        }
    };

    let drawn = geometrie(
        &colonnes,
        construire(OverlaySpec {
            x_top: true,
            y_right: true,
        }),
    );
    assert_eq!(drawn.panels.len(), 2);
    let (base, haut) = (&drawn.panels[0], &drawn.panels[1]);
    assert_eq!((haut.index, haut.row, haut.column), (1, 0, 0));
    assert_eq!(base.pixel_origin, haut.pixel_origin, "même zone de tracé");
    assert_eq!(base.pixel_size, haut.pixel_size);
    assert!(haut.axis_areas.top.size > 0 && haut.axis_areas.right.size > 0);
    assert_eq!(
        (haut.axis_areas.bottom.size, haut.axis_areas.left.size),
        (0, 0)
    );
    assert!(
        base.axis_areas.top.size == 0,
        "l'axe du haut est celui du calque"
    );
    assert!(
        haut.x_range.0 > 300.0,
        "échelle X propre : {:?}",
        haut.x_range
    );
    assert!(base.x_range.1 < 10.0, "{:?}", base.x_range);

    // Axes non affichés : l'échelle est celle du calque recouvert.
    let lie = geometrie(
        &colonnes,
        construire(OverlaySpec {
            x_top: false,
            y_right: false,
        }),
    );
    assert_eq!(lie.panels[1].x_range, lie.panels[0].x_range);
    assert_eq!(lie.panels[1].y_range, lie.panels[0].y_range);
    assert_eq!(lie.panels[1].pixel_size, lie.panels[0].pixel_size);
    assert!(
        drawn.panels[0].pixel_size.0 < lie.panels[0].pixel_size.0,
        "les axes du haut et de droite réservent leur place à tous les calques de la case"
    );
}

/// #174 : les échelles d'un calque superposé sont jointes à celles du calque
/// recouvert — mêmes fractions de l'axe pour les graduations, donc le 0 du
/// haut en face du 0 du bas — tout en enveloppant ses propres données.
#[test]
fn les_echelles_d_un_calque_superpose_sont_jointes_a_celles_du_calque_recouvert() {
    use stokesforge_plot::{OverlaySpec, Panels};
    use stokesforge_render::scale::{linear_key_points, DEFAULT_LABELS};
    let colonnes = [
        ("X", vec![0.0, 400.0, 800.0]),
        ("Y", vec![0.0, 5.0, 10.0]),
        ("X2", vec![0.0, 60.0, 120.0]),
        ("Y2", vec![0.0, 0.4, 0.9]),
    ];
    let drawn = geometrie(&colonnes, |ids: &[NodeId]| {
        let mut base = PanelSpec::at(0, 0);
        base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
        let mut haut = PanelSpec::overlay_on(
            &base,
            OverlaySpec {
                x_top: true,
                y_right: true,
            },
        );
        haut.series.push(SeriesSpec::new_xy(ids[2], ids[3]));
        PlotSpec::empty("Test").with_panels(Panels::new(vec![base, haut]))
    });
    let (base, haut) = (&drawn.panels[0], &drawn.panels[1]);
    let fractions = |range: (f64, f64)| -> Vec<f64> {
        linear_key_points(range, DEFAULT_LABELS)
            .iter()
            .map(|t| (t - range.0) / (range.1 - range.0))
            .collect()
    };
    for (own, other, data) in [
        (base.x_range, haut.x_range, (0.0, 120.0)),
        (base.y_range, haut.y_range, (0.0, 0.9)),
    ] {
        let (a, b) = (fractions(own), fractions(other));
        assert_eq!(a.len(), b.len(), "{own:?} / {other:?}");
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6, "{own:?} / {other:?}");
        }
        assert!(other.0 <= data.0 && other.1 >= data.1, "{other:?}");
    }
}

/// #172 : la légende du calque ordinaire liste aussi les courbes des calques
/// superposés ; celle du calque superposé, masquée à la création, n'en fait pas
/// une seconde, et l'afficher la limite à ses propres courbes.
#[test]
fn la_legende_du_calque_ordinaire_liste_les_courbes_des_calques_superposes() {
    use stokesforge_plot::{OverlaySpec, Panels};
    let colonnes = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("Y2", vec![0.0, 1.0, 2.0]),
    ];
    let construire = |legende_haut: bool| {
        move |ids: &[NodeId]| {
            let mut base = PanelSpec::at(0, 0);
            base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
            let mut haut = PanelSpec::overlay_on(
                &base,
                OverlaySpec {
                    x_top: false,
                    y_right: true,
                },
            );
            haut.legend.visible = legende_haut;
            haut.series.push(SeriesSpec::new_xy(ids[0], ids[2]));
            PlotSpec::empty("Test").with_panels(Panels::new(vec![base, haut]))
        }
    };
    let drawn = geometrie(&colonnes, construire(false));
    assert_eq!(drawn.panels[0].legend_entries, 2, "tous les calques");
    assert_eq!(drawn.panels[1].legend_entries, 0, "pas de seconde légende");
    let drawn = geometrie(&colonnes, construire(true));
    assert_eq!(drawn.panels[0].legend_entries, 2);
    assert_eq!(drawn.panels[1].legend_entries, 1, "ses propres courbes");
}
