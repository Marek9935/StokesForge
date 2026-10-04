//! Dépôt de fichiers sur la fenêtre : l'emplacement du dépôt de l'image vit
//! dans la mémoire temporaire d'egui (un par viewport). La racine ou une
//! fenêtre détachée l'ouvre en début d'image ([`begin_file_drop`]), le
//! classeur survolé le réclame en se peignant ([`claim_file_drop`]), et il
//! devient une [`Action::DropFiles`] en fin d'image ([`finish_file_drop`]).
//! Un projet déposé s'ouvre dans un second processus
//! ([`open_project_in_new_window`]) ; les fichiers importés rejoignent la liste
//! des récents ([`remember_recent_imports`]).
//!
//! Sorti d'`app.rs` tel quel (#121).

use eframe::egui;
use stokesforge_core::NodeId;

use crate::state::Action;

/// Inscrit `paths` aux fichiers d'import récents (dossier de config). Un échec
/// d'écriture est sans conséquence : la liste est un confort.
pub(super) fn remember_recent_imports(paths: &[std::path::PathBuf]) {
    let mut recent = stokesforge_import::RecentFiles::load();
    recent.record(paths);
    let _ = recent.save();
}

/// Emplacement (mémoire temporaire d'egui) des fichiers déposés cette image :
/// `book` est renseigné par le classeur sous le curseur, s'il y en a un.
#[derive(Clone)]
struct FileDropSlot {
    paths: Vec<std::path::PathBuf>,
    book: Option<NodeId>,
}

fn file_drop_slot_id(ctx: &egui::Context) -> egui::Id {
    // La mémoire temporaire est commune aux viewports : un dépôt différé
    // ne doit ni écraser ni consommer celui de la fenêtre principale.
    egui::Id::new("stokesforge_file_drop").with(ctx.viewport_id())
}

/// Ouvre l'emplacement du dépôt de l'image et peint le retour visuel : un
/// contour autour de l'application tant qu'un fichier survole la fenêtre
/// (destination « nouveau classeur » ; un classeur survolé se surligne
/// lui-même, voir [`claim_file_drop`]).
pub(super) fn begin_file_drop(ctx: &egui::Context) {
    let slot_id = file_drop_slot_id(ctx);
    let (paths, hovering) = ctx.input(|input| {
        (
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect::<Vec<_>>(),
            !input.raw.hovered_files.is_empty(),
        )
    });
    if hovering {
        let rect = ctx.content_rect().shrink(2.0);
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("stokesforge_file_drop_outline"),
        ))
        .rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(0x2a, 0x7f, 0xff)),
            egui::StrokeKind::Inside,
        );
    }
    ctx.data_mut(|data| {
        if paths.is_empty() {
            data.remove::<FileDropSlot>(slot_id);
        } else {
            data.insert_temp(slot_id, FileDropSlot { paths, book: None });
        }
    });
}

/// Appelé par le classeur en train de se peindre : surligne sa zone quand un
/// fichier la survole, et réclame le dépôt de l'image s'il a eu lieu sous le
/// curseur (`rect_contains_pointer` tient compte de l'empilement des
/// fenêtres : seule la fenêtre du dessus répond).
pub(super) fn claim_file_drop(ui: &egui::Ui, book: NodeId) {
    let slot_id = file_drop_slot_id(ui.ctx());
    let rect = ui.max_rect();
    if !ui.rect_contains_pointer(rect) {
        return;
    }
    if ui.ctx().input(|input| !input.raw.hovered_files.is_empty()) {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(3.0, egui::Color32::from_rgb(0x2a, 0x7f, 0xff)),
            egui::StrokeKind::Inside,
        );
    }
    ui.ctx().data_mut(|data| {
        if let Some(slot) = data.get_temp::<FileDropSlot>(slot_id) {
            if slot.book.is_none() {
                data.insert_temp(
                    slot_id,
                    FileDropSlot {
                        book: Some(book),
                        ..slot
                    },
                );
            }
        }
    });
}

/// Transforme le dépôt de l'image en [`Action::DropFiles`].
pub(super) fn finish_file_drop(ctx: &egui::Context) -> Option<Action> {
    let slot_id = file_drop_slot_id(ctx);
    let slot = ctx.data_mut(|data| {
        let slot = data.get_temp::<FileDropSlot>(slot_id);
        data.remove::<FileDropSlot>(slot_id);
        slot
    })?;
    Some(Action::DropFiles {
        paths: slot.paths,
        target_book: slot.book,
    })
}

