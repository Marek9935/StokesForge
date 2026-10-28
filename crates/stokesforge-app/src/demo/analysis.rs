//! `Analysis` folder of the demo: **one workbook per computation family** of
//! the Analysis menu, each with data whose answer is known.
//!
//! Each [`Dataset`] carries its data, the operation that suits them and what
//! it must recover ([`Expected`], with a tolerance): the test
//! `every_analysis_dataset_recovers_its_parameters` replays `core::compute` on
//! each, so a demo whose data no longer say what they claim fails instead of
//! misleading the user. The data live in the code (no file) and their noise
//! has a fixed seed ([`Noise`]).
//!
//! A few datasets ([`Dataset::computed`]) already have their calculation
//! recorded, built the way `pyst` does it (`compute` then
//! [`CalculationDraft`]): its values appear in the Results pane right at
//! opening, and its output columns are inserted after the Y column. The others
//! wait for the analysis window to be opened. Column names stay the automatic
//! letters; only the titles (long names) and the units are readable.

use std::f64::consts::{LN_2, PI};

use anyhow::Context as _;
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
use stokesforge_analysis::voigt;
use stokesforge_core::{
    compute, family_english, output_unit, AuxRef, CalculationDraft, Column, ColumnMeta, ColumnRole,
    NodeId, OutputDraft, OutputName, Project, SeriesRef,
};

use super::noise::Noise;

/// What a dataset must recover after its calculation. Read by the tests only:
/// the binary does nothing with it, but these expectations **are** the proof
/// that the data say what they claim.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum Expected {
    /// A value of the Results pane: key, rank (range, peak), value, absolute
    /// tolerance.
    Value(&'static str, Option<usize>, f64, f64),
    /// A cell of an output column: key, row (0 = the first), value, absolute
    /// tolerance.
    Row(&'static str, usize, f64, f64),
}

/// Data and the operation that processes them.
pub(super) struct Dataset {
    /// Name of the workbook: unique across the whole project.
    pub(super) name: &'static str,
    /// Long name and unit of X, then of Y.
    pub(super) titles: [(&'static str, &'static str); 2],
    pub(super) x: Vec<f64>,
    pub(super) y: Vec<f64>,
    /// Uncertainty of each point (weighted fit).
    pub(super) sigma: Option<Vec<f64>>,
    /// The reference of a correction, the IRF of a deconvolution: its X, its Y.
    pub(super) reference: Option<(Vec<f64>, Vec<f64>)>,
    pub(super) operation: Operation,
    pub(super) expected: Vec<Expected>,
    /// The calculation is already recorded in the project (Results pane).
    pub(super) computed: bool,
}

/// The columns of a dataset once placed in the project.
pub(super) struct Columns {
    pub(super) x: NodeId,
    pub(super) y: NodeId,
    pub(super) sigma: Option<NodeId>,
    pub(super) reference: Option<AuxRef>,
}

impl Dataset {
    pub(super) fn series(&self, columns: &Columns) -> Vec<SeriesRef> {
        vec![SeriesRef {
            x: Some(columns.x),
            y: columns.y,
            sigma: columns.sigma,
        }]
    }
}

fn dataset(
    name: &'static str,
    titles: [(&'static str, &'static str); 2],
    (x, y): (Vec<f64>, Vec<f64>),
    operation: Operation,
    expected: Vec<Expected>,
) -> Dataset {
    Dataset {
        name,
        titles,
        x,
        y,
        sigma: None,
        reference: None,
        operation,
        expected,
        computed: false,
    }
}

/// `start, start + step, …, end` (bounds included).
fn grid(start: f64, end: f64, step: f64) -> Vec<f64> {
    let n = ((end - start) / step).round() as usize;
    (0..=n).map(|i| start + i as f64 * step).collect()
}

fn gaussian(x: f64, height: f64, center: f64, width: f64) -> f64 {
    let u = (x - center) / width;
    height * (-u * u).exp()
}

/// The Voigt profile of a peak, normalized to `height` at its top.
fn voigt_peak(x: f64, height: f64, center: f64, fwhm_gauss: f64, fwhm_lorentz: f64) -> f64 {
    height * voigt::profile(x - center, fwhm_gauss, fwhm_lorentz)
        / voigt::profile(0.0, fwhm_gauss, fwhm_lorentz)
}

fn fit_operation(model: Model) -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model,
            params: Vec::new(),
        },
        range: None,
        residuals: true,
    }
}

