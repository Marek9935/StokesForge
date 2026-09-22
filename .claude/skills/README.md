# Skills PlotYx

Compétences de projet chargées automatiquement par Claude Code depuis
`.claude/skills/`. Chacune est un `SKILL.md` : un en-tête (nom + quand la
déclencher) et des instructions qui n'entrent en contexte que si la tâche la
concerne.

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

Ressource partagée par les deux skills de réponse :
[`rust-pour-pythonistes.md`](plotyx-architecture/references/rust-pour-pythonistes.md)
— table de traduction Rust → Python, avec pour chaque entrée **ce que
l'analogie a de faux**.

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

Claude les déclenche seul quand la tâche correspond. Pour forcer :
`/plotyx-feature`, `/plotyx-bug-fix`, etc.

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
