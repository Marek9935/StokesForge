# Questions ouvertes du socle Rust — à trancher, expliquées sans Rust

Statut : **ouvert**, rédigé le 2026-09-08 à l'issue de la revue technique du
`cahier-des-charges-technique.md`. Aucune de ces questions ne bloque le
socle ; chacune finira par se poser, et chacune coûte plus cher à trancher
tard qu'à trancher tôt.

**À qui s'adresse ce document.** Aux mêmes yeux que les autres, mais sans
supposer qu'ils lisent du Rust. Chaque question est posée en termes de
*comportement observable* — ce que l'utilisateur ou un script verra — et pas
en termes de types. Le §0 traduit les quelques mots de Rust qui traversent
les autres documents ; le reste s'appuie dessus.

Comme partout ailleurs dans ce dossier, les options écartées restent
écrites. On ne supprime pas une option rejetée : on garde pourquoi.

---

## 0. Petit lexique — les mots de Rust qui reviennent, dits en Python

Ce tableau n'est pas une leçon de Rust ; c'est juste de quoi lire les autres
documents sans buter.

| Terme Rust | L'équivalent le plus proche en Python | Ce qu'il change ici |
|---|---|---|
| `Vec<f64>` | un `array` numpy de `float64` | un bloc de nombres contigu en mémoire, exactement la même chose des deux côtés |
| `String` | une `str` | une chaîne ; en Rust chaque chaîne est un petit bloc alloué à part, ce qui compte quand on en copie 10 000 |
| `HashMap<K, V>` | un `dict` | même chose : une table clé → valeur |
| `Arc<T>` | **un objet Python ordinaire** | c'est *exactement* le modèle de Python : plusieurs noms peuvent désigner le même objet, un compteur de références suit combien, et l'objet meurt quand le compteur tombe à zéro. `Arc` = « objet Python » rendu explicite |
| `Arc::make_mut` | (pas d'équivalent direct) | « si je suis le seul à tenir cet objet, je le modifie sur place ; si quelqu'un d'autre le tient aussi, j'en fais d'abord une copie pour moi ». C'est ce qui protège la vue numpy d'un script — §3 ci-dessous |
| `crate` | un *package* / un module importable | une unité de compilation. La particularité : Rust **interdit à la compilation** qu'un crate en importe un autre s'il n'est pas déclaré — c'est ce qui rend vérifiable « le cœur ne connaît pas Python » |
| *feature flag* | un `extra` de `pip install paquet[extra]` | un morceau de code compilé seulement si on le demande. Ici : tout ce qui touche à Python, pour que les tests tournent sur une machine sans Python |
| *borrow* / emprunt | passer un objet à une fonction sans le copier | Rust vérifie à la compilation que l'objet emprunté vit plus longtemps que l'emprunt. Le cas numpy (§3) est celui où cette vérification **ne peut pas** se faire toute seule, parce qu'un des deux côtés est Python |
| GIL | le GIL | le même, littéralement : c'est CPython qui est embarqué |
| `Result<T, E>` | une fonction qui renvoie `(valeur, erreur)` et qu'on **doit** tester | Rust n'a pas d'exceptions : une fonction qui peut échouer le dit dans son type, et le compilateur refuse qu'on ignore le cas d'erreur |
| `unsafe` | (rien en Python) | un bloc où le programmeur promet quelque chose que le compilateur ne peut pas vérifier. Rare, et toujours accompagné d'une clause écrite à honorer. On en a **un seul** dans tout le socle : §3 |

---

## 1. Le cahier des charges API dit « UUID », le modèle dit `NodeId(u64)`

**Ce dont on parle.** Un script référence un objet du projet de deux
façons : par chemin (`[Book1]Sheet1!ColA`) ou par une **poignée** qu'il
garde dans une variable, et qui continue de désigner le bon objet même si
l'utilisateur renomme ou déplace la colonne entre-temps.

```python
col = pyx.resolve("[Book1]Sheet1!ColA")   # la poignée
# ... l'utilisateur renomme la colonne dans l'UI ...
pyx.column(col)                            # doit toujours marcher
```

`cahier-des-charges-api-script.md` §3 décrit cette poignée comme un « UUID
stable » — un héritage de l'`ObjectRegistry` Python, qui en utilisait un
vrai. Le socle Rust utilise à la place un **entier** qui s'incrémente et
n'est jamais réutilisé (`cahier-des-charges-technique.md` §4.1).

**Ce que ça change vraiment : rien, sauf le mot.** Ce qui est contractuel
pour un script, c'est que la poignée soit **opaque** (on ne l'inspecte pas,
on ne la fabrique pas soi-même) et **stable** (elle survit à un renommage,
à un déplacement, et à une suppression suivie d'un `undo`). Un entier
satisfait les trois. La seule différence pratique : un UUID est unique
*entre projets différents*, un entier ne l'est qu'à l'intérieur d'un
projet.

| | UUID (texte, 128 bits) | Entier monotone (retenu) |
|---|---|---|
| Opaque pour le script | oui | oui |
| Survit renommage / déplacement / `undo` | oui | oui |
| Unique **entre deux projets distincts** | oui | **non** |
| Taille en mémoire, par nœud | 16 octets + allocation | 8 octets |
| Lisible dans un message d'erreur | non (`3f2a…`) | oui (`nœud 412`) |

**La seule chose à trancher : faut-il l'unicité inter-projets ?** Elle ne
servirait qu'à une fonction qui n'existe pas et n'est pas demandée —
fusionner deux fichiers `.plx` en gardant les identités des deux côtés.

**Recommandation** : garder l'entier, et corriger le mot « UUID » dans le
cahier des charges API le jour où ce document sera rouvert. Ne pas le
rouvrir seulement pour ça : le contrat, tel qu'il est écrit, est déjà
respecté.

**Quand ça doit être tranché** : avant la première ligne de `plotyx.api`
v1, pas avant. Le socle n'expose que quatre appels (spec §8.5) et ils
n'affichent jamais la poignée.

---

## 2. Que fait `pyx.undo()` quand il n'y a rien à annuler ?

**Ce dont on parle.** Un script peut demander une annulation :

```python
pyx.set_column(col, valeurs)
pyx.undo()          # annule ce qu'on vient de faire
pyx.undo()          # ... et si la pile est vide ?
```

Le cahier des charges API (§7) liste cinq exceptions nommées ; **aucune**
ne couvre ce cas. Ce n'est pas un oubli anodin : c'est le genre de trou
qu'on bouche par accident, différemment côté UI et côté script.

| Option | Comportement | Pour | Contre |
|---|---|---|---|
| **a. Renvoyer `False`** | `if not pyx.undo(): ...` | s'écrit naturellement dans une boucle « annuler tant qu'il y a quelque chose » ; c'est ce que fait l'UI (bouton grisé) | une valeur de retour ignorée passe inaperçue |
| b. Lever une exception | `EmptyHistoryError` | impossible à ignorer | oblige un `try/except` autour d'un cas parfaitement normal ; contraire au principe « une exception par *cause d'erreur* » du §7 — une pile vide n'est pas une erreur |
| c. Ne rien faire, silencieusement | `None` | le plus simple | un script ne peut pas savoir s'il a annulé ; le seul comportement vraiment mauvais |

**Recommandation** : **option a**. Elle aligne le script sur l'UI (où
« rien à annuler » se traduit par un bouton grisé, pas par un message
d'erreur) et elle garde le vocabulaire d'exceptions du §7 réservé aux
vraies erreurs.

**Question jumelle, à trancher en même temps** : `pyx.redo()`, évidemment
— et `pyx.undo()` doit-il pouvoir annuler une commande **lancée depuis
l'UI**, pas seulement depuis le script ? Le socle n'a qu'une seule pile
d'historique (spec §5), donc la réponse par défaut est **oui**, et c'est
exactement ce que teste le critère de fin du §1.1. À écrire noir sur blanc
plutôt qu'à laisser déduire.

**Quand ça doit être tranché** : phase 5, tâche 5.5 — c'est un des quatre
appels de la surface-témoin.

---

## 3. Une vue numpy qui « vieillit » sous le script

**Ce dont on parle.** C'est le cœur technique du socle, et il se raconte
sans une ligne de Rust.

Quand un script lit une colonne, il ne reçoit **pas une copie** : il reçoit
un tableau numpy qui pointe directement sur la mémoire du modèle. C'est
tout l'intérêt — 1 million de points, zéro copie, zéro attente.

```python
v = pyx.column(col)      # v regarde directement la mémoire du modèle
```

Maintenant, pendant que `v` existe, quelqu'un modifie la colonne — depuis
l'UI, ou depuis le script lui-même. Que voit `v` ?

**La réponse retenue : `v` continue de voir les *anciennes* valeurs.** Le
modèle, en détectant qu'un lecteur tient encore l'ancien bloc, écrit dans
un **bloc neuf** et laisse l'ancien intact aussi longtemps que `v` vit.
C'est le mécanisme `Arc::make_mut` du lexique. Techniquement c'est le seul
choix sûr : muter le bloc sous les pieds de numpy serait un plantage, pas
une valeur surprenante.

```python
v = pyx.column(col)
print(v[0])              # 1.0
pyx.set_column(col, autres_valeurs)
print(v[0])              # 1.0  ← toujours l'ancienne valeur !
print(pyx.column(col)[0])# 2.0  ← la nouvelle
```

**Ce qu'il reste à trancher, ce n'est donc pas le mécanisme — c'est ce
qu'on en dit à l'utilisateur.** Trois politiques possibles :

| Option | Ce que le script voit | Pour | Contre |
|---|---|---|---|
| **a. Documenter et ne rien faire** | l'exemple ci-dessus, en l'état | zéro coût, zéro surprise pour qui a lu la doc ; c'est déjà le comportement d'une vue numpy sur un tableau qu'on remplace | quelqu'un se fera avoir un jour, et le bug sera silencieux (des valeurs justes, mais périmées) |
| b. Invalider la vue | `v[0]` lève une exception après toute écriture | l'erreur devient bruyante au lieu d'être silencieuse | numpy ne permet pas d'invalider un tableau proprement ; il faudrait rendre un objet enveloppant, donc **perdre** le fait que `v` soit un vrai `ndarray` — ce que toute la bibliothèque scientifique attend |
| c. Vue « vivante » qui se met à jour | `v[0]` renvoie 2.0 | ce à quoi on s'attend naïvement | impossible sans copier ou sans verrou : c'est précisément ce que le zéro-copie interdit |

**Recommandation** : **option a**, avec deux garde-fous d'écriture, pas de
code — (i) le dire dans la docstring de `pyx.column()`, avec cet
exemple-là ; (ii) écrire dans le guide utilisateur la règle simple qui
évite tout le problème : *ne gardez pas une vue plus longtemps qu'un
calcul ; relisez la colonne après toute écriture.*

**Ce qui n'est pas ouvert** : la vue est en lecture seule
(`writeable = False`). Écrire dedans directement ne doit pas marcher, et
toute écriture passe par `pyx.set_column()`, donc par le bus, donc par
Ctrl+Z. C'est acté (spec §8.3, cahier API §2.3).

**Quand ça doit être tranché** : phase 5, tâche 5.2 — au moment où le test
de durée de vie s'écrit, parce que ce test doit *affirmer* le comportement
choisi.

---

## 4. Lire un gros fichier sans tout charger — pas dans le socle, mais quand ?

**Ce dont on parle.** Aujourd'hui, ouvrir un projet le charge **entièrement
en mémoire** : 50 colonnes × 1M points = 400 Mo, avant d'afficher la
première cellule. C'est ce que fait déjà la version Python, et le socle
Rust le reproduit tel quel (spec §7).

Le futur format `.plx` (Zarr, `format-fichier-plx.md`) permettrait de faire
mieux : lire une colonne **par morceaux**, n'en charger que ce qu'on
affiche, et faire grandir un fichier au fil d'une acquisition sans le
réécrire en entier.

**Le point de vigilance** : la frontière figée du socle est
`load(chemin) → Projet complet`. Elle est compatible avec Zarr — on peut
changer le format sous elle sans rien casser au-dessus — mais **elle ne
donne aucun moyen d'exprimer un chargement partiel**. Le jour venu, la
lecture paresseuse s'ajoutera **à côté** de `load`, elle ne la remplacera
pas.

| Option | Quand | Coût |
|---|---|---|
| **a. Assumer le chargement intégral dans le socle** (retenu) | maintenant | aucun ; c'est le comportement d'aujourd'hui |
| b. Concevoir la lecture paresseuse tout de suite | phase 3 | fait porter au socle un besoin qui n'existe pas encore, et il faudrait décider *avant* d'avoir Zarr à quoi ressemble une colonne à moitié chargée |

**La vraie question, à poser quand elle se posera** : à partir de quelle
taille de fichier l'ouverture devient-elle pénible ? 400 Mo se chargent en
quelques secondes depuis un SSD ; le problème n'est pas la lecture, c'est
la RAM d'une machine qui en a 8 Go et deux projets ouverts. Tant que ce
n'est pas mesuré sur un cas réel, ce serait de l'optimisation à l'aveugle.

**Déclencheur à surveiller** : le mode acquisition live. Une colonne qui
s'allonge en continu est le seul usage qui rend la réécriture complète
franchement inacceptable — et c'est aussi celui qui décidera du sort du
« visualiseur léger » (§6).

---

## 5. Fusionner les frappes successives dans l'historique (« coalescence »)

**Ce dont on parle.** Dans un tableur, taper `1`, `2`, `3` dans une cellule
puis faire Ctrl+Z une fois efface les trois caractères d'un coup, pas le
`3` seulement. Le socle fait **le contraire** : chaque écriture est une
entrée d'historique distincte (spec §5.2, tranché le 2026-09-08).

**Ce n'est pas une question rouverte** — c'est un « pas ici » assumé. Ce
qui est consigné ici, c'est **le critère qui manquait** pour trancher, afin
de ne pas rejouer la discussion à zéro quand l'UI arrivera :

Fusionner deux écritures demande de répondre à *quand* elles se
ressemblent assez, et il y a quatre réponses possibles, incompatibles
entre elles :

| Critère de regroupement | Ce que ça donne | Où ça casse |
|---|---|---|
| Fenêtre de temps (< 1 s) | proche d'un tableur | un script qui écrit vite fusionnerait des opérations sans rapport |
| Même cellule | intuitif pour la frappe | ne fusionne rien lors d'un collage sur une plage |
| Même colonne | fusionne un collage | fusionne aussi deux corrections faites à dix minutes d'écart |
| Même appelant (UI vs script) | la règle la plus sûre | demande que le bus sache **qui** appelle, ce que l'acteur ne distingue pas aujourd'hui |

**Ce qui manque pour choisir** : l'UI réelle. Le bon critère se voit en
tapant dans une grille, pas en le raisonnant. Et la dernière ligne du
tableau est une information architecturale à retenir dès maintenant : si le
critère retenu est « même appelant », il faudra que la requête envoyée à
l'acteur porte l'identité de son émetteur. Ça ne coûte presque rien à
ajouter plus tard, mais il faut savoir que c'est la seule des quatre
options qui touche au protocole du §6.

**Quand ça doit être tranché** : au chantier de l'UI réelle, jamais avant.

---

## 6. Le mode « visualiseur léger » (ex-PyQtGraph) — garder ou abandonner

**Ce dont on parle.** La version Python a deux moteurs de tracé :
Matplotlib (par défaut depuis le 2026-09-07) et PyQtGraph, gardé comme
« visualiseur léger » pour l'acquisition live. Le portage Rust n'embarque
pas PyQtGraph : la question est de savoir si ce mode doit exister
autrement, ou disparaître.

**Décidé le 2026-09-08 : le choix est reporté au chantier de l'UI réelle**,
et ce report est sans risque — aucun crate du socle n'en dépend.

**Ce qu'on sait déjà, et qu'il ne faut pas redécouvrir** :

- le seul argument technique différenciant de PyQtGraph était `useOpenGL`,
  et il **n'est activé nulle part** dans le code actuel
  (`resume-decisions.md` §8) — c'est-à-dire que l'avantage revendiqué n'a
  jamais été mesuré, et encore moins utilisé ;
- Matplotlib tient 141 ms pour 1M de points et 0,2 ms pour le réticule
  (mémoire projet `plotyx-matplotlib-default-renderer`) — ce qui reste
  lourd pour un rafraîchissement continu à 10 Hz ;
- donc le vrai critère n'est pas « quel moteur est le meilleur » mais
  **« l'acquisition live existe-t-elle vraiment comme besoin ? »**. Si oui,
  il faudra *un* chemin rapide, et rien n'oblige à ce que ce soit
  PyQtGraph — `egui` sait dessiner une courbe qui bouge sans passer par
  Python du tout.

**Quand ça doit être tranché** : au chantier de l'UI réelle, avec le
système de graphes sous les yeux.

---

## 7. La limite d'historique par défaut — un nombre à choisir

**Ce dont on parle.** La spec (§5.4) dit que l'historique est « configurable,
borné par défaut », sans dire **par combien**. Ce n'est pas un détail : la
limite est le seul garde-fou contre le piège mémoire du §2.1.1 — supprimer
un classeur de 400 Mo garde ces 400 Mo vivants tant que l'annulation reste
possible.

| Limite | Mémoire d'historique au pire | Ce que l'utilisateur ressent |
|---|---|---|
| 20 entrées | quelques Mo, sauf grosse suppression | « je ne peux pas remonter assez loin » |
| **100 entrées** | ~10 Mo de structure, plus les charges utiles retenues | l'ordre de grandeur des tableurs |
| illimité | jusqu'à plusieurs Go après quelques suppressions | rien... jusqu'au jour où la machine rame sans raison visible |

Le chiffre seul ne suffit pas : deux comportements complémentaires sont à
décider en même temps.

1. **Compter en entrées, ou en octets ?** Cent renommages ne pèsent rien ;
   *une* suppression de classeur pèse 400 Mo. Une limite en nombre
   d'entrées ne protège donc pas contre le vrai cas. Une limite en mémoire
   (« au plus 500 Mo d'historique ») protège, mais rend la profondeur
   d'annulation imprévisible pour l'utilisateur.
2. **Que se passe-t-il quand la limite est atteinte ?** L'entrée la plus
   ancienne est évincée silencieusement (comportement de `QUndoStack`
   aujourd'hui), ou l'utilisateur est prévenu ?

**Recommandation** : **100 entrées** en dur pour le socle, éviction
silencieuse de la plus ancienne — c'est ce que fait la version Python, donc
zéro régression ressentie — et **rouvrir la question d'une limite en
octets** seulement si un cas réel de saturation apparaît. Écrire le chiffre
dans la spec plutôt que de le laisser au code.

**Quand ça doit être tranché** : phase 2, au moment d'écrire la pile.

---

## 8. Le calendrier

Non chiffré, et volontairement. Les estimations par phase du `roadmap.md`
(≈3-5 jours, ≈2 semaines…) sont des ordres de grandeur de contenu, pas des
engagements de date.

**À faire avant tout engagement réel**, sur le modèle du roadmap phasé
Python existant. Le seul élément dur disponible aujourd'hui : la phase 5
(pont Python) contient le risque le plus lourd de l'architecture, et ses
deux premières tâches sont des **verrous** — tant que le hello-world
empaqueté ne tourne pas sur Linux **et** Windows, aucune estimation de la
suite n'a de valeur.

---

## Récapitulatif

| # | Question | Bloque quoi ? | À trancher |
|---|---|---|---|
| 1 | « UUID » vs entier dans le cahier API | rien | avant `plotyx.api` v1 |
| 2 | `pyx.undo()` sur pile vide | la surface-témoin | phase 5, tâche 5.5 |
| 3 | Politique de la vue numpy périmée | le test de durée de vie | phase 5, tâche 5.2 |
| 4 | Lecture partielle d'un fichier | rien | quand l'acquisition live devient un besoin |
| 5 | Coalescence des éditions | rien | chantier de l'UI réelle |
| 6 | Sort du « visualiseur léger » | rien | chantier de l'UI réelle |
| 7 | Limite d'historique par défaut | la pile | phase 2 |
| 8 | Calendrier | rien | avant engagement |
