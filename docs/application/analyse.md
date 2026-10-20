# Application : analyse

Suite du sommaire [`README.md`](README.md). Contenu : fenêtre d'analyse, menu Analysis, volet Results.

### Analyse : fenêtre, menu, volet Results (#181)

L'interface du traitement mathématique est répartie ainsi :

- `stokesforge-app-state/src/analysis.rs` — **le modèle de formulaire** `AnalysisForm` (sans egui) :
  famille, réglages de chaque famille (tous conservés quand on change de
  famille), séries, auxiliaire (référence, réponse), `issues()` (ce qui bloque
  Apply : `Operation::readiness()` de `stokesforge-analysis` pour les points, la
  zone et le modèle, traduit par `issue_of` ; le `Tree` pour le reste, #260),
  `operation_for(for_preview)`, `preview(tree)`, `draft(tree)` (noms
  longs des colonnes de sortie, M6 : la règle est `stokesforge_core::OutputName`,
  traduite ici par `output_name_text_in`, #256), `from_calculation` (édition), les gestes
  de l'aperçu (`add_point`, `add_zone`, `remove_near`). Testé sans fenêtre
  (`stokesforge-app-state/tests/analysis.rs`).
- `stokesforge-app-state/src/state/analysis.rs` — l'état et les `Action` (`OpenAnalysis`,
  `EditCalculation`, `ApplyAnalysis`, `RecalculateCalculations`,
  `DetachCalculation`, `RequestDeleteCalculation`/`ConfirmDeleteCalculation`) ;
  Apply passe par `AddCalculation`/`UpdateCalculation` et garde la fenêtre
  ouverte sur erreur.
- Les surfaces de `src/app/` : `analysis_menu`, `analysis_dialog`,
  `analysis_forms`, `analysis_preview`, `results_pane` (table des modules plus haut).
- Poignées sur le graphe (#199) : `AnalysisForm::handles` / `move_handle` (modèle,
  sans egui) listent et déplacent points d'ancrage, bornes de zones/plages et
  positions de pics ; `app/analysis_handles.rs` les peint par-dessus la figure
  (jamais dans `stokesforge-render`, donc jamais à l'export) pour le calcul
  dont une courbe est sélectionnée ou dont la ligne du volet Results est
  choisie. Pendant le glisser, la ligne de base est recalculée à la volée ;
  au lâcher, `Action::MoveCalculationHandle` rouvre le calcul comme la fenêtre
  et pousse un `UpdateCalculation` : **un pas d'historique par geste**, aucun
  si la poignée n'a pas bougé. Le nœud de l'Object Manager n'existe pas (un
  calcul n'est pas un nœud).
- L'aperçu est peint à la main (pas de graphe du projet) mais **sa géométrie ne
  lui appartient pas** (#261) : pixel ↔ données, zoom molette, rectangle de
  zoom, cadre initial (marge de 5 %, bornes mémorisées, inversion), aimantation
  sur la courbe et graduations viennent de `stokesforge-render`
  (`PanelTransform::new`, `axis_frame`, `snap_to_curve`, `scale::axis_ticks`) ;
  l'application construit une transformation par image (une seconde pour la
  bande des résidus) et convertit en `Pos2`. Les graduations ont donc le pas et
  le format de celles du graphe (point décimal, sans virgule), et suivent
  l'échelle logarithmique et l'inversion de l'axe du graphe source. Un zoom ou
  un rectangle sans image (borne logarithmique ≤ 0) garde le cadre. Il est
  **calculé sur un fil
  de travail** (#201, `analysis_preview.rs`, `PreviewJobs`) : un seul fil à la
  fois, la demande la plus récente remplace celle qui attend, l'image attend
  40 ms au plus puis montre l'ancien résultat marqué « Calculating… » ; le fil
  réveille le viewport une fois à sa fin, jamais de réveil périodique. Un
  calcul déjà lancé n'est pas interrompu : son résultat périmé est ignoré.
- Colonnes périmées : grisées dans la grille, lettre teintée (`theme::stale_color`)
  et info-bulle ; le volet Results (replié par défaut) liste les calculs
  conservés (portée Feuille / Classeur) avec Edit / Recalculate / Delete. Bandeau
  de 22 px centré, fond un cran plus clair (`theme::results_pane_fill`) ; la liste
  (≤ 45 % de la largeur) et le détail sont deux rectangles bornés, le texte du
  détail revient à la ligne ; une ligne de liste est « Opération · Feuille/Colonne »
  (`row_title`), sans catégorie.
- Une catégorie du sous-menu `Analysis ▸` du clic droit rouvre **la dernière
  méthode lancée dans cette catégorie** (tirée des analyses récentes, donc des trois
  dernières seulement), sinon `Category::default_preset`. `publish_last_presets`
  la publie chaque image dans le `Context`, `category_preset` la lit : les menus
  du graphe n'ont pas l'état sous la main. Seule la méthode est reprise, pas ses
  réglages.
- Séries pas à jour sur le graphe (#198) : le cœur résout le drapeau
  (`ResolvedSeries.stale` : une des colonnes X, Y ou d'erreur sort d'un calcul
  pas à jour) et le rendu l'applique sans rien savoir du modèle — trait grisé
  (`#9a9a9a`) et triangle d'avertissement juste au-dessus de son dernier point ; la ligne de
  série de l'Object Manager porte l'icône `Icon::Stale`. Le réglage vit dans le
  graphe (`PlotSpec::stale_warning`, case « Warn about out-of-date data » de la
  page Graph des propriétés, défaut actif, fichier ancien = actif). Exporter ou
  copier la figure d'un graphe qui montre un tel calcul demande d'abord
  (`app/stale_export.rs`, `AppState::pending_stale_export`) : Export Anyway,
  Export Without Warning (même figure, sans gris ni marqueur), Recalculate and
  Export, Cancel. Ni commande ni historique pour la confirmation elle-même ; le
  recalcul passe par les commandes habituelles.
- Les erreurs viennent de `errors::analysis::error_message` (match exhaustif).
- Fenêtre d'ajustement (#204) : trois exponentielles distinctes (`Exponential`,
  `Exponential with Plateau`, `Two Exponentials`, mêmes noms que `pyst` :
  `exponential`, `exponential_offset`, `exponential2`) ; la formule du modèle
  est composée par Typst (`model_formula_source` →
  `stokesforge_render_egui::formula_label`, cache et polices gardés dans la
  mémoire d'egui) ; `Auto` remplit les valeurs de départ par
  `CompiledModel::guess` (pour le modèle libre, un balayage par paramètre qui
  suit l'échelle des données) ; une aire s'affiche avec le produit des unités de
  X et de Y quand les deux existent (`value_unit`) ; la fenêtre des pics borne
  les centres (`center_window`, aussi `pyst.analysis.peaks`).
- Finitions (#200) : « Add to graph » (case de la sortie, visible quand un
  graphique montre la courbe ; `AppState::graphs_plotting`) crée le calcul **et**
  trace ses colonnes (hors résidus, `PanelSpec::push_series_batch` via
  `AddSeriesToGraph`) dans une seule macro `MacroKind::AnalysisToGraph` ;
  Show / Keep in Results se souvient du dernier choix d'une session à l'autre
  (`preferences.json`, `analysis_keep_results`) ; l'en-tête d'une colonne pas à
  jour porte l'icône `Icon::Stale` (Phosphor `warning`, rôle neutre) à gauche de
  sa lettre teintée.
- Analyse depuis une courbe (#210) : le clic droit de la figure et de
  l’Object Manager partagent l’entrée « Analysis… ». Le menu Analysis du haut
  utilise aussi la courbe ou le groupe sélectionné quand le graphique est actif.
  Le formulaire conserve les références X/Y de chaque série, son style et son
  cadrage ; il n’apparie pas de nouveau les colonnes selon leur position dans la
  feuille. Un groupe ouvre un lot, les données d’un classeur caché restent
  analysables. « Add to graph » est coché au départ et les colonnes produites
  rejoignent le calque et le groupe de leur courbe source.
  L’aperçu conserve aussi l’échelle logarithmique et l’inversion des axes,
  avec les gestes en coordonnées des données. Pour des barres ou un
  histogramme, il relie les Y bruts à analyser avec le style de la source ;
  il ne calcule pas d’histogramme des résultats.
- Ligne de base constante (#211) : « Constant… » est la première méthode du
  menu Baseline. Le fond plat se fixe par un point unique (libre, ou aimanté avec
  Maj et moyenne locale) éditable en X/Y, ou par la moyenne d’une zone inclusive
  de X. Une nouvelle zone remplace la précédente ; seuls les couples X/Y finis
  entrent dans sa moyenne. Subtract et Divide réutilisent les mêmes opérations
  que les autres lignes de base.
- Remplacement des données originales (#212) : la sortie d’une ligne de base
  ou d’une correction par référence peut écraser ses colonnes Y ordinaires.
  La colonne conserve sa longueur et ses cellules non exploitables ; les
  courbes qui la tracent suivent les nouvelles valeurs. Les colonnes à formule
  ou calculées restent protégées. Plusieurs courbes qui partagent une source
  déclenchent une confirmation. La commande `ReplaceAnalysisData` écrit les
  données et une trace immuable `CorrectionRecord` dans une même entrée
  annulable, sans lien de calcul ni colonne produite ; les réglages ne se
  rouvrent pas. Résultats conserve les corrections successives, y compris
  après une saisie manuelle, et le fichier `.stf` les enregistre. Le petit
  crayon neutre dans l’en-tête renvoie à ces réglages par son info-bulle ;
  Annuler retire les valeurs corrigées et la trace du geste ensemble.
- Le choix du type d'analyse se fait dans la fenêtre (liste « Analysis type »,
  `family_chooser`) tant qu'elle crée un calcul ; un calcul rouvert garde sa
  famille (#202).
- Lignes écartées et comptées (`skipped`) : X ou Y non fini, et, en ajustement
  pondéré, σ vide, nul ou négatif (#205) — jamais un échec de tout l'ajustement.
  Un paramètre que les données ne déterminent pas (colonne du jacobien nulle ou
  combinaison des précédentes) n'a pas d'incertitude et lève
  `Warning::Undetermined(n)` ; si aucun paramètre libre n'est déterminé,
  l'ajustement est refusé (`SingularSystem`, #203).

