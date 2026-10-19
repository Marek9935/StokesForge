# Catalogue des fenêtres et dialogues (#300)

Inventaire de **toutes** les fenêtres de `stokesforge-app` : ce qui existe, de
quel type, avec quelle brique, quelle taille, quelles pages, d'où on l'ouvre.
Sert de point de départ avant de créer une fenêtre neuve : on repère le
jumeau, on l'ouvre, on l'imite. Le **comportement** (ordre des boutons,
Échap, focus, grisé…) est dans [`standard-interface.md`](standard-interface.md),
§ Fenêtres et dialogues et § Confirmations ; cette page ne le recopie pas.

Règles de tenue :

- **E1 : ce catalogue se tient à jour à chaque fenêtre.** Toute fenêtre ou
  dialogue ajouté, retiré, redimensionné ou dont les pages changent met sa
  ligne à jour dans le même commit.
- **E2 : maquette validée par l'utilisateur avant tout code de fenêtre
  neuve.** Toujours, sans exception de taille. La maquette se présente comme
  une décision (formulaire de décisions,
  `.claude/skills/stokesforge-bug-report/references/formulaire-de-decisions.md`) ;
  le code ne commence qu'après accord.

Le code fait foi : les tailles ci-dessous sont les tailles **par défaut**
passées à la brique ; la taille et la position utilisées sont ensuite celles
que l'utilisateur a laissées (S-F8, voir plus bas).

## Les types

| Type | Brique (`app/dialogs.rs`) | Pied (`FooterLayout`) | Quand |
|---|---|---|---|
| **Réglages en direct** (S-F1) | `modal_dialog` | `[Cancel] [OK]` | un objet visible se règle, OK garde, Cancel restaure |
| **Création sur brouillon** (S-F1) | `modal_dialog_with` | `[secondaires] [message] [Cancel] [verbe]` | une action qui crée ; le bouton porte son verbe (Create, Import) |
| **Choix** | `modal_dialog_with` | idem, secondaires = autres réponses | une question sans réponse « oui » |
| **Assistant** | `modal_wizard` | `[Cancel] [message] [Back · Next] [Finish]` | plusieurs étapes ordonnées |
| **Lecture seule** (S-F2) | `modal_read_only` | `[secondaires] [message] [Close]` | consulter, ou régler sans rien à valider |
| **Confirmation** (S-F3 à S-F5) | `modal_confirm` | `[message] [Cancel] [verbe]` | seulement ce que Ctrl+Z ne rattrape pas |
| **Enregistrer ?** | `modal_save_changes` | `[Discard Changes] [message] [Cancel] [Save]` | projet non enregistré |
| **Échec grave** (S-M8) | `modal_alert` | `[action] [message] [Close]` | échec avec une action possible |
| **Fenêtre de contenu** (non modale) | `window_chrome::show_floating_window` | aucun | classeur, graphe, matrice, note |

