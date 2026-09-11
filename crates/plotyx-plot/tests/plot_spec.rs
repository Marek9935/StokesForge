//! Le `PlotSpec` lui-même : construction, invariants, lecture d'un spec
//! composé. Port de `legacy-python/tests/test_plot_spec.py`.
//!
//! Aucune de ces vérifications n'a besoin d'un moteur de rendu — c'est le
//! point : la couche de description est testable seule, exactement comme
//! `plotyx-core`. Elle n'a même pas besoin d'un `Tree` : une série référence
//! des `NodeId`, et un `NodeId` se fabrique sans arbre.
//!
//! # Ce qui n'a pas été transposé, et pourquoi
//!
//! Sept vérifications de la suite Python n'ont pas d'équivalent ici, et aucune
//! ne disparaît parce qu'elle gênait :
//!
//! - `test_a_series_must_designate_a_y_column` — `SeriesSpec::y` est un
//!   `NodeId` obligatoire ; il n'y a pas de valeur à refuser ;
//! - `test_a_series_only_attaches_to_the_left_or_right_axis` — `SeriesAxis`
//!   n'a que deux variantes ; l'appel fautif ne compile pas ;
//! - `test_histogram_bins_and_widths_are_sanitised` — `NonZeroU32` et
//!   `Option<f64>` remplacent le `max(1, …)` et le `float(…)` ; il n'y a plus
//!   d'assainissement à observer. Ce qui reste vérifiable l'est ci-dessous
//!   (`default_histogram_bins_is_forty`, `a_bar_width_of_none_means_deduced`) ;
//! - `test_string_values_are_coerced_to_enums` — ce crate n'a pas de
//!   vocabulaire textuel : les énumérations n'entrent ni ne sortent sous forme
//!   de chaînes, la conversion depuis le format sur disque appartient à
//!   `plotyx-persistence` ;
//! - `test_ids_are_unique` — il n'y a pas d'identifiant de série (voir
//!   `SeriesRef`) ;
//! - `test_round_trip_through_dict_is_faithful`,
//!   `test_round_trip_survives_json` et
//!   `test_unknown_fields_are_ignored_rather_than_fatal` — pas de
//!   sérialisation dans ce crate, donc pas d'aller-retour à vérifier.
//!
//! Les onze tests d'édition (`edit.*`) sont hors périmètre de la phase, à
//! l'exception de la palette, vérifiée ici et dans `src/style.rs`.

use std::num::NonZeroU32;

use plotyx_id::NodeId;
use plotyx_plot::{
    palette_color, AxisScale, AxisSide, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle,
    PanelSpec, Panels, PlotSpec, PlotType, SeriesAxis, SeriesRef, SeriesSpec, SymbolShape,
    SymbolSpec, DEFAULT_HISTOGRAM_BINS, DEFAULT_PALETTE, SPEC_FORMAT_VERSION,
};

/// Colonnes du décor. Un `NodeId` se fabrique sans arbre : le spec ne sait pas
/// si la colonne existe, et c'est précisément ce qui fait qu'une référence
/// brisée est détectable — par la résolution, pas ici.
const X: NodeId = NodeId::from_raw(10);
const Y1: NodeId = NodeId::from_raw(11);
const Y2: NodeId = NodeId::from_raw(12);
const Y3: NodeId = NodeId::from_raw(13);
const ERR: NodeId = NodeId::from_raw(14);
const ABSENTE: NodeId = NodeId::from_raw(99);

// -- construction et invariants -----------------------------------------------

#[test]
fn an_empty_spec_always_has_one_panel() {
    let spec = PlotSpec::empty("Titre");
    assert_eq!(spec.title, "Titre");
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(spec.series_count(), 0);
    assert_eq!(spec.rows(), 1);
    assert_eq!(spec.columns(), 1);
    assert_eq!(spec.format_version, SPEC_FORMAT_VERSION);
}

#[test]
fn a_spec_built_without_panels_gets_one_anyway() {
    // Un graphe sans panneau ne serait pas rendable : le type en garantit un.
    let spec = PlotSpec::empty("").with_panels(Vec::new());
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(Panels::new(Vec::new()).len(), 1);
}

#[test]
fn a_series_designates_its_columns_by_identity() {
    // Le contraire du port naïf : pas de nom, pas de chemin, un NodeId.
    let series = SeriesSpec::new_xy(X, Y1);
    assert_eq!(series.y, Y1);
    assert_eq!(series.x, Some(X));
    assert_eq!(series.x_error, None);
    assert_eq!(series.y_error, None);
}

#[test]
fn a_series_attaches_to_the_left_axis_by_default() {
    let series = SeriesSpec::new(Y1);
    assert_eq!(series.axis, SeriesAxis::Left);
    assert_eq!(series.axis.side(), AxisSide::Left);
    assert_eq!(AxisSide::from(SeriesAxis::Right), AxisSide::Right);
}

