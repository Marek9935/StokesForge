---
name: run-stokesforge
description: Build, launch, drive and screenshot the real StokesForge desktop app on Linux — clicks, keys, PNG captures. Use for « lance l'app » (launch the app), « screenshot », « vérifie dans l'app » (check in the app), to look at a visual symptom, or to confirm a UI change works in the real app, not only in tests.
---

StokesForge is an egui desktop app with no port and no API. Drive it with `driver.py` (rootful Xwayland + XTEST mouse/keyboard + ImageMagick capture). Paths are relative to the repo root; no `sudo`. Prerequisites: `Xwayland`, ImageMagick (`import`, `magick`), Python 3 with `python-xlib`, an open Wayland session.

## Steps

1. **Build**: `cargo build -p stokesforge-app -p stokesforge-cli`. Without the `python` feature the console shows « Not available in this build » (`scripts/python-test.sh` for it).
2. **Drive**:

   ```sh
   D=.claude/skills/run-stokesforge/driver.py
   python3 $D start [project.stf]   # Xwayland :99 (1400x900) + the app in X11, ~15 s
   python3 $D click 20 11           # x y [left|right|double] — File menu
   python3 $D key ctrl+z            # F2, Return, Escape, ctrl+shift+s…
   python3 $D type "Renamed"
   python3 $D ss /tmp/shot.png      # capture, then READ the PNG
   python3 $D stop                  # kills the app and Xwayland
   ```

   Screen pixels, app window 1100x700 at (0,0), scale 1.0. Without argument, `start` opens the demo project (the driver sets `STOKESFORGE_DEMO=1`; a normal launch opens it only the first time, #333) (Book1, Graph1, folders Workbooks and Analysis, graphs Panels, Axes, PlotTypes, Annotations, Groups, Rows; `crates/stokesforge-app/src/demo/`). A project to open: `target/debug/stokesforge-cli import --out p.stf file.csv`. Logs: `/tmp/stokesforge-run/{app,xwayland}.log` (`SF_RUN_DIR`). `SF_APP` picks the binary (release measurements: `docs/performance-measurement.md` § Running the application in release).
3. **Single capture in native Wayland**, or text checks with the pseudo-language: [references/quick-capture.md](references/quick-capture.md).
4. **Read the PNG** with the Read tool and compare before / after; a visual claim without a capture is not made.
5. **Stop**: always `stop` (or `kill` by pid); reset the language to English if a test switched it.

Traps (lost clicks, capture lag, focus, killing the right process) and troubleshooting: [references/pitfalls.md](references/pitfalls.md).

Limits to state in the report: X11 under Xwayland only — no native Wayland, no `rfd` file chooser, no drag, no detached OS window. For those, ask the user for a screenshot.

Done when the capture showing the expected state was read, the app and Xwayland are stopped, and the report says what was seen and what this path cannot show.
