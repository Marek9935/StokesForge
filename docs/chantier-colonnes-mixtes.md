# Chantier « colonnes à types mixtes » — note de passation

**Ce document n'est pas une page de référence.** Contrairement au reste de
`docs/`, qui décrit le code tel qu'il est, celui-ci décrit un *chantier* : ce
qu'il visait, ce qu'il a trouvé en chemin, et ce qu'il laisse ouvert. Il a une
durée de vie limitée — quand la colonne « reste à faire » sera vide, il rejoint
`docs/archive/`.

Le *quoi* et le *pourquoi* du résultat, eux, vivent dans les pages normales :
[`modele-de-donnees.md`](modele-de-donnees.md) (le type et ses règles),
[`persistance.md`](persistance.md) (le format et la compatibilité),
[`commandes-et-historique.md`](commandes-et-historique.md) (les deux écritures
de cellules), [`application.md`](application.md) (la saisie).

## L'objectif

Une colonne avait un type unique pour toute sa longueur : `Numeric`,
`DateTime` ou `Text`. Le but du chantier : **qu'une même colonne porte du texte
et des nombres selon les lignes**.

Le besoin vient d'un import réel. Une colonne de mesures qui porte
`1,2 / 1,3 / n/a / 1,5` n'avait que deux issues, mauvaises toutes les deux :

- **tout en nombres** — le `n/a` devient un `NaN`, indiscernable d'une mesure
  simplement absente ; l'information « l'instrument a répondu *n/a* ici » est
  perdue à la lecture du fichier, définitivement ;
- **tout en texte** — rien ne se trace plus, et les 999 autres lignes paient
  pour la première.

La contrainte qui a guidé la conception : **minimiser la casse**. Le format
`.stf` déjà écrit, les commandes existantes, le rendu qui suppose une colonne
homogène, et la vue numpy sans copie du pont Python devaient tous survivre.

## Ce qui est en place

| Crate | Ce qui a changé |
|---|---|
| `stokesforge-core` | `ColumnType::Mixed`, `MixedCell { Empty, Number, Text }`, `ColumnValues::Mixed`, `ValueBlock::Mixed`, les six mutateurs, les conversions dans les deux sens, `Column::cell`, `Column::numeric_view`, `ColumnValues::number_at` ; `accepts_child` corrigé (voir les bugs) |
| `stokesforge-commands` | `Command::SetCellsPromoting` et ses trois chemins ; `check_writable_cells`/`write_cells` factorisés ; empreinte mémoire d'un buffer mixte |
| `stokesforge-persistence` | `TypeRecord::Mixed`, `ValuesRecord::Mixed` en trois tableaux parallèles, et les deux erreurs de cohérence qui vont avec |
| `stokesforge-session` | `write_formula_column` conforme son bloc au type de la colonne |
| `stokesforge-app` | saisie tolérante, affichage d'une cellule mixte, collage TSV, suffixe de type |
| `stokesforge-script` | messages d'erreur qui nomment le vrai type |
| `stokesforge-render` | **rien** — c'est le résultat, pas un oubli |

Le fait que `stokesforge-render` soit vide dans ce tableau est la propriété qui
compte : la traduction « cellule mixte → `f64` traçable » a lieu une fois, dans
`Column::numeric_view`, et le pipeline — donc les deux backends — n'a rien
appris de nouveau (invariant 5).

## Les bugs rencontrés en chemin

Trois, dont un sérieux. Aucun n'était causé par ce chantier ; les trois étaient
là avant, et c'est le chantier qui les a mis en lumière.

### 1. Une nature non reconnue sous une feuille perdait tout le fichier

**Sérieux, corrigé.** C5 (niveau 2) promet qu'une variante inconnue lue d'un
`.stf` borne sa perte à *ce nœud* (invariant 10). La promesse ne tenait que
pour un nœud rangé dans un **dossier** : `NodeKind::Unknown` compte comme
fenêtre, donc `Folder::accepts_child` l'acceptait — mais `Workbook` et
`Worksheet`, non. `unknown.rs` faisait correctement son travail, rangeait le
nœud en `Unknown`, et la validation d'arbre de `load` le refusait aussitôt :
`InvalidModel`, fichier entier perdu.

Découvert en écrivant le test qui devait *prouver* la stratégie de migration —
lequel a échoué du premier coup, avec le message « nœud 5 est mal rangé : une
feuille ne contient que des colonnes ». Sans ce test, la note de compatibilité
de [`persistance.md`](persistance.md) aurait été fausse et personne ne l'aurait
su avant qu'un utilisateur n'ouvre un fichier d'un collègue.

Les trois conteneurs acceptent désormais une nature non reconnue.

### 2. Un recalcul de formule avalé en silence

