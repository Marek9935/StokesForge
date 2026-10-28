//! Guard of the frozen V1 API of `pyst` (#331, batch 3).
//!
//! The reference list is the "Frozen API V1" section of
//! `docs/python-and-scripting.md`, in blocks ```` ```pyst-api <kind> <name> ````.
//! This test **reads those blocks** (it keeps no copy of them) and compares
//! them with the module actually built:
//!
//! - **surface**: public names and kind (function, class, exception,
//!   submodule) of `pyst` and `pyst.analysis`, and the signature that
//!   `inspect.signature` returns for each function (if it failed, the test
//!   would pass vacuously: this is therefore checked); `Handle` has nothing
//!   public;
//! - **options**: the keys accepted by `plot`, `set_graph` and
//!   `set_column_meta` (the "unknown key" message lists them, from the
//!   constants `PLOT_OPTIONS`, `SET_GRAPH_OPTIONS`, `COLUMN_META_FIELDS`);
//! - **returns**: the keys of the returned dictionaries, on a sample project;
//! - **enumerated values**: each value of the list is accepted (as input) or
//!   returned (as output), and a value outside the list is refused.
//!
//! A change of surface requires an explicit decision and an update of the
//! list, in the same commit as the code; the failure message says so.
//! It replaces the former `la_surface_expose_vingt_huit_appels_et_quatre_erreurs`.
#![cfg(feature = "python")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::Session;

/// What the failure message reminds whoever touches the surface of.
const GUIDANCE: &str = "The `pyst` surface no longer matches the frozen list \
\"Frozen API V1\" of docs/python-and-scripting.md (#331). A change of surface \
(call, parameter, default, exception, key, enumerated value) requires an \
explicit DECISION and an update of this list in the same commit as the code.";

// --- The list, read from the doc ------------------------------------------------

struct Block {
    kind: String,
    name: String,
    lines: Vec<String>,
}

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/python-and-scripting.md")
}

/// All the ```` ```pyst-api <kind> <name> ```` blocks of the doc, in order.
fn blocks() -> Vec<Block> {
    let text = std::fs::read_to_string(doc_path()).expect("docs/python-and-scripting.md readable");
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    for line in text.lines() {
        if let Some(block) = current.as_mut() {
            if line.trim_end() == "```" {
                blocks.push(current.take().expect("open block"));
            } else if !line.trim().is_empty() {
                block.lines.push(line.trim_end().to_owned());
            }
        } else if let Some(header) = line.strip_prefix("```pyst-api ") {
            let mut words = header.split_whitespace();
            let kind = words.next().expect("kind of the pyst-api block").to_owned();
            let name = words.next().expect("name of the pyst-api block").to_owned();
            assert!(words.next().is_none(), "unexpected block header: {line}");
            current = Some(Block {
                kind,
                name,
                lines: Vec::new(),
            });
        }
    }
    assert!(current.is_none(), "pyst-api block not closed");
    blocks
}

/// The lines of the block `kind name`, without duplicates; `None` if the block is missing.
fn block(blocks: &[Block], kind: &str, name: &str) -> Option<BTreeSet<String>> {
    let mut found = blocks.iter().filter(|b| b.kind == kind && b.name == name);
    let first = found.next()?;
    assert!(
        found.next().is_none(),
        "pyst-api block {kind} {name} present twice"
    );
    let lines: BTreeSet<String> = first.lines.iter().cloned().collect();
    assert_eq!(
        lines.len(),
        first.lines.len(),
        "duplicate in the pyst-api block {kind} {name}"
    );
    Some(lines)
}

fn names(blocks: &[Block], kind: &str) -> BTreeSet<String> {
    blocks
        .iter()
        .filter(|b| b.kind == kind)
        .map(|b| b.name.clone())
        .collect()
}

