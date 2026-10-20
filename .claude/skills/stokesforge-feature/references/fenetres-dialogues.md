# Fiche : fenêtres et dialogues

Quand la lire : on crée ou modifie une fenêtre de contenu, un dialogue, un assistant, une confirmation, un panneau fixe.

## Où lire la règle

- Catalogue, types, fenêtre modèle par type, anatomie d'une fenêtre neuve : `docs/fenetres.md` § Les types ; `docs/fenetres.md` § Anatomie d'une fenêtre neuve ; `docs/fenetres.md` § Fenêtre modèle par type.
- Ce que la brique fait seule (taille, position, Échap, focus) : `docs/fenetres.md` § Taille, position, Échap, focus : ce que la brique fait seule.
- Dialogues, pied, boutons, erreurs, confirmations : `docs/standard-interface.md` § Fenêtres et dialogues ; `docs/standard-interface.md` § Confirmations.
- Briques (`show_window_in_layer`, `modal_dialog_with`, `modal_wizard`, `modal_read_only`, `modal_confirm`...) : `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `toute fenêtre passe par la brique commune`.
- Panneaux fixes : `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application/fenetres-et-panneaux.md` § Panneaux rétractables et détachés (#100).
- Échec d'un geste de dialogue : `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue).
- Macro d'historique d'un dialogue d'édition : `docs/commandes-et-historique.md`.

## À vérifier

- [ ] aucune `egui::Window` brute (`aucune_fenetre_n_est_une_egui_window_brute`)
- [ ] un jumeau de référence est nommé et comparé par capture (`docs/fenetres.md` § Produire une capture)
- [ ] maquette validée par l'utilisateur avant le code (fenêtre neuve)
- [ ] dialogue d'édition : geste dans une macro (`begin_macro`), Valider la ferme, Annuler l'avorte
- [ ] bouton principal grisé : sa raison est `accept_disabled`
- [ ] pas de voile sur le reste de l'application ; premier champ = vrai `TextEdit`
- [ ] suppression annulable sans confirmation ; sinon `modal_confirm`
