---
name: plotyx-architecture
description: Répondre aux questions sur l'architecture globale de PlotYx — découpage en crates, rôle de chacun, acteur de session, commandes annulables, pipeline de rendu, pont Python, format `.plx`, invariants, et « où vit X ? ». À utiliser dès que la question est « comment ça marche », « pourquoi c'est découpé comme ça », « où se trouve… », « qu'est-ce que fait ce crate », « pourquoi ce choix plutôt qu'un autre », « c'est quoi un acteur / une commande / un invariant ». La réponse s'adresse à un **chimiste** : bonne connaissance de Python, aucune de Rust, bases de code limitées — expliquer par analogies Python, sans jargon Rust non traduit.
---

# Répondre sur l'architecture de PlotYx

## À qui on parle

L'utilisateur est **chimiste**. Il connaît correctement Python, n'a jamais
écrit de Rust, et n'a pas de pratique étendue du développement logiciel. Il
comprend parfaitement des idées d'ingénierie **si on les lui donne en
concepts** plutôt qu'en vocabulaire.

Ce que ça change, concrètement :

- **Traduire, ne pas simplifier.** Il n'y a aucune raison de lui cacher
  pourquoi l'acteur détient l'unique référence mutable — c'est une idée simple.
  Ce qu'il faut éviter, c'est le mot `&mut Project` non expliqué.
- **Une analogie Python par concept Rust**, et dire tout de suite où elle
  casse. Une analogie non corrigée produit une mauvaise attente qui coûtera
  plus tard. Table complète :
  [`references/rust-pour-pythonistes.md`](references/rust-pour-pythonistes.md)
  — y piocher un mot au besoin, jamais la dérouler.
- **Pas de code dans la réponse sauf demande explicite.** Et si du code est
  montré, 3 à 10 lignes, commentées en français.
- **Les noms de fichiers sont utiles**, eux : ils lui permettent d'aller voir.
  Toujours sous la forme `chemin:ligne` ou `chemin` — c'est cliquable.
- Il vient d'**OriginLab** : comparer à Origin dit souvent plus qu'un
  paragraphe d'explication.

Mots à ne pas employer sans les traduire en une demi-phrase : *borrow checker*,
*lifetime*, monomorphisation, *trait object*, `Send`/`Sync`, TOCTOU, LIFO,
acteur, *backend*, tessellation.

## Méthode

1. **Vérifier dans le code avant de répondre.** Les pages `docs/` orientent, le
   code fait foi ; `docs/archive/` n'est plus maintenu et décrit parfois des
   options **écartées**. Une réponse d'architecture qui cite un fichier doit
   avoir vérifié qu'il existe encore.
2. **Charger le domaine, pas le workspace.** `docs/README.md` route par
   question, `.claude/rules.md` dit quels crates ouvrir et lesquels ne pas
   ouvrir.
3. **Répondre en trois temps** : la réponse en une ou deux phrases, puis le
   détail utile, puis où aller voir. Ne pas noyer une question courte.
4. **Dire ce qu'on ne sait pas.** « Ce n'est pas décrit et je ne l'ai pas
   vérifié » vaut mieux qu'une reconstitution plausible.

## La carte, en langage ordinaire

À réutiliser comme base d'explication, en n'en sortant que ce que la question
demande.

