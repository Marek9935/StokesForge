# Notices complémentaires des dépendances Rust

`cargo-about` restitue parfois un texte générique lorsqu'un paquet crates.io
ne livre pas son fichier de licence, ou ne livre qu'un renvoi à ce texte.
Il faut aussi conserver les copyrights et les permissions présents dans les
sources amont. `NOTICE.txt` complète le rapport généré, sans remplacer ses
textes de licence. `scripts/licenses-rust.sh` concatène les deux en CI et
dans les distributions.

`provenance.json` relie les textes aux paquets/version, commits déclarés dans
leurs `.cargo_vcs_info.json`, URL d'origine et SHA-256 des textes. Les textes
identiques sont regroupés, avec toutes leurs références et tous les paquets.
Les fichiers de licence sont conservés intégralement ; la section License
du README de Glidesort est un extrait identifié. Pour `dispatch` 0.2.0,
l'instantané ne contient pas de fichier de licence : sa déclaration MIT et
ses auteurs proviennent du manifeste, repris dans le rapport généré.

Points relevés lors de la vérification :

- `brotli-decompressor` 5.0.3 déclare MIT dans son manifeste, mais livre un
  texte BSD-3-Clause de Dropbox : `deny.toml` et `about.toml` le clarifient
  avec le hash de son fichier `LICENSE`, conservé sans modification ;
- `dpi` livre les permissions de son code `libm` dans `LICENSE-LIBM-MIT` ;
- `r-efi` les livre dans `AUTHORS` ;
- `siphasher` et `zune-image` livrent des copyrights avec un renvoi à
  MIT/Apache ; le rapport généré fournit également leurs conditions intégrales ;
- les fichiers `LICENSE.md` d'objc2 décrivent les conditions et limites
  signalées par cet amont pour les SDK Apple ; ils sont conservés tels quels.

Le champ repository de `zune-core` 0.4.12 est absent ; son dépôt zune-image
est identifié par la crate sœur `zune-jpeg`, puis le texte est lu au commit
exact déclaré par `zune-core`. Aucun texte n'est pris sur une branche mobile.

## Mettre à jour

`checksums.sha256` lie ces compléments au **Cargo.lock exact** et à leurs
fichiers de provenance. Une modification du verrou bloque leur génération
tant que les compléments n'ont pas été relus. Depuis la racine :

1. Produire le rapport détaillé avec la version figée de cargo-about :
   `cargo about generate --workspace --all-features --locked --fail --format json -o target/licenses/report.json`.
2. Examiner les licences dont `source_path` est absent. Conserver les textes
   amont et copyrights manquants, à la référence publiée par chaque paquet ;
   les licences/copyrights peuvent aussi se trouver dans `COPYING`, `AUTHORS`,
   un README, ou les ressources binaires. Confronter tout écart avec le
   manifeste, et préciser les exceptions dans les configurations des outils.
3. Actualiser `NOTICE.txt` et `provenance.json`, puis leurs empreintes :
   `sha256sum Cargo.lock vendor/crate-license-notices/NOTICE.txt vendor/crate-license-notices/provenance.json > vendor/crate-license-notices/checksums.sha256`.
4. Rejouer le contrôle, `scripts/licenses-test.sh` et
   `scripts/licenses-rust.sh target/licenses/THIRD-PARTY-LICENSES.txt`.

Les polices d'egui ont leur notice dédiée dans
[`../epaint-default-fonts/`](../epaint-default-fonts/README.md). Les compléments
ne constituent pas une archive du code source correspondant.
