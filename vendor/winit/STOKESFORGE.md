# Wayland drop: transition #72

This copy is **winit 0.30.13**, under its Apache-2.0 license (`LICENSE`).
The root `[patch.crates-io]` selects it for eframe/egui-winit 0.36.2 as well.
It stays outside the workspace members.

Source: crates.io package `winit` 0.30.13, SHA-256 of the upstream package
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`. The
upstream production files are kept, except for the changes explicitly listed
below. Files generated in the Cargo cache are not part of this copy.

## Shared receiver: fix of the first prototype

The first prototype added a `wl_data_device` in winit, which had none. That
was not enough: egui-winit then creates `smithay-clipboard` **on the same
wl_display**, with its own queue. On Mutter, the offers were sent only to the
clipboard receiver, whose upstream drop handlers are empty. The problem was
reproduced with a real drop from Files: `text/uri-list`, `enter`, `motion` and
`leave` on the clipboard receiver, with no acceptance or reception. The
ticket's second receiver was therefore a real risk.

The fix now uses **the clipboard's existing receiver**.
`vendor/smithay-clipboard` is the 0.7.3 copy under the MIT license, with the
drop added to its handlers. One worker and one receiver per seat are shared
among egui's viewports. Paste requests/replies are serialized; copy-paste and
the primary selection stay available. The last view closes the worker before
the borrowed display is destroyed.

winit no longer creates any `wl_data_device`. A calloop channel receives the
events from the existing worker: surface identifier, logical position and
paths. The winit loop finds the window and its scale, then emits the usual
events. No Wayland object or borrowed pointer crosses the channel; the
callback holds a weak reference and becomes inactive when the loop stops. No
new thread is created.

Development estimates and validation costs, before real validation of the
drop (orders of magnitude, not measurements):

| Option | Development | Memory and maintenance | Defect detection |
|---|---|---|---|
| External module on a borrowed pointer | 1-2 days with detached windows | extra thread and queue; lifetime handling and surface/viewport mapping | high: concurrency, closing and several windows |
| Receiver shared between clipboard and winit, chosen | about one day with tests and docs | two patches of versions already compiled; one receiver per seat and one worker for all views | high for the protocol; parser, channel and lifetime covered automatically |
| Backport of the 0.31 backend | several days | adapting the reworked event APIs; wider diff | high: all window events |
| XDG picker only | a few hours | `rfd` component already present | low, but does not allow dropping a file |
| X11 kept as default | a few minutes | no transition | low for the drop; keeps the latency seen under XWayland |
| Immediate migration to winit 0.31 beta | several days at least | coordinated migration of eframe/egui-winit | very high: the whole interface |

Upstream state checked on 2026-10-02:
[winit 0.31.0-beta.3](https://github.com/rust-windowing/winit/releases),
[egui-winit](https://github.com/emilk/egui/blob/main/crates/egui-winit/Cargo.toml).
Check these sources again before migrating.

## Production diff to review

winit: `src/platform/wayland.rs`, `src/platform_impl/linux/wayland/mod.rs`,
`state.rs`, `event_loop/mod.rs` and the new `file_drop_bridge.rs`.
The manifest and the seat handlers are the upstream ones.

smithay-clipboard: `Cargo.toml` (winit and url, already in the graph),
`src/lib.rs` (shared worker), `worker.rs` (callback), `state.rs` (wiring),
the new `file_drop.rs` and `file_drop_uri.rs`. **#149** adds
`Clipboard::store_formats` (`lib.rs`, `Command::StoreFormats` of `worker.rs`,
`State::store_formats` and the lookup of the requested MIME type in
`send_request` of `state.rs`): a clipboard selection with several MIME types
(SVG, PNG, PDF of a figure), consumed by `stokesforge-clipboard`. The text
path is unchanged; a text selection clears the representations, and vice
versa. The original manifests remain the upstream witnesses. crates.io 0.7.3
source, SHA-256:
`71704c03f739f7745053bde45fa203a46c58d25bc5c4efba1d9a60e9dba81226`.

The exact parser and channel are also compiled by the ordinary tests
`stokesforge-app/tests/wayland_uri.rs` and `wayland_bridge.rs`.
Both copies stay outside the workspace; a targeted `rustfmt` must complement
`cargo fmt`. No other upstream code is modified.

Patch checks, from the root (Linux):

```sh
cargo test -p stokesforge-app --test wayland_uri --test wayland_bridge
cargo clippy -p winit -p smithay-clipboard --lib -- -D warnings
rustfmt --edition 2021 --config-path /dev/null --config skip_children=true --check \
  vendor/winit/src/platform/wayland.rs \
  vendor/winit/src/platform_impl/linux/wayland/{state,file_drop_bridge}.rs \
  vendor/winit/src/platform_impl/linux/wayland/event_loop/mod.rs
