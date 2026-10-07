---
name: stokesforge-bug-report
description: >-
  Trier et documenter ce que l'utilisateur remonte sur StokesForge — un symptôme isolé comme un vrac de quinze remarques mêlant correctifs et demandes d'évolution. Produit une reproduction minimale, le classement bug/évolution, la localisation dans le bon crate, et les questions à trancher. À utiliser pour créer ou compléter des issues, ou dès que l'utilisateur signale « ça plante », « ça ne marche pas », « c'est bizarre », « la valeur est fausse », « le cadre est mal placé », « il manque… », ou envoie une liste de retours en vrac — même vaguement, même sans demander de correction. Ce skill diagnostique et documente ; le contexte de signalement persiste jusqu'à une demande explicite d'implémentation dans le dépôt.
---

# Trier et documenter ce qui est remonté

Un rapport utile est un rapport qu'on peut rejouer, et dont chaque ligne est
soit une cause localisée, soit une question posée. L'erreur qui coûte le plus
cher ici n'est pas de rater un bug : c'est de **classer avec assurance ce qui
demandait une question**.

## Avant tout : conserver le contexte de signalement

Quand la conversation porte sur des rapports ou sur la création et
l'enrichissement d'issues, **rester dans ce contexte entre les messages**.
Une capture supplémentaire ou une formulation impérative (« ajoute »,
« corrige », « enlève », « supprime cette icône ») décrit alors le changement
à consigner dans l'issue ; elle n'autorise pas une modification de l'application,
même évidente ou limitée à une ligne.

Créer ou compléter l'issue concernée, en conservant les retours précédents.
La lecture du code pour localiser le problème reste possible ; ne pas modifier
le code, les tests ou la documentation de l'état implémenté, ni lancer une
correction opportuniste pendant ce travail.

Passer à **stokesforge-bug-fix** ou **stokesforge-feature** seulement si
l'utilisateur demande explicitement de réaliser le changement dans le dépôt
(par exemple « implémente #152 maintenant » ou « corrige ce bug dans le code »).
La validation du contenu d'une issue ne constitue pas cette demande. Si la
transition reste ambiguë, continuer le travail sur l'issue ; demander une
clarification seulement si elle est nécessaire pour poursuivre.

## Un menu qui paraît faux : le comparer à la grammaire

Un séparateur en trop ou orphelin, un sous-menu à une entrée, une entrée
grisée sans info-bulle, un Undo au clic droit, un « Properties ▸ », une
action qui ne s'applique pas à la cible visée : **lire
[`docs/menus.md`](../../../docs/menus.md)** et signaler le menu contre la règle
qu'il viole (nommer la règle dans l'issue). La brique de sections
(`app/menu_sections.rs`) et sa garde (`aucun_separateur_brut_dans_un_menu`)
empêchent déjà les traits à la main ; si la grammaire elle-même paraît
mauvaise, c'est une question à poser à l'utilisateur, pas un bug.

## 0. Regarder, si c'est visuel

Position, contour, alignement, troncature, chevauchement, surbrillance,
taille, ombre, rafraîchissement : **ne jamais conclure depuis le code seul**.
Un raisonnement juste sur un code mal compris produit une cause fausse et
convaincante — c'est arrivé, et la capture de l'utilisateur disait l'inverse.

`stokesforge-app` sait se photographier (`STOKESFORGE_APP_SCREENSHOT` +
`STOKESFORGE_APP_AUTOQUIT_MS`). Procédure, conversion PPM → PNG, et ce que la
capture ne peut pas montrer :
[`references/voir-l-interface.md`](references/voir-l-interface.md).

Quand l'état en cause n'est pas capturable sans interaction (glisser, repli,
fenêtre détachée), **demander la capture à l'utilisateur** au lieu de deviner.

## 1. Si c'est un vrac : trier d'abord, détailler ensuite

Un message qui contient dix à vingt remarques ne se traite pas remarque par
remarque en déroulant le gabarit complet — ça produit un document que personne
ne relit, et ça coûte du temps sur les points évidents.

Ordre de travail :

1. **Un tableau de tri**, une ligne par remarque : intitulé repris de
   l'utilisateur → `bug` / `évolution` / `à trancher` → lot.
2. **Regrouper** par cause ou par zone de code commune, pas par ordre d'arrivée.
3. **Ne déplier le gabarit complet** (§3) que pour les bugs **non évidents** :
   ceux dont la cause n'est pas déjà nommée dans la remarque elle-même.
4. **Les demandes d'évolution ne se conçoivent pas ici.** Les reporter dans une
   todo en reformulant fidèlement la demande de l'utilisateur, et s'arrêter là.
   Pas d'ordre de livraison inventé, pas de choix d'implémentation, pas de
   découpage en sous-tâches techniques : ce travail appartient à
   **stokesforge-feature**, une fois la demande précisée. La seule valeur ajoutée
   attendue ici est de **signaler ce qui manque de précision** pour pouvoir
   l'implémenter.

## 2. Est-ce vraiment un bug ? — et la nuance qui change tout

