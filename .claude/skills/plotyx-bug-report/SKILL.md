---
name: plotyx-bug-report
description: Rédiger un rapport de bug exploitable pour PlotYx — reproduction minimale, comportement attendu vs observé, localisation dans le bon crate, invariant potentiellement touché, et test de reproduction `#[ignore]` prêt à poser. À utiliser dès que l'utilisateur signale « ça plante », « ça ne marche pas », « c'est bizarre », « la valeur est fausse », « la fenêtre disparaît », ou décrit un comportement surprenant — même vaguement, même sans demander de correction. Ce skill **diagnostique et documente, il ne corrige pas** : pour corriger, utiliser plotyx-bug-fix.
---

# Signaler un bug dans PlotYx

Un rapport utile est un rapport qu'on peut rejouer. L'objectif de ce skill est
d'arriver à **une reproduction minimale** et à **la bonne couche**, avant que
quiconque touche au code.

## 1. D'abord : est-ce vraiment un bug ?

Trois familles de faux positifs reviennent constamment dans ce dépôt. Les
écarter **avant** d'écrire le rapport, sinon on fait corriger une décision.

### Un plafond arbitré

Longueur de nom, `MAX_NODE_ID` (10¹²), `MAX_CELLS`, nombre de classes
d'histogramme, limites d'historique… Ces bornes sont documentées à l'endroit de
leur constante et recensées dans
`docs/archive/rustrewrite/questions-ouvertes.md` §11. Un refus à la borne
n'est pas un bug.

### Une limite assumée du banc

`plotyx-app` est explicitement un banc d'intégration, pas l'interface finale.
Sont **voulus**, écrits dans `docs/application.md` :

- une seule cellule éditable à la fois (grille virtualisée : pas de tampon par
  cellule sur une feuille d'un million de lignes) ; un onglet détaché a son
  propre tampon ;
- Ctrl+Z pendant une saisie annule la dernière **commande du modèle**, pas la
  dernière frappe ;
- les fenêtres ouvertes et leur mode ne sont pas restaurés au démarrage (seules
  position et taille le sont) ;
- l'éditeur de matrice est un espace réservé ;
- pas de sélecteur de couleur graphique, pas de multi-panneaux dans le panneau
  de configuration des graphes.

### Une dégradation volontaire (invariant 10)

« Un défaut de donnée dégrade, il ne plante pas. » Une série grise annotée
« référence rompue », une variante d'énumération inconnue conservée telle
quelle, une formule Typst affichée en texte brut, une cellule vide affichée
comme telle : c'est le comportement **attendu**, pas une panne. Voir le tableau
de `docs/invariants.md` §10.

Si le doute persiste, le dire dans le rapport plutôt que de trancher seul :
« comportement possiblement voulu, à confirmer » est une information, pas une
faiblesse.

## 2. Réduire

- **Reproduire à la main** avant tout : quels gestes exactement, dans quel
  ordre, sur quel fichier. Un rapport sans séquence rejouable ne vaut rien.
- **Minimiser** : retirer un geste à la fois jusqu'à ce que le symptôme
  disparaisse. Le dernier geste retiré fait partie de la cause.
- **Localiser la couche** avec le tableau de routage de `.claude/rules.md`.
  Un symptôme visuel n'est pas forcément un bug de rendu : une valeur fausse à
  l'écran peut venir du modèle, de la commande, de la résolution de série ou du
  tracé — quatre crates différents.
- **Distinguer la nature de l'écart** : mauvais résultat, panique, blocage,
  lenteur, ou incohérence entre deux surfaces (écran vs export, dock vs fenêtre
  détachée, menu vs raccourci). La dernière famille est la plus fréquente ici
  et la plus révélatrice : elle pointe presque toujours vers deux chemins qui
  auraient dû être un seul.

Si le bug touche au **rendu**, poser la question qui tranche : l'écran et
l'export SVG montrent-ils la **même** chose ? Un désaccord entre les deux
oriente immédiatement vers la mesure de texte (invariant 4), le piège le plus
coûteux du dépôt parce qu'il est silencieux.

## 3. Format du rapport

Répondre avec exactement cette structure :

```markdown
## <symptôme en une phrase, du point de vue de l'utilisateur>

**Reproduction** (depuis un projet neuf, sauf mention contraire)
1. …
2. …

**Attendu** : …
**Observé** : …

**Portée** : <crate(s) et fichier(s) suspects, avec `chemin:ligne` si trouvé>
**Nature** : résultat faux | panique | blocage | lenteur | incohérence entre surfaces
**Invariant potentiellement touché** : <n° et nom, ou « aucun »>
**Écarté** : <plafond / limite assumée / dégradation volontaire vérifiés et écartés>
**Gravité** : perte de données | bloquant | gênant | cosmétique
```

Ajouter, seulement si l'enquête l'a établi :

- **Cause probable**, avec le `chemin:ligne` qui la porte et pourquoi ce code
  produit le symptôme. Ne pas spéculer : « non localisé » est préférable à une
  hypothèse séduisante et fausse.
- **Contournement** pour l'utilisateur en attendant la correction.

## 4. Le test de reproduction — l'idiome du dépôt

PlotYx n'a pas de gestionnaire de tickets : **c'est un test qui tient un bug
ouvert**. Le dépôt a une convention précise, héritée de la campagne de
robustesse :

- une **reproduction** échoue délibérément et porte `#[ignore]`, avec son motif
  écrit en toutes lettres dans l'attribut — pour que le pipeline garde sa
  valeur de signal (un CI rouge en permanence ne dit plus rien) ;
- le jour où elle **passe**, c'est le signal attendu : le correctif est bon, on
  retire `#[ignore]` et le garde-fou prend le relais.

Proposer donc un test, et le placer au bon endroit :

| Portée du bug | Où poser le test |
|---|---|
| comportement local d'une fonction | `#[cfg(test)]` dans le `src/` concerné |
| contrat public ou frontière d'un crate | `crates/<crate>/tests/` |
| le socle poussé aux limites | la batterie `bN` correspondante de `plotyx-robustesse` (index dans son `src/lib.rs`) |
| logique d'interface | `crates/plotyx-app/tests/shell.rs` (sans fenêtre) |

Écrire le test et ses commentaires en **français**.

Un symptôme qui n'apparaît qu'avec une vraie fenêtre (Wayland, sélecteur de
fichiers natif, multi-écran) n'est pas testable en CI : le dire explicitement
dans le rapport plutôt que de fabriquer un test qui n'éprouve rien.

## 5. Ne pas corriger ici

Ce skill s'arrête au rapport. Enchaîner sur **plotyx-bug-fix** seulement si
l'utilisateur demande la correction — un diagnostic posé sans hâte vaut mieux
qu'un correctif au mauvais endroit, et certains bugs de ce dépôt ont une
question de conception à trancher avant (par exemple : que doit faire l'undo
après cette correction ?).
