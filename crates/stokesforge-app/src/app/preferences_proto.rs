//! PROTOTYPE (test only) — the "advanced" Preferences window of #148,
//! assembled **without logic**: an in-memory `Proto` draft, no session,
//! no `preferences.json`. It uses the real building blocks (`modal_dialog_with`,
//! `tab_strip`, `form`, `choice`, `number_field`) so that its rendering is that
//! of the application, and is driven with `egui_kittest` (clicks by label,
//! reference screenshots in `tests/snapshots/`).
//!
//! Many settings below **do not exist yet** in StokesForge
//! (theme, interface scale, autosave, default folders,
//! copy formats…): they serve to judge the shape, not to promise the substance.
//! The labels are hard-coded because the module delivers nothing (it is only
//! compiled in test); a delivered version would go through `tr!` (en/fr).

use eframe::egui;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize,
};
use super::form::{choice, form, ChoiceOption};
use super::hover_help::help_beside;
use super::reorder_list::{move_before, reorder_move, reorder_row};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use stokesforge_app_state::quantities::Quantity;

/// The sections, in the order of #148 (+ Analysis, decision M5 of #181).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Page {
    General,
    Display,
    Export,
    Folders,
    Analysis,
}

impl Page {
    const ALL: [Page; 5] = [
        Page::General,
        Page::Display,
        Page::Export,
        Page::Folders,
        Page::Analysis,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Display => "Display",
            Page::Export => "Export",
            Page::Folders => "Folders and Paths",
            Page::Analysis => "Analysis",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Language {
    English,
    French,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Theme {
    System,
    Light,
    Dark,
}

/// At start, the panels take back their last state or a fixed state
/// (question 1 of #148).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartMode {
    Remember,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SvgText {
    Text,
    Outlines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Format {
    Svg,
    Png,
    Pdf,
}

impl Format {
    fn label(self) -> &'static str {
        match self {
            Format::Svg => "SVG",
            Format::Png => "PNG",
            Format::Pdf => "PDF",
        }
    }
}

/// A format offered by Copy Graph, and whether it is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CopyFormat {
    pub(super) format: Format,
    pub(super) on: bool,
}

/// A default folder: the last used, or a fixed folder (question 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FolderRule {
    pub(super) fixed: bool,
    pub(super) path: String,
}

/// The preferences draft: everything the window can set.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Proto {
    // General
    pub(super) language: Language,
    pub(super) theme: Theme,
    pub(super) ui_scale: u32,
    pub(super) confirm_close: bool,
    pub(super) autosave: bool,
    pub(super) autosave_minutes: u32,
    pub(super) undo_depth: u32,
    // Display
    pub(super) toolbars: [bool; 3],
    pub(super) panels: [bool; 3],
    pub(super) panels_start: StartMode,
    pub(super) metadata_row: bool,
    pub(super) decimal_comma: bool,
    pub(super) reduce_motion: bool,
    // Export
    pub(super) png_scale: u32,
    pub(super) svg_text: SvgText,
    pub(super) default_format: Format,
    /// In the order in which Copy Graph offers them; `on` says whether it is offered.
    pub(super) copy_formats: Vec<CopyFormat>,
    // Folders: open, save, import, export
    pub(super) folders: [FolderRule; 4],
    // Analysis
    pub(super) keep_in_results: bool,
    pub(super) confidence_percent: u32,
    pub(super) max_iterations: u32,
}

impl Default for Proto {
    fn default() -> Self {
        let rule = FolderRule {
            fixed: false,
            path: String::new(),
        };
        Self {
            language: Language::English,
            theme: Theme::System,
            ui_scale: 100,
            confirm_close: true,
            autosave: false,
            autosave_minutes: 10,
            undo_depth: 200,
            toolbars: [true; 3],
            panels: [true, true, false],
            panels_start: StartMode::Remember,
            metadata_row: true,
            decimal_comma: false,
            reduce_motion: false,
            png_scale: 4,
            svg_text: SvgText::Text,
            default_format: Format::Svg,
            copy_formats: [Format::Svg, Format::Png, Format::Pdf]
                .map(|format| CopyFormat { format, on: true })
                .to_vec(),
            folders: [rule.clone(), rule.clone(), rule.clone(), rule],
            keep_in_results: false,
            confidence_percent: 95,
            max_iterations: 500,
        }
    }
}

const SCALE: Quantity = Quantity::whole(10.0).between(75.0, 200.0);
const MINUTES: Quantity = Quantity::whole(5.0).between(1.0, 120.0);
const UNDO: Quantity = Quantity::whole(50.0).between(10.0, 1000.0);
const CONFIDENCE: Quantity = Quantity::whole(1.0).between(50.0, 99.0);
const ITER: Quantity = stokesforge_app_state::quantities::ITERATIONS;

