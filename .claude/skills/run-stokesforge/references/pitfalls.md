# driver.py: traps and troubleshooting

Read from `run-stokesforge` when a click, key or capture does not do what you expect.

## Traps

- **Windows really open on the user's desktop**: Xwayland runs without `-rootless`, the fast path too. Keep runs short, always end with `stop` / `kill`.
- **Kill by pid only**: `kill $P`. `pkill -f stokesforge-app` matches the shell running the command and kills it (exit 144); `pgrep -x` would also kill the user's own instance.
- **The X capture lags one action under Xwayland**: `ss` makes four micro-moves (egui repaints only on events), captures once to discard, then keeps the second. Do not bypass it with a direct `import`.
- **A click on a far target is lost** if the pointer never hovered it: `click` moves, micro-moves and waits before pressing. Menu entries need it.
- **No window manager → no keyboard focus**: `start` calls `focus()`; without it `key` / `type` do nothing.
- **Menus are fixed pixels**; in French the widths change (« Préférences… » stays at (58,183) but the menu is wider).
- **The chosen language** is saved in `$RUN/config/preferences.json` (the driver sets `STOKESFORGE_CONFIG_DIR`, so not in the user's own); it persists across `start`s of the same run folder: switch back to English before finishing.
- **First render is slow** (software wgpu; `MESA: warning … Xe KMD` is harmless): `start` waits 10 s; an earlier capture shows a black window.
- `type` knows letters, digits and `. , - _ = ( ) / # " : [ ] !`; add others to the `names` table of `driver.py`.

## Troubleshooting

- `unknown key: #` → character missing from the `names` table of `type_text`.
- `app: DEAD` after `start` → read `$RUN/app.log` (`driver.py where` prints the folder).
- `app window not found` (`focus`) → the app is not mapped yet: wait or rerun `start`.
- Capture shows the menu open but not its effect → the click was lost or a second `ss` is needed; click again.
- Known-good scenario: Preferences → switch to Français (whole UI changes) → back to English; Book1 in the Project Manager → `F2` → `type Renamed` → `Return` → renamed → `ctrl+z` → undone.
