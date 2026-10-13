//! Textes libres (issue #35) : ancrés aux données, ils suivent le zoom, et
//! un texte hors du cadrage visible n'est pas dessiné.

use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{AnnotationSpec, AxisScale, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::formula::FormulaCache;
use stokesforge_render::{FontChoice, PanelTransform, TextMeasurer};
use stokesforge_render_egui::{collect_shapes_with_typst, DrawnPlot, ScreenText, TypstScreen};

fn projet() -> (Project, PanelSpec) {
    let colonne = |values: Vec<f64>| {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("colonne")
    };
    let mut project = Project::new("Annotations").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let xs: Vec<f64> = (1..=10).map(f64::from).collect();
    let x = project
        .add_column(sheet, None, Some("X"), colonne(xs.clone()))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), colonne(xs))
        .expect("Y");
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(x, y));
    panel
        .annotations
        .push(AnnotationSpec::new("Pic $lambda$", 5.0, 5.0));
    (project, panel)
}

fn trace(project: &Project, panel: PanelSpec) -> DrawnPlot {
    let ctx = egui::Context::default();
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
    let mut shapes = Vec::new();
    let mut drawn = None;
    let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("polices");
        drawn =
            Some(collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("tracé"));
    });
    sortie.textures_delta.clear();
    drawn.expect("figure")
}

/// Le centre tracé est la projection du point de données, par la même
/// transformation que les séries — avant et après un zoom.
#[test]
fn un_texte_suit_le_zoom() {
    let (project, panel) = projet();
    let centre = |panel: PanelSpec| {
        let drawn = trace(&project, panel);
        let drawn_panel = &drawn.panels[0];
        let annotation = drawn_panel.annotations.first().expect("texte tracé");
        let attendu = PanelTransform::of(drawn_panel)
            .and_then(|t| t.data_to_pixel(5.0, 5.0))
            .expect("projection");
        // À 1,5 px près : le texte est projeté par Plotters, comme les
        // points des séries, et `PanelTransform` (la désignation) arrondit
        // les bords de la zone d'un pixel près.
        assert!(
            (annotation.center.0 - attendu.0).abs() <= 1.5
                && (annotation.center.1 - attendu.1).abs() <= 1.5,
            "{:?} au lieu de {attendu:?}",
            annotation.center
        );
        assert!(annotation.size.0 > 0 && annotation.size.1 > 0);
        annotation.center
    };
    let avant = centre(panel.clone());
    // Ce que `ViewState::apply` pose pour un zoom sur 4..6.
    let mut zoome = panel;
    zoome.bottom.auto_range = false;
    zoome.bottom.range_min = Some(4.0);
    zoome.bottom.range_max = Some(6.0);
    let apres = centre(zoome);
    assert!(
        (avant.0 - apres.0).abs() > 10.0,
        "le texte n'a pas suivi le zoom"
    );
}

/// Hors du cadrage (zoom ailleurs), un texte libre reste tracé — il peut
/// sortir de la zone de tracé, dans les marges (#163) —, mais à une valeur
/// ≤ 0 sur un axe log l'échelle ne le représente pas : pas dessiné.
#[test]
fn un_texte_hors_du_cadrage_reste_dessine_mais_pas_hors_echelle() {
    let (project, panel) = projet();
    let mut ailleurs = panel.clone();
    ailleurs.bottom.auto_range = false;
    ailleurs.bottom.range_min = Some(7.0);
    ailleurs.bottom.range_max = Some(9.0);
    assert_eq!(trace(&project, ailleurs).panels[0].annotations.len(), 1);

    let mut log = panel;
    log.left.scale = AxisScale::Log;
    log.annotations[0].y = -1.0;
    assert!(trace(&project, log).panels[0].annotations.is_empty());
}

/// #163 : un texte posé dans les marges est **dessiné** là où il est
/// désigné. Plotters rogne chaque point sur le rectangle de sa zone
/// (`BackendCoordOnly`) : tracé sur la zone de tracé, le texte restait collé
/// à son bord alors que sa boîte de sélection, elle, sortait. Le dernier
/// `Shape` est le texte (tracé en dernier) ; son centre doit être le centre
/// désigné.
#[test]
fn un_texte_dans_les_marges_est_dessine_la_ou_il_est_designe() {
    let (project, panel) = projet();
    for y in [0.0, -1.0] {
        let mut p = panel.clone();
        p.annotations[0].text = "test".into();
        p.annotations[0].y = y;
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render::font_definitions());
        let measurer = TextMeasurer::default();
        let choice = FontChoice::default();
        let cache: FormulaCache<_> = FormulaCache::new();
        let spec = PlotSpec::empty("").with_panels(Panels::new(vec![p]));
        let plot = resolve_spec(project.tree(), &spec);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
        let mut shapes = Vec::new();
        let mut drawn = None;
        let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
            let text = ScreenText::new(&ctx, &measurer);
            let typst = TypstScreen::new(&ctx, &cache, &choice).expect("polices");
            drawn = Some(
                collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("tracé"),
            );
        });
        sortie.textures_delta.clear();
        let centre = drawn.expect("figure").panels[0].annotations[0].center;
        let boite = shapes.last().expect("une forme").visual_bounding_rect();
        assert!(
            (boite.center().y as f64 - centre.1).abs() <= 2.0,
            "y={y} : texte dessiné en {:?}, désigné en {centre:?}",
            boite.center()
        );
    }
}

/// Largeur fixée (#244) : la boîte tracée fait la fraction demandée de la
/// figure (480 px ici), le texte long passe à la ligne (boîte plus haute),
/// et sans largeur le texte reste sur une ligne.
#[test]
fn une_largeur_fixee_fait_passer_le_texte_a_la_ligne() {
    let (project, mut panel) = projet();
    let long = "Un texte bien trop long pour tenir sur une seule ligne dans la boîte";
    panel.annotations[0].text = long.to_owned();
    let libre = trace(&project, panel.clone()).panels[0].annotations[0];
    panel.annotations[0].width = Some(0.25);
    let boite = trace(&project, panel.clone()).panels[0].annotations[0];
    assert!(
        (i64::from(boite.size.0) - 120).abs() <= 2,
        "largeur {:?}",
        boite.size
    );
    assert!(boite.size.1 > libre.size.1, "{boite:?} contre {libre:?}");
    // Le plafond est la figure.
    panel.annotations[0].width = Some(7.0);
    let plafonnee = trace(&project, panel).panels[0].annotations[0];
    assert!(plafonnee.size.0 <= 482, "{plafonnee:?}");
}

/// Largeur d'une zone de titre (#244) : fixée, elle fait passer un long titre
/// d'axe à la ligne — texte tracé plus étroit et plus haut.
#[test]
fn la_largeur_d_un_titre_d_axe_le_fait_passer_a_la_ligne() {
    use stokesforge_plot::{AxisSide, TextFont, TextTarget};
    let (project, mut panel) = projet();
    panel.bottom.label =
        "Un titre d'axe vraiment beaucoup trop long pour une seule ligne".to_owned();
    let cible = TextTarget::AxisTitle {
        panel: 0,
        side: AxisSide::Bottom,
    };
    let taille = |panel: PanelSpec| {
        trace(&project, panel).panels[0]
            .texts
            .iter()
            .find(|t| t.target == cible)
            .expect("titre tracé")
            .size
    };
    let libre = taille(panel.clone());
    panel.bottom.title_font = TextFont {
        width: Some(0.2),
        ..TextFont::default()
    };
    let boite = taille(panel);
    assert!(
        boite.0 < libre.0 && boite.1 > libre.1,
        "{boite:?} / {libre:?}"
    );
}