const FOLDER_KINDS: [&str; 4] = ["Open", "Save", "Import", "Export"];

/// The state of the window in the bench: draft, page, validated value.
pub(super) struct Bench {
    pub(super) draft: Proto,
    pub(super) page: Page,
    pub(super) applied: Option<Proto>,
    pub(super) closed: bool,
}

impl Bench {
    pub(super) fn new() -> Self {
        Self {
            draft: Proto::default(),
            page: Page::General,
            applied: None,
            closed: false,
        }
    }
}

/// Why "Apply" is greyed out, if it is (S-G1): a fixed folder without a path.
pub(super) fn apply_blocker(draft: &Proto) -> Option<String> {
    draft
        .folders
        .iter()
        .zip(FOLDER_KINDS)
        .find(|(rule, _)| rule.fixed && rule.path.trim().is_empty())
        .map(|(_, kind)| format!("Choose a folder for “{kind}”, or switch it to Last used."))
}

/// Paints the window (one frame). Returns the value validated by "Apply".
pub(super) fn preferences_proto(ctx: &egui::Context, bench: &mut Bench) {
    if bench.closed {
        return;
    }
    let blocker = apply_blocker(&bench.draft);
    let reset_page = std::cell::Cell::new(false);
    let Bench { draft, page, .. } = bench;
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("preferences_proto"),
        "Preferences",
        // FREE-SIZE(#300): test bench whose reference screenshots (tests/snapshots/preferences_proto_*.png) fix the size
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(560.0, 470.0))),
        DialogFooter::gated(
            "Apply",
            "Save the preferences and close.",
            blocker.is_none(),
            blocker.as_deref().unwrap_or(""),
        ),
        |ui| {
            if ui.button("Reset Page").clicked() {
                reset_page.set(true);
            }
        },
        |ui| {
            let tabs = Page::ALL.map(|page| (page, page.title().to_string()));
            tab_strip(
                ui,
                TabStripSide::Top,
                &tabs,
                *page,
                |picked| *page = picked,
                None,
                || {},
            );
            let current = *page;
            crate::theme::page_scroll_area().show(ui, |ui| match current {
                Page::General => general_page(ui, draft),
                Page::Display => display_page(ui, draft),
                Page::Export => export_page(ui, draft),
                Page::Folders => folders_page(ui, draft),
                Page::Analysis => analysis_page(ui, draft),
            });
        },
    );
    if reset_page.get() {
        let defaults = Proto::default();
        match bench.page {
            Page::General => {
                bench.draft.language = defaults.language;
                bench.draft.theme = defaults.theme;
                bench.draft.ui_scale = defaults.ui_scale;
                bench.draft.confirm_close = defaults.confirm_close;
                bench.draft.autosave = defaults.autosave;
                bench.draft.autosave_minutes = defaults.autosave_minutes;
                bench.draft.undo_depth = defaults.undo_depth;
            }
            Page::Display => {
                bench.draft.toolbars = defaults.toolbars;
                bench.draft.panels = defaults.panels;
                bench.draft.panels_start = defaults.panels_start;
                bench.draft.metadata_row = defaults.metadata_row;
                bench.draft.decimal_comma = defaults.decimal_comma;
                bench.draft.reduce_motion = defaults.reduce_motion;
            }
            Page::Export => {
                bench.draft.png_scale = defaults.png_scale;
                bench.draft.svg_text = defaults.svg_text;
                bench.draft.default_format = defaults.default_format;
                bench.draft.copy_formats = defaults.copy_formats.clone();
            }
            Page::Folders => bench.draft.folders = defaults.folders,
            Page::Analysis => {
                bench.draft.keep_in_results = defaults.keep_in_results;
                bench.draft.confidence_percent = defaults.confidence_percent;
                bench.draft.max_iterations = defaults.max_iterations;
            }
        }
    }
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => {
            bench.applied = Some(bench.draft.clone());
            bench.closed = true;
        }
        DialogOutcome::Cancel => bench.closed = true,
    }
}