Les cinq premières lignes de brique passent toutes par le même corps
`modal_frame` (même cadre, même barre de titre, même blocage du reste de
l'interface) ; la fenêtre de contenu passe par `show_window_in_layer`, une
`egui::Area` et non une `egui::Window`. `egui::Window` brute est interdite
(garde `aucune_fenetre_n_est_une_egui_window_brute`).

## Dialogues modaux

Tous sont peints dans `StokesForgeApp::show_edit_dialogs` (`app.rs`), tant que
leur état est ouvert dans `AppState`, sauf les blocants (dernière section).

| Fenêtre | Type | Brique | Taille par défaut | Pages ou contenu | Ouverture | Fichier |
|---|---|---|---|---|---|---|
| Propriétés du graphe | réglages en direct | `modal_dialog` | 380 x 460 | 5 onglets (`tab_strip`) : Graph, Plot Area, Axes, Legend, Theme | clic droit sur un composant du graphe, menu Graph, bouton Properties de la barre d'outils (`Action::OpenPlotSettings`) | `plot_settings.rs` |
| Propriétés des séries | réglages en direct | `modal_dialog` | 640 x 500 | liste des séries à gauche, éditeur à droite (`curve_settings_page`) | double-clic sur une courbe, Object Manager, menu Graph (`Action::OpenObjectProperties` sur une série ou un groupe) | `plot_settings.rs` (fenêtre), `curve_settings.rs` (page) |
| Propriétés d'un objet (rectangle, flèche…) | réglages en direct | `modal_dialog` | 360 x 280 | coordonnées X1 Y1 X2 Y2, couleurs de trait et de fond | double-clic sur l'objet, Object Manager (`open_shape_properties`) | `graph_window.rs` |
| Analyse (nouvelle ou modification) | création sur brouillon | `modal_dialog_with` | 1000 x 660 | deux colonnes : réglages (330 de large, défilants) et aperçu cliquable avec outils et valeurs | menu Analysis, clic droit, panneau des résultats (`Action::OpenAnalysis`) | `analysis_dialog.rs`, `analysis_forms.rs`, `analysis_preview.rs` |
| Assistant d'import | assistant | `modal_wizard` | 1000 x 620 | 6 étapes : Files, Format, Header, Columns, Metadata, Destination ; aperçu du fichier à droite à toutes les étapes | File > Import, Ctrl+3 (`open_import_dialog`) | `import_wizard.rs` |
| Préférences | lecture seule (réglages immédiats) | `modal_read_only` | 420 x 300 | 3 onglets : General (langue, analyse), Display (nouveaux classeurs), Export (formats de Copy Graph) | File > Preferences (`Action::OpenPreferences`) | `preferences_window.rs` |
| Raccourcis clavier | lecture seule | `modal_read_only` | 480 x 460 | liste défilante par zone, générée de `shortcuts::HELP` | Help > Keyboard Shortcuts (`Action::OpenKeyboardShortcuts`) | `keyboard_shortcuts_window.rs` |
| Historique de la barre d'état | lecture seule | `modal_read_only` | 520 x 360 | les 50 derniers messages, état vide si aucun | clic sur la ligne d'état (`Action::OpenStatusHistory`) | `status_history.rs` |
| Liste des R² faibles | lecture seule | `modal_read_only` | 480 x 170 | compteurs par type, texte normal | pastille de la barre d'état (`Action::OpenLowR2List`) | `status_history.rs` |
| Métadonnées d'une colonne | lecture seule | `modal_read_only` | 520 x 420 | grille clé / valeur défilante, bouton « Use as x' » par ligne numérique | clic sur la cellule de métadonnées de l'en-tête d'une colonne, quand elle n'est pas vide (`Action::ViewColumnMetadata`) | `plot_settings.rs` |
| Suppression d'un calcul | choix | `modal_dialog_with` | 520 x 190 | Delete, Delete with Columns, Cancel | suppression d'un calcul (`Action::RequestDeleteCalculation`, panneau des résultats ou menu d'analyse) | `analysis_dialog.rs` |
| Export de séries périmées | choix | `modal_dialog_with` | 560 x 170 | Export Anyway, Export Without Warning, Recalculate and Export, Cancel | export ou copie d'un graphe qui montre un calcul pas à jour | `stale_export.rs` |
| Remplacement d'une analyse partagée | confirmation | `modal_confirm` | 520 x 170 | un message | appliquer une analyse dont la source nourrit d'autres courbes | `analysis_dialog.rs` |
| Suppression d'un modèle d'import | confirmation | `modal_confirm` | 420 x 150 | un message | bouton de suppression d'un modèle, à l'étape Files ; peinte **dans** l'assistant | `import_wizard.rs` |

## Dialogues bloquants

`confirmations::show` (`app/confirmations.rs`) les peint après tout le reste ;
`AppState::blocking_dialog` (projet non enregistré, dépôt de lignes, alerte,
écrasement) fait vider les autres actions de l'image
(`blocking_confirmations`). Taille par défaut commune : 480 x 170.

| Fenêtre | Type | Brique | Ouverture |
|---|---|---|---|
| Projet non enregistré | enregistrer ? | `modal_save_changes` | New Project, Open…, Quit, fermeture de la fenêtre (`AppState::unsaved_pending`) |
| Échec d'enregistrement / d'ouverture | échec grave | `modal_alert` | un enregistrement ou une ouverture qui échoue (`AlertKind::SaveFailed`, `OpenFailed`) |
| R² faible (avertissement) | lecture seule | `modal_read_only` | fin d'un calcul dont le R² est faible, si la préférence l'autorise (`AlertKind::LowRSquared`) |
| Écraser un fichier | confirmation | `modal_confirm` | export dont l'extension ajoutée vise un fichier existant (`pending_overwrite`) |
| Axe d'un dépôt de lignes | choix | `modal_dialog_with` | glisser des lignes sur un graphe qui a déjà un axe X (`pending_row_drop`) |

Les deux alertes (échec, R² faible) partagent l'identifiant
`stokesforge_alert` : une seule à la fois.

## Fenêtres de contenu (non modales)

Classeur, graphe, matrice, note : des onglets de l'espace de travail, peints en
fenêtres flottantes confinées (`show_floating_window`) ou détachées en fenêtre
de l'OS (`window_detach.rs`), avec un en-tête à icônes (replier, détacher,
maximiser, fermer ; `HeaderPlacement::Floating` / `Detached`). Pas de pied, pas
de focus initial imposé. Tailles par défaut (`window_placement.rs`) : classeur
373 x 420 (deux tiers de 560), graphe 560 x 460, matrice et note 560 x 420 ;
minimum 220 x 140. Ouverture : `open_tab` (`window_stack.rs`), depuis le Project Manager ; une
seconde ouverture sur la même entrée amène la fenêtre au premier plan sans la
dupliquer. Chaque nouvelle fenêtre est décalée en cascade.

## Hors catalogue

- **Éditions sur place** (titre, légende, texte d'un graphe, cellule de la
  grille, renommage) : pas de fenêtre, voir le standard (S-T4) et
  `inplace_edit.rs`.
- **Fenêtres natives de l'OS** : sélecteur de fichiers (`file_picker.rs`),
  jamais une fenêtre egui.
- **Sélecteur de couleur** : une fenêtre surgissante ancrée au bouton-échantillon
  (`color_picker.rs`), pas un dialogue du catalogue.
- **Prototype de test** : `preferences_proto.rs`, compilé en test seulement,
  banc de la forme avancée de #148 (cinq pages dont Folders and Paths et
  Analysis, pas encore livrées). Ses captures de référence sont dans
  `crates/stokesforge-app/tests/snapshots/preferences_proto_*.png` ; ce n'est
  pas une fenêtre de l'application.

