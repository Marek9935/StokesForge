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
