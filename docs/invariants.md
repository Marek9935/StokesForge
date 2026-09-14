# Invariants

Les règles qu'on ne casse pas. Chacune est ici parce qu'elle traverse
plusieurs crates — donc parce qu'aucun commentaire de module ne peut à lui
seul la faire respecter.

La colonne qui compte est la dernière : **ce qui la tient**. Une règle tenue
par le compilateur ou par un test n'a pas besoin qu'on y pense ; une règle
tenue par la relecture se perd.

| # | Règle | Ce qui la tient |
|---|---|---|
| 1 | Aucune mutation du modèle hors d'une commande | la propriété : l'acteur détient le seul `&mut Project` |
| 2 | Aucune dépendance UI sous `plotyx-session` | `tests/no_ui_dependency.rs`, une liste blanche par crate |
| 3 | `plotyx-plot` ne connaît aucun moteur de rendu | sa seule dépendance est `plotyx-id` |
| 4 | Une seule source de mesure de texte | `plotyx-render/tests/mesure.rs` + le test d'accord |
| 5 | Une fonctionnalité de tracé n'entre pas dans un seul backend | `plotyx-render` est générique sur `DrawingBackend` |
| 6 | Le *feathering* n'entre pas dans la clé de cache | `plotyx-render-egui/tests/feathering.rs` |
| 7 | Identifiants jamais réutilisés | l'allocateur vit hors de l'arbre, dans `Project` |
| 8 | LIFO strict sur la pile d'historique | la structure de `History`, vérifiée par `tests/history.rs` |
| 9 | Aucun objet Python ne traverse le canal de l'acteur | le marqueur `Ungil` de PyO3, à la compilation |
| 10 | Un défaut de donnée dégrade, il ne plante pas | des états de première classe, pas des erreurs |

---

## 1. Aucune mutation du modèle hors d'une commande

`plotyx-core::Project` est la seule façade publique de mutation du modèle, et
toutes ses méthodes prennent `&mut self`. L'acteur de `plotyx-session` détient
le **seul `&mut Project` du processus** : un client n'a qu'un `Arc<Tree>` figé.

La règle n'est donc pas une convention à surveiller mais une propriété de
propriété. Pour muter, il faut passer par `SessionHandle::execute(Command)`,
et donc par la pile d'historique — donc être annulable.

Corollaire : ajouter une méthode `pub` mutante à `plotyx-core` sans commande
correspondante dans `plotyx-commands` ouvre un chemin d'écriture non
annulable. C'est ce que la batterie `b21` vérifie.

**À lire** : `plotyx-core/src/project.rs` (tête), `plotyx-session/src/lib.rs`
(tête).

## 2. Aucune dépendance UI sous `plotyx-session`

Plutôt que de deviner ce qu'est « une dépendance UI », chaque crate du socle
porte un `tests/no_ui_dependency.rs` qui **liste les dépendances autorisées**
et lit son propre `Cargo.toml`. Toute addition doit être ajoutée à la liste
explicitement : la décision devient consciente au lieu de se glisser par un
`use`.

Crates gardés ainsi : `plotyx-core`, `plotyx-plot`, `plotyx-commands`,
`plotyx-persistence`, `plotyx-session`, `plotyx-script`.

`plotyx-render-svg` porte l'équivalent sous un autre nom
(`tests/frontiere.rs`) : il ne doit voir ni `egui`, ni `eframe`, ni `winit` —
c'est ce qui fait que **le rendu se teste sans serveur graphique**, donc en
intégration continue.

## 3. `plotyx-plot` ne connaît aucun moteur de rendu

Un `PlotSpec` dit *ce qu'il faut tracer*, jamais *comment*. Sa seule
dépendance est `plotyx-id` : ni `plotters`, ni `egui`, ni `typst`, ni `serde`
(la persistance a ses propres types de transport et fait elle-même la
conversion).

Conséquence concrète, et c'est la raison d'être de la règle : le vocabulaire
du spec est le sien. Pas de `'o'`, pas de `linestyle='--'`, pas de
`PenStyle` — des énumérations nommées, dont la table de traduction vit dans le
renderer. La couche équivalente de la version Python a survécu à un changement
complet de renderer sans qu'une ligne bouge.

