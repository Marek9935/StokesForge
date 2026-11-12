//! Battery 24: every analysis operation on hostile series (#522).
//!
//! The batteries on the model feed the calculations tame columns. Here one
//! [`Operation`] per [`OperationKind`] meets series holding `NaN`, infinities,
//! `1e300`, subnormals, an X offset by `1e6`, duplicate or unsorted X, and
//! zero to two points. Two properties:
//!
//! 1. `run` never panics, whatever the settings and the data (invariant 10: a
//!    calculation fails with an error, never with a panic);
//! 2. on clean data, an operation that is ready (`readiness`) and valid
//!    (`validate`) is not refused by `run` as an invalid parameter: a setting
//!    judged acceptable up front is not judged unacceptable later.
//!
//! The draw is deterministic like `b14`; `SF_PROPTEST_SEED` and
//! `SF_PROPTEST_SCALE` replace the seed and multiply the cases.

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use proptest::prelude::*;
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{
    run, AutoDetect, AuxInput, Operation, OperationKind, Readiness, SeriesInput,
};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
use stokesforge_analysis::AnalysisError;

/// Cases per operation kind at scale 1.
const CASES_PER_KIND: u32 = 24;

// -- the data ---------------------------------------------------------------

/// A value that is hostile to arithmetic.
fn nasty() -> impl Strategy<Value = f64> {
    prop_oneof![
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(1e300),
        Just(-1e300),
        Just(f64::MIN_POSITIVE),
        Just(5e-324),
        Just(0.0),
        Just(-0.0),
        -1e3..1e3f64,
    ]
}

/// A value of a plausible curve, possibly spoiled.
fn value(spoil: bool) -> BoxedStrategy<f64> {
    if spoil {
        prop_oneof![3 => -50.0..50.0f64, 1 => nasty()].boxed()
    } else {
        (-50.0..50.0f64).boxed()
    }
}

/// How the X column is built.
#[derive(Debug, Clone, Copy)]
enum XShape {
    Sorted,
    Offset,
    Duplicates,
    Shuffled,
    Hostile,
}

fn x_column(n: usize, shape: XShape, rotation: usize) -> BoxedStrategy<Vec<f64>> {
    let base: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    match shape {
        XShape::Sorted => Just(base).boxed(),
        XShape::Offset => Just(base.iter().map(|v| v + 1e6).collect()).boxed(),
        XShape::Duplicates => Just(base.iter().map(|v| (v / 2.0).floor()).collect()).boxed(),
        XShape::Shuffled => {
            let mut v = base;
            if n > 0 {
                v.rotate_left(rotation % n);
                v.reverse();
            }
            Just(v).boxed()
        }
        XShape::Hostile => prop::collection::vec(nasty(), n).boxed(),
    }
}

fn x_shape() -> impl Strategy<Value = XShape> {
    prop_oneof![
        3 => Just(XShape::Sorted),
        1 => Just(XShape::Offset),
        1 => Just(XShape::Duplicates),
        1 => Just(XShape::Shuffled),
        1 => Just(XShape::Hostile),
    ]
}

fn length() -> impl Strategy<Value = usize> {
    prop_oneof![1 => 0..=2usize, 4 => 3..40usize]
}

fn curve(spoil: bool) -> impl Strategy<Value = (Vec<f64>, Vec<f64>)> {
    (length(), x_shape(), 0..40usize).prop_flat_map(move |(n, shape, rot)| {
        (
            x_column(n, shape, rot),
            prop::collection::vec(value(spoil), n),
        )
    })
}

fn series_input(spoil: bool) -> impl Strategy<Value = SeriesInput> {
    (
        curve(spoil),
        any::<bool>(),
        prop::collection::vec(nasty(), 0..40),
    )
        .prop_map(|((x, y), with_sigma, noise)| {
            let n = x.len();
            let sigma = with_sigma.then(|| {
                (0..n)
                    .map(|i| noise.get(i).copied().unwrap_or(1.0))
                    .collect()
            });
            SeriesInput { x, y, sigma }
        })
}

