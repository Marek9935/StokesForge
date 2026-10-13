//! L'aperçu d'analyse reprend les primitives de style partagées (#210).
use egui::{pos2, vec2, Rect};
use stokesforge_core::NodeId;
use stokesforge_plot::{LineStyle, PlotType, SeriesSpec, SymbolShape};
use stokesforge_render_egui::analysis_curve;

#[test]
fn apercu_distingue_traits_marqueurs_et_ne_vide_pas_les_barres() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0));
    let points = [(5.0, 60.0), (50.0, 10.0), (95.0, 30.0)];
    let mut spec = SeriesSpec::new(NodeId::from_raw(1));
    let solid = analysis_curve(&spec, rect, &points);
    assert!(!solid.is_empty());
    spec.line.style = LineStyle::Dash;
    let dashed = analysis_curve(&spec, rect, &points);
    assert_ne!(
        solid, dashed,
        "témoin : le style source est réellement utilisé"
    );
    spec.plot_type = PlotType::Scatter;
    spec.symbol.shape = SymbolShape::Circle;
    let symbols = analysis_curve(&spec, rect, &points);
    assert!(!symbols.is_empty());
    assert_ne!(symbols, dashed);
    for kind in [PlotType::Bar, PlotType::Histogram] {
        spec.plot_type = kind;
        spec.line.style = LineStyle::None;
        assert!(
            !analysis_curve(&spec, rect, &points).is_empty(),
            "les Y bruts restent visibles"
        );
    }
}
