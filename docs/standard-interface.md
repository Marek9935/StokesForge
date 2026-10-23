# Standard de comportement de l'interface (#217)

Source **unique** de la façon dont réagissent champs, messages, dialogues,
clavier et souris de `stokesforge-app` : les mêmes gestes, messages, champs et
fenêtres réagissent de la même façon partout. Décisions de l'utilisateur du
2026-10-10, consignées dans l'issue #217. L'organisation des menus est dans
[`menus.md`](menus.md), les mots et les majuscules dans
[`textes-interface.md`](textes-interface.md) ; cette page dit le
**comportement**.

Chaque règle porte un **code** `S-…` (S pour *standard* : `S-N1`, `S-F7`,
`S-R1`…) et un **nom lisible** : l'[index des règles](#index-des-règles) donne
les deux et dit la règle en une phrase, sans qu'il faille ouvrir une issue. Le
préfixe `S-` évite la confusion avec les règles `R1`–`R16` de
[`textes-interface.md`](textes-interface.md), qui sont d'une autre famille
(rédaction des textes). Les lettres disent le sujet : N nombres, D affichage
des données, T textes, M messages, F fenêtres, G grisé, C choix, L disposition,
K clavier et souris, X transversal, P travaux longs ; `S-R1` à `S-R16` sont
les arbitrages du second formulaire de décisions de #217.

## Comment lire cette page

- Une section ou une règle sans marquage est **en place** : le code la suit et
  une garde ou des tests la tiennent. Seul ce qui n'est pas encore livré porte
  *à venir* (issue de livraison entre parenthèses). Les numéros d'issues des
  livraisons passées ne sont pas dans les titres : ils sont regroupés dans la
  ligne « Historique : #… » qui ferme chaque section.
- Une règle *à venir* est déjà la règle : **tout code nouveau ou modifié la
  suit**. Le code existant qui déroge n'est pas un modèle, c'est une dette
  suivie par l'issue de la section.
- Les **briques** et **gardes** du tableau ci-dessous n'existent pas tant
  qu'elles sont *à venir* : ne pas les appeler, ne pas compter sur elles. Le
  commit qui les crée les passe à *en place*, ajoute la garde à la table de
  `stokesforge-verification` et met à jour les skills.

## Index des règles

Un code, un nom, une phrase. Les sections plus bas donnent le détail et la
mécanique.

