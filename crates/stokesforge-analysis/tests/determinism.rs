//! Determinism of analysis results across platforms (#332).
//!
//! The transcendental functions of `f64` delegate to the platform libm
//! (MSVC CRT on Windows, glibc on Linux), which differ in the last ulp; an
//! iterative peak decomposition converged on Linux and not on Windows (#290).
//! `stokesforge-analysis` therefore goes through the `libm` crate (pure Rust),
//! guarded by `clippy.toml`. The bits frozen below hold on every platform;
//! the Linux CI confirms them.

use stokesforge_analysis::error::AnalysisError;
use stokesforge_analysis::expr::Expr;
use stokesforge_analysis::ops::{run, AutoDetect, Operation, SeriesInput};
use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};

/// The sawtooth series of #290: 40 points, x = 0..39, y = 0 on even rows
/// and 100 on odd rows.
fn sawtooth() -> SeriesInput {
    SeriesInput {
        x: (0..40).map(f64::from).collect(),
        y: (0..40)
            .map(|i| if i % 2 == 0 { 0.0 } else { 100.0 })
            .collect(),
        sigma: None,
    }
}

/// Steps of five rows at 0 then at 100 (low R²): the peak decomposition
/// converges. This is the series of the state test #290 since #332.
fn steps() -> SeriesInput {
    SeriesInput {
        x: (0..40).map(f64::from).collect(),
        y: (0..40)
            .map(|i| if (i / 5) % 2 == 0 { 0.0 } else { 100.0 })
            .collect(),
        sigma: None,
    }
}

fn peak_decomposition() -> Operation {
    Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: vec![],
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: 0.05,
            max_peaks: 6,
        }),
        range: None,
        residuals: false,
    }
}

fn evaluate(src: &str, x: f64, p: &[f64]) -> f64 {
    Expr::parse(src).unwrap().eval(x, p)
}

/// Record of the peak decomposition of the steps: (key, index, value,
/// uncertainty), as `f64` bits.
#[allow(clippy::type_complexity)]
const EXPECTED_STEPS: [(&str, Option<usize>, u64, Option<u64>); 20] = [
    (
        "peak.height",
        Some(1),
        0x4058185c389adc92,
        Some(0x40407727febcd8e7),
    ),
    (
        "peak.center",
        Some(1),
        0x401bffcd205d120c,
        Some(0x3fe2a91943f607ba),
    ),
    (
        "peak.fwhm",
        Some(1),
        0x400ca14f02ff0aae,
        Some(0x3ff7c142c85059e6),
    ),
    ("peak.area", Some(1), 0x4076f28e3099584d, None),
    ("peak.area_percent", Some(1), 0x4040aad12a261149, None),
    (
        "peak.height",
        Some(2),
        0x405818cb35260c01,
        Some(0x40407861300c5dad),
    ),
    (
        "peak.center",
        Some(2),
        0x4030ffffffcd5eae,
        Some(0x3fe2a8564a19a683),
    ),
    (
        "peak.fwhm",
        Some(2),
        0x400c9ffda52a5095,
        Some(0x3ff7bc86237f4b12),
    ),
    ("peak.area", Some(2), 0x4076f1e97765d1c1, None),
    ("peak.area_percent", Some(2), 0x4040aa5985be35d0, None),
    (
        "peak.height",
        Some(3),
        0x4058185727e52334,
        Some(0x40407723a2a40834),
    ),
    (
        "peak.center",
        Some(3),
        0x403b000d077e5405,
        Some(0x3fe2a92179120ba4),
    ),
    (
        "peak.fwhm",
        Some(3),
        0x400ca15c27f2ade2,
        Some(0x3ff7c1512d870c87),
    ),
    ("peak.area", Some(3), 0x4076f293e6b4949c, None),
    ("peak.area_percent", Some(3), 0x4040aad5501bb8e8, None),
    (
        "background.y0",
        None,
        0x403676c8525be4f5,
        Some(0x402596c418c7a5ee),
    ),
    ("stat.points", None, 0x4044000000000000, None),
    ("stat.r_squared", None, 0x3fdca20dc90a7734, None),
    ("stat.reduced_chi2", None, 0x409cc81ec80f2653, None),
    ("stat.dof", None, 0x403e000000000000, None),
];

