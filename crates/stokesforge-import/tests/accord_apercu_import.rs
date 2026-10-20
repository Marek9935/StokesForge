//! L'aperçu et l'import doivent découper un fichier de la même façon : mêmes
//! colonnes, mêmes valeurs. Sans forcer le séparateur — c'est `Auto` qui est
//! en cause (issue #23).

use stokesforge_import::{
    detect_header_layout, parse, preview, DataEnd, DecimalStyle, HeaderSkip, ImportRecipe,
    LineRole, Separator,
};

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
    recipe.encoding = stokesforge_import::Encoding::Latin1;
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("T°C"));
    assert_same_split(bytes, &recipe);
    recipe.encoding = stokesforge_import::Encoding::Utf8;
    assert_eq!(
        parse(bytes, &recipe).unwrap().columns[0].name.as_deref(),
        Some("T\u{FFFD}C"),
        "UTF-8 forcé : l'octet invalide ne fait pas échouer l'import"
    );
}

/// Nature forcée d'une colonne (issue #7) : texte (même un nombre reste du
/// texte), date-heure (timestamps POSIX, une cellule illisible reste du texte).
#[test]
fn colonnes_texte_et_date_heure() {
    use stokesforge_import::ColumnKind;
    let mut recipe = ImportRecipe::new("Natures");
    recipe.separator = Separator::Semicolon;
    recipe.column_kinds = vec![ColumnKind::Text, ColumnKind::DateTime, ColumnKind::Auto];
    let bytes = b"007;25/11/24 15:17:22;1\nA12;2000-01-01;2\nZ9;pas une date;3\n";
    let table = parse(bytes, &recipe).unwrap();
    // Texte : les jetons non vides sont gardés tels quels, même « 007 ».
    assert_eq!(
        table.columns[0].texts,
        vec![
            Some("007".to_string()),
            Some("A12".to_string()),
            Some("Z9".to_string())
        ]
    );
    assert!(table.columns[0].values.iter().all(|v| v.is_nan()));
    // Date-heure : timestamps, la cellule illisible reste du texte.
    assert!(table.columns[1].datetime);
    assert_eq!(table.columns[1].values[0], 1_732_547_842.0);
    assert_eq!(table.columns[1].values[1], 946_684_800.0);
    assert_eq!(table.columns[1].texts[2].as_deref(), Some("pas une date"));
    assert!(!table.columns[2].datetime);
    // L'aperçu annote exactement les mêmes cellules.
    let shown = preview(bytes, &recipe).unwrap();
    let flags: Vec<Vec<bool>> = shown
        .lines
        .iter()
        .filter_map(|l| l.cells.as_ref())
        .map(|cells| cells.iter().map(|c| c.is_text).collect())
        .collect();
    assert_eq!(
        flags,
        vec![
            vec![true, false, false],
            vec![true, false, false],
            vec![true, true, false]
        ]
    );
}

/// Un fichier à plusieurs blocs (plusieurs spectres, chacun avec son en-tête)
/// donne un tableau par bloc ; `parse` rend le premier, l'aperçu en compte le
/// total (issue #7).
#[test]
fn fichier_a_plusieurs_blocs() {
    use stokesforge_import::{parse_blocks, BlockSplit, HeaderSkip};
    let mut recipe = ImportRecipe::new("Multi");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.name_source = stokesforge_import::NameSource::HeaderKey("TITLE".into());
    recipe.block_split = BlockSplit::BeforeLine("TITLE".into());
    let bytes =
        b"TITLE;premier\nXYDATA\n1,5;10\n2,5;20\nTITLE;second\nXYDATA\n3,5;30\n4,5;40\n5,5;50\n";

    let tables = parse_blocks(bytes, &recipe).unwrap();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].columns[0].values, vec![1.5, 2.5]);
    assert_eq!(tables[1].columns[0].values, vec![3.5, 4.5, 5.5]);
    assert_eq!(
        stokesforge_import::resolve_series_name(&recipe, &tables[1], "f"),
        "second"
    );
    assert_eq!(parse(bytes, &recipe).unwrap().row_count(), 2, "le premier");
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(shown.block_count, 2);
    assert_eq!(shown.total_data_lines, 2);

    // Sans découpage : un seul bloc, les « TITLE » suivants sont des données
    // illisibles coupées par la fin automatique.
    recipe.block_split = BlockSplit::None;
    assert_eq!(parse_blocks(bytes, &recipe).unwrap().len(), 1);
    assert_eq!(preview(bytes, &recipe).unwrap().block_count, 1);

    // Un bloc sans donnée est écarté, et sans aucune donnée l'import échoue.
    recipe.block_split = BlockSplit::BlankLines;
    assert!(parse_blocks(b"une\nautre\n", &recipe).is_err());
}

