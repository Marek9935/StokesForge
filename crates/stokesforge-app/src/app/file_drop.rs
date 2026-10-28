//! File drop on the window: the slot for the frame's drop lives in egui's
//! temporary memory (one per viewport). The root or a detached window opens it
//! at the start of the frame ([`begin_file_drop`]), the hovered workbook claims
//! it while painting ([`claim_file_drop`]), and it becomes an
//! [`Action::DropFiles`] at the end of the frame ([`finish_file_drop`]).
//! A dropped project opens in a second process
//! ([`open_project_in_new_window`]); imported files join the recent list
//! ([`remember_recent_imports`]).
//!
//! Moved out of `app.rs` as is (#121).

use eframe::egui;
use stokesforge_core::NodeId;

use stokesforge_app_state::state::Action;

/// Records `paths` in the recent import files (config folder). A write failure
/// is harmless: the list is a convenience.
pub(super) fn remember_recent_imports(paths: &[std::path::PathBuf]) {
    let mut recent = stokesforge_import::RecentFiles::load();
    recent.record(paths);
    let _ = recent.save();
}

/// Slot (egui's temporary memory) for the files dropped this frame: `book` is
/// filled in by the workbook under the cursor, if there is one.
#[derive(Clone)]
struct FileDropSlot {
    paths: Vec<std::path::PathBuf>,
    book: Option<NodeId>,
}

fn file_drop_slot_id(ctx: &egui::Context) -> egui::Id {
    // Temporary memory is shared by the viewports: a deferred drop must
    // neither overwrite nor consume the one of the main window.
    egui::Id::new("stokesforge_file_drop").with(ctx.viewport_id())
}

/// Opens the frame's drop slot and paints the visual feedback: an outline
/// around the application while a file hovers over the window (destination
/// "new workbook"; a hovered workbook highlights itself, see
/// [`claim_file_drop`]).
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
            crate::theme::drop_stroke(),
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

/// Called by the workbook while it paints: highlights its area when a file
/// hovers over it, and claims the frame's drop if it happened under the cursor
/// (`rect_contains_pointer` accounts for window stacking: only the top window
/// answers).
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
            crate::theme::drop_stroke(),
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

/// Turns the frame's drop into an [`Action::DropFiles`].
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

