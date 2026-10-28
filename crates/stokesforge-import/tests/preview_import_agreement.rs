//! The preview and the import must split a file the same way: same columns,
//! same values. Without forcing the separator: it is `Auto` that is at stake
//! (issue #23).

use stokesforge_import::{
    detect_header_layout, parse, preview, DataEnd, DateOrder, DecimalStyle, HeaderSkip,
    ImportError, ImportRecipe, LineRole, Separator,
};

/// Number of cells of the preview's first data line, and values of the first
/// column read by the import.
fn assert_same_split(bytes: &[u8], recipe: &ImportRecipe) {
    let table = parse(bytes, recipe).expect("import");
    let shown = preview(bytes, recipe).expect("preview");
    for line in shown.lines.iter().filter(|l| l.role == LineRole::Data) {
        let cells = line.cells.as_ref().expect("data cells");
        assert_eq!(
            cells.len(),
            table.columns.len(),
            "line {}: the preview shows {} cells, the import {} columns ({:?})",
            line.number,
            cells.len(),
            table.columns.len(),
            cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>()
        );
        assert!(cells.iter().all(|c| !c.is_text), "line {}", line.number);
    }
}

#[test]
fn tab_and_decimal_comma_with_a_names_line() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.has_column_header_row = true;
    recipe.decimal = DecimalStyle::Comma;
    let bytes = b"Time\tSignal\n1,5\t2,5\n3,5\t4,5\n";
    assert_same_split(bytes, &recipe);
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values[0], 1.5);
    assert_eq!(table.columns[1].values[0], 2.5);
}

#[test]
fn same_columns_without_a_names_line() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.decimal = DecimalStyle::Comma;
    assert_same_split(b"1,5\t2,5\n3,5\t4,5\n", &recipe);
}

#[test]
fn with_header_skip_and_prefix() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.decimal = DecimalStyle::Comma;
    recipe.has_column_header_row = true;
    recipe.header = HeaderSkip::Lines(2);
    recipe.skip_chars = 2;
    assert_same_split(b"title\nother\n>>t\ts\n>>1,5\t2,5\n>>3,5\t4,5\n", &recipe);
    recipe.header = HeaderSkip::None;
    recipe.header_layout.main_header_lines = 2;
    assert_same_split(b"title\nother\n>>t\ts\n>>1,5\t2,5\n>>3,5\t4,5\n", &recipe);
}

#[test]
fn explicit_separators_unchanged() {
    let mut recipe = ImportRecipe::new("Audit");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    assert_same_split(b"1,5;2,5\n3,5;4,5\n", &recipe);
}

/// `DecimalStyle::Auto` (issue #7): the comma is detected with another
/// separator, the dot otherwise; the preview and the import agree.
#[test]
fn automatic_decimal() {
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

/// Missing-value tokens leave the column numeric; the thousands separator is
/// ignored.
#[test]
fn missing_values_and_thousands() {
    let mut recipe = ImportRecipe::new("Missing");
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
        "the missing ones are not text"
    );
    assert_same_split(bytes, &recipe);
    recipe.missing_tokens.clear();
    let with_text = parse(bytes, &recipe).unwrap();
    assert!(with_text.columns[1].texts.iter().any(Option::is_some));
}

/// Custom separator and fixed-width columns (issue #7).
#[test]
fn custom_separator_and_fixed_width() {
    let mut recipe = ImportRecipe::new("Bar");
    recipe.separator = Separator::Custom('|');
    let bytes = b"1|10|100\n2|20|200\n";
    assert_same_split(bytes, &recipe);
    assert_eq!(parse(bytes, &recipe).unwrap().columns.len(), 3);

    let mut fixed = ImportRecipe::new("Fixed");
    fixed.separator = Separator::FixedWidth(vec![4, 6]);
    // 4 + 6 + rest: "1.5", "20.25", "7".
    let bytes = b"1.5 20.25 7\n2.5 30.75 8\n";
    assert_same_split(bytes, &fixed);
    let table = parse(bytes, &fixed).unwrap();
    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].values, vec![1.5, 2.5]);
    assert_eq!(table.columns[1].values, vec![20.25, 30.75]);
    assert_eq!(table.columns[2].values, vec![7.0, 8.0]);
}

