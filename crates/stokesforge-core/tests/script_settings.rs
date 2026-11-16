//! Script Settings text: reading, hints, lossless edits, script writes (#548).

use stokesforge_core::script_settings::{
    apply_values, parse, to_values, EditError, ElementKind, EntryValue, Scalar, ScalarKind, Seg,
    SettingsValue,
};

fn key(k: &str) -> Vec<Seg> {
    vec![Seg::Key(k.to_string())]
}

fn entry_value(text: &str, k: &str) -> EntryValue {
    let doc = parse(text).expect("parses");
    doc.entry(&key(k)).expect("entry").value.clone()
}

#[test]
fn syntax_error_reports_one_based_line() {
    let err = parse("a = 1\nb =").expect_err("malformed");
    assert_eq!(err.line, 2);
    assert!(err.column >= 1);
    assert!(!err.message.is_empty());
    assert_eq!(parse("a = 1\nb = 2\nc = = 3").expect_err("bad").line, 3);
}

#[test]
fn set_scalar_changes_only_the_value() {
    let text = "# help\na = 1 # end\n[t]\nb = 'x'";
    let doc = parse(text).expect("parses");
    let out = doc.set_scalar(&key("a"), &Scalar::Integer(5)).expect("set");
    assert_eq!(out, "# help\na = 5 # end\n[t]\nb = 'x'");
}

#[test]
fn set_scalar_in_a_table_and_a_block() {
    let text = "[t]\nb = 'x' # c\n[[r]]\nn = 1\n[[r]]\nn = 2\n";
    let doc = parse(text).expect("parses");
    let out = doc
        .set_scalar(
            &[Seg::Key("t".into()), Seg::Key("b".into())],
            &Scalar::String("y".into()),
        )
        .expect("set");
    assert_eq!(out, "[t]\nb = \"y\" # c\n[[r]]\nn = 1\n[[r]]\nn = 2\n");
    let out = doc
        .set_scalar(
            &[Seg::Key("r".into()), Seg::Index(1), Seg::Key("n".into())],
            &Scalar::Integer(9),
        )
        .expect("set");
    assert_eq!(out, "[t]\nb = 'x' # c\n[[r]]\nn = 1\n[[r]]\nn = 9\n");
}

#[test]
fn hints_are_read_and_removed_from_the_help() {
    let text = "# Speed of the run\n# CHOICES: fast | slow\nmode = 'fast'\n\
                # between 1 and 1000\nn = 5\n\
                # between x and 3\nm = 5\n\
                # at least 2\n# at most 8\nk = 4\n\
                # choices: only\nz = 1\n";
    let doc = parse(text).expect("parses");
    let mode = doc.entry(&key("mode")).expect("mode");
    assert_eq!(
        mode.hint.choices,
        Some(vec!["fast".to_string(), "slow".to_string()])
    );
    assert_eq!(mode.help, "Speed of the run");
    let n = doc.entry(&key("n")).expect("n");
    assert_eq!((n.hint.min, n.hint.max), (Some(1.0), Some(1000.0)));
    assert_eq!(n.help, "");
    let m = doc.entry(&key("m")).expect("m");
    assert_eq!((m.hint.min, m.hint.max), (None, None));
    assert_eq!(m.help, "between x and 3");
    let k = doc.entry(&key("k")).expect("k");
    assert_eq!((k.hint.min, k.hint.max), (Some(2.0), Some(8.0)));
    let z = doc.entry(&key("z")).expect("z");
    assert!(z.hint.choices.is_none());
    assert_eq!(z.help, "choices: only");
}

#[test]
fn help_stops_at_a_blank_line() {
    let doc = parse("# far\n\n# near\na = 1\n").expect("parses");
    assert_eq!(doc.entry(&key("a")).expect("a").help, "near");
}

#[test]
fn out_of_hint_reports_without_rewriting() {
    let text = "# between 1 and 10\na = 11\n# choices: x | y\nb = 'z'\n# at most 3\nc = 2.5\n";
    let doc = parse(text).expect("parses");
    assert!(doc.entry(&key("a")).expect("a").out_of_hint());
    assert!(doc.entry(&key("b")).expect("b").out_of_hint());
    assert!(!doc.entry(&key("c")).expect("c").out_of_hint());
}