/// A Gaussian impulse response on the same step as the time signals: 0 to 3 ns
/// by 0.1 ns, centered at 1.5 ns.
fn instrument_response() -> (Vec<f64>, Vec<f64>) {
    let t = grid(0.0, 3.0, 0.1);
    let y = t
        .iter()
        .map(|&t| gaussian(t, 1.0, 1.5, 0.4 / (2.0 * LN_2.sqrt())))
        .collect();
    (t, y)
}

/// The signal of a sum of exponentials seen through `response`: the model of
/// the reconvolution (`A·Σ h_i·e^(−(k−i)·step/τ)`, `h` normalized to 1).
fn measured_decay(
    t: &[f64],
    response: &(Vec<f64>, Vec<f64>),
    background: f64,
    components: &[(f64, f64)],
) -> Vec<f64> {
    let step = t[1] - t[0];
    let mut h: Vec<f64> = t
        .iter()
        .map(|&t| {
            response
                .0
                .iter()
                .position(|&x| (x - t).abs() < 1e-9)
                .map_or(0.0, |i| response.1[i])
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= sum);
    let mut output = vec![background; t.len()];
    for &(amplitude, tau) in components {
        let factor = (-step / tau).exp();
        let mut c = 0.0;
        for (k, s) in output.iter_mut().enumerate() {
            c = factor * c + h[k];
            *s += amplitude * c;
        }
    }
    output
}

// ── The datasets, family by family ─────────────────────────────────────

/// A peak on a curving slope: the baseline is fitted on the two peak-free
/// flanks (zones, degree 2).
fn baseline() -> Dataset {
    let mut noise = Noise::new(11);
    let x = grid(0.0, 100.0, 0.5);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| 0.8 + 0.03 * x + 0.0006 * x * x + gaussian(x, 4.0, 50.0, 6.0))
        .collect();
    dataset(
        "Baseline",
        [("Displacement", "mm"), ("Signal", "mV")],
        (x, noise.noisy(&clean, 0.05)),
        Operation::Baseline {
            method: BaselineMethod::Zones {
                zones: vec![(0.0, 30.0), (70.0, 100.0)],
                degree: 2,
            },
            op: BaselineOp::Subtract,
            keep_baseline: true,
        },
        vec![Expected::Row("corrected", 100, 4.0, 0.15)],
    )
}

/// A noisy peak that Savitzky-Golay smooths without lowering it: the top keeps
/// its height (10) where an average over the same window would flatten it.
fn smoothing() -> Dataset {
    let mut noise = Noise::new(14);
    let x = grid(0.0, 100.0, 0.5);
    let clean: Vec<f64> = x.iter().map(|&x| gaussian(x, 10.0, 50.0, 8.0)).collect();
    dataset(
        "Smooth",
        [("Displacement", "mm"), ("Signal", "mV")],
        (x, noise.noisy(&clean, 0.5)),
        Operation::Smooth {
            spec: SmoothSpec {
                method: SmoothMethod::SavitzkyGolay { order: 3 },
                window: SmoothWindow::Points(15),
            },
        },
        vec![
            Expected::Value("count.window", None, 15.0, 0.0),
            Expected::Row("smoothed", 100, 10.0, 0.5),
            Expected::Row("smoothed", 0, 0.0, 0.5),
        ],
    )
}

