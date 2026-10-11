# Skills StokesForge

Compétences de projet chargées automatiquement par Claude Code depuis
`.claude/skills/`. Chacune est un `SKILL.md` : un en-tête (nom + quand la
déclencher) et des instructions qui n'entrent en contexte que si la tâche la
concerne.
Codex charge les mêmes dossiers via les liens de `.agents/skills/` ;
`.claude/skills/` reste leur source commune. Son parcours de lecture est dans
[`../../.codex/README.md`](../../.codex/README.md).

Elles ne remplacent pas la documentation — elles l'**utilisent**.
`docs/README.md` route par domaine, `docs/invariants.md` dit ce qu'on ne casse
pas, `.claude/rules.md` dit quoi charger et quoi ne pas ouvrir. Les skills
disent *quoi faire*, dans quel ordre, et ce qu'on oublie systématiquement.

## Les skills

| Skill | Pour quoi | Se déclenche sur |
|---|---|---|
| [`stokesforge-feature`](stokesforge-feature/SKILL.md) | implémenter une fonctionnalité de bout en bout : couche, menu contextuel, raccourci, tests, doc | demande de réalisation dans le dépôt ; passage explicite à l'implémentation après un signalement |
| [`stokesforge-bug-report`](stokesforge-bug-report/SKILL.md) | documenter les retours, créer ou compléter des issues | signalement, création d'issue et retours suivants tant que l'implémentation n'est pas demandée explicitement |
| [`stokesforge-bug-fix`](stokesforge-bug-fix/SKILL.md) | corriger : reproduire d'abord, réparer à la couche de la cause, sans affaiblir un test | demande de correction dans le dépôt ; passage explicite à la correction après un signalement |
| [`stokesforge-commande`](stokesforge-commande/SKILL.md) | ajouter une commande annulable — la seule voie de mutation du modèle | toute création / suppression / modification dans le projet |
| [`stokesforge-rendu`](stokesforge-rendu/SKILL.md) | tracé, axes, légende, export ; les deux pièges silencieux du dépôt | figure décalée, écran ≠ export, nouvelle série |
| [`stokesforge-import`](stokesforge-import/SKILL.md) | recettes d'appareil, prévisualisation façon Origin, fichiers mal découpés | CSV/TXT/ASC, nouvel instrument, aperçu faux |
| [`stokesforge-python`](stokesforge-python/SKILL.md) | pont PyO3, surface `pyst`, console de script ; le seul domaine que `cargo test --workspace` ne compile pas (travail CI `test-python` dédié) | script, GIL, numpy, `PYO3_CONFIG_FILE` |
| [`stokesforge-verification`](stokesforge-verification/SKILL.md) | écrire un test au bon endroit, puis vérifier et committer | « teste », « vérifie », « c'est bon ? », « commit » |
| [`run-stokesforge`](run-stokesforge/SKILL.md) | lancer, piloter (clics, clavier) et capturer l'application réelle sous Linux | « lance l'app », « screenshot », « vérifie dans l'app » |
| [`stokesforge-architecture`](stokesforge-architecture/SKILL.md) | répondre sur l'architecture — **pour un chimiste**, par analogies Python | « comment ça marche », « pourquoi ce découpage », « où vit X » |
| [`stokesforge-fonctionnalites`](stokesforge-fonctionnalites/SKILL.md) | répondre sur ce que le logiciel sait faire — **pour un chimiste** venu d'Origin | « est-ce que je peux… », « comment je fais pour… » |
| [`stokesforge-textes-interface`](stokesforge-textes-interface/SKILL.md) | écrire un texte visible : glossaire anglais ↔ français, majuscules, ton, messages | tout libellé, infobulle, message ou titre ajouté ou modifié |

## Ressources partagées

- [`rust-pour-pythonistes.md`](stokesforge-architecture/references/rust-pour-pythonistes.md)
  — table de traduction Rust → Python, avec pour chaque entrée **ce que
  l'analogie a de faux**. Utilisée par les deux skills de réponse.