Trois familles de faux positifs reviennent constamment. Les écarter **avant**
d'écrire le rapport, sinon on fait corriger une décision.

### Un plafond arbitré

Longueur de nom, `MAX_NODE_ID` (10¹²), `MAX_CELLS`, nombre de classes
d'histogramme, limites d'historique : documentés à l'endroit de leur constante
et recensés dans `docs/archive/rustrewrite/questions-ouvertes.md` §11. Un refus
à la borne n'est pas un bug.

### Une limite assumée du banc

`stokesforge-app` reste décrit comme un banc d'intégration, mais ses capacités
évoluent. Vérifier chaque limite dans `docs/application.md` **et dans le
code** : l'ancienne absence de restauration des fenêtres ou de sélecteur de
couleur ne doit plus servir à écarter un signalement. Ne pas recopier ici une
liste figée de fonctions absentes.

### Une dégradation volontaire (invariant 10)

Une série grise annotée « référence rompue », une variante d'énumération
inconnue conservée telle quelle, une formule Typst affichée en texte brut :
comportement **attendu**. Voir `docs/invariants.md` §10.

### ⚠️ La nuance : une décision documentée ferme **une** option, pas toutes

C'est l'erreur à ne pas refaire. Trouver un commentaire qui dit « ce choix est
volontaire » **ne suffit pas** à classer la demande en évolution. Il faut lire
*quelle alternative* a été rejetée, et *pourquoi*.

Cas réel : la zone de prise du glisser-déposer est confinée au bord de la
grille, et le commentaire (`grid_contents`, `src/app/grid.rs`, près de
`COLUMN_GRIP_WIDTH` ; `app.rs:4308-4319` à l'époque) explique que couvrir **tout**
`grid_rect` écraserait le glisser d'extension de sélection. L'utilisateur
demandait une bande autour de **la sélection** — une troisième option, que le
commentaire n'examine pas, et qui reste possible puisqu'une cellule fait plus
du double de `COLUMN_GRIP_WIDTH`. La classer « évolution, comportement
délibéré » était aussi faux que la classer « bug » sans rien lire.

Donc : quand la demande de l'utilisateur **diffère de l'option rejetée**,
ne pas trancher. Poser la question (§4).

## 3. Réduire et localiser

- **Reproduire** : quels gestes, dans quel ordre, sur quel fichier. Un rapport
  sans séquence rejouable ne vaut rien.
- **Minimiser** : retirer un geste à la fois jusqu'à ce que le symptôme
  disparaisse. Le dernier geste retiré fait partie de la cause.
- **Localiser la couche** avec `.claude/rules.md`. Un symptôme visuel n'est pas
  forcément un bug de rendu : une valeur fausse à l'écran peut venir du modèle,
  de la commande, de la résolution de série ou du tracé.

### « Jamais câblé » ≠ « câblé et cassé »

Distinction la plus rentable du diagnostic, et la plus souvent ratée. Avant
d'écrire « le mécanisme n'existe pas », chercher si le code porte déjà
l'intention :

```sh
rg "AddColumnAndSelect|Action::|Command::" crates/stokesforge-app/src/
```

Si une `Action`, une `Command` ou une fonction porte déjà le comportement
attendu, alors le symptôme n'est pas « à implémenter » mais **« ça existe et ça
ne se déclenche pas »** — et on cherche alors *la condition qui échoue*, pas du
code à écrire. Les deux diagnostics envoient à des endroits opposés.

Réciproquement : une entrée de menu à ajouter peut cacher un câblage entier à
créer. `Command::DeleteColumn` existe dans `stokesforge-commands` mais
`rg DeleteColumn crates/stokesforge-app/src/` ne renvoie rien — « ajouter Supprimer
au menu » est donc une `Action` + un `dispatch` + un test, pas une ligne d'UI.

### Chercher le doublon docké / détaché

Piège structurel de `stokesforge-app` : un contenu a longtemps existé **en deux
écritures**, une pour la fenêtre flottante et une pour la fenêtre détachée
(`book_contents` / `detached_book_contents`…). Classeur, graphe, note et matrice sont
désormais unifiés ; un doublon réintroduit se trouve par
`rg "fn detached_" crates/stokesforge-app/src/`. Un rapport qui ne cite qu'une des
deux écritures fait livrer un correctif à moitié.

### Un message d'erreur faux ou ambigu

Le texte d'une erreur affichée vient du catalogue (`i18n/en/stokesforge_app.ftl`, clé
`error-*`) et de l'association variante → clé de `crates/stokesforge-app/src/errors.rs` ;
le texte brut anglais du crate d'origine (`#[error(...)]`) n'est lu que par le CLI,
`pyst` et les journaux. Localiser le message **par sa clé**, pas par le littéral, et
dire si c'est la phrase du catalogue, l'association, ou le `detail` brut du crate
d'origine qui est en cause.

### La famille de symptôme la plus révélatrice