#[test]
fn types_follow_the_stored_value() {
    let doc = parse("i = 1\nf = 2.0\n").expect("parses");
    assert!(matches!(
        doc.set_scalar(&key("i"), &Scalar::Float(1.5)),
        Err(EditError::TypeMismatch { .. })
    ));
    assert_eq!(
        doc.set_scalar(&key("f"), &Scalar::Integer(3))
            .expect("int into float"),
        "i = 1\nf = 3.0\n"
    );
    assert!(matches!(
        doc.set_scalar(&key("i"), &Scalar::String("1".into())),
        Err(EditError::TypeMismatch { .. })
    ));
}

#[test]
fn big_integers_and_dates_are_exact() {
    let text = "n = 9007199254740993\nd = 2024-05-01\nt = 07:30:00\nl = 2024-05-01T07:30:00\no = 2024-05-01T07:30:00Z\n";
    let doc = parse(text).expect("parses");
    assert_eq!(
        doc.entry(&key("n")).expect("n").value,
        EntryValue::Scalar(Scalar::Integer(9007199254740993))
    );
    assert!(matches!(
        entry_value(text, "d"),
        EntryValue::Scalar(Scalar::LocalDate(_))
    ));
    assert!(matches!(
        entry_value(text, "t"),
        EntryValue::Scalar(Scalar::LocalTime(_))
    ));
    assert!(matches!(
        entry_value(text, "l"),
        EntryValue::Scalar(Scalar::LocalDateTime(_))
    ));
    assert!(matches!(
        entry_value(text, "o"),
        EntryValue::Scalar(Scalar::OffsetDateTime(_))
    ));
    let out = doc
        .set_scalar(&key("n"), &Scalar::Integer(9007199254740995))
        .expect("set");
    assert!(out.starts_with("n = 9007199254740995\n"));
    // A date-only key stays date-only and refuses a date-time.
    let out = doc
        .set_scalar(&key("d"), &Scalar::LocalDate("2025-01-02".into()))
        .expect("date");
    assert!(out.contains("d = 2025-01-02\n"));
    assert!(doc
        .set_scalar(
            &key("d"),
            &Scalar::LocalDateTime("2025-01-02T01:02:03".into())
        )
        .is_err());
    assert!(matches!(
        doc.set_scalar(&key("d"), &Scalar::LocalDate("nope".into())),
        Err(EditError::BadDate(_))
    ));
}

#[test]
fn pathological_inputs_do_not_crash() {
    let deep = format!("a = {}", "[".repeat(10_000));
    // The depth bound is the parser's own (toml_edit: recursion limit of 80).
    assert!(parse(&deep)
        .expect_err("too deep")
        .message
        .contains("recursion limit"));
    let deep_inline = format!("a = {}", "{a=".repeat(10_000));
    assert!(parse(&deep_inline).is_err());
    let long_line = format!("a = \"{}\"", "x".repeat(2_000_000));
    assert!(parse(&long_line).is_ok());
    let huge: String = (0..400_000).map(|i| format!("key{i} = 1\n")).collect();
    assert!(huge.len() > stokesforge_core::script_settings::MAX_TEXT_BYTES);
    assert!(parse(&huge).is_err());
    let many_tables: String = (0..20_000).map(|i| format!("[t{i}]\nk = {i}\n")).collect();
    assert_eq!(parse(&many_tables).expect("parses").sections.len(), 20_000);
}

