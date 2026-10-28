# Notices for the embedded Python, Windows

Windows counterpart of [`../licenses-linux/`](../licenses-linux/README.md)
(#332): **CPython 3.12.14, python-build-standalone 20260901,
x86_64-pc-windows-msvc**, `install_only_stripped` archive. This archive does
not ship a licenses directory; CPython's text is in its `LICENSE.txt` (copied
into the package), the other notices are here.

**Status to lift before publishing a V1:** these texts are those of the Linux
distribution of the same release (same upstream components, same versions:
bzip2, expat, libffi, liblzma, mpdecimal, OpenSSL 3, SQLite, zlib). They were
**not** cross-checked against the `python/licenses/` directory or the
`PYTHON.json` of the full Windows archive (`pgo-full`): `provenance.json`
carries `verified_against_windows_full_archive: false`. Check, then set this
field to `true`.

Removed from the Windows package (hence without notice): tkinter, Tcl/Tk,
`_msi`, the test modules. `LICENSE.msvc-runtime.txt` covers the Visual C++
DLLs shipped. numpy and scipy ship their own texts (OpenBLAS, numpy's
`msvcp140`), copied from their wheels by `scripts/licenses-dist.sh`.

Updating the `vendor/python/lock.env` lock: same procedure as for Linux
(`../licenses-linux/README.md`), with the Windows archive.