/// La coupure teste le X après retrait du préfixe (compté en caractères),
/// même avec des espaces en tête. Le texte et le vide avant le premier
/// nombre restent des données ; le texte dans Y ne coupe jamais (issue #4).
#[test]
fn fin_automatique_apres_prefixe_et_avant_pied() {
    for (separator, text) in [
        (
            Separator::Semicolon,
            "titre\nλ>texte;avant\nλ>\nλ> 1;10\nλ> 2;n/a\nλ>\nλ>Data points;551\n",
        ),
        (
            Separator::Whitespace,
            "titre\nλ>texte avant\nλ>\nλ>   1   10\nλ>   2   n/a\nλ>\nλ>Data points 551\n",
        ),
    ] {
        let mut recipe = ImportRecipe::new("Préfixe d'instrument");
        recipe.data_end = stokesforge_import::DataEnd::Auto;
        recipe.separator = separator;
        recipe.header = HeaderSkip::Lines(1);
        recipe.skip_chars = 2;
        let table = parse(text.as_bytes(), &recipe).expect("import");
        assert_eq!(table.row_count(), 4);
        assert_eq!(table.columns[0].texts[0].as_deref(), Some("texte"));
        assert!(table.columns[0].values[1].is_nan());
        assert!(table.columns[0].texts[1].is_none());
        assert_eq!(&table.columns[0].values[2..], &[1.0, 2.0]);
        assert_eq!(table.columns[1].texts[3].as_deref(), Some("n/a"));

        let shown = preview(text.as_bytes(), &recipe).expect("aperçu");
        assert_eq!(shown.total_data_lines, 4);
        let footer: Vec<_> = shown
            .lines
            .iter()
            .filter(|line| line.role == LineRole::Footer)
            .collect();
        assert_eq!(
            footer.iter().map(|line| line.number).collect::<Vec<_>>(),
            vec![6, 7]
        );
        assert_eq!(
            footer
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            table.footer_lines()
        );

        // La désactivation survit à l'enregistrement de la recette et garde
        // le pied dans les données : témoin d'une coupure réellement active.
        recipe.data_end = stokesforge_import::DataEnd::None;
        let restored = ImportRecipe::from_json(&recipe.to_json()).expect("recette relue");
        let uncut = parse(text.as_bytes(), &restored).expect("sans coupure");
        assert_eq!(uncut.row_count(), 6);
        assert!(uncut.footer_lines().is_empty());
        let shown = preview(text.as_bytes(), &restored).expect("aperçu sans coupure");
        assert_eq!(shown.total_data_lines, 6);
        assert!(!shown.lines.iter().any(|line| line.role == LineRole::Footer));
    }
}

/// Une mesure manquante en X (« n/a », « - », « NaN ») ne coupe plus le
/// fichier : par défaut aucune coupure, toutes les lignes sont importées et
/// l'aperçu en montre autant. La coupure automatique perdait sans
/// avertissement tout ce qui suivait.
#[test]
fn par_defaut_une_mesure_manquante_en_x_ne_coupe_pas_le_fichier() {
    let text = "1;10\nn/a;20\n3;30\n-;40\n5;50\n";
    let mut recipe = ImportRecipe::new("Défaut");
    recipe.separator = Separator::Semicolon;
    let table = parse(text.as_bytes(), &recipe).expect("import");
    assert_eq!(table.row_count(), 5, "aucune ligne perdue");
    assert_eq!(table.columns[1].values, [10.0, 20.0, 30.0, 40.0, 50.0]);
    let shown = preview(text.as_bytes(), &recipe).expect("aperçu");
    assert_eq!(shown.total_data_lines, 5, "l'aperçu suit l'import");

    // Témoin : la coupure automatique, demandée en clair, coupe bien.
    recipe.data_end = stokesforge_import::DataEnd::Auto;
    let cut = parse(text.as_bytes(), &recipe).expect("import");
    assert_eq!(cut.row_count(), 1);
}

