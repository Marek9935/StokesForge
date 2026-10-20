# Chantier analyse (#181) — maquettes à choisir avant de lancer

> Rapport du 2026-10-08, choix faits le 2026-10-09. **Aucun code** : des maquettes d'écran et, pour chaque
> point encore ouvert dans #181–#190, deux à quatre options chiffrées. La
> logique validée le 2026-10-08 (Rust pur, volet Results C', recalcul
> automatique, souris avec aimant, etc.) n'est **pas** rediscutée ici : les
> maquettes la mettent en forme.

## Décisions (2026-10-09)

Choix de l'utilisateur, à reporter dans les sous-tickets. Là où la décision
s'écarte de la maquette ou la complète, elle fait foi.

- **M1 B** : sous-menus par famille (`Baseline ▸`, `Fit ▸`,
  `Peaks and Deconvolution ▸`), la méthode se choisit dans le menu. La
  fenêtre doit quand même savoir changer de méthode (pour
  `Edit Calculation…`).
- **M2 A** : sous-menu `Analysis ▸` dans la section « Créer » du clic droit.
- **M3 A** : modale avec son propre aperçu cliquable. Cet aperçu servira
  aussi à afficher des **aides visuelles propres à chaque opération** (zones
  de fond, aires hachurées, composantes, résidus…).
- **M4 a, A + B** : des outils explicites sous l'aperçu **et** des gestes
  implicites selon l'opération (Ctrl+glisser pour zoomer). Si les gestes
  implicites entrent trop souvent en conflit, on passe aux outils explicites
  seuls (A).
- **M4 b, C complété** : par défaut, un point se pose librement ; **Maj
  maintenue** l'aimante sur la courbe, avec la moyenne locale de A. Tant que
  Maj est maintenue au-dessus du graphe, une info-bulle indique que
  **Maj + molette** règle la sensibilité (le nombre de points moyennés). Le
  même réglage existe aussi sans molette, dans un champ classique de la
  fenêtre (`Average over [ 5 ] points`).
- **M5 A complété** : bouton radio `Show / Keep in Results`, avec un
  **comportement par défaut réglable dans les préférences de l'application**,
  à traiter avec #148 (fenêtre Preferences).
- **M6 A** : nom long = nom source + suffixe court (`Intensity (B-corr)`).
- **M7 a A, b P1, c A** : liste + détail ; volet entre la grille et les
  onglets ; bascule `This sheet | Workbook`.
- **M8 C, libellés changés** : la confirmation porte
  `[ Delete ]  [ Delete with Columns ]  [ Cancel ]`.
- **M9 a A complété** : toute la colonne grisée avec une icône dans l'en-tête,
  **et** les séries qui la tracent grisées sur les graphes, avec un
  **marqueur distinctif** (triangle portant un « ! ») qui dit clairement que
  les données sont périmées. **À l'export** (SVG, PNG, PDF, Ctrl+C) d'une
  figure qui contient des séries périmées, une confirmation propose
  d'exporter avec ou sans l'avertissement. Le graphe peut aussi
  **désactiver cet avertissement** (réglage du graphe).
