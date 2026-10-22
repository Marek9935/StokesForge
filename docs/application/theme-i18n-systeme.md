# Application : thème, interface multilingue, sélecteur de fichiers

Suite du sommaire [`README.md`](README.md). Contenu : thème, interface multilingue (Fluent), sélecteur de fichiers natif.

### Thème

L'appli **ne suit pas** le thème clair/sombre du système : `stokesforge-app::theme`
pose, une seule fois au démarrage (`main.rs`, dans la fermeture de création
d'`eframe::run_native`, aux côtés du réglage des polices), une palette maison
dérivée de `egui::Visuals::light()` — jamais du sombre. `Context::set_theme`
verrouille la préférence sur `Theme::Light` (empêche egui de re-suivre le
système à la moindre notification `winit`), puis `Context::set_visuals`
applique la palette (`theme::visuals`) : accent vert forêt (sélection,
liens, contours des widgets actifs/survolés), coins légèrement moins arrondis
que le défaut egui. Le reste (gris de fond, contrastes de texte) vient tel
quel du thème clair d'egui, déjà calibré pour la lisibilité.

Deux raisons à ce choix, documentées en tête de `theme.rs` : `stokesforge-render`
dessine les tracés sur un fond clair implicite (axes, grilles, texte noir) —
un thème sombre romprait ce contraste sans qu'aucun code de rendu n'en soit
informé ; et un thème qui change avec le système est un thème qu'on ne peut
pas garantir en capture d'écran ni en support.

**Barres de défilement : un seul standard.** Toutes les barres de
l'application (espace de travail, grille, panneaux, fenêtres d'import et de
script, fenêtres détachées) ont le même aspect : 10 points d'épaisseur,
poignée gris foncé atténuée, qui passe à la couleur de survol puis à l'accent
pendant le glisser. `theme::scroll_style` le définit, `theme::install` le pose
une fois sur le style global d'egui ; changer les constantes `SCROLL_BAR_*` de
`theme.rs` change toutes les barres d'un coup. Règle pour le code futur : ne
jamais construire ni modifier un `ScrollStyle` hors de `theme.rs` ; une zone
qui doit réserver la place d'une barre la lit par
`theme::scroll_bar_allocated_width`. Le test `aucun_scroll_style_hors_du_theme`
fait échouer la CI sinon.

**Icônes.** Phosphor duotone, un seul composant (`icons.rs` : `icon_image`,
`icon_button`, taille unique, info-bulle obligatoire), couleur « neutre par
défaut, rare ensuite » : règles et marche à suivre dans [`icones.md`](../icones.md).
Le test `aucune_icone_hors_du_composant` interdit tout chargement d'icône
ailleurs.

### Interface multilingue (Fluent)

L'interface s'écrit **d'abord en anglais** ; le français en est une traduction
(chantier #39). Seule l'application traduit : Fluent (`i18n-embed`,
`i18n-embed-fl`, `rust-embed`, plus `fluent-syntax` pour la pseudo-langue) est
une dépendance de `stokesforge-app-state` (catalogues, chargeur, `tr!`) et de `stokesforge-app` (la vérification de `fl!` à la compilation) et d'aucun autre crate — `core`, `commands`,
`session` et les autres rendent des données structurées que l'application met
en phrase (#46, #47). Le pourquoi du choix de Fluent (clé et arguments vérifiés
à la compilation) est en tête de `stokesforge-app-state/src/i18n.rs`.

