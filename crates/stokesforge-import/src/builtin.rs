//! Templates provided out of the box, based on the device files in
//! `test/import/`: on first launch, the template list is not empty.
//! Each one is checked against its fixture (`tests/builtin.rs`).

use crate::recipe::{
    AutoMatch, DataEnd, DecimalStyle, HeaderSkip, ImportRecipe, NameSource, Separator,
};

/// The provided templates. The user can save a modified version of one under
/// the same name: theirs takes precedence.
pub fn builtin_recipes() -> Vec<ImportRecipe> {
    let mut jasco_semicolon = ImportRecipe::new("Jasco (point-virgule)");
    jasco_semicolon.separator = Separator::Semicolon;
    jasco_semicolon.decimal = DecimalStyle::Auto;
    jasco_semicolon.header = HeaderSkip::UntilMarker("XYDATA".into());
    jasco_semicolon.name_source = NameSource::HeaderKey("TITLE".into());
    // The "Extended Information" footer must be cut: cutting is no longer
    // the default, so it is requested here.
    jasco_semicolon.data_end = DataEnd::Auto;
    jasco_semicolon.auto_match = AutoMatch {
        extensions: vec!["csv".into()],
        name_contains: String::new(),
        content_contains: "ORIGIN;JASCO".into(),
    };

    let mut jasco_tab = ImportRecipe::new("Jasco (tabulation)");
    jasco_tab.separator = Separator::Tab;
    jasco_tab.decimal = DecimalStyle::Auto;
    jasco_tab.header = HeaderSkip::UntilMarker("XYDATA".into());
    jasco_tab.name_source = NameSource::HeaderKey("TITLE".into());
    jasco_tab.data_end = DataEnd::Auto;
    jasco_tab.auto_match = AutoMatch {
        extensions: vec!["txt".into()],
        name_contains: String::new(),
        content_contains: "ORIGIN\tJASCO".into(),
    };

    let mut analytik = ImportRecipe::new("AnalytikJena (sans en-tête)");
    analytik.separator = Separator::Semicolon;
    analytik.decimal = DecimalStyle::Comma;

    let mut rpe = ImportRecipe::new("RPE (tabulation)");
    rpe.separator = Separator::Tab;
    rpe.decimal = DecimalStyle::Dot;
    rpe.auto_match = AutoMatch {
        extensions: vec!["asc".into()],
        name_contains: String::new(),
        content_contains: String::new(),
    };

    let mut generic = ImportRecipe::new("CSV avec noms de colonnes");
    generic.separator = Separator::Auto;
    generic.decimal = DecimalStyle::Auto;
    generic.has_column_header_row = true;

    vec![jasco_semicolon, jasco_tab, analytik, rpe, generic]
}