#[test]
fn default_styles_match_the_python_defaults() {
    // Les valeurs par défaut sont un contrat : elles décident de ce à quoi
    // ressemble un graphe qu'on n'a pas configuré.
    let series = SeriesSpec::new(Y1);
    assert_eq!(series.plot_type, PlotType::Line);
    assert!(series.visible && series.show_in_legend);
    assert!(series.name.is_empty());
    assert_eq!(series.line, LineSpec::default());
    assert_eq!(series.line.style, LineStyle::Solid);
    assert!((series.line.width - 1.5).abs() < f64::EPSILON);
    assert_eq!(series.symbol.shape, SymbolShape::Circle);
    assert!((series.symbol.size - 7.0).abs() < f64::EPSILON);
    assert_eq!(AxisSpec::default().scale, AxisScale::Linear);
    assert_eq!(LegendSpec::default().position, LegendPosition::TopRight);
    assert!(LegendSpec::default().visible);
}

#[test]
fn default_histogram_bins_is_forty() {
    assert_eq!(DEFAULT_HISTOGRAM_BINS, NonZeroU32::new(40).expect("40 ≠ 0"));
    assert_eq!(SeriesSpec::new(Y1).histogram_bins, DEFAULT_HISTOGRAM_BINS);
}

#[test]
fn a_bar_width_of_none_means_deduced() {
    // Le `0.0` sentinelle de la version Python devient `None` : zéro était une
    // largeur de barre parfaitement écrivable, et donc ambiguë.
    assert_eq!(SeriesSpec::new(Y1).bar_width, None);
    let explicite = SeriesSpec {
        bar_width: Some(0.8),
        ..SeriesSpec::new(Y1)
    };
    assert_eq!(explicite.bar_width, Some(0.8));
}

#[test]
fn series_columns_list_only_the_references_that_exist() {
    let series = SeriesSpec {
        x: Some(X),
        y_error: Some(ERR),
        ..SeriesSpec::new(Y1)
    };
    assert_eq!(series.referenced_columns(), vec![X, Y1, ERR]);
    assert!(series.references(ERR));
    assert!(!series.references(ABSENTE));
}

#[test]
fn histogram_does_not_need_an_x_column() {
    assert!(!PlotType::Histogram.needs_x());
    assert!(PlotType::Bar.needs_x());
}

#[test]
fn plot_type_draw_flags() {
    assert!(PlotType::Line.draws_line() && !PlotType::Line.draws_symbols());
    assert!(PlotType::Scatter.draws_symbols() && !PlotType::Scatter.draws_line());
    assert!(PlotType::LineScatter.draws_line() && PlotType::LineScatter.draws_symbols());
    assert!(PlotType::ErrorBar.draws_line() && PlotType::ErrorBar.draws_symbols());
}

#[test]
fn axis_display_label_never_shows_empty_parentheses() {
    let axis = |label: &str, unit: &str| AxisSpec {
        label: label.to_string(),
        unit: unit.to_string(),
        ..AxisSpec::default()
    };
    assert_eq!(axis("Temps", "s").display_label(), "Temps (s)");
    assert_eq!(axis("Temps", "").display_label(), "Temps");
    assert_eq!(axis("", "s").display_label(), "(s)");
    assert_eq!(axis("", "").display_label(), "");
}

#[test]
fn explicit_range_needs_both_bounds_and_auto_off() {
    let bounded = AxisSpec {
        range_min: Some(0.0),
        range_max: Some(1.0),
        ..AxisSpec::default()
    };
    // `auto_range` vaut `true` : les bornes sont mémorisées, pas appliquées.
    assert!(!bounded.has_explicit_range());
    assert!(!AxisSpec {
        auto_range: false,
        range_max: None,
        ..bounded.clone()
    }
    .has_explicit_range());
    assert!(AxisSpec {
        auto_range: false,
        ..bounded
    }
    .has_explicit_range());
}

#[test]
fn palette_cycles() {
    assert_eq!(palette_color(0), DEFAULT_PALETTE[0]);
    assert_eq!(palette_color(DEFAULT_PALETTE.len()), DEFAULT_PALETTE[0]);
}

// -- lecture d'un spec composé --------------------------------------------------

/// Le décor de la suite Python (`two_panel_spec`), construit à la main : les
/// fonctions d'édition qui le montaient là-bas sont hors périmètre de la
/// phase. Deux séries sur le premier panneau, dont une sur l'axe de droite ;
/// un histogramme seul sur le second, empilé en dessous.
fn two_panel_spec() -> PlotSpec {
    let premier = PanelSpec {
        series: vec![
            SeriesSpec::new_xy(X, Y1),
            SeriesSpec {
                axis: SeriesAxis::Right,
                ..SeriesSpec::new_xy(X, Y2)
            },
        ],
        ..PanelSpec::default()
    };
    let second = PanelSpec {
        series: vec![SeriesSpec {
            plot_type: PlotType::Histogram,
            ..SeriesSpec::new(Y3)
        }],
        ..PanelSpec::at(1, 0)
    };
    PlotSpec::empty("Deux panneaux").with_panels(vec![premier, second])
}

