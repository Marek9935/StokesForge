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
que l'utilisateur a laissées (S-F8, voir plus bas). Deux gardes tiennent cette
page (S-F11) : `le_catalogue_des_fenetres_et_le_code_concordent` (tout
identifiant `stokesforge_…` passé à un `modal_*` y figure, et chaque ligne a son
appel ; **elle ne lit pas les tailles**) et `aucune_taille_libre_sans_justification`.

## Les natures et les classes de taille (S-F11)

Une fenêtre modale a une **nature** (`DialogNature`, `app/dialogs.rs`) : elle
fixe le pied, la taille par défaut, le focus initial et le bouton par défaut.
Chaque `match` sur la nature est exhaustif, sans `_` : une nature nouvelle ne
compile pas avant de les avoir dits. La taille est un **champ** de la variante,
dont la valeur par défaut est une classe :

| Classe | Taille | Pour |
|---|---|---|
| Petite | 480 x 170 | une question, un message, quelques boutons |
| Moyenne | 480 x 420 | des réglages ou un formulaire à quelques pages |
| Grande | 640 x 500 | une liste et son éditeur côte à côte |

- **Fenêtre interactive** (réglages, réglages en direct, brouillon, assistant : `InteractiveSize`) :
  la classe par défaut de la nature, une autre classe, ou une **taille libre**
  (`InteractiveSize::Free`) qui exige `// TAILLE-LIBRE(#300): <justification>`
  dans les douze lignes qui précèdent (15 caractères au moins). Tailles libres
  aujourd'hui : l'analyse (1000 x 660), l'assistant d'import (1000 x 620) et le
  banc `preferences_proto` (560 x 470, captures de référence).
- **Fenêtre sans interaction riche** (choix, lecture seule, confirmation,
  enregistrement, alerte : `PlainSize`) : peut prendre la **taille de son
  contenu** (`PlainSize::Content`) **sans justification** — pas de formulaire à
  ranger, une taille mal choisie y coûte peu. Le type ne l'offre pas aux
  fenêtres interactives.

