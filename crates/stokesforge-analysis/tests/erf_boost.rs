//! `erf` and `erfc` against the Boost.Math reference tables (BSL-1.0, see
//! `tests/data/boost-erf/README.md`): every row, subnormals included, within
//! `MAX_ULPS` of the correctly rounded reference value.

use stokesforge_analysis::functions::{erf, erfc};

/// Ordered-bits distance: 0 for equal values, 1 for neighbours, across zero
/// too (`+0` and `-0` are the same point).
fn ulp_distance(a: f64, b: f64) -> u64 {
    fn key(x: f64) -> i64 {
        let bits = x.to_bits() as i64;
        if bits < 0 {
            i64::MIN - bits
        } else {
            bits
        }
    }
    key(a).abs_diff(key(b))
}

/// One row `{{ SC_(x), SC_(erf), SC_(erfc) }}` per line; other lines are ignored.
fn rows(text: &str) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.contains("{{ SC_(") {
            continue;
        }
        let nums: Vec<f64> = line
            .split("SC_(")
            .skip(1)
            .map(|part| {
                let lit = part.split(')').next().expect("closing parenthesis");
                lit.trim()
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("number {lit:?}"))
            })
            .collect();
        assert_eq!(nums.len(), 3, "row {line:?}");
        out.push([nums[0], nums[1], nums[2]]);
    }
    out
}

/// Largest distance accepted: `libm::erfc` is within 2 ulps of the reference
/// (fdlibm); observed worst: `erf` 1, `erfc` 2. The observed worst is reported by the test.
const MAX_ULPS: u64 = 2;

fn check(name: &str, text: &str, expected_rows: usize) {
    let rows = rows(text);
    assert_eq!(rows.len(), expected_rows, "{name}: rows read");
    let mut worst = (0u64, 0.0, "");
    let mut failures = Vec::new();
    for [x, e, c] in rows {
        for (what, got, want) in [("erf", erf(x), e), ("erfc", erfc(x), c)] {
            let d = ulp_distance(got, want);
            if d > worst.0 {
                worst = (d, x, what);
            }
            if d > MAX_ULPS {
                failures.push(format!(
                    "{what}({x:e}) = {got:e}, expected {want:e}, {d} ulps"
                ));
            }
        }
    }
    eprintln!(
        "{name}: worst distance {} ulps ({} at {:e})",
        worst.0, worst.2, worst.1
    );
    assert!(
        failures.is_empty(),
        "{name}: {} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn erf_and_erfc_match_boost_erf_data() {
    check("erf_data", include_str!("data/boost-erf/erf_data.ipp"), 500);
}

#[test]
fn erf_and_erfc_match_boost_erf_small_data() {
    check(
        "erf_small_data",
        include_str!("data/boost-erf/erf_small_data.ipp"),
        150,
    );
}

#[test]
fn erf_and_erfc_match_boost_erf_large_data() {
    check(
        "erf_large_data",
        include_str!("data/boost-erf/erf_large_data.ipp"),
        300,
    );
}
