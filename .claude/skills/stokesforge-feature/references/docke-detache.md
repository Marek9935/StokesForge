# Fiche : doublon docké / détaché

Quand la lire : avant de toucher à une surface d'interface (classeur, graphe, note, matrice, panneau fixe, raccourci global, état ajouté à `AppState`).

## Où lire la règle

- Règles de câblage de la vue détachée (état recopié, raccourcis globaux, IDs egui) : `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue).
- Fenêtres détachées et réveil du viewport : `.claude/conventions.md` § Graphes neufs et fenêtres détachées.
- Contrainte `'static` de `show_viewport_deferred` et ce qui en découle : `docs/application.md` § Détachement générique des onglets de contenu ; `docs/application.md` § Project Manager et fenêtres par contenu.
- Panneaux fixes (PM, OM, console) attachés / détachés : `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application.md` § Panneaux rétractables et détachés (#100).
- Copie d'objets entre projets, F2 et Ctrl+D : `.claude/conventions.md` § Copie d'objets et de figures.

## À vérifier

- [ ] `rg "fn <nom>|fn detached_<nom>" crates/stokesforge-app/src/ crates/stokesforge-app-state/src/` : un seul résultat, sinon factoriser ou modifier les deux (et le dire dans le compte rendu)
- [ ] un état d'`AppState` neuf existe aussi dans la vue détachée s'il doit y valoir
- [ ] un raccourci global essayé avec un graphe actif et un classeur détaché
- [ ] aucun IDs egui calculé sous `Context::data_mut`
