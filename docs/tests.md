# Tests et intégration continue

## Licences et notices (#129)

Le travail automatique et bloquant `licenses` installe les versions de
`scripts/licenses-tools.env`, exécute `cargo deny --locked check licenses`
puis produit les notices Rust avec `cargo-about`. La résolution porte sur
toutes les plateformes et features, y compris `python`, sans compilation :
ce contrôle ne valide pas le pont Python. L'artefact conserve les textes
générés à partir du verrou de ce pipeline.
`scripts/licenses-test.sh` vérifie les témoins de refus : runtime sans notice,
jeu de notices Python périmé, compléments pour un autre verrou Cargo et
dépendance GPL-2.0-only hors politique. Ces
sondes utilisent des répertoires temporaires, sans modifier les sources.

Les distributions appellent `scripts/licenses-dist.sh`, qui ajoute les textes
de StokesForge, les notices Rust, OFL et Phosphor, celles des patches, ainsi
que CPython, ses bibliothèques natives et numpy. Il refuse un verrou Python
différent du jeu de notices versionné et un runtime sans les licences de
CPython/numpy. Politique et commandes :
[`licence-et-extensions.md`](licence-et-extensions.md).

## Trois familles

| Famille | Où | Ce qu'elle garde |
|---|---|---|
| **tests unitaires** | `#[cfg(test)]` dans `src/` | un comportement local, au plus près de son code |
| **tests d'intégration** | `crates/*/tests/` | une propriété de crate : contrat public, frontière, accord entre chemins |
| **banc de robustesse** | `stokesforge-robustesse/tests/` | le socle aux limites — 22 batteries `b1`…`b22` |

## Les garde-fous à ne pas casser

Quelques tests ne vérifient pas un comportement mais **une propriété
d'architecture**. Ils échouent exprès quand quelqu'un franchit une frontière
par inadvertance, et ce sont eux qu'il faut comprendre avant de les modifier.