/// Opens `path` in a new StokesForge window: a second process of this same
/// binary (`stokesforge-app <path>`). A launch failure is only logged — a
/// failed drop must break nothing.
pub(super) fn open_project_in_new_window(path: &std::path::Path) {
    let spawned =
        std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(path).spawn());
    if let Err(err) = spawned {
        eprintln!("[stokesforge-app] cannot open {}: {err}", path.display());
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

    fn dropped_file(name: &str) -> egui::DroppedFileHandle {
        std::sync::Arc::new(FakeFile(name.into()))
    }

    /// What a frame injects: pointer, dropped files, files hovering over the
    /// window.
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

    /// A fake workbook `book` placed at (`at`, 300 × 300), which claims the
    /// drop while painting like the real one does.
    fn workbook(ctx: &egui::Context, name: &str, at: egui::Pos2, book: NodeId) {
        egui::Area::new(egui::Id::new(name))
            .fixed_pos(at)
            .show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(300.0, 300.0), |ui| {
                    ui.set_min_size(egui::vec2(300.0, 300.0));
                    claim_file_drop(ui, book);
                });
            });
    }

    /// Issue #8: a file dropped on a workbook is claimed by that workbook (and
    /// only it), a file dropped elsewhere has no workbook, and a hover without
    /// a drop emits no action. Without a window: the `hovered_files` /
    /// `dropped_files` events are injected into egui's input.
    #[test]
    fn a_drop_is_claimed_by_the_workbook_under_the_cursor() {
        let book = NodeId::from_raw(42);
        let context = egui::Context::default();
        // A workbook occupies (100,100)-(400,400); the cursor is inside or outside.
        let frame = |image: Image| -> (Option<Action>, egui::FullOutput) {
            let mut result = None;
            let mut output = context.run_ui(input(&image), |ui| {
                let ctx = ui.ctx().clone();
                begin_file_drop(&ctx);
                workbook(&ctx, "fake_workbook", egui::pos2(100.0, 100.0), book);
                result = finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            (result, output)
        };
        let on_book = egui::pos2(200.0, 200.0);
        let image = |pointer, dropped: &[&str], hovered| Image {
            pointer,
            dropped: dropped.iter().map(|name| dropped_file(name)).collect(),
            hovered,
        };
        // Two warm-up frames (the Area is interactive only after the first).
        frame(image(on_book, &[], false));
        frame(image(on_book, &[], false));
        assert_eq!(frame(image(on_book, &[], false)).0, None, "no drop");
        assert_eq!(
            frame(image(on_book, &[], true)).0,
            None,
            "a hover without a drop emits nothing"
        );
        let on_book = frame(image(on_book, &["measurement.csv"], false)).0;
        assert!(
            matches!(&on_book, Some(Action::DropFiles { target_book: Some(b), paths }) if *b == book && paths.len() == 1),
            "{on_book:?}"
        );
        let elsewhere = frame(image(egui::pos2(700.0, 500.0), &["measurement.csv"], false)).0;
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

    /// A file hover is visible: an outline around the window (destination
    /// "new workbook"), a second one around the hovered workbook, **with the
    /// same stroke** as the graph hovered by columns (`theme::drop_stroke`,
    /// #170 point 2). Without a hover, no outline.
    #[test]
    fn a_file_hover_outlines_the_window_and_the_hovered_workbook() {
        let book = NodeId::from_raw(42);
        let context = egui::Context::default();
        let strokes = |pointer: egui::Pos2, hovered: bool| -> Vec<(f32, [u8; 3])> {
            let mut found = Vec::new();
            for _ in 0..3 {
                let image = Image {
                    pointer,
                    dropped: Vec::new(),
                    hovered,
                };
                let mut output = context.run_ui(input(&image), |ui| {
                    let ctx = ui.ctx().clone();
                    begin_file_drop(&ctx);
                    workbook(&ctx, "fake_workbook", egui::pos2(100.0, 100.0), book);
                    finish_file_drop(&ctx);
                });
                output.textures_delta.clear();
                found = output
                    .shapes
                    .iter()
                    .filter_map(|painted| match &painted.shape {
                        egui::Shape::Rect(rect) if rect.stroke.width > 0.0 => {
                            // Without the alpha: the fake workbook's window opens with a fade.
                            let [r, g, b, _] = rect.stroke.color.to_array();
                            Some((rect.stroke.width, [r, g, b]))
                        }
                        _ => None,
                    })
                    .collect();
            }
            found
        };
        let stroke = crate::theme::drop_stroke();
        let [r, g, b, _] = stroke.color.to_array();
        let expected = (stroke.width, [r, g, b]);
        assert!(
            strokes(egui::pos2(200.0, 200.0), false).is_empty(),
            "without a hover, nothing"
        );
        assert_eq!(
            strokes(egui::pos2(700.0, 500.0), true),
            vec![expected],
            "the window alone, exactly the drop stroke"
        );
        // The hovered workbook opens with a fade: its tint is not full yet, so
        // the width and the green dominance are checked.
        let both = strokes(egui::pos2(200.0, 200.0), true);
        assert_eq!(both.len(), 2, "the window and the hovered workbook");
        for (width, [r, g, b]) in both {
            assert_eq!(width, stroke.width, "same width as the graph");
            assert!(g > r && g > b, "accent green, not the earlier blue");
        }
    }

    /// Two stacked workbooks: the drop goes to the one on top, the only one to
    /// answer the pointer (`rect_contains_pointer` accounts for stacking). And
    /// if two workbooks claim it, the first keeps it.
    #[test]
    fn the_drop_goes_to_the_top_workbook_and_to_the_first_to_claim_it() {
        let below = NodeId::from_raw(1);
        let above = NodeId::from_raw(2);
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
                // Painted second, the "above" workbook is in the foreground.
                workbook(&ctx, "below", egui::pos2(100.0, 100.0), below);
                workbook(&ctx, "above", egui::pos2(200.0, 200.0), above);
                result = finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            result
        };
        frame(Vec::new());
        frame(Vec::new());
        assert_eq!(
            frame(vec![dropped_file("measurement.csv")]),
            Some(Action::DropFiles {
                paths: vec!["measurement.csv".into()],
                target_book: Some(above),
            })
        );

        // Two claims in the same area (no stacking between them, fresh context
        // without the earlier windows): the first one wins.
        let context = egui::Context::default();
        let image = Image {
            pointer: egui::pos2(200.0, 200.0),
            dropped: vec![dropped_file("measurement.csv")],
            hovered: false,
        };
        let mut result = None;
        let mut output = context.run_ui(input(&image), |ui| {
            begin_file_drop(ui.ctx());
            claim_file_drop(ui, below);
            claim_file_drop(ui, above);
            result = finish_file_drop(ui.ctx());
        });
        output.textures_delta.clear();
        assert_eq!(
            result,
            Some(Action::DropFiles {
                paths: vec!["measurement.csv".into()],
                target_book: Some(below),
            })
        );
    }

    /// A drop serves only once: `finish_file_drop` consumes it, and a frame
    /// without a file erases a drop that was not consumed.
    #[test]
    fn a_drop_serves_only_once_and_does_not_survive_the_next_frame() {
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
                    // A second call in the same frame replays nothing.
                    assert_eq!(finish_file_drop(ui.ctx()), None);
                }
            });
            output.textures_delta.clear();
            result
        };
        assert!(run(vec![dropped_file("a.csv")], true).is_some());
        // Unconsumed drop, then a frame without a file: it is forgotten.
        run(vec![dropped_file("b.csv")], false);
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
                    "the earlier drop does not come back"
                );
            },
        );
        output.textures_delta.clear();
    }
}
