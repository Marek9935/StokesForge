# Évolutions à développer — retours du 2026-09-23

Neuf chantiers, tirés d'un vrac de seize retours trié le 2026-09-23. Les
défauts du même message vivent à côté, dans
[`retours-2026-09-23-bugs.md`](retours-2026-09-23-bugs.md).

Note de passation : elle rejoindra `docs/archive/` quand sa dernière case sera
cochée. Chaque chantier passe par le skill `plotyx-feature` — les quatre
surfaces d'une action (raccourci, menu, grisage, libellé) ne se tiennent en
accord que si on y pense à chaque fois.

## Décisions déjà arbitrées le 2026-09-23

À ne pas rouvrir sans raison nouvelle :

| Sujet | Décision |
|---|---|
| Tab dans les quatre lignes d'en-tête | **Étendre.** La circulation entre champs qu'on y perd est acceptée : les flèches, Entrée et Maj+Tab la remplacent. |
| Poignée du glisser-déposer | **Bande autour de la sélection.** Le conflit redouté par le commentaire du code n'existe pas : l'extension de sélection est un appui-déplacement-relâchement, la poignée ne s'arme **qu'après** le relâchement. Les deux gestes se succèdent dans le temps, ils ne se disputent pas l'espace. |
| Périmètre du redimensionnement | Le rognage est un bug (B4), le défilement horizontal une évolution (F8). Les deux se livrent ensemble : sans rognage, le glisseur n'a rien à faire défiler. |

---

## Lot A — l'ajout de colonne quitte le bandeau

Le bandeau du classeur disparaît entièrement ; ses deux gestes migrent vers le
menu contextuel, le clavier et le panneau d'icônes.

### F1 — Supprimer le bandeau du classeur

- [ ] Retirer les boutons « Ajouter une colonne » et « Importer… », ainsi que
      l'affichage « N lignes × M colonnes ».

**Deux écritures à traiter** : `book_contents` (`app.rs:3653`) **et**
`detached_book_contents` (`app.rs:5592`). Un commentaire du fichier dit déjà
« *Même correctif B5 que `book_contents`* » — la duplication est connue et
oblige à corriger deux fois.

Objectif de fond : **factoriser plutôt que dupliquer**. `docs/application.md`
explique ce qui doit rester séparé (les tampons de saisie transitoires, à cause
de la fermeture `'static` de `show_viewport_deferred`) — un bandeau qui ne fait
que pousser des `Action` n'y bute pas.

### F2 — Menu contextuel « Ajouter une colonne » en fin de grille

- [ ] Clic droit dans la zone vide à droite de la dernière colonne → entrée
      « Ajouter une colonne ».

Rappel de la règle maison : le menu contextuel de la grille est le **miroir
exact du clavier**, et la décision (quoi proposer, quoi griser, comment nommer)
vit dans `AppState::grid_menu` → `GridMenu`, jamais dans le code egui — c'est
ce qui la rend testable sans fenêtre dans `tests/shell.rs`.

### F3 — Tab prolonge la grille depuis tout l'en-tête

- [ ] Étendre la condition aux champs `Nom long`, `Unité` et `Commentaire`, en
      plus de `Formule` : `app.rs:4459-4484` (raccourci clavier de
      `grid_contents`), `app.rs:3395-3415` (`header_field_widget`), et le
      duplicata détaché `app.rs:5453-5484`.
- [ ] **Mettre à jour le commentaire `app.rs:4455-4467` et la section « Tab en
      dernière colonne » de `docs/application.md`** : ils décrivent la
      décision que ce chantier renverse, ils deviendraient faux.

⚠️ Ce n'est pas la réparation d'un oubli : le comportement actuel est un choix
documenté. Le renversement est assumé (voir les décisions ci-dessus).

### F4 — Sélection et suppression de colonne depuis l'en-tête

- [ ] **Vérifier d'abord à l'écran ce qui manque réellement.** La rangée des
      lettres sélectionne déjà la colonne entière et ouvre déjà
      `grid_edit_menu` (`app.rs:3095`, `app.rs:3145`). Reste à établir si un
      clic sur un champ d'en-tête (`header_field_widget`, `app.rs:3350`)
      sélectionne la colonne entière ou seulement ce champ.
- [ ] **Câbler la suppression de colonne — tout est à faire.**
      `Command::DeleteColumn` existe dans `plotyx-commands`, mais
      `rg DeleteColumn crates/plotyx-app/src/` ne renvoie **rien** : il faut
      une variante d'`Action`, son traitement dans `AppState::dispatch`,
      l'entrée de menu, et un test dans `tests/shell.rs`. Ce n'est pas une
      ligne d'interface.
- [ ] Menu contextuel identique quelle que soit la zone dès qu'une colonne
      entière est sélectionnée, en gardant celui de la rangée des lettres.

## Lot B — fenêtres et chrome

### F5 — Panneau d'icônes façon Origin

- [ ] Sous la barre de menus, un panneau de raccourcis illustrés : nouveau
      classeur, nouveau graphe, importer un fichier.

**Dépendance** : l'import quitte le bandeau du classeur avec F1. Livrer F5
**avant ou avec** F1, sinon l'import n'a plus aucun point d'entrée.

*À préciser avant de concevoir* : les actions à inclure au-delà des trois
citées ; le jeu d'icônes (en existe-t-il déjà dans `plotyx-app`, ou faut-il en
choisir ?) ; panneau permanent ou repliable.

### F6 — Contour de fenêtre, sans ombre

- [ ] Un trait gris fin, angles légèrement arrondis, sur la fenêtre détachée
      **et** la fenêtre principale ; supprimer l'effet d'ombre actuel.

À faire dans la même passe que B1/B3 (contour de surbrillance) : les deux
peignent le bord de la même fenêtre, les traiter séparément reviendrait à
repeindre deux fois la même zone.

### F7 — Largeur des colonnes × 4/3

- [ ] Augmenter la largeur par défaut d'un tiers.

Vérifier l'effet sur B4 : des colonnes plus larges débordent plus tôt.

## Lot C — grille et glisser-déposer

### F8 — Défilement horizontal du classeur

- [ ] Glisseur horizontal une fois les colonnes rognées (B4).
- [ ] Maj+molette pilote ce glisseur.

**Préalable : B4.** Sans rognage, le glisseur n'a rien à faire défiler.

### F9 — Poignée de glisser autour de la sélection

- [ ] Déplacer la zone de prise (`COLUMN_GRIP_WIDTH`, `app.rs:514` ; zone
      actuelle `app.rs:4304-4340`) du bord de la grille vers le pourtour de la
      sélection.
- [ ] **La poignée ne s'arme qu'après le relâchement du clic** — c'est ce qui
      la sépare du glisser d'extension de sélection.
- [ ] Mettre à jour le commentaire `app.rs:4308-4321`, qui justifie
      aujourd'hui la position au bord par un conflit que cette règle dissout.

---

## Ordre suggéré

1. **F5** (panneau d'icônes) — préalable à F1, sinon l'import disparaît.
2. **F1 + F2 + F3** — un seul chantier cohérent : le bandeau s'en va, le menu
   contextuel et Tab le remplacent.
3. **F4** — le plus lourd du lot A, à cause du câblage de `DeleteColumn`.
4. **F6** avec B1/B3, **F7** quand on veut.
5. **F8** après B4.
6. **F9** — indépendant.
