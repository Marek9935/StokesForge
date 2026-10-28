# Interface texts — glossary and writing rules

> **Approved on 2026-10-01** (issue #40). The choices made are recorded in
> section 4.

The interface is **written in English first**; French is its translation
(#39). The demo project (`crates/stokesforge-app/src/demo/`) is the exception: its
names and texts are hard-coded English, outside Fluent and the `fr` catalog (#379). This guide is the single source of terms and rules: a visible text that
departs from it is a defect. The paths `app.rs:N` refer to
`crates/stokesforge-app/src/` on 2026-10-01, before `app.rs` was split into
modules (#115): the code of each surface has since lived in `src/app/` (module
→ role table in `docs/application/shell-and-code.md`). Since #263, the state modules and the
`i18n/` catalogs live in `stokesforge-app-state` (the `state.rs` line numbers
cited below refer to its contents at the time).

"Ref." column: **O** Origin usage · **G** generic term of desktop software ·
**I** IUPAC · **D** common usage of the discipline · **—** specific to
StokesForge. A `†` marks a **reserved** term: it is not yet in the interface,
but its word is already fixed.

## 1. Rules

> These `R1`–`R16` are the rules for **writing texts**. They have nothing to do with the codes `S-R1`–`S-R16` of the behavior standard ([`interface-standard.md`](interface-standard.md), rule index), which carry the `S-` prefix.

| # | Rule |
|---|---|
| R1 | **English first.** French translates English, never the reverse. |
| R2 | **One notion = one word.** The glossary is authoritative; a refused term appears nowhere (label, tooltip, message, key name). |
| R3 | **Title Case**: menus, commands, buttons, tabs, window titles. **Sentence case**: fields, checkboxes, options (radio buttons, lists), tooltips, messages. Linking words in lowercase in Title Case (*of, and, to, in*). French: capital on the first word only, never Title Case. |
| R4 | **Command = verb** (*Add Column*, *Export Graph…*). "…" only if a window still asks for something. An entry that opens a settings page carries the name of the page (*Axes…*). |
| R5 | **Visibility = "Show X"** (*Show legend*), never « X affichée » nor « Présente dans… ». |
| R6 | **Units.** Interface field: unit in parentheses, SI symbol, after a space (*Line width (pt)*). **Axis title: quantity / unit** (IUPAC), symbol or name: *ν̃ / cm⁻¹*, *Wavenumber / cm⁻¹*; a title taken from a column is written *Long name / Units*. Superscripts in Unicode in the interface (`cm⁻¹`); in an axis title (Typst), `$"cm"^(-1)$`. |
| R7 | **Short label, precise tooltip.** Label: 1 to 3 words. Tooltip: the effect **in the 3rd person** ("Adds a workbook", never "Add a workbook"), the default value if it matters, the shortcut; never a repetition of the label. A gesture is written as a gesture ("Right-click to remove"). |
| R8 | **No final period** on a single sentence. Period between two sentences. |
| R9 | **Message = what happened, then what to do.** No "please", no apology, no exclamation mark. Never internal jargon: *session, node, tree, id, thread, spec, command, history entry, token, recipe*. Never `{err}`: the cause of an `io::Error` is put into a sentence by `errors::io` and enters the message as a variable (`{ $cause }`). |
| R10 | **No "(s)"**: plural and agreement are handled by Fluent. |
| R11 | **No colon** after a field label. |
| R12 | **Typography.** "…" (one character), "×" and not "x". Quotation marks « » in French, “ ” in English. Non-breaking space before `: ; ? !` in French. |
| R13 | **French: infinitive** for instructions («Choisir un fichier»). No informal "tu", no "vous". |
| R14 | **Dialog buttons**: the verb of the action (*Delete*, *Replace*, *Discard Changes*); a confirmation is Cancel + verb, never *Keep* alone (#229). *OK* / *Cancel* only to confirm / abandon. Never *Yes* / *No*. Wizard: *Back*, *Next*, *Finish*. |
| R15 | **Names chosen by the user or by default** (*Book1*, *Sheet1*, column names): never translated. Without a name: *Untitled* / *Sans titre*. |
| R16 | **Fluent keys**: `zone-element-role` (`menu-file-open`, `plot-axes-tick-length`, `import-step-header-title`). |

## 2. Glossary

Before proposing a synonym or a new term, reread § 4 Decisions: a term already decided (for example *Sheet* everywhere, Q4) is not discussed again, and an exception to "one notion = one word" is reported to the user.

### 2.1 Project and windows

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Project | Projet | Contents of a `.stf` file | Document, Espace de travail | O |
| Project Manager | Gestionnaire de projet | Panel that shows the project tree | Explorateur, Navigateur | O (Q15) |
| Folder | Dossier | Container of objects in the project | Répertoire | O |
| Workbook | Classeur | Data window that contains sheets | Book, Cahier, Tableur | O |
| Sheet | Feuille | Grid of columns of a workbook | Worksheet, Onglet, Table, Feuille de calcul | O (Q4) |
| Matrix | Matrice | Data window in a regular grid | Tableau | O |
| Graph | Graphique | Window that plots series | Graphe, Chart, Figure, Plot | O (Q1, Q2) |
| Note | Note | Free text window | Mémo | G |
| Window | Fenêtre | Open workbook, graph, matrix or note | Onglet, Vue | O |
| Detach / Reattach | Détacher / Réattacher | Take a window out of the main frame / put it back in | Flotter, Désancrer, Dock | G |
| Restore | Restaurer | Reopen a closed or minimized window | Rétablir | G |
| Status bar | Barre d'état | Bottom line: last message | Barre de statut | G |
| Script | Script | Python program run on the project | Macro, Programme | — |
| Run / Stop | Exécuter / Arrêter | Start / interrupt a script | Lancer, Interrompre, Kill | G |
| Output | Sortie | Text written by a script | Résultat, Log | G |
| Traceback | Trace d'appels | Call stack of a Python exception | Backtrace, Pile | D |

### 2.2 Application

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| New Project | Nouveau projet | Replace the current project with an empty project, after confirmation if needed | Nouvelle session, Nouveau document | #73 |
| Discard Changes | Abandonner les modifications | Confirm the loss of unsaved work (New Project, Open…, Quit); next to Cancel and Save | Supprimer le projet, Effacer | #73, #229 |
| Open / Save / Save As / Quit | Ouvrir / Enregistrer / Enregistrer sous / Quitter | File commands | Sauvegarder, Fermer (pour Quitter) | G |
| Undo / Redo | Annuler / Rétablir | Undo / redo the last action | Refaire, Répéter | G |
| Import / Export | Importer / Exporter | Read an instrument file / write a graph | Charger, Sauvegarder | O |
| Refresh | Actualiser | Reread the source file of a column | Rafraîchir, Recharger | G |
| Properties | Propriétés | Settings of a specific object | Options, Paramètres | O |
| Preferences† | Préférences | Application settings | Options, Paramètres | G |
| Expand by Default | Déplier par défaut | Initial collapse state of new Object Manager groups; does not change groups already seen | Tout déplier | #139 |
| Objects | Objets | Section of the texts and shapes of the graph or of a layer in the Object Manager | Éléments | #139 |
| Setting | Réglage | Value chosen by the user | Option, Paramètre | G |
| Template | Modèle | Import settings saved under a name | Recette, Preset, Filtre | O |
| Wizard | Assistant | Window with steps | Wizard, Guide | G |
| Theme | Thème | Set of global colors of a graph | Style, Palette | O |
| Shortcut | Raccourci | Keys that run a command | Raccourci clavier | G |

### 2.3 Workbook and grid

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Column | Colonne | Series of values of one type | Champ, Variable | O |
| Row | Ligne | Row of cells of a sheet | Enregistrement | O |
| x' (x prime) | x' | Numeric value specific to a column (special header row), abscissa of a "line" series; is not translated | X2, Temps, Instant (le temps n'est qu'un cas d'usage) | — |
| Line | Ligne | Row of text **of a file** (import) | Row | G |
| Cell | Cellule | One value in a column and a row | Case | G |
| Selection | Sélection | Chosen cells, columns or rows | Plage | G |
| Long Name | Nom long | Description of a column, shown in the header | Libellé, Description | O |
| Units | Unités | Unit of the values of a column, shown in the header | Unité (singulier) | O (Q13) |
| Comments | Commentaires | Free text of a column | Remarques, Notes | O |
| Metadata | Métadonnées | Key-value pairs read from the imported file | Infos, En-tête du fichier | O |
| Role | Rôle | Function of a column in a plot | Désignation, Type | O (Q8) |
| X / Y / Z | X / Y / Z | Roles: abscissa, ordinate, third axis | — | O |
| X Error / Y Error | Erreur X / Erreur Y | Roles: error bar in X or in Y | xerr, yerr | O |
| Label | Étiquette | Role: text attached to each point | Libellé, Annotation | O (Q11) |
| None | Aucun | Role: column left out of the plot | Ignorer, Disregard | O |
| Type | Type | Nature of a column: Numeric, Text, Mixed, Date/Time | Format | O |
| Numeric / Text / Mixed / Date/Time | Numérique / Texte / Mixte / Date et heure | Values of the types | Nombres, Date-heure | O |
| Cut / Copy / Paste | Couper / Copier / Coller | Clipboard | — | G |
| Paste in Place | Coller à l'identique | Paste at the original position of the copy | Coller en absolu | G |
| Copy with Header | Copier avec l'en-tête | Copy the values and the column header | — | O |
| Clipboard | Presse-papiers | System copy memory | Presse-papier, Tampon | G |
| Add | Ajouter | Create an object **at the end** (column, sheet, text, folder) | Nouveau, Créer | O |
| Insert | Insérer | Create **at a position** (rows) | Ajouter | G |
| Delete | Supprimer | Destroy an object, rows or cells | Effacer, Enlever | G |
| Delete and Shift Up | Supprimer et remonter | Delete cells and shift up the ones below | Supprimer et décaler | G |
| Clear | Effacer | Empty cells, without deleting them | Vider | G |
| Remove | Retirer | Take an element out of a list, without destroying it | Supprimer | G |
| Rename / Move | Renommer / Déplacer | Change the name / the place of an object | — | G |
| Move to First / Move Left / Move Right / Move to Last | Déplacer au début / Déplacer vers la gauche / Déplacer vers la droite / Déplacer à la fin | Entries of the **Move ▸** (Déplacer ▸) submenu of a column; the history says « Move “B” » (one column) or « Move Columns » / « Déplacer des colonnes » (several) (#319) | Avancer, Reculer | O |
| Edit | Modifier | Change the contents or settings of an existing object (history labels); *Change* only in « Change Type of Column A » | Éditer, Changer | G |
| Show / Hide | Afficher / Masquer | Make visible / invisible | Cacher | G |
| Open / Close (Project Manager window) | Ouvrir / Fermer | Opens or activates / closes the window, like the cross (#327) | Afficher / Masquer | G |
| Grid | Grille | Cells of a sheet (never the gridlines of a graph) | — | G |

### 2.4 Import

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Import Wizard | Assistant d'import | Six-step window that sets up the import | Importateur | O |
| Delimiter | Séparateur | Character that separates the values of a line | Délimiteur | O |
| Decimal separator | Séparateur décimal | Character between the integer and decimal parts | Virgule (pour le nom) | O |
| Thousands separator | Séparateur de milliers | Character that groups digits | Milliers | O |
| Date order | Ordre des dates | Day first or month first for `03/04/2026` | Format de date | O |
| Reset Width / Reset Column Widths | Réinitialiser la largeur / Réinitialiser les largeurs | Resets a column (selection) or all columns of a sheet to the default width (#330) | Largeur auto, Ajuster | G |
| Fixed width | Largeur fixe | Columns of a given width, without a delimiter | Colonnes fixes | O |
| Encoding | Encodage | Character set of the file | Codage, Charset | G |
| Preview | Aperçu | Contents of the file as read | Prévisualisation | O |
| Header | En-tête | Lines at the start of the file, before the data | Entête | O |
| Subheader | Sous-en-tête | Header lines that give long name, units, comments | — | O |
| Footer | Pied de page | Lines after the data | Fin | G |
| Comment line | Ligne de commentaire | Line ignored because of its prefix | — | G |
| Skipped line | Ligne ignorée | Line that the import does not use | — | G |
| Block | Bloc | Series of data in a file, separated from the others | Section | O |
| Missing values | Valeurs manquantes | Texts read as an empty cell | Jetons, Nulls | G |
| Source file | Fichier source | File a column comes from | Origine | O |
| Header key | Clé d'en-tête | Name of a header metadata item | — | — |
| Destination | Destination | Where the import writes its columns | Cible | O |

### 2.5 Graph

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Graph Properties | Propriétés du graphique | Settings window of a graph | Propriétés du graphe | O |
| General | Général | Tab: title and background | Graph, Graphe | G (Q23) |
| Plot Area | Zone de tracé | Rectangle bounded by the axes | Zone de dessin, Plot | G |
| Layer | Calque | Plot area with its axes; a graph contains one or more | Panel, Panneau, Couche | O (Q22) |
| Title | Titre | Text at the top of the graph | Intitulé | G |
| Axis (pl. Axes) | Axe | Graduated line carrying a quantity | — | G |
| X (bottom) / Y (left) / Secondary Y (right) | X (bas) / Y (gauche) / Y secondaire (droite) | The three axes of an area | Y2, Axe de droite | G |
| Axis title | Titre de l'axe | Text that names the quantity of the axis | Libellé, Label | G |
| Tick | Graduation | Mark on the axis | Repère, Coche | I |
| Tick label | Étiquette de graduation | Value written next to a tick | Libellé de graduation | G (Q11) |
| Gridlines | Quadrillage | Lines that extend the ticks | Grille | G (Q12) |
| Scale | Échelle | Law of distribution of values on the axis | Mode | O |
| Linear / Log scale | Linéaire / Échelle log | The two current scales (Log10) | Logarithmique | O |
| Reciprocal† | Inverse | 1/x scale (spectra in wavenumber) | Réciproque | O |
| Range | Plage | Interval of values shown on the axis | Bornes, Étendue | O |
| Auto range | Plage auto | Range computed from the data; **no checkbox** since #344: « De » and « À » follow the data as long as nothing has been typed, and the « Auto » button of the row brings them back to it | Bornes automatiques | O |
| Min / Max | Min / Max | Ends of the range | De / À | O |
| Reverse axis | Axe inversé | Values decreasing in the reading direction; **no checkbox** since #344: « De » > « À » reverses the axis | Axe retourné | O |
| Legend | Légende | List of the series with their style | — | G |
| Broken reference | Référence rompue | A series whose Y column no longer exists; kept in the legend, annotated « (broken reference) » / « (référence rompue) » (`graph-series-broken-reference*`, #369) | Lien cassé, Référence invalide | G |
| Legend entry | Entrée de légende | One line of the legend | Libellé de légende | G |
| Position | Position | Corner of the legend: Top left…; Reset Position / «Réinitialiser la position»: puts a title or the legend back at its default place | Emplacement | G |
| Series | Série | Plotted data of a Y column | Courbe, Trace, Plot, Jeu de données | G (Q3) |
| Series Group | Groupe de séries | Series added together (or gathered by hand) that are set together; collapsible node of the Object Manager and of the series list, named "Group n" until it is renamed (#177, #197) | Groupe de courbes | G |
| Ungroup | Dissoudre le groupe | Each series of the group becomes a single series, style kept; deletes nothing (#197). Not to be confused with « Delete Group » / « Supprimer le groupe », which also removes the series | Dégrouper, Éclater | G |
| Delete Group | Supprimer le groupe | Removes the group and all its series from the graph (#197); in menus, it is « Delete » / « Supprimer » applied to the group | Effacer le groupe | G |
| Select Members | Sélectionner les membres | Selects the group: the handles of all its series are shown (#197) | Sélectionner le groupe | G |
| Series (Object Manager) | Séries | The node under a layer that lists its series; formerly « Plot Group » / « Groupe de tracés », renamed; do not call it a group (#177) | Plot Group, Groupe de tracés | G |
| Whole group / This series only | Tout le groupe / Cette série seulement | Scope of the settings of a series that is in a group; the settings specific to one series are greyed out in "group" scope (#177) | Appliquer à tous | G |
| Leave Group | Quitter le groupe | Detaches the series from its group, style kept (#177) | Dissocier, Sortir | G |
| Color gradient | Nuancier | Gradient of colors spread over the series of a group: Blue to Red / Bleu vers Rouge… (#178; the arrow → is not in the interface font: a square is shown in its place, do not use it); can be saved in the project or for the user, with its name | Palette (qualitative, sans ordre), Dégradé | G |
| Plot type | Type de tracé | Way of plotting a series | Mode | O |
| Scatter | Points | Symbols only | Nuage de points | O (Q6) |
| Line | Ligne | Line only | Courbe | O |
| Line + Symbol | Ligne + symboles | Line and symbols | Ligne + points | O |
| Bar | Barres | Vertical bars | Colonnes | O (Q7) |
| Histogram | Histogramme | Bars of the counts per class | — | G |
| Line + Error Bars | Ligne + barres d'erreur | Line with error bars | — | G |
| Line (stroke) | Trait | Style, width and color of the plot line | Ligne | O |
| Line style | Style de trait | None, Solid, Dashed, Dotted, Dash-dot | Plein | G |
| Line width (pt) | Épaisseur (pt) | Width of the line | Largeur | O |
| Symbol | Symbole | Figure placed on each point | Marqueur, Point | O (Q5) |
| Shape / Size (pt) | Forme / Taille (pt) | Figure of the symbol / its dimension | — | O |
| Circle, Square, Triangle, Diamond, Plus, Cross | Cercle, Carré, Triangle, Losange, Plus, Croix | Symbol shapes | Disque, Rond | G (Q24) |
| Fill† | Remplissage | Color of the inside of a shape | Fond | O |
| Color | Couleur | Hue of an element | — | G |
| Opacity | Opacité | Share of opaque color (0-100 %) | Transparence, Alpha | G |
| Background | Fond | Color behind the graph | Arrière-plan | G |
| Auto | Auto | Value chosen by StokesForge from the context | Automatique, Défaut | G (Q9) |
| Text | Texte | Free text anchored to the data, placed on the graph | Annotation, Texte libre | O (Q21) |
| Font size (pt) / Rotation (°) | Taille (pt) / Rotation (°) | Settings of a text | Angle | G |
| Alignment (Left / Center / Right) | Alignement (Gauche / Centré / Droite) | Place of the text in a box of fixed width (#244) | Justification | G |
| Object | Objet | Rectangle, ellipse, line or arrow placed on the graph, anchored to the data ("shape" is reserved for the symbol) | Forme, Dessin, Shape | O (#76) |
| Bring to Front / Send to Back | Amener devant / Envoyer derrière | Stacking order of the objects | Premier plan, Arrière-plan (réservé au fond) | O (#76) |
| Formula | Formule | Computation of a column (`F(x)=`), as in Excel and Origin | Équation, Expression | #322 |
| Math | Maths | Text typeset by Typst, between `$…$` ("math mode") | Formula, Équation, LaTeX | #322 |
| Model expression | Expression de modèle | What the user writes for a custom fit | Formula, Équation | #322 |
| Zoom / Reset Zoom | Zoom / Réinitialiser le zoom | Framing of the view; return to the original ranges | Rescale, Recadrer | G |
| Pan† | Panoramique | Move the framing | Déplacement | G |
| Global colors | Couleurs globales | Background, text, axes of a theme | — | — |
| Palette | Palette | Set of colors offered | — | G |
| Swatch | Nuance | One color of a palette | Échantillon | G |
| Saved colors | Couleurs enregistrées | Colors kept in the project or for the user | Favoris | — |
| Hex / RGB / HSL | Hexa / RVB / TSL | Ways of writing a color | — | G |
| Resolution | Résolution | Scale factor of a PNG export | DPI | G |
| Outlines | Contours | Text of an exported SVG drawn as shapes rather than as text (#162) | Courbes, Tracés | G |
| #REF | #REF | What a column formula shows in place of a column it reads that is no longer in its sheet (deleted, or moved); keeps the spreadsheet abbreviation (#321) | — | — |

### 2.6 Spectroscopy

The glossary gives the words for the **axis titles, units and processing**
that the interface will have to write. Quantities and units: IUPAC *Green
Book*; a dimensionless quantity (absorbance) carries no unit. Axis title:
**quantity / unit** (R6), given last in the definition.

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Spectrum | Spectre | Intensity measured as a function of a spectral abscissa | Courbe, Trace | I |
| Wavenumber | Nombre d'onde | Inverse of the wavelength · *ν̃ / cm⁻¹* | Fréquence (cm⁻¹) | I |
| Wavelength | Longueur d'onde | Distance between two maxima of the field · *λ / nm* | — | I |
| Raman shift | Déplacement Raman | Wavenumber difference from the exciting line · *Δν̃ / cm⁻¹* | Décalage Raman, Shift | I, D (Q16) |
| Energy | Énergie | Energy of the photons · *E / eV* | — | I |
| Frequency | Fréquence | Number of oscillations per second · *ν / Hz* | — | I |
| Chemical shift† | Déplacement chimique | NMR abscissa · *δ / ppm* | Shift | I |
| Absorbance | Absorbance | Unitless quantity, A = lg(P₀/P) · *A* | Densité optique, OD, Extinction | I (Q17) |
| Transmittance | Transmittance | Share of light transmitted · *T / %* | Transmission | I |
| Reflectance | Réflectance | Share of light reflected · *R / %* | Réflexion | I |
| Intensity | Intensité | Signal in arbitrary units · *I / a.u.* (French *u.a.*) | Intensité (arb. units) | D |
| Counts | Coups | Number of detections | Comptes | D (Q18) |
| Molar absorption coefficient† | Coefficient d'absorption molaire | Absorbance scaled to 1 mol L⁻¹ and 1 cm · *ε / (L mol⁻¹ cm⁻¹)* | Coefficient d'extinction | I |
| Sample / Blank / Reference | Échantillon / Blanc / Référence | Measurement under study / without analyte / for comparison | Témoin | D |
| New column | Nouvelle colonne | Analysis output separate from the source | Colonne calculée (pour le choix de destination) | D (#212) |
| Replace original data | Remplacer les données originales | Final correction of the source column | Écraser | D (#212) |
| Modified data | Données modifiées | Trace of a final correction | Périmé | D (#212) |
| Baseline† | Ligne de base | Background signal under the peaks | Fond continu | I |
| Correction† | Correction | Analysis type: a computed baseline (constant, line, polynomial, anchor points), subtracted or divided (#211) | Baseline seul, Mathematical Correction | D |
| Reference† | Référence | Analysis type: a reference column measured separately, interpolated on the X of the data, divided, multiplied or subtracted; its menu entry is « Reference Correction… » | Correction (seul), Estimated Baseline | D |
| Peak† | Pic | Local maximum of the signal | Bande (sauf IR/Raman : *band*) | I |
| Peak position† | Position du pic | Abscissa of the maximum | Centre | D |
| Height† | Hauteur | Ordinate of the maximum, baseline removed | Amplitude | D |
| FWHM† | LMH | Width at half height of a peak | Largeur | I |
| Area† | Aire | Integral of a peak | Surface | D |
| Normalize† | Normaliser | Scale the intensity to 1 at the maximum or at the area | Standardiser | D |
| Smooth† | Lisser | Reduce the noise of a spectrum | Filtrer | D |
| Derivative† | Dérivée | Slope of the spectrum, of order 1 or 2 | Différentielle | D |
| Integrate† | Intégrer | Compute the area under the curve | — | D |
| Data Manipulation† | Manipulation des données | Category: baseline corrections (mathematical or by reference) | — | D (#248) |
| Fitting† | Ajustement | Category: models fitted to the data | — | D (#248) |
| Restore original data | Restaurer les données originales | Gives back the values from before a replacement | Annuler la correction | D (#248) |
| Fit† | Ajuster | Make a model match the data | Fitter, Lisser | D |
| Offset (stack)† | Décalage (empilement) | Vertical translation between overlaid spectra | Espacement | D |

### 2.7 Analysis (#181)

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Analysis | Analyse | Menu and window of the mathematical processing | Traitement | D |
| Results | Résultats | Pane under the grid: kept calculations and their values | Sorties | D |
| Calculation | Calcul | A saved processing, with its output columns | Opération | D |
| Out of date | Pas à jour | Calculation whose input has changed or disappeared | Périmé, Obsolète | D |
| Transposed sheet | Feuille transposée | Sheet « Sheet1ᵀ » that carries the rows of a sheet as columns (x' then « Row N ») in order to analyze them; recomputable snapshot (#253) | Feuille inversée, Pivot | — |
| Relink | Relier de nouveau | Rewrite the linked columns of a transposed sheet whose values have been edited (#253) | Reset, Resynchroniser | — |
| Recalculate | Recalculer | Redo a calculation on the current data | Rafraîchir | D |
| Low R² warning | Avertir quand R² est faible | Checkbox of the calculation: dialog box when the R² of a fit is very low (#288) | Alerte | D |
| Low R² (badge) | R² faible(s) | Status bar badge: number of series with R² < 0.6 in the project; a click lists the types (#290) | Alerte | D |
| Constant (baseline) | Constante (ligne de base) | Flat background through a point or the mean of a zone | Décalage | D (#211) |
| Anchor point | Point d'ancrage | Point placed on the preview to draw a baseline | Ancre | D |
| Smoothing | Lissage | Attenuate the noise of a curve from its neighbors (#324) | Filtrage, Débruitage | D |
| Adjacent averaging | Moyenne adjacente | Smoothing by the mean of the points of the window | Moyenne mobile | D (#324) |
| Savitzky-Golay | Savitzky-Golay | Smoothing by a polynomial fitted on the window | Filtre SG | D (#324) |
| Rolling median | Médiane glissante | Smoothing by the median of the points of the window | Filtre médian | D (#324) |
| Smoothing width | Largeur de lissage | Neighboring points that a smoothing reads (odd number) or width in X units | Fenêtre (déjà prise : fenêtre de l'application) | D (#324) |
| Zone | Zone | Interval of abscissae chosen on the preview | Région, Plage | D |
| Reference | Référence | Column by which a series is corrected | Témoin | D |
| IRF (instrument response) | Réponse de l'instrument | Time response of the instrument, to be deconvolved | — | I |
| Reconvolution | Reconvolution | Fit a model convolved with the instrument response | — | D |
| Deconvolution | Déconvolution | Recover the signal before the instrument response | — | D |
| Residuals | Résidus | Difference between the data and the fit | Erreurs | I |
| Fitted | Ajusté | Value of the fitted model | Fitté | D |
| Std. error | Erreur type | Uncertainty of a fitted parameter | Écart-type, Sigma | I |
| Prominence | Proéminence | Height of a peak above its surroundings | Saillance | D |
| Shift | Décalage | Translation of the response along the abscissa (≠ *chemical shift*) | Shift | D |
| Plateau | Palier | Constant level toward which an exponential settles (≠ *Offset* of the stack) | Décalage, Offset | D |
| Not converged | N'a pas convergé | The fit solver stopped without reaching a minimum (#281); never "plateau" (reserved for the asymptote) | Échec, Plateau | D |
| Analysis type | Type d'analyse | Family of calculation chosen in the analysis window | Opération | D |

## 3. Before / after examples

Real labels, quoted as is. Rule: number from section 1 or glossary term.

<!-- lang: fr columns: Before (`fichier:ligne`), French -->
| # | Before (`fichier:ligne`) | English | French | Rule |
|---|---|---|---|---|
| 1 | « Graphique » › « Propriétés du graphe… » (`app.rs:1282-1284`) ; onglet « Graphe » (`state.rs:1296`) | Graph › Graph Properties… ; tab General | Graphique › Propriétés du graphique… ; onglet Général | R2 |
| 2 | « Réinitialiser le zoom » (`app.rs:1301`) ; « réinitialiser le zoom » (`app.rs:7239`) | Reset Zoom | Réinitialiser le zoom | R3 |
| 3 | « Ajouter un texte », « Supprimer le texte sélectionné » (`app.rs:1314, 1326`) ; « Texte libre » (`state.rs:1355`) | Add Text, Delete Text | Ajouter un texte, Supprimer le texte | R2 |
| 4 | « Exporter le graphe… » + « SVG, PDF ou PNG, avec le zoom courant » (`app.rs:1342-1345`) | Export Graph… + tip “SVG, PDF or PNG, at the current zoom” | Exporter le graphique… + « SVG, PDF ou PNG, au zoom courant » | R4, R7 |
| 5 | « Résolution des exports PNG » (`app.rs:1356`) | PNG Resolution | Résolution PNG | R7 |
| 6 | « Afficher la ligne Metadata des colonnes » (`app.rs:1378`) ; « Long Name », « Units » (`app.rs:5162-5171`) in English in a French interface | Show Metadata Row ; Long Name, Units | Afficher la ligne Métadonnées ; Nom long, Unités | R1, R2 |
| 7 | « Détacher le Project Manager en fenêtre séparée » (`app.rs:1419`) | Detach Project Manager | Détacher le gestionnaire de projet | R7 |
| 8 | « Suivant > », « < Précédent », « Terminer » (`app.rs:6515, 6524, 6506`) | Next, Back, Finish | Suivant, Précédent, Terminer | R14 |
| 9 | « Séparateur décimal : » (`app.rs:5939`) | Decimal separator | Séparateur décimal | R11 |
| 10 | « Valeurs manquantes (séparées par des virgules) : » + « Jetons lus comme une cellule vide : --, n/a, *, NaN… » (`app.rs:5967-5971`) | Missing values + tip “Comma-separated. Read as empty cells: --, n/a, *, NaN…” | Valeurs manquantes + « Séparées par des virgules. Lues comme cellules vides : --, n/a, *, NaN… » | R7, R9 |
| 11 | « Caractères à retirer en tête de chaque ligne de données : » (`app.rs:6042`) | Remove leading characters + tip “Removed at the start of every data line” | Retirer les premiers caractères + « Retirés au début de chaque ligne de données » | R7 |
| 12 | « Lignes d'en-tête : », « dont sous-en-tête : » (`app.rs:6104, 6112`) | Header lines, Subheader lines + tip “Counted in the header lines” | Lignes d'en-tête, Lignes de sous-en-tête + « Comptées dans les lignes d'en-tête » | R7, R11 |
| 13 | « Les données commencent ici », « Cette ligne donne les unités » (`app.rs:5574-5578`) | Start Data Here, Use as Units Line | Commencer les données ici, Utiliser comme ligne d'unités | R4 |
| 14 | « {shown} ligne(s) de données montrée(s) sur {} au total. » (`app.rs:5543`) | Showing {$shown} of {$total} data lines | {$shown} lignes de données sur {$total} | R8, R10 |
| 15 | « Nom des séries : » → « Nom de fichier » / « Clé d'en-tête » (`app.rs:6173-6191`) | Column names → File name / Header key | Nom des colonnes → Nom du fichier / Clé d'en-tête | Column ≠ Series |
| 16 | « Choisis un fichier avant de détecter l'en-tête » (`state.rs:4348`) ; « Choisissez un fichier à l'étape 1. » (`app.rs:6224`) | Choose a file before detecting the header | Choisir un fichier avant de détecter l'en-tête | R13 |
| 17 | « Presse-papier vide ou illisible » (`app.rs:4588`) | Clipboard is empty or unreadable | Presse-papiers vide ou illisible | R9 (spelling) |
| 18 | « Vider » (`app.rs:4609`) ; « Effacer la sélection » (`state.rs:4740`) | Clear ; Clear Selection | Effacer ; Effacer la sélection | R2 |
| 19 | « Supprimer et décaler » + « Suppr vide à la première pression et ne supprime qu'à la seconde ; cette entrée supprime tout de suite. » (`app.rs:4619-4622`) | Delete and Shift Up + tip “Deletes now and shifts cells up. The Del key first clears, then deletes” | Supprimer et remonter + « Supprime tout de suite et remonte les cellules. La touche Suppr efface d'abord, puis supprime » | R7 |
| 20 | « Session injoignable : {err} » (`state.rs:3430`) ; « Le fil de script a disparu sans répondre (a-t-il paniqué ?) » (`state.rs:3453`) | Internal error: {$error} ; The script engine stopped unexpectedly. Run the script again | Erreur interne : {$error} ; Le moteur de script s'est arrêté. Relancer le script | R9 |
| 21 | « {id} n'est pas une feuille de ce projet » (`state.rs:4562`) ; « Impossible de grouper l'effacement de la sélection » (`state.rs:4741`) | This sheet is no longer in the project ; Could not clear the selection | Cette feuille n'est plus dans le projet ; Effacement impossible | R9 |
| 22 | « Aucune colonne de rôle Y dans la sélection : rien à tracer » (`state.rs:5674`) | No Y column selected. Select a Y column to plot | Aucune colonne Y sélectionnée. Sélectionner une colonne Y à tracer | R9 |
| 23 | « Import en cours… 0/{total} fichier(s) » (`state.rs:3645`) ; « {} sélectionné(s) » (`app.rs:4353`) | Importing… {$done} of {$total} files ; {$count} selected | Import en cours… {$done}/{$total} fichiers ; {$count} sélectionné / {$count} sélectionnés (accord par Fluent) | R10 |
| 24 | « Axe affiché », « Légende affichée », « Présente dans la légende » (`app.rs:8451, 8628, 8526`) | Show axis, Show legend, Show in legend | Afficher l'axe, Afficher la légende, Afficher dans la légende | R5 |
| 25 | « Échelle logarithmique », « Axe inversé », « Bornes automatiques » (`app.rs:8416-8428`) | Log scale, Reverse axis, Auto range | Échelle log, Axe inversé, Plage auto | R7 |
| 26 | « Épaisseur (pt) », « Taille (pt) », « Plein » (`app.rs:8563, 8600, 8675`) | Line width (pt), Size (pt), Solid | Épaisseur (pt), Taille (pt), Continu | R6 |
| 27 | « Marqueur », « Forme », « Couleur du marqueur », « Comme le trait » (`app.rs:8574-8615`) | Symbol, Shape, Symbol color, Same as line | Symbole, Forme, Couleur du symbole, Comme le trait | Symbol |
| 28 | « Points », « Ligne + points », « Barres » (`app.rs:8661-8664`) | Scatter, Line + Symbol, Bar | Points, Ligne + symboles, Barres | Symbol |
| 29 | « Automatique (contraste avec le fond) », « Automatique (blanc) » (`app.rs:8158, 8167`) ; « Auto » (`color_picker.rs:389`) | Auto (contrasts with background), Auto (white) | Auto (contraste avec le fond), Auto (blanc) | R2 |
| 30 | « Grille », « Lignes verticales (graduations X) » (`app.rs:8182-8184`) | Gridlines, Vertical (X ticks) | Quadrillage, Verticales (graduations X) | Grid ≠ Gridlines |
| 31 | « Nom vide = celui de la colonne tracée, affiché tel quel. » (`app.rs:8487`) | Empty: uses the plotted column's name as is | Vide : reprend le nom de la colonne tracée tel quel | R7 |
| 32 | « Clic droit : retirer » (`color_picker.rs:559`) | Right-click to remove | Clic droit pour retirer | R7 |
| 33 | « Classeur actif (une feuille par fichier) » (`app.rs:6388`) | Active workbook (one sheet per file) | Classeur actif (une feuille par fichier) | R3 |
| 34 | « Fichiers et modèle », « En-tête et fin des données » (`state.rs:1584-1586`) | Files and Template, Header and Data End | Fichiers et modèle, En-tête et fin des données | R3 |

## 4. Decisions

Approved by the user on 2026-10-01: the recommendation is kept
everywhere, **except Q14 and Q22** where the alternative wins (marked ✔).

| # | Question | Recommendation | Alternative |
|---|---|---|---|
| Q1 | English: **Graph**, **Chart** or **Plot** for the plotting window? | **Graph** (Origin; the code already says `Graph1`) | *Chart* (generic, Excel); *Plot* (matplotlib, but Origin uses it for the series) |
| Q2 | French: **Graphique** or **Graphe**? | **Graphique** (generic office term; « graphe » evokes graph theory) | *Graphe* (Origin's usage in French, word of the current code and docs) |
| Q3 | The plotted data: **Series**, **Plot** (Origin) or **Curve**? | **Series / Série** (generic, never ambiguous with "plot" or with "graph") | *Plot* (Origin); *Curve / Courbe* (spectroscopists' usage, wrong for a scatter plot) |
| Q4 | **Worksheet** everywhere, or **Sheet** (shorter)? | **Sheet** everywhere (decision #94, 2026-10-03: "Worksheet" was confused with the default name `Sheet1`); replaces decision #39 | *Worksheet*: Origin's term for workbooks, longer |
| Q5 | **Symbol** or **Marker**? (French: Symbole or Marqueur) | **Symbol / Symbole** (Origin; consistent with *Line + Symbol*) | *Marker / Marqueur* (matplotlib, Excel; current term of the interface) |
| Q6 | French for *Scatter*: **Points** or **Nuage de points**? | **Points** (one word, already in place) | *Nuage de points* (statistical term, 3 words) |
| Q7 | **Bar** or **Column** for vertical bars? | **Bar / Barres** (*Column* is already "data column") | *Column* (Origin: Column = vertical, Bar = horizontal) |
| Q8 | Function of a column: **Role** or **Designation** (Origin "Set As")? And *None* or *Disregard*? | **Role / Rôle**, value **None** | *Designation*; *Disregard / Ignorer* |
| Q9 | **Auto** or **Automatic**? | **Auto** (short; the tooltip specifies « contraste avec le fond ») | *Automatic / Automatique* written in full |
| Q10 | « Annuler » names both *Undo* and the *Cancel* button of the properties window. Keep it? | **Yes** (common usage; distinct contexts) | *Abandonner* for *Cancel* (clean, but unusual) |
| Q11 | Text attached to a point or to a tick: **Label / «Étiquette»** for the column role and **Tick label / «Étiquette de graduation»**? | **«Étiquette»** for both; « libellé » refused everywhere | «Libellé de graduation»; «Valeur de graduation» |
| Q12 | Lines that extend the ticks: **Gridlines / Quadrillage** or **Grid / Grille**? | **Quadrillage** (« grille » is already the sheet grid) | *Grille* (current term) |
| Q13 | **Units** (Origin's plural) or **Unit**? | **Units / «Unités»** everywhere | *Unit / «Unité»* (singular, current in the import preview and the axis) |
| Q14 | Units: parentheses **Wavenumber (cm⁻¹)** or IUPAC bar **ν̃ / cm⁻¹**? | *Parentheses* (usage of journals and Origin) | ✔ **Fraction bar** (IUPAC) for axis titles; the interface fields keep parentheses (R6) |
| Q15 | Tree panel: **Project Manager** (current name) or **Project Explorer** (Origin's name)? | **Project Manager** (already in the code and docs) | *Project Explorer* |
| Q16 | French for *Raman shift*: **«Déplacement Raman»** or **«Décalage Raman»**? And *Offset* (stack): **«Décalage»**? | **«Déplacement Raman»**; **«Décalage»** reserved for the stack, so as not to confuse them | «Décalage Raman»; «Translation» or «Espacement» for the stack |
| Q17 | **Absorbance** without unit (IUPAC) or **Absorbance (a.u.)** (common usage, Origin templates)? | **Absorbance** alone | *Absorbance (a.u.)* |
| Q18 | French for *Counts*: **Coups** or **Counts** kept? | **Coups** | *Counts* invariant (oral usage in XPS and Raman) |
| Q19 | Informal "tu", "vous" or infinitive for French instructions? | **Infinitive** («Choisir un fichier») | Imperative «vous» («Choisissez un fichier») |
| Q20 | **Detach / Reattach** or **Float / Dock**? | **Detach / Reattach** (verbs already in place, understood without jargon) | *Float / Dock* (Qt, Office) |
| Q21 | Text placed on the graph: **Text** or **Annotation**? | **Text** (one word, the gesture is "Add Text") | *Annotation* (matplotlib, term of the code); *Text Label* (clashes with *Label*) |
| Q22 | Plotted subset of a graph: **Panel** or **Layer** (Origin)? | *Panel / Panneau* | ✔ **Layer / Calque** (Origin) |
| Q23 | First tab of *Graph Properties*: **General** or **Graph**? | **General** ("Graph" repeats the window title) | *Graph* (current name: « Graphe ») |
| Q24 | Symbol shapes: **Plus / Cross** or **Cross / X**? | **Plus (+)** and **Cross (×)** | *Cross (+)* and *X (×)* |
| Q25 | Colon after a field label (« Séparateur : ») — remove them? | **Remove** (R11: shorter, no non-breaking space concern) | *Keep* in French only |

### Object Manager (#81–85)

<!-- lang: fr columns: French -->
| English | French | Definition |
|---|---|---|
| Object Manager | Gestionnaire d'objets | Tree of the contents of the active graph |
| Layer | Calque | A plot panel (`PlotSpec::panels`) |
| Series | Séries | The series of a layer, with their series groups indented (formerly « Plot Group », #177) |
| Graph texts | Textes du graphique | Titles of the graph, of the layers and of the axes, and legend: never listed in the Object Manager (#214): edited by double-click or in the properties |
| Show Axes | Afficher les axes | The axes of a layer (line, ticks, labels), listed in the Object Manager if the "Show Axes" toggle is on (#111, #147); this filter does not change their visibility on the figure; one « X (bottom) Axis » / « Axe X (bas) » row per axis, including Secondary X (top) / X secondaire (haut) |
| Mirror axis | Axe miroir | Top or right axis of an ordinary layer that repeats the scale of the bottom or left one, without a title (#176) |
| Closed frame | Cadre fermé | Toggle of a layer (`Action::SetClosedFrame`): top and right as mirror axes without tick labels, ticks copied from the bottom and left (#344). **No command in the interface** since the lightened Axes page: the `action-closed-frame*` keys are removed |
| Minor ticks | Sous-graduations | Small marks drawn between the major ticks of an axis, in the same direction; their number per interval (Auto or 1 to 9); « Un nombre sur n » writes only one tick number out of n (#344) |
| Advanced | Avancé | Collapsible block, closed by default, at the bottom of the Ticks section of an axis: length of the major and minor marks (px), width (pt), color; all Auto (S-L8, #344) |
| Ticks (section) | Graduations | Section of the page of an axis: marks, direction, minor ticks, Advanced block (#344); « Afficher les nombres » and « Un nombre sur » stay in « Affichage » |
| Tick direction | Sens des graduations | Outward (Outward), inward (Inward) or on both sides (Both sides) of the axis line (#344) |
| Axes | Axes | The group that gathers the four axes of a layer in the Object Manager (#111, #147) |
| Detach / Reattach | Détacher / Rattacher | Open a system window / put the panel back at its fixed location (#100) |

### Embedded data (#107)

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused | Ref. |
|---|---|---|---|---|
| Show Data | Afficher les données | Reveal and open the workbooks referenced by the graph | Extraire les données | #107 |

### Menu groupings (#114)

<!-- lang: fr columns: French, Refused -->
| English | French | Definition | Refused |
|---|---|---|---|
| New | Nouveau | Submenu of project, workbook and graph creations | Créer (pour le groupe) |
| Panels | Panneaux | Group of the three fixed panels of the application | Calques (réservé au graphique) |
| Toolbars | Barres d'outils | Group of the visibility toggles of the bars | Outils (nom d'une barre) |
| Order | Ordre | Rank of the series in their group or stacking of the objects | Position (géométrie) |

The groups reuse the existing terms Properties, Text, Object,
Sheet, Column, Import, Export and Selection, in the plural when the group
holds several objects. The same entry shares its key in the top menu
and the right-click menu. Display toggles keep their checkbox and leave the
menu open; actions and exclusive choices close it.

**Menu grammar (#152, [`menus.md`](menus.md))** — what it requires of the
texts:

- entries are **bare verbs** ("Rename", "Delete", not "Rename
  Series" nor "Delete Object"): the right-click has already designated the target;
- the **greyed header** that names the target is reserved for the right-click of the graph;
- **Properties…** is a direct action, never a "Properties ▸" submenu
  on right-click (the *Graph* menu of the bar keeps its submenu of pages);
- submenu names that name the target (*Selection*, *Sheet*, *Columns*)
  exist only in the menu bar, which wraps the contents of the right-click;
- a greyed-out entry carries a `.disabled` tooltip that says why;
- **no icon** in a menu, and a shortcut always rendered by
  `format_shortcut` from a `SHORTCUT_*` constant.