**Latent, corrigé.** `actor::write_formula_column` écrivait toujours un
`ValueBlock::Float`, et `let _ = project.set_values(...)` jetait le refus. Tant
que toutes les colonnes étaient numériques, le refus n'arrivait jamais ; une
formule posée sur une colonne mixte l'aurait rendu réel — et invisible. Le bloc
est maintenant conformé au type de la colonne avant l'écriture.

### 3. « la colonne est de type texte » pour une colonne qui ne l'est pas

**Cosmétique, corrigé.** `pyst.column` et `pyst.set_column` refusaient toute
colonne sans buffer `f64` avec le message « est de type texte ». Exact tant que
`Text` était le seul cas ; faux ensuite. Les deux messages nomment le vrai
type, via un `ColumnType::type_name` ajouté au cœur.

### Ce qui n'était pas un bug

Deux tests figeaient l'ancien comportement (`shell.rs`,
`invalid_numeric_input_reports_an_error_and_sends_no_command` ; `b15`,
`..._est_refuse_sans_ecrire`). Ils ont été **réécrits, pas supprimés** : ils
gardent la propriété qui compte — aucune donnée ne disparaît — et affirment
désormais l'inverse en surface, parce que le comportement a délibérément
changé.

Par ailleurs, les six plafonds de
`docs/archive/rustrewrite/questions-ouvertes.md` §11 ont été vérifiés avant
toute chose : `MAX_COLUMN_ROWS` est le seul que ce chantier touche, il
s'applique aux colonnes mixtes par la même fonction (`write_block`,
générique), et rien n'a été « corrigé » qui n'était qu'un plafond documenté.

## Ce qui reste

### Côté test

Les trous de test recensés à la clôture du chantier sont comblés, sauf un —
la feature `python`, restée **non vérifiée** (voir la note dédiée après le
tableau, pas un problème de code mais d'accès réseau dans la sandbox qui a
traité cette tâche). Le détail, gardé pour mémoire :

