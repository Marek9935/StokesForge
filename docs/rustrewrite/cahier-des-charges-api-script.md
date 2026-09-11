# Cahier des charges — API `plotyx.api` exposée aux scripts

Statut : **spécification à figer avant implémentation**, pas encore
construite. Ce document définit le contrat entre le cœur applicatif (hôte —
aujourd'hui Python/Qt, potentiellement Rust demain, cf.
`resume-decisions.md`) et tout script Python exécuté dans la console
embarquée. Il prolonge une intention déjà présente dans le roadmap actuel
(phase 9 : *« API publique `plotyx.api` [...] chaque appel API passe par les
mêmes `QUndoCommand` que l'UI »*, et `ObjectRegistry`, dont le docstring
cite déjà la console de script comme cas d'usage des clés opaques). Rien
ici ne le contredit — ce document le précise et l'étend au cas d'un hôte
Rust.

## 1. Objectif

Donner à l'utilisateur un moyen d'automatiser et de scripter PlotYx
(imports en série, transformations de colonnes, génération de graphes,
export par lot) **sans jamais pouvoir mettre le modèle dans un état que
l'UI ne saurait pas afficher ou annuler**. L'API est le seul point d'entrée
autorisé — un script n'a jamais accès aux objets internes du modèle.

## 2. Principes directeurs (non négociables)

1. **Toute mutation passe par le command bus de l'hôte.** Un script qui
   modifierait une colonne ou un spec de graphe sans passer par une
   commande annulable est un bug bloquant, pas une optimisation acceptée —
   c'est la même règle que celle déjà actée pour la phase 9 Python, reprise
   ici sans exception pour un hôte Rust.
2. **Aucune référence directe à un objet interne du modèle n'est exposée.**
   Un script manipule des **valeurs** (tableaux numpy, chaînes, nombres) et
   des **poignées opaques** (chemins et clés, cf. §3), jamais des instances
   Rust/Python internes. Ça vaut dans les deux sens : ça protège l'hôte
   d'une mutation hors-bande, et ça protège le contrat API d'un changement
   de représentation interne (portage Rust y compris).
3. **Lecture sans copie quand c'est possible, jamais de surprise sinon.**
   Une colonne lue renvoie une vue numpy sur le buffer réel tant que le
   script ne la modifie pas ; toute écriture repasse explicitement par une
   commande (§4), jamais par une mutation en place de la vue retournée. Le
   contrat le documente noir sur blanc : modifier la vue renvoyée par
   `pyx.column(...)` sans appeler `.set_values(...)` est un comportement
   **non défini**, à interdire au niveau du type retourné si le langage
   hôte le permet (vue en lecture seule, `writeable = False` côté numpy).
4. **L'API est stable, versionnée, indépendante de l'implémentation
   hôte.** Un script écrit contre `plotyx.api` v1 continue de fonctionner
   à l'identique que l'hôte soit le Python/Qt actuel ou un futur cœur
   Rust exposant la même API via PyO3. C'est la condition qui rend le
   portage Rust compatible avec l'existant sans casser les scripts déjà
   écrits par l'utilisateur.
5. **Erreurs explicites, jamais de plantage silencieux.** Toute opération
   invalide (chemin inconnu, type de colonne incompatible, index hors
   plage) lève une exception Python nommée et documentée (§7) — jamais un
   crash du renderer ou un état incohérent laissé au modèle.

## 3. Adressage des objets — repris tel quel du modèle existant

Pas de nouvelle grammaire à inventer : l'API de script réutilise
**exactement** la grammaire de chemins déjà spécifiée dans
`plotyx/core/paths.py` (`[Book1]Sheet1!ColA`) et les clés opaques déjà
prévues par `ObjectRegistry` (UUID stable, indépendant du nom/de la
position, survit à un renommage/déplacement).

- Un script référence un objet **par chemin** (lisible, collable depuis
  l'explorateur de projet) ou **par clé** (stable, à garder dans une
  variable de script sur toute une session sans risque qu'un renommage la
  casse).
- Une clé résolue vers un objet supprimé puis restauré par `undo` reste
  valide (comportement déjà garanti côté registre — l'API de script en
  hérite sans travail supplémentaire).
- Une clé résolue vers un objet supprimé et **non restauré** lève
  `plotyx.api.BrokenReferenceError` à la prochaine utilisation — jamais un
  either silencieux.

## 4. Surface de l'API — v1

Regroupée par domaine. Chaque fonction est une façade fine vers une
primitive de mutation déjà fermée côté modèle (`Project.py` §« Frontière
des mutations ») — l'API de script n'invente aucune nouvelle mutation, elle
en expose un sous-ensomé nommé et documenté.

| Domaine | Fonction | Effet | Passe par le bus ? |
|---|---|---|---|
| Navigation | `pyx.active_sheet()` | Feuille active dans l'UI (ou `None`) | — (lecture) |
| Navigation | `pyx.resolve(path_or_key)` | Poignée opaque vers un objet | — (lecture) |
| Navigation | `pyx.list_children(handle)` | Liste des enfants d'un dossier/classeur | — (lecture) |
| Colonne | `pyx.column(handle)` | Vue numpy en lecture seule sur les valeurs | — (lecture) |
| Colonne | `pyx.set_column(handle, values)` | Réécrit les valeurs | oui |
| Colonne | `pyx.insert_column(sheet, index, name, values)` | Ajoute une colonne | oui |
| Colonne | `pyx.remove_column(handle)` | Retire une colonne | oui |
| Feuille | `pyx.insert_rows(handle, index, count)` | Insère des lignes | oui |
| Feuille | `pyx.delete_rows(handle, index, count)` | Supprime des lignes | oui |
| Structure | `pyx.rename(handle, new_name)` | Renomme (unicité validée par l'hôte) | oui |
| Structure | `pyx.move(handle, new_parent, index)` | Déplace dans l'arbre | oui |
| Graphe | `pyx.plot_spec(handle)` | Copie immuable du `PlotSpec` courant | — (lecture) |
| Graphe | `pyx.set_plot_spec(handle, spec)` | Remplace le spec (delta géré par l'hôte) | oui |
| Graphe | `pyx.plot(sheet, x, y, **style)` | Sucre : construit un spec simple et crée/peuple un graphe | oui |
| Export | `pyx.export(handle, path, **options)` | Rendu fichier (PNG/SVG/PDF/EPS/pgf) | — (aucun état modèle changé) |
| Session | `pyx.undo()` / `pyx.redo()` | Un cran de la pile hôte | oui (délègue) |
| Session | `pyx.begin_macro(label)` / `pyx.end_macro()` | Groupe plusieurs appels en une seule entrée d'historique | oui (délègue) |

Volontairement **non exposé en v1** (à ajouter seulement sur besoin réel,
jamais par anticipation) : suppression de dossier/classeur entier,
modification de la configuration d'export par défaut, accès aux
`Matrix`/`Note` (pas encore stabilisés côté modèle au moment de la
rédaction), tout ce qui toucherait aux préférences applicatives plutôt
qu'au contenu du projet.

### Concurrence optimiste sur les écritures de colonne

**À savoir avant d'écrire une boucle de script** : `pyx.set_column` peut
retenter d'elle-même, et peut échouer pour une raison qui n'a rien à voir avec
la valeur envoyée. Un script qui l'ignore peut buter dessus la première fois
qu'il tourne à côté d'un autre écrivain (l'UI, un autre script, un import).

Un script lit une colonne (`pyx.column`), calcule, puis écrit
(`pyx.set_column`) — mais entre les deux, un autre client de la session (l'UI,
un autre script) a pu modifier cette même colonne. L'hôte le détecte : chaque
colonne porte un numéro de version qui change à **toute** modification, quelle
qu'elle soit (pas seulement un changement de longueur). `pyx.set_column`
retient ce numéro au moment de la lecture et vérifie, **au moment d'écrire**,
qu'il n'a pas bougé.

- **S'il n'a pas bougé** : l'écriture est appliquée normalement, rien de
  visible ne change pour le script.
- **S'il a bougé** : l'écriture est refusée par l'hôte, `pyx.set_column`
  relit la colonne, laisse le script recalculer sur l'état à jour, et
  retente — jusqu'à **cinq tentatives**.
- **Après la cinquième tentative infructueuse** : `pyx.set_column` lève
  `ConcurrentWriteError` (§7) plutôt que de retenter indéfiniment ou d'écraser
  silencieusement le travail d'un autre écrivain.

Ce mécanisme concerne aujourd'hui `pyx.set_column` ; le même motif — lire un
instantané, décider, puis écrire — s'applique à toute fonction qui mute une
colonne (§4), donc `ConcurrentWriteError` doit être anticipée par tout script
qui écrit en boucle pendant qu'un autre processus peut toucher au même projet,
pas seulement par celui qui utilise `set_column` directement.

## 5. Exécution

- **Thread dédié**, jamais le thread UI — même famille de pattern que
  `PlotPrepareWorker` déjà en place pour la préparation de graphe.
- **Annulation** : un bouton « Stop » doit pouvoir interrompre un script en
  cours. Une boucle Python pure garde le GIL et ne se laisse pas interrompre
  proprement sans plomberie dédiée — prévoir explicitement le mécanisme
  d'interruption (signal coopératif vérifié périodiquement, ou
  interruption forcée du GIL côté hôte) plutôt que de le découvrir en test.
- **Sortie standard** : `print()`/tracebacks capturés et affichés dans la
  console, jamais perdus dans les logs du process hôte.
- Le résultat d'un script (valeur de la dernière expression, à la
  `qtconsole`) est affiché ; les exceptions non attrapées interrompent le
  script en cours **sans** laisser de macro d'historique à moitié appliquée
  — `begin_macro`/`end_macro` doit avorter proprement (mécanisme déjà
  prévu côté bus : `abort_macro`).

## 6. Modèle de confiance

Un script s'exécute avec les pleins pouvoirs de l'interpréteur (accès
fichiers, réseau, sous-process) — même modèle que les macros VBA d'Excel ou
LabTalk d'OriginLab. **Acceptable tant que** :

- les scripts exécutés sont écrits par l'utilisateur lui-même ou explicitement
  importés par lui, jamais chargés automatiquement depuis une source
  externe non vérifiée ;
- aucune fonctionnalité de partage de scripts entre utilisateurs n'existe.

**Si l'une de ces deux conditions change** (marketplace de scripts,
chargement automatique, scripts embarqués dans un fichier `.plx` partagé),
ce modèle de confiance doit être retranché explicitement avant
d'implémenter — pas découvert après coup. À noter dans le roadmap le jour
où l'un des deux cas se présente.

## 7. Erreurs — vocabulaire à stabiliser

Exceptions nommées, dans `plotyx.api.errors`, une par cause — pas
d'exception générique `RuntimeError` qui obligerait un script à parser un
message pour savoir quoi faire :

- `UnknownPathError` — chemin ne résolvant à aucun objet.
- `BrokenReferenceError` — clé valide autrefois, objet non restauré.
- `InvalidValueError` — type/forme de valeur incompatible avec la colonne
  ou le paramètre attendu.
- `NameConflictError` — renommage/insertion en collision avec un nom
  existant au même niveau.
- `MacroAbortedError` — levée par `end_macro()` si le script a interrompu
  la macro par une exception avant de l'avoir explicitement close.
- `ConcurrentWriteError` — une écriture de colonne (voir §4, « Concurrence
  optimiste ») a échoué cinq fois de suite parce qu'un autre client modifiait
  la même colonne entre chaque tentative. Ce n'est pas un bug du script ni une
  valeur refusée : la bonne réaction est de relire l'état courant, ou de
  retenter plus tard.

## 8. Compatibilité et versionnement

- `plotyx.api.__version__` exposé, incrémenté à tout changement de
  signature ou de comportement observable.
- Une fonction dépréciée reste appelable au moins une version majeure,
  avec un `DeprecationWarning` explicite — jamais une suppression silencieuse.
- Ce document fait foi ; toute extension de la surface (§4) passe par une
  mise à jour de ce cahier des charges **avant** l'implémentation, pas
  après.

## 9. Hors scope v1 — explicitement différé

- Complétion/introspection avancée dans la console (au-delà de ce que
  `qtconsole`/l'équivalent hôte fournit nativement).
- Scripts déclenchés automatiquement par un événement du modèle (hooks) —
  risque de boucle de rétroaction avec le command bus, à concevoir à part
  si le besoin apparaît.
- Accès à l'API depuis un processus externe (pas de serveur RPC prévu) —
  la console reste in-process.

## 10. Questions ouvertes, à trancher avant implémentation

- `pyx.plot(...)` (le sucre haut-niveau) doit-il créer un nouveau nœud
  `Graph` systématiquement, ou accepter un handle de graphe existant à
  peupler ? Non tranché.
- Faut-il exposer une variante asynchrone (`pyx.export_async`) pour un
  lot d'exports volumineux, ou l'utilisateur boucle-t-il lui-même sur
  `pyx.export` ? Non tranché — probablement inutile en v1, à réévaluer si
  un cas d'usage concret d'export par lot apparaît.
- Le format exact du delta retourné par `pyx.set_plot_spec` pour
  l'historique (spec complet vs delta) suit ce que fait déjà
  `Graph.set_plot_spec` côté modèle — à confirmer que rien de spécifique au
  script n'est nécessaire.
