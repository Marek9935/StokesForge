# Fiche : place dans les menus

Quand la lire : pour toute fonctionnalité, avant de coder (décider où l'utilisateur la trouvera), et chaque fois qu'on ajoute une entrée de menu ou de clic droit.

## Où lire la règle

- Grammaire (sections, sous-menus, absent / grisé, verbes nus, séparateurs) : `docs/menus.md` § Règles ; `docs/menus.md` § Ordre canonique des sections ; `docs/menus.md` § Ajouter une entrée.
- Contenu et enveloppe : `docs/menus.md` § Le principe : contenu et enveloppe.
- Résumé opérationnel et peintures partagées : `.claude/conventions.md` § Object Manager, panneaux fixes, menus (point `Menus (#114, #152)`).
- Barre de menus (File, Edit, Workbook, Graph, View, Window) et menus thématiques : `docs/application/menus-et-barres.md` § Menus thématiques (#114).
- Menu contextuel de la grille (`GridMenu`) : `docs/application/grille.md` § Menus contextuels de la grille ; écarts délibérés entre menu et clavier : même section et `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue).
- Rien d'atteignable seulement au clic droit : `docs/menus.md` § Règles.

## À vérifier

- [ ] la place est décidée et dite dans le compte rendu (barre de menus ET menus contextuels)
- [ ] entrée rangée dans une section, séparateurs par `MenuSections::section` seulement
- [ ] entrée absente (sans objet) ou grisée avec `.disabled`
- [ ] test dans le `mod tests` de `app/menus.rs` (grisage lu par l'arbre d'accessibilité)
- [ ] bascules : deux clics dans le même menu ; action : fermeture de toute la chaîne
- [ ] menu de la grille : décision dans `AppState::grid_menu`, testée dans `stokesforge-app-state/tests/shell.rs`
- [ ] un clic droit hors sélection la remplace, dedans la laisse intacte
