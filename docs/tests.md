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
| `*/tests/no_ui_dependency.rs` | liste blanche de dépendances : toute addition au `Cargo.toml` doit y être ajoutée explicitement (invariant 2). Lecture commune `stokesforge-arch-guard` : dépendances normales, de build, `target.*` et renommées (`package`) ; `dev-dependencies` exclues ; témoins négatifs dans `stokesforge-arch-guard/src/lib.rs` |
| `stokesforge-render-svg/tests/frontiere.rs` | l'export ne voit ni `egui`, ni `eframe`, ni `winit` — donc se teste sans serveur graphique |
| `stokesforge-render/tests/mesure.rs` | la source unique de mesure de texte (invariant 4) |
| `stokesforge-render-egui/tests/accord.rs` | écran et export placent les mêmes primitives aux mêmes pixels |
| `stokesforge-render-egui/tests/feathering.rs` | le feathering n'entre pas dans la clé de cache (invariant 6) |
| `stokesforge-app/tests/script_bridge_lazy.rs` | l'interpréteur Python ne démarre pas à l'ouverture de la fenêtre |
| `stokesforge-app/tests/libelles.rs` | aucun texte d'interface en dur (`aucun_libelle_en_dur`, garde absolue, sans dérogation, #48), y compris les titres `set_title` et noms visibles `add_filter` des boîtes de fichier (#127 ; extensions et sigles de format techniques préservés) et les fonctions libres `section_title`, `bold`, `list_row`, `list_row_text` (#169) ; la zone #43 (`ZONE_43`) sans aucun littéral, ses fonctions cherchées par leur nom dans toutes les sources — une entrée introuvable échoue ; les dépendances de traduction restent dans `stokesforge-app` ; les témoins qui prouvent que la garde voit un littéral, y compris dans un sous-dossier de `src/` |
| `stokesforge-app/tests/gardes.rs` | aucune `egui::Window` brute (`aucune_fenetre_n_est_une_egui_window_brute`) ; la fenêtre des propriétés passe par `modal_dialog` → `modal_dialog_with` → `modal_frame` → `show_window_in_layer` (comme `modal_wizard`, l'assistant d'import, et `modal_read_only`, #228, puis `modal_confirm`, `modal_save_changes` et `modal_alert`, #229) ; aucun import global (`use …::*`) hors du code de test (`aucun_import_global_hors_des_tests`, décision 2 de #115, absolue depuis #117) ; aucun `ui.separator()` dans un menu hors de la brique de sections (`aucun_separateur_brut_dans_un_menu`, #155, absolue) ; bouton sélectionnable d'egui seulement dans un menu, une liste déroulante ou le sélecteur de couleur (`aucun_bouton_selectionnable_hors_d_un_menu`, #169) ; aucune couleur `Color32::from_…` ni teinte nommée hors de `theme.rs` et `color_picker.rs` (`aucune_couleur_en_dur_hors_du_theme`, #169) ; aucun `ui.separator()` juste avant un titre de section (`aucun_separateur_avant_un_titre_de_section`, #169) ; ni `DragValue` ni `Slider` (`aucun_champ_numerique_hors_de_la_brique`, #220, absolue) ; aucune bulle nue (`on_hover_text`/`on_hover_ui`) sur une case, un radio, un champ texte ou nombre, un libellé de champ hors menu (`aucune_bulle_nue_sur_un_champ_de_formulaire`, #225, absolue — le rappel passe par `help_beside`/`help_text`) ; tout `add_enabled` porte `.on_disabled_hover_text` (`aucun_element_grise_sans_sa_raison`, #226, absolue) ; aucun `add_space(<nombre>)` hors de `theme.rs` (`aucun_add_space_litteral_hors_du_theme`, #227, absolue — `theme::SPACE_ITEM` / `SPACE_SECTION`) ; les témoins du lecteur de sources |
| `stokesforge-app/src/theme.rs` (module `tests`) | aucun `ScrollStyle` hors du thème (`aucun_scroll_style_hors_du_theme`) ; aucun `strong()` d'egui, tout gras par `theme::bold` (`aucun_strong_d_egui_hors_du_theme`, #169) ; `bold` élargit vraiment le texte avec les polices embarquées |
| `stokesforge-app/tests/support/sources.rs` | pas un test : le **lecteur unique** de toutes les gardes qui lisent le code (`libelles.rs`, `history_text.rs`, `gardes.rs`, et par `src/lib.rs` celles d'`icons`, `theme`, `i18n`). Lit `src/` récursivement ; vue de production sans commentaires ni items `#[cfg(test)]` (chacun, pas seulement le premier ; un `#[cfg(test)] mod x;` rend son fichier entier de test) ; cherche une fonction par son nom, où qu'elle vive (#116). Une garde nouvelle qui lit le code passe par lui, jamais par `include_str!` ni un fichier nommé |
| `stokesforge-app/src/field_state.rs` (tests) | état de champ (#219) : décision pure du cycle (`decide`), cadre rouge/orange et bulle lus dans les formes peintes, ancienne valeur rendue à la sortie, orange éteint après 2 s, témoin « saisie valide → aucun cadre » ; mutants éprouvés (couleur, champ gardé ouvert, échéance, sortie sans changement, bulle) |
| `app/busy.rs`, `app/empty_state.rs`, `tests/shell.rs` (#232) | indicateur absent avant 0,5 s puis mot, Stop seulement après 3 s et seulement si le travail s'interrompt ; phrase d'état vide grisée et centrée ; Stop de l'import : rien créé, aucune entrée d'historique, un import ultérieur va au bout |
| `stokesforge-app/tests/gardes.rs`, `app/shortcuts.rs`, `app/keyboard_shortcuts_window.rs`, `project_manager.rs` (tests) | clavier et souris (#231) : gardes `aucun_consume_key_hors_du_registre` et `aucun_curseur_hors_du_module_des_curseurs` (témoins), toute constante consommée figure dans `HELP`, fenêtre des raccourcis, menu Help, double-clic du Project Manager (active, ne ferme jamais, deux Ctrl-clics n'ouvrent pas) |
| `stokesforge-app/src/number_field.rs` (tests) | brique « nombre » (#220) : lecture (refus, bornes, entiers), pas et grands pas, liste de valeurs au clavier (Alt+↓, Entrée, Échap), clic = tout sélectionné, molette au survol sans focus qui suit la liste (`step_preset` : hors liste, extrémités), champ sans liste = fenêtre qui défile, flèche pleine dans le cadre (`list_frame` : fonds du thème distincts), cohérence de la table des grandeurs ; dans `plot_settings` : saisie validée à Entrée, crans de molette = une entrée d'historique, Échap rend la valeur puis annule le dialogue |
| `stokesforge-app/src/app/dialogs.rs` (tests) | brique des dialogues (#228) : ordre des boutons lu dans les formes (dialogue, assistant, lecture seule), décision pure du pied (`footer_message` : l'erreur passe devant la raison) et sa peinture (la raison revient quand l'erreur disparaît, témoin bouton actif), focus initial sur le premier champ texte et aucun focus sans champ, Échap qui sort d'un champ puis annule, géométrie rendue à la fermeture et reprise à l'ouverture ; mutants éprouvés (focus initial, Échap) ; côté état (`tests/shell.rs`) : géométrie écrite dans les préférences hors historique, erreur dans le pied du dialogue ouvert seulement. Confirmations (#229) : Entrée et Échap = Cancel dans `modal_confirm` (le clic du verbe confirme), Entrée = Save et Échap = Cancel dans le trio, Discard Changes à gauche, verbe rouge au survol puis au focus clavier (Tab), pointeur parti au repos ; `modal_alert` : Entrée = Close, action par son bouton ; mutants éprouvés (Entrée de la confirmation, rouge au focus) |
| `stokesforge-app` : confirmations côté état et fenêtre (#229) | `tests/shell.rs` : New Project / Quit / Open… ne demandent que s'il y a du travail à perdre, Save enregistre puis poursuit (et un échec ou un sélecteur fermé abandonne le geste), projet illisible : chemin et projet gardés (M8), enregistrement raté (M8), export dont l'extension ajoutée viserait un fichier existant ; feuille et objets supprimés sans confirmation, message qui nomme les graphes (`tests/project_objects.rs`) ; `app::tests::fermer_la_fenetre_demande_avant_de_perdre_le_travail` (CancelClose, puis Close après Discard) ; `import_wizard::tests::supprimer_un_modele_passe_par_une_confirmation_modale` (Entrée annule sans valider l'assistant) |
| `stokesforge-app/src/i18n.rs` (tests) | toute clé de `fr` existe dans `en` avec les mêmes arguments ; la pseudo-langue entoure et allonge chaque message ; complétude bloquante (toute clé de `en` existe dans `fr`) ; pluriels et typographie françaises ; une préférence `fr` ouvre l'interface en français ; choix de la langue sans panique ; `tests/language_switch.rs` : la langue change sans redémarrer |
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
| `stokesforge-session/tests/recalc_cible.rs` | un cas nommé par composante de l'empreinte (texte, colonne désignée, version des sources, longueur du résultat, version propre, issue), les gestes qui les font changer (déplacement, cycle créé ailleurs, suppression de cellules dans une colonne-formule, macro avortée, feuille dupliquée…), la stabilité d'une chaîne et d'un cycle, l'oubli du mémo par `Load` et `NewProject` sur une coïncidence d'identifiants et de versions **fabriquée et vérifiée**. Chaque assertion « pas réécrite » a son témoin en mode complet |
| `stokesforge-session/src/lib.rs` (`tests`), `src/recalc.rs` (`tests`) | le recalcul interruptible (#239) : Stop annule la commande et laisse rétablir (arrêt posé d'avance sur le prochain recalcul, donc déterministe), Stop tardif sans effet sur le suivant, arrêt entre deux colonnes puis passe suivante qui achève sans réécrire ce qui l'est déjà. `stokesforge-app/tests/shell.rs` (`heavy`) : le même geste vu de l'interface sur un projet lourd, avec son témoin sans Stop |
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
| colonne désignée remplacée par la colonne-formule elle-même | `deplacer_une_source_change_ce_que_designe_la_lettre`, `b22` |
| mémo non vidé par `Load` | `le_chargement_oublie_le_memo_du_projet_remplace` seul — le tirage ne fabrique pas de coïncidence entre projets |
| mémo non vidé par `NewProject` | `un_nouveau_projet_oublie_le_memo_de_l_ancien` seul (un collage comme première requête : sinon la purge d'une passe ordinaire suffit) |
| insertion d'une colonne sans faire passer le compteur au-delà de sa version (`Project::insert_node`) | `une_colonne_ajoutee_avec_sa_version_ne_fait_pas_resservir_ce_numero`, le test unitaire de `core`, `b14` — pas `b22`, dont le tirage ne retombe pas sur le numéro |
| tout recalculer (le « ciblé » n'en est plus un) | les cas « ciblé » et leurs témoins, `b22` (propriété 3) |
| présence dans le mémo au lieu de l'égalité d'empreinte | quinze cas nommés, `b22` |
| `self_index` retiré | **personne, par construction** : une auto-référence finit dans le cycle, jamais dans `evaluate_formula`. Gardé parce que `evaluate_formula` le lit |
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
`stokesforge-app/tests/analysis.rs` vérifie le point et la zone uniques,
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
`script_bridge_lazy` exigent aussi la feature Python de `stokesforge-app`.
Avec l'environnement embarqué décrit dans [`.codex/README.md`](../.codex/README.md),
`scripts/python-test.sh` les inclut ; pour les isoler :

```sh
cargo test -p stokesforge-app --features python --test script_panel --test script_bridge_lazy
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
- Dans `stokesforge-app`, un test suit le code qu'il éprouve (#115) : une
  décision d'`AppState` dans `tests/shell.rs`, un geste peint dans le module
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
  unitaires de `stokesforge-app` n'en ont aucune (`default_path` rend `None`
  sous `cfg(test)`) ; les tests d'intégration (`tests/`) construisent leur
  état par `AppState::new_with_preferences(…, Preferences::load_from(None))`,
  jamais `AppState::new` (garde dans `tests/gardes.rs`). L'écriture du fichier
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

### Remplacement définitif d'une correction (#212)

`commands/tests/calculations.rs` tient l'atomicité données/trace, undo/redo,
longueur et positions, conflit de versions, refus des colonnes en lecture seule
et corrections contradictoires sur un Y partagé. `persistence/tests/calculations.rs`
garde les traces successives et l'allocateur après rechargement.
`session/tests/analyse.rs` vérifie que les calculs dépendants suivent le
remplacement et son annulation. `app/tests/analysis_state.rs` exerce la
confirmation des sources partagées ; `app::form::tests` refuse les options
grisées dans les radios et les listes. `script/tests/analysis.rs` vérifie le
retour de la poignée originale et la macro Script avec `replace_original=True`.
