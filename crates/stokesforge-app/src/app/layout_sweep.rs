//! A sweep for clipped text (#521): the dialogs that open with a bare action,
//! in `en`, `fr` and the pseudo-locale `en-XA`, at their class size. (The
//! smallest window size, 220 x 140, cuts every title and tab strip by design:
//! tried, left out.)
//!
//! For every text a dialog paints that the shell behind it did not paint
//! already: the galley is not elided, and its right edge stays inside the
//! clip rectangle it is drawn with and inside the dialog. Only the horizontal
//! edge is checked: a scroll area clips vertically by design. The shell is swept alone at three
//! widths. The pseudo-locale only counts what it cuts (a third longer than any
//! real language: about forty texts today), it does not fail on it.
//!
//! The language is a process global (`docs/tests.md`): the sweep runs in one
//! child process per language, like
//! `plot_settings::tests::each_tab_draws_in_english_and_in_the_pseudo_locale`,
//! and the parent reads the line "layout sweep in <tag>: " to know the child
//! ran in the right language.
//!
//! Left out because they need a fixture: the analysis windows, the import
//! wizard, the confirmations, the series window, the column metadata window and
//! the Preferences tabs other than the first.

use eframe::egui;
use stokesforge_app_state::i18n::{self, Language, LANGUAGE_VARIABLE};
use stokesforge_app_state::state::{Action, PlotSettingsTab};

use super::test_support::bench::{Bench, Pass};

/// Clippings found and not fixed here: `(case prefix, text prefix, reason)`.
/// A case matches when its name starts with the first field and the text with
/// the second. Each entry names a defect, so that nobody mistakes it for a
/// rule.
const KNOWN_FAILURES: &[(&str, &str, &str)] = &[
    (
        "graph properties Axes",
        "Afficher les étiquettes de graduation",
        "#525: fr Axes page, the tick labels checkbox text runs past the dialog edge",
    ),
    (
        "graph properties Axes",
        "Trait de l'axe, graduations et étiquettes",
        "#525: fr Axes page, the axis line hint runs past the dialog edge",
    ),
];

/// The dialogs: a name and the action that opens each of them on the bench
/// project.
fn dialogs(bench: &Bench) -> Vec<(String, Action)> {
    let mut all: Vec<(String, Action)> = PlotSettingsTab::ALL
        .iter()
        .map(|tab| {
            (
                format!("graph properties {tab:?}"),
                Action::OpenPlotSettings {
                    graph: bench.graph,
                    tab: *tab,
                    axis: None,
                },
            )
        })
        .collect();
    all.push(("preferences".into(), Action::OpenPreferences));
    all.push(("shortcuts".into(), Action::OpenKeyboardShortcuts));
    all.push(("formula functions".into(), Action::OpenFormulaFunctions));
    all.push(("about".into(), Action::OpenAbout));
    all.push(("status history".into(), Action::OpenStatusHistory));
    all
}

/// A painted text with the rectangles that bound it.
struct Painted {
    text: String,
    elided: bool,
    left: f32,
    right: f32,
    clip: egui::Rect,
}

fn settle(bench: &mut Bench, size: egui::Vec2) -> egui::FullOutput {
    let mut output = bench.root(Pass::root().focused(true).size(size.x, size.y));
    for _ in 0..4 {
        output = bench.root(Pass::root().focused(true).size(size.x, size.y));
    }
    output
}

fn paints(output: &egui::FullOutput) -> Vec<Painted> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                Some(Painted {
                    text: text.galley.job.text.clone(),
                    elided: text.galley.elided,
                    left: rect.left(),
                    right: rect.right(),
                    clip: clipped.clip_rect,
                })
            }
            _ => None,
        })
        .collect()
}

/// The rectangle of the dialog: the smallest foreground area (the invisible
/// backdrop is as large as the screen).
fn dialog_rect(bench: &Bench) -> Option<egui::Rect> {
    bench.ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == egui::Order::Foreground)
            .filter_map(|layer| memory.area_rect(layer.id).map(|rect| (layer, rect)))
            .map(|(_, rect)| rect)
            .min_by(|a, b| a.area().total_cmp(&b.area()))
    })
}

