# Catalogue français de l'interface de StokesForge (issue #49, chantier #39).
#
# Traduction du catalogue de référence `en` : mêmes clés, mêmes arguments, selon
# la colonne française de `docs/textes-interface.md` (infinitif pour les
# consignes, minuscule après la première lettre, « … » en un caractère,
# espace insécable avant « : », « ; », « ? » et dans les guillemets « »).
# Toute clé ajoutée à `en` s'ajoute ici dans le même commit : le test de
# complétude est bloquant.

## Fenêtre

window-title = StokesForge
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
new-project-confirm = Abandonner les modifications non enregistrées et créer un projet vide ?
new-project-discard = Abandonner les modifications
    .hint = Remplace le projet courant ; cette action ne peut pas être annulée
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
status-revision = Révision { $revision }
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
import-files-delete-confirm = Supprimer le modèle « { $name } » ?
import-files-delete-yes = Supprimer
import-files-delete-keep = Garder
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
plot-axes-min = Min
plot-axes-max = Max
plot-axes-display-section = Affichage
plot-axes-show-axis = Afficher l'axe
plot-axes-show-gridlines = Afficher le quadrillage
plot-axes-color = Couleur de l'axe
plot-axes-color-note = Trait de l'axe, graduations et étiquettes de graduation. Le quadrillage la reprend, estompée

# Onglet Séries

plot-series-empty = Ce graphique n'a aucune série
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
plot-formula-footer = Le texte entre $ … $ est une formule, le reste est du texte simple. Syntaxe complète : { $url }

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
workbook-grid-no-columns = Aucune colonne dans cette feuille
workbook-no-worksheets = Ce classeur n'a aucune feuille
workbook-worksheet-add = Ajouter une feuille
action-rename-sheet = Renommer
    .hint = Modifier le nom de la feuille
action-duplicate-sheet = Dupliquer
    .hint = Ajouter une copie de cette feuille, colonnes comprises
action-delete-sheet = Supprimer
    .hint = Supprimer cette feuille et ses colonnes
    .disabled = Un classeur garde au moins une feuille
sheet-copy-name = { $name } copie { $number }
sheet-invalid = { $id } n'est pas une feuille de ce projet
sheet-rename-empty = Le nom d'une feuille ne peut pas être vide ; « { $name } » conservé
sheet-rename-taken = Une feuille nommée « { $name } » existe déjà dans ce classeur ; « { $old } » conservé
sheet-delete-last = Un classeur garde au moins une feuille
sheet-delete-title = Supprimer la feuille
sheet-delete-confirm = Supprimer la feuille « { $name } » et ses colonnes ?
sheet-delete-used = Tracée par : { $graphs }
sheet-delete-accept = Supprimer
    .hint = Supprimer la feuille ; Annuler la rétablit
matrix-unavailable = L'édition des matrices n'est pas encore disponible
note-deleted = Cette note a été supprimée

# Console de script
script-title = Script Python
script-unavailable = — Indisponible dans cette version
script-run = Exécuter
script-stop = Arrêter
script-running = Exécution…
script-not-run = Pas encore exécuté
script-stdout = Sortie standard
script-stderr = Sortie d'erreur
script-traceback = Trace d'appels
script-no-output = (aucune sortie)

# Sélecteur de couleur
color-none = Aucune couleur (transparent)
color-none-swatch = Aucune couleur (transparent, sans fond dans les exports)
color-custom = + Personnalisée
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
color-opacity = Opacité
color-rgb = RVB
color-hsl = TSL
color-hex = Hexa
    .hint = #RRGGBB, ou #RRGGBBAA avec l'opacité
color-channel-red = R
color-channel-green = V
color-channel-blue = B
color-channel-hue = T
color-channel-saturation = S
color-channel-lightness = L
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
format-object-disabled = Sélectionnez un objet du graphique pour le mettre en forme
format-object-line-color = Couleur du trait
    .hint = Règle la couleur du trait de l'objet sélectionné
format-object-fill-color = Couleur de remplissage
    .hint = Règle le remplissage du rectangle ou de l'ellipse sélectionné{NB}; l'opacité se règle dans le sélecteur de couleur
format-object-no-fill = Aucun remplissage
format-object-line-width = Épaisseur du trait
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
object-manager-no-graph = Activer un graphique pour voir ses objets.
object-manager-visibility = Afficher ou masquer ce contenu. Les séries masquées sont exclues de la légende et des bornes automatiques des axes.
object-manager-layer = Calque { $index }
object-manager-group = Groupe de tracés
object-manager-axes = Axes
object-manager-show-texts = Afficher les textes du graphique
    .hint = Liste dans le gestionnaire d’objets le titre du graphique, les titres des calques et des axes et la légende. Ne modifie pas leur visibilité sur le graphique. Non listés par défaut.
object-manager-show-axes = Afficher les axes
    .hint = Liste les axes de chaque calque dans le gestionnaire d’objets. Ne modifie pas leur visibilité sur le graphique. Non listés par défaut.

menu-selected-content = Sélection
    .disabled = Sélectionnez d’abord une courbe, un texte ou un objet
action-rename-series = Renommer
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
object-delete-title = Supprimer des objets
object-delete-confirm = Supprimer { $names } ?
object-delete-used = Utilisés par les graphiques : { $graphs }

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
