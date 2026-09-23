# Rapport de bug — retours du 2026-09-23

Six défauts, tirés d'un vrac de seize retours trié le 2026-09-23. Les
évolutions demandées dans le même message vivent à côté, dans
[`retours-2026-09-23-features.md`](retours-2026-09-23-features.md).

Toutes les localisations pointent `crates/plotyx-app/src/app.rs` et ont été
vérifiées à la lecture du code. **Les quatre symptômes visuels ont été
confirmés à l'écran** (captures dans `captures-2026-09-23/`) — pas déduits du
code seul : sur ce chantier précis, une cause déduite d'une lecture s'est déjà
révélée fausse.

## Ce qui n'est pas dans ce rapport, et pourquoi

Deux comportements signalés ne sont **pas** des bugs, vérification faite —
les corriger par réflexe serait une régression :

- **Tab dans le corps fonctionne.** Sur la dernière colonne d'une ligne de
  données, Tab ajoute bien une colonne. Confirmé par l'utilisateur.
- **Tab dans l'en-tête est limité à `F(x)=` délibérément.** Le commentaire
  `app.rs:4455-4467` l'écrit : « seul Formule marque la vraie fin de
  l'en-tête » ; les trois autres champs font circuler Tab de colonne en
  colonne pour pouvoir atteindre le champ suivant. L'utilisateur renverse ce
  choix — c'est donc une évolution (F3), pas une correction.

---

## B1 — Le contour de surbrillance ne suit pas le bord de la fenêtre [corrigé]

**Correctif (révisé)** : la surbrillance ne peint plus un second contour —
elle recolore le trait du `Frame` de la fenêtre lui-même
(`floating_window_frame`, `crates/plotyx-app/src/app.rs`), qui suit déjà par
construction le rectangle réellement peint (marge comprise, repli compris).
Une première version avait peint un contour séparé après `.show()`, sur le
rectangle lu via `ctx.memory(|m| m.area_rect(id))` plutôt que sur
`ui.max_rect()` dans la fermeture — fonctionnel, mais une primitive de plus
que nécessaire, écartée au profit de celle-ci.

**Étendu aux fenêtres détachées** : les fenêtres détachées (viewports OS
séparés, sans décoration) n'avaient elles-mêmes aucun contour du tout — pas
un cas de B1/B3 à proprement parler (rien n'y était mal placé, l'absence
était juste jamais comblée), mais la même correction logique s'y applique.
`active_tab`/`floating_tabs` ne couvrent pas ces fenêtres (chacune est son
propre viewport OS) : « active » y prend donc le sens du focus OS
(`RawInput::focused`, rapporté par le backend par viewport) plutôt que du
z-order interne. Le trait recoloré est partagé via `active_window_stroke`,
appliqué au `Frame` du `CentralPanel` de `show_detached_tabs` plutôt qu'à
celui d'`egui::Window`.

**Capture** : [voir la capture](captures-2026-09-23/surbrillance-mal-placee.png)

**Reproduction**
1. Ouvrir un classeur en fenêtre flottante.
2. Le rendre actif (double-clic depuis le Project Manager).

**Attendu** : le contour longe le bord visible de la fenêtre.
**Observé** : il est en retrait sur les quatre côtés, avec un écart plus large
en bas — il entoure le **contenu**, pas la fenêtre.

**Cause établie** : `app.rs:1931-1936`, le contour est peint **à l'intérieur**
de la fermeture `.show()`, sur `ui.max_rect()` — c'est-à-dire le rectangle de
contenu, déjà rétréci par la marge intérieure du `Frame` de la fenêtre. Le
cadre peint par egui, lui, est à l'extérieur de cette marge.

**Piste de correction** : le rectangle réel est déjà lu **quinze lignes plus
bas**, après `.show()`, pour enregistrer la géométrie —
`ui.ctx().memory(|memory| memory.area_rect(id))` (`app.rs:1978`). Peindre le
contour là, ou styler le `Frame` de la fenêtre, plutôt que dans la fermeture.

**Nature** : résultat visuel faux. **Gravité** : cosmétique.
**Écarté** : ni plafond, ni limite documentée, ni dégradation volontaire.

