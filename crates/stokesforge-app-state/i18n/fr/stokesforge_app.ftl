# Catalogue français de l'interface de StokesForge (issue #49, chantier #39).
#
# Traduction du catalogue de référence `en` : mêmes clés, mêmes arguments, selon
# la colonne française de `docs/ui-texts.md` (infinitif pour les
# consignes, minuscule après la première lettre, « … » en un caractère,
# espace insécable avant « : », « ; », « ? » et dans les guillemets « »).
# Toute clé ajoutée à `en` s'ajoute ici dans le même commit : le test de
# complétude est bloquant.

## Fenêtre

window-title-untitled = Projet sans titre
window-title-project = { $name } — StokesForge
window-title-project-unsaved = { $name } • — StokesForge
window-title-project-manager = StokesForge — Gestionnaire de projet
window-title-detached = StokesForge — { $name }

## Boîtes de fichier du projet (#127)

project-open-title = Ouvrir un projet
project-save-title = Enregistrer le projet
project-file-filter = Projet StokesForge

## Menus et actions (#42)

menu-file = Fichier
menu-edit = Édition
menu-workbook = Classeur
menu-graph = Graphique
menu-view = Affichage
menu-window = Fenêtre
menu-new = Nouveau
menu-import = Importer
menu-properties = Propriétés
menu-insert-object = Insérer un objet
menu-export = Exporter
menu-sheet = Feuille
menu-columns = Colonnes
menu-language = Langue

action-add-workbook = Ajouter un classeur
    .hint = Ajoute un classeur avec des colonnes X et Y
action-new-project = Nouveau projet
    .hint = Crée un projet vide dans cette fenêtre
action-add-graph = Ajouter un graphique
    .hint = Ajoute un graphique vide avec une zone de tracé
action-add-folder = Ajouter un dossier
    .hint = Ajoute un dossier vide, puis permet de le nommer
unsaved-title = Modifications non enregistrées
unsaved-new = Enregistrer les modifications de ce projet avant d'en créer un nouveau ?
unsaved-open = Enregistrer les modifications de ce projet avant d'en ouvrir un autre ?
unsaved-quit = Enregistrer les modifications de ce projet avant de quitter ?
unsaved-discard = Abandonner les modifications
    .hint = Perd les modifications depuis le dernier enregistrement ; cette action ne peut pas être annulée
unsaved-save = Enregistrer
    .hint = Enregistre le projet, puis continue (Entrée)
alert-save-failed-title = Projet non enregistré
alert-save-failed = Le projet n'a pas pu être enregistré. Ses modifications sont conservées.
alert-save-as = Enregistrer sous…
    .hint = Enregistre le projet dans un autre fichier
alert-open-failed-title = Projet non ouvert
alert-open-failed = Le projet n'a pas pu être ouvert. Le projet courant n'a pas changé.
alert-open-another = Ouvrir un autre…
    .hint = Choisit un autre fichier de projet
alert-session-lost-title = Session interrompue
alert-session-lost = Le moteur du projet s'est arrêté de façon inattendue. Les modifications faites depuis le dernier enregistrement sont perdues. StokesForge va se fermer.
alert-session-lost-recovered = Une copie de secours du dernier état affiché a été enregistrée dans { $path }. Elle peut ne pas contenir votre dernière modification.
alert-session-lost-no-recovery = Aucune copie de secours n'a pu être écrite : { $detail }

alert-low-r2-title = R² faible
alert-low-r2 = Des résultats suivent mal les données (R² inférieur à 0,6). Vérifiez le modèle et les valeurs de départ.
low-r2-summary-head = R² < 0,6 pour :
low-r2-summary-see-results = Les valeurs sont dans le volet Résultats.
low-r2-count-baseline =
    { $count ->
        [one] { $count } correction
       *[other] { $count } corrections
    }
low-r2-count-correct =
    { $count ->
        [one] { $count } correction par référence
       *[other] { $count } corrections par référence
    }
low-r2-count-integrate =
    { $count ->
        [one] { $count } intégration
       *[other] { $count } intégrations
    }
low-r2-count-fit =
    { $count ->
        [one] { $count } ajustement
       *[other] { $count } ajustements
    }
low-r2-count-peaks =
    { $count ->
        [one] { $count } décomposition en pics
       *[other] { $count } décompositions en pics
    }
low-r2-count-reconvolution =
    { $count ->
        [one] { $count } reconvolution
       *[other] { $count } reconvolutions
    }
low-r2-count-direct-deconvolution =
    { $count ->
        [one] { $count } déconvolution directe
       *[other] { $count } déconvolutions directes
    }
status-low-r2-badge =
    { $count ->
        [one] { $count } R² faible
       *[other] { $count } R² faibles
    }
status-low-r2-badge-hint = Des calculs suivent mal les données (R² inférieur à 0,6). Cliquer pour les lister par type.
overwrite-title = Remplacer le fichier
overwrite-confirm = « { $name } » existe déjà. Le remplacer ?
overwrite-replace = Remplacer
    .hint = Écrase le fichier existant ; cette action ne peut pas être annulée
discarded-save-title = Remplacer le fichier
discarded-save-confirm = « { $name } » a été lu sans { $what }. L'enregistrer le remplace sans eux : les données écartées seront perdues.
discarded-save-replace = Remplacer
    .hint = Enregistre le projet sur le fichier ; les données écartées ne pourront pas être récupérées
action-import = Importer…
    .hint = Lit des fichiers d'appareil dans des feuilles
action-refresh-from-source = Actualiser depuis la source
    .hint = Relit les fichiers source des colonnes de la feuille active
action-reopen-last-import = Rouvrir le dernier import…
    .hint = Rouvre l'assistant d'import avec les derniers fichiers et le dernier modèle
action-open = Ouvrir…
action-save = Enregistrer
action-save-as = Enregistrer sous…
action-quit = Quitter

menu-graph-properties = Propriétés du graphique…
    .hint = Modifie le titre et le fond
menu-graph-plot-area = Zone de tracé…
    .hint = Modifie le titre de la zone de tracé et le quadrillage
menu-graph-axes = Axes…
    .hint = Modifie les titres, les échelles et les plages des axes
menu-graph-series = Séries…
    .hint = Modifie le type de tracé, le trait et le symbole de chaque série
menu-graph-legend = Légende…
    .hint = Modifie l'affichage et la position de la légende
menu-graph-theme = Thème…
    .hint = Modifie les couleurs globales et applique des thèmes prédéfinis
menu-graph-png-resolution = Résolution PNG
    .hint = Facteur d'échelle des exports PNG
menu-graph-png-resolution-option = ×{ $scale } ({ $width } × { $height } px)
menu-graph-svg-text = Texte SVG
    .hint = Façon d'écrire le texte dans les exports SVG
menu-graph-svg-text-text = Texte
    .hint = Garde le texte modifiable dans le SVG ; la police doit être installée pour l'afficher
menu-graph-svg-text-outlines = Contours
    .hint = Dessine le texte en contours ; le fichier s'affiche partout pareil

action-reset-zoom = Réinitialiser le zoom
    .hint = Revient aux plages d'axes d'origine
action-add-text = Ajouter un texte
    .hint = Ajoute un texte dans la zone de tracé
    .disabled = Clic droit dans une zone de tracé
menu-new-layer = Nouveau calque
    .disabled = Trois calques superposés au plus
    .hint = Ajoute un calque au-dessus du calque actif, avec ses propres axes
action-mirror-axis-top = Axe miroir du haut
    .hint = Répète l'échelle du bas le long du haut du calque actif, sans titre
action-mirror-axis-right = Axe miroir de droite
    .hint = Répète l'échelle de gauche le long de la droite du calque actif, sans titre
action-new-layer-top-x = X en haut (Y lié)
    .hint = Ajoute un calque avec son propre axe X en haut ; son échelle Y suit celle du calque actif
action-new-layer-right-y = Y à droite (X lié)
    .hint = Ajoute un calque avec son propre axe Y à droite ; son échelle X suit celle du calque actif
action-new-layer-top-x-right-y = X en haut, Y à droite
    .hint = Ajoute un calque avec son propre axe X en haut et son propre axe Y à droite
menu-new-layer-beside = Calque à côté
    .hint = Ajoute un calque vide à côté du calque actif, dans la grille du graphique
    .disabled = La grille de calques est pleine
action-new-layer-beside-right = À droite
    .hint = Ajoute un calque à droite du calque actif, dans la même ligne
action-new-layer-beside-below = En dessous
    .hint = Ajoute un calque en dessous du calque actif, dans la même colonne
action-edit-text = Modifier le texte
action-reset-text-position = Réinitialiser la position
    .hint = Remet ce titre ou cette légende à sa place par défaut
action-reset-text-positions = Réinitialiser la position des textes
    .hint = Remet tous les titres et la légende à leur place par défaut
    .disabled = Aucun titre ni aucune légende n'a été déplacé
action-bring-to-front = Amener devant
    .hint = Place l'objet sélectionné au-dessus des autres objets
    .disabled = Sélectionnez d'abord un objet
action-send-to-back = Envoyer derrière
    .hint = Place l'objet sélectionné sous les autres objets
    .disabled = Sélectionnez d'abord un objet
action-layer-bring-to-front = Amener devant
    .hint = Dessine le calque par-dessus les autres calques
    .disabled = Le calque est déjà devant
action-layer-send-to-back = Envoyer derrière
    .hint = Dessine le calque derrière les autres calques
    .disabled = Le calque est déjà derrière
action-properties = Propriétés…
    .hint = Ouvre les propriétés du graphique sur cet élément
action-export-graph = Exporter le graphique…
    .hint = SVG, PDF ou PNG, au zoom courant
    .disabled = Mettez d'abord une fenêtre de graphique au premier plan
action-copy-graph = Copier le graphique
    .hint = Copie le graphique comme image (SVG, PNG et PDF) pour le coller dans une autre application. Ctrl+C fait de même quand aucun objet n'est sélectionné
    .disabled = Mettez d'abord une fenêtre de graphique au premier plan

menu-window-show-metadata-row = Afficher la ligne Métadonnées
    .hint = Affiche les métadonnées des fichiers importés dans les en-têtes de colonnes
menu-workbook-show-x-prime-row = Afficher la ligne x'
    .hint = Affiche la ligne x' dans les en-têtes de colonnes de ce classeur
action-maximize-window = Agrandir la fenêtre
    .hint = Remplit l'espace de travail avec la fenêtre active
action-restore-window = Restaurer « { $title } »
    .hint = Réaffiche toutes les fenêtres
action-detach-project-manager = Détacher le gestionnaire de projet
    .hint = Place le gestionnaire de projet dans sa propre fenêtre
action-reattach-project-manager = Réattacher le gestionnaire de projet
    .hint = Replace le gestionnaire de projet dans la fenêtre principale

action-cut = Couper
action-copy = Copier
    .disabled = Les blocs sélectionnés ne partagent ni lignes ni colonnes : ils ne se copient pas ensemble
action-copy-with-header = Copier avec l'en-tête
    .hint = Copie les valeurs et l'en-tête de colonne
grid-drag-rows =
    { $count ->
        [one] { $count } ligne
       *[other] { $count } lignes
    }
graph-row-series = Ligne { $number }
graph-series-broken-reference = (référence rompue)
graph-series-broken-reference-named = { $name } — référence rompue
graph-legend-more = { $count ->
    [one] … et { $count } autre
   *[other] … et { $count } autres
    }
status-row-no-x-prime = Aucune colonne de cette feuille n'a de valeur x' : rien à tracer
row-drop-title = Tracer les lignes
row-drop-confirm = Ce graphique a déjà un axe X. Tracer les lignes contre un nouvel axe X, ou contre l'axe présent ?
row-drop-new-axis = Créer un axe
    .hint = Ajoute un calque avec son propre axe X, gradué par les valeurs x'
row-drop-present-axis = Utiliser l'axe présent
    .hint = Trace les lignes contre l'axe X du calque actif
grid-drag-columns =
    { $count ->
        [one] { $count } colonne
       *[other] { $count } colonnes
    }
action-select-all = Tout sélectionner
    .hint = Sélectionne toutes les colonnes de la feuille, en-têtes compris
action-paste = Coller
    .disabled = Sélectionner un seul bloc de cellules ou des colonnes contiguës pour coller
action-paste-in-place = Coller à l'identique
    .hint = Colle à la position d'origine de la copie
    .disabled = Copier d'abord depuis la grille. Un texte venu d'ailleurs n'a pas de position d'origine
action-clear = Effacer
    .hint = Vide les cellules sélectionnées
action-remove-formula-keep-data = Retirer la formule, garder les données
    .hint = Retire la formule et garde les valeurs calculées comme données ordinaires
    .disabled = Sélectionnez une colonne qui a une formule
column-move = Déplacer
action-move-columns-first = Déplacer au début
    .hint = Déplace les colonnes sélectionnées au début de la feuille, dans leur ordre
    .disabled = Les colonnes sélectionnées sont déjà en tête
action-move-columns-left = Déplacer vers la gauche
    .hint = Déplace chaque colonne sélectionnée d'une place vers la gauche
    .disabled = Une colonne sélectionnée est déjà la première
action-move-columns-right = Déplacer vers la droite
    .hint = Déplace chaque colonne sélectionnée d'une place vers la droite
    .disabled = Une colonne sélectionnée est déjà la dernière
action-move-columns-last = Déplacer à la fin
    .hint = Déplace les colonnes sélectionnées à la fin de la feuille, dans leur ordre
    .disabled = Les colonnes sélectionnées sont déjà en fin de feuille
action-reset-column-widths = Réinitialiser la largeur
    .hint = Remet les colonnes sélectionnées à la largeur par défaut
    .disabled = Les colonnes sont déjà à la largeur par défaut
action-delete-columns = Supprimer
    .hint = Retire les colonnes sélectionnées de la feuille. Annuler les rend avec leurs valeurs
action-delete-shift-up = Supprimer et remonter
    .hint = Supprime tout de suite et remonte les cellules. La touche Suppr efface d'abord, puis supprime

shortcut-backspace = Retour arrière
shortcut-delete = Suppr

tabs-previous-worksheets = Afficher les feuilles précédentes
tabs-next-worksheets = Afficher les feuilles suivantes

detached-workbook-unavailable = Classeur indisponible dans cette fenêtre. Le réattacher à la fenêtre principale
detached-note-unavailable = Note indisponible dans cette fenêtre. La réattacher à la fenêtre principale


window-kind-workbook = Classeur
window-kind-graph = Graphique
window-kind-matrix = Matrice
window-kind-note = Note
window-deleted = { $kind } { $id } (élément supprimé)

