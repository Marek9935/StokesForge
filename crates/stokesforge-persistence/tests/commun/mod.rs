//! Constructions partagées par les tests d'intégration de `stokesforge-persistence`.

#![allow(dead_code)]

use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Project, ValueBlock, WindowMeta,
};
use stokesforge_plot::{
    AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle, PanelSpec, PlotSpec,
    PlotType, SeriesAxis, SeriesSpec, SymbolShape, SymbolSpec,
};

/// Construit le projet de fixture décrit par le critère 1 : dossiers
/// imbriqués, classeurs, feuilles, colonnes de tous les types avec cas
/// limites, fenêtres non triviales.
pub fn fixture_project() -> Project {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();

    let manips = project.add_folder(root, "Manips").expect("dossier créé");
    project
        .add_folder(manips, "Sous-dossier")
        .expect("sous-dossier créé");

    let book_in_manips = project
        .add_empty_workbook(manips, Some("BookManips"))
        .expect("classeur créé");
    let book_at_root = project
        .add_empty_workbook(root, Some("BookRacine"))
        .expect("classeur créé");

    project
        .set_window_meta(
            book_in_manips,
            WindowMeta {
                label: "Étiquette du classeur".to_string(),
                comment: "Commentaire du classeur".to_string(),
                ..Default::default()
            },
        )
        .expect("métadonnées de fenêtre valides");

    for book in [book_in_manips, book_at_root] {
        let sheet1 = project
            .add_worksheet(book, Some("Sheet1"))
            .expect("feuille créée");
        project
            .add_worksheet(book, Some("Sheet2"))
            .expect("feuille créée");

        let numeric = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0, f64::NAN, 3.0, 4.0, f64::NAN]),
            ColumnMeta {
                role: ColumnRole::X,
                unit: "s".to_string(),
                long_name: "Temps".to_string(),
                comment: "colonne de temps".to_string(),
                x_prime: Some(12.5),
                display_format: "0.000".to_string(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
            },
        )
        .expect("colonne numérique valide");
        project
            .add_column(sheet1, None, Some("A"), numeric)
            .expect("colonne ajoutée");

        let datetime = Column::from_block(
            ColumnType::DateTime,
            ValueBlock::Float(vec![0.0, 86_400.0]),
            ColumnMeta::default(),
        )
        .expect("colonne date-heure valide");
        project
            .add_column(sheet1, None, Some("B"), datetime)
            .expect("colonne ajoutée");

        let text = Column::from_block(
            ColumnType::Mixed,
            ValueBlock::Mixed(vec![
                MixedCell::Text(String::new()),
                MixedCell::Empty,
                MixedCell::Text("é ü 日本".to_string()),
            ]),
            ColumnMeta::default(),
        )
        .expect("colonne texte valide");
        project
            .add_column(sheet1, None, Some("C"), text)
            .expect("colonne ajoutée");

        let empty = Column::new(ColumnType::Numeric);
        project
            .add_column(sheet1, None, Some("D"), empty)
            .expect("colonne vide ajoutée");

        // Colonne à types mixtes : les trois variantes de cellule, plus les
        // trois « manquantes » qui doivent rester distinctes au disque
        // (`Empty`, `Number(NaN)`, `Text("")`), plus un `-0.0` — dont le
        // motif binaire ne survit qu'à un encodage brut.
        let mixed = Column::from_block(
            ColumnType::Mixed,
            ValueBlock::Mixed(vec![
                MixedCell::Number(1.5),
                MixedCell::Text("n/a".to_string()),
                MixedCell::Empty,
                MixedCell::Number(f64::NAN),
                MixedCell::Text(String::new()),
                MixedCell::Number(-0.0),
                MixedCell::Text("é ü 日本".to_string()),
            ]),
            ColumnMeta {
                role: ColumnRole::Y,
                unit: "V".to_string(),
                long_name: "Tension mesurée".to_string(),
                comment: "avec des n/a".to_string(),
                x_prime: None,
                display_format: String::new(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
            },
        )
        .expect("colonne mixte valide");
        project
            .add_column(sheet1, None, Some("E"), mixed)
            .expect("colonne mixte ajoutée");

        // `Sheet2` reste une feuille vide (aucune colonne).
        let _ = sheet1;
    }

    let graph = project
        .add_graph(root, Some("Graph1"), Default::default())
        .expect("graphe créé");
    project
        .set_window_meta(
            graph,
            WindowMeta {
                label: "Mon graphe".to_string(),
                comment: String::new(),
                ..Default::default()
            },
        )
        .expect("métadonnées de fenêtre valides");

    project
        .add_matrix(root, Some("MBook1"))
        .expect("matrice créée");

    project
        .add_note(root, Some("Note1"), "Texte de la note, avec accents éàü.")
        .expect("note créée");

    project
}