/// A signal and its reference **on other abscissas**: the reference is
/// interpolated at the signal's X.
fn correct() -> Dataset {
    let mut noise = Noise::new(12);
    let lamp = |x: f64| 200.0 + gaussian(x, 1000.0, 600.0, 250.0);
    let transmission = |x: f64| 1.0 - gaussian(x, 0.6, 550.0, 20.0);
    let x = grid(400.0, 800.0, 5.0);
    let clean: Vec<f64> = x.iter().map(|&x| lamp(x) * transmission(x)).collect();
    let xr = grid(380.0, 820.0, 10.0);
    let yr = xr.iter().map(|&x| lamp(x)).collect();
    let mut dataset = dataset(
        "Correct",
        [("Wavelength", "nm"), ("Signal", "a.u.")],
        (x, noise.noisy(&clean, 3.0)),
        Operation::Correct {
            op: CorrectOp::Divide,
        },
        vec![
            Expected::Row("corrected", 30, 0.4, 0.03),
            Expected::Row("corrected", 0, 1.0, 0.03),
        ],
    );
    dataset.reference = Some((xr, yr));
    dataset
}

/// Two peaks, two integration ranges: area of each, cumulative and normalized
/// integral.
fn integrate() -> Dataset {
    let mut noise = Noise::new(13);
    let x = grid(0.0, 20.0, 0.1);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| gaussian(x, 2.0, 6.0, 1.0) + gaussian(x, 1.2, 14.0, 1.5))
        .collect();
    let mut dataset = dataset(
        "Integrate",
        [("Retention time", "min"), ("Absorbance", "")],
        (x, noise.noisy(&clean, 0.01)),
        Operation::Integrate {
            spec: IntegrateSpec {
                ranges: vec![(2.0, 10.0), (10.0, 18.0)],
                under: Under::Zero,
                cumulative: true,
                normalize: true,
            },
        },
        // h · s · √π
        vec![
            Expected::Value("area", Some(1), 2.0 * PI.sqrt(), 0.05),
            Expected::Value("area", Some(2), 1.2 * 1.5 * PI.sqrt(), 0.05),
        ],
    );
    dataset.computed = true;
    dataset
}

/// A weighted line: the uncertainty grows with X, the σ column carries it.
fn linear_fit() -> Dataset {
    let mut noise = Noise::new(14);
    let x = grid(0.0, 20.0, 0.5);
    let sigma: Vec<f64> = x.iter().map(|&x| 0.15 + 0.01 * x).collect();
    let y = x
        .iter()
        .zip(&sigma)
        .map(|(&x, &s)| 2.0 + 0.5 * x + noise.gauss(s))
        .collect();
    let mut dataset = dataset(
        "FitLinear",
        [("Concentration", "mmol/L"), ("Absorbance", "")],
        (x, y),
        fit_operation(Model::Linear),
        vec![
            Expected::Value("param.intercept", None, 2.0, 0.3),
            Expected::Value("param.slope", None, 0.5, 0.03),
        ],
    );
    dataset.sigma = Some(sigma);
    dataset
}

fn polynomial_fit() -> Dataset {
    let mut noise = Noise::new(15);
    let x = grid(-3.0, 3.0, 0.1);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| 1.0 - 2.0 * x + 0.5 * x * x + 0.3 * x * x * x)
        .collect();
    let mut dataset = dataset(
        "FitPolynomial",
        [("Voltage", "V"), ("Current", "mA")],
        (x, noise.noisy(&clean, 0.1)),
        fit_operation(Model::Polynomial { degree: 3 }),
        vec![
            Expected::Value("param.c0", None, 1.0, 0.1),
            Expected::Value("param.c1", None, -2.0, 0.1),
            Expected::Value("param.c2", None, 0.5, 0.1),
            Expected::Value("param.c3", None, 0.3, 0.05),
        ],
    );
    dataset.computed = true;
    dataset
}

fn exponential_fit() -> Dataset {
    let mut noise = Noise::new(16);
    let x = grid(0.0, 10.0, 0.1);
    let clean: Vec<f64> = x.iter().map(|&x| 0.5 + 4.0 * (-x / 2.2).exp()).collect();
    dataset(
        "FitExponential",
        [("Time", "s"), ("Intensity", "a.u.")],
        (x, noise.noisy(&clean, 0.05)),
        fit_operation(Model::ExponentialOffset),
        vec![
            Expected::Value("param.y0", None, 0.5, 0.15),
            Expected::Value("param.A", None, 4.0, 0.2),
            Expected::Value("param.tau", None, 2.2, 0.2),
        ],
    )
}

