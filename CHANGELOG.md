# Journal des modifications

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/).
Les versions suivent la numérotation `MAJEUR.MINEUR.CORRECTIF` ; les critères
de la V1 sont dans [`docs/criteres-v1.md`](docs/criteres-v1.md).

Ce qui compte comme compatibilité à partir de la V1 : le format de fichier
`.stf` (un fichier V1 se relit et se migre jusqu'en V3, invariant 12) et
l'API `pyst` ([« API figée V1 »](docs/python-et-script.md)). Le reste du code
n'a pas de promesse de compatibilité.

Une section par version, titrée `## [X.Y.Z] - date`. Le travail de publication
(`scripts/release-notes.sh`) lit la section de la version du tag et refuse un
tag dont la section est absente, vide ou encore « à dater » (toléré sur un
`-rcN`).

Qui remplit `[Non publié]` : chaque changement visible de l'utilisateur y ajoute
une ligne, dans le commit qui le porte. Le commit de version renomme cette
section en `[X.Y.Z] - date` et en rouvre une vide. Un correctif repris par
cherry-pick vers `release/N.x` y porte aussi sa ligne. Un champ additif du
format `.stf` y est annoncé (une version plus ancienne qui réenregistre le
fichier le perd sans message, voir `docs/persistance.md`).

## [Non publié]

### Ajouté

- Sous-graduations des axes (#344) : une case « Afficher les sous-graduations »
  par axe (allumée sur un graphe neuf, éteinte dans un fichier existant), seules
  ou avec les graduations, leur nombre entre deux graduations, et « Un nombre
  sur » n pour n'écrire qu'une graduation sur n. Champs additifs du format `.stf`
  (`minor_ticks`, `minor_tick_count`, `label_every`) : une version plus ancienne
  qui réenregistre le fichier les perd.
- Bloc repliable « Avancé » de la section Graduations (#344), fermé par
  défaut, par axe (miroirs compris) : longueur des grands et des petits tirets
  (px), épaisseur (pt) et couleur des tirets, tous Auto tant qu'on n'y touche
  pas (le petit tiret Auto vaut la moitié du grand). Champs additifs du format
  `.stf` (`tick_length`, `minor_tick_length`, `tick_width`,
  `tick_mark_color`) : une version plus ancienne qui réenregistre le fichier
  les perd.

### Modifié

- Page Axes des propriétés (#344) : les cases « Axe inversé » et « Plage
  auto » disparaissent. Le sens de l'axe se lit sur l'ordre des bornes (De >
  À inverse l'axe) ; De et À sont toujours éditables, suivent les données tant
  qu'on n'a rien saisi, et une saisie fige les deux bornes (le bouton « Auto »
  de la rangée les rend aux données, sens conservé). Les graduations ont leur
  section « Graduations ».
- À l'ouverture d'un fichier `.stf`, un axe qui porte `inverted` à côté de
  bornes croissantes voit ses bornes échangées (le champ `inverted` reste
  écrit dans le fichier, la figure ne change pas) ; la surface `pyst`
  (`x_inverted`, `x_auto_range`…) ne change pas, mais `x_inverted=True` échange
  aussi les bornes.
- La case « Cadre fermé » (page Axes et menu du calque) disparaît de
  l'interface ; ses réglages se font par axe (miroirs, graduations).

## [1.0.0] - 2026-10-27

Première version publiée. Portage de StokesForge de Python/Qt vers Rust
(l'ancienne implémentation reste au tag `legacy-python`).

### Ajouté

- Application de bureau (Linux et Windows) : classeurs et feuilles, import de
  fichiers d'appareil avec prévisualisation, formules par colonne, graphes
  (rendu, export SVG, PDF et PNG, copie de la figure au presse-papiers),
  historique annulable.
- Analyse : lignes de base, correction, intégrale, ajustements, pics,
  déconvolution, lissage de données (#324).
- Rôles de colonne supplémentaires X3, X4… et Y3, Y4… : calques superposés
  (#168).
- Scripts Python embarqués par le module `pyst`, dont l'API est figée
  (#331) ; `stokesforge-cli import` et `stokesforge-cli run`.
- Première ouverture : un projet de démonstration est offert, enregistré dans
  `demo.stf` du dossier de configuration ; les lancements suivants démarrent
  sur un projet vide.
- Distribution autonome : archive Linux x86_64 et installeur Windows, avec
  l'interpréteur Python embarqué et les notices de licences (#332).

### Installation sous Windows

- L'installeur n'est pas signé : Windows affiche « éditeur inconnu » (ou « Windows
  a protégé votre ordinateur ») au lancement. C'est attendu ; choisir « Informations
  complémentaires » puis « Exécuter quand même ».

### Format de fichier `.stf`

- Version 1 figée (#323) : tout fichier V1 se relira et se migrera jusqu'en V3.
  Un fichier d'une version plus récente est refusé avec un message qui invite
  à mettre à jour StokesForge.
- Les fichiers écrits avant la V1 (développement) ne sont pas garantis.

### Corrigé

- Fichier `.stf` d'un tiers : chargement quadratique, amplification mémoire et
  recalcul non interruptible (#316).
- Séries « ligne » : la suppression de lignes réécrit le numéro de ligne (#307).
- Une courbe tracée depuis une ligne peut être analysée (#253).
- Project Manager : « Hide » ferme la fenêtre comme la croix (#327).
- Tests d'état de l'application : la langue de la personne qui teste ne fuit
  plus dans les tests (#329).
- Fenêtre d'analyse : libellés qui passent à la ligne, pluriel de l'aperçu,
  rond d'aimantation (#334, #335, #336).
