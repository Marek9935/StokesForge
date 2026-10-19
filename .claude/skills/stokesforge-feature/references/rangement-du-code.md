# Fiche : où ranger du code d'interface

Quand la lire : on ajoute ou déplace du code dans `stokesforge-app` (surface neuve, menu, fenêtre, brique, test, garde), ou on redécoupe une fonction géante.

## Où lire la règle

- Table module -> rôle : `docs/application.md` § Découpage du code.
- Règles de rangement (un module par surface, imports explicites, `app.rs` = orchestration, les tests suivent leur code) : `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, premier point `découpage d'app.rs`.
- Redécoupe sur place, témoins : `docs/tests.md` § Redécoupe des fonctions d'interface (#124) ; `CLAUDE.md`, « Délégation et choix du modèle ».
- Gardes qui lisent le code (lecteur unique) : `docs/tests.md` § Les garde-fous à ne pas casser.

## À vérifier

- [ ] dessin et événements dans le module de la surface, rien de nouveau dans `app.rs` hors orchestration
- [ ] surface neuve : module à imports explicites, doc de tête `//!`, ligne ajoutée à la table de `docs/application.md`
- [ ] aucun `use ...::*` hors tests ; visibilité `pub(super)` au minimum
- [ ] la restructuration est un commit à part, sans changer d'ID egui ni l'ordre des événements
- [ ] un test déplacé avec son code ; une garde dont la fonction a été renommée est mise à jour, pas contournée
