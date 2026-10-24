# Fiche : comportement de l'interface (champs, messages, grisage, clavier)

Quand la lire : on pose un champ, un message, un dialogue, une confirmation, un grisage, un geste clavier ou souris, un nombre affiché, un travail long, un panneau vide.

## Où lire la règle

`docs/standard-interface.md` est la source unique ; commencer par `docs/standard-interface.md` § Index des règles, puis la section du geste :

- Champs numériques, bornes, pas, molette : `docs/standard-interface.md` § Champs numériques ; brique `number_field` et `quantities.rs`.
- Saisie refusée / ramenée, deux champs liés : `docs/standard-interface.md` § Saisie refusée ou ramenée ; brique `field_state`.
- Virgule décimale : `docs/standard-interface.md` § Virgule décimale.
- Nombre affiché : `docs/standard-interface.md` § Affichage des nombres (`number_format`).
- Texte, édition sur place (`inplace_edit`), formule Typst : `docs/standard-interface.md` § Champs texte et édition sur place.
- Rappel d'un champ (icône « ? », `hover_help`), texte atténué : `docs/standard-interface.md` § Messages et aide.
- Fenêtres, dialogues, confirmations : `docs/standard-interface.md` § Fenêtres et dialogues ; `docs/standard-interface.md` § Confirmations.
- Grisé et sa raison : `docs/standard-interface.md` § Grisé.
- Choix exclusif, formulaire, menu : `docs/standard-interface.md` § Choix, disposition.
- Clavier, souris, curseurs, double-clic : `docs/standard-interface.md` § Clavier et souris ; matrice des gestes : `docs/gestes.md` § Le modèle.
- Travail long, panneau vide : `docs/standard-interface.md` § Travaux longs et états vides (`work_in_progress`, `execute_interruptible`, `empty_state`).
- Briques en place / à venir et gardes : `docs/standard-interface.md` § Briques et gardes ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `standard de comportement (#217)`.
- Une règle qui paraît fausse : `docs/standard-interface.md` § Une règle qui paraît fausse.
- Vue d'ensemble : `docs/principes-ui.md`.

## À vérifier

- [ ] chaque champ, message ou dialogue relu contre la section correspondante
- [ ] brique « à venir » non appelée ; règle « à venir » appliquée avec les moyens existants
- [ ] brique ou garde créée : passée à « en place » dans la page, skills et table des gardes de `stokesforge-verification`, dans le même commit
- [ ] nombre saisi par `number_field`, affiché par `number_format`, lu par `parse_input`
- [ ] tout élément grisé dit pourquoi (`.on_disabled_hover_text`)
- [ ] aucun `.weak()` sur le contenu principal d'un dialogue
