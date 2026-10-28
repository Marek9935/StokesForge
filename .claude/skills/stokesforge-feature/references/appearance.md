# Sheet: appearance — scrolling, tabs, lists, text levels, forms, colours, icons

Read when painting a scroll bar, tabs, a selectable list or tree, a section title, a form, an exclusive choice, a spacing, a colour field, or an icon.

## Where the rule lives

- Scroll bars: `docs/application/theme-i18n-system.md` § Theme ; `.claude/conventions.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards (`theme::scroll_style`).
- Four roles (tabs, list, exclusive choice, tool toggle): `docs/application/windows-and-panels.md` § Tabs, lists and selection: four aspects, four roles (#169).
- Bold, section titles, state colours, grid form, `choice`, spacing: `.claude/conventions.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards, point "common appearance (#169)" ; `docs/interface-standard.md` § Choices, layout.
- Colour picker, usages, saved colours: `docs/application/graph.md` § Graph properties ; `.claude/conventions.md` § Application (`stokesforge-app` and `stokesforge-app-state`): building blocks and guards, point `color_picker::color_field`.
- Empty colour = automatic, automatic ink: `docs/rendering.md` § Text and scale colors: automatic ink.
- New colour field in the model (`#[serde(default)]`): `docs/persistence.md` § The format is not written into the model.
- Icons: `docs/icons.md` § When to put an icon — and when not ; `docs/icons.md` § Every action has its logo (rule set with #110) ; `docs/icons.md` § Color: neutral by default, color is rare ; `docs/icons.md` § Adding an icon ; `docs/icons.md` § Color swatches (#89).
- General principle: `docs/ui-principles.md` § 1. One building block, one gesture.
- Guards: `docs/tests.md` § Guards not to break.

## Check

- [ ] no local `ScrollStyle`; a bar's room read by `theme::scroll_bar_allocated_width`
- [ ] tabs by `tab_strip`; lists and trees by `list_row` / `ListRowBackground`; composed rows at `LIST_ROW_HEIGHT` (`with_list_row_height`)
- [ ] keyboard-walked list (S-K9): `list_row::read_navigation` and `neighbour` on visible rows, focus by `paint_focus_outline`
- [ ] bold by `theme::bold`; title by `theme::section_title` (no `separator()` just before); `theme::table_header`, `theme::secondary_text`; never `bold(…).small()`
- [ ] no hard-coded colour outside `theme.rs` and `color_picker.rs`; form by `app/form.rs`, spacing by `theme::SPACE_*`
- [ ] colour by `color_picker::color_field` with its `ColorUsage` and `PickerEnv`, never a `#RRGGBB` text field
- [ ] icon by `icons::icon_image` / `icon_button` with a `tr!` tooltip, never a glyph or emoji; `IconRole::Neutral` unless object type, destructive action or state; the action's logo named in the report
- [ ] an egui test that depends on appearance calls `theme::install`
