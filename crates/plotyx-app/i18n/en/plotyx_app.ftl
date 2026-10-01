# Catalogue de référence de l'interface de PlotYx (issue #41, chantier #39).
#
# L'anglais est la langue source : une clé absente d'ici n'existe pas, et `tr!`
# (`src/i18n.rs`) refuse à la compilation une clé qui n'y figure pas. Règles de
# rédaction : `docs/textes-interface.md` ; mécanique : skill
# `plotyx-textes-interface`.
#
# Nommage des clés : `zone-objet[-précision]`, en minuscules et tirets.

## Window

window-title = PlotYx

## Menu bar

menu-file = File

## Witness of the machinery
## (read by the tests of `i18n.rs` only: one argument and one CLDR plural; to be
## removed once a real key with arguments exists)

witness-count =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }
