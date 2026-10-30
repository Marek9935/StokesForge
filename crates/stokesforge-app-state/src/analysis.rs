//! The model of the analysis window (#181): what the user sets to run a
//! computation (baseline, correction, integral, fit, peaks, deconvolution),
//! **without egui**.
//!
//! The window (`app/analysis_dialog.rs`) only paints this form and pushes
//! [`Action`](crate::state::Action)s; everything that is decided — the default
//! values of an operation, what is missing to validate, the operation the
//! settings describe, the columns produced and their names — is read here and
//! tested without a window (`tests/analysis.rs`).
//!
//! # One setting per family, all kept
//!
//! The form keeps the settings of **every** family at the same time
//! ([`Setup`]): going from "Reconvolution" to "Direct Deconvolution", or from a
//! two-point baseline to zones, loses nothing of what had been set.
//! [`AnalysisForm::operation`] reads only the active family. An [`Operation`] is
//! built only once the settings are complete: a one-point baseline is not an
//! operation, it is an [`Issue`].
//!
//! # Preview and validation
//!
//! [`AnalysisForm::preview`] computes the displayed series (all the series for
//! a global fit) with the helper columns forced — the baseline and the
//! residuals show even if the column will not be written.
//! [`AnalysisForm::draft`] computes the whole batch, keeps only the requested
//! columns and gives them a long name (M6); this is what the `AddCalculation` /
//! `UpdateCalculation` command receives.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{AutoDetect, Operation, OperationKind, Readiness, HEAVY_BATCH};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::series::Series;
use stokesforge_analysis::smooth::{self, SmoothMethod, SmoothSpec, SmoothWindow};
use stokesforge_analysis::AnalysisError;
use stokesforge_core::{
    compute, AuxRef, Calculation, CalculationDraft, ComputeError, Computed, NodeId, OutputDraft,
    OutputName, SeriesRef, Tree,
};

use i18n_embed::fluent::FluentLanguageLoader;

use crate::tr;

/// Default number of points averaged when a point is snapped onto the curve
/// (M4 b).
pub const DEFAULT_AVERAGE: usize = 5;

/// Bounds of the "average over N points" setting (wheel).
pub const AVERAGE_RANGE: std::ops::RangeInclusive<usize> = 1..=101;

/// An entry of the Analysis menu: the family **and** the pre-chosen method
/// (M1 B). The window can change method, for "Edit Calculation…".
#[derive(Debug, Clone, PartialEq)]
pub enum Preset {
    /// Constant baseline from one point or the mean of a zone.
    BaselineConstant,
    /// Baseline from two points.
    BaselineLine,
    /// Polynomial baseline on zones.
    BaselinePolynomial,
    /// Baseline from anchor points.
    BaselineAnchors,
    /// Correction by a reference column.
    Correct,
    /// Integral over one or more ranges.
    Integrate,
    /// Fit, with the pre-chosen model.
    Fit(Model),
    /// Peak decomposition.
    Peaks,
    /// Reconvolution by an instrument response.
    Reconvolution,
    /// Direct deconvolution.
    DirectDeconvolution,
    /// Smoothing by adjacent averaging (#324).
    SmoothAdjacent,
    /// Savitzky-Golay smoothing (#324).
    SmoothSavitzkyGolay,
    /// Smoothing by a rolling median (#324).
    SmoothMedian,
}

impl Preset {
    /// The operation family this choice launches.
    pub fn kind(&self) -> OperationKind {
        match self {
            Preset::BaselineConstant
            | Preset::BaselineLine
            | Preset::BaselinePolynomial
            | Preset::BaselineAnchors => OperationKind::Baseline,
            Preset::Correct => OperationKind::Correct,
            Preset::Integrate => OperationKind::Integrate,
            Preset::Fit(_) => OperationKind::Fit,
            Preset::Peaks => OperationKind::Peaks,
            Preset::Reconvolution => OperationKind::Reconvolution,
            Preset::DirectDeconvolution => OperationKind::DirectDeconvolution,
            Preset::SmoothAdjacent | Preset::SmoothSavitzkyGolay | Preset::SmoothMedian => {
                OperationKind::Smooth
            }
        }
    }

    /// The category (hence the window) of this choice.
    pub fn category(&self) -> Category {
        Category::of(self.kind())
    }

    /// The models offered by `Fitting ▸` (the free model is typed in the window, it
    /// has no menu entry that pre-fills it).
    pub fn fit_models() -> [Model; 10] {
        [
            Model::Linear,
            Model::Polynomial { degree: 3 },
            Model::Exponential,
            Model::ExponentialOffset,
            Model::Exponential2,
            Model::Logarithm,
            Model::Power,
            Model::Gaussian,
            Model::Lorentzian,
            Model::Custom {
                formula: String::new(),
            },
        ]
    }
}

/// The category of an analysis (#248), as in OriginLab: one Analysis menu entry,
/// one window. Only `DataManipulation` transforms the curve and can **replace**
/// the data; the others write new columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// Baselines, correction by a reference and smoothing.
    DataManipulation,
    /// Fit by a model.
    Fitting,
    /// Peak decomposition, reconvolution, direct deconvolution.
    PeaksAndDeconvolution,
    /// Integral.
    Integrate,
}

impl Category {
    /// The categories, in menu order.
    pub const ALL: [Category; 4] = [
        Category::DataManipulation,
        Category::Fitting,
        Category::PeaksAndDeconvolution,
        Category::Integrate,
    ];

    /// The category of an operation family.
    pub fn of(kind: OperationKind) -> Self {
        match kind {
            OperationKind::Baseline | OperationKind::Correct | OperationKind::Smooth => {
                Category::DataManipulation
            }
            OperationKind::Fit => Category::Fitting,
            OperationKind::Peaks
            | OperationKind::Reconvolution
            | OperationKind::DirectDeconvolution => Category::PeaksAndDeconvolution,
            OperationKind::Integrate => Category::Integrate,
        }
    }

    /// The families that the category's window offers ("Analysis type").
    pub fn kinds(self) -> &'static [OperationKind] {
        match self {
            Category::DataManipulation => &[
                OperationKind::Baseline,
                OperationKind::Correct,
                OperationKind::Smooth,
            ],
            Category::Fitting => &[OperationKind::Fit],
            Category::PeaksAndDeconvolution => &[
                OperationKind::Peaks,
                OperationKind::Reconvolution,
                OperationKind::DirectDeconvolution,
            ],
            Category::Integrate => &[OperationKind::Integrate],
        }
    }

    /// The method pre-chosen when the category's window is opened without a method
    /// (right-click).
    pub fn default_preset(self) -> Preset {
        match self {
            Category::DataManipulation => Preset::BaselineLine,
            Category::Fitting => Preset::Fit(Model::Linear),
            Category::PeaksAndDeconvolution => Preset::Peaks,
            Category::Integrate => Preset::Integrate,
        }
    }

    /// The name of the category, in the current language.
    pub fn title(self) -> String {
        match self {
            Category::DataManipulation => tr!("analysis-category-data-manipulation"),
            Category::Fitting => tr!("analysis-category-fitting"),
            Category::PeaksAndDeconvolution => tr!("analysis-category-peaks"),
            Category::Integrate => tr!("analysis-category-integrate"),
        }
    }
}

/// Where a recent analysis is rerun (#249): the current selection, columns of a
/// worksheet or curves of a graph.
#[derive(Debug, Clone, PartialEq)]
pub enum RecentTarget {
    Columns {
        sheet: NodeId,
        columns: Vec<NodeId>,
    },
    Graph {
        graph: NodeId,
        series: Vec<stokesforge_plot::SeriesRef>,
        spec: Box<stokesforge_plot::PlotSpec>,
    },
    /// Whole rows `(worksheet, row)` (#253).
    Rows {
        rows: Vec<(NodeId, usize)>,
    },
}

/// The shape of a baseline (M10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineShape {
    /// Flat background from one point.
    Constant,
    /// Flat background from the mean of a zone.
    ConstantZone,
    /// A straight line through two placed points.
    TwoPoints,
    /// A polynomial fitted on background zones.
    Zones,
    /// A curve that goes through anchor points.
    Anchors,
}