- [`voir-l-interface.md`](stokesforge-bug-report/references/voir-l-interface.md)
  — comment *regarder* la fenêtre (`STOKESFORGE_APP_SCREENSHOT`, conversion
  PPM → PNG) et ce que la capture ne peut pas montrer. Utilisée par
  `stokesforge-bug-report`, `stokesforge-bug-fix`, `stokesforge-feature` et
  `stokesforge-verification` : les tests couvrent la géométrie, **pas
  l'apparence**.
- [`formulaire-de-decisions.md`](stokesforge-bug-report/references/formulaire-de-decisions.md)
  et son gabarit [`formulaire-de-decisions.html`](stokesforge-bug-report/references/formulaire-de-decisions.html)
  — faire trancher **six choix ou plus** par une page de décisions (une carte
  par décision : contexte, avant, après, avantage et défaut de chaque option).
  Utilisée par tous les skills qui posent des questions.

## Règles transversales, apprises à l'usage

**Conserver le contexte de signalement.** Pendant la création ou
l'enrichissement d'issues, les retours suivants complètent les issues, même
formulés comme « ajoute » ou « supprime cette icône ». Le passage au code
exige une demande explicite d'implémentation dans le dépôt ; voir la
[règle du triage](stokesforge-bug-report/SKILL.md#avant-tout--conserver-le-contexte-de-signalement).

**Ranger les menus selon la grammaire.** Toute entrée de menu ou de clic
droit se range dans une section de [`docs/menus.md`](../../docs/menus.md)
(#152) : ordre canonique, séparateurs par `MenuSections::section` seulement
(garde `aucun_separateur_brut_dans_un_menu`), *absent* ou *grisé avec
`.disabled`*, verbes nus, `Properties…` en dernier. Les skills
`stokesforge-feature` (où et comment), `stokesforge-textes-interface` (les
mots), `stokesforge-fonctionnalites` (les gestes décrits) et
`stokesforge-bug-report` (signaler contre la grammaire) s'y réfèrent.

**Suivre le standard de comportement.** Champs, messages, dialogues,
confirmations, grisage, clavier et souris suivent
[`docs/standard-interface.md`](../../docs/standard-interface.md) (#217). La
page marque chaque section *en place* ou *à venir* (issues #219–#232) : une
règle *à venir* vaut déjà pour tout code neuf, mais une brique ou une garde
*à venir* n'existe pas et ne s'appelle pas. Les skills `stokesforge-feature`
(§2 novies), `stokesforge-textes-interface` (les mots),
`stokesforge-verification` (gardes à venir) et `stokesforge-bug-report`
(signaler contre la règle) s'y réfèrent.

**Demander plutôt que trancher.** L'utilisateur n'est pas développeur : il
arbitre des comportements observables, jamais des structures de code. Un
commentaire disant « choix volontaire » ferme *une* alternative, pas toutes —
si la demande diffère de l'option rejetée, c'est une question à poser, pas un
classement à faire.

**Six choix ou plus : un formulaire, pas une liste.** L'utilisateur est
dyslexique ; il décide mieux une carte à la fois, en puces courtes avec un
schéma. Jusqu'à cinq choix, `AskUserQuestion`. Au-delà, publier le
[formulaire de décisions](stokesforge-bug-report/references/formulaire-de-decisions.md) :
pour chaque décision, le **contexte** (où, quand, décisions liées),
l'**avant** vérifié dans le code avec son schéma, puis chaque option avec son
**après** schématisé, son avantage et son **défaut** — recommandation
comprise. Relire ensuite les réponses, signaler les incohérences et le défaut
de chaque choix non recommandé.

**Partir de la brique commune, jamais de l'existant qui déroge.** Une fenêtre,
un dialogue, un assistant se construisent avec les briques du projet
(`modal_dialog_with`, `show_window_in_layer`, `tab_strip`, `list_row`,
`theme::section_title`, `theme::bold`, `color_field`,
`theme::scroll_style`, `icons::icon_button`…), **même si** un morceau voisin du code les contourne.
Quand la brique manque d'un réglage, on l'étend ; on ne la réécrit pas à côté.
L'assistant d'import a été écrit en `egui::Window` brute parce qu'un skill
décrivait cette dérogation comme « encore brute » : un skill qui constate une
dérogation doit dire qu'elle est à corriger, pas qu'elle existe.

**Un module par surface, et les tests avec leur code.** Dans
`stokesforge-app`, le code d'une surface d'interface vit dans son module de
`src/app/` (table dans `docs/application.md`, § Découpage du code), à imports
explicites ; `app.rs` ne garde que l'orchestration de l'image. Un test suit le
code qu'il éprouve : déplacer une fonction, c'est déplacer ses tests, puis les
relire (ce qu'ils prouvent, leur témoin vérifié par mutation) — travail confié
à un agent Opus à chaque restructuration (#115). Un skill qui cite un chemin
le cite là où le code vit, pas là où il vivait.

**Regarder avant de conclure.** Un symptôme visuel diagnostiqué depuis le code
seul produit une cause fausse et convaincante. C'est arrivé.

**Tenir les skills à jour, dans le même commit** (et ne jamais y laisser une dérogation décrite comme un état normal). Une fonctionnalité, un
correctif ou un chantier qui installe une règle nouvelle — un standard
d'interface (barres de défilement, sélecteur de couleur), un motif à suivre
(une fenêtre, une commande), une frontière — met à jour **au besoin** le ou
les skills concernés (`stokesforge-feature`, `stokesforge-bug-fix`, etc.), ainsi que
`CLAUDE.md`, dans le même commit que le code. Sans cela, le prochain
développement repart de l'ancienne règle et réintroduit l'incohérence. Les
issues GitLab le rappellent dans une ligne « **Cohérence** ».

## Comment ils s'enchaînent

```text
question ──► stokesforge-architecture  (comment c'est construit)
         └─► stokesforge-fonctionnalites (ce que ça sait faire)

symptôme ──► stokesforge-bug-report ──► stokesforge-bug-fix ──┐
                                                     ├──► stokesforge-verification ──► commit
demande  ──► stokesforge-feature ──┬─► stokesforge-commande ──┤
                              ├─► stokesforge-rendu ─────┤
                              ├─► stokesforge-import ────┤
                              └─► stokesforge-python ────┘
```

La flèche de `stokesforge-bug-report` vers une correction ou une
implémentation exige une demande explicite de passer au code ; elle ne suit
pas automatiquement la création ou la validation d'une issue.

`stokesforge-verification` est le passage obligé avant tout commit : c'est lui qui
porte ce qu'un `cargo test --workspace` vert **ne prouve pas** (ni `python`, ni
rien qui exige une vraie fenêtre).

## Les invoquer à la main

Claude et Codex les déclenchent quand la tâche correspond. Pour forcer :
`/stokesforge-feature` dans Claude, `$stokesforge-feature` dans Codex, et de même pour
les autres skills.

## En ajouter un

Un skill mérite d'exister quand un domaine a **des pièges qui ne lèvent aucune
erreur** — c'est le critère qui a produit ceux-ci. Un domaine bien décrit par
sa page `docs/` et sans piège silencieux n'en a pas besoin : `.claude/rules.md`
y suffit.

Format : un dossier par skill, un `SKILL.md` dedans, en-tête YAML avec `name`
(identique au dossier) et `description` (ce que le skill fait **et** dans quels
cas le déclencher — c'est le seul texte lu pour décider). Ressources longues
dans `references/`, citées depuis le `SKILL.md`. Tout en français, comme le
reste du dépôt.
