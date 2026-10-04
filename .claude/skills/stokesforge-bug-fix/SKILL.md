---
name: stokesforge-bug-fix
description: Corriger un bug dans StokesForge en partant d'une reproduction qui échoue, puis en réparant à la bonne couche sans affaiblir un test ni casser un invariant. À utiliser dès que la demande est « corrige », « répare », « ça devrait faire X », « enlève ce comportement », ou après un rapport produit par stokesforge-bug-report. Couvre aussi la levée d'un `#[ignore]` de reproduction dans stokesforge-robustesse et le choix de l'endroit où poser le test de non-régression.
---

# Corriger un bug dans StokesForge

L'ordre compte : **reproduire, puis corriger**. Un correctif écrit avant la
reproduction ne sait pas ce qu'il corrige, et le dépôt a déjà une convention
pour tenir un bug ouvert — un test.

## 1. Reproduire avant de corriger

1. Si le bug n'est pas encore diagnostiqué, passer par **stokesforge-bug-report**
   (reproduction minimale, faux positifs écartés, couche localisée). Les trois
   familles de faux positifs — plafond arbitré, limite assumée du banc,
   dégradation volontaire de l'invariant 10 — y sont listées ; corriger l'une
   d'elles est une régression, pas un correctif.
2. Écrire un test qui **échoue pour la bonne raison**. Le voir échouer est la
   seule preuve qu'il éprouve bien le symptôme. Un test qui passe déjà avant
   correction ne teste pas le bug.
3. Si une reproduction `#[ignore]` existe déjà dans `stokesforge-robustesse`, c'est
   elle le point de départ : `cargo test -p stokesforge-robustesse -- --ignored`.

### Ce que « reproduire » veut dire pour un bug visuel

Position, contour, alignement, troncature, surbrillance : les tests couvrent la
géométrie, **pas l'apparence**. Reproduire, ici, c'est *regarder* — sinon on
corrige une cause déduite d'une lecture de code, et une cause déduite peut être
fausse tout en étant convaincante. `stokesforge-app` sait se photographier :
[`../stokesforge-bug-report/references/voir-l-interface.md`](../stokesforge-bug-report/references/voir-l-interface.md).
Garder la capture d'avant : elle sert de preuve au moment de montrer l'après.

### « Jamais câblé » ≠ « câblé et cassé »

Avant de conclure qu'un comportement manque, chercher si le code porte déjà
l'intention (`rg "Action::|Command::|fn <nom>" crates/stokesforge-app/src/`). Si une
`Action` ou une `Command` existe déjà pour ce geste, le bug est **une condition
qui échoue**, pas du code à écrire — et on cherche alors pourquoi elle ne se
déclenche pas. Les deux diagnostics envoient à des endroits opposés du fichier.

## 2. Corriger à la bonne couche

Le réflexe coûteux est de réparer là où le symptôme se voit. Table de renvoi :

| Symptôme | Où la cause vit le plus souvent |
|---|---|
| valeur fausse dans la grille | `stokesforge-core` (colonnes, conversion) ou la `Command` qui l'a écrite, pas `app.rs` |
| annulation qui ne rend pas l'état d'avant | `stokesforge-commands` : le delta calculé à l'application, ou la nature structurelle/données de la commande |
| état perdu après enregistrement + rechargement | `stokesforge-persistence` (`convert.rs`, `schema.rs`) — un champ ajouté au modèle sans transport |
| écran ≠ export SVG/PDF/PNG | mesure de texte (invariant 4) ou fonctionnalité écrite dans un seul backend (invariant 5) — voir **stokesforge-rendu** |
| figure qui ne se met pas à jour | clé de cache `(revision, rect, pixels_per_point, view_generation)` de `stokesforge-render-egui` |
| fenêtre détachée à jour seulement au survol | publication (`snapshot_stamp` dans `app.rs`, `sync_window_views` et `show_detached_tabs` dans `app/window_detach.rs`) : `show_viewport_deferred` ne redessine pas en recevant un nouvel instantané. Comparer époque, révision et révision des géométries, puis `request_repaint_of` du lecteur ; une commande appliquée après peinture doit programmer l'image racine de publication (#104). Tester aussi le retour au repos, sans rafraîchissement périodique |
| action grisée à tort, libellé faux, menu incohérent | `AppState` (`grid_menu`, `undo_label`…), jamais le code egui |
| comportement présent en flottant, absent en fenêtre détachée | deux écritures là où il en faudrait une (`rg "fn detached_" crates/stokesforge-app/src/`), ou un état d'`AppState` jamais recopié dans la vue détachée (`fork_for_detached_view`, `sync_detached_snapshot`, `adopt_detached_book_view`). Factoriser si la contrainte `'static` de `show_viewport_deferred` ne s'y oppose pas, sinon corriger **les deux** |
| blocage de l'interface | quelque chose de bloquant appelé depuis la boucle de rendu (cf. `ScriptEngine::run`, résolu par un fil détaché + `poll_script`) |

**Si le correctif demande un arbitrage, demander.** Un bug dont la correction
casse un geste existant, ou dont le « bon » comportement se discute (que doit
faire l'undo après ? le menu doit-il rester identique dans les deux zones ?),
n'est pas une question technique : c'est une décision de produit.
`AskUserQuestion`, avec des options formulées en **comportements observables**
— l'utilisateur n'est pas développeur, il arbitre ce qu'il voit, pas une
structure de code.

Deux règles de conduite qui orientent la forme du correctif :

- **`AppState::dispatch` ne rend jamais d'erreur.** Toute erreur devient un
  `StatusMessage` affiché. Corriger un bug en faisant remonter une erreur
  d'interface casse cette règle ; la nuance « impossible pour l'instant »
  (annuler à vide) n'est d'ailleurs pas une erreur.
- **Un message d'erreur faux ou ambigu se corrige dans le catalogue** : la
  phrase vit dans `crates/stokesforge-app/i18n/en/stokesforge_app.ftl` (clé `error-*`),
  associée à sa variante dans `crates/stokesforge-app/src/errors.rs`. Le `Display`
  anglais du crate d'origine (`#[error(...)]`) ne se modifie que s'il est lui-même
  faux — il est lu par le CLI et `pyst`. Une variante d'erreur **nouvelle** s'associe
  dans `errors.rs` (le `match` exhaustif le force) avec un exemple dans le test de
  sa famille ; jamais de `to_string()` d'erreur dans l'interface.