/// Comment lines: ignored by the import, shown by the preview with their own
/// role, without shifting the data; and line range.
#[test]
fn comments_and_line_range() {
    let mut recipe = ImportRecipe::new("Comments");
    recipe.separator = Separator::Comma;
    recipe.comment_prefix = "#".into();
    let bytes = b"# Monday measurement\n1,10\n# recalibration\n2,20\n3,30\n4,40\n";
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

    // The first two data lines are skipped; the preview marks them.
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
    // Skip everything: no data left.
    recipe.skip_data_rows = 10;
    assert!(parse(bytes, &recipe).is_err());
    assert!(preview(bytes, &recipe).is_err());
}

/// A forced encoding applies to the import as to the preview.
#[test]
fn forced_encoding() {
    let mut recipe = ImportRecipe::new("Latin");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.has_column_header_row = true;
    // "T°C;Signal" in Windows-1252: the 0xB0 byte is invalid in UTF-8.
    let bytes: &[u8] = b"T\xB0C;Signal\n1,5;2,5\n";
    recipe.encoding = stokesforge_import::Encoding::Latin1;
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("T°C"));
    assert_same_split(bytes, &recipe);
    recipe.encoding = stokesforge_import::Encoding::Utf8;
    assert_eq!(
        parse(bytes, &recipe).unwrap().columns[0].name.as_deref(),
        Some("T\u{FFFD}C"),
        "forced UTF-8: the invalid byte does not fail the import"
    );
}

/// Forced kind of a column (issue #7): text (even a number stays text),
/// date-time (POSIX timestamps, an unreadable cell stays text).
#[test]
fn text_and_datetime_columns() {
    use stokesforge_import::ColumnKind;
    let mut recipe = ImportRecipe::new("Kinds");
    recipe.separator = Separator::Semicolon;
    recipe.column_kinds = vec![ColumnKind::Text, ColumnKind::DateTime, ColumnKind::Auto];
    let bytes = b"007;25/11/24 15:17:22;1\nA12;2000-01-01;2\nZ9;not a date;3\n";
    let table = parse(bytes, &recipe).unwrap();
    // Text: non-empty tokens are kept as is, even "007".
    assert_eq!(
        table.columns[0].texts,
        vec![
            Some("007".to_string()),
            Some("A12".to_string()),
            Some("Z9".to_string())
        ]
    );
    assert!(table.columns[0].values.iter().all(|v| v.is_nan()));
    // Date-time: timestamps, the unreadable cell stays text.
    assert!(table.columns[1].datetime);
    assert_eq!(table.columns[1].values[0], 1_732_547_842.0);
    assert_eq!(table.columns[1].values[1], 946_684_800.0);
    assert_eq!(table.columns[1].texts[2].as_deref(), Some("not a date"));
    assert!(!table.columns[2].datetime);
    // The preview annotates exactly the same cells.
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

/// A file with several blocks (several spectra, each with its header) gives
/// one table per block; `parse` returns the first, the preview counts the
/// total (issue #7).
#[test]
fn file_with_several_blocks() {
    use stokesforge_import::{parse_blocks, BlockSplit, HeaderSkip};
    let mut recipe = ImportRecipe::new("Multi");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.name_source = stokesforge_import::NameSource::HeaderKey("TITLE".into());
    recipe.block_split = BlockSplit::BeforeLine("TITLE".into());
    let bytes =
        b"TITLE;first\nXYDATA\n1,5;10\n2,5;20\nTITLE;second\nXYDATA\n3,5;30\n4,5;40\n5,5;50\n";

    let tables = parse_blocks(bytes, &recipe).unwrap();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].columns[0].values, vec![1.5, 2.5]);
    assert_eq!(tables[1].columns[0].values, vec![3.5, 4.5, 5.5]);
    assert_eq!(
        stokesforge_import::resolve_series_name(&recipe, &tables[1], "f"),
        "second"
    );
    assert_eq!(parse(bytes, &recipe).unwrap().row_count(), 2, "the first");
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(shown.block_count, 2);
    assert_eq!(shown.total_data_lines, 2);

    // Without splitting: a single block, the following "TITLE" lines are
    // unreadable data cut by the automatic end.
    recipe.block_split = BlockSplit::None;
    assert_eq!(parse_blocks(bytes, &recipe).unwrap().len(), 1);
    assert_eq!(preview(bytes, &recipe).unwrap().block_count, 1);

    // A block without data is dropped, and with no data at all the import fails.
    recipe.block_split = BlockSplit::BlankLines;
    assert!(parse_blocks(b"one\nother\n", &recipe).is_err());
}

