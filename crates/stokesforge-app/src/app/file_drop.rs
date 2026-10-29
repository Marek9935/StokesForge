//! File drop on the window: the slot for the frame's drop lives in egui's
//! temporary memory (one per viewport). The root or a detached window opens it
//! at the start of the frame ([`begin_file_drop`]), the hovered workbook claims
//! it while painting ([`claim_file_drop`]), and it becomes an
//! [`Action::DropFiles`] at the end of the frame ([`finish_file_drop`]).
//! A dropped project opens in a second process
//! ([`open_project_in_new_window`]); imported files join the recent list
//! ([`remember_recent_imports`]).
//!
//! Position source: egui's pointer, except on Windows where winit gives none
//! during an OLE drag, so the real cursor is read there (#383).
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

fn file_drop_target_id(ctx: &egui::Context) -> egui::Id {
    file_drop_slot_id(ctx).with("target")
}

/// Records `layer` as the window that is the drop target of this frame (#386):
/// that window lights up its own border. A single one per viewport and per
/// frame ([`begin_file_drop`] resets it); used for a file over a workbook
/// ([`claim_file_drop`]) and for columns over a graph (`graph_drop_target`).
pub(super) fn set_drop_target(ctx: &egui::Context, layer: egui::LayerId) {
    let id = file_drop_target_id(ctx);
    ctx.data_mut(|data| data.insert_temp(id, layer));
}

/// The window layer recorded by [`set_drop_target`] this frame, if any.
pub(super) fn drop_target(ctx: &egui::Context) -> Option<egui::LayerId> {
    let id = file_drop_target_id(ctx);
    ctx.data(|data| data.get_temp::<egui::LayerId>(id))
}

/// Whether `layer` is this frame's drop target.
pub(super) fn is_drop_target(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    drop_target(ctx) == Some(layer)
}

fn file_drop_cursor_id(ctx: &egui::Context) -> egui::Id {
    file_drop_slot_id(ctx).with("cursor")
}

/// Window position (points) of a cursor given in physical screen pixels:
/// `cursor_px / pixels_per_point - inner_rect.min`. `None` without the
/// window's inner rectangle.
fn cursor_in_window(
    cursor_px: egui::Pos2,
    inner_rect: Option<egui::Rect>,
    pixels_per_point: f32,
) -> Option<egui::Pos2> {
    let inner = inner_rect?;
    Some(egui::pos2(
        cursor_px.x / pixels_per_point - inner.min.x,
        cursor_px.y / pixels_per_point - inner.min.y,
    ))
}

/// Real cursor in physical screen pixels (Windows only: winit sends no
/// position during a drag from Explorer).
#[cfg(windows)]
fn screen_cursor_px() -> Option<egui::Pos2> {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
    }
    let mut point = Point { x: 0, y: 0 };
    // SAFETY: `point` is a valid, writable POINT for the duration of the call.
    let ok = unsafe { GetCursorPos(&mut point) };
    (ok != 0).then(|| egui::pos2(point.x as f32, point.y as f32))
}

#[cfg(not(windows))]
fn screen_cursor_px() -> Option<egui::Pos2> {
    None
}

/// During a hover with an overridden cursor, winit sends no event while the
/// cursor moves (OLE drag): repaint on our own to follow it. (egui's own
/// hover repaint is decided from the frame's input and masks this request in
/// a headless test, hence a pure decision.)
fn keeps_repainting(hovering: bool, cursor: Option<egui::Pos2>) -> bool {
    hovering && cursor.is_some()
}

/// Opens the frame's drop slot and forgets the previous frame's drop target.
/// Nothing is painted here: the target window paints its own border (#386),
/// or [`finish_file_drop`] outlines the application.
pub(super) fn begin_file_drop(ctx: &egui::Context) {
    let active = ctx.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty());
    let cursor = if active {
        screen_cursor_px().and_then(|px| {
            cursor_in_window(
                px,
                ctx.input(|i| i.viewport().inner_rect),
                ctx.pixels_per_point(),
            )
        })
    } else {
        None
    };
    begin_file_drop_with(ctx, cursor);
}

/// [`begin_file_drop`] with the cursor in window points (`None`: use egui's
/// pointer).
fn begin_file_drop_with(ctx: &egui::Context, cursor: Option<egui::Pos2>) {
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
    if keeps_repainting(hovering, cursor) {
        ctx.request_repaint();
    }
    let cursor_id = file_drop_cursor_id(ctx);
    let target_id = file_drop_target_id(ctx);
    ctx.data_mut(|data| {
        data.remove::<egui::LayerId>(target_id);
        match cursor {
            Some(pos) => {
                data.insert_temp(cursor_id, pos);
            }
            None => {
                data.remove::<egui::Pos2>(cursor_id);
            }
        }
        if paths.is_empty() {
            data.remove::<FileDropSlot>(slot_id);
        } else {
            data.insert_temp(slot_id, FileDropSlot { paths, book: None });
        }
    });
}

/// Called by the workbook while it paints: records its window as the drop
/// target when a file hovers over it, and claims the frame's drop if it happened under the cursor
/// (`rect_contains_pointer` accounts for window stacking: only the top window
/// answers).
pub(super) fn claim_file_drop(ui: &egui::Ui, book: NodeId) {
    let slot_id = file_drop_slot_id(ui.ctx());
    let rect = ui.max_rect();
    let cursor = ui
        .ctx()
        .data(|data| data.get_temp::<egui::Pos2>(file_drop_cursor_id(ui.ctx())));
    let inside = match cursor {
        Some(pos) => cursor_over(ui, rect, pos),
        None => ui.rect_contains_pointer(rect),
    };
    if !inside {
        return;
    }
    if ui.ctx().input(|input| !input.raw.hovered_files.is_empty()) {
        // First claim wins: the top workbook is the one lit.
        if drop_target(ui.ctx()).is_none() {
            set_drop_target(ui.ctx(), ui.layer_id());
        }
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

/// Same test as `Ui::rect_contains_pointer`, for an explicit position.
fn cursor_over(ui: &egui::Ui, rect: egui::Rect, pos: egui::Pos2) -> bool {
    let mut rect = ui.clip_rect().intersect(rect);
    if let Some(to_global) = ui.ctx().layer_transform_to_global(ui.layer_id()) {
        rect = to_global * rect;
    }
    rect.is_positive() && rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
}

/// Turns the frame's drop into an [`Action::DropFiles`]. While a file hovers
/// and no window claimed it (destination "new workbook"), outlines the
/// application: one highlighted window at a time (#386).
pub(super) fn finish_file_drop(ctx: &egui::Context) -> Option<Action> {
    if ctx.input(|i| !i.raw.hovered_files.is_empty()) && drop_target(ctx).is_none() {
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("stokesforge_file_drop_outline"),
        ))
        .rect_stroke(
            ctx.content_rect().shrink(2.0),
            0.0,
            crate::theme::drop_target_stroke(),
            egui::StrokeKind::Inside,
        );
    }
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

    const FRAMED_AT: egui::Pos2 = egui::pos2(100.0, 100.0);

    fn framed_rect() -> egui::Rect {
        egui::Rect::from_min_size(FRAMED_AT, egui::vec2(300.0, 300.0))
    }

    /// A workbook inside a real framed window (`show_floating_window`): the
    /// frame is where the drop border is painted.
    fn framed_workbook(ctx: &egui::Context, book: NodeId, active: bool) {
        super::super::window_chrome::show_floating_window(
            ctx,
            egui::Id::new("framed_workbook"),
            framed_rect(),
            ctx.content_rect(),
            active,
            |ui| claim_file_drop(ui, book),
            |_, _| {},
        );
    }

    /// Rectangles stroked in the drop colour, and those stroked in the
    /// selection colour, after a few warm-up frames.
    fn framed_strokes(
        context: &egui::Context,
        pointer: egui::Pos2,
        hovered: bool,
        active: bool,
    ) -> (Vec<egui::Rect>, Vec<egui::Rect>) {
        let mut found = (Vec::new(), Vec::new());
        for _ in 0..3 {
            let image = Image {
                pointer,
                dropped: Vec::new(),
                hovered,
            };
            let mut selection = egui::Color32::TRANSPARENT;
            let mut output = context.run_ui(input(&image), |ui| {
                let ctx = ui.ctx().clone();
                selection = ui.visuals().selection.stroke.color;
                begin_file_drop(&ctx);
                framed_workbook(&ctx, NodeId::from_raw(42), active);
                finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            let rects = |color: egui::Color32| -> Vec<egui::Rect> {
                output
                    .shapes
                    .iter()
                    .filter_map(|painted| match &painted.shape {
                        egui::Shape::Rect(rect)
                            if rect.stroke.width > 0.0 && rect.stroke.color == color =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect()
            };
            found = (
                rects(crate::theme::drop_target_stroke().color),
                rects(selection),
            );
        }
        found
    }

    /// Hovering the workspace away from the workbooks (destination "new
    /// workbook"): the application's outline alone, in the drop colour.
    /// Without a hover, no drop-coloured stroke.
    #[test]
    fn a_file_over_the_workspace_outlines_the_application_alone() {
        let context = egui::Context::default();
        let away = egui::pos2(700.0, 500.0);
        let (drop, _) = framed_strokes(&context, away, true, false);
        assert_eq!(
            drop,
            vec![context.content_rect().shrink(2.0)],
            "the main outline, once"
        );
        let (drop, _) = framed_strokes(&egui::Context::default(), away, false, false);
        assert!(drop.is_empty(), "without a hover, nothing");
    }

    /// Hovering a framed workbook: its own border alone lights up (one window
    /// at a time), neither the application's outline nor a stroke inside the
    /// content (#386).
    #[test]
    fn a_file_over_a_workbook_lights_up_its_window_border_alone() {
        let context = egui::Context::default();
        let (drop, _) = framed_strokes(&context, egui::pos2(200.0, 200.0), true, false);
        assert_eq!(drop, vec![framed_rect()]);
        let (drop, _) = framed_strokes(
            &egui::Context::default(),
            egui::pos2(200.0, 200.0),
            false,
            false,
        );
        assert!(drop.is_empty(), "without a hover, nothing");
    }

    /// An active and targeted window: the drop border replaces the active
    /// outline instead of being drawn over it.
    #[test]
    fn the_drop_border_replaces_the_active_outline() {
        let context = egui::Context::default();
        let (drop, selection) = framed_strokes(&context, egui::pos2(200.0, 200.0), true, true);
        assert_eq!(drop, vec![framed_rect()]);
        assert!(selection.is_empty(), "no active outline under the drop one");
    }

    /// The target is forgotten from one frame to the next: the workbook
    /// hovered at frame N is back to its normal border at N+1 when the file
    /// moves to the workspace, and the application's outline returns.
    #[test]
    fn the_drop_target_does_not_outlive_its_frame() {
        let context = egui::Context::default();
        let (drop, _) = framed_strokes(&context, egui::pos2(200.0, 200.0), true, false);
        assert_eq!(drop, vec![framed_rect()]);
        let (drop, _) = framed_strokes(&context, egui::pos2(700.0, 500.0), true, false);
        assert_eq!(drop, vec![context.content_rect().shrink(2.0)]);
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

    /// Frame helper with a cursor override (#383): the pointer of `image` is
    /// egui's, `cursor` is what Windows' `GetCursorPos` would give.
    fn frame_with(
        context: &egui::Context,
        image: &Image,
        cursor: Option<egui::Pos2>,
        books: &[(&str, egui::Pos2, NodeId)],
        move_pointer: bool,
    ) -> (Option<Action>, egui::FullOutput) {
        let mut raw = input(image);
        if !move_pointer {
            raw.events.clear();
        }
        let mut result = None;
        let mut output = context.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            begin_file_drop_with(&ctx, cursor);
            for (name, at, book) in books {
                workbook(&ctx, name, *at, *book);
            }
            result = finish_file_drop(&ctx);
        });
        output.textures_delta.clear();
        (result, output)
    }

    fn one_file(pointer: egui::Pos2) -> Image {
        Image {
            pointer,
            dropped: vec![dropped_file("measurement.csv")],
            hovered: false,
        }
    }

    fn idle(pointer: egui::Pos2) -> Image {
        Image {
            pointer,
            dropped: Vec::new(),
            hovered: false,
        }
    }

    fn target(action: Option<Action>) -> Option<NodeId> {
        match action {
            Some(Action::DropFiles { target_book, .. }) => target_book,
            other => panic!("{other:?}"),
        }
    }

    /// Warm-up (the areas are known from the second frame) then a drop.
    fn drop_with(
        pointer: egui::Pos2,
        cursor: Option<egui::Pos2>,
        books: &[(&str, egui::Pos2, NodeId)],
        move_pointer: bool,
    ) -> Option<NodeId> {
        let context = egui::Context::default();
        for _ in 0..2 {
            frame_with(&context, &idle(pointer), None, books, true);
        }
        target(frame_with(&context, &one_file(pointer), cursor, books, move_pointer).0)
    }

    const ORIGIN: egui::Pos2 = egui::pos2(100.0, 100.0);

    #[test]
    fn the_real_cursor_wins_over_a_stale_pointer() {
        let book = NodeId::from_raw(42);
        let books = [("b", ORIGIN, book)];
        let stale = egui::pos2(700.0, 500.0);
        let on_book = Some(egui::pos2(200.0, 200.0));
        assert_eq!(drop_with(stale, on_book, &books, true), Some(book));
        // Not even one PointerMoved reached egui.
        assert_eq!(drop_with(stale, on_book, &books, false), Some(book));
        // The override also wins when egui's pointer is on the book.
        assert_eq!(
            drop_with(
                egui::pos2(200.0, 200.0),
                Some(egui::pos2(700.0, 500.0)),
                &books,
                true
            ),
            None
        );
    }

    #[test]
    fn the_real_cursor_respects_stacking() {
        let below = NodeId::from_raw(1);
        let above = NodeId::from_raw(2);
        let books = [
            ("below", ORIGIN, below),
            ("above", egui::pos2(200.0, 200.0), above),
        ];
        assert_eq!(
            drop_with(
                egui::pos2(700.0, 500.0),
                Some(egui::pos2(250.0, 250.0)),
                &books,
                true
            ),
            Some(above)
        );
    }

    #[test]
    fn a_hover_outlines_the_book_under_the_real_cursor() {
        let context = egui::Context::default();
        let stale = egui::pos2(700.0, 500.0);
        let cursor = Some(egui::pos2(200.0, 200.0));
        let mut drop = Vec::new();
        for _ in 0..3 {
            let image = Image {
                pointer: stale,
                dropped: Vec::new(),
                hovered: true,
            };
            let mut output = context.run_ui(input(&image), |ui| {
                let ctx = ui.ctx().clone();
                begin_file_drop_with(&ctx, cursor);
                framed_workbook(&ctx, NodeId::from_raw(42), false);
                finish_file_drop(&ctx);
            });
            output.textures_delta.clear();
            drop = output
                .shapes
                .iter()
                .filter_map(|p| match &p.shape {
                    egui::Shape::Rect(r) if r.stroke == crate::theme::drop_target_stroke() => {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .collect();
        }
        assert_eq!(drop, vec![framed_rect()], "the workbook's window alone");
    }

    #[test]
    fn the_cursor_is_converted_to_window_points() {
        let inner = Some(egui::Rect::from_min_size(
            egui::pos2(100.0, 50.0),
            egui::vec2(800.0, 600.0),
        ));
        let px = egui::pos2(300.0, 240.0);
        assert_eq!(
            cursor_in_window(px, inner, 1.5),
            Some(egui::pos2(100.0, 110.0))
        );
        assert_eq!(
            cursor_in_window(px, inner, 1.0),
            Some(egui::pos2(200.0, 190.0))
        );
        assert_eq!(cursor_in_window(px, None, 1.0), None);
    }

    #[test]
    fn a_hover_with_a_real_cursor_keeps_repainting() {
        let cursor = Some(egui::pos2(5.0, 5.0));
        assert!(keeps_repainting(true, cursor));
        assert!(!keeps_repainting(true, None));
        assert!(!keeps_repainting(false, cursor));
    }
}