- **M9 b C** : case `Recalculate automatically`, défaut selon l'opération.
- **M9 c** : `Recalculate` partout où se trouve le calcul (pas d'alternative).
- **M9 d A** : saisie refusée, `Detach from Calculation` proposé.
- **M10 a B** : poignées visibles quand le calcul est sélectionné.
- **M10 b** : segments ou spline au choix, segments par défaut.
- **M10 c** : les mêmes poignées pour les bornes d'intégration et les
  positions initiales des pics.
- **M11 A** : hors de la référence, cellule vide comptée.
- **M12 a B, b** : plusieurs plages dans un calcul ; sous la courbe, zéro ou
  droite entre les bornes.
- **M13 a B** : **modèle libre saisi par l'utilisateur dès la v1** (analyseur
  d'expressions avec fonctions, dérivées numériques).
- **M13 b** : **ajustement global dès la v1** (paramètres partagés entre
  séries, colonne `Shared`).
- **M14 a C, b C** : proéminence relative + nombre maximal ; ligne de base
  constante, linéaire ou aucune.
- **M15 a A** : IRF interpolée sur la grille du signal, avertissement si les
  pas diffèrent de plus de 1 %.
- **M15 b B** : **Richardson-Lucy par défaut**, Wiener en option.

Coût des choix au-dessus du socle chiffré dans les sous-tickets :
**≈ 13 à 15 j** (détail dans le récapitulatif), dont ≈ 6,5 j pour le modèle
libre et l'ajustement global (M13).

## Comment lire et répondre

- Chaque point porte un numéro **M1…M15**, les options une lettre ; ★ marque
  la recommandation. Le coût est donné en jours de développement, tests
  compris, **en plus** du socle déjà chiffré dans les sous-tickets.
- Les libellés des maquettes sont en **anglais** (catalogue `en` d'abord,
  `docs/textes-interface.md` R1) ; les termes nouveaux et leur traduction
  sont regroupés en fin de rapport.
- Pour répondre, une ligne suffit : « tout ★ sauf M7 B et M13 b-oui », ou
  point par point. Le tableau récapitulatif est en fin de document.
- Conventions des maquettes : `( )`/`(•)` bouton radio, `[ ]`/`[x]` case,
  `[ … ▾]` liste déroulante, `[ 123 ]` champ, `▌` filet d'accent d'une ligne
  sélectionnée (`list_row`), `▔Sheet1▔` onglet actif (`tab_strip`, trait
  au-dessus pour les feuilles).

## Ce que les maquettes ont fait apparaître

Trois constats, trouvés en dessinant, qui pèsent sur les choix :

1. **La fenêtre modale empêche de cliquer sur le graphe.** `modal_dialog_with`
   bloque tout clic hors de la fenêtre (le contour clignote en rouge,
   `docs/application/graphe.md` § Propriétés des graphiques). Or la fenêtre de
   réglage commune est une modale (règle du dépôt) **et** les points et
   plages se posent à la souris sur le graphe (décision du 2026-10-08). Les
   deux ne tiennent ensemble que si la fenêtre porte **son propre aperçu
   cliquable**, ou si elle se retire pendant la pose. C'est le point M3.
2. **Un lancement depuis le classeur n'a pas de graphe** pour l'aperçu ni pour
   la souris. Même réponse : un aperçu dans la fenêtre (M3 A) règle les deux
   cas d'un coup.
3. **Le volet Results par feuille disperse les lots.** L'import courant met
   un fichier par feuille (« Active workbook (one sheet per file) ») ; un
   ajustement lancé sur 20 courbes d'un graphe range donc ses 20 résultats
   sur 20 feuilles, et comparer les 20 τ oblige à ouvrir 20 onglets. La
   décision C' reste ; M7 propose de quoi voir un lot d'un coup.

---

## M1 — Menu Analysis dans la barre (#182)

Place décidée : entre Graph et View. Reste à fixer sa forme. Grammaire
(`docs/menus.md`) : sous-menu seulement pour un choix exclusif ou au moins
3 entrées, profondeur 2, verbes nus, « … » si une fenêtre suit.

### A ★ — Une entrée par opération, la méthode se choisit dans la fenêtre

```
 File  Edit  Workbook  Graph  Analysis  View  Window
                             ┌──────────────────────────────┐
                             │ Subtract Baseline…           │
                             │ Correct by Reference…        │
                             ├──────────────────────────────┤
                             │ Integrate…                   │
                             │ Fit…                         │
                             │ Decompose Peaks…             │
                             │ Deconvolve by IRF…           │
                             ├──────────────────────────────┤
                             │ Recalculate Out of Date      │
                             └──────────────────────────────┘
```

La méthode (linéaire, polynôme, points ; modèle d'ajustement ;
reconvolution ou directe) est une liste dans la fenêtre : on peut en
changer **en regardant l'aperçu**, sans refermer. Menu court, sans
sous-menu.

### B — Sous-menus par famille, la méthode se choisit dans le menu

```
                             ┌──────────────────────────────┐
                             │ Baseline                   ▸ │──┬───────────────────┐
                             │ Correct by Reference…        │  │ Linear…           │
                             ├──────────────────────────────┤  │ Polynomial…       │
                             │ Integrate…                   │  │ Anchor Points…    │
                             │ Fit                        ▸ │  └───────────────────┘
                             │ Peaks and Deconvolution    ▸ │
                             ├──────────────────────────────┤
                             │ Recalculate Out of Date      │
                             └──────────────────────────────┘
       Fit ▸ : Linear… · Polynomial… · Exponential… · Logarithm… · Power…
               · Gaussian… · Lorentzian…
       Peaks and Deconvolution ▸ : Decompose Peaks… · Reconvolution…
               · Direct Deconvolution…
```

Plus proche d'Origin, un clic de moins pour qui sait ce qu'il veut ; mais
changer de méthode oblige à annuler et rouvrir, et la fenêtre doit quand
même savoir changer de méthode (pour « Modifier les paramètres »).

### C — A, avec les modèles d'ajustement en sous-menu

A partout, sauf `Fit ▸` (sept modèles) qui préremplit le modèle. Compromis,
mais deux chemins vers la même fenêtre.

| | A ★ | B | C |
|---|---|---|---|
| Changer de méthode sans refermer | oui | non | oui sauf Fit |
| Longueur du menu | 7 entrées | 5 + 13 en sous-menus | 6 + 7 |
| Coût | réf. | +0,5 j (libellés, tests de grammaire) | +0,3 j |

**Entrée « Recalculate Out of Date »** : présente dans les trois options,
grisée (info-bulle « Everything is up to date ») quand rien n'attend ; elle
sert le recalcul manuel des calculs lourds (M9 c). Pas de raccourci proposé.

---

## M2 — Clic droit : courbe, Object Manager, en-tête de colonne (#182)

Les mêmes entrées que le menu de la barre (une seule peinture partagée,
`analysis_menu_contents`, enveloppée dans la barre, à plat ou en sous-menu
au clic droit selon l'option).

### A ★ — Sous-menu `Analysis ▸` dans la section « Créer »

```
 Courbe (graphe, Object Manager)          En-tête de colonne Y
 ┌──────────────────────────────┐         ┌───────────────────────────────┐
 │ Series: B                    │ (grisé) │ Plot                        ▸ │
 │ Show Data                    │         │ Add Column                    │
 ├──────────────────────────────┤         │ Analysis                    ▸ │
 │ Analysis                   ▸ │         ├───────────────────────────────┤
 ├──────────────────────────────┤         │ Cut · Copy · …                │
 │ Rename                       │         ├───────────────────────────────┤
 │ Move Up                      │         │ Type ▸ · Role ▸               │
 │ Move Down                    │         ├───────────────────────────────┤
 │ Hide                         │         │ Clear · Delete and Shift Up   │
 ├──────────────────────────────┤         └───────────────────────────────┘
 │ Delete                       │
 ├──────────────────────────────┤
 │ Properties…                  │
 └──────────────────────────────┘
```

Le menu de la courbe ne grossit que d'une ligne. Profondeur 2 respectée.

### B — Entrées à plat dans la section « Créer »

Les six opérations directement dans le menu de la courbe : un clic de moins,
mais un menu de 14 lignes, et la grammaire demande des menus contextuels
courts.

| | A ★ | B |
|---|---|---|
| Longueur du menu de la courbe | 9 | 14 |
| Coût | réf. | même coût |

**Sur une colonne calculée** (en-tête, ou courbe qui la trace), le même menu
gagne en tête de la section « Organiser » : `Edit Calculation…` (rouvre la
fenêtre sur ce calcul), `Recalculate` (grisé si à jour), `Detach from
Calculation` (voir M9 d). Proposé pour toutes les options.

---

## M3 — Fenêtre de réglage : où vivent l'aperçu et la souris (#182)

Voir le constat 1. C'est **le** choix structurant du socle.

### A ★ — Modale avec son propre aperçu cliquable

```
┌ Subtract Baseline ──────────────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────────┐ │
│ Series   [ Sheet1 › B (+3)        ▾]  │ │ ▲                                │ │
│ X range  (•) All                      │ │ │        ╱╲                      │ │
│          ( ) From [  420  ] to [ 680 ]│ │ │       ╱  ╲         ╱╲            │ │
│          ( ) Current zoom             │ │ │  ____╱    ╲___    ╱  ╲____      │ │
│                                       │ │ │ ●- - - - - - -●- - - - - -●    │ │
│ Baseline                              │ │ └─────────────────────────────▶  │ │
│ Method   [ Anchor points          ▾]  │ │  400        500        600   λ   │ │
│ Points   3 · click to add,            │ ├──────────────────────────────────┤ │
│          right-click to remove        │ │ [Add Points] [Zone] [Zoom] [Fit] │ │
│ Shape    (•) Segments  ( ) Spline     │ │ Showing: B · 1 of 4  [◀] [▶]     │ │
│ Apply    (•) Subtract  ( ) Divide     │ └──────────────────────────────────┘ │
│                                       │                                      │
│ Output                                │                                      │
│ [x] Corrected column                  │                                      │
│ [ ] Baseline column                   │                                      │
│ [x] Add to graph   (Graph1)           │                                      │
│ Values  ( ) Show  (•) Keep in Results │                                      │
│ [x] Recalculate automatically         │                                      │
├───────────────────────────────────────┴──────────────────────────────────────┤
│                                                           [ OK ]  [ Cancel ] │
└──────────────────────────────────────────────────────────────────────────────┘
```

- La fenêtre reste une `modal_dialog_with` (règle du dépôt tenue).
- L'aperçu est un petit graphe interactif (même rendu que les graphes, aimant
  `nearest_on_curve`, zoom à la molette) : il marche aussi depuis le
  classeur, où il n'y a pas de graphe.
- Lancée depuis un graphe, l'aperçu reprend ses styles et son zoom ; le
  graphe derrière montre aussi le résultat en direct (comme les Propriétés).
- En lot, `[◀] [▶]` passe d'une série à l'autre ; les points posés valent
  pour toutes (option par série : plus tard).
- Coût : un `GraphView` dans la fenêtre, ≈ +1,5 j sur le socle.

### B — Modale qui se retire pendant la pose (« Pick on Graph », façon Origin)

```
┌ Subtract Baseline ───────────────────────── ✕ ┐
│ …                                             │
│ Points   3   [ Pick on Graph ]                │   ← la fenêtre se cache
│ …                                             │
└───────────────────────────────────────────────┘

  Graph1 ─────────────────────────────────────────────────────────────────
  │ Picking anchor points · Enter to finish · Esc to cancel             │
  │                ╱╲                                                   │
  │   ●___________╱  ╲____●______╱╲_____●                               │
```

Le vrai graphe, en grand ; mais allers-retours, une exception au blocage
modal à écrire et à tester, et rien à cliquer depuis le classeur (il faut un
graphe provisoire). Coût ≈ +1 j, plus le cas « classeur » ≈ +1 j.

### C — Outil non modal ancré au graphe (façon Peak Analyzer)

```
  Graph1 ──────────────────────────────────────────┬──────────────────────
  │                                                │ Subtract Baseline   │
  │         ╱╲             ╱╲                      │ Method [Anchor ▾]   │
  │   ●____╱  ╲_____●_____╱  ╲____●                │ Points 3            │
  │                                                │ …                   │
  │                                                │ [ OK ] [ Cancel ]   │
```

Graphe pleinement interactif, mais une **nouvelle brique** de fenêtre (hors
`modal_dialog_with`, donc contre la règle et la garde
`aucune_fenetre_n_est_une_egui_window_brute` à étendre), une macro ouverte
pendant que l'utilisateur peut cliquer ailleurs, et toujours rien depuis le
classeur. Coût ≈ +3 j.

| | A ★ | B | C |
|---|---|---|---|
| Règle « toute fenêtre par la brique commune » | tenue | exception | nouvelle brique |
| Depuis le classeur | oui | graphe provisoire | non |
| Taille de la zone cliquable | moyenne (fenêtre redimensionnable) | grande | grande |
| Coût | +1,5 j | +2 j | +3 j |

**Modification après coup** (#185) : quelle que soit l'option, la ligne de
base et les bornes restent modifiables **sur le vrai graphe** par leurs
poignées (M10 a) ; M3 ne concerne que la création et « Edit Calculation… ».

---

## M4 — Gestes de souris dans l'aperçu (#182, #185)

### a. Outil actif

| Option | Geste | Pour | Contre | Coût |
|---|---|---|---|---|
| **A ★ — Outils explicites** sous l'aperçu (`[Add Points] [Zone] [Zoom]`), celui par défaut selon l'opération | clic = point ; glisser = zone ; glisser un point = le déplacer ; clic droit ou Suppr = le retirer | on voit ce que fera le clic ; zoom sans conflit | un clic de plus pour changer d'outil | réf. |
| B — Outil implicite selon l'opération | même gestes, Ctrl+glisser = zoom | pas de barre | zoom caché derrière une touche | −0,2 j |

### b. Où s'accroche un point

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Aimant sur la courbe, moyenne locale en option** (`Average over [ 5 ] points`, infobulle « Less sensitive to noise ») | le point tombe sur un point mesuré ; la moyenne lisse le bruit | +0,3 j |
| B — Aimant seul | plus simple, sensible au bruit | réf. |
| C — Point libre, Maj pour aimanter | permet un point hors courbe (fond extrapolé) | +0,2 j |

---

## M5 — Bloc « Output » : montrer ou garder les valeurs (#183)

Décidé : toujours le choix entre **montrer** (rien dans le projet) et
**garder** (volet Results).

### A ★ — Bouton radio dans la fenêtre, mémorisé par opération

`Values  ( ) Show  (•) Keep in Results` (maquette M3 A). Prévisible, adapté
aux lots. La fenêtre « Show » propose quand même `Keep in Results` pour se
raviser.

### B — Deux boutons de validation

`[ Show Values ]  [ OK ]  [ Cancel ]` : le choix se fait en validant. Plus
court, mais deux boutons d'action côte à côte se confondent (R14 : un bouton
= un verbe).

### C — Toujours montrer, puis garder au besoin

OK calcule et ouvre toujours la fenêtre des valeurs, avec `Keep in Results`.
On décide en voyant ; une fenêtre de plus à chaque calcul, pénible en lot.

### La fenêtre des valeurs (commune aux trois)

```
┌ Fit Results ─────────────────────────────────────────────────── ✕ ┐
│ Exponential decay, 1 term + offset · Sheet1 › C                   │
│ 512 points used, 3 ignored (empty or text)                        │
│ ┌───────────┬──────────────┬──────────────┬──────────┐            │
│ │ Parameter │ Value        │ Std. error   │ Unit     │            │
│ ├───────────┼──────────────┼──────────────┼──────────┤            │
│ │ y0        │ 12.4         │ 0.8          │ counts   │            │
│ │ A1        │ 9 831        │ 41           │ counts   │            │
│ │ τ1        │ 4.213        │ 0.031        │ ns       │            │
│ └───────────┴──────────────┴──────────────┴──────────┘            │
│ R² 0.9981 · Reduced χ² 1.07                                       │
├───────────────────────────────────────────────────────────────────┤
│                       [ Copy ]  [ Keep in Results ]  [ Close ]    │
└───────────────────────────────────────────────────────────────────┘
```

`Copy` met le tableau au presse-papiers en texte tabulé (se colle dans une
feuille, un tableur, un cahier). En lot : une ligne par série, une colonne
par paramètre (voir M7 c).

| | A ★ | B | C |
|---|---|---|---|
| Lots | bien | bien | une fenêtre à fermer |
| Coût | réf. | −0,2 j | même coût |

---

## M6 — Colonnes produites : place et nom (#182)

Décidé : juste après la colonne Y source, nom long qui dit l'opération,
détail en commentaire, trace persistée.

```
┌ Book1 ──────────────────────────────────────────────────────────────────┐
│       │ A(X)    │ B(Y)      │ C(Y)               │ D(Y)               │ │
│ Long  │ λ       │ Intensity │ Intensity (B-corr) │ Baseline of B      │ │
│ Units │ nm      │ counts    │ counts             │ counts             │ │
│ Comm. │         │           │ Anchor points, 3,  │ Anchor points, 3,  │ │
│       │         │           │ subtracted         │ segments           │ │
│ Meta  │         │           │ ⤷ Baseline 1       │ ⤷ Baseline 1       │ │
│ 1     │ 400.0   │ 152.3     │ 1.8                │ 150.5              │ │
│ 2     │ 400.5   │ 151.9     │ 1.5                │ 150.4              │ │
```

`⤷ Baseline 1` dans la ligne Metadata est un lien : un clic déplie le volet
Results et y sélectionne le calcul (#183).

### Forme du nom long

| Option | Exemple | Pour | Contre |
|---|---|---|---|
| **A ★ — Nom source + suffixe court** | `Intensity (B-corr)`, `Intensity (fit)`, `Intensity (resid.)`, `Peak 1 of B` | se lit comme le titre d'axe repris (Long Name / Units) ; la source reste en tête | abréviations à fixer dans le glossaire |
| B — Opération + lettre source | `Baseline corrected B`, `Fit of B`, `Residuals of B` | sans abréviation | la grandeur (Intensity) disparaît du titre d'axe et de la légende |
| C — Nom libre saisi dans « Output » | champ prérempli par A | maîtrise totale | un champ de plus par sortie |

Coût identique (C : +0,3 j).

---

## M7 — Volet Results sous la grille (#183)

Décidé : C', un volet repliable sous la grille, qui suit la feuille active ;
replié par défaut, compteur, absent sans résultat.

### a. Contenu du volet

**A ★ — Liste + détail**

```
┌ Book1 ──────────────────────────────────────────────────────────────────┐
│ (grille…)                                                             ▲ │
│                                                                       │ │
├═════════════════════════════════════════════════════════════════════ ═══┤ ← poignée
│ ▾ Results (3)                                  [This sheet | Workbook]  │
│ ┌──────────────────────────────┬──────────────────────────────────────┐ │
│ │▌Baseline 1      B            │ Fit 1 · Exponential decay, 1 term    │ │
│ │ Integrate 1     C   1.28e4   │ Input   Sheet1 › C, X 0 – 50 ns      │ │
│ │ Fit 1           C   τ 4.21   │ Output  E, F                         │ │
│ │                  (!) out of  │ y0   12.4   ± 0.8     counts         │ │
│ │                      date    │ A1   9 831  ± 41      counts         │ │
│ │                              │ τ1   4.213  ± 0.031   ns             │ │
│ │                              │ R² 0.9981 · Reduced χ² 1.07          │ │
│ │                              │ [Edit…] [Recalculate] [Copy] [Delete]│ │
│ └──────────────────────────────┴──────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────┤
│ ▔Sheet1▔   Sheet2   Sheet3                                              │
│ ◄═══════════════════════════════════════════════════════►              │
└─────────────────────────────────────────────────────────────────────────┘

 Replié :  │ ▸ Results (3)                                                 │
```

Liste par `list_row`, détail à droite, titre par `theme::section_title`.
Hauteur réglable par la poignée, mémorisée par classeur (mémoire d'egui,
comme la ligne Metadata, pas dans le `.stf`).

**B — Fiches empilées** : une fiche par calcul, toutes dépliées l'une sous
l'autre (pas de détail à droite). Lisible pour 2 ou 3 calculs, long à
parcourir au-delà.

**C — Arbre** (`tree_lines`) : `Fit 1 ▸` déplie ses paramètres en lignes.
Compact, mais la comparaison de deux calculs côte à côte est impossible.

### b. Position du volet

| Option | Ordre de haut en bas | Pour | Contre |
|---|---|---|---|
| **P1 ★** | grille · **Results** · onglets · barre horizontale | les onglets restent au bord, comme un tableur ; ils pilotent grille et volet | la barre horizontale de la grille s'éloigne de la grille (elle l'est déjà, sous les onglets) |
| P2 | grille · onglets · barre · **Results** | la grille garde ses barres collées | le volet semble ne dépendre d'aucune feuille |

### c. Voir un lot d'un coup (constat 3)

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Bascule `This sheet | Workbook`** dans l'en-tête du volet | « Workbook » liste les calculs de toutes les feuilles du classeur, colonne « Sheet » en plus ; un lot lancé ensemble s'y range en tableau série × paramètre, copiable | +1 j |
| B — Rien de plus | un lot se compare par la fenêtre des valeurs (M5) au moment du calcul, ou par `pyst` | 0 |
| C — Commande `Copy Batch Results` sur un calcul d'un lot | un tableau série × paramètre au presse-papiers, sans changer le volet | +0,5 j |

« Workbook » ne crée ni feuille ni objet : c'est un filtre du même volet.

---

## M8 — Supprimer un résultat ou une colonne produite (#183)

| Option | Supprimer un **résultat** | Supprimer une **colonne produite** | Coût |
|---|---|---|---|
| A — Liés | supprime aussi ses colonnes (confirmation qui les nomme) | supprime le résultat si c'était sa seule sortie, sinon le résultat ne l'écrit plus | réf. |
| B — Indépendants | les colonnes restent et deviennent ordinaires (trace retirée, plus de recalcul) | le résultat reste, la sortie y est marquée « deleted » | réf. |
| **C ★ — Au choix au moment de supprimer** | confirmation à deux verbes : `[ Delete Result ]  [ Delete with Columns ]  [ Cancel ]` | comme B | +0,3 j |

```
┌ Delete Result ───────────────────────────────────────────── ✕ ┐
│ Fit 1 wrote columns E (Intensity (fit)) and F (resid.).       │
│ Keep them as ordinary columns, or delete them too?            │
├───────────────────────────────────────────────────────────────┤
│        [ Delete Result ]  [ Delete with Columns ]  [ Cancel ] │
└───────────────────────────────────────────────────────────────┘
```

Une seule entrée d'historique dans tous les cas.

---

## M9 — Recalcul : grisé, calculs lourds, édition à la main (#184, #185)

### a. Une colonne périmée (échec ou attente de recalcul manuel)

```
│       │ B(Y)      │ C(Y)  (!)          │ ← info-bulle de l'icône :
│ Long  │ Intensity │ Intensity (B-corr) │   « Out of date: the fit no longer
│ 1     │ 152.3     │ 1.8                │     converges (B changed). Showing
│ 2     │ 151.9     │ 1.5                │     the last valid values »
          normal      texte grisé
```

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Toute la colonne grisée**, icône d'avertissement dans l'en-tête, motif en info-bulle | un calcul est un tout : toutes ses valeurs viennent du même passage | réf. |
| B — Seules les cellules changées | précis en apparence, mais une ligne de base ou un ajustement change toutes les valeurs : rien à distinguer | +0,5 j |

Même grisé et même icône sur la ligne du volet Results (maquette M7).

### b. Qui décide qu'un calcul est « lourd » (recalcul manuel)

| Option | Effet | Coût |
|---|---|---|
| A — L'opération | reconvolution et déconvolution toujours manuelles | réf. |
| B — Un seuil | au-delà de N séries × points, manuel | +0,3 j, seuil à régler |
| **C ★ — L'utilisateur, avec un défaut par opération** | case `Recalculate automatically` dans « Output » (M3 A), cochée par défaut sauf reconvolution, déconvolution directe et lots de plus de 20 séries ; modifiable ensuite par `Edit Calculation…` | +0,2 j |

### c. Où relancer un recalcul manuel

**★ Partout où se trouve le calcul** (la grammaire veut que rien ne soit
seulement au clic droit) : bouton `Recalculate` du détail (M7), entrée
`Recalculate` au clic droit d'une colonne produite (M2), et
`Analysis › Recalculate Out of Date` pour tout le projet (M1). Pas
d'alternative proposée.

### d. Saisie à la main dans une colonne produite

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Refusée**, message qui propose de détacher | « This column is computed by Baseline 1. Detach it from the calculation to edit it » + `Detach from Calculation` (la colonne devient ordinaire, le résultat garde ses valeurs) | réf. |
| B — Acceptée, le lien se rompt après confirmation | plus souple, une confirmation à chaque fois | +0,2 j |
| C — Acceptée, écrasée au prochain recalcul | comme Origin ; la saisie peut disparaître sans prévenir | réf. |

---

## M10 — Ligne de base (#185)

Fenêtre : maquette M3 A. Selon `Method` le bloc change :

```
Method   [ Linear               ▾]       Method   [ Polynomial           ▾]
From     (•) Two points ( ) Zones        Degree   [  3  ]
Points   2 · click to add                Zones    4 · drag to add
Apply    (•) Subtract ( ) Divide         Apply    (•) Subtract ( ) Divide
```

« Zones » : plages de fond posées en glissant, sur lesquelles la droite ou
le polynôme est ajusté ; elles s'affichent en bandes pâles dans l'aperçu.

### a. Poignées sur le vrai graphe après création

```
  Graph1
  │            ╱╲                ╱╲
  │   ◆_______╱  ╲______◆______╱  ╲______◆      ◆ poignées de la ligne de base
  │   - - - - - - - - - - - - - - - - - - -     (en pointillés, couleur de B)
```

| Option | Quand voit-on la ligne de base et ses poignées | Coût |
|---|---|---|
| A — Toujours | encombre une figure destinée à l'export ; il faudrait l'exclure de l'export | +0,5 j |
| **B ★ — Quand le calcul est sélectionné** : clic sur la courbe corrigée, ligne du volet Results, nœud de l'Object Manager | rien sur la figure au repos ni à l'export ; glisser une poignée = un pas d'historique | réf. |
| C — Case « Show baseline » par calcul | choix fin, une bascule de plus à gérer et à persister | +0,5 j |

### b. Ligne par points : segments ou courbe lisse

**★ Au choix** (`Shape (•) Segments ( ) Spline`, segments par défaut).
Alternative : segments seuls (−0,3 j).

### c. Même principe pour les autres paramètres posés sur le graphe

**★ Oui** : bornes d'intégration (M12) et positions initiales des pics (M14)
ont les mêmes poignées, visibles quand le calcul est sélectionné.
Alternative : poignées pour la ligne de base seulement, le reste par
`Edit Calculation…` (−1 j).

---

## M11 — Correction par une colonne de référence (#186)

```
┌ Correct by Reference ───────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────┐ │
│ Series     [ Sheet1 › B         ▾]    │ │  données ───  référence ┄┄┄  │ │
│                                       │ │      ╱╲   ┄┄┄┄┄┄┄┄           │ │
│ Reference                             │ │ ┄┄┄┄╱  ╲┄┄        ┄┄┄┄       │ │
│ Column     [ Detector › B (QE)  ▾]    │ │ ___╱    ╲____________        │ │
│ X          Detector › A (λ / nm)      │ │ ░░░                     ░░░░ │ │
│ Operation  (•) Divide ( ) Multiply    │ │ ↑ hors de la référence       │ │
│            ( ) Subtract               │ └──────────────────────────────┘ │
│ Outside    32 points outside the      │                                  │
│ reference  reference range            │                                  │
│ Output  …                             │                                  │
```

Ordre retenu dans l'application (#252) : Input, Analysis type, puis la
colonne Reference en tête du bloc de la famille (Response pour la
déconvolution), puis l'opération et Output — comme pour la Baseline.

La référence est ramenée sur la grille X des données par interpolation
linéaire (décidé). Points hors du domaine de la référence :

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Cellule vide (NaN), comptée** | honnête : rien n'est inventé ; le compte s'affiche (maquette) et dans le résultat | réf. |
| B — Refus si la référence ne couvre pas tout | sûr, mais bloque le cas courant d'une référence un peu plus courte | réf. |
| C — Dernière valeur prolongée | courbe continue, mais une correction inventée aux bords | réf. |

La correction suit les changements de la référence (recalcul automatique,
dépendance entre feuilles) : conséquence directe des décisions, à confirmer
seulement.

---

## M12 — Intégration (#187)

```
┌ Integrate ──────────────────────────────────────────────────────────── ✕ ┐
│ Input                                 │ ┌──────────────────────────────┐ │
│ Series     [ Sheet1 › C         ▾]    │ │        ╱╲           ╱╲       │ │
│                                       │ │       ╱▓▓╲         ╱▒▒╲      │ │
│ Ranges     drag on the preview        │ │  ____╱▓▓▓▓╲_______╱▒▒▒▒╲___  │ │
│ ┌────┬──────────┬──────────┬────────┐ │ │      |  1  |     |  2   |    │ │
│ │ #  │ From     │ To       │ Area   │ │ └──────────────────────────────┘ │
│ │ 1  │ 455.0    │ 505.0    │ 8 412  │ │                                  │
│ │ 2  │ 590.0    │ 650.0    │ 4 377  │ │                                  │
│ └────┴──────────┴──────────┴────────┘ │                                  │
│ Unit       nm·counts (X × Y, as is)   │                                  │
│ Under      (•) Zero  ( ) Straight line│                                  │
│            between range ends         │                                  │
│ Output                                │                                  │
│ [ ] Cumulative integral column        │                                  │
│ [ ] Normalized column (area = 1)      │                                  │
│ Values  ( ) Show  (•) Keep in Results │                                  │
```

L'unité de l'aire est le produit des unités des colonnes, sans conversion
(décidé), écrit seulement si les deux unités sont renseignées.

### a. Plusieurs plages dans un même calcul

| Option | Effet | Coût |
|---|---|---|
| A — Une plage par calcul | simple ; trois bandes = trois calculs | réf. |
| **B ★ — Plusieurs plages, un tableau** | les aires d'un spectre d'émission ensemble, une seule ligne de résultat, rapports d'aires faciles | +0,5 j |

### b. Sous la courbe

**★ `Zero` ou `Straight line between range ends`**, plus « une ligne de base
existante » si la série est déjà une colonne corrigée (elle l'est alors de
fait). Alternative : `Zero` seul (−0,3 j), la ligne de base passant toujours
par M10.

---

## M13 — Ajustement (#188)

```
┌ Fit ────────────────────────────────────────────────────────────────── ✕ ┐
│ Input                                    │ ┌───────────────────────────┐ │
│ Series   [ Sheet1 › C (+19)       ▾]     │ │ ●                         │ │
│ X range  (•) All ( ) From … ( ) Zoom     │ │  ●●                       │ │
│ Weight   [ None / Sheet1 › D (yErr) ▾]   │ │    ●●●___   données ●     │ │
│                                          │ │          ‾‾‾───────  fit  │ │
│ Model                                    │ ├───────────────────────────┤ │
│ ┌─────────────────────┐                  │ │ résidus  ·  ·· · ·  · ·   │ │
│ │ Linear              │ y = y0 + A e^-x/τ│ └───────────────────────────┘ │
│ │ Polynomial          │                  │                               │
│ │▌Exponential decay   │ Terms  [ 1 ▾]    │                               │
│ │ Exponential growth  │ [x] Offset y0    │                               │
│ │ Logarithm           │                  │                               │
│ │ Power               │                  │                               │
│ │ Gaussian            │                  │                               │
│ │ Lorentzian          │                  │                               │
│ └─────────────────────┘                  │                               │
│ Parameters                       [ Auto ]│                               │
│ ┌──────┬──────────┬───────┬──────┬──────┬────────────┐                   │
│ │ Name │ Initial  │ Fixed │ Min  │ Max  │ Fitted     │                   │
│ │ y0   │ 12       │ [ ]   │      │      │ 12.4 ± 0.8 │                   │
│ │ A1   │ 9 800    │ [ ]   │ 0    │      │ 9 831 ± 41 │                   │
│ │ τ1   │ 4        │ [ ]   │ 0    │      │ 4.21 ± 0.03│                   │
│ └──────┴──────────┴───────┴──────┴──────┴────────────┘                   │
│ R² 0.9981 · Reduced χ² 1.07 · converged in 7 iterations                  │
│ Output  [x] Fitted column [x] Residuals column [x] Add to graph …        │
```

La formule du modèle est composée par Typst (comme les titres). Le calcul se
relance à chaque changement (aperçu), `Auto` réestime les valeurs initiales.

### a. Modèle libre saisi par l'utilisateur en v1

| Option | Effet | Coût |
|---|---|---|
| **A ★ — Non en v1** | les modèles listés ; le cas libre passe par `pyst` + scipy (#191) | réf. |
| B — Oui | champ « y = … » ; il faut un analyseur d'expressions avec fonctions (`exp`, `ln`, `sqrt`…), absent aujourd'hui (`formula.rs` ne fait que `+ - * /`), et des dérivées numériques | +3 à 4 j |

### b. Ajustement global (plusieurs séries, paramètres partagés)

**★ Plus tard**, dans un sous-ticket à part. Alternative : en v1, une colonne
`Shared` dans le tableau des paramètres (+3 j).

---

## M14 — Décomposition en pics (#189)

```
┌ Decompose Peaks ────────────────────────────────────────────────────── ✕ ┐
│ Input   [ Sheet1 › C          ▾]         │ ┌───────────────────────────┐ │
│ Shape   [ Gaussian            ▾]         │ │      ╱╲      ╱‾╲          │ │
│ Peaks   [ Find ]  Prominence [ 5 % ]     │ │     ╱  ╲    ╱   ╲         │ │
│         Max. number [ 6 ]                │ │   ▲╱ 1  ╲ ▲╱  2  ╲▲       │ │
│         click to add, right-click        │ │  _╱      ╲╱       ╲╲_     │ │
│         to remove                        │ │  composantes ┄┄ somme ──  │ │
│ Baseline (•) Constant ( ) Linear         │ └───────────────────────────┘ │
│          ( ) None (already corrected)    │                               │
│ [ ] Same width for all peaks             │                               │
│ ┌───┬──────────┬──────────┬──────────┬──────────┐                        │
│ │ # │ Position │ Height   │ FWHM     │ Area     │                        │
│ │ 1 │ 478.2 ±… │ 1 204 ±… │ 21.3 ±…  │ 27 310   │                        │
│ │ 2 │ 612.0 ±… │   655 ±… │ 38.9 ±…  │ 27 120   │                        │
│ │ 3 │ 640.5 ±… │   210 ±… │ 15.0 ±…  │  3 350   │                        │
│ └───┴──────────┴──────────┴──────────┴──────────┘                        │
│ Output  [x] One column per peak  [x] Sum  [ ] Residuals  [x] Add to graph│
```

Les composantes ajoutées au graphe forment un groupe de séries (#177),
recolorées par un nuancier.

### a. Détection automatique

| Option | Effet | Coût |
|---|---|---|
| A — Nombre de pics imposé | prévisible, mais il faut connaître la réponse | réf. |
| B — Seuil de hauteur relative | simple, sensible à la ligne de base | réf. |
| **C ★ — Proéminence relative + nombre maximal** | robuste au fond qui monte ; le maximum évite 40 pics sur un spectre bruité | +0,3 j |

### b. Ligne de base

| Option | Effet | Coût |
|---|---|---|
| A — Toujours soustraite avant (par M10) | un seul chemin, mais deux calculs à enchaîner | réf. |
| B — Toujours dans l'ajustement | un geste, mais pas de forme libre | réf. |
| **C ★ — `Constant`, `Linear` dans l'ajustement, ou `None` pour une série déjà corrigée** | couvre les deux usages | +0,3 j |

---

## M15 — Déconvolution par une réponse mesurée (#190)

```
┌ Deconvolve by IRF ──────────────────────────────────────────────────── ✕ ┐
│ Input     [ Decays › B          ▾]       │ ┌───────────────────────────┐ │
│ IRF       [ IRF › B             ▾]       │ │ IRF ┄┄   déclin ●  fit ── │ │
│ Method    (•) Reconvolution              │ │  ┆╲●                      │ │
│           ( ) Direct deconvolution       │ │  ┆ ●●╲                    │ │
│                                          │ │ ┄┘   ●●●╲___              │ │
│ Reconvolution                            │ │            ‾‾‾●●●───────  │ │
│ Model    [ Exponential decay ▾] Terms [2]│ ├───────────────────────────┤ │
│ Shift     [ 0.00 ] ns  [x] Fit the shift │ │ résidus · ·  ·· · · ·     │ │
│ Background [x] Fit constant background   │ └───────────────────────────┘ │
│ Parameters (tableau comme M13)           │                               │
│                                          │                               │
│ Direct deconvolution (si choisie)        │                               │
│ Algorithm [ Wiener              ▾]       │                               │
│ Noise level [ Auto ]  ──●────────        │                               │
```

Décidé : les deux méthodes ; décalage ajustable avec une case (montrée).

### a. IRF et signal sur des grilles X différentes

| Option | Effet | Coût |
|---|---|---|
| **A ★ — IRF interpolée sur la grille du signal**, avertissement si les pas diffèrent de plus de 1 % | marche avec des fichiers d'appareils différents | réf. |
| B — Refus si les grilles diffèrent | aucune interpolation cachée ; force à rééchantillonner avant | réf. |

### b. Déconvolution directe : méthode par défaut et réglage exposé

| Option | Réglage montré | Pour | Contre |
|---|---|---|---|
| **A ★ — Wiener par défaut, Richardson-Lucy au choix** | un niveau de bruit (`Auto` estimé, curseur) ; pour Richardson-Lucy, un nombre d'itérations | immédiat, un seul réglage | Wiener peut donner des valeurs négatives (signalées) |
| B — Richardson-Lucy par défaut | nombre d'itérations | reste positif, adapté aux comptages | plus lent, l'arrêt est empirique |
| C — Wiener seul en v1 | niveau de bruit | −1 j | pas d'alternative positive |

---

## Termes nouveaux (à faire valider avec `docs/textes-interface.md`)

Les termes déjà réservés (†) servent tels quels : *Baseline*, *Peak*,
*Peak position*, *Height*, *FWHM*, *Area*, *Normalize*, *Integrate*, *Fit*.
À ajouter au glossaire au lancement, traduction proposée :

| Anglais | Français | Note |
|---|---|---|
| Analysis | Analyse | menu |
| Results | Résultats | volet sous la grille |
| Calculation | Calcul | une entrée du volet ; « Edit Calculation… », « Detach from Calculation » |
| Out of date | Pas à jour | état grisé (M9) |
| Recalculate | Recalculer | |
| Anchor point | Point d'ancrage | ligne de base par points |
| Zone | Zone | plage de fond d'une ligne de base (≠ *Range*, plage d'axe ou d'intégration) |
| Reference | Référence | colonne de correction |
| Instrument response (IRF) | Réponse instrumentale (IRF) | |
| Reconvolution / Deconvolve | Reconvolution / Déconvoluer | |
| Residuals | Résidus | |
| Fitted | Ajusté | colonne et valeur ajustées ; *Series* reste le mot de la donnée tracée (pas *curve*) |
| Std. error | Erreur type | colonne « ± » |
| Prominence | Proéminence | détection des pics |
| Shift | Décalage | IRF/signal ; à distinguer de *Offset (stack)* |

---

## Récapitulatif des choix

| # | Sujet | ★ recommandé | **Choisi (2026-10-09)** | Coût du choix en plus du socle |
|---|---|---|---|---|
| M1 | Menu Analysis | A une entrée par opération | **B** sous-menus par famille | +0,5 j |
| M2 | Clic droit | A sous-menu `Analysis ▸` | **A** | 0 |
| M3 | Fenêtre et souris | A modale avec aperçu cliquable | **A**, aperçu aussi support d'aides visuelles par opération | +1,5 j |
| M4 a | Outil de souris | A outils explicites | **A + B**, repli sur A si conflits | +0,2 j |
| M4 b | Accroche d'un point | A aimant + moyenne en option | **C** libre, Maj = aimant + moyenne ; Maj + molette règle la sensibilité (info-bulle) ; champ classique aussi | +0,6 j |
| M5 | Montrer / garder | A radio mémorisé | **A** + défaut dans Preferences (#148) | +0,3 j |
| M6 | Nom long produit | A source + suffixe | **A** | 0 |
| M7 a | Contenu du volet | A liste + détail | **A** | 0 |
| M7 b | Position du volet | P1 au-dessus des onglets | **P1** | 0 |
| M7 c | Voir un lot | A bascule `This sheet / Workbook` | **A** | +1 j |
| M8 | Supprimer | C au choix | **C**, boutons `Delete` / `Delete with Columns` / `Cancel` | +0,3 j |
| M9 a | Grisé | A toute la colonne | **A** + séries grisées + marqueur triangle « ! » ; confirmation à l'export ; avertissement désactivable par graphe | +1,5 j |
| M9 b | Calcul lourd | C case, défaut par opération | **C** | +0,2 j |
| M9 c | Relancer à la main | partout | **partout** | 0 |
| M9 d | Saisie à la main | A refusée, « Detach » proposé | **A** | 0 |
| M10 a | Poignées après création | B quand sélectionné | **B** | 0 |
| M10 b | Ligne par points | segments ou spline au choix | **au choix**, segments par défaut | 0 |
| M10 c | Poignées pour bornes et pics | oui | **oui** | 0 |
| M11 | Hors de la référence | A vide, compté | **A** | 0 |
| M12 a | Plages d'intégration | B plusieurs | **B** | +0,5 j |
| M12 b | Sous la courbe | zéro ou droite entre bornes | **zéro ou droite** | 0 |
| M13 a | Modèle libre v1 | A non (scipy) | **B oui** | +3 à 4 j |
| M13 b | Ajustement global | plus tard | **v1** | +3 j |
| M14 a | Détection des pics | C proéminence + max | **C** | +0,3 j |
| M14 b | Ligne de base des pics | C constante, linéaire ou aucune | **C** | +0,3 j |
| M15 a | Grilles IRF/signal | A interpolation + avertissement | **A** | 0 |
| M15 b | Directe par défaut | A Wiener, R-L au choix | **B** Richardson-Lucy, Wiener en option | 0 |

Total des choix au-dessus du socle chiffré dans les sous-tickets :
**≈ 13 à 15 j** (≈ 4,4 j pour les seuls ★).

Les choix sont reportés dans les sous-tickets concernés (section
« Décisions ») ; ce rapport garde les maquettes de référence et pourra être
archivé à la fin du chantier.
