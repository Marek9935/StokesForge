# Principes de l'interface (vue d'ensemble)

Page d'entrée, courte, pour savoir **quoi respecter** avant de toucher à
l'interface de `stokesforge-app`. Elle ne recopie rien : chaque principe
renvoie à la page qui le détaille. En cas de divergence, le code fait foi,
puis [`standard-interface.md`](standard-interface.md) (555 lignes, décisions
de #217).

## 1. Une brique, un geste

Chaque élément d'interface a **une** brique ; on ne le refait pas à la main.

- Un nombre saisi : `number_field` et la table des grandeurs (`quantities`),
  jamais un `DragValue` ni un `Slider`.
- Un formulaire : `form` / `choice` (`app/form.rs`) ; des onglets :
  `tab_strip` ; une liste : `list_row` ; un état vide : `empty_state` ; une aide
  de champ : `hover_help` ; un travail long : `busy`.
- Une couleur : `theme.rs` seul (et le sélecteur `color_field`). Un écart :
  `theme::SPACE_ITEM` / `SPACE_SECTION`, jamais un nombre littéral.
- Une touche : le registre `app/shortcuts.rs`. Un curseur : `app/cursors.rs`.
  Un sélecteur de fichiers : `file_picker.rs`.

Ces règles ne reposent pas sur la mémoire de chacun : des gardes de
`crates/stokesforge-app/tests/gardes.rs` les font échouer. Liste et état :
standard, tableau « Briques et gardes ».

## 2. Les fenêtres ont un type

Toute fenêtre est modale (sauf celles de contenu : classeur, graphe, matrice,
note) et construite par `app/dialogs.rs`, jamais par une `egui::Window` brute.
Le type fixe le pied de page, l'ordre des boutons, Échap et le focus : réglages
en direct, création sur brouillon, choix, assistant, lecture seule,
confirmation. Taille et position sont mémorisées par dialogue.

- Ce qui existe, avec taille, pages et point d'ouverture :
  [`fenetres.md`](fenetres.md) (à tenir à jour à chaque fenêtre).
- Le comportement : standard, § Fenêtres et dialogues, § Confirmations.
- **Une fenêtre neuve se maquette et se fait valider avant le code.**

## 3. Ce qui est annulable s'annule, ce qui ne l'est pas se confirme

Toute modification du modèle passe par une commande annulable (Ctrl+Z).
On ne demande confirmation que pour ce que Ctrl+Z ne rattrape pas. Un objet
visible se règle en direct (OK garde, Cancel restaure) ; une action qui crée
travaille sur un brouillon et son bouton porte son verbe.
Détail : standard (F1, F3-F5) ; mécanique : [`commandes-et-historique.md`](commandes-et-historique.md).

## 4. Rien n'est grisé ou refusé en silence

Un élément grisé dit pourquoi et ce qu'il faut faire ; une saisie refusée ou
ramenée se voit (cadre rouge ou orange, bulle). Une erreur d'un dialogue
s'écrit dans son pied, pas dans la barre d'état. Les messages d'état et
d'erreur sont des textes du catalogue, jamais un `Display` brut.
Détail : standard, § Grisé, § Saisie refusée ou ramenée, § Messages et aide.

## 5. Les mots viennent du glossaire

Tout texte visible est une clé Fluent (anglais d'abord, puis français), écrit
avec le glossaire : une notion, un mot, partout ; majuscules et ton y sont
fixés. L'organisation des menus et des clics droits a sa grammaire.
Détail : [`textes-interface.md`](textes-interface.md), [`menus.md`](menus.md),
skill `stokesforge-textes-interface`.

## 6. Les nombres s'affichent d'une seule façon

Un seul formateur (`stokesforge_core::number_format`), la virgule décimale
acceptée en saisie, la valeur exacte en bulle, un résultat d'incertitude arrondi
à l'incertitude. Un R² faible se lit à deux signaux, pas à la couleur seule.
Détail : standard, § Affichage des nombres, § Virgule décimale, § R² faible.

## 7. Clavier et souris suivent un registre

Les raccourcis sont des constantes d'un seul module, et Help > Keyboard
Shortcuts est généré de ce registre : il ne peut pas diverger. Détail :
standard, § Clavier et souris.

## 8. L'état de l'interface vit hors de l'écran

`stokesforge-app-state` porte l'état, les textes et les erreurs mises en
phrase, sans egui ; `stokesforge-app` ne fait que peindre et traduire les
gestes en actions. Un dialogue ouvert est un drapeau de `AppState`, pas une
variable locale de la coquille. Détail : [`application.md`](application.md),
[`architecture.md`](architecture.md).

## Où aller ensuite

| Je veux… | Lire |
|---|---|
| créer ou modifier une fenêtre | [`fenetres.md`](fenetres.md), puis le standard § Fenêtres |
| poser un champ, un message, un grisage | [`standard-interface.md`](standard-interface.md) |
| ajouter une entrée de menu | [`menus.md`](menus.md) |
| écrire un texte visible | [`textes-interface.md`](textes-interface.md) |
| ajouter une icône | [`icones.md`](icones.md) |
| toucher à la fenêtre principale, à la grille | [`application.md`](application.md) |
| connaître les conventions par domaine | `.claude/conventions.md`, section « Application » |
