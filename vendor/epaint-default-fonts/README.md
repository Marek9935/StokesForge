# Supplementary notices for egui's fonts

`epaint_default_fonts` 0.36.2 embeds Hack, Ubuntu Light, Noto Emoji and
emoji-icon-font. Its Cargo metadata declares MIT/Apache-2.0, OFL-1.1 and
Ubuntu-font-1.0, but its package ships only an MIT text in `fonts/`.
`cargo-about` renders the other generic texts without all the fonts'
copyrights. In addition, Hack keeps a **Bitstream Vera** permission that does
not appear in this crate's manifest expression.

`NOTICE.txt` keeps the copyright (name ID 0), license (13) and URL (14)
fields of the four TTF files, as well as the MIT text of
`emoji-icon-font-mit-license.txt`, extracted from the crates.io package
without changing their content. Hack's notice includes MIT and Bitstream Vera
in full. The generic OFL and Ubuntu texts are also present in the generated
Rust notices. The SHA-256 of the sources are in `provenance.json`.

These notices are shipped by `scripts/licenses-dist.sh`. An update of
`epaint_default_fonts` requires checking the fonts and their permissions
again and refreshing these files, in addition to the Cargo check. Detection
through metadata is not enough for embedded resources.
