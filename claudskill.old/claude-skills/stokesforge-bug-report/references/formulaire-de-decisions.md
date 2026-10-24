# Faire trancher plusieurs décisions : le formulaire

Règle demandée par l'utilisateur (2026-10-10). Dès qu'**au moins six choix**
sont à faire trancher en une fois (maquettes, arbitrages d'interface,
incohérences, options d'une issue), on ne pose pas les questions dans le
terminal ni avec `AskUserQuestion` : on publie un **formulaire de décisions**,
une page Artifact bâtie sur le gabarit
[`formulaire-de-decisions.html`](formulaire-de-decisions.html). Jusqu'à cinq
choix, `AskUserQuestion` suffit.

Pourquoi : l'utilisateur est dyslexique. Une décision par écran, des puces
courtes et un schéma pour chaque état lui évitent de relire de longs
paragraphes. C'est le format qu'il juge idéal pour décider en série.

## Ce que montre chaque carte, dans cet ordre

1. **Question** : courte, en titre, avec priorité (P1 risque de perte ou piège
   silencieux, P2 incohérence visible, P3 finition) et code (`R1`, `N4`…).
2. **Contexte** : *où* et *quand* l'utilisateur rencontre le cas (fenêtre,
   geste, exemple concret), et les **décisions liées** par leur code. C'était
   le manque du premier formulaire : ne jamais supposer qu'il se souvient de
   l'écran concerné ou d'une décision précédente.
3. **Aujourd'hui (avant)** : ce que fait le logiciel maintenant, **vérifié
   dans le code** (et cité au besoin), avec un schéma de l'état actuel.
4. **Les choix (après)** : la recommandation en premier, puis les autres.
   Chaque option porte :
   - des puces « après » (ce qui change) ;
   - un **schéma de l'après**, chaque fois qu'un écran ou un geste peut se
     dessiner ;
   - une ligne **+** (avantage) et une ligne **−** (défaut), **pour toutes
     les options, recommandation comprise**. Le défaut d'un choix non
     recommandé doit être lisible *avant* qu'il le choisisse.
5. « Autre / à discuter » et un commentaire libre.

Puces : une idée par puce, moins de 12 mots si possible, vocabulaire de
l'utilisateur (gestes, noms de fenêtres), jamais de nom de fonction Rust sauf
si l'utilisateur l'a déjà employé. Textes de l'interface en anglais, tels
qu'ils apparaissent à l'écran.

## Le gabarit

- Copier `formulaire-de-decisions.html` dans le scratchpad, remplacer
  seulement le bloc entre `DONNÉES — début` et `DONNÉES — fin` (`TITRE`,
  `SEC`, `D`), puis adapter `<title>`.
- Une décision : `{c, p, t, l, ctx:[…], av:{pts:[…], m}, o:[O(nom, puces,
  schéma, plus, moins), …]}` ; la première option de `o` est la
  recommandation.
- Les schémas se bâtissent avec les briques du haut du script : `f` (champ ;
  `err`, `ok`, `warn`, `off`, `ph`), `b` (bouton ; `pri`, `def`, `dan`,
  `off`), `tip` (bulle), `row`, `pair` (deux maquettes côte à côte), `grid`
  (table avant/après), `win` (dialogue avec pied), `sbar` (barre d'état),
  `slider`, `sheet` (colonnes d'une feuille avec leur lettre ; `bad`, `ok`,
  `warn`, `new`). En ajouter une au gabarit plutôt que d'écrire du HTML à la main
  dans les données.
- Vérifier le script (`node --check`) avant de publier.
- Publier avec `capabilities: {db: {}}` : les réponses vont dans la
  collection `decisions` (code, choix, `recommande`, commentaire, statut).
  Elles restent aussi dans le navigateur si la base manque.
- Un nouveau lot de décisions = un **nouvel** artifact (nouveau fichier),
  pas une republication du précédent : ses réponses sont la trace du lot.

## Défendre une option sur le fond (règle demandée le 2026-10-14)

- « Fidèle à ce que tu as choisi », « cohérent avec ta réponse précédente »
  **n'est pas un argument** pour une option. Il ne vaut que si une **règle
  générale d'interface** (`docs/standard-interface.md`, `docs/menus.md`) impose
  la cohérence ; une décision de produit prise en cherchant une solution n'en
  est pas une.
- Remettre en question les points déjà abordés : si une réponse antérieure
  mène à un résultat discutable (menu à une entrée, module d'une fonction),
  le dire dans la question et chiffrer l'inconvénient plutôt que le recopier.
- Avant de proposer un classement de menus, relire la règle des sous-menus
  (`docs/menus.md` : 3 entrées au moins, profondeur 2) : une maquette qui la
  viole est une erreur de la question, pas un choix de l'utilisateur.

## Après les réponses

1. Relire la collection avec `ArtifactData` (`action: list`,
   `collection: decisions`).
2. Croiser les réponses entre elles et avec les règles existantes
   (`CLAUDE.md`, `docs/`) : chercher les **incohérences**.
3. Pour chaque choix qui n'est pas la recommandation : dire son défaut, et
   proposer un aménagement qui garde son intention.
4. Pour un « Autre » qui demande du contexte : aller le chercher dans le code,
   puis reposer la question.
5. S'il reste au moins six points à trancher, proposer un nouveau formulaire
   au même format. Sinon, utiliser `AskUserQuestion`.
6. Consigner les décisions arrêtées dans les issues (ligne « **Cohérence** »)
   ou la doc concernée. Ne pas passer au code sans demande explicite.