/// Adds to `gaps` what is missing and what is extra between `expected` (the
/// doc) and `actual` (the module).
fn compare(
    gaps: &mut Vec<String>,
    title: &str,
    expected: Option<BTreeSet<String>>,
    actual: &BTreeSet<String>,
) {
    let Some(expected) = expected else {
        gaps.push(format!("{title}: block missing from the doc"));
        return;
    };
    let extra: Vec<_> = actual.difference(&expected).collect();
    let missing: Vec<_> = expected.difference(actual).collect();
    if !extra.is_empty() {
        gaps.push(format!("{title}: in the code, not in the doc: {extra:?}"));
    }
    if !missing.is_empty() {
        gaps.push(format!("{title}: in the doc, not in the code: {missing:?}"));
    }
}

fn require_no_gap(gaps: Vec<String>) {
    assert!(gaps.is_empty(), "{GUIDANCE}\n  - {}", gaps.join("\n  - "));
}

// --- The bench --------------------------------------------------------------------

struct Bench {
    engine: Arc<ScriptEngine>,
    _actor: Session,
}

/// `[Book1]Sheet1`: `A` = 1, 2, 3 and `B` = 4, 5, 6; a folder, a matrix and a
/// note at the root (the kinds that `pyst` cannot create).
fn bench() -> Bench {
    let mut project = Project::new("Surface").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .unwrap();
    project.add_folder(root, "Folder").unwrap();
    project.add_matrix(root, Some("Mat")).unwrap();
    project.add_note(root, Some("Note1"), "text").unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(actor.handle(), EmbeddedPython::with_home(prefix)).unwrap());
    Bench {
        engine,
        _actor: actor,
    }
}

/// Common script setup: handles, long columns (smoothing), a graph `Gen`, a
/// temporary folder.
const PREAMBLE: &str = "
import os, tempfile
a = pyst.resolve('[Book1]Sheet1!A')
b = pyst.resolve('[Book1]Sheet1!B')
sh = pyst.resolve('[Book1]Sheet1')
big = pyst.create_column(sh)
pyst.set_values(big, [float((i * i) % 7) for i in range(30)])
bigx = pyst.create_column(sh)
pyst.set_values(bigx, [float(i) for i in range(30)])
g = pyst.plot(b, a)
tmp = tempfile.mkdtemp()
";

/// Runs `script` (after [`PREAMBLE`]) and returns its `LABEL|rest` lines
/// grouped by label.
fn output(bench: &Bench, script: &str) -> BTreeMap<String, Vec<String>> {
    let source = format!("{PREAMBLE}{script}");
    let outcome = bench.engine.run(&source).expect("engine available");
    assert!(
        outcome.error.is_none(),
        "the guard script failed: {:?}\n{}",
        outcome.error,
        outcome.stdout
    );
    let mut output: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in outcome.stdout.lines() {
        if let Some((label, rest)) = line.split_once('|') {
            output
                .entry(label.to_owned())
                .or_default()
                .push(rest.to_owned());
        }
    }
    output
}

fn to_set(list: &str) -> BTreeSet<String> {
    list.split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn one<'a>(output: &'a BTreeMap<String, Vec<String>>, label: &str) -> &'a str {
    let lines = output
        .get(label)
        .unwrap_or_else(|| panic!("label {label} missing from the output"));
    assert_eq!(lines.len(), 1, "label {label}: {lines:?}");
    &lines[0]
}

/// Python literal (list of strings) for identifier values.
fn python_list(values: &BTreeSet<String>) -> String {
    let elements: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
    format!("[{}]", elements.join(", "))
}

// --- 1. Names, kinds, signatures ----------------------------------------------

/// Two defaults that PyO3 cannot write (a tuple, a constant): Python returns
/// `Ellipsis`. The doc carries the real value; it is matched here.
const ELLIPSIS_DEFAULTS: &[(&str, &str)] = &[
    ("panel=(0, 0)", "panel=Ellipsis"),
    ("png_scale=4.0", "png_scale=Ellipsis"),
];

fn as_python(line: &str) -> String {
    let mut line = line.to_owned();
    if line.starts_with("function ") {
        for (doc, python) in ELLIPSIS_DEFAULTS {
            line = line.replace(doc, python);
        }
    }
    line
}