/// Un `PlotSpec` non trivial : deux panneaux, plusieurs séries, des styles
/// qui ne sont **pas** ceux par défaut — pour que l'égalité de la tâche 3.5
/// ne puisse pas passer par accident sur des champs restés à leur valeur par
/// défaut des deux côtés du disque.
pub fn non_trivial_spec(
    x: stokesforge_core::NodeId,
    y1: stokesforge_core::NodeId,
    y2: stokesforge_core::NodeId,
) -> PlotSpec {
    let scatter = SeriesSpec {
        name: "Brute".to_string(),
        plot_type: PlotType::Scatter,
        line: LineSpec {
            color: "#112233".to_string(),
            style: LineStyle::Dash,
            width: 3.5,
        },
        symbol: SymbolSpec {
            shape: SymbolShape::Diamond,
            size: 9.0,
            color: "#445566".to_string(),
            edge_color: "#000000".to_string(),
        },
        axis: SeriesAxis::Right,
        visible: false,
        show_in_legend: false,
        bar_width: Some(0.42),
        // Série « ligne » (#166) : la ligne 4 de la feuille de `y1`.
        row: Some(4),
        // Groupe de courbes (#177).
        group: Some(3),
        ..SeriesSpec::new_xy(x, y1)
    };
    let histogram = SeriesSpec {
        name: String::new(),
        plot_type: PlotType::Histogram,
        histogram_bins: stokesforge_plot::BinCount::new(17).expect("17 est dans les bornes"),
        ..SeriesSpec::new(y2)
    };

    let panel_a = PanelSpec {
        title: "Signal".to_string(),
        series: vec![scatter],
        bottom: AxisSpec {
            label: "Temps".to_string(),
            unit: "s".to_string(),
            scale: AxisScale::Log,
            auto_range: false,
            range_min: Some(-1.0),
            range_max: Some(10.0),
            grid: true,
            visible: true,
            inverted: true,
            title_color: "#aa3311".to_string(),
            // Issue #53 : textes déplacés, une ancre par sorte de texte.
            title_visible: true,
            title_anchor: Some(stokesforge_plot::TextAnchor::new(3.5, -0.75)),
            // Issues #77 et #78 : police et corps, un jeu par sorte de texte.
            title_font: stokesforge_plot::TextFont {
                family: "Fira Sans".to_string(),
                size: Some(18.0),
                width: Some(0.4),
                align: stokesforge_plot::TextAlign::Right,
            },
            tick_color: "#fefefe".to_string(),
            // #176 : axe miroir, sans étiquettes de graduation.
            mirror: true,
            tick_labels: false,
        },
        title_visible: true,
        title_anchor: Some(stokesforge_plot::TextAnchor {
            axis: SeriesAxis::Right,
            ..stokesforge_plot::TextAnchor::new(0.5, 4.25)
        }),
        legend: LegendSpec {
            visible: true,
            position: LegendPosition::BottomLeft,
            anchor: Some(stokesforge_plot::TextAnchor::new(-2.0, 8.0)),
            font: stokesforge_plot::TextFont {
                family: "Libertinus Sans".to_string(),
                size: Some(11.0),
                ..Default::default()
            },
            color: "#118899".to_string(),
        },
        title_color: "#aa8811".to_string(),
        // Issue #35 : textes libres, tous champs non défaut.
        annotations: vec![
            stokesforge_plot::AnnotationSpec::new("Pic $lambda_\"max\"$", 1.5, 2.5),
            stokesforge_plot::AnnotationSpec {
                axis: SeriesAxis::Right,
                color: "#11223344".to_string(),
                size: 22.0,
                rotation: -30.0,
                font: "Fira Sans".to_string(),
                visible: false,
                width: Some(0.3),
                align: stokesforge_plot::TextAlign::Right,
                ..stokesforge_plot::AnnotationSpec::new("*seuil*", -0.25, 1e-3)
            },
        ],
        // Issue #76 : objets graphiques, un de chaque sorte, dans l'ordre.
        shapes: vec![
            stokesforge_plot::ShapeSpec {
                visible: false,
                axis: SeriesAxis::Right,
                line_color: "#aa221188".to_string(),
                line_width: 3.0,
                fill_color: "#33669955".to_string(),
                ..stokesforge_plot::ShapeSpec::new(
                    stokesforge_plot::ShapeKind::Rectangle,
                    0.5,
                    1.0,
                    2.5,
                    3.0,
                )
            },
            stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Ellipse,
                -1.0,
                -2.0,
                4.0,
                6.0,
            ),
            stokesforge_plot::ShapeSpec::new(stokesforge_plot::ShapeKind::Line, 0.0, 0.0, 1.0, 1.0),
            stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Arrow,
                2.0,
                2.0,
                1.0,
                0.5,
            ),
        ],
        // Nom du groupe 3 (#197).
        group_names: [(3, "Séries d'été".to_string())].into(),
        // Nuancier retenu par le groupe 3 (#213).
        group_gradients: [(3, vec!["#0000ff".to_string(), "#ff0000".to_string()])].into(),
        ..PanelSpec::at(0, 0)
    };
    let panel_b = PanelSpec {
        title: "Répartition".to_string(),
        series: vec![histogram],
        legend: LegendSpec {
            visible: false,
            position: LegendPosition::TopLeft,
            anchor: None,
            font: stokesforge_plot::TextFont::default(),
            // Issue #90 : couleur de la légende et du titre du panneau.
            color: "#228833".to_string(),
        },
        title_font: stokesforge_plot::TextFont {
            family: "Noto Sans".to_string(),
            size: None,
            ..Default::default()
        },
        title_color: "#8833aa".to_string(),
        ..PanelSpec::at(1, 0)
    };

    PlotSpec {
        background: "#0a0a0a".to_string(),
        title_color: "#ffcc00".to_string(),
        title_visible: true,
        title_anchor: Some(stokesforge_plot::TextAnchor::new(1.0, 9.5)),
        title_font: stokesforge_plot::TextFont {
            family: "Libertinus Sans".to_string(),
            size: Some(30.0),
            ..Default::default()
        },
        ..PlotSpec::empty("Mesure du 11/09").with_panels(vec![panel_a, panel_b])
    }
}