| Test | Ce qu'il tient |
|---|---|
| `*/tests/no_ui_dependency.rs` (onze crates, dont `stokesforge-app-state` depuis #263 : ni `egui`, ni `eframe`, ni `winit`, ni `stokesforge-clipboard`) | liste blanche de dépendances : toute addition au `Cargo.toml` doit y être ajoutée explicitement (invariant 2). Lecture commune `stokesforge-arch-guard` : dépendances normales, de build, `target.*` et renommées (`package`) ; `dev-dependencies` exclues ; témoins négatifs dans `stokesforge-arch-guard/src/lib.rs` |
| `stokesforge-arch-guard/tests/renvois_fiches.rs` (F1, #301) | les fiches `.claude/skills/stokesforge-feature/references/*.md` ne portent que des renvois « `fichier.md` § Titre » : chaque fichier et chaque titre cité doit exister, un `§` sans chemin est refusé, et le `SKILL.md` liste exactement les fiches présentes |
| `stokesforge-arch-guard/tests/codes_standard.rs` (#301) | tout code `S-XN` cité (docs, skills, commentaires, tests) figure dans l'index des règles de `docs/standard-interface.md` ; témoin sur un code inconnu |
| `stokesforge-render-svg/tests/frontiere.rs` | l'export ne voit ni `egui`, ni `eframe`, ni `winit` — donc se teste sans serveur graphique |
| `stokesforge-render/tests/mesure.rs` | la source unique de mesure de texte (invariant 4) |
| `stokesforge-render/src/{view,interact}.rs` (modules `tests`) | interaction sans egui (#262) : cadrage d'un geste (zoom, pan, échelle log, axe inversé, calques superposés, axe seul, panneau inconnu), réinitialisation, point le plus proche (courbe, hors de portée, série masquée ou de droite, calque du spec) ; mutants éprouvés (signe du pan, filtre de case, calque lié, rayon, série masquée) ; géométrie de l'aperçu d'analyse (#261) : `PanelTransform::new` (origine fractionnaire, log inversé, zoom, rectangle), cadre initial (`fit_range`, `axis_frame`), `snap_to_curve` (n, bord, `ys` trop court) et `scale::axis_ticks` (mêmes points que le pipeline, décades, virgule, borne sans image) ; mutants éprouvés (origine arrondie, `1 - fy`, filtre `> 0` en log, demi-fenêtre, `label_count` figé à 5, virgule oubliée, `abs` de la tolérance du clic droit) ; `app/analysis_preview.rs` (`zone_tests`) : glisser de zone, clic droit sur axe X inversé ; navigation de l'aperçu (#270, banc `Banc` image par image) : molette seule sans effet, Ctrl + molette et pincement (donnée sous le pointeur gardée), zoom par axe, Maj + molette = moyenne sans zoom, Ctrl + glisser (cadre translaté, aucune zone, axe X inversé, par axe), Ctrl + clic sans point, double-clic recadre, pics (clic = graine, glisser = plage) ; mutants éprouvés (signe du pan, molette seule, `!command` du clic, bornes d'axe, mode du glisser, lecture de la molette sous Maj, facteur inversé, double-clic) ; `app/form.rs` : textes entourés (`toggle_texts` : fond plein de l'option courante, bascule, clic sur la courante) ; `stokesforge-app-state/tests/analysis.rs` : `tool_choices` (seuls les pics ont le choix) |
| `stokesforge-render-egui/tests/accord.rs` | écran et export placent les mêmes primitives aux mêmes pixels |
| `stokesforge-render-egui/tests/{interaction,cache_session}.rs` | gestes du graphe rejoués image par image (#318) : glisser simple sans effet (cadrage, génération, cache), Ctrl ou clic milieu + glisser = pan (fond, axe seul, outils Zoom et de forme, au-dessus d'un texte, titre ou forme), Ctrl lu à la pression, Ctrl + clic sans texte ni forme, feathering coupé seulement pendant un geste réel ; mutants éprouvés (pan au glisser simple, bouton milieu, garde d'objet, garde du rectangle, du texte et du tracé, feathering) |
| `stokesforge-render-egui/tests/feathering.rs` | le feathering n'entre pas dans la clé de cache (invariant 6) |
| `stokesforge-app-state/tests/script_bridge_lazy.rs` | l'interpréteur Python ne démarre pas à l'ouverture de la fenêtre |
| `stokesforge-app/tests/libelles.rs` | aucun texte d'interface en dur (`aucun_libelle_en_dur`, garde absolue, sans dérogation, #48), y compris les titres `set_title` et noms visibles `add_filter` des boîtes de fichier (#127 ; extensions et sigles de format techniques préservés) et les fonctions libres `section_title`, `bold`, `list_row`, `list_row_text` (#169) ; la zone #43 (`ZONE_43`) sans aucun littéral, ses fonctions cherchées par leur nom dans toutes les sources — une entrée introuvable échoue ; les dépendances de traduction restent dans `stokesforge-app` et `stokesforge-app-state` ; `aucune_cle_du_catalogue_n_est_morte` (une clé de `en` sans emploi dans les sources des deux crates, témoin `la_garde_des_cles_mortes_lit_les_deux_crates`) ; les clés `history-*` employées ; `notice`/`fail` de `app-state/state.rs` lus (`la_garde_lit_les_messages_d_etat_du_crate_d_etat`) ; les témoins qui prouvent que la garde voit un littéral, y compris dans un sous-dossier de `src/` |
| `stokesforge-app/tests/gardes.rs` | aucune `egui::Window` brute (`aucune_fenetre_n_est_une_egui_window_brute`) ; la fenêtre des propriétés passe par `modal_dialog` → `modal_dialog_with` → `modal_frame` → `show_window_in_layer` (comme `modal_wizard`, l'assistant d'import, `modal_live_settings` et `modal_read_only`, #228, puis `modal_confirm`, `modal_save_changes` et `modal_alert`, #229) ; aucun import global (`use …::*`) hors du code de test (`aucun_import_global_hors_des_tests`, décision 2 de #115, absolue depuis #117) ; aucun `ui.separator()` dans un menu hors de la brique de sections (`aucun_separateur_brut_dans_un_menu`, #155, absolue) ; bouton sélectionnable d'egui seulement dans un menu, une liste déroulante ou le sélecteur de couleur (`aucun_bouton_selectionnable_hors_d_un_menu`, #169) ; aucune couleur `Color32::from_…` ni teinte nommée hors de `theme.rs` et `color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`, #169) ; aucun `ui.separator()` juste avant un titre de section (`aucun_separateur_avant_un_titre_de_section`, #169) ; ni `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`, #220, absolue) ; aucune bulle nue (`on_hover_text`/`on_hover_ui`) sur une case, un radio, un champ texte ou nombre, un libellé de champ hors menu (`aucune_bulle_nue_sur_un_champ_de_formulaire`, #225, absolue — le rappel passe par `help_beside`/`help_text`) ; tout `add_enabled` porte `.on_disabled_hover_text` (`aucun_element_grise_sans_sa_raison`, #226, absolue) ; aucun `add_space(<nombre>)` hors de `theme.rs` (`aucun_add_space_litteral_hors_du_theme`, #227, absolue — `theme::SPACE_ITEM` / `SPACE_SECTION`) ; un `rfd::` seulement dans `file_picker.rs` de `stokesforge-app-state` (`aucun_selecteur_rfd_hors_du_module`, #192, témoin `la_garde_du_selecteur_voit_un_rfd_hors_du_module`) ; aucun test d'intégration (des deux dossiers `tests/`) n'appelle `AppState::new` (`aucun_test_d_integration_n_utilise_appstate_new`) ; l'ancienne garde « les modules d'état ne nomment pas egui » (`MODULES_SANS_EGUI`, #255) est retirée : depuis #263 le compilateur et `stokesforge-app-state/tests/no_ui_dependency.rs` tiennent la frontière ; aucune taille libre de fenêtre interactive sans `// TAILLE-LIBRE(#300): <justification>` dans les 12 lignes précédentes (`aucune_taille_libre_sans_justification`, #300, S-F11, témoin `la_garde_des_tailles_voit_une_taille_libre_nue`, mutation éprouvée : marqueur retiré) ; tout identifiant `stokesforge_…` passé à un `modal_*` figure dans `docs/fenetres.md` et chaque ligne du catalogue a son appel, sans lire les tailles (`le_catalogue_des_fenetres_et_le_code_concordent`, #300, témoin `la_garde_du_catalogue_voit_un_ecart`, mutation éprouvée : ligne du catalogue retirée) ; les témoins du lecteur de sources |
| `stokesforge-app/src/app/dialogs.rs` (module `tests`) | natures de fenêtre (#300, S-F11) : `chaque_nature_fixe_son_pied_sa_taille_et_son_focus` (table nature → pied, classe par défaut Petite 480 x 170 / Moyenne 480 x 420 / Grande 640 x 500, focus ; le champ de la variante l'emporte), `le_focus_initial_suit_la_nature` (confirmation → Cancel, enregistrement et alerte → principal, jamais le champ du contenu ; lecture seule → rien ; témoin : le brouillon va au champ), `un_pied_grise_montre_sa_raison_et_non_sa_bulle` (`DialogFooter::gated` / `always`) ; témoin S-F8 gardé (`un_dialogue_rouvert_reprend_sa_taille_et_sa_place`) ; mutants éprouvés (Petite et Moyenne intervertis, confirmation passée au premier champ) |
| `stokesforge-app/src/theme.rs` (module `tests`) | aucun `ScrollStyle` hors du thème (`aucun_scroll_style_hors_du_theme`) ; aucun `strong()` d'egui, tout gras par `theme::bold` (`aucun_strong_d_egui_hors_du_theme`, #169) ; `bold` élargit vraiment le texte avec les polices embarquées |
| `stokesforge-app/tests/support/sources.rs` | pas un test : le **lecteur unique** de toutes les gardes qui lisent le code (`libelles.rs`, `gardes.rs`, et par `src/lib.rs` celles d'`icons` et `theme`). Lit récursivement `src/` des **deux** crates (`stokesforge-app` et `stokesforge-app-state`, ce dernier sous le préfixe `app-state/` : `app-state/state.rs`) ; vue de production sans commentaires ni items `#[cfg(test)]` (chacun, pas seulement le premier ; un `#[cfg(test)] mod x;` rend son fichier entier de test) ; cherche une fonction par son nom, où qu'elle vive (#116). Une garde nouvelle qui lit le code passe par lui, jamais par `include_str!` ni un fichier nommé |
| `stokesforge-app/src/field_state.rs` (tests) | état de champ (#219) : décision pure du cycle (`decide`), cadre rouge/orange et bulle lus dans les formes peintes, ancienne valeur rendue à la sortie, orange éteint après 2 s, témoin « saisie valide → aucun cadre » ; mutants éprouvés (couleur, champ gardé ouvert, échéance, sortie sans changement, bulle) |
| `app/busy.rs`, `app/empty_state.rs`, `stokesforge-app-state/tests/shell.rs` (#232) | indicateur absent avant 0,5 s puis mot, Stop seulement après 3 s et seulement si le travail s'interrompt ; phrase d'état vide grisée et centrée ; Stop de l'import : rien créé, aucune entrée d'historique, un import ultérieur va au bout |
| `stokesforge-app/tests/gardes.rs`, `app/shortcuts.rs`, `app/keyboard_shortcuts_window.rs`, `project_manager.rs` (tests) | clavier et souris (#231) : gardes `aucun_consume_key_hors_du_registre` et `aucun_curseur_hors_du_module_des_curseurs` (témoins), toute constante consommée figure dans `HELP`, fenêtre des raccourcis, menu Help, double-clic du Project Manager (active, ne ferme jamais, deux Ctrl-clics n'ouvrent pas) |
| `stokesforge-app/src/number_field.rs` (tests) | brique « nombre » (#220) : lecture (refus, bornes, entiers), pas et grands pas, liste de valeurs au clavier (Alt+↓, Entrée, Échap), clic = tout sélectionné, molette au survol sans focus qui suit la liste (`step_preset` : hors liste, extrémités), champ sans liste = fenêtre qui défile, flèche pleine dans le cadre (`list_frame` : fonds du thème distincts), cohérence de la table des grandeurs ; dans `plot_settings` : saisie validée à Entrée, crans de molette = une entrée d'historique, Échap rend la valeur puis annule le dialogue |
| `stokesforge-app/src/app/dialogs.rs` (tests) | brique des dialogues (#228) : ordre des boutons lu dans les formes (dialogue, assistant, lecture seule), décision pure du pied (`footer_message` : l'erreur passe devant la raison) et sa peinture (la raison revient quand l'erreur disparaît, témoin bouton actif), focus initial sur le premier champ texte et aucun focus sans champ, Échap qui sort d'un champ puis annule, géométrie rendue à la fermeture et reprise à l'ouverture ; mutants éprouvés (focus initial, Échap) ; côté état (`tests/shell.rs`) : géométrie écrite dans les préférences hors historique, erreur dans le pied du dialogue ouvert seulement. Confirmations (#229) : Entrée et Échap = Cancel dans `modal_confirm` (le clic du verbe confirme), Entrée = Save et Échap = Cancel dans le trio, Discard Changes à gauche, verbe rouge au survol puis au focus clavier (Tab), pointeur parti au repos ; `modal_alert` : Entrée = Close, action par son bouton ; mutants éprouvés (Entrée de la confirmation, rouge au focus) |
| `stokesforge-app` : confirmations côté état et fenêtre (#229) | `stokesforge-app-state/tests/shell.rs` : New Project / Quit / Open… ne demandent que s'il y a du travail à perdre, Save enregistre puis poursuit (et un échec ou un sélecteur fermé abandonne le geste), projet illisible : chemin et projet gardés (S-M8), enregistrement raté (S-M8), export dont l'extension ajoutée viserait un fichier existant ; feuille et objets supprimés sans confirmation, message qui nomme les graphes (`tests/project_objects.rs`) ; `app::tests::fermer_la_fenetre_demande_avant_de_perdre_le_travail` (CancelClose, puis Close après Discard) ; `import_wizard::tests::supprimer_un_modele_passe_par_une_confirmation_modale` (Entrée annule sans valider l'assistant) |
| `stokesforge-core/src/calc.rs` (module `output_name_tests`), `stokesforge-app-state/tests/output_names.rs`, `stokesforge-script/tests/analysis.rs` (#256) | noms des colonnes de sortie d'analyse : la règle (`OutputName::of`, `output_unit`, `wants_output`, `family_english`) se teste dans core ; le **test d'accord** vérifie que le catalogue `en` dit `OutputName::english()` pour chaque variante (liste d'exemples gardée par un `rank()` exhaustif) et `analysis-kind-*` = `family_english` ; `pyst` donne les mêmes nom, unité et commentaire que l'interface, et `peak_columns`/`sum_column` filtrent comme les cases. Une variante nouvelle d'`OutputName` ne compile pas sans clé `analysis-output-*` |
| `stokesforge-app-state/tests/shell.rs` (copie de la figure) et `stokesforge-app/src/app/figure_copy.rs` (tests), #149/#263 | l'état prépare des octets neutres (`FigureBytes`) dans l'ordre des préférences (SVG, PNG, PDF par défaut ; ordre et cases décochées suivis), sans toucher au modèle ; la fenêtre traduit chaque `ExportFormat` en son type MIME (`mime_of`, témoin : deux types échangés font échouer le test) et chaque `ClipboardError` en phrase qui reprend le détail (`clipboard_error_message`) ; le refus du système remplace le message « copié » |
| `stokesforge-app-state/src/i18n.rs` (tests) | toute clé de `fr` existe dans `en` avec les mêmes arguments ; la pseudo-langue entoure et allonge chaque message ; complétude bloquante (toute clé de `en` existe dans `fr`) ; pluriels et typographie françaises ; une préférence `fr` ouvre l'interface en français ; choix de la langue sans panique ; `tests/language_switch.rs` : la langue change sans redémarrer |
| `stokesforge-commands/tests/memory_pinning.rs` | l'historique n'épingle pas indéfiniment de gros buffers |

### Le témoin, sans lequel un test d'égalité ne prouve rien

C'est la méthode la plus importante du dépôt, et elle vaut au-delà du rendu.

Un test qui compare deux chemins peut passer parce que les deux sont justes —
ou parce que **les deux sont faux de la même façon**. `tests/mesure.rs` et
`tests/accord.rs` portent donc, chacun, des cas qui doivent être **différents** :

- la même figure tracée avec le `SVGBackend` **nu**, celui qui retombe sur
  l'heuristique par octets UTF-8, doit se placer **ailleurs** ;
- pour chaque fonctionnalité ajoutée, une variante du graphe qui la change doit
  produire une figure **différente**.

Sans eux, une enveloppe qui déléguerait `estimate_text_size` par erreur
passerait pour correcte. Si vous touchez à ces tests, lisez leurs témoins
d'abord.

## Le banc de robustesse

`stokesforge-robustesse` ne contient **aucun code de production**. Il dépend de tout
le workspace et porte deux choses :

- **22 batteries** (`tests/b1…b22`) qui éprouvent le socle aux limites :
  valeurs numériques extrêmes (`b1`), persistance (`b2`), fichiers `.stf`
  malformés puis sémantiquement hostiles (`b3`), allocateur d'identifiants
  (`b4`), macros (`b5`), concurrence entre clients (`b6`), frontière
  Rust ↔ Python (`b7`), propriétés tirées au sort par proptest (`b8`), script
  sous contrainte (`b9`), compteur de version (`b10`), portée d'unicité des
  noms (`b11`), commandes acceptées sans effet (`b12`), bornes d'un `PlotSpec`
  venu du disque (`b13`), balayage revérifiant trois invariants après chaque pas
  d'une séquence tirée au sort (`b14`), traduction cellule ↔ texte (`b15`),
  séries aux colonnes de longueurs inégales (`b16`), coût des opérations quand
  le projet grandit (`b17`), ce qui reste possible pendant qu'une macro est
  ouverte (`b18`), fichier écrit par une version plus récente (`b19`), deux
  écrivains sur un même fichier (`b20`), coût et identité sur le chemin
  d'écriture (`b21`), recalcul ciblé des formules contre le recalcul complet
  (`b22`, voir « Recalcul ciblé » ci-dessous) ;
- **deux sondes hors-processus**, `sonde_alloc` et `sonde_pile`, parce que les
  cas qu'elles éprouvent se terminent par un `abort()` que rien ne rattrape :
  un échec d'allocation Rust n'est pas une panique rattrapable, et un
  débordement de pile non plus (le runtime écrit `has overflowed its stack` et
  appelle `abort()` — ni `catch_unwind` ni `join` ne rendent la main, et le
  binaire de test mourrait avec).

`sonde_pile` utilise par défaut une pile de 2 Mio : la taille qu'un
`std::thread` reçoit quand personne ne la fixe, c'est-à-dire celle du fil de
l'acteur.

### Deux natures de test, et comment les distinguer

**Les garde-fous** passent et doivent continuer à passer : ils entrent dans
`cargo test --workspace` comme les autres.

**Les reproductions** échouent délibérément, et sont marquées `#[ignore]` pour
que le pipeline garde sa valeur de signal — un CI rouge en permanence ne dit
plus rien. Une reproduction qui se met à **passer** est le signal attendu : le
correctif est bon, et c'est le moment de retirer son `#[ignore]` pour que le
garde-fou prenne le relais.

La campagne de correction issue des rapports du 2026-09-11 est close : il ne
reste qu'un `#[ignore]`, dans `b9`, dont le motif est écrit en toutes lettres
dans l'attribut (il dépend de l'ordonnancement, et son motif original ne prévoit
pas de boucler sur `ConcurrentWriteError`).

Mesurer plutôt que chronométrer : `b17` et `b21` comptent des **octets
alloués**, pas des millisecondes, pour que l'assertion dise « ce coût ne doit
pas croître avec X » et reste vraie sur n'importe quelle machine.
Exception assumée (#316) : `b17_charger_un_dossier_a_beaucoup_d_enfants_n_est_pas_quadratique`
borne un temps (10 s, très large) car un coût quadratique en CPU n'alloue rien.
Les refus de `MAX_COLUMN_ROWS` à la lecture se testent par `check_rows` et des
limites minuscules (`convert.rs`), jamais avec 10^8 lignes.

## Recalcul ciblé (#193) — et ce qu'il faut tester en s'appuyant dessus

Depuis #193, l'acteur ne réécrit une colonne-formule que si une donnée de son
calcul a changé (`stokesforge-session/src/recalc.rs`, [`session.md`](session.md)).
Ce genre d'optimisation n'a qu'un mode de défaillance, et il est
**silencieux** : une donnée oubliée dans l'empreinte, et la valeur affichée
reste fausse sans la moindre erreur. Les tests ci-dessous sont faits pour ce
cas-là, et c'est sur eux que doit s'appuyer tout ce qui réutilisera le
mécanisme (analyses recalculées #184, références entre feuilles #186,
nouvelles fonctions de formule).

### Ce qui le tient aujourd'hui

| Test | Ce qu'il tient |
|---|---|
| `stokesforge-robustesse/tests/b22_recalcul_cible.rs` | **l'oracle** : deux sessions jumelles, l'une ciblée, l'autre en recalcul complet (`FormulaRecalc::Full`, le code d'avant #193), reçoivent les mêmes pas tirés au sort ; mêmes réponses et mêmes arbres exigés après chaque pas. Plus, à chaque pas : la doctrine version ↔ contenu **dans l'acteur** (écritures du recalcul, macros, rechargements compris), et le témoin du « ciblé » — une commande sans rapport ne change la version d'aucune colonne. Un contrôle de couverture final exige des formules calculées, non calculées et épargnées, et **au moins vingt passes par nature de pas** : un geste toujours refusé ne prouverait rien. Le tirage est rejouable à l'identique : aucune cible n'y est choisie dans l'ordre de `Tree::iter` (table de hachage à graine aléatoire) |
| `stokesforge-session/tests/recalc_cible.rs` | un cas nommé par composante de l'empreinte (formule stockée, appartenance à la feuille et version des sources, longueur du résultat, version propre, issue), les gestes qui les font changer (déplacement dans la feuille ou vers une autre, suppression puis annulation, cycle créé ailleurs, suppression de cellules dans une colonne-formule, macro avortée, feuille dupliquée…), la stabilité d'une chaîne et d'un cycle, l'oubli du mémo par `Load` et `NewProject` sur une coïncidence d'identifiants et de versions **fabriquée et vérifiée**. Chaque assertion « pas réécrite » a son témoin en mode complet |
| `stokesforge-session/src/lib.rs` (`tests`), `src/recalc.rs` (`tests`) | le recalcul interruptible (#239) : Stop annule la commande et laisse rétablir (arrêt posé d'avance sur le prochain recalcul, donc déterministe), Stop après une commande **sans effet** ne défait rien (`History::execute_recorded`), la mort de l'acteur (panic simulé par `Request::PanicForTest`, ou en plein recalcul) libère la jauge et rend `actor_alive` faux (#310), Stop tardif sans effet sur le suivant, arrêt entre deux colonnes puis passe suivante qui achève sans réécrire ce qui l'est déjà. `stokesforge-app-state/tests/shell.rs` (`heavy`) : le même geste vu de l'interface sur un projet lourd, avec son témoin sans Stop, et la question « enregistrer ? » posée à la fermeture pendant le recalcul |
| `stokesforge-session/tests/analyse.rs` (`au_chargement_aucun_calcul_n_est_relance_et_l_empreinte_dit_s_il_est_perime`, `la_confiance_au_chargement_ne_vaut_que_pour_le_chargement`) | A1 (#316) : à l'ouverture aucun calcul (automatique ou manuel) n'est relancé, dans les deux modes (`Full` comme `Targeted`, sinon l'oracle diverge) ; entrée modifiée hors de l'application (octets du fichier) → `Manual` et valeurs du fichier gardées, puis `recalculate` les rend justes ; fichier intact → à jour ; le drapeau `trust_loaded` est à usage unique (un calcul sans mémo après `AddCalculation` reste recalculé). Mutants : drapeau retiré, comparaison d'empreinte toujours vraie, toujours fausse |
| `stokesforge-session/tests/formula.rs` | le comportement des formules lui-même (antérieur à #193, inchangé) |
| `stokesforge-robustesse/tests/b14_balayage.rs` | la doctrine version ↔ contenu sur `History` seul, sur laquelle repose tout le mécanisme — y compris pour une colonne ajoutée avec un numéro déjà porté |
| `stokesforge-core` (`project.rs`, `une_colonne_inseree_fait_passer_le_compteur_au_dela_de_sa_version`) | l'insertion d'une colonne fait passer le compteur au-delà de sa version |

L'oracle ne voit pas une erreur **commune** aux deux modes (ordre de calcul,
évaluation, type de sortie) : elles partagent tout le code sauf la décision
« propre / à recalculer ». Ce sont les cas nommés et `formula.rs` qui gardent
le calcul lui-même.

### Mutants éprouvés (#193)

Chaque composante a été cassée tour à tour par une mutation temporaire du
code, retirée avant livraison ; le tableau dit ce qui a été cassé et quels
tests le voient.

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| texte de la formule retiré de l'empreinte | `changer_le_texte_de_la_formule_recalcule`, `b22` |
| longueur du résultat retirée (revu par #194 : pour une formule calculée, elle découle des versions des sources ; elle ne compte que pour les autres issues et les constantes) | `un_cycle_suit_la_longueur_de_la_feuille`, `une_constante_suit_les_colonnes_ordinaires`, `une_formule_illisible_suit_la_feuille_puis_se_corrige`, `b22` |
| longueur retirée pour une formule bloquée seulement | `un_cycle_suit_la_longueur_de_la_feuille`, `b22` |
| version propre ignorée (des deux côtés) | `une_colonne_formule_modifiee_directement_est_recalculee`, `une_formule_bloquee_ou_illisible_modifiee_directement_est_reecrite`, `b22` |
| version propre ignorée pour une formule bloquée, ou illisible, seulement | `une_formule_bloquee_ou_illisible_modifiee_directement_est_reecrite`, `b22` |
| version propre notée **avant** l'écriture, ou jamais | les cas « ciblé » (`une_commande_sans_rapport_ne_reecrit_aucune_formule`, `une_chaine_recalculee_est_stable_a_la_commande_suivante`, `un_cycle_stable_ne_se_reecrit_pas`…), `b22` (propriété 3) |
| versions des sources retirées | huit cas nommés (propagation, macro avortée, source promue…), `b22` |
| résolution des identifiants **par position** au lieu de l'identifiant (#321, le bug #320) | `deplacer_une_source_dans_la_feuille_la_formule_suit`, `une_analyse_qui_insere_ses_sorties_ne_change_pas_ce_que_lit_une_formule` |
| appartenance à la feuille absente de l'empreinte (la version d'une colonne lue est prise dans tout l'arbre) | `deplacer_une_source_vers_une_autre_feuille` — pas `b22` |
| un calcul automatique `Manual` lu d'un fichier est recalculé par le mode complet (arme `Manual` de `update_calculation`) | `un_automatique_perime_au_chargement_attend_un_geste_meme_en_mode_complet`, `b22` ; la comparaison de **contenu** (`Renumber`) : `un_automatique_perime_qui_lit_une_formule_reste_perime_en_mode_complet` |
| mémo non vidé par `Load` | `le_chargement_oublie_le_memo_du_projet_remplace` seul — le tirage ne fabrique pas de coïncidence entre projets |
| mémo non vidé par `NewProject` | `un_nouveau_projet_oublie_le_memo_de_l_ancien` seul (un collage comme première requête : sinon la purge d'une passe ordinaire suffit) |
| insertion d'une colonne sans faire passer le compteur au-delà de sa version (`Project::insert_node`) | `une_colonne_ajoutee_avec_sa_version_ne_fait_pas_resservir_ce_numero`, le test unitaire de `core`, `b14` — pas `b22`, dont le tirage ne retombe pas sur le numéro |
| tout recalculer (le « ciblé » n'en est plus un) | les cas « ciblé » et leurs témoins, `b22` (propriété 3) |
| présence dans le mémo au lieu de l'égalité d'empreinte | quinze cas nommés, `b22` |
| auto-référence | refusée à la saisie (`Tree::formula_from_letters` : `SelfReference`, test de `core`) ; un fichier fabriqué qui la porte finit dans le cycle, jamais dans `evaluate_formula` |
| purge du mémo retirée | **personne, par construction** : un identifiant n'est jamais réattribué dans un projet, et `forget` couvre le remplacement. Hygiène mémoire |

Une première version mettait dans l'empreinte d'une formule bloquée les
colonnes qu'elle désigne : aucun mutant ne la voyait manquer, et pour cause,
un bloc `NaN` ne les lit pas. Elle a été retirée — c'est l'usage de ce
tableau : une composante qu'aucun test ne voit est morte ou non couverte.
La relecture critique de #193 (agent indépendant) a ajouté trois lignes : les
composantes « par issue », que seul `b22` voyait, ont reçu leurs cas
nommés, et l'insertion d'une colonne numérotée, un vrai faux « propre »
latent, a été corrigée dans `core`.

### Mutants éprouvés (#194, longueur du résultat)

#194 a changé **le calcul**, pas la décision « propre / à recalculer » :
une formule a la longueur de la plus longue colonne qu'elle lit, à défaut
celle de la plus longue colonne ordinaire. Ces mutants touchent du code
commun aux deux modes : `b22` ne peut pas les voir, et ne les voit pas. Seuls
les cas nommés les tiennent — toute règle de calcul nouvelle doit donc avoir
les siens.

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| retour à la longueur de la feuille (toutes colonnes confondues) | `le_resultat_suit_la_longueur_des_colonnes_lues`, `allonger_une_colonne_non_lue_ne_change_pas_la_formule`, `deplacer_une_source_vers_une_autre_feuille`, `une_colonne_formule_modifiee_directement_est_recalculee`, `une_formule_illisible_suit_la_feuille_puis_se_corrige` — pas `b22` |
| colonnes lues ignorées (toujours la longueur des ordinaires) | `le_resultat_suit_la_longueur_des_colonnes_lues`, `allonger_une_colonne_non_lue_ne_change_pas_la_formule`, `deplacer_une_source_vers_une_autre_feuille`, `une_formule_illisible_suit_la_feuille_puis_se_corrige` — pas `b22` |
| colonnes-formule comptées dans la longueur de repli | `une_constante_suit_les_colonnes_ordinaires`, `une_formule_illisible_suit_la_feuille_puis_se_corrige` — pas `b22` |

### Fiabilité des ajustements (#203, #205)

`stokesforge-analysis/tests/indetermine.rs` : paramètre sans effet (avertit, les
autres gardent leur incertitude), X tous égaux (pente indéterminée), aucun
paramètre déterminé (`SingularSystem`), ajustement bien posé (aucun
avertissement), ligne à σ vide ou nul écartée et comptée.

### Ajustements sur un X décalé (#282, #293, #294)

`stokesforge-analysis/tests/decalage.rs` : droite libre `a + b*x` et gaussienne
sur un X décalé de 1e3 à 1e10, comparées au modèle `Linear` et au repère centré
(valeurs **et** incertitudes). Témoins par mutation : sans raffinement du pas
de différence, `une_gaussienne_decalee_de_un_million_retrouve_ses_parametres` et
`une_gaussienne_bruitee_decalee_de_un_million_a_les_incertitudes_du_repere_centre`
échouent ; sans le pas de vérification non amorti,
`un_pas_amorti_minuscule_n_est_pas_une_convergence` (`solver.rs`) et
`au_dela_le_resultat_est_celui_du_modele_lineaire` échouent ; avec l'ancien seuil
d'indétermination (1e-6), `le_cas_de_l_issue_donne_les_incertitudes_du_modele_lineaire`
échoue. Au-delà de X ≈ 3e7, la pente d'une droite libre est déclarée indéterminée
(incertitude omise, avertissement) : le bruit des différences finies (≈ 2e-10)
n'y sépare plus la direction faible.

### Bornes, prérequis et réglages d'analyse (#260)

`stokesforge-analysis/tests/limites.rs` : `Model::compile` refuse un degré hors
plafond (`u32::MAX`) sans rien allouer (le test s'exécute sous `ulimit -v` pour
voir l'échec d'un `compile` sans plafond plutôt qu'un blocage) ; `MAX_*` et
leurs bords dans `validate` (100 pics, pas 50) ; `validate` refuse un
ajustement sans paramètre libre (aucun, ou tous fixés) ; `Operation::readiness`
a un cas par variante (`NeedsPoints` avec son `have`, `NeedsZone`,
`BadExpression`, `NoParameter`, `Invalid`) ; `forget_positions` efface
exactement plages, positions, `initial`/`fixed`/`min`/`max`/`shared`, fenêtre
de reconvolution, et garde la méthode ; `FitSpec::auto_initial` filtre la plage
(bornes dans le désordre), ignore les paramètres fixés et les modèles illisibles.
`stokesforge-app` : `number_field.rs` (le champ « Max peaks » monte à 100 et
chaque grandeur d'analyse suit la constante du crate), `tests/analysis.rs` (point
ou zone uniques manquants, degré énorme = prérequis refusé, « Auto » lit la
plage), `analysis_recent.rs` (la réouverture oublie `fixed`/`shared`, un degré
énorme des préférences ne fabrique rien).

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| `compile` sans plafond | `compile_refuse_un_degre_hors_plafond_sans_allouer` |
| `readiness` oublie `NeedsZone` | `readiness_dit_ce_qui_manque_a_une_ligne_de_base` |
| `forget_positions` garde `shared` | `forget_positions_efface_les_plages_et_les_reglages_par_parametre`, `rouvrir_une_entree_oublie_aussi_les_reglages_fixes_et_partages` |
| `MAX_PEAKS` à 50 | `les_plafonds_sont_ceux_de_la_table_des_grandeurs`, `les_champs_d_analyse_suivent_les_plafonds_du_crate_d_analyse` |
| `auto_initial` sans filtre de plage | `auto_initial_suit_le_modele_sur_les_points_de_la_plage`, `auto_reads_only_the_fitted_range` |
| `validate` sans « zéro paramètre libre » | `validate_refuse_un_ajustement_sans_parametre_libre` |
| `validate` laisse passer plage, graine, proéminence, décalage ou fenêtre non finis (#278) | `validate_refuse_une_plage_non_finie`, `validate_refuse_une_graine_ou_une_proeminence_non_finie`, `validate_refuse_un_decalage_ou_une_fenetre_non_finis`, `validate_refuse_chaque_champ_non_fini_et_accepte_le_fini_qui_fait_l_aller_retour` (table champ par champ, NaN/±inf, aller-retour du fini) |
| `add_/update_calculation` sans `Operation::validate` (#278) | `une_operation_non_finie_est_refusee_a_l_ajout_et_a_la_mise_a_jour` (core), `toute_operation_non_finie_est_refusee_sans_toucher_l_arbre` (douze familles, add/update/replace, arbre et identifiants intacts), `la_session_refuse_une_operation_non_finie_sans_rien_changer` (session) |
| calcul illisible écarté sans rapport (#278) | `un_calcul_illisible_est_nomme_par_le_rapport_et_le_reste_se_charge` (persistance), `loading_a_file_with_an_unreadable_calculation_warns` (app-state), `plusieurs_calculs_ecartes_sont_tous_listes_dans_l_ordre_et_les_sains_se_chargent`, `un_fichier_sans_calcul_ecarte_rend_un_rapport_vide`, `un_identifiant_hors_borne_ecarte_le_calcul` (borne, tous les champs d'id), `le_chargement_relaie_les_calculs_ecartes_de_la_persistance` (session), `the_discarded_calculations_warning_counts_and_abbreviates` (singulier/pluriel, 5 ids puis « … », fr/en), `une_entree_a_nombre_non_fini_ne_se_relit_jamais_non_finie` (préférences) ; #309 : `une_forme_invalide_ecarte_et_nomme_le_calcul`, `une_trace_ecartee_est_comptee`, `une_trace_de_correction_illisible_est_comptee_par_le_rapport` (persistance), `le_chargement_relaie_les_traces_de_correction_ecartees` (session), `mark_loaded_avertit_des_calculs_ecartes_du_rapport` (app-state), `un_doublon_d_identifiant_garde_le_premier_et_compte_le_second` (persistance, F4), `le_rapport_compte_les_traces_et_les_calculs_dans_un_seul_message` (F1), `run_project_avertit_des_ecarts_puis_execute_le_script` (CLI, feature python, F2), confirmation à l'enregistrement (F3, app-state `shell.rs`) : `enregistrer_sur_la_source_avec_ecarts_demande_puis_ecrit`, `refuser_l_ecrasement_n_ecrit_rien_et_oublie_le_geste`, `quitter_enregistrer_accepter_enregistre_puis_quitte`, `enregistrer_sous_ailleurs_ne_demande_rien_mais_revenir_a_la_source_oui`, `un_fichier_propre_ou_un_nouveau_projet_n_ouvre_aucune_question`, `le_chargement_du_demarrage_pose_le_repere_de_la_source`, `le_demarrage_rend_le_rapport_des_calculs_ecartes` (main) ; `un_nan_edite_a_la_main_dans_le_fichier_ecarte_et_signale_le_calcul` (persistance, #291 : `validate` appelé au chargement) |

### Séries pas à jour sur le graphe (#198)

`stokesforge-core/tests/calculations.rs` (`une_serie_tracee_depuis_un_calcul_perime…` :
drapeau posé, levé au recalcul, éteint par `stale_warning`) ;
`stokesforge-persistence/tests/calculations.rs` et `schema.rs` (aller-retour du
réglage, fichier ancien = actif) ; `stokesforge-app/src/app/stale_export.rs`
(les quatre réponses de la confirmation, SVG grisé `#9a9a9a` ou non, graphe sans
avertissement exporté sans demander, ligne de l'Object Manager marquée).

### Ligne de base constante (#211)

`stokesforge-analysis/src/baseline.rs` et `tests/ops.rs` comparent le point
libre à la moyenne d’une zone, gardent une pente exactement nulle, vérifient
les bornes inclusives, les lignes écartées et la division par zéro. Les
réglages non finis et la zone sans donnée sont refusés.
`stokesforge-app-state/tests/analysis.rs` vérifie le point et la zone uniques,
leurs poignées et la réouverture du calcul ;
`stokesforge-persistence/tests/calculations.rs` traverse le `.stf` avec les
deux variantes. `stokesforge-script/tests/analysis.rs` exerce les mêmes
méthodes via `pyst`, leurs refus et l’annulation du script (feature `python`).

### Mutants éprouvés (#181, calculs d'analyse)

Même mécanisme, autre mémo (`CalcFingerprint`, `CalcMemo`). `b22` joue les pas
`AjouterCalcul` (ajustement ou correction, automatique ou manuel), `RetirerCalcul`
et `RecalculerPerimes` à côté des formules ; `stokesforge-session/tests/analyse.rs`
nomme les comportements.

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| versions des entrées retirées de l'empreinte d'un calcul automatique | `b22`, `un_calcul_automatique_suit_ses_entrees` |
| manuel : comparaison par le contenu (`seen`) supprimée | `b22` (le recalcul complet réécrit les formules, donc leurs versions) |
| garde des boucles calcul → formule → calcul retirée | `b22` (propriété 3 : une commande sans rapport réécrit des colonnes) |
| manuel : changement de définition non « cru » (`Trust`) | `modifier_un_calcul_manuel_perime_le_met_a_jour_sans_le_reperimer` seul |

### Mutants éprouvés (#313, avertissements et erreurs d'analyse)

Calcul pur, hors fingerprint : les avertissements viennent de `ops::run`, que le
recalcul ciblé et le complet appellent identiquement (`b22` et `b19` restent
verts). Les cas sont nommés dans `stokesforge-analysis` (`solver.rs`, `deconv.rs`,
`correct.rs` en `mod tests`, `tests/plateau.rs`, `tests/ops.rs`), `stokesforge-core/tests/calculations.rs`
(C4) et les trois tables de transport des `Warning` (persistance, `analysis_dialog.rs`,
`analysis_warning_text.rs`).

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| arrêt sans pas accepté toujours « convergé » (`stalled` jamais levé) | `un_solveur_sans_pas_accepte_et_a_pente_forte_est_marque_bloque`, `un_solveur_bloque_sur_une_pointe_avertit_non_converge` |
| test de non-fini retiré avant le pli `max` (pente `NaN` lue comme « convergé ») | `une_pente_non_finie_est_une_erreur_modele_non_evaluable` (solveur et `tests/plateau.rs`), `largeur_imposee_minuscule_ou_non_finie_n_est_pas_jugee_par_decompose` |
| dérivée non finie remise à 0 (case « sans effet ») | mêmes cas (l'erreur redevient `SingularSystem`) |
| seuil d'ajustement exact retiré (cosinus lu sur du bruit d'arrondi) | `un_ajustement_sain_ou_bruite_ne_previent_pas` (`tests/indetermine.rs`) |
| IRF coupée : seuil 1 % déplacé (0,5 % ou 2 %), bords ou points hors fenêtre ignorés | `une_irf_coupee_au_dessus_d_un_pour_cent_de_son_pic_est_signalee` (0,5 / 0,9 / 1,1 / 5 %), `un_point_d_irf_hors_de_la_fenetre_est_signale_selon_le_decalage`, `la_deconvolution_directe_dit_une_irf_coupee_et_garde_la_renormalisation` |
| trou de la référence : comptage retiré, point exactement sur un point gardé compté | `un_trou_de_la_reference_est_enjambe_et_compte`, `un_trou_au_milieu_de_la_reference_est_enjambe_et_dit`, `un_trou_en_fin_ou_sans_position_n_est_pas_un_enjambement` |
| division par zéro : compte de tout non-fini (débordement compris), ligne de base non comptée, `±inf` de `sat/sa` | `la_division_par_une_reference_nulle_est_comptee_pas_le_debordement`, `une_ligne_de_base_passant_par_zero_compte_la_division`, `un_denominateur_nul_donne_nan_et_pas_l_infini`, `des_amplitudes_de_somme_nulle_donnent_des_durees_moyennes_nan` |
| longueur de X remplacée par la plus longue des colonnes (C4) | `la_sortie_a_la_longueur_de_x_quand_x_est_donne`, `sans_x_la_sortie_a_la_longueur_de_y` |
| refus « source length » rétabli pour une sortie plus courte que Y | `le_remplacement_avec_y_plus_long_que_x_garde_les_cellules_au_dela_de_x` |

Mutant équivalent connu : `h >= lo && h <= hi` dans `bridges_hole` (un trou
à la même abscisse qu'un point gardé suppose un X dupliqué).

### Ajouter une fonctionnalité qui en dépend : les tests à écrire

**A. Une donnée de plus entre dans le calcul d'une formule** (fonction
`row()`, référence par nom de colonne, unité, colonne d'une autre feuille
#186, paramètre d'analyse…).

1. L'ajouter à l'empreinte (`Fingerprint`, ou une variante d'`Outcome`) :
   *toute donnée que lit le calcul y entre*. Attention à ce que la version
   d'une colonne **ne couvre pas** : elle ne change qu'avec le type et les
   valeurs, jamais avec le nom, l'unité, la formule, la position ni la
   feuille — une telle donnée se compare directement.
2. Un cas nommé dans `recalc_cible.rs` qui ne change **que** cette donnée et
   exige la nouvelle valeur.
3. Un pas dans le tirage de `b22` qui la fait changer. Sans lui, l'oracle ne
   peut pas voir l'oubli : il ne compare que ce que le tirage produit.
4. Mutation : retirer la composante doit faire échouer le cas nommé **et**
   `b22`. Une composante qu'aucun test ne voit manquer est soit morte (la
   retirer), soit non couverte (ajouter le pas) — l'écrire dans le tableau
   ci-dessus dans un cas comme dans l'autre.

**B. Une nouvelle voie d'écriture des données d'une colonne** (primitive de
`core`, commande, import, pont Python, recalcul d'un autre type).

1. Elle tire un numéro neuf de `Project::next_column_version` : sans lui, un
   même numéro peut désigner deux contenus, et le recalcul ciblé croit une
   formule à jour. Une voie qui **insère** une colonne déjà numérotée (fichier,
   presse-papiers, instantané gardé par un client) passe par
   `Project::insert_node`, qui fait avancer le compteur au-delà.
2. L'ajouter aux tirages de `b14` (doctrine sur `History`) et de `b22`
   (doctrine et oracle dans l'acteur).
3. Si elle peut viser une colonne-formule (comme `DeleteCells` et
   `SetColumnType` aujourd'hui), un cas nommé : la colonne est réécrite par sa
   formule.

Le remplacement par une correction (#212) est exercé par le pas
`RemplacerCorrection` dans `b14` et `b22` : colonne numérique ordinaire,
constante soustraite par `compute`, commande `ReplaceAnalysisData` avec les
versions de ses entrées. `b14` vérifie aussi la trace dans l’aller-retour `.stf` ;
`b22` exige au moins vingt passes pour ce geste, en conservant son oracle
ciblé/complet et son témoin « sans rapport ». Les gardes de concurrence
prennent les versions propres à chaque jumelle, dont les compteurs peuvent
diverger ; le résultat calculé soumis aux deux reste identique.

**C. Un nouveau calcul dérivé réutilise le mécanisme** (analyses #184).

1. Même règle d'empreinte (A.1), même ordre de passe : aucune écriture après
   qu'un lecteur a pris son empreinte, sans quoi une seconde passe réécrit et
   le « ciblé » ne l'est plus.
2. Garder un mode complet qui partage **tout** le code sauf la décision, et
   une batterie jumelle sur le modèle de `b22`, avec ses trois propriétés
   (oracle, version ↔ contenu, commande sans rapport sans réécriture) et son
   contrôle de couverture **par nature de pas**. Le tirage doit rester
   rejouable : ne jamais choisir une cible dans l'ordre de `Tree::iter`, et
   ne pas comparer des versions de part et d'autre d'un chargement (le
   compteur y repart du fichier, une coïncidence n'y prouve rien).
3. Les cas nommés de la famille « ciblé » avec leur **témoin** en mode
   complet : sans témoin, une version inchangée ne prouve rien.
4. Un résultat en erreur stable (cycle, ajustement qui ne converge pas) ne se
   réécrit pas à chaque passe : un cas nommé le vérifie.
5. Le mémo est vidé quand le projet est remplacé (`Load`, `NewProject`) : un
   cas nommé fabrique une coïncidence d'identifiants et de versions, la
   **vérifie**, puis exige le recalcul.
6. Si le calcul coûte (ajustement), mesurer qu'une commande sans rapport ne
   le relance pas — en comptant, comme `b17`/`b21`, pas en chronométrant.
7. Relancer les mutants de chaque composante, et compléter le tableau.

**D. Toucher à la passe elle-même** (ordre des étapes, mise en cache des
formules analysées, nouvelle étape) : relancer `b22` à plus de cas
(`runner(…)`) et les mutants ; une étape qui écrit après qu'une autre a pris
son empreinte se voit par la propriété « sans rapport » de `b22` et par
`une_chaine_recalculee_est_stable_a_la_commande_suivante`.

## Lancer

```sh
cargo test --workspace                        # tout, sauf #[ignore]
cargo test -p <crate> --test <fichier>        # un fichier de tests
cargo test -p stokesforge-robustesse -- --ignored  # reproductions et bancs coûteux
# (Typst est compilé et testé par `cargo test --workspace` ; `--all-features` n'active plus que Python : voir .codex/README.md)
scripts/python-test.sh                        # pont Python, scripts, robustesse, application et CLI
scripts/python-test.sh -- --ignored           # + les reproductions de la frontière Python
```

Les tests de console `script_panel` et de démarrage paresseux
`script_bridge_lazy` (dans `stokesforge-app-state/tests/`) exigent aussi la feature Python de `stokesforge-app-state`.
Avec l'environnement embarqué décrit dans [`.codex/README.md`](../.codex/README.md),
`scripts/python-test.sh` les inclut ; pour les isoler :

```sh
cargo test -p stokesforge-app-state --features python --test script_panel --test script_bridge_lazy
```

## CI (`.gitlab-ci.yml`)

Un seul étage, `check`, et cinq travaux :

| Travail | Bloquant | Ce qu'il fait |
|---|---|---|
| `fmt` | oui | `cargo fmt --all -- --check` |
| `clippy` | oui | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | oui | `cargo test --workspace` |
| `robustesse` | non | `cargo test -p stokesforge-robustesse -- --ignored` |
| `test-python` | oui, automatique | vendorise CPython + numpy, puis `scripts/python-test.sh` |

**Ce qu'un pipeline vert ne prouve pas**, et c'est écrit en tête du fichier CI :
`fmt`, `clippy` et `test` n'activent **jamais** la feature `python`. Tout ce qui
touche à PyO3 y est compilé **vide** — le pont Python, la partie la plus risquée
de l'architecture, n'est ni compilé, ni vérifié par clippy, ni testé par ces
trois travaux.

L'interpréteur n'est pas versionné. `test-python` le télécharge à froid et
le garde en cache selon `vendor/python/lock.env` ; ses artefacts Cargo ont
une clé distincte des travaux Rust purs. Il couvre automatiquement pont,
scripts, robustesse, application et CLI. Les reproductions ignorées et le
spike Matplotlib restent des essais explicites. Aucun Python système n'est
requis.

Typst, lui, n'est plus une feature (issue #29) : c'est une dépendance ferme des
trois crates de rendu, donc `fmt`, `clippy` et `test` le compilent et le
testent. Ce que la CI ne prouve pas pour Typst, c'est l'export : tant que
l'issue #34 n'a pas branché SVG/PDF/PNG dessus, il sort en texte brut.

## Ce qu'il faut savoir avant d'écrire un test

- Un test qui compare deux chemins a besoin d'un **témoin** qui doit différer.
- Un plafond qui ressemble à un bug n'en est probablement pas un : les
  plafonds et compteurs déjà arbitrés sont documentés à l'endroit de leur
  constante, et recensés dans
  `docs/archive/rustrewrite/questions-ouvertes.md` §11.
- La langue de l'interface est un état **global** du processus
  (`i18n::set_language`) : un test unitaire qui doit peindre dans une langue
  précise (`en`, `en-XA`) se relance en sous-processus avec `STOKESFORGE_LANG`
  posé, et vérifie que le fils a bien peint dans cette langue (modèle :
  `chaque_onglet_se_peint_en_anglais_et_en_pseudo_langue`,
  `src/app/plot_settings.rs`, #119).
- Un menu egui se teste ouvert pour de vrai (clic sur son titre), et se lit
  dans l'arbre d'accessibilité qu'egui publie (`ctx.enable_accesskit()`,
  `platform_output.accesskit_update`) : libellé, grisage, case cochée et
  rectangle de chaque entrée, sans supposer de position ni comparer de
  couleur (modèle : `entrees`, `src/app/menus.rs`, #122).
- Dans `stokesforge-app` et `stokesforge-app-state`, un test suit le code qu'il éprouve (#115) : une
  décision d'`AppState` dans `stokesforge-app-state/tests/shell.rs` (compile sans egui), un geste peint dans le module
  `tests` de sa surface (`src/app/<surface>.rs`) ; celui d'`app.rs` n'éprouve
  que l'orchestration (ordre des panneaux, application différée,
  confirmations, raccourcis globaux, restauration des fenêtres). Une aide de
  test que plusieurs surfaces recopieraient (contexte aux polices et au thème,
  image d'egui, touche, clic en trois images, texte peint, `colonnes_xy`) vit
  dans `src/app/test_support.rs` (#128), importée par nom ; une aide propre à
  une surface reste `pub(in crate::app)` dans le module de tests qui la porte
  (`graph_window::tests::graphe`, `project_manager::tests::click_at`). Une
  assertion de vide (« la table est vidée ») a besoin du témoin qui montre que
  la table était pleine avant (modèle :
  `ctrl_n_confirme_et_oublie_les_vues_du_projet_precedent`, #123).
- Un test ne dispatche pas une action qui écrit le `preferences.json` de la
  machine (`SetObjectManagerOpen`, `SetObjectManagerShowTexts`…) : il pose
  l'état en mémoire (`AppState::set_object_manager_open_for_test`), ou ne
  vérifie que l'action poussée. Pour exercer les actions de réglage (#139),
  `AppState::new_with_preferences` injecte un `Preferences::load_from` sur
  un fichier de dossier temporaire, sans modifier l'environnement global.
- Règle (#250) : un test n'écrit jamais dans les vraies préférences. Les tests
  unitaires de `stokesforge-app-state` n'en ont aucune (`default_path` rend `None`
  sous `cfg(test)`) ; ceux de `stokesforge-app`, qui bâtissent un `AppState` par
  `new`, n'en ont aucune non plus grâce à la feature `test-support`, que la
  dev-dependency de l'app active (`cfg(test)` ne passe pas la frontière du
  crate, #263 ; retirer la feature fait échouer
  `stokesforge-app/tests/isolation_preferences.rs`) ; les tests d'intégration
  (`tests/`) construisent leur
  état par `AppState::new_with_preferences(…, Preferences::load_from(None))`,
  jamais `AppState::new` (garde dans `stokesforge-app/tests/gardes.rs`, qui lit
  les deux dossiers `tests/`). L'écriture du fichier
  est atomique (temporaire + renommage).
- Un test ne lit pas non plus les modèles d'import ni les fichiers récents de
  la machine (`import_recipes.json`, `import_recent.json`) : un modèle
  enregistré « Jasco » passait devant le modèle intégré et faisait échouer le
  choix automatique (#195). Il redirige `XDG_CONFIG_HOME` vers un dossier
  temporaire — sous verrou si le test restaure la valeur
  (`stokesforge-import/src/store.rs`), ou une seule fois par processus avant
  tout banc (`configuration_hermetique`, `stokesforge-script/tests/script.rs`).
- `stokesforge-clipboard` (#149) : la décision des réponses X11 (`x11::answer`) et les noms de formats Windows/macOS se testent sans serveur ; l'offre X11 se teste contre un **vrai serveur** (`tests/x11.rs`, `#[ignore]`, `DISPLAY` requis, XWayland suffit : `cargo test -p stokesforge-clipboard --test x11 -- --ignored`). Wayland, Windows et macOS n'ont pas de test automatique : Wayland exige un compositeur et une entrée réelle (serial), les deux autres ne sont que compilés (`cargo check --target …`). Un test X11 d'un destinataire ne teste pas `response_type` d'un `SelectionNotify` : un événement venu de `SendEvent` porte le bit 0x80.
- Les tests et leurs commentaires sont en français, comme le reste.

**À lire** : `stokesforge-robustesse/src/lib.rs` (l'index des batteries), puis la
tête du fichier `b*` visé.

## Redécoupe des fonctions d'interface (#124)

Les bancs des surfaces gardent les mêmes événements et rectangles : menus,
grille, pages des propriétés, assistant d'import, Object Manager et bande
d'onglets. Les cas ajoutés vérifient notamment la plage visée par le menu de
type, le maintien des paramètres de format, le calque et la série réellement
édités, le repli indépendant des groupes de l'arbre et l'ordre des callbacks.
Le callback détaché de la note collecte son édition et sa fermeture avant leur
application par la racine.

Dans `app.rs`, les phases extraites sont éprouvées aussi directement : une
confirmation vide les actions récoltées, arrête le dispatch de celles qui
suivent, et une commande de tracé sans effet n'ouvre aucun onglet. Dans
`window_stack.rs`, masquer un graphe garde sa place mais saute contenu et
publication de géométrie ; le témoin visible les retrouve.

Les nouveaux cas sont éprouvés par des mutations temporaires retirées avant
livraison. Les comparaisons de formes egui avant/après (texte, positions,
couleurs, rognage) complètent les tests de gestes ; elles ne remplacent pas une
validation interactive des fenêtres OS détachées ou des sélecteurs natifs.

## Groupes de séries (#196, #197, #209)

Les gestes passent par les événements egui, jamais par un appel direct de la
méthode de `PanelSpec` qu'ils déclenchent : un test qui appelle
`set_series_group` lui-même ne voit pas l'éditeur perdre sa série.

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| `membership_editor` : la sélection ne suit plus la série déplacée (`*index = moved` retiré) | `la_liste_deroulante_fait_rejoindre_un_groupe_et_l_editeur_suit_la_serie` (`app/curve_settings.rs`) |
| graphe : la branche « groupe sélectionné » sautée, Suppr vise la première série | `les_raccourcis_d_un_groupe_selectionne_visent_le_groupe` (`app/graph_window.rs`) |
| Ctrl+↑/↓ sans la borne `can_move_group` | idem |
| F2 n'ouvre plus le renommage du groupe | idem |
| `.stf` : noms de groupe vides gardés au chargement | `un_nom_de_groupe_vide_est_ecarte_au_chargement` (`stokesforge-persistence/src/convert.rs`) |
| `.stf` : `#[serde(default)]` retiré de `group_names` (ancien fichier illisible) | `un_calque_sans_noms_de_groupe_se_relit_avec_ses_groupes` |

## Copier/coller de cellules (#259)

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| `tsv::needs_quotes` toujours faux (simple `join("\t")`) | `stokesforge-core/src/tsv.rs` (aller-retour tab, saut de ligne, guillemet) ; `a_multiline_text_cell_pastes_into_another_session_without_shifting_anything` (`stokesforge-app-state/tests/shell.rs`) |
| `decode` sans gestion des guillemets | idem + `a_single_multiline_text_cell_pastes_as_one_cell` |
| `decode` sans `\r\n` | `un_tableur_windows_termine_ses_lignes_par_crlf`, `external_clipboard_from_text_lit_le_format_des_tableurs` |
| `cell_copy_text_in` qui retire `MISSING_TEXT_DISPLAY` après coup | `a_typed_non_breaking_space_survives_copy_and_paste` |
| case absente lue comme vide (`unwrap_or("")`, plus de `value_block_keeping_absent_cells`) | `a_pasted_row_shorter_than_the_others_keeps_the_existing_cell`, `a_short_row_pasted_over_a_mixed_column_keeps_its_cell_in_one_undo` (#311) |
| case présente et vide traitée comme absente | `a_present_but_empty_pasted_cell_is_still_a_missing_value`, `pure_data_copy_paste_keeps_working_without_header_markers` |
| bloc non coupé après la dernière case présente (colonne allongée) | `a_short_last_row_does_not_lengthen_the_shorter_column` |
| statut `status-paste-columns-dropped` retiré / posé sans débordement | `a_paste_wider_than_the_sheet_reports_the_dropped_columns`, `a_paste_that_fits_the_sheet_width_reports_nothing` |
| `looks_like_thousands_separator` élargi à `\d+,\d+` (ou à `1,5`) | `la_forme_milliers_est_reconnue_exactement` (`stokesforge-core/src/number_format.rs`) |
| avis `status-cell-decimal-comma` retiré, ou posé au collage | `typing_a_thousands_looking_number_in_a_numeric_cell_notices_the_decimal_comma`, `the_decimal_comma_notice_also_applies_to_a_mixed_cell`, `pasting_a_thousands_looking_number_does_not_notice` (#311) |
| rangées d'en-tête jetées non comptées (`status-paste-fields-dropped`) / message combiné cassé | `a_header_field_paste_anchored_low_counts_the_rows_beyond_formula`, `a_header_field_paste_anchored_on_long_name_reports_nothing`, `columns_and_header_fields_dropped_together_give_one_combined_notice` (#311) |
| « 2026 » en Date/Time lu autrement que 2026 secondes | `from_texts_datetime_lit_un_nombre_nu_comme_des_secondes` (`stokesforge-core/src/column.rs`) |

Le collage rejoue la structure interne quand le texte est celui qu'on a poussé :
pour éprouver le chemin « texte externe », coller dans une **autre** session.

## Saisie et collage de cellules : une seule règle (#258)

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| le bloc élargi repasse par `MixedCell::from_text` (date perdue, `inf` lu en nombre) | `from_texts_datetime_garde_la_date_comme_valeur_dans_un_bloc_elargi`, `from_texts_numeric_garde_infini_et_debordement_en_texte`, `from_texts_en_colonne_vaut_case_par_case_pour_les_trois_types` (`stokesforge-core/src/column.rs`) ; `a_pasted_date_keeps_its_value_when_the_column_widens_to_mixed` (`stokesforge-app-state/tests/shell.rs`) |
| la branche `Mixed` lit `inf` / `nan` en nombre (sans `from_texts`) | `from_texts_mixed_ne_lit_ni_infini_ni_nan` ; `typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text` |
| `parse_f64_text` au lieu de `parse_input` | `from_texts_mixed_ne_lit_ni_infini_ni_nan`, `from_texts_en_colonne_vaut_case_par_case_pour_les_trois_types`, `typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text` |
| garde `kind == DateTime` retirée (`Numeric` lit les dates) | `from_texts_numeric_ne_lit_pas_les_dates` |
| case vide lue `Text("")` au lieu de manquante | `from_texts_case_vide_est_une_valeur_manquante_pas_un_texte`, `pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo` |
| repromotion après collage retirée (aux trois points de collage) | `pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo`, `a_multi_column_paste_with_a_repromotion_is_one_undo`, `single_value_and_full_column_pastes_also_repromote_a_mixed_column` |
| repromotion appelée après `end_grouped_macro` (deux entrées d'historique) | `a_multi_column_paste_with_a_repromotion_is_one_undo`, `undo_state_is_fresh_right_after_a_grouped_paste` |
| macro propre ouverte même sans repromotion | `a_paste_that_leaves_a_mixed_column_mixed_is_a_plain_entry` |

`SetCellsPromoting` (stokesforge-commands) est inchangé : seul le bloc qu'on lui
passe vient désormais de `from_texts`.

## Composition des graphes (#257)

| Mutant (ce qui est cassé) | Vu par |
|---|---|
| appariement au X le plus proche à **droite** (ou le premier à gauche) | `chaque_y_prend_le_x_le_plus_proche_a_sa_gauche_meme_non_selectionne`, `un_y2_s_apparie_a_x2_et_va_au_second_calque`, `x_for_suit_la_meme_regle_que_series_from_columns` (`stokesforge-core/src/compose.rs`) |
| `x_for` ne distingue plus Y2 → X2 | `x_for_suit_la_meme_regle_que_series_from_columns` |
| palette sans décalage de rang (`start_index`, séries « ligne », rang du panneau dans `pyst`) | `la_palette_suit_le_rang_avec_le_decalage_de_chaque_calque`, `les_series_ligne_prennent_la_palette_du_rang_et_le_nom_donne` ; `plusieurs_colonnes_forment_un_groupe_et_une_seule_non` (`stokesforge-script/tests/graphs.rs`) |
| second calque confondu avec le calque des lignes, ou calque de base cherché parmi les superposés | `le_second_calque_est_celui_des_deux_axes_pas_celui_des_lignes`, `un_calque_superpose_recouvre_le_dernier_calque_ordinaire_de_sa_case` |
| `spec_has_x` sans les séries « ligne » ; x' non fini accepté | `le_graphe_a_un_x_par_une_colonne_x_ou_une_serie_ligne`, `les_colonnes_x_prime_sont_celles_dont_la_valeur_est_finie` |
| constructeur `OverlaySpec::new` inversé, `TOP_X` qui affiche aussi Y | `les_calques_nommes_disent_quels_axes_ils_affichent` (`stokesforge-plot/src/panel.rs`) |
| `PanelSpec::remove_series` qui ne retire rien ou accepte un rang hors borne | `remove_series_retire_la_serie_designee_et_pas_une_autre` ; `object_manager_supprime_une_courbe_sans_toucher_aux_donnees` (`stokesforge-app-state/tests/shell.rs`) |
| `pyst.plot(y)` sans `x=` qui garde l'indice de ligne malgré une colonne X à gauche | `plot_sans_x_prend_la_colonne_x_a_gauche_par_role` (`stokesforge-script/tests/graphs.rs`, feature `python`) |
| `aux_for` qui perd le X apparié | `reference_candidates_exclude_the_processed_series_and_pair_their_x` (`stokesforge-app-state/tests/analysis.rs`) |

Les tests de l'app (`plot_columns_*`, `add_layer_*`, `plot_rows_*`) gardent le
câblage : `compose` rend les séries, l'app les envoie en une commande.

### R² faible (#288)

`app-state/src/r_squared.rs` tient les paliers (0,95 / 0,8 / 0,6 pile,
négatif, `NaN`). `app-state/tests/analysis_state.rs` (fin du fichier) tient la
boîte : ouverte à l'application (valeurs gardées ou non) et au recalcul
automatique, muette pour un bon R², préférence coupée, réglage du calcul coupé,
aperçu, ouverture d'un projet, alerte déjà là, plusieurs calculs. Mutants
éprouvés : garde d'alerte, préférence, réglage du calcul (deux endroits),
empreinte, bornes strictes.

Boîte groupée et pastille (#290) : `app-state/tests/analysis_state.rs` (fin du
fichier) tient l'état « R² faibles du projet » — compté malgré le réglage du
calcul coupé, la préférence coupée, une alerte déjà ouverte ; resté après la
boîte ; absent pour un calcul « Show » et pour un bon R² ; revenu à 0 après un
recalcul qui remonte R² ou la suppression du calcul (et rétabli par l'annulation) ;
compté par série ; deux types dans l'ordre fixe ; liste ouverte/fermée sans
toucher `alert`. `app-state/tests/low_r2_text.rs` tient le texte (fusion et
ordre des groupes, singulier/pluriel fr+en, chaque type nommé). Mutants
éprouvés : réglage du calcul qui filtre la pastille, compte par calcul au lieu
de par série, absence de rafraîchissement à l'instantané, ordre des types,
fermeture automatique de la liste. La peinture de la pastille (position à droite,
teinte, clic) n'a pas de test : seule l'action qu'elle pousse est gardée par
l'état. La peinture egui (gras, teinte, bulle) n'a pas de
test d'image : seul `theme::r_squared_color` est testé.

### Remplacement définitif d'une correction (#212)

`commands/tests/calculations.rs` tient l'atomicité données/trace, undo/redo,
longueur et positions, conflit de versions, refus des colonnes en lecture seule
et corrections contradictoires sur un Y partagé. `persistence/tests/calculations.rs`
garde les traces successives et l'allocateur après rechargement.
`session/tests/analyse.rs` vérifie que les calculs dépendants suivent le
remplacement et son annulation. `stokesforge-app-state/tests/analysis_state.rs` exerce la
confirmation des sources partagées ; `app::form::tests` refuse les options
grisées dans les radios et les listes. `script/tests/analysis.rs` vérifie le
retour de la poignée originale et la macro Script avec `replace_original=True`.

## Colonne utilisée par un calcul (#306)

`stokesforge-app-state/tests/column_use.rs`. Chaque règle a son témoin, vérifié par mutation.

| Règle | Témoin |
|---|---|
| une formule qui lit une colonne supprimée est listée ; celle dont seule une lettre glisse ne l'est pas (#321 : lecture par identifiant) | `supprimer_une_colonne_lue_par_une_formule_ouvre_la_liste_puis_close_ne_fait_rien`, `les_lettres_qui_glissent_ne_comptent_pas_seule_la_colonne_lue_compte` (la liste de toute formule dès qu'une colonne part : rouge) |
| une colonne sans lecteur n'ouvre rien | `une_colonne_sans_lecteur_n_ouvre_rien` |
| un calcul d'analyse est nommé par sa famille et sa première sortie | `un_calcul_d_analyse_est_liste_par_sa_famille_et_sa_premiere_sortie` |
| un calcul dont une sortie disparaît avec le geste n'est pas listé | `supprimer_un_classeur_emporte_ses_calculs_sans_rien_signaler` (sans l'exclusion : rouge) |
| plusieurs colonnes, une liste, un seul Undo | `undo_rend_la_colonne_et_la_formule_en_un_pas_et_plusieurs_colonnes_font_une_liste` |
| Close ne rend rien | `supprimer_une_colonne_lue_par_une_formule_ouvre_la_liste_puis_close_ne_fait_rien` |
| Ctrl+Maj+C pose la question seulement si une colonne a une formule ; Valeurs vide la ligne Formule | `ctrl_maj_c_sur_une_colonne_formule_pose_la_question`, `ctrl_maj_c_sans_formule_copie_directement` |

Limites : le déplacement d'une colonne (#319) n'ouvre aucune fenêtre : le témoin est `column_move.rs` (« Déplacer des colonnes », plus bas) ; ses effets sur les formules sont aussi tenus au niveau de la session (`recalc_cible.rs`) ; la référence cachée d'une correction (`aux`) est exclue par construction, sans témoin dédié.

## Formules par identifiant (#321, #320)

Une formule retient ses colonnes par identifiant ; les lettres ne sont qu'aux frontières. Chaque règle a son témoin, vérifié par mutation.

| Règle | Témoin |
|---|---|
| lettres vers identifiants : refus d'une lettre inconnue, de la colonne elle-même, de `#REF` ; l'exposant n'est pas une colonne | `stokesforge-core`, `formula::tests::from_letters_*` |
| `#REF` à l'affichage pour une colonne hors feuille | `a_column_outside_the_sheet_is_written_ref` |
| dupliquer une feuille fait lire à la copie ses propres colonnes ; une référence rompue reste `#REF` ; un identifiant lu n'est jamais réattribué au chargement | `project::tests` (`dupliquer_une_feuille_…`, `une_reference_rompue_…`, `from_tree_ne_reattribue_pas_…`) |
| une analyse qui insère ses sorties au milieu ne change pas ce que lit une formule (#320) ; déplacer, supprimer puis annuler | `analyse.rs` (`une_analyse_qui_insere_…`), `recalc_cible.rs` (`deplacer_une_source_*`, `supprimer_une_source_donne_ref_…`) |
| `.stf` : `formula_parts` aller-retour, id hors feuille chargé en `#REF`, id hors borne refusé, ancienne clé `formula` ignorée | `roundtrip.rs` (`a_formula_*`, `a_legacy_letters_formula_…`) |
| saisie : lettre inconnue, auto-lecture, `#REF` modifié refusés dans `app-state` (`header_edit_refusal`), jamais une erreur de syntaxe ; `#REF` non modifié n'écrit rien ; `formula_issue` pour la case | `shell.rs` (`a_formula_reading_its_own_column_is_refused_while_typing`, `a_ref_formula_is_refused_only_when_its_text_is_changed`, `a_formula_naming_a_missing_column_…`) |
| collage : la formule refusée seule n'est pas collée, la barre d'état dit le refus après les écritures ; Ctrl+Maj+C copie des lettres relues dans la feuille d'arrivée | `a_refused_formula_in_a_paste_…`, `copied_formulas_travel_as_letters_…` |
| `pyst` : lettres en lecture et en écriture, refus d'une lettre inconnue et de `#REF`, un autre champ ne touche pas la formule | `script/tests/navigation.rs` (`les_formules_se_lisent_…`), `mutations.rs` |

Limite : le cadre rouge, la bulle et le champ qui reste ouvert (grille) ne sont pas testables sans fenêtre : la décision (`header_edit_refusal`, `inplace_edit::resolve`) l'est, la peinture est relue à l'écran.

## Largeur des colonnes (#325)

Table `NodeId -> f32` de l'acteur, hors modèle et hors historique, enregistrée dans le `.stf`. Chaque règle a son témoin, vérifié par mutation.

| Règle | Témoin |
|---|---|
| poser une largeur n'avance que `geometry_revision` ; la même largeur, NaN, ≤ 0, ∞ : ignorées | `session/tests/largeurs.rs` `poser_une_largeur_n_avance_que_geometry_revision` (passer par `revision` : rouge) |
| la largeur d'une colonne supprimée revient avec Ctrl+Z | `la_largeur_revient_avec_la_colonne_apres_annulation` (purger les largeurs à la mutation : rouge) |
| aller-retour `.stf` ; `NewProject` vide | `les_largeurs_font_l_aller_retour_du_fichier` (ne pas écrire le champ : rouge) |
| l'écriture n'émet pas la clé d'une colonne disparue | `la_sauvegarde_n_ecrit_pas_la_largeur_d_une_colonne_supprimee`, `persistence` `save_drops_dead_keys_and_invalid_widths` (retirer le filtre : rouge) |
| fichier sans le champ → table vide ; clé orpheline, NaN, ∞, 0, négatif écartés un à un ; identifiant > `MAX_NODE_ID` refusé | `persistence` `a_file_without_the_widths_block_*`, `hostile_column_widths_*`, `a_column_width_key_above_the_id_ceiling_*`, `robustesse/b3` `b3_largeurs_de_colonne_hostiles` (lecture sans filtre, sans `check_node_id` : rouge) |
| l'état pose la largeur par l'acteur sans historique, y compris une vue détachée | `app-state/tests/column_width.rs` |
| le `.stf` donne sa largeur à l'instantané de démarrage | `main.rs` `le_demarrage_restaure_les_largeurs_de_colonne_du_fichier` |
| la poignée tirée ne pousse la largeur qu'au relâchement | `grid::tests::un_redimensionnement_a_la_main_envoie_la_largeur_a_la_fin_seulement` |
| ouvrir et peindre n'envoie rien, ne marque pas le projet modifié | `ouvrir_la_feuille_n_envoie_aucune_largeur` (pousser la largeur rendue à chaque image : rouge) |
| Add Column : les largeurs gardées, la neuve à 54 ; Delete puis Ctrl+Z | `la_largeur_de_chaque_colonne_survit_a_l_ajout_d_une_colonne`, `supprimer_puis_annuler_rend_la_colonne_et_ses_voisines_a_leur_largeur` (largeur par position : rouge ; sans `reset()` quand la liste change à nombre égal : rouge) |

La largeur se lit dans le banc sur l'aplat de la lettre de colonne (`largeur_peinte`).

## Déplacer des colonnes (#319)

Sous-menu `Move ▸` (First · Left · Right · Last). Chaque règle a son témoin, vérifié par mutation.

| Règle | Témoin |
|---|---|
| Left/Right gardent l'écart (`A [B] C [D] E` → `B A D C E` / `A C B E D`), First/Last regroupent dans l'ordre | `column_move::tests::left_keeps_the_gap_*`, `first_and_last_gather_*` (resserrer le bloc : rouge) |
| tout ou rien : `[A] C` et `[A] [C]` Left → `None` ; geste sans effet → `None` | `a_column_that_cannot_move_greys_*`, `a_gesture_that_changes_nothing_*` (ne bouger que C : rouge) |
| la suite de `Move` rejouée (index après retrait) donne l'ordre cible, toutes sélections de six colonnes, quatre sens ; seules les colonnes sélectionnées bougent | `the_steps_reach_the_target_order_for_every_selection` (index +1 : rouge) |
| un enfant `Unknown` garde sa place, index dans la liste complète | `an_unknown_child_keeps_its_place_*` (index = rang visible : rouge) |
| un seul Undo pour plusieurs colonnes, libellé « Move Columns » ; une colonne : `Move “B”` | `app-state/tests/column_move.rs` `deux_colonnes_deplacees_*` (sans macro : rouge), `une_colonne_seule_*` |
| aucune fenêtre « Colonne utilisée », valeurs de formule inchangées | `deplacer_une_colonne_lue_par_une_formule_*` (appeler `open_column_use` : rouge) |
| la barre d'état dit le libellé du **groupe** (« Move Columns ») à la clôture, comme l'annulation, pour tout groupe fermé par `end_grouped_macro` (la dernière étape ne reste pas affichée) ; une colonne seule garde `Move “B”` | `la_barre_d_etat_dit_le_libelle_du_groupe_*` (retirer le remplacement dans `end_grouped_macro` : rouge) |
| la sélection couvre les mêmes colonnes après le geste | `la_selection_reste_sur_les_colonnes_deplacees` (vider la sélection : rouge) |
| `Move ▸` entre le presse-papiers et `Type ▸`, quatre entrées ; grisage tout ou rien (A seule, C seule, A et C, lignes entières) ; l'entrée active pousse `MoveColumns` | `grid::tests::move_est_place_*`, `les_entrees_de_move_sont_grisees_*`, `une_entree_de_move_pousse_*` (grisage supprimé, entrée retirée : rouge) |
| même sous-menu et même grisage dans Workbook › Columns | `menus::tests::le_menu_colonnes_porte_move_*` (retirer `Move ▸` de la peinture partagée : rouge) |
| la largeur suit la colonne déplacée (#325) | `grid::tests::la_largeur_suit_la_colonne_deplacee` (stockage par position : rouge) |

Limite : l'aspect du sous-menu et le texte des bulles se relisent à l'écran.

## Polynôme contraint sur un X décalé (#296)

`fit::tests::polynome_contraint_sur_x_tres_decale_est_signale` : signalé (`shifted_x`) pour un polynôme fixé ou à valeur initiale sur X ≈ 1e4 ; témoins non signalés : X centré, décalage modéré (1e2), polynôme sans contrainte (QR centré). Transport `.stf` : `shifted_x` dans `persistence/src/calculations.rs` (tests d'aller-retour et de noms figés).

## Projet de démonstration (`demo/`)

`crates/stokesforge-app/src/demo/tests.rs` (module du **binaire** : `cargo test -p stokesforge-app --bin stokesforge-app`). Chaque test a son témoin, vérifié par mutation (graine changée, titre donné à un graphe, colonne supprimée sous un graphe, amplitude du jeu gaussien changée, `recalculate` retiré de `session_for_project`) : noms de fenêtre uniques dans tout le projet, aucun titre de graphe (#103), chaque graphe résolu sans référence brisée et dessiné, axes logarithmiques sur données strictement positives (invariant 10), chaque jeu d'analyse retrouve ses paramètres connus par `core::compute` (`Attendu`, tolérance), les sept familles d'`OperationKind::ALL` couvertes, bruit à graine fixe épinglé, colonnes à formule calculées dès le démarrage, aller-retour `.stf` par la session (rapport propre, mêmes specs), tous les `PlotType` (sans `Unknown`).
