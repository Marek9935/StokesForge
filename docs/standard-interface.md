# Standard de comportement de l'interface (#217)

Source **unique** de la façon dont réagissent champs, messages, dialogues,
clavier et souris de `stokesforge-app` : les mêmes gestes, messages, champs et
fenêtres réagissent de la même façon partout. Décisions de l'utilisateur du
2026-10-10, consignées dans l'issue #217. L'organisation des menus est dans
[`menus.md`](menus.md), les mots et les majuscules dans
[`textes-interface.md`](textes-interface.md) ; cette page dit le
**comportement**.

Les codes entre parenthèses (N1, F7, R1…) sont ceux des formulaires de #217.
Leurs `R1`–`R16` (arbitrages du second formulaire) n'ont **rien à voir** avec
les règles R1–R16 de `textes-interface.md`.

## Comment lire cette page

- Chaque section porte un **état** : *en place* (le code la suit et une garde
  ou des tests la tiennent) ou *à venir* (issue de livraison entre
  parenthèses).
- Une règle *à venir* est déjà la règle : **tout code nouveau ou modifié la
  suit**. Le code existant qui déroge n'est pas un modèle, c'est une dette
  suivie par l'issue de la section.
- Les **briques** et **gardes** du tableau ci-dessous n'existent pas tant
  qu'elles sont *à venir* : ne pas les appeler, ne pas compter sur elles. Le
  commit qui les crée les passe à *en place*, ajoute la garde à la table de
  `stokesforge-verification` et met à jour les skills.

## Briques et gardes