fn aux_input(spoil: bool) -> impl Strategy<Value = Option<AuxInput>> {
    prop_oneof![
        1 => Just(None),
        6 => curve(spoil).prop_map(|(x, y)| Some(AuxInput { x, y })),
    ]
}

// -- the operations ---------------------------------------------------------

/// A coordinate: mostly plausible, sometimes unplaced or hostile.
fn coord() -> impl Strategy<Value = f64> {
    prop_oneof![5 => -5.0..30.0f64, 1 => nasty()]
}

fn pair() -> impl Strategy<Value = (f64, f64)> {
    (coord(), coord())
}

fn baseline_op() -> impl Strategy<Value = BaselineOp> {
    prop_oneof![Just(BaselineOp::Subtract), Just(BaselineOp::Divide)]
}

fn param_setting() -> impl Strategy<Value = ParamSetting> {
    (
        prop::option::of(nasty()),
        any::<bool>(),
        prop::option::of(nasty()),
        prop::option::of(nasty()),
        any::<bool>(),
    )
        .prop_map(|(initial, fixed, min, max, shared)| ParamSetting {
            initial,
            fixed,
            min,
            max,
            shared,
        })
}

fn model() -> impl Strategy<Value = Model> {
    prop_oneof![
        Just(Model::Linear),
        (0..14u32).prop_map(|degree| Model::Polynomial { degree }),
        Just(Model::Exponential),
        Just(Model::ExponentialOffset),
        Just(Model::Exponential2),
        Just(Model::Logarithm),
        Just(Model::Power),
        Just(Model::Gaussian),
        Just(Model::Lorentzian),
        prop_oneof![
            Just("a + b*x".to_string()),
            Just("a*exp(-x/b)".to_string()),
            Just("a/(x-b)".to_string()),
            Just("ln(a*x) + b".to_string()),
            Just("a*x^b".to_string()),
            Just("1e400*x + a".to_string()),
            Just("a*(((x)))".to_string()),
            Just("a +".to_string()),
            Just("".to_string()),
        ]
        .prop_map(|formula| Model::Custom { formula }),
    ]
}

fn opt_range() -> impl Strategy<Value = Option<(f64, f64)>> {
    prop_oneof![2 => Just(None), 3 => pair().prop_map(Some)]
}

fn seed() -> impl Strategy<Value = PeakSeed> {
    let sane = (
        0.0..20.0f64,
        prop::option::of(0.1..10.0f64),
        prop::option::of(0.2..5.0f64),
    );
    let wild = (
        coord(),
        prop::option::of(nasty()),
        prop::option::of(nasty()),
    );
    prop_oneof![3 => sane, 1 => wild].prop_map(|(center, height, fwhm)| PeakSeed {
        center,
        height,
        fwhm,
    })
}