/// Baseline settings.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineSetup {
    pub shape: BaselineShape,
    /// Settings specific to constants: changing shape keeps those of the anchors
    /// and of the polynomial, even after a new gesture on the preview.
    pub constant_point: Option<(f64, f64)>,
    pub constant_zone: Option<(f64, f64)>,
    /// The two points of the line, or the anchor points.
    pub points: Vec<(f64, f64)>,
    /// The background zones.
    pub zones: Vec<(f64, f64)>,
    /// Degree of the zones' polynomial (1: a line).
    pub degree: u32,
    pub interpolation: Interpolation,
    pub op: BaselineOp,
    /// Also write the baseline into a column.
    pub baseline_column: bool,
}

impl Default for BaselineSetup {
    fn default() -> Self {
        Self {
            shape: BaselineShape::Anchors,
            constant_point: None,
            constant_zone: None,
            points: Vec::new(),
            zones: Vec::new(),
            degree: 1,
            interpolation: Interpolation::Segments,
            op: BaselineOp::Subtract,
            baseline_column: false,
        }
    }
}

/// Integral settings.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrateSetup {
    /// Ranges `(from, to)`; empty: the whole series.
    pub ranges: Vec<(f64, f64)>,
    pub under: Under,
    pub cumulative: bool,
    pub normalize: bool,
}

impl Default for IntegrateSetup {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        }
    }
}

/// Fit settings.
#[derive(Debug, Clone, PartialEq)]
pub struct FitSetup {
    pub model: Model,
    /// One setting per model parameter (see [`FitSetup::reset_params`]).
    pub params: Vec<ParamSetting>,
    /// Fitted X range; `None`: the whole series.
    pub range: Option<(f64, f64)>,
    pub residuals: bool,
    /// Weight by the error column of each series (`1/σ`). Starts ticked when
    /// a series has an error column ([`AnalysisForm::new`], #360).
    pub weighted: bool,
}

impl Default for FitSetup {
    fn default() -> Self {
        Self {
            model: Model::Linear,
            params: Vec::new(),
            range: None,
            residuals: false,
            weighted: false,
        }
    }
}

impl FitSetup {
    /// The number of parameters of the current model (0 if a free model does not
    /// compile yet).
    pub fn param_count(&self) -> usize {
        self.model.compile().map_or(0, |m| m.param_count())
    }

    /// The names of the parameters of the current model.
    pub fn param_names(&self) -> Vec<String> {
        self.model.compile().map_or_else(
            |_| Vec::new(),
            |m| m.params().iter().map(|p| p.name.clone()).collect(),
        )
    }

    /// Resets the per-parameter settings to their default, at the right length
    /// (change of model, degree or expression).
    pub fn reset_params(&mut self) {
        self.params = vec![ParamSetting::default(); self.param_count()];
    }

    /// Fills the starting values with the model's automatic estimate on the given
    /// `(x, y)` points, restricted to the fitted range ("Auto" button, #204). The
    /// rule is that of `FitSpec::auto_initial` (`stokesforge-analysis`). No effect
    /// if the model does not compile.
    pub fn auto_initial(&mut self, x: &[f64], y: &[f64]) {
        let mut spec = FitSpec {
            model: self.model.clone(),
            params: std::mem::take(&mut self.params),
        };
        spec.auto_initial(&Series::new(x.to_vec(), y.to_vec()), self.range);
        self.params = spec.params;
    }

    /// The model changes: settings reset.
    pub fn set_model(&mut self, model: Model) {
        self.model = model;
        self.reset_params();
    }

    /// At least one parameter is shared: global fit.
    pub fn is_global(&self) -> bool {
        self.params.iter().any(|p| p.shared)
    }
}

/// Peak decomposition settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PeaksSetup {
    pub shape: PeakShape,
    pub background: PeakBackground,
    /// Starting positions set by hand; when empty, detection finds them.
    pub seeds: Vec<PeakSeed>,
    pub link_widths: bool,
    /// Half-width (in X) within which each position stays bounded around its
    /// starting point; `None`: free positions (#204).
    pub center_window: Option<f64>,
    /// Minimum prominence for detection, in % of the Y extent.
    pub prominence_percent: f64,
    pub max_peaks: usize,
    pub range: Option<(f64, f64)>,
    pub residuals: bool,
    /// One column per peak.
    pub peak_columns: bool,
    /// One column for the sum of the peaks (background included).
    pub sum_column: bool,
}

impl Default for PeaksSetup {
    fn default() -> Self {
        Self {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds: Vec::new(),
            link_widths: false,
            center_window: None,
            prominence_percent: 5.0,
            max_peaks: 6,
            range: None,
            residuals: false,
            peak_columns: true,
            sum_column: true,
        }
    }
}

/// Reconvolution settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconvSetup {
    pub spec: ReconvSpec,
    pub residuals: bool,
}

impl Default for ReconvSetup {
    fn default() -> Self {
        let mut setup = Self {
            spec: ReconvSpec {
                components: 1,
                params: Vec::new(),
                shift_initial: 0.0,
                fit_shift: true,
                window: None,
            },
            residuals: false,
        };
        setup.resize_params();
        setup
    }
}

impl ReconvSetup {
    /// Number of parameters: background, one amplitude and one lifetime per
    /// component, then the shift.
    pub fn param_count(&self) -> usize {
        2 + 2 * self.spec.components
    }

    /// Adjusts the length of the settings list to the number of components. The
    /// shift is the **last** parameter: it is set aside, the components are resized,
    /// then it is put back at the end — otherwise its settings (start, fixed,
    /// bounds) would slide onto the added component.
    pub fn resize_params(&mut self) {
        let count = self.param_count();
        let shift = self.spec.params.pop().unwrap_or_default();
        self.spec.params.resize(count - 1, ParamSetting::default());
        self.spec.params.push(shift);
    }
}

/// The direct deconvolution algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectAlgorithm {
    RichardsonLucy,
    Wiener,
}

/// Direct deconvolution settings.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectSetup {
    pub algorithm: DirectAlgorithm,
    pub iterations: usize,
    /// Wiener noise-to-signal ratio; `None`: estimated.
    pub noise: Option<f64>,
    pub shift: f64,
}

impl Default for DirectSetup {
    fn default() -> Self {
        Self {
            algorithm: DirectAlgorithm::RichardsonLucy,
            iterations: 50,
            noise: None,
            shift: 0.0,
        }
    }
}

/// The algorithm of a smoothing (#324), the polynomial order being kept apart
/// so that it survives a change of method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothAlgorithm {
    AdjacentAverage,
    SavitzkyGolay,
    Median,
}

/// The unit in which a smoothing's window is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothUnit {
    Points,
    XUnits,
}

/// Settings of a smoothing (#324).
#[derive(Debug, Clone, PartialEq)]
pub struct SmoothSetup {
    pub algorithm: SmoothAlgorithm,
    pub unit: SmoothUnit,
    /// Window in points: always odd (see [`Self::set_points`]).
    pub points: usize,
    /// Window in X units.
    pub x_width: f64,
    /// Savitzky-Golay polynomial order.
    pub order: usize,
}

impl Default for SmoothSetup {
    fn default() -> Self {
        Self {
            algorithm: SmoothAlgorithm::AdjacentAverage,
            unit: SmoothUnit::Points,
            points: 5,
            x_width: 1.0,
            order: 2,
        }
    }
}

impl SmoothSetup {
    /// Sets the window in points: bounded to `[MIN_WINDOW, MAX_WINDOW]` and raised
    /// to the next odd number (a centered smoothing has as many neighbors on each
    /// side), so that the field cannot produce a setting the computation would
    /// refuse.
    pub fn set_points(&mut self, points: usize) {
        self.points = (points | 1).clamp(smooth::MIN_WINDOW, smooth::MAX_WINDOW);
    }

    /// The operation these settings describe.
    pub fn spec(&self) -> SmoothSpec {
        SmoothSpec {
            method: match self.algorithm {
                SmoothAlgorithm::AdjacentAverage => SmoothMethod::AdjacentAverage,
                SmoothAlgorithm::SavitzkyGolay => SmoothMethod::SavitzkyGolay { order: self.order },
                SmoothAlgorithm::Median => SmoothMethod::Median,
            },
            window: match self.unit {
                SmoothUnit::Points => SmoothWindow::Points(self.points),
                SmoothUnit::XUnits => SmoothWindow::XUnits(self.x_width),
            },
        }
    }
}

