# Notices complémentaires des polices d'egui

`epaint_default_fonts` 0.36.2 embarque Hack, Ubuntu Light, Noto Emoji et
emoji-icon-font. Ses métadonnées Cargo déclarent MIT/Apache-2.0, OFL-1.1 et
Ubuntu-font-1.0, mais son paquet ne livre qu'un texte MIT dans `fonts/`.
`cargo-about` restitue les autres textes génériques sans tous les copyrights
des polices. En outre, Hack conserve une permission **Bitstream Vera** qui
n'apparaît pas dans l'expression du manifeste de ce crate.

`NOTICE.txt` conserve les champs de copyright (name ID 0), de licence (13)
et d'URL (14) des quatre fichiers TTF, ainsi que le texte MIT de
`emoji-icon-font-mit-license.txt`, extraits du paquet crates.io sans modifier
leur contenu. La notice de Hack inclut intégralement MIT et Bitstream Vera.
Les textes OFL et Ubuntu génériques sont aussi présents dans les notices
Rust générées. Les SHA-256 des sources sont dans `provenance.json`.

Ces notices sont livrées par `scripts/licenses-dist.sh`. Une mise à jour
d'`epaint_default_fonts` exige de vérifier à nouveau les polices et leurs
permissions et d'actualiser ces fichiers, en plus du contrôle Cargo. La
détection par métadonnées ne suffit pas pour les ressources embarquées.