| Où | Quoi |
|---|---|
| `crates/stokesforge-app-state/i18n/en/stokesforge_app.ftl` | le catalogue de référence ; une clé absente d'ici n'existe pas |
| `crates/stokesforge-app-state/i18n/fr/stokesforge_app.ftl` | la traduction française, complète (#49) : même ensemble de clés que `en`, même commit |
| `crates/stokesforge-app-state/i18n.toml`, `crates/stokesforge-app/i18n.toml` | `fallback_language = "en"`, dossier des catalogues ; `fl!` lit l'`i18n.toml` du crate **appelant**, donc les deux fixent `domain = "stokesforge_app"` et celui de l'app pointe le catalogue d'`app-state` |
| `stokesforge-app-state/src/i18n.rs` | chargeur, langue courante, macro `tr!` ; `src/i18n/pseudo.rs` : la pseudo-langue |
| `stokesforge-app-state/src/preferences.rs` | `preferences.json`, même dossier de configuration que `couleurs.json` |
| `stokesforge-import/src/config_dir.rs` | la règle unique du dossier de configuration (`STOKESFORGE_CONFIG_DIR`, #332) |
| `stokesforge-app-state/src/history_text.rs` | les libellés d'historique (`HistoryLabel`) en phrases, clés `history-*` (#46) |
| `stokesforge-app/tests/libelles.rs` | la garde `aucun_libelle_en_dur` (absolue : aucune dérogation), qui lit les sources des deux crates, et les clés mortes |

**`tr!` est l'unique point d'entrée** : `tr!("menu-file")`, ou avec arguments
nommés `tr!("history-insert-rows", count = n)`. Il enveloppe `fl!` ; une clé ou un
argument inconnu de `en` ne compile pas. Aucun littéral dans un libellé. Pour l'infobulle d'un contrôle (attribut `.hint` de sa clé) : `tr!("project-manager-open", "hint")`. Un test
qui compare une langue précise passe son chargeur : `tr!(in loader; "clé")`
(`build_loader`), pour ne pas dépendre de `STOKESFORGE_LANG` ; un test qui cherche un texte peint le demande à `tr!`, jamais un littéral.

**Menus et actions (#42)** : une action a **un** libellé, la clé `action-*`, partagé
par la barre de menus, les menus contextuels, la barre d'état et le bouton de
fenêtre détachée (`action-reopen-last-import`, `action-reset-zoom`…) ; les
menus eux-mêmes sont `menu-*`. L'infobulle (l'effet, pas le raccourci, que le
menu dessine déjà) est l'attribut `.hint` (`tr!("action-save", "hint")`) ;
l'infobulle d'une entrée grisée, qui dit pourquoi, est `.disabled`. Les noms
de touches (`shortcut-backspace`, `shortcut-delete`) sont aussi des clés. 
La barre de menus et la barre d'état sont `StokesForgeApp::menu_bar` et
`StokesForgeApp::status_bar` (`app/menus.rs`, #122) ; leurs tests ouvrent
chaque menu de haut niveau pour de vrai et lisent libellés, grisage et cases
à cocher dans l'arbre d'accessibilité qu'egui publie (AccessKit), sans
supposer de position.

**Langue** : la variable `STOKESFORGE_LANG` (`en`, `fr`, `en-XA`), puis la
préférence `language` de `preferences.json`, sinon l'anglais. La langue du
système n'est **pas** suivie (décision du 2026-10-01). Une valeur inconnue ou
un fichier corrompu passe au niveau suivant, sans panique (invariant 10).
Menu **Fichier › Langue** : change la langue tout de suite (`set_language`, les
textes se redessinent à l'image suivante) et l'écrit dans `preferences.json` ;
`STOKESFORGE_LANG` l'emporte encore au prochain démarrage.

**Dossier de configuration** (#332) : la variable `STOKESFORGE_CONFIG_DIR`,
si elle est posée et non vide, désigne le dossier tel quel (sans suffixe) ;
sinon le dossier du système (`directories::ProjectDirs`, `…\StokesForge\config`
sous Windows). `preferences.json`, `couleurs.json`, `import_recipes.json` et
`import_recent.json` y vivent ; les copies de secours dans
`<STOKESFORGE_CONFIG_DIR>/recovery`, sinon `recovery/` du dossier de données.
Utile pour une installation portable, et c'est ce par quoi les tests
s'isolent du vrai profil (`docs/tests.md`, « Règle (#250) »). Règle et fonctions :
`stokesforge_import::config_dir`.

**Pseudo-langue `en-XA`** : fabriquée à la volée depuis `en` (accents, `[[ ]]`,
+30 %), sans fichier. `STOKESFORGE_LANG=en-XA` : un texte sans `[[ ]]` n'est pas passé
par le catalogue, un `]]` manquant signale un texte tronqué.

**Garde `aucun_libelle_en_dur`** (`tests/libelles.rs`) : aucun littéral textuel
dans `label`, `button`, `menu_button`, `checkbox`, `radio_value`,
`selectable_value`, `selectable_label`, `on_hover_text`, `heading`, ni dans les
`Button::new(…)`/`RichText::new(…)`, ni dans les tables `fn label()`/`fn title()`.
Elle est **absolue** : aucune dérogation (les 213 littéraux d'avant #41 ont été
convertis, #42 à #45, et la liste supprimée à #48). Elle voit aussi `strong`,
`weak`, `small`, `monospace`, `colored_label`, et ignore les champs de format
(`{code}`). Les boîtes de fichier natives sont gardées aussi (#127) :
`set_title` pour le titre, `add_filter` pour le nom visible du filtre ; les
extensions et les sigles de format (`SVG`, `PDF`, `PNG`) restent techniques.
Les titres d'ouverture et d'enregistrement ainsi que le filtre du projet
passent par `tr!`, dans la langue courante au moment d'ouvrir la boîte.
La zone #43 (propriétés du graphe, éditeur de texte, pied des
dialogues, export) reste gardée plus strictement (`ZONE_43` : aucun littéral
textuel dans ses fonctions, quelle que soit la forme de l'appel). Une clé du
catalogue `en` qu'aucun source de `src/` n'emploie fait échouer
`aucune_cle_du_catalogue_n_est_morte` (`tests/libelles.rs`, qui lit `src/` des deux crates). Mécanique détaillée : skill
`stokesforge-textes-interface`.

**Erreurs affichées (#47).** Les crates du dessous rendent des erreurs
structurées dont le `Display` (`#[error(...)]`) est en **anglais brut** — ce que
lisent le CLI, `pyst` et les journaux — et que personne ne traduit. L'application
les met en phrase par `stokesforge-app-state/src/errors.rs` : une fonction `error_message` par
famille (`model` pour `CoreError`, `commands`, `session`, `file` pour
`PersistenceError`, `import`, `formula`, `render`, `export`, et `script` avec la
feature `python`), qui associe chaque variante à une clé `error-*` du catalogue
`en` avec ses champs en arguments. Le `match` est exhaustif, sans `_` : une
variante nouvelle ne compile pas tant qu'elle n'est pas associée (ou renvoyée
volontairement à son `Display` par `errors::fallback`). Le test de chaque
famille construit une valeur de chaque variante et vérifie que la phrase n'est
pas vide, sans jargon interne, sans point final. Toute erreur montrée à
l'utilisateur passe par ces fonctions, jamais par `to_string()` ni `{err}`.
Les champs `detail` (cause décrite par le crate d'origine) et les messages du
système restent en anglais dans la phrase. `stokesforge-importer` rend un
`ImportFailure` structuré (fichier + cause) pour la même raison.

Règle de conduite (chaîne complète, mise en phrase) : `.claude/conventions.md`, section « Erreurs : texte brut anglais en bas, phrase du catalogue dans l'application ».

### Sélecteur de fichiers natif

Le bandeau de fichier a été supprimé : Ouvrir, Enregistrer et Enregistrer
sous sont réunis dans le menu File. Les clics et leurs raccourcis
partagent le même routage. Ouvrir (Ctrl+O) affiche un `rfd::FileDialog` ;
Enregistrer sous (Ctrl+Maj+S) demande une destination ; Enregistrer
(Ctrl+S) utilise la destination courante.

**Fil à part (#192).** Tous les sélecteurs (Open, Save As, export du graphe,
assistant d'import) passent par `Action::PickFile(PickRequest)` et
`file_picker.rs` (dans `stokesforge-app-state`) : `rfd` s'exécute sur un fil dédié, la réponse revient par un
canal sondé à chaque image (`StokesForgeApp::poll_file_picker`). Avant, l'appel
synchrone sur le fil d'egui gelait l'application quand le sélecteur se fermait
sans que le portail ne réponde (`wait_for_response` de `rfd` n'a pas de délai) ;
désormais, au pire, la demande reste en attente. Un seul sélecteur suivi à la fois : une nouvelle demande remplace
l'ancienne, sinon un sélecteur fermé sans réponse du portail bloquerait
Ouvrir ensuite (le fil abandonné reste bloqué dans `rfd`, sans effet).
Garde : `aucun_selecteur_rfd_hors_du_module`. À valider sur un vrai bureau
GNOME/Wayland (clic sur l'application pendant le sélecteur) ; sur macOS, les
sélecteurs hors fil principal restent à vérifier. Le sélecteur n'est pas encore
attaché à la fenêtre (`set_parent`), pas nécessaire au dégel.

`rfd` (feature par défaut `xdg-portal` + `wayland`) compile et se lie sans
portail système présent : le portail (`xdg-desktop-portal`, D-Bus) n'est
qu'une dépendance d'exécution. Vérifié par une compilation `cargo build` et
`cargo build --release` réussies dans un bac à sable sans session graphique
garantie. L'ouverture réelle du sélecteur sur un vrai bureau Wayland reste à
valider interactivement — ce qu'aucun outil de ce dépôt ne peut faire à la
place d'un humain devant un écran : `run-stokesforge` pilote l'application en
X11 (Xwayland), pas en Wayland natif, et n'ouvre pas le sélecteur `rfd`.

