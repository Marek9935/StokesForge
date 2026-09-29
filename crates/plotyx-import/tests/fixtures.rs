//! Tests d'intégration contre les fichiers réels de `test/import/` à la
//! racine du dépôt (fixtures Jasco, AnalytikJena, RPE, JCAMP-like, matrice
//! large, binaire propriétaire…).

use plotyx_import::{DecimalStyle, HeaderSkip, ImportError, ImportRecipe, NameSource, Separator};

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("lecture de {path:?} impossible : {e}"))
}

#[test]
fn jasco_uvvis_semicolon() {
    let bytes = fixture("jasco_uvvis_semicolon.csv");
    let mut recipe = ImportRecipe::new("Jasco UV-Vis");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.has_column_header_row = false;
    recipe.name_source = NameSource::HeaderKey("TITLE".into());

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.row_count(), 6);
    assert_eq!(table.columns.len(), 2);
    assert_eq!(
        plotyx_import::resolve_series_name(&recipe, &table, "jasco_uvvis_semicolon"),
        "Fixture_Jasco_UVVis"
    );
}

#[test]
fn jasco_fluo_tab() {
    let bytes = fixture("jasco_fluo_tab.txt");
    let mut recipe = ImportRecipe::new("Jasco Fluo");
    recipe.separator = Separator::Tab;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.name_source = NameSource::HeaderKey("TITLE".into());

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.row_count(), 6);
    assert_eq!(
        plotyx_import::resolve_series_name(&recipe, &table, "jasco_fluo_tab"),
        "Fixture_Jasco_Fluo"
    );
}

#[test]
fn jasco_english_locale_dot() {
    let bytes = fixture("jasco_english_locale_dot.csv");
    let mut recipe = ImportRecipe::new("Jasco English locale");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Dot;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.name_source = NameSource::HeaderKey("TITLE".into());

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.row_count(), 6);
    assert_eq!(
        plotyx_import::resolve_series_name(&recipe, &table, "jasco_english_locale_dot"),
        "Fixture_Jasco_EnglishLocale"
    );
    assert_eq!(table.columns[0].values[0], 600.0);
}

#[test]
fn jasco_npoints_mismatch_ne_bloque_pas() {
    let bytes = fixture("jasco_npoints_mismatch.txt");
    let mut recipe = ImportRecipe::new("Jasco NPOINTS mismatch");
    recipe.separator = Separator::Tab;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());

    let table = plotyx_import::parse(&bytes, &recipe).expect("le parsing réussit malgré tout");
    assert_eq!(table.row_count(), 4);
    assert!(
        table
            .warnings
            .iter()
            .any(|w| w.contains("99") && w.contains('4')),
        "attendu un avertissement mentionnant 99 vs 4, obtenu : {:?}",
        table.warnings
    );
}

#[test]
fn analytikjena_nohead_semicolon() {
    let bytes = fixture("analytikjena_nohead_semicolon.csv");
    let mut recipe = ImportRecipe::new("AnalytikJena");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::None;
    recipe.has_column_header_row = false;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.row_count(), 20);
}

#[test]
fn rpe_nohead_tab() {
    let bytes = fixture("rpe_nohead_tab.asc");
    let mut recipe = ImportRecipe::new("RPE");
    recipe.separator = Separator::Tab;
    recipe.decimal = DecimalStyle::Dot;
    recipe.header = HeaderSkip::None;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.row_count(), 20);
    assert_eq!(table.columns[1].values[0], -1.2);
}

#[test]
fn generic_named_header() {
    let bytes = fixture("generic_named_header.csv");
    let mut recipe = ImportRecipe::new("Générique");
    recipe.separator = Separator::Comma;
    recipe.decimal = DecimalStyle::Dot;
    recipe.header = HeaderSkip::None;
    recipe.has_column_header_row = true;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns[0].name, Some("Time".to_string()));
    assert_eq!(table.columns[1].name, Some("Signal".to_string()));
    assert_eq!(table.columns[0].values[0], 0.0);
    assert_eq!(table.columns[1].values[0], 0.0);
}

#[test]
fn generic_named_header_avec_separateur_auto() {
    let bytes = fixture("generic_named_header.csv");
    let mut recipe = ImportRecipe::new("Générique auto");
    recipe.separator = Separator::Auto;
    recipe.decimal = DecimalStyle::Dot;
    recipe.header = HeaderSkip::None;
    recipe.has_column_header_row = true;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns[0].name, Some("Time".to_string()));
    assert_eq!(table.columns[1].name, Some("Signal".to_string()));
}

#[test]
fn ir_jcamp_lookalike() {
    let bytes = fixture("ir_jcamp_lookalike.txt");
    let mut recipe = ImportRecipe::new("IR JCAMP-like");
    // Le fichier a 4 lignes de métadonnées `##CLÉ=VALEUR` avant les
    // données (##TITLE, ##DATA TYPE, ##XUNITS, ##YUNITS).
    recipe.header = HeaderSkip::Lines(4);
    recipe.separator = Separator::Whitespace;
    recipe.decimal = DecimalStyle::Dot;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values[0], 401.19);
    assert_eq!(table.columns[1].values[0], 91.38);
}

#[test]
fn irradiation_wide_matrix() {
    let bytes = fixture("irradiation_wide_matrix.txt");
    let mut recipe = ImportRecipe::new("Matrice large");
    recipe.separator = Separator::Tab;
    recipe.decimal = DecimalStyle::Dot;
    recipe.header = HeaderSkip::None;
    recipe.has_column_header_row = true;

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns.len(), 31);
    assert_eq!(table.columns[0].name, Some("wavelength".to_string()));

    let total_lines = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(table.row_count(), total_lines - 1);
}

#[test]
fn proprietary_binary_rend_not_text_sans_paniquer() {
    let bytes = fixture("proprietary_binary.dat");
    let recipe = ImportRecipe::new("N'importe quoi");
    assert_eq!(
        plotyx_import::parse(&bytes, &recipe),
        Err(ImportError::NotText)
    );
}

#[test]
fn jasco_uvvis_windows_1252_avec_pied_etendu() {
    let bytes = fixture("JascoUV_vis.csv");
    let mut recipe = ImportRecipe::new("Jasco UV-Vis");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.has_column_header_row = false;
    recipe.name_source = NameSource::HeaderKey("TITLE".into());

    let table = plotyx_import::parse(&bytes, &recipe).expect("parsing réussi");
    assert_eq!(table.columns.len(), 2);
    // Les 551 points (NPOINTS) ; le pied « Extended Information » suit, et
    // tombe encore dans les données tant que la fin automatique (issue #4)
    // n'existe pas.
    let (x, y) = (&table.columns[0].values, &table.columns[1].values);
    assert_eq!((x[0], y[0]), (800.0, -0.000371463));
    assert_eq!((x[550], y[550]), (250.0, 0.219912));
    assert!(x[..551].iter().chain(&y[..551]).all(|v| v.is_finite()));
    assert_eq!(
        plotyx_import::resolve_series_name(&recipe, &table, "JascoUV_vis"),
        "S13"
    );
    // L'octet 0xB0 du pied de page, lu en Windows-1252, redonne « °C ».
    assert!(table.columns[1]
        .texts
        .iter()
        .any(|t| t.as_deref() == Some("19.91 °C")));
}