/// The settings of all the families, kept together.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub baseline: BaselineSetup,
    pub correct: CorrectOp,
    pub integrate: IntegrateSetup,
    pub fit: FitSetup,
    pub peaks: PeaksSetup,
    pub reconv: ReconvSetup,
    pub direct: DirectSetup,
    pub smooth: SmoothSetup,
}

impl Default for Setup {
    fn default() -> Self {
        let mut fit = FitSetup::default();
        fit.reset_params();
        Self {
            baseline: BaselineSetup::default(),
            correct: CorrectOp::Divide,
            integrate: IntegrateSetup::default(),
            fit,
            peaks: PeaksSetup::default(),
            reconv: ReconvSetup::default(),
            direct: DirectSetup::default(),
            smooth: SmoothSetup::default(),
        }
    }
}

/// The active tool of the preview (M4 a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTool {
    /// A click sets a point (anchor, peak seed, bound).
    #[default]
    Points,
    /// Dragging sets a zone or a range.
    Zone,
}

/// What is missing to validate or to show the preview.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// No series to process.
    NoSeries,
    /// The correction awaits a reference column.
    NeedsReference,
    /// A deconvolution awaits an instrument response.
    NeedsResponse,
    /// Not enough points placed.
    NeedsPoints { needed: usize, have: usize },
    /// At least one background zone.
    NeedsZone,
    /// The free model is unreadable (`Some`) or has no parameter (`None`).
    BadModel(Option<stokesforge_analysis::expr::ExprError>),
    /// A cited column no longer exists.
    MissingColumn,
    /// The computation itself failed.
    Failed(AnalysisError),
}

/// The complete form of an analysis window.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisForm {
    /// The active family; the settings of the others are kept in `setup`.
    pub kind: OperationKind,
    pub setup: Setup,
    pub series: Vec<SeriesRef>,
    /// Reference of a correction, instrument response of a deconvolution.
    pub aux: Option<AuxRef>,
    /// "Keep in Results" rather than "Show" (M5).
    pub keep_results: bool,
    /// Automatic recalculation (M9 b).
    pub auto_recalc: bool,
    /// The user touched the checkbox: a change of family no longer resets it to the
    /// operation's default.
    pub auto_recalc_chosen: bool,
    /// Dialog box when R² is low (#288), per computation.
    pub low_r2_dialog: bool,
    /// The computation being edited ("Edit Calculation…").
    pub editing: Option<NodeId>,
    /// The graph the user comes from: "Add to graph" puts the columns there.
    pub graph: Option<NodeId>,
    pub add_to_graph: bool,
    /// Permanently replaces the Y values; only Undo returns to the raw values.
    pub replace_original: bool,
    /// Framing and styles frozen at the gesture from a graph (#210), with the
    /// addresses of the sources in this spec; never persisted as a computation.
    pub graph_source: Option<(stokesforge_plot::PlotSpec, Vec<stokesforge_plot::SeriesRef>)>,
    /// The series shown in the preview.
    pub current: usize,
    pub tool: PreviewTool,
    /// Local mean of a snapped point (M4 b).
    pub average: usize,
}

impl AnalysisForm {
    /// A fresh form for `preset` on `series`.
    pub fn new(preset: &Preset, series: Vec<SeriesRef>, keep_results: bool) -> Self {
        let mut setup = Setup::default();
        match preset {
            Preset::BaselineConstant => setup.baseline.shape = BaselineShape::Constant,
            Preset::BaselineLine => setup.baseline.shape = BaselineShape::TwoPoints,
            Preset::BaselinePolynomial => {
                setup.baseline.shape = BaselineShape::Zones;
                setup.baseline.degree = 3;
            }
            Preset::BaselineAnchors => setup.baseline.shape = BaselineShape::Anchors,
            Preset::Fit(model) => setup.fit.set_model(model.clone()),
            Preset::SmoothAdjacent => setup.smooth.algorithm = SmoothAlgorithm::AdjacentAverage,
            Preset::SmoothSavitzkyGolay => {
                setup.smooth.algorithm = SmoothAlgorithm::SavitzkyGolay;
                // A polynomial of order 2 over 5 points is too tight to smooth:
                // 9 points is the current default.
                setup.smooth.points = 9;
            }
            Preset::SmoothMedian => setup.smooth.algorithm = SmoothAlgorithm::Median,
            _ => {}
        }
        let kind = preset.kind();
        let mut form = Self {
            kind,
            setup,
            series,
            aux: None,
            keep_results,
            auto_recalc: true,
            auto_recalc_chosen: false,
            low_r2_dialog: true,
            editing: None,
            graph: None,
            add_to_graph: false,
            replace_original: false,
            graph_source: None,
            current: 0,
            tool: PreviewTool::default(),
            average: DEFAULT_AVERAGE,
        };
        // A fit on a series that has an error column starts weighted (#360);
        // the user can untick it, and `from_calculation` restores the stored
        // choice when editing.
        form.setup.fit.weighted = form.series.iter().any(|s| s.sigma.is_some());
        form.apply_default_auto();
        form.tool = form.natural_tool();
        form
    }

    /// Do the two forms give the same preview? The mouse tool, the local mean, the
    /// destination and the output checkboxes do not change what is computed: the
    /// preview is not recomputed for them.
    pub fn same_preview(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.current == other.current
            && self.aux == other.aux
            && self.series == other.series
            && self.setup == other.setup
    }

    /// The form that reopens an existing computation ("Edit Calculation…").
    pub fn from_calculation(id: NodeId, calc: &Calculation) -> Self {
        let mut form = Self::new(
            &preset_of(&calc.operation),
            calc.series.clone(),
            calc.keep_results,
        );
        form.editing = Some(id);
        form.aux = calc.aux.clone();
        form.auto_recalc = calc.auto_recalc;
        form.low_r2_dialog = calc.low_r2_dialog;
        form.auto_recalc_chosen = true;
        form.setup.load(&calc.operation);
        // Weighting is not in the operation: it is read from the series.
        form.setup.fit.weighted = calc.series.iter().any(|s| s.sigma.is_some());
        let has = |key: &str| calc.outputs.iter().any(|o| o.key == key);
        form.setup.peaks.peak_columns = has("peak");
        form.setup.peaks.sum_column = has("peaks_sum");
        form
    }

    /// Changes family; automatic recalculation returns to the default of the new
    /// operation as long as the user has not chosen.
    pub fn set_kind(&mut self, kind: OperationKind) {
        self.kind = kind;
        self.apply_default_auto();
    }

    /// Resets the "automatic recalculation" checkbox to the default of the
    /// operation and of the number of series (reconvolution, deconvolution and
    /// large batches: manual).
    pub fn apply_default_auto(&mut self) {
        if self.auto_recalc_chosen {
            return;
        }
        self.auto_recalc = self.auto_by_default();
    }

    /// The M9 b default for the current family and number of series.
    pub fn auto_by_default(&self) -> bool {
        !matches!(
            self.kind,
            OperationKind::Reconvolution | OperationKind::DirectDeconvolution
        ) && self.series.len() <= HEAVY_BATCH
    }

    /// This family reads an auxiliary column.
    pub fn needs_aux(&self) -> bool {
        matches!(
            self.kind,
            OperationKind::Correct
                | OperationKind::Reconvolution
                | OperationKind::DirectDeconvolution
        )
    }

    /// The fit links parameters across series: the preview computes them all.
    pub fn is_global(&self) -> bool {
        self.kind == OperationKind::Fit && self.setup.fit.is_global() && self.series.len() > 1
    }

    /// The series are named one by one in the preview (`◀ ▶`).
    pub fn clamp_current(&mut self) {
        if self.current >= self.series.len() {
            self.current = self.series.len().saturating_sub(1);
        }
    }

    /// What prevents validating, without consulting the data.
    pub fn issues(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        if self.series.is_empty() {
            issues.push(Issue::NoSeries);
        }
        if self.needs_aux() && self.aux.is_none() {
            issues.push(if self.kind == OperationKind::Correct {
                Issue::NeedsReference
            } else {
                Issue::NeedsResponse
            });
        }
        issues.extend(self.setup_issues());
        issues
    }