/// One operation of `kind`. The `match` has no wildcard: a new kind must be
/// given its strategy here before this battery compiles.
fn operation(kind: OperationKind) -> BoxedStrategy<Operation> {
    match kind {
        OperationKind::Baseline => {
            let method = prop_oneof![
                pair().prop_map(|point| BaselineMethod::Constant { point }),
                pair().prop_map(|zone| BaselineMethod::ConstantZone { zone }),
                (pair(), pair()).prop_map(|(p1, p2)| BaselineMethod::Line { p1, p2 }),
                (prop::collection::vec(pair(), 0..4), 0..14u32)
                    .prop_map(|(zones, degree)| BaselineMethod::Zones { zones, degree }),
                (
                    prop::collection::vec(pair(), 0..6),
                    prop_oneof![Just(Interpolation::Segments), Just(Interpolation::Spline)]
                )
                    .prop_map(|(points, interpolation)| BaselineMethod::Anchors {
                        points,
                        interpolation
                    }),
            ];
            (method, baseline_op(), any::<bool>())
                .prop_map(|(method, op, keep_baseline)| Operation::Baseline {
                    method,
                    op,
                    keep_baseline,
                })
                .boxed()
        }
        OperationKind::Correct => prop_oneof![
            Just(CorrectOp::Divide),
            Just(CorrectOp::Multiply),
            Just(CorrectOp::Subtract)
        ]
        .prop_map(|op| Operation::Correct { op })
        .boxed(),
        OperationKind::Integrate => (
            prop::collection::vec(pair(), 0..3),
            prop_oneof![Just(Under::Zero), Just(Under::Line)],
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(
                |(ranges, under, cumulative, normalize)| Operation::Integrate {
                    spec: IntegrateSpec {
                        ranges,
                        under,
                        cumulative,
                        normalize,
                    },
                },
            )
            .boxed(),
        OperationKind::Fit => (
            model(),
            prop::collection::vec(param_setting(), 0..4),
            opt_range(),
            any::<bool>(),
        )
            .prop_map(|(model, params, range, residuals)| Operation::Fit {
                spec: FitSpec { model, params },
                range,
                residuals,
            })
            .boxed(),
        OperationKind::Peaks => (
            prop_oneof![
                Just(PeakShape::Gaussian),
                Just(PeakShape::Lorentzian),
                Just(PeakShape::Voigt)
            ],
            prop_oneof![
                Just(PeakBackground::None),
                Just(PeakBackground::Constant),
                Just(PeakBackground::Linear)
            ],
            prop::collection::vec(seed(), 0..4),
            any::<bool>(),
            prop::option::of(nasty()),
            prop::option::of(
                (nasty(), 0..150usize).prop_map(|(min_prominence, max_peaks)| AutoDetect {
                    min_prominence,
                    max_peaks,
                }),
            ),
            opt_range(),
            any::<bool>(),
        )
            .prop_map(
                |(shape, background, seeds, link_widths, center_window, auto, range, residuals)| {
                    Operation::Peaks {
                        spec: PeaksSpec {
                            shape,
                            background,
                            seeds,
                            link_widths,
                            center_window,
                        },
                        auto,
                        range,
                        residuals,
                    }
                },
            )
            .boxed(),
        OperationKind::Reconvolution => (
            0..6usize,
            prop::collection::vec(param_setting(), 0..10),
            nasty(),
            any::<bool>(),
            prop_oneof![1 => Just(None), 2 => pair().prop_map(Some)],
            any::<bool>(),
        )
            .prop_map(
                |(components, params, shift_initial, fit_shift, window, residuals)| {
                    Operation::Reconvolution {
                        spec: ReconvSpec {
                            components,
                            params,
                            shift_initial,
                            fit_shift,
                            window,
                        },
                        residuals,
                    }
                },
            )
            .boxed(),
        OperationKind::DirectDeconvolution => (
            prop_oneof![
                prop_oneof![5 => 0..200usize, 1 => 5000..6000usize]
                    .prop_map(|iterations| DirectMethod::RichardsonLucy { iterations }),
                prop::option::of(nasty()).prop_map(|noise| DirectMethod::Wiener { noise }),
            ],
            nasty(),
        )
            .prop_map(|(method, shift)| Operation::DirectDeconvolution { method, shift })
            .boxed(),
        OperationKind::Smooth => (
            prop_oneof![
                Just(SmoothMethod::AdjacentAverage),
                (0..8usize).prop_map(|order| SmoothMethod::SavitzkyGolay { order }),
                Just(SmoothMethod::Median),
            ],
            prop_oneof![
                (0..1100usize).prop_map(SmoothWindow::Points),
                nasty().prop_map(SmoothWindow::XUnits),
            ],
        )
            .prop_map(|(method, window)| Operation::Smooth {
                spec: SmoothSpec { method, window },
            })
            .boxed(),
    }
}

// -- the properties ---------------------------------------------------------

#[derive(Default, Debug, Clone, Copy)]
struct Tally {
    drawn: u32,
    ok: u32,
    ready_and_valid_on_clean: u32,
}

fn describe(op: &Operation, inputs: &[SeriesInput], aux: &Option<AuxInput>) -> String {
    format!("{op:?} on {inputs:?} aux {aux:?}; {}", common::seed_note())
}

/// One kind: property 1 on hostile data, property 2 on clean data.
fn sweep_kind(kind: OperationKind) -> Tally {
    let tally = std::cell::RefCell::new(Tally::default());
    let hostile = (
        operation(kind),
        prop::collection::vec(series_input(true), 1..3),
        aux_input(true),
    );
    common::env_runner(CASES_PER_KIND)
        .run(&hostile, |(op, inputs, aux)| {
            let hostile_run = catch_unwind(AssertUnwindSafe(|| run(&op, &inputs, aux.as_ref())));
            prop_assert!(
                hostile_run.is_ok(),
                "run panicked on {}",
                describe(&op, &inputs, &aux)
            );
            let mut t = tally.borrow_mut();
            t.drawn += 1;
            if matches!(hostile_run, Ok(Ok(_))) {
                t.ok += 1;
            }
            // Property 2 on the same operation over a clean curve.
            if matches!(op.readiness(), Readiness::Ready) && op.validate().is_ok() {
                let x: Vec<f64> = (0..40).map(|i| i as f64 * 0.5).collect();
                let y: Vec<f64> = x
                    .iter()
                    .map(|v| (-(v - 10.0).powi(2) / 8.0).exp() * 5.0 + 0.1 * v)
                    .collect();
                let clean = [SeriesInput {
                    x: x.clone(),
                    y: y.clone(),
                    sigma: None,
                }];
                let aux = Some(AuxInput {
                    x,
                    y: y.iter().map(|v| v + 1.0).collect(),
                });
                let result = catch_unwind(AssertUnwindSafe(|| run(&op, &clean, aux.as_ref())));
                prop_assert!(result.is_ok(), "run panicked on a clean curve: {op:?}");
                t.ready_and_valid_on_clean += 1;
                if matches!(result, Ok(Ok(_))) {
                    t.ok += 1;
                }
                if let Ok(Err(AnalysisError::InvalidParameter { name })) = result {
                    // Auto detection that finds nothing is a property of the
                    // data, reported under the name "peaks".
                    prop_assert!(
                        name == "peaks",
                        "ready and valid, yet refused as invalid parameter {name:?}: {op:?}; {}",
                        common::seed_note()
                    );
                }
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{kind:?}: {e}; {}", common::seed_note()));
    tally.into_inner()
}

/// One test per kind (they run in parallel), with the draw reaching a success
/// at least once: without that, a strategy that only draws invalid settings
/// would pass the properties trivially.
fn check_kind(kind: OperationKind) {
    let started = std::time::Instant::now();
    let tally = sweep_kind(kind);
    assert!(
        tally.drawn >= CASES_PER_KIND,
        "{kind:?}: only {} cases drawn",
        tally.drawn
    );
    assert!(tally.ok >= 1, "{kind:?}: no draw ever succeeded: {tally:?}");
    eprintln!("b24 {kind:?}: {tally:?} in {:?}", started.elapsed());
}

/// Names the kinds the tests below cover: adding a kind breaks this `match`
/// until it has its own test.
#[test]
fn b24_every_kind_has_its_test() {
    for kind in OperationKind::ALL {
        match kind {
            OperationKind::Baseline
            | OperationKind::Correct
            | OperationKind::Integrate
            | OperationKind::Fit
            | OperationKind::Peaks
            | OperationKind::Reconvolution
            | OperationKind::DirectDeconvolution
            | OperationKind::Smooth => {}
        }
    }
    assert_eq!(OperationKind::ALL.len(), 8);
}

macro_rules! kind_test {
    ($name:ident, $kind:ident) => {
        #[test]
        fn $name() {
            check_kind(OperationKind::$kind);
        }
    };
}

kind_test!(b24_baseline_survives_hostile_series, Baseline);
kind_test!(b24_correct_survives_hostile_series, Correct);
kind_test!(b24_integrate_survives_hostile_series, Integrate);
kind_test!(b24_fit_survives_hostile_series, Fit);
kind_test!(b24_peaks_survive_hostile_series, Peaks);
kind_test!(b24_reconvolution_survives_hostile_series, Reconvolution);
kind_test!(
    b24_direct_deconvolution_survives_hostile_series,
    DirectDeconvolution
);
kind_test!(b24_smooth_survives_hostile_series, Smooth);
