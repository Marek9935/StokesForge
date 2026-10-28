# Measuring interface smoothness (#340, #341)

A reproducible procedure to measure the per-frame cost of the application
while a floating window is dragged. It was used on 2026-10-23 to tell "slow
code" from "slow debug build" (results below). Nothing below is in the
production code: the timers were a throwaway patch (`app/prof.rs` and its calls),
no longer kept in the repository; rewrite them if needed.

## Running the application in release

```sh
cargo run --release -p stokesforge-app     # binaire : target/release/stokesforge-app
```

`cargo run` alone builds in **debug**, with no optimization at all (the
`Cargo.toml` has no `[profile.dev]`): egui, Plotters and Typst are much slower
there. Any smoothness measurement is done in release, or compares debug and
release.

## Redoing the measurement

Prerequisites: those of `.claude/skills/run-stokesforge` (Xwayland, ImageMagick,
Python with `python-xlib`), and a build instrumented with the timers (see above).

```sh
cargo build -p stokesforge-app                         # debug
cargo build --release -p stokesforge-app               # release
scripts/perf/measure-windows.sh debug 6                # 6 graphes + Book1 + Graph1
scripts/perf/measure-windows.sh release 6
```

The script goes through the `run-stokesforge` driver, which sets
`STOKESFORGE_DEMO=1`. The second argument (0 to 6) is the number of graphs of
the demo project opened in addition to Book1 and Graph1: varying this number
gives the "cost per window" curve.

The script starts the application under Xwayland (`driver.py`), opens the graphs
by double-click in the Project Manager, drags the title bar of a window for 6 s
(`scripts/perf/drag.py`, sinusoidal motion), then prints the 2 s blocks during
the drag (`scripts/perf/summ.py`). The drag is sometimes lost, especially in
debug: the script retries up to 3 times.

`STOKESFORGE_PROF=1` turns the timers on; `driver.py` honors `SF_APP` (the
binary to launch) to choose the profile.

### Reading the table

Columns: number of calls, mean, maximum, total, in milliseconds over 2 s.

| Line | What it measures |
|---|---|
| `00 intervalle entre images` | real time between two `ui()` calls; 8.3 ms = 120 frames/s |
| `00b cpu_usage image precedente` | CPU of the whole frame according to eframe (`ui()` + tessellation + paint preparation, without the vertical-sync wait) |
| `01 ui total` | the whole `App::ui` |
| `05 show_workspace` | the stack of floating windows |
| `07`, `08` | `sync_window_views` and the `set_window_geometry` round trip to the session actor |
| `09`, `10` | per floating window: shell and `tab_contents` |

`cpu_usage − ui total` is the part outside `ui()` (egui tessellation, shape
copy); the GPU wait is not included.

## Results of 2026-10-23 (release and debug, Xwayland + llvmpipe)

Measurements while dragging a window, Core Ultra 7 155H:

| Build | Windows | Frames/s | CPU/frame | `ui()` | per window (`09`) |
|---|---|---|---|---|---|
| debug | 2 | ~75 | 12 ms | 8.7 ms | 1.9 ms |
| debug | 8 | ~23 | 40 ms | 25.6 ms | 1.3 ms |
| release | 8 | ~120 | 3.3 ms | 1.1 ms | 0.07 ms |

Findings:
- the debug build explains most of it (×10 on `ui()`, ×7 on the whole frame);
- `set_window_geometry` (round trip with the actor) costs 0.1 to 0.2 ms: not the cause;
- the rendering cache key does not contain the window position: a move does not trigger it;
- in debug, only 40% of `ui()` is in the floating windows; the rest comes from the panels, the toolbars and what is published on every frame.

## Known limits

- Software rendering (llvmpipe) under Xwayland: the GPU part is not representative.
- The "Rows" window takes the full width of the work area and cannot be moved: the script targets another window (coordinates 440,138 of the demo project). Another project or another resolution requires adapting these coordinates in `measure-windows.sh`.
- Not measured yet: a workbook of 10 columns × 400 rows (#341), which needs a project to import (`stokesforge-cli import`) and a pan or selection gesture in the grid.
- Leads from #341 not confirmed by measurement: `column_staleness` and `is_selected` called per cell (`docs/` says nothing about it; see `grid.rs`).
