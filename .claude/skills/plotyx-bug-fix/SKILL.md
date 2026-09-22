---
name: plotyx-bug-fix
description: Corriger un bug dans PlotYx en partant d'une reproduction qui échoue, puis en réparant à la bonne couche sans affaiblir un test ni casser un invariant. À utiliser dès que la demande est « corrige », « répare », « ça devrait faire X », « enlève ce comportement », ou après un rapport produit par plotyx-bug-report. Couvre aussi la levée d'un `#[ignore]` de reproduction dans plotyx-robustesse et le choix de l'endroit où poser le test de non-régression.
---

# Corriger un bug dans PlotYx

L'ordre compte : **reproduire, puis corriger**. Un correctif écrit avant la
reproduction ne sait pas ce qu'il corrige, et le dépôt a déjà une convention
pour tenir un bug ouvert — un test.

## 1. Reproduire avant de corriger

1. Si le bug n'est pas encore diagnostiqué, passer par **plotyx-bug-report**
   (reproduction minimale, faux positifs écartés, couche localisée). Les trois
   familles de faux positifs — plafond arbitré, limite assumée du banc,
   dégradation volontaire de l'invariant 10 — y sont listées ; corriger l'une
   d'elles est une régression, pas un correctif.
2. Écrire un test qui **échoue pour la bonne raison**. Le voir échouer est la
   seule preuve qu'il éprouve bien le symptôme. Un test qui passe déjà avant
   correction ne teste pas le bug.
3. Si une reproduction `#[ignore]` existe déjà dans `plotyx-robustesse`, c'est
   elle le point de départ : `cargo test -p plotyx-robustesse -- --ignored`.

## 2. Corriger à la bonne couche

Le réflexe coûteux est de réparer là où le symptôme se voit. Table de renvoi :

| Symptôme | Où la cause vit le plus souvent |
|---|---|
| valeur fausse dans la grille | `plotyx-core` (colonnes, conversion) ou la `Command` qui l'a écrite, pas `app.rs` |
| annulation qui ne rend pas l'état d'avant | `plotyx-commands` : le delta calculé à l'application, ou la nature structurelle/données de la commande |
| état perdu après enregistrement + rechargement | `plotyx-persistence` (`convert.rs`, `schema.rs`) — un champ ajouté au modèle sans transport |
| écran ≠ export SVG/PDF/PNG | mesure de texte (invariant 4) ou fonctionnalité écrite dans un seul backend (invariant 5) — voir **plotyx-rendu** |
| figure qui ne se met pas à jour | clé de cache `(revision, rect, pixels_per_point, view_generation)` de `plotyx-render-egui` |
| action grisée à tort, libellé faux, menu incohérent | `AppState` (`grid_menu`, `undo_label`…), jamais le code egui |
| comportement présent dans le dock, absent en fenêtre détachée | deux chemins qui auraient dû être un : chercher la fonction partagée (`tab_contents`, `state::*_in`) |
| blocage de l'interface | quelque chose de bloquant appelé depuis la boucle de rendu (cf. `ScriptEngine::run`, résolu par un fil détaché + `poll_script`) |

Deux règles de conduite qui orientent la forme du correctif :

- **`AppState::dispatch` ne rend jamais d'erreur.** Toute erreur devient un
  `StatusMessage` affiché. Corriger un bug en faisant remonter une erreur
  d'interface casse cette règle ; la nuance « impossible pour l'instant »
  (annuler à vide) n'est d'ailleurs pas une erreur.
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
  Un bug causé dans `plotyx-core` et vu dans l'UI se teste dans `plotyx-core` ;
  ajouter *en plus* un test d'interface n'a de sens que si le câblage lui-même
  était fautif.
- Si le bug venait d'un désaccord entre deux surfaces (écran/export,
  dock/détaché, menu/raccourci), le test le plus utile compare les deux plutôt
  que de vérifier chacune séparément.

## 5. Vérifier et rendre compte

Enchaîner sur **plotyx-verification** : `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, plus les features hors défaut si le correctif les
touche (`--all-features` pour `typst`, `scripts/python-test.sh` pour `python`).

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
- [ ] test de reproduction écrit et **vu échouer** avant correction
- [ ] correction posée à la couche de la cause, pas du symptôme
- [ ] aucun test affaibli ; garde-fous d'architecture toujours verts
- [ ] `#[ignore]` de reproduction retiré s'il y en avait un
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts (+ features touchées)
- [ ] `//!` et page `docs/` à jour si le comportement décrit a changé
- [ ] compte rendu : ce qui a changé, ce qui a été exécuté, ce qui reste non validé