const SCRIPT_SURFACE: &str = "
import inspect
for modname, mod in (('pyst', pyst), ('pyst.analysis', pyst.analysis)):
    for n in sorted(dir(mod)):
        if n.startswith('_'):
            continue
        o = getattr(mod, n)
        if inspect.ismodule(o):
            line = 'module ' + n
        elif inspect.isclass(o) and issubclass(o, BaseException):
            line = ('exception ' if issubclass(o, Exception) else 'exception-not-Exception ') + n
        elif inspect.isclass(o):
            line = 'class ' + n
        elif callable(o):
            try:
                line = 'function ' + n + str(inspect.signature(o))
            except Exception as e:
                line = 'function %s<unreadable signature: %r>' % (n, e)
        else:
            line = 'other ' + n
        print(modname + '|' + line)
no_doc = [m.__name__ + '.' + n for m in (pyst, pyst.analysis) for n in dir(m)
            if not n.startswith('_') and callable(getattr(m, n)) and not getattr(getattr(m, n), '__doc__', None)]
no_doc += [m.__name__ for m in (pyst, pyst.analysis) if not m.__doc__]
print('NODOC|' + ','.join(no_doc))
print('HANDLE|' + ','.join(n for n in dir(pyst.Handle) if not n.startswith('_')))
";

#[test]
fn the_pyst_surface_is_the_frozen_list() {
    let bench = bench();
    let blocks = blocks();
    let output = output(&bench, SCRIPT_SURFACE);
    let mut gaps = Vec::new();

    for module in ["pyst", "pyst.analysis"] {
        let actual: BTreeSet<String> = output
            .get(module)
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default();
        // An unreadable signature would make the test pass vacuously: refuse outright.
        let unreadable: Vec<_> = actual.iter().filter(|l| l.contains("unreadable")).collect();
        assert!(
            unreadable.is_empty(),
            "{GUIDANCE}\ninspect.signature failed: {unreadable:?}"
        );
        let functions = actual.iter().filter(|l| l.starts_with("function ")).count();
        assert!(
            functions >= if module == "pyst" { 30 } else { 8 },
            "{module}: only {functions} functions, the guard would pass vacuously"
        );
        let expected = block(&blocks, "module", module)
            .map(|b| b.iter().map(|l| as_python(l)).collect::<BTreeSet<_>>());
        compare(&mut gaps, &format!("module {module}"), expected, &actual);
    }

    // The former guard also required a docstring on each call (in French, S8).
    let no_doc = one(&output, "NODOC");
    if !no_doc.is_empty() {
        gaps.push(format!("without docstring: {no_doc}"));
    }

    let public_attrs = one(&output, "HANDLE");
    if !public_attrs.is_empty() {
        gaps.push(format!(
            "Handle must stay opaque, public attributes: {public_attrs}"
        ));
    }
    require_no_gap(gaps);
}

// --- 2. Option keys -----------------------------------------------------------

/// For each call with options: the list given by the "unknown key" message,
/// and the keys of the doc that the call does not recognize.
const SCRIPT_OPTIONS: &str = "
import re
calls = {
    'plot': lambda k: pyst.plot(b, a, **{k: 0}),
    'set_graph': lambda k: pyst.set_graph(g, **{k: 0}),
    'set_column_meta': lambda k: pyst.set_column_meta(big, **{k: 0}),
}
doc = {'plot': @PLOT@, 'set_graph': @SET_GRAPH@, 'set_column_meta': @META@}
for name, call in calls.items():
    try:
        call('nonexistent_key_zzz')
        print('ACCEPTED|%s|accepted without error' % name)
    except pyst.InvalidValueError as e:
        m = re.search(r'\\(accepted: (.*)\\)', str(e))
        print('ACCEPTED|%s|%s' % (name, m.group(1).replace(' ', '') if m else 'list missing: ' + str(e)))
    rejected = []
    for cle in doc[name]:
        try:
            call(cle)
        except pyst.InvalidValueError as e:
            if str(e).startswith(('unknown plot option', 'unknown graph option', 'unknown column metadata field')):
                rejected.append(cle)
        except Exception:
            pass
    print('REJECTED|%s|%s' % (name, ','.join(rejected)))
