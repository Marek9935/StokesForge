# Textes d'interface — glossaire et règles de rédaction

> **Validé le 2026-10-01** (issue #40). Les choix faits sont consignés en
> section 4.

L'interface est **écrite d'abord en anglais** ; le français en est la
traduction (#39). Ce guide est la source unique des termes et des règles : un
texte visible qui s'en écarte est un défaut. Les chemins `app.rs:N` désignent
`crates/stokesforge-app/src/` au 2026-10-01, avant le découpage d'`app.rs` en
modules (#115) : le code de chaque surface vit depuis dans `src/app/` (table
module → rôle dans `docs/application.md`).

Colonne « Réf. » : **O** usage d'Origin · **G** terme générique des logiciels
de bureau · **I** IUPAC · **D** usage courant de la discipline · **—** propre
à StokesForge. Un `†` marque un terme **réservé** : il n'est pas encore dans
l'interface, mais son mot est déjà fixé.

## 1. Règles

| # | Règle |
|---|---|
| R1 | **Anglais d'abord.** Le français traduit l'anglais, jamais l'inverse. |
| R2 | **Une notion = un mot.** Le glossaire fait foi ; un terme refusé n'apparaît nulle part (libellé, infobulle, message, nom de clé). |
| R3 | **Title Case** : menus, commandes, boutons, onglets, titres de fenêtre. **Sentence case** : champs, cases à cocher, options (boutons radio, listes), infobulles, messages. Mots de liaison en minuscule en Title Case (*of, and, to, in*). Français : initiale seule, jamais Title Case. |
| R4 | **Commande = verbe** (*Add Column*, *Export Graph…*). « … » seulement si une fenêtre demande encore quelque chose. Une entrée qui ouvre une page de réglages porte le nom de la page (*Axes…*). |
| R5 | **Visibilité = « Show X »** (*Show legend*), jamais « X affichée » ni « Présente dans… ». |
| R6 | **Unités.** Champ de l'interface : unité entre parenthèses, symbole SI, après un espace (*Line width (pt)*). **Titre d'axe : grandeur / unité** (IUPAC), symbole ou nom : *ν̃ / cm⁻¹*, *Wavenumber / cm⁻¹* ; un titre repris d'une colonne s'écrit *Nom long / Unités*. Exposants en Unicode dans l'interface (`cm⁻¹`) ; dans un titre d'axe (Typst), `$"cm"^(-1)$`. |
| R7 | **Libellé court, infobulle précise.** Libellé : 1 à 3 mots. Infobulle : l'effet **à la 3ᵉ personne** (« Adds a workbook », jamais « Add a workbook »), la valeur par défaut si elle compte, le raccourci ; jamais une répétition du libellé. Un geste s'écrit comme un geste (« Right-click to remove »). |
| R8 | **Pas de point final** sur une phrase seule. Point entre deux phrases. |
| R9 | **Message = ce qui s'est passé, puis quoi faire.** Ni « please », ni excuse, ni point d'exclamation. Jamais de jargon interne : *session, nœud, arbre, id, fil, spec, commande, entrée d'historique, jeton, recette*. Jamais de `{err}` : la cause d'une `io::Error` se met en phrase par `errors::io` et entre dans le message en variable (`{ $cause }`). |
| R10 | **Pas de « (s) »** : pluriel et accord par Fluent. |
| R11 | **Pas de deux-points** après un libellé de champ. |
| R12 | **Typographie.** « … » (un caractère), « × » et non « x ». Guillemets « » en français, “ ” en anglais. Espace insécable avant `: ; ? !` en français. |
| R13 | **Français : infinitif** pour les consignes (*Choisir un fichier*). Ni tutoiement, ni « vous ». |
| R14 | **Boutons de dialogue** : le verbe de l'action (*Delete*, *Replace*, *Discard Changes*) ; une confirmation est Cancel + verbe, jamais *Keep* seul (#229). *OK* / *Cancel* seulement pour valider / abandonner. Jamais *Yes* / *No*. Assistant : *Back*, *Next*, *Finish*. |
| R15 | **Noms choisis par l'utilisateur ou par défaut** (*Book1*, *Sheet1*, noms de colonnes) : jamais traduits. Sans nom : *Untitled* / *Sans titre*. |
| R16 | **Clés Fluent** : `zone-element-role` (`menu-file-open`, `plot-axes-tick-length`, `import-step-header-title`). |

## 2. Glossaire

### 2.1 Projet et fenêtres

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Project | Projet | Contenu d'un fichier `.stf` | Document, Espace de travail | O |
| Project Manager | Gestionnaire de projet | Panneau qui montre l'arborescence du projet | Explorateur, Navigateur | O (Q15) |
| Folder | Dossier | Conteneur d'objets dans le projet | Répertoire | O |
| Workbook | Classeur | Fenêtre de données qui contient des feuilles | Book, Cahier, Tableur | O |
| Sheet | Feuille | Grille de colonnes d'un classeur | Worksheet, Onglet, Table, Feuille de calcul | O (Q4) |
| Matrix | Matrice | Fenêtre de données en grille régulière | Tableau | O |
| Graph | Graphique | Fenêtre qui trace des séries | Graphe, Chart, Figure, Plot | O (Q1, Q2) |
| Note | Note | Fenêtre de texte libre | Mémo | G |
| Window | Fenêtre | Classeur, graphique, matrice ou note ouvert | Onglet, Vue | O |
| Detach / Reattach | Détacher / Réattacher | Sortir une fenêtre du cadre principal / l'y remettre | Flotter, Désancrer, Dock | G |
| Restore | Restaurer | Rouvrir une fenêtre fermée ou réduite | Rétablir | G |
| Status bar | Barre d'état | Ligne du bas : dernier message | Barre de statut | G |
| Script | Script | Programme Python exécuté sur le projet | Macro, Programme | — |
| Run / Stop | Exécuter / Arrêter | Lancer / interrompre un script | Lancer, Interrompre, Kill | G |
| Output | Sortie | Texte écrit par un script | Résultat, Log | G |
| Traceback | Trace d'appels | Pile d'appels d'une exception Python | Backtrace, Pile | D |

### 2.2 Application

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| New Project | Nouveau projet | Remplacer le projet courant par un projet vide, après confirmation si nécessaire | Nouvelle session, Nouveau document | #73 |
| Discard Changes | Abandonner les modifications | Confirmer la perte du travail non enregistré (New Project, Open…, Quit) ; à côté de Cancel et Save | Supprimer le projet, Effacer | #73, #229 |
| Open / Save / Save As / Quit | Ouvrir / Enregistrer / Enregistrer sous / Quitter | Commandes de fichier | Sauvegarder, Fermer (pour Quitter) | G |
| Undo / Redo | Annuler / Rétablir | Défaire / refaire la dernière action | Refaire, Répéter | G |
| Import / Export | Importer / Exporter | Lire un fichier d'appareil / écrire un graphique | Charger, Sauvegarder | O |
| Refresh | Actualiser | Relire le fichier source d'une colonne | Rafraîchir, Recharger | G |
| Properties | Propriétés | Réglages d'un objet précis | Options, Paramètres | O |
| Preferences† | Préférences | Réglages de l'application | Options, Paramètres | G |
| Expand by Default | Déplier par défaut | Repli initial des nouveaux groupes de l’Object Manager ; ne modifie pas les groupes déjà vus | Tout déplier | #139 |
| Objects | Objets | Section des textes et formes du graphique ou d’un calque dans l’Object Manager | Éléments | #139 |
| Setting | Réglage | Valeur choisie par l'utilisateur | Option, Paramètre | G |
| Template | Modèle | Réglages d'import enregistrés sous un nom | Recette, Preset, Filtre | O |
| Wizard | Assistant | Fenêtre à étapes | Wizard, Guide | G |
| Theme | Thème | Jeu de couleurs globales d'un graphique | Style, Palette | O |
| Shortcut | Raccourci | Touches qui lancent une commande | Raccourci clavier | G |

### 2.3 Classeur et grille

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Column | Colonne | Série de valeurs d'un même type | Champ, Variable | O |
| Row | Ligne | Rangée de cellules d'une feuille | Enregistrement | O |
| x' (x prime) | x' | Valeur numérique propre à une colonne (ligne spéciale de l'en-tête), abscisse d'une série « ligne » ; ne se traduit pas | X2, Temps, Instant (le temps n'est qu'un cas d'usage) | — |
| Line | Ligne | Rangée de texte **d'un fichier** (import) | Row | G |
| Cell | Cellule | Une valeur dans une colonne et une ligne | Case | G |
| Selection | Sélection | Cellules, colonnes ou lignes choisies | Plage | G |
| Long Name | Nom long | Description d'une colonne, affichée dans l'en-tête | Libellé, Description | O |
| Units | Unités | Unité des valeurs d'une colonne, affichée dans l'en-tête | Unité (singulier) | O (Q13) |
| Comments | Commentaires | Texte libre d'une colonne | Remarques, Notes | O |
| Metadata | Métadonnées | Paires clé-valeur lues dans le fichier importé | Infos, En-tête du fichier | O |
| Role | Rôle | Fonction d'une colonne dans un tracé | Désignation, Type | O (Q8) |
| X / Y / Z | X / Y / Z | Rôles : abscisse, ordonnée, troisième axe | — | O |
| X Error / Y Error | Erreur X / Erreur Y | Rôles : barre d'erreur en X ou en Y | xerr, yerr | O |
| Label | Étiquette | Rôle : texte rattaché à chaque point | Libellé, Annotation | O (Q11) |
| None | Aucun | Rôle : colonne hors tracé | Ignorer, Disregard | O |
| Type | Type | Nature d'une colonne : Numeric, Text, Mixed, Date/Time | Format | O |
| Numeric / Text / Mixed / Date/Time | Numérique / Texte / Mixte / Date et heure | Valeur des types | Nombres, Date-heure | O |
| Cut / Copy / Paste | Couper / Copier / Coller | Presse-papiers | — | G |
| Paste in Place | Coller à l'identique | Coller à la position d'origine de la copie | Coller en absolu | G |
| Copy with Header | Copier avec l'en-tête | Copier les valeurs et l'en-tête de colonne | — | O |
| Clipboard | Presse-papiers | Mémoire de copie du système | Presse-papier, Tampon | G |
| Add | Ajouter | Créer un objet **à la fin** (colonne, feuille, texte) | Nouveau, Créer | O |
| Insert | Insérer | Créer **à une position** (lignes) | Ajouter | G |
| Delete | Supprimer | Détruire un objet, des lignes ou des cellules | Effacer, Enlever | G |
| Delete and Shift Up | Supprimer et remonter | Supprimer des cellules et remonter celles du dessous | Supprimer et décaler | G |
| Clear | Effacer | Vider des cellules, sans les supprimer | Vider | G |
| Remove | Retirer | Sortir un élément d'une liste, sans le détruire | Supprimer | G |
| Rename / Move | Renommer / Déplacer | Changer le nom / la place d'un objet | — | G |
| Edit | Modifier | Changer le contenu ou les réglages d'un objet existant (libellés d'historique) ; *Change* seulement dans « Change Type of Column A » | Éditer, Changer | G |
| Show / Hide | Afficher / Masquer | Rendre visible / invisible | Cacher | G |
| Grid | Grille | Cellules d'une feuille (jamais le quadrillage d'un graphique) | — | G |

### 2.4 Import

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Import Wizard | Assistant d'import | Fenêtre à six étapes qui règle l'import | Importateur | O |
| Delimiter | Séparateur | Caractère qui sépare les valeurs d'une ligne | Délimiteur | O |
| Decimal separator | Séparateur décimal | Caractère entre partie entière et décimale | Virgule (pour le nom) | O |
| Thousands separator | Séparateur de milliers | Caractère qui groupe les chiffres | Milliers | O |
| Fixed width | Largeur fixe | Colonnes de largeur donnée, sans séparateur | Colonnes fixes | O |
| Encoding | Encodage | Jeu de caractères du fichier | Codage, Charset | G |
| Preview | Aperçu | Contenu du fichier tel que lu | Prévisualisation | O |
| Header | En-tête | Lignes au début du fichier, avant les données | Entête | O |
| Subheader | Sous-en-tête | Lignes d'en-tête qui donnent nom long, unités, commentaires | — | O |
| Footer | Pied de page | Lignes après les données | Fin | G |
| Comment line | Ligne de commentaire | Ligne ignorée à cause de son préfixe | — | G |
| Skipped line | Ligne ignorée | Ligne que l'import n'utilise pas | — | G |
| Block | Bloc | Série de données d'un fichier, séparée des autres | Section | O |
| Missing values | Valeurs manquantes | Textes lus comme une cellule vide | Jetons, Nulls | G |
| Source file | Fichier source | Fichier d'où vient une colonne | Origine | O |
| Header key | Clé d'en-tête | Nom d'une métadonnée de l'en-tête | — | — |
| Destination | Destination | Où l'import écrit ses colonnes | Cible | O |

### 2.5 Graphique

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Graph Properties | Propriétés du graphique | Fenêtre de réglage d'un graphique | Propriétés du graphe | O |
| General | Général | Onglet : titre et fond | Graph, Graphe | G (Q23) |
| Plot Area | Zone de tracé | Rectangle délimité par les axes | Zone de dessin, Plot | G |
| Layer | Calque | Zone de tracé avec ses axes ; un graphique en contient une ou plusieurs | Panel, Panneau, Couche | O (Q22) |
| Title | Titre | Texte en tête du graphique | Intitulé | G |
| Axis (pl. Axes) | Axe | Ligne graduée portant une grandeur | — | G |
| X (bottom) / Y (left) / Secondary Y (right) | X (bas) / Y (gauche) / Y secondaire (droite) | Les trois axes d'une zone | Y2, Axe de droite | G |
| Axis title | Titre de l'axe | Texte qui nomme la grandeur de l'axe | Libellé, Label | G |
| Tick | Graduation | Trait sur l'axe | Repère, Coche | I |
| Tick label | Étiquette de graduation | Valeur écrite à côté d'une graduation | Libellé de graduation | G (Q11) |
| Gridlines | Quadrillage | Lignes qui prolongent les graduations | Grille | G (Q12) |
| Scale | Échelle | Loi de répartition des valeurs sur l'axe | Mode | O |
| Linear / Log scale | Linéaire / Échelle log | Les deux échelles actuelles (Log10) | Logarithmique | O |
| Reciprocal† | Inverse | Échelle en 1/x (spectres en nombre d'onde) | Réciproque | O |
| Range | Plage | Intervalle de valeurs montré sur l'axe | Bornes, Étendue | O |
| Auto range | Plage auto | Plage calculée d'après les données | Bornes automatiques | O |
| Min / Max | Min / Max | Extrémités de la plage | De / À | O |
| Reverse axis | Axe inversé | Valeurs décroissantes dans le sens de lecture | Axe retourné | O |
| Legend | Légende | Liste des séries avec leur style | — | G |
| Legend entry | Entrée de légende | Une ligne de la légende | Libellé de légende | G |
| Position | Position | Coin de la légende : Top left… ; Reset Position / Réinitialiser la position : remet un titre ou la légende à sa place par défaut | Emplacement | G |
| Series | Série | Données tracées d'une colonne Y | Courbe, Trace, Plot, Jeu de données | G (Q3) |
| Series Group | Groupe de séries | Séries ajoutées ensemble (ou réunies à la main) qui se règlent ensemble ; nœud repliable de l'Object Manager et de la liste des séries, nommé « Group n » tant qu'on ne l'a pas renommé (#177, #197) | Groupe de courbes | G |
| Ungroup | Dissoudre le groupe | Chaque série du groupe devient une série simple, style gardé ; ne supprime rien (#197). À ne pas confondre avec « Delete Group » / « Supprimer le groupe », qui retire aussi les séries | Dégrouper, Éclater | G |
| Delete Group | Supprimer le groupe | Retire du graphique le groupe et toutes ses séries (#197) ; dans les menus, c'est « Delete » / « Supprimer » appliqué au groupe | Effacer le groupe | G |
| Select Members | Sélectionner les membres | Sélectionne le groupe : les poignées de toutes ses séries s'affichent (#197) | Sélectionner le groupe | G |
| Series (Object Manager) | Séries | Le nœud sous un calque qui liste ses séries ; ancien « Plot Group » / « Groupe de tracés », renommé ; ne pas l'appeler groupe (#177) | Plot Group, Groupe de tracés | G |
| Whole group / This series only | Tout le groupe / Cette série seulement | Portée des réglages d'une série qui est dans un groupe ; les réglages propres à une série sont grisés en portée « groupe » (#177) | Appliquer à tous | G |
| Leave Group | Quitter le groupe | Détache la série de son groupe, style conservé (#177) | Dissocier, Sortir | G |
| Color gradient | Nuancier | Dégradé de couleurs réparti sur les séries d'un groupe : Blue to Red / Bleu vers Rouge… (#178 ; la flèche → n'est pas dans la police de l'interface : un carré s'affiche à sa place, ne pas l'employer) ; enregistrable dans le projet ou pour l'utilisateur, avec son nom | Palette (qualitative, sans ordre), Dégradé | G |
| Plot type | Type de tracé | Façon de tracer une série | Mode | O |
| Scatter | Points | Symboles seuls | Nuage de points | O (Q6) |
| Line | Ligne | Trait seul | Courbe | O |
| Line + Symbol | Ligne + symboles | Trait et symboles | Ligne + points | O |
| Bar | Barres | Barres verticales | Colonnes | O (Q7) |
| Histogram | Histogramme | Barres des effectifs par classe | — | G |
| Line + Error Bars | Ligne + barres d'erreur | Trait avec barres d'erreur | — | G |
| Line (stroke) | Trait | Style, épaisseur et couleur du tracé | Ligne | O |
| Line style | Style de trait | Aucun, Continu, Tirets, Pointillés, Tiret-point | Plein | G |
| Line width (pt) | Épaisseur (pt) | Largeur du trait | Largeur | O |
| Symbol | Symbole | Figure posée sur chaque point | Marqueur, Point | O (Q5) |
| Shape / Size (pt) | Forme / Taille (pt) | Figure du symbole / sa dimension | — | O |
| Circle, Square, Triangle, Diamond, Plus, Cross | Cercle, Carré, Triangle, Losange, Plus, Croix | Formes de symbole | Disque, Rond | G (Q24) |
| Fill† | Remplissage | Couleur de l'intérieur d'une forme | Fond | O |
| Color | Couleur | Teinte d'un élément | — | G |
| Opacity | Opacité | Part de couleur opaque (0-100 %) | Transparence, Alpha | G |
| Background | Fond | Couleur derrière le graphique | Arrière-plan | G |
| Auto | Auto | Valeur choisie par StokesForge d'après le contexte | Automatique, Défaut | G (Q9) |
| Text | Texte | Texte libre ancré aux données, posé sur le graphique | Annotation, Texte libre | O (Q21) |
| Font size (pt) / Rotation (°) | Taille (pt) / Rotation (°) | Réglages d'un texte | Angle | G |
| Object | Objet | Rectangle, ellipse, ligne ou flèche posé sur le graphique, ancré aux données (« forme » est réservé au symbole) | Forme, Dessin, Shape | O (#76) |
| Bring to Front / Send to Back | Amener devant / Envoyer derrière | Ordre d'empilement des objets | Premier plan, Arrière-plan (réservé au fond) | O (#76) |
| Formula | Formule | Texte composé par Typst, entre `$…$` | Équation, LaTeX | — |
| Zoom / Reset Zoom | Zoom / Réinitialiser le zoom | Cadrage de la vue ; retour aux plages d'origine | Rescale, Recadrer | G |
| Pan† | Panoramique | Déplacer le cadrage | Déplacement | G |
| Global colors | Couleurs globales | Fond, texte, axes d'un thème | — | — |
| Palette | Palette | Jeu de couleurs proposé | — | G |
| Swatch | Nuance | Une couleur d'une palette | Échantillon | G |
| Saved colors | Couleurs enregistrées | Couleurs gardées dans le projet ou pour l'utilisateur | Favoris | — |
| Hex / RGB / HSL | Hexa / RVB / TSL | Façons d'écrire une couleur | — | G |
| Resolution | Résolution | Facteur d'échelle d'un export PNG | DPI | G |
| Outlines | Contours | Texte d'un SVG exporté dessiné en formes plutôt qu'en texte (#162) | Courbes, Tracés | G |

### 2.6 Spectroscopie

Le glossaire donne les mots des **titres d'axe, unités et traitements** que
l'interface devra écrire. Grandeurs et unités : IUPAC *Green Book* ; une
grandeur sans dimension (absorbance) ne porte pas d'unité. Titre d'axe :
**grandeur / unité** (R6), donné en dernière colonne de la définition.

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Spectrum | Spectre | Intensité mesurée en fonction d'une abscisse spectrale | Courbe, Trace | I |
| Wavenumber | Nombre d'onde | Inverse de la longueur d'onde · *ν̃ / cm⁻¹* | Fréquence (cm⁻¹) | I |
| Wavelength | Longueur d'onde | Distance entre deux maxima du champ · *λ / nm* | — | I |
| Raman shift | Déplacement Raman | Écart en nombre d'onde à la raie excitatrice · *Δν̃ / cm⁻¹* | Décalage Raman, Shift | I, D (Q16) |
| Energy | Énergie | Énergie des photons · *E / eV* | — | I |
| Frequency | Fréquence | Nombre d'oscillations par seconde · *ν / Hz* | — | I |
| Chemical shift† | Déplacement chimique | Abscisse RMN · *δ / ppm* | Shift | I |
| Absorbance | Absorbance | Grandeur sans unité, A = lg(P₀/P) · *A* | Densité optique, OD, Extinction | I (Q17) |
| Transmittance | Transmittance | Part de lumière transmise · *T / %* | Transmission | I |
| Reflectance | Réflectance | Part de lumière réfléchie · *R / %* | Réflexion | I |
| Intensity | Intensité | Signal en unités arbitraires · *I / a.u.* (fr. *u.a.*) | Intensité (arb. units) | D |
| Counts | Coups | Nombre de détections | Comptes | D (Q18) |
| Molar absorption coefficient† | Coefficient d'absorption molaire | Absorbance ramenée à 1 mol L⁻¹ et 1 cm · *ε / (L mol⁻¹ cm⁻¹)* | Coefficient d'extinction | I |
| Sample / Blank / Reference | Échantillon / Blanc / Référence | Mesure étudiée / sans analyte / de comparaison | Témoin | D |
| Baseline† | Ligne de base | Signal de fond sous les pics | Fond continu | I |
| Peak† | Pic | Maximum local du signal | Bande (sauf IR/Raman : *band*) | I |
| Peak position† | Position du pic | Abscisse du maximum | Centre | D |
| Height† | Hauteur | Ordonnée du maximum, ligne de base retirée | Amplitude | D |
| FWHM† | LMH | Largeur à mi-hauteur d'un pic | Largeur | I |
| Area† | Aire | Intégrale d'un pic | Surface | D |
| Normalize† | Normaliser | Ramener l'intensité à 1 au maximum ou à l'aire | Standardiser | D |
| Smooth† | Lisser | Réduire le bruit d'un spectre | Filtrer | D |
| Derivative† | Dérivée | Pente du spectre, d'ordre 1 ou 2 | Différentielle | D |
| Integrate† | Intégrer | Calculer l'aire sous la courbe | — | D |
| Fit† | Ajuster | Faire coller un modèle aux données | Fitter, Lisser | D |
| Offset (stack)† | Décalage (empilement) | Translation verticale entre spectres superposés | Espacement | D |

### 2.7 Analyse (#181)

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Analysis | Analyse | Menu et fenêtre des traitements mathématiques | Traitement | D |
| Results | Résultats | Volet sous la grille : calculs conservés et leurs valeurs | Sorties | D |
| Calculation | Calcul | Un traitement enregistré, avec ses colonnes de sortie | Opération | D |
| Out of date | Pas à jour | Calcul dont une entrée a changé ou disparu | Périmé, Obsolète | D |
| Recalculate | Recalculer | Refaire un calcul sur les données actuelles | Rafraîchir | D |
| Anchor point | Point d'ancrage | Point posé sur l'aperçu pour tracer une ligne de base | Ancre | D |
| Zone | Zone | Intervalle d'abscisses choisi sur l'aperçu | Région, Plage | D |
| Reference | Référence | Colonne par laquelle on corrige une série | Témoin | D |
| IRF (instrument response) | Réponse de l'instrument | Réponse temporelle de l'appareil, à déconvoluer | — | I |
| Reconvolution | Reconvolution | Ajuster un modèle convolué avec la réponse de l'instrument | — | D |
| Deconvolution | Déconvolution | Retrouver le signal avant la réponse de l'instrument | — | D |
| Residuals | Résidus | Écart entre les données et l'ajustement | Erreurs | I |
| Fitted | Ajusté | Valeur du modèle ajusté | Fitté | D |
| Std. error | Erreur type | Incertitude d'un paramètre ajusté | Écart-type, Sigma | I |
| Prominence | Proéminence | Hauteur d'un pic au-dessus de son entourage | Saillance | D |
| Shift | Décalage | Translation en abscisse de la réponse (≠ *chemical shift*) | Shift | D |
| Plateau | Palier | Niveau constant vers lequel une exponentielle se stabilise (≠ *Offset* de l'empilement) | Décalage, Offset | D |
| Analysis type | Type d'analyse | Famille de calcul choisie dans la fenêtre d'analyse | Opération | D |

## 3. Exemples avant / après

Libellés réels, cités tels quels. Règle : numéro de la section 1 ou terme du
glossaire.

| # | Avant (`fichier:ligne`) | Anglais | Français | Règle |
|---|---|---|---|---|
| 1 | « Graphique » › « Propriétés du graphe… » (`app.rs:1282-1284`) ; onglet « Graphe » (`state.rs:1296`) | Graph › Graph Properties… ; tab General | Graphique › Propriétés du graphique… ; onglet Général | R2 |
| 2 | « Réinitialiser le zoom » (`app.rs:1301`) ; « réinitialiser le zoom » (`app.rs:7239`) | Reset Zoom | Réinitialiser le zoom | R3 |
| 3 | « Ajouter un texte », « Supprimer le texte sélectionné » (`app.rs:1314, 1326`) ; « Texte libre » (`state.rs:1355`) | Add Text, Delete Text | Ajouter un texte, Supprimer le texte | R2 |
| 4 | « Exporter le graphe… » + « SVG, PDF ou PNG, avec le zoom courant » (`app.rs:1342-1345`) | Export Graph… + tip “SVG, PDF or PNG, at the current zoom” | Exporter le graphique… + « SVG, PDF ou PNG, au zoom courant » | R4, R7 |
| 5 | « Résolution des exports PNG » (`app.rs:1356`) | PNG Resolution | Résolution PNG | R7 |
| 6 | « Afficher la ligne Metadata des colonnes » (`app.rs:1378`) ; « Long Name », « Units » (`app.rs:5162-5171`) en anglais dans une interface française | Show Metadata Row ; Long Name, Units | Afficher la ligne Métadonnées ; Nom long, Unités | R1, R2 |
| 7 | « Détacher le Project Manager en fenêtre séparée » (`app.rs:1419`) | Detach Project Manager | Détacher le gestionnaire de projet | R7 |
| 8 | « Suivant > », « < Précédent », « Terminer » (`app.rs:6515, 6524, 6506`) | Next, Back, Finish | Suivant, Précédent, Terminer | R14 |
| 9 | « Séparateur décimal : » (`app.rs:5939`) | Decimal separator | Séparateur décimal | R11 |
| 10 | « Valeurs manquantes (séparées par des virgules) : » + « Jetons lus comme une cellule vide : --, n/a, *, NaN… » (`app.rs:5967-5971`) | Missing values + tip “Comma-separated. Read as empty cells: --, n/a, *, NaN…” | Valeurs manquantes + « Séparées par des virgules. Lues comme cellules vides : --, n/a, *, NaN… » | R7, R9 |
| 11 | « Caractères à retirer en tête de chaque ligne de données : » (`app.rs:6042`) | Remove leading characters + tip “Removed at the start of every data line” | Retirer les premiers caractères + « Retirés au début de chaque ligne de données » | R7 |
| 12 | « Lignes d'en-tête : », « dont sous-en-tête : » (`app.rs:6104, 6112`) | Header lines, Subheader lines + tip “Counted in the header lines” | Lignes d'en-tête, Lignes de sous-en-tête + « Comptées dans les lignes d'en-tête » | R7, R11 |
| 13 | « Les données commencent ici », « Cette ligne donne les unités » (`app.rs:5574-5578`) | Start Data Here, Use as Units Line | Commencer les données ici, Utiliser comme ligne d'unités | R4 |
| 14 | « {shown} ligne(s) de données montrée(s) sur {} au total. » (`app.rs:5543`) | Showing {$shown} of {$total} data lines | {$shown} lignes de données sur {$total} | R8, R10 |
| 15 | « Nom des séries : » → « Nom de fichier » / « Clé d'en-tête » (`app.rs:6173-6191`) | Column names → File name / Header key | Nom des colonnes → Nom du fichier / Clé d'en-tête | Colonne ≠ Série |
| 16 | « Choisis un fichier avant de détecter l'en-tête » (`state.rs:4348`) ; « Choisissez un fichier à l'étape 1. » (`app.rs:6224`) | Choose a file before detecting the header | Choisir un fichier avant de détecter l'en-tête | R13 |
| 17 | « Presse-papier vide ou illisible » (`app.rs:4588`) | Clipboard is empty or unreadable | Presse-papiers vide ou illisible | R9 (orthographe) |
| 18 | « Vider » (`app.rs:4609`) ; « Effacer la sélection » (`state.rs:4740`) | Clear ; Clear Selection | Effacer ; Effacer la sélection | R2 |
| 19 | « Supprimer et décaler » + « Suppr vide à la première pression et ne supprime qu'à la seconde ; cette entrée supprime tout de suite. » (`app.rs:4619-4622`) | Delete and Shift Up + tip “Deletes now and shifts cells up. The Del key first clears, then deletes” | Supprimer et remonter + « Supprime tout de suite et remonte les cellules. La touche Suppr efface d'abord, puis supprime » | R7 |
| 20 | « Session injoignable : {err} » (`state.rs:3430`) ; « Le fil de script a disparu sans répondre (a-t-il paniqué ?) » (`state.rs:3453`) | Internal error: {$error} ; The script engine stopped unexpectedly. Run the script again | Erreur interne : {$error} ; Le moteur de script s'est arrêté. Relancer le script | R9 |
| 21 | « {id} n'est pas une feuille de ce projet » (`state.rs:4562`) ; « Impossible de grouper l'effacement de la sélection » (`state.rs:4741`) | This sheet is no longer in the project ; Could not clear the selection | Cette feuille n'est plus dans le projet ; Effacement impossible | R9 |
| 22 | « Aucune colonne de rôle Y dans la sélection : rien à tracer » (`state.rs:5674`) | No Y column selected. Select a Y column to plot | Aucune colonne Y sélectionnée. Sélectionner une colonne Y à tracer | R9 |
| 23 | « Import en cours… 0/{total} fichier(s) » (`state.rs:3645`) ; « {} sélectionné(s) » (`app.rs:4353`) | Importing… {$done} of {$total} files ; {$count} selected | Import en cours… {$done}/{$total} fichiers ; {$count} sélectionné / {$count} sélectionnés (accord par Fluent) | R10 |
| 24 | « Axe affiché », « Légende affichée », « Présente dans la légende » (`app.rs:8451, 8628, 8526`) | Show axis, Show legend, Show in legend | Afficher l'axe, Afficher la légende, Afficher dans la légende | R5 |
| 25 | « Échelle logarithmique », « Axe inversé », « Bornes automatiques » (`app.rs:8416-8428`) | Log scale, Reverse axis, Auto range | Échelle log, Axe inversé, Plage auto | R7 |
| 26 | « Épaisseur (pt) », « Taille (pt) », « Plein » (`app.rs:8563, 8600, 8675`) | Line width (pt), Size (pt), Solid | Épaisseur (pt), Taille (pt), Continu | R6 |
| 27 | « Marqueur », « Forme », « Couleur du marqueur », « Comme le trait » (`app.rs:8574-8615`) | Symbol, Shape, Symbol color, Same as line | Symbole, Forme, Couleur du symbole, Comme le trait | Symbole |
| 28 | « Points », « Ligne + points », « Barres » (`app.rs:8661-8664`) | Scatter, Line + Symbol, Bar | Points, Ligne + symboles, Barres | Symbole |
| 29 | « Automatique (contraste avec le fond) », « Automatique (blanc) » (`app.rs:8158, 8167`) ; « Auto » (`color_picker.rs:389`) | Auto (contrasts with background), Auto (white) | Auto (contraste avec le fond), Auto (blanc) | R2 |
| 30 | « Grille », « Lignes verticales (graduations X) » (`app.rs:8182-8184`) | Gridlines, Vertical (X ticks) | Quadrillage, Verticales (graduations X) | Grille ≠ Quadrillage |
| 31 | « Nom vide = celui de la colonne tracée, affiché tel quel. » (`app.rs:8487`) | Empty: uses the plotted column's name as is | Vide : reprend le nom de la colonne tracée tel quel | R7 |
| 32 | « Clic droit : retirer » (`color_picker.rs:559`) | Right-click to remove | Clic droit pour retirer | R7 |
| 33 | « Classeur actif (une feuille par fichier) » (`app.rs:6388`) | Active workbook (one sheet per file) | Classeur actif (une feuille par fichier) | R3 |
| 34 | « Fichiers et modèle », « En-tête et fin des données » (`state.rs:1584-1586`) | Files and Template, Header and Data End | Fichiers et modèle, En-tête et fin des données | R3 |

## 4. Décisions

Validées par l'utilisateur le 2026-10-01 : la recommandation est retenue
partout, **sauf Q14 et Q22** où l'alternative l'emporte (marquées ✔).

| # | Question | Recommandation | Alternative |
|---|---|---|---|
| Q1 | Anglais : **Graph**, **Chart** ou **Plot** pour la fenêtre de tracé ? | **Graph** (Origin ; le code dit déjà `Graph1`) | *Chart* (générique, Excel) ; *Plot* (matplotlib, mais Origin l'emploie pour la série) |
| Q2 | Français : **Graphique** ou **Graphe** ? | **Graphique** (générique bureautique ; « graphe » évoque la théorie des graphes) | *Graphe* (usage d'Origin en français, mot du code et des docs actuels) |
| Q3 | La donnée tracée : **Series**, **Plot** (Origin) ou **Curve** ? | **Series / Série** (générique, jamais ambigu avec « tracé » ni avec « graphique ») | *Plot* (Origin) ; *Curve / Courbe* (usage des spectroscopistes, faux pour un nuage de points) |
| Q4 | **Worksheet** partout, ou **Sheet** (plus court) ? | **Sheet** partout (décision #94, 2026-10-03 : « Worksheet » prêtait à confusion avec le nom par défaut `Sheet1`) ; remplace la décision #39 | *Worksheet* : le terme d'Origin pour les classeurs, plus long |
| Q5 | **Symbol** ou **Marker** ? (français : Symbole ou Marqueur) | **Symbol / Symbole** (Origin ; cohérent avec *Line + Symbol*) | *Marker / Marqueur* (matplotlib, Excel ; terme actuel de l'interface) |
| Q6 | Français de *Scatter* : **Points** ou **Nuage de points** ? | **Points** (un mot, déjà en place) | *Nuage de points* (terme statistique, 3 mots) |
| Q7 | **Bar** ou **Column** pour les barres verticales ? | **Bar / Barres** (*Column* est déjà « colonne de données ») | *Column* (Origin : Column = vertical, Bar = horizontal) |
| Q8 | Fonction d'une colonne : **Role** ou **Designation** (Origin « Set As ») ? Et *None* ou *Disregard* ? | **Role / Rôle**, valeur **None** | *Designation* ; *Disregard / Ignorer* |
| Q9 | **Auto** ou **Automatic** ? | **Auto** (court ; l'infobulle précise « contraste avec le fond ») | *Automatic / Automatique* écrit en entier |
| Q10 | « Annuler » nomme à la fois *Undo* et le bouton *Cancel* de la fenêtre des propriétés. Le garder ? | **Oui** (usage courant ; contextes distincts) | *Abandonner* pour *Cancel* (propre, mais inhabituel) |
| Q11 | Texte rattaché à un point ou à une graduation : **Label / Étiquette** pour le rôle de colonne et **Tick label / Étiquette de graduation** ? | **Étiquette** pour les deux ; « libellé » refusé partout | *Libellé de graduation* ; *Valeur de graduation* |
| Q12 | Lignes qui prolongent les graduations : **Gridlines / Quadrillage** ou **Grid / Grille** ? | **Quadrillage** (« grille » est déjà la grille de la feuille) | *Grille* (terme actuel) |
| Q13 | **Units** (pluriel d'Origin) ou **Unit** ? | **Units / Unités** partout | *Unit / Unité* (singulier, actuel dans l'aperçu d'import et l'axe) |
| Q14 | Unités : parenthèses **Wavenumber (cm⁻¹)** ou barre IUPAC **ν̃ / cm⁻¹** ? | *Parenthèses* (usage des revues et d'Origin) | ✔ **Barre de fraction** (IUPAC) pour les titres d'axe ; les champs de l'interface gardent les parenthèses (R6) |
| Q15 | Panneau d'arborescence : **Project Manager** (nom actuel) ou **Project Explorer** (nom d'Origin) ? | **Project Manager** (déjà dans le code et les docs) | *Project Explorer* |
| Q16 | Français de *Raman shift* : **Déplacement Raman** ou **Décalage Raman** ? Et *Offset* (empilement) : **Décalage** ? | **Déplacement Raman** ; **Décalage** réservé à l'empilement, pour ne pas les confondre | *Décalage Raman* ; *Translation* ou *Espacement* pour l'empilement |
| Q17 | **Absorbance** sans unité (IUPAC) ou **Absorbance (a.u.)** (usage courant, gabarits d'Origin) ? | **Absorbance** seule | *Absorbance (a.u.)* |
| Q18 | Français de *Counts* : **Coups** ou **Counts** conservé ? | **Coups** | *Counts* invariant (usage oral en XPS et Raman) |
| Q19 | Tutoiement, « vous » ou infinitif pour les consignes en français ? | **Infinitif** (*Choisir un fichier*) | *Vous* impératif (*Choisissez un fichier*) |
| Q20 | **Detach / Reattach** ou **Float / Dock** ? | **Detach / Reattach** (verbes déjà en place, compris sans jargon) | *Float / Dock* (Qt, Office) |
| Q21 | Texte posé sur le graphique : **Text** ou **Annotation** ? | **Text** (un mot, le geste est « Add Text ») | *Annotation* (matplotlib, terme du code) ; *Text Label* (heurte *Label*) |
| Q22 | Sous-ensemble tracé d'un graphique : **Panel** ou **Layer** (Origin) ? | *Panel / Panneau* | ✔ **Layer / Calque** (Origin) |
| Q23 | Premier onglet de *Graph Properties* : **General** ou **Graph** ? | **General** (« Graph » répète le titre de la fenêtre) | *Graph* (nom actuel : « Graphe ») |
| Q24 | Formes de symbole : **Plus / Cross** ou **Cross / X** ? | **Plus (+)** et **Cross (×)** | *Cross (+)* et *X (×)* |
| Q25 | Deux-points après un libellé de champ (« Séparateur : »)  — les supprimer ? | **Supprimer** (R11 : plus court, aucun souci d'espace insécable) | *Garder* en français seulement |

### Gestionnaire d'objets (#81–85)

| Anglais | Français | Définition |
|---|---|---|
| Object Manager | Gestionnaire d'objets | Arbre des contenus du graphique actif |
| Layer | Calque | Un panneau de tracé (`PlotSpec::panels`) |
| Series | Séries | Les séries d'un calque, avec leurs groupes de séries en retrait (ancien « Plot Group », #177) |
| Graph texts | Textes du graphique | Titres du graphique, des calques et des axes, et légende : listés dans l'Object Manager si la bascule « Show graph texts » est active (#102) |
| Show Axes | Afficher les axes | Les axes d'un calque (ligne, graduations, libellés), listés dans l'Object Manager si la bascule « Show Axes » est active (#111, #147) ; ce filtre ne modifie pas leur visibilité sur la figure ; une ligne « X (bottom) Axis » / « Axe X (bas) » par axe, dont Secondary X (top) / X secondaire (haut) |
| Mirror axis | Axe miroir | Axe du haut ou de droite d'un calque ordinaire qui reprend l'échelle de celui du bas ou de gauche, sans titre (#176) |
| Axes | Axes | Le groupe qui réunit les quatre axes d'un calque dans l'Object Manager (#111, #147) |
| Detach / Reattach | Détacher / Rattacher | Ouvrir une fenêtre système / replacer le panneau à son emplacement fixe (#100) |

### Données embarquées (#107)

| Anglais | Français | Définition | Refusés | Réf. |
|---|---|---|---|---|
| Show Data | Afficher les données | Révéler et ouvrir les classeurs référencés par le graphique | Extraire les données | #107 |

### Regroupements des menus (#114)

| Anglais | Français | Définition | Refusés |
|---|---|---|---|
| New | Nouveau | Sous-menu des créations de projet, classeur et graphique | Créer (pour le groupe) |
| Panels | Panneaux | Groupe des trois panneaux fixes de l'application | Calques (réservé au graphique) |
| Toolbars | Barres d'outils | Groupe des bascules de visibilité des barres | Outils (nom d'une barre) |
| Order | Ordre | Rang des séries dans leur groupe ou empilement des objets | Position (géométrie) |

Les groupes reprennent les termes existants Properties, Text, Object,
Sheet, Column, Import, Export et Selection, au pluriel lorsque le groupe
porte plusieurs objets. Une même entrée partage sa clé dans le menu du haut
et le clic droit. Les bascules d'affichage gardent leur case et laissent le
menu ouvert ; les actions et les choix exclusifs le ferment.

**Grammaire des menus (#152, [`menus.md`](menus.md))** — ce qu'elle impose aux
textes :

- les entrées sont des **verbes nus** (« Rename », « Delete », pas « Rename
  Series » ni « Delete Object ») : le clic droit a déjà désigné la cible ;
- l'**en-tête grisé** qui nomme la cible est réservé au clic droit du graphe ;
- **Properties…** est une action directe, jamais un sous-menu « Properties ▸ »
  au clic droit (le menu *Graph* de la barre garde son sous-menu de pages) ;
- les noms de sous-menus qui nomment la cible (*Selection*, *Sheet*, *Columns*)
  n'existent que dans la barre de menus, qui enveloppe le contenu du clic droit ;
- un grisage porte une info-bulle `.disabled` qui dit pourquoi ;
- **aucune icône** dans un menu, et un raccourci toujours rendu par
  `format_shortcut` d'une constante `SHORTCUT_*`.
