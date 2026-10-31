//! The latest analyses run (#249), listed at the bottom of the Analysis menu,
//! OriginLab-style: a method **and its settings**, without the columns or the
//! ranges. **No egui.**
//!
//! # What is kept, what is not
//!
//! An entry keeps the [`Operation`] of the calculation run, **without the
//! ranges and the starting values**: integration, fit and peak ranges, peak
//! starting positions, per-parameter settings of a fit or a reconvolution
//! (initial values, bounds, fixed or shared parameters), window of a
//! reconvolution — the rule is `Operation::forget_positions`
//! (`stokesforge-analysis`). What remains is the method, the degree, the
//! model, Subtract or Divide, the output boxes, "Replace original data" —
//! and the **definition of a baseline** (its points, its zones, its
//! anchors), without which Ctrl+click could never rerun a baseline on the
//! next spectrum, which is the intended gesture. The reference or response
//! column (`aux`) and the series are not settings: the rerun takes them from
//! the current selection (so a correction by a reference always opens the
//! window, which waits for its column).
//!
//! # Persistence
//!
//! The entries live in the application preferences (#148), never in the
//! `.stf`: they follow the user from one project to another. The operation
//! goes through the file as the generic tree of `stokesforge_analysis::tree`
//! converted to JSON (the `.stf`, for its part, writes it as MessagePack
//! through `tree_to_value`, `stokesforge-persistence`): a new setting does not
//! change the format. An unreadable entry is ignored, never an error
//! (invariant 10).

use stokesforge_analysis::baseline::BaselineMethod;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_analysis::tree::Tree as OpTree;

use crate::analysis::{preset_of, AnalysisForm, Preset};
use crate::tr;

/// The number of analyses kept (Origin keeps as many).
pub const MAX_RECENT: usize = 3;

/// A kept analysis: the method and its settings.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentAnalysis {
    /// The operation, without points, ranges or starting values.
    pub operation: Operation,
    /// "Replace original data" was chosen.
    pub replace_original: bool,
}

impl RecentAnalysis {
    /// What the form launches, or `None` when it launches nothing new
    /// (editing a calculation) or has no complete operation.
    pub fn from_form(form: &AnalysisForm) -> Option<Self> {
        if form.editing.is_some() {
            return None;
        }
        Some(Self {
            operation: forgotten(form.operation()?),
            replace_original: form.replace_original,
        })
    }

    /// The window's preselected method.
    pub fn preset(&self) -> Preset {
        preset_of(&self.operation)
    }

    /// Takes the settings into a new form opened on [`Self::preset`].
    /// "Replace original data" is not applied here: the window grants it only
    /// if the selection allows it (`replacement_disabled`).
    pub fn apply(&self, form: &mut AnalysisForm) {
        // An entry coming from the preferences may keep positions (hand-edited
        // file, old version): they are forgotten on reading too.
        form.setup.load(&forgotten(self.operation.clone()));
        form.tool = form.natural_tool();
    }

    /// The menu label: `Polynomial Baseline (degree 3)`.
    pub fn label(&self) -> String {
        let name = self.name();
        match self.detail() {
            Some(detail) => tr!("analysis-recent-detail", name = name, detail = detail),
            None => name,
        }
    }

    /// The tooltip: what the click and the Ctrl+click do.
    pub fn hint(&self) -> String {
        if self.replace_original {
            tr!("analysis-recent-hint-replace")
        } else {
            tr!("analysis-recent-hint")
        }
    }

