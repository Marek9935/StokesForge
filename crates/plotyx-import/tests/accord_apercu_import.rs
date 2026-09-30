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

/// `DecimalStyle::Auto` (issue #7) : la virgule est détectée avec un autre
/// séparateur, le point sinon ; l'aperçu et l'import s'accordent.
#[test]
fn decimale_automatique() {
    let mut recipe = ImportRecipe::new("Auto");
    recipe.decimal = DecimalStyle::Auto;
    let comma = b"1,5\t2,5\n3,5\t4,5\n";
    assert_same_split(comma, &recipe);
    assert_eq!(parse(comma, &recipe).unwrap().columns[0].values[0], 1.5);
    let dot = b"1.5,2.5\n3.5,4.5\n";
    assert_same_split(dot, &recipe);
    assert_eq!(parse(dot, &recipe).unwrap().columns[1].values[0], 2.5);
    let integers = b"1,2,3\n4,5,6\n";
    assert_eq!(parse(integers, &recipe).unwrap().columns.len(), 3);
}

/// Les jetons de valeur manquante laissent la colonne numérique ; le
/// séparateur de milliers est ignoré.
#[test]
fn valeurs_manquantes_et_milliers() {
    let mut recipe = ImportRecipe::new("Manquantes");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.thousands = Some(' ');
    recipe.missing_tokens = vec!["--".into(), "n/a".into()];
    let bytes = b"1 000,5;--\n2 000,5;n/a\n3;4\n";
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].values, vec![1000.5, 2000.5, 3.0]);
    assert!(table.columns[1].values[0].is_nan());
    assert!(
        table.columns[1].texts.iter().all(Option::is_none),
        "les manquantes ne sont pas du texte"
    );
    assert_same_split(bytes, &recipe);
    recipe.missing_tokens.clear();
    let with_text = parse(bytes, &recipe).unwrap();
    assert!(with_text.columns[1].texts.iter().any(Option::is_some));
}

/// Séparateur au choix et colonnes à largeur fixe (issue #7).
#[test]
fn separateur_personnalise_et_largeur_fixe() {
    let mut recipe = ImportRecipe::new("Barre");
    recipe.separator = Separator::Custom('|');
    let bytes = b"1|10|100\n2|20|200\n";
    assert_same_split(bytes, &recipe);
    assert_eq!(parse(bytes, &recipe).unwrap().columns.len(), 3);

    let mut fixed = ImportRecipe::new("Fixe");
    fixed.separator = Separator::FixedWidth(vec![4, 6]);
    // 4 + 6 + reste : « 1.5 », « 20.25 », « 7 ».
    let bytes = b"1.5 20.25 7\n2.5 30.75 8\n";
    assert_same_split(bytes, &fixed);
    let table = parse(bytes, &fixed).unwrap();
    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].values, vec![1.5, 2.5]);
    assert_eq!(table.columns[1].values, vec![20.25, 30.75]);
    assert_eq!(table.columns[2].values, vec![7.0, 8.0]);
}

/// Lignes de commentaire : ignorées à l'import, montrées par l'aperçu avec
/// leur propre rôle, sans décaler les données ; et plage de lignes.
#[test]
fn commentaires_et_plage_de_lignes() {
    let mut recipe = ImportRecipe::new("Commentaires");
    recipe.separator = Separator::Comma;
    recipe.comment_prefix = "#".into();
    let bytes = b"# mesure lundi\n1,10\n# recalibrage\n2,20\n3,30\n4,40\n";
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].values, vec![1.0, 2.0, 3.0, 4.0]);
    assert_same_split(bytes, &recipe);
    let shown = preview(bytes, &recipe).unwrap();
    let comments = shown
        .lines
        .iter()
        .filter(|l| l.role == LineRole::Comment)
        .count();
    assert_eq!(comments, 2);
    assert_eq!(shown.total_data_lines, 4);

    // Les deux premières lignes de données sont ignorées ; l'aperçu les marque.
    recipe.skip_data_rows = 2;
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].values, vec![3.0, 4.0]);
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(
        shown
            .lines
            .iter()
            .filter(|l| l.role == LineRole::Skipped)
            .count(),
        2
    );
    assert_eq!(shown.total_data_lines, 2);
    // Tout ignorer : plus aucune donnée.
    recipe.skip_data_rows = 10;
    assert!(parse(bytes, &recipe).is_err());
    assert!(preview(bytes, &recipe).is_err());
}

/// Un encodage forcé s'applique à l'import comme à l'aperçu.
#[test]
fn encodage_force() {
    let mut recipe = ImportRecipe::new("Latin");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.has_column_header_row = true;
    // « T°C;Signal » en Windows-1252 : l'octet 0xB0 est invalide en UTF-8.
    let bytes: &[u8] = b"T\xB0C;Signal\n1,5;2,5\n";
    recipe.encoding = plotyx_import::Encoding::Latin1;
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("T°C"));
    assert_same_split(bytes, &recipe);
    recipe.encoding = plotyx_import::Encoding::Utf8;
    assert_eq!(
        parse(bytes, &recipe).unwrap().columns[0].name.as_deref(),
        Some("T\u{FFFD}C"),
        "UTF-8 forcé : l'octet invalide ne fait pas échouer l'import"
    );
}