/// Une recette dont chaque champ s'écarte de sa valeur par défaut : un champ
/// oublié par le miroir du fichier reviendrait à sa valeur par défaut et
/// ferait échouer l'égalité (S2, #323).
pub fn recipe_hors_defaut() -> stokesforge_import::recipe::ImportRecipe {
    use stokesforge_import::recipe::*;
    ImportRecipe {
        name: "Spectro".into(),
        separator: Separator::FixedWidth(vec![4, 9, 15]),
        decimal: DecimalStyle::Comma,
        header: HeaderSkip::UntilMarker("[DATA]".into()),
        footer_skip_lines: 3,
        has_column_header_row: false,
        name_source: NameSource::HeaderKey("TITLE".into()),
        header_layout: HeaderLayout {
            main_header_lines: 2,
            subheader_lines: 4,
            auto_detect: true,
        },
        subheader_roles: SubheaderRoles {
            long_name: Some(SubheaderLineRange { from: 1, to: 2 }),
            unit: Some(SubheaderLineRange::single(3)),
            comment: Some(SubheaderLineRange { from: 4, to: 5 }),
            x_prime: Some(SubheaderLineRange::single(6)),
            same_line_name_unit: true,
        },
        skip_chars: 5,
        preview_lines: 33,
        data_end: DataEnd::Marker("END".into()),
        keep_header_metadata: false,
        keep_footer_metadata: true,
        excluded_metadata: vec!["SERIAL".into(), "DATE".into()],
        column_roles: vec![ColumnRoleChoice::X, ColumnRoleChoice::YError],
        thousands: Some('\''),
        date_order: DateOrder::MonthFirst,
        missing_tokens: vec!["n/a".into()],
        comment_prefix: "#".into(),
        skip_data_rows: 7,
        encoding: Encoding::Utf16Be,
        auto_match: AutoMatch {
            extensions: vec!["asc".into()],
            name_contains: "sp".into(),
            content_contains: "JASCO".into(),
        },
        column_kinds: vec![ColumnKind::Text, ColumnKind::DateTime],
        import_mode: ImportMode::AddRows,
        files_layout: FilesLayout::SharedX,
        block_split: BlockSplit::BeforeLine("XYDATA".into()),
    }
}
