//! Script Settings from `pyst` (#548, lot L4): `settings` and `set_settings`.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{NodeKind, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

const STORED: &str = "# Script settings\n\
title = \"run\"  # kept\n\
big = 9007199254740993\n\
ratio = 0.5\n\
on = true\n\
day = 2026-11-16\n\
stamp = 1979-05-27T07:32:00\n\
at = 07:32:00\n\
list = [1, 2, 3]\n\
\n\
# choices: a | b\n\
mode = \"a\"\n\
\n\
[fit]\n\
# at most 10\n\
order = 3\n\
\n\
[[peak]]\n\
x = 1.5\n\
[[peak]]\n\
x = 2.5\n";

fn bench_with(text: &str) -> Bench {
    let mut project = Project::new("Settings").unwrap();
    let root = project.root();
    project
        .add_script_settings(root, Some("Settings1"), text)
        .unwrap();
    project.add_note(root, Some("Note1"), "before").unwrap();
    project
        .add_script_settings(root, Some("Broken"), "a = \nb = 2\n")
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());
    Bench {
        engine,
        session,
        _actor: actor,
    }
}

fn bench() -> Bench {
    bench_with(STORED)
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

fn stored(bench: &Bench, name: &str) -> String {
    let snapshot = bench.session.snapshot().unwrap();
    let id = snapshot.tree.resolve(&format!("[{name}]")).unwrap();
    match snapshot.tree.node(id).unwrap().kind() {
        NodeKind::ScriptSettings { text, .. } => text.clone(),
        other => panic!("{other:?}"),
    }
}

/// Runs `body`, which must raise `pyst.InvalidValueError`; returns the message.
fn refused(bench: &Bench, body: &str) -> String {
    let source = format!(
        "s = pyst.resolve('[Settings1]')\ntry:\n    {body}\nexcept pyst.InvalidValueError as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no InvalidValueError')\nprint(caught)"
    );
    run(bench, &source).stdout.trim().to_owned()
}

#[test]
fn settings_reads_a_dict_in_file_order_with_exact_and_dated_values() {
    let b = bench();
    run(
        &b,
        "import datetime\n\
         s = pyst.resolve('[Settings1]')\n\
         assert pyst.info(s)['kind'] == 'script_settings'\n\
         d = pyst.settings(s)\n\
         assert list(d)[:9] == ['title', 'big', 'ratio', 'on', 'day', 'stamp', 'at', 'list', 'mode'], list(d)\n\
         assert d['title'] == 'run' and d['on'] is True and d['ratio'] == 0.5\n\
         assert d['big'] == 9007199254740993 and type(d['big']) is int, d['big']\n\
         assert type(d['day']) is datetime.date and d['day'] == datetime.date(2026, 11, 16), d['day']\n\
         assert type(d['stamp']) is datetime.datetime and d['stamp'].hour == 7\n\
         assert type(d['at']) is datetime.time and d['at'] == datetime.time(7, 32)\n\
         assert d['list'] == [1, 2, 3]\n\
         assert d['fit'] == {'order': 3}\n\
         assert d['peak'] == [{'x': 1.5}, {'x': 2.5}], d['peak']\n",
    );
}

#[test]
fn set_settings_and_set_note_in_one_run_are_one_undo_entry() {
    let b = bench();
    run(
        &b,
        "s = pyst.resolve('[Settings1]')\n\
         pyst.set_settings(s, {'title': 'changed'})\n\
         pyst.set_note(pyst.resolve('[Note1]'), 'after')\n",
    );
    assert!(stored(&b, "Settings1").contains("title = \"changed\""));
    run(&b, "assert pyst.undo() is True\n");
    assert!(stored(&b, "Settings1").contains("title = \"run\""));
    let snapshot = b.session.snapshot().unwrap();
    let note = snapshot.tree.resolve("[Note1]").unwrap();
    assert!(matches!(
        snapshot.tree.node(note).unwrap().kind(),
        NodeKind::Note { text, .. } if text == "before"
    ));
    run(&b, "assert pyst.undo() is False\n");
}

#[test]
fn set_settings_creates_missing_keys_and_keeps_comments_and_order() {
    let b = bench();
    run(
        &b,
        "import datetime\n\
         s = pyst.resolve('[Settings1]')\n\
         pyst.set_settings(s, {'title': 'new', 'fresh': 7, 'fit': {'order': 4, 'extra': 'x'}, 'day': datetime.date(2027, 1, 2), 'ratio': 2})\n\
         d = pyst.settings(s)\n\
         assert d['title'] == 'new' and d['fresh'] == 7 and d['fit'] == {'order': 4, 'extra': 'x'}\n\
         assert d['day'] == datetime.date(2027, 1, 2) and d['ratio'] == 2.0\n\
         assert d['mode'] == 'a' and d['big'] == 9007199254740993\n",
    );
    let text = stored(&b, "Settings1");
    assert!(
        text.starts_with("# Script settings\ntitle = \"new\"  # kept\n"),
        "{text}"
    );
    assert!(text.contains("# choices: a | b\nmode = \"a\""), "{text}");
    assert!(text.contains("fresh = 7"), "{text}");
    assert!(text.find("title").unwrap() < text.find("big").unwrap());
}

#[test]
fn a_hint_never_refuses_a_script() {
    let b = bench();
    run(
        &b,
        "s = pyst.resolve('[Settings1]')\n\
         pyst.set_settings(s, {'mode': 'zzz', 'fit': {'order': 99}})\n\
         assert pyst.settings(s)['fit']['order'] == 99\n",
    );
    assert!(stored(&b, "Settings1").contains("mode = \"zzz\""));
}

#[test]
fn a_wrong_type_is_refused_naming_the_key_and_nothing_is_written() {
    let b = bench();
    let before = stored(&b, "Settings1");
    // The first key is valid, the second has the wrong type: nothing lands.
    let message = refused(
        &b,
        "pyst.set_settings(s, {'title': 'ok', 'fit': {'order': 'three'}})",
    );
    assert!(message.contains("fit.order"), "{message}");
    assert_eq!(stored(&b, "Settings1"), before);
    let message = refused(&b, "pyst.set_settings(s, {'on': 1})");
    assert!(message.contains("on"), "{message}");
    refused(&b, "pyst.set_settings(s, {'big': 2 ** 70})");
    refused(&b, "pyst.set_settings(s, {'x': object()})");
    assert_eq!(stored(&b, "Settings1"), before);
    // Refused writes leave no history entry.
    run(&b, "assert pyst.undo() is False\n");
}

#[test]
fn a_handle_that_is_not_script_settings_is_refused() {
    let b = bench();
    let message = refused(&b, "pyst.settings(pyst.resolve('[Note1]'))");
    assert!(message.contains("not a Script Settings"), "{message}");
    refused(&b, "pyst.set_settings(pyst.resolve('[Note1]'), {'a': 1})");
}

#[test]
fn a_malformed_stored_text_gives_the_object_and_the_line() {
    let b = bench();
    let source = "try:\n    pyst.settings(pyst.resolve('[Broken]'))\nexcept pyst.InvalidValueError as error:\n    print(str(error))\n";
    let message = run(&b, source).stdout;
    assert!(
        message.contains("Broken") && message.contains("line 1"),
        "{message}"
    );
    let source = "try:\n    pyst.set_settings(pyst.resolve('[Broken]'), {'b': 3})\nexcept pyst.InvalidValueError as error:\n    print(str(error))\n";
    let message = run(&b, source).stdout;
    assert!(message.contains("Broken"), "{message}");
    assert_eq!(stored(&b, "Broken"), "a = \nb = 2\n");
}