- **Un défaut de donnée dégrade, il ne plante pas** (invariant 10). Le
  correctif d'un cas de donnée invalide est un état de première classe visible
  et localisé, pas une erreur qui remonte ni une série silencieusement vide.

## 3. Ne pas affaiblir le filet

C'est la faute la plus grave possible dans ce dépôt, parce qu'elle ne se voit
pas dans le diff du correctif.

- **Ne jamais assouplir un test pour le faire passer.** Certains tests ne
  vérifient pas un comportement mais une propriété d'architecture
  (`no_ui_dependency.rs`, `frontiere.rs`, `mesure.rs`, `accord.rs`,
  `feathering.rs`, `script_bridge_lazy.rs`, `memory_pinning.rs`). S'ils
  échouent, c'est qu'une frontière vient d'être franchie — le correctif est à
  refaire, pas le test.
- **Les témoins des tests de rendu** (`mesure.rs`, `accord.rs`) sont des cas
  qui doivent rester **différents**. Un test d'égalité passe aussi bien parce
  que les deux chemins sont justes que parce qu'ils sont faux de la même façon.
  Avant de toucher à ces fichiers, lire leurs témoins.
- Une nouvelle dépendance ajoutée au `Cargo.toml` d'un crate du socle doit être
  ajoutée explicitement à la liste blanche de son `no_ui_dependency.rs` : la
  décision devient consciente au lieu de se glisser par un `use`.

## 4. Verrouiller la correction

- **La reproduction devient le garde-fou.** Si le test portait `#[ignore]`,
  le retirer maintenant : c'est exactement le signal que la convention attend.
- Poser le test de non-régression au niveau de la **cause**, pas du symptôme.
  Un bug causé dans `stokesforge-core` et vu dans l'UI se teste dans `stokesforge-core` ;
  ajouter *en plus* un test d'interface n'a de sens que si le câblage lui-même
  était fautif.
- Si le bug venait d'un désaccord entre deux surfaces (écran/export,
  dock/détaché, menu/raccourci), le test le plus utile compare les deux plutôt
  que de vérifier chacune séparément.

### Fermer le bug là où il est suivi

Un bug corrigé dont le statut reste « ouvert » quelque part sera rapporté comme
ouvert au prochain récap — c'est arrivé (B6). Donc, **dans le même geste que le
commit du correctif** :

- le message de commit porte `Closes #<n>` (l'issue GitLab se ferme au push sur
  la branche par défaut) ; si le push n'est pas fait ou pas sur `main`, fermer
  explicitement : `glab issue close <n>` avec un commentaire citant le commit ;
- pas d'issue ? Rien à fermer — mais ne pas créer de `.md`
  pour l'occasion.

## 5. Vérifier et rendre compte

Enchaîner sur **stokesforge-verification** : `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, plus la feature hors défaut si le correctif la
touche (`scripts/python-test.sh` pour `python` ; Typst est couvert par
`cargo test --workspace`).

Mettre à jour la doc si le correctif change un comportement décrit :
le `//!` du module (le *pourquoi*, y compris « l'hypothèse précédente était
fausse pour telle raison » — c'est précisément ce qu'un futur lecteur cherchera)
et la page `docs/` du domaine.

Rendre compte honnêtement : ce qui a changé, **les contrôles réellement
exécutés**, et les limites de validation. Un bug dont la vérification exige une
vraie fenêtre (Wayland, sélecteur de fichiers natif, multi-écran) n'est pas
validé par un test vert : le dire.

## Checklist

- [ ] faux positifs écartés (plafond / limite assumée / dégradation voulue)
- [ ] bug visuel : **regardé** (capture avant), pas seulement lu dans le code
- [ ] « câblé et cassé » vs « jamais câblé » tranché par un `rg` avant de coder
- [ ] arbitrage renvoyé à l'utilisateur si le correctif casse un geste existant
- [ ] test de reproduction écrit et **vu échouer** avant correction
- [ ] correction posée à la couche de la cause, pas du symptôme
- [ ] aucun test affaibli ; garde-fous d'architecture toujours verts
- [ ] `#[ignore]` de reproduction retiré s'il y en avait un
- [ ] issue GitLab fermée (`Closes #n` ou `glab issue close`)
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts (+ features touchées)
- [ ] `//!` et page `docs/` à jour si le comportement décrit a changé
- [ ] compte rendu : ce qui a changé, ce qui a été exécuté, ce qui reste non validé