## B2 — Aucune surbrillance quand on sélectionne une fenêtre au clic [corrigé]

**Correctif** : `egui::Area` amène déjà la fenêtre cliquée au premier plan de
son propre ordre de calques (`Sense::CLICK`, indépendamment de tout code
applicatif), mais `floating_tabs` — seule source d'`active_tab`, donc de la
surbrillance — ne le savait pas. Après la boucle des fenêtres flottantes (une
fois sa mutation de nouveau sûre), `app.rs` relit `ctx.top_layer_id()`, y
retrouve le tab correspondant, et l'appelle à
`sync_floating_tabs_order_to_top_layer` pour ramener ce tab en fin de
`floating_tabs` — même position que celle que produit déjà le double-clic du
Project Manager. Voir `crates/plotyx-app/src/app.rs`, juste après la boucle
des fenêtres flottantes, et la fonction `sync_floating_tabs_order_to_top_layer`.

**Capture** : [voir la capture](captures-2026-09-23/pas-de-surbrillance-au-clic.png)

**Reproduction**
1. Ouvrir deux fenêtres flottantes.
2. Cliquer directement sur l'une d'elles (sans passer par le Project Manager).

**Attendu** : elle passe au premier plan **et** se surligne.
**Observé** : elle passe au premier plan, sans surbrillance.

**Cause établie** : `app.rs:1910`, `is_active = Some(tab) == active_tab`.
D'après le commentaire `app.rs:1927-1931`, `active_tab` est alimenté par
`floating_tabs.last()` et le double-clic du Project Manager. Un clic direct
dans la fenêtre ne le met pas à jour — **la logique manque**, elle n'échoue
pas (`rg "is_active"` ne montre aucune autre écriture).

**Nature** : incohérence entre deux chemins de sélection (Project Manager vs
clic direct). **Gravité** : gênant.

## B3 — Surbrillance mal placée sur une fenêtre repliée [corrigé]

**Capture** : [voir la capture](captures-2026-09-23/surbrillance-fenetre-repliee.png)

Une fenêtre repliée (« Masquer le contenu de… ») garde un contour à sa taille
**dépliée** : un grand cadre vert autour d'une zone vide.

**Cause confirmée** : même origine que B1 — le contour suivait un rectangle
de contenu qui ne correspond plus à ce qui est peint une fois le contenu
masqué. Corrigé dans la même passe que B1, avec la même primitive
(`floating_window_frame`, qui suit le rectangle réellement peint).

**Correctif complémentaire** : la taille **demandée** à egui pendant un
repli (`.fixed_size`) se réduit désormais à la hauteur de la bande
d'en-tête — sinon le cadre du `Frame` et les poignées de redimensionnement
maison restaient positionnés à la taille dépliée, même une fois le contour
corrigé. La taille logique (`current_size`, persistée dans
`window_geometries`/`.plx`) reste inchangée : seule la taille demandée cette
image se réduit, pour que déplier une fenêtre après rechargement du projet
retrouve sa taille d'avant repli. Voir `floating_window_display_size` et
`floating_window_geometry_to_persist` dans `crates/plotyx-app/src/app.rs`.

**Gravité** : cosmétique.

## B4 — Le contenu d'un classeur déborde hors de la fenêtre au rétrécissement [corrigé]

**Capture** : [voir la capture](captures-2026-09-23/debordement-resize.png) — le bandeau, les
colonnes et la rangée d'onglets de feuille sont peints **au-delà** du bord
droit de la fenêtre.

**Reproduction**
1. Ouvrir un classeur d'au moins trois colonnes.
2. Rétrécir la fenêtre par sa poignée droite.

**Attendu** : ce qui dépasse est rogné au bord de la fenêtre.
**Observé** : le contenu continue d'être peint par-dessus l'espace de travail.

**Cause établie** : la fenêtre est posée avec `.fixed_size(current_size)`
(`app.rs:1921`) — le redimensionnement natif d'egui est remplacé par
`floating_resize_handles`. Une taille forcée **ne rogne pas** le contenu : un
`Ui` dont les enfants dépassent son `max_rect` les peint quand même. Rien, dans
le chemin du classeur, ne réduit le rectangle de découpe ni n'offre de
défilement horizontal.