    /// The settings of the active family that are missing: the draft is described
    /// as an [`Operation`] (even incomplete) and `stokesforge-analysis` says what
    /// is missing ([`Operation::readiness`]).
    fn setup_issues(&self) -> Option<Issue> {
        issue_of(self.draft_operation(false).readiness())
    }

    /// The operation the settings describe, or `None` while a setting is missing.
    /// `for_preview` forces the helper columns (baseline, residuals) so that they
    /// show even if not written.
    pub fn operation_for(&self, for_preview: bool) -> Option<Operation> {
        let op = self.draft_operation(for_preview);
        (op.readiness() == Readiness::Ready).then_some(op)
    }

    /// The operation of the active family's settings, **without judging** whether
    /// they suffice: a point or a zone not yet placed is a `NaN` there, which
    /// [`Operation::readiness`] reads as "to be placed".
    fn draft_operation(&self, for_preview: bool) -> Operation {
        const UNPOSED: (f64, f64) = (f64::NAN, f64::NAN);
        let s = &self.setup;
        match self.kind {
            OperationKind::Baseline => {
                let b = &s.baseline;
                let method = match b.shape {
                    BaselineShape::Constant => BaselineMethod::Constant {
                        point: b.constant_point.unwrap_or(UNPOSED),
                    },
                    BaselineShape::ConstantZone => BaselineMethod::ConstantZone {
                        zone: b.constant_zone.unwrap_or(UNPOSED),
                    },
                    BaselineShape::TwoPoints => BaselineMethod::Line {
                        p1: b.points.first().copied().unwrap_or(UNPOSED),
                        p2: b.points.get(1).copied().unwrap_or(UNPOSED),
                    },
                    BaselineShape::Zones => BaselineMethod::Zones {
                        zones: b.zones.clone(),
                        degree: b.degree,
                    },
                    BaselineShape::Anchors => BaselineMethod::Anchors {
                        points: b.points.clone(),
                        interpolation: b.interpolation,
                    },
                };
                Operation::Baseline {
                    method,
                    op: b.op,
                    keep_baseline: b.baseline_column || for_preview,
                }
            }
            OperationKind::Correct => Operation::Correct { op: s.correct },
            OperationKind::Integrate => Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: s.integrate.ranges.clone(),
                    under: s.integrate.under,
                    cumulative: s.integrate.cumulative || for_preview,
                    normalize: s.integrate.normalize,
                },
            },
            OperationKind::Fit => Operation::Fit {
                spec: FitSpec {
                    model: s.fit.model.clone(),
                    params: s.fit.params.clone(),
                },
                range: s.fit.range,
                residuals: s.fit.residuals || for_preview,
            },
            OperationKind::Peaks => {
                let p = &s.peaks;
                Operation::Peaks {
                    spec: PeaksSpec {
                        shape: p.shape,
                        background: p.background,
                        seeds: p.seeds.clone(),
                        link_widths: p.link_widths,
                        center_window: p.center_window,
                    },
                    auto: Some(AutoDetect {
                        min_prominence: p.prominence_percent / 100.0,
                        max_peaks: p.max_peaks,
                    }),
                    range: p.range,
                    residuals: p.residuals || for_preview,
                }
            }
            OperationKind::Reconvolution => Operation::Reconvolution {
                spec: s.reconv.spec.clone(),
                residuals: s.reconv.residuals || for_preview,
            },
            OperationKind::Smooth => Operation::Smooth {
                spec: s.smooth.spec(),
            },
            OperationKind::DirectDeconvolution => {
                let d = &s.direct;
                Operation::DirectDeconvolution {
                    method: match d.algorithm {
                        DirectAlgorithm::RichardsonLucy => DirectMethod::RichardsonLucy {
                            iterations: d.iterations,
                        },
                        DirectAlgorithm::Wiener => DirectMethod::Wiener { noise: d.noise },
                    },
                    shift: d.shift,
                }
            }
        }
    }

    /// The operation that will be saved.
    pub fn operation(&self) -> Option<Operation> {
        self.operation_for(false)
    }

    /// The series as the computation reads them: the error column counts only for
    /// a weighted fit.
    pub fn effective_series(&self) -> Vec<SeriesRef> {
        let weighted = self.kind == OperationKind::Fit && self.setup.fit.weighted;
        self.series
            .iter()
            .map(|s| SeriesRef {
                sigma: s.sigma.filter(|_| weighted),
                ..s.clone()
            })
            .collect()
    }

    /// The series the preview computes: the shown series, or all of them for a
    /// global fit (the shared parameters link them).
    fn preview_series(&self) -> Vec<SeriesRef> {
        let all = self.effective_series();
        if self.is_global() {
            all
        } else {
            all.get(self.current).cloned().into_iter().collect()
        }
    }

    /// The preview computation: helper columns included. The series indices of the
    /// returned columns are those of [`Self::preview_rank`].
    pub fn preview(&self, tree: &Tree) -> Result<Computed, Issue> {
        if let Some(issue) = self.issues().into_iter().next() {
            return Err(issue);
        }
        let op = self.operation_for(true).ok_or(Issue::NoSeries)?;
        let series = self.preview_series();
        run(tree, &op, &series, self.aux.as_ref())
    }

    /// The rank, in `preview`, of the shown series.
    pub fn preview_rank(&self) -> usize {
        if self.is_global() {
            self.current
        } else {
            0
        }
    }

    /// A correction can replace ordinary columns only.
    pub fn replacement_disabled(&self, tree: &Tree) -> Option<String> {
        if self.editing.is_some()
            || !matches!(
                self.kind,
                OperationKind::Baseline | OperationKind::Correct | OperationKind::Smooth
            )
        {
            return Some(tr!("disabled-analysis-replace-operation"));
        }
        if self.series.iter().any(|s| {
            tree.calculation_of_column(s.y).is_some()
                || tree
                    .node(s.y)
                    .and_then(|n| n.as_column())
                    .is_none_or(|c| c.meta().formula.is_some())
        }) {
            return Some(tr!("disabled-analysis-replace-readonly"));
        }
        None
    }

    /// The batch to save: all the series computed, the requested columns with their
    /// long name (M6), the values if they are kept.
    ///
    /// When editing, a column that already exists keeps its metadata (long name,
    /// unit, comment that the user may have touched up).
    pub fn draft(&self, tree: &Tree) -> Result<CalculationDraft, Issue> {
        self.draft_with_values(tree).map(|(draft, _)| draft)
    }

    /// [`Self::draft`], and in addition **all** the computed values, kept or not:
    /// the low R² warning (#288) also judges a "Show" computation.
    pub fn draft_with_values(
        &self,
        tree: &Tree,
    ) -> Result<(CalculationDraft, Vec<stokesforge_core::SeriesResult>), Issue> {
        if let Some(issue) = self.issues().into_iter().next() {
            return Err(issue);
        }
        let operation = self.operation().ok_or(Issue::NoSeries)?;
        let series = self.effective_series();
        let computed = run(tree, &operation, &series, self.aux.as_ref())?;
        // The helper columns follow the operation; the two peak checkboxes are the
        // only ones to live in the form (the list is the core's).
        let peak_outputs = stokesforge_core::PeakOutputs {
            peak_columns: self.setup.peaks.peak_columns,
            sum_column: self.setup.peaks.sum_column,
        };
        let existing = self
            .editing
            .and_then(|id| tree.calculation(id))
            .map(|c| c.outputs.clone())
            .unwrap_or_default();
        let outputs = computed
            .columns
            .into_iter()
            .filter(|c| stokesforge_core::wants_output(&operation, &c.key, peak_outputs))
            .map(|c| {
                let meta = existing
                    .iter()
                    .find(|o| o.series == c.series && o.key == c.key && o.index == c.index)
                    .and_then(|o| o.column)
                    .and_then(|col| tree.node(col))
                    .and_then(|n| n.as_column())
                    .map(|col| col.meta().clone())
                    .unwrap_or_else(|| {
                        output_meta(tree, self.kind, &series[c.series], &c.key, c.index)
                    });
                OutputDraft {
                    series: c.series,
                    key: c.key,
                    index: c.index,
                    meta,
                    data: c.data,
                }
            })
            .collect();
        let all_values = computed.results.clone();
        let draft = CalculationDraft {
            operation,
            series,
            aux: self.aux.clone(),
            outputs,
            results: if self.keep_results || self.replace_original {
                computed.results
            } else {
                Vec::new()
            },
            keep_results: self.keep_results,
            auto_recalc: self.auto_recalc,
            low_r2_dialog: self.low_r2_dialog,
        };
        Ok((draft, all_values))
    }
}

