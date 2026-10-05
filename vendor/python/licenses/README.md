# Notices du Python embarqué

Ces textes proviennent du répertoire `python/licenses/` de l'archive complète
officielle **CPython 3.12.14, python-build-standalone 20260901,
x86_64-unknown-linux-gnu, pgo+lto**. L'archive `install_only_stripped` utilisée
par StokesForge ne livre pas ce répertoire : elle conserve le texte de CPython
dans `lib/python3.12/LICENSE.txt`, mais pas toutes les notices de ses
bibliothèques natives. Les fichiers sont copiés intégralement, sans traduction.

[Release d'origine](https://github.com/astral-sh/python-build-standalone/releases/tag/20260901).
`provenance.json` conserve le nom et le SHA-256 de l'archive complète ainsi que
les indications de licence de son `PYTHON.json`. Il s'agit des indications
amont, y compris les notices de composants qui ne sont pas utilisés par tous
les modules. Par exemple, l'amont énumère OpenSSL 1.1 et 3 ; cela ne signifie
pas que les deux versions sont chargées ensemble. Certains chemins
`LICENSE.zlib-ng.txt` énumérés par l'amont sont absents de cette archive ; son
texte `LICENSE.zlib.txt` est conservé tel quel.

Les distributions livrent ces notices, le `LICENSE.txt` du CPython réellement
empaqueté et **tous les textes du répertoire de licences de la roue numpy**,
y compris ceux de ses composants natifs. La licence du code Python de
StokesForge ne remplace aucune de ces licences.

Lors d'une mise à jour de `vendor/python/lock.env` :

1. Récupérer l'archive complète correspondant à la nouvelle release et à la
   plateforme, puis vérifier les licences et les bibliothèques effectivement
   livrées.
2. Remplacer ces textes par ceux de `python/licenses/` et actualiser
   `provenance.json` et `versions.env`.
3. Rejouer les scripts de distribution ; `scripts/licenses-dist.sh` refuse
   des notices dont la version, la release ou la plateforme ne correspondent
   plus au verrou Python.