**Le principe central** : PlotYx est découpé en une douzaine de paquets
(*crates*, l'équivalent des paquets Python) empilés du plus abstrait au plus
concret. Chaque étage ne connaît que les étages du dessous. Un paquet qui
décrit des données ne sait pas qu'une fenêtre existe — c'est ce qui permet de
changer l'interface sans toucher aux données, et de tester les données sans
ouvrir de fenêtre.

| Paquet | Ce qu'il fait, dit simplement |
|---|---|
| `plotyx-id` | donne à chaque objet du projet un numéro unique, jamais réutilisé |
| `plotyx-plot` | décrit **ce qu'il faut tracer** (« une courbe rouge, axe log ») sans savoir comment dessiner |
| `plotyx-core` | le projet lui-même : dossiers, classeurs, feuilles, colonnes, valeurs |
| `plotyx-commands` | les actions annulables — la pile Ctrl+Z |
| `plotyx-persistence` | lire et écrire le fichier `.plx` |
| `plotyx-session` | le gardien : un seul endroit au monde a le droit de modifier le projet |
| `plotyx-import` | lire les CSV/TXT/ASC des appareils |
| `plotyx-render` | **tout** le dessin, indépendamment de la destination |
| `plotyx-render-egui` | poser ce dessin à l'écran |
| `plotyx-render-svg` | poser ce même dessin dans un SVG, un PDF, un PNG |
| `plotyx-script` / `plotyx-python` | exécuter des scripts Python qui pilotent le projet |
| `plotyx-app` | la fenêtre |
| `plotyx-cli` | un binaire sans interface, pour vérifier que tout se lie |
| `plotyx-robustesse` | 21 batteries de tests qui poussent le reste à ses limites |

**Les trois idées qui expliquent presque tout le reste** :

1. **Un seul écrivain.** Un composant (« l'acteur ») détient le projet ; tous
   les autres n'en ont qu'une photo figée, qu'ils peuvent lire à volonté sans
   risque. Pour modifier, on lui envoie une demande. Analogie Python : un seul
   fil possède l'objet, les autres reçoivent une copie en lecture. Conséquence
   directe : il est **impossible** de modifier le projet sans passer par une
   action annulable — ce n'est pas une règle de discipline, c'est le
   compilateur qui l'interdit.
2. **Toute modification est une commande annulable.** Taper une valeur,
   renommer une feuille, ajouter un graphe : chacune est un objet « commande »
   qui sait aussi défaire son effet. C'est pour ça que Ctrl+Z marche partout de
   la même façon, et pourquoi ajouter une fonctionnalité qui modifie les
   données demande de créer une commande.
3. **Le dessin est écrit une seule fois.** Une courbe à l'écran et la même
   courbe exportée en PDF passent par le **même** code ; seule la dernière
   étape (« poser un pixel » vs « écrire une balise SVG ») diffère. C'est ce
   qui garantit que ce qu'on exporte est ce qu'on voit.

**Les invariants** (`docs/invariants.md`) sont les dix règles qu'aucun
changement n'a le droit de casser. Leur particularité, et c'est ce qu'il faut
expliquer quand on en parle : la colonne qui compte est « ce qui la tient ».
Une règle tenue par le compilateur ou par un test n'a pas besoin qu'on y pense ;
une règle tenue par la relecture se perd.

## Questions fréquentes, et où la réponse se trouve

| La question porte sur | Lire |
|---|---|
| le découpage et ses raisons | `docs/architecture.md` |
| ce qu'on n'a pas le droit de casser | `docs/invariants.md` |
| l'arbre du projet, les colonnes, la description d'un graphe | `docs/modele-de-donnees.md` |
| l'annulation, les macros | `docs/commandes-et-historique.md` |
| le fichier `.plx` | `docs/persistance.md` |
| l'acteur, la concurrence | `docs/session.md` |
| le tracé, les axes, l'export | `docs/rendu.md` |
| Python embarqué, scripts | `docs/python-et-script.md` |
| la fenêtre, la grille, les raccourcis | `docs/application.md` |
| les tests et la CI | `docs/tests.md` |
| le *pourquoi* d'une décision précise | le commentaire de tête (`//!`) du module concerné — c'est là qu'il vit, jamais dans les pages `docs/` |
| une option **écartée** et sa raison | `docs/archive/` (non maintenu : ne jamais le présenter comme l'état actuel) |

## Deux pièges de réponse

- **Une note de chantier ne prouve rien.** `docs/chantier-*.md` et
  `docs/chantiers/` sont des notes de passation ; elles listent parfois un
  « reste à faire » déjà fait. Vérifier dans le code avant d'affirmer qu'une
  chose manque.
- **Les phases citées partout sont closes.** Un `R6`, `R7`, `R8` ou une phase
  0-5 dans un commentaire désigne du travail **fait**, pas du travail à venir.
  Ne jamais les présenter comme une feuille de route en cours.
