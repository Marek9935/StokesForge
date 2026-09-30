# Skills PlotYx

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
| [`plotyx-feature`](plotyx-feature/SKILL.md) | implémenter une fonctionnalité de bout en bout : couche, menu contextuel, raccourci, tests, doc | « ajoute », « implémente », « je voudrais pouvoir… » |
| [`plotyx-bug-report`](plotyx-bug-report/SKILL.md) | transformer un symptôme en rapport rejouable, avec test de reproduction | « ça plante », « c'est bizarre », « la valeur est fausse » |
| [`plotyx-bug-fix`](plotyx-bug-fix/SKILL.md) | corriger : reproduire d'abord, réparer à la couche de la cause, sans affaiblir un test | « corrige », « répare », après un rapport |
| [`plotyx-commande`](plotyx-commande/SKILL.md) | ajouter une commande annulable — la seule voie de mutation du modèle | toute création / suppression / modification dans le projet |
| [`plotyx-rendu`](plotyx-rendu/SKILL.md) | tracé, axes, légende, export ; les deux pièges silencieux du dépôt | figure décalée, écran ≠ export, nouvelle série |
| [`plotyx-import`](plotyx-import/SKILL.md) | recettes d'appareil, prévisualisation façon Origin, fichiers mal découpés | CSV/TXT/ASC, nouvel instrument, aperçu faux |
| [`plotyx-python`](plotyx-python/SKILL.md) | pont PyO3, surface `pyx`, console de script ; le seul domaine où un CI vert ne prouve rien | script, GIL, numpy, `PYO3_CONFIG_FILE` |
| [`plotyx-verification`](plotyx-verification/SKILL.md) | écrire un test au bon endroit, puis vérifier et committer | « teste », « vérifie », « c'est bon ? », « commit » |
| [`plotyx-architecture`](plotyx-architecture/SKILL.md) | répondre sur l'architecture — **pour un chimiste**, par analogies Python | « comment ça marche », « pourquoi ce découpage », « où vit X » |
| [`plotyx-fonctionnalites`](plotyx-fonctionnalites/SKILL.md) | répondre sur ce que le logiciel sait faire — **pour un chimiste** venu d'Origin | « est-ce que je peux… », « comment je fais pour… » |

## Ressources partagées

- [`rust-pour-pythonistes.md`](plotyx-architecture/references/rust-pour-pythonistes.md)
  — table de traduction Rust → Python, avec pour chaque entrée **ce que
  l'analogie a de faux**. Utilisée par les deux skills de réponse.
- [`voir-l-interface.md`](plotyx-bug-report/references/voir-l-interface.md)
  — comment *regarder* la fenêtre (`PLOTYX_APP_SCREENSHOT`, conversion
  PPM → PNG) et ce que la capture ne peut pas montrer. Utilisée par
  `plotyx-bug-report`, `plotyx-bug-fix`, `plotyx-feature` et
  `plotyx-verification` : les tests couvrent la géométrie, **pas
  l'apparence**.

## Quatre règles transversales, apprises à l'usage

**Demander plutôt que trancher.** L'utilisateur n'est pas développeur : il
arbitre des comportements observables, jamais des structures de code. Un
commentaire disant « choix volontaire » ferme *une* alternative, pas toutes —
si la demande diffère de l'option rejetée, c'est une question à poser, pas un
classement à faire.

**Partir de la brique commune, jamais de l'existant qui déroge.** Une fenêtre,
un dialogue, un assistant se construisent avec les briques du projet
(`modal_dialog_with`, `show_window_in_layer`, `tab_strip`, `color_field`,
`theme::scroll_style`…), **même si** un morceau voisin du code les contourne.
Quand la brique manque d'un réglage, on l'étend ; on ne la réécrit pas à côté.
L'assistant d'import a été écrit en `egui::Window` brute parce qu'un skill
décrivait cette dérogation comme « encore brute » : un skill qui constate une
dérogation doit dire qu'elle est à corriger, pas qu'elle existe.

**Regarder avant de conclure.** Un symptôme visuel diagnostiqué depuis le code
seul produit une cause fausse et convaincante. C'est arrivé.

**Tenir les skills à jour, dans le même commit** (et ne jamais y laisser une dérogation décrite comme un état normal). Une fonctionnalité, un
correctif ou un chantier qui installe une règle nouvelle — un standard
d'interface (barres de défilement, sélecteur de couleur), un motif à suivre
(une fenêtre, une commande), une frontière — met à jour **au besoin** le ou
les skills concernés (`plotyx-feature`, `plotyx-bug-fix`, etc.), ainsi que
`CLAUDE.md`, dans le même commit que le code. Sans cela, le prochain
développement repart de l'ancienne règle et réintroduit l'incohérence. Les
issues GitLab le rappellent dans une ligne « **Cohérence** ».

## Comment ils s'enchaînent

```text
question ──► plotyx-architecture  (comment c'est construit)
         └─► plotyx-fonctionnalites (ce que ça sait faire)

symptôme ──► plotyx-bug-report ──► plotyx-bug-fix ──┐
                                                     ├──► plotyx-verification ──► commit
demande  ──► plotyx-feature ──┬─► plotyx-commande ──┤
                              ├─► plotyx-rendu ─────┤
                              ├─► plotyx-import ────┤
                              └─► plotyx-python ────┘
```

`plotyx-verification` est le passage obligé avant tout commit : c'est lui qui
porte ce qu'un `cargo test --workspace` vert **ne prouve pas** (ni `python`, ni
`typst`, ni rien qui exige une vraie fenêtre).

## Les invoquer à la main

Claude et Codex les déclenchent quand la tâche correspond. Pour forcer :
`/plotyx-feature` dans Claude, `$plotyx-feature` dans Codex, et de même pour
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
