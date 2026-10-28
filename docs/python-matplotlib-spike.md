# Matplotlib evaluation — spike #69

The prototype works on Linux x86_64: Matplotlib imports in an isolated CPython,
reads the `pyst` columns through numpy and produces SVG and PNG off screen.
The SVG also goes through the PNG/PDF converters already used by the
StokesForge exports. This result is an exported file, not an editable `PlotSpec`.

## Reproduction

```sh
scripts/python-vendor.sh
CARGO_BUILD_JOBS=2 scripts/python-matplotlib-spike.sh
```

The script copies the normal runtime into
`vendor/python/extracted/matplotlib-spike/python/` (ignored by Git), then
installs only the binary wheels pinned in
`vendor/python/matplotlib-requirements.txt`. numpy comes from the normal lock.
The copy is renewed when `vendor/python/lock.env` changes; `pip check`
checks consistency. The usual runtime and the distribution packages
remain the base ones, without Matplotlib.

The executable test is `stokesforge-script/examples/matplotlib-spike.rs`,
launched explicitly by the script. It checks:

- version 3.11.2, import from the copy and `sys.flags.isolated == 1`;
- creation of columns, numpy computation and plotting from their read-only views;
- `FigureCanvasSVG` then `FigureCanvasAgg`, without a window or `pyplot`;
- two identical SVG exports (`svg.hashsalt` fixed and metadata date removed);
- direct/Agg PNG signatures and PNG/PDF converted by the Rust exports;
- data preserved and undo of the creation group.

The exported files stay on disk after undo: the history undoes the model, not
the writing of arbitrary files by a script. The check artifacts are
`/tmp/stokesforge69-spike.svg`, `/tmp/stokesforge69-spike.png` and
`/tmp/stokesforge69-measurements.json`. No generated artifact is versioned.

## Measurements of 2 October 2026

CPython 3.12.14 and numpy 2.5.3 of the lock, Matplotlib 3.11.2 and its nine
pinned dependencies, Linux x86_64, shared development machine.
Sizes are apparent (`du -sb`), full prefix including pip; they
do not represent a compressed archive nor the size of an installer.

| Measurement | Result |
|---|---:|
| Base runtime | 174,605,105 bytes (166.5 MiB) |
| Copy with Matplotlib | 269,414,669 bytes (256.9 MiB) |
| Overhead | 94,809,564 bytes (90.4 MiB) |
| Initialization of the embedded engine, without Matplotlib | 65.6 ms |
| Import numpy + Matplotlib + Figure/SVG/Agg in the engine, fresh font cache | 1,236 ms |
| Import Matplotlib + SVG backend in a fresh process, first sample | 769 ms |
| Same import, median of the next five processes | 151 ms |

The last series starts by importing numpy separately (49–53 ms), then
times Matplotlib and the SVG backend. Each process is fresh; only
`MPLCONFIGDIR` is shared between the six launches. The first builds the
font cache; the next five reuse it. The file system caches are not
purged. The embedded import also loads Figure and Agg:
the two measurements are therefore not directly comparable. These values
place the cost on this machine, without promising a universal time.

## What the spike lets us decide

Matplotlib suits a free export from a script: the scientific user
can reuse their plotting code and the standard backends. The
size cost is noticeable, and the import must stay lazy. The prototype
adds neither a Matplotlib window nor a new native `pyst` surface.

Graphs created by `pyst.plot` keep their series and column references
in a `PlotSpec`, editable in the StokesForge properties. A Matplotlib SVG
is a frozen result; it does not offer this editing history. The two paths
therefore have distinct uses. The title of the prototype uses Matplotlib's
mathtext; the Typst rendering of native graphs remains that of StokesForge.

The trial demonstrates the "export" path requested in #69. Display in a
note, management of persistent figures or a dedicated window are not
implemented by this prototype. The possible integration into the distributed
runtime remains a product choice; the measurements and the test make it
possible to take it.

Official documentation used:
[dependencies](https://matplotlib.org/stable/install/dependencies.html),
[SVG backend](https://matplotlib.org/stable/api/backend_svg_api.html),
[Figure.savefig](https://matplotlib.org/stable/api/_as_gen/matplotlib.figure.Figure.savefig.html).
