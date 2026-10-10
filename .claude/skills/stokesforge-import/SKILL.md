---
name: stokesforge-import
description: Travailler sur l'import de fichiers d'instruments dans StokesForge (`stokesforge-import`) — recettes de format d'appareil, séparateurs, virgule décimale, lignes d'en-tête et de pied, sous-en-têtes et leurs rôles (nom long, unité, commentaire), prévisualisation façon assistant ASCII d'OriginLab, conversion vers les colonnes du modèle. À utiliser dès qu'il est question de lire un CSV/TXT/ASC d'appareil, d'ajouter le support d'un nouvel instrument (Jasco, spectro, chromato…), d'un fichier mal découpé, d'une colonne qui arrive en texte au lieu de nombres, ou d'un aperçu d'import incorrect.
---

# Import de fichiers d'instruments

`stokesforge-import` reproduit l'assistant d'import ASCII d'OriginLab — et sa fenêtre côté application est un **dialogue modal de la brique commune** (`modal_dialog_with`, voir `stokesforge-feature` § 2 sexies), jamais une `egui::Window` brute :
l'utilisateur choisit une **recette** décrivant comment un appareil écrit ses
fichiers, l'applique, et peut l'enregistrer pour la réutiliser.

Lire d'abord la doc de tête de `stokesforge-import/src/lib.rs`, puis
`docs/chantier-import-previsualisation.md` — en gardant en tête que c'est une
**note de passation** : son « reste à faire » demande confrontation au code
avant d'être repris.

## La frontière à ne pas franchir

Le crate est **volontairement pur** : zéro dépendance UI, zéro dépendance à
`stokesforge-commands`, `stokesforge-session` **et `stokesforge-core`**. Son vocabulaire de
sortie est le plus simple qui existe — des `f64`, `NaN` pour une valeur
manquante — précisément pour rester indépendant.

C'est `stokesforge-importer` qui convertit vers `stokesforge_core::Column`
(`plan_import`, `make_import_column`), pour l'application, le CLI et
`pyst.import_file` à la fois — jamais un consommateur de son côté.
Conséquence pratique : **ne jamais faire entrer un type de `stokesforge-core` dans
`stokesforge-import`** pour se simplifier la vie. Si une information manque à la
conversion, elle s'ajoute au vocabulaire propre du crate, pas en important le
modèle.