fn general_page(ui: &mut egui::Ui, p: &mut Proto) {
    form(ui, "proto_general", |rows| {
        rows.section("Language and appearance");
        rows.row("Language", |ui| {
            choice(
                ui,
                "proto_language",
                &mut p.language,
                &[
                    ChoiceOption::new(Language::English, "English"),
                    ChoiceOption::new(Language::French, "Français"),
                ],
            )
        });
        rows.row("Theme", |ui| {
            choice(
                ui,
                "proto_theme",
                &mut p.theme,
                &[
                    ChoiceOption::new(Theme::System, "System"),
                    ChoiceOption::new(Theme::Light, "Light"),
                    ChoiceOption::new(Theme::Dark, "Dark"),
                ],
            )
        });
        rows.row("Interface scale (%)", |ui| {
            number_field(ui, "proto_scale", &mut p.ui_scale, &SCALE);
        });
        rows.note("The scale applies at the next start.");

        rows.section("Safety");
        rows.checkbox(
            &mut p.confirm_close,
            "Ask before closing a project with unsaved changes",
            None,
        );
        rows.checkbox(&mut p.autosave, "Save a recovery copy automatically", None);
        rows.enabled(p.autosave, |rows| {
            rows.row("Every (min)", |ui| {
                number_field(ui, "proto_autosave", &mut p.autosave_minutes, &MINUTES);
            });
        });
        rows.row("Undo steps kept", |ui| {
            number_field(ui, "proto_undo", &mut p.undo_depth, &UNDO);
        });
    });
}

fn display_page(ui: &mut egui::Ui, p: &mut Proto) {
    form(ui, "proto_display", |rows| {
        rows.section("Toolbars");
        for (index, name) in ["File", "Tools", "Text format"].into_iter().enumerate() {
            rows.checkbox(&mut p.toolbars[index], name, None);
        }
        rows.section("Panels");
        for (index, name) in ["Project Manager", "Object Manager", "Script Console"]
            .into_iter()
            .enumerate()
        {
            rows.checkbox(&mut p.panels[index], name, None);
        }
        rows.row("At start", |ui| {
            choice(
                ui,
                "proto_panels_start",
                &mut p.panels_start,
                &[
                    ChoiceOption::new(StartMode::Remember, "Last state")
                        .hint("Reopen the panels as you left them."),
                    ChoiceOption::new(StartMode::Fixed, "As set above")
                        .hint("Always start with the panels checked above."),
                ],
            )
        });
        rows.section("Workbook and numbers");
        rows.checkbox(&mut p.metadata_row, "Show the Metadata row", None);
        rows.checkbox(
            &mut p.decimal_comma,
            "Decimal comma in new graphs",
            Some("Applies to the tick labels of graphs created from now on.".into()),
        );
        rows.checkbox(&mut p.reduce_motion, "Reduce animations", None);
    });
}

fn export_page(ui: &mut egui::Ui, p: &mut Proto) {
    let ctx = ui.ctx().clone();
    form(ui, "proto_export", |rows| {
        rows.section("Images");
        rows.row("PNG resolution", |ui| {
            let options = [1u32, 2, 4].map(|n| ChoiceOption::new(n, format!("×{n}")));
            choice(ui, "proto_png", &mut p.png_scale, &options)
        });
        rows.row("SVG text", |ui| {
            choice(
                ui,
                "proto_svg_text",
                &mut p.svg_text,
                &[
                    ChoiceOption::new(SvgText::Text, "Text")
                        .hint("Selectable text; needs the font on the reader’s machine."),
                    ChoiceOption::new(SvgText::Outlines, "Outlines")
                        .hint("Drawn shapes; identical everywhere, not editable as text."),
                ],
            )
        });
        rows.row("Default format", |ui| {
            choice(
                ui,
                "proto_format",
                &mut p.default_format,
                &[
                    ChoiceOption::new(Format::Svg, "SVG"),
                    ChoiceOption::new(Format::Png, "PNG"),
                    ChoiceOption::new(Format::Pdf, "PDF"),
                ],
            )
        });
        rows.section("Copy Graph");
        rows.note(
            "Formats offered together to other applications, in this order. Drag the handle to reorder.",
        );
        let ids: Vec<egui::Id> = p
            .copy_formats
            .iter()
            .map(|c| egui::Id::new(("proto_copy_format", c.format)))
            .collect();
        let mut over = None;
        for (index, id) in ids.iter().enumerate() {
            let CopyFormat { format, on } = &mut p.copy_formats[index];
            let name = format.label();
            rows.field(|ui| {
                let hovered = reorder_row(
                    ui,
                    *id,
                    format!("Move {name}"),
                    "Drag to change the order.",
                    |ui| {
                        ui.set_min_width(220.0);
                        ui.weak(format!("{}.", index + 1));
                        ui.checkbox(on, name);
                    },
                );
                if hovered {
                    over = Some(index);
                }
            });
        }
        if let Some((from, to)) = reorder_move(&ctx, &ids, over) {
            move_before(&mut p.copy_formats, from, to);
        }
    });
}