#[test]
fn iter_series_walks_every_panel() {
    let spec = two_panel_spec();
    let panels: Vec<usize> = spec.iter_series().map(|(at, _)| at.panel).collect();
    assert_eq!(panels, vec![0, 0, 1]);
    assert_eq!(spec.series_count(), 3);
}

#[test]
fn columns_are_deduplicated_and_ordered() {
    let spec = two_panel_spec();
    assert_eq!(spec.referenced_columns(), vec![X, Y1, Y2, Y3]);
    assert_eq!(spec.panels[0].referenced_columns(), vec![X, Y1, Y2]);
}

#[test]
fn a_panel_knows_it_uses_the_secondary_axis() {
    let spec = two_panel_spec();
    assert!(spec.panels[0].uses_right_axis());
    assert!(!spec.panels[1].uses_right_axis());
}

#[test]
fn rows_and_columns_come_from_the_panel_positions() {
    let spec = two_panel_spec();
    assert_eq!(spec.rows(), 2);
    assert_eq!(spec.columns(), 1);
    assert_eq!(spec.panels[1].row, 1);

    // Deux panneaux côte à côte, plutôt qu'empilés.
    let cote_a_cote =
        PlotSpec::empty("").with_panels(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]);
    assert_eq!(cote_a_cote.rows(), 1);
    assert_eq!(cote_a_cote.columns(), 2);
}

#[test]
fn a_series_is_addressed_by_its_position() {
    // Ce qui remplace `find_series(id)` / `panel_index(id)` de la version
    // Python : l'adresse est la position, et elle vaut pour ce spec-là.
    let spec = two_panel_spec();
    let at = SeriesRef::new(1, 0);
    assert_eq!(spec.series_at(at), Some(&spec.panels[1].series[0]));
    assert_eq!(spec.series_at(SeriesRef::new(9, 0)), None);
    assert_eq!(spec.series_at(SeriesRef::new(1, 9)), None);
    assert_eq!(
        spec.iter_series().map(|(at, _)| at).last(),
        Some(SeriesRef::new(1, 0))
    );
}

#[test]
fn panel_axis_lookup_by_side() {
    let spec = two_panel_spec();
    let panel = &spec.panels[0];
    assert_eq!(panel.axis(AxisSide::Left), &panel.left);
    assert_eq!(panel.axis(AxisSide::Bottom), &panel.bottom);
    assert_eq!(panel.axis(AxisSide::Right), &panel.right);
    // L'axe du haut est le seul masqué par défaut.
    assert_eq!(panel.axis(AxisSide::Top), &panel.top);
    assert!(!panel.top.visible);
    assert!(panel.bottom.visible && panel.left.visible && panel.right.visible);
}

#[test]
fn a_spec_knows_which_columns_would_break_it() {
    // La question que posera la suppression d'une colonne (§11.1) : une
    // référence brisée doit être détectable, jamais silencieuse.
    let spec = two_panel_spec();
    assert!(spec.references(Y3));
    assert!(!spec.references(ABSENTE));
}

// -- immuabilité ------------------------------------------------------------------

#[test]
fn editing_a_spec_never_touches_the_one_it_came_from() {
    // La version Python tenait ça par `frozen=True` ; ici c'est la propriété
    // des valeurs. Le test reste utile : il verrouille le fait qu'un spec se
    // modifie par clonage, ce dont dépend l'annulation (l'ancien spec est le
    // delta d'annulation).
    let spec = two_panel_spec();
    let mut modifie = spec.clone();
    modifie
        .series_at_mut(SeriesRef::new(0, 0))
        .expect("la série (0, 0) existe dans le décor")
        .line
        .width = 4.0;
    assert!((spec.panels[0].series[0].line.width - 1.5).abs() < f64::EPSILON);
    assert!((modifie.panels[0].series[0].line.width - 4.0).abs() < f64::EPSILON);
    assert_ne!(spec, modifie);
}

#[test]
fn the_symbol_of_a_series_can_be_restyled_without_rebuilding_it() {
    let base = SeriesSpec::from_palette(Y1, 1);
    assert_eq!(base.line.color, DEFAULT_PALETTE[1]);
    assert_eq!(base.symbol.color, DEFAULT_PALETTE[1]);
    let creux = SeriesSpec {
        symbol: SymbolSpec {
            edge_color: "#000000".to_string(),
            ..base.symbol.clone()
        },
        ..base.clone()
    };
    assert_eq!(creux.symbol.effective_edge_color(), "#000000");
    assert_eq!(base.symbol.effective_edge_color(), DEFAULT_PALETTE[1]);
}
