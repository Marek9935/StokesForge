# Résumé des décisions — piste de portage Rust — 2026-09-07

Ce document consigne le raisonnement d'une conversation exploratoire sur un
portage éventuel de PlotYx vers Rust. **Rien n'est décidé ni engagé** : la
phase 6 (rendu Matplotlib) vient d'être livrée en Python le jour même (voir
`docs/decision-rendu-matplotlib-2026-09-07.md`) et reste la base active du
projet. Ce document sert de mémoire à la discussion, pour ne pas avoir à la
rejouer, et de point de départ si la piste est reprise plus tard.

## 1. Le motif initial et ce qui a été écarté

Point de départ : la RAM d'une appli Python peut monter vite sur un graphe à
1M de points — motif réévalué en cours de route.

**Écarté comme argument principal** : la RAM d'un tableau de 1M points n'est
**pas** un écart Python/Rust. Un `numpy.ndarray` de `float64` est un buffer C
contigu, identique à un `Vec<f64>` Rust (~8 Mo pour 1M points dans les deux
cas). Le piège réel (`list` Python de flottants boxés, ~4-5× plus lourd)
n'existe que si on sort des données de numpy — ce que le code actuel évite
déjà (`plot_renderer.py` : *« Aucun tableau n'est copié »*).

**Bug réel trouvé pendant l'échange**, qui illustre où la RAM fuit
réellement : voir `plotyx-orphaned-subwindow-bug` (mémoire projet) —
supprimer un nœud `Graph`/`Worksheet` ne ferme pas sa sous-fenêtre MDI, donc
`closeEvent`/`disconnect_signals` ne se déclenchent jamais, donc
`project.data_changed` garde une référence forte vers le `GraphWindow` et
tout ce qu'il retient (renderer, courbes, vues numpy). C'est une fuite par
**connexion signal Qt oubliée**, pas un problème de langage — mais c'est
une classe de bug qui existe spécifiquement à cause du modèle
signal/slot à référence forte, et qui **ne peut pas se produire de la même
façon** dans une UI en mode immédiat (rien à désabonner).

**Motif retenu au final, dans les mots de l'utilisateur** : stabilité dans
le temps, binaire cross-platform, performance générale — pas la RAM en tant
que telle.

## 2. Analyse Rust vs Python, resituée pour ce projet précis

- **Stockage brut** : égalité, cf. §1. Pas un argument de poids.
- **Stabilité runtime** : argument solide en faveur de Rust — le compilateur
  élimine à la compilation toute une classe d'erreurs qui, en Python,
  n'explosent qu'à l'exécution (`AttributeError`, `TypeError`, etc.).
  Nuance : le crash réellement rencontré cette semaine (édition du titre
  d'axe, `RuntimeError: already deleted`) vient de la sémantique de durée de
  vie des objets **Qt côté C++**, pas de Python — il referait surface à
  l'identique avec Rust+Qt (`cxx-qt`). Il ne disparaît qu'en quittant Qt
  pour une UI Rust native (egui/iced).
- **Stabilité de déploiement** : argument le plus solide et le moins
  discutable — un binaire statique n'a pas de dérive d'environnement dans
  le temps (pas de venv qui casse, pas de dépendance qui bouge à une
  réinstallation). Vrai que le langage hôte soit Python ou Rust, tant que
  l'artefact distribué est figé.
- **Vitesse générale** : réelle pour du code Python pur non vectorisé, mais
  peu pertinente ici — le chemin chaud de PlotYx (résolution de spec,
  downsampling, histogramme, peinture) est déjà natif (numpy, Agg/QPainter
  en C++). Réécrire l'orchestration en Rust n'accélère pas ce qui est déjà
  natif en dessous. Le gain visible serait le **temps de démarrage** (pas
  d'interpréteur/imports à amorcer) et la garantie qu'aucune boucle Python
  non vectorisée ne se glisse par accident dans le futur.
- **Pourquoi OriginLab/ChemDraw sont en C++** : pas un jugement de valeur
  contre Python — ces produits datent d'avant que Python soit une option
  viable pour du GUI desktop, et le choix reflète aussi des contraintes
  machine de l'époque et un modèle de vente sous licence (protection de la
  propriété intellectuelle par binaire compilé). Pas une preuve
  d'inadéquation de Python aujourd'hui (cf. napari, Orange3, Spyder — Python
  scientifique en prod, à l'échelle visée ici).
- **Le binaire prévu (`briefcase`, déjà en phase 0/12 du roadmap actuel)
  résout la dérive d'environnement, pas les bugs runtime.** Un bug logique
  comme celui de cette semaine se produirait à l'identique dans l'AppImage
  packagé — empaqueter n'est pas corriger.

## 3. egui vs iced

**egui retenu** comme le plus mature des deux pour cette forme d'appli :

- `egui_dock` (docking) est éprouvé en production sur un projet exigeant
  (Rerun.io) — la meilleure preuve de maturité disponible.
- iced (architecture Elm, plus élégante sur le papier) a un écosystème de
  docking plus faible et un historique de breaking changes plus marqué.

**Mais « mature comme toolkit » ≠ « composants prêts à l'emploi » pour ce
projet.** Ce qui manque par rapport à Qt, à reconstruire à la main :
- pas de thème équivalent à `qt-material` (style à construire soi-même) ;
- texte non rendu nativement par l'OS (plus faible en HiDPI/IME) ;
- accessibilité (lecteurs d'écran) nettement moins mature ;
- pas de menu natif par OS (egui dessine son propre menu dans la fenêtre) ;
- pas d'équivalent à `QTableView`+délégués pour une grille éditable
  (`egui_extras::TableBuilder` existe mais est plus pauvre) ;
- `egui_plot` très en retrait face à Matplotlib/PyQtGraph (légende, axes
  secondaires, styles par série) — argument décisif pour **ne pas**
  remplacer le renderer Matplotlib actuel par un équivalent natif Rust.

Point positif propre à egui, lié au §1 : en mode immédiat, pas de
connexions persistantes à désabonner — la classe de bug « fuite par signal
oublié » ne peut pas se produire de la même façon.

## 4. Architecture retenue si la piste est poursuivie

**Hybride, pas une réécriture totale.** Rust/egui possède tout ce qui est
interactif et permanent (modèle de données, command bus/undo, docking,
explorateur de projet, feuille de calcul). Le renderer Matplotlib —
**déjà construit et réglé le jour même** (141 ms pour 1M points, réticule
blitté à 0,2 ms via `copy_from_bbox`, voir `plotyx-matplotlib-default-renderer`
en mémoire projet) — est **conservé**, pas réécrit, et hébergé via un
interpréteur Python **embarqué et figé** :

- **PyO3** pour embarquer CPython dans le process Rust (pas de sous-processus
  par frame : le round-trip IPC serait trop coûteux pour du pan/zoom
  interactif, sachant que Matplotlib redessine déjà toute la figure à
  chaque interaction).
- **`rust-numpy`** pour passer les tableaux sans copie dans les deux sens —
  préserve la propriété « zéro copie » déjà acquise côté `plot_renderer.py`.
- Sortie : Matplotlib rend dans un canvas Agg hors-écran
  (`FigureCanvasAgg.buffer_rgba()`), le buffer RGBA est uploadé comme
  texture egui.
- Interpréteur **vendored et figé** (type `python-build-standalone`) +
  wheels numpy/matplotlib épinglées, un jeu par plateforme cible — pas de
  Python système, pas de `pip` exposé à l'utilisateur.

**Bénéfice de robustesse concret et spécifique à ce montage** : un appel
PyO3 renvoie un `PyResult` — une exception côté Matplotlib remonte comme une
`Err` Rust gérée à la frontière, au lieu de potentiellement dérouler dans
toute la boucle d'événements comme le crash de cette semaine. Contient le
bug plutôt que l'éliminer — distinction à garder honnête.

**Console de script** : mécanisme quasi gratuit une fois PyO3 en place pour
le renderer (même interpréteur, une deuxième porte d'entrée). Le
vrai travail est la surface d'API exposée — voir
`cahier-des-charges-api-script.md` dans ce même dossier. Ce besoin était
déjà anticipé dans le roadmap Python actuel (phase 9, `plotyx.api`,
`ObjectRegistry` explicitement commenté *« point d'appui [...] de la
console de script (phase 9) »*) — le cahier des charges rédigé ici en est
la continuité, pas une invention indépendante.

## 5. Ce que le compromis RAM/démarrage coûte réellement

Le budget déjà estimé dans `ROADMAP.md` (phase 12, ~150-300 Mo) est dominé
par numpy + Matplotlib, **pas** par PySide6/Qt. Passer à egui retire la part
Qt du poids et du temps de démarrage, mais **pas** la part scientifique —
à ne pas confondre avec un allègement global. Le gain net attendu se situe
sur le socle applicatif (UI), pas sur la stack de rendu, qui reste aussi
lourde qu'aujourd'hui.

## 6. Pinning d'un vieux Python — évalué comme sain

Un interpréteur **embarqué et vendored** (pas système) n'est exposé à
aucune dérive — traité comme une bibliothèque C figée dans le binaire.
Argument jugé solide. Seule réserve honnête : pas de correctifs de sécurité
CPython sur une version figée pendant des années — risque jugé faible pour
un outil desktop hors-ligne traitant ses propres fichiers de projet, mais à
ne pas balayer comme nul.

## 7. Faisabilité d'un portage assisté (jusqu'à l'équivalent phase 6)

Jugé faisable, cas favorable car **portage d'un système déjà spécifié et
testé** (664 tests Python encodent le comportement attendu), pas une
conception ouverte.

- **Bas risque, portage mécanique** : modèle de nœuds, command bus,
  undo/redo, arbre de projet.
- **Vrai travail, itératif** : docking egui, grille éditable, binding du
  panneau Properties, pipeline PyO3 + empaquetage d'un Python figé par
  plateforme. Attendre **un cycle build → lancer → capture d'écran →
  ajustement par composant majeur**, pas « quelques allers-retours » pour
  l'ensemble de l'UI — Qt absorbe des décennies de détails d'interaction
  qui devront être redécouverts un par un.
- Prochaine étape actée par l'utilisateur : rédiger le cahier des charges de
  l'API de script **avant** tout code Rust, parce que c'est un contrat
  (le casser casse les scripts utilisateurs) — contrairement au design des
  fenêtres, qui se traite mieux en itérant.

## 8. Questions ouvertes, non tranchées

- Le mode de confiance des scripts (exécution locale non sandboxée) reste
  valable tant qu'aucun partage de scripts entre utilisateurs ou chargement
  depuis une source externe non fiable n'est envisagé — à retrancher
  explicitement si ça change.
- Faut-il garder PyQtGraph/le mode « visualiseur léger » dans un portage
  Rust, ou l'abandonner (son seul argument différenciant, `useOpenGL`,
  n'est toujours pas activé nulle part à ce jour) ?
- Le calendrier réel d'un tel chantier n'a pas été chiffré — à faire avant
  tout engagement, sur le modèle du roadmap phasé existant.