";

#[test]
fn the_option_keys_are_those_of_the_frozen_list() {
    let bench = bench();
    let blocks = blocks();
    let mut gaps = Vec::new();
    let mut script = SCRIPT_OPTIONS.to_owned();
    for (marker, call) in [
        ("@PLOT@", "plot"),
        ("@SET_GRAPH@", "set_graph"),
        ("@META@", "set_column_meta"),
    ] {
        let keys = block(&blocks, "options", call).unwrap_or_default();
        script = script.replace(marker, &python_list(&keys));
    }
    let output = output(&bench, &script);

    for call in ["plot", "set_graph", "set_column_meta"] {
        let expected = block(&blocks, "options", call);
        let announced = output["ACCEPTED"]
            .iter()
            .find_map(|l| l.strip_prefix(&format!("{call}|")))
            .unwrap_or_else(|| panic!("ACCEPTED|{call} missing"));
        if announced.contains("list missing") || announced.contains("without error") {
            gaps.push(format!("options {call}: {announced}"));
            continue;
        }
        compare(
            &mut gaps,
            &format!("options {call} (list of the error message)"),
            expected,
            &to_set(announced),
        );
        let rejected = output["REJECTED"]
            .iter()
            .find_map(|l| l.strip_prefix(&format!("{call}|")))
            .unwrap_or_default();
        if !rejected.is_empty() {
            gaps.push(format!(
                "options {call}: doc keys that the code does not recognize: {rejected}"
            ));
        }
    }
    require_no_gap(gaps);
}

// --- 3. Keys of the returned dictionaries ------------------------------------------------

const SCRIPT_RETURNS: &str = "
def keys(d):
    return ','.join(sorted(d.keys()))
r = pyst.info(a)
print('info|' + keys(r))
print('info.column|' + keys(r['column']))
two = pyst.plot([a, b], None)
print('series.item|' + keys(pyst.series(two)[0]))
print('series_groups.item|' + keys(pyst.series_groups(two)[0]))
r = pyst.analysis.baseline(b, a, method='line', points=[(1, 1), (3, 3)], keep_baseline=True)
print('analysis|' + keys(r))
print('analysis.baseline.columns|' + keys(r['columns']))
r = pyst.analysis.correct(b, a)
print('analysis.correct.columns|' + keys(r['columns']))
r = pyst.analysis.smooth(big, bigx, window=3)
print('analysis.smooth.columns|' + keys(r['columns']))
r = pyst.analysis.baseline([a, b], a, method='line', points=[(1, 1), (3, 3)])
print('analysis.batch|' + keys(r))
print('analysis.batch.item|' + keys(r['series'][0]))
print('readiness|' + keys(pyst.analysis.readiness('fit', model='linear')))
print('readiness.needs_points|' + keys(pyst.analysis.readiness('baseline', method='anchors')))
";

#[test]
fn the_returned_dictionary_keys_are_those_of_the_frozen_list() {
    let bench = bench();
    let blocks = blocks();
    let output = output(&bench, SCRIPT_RETURNS);
    let mut gaps = Vec::new();
    let mut checked = BTreeSet::new();
    for (label, lines) in &output {
        // The keys of a batch of series are those of a single result.
        let name = if label == "analysis.batch.item" {
            "analysis"
        } else {
            label.as_str()
        };
        checked.insert(name.to_owned());
        compare(
            &mut gaps,
            &format!("return {label}"),
            block(&blocks, "returns", name),
            &to_set(&lines[0]),
        );
    }
    // No "returns" block of the doc may escape the test.
    let documented = names(&blocks, "returns");
    let forgotten: Vec<_> = documented.difference(&checked).collect();
    if !forgotten.is_empty() {
        gaps.push(format!(
            "`returns` blocks of the doc that the test does not check: {forgotten:?}"
        ));
    }
    require_no_gap(gaps);
}

// --- 4. Enumerated values ------------------------------------------------------------

