# Features to develop — feedback of 2026-09-23

Work packages drawn from the feedback of 2026-09-23. The defects from the same
message, all fixed, are no longer tracked here (see the GitLab issues).

Handover note: it will move to `docs/archive/` when its last box is checked.
The `app.rs:N` references date from 2026-09-23, before `app.rs` was split into
modules (#115): the code now lives in `src/app/` (module → role table in
`docs/application/coquille-et-code.md`). Each work package goes through the
`stokesforge-feature` skill — the four surfaces of an action (shortcut, menu,
disabling, label) stay in agreement only if someone thinks about them each time.

## Decisions already made on 2026-09-23

Not to be reopened without a new reason:

| Topic | Decision |
|---|---|
| Tab in the four header rows | **Extend.** The movement between fields that is lost there is accepted: the arrows, Enter and Shift+Tab replace it. |
| Drag-and-drop handle | **Band around the selection.** The conflict feared by the code comment does not exist: extending the selection is a press-move-release, and the handle arms **only after** the release. The two gestures follow each other in time, they do not compete for space. |
| Scope of the resizing | Clipping is a bug (B4), horizontal scrolling is a feature (F8). Both ship together: without clipping, the slider has nothing to scroll. |

---

## Batch A — adding a column leaves the banner

The workbook banner disappears entirely; its two gestures move to the context
menu, the keyboard and the icon panel.

### F1 — Remove the workbook banner

- [ ] Remove the « Ajouter une colonne » and « Importer… » buttons, and the
      "N rows × M columns" display.

**Two writes to handle**: `book_contents` (`app.rs:3653`) **and**
`detached_book_contents` (`app.rs:5592`). A comment in the file already says
"*Même correctif B5 que `book_contents`*" — the duplication is known and forces
a fix in two places.

Underlying goal: **factor out rather than duplicate**. `docs/application/fenetres-et-panneaux.md`
explains what must stay separate (the transient input buffers, because of the
`'static` closure of `show_viewport_deferred`) — a banner that only pushes
`Action`s does not run into it.

### F2 — Context menu « Ajouter une colonne » at the end of the grid

- [ ] Right-click in the empty area to the right of the last column → entry
      « Ajouter une colonne ».

Reminder of the house rule: the grid context menu is the **exact mirror of the
keyboard**, and the decision (what to offer, what to disable, how to name it)
lives in `AppState::grid_menu` → `GridMenu`, never in the egui code — that is
what makes it testable without a window in `tests/shell.rs`.

### F3 — Tab extends the grid from the whole header

- [ ] Extend the condition to the `Nom long`, `Unité` and `Commentaire` fields,
      in addition to `Formule`: `app.rs:4459-4484` (keyboard shortcut of
      `grid_contents`), `app.rs:3395-3415` (`header_field_widget`), and the
      detached duplicate `app.rs:5453-5484`.
- [ ] **Update the comment `app.rs:4455-4467` and the "Tab in
      the last column" section of `docs/application/grille.md`**: they describe
      the decision that this work package reverses, so they would become false.

⚠️ This is not the repair of an oversight: the current behavior is a documented
choice. The reversal is deliberate (see the decisions above).

### F4 — Column selection and deletion from the header

- [ ] **First check on screen what is really missing.** The letters row already
      selects the whole column and already opens `grid_edit_menu` (`app.rs:3095`,
      `app.rs:3145`). It remains to establish whether a click on a header field
      (`header_field_widget`, `app.rs:3350`) selects the whole column or only
      that field.
- [ ] **Wire up column deletion — everything is still to do.**
      `Command::DeleteColumn` exists in `stokesforge-commands`, but
      `rg DeleteColumn crates/stokesforge-app/src/` returns **nothing**: it needs
      an `Action` variant, its handling in `AppState::dispatch`, the menu entry,
      and a test in `tests/shell.rs`. This is not a one-line interface change.
- [ ] Identical context menu whatever the area, as soon as a whole column is
      selected, keeping the one of the letters row.

## Batch B — windows and chrome

### F5 — Origin-style icon panel

- [ ] Below the menu bar, a panel of illustrated shortcuts: new workbook, new
      graph, import a file.

