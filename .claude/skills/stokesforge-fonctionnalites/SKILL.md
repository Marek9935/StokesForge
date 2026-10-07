---
name: stokesforge-fonctionnalites
description: Répondre aux questions sur ce que StokesForge sait faire aujourd'hui, côté utilisateur — grille et saisie, types de colonnes, sélection, copier/coller, menus contextuels, raccourcis, import CSV/TXT/ASC, tracé depuis un classeur, fenêtre des propriétés des graphes (clic droit, menu Graphique), export, fenêtres, scripts Python, enregistrement `.stf`. À utiliser dès que la question est « est-ce que je peux… », « comment je fais pour… », « ça existe ? », « pourquoi ça ne marche pas comme dans Origin », « à quoi sert ce menu ». La réponse s'adresse à un **chimiste** venu d'OriginLab : Python correct, Rust nul — décrire des **gestes**, pas des fonctions, et vérifier dans le code avant d'affirmer qu'une fonctionnalité existe.
---

# Répondre sur les fonctionnalités de StokesForge

## À qui on parle

L'utilisateur est **chimiste**, vient d'**OriginLab**, connaît bien Python et
pas du tout Rust. Il veut savoir ce qu'il peut faire avec le logiciel, pas
comment c'est écrit.

- Décrire des **gestes** : « clic droit sur la rangée des lettres → Tracer →
  Ligne », pas « `AppState::plot_columns` appelle `series_from_columns` ».
- Comparer à **Origin** quand c'est éclairant : l'appariement X/Y des colonnes,
  la case en coin qui sélectionne tout le tableau, les quatre champs d'en-tête
  (Nom long, Unité, Commentaire, Formule) suivent volontairement ses
  conventions.
- Donner le **raccourci clavier** quand il existe : c'est souvent la vraie
  réponse à « comment je fais plus vite ».
- Ne citer un nom de fichier ou de fonction **que si l'utilisateur veut aller
  voir**. Sinon c'est du bruit.
- Si un terme technique est inévitable, le traduire en une demi-phrase. La
  table d'analogies Python vit dans le skill voisin :
  `.claude/skills/stokesforge-architecture/references/rust-pour-pythonistes.md`.

## La règle qui compte : vérifier avant d'affirmer

**Le code fait foi.** Trois sources induisent en erreur dans ce dépôt :

- **les notes de chantier** (`docs/chantier-*.md`, `docs/chantiers/`) sont des
  passations : elles listent parfois un « reste à faire » déjà fait, ou une
  intention jamais implémentée ;
- **`docs/archive/`** n'est plus maintenu et décrit des options **écartées** ;
- **les pages `docs/`** décrivent le code tel qu'il est, mais peuvent avoir
  pris du retard sur un commit récent.

Donc : une réponse du type « oui, ça existe » se vérifie par un `rg` dans le
crate concerné, ou par le test qui l'exerce. Une réponse du type « non, ça
n'existe pas encore » aussi — c'est la plus facile à se tromper.

## Où vérifier, par famille de question

| La question porte sur | Page | Code |
|---|---|---|
| grille, saisie, sélection, copier/coller, menus contextuels, raccourcis, fenêtres, thème, console de script | `docs/application.md` | `stokesforge-app/src/state.rs`, `app.rs` et `src/app/` (barre de menus : `app/menus.rs`), `selection.rs` |
| types de colonnes, valeurs manquantes, colonnes mixtes, noms et champs d'en-tête | `docs/modele-de-donnees.md` | `stokesforge-core` |
| annulation, macros, ce qu'un Ctrl+Z ramène | `docs/commandes-et-historique.md` | `stokesforge-commands/src/command.rs` |
| enregistrement, format de fichier, compatibilité | `docs/persistance.md` | `stokesforge-persistence` |
| tracé, axes, échelles, légende, export SVG/PDF/PNG, formules | `docs/rendu.md` | `stokesforge-render` |
| import CSV/TXT/ASC et aperçu | `docs/chantier-import-previsualisation.md` (passation) | `stokesforge-import/src/{recipe,parse,preview,store}.rs` |
| scripts Python, surface `pyst` | `docs/python-et-script.md` | `stokesforge-script`, `stokesforge-python` |

## Ce qui n'existe pas — le dire clairement

Repères #72–#74 : File › New Project (Ctrl+N, confirmation si travail non
enregistré), File › Add Graph (graphique vide annulable) et leurs boutons de
barre Fichier existent. Wayland natif est le défaut avec le correctif
transitoire `vendor/winit` / `vendor/smithay-clipboard` : même récepteur de
dépôt et de presse-papiers, partagé entre les viewports. Un second récepteur
perd les offres sur Mutter. `STOKESFORGE_X11=1` force le secours X11. Les tests
du parseur/canal et le démarrage natif ne prouvent pas le geste réel de dépôt :
consulter le statut et le protocole de `vendor/winit/STOKESFORGE.md` avant
d'affirmer sa validation sur un compositeur.

`stokesforge-app` est explicitement un **banc d'intégration**, pas l'interface
finale : la grille éditable à délégués, le panneau de propriétés et un système
de graphes complet seront repris entièrement plus tard. Plusieurs manques sont
donc **voulus à ce stade**, pas des oublis. Les principaux, écrits dans
`docs/application.md` :