/// Ouvre `path` dans une nouvelle fenêtre StokesForge : un second processus de ce
/// même binaire (`stokesforge-app <chemin>`). Un échec de lancement n'est que
/// journalisé — un dépôt raté ne doit rien casser.
pub(super) fn open_project_in_new_window(path: &std::path::Path) {
    let spawned =
        std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(path).spawn());
    if let Err(err) = spawned {
        eprintln!(
            "[stokesforge-app] ouverture de {} impossible : {err}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeFile(std::path::PathBuf);

    impl egui::DroppedFile for FakeFile {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    fn fichier(name: &str) -> egui::DroppedFileHandle {
        std::sync::Arc::new(FakeFile(name.into()))
    }

    /// Ce qu'une image injecte : pointeur, fichiers lâchés, fichiers qui
    /// survolent la fenêtre.
    struct Image {
        pointer: egui::Pos2,
        dropped: Vec<egui::DroppedFileHandle>,
        hovered: bool,
    }

    fn input(image: &Image) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events: vec![egui::Event::PointerMoved(image.pointer)],
            ..Default::default()
        };
        input.dropped_files = image.dropped.clone();
        if image.hovered {
            input.hovered_files = vec![egui::HoveredFile::default()];
        }
        input
    }

    /// Un faux classeur `book` posé en (`at`, 300 × 300), qui réclame le dépôt
    /// en se peignant comme le fait le vrai.
    fn classeur(ctx: &egui::Context, name: &str, at: egui::Pos2, book: NodeId) {
        egui::Area::new(egui::Id::new(name))
            .fixed_pos(at)
            .show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(300.0, 300.0), |ui| {
                    ui.set_min_size(egui::vec2(300.0, 300.0));
                    claim_file_drop(ui, book);
                });
            });
    }

    /// Issue #8 : un fichier lâché sur un classeur est réclamé par ce classeur
    /// (et seulement lui), un fichier lâché ailleurs n'a pas de classeur, et un
    /// survol sans dépôt n'émet aucune action. Sans fenêtre : les événements
    /// `hovered_files`/`dropped_files` sont injectés dans l'entrée d'egui.
    #[test]
    fn un_depot_est_reclame_par_le_classeur_sous_le_curseur() {
        let book = NodeId::from_raw(42);
        let context = egui::Context::default();
        // Un classeur occupe (100,100)-(400,400) ; le curseur est dedans ou dehors.
        let frame = |image: Image| -> (Option<Action>, egui::FullOutput) {
            let mut result = None;
            let mut output = context.run_ui(input(&image), |ui| {
                let ctx = ui.ctx().clone();
                begin_file_drop(&ctx);
                classeur(&ctx, "faux_classeur", egui::pos2(100.0, 100.0), book);
                result = finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            (result, output)
        };
        let on_book = egui::pos2(200.0, 200.0);
        let image = |pointer, dropped: &[&str], hovered| Image {
            pointer,
            dropped: dropped.iter().map(|name| fichier(name)).collect(),
            hovered,
        };
        // Deux images de chauffe (l'Area n'est interactive qu'après la première).
        frame(image(on_book, &[], false));
        frame(image(on_book, &[], false));
        assert_eq!(frame(image(on_book, &[], false)).0, None, "pas de dépôt");
        assert_eq!(
            frame(image(on_book, &[], true)).0,
            None,
            "un survol sans dépôt n'émet rien"
        );
        let on_book = frame(image(on_book, &["mesure.csv"], false)).0;
        assert!(
            matches!(&on_book, Some(Action::DropFiles { target_book: Some(b), paths }) if *b == book && paths.len() == 1),
            "{on_book:?}"
        );
        let elsewhere = frame(image(egui::pos2(700.0, 500.0), &["mesure.csv"], false)).0;
        assert!(
            matches!(
                &elsewhere,
                Some(Action::DropFiles {
                    target_book: None,
                    ..
                })
            ),
            "{elsewhere:?}"
        );
    }

    /// Le survol d'un fichier se voit : un contour autour de la fenêtre
    /// (destination « nouveau classeur »), plus épais autour du classeur
    /// survolé. Sans survol, aucun contour.
    #[test]
    fn le_survol_d_un_fichier_entoure_la_fenetre_et_le_classeur_survole() {
        let book = NodeId::from_raw(42);
        let context = egui::Context::default();
        let strokes = |pointer: egui::Pos2, hovered: bool| -> Vec<f32> {
            let mut widths = Vec::new();
            for _ in 0..3 {
                let image = Image {
                    pointer,
                    dropped: Vec::new(),
                    hovered,
                };
                let mut output = context.run_ui(input(&image), |ui| {
                    let ctx = ui.ctx().clone();
                    begin_file_drop(&ctx);
                    classeur(&ctx, "faux_classeur", egui::pos2(100.0, 100.0), book);
                    finish_file_drop(&ctx);
                });
                output.textures_delta.clear();
                widths = output
                    .shapes
                    .iter()
                    .filter_map(|painted| match &painted.shape {
                        egui::Shape::Rect(rect) if rect.stroke.width > 0.0 => {
                            Some(rect.stroke.width)
                        }
                        _ => None,
                    })
                    .collect();
            }
            widths.sort_by(f32::total_cmp);
            widths
        };
        assert!(
            strokes(egui::pos2(200.0, 200.0), false).is_empty(),
            "sans survol, rien"
        );
        assert_eq!(
            strokes(egui::pos2(700.0, 500.0), true),
            vec![2.0],
            "la fenêtre seule"
        );
        assert_eq!(
            strokes(egui::pos2(200.0, 200.0), true),
            vec![2.0, 3.0],
            "la fenêtre et le classeur survolé"
        );
    }

    /// Deux classeurs superposés : le dépôt va à celui du dessus, seul à
    /// répondre au pointeur (`rect_contains_pointer` tient compte de
    /// l'empilement). Et si deux classeurs le réclament, le premier le garde.
    #[test]
    fn le_depot_va_au_classeur_du_dessus_et_au_premier_qui_le_reclame() {
        let dessous = NodeId::from_raw(1);
        let dessus = NodeId::from_raw(2);
        let context = egui::Context::default();
        let frame = |dropped: Vec<egui::DroppedFileHandle>| -> Option<Action> {
            let image = Image {
                pointer: egui::pos2(250.0, 250.0),
                dropped,
                hovered: false,
            };
            let mut result = None;
            let mut output = context.run_ui(input(&image), |ui| {
                let ctx = ui.ctx().clone();
                begin_file_drop(&ctx);
                // Peint en second, le classeur « dessus » est au premier plan.
                classeur(&ctx, "dessous", egui::pos2(100.0, 100.0), dessous);
                classeur(&ctx, "dessus", egui::pos2(200.0, 200.0), dessus);
                result = finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            result
        };
        frame(Vec::new());
        frame(Vec::new());
        assert_eq!(
            frame(vec![fichier("mesure.csv")]),
            Some(Action::DropFiles {
                paths: vec!["mesure.csv".into()],
                target_book: Some(dessus),
            })
        );

        // Deux réclamations dans la même zone (aucun empilement entre elles,
        // contexte neuf sans les fenêtres d'avant) : la première l'emporte.
        let context = egui::Context::default();
        let image = Image {
            pointer: egui::pos2(200.0, 200.0),
            dropped: vec![fichier("mesure.csv")],
            hovered: false,
        };
        let mut result = None;
        let mut output = context.run_ui(input(&image), |ui| {
            begin_file_drop(ui.ctx());
            claim_file_drop(ui, dessous);
            claim_file_drop(ui, dessus);
            result = finish_file_drop(ui.ctx());
        });
        output.textures_delta.clear();
        assert_eq!(
            result,
            Some(Action::DropFiles {
                paths: vec!["mesure.csv".into()],
                target_book: Some(dessous),
            })
        );
    }

    /// Un dépôt ne sert qu'une fois : `finish_file_drop` le consomme, et une
    /// image sans fichier efface un dépôt qu'on n'aurait pas consommé.
    #[test]
    fn un_depot_ne_sert_qu_une_fois_et_ne_survit_pas_a_l_image_suivante() {
        let context = egui::Context::default();
        let run = |dropped: Vec<egui::DroppedFileHandle>, finish: bool| -> Option<Action> {
            let image = Image {
                pointer: egui::pos2(10.0, 10.0),
                dropped,
                hovered: false,
            };
            let mut result = None;
            let mut output = context.run_ui(input(&image), |ui| {
                begin_file_drop(ui.ctx());
                if finish {
                    result = finish_file_drop(ui.ctx());
                    // Un second appel dans la même image ne rejoue rien.
                    assert_eq!(finish_file_drop(ui.ctx()), None);
                }
            });
            output.textures_delta.clear();
            result
        };
        assert!(run(vec![fichier("a.csv")], true).is_some());
        // Dépôt non consommé, puis une image sans fichier : il est oublié.
        run(vec![fichier("b.csv")], false);
        let mut output = context.run_ui(
            input(&Image {
                pointer: egui::pos2(10.0, 10.0),
                dropped: Vec::new(),
                hovered: false,
            }),
            |ui| {
                begin_file_drop(ui.ctx());
                assert_eq!(
                    finish_file_drop(ui.ctx()),
                    None,
                    "le dépôt d'avant ne revient pas"
                );
            },
        );
        output.textures_delta.clear();
    }
}