fn is_known(case: &str, text: &str) -> bool {
    KNOWN_FAILURES
        .iter()
        .any(|(c, t, _)| case.starts_with(c) && text.starts_with(t))
}

/// What is wrong with the texts `now` painted, the shell's own (`before`)
/// left out.
fn problems(
    case: &str,
    now: &[Painted],
    before: &[Painted],
    window: Option<egui::Rect>,
) -> Vec<String> {
    let mut found = Vec::new();
    for item in now {
        let known = before
            .iter()
            .any(|old| old.text == item.text && (old.left - item.left).abs() < 0.5);
        if known || item.text.is_empty() || is_known(case, &item.text) {
            continue;
        }
        let limit = window.map_or(item.clip.right(), |rect| rect.right());
        if item.elided {
            found.push(format!("{case}: {:?} is elided", item.text));
        } else if item.right > item.clip.right() + 0.5 {
            found.push(format!(
                "{case}: {:?} is cut by its clip rectangle ({} > {})",
                item.text,
                item.right,
                item.clip.right()
            ));
        } else if item.right > limit + 0.5 {
            found.push(format!(
                "{case}: {:?} leaves the window ({} > {limit})",
                item.text, item.right
            ));
        }
    }
    found
}

#[test]
#[ignore = "run in a subprocess by the_dialogs_clip_no_text_in_each_language"]
fn the_dialogs_clip_no_text_in_the_process_language() {
    let language = i18n::language();
    let size = egui::vec2(1400.0, 900.0);
    let mut found = Vec::new();
    let mut checked = 0;

    let names: Vec<String> = dialogs(&Bench::new()).into_iter().map(|d| d.0).collect();
    for (index, name) in names.iter().enumerate() {
        let mut bench = Bench::new();
        let before = paints(&settle(&mut bench, size));
        let action = dialogs(&bench).remove(index).1;
        bench.app.state.dispatch(action);
        let output = settle(&mut bench, size);
        let window = dialog_rect(&bench);
        let now = paints(&output);
        checked += now.len();
        found.extend(problems(
            &format!("{name} at class size"),
            &now,
            &before,
            window,
        ));
        bench.finish();
    }
    for width in [1100.0, 1400.0, 1920.0] {
        let mut bench = Bench::new();
        let now = paints(&settle(&mut bench, egui::vec2(width, 900.0)));
        checked += now.len();
        found.extend(problems(&format!("shell at {width} wide"), &now, &[], None));
        bench.finish();
    }
    if language == Language::PseudoEnglish {
        // The pseudo-locale lengthens every text by a third: a Auto combo or a
        // wide label is cut there although no real language cuts it. Counted,
        // not enforced; the tab test enforces the framing marks.
        println!("pseudo-locale texts cut by the layout: {}", found.len());
    } else {
        assert!(found.is_empty(), "{}", found.join("\n"));
    }
    println!("layout sweep in {}: {checked} texts", language.tag());
}

/// Reruns the sweep in a child process per language. The child must succeed
/// **and** say it ran in the requested language. Witness: shrinking
/// `DialogNature` `Small` from 480 to 300 in `dialogs.rs` makes it fail.
#[test]
fn the_dialogs_clip_no_text_in_each_language() {
    let executable = std::env::current_exe().unwrap();
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, rest)| rest);
    let name = format!("{module}::the_dialogs_clip_no_text_in_the_process_language");
    for language in Language::ALL {
        let output = std::process::Command::new(&executable)
            .args([
                name.as_str(),
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(LANGUAGE_VARIABLE, language.tag())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{}:\n{stdout}\n{stderr}",
            language.tag()
        );
        assert!(
            stdout.contains(&format!("layout sweep in {}: ", language.tag())),
            "{}: the child did not run in this language:\n{stdout}",
            language.tag()
        );
    }
}