fn folders_page(ui: &mut egui::Ui, p: &mut Proto) {
    form(ui, "proto_folders", |rows| {
        rows.section("Where StokesForge keeps its files");
        for (label, path) in [
            ("Preferences", "~/.config/stokesforge/preferences.json"),
            ("Saved colors", "~/.config/stokesforge/couleurs.json"),
            ("Import recipes", "~/.config/stokesforge/recipes/"),
        ] {
            rows.row(label, |ui| {
                ui.label(egui::RichText::new(path).weak());
                ui.button("Open Folder").clicked()
            });
        }
        rows.section("Default folders");
        rows.note("“Last used” reopens the folder of your previous file.");
        for (index, kind) in FOLDER_KINDS.into_iter().enumerate() {
            let rule = &mut p.folders[index];
            rows.row(kind, |ui| {
                let mut fixed = rule.fixed;
                if choice(
                    ui,
                    ("proto_folder_mode", index),
                    &mut fixed,
                    &[
                        ChoiceOption::new(false, "Last used"),
                        ChoiceOption::new(true, "Fixed"),
                    ],
                ) {
                    rule.fixed = fixed;
                }
            });
            rows.enabled(rule.fixed, |rows| {
                rows.row("", |ui| {
                    help_beside(ui, "A full path; Browse… fills it for you.", |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut rule.path)
                                .desired_width(260.0)
                                .hint_text("/home/…"),
                        )
                    });
                    ui.button("Browse…").clicked()
                });
            });
        }
    });
}

fn analysis_page(ui: &mut egui::Ui, p: &mut Proto) {
    form(ui, "proto_analysis", |rows| {
        rows.section("Results");
        rows.row("New analyses", |ui| {
            choice(
                ui,
                "proto_keep",
                &mut p.keep_in_results,
                &[
                    ChoiceOption::new(false, "Show values"),
                    ChoiceOption::new(true, "Keep in Results"),
                ],
            )
        });
        rows.note("Each analysis window remembers its own choice afterwards.");
        rows.section("Fitting");
        rows.row("Confidence level (%)", |ui| {
            number_field(ui, "proto_conf", &mut p.confidence_percent, &CONFIDENCE);
        });
        rows.row("Maximum iterations", |ui| {
            number_field(ui, "proto_iter", &mut p.max_iterations, &ITER);
        });
    });
}

/// The bench in a real window: the modal window reopens through a button
/// and the validated value is shown behind it.
struct Demo {
    bench: Bench,
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Preferences prototype");
            if self.bench.closed && ui.button("Reopen Preferences").clicked() {
                self.bench.closed = false;
            }
            ui.add_space(crate::theme::SPACE_ITEM);
            match &self.bench.applied {
                Some(applied) => ui.monospace(format!("Applied:\n{applied:#?}")),
                None => ui.label("Nothing applied yet."),
            };
        });
        preferences_proto(&ctx, &mut self.bench);
    }
}

