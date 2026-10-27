# Contribuer à StokesForge

Le développement actif se fait sur `main`. Lire
[`AGENTS.md`](AGENTS.md), [`.codex/README.md`](.codex/README.md) et la
documentation du domaine avant de proposer une modification. Les bugs et
évolutions se suivent dans les issues GitLab. Les commentaires, la
documentation et les messages de commit sont en français.

## Travailler à plusieurs

Une contribution passe par une merge request vers `main` : une relecture, la CI
verte exigée. Ni force-push ni rebase sur une branche partagée ; la branche se
nomme `feature/<n>-<sujet>`, reste courte et reçoit `main` avant la fusion
(détail : [`docs/criteres-v1.md`](docs/criteres-v1.md), « Branches et versions »).

## Licence des contributions

Toute contribution originale à cette branche doit être proposée sous
**GPL-3.0-or-later avec la permission supplémentaire de
[`LICENSE-EXCEPTION`](LICENSE-EXCEPTION)**. Ne proposer que des éléments dont
vous détenez les droits nécessaires ; conserver les notices des composants
tiers et indiquer leur provenance. Les contributions à une copie tierce
vendorisée conservent sa licence, et leurs modifications doivent être
identifiées.

Chaque commit extérieur doit porter un `Signed-off-by` attestant le
[`Developer Certificate of Origin 1.1`](DCO), fourni sans modification.
Après lecture du DCO, `git commit -s` ajoute cette ligne avec l'identité Git :

```text
Signed-off-by: Prénom Nom <adresse@example.org>
```

La signature confirme votre droit de proposer la contribution sous sa
licence ; elle ne transfère pas votre copyright. Aucun CLA de cession ou de
relicenciement n'est actuellement mis en place. Une éventuelle double licence
commerciale nécessiterait l'accord des titulaires concernés ou un CLA séparé
convenu avant intégration ; le DCO ne remplace pas cet accord.

## Vérifications

Suivre les contrôles du guide Codex : `fmt`, `clippy`, tests du workspace et
tests Python quand le chemin Python est modifié. Une dépendance ou une
ressource nouvelle doit aussi respecter [`deny.toml`](deny.toml) et figurer
dans les notices de distribution. Les outils et commandes de licence sont
dans [`docs/licence-et-extensions.md`](docs/licence-et-extensions.md).

La CI Linux et Windows tourne sur GitHub (miroir du dépôt) ; les
résultats se lisent dans le pipeline GitLab du commit ou par
`scripts/ci-statuts.sh`. Mode d'emploi, pièges du runner et maintenance :
[`docs/tests.md`](docs/tests.md), « Utiliser la CI au quotidien ».

Toute nouvelle interface d'extension couverte par l'exception nécessite un
commit explicite dans la liste fermée de cette même page, avec son contrat
public. Le présent accord de contribution concerne `main` ; il ne
change pas la licence de l'ancienne implémentation conservée sous le tag `legacy-python`.