fn double_exponential_fit() -> Dataset {
    let mut noise = Noise::new(17);
    let x = grid(0.0, 30.0, 0.3);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| 0.2 + 3.0 * (-x / 1.2).exp() + 2.0 * (-x / 9.0).exp())
        .collect();
    dataset(
        "FitExponential2",
        [("Time", "ns"), ("Intensity", "a.u.")],
        (x, noise.noisy(&clean, 0.02)),
        fit_operation(Model::Exponential2),
        vec![
            Expected::Value("param.tau1", None, 1.2, 0.3),
            Expected::Value("param.tau2", None, 9.0, 1.5),
        ],
    )
}

fn power_fit() -> Dataset {
    let mut noise = Noise::new(18);
    let x = grid(1.0, 50.0, 1.0);
    let y = x
        .iter()
        .map(|&x| 2.5 * x.powf(0.7) * (1.0 + noise.gauss(0.02)))
        .collect();
    dataset(
        "FitPower",
        [("Frequency", "Hz"), ("Amplitude", "a.u.")],
        (x, y),
        fit_operation(Model::Power),
        vec![
            Expected::Value("param.a", None, 2.5, 0.2),
            Expected::Value("param.b", None, 0.7, 0.03),
        ],
    )
}

/// Strictly positive X: the logarithm is defined only for `x > 0`.
fn logarithm_fit() -> Dataset {
    let mut noise = Noise::new(19);
    let x = grid(1.0, 100.0, 1.0);
    let clean: Vec<f64> = x.iter().map(|&x| 3.0 + 1.8 * x.ln()).collect();
    dataset(
        "FitLogarithm",
        [("Dose", "mg"), ("Response", "%")],
        (x, noise.noisy(&clean, 0.1)),
        fit_operation(Model::Logarithm),
        vec![
            Expected::Value("param.a", None, 3.0, 0.15),
            Expected::Value("param.b", None, 1.8, 0.05),
        ],
    )
}

fn gaussian_fit() -> Dataset {
    let mut noise = Noise::new(20);
    let x = grid(-5.0, 5.0, 0.1);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| 0.1 + gaussian(x, 3.2, 0.4, 1.8 / (2.0 * LN_2.sqrt())))
        .collect();
    let mut dataset = dataset(
        "FitGaussian",
        [("Energy", "eV"), ("Intensity", "a.u.")],
        (x, noise.noisy(&clean, 0.05)),
        fit_operation(Model::Gaussian),
        vec![
            Expected::Value("param.y0", None, 0.1, 0.05),
            Expected::Value("param.A", None, 3.2, 0.1),
            Expected::Value("param.xc", None, 0.4, 0.03),
            Expected::Value("param.fwhm", None, 1.8, 0.1),
        ],
    );
    dataset.computed = true;
    dataset
}

fn lorentzian_fit() -> Dataset {
    let mut noise = Noise::new(21);
    let x = grid(-10.0, 10.0, 0.2);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| 0.05 + 2.4 / (1.0 + 4.0 * (x + 0.6) * (x + 0.6) / (2.2 * 2.2)))
        .collect();
    dataset(
        "FitLorentzian",
        [("Frequency", "MHz"), ("Intensity", "a.u.")],
        (x, noise.noisy(&clean, 0.04)),
        fit_operation(Model::Lorentzian),
        vec![
            Expected::Value("param.A", None, 2.4, 0.15),
            Expected::Value("param.xc", None, -0.6, 0.05),
            Expected::Value("param.fwhm", None, 2.2, 0.15),
        ],
    )
}

/// A hand-written model: Michaelis-Menten saturation.
fn custom_fit() -> Dataset {
    let mut noise = Noise::new(22);
    let x = grid(0.2, 10.0, 0.2);
    let clean: Vec<f64> = x.iter().map(|&x| 5.0 * x / (1.5 + x)).collect();
    dataset(
        "FitCustom",
        [("Substrate", "mmol/L"), ("Rate", "µmol/min")],
        (x, noise.noisy(&clean, 0.08)),
        fit_operation(Model::Custom {
            formula: "a*x/(b+x)".into(),
        }),
        vec![
            Expected::Value("param.a", None, 5.0, 0.3),
            Expected::Value("param.b", None, 1.5, 0.2),
        ],
    )
}