**Dependency**: import leaves the workbook banner with F1. Ship F5
**before or with** F1, otherwise import has no entry point left.

*To clarify before designing*: the actions to include beyond the three
mentioned; the icon set (is there one already in `stokesforge-app`, or must one
be chosen?); permanent or collapsible panel.

### F6 — Window outline, no shadow

- [ ] A thin grey line, slightly rounded corners, on the detached window
      **and** the main window; remove the current shadow effect.

To be done in the same pass as B1/B3 (highlight outline): both paint the edge
of the same window, so handling them separately would repaint the same area
twice.

### F7 — Column width × 4/3

- [ ] Increase the default width by a third.

Check the effect on B4: wider columns overflow sooner.

## Batch C — grid and drag and drop

### F8 — Horizontal scrolling of the workbook

- [x] Horizontal slider in the grid when the columns overflow.
- [x] Shift+wheel drives this slider through the native behavior of `egui::ScrollArea`.
- [x] The sheet tab strip also scrolls horizontally (its own
      `egui::ScrollArea`, independent from the grid's) instead of shrinking its
      tabs or folding the hidden sheets into a "…" menu (revised on 2026-09-23:
      that was the initial approach, dropped to keep the original tab style, at
      natural size).

The action banner stays outside the scrollable area. The floating window and
the detached window use the same rule, and the same function
(`sheet_tabs`, `crates/stokesforge-app/src/app/workbook.rs`). Selecting a sheet
whose tab is out of view (click, keyboard, script) scrolls the strip to bring it
back (`Response::scroll_to_me`, triggered only when the selection really
changes, not on every frame).

### F9 — Drag handle around the selection

- [ ] Move the grip area (`COLUMN_GRIP_WIDTH`, `app.rs:514`; current area
      `app.rs:4304-4340`) from the edge of the grid to the perimeter of the
      selection.
- [ ] **The handle arms only after the click is released** — this is what
      separates it from the selection-extension drag.
- [ ] Update the comment `app.rs:4308-4321`, which today justifies the position
      at the edge by a conflict that this rule dissolves.

### F10 — Reduce the height of the header strip

- [ ] The header strip of floating/detached windows (`window_header`,
      `WINDOW_HEADER_HEIGHT = 28.0`, `crates/stokesforge-app/src/app.rs`) is
      perceived as taller than necessary — feedback of 2026-09-23, in reference
      to the greyed area on hover of the collapse chevron ("v"), which already
      takes the full height of the strip (`header_icon`, the hover rectangle is
      `rect.height()`, the same value).

*To clarify before designing*: does the discomfort come from
`WINDOW_HEADER_HEIGHT` itself, or from the space added around it (margin of the
`Frame`, `style.spacing.window_margin`, or the default spacing between the
header strip and the tools row that follows, on the workbook side)? A capture
with a pixel measurement would answer faster than a new question.

**To be done in the same area as F6** (window outline): both retouch the
appearance of the same strip, so avoid two separate passes.

### F11 — Define the graph behavior on resize

- [x] Asked of the user (2026-09-23): the graph always keeps the 4:3 ratio of
      `DEFAULT_FIGURE_SIZE` and scales "contain" — maximized in the available
      space by taking the limiting dimension (width or height), growing as well
      as shrinking. No clipping, no scrolling for the figure itself. Identical
      rule in floating and detached mode (a single function,
      `graph_view_contents`, covers both).
- [x] Implemented in `graph_view_contents`
      (`crates/stokesforge-app/src/app.rs`); test adapted
      (`le_rendu_garde_son_ratio_et_se_met_a_l_echelle_dans_la_zone_disponible`);
      `docs/application/` updated.

---

## Suggested order

1. **F5** (icon panel) — prerequisite to F1, otherwise import disappears.
2. **F1 + F2 + F3** — a single coherent work package: the banner goes away, the
   context menu and Tab replace it.
3. **F4** — the heaviest of batch A, because of the `DeleteColumn` wiring.
4. **F6** with B1/B3, **F7** whenever wanted.
5. **F8** after B4.
6. **F9** — independent.
7. **F10** with F6 — same area, and its exact scope remains to be clarified (see
   above).
8. **F11** after defining the expected behavior with the user.
