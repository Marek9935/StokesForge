---
name: stokesforge-architecture
description: >-
  Répondre aux questions sur l'architecture globale de StokesForge — découpage en crates, rôle de chacun, acteur de session, commandes annulables, pipeline de rendu, pont Python, format `.stf`, invariants, et « où vit X ? ». À utiliser dès que la question est « comment ça marche », « pourquoi c'est découpé comme ça », « où se trouve… », « qu'est-ce que fait ce crate », « pourquoi ce choix plutôt qu'un autre », « c'est quoi un acteur / une commande / un invariant ». La réponse s'adresse à un **chimiste** : bonne connaissance de Python, aucune de Rust, bases de code limitées — expliquer par analogies Python, sans jargon Rust non traduit.
---

# Répondre sur l'architecture de StokesForge

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

### Quand la demande est un audit ou une recherche de doublons

- Partir des manifestes et de `cargo metadata --no-deps --format-version 1
  --offline` pour vérifier le graphe réel. Distinguer dépendances de
  production, de test et features optionnelles ; le schéma documentaire ne
  remplace pas cette vérification.
- Suivre une même règle sur ses différents chemins : aperçu/import,
  écran/export, fenêtres flottantes/détachées, ouverture/clôture des macros.
  Chercher une divergence observable avant de proposer une factorisation.
- Un type de transport distinct du modèle ou deux adaptations de backend
  ne sont pas à fusionner par principe. Pour un doublon volontaire, lire sa
  justification et distinguer dette de maintenance et bug reproduit.
