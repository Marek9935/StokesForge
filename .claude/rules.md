# Règles de chargement contextuel

Un seul domaine actif : le portage Rust (`crates/`). `legacy-python/` a été
supprimé de cette branche et rien n'y fait plus référence.

**Charger ce que la tâche demande, pas le workspace.** Chaque domaine ci-dessous
dit quoi ouvrir, dans quel ordre, et surtout **quoi ne pas ouvrir**.

L'ordre est toujours le même : la page `docs/` **d'abord** (elle route et donne
les invariants), le code ensuite. `docs/invariants.md` vaut pour tous les
domaines — 11 règles, une table ; le lire avant de toucher au rendu, à
l'historique ou à l'acteur.

## Routage

| Tâche | Lire d'abord | Charger | Ne pas ouvrir |
|---|---|---|---|
| Arbre, colonnes, `PlotSpec` | `docs/modele-de-donnees.md` | `stokesforge-core`, `stokesforge-id`, `stokesforge-plot` | `commands`, `session`, `persistence`, `app` |
| Undo/redo, macros | `docs/commandes-et-historique.md` | `stokesforge-commands` + `stokesforge-core` en lecture | `session`, `persistence`, `app` |
| Format `.stf`, `save`/`load` | `docs/persistance.md` | `stokesforge-persistence`, `stokesforge-core`, `stokesforge-plot` | `session`, `render*`, `app` |
| Acteur, concurrence, instantanés | `docs/session.md` | `stokesforge-session`, `stokesforge-core`, `stokesforge-commands`, `stokesforge-persistence` | `python`, `script`, `app` |
| Tracé, axes, export, formules | `docs/rendu.md` | `stokesforge-render` **d'abord**, puis le backend visé, + `stokesforge-plot` et `stokesforge-core::resolve` en lecture | `commands`, `persistence`, `python`, `script` |
| Pont PyO3, surface `pyst` | `docs/python-et-script.md` | `stokesforge-python`, `stokesforge-script` | `render*`, `persistence` |
| Fenêtre, raccourcis, grille | `docs/application.md` (§ Découpage du code : la table module → rôle) | `stokesforge-app-state/src/state.rs` (l'état sans egui, #263) pour la décision, puis **le seul module** de la surface dans `src/app/` (grille `grid.rs`, classeur `workbook.rs`, graphe `graph_window.rs`, menus `menus.rs`, fenêtres `window_*.rs`…) ; `app.rs` seulement pour l'orchestration (ordre des panneaux, `collect_shortcuts`, application différée, confirmations) ; `stokesforge-session` ; `stokesforge-render-egui` si la tâche touche au graphe | `render-svg`, `persistence`, les autres modules de `src/app/` |
| Menu, clic droit (entrée, section, grisage) | `docs/menus.md` | `stokesforge-app/src/app/menus.rs` + la peinture partagée de la surface (`object_menu`, `columns_menu_contents`…), `app/menu_sections.rs` | les autres modules de `src/app/` |
| Comportement d'un champ, message, dialogue, confirmation, grisage, geste clavier/souris | `docs/standard-interface.md` (#217 : état *en place* / *à venir* de chaque section) | le module de la surface dans `src/app/`, `app/dialogs.rs`, `app/shortcuts.rs`, `theme.rs` | les autres modules de `src/app/` |
| Texte d'interface (libellé, infobulle, message) | `docs/textes-interface.md` + skill `stokesforge-textes-interface` | le seul fichier de `stokesforge-app` ou `stokesforge-app-state` qui porte le texte (catalogues dans `stokesforge-app-state/i18n/`) | le reste du crate, les autres crates |
| Test de robustesse `bN` | `docs/tests.md` | `stokesforge-robustesse/src/lib.rs` (index) + le **seul** fichier `bN` visé + le crate qu'il éprouve | les 20 autres fichiers de `tests/` |
| Question transverse (« où vit X ? ») | `docs/README.md` | rien encore | tout |

## Les pièges qui coûtent le plus cher

Quatre seulement, parce qu'ils ne lèvent aucune erreur et qu'on ne les voit pas
en relisant son propre diff.

**Mesure de texte (rendu).** `plotters` est pris sans ses fonts. Un backend qui
ne surcharge pas `draw_text` panique ; un qui ne surcharge pas
`estimate_text_size` mesure le texte au nombre d'octets UTF-8, **silencieusement**.
Toute mesure passe par `stokesforge_render::text` et par nulle part ailleurs. Le
filet est `stokesforge-render-egui/tests/accord.rs` : s'il change, lire ses deux
familles de **témoins** avant — sans elles, un test d'égalité passe aussi bien
parce que les deux chemins sont justes que parce qu'ils sont faux de la même
façon.

**Une fonctionnalité de tracé ne s'écrit pas dans un backend.** Elle va dans
`stokesforge-render`, générique sur `DrawingBackend`. Seule exception vivante :
`stokesforge-render-egui::feathering` (la tessellation n'existe que pour l'écran).
Et le feathering n'entre **pas** dans la clé de cache.

**Zoom et pan ne sont pas des mutations du modèle.** `ViewState` (`stokesforge_render::view`, sans egui) vit à côté, sur
`GraphView`, et ne touche jamais au `PlotSpec` du `Tree` — personne ne veut
annuler un zoom avec `Ctrl+Z`. La règle « toute mutation passe par une commande »
ne s'applique donc pas au cadrage de vue.

**Un plafond n'est pas forcément un bug.** Longueur de nom, `MAX_NODE_ID`,
`MAX_CELLS`, nombre de classes d'histogramme, limites d'historique : déjà
arbitrés, documentés à l'endroit de leur constante et recensés dans
`docs/archive/rustrewrite/questions-ouvertes.md` §11. Vérifier avant de
« corriger ».

## Features : ce que `cargo build --workspace` ne compile pas

Seule `python` est hors `default`. Une tâche qui la touche doit compiler et
tester **explicitement**, sinon elle valide du code mort. Typst n'est plus une
feature : c'est une dépendance ferme des crates de rendu, que `cargo build` et
`cargo test --workspace` compilent et testent.

- `python` → `scripts/python-test.sh` (pose `PYO3_CONFIG_FILE` et les autres
  variables ; sans lui, PyO3 ne compile pas). L'interpréteur est vendorisé dans
  `vendor/python/` et **n'est pas versionné**.

## `docs/archive/`

Les commentaires du code citent des documents par leur nom de fichier
(`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6,
`roadmap-rendu.md`…) : ils sont tous dans `docs/archive/`. Non maintenus —
utiles pour le *pourquoi* d'une décision et pour les options déjà écartées,
jamais comme description de l'état actuel. Deux valent le détour avant d'agir :
`format-fichier-plx.md` avant toute modification de schéma, et
`questions-ouvertes.md` avant de traiter un comportement qui ressemble à un bug.

Les phases citées un peu partout (0-5 pour le socle, R0-R8 pour le rendu) sont
**toutes closes** : un `R6`/`R7`/`R8` dans un commentaire désigne du travail
fait, pas du travail à venir.

## Conventions par domaine

Les règles détaillées par domaine (briques de `stokesforge-app`, menus,
séries, copie, analyse, `pyst`, pièges du rendu) sont dans
[`conventions.md`](conventions.md) ; la table de `CLAUDE.md` dit quelle section
lire. Une grande recherche (beaucoup de lecture) part à l'agent `recherche`
(`agents/recherche.md`, Haiku) : voir `CLAUDE.md`, « Délégation ».
