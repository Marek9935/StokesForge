# Interface behavior standard (#217)

The **single** source for how fields, messages, dialogs, keyboard and mouse
react in `stokesforge-app`: the same gestures, messages, fields and windows
react the same way everywhere. Decisions of the user of
2026-10-10, recorded in issue #217. Menu organization is in
[`menus.md`](menus.md), words and capital letters in
[`ui-texts.md`](ui-texts.md); this page states the
**behavior**.

Each rule has a **code** `S-…` (S for *standard*: `S-N1`, `S-F7`,
`S-R1`…) and a **readable name**: the [rule index](#rule-index) gives
both and states the rule in one sentence, so nobody has to open an issue. The
`S-` prefix avoids confusion with the rules `R1`–`R16` of
[`ui-texts.md`](ui-texts.md), which are of another family
(writing of texts). The letters give the subject: N numbers, D data display,
T texts, M messages, F windows, G disabled, C choices, L layout,
K keyboard and mouse, X cross-cutting, P long tasks; `S-R1` to `S-R16` are
the rulings of the second decision form of #217.

## How to read this page

- A section or rule without a mark is **in place**: the code follows it and
  a guard or tests hold it. Only what is not yet delivered carries
  *planned* (delivery issue in parentheses). The issue numbers of
  past deliveries are not in the headings: they are grouped in the
  "History: #…" line that closes each section.
- A *planned* rule is already the rule: **all new or changed code
  follows it**. Existing code that departs from it is not a model, it is a debt
  tracked by the issue of the section.
- The **building blocks** and **guards** of the table below do not exist as long
  as they are *planned*: do not call them, do not count on them. The
  commit that creates them switches them to *in place*, adds the guard to the table of
  `stokesforge-verification` and updates the skills.

## Rule index

One code, one name, one sentence. The sections below give the detail and the
mechanics.

| Code | Name | The rule |
|---|---|---|
| S-N1 | `champ-nombre-unique` | A single numeric field, the "number" building block; `▼` only if the quantities table offers values. |
| S-N2 | `molette-champ-a-liste` | The mouse wheel changes only a list field, on hover; a field without a list is never changed by the wheel. |
| S-N3 | `nombre-applique-a-la-validation` | A number applies on Enter or when leaving the field; one history entry per validation. |
| S-N4 | `virgule-decimale-acceptee` | "," and "." are accepted in a field that holds a single number; the interface always writes the dot. |
| S-N5 | `virgule-decimale-cellules` | The same reading applies in Numeric (and Mixed) cells. |
| S-N6 | `saisie-illisible-refusee` | An unreadable input is refused: red frame, tooltip that states the rule, the old value comes back. |
| S-N7 | `hors-bornes-ramene` | An out-of-bounds value is brought back to the bound and flagged in orange for 2 s. |
| S-N8 | `grandeur-une-definition` | A single quantities table gives bounds, step, decimals, unit and offered values. |
| S-N9 | `barres-outils-listes-editables` | The editable lists of the toolbars (size, thickness: choose 12 or type 15) go through the same building block. |
| S-N10 | `unite-dans-le-libelle` | The unit is in the label ("Line width (pt)"), never as a suffix of the field. |
| S-N11 | `champs-couples` | "from" ≤ "to": the constraint between two fields is refused, on the second; exception: From / To of an axis (From > To is the reversed axis). |
| S-N12 | `aucun-slider` | No slider: opacity is a field, like R, G and B. |
| S-D1 | `chiffres-resultats-et-graduations` | Results and ticks: 3 significant digits, `e` notation outside [1e−4, 1e6). |
| S-D2 | `chiffres-grille-et-champs` | Grid and fields: 6 significant digits; while editing, the full value. |
| S-D3 | `notation-scientifique` | Typographic exponent on the figure (1×10⁻⁵), `e` notation in the grid and fields. |
| S-D4 | `dates-iso` | Dates displayed in ISO 8601; input in ISO or day first, never month first. |
| S-D5 | `chemins-nom-seul` | File name alone in messages, full path in a tooltip. |
| S-T1 | `apercu-apres-pause` | A text field applies on Enter or on leaving; previews (analysis, import) recompute while typing, after a 0.3 s pause. |
| S-T2 | `texte-refuse-garde` | A refused text stays in the field under the red frame; a refused rename leaves the field open. |
| S-T3 | `erreur-typst-dans-le-champ` | The Typst error of a rich text is painted, once the field is left, as a red frame and tooltip; the text is applied as is. |
| S-T4 | `edition-sur-place-un-contrat` | A single in-place editing contract: everything selected, Enter and a click elsewhere validate, Esc cancels, Tab validates and moves to the next. |
| S-T5 | `exemple-de-champ-vide` | The example text of an empty field is the value taken if empty or a syntax example, never an instruction. |
| S-T6 | `champ-formule-icone-aide` | A formula field has only the "?" icon as help. The `F(x)=` cell of the workbook grid has it only while it is edited (#404). |
| S-M1 | `aide-trois-niveaux` | Three help levels: plain text (2 lines), "?" icon for the reminder of a field, bare tooltip only for buttons, icons, tabs, menus. |
| S-M2 | `pas-de-bulle-nue-sur-un-champ` | A bare tooltip on a form field is forbidden. |
| S-M3 | `bulle-de-bouton` | Icon-only button: name, effect, shortcut; text button: effect and shortcut without repeating the label. |
| S-M4 | `barre-etat-trois-niveaux` | The status bar distinguishes information, warning (warning tint) and error (red, icon). |
| S-M5 | `information-qui-s-efface` | An information message fades after ≈ 8 s; warning and error stay until the next action. |
| S-M6 | `historique-des-messages` | A click on the status bar shows the last 50 messages, timestamped. |
| S-M7 | `erreur-dans-le-pied` | An error raised in a modal window goes in the footer of that window, never in the status bar. |
| S-M8 | `echec-grave-en-dialogue` | A serious failure outside a window (failed save, unreadable project) is a blocking dialog with a possible action. |
| S-M9 | `un-canal-par-information` | A piece of information has a single channel, apart from the two exceptions of S-R13. |
| S-M10 | `erreur-de-script-renvoi` | A script error says "Script failed. See the console" in the status bar (flagged as an error); the detail is in the console. |
| S-M11 | `barre-etat-sans-revision` | No "Revision N" and no "Reopen Last Import…" in the status bar. |
| S-F1 | `reglages-en-direct-ou-brouillon` | Visible object: live change, OK/Cancel; action that creates: draft, and button named by its verb. |
| S-F2 | `lecture-seule-close` | A read-only window has only a Close button. |
| S-F3 | `confirmer-l-irreversible` | Confirmation only for what Ctrl+Z does not recover; an undoable deletion asks nothing. |
| S-F4 | `confirmation-forme` | Confirmation = modal dialog, Cancel + verb; Quit and Open: Discard Changes · Cancel · Save; never "Keep" alone. |
| S-F5 | `confirmation-clavier` | Enter and Esc = Cancel, default button framed on Cancel; exception: Enter = Save in the trio. |
| S-F6 | `ordre-des-boutons` | Dialog: secondary buttons, Cancel, primary on the right; wizard: Cancel on the left, Back · Next · Finish on the right. |
| S-F7 | `niveaux-d-echap` | Esc first closes an open list, then gives the field its value back, then does the dialog Cancel. |
| S-F8 | `geometrie-memorisee` | Size and position of a dialog are remembered per kind of dialog, in the preferences. |
| S-F9 | `focus-initial` | The first input field has the focus on opening. |
| S-F10 | `bouton-principal-grise` | A disabled primary button states its reason in the footer and in a tooltip. |
| S-F11 | `nature-de-fenetre` | A modal window has a nature (`DialogNature`) that sets its footer, its default size (Small 480 x 170, Medium 480 x 420, Large 640 x 500), its initial focus and its default button; a free size is allowed only with a justification, for an interactive window (settings, live settings with Close only like Preferences, draft, wizard). |
| S-G1 | `grise-avec-sa-raison` | Every disabled element carries its reason, which says what to do ("Select a series first"). |
| S-G2 | `raison-en-bulle-propre` | The reason is the `.disabled` tooltip of the element itself, never mixed with the normal tooltip nor put on a group. |
| S-G3 | `undo-redo-grises` | Disabled Undo and Redo say "Nothing to undo"; enabled, they name the action ("Undo Rename"). |
| S-G4 | `champ-dependant-d-une-case` | A field that depends on a checkbox is disabled, value visible and kept; hidden only if the whole section loses its meaning. |
| S-C1 | `radios-ou-liste` | Radios for 2 to 3 options, drop-down list from 4. |
| S-C2 | `radios-cote-a-cote-ou-empiles` | Horizontal radios if each label is at most 3 words, vertical otherwise. |
| S-C3 | `choix-de-menu-coche` | An exclusive choice in a menu is a check mark alone, without radio. |
| S-C4 | `champ-vide-egale-auto` | A number or a text that has an Auto: emptied field = Auto (the text size of the format bar has one). |
| S-C5 | `bascule-case-a-cocher` | A toggle is a checkbox, label on the right. |
| S-L1 | `formulaire-en-grille` | A form is a two-column grid: labels on the left, aligned fields. |
| S-L2 | `quatre-largeurs-de-champ` | Four named field widths in `theme.rs`: a few digits, short, medium, full. |
| S-L3 | `titre-de-section-entre-blocs` | A section heading between two blocks, never a rule line alone. |
| S-L4 | `deux-espacements` | Two spacing constants in `theme.rs`; no numeric `add_space` elsewhere. |
| S-L5 | `pas-de-section-repliable` | No collapsible section in a form; too long, switch to tabs. Only exception: the "Advanced" block (S-L8). |
| S-L6 | `trois-niveaux-de-texte` | Three text levels named in `theme.rs`: section heading, table header, secondary text; never `bold(…).small()` written in place. |
| S-L7 | `hauteur-de-ligne-unique` | A list or tree row has a single height, `LIST_ROW_HEIGHT`, composed or not. |
| S-L8 | `bloc-avance-replie` | Settings that are almost never touched, all Auto by default, go in a collapsible "Advanced" block, closed by default, at the bottom of their section; never a common setting nor one without Auto. |
| S-L9 | `onglets-secondaires-a-gauche` | One level of tabs: the strip is on top. Two levels: the primary tabs are horizontal on top, the secondary tabs vertical at the left of the content; the secondary strip stays fixed, only the content scrolls. |
| S-K1 | `registre-des-touches` | All keys are in the registry of `app/shortcuts.rs`; no `consume_key` elsewhere. |
| S-K2 | `suppr-immediat-annulable` | Del deletes the selection right away, undoable, everywhere (the grid empties first). |
| S-K3 | `double-clic-par-sorte` | Double-click: text or name → in-place editing; object → Properties; empty background of a graph → view reset; Project Manager → activates the window, never closes it, and collapses a folder. |
| S-K4 | `renommer-f2-ou-clic-lent` | Rename in the Project Manager: F2, or a slow click on an already selected name. |
| S-K5 | `aide-raccourcis-generee` | Help › Keyboard Shortcuts is generated from the key registry. |
| S-K6 | `tab-suit-la-lecture` | Tab follows the reading order: top-bottom, left-right. |
| S-K7 | `outils-dans-la-barre-tools` | No tool selector in a window: tools stay in the Tools bar, the object first; an accepted exit (analysis tools), always justified in the code and the docs. |
| S-K8 | `gestes-dans-la-matrice` | Every mouse wheel or double-click gesture is filed in the `gestures.md` matrix, where one sees whether the same gesture does something else elsewhere. |
| S-K9 | `fleches-dans-les-gestionnaires` | In the focused Project Manager and Object Manager, ↑ and ↓ move the selection from one visible row to the next (without opening or writing to the history), Enter opens the window, expands or collapses the folder, or opens the Properties of the object; the cursor row carries the focus outline. |
| S-X1 | `ctrl-molette-zoom` | Ctrl+wheel zooms, everywhere. |
| S-X2 | `curseurs` | Open then closed hand for dragging, resize arrow on a handle, finger on a clickable element that is not a button. |
| S-X3 | `fenetre-preferences` | **Planned (#148).** Application settings go in Edit › Preferences…; menus keep the frequent display toggles. |
| S-X4 | `registre-d-actions` | **Planned (#110).** One table per action (label, logo, shortcut, disabling and its reason) read by menus, bars and right-click. |
| S-X5 | `trait-de-depot-unique` | A single "drop here" stroke (`theme::drop_target_stroke`, light blue) on the border of the one window that is the drop target: external file on a workbook or on the application, columns and rows on a graph. |
| S-P1 | `indicateur-apres-0-5-s` | Beyond ≈ 0.5 s: spinning indicator + word at the place of the result; progress bar if the steps are known. |
| S-P2 | `stop-au-dela-de-3-s` | Stop button for any work of more than a few seconds. |
| S-P3 | `etat-vide-explicite` | An empty state is a centered greyed-out sentence that says what is missing and the gesture to create it. |

The rulings of the second form of #217; each one decides a conflict between
two rules above:

| Code | Name | The rule |
|---|---|---|
| S-R1 | `molette-au-survol` | The wheel changes a list field on hover, without focus; Ctrl+wheel stays zoom. |
| S-R2 | `illisible-refuse-hors-bornes-ramene` | Unreadable → refused; out of bounds → brought back and flagged. |
| S-R3 | `chiffres-affiches` | 3 significant digits for results and ticks, 6 for grid and fields. |
| S-R4 | `confirmation-entree-annule` | In a confirmation, Enter and Esc do Cancel. |
| S-R5 | `destructeur-rouge-au-survol` | The destructive verb is normal at rest, red on hover and keyboard focus. |
| S-R6 | `virgule-formules-et-listes` | The decimal comma applies in a field with a single number; formulas and value lists require the dot; Mixed follows Numeric. |
| S-R7 | `double-clic-par-sorte-et-zoom` | One double-click rule per kind of thing; Ctrl+wheel zooms everywhere. |
| S-R8 | `valeur-ramenee-orange` | A value brought back to the bound is flagged in orange; red is reserved for refusal. |
| S-R9 | `separateur-par-graphe` | The decimal separator is a per-graph setting, independent of the language. |
| S-R10 | `champ-vide-selon-auto` | An emptied field means Auto if it has an Auto, otherwise it is refused. |
| S-R11 | `apercu-texte-et-nombres-apres-pause` | Previews recompute while typing after a pause, text as well as numbers; the rest applies on Enter. |
| S-R12 | `fleche-seulement-avec-valeurs` | `▼` only when the quantities table offers values; click in the text = everything selected. |
| S-R13 | `canal-unique-deux-exceptions` | A single channel per piece of information, with two written exceptions: script error (status bar + console) and reason of the disabled button (footer + tooltip). |
| S-R14 | `pied-erreur-avant-raison` | In the footer of a dialog, the error comes before the reason of the disabled button; the reason comes back when the error disappears. |
| S-R15 | `barre-etat-sans-contenu-permanent` | No permanent content in the status bar (neither revision nor import link). |
| S-R16 | `opacite-sans-slider` | Opacity is a field, not a slider. |

## Building blocks and guards

| Element | Role | State |
|---|---|---|
| refused / brought-back field state (`field_state`) | red or orange frame, tooltip, cycle of a validated text field | **in place** (#219) |
| "number" building block (`number_field`) + quantities table (`quantities`) | the only numeric field | **in place** (#220) |
| guard: no `DragValue` nor `Slider` (`no_numeric_field_outside_the_building_block`) | | **in place** (#220) |
| number formatter (`stokesforge_core::number_format`) | the only displayed formatting | **in place** (#222) |
| in-place editing building block (`app/inplace_edit.rs`) | a single S-T4 contract; previews with a 0.3 s pause | **in place** (#224) |
| "?" icon (`app/hover_help.rs`: `hover_help`, `help_text`, `help_beside`) | reminder of a field, placed on the right (S-M1, S-T6) | **in place** (#225) |
| guard: no bare tooltip on a form field (`no_bare_tooltip_on_a_form_field`) | checkbox, radio, text/number field, field label; menus and `…_contents` exempt | **in place** (#225) |
| guard: no `add_enabled` without a reason (`no_disabled_element_without_its_reason`) | call chain read up to `.on_disabled_hover_text`; `add_enabled_ui` (group) exempt | **in place** (#226) |
| "form" building block (`app/form.rs`: `form`, `Form`, `choice`), widths and spacings of `theme.rs` | two-column grid, radio/list choice (S-C1, S-C2) | **in place** (#227) |
| exclusive choice in a menu (`menu_sections::menu_choice`) | check mark alone (S-C3) | **in place** (#227) |
| "framed texts" building block (`app/form.rs`: `toggle_texts`) | toggle exclusive choice, rounded frame around each label (active: solid accent fill; inactive: outline only); reserved for the gesture row of the analysis preview (exception S-K7) | **in place** (#270) |
| page scroll area (`theme::page_scroll_area`) + guard: no raw scroll area without justification (`no_raw_scroll_area_without_justification`, witness `the_scroll_area_guard_sees_a_bare_area`) | end margin `SPACE_SECTION` under the last control of a scrolling page; trees, tables, canvas: `// RAW-SCROLL(#347)` | **in place** (#347) |
| guard: no literal `add_space` outside `theme.rs` (`no_literal_add_space_outside_the_theme`) | | **in place** (#227) |
| message area of the footer of `modal_dialog_with`, initial focus, remembered size, `modal_wizard`, `modal_live_settings`, `modal_read_only` | button order, error and reason in the footer, Esc, S-F8, S-F9 | **in place** (#228) |
| window nature (`app/dialogs.rs`: `DialogNature`, `SizeClass`, `InteractiveSize`, `PlainSize`, `DialogFooter::always` / `gated`) | footer, size class, initial focus and default button imposed by exhaustive `match` (S-F11) | **in place** (#300) |
| guard: no free size without justification (`no_free_size_without_justification`, witness `the_sizes_guard_sees_a_bare_free_size`) | `InteractiveSize::Free` refused without the marker `// FREE-SIZE(#300): <justification>` (S-F11) | **in place** (#300) |
| guard: the window catalog and the code agree (`the_window_catalog_and_the_code_agree`, witness `the_catalog_guard_sees_a_deviation`) | every `stokesforge_…` identifier passed to a `modal_*` is in [`windows.md`](windows.md), and every catalog row has its call; does not read sizes (S-F11) | **in place** (#300) |
| confirmations and serious failure (`modal_confirm`, `modal_save_changes`, `modal_alert`; `app/confirmations.rs`) | Cancel by default, trio Discard Changes · Cancel · Save, S-M8 | **in place** (#229) |
| key registry of `app/shortcuts.rs`, `HELP`, Help › Keyboard Shortcuts | S-K1, S-K5 | **in place** (#231) |
| guard: no `consume_key` outside `app/shortcuts.rs` (`no_consume_key_outside_the_registry`) | | **in place** (#231) |
| cursors (`app/cursors.rs`) and guard `no_cursor_outside_the_cursors_module` | S-X2 | **in place** (#231) |
| guard: no tool selector outside the Tools bar (`no_tool_picker_outside_the_tools_bar`, witness `the_tools_guard_sees_a_bare_tool_picker`) | exclusive choice whose arguments name a tool, refused outside `toolbar.rs` without the marker `// TOOL-IN-WINDOW(#299): <justification>` (S-K7) | **in place** (#299) |
| guard: no gesture outside the matrix (`no_gesture_outside_the_matrix`, witness `the_gestures_guard_sees_a_gesture_outside_the_matrix`) | every file that reads a mouse wheel or a double-click is cited by [`gestures.md`](gestures.md) (S-K8) | **in place** (#299) |
| "work in progress" building block (`app/busy.rs`: `work_in_progress`, `indicator`) | S-P1, S-P2 | **in place** (#232); wired to the background import and the script console |
| "empty state" building block (`app/empty_state.rs`: `empty_state`) | S-P3 | **in place** (#232) |

Already in place and reused by this standard: `modal_dialog_with`
(`app/dialogs.rs`), `hover_help` (`app/hover_help.rs`), `theme::section_title`,
`MenuSections::section`, `color_field`.

## Numeric fields

Building block: `stokesforge_app::number_field::number_field(ui, sel, &mut valeur,
&quantities::GRANDEUR)` (`src/number_field.rs`), table: `src/quantities.rs`
(`Quantity`: bounds, step, large step, decimals, integer or not, offered
values). A new quantity is added to the table, never to the call; the 28
`DragValue` and the opacity `Slider` are migrated, the object format bar
(thickness) and the text size (`quantities::TEXT_FORMAT_SIZE`, "Auto"
entry) too (S-N9).

- **A single "number" building block** (S-N1 → S-R1, S-R12). Look: text field, with
  `▼` **only** if the quantities table offers values (thickness,
  font…); an axis bound or a fit parameter has none.
  - Click in the text: all the text is selected, without opening the list.
    The list opens with `▼` or Alt+↓.
  - Closed list: ↑/↓ = one step, Shift+↑/↓ = one large step.
  - Open list: the wheel scrolls the list, ↑/↓ highlights, Enter
    chooses, Esc closes.
  - In a modal dialog, the list follows #9 (its own `egui::Area`).
- **Mouse wheel** (S-N2, S-X1 → S-R1; in place, #233, #235): only a **list**
  field reacts, on **hover** (no focus needed); one notch = the next or
  previous entry of the list (1, 1.5, 2, 3, 4…), a value typed outside the list jumps to
  the neighboring entry, at the ends the value stays. A field **without a list**
  (axis bound, fit parameter) is never changed by the wheel: the
  window scrolls, as when the cursor leaves the field. Notches in a row
  form **one** history entry. Ctrl+wheel stays zoom.
  `number_field::step_preset`.
- **Look of a list field** (S-N1, S-N9; in place, #234): a single frame,
  solid arrow inside on the right, like the drop-down list; **white
  background** if the field is editable by keyboard, **grey background** if only the list
  chooses. Fills taken from the theme (`list_frame::editable_fill` / `list_only_fill`).
  The number field applies it, including the editable text size list of
  the format bar.
- **Application** (S-N3): on Enter or when leaving the field; Esc gives back
  the old value; one history entry per validation.
- **One quantity, one definition** (S-N8): a single table gives bounds, step,
  large step, decimals, unit and offered values (the wheel follows these
  values, #233; the step serves the keyboard arrows). Thickness: 0 to 20 pt,
  decimals possible by keyboard.
  The caps of an analysis setting (degree 9, iterations 5000, peaks **100**,
  components 4) are not written in the table: it reads them in
  `stokesforge_analysis::ops` (#260), the source that `Operation::validate`
  also applies to `pyst` and to the file.
- **Toolbars** (S-N9): editable lists (choose 12 or type 15), same
  building block.
- **Unit** (S-N10): in the label ("Line width (pt)"), never as a suffix,
  including % and ° of the color picker.
- **No slider** (S-N12 → S-R16): opacity is a field, like R, G and B.
- **Coupled fields** (S-N11, in place, #240): "from" ≤ "to" (analysis
  ranges, `analysis_forms::flag_pair_refusal`), error on the **second** field.
  **Exception: From / To of an axis** — an axis can be reversed (cm⁻¹), no
  constraint, no error: From > To *is* the reversed axis (#344). A constraint between two fields is refused; only a fixed
  bound is brought back.

History: #220 (building block), #233–#235 (wheel, look of a list field), #240 (S-N9, S-N11).

### Auto (S-C4, S-R10, S-G4)

Mechanics: `quantities::AutoKind` (`None`, `Field`, `Group`) on the quantity,
`number_field::auto_number_field` (`Option` value, greyed-out text "Auto (…)")
for a `Field`, `NumberResponse::auto` for a `Group` (`AXIS_BOUND`). The computed
bounds come from `DrawnPanel` (`stokesforge-render`), published by
`graph_window::publish_axis_bounds`. The text size of the format bar
has its "Auto" entry (editable list, S-C4).


- Choice in a list: "Auto" entry at the top.
- Number or text that has an Auto: emptied field = Auto, displayed "Auto (0–4000)"
  in grey (computed value). Number **without** Auto: emptied field refused.
- Linked group (From / To of an axis, #344): no more "Auto range" checkbox; From and To
  are **always editable**. As long as nothing has been typed, they show the
  bounds **computed** at plotting, in display order (a reversed axis
  shows its high bound in From); typing one **freezes both** (the other is
  taken from the computation) and the direction of the axis follows the order (From > To = reversed, no more
  "Inverted axis" checkbox). The "Auto" button of the row (or the typed word `auto`)
  gives both bounds back to the data and **keeps the direction**. Emptying From or To is
  refused (red frame, tooltip "Type Auto to follow the data").
- The word `auto` typed on the keyboard is accepted by a field whose quantity has
  an Auto (including the Auto of the group), and no other text; elsewhere, it is an
  unreadable input. The optional numbers of the analysis (fit
  parameters, `a`/`b`), which do not go through `number_field`, also read
  `auto` and `a` as an emptied field (`parse_optional`, #247).

History: #221.

## Input refused or brought back

Building block: `stokesforge_app::field_state` (`src/field_state.rs`, public module
like `color_picker`), three stages that the "number" (#220) and
"text" (#224) building blocks reuse instead of repainting a frame:

- `show_issue(ui, &response, &issue)`: frame and tooltip of a `FieldIssue`
  (`refused` red, `clamped` orange) on an already placed field — the stage of the
  **only** painting of a field defect, also for an error coming from elsewhere
  (Typst, S-T3);
- `flag` / `active` / `clear`: the state that survives the input (orange 2 s,
  `CLAMPED_SECONDS`);
- `edit_text(ui, sel, valeur_affichée, lecture)`: the full cycle of a validated
  field, `lecture` returning a `Verdict` (`Valid`, `Clamped`, `Refused`);
  `decide` is its pure decision.

A new form field that reads an input goes through `edit_text`; an already
known error goes through `show_issue`. The tints are `theme::field_refused_color`
and `theme::field_clamped_color`. Wired: the "number" building block (#220) and the
renames (#224, `app/inplace_edit.rs`). Also wired (#240): the
constraint between fields (S-N11) and the Typst error of rich texts (S-T3).

- **Unreadable** (`abc`, `1..2`): **red** frame, tooltip that states the rule,
  the old value comes back when leaving the field (S-N6, S-N7 → S-R2, S-R8). The red
  is painted from the typing; on Enter the field stays open, text kept; on
  leaving the old value comes back and the refusal stays flagged 2 s; Esc gives
  back the value without a frame.
- **Out of bounds**: brought back to the bound, **orange** frame (warning
  tint) for 2 s, tooltip "Set to the maximum, 20". Red
  is reserved for "refused", with **one written exception**: low R² (see
  "Low R²" below), which has its dedicated token (`theme::r_squared_color`)
  and is never used for a field refusal.
- **Refused text** (S-T2): same rule, text kept; a refused rename leaves the
  field open.
- **Typst error of a rich text** (S-T3, in place, #240): title, axis titles,
  legend — every field placed by `plot_settings::formula_field` — paints, **once the field is
  left** (not while typing), the red frame and the Typst message in a tooltip
  (`stokesforge_render::formula::diagnose`, memoized per text). The text is
  applied as is: the graph falls back to the raw text, **in a red frame on
  screen** (`EguiBackend::draw_text`); never in an export or a copy
  (SVG backend).
- Never an erasure or a silent correction.

History: #219; wiring: #220, #224, #240 (S-N11, S-T3).

## Low R²

- **Tiers** (`stokesforge_app_state::r_squared`, neutral data, strict
  bounds, cumulative): R² < 0.95 **orange**; < 0.8 **red**; < 0.6 red
  **and** dialog box. A negative R² falls in the < 0.6 tier; `NaN`
  warns of nothing. The colors apply to every `stat.r_squared` value
  (fits and peaks): analysis window preview, Results panel,
  calculation settings — `exact_result_label` paints them all.
- **Not only color**: the value turns **bold** and its tooltip adds
  "Low R² …"; the tint is a light/dark token (`theme::r_squared_color`).
  This red is the only one allowed outside the field refusal (`field_refused_color`);
  it stays a distinct token.
- **Dialog box**: warning, not failure — `AlertKind::LowRSquared`,
  footer "Close" alone (`modal_read_only`), Enter and Esc close. It opens
  **when the R² of a saved calculation changes**: apply, manual or
  automatic recalculation (`AppState::watch_low_r2`, fingerprint per calculation — a
  snapshot that does not change the R² does not reopen it), or when applying a
  "Show" calculation (whose values are not kept, seen only at apply).
  Never for the live preview nor when opening a project. An existing save or open
  alert is not overwritten; several calculations
  at the same snapshot: **a single box**, with text grouped by type (#290):
  "R² < 0.6 for: • 12 reconvolutions • 1 fit", then a pointer to the
  Results panel. The unit of count is the **series** (a fit of 5
  columns of which 2 are faulty counts 2); the types follow the fixed order of
  `OperationKind::ALL`, their names come from the glossary (keys
  `low-r2-count-*`, plurals fr+en). No series names and no scrolling list.
- **Status bar badge** (#290): "⚠ 13 low R²", on the right of the
  bar, in R² red (`theme::r_squared_color`), with a tooltip. It is a
  **state** recomputed at each snapshot (`AppState::low_r2_groups`,
  `low_r2_total`), not an event: it stays after the box is closed,
  even when the box has opened, it exists only as long as there remain
  R² < 0.6 (a recalculation or undo that raises them, a deleted calculation: it
  disappears). It counts **the whole project**, including calculations whose setting
  `low_r2_dialog` is off (the setting turns off the box, not
  the information), and the cases where an alert was already waiting. "Show" calculations
  do not keep their values: they are not counted there (the box tells them at apply
  only). A click opens the **same grouped list** in a read-only window
  (`Action::OpenLowR2List`/`CloseLowR2List`, state `low_r2_list_open` separate from
  `alert`: a save alert is never overwritten); the window
  closes itself when nothing is low anymore.
- **Two switches, ORed**: the application preference (Preferences,
  General page, `Action::SetAnalysisLowR2Dialog`) and the calculation setting
  (`Calculation::low_r2_dialog`, Output section). The colors stay.

History: #288, #290 (badge and grouped list).

## Decimal comma

- "," and "." accepted in a field that holds **a single number** and
  in Numeric cells (S-N4, S-N5 → S-R6). On validation, the comma becomes
  a dot: the interface always displays the dot. Accepted effect: in English,
  "1,000" is read as 1. **Discreet notice** (#311): when a **cell input**
  (Numeric or Mixed column) has exactly the form `\d{1,3},\d{3}` ("1,234"),
  the status bar says how it was read ("1,234" was read as 1.234
  (decimal comma)). The notice follows the successful write; it does not apply to
  pasting nor to a Date/Time column, and does not change the rule
  (`number_format::looks_like_thousands_separator`).
- Formulas and value lists: the dot is required (the comma separates there).
- Mixed cell: follows the Numeric rule (decided) — "1,5" becomes the number
  1.5. A single reading, `stokesforge_core::number_format::parse_input`,
  shared by the number building block, the grid and `Text → Numeric`; the comma
  is accepted if there is **one** and no dot, otherwise refused.
- **Graph separator** (S-N4 → S-R9): **per-graph** setting (Graph
  Properties › Axes › Display, `PlotSpec::decimal_comma`, undoable by
  `EditPlotSpec`), independent of the language, for axes,
  ticks and exports (the legend writes no number today). A new
  graph takes the dot; the default in Preferences remains to be delivered with
  #148.

History: #223.

## Number display

- **A single formatter**, in `stokesforge-core` (S-D1, S-D2 → S-R3):
  - results and ticks: 3 significant digits, `e` notation outside
    [1e−4, 1e6);
  - grid and fields: 6 significant digits;
  - while editing: the full value; nothing is written if the text has not
    changed;
  - with uncertainty: uncertainty to 2 significant digits, value to the
    same decimal (`12.346 ± 0.052`); without uncertainty: 3 digits;
  - exact value of a result in a hover tooltip; **Ctrl+C always copies
    the exact value**, with a dot.
- **Where**: `format_results` (3 digits: Results panel, preview tooltip),
  `format_grid` (6: grid — `AppState::cell_display` —, fields at rest),
  `format_exact` (editing, copy, input comparison: `cell_text`, active
  numeric field), `format_with_uncertainty`, `format_tick` (ticks,
  `stokesforge-render::scale`, analysis preview). The digits before the
  decimal point are never rounded. A newly displayed number goes through this
  module, never through a local `format!`.
- **Scientific notation** (S-D3): typographic exponent on the figure
  (1×10⁻⁵), `e` notation in the grid and fields. Ticks stay in
  `e` notation (they do not go through Typst): the typographic exponent of the
  ticks is not delivered.
- **Pasting data rows**: an **absent** cell (row shorter than the target column) keeps the existing value and never lengthens the column; a **present and empty** cell (`3<tab>`) is a missing value. If the copy has more columns than the sheet has from the anchor, the extra columns are dropped (none is added) and the status bar says so (`status-paste-columns-dropped`). For a paste of **header fields** anchored at the bottom, the rows that go beyond Formula are dropped and counted the same way (`status-paste-fields-dropped`); if columns and fields overflow together, a single message gives both (`status-paste-columns-fields-dropped`). Ctrl+Shift+V of whole columns rewrites the whole header at its original rank (intended matrix).
- **Dates** (S-D4): ISO 8601 (`2026-10-10 14:05`); input forms (#218, in place): ISO `2026-10-10 14:30` and `JJ/MM/AAAA [HH:MM[:SS]]` (`10/10/2026 14:30`), **day first**, never month first; a bare number stays a POSIX timestamp in **seconds** ("2026" becomes 1970-01-01 00:33:46, not the year 2026: decided, #311); anything else promotes the column to Mixed, like any non-numeric text. Single reader and formatter: `stokesforge_core::datetime` (grid, paste, Mixed → Date/Time conversion); `stokesforge-import` keeps a copy, matched by a test of `stokesforge-importer`.
- **Paths** (S-D5): file name alone in messages, full path in a
  tooltip.

History: #222, #218 (dates).

## Text fields and in-place editing

- **Application** (S-T1 → S-R11): on Enter or when leaving the field. Exception:
  previews (analysis, import) recompute while typing, text and numbers,
  after a 0.3 s pause. Nothing is created before the dialog is validated.
- **In-place editor, a single contract** (S-T4): all the text selected on
  opening; Enter and a click elsewhere validate, Esc cancels, Tab validates and
  moves to the next.
- **Rename in the Project Manager** (S-K4): F2, or a slow click on an already
  selected name (0.5 s without a second click; a double-click never opens the
  field).

How it is held:

- **`app/inplace_edit.rs`** (`begin` / `show` / `finish`, `resolve`) reads the
  focus and the keys of a field placed by the caller and returns its end
  (`EditEnd`: Enter, Tab, elsewhere, Esc); it consumes Enter, Tab and Esc
  (never reread further down). `resolve` is the pure decision: refused input →
  the field stays open under the red frame of `field_state` (Enter, Tab) or
  the old value comes back (click elsewhere); unchanged text → nothing is written.
- Callers: cells and headers of the grid (`FocusLoss::Ends`, opening
  by typing = `KeepCursor`), sheet tabs (empty or taken name refused; Tab
  opens the neighboring tab), Project Manager (empty or taken name refused; Enter,
  Tab and click elsewhere move to the next row of the selection), series
  group of the Object Manager, titles and free texts of the graph
  (`FocusLoss::Ignored`: their settings bar takes the focus without ending the
  input, the "elsewhere" is still decided by the editor).
- Previews: the analysis (`analysis_preview::PREVIEW_PAUSE`) and the import
  (`ImportState::debounce_preview`, pause only while typing in a text
  field; list, checkbox and button are immediate).

History: #224.

## Messages and help

- **Three help levels** (S-M1): plain text (2 lines at most) for a side
  effect, a scope, a gesture on a preview; "?" icon (`hover_help`) for
  the reminder of a field; bare tooltip only for buttons, icons, tabs and
  menus. **Bare tooltip on a form field: forbidden** (S-M2).
- **Button tooltip** (S-M3): icon-only, name, effect and shortcut; text
  button, effect and shortcut without repeating the label.
- **Example text of an empty field** (S-T5): the value taken if empty ("Auto",
  "Group 2") or a syntax example ("a*exp(-x/b)"), never an
  instruction. Formula field (S-T6): the "?" icon alone. The `F(x)=` cell of
  the grid has no empty-field text: its "?" (`app/column_formula_help.rs`,
  shown only while the cell is edited, own preference "Show F(x)= help", #404)
  lists the callable functions by family; the icon only senses the hover, so it
  never takes the focus whose loss ends the edit.
- **Status bar** (#230): normal information, warning in the warning
  tint, error in red with an icon (S-M4). An information message fades
  after ≈ 8 s; warning and error stay until the next action
  (S-M5). A click shows the last 50 messages, timestamped (S-M6). Neither
  "Revision N" nor "Reopen Last Import…" (which stays in File) (S-M11 → S-R15).
- **Window title**: project name, "•" as long as it is not
  saved: "essai.stf • — StokesForge".
- **A single channel per piece of information** (S-M9 → S-R13), two written exceptions: a
  script error shows "Script failed. See the console" in the status
  bar (S-M10: the exception is flagged as an error); the reason of a disabled
  primary button is in the footer of the dialog **and** in its tooltip (S-F10).
- **Error in a modal window** (S-M7, in place #228): never in the status
  bar; message area of the footer, to the left of the buttons, common to all
  dialogs (`AppState::dialog_error`, `dialogs::sync_dialogs`).
- **Serious failure outside a window** (S-M8, in place #229; failed save,
  unreadable project): blocking dialog with a message and a possible action
  (`AppState::alert`, `modal_alert`: Close, and Save As… or Open Another…).
  The failure of a graph export stays in the status bar.

History: #225, #230 (status bar), #228 (S-M7), #229 (S-M8).

## Windows and dialogs

- **Application of settings** (S-F1): visible object (graph, series, object,
  text) → live change, OK/Cancel; Cancel restores, OK = one history
  entry. Action that creates (analysis, import) → draft, button named
  by its verb (Create, Import).
- **Read-only** (S-F2): a single Close button.
- **Button order** (S-F6): dialog = secondary buttons on the left, then Cancel,
  then the primary on the right; wizard = Cancel on the left, Back · Next ·
  Finish grouped on the right.
- **Esc levels** (S-F7 + S-R1): an open list closes first; otherwise
  the first Esc gives the field its value back, the next does the dialog
  Cancel.
- **Size and position** (S-F8) remembered per kind of dialog, in the
  preferences (outside `.stf`).
- **Focus** (S-F9): the first input field has it on opening.
- **Disabled primary button** (S-F10 → S-R14): reason in the footer and in a tooltip;
  a displayed error comes first, the reason comes back when it disappears.
- **Window nature** (S-F11): every modal window has a nature
  (`DialogNature`) that sets its footer, its default size, its initial focus and
  its default button. The size comes from a **field** of the variant, whose
  default value is a class: **Small** 480 x 170 (a question, a
  message), **Medium** 480 x 420 (settings, a form), **Large**
  640 x 500 (a list and its editor). An **interactive** window (settings,
  live settings, draft, wizard) takes a class; outside the classes, a **free size**
  justified in the code (`// FREE-SIZE(#300): <justification>`). A window
  **without rich interaction** (choice, read-only, confirmation,
  save, alert) may take the size of its content **without
  justification**: it has no form to arrange, and a badly
  chosen size costs little there. Initial focus: first field (settings, draft,
  choice, wizard, S-F9; settings **applied live** with Close alone, like
  Preferences: first field too, because they have fields), default button (confirmation: Cancel;
  save: Save; alert: Close), nothing (read-only).

How it is held (`app/dialogs.rs`, all through the same `modal_frame` body):

- **`modal_dialog_with`**: `[secondaires] [message] [Cancel] [principal]`;
  **`modal_wizard`**: `[Cancel] [message] [Back · Next] [Finish]` (the import
  wizard); **`modal_live_settings`**: `[message] [Close]` with focus on the first field
  (Preferences); **`modal_read_only`**: `[message] [Close]` (metadata of a
  column). A gesture of a read-only window ("Use as x'") stays a
  button of its content.
- **Dimmed text (`.weak()`)**: reserved for what is secondary — technical
  detail under a sentence (the system error of a failed save), reason,
  help, empty state. Never the **main content** of a dialog or of a
  read-only window: what the user must read (the low R² counters,
  a status message to remember) is in normal text. The grey of egui has
  low contrast on a light background; when in doubt, normal text.
- **Reason for disabling**: `DialogFooter::gated` (#226, S-F11) carries the tooltip
  **and** the reason of the primary button that can be disabled
  (`DialogFooter::always` for one that never is); the reason is shown
  when it is disabled (keys `disabled-*`: Finish without a
  file, Create of an incomplete analysis — the reason is then the first
  gap of the form); written in the footer **and** in the tooltip of the disabled
  button; `accept_hint` stays the tooltip of the active button.
- **Error** (S-M7): `AppState::fail` writes it to `dialog_error` as long as a
  content dialog is open (import, properties, analysis), never to the
  status bar; `notice` clears it, it does not outlive its dialog.
  `sync_dialogs` (once per frame, `show_edit_dialogs`) publishes it for the
  footer.
- **Esc** (S-F7): the one that leaves a text field of the dialog does not cancel it
  (the focus of the previous frame is authoritative: egui gives it back from the start of
  the pass); the "number" building block consumes its own.
- **Size and position** (S-F8): on closing, `sync_dialogs` returns
  `Action::RememberDialogGeometry` (kind = dialog identifier),
  `preferences.json` › `dialogs`, 32 kinds at most; restored on opening,
  brought back into the screen.
- **Focus** (S-F9): an invisible anchor at the head of the content takes the focus on
  opening, then the focus advances by one widget per pass up to the first
  text field, all the passes in the same frame (`request_discard`, nothing
  is painted between two steps); stop (focus given back) at the footer, outside the dialog, after 32 widgets
  or at the first gesture.

The properties window of an object (rectangle, arrow…) also applies its
settings live (S-F1): `Action::BeginShapeEdit` opens the history macro
at the first change, each setting is a `SetShape`, OK closes the macro
(`EndShapeEdit { keep: true }`, one entry), Cancel aborts it (`keep: false`).

How it is held (S-F11, `app/dialogs.rs`): `DialogNature` is an
enumeration with an exhaustive `match` without `_` (footer, default class, initial
focus, default button); the `modal_*` building blocks take the size field
of their nature (`InteractiveSize`: default, class, free;
`PlainSize`: default, class, content), `modal_dialog_with` the nature
(settings, draft or choice). Guards (`stokesforge-app/tests/guards.rs`):

- **`no_free_size_without_justification`**: `InteractiveSize::Free(` in production code
  requires, in the 12 lines that precede it, the exact marker
  `// FREE-SIZE(#300): <justification>` with a justification of at least 15 characters. Witness:
  `the_sizes_guard_sees_a_bare_free_size` (bare, empty, too short,
  other number, no number, too far; justified, comment, string and test code
  pass). Free sizes today: the analysis, the import
  wizard, the `preferences_proto` bench.
- **Analysis window, two columns** (#384): the settings column is `FORM_WIDTH`
  wide (400 pt, sized for Peak Decomposition, the widest form) and the preview
  takes the width that **really remains** after the column, never a width
  computed as if the column had exactly `FORM_WIDTH`. Witness:
  `analysis_dialog::tests::the_preview_stays_inside_the_window_and_the_widest_form_fits`
  (preview inside the window for five analyses; "Keep in Results" on one line).
- **`the_window_catalog_and_the_code_agree`**: every literal
  `"stokesforge_…"` passed as an argument of a `modal_*` function (before its
  closures) appears in backticks in `docs/windows.md`, and every
  identifier of the catalog has its call. The identifiers **built** before
  the call (`BUILT_IDENTIFIERS`: graph properties, series properties,
  analysis) are searched as literals. Witness:
  `the_catalog_guard_sees_a_deviation`. **Limits**: it does not read
  sizes, types or pages; a window whose identifier is derived
  (`id.with(…)`, the properties of an object) is not seen.

History: #228, #226 (reason for disabling), #300 (S-F11: window natures, size classes, catalog held by a guard).

### Confirmations

- **Only for what Ctrl+Z does not recover** (S-F3): unsaved project
  (New, Open, Quit, closing the window), overwriting a
  file, deleting an import template. **An undoable deletion asks
  nothing.**
- **Form** (S-F4): modal dialog, Cancel + verb (Delete, Discard Changes).
  Quit and Open: Discard Changes · Cancel · Save. Never "Keep" alone.
- **Keyboard and look** (S-F5 → S-R4, S-R5): Enter and Esc = Cancel, default button
  framed on Cancel; Delete in normal style, red on hover **and** on
  keyboard focus. Written exception: in "Discard Changes · Cancel ·
  Save", **Enter = Save**, Esc = Cancel.

How it is held:

- **Building blocks** (`app/dialogs.rs`, same `modal_frame` body):
  `modal_confirm` — `[message] [Cancel] [verbe]`, Cancel framed (default
  button), Enter and Esc = Cancel, the verb only through its button;
  `modal_save_changes` — `[Discard Changes] [message] [Cancel] [Save]`,
  Enter = Save; `modal_alert` — `[action] [message] [Close]` (S-M8). The
  destructive style is `theme::destructive_button_visuals` (red of the
  *hovered* and *active* styles; egui paints a focused widget as *active*).
- **Classification** (`app/confirmations.rs`): confirmations = unsaved
  project (New Project, Open…, Quit, closing the window:
  `Action::NewProject`/`OpenProject`/`Quit` → `AppState::unsaved_pending`,
  answer `Action::ConfirmUnsaved`), export whose added `.svg` extension
  would target an existing file that the native picker did not see
  (`Action::ConfirmOverwrite`; the native picker itself asks for the
  typed name), saving over the source file of a read that
  dropped data (`Action::ConfirmDiscardedSave`, #309), deletion of an import template (modal painted in
  the wizard, which reads Enter and Esc before it). **Choices**, not
  confirmations: the X axis of a line drop, the deletion of a calculation
  with or without its columns, the export of stale series — they keep
  `modal_dialog_with`. Deleting a sheet or objects from the Project Manager
  asks nothing anymore (`Action::DeleteSheet`, `Action::DeleteObjects`); the
  status bar message names the graphs that used their data.
- **Save**: saves, then continues the gesture (`after_save`); without a
  file, the window asks where to write first; a picker closed without
  a choice or a failed save (S-M8) abandons the gesture.
- **Closing**: `StokesForgeApp::logic` holds back every closing of the
  window (`CancelClose`) and turns it into `Action::Quit`; the window closes
  when `AppState::take_quit_ready` releases it (the verification
  auto-close asks nothing).

History: #229.

## Disabled

- **Every disabled element carries its reason**, which says what to do ("Select a
  series first") (S-G1).
- **Form** (S-G2): `.disabled` tooltip of the element itself, never mixed with the
  normal tooltip, never put on a group.
- **Mechanics**: the reasons are the `disabled-*` keys (or the `.disabled` of the action when it has one); `ToolbarButton` and the primary button of `modal_dialog_with` (`DialogFooter::accept_disabled`) set their own. Guard: `no_disabled_element_without_its_reason`.
- **Undo/Redo** (S-G3): disabled, "Nothing to undo"; enabled, "Undo Rename"
  (history label).
- **Field that depends on a checkbox** (S-G4): disabled, value visible and kept;
  hidden only if the whole section loses its meaning (From / To of an axis: see Auto).

History: #226.

## Choices, layout

- **Radios** for 2–3 options, drop-down list from 4 (S-C1); horizontal if
  each label is at most 3 words, vertical otherwise (S-C2).
- **Exclusive choice in a menu**: check mark alone, without radio (S-C3, see
  `menus.md`).
- **Toggle** = checkbox, label on the right; the "Custom" toggle of the
  color picker becomes a checkbox (S-C5).
- **Two-column grid form** (S-L1): labels on the left, fields
  aligned on the same vertical, checkboxes in the field column.
- **Four field widths** named in `theme.rs` (S-L2): a few
  digits (~38 px, #237: positive integer of at most three digits, without a
  list — the "number" building block takes it alone, `Quantity::is_compact`),
  short (~80 px), medium (~160 px), full.
- **A section heading between two blocks**, never a rule line alone (S-L3).
- **Two spacing constants** in `theme.rs`, no numeric `add_space`
  elsewhere (S-L4).
- **No collapsible section** in a form; too long → tabs (S-L5).
  A single exception, the **"Advanced"** block (S-L8, #344, `Form::collapsible`):
  closed by default, at the bottom of its section, for settings that are all Auto by
  default (length, thickness and color of the tick marks; **Figure size** of
  Graph Properties › General, #405); its arrow
  and its title form a single clickable area, without dead space, in the
  label column (like "Tick direction"), the title not being
  selectable; its fields stay aligned on the
  grid; the open/closed state is kept only for the session.
- **Tab orientation** (S-L9, #393): a window with a single level of tabs puts
  its strip on top (`TabStripSide::Top`). A window with two levels puts the
  **primary** tabs horizontally on top and the **secondary** tabs vertically at
  the left of the content (`TabStripSide::Left`), the content on their right.
  The secondary strip does not scroll away: only the page content scrolls
  (`theme::page_scroll_area` inside the column at the right of the strip).
  Examples: Preferences (rubrics on top, Display sub-tabs at the left), Graph
  Properties › Axes (Bottom, Left, Right, Top at the left, under the main tabs).
- **Three text levels** (S-L6, #170): `theme::section_heading` (section
  heading, bold), `theme::table_header` (column header of a table, bold
  in small size), `theme::secondary_text` (dimmed, small size). Guard
  `no_small_bold_outside_the_theme`.
- **A single row height** (S-L7, #170): `list_row::LIST_ROW_HEIGHT`
  (24 pt) for `list_row` and for the composed rows of the Object Manager, the
  Project Manager (folder included) and the series list.
- **Tab follows the reading order** (S-K6): top-bottom, left-right.

How it is held:

- **`app/form.rs`**: `form(ui, sel, |rows| …)` opens the grid; `Form::row`
  (label + field, the "?" icon after the field), `Form::field` (field column
  without a label), `Form::checkbox`, `Form::color`, `Form::note` (dimmed
  plain text), `Form::section` (heading across the full width), `Form::enabled`
  (disabled rows, S-G4), `Form::ui` (table or list across the full width).
  The label column takes the width of the longest, measured on the previous
  frame; a frame in which it changes is redone (`request_discard`).
  No `egui::Grid`: no cell spanning two columns for a heading, and
  `add_space` is forbidden there. S-K6 holds by construction (rows painted from
  top to bottom, left to right).
- **`choice` / `choice_by`** (`ChoiceOption`, optional `hint` reminder) paints
  a **fixed** exclusive choice according to `choice_layout` (S-C1, S-C2, tested pure). A
  data list (columns, graphs, saved templates, recent
  files, layers) stays a drop-down list whatever its number. A guard
  "list of fewer than 4 options" was studied and not set: a data `ComboBox`
  cannot be told apart from a fixed choice by reading the code.
- **`menu_sections::menu_choice`**: check mark in the margin before the current
  option (Type ▸, Role ▸, Language ▸, SVG Text ▸, PNG Resolution ▸). The `Move ▸` of
  columns (#319) is a submenu of actions, disabled with their reason, not an
  exclusive choice: no check mark.
- **`theme.rs`**: `SPACE_ITEM`, `SPACE_SECTION`, `FIELD_WIDTH_SHORT`
  (also `number_field::FIELD_WIDTH`), `FIELD_WIDTH_MEDIUM`, `FieldWidth`.
  Guard `no_literal_add_space_outside_the_theme` (`tests/guards.rs`), without
  exemption; a computed gap passes.
- Migrated: graph properties (all pages, series editor shared
  with the series window), analysis form, import wizard
  (files, format, header, metadata, destination; the rules between
  blocks became headings). The series window (group, scope,
  color gradient — `curve_settings.rs`) is migrated, as are the tools of
  the scope of the Results panel (#241;
  its series × value table and the "from / to" lists stay
  data tables/lists). Remaining to migrate: the future preferences (#148).

History: #227, #237 (width "a few digits"), #241 (Results panel).

## Keyboard and mouse

- **Hand-painted targets and accessibility** (#170): a
  layer badge announces its number and its selection; a window handle announces
  the resize direction. The name comes from the catalog and the accessible
  state follows the same state as the drawing (`Response::widget_info`).
- **Single key registry** in `app/shortcuts.rs` (S-K1: F2, Del,
  Esc, Enter…); no `consume_key` elsewhere.
- **Del** deletes the selection right away, undoable, everywhere (S-K2):
  Project Manager, Object Manager, figure. The grid keeps its exception
  (empty first); sheet tabs have no Del (the menu is enough,
  `shortcuts.rs`).
- **Double-click** (S-K3 → S-R7): text or name (cell, tab, title) → in-place
  editing; object (series, axis) → Properties; empty background of a graph → view
  reset; Project Manager → activates or restores the window, **never
  closes**. On a **folder**, a click selects it and expands it if it is
  closed (never collapses it), the double-click collapses it (#170).
- **Help › Keyboard Shortcuts**, generated from `shortcuts.rs` (S-K5).
- **Ctrl+wheel = zoom**, everywhere (S-X1 → S-R1, S-R7).
- **Cursors** (S-X2): open then closed hand for dragging, resize
  arrow on a handle, finger on a clickable element that is
  not a button.
- **Single drop stroke** (S-X5, #170): hovering a workbook or the workspace with a file from the system, a graph with columns or rows, lights up the **border of the target window** with the same light blue stroke, `theme::drop_target_stroke` (never red, which is for errors; nothing is painted inside the content, #386). One window at a time: over a workbook only its border, over the workspace (destination "new workbook") only the application's outline; a maximized target gets the stroke around the area it fills. The stroke replaces the active outline. Tests: `file_drop::tests`, `graph_window::tests`, `window_detach::tests`, `window_stack::tests`, `theme::tests`.
- **Arrows and Enter in the managers** (S-K9, #170): the Project Manager and the Object Manager can be walked by keyboard when they have the focus (a click in them gives it). ↑ and ↓ select the neighboring **visible** row (the children of a collapsed folder or section are skipped), bounded at both ends; they open nothing and write nothing to the history. Enter opens: a window opens (like the double-click), a folder expands or collapses, a series or an object of the Object Manager opens its Properties. The row that carries the cursor has the focus outline of `selection.stroke` (`list_row::paint_focus_outline`). Shift+↑/↓ and Alt+↓ are consumed without effect; Ctrl+↑/↓ of the Object Manager keeps "move the series". The keys are consumed before the grid of a workbook, which reads the arrows without consuming them. Keys: `KEY_ARROW_UP`, `KEY_ARROW_DOWN`, `KEY_ENTER` of `app/shortcuts.rs`, help lines `help-select-neighbour` and `help-open-entry`.
- **No tool selector in a window** (S-K7, #299). The object first:
  one clicks or drags the object itself (a curve, a handle, a
  cell); the tools (Pointer, Text, Zoom, shapes) live in the Tools
  bar and nowhere else. It is a *guideline*: one may leave it, but
  **every exit is justified twice**, in the code and here.
- **Gestures are in a matrix** (S-K8, #299): [`gestures.md`](gestures.md)
  states what click, drag, double-click, wheel, Ctrl + wheel, Ctrl +
  drag, middle click and right-click do on each surface (graph, analysis preview, grid,
  trees, tabs, windows, fields). A new surface, or a new gesture
  on an existing surface, is filed there; a gesture that differs from the same action
  elsewhere is flagged against this page. The matrix is not copied here.
  Moving the view is the same gesture everywhere (#318, #270): Ctrl +
  drag or middle click + drag, on the graph as on the analysis
  preview; a plain drag never moves the view (it moves the object
  grabbed, or nothing), and started from an axis, the gesture moves only that scale.

How it is held (S-K7, S-K8):

- **`no_tool_picker_outside_the_tools_bar`**
  (`stokesforge-app/tests/guards.rs`): an exclusive choice (building block `choice` /
  `choice_by`, `selectable_label`, `selectable_value`, `Button::selectable`,
  `RadioButton::new`, `radio`, `radio_value`, `toggle_value`) whose
  arguments name a tool ("tool", excluding "toolbar" and "tooltip") is
  refused in every file except `toolbar.rs`, unless one of the **12 lines
  that precede** (or the line itself) carries the exact marker
  `// TOOL-IN-WINDOW(#299): <justification>`, a justification of at least 15
  characters. Witness: `the_tools_guard_sees_a_bare_tool_picker`
  (each form of choice, empty exception, too short, from another marker or
  too far). Limit: it relies on the word "tool" in the arguments; a
  tool selector without that word escapes it.
- **`no_gesture_outside_the_matrix`**: every production file of
  `stokesforge-app` or `stokesforge-render-egui` that reads a mouse wheel
  (`scroll_delta`, `zoom_delta`, `MouseWheel`) or a double-click
  (`double_clicked`) must be cited (path `….rs` in backticks)
  in `gestures.md`; a cited file that no longer exists also fails the
  guard. Witness: `the_gestures_guard_sees_a_gesture_outside_the_matrix`.
  **Real limits**: it works at the **file** level (a new
  wheel gesture in an already cited file passes, and it does not read what
  the gesture does); it sees only the **wheel and the double-click**, not
  click, drag, right-click or modifiers. It prevents a
  surface from being born outside the page, not a gesture from being wrong there.

**Exception: analysis gestures** (S-K7). The analysis window keeps, in
its preview, a gesture selector: **Point** (a click places an anchor point,
a peak seed, a bound) and **Zone** (a drag draws a zone or a
range), as two **framed texts** with toggle (`toggle_texts`, `app/form.rs`:
rounded frame, active in solid accent fill, inactive in outline only),
marker `TOOL-IN-WINDOW(#299)` in `app/analysis_dialog.rs`. Justification:
these gestures need actions specific to the preview (adding a point, drawing
a zone, snapping to the curve); putting them in the Tools bar of the graph
would mix these gestures with those of the figure. The exception is as narrow
as possible: **only peak detection** needs both (position of a peak
by a click, search range by a drag). Every other analysis has no
gesture to choose: that of its operation (`AnalysisForm::natural_tool`,
`tool_choices`) is active from the start, and the row is not painted. Zoom
is no longer a tool of the preview (D2, D3 of #299, #270).

**Preview navigation** (D3 of #299, #270): as on the graph. Ctrl +
wheel zooms, anchored under the pointer (above an axis: that scale
alone); Ctrl + drag moves the view (started from an axis: that axis alone);
double-click reframes; Shift + wheel sets the snap average. The wheel
alone does nothing. The computation of zoom and pan is that of
`stokesforge-render` (`PanelTransform`); the detail per gesture is in
[`gestures.md`](gestures.md).

History: #231, #299 (S-K7, S-K8: tools guideline and gesture matrix).

## Long tasks and empty states

- Beyond ≈ 0.5 s: spinning indicator + word ("Importing…") at the place
  of the result; progress bar if the number of steps is known (S-P1).
- **Stop** button for any work of more than a few seconds (multiple
  import, script, heavy recalculation) (S-P2). Import by drag and drop follows the
  same path as File › Import (reading, template choice and analysis on the
  background thread, #266).
- **Empty state**: centered greyed-out sentence that says what is missing and the gesture to create
  it (S-P3): `empty_state`, everywhere (workbook without a column or without a
  sheet, graph without a series, Object Manager, Results, message history,
  script console).
- Thresholds: the indicator after 0.5 s (`busy::SHOW_AFTER`), Stop after 3 s
  (`busy::STOP_AFTER`). **Import Stop**: the thread stops after the current
  file, nothing is created (`AppState::stop_import`). **Script Stop**: the
  console keeps its own (#215). **Formula recalculation** (#239): a command waits 0.2 s
  (`RECALC_WAIT`) for the end of the recalculation it launches — a short recalculation stays
  synchronous and invisible — then hands control back; the status bar shows
  "Recalculating…" and Stop (`Action::StopRecalc`). Stop interrupts between
  two columns, then **undoes the command** like Ctrl+Z (redoable) and
  recalculates what changed. Every other gesture waits for the end of the recalculation
  (`docs/session.md`).

History: #232, #239 (formula recalculation), #266 (import by drag and drop), #215 (script Stop).

## Outside this page

- **Edit › Preferences… window** for application settings, the menus
  keep the frequent display toggles (S-X3): #148.
- **Action registry** (S-X4): one table per action (label, logo,
  shortcut, disabling condition and its reason), read by menus, toolbars
  and right-click; it will carry S-G1 and S-K1 (#110).

## A rule that looks wrong

A behavior that does not follow this page is flagged against it
(`stokesforge-bug-report`). A rule that does not fit is rediscussed with
the user, it is not worked around: changing it means changing this
page, the issue concerned and the skills in the same commit.

### Unavailable options of an exclusive choice

`ChoiceOption::disabled(Some(reason))` disables an option of the form and
shows its reason on hover. The same refusal applies to radios and
drop-down lists: a disabled option does not change the selected value.
The choice stays painted by the `choice` building block; do not rebuild ad hoc
radios for an unavailability.

History: #212.
