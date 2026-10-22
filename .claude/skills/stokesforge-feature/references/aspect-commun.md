# Fiche : défilement, onglets, listes, gras, couleurs d'état, formulaires

Quand la lire : on peint une barre de défilement, un jeu d'onglets, une liste ou un arbre sélectionnable, un titre de section, un formulaire, un choix exclusif, un espacement.

## Où lire la règle

- Barres de défilement : `docs/application/theme-i18n-systeme.md` § Thème (paragraphe « Barres de défilement : un seul standard ») ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes (`theme::scroll_style`).
- Quatre rôles (onglets, liste, choix exclusif, bascule d'outil) : `docs/application/fenetres-et-panneaux.md` § Onglets, listes et sélection : quatre aspects, quatre rôles (#169).
- Gras, titres de section, couleurs d'état, formulaire en grille, `choice`, écarts : `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `aspect commun (#169)` ; `docs/standard-interface.md` § Choix, disposition.
- Principe général : `docs/principes-ui.md` § 1. Une brique, un geste.
- Gardes : `docs/tests.md` § Les garde-fous à ne pas casser.

## À vérifier

- [ ] aucun `ScrollStyle` local ; place d'une barre lue par `theme::scroll_bar_allocated_width`
- [ ] onglets par `tab_strip`, jamais une rangée de `selectable_label`
- [ ] liste ou arbre par `list_row` / `ListRowBackground` ; ligne composée à la hauteur `LIST_ROW_HEIGHT` (`with_list_row_height` dans son `ui.horizontal`)
- [ ] gras par `theme::bold` ; titre par `theme::section_title`, sans `separator()` juste avant ; en-tête de tableau `theme::table_header`, texte secondaire `theme::secondary_text`, jamais `bold(…).small()` (#170)
- [ ] un séparateur décoratif est une icône atténuée, pas un glyphe (`›`)
- [ ] aucune couleur en dur hors `theme.rs` et `color_picker.rs`
- [ ] formulaire par `app/form.rs`, choix par `choice`, écarts par `theme::SPACE_*`
- [ ] un test egui qui dépend de l'aspect appelle `theme::install`
