# Sheet: windows and dialogs

Read when creating or changing a content window, dialog, wizard, confirmation or fixed panel.

## Where the rule lives

- Catalogue, natures and size classes (S-F11), types, model window per type, anatomy: `docs/fenetres.md` § Les natures et les classes de taille ; `docs/fenetres.md` § Les types ; `docs/fenetres.md` § Anatomie d'une fenêtre neuve ; `docs/fenetres.md` § Fenêtre modèle par type.
- What the building block does alone (size, position, Escape, focus): `docs/fenetres.md` § Taille, position, Échap, focus : ce que la brique fait seule.
- Dialogs, footer, buttons, errors, confirmations: `docs/standard-interface.md` § Fenêtres et dialogues ; `docs/standard-interface.md` § Confirmations.
- Building blocks (`show_window_in_layer`, `modal_dialog_with`, `modal_wizard`, `modal_live_settings`, `modal_read_only`, `modal_confirm`…): `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `toute fenêtre passe par la brique commune`.
- Fixed panels: `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application/fenetres-et-panneaux.md` § Panneaux rétractables et détachés (#100).
- Failure of a dialog gesture: `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue).
- History macro of an editing dialog: `docs/commandes-et-historique.md` § Macros.
- Screenshot to compare: `docs/fenetres.md` § Produire une capture.

## Check

- [ ] no raw `egui::Window` (`aucune_fenetre_n_est_une_egui_window_brute`), even when a neighbour does it: existing code that deviates is not a model
- [ ] a reference twin named and compared by screenshot; mockup validated by the user before code (new window)
- [ ] editing dialog: gesture in a macro (`begin_macro`), OK closes it, Cancel aborts it
- [ ] greyed main button: `DialogFooter::gated` (tooltip and reason), otherwise `DialogFooter::always`
- [ ] nature chosen (`DialogNature`, S-F11) and a size class; free size only with `// TAILLE-LIBRE(#300): <justification>`
- [ ] row added to `docs/fenetres.md` with the `stokesforge_…` id (`le_catalogue_des_fenetres_et_le_code_concordent`)
- [ ] an undoable deletion asks nothing; otherwise `modal_confirm`