/// Enumerations **accepted as input**: the body of `attempt(v)` must succeed
/// for each value of the doc; with `refusal`, an invented value must raise
/// `InvalidValueError`.
struct Input {
    name: &'static str,
    body: &'static str,
    refusal: bool,
}

const INPUTS: &[Input] = &[
    Input {
        name: "column_type",
        body: "c = pyst.create_column(sh, None, column_type=v)\n    pyst.set_column_type(c, v)",
        refusal: true,
    },
    Input {
        name: "role",
        body: "c = pyst.create_column(sh)\n    pyst.set_column_meta(c, role=v)\n    assert pyst.info(c)['column']['role'] == v",
        refusal: true,
    },
    Input {
        name: "plot_type",
        body: "pyst.plot(b, a, plot_type=v)",
        refusal: true,
    },
    Input {
        name: "line_style",
        body: "pyst.plot(b, a, line_style=v)",
        refusal: true,
    },
    Input {
        name: "symbol",
        body: "pyst.plot(b, a, symbol=v)",
        refusal: true,
    },
    Input {
        name: "scale",
        body: "pyst.plot(b, a, x_scale=v, y_scale=v)\n    pyst.set_graph(g, x_scale=v, y_scale=v)",
        refusal: true,
    },
    Input {
        name: "axis",
        body: "pyst.plot(b, a, axis=v)",
        refusal: true,
    },
    Input {
        name: "legend_position",
        body: "pyst.set_graph(g, legend_position=v)",
        refusal: true,
    },
    Input {
        name: "shape_kind",
        body: "pyst.add_shape(g, v, 0, 0, 1, 1)",
        refusal: true,
    },
    Input {
        name: "export_extension",
        body: "pyst.export_graph(g, os.path.join(tmp, 'f.' + v))",
        refusal: true,
    },
    Input {
        name: "analysis_kind",
        body: "pyst.analysis.readiness(v)",
        refusal: true,
    },
    Input {
        name: "baseline_method",
        body: "kw = {'constant': {'points': [(1.0, 1.0)]}, 'anchors': {'points': [(1.0, 1.0), (2.0, 2.0)]}, 'line': {'points': [(1.0, 1.0), (3.0, 3.0)]}, 'zones': {'zones': [(1.0, 2.0)]}}.get(v, {})\n    pyst.analysis.readiness('baseline', method=v, **kw)",
        refusal: true,
    },
    Input {
        name: "baseline_op",
        body: "pyst.analysis.readiness('baseline', method='anchors', points=[(1.0, 1.0), (2.0, 2.0)], op=v)",
        refusal: true,
    },
    Input {
        name: "integrate_under",
        body: "pyst.analysis.readiness('integrate', under=v)",
        refusal: true,
    },
    Input {
        name: "smooth_method",
        body: "pyst.analysis.smooth(big, bigx, method=v, window=5)",
        refusal: true,
    },
    // An unknown text is read there as a formula: no refusal to check.
    Input {
        name: "fit_model",
        body: "pyst.analysis.readiness('fit', model=v)",
        refusal: false,
    },
];

/// Enumerations **returned as output** (the test observes them).
const OUTPUTS: &[&str] = &["kind", "type", "role", "readiness_reason"];

/// Values that no script can produce: excluded from the completeness check.
const UNPRODUCIBLE: &[(&str, &str)] = &[("kind", "unknown")];

const SCRIPT_OBSERVE: &str = "
pyst.create_column(sh, 'Mixed', column_type='mixed')
pyst.create_column(sh, 'Date', column_type='datetime')
seen = {'kind': set(), 'type': set(), 'role': set()}
def walk(h):
    i = pyst.info(h)
    seen['kind'].add(i['kind'])
    if i['column']:
        seen['type'].add(i['column']['type'])
        seen['role'].add(i['column']['role'])
    for child in pyst.children(h):
        walk(child)
walk(pyst.root())
for name in sorted(seen):
    print('OBS|%s|%s' % (name, ','.join(sorted(seen[name]))))