**À lire** : `plotyx-plot/src/lib.rs` et `src/style.rs` (têtes).

## 4. Une seule source de mesure de texte

C'est le **seul endroit du chantier où un bug est silencieux**, et c'est ce
qui justifie le poids qu'on y met.

Tout ce qui répond à « quelle place prend cette chaîne ? » passe par
`plotyx_render::text`, quel que soit le backend qui pose la question. Ce qui
est partagé, ce sont les `epaint::FontDefinitions` — pas l'instance
`epaint::Fonts`, qui n'est de toute façon ni `Clone` ni utilisable par `&self`.
Le danger n'est pas « deux instances », c'est « deux jeux de polices ».

Le piège précis à connaître : `plotters` est pris **sans ses fonts**
(`default-features = false`). Un `DrawingBackend` qui ne surcharge pas
`draw_text` **panique** à la première graduation ; un qui ne surcharge pas
`estimate_text_size` mesure le texte au **nombre d'octets UTF-8** — « λ »
compte pour 2, « 111 » mesure comme « WWW » — et ne signale rien. Les marges
réservées aux libellés seraient fausses, donc la zone de tracé différente de
celle de l'écran : la figure exportée ne serait pas la figure vue.

Deux angles morts connus, tous deux traités : `configure_series_labels` et
`MultiLineText` de Plotters mesurent le texte **eux-mêmes**, par
`FontData::estimate_layout`, donc par l'heuristique naïve — jamais par
`estimate_text_size`. C'est pour cette raison précise que la légende est
écrite à la main dans `plotyx_render::legend`.

Trois tests tiennent la règle, et le troisième est celui sans lequel les deux
autres ne prouveraient rien :

- `plotyx-render/tests/mesure.rs` : deux mesureurs mesurent identiquement ; la
  mesure ne dépend pas du `pixels_per_point` ; et elle est **différente** de
  l'heuristique naïve — le témoin, sans lequel deux chemins également faux
  passeraient ;
- `plotyx-render-egui/tests/accord.rs` : la même figure, écran contre SVG,
  toutes les primitives comparées au pixel entier, plus deux familles de
  témoins ;
- `plotyx-render-egui/tests/formes.rs` : les conventions d'ancrage propres à
  chaque backend, que l'accord ne peut pas comparer directement.

**À lire** : `plotyx-render/src/text.rs` (tête), puis
`plotyx-render-svg/src/backend.rs` (tête) pour le piège vu du côté export.

## 5. Une fonctionnalité de tracé n'entre pas dans un seul backend

Le pipeline Plotters est écrit **une fois**, générique sur
`DB: DrawingBackend` (`plotyx_render::pipeline`). Les deux backends ne
diffèrent plus que par la façon de poser des pixels.

Un test peut constater un désaccord ; il ne peut pas empêcher qu'il naisse. Si
chaque backend appelait son propre `ChartBuilder`, la moindre divergence de
marge, de taille de police ou d'ordre d'appel deviendrait un désaccord à
trouver après coup.

Une seule exception vit aujourd'hui : `plotyx-render-egui::feathering`. Elle
n'est pas une entorse — `TessellationOptions` est un réglage de la pile egui,
en aval de la collecte des `Shape`, et un SVG ne triangule rien. Il n'y a
littéralement rien à mettre de l'autre côté.

## 6. Le *feathering* n'entre pas dans la clé de cache

Couper la frange d'anticrénelage change la **tessellation**, pas les `Shape` —
et la tessellation a lieu à chaque image de toute façon. Ajouter un terme à la
clé `(revision, rect, pixels_per_point, view_generation)` ferait reconstruire
toute la figure à chaque pression de bouton, c'est-à-dire payer cher
exactement au moment qu'on voulait alléger (le levier vaut 78 % du coût de
tessellation, mesuré).

À comparer avec `view_generation`, qui **doit** y être : un zoom change la
géométrie tracée, pas seulement sa finition.