    fn name(&self) -> String {
        match self.preset() {
            Preset::BaselineConstant => tr!("analysis-recent-baseline-constant"),
            Preset::BaselineLine => tr!("analysis-recent-baseline-linear"),
            Preset::BaselinePolynomial => tr!("analysis-recent-baseline-polynomial"),
            Preset::BaselineAnchors => tr!("analysis-recent-baseline-anchors"),
            Preset::Correct => tr!("analysis-recent-correct"),
            Preset::Integrate => tr!("analysis-recent-integrate"),
            Preset::Fit(model) => tr!("analysis-recent-fit", model = model_name(&model)),
            Preset::Peaks => tr!("analysis-recent-peaks"),
            Preset::Reconvolution => tr!("analysis-recent-reconvolution"),
            Preset::DirectDeconvolution => tr!("analysis-recent-direct"),
            Preset::SmoothAdjacent => tr!("analysis-recent-smooth-adjacent"),
            Preset::SmoothSavitzkyGolay => tr!("analysis-recent-smooth-savitzky-golay"),
            Preset::SmoothMedian => tr!("analysis-recent-smooth-median"),
        }
    }

    /// The setting that tells two entries of the same method apart.
    fn detail(&self) -> Option<String> {
        use stokesforge_analysis::baseline::BaselineOp;
        use stokesforge_analysis::correct::CorrectOp;
        use stokesforge_analysis::smooth::SmoothWindow;
        let op_word = |op: BaselineOp| match op {
            BaselineOp::Subtract => tr!("analysis-recent-subtract"),
            BaselineOp::Divide => tr!("analysis-recent-divide"),
        };
        match &self.operation {
            Operation::Baseline { method, op, .. } => Some(match method {
                BaselineMethod::Zones { degree, .. } => tr!(
                    "analysis-recent-degree",
                    degree = i64::from(*degree),
                    op = op_word(*op)
                ),
                _ => op_word(*op),
            }),
            Operation::Correct { op } => Some(match op {
                CorrectOp::Divide => tr!("analysis-recent-divide"),
                CorrectOp::Multiply => tr!("analysis-recent-multiply"),
                CorrectOp::Subtract => tr!("analysis-recent-subtract"),
            }),
            Operation::Smooth { spec } => Some(match spec.window {
                SmoothWindow::Points(points) => tr!(
                    "analysis-recent-window-points",
                    points = i64::try_from(points).unwrap_or(i64::MAX)
                ),
                SmoothWindow::XUnits(width) => tr!("analysis-recent-window-x", width = width),
            }),
            Operation::Fit { spec, .. } => match &spec.model {
                Model::Polynomial { degree } => Some(tr!(
                    "analysis-recent-degree-only",
                    degree = i64::from(*degree)
                )),
                Model::Custom { formula } => Some(formula.chars().take(24).collect()),
                _ => None,
            },
            _ => None,
        }
    }

    // ── JSON ───────────────────────────────────────────────────────────