/// Une recette déjà enregistrée garde sa coupure (`"Auto"` écrit en clair) ;
/// seule une recette sans le champ prend le nouveau défaut.
#[test]
fn les_recettes_enregistrees_gardent_leur_fin_de_donnees() {
    let mut recipe = ImportRecipe::new("Ancienne");
    recipe.data_end = stokesforge_import::DataEnd::Auto;
    let json = serde_json::to_string(&recipe).expect("sérialisation");
    let back: ImportRecipe = serde_json::from_str(&json).expect("relecture");
    assert_eq!(back.data_end, stokesforge_import::DataEnd::Auto);

    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("data_end");
    let without: ImportRecipe = serde_json::from_value(value).expect("sans le champ");
    assert_eq!(without.data_end, stokesforge_import::DataEnd::None);
}

/// Fichier à fins de ligne `\r` seules (anciens Mac) : l'aperçu, l'import et
/// la détection des lignes d'en-tête découpent pareil.
#[test]
fn fichier_cr_seul_avec_ligne_de_noms() {
    let mut recipe = ImportRecipe::new("CR");
    recipe.has_column_header_row = true;
    recipe.separator = Separator::Semicolon;
    let bytes = b"Temps;Signal\r1;2\r3;4\r5;6\r";
    assert_same_split(bytes, &recipe);
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values, vec![1.0, 3.0, 5.0]);
    assert_eq!(table.columns[1].values, vec![2.0, 4.0, 6.0]);
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(shown.total_data_lines, 3);
    // Deux lignes de titre, puis des données : même compte qu'en `\n`.
    let cr = b"Titre\rAutre;texte;x\r1;2;3\r4;5;6\r";
    let lf = b"Titre\nAutre;texte;x\n1;2;3\n4;5;6\n";
    let layout = |b: &[u8]| detect_header_layout(b, Separator::Semicolon, DecimalStyle::Dot);
    assert_eq!(layout(cr), layout(lf));
    assert_eq!(layout(cr).main_header_lines, 1);
}

/// `inf` n'entre pas comme infini : jeton gardé en texte, et la fin
/// automatique des données ne coupe pas avant cette ligne.
#[test]
fn inf_reste_du_texte_et_ne_coupe_pas_la_fin_automatique() {
    let mut recipe = ImportRecipe::new("Inf");
    recipe.separator = Separator::Semicolon;
    recipe.data_end = DataEnd::Auto;
    let bytes = b"1;10\ninf;20\n3;30\n";
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.columns[0].values[0], 1.0);
    assert!(table.columns[0].values[1].is_nan());
    assert_eq!(table.columns[0].texts[1].as_deref(), Some("inf"));
    assert_eq!(table.columns[0].values[2], 3.0);
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(shown.total_data_lines, 3);
}

/// Séparateur « espaces » et milliers espace : l'espace insécable est un
/// séparateur de milliers, pas une coupure de colonne.
#[test]
fn espaces_et_milliers_espace_gardent_l_insecable() {
    let mut recipe = ImportRecipe::new("Milliers");
    recipe.separator = Separator::Whitespace;
    recipe.decimal = DecimalStyle::Comma;
    recipe.thousands = Some(' ');
    let table = parse("1\u{a0}234,5 7\n".as_bytes(), &recipe).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values, vec![1234.5]);
    assert_eq!(table.columns[1].values, vec![7.0]);
    let table = parse("1\u{202f}234,5 7\n".as_bytes(), &recipe).unwrap();
    assert_eq!(table.columns[0].values, vec![1234.5]);
}