Verrouillé par `plotyx-render-egui/tests/feathering.rs`.

## 7. Identifiants jamais réutilisés

Un `NodeId` est un compteur monotone, jamais réattribué sur toute la vie d'un
projet, **rechargement compris**. L'allocateur vit dans `Project`, **hors de
l'arbre**, précisément pour qu'une annulation qui restaure un ancien arbre
(`replace_tree`) ne le fasse pas reculer — sans quoi deux nœuds pourraient
porter le même identifiant.

À la frontière `u64::MAX`, `IdAllocator::next` **refuse** (`checked_add`)
plutôt que de déborder ou de reculer. Et `plotyx_persistence::load` refuse dès
la lecture tout identifiant au-delà de `MAX_NODE_ID` (10¹²) : très loin de
`u64::MAX`, mais assez tôt pour qu'un fichier falsifié ne puisse pas amener un
projet inutilisable en mémoire.

## 8. LIFO strict sur la pile d'historique

L'historique est hybride : la **structure** est retenue par instantanés
d'arbre, les **données** par deltas ([`commandes-et-historique.md`](commandes-et-historique.md)).
De cette asymétrie découle une contrainte non négociable : restaurer un arbre
entier écrase tout ce qui a été écrit après lui, donc une entrée structurelle
ne s'annule qu'une fois toutes les entrées postérieures annulées.

La pile le garantit par construction, et **dans une macro les entrées
s'annulent strictement en ordre inverse d'insertion**, quelles que soient leurs
natures. C'est ce qui fait tenir l'hybride.

## 9. Aucun objet Python ne traverse le canal de l'acteur

Trois règles, dont deux sont tenues par le compilateur :

1. le fil de l'interface ne touche jamais à Python ;
2. le fil de script relâche le GIL pendant qu'il attend l'acteur — chaque
   appel de la surface `pyx` traverse `Python::detach`, et le marqueur `Ungil`
   de PyO3 **interdit à la compilation** qu'une valeur liée au GIL entre dans
   le bloc d'attente ou en ressorte ;
3. l'acteur ne prend jamais le GIL — `plotyx-session` ne dépend pas de
   `plotyx-python`, le type d'un objet Python n'y serait même pas nommable.

Le chemin caché que ces trois règles ferment : une requête ou une réponse qui
transporterait un objet Python serait **détruite sur le fil de l'acteur** ; or
détruire un objet Python décrémente son compteur de références, ce qui est une
opération Python. D'où la formulation retenue — aucun objet Python ne traverse
le canal, ni à l'aller, ni au retour, ni comme champ.

Sans ces règles, le blocage mutuel est structurel : le script tiendrait le GIL
en attendant l'acteur, pendant que l'acteur attendrait le GIL.

## 10. Un défaut de donnée dégrade, il ne plante pas

La même politique, appliquée partout où une donnée peut être absente, inconnue
ou invalide. Elle vaut d'être énoncée en une fois parce qu'elle revient sous
quatre formes qui se ressemblent :

| Situation | Ce qui se passe |
|---|---|
| une série référence une colonne supprimée | `ResolvedSeries` existe toujours, `broken()` vrai, tracée en gris et annotée dans la légende |
| une variante d'énumération inconnue dans un `.plx` | capturée telle quelle dans un `RawVariant` ; la perte est bornée à ce nœud (ou cette série), pas au fichier |
| une formule Typst qui ne compile pas | le texte brut, `$…$` compris, s'affiche tel quel ; le graphe ne disparaît pas |
| une valeur manquante | `f64::NAN` (numérique, date-heure) ou `None` (texte, distinct de `Some("")`) |

Le fil commun : **un défaut visible et localisé, jamais un panic qui remonte**,
et jamais une série silencieusement vide. Renommer ou déplacer une colonne
référencée ne casse d'ailleurs rien du tout — une référence est un `NodeId`,
pas un nom.

Corollaire côté UI : `AppState::dispatch` ne rend **jamais** d'erreur ; toute
erreur devient un message de statut affiché.