#[test]
fn arrays_set_append_remove() {
    let text = "a = [1, 2, 3] # arr\nm = [1, 'x']\ne = []\n";
    let doc = parse(text).expect("parses");
    assert_eq!(
        entry_value(text, "a"),
        EntryValue::Array {
            kind: ElementKind::Uniform(ScalarKind::Integer),
            items: vec![Scalar::Integer(1), Scalar::Integer(2), Scalar::Integer(3)]
        }
    );
    assert_eq!(entry_value(text, "m"), EntryValue::MixedArray);
    let a = key("a");
    assert_eq!(
        doc.set_element(&a, 1, &Scalar::Integer(9)).expect("set"),
        "a = [1, 9, 3] # arr\nm = [1, 'x']\ne = []\n"
    );
    assert!(doc.set_element(&a, 1, &Scalar::String("s".into())).is_err());
    assert!(doc.set_element(&a, 7, &Scalar::Integer(1)).is_err());
    assert_eq!(
        doc.append_element(&a).expect("append"),
        "a = [1, 2, 3, 3] # arr\nm = [1, 'x']\ne = []\n"
    );
    assert!(matches!(
        doc.append_element(&key("e")),
        Err(EditError::EmptyArray(_))
    ));
    assert!(doc.append_element(&key("m")).is_err());
    assert_eq!(
        doc.remove_element(&a, 0).expect("remove"),
        "a = [2, 3] # arr\nm = [1, 'x']\ne = []\n"
    );
    assert!(doc.remove_element(&a, 3).is_err());
}

#[test]
fn append_copies_the_type_of_the_last_element() {
    let doc = parse("s = ['a', 'b']\nf = [1.5]\n").expect("parses");
    let out = parse(&doc.append_element(&key("s")).expect("append")).expect("parses");
    match &out.entry(&key("s")).expect("s").value {
        EntryValue::Array { kind, items } => {
            assert_eq!(*kind, ElementKind::Uniform(ScalarKind::String));
            assert_eq!(items.len(), 3);
        }
        other => panic!("{other:?}"),
    }
    let out = parse(&doc.append_element(&key("f")).expect("append")).expect("parses");
    assert!(matches!(
        &out.entry(&key("f")).expect("f").value,
        EntryValue::Array {
            kind: ElementKind::Uniform(ScalarKind::Float),
            ..
        }
    ));
}

#[test]
fn sections_follow_the_file() {
    let text = "r = 1\n[fit]\nk = 1\n[fit.baseline]\norder = 2\n[[run]]\nid = 1\n[[run]]\nid = 2\n\
                [empty]\n";
    let doc = parse(text).expect("parses");
    let names: Vec<(Vec<&str>, Option<usize>)> =
        doc.sections.iter().map(|s| (s.names(), s.block)).collect();
    assert_eq!(
        names,
        vec![
            (vec![], None),
            (vec!["fit"], None),
            (vec!["fit", "baseline"], None),
            (vec!["run"], Some(1)),
            (vec!["run"], Some(2)),
            (vec!["empty"], None),
        ]
    );
}

#[test]
fn dotted_keys_and_inline_tables_are_nested_sections() {
    let doc = parse("a.b = 1\nc = { d = 2 }\n").expect("parses");
    let names: Vec<Vec<&str>> = doc.sections.iter().map(|s| s.names()).collect();
    assert_eq!(names, vec![vec!["a"], vec!["c"]]);
    let out = doc
        .set_scalar(
            &[Seg::Key("c".into()), Seg::Key("d".into())],
            &Scalar::Integer(7),
        )
        .expect("set");
    assert_eq!(out, "a.b = 1\nc = { d = 7 }\n");
    let out = doc
        .set_scalar(
            &[Seg::Key("a".into()), Seg::Key("b".into())],
            &Scalar::Integer(7),
        )
        .expect("set");
    assert_eq!(out, "a.b = 7\nc = { d = 2 }\n");
}

#[test]
fn help_on_dotted_keys() {
    let doc = parse("# about b\na.b = 1\n").expect("parses");
    assert_eq!(
        doc.entry(&[Seg::Key("a".into()), Seg::Key("b".into())])
            .expect("b")
            .help,
        "about b"
    );
}

#[test]
fn to_values_is_ordered_and_nested() {
    let doc = parse("z = 1\na = [1, 2]\n[t]\nk = 'v'\n[[r]]\nn = 1\n").expect("parses");
    let SettingsValue::Map(root) = to_values(&doc) else {
        panic!("map")
    };
    let keys: Vec<&str> = root.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["z", "a", "t", "r"]);
    assert_eq!(
        root[1].1,
        SettingsValue::List(vec![
            SettingsValue::Scalar(Scalar::Integer(1)),
            SettingsValue::Scalar(Scalar::Integer(2))
        ])
    );
    assert!(matches!(&root[3].1, SettingsValue::List(l) if l.len() == 1));
}

