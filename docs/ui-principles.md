# Interface principles (overview)

Entry page, short, to know **what to respect** before touching the interface
of `stokesforge-app`. It copies nothing: each principle points to the page
that details it. In case of divergence, the code is authoritative, then
[`interface-standard.md`](interface-standard.md) (555 lines, decisions
of #217).

## 1. One building block, one gesture

Each interface element has **one** building block; it is not redone by hand.

- A typed number: `number_field` and the table of quantities (`quantities`),
  never a `DragValue` nor a `Slider`.
- A form: `form` / `choice` (`app/form.rs`); tabs:
  `tab_strip` (one level on top; two levels: primary on top, secondary
  vertical at the left, S-L9); a list: `list_row`; an empty state: `empty_state`; a field
  help: `hover_help`; a long job: `busy`.
- A color: `theme.rs` only (and the `color_field` picker). A spacing:
  `theme::SPACE_ITEM` / `SPACE_SECTION`, never a literal number.
- A page that scrolls (settings tab, form, help list):
  `theme::page_scroll_area()`, which keeps `SPACE_SECTION` under the last
  control (#347; seen on the Axes tab, #344). A tree, table, canvas or
  computed-height list stays raw with `// RAW-SCROLL(#347): <why>`.
- A key: the `app/shortcuts.rs` registry. A cursor: `app/cursors.rs`.
  A file picker: `file_picker.rs`.

These rules do not rest on everyone's memory: guards in
`crates/stokesforge-app/tests/guards.rs` make them fail. List and state:
standard, table "Building blocks and guards".

## 2. Windows have a type

Every window is modal (except the content ones: workbook, graph, matrix,
note) and built by `app/dialogs.rs`, never by a raw `egui::Window`.
The type fixes the footer, the button order, Escape and the focus: live
settings, creation on a draft, choice, wizard, read-only,
confirmation. Size and position are remembered per dialog.

- What exists, with size, pages and opening point:
  [`windows.md`](windows.md) (to be kept up to date with each window).
- The behavior: standard, § Windows and dialogs, § Confirmations.
- **A new window is mocked up and validated before the code.**

## 3. What can be undone is undone, what cannot is confirmed

Every modification of the model goes through an undoable command (Ctrl+Z).
Confirmation is asked only for what Ctrl+Z does not recover. A
visible object is set live (OK keeps, Cancel restores); an action that creates
works on a draft and its button carries its verb.
Detail: standard (S-F1, S-F3 to S-F5); mechanics: [`commands-and-history.md`](commands-and-history.md).

## 4. Nothing is greyed out or refused silently

A greyed-out element says why and what to do; a refused or brought-back input
is visible (red or orange frame, bubble). An error in a dialog
is written in its footer, not in the status bar. Status and
error messages are catalog texts, never a raw `Display`.
Detail: standard, § Disabled, § Input refused or brought back, § Messages and help.

## 5. Words come from the glossary

Every visible text is a Fluent key (English first, then French), written
with the glossary: one notion, one word, everywhere; capitalization and tone are
fixed there. The organization of menus and right-clicks has its grammar.
Detail: [`ui-texts.md`](ui-texts.md), [`menus.md`](menus.md),
skill `stokesforge-ui-texts`.

## 6. Numbers are displayed in a single way

A single formatter (`stokesforge_core::number_format`), the decimal comma
accepted on input, the exact value in a bubble, an uncertainty result rounded
to the uncertainty. A low R² is read from two signals, not from color alone.
Detail: standard, § Number display, § Decimal comma, § Low R².

## 7. Keyboard and mouse follow a registry

Shortcuts are constants of a single module, and Help > Keyboard
Shortcuts is generated from this registry: it cannot diverge. Detail:
standard, § Keyboard and mouse.

## 8. Interface state lives off screen

`stokesforge-app-state` carries the state, the texts and the errors put into
sentences, without egui; `stokesforge-app` only paints and translates
gestures into actions. An open dialog is a flag of `AppState`, not a local
variable of the shell. Detail: [`application/`](application/README.md),
[`architecture.md`](architecture.md).

## Where to go next

| I want to… | Read |
|---|---|
| create or modify a window | [`windows.md`](windows.md), then the standard § Windows and dialogs |
| add a field, a message, a greyed-out state | [`interface-standard.md`](interface-standard.md) |
| add a menu entry | [`menus.md`](menus.md) |
| write a visible text | [`ui-texts.md`](ui-texts.md) |
| add an icon | [`icons.md`](icons.md) |
| touch the main window, the grid | [`application/`](application/README.md) |
| know the conventions per domain | `.claude/conventions.md`, section "Application" |
