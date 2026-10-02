# Dépôt Wayland — transition #72

Cette copie est **winit 0.30.13**, sous sa licence Apache-2.0 (`LICENSE`).
Le `[patch.crates-io]` racine la sélectionne aussi pour eframe/egui-winit
0.36.2. Elle reste hors des membres du workspace.

Source : paquet crates.io `winit` 0.30.13, SHA-256 du paquet amont
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`. Les fichiers de production amont
sont conservés, sauf les modifications explicitement recensées ci-dessous.
Les fichiers générés du cache Cargo ne font pas partie de cette copie.

## Récepteur partagé : correction du premier prototype

Le premier prototype ajoutait un `wl_data_device` dans winit, qui n'en
possédait pas. C'était insuffisant : egui-winit crée ensuite
`smithay-clipboard` **sur le même wl_display**, avec sa propre file. Sur
Mutter, les offres n'étaient envoyées qu'au récepteur du presse-papiers,
dont les handlers de dépôt amont sont vides. Le problème a été reproduit
par un dépôt réel depuis Fichiers : `text/uri-list`, `enter`, `motion` et
`leave` sur le récepteur du presse-papiers, aucune acceptation ni réception.
Le second récepteur du ticket était donc bien un risque réel.

Le correctif utilise maintenant **le récepteur existant du presse-papiers**.
`vendor/smithay-clipboard` est la copie 0.7.3 sous licence MIT, avec le dépôt
ajouté à ses handlers. Un seul worker et un seul récepteur par siège sont
partagés entre les viewports d'egui. Les requêtes/réponses de collage sont
sérialisées ; le copier-coller et la sélection primaire restent disponibles.
La dernière vue ferme le worker avant que le display emprunté soit détruit.

winit ne crée plus aucun `wl_data_device`. Un canal calloop reçoit les
événements du worker existant : identifiant de surface, position logique et
chemins. La boucle winit retrouve la fenêtre et son échelle puis émet les
événements habituels. Aucun objet Wayland ni pointeur emprunté ne traverse
le canal ; le callback possède une référence faible et devient inactif à
l'arrêt de la boucle. Aucun nouveau fil n'est créé.

Estimations de développement et coûts de validation, avant validation réelle
du dépôt (ordres de grandeur, pas des mesures) :

| Option | Développement | Mémoire et maintenance | Détection des défauts |
|---|---|---|---|
| Module externe sur pointeur emprunté | 1–2 jours avec fenêtres détachées | fil et file supplémentaires ; gestion de durée de vie et correspondance surface/viewport | élevée : concurrence, fermeture et plusieurs fenêtres |
| Récepteur partagé presse-papiers/winit, retenu | environ une journée avec tests et doc | deux patches de versions déjà compilées ; un récepteur par siège et un worker pour toutes les vues | élevée pour le protocole ; parseur, canal et durée de vie couverts automatiquement |
| Backport du backend 0.31 | plusieurs jours | adaptation des API événementielles remaniées ; diff plus large | élevée : tous les événements de fenêtre |
| Sélecteur XDG seul | quelques heures | composant `rfd` déjà présent | faible, mais ne permet pas de déposer un fichier |
| X11 conservé par défaut | quelques minutes | aucune transition | faible pour le dépôt ; conserve la latence constatée sous XWayland |
| Migration immédiate vers winit 0.31 bêta | plusieurs jours au moins | migration coordonnée d'eframe/egui-winit | très élevée : ensemble de l'interface |

État amont consulté le 2026-10-02 :
[winit 0.31.0-beta.3](https://github.com/rust-windowing/winit/releases),
[egui-winit](https://github.com/emilk/egui/blob/main/crates/egui-winit/Cargo.toml).
Vérifier ces sources de nouveau avant de migrer.

## Diff de production à relire

winit : `src/platform/wayland.rs`, `src/platform_impl/linux/wayland/mod.rs`,
`state.rs`, `event_loop/mod.rs` et le nouveau `file_drop_bridge.rs`.
Le manifeste et les handlers de siège sont ceux d'amont.

smithay-clipboard : `Cargo.toml` (winit et url, déjà présents dans le graphe),
`src/lib.rs` (worker partagé), `worker.rs` (callback), `state.rs` (raccordement),
les nouveaux `file_drop.rs` et `file_drop_uri.rs`. Les manifestes d'origine
restent les témoins amont. Source crates.io 0.7.3, SHA-256 :
`71704c03f739f7745053bde45fa203a46c58d25bc5c4efba1d9a60e9dba81226`.

Le parseur et le canal exacts sont aussi compilés par les tests ordinaires
`stokesforge-app/tests/wayland_uri.rs` et `wayland_bridge.rs`.
Les deux copies restent hors workspace ; `rustfmt` ciblé doit compléter
`cargo fmt`. Aucun autre code amont n'est modifié.

Contrôles du patch, depuis la racine (Linux) :

```sh
cargo test -p stokesforge-app --test wayland_uri --test wayland_bridge
cargo clippy -p winit -p smithay-clipboard --lib -- -D warnings
rustfmt --edition 2021 --config-path /dev/null --config skip_children=true --check \
  vendor/winit/src/platform/wayland.rs \
  vendor/winit/src/platform_impl/linux/wayland/{state,file_drop_bridge}.rs \
  vendor/winit/src/platform_impl/linux/wayland/event_loop/mod.rs
