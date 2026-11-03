# Boost.Math test tables for `erf` and `erfc`

Source: Boost.Math, <https://github.com/boostorg/math>, tag `boost-1.87.0`
(commit `f1b0b95a04f56717b3454cb599e0647770c5852e`), files `test/erf_data.ipp`,
`test/erf_small_data.ipp` and `test/erf_large_data.ipp`, downloaded unmodified
on 2026-11-02. `LICENSE_1_0.txt` is the `LICENSE` file of the same tag.

License: Boost Software License 1.0 (permissive, GPL-compatible); the copyright
header of each table (John Maddock) stays in the file.

Each row is `{{ SC_(x), SC_(erf(x)), SC_(erfc(x)) }}`, computed by Boost to
about 40 significant digits. `tests/erf_boost.rs` parses the rows as text and
requires `erf` and `erfc` of `stokesforge-analysis` to be within a few ulps of
the correctly rounded reference. Treat the files as data.
