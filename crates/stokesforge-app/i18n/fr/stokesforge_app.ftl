# Catalogue français de l'interface de StokesForge (issue #49, chantier #39).
#
# Traduction du catalogue de référence `en` : mêmes clés, mêmes arguments, selon
# la colonne française de `docs/textes-interface.md` (infinitif pour les
# consignes, minuscule après la première lettre, « … » en un caractère,
# espace insécable avant « : », « ; », « ? » et dans les guillemets « »).
# Toute clé ajoutée à `en` s'ajoute ici dans le même commit : le test de
# complétude est bloquant.

## Fenêtre

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
menu-panels = Panneaux
menu-toolbars = Barres d’outils
menu-sheet = Feuille
menu-columns = Colonnes
menu-language = Langue

action-add-workbook = Ajouter un classeur
    .hint = Ajoute un classeur avec des colonnes X et Y
action-new-project = Nouveau projet
    .hint = Crée un projet vide dans cette fenêtre
action-add-graph = Ajouter un graphique
    .hint = Ajoute un graphique vide avec une zone de tracé
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
overwrite-title = Remplacer le fichier
overwrite-confirm = « { $name } » existe déjà. Le remplacer ?
overwrite-replace = Remplacer
    .hint = Écrase le fichier existant ; cette action ne peut pas être annulée
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
status-loaded = Chargé depuis { $path }
status-nothing-to-undo = Rien à annuler
status-nothing-to-redo = Rien à rétablir
status-clipboard-unreadable = Presse-papiers vide ou illisible
status-paste-non-contiguous = Impossible de coller dans une sélection non contiguë
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
error-file-too-new = { $path } demande une version plus récente de StokesForge (format de fichier { $found }, pris en charge jusqu'à { $supported })
error-file-corrupt = { $path } est endommagé : { $detail }
error-file-invalid-project = { $path } ne contient pas de projet valide : { $detail }
error-file-unknown-feature = { $path } utilise des fonctions d'une version plus récente de StokesForge ({ $detail })
error-file-changed = { $path } a été modifié ailleurs depuis son ouverture ou son enregistrement. Le recharger avant d'enregistrer

# Import
error-import-binary = Le fichier n'est pas un texte lisible
error-import-empty = Le fichier est vide
error-import-no-data = Aucune donnée après l'en-tête. Vérifier le nombre de lignes d'en-tête
error-import-file = { $path } : { $detail }

# Formules
error-formula-invalid = Formule invalide : { $detail }
error-formula-unknown-column = La colonne { $letter } n'existe pas dans cette feuille
error-formula-self-reference = Une formule ne peut pas faire référence à sa propre colonne ({ $letter })
error-formula-cycle = Des formules se renvoient l'une à l'autre en boucle. Retirer l'une des références

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

plot-axis-bottom = X (bas)
plot-axis-left = Y (gauche)
plot-axis-right = Y secondaire (droite)
plot-axis-top = X secondaire (haut)
plot-axis-overlay-x = X (haut)
plot-axis-overlay-y = Y (droite)

# Ce que désigne un clic droit (tête du menu contextuel)

plot-target-graph = Graphique
plot-target-title = Titre
plot-target-plot-area = Zone de tracé
plot-target-axis = Axe { $axis }
plot-target-axis-title = Titre de l'axe { $axis }
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
plot-axes-reverse =
    Axe inversé
    .hint = Les valeurs décroissent dans le sens de lecture
plot-axes-auto-range =
    Plage auto
    .hint = Calculée d'après les données
    .disabled = Décochez Plage auto pour fixer les bornes
plot-axes-from = De
plot-axes-display-section = Affichage
plot-axes-decimal-comma = Virgule décimale sur les graduations
    .hint = Écrit 1,5 au lieu de 1.5 sur tous les axes de ce graphe, à l'écran et dans les exports
plot-axes-show-axis = Afficher l'axe
plot-axes-show-gridlines = Afficher le quadrillage
plot-axes-show-tick-labels = Afficher les étiquettes de graduation
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
curve-gradient-needs-group = Mettez la série dans un groupe pour appliquer un nuancier
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
curve-gradient-apply =
    Appliquer au groupe
    .hint = Répartit le nuancier régulièrement sur les séries du groupe, dans l'ordre de la liste
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
plot-theme-global-colors = Couleurs globales
plot-theme-background = Fond
plot-theme-text = Texte
plot-theme-axes = Axes
plot-theme-note = S'applique à tous les titres, axes, graduations et quadrillages. Remplace les couleurs réglées dans les autres onglets

# Mémo de la syntaxe Typst, sous chaque champ de titre

plot-formula-memo-header = Syntaxe des formules
plot-formula-greek = Lettres grecques λ, Δ
plot-formula-subscript = Indice
plot-formula-superscript = Exposant
plot-formula-full = Formule complète
plot-formula-upright = Texte droit dans une formule
plot-formula-format = Mise en forme du texte
plot-formula-escape = Le caractère lui-même
plot-formula-footer = Le texte entre $ … $ est une formule, le reste est du texte simple. C'est la syntaxe de Typst ; référence complète : { $url }

# Texte sur le graphique (éditeur en place) et vue du graphique

plot-text-size = Taille de police (pt)
plot-text-rotation = Rotation (°)
plot-text-ok =
    OK
    .hint = Applique le texte (Entrée)
plot-text-placeholder = Texte, $formule$
plot-view-reset-zoom = Réinitialiser le zoom
plot-layer-badge = { $n }
    .hint = Fait du calque { $n } la cible des ajouts à ce graphique

# Export

export-dialog-title = Exporter le graphique

## Classeur, gestionnaire de projet, notes, couleurs, console (#45)

# Gestionnaire de projet
project-manager-title = Gestionnaire de projet
project-manager-show = Afficher
    .hint = Ouvre ou réaffiche les fenêtres sélectionnées
project-manager-hide = Masquer
    .hint = Masque les fenêtres sélectionnées sans les fermer
project-folder-empty = (vide)

# Classeur : en-tête de colonne empilé (gouttière de gauche)
column-x-prime = x'
status-x-prime-invalid = x' doit être un nombre fini
column-long-name = Nom long
column-units = Unités
column-comments = Commentaires
workbook-header-formula = F(x)=
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
    .hint = Glisser pour déplacer la vue, ou pour déplacer et redimensionner textes et objets ; Ctrl + molette pour zoomer
tool-zoom = Zoom
    .hint = Tracer un rectangle pour zoomer dessus
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
format-formula = Formule
    .hint = Écrit le texte sélectionné comme une formule mathématique
format-font = Police
    .hint = Règle la police de toute la zone de texte, formules comprises. Une police du système change la figure sur un ordinateur qui ne l'a pas
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
object-manager-show-texts = Afficher les textes du graphique
    .hint = Liste dans le gestionnaire d’objets le titre du graphique, les titres des calques et des axes et la légende. Ne modifie pas leur visibilité sur le graphique. Non listés par défaut.
object-manager-show-axes = Afficher les axes
    .hint = Liste les axes de chaque calque dans le gestionnaire d’objets. Ne modifie pas leur visibilité sur le graphique. Non listés par défaut.

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

# Panneaux fixes et fenêtres détachées (#100)
panel-expand = Déployer le panneau
panel-collapse = Replier le panneau
panel-detach = Détacher dans une fenêtre séparée
panel-reattach = Rattacher à la fenêtre principale

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
analysis-kind-baseline = Ligne de base
analysis-kind-correct = Correction
analysis-kind-integrate = Intégration
analysis-kind-fit = Ajustement
analysis-kind-peaks = Décomposition en pics
analysis-kind-reconvolution = Reconvolution
analysis-kind-direct-deconvolution = Déconvolution directe
error-model-calculated-column-read-only = Cette colonne est produite par un calcul. La détacher du calcul pour modifier ses valeurs

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
analysis-issue-needs-points = Placez au moins { $needed } points ({ $have } placés)
analysis-issue-needs-zone = Tracez au moins une zone de fond
analysis-issue-no-parameter = Le modèle n'a aucun paramètre
analysis-issue-bad-model = Le modèle est invalide : { $detail }
analysis-issue-missing-column = Une colonne de ce calcul n'existe plus

## Analysis — menu (#181)
menu-analysis = Analyse
analysis-block-type = Type d'analyse
action-analysis-open = Analyse…
    .hint = Ouvre la fenêtre d'analyse ; le type d'analyse s'y choisit
analysis-needs-y = Sélectionnez d'abord au moins une colonne Y
analysis-menu-baseline = Ligne de base
analysis-baseline-linear = Droite…
    .hint = Soustrait ou divise par une droite passant par deux points
analysis-baseline-polynomial = Polynôme…
    .hint = Ajuste un polynôme sur des zones de fond, puis le soustrait ou divise
analysis-baseline-anchors = Points d'ancrage…
    .hint = Soustrait ou divise par une courbe passant par les points d'ancrage posés
action-analysis-correct = Corriger par une référence…
    .hint = Divise, multiplie ou soustrait une colonne de référence, interpolée sur les données
action-analysis-integrate = Intégrer…
    .hint = Calcule des aires sur une ou plusieurs plages, et l'intégrale cumulée
analysis-menu-fit = Ajuster
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
    .hint = Ajuste une formule que vous écrivez, avec ses propres paramètres
analysis-menu-peaks = Pics et déconvolution
action-analysis-peaks = Décomposer en pics…
    .hint = Ajuste plusieurs pics à la fois, trouvés automatiquement ou posés à la main
action-analysis-reconvolution = Reconvolution…
    .hint = Ajuste des exponentielles convoluées avec la réponse instrumentale
action-analysis-direct = Déconvolution directe…
    .hint = Retrouve le signal avant la réponse instrumentale, sans modèle
action-analysis-recalculate-stale = Recalculer les calculs périmés
    .hint = Recalcule chaque calcul dont les données ont changé
    .disabled = Tout est à jour
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
analysis-field-reference = Référence
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
analysis-output-heavy = Un gros lot : recalculer à chaque modification peut être lent
analysis-tool-points = Poser des points
    .hint = Cliquez pour poser un point ; Maj aimante sur la courbe, Maj + molette règle la moyenne
analysis-tool-zone = Zone
    .hint = Glissez pour marquer une zone ou une plage
analysis-tool-zoom = Zoom
    .hint = Glissez un rectangle pour zoomer ; la molette zoome autour du curseur
analysis-tool-reset = Réinitialiser la vue
analysis-warning-step-not-uniform = Le pas de X n'est pas constant : le résultat peut être imprécis
analysis-warning-irf-step = La réponse a un autre pas de X que les données
analysis-warning-negative-clipped = { $count } points négatifs ont été ramenés à zéro
analysis-warning-undetermined = { $count } paramètre(s) ne sont pas déterminés par les données : aucune incertitude n'est donnée
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
error-plot-settings-cancel = Impossible d'annuler les propriétés du graphe : { $cause }
error-plot-settings-undo-group = Les modifications des propriétés du graphe ne pourront pas être annulées en une fois : { $cause }
error-plot-settings-block-detached = Les propriétés du graphe sont ouvertes. Les valider ou les annuler avant de modifier une fenêtre détachée
error-preferences-not-saved = Impossible d'enregistrer les préférences : { $cause }
error-colors-session-only = Les couleurs enregistrées ne seront gardées que pour cette session : { $cause }
error-script-vanished = Le moteur de script s'est arrêté. Relancer le script
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
error-layer-gone = Ce calque n'est plus dans le graphique
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
field-refused-use-auto = Utilisez Plage auto
field-refused-name-empty = Le nom ne peut pas être vide
field-refused-name-taken = Ce nom est déjà utilisé
field-refused-range-order = Le début de la plage ne doit pas dépasser sa fin

# Champ numérique : bulle du ▼ (#220).
number-presets = Valeurs courantes

## Raisons de grisage (#226) : un élément grisé dit pourquoi et quoi faire

disabled-object-gone = Cet élément n'existe plus
disabled-move-first = Déjà en premier
disabled-move-last = Déjà en dernier
disabled-visibility-empty = Rien à afficher ou masquer
disabled-axis-blocked = Un calque au-dessus dessine déjà cet axe
disabled-axis-on-overlay = Indisponible sur un calque superposé
disabled-text-not-moved = Ce texte n'a pas été déplacé
disabled-no-graph-data = Ce graphique n'a aucune donnée à montrer
disabled-nothing-to-undo = Rien à annuler
disabled-nothing-to-redo = Rien à rétablir
disabled-new-project-busy = Terminez ou annulez d'abord le script, l'import ou la modification en cours
disabled-no-source-file = Aucune colonne de cette feuille ne vient d'un fichier
disabled-no-last-import = Aucun import terminé à rouvrir
disabled-needs-sheet = Mettez d'abord une feuille de classeur au premier plan
disabled-no-window = Aucune fenêtre à agrandir
disabled-import-not-data-line = Seule une ligne de données ou de pied de page peut terminer les données
disabled-import-auto-end-off = La fin automatique des données est déjà désactivée
disabled-import-builtin-template = Seul un modèle que vous avez enregistré peut être supprimé
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
disabled-analysis-issues = Corrigez d'abord le problème signalé dans le formulaire

## Aide › Raccourcis clavier (#231, K5). Les listes sont générées du registre
## des touches (`app/shortcuts.rs`, `HELP`).

menu-help = Aide
action-keyboard-shortcuts = Raccourcis clavier…
    .hint = Liste les touches et les gestes de l'application
keyboard-shortcuts-title = Raccourcis clavier
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
status-recalculating = Recalcul en cours…
status-recalc-stopped = Recalcul arrêté. La modification a été annulée