status-ready = Prêt
status-history = Afficher les 50 derniers messages
status-history-title = Messages
status-history-empty = Aucun message pour l'instant. Les messages de vos actions apparaîtront ici
status-history-time = { $time } UTC
status-level-warning = Avertissement
status-level-error = Erreur
status-saved = Enregistré dans { $path }
status-saved-over-limit = Enregistré dans { $path }, mais le fichier ({ $size } Mo) dépasse la limite d'ouverture de { $limit } Mo. Relever la limite dans Fichier > Préférences > Général pour le rouvrir
status-loaded = Chargé depuis { $path }
status-load-discarded-calculations = { $count } { $count ->
        [one] calcul
       *[other] calculs
    }
status-load-discarded-corrections = { $count } { $count ->
        [one] trace de correction
       *[other] traces de correction
    }
status-load-discarded-and = { $first } et { $second }
status-load-discarded = { $what } { $total ->
        [one] a été écarté
       *[other] ont été écartés
    } du projet ({ $ids })
status-load-discarded-corrections-only = { $what } { $total ->
        [one] a été écartée
       *[other] ont été écartées
    } du projet ({ $ids })
status-nothing-to-undo = Rien à annuler
status-nothing-to-redo = Rien à rétablir
status-clipboard-unreadable = Presse-papiers vide ou illisible
status-paste-non-contiguous = Impossible de coller dans une sélection non contiguë
status-paste-columns-dropped =
    { $count ->
        [one] Une colonne ne tenait pas dans la feuille et n'a pas été collée
       *[other] { $count } colonnes ne tenaient pas dans la feuille et n'ont pas été collées
    }
status-cell-decimal-comma = « { $text } » a été lu { $value } (virgule décimale)
status-paste-fields-dropped =
    { $count ->
        [one] Un champ d'en-tête ne tenait pas dans la feuille et n'a pas été collé
       *[other] { $count } champs d'en-tête ne tenaient pas dans la feuille et n'ont pas été collés
    }
status-paste-columns-fields-dropped = { $columns } colonne(s) et { $fields } champ(s) d'en-tête ne tenaient pas dans la feuille et n'ont pas été collés
status-graph-copied = Graphique copié en { $formats }
status-graph-copied-partly = Graphique copié en { $formats } ; { $missing } n'a pas pu être préparé
status-language-not-saved = Impossible d'enregistrer le choix de la langue : { $detail }

## Assistant d'import (#44)
## Les noms de modèles et le contenu des fichiers sont des données : jamais traduits.

import-window-title = Assistant d'import
import-button-back = Précédent
import-button-next = Suivant
import-button-finish = Terminer
    .hint = Importe avec les réglages actuels (Entrée)

import-step-files-title = Fichiers et modèle
import-step-format-title = Format
import-step-header-title = En-tête et fin des données
import-step-columns-title = Colonnes

# Valeurs partagées par plusieurs listes.
common-auto = Auto
common-advanced = Avancé
common-auto-value = Auto ({ $value })
import-option-none = Aucun
import-option-comma = Virgule
import-option-dot = Point
import-option-space = Espace
import-option-apostrophe = Apostrophe
import-option-other = Autre

# Types de colonne, partagés par « Lu comme » et « Type ».
column-type = Type
column-type-numeric = Numérique
column-type-text = Texte
column-type-mixed = Mixte
column-type-datetime = Date et heure

# Étape 1 : fichiers et modèle.
import-template-untitled = Sans titre
import-files-template = Modèle
import-files-template-modified = { $name } (modifié)
import-files-reset = Réinitialiser
    .hint = Repart des réglages par défaut
import-files-delete = Supprimer…
    .hint = Supprime ce modèle enregistré
import-files-delete-title = Supprimer le modèle
import-files-delete-confirm = Supprimer le modèle « { $name } » ? Cette action ne peut pas être annulée.
import-files-delete-yes = Supprimer
    .hint = Supprime le modèle enregistré ; cette action ne peut pas être annulée
import-files-export = Exporter le modèle…
    .hint = Enregistre ce modèle dans un fichier JSON
import-files-export-title = Exporter le modèle
import-files-import = Importer un modèle…
    .hint = Charge un modèle depuis un fichier JSON
import-files-import-title = Importer un modèle
import-files-template-filter = Modèle d'import
import-files-unsaved = Modifications non enregistrées. Enregistrer le modèle à l'étape Destination
import-files-add = Ajouter des fichiers…
    .hint = Choisit les fichiers à importer
import-files-add-title = Choisir les fichiers à importer
import-files-drop = ou déposer des fichiers dans la fenêtre
import-files-section = Fichiers
import-files-recent = Fichiers récents…
import-files-none = Aucun fichier choisi
import-files-row-count =
    { $count ->
        [one] { $count } ligne
       *[other] { $count } lignes
    }
import-files-remove = Retirer

# Étape 2 : format.
import-format-delimiter = Séparateur
    .hint = Caractère qui sépare les valeurs d'une ligne
import-delimiter-semicolon = Point-virgule
import-delimiter-tab = Tabulation
import-delimiter-whitespace = Espaces
import-delimiter-fixed = Largeur fixe
import-format-widths-hint = Largeur de chaque colonne sauf la dernière, p. ex. 10,8,8
import-format-decimal = Séparateur décimal
import-format-thousands = Séparateur de milliers
import-format-date-order = Ordre des dates
import-date-day-first = Jour, puis mois
import-date-month-first = Mois, puis jour
import-format-missing = Valeurs manquantes
    .hint = Séparées par des virgules. Lues comme des cellules vides : --, n/a, *, NaN…