| Élément | Rôle | État |
|---|---|---|
| état de champ refusé / ramené (`field_state`) | cadre rouge ou orange, bulle, cycle d'un champ texte validé | **en place** (#219) |
| brique « nombre » (`number_field`) + table des grandeurs (`quantities`) | seul champ numérique | **en place** (#220) |
| garde : aucun `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`) | | **en place** (#220) |
| formateur des nombres (`stokesforge_core::number_format`) | seul formatage affiché | **en place** (#222) |
| brique d'édition sur place (`app/inplace_edit.rs`) | un seul contrat T4 ; aperçus à pause de 0,3 s | **en place** (#224) |
| icône « ? » (`app/hover_help.rs` : `hover_help`, `help_text`, `help_beside`) | rappel d'un champ, posé à droite (M1, T6) | **en place** (#225) |
| garde : aucune bulle nue sur un champ de formulaire (`aucune_bulle_nue_sur_un_champ_de_formulaire`) | case, radio, champ texte/nombre, libellé de champ ; menus et `…_contents` exemptés | **en place** (#225) |
| garde : aucun `add_enabled` sans raison | | à venir (#226) |
| brique « formulaire » (grille à deux colonnes), largeurs et espacements de `theme.rs` | | à venir (#227) |
| garde : aucun `add_space` littéral hors de `theme.rs` | | à venir (#227) |
| zone de message du pied de `modal_dialog_with`, focus initial, taille mémorisée | | à venir (#228) |
| variante « confirmation » de `modal_dialog_with` | | à venir (#229) |
| registre des touches de `app/shortcuts.rs` | | à venir (#231) |
| garde : aucun `consume_key` hors de `app/shortcuts.rs` | | à venir (#231) |
| briques « travail en cours » et « état vide » | | à venir (#232) |

Déjà en place et réutilisées par ce standard : `modal_dialog_with`
(`app/dialogs.rs`), `hover_help` (`app/hover_help.rs`), `theme::section_title`,
`MenuSections::section`, `color_field`.

## Champs numériques — en place (#220, Auto #221, #233–#235), reste N11

Brique : `stokesforge_app::number_field::number_field(ui, sel, &mut valeur,
&quantities::GRANDEUR)` (`src/number_field.rs`), table : `src/quantities.rs`
(`Quantity` : bornes, pas, grand pas, décimales, entier ou non, valeurs
proposées). Une grandeur neuve s'ajoute à la table, jamais à l'appel ; les 28
`DragValue` et le `Slider` d'opacité sont migrés, la barre de format des objets
(épaisseur) aussi (N9). Reste à migrer : le corps de texte de la barre de format
(il a une entrée « Auto », #221).

- **Une brique « nombre » unique** (N1 → R1, R12). Aspect : champ texte, avec
  `▼` **seulement** si la table des grandeurs propose des valeurs (épaisseur,
  police…) ; une borne d'axe ou un paramètre d'ajustement n'en a pas.
  - Clic dans le texte : tout le texte est sélectionné, sans ouvrir la liste.
    La liste s'ouvre par `▼` ou Alt+↓.
  - Liste fermée : ↑/↓ = un pas, Maj+↑/↓ = un grand pas.
  - Liste ouverte : la molette fait défiler la liste, ↑/↓ surligne, Entrée
    choisit, Échap ferme.
  - Dans un dialogue modal, la liste respecte #9 (une `egui::Area` à soi).
- **Molette** (N2, X1 → R1 ; en place, #233, #235) : seul un champ **à liste**
  réagit, au **survol** (focus inutile) ; un cran = l'entrée suivante ou
  précédente de la liste (1, 1,5, 2, 3, 4…), une valeur tapée hors liste saute à
  l'entrée voisine, aux extrémités la valeur reste. Un champ **sans liste**
  (borne d'axe, paramètre d'ajustement) n'est jamais changé par la molette : la
  fenêtre défile, comme quand le curseur quitte le champ. Des crans d'affilée
  forment **une seule** entrée d'historique. Ctrl+molette reste le zoom.
  `number_field::step_preset`.
- **Aspect d'un champ à liste** (N1, N9 ; en place, #234) : un cadre unique,
  flèche pleine à l'intérieur à droite, comme la liste déroulante ; **fond
  blanc** si le champ est éditable au clavier, **fond gris** si seule la liste
  choisit. Fonds tirés du thème (`list_frame::editable_fill` / `list_only_fill`).
  Le champ nombre l'applique ; la liste éditable de la barre de format (corps
  de texte, encore une `ComboBox`) le suivra avec sa migration.
- **Application** (N3) : à Entrée ou en quittant le champ ; Échap rend
  l'ancienne valeur ; une entrée d'historique par validation.
- **Une grandeur, une définition** (N8) : une table unique donne bornes, pas,
  grand pas, décimales, unité et valeurs proposées (la molette suit ces
  valeurs, #233 ; le pas sert aux flèches du clavier). Épaisseur : 0 à 20 pt,
  décimales possibles au clavier.
- **Barres d'outils** (N9) : listes éditables (choisir 12 ou taper 15), même
  brique.
- **Unité** (N10) : dans le libellé (« Line width (pt) »), jamais en suffixe,
  y compris % et ° du sélecteur de couleur.
- **Aucun slider** (N12 → R16) : l'opacité est un champ, comme R, G et B.
- **Champs couplés** (N11) : min < max, « de » ≤ « à », erreur sur le
  **second** champ. Une contrainte entre deux champs est refusée ; seule une
  borne fixe est ramenée.

### Auto (C4, R10, G4) — en place (#221)

Mécanique : `quantities::AutoKind` (`None`, `Field`, `Group`) sur la grandeur,
`number_field::auto_number_field` (valeur `Option`, texte grisé « Auto (…) »)
pour un `Field`, `NumberResponse::auto` pour un `Group` (`AXIS_BOUND`). Les
bornes calculées viennent de `DrawnPanel` (`stokesforge-render`), publiées par
`graph_window::publish_axis_bounds`. Reste à migrer : le corps de texte de la
barre de format (liste « Auto », C4).


- Choix dans une liste : entrée « Auto » en tête.
- Nombre ou texte qui a un Auto : champ vidé = Auto, affiché « Auto (0–4000) »
  en gris (valeur calculée). Nombre **sans** Auto : champ vidé refusé.
- Groupe lié (Min/Max d'axe) : une case « Auto range » commune ; vider Min ou
  Max est refusé (cadre rouge, bulle « Use Auto range »). Sous « Auto range »,
  Min et Max sont **grisés** et montrent les bornes **calculées** ; décocher
  les reprend comme valeurs.
- Le mot `auto` tapé au clavier est accepté par un champ dont la grandeur a
  un Auto (Auto du groupe compris), et aucun autre texte ; ailleurs, c'est une
  saisie illisible.

## Saisie refusée ou ramenée — en place (#219)

Brique : `stokesforge_app::field_state` (`src/field_state.rs`, module public
comme `color_picker`), trois étages que les briques « nombre » (#220) et
« texte » (#224) réutilisent au lieu de repeindre un cadre :

- `show_issue(ui, &response, &issue)` : cadre et bulle d'un `FieldIssue`
  (`refused` rouge, `clamped` orange) sur un champ déjà posé — l'étage de la
  **seule** peinture d'un défaut de champ, aussi pour une erreur venue d'ailleurs
  (Typst, T3) ;
- `flag` / `active` / `clear` : l'état qui survit à la saisie (orange 2 s,
  `CLAMPED_SECONDS`) ;
- `edit_text(ui, sel, valeur_affichée, lecture)` : le cycle complet d'un champ
  validé, `lecture` rendant un `Verdict` (`Valid`, `Clamped`, `Refused`) ;
  `decide` en est la décision pure.

Un champ de formulaire neuf qui lit une saisie passe par `edit_text` ; une erreur
déjà connue passe par `show_issue`. Les teintes sont `theme::field_refused_color`
et `theme::field_clamped_color`. Reste à brancher (briques à venir) : les
champs numériques existants (`DragValue`, #220), la contrainte entre champs
(N11, #221), les renommages (#224), l'erreur Typst du titre (T3).

- **Illisible** (`abc`, `1..2`) : cadre **rouge**, bulle qui dit la règle,
  l'ancienne valeur revient à la sortie du champ (N6, N7 → R2, R8). Le rouge
  se peint dès la frappe ; à Entrée le champ reste ouvert, texte gardé ; à la
  sortie l'ancienne valeur revient et le refus reste signalé 2 s ; Échap rend
  la valeur sans cadre.
- **Hors bornes** : ramené à la borne, cadre **orange** (teinte
  d'avertissement) pendant 2 s, bulle « Set to the maximum, 20 ». Le rouge
  est réservé à « refusé ».
- **Texte refusé** (T2) : même règle, texte gardé ; un renommage refusé laisse
  le champ ouvert.
- **Erreur Typst d'un titre** (T3) : aussi sur le champ des propriétés, cadre
  rouge et message Typst en bulle.
- Jamais d'effacement ni de correction muette.

## Virgule décimale — en place (#223)

- « , » et « . » acceptés dans un champ qui contient **un seul nombre** et
  dans les cellules Numeric (N4, N5 → R6). À la validation, la virgule devient
  un point : l'interface affiche toujours le point. Effet assumé : en anglais,
  « 1,000 » est lu 1.
- Formules et listes de valeurs : point obligatoire (la virgule y sépare).
- Cellule Mixed : suit la règle Numeric (décidé) — « 1,5 » devient le nombre
  1.5. Une seule lecture, `stokesforge_core::number_format::parse_input`,
  partagée par la brique nombre, la grille et `Text → Numeric` ; la virgule
  est acceptée s'il y en a **une** et aucun point, sinon refus.
- **Séparateur des graphes** (N4 → R9) : réglage **par graphe** (Graph
  Properties › Axes › Display, `PlotSpec::decimal_comma`, annulable par
  `EditPlotSpec`), indépendant de la langue, pour axes,
  graduations et exports (la légende n'écrit aucun nombre aujourd'hui). Un
  graphe neuf prend le point ; le défaut des Preferences reste à livrer avec
  #148.

## Affichage des nombres — en place (#222)

- **Un seul formateur**, dans `stokesforge-core` (D1, D2 → R3) :
  - résultats et graduations : 3 chiffres significatifs, notation `e` hors de
    [1e−4, 1e6) ;
  - grille et champs : 6 chiffres significatifs ;
  - en édition : la valeur complète ; rien n'est écrit si le texte n'a pas
    changé ;
  - avec incertitude : incertitude à 2 chiffres significatifs, valeur à la
    même décimale (`12.346 ± 0.052`) ; sans incertitude : 3 chiffres ;
  - valeur exacte d'un résultat dans une bulle au survol ; **Ctrl+C copie
    toujours la valeur exacte**, avec un point.
- **Où** : `format_results` (3 chiffres : volet Results, bulle d'aperçu),
  `format_grid` (6 : grille — `AppState::cell_display` —, champs au repos),
  `format_exact` (édition, copier, comparaison de saisie : `cell_text`, champ
  numérique actif), `format_with_uncertainty`, `format_tick` (graduations,
  `stokesforge-render::scale`, aperçu d'analyse). Les chiffres avant la
  virgule ne sont jamais arrondis. Un nouveau nombre affiché passe par ce
  module, jamais par un `format!` local.
- **Notation scientifique** (D3) : exposant typographique sur la figure
  (1×10⁻⁵), notation `e` dans la grille et les champs. Les graduations restent en
  notation `e` (elles ne passent pas par Typst) : l'exposant typographique des
  graduations n'est pas livré.
- **Dates** (D4) : ISO 8601 (`2026-10-10 14:05`) ; formes de saisie (#218, en place) : ISO `2026-10-10 14:30` et `JJ/MM/AAAA [HH:MM[:SS]]` (`10/10/2026 14:30`), **jour d'abord**, jamais mois d'abord ; un nombre nu reste un timestamp POSIX ; le reste promeut la colonne en Mixed, comme tout texte non numérique. Lecteur et formateur uniques : `stokesforge_core::datetime` (grille, collage, conversion Mixed → Date/Time) ; `stokesforge-import` en garde une copie, accordée par un test de `stokesforge-importer`.
- **Chemins** (D5) : nom du fichier seul dans les messages, chemin complet en
  bulle.

## Champs texte et édition sur place — en place (#224)

- **Application** (T1 → R11) : à Entrée ou en quittant le champ. Exception :
  les aperçus (analyse, import) se recalculent à la frappe, texte et nombres,
  après une pause de 0,3 s. Rien n'est créé avant la validation du dialogue.
- **Éditeur sur place, un seul contrat** (T4) : tout le texte sélectionné à
  l'ouverture ; Entrée et clic ailleurs valident, Échap annule, Tab valide et
  passe au suivant.
- **Renommer dans le Project Manager** (K4) : F2, ou un clic lent sur un nom
  déjà sélectionné (0,5 s sans second clic ; un double-clic n'ouvre jamais le
  champ).

Comment c'est tenu :

- **`app/inplace_edit.rs`** (`begin` / `show` / `finish`, `resolve`) lit le
  focus et les touches d'un champ posé par l'appelant et en rend la fin
  (`EditEnd` : Entrée, Tab, ailleurs, Échap) ; il consomme Entrée, Tab et Échap
  (jamais relus plus bas). `resolve` est la décision pure : saisie refusée →
  le champ reste ouvert sous le cadre rouge de `field_state` (Entrée, Tab) ou
  l'ancienne valeur revient (clic ailleurs) ; texte inchangé → rien n'écrit.
- Appelants : cellules et en-têtes de la grille (`FocusLoss::Ends`, ouverture
  par frappe = `KeepCursor`), onglets de feuille (nom vide ou pris refusé ; Tab
  ouvre l'onglet voisin), Project Manager (nom vide ou pris refusé ; Entrée,
  Tab et clic ailleurs passent à la ligne suivante de la sélection), groupe de
  séries de l'Object Manager, titres et textes libres du graphe
  (`FocusLoss::Ignored` : leur barre de réglages prend le focus sans finir la
  saisie, le « ailleurs » reste décidé par l'éditeur).
- Aperçus : l'analyse (`analysis_preview::PREVIEW_PAUSE`) et l'import
  (`ImportState::debounce_preview`, pause seulement pendant la frappe dans un
  champ texte ; liste, case et bouton sont immédiats).

## Messages et aide — M1–M3, T5–T6 en place (#225) ; barre d'état à venir (#230)

- **Trois niveaux d'aide** (M1) : en clair (2 lignes au plus) pour un effet de
  bord, une portée, un geste sur un aperçu ; icône « ? » (`hover_help`) pour
  le rappel d'un champ ; bulle nue seulement pour boutons, icônes, onglets et
  menus. **Bulle nue sur un champ de formulaire : interdite** (M2).
- **Bulle d'un bouton** (M3) : à icône seule, nom, effet et raccourci ; à
  texte, effet et raccourci sans répéter le libellé.
- **Texte d'exemple d'un champ vide** (T5) : la valeur prise si vide (« Auto »,
  « Group 2 ») ou un exemple de syntaxe (« a*exp(-x/b) »), jamais une
  consigne. Champ de formule (T6) : l'icône « ? » seule.
- **Barre d'état** (#230) : information normale, avertissement en teinte
  d'avertissement, erreur en rouge avec icône (M4). Une information s'efface
  après ≈ 8 s ; avertissement et erreur restent jusqu'à l'action suivante
  (M5). Un clic montre les 50 derniers messages, horodatés (M6). Ni
  « Revision N » ni « Reopen Last Import… » (qui reste dans File) (M11 → R15).
- **Titre de la fenêtre** : nom du projet, « • » tant qu'il n'est pas
  enregistré : « essai.stf • — StokesForge ».
- **Un seul canal par information** (M9 → R13), deux exceptions écrites : une
  erreur de script affiche « Script failed. See the console » dans la barre
  d'état (M10 : l'exception est marquée comme erreur) ; la raison d'un bouton
  principal grisé est dans le pied du dialogue **et** dans sa bulle (F10).
- **Erreur dans une fenêtre modale** (M7) : jamais dans la barre d'état ; zone
  de message du pied, à gauche des boutons, commune à tous les dialogues.
- **Échec grave hors fenêtre** (M8 ; enregistrement raté, fichier illisible) :
  dialogue bloquant avec message et action possible.

## Fenêtres et dialogues — à venir (#228)

- **Application des réglages** (F1) : objet visible (graphe, série, objet,
  texte) → changement en direct, OK/Cancel ; Cancel restaure, OK = une entrée
  d'historique. Action qui crée (analyse, import) → brouillon, bouton nommé
  par son verbe (Create, Import).
- **Lecture seule** (F2) : un seul bouton Close.
- **Ordre des boutons** (F6) : dialogue = secondaires à gauche, puis Cancel,
  puis le principal à droite ; assistant = Cancel à gauche, Back · Next ·
  Finish groupés à droite.
- **Niveaux d'Échap** (F7 + R1) : une liste ouverte se ferme d'abord ; sinon
  le premier Échap rend au champ sa valeur, le suivant fait Cancel du
  dialogue.
- **Taille et position** (F8) mémorisées par sorte de dialogue, dans les
  préférences (hors `.stf`).
- **Focus** (F9) : le premier champ de saisie l'a à l'ouverture.
- **Bouton principal grisé** (F10 → R14) : raison dans le pied et en bulle ;
  une erreur affichée passe devant, la raison revient quand elle disparaît.

### Confirmations — à venir (#229)

- **Seulement pour ce que Ctrl+Z ne rattrape pas** (F3) : projet non
  enregistré (Nouveau, Ouvrir, Quitter, fermer la fenêtre), écraser un
  fichier, supprimer un modèle d'import. **Une suppression annulable ne
  demande rien.**
- **Forme** (F4) : dialogue modal, Cancel + verbe (Delete, Discard Changes).
  Quitter et Ouvrir : Discard Changes · Cancel · Save. Jamais « Keep » seul.
- **Clavier et aspect** (F5 → R4, R5) : Entrée et Échap = Cancel, bouton par
  défaut encadré sur Cancel ; Delete en style normal, rouge au survol **et**
  au focus clavier. Exception écrite : dans « Discard Changes · Cancel ·
  Save », **Entrée = Save**, Échap = Cancel.

## Grisé — à venir (#226)

- **Tout élément grisé porte sa raison**, qui dit quoi faire (« Select a
  series first ») (G1).
- **Forme** (G2) : bulle `.disabled` de l'élément lui-même, jamais mêlée à la
  bulle normale, jamais posée sur un groupe.
- **Undo/Redo** (G3) : grisés, « Nothing to undo » ; actifs, « Undo Rename »
  (libellé d'historique).
- **Champ qui dépend d'une case** (G4) : grisé, valeur visible et conservée ;
  masqué seulement si toute la section perd son sens (Min/Max : voir Auto).

## Choix, disposition — à venir (#227)

- **Radios** pour 2–3 options, liste déroulante dès 4 (C1) ; horizontaux si
  chaque libellé fait 3 mots au plus, verticaux sinon (C2).
- **Choix exclusif dans un menu** : coche seule, sans radio (C3, voir
  `menus.md`).
- **Bascule** = case à cocher, libellé à droite ; le toggle « Custom » du
  sélecteur de couleur devient une case (C5).
- **Formulaire en grille à deux colonnes** (L1) : libellés à gauche, champs
  alignés sur une même verticale, cases dans la colonne des champs.
- **Trois largeurs de champ** nommées dans `theme.rs` (L2) : courte (~80 px),
  moyenne (~160 px), pleine.
- **Un titre de section entre deux blocs**, jamais un trait seul (L3).
- **Deux constantes d'espacement** dans `theme.rs`, aucun `add_space`
  numérique ailleurs (L4).
- **Pas de section repliable** dans un formulaire ; trop long → onglets (L5).
- **Tab suit l'ordre de lecture** (K6) : haut-bas, gauche-droite.

## Clavier et souris — à venir (#231)

- **Registre unique des touches** dans `app/shortcuts.rs` (K1 : F2, Suppr,
  Échap, Entrée…) ; aucun `consume_key` ailleurs.
- **Suppr** supprime la sélection tout de suite, annulable, partout (K2). La
  grille garde son exception (vider d'abord), documentée.
- **Double-clic** (K3 → R7) : texte ou nom (cellule, onglet, titre) → édition
  sur place ; objet (série, axe) → Properties ; fond vide d'un graphe → vue
  réinitialisée ; Project Manager → active ou restaure la fenêtre, **ne ferme
  jamais**.
- **Help › Keyboard Shortcuts**, généré depuis `shortcuts.rs` (K5).
- **Ctrl+molette = zoom**, partout (X1 → R1, R7).
- **Curseurs** (X2) : main ouverte puis fermée pour glisser, flèche de
  redimensionnement sur une poignée, doigt sur un élément cliquable qui n'est
  pas un bouton.

## Travaux longs et états vides — à venir (#232)

- Au-delà d'≈ 0,5 s : indicateur tournant + mot (« Importing… ») à l'endroit
  du résultat ; barre de progression si le nombre d'étapes est connu (P1).
- Bouton **Stop** pour tout travail de plus de quelques secondes (import
  multiple, script, recalcul lourd) (P2).
- **État vide** : phrase grisée centrée qui dit ce qui manque et le geste pour
  le créer (P3).

## Hors de cette page

- **Fenêtre Edit › Preferences…** pour les réglages d'application, les menus
  gardent les bascules d'affichage fréquentes (X3) : #148.
- **Registre d'actions** (X4) : une table par action (libellé, logo,
  raccourci, condition de grisage et sa raison), lue par les menus, les
  barres d'outils et le clic droit ; il portera G1 et K1 (#110).

## Une règle qui paraît fausse

Un comportement qui ne suit pas cette page se signale contre elle
(`stokesforge-bug-report`). Une règle qui ne convient pas se rediscute avec
l'utilisateur, elle ne se contourne pas : la changer, c'est changer cette
page, l'issue concernée et les skills dans le même commit.