/// Three overlapping Voigt peaks on a linear background.
fn peaks() -> Dataset {
    let mut noise = Noise::new(23);
    let x = grid(0.0, 100.0, 0.5);
    let clean: Vec<f64> = x
        .iter()
        .map(|&x| {
            0.3 + 0.004 * x
                + voigt_peak(x, 3.0, 40.0, 5.0, 3.0)
                + voigt_peak(x, 2.0, 55.0, 4.0, 3.0)
                + voigt_peak(x, 1.2, 62.0, 4.0, 3.0)
        })
        .collect();
    dataset(
        "Peaks",
        [("Raman shift", "cm⁻¹"), ("Intensity", "cps")],
        (x, noise.noisy(&clean, 0.03)),
        Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Voigt,
                background: PeakBackground::Linear,
                seeds: [40.0, 55.0, 62.0]
                    .into_iter()
                    .map(|center| PeakSeed {
                        center,
                        height: None,
                        fwhm: None,
                    })
                    .collect(),
                link_widths: false,
                center_window: None,
            },
            auto: None,
            range: None,
            residuals: true,
        },
        vec![
            Expected::Value("peak.center", Some(1), 40.0, 0.5),
            Expected::Value("peak.center", Some(2), 55.0, 0.5),
            Expected::Value("peak.center", Some(3), 62.0, 0.8),
        ],
    )
}

/// The signal of a double decay seen through the IRF, and this IRF on the same
/// step (0.1 ns): the grid is uniform, as the reconvolution and the direct
/// deconvolution require.
fn decay(name: &'static str, seed: u64, components: &[(f64, f64)]) -> Dataset {
    let mut noise = Noise::new(seed);
    let t = grid(0.0, 25.5, 0.1);
    let response = instrument_response();
    let clean = measured_decay(&t, &response, 2.0, components);
    let mut dataset = dataset(
        name,
        [("Time", "ns"), ("Intensity", "cps")],
        (t, noise.noisy(&clean, 1.5)),
        Operation::Reconvolution {
            spec: ReconvSpec {
                components: components.len(),
                params: Vec::new(),
                shift_initial: 0.0,
                fit_shift: false,
                window: None,
            },
            residuals: true,
        },
        Vec::new(),
    );
    dataset.reference = Some(response);
    dataset
}

fn reconvolution() -> Dataset {
    let mut dataset = decay("Reconvolution", 24, &[(800.0, 0.9), (300.0, 4.5)]);
    dataset.expected = vec![
        Expected::Value("param.tau1", None, 0.9, 0.15),
        Expected::Value("param.tau2", None, 4.5, 0.8),
    ];
    dataset
}

fn deconvolution() -> Dataset {
    let mut dataset = decay("Deconvolution", 25, &[(500.0, 2.0)]);
    dataset.operation = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 40 },
        shift: 0.0,
    };
    // The decay of the object, `500·e^(−t/2)`, read back at t = 2 ns.
    dataset.expected = vec![Expected::Row(
        "deconvolved",
        20,
        500.0 * (-1.0f64).exp(),
        25.0,
    )];
    dataset
}

/// All the datasets, in the order of the Analysis menu families.
pub(super) fn datasets() -> Vec<Dataset> {
    vec![
        baseline(),
        correct(),
        smoothing(),
        integrate(),
        linear_fit(),
        polynomial_fit(),
        exponential_fit(),
        double_exponential_fit(),
        power_fit(),
        logarithm_fit(),
        gaussian_fit(),
        lorentzian_fit(),
        custom_fit(),
        peaks(),
        reconvolution(),
        deconvolution(),
    ]
}