    /// The entry as JSON, for the preferences file.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "operation": tree_to_json(&self.operation.to_tree()),
            "replace_original": self.replace_original,
        })
    }

    /// Reads an entry back; `None` if it is unreadable.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        let operation = Operation::from_tree(&json_to_tree(value.get("operation")?)).ok()?;
        Some(Self {
            operation,
            replace_original: value
                .get("replace_original")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// The name of a fit model in a recent entry.
fn model_name(model: &Model) -> String {
    match model {
        Model::Linear => tr!("analysis-fit-linear"),
        Model::Polynomial { .. } => tr!("analysis-fit-polynomial"),
        Model::Exponential => tr!("analysis-fit-exponential"),
        Model::ExponentialOffset => tr!("analysis-fit-exponential-offset"),
        Model::Exponential2 => tr!("analysis-fit-exponential2"),
        Model::Logarithm => tr!("analysis-fit-logarithm"),
        Model::Power => tr!("analysis-fit-power"),
        Model::Gaussian => tr!("analysis-fit-gaussian"),
        Model::Lorentzian => tr!("analysis-fit-lorentzian"),
        Model::Custom { .. } => tr!("analysis-fit-custom"),
    }
    .trim_end_matches('…')
    .to_string()
}

/// Removes from the operation everything that designates data (see the
/// top-level doc): the rule is `Operation::forget_positions`, in
/// `stokesforge-analysis`.
fn forgotten(mut op: Operation) -> Operation {
    op.forget_positions();
    op
}

/// Puts `new` at the head of `list`, without duplicates, `MAX_RECENT` at most
/// (#249): the same analysis takes only one place, moved to the head.
pub fn remember(list: &mut Vec<RecentAnalysis>, new: RecentAnalysis) {
    list.retain(|entry| entry != &new);
    list.insert(0, new);
    list.truncate(MAX_RECENT);
}

fn tree_to_json(tree: &OpTree) -> serde_json::Value {
    match tree {
        OpTree::Null => serde_json::Value::Null,
        OpTree::Bool(b) => (*b).into(),
        OpTree::Num(n) => serde_json::Number::from_f64(*n)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        OpTree::Text(t) => t.as_str().into(),
        OpTree::List(items) => items.iter().map(tree_to_json).collect(),
        OpTree::Map(entries) => entries
            .iter()
            .map(|(k, v)| (k.clone(), tree_to_json(v)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
    }
}

fn json_to_tree(value: &serde_json::Value) -> OpTree {
    match value {
        serde_json::Value::Null => OpTree::Null,
        serde_json::Value::Bool(b) => OpTree::Bool(*b),
        serde_json::Value::Number(n) => n.as_f64().map_or(OpTree::Null, OpTree::Num),
        serde_json::Value::String(s) => OpTree::Text(s.clone()),
        serde_json::Value::Array(items) => OpTree::List(items.iter().map(json_to_tree).collect()),
        serde_json::Value::Object(map) => OpTree::Map(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_tree(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{BaselineShape, Preset};
    use crate::i18n::{self, Language};
    use stokesforge_analysis::baseline::{BaselineOp, Interpolation};
    use stokesforge_analysis::fit::{FitSpec, ParamSetting};
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_core::SeriesRef;

    fn english() -> i18n::LanguageGuard {
        i18n::test_language(Language::English)
    }

    fn form(preset: &Preset) -> AnalysisForm {
        AnalysisForm::new(
            preset,
            vec![SeriesRef {
                x: None,
                y: stokesforge_core::NodeId::from_raw(1),
                sigma: None,
            }],
            false,
        )
    }

    fn recent(operation: Operation) -> RecentAnalysis {
        RecentAnalysis {
            operation,
            replace_original: false,
        }
    }

    #[test]
    fn the_label_names_the_method_and_its_setting() {
        let _language = english();
        let polynomial = recent(Operation::Baseline {
            method: BaselineMethod::Zones {
                zones: vec![(0.0, 1.0)],
                degree: 3,
            },
            op: BaselineOp::Subtract,
            keep_baseline: false,
        });
        assert_eq!(
            polynomial.label(),
            "Polynomial Baseline (degree 3, Subtract)"
        );
        let divide = recent(Operation::Baseline {
            method: BaselineMethod::Line {
                p1: (0.0, 0.0),
                p2: (1.0, 1.0),
            },
            op: BaselineOp::Divide,
            keep_baseline: false,
        });
        assert_eq!(divide.label(), "Linear Baseline (Divide)");
        let fit = recent(Operation::Fit {
            spec: FitSpec {
                model: Model::Polynomial { degree: 4 },
                params: Vec::new(),
            },
            range: None,
            residuals: false,
        });
        assert_eq!(fit.label(), "Polynomial Fit (degree 4)");
        let gaussian = recent(Operation::Fit {
            spec: FitSpec {
                model: Model::Gaussian,
                params: Vec::new(),
            },
            range: None,
            residuals: false,
        });
        assert_eq!(gaussian.label(), "Gaussian Fit");
    }

    #[test]
    fn ranges_and_starting_values_are_not_kept() {
        let mut f = form(&Preset::Integrate);
        f.setup.integrate.ranges = vec![(1.0, 2.0)];
        f.setup.integrate.cumulative = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let Operation::Integrate { spec } = &kept.operation else {
            panic!("an integral");
        };
        assert!(spec.ranges.is_empty(), "ranges forgotten");
        assert!(spec.cumulative, "witness: the setting stays");

        let mut f = form(&Preset::Fit(Model::Gaussian));
        f.setup.fit.range = Some((1.0, 2.0));
        f.setup.fit.params[1].initial = Some(5.0);
        f.setup.fit.params[2].fixed = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let Operation::Fit { spec, range, .. } = &kept.operation else {
            panic!("a fit");
        };
        assert_eq!(*range, None);
        assert!(spec.params.iter().all(|p| p.initial.is_none() && !p.fixed));
    }

    /// #260: an entry read from the preferences may keep positions (hand-edited
    /// file): reopening forgets them, `fixed` and `shared` included, through
    /// the single rule `Operation::forget_positions`.
    #[test]
    fn reopening_an_entry_also_forgets_fixed_and_shared_settings() {
        let setting = ParamSetting {
            initial: Some(3.0),
            fixed: true,
            min: Some(0.0),
            max: Some(9.0),
            shared: true,
        };
        let entry = recent(Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![setting.clone(), setting],
            },
            range: Some((1.0, 2.0)),
            residuals: true,
        });
        let mut reopened = form(&entry.preset());
        entry.apply(&mut reopened);
        assert_eq!(reopened.setup.fit.range, None);
        assert_eq!(reopened.setup.fit.params, vec![ParamSetting::default(); 2]);
        assert!(!reopened.setup.fit.is_global(), "no shared parameter left");
        assert!(reopened.setup.fit.residuals, "witness: the setting stays");
    }

    /// #260: a huge degree read from the preferences builds no parameter —
    /// neither on reopening, nor in the form's prerequisites.
    #[test]
    fn a_huge_degree_from_the_preferences_is_rejected_without_allocation() {
        let entry = recent(Operation::Fit {
            spec: FitSpec {
                model: Model::Polynomial { degree: u32::MAX },
                params: vec![ParamSetting::default()],
            },
            range: None,
            residuals: false,
        });
        let mut reopened = form(&entry.preset());
        entry.apply(&mut reopened);
        assert!(matches!(
            reopened.issues().as_slice(),
            [crate::analysis::Issue::Failed(_)]
        ));
        assert_eq!(reopened.setup.fit.param_count(), 0);
    }

    #[test]
    fn the_definition_of_a_baseline_is_kept() {
        let mut f = form(&Preset::BaselineAnchors);
        f.setup.baseline.points = vec![(0.0, 1.0), (10.0, 2.0)];
        f.setup.baseline.op = BaselineOp::Divide;
        f.setup.baseline.interpolation = Interpolation::Segments;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let mut reopened = form(&kept.preset());
        kept.apply(&mut reopened);
        assert_eq!(reopened.setup.baseline.shape, BaselineShape::Anchors);
        assert_eq!(reopened.setup.baseline.op, BaselineOp::Divide);
        assert_eq!(reopened.setup.baseline.points.len(), 2);
        assert!(reopened.issues().is_empty(), "ready to be rerun");
    }

    #[test]
    fn editing_a_calculation_keeps_nothing() {
        let mut f = form(&Preset::Integrate);
        f.editing = Some(stokesforge_core::NodeId::from_raw(7));
        assert!(RecentAnalysis::from_form(&f).is_none());
    }

    #[test]
    fn the_same_analysis_takes_one_place_moved_to_the_head_and_three_at_most() {
        let integrate = |cumulative| {
            recent(Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: Vec::new(),
                    under: Under::Zero,
                    cumulative,
                    normalize: false,
                },
            })
        };
        let line = recent(Operation::Correct {
            op: stokesforge_analysis::correct::CorrectOp::Divide,
        });
        let mut list = Vec::new();
        remember(&mut list, integrate(false));
        remember(&mut list, line.clone());
        remember(&mut list, integrate(false));
        assert_eq!(
            list,
            vec![integrate(false), line.clone()],
            "duplicate moved up"
        );
        let subtract = recent(Operation::Correct {
            op: stokesforge_analysis::correct::CorrectOp::Subtract,
        });
        remember(&mut list, integrate(true));
        remember(&mut list, subtract.clone());
        assert_eq!(
            list,
            vec![subtract, integrate(true), integrate(false)],
            "three at most: the oldest (the correction) goes"
        );
    }

    #[test]
    fn json_round_trips_and_an_unreadable_entry_is_discarded() {
        let mut f = form(&Preset::Fit(Model::Polynomial { degree: 3 }));
        f.replace_original = false;
        f.setup.fit.residuals = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let json = kept.to_json();
        assert_eq!(RecentAnalysis::from_json(&json), Some(kept));
        assert_eq!(
            RecentAnalysis::from_json(&serde_json::json!({ "operation": { "kind": "unknown" } })),
            None
        );
        assert_eq!(RecentAnalysis::from_json(&serde_json::json!(42)), None);
    }

    /// An entry whose operation carries a non-finite number (#278): written
    /// as JSON (`null`), it is never read back as a non-finite operation —
    /// either unreadable (`None`, the pair lost), or read back valid (the
    /// optional field becomes absent). Finite values do round-trip.
    #[test]
    fn an_entry_with_a_non_finite_number_is_never_read_back_non_finite() {
        use stokesforge_analysis::baseline::BaselineMethod;
        use stokesforge_analysis::deconv::DirectMethod;
        use stokesforge_analysis::model::Model;
        use stokesforge_analysis::ops::AutoDetect;
        use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
        let fit = |range| Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: Vec::new(),
            },
            range,
            residuals: false,
        };
        let peaks = |seeds, prominence| Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Gaussian,
                background: PeakBackground::Constant,
                seeds,
                link_widths: false,
                center_window: None,
            },
            auto: Some(AutoDetect {
                min_prominence: prominence,
                max_peaks: 3,
            }),
            range: None,
            residuals: false,
        };
        let seed = |center, height| PeakSeed {
            center,
            height,
            fwhm: None,
        };
        let baseline = |method| Operation::Baseline {
            method,
            op: BaselineOp::Subtract,
            keep_baseline: false,
        };
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            // Pairs: reading fails, the entry is discarded.
            for operation in [
                baseline(BaselineMethod::Constant { point: (bad, 1.0) }),
                baseline(BaselineMethod::Line {
                    p1: (0.0, 1.0),
                    p2: (2.0, bad),
                }),
                fit(Some((0.0, bad))),
                peaks(vec![seed(bad, None)], 0.1),
            ] {
                let json = recent(operation.clone()).to_json();
                assert_eq!(
                    RecentAnalysis::from_json(&json),
                    None,
                    "{operation:?} → {json}"
                );
            }
            // Optional settings: read back absent, but never non-finite.
            for operation in [
                peaks(vec![seed(1.0, Some(bad))], 0.1),
                peaks(Vec::new(), bad),
                Operation::DirectDeconvolution {
                    method: DirectMethod::RichardsonLucy { iterations: 5 },
                    shift: bad,
                },
            ] {
                let back = RecentAnalysis::from_json(&recent(operation.clone()).to_json());
                if let Some(entry) = back {
                    assert_eq!(entry.operation.validate(), Ok(()), "{operation:?}");
                }
            }
        }
        // Finite values round-trip exactly.
        for operation in [
            baseline(BaselineMethod::Constant { point: (0.5, -1.0) }),
            fit(Some((0.0, 2.0))),
            peaks(vec![seed(1.0, Some(2.0))], 0.1),
        ] {
            let entry = recent(operation);
            assert_eq!(
                RecentAnalysis::from_json(&entry.to_json()),
                Some(entry.clone()),
                "{entry:?}"
            );
        }
    }
}