/// The cut tests X after the prefix is removed (counted in characters), even
/// with leading spaces. Text and emptiness before the first number stay data;
/// text in Y never cuts (issue #4).
#[test]
fn automatic_end_after_prefix_and_before_footer() {
    for (separator, text) in [
        (
            Separator::Semicolon,
            "title\nλ>text;before\nλ>\nλ> 1;10\nλ> 2;n/a\nλ>\nλ>Data points;551\n",
        ),
        (
            Separator::Whitespace,
            "title\nλ>text before\nλ>\nλ>   1   10\nλ>   2   n/a\nλ>\nλ>Data points 551\n",
        ),
    ] {
        let mut recipe = ImportRecipe::new("Instrument prefix");
        recipe.data_end = stokesforge_import::DataEnd::Auto;
        recipe.separator = separator;
        recipe.header = HeaderSkip::Lines(1);
        recipe.skip_chars = 2;
        let table = parse(text.as_bytes(), &recipe).expect("import");
        assert_eq!(table.row_count(), 4);
        assert_eq!(table.columns[0].texts[0].as_deref(), Some("text"));
        assert!(table.columns[0].values[1].is_nan());
        assert!(table.columns[0].texts[1].is_none());
        assert_eq!(&table.columns[0].values[2..], &[1.0, 2.0]);
        assert_eq!(table.columns[1].texts[3].as_deref(), Some("n/a"));

        let shown = preview(text.as_bytes(), &recipe).expect("preview");
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

        // Disabling survives saving the recipe and keeps the footer in the
        // data: witness of a cut that is really active.
        recipe.data_end = stokesforge_import::DataEnd::None;
        let restored = ImportRecipe::from_json(&recipe.to_json()).expect("recipe read back");
        let uncut = parse(text.as_bytes(), &restored).expect("without a cut");
        assert_eq!(uncut.row_count(), 6);
        assert!(uncut.footer_lines().is_empty());
        let shown = preview(text.as_bytes(), &restored).expect("preview without a cut");
        assert_eq!(shown.total_data_lines, 6);
        assert!(!shown.lines.iter().any(|line| line.role == LineRole::Footer));
    }
}

/// A missing measurement in X ("n/a", "-", "NaN") no longer cuts the file:
/// by default no cut, all lines are imported and the preview shows as many.
/// The automatic cut silently lost everything that followed.
#[test]
fn by_default_a_missing_measurement_in_x_does_not_cut_the_file() {
    let text = "1;10\nn/a;20\n3;30\n-;40\n5;50\n";
    let mut recipe = ImportRecipe::new("Default");
    recipe.separator = Separator::Semicolon;
    let table = parse(text.as_bytes(), &recipe).expect("import");
    assert_eq!(table.row_count(), 5, "no line lost");
    assert_eq!(table.columns[1].values, [10.0, 20.0, 30.0, 40.0, 50.0]);
    let shown = preview(text.as_bytes(), &recipe).expect("preview");
    assert_eq!(shown.total_data_lines, 5, "the preview follows the import");

    // Witness: the automatic cut, requested explicitly, does cut.
    recipe.data_end = stokesforge_import::DataEnd::Auto;
    let cut = parse(text.as_bytes(), &recipe).expect("import");
    assert_eq!(cut.row_count(), 1);
}

/// An already saved recipe keeps its cut (`"Auto"` written explicitly); only
/// a recipe without the field takes the new default.
#[test]
fn saved_recipes_keep_their_data_end() {
    let mut recipe = ImportRecipe::new("Old");
    recipe.data_end = stokesforge_import::DataEnd::Auto;
    let json = serde_json::to_string(&recipe).expect("serialization");
    let back: ImportRecipe = serde_json::from_str(&json).expect("read back");
    assert_eq!(back.data_end, stokesforge_import::DataEnd::Auto);

    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("data_end");
    let without: ImportRecipe = serde_json::from_value(value).expect("without the field");
    assert_eq!(without.data_end, stokesforge_import::DataEnd::None);
}

/// File with lone `\r` line endings (old Macs): the preview, the import and
/// the header line detection split the same way.
#[test]
fn lone_cr_file_with_a_names_line() {
    let mut recipe = ImportRecipe::new("CR");
    recipe.has_column_header_row = true;
    recipe.separator = Separator::Semicolon;
    let bytes = b"Time;Signal\r1;2\r3;4\r5;6\r";
    assert_same_split(bytes, &recipe);
    let table = parse(bytes, &recipe).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].values, vec![1.0, 3.0, 5.0]);
    assert_eq!(table.columns[1].values, vec![2.0, 4.0, 6.0]);
    let shown = preview(bytes, &recipe).unwrap();
    assert_eq!(shown.total_data_lines, 3);
    // Two title lines, then data: same count as with `\n`.
    let cr = b"Title\rOther;text;x\r1;2;3\r4;5;6\r";
    let lf = b"Title\nOther;text;x\n1;2;3\n4;5;6\n";
    let layout = |b: &[u8]| detect_header_layout(b, Separator::Semicolon, DecimalStyle::Dot);
    assert_eq!(layout(cr), layout(lf));
    assert_eq!(layout(cr).main_header_lines, 1);
}

