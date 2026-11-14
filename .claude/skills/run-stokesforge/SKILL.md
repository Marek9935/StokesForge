---
name: run-stokesforge
description: Build, launch, drive and screenshot the real StokesForge desktop app on Linux — clicks, keys, PNG captures. Use for « lance l'app » (launch the app), « screenshot », « vérifie dans l'app » (check in the app), to look at a visual symptom, or to confirm a UI change works in the real app, not only in tests.
---

StokesForge is an egui desktop app with no port and no API. Drive it with `driver.py` (headless mutter + rootful Xwayland + XTEST mouse/keyboard + ImageMagick capture). Paths are relative to the repo root; no `sudo`. Prerequisites: `Xwayland`, `mutter` and `dbus-run-session` (invisible screen), ImageMagick (`import`, `magick`), Python 3 with `python-xlib`. Without `mutter` or with `SF_VISIBLE=1`, the driver falls back to visible mode: Xwayland opens a window on the current Wayland session, which then must be open.

## Steps

1. **Build**: `cargo build -p stokesforge-app -p stokesforge-cli`. Without the `python` feature the console shows « Not available in this build » (`scripts/python-test.sh` for it).
2. **Drive**:

   ```sh
   D=.claude/skills/run-stokesforge/driver.py
   python3 $D start [project.stf]   # reserves the first free display (:99, :100…), headless mutter + Xwayland (1400x900) + the app in X11, ~15 s
   python3 $D where                 # display, run folder and mode (invisible|visible); reserves nothing
   python3 $D click 20 11           # x y [mods+]left|right|double — File menu; shift+left for a multi-select
   python3 $D drag 500 123 560 330  # x1 y1 x2 y2 [mods] — hover, press, steps, release (window move, handle)
   python3 $D key ctrl+z            # F2, Return, Escape, ctrl+shift+s…
   python3 $D type "Renamed"
   python3 $D ss /tmp/shot.png      # capture, then READ the PNG
   python3 $D stop                  # kills the app, Xwayland and the compositor (process groups); `stop app` only the app
   ```

   Screen pixels, app window 1100x700 at (0,0), scale 1.0. **Invisible screen**: nothing appears on the user's desktop, so several agents may run at once (each its own display, reserved by a lock the kernel frees when the stack dies: four simultaneous `start` get four displays). An idle stack costs about 330 MB (debug app 170 MB, Xwayland 70 MB, mutter and its bus 100 MB) and no measurable CPU. Without argument, `start` opens the demo project (the driver sets `STOKESFORGE_DEMO=1`; a normal launch opens it only the first time, #333) (Book1, Graph1, folders Workbooks and Analysis, graphs Panels, Axes, PlotTypes, Annotations, Groups, Rows; `crates/stokesforge-app/src/demo/`). A project to open: `target/debug/stokesforge-cli import --out p.stf file.csv`. Logs: `/tmp/stokesforge-run/<checkout folder name>/{app,xwayland}.log` (`SF_RUN_DIR`; one folder and one display per checkout, so two worktrees never collide; two agents in the **same** worktree set distinct `SF_RUN_DIR`; the compositor log is `compositor.log`; if mutter gives no socket within 10 s, `start` kills it, says why and falls back to visible mode). The app runs with `STOKESFORGE_CONFIG_DIR=$RUN/config`: your preferences are never written (#332). `SF_APP` picks the binary (release measurements: `docs/performance-measurement.md` § Running the application in release).
3. **Single capture in native Wayland**, or text checks with the pseudo-language: [references/quick-capture.md](references/quick-capture.md).
4. **Read the PNG** with the Read tool and compare before / after, on zoomed crops of the changed area (`magick shot.png -crop 300x200+x+y +repage -scale 200% crop.png`); a visual claim without a capture is not made. **Capture budget (#517)**: one full frame per checkpoint (`python3 $D ss f.png`), then crop and shrink what you re-read: `ss f.png --crop WxH+X+Y --max-width 800` (the crop must lie inside the screen; the final size is printed). Never re-read an unchanged image; the captures are your own check, not something to show the user.
5. **Stop**: always `stop` (or `kill` by pid); reset the language to English if a test switched it.

Traps (lost clicks, capture lag, focus, killing the right process) and troubleshooting: [references/pitfalls.md](references/pitfalls.md).

Images for the user manual (#365) are not taken with this driver: `scripts/manual-screenshots.sh` renders them headless and reproducibly (`docs/tests.md`, "Manual screenshots"); this skill stays for exploring and checking the real app.

Screenshots taken with this driver are the agent's own internal check (#455): never shown to the user on a card, as proof or as a comparison bank. The user always opens the app themselves.

Parallel agents: only for independent routes (no shared file, language or sequence), one `SF_RUN_DIR` each; corrections one at a time.

Limits to state in the report: X11 under Xwayland only — no native Wayland, no `rfd` file chooser, no OS drag and drop (drags inside the window work), no detached OS window, scale 1.0 only, software rendering (not the real GPU look). For those, ask the user for a screenshot. A file drop on Windows (cursor read with `GetCursorPos`, #383) can only be checked by hand (confirmed by the user on 2026-10-29).

Done when the capture showing the expected state was read, the app, Xwayland and the compositor are stopped, and the report says what was seen and what this path cannot show.
