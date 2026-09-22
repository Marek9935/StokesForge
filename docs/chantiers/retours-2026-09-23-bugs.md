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

## B1 — Le contour de surbrillance ne suit pas le bord de la fenêtre

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

## B2 — Aucune surbrillance quand on sélectionne une fenêtre au clic

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

## B3 — Surbrillance mal placée sur une fenêtre repliée

**Capture** : [voir la capture](captures-2026-09-23/surbrillance-fenetre-repliee.png)

Une fenêtre repliée (« Masquer le contenu de… ») garde un contour à sa taille
**dépliée** : un grand cadre vert autour d'une zone vide.

**Cause probable** : même origine que B1 — le contour suit un rectangle de
contenu qui ne correspond plus à ce qui est peint une fois le contenu masqué.
À traiter dans la même passe : **une seule primitive de contour** pour tous les
cas (actif/inactif × déplié/replié × flottant/détaché), plutôt que quatre
rustines.

**Gravité** : cosmétique.

## B4 — Le contenu d'un classeur déborde hors de la fenêtre au rétrécissement

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

**Nature** : résultat visuel faux. **Gravité** : gênant.

**Note** : deux des éléments qui débordent sur la capture (le compteur
« N lignes × M colonnes » et le bouton « Importer… ») disparaissent avec F1 —
une partie du symptôme s'éteindra d'elle-même, mais pas la cause.

## B5 — Icône « Masquer le contenu de… » absente sur les classeurs

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

**Nature** : incohérence entre surfaces (graphe vs classeur).
**Gravité** : gênant.

## B6 — Glisser-déposer colonne → graphe : la légende se met à jour, pas la courbe

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
   débloque la lecture des deux autres.
2. **B2** — dans la foulée, puisqu'il touche la même zone.
3. **B4** — avant F8, dont il est le préalable.
4. **B5** — indépendant, local.
5. **B6** — à investiguer avant de chiffrer ; c'est le seul dont la cause
   n'est pas établie.

Chaque correction commence par un test qui échoue (`plotyx-bug-fix`). Pour
B1 à B4, le test ne peut pas porter sur l'apparence : **les tests couvrent la
géométrie, pas le rendu**. Le contrôle est une capture avant / après —
mécanisme décrit dans
`.claude/skills/plotyx-bug-report/references/voir-l-interface.md`.