rustfmt --edition 2024 --config-path /dev/null --config skip_children=true --check \
  vendor/smithay-clipboard/src/{file_drop,file_drop_uri,lib,state,worker}.rs
```

Le fichier de configuration amont de smithay-clipboard contient des options
nightly ; `/dev/null` choisit le formatage stable pour les fichiers modifiés.
`skip_children=true` laisse les modules amont inchangés.

## Interface et limites

Le pont n'émet que `CursorMoved` (position du dépôt), `HoveredFile`,
`HoveredFileCancelled` et `DroppedFile`, avec l'identité native de la fenêtre.
egui-winit les traduit en `hovered_files`/`dropped_files` comme sous X11.
L'application conserve `Action::DropFiles` et son import partagé.

`text/uri-list` et l'action **Copy** sont acceptés. Aucune suppression de
fichier source n'est demandée. Les URI `file:` locales sont décodées par
`url`, avec commentaires, CRLF, espaces échappés et dédoublonnage. Les URI
distantes, relatives, à fragment/requête, ou contenant NUL sont rejetées.
Un marqueur de survol à chemin vide indique l'offre avant réception ; il ne
devient jamais un dépôt. Les chemins sont demandés seulement après `drop`.

La lecture du pipe passe par calloop : une lecture par notification,
**1 Mio maximum de liste d'URI par dépôt**, délai maximum **10 secondes**.
Ces limites portent sur les noms, pas sur la taille des fichiers importés.
Une source lente n'immobilise donc pas l'interface. Le siège, la fenêtre et
l'offre doivent rester valides ; une erreur est journalisée. La position
du dépôt est réémise quand les chemins arrivent pour conserver sa destination.
Le support tactile et les compositeurs autres que ceux testés restent à vérifier.

## Vérification manuelle obligatoire

Sur **GNOME/Mutter**, **KDE/KWin**, puis un compositeur **wlroots**, lancer :

```sh
cargo run -p stokesforge-app
STOKESFORGE_X11=1 cargo run -p stokesforge-app
```

Pour chaque backend, déposer depuis le gestionnaire de fichiers :

1. Un CSV, puis plusieurs fichiers avec espaces, accents et `%` dans leurs
   noms : survol visible, une feuille par fichier, une entrée d'import
   annulable, sans suppression à la source.
2. Sur un classeur flottant précis, puis hors des classeurs : bonne
   destination ; bouger le pointeur juste après avoir lâché le fichier.
3. Sur un classeur détaché, puis sur un graphe et le Project Manager
   détachés : même décision de destination que sous X11.
4. Sortir de la fenêtre sans déposer, puis déposer une nouvelle fois :
   aucun marqueur de survol persistant, aucun fichier importé à l'annulation.
5. Déposer un `.stf` : ouverture par le second processus existant.
6. Essayer avec les propriétés d'un graphe ouvertes et avec l'assistant
   d'import ouvert : refus ou ajout aux fichiers selon les règles actuelles.
7. Fermer une fenêtre pendant le survol, puis l'application après un dépôt :
   pas d'erreur de protocole ni de fil survivant.
8. Répéter les dépôts, dont une offre non fichier ; tester deux sièges si
   disponibles. Aucun dépôt ne doit être doublé.

Le **démarrage/arrêt natif GNOME**, le secours **X11/XWayland** et le défaut
du premier prototype ont été observés. Après correction, le dépôt depuis
Fichiers en Wayland natif sur GNOME/Mutter est confirmé par l'utilisateur
le 2026-10-02. Les autres cas de la matrice, notamment les fenêtres détachées,
KDE/KWin et wlroots, restent à valider ; les tests Rust ne les remplacent pas.
X11 reste disponible en secours.

## Retrait

Quand la version stable d'eframe choisie emploie winit avec dépôt Wayland :

1. Monter eframe/egui-winit/egui ensemble, puis adapter le transport de dépôt
   si l'API de winit 0.31 diffère. Garder `Action::DropFiles`.
2. Retirer les entrées `[patch.crates-io]`, les exclusions `vendor/winit` et
   `vendor/smithay-clipboard`, les deux copies, les tests `wayland_uri.rs` et
   `wayland_bridge.rs` et leurs dev-dépendances `url`/`calloop`.
3. Régénérer `Cargo.lock` ; vérifier qu'une seule version de winit est utilisée.
4. Réexaminer `configure_window_backend` : le choix natif doit rester celui
   de winit ; conserver le secours `STOKESFORGE_X11=1` s'il reste utile.
5. Rejouer la matrice manuelle et les tests du workspace, puis actualiser
   `docs/application.md`, `.codex/README.md`, `AGENTS.md` et `CLAUDE.md`.