Un désaccord **entre deux surfaces** — écran vs export, dock vs détaché, menu
vs raccourci, légende vs tracé — pointe presque toujours vers deux chemins qui
auraient dû n'en faire qu'un. C'est la piste à suivre en premier quand elle
s'applique.

## 4. Demander plutôt que trancher

Utiliser `AskUserQuestion` dès qu'un de ces cas se présente, **avant** de
figer le classement ou le rapport :

| Situation | Pourquoi demander |
|---|---|
| la demande diffère de l'option rejetée par un commentaire | ni bug ni évolution : un arbitrage que seul l'utilisateur peut faire |
| le symptôme peut être un bug **ou** une décision de produit | les deux sont défendables ; choisir seul, c'est décider à sa place |
| la demande est claire mais son **périmètre** ne l'est pas (« revoir toute la logique de X ») | l'écart entre un correctif d'une heure et une refonte se joue là |
| corriger casserait un geste existant | l'utilisateur doit savoir ce qu'il perd |
| le symptôme n'est pas reproductible sans capture ou sans son fichier | deviner produit un rapport faux |

Formuler la question avec ce qu'on a déjà établi, options chiffrées quand
c'est possible — coût, risque, ce que ça casse — jamais une question ouverte
qui renvoie le travail à l'utilisateur. Il n'est pas développeur : lui
demander de choisir entre deux comportements observables, oui ; entre deux
implémentations, non.

Une seule question groupée vaut mieux que cinq questions séparées.

## 5. Format du rapport

Le tableau de tri (§1) d'abord. Puis, **pour les seuls bugs non évidents** :

```markdown
## <symptôme en une phrase, du point de vue de l'utilisateur>

**Reproduction** (depuis un projet neuf, sauf mention contraire)
1. …

**Attendu** : …
**Observé** : …

**Portée** : <crate(s) et `chemin:ligne` vérifiés — les deux écritures si doublon>
**Nature** : résultat faux | panique | blocage | lenteur | incohérence entre surfaces
**Écarté** : <plafond / limite assumée / dégradation volontaire vérifiés>
**Gravité** : perte de données | bloquant | gênant | cosmétique
```

Ajouter **Cause probable** seulement si l'enquête l'a établie, avec le
`chemin:ligne` qui la porte. « Non localisé » est préférable à une hypothèse
séduisante et fausse — et pour un symptôme visuel, une cause avancée **sans
avoir regardé la fenêtre** n'a pas sa place dans le rapport.

Mentionner l'invariant potentiellement touché **seulement** s'il y en a un.

## 6. Où vit le bug : une issue GitLab, pas un `.md`

**L'état d'un bug (ouvert / corrigé) vit à un seul endroit : les issues GitLab
du projet** (`origin` = `gitlab.com:askrzypczak/stokesforge`), via `glab`. Plus de
nouveau rapport dans `docs/chantiers/*.md` : un fichier figé à sa date
d'écriture ne sait pas qu'un commit l'a rendu faux — c'est arrivé, B6 était
listé « non corrigé » alors qu'il l'était.

- **Créer** : `glab issue create --title "<symptôme>" --label bug
  --description "<gabarit §5>"` (label `évolution` pour une demande). Une
  issue par bug ; un vrac se trie d'abord (§1), puis chaque ligne `bug`
  devient une issue.
- **Lister les bugs ouverts** (« fais-moi un récap ») : `glab issue list
  --label bug`, jamais une note `.md`. Un bug qu'on croit ouvert sans issue se
  vérifie (`git log --all --grep`, branches et worktrees non fusionnées,
  question à l'utilisateur) avant d'être répété.
- **Si `glab` n'est pas installé ou pas authentifié**, le dire et demander à
  l'utilisateur de lancer `! glab auth login` ; ne pas retomber en silence sur
  un `.md`.

### Le test de reproduction — l'idiome du dépôt

L'issue porte l'état ; **un test peut en plus tenir le bug ouvert** dans le
code. Une reproduction échoue délibérément et porte `#[ignore]`, motif écrit
dans l'attribut **avec le numéro de l'issue** (`#[ignore = "#12 : …"]`). Quand
elle se met à passer, on retire `#[ignore]` et le garde-fou prend le relais.

| Portée du bug | Où poser le test |
|---|---|
| comportement local d'une fonction | `#[cfg(test)]` dans le `src/` concerné |
| contrat public ou frontière d'un crate | `crates/<crate>/tests/` |
| le socle aux limites | la batterie `bN` de `stokesforge-robustesse` |
| logique d'interface | `crates/stokesforge-app/tests/shell.rs` (sans fenêtre) |

Un symptôme purement visuel n'est **pas** testable en CI : les tests couvrent
la géométrie, pas l'apparence. Le dire plutôt que de fabriquer un test qui
n'éprouve rien.

## 7. Ne pas corriger, ne pas concevoir

Ce skill s'arrête au rapport et aux questions. Enchaîner sur **stokesforge-bug-fix**
ou **stokesforge-feature** seulement sur demande explicite d'implémentation
dans le dépôt, selon la règle de contexte ci-dessus.
