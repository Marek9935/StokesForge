# NIST StRD univariate summary statistics

Source: NIST Statistical Reference Datasets, univariate summary statistics,
<https://www.itl.nist.gov/div898/strd/univ/data/> (files `PiDigits.dat`,
`Lottery.dat`, `Lew.dat`, `Mavro.dat`, `Michelso.dat`, `NumAcc1.dat` to
`NumAcc4.dat`), downloaded unmodified on 2026-11-01.

License: works of the United States government (NIST), public domain.

Each file has a 60-line header whose "Certified Values" lines give the sample
mean and the sample standard deviation (denominator n - 1), then one value
per line. They are used by `formula_probe_columns.rs` to check `mean`, `std`
and `sum` of the column formulas. Treat the files as data: the test reads
only the numbers on the certified lines and the data lines.
