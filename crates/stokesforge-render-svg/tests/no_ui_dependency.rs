//! The export's boundaries, checked by the compiler rather than by
//! review — same discipline as
//! `stokesforge-core/tests/no_ui_dependency.rs` and
//! `stokesforge-plot/tests/no_ui_dependency.rs`.
//!
//! Three properties, and the third is the most important of the three because
//! it is the only one whose failure would be invisible.

use std::collections::{BTreeMap, BTreeSet};

/// Allowed direct dependencies. Any addition to `Cargo.toml` must be
/// added here: this is what forces a conscious decision rather than
/// an import that slips in.
const ALLOWED_DIRECT_DEPENDENCIES: &[&str] = &[
    "stokesforge-core",   // ResolvedPlot, Tree
    "stokesforge-plot",   // PlotSpec
    "stokesforge-render", // shared text measurement + common pipeline
    "plotters",           // feature svg_backend
    "plotters-backend",
    "thiserror",
    "svg2pdf", // R6: SVG → PDF conversion, expected by architecture-choisie.md §8
    "resvg",   // PNG export (post-roadmap addition): same pattern, SVG → PNG conversion
    // R8, feature `typst` (optional, absent from `default`): the SVG
    // fragment of a compiled formula (8.4). `typst` and `typst-svg` enter
    // the real tree only with `--features typst`; their mere presence
    // in `[dependencies]` (with `optional = true`) is enough to have their
    // name listed here, feature enabled or not — this test reads `Cargo.toml`,
    // not the tree resolved for a given combination of features.
    "typst",
    "typst-svg",
    // #162: the outline of a glyph with no Unicode character (per-glyph
    // fallback of the text emitter); already in the tree via `typst`.
    "ttf-parser",
];

/// What an export crate must never pull in, even remotely.
///
/// "The renderer is tested **without a graphics server**, through the SVG
/// export" (`roadmap-rendu.md` §9): this list is what makes the sentence
/// true. `epaint` is not in it — it is explicitly wanted (§4.4: the
/// `estimate_text_size` override is mandatory here too), and that is the whole
/// subtlety of the split: egui's measurement without egui's stack.
const FORBIDDEN: &[&str] = &[
    "egui",
    "eframe",
    "egui-winit",
    "egui-wgpu",
    "egui_extras",
    "egui_glow",
    "winit",
    "wgpu",
    "glow",
];

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the crate lives in crates/<name>")
        .to_path_buf()
}

fn lock() -> toml::Value {
    let path = workspace_root().join("Cargo.lock");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} unreadable: {err}", path.display()))
        .parse()
        .expect("valid Cargo.lock")
}

/// The `package → dependencies` graph as the lock file freezes it.
///
/// Cargo puts development dependencies in the same list: the
/// check is therefore stricter than "normal dependencies", which is
/// exactly what we want here (a test that pulled in egui would make
/// `cargo tree -p stokesforge-render-svg` lie).
fn graph(lock: &toml::Value) -> BTreeMap<String, Vec<String>> {
    let mut graph = BTreeMap::new();
    let Some(packages) = lock.get("package").and_then(|p| p.as_array()) else {
        panic!("Cargo.lock without a [[package]] section");
    };
    for package in packages {
        let Some(name) = package.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let deps = package
            .get("dependencies")
            .and_then(|d| d.as_array())
            .map(|liste| {
                liste
                    .iter()
                    .filter_map(|d| d.as_str())
                    // A lock entry can be "name version" or
                    // "name version (source)" when several versions
                    // coexist.
                    .map(|d| d.split_whitespace().next().unwrap_or(d).to_string())
                    .collect()
            })
            .unwrap_or_default();
        graph.insert(name.to_string(), deps);
    }
    graph
}

fn closure(graph: &BTreeMap<String, Vec<String>>, start: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start.to_string()];
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(deps) = graph.get(&name) {
            stack.extend(deps.iter().cloned());
        }
    }
    seen
}

#[test]
fn only_the_expected_direct_dependencies_are_declared() {
    let manifest: toml::Value =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("readable Cargo.toml")
            .parse()
            .expect("valid Cargo.toml");
    let deps = manifest
        .get("dependencies")
        .and_then(|d| d.as_table())
        .expect("[dependencies] section");
    for name in deps.keys() {
        assert!(
            ALLOWED_DIRECT_DEPENDENCIES.contains(&name.as_str()),
            "dependency `{name}` not listed: the export must stay free of a UI stack \
             (architecture-choisie.md §8)"
        );
    }
}

#[test]
fn neither_egui_nor_eframe_nor_winit_even_transitively() {
    let lock = lock();
    let graph = graph(&lock);
    let reachable = closure(&graph, "stokesforge-render-svg");
    let offenders: Vec<&str> = FORBIDDEN
        .iter()
        .copied()
        .filter(|name| reachable.contains(*name))
        .collect();
    assert!(
        offenders.is_empty(),
        "stokesforge-render-svg reaches {offenders:?} — the export would no longer be \
         testable without a graphics server, and the boundary of §8 would be lost"
    );
}

#[test]
fn epaint_is_present_and_in_a_single_copy() {
    let lock = lock();
    let graph = graph(&lock);
    assert!(
        closure(&graph, "stokesforge-render-svg").contains("epaint"),
        "stokesforge-render-svg no longer reaches epaint: it therefore no longer measures \
         like the screen, and `estimate_text_size` has fallen back to the \
         byte-based heuristic (architecture-choisie.md §4.4)"
    );

    // **The test that matters.** Two versions of `epaint` in the lock file — one
    // pulled in by `egui` for the screen backend, the other directly for the
    // export — would be two font sets, hence two measurements, hence shifted
    // axes. And nothing, absolutely nothing, would signal it: the agreement
    // test itself would pass, since it builds both its measurers
    // from the same compiled crate.
    let packages = lock
        .get("package")
        .and_then(|p| p.as_array())
        .expect("[[package]] section");
    let versions: Vec<&str> = packages
        .iter()
        .filter(|p| p.get("name").and_then(|n| n.as_str()) == Some("epaint"))
        .filter_map(|p| p.get("version").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(
        versions.len(),
        1,
        "epaint resolved to several versions ({versions:?}): the screen \
         backend and the export would measure with different fonts without \
         anything saying so"
    );
}
