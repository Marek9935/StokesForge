//! L'aperçu et l'import doivent découper un fichier de la même façon : mêmes
//! colonnes, mêmes valeurs. Sans forcer le séparateur — c'est `Auto` qui est
//! en cause (issue #23).

use plotyx_import::{parse, preview, DecimalStyle, HeaderSkip, ImportRecipe, LineRole, Separator};

/// Nombre de cellules de la première ligne de données de l'aperçu, et
/// valeurs de la première colonne lues par l'import.
fn assert_same_split(bytes: &[u8], recipe: &ImportRecipe) {
    let table = parse(bytes, recipe).expect("import");
    let shown = preview(bytes, recipe).expect("aperçu");
    for line in shown.lines.iter().filter(|l| l.role == LineRole::Data) {
        let cells = line.cells.as_ref().expect("cellules de données");
        assert_eq!(
            cells.len(),
            table.columns.len(),
            "ligne {} : l'aperçu montre {} cellules, l'import {} colonnes ({:?})",
            line.number,
            cells.len(),
            table.columns.len(),
            cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>()
        );
        assert!(cells.iter().all(|c| !c.is_text), "ligne {}", line.number);
    }
}

#[test]
fn tabulation_et_virgule_decimale_avec_ligne_de_noms() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.has_column_header_row = true;
    recipe.decimal = DecimalStyle::Comma;
    let bytes = b"Temps\tSignal\n1,5\t2,5\n3,5\t4,5\n";
    assert_same_split(bytes, &recipe);
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values[0], 1.5);
    assert_eq!(table.columns[1].values[0], 2.5);
}

#[test]
fn memes_colonnes_sans_ligne_de_noms() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.decimal = DecimalStyle::Comma;
    assert_same_split(b"1,5\t2,5\n3,5\t4,5\n", &recipe);
}

#[test]
fn avec_saut_d_en_tete_et_prefixe() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.decimal = DecimalStyle::Comma;
    recipe.has_column_header_row = true;
    recipe.header = HeaderSkip::Lines(2);
    recipe.skip_chars = 2;
    assert_same_split(b"titre\nautre\n>>t\ts\n>>1,5\t2,5\n>>3,5\t4,5\n", &recipe);
    recipe.header = HeaderSkip::None;
    recipe.header_layout.main_header_lines = 2;
    assert_same_split(b"titre\nautre\n>>t\ts\n>>1,5\t2,5\n>>3,5\t4,5\n", &recipe);
}

#[test]
fn separateurs_explicites_inchanges() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    assert_same_split(b"1,5;2,5\n3,5;4,5\n", &recipe);
}
