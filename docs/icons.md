# Interface icons

The icons of `stokesforge-app` are **Phosphor Icons, duotone variant**
(MIT license, copied to `crates/stokesforge-app/assets/icons/LICENSE`). The code
that displays them is `crates/stokesforge-app/src/icons.rs`; its `//!` says
*why*, this page says *what* and *how to add one*.

## When to put an icon — and when not

- **Yes**: a repeated action whose shape is recognized at a glance
  (close, detach, maximize, collapse, scroll the tabs), and the **object
  type** of the project (folder, workbook, graph, matrix, note) everywhere it
  appears.
- **No**: where a word is clearer ("#" of the gutter, a column name, a
  metadata summary); where the icon would only decorate. No icon in every
  menu on principle: an icon that distinguishes nothing is noise.
- **Always a tooltip**: an icon alone does not say what it does.
  The text comes from `tr!` (see `stokesforge-ui-texts`); `icon_button`
  requires it.
- **Never an emoji or a Unicode glyph** ("×", "↗", "📁") as an icon.

## Every action has its logo (rule set with #110)

Every **action** of the application — what is reached through a menu, a
shortcut or a toolbar — **has a logo**, even when it does not display one. The
logo is data of the action (its visual identity): this is what will allow it,
later, to be placed in a custom toolbar (#110) without waiting for one.
This does not change where the icon is **displayed**: menus do not have to show
one (rule above), and a word remains preferable where it is clearer.

Two quality levels:

- **Action of a fixed toolbar**: a polished Phosphor icon, added as
  below (validated contact sheet, color role, `tr!` tooltip).
- **Other action** (for example a data manipulation that no fixed toolbar
  carries): a **standard logo per family** — a template common to the
  family (data manipulation, for example) with **two letters** of the feature
  overlaid. No custom drawing: the cost of a new action stays close to zero.

State: the rule is set, **not yet locked by a test** and the standard logo
template does not exist yet (it will come with the action registry,
#110). Until then, a new action of a fixed toolbar gets its icon; for
the others, the `stokesforge-feature` skill asks to **name the planned logo**
in the report rather than silently omit it.

### Retouched icons

A Phosphor icon can be retouched: the file in `src/` then carries a
comment that says what changes (MIT notice of the neighboring `LICENSE` unchanged).
The file name carries `-modify` to flag it. Currently: `text-a-underline-modify` (thicker underline, text color), `pen-nib-modify` (stroke color) and `paint-bucket-modify` (fill color), #89.

## Color: neutral by default, color is rare

The stroke is dark and shared; color comes only from the **role** (`IconRole`),
and only when it carries information that the shape does not:

| Role | For | Hue |
|---|---|---|
| `Neutral` | everything else (default) | dark blue-gray |
| `Workbook`, `Graph`, `Matrix`, `Note`, `Folder` | the project object of this type | yellow, blue, violet, orange, sand |
| `Danger` | the destructive action of a context | red |

Indicative budget: no more than about one colored icon in four in the same
view. No color per tool family. The application theme is light and fixed
(`theme.rs`): no dark variant.

Phosphor duotone has two layers (background at 20% opacity, stroke
`currentColor`). `Image::tint` multiplies the pixels and cannot recolor them
separately: **each variant (icon, role) is baked into the SVG** by
`scripts/icons-build.sh`.

## Adding an icon

1. Search on <https://phosphoricons.com/?weight=duotone>; the file name
   is that of `vendor/phosphor-icons/SVGs/duotone/<nom>-duotone.svg`
   (`vendor/` is **not** versioned).
2. `scripts/icons-build.sh --add <nom>` copies the SVG into `assets/icons/src/`.
3. Add a line to `assets/icons/icons.tsv` (`nom<TAB>rôle[,rôle]`); a new
   role goes into `palette.tsv` (stroke, background), after the user's
   agreement.
4. `scripts/icons-build.sh --sheet` bakes the variants
   (`assets/icons/<nom>.<rôle>.svg`) and the **contact sheet**
   (`target/icons/sheet-color.png` and `sheet-gray.png`, via Inkscape). Look at
   it: in grayscale, the shapes must stay distinct.
5. In `icons.rs`: a variant of `Icon` and its lines of `VARIANTS`.
   The test `the_variants_table_follows_icons_tsv` fails if one is missing.
   The script first checks that every source of `icons.tsv` exists and every
   role is known (it reports all problems and replaces nothing), bakes into a
   temporary directory, then swaps the result in.
6. The baked variants and `src/` are versioned; commit both.

## Guards

- `no_icon_outside_the_component`: no module outside `icons.rs` cites
  `assets/icons` or an icon URI.
- `every_icon_loads`: every variant rasterizes with the real loader.
- Width: `ICON_SIZE` (16 pt) is the only size; the strip of a window
  header icon is `HEADER_ACTION_WIDTH` (`app/window_chrome.rs`, #50).
- In an interface test, a painted icon is a `Shape::Rect` with a texture;
  install `egui_extras::install_image_loaders` and paint **two images**
  (the texture is ready in the second) — see `find_icon_centers`.

## Performance

Embedded SVGs, rasterized on demand by `egui_extras` (resvg) once per
size × `pixels_per_point`, then kept as a texture: only the displayed icons
cost anything.

The zoom reset (#74) uses `arrows-out`: four arrows toward
the framing limits, distinct from the maximize corners. "New Project"
(#73) uses `file-plus`; the new graph keeps the `Graph` role.

Object Manager (#82) reuses the `Graph` icon in neutral role for its toolbar
button: its tooltip carries the panel name. The fixed panels (#100) reuse `Detach`, `Reattach` and the chevrons
`CaretLeft`/`CaretRight`/`CaretUp`/`CaretDown` for collapsing; no icon added.

The arrow of a list field (#234) is **not** an icon: a filled triangle painted by `list_frame::paint_arrow`, like that of the `ComboBox`.
Their headers offer neither `Close` nor `Maximize`.

The hover help of a field (#197, `app/hover_help.rs`) uses `question`
(`Icon::Help`, neutral role) faded by `theme::help_icon_tint`: the
question mark does not distinguish an object type, it says "a reminder exists here".

The "out of date" state of an analysis computation (#200) uses `warning`
(`Icon::Stale`, neutral role): header of the produced column, and row of the
series that plots it. The warning hue stays that of the letter
(`theme::stale_color`), not a palette role.

The lock between the total width and height of a graph (#407, Graph
Properties › General › Size) uses `lock-simple` (`Icon::LockClosed`) and
`lock-simple-open` (`Icon::LockOpen`), neutral role; the tooltip says what the
state is and what a click does.

## Color swatches (#89)

A color setting is a **small square** (`color_picker::color_button`).
An undefined color (empty value = automatic, for a fill:
none) is painted as a light square **crossed by a red diagonal line**; a
transparent color keeps the checkerboard. In a toolbar, the square is
preceded by an icon that says *which* color (`color_icon_button`: `LineColor`
for the stroke, `FillColor` for the fill, `TextColor` for the text);
in a form, by a label (`color_field`). Never a swatch alone.

Columns corrected in place (#212) carry a small neutral pencil
(`pencil-simple`, `Icon::ModifiedData`) in the header margin, distinct from the
triangle of an out-of-date computation. The bubble points to the settings kept in
Results; no extra state color or badge.
