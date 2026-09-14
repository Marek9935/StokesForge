# Application

## `plotyx-app` — la coquille UI

**Ce que ce crate n'est pas** : une ébauche de l'interface finale. Le docking,
la grille éditable à délégués, le panneau de propriétés et le système de
graphes seront repris entièrement plus tard ; rien ici ne doit les préempter.

Le banc est volontairement pauvre. Son seul rôle est de prouver que
l'intégration bout en bout tient — modèle → commande → annulation → sauvegarde
→ rechargement, dans une vraie fenêtre — **tant qu'une intégration ratée ne
coûte qu'une réécriture de ce crate**, et non celle de tout ce qu'on aurait
bâti par-dessus.

Toolkit : `eframe` / `egui`.

### Deux modules, une ligne de partage

| Module | Contenu | Testable en CI |
|---|---|---|
| `state` | **toute la logique**, sans egui, pilotée par un enum d'`Action` | oui (`tests/shell.rs`) |
| `app` | la glu `eframe` : traduit clavier et souris en actions, peint l'état | non |

Une fenêtre ne se teste pas en intégration continue. C'est toute la raison du
découpage : **rien de ce que contient `app` n'est une décision de produit**.

`Action` : `Undo`, `Redo`, `Save`, `Load`, `CommitCell`, `AddColumn`,
`InsertRows`, `DeleteRows`, `RunScript`, `InterruptScript`.

### Deux règles de conduite

1. **`AppState::dispatch` ne rend jamais d'erreur.** Toute erreur devient un
   `StatusMessage` affiché à l'utilisateur. Un banc d'essai qui meurt sur une
   saisie invalide ne prouve rien. Nuance portée par le type : « impossible
   pour l'instant » (annuler à vide) n'est **pas** une erreur ; un chemin
   illisible en est une.
2. **L'instantané détenu est figé** (l'arbre est persistant). Il faut le
   redemander à l'acteur après chaque mutation — ce que fait `refresh`, appelé
   par `dispatch` après toute action ayant pu changer le modèle.

### La console de script ne fige pas la fenêtre

`ScriptEngine::run` **bloque** jusqu'au terme du script. L'appeler depuis la
boucle de rendu figerait la fenêtre entière, bouton « Arrêter » compris — le
défaut le plus visible qu'une console de script puisse avoir.

La parade vit dans `state`, pas dans `app`. Trois décisions, prises là et
nulle part ailleurs :

1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
   `AppState::new` ne touche pas à `ScriptEngine` ; seul le premier
   `RunScript` le construit. Le temps de démarrage de l'interpréteur est l'un
   des motifs du portage — le perdre à l'ouverture annulerait le bénéfice.
   Verrouillé par `tests/script_bridge_lazy.rs`.
2. **`RunScript` ne bloque jamais l'appelant** : le travail part sur un **fil
   détaché** (création paresseuse du moteur comprise) et l'appel revient
   aussitôt.
3. **`poll_script` est à appeler à chaque image** : il sonde le résultat par
   `try_recv` et l'applique dès qu'il est prêt, rafraîchissement de l'instantané
   compris. C'est ce qui rend `dispatch` sûr à appeler depuis la boucle de
   rendu pour **toutes** les actions.

Sans la feature `python`, la console reste visible mais désactivée
(`AppState::script_available`). `ScriptOutput` est une copie **volontairement
indépendante** de `plotyx_script::ScriptOutcome`, pour que la surface publique
de `state` soit la même avec et sans la feature.

Une exception de script n'est pas une erreur d'application : sa trace Python
complète s'affiche, sans jamais marquer le statut en erreur.

### Ce que Qt donnait gratuitement, et qu'il faut écrire ici

Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci clavier,
l'entrée de menu, son grisage et son libellé (« Annuler *Renommer* »). Ici les
quatre sont séparés : le raccourci est déclaré et consommé à la main
(`SHORTCUT_UNDO` et compagnie), le grisage passe par `add_enabled`, le libellé
vient de `AppState::undo_label` — et **rien ne garantit leur cohérence sinon la
relecture**. C'est le point à vérifier en premier quand une action est ajoutée.

### Limites assumées du banc

- **Pas de sélecteur de fichiers natif** : le chemin se tape dans un champ de
  texte. Un vrai sélecteur veut une dépendance de plus (`rfd`) et, sous
  Wayland, un portail système.
- **Les raccourcis sont consommés au niveau du contexte**, donc *avant* les
  widgets. Ctrl+Z pendant la saisie d'une cellule annule la dernière **commande
  du modèle**, pas la dernière frappe : c'est le comportement attendu d'un
  tableur, et c'est aussi la limite — le banc ne sait pas distinguer les deux
  intentions.
- **Une seule cellule éditable à la fois.** La grille est virtualisée
  (`TableBuilder::body().rows()`) : une feuille peut avoir un million de
  lignes, il n'est pas question de garder un tampon par cellule.
- **Un seul graphe affiché à la fois.**

### Un détail qui a l'air d'un détail

`MISSING_TEXT_DISPLAY` vaut `\u{00A0}` — un espace insécable — pour afficher une
valeur texte **absente**. Il occupe la cellule comme un blanc ordinaire (rien ne
saute aux yeux) tout en restant, en mémoire, une chaîne différente de `""`, donc
distincte d'une chaîne vide **présente**. Un caractère visible (`—`, `∅`, `?`)
aurait été une réponse légitime à « qu'est-ce qu'un utilisateur pourrait vouloir
taper ici ? » ; un espace insécable n'est accessible à la frappe sur aucun
clavier usuel.

### Binaire

`plotyx-app [chemin]`. Si `chemin` existe, le projet est chargé depuis ce
fichier ; sinon un projet de démonstration est construit et `chemin` devient la
cible d'un futur enregistrement.

Deux variables d'environnement servent aux tests et aux captures :
`PLOTYX_APP_AUTOQUIT_MS` et `PLOTYX_APP_SCREENSHOT`.

### Tests

`tests/shell.rs` (la boucle complète, sans fenêtre), `tests/script_panel.rs`,
`tests/script_bridge_lazy.rs` (l'interpréteur ne démarre pas tout seul),
`tests/perf_write_under_snapshot.rs`.

## `plotyx-cli`

Binaire headless de vérification — **pas** l'UI. Il sert à valider que le
workspace se lie de bout en bout sans dépendre d'un choix d'UI. C'est un
squelette : il affiche une ligne et sort.
