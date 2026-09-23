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

Chaque correction commence par un test qui échoue (`plotyx-bug-fix`). Pour
B1 à B4, le test ne peut pas porter sur l'apparence : **les tests couvrent la
géométrie, pas le rendu**. Le contrôle est une capture avant / après —
mécanisme décrit dans
`.claude/skills/plotyx-bug-report/references/voir-l-interface.md`.