## Taille, position, Échap, focus : ce que la brique fait seule

Rien de cela ne se réécrit dans une fenêtre neuve (S-F7 à S-F9, détail dans le
standard) :

- la taille et la position sont mémorisées **par identifiant** de dialogue
  (clé du dialogue = `egui::Id`), dans les préférences, 32 sortes au plus ;
  donner à une fenêtre neuve un identifiant `stokesforge_<nom>` stable et
  unique ;
- Échap : un champ texte puis le dialogue ; la croix vaut Cancel ;
- focus initial : le premier champ texte, trouvé par la brique ;
- zone de message du pied : l'erreur (`AppState::fail` tant qu'un dialogue de
  contenu est ouvert) ou la raison du bouton principal grisé
  (`DialogFooter::accept_disabled`).

## Anatomie d'une fenêtre neuve

1. **Type.** Choisir dans la table des types. En cas de doute entre
   confirmation et choix : une confirmation n'existe que pour ce que Ctrl+Z ne
   rattrape pas (standard S-F3).
2. **Maquette (E2).** La faire valider avant tout code : taille, pages ou
   sections, contenu de chaque page, pied, point d'ouverture. Joindre, quand
   c'est utile, une capture du jumeau (voir ci-dessous).
3. **Jumeau.** Ouvrir la fenêtre modèle du type (table suivante) et la copier,
   sans en inventer l'ossature.