**Piste de correction** : rogner explicitement le contenu de la fenêtre au
rectangle demandé, puis ajouter le défilement horizontal (voir F8 — sans
rognage, le glisseur n'aurait rien à faire défiler).

**Correctif** : dans la boucle des fenêtres flottantes (`app.rs`, juste après
le calcul de `display_size`), le contenu de l'onglet
(`tab_contents`/`book_contents` : bandeau, grille, onglets de feuille) est
maintenant peint dans le rectangle de découpe (`ui.clip_rect().intersect(
ui.max_rect())`) plutôt que sans aucune limite. Ce qui dépasse est rogné au
bord de la fenêtre, sans défilement horizontal — F8 reste une évolution
séparée. Non couvert par ce correctif : la fenêtre détachée (viewport OS
natif, qui clippe déjà) et le mode maximisé (`CentralPanel`, qui clippe déjà
via egui). Test de non-régression :
`le_contenu_qui_deborde_est_rogne_au_rectangle_de_la_fenetre` dans
`crates/plotyx-app/src/app.rs`.

**Nature** : résultat visuel faux. **Gravité** : gênant.

**Note** : deux des éléments qui débordent sur la capture (le compteur
« N lignes × M colonnes » et le bouton « Importer… ») disparaissent avec F1 —
une partie du symptôme s'éteindra d'elle-même, mais pas la cause.

## B5 — Icône « Masquer le contenu de… » absente sur les classeurs [corrigé]

**Cause certaine**, `app.rs:2487-2492` :

```rust
let collapse_width = if matches!(tab, Tab::Graph(_)) {
    COLLAPSE_WIDTH
} else {
    0.0
};
```

`window_header` réserve l'icône aux `Tab::Graph`, quel que soit le
`HeaderPlacement`. **Aucun commentaire ne justifie cette restriction** —
contrairement au cas de Tab dans l'en-tête, c'est une extension jamais faite,
pas un choix.

**À vérifier avant de lever la restriction** : le repli lui-même
(`toggle_graph_collapse`, `collapsed_graph_tabs: HashSet<Tab>`,
`app.rs:2126`) est générique sur `Tab`, mais la peinture du contenu replié n'a
été éprouvée que pour un graphe. Lever l'icône sans vérifier ce chemin
donnerait un classeur replié à l'affichage douteux.

**Correctif** : la condition dans `window_header` inclut désormais
`Tab::Book` (restriction levée pour les classeurs, mais volontairement pas
pour `Matrix`/`Note`, faute de raison de le faire). `toggle_graph_collapse`/
`toggle_graph_collapse_to` et `collapsed_graph_tabs` renommés en
`toggle_collapse`/`toggle_collapse_to`/`collapsed_tabs` puisqu'ils ne sont
plus propres aux graphes. Vérifié avant de lever la restriction : la peinture
du contenu replié (fenêtres flottantes et détachées) était déjà générique sur
`tab`, rien de spécifique au graphe n'y était caché.

**Nature** : incohérence entre surfaces (graphe vs classeur).
**Gravité** : gênant.

## B6 — Glisser-déposer colonne → graphe : la légende se met à jour, pas la courbe [investigué, non corrigé]

**Investigation menée** : la piste ci-dessous (revision qui n'avance pas,
cache périmé) a été **falsifiée** par un test reproduisant le chemin réel
(dispatch → `AppState::add_series_to_graph` → `Command::SetPlotSpec` →
`execute`) : la révision avance bien, le cache de `plotyx-render-egui` est
bien invalidé, et la courbe est bien reconstruite avec la nouvelle série.
Légende et courbe sont produites par le **même** calcul mis en cache
(`GraphView::paint` → `collect_shapes` → `pipeline::draw` puis
`legend::draw`) : il n'existe structurellement aucun second chemin qui
dessinerait la légende séparément, donc les deux ne peuvent pas diverger par
ce mécanisme.

Aucune cause alternative n'a pu être démontrée aussi solidement — pas de
correctif livré plutôt que de deviner. Pour trancher, il manque soit une
capture d'écran prise **après** un vrai glisser-déposer (hors de portée du
mécanisme de capture actuel, qui ne photographie que l'état initial), soit
une clarification : la « légende » signalée est-elle celle dessinée dans le
graphe, ou la liste « Séries » du panneau de configuration à droite (dont le
brouillon `PlotEdit` est volontairement non rafraîchi tant qu'il porte sur le
même graphe — comportement inverse de celui décrit, à vérifier auprès de
l'utilisateur) ?

**Reproduction**
1. Ouvrir un classeur et un graphe.
2. Sélectionner une colonne numérique, la glisser sur le graphe.

**Attendu** : la série apparaît dans la légende **et** son tracé est visible.
**Observé** : seule la légende se met à jour.

**Nature** : incohérence entre deux surfaces qui auraient dû partager un seul
chemin — la famille de symptôme la plus révélatrice de ce dépôt.

**Portée** : non localisée. Point d'entrée : `graph_drop_target` /
`drop_payload_action`.

**Piste à éprouver en premier** : si l'ajout de série ne fait pas avancer
`revision`, le cache de formes de `plotyx-render-egui` (clé
`(revision, rect, pixels_per_point, view_generation)`, voir `docs/rendu.md`)
rend l'ancien tracé, pendant que la légende — recalculée hors cache — voit
bien la nouvelle série. Cohérent avec le symptôme observé.

**Gravité** : gênant (le glisser perd sa seule utilité visible).

---

## Retours du 2026-09-23, suite — fenêtres flottantes/détachées

Quatre captures supplémentaires de l'utilisateur, après la livraison de
B1-B6 et d'un correctif de surbrillance étendu aux fenêtres détachées
(commits `7f273b3` et `2b12b64` sur `rewriterust`). Tableau de tri d'abord,
détail ensuite pour les points non évidents.

| Remarque de l'utilisateur | Classement | Lot |
|---|---|---|
| « reduit l'entete, on peu ce caler sur la hauteur grisé par le sélecteur v » | évolution | reportée dans `retours-2026-09-23-features.md` (F10) |
| « il y a toujours un bug dans le resize de fenêtre » (fenêtre flottante, captures 1/2) | bug, précisé par l'utilisateur | **B7** |
| « en détacher il y a un bug lors de la réduction de la fenêtre » (capture 3 : zone vide) | bug | **B9** |
| « le liseré de bordure disparaît » (fenêtre détachée rétrécie, capture 4) | bug, même cause que B7 | **B7** |
| « le slider horizontal n'est pas présent » (fenêtre détachée, capture 4) | bug (angle mort d'un correctif existant) | **B8** |

Les captures elles-mêmes ne sont pas jointes au dépôt (fournies en pièce
jointe de la conversation, pas de fichier local) — les descriptions
ci-dessous en tiennent lieu, vérifiées à la relecture pixel par pixel des
images.

## B7 — Le contour de surbrillance suit l'étendue du contenu, pas la taille réelle de la fenêtre

**Reproduction**
1. Ouvrir un classeur en fenêtre flottante (ou en fenêtre détachée).
2. Le rétrécir en largeur, jusqu'à ce que la bande d'outils (« Ajouter une
   colonne » / « Importer… » / « N lignes × M colonnes », ou le même bandeau
   côté détaché) ne tienne plus entièrement.

**Attendu** : le contour de surbrillance longe le bord réellement visible de
la fenêtre — celui où s'arrêtent les icônes d'en-tête (`[] ↗ ×`).
**Observé**, deux manifestations du même défaut :
- **Fenêtre flottante** : le contour **déborde vers l'extérieur**, bien
  au-delà des icônes d'en-tête et du contenu visiblement rogné (B4). Sur la
  capture fournie, les icônes se regroupent vers x≈450-570 (sur une image de
  770px de large) alors que le contour de droite reste à x≈760 — un écart
  d'environ 190px de fond vide entre les deux.
- **Fenêtre détachée** : à l'inverse, le contour **disparaît** sur les bords
  droit et bas — seuls le côté gauche et le haut restent visibles. Sur la
  capture, le texte « Note » de l'en-tête de colonne est tronqué à mi-glyphe
  exactement au bord de la fenêtre, sans aucun contour à cet endroit.

**Cause établie** — un seul mécanisme pour les deux. Les chemins de fichier
ci-dessous sont ceux du crate `egui` 0.36.2 lui-même (dans le cache Cargo,
`~/.cargo/registry/src/…/egui-0.36.2/src/…`), pas de ce dépôt :
- Le contour d'une fenêtre flottante est le trait du `Frame` d'`egui::Window`
  (`floating_window_frame`, `crates/plotyx-app/src/app.rs`). Sa géométrie
  n'est **pas** celle demandée par `.fixed_size(display_size)` :
  `egui::Window` construit toujours son `Resize` interne avec
  `.with_stroke(false)` (`containers/window.rs:111-112`), donc la branche de
  `Resize::end` qui fait suivre la taille du contenu réel s'applique
  **toujours** — `size[d] = state.last_content_size[d]`
  (`containers/resize.rs:362`) — quelle que soit la taille fixée. Un
  `ui.horizontal(…)` (la bande d'outils) qui ne s'enroule pas continue
  d'étendre le `min_rect` logique de la fenêtre même quand son rendu est
  visuellement rogné par B4 : le rognage cache le dépassement à l'écran, il
  ne l'empêche pas de compter dans la géométrie que `Frame::paint` utilise
  pour peindre le contour (`content_ui.min_rect()`, `containers/frame.rs:474`).
- Côté fenêtre détachée, le `Frame` du `CentralPanel` obéit au même
  mécanisme (`content_ui.min_rect()`), mais `CentralPanel::show_inside_dyn`
  pose `panel_ui.set_clip_rect(outer_rect)` (`containers/panel.rs:1241`,
  viewport OS réel) **avant** de peindre le `Frame` : la portion du contour
  qui déborde de `outer_rect` (à cause du même dépassement logique de
  contenu) est donc purement et simplement rognée à l'affichage — d'où sa
  disparition sur les bords qui dépassent, plutôt qu'un débordement visible
  comme côté flottant.

**Piste de correction** : ne pas dépendre du `Frame` auto-dimensionné pour
peindre le contour. Revenir à un contour peint explicitement sur le
rectangle **demandé**, pas sur un rectangle qu'egui recalcule depuis le
contenu réellement peint :
- flottante : `egui::Rect::from_min_size(current_pos, display_size)` — le
  même `requested_rect` déjà calculé juste après pour
  `floating_resize_handles` ;
- détachée : `ui.ctx().input(|input| input.content_rect())` — le même
  rectangle déjà utilisé par `detached_resize_handles`
  (`app.rs:2528`).

C'est très proche de la toute première version du correctif B1 (peindre
après `.show()`, sur un rectangle connu) — écartée à l'époque pour « une
primitive de moins », mais qui s'avère être la seule robuste au débordement
de contenu. Revenir à un `Frame` sans trait particulier
(`.stroke(Stroke::NONE)` ou simplement ne pas appeler `.frame(...)`, pour
laisser le trait par défaut d'egui gérer le cas inactif) et peindre le
contour actif par-dessus, comme B1 à l'origine — en gardant cette fois la
leçon de B1/B3 sur *où* lire le bon rectangle.

**Nature** : incohérence entre le rectangle utilisé pour le contour et celui
réellement visible. **Gravité** : cosmétique côté flottant (débordement dans
le vide), plus gênant côté détaché (le contour, censé montrer que la
fenêtre est active, disparaît justement quand elle est la plus étroite).

## B8 — Le rognage du contenu (B4) n'a jamais été porté aux fenêtres détachées [corrigé]

**Reproduction**
1. Détacher un classeur d'au moins trois colonnes.
2. Rétrécir la fenêtre OS en largeur (poignées invisibles de
   `detached_resize_handles`, ou redimensionnement natif si le gestionnaire
   de fenêtres l'autorise).

**Attendu** : au minimum, le même rognage propre qu'en fenêtre flottante
(B4) — dans l'idéal, à terme, le même défilement horizontal que F8 prévoit
pour le classeur flottant.
**Observé** : ni l'un ni l'autre. Le contenu (bandeau d'outils, en-têtes de
colonne, grille) déborde du viewport OS sans aucun rognage contrôlé — sur la
capture, l'en-tête de colonne « Note » est coupé à mi-glyphe, pas au niveau
d'un caractère entier, signe qu'aucun code applicatif ne gère ce cas : c'est
la fenêtre OS elle-même qui tronque au pixel, faute de mieux.

**Cause établie** : `detached_book_contents`/`detached_grid_contents`
(`crates/plotyx-app/src/app.rs`) ne posent aucun `ui.set_clip_rect(...)`,
contrairement à la boucle des fenêtres flottantes depuis B4
(`ui.clip_rect().intersect(ui.max_rect())`, juste avant l'appel à
`tab_contents`). Doublon connu du dépôt (dock vs détaché, voir
`book_contents`/`detached_book_contents`) : B4 a corrigé une seule des deux
écritures.

**Portée** : `crates/plotyx-app/src/app.rs`, fonction `show_detached_tabs`
et `detached_book_contents`/`detached_grid_contents` (à vérifier aussi pour
`detached_matrix_contents`/`detached_note_contents`, même famille de
symptôme probable).
**Nature** : incohérence entre surfaces (flottante vs détachée), même famille
que B1-B6. **Gravité** : gênant.
**Écarté** : ni plafond, ni limite assumée du banc, ni dégradation
volontaire — c'est un angle mort de B4, pas un choix.

**Correctif** : un seul point d'insertion, dans `PlotyxApp::show_detached_tabs`
(`crates/plotyx-app/src/app.rs`), juste après la vérification du repli et
juste avant le `match tab { Tab::Book | Tab::Graph | Tab::Matrix | Tab::Note }`
qui aiguille vers `detached_book_contents`/`graph_view_contents`/
`detached_matrix_contents`/`detached_note_contents` — même geste que B4
(`ui.clip_rect().intersect(ui.max_rect())` puis `ui.set_clip_rect(...)`), posé
une fois la taille du `CentralPanel` (donc celle du viewport OS) connue.
Couvre les **quatre natures d'onglet** : elles partagent toutes ce point
d'aiguillage, donc un seul appel suffit — pas besoin d'un appel par fonction
de contenu. `graph_view_contents` gardait déjà son propre sous-rognage
(`figure_ui.set_clip_rect`, pour la figure elle-même) ; le nouveau rognage
s'ajoute au niveau du panneau et couvre en plus ses contrôles (bouton
« réinitialiser le zoom »).

Pas de défilement horizontal ajouté (ça reste F8, `retours-2026-09-23-
features.md`) — ici, comme B4, on rogne, on ne fait pas défiler.

Test de non-régression :
`app::graph_layout_tests::le_classeur_detache_est_rogne_au_rectangle_du_viewport`
(`crates/plotyx-app/src/app.rs`), calqué sur le scaffolding de
`un_graphe_detache_reconstruit_ses_formes_dans_son_viewport` (détachement réel
d'un classeur à dix colonnes, callback de viewport publié par
`show_detached_tabs`, rejoué dans un `screen_rect` volontairement plus étroit
que le contenu). Nuance à noter : le backend borne de toute façon le tampon au
viewport OS, donc même sans le correctif aucune forme n'a un `clip_rect` qui
déborde `window_rect` — ce n'est pas ce que ce test mesure. Sans le correctif,
tout le contenu (bandeau, en-têtes de colonne, onglet de feuille) hérite du
rognage *par défaut* du `CentralPanel` : le rectangle plein du viewport, marge
du `Frame` comprise. Avec le correctif, il hérite du rectangle de contenu
réellement disponible, strictement plus petit. Le test verrouille cette
différence (rognage strictement plus petit que `window_rect`, pas égal à lui)
plutôt qu'un débordement au sens propre — géométriquement plus fin que la
capture du rapport, mais c'est la même cause : l'absence de
`ui.set_clip_rect(...)` sur ce chemin.

**Non validé** : le mécanisme de capture (`PLOTYX_APP_SCREENSHOT`/
`PLOTYX_APP_AUTOQUIT_MS`) ne photographie que la fenêtre principale, pas un
viewport détaché (`show_viewport_deferred`) — voir la doc de tête de
`PlotyxApp::screenshot`. Le rendu réel d'une fenêtre détachée rétrécie
manuellement (l'apparence, pas la géométrie) reste donc non vérifié par un
outil non interactif de ce dépôt.

## B9 — Une fenêtre détachée repliée garde sa taille OS dépliée

**Reproduction**
1. Détacher un graphe (ou un classeur).
2. Cliquer sur le chevron de repli (« Masquer le contenu de… ») dans la
   fenêtre détachée.

**Attendu** : la fenêtre OS se réduit à la hauteur de sa bande d'en-tête,
comme une fenêtre flottante repliée (B3).
**Observé** : le contenu est bien masqué (`tab_contents` sauté), mais la
fenêtre OS garde sa taille dépliée — une grande zone vide, entourée du
contour de surbrillance (B1/B3-détaché, qui lui fonctionne correctement ici
puisque rien ne déborde).

**Cause établie** : le correctif B3 réduit la taille **demandée à egui**
pendant un repli (`floating_window_display_size`, utilisée par
`.fixed_size(display_size)` dans la boucle des fenêtres flottantes) — mais
`show_detached_tabs` n'a pas d'équivalent. Une fenêtre détachée est un
viewport OS réel : rien n'y envoie de
`ViewportCommand::InnerSize`/`ViewportCommand::MinInnerSize` quand
`header_actions.toggle_collapse` bascule `model.collapsed`
(`crates/plotyx-app/src/app.rs`, autour de la fin de la fermeture de
`show_viewport_deferred`). Son `CentralPanel` remplit toujours l'intégralité
du viewport OS, quelle que soit la quantité de contenu peint à l'intérieur
(contrairement à `egui::Window`, voir B7 : ici c'est la taille de la
**fenêtre OS elle-même**, pas de l'`Area` interne à egui, qui ne bouge pas).

**Piste de correction** : envoyer un `ViewportCommand::InnerSize` (largeur
inchangée, hauteur ramenée à celle de la bande d'en-tête — la même valeur
que `floating_window_display_size` calcule déjà) au moment du bascule, et
un second pour revenir à la taille mémorisée au dépli. Vérifier le
comportement de `detached_resize_handles` pendant qu'une fenêtre est
repliée (probablement à désactiver, comme les poignées maison des fenêtres
flottantes ne s'activent pas d'intérêt sur un repli).

**Portée** : `crates/plotyx-app/src/app.rs`, `PlotyxApp::show_detached_tabs`.
**Nature** : incohérence entre surfaces (flottante vs détachée), même famille
que B5 (comportement jamais porté d'un côté). **Gravité** : gênant.
**Écarté** : ni plafond, ni limite assumée, ni dégradation volontaire.

---

## Ordre suggéré

1. **B1 + B3** ensemble — une seule primitive de contour, vérifiée sur les
   quatre combinaisons d'états. C'est la correction la moins risquée et elle
   débloque la lecture des deux autres. **Corrigé** (voir les deux sections
   ci-dessus).
2. **B2** — dans la foulée, puisqu'il touche la même zone. **Corrigé**.
3. **B4** — avant F8, dont il est le préalable. **Corrigé**.
4. **B5** — indépendant, local. **Corrigé**.
5. **B6** — à investiguer avant de chiffrer ; c'est le seul dont la cause
   n'est pas établie. **Investigué, non corrigé** — la piste s'est révélée
   fausse, voir la section ci-dessus pour ce qu'il faut clarifier avant de
   rouvrir ce chantier.
6. **B7** — priorité haute : casse la lisibilité même du correctif B1/B3 tout
   juste livré, sur les deux surfaces (flottante et détachée). Indépendant de
   B8/B9 (cause différente), mais touche la même zone de code : à corriger
   avant ou avec eux pour éviter un troisième passage sur ce fichier.
7. **B8** — indépendant, porte le même correctif que B4 vers le chemin
   détaché ; vérifier au passage `detached_matrix_contents`/
   `detached_note_contents`.
8. **B9** — indépendant, porte le même correctif que B3 vers le chemin
   détaché (taille demandée, pas primitive de contour).

Chaque correction commence par un test qui échoue (`plotyx-bug-fix`). Pour
B1 à B4 et B7 à B9, le test ne peut pas porter sur l'apparence : **les tests
couvrent la géométrie, pas le rendu**. Le contrôle est une capture avant /
après — mécanisme décrit dans
`.claude/skills/plotyx-bug-report/references/voir-l-interface.md`.
