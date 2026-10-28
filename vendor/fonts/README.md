# StokesForge embedded fonts

Three "sans-serif text + Math" font pairs, under the **SIL Open Font License
1.1** (checked on each `OFL.txt` file below, not assumed). Embedded in the
binary by `crates/stokesforge-render/src/fonts.rs` through `include_bytes!`,
so that StokesForge can always draw readable text without depending on what
is installed on the machine; see
`docs/archive/newarchitecture/architecture-choisie.md` §7 for the decision and
its date (2026-09-13).

Noto Sans / Noto Sans Math has a double role in this mechanism: it is the
default font (nothing requested => Noto) and the silent fallback (requested
system font not found => Noto). The other two pairs are used only on explicit
request.

## Why these files are committed as they are (not a `lock.env` + fetch)

`vendor/python/` (see `vendor/python/lock.env` and `.gitignore`) locks a
version and downloads ~300 MB at build time, because the distribution is
large, tied to a specific platform and ABI (CPython + numpy + PyO3), and
would often change its real content without changing its declared version.
None of this applies here: six files, 4.5 MB in total, static,
platform-independent, which will no longer change once chosen (a version
update would be a conscious choice, not a build default). A fetch mechanism
would have added a network dependency at build time for almost no gain, and
would have broken the offline robustness that `stokesforge-robustesse` (and
CI) take for granted. Hence the choice: commit the files directly.

## Provenance

| Directory | File | Family (declared in the file) | Source | Version / commit | Retrieved on |
|---|---|---|---|---|---|
| `libertinus-sans/` | `LibertinusSans-Regular.otf` | Libertinus Sans | [alerque/libertinus](https://github.com/alerque/libertinus), release `v7.051` | 7.051 (2024-09-26) | 2026-09-13 |
| `libertinus-math/` | `LibertinusMath-Regular.otf` | Libertinus Math | [alerque/libertinus](https://github.com/alerque/libertinus), release `v7.051` (same archive) | 7.051 (2024-09-26) | 2026-09-13 |
| `fira-sans/` | `FiraSans-Regular.ttf` | Fira Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/firasans) (canonical Google Fonts distribution; origin Mozilla/Telefónica) | commit `ed7143b8f0c9` (2016-12-02) | 2026-09-13 |
| `fira-math/` | `FiraMath-Regular.otf` | Fira Math | [firamath/firamath](https://github.com/firamath/firamath), release `v0.3.4` | 0.3.4 (2020-10-15, confirmed current on [CTAN](https://ctan.org/pkg/firamath)) | 2026-09-13 |
| `noto-sans/` | `NotoSans-Variable.ttf` (renamed from `NotoSans[wdth,wght].ttf`) | Noto Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosans) | commit `2984c575fdce` (2024-11-20) | 2026-09-13 |
| `noto-sans/` | `NotoSans-Italic-Variable.ttf` (renamed from `NotoSans-Italic[wdth,wght].ttf`) | Noto Sans (italic) | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosans) | commit `2984c575fdce` (2024-11-20) | 2026-10-02 |
| `noto-sans-math/` | `NotoSansMath-Regular.ttf` | Noto Sans Math | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosansmath) | commit `dbd1ab6e65dc` (2024-06-27) | 2026-09-13 |

Each directory carries its own `OFL.txt` (full license text), copied from the
same source as the font file; this is the standard font distribution
practice, with the same care as the vendored licenses of `vendor/python/`.
The distribution scripts copy each text into
`licenses/fonts/<family>/OFL.txt` through `scripts/licenses-dist.sh` (#129).

## Notes

- **Libertinus**: static regular chosen (not the variable one);
  `alerque/libertinus` publishes both formats in the same release, and the
  static one is enough since StokesForge only needs the regular weight.
- **Fira Sans**: static regular, taken from `google/fonts` rather than from
  the `googlefonts/FiraGFVersion` repository (archived, no release);
  `google/fonts` is the distribution actually published and serves the
  license next to the font.
- **Fira Math**: latest stable version published (`v0.3.4`; there are more
  recent `v0.4-beta-*` pre-releases, but explicitly beta, not retained).
- **Noto Sans**: variable font (axes `wdth`, `wght`) rather than a static
  cut, chosen to stay on the canonical `google/fonts` distribution, which
  serves `OFL.txt` in the same place; the file's default instance matches the
  regular weight. Renamed `NotoSans-Variable.ttf` to avoid the `[...]`
  brackets of the original file name in the repository.
- **Noto Sans italic** (issue #63): Typst does not synthesize an italic;
  without this face, the "italic" button of the text format bar had no
  effect. Bold, for its part, comes from the `wght` axis of the variable
  font. Fira and Libertinus have only the regular: bold and italic change
  nothing there.
- **Noto Sans Math**: already distributed as a static regular by
  `google/fonts`.

## Updating a font

1. Check the OFL license at the new source (do not assume it has not
   changed).
2. Replace the font file and `OFL.txt` in the relevant directory.
3. Update the matching row of this table (source, version or commit,
   retrieval date).
4. If the family name inside the file changed, update
   `FontSet::sans_family_name`/`math_family_name` in
   `crates/stokesforge-render/src/fonts.rs`; a test
   (`family_names_match_the_file_content`) fails otherwise.