4. **Textes.** Clés Fluent `en` d'abord, glossaire de
   [`textes-interface.md`](textes-interface.md) (skill
   `stokesforge-textes-interface`) : titre, boutons, bulles `hint`, raison de
   grisage `disabled-*`.
5. **État.** Un drapeau ou un `Option` dans `AppState` et deux actions
   `Open…` / `Close…` (le dialogue ne mute pas le modèle ; toute mutation passe
   par une commande annulable, voir `stokesforge-commande`).
6. **Peinture.** Une fonction `xxx_window(ctx, &state, actions)` qui rend
   immédiatement si l'état est fermé, puis l'ajouter à `show_edit_dialogs`
   (ou à `confirmations::show` si elle doit bloquer le reste).
7. **Briques de contenu.** `form`/`choice` (`form.rs`), `number_field`,
   `tab_strip`, `hover_help`, `empty_state`, `theme::SPACE_*`. Aucune bulle
   nue, aucun `DragValue`, aucune couleur en dur : les gardes de
   `tests/gardes.rs` le refusent.
8. **Tests.** Peindre la fenêtre sans fenêtre réelle (`test_support.rs`) et
   lire les textes peints ; la garde `aucune_fenetre_n_est_une_egui_window_brute`
   et `tests/libelles.rs` couvrent déjà la brique et les libellés.
9. **Catalogue (E1).** Ajouter la ligne de cette page dans le même commit.

### Fenêtre modèle par type

| Type | Fenêtre modèle | Pourquoi celle-là |
|---|---|---|
| Réglages en direct | `plot_settings_window` (`plot_settings.rs`) ; `shape_properties_ui` (`graph_window.rs`) pour le plus petit | brouillon, validation unique, onglets |
| Création sur brouillon | `analysis_window` (`analysis_dialog.rs`) | verbe du bouton, raison du grisage, aperçu |
| Choix | `stale_export_window` (`stale_export.rs`) | secondaires + principal, le plus court |
| Assistant | `import_window` (`import_wizard.rs`) | le seul ; Back · Next · Finish |
| Lecture seule | `keyboard_shortcuts_window.rs` (le plus simple) ; `preferences_window.rs` pour des réglages immédiats à onglets | Close seul |
| Confirmation | `analysis_replace_window` (`analysis_dialog.rs`) ; `confirmations.rs` pour les blocantes | Cancel par défaut |

## Produire une capture

Ne pas en fabriquer : les produire avec le skill `run-stokesforge`
(`.claude/skills/run-stokesforge/SKILL.md`).

```sh
cargo build -p stokesforge-app -p stokesforge-cli
D=.claude/skills/run-stokesforge/driver.py
python3 $D start [projet.stf]      # sans argument : projet de démonstration
python3 $D click 20 11             # menu File ; puis l'entrée qui ouvre la fenêtre
python3 $D ss /tmp/fenetre.png     # capture, à lire avec Read
python3 $D stop
```

Gestes d'ouverture utiles : File > Preferences, File > Import (ou Ctrl+3),
Help > Keyboard Shortcuts, clic sur la ligne d'état pour l'historique, clic
droit sur un graphe ou menu Graph pour les propriétés. Les coordonnées de
menu sont en pixels fixes (voir le skill) ; relire la capture avant de la
comparer au jumeau. L'anglais est la langue de référence : repasser en English
après un essai en français (la langue est enregistrée dans les préférences).
Pour une mise en page seule, les bancs de tests (`egui_kittest`, voir
`preferences_proto.rs`) rendent la fenêtre sans lancer l'application.