Et côté application : l'assistant vit dans
`stokesforge-app/src/app/import_wizard.rs` (#118) — `import_window`, seul point
d'entrée appelé par `app.rs`, ses six étapes, l'aperçu, les tables de libellés —
et ses tests unitaires l'y suivent (étapes peintes avec leur aperçu,
Terminer/Annuler, clic droit sur une ligne, libellés distincts) ; l'état
(`ImportState`, `import_line_action`…) reste dans `state.rs`, testé par
`tests/shell.rs`. **Les textes de l'assistant** (`import_window` et ses six
étapes, `ImportStep::title`, tables `separator_label`, `column_role_label`…) sont
des clés `import-*` du catalogue `en`, employées par `tr!` ; les termes viennent
de `docs/textes-interface.md` § 2.4 (*Delimiter*, pas « separator » ; *Template*,
jamais « recipe » ni « recette » à l'écran ; *Subheader*, *Footer*, *Block*). Un
réglage nouveau ajoute sa clé (libellé de 1 à 3 mots, infobulle en attribut
`.hint`), pas un littéral. Les noms de modèles (« JASCO »…) et le contenu du
fichier sont des données : jamais traduits. Le vocabulaire de `stokesforge-import`
(`recipe`, `SubheaderRoles`…) reste celui du code et ne passe jamais à l'écran.

Et côté application (et `pyst`) : l'import écrit dans un projet vivant, donc il
passe par une **commande annulable** comme tout le reste (skill
**stokesforge-commande**). Un import qu'on ne peut pas annuler serait la seule
écriture non réversible du logiciel. Seul `stokesforge-cli import` construit un
`Project` neuf directement, sans session ni historique : il n'y a rien à
annuler.

## Les modules

| Module | Rôle |
|---|---|
| `recipe` | le **modèle de recette** : description figée et sérialisable d'un format d'appareil. Ne connaît pas le moteur de parsing |
| `parse` | applique une recette à un fichier |
| (crate `stokesforge-importer`) | `plan_import`, `make_import_column` : des tableaux analysés aux **colonnes du modèle**, partagés par l'application, le CLI et `pyst.import_file` — ne jamais réécrire cette conversion dans un consommateur ; chacun décide seulement comment écrire (commandes, `Project` direct) |
| `layout` | **le** découpage d'un fichier en zones (en-tête, sous-en-tête, noms, données, pied, commentaires, lignes ignorées), calculé une fois et partagé par `parse` et `preview` : ne jamais réécrire cette suite d'étapes dans l'un des deux |
| `numbers`, `datetime` | lecture des nombres (décimale `Auto`, milliers, valeurs manquantes) et des dates en timestamps POSIX (copie de `stokesforge_core::datetime::parse_datetime`, accord testé dans `stokesforge-importer`) |
| `builtin`, `choose` | modèles fournis d'office, et choix automatique d'un modèle (extension, nom, signature de contenu) |
| `preview` | (même découpage que `parse` : séparateur résolu sur la même ligne, virgule exclue si décimale virgule — test `accord_apercu_import.rs`, issue #23) classe chaque ligne en rôle (en-tête principal, sous-en-tête et son rôle, donnée, pied) pour l'aperçu en direct |
| `store` | enregistre et recharge les recettes (JSON) |
| `decode` | octets → texte avant tout découpage (UTF-8, ou page de code Windows des logiciels d'appareil, `°C` en `0xB0`) |
| `metadata` | l'en-tête (et le pied si demandé) en lignes clé / valeur ; `stokesforge-importer` les convertit en `MetaLine` |
| `recent` | fichiers importés récemment et dossier mémorisé (`import_recent.json`), jamais de panique sur un fichier corrompu |

`preview` existe pour que la fenêtre d'import montre **en direct** comment un
fichier va être découpé pendant que l'utilisateur règle la recette — comme
Origin. Sa doc de tête porte le budget de coût sur un gros fichier : le
respecter, un aperçu qui relit tout le fichier à chaque frappe rend la fenêtre
inutilisable.

## Étendre une recette sans casser les recettes enregistrées

C'est le piège principal de ce crate. Une recette est sérialisée en JSON et
**rechargée depuis le disque** : un champ ajouté sans précaution rend
illisibles toutes les recettes que l'utilisateur a déjà enregistrées.

La convention en place, à suivre à l'identique :

- tout champ nouveau porte `#[serde(default)]`, pour qu'une recette enregistrée
  avant son arrivée se recharge à l'identique ;
- sa valeur par défaut doit reproduire **exactement** le comportement d'avant.
  `HeaderLayout` (découpage en-tête principal / sous-en-tête) le fait bien :
  zéro partout laisse `header` entièrement aux commandes, comme avant le
  chantier de prévisualisation — les deux modèles ne se recouvrent que si
  l'appelant y touche ;
- un champ sans effet tant qu'un autre n'est pas réglé (`subheader_roles` sans
  ligne de sous-en-tête) doit le dire dans sa doc, sinon le réglage a l'air
  cassé.

Tester ce point explicitement : une recette JSON **d'avant** le changement doit
se recharger et produire le même découpage.

## Erreurs de lecture les plus fréquentes, et où elles vivent

| Symptôme | Où regarder |
|---|---|
| colonnes fusionnées en une seule | `Separator` de la recette (tabulation vs espaces multiples vs `;`) |
| nombres lus comme du texte | `DecimalStyle` — un appareil européen écrit `1,234` |
| premières lignes de données avalées | `HeaderSkip` / `HeaderLayout` |
| dernières lignes parasites (sommes, mentions) | `footer_skip_lines` (nombre fixe) ou `data_end` (`DataEnd`, défaut `Auto` : coupe au premier X non numérique une fois armée ; règle unique dans `data_end.rs`, partagée par `parse` et `preview`) |
| noms de colonnes absents ou faux | `has_column_header_row`, `NameSource` |
| nom long / unité / commentaire / x' (#164) mal placés | `SubheaderRoles` |
| préfixe parasite sur chaque ligne | le retrait de caractères en tête de ligne de données |

Une **colonne à types mixtes** n'est pas un échec d'import : le modèle sait
porter une colonne dont certaines cellules sont du texte (« n/a », « sat. »)
et d'autres des nombres, et un tracé n'y garde que les cellules numériques.
Voir `docs/modele-de-donnees.md`. Refuser le fichier serait la mauvaise
réponse — invariant 10, un défaut de donnée dégrade, il ne plante pas.

## Tests

`stokesforge-import/tests/fixtures.rs` porte les fichiers d'exemple. Ajouter le
support d'un appareil, c'est **ajouter son fichier d'exemple** — un extrait
réduit et anonymisé suffit — et le test qui vérifie le découpage attendu.
Sans fichier réel, une recette n'est qu'une hypothèse.

Le dépôt contient déjà des échantillons sous `test/import/` : s'en servir
plutôt que d'en inventer, et vérifier avant d'en committer un nouveau qu'il ne
contient pas de données que l'utilisateur ne veut pas publier.

## Checklist

- [ ] doc de tête de `lib.rs` lue ; note de passation confrontée au code
- [ ] aucune dépendance à `core`/`commands`/`session`/UI ajoutée
- [ ] nouveau champ de recette : `#[serde(default)]` + défaut = comportement d'avant
- [ ] rechargement d'une recette enregistrée **avant** le changement testé
- [ ] fichier d'exemple ajouté à `tests/fixtures.rs`
- [ ] budget de coût de l'aperçu respecté sur un gros fichier
- [ ] l'écriture dans le projet passe par une commande annulable
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts
