//! Modèles fournis d'office, d'après les fichiers d'appareils de
//! `test/import/` : au premier lancement, la liste des modèles n'est pas vide.
//! Chacun est vérifié contre sa fixture (`tests/builtin.rs`).

use crate::recipe::{
    AutoMatch, DataEnd, DecimalStyle, HeaderSkip, ImportRecipe, NameSource, Separator,
};

/// Les modèles fournis. L'utilisateur peut en enregistrer une version
/// modifiée sous le même nom : la sienne prend le dessus.
pub fn builtin_recipes() -> Vec<ImportRecipe> {
    let mut jasco_semicolon = ImportRecipe::new("Jasco (point-virgule)");
    jasco_semicolon.separator = Separator::Semicolon;
    jasco_semicolon.decimal = DecimalStyle::Auto;
    jasco_semicolon.header = HeaderSkip::UntilMarker("XYDATA".into());
    jasco_semicolon.name_source = NameSource::HeaderKey("TITLE".into());
    // Le pied « Extended Information » doit être coupé : la coupure n'est plus
    // le défaut, on la demande ici.
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