- l'éditeur de matrice est un espace réservé ;
- pas de sélecteur de couleur graphique (les couleurs se tapent en `#RRGGBB`) ;
- pas de multi-panneaux dans la fenêtre des propriétés des graphes ;
- pas de largeur de barre ni de nombre de classes d'histogramme dans ce
  panneau ;
- les fenêtres ouvertes et leur mode ne sont pas restaurés au démarrage (seules
  position et taille le sont) ;
- une seule cellule éditable à la fois par fenêtre.

**Langue de l'interface** : l'application démarre en **anglais** ; le français
se choisit par « File » → « Language » → « Français » (tout de suite, sans
redémarrer, et retenu au lancement suivant). Quand on décrit un menu, donner le
nom **dans la langue de l'utilisateur** (celle de sa question), avec l'autre
entre parenthèses la première fois : « Add Text » (« Ajouter un texte »). Les
noms viennent des catalogues `crates/stokesforge-app/i18n/{en,fr}/stokesforge_app.ftl`.

**Les clics droits** (#152, `docs/menus.md`) : le menu dépend de ce qu'on vise
et propose des actions directes, sans sous-menu qui répète la cible — clic
droit sur une **courbe** (elle se sélectionne) → « Show Data » (ouvre le
classeur de *cette* courbe), « Rename », « Move Up/Down », « Hide », « Delete »,
« Properties… » ; sur le **fond** ou la zone de tracé → « Add Text »,
« Insert Object » (Rectangle, Ellipse, Line, Arrow, posé là où on a cliqué),
« Export », « Properties… ». « Reset Zoom » n'est plus au clic droit : barre
d'outils, menu « Graph », bouton du graphe zoomé ou double-clic. Le même menu
d'objet sert dans l'Object Manager et sous « Graph » → « Selection ».
Dans le Project Manager, un **dossier** se renomme, se colle et se supprime au
clic droit ; une **fenêtre** propose Show/Hide, Copy/Paste, Rename/Duplicate,
Delete. Dans un classeur, Undo/Redo ne sont plus au clic droit (menu « Edit »,
Ctrl+Z / Ctrl+Y).

**Écrire un texte sur un graphe** (outil Texte d'Origin) : clic droit dans
la zone de tracé → « Add Text » (ou menu « Graph »). On tape le
texte sur place, formules comprises ; la petite barre au-dessus règle couleur,
taille et rotation ; Entrée valide, Échap annule. Double-clic pour le
modifier, glisser pour le déplacer, Suppr pour le retirer. Le texte est
**attaché aux données** : il suit le zoom et disparaît si son point sort du
cadrage. Pas encore de cadre ni de fond autour du texte.

**Exporter un graphe** : menu « Graph » → « Export Graph… »
(Ctrl+E), ou clic droit sur la figure. SVG, PDF ou PNG selon l'extension du
fichier ; l'export reprend le **zoom affiché** et les formules. La résolution
PNG se choisit dans « Graph » → « PNG Resolution » (×4 par
défaut). `docs/application.md` § Propriétés des graphiques.

Quatre comportements ressemblent à des manques et n'en sont pas :

- **Écrire une lettre grecque, un indice ou un exposant** dans un titre, un
  libellé d'axe ou un nom de série : la syntaxe est celle de Typst, entre
  dollars — `$lambda$`, `$x_2$`, `$x^2$`, `$"cm"^(-1)$` ; `\$` pour un vrai
  dollar, `\_` pour un vrai tiret bas. La fenêtre des propriétés a un mémo
  replié « Formula syntax » sous ces champs. Un nom de colonne repris en
  légende s'affiche toujours tel quel (`docs/rendu.md` § Typst).

- **Ctrl+Z pendant une saisie** annule la dernière modification du projet, pas
  la dernière frappe — comportement de tableur, assumé.
- **Une série qui référence une colonne supprimée** ne disparaît pas : elle
  reste, en gris, annotée dans la légende. Une donnée manquante dégrade
  l'affichage, elle ne fait jamais disparaître le graphe.
- **Un plafond** (longueur de nom, nombre de cellules, nombre de classes…) est
  une limite arbitrée et documentée, pas une panne.

Quand une fonctionnalité manque, donner le **contournement** s'il existe : le
faire au clavier, par le menu, par un script Python, ou en deux gestes au lieu
d'un. C'est la partie de la réponse qui sert le plus.

## Forme de la réponse

1. **Oui / non / partiellement**, en une phrase.
2. **Le geste**, pas à pas si c'en vaut la peine, avec le raccourci.
3. **La limite ou la nuance**, s'il y en a une (et il y en a souvent :
   sélection multiple, colonne mixte, fenêtre détachée…).
4. **Le contournement** si la réponse est « non ».

Si la question révèle un manque qui mérite d'être comblé, le dire et proposer
d'enchaîner — mais ne pas commencer à coder dans une réponse à une question.
Pour implémenter : skill **stokesforge-feature**. Si la question révèle un
comportement anormal : skill **stokesforge-bug-report**.