fn run(
    tree: &Tree,
    op: &Operation,
    series: &[SeriesRef],
    aux: Option<&AuxRef>,
) -> Result<Computed, Issue> {
    compute(tree, op, series, aux).map_err(|e| match e {
        ComputeError::MissingInput(_) => Issue::MissingColumn,
        ComputeError::Analysis(e) => Issue::Failed(e),
    })
}

/// The short name of a source column ("B"), otherwise empty.
fn short_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .map(|n| n.name().to_string())
        .unwrap_or_default()
}

/// The long name of a source column, otherwise its short name.
fn long_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().long_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| short_name(tree, column))
}

/// The name of an output column, in the language of the `loader`. The rule
/// (which name, which pieces) is that of `stokesforge_core::OutputName`; this
/// `match` has **no** default branch: a variant added to the core does not
/// compile here until it has its `analysis-output-*` key.
/// `tests/output_names.rs` guards the agreement of English with `english()`.
pub fn output_name_text_in(loader: &FluentLanguageLoader, name: &OutputName) -> String {
    macro_rules! tr {
        ($($arguments:tt)*) => {
            crate::tr!(in loader; $($arguments)*)
        };
    }
    match name {
        OutputName::BaselineCorrected { name, column } => tr!(
            "analysis-output-baseline-corrected",
            name = name.as_str(),
            column = column.as_str()
        ),
        OutputName::Corrected { name } => {
            tr!("analysis-output-corrected", name = name.as_str())
        }
        OutputName::Baseline { column } => {
            tr!("analysis-output-baseline", column = column.as_str())
        }
        OutputName::Cumulative { column } => {
            tr!("analysis-output-cumulative", column = column.as_str())
        }
        OutputName::Normalized { column } => {
            tr!("analysis-output-normalized", column = column.as_str())
        }
        OutputName::Fit { name } => tr!("analysis-output-fit", name = name.as_str()),
        OutputName::Residuals { name } => {
            tr!("analysis-output-residuals", name = name.as_str())
        }
        OutputName::Peak { index, column } => tr!(
            "analysis-output-peak",
            index = i64::try_from(*index).unwrap_or(i64::MAX),
            column = column.as_str()
        ),
        OutputName::Background { column } => {
            tr!("analysis-output-background", column = column.as_str())
        }
        OutputName::PeaksSum { column } => {
            tr!("analysis-output-peaks-sum", column = column.as_str())
        }
        OutputName::Deconvolved { name } => {
            tr!("analysis-output-deconvolved", name = name.as_str())
        }
        OutputName::Smoothed { name } => {
            tr!("analysis-output-smoothed", name = name.as_str())
        }
        // The fallback of an unknown key is the source's name, already user text:
        // nothing to translate.
        OutputName::Source { name } => name.clone(),
    }
}

/// The name of an output column, in the current language.
pub fn output_name_text(name: &OutputName) -> String {
    output_name_text_in(crate::i18n::loader(), name)
}

/// The metadata of a produced column (M6): the long name says the output and
/// the source (`OutputName`, translated); the unit is **always** the source's
/// (`output_unit`, empty for an unknown key); the comment names the operation.
pub fn output_meta(
    tree: &Tree,
    kind: OperationKind,
    series: &SeriesRef,
    key: &str,
    index: Option<usize>,
) -> stokesforge_core::ColumnMeta {
    let name = OutputName::of(tree, kind, series, key, index);
    stokesforge_core::ColumnMeta {
        long_name: output_name_text(&name),
        unit: stokesforge_core::output_unit(tree, series, key),
        comment: kind_title(kind),
        ..stokesforge_core::ColumnMeta::default()
    }
}

/// The name of an operation family, in the current language.
pub fn kind_title(kind: OperationKind) -> String {
    match kind {
        OperationKind::Baseline => tr!("analysis-kind-baseline"),
        OperationKind::Correct => tr!("analysis-kind-correct"),
        OperationKind::Integrate => tr!("analysis-kind-integrate"),
        OperationKind::Fit => tr!("analysis-kind-fit"),
        OperationKind::Peaks => tr!("analysis-kind-peaks"),
        OperationKind::Reconvolution => tr!("analysis-kind-reconvolution"),
        OperationKind::DirectDeconvolution => tr!("analysis-kind-direct-deconvolution"),
        OperationKind::Smooth => tr!("analysis-kind-smooth"),
    }
}

/// The reference column of a correction, as the user reads it:
/// "copy of /[Book1]Sheet1!B" when the computation reads its frozen copy (#248),
/// otherwise the column's name.
pub fn reference_label(tree: &Tree, aux: &AuxRef) -> String {
    if tree.is_analysis_copy(aux.y) {
        let source = tree
            .node(aux.y)
            .and_then(|n| n.as_column())
            .map(|c| c.meta().long_name.clone())
            .unwrap_or_default();
        return tr!("analysis-reference-copy", name = source.as_str());
    }
    series_label(
        tree,
        &SeriesRef {
            x: aux.x,
            y: aux.y,
            sigma: None,
        },
    )
}

/// The preset that reopens an existing operation.
pub fn preset_of(op: &Operation) -> Preset {
    match op {
        Operation::Baseline { method, .. } => match method {
            BaselineMethod::Constant { .. } | BaselineMethod::ConstantZone { .. } => {
                Preset::BaselineConstant
            }
            BaselineMethod::Line { .. } => Preset::BaselineLine,
            BaselineMethod::Zones { .. } => Preset::BaselinePolynomial,
            BaselineMethod::Anchors { .. } => Preset::BaselineAnchors,
        },
        Operation::Correct { .. } => Preset::Correct,
        Operation::Integrate { .. } => Preset::Integrate,
        Operation::Fit { spec, .. } => Preset::Fit(spec.model.clone()),
        Operation::Peaks { .. } => Preset::Peaks,
        Operation::Reconvolution { .. } => Preset::Reconvolution,
        Operation::DirectDeconvolution { .. } => Preset::DirectDeconvolution,
        Operation::Smooth { spec } => match spec.method {
            SmoothMethod::AdjacentAverage => Preset::SmoothAdjacent,
            SmoothMethod::SavitzkyGolay { .. } => Preset::SmoothSavitzkyGolay,
            SmoothMethod::Median => Preset::SmoothMedian,
        },
    }
}

