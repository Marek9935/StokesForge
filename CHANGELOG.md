# Changelog

Format inspired by [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow the numbering `MAJOR.MINOR.PATCH`; the V1 criteria are in
[`docs/criteres-v1.md`](docs/criteres-v1.md).

What counts as compatibility from V1 on: the `.stf` file format (a V1 file can
be read back and migrated up to V3, invariant 12) and the `pyst` API
(["V1 frozen API"](docs/python-et-script.md)). The rest of the code has no
compatibility promise.

One section per version, titled `## [X.Y.Z] - date`. The release job
(`scripts/release-notes.sh`) reads the section of the tag's version and
refuses a tag whose section is missing, empty or still "undated" (tolerated
on an `-rcN`).

Who fills in `[Unreleased]`: every change visible to the user adds a line
there, in the commit that carries it. The version commit renames this section
to `[X.Y.Z] - date` and opens a new empty one. A fix cherry-picked to
`release/N.x` also carries its line there. An additive field of the `.stf`
format is announced there (an older version that saves the file again loses it
without a message, see `docs/persistance.md`).

## [Unreleased]

### Added

- Minor ticks on axes (#344): a "Show minor ticks" box per axis (on in a new
  graph, off in an existing file), alone or together with the major ticks, the
  number of minor ticks between two ticks, and "Label every" n to write only
  one tick label out of n. Additive fields of the `.stf` format
  (`minor_ticks`, `minor_tick_count`, `label_every`): an older version that
  saves the file again loses them.
- Collapsible "Advanced" block in the Ticks section (#344), closed by
  default, per axis (mirrors included): length of the major and minor tick
  marks (px), thickness (pt) and color of the tick marks, all Auto until they
  are touched (the Auto minor tick mark is half the major one). Additive
  fields of the `.stf` format (`tick_length`, `minor_tick_length`,
  `tick_width`, `tick_mark_color`): an older version that saves the file again
  loses them.

### Changed

- Axes page of the properties (#344): the "Reverse axis" and "Auto range"
  boxes disappear. The direction of the axis is read from the order of the
  bounds (From > To inverts the axis); From and To are always editable, follow
  the data until something is typed, and typing freezes both bounds (the
  "Auto" button of the row gives them back to the data, direction kept). The
  ticks have their own "Ticks" section.
- When a `.stf` file is opened, an axis that carries `inverted` next to
  increasing bounds has its bounds swapped (the `inverted` field is still
  written in the file, the figure does not change); the `pyst` surface
  (`x_inverted`, `x_auto_range`…) does not change, but `x_inverted=True` also
  swaps the bounds.
- The "Closed Frame" box (Axes page and layer menu) disappears from the
  interface; its settings are made per axis (mirrors, ticks).
- English text in the tools (#376): the `pyst` docstrings (`help()` in the
  console), the output of `stokesforge-selftest` and `stokesforge-pyhello`,
  the default recipe name of the CLI and of `pyst` ("Untitled", was "Sans
  titre"), the encoding shown for UTF-16 files in the import window
  ("little-endian"/"big-endian"), and the name given to an imported file
  without a stem ("file"). Names and parameters of the `pyst` API do not
  change; the built-in recipe names stay as they are (#378).
- The robustness probes are renamed `stokesforge-alloc-probe` and
  `stokesforge-stack-probe` (arguments `build`, `undo`, `clear`, `drop`).

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
