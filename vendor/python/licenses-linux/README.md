# Notices for the embedded Python

These texts come from the `python/licenses/` directory of the official full
archive **CPython 3.12.14, python-build-standalone 20260901,
x86_64-unknown-linux-gnu, pgo+lto**. The `install_only_stripped` archive used
by StokesForge does not ship this directory: it keeps CPython's text in
`lib/python3.12/LICENSE.txt`, but not all the notices of its native
libraries. The files are copied in full, untranslated.

[Original release](https://github.com/astral-sh/python-build-standalone/releases/tag/20260901).
`provenance.json` keeps the name and SHA-256 of the full archive as well as
the license indications of its `PYTHON.json`. These are upstream indications,
including the notices of components that are not used by every module. For
example, upstream lists OpenSSL 1.1 and 3; this does not mean that both
versions are loaded together. Some `LICENSE.zlib-ng.txt` paths listed by
upstream are missing from this archive; its `LICENSE.zlib.txt` text is kept
as is.

The distributions ship these notices, the `LICENSE.txt` of the CPython that
is actually packaged and **all the texts of the numpy wheel's license
directory**, including those of its native components, and the `LICENSE.txt`
of the scipy wheel (#191), which carries the licenses of its native
libraries. The license of StokesForge's Python code replaces none of these
licenses.

When updating `vendor/python/lock.env`:

1. Fetch the full archive matching the new release and the platform, then
   check the licenses and the libraries actually shipped.
2. Replace these texts with those of `python/licenses/` and refresh
   `provenance.json` and `versions.env`.
3. Replay the distribution scripts; `scripts/licenses-dist.sh` refuses
   notices whose version, release or platform no longer match the Python
   lock.