reasons = set()
for call in (
    lambda: pyst.analysis.readiness('baseline', method='anchors'),
    lambda: pyst.analysis.readiness('baseline', method='anchors', points=[(1.0, 1.0), (2.0, 2.0)]),
    lambda: pyst.analysis.readiness('integrate', ranges=[(float('nan'), float('nan'))]),
    lambda: pyst.analysis.readiness('integrate'),
    lambda: pyst.analysis.readiness('fit', model='x +'),
    lambda: pyst.analysis.readiness('fit', model='3'),
    lambda: pyst.analysis.readiness('fit', model='polynomial', degree=0),
):
    try:
        r = call()['reason']
        if r is not None:
            reasons.add(r)
    except pyst.InvalidValueError:
        pass
print('OBS|readiness_reason|%s' % ','.join(sorted(reasons)))
";

#[test]
fn the_enumerated_values_are_those_of_the_frozen_list() {
    let bench = bench();
    let blocks = blocks();
    let mut gaps = Vec::new();

    // Coverage: each enumeration of the doc has a trial, and conversely.
    let documented = names(&blocks, "enum");
    let covered: BTreeSet<String> = INPUTS
        .iter()
        .map(|e| e.name.to_owned())
        .chain(OUTPUTS.iter().map(|s| (*s).to_owned()))
        .collect();
    compare(
        &mut gaps,
        "enumerations covered by the test",
        Some(documented),
        &covered,
    );

    // As input.
    for entree in INPUTS {
        let Some(values) = block(&blocks, "enum", entree.name) else {
            continue;
        };
        let script = format!(
            "def attempt(v):\n    {body}\n\
             refused_values = []\n\
             for v in {values}:\n    try:\n        attempt(v)\n    except Exception as e:\n        refused_values.append('%s: %r' % (v, e))\n\
             print('REFUSED|' + ' ; '.join(refused_values))\n\
             if {refusal}:\n    try:\n        attempt('invented_value_zzz')\n        print('INVENTED|accepted')\n    except pyst.InvalidValueError:\n        print('INVENTED|refused')\n    except Exception as e:\n        print('INVENTED|other error %r' % e)\n\
             else:\n    print('INVENTED|refused')\n",
            body = entree.body,
            values = python_list(&values),
            refusal = if entree.refusal { "True" } else { "False" },
        );
        let output = output(&bench, &script);
        let refused_values = one(&output, "REFUSED");
        if !refused_values.is_empty() {
            gaps.push(format!(
                "enum {}: doc values that the code refuses: {refused_values}",
                entree.name
            ));
        }
        let invented = one(&output, "INVENTED");
        if invented != "refused" {
            gaps.push(format!(
                "enum {}: a value outside the list is not refused ({invented})",
                entree.name
            ));
        }
    }

    // As output.
    let output = output(&bench, SCRIPT_OBSERVE);
    let observed: BTreeMap<&str, BTreeSet<String>> = output["OBS"]
        .iter()
        .filter_map(|l| l.split_once('|'))
        .map(|(name, values)| (name, to_set(values)))
        .collect();
    for name in OUTPUTS {
        let Some(documented) = block(&blocks, "enum", name) else {
            gaps.push(format!("enum {name}: block missing from the doc"));
            continue;
        };
        let seen = observed.get(name).cloned().unwrap_or_default();
        let off_list: Vec<_> = seen.difference(&documented).collect();
        if !off_list.is_empty() {
            gaps.push(format!(
                "enum {name}: values returned by the code, not in the doc: {off_list:?}"
            ));
        }
        // Completeness for the enumerations that the sample can all produce.
        if matches!(*name, "kind" | "type") {
            let expected_values: Vec<_> = documented
                .iter()
                .filter(|v| !UNPRODUCIBLE.contains(&(*name, v.as_str())))
                .filter(|v| !seen.contains(*v))
                .collect();
            if !expected_values.is_empty() {
                gaps.push(format!(
                    "enum {name}: doc values never returned by the code: {expected_values:?}"
                ));
            }
        }
    }
    require_no_gap(gaps);
}