| Où | Ce qui manquait | État |
|---|---|---|
| `b16` (séries résolues) | aucun cas mixte : le préfixe commun, les colonnes d'erreur trop courtes et la troncature ne sont éprouvés que sur des colonnes homogènes | **comblé** — trois cas ajoutés (colonne mixte complète, colonne mixte comme X plus courte que Y avec texte/vide au milieu, colonne d'erreur mixte trop courte, colonne d'erreur mixte vide) ; les deux anomalies déjà documentées (troncature du signal, série disparue) se reproduisent à l'identique une fois passées par `numeric_view` |
| `b17` (coût des opérations) | `numeric_view` est **la seule lecture du modèle qui copie**, et rien ne le mesure ni ne garde-fou contre une régression — par exemple quelqu'un qui l'appellerait par point de tracé au lieu d'une fois par résolution | **comblé** — un garde-fou compare le coût d'une résolution complète à celui d'un unique appel à `numeric_view`, et à celui d'un contre-exemple simulant des rappels par point |
| `b21` (coût et identité) | `SetCellsPromoting` absente. La propriété « le chemin coûteux n'a lieu qu'une fois par colonne » n'est aujourd'hui affirmée que par un test de `shell.rs`, jamais **mesurée** | **comblé** — mesure directement que le chemin 3 (promotion) coûte proportionnellement à la taille de la colonne, et que les chemins 1/2 (colonne déjà mixte) n'en dépendent pas |
| `memory_pinning` | aucun scénario avec une colonne mixte ; `mixed_buffer_bytes` est une estimation calquée sur `text_buffer_bytes`, non éprouvée | **comblé** — scénario 8 : une colonne mixte supprimée reste retenue tant que son entrée est sur la pile puis libérée à l'éviction (même schéma que le scénario 6), et `History::retained_bytes()` correspond exactement à l'estimation de `mixed_buffer_bytes`, recalculée indépendamment dans le test |
| `b10` (compteur de version) | pas de cas mixte | **comblé** — la saisie tolérante qui promeut une colonne, puis une seconde saisie sur une colonne déjà mixte, avancent toutes deux le compteur |
| `b2` (persistance) | le cas mixte vit dans `roundtrip.rs`, pas dans la batterie | **comblé** — ajouté à `b2_aller_retour_preserve_tout`, qui compare déjà type/méta/version/cellules nœud par nœud |
| feature `python` | **non compilée par défaut, donc non vérifiée.** `float_values()` rend `None` pour une colonne mixte, ce qui *devrait* donner le refus attendu côté `pyst`, mais aucun test ne l'exerce. À lancer avec `scripts/python-test.sh` | **toujours ouvert** — voir la note dédiée ci-dessous |
| feature `typst` (supprimée depuis : Typst est une dépendance ferme, issue #29) | non compilée non plus. Aucun code sous `typst` ne touche aux colonnes (vérifié), donc risque faible — mais non nul | **vérifié** — `cargo build`/`cargo test` avec `--features typst` sur `stokesforge-render`, `stokesforge-render-svg` et `stokesforge-render-egui` passent tous, sans aucun changement de code requis : confirme que rien sous `typst` ne touche aux colonnes |

**Note sur la feature `python`.** `scripts/python-test.sh` exige un
interpréteur CPython embarqué que `scripts/python-vendor.sh` télécharge
(`vendor/python/lock.env` : CPython 3.12.14, ~90 Mo, plus une roue numpy
épinglée). La sandbox qui a traité cette tâche a un accès réseau vers
`github.com` (vérifié par un simple `curl`), mais le débit vers
`objects.githubusercontent.com` (où GitHub redirige le téléchargement de
l'archive de release) était de l'ordre de 100 à 150 Ko/s — l'archive seule
n'a pas fini de télécharger après plusieurs minutes, et le script échoue si
interrompu à mi-chemin (pas de reprise : il réécrit `$ARCHIVE.part` depuis
zéro). Le test qui manque (`float_values()` rend `None` pour une colonne
mixte → `pyst.column`/`pyst.set_column` doivent le refuser en nommant le bon
type, comme les deux autres refus déjà couverts pour `Text`) **n'a donc pas
pu être écrit ni exécuté sous cette feature dans cette tâche.** Le code visé
(`stokesforge-python`/`stokesforge-script`) n'a pas changé depuis la clôture du
chantier ; rien n'indique un problème de correction, seulement une lacune de
couverture qui reste à combler dès qu'un environnement avec un débit réseau
suffisant (ou l'interpréteur déjà vendorisé) est disponible.

### Côté fonctionnalité

- **L'import ne produit jamais de colonne mixte.** `stokesforge_import::ParsedColumn`
  porte un `Vec<f64>` : un jeton illisible y devient `NaN`, exactement la perte
  que ce chantier existe pour éviter. C'est **le** cas d'usage motivant, et il
  n'en profite pas encore — il relève du chantier 2 (prévisualisation d'import),
  délibérément hors périmètre ici. C'est la première chose à faire.
- **Aucune UI pour changer le type d'une colonne.** `Command::SetColumnType`
  n'est dispatché nulle part dans `stokesforge-app` ; on ne peut rendre une colonne
  mixte que par la saisie tolérante, un script ou un fichier. Antérieur à ce
  chantier, mais plus visible maintenant qu'il y a un quatrième type.
- **`format_f64_for_display` existe en double** — dans `stokesforge-core::column` et
  dans `stokesforge-app::state`, avec des seuils différents. Le guide de robustesse
  (§F8, « une seule table, deux directions ») demandait leur unification une
  fois les deux moitiés du correctif C4 fusionnées ; elles le sont, l'unification
  ne l'est pas. Sans rapport avec les colonnes mixtes, mais c'est le moment de
  le noter.
- **Le pont Python refuse les colonnes mixtes**, délibérément (voir plus bas).
  Si un besoin apparaît, la bonne réponse est une surface distincte qui dit
  qu'elle copie, pas un assouplissement de `pyst.column`.

## Décisions à ne pas rouvrir sans raison nouvelle

- **`Mixed` est un quatrième type, pas une refonte des trois autres.** Faire de
  toute colonne un `Vec<MixedCell>` coûterait 32 octets par ligne au lieu de 8
  et retirerait le buffer contigu dont dépendent la vue numpy sans copie et la
  résolution zéro-copie.
- **`FORMAT_VERSION` ne bouge pas.** L'ajout est additif et la dégradation d'un
  lecteur antérieur est testée. Incrémenter aurait rendu illisibles par les
  anciens lecteurs *tous* les fichiers suivants, colonne mixte ou pas.
- **Trois tableaux parallèles au disque, pas un tableau d'énumérations.**
  `to_vec_named` écrit le *nom* d'une variante : sept octets de `"Number"`
  devant chaque valeur de huit.
- **`Cell` est inchangé.** Une cellule mixte se lit dans le vocabulaire commun
  (`Column::get`) ou telle qu'elle est stockée (`Column::cell`) ; ajouter une
  troisième variante à `Cell` aurait touché tous ses lecteurs pour une
  distinction dont un seul a besoin.
- **`pyst.column` refuse une colonne mixte.** Elle rend une vue *sans copie* ;
  lui rendre la copie matérialisée de `numeric_view` serait une vue qui ne suit
  plus la colonne — un piège silencieux pour un script qui croirait tenir le
  buffer.