rustfmt --edition 2024 --config-path /dev/null --config skip_children=true --check \
  vendor/smithay-clipboard/src/{file_drop,file_drop_uri,lib,state,worker}.rs
```

The upstream smithay-clipboard configuration file contains nightly options;
`/dev/null` selects the stable formatting for the modified files.
`skip_children=true` leaves the unchanged upstream modules alone.

## Interface and limits

The bridge emits only `CursorMoved` (drop position), `HoveredFile`,
`HoveredFileCancelled` and `DroppedFile`, with the window's native identity.
egui-winit translates them into `hovered_files`/`dropped_files` as under X11.
The application keeps `Action::DropFiles` and its shared import.

`text/uri-list` and the **Copy** action are accepted. No deletion of the
source file is requested. Local `file:` URIs are decoded by `url`, with
comments, CRLF, escaped spaces and deduplication. Remote or relative URIs,
URIs with a fragment/query, and URIs containing NUL are rejected. A hover
marker with an empty path signals the offer before reception; it never
becomes a drop. Paths are requested only after `drop`.

Reading the pipe goes through calloop: one read per notification,
**1 MiB maximum of URI list per drop**, maximum delay **10 seconds**.
These limits apply to the names, not to the size of the imported files. A
slow source therefore does not freeze the interface. The seat, the window and
the offer must stay valid; an error is logged. The drop position is re-emitted
when the paths arrive, to keep its destination. Touch support and compositors
other than the tested ones remain to be checked.

## Mandatory manual check

On **GNOME/Mutter**, **KDE/KWin**, then a **wlroots** compositor, run:

```sh
cargo run -p stokesforge-app
STOKESFORGE_X11=1 cargo run -p stokesforge-app
```

For each backend, drop from the file manager:

1. A CSV, then several files with spaces, accents and `%` in their names:
   visible hover, one sheet per file, one undoable import entry, no deletion
   at the source.
2. On a specific floating workbook, then outside the workbooks: correct
   destination; move the pointer right after releasing the file.
3. On a detached workbook, then on a detached graph and Project Manager: same
   destination decision as under X11.
4. Leave the window without dropping, then drop again: no persistent hover
   marker, no file imported on cancel.
5. Drop a `.stf`: opened by the existing second process.
6. Try with a graph's properties open and with the import wizard open:
   refusal or addition to the files according to the current rules.
7. Close a window during the hover, then the application after a drop: no
   protocol error and no surviving thread.
8. Repeat the drops, including a non-file offer; test two seats if available.
   No drop must be doubled.

The **native GNOME start/stop**, the **X11/XWayland** fallback and the defect
of the first prototype have been observed. After the fix, the drop from Files
in native Wayland on GNOME/Mutter was confirmed by the user on 2026-10-02.
The other cases of the matrix, notably detached windows, KDE/KWin and
wlroots, remain to be validated; Rust tests do not replace them. X11 stays
available as a fallback.

## Removal

When the chosen stable eframe version uses a winit with Wayland drop:

1. Bump eframe/egui-winit/egui together, then adapt the drop transport if the
   winit 0.31 API differs. Keep `Action::DropFiles`.
2. Remove the `[patch.crates-io]` entries, the `vendor/winit` and
   `vendor/smithay-clipboard` exclusions, the two copies, the tests
   `wayland_uri.rs` and `wayland_bridge.rs` and their `url`/`calloop`
   dev-dependencies.
3. Regenerate `Cargo.lock`; check that a single winit version is used.
4. Re-examine `configure_window_backend`: the native choice must remain
   winit's; keep the `STOKESFORGE_X11=1` fallback if it is still useful.
5. Replay the manual matrix and the workspace tests, then update
   `docs/application/`, `.codex/README.md`, `AGENTS.md` and `CLAUDE.md`.