- Éprouver la portée des garde-fous : les tests `no_ui_dependency.rs` lisent
  les manifestes par `stokesforge-arch-guard` (#24), qui compte
  `[dependencies]`, `[build-dependencies]`, leurs variantes conditionnelles
  `[target.…]`, les optionnelles et le nom réel d'une dépendance renommée, mais
  pas `[dev-dependencies]`. Ils ne voient que **leur** crate et sa liste
  blanche : un crate sans `no_ui_dependency.rs` n'est pas gardé, et le graphe
  transitif se vérifie par `cargo metadata`.
- Consulter les issues GitLab ouvertes **et fermées** avant de signaler.
  Pour une demande de diagnostic seul, créer les issues autorisées avec
  preuves, portée, priorité et cohérence documentaire ; ne pas corriger le
  code. Garder les sondes temporaires hors des sources si aucun ajout de
  test au dépôt n'est demandé.

## La carte, en langage ordinaire

À réutiliser comme base d'explication, en n'en sortant que ce que la question
demande.

**Le principe central** : StokesForge est découpé en paquets
(*crates*, l'équivalent des paquets Python) empilés du plus abstrait au plus
concret. Chaque étage ne connaît que les étages du dessous. Un paquet qui
décrit des données ne sait pas qu'une fenêtre existe — c'est ce qui permet de
changer l'interface sans toucher aux données, et de tester les données sans
ouvrir de fenêtre.

| Paquet | Ce qu'il fait, dit simplement |
|---|---|
| `stokesforge-id` | donne à chaque objet du projet un numéro unique, jamais réutilisé |
| `stokesforge-plot` | décrit **ce qu'il faut tracer** (« une courbe rouge, axe log ») sans savoir comment dessiner |
| `stokesforge-core` | le projet lui-même : dossiers, classeurs, feuilles, colonnes, valeurs |
| `stokesforge-commands` | les actions annulables — la pile Ctrl+Z |
| `stokesforge-persistence` | lire et écrire le fichier `.stf` |
| `stokesforge-session` | le gardien : un seul endroit au monde a le droit de modifier le projet |
| `stokesforge-import` | lire les CSV/TXT/ASC des appareils (ne connaît rien du projet) |
| `stokesforge-importer` | transformer ce qui a été lu en colonnes du projet — la même conversion pour la fenêtre, la ligne de commande et les scripts |
| `stokesforge-render` | **tout** le dessin, indépendamment de la destination |
| `stokesforge-render-egui` | poser ce dessin à l'écran |
| `stokesforge-render-svg` | poser ce même dessin dans un SVG, un PDF, un PNG |
| `stokesforge-clipboard` | offrir une figure au presse-papiers du système (SVG, PNG, PDF à la fois) |
| `stokesforge-script` / `stokesforge-python` | exécuter des scripts Python qui pilotent le projet |
| `stokesforge-app-state` | l'état de l'application **sans fenêtre** : ce qui se décide (actions, sélection, préférences, textes traduits, erreurs en phrases) ; il ne connaît aucune bibliothèque d'interface, et c'est le compilateur qui le garantit |
| `stokesforge-app` | la fenêtre : elle peint cet état et traduit clavier et souris en actions |
| `stokesforge-cli` | la ligne de commande : `import` (fichier d'appareil → `.stf`) et, avec Python, `run` (exécuter un script sans fenêtre) |
| `stokesforge-robustesse` | 22 batteries de tests qui poussent le reste à ses limites |
| `stokesforge-arch-guard` | outil de test : lit les manifestes pour vérifier qu'aucun paquet ne dépend de ce qu'il ne doit pas connaître |

**Les trois idées qui expliquent presque tout le reste** :

1. **Un seul écrivain.** Un composant (« l'acteur ») détient le projet ; tous
   les autres n'en ont qu'une photo figée, qu'ils peuvent lire à volonté sans
   risque. Pour modifier, on lui envoie une demande. Analogie Python : un seul
   fil possède l'objet, les autres reçoivent une copie en lecture. Conséquence
   directe : les clients d'une session ne disposent pas du projet mutable.
   Cela ne rend pas les primitives de `Project` privées : initialisation,
   chargement et tests les emploient directement. La garantie porte sur
   l'accès des clients au projet vivant de cette session, pas sur tout le
   processus ni sur tout appel possible à `stokesforge-core`.
2. **Toute modification est une commande annulable.** Taper une valeur,
   renommer une feuille, ajouter un graphe : chacune est un objet « commande »
   qui sait aussi défaire son effet. C'est pour ça que Ctrl+Z marche partout de
   la même façon, et pourquoi ajouter une fonctionnalité qui modifie les
   données demande de créer une commande.
   Distinguer les exceptions documentées : état de vue et géométrie hors
   historique, chargement du projet, recalcul des valeurs de formule par
   l'acteur comme conséquence d'une commande (sans entrée supplémentaire).
   Ce recalcul est **ciblé** (#193) : l'acteur garde, pour chaque
   colonne-formule, une « empreinte » de ce qu'il a lu au dernier calcul
   (texte, colonnes désignées et leur numéro de version, longueur…)
   et ne recalcule que si elle a changé. Analogie Python : un
   `functools.lru_cache` dont la clé serait toutes les entrées du calcul —
   ce que l'analogie a de faux : la clé n'est pas le contenu des colonnes
   mais leur numéro de version, qui change à chaque écriture ; et il n'y a
   qu'une entrée par colonne, pas un cache de résultats. Le recalcul
   complet d'avant reste comme oracle de test (`b22`).
3. **Le dessin est écrit une seule fois.** Une courbe à l'écran et la même
   courbe exportée en PDF passent par le **même** code ; seule la dernière
   étape (« poser un pixel » vs « écrire une balise SVG ») diffère. C'est ce
   qui garantit que ce qu'on exporte est ce qu'on voit.

**Les invariants** (`docs/invariants.md`) sont les onze règles qu'aucun
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
| le fichier `.stf` | `docs/persistance.md` |
| l'acteur, la concurrence | `docs/session.md` |
| le tracé, les axes, l'export | `docs/rendu.md` |
| Python embarqué, scripts | `docs/python-et-script.md` |
| la fenêtre, la grille, les raccourcis | `docs/application/coquille-et-code.md` (§ Découpage du code : un module par surface) |
| l'ordre et le rangement des menus | `docs/menus.md` |
| les mots de l'interface, la traduction | `docs/textes-interface.md` |
| les icônes | `docs/icones.md` |
| la licence, ce que couvre l'exception `pyst` | `docs/licence-et-extensions.md` |
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
