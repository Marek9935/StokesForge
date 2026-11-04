# driver.py: traps and troubleshooting

Read from `run-stokesforge` when a click, key or capture does not do what you expect.

## Traps

- **Invisible by default**: a headless mutter (`sf-<n>` socket) hosts the rootful Xwayland, nothing shows on the desktop. `SF_VISIBLE=1` (or no `mutter`) opens a real window on the user's screen: keep those runs short. Always end with `stop`.
- **Never `pkill`, never pre-create `/tmp/.X<n>-lock`** (Xwayland owns it); the driver kills process groups only and keeps `dbus-run-session` so mutter stays off the user's bus. Do not touch an Xwayland or mutter you did not start.
- **Kill by pid only** (outside the driver): `kill $P`. `pkill -f stokesforge-app` matches the shell running the command and kills it (exit 144); `pgrep -x` would also kill the user's own instance.
- **The X capture lags one action under Xwayland**: `ss` makes four micro-moves (egui repaints only on events), captures once to discard, then keeps the second. Do not bypass it with a direct `import`.
- **`click` and `drag` modifiers**: `shift+left`, `ctrl+shift+left`, `drag … shift`; `ctrl`, `shift`, `alt` only (`meta` is refused). The modifier is pressed after the hover and released after the button.
- **A click on a far target is lost** if the pointer never hovered it: `click` moves, micro-moves and waits before pressing. Menu entries need it.
- **No window manager → no keyboard focus**: `start` calls `focus()`; without it `key` / `type` do nothing.
- **Menus are fixed pixels**; in French the widths change (« Préférences… » stays at (58,183) but the menu is wider).
- **The chosen language** is saved in `$RUN/config/preferences.json` (the driver sets `STOKESFORGE_CONFIG_DIR`, so not in the user's own); it persists across `start`s of the same run folder: switch back to English before finishing.
- **Ctrl+letter and the layout** (#415): under the user's GNOME session (visible mode) winit reads the compositor keymap (AZERTY here) while the driver resolves keysyms in the US core map: `key ctrl+a` reaches the app as Ctrl+Q and **quits it cleanly (status 0)**, `ctrl+w` as Ctrl+Z. Not an application bug. Under the **headless mutter** (measured 2026-11-04, #452) the keymap is US: `key ctrl+a` is Ctrl+A (selects all cells) and `key ctrl+q` quits. Plain letters and `type` are not affected. In visible mode check the layout first (`gsettings get org.gnome.desktop.input-sources sources`) and prefer shortcuts not on A/Q/W/Z (ctrl+s, ctrl+m behave).
- **First render is slow** (software wgpu; `MESA: warning … Xe KMD` is harmless): `start` waits 10 s; an earlier capture shows a black window.
- `type` knows letters, digits and `. , - _ = ( ) / # " : [ ] !`; add others to the `names` table of `driver.py`.

## Troubleshooting

- `unknown key: #` → character missing from the `names` table of `type_text`.
- `headless mutter gave no socket … falling back to visible mode` → read `$RUN/compositor.log`; the run continues on the desktop.
- A reused stack whose compositor died is restarted by `start`.
- `app: DEAD` after `start` → read `$RUN/app.log` (`driver.py where` prints the folder).
- `app window not found` (`focus`) → the app is not mapped yet: wait or rerun `start`.
- Capture shows the menu open but not its effect → the click was lost or a second `ss` is needed; click again.
- Known-good scenario: Preferences → switch to Français (whole UI changes) → back to English; Book1 in the Project Manager → `F2` → `type Renamed` → `Return` → renamed → `ctrl+z` → undone.