impl Setup {
    /// Takes over the settings of an existing operation (editing).
    pub fn load(&mut self, op: &Operation) {
        match op {
            Operation::Baseline {
                method,
                op,
                keep_baseline,
            } => {
                let b = &mut self.baseline;
                b.op = *op;
                b.baseline_column = *keep_baseline;
                match method {
                    BaselineMethod::Constant { point } => {
                        b.shape = BaselineShape::Constant;
                        b.constant_point = Some(*point);
                    }
                    BaselineMethod::ConstantZone { zone } => {
                        b.shape = BaselineShape::ConstantZone;
                        b.constant_zone = Some(*zone);
                    }
                    BaselineMethod::Line { p1, p2 } => {
                        b.shape = BaselineShape::TwoPoints;
                        b.points = vec![*p1, *p2];
                    }
                    BaselineMethod::Zones { zones, degree } => {
                        b.shape = BaselineShape::Zones;
                        b.zones = zones.clone();
                        b.degree = *degree;
                    }
                    BaselineMethod::Anchors {
                        points,
                        interpolation,
                    } => {
                        b.shape = BaselineShape::Anchors;
                        b.points = points.clone();
                        b.interpolation = *interpolation;
                    }
                }
            }
            Operation::Correct { op } => self.correct = *op,
            Operation::Smooth { spec } => {
                let s = &mut self.smooth;
                match spec.method {
                    SmoothMethod::AdjacentAverage => s.algorithm = SmoothAlgorithm::AdjacentAverage,
                    SmoothMethod::SavitzkyGolay { order } => {
                        s.algorithm = SmoothAlgorithm::SavitzkyGolay;
                        s.order = order;
                    }
                    SmoothMethod::Median => s.algorithm = SmoothAlgorithm::Median,
                }
                match spec.window {
                    SmoothWindow::Points(n) => {
                        s.unit = SmoothUnit::Points;
                        s.set_points(n);
                    }
                    SmoothWindow::XUnits(w) => {
                        s.unit = SmoothUnit::XUnits;
                        s.x_width = w;
                    }
                }
            }
            Operation::Integrate { spec } => {
                self.integrate = IntegrateSetup {
                    ranges: spec.ranges.clone(),
                    under: spec.under,
                    cumulative: spec.cumulative,
                    normalize: spec.normalize,
                };
            }
            Operation::Fit {
                spec,
                range,
                residuals,
            } => {
                self.fit.model = spec.model.clone();
                self.fit.params = spec.params.clone();
                self.fit.range = *range;
                self.fit.residuals = *residuals;
            }
            Operation::Peaks {
                spec,
                auto,
                range,
                residuals,
            } => {
                let p = &mut self.peaks;
                p.shape = spec.shape;
                p.background = spec.background;
                p.seeds = spec.seeds.clone();
                p.link_widths = spec.link_widths;
                p.center_window = spec.center_window;
                p.range = *range;
                p.residuals = *residuals;
                if let Some(a) = auto {
                    p.prominence_percent = a.min_prominence * 100.0;
                    p.max_peaks = a.max_peaks;
                }
            }
            Operation::Reconvolution { spec, residuals } => {
                self.reconv.spec = spec.clone();
                self.reconv.residuals = *residuals;
            }
            Operation::DirectDeconvolution { method, shift } => {
                self.direct.shift = *shift;
                match method {
                    DirectMethod::RichardsonLucy { iterations } => {
                        self.direct.algorithm = DirectAlgorithm::RichardsonLucy;
                        self.direct.iterations = *iterations;
                    }
                    DirectMethod::Wiener { noise } => {
                        self.direct.algorithm = DirectAlgorithm::Wiener;
                        self.direct.noise = *noise;
                    }
                }
            }
        }
    }
}

/// What `Operation::readiness` says about the operation, in the interface's
/// cases. An exhaustive `match` without `_`: a new variant of `Readiness` does
/// not compile without its case (and its sentence, `issue_message`).
fn issue_of(readiness: Readiness) -> Option<Issue> {
    match readiness {
        Readiness::Ready => None,
        Readiness::NeedsPoints { needed, have } => Some(Issue::NeedsPoints { needed, have }),
        Readiness::NeedsZone => Some(Issue::NeedsZone),
        Readiness::BadExpression(error) => Some(Issue::BadModel(Some(error))),
        Readiness::NoParameter => Some(Issue::BadModel(None)),
        Readiness::Invalid(error) => Some(Issue::Failed(error)),
    }
}

/// What is missing or what failed, as an interface sentence.
pub fn issue_message(issue: &Issue) -> String {
    match issue {
        Issue::NoSeries => tr!("analysis-issue-no-series"),
        Issue::NeedsReference => tr!("analysis-issue-needs-reference"),
        Issue::NeedsResponse => tr!("analysis-issue-needs-response"),
        Issue::NeedsPoints { needed, have } => tr!(
            "analysis-issue-needs-points",
            needed = i64::try_from(*needed).unwrap_or(i64::MAX),
            have = i64::try_from(*have).unwrap_or(i64::MAX)
        ),
        Issue::NeedsZone => tr!("analysis-issue-needs-zone"),
        Issue::BadModel(None) => tr!("analysis-issue-no-parameter"),
        Issue::BadModel(Some(error)) => tr!(
            "analysis-issue-bad-model",
            detail = crate::errors::analysis::expr_message(error)
        ),
        Issue::MissingColumn => tr!("analysis-issue-missing-column"),
        Issue::Failed(error) => crate::errors::analysis::error_message(error),
    }
}

impl ReconvSetup {
    /// The parameter names, in the order of the settings: background, one
    /// amplitude and one lifetime per component, then the shift.
    pub fn param_names(&self) -> Vec<String> {
        let mut names = vec!["bkg".to_string()];
        for i in 1..=self.spec.components {
            names.push(format!("A{i}"));
            names.push(format!("tau{i}"));
        }
        names.push("shift".to_string());
        names
    }
}

impl AnalysisForm {
    /// The tool most often used for the current family and shape: zones for ranges
    /// and background, points otherwise.
    pub fn natural_tool(&self) -> PreviewTool {
        match self.kind {
            OperationKind::Baseline
                if matches!(
                    self.setup.baseline.shape,
                    BaselineShape::Zones | BaselineShape::ConstantZone
                ) =>
            {
                PreviewTool::Zone
            }
            OperationKind::Integrate | OperationKind::Fit | OperationKind::Reconvolution => {
                PreviewTool::Zone
            }
            _ => PreviewTool::Points,
        }
    }

    /// The gestures among which the user chooses in the preview (#270, #299): the
    /// two of peak detection (a click sets a peak's position, a drag its search
    /// range), none for the other analyses, whose gesture is that of
    /// [`Self::natural_tool`] with nothing to show.
    pub fn tool_choices(&self) -> &'static [PreviewTool] {
        match self.kind {
            OperationKind::Peaks => &[PreviewTool::Points, PreviewTool::Zone],
            _ => &[],
        }
    }

    /// A click of the "points" tool at `(x, y)`: an anchor or line point, or the
    /// starting position of a peak.
    pub fn add_point(&mut self, x: f64, y: f64) {
        match self.kind {
            OperationKind::Baseline => {
                let b = &mut self.setup.baseline;
                match b.shape {
                    BaselineShape::Constant => b.constant_point = Some((x, y)),
                    BaselineShape::ConstantZone => {}
                    BaselineShape::TwoPoints => {
                        b.points.push((x, y));
                        if b.points.len() > 2 {
                            b.points.remove(0);
                        }
                    }
                    BaselineShape::Anchors => {
                        b.points.push((x, y));
                        b.points.sort_by(|p, q| p.0.total_cmp(&q.0));
                    }
                    BaselineShape::Zones => {}
                }
            }
            OperationKind::Peaks => {
                self.setup.peaks.seeds.push(PeakSeed {
                    center: x,
                    height: None,
                    fwhm: None,
                });
                self.setup
                    .peaks
                    .seeds
                    .sort_by(|p, q| p.center.total_cmp(&q.center));
            }
            _ => {}
        }
    }

    /// A zone drawn from `a` to `b`: background zone, integration range, fit range
    /// or reconvolution window.
    pub fn add_zone(&mut self, a: f64, b: f64) {
        let range = (a.min(b), a.max(b));
        if range.0 == range.1 {
            return;
        }
        match self.kind {
            OperationKind::Baseline
                if matches!(
                    self.setup.baseline.shape,
                    BaselineShape::Zones | BaselineShape::ConstantZone
                ) =>
            {
                if self.setup.baseline.shape == BaselineShape::ConstantZone {
                    self.setup.baseline.constant_zone = Some(range);
                } else {
                    self.setup.baseline.zones.push(range);
                }
            }
            OperationKind::Integrate => self.setup.integrate.ranges.push(range),
            OperationKind::Fit => self.setup.fit.range = Some(range),
            OperationKind::Peaks => self.setup.peaks.range = Some(range),
            OperationKind::Reconvolution => self.setup.reconv.spec.window = Some(range),
            _ => {}
        }
    }

    /// Removes what the right-click aims at in `x`: the point or peak within
    /// `tolerance`, or the zone that contains `x`. Returns true if something was
    /// removed.
    pub fn remove_near(&mut self, x: f64, tolerance: f64) -> bool {
        fn nearest(values: impl Iterator<Item = f64>, x: f64, tolerance: f64) -> Option<usize> {
            values
                .enumerate()
                .map(|(i, v)| (i, (v - x).abs()))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        }
        fn containing(ranges: &[(f64, f64)], x: f64) -> Option<usize> {
            ranges.iter().position(|(a, b)| *a <= x && x <= *b)
        }
        match self.kind {
            OperationKind::Baseline => {
                let b = &mut self.setup.baseline;
                if b.shape == BaselineShape::Constant {
                    if b.constant_point
                        .is_some_and(|(px, _)| (px - x).abs() <= tolerance)
                    {
                        b.constant_point = None;
                        true
                    } else {
                        false
                    }
                } else if b.shape == BaselineShape::ConstantZone {
                    if b.constant_zone.is_some_and(|(a, z)| a <= x && x <= z) {
                        b.constant_zone = None;
                        true
                    } else {
                        false
                    }
                } else if b.shape == BaselineShape::Zones {
                    containing(&b.zones, x).map(|i| b.zones.remove(i)).is_some()
                } else {
                    nearest(b.points.iter().map(|p| p.0), x, tolerance)
                        .map(|i| b.points.remove(i))
                        .is_some()
                }
            }
            OperationKind::Peaks => {
                let p = &mut self.setup.peaks;
                if let Some(i) = nearest(p.seeds.iter().map(|s| s.center), x, tolerance) {
                    p.seeds.remove(i);
                    true
                } else if p.range.is_some_and(|(a, b)| a <= x && x <= b) {
                    p.range = None;
                    true
                } else {
                    false
                }
            }
            OperationKind::Integrate => {
                let r = &mut self.setup.integrate.ranges;
                containing(r, x).map(|i| r.remove(i)).is_some()
            }
            OperationKind::Fit => {
                let hit = self.setup.fit.range.is_some_and(|(a, b)| a <= x && x <= b);
                if hit {
                    self.setup.fit.range = None;
                }
                hit
            }
            OperationKind::Reconvolution => {
                let hit = self
                    .setup
                    .reconv
                    .spec
                    .window
                    .is_some_and(|(a, b)| a <= x && x <= b);
                if hit {
                    self.setup.reconv.spec.window = None;
                }
                hit
            }
            _ => false,
        }
    }
}

