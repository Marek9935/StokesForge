# Sheet: appearance — scrolling, tabs, lists, text levels, forms, colours, icons

Read when painting a scroll bar, tabs, a selectable list or tree, a section title, a form, an exclusive choice, a spacing, a colour field, or an icon.

## Where the rule lives

- Scroll bars: `docs/application/theme-i18n-systeme.md` § Thème ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes (`theme::scroll_style`).
- Four roles (tabs, list, exclusive choice, tool toggle): `docs/application/fenetres-et-panneaux.md` § Onglets, listes et sélection : quatre aspects, quatre rôles (#169).
- Bold, section titles, state colours, grid form, `choice`, spacing: `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `aspect commun (#169)` ; `docs/standard-interface.md` § Choix, disposition.
- Colour picker, usages, saved colours: `docs/application/graphe.md` § Propriétés des graphiques ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `color_picker::color_field`.
- Empty colour = automatic, automatic ink: `docs/rendu.md` § Couleurs du texte et des échelles : l'encre automatique.
- New colour field in the model (`#[serde(default)]`): `docs/persistance.md` § Le format ne s'écrit pas dans le modèle.
- Icons: `docs/icones.md` § Quand mettre une icône — et quand non ; `docs/icones.md` § Chaque action a son logo (règle posée avec #110) ; `docs/icones.md` § Couleur : neutre par défaut, couleur rare ; `docs/icones.md` § Ajouter une icône ; `docs/icones.md` § Échantillons de couleur (#89).
- General principle: `docs/principes-ui.md` § 1. Une brique, un geste.
- Guards: `docs/tests.md` § Les garde-fous à ne pas casser.

## Check

- [ ] no local `ScrollStyle`; a bar's room read by `theme::scroll_bar_allocated_width`
- [ ] tabs by `tab_strip`; lists and trees by `list_row` / `ListRowBackground`; composed rows at `LIST_ROW_HEIGHT` (`with_list_row_height`)
- [ ] keyboard-walked list (S-K9): `list_row::read_navigation` and `neighbour` on visible rows, focus by `paint_focus_outline`
- [ ] bold by `theme::bold`; title by `theme::section_title` (no `separator()` just before); `theme::table_header`, `theme::secondary_text`; never `bold(…).small()`
- [ ] no hard-coded colour outside `theme.rs` and `color_picker.rs`; form by `app/form.rs`, spacing by `theme::SPACE_*`
- [ ] colour by `color_picker::color_field` with its `ColorUsage` and `PickerEnv`, never a `#RRGGBB` text field
- [ ] icon by `icons::icon_image` / `icon_button` with a `tr!` tooltip, never a glyph or emoji; `IconRole::Neutral` unless object type, destructive action or state; the action's logo named in the report
- [ ] an egui test that depends on appearance calls `theme::install`
