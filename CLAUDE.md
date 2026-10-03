# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Contexte

Branche `rewriterust` : portage de StokesForge (analyse/tracé de données scientifiques, façon OriginLab) de Python/Qt vers Rust. Tout le code vit dans le workspace Cargo à la racine — l'ancienne implémentation Python/Qt (`legacy-python/`, qui servait de spec de comportement) a été supprimée de cette branche ; elle continue d'exister sur `main`.

Documentation du code : `docs/` — [`docs/README.md`](docs/README.md) est le point d'entrée et route par tâche. `docs/invariants.md` liste les règles à ne pas casser ; les lire avant de toucher au rendu, à l'historique ou à l'acteur.

`docs/archive/` conserve les documents de conception antérieurs (cahier des charges technique, roadmaps 0-5 et R0-R8, rapports de robustesse, options explorées). Ils ne sont plus maintenus, mais **les commentaires du code les citent par leur nom de fichier** (`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6…) : toute citation de ce genre se résout dans `docs/archive/`. Là où ils divergent du code, c'est le code qui a raison. Ils datent d'avant le renommage (#87) : ils parlent encore de PlotYx, `plotyx-*`, `.plx` et `pyx`, qui se lisent aujourd'hui StokesForge, `stokesforge-*`, `.stf` et `pyst` (le fichier `format-fichier-plx.md` garde son nom).

Exception : les rapports de bugs `retours-2026-09-23-bugs.md` et `retours-2026-09-24-bugs.md` (B1…B9, « Bug 1 »…), cités dans `crates/stokesforge-app/src/app.rs`, ont été supprimés — tous leurs bugs sont corrigés. Ils se lisent par `git show a6fd933^:docs/chantiers/<fichier>`. Les bugs se suivent désormais en issues GitLab (`glab issue list --label bug`), plus en `.md`.

## Commandes

```sh
cargo build --workspace
cargo test --workspace                       # inclut stokesforge-robustesse (sauf #[ignore])
cargo test -p <crate> <nom_test>              # un seul test
cargo test -p stokesforge-robustesse -- --ignored  # bancs coûteux
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture (crates/, dépendances bas→haut)

`stokesforge-id` (clés stables) + `stokesforge-plot` (description de graphe neutre vis-à-vis du renderer) → `stokesforge-core` (modèle Project/Folder/Workbook/Worksheet/Column, zéro dépendance UI) → `stokesforge-commands` (bus de commandes annulables) et `stokesforge-persistence` (format `.stf`), deux crates **parallèles** au-dessus de `core`/`plot` → `stokesforge-session` (acteur propriétaire du modèle pour ses clients, concurrence ; dépend de commands **et** persistence) → `stokesforge-script`/`stokesforge-python` (pont PyO3) → `stokesforge-app` (coquille UI sur eframe). `stokesforge-cli import` construit un `Project` neuf directement, sans session ; `stokesforge-cli run` (feature `python`) passe par `ScriptEngine` et une session headless, avec les mêmes commandes et groupes annulables que la console. `stokesforge-import` (fichiers d'appareil) ne dépend d'aucun crate du workspace ; `stokesforge-importer` en fait des colonnes du modèle (partagé par l'application, le CLI et les scripts `pyst.import_file`, sans commandes) ; `stokesforge-arch-guard` est l'outil de test des garde-fous de dépendances. Le graphe exact est dans `docs/architecture.md`.

Rendu (phases R0-R8 closes) : `stokesforge-render` (mesure de texte unique via `epaint`, pipeline Plotters générique sur le backend, et **tout le rendu** : types de série, échelles d'axe, primitives en pixels, rognage, légende, LTTB, interaction, formules Typst, graphes d'exemple) → `stokesforge-render-egui` (backend `DrawingBackend` collectant des `egui::Shape`, cache `(revision, rect, pixels_per_point, view_generation)`, zoom/pan, levier de *feathering*) et `stokesforge-render-svg` (export SVG, et par-dessus PDF et PNG). Détail : `docs/rendu.md`.

`stokesforge-robustesse` : pas de code de prod, dépend de tout le workspace — vingt-et-une batteries de tests (`b1`…`b21`, voir son `src/lib.rs`) qui éprouvent le socle aux limites.

Feature hors `default` : `python` (pont PyO3, exige `PYO3_CONFIG_FILE` — voir `scripts/python-test.sh`), seule du dépôt ; `cargo build --workspace` ne la compile pas. `scripts/python-test.sh` teste pont, scripts, robustesse, application et CLI ; le travail CI `test-python` est automatique et bloquant. Typst (notations scientifiques) est une dépendance ferme des trois crates de rendu : `cargo build/test --workspace` le compilent et le testent.

Transitions #72–#73 : Wayland natif est le défaut, `STOKESFORGE_X11=1` force
X11 ; les patches `vendor/winit` et `vendor/smithay-clipboard`, hors workspace,
partagent le récepteur de dépôt et le presse-papiers entre les viewports.
Choix, commandes `rustfmt` ciblées, vérification manuelle et retrait :
`vendor/winit/STOKESFORGE.md`. Le parseur et le canal exacts sont testés par
`stokesforge-app/tests/wayland_{uri,bridge}.rs`.
Le graphique vide passe par `AddGraph` ; `SessionHandle::new_project`, comme
`load`, remplace l'identité et vide l'historique après confirmation dans l'UI.

Règles de chargement par domaine (quel crate/doc charger selon la tâche) : `.claude/rules.md`.

Skills de projet : `.claude/skills/` — onze compétences par type de tâche
(implémenter une fonctionnalité, signaler puis corriger un bug, ajouter une
commande annulable, rendu, import, pont Python, vérifier avant de committer,
répondre sur l'architecture ou sur les fonctionnalités, écrire un texte
d'interface). Index et
enchaînements : [`.claude/skills/README.md`](.claude/skills/README.md).

## Conventions

- `rustfmt` + `clippy -D warnings` obligatoires (CI `.gitlab-ci.yml`).
- Quand un travail installe une règle nouvelle (standard d'interface, motif à suivre, frontière), mettre à jour **au besoin** les skills concernés (`.claude/skills/`) et ce fichier, dans le même commit que le code, pour garder le développement cohérent. Voir `.claude/skills/README.md` (règles transversales).
- Bugs et évolutions se suivent en issues GitLab (`glab issue list`), pas en `.md` ; chaque issue porte une ligne « **Cohérence** » qui dit quels skills et quelles docs mettre à jour.
- Commits et docs techniques en français.
- `pyst` : les écritures de #67 réutilisent les commandes, dans une macro `Script` ouverte à la première mutation et avortée sur erreur non rattrapée/interruption. Le moteur possède sa propre identité de client ; les poignées sont revalidées après réservation. `set_values` remplace longueur et cellules sans changer le type ni la provenance ; `set_column` reste strict. `undo()` annule le groupe courant et fait un point de validation. Les objets graphiques (`add_shape`, `remove_shape`, #76) rejoignent également ce groupe, avec relecture du spec sous réservation. Contrat : `stokesforge-script/tests/mutations.rs`, `tests/graphs.rs`, `docs/python-et-script.md`.
- `pyst` : la navigation (`root`, `children`, `info`) lit un seul instantané par appel et conserve son époque dans toutes les poignées rendues. `column` garde sa vue numpy numérique sans copie ; `values` copie les cellules en `float`/`str`/`None`, sans analyser les textes ni confondre vide, chaîne vide et NaN. Contrat : `docs/python-et-script.md`, `stokesforge-script/tests/navigation.rs` (#66).
- Toute mutation du modèle passe par une commande annulable de `stokesforge-commands`, jamais par une primitive `core` en direct.
- `stokesforge-app` : tout réglage de couleur passe par `color_picker::color_field` avec l'usage de la couleur (`ColorUsage::Curves`/`Text`/`Background`) et le `PickerEnv` des couleurs enregistrées, jamais par un champ texte `#RRGGBB`. Dans une barre d'outils, l'échantillon est un petit carré précédé d'une icône (`color_icon_button`, #89) ; dans un formulaire, `color_field` (libellé + carré). Couleur vide = automatique, peinte en carré barré d'un trait rouge ; opacité en `#RRGGBBAA` ; le texte et les échelles automatiques suivent `stokesforge_render::color::auto_ink` (blanc sur fond sombre).
- `stokesforge-app` : toute fenêtre passe par la brique commune — `show_window_in_layer` pour une fenêtre de contenu, `modal_dialog_with` pour un dialogue ou un assistant (même cadre, même barre de titre, même blocage que les propriétés du graphe, pied de page personnalisable) ; jamais d'`egui::Window` brute, même pour un assistant à étapes (verrouillé par le test `aucune_fenetre_n_est_une_egui_window_brute`). Un code existant qui déroge n'est pas un modèle.
- `stokesforge-app` : toute icône passe par `icons::icon_image` / `icon_button` (Phosphor duotone, rôle de couleur `IconRole`, info-bulle `tr!` obligatoire), jamais par un glyphe Unicode ni un emoji ; couleur rare, règles dans `docs/icones.md` (verrouillé par `aucune_icone_hors_du_composant`).
- `stokesforge-app` : tout texte d'interface passe par `tr!` avec une clé du catalogue `en`, rédigée selon `docs/textes-interface.md` ; jamais de littéral (verrouillé par `aucun_libelle_en_dur`). Toute clé s'écrit **dans les deux** catalogues (`en` et `fr`) dans le même commit (complétude bloquante). Aucune dérogation : la garde est absolue (#48), et une clé du catalogue qu'aucun appel n'emploie fait échouer `aucune_cle_du_catalogue_n_est_morte`.
- Un libellé d'historique est une **donnée** (`stokesforge_commands::HistoryLabel`, `MacroKind` fermé pour `begin_macro`), jamais une phrase : les crates du dessous ne mettent rien en phrase, `stokesforge-app` le fait par `history_text` et les clés `history-*` (#46).
- Erreurs : le `Display` (`#[error(...)]`) des crates du dessous est en **anglais brut**, jamais traduit (CLI, `pyst`, journaux). `stokesforge-app` les met en phrase par `errors::<famille>::error_message`, qui associe chaque variante à une clé `error-*` du catalogue `en` ; un `match` exhaustif, sans `_`, force à associer toute variante nouvelle. Jamais de `to_string()` ni de `{err}` d'une erreur affichée à l'utilisateur.
- `stokesforge-app` : une barre d'outils est une variante de `toolbar::ToolbarId` dont le contenu s'écrit dans `toolbar::bar_contents` avec des `ToolbarButton` (icône du composant commun, info-bulle `tr!`) qui poussent les `Action` des menus ; jamais un second chemin d'exécution ni un panneau posé à côté. Sa disposition vit dans `preferences.json`, pas dans le `.stf`. Le format de texte (`ToolbarId::Text`) écrit des balises Typst dans le texte via `text_format::apply` pour le gras, l'italique, la couleur et le corps d'un passage ; la **police** (et, pour un titre ou la légende, le corps) est un champ de la zone (`TextFont`, `AnnotationSpec::font`, #77/#78), posé par `StokesForgeApp::format_zone` — jamais une balise `#text(font: …)` dans le contenu.
- `stokesforge-app` : l'aspect des barres de défilement (largeur, couleur) vient **uniquement** de `theme::scroll_style`, posé globalement par `theme::install` ; jamais de `ScrollStyle` local (verrouillé par le test `aucun_scroll_style_hors_du_theme`). La place d'une barre se lit par `theme::scroll_bar_allocated_width`.

## Pièges connus

- Fenêtres détachées (#104) : publier un instantané ne réveille pas le callback `show_viewport_deferred`. `app.rs` compare époque, révision métier et révision des géométries, puis demande le redessin du viewport lecteur ; une mutation appliquée après peinture programme l'image racine de publication. Le Project Manager suit aussi sa sélection, son masquage et l'onglet actif. Aucun réveil périodique au repos.
- Avant undo/historique ou format `.stf` : `docs/commandes-et-historique.md`, `docs/persistance.md`, puis `docs/archive/rustrewrite/format-fichier-plx.md` pour les options déjà écartées.
- `docs/archive/rustrewrite/questions-ouvertes.md` §11 : plafonds/compteurs documentés — vérifier avant de "corriger" un comportement qui ressemble à un bug.
- Rendu : `plotters` est pris **sans ses fonts** (`default-features = false`). Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la première graduation, et un qui ne surcharge pas `estimate_text_size` mesure le texte au nombre d'octets UTF-8 — silencieusement. Toute mesure passe par `stokesforge_render::text` et par nulle part ailleurs (`architecture-choisie.md` §4.3/§4.4).
- Rendu : **une fonctionnalité de tracé n'entre jamais dans un seul backend** — elle va dans `stokesforge-render`, qui est générique sur le backend. Corollaire du même piège : les éléments de Plotters qui mesurent du texte eux-mêmes (`configure_series_labels`, `MultiLineText`) passent par `FontData::estimate_layout`, donc par l'heuristique naïve, **pas** par `estimate_text_size` — la légende est écrite à la main pour cette raison (`stokesforge_render::legend`).
- Rendu : **tout titre, titre d'axe et entrée de légende passe par Typst, les graduations jamais** (issue #32). Le rôle est décidé dans le pipeline par le style (`formula::rich_style`), et les deux backends aiguillent sur `formula::is_rich` à la mesure **et** au dessin — jamais sur la présence de `$`. Ces textes sont donc du markup Typst : un texte repris automatiquement (nom de colonne) doit être échappé en amont.
- Rendu : le levier `TessellationOptions::feathering` vit dans `stokesforge_render_egui::feathering`, en fonction libre sur le `Context`. Il n'est **pas** dans la clé de cache et ne doit pas y entrer (`architecture-choisie.md` §4.2, verrouillé par `stokesforge-render-egui/tests/feathering.rs`). Son déclencheur, le glisser, est câblé dans `stokesforge-render-egui/src/lib.rs` depuis R7.3.

Object Manager (#81–85) : `object_manager.rs` projette le graphe, la sélection
reste dans `GraphView` (même vue en fenêtre détachée). Les bascules réutilisent
`EditPlotSpec`, dont le motif d'historique reste une donnée. Les échantillons
appellent `series_swatch`, partagé avec la légende.