| Code | Nom | La règle |
|---|---|---|
| S-N1 | `champ-nombre-unique` | Un seul champ numérique, la brique « nombre » ; `▼` seulement si la table des grandeurs propose des valeurs. |
| S-N2 | `molette-champ-a-liste` | La molette ne change qu'un champ à liste, au survol ; un champ sans liste n'est jamais changé par la molette. |
| S-N3 | `nombre-applique-a-la-validation` | Un nombre s'applique à Entrée ou en quittant le champ ; une entrée d'historique par validation. |
| S-N4 | `virgule-decimale-acceptee` | « , » et « . » sont acceptés dans un champ qui contient un seul nombre ; l'interface écrit toujours le point. |
| S-N5 | `virgule-decimale-cellules` | La même lecture vaut dans les cellules Numeric (et Mixed). |
| S-N6 | `saisie-illisible-refusee` | Une saisie illisible est refusée : cadre rouge, bulle qui dit la règle, l'ancienne valeur revient. |
| S-N7 | `hors-bornes-ramene` | Une valeur hors bornes est ramenée à la borne et signalée en orange pendant 2 s. |
| S-N8 | `grandeur-une-definition` | Une table unique des grandeurs donne bornes, pas, décimales, unité et valeurs proposées. |
| S-N9 | `barres-outils-listes-editables` | Les listes éditables des barres d'outils (corps, épaisseur : choisir 12 ou taper 15) passent par la même brique. |
| S-N10 | `unite-dans-le-libelle` | L'unité est dans le libellé (« Line width (pt) »), jamais en suffixe du champ. |
| S-N11 | `champs-couples` | « de » ≤ « à » : la contrainte entre deux champs est refusée, sur le second ; exception : Min/Max d'un axe. |
| S-N12 | `aucun-slider` | Aucun slider : l'opacité est un champ, comme R, G et B. |
| S-D1 | `chiffres-resultats-et-graduations` | Résultats et graduations : 3 chiffres significatifs, notation `e` hors de [1e−4, 1e6). |
| S-D2 | `chiffres-grille-et-champs` | Grille et champs : 6 chiffres significatifs ; en édition, la valeur complète. |
| S-D3 | `notation-scientifique` | Exposant typographique sur la figure (1×10⁻⁵), notation `e` dans la grille et les champs. |
| S-D4 | `dates-iso` | Dates affichées en ISO 8601 ; saisie ISO ou jour d'abord, jamais mois d'abord. |
| S-D5 | `chemins-nom-seul` | Nom du fichier seul dans les messages, chemin complet en bulle. |
| S-T1 | `apercu-apres-pause` | Un champ texte s'applique à Entrée ou en sortie ; les aperçus (analyse, import) se recalculent à la frappe, après 0,3 s de pause. |
| S-T2 | `texte-refuse-garde` | Un texte refusé reste dans le champ sous le cadre rouge ; un renommage refusé laisse le champ ouvert. |
| S-T3 | `erreur-typst-dans-le-champ` | L'erreur Typst d'un texte riche se peint, champ quitté, en cadre rouge et bulle ; le texte est appliqué tel quel. |
| S-T4 | `edition-sur-place-un-contrat` | Un seul contrat d'édition sur place : tout sélectionné, Entrée et clic ailleurs valident, Échap annule, Tab valide et passe au suivant. |
| S-T5 | `exemple-de-champ-vide` | Le texte d'exemple d'un champ vide est la valeur prise si vide ou un exemple de syntaxe, jamais une consigne. |
| S-T6 | `champ-formule-icone-aide` | Un champ de formule n'a que l'icône « ? » pour aide. |
| S-M1 | `aide-trois-niveaux` | Trois niveaux d'aide : texte en clair (2 lignes), icône « ? » pour le rappel d'un champ, bulle nue seulement pour boutons, icônes, onglets, menus. |
| S-M2 | `pas-de-bulle-nue-sur-un-champ` | Une bulle nue sur un champ de formulaire est interdite. |
| S-M3 | `bulle-de-bouton` | Bouton à icône seule : nom, effet, raccourci ; bouton à texte : effet et raccourci sans répéter le libellé. |
| S-M4 | `barre-etat-trois-niveaux` | La barre d'état distingue information, avertissement (teinte d'avertissement) et erreur (rouge, icône). |
| S-M5 | `information-qui-s-efface` | Une information s'efface après ≈ 8 s ; avertissement et erreur restent jusqu'à l'action suivante. |
| S-M6 | `historique-des-messages` | Un clic sur la barre d'état montre les 50 derniers messages, horodatés. |
| S-M7 | `erreur-dans-le-pied` | Une erreur survenue dans une fenêtre modale va dans le pied de cette fenêtre, jamais dans la barre d'état. |
| S-M8 | `echec-grave-en-dialogue` | Un échec grave hors fenêtre (enregistrement raté, projet illisible) est un dialogue bloquant avec une action possible. |
| S-M9 | `un-canal-par-information` | Une information n'a qu'un canal, hors les deux exceptions de S-R13. |
| S-M10 | `erreur-de-script-renvoi` | Une erreur de script dit « Script failed. See the console » dans la barre d'état (marquée erreur) ; le détail est dans la console. |
| S-M11 | `barre-etat-sans-revision` | Pas de « Revision N » ni de « Reopen Last Import… » dans la barre d'état. |
| S-F1 | `reglages-en-direct-ou-brouillon` | Objet visible : changement en direct, OK/Cancel ; action qui crée : brouillon et bouton nommé par son verbe. |
| S-F2 | `lecture-seule-close` | Une fenêtre en lecture seule n'a qu'un bouton Close. |
| S-F3 | `confirmer-l-irreversible` | Confirmation seulement pour ce que Ctrl+Z ne rattrape pas ; une suppression annulable ne demande rien. |
| S-F4 | `confirmation-forme` | Confirmation = dialogue modal, Cancel + verbe ; Quitter et Ouvrir : Discard Changes · Cancel · Save ; jamais « Keep » seul. |
| S-F5 | `confirmation-clavier` | Entrée et Échap = Cancel, bouton par défaut encadré sur Cancel ; exception : Entrée = Save dans le trio. |
| S-F6 | `ordre-des-boutons` | Dialogue : secondaires, Cancel, principal à droite ; assistant : Cancel à gauche, Back · Next · Finish à droite. |
| S-F7 | `niveaux-d-echap` | Échap ferme d'abord une liste ouverte, puis rend au champ sa valeur, puis fait Cancel du dialogue. |
| S-F8 | `geometrie-memorisee` | Taille et position d'un dialogue sont mémorisées par sorte de dialogue, dans les préférences. |
| S-F9 | `focus-initial` | Le premier champ de saisie a le focus à l'ouverture. |
| S-F10 | `bouton-principal-grise` | Un bouton principal grisé dit sa raison dans le pied et en bulle. |
| S-F11 | `nature-de-fenetre` | Une fenêtre modale a une nature (`DialogNature`) qui fixe son pied, sa taille par défaut (Petite 480 x 170, Moyenne 480 x 420, Grande 640 x 500), son focus initial et son bouton par défaut ; taille libre seulement justifiée pour une fenêtre interactive (réglages, réglages en direct à Close seul comme Préférences, brouillon, assistant). |
| S-G1 | `grise-avec-sa-raison` | Tout élément grisé porte sa raison, qui dit quoi faire (« Select a series first »). |
| S-G2 | `raison-en-bulle-propre` | La raison est la bulle `.disabled` de l'élément lui-même, jamais mêlée à la bulle normale ni posée sur un groupe. |
| S-G3 | `undo-redo-grises` | Undo et Redo grisés disent « Nothing to undo » ; actifs, ils nomment l'action (« Undo Rename »). |
| S-G4 | `champ-dependant-d-une-case` | Un champ qui dépend d'une case est grisé, valeur visible et conservée ; masqué seulement si toute la section perd son sens. |
| S-C1 | `radios-ou-liste` | Radios pour 2 à 3 options, liste déroulante dès 4. |
| S-C2 | `radios-cote-a-cote-ou-empiles` | Radios horizontaux si chaque libellé fait 3 mots au plus, verticaux sinon. |
| S-C3 | `choix-de-menu-coche` | Un choix exclusif dans un menu est une coche seule, sans radio. |
| S-C4 | `champ-vide-egale-auto` | Un nombre ou un texte qui a un Auto : champ vidé = Auto (le corps de texte de la barre de format en a un). |
| S-C5 | `bascule-case-a-cocher` | Une bascule est une case à cocher, libellé à droite. |
| S-L1 | `formulaire-en-grille` | Un formulaire est une grille à deux colonnes : libellés à gauche, champs alignés. |
| S-L2 | `quatre-largeurs-de-champ` | Quatre largeurs de champ nommées dans `theme.rs` : quelques chiffres, courte, moyenne, pleine. |
| S-L3 | `titre-de-section-entre-blocs` | Un titre de section entre deux blocs, jamais un trait seul. |
| S-L4 | `deux-espacements` | Deux constantes d'espacement dans `theme.rs` ; aucun `add_space` numérique ailleurs. |
| S-L5 | `pas-de-section-repliable` | Pas de section repliable dans un formulaire ; trop long, on passe en onglets. |
| S-L6 | `trois-niveaux-de-texte` | Trois niveaux de texte nommés dans `theme.rs` : titre de section, en-tête de tableau, texte secondaire ; jamais `bold(…).small()` écrit sur place. |
| S-L7 | `hauteur-de-ligne-unique` | Une ligne de liste ou d'arbre a une seule hauteur, `LIST_ROW_HEIGHT`, composée ou non. |
| S-K1 | `registre-des-touches` | Toutes les touches sont dans le registre de `app/shortcuts.rs` ; aucun `consume_key` ailleurs. |
| S-K2 | `suppr-immediat-annulable` | Suppr supprime la sélection tout de suite, annulable, partout (la grille vide d'abord). |
| S-K3 | `double-clic-par-sorte` | Double-clic : texte ou nom → édition sur place ; objet → Properties ; fond vide d'un graphe → vue réinitialisée ; Project Manager → active la fenêtre, ne la ferme jamais, et replie un dossier. |
| S-K4 | `renommer-f2-ou-clic-lent` | Renommer dans le Project Manager : F2, ou un clic lent sur un nom déjà sélectionné. |
| S-K5 | `aide-raccourcis-generee` | Help › Keyboard Shortcuts est générée depuis le registre des touches. |
| S-K6 | `tab-suit-la-lecture` | Tab suit l'ordre de lecture : haut-bas, gauche-droite. |
| S-K7 | `outils-dans-la-barre-tools` | Pas de sélecteur d'outil dans une fenêtre : les outils restent dans la barre Tools, l'objet d'abord ; sortie admise (outils d'analyse), toujours justifiée dans le code et la doc. |
| S-K8 | `gestes-dans-la-matrice` | Tout geste de molette ou de double-clic est rangé dans la matrice `gestes.md`, où l'on voit si le même geste fait ailleurs autre chose. |
| S-X1 | `ctrl-molette-zoom` | Ctrl+molette zoome, partout. |
| S-X2 | `curseurs` | Main ouverte puis fermée pour glisser, flèche de redimensionnement sur une poignée, doigt sur un élément cliquable qui n'est pas un bouton. |
| S-X3 | `fenetre-preferences` | **À venir (#148).** Les réglages d'application vont dans Edit › Preferences… ; les menus gardent les bascules d'affichage fréquentes. |
| S-X4 | `registre-d-actions` | **À venir (#110).** Une table par action (libellé, logo, raccourci, grisage et sa raison) lue par menus, barres et clic droit. |
| S-X5 | `trait-de-depot-unique` | Un seul trait « déposer ici » (`theme::drop_stroke`, vert de l'accent) : fichier externe sur la fenêtre ou le classeur, colonnes et lignes sur un graphe. |
| S-P1 | `indicateur-apres-0-5-s` | Au-delà d'≈ 0,5 s : indicateur tournant + mot à l'endroit du résultat ; barre de progression si les étapes sont connues. |
| S-P2 | `stop-au-dela-de-3-s` | Bouton Stop pour tout travail de plus de quelques secondes. |
| S-P3 | `etat-vide-explicite` | Un état vide est une phrase grisée centrée qui dit ce qui manque et le geste pour le créer. |

Les arbitrages du second formulaire de #217 ; chacun tranche un conflit entre
deux règles ci-dessus :

| Code | Nom | La règle |
|---|---|---|
| S-R1 | `molette-au-survol` | La molette change un champ à liste au survol, sans focus ; Ctrl+molette reste le zoom. |
| S-R2 | `illisible-refuse-hors-bornes-ramene` | Illisible → refusé ; hors bornes → ramené et signalé. |
| S-R3 | `chiffres-affiches` | 3 chiffres significatifs pour résultats et graduations, 6 pour grille et champs. |
| S-R4 | `confirmation-entree-annule` | Dans une confirmation, Entrée et Échap font Cancel. |
| S-R5 | `destructeur-rouge-au-survol` | Le verbe destructeur est normal au repos, rouge au survol et au focus clavier. |
| S-R6 | `virgule-formules-et-listes` | La virgule décimale vaut dans un champ à un seul nombre ; formules et listes de valeurs exigent le point ; Mixed suit Numeric. |
| S-R7 | `double-clic-par-sorte-et-zoom` | Une règle de double-clic par sorte de chose ; Ctrl+molette zoome partout. |
| S-R8 | `valeur-ramenee-orange` | Une valeur ramenée à la borne se signale en orange ; le rouge est réservé au refus. |
| S-R9 | `separateur-par-graphe` | Le séparateur décimal est un réglage par graphe, indépendant de la langue. |
| S-R10 | `champ-vide-selon-auto` | Un champ vidé veut dire Auto s'il a un Auto, sinon il est refusé. |
| S-R11 | `apercu-texte-et-nombres-apres-pause` | Les aperçus se recalculent à la frappe après une pause, texte comme nombres ; le reste s'applique à Entrée. |
| S-R12 | `fleche-seulement-avec-valeurs` | `▼` seulement quand la table des grandeurs propose des valeurs ; clic dans le texte = tout sélectionné. |
| S-R13 | `canal-unique-deux-exceptions` | Un seul canal par information, avec deux exceptions écrites : erreur de script (barre d'état + console) et raison du bouton grisé (pied + bulle). |
| S-R14 | `pied-erreur-avant-raison` | Dans le pied d'un dialogue, l'erreur passe devant la raison du bouton grisé ; la raison revient quand l'erreur disparaît. |
| S-R15 | `barre-etat-sans-contenu-permanent` | Aucun contenu permanent dans la barre d'état (ni révision ni lien d'import). |
| S-R16 | `opacite-sans-slider` | L'opacité est un champ, pas un slider. |

## Briques et gardes

| Élément | Rôle | État |
|---|---|---|
| état de champ refusé / ramené (`field_state`) | cadre rouge ou orange, bulle, cycle d'un champ texte validé | **en place** (#219) |
| brique « nombre » (`number_field`) + table des grandeurs (`quantities`) | seul champ numérique | **en place** (#220) |
| garde : aucun `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`) | | **en place** (#220) |
| formateur des nombres (`stokesforge_core::number_format`) | seul formatage affiché | **en place** (#222) |
| brique d'édition sur place (`app/inplace_edit.rs`) | un seul contrat S-T4 ; aperçus à pause de 0,3 s | **en place** (#224) |
| icône « ? » (`app/hover_help.rs` : `hover_help`, `help_text`, `help_beside`) | rappel d'un champ, posé à droite (S-M1, S-T6) | **en place** (#225) |
| garde : aucune bulle nue sur un champ de formulaire (`aucune_bulle_nue_sur_un_champ_de_formulaire`) | case, radio, champ texte/nombre, libellé de champ ; menus et `…_contents` exemptés | **en place** (#225) |
| garde : aucun `add_enabled` sans raison (`aucun_element_grise_sans_sa_raison`) | chaîne de l'appel lue jusqu'à `.on_disabled_hover_text` ; `add_enabled_ui` (groupe) exempté | **en place** (#226) |
| brique « formulaire » (`app/form.rs` : `form`, `Form`, `choice`), largeurs et espacements de `theme.rs` | grille à deux colonnes, choix radio/liste (S-C1, S-C2) | **en place** (#227) |
| choix exclusif dans un menu (`menu_sections::menu_choice`) | coche seule (S-C3) | **en place** (#227) |
| brique « textes entourés » (`app/form.rs` : `toggle_texts`) | choix exclusif à bascule, cadre arrondi autour de chaque libellé (actif : fond plein de l'accent ; inactif : contour seul) ; réservée à la rangée de gestes de l'aperçu d'analyse (exception S-K7) | **en place** (#270) |
| garde : aucun `add_space` littéral hors de `theme.rs` (`aucun_add_space_litteral_hors_du_theme`) | | **en place** (#227) |
| zone de message du pied de `modal_dialog_with`, focus initial, taille mémorisée, `modal_wizard`, `modal_live_settings`, `modal_read_only` | ordre des boutons, erreur et raison dans le pied, Échap, S-F8, S-F9 | **en place** (#228) |
| nature de fenêtre (`app/dialogs.rs` : `DialogNature`, `SizeClass`, `InteractiveSize`, `PlainSize`, `DialogFooter::always` / `gated`) | pied, classe de taille, focus initial et bouton par défaut imposés par des `match` exhaustifs (S-F11) | **en place** (#300) |
| garde : aucune taille libre sans justification (`aucune_taille_libre_sans_justification`, témoin `la_garde_des_tailles_voit_une_taille_libre_nue`) | `InteractiveSize::Free` refusé sans le marqueur `// TAILLE-LIBRE(#300): <justification>` (S-F11) | **en place** (#300) |
| garde : le catalogue des fenêtres et le code concordent (`le_catalogue_des_fenetres_et_le_code_concordent`, témoin `la_garde_du_catalogue_voit_un_ecart`) | tout identifiant `stokesforge_…` passé à un `modal_*` est dans [`fenetres.md`](fenetres.md), et chaque ligne du catalogue a son appel ; ne lit pas les tailles (S-F11) | **en place** (#300) |
| confirmations et échec grave (`modal_confirm`, `modal_save_changes`, `modal_alert` ; `app/confirmations.rs`) | Cancel par défaut, trio Discard Changes · Cancel · Save, S-M8 | **en place** (#229) |
| registre des touches de `app/shortcuts.rs`, `HELP`, Help › Keyboard Shortcuts | S-K1, S-K5 | **en place** (#231) |
| garde : aucun `consume_key` hors de `app/shortcuts.rs` (`aucun_consume_key_hors_du_registre`) | | **en place** (#231) |
| curseurs (`app/cursors.rs`) et garde `aucun_curseur_hors_du_module_des_curseurs` | S-X2 | **en place** (#231) |
| garde : aucun sélecteur d'outil hors de la barre Tools (`aucun_selecteur_d_outil_hors_de_la_barre_tools`, témoin `la_garde_des_outils_voit_un_selecteur_d_outil_nu`) | choix exclusif dont les arguments nomment un outil, refusé hors de `toolbar.rs` sans le marqueur `// OUTIL-EN-FENETRE(#299): <justification>` (S-K7) | **en place** (#299) |
| garde : aucun geste hors de la matrice (`aucun_geste_hors_de_la_matrice`, témoin `la_garde_des_gestes_voit_un_geste_hors_de_la_matrice`) | tout fichier qui lit une molette ou un double-clic est cité par [`gestes.md`](gestes.md) (S-K8) | **en place** (#299) |
| brique « travail en cours » (`app/busy.rs` : `work_in_progress`, `indicator`) | S-P1, S-P2 | **en place** (#232) ; branchée sur l'import en fond et la console de script |
| brique « état vide » (`app/empty_state.rs` : `empty_state`) | S-P3 | **en place** (#232) |

Déjà en place et réutilisées par ce standard : `modal_dialog_with`
(`app/dialogs.rs`), `hover_help` (`app/hover_help.rs`), `theme::section_title`,
`MenuSections::section`, `color_field`.

## Champs numériques

Brique : `stokesforge_app::number_field::number_field(ui, sel, &mut valeur,
&quantities::GRANDEUR)` (`src/number_field.rs`), table : `src/quantities.rs`
(`Quantity` : bornes, pas, grand pas, décimales, entier ou non, valeurs
proposées). Une grandeur neuve s'ajoute à la table, jamais à l'appel ; les 28
`DragValue` et le `Slider` d'opacité sont migrés, la barre de format des objets
(épaisseur) et le corps de texte (`quantities::TEXT_FORMAT_SIZE`, entrée
« Auto ») aussi (S-N9).

- **Une brique « nombre » unique** (S-N1 → S-R1, S-R12). Aspect : champ texte, avec
  `▼` **seulement** si la table des grandeurs propose des valeurs (épaisseur,
  police…) ; une borne d'axe ou un paramètre d'ajustement n'en a pas.
  - Clic dans le texte : tout le texte est sélectionné, sans ouvrir la liste.
    La liste s'ouvre par `▼` ou Alt+↓.
  - Liste fermée : ↑/↓ = un pas, Maj+↑/↓ = un grand pas.
  - Liste ouverte : la molette fait défiler la liste, ↑/↓ surligne, Entrée
    choisit, Échap ferme.
  - Dans un dialogue modal, la liste respecte #9 (une `egui::Area` à soi).
- **Molette** (S-N2, S-X1 → S-R1 ; en place, #233, #235) : seul un champ **à liste**
  réagit, au **survol** (focus inutile) ; un cran = l'entrée suivante ou
  précédente de la liste (1, 1,5, 2, 3, 4…), une valeur tapée hors liste saute à
  l'entrée voisine, aux extrémités la valeur reste. Un champ **sans liste**
  (borne d'axe, paramètre d'ajustement) n'est jamais changé par la molette : la
  fenêtre défile, comme quand le curseur quitte le champ. Des crans d'affilée
  forment **une seule** entrée d'historique. Ctrl+molette reste le zoom.
  `number_field::step_preset`.
- **Aspect d'un champ à liste** (S-N1, S-N9 ; en place, #234) : un cadre unique,
  flèche pleine à l'intérieur à droite, comme la liste déroulante ; **fond
  blanc** si le champ est éditable au clavier, **fond gris** si seule la liste
  choisit. Fonds tirés du thème (`list_frame::editable_fill` / `list_only_fill`).
  Le champ nombre l'applique, y compris la liste éditable du corps de texte de
  la barre de format.
- **Application** (S-N3) : à Entrée ou en quittant le champ ; Échap rend
  l'ancienne valeur ; une entrée d'historique par validation.
- **Une grandeur, une définition** (S-N8) : une table unique donne bornes, pas,
  grand pas, décimales, unité et valeurs proposées (la molette suit ces
  valeurs, #233 ; le pas sert aux flèches du clavier). Épaisseur : 0 à 20 pt,
  décimales possibles au clavier.
  Les plafonds d'un réglage d'analyse (degré 9, itérations 5000, pics **100**,
  composantes 4) ne sont pas écrits dans la table : elle les lit dans
  `stokesforge_analysis::ops` (#260), la source que `Operation::validate`
  applique aussi à `pyst` et au fichier.
- **Barres d'outils** (S-N9) : listes éditables (choisir 12 ou taper 15), même
  brique.
- **Unité** (S-N10) : dans le libellé (« Line width (pt) »), jamais en suffixe,
  y compris % et ° du sélecteur de couleur.
- **Aucun slider** (S-N12 → S-R16) : l'opacité est un champ, comme R, G et B.
- **Champs couplés** (S-N11, en place, #240) : « de » ≤ « à » (plages de
  l'analyse, `analysis_forms::flag_pair_refusal`), erreur sur le **second** champ.
  **Exception : Min/Max d'un axe** — un axe peut être inversé (cm⁻¹), aucune
  contrainte, aucune erreur. Une contrainte entre deux champs est refusée ; seule une
  borne fixe est ramenée.

Historique : #220 (brique), #233–#235 (molette, aspect d'un champ à liste), #240 (S-N9, S-N11).

### Auto (S-C4, S-R10, S-G4)

Mécanique : `quantities::AutoKind` (`None`, `Field`, `Group`) sur la grandeur,
`number_field::auto_number_field` (valeur `Option`, texte grisé « Auto (…) »)
pour un `Field`, `NumberResponse::auto` pour un `Group` (`AXIS_BOUND`). Les
bornes calculées viennent de `DrawnPanel` (`stokesforge-render`), publiées par
`graph_window::publish_axis_bounds`. Le corps de texte de la barre de format
a son entrée « Auto » (liste éditable, S-C4).


- Choix dans une liste : entrée « Auto » en tête.
- Nombre ou texte qui a un Auto : champ vidé = Auto, affiché « Auto (0–4000) »
  en gris (valeur calculée). Nombre **sans** Auto : champ vidé refusé.
- Groupe lié (Min/Max d'axe) : une case « Auto range » commune ; vider Min ou
  Max est refusé (cadre rouge, bulle « Use Auto range »). Sous « Auto range »,
  Min et Max sont **grisés** et montrent les bornes **calculées** ; décocher
  les reprend comme valeurs.
- Le mot `auto` tapé au clavier est accepté par un champ dont la grandeur a
  un Auto (Auto du groupe compris), et aucun autre texte ; ailleurs, c'est une
  saisie illisible. Les nombres facultatifs de l'analyse (paramètres
  d'ajustement, `a`/`b`), qui ne passent pas par `number_field`, lisent eux
  aussi `auto` et `a` comme un champ vidé (`parse_optional`, #247).

Historique : #221.

## Saisie refusée ou ramenée

Brique : `stokesforge_app::field_state` (`src/field_state.rs`, module public
comme `color_picker`), trois étages que les briques « nombre » (#220) et
« texte » (#224) réutilisent au lieu de repeindre un cadre :

- `show_issue(ui, &response, &issue)` : cadre et bulle d'un `FieldIssue`
  (`refused` rouge, `clamped` orange) sur un champ déjà posé — l'étage de la
  **seule** peinture d'un défaut de champ, aussi pour une erreur venue d'ailleurs
  (Typst, S-T3) ;
- `flag` / `active` / `clear` : l'état qui survit à la saisie (orange 2 s,
  `CLAMPED_SECONDS`) ;
- `edit_text(ui, sel, valeur_affichée, lecture)` : le cycle complet d'un champ
  validé, `lecture` rendant un `Verdict` (`Valid`, `Clamped`, `Refused`) ;
  `decide` en est la décision pure.

Un champ de formulaire neuf qui lit une saisie passe par `edit_text` ; une erreur
déjà connue passe par `show_issue`. Les teintes sont `theme::field_refused_color`
et `theme::field_clamped_color`. Branchés : la brique « nombre » (#220) et les
renommages (#224, `app/inplace_edit.rs`). Branchés aussi (#240) : la
contrainte entre champs (S-N11) et l'erreur Typst des textes riches (S-T3).

- **Illisible** (`abc`, `1..2`) : cadre **rouge**, bulle qui dit la règle,
  l'ancienne valeur revient à la sortie du champ (S-N6, S-N7 → S-R2, S-R8). Le rouge
  se peint dès la frappe ; à Entrée le champ reste ouvert, texte gardé ; à la
  sortie l'ancienne valeur revient et le refus reste signalé 2 s ; Échap rend
  la valeur sans cadre.
- **Hors bornes** : ramené à la borne, cadre **orange** (teinte
  d'avertissement) pendant 2 s, bulle « Set to the maximum, 20 ». Le rouge
  est réservé à « refusé », à **une exception écrite** : R² faible (voir
  « R² faible » ci-dessous), qui a son jeton dédié (`theme::r_squared_color`)
  et ne sert jamais à un refus de champ.
- **Texte refusé** (S-T2) : même règle, texte gardé ; un renommage refusé laisse
  le champ ouvert.
- **Erreur Typst d'un texte riche** (S-T3, en place, #240) : titre, titres d'axe,
  légende — tout champ posé par `plot_settings::formula_field` — peint, **champ
  quitté** (pas pendant la frappe), le cadre rouge et le message Typst en bulle
  (`stokesforge_render::formula::diagnose`, mémorisé par texte). Le texte est
  appliqué tel quel : le graphe retombe sur le texte brut, **dans un cadre rouge à
  l'écran** (`EguiBackend::draw_text`) ; jamais dans un export ni une copie
  (backend SVG).
- Jamais d'effacement ni de correction muette.

Historique : #219 ; branchements : #220, #224, #240 (S-N11, S-T3).

## R² faible

- **Paliers** (`stokesforge_app_state::r_squared`, donnée neutre, bornes
  strictes, cumulatifs) : R² < 0,95 **orange** ; < 0,8 **rouge** ; < 0,6 rouge
  **et** boîte de dialogue. Un R² négatif relève du palier < 0,6 ; `NaN`
  n'avertit de rien. Les couleurs valent pour toute valeur `stat.r_squared`
  (ajustements et pics) : aperçu de la fenêtre d'analyse, volet Results,
  réglages d'un calcul — `exact_result_label` les peint toutes.
- **Pas seulement la couleur** : la valeur passe en **gras** et sa bulle ajoute
  « Low R² … » ; la teinte est un jeton clair/sombre (`theme::r_squared_color`).
  Ce rouge est le seul permis hors du refus de champ (`field_refused_color`) ;
  il reste un jeton distinct.
- **Boîte de dialogue** : avertissement, pas échec — `AlertKind::LowRSquared`,
  pied « Close » seul (`modal_read_only`), Entrée et Échap ferment. Elle s'ouvre
  **quand le R² d'un calcul enregistré change** : application, recalcul manuel
  ou automatique (`AppState::watch_low_r2`, empreinte par calcul — un
  instantané qui ne change pas le R² ne la rouvre pas), ou à l'application d'un
  calcul « Show » (dont les valeurs ne sont pas gardées, vues au seul apply).
  Jamais pour l'aperçu en direct ni à l'ouverture d'un projet. Une alerte
  d'enregistrement ou d'ouverture déjà là n'est pas écrasée ; plusieurs calculs
  au même instantané : **une seule boîte**, au texte groupé par type (#290) :
  « R² < 0,6 pour : • 12 reconvolutions • 1 ajustement », puis un renvoi vers le
  volet Résultats. L'unité de compte est la **série** (un ajustement de 5
  colonnes dont 2 en défaut compte 2) ; les types suivent l'ordre fixe de
  `OperationKind::ALL`, leurs noms viennent du glossaire (clés
  `low-r2-count-*`, pluriels fr+en). Pas de noms de séries ni de liste défilante.
- **Pastille de la barre d'état** (#290) : « ⚠ 13 R² faibles », à droite de la
  barre, en rouge R² (`theme::r_squared_color`), avec infobulle. C'est un
  **état** recalculé à chaque instantané (`AppState::low_r2_groups`,
  `low_r2_total`), pas un évènement : elle reste après la fermeture de la
  boîte, même quand la boîte s'est ouverte, elle n'existe que tant qu'il reste
  des R² < 0,6 (recalcul ou annulation qui les remonte, calcul supprimé : elle
  disparaît). Elle compte **tout le projet**, calculs dont le réglage
  `low_r2_dialog` est coupé compris (le réglage coupe la boîte, pas
  l'information), et les cas où une alerte attendait déjà. Les calculs « Show »
  ne gardent pas leurs valeurs : ils n'y comptent pas (la boîte les dit au seul
  apply). Un clic ouvre la **même liste groupée** en fenêtre en lecture seule
  (`Action::OpenLowR2List`/`CloseLowR2List`, état `low_r2_list_open` séparé de
  `alert` : une alerte d'enregistrement n'est jamais écrasée) ; la fenêtre se
  referme d'elle-même quand plus rien n'est faible.
- **Deux interrupteurs, en OU** : la préférence d'application (Préférences,
  page General, `Action::SetAnalysisLowR2Dialog`) et le réglage du calcul
  (`Calculation::low_r2_dialog`, section Output). Les couleurs, elles, restent.

Historique : #288, #290 (pastille et liste groupée).

## Virgule décimale

- « , » et « . » acceptés dans un champ qui contient **un seul nombre** et
  dans les cellules Numeric (S-N4, S-N5 → S-R6). À la validation, la virgule devient
  un point : l'interface affiche toujours le point. Effet assumé : en anglais,
  « 1,000 » est lu 1. **Avis discret** (#311) : quand une **saisie de cellule**
  (colonne Numeric ou Mixed) a exactement la forme `\d{1,3},\d{3}` (« 1,234 »),
  la barre d'état dit comment elle a été lue (« "1,234" was read as 1.234
  (decimal comma) »). L'avis suit l'écriture réussie ; il ne vaut ni pour le
  collage ni pour une colonne Date/Time, et ne change pas la règle
  (`number_format::looks_like_thousands_separator`).
- Formules et listes de valeurs : point obligatoire (la virgule y sépare).
- Cellule Mixed : suit la règle Numeric (décidé) — « 1,5 » devient le nombre
  1.5. Une seule lecture, `stokesforge_core::number_format::parse_input`,
  partagée par la brique nombre, la grille et `Text → Numeric` ; la virgule
  est acceptée s'il y en a **une** et aucun point, sinon refus.
- **Séparateur des graphes** (S-N4 → S-R9) : réglage **par graphe** (Graph
  Properties › Axes › Display, `PlotSpec::decimal_comma`, annulable par
  `EditPlotSpec`), indépendant de la langue, pour axes,
  graduations et exports (la légende n'écrit aucun nombre aujourd'hui). Un
  graphe neuf prend le point ; le défaut des Preferences reste à livrer avec
  #148.

Historique : #223.

## Affichage des nombres

- **Un seul formateur**, dans `stokesforge-core` (S-D1, S-D2 → S-R3) :
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
- **Notation scientifique** (S-D3) : exposant typographique sur la figure
  (1×10⁻⁵), notation `e` dans la grille et les champs. Les graduations restent en
  notation `e` (elles ne passent pas par Typst) : l'exposant typographique des
  graduations n'est pas livré.
- **Collage de lignes de données** : une case **absente** (ligne plus courte que la colonne visée) garde la valeur existante et n'allonge jamais la colonne ; une case **présente et vide** (`3<tab>`) est une valeur manquante. Si la copie compte plus de colonnes que la feuille n'en a à partir de l'ancre, les colonnes en trop sont abandonnées (aucune n'est ajoutée) et la barre d'état le dit (`status-paste-columns-dropped`). Pour un collage de **champs d'en-tête** ancré bas, les rangées qui passent au-delà de Formule sont jetées et comptées de la même façon (`status-paste-fields-dropped`) ; si des colonnes et des champs débordent ensemble, un seul message les donne (`status-paste-columns-fields-dropped`). Ctrl+Maj+V de colonnes entières réécrit tout l'en-tête à son rang d'origine (matrice voulue).
- **Dates** (S-D4) : ISO 8601 (`2026-10-10 14:05`) ; formes de saisie (#218, en place) : ISO `2026-10-10 14:30` et `JJ/MM/AAAA [HH:MM[:SS]]` (`10/10/2026 14:30`), **jour d'abord**, jamais mois d'abord ; un nombre nu reste un timestamp POSIX en **secondes** (« 2026 » devient 1970-01-01 00:33:46, pas l'année 2026 : décidé, #311) ; le reste promeut la colonne en Mixed, comme tout texte non numérique. Lecteur et formateur uniques : `stokesforge_core::datetime` (grille, collage, conversion Mixed → Date/Time) ; `stokesforge-import` en garde une copie, accordée par un test de `stokesforge-importer`.
- **Chemins** (S-D5) : nom du fichier seul dans les messages, chemin complet en
  bulle.

Historique : #222, #218 (dates).

## Champs texte et édition sur place

- **Application** (S-T1 → S-R11) : à Entrée ou en quittant le champ. Exception :
  les aperçus (analyse, import) se recalculent à la frappe, texte et nombres,
  après une pause de 0,3 s. Rien n'est créé avant la validation du dialogue.
- **Éditeur sur place, un seul contrat** (S-T4) : tout le texte sélectionné à
  l'ouverture ; Entrée et clic ailleurs valident, Échap annule, Tab valide et
  passe au suivant.
- **Renommer dans le Project Manager** (S-K4) : F2, ou un clic lent sur un nom
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

Historique : #224.

## Messages et aide

- **Trois niveaux d'aide** (S-M1) : en clair (2 lignes au plus) pour un effet de
  bord, une portée, un geste sur un aperçu ; icône « ? » (`hover_help`) pour
  le rappel d'un champ ; bulle nue seulement pour boutons, icônes, onglets et
  menus. **Bulle nue sur un champ de formulaire : interdite** (S-M2).
- **Bulle d'un bouton** (S-M3) : à icône seule, nom, effet et raccourci ; à
  texte, effet et raccourci sans répéter le libellé.
- **Texte d'exemple d'un champ vide** (S-T5) : la valeur prise si vide (« Auto »,
  « Group 2 ») ou un exemple de syntaxe (« a*exp(-x/b) »), jamais une
  consigne. Champ de formule (S-T6) : l'icône « ? » seule.
- **Barre d'état** (#230) : information normale, avertissement en teinte
  d'avertissement, erreur en rouge avec icône (S-M4). Une information s'efface
  après ≈ 8 s ; avertissement et erreur restent jusqu'à l'action suivante
  (S-M5). Un clic montre les 50 derniers messages, horodatés (S-M6). Ni
  « Revision N » ni « Reopen Last Import… » (qui reste dans File) (S-M11 → S-R15).
- **Titre de la fenêtre** : nom du projet, « • » tant qu'il n'est pas
  enregistré : « essai.stf • — StokesForge ».
- **Un seul canal par information** (S-M9 → S-R13), deux exceptions écrites : une
  erreur de script affiche « Script failed. See the console » dans la barre
  d'état (S-M10 : l'exception est marquée comme erreur) ; la raison d'un bouton
  principal grisé est dans le pied du dialogue **et** dans sa bulle (S-F10).
- **Erreur dans une fenêtre modale** (S-M7, en place #228) : jamais dans la barre
  d'état ; zone de message du pied, à gauche des boutons, commune à tous les
  dialogues (`AppState::dialog_error`, `dialogs::sync_dialogs`).
- **Échec grave hors fenêtre** (S-M8, en place #229 ; enregistrement raté,
  projet illisible) : dialogue bloquant avec message et action possible
  (`AppState::alert`, `modal_alert` : Close, et Save As… ou Open Another…).
  L'échec d'un export de graphe reste dans la barre d'état.

Historique : #225, #230 (barre d'état), #228 (S-M7), #229 (S-M8).

## Fenêtres et dialogues

- **Application des réglages** (S-F1) : objet visible (graphe, série, objet,
  texte) → changement en direct, OK/Cancel ; Cancel restaure, OK = une entrée
  d'historique. Action qui crée (analyse, import) → brouillon, bouton nommé
  par son verbe (Create, Import).
- **Lecture seule** (S-F2) : un seul bouton Close.
- **Ordre des boutons** (S-F6) : dialogue = secondaires à gauche, puis Cancel,
  puis le principal à droite ; assistant = Cancel à gauche, Back · Next ·
  Finish groupés à droite.
- **Niveaux d'Échap** (S-F7 + S-R1) : une liste ouverte se ferme d'abord ; sinon
  le premier Échap rend au champ sa valeur, le suivant fait Cancel du
  dialogue.
- **Taille et position** (S-F8) mémorisées par sorte de dialogue, dans les
  préférences (hors `.stf`).
- **Focus** (S-F9) : le premier champ de saisie l'a à l'ouverture.
- **Bouton principal grisé** (S-F10 → S-R14) : raison dans le pied et en bulle ;
  une erreur affichée passe devant, la raison revient quand elle disparaît.
- **Nature de fenêtre** (S-F11) : toute fenêtre modale a une nature
  (`DialogNature`) qui fixe son pied, sa taille par défaut, son focus initial et
  son bouton par défaut. La taille vient d'un **champ** de la variante, dont la
  valeur par défaut est une classe : **Petite** 480 x 170 (une question, un
  message), **Moyenne** 480 x 420 (des réglages, un formulaire), **Grande**
  640 x 500 (une liste et son éditeur). Une fenêtre **interactive** (réglages,
  réglages en direct, brouillon, assistant) prend une classe ; hors classes, une **taille libre**
  justifiée dans le code (`// TAILLE-LIBRE(#300): <justification>`). Une fenêtre
  **sans interaction riche** (choix, lecture seule, confirmation,
  enregistrement, alerte) peut prendre la taille de son contenu **sans
  justification** : elle n'a pas de formulaire à ranger, et une taille mal
  choisie y coûte peu. Focus initial : premier champ (réglages, brouillon,
  choix, assistant, S-F9 ; réglages **appliqués en direct** à Close seul, comme
  Préférences : premier champ aussi, car ils ont des champs), bouton par défaut (confirmation : Cancel ;
  enregistrement : Save ; alerte : Close), rien (lecture seule).

Comment c'est tenu (`app/dialogs.rs`, tous par un même corps `modal_frame`) :

- **`modal_dialog_with`** : `[secondaires] [message] [Cancel] [principal]` ;
  **`modal_wizard`** : `[Cancel] [message] [Back · Next] [Finish]` (l'assistant
  d'import) ; **`modal_live_settings`** : `[message] [Close]` avec focus sur le premier champ
  (Préférences) ; **`modal_read_only`** : `[message] [Close]` (métadonnées d'une
  colonne). Un geste d'une fenêtre en lecture seule (« Use as x' ») reste un
  bouton de son contenu.
- **Texte atténué (`.weak()`)** : réservé à ce qui est secondaire — détail
  technique sous une phrase (l'erreur système d'un échec d'enregistrement), raison,
  aide, état vide. Jamais le **contenu principal** d'un dialogue ou d'une
  fenêtre en lecture seule : ce que l'utilisateur doit lire (les compteurs de
  R² faible, un message d'état à retenir) est en texte normal. Le gris d'egui a
  un contraste faible sur fond clair ; en cas de doute, texte normal.
- **Raison du grisage** : `DialogFooter::gated` (#226, S-F11) porte la bulle
  **et** la raison du bouton principal qui peut être grisé
  (`DialogFooter::always` pour celui qui ne l'est jamais) ; la raison s'affiche
  quand il est grisé (clés `disabled-*` : Finish sans
  fichier, Create d'une analyse incomplète — la raison est alors le premier
  manque du formulaire) ; écrite dans le pied **et** dans la bulle du bouton
  grisé ; `accept_hint` reste la bulle du bouton actif.
- **Erreur** (S-M7) : `AppState::fail` l'écrit dans `dialog_error` tant qu'un
  dialogue de contenu est ouvert (import, propriétés, analyse), jamais dans la
  barre d'état ; `notice` l'efface, elle ne survit pas à son dialogue.
  `sync_dialogs` (une fois par image, `show_edit_dialogs`) la publie pour le
  pied.
- **Échap** (S-F7) : celui qui sort d'un champ texte du dialogue ne l'annule pas
  (le focus de l'image précédente fait foi : egui le rend dès le début de la
  passe) ; la brique « nombre » consomme le sien.
- **Taille et position** (S-F8) : à la fermeture, `sync_dialogs` rend
  `Action::RememberDialogGeometry` (sorte = identifiant du dialogue),
  `preferences.json` › `dialogs`, 32 sortes au plus ; reprises à l'ouverture,
  ramenées dans l'écran.
- **Focus** (S-F9) : une ancre invisible en tête du contenu prend le focus à
  l'ouverture, puis le focus avance d'un widget par passe jusqu'au premier
  champ texte, toutes les passes dans la même image (`request_discard`, rien
  n'est peint entre deux pas) ; arrêt (focus rendu) au pied, hors du dialogue, après 32 widgets
  ou au premier geste.

La fenêtre des propriétés d'un objet (rectangle, flèche…) applique aussi ses
réglages en direct (S-F1) : `Action::BeginShapeEdit` ouvre la macro d'historique
au premier changement, chaque réglage est un `SetShape`, OK referme la macro
(`EndShapeEdit { keep: true }`, une entrée), Cancel l'avorte (`keep: false`).

Comment c'est tenu (S-F11, `app/dialogs.rs`) : `DialogNature` est une
énumération à `match` exhaustif sans `_` (pied, classe par défaut, focus
initial, bouton par défaut) ; les briques `modal_*` prennent le champ de taille
de leur nature (`InteractiveSize` : par défaut, classe, libre ;
`PlainSize` : par défaut, classe, contenu), `modal_dialog_with` la nature
(réglages, brouillon ou choix). Gardes (`stokesforge-app/tests/gardes.rs`) :

- **`aucune_taille_libre_sans_justification`** : `InteractiveSize::Free(` du code
  de production exige, dans les 12 lignes qui précèdent, le marqueur exact
  `// TAILLE-LIBRE(#300): <justification>` de 15 caractères au moins. Témoin :
  `la_garde_des_tailles_voit_une_taille_libre_nue` (nue, vide, trop courte,
  autre numéro, sans numéro, trop loin ; justifiée, commentaire, chaîne et code
  de test passent). Tailles libres aujourd'hui : l'analyse, l'assistant
  d'import, le banc `preferences_proto`.
- **`le_catalogue_des_fenetres_et_le_code_concordent`** : tout littéral
  `"stokesforge_…"` passé en argument d'une fonction `modal_*` (avant ses
  fermetures) figure en apostrophes inverses dans `docs/fenetres.md`, et chaque
  identifiant du catalogue a son appel. Les identifiants **construits** avant
  l'appel (`IDENTIFIANTS_CONSTRUITS` : propriétés du graphe, des séries,
  analyse) sont cherchés comme littéraux. Témoin :
  `la_garde_du_catalogue_voit_un_ecart`. **Limites** : elle ne lit pas les
  tailles, les types ni les pages ; une fenêtre dont l'identifiant est dérivé
  (`id.with(…)`, les propriétés d'un objet) n'est pas vue.

Historique : #228, #226 (raison du grisage), #300 (S-F11 : natures de fenêtre, classes de taille, catalogue tenu par une garde).

### Confirmations

- **Seulement pour ce que Ctrl+Z ne rattrape pas** (S-F3) : projet non
  enregistré (Nouveau, Ouvrir, Quitter, fermer la fenêtre), écraser un
  fichier, supprimer un modèle d'import. **Une suppression annulable ne
  demande rien.**
- **Forme** (S-F4) : dialogue modal, Cancel + verbe (Delete, Discard Changes).
  Quitter et Ouvrir : Discard Changes · Cancel · Save. Jamais « Keep » seul.
- **Clavier et aspect** (S-F5 → S-R4, S-R5) : Entrée et Échap = Cancel, bouton par
  défaut encadré sur Cancel ; Delete en style normal, rouge au survol **et**
  au focus clavier. Exception écrite : dans « Discard Changes · Cancel ·
  Save », **Entrée = Save**, Échap = Cancel.

Comment c'est tenu :

- **Briques** (`app/dialogs.rs`, même corps `modal_frame`) :
  `modal_confirm` — `[message] [Cancel] [verbe]`, Cancel encadré (bouton par
  défaut), Entrée et Échap = Cancel, le verbe seulement par son bouton ;
  `modal_save_changes` — `[Discard Changes] [message] [Cancel] [Save]`,
  Entrée = Save ; `modal_alert` — `[action] [message] [Close]` (S-M8). Le style
  destructeur est `theme::destructive_button_visuals` (rouge des styles
  *hovered* et *active* ; egui peint un widget focalisé en *active*).
- **Classement** (`app/confirmations.rs`) : confirmations = projet non
  enregistré (New Project, Open…, Quit, fermeture de la fenêtre :
  `Action::NewProject`/`OpenProject`/`Quit` → `AppState::unsaved_pending`,
  réponse `Action::ConfirmUnsaved`), export dont l'extension `.svg` ajoutée
  viserait un fichier existant que le sélecteur natif n'a pas vu
  (`Action::ConfirmOverwrite` ; le sélecteur natif demande lui-même pour le
  nom saisi), enregistrement sur le fichier source d'une lecture qui a
  écarté des données (`Action::ConfirmDiscardedSave`, #309), suppression d'un modèle d'import (modale peinte dans
  l'assistant, qui lit Entrée et Échap avant lui). **Choix**, pas des
  confirmations : l'axe X d'un dépôt de lignes, la suppression d'un calcul
  avec ou sans ses colonnes, l'export de séries périmées — ils gardent
  `modal_dialog_with`. Supprimer une feuille ou des objets du Project Manager
  ne demande plus rien (`Action::DeleteSheet`, `Action::DeleteObjects`) ; le
  message de la barre d'état nomme les graphes qui en utilisaient les données.
- **Save** : enregistre, puis poursuit le geste (`after_save`) ; sans
  fichier, la fenêtre demande où écrire d'abord ; un sélecteur fermé sans
  choix ou un enregistrement raté (S-M8) abandonne le geste.
- **Fermeture** : `StokesForgeApp::logic` retient toute fermeture de la
  fenêtre (`CancelClose`) et en fait `Action::Quit` ; la fenêtre se ferme
  quand `AppState::take_quit_ready` la libère (l'auto-fermeture de
  vérification ne demande rien).

Historique : #229.

## Grisé

- **Tout élément grisé porte sa raison**, qui dit quoi faire (« Select a
  series first ») (S-G1).
- **Forme** (S-G2) : bulle `.disabled` de l'élément lui-même, jamais mêlée à la
  bulle normale, jamais posée sur un groupe.
- **Mécanique** : les raisons sont les clés `disabled-*` (ou le `.disabled` de l'action quand elle en a un) ; `ToolbarButton` et le bouton principal de `modal_dialog_with` (`DialogFooter::accept_disabled`) posent la leur. Garde : `aucun_element_grise_sans_sa_raison`.
- **Undo/Redo** (S-G3) : grisés, « Nothing to undo » ; actifs, « Undo Rename »
  (libellé d'historique).
- **Champ qui dépend d'une case** (S-G4) : grisé, valeur visible et conservée ;
  masqué seulement si toute la section perd son sens (Min/Max : voir Auto).

Historique : #226.

## Choix, disposition

- **Radios** pour 2–3 options, liste déroulante dès 4 (S-C1) ; horizontaux si
  chaque libellé fait 3 mots au plus, verticaux sinon (S-C2).
- **Choix exclusif dans un menu** : coche seule, sans radio (S-C3, voir
  `menus.md`).
- **Bascule** = case à cocher, libellé à droite ; le toggle « Custom » du
  sélecteur de couleur devient une case (S-C5).
- **Formulaire en grille à deux colonnes** (S-L1) : libellés à gauche, champs
  alignés sur une même verticale, cases dans la colonne des champs.
- **Quatre largeurs de champ** nommées dans `theme.rs` (S-L2) : quelques
  chiffres (~38 px, #237 : entier positif de trois chiffres au plus, sans
  liste — la brique « nombre » la prend seule, `Quantity::is_compact`),
  courte (~80 px), moyenne (~160 px), pleine.
- **Un titre de section entre deux blocs**, jamais un trait seul (S-L3).
- **Deux constantes d'espacement** dans `theme.rs`, aucun `add_space`
  numérique ailleurs (S-L4).
- **Pas de section repliable** dans un formulaire ; trop long → onglets (S-L5).
- **Trois niveaux de texte** (S-L6, #170) : `theme::section_heading` (titre de
  section, gras), `theme::table_header` (en-tête de colonne d'un tableau, gras
  en petit corps), `theme::secondary_text` (atténué, petit corps). Garde
  `aucun_gras_petit_hors_du_theme`.
- **Une hauteur de ligne unique** (S-L7, #170) : `list_row::LIST_ROW_HEIGHT`
  (24 pt) pour `list_row` et pour les lignes composées de l'Object Manager, du
  Project Manager (dossier compris) et de la liste des séries.
- **Tab suit l'ordre de lecture** (S-K6) : haut-bas, gauche-droite.

Comment c'est tenu :

- **`app/form.rs`** : `form(ui, sel, |rows| …)` ouvre la grille ; `Form::row`
  (libellé + champ, l'icône « ? » après le champ), `Form::field` (colonne des
  champs sans libellé), `Form::checkbox`, `Form::color`, `Form::note` (texte en
  clair estompé), `Form::section` (titre sur toute la largeur), `Form::enabled`
  (rangées grisées, S-G4), `Form::ui` (tableau ou liste sur toute la largeur).
  La colonne des libellés prend la largeur du plus long, mesurée à l'image
  précédente ; une image dont elle change est refaite (`request_discard`).
  Pas d'`egui::Grid` : pas de cellule sur deux colonnes pour un titre, et
  `add_space` y est interdit. S-K6 tient par construction (rangées peintes de
  haut en bas, de gauche à droite).
- **`choice` / `choice_by`** (`ChoiceOption`, rappel `hint` facultatif) peint
  un choix exclusif **fixe** selon `choice_layout` (S-C1, S-C2, testé pur). Une
  liste de données (colonnes, graphiques, modèles enregistrés, fichiers
  récents, calques) reste une liste déroulante quel que soit son nombre. Garde
  « liste de moins de 4 options » étudiée et non posée : une `ComboBox` de
  données ne se distingue pas d'un choix fixe à la lecture du code.
- **`menu_sections::menu_choice`** : coche dans la marge devant l'option
  courante (Type ▸, Role ▸, Language ▸, SVG Text ▸, PNG Resolution ▸). `Move ▸` des
  colonnes (#319) est un sous-menu d'actions, grisées avec leur raison, pas un choix
  exclusif : pas de coche.
- **`theme.rs`** : `SPACE_ITEM`, `SPACE_SECTION`, `FIELD_WIDTH_SHORT`
  (aussi `number_field::FIELD_WIDTH`), `FIELD_WIDTH_MEDIUM`, `FieldWidth`.
  Garde `aucun_add_space_litteral_hors_du_theme` (`tests/gardes.rs`), sans
  dérogation ; un écart calculé passe.
- Migrés : propriétés du graphe (toutes les pages, éditeur de série partagé
  avec la fenêtre des séries), formulaire d'analyse, assistant d'import
  (fichiers, format, en-tête, métadonnées, destination ; les traits entre
  blocs sont devenus des titres). La fenêtre des séries (groupe, portée,
  nuancier — `curve_settings.rs`) est migrée, ainsi que les outils de
  la portée du volet Results (#241 ;
  son tableau série × valeur et les listes « de / à » restent des
  tableaux/listes de données). Reste à migrer : les préférences futures (#148).

Historique : #227, #237 (largeur « quelques chiffres »), #241 (volet Results).

## Clavier et souris

- **Cibles peintes à la main et accessibilité** (#170) : une pastille de
  calque annonce son numéro et sa sélection ; une poignée de fenêtre annonce
  la direction de redimensionnement. Le nom vient du catalogue et l'état
  accessible suit le même état que le dessin (`Response::widget_info`).
- **Registre unique des touches** dans `app/shortcuts.rs` (S-K1 : F2, Suppr,
  Échap, Entrée…) ; aucun `consume_key` ailleurs.
- **Suppr** supprime la sélection tout de suite, annulable, partout (S-K2) :
  Project Manager, Object Manager, figure. La grille garde son exception
  (vider d'abord) ; les onglets de feuille n'ont pas de Suppr (le menu suffit,
  `shortcuts.rs`).
- **Double-clic** (S-K3 → S-R7) : texte ou nom (cellule, onglet, titre) → édition
  sur place ; objet (série, axe) → Properties ; fond vide d'un graphe → vue
  réinitialisée ; Project Manager → active ou restaure la fenêtre, **ne ferme
  jamais**. Sur un **dossier**, un clic le sélectionne et le déplie s'il est
  fermé (jamais ne le replie), le double-clic le replie (#170).
- **Help › Keyboard Shortcuts**, généré depuis `shortcuts.rs` (S-K5).
- **Ctrl+molette = zoom**, partout (S-X1 → S-R1, S-R7).
- **Curseurs** (S-X2) : main ouverte puis fermée pour glisser, flèche de
  redimensionnement sur une poignée, doigt sur un élément cliquable qui n'est
  pas un bouton.
- **Trait de dépôt unique** (S-X5, #170) : survoler une fenêtre ou un classeur avec un fichier du système, un graphe avec des colonnes ou des lignes, se voit du même trait, `theme::drop_stroke` (vert de l'accent). Tests : `file_drop::tests`, `graph_window::tests`, `theme::tests`.
- **Pas de sélecteur d'outil dans une fenêtre** (S-K7, #299). L'objet d'abord :
  on clique ou on glisse l'objet lui-même (une courbe, une poignée, une
  cellule) ; les outils (Pointeur, Texte, Zoom, formes) vivent dans la barre
  Tools et nulle part ailleurs. C'est une *guideline* : on peut en sortir, mais
  **toute sortie est justifiée deux fois**, dans le code et ici.
- **Les gestes sont dans une matrice** (S-K8, #299) : [`gestes.md`](gestes.md)
  dit ce que font clic, glisser, double-clic, molette, Ctrl + molette, Ctrl +
  glisser, clic milieu et clic droit sur chaque surface (graphe, aperçu d'analyse, grille,
  arbres, onglets, fenêtres, champs). Une surface nouvelle, ou un geste nouveau
  sur une surface existante, s'y range ; un geste qui diffère de la même action
  ailleurs se signale contre cette page. La matrice n'est pas recopiée ici.
  Le déplacement de la vue est le même geste partout (#318, #270) : Ctrl +
  glisser ou clic milieu + glisser, sur le graphe comme sur l'aperçu
  d'analyse ; le glisser simple ne déplace jamais la vue (il déplace l'objet
  saisi, ou rien), et parti d'un axe, le geste ne déplace que cette échelle.

Comment c'est tenu (S-K7, S-K8) :

- **`aucun_selecteur_d_outil_hors_de_la_barre_tools`**
  (`stokesforge-app/tests/gardes.rs`) : un choix exclusif (brique `choice` /
  `choice_by`, `selectable_label`, `selectable_value`, `Button::selectable`,
  `RadioButton::new`, `radio`, `radio_value`, `toggle_value`) dont les
  arguments nomment un outil (« tool », hors « toolbar » et « tooltip ») est
  refusé dans tout fichier sauf `toolbar.rs`, à moins que l'une des **12 lignes
  qui précèdent** (ou la ligne même) porte le marqueur exact
  `// OUTIL-EN-FENETRE(#299): <justification>`, justification d'au moins 15
  caractères. Témoin : `la_garde_des_outils_voit_un_selecteur_d_outil_nu`
  (chaque forme de choix, exception vide, trop courte, d'un autre marqueur ou
  trop loin). Limite : elle repose sur le mot « tool » dans les arguments ; un
  sélecteur d'outil sans ce mot lui échappe.
- **`aucun_geste_hors_de_la_matrice`** : tout fichier de production de
  `stokesforge-app` ou de `stokesforge-render-egui` qui lit une molette
  (`scroll_delta`, `zoom_delta`, `MouseWheel`) ou un double-clic
  (`double_clicked`) doit être cité (chemin `….rs` entre apostrophes inverses)
  dans `gestes.md` ; un fichier cité qui n'existe plus fait aussi échouer la
  garde. Témoin : `la_garde_des_gestes_voit_un_geste_hors_de_la_matrice`.
  **Limites réelles** : elle travaille au niveau du **fichier** (un geste de
  molette nouveau dans un fichier déjà cité passe, et elle ne lit pas ce que
  fait le geste) ; elle ne voit que la **molette et le double-clic**, ni le
  clic, le glisser, le clic droit ni les modificateurs. Elle empêche qu'une
  surface naisse hors de la page, pas qu'un geste y soit faux.

**Exception : les gestes d'analyse** (S-K7). La fenêtre d'analyse garde, dans
son aperçu, un sélecteur de geste : **Point** (un clic pose un point d'ancrage,
une graine de pic, une borne) et **Zone** (un glisser trace une zone ou une
plage), en deux **textes entourés** à bascule (`toggle_texts`, `app/form.rs` :
cadre arrondi, actif en fond plein de l'accent, inactif en contour seul),
marqueur `OUTIL-EN-FENETRE(#299)` dans `app/analysis_dialog.rs`. Justification :
ces gestes demandent des actions propres à l'aperçu (ajouter un point, tracer
une zone, aimanter à la courbe) ; les mettre dans la barre Tools du graphe
mélangerait ces gestes à ceux de la figure. L'exception est la plus étroite
possible : **seule la détection de pics** a besoin des deux (position d'un pic
par un clic, plage de recherche par un glisser). Toute autre analyse n'a aucun
geste à choisir : celui de son opération (`AnalysisForm::natural_tool`,
`tool_choices`) est actif d'emblée, et la rangée n'est pas peinte. Le Zoom
n'est plus un outil de l'aperçu (D2, D3 de #299, #270).

**Navigation de l'aperçu** (D3 de #299, #270) : comme sur le graphe. Ctrl +
molette zoome, ancré sous le pointeur (au-dessus d'un axe : cette échelle
seule) ; Ctrl + glisser déplace la vue (parti d'un axe : cet axe seul) ;
double-clic recadre ; Maj + molette règle la moyenne de l'aimant. La molette
seule ne fait rien. Le calcul du zoom et du déplacement est celui de
`stokesforge-render` (`PanelTransform`) ; le détail par geste est dans
[`gestes.md`](gestes.md).

Historique : #231, #299 (S-K7, S-K8 : guideline des outils et matrice des gestes).

## Travaux longs et états vides

- Au-delà d'≈ 0,5 s : indicateur tournant + mot (« Importing… ») à l'endroit
  du résultat ; barre de progression si le nombre d'étapes est connu (S-P1).
- Bouton **Stop** pour tout travail de plus de quelques secondes (import
  multiple, script, recalcul lourd) (S-P2). L'import par glisser-déposer suit la
  même voie que File › Import (lecture, choix du modèle et analyse sur le fil
  de fond, #266).
- **État vide** : phrase grisée centrée qui dit ce qui manque et le geste pour
  le créer (S-P3) : `empty_state`, partout (classeur sans colonne ou sans
  feuille, graphe sans série, Object Manager, Results, historique des messages,
  console de script).
- Seuils : l'indicateur après 0,5 s (`busy::SHOW_AFTER`), Stop après 3 s
  (`busy::STOP_AFTER`). **Stop de l'import** : le fil s'arrête après le fichier
  en cours, rien n'est créé (`AppState::stop_import`). **Stop du script** : la
  console garde le sien (#215). **Recalcul de formules** (#239) : une commande attend 0,2 s
  (`RECALC_WAIT`) la fin du recalcul qu'elle lance — un recalcul bref reste
  synchrone et invisible — puis rend la main ; la barre d'état montre
  « Recalculating… » et Stop (`Action::StopRecalc`). Stop interrompt entre
  deux colonnes puis **annule la commande** comme Ctrl+Z (rétablissable) et
  recalcule ce qui a changé. Tout autre geste attend la fin du recalcul
  (`docs/session.md`).

Historique : #232, #239 (recalcul de formules), #266 (import par glisser-déposer), #215 (Stop du script).

## Hors de cette page

- **Fenêtre Edit › Preferences…** pour les réglages d'application, les menus
  gardent les bascules d'affichage fréquentes (S-X3) : #148.
- **Registre d'actions** (S-X4) : une table par action (libellé, logo,
  raccourci, condition de grisage et sa raison), lue par les menus, les
  barres d'outils et le clic droit ; il portera S-G1 et S-K1 (#110).

## Une règle qui paraît fausse

Un comportement qui ne suit pas cette page se signale contre elle
(`stokesforge-bug-report`). Une règle qui ne convient pas se rediscute avec
l'utilisateur, elle ne se contourne pas : la changer, c'est changer cette
page, l'issue concernée et les skills dans le même commit.

### Options indisponibles d'un choix exclusif

`ChoiceOption::disabled(Some(reason))` grise une option du formulaire et
affiche sa raison au survol. Le même refus vaut pour les radios et les
listes déroulantes : une option grisée ne change pas la valeur sélectionnée.
Le choix reste peint par la brique `choice` ; ne pas reconstruire des radios
ad hoc pour une indisponibilité.

Historique : #212.