import-format-comment-prefix = Préfixe de commentaire
    .hint = Les lignes de données qui commencent par ce préfixe sont ignorées (#, ;, //…)
import-format-blocks = Blocs
    .hint = Façon de découper un fichier qui contient plusieurs blocs de données
import-blocks-single = Bloc unique
import-blocks-line-start = Ligne commençant par
import-blocks-blank = Lignes vides
import-format-encoding = Encodage
import-format-skip-chars = Retirer les premiers caractères
    .hint = Retirés au début de chaque ligne de données
import-format-skip-lines = Ignorer des lignes de données
    .hint = Ignorées au début des données

# Étape 3 : en-tête et fin des données.
import-header-mode = En-tête
import-header-mode-lines = Nombre de lignes
import-header-mode-marker = Jusqu'à un repère
import-header-marker = Repère
    .hint = L'en-tête se termine à la ligne égale à ce texte
import-header-lines = Lignes d'en-tête
import-header-subheader-lines = Lignes de sous-en-tête
    .hint = Comptées dans les lignes d'en-tête
import-header-detect = Détecter l'en-tête
    .hint = Recompte les lignes d'en-tête et de sous-en-tête d'après le fichier
import-header-combined = Ligne combinée
    .hint = Le nom long et les unités partagent la même ligne de sous-en-tête
import-header-role-long-name = Nom long
    .hint = Lignes de sous-en-tête qui donnent les noms longs
import-header-role-units = Unités
    .hint = Lignes de sous-en-tête qui donnent les unités
import-header-role-comments = Commentaires
    .hint = Lignes de sous-en-tête qui donnent les commentaires
import-header-role-x-prime = x'
    .hint = Ligne de sous-en-tête qui donne la valeur x' de chaque colonne (par ex. son instant)
import-header-range-from = de
import-header-range-to = à
import-header-names-first-line = Noms sur la première ligne
    .hint = La première ligne de données donne les noms de colonnes
import-dataend = Fin des données
import-dataend-auto = Auto
    .hint = S'arrête au premier X non numérique
import-dataend-blank = Première ligne vide
import-dataend-marker = Repère de fin
import-dataend-after-header = Lignes après l'en-tête
import-dataend-before-end = Lignes avant la fin
import-header-skip-footer = Ignorer des lignes de pied de page
    .hint = Ignorées à la fin du fichier
import-column-names = Noms des colonnes
import-header-name-file = Nom du fichier
import-header-name-key = Clé d'en-tête
import-header-key = Clé

# Étape 4 : colonnes.
import-columns-name = Nom
import-columns-read-as = Lu comme
import-columns-type = Type
import-columns-values = Valeurs
import-columns-choose-file = Choisir un fichier à l'étape 1

# Étape 5 : métadonnées.
import-metadata-keep-header = Garder l'en-tête
    .hint = Conserve l'en-tête du fichier comme métadonnées
import-metadata-keep-footer = Garder le pied de page
    .hint = Conserve le pied de page du fichier comme métadonnées
import-metadata-uncheck = Décocher une clé ou une section pour l'écarter
import-metadata-lines-section = Métadonnées du fichier
import-metadata-none = Aucune métadonnée avec les réglages actuels

# Étape 6 : destination et modèle.
import-destination = Destination
import-destination-active = Classeur actif (une feuille par fichier)
import-destination-active-none = Classeur actif (aucun)
import-destination-new-per-file = Un classeur par fichier
import-destination-new-all = Un classeur pour tous les fichiers
import-destination-mode = Dans le classeur actif
import-mode-new-sheet = Nouvelle feuille
import-mode-replace = Remplacer la feuille active
import-mode-add-columns = Ajouter des colonnes à la feuille active
import-mode-add-rows = Ajouter des lignes à la feuille active
import-destination-multiple = Plusieurs fichiers
import-layout-per-file = Une feuille par fichier
import-layout-side-by-side = Une feuille, côte à côte (X Y X Y…)
import-layout-shared-x = Une feuille, X du premier fichier partagé
import-destination-template-name = Nom du modèle
import-destination-template-section = Enregistrer comme modèle
import-destination-save = Enregistrer le modèle
import-destination-saved-note = Les modèles enregistrés sont proposés au prochain import
import-destination-auto = Sélection auto
    .hint = Choisit ce modèle pour un fichier qui répond à tous les critères renseignés
import-auto-extensions = Extensions
    .hint = csv, txt, asc…
import-auto-name = Le nom contient
import-auto-content = Le début du fichier contient
    .hint = Une signature, p. ex. ORIGIN;JASCO

# Volet d'aperçu.
import-preview-title = Aperçu
import-preview-encoding = Encodage { $encoding }
import-preview-blocks = { $count } blocs, le premier est affiché
import-preview-tip = Clic droit sur une ligne pour y régler l'import
import-preview-empty = Choisir un fichier pour voir un aperçu
import-preview-showing =
    { $total ->
        [one] Affichage de { $shown } sur { $total } ligne de données
       *[other] Affichage de { $shown } sur { $total } lignes de données
    }
import-legend-header = En-tête
import-legend-data = Données
import-legend-footer = Pied de page
import-legend-comment-line = Ligne de commentaire
import-legend-skipped = Ligne ignorée
import-line-data-start = Commencer les données ici
import-line-units = Utiliser comme ligne d'unités
import-line-data-end = Terminer les données ici
import-line-no-auto-end = Désactiver la fin automatique des données

## Entrées d'historique (#46)
## Une clé par variante de `HistoryLabel`. Une commande à l'infinitif ; un nom
## choisi par l'utilisateur est entre « » ; une colonne est sa lettre de grille.

history-set-cells = Modifier les cellules
history-set-cells-in-column = Modifier les cellules de la colonne { $column }
history-add-column = Ajouter une colonne
history-delete-column = Supprimer la colonne
history-delete-column-named = Supprimer la colonne { $column }
history-rename = Renommer « { $from } » en « { $to } »
history-rename-unnamed = Renommer en « { $to } »
history-move = Déplacer
history-move-named = Déplacer « { $name } »
history-set-column-type = Changer le type de la colonne
history-set-column-type-of = Changer le type de la colonne { $column }
history-insert-rows =
    Insérer { $count } { $count ->
        [one] ligne
       *[other] lignes
    }
history-delete-rows =
    Supprimer { $count } { $count ->
        [one] ligne
       *[other] lignes
    }
history-delete-cells =
    Supprimer des cellules et remonter ({ $count } { $count ->
        [one] ligne
       *[other] lignes
    })
history-add-folder = Ajouter le dossier « { $name } »
history-add-workbook = Ajouter un classeur
history-add-empty-workbook = Ajouter un classeur vide
history-add-worksheet = Ajouter une feuille
history-duplicate-worksheet = Dupliquer la feuille
history-duplicate-worksheet-named = Dupliquer la feuille « { $name } »
history-add-graph = Ajouter un graphique
history-add-matrix = Ajouter une matrice
history-add-note = Ajouter une note
history-remove-node = Supprimer
history-remove-node-named = Supprimer « { $name } »
history-set-window-meta = Modifier les propriétés de la fenêtre
history-set-window-meta-named = Modifier les propriétés de « { $name } »
history-set-note-text = Modifier la note
history-set-note-text-named = Modifier la note « { $name } »
history-set-column-meta = Modifier les propriétés de la colonne
history-set-column-meta-of = Modifier les propriétés de la colonne { $column }
history-set-plot-spec = Modifier le graphique
history-set-plot-spec-named = Modifier le graphique « { $name } »
history-set-saved-colors = Modifier les couleurs enregistrées
history-macro-paste-cells = Coller
history-macro-clear-selection = Effacer la sélection
history-macro-graph-properties = Modifier les propriétés du graphique
history-macro-refresh-from-file = Actualiser depuis le fichier
history-macro-import = Importer
history-macro-add-workbook = Ajouter un classeur
history-macro-add-worksheet = Ajouter une feuille
history-macro-script = Exécuter le script
history-macro-set-columns-type = Changer le type des colonnes
history-macro-set-columns-role = Changer le rôle des colonnes
history-macro-analysis-to-graph = Analyser et tracer
history-macro-remove-formula = Retirer la formule
history-macro-delete-columns = Supprimer des colonnes
history-macro-move-columns = Déplacer des colonnes
history-macro-analyze-rows = Analyser des lignes
history-transpose-rows = Transposer les lignes
history-recalculate-transpose = Recalculer la feuille transposée

## Annuler et rétablir

history-undo = Annuler
history-undo-entry = Annuler « { $label } »
history-redo = Rétablir
history-redo-entry = Rétablir « { $label } »
history-undone = Annulé : { $label }
history-undone-last = Dernière action annulée
history-redone = Rétabli : { $label }
history-redone-last = Dernière action rétablie

## Erreurs (#47)
## Le texte brut des crates du dessous reste en anglais ; chaque variante est
## associée à l'une de ces clés dans `src/errors.rs`. Ce qui s'est passé, puis
## quoi faire (seulement si utile). `{ $detail }` est la cause brute, non traduite.

# Modèle de projet
error-model-invalid-name = Nom invalide : { $detail }
error-model-duplicate-name = Nom déjà utilisé : { $detail }
error-model-invalid-path = Chemin invalide : { $detail }
error-model-invalid-structure = Opération non autorisée : { $detail }
error-model-unknown-object = Cet objet n'existe plus dans le projet
error-model-index-out-of-bounds = Position hors limites : { $detail }
error-model-incompatible-values = Valeurs incompatibles : { $detail }
error-model-ids-exhausted = Plus aucun objet ne peut être créé dans ce projet
error-model-concurrent-edit = La colonne a changé pendant sa modification. Réessayer
error-model-formula-column-read-only = Cette colonne est calculée par une formule. Modifier la formule pour changer ses valeurs

# Actions groupées et historique
error-command-no-group = Aucune action groupée en cours
error-command-busy = Indisponible pendant une autre action. La terminer d'abord
error-command-limit-not-empty = La limite de l'historique ne peut changer que lorsqu'il est vide
error-command-group-too-deep = Les actions groupées sont imbriquées trop profondément (limite : { $limit })
error-command-foreign-group = Une autre action est en cours. Attendre sa fin, puis réessayer

# Moteur
error-session-closed = Le projet n'est plus disponible. Redémarrer StokesForge

# Fichiers de projet
error-file-io = Impossible d'accéder à { $path } : { $detail }
error-file-not-stokesforge = { $path } n'est pas un fichier StokesForge
error-file-too-new = { $path } a été enregistré par une version plus récente de StokesForge (format de fichier { $found }, cette version lit jusqu'au format { $supported }) : mettez à jour StokesForge pour l'ouvrir
error-file-too-many-cells = { $path } déclare trop de cellules ({ $cells } millions, limite { $limit } millions). Relever la limite dans Fichier > Préférences > Général, puis rouvrir le fichier
error-file-too-large = { $path } est trop volumineux pour être ouvert ({ $size } Mo, limite { $limit } Mo). Relever la limite dans Fichier > Préférences > Général, puis rouvrir le fichier
error-file-corrupt = { $path } est endommagé : { $detail }
error-file-invalid-project = { $path } ne contient pas de projet valide : { $detail }
error-file-unknown-feature = { $path } utilise des fonctions d'une version plus récente de StokesForge ({ $detail })
error-file-changed = { $path } a été modifié ailleurs depuis son ouverture ou son enregistrement. Le recharger avant d'enregistrer
error-file-unreadable-target = Impossible d'enregistrer sur { $path } : c'est un fichier StokesForge illisible ({ $detail }). Utilisez Enregistrer sous avec un autre nom, ou déplacez d'abord ce fichier

# Import
error-import-binary = Le fichier n'est pas un texte lisible
error-import-empty = Le fichier est vide
error-import-no-data = Aucune donnée après l'en-tête. Vérifier le nombre de lignes d'en-tête
error-import-whitespace-thousands = L'espace ne peut pas à la fois séparer les colonnes et grouper les milliers. Choisir un autre séparateur de colonnes ou de milliers
error-import-file = { $path } : { $detail }

# Formules
error-formula-invalid = Formule invalide : { $detail }
error-formula-unknown-column = La colonne { $letter } n'existe pas dans cette feuille
error-formula-self-reference = Une formule ne peut pas faire référence à sa propre colonne ({ $letter })
error-formula-deleted-column = Colonne supprimée. Remplacer #REF par une lettre de colonne
error-formula-cycle = Des formules se renvoient l'une à l'autre en boucle. Retirer l'une des références
error-formula-too-deep = La formule est trop imbriquée. La simplifier (moins de parenthèses imbriquées ou de termes enchaînés)
error-formula-unknown-function = Fonction { $name } inconnue. Vérifier son orthographe
error-formula-unknown-function-suggestion = Fonction { $name } inconnue. Vouliez-vous écrire { $suggestion } ?
# Nombres d'arguments acceptés par une fonction (#404) : `count` est le nombre
# de valeurs acceptées, `first` et `last` la plus petite et la plus grande,
# `second` celle qui suit la première (trois valeurs : « 1, 2 ou 3 »). Un terme
# Fluent n'accepte pas de variables : la phrase est écrite dans les deux messages.
error-formula-wrong-argument-count = La fonction { $name } attend { $count ->
        [1] { $first ->
            [0] aucun argument
            [1] 1 argument
           *[other] { $first } arguments
        }
        [2] { $first } ou { $last } arguments
       *[other] { $first }, { $second } ou { $last } arguments
    } (reçu : { $found })
error-formula-wrong-argument-count-suggestion = La fonction { $name } attend { $count ->
        [1] { $first ->
            [0] aucun argument
            [1] 1 argument
           *[other] { $first } arguments
        }
        [2] { $first } ou { $last } arguments
       *[other] { $first }, { $second } ou { $last } arguments
    } (reçu : { $found }). Écrire plutôt { $suggestion }
error-formula-not-a-function = { $letter } est une colonne, pas une fonction. Pour multiplier, écrire { $letter }*(...)
error-formula-pi-needs-parentheses = Pour le nombre π, écrire pi() (un pi seul serait la colonne PI)
error-formula-column-expected = La fonction { $name } lit une colonne entière. Lui donner une lettre de colonne, pas un calcul (B, pas B*2)
error-formula-no-paired-x = La colonne lue par cette formule n'a pas de colonne X. Placer à sa gauche une colonne de rôle X
error-formula-no-point = La formule n'a trouvé aucun point à calculer : la colonne ou la plage est vide, ou la valeur demandée est hors des X. Vérifier les bornes
error-formula-unknown-sheet = Il n'y a pas de feuille nommée { $name } dans ce classeur. Vérifier son orthographe
error-formula-unknown-workbook = Il n'y a pas de classeur nommé { $name }. Vérifier son orthographe
error-formula-workbook-without-sheet = Une référence à un autre classeur nomme sa feuille. Écrire [{ $name }]Sheet1!B
error-formula-ambiguous-workbook = Plusieurs classeurs s'appellent { $name }. Écrire le chemin depuis la racine du projet, par exemple [Dossier/{ $name }]Sheet1!B
error-formula-ambiguous-name = Plusieurs objets s'appellent { $name } à la casse près. Renommer l'un d'eux
error-formula-unknown-column-in = La colonne { $column } n'existe pas dans la feuille { $sheet }
error-formula-remote-outside = Une colonne d'une autre feuille ne peut être lue que par at, max, min, mean, sum, std ou integral. La donner à l'une d'elles comme colonne
error-formula-remote-x = La colonne X d'un at() sur une autre feuille se lit dans cette feuille. L'écrire sans préfixe (D, pas Sheet3!D)

# Affichage et export du graphique
error-render-unknown-graph = Ce graphique n'existe plus
error-render-not-a-graph = Cet objet n'est pas un graphique
error-export-drawing = Impossible de dessiner le graphique : { $detail }
error-export-write = Impossible d'écrire { $path } : { $detail }
error-export-fonts = Impossible de charger les polices : { $detail }
error-export-reread = Impossible de préparer la figure pour l'export : { $detail }
error-export-pdf = Impossible de créer le PDF : { $detail }
error-export-png-scale = Résolution PNG invalide ({ $scale }). Utiliser un nombre positif
error-export-png-size = La taille du PNG est invalide ({ $width }×{ $height } px). Augmenter la résolution
error-export-png-encode = Impossible d'encoder le PNG : { $detail }

# Scripts
error-script-thread = Impossible de démarrer le moteur de script : { $detail }
error-script-install = Impossible de configurer le module pyst : { $detail }
error-script-busy = Un script est déjà en cours
error-script-stopped = Le moteur de script s'est arrêté. Relancer le script
error-python-home = Python est introuvable dans { $path }
error-python-init = Impossible de démarrer Python : { $detail }
error-python-trace =
    Erreur Python :
    { $trace }

## Propriétés du graphique et export (#43)
##
## Zone `plot-*` : fenêtre des propriétés du graphique (six onglets), cibles du
## clic droit, éditeur de texte sur le graphique. `dialog-*` : pied commun des
## dialogues modaux. `export-*` : dialogue d'export. Un attribut `.hint` est
## l'infobulle de son contrôle. Les noms saisis par l'utilisateur, les thèmes et
## les noms de colonnes sont des données, jamais traduits.

# Pied commun des dialogues modaux (`modal_dialog_with`)

dialog-ok =
    OK
    .hint = Conserve les modifications (Entrée)
dialog-cancel =
    Annuler
    .hint = Abandonne les modifications (Échap)
dialog-close =
    Fermer
    .hint = Ferme cette fenêtre (Entrée)

# Fenêtre et onglets

plot-window-title = Propriétés du graphique — { $name }
plot-tab-general = Général
plot-tab-plot-area = Zone de tracé
plot-tab-axes = Axes
plot-tab-series = Séries
plot-tab-legend = Légende
plot-tab-theme = Thème

# Les trois axes d'un calque

plot-axis-bottom = Bas
plot-axis-left = Gauche
plot-axis-right = Droite
plot-axis-top = Haut

# Ce que désigne un clic droit (tête du menu contextuel)

plot-target-graph = Graphique
plot-target-title = Titre
plot-target-plot-area = Zone de tracé
plot-target-axis = { $side ->
        [bottom] Axe du bas
        [left] Axe de gauche
        [right] Axe de droite
       *[top] Axe du haut
    }
plot-target-axis-title = { $side ->
        [bottom] Titre de l'axe du bas
        [left] Titre de l'axe de gauche
        [right] Titre de l'axe de droite
       *[top] Titre de l'axe du haut
    }
plot-target-text = Texte
plot-target-object = Objet
plot-target-legend = Légende

# Valeurs automatiques des champs de couleur

plot-color-auto-contrast = Auto (contraste avec le fond)
plot-color-auto-white = Auto (blanc)
plot-color-same-as-line = Comme le trait

# Onglet Général

plot-general-title =
    Titre
    .hint = Affiché au-dessus du graphique
plot-title-color = Couleur du titre
plot-general-background = Fond
plot-general-dark-note = Les couleurs auto passent au blanc sur fond sombre
plot-stale-warning = Avertir si des données ne sont pas à jour
    .hint = Grise les courbes des calculs pas à jour et les marque, à l'écran et dans les exports

plot-size = Taille
plot-size-width = Largeur de la zone de tracé ({ $unit })
plot-size-height = Hauteur de la zone de tracé ({ $unit })
plot-size-column-width = Largeur de la zone de tracé, colonne { $n } ({ $unit })
plot-size-row-height = Hauteur de la zone de tracé, ligne { $n } ({ $unit })
plot-size-layer-background = Fond du calque
plot-size-layer-background-auto = Auto (seulement sur un autre calque)
plot-size-total = Taille du graphique ({ $unit })
plot-size-total-value = { $width } × { $height }
plot-size-left = Zone de tracé, depuis le bord gauche ({ $unit })
plot-size-top = Zone de tracé, depuis le bord haut ({ $unit })
plot-size-left-raised = Les axes demandent au moins { $minimum } ({ $unit }) à gauche : la valeur a été remontée à ce minimum
plot-size-top-raised = Les axes et le titre demandent au moins { $minimum } ({ $unit }) en haut : la valeur a été remontée à ce minimum
plot-size-note = Les axes et le titre s’ajoutent autour de la zone de tracé : en ajouter ou en retirer agrandit ou réduit le graphique, jamais la zone de tracé. Les autres colonnes et lignes gardent leur taille.
plot-size-placement = Placement des calques
plot-size-placement-grid = Grille régulière
plot-size-placement-free = Libre
plot-size-position-left = Position, gauche ({ $unit })
plot-size-position-top = Position, haut ({ $unit })
plot-size-free-note = Les valeurs ci-dessus sont celles du calque choisi dans la liste. Les positions sont celles de la page : elles ne changent pas quand un autre calque bouge, et elles peuvent être négatives. Le graphique grandit ou rétrécit autour des calques.

plot-figure-size = Taille de la figure
    .hint = Grandit avec les axes : un axe ou un titre ajouté agrandit l’image. Fixe : la place de quatre axes est gardée d’avance, donc ajouter un axe laisse l’image et la zone de tracé telles quelles
plot-figure-size-grows = Grandit avec les axes
plot-figure-size-fixed = Fixe

# Onglet Zone de tracé

plot-area-layer-title =
    Titre du calque
    .hint = Affiché au-dessus de la zone de tracé
plot-area-gridlines = Quadrillage
plot-area-gridlines-vertical = Verticales (graduations X)
plot-area-gridlines-horizontal = Horizontales (graduations Y)

# Onglet Axes

plot-axes-title-section = Titre de l'axe
plot-axes-quantity =
    Grandeur
    .hint = Nom ou symbole, p. ex. Nombre d'onde
plot-axes-units =
    Unités
    .hint = Affichées après la grandeur, entre parenthèses
plot-axes-scale-section = Échelle
plot-axes-log-scale =
    Échelle log
    .hint = Logarithme en base 10
plot-axes-from = De
plot-axes-from-auto = Auto
    .hint = Les bornes suivent de nouveau les données. L'axe garde son sens
    .disabled = Les bornes suivent déjà les données
plot-axes-display-section = Affichage
plot-axes-decimal-comma = Virgule décimale sur les graduations
    .hint = Écrit 1,5 au lieu de 1.5 sur tous les axes de ce graphe, à l'écran et dans les exports
plot-axes-show-axis = Afficher l'axe
plot-axes-own-scale =
    Échelle propre
    .hint = Donne à ce calque sa propre échelle de ce côté. Décochée, il suit l'échelle du calque qu'il recouvre
plot-axes-show-gridlines = Afficher le quadrillage
plot-axes-show-tick-labels = Afficher les étiquettes de graduation
plot-axes-show-ticks = Afficher les graduations
plot-axes-show-minor-ticks = Afficher les sous-graduations
plot-axes-minor-tick-count = Sous-graduations par intervalle
    .auto = Auto
    .hint = Nombre de petits tirets entre deux graduations principales. Vide = Auto. Sans effet en échelle logarithmique, qui montre toujours 2 à 9
plot-axes-ticks-section = Graduations

plot-axes-tick-length = Longueur des grands tirets (px)
plot-axes-minor-tick-length = Longueur des petits tirets (px)
plot-axes-tick-width = Épaisseur des tirets (pt)
plot-axes-tick-mark-color = Couleur des tirets
    .auto = Auto (couleur de l'axe)
plot-axes-label-every = Un nombre sur
    .hint = N'écrit qu'un nombre sur ce nombre, en commençant par le premier. 1 les écrit tous
plot-axes-tick-direction = Sens des graduations
plot-axes-tick-out = Vers l'extérieur
plot-axes-tick-in = Vers l'intérieur
plot-axes-tick-both = Des deux côtés
plot-axes-show-mirror = Afficher l'axe miroir
plot-axes-mirror-note = Reprend l'échelle de l'axe opposé (bas ou gauche), sans titre
plot-axes-color = Couleur de l'axe
plot-axes-color-note = Trait de l'axe, graduations et étiquettes de graduation. Le quadrillage la reprend, estompée

# Onglet Séries

plot-series-empty = Ce graphique n'a aucune série. Glisser des colonnes d'un classeur dessus
plot-series-name =
    Nom
    .hint = Vide : reprend tel quel le nom de la colonne tracée
plot-series-plot-type = Type de tracé
plot-series-type-unknown = Type inconnu (d'une version plus récente)
plot-series-bins = Classes
plot-series-axis = Axe Y
    .hint = L'axe vertical contre lequel cette série est tracée. Droite fait apparaître l'axe de droite.
    .disabled = Un calque superposé n'a qu'un seul axe Y ; ce réglage ne s'applique pas
plot-series-y-error = Erreur Y
    .hint = La colonne d'incertitudes tracée en barres d'erreur verticales, quel que soit le type de tracé. Aucun : pas de barres d'erreur. Par défaut, la colonne Erreur Y à droite de la colonne Y.
plot-series-x-error = Erreur X
    .hint = La colonne d'incertitudes tracée en barres d'erreur horizontales, quel que soit le type de tracé. Aucun : pas de barres d'erreur. Par défaut, la colonne Erreur X à droite de la colonne Y.
plot-series-error-none = Aucun
plot-series-bar-width = Largeur (unités X)
plot-series-show = Afficher la série
plot-series-show-in-legend = Afficher dans la légende
plot-series-line-section = Trait
plot-series-line-color = Couleur
plot-series-line-style = Style de trait
plot-series-line-width = Épaisseur (pt)
plot-series-symbol-section = Symbole
plot-series-symbol-shape = Forme
plot-series-symbol-size = Taille (pt)
plot-series-symbol-color = Couleur du symbole

# Fenêtre des séries et groupes (#177–#179)

curve-window-title = Propriétés des séries — { $name }
curve-group-name = Groupe { $number }
curve-group-section = Groupe
curve-group-field = Groupe
curve-group-none = Aucun
curve-group-new = Nouveau groupe
curve-group-entry = { $group } ({ $count })
curve-group-leave =
    Quitter le groupe
    .hint = Détache cette série de son groupe ; elle garde son style actuel
    .disabled = Cette série n'est dans aucun groupe
curve-group-name-field =
    Nom
    .hint = Nom du groupe dans l'Object Manager et la liste des séries. Laisser vide pour le nom automatique.
curve-group-ungroup =
    Dissoudre le groupe
    .hint = Chaque série du groupe devient une série simple ; elle garde son style
curve-group-delete =
    Supprimer le groupe
    .hint = Retire du graphique le groupe et toutes ses séries
curve-group-move-up =
    Monter le groupe
    .hint = Remonte tout le groupe dans l'ordre de tracé
curve-group-move-down =
    Descendre le groupe
    .hint = Fait descendre tout le groupe dans l'ordre de tracé
curve-scope-field = Modifier
curve-scope-series = Cette série seulement
curve-scope-group = Tout le groupe ({ $count } séries)
curve-scope-group-note = Le style et l'épaisseur du trait, la forme et la taille des symboles s'appliquent à toutes les séries du groupe. Le nom, le type, la visibilité et les couleurs restent propres à chaque série ; répartissez les couleurs avec le nuancier ci-dessous.
curve-scope-series-note = Les changements ne touchent que cette série. Elle reste dans son groupe ; « Quitter le groupe » la détache.
curve-scope-locked = Réglage propre à une série : choisissez « Cette série seulement » pour le modifier
curve-gradient-locked = Choisissez « Tout le groupe » pour appliquer un nuancier
curve-gradient-saved = { $name } ({ $target })
curve-gradient-delete =
    Supprimer
    .hint = Supprime ce nuancier enregistré
    .disabled = Seul un nuancier enregistré peut être supprimé
curve-gradient-name-field =
    Nom
    .hint = Nom du nuancier
curve-gradient-save =
    Enregistrer
    .hint = Enregistre le nuancier personnalisé dans le projet ou pour l'utilisateur
    .disabled = Donnez d'abord un nom au nuancier
curve-gradient-section = Nuancier
curve-gradient-field = Nuancier
curve-gradient-custom = Personnalisé
curve-gradient-stop = Couleur { $number }
curve-gradient-add-stop = Ajouter une couleur
curve-gradient-remove-stop = Retirer une couleur
curve-gradient-blue-red = Bleu vers Rouge
curve-gradient-black-red = Noir vers Rouge
curve-gradient-black-blue = Noir vers Bleu
curve-gradient-blue-yellow = Bleu vers Jaune
curve-gradient-green-red = Vert vers Rouge
curve-gradient-blue-white-red = Bleu vers Blanc vers Rouge
curve-gradient-viridis = Viridis
curve-gradient-plasma = Plasma

plot-type-scatter = Points
plot-type-line = Ligne
plot-type-line-symbol = Ligne + symboles
plot-type-bar = Barres
plot-type-histogram = Histogramme
plot-type-error-bars = Ligne + barres d'erreur
plot-type-unknown = Type inconnu

plot-line-style-none = Aucun
plot-line-style-solid = Continu
plot-line-style-dashed = Tirets
plot-line-style-dotted = Pointillés
plot-line-style-dash-dot = Tiret-point

plot-symbol-none = Aucun
plot-symbol-circle = Cercle
plot-symbol-square = Carré
plot-symbol-triangle = Triangle
plot-symbol-diamond = Losange
plot-symbol-plus = Plus (+)
plot-symbol-cross = Croix (×)

# Onglet Légende

plot-legend-show = Afficher la légende
plot-legend-position = Position
plot-legend-top-left = Haut gauche
plot-legend-top-right = Haut droite
plot-legend-bottom-left = Bas gauche
plot-legend-bottom-right = Bas droite

# Onglet Thème

plot-theme-themes = Thèmes
plot-theme-light = Clair
plot-theme-dark = Sombre
plot-theme-global-colors = Couleurs globales
plot-theme-background = Fond
plot-theme-text = Texte
plot-theme-axes = Axes
plot-theme-note = S'applique à tous les titres, axes, graduations et quadrillages. Remplace les couleurs réglées dans les autres onglets

# Mémo de la syntaxe Typst, sous chaque champ de titre

plot-math-memo-header = Syntaxe mathématique
plot-math-greek = Lettres grecques λ, Δ
plot-math-subscript = Indice
plot-math-superscript = Exposant
plot-math-full = Maths à plusieurs termes
plot-math-upright = Texte droit en mode mathématique
plot-math-format = Mise en forme du texte
plot-math-escape = Le caractère lui-même
plot-math-footer = Le texte entre $ … $ est en mode mathématique, le reste est du texte simple. C'est la syntaxe de Typst ; référence complète : { $url }

# Texte sur le graphique (éditeur en place) et vue du graphique

plot-text-size = Taille de police (pt)
plot-text-rotation = Rotation (°)
plot-text-align =
    Alignement
    .hint = Aligne le texte dans sa boîte (fixez la largeur avec les poignées)
plot-text-align-left = Gauche
plot-text-align-center = Centré
plot-text-align-right = Droite
plot-text-ok =
    OK
    .hint = Applique le texte (Entrée)
plot-text-placeholder = Texte, $maths$
plot-view-reset-zoom = Réinitialiser le zoom
plot-layer-badge = { $n }
    .hint = Fait du calque { $n } la cible des ajouts à ce graphique
plot-layer-choice-disabled = Cette page vaut pour tout le graphique, pas pour un calque

# Export

export-dialog-title = Exporter le graphique

## Classeur, gestionnaire de projet, notes, couleurs, console (#45)

# Gestionnaire de projet
project-manager-title = Gestionnaire de projet
project-manager-open = Ouvrir
    .hint = Ouvre les fenêtres sélectionnées, ou les amène au premier plan si elles sont déjà ouvertes
project-manager-close = Fermer
    .hint = Ferme les fenêtres sélectionnées, comme la croix de leur barre de titre
project-folder-empty = (vide)

# Classeur : en-tête de colonne empilé (gouttière de gauche)
column-x-prime = x'
status-x-prime-invalid = x' doit être un nombre fini
column-long-name = Nom long
column-units = Unités
column-comments = Commentaires
workbook-header-formula = F(x)=
# Aide de la case F(x)= (#404) : les fonctions qu'une formule de colonne peut appeler
formula-help-title = Fonctions de formule
formula-help-icon = Fonctions de formule
    .hint = Ouvre la liste des fonctions utilisables dans une formule
formula-help-column-function = Fonction
formula-help-column-example = Exemple
formula-help-column-meaning = Ce qu'elle fait
formula-help-note = Les noms ne distinguent pas majuscules et minuscules. Une colonne s'écrit par sa lettre.
formula-help-family-operators = Opérateurs
formula-help-op-arithmetic = Additionner, soustraire, multiplier, diviser
formula-help-op-power = Puissance
formula-help-op-comparison = Comparaison : 1 si vrai, 0 si faux
formula-help-reference = Colonne d'une autre feuille, lue par les fonctions de colonne (at, max, min, mean, sum, std, integral). Autre classeur : [Book2]Sheet5!B. Un nom avec une espace se met entre apostrophes : 'Blanc 2'!B
formula-help-family-powers = Puissances et logarithmes
formula-help-family-trigonometry = Trigonométrie
formula-help-family-error = Fonction d'erreur
formula-help-family-rounding = Signe et arrondis
formula-help-family-choice = Choix et comparaison
formula-help-family-column = Valeurs d'une colonne
formula-help-family-constants = Constantes
formula-help-exp = e puissance x
formula-help-ln = Logarithme népérien
formula-help-log = Logarithme décimal
formula-help-log2 = Logarithme en base 2
formula-help-logx = Logarithme de x en base b
formula-help-sqrt = Racine carrée
formula-help-root = Racine n-ième de x
formula-help-pow = x puissance y
formula-help-abs = Valeur absolue
formula-help-sign = Signe : -1, 0 ou 1
formula-help-floor = Plus grand entier non supérieur à x
formula-help-ceil = Plus petit entier non inférieur à x
formula-help-round = x arrondi à n décimales
formula-help-mod = Reste de la division de x par y
formula-help-sin = Sinus, x en radians
formula-help-cos = Cosinus, x en radians
formula-help-tan = Tangente, x en radians
formula-help-asin = Arc sinus, en radians
formula-help-acos = Arc cosinus, en radians
formula-help-atan = Arc tangente, en radians
formula-help-atan2 = Angle du point (x, y), en radians
formula-help-sinh = Sinus hyperbolique
formula-help-cosh = Cosinus hyperbolique
formula-help-tanh = Tangente hyperbolique
formula-help-erf = Fonction d'erreur
formula-help-erfc = Fonction d'erreur complémentaire
formula-help-min = Le plus petit de x et y
formula-help-max = Le plus grand de x et y
formula-help-if = a si la condition c est vraie, sinon b
formula-help-pi = Le nombre π
formula-help-at = Valeur de la colonne B pour X = x, par interpolation linéaire entre les deux points voisins. Vide hors des X. x peut être une colonne ; at(B, x, D) lit les X dans la colonne D.
formula-help-max-column = Plus grande valeur de la colonne B. Ajouter deux valeurs de X pour ne garder que les points entre elles : max(B, x1, x2).
formula-help-min-column = Plus petite valeur de la colonne B. Ajouter deux valeurs de X pour ne garder que les points entre elles : min(B, x1, x2).
formula-help-mean = Moyenne de la colonne B. Ajouter deux valeurs de X pour ne garder que les points entre elles : mean(B, x1, x2).
formula-help-sum = Somme de la colonne B. Ajouter deux valeurs de X pour ne garder que les points entre elles : sum(B, x1, x2).
formula-help-std = Écart-type d'échantillon (divisé par n − 1) de la colonne B, qui demande deux valeurs. Ajouter deux valeurs de X pour ne garder que les points entre elles : std(B, x1, x2).
formula-help-integral = Aire sous la colonne B par la méthode des trapèzes, même résultat que l'outil Intégration sans ligne de base. Ajouter deux valeurs de X pour intégrer entre elles : integral(B, x1, x2).
common-metadata = Métadonnées
workbook-metadata-hide = Masquer la ligne Métadonnées
workbook-metadata-summary =
    { $count ->
        [one] { $count } clé
       *[other] { $count } clés
    }
    .hint = Affiche les métadonnées du fichier
workbook-metadata-use-as-x-prime = Utiliser comme x'
    .hint = Écrit cette valeur dans la ligne x' de la colonne
workbook-metadata-title = Métadonnées du fichier
workbook-metadata-title-named = Métadonnées — { $name }

# Classeur : menu de colonne (type, rôle, tracé)
column-role = Rôle
workbook-role-x = X
workbook-role-y = Y
workbook-role-x2 = X2
workbook-role-y2 = Y2
workbook-role-x3 = X3
workbook-role-y3 = Y3
workbook-role-x4 = X4
workbook-role-y4 = Y4
workbook-role-z = Z
column-role-x-error = Erreur X
column-role-y-error = Erreur Y
workbook-role-label = Étiquette
workbook-role-none = Aucun
workbook-plot-menu = Tracer
workbook-send-to-graph = Envoyer vers { $graph }
    .hint = Ajoute les colonnes sélectionnées à un graphique ouvert
workbook-send-to-graph-menu = Envoyer vers un graphique
    .hint = Ajoute les colonnes sélectionnées à un graphique ouvert

# Classeur : grille, onglets de feuilles, états vides
workbook-column-add = Ajouter une colonne
workbook-grid-no-columns = Aucune colonne dans cette feuille. Clic droit ici, puis Ajouter une colonne
workbook-no-worksheets = Ce classeur n'a aucune feuille. Utiliser Ajouter une feuille dans les onglets
workbook-worksheet-add = Ajouter une feuille
action-rename-sheet = Renommer
    .hint = Modifier le nom de la feuille
action-duplicate-sheet = Dupliquer
    .hint = Ajouter une copie de cette feuille, colonnes comprises
action-reset-sheet-column-widths = Réinitialiser les largeurs
    .hint = Remet toutes les colonnes de cette feuille à la largeur par défaut
    .disabled = Les colonnes sont déjà à la largeur par défaut
action-delete-sheet = Supprimer
    .hint = Retire cette feuille et ses colonnes ; Annuler les rétablit
    .disabled = Un classeur garde au moins une feuille
sheet-copy-name = { $name } copie { $number }
sheet-invalid = { $id } n'est pas une feuille de ce projet
sheet-rename-empty = Le nom d'une feuille ne peut pas être vide ; « { $name } » conservé
sheet-rename-taken = Une feuille nommée « { $name } » existe déjà dans ce classeur ; « { $old } » conservé
sheet-delete-last = Un classeur garde au moins une feuille
sheet-delete-used = Feuille supprimée ; elle était tracée par { $graphs }. Annuler la rétablit
matrix-unavailable = L'édition des matrices n'est pas encore disponible
note-deleted = Cette note a été supprimée

# Console de script
script-title = Script Python
script-unavailable = — Indisponible dans cette version
script-run = Exécuter
script-stop = Arrêter
script-running = Exécution…
script-not-run = Pas encore exécuté. Écrire un script, puis cliquer sur Exécuter
script-stdout = Sortie standard
script-stderr = Sortie d'erreur
script-traceback = Trace d'appels
script-no-output = (aucune sortie)

# Sélecteur de couleur
color-none = Aucune couleur (transparent)
color-none-swatch = Aucune couleur (transparent, sans fond dans les exports)
color-custom = Couleur personnalisée
    .hint = Choisit n'importe quelle couleur et son opacité, et l'enregistre
color-shades-dark = Nuances plus foncées
color-shades-light = Nuances plus claires
color-section-text = Couleurs de texte
color-section-light-backgrounds = Aucune couleur (par défaut) et fonds clairs
color-section-dark-backgrounds = Fonds sombres (le texte auto est blanc)
color-saved-project = Enregistrées dans le projet
color-saved-user = Mes couleurs (tous les projets)
color-saved-remove-hint = Clic droit pour retirer
color-save = Enregistrer la couleur
    .hint = Ajoute la couleur actuelle aux couleurs enregistrées
color-target-project = Dans le projet
color-target-user = Dans mes couleurs
color-shape-square = Carré
color-shape-wheel = Roue
color-opacity = Opacité (%)
color-rgb = RVB
color-hsl = TSL
color-hex = Hexa
    .hint = #RRGGBB, ou #RRGGBBAA avec l'opacité
color-channel-red = R
color-channel-green = V
color-channel-blue = B
color-channel-hue = T (°)
color-channel-saturation = S (%)
color-channel-lightness = L (%)
color-palette-stokesforge = StokesForge (par défaut)
color-palette-tableau = Tableau 10
color-palette-okabe-ito = Okabe-Ito (adaptée au daltonisme)
color-palette-brewer-set1 = ColorBrewer Set1
color-palette-brewer-dark2 = ColorBrewer Dark2
color-palette-brewer-paired = ColorBrewer Paired
color-palette-viridis = Viridis (dégradé)
color-palette-plasma = Plasma (dégradé)
color-palette-grayscale = Niveaux de gris (dégradé)

## Toolbars (#60)

toolbar-file = Fichier
    .hint = Affiche ou masque la barre Fichier
status-toolbars-not-saved = Impossible d'enregistrer la disposition des barres d'outils : { $detail }

toolbar-tools = Outils
    .hint = Affiche ou masque la barre d'outils de la fenêtre active
    .disabled = Mettez d'abord une fenêtre de graphique au premier plan
tool-pointer = Pointeur
    .hint = Glisser pour déplacer et redimensionner textes et objets ; Ctrl + glisser ou clic milieu + glisser pour déplacer la vue ; Ctrl + molette pour zoomer
tool-zoom = Zoom
    .hint = Tracer un rectangle pour zoomer dessus ; Ctrl + glisser ou clic milieu + glisser pour déplacer la vue
tool-text = Texte
    .hint = Cliquer dans la zone de tracé pour ajouter un texte
tool-rectangle = Rectangle
    .hint = Glisser dans la zone de tracé pour tracer un rectangle
tool-ellipse = Ellipse
    .hint = Glisser dans la zone de tracé pour tracer une ellipse
tool-line = Ligne
    .hint = Glisser dans la zone de tracé pour tracer une ligne
tool-arrow = Flèche
    .hint = Glisser dans la zone de tracé pour tracer une flèche ; la pointe est là où l'on relâche
toolbar-plot-selection = Tracer la sélection
    .hint = Trace les colonnes sélectionnées dans un nouveau graphique
    .disabled = Sélectionnez d'abord des colonnes dans la feuille
toolbar-text = Format du texte
    .hint = Affiche ou masque la barre de format du texte du graphique actif
toolbar-edit = Édition
    .hint = Affiche ou masque la barre d'édition
format-object-disabled = Sélectionnez un objet du graphique pour le mettre en forme
format-object-line-color = Couleur du trait
    .hint = Règle la couleur du trait de l'objet sélectionné
format-object-fill-color = Couleur de remplissage
    .hint = Règle le remplissage du rectangle ou de l'ellipse sélectionné{NB}; l'opacité se règle dans le sélecteur de couleur
format-object-no-fill = Aucun remplissage
format-object-line-width = Épaisseur du trait (pt)
    .hint = Règle l'épaisseur, en points, du trait de l'objet sélectionné
format-disabled = Sélectionnez un texte du graphique, ou modifiez-en un, pour le mettre en forme. Les titres et la légende n'acceptent que la police et la taille
format-bold = Gras
    .hint = Entoure le texte sélectionné de balises de gras ; un second appui les retire
format-italic = Italique
    .hint = Entoure le texte sélectionné de balises d'italique ; un second appui les retire
format-subscript = Indice
    .hint = Abaisse le texte sélectionné
format-superscript = Exposant
    .hint = Élève le texte sélectionné
format-math = Maths
    .hint = Écrit le texte sélectionné en mode mathématique
format-font = Police
    .hint = Règle la police de toute la zone de texte, maths comprises. Une police du système change la figure sur un ordinateur qui ne l'a pas
format-font-default = Police du graphe
format-font-embedded = Fournies avec StokesForge
format-font-system = Installées sur cet ordinateur
format-size = Taille
    .hint = Règle la taille, en points, du texte sélectionné
format-color = Couleur du texte

action-object-manager = Gestionnaire d’objets
    .hint = Afficher ou masquer les objets du graphique actif.
object-manager-no-graph = Aucun graphique actif. Cliquer un graphique pour voir ses objets
object-manager-visibility = Afficher ou masquer ce contenu. Les séries masquées sont exclues de la légende et des bornes automatiques des axes.
object-manager-layer = Calque { $index }
object-manager-group = Séries
object-manager-axes = Axes
object-manager-show-axes = Afficher les axes
    .hint = Liste les axes de chaque calque dans le gestionnaire d’objets. Ne modifie pas leur visibilité sur le graphique. Non listés par défaut.

object-manager-show-objects = Afficher les objets
    .hint = Liste les objets libres de chaque calque (textes libres, formes) dans le gestionnaire d’objets. Ne modifie pas leur visibilité sur le graphique. Listés par défaut.

menu-selected-content = Sélection
    .disabled = Sélectionnez d’abord une courbe, un texte ou un objet
action-rename-series = Renommer
action-rename-group = Renommer
action-ungroup-series =
    Dissoudre le groupe
    .hint = Chaque série de ce groupe devient une série simple ; elle garde son style
action-select-group-series =
    Sélectionner les membres
    .hint = Sélectionne le groupe et affiche les poignées de toutes ses séries
action-move-series-up = Déplacer vers le haut
action-move-series-down = Déplacer vers le bas
action-show-content = Afficher
action-hide-content = Masquer
action-delete-content = Supprimer
object-properties-title = Propriétés de l'objet
object-coordinate = { $coordinate }
history-plot-visibility = Modifier la visibilité
history-plot-rename-series = Renommer la série
history-plot-reorder-series = Réordonner les séries
history-plot-remove-series = Supprimer la série
history-plot-remove-text = Supprimer le texte
history-plot-remove-object = Supprimer l'objet
history-plot-reorder-object = Réordonner les objets
history-plot-rename-group = Renommer le groupe
history-plot-dissolve-group = Dissoudre le groupe
history-plot-remove-group = Supprimer le groupe
history-plot-reorder-group = Réordonner le groupe
history-plot-closed-frame = Modifier le cadre fermé
history-plot-add-cell = Ajouter un calque à côté
history-plot-remove-cell = Supprimer le calque
history-plot-resize-cell = Redimensionner la zone de tracé
history-plot-place-figure = Positionner la zone de tracé
history-plot-bring-layer-to-front = Mettre le calque au premier plan
history-plot-send-layer-to-back = Mettre le calque à l'arrière-plan
history-plot-layer-background = Modifier le fond du calque
history-plot-move-layer = Déplacer le calque
history-plot-resize-layer = Redimensionner le calque
history-plot-align-layers = Aligner les calques
history-plot-match-layer-size = Uniformiser la taille des calques
history-plot-distribute-layers = Répartir les calques
history-plot-layer-placement = Changer le placement des calques

# Panneaux fixes et fenêtres détachées (#100)
panel-expand = Déployer le panneau
panel-collapse = Replier le panneau
panel-detach = Détacher dans une fenêtre séparée
panel-reattach = Rattacher à la fenêtre principale
window-header-show-workbook = Afficher le contenu du classeur
window-header-hide-workbook = Masquer le contenu du classeur
window-header-show-graph = Afficher le contenu du graphe
window-header-hide-graph = Masquer le contenu du graphe
window-header-close = Fermer la fenêtre
window-header-detach = Détacher dans une fenêtre séparée
window-header-reattach = Réattacher à la fenêtre principale
window-header-maximize = Maximiser la fenêtre

history-macro-paste-objects = Coller des objets
history-macro-duplicate-objects = Dupliquer des objets
history-macro-delete-objects = Supprimer des objets
history-macro-move-objects = Déplacer des objets

object-rename-empty = Le nom est vide, le nom précédent a été conservé
object-clipboard-invalid = Le presse-papiers ne contient aucun objet StokesForge valide
object-delete-used = Supprimé ; données utilisées par { $graphs }. Annuler les rétablit

action-rename-project-object = Renommer
action-duplicate-project-object = Dupliquer
action-delete-project-object = Supprimer

history-macro-rename-objects = Renommer des objets

object-clipboard-use-manager = Sélectionner un dossier ou un objet dans le gestionnaire de projet pour coller ces objets

action-show-graph-data = Afficher les données

## Copie du graphe vers une autre application (#149)

error-clipboard-empty = Il n'y a rien à copier
error-clipboard-unsupported = Cet affichage ne peut pas recevoir un graphique copié ({ $display })
error-clipboard-backend = Impossible de copier le graphique : { $detail }

# Réglages de vue de l’Object Manager (#139).
object-manager-default-expansion = Déplier par défaut
    .hint = Définit seulement l’état initial des nouveaux groupes ; les groupes déjà dépliés ou repliés gardent leur état
object-manager-expand-layers = Calques
object-manager-expand-plots = Groupes de tracés
object-manager-objects = Objets
object-manager-reset = Réinitialiser les réglages
    .hint = Rétablit les réglages par défaut du gestionnaire d’objets sans changer le repli courant des groupes

project-manager-selection-needed = Sélectionnez d’abord au moins un objet

history-add-calculation = Ajouter un calcul : { $kind }
history-edit-calculation = Modifier le calcul : { $kind }
history-delete-calculation = Supprimer le calcul : { $kind }
history-detach-calculation = Détacher le calcul : { $kind }
history-delete-calculation-any = Supprimer le calcul
history-detach-calculation-any = Détacher le calcul
analysis-kind-baseline = Correction
analysis-kind-correct = Référence
analysis-kind-integrate = Intégration
analysis-kind-fit = Ajustement
analysis-kind-peaks = Décomposition en pics
analysis-kind-reconvolution = Reconvolution
analysis-kind-direct-deconvolution = Déconvolution directe
error-model-calculated-column-read-only = Cette colonne est produite par un calcul. La détacher du calcul pour modifier ses valeurs
error-model-transpose-no-x-prime = La colonne { $letter } ({ $name }) n’a pas de x'. Lui en donner un pour analyser une ligne
error-model-transpose-not-numeric = La colonne { $letter } ({ $name }) contient du texte. Seules des colonnes numériques permettent d’analyser une ligne

## Analysis — names of the columns an operation produces (#181)
analysis-output-baseline-corrected = { $name } ({ $column }-corr.)
analysis-output-corrected = { $name } (corrigé)
analysis-output-baseline = Ligne de base de { $column }
analysis-output-cumulative = Intégrale cumulée de { $column }
analysis-output-normalized = Intégrale normalisée de { $column }
analysis-output-fit = { $name } (ajusté)
analysis-output-residuals = { $name } (résid.)
analysis-output-peak = Pic { $index } de { $column }
analysis-output-background = Fond de { $column }
analysis-output-peaks-sum = Somme des pics de { $column }
analysis-output-deconvolved = { $name } (déconvolué)

## Analysis — errors of a calculation (#181)
error-analysis-not-enough-points = Seulement { $found } points utilisables, au moins { $needed } sont nécessaires. Vérifiez la plage et les données
error-analysis-duplicate-x = Deux points ont la même valeur de X. Supprimez ou moyennez d'abord les doublons
error-analysis-bad-expression = L'expression du modèle est invalide : { $detail }
error-analysis-no-free-parameter = Tous les paramètres sont fixés. Libérez au moins un paramètre
error-analysis-empty-bounds = Les bornes de { $name } sont inversées (le minimum { $min } dépasse le maximum { $max }). Corrigez les bornes
error-analysis-non-finite-model = Le modèle ne donne aucune valeur valide avec les valeurs de départ. Changez les valeurs de départ
error-analysis-model-not-evaluable = Le modèle n'est pas évaluable autour des valeurs actuelles : sa pente n'est pas un nombre. Vérifiez l'expression, les bornes et les valeurs de départ
error-analysis-singular-system = Les données ne déterminent pas les paramètres. Fixez un paramètre ou élargissez la plage
error-analysis-not-converged = L'ajustement n'a pas convergé après { $iterations } itérations. Changez les valeurs de départ ou le modèle
error-analysis-not-enough-anchors = La ligne de base demande au moins { $needed } points d'ancrage, { $found } posés
error-analysis-zones-too-small = Les zones de fond contiennent { $found } points, { $needed } sont nécessaires pour le degré { $degree }. Élargissez les zones ou baissez le degré
error-analysis-duplicate-anchor = Deux points d'ancrage ont la même valeur de X. Déplacez l'un d'eux
error-analysis-empty-reference = La colonne de référence n'a aucune valeur utilisable. Choisissez une autre colonne
error-analysis-empty-response = La réponse instrumentale n'a aucun signal utilisable. Choisissez une autre colonne
error-analysis-empty-range = La plage ne contient aucun point. Élargissez-la
error-analysis-invalid-parameter = La valeur de { $name } est invalide
error-analysis-length-mismatch = Une série et sa colonne X n'ont pas le même nombre de lignes
error-analysis-output-unwritable = Une colonne de résultat n'accepte plus le résultat. Supprimez cette colonne ou détachez le calcul
error-analysis-no-peak = Aucun pic à ajuster. Posez un pic sur l'aperçu ou baissez la proéminence minimale
error-analysis-bad-sigma = La colonne d'erreurs contient une valeur manquante, nulle ou négative. Corrigez-la ou désactivez la pondération
error-analysis-bad-signal = Ce signal ne peut pas être déconvolué : sa moyenne n'est pas positive ou ses pas en X sont trop irréguliers. Vérifiez les données
error-analysis-bad-components = Le nombre de composantes doit être compris entre 1 et 4
error-analysis-bad-degree = Le degré doit être compris entre 1 et 9
error-analysis-bad-iterations = Le nombre d'itérations doit être compris entre 1 et 5000
error-analysis-bad-noise = Le rapport de bruit doit être un nombre positif
error-analysis-bad-max-peaks = Le nombre maximal de pics est 100
error-analysis-expr-empty = L'expression est vide
error-analysis-expr-unexpected-char = Caractère inattendu « { $symbol } » en position { $position }
error-analysis-expr-unexpected-end = L'expression se termine trop tôt (position { $position })
error-analysis-expr-unexpected-token = Symbole inattendu en position { $position }
error-analysis-expr-unbalanced = Parenthèses non appariées (position { $position })
error-analysis-expr-bad-number = Nombre mal formé en position { $position }
error-analysis-expr-too-deep = Expression trop imbriquée (position { $position })
error-analysis-expr-unknown-function = Fonction inconnue { $name } en position { $position }
error-analysis-expr-wrong-arguments = { $name } prend { $expected } argument(s), { $found } donné(s) (position { $position })

## Analysis — status messages and what is missing (#181)
status-analysis-no-series = Sélectionnez au moins une colonne Y à analyser
status-analysis-gone = Ce calcul n'existe plus
status-analysis-recalculated = Calculs recalculés
analysis-title = { $kind } de { $column }
analysis-issue-no-series = Sélectionnez au moins une colonne Y
analysis-issue-needs-reference = Choisissez une colonne de référence
analysis-issue-needs-response = Choisissez la colonne de la réponse instrumentale
analysis-issue-needs-points =
    { $needed ->
        [one] Placez au moins { $needed } point ({ $have } placé)
       *[other] Placez au moins { $needed } points ({ $have } placés)
    }
analysis-issue-needs-zone = Tracez au moins une zone de fond
analysis-issue-no-parameter = Le modèle n'a aucun paramètre
analysis-issue-bad-model = Le modèle est invalide : { $detail }
analysis-issue-missing-column = Une colonne de ce calcul n'existe plus

## Analysis — menu (#181)
menu-analysis = Analyse
analysis-block-type = Type d'analyse
action-analysis-open-data-manipulation = Manipulation de données…
    .hint = Ouvre la fenêtre Manipulation de données : corrections de ligne de base, mathématiques ou par une référence, qui peuvent remplacer les données originales
action-analysis-open-fitting = Ajustement…
    .hint = Ouvre la fenêtre Ajustement : ajuste un modèle aux données
action-analysis-open-peaks = Pics et déconvolution…
    .hint = Ouvre la fenêtre Pics et déconvolution : décomposition en pics, reconvolution et déconvolution directe
action-analysis-open-integrate = Intégrer…
    .hint = Ouvre la fenêtre Intégrer : aires sur des plages et intégrale cumulée
analysis-category-data-manipulation = Manipulation de données
analysis-category-fitting = Ajustement
analysis-category-peaks = Pics et déconvolution
analysis-category-integrate = Intégrer
analysis-needs-y = Sélectionnez d'abord au moins une colonne Y
analysis-baseline-linear = Droite…
    .hint = Soustrait ou divise par une droite passant par deux points
analysis-baseline-polynomial = Polynôme…
    .hint = Ajuste un polynôme sur des zones de fond, puis le soustrait ou divise
analysis-baseline-anchors = Points d'ancrage…
    .hint = Soustrait ou divise par une courbe passant par les points d'ancrage posés
action-analysis-correct = Correction par référence…
    .hint = Divise, multiplie ou soustrait une colonne de référence, interpolée sur les données
action-analysis-integrate = Intégrer…
    .hint = Calcule des aires sur une ou plusieurs plages, et l'intégrale cumulée
analysis-fit-linear = Linéaire…
    .hint = Ajuste une droite
analysis-fit-polynomial = Polynôme…
    .hint = Ajuste un polynôme
analysis-fit-exponential = Exponentielle…
    .hint = Ajuste une exponentielle décroissante
analysis-fit-exponential-offset = Exponentielle avec palier…
    .hint = Ajuste une exponentielle décroissante qui se stabilise sur un palier constant
analysis-fit-exponential2 = Deux exponentielles…
    .hint = Ajuste la somme de deux exponentielles décroissantes et d'un palier constant
analysis-fit-logarithm = Logarithme…
    .hint = Ajuste un logarithme
analysis-fit-power = Puissance…
    .hint = Ajuste une loi de puissance
analysis-fit-gaussian = Gaussienne…
    .hint = Ajuste un pic gaussien
analysis-fit-lorentzian = Lorentzienne…
    .hint = Ajuste un pic lorentzien
analysis-fit-custom = Modèle libre…
    .hint = Ajuste une expression de modèle que vous écrivez, avec ses propres paramètres
action-analysis-peaks = Décomposer en pics…
    .hint = Ajuste plusieurs pics à la fois, trouvés automatiquement ou posés à la main
action-analysis-reconvolution = Reconvolution…
    .hint = Ajuste des exponentielles convoluées avec la réponse instrumentale
action-analysis-direct = Déconvolution directe…
    .hint = Retrouve le signal avant la réponse instrumentale, sans modèle
action-analysis-recalculate-stale = Recalculer les calculs périmés
    .hint = Recalcule chaque calcul dont les données ont changé
    .disabled = Tout est à jour

## Analyse — analyses récentes (#249)
analysis-recent-entry = { $rank } { $label }…
analysis-recent-hint = Ouvre la fenêtre avec les mêmes réglages. Ctrl+clic la lance sur la sélection courante
analysis-recent-hint-replace = Ouvre la fenêtre avec les mêmes réglages. Ctrl+clic remplace les données originales de la sélection courante ; Ctrl+Z l'annule
analysis-recent-baseline-constant = Ligne de base constante
analysis-recent-baseline-linear = Ligne de base droite
analysis-recent-baseline-polynomial = Ligne de base polynomiale
analysis-recent-baseline-anchors = Ligne de base par points d'ancrage
analysis-recent-correct = Correction par référence
analysis-recent-integrate = Intégrer
analysis-recent-fit = Ajustement { $model }
analysis-recent-peaks = Décomposer en pics
analysis-recent-reconvolution = Reconvolution
analysis-recent-direct = Déconvolution directe
analysis-recent-subtract = Soustraire
analysis-recent-divide = Diviser
analysis-recent-multiply = Multiplier
analysis-recent-degree = degré { $degree }, { $op }
analysis-recent-degree-only = degré { $degree }
analysis-recent-detail = { $name } ({ $detail })
status-analysis-replaced = Données originales remplacées par { $label } · Ctrl+Z pour annuler
action-calculation-edit = Modifier le calcul…
    .hint = Rouvre le calcul qui a produit cette colonne
action-calculation-recalculate = Recalculer
    .hint = Rafraîchit les colonnes de sortie depuis les données actuelles
    .disabled = Ce calcul est à jour
action-calculation-detach = Détacher du calcul
    .hint = Garde les colonnes comme colonnes ordinaires et retire le calcul
action-calculation-delete = Supprimer le calcul…
    .hint = Supprime le calcul, avec ou sans ses colonnes

## Analysis — names of calculated values (#181)
analysis-value-area = Aire { $index }
analysis-value-area-from = Aire { $index } : de
analysis-value-area-to = Aire { $index } : à
analysis-value-area-total = Aire totale
analysis-value-out-of-reference = Points hors de la référence
analysis-value-points = Points utilisés
analysis-value-r-squared = R²
analysis-value-reduced-chi2 = χ² réduit
analysis-value-dof = Degrés de liberté
analysis-value-tau-amplitude = τ (pondéré par l'amplitude)
analysis-value-tau-intensity = τ (pondéré par l'intensité)
analysis-value-noise-ratio = Rapport de bruit
analysis-value-baseline-points = Points de la ligne de base
analysis-value-baseline-slope = Pente de la ligne de base
analysis-value-baseline-intercept = Ordonnée à l'origine de la ligne de base
analysis-value-peak-height = Pic { $index } : hauteur
analysis-value-peak-center = Pic { $index } : centre
analysis-value-peak-fwhm = Pic { $index } : largeur à mi-hauteur
analysis-value-peak-fwhm-gauss = Pic { $index } : largeur gaussienne
analysis-value-peak-fwhm-lorentz = Pic { $index } : largeur lorentzienne
analysis-value-peak-area = Pic { $index } : aire
analysis-value-peak-area-percent = Pic { $index } : aire (%)
analysis-value-background-y0 = Fond : décalage
analysis-value-background-slope = Fond : pente

## Analysis — preview (#181)
analysis-preview-residuals = Résidus
analysis-preview-average = Moyenne sur { $count } points

## Analysis — window (#181)
analysis-window-new = Nouvelle analyse : { $kind }
analysis-window-edit = Modifier le calcul : { $kind }
analysis-apply-create = Créer
    .hint = Lance le calcul et écrit ses colonnes
analysis-apply-update = Mettre à jour
    .hint = Applique les nouveaux réglages et réécrit les colonnes
analysis-block-input = Entrée
analysis-block-output = Sortie
analysis-series-previous = Série précédente
analysis-series-next = Série suivante
analysis-series-position = Série { $current } sur { $total }
analysis-field-reference = Colonne de référence
analysis-field-response = Réponse instrumentale
analysis-field-choose-column = Choisir une colonne
analysis-field-no-candidate = Aucune autre colonne Y dans ce classeur
analysis-field-auto = auto
analysis-field-to = à
analysis-field-range = Plage
analysis-field-remove = Retirer
analysis-field-clear-range = Effacer la plage (utiliser toutes les données)
analysis-field-add-range = Ajouter une plage
    .hint = Ajoute une plage à saisir ; vous pouvez aussi glisser sur l'aperçu avec l'outil Zone
analysis-baseline-shape-line = Droite passant par deux points
analysis-baseline-shape-zones = Polynôme sur des zones de fond
analysis-baseline-shape-anchors = Courbe passant par des points d'ancrage
analysis-baseline-hint-line = Cliquez deux points sur l'aperçu
analysis-baseline-hint-anchors = Cliquez des points d'ancrage sur l'aperçu ; le clic droit en retire un. Maintenez Maj pour aimanter sur la courbe
analysis-baseline-hint-zones = Glissez sur l'aperçu pour marquer les zones où il n'y a que du fond
analysis-baseline-segments = Segments
analysis-baseline-spline = Spline
analysis-baseline-degree = Degré
analysis-baseline-subtract = Soustraire
analysis-baseline-divide = Diviser
analysis-correct-hint = La référence est interpolée sur les X des données ; les points hors de la référence restent vides
analysis-correct-divide = Diviser
analysis-correct-multiply = Multiplier
analysis-correct-subtract = Soustraire
analysis-integrate-hint = Sans plage, toute la courbe est intégrée. Glissez sur l'aperçu pour ajouter des plages
analysis-integrate-zero = Aire jusqu'à zéro
analysis-integrate-line = Aire au-dessus de la droite entre les bornes
analysis-integrate-cumulative = Écrire l'intégrale cumulée
analysis-integrate-normalize = Écrire l'intégrale normalisée
analysis-model-linear = Linéaire
analysis-model-polynomial = Polynôme
analysis-model-exponential = Exponentielle
analysis-model-exponential-offset = Exponentielle avec palier
analysis-model-exponential2 = Deux exponentielles
analysis-model-logarithm = Logarithme
analysis-model-power = Puissance
analysis-model-gaussian = Gaussienne
analysis-model-lorentzian = Lorentzienne
analysis-model-custom = Modèle libre
analysis-fit-auto = Auto
    .hint = Remplit les valeurs de départ par une estimation d'après les données montrées
analysis-fit-model = Modèle
analysis-fit-degree = Degré
analysis-fit-custom-hint = Écrivez y en fonction de x ; tout autre nom devient un paramètre
analysis-fit-weighted = Pondérer par la colonne d'erreur
    .hint = Utilise la colonne d'erreur de chaque série comme écart-type de ses points
analysis-fit-residuals = Écrire les résidus
analysis-param-initial = Départ
analysis-param-fixed = Fixé
analysis-param-min = Min
analysis-param-max = Max
analysis-param-shared = Partagé
    .hint = Un paramètre partagé a une seule valeur pour toutes les séries (ajustement global)
analysis-peaks-shape = Forme des pics
analysis-peaks-voigt = Voigt
analysis-peaks-background = Fond
analysis-peaks-bg-none = Aucun
analysis-peaks-bg-constant = Constant
analysis-peaks-bg-linear = Linéaire
analysis-peaks-link-widths = Même largeur pour tous les pics
analysis-peaks-center-window = Garder les positions à ±
    .hint = Le centre de chaque pic reste à cette distance de la position posée ou détectée
analysis-peaks-hint = Cliquez sur l'aperçu pour placer les pics vous-même ; sans aucun, ils sont détectés
analysis-peaks-seed = Pic { $index } en
analysis-peaks-prominence = Proéminence minimale (%)
    .hint = Part de l'étendue des données dont un pic doit ressortir pour être détecté
analysis-peaks-max = Nombre maximal de pics
analysis-reconv-hint = Ajuste une somme d'exponentielles convoluées avec la réponse instrumentale
analysis-reconv-components = Exponentielles
analysis-reconv-shift = Décalage de la réponse
analysis-reconv-fit-shift = Ajuster le décalage
analysis-direct-hint = Retrouve le signal avant la réponse instrumentale, sans modèle
analysis-direct-richardson-lucy = Richardson-Lucy
analysis-direct-wiener = Wiener
analysis-direct-iterations = Itérations
analysis-direct-noise = Rapport de bruit
analysis-output-baseline-column = Écrire la ligne de base
analysis-output-peak-columns = Écrire une colonne par pic
analysis-output-sum-column = Écrire la somme des pics
analysis-output-to-graph = Ajouter au graphique
    .hint = Trace les nouvelles colonnes sur le graphique qui montre cette courbe, dans la même étape d'annulation
analysis-output-show = Montrer
    .hint = Écrit les colonnes seulement ; les valeurs ne sont pas gardées
analysis-output-keep = Garder dans les résultats
    .hint = Garde aussi les valeurs dans le volet Résultats du classeur
analysis-output-auto = Recalculer automatiquement
    .hint = Recalcule dès que les données sources changent ; sinon les colonnes sont marquées comme périmées
analysis-output-low-r2 = Avertir quand R² est faible
    .hint = Ouvre une boîte de dialogue après un calcul quand le R² d'un ajustement est très faible
analysis-r2-low-hint = R² faible : le modèle décrit peut-être mal les données
analysis-output-heavy = Un gros lot : recalculer à chaque modification peut être lent
analysis-tool-points = Point
    .hint = Cliquez pour poser un point ; Maj aimante sur la courbe, Maj + molette règle la moyenne
analysis-tool-zone = Zone
    .hint = Glissez pour marquer une zone ou une plage
analysis-preview-navigation = Ctrl + molette zoome, Ctrl + glisser déplace la vue, double-clic réinitialise
analysis-warning-step-not-uniform = Le pas de X n'est pas constant : le résultat peut être imprécis
analysis-warning-irf-step = La réponse a un autre pas de X que les données
analysis-warning-negative-clipped = { $count } points négatifs ont été ramenés à zéro
analysis-warning-not-converged = L'ajustement n'a pas convergé : le modèle n'atteint pas les données, vérifier les valeurs de départ
analysis-warning-undetermined = { $count } paramètre(s) ne sont pas déterminés par les données : aucune incertitude n'est donnée
analysis-warning-irf-truncated = La réponse est coupée au-dessus de 1 % de son pic : la queue manquante est prise pour nulle, les durées de vie peuvent être biaisées
analysis-warning-bridged-reference-gap = { $count } point(s) ont été interpolés à travers une cellule vide de la colonne de référence
analysis-warning-division-by-zero = { $count } division(s) par zéro ont été laissées vides
analysis-warning-shifted-x = Le polynôme contraint a été ajusté sur des X loin de 0 : la précision peut être faible. Soustrayez d'abord la moyenne de X
stale-series-hint = Pas à jour : son calcul doit être refait
stale-export-title = Données pas à jour
stale-export-message = Ce graphique montre { $count } calcul(s) pas à jour.
stale-export-anyway = Exporter quand même
    .hint = Exporte le graphique tel qu'il s'affiche, courbes grisées et marqueur d'avertissement
stale-export-without-warning = Exporter sans avertissement
    .hint = Exporte le graphique avec ses couleurs normales, sans marqueur
stale-export-recalculate = Recalculer puis exporter
    .hint = Remet d'abord les calculs à jour, puis exporte
analysis-calculating = Calcul en cours…
analysis-skipped-rows = { $count } lignes sans X, Y ou erreur valide ont été ignorées
analysis-delete-title = Supprimer le calcul
analysis-delete-confirm = Supprimer le calcul « { $name } » ?
analysis-delete-columns = Ses colonnes peuvent rester comme colonnes ordinaires ({ $count } actuellement).
analysis-delete-accept = Supprimer
    .hint = Supprime le calcul ; ses colonnes restent comme colonnes ordinaires
analysis-delete-with-columns = Supprimer avec les colonnes
    .hint = Supprime le calcul et les colonnes qu'il a écrites
analysis-field-x = X
analysis-field-y = Y
analysis-fit-custom-example = a*exp(-x/b)+c

## Results pane (#181)
results-title = Résultats ({ $count })
results-expand = Afficher les résultats
results-collapse = Masquer les résultats
results-scope-sheet = Cette feuille
results-scope-book = Classeur
results-empty = Aucun résultat gardé ici. Lancer un calcul depuis le menu Analyse
results-no-values = Ce calcul n'a produit aucune valeur
results-stale-suffix = { $title } (pas à jour)
results-replace-hint = La correction écrase les données originales.
results-row = { $kind } · { $place }

## Analysis — out-of-date columns (#181)
analysis-stale-manual = Pas à jour : les données sources ont changé. Recalculez pour mettre cette colonne à jour
analysis-stale-input-missing = Pas à jour : une colonne source a été supprimée. Modifiez le calcul ou détachez cette colonne
analysis-stale-failed = Pas à jour : le dernier recalcul a échoué ({ $detail })

## Erreurs du système de fichiers (#216)

error-io-not-found = Fichier ou dossier introuvable
error-io-permission-denied = Accès refusé
error-io-already-exists = Il existe déjà
error-io-storage-full = Le disque est plein
error-io-read-only = L'emplacement est en lecture seule
error-io-invalid-data = Le contenu est invalide
error-io-invalid-input = Emplacement ou nom invalide
error-io-interrupted = L'opération a été interrompue
error-io-unexpected-eof = Le fichier se termine de façon inattendue
error-io-is-directory = C'est un dossier, pas un fichier
error-io-not-directory = C'est un fichier, pas un dossier
error-io-timed-out = L'opération a expiré
error-io-other = { $detail }

## Messages de la barre d'état (#216)

error-window-geometry = Impossible de mettre à jour la position de la fenêtre : { $cause }
error-column-width = Impossible d'enregistrer la largeur de la colonne : { $cause }
error-plot-settings-cancel = Impossible d'annuler les propriétés du graphe : { $cause }
error-plot-settings-undo-group = Les modifications des propriétés du graphe ne pourront pas être annulées en une fois : { $cause }
error-plot-settings-block-detached = Les propriétés du graphe sont ouvertes. Les valider ou les annuler avant de modifier une fenêtre détachée
error-preferences-not-saved = Impossible d'enregistrer les préférences : { $cause }
error-folder-not-utf8 = Ce dossier ne peut pas servir de dossier par défaut : son nom n'est pas du texte valide. Choisir un autre dossier
error-folder-open = Impossible d'ouvrir le dossier : { $cause }
error-colors-session-only = Les couleurs enregistrées ne seront gardées que pour cette session : { $cause }
error-script-vanished = Le moteur de script s'est arrêté. Relancer le script
error-save-script-running = Attendre la fin du script avant d'enregistrer
error-drop-plot-settings-open = Valider ou annuler les propriétés du graphe avant de déposer des fichiers
error-import-running = Un import est déjà en cours
status-import-progress = Import en cours… { $done } sur { $total } { $total ->
    [one] fichier
   *[other] fichiers
    }
error-import-start = Impossible de lancer l'import : { $cause }
error-import-stopped = L'import s'est arrêté avant la fin
error-refresh-no-imported-column = Cette feuille n'a aucune colonne importée d'un fichier
error-refresh-failed = Actualisation impossible : { $failures }
error-refresh-group = Impossible d'actualiser les colonnes
error-refresh-close-group = l'actualisation n'a pas pu être regroupée en une seule étape d'annulation
status-refresh-done = { $count ->
    [one] { $count } colonne actualisée depuis son fichier
   *[other] { $count } colonnes actualisées depuis leurs fichiers
    }
error-refresh-partial = { $count ->
    [one] { $count } colonne actualisée depuis son fichier
   *[other] { $count } colonnes actualisées depuis leurs fichiers
} : { $failures }
error-import-nothing-to-reopen = Il n'y a aucun import à rouvrir
status-import-reopened-undone = Dernier import annulé. Régler ses paramètres, puis le relancer
status-import-reopened-kept = Régler les paramètres du dernier import, puis le relancer. L'import précédent reste en place
status-template-chosen = Modèle « { $name } » choisi d'après le fichier
status-template-exported = Modèle exporté dans { $path }
error-template-export = Impossible d'exporter le modèle : { $cause }
status-template-imported = Modèle « { $name } » importé
error-template-import-not-saved = Modèle importé mais non enregistré : { $cause }
error-template-name-required = Donner un nom au modèle pour l'enregistrer
error-template-save = Impossible d'enregistrer le modèle : { $cause }
status-template-saved = Modèle « { $name } » enregistré
error-template-delete = Impossible de supprimer le modèle : { $cause }
status-template-deleted = Modèle « { $name } » supprimé
error-import-choose-file-first = Choisir un fichier avant de détecter l'en-tête
error-export-unknown-format = Format d'export inconnu pour { $path }. Choisir .svg, .pdf ou .png
status-graph-exported = Graphique exporté dans { $path }
error-sheet-gone = Cette feuille n'est plus dans le projet
error-sheet-no-workbook = Cette feuille n'est dans aucun classeur
error-column-gone = Cette colonne n'est plus dans le projet
error-workbook-no-folder = Ce classeur n'est dans aucun dossier
error-graph-gone = Ce graphique n'est plus dans le projet
error-layers-full = Ce graphique a déjà trois calques superposés
error-layer-gone = Ce calque n'est plus dans le graphique
error-layer-grid-full = La grille de calques de ce graphique est pleine
error-last-layer = Un graphique garde au moins un calque
error-clear-group = Impossible d'effacer la sélection
error-paste-group = Impossible de coller
error-columns-group = Impossible de modifier les colonnes
error-create-group = Impossible de créer les objets
error-import-group = Impossible d'importer les fichiers
error-group-close = Impossible de terminer l'opération en une seule étape d'annulation : { $cause }
error-paste-no-cell = Sélectionner une cellule où coller
status-paste-absolute-empty = Rien à coller en absolu : copier d'abord depuis la grille
status-import-done = { $imported ->
    [one] { $imported } fichier importé
   *[other] { $imported } fichiers importés
} sur { $total }
error-import-partial = { $imported ->
    [one] { $imported } fichier importé
   *[other] { $imported } fichiers importés
} sur { $total } : { $failures }
error-import-close-group = l'entrée d'historique de l'import n'a pas pu être fermée
error-import-create-book = { $name } : le classeur n'a pas pu être créé
error-import-create-sheet = { $name } : la feuille n'a pas pu être créée
error-plot-no-y = Aucune colonne Y sélectionnée. Sélectionner une colonne Y à tracer
error-add-no-y = Aucune colonne Y sélectionnée. Sélectionner une colonne Y à ajouter
error-script-unavailable = Les scripts ne sont pas disponibles dans cette version de StokesForge
error-script-running = Un script est déjà en cours d'exécution
error-script-no-python = Python est introuvable. Définir STOKESFORGE_PYTHON_HOME ou réinstaller StokesForge
status-script-running = Exécution du script…
error-script-start = Impossible de démarrer le script : { $cause }
status-script-interrupted = Script interrompu
status-script-exception = Le script a échoué. Voir la console
status-script-done = Script terminé
status-script-interrupt-requested = Interruption demandée
status-script-none-running = Aucun script en cours
error-import-not-open = La fenêtre d'import est fermée
error-template-import-read = Impossible de lire le modèle { $path } : { $cause }
error-template-import-invalid = Le modèle { $path } n'est pas valide : { $detail }
status-import-dropped = { $count ->
    [one] { $count } fichier importé avec le modèle « { $models } »
   *[other] { $count } fichiers importés avec le modèle « { $models } »
    }

# Champs : valeur ramenée à une borne fixe (#219).
field-clamped-min = Ramené au minimum, { $limit }
field-clamped-max = Ramené au maximum, { $limit }
field-refused-number = Saisissez un nombre
field-refused-integer = Saisissez un nombre entier
field-refused-use-auto = Tapez Auto pour suivre les données
field-refused-name-empty = Le nom ne peut pas être vide
field-refused-name-taken = Ce nom est déjà utilisé

# Champ numérique : bulle du ▼ (#220).
number-presets = Valeurs courantes

## Raisons de grisage (#226) : un élément grisé dit pourquoi et quoi faire

disabled-object-gone = Cet élément n'existe plus
disabled-move-first = Déjà en premier
disabled-move-last = Déjà en dernier
disabled-visibility-empty = Rien à afficher ou masquer
disabled-axis-blocked = Un calque au-dessus dessine déjà cet axe
disabled-axis-on-overlay = Indisponible sur un calque superposé
disabled-axis-own-scale-off = Cochez Échelle propre pour donner à ce calque sa propre échelle
disabled-axis-side-base-x = L'axe X du calque { $layer } est tracé ici. L'axe X de ce calque est dans l'onglet { $tab }
disabled-axis-side-base-y = L'axe Y du calque { $layer } est tracé ici. L'axe Y de ce calque est dans l'onglet { $tab }
disabled-axis-side-mirror = Le calque { $layer } affiche son axe miroir ici. Décochez Afficher l'axe miroir dans son onglet { $tab } pour libérer ce côté
disabled-axis-side-overlay-x = Le calque { $layer } trace son axe X ici. Décochez Afficher l'axe dans son onglet { $tab } pour libérer ce côté
disabled-axis-side-overlay-y = Le calque { $layer } trace son axe Y ici. Décochez Afficher l'axe dans son onglet { $tab } pour libérer ce côté
disabled-text-not-moved = Ce texte n'a pas été déplacé
disabled-no-graph-data = Ce graphique n'a aucune donnée à montrer
disabled-nothing-to-undo = Rien à annuler
disabled-copy-last-format = Au moins un format doit être offert
disabled-nothing-to-redo = Rien à rétablir
disabled-new-project-busy = Terminez ou annulez d'abord le script, l'import ou la modification en cours
disabled-no-source-file = Aucune colonne de cette feuille ne vient d'un fichier
disabled-no-last-import = Aucun import terminé à rouvrir
disabled-needs-sheet = Mettez d'abord une feuille de classeur au premier plan
disabled-no-window = Aucune fenêtre à agrandir
disabled-import-not-data-line = Seule une ligne de données ou de pied de page peut terminer les données
disabled-import-auto-end-off = La fin automatique des données est déjà désactivée
disabled-import-unsaved-template = Seul un modèle que vous avez enregistré peut être supprimé
disabled-add-file-first = Ajoutez d'abord un fichier
disabled-import-no-active-book = Mettez d'abord un classeur au premier plan
disabled-import-last-step = C'est la dernière étape
disabled-import-first-step = C'est la première étape
disabled-script-running = Attendez la fin du script
disabled-script-unavailable = Python n'est pas disponible dans cette version
disabled-script-not-running = Aucun script en cours
disabled-color-invalid = Saisissez d'abord une couleur valide
disabled-gradient-min-colors = Un dégradé a besoin d'au moins deux couleurs
disabled-mirror-off = Activez d'abord l'axe miroir
disabled-ticks-off = Affichez d'abord les graduations
disabled-minor-ticks-off = Affichez d'abord les sous-graduations
disabled-analysis-issues = Corrigez d'abord le problème signalé dans le formulaire

## Aide › Raccourcis clavier (#231, S-K5). Les listes sont générées du registre
## des touches (`app/shortcuts.rs`, `HELP`).

menu-help = Aide
action-keyboard-shortcuts = Raccourcis clavier…
    .hint = Liste les touches et les gestes de l'application
keyboard-shortcuts-title = Raccourcis clavier
action-about = À propos de StokesForge
    .hint = Affiche la version, la licence et la page du projet
about-title = À propos de StokesForge
about-version = Version { $version }
about-license = Licence : { $license }
about-repository = Page du projet
action-copy-version = Copier
    .hint = Copie la version et le système, à citer dans un rapport de bogue
status-version-copied = Version copiée
action-preferences = Préférences…
    .hint = Règle l'application, pas le projet
preferences-title = Préférences
preferences-general = Général
preferences-export = Export
preferences-display = Affichage
preferences-display-interface = Interface
preferences-display-object-manager = Gestionnaire d'objets
preferences-display-new = Nouveaux
preferences-show-formula-help = Afficher l'aide de syntaxe des formules
    .hint = Affiche l'icône « ? » à côté des champs de texte qui acceptent une formule (titres, étiquettes, unités). Masquée, les champs gardent leur propre rappel.
preferences-show-column-formula-help = Afficher l'aide de F(x)=
    .hint = Affiche l'icône « ? », qui liste les fonctions, pendant la saisie de la case F(x)= d'une colonne.
preferences-om-listed = Listés dans le panneau
preferences-new-graphs = Nouveaux graphiques
preferences-new-graphs-note = Point de départ des graphiques que vous créez. Chaque graphique garde ensuite son propre réglage, dans ses Propriétés du graphique ; les graphiques existants ne changent pas.
preferences-new-graph-decimal-comma = Virgule décimale sur les graduations
    .hint = Les nouveaux graphiques écrivent 1,5 au lieu de 1.5 sur tous les axes, à l'écran et dans les exports. Les graphiques existants et ceux créés par des scripts gardent leur réglage.
preferences-analysis-values = Valeurs
preferences-analysis-values-note = Ce que propose la fenêtre d'une analyse la première fois que vous l'utilisez. Chaque analyse retient ensuite son dernier choix.
preferences-export-graph = Exporter le graphique
preferences-export-graph-note = Réglable aussi depuis Graphique › Exporter. Copier le graphique utilise la même résolution PNG.
preferences-png-scale-option = ×{ $scale }
preferences-folders = Dossiers
preferences-folders-config = Dossier de configuration
preferences-folders-config-note = Là où StokesForge garde vos réglages, vos couleurs enregistrées et vos modèles d'import. Vos projets n'y sont pas rangés.
preferences-folders-location = Emplacement
preferences-folders-open = Ouvrir le dossier
    .hint = Ouvre le dossier de configuration dans le gestionnaire de fichiers, et le crée s'il n'existe pas encore
preferences-folders-files = Fichiers
preferences-folders-file-preferences = Préférences
preferences-folders-file-colors = Couleurs enregistrées
preferences-folders-file-templates = Modèles d'import
preferences-folders-file-recent = Fichiers importés récemment
preferences-folders-file-demo = Projet de démonstration
preferences-folders-empty = Aucun fichier pour l'instant
preferences-folders-recovery = Copies de secours
preferences-folders-recovery-note = Rangées hors du dossier de configuration.
preferences-folders-unavailable = Indisponible
preferences-folders-default = Dossiers par défaut
preferences-folders-default-note = Une fenêtre de choix de fichier s'ouvre dans le dossier réglé ici la première fois que vous l'utilisez pendant une session, puis dans le dernier dossier utilisé. Laisser un champ vide pour toujours reprendre le dernier dossier utilisé.
preferences-folders-projects = Ouvrir et enregistrer
preferences-folders-import = Importer des fichiers
preferences-folders-export = Exporter des graphiques
preferences-folders-browse = Parcourir…
    .hint = Choisit le dossier dans une fenêtre de choix de fichier
preferences-folders-not-found = Dossier introuvable
preferences-folders-pick-title = Choisir le dossier par défaut
preferences-length-unit = Unité de longueur
preferences-length-unit-cm = Centimètres (cm)
preferences-length-unit-mm = Millimètres (mm)
preferences-length-unit-pt = Points (pt)
preferences-length-unit-in = Pouces (in)
preferences-length-unit-note = Sert à la taille des graphiques. Le fichier garde des points (1 in = 72 pt).
preferences-files = Fichiers
preferences-max-file-size = Taille maximale d'un fichier (Mo)
preferences-max-cells = Nombre maximal de cellules (millions)
preferences-max-cells-note = Plus grand nombre de cellules qu'un fichier StokesForge peut déclarer. Un fichier qui en déclare davantage est refusé avant le chargement de ses données, ce qui protège l'ordinateur d'un fichier reçu d'un tiers.
preferences-max-file-size-note = Plus gros fichier StokesForge ouvert. Un fichier plus gros est refusé avant d'être lu, ce qui protège l'ordinateur d'un fichier reçu d'un tiers.
preferences-analysis = Analyse
preferences-low-r2-dialog = Avertir quand R² est faible
    .hint = Ouvre une boîte de dialogue après un calcul quand R² est inférieur à 0,6. Chaque calcul peut aussi la couper dans ses réglages de sortie.
preferences-new-workbooks = Nouveaux classeurs
preferences-new-workbooks-note = Lignes de l'en-tête de colonnes affichées à l'ouverture d'un classeur. Chaque classeur règle ensuite les siennes depuis son menu Classeur ou sa case #.
preferences-show-metadata-row = Afficher la ligne Métadonnées
preferences-show-x-prime-row = Afficher la ligne x'
preferences-copy-graph = Copier le graphique
preferences-copy-graph-note = Formats offerts ensemble aux autres applications, dans cet ordre. Glissez une ligne pour la déplacer.
preferences-copy-move = Déplacer { $format }
preferences-copy-svg = SVG
preferences-copy-png = PNG
preferences-copy-pdf = PDF
preferences-copy-drag = Glisser pour changer l'ordre.
preferences-autosave = Enregistrer une copie de secours automatiquement
    .hint = Écrit une copie du projet ouvert dans un dossier de secours, jamais sur votre fichier
preferences-autosave-minutes = Copie toutes les (min)
preferences-autosave-note = Si StokesForge ne se ferme pas correctement, la copie est proposée au prochain démarrage. Elle est supprimée quand vous enregistrez ou quittez.
disabled-autosave-off = Activez d'abord les copies de secours automatiques
status-autosave-failed = Impossible d'écrire la copie de secours : { $cause }
status-recovery-opened = Copie de secours de { $name } ouverte. Enregistrez le projet pour la conserver
recovery-title = Récupérer des projets
recovery-message = StokesForge ne s'est pas fermé correctement. Ces copies de secours sont plus récentes que les fichiers enregistrés.
recovery-row = { $name } · { $date } UTC
recovery-untitled = Sans titre
recovery-open = Ouvrir
    .hint = Ouvre la copie de secours ; la liste reste ouverte pour les autres
recovery-ignore = Ignorer
    .hint = Garde la copie ; elle est reproposée au prochain démarrage de StokesForge
recovery-delete = Supprimer
    .hint = Supprime la copie de secours ; cette action ne peut pas être annulée
recovery-delete-title = Supprimer la copie de secours
recovery-delete-confirm = Supprimer la copie de secours de « { $name } » ? Cette action ne peut pas être annulée.
help-area-general = Général
help-area-workbook = Classeur
help-area-graph = Graphique
help-area-managers = Gestionnaire de projet et gestionnaire d'objets
help-area-fields = Champs et dialogues
help-keys-either = { $first } ou { $second }
help-keys-ctrl-wheel = Ctrl+Molette
help-keys-enter = Entrée
help-keys-escape = Échap
help-keys-tab = Tab
help-keys-arrows = Haut / Bas
help-keys-shift-arrows = Maj+Haut / Maj+Bas
help-undo = Annuler
help-redo = Rétablir
help-new-project = Nouveau projet
help-open = Ouvrir un projet
help-save = Enregistrer
help-save-as = Enregistrer sous
help-import = Importer des fichiers
help-export-graph = Exporter le graphique
help-run-script = Exécuter le script
help-edit-cell = Modifier la cellule ou le champ d'en-tête
help-select-all = Tout sélectionner
help-copy = Copier
help-copy-full-columns = Copier les colonnes entières
help-cut = Couper
help-paste = Coller
help-paste-absolute = Coller au même endroit
help-clear-cells = Vider les cellules sélectionnées, puis les supprimer
help-duplicate-sheet = Dupliquer la feuille
help-copy-graph = Copier le graphique comme image (rien de sélectionné)
help-delete-object = Supprimer l'objet sélectionné
help-move-series-up = Monter la série sélectionnée
help-move-series-down = Descendre la série sélectionnée
help-zoom = Zoomer
help-rename = Renommer
help-delete-selection = Supprimer la sélection
help-duplicate = Dupliquer
help-select-neighbour = Sélectionner la ligne précédente ou suivante
help-open-entry = Ouvrir la fenêtre, déplier ou replier le dossier
help-validate = Valider
help-cancel = Annuler
help-next-field = Valider et passer au champ suivant
help-step-value = Changer le nombre d'un pas
help-step-value-ten = Changer le nombre de dix pas
help-open-list = Ouvrir la liste des valeurs

## Travail en cours (#232)
busy-stop = Arrêter
    .hint = Arrête le travail en cours
status-import-stopped = Import arrêté. Rien n'a été importé
status-figure-grew = L’image a dépassé la place gardée par la taille de figure Fixe. La zone de tracé a gardé sa taille
status-recalculating = Recalcul en cours…
status-recalc-stopped = Recalcul arrêté. La modification a été annulée

analysis-baseline-constant = Constante…
    .hint = Retirer un fond plat à partir d’un point ou de la moyenne d’une zone
analysis-baseline-shape-constant = Constante (point)
analysis-baseline-shape-constant-zone = Constante (moyenne de zone)
analysis-baseline-hint-constant = Cliquez un point. Maj l’aimante sur la courbe avec la moyenne locale. Modifiez X et Y ci-dessous.
analysis-baseline-hint-constant-zone = Glissez une plage de X. Sa moyenne Y définit un fond plat ; une nouvelle plage remplace la précédente.

history-replace-analysis-data = Remplacer les données originales ({ $kind })
history-restore-original-data = Restaurer les données originales ({ $kind })
history-restore-original-data-any = Restaurer les données originales

analysis-output-new-column = Nouvelle colonne
analysis-output-replace-original = Remplacer les données originales
action-restore-original-data = Restaurer les données originales
    .hint = Remet les valeurs d'avant la correction et écrase ce qui a été saisi depuis. Ctrl+Z l'annule
    .disabled = Cette correction n'a gardé aucune valeur originale
analysis-reference-copy = copie de { $name }
analysis-reference-line = Référence : { $reference }
analysis-output-replace-hint = La correction remplace les valeurs sources. Ses réglages restent dans Résultats. Annuler restaure les données originales.
disabled-analysis-replace-operation = Seule une nouvelle correction ou référence peut remplacer les données originales.
disabled-analysis-replace-readonly = Les colonnes calculées et les colonnes à formule ne peuvent pas être remplacées.
disabled-copy-folder = Un dossier ne peut pas encore être copié. Sélectionnez seulement des fenêtres
analysis-replace-title = Remplacer les données originales
analysis-replace-confirm = Plusieurs courbes utilisent ces données. Elles afficheront toutes les valeurs corrigées.
analysis-replace-accept = Remplacer
analysis-correction-trace = Données modifiées : { $kind }

# Repère discret des corrections définitives (#212).
grid-column-modified = Données originales remplacées par une correction. Voir Résultats pour les réglages de la correction


# Glisser-déposer des séries, des groupes et des objets libres.
object-drag-end = Fin de la liste

# Colonne utilisée par un calcul (#306)
column-use-title = Colonne utilisée
column-use-message = Cette suppression touche { $count } calcul(s) :
column-use-formula = { $name } (formule)
column-use-calculation = { $kind } → { $output }
column-use-calculation-bare = { $kind }
dialog-undo =
    Annuler
    .hint = Annule la suppression

# Copier avec l'en-tête sur des colonnes à formule (#306)
full-copy-title = Copier avec l'en-tête
full-copy-message = Les colonnes copiées contiennent des formules. Copier leurs valeurs ou leurs formules ?
full-copy-values = Valeurs
    .hint = Copie les valeurs ; la ligne Formule reste vide
full-copy-formulas = Formules
    .hint = Copie aussi les formules

## Feuilles transposées (#253)
analysis-needs-same-curves = L’analyse demande des courbes de même nature : toutes tracées depuis des colonnes, ou toutes depuis des lignes
status-analysis-mixed-curves = Sélectionnez des courbes toutes tracées depuis des colonnes, ou toutes depuis des lignes
status-transpose-not-linked = Cette feuille n’est pas une feuille transposée
transpose-banner = Transposée de { $source }
transpose-state-stale = Pas à jour
transpose-state-modified = Valeurs modifiées
transpose-recalculate = Recalculer
transpose-recalculate-hint = Réécrit les colonnes liées depuis { $source }
transpose-relink = Relier de nouveau
transpose-relink-hint = Réécrit les colonnes liées depuis { $source } et efface les valeurs qui y ont été modifiées
transpose-relink-title = Relier de nouveau la feuille transposée
transpose-relink-confirm = Les valeurs modifiées dans les colonnes liées de { $sheet } seront effacées. Annuler les rétablit

# Lissage (#324)
analysis-kind-smooth = Lissage
analysis-output-smoothed = { $name } (lissé)
analysis-recent-smooth-adjacent = Moyenne adjacente
analysis-recent-smooth-savitzky-golay = Savitzky-Golay
analysis-recent-smooth-median = Médiane glissante
analysis-recent-window-points = { $points } points
analysis-recent-window-x = { $width } unités de X
low-r2-count-smooth =
    { $count ->
        [one] { $count } lissage
       *[other] { $count } lissages
    }
analysis-value-window = Largeur utilisée (points)
analysis-smooth-adjacent = Moyenne adjacente…
    .hint = Remplace chaque point par la moyenne de ses voisins
analysis-smooth-savitzky-golay = Savitzky-Golay…
    .hint = Ajuste un polynôme sur une fenêtre glissante ; garde mieux la hauteur des pics qu’une moyenne
analysis-smooth-median = Médiane glissante…
    .hint = Remplace chaque point par la médiane de ses voisins ; supprime les pointes isolées
analysis-smooth-hint = Chaque point est reconstruit depuis ses voisins dans l’ordre des X ; les extrémités de la courbe sont reflétées
analysis-smooth-choice-adjacent = Moyenne adjacente
analysis-smooth-choice-savitzky-golay = Savitzky-Golay
analysis-smooth-choice-median = Médiane glissante
analysis-smooth-unit-points = Largeur en points
analysis-smooth-unit-x = Largeur en unités de X
analysis-smooth-window-points = Largeur (points)
analysis-smooth-window-x = Largeur (unités de X)
analysis-smooth-order = Ordre du polynôme

# Accessibilité des calques et des poignées de fenêtre (#170)
plot-layer-badge-accessible = Calque { $n }
window-resize-top = Redimensionner par le bord supérieur
window-resize-bottom = Redimensionner par le bord inférieur
window-resize-left = Redimensionner par le bord gauche
window-resize-right = Redimensionner par le bord droit
window-resize-top-left = Redimensionner par le coin supérieur gauche
window-resize-top-right = Redimensionner par le coin supérieur droit
window-resize-bottom-left = Redimensionner par le coin inférieur gauche
window-resize-bottom-right = Redimensionner par le coin inférieur droit

# Groupes de réglages du format d'import (#170)
import-format-delimiter-section = Séparateur
import-format-numbers-section = Nombres et dates
import-format-comments-section = Lignes de commentaire
import-format-file-section = Blocs et encodage
import-format-skipped-section = Lignes ignorées
