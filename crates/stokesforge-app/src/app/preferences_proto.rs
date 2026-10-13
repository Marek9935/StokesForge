//! PROTOTYPE (test seulement) — la fenêtre Preferences « avancée » de #148,
//! montée **sans logique** : un brouillon `Proto` en mémoire, aucune session,
//! aucun `preferences.json`. Elle emploie les vraies briques (`modal_dialog_with`,
//! `tab_strip`, `form`, `choice`, `number_field`) pour que son rendu soit celui
//! de l'application, et se pilote avec `egui_kittest` (clics par libellé,
//! captures de référence dans `tests/snapshots/`).
//!
//! Beaucoup de réglages ci-dessous **n'existent pas encore** dans StokesForge
//! (thème, échelle de l'interface, sauvegarde automatique, dossiers par défaut,
//! formats de copie…) : ils servent à juger la forme, pas à promettre le fond.
//! Les libellés sont en dur parce que le module ne livre rien (il n'est
//! compilé qu'en test) ; une version livrée passerait par `tr!` (en/fr).

use eframe::egui;

use super::dialogs::{modal_dialog_with, DialogFooter, DialogOutcome};
use super::form::{choice, form, ChoiceOption};
use super::hover_help::help_beside;
use super::reorder_list::{move_before, reorder_move, reorder_row};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::number_field::number_field;
use crate::quantities::Quantity;

/// Les rubriques, dans l'ordre de #148 (+ Analysis, décision M5 de #181).
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

/// Au démarrage, les panneaux reprennent leur dernier état ou un état fixé
/// (question 1 de #148).
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

/// Un format offert par Copy Graph, et s'il l'est.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CopyFormat {
    pub(super) format: Format,
    pub(super) on: bool,
}

/// Un dossier par défaut : le dernier utilisé, ou un dossier fixé (question 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FolderRule {
    pub(super) fixed: bool,
    pub(super) path: String,
}

/// Le brouillon des préférences : tout ce que la fenêtre peut régler.
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
    /// Dans l'ordre où Copy Graph les offre ; `on` dit s'il est offert.
    pub(super) copy_formats: Vec<CopyFormat>,
    // Folders: ouvrir, enregistrer, importer, exporter
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
const ITER: Quantity = crate::quantities::ITERATIONS;

const FOLDER_KINDS: [&str; 4] = ["Open", "Save", "Import", "Export"];

/// L'état de la fenêtre dans le banc : brouillon, page, valeur validée.
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

/// Pourquoi « Apply » est grisé, s'il l'est (G1) : un dossier fixé sans chemin.
pub(super) fn apply_blocker(draft: &Proto) -> Option<String> {
    draft
        .folders
        .iter()
        .zip(FOLDER_KINDS)
        .find(|(rule, _)| rule.fixed && rule.path.trim().is_empty())
        .map(|(_, kind)| format!("Choose a folder for “{kind}”, or switch it to Last used."))
}