/// `inf` does not enter as infinity: the token is kept as text, and the
/// automatic data end does not cut before that line.
#[test]
fn inf_stays_text_and_does_not_cut_the_automatic_end() {
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

/// "Whitespace" separator (chosen, or resolved from Auto) with the space as
/// thousands separator: `1 234,5` would be split into two columns. The import
/// and the preview reject the combination, in exactly the same way.
#[test]
fn whitespace_and_space_thousands_are_rejected_by_parse_and_preview() {
    for separator in [Separator::Whitespace, Separator::Auto] {
        let mut recipe = ImportRecipe::new("Thousands");
        recipe.separator = separator;
        recipe.decimal = DecimalStyle::Comma;
        recipe.thousands = Some(' ');
        let bytes = "1 234,5 7\n".as_bytes();
        let err = parse(bytes, &recipe).unwrap_err();
        assert_eq!(err, ImportError::WhitespaceWithSpaceThousands);
        assert_eq!(preview(bytes, &recipe).unwrap_err(), err);
    }
}

/// With an explicit column separator, the space stays a valid thousands
/// separator (non-breaking spaces included).
#[test]
fn semicolon_and_tab_accept_space_thousands() {
    for (separator, bytes) in [
        (Separator::Semicolon, "1 234,5;7\n"),
        (Separator::Tab, "1 234,5\t7\n"),
        (Separator::Semicolon, "1\u{a0}234,5;7\n"),
    ] {
        let mut recipe = ImportRecipe::new("Thousands");
        recipe.separator = separator;
        recipe.decimal = DecimalStyle::Comma;
        recipe.thousands = Some(' ');
        let table = parse(bytes.as_bytes(), &recipe).unwrap();
        assert_eq!(table.columns[0].values, vec![1234.5]);
        assert_eq!(table.columns[1].values, vec![7.0]);
        assert!(preview(bytes.as_bytes(), &recipe).is_ok());
    }
}

/// Day/month order (B2): the column forced to dates follows the recipe
/// setting, ISO does not depend on it, a month 13 stays text.
#[test]
fn the_recipe_day_month_order_governs_dates() {
    use stokesforge_import::ColumnKind;
    let days = |text: &str, order: DateOrder| {
        let mut recipe = ImportRecipe::new("Dates");
        recipe.separator = Separator::Semicolon;
        recipe.column_kinds = vec![ColumnKind::DateTime];
        recipe.date_order = order;
        let table = parse(format!("{text};1\n").as_bytes(), &recipe).unwrap();
        table.columns[0].values[0]
    };
    let ts = |iso: &str| stokesforge_import::parse_datetime_for_agreement(iso).unwrap();
    assert_eq!(days("03/04/2026", DateOrder::MonthFirst), ts("2026-03-04"));
    assert_eq!(days("03/04/2026", DateOrder::DayFirst), ts("2026-04-03"));
    assert!(days("13/01/2026", DateOrder::MonthFirst).is_nan());
    assert_eq!(days("2026-03-04", DateOrder::MonthFirst), ts("2026-03-04"));
    assert_eq!(days("2026-03-04", DateOrder::DayFirst), ts("2026-03-04"));
}

/// The automatic data end reads the first column as dates according to the
/// order: "13/01/2026" is a date day first (the preceding lines, month 13+,
/// are then kept text), not month first (where it cuts the data).
#[test]
fn automatic_data_end_follows_the_day_month_order() {
    use stokesforge_import::ColumnKind;
    let bytes = b"01/13/2026;1\n02/14/2026;2\n03/15/2026;3\n13/01/2026;4\n";
    let rows = |order| {
        let mut recipe = ImportRecipe::new("Dates");
        recipe.separator = Separator::Semicolon;
        recipe.column_kinds = vec![ColumnKind::DateTime];
        recipe.data_end = DataEnd::Auto;
        recipe.date_order = order;
        let n = parse(bytes, &recipe).unwrap().row_count();
        assert_eq!(preview(bytes, &recipe).unwrap().total_data_lines, n);
        n
    };
    assert_eq!(rows(DateOrder::MonthFirst), 3);
    assert_eq!(rows(DateOrder::DayFirst), 4);
}