/// Opens the demonstration window (blocks until it is closed). winit usually
/// wants the main thread; a test runs in another, hence
/// `with_any_thread` (one extension per platform: Wayland and X11, Windows).
fn run() -> eframe::Result {
    eframe::run_native(
        "Preferences prototype",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 640.0]),
            event_loop_builder: Some(Box::new(|builder| {
                #[cfg(target_os = "linux")]
                {
                    use winit::platform::{
                        wayland::EventLoopBuilderExtWayland, x11::EventLoopBuilderExtX11,
                    };
                    EventLoopBuilderExtWayland::with_any_thread(builder, true);
                    EventLoopBuilderExtX11::with_any_thread(builder, true);
                }
                #[cfg(windows)]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    EventLoopBuilderExtWindows::with_any_thread(builder, true);
                }
                // Other platform (macOS): nothing to set, `builder` stays used.
                #[cfg(not(any(target_os = "linux", windows)))]
                let _ = builder;
            })),
            ..Default::default()
        },
        Box::new(|cc| {
            cc.egui_ctx
                .set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(Demo {
                bench: Bench::new(),
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    use super::*;

    /// Only one capture engine at a time. Under Windows, two simultaneous
    /// wgpu instances (egui_kittest) crash the `d3d10warp.dll` software rasterizer
    /// with `0xc0000005` (#333). To be taken **before** the bench
    /// in any test that calls `snapshot`: it is released after it.
    fn screenshot_lock() -> std::sync::MutexGuard<'static, ()> {
        static CAPTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        CAPTURES.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The bench: the application theme UI, the window open.
    fn bench() -> Harness<'static, Bench> {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(760.0, 600.0))
            .with_theme(egui::Theme::Dark)
            .build_ui_state(
                |ui, bench: &mut Bench| preferences_proto(ui.ctx(), bench),
                Bench::new(),
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn go_to(harness: &mut Harness<'static, Bench>, page: &str) {
        harness.get_by_label(page).click();
        harness.run();
    }

    #[test]
    fn the_five_sections_are_tabs() {
        let harness = bench();
        for page in Page::ALL {
            harness.get_by_label(page.title());
        }
    }

    #[test]
    fn choosing_the_dark_theme_changes_the_draft_not_the_validated_value() {
        let mut harness = bench();
        harness.get_by_label("Dark").click();
        harness.run();
        assert_eq!(harness.state().draft.theme, Theme::Dark);
        assert!(harness.state().applied.is_none(), "nothing is validated");
    }

    #[test]
    fn ticking_autosave_changes_the_draft() {
        let mut harness = bench();
        assert!(!harness.state().draft.autosave);
        harness
            .get_by_label("Save a recovery copy automatically")
            .click();
        harness.run();
        assert!(harness.state().draft.autosave);
    }

    fn order(harness: &Harness<'static, Bench>) -> Vec<Format> {
        harness
            .state()
            .draft
            .copy_formats
            .iter()
            .map(|c| c.format)
            .collect()
    }

    #[test]
    fn a_fixed_folder_without_a_path_blocks_apply_and_says_why() {
        let mut harness = bench();
        go_to(&mut harness, "Folders and Paths");
        // "Fixed" appears four times (Open, Save, Import, Export): the first.
        harness.get_all_by_label("Fixed").next().unwrap().click();
        harness.run();
        assert!(harness.state().draft.folders[0].fixed);
        assert!(apply_blocker(&harness.state().draft).is_some());
        harness.get_by_label("Apply").click();
        harness.run();
        assert!(
            !harness.state().closed,
            "Apply greyed out: the window stays"
        );
        harness.get_by_label_contains("Choose a folder for “Open”");
    }

    #[test]
    fn apply_validates_the_draft_and_closes() {
        let mut harness = bench();
        go_to(&mut harness, "Export");
        harness.get_by_label("×2").click();
        harness.run();
        harness.get_by_label("Apply").click();
        harness.run();
        assert!(harness.state().closed);
        assert_eq!(harness.state().applied.as_ref().unwrap().png_scale, 2);
    }

    #[test]
    fn cancel_closes_without_validating_anything() {
        let mut harness = bench();
        harness.get_by_label("Dark").click();
        harness.run();
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(harness.state().closed);
        assert!(harness.state().applied.is_none());
    }

    #[test]
    fn reset_page_touches_only_the_current_section() {
        let mut harness = bench();
        harness.get_by_label("Dark").click();
        go_to(&mut harness, "Export");
        harness.get_by_label("×1").click();
        harness.run();
        harness.get_by_label("Reset Page").click();
        harness.run();
        assert_eq!(harness.state().draft.png_scale, 4, "Export back to default");
        assert_eq!(harness.state().draft.theme, Theme::Dark, "General intact");
    }

    /// The interactive window: `cargo test -p stokesforge-app --lib
    /// preferences_proto_interactive -- --ignored --nocapture`.
    #[test]
    #[ignore = "opens a real window"]
    fn preferences_proto_interactive() {
        run().unwrap();
    }

    #[test]
    fn dragging_a_row_changes_the_copy_order() {
        let _captures = screenshot_lock();
        let mut harness = bench();
        go_to(&mut harness, "Export");
        assert_eq!(order(&harness), [Format::Svg, Format::Png, Format::Pdf]);
        let from = harness.get_by_label("Move PDF").rect().center();
        let first = harness.get_by_label("Move SVG").rect();
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from - egui::vec2(0.0, 12.0));
        harness.step();
        harness.hover_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.step();
        harness.step();
        harness.drop_at(egui::pos2(first.center().x, first.top() + 2.0));
        harness.run();
        assert_eq!(order(&harness), [Format::Pdf, Format::Svg, Format::Png]);
        harness.snapshot("preferences_proto_export_reordered");
    }

    /// Reference screenshots of each section (first run:
    /// `UPDATE_SNAPSHOTS=1 cargo test -p stokesforge-app preferences_proto`).
    #[test]
    fn screenshots_of_the_sections() {
        let _captures = screenshot_lock();
        let mut harness = bench();
        for page in Page::ALL {
            go_to(&mut harness, page.title());
            harness.snapshot(format!(
                "preferences_proto_{}",
                page.title().to_lowercase().replace(' ', "_")
            ));
        }
    }
}