/// A set of ranges of a computation: the one whose bound is being moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeSet {
    /// The background zones of a baseline.
    BaselineZones,
    /// The ranges of an integral.
    Integrate,
    /// The range of a fit.
    Fit,
    /// The range of a peak decomposition.
    Peaks,
    /// The window of a reconvolution.
    Window,
}

/// A bound of a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    Lo,
    Hi,
}

/// What a handle moves in a computation's settings (#199).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleSlot {
    /// A line or anchor point: X **and** Y move.
    Point(usize),
    /// The starting position of a peak: only X moves.
    Seed(usize),
    /// The bound of a range: only X moves.
    Edge(RangeSet, usize, Edge),
}

/// A handle placed on the figure: where it is, in data. `y` is `None` for
/// handles that carry only an abscissa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    pub slot: HandleSlot,
    pub x: f64,
    pub y: Option<f64>,
}

impl AnalysisForm {
    /// The handles the current settings expose on the graph (#199): anchor points,
    /// bounds of zones and ranges, peak positions.
    pub fn handles(&self) -> Vec<Handle> {
        let mut out = Vec::new();
        let mut edges = |set: RangeSet, ranges: &[(f64, f64)]| {
            for (i, (a, b)) in ranges.iter().enumerate() {
                out.push(Handle {
                    slot: HandleSlot::Edge(set, i, Edge::Lo),
                    x: *a,
                    y: None,
                });
                out.push(Handle {
                    slot: HandleSlot::Edge(set, i, Edge::Hi),
                    x: *b,
                    y: None,
                });
            }
        };
        let s = &self.setup;
        match self.kind {
            OperationKind::Baseline => {
                if s.baseline.shape == BaselineShape::ConstantZone {
                    edges(
                        RangeSet::BaselineZones,
                        &s.baseline.constant_zone.into_iter().collect::<Vec<_>>(),
                    );
                } else if s.baseline.shape == BaselineShape::Zones {
                    edges(RangeSet::BaselineZones, &s.baseline.zones);
                } else {
                    let points = if s.baseline.shape == BaselineShape::Constant {
                        s.baseline.constant_point.into_iter().collect()
                    } else {
                        s.baseline.points.clone()
                    };
                    for (i, (x, y)) in points.into_iter().enumerate() {
                        out.push(Handle {
                            slot: HandleSlot::Point(i),
                            x,
                            y: Some(y),
                        });
                    }
                }
            }
            OperationKind::Integrate => edges(RangeSet::Integrate, &s.integrate.ranges),
            OperationKind::Fit => {
                edges(RangeSet::Fit, &s.fit.range.into_iter().collect::<Vec<_>>())
            }
            OperationKind::Peaks => {
                edges(
                    RangeSet::Peaks,
                    &s.peaks.range.into_iter().collect::<Vec<_>>(),
                );
                for (i, seed) in s.peaks.seeds.iter().enumerate() {
                    out.push(Handle {
                        slot: HandleSlot::Seed(i),
                        x: seed.center,
                        y: None,
                    });
                }
            }
            OperationKind::Reconvolution => edges(
                RangeSet::Window,
                &s.reconv.spec.window.into_iter().collect::<Vec<_>>(),
            ),
            _ => {}
        }
        out
    }

    /// Moves the handle `slot` to `(x, y)`. A bound does not cross its twin (the
    /// range keeps at least one step); anchor points stay sorted. Returns false
    /// when nothing changed (handle gone, non-finite value, range reduced to a
    /// point).
    pub fn move_handle(&mut self, slot: HandleSlot, x: f64, y: f64) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        fn edge(range: &mut (f64, f64), which: Edge, x: f64) -> bool {
            let (lo, hi) = match which {
                Edge::Lo => (x, range.1),
                Edge::Hi => (range.0, x),
            };
            if lo == hi {
                return false;
            }
            let next = (lo.min(hi), lo.max(hi));
            let changed = next != *range;
            *range = next;
            changed
        }
        let s = &mut self.setup;
        match slot {
            HandleSlot::Point(i) => {
                let point = if s.baseline.shape == BaselineShape::Constant {
                    if i == 0 {
                        s.baseline.constant_point.as_mut()
                    } else {
                        None
                    }
                } else {
                    s.baseline.points.get_mut(i)
                };
                let Some(point) = point else {
                    return false;
                };
                let changed = *point != (x, y);
                *point = (x, y);
                if s.baseline.shape == BaselineShape::Anchors {
                    s.baseline.points.sort_by(|p, q| p.0.total_cmp(&q.0));
                }
                changed
            }
            HandleSlot::Seed(i) => {
                let Some(seed) = s.peaks.seeds.get_mut(i) else {
                    return false;
                };
                let changed = seed.center != x;
                seed.center = x;
                s.peaks.seeds.sort_by(|p, q| p.center.total_cmp(&q.center));
                changed
            }
            HandleSlot::Edge(set, i, which) => match set {
                RangeSet::BaselineZones => {
                    if s.baseline.shape == BaselineShape::ConstantZone {
                        i == 0
                            && s.baseline
                                .constant_zone
                                .as_mut()
                                .is_some_and(|r| edge(r, which, x))
                    } else {
                        s.baseline
                            .zones
                            .get_mut(i)
                            .is_some_and(|r| edge(r, which, x))
                    }
                }
                RangeSet::Integrate => s
                    .integrate
                    .ranges
                    .get_mut(i)
                    .is_some_and(|r| edge(r, which, x)),
                RangeSet::Fit => i == 0 && s.fit.range.as_mut().is_some_and(|r| edge(r, which, x)),
                RangeSet::Peaks => {
                    i == 0 && s.peaks.range.as_mut().is_some_and(|r| edge(r, which, x))
                }
                RangeSet::Window => {
                    i == 0
                        && s.reconv
                            .spec
                            .window
                            .as_mut()
                            .is_some_and(|r| edge(r, which, x))
                }
            },
        }
    }
}