/// A titled numeric column: long name, unit and role (the name stays the
/// automatic letter).
pub(super) fn titled_column(
    project: &mut Project,
    sheet: NodeId,
    values: &[f64],
    (title, unit): (&str, &str),
    role: ColumnRole,
) -> anyhow::Result<NodeId> {
    let mut column = Column::from_f64(values);
    column.set_meta(ColumnMeta {
        role,
        long_name: title.to_string(),
        unit: unit.to_string(),
        ..ColumnMeta::default()
    });
    project
        .add_column(sheet, None, None, column)
        .with_context(|| format!("column {title}"))
}

/// Places the workbook of a dataset under `parent`: X, Y, then the uncertainty
/// and the reference if the dataset has them.
pub(super) fn build(
    project: &mut Project,
    parent: NodeId,
    dataset: &Dataset,
) -> anyhow::Result<Columns> {
    let workbook = project
        .add_workbook(parent, Some(dataset.name))
        .with_context(|| format!("workbook {}", dataset.name))?;
    let sheet = project
        .tree()
        .children(workbook)?
        .first()
        .copied()
        .context("sheet of the analysis workbook")?;
    let x = titled_column(project, sheet, &dataset.x, dataset.titles[0], ColumnRole::X)?;
    let y = titled_column(project, sheet, &dataset.y, dataset.titles[1], ColumnRole::Y)?;
    let sigma = dataset
        .sigma
        .as_ref()
        .map(|values| {
            titled_column(
                project,
                sheet,
                values,
                ("Uncertainty", dataset.titles[1].1),
                ColumnRole::YError,
            )
        })
        .transpose()?;
    let reference = dataset
        .reference
        .as_ref()
        .map(|(rx, ry)| -> anyhow::Result<AuxRef> {
            let (title_x, title_y) = if matches!(dataset.operation, Operation::Correct { .. }) {
                (("Wavelength (ref.)", "nm"), ("Reference", "a.u."))
            } else {
                (("Time (IRF)", "ns"), ("Instrument response", "a.u."))
            };
            Ok(AuxRef {
                x: Some(titled_column(project, sheet, rx, title_x, ColumnRole::X)?),
                y: titled_column(project, sheet, ry, title_y, ColumnRole::Y)?,
            })
        })
        .transpose()?;
    Ok(Columns {
        x,
        y,
        sigma,
        reference,
    })
}

/// Records the dataset's calculation the way `pyst` does: `compute`, then a
/// draft whose output columns carry the long name, the unit and the comment
/// that the core gives.
fn record_calculation(
    project: &mut Project,
    dataset: &Dataset,
    columns: &Columns,
) -> anyhow::Result<()> {
    let series = dataset.series(columns);
    let computed = compute(
        project.tree(),
        &dataset.operation,
        &series,
        columns.reference.as_ref(),
    )
    .with_context(|| format!("calculation of {}", dataset.name))?;
    let kind = dataset.operation.kind();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta {
                long_name: OutputName::of(project.tree(), kind, &series[c.series], &c.key, c.index)
                    .english(),
                unit: output_unit(project.tree(), &series[c.series], &c.key),
                comment: family_english(kind).to_string(),
                ..ColumnMeta::default()
            },
            data: c.data.clone(),
        })
        .collect();
    project
        .add_calculation(CalculationDraft {
            operation: dataset.operation.clone(),
            series,
            aux: columns.reference.clone(),
            outputs,
            results: computed.results,
            keep_results: true,
            auto_recalc: dataset.operation.auto_recalc_by_default(1),
            low_r2_dialog: true,
        })
        .with_context(|| format!("recording the calculation of {}", dataset.name))?;
    Ok(())
}

/// The `Analysis` folder: one workbook per dataset, and the calculation of
/// those that have one.
pub(super) fn folder(project: &mut Project) -> anyhow::Result<NodeId> {
    let root = project.root();
    let folder = project
        .add_folder(root, "Analysis")
        .context("Analysis folder")?;
    for dataset in datasets() {
        let columns = build(project, folder, &dataset)?;
        if dataset.computed {
            record_calculation(project, &dataset, &columns)?;
        }
    }
    Ok(folder)
}
