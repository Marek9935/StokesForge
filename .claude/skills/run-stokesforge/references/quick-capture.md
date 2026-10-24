# Single capture without Xwayland, and text checks

Read from `run-stokesforge` for one screenshot of the native Wayland window, or to check texts.

A visual symptom is never diagnosed by reading code: a correct reasoning on misread code gives a false, convincing cause — it happened (the user's screenshots showed the opposite of the code reading). Tests cover geometry, not appearance.

## The built-in mechanism

`stokesforge-app` photographs itself. Two variables, declared in `crates/stokesforge-app/src/app.rs` (`SCREENSHOT_ENV`, `AUTOQUIT_ENV`):

```sh
STOKESFORGE_APP_SCREENSHOT=/tmp/stokesforge.ppm \
STOKESFORGE_APP_AUTOQUIT_MS=2000 \
timeout 60 cargo run -p stokesforge-app -- /path/to/project.stf
magick /tmp/stokesforge.ppm /tmp/stokesforge.png   # binary PPM (P6) → PNG, then read it
```

The window opens, captures itself and closes after the delay. `AUTOQUIT` is evaluated only on a painted frame: in an agent session without a real display the window may never paint, so nothing is written and nothing closes. Always bound the command (`timeout`), try **once** (at most a second time with `STOKESFORGE_X11=1`), never loop, and say in the report that the appearance was not looked at. This is the only way to photograph a native Wayland window; it shows **one** state of the main window, no interaction.

## Texts: the pseudo-language

For a text symptom (truncated label, untranslated text, narrow button), capture with `STOKESFORGE_LANG=en-XA`: every catalogue text is accented, 30 % longer and framed by `[[ ]]`. A text **without** `[[ ]]` is hard-coded (escaped `tr!`); a **missing** `]]` is cut by the layout.

## What no capture shows

A detached OS window (`show_viewport_deferred`), a compositor-dependent behaviour (position refused under Wayland, native file chooser), a drag. Ask the user: « Peux-tu m'envoyer une capture de l'état où ça se produit ? Je préfère regarder plutôt que déduire du code. »