/// The X and Y values of **each row** of a series, `NaN` for an empty or text
/// cell; without an X column, the row number starting at 1 (like the
/// computation).
pub fn series_rows(tree: &Tree, series: &SeriesRef) -> (Vec<f64>, Vec<f64>) {
    let numbers = |id: NodeId| -> Vec<f64> {
        tree.node(id)
            .and_then(|n| n.as_column())
            .map(|c| (0..c.len()).map(|r| c.values().number_at(r)).collect())
            .unwrap_or_default()
    };
    let ys = numbers(series.y);
    let mut xs = match series.x {
        Some(x) => numbers(x),
        None => (1..=ys.len()).map(|r| r as f64).collect(),
    };
    xs.resize(ys.len(), f64::NAN);
    (xs, ys)
}

/// The finite `(x, y)` points of a series, in row order.
pub fn series_points(tree: &Tree, series: &SeriesRef) -> (Vec<f64>, Vec<f64>) {
    let (xs, ys) = series_rows(tree, series);
    xs.into_iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .unzip()
}

/// The name of a series for the window: "Sheet / long name (B)".
pub fn series_label(tree: &Tree, series: &SeriesRef) -> String {
    let short = short_name(tree, series.y);
    let long = long_name(tree, series.y);
    if long == short {
        short
    } else {
        format!("{long} ({short})")
    }
}

/// The Y columns the window offers as reference or as instrument response:
/// those of the first series' workbook, excluding the series being processed
/// themselves. The name says the worksheet.
pub fn aux_candidates(tree: &Tree, series: &[SeriesRef]) -> Vec<(NodeId, String)> {
    let Some(first) = series.first() else {
        return Vec::new();
    };
    let Some(book) = tree
        .node(first.y)
        .and_then(|n| n.parent())
        .and_then(|sheet| tree.node(sheet))
        .and_then(|n| n.parent())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for sheet in tree.children(book).unwrap_or_default().iter().copied() {
        let sheet_name = short_name(tree, sheet);
        for column in tree.children(sheet).unwrap_or_default().iter().copied() {
            let is_y = tree
                .node(column)
                .and_then(|n| n.as_column())
                .is_some_and(|c| c.meta().role == stokesforge_core::ColumnRole::Y);
            if is_y && !series.iter().any(|s| s.y == column) {
                out.push((
                    column,
                    format!("{sheet_name} / {}", series_label_of(tree, column)),
                ));
            }
        }
    }
    out
}

/// The columns the "Y error" / "X error" fields of the series properties
/// offer for the series on `y` (#360): the other columns of its
/// worksheet, in sheet order, each with its label ("long name (B)"). The
/// columns of `current` (the ones the series already uses) stay offered even
/// when they sit in another worksheet, so the field can show them. Empty if
/// `y` is not a column of a worksheet.
pub fn error_candidates(
    tree: &Tree,
    y: NodeId,
    current: [Option<NodeId>; 2],
) -> Vec<(NodeId, String)> {
    let Some(sheet) = tree.node(y).and_then(|n| n.parent()) else {
        return Vec::new();
    };
    let is_column = |id: NodeId| tree.node(id).is_some_and(|n| n.as_column().is_some());
    let mut columns: Vec<NodeId> = tree
        .children(sheet)
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|column| *column != y && is_column(*column))
        .collect();
    for id in current.into_iter().flatten() {
        if !columns.contains(&id) && is_column(id) {
            columns.push(id);
        }
    }
    columns
        .into_iter()
        .map(|column| (column, series_label_of(tree, column)))
        .collect()
}

fn series_label_of(tree: &Tree, column: NodeId) -> String {
    series_label(
        tree,
        &SeriesRef {
            x: None,
            y: column,
            sigma: None,
        },
    )
}

/// The auxiliary column `y` with the nearest X to its left in its worksheet
/// (the pairing rule of `stokesforge_core::compose::x_for`).
pub fn aux_for(tree: &Tree, y: NodeId) -> AuxRef {
    AuxRef {
        x: stokesforge_core::compose::x_for(tree, y),
        y,
    }
}

/// The formula of a model, in Typst, for the fit window (#204). The parameter
/// names are those of the table (`A`, `tau`, `y0`...). `None` for the free
/// model: its formula is the one the user writes.
pub fn model_formula_source(model: &Model) -> Option<&'static str> {
    Some(match model {
        Model::Linear => r#"$ y = "intercept" + "slope" dot x $"#,
        Model::Polynomial { .. } => "$ y = c_0 + c_1 x + c_2 x^2 + dots.c + c_n x^n $",
        Model::Exponential => "$ y = A e^(-x/tau) $",
        Model::ExponentialOffset => "$ y = y_0 + A e^(-x/tau) $",
        Model::Exponential2 => "$ y = y_0 + A_1 e^(-x/tau_1) + A_2 e^(-x/tau_2) $",
        Model::Logarithm => "$ y = a + b ln x $",
        Model::Power => "$ y = a x^b $",
        Model::Gaussian => r#"$ y = y_0 + A exp(-4 ln 2 (x - x_c)^2 / "fwhm"^2) $"#,
        Model::Lorentzian => r#"$ y = y_0 + A / (1 + 4 (x - x_c)^2 / "fwhm"^2) $"#,
        Model::Custom { .. } => return None,
    })
}

/// The unit of a computed value: for an area, the product of the units of X
/// and Y (`nm·counts`), without conversion, written only if both are filled in
/// (#187, #204); empty for everything else.
pub fn value_unit(tree: &Tree, series: &SeriesRef, key: &str) -> String {
    if !matches!(key, "area" | "area.total" | "peak.area") {
        return String::new();
    }
    let unit_of = |id: Option<NodeId>| {
        id.and_then(|id| tree.node(id))
            .and_then(|n| n.as_column())
            .map(|c| c.meta().unit.trim().to_string())
            .unwrap_or_default()
    };
    let (x, y) = (unit_of(series.x), unit_of(Some(series.y)));
    if x.is_empty() || y.is_empty() {
        String::new()
    } else {
        format!("{x}·{y}")
    }
}

/// A value and its error, followed by their unit when it exists.
pub fn with_unit(text: String, unit: &str) -> String {
    if unit.is_empty() {
        text
    } else {
        format!("{text} {unit}")
    }
}

/// The label of a computed value (`area`, `param.tau`, `stat.r_squared`...),
/// with its rank for per-range or per-peak values. An unknown key is displayed
/// raw: nothing disappears.
///
pub fn value_label(key: &str, index: Option<usize>) -> String {
    let n = index.map_or(0, |i| i64::try_from(i).unwrap_or(i64::MAX));
    if let Some(name) = key.strip_prefix("param.") {
        return name.to_string();
    }
    match key {
        "area" => tr!("analysis-value-area", index = n),
        "area.from" => tr!("analysis-value-area-from", index = n),
        "area.to" => tr!("analysis-value-area-to", index = n),
        "area.total" => tr!("analysis-value-area-total"),
        "count.out_of_reference" => tr!("analysis-value-out-of-reference"),
        "count.window" => tr!("analysis-value-window"),
        "stat.points" => tr!("analysis-value-points"),
        "stat.r_squared" => tr!("analysis-value-r-squared"),
        "stat.reduced_chi2" => tr!("analysis-value-reduced-chi2"),
        "stat.dof" => tr!("analysis-value-dof"),
        "stat.tau_amplitude" => tr!("analysis-value-tau-amplitude"),
        "stat.tau_intensity" => tr!("analysis-value-tau-intensity"),
        "stat.noise_ratio" => tr!("analysis-value-noise-ratio"),
        "baseline.points" => tr!("analysis-value-baseline-points"),
        "baseline.slope" => tr!("analysis-value-baseline-slope"),
        "baseline.intercept" => tr!("analysis-value-baseline-intercept"),
        "peak.height" => tr!("analysis-value-peak-height", index = n),
        "peak.center" => tr!("analysis-value-peak-center", index = n),
        "peak.fwhm" => tr!("analysis-value-peak-fwhm", index = n),
        "peak.fwhm_gauss" => tr!("analysis-value-peak-fwhm-gauss", index = n),
        "peak.fwhm_lorentz" => tr!("analysis-value-peak-fwhm-lorentz", index = n),
        "peak.area" => tr!("analysis-value-peak-area", index = n),
        "peak.area_percent" => tr!("analysis-value-peak-area-percent", index = n),
        "background.y0" => tr!("analysis-value-background-y0"),
        "background.slope" => tr!("analysis-value-background-slope"),
        other => other.to_string(),
    }
}