#[test]
fn the_peak_decomposition_of_the_steps_gives_the_same_bits_everywhere() {
    let outputs = run(&peak_decomposition(), &[steps()], None).unwrap();
    let obtained: Vec<(&str, Option<usize>, u64, Option<u64>)> = outputs[0]
        .values
        .iter()
        .map(|v| {
            (
                v.key.as_str(),
                v.index,
                v.value.to_bits(),
                v.error.map(f64::to_bits),
            )
        })
        .collect();
    assert_eq!(obtained, EXPECTED_STEPS);
}

/// The sawtooth of #290 does not converge in 400 iterations with `libm`, and
/// this holds on every platform: the state test #290 therefore uses the
/// steps, which converge.
#[test]
fn the_sawtooth_converges_on_no_platform() {
    let error = run(&peak_decomposition(), &[sawtooth()], None).unwrap_err();
    assert!(
        matches!(error, AnalysisError::NotConverged { iterations: 400 }),
        "{error:?}"
    );
}

#[test]
fn the_expression_functions_give_the_same_bits_everywhere() {
    let expected_values: [(&str, f64, &[f64], u64); 5] = [
        ("exp(x)", 0.5, &[], 0x3ffa_6129_8e1e_069c),
        ("ln(x)", 3.0, &[], 0x3ff1_93ea_7aad_030a),
        ("sin(x)", 1e6, &[], 0xbfd6_664b_2568_d867),
        ("pow(x, a)", 2.5, &[1.7], 0x4012_fdcf_53f3_e6f3),
        ("atan2(x, a)", 1.0, &[-3.0], 0x4006_8f09_5fdf_593c),
    ];
    for (source, x, p, bits) in expected_values {
        let obtained = evaluate(source, x, p);
        assert_eq!(obtained.to_bits(), bits, "{source} at {x}: {obtained:?}");
    }
}

/// FNV-1a fingerprint of the bits of a function swept over 256 abscissas: a
/// single point near a rounding edge is enough to move it, which five
/// isolated values do not guarantee.
fn fingerprint(source: &str, p: &[f64], abscissa: impl Fn(usize) -> f64) -> u64 {
    (0..256).fold(0xcbf2_9ce4_8422_2325_u64, |h, i| {
        let bits = evaluate(source, abscissa(i), p).to_bits();
        (h ^ bits).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn the_sweep_of_the_expression_functions_gives_the_same_bits_everywhere() {
    let step = |i: usize| (i as f64 - 128.0) * 0.173 + 0.011;
    let positive = |i: usize| 0.0137 * (i as f64 + 1.0);
    let obtained = [
        ("exp(x)", fingerprint("exp(x)", &[], step)),
        ("ln(x)", fingerprint("ln(x)", &[], positive)),
        ("sin(x)", fingerprint("sin(x)", &[], |i| step(i) * 997.0)),
        ("pow(x, a)", fingerprint("pow(x, a)", &[1.7], positive)),
        ("atan2(x, a)", fingerprint("atan2(x, a)", &[-3.0], step)),
        ("tanh(x)", fingerprint("tanh(x)", &[], step)),
    ];
    let expected = [
        ("exp(x)", 0xb645_35ea_6664_ce0b),
        ("ln(x)", 0x065c_a518_45f6_7c01),
        ("sin(x)", 0x8924_bcd5_724e_304d),
        ("pow(x, a)", 0x6c99_0481_a84f_e677),
        ("atan2(x, a)", 0xab38_4b41_92ae_7740),
        ("tanh(x)", 0x1b6c_37db_5200_3baa),
    ];
    assert_eq!(obtained, expected);
}
