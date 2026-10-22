# Notices du Python embarqué, Windows

Pendant Windows de [`../licenses-linux/`](../licenses-linux/README.md) (#332) :
**CPython 3.12.14, python-build-standalone 20260901,
x86_64-pc-windows-msvc**, archive `install_only_stripped`. Cette archive ne
livre pas de répertoire de licences ; le texte de CPython est dans son
`LICENSE.txt` (copié dans le paquet), les autres notices sont ici.

**Statut à lever avant de publier une V1 :** ces textes sont ceux de la
distribution Linux de la même release (mêmes composants amont, mêmes versions :
bzip2, expat, libffi, liblzma, mpdecimal, OpenSSL 3, SQLite, zlib). Ils n'ont
**pas** été recoupés avec le répertoire `python/licenses/` ni le `PYTHON.json`
de l'archive complète Windows (`pgo-full`) : `provenance.json` porte
`verified_against_windows_full_archive: false`. Vérifier, puis passer ce champ
à `true`.

Retirés du paquet Windows (donc sans notice) : tkinter, Tcl/Tk, `_msi`, les
modules de test. `LICENSE.msvc-runtime.txt` couvre les DLL Visual C++ livrées.
numpy et scipy livrent leurs propres textes (OpenBLAS, `msvcp140` de numpy),
copiés depuis leurs roues par `scripts/licenses-dist.sh`.

Mise à jour du verrou `vendor/python/lock.env` : même marche que pour Linux
(`../licenses-linux/README.md`), avec l'archive Windows.
