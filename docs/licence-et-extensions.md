# Licence et interfaces d'extension

Copyright © 2026 Aliocha Skrzypczak. Le code original de StokesForge de la
branche `dev` est distribué sous **GNU GPL version 3 ou ultérieure**,
avec la permission supplémentaire de [`LICENSE-EXCEPTION`](../LICENSE-EXCEPTION).
Le texte officiel de la GPLv3 est reproduit sans modification dans
[`LICENSE`](../LICENSE). Les composants tiers conservent leurs licences.
Cette décision ne change pas le statut du tag `legacy-python`, l'ancienne version
Python/Qt, ni celui des versions antérieures à ce changement.

La GPL autorise les distributions commerciales ; la distribution du
programme ou d'une version modifiée doit respecter ses conditions, notamment
la fourniture du code source correspondant. La permission supplémentaire
relève de la [section 7 de la GPLv3](https://www.gnu.org/licenses/gpl-3.0.html#section7).
Le texte de l'exception reprend la décision de l'issue #129 ; sa relecture
par un juriste reste recommandée avant publication.

## Liste fermée des interfaces couvertes

| Interface d'extension documentée | Contrat public | Utilisation |
|---|---|---|
| `pyst`, API de script Python fournie par StokesForge | [`python-et-script.md`](python-et-script.md), liste figée « API figée V1 » (noms, signatures, clés, valeurs ; tout changement est une décision explicite, #331) | Scripts exécutés dans la console de l'application ou par `stokesforge-cli run` |

Un script ou module qui utilise uniquement cette interface, sans compiler,
incorporer ni lier du code source ou objet de StokesForge dans son propre
programme, peut être distribué sous la licence choisie par son auteur.
Le chargement du module `pyst` fourni par l'hôte est l'usage de l'interface
couvert par cette permission. Les dépendances propres au script conservent
leurs conditions.

L'exception ne couvre pas les modifications de StokesForge, la réutilisation
de ses sources ou une bibliothèque ou un binaire lié aux crates Rust de
StokesForge. Les API Rust internes, le format `.stf` et les interfaces des
dépendances tierces ne sont pas ajoutés à cette liste par leur seule présence
dans la documentation.

**Une nouvelle interface n'est couverte qu'après un commit explicite qui
l'ajoute à cette table et décrit son contrat public.** Ajouter une fonction
à l'API `pyst` existante actualise son contrat ; cela ne crée pas une nouvelle
interface. Ne pas appliquer l'exception par analogie à une future liaison.

## Métadonnées Cargo

Le workspace déclare
`GPL-3.0-or-later WITH AdditionRef-StokesForge-Extension` ; les vingt
crates héritent de `license`. Les textes sont `LICENSE` et `LICENSE-EXCEPTION`.
`AdditionRef-StokesForge-Extension` identifie cette permission supplémentaire.
La syntaxe [SPDX 3.0](https://spdx.github.io/spdx-spec/v3.0.1/annexes/spdx-license-expressions/)
utilise `AdditionRef-` après `WITH` pour un ajout personnalisé ; `LicenseRef-`
désigne une licence entière et ne convient pas à cet emplacement.

Les licences Apache-2.0 de `vendor/winit` et MIT de
`vendor/smithay-clipboard` restent intactes, y compris pour les modifications
locales décrites dans [`vendor/winit/STOKESFORGE.md`](../vendor/winit/STOKESFORGE.md).

## Licences tierces et distribution

La politique [`deny.toml`](../deny.toml) contrôle toutes les plateformes et
features Cargo, y compris `python` et les dépendances de test, sans compiler
l'interpréteur. Les licences
admises sont explicites. MPL-2.0 (`option-ext` 0.2.0, et `colored` 2.2.0, tirée seulement par les tests
d'interface `egui_kittest` → `dify` et jamais livrée) et Ubuntu-font-1.0
(`epaint_default_fonts` 0.36.2) sont autorisées seulement pour ces versions.
Les six familles de polices de `vendor/fonts` portent chacune leur OFL-1.1 ;
la paire Noto Sans inclut aussi sa face italique. Phosphor porte sa notice MIT.
Les polices embarquées par egui ont également des notices complémentaires
dans `vendor/epaint-default-fonts/`, notamment Bitstream Vera pour Hack,
absente des métadonnées Cargo. Leur provenance est vérifiée séparément ;
le contrôle des manifestes ne remplace pas celui des ressources.
Le manifeste de `brotli-decompressor` 5.0.3 annonce MIT, mais son `LICENSE`
porte BSD-3-Clause : les deux outils le clarifient avec l'empreinte de ce texte.

Installation des outils figés, depuis la racine :

```sh
source scripts/licenses-tools.env
cargo install cargo-deny --locked --version "$CARGO_DENY_VERSION"
cargo install cargo-about --locked --features cli --version "$CARGO_ABOUT_VERSION"
cargo deny --locked check licenses
```

`cargo-about` produit les textes à partir de `Cargo.lock` et de
[`about.toml`](../about.toml), avec le gabarit
[`scripts/licenses.hbs`](../scripts/licenses.hbs). Le travail CI `licenses`
est automatique et bloquant : il exécute le contrôle puis génère les notices,
conservées comme artefact. Il exerce aussi les témoins de refus de
`scripts/licenses-test.sh` (notices manquantes/périmées, verrou Cargo différent,
licence interdite).
Il ne remplace pas les tests Python.

[`scripts/licenses-rust.sh`](../scripts/licenses-rust.sh) produit le même
rapport en CI et dans les distributions. Il ajoute les notices/copyrights
absents des paquets crates.io, conservés dans
[`vendor/crate-license-notices/`](../vendor/crate-license-notices/README.md)
avec leurs références exactes et empreintes. Il refuse un verrou Cargo
différent tant que ces compléments n'ont pas été relus et actualisés.

Les trois scripts `app-dist.sh`, `cli-dist.sh` et `pyhello-dist.sh` appellent
[`scripts/licenses-dist.sh`](../scripts/licenses-dist.sh) après assemblage.
Les outils doivent être installés avant de les lancer. La génération peut
consulter les sources amont pour récupérer un texte absent du paquet Cargo.
Chaque distribution contient :

- `LICENSE` et `LICENSE-EXCEPTION` pour StokesForge ;
- `THIRD-PARTY-LICENSES.txt`, textes des dépendances Rust transitives,
  notamment Typst (Apache-2.0), avec leurs noms et versions ;
- `licenses/fonts/<famille>/OFL.txt` et `licenses/phosphor/LICENSE` ;
- `licenses/epaint-default-fonts/`, copyrights et permissions des polices
  d'egui, dont MIT et Bitstream Vera pour Hack ;
- `licenses/winit/` et `licenses/smithay-clipboard/`, notices des patches ;
- `licenses/python/`, textes PSF et historiques de CPython, notices de ses
  bibliothèques natives, provenance et texte du runtime réellement livré ;
- `licenses/numpy/`, tous les textes livrés par la roue numpy, y compris
  ceux de ses composants natifs. Les `.dist-info/licenses` restent aussi
  dans le runtime ;
- `licenses/scipy/LICENSE.txt`, le texte de la roue scipy, qui reprend les
  licences de ses composants natifs (OpenBLAS, libgfortran, libquadmath…).

Les notices natives de Python viennent de l'archive complète correspondant au
verrou, sont versionnées et leur mise à jour est décrite dans
[`vendor/python/licenses-linux/README.md`](../vendor/python/licenses-linux/README.md)
(Windows, #332 : [`licenses-windows/`](../vendor/python/licenses-windows/README.md),
notices dérivées de celles de Linux tant que l'archive complète Windows n'est
pas recoupée, ce que son README dit).
Le packaging refuse un jeu de notices qui ne correspond plus au verrou, ou
un runtime sans les textes de CPython, numpy et scipy. Une livraison doit également
fournir les sources correspondantes selon les licences applicables ; le
fichier de notices n'est pas une archive de sources.

## Contributions

Voir [`CONTRIBUTING.md`](../CONTRIBUTING.md) : toute contribution extérieure
doit être proposée sous les conditions GPL avec cette exception et porter
le `Signed-off-by` du [DCO 1.1](../DCO). Les auteurs gardent leurs droits.
Le DCO ne donne pas, à lui seul, le droit de relicencier leurs contributions
pour une double licence commerciale. Une telle décision nécessiterait leur
accord ou un CLA séparé, explicite, à convenir avant intégration.