fn map(entries: Vec<(&str, SettingsValue)>) -> SettingsValue {
    SettingsValue::Map(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}
fn int(i: i64) -> SettingsValue {
    SettingsValue::Scalar(Scalar::Integer(i))
}

#[test]
fn apply_values_updates_creates_and_keeps_comments() {
    let text = "# help\na = 1 # end\nb = 2\n[t]\nc = 'x'\n";
    let values = map(vec![
        ("a", int(10)),
        (
            "t",
            map(vec![
                ("d", int(4)),
                ("c", SettingsValue::Scalar(Scalar::String("y".into()))),
            ]),
        ),
        ("n", map(vec![("deep", map(vec![("k", int(1))]))])),
        ("top", int(3)),
    ]);
    let out = apply_values(text, &values).expect("applied");
    assert!(
        out.starts_with("# help\na = 10 # end\nb = 2\ntop = 3\n[t]\nc = \"y\"\nd = 4\n"),
        "{out}"
    );
    let doc = parse(&out).expect("result is valid TOML");
    assert!(doc
        .entry(&[
            Seg::Key("n".into()),
            Seg::Key("deep".into()),
            Seg::Key("k".into())
        ])
        .is_some());
    assert!(doc.entry(&key("b")).is_some());
}

#[test]
fn apply_values_refuses_a_wrong_type_and_names_the_key() {
    let text = "a = 1\nb = 2\n";
    let values = map(vec![
        ("a", int(5)),
        ("c", int(7)),
        ("b", SettingsValue::Scalar(Scalar::String("no".into()))),
    ]);
    match apply_values(text, &values) {
        Err(EditError::TypeMismatch { key, .. }) => assert_eq!(key, "b"),
        other => panic!("{other:?}"),
    }
    // A float into an integer key is refused, an integer into a float key is kept as a float.
    assert!(apply_values(
        "a = 1\n",
        &map(vec![("a", SettingsValue::Scalar(Scalar::Float(1.5)))])
    )
    .is_err());
    assert_eq!(
        apply_values("a = 1.0\n", &map(vec![("a", int(3))])).expect("ok"),
        "a = 3.0\n"
    );
}

#[test]
fn apply_values_ignores_hints() {
    let text = "# between 1 and 10\na = 5\n";
    let out = apply_values(text, &map(vec![("a", int(99))])).expect("applied");
    assert_eq!(out, "# between 1 and 10\na = 99\n");
}

#[test]
fn apply_values_rewrites_lists_and_blocks() {
    let text = "a = [1, 2]\n[[r]]\nn = 1\n[[r]]\nn = 2\n";
    let values = map(vec![
        ("a", SettingsValue::List(vec![int(5), int(6), int(7)])),
        (
            "r",
            SettingsValue::List(vec![map(vec![("n", int(8))]), map(vec![("n", int(9))])]),
        ),
    ]);
    let out = apply_values(text, &values).expect("applied");
    assert_eq!(out, "a = [5, 6, 7]\n[[r]]\nn = 8\n[[r]]\nn = 9\n");
    let wrong_len = map(vec![(
        "r",
        SettingsValue::List(vec![map(vec![("n", int(8))])]),
    )]);
    assert!(apply_values(text, &wrong_len).is_err());
    let mixed = map(vec![(
        "a",
        SettingsValue::List(vec![int(1), SettingsValue::Scalar(Scalar::Bool(true))]),
    )]);
    assert!(apply_values(text, &mixed).is_err());
}

#[test]
fn apply_values_on_a_text_without_final_newline() {
    let out = apply_values("a = 1", &map(vec![("b", int(2))])).expect("applied");
    assert!(parse(&out).is_ok(), "{out:?}");
    assert_eq!(parse(&out).expect("p").sections[0].entries.len(), 2);
}

#[test]
fn apply_values_refuses_malformed_text() {
    assert!(matches!(
        apply_values("a =", &map(vec![])),
        Err(EditError::Parse(_))
    ));
}