/// Peint la fenêtre (une image). Rend la valeur validée à « Apply ».
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
        egui::vec2(560.0, 470.0),
        DialogFooter {
            accept_label: "Apply",
            accept_hint: "Save the preferences and close.",
            accept_enabled: blocker.is_none(),
            accept_disabled: blocker.as_deref(),
        },
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
            egui::ScrollArea::vertical().show(ui, |ui| match current {
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

/// Le banc dans une vraie fenêtre : la fenêtre modale se rouvre par un bouton
/// et la valeur validée s'affiche derrière elle.
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

/// Ouvre la fenêtre de démonstration (bloque jusqu'à sa fermeture). winit veut
/// d'ordinaire le fil principal ; un test tourne dans un autre, d'où
/// `with_any_thread`.
fn run() -> eframe::Result {
    eframe::run_native(
        "Preferences prototype",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 640.0]),
            event_loop_builder: Some(Box::new(|builder| {
                use winit::platform::{
                    wayland::EventLoopBuilderExtWayland, x11::EventLoopBuilderExtX11,
                };
                EventLoopBuilderExtWayland::with_any_thread(builder, true);
                EventLoopBuilderExtX11::with_any_thread(builder, true);
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

    /// Le banc : l'UI du thème de l'application, la fenêtre ouverte.
    fn banc() -> Harness<'static, Bench> {
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

    fn aller_a(harness: &mut Harness<'static, Bench>, page: &str) {
        harness.get_by_label(page).click();
        harness.run();
    }

    #[test]
    fn les_cinq_rubriques_sont_des_onglets() {
        let harness = banc();
        for page in Page::ALL {
            harness.get_by_label(page.title());
        }
    }

    #[test]
    fn choisir_le_theme_sombre_modifie_le_brouillon_pas_la_valeur_validee() {
        let mut harness = banc();
        harness.get_by_label("Dark").click();
        harness.run();
        assert_eq!(harness.state().draft.theme, Theme::Dark);
        assert!(harness.state().applied.is_none(), "rien n'est validé");
    }

    #[test]
    fn cocher_la_sauvegarde_automatique_modifie_le_brouillon() {
        let mut harness = banc();
        assert!(!harness.state().draft.autosave);
        harness
            .get_by_label("Save a recovery copy automatically")
            .click();
        harness.run();
        assert!(harness.state().draft.autosave);
    }

    fn ordre(harness: &Harness<'static, Bench>) -> Vec<Format> {
        harness
            .state()
            .draft
            .copy_formats
            .iter()
            .map(|c| c.format)
            .collect()
    }

    #[test]
    fn un_dossier_fixe_sans_chemin_bloque_apply_et_dit_pourquoi() {
        let mut harness = banc();
        aller_a(&mut harness, "Folders and Paths");
        // « Fixed » apparaît quatre fois (Open, Save, Import, Export) : le premier.
        harness.get_all_by_label("Fixed").next().unwrap().click();
        harness.run();
        assert!(harness.state().draft.folders[0].fixed);
        assert!(apply_blocker(&harness.state().draft).is_some());
        harness.get_by_label("Apply").click();
        harness.run();
        assert!(!harness.state().closed, "Apply grisé : la fenêtre reste");
        harness.get_by_label_contains("Choose a folder for “Open”");
    }

    #[test]
    fn apply_valide_le_brouillon_et_ferme() {
        let mut harness = banc();
        aller_a(&mut harness, "Export");
        harness.get_by_label("×2").click();
        harness.run();
        harness.get_by_label("Apply").click();
        harness.run();
        assert!(harness.state().closed);
        assert_eq!(harness.state().applied.as_ref().unwrap().png_scale, 2);
    }

    #[test]
    fn cancel_ferme_sans_rien_valider() {
        let mut harness = banc();
        harness.get_by_label("Dark").click();
        harness.run();
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(harness.state().closed);
        assert!(harness.state().applied.is_none());
    }

    #[test]
    fn reset_page_ne_touche_que_la_rubrique_courante() {
        let mut harness = banc();
        harness.get_by_label("Dark").click();
        aller_a(&mut harness, "Export");
        harness.get_by_label("×1").click();
        harness.run();
        harness.get_by_label("Reset Page").click();
        harness.run();
        assert_eq!(harness.state().draft.png_scale, 4, "Export remis au défaut");
        assert_eq!(harness.state().draft.theme, Theme::Dark, "General intact");
    }

    /// La fenêtre interactive : `cargo test -p stokesforge-app --lib
    /// preferences_proto_interactif -- --ignored --nocapture`.
    #[test]
    #[ignore = "ouvre une vraie fenêtre"]
    fn preferences_proto_interactif() {
        run().unwrap();
    }

    #[test]
    fn glisser_une_ligne_change_l_ordre_de_copie() {
        let mut harness = banc();
        aller_a(&mut harness, "Export");
        assert_eq!(ordre(&harness), [Format::Svg, Format::Png, Format::Pdf]);
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
        assert_eq!(ordre(&harness), [Format::Pdf, Format::Svg, Format::Png]);
        harness.snapshot("preferences_proto_export_reordered");
    }

    /// Captures de référence de chaque rubrique (premier lancement :
    /// `UPDATE_SNAPSHOTS=1 cargo test -p stokesforge-app preferences_proto`).
    #[test]
    fn captures_des_rubriques() {
        let mut harness = banc();
        for page in Page::ALL {
            aller_a(&mut harness, page.title());
            harness.snapshot(format!(
                "preferences_proto_{}",
                page.title().to_lowercase().replace(' ', "_")
            ));
        }
    }
}