| Nature | Brique | Classe par défaut | Focus initial |
|---|---|---|---|
| `Settings` | `modal_dialog` | Moyenne | premier champ (S-F9) |
| `Draft` | `modal_dialog_with` | Moyenne | premier champ |
| `Choice` | `modal_dialog_with` | Petite | premier champ (s'il y en a) |
| `Wizard` | `modal_wizard` | Grande | premier champ |
| `LiveSettings` | `modal_live_settings` | Moyenne | premier champ (S-F9) |
| `ReadOnly` | `modal_read_only` | Moyenne | aucun |
| `Confirm` | `modal_confirm` | Petite | Cancel (S-F5) |
| `SaveChanges` | `modal_save_changes` | Petite | Save |
| `Alert` | `modal_alert` | Petite | Close |

`modal_dialog_with` prend la nature (réglages, brouillon ou choix) ; les autres
briques ont une nature fixe et prennent le champ de taille de cette nature.
Le bouton principal d'un pied est soit **toujours actif**
(`DialogFooter::always(libellé, bulle)`), soit **peut être grisé**
(`DialogFooter::gated(libellé, bulle, actif, raison)` : bulle et raison
obligatoires, S-F10).

## Les types

| Type | Brique (`app/dialogs.rs`) | Nature | Pied | Quand |
|---|---|---|---|---|
| **Réglages en direct** (S-F1) | `modal_dialog` | `Settings` | `[Cancel] [OK]` | un objet visible se règle, OK garde, Cancel restaure |
| **Création sur brouillon** (S-F1) | `modal_dialog_with` | `Draft` | `[secondaires] [message] [Cancel] [verbe]` | une action qui crée ; le bouton porte son verbe (Create, Import) |
| **Choix** | `modal_dialog_with` | `Choice` | idem, secondaires = autres réponses | une question sans réponse « oui » |
| **Assistant** | `modal_wizard` | `Wizard` | `[Cancel] [message] [Back · Next] [Finish]` | plusieurs étapes ordonnées |
| **Réglages appliqués en direct** (S-F2) | `modal_live_settings` | `LiveSettings` | `[message] [Close]` | régler sans rien à valider ni annuler : pied de lecture seule, mais des champs (focus, taille par classe) |
| **Lecture seule** (S-F2) | `modal_read_only` | `ReadOnly` | `[secondaires] [message] [Close]` | consulter seulement |
| **Confirmation** (S-F3 à S-F5) | `modal_confirm` | `Confirm` | `[message] [Cancel] [verbe]` | seulement ce que Ctrl+Z ne rattrape pas |
| **Enregistrer ?** | `modal_save_changes` | `SaveChanges` | `[Discard Changes] [message] [Cancel] [Save]` | projet non enregistré |
| **Échec grave** (S-M8) | `modal_alert` | `Alert` | `[action] [message] [Close]` | échec avec une action possible |
| **Fenêtre de contenu** (non modale) | `window_chrome::show_floating_window` | aucune | aucun | classeur, graphe, matrice, note |

Les cinq premières lignes de brique passent toutes par le même corps
`modal_frame` (même cadre, même barre de titre, même blocage du reste de
l'interface) ; la fenêtre de contenu passe par `show_window_in_layer`, une
`egui::Area` et non une `egui::Window`. `egui::Window` brute est interdite
(garde `aucune_fenetre_n_est_une_egui_window_brute`).

## Dialogues modaux

Tous sont peints dans `StokesForgeApp::show_edit_dialogs` (`app.rs`), tant que
leur état est ouvert dans `AppState`, sauf les blocants (dernière section).

| Fenêtre | Nature | Taille | Pages ou contenu | Ouverture | Identifiant, fichier |
|---|---|---|---|---|---|
| Propriétés du graphe | `Settings` | Moyenne | 5 onglets (`tab_strip`) : Graph, Plot Area, Axes, Legend, Theme | clic droit sur un composant du graphe, menu Graph, bouton Properties de la barre d'outils (`Action::OpenPlotSettings`) | `stokesforge_plot_settings` (construit), `plot_settings.rs` |
| Propriétés des séries | `Settings` | Grande | liste des séries à gauche, éditeur à droite (`curve_settings_page`) | double-clic sur une courbe, Object Manager, menu Graph (`Action::OpenObjectProperties` sur une série ou un groupe) | `stokesforge_curve_settings` (construit), `plot_settings.rs` (fenêtre), `curve_settings.rs` (page) |
| Propriétés d'un objet (rectangle, flèche…) | `Settings` | Moyenne | coordonnées X1 Y1 X2 Y2, couleurs de trait et de fond | double-clic sur l'objet, Object Manager (`open_shape_properties`) | dérivé de l'objet, `graph_window.rs` |
| Analyse (nouvelle ou modification) | `Draft` | libre 1000 x 660 | deux colonnes : réglages (330 de large, défilants) et aperçu cliquable avec outils et valeurs | menu Analysis, clic droit, panneau des résultats (`Action::OpenAnalysis`) | `stokesforge_analysis` (construit), `analysis_dialog.rs`, `analysis_forms.rs`, `analysis_preview.rs` |
| Assistant d'import | `Wizard` | libre 1000 x 620 | 6 étapes : Files, Format, Header, Columns, Metadata, Destination ; aperçu du fichier à droite à toutes les étapes | File > Import, Ctrl+3 (`open_import_dialog`) | `stokesforge_import_wizard`, `import_wizard.rs` |
| Préférences | `LiveSettings` (réglages immédiats) | Moyenne | 3 onglets : General (langue, taille maximale d'un fichier à l'ouverture, analyse), Display (nouveaux classeurs), Export (formats de Copy Graph) | File > Preferences (`Action::OpenPreferences`) | `stokesforge_preferences`, `preferences_window.rs` |
| Raccourcis clavier | `ReadOnly` | contenu 480 x 460 | liste défilante par zone, générée de `shortcuts::HELP` | Help > Keyboard Shortcuts (`Action::OpenKeyboardShortcuts`) | `stokesforge_keyboard_shortcuts`, `keyboard_shortcuts_window.rs` |
| Historique de la barre d'état | `ReadOnly` | contenu 520 x 360 | les 50 derniers messages, état vide si aucun | clic sur la ligne d'état (`Action::OpenStatusHistory`) | `stokesforge_status_history`, `status_history.rs` |
| Liste des R² faibles | `ReadOnly` | contenu 480 x 170 | compteurs par type, texte normal | pastille de la barre d'état (`Action::OpenLowR2List`) | `stokesforge_low_r2_list`, `status_history.rs` |
| Métadonnées d'une colonne | `ReadOnly` | Moyenne | grille clé / valeur défilante, bouton « Use as x' » par ligne numérique | clic sur la cellule de métadonnées de l'en-tête d'une colonne, quand elle n'est pas vide (`Action::ViewColumnMetadata`) | `stokesforge_column_metadata`, `plot_settings.rs` |
| Suppression d'un calcul | `Choice` | Petite | Delete, Delete with Columns, Cancel | suppression d'un calcul (`Action::RequestDeleteCalculation`, panneau des résultats ou menu d'analyse) | `stokesforge_delete_calculation`, `analysis_dialog.rs` |
| Export de séries périmées | `Choice` | contenu 560 x 170 | Export Anyway, Export Without Warning, Recalculate and Export, Cancel (quatre libellés longs : plus large que Petite) | export ou copie d'un graphe qui montre un calcul pas à jour | `stokesforge_stale_export`, `stale_export.rs` |
| Colonne utilisée par un calcul | `ReadOnly` | contenu 480 x 170 | Undo, Close ; la liste des colonnes à formule et des calculs qui lisaient ce qu'on vient de supprimer | suppression de colonnes, d'une feuille ou d'un classeur lus par un calcul (#306), après le geste | `stokesforge_column_use`, `column_use.rs` |
| Copie avec en-tête de colonnes à formule | `Choice` | Petite | Values, Formulas (par défaut), Cancel | Ctrl+Maj+C ou Copy with Header quand une colonne copiée porte une formule (#306) | `stokesforge_full_copy`, `column_use.rs` |
| Remplacement d'une analyse partagée | `Confirm` | Petite | un message | appliquer une analyse dont la source nourrit d'autres courbes | `stokesforge_replace_analysis`, `analysis_dialog.rs` |
| Suppression d'un modèle d'import | `Confirm` | Petite | un message | bouton de suppression d'un modèle, à l'étape Files ; peinte **dans** l'assistant | `stokesforge_import_template_delete`, `import_wizard.rs` |

Les identifiants « construits » sont choisis avant l'appel (un `&str`, pas un
littéral dans ses arguments) : la garde du catalogue les cherche comme
littéraux (`IDENTIFIANTS_CONSTRUITS` dans `tests/gardes.rs`).

## Dialogues bloquants

`confirmations::show` (`app/confirmations.rs`) les peint après tout le reste ;
`AppState::blocking_dialog` (projet non enregistré, dépôt de lignes, alerte,
écrasement, écrasement d'un fichier lu avec des écarts) fait vider les autres actions de l'image
(`blocking_confirmations`). Tous de classe Petite (480 x 170).

| Fenêtre | Nature | Identifiant | Ouverture |
|---|---|---|---|
| Projet non enregistré | `SaveChanges` | `stokesforge_unsaved` | New Project, Open…, Quit, fermeture de la fenêtre (`AppState::unsaved_pending`) |
| Échec d'enregistrement / d'ouverture | `Alert` | `stokesforge_alert` | un enregistrement ou une ouverture qui échoue (`AlertKind::SaveFailed`, `OpenFailed`) |
| R² faible (avertissement), session perdue | `ReadOnly` (Petite) | `stokesforge_alert` | fin d'un calcul dont le R² est faible, si la préférence l'autorise (`AlertKind::LowRSquared`) ; session morte (`AlertKind::SessionLost`) |
| Écraser un fichier | `Confirm` | `stokesforge_overwrite` | export dont l'extension ajoutée vise un fichier existant (`pending_overwrite`) |
| Écraser un fichier lu avec des écarts | `Confirm` | `stokesforge_discarded_save` | Enregistrer qui écraserait le fichier source d'un chargement ayant écarté des calculs ou des traces non encore enregistrés (`pending_discarded_save`, #309) ; pas pour Enregistrer sous ailleurs |
| Axe d'un dépôt de lignes | `Choice` | `stokesforge_row_drop` | glisser des lignes sur un graphe qui a déjà un axe X (`pending_row_drop`) |

Les alertes (échec, R² faible, session perdue) partagent l'identifiant
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
- focus initial : fixé par la nature (S-F11) : le premier champ texte, trouvé
  par la brique, pour les réglages, brouillons, choix et assistants ; le
  bouton par défaut (Cancel, Save, Close) pour une confirmation, un
  enregistrement ou une alerte ; rien pour une lecture seule ;
- zone de message du pied : l'erreur (`AppState::fail` tant qu'un dialogue de
  contenu est ouvert) ou la raison du bouton principal grisé
  (`DialogFooter::gated`, S-F10).

## Anatomie d'une fenêtre neuve

1. **Type et nature.** Choisir dans la table des types : la ligne donne la
   nature (`DialogNature`), donc le pied, le focus et la classe de taille par
   défaut. En cas de doute entre confirmation et choix : une confirmation
   n'existe que pour ce que Ctrl+Z ne rattrape pas (standard S-F3).
   **Taille** : la classe par défaut de la nature, sinon une autre classe ;
   hors classes, une fenêtre interactive demande une taille libre justifiée
   (`// TAILLE-LIBRE(#300):`), une fenêtre sans interaction riche prend la
   taille de son contenu.
2. **Maquette (E2).** La faire valider avant tout code : taille (classe), pages ou
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
9. **Catalogue (E1).** Ajouter la ligne de cette page dans le même commit,
   avec l'identifiant `stokesforge_<nom>` en apostrophes inverses : la garde
   `le_catalogue_des_fenetres_et_le_code_concordent` refuse un identifiant
   absent de la page, ou une ligne sans appel.

### Fenêtre modèle par type

| Type | Fenêtre modèle | Pourquoi celle-là |
|---|---|---|
| Réglages en direct | `plot_settings_window` (`plot_settings.rs`) ; `shape_properties_ui` (`graph_window.rs`) pour le plus petit | brouillon, validation unique, onglets |
| Création sur brouillon | `analysis_window` (`analysis_dialog.rs`) | verbe du bouton, raison du grisage, aperçu |
| Choix | `stale_export_window` (`stale_export.rs`) | secondaires + principal, le plus court |
| Assistant | `import_window` (`import_wizard.rs`) | le seul ; Back · Next · Finish |
| Réglages en direct | `preferences_window.rs` (onglets, réglages immédiats) | Close seul, focus sur le premier champ |
| Lecture seule | `keyboard_shortcuts_window.rs` (le plus simple) | Close seul |
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
