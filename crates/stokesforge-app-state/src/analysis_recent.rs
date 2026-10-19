//! Les dernières analyses lancées (#249), listées en bas du menu Analysis,
//! façon OriginLab : une méthode **et ses réglages**, sans les colonnes ni les
//! plages. **Sans egui.**
//!
//! # Ce qui est retenu, ce qui ne l'est pas
//!
//! Une entrée garde l'[`Operation`] du calcul lancé, **sans les plages et les
//! valeurs de départ** : plages d'intégration, d'ajustement et de pics,
//! positions de départ des pics, réglages par paramètre d'un ajustement ou d'une
//! reconvolution (valeurs initiales, bornes, paramètres fixés ou partagés),
//! fenêtre d'une reconvolution — la règle est `Operation::forget_positions`
//! (`stokesforge-analysis`). Il reste la méthode, le degré, le
//! modèle, Subtract ou Divide, les cases de sortie, « Replace original data » —
//! et la **définition d'une ligne de base** (ses points, ses zones, ses
//! ancrages), faute de quoi Ctrl+clic ne pourrait jamais relancer une ligne de
//! base sur le spectre suivant, qui est le geste visé. La colonne de référence
//! ou de réponse (`aux`) et les séries ne sont pas des réglages : la relance les
//! reprend de la sélection courante (donc une correction par une référence
//! ouvre toujours la fenêtre, qui attend sa colonne).
//!
//! # Persistance
//!
//! Les entrées vivent dans les préférences de l'application (#148), jamais dans
//! le `.stf` : elles suivent l'utilisateur d'un projet à l'autre. L'opération
//! traverse le fichier sous la forme de l'arbre générique de
//! `stokesforge_analysis::tree` converti en JSON (le `.stf`, lui, l'écrit en
//! MessagePack par `tree_to_value`, `stokesforge-persistence`) : un réglage
//! nouveau ne change pas le format. Une entrée illisible est ignorée,
//! jamais une erreur (invariant 10).

use stokesforge_analysis::baseline::BaselineMethod;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_analysis::tree::Tree as OpTree;

use crate::analysis::{preset_of, AnalysisForm, Preset};
use crate::tr;

/// Le nombre d'analyses retenues (Origin en garde autant).
pub const MAX_RECENT: usize = 3;

/// Une analyse retenue : la méthode et ses réglages.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentAnalysis {
    /// L'opération, sans point, plage ni valeur de départ.
    pub operation: Operation,
    /// « Replace original data » était choisi.
    pub replace_original: bool,
}

impl RecentAnalysis {
    /// Ce que le formulaire lance, ou `None` quand il ne lance rien de neuf
    /// (modification d'un calcul) ou n'a pas d'opération complète.
    pub fn from_form(form: &AnalysisForm) -> Option<Self> {
        if form.editing.is_some() {
            return None;
        }
        Some(Self {
            operation: forgotten(form.operation()?),
            replace_original: form.replace_original,
        })
    }

    /// La méthode préchoisie de la fenêtre.
    pub fn preset(&self) -> Preset {
        preset_of(&self.operation)
    }

    /// Reprend les réglages dans un formulaire neuf ouvert sur [`Self::preset`].
    /// « Replace original data » n'est pas appliqué ici : la fenêtre ne
    /// l'accorde que si la sélection le permet (`replacement_disabled`).
    pub fn apply(&self, form: &mut AnalysisForm) {
        // Une entrée venue des préférences peut garder des positions (fichier
        // édité à la main, ancienne version) : on les oublie à la lecture aussi.
        form.setup.load(&forgotten(self.operation.clone()));
        form.tool = form.natural_tool();
    }

    /// Le libellé du menu : `Polynomial Baseline (degree 3)`.
    pub fn label(&self) -> String {
        let name = self.name();
        match self.detail() {
            Some(detail) => tr!("analysis-recent-detail", name = name, detail = detail),
            None => name,
        }
    }

    /// L'info-bulle : ce que le clic et le Ctrl+clic font.
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
        }
    }

    /// Le réglage qui distingue deux entrées de la même méthode.
    fn detail(&self) -> Option<String> {
        use stokesforge_analysis::baseline::BaselineOp;
        use stokesforge_analysis::correct::CorrectOp;
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

    /// L'entrée sous forme JSON, pour le fichier de préférences.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "operation": tree_to_json(&self.operation.to_tree()),
            "replace_original": self.replace_original,
        })
    }

    /// Relit une entrée ; `None` si elle est illisible.
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

/// Le nom d'un modèle d'ajustement dans une entrée récente.
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

/// Retire de l'opération tout ce qui désigne des données (voir la doc de tête) :
/// la règle est `Operation::forget_positions`, dans `stokesforge-analysis`.
fn forgotten(mut op: Operation) -> Operation {
    op.forget_positions();
    op
}

/// Range `new` en tête de `list`, sans doublon, `MAX_RECENT` au plus (#249) :
/// une même analyse n'occupe qu'une place, remontée en tête.
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

    fn english() {
        i18n::set_language(Language::English);
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
    fn le_libelle_nomme_la_methode_et_son_reglage() {
        english();
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
    fn les_plages_et_les_valeurs_de_depart_ne_sont_pas_retenues() {
        let mut f = form(&Preset::Integrate);
        f.setup.integrate.ranges = vec![(1.0, 2.0)];
        f.setup.integrate.cumulative = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let Operation::Integrate { spec } = &kept.operation else {
            panic!("une intégrale");
        };
        assert!(spec.ranges.is_empty(), "plages oubliées");
        assert!(spec.cumulative, "témoin : le réglage reste");

        let mut f = form(&Preset::Fit(Model::Gaussian));
        f.setup.fit.range = Some((1.0, 2.0));
        f.setup.fit.params[1].initial = Some(5.0);
        f.setup.fit.params[2].fixed = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let Operation::Fit { spec, range, .. } = &kept.operation else {
            panic!("un ajustement");
        };
        assert_eq!(*range, None);
        assert!(spec.params.iter().all(|p| p.initial.is_none() && !p.fixed));
    }

    /// #260 : une entrée lue des préférences peut garder des positions (fichier
    /// édité à la main) : la réouverture les oublie, `fixed` et `shared`
    /// compris, par la règle unique `Operation::forget_positions`.
    #[test]
    fn rouvrir_une_entree_oublie_aussi_les_reglages_fixes_et_partages() {
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
        assert!(!reopened.setup.fit.is_global(), "plus de paramètre partagé");
        assert!(reopened.setup.fit.residuals, "témoin : le réglage reste");
    }

    /// #260 : un degré énorme lu des préférences ne fabrique aucun paramètre —
    /// ni à la réouverture, ni dans les prérequis du formulaire.
    #[test]
    fn un_degre_enorme_des_preferences_est_refuse_sans_allocation() {
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
    fn la_definition_d_une_ligne_de_base_est_retenue() {
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
        assert!(reopened.issues().is_empty(), "prête à être relancée");
    }

    #[test]
    fn modifier_un_calcul_ne_retient_rien() {
        let mut f = form(&Preset::Integrate);
        f.editing = Some(stokesforge_core::NodeId::from_raw(7));
        assert!(RecentAnalysis::from_form(&f).is_none());
    }

    #[test]
    fn une_meme_analyse_n_occupe_qu_une_place_remontee_en_tete_et_trois_au_plus() {
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
            "doublon remonté"
        );
        let subtract = recent(Operation::Correct {
            op: stokesforge_analysis::correct::CorrectOp::Subtract,
        });
        remember(&mut list, integrate(true));
        remember(&mut list, subtract.clone());
        assert_eq!(
            list,
            vec![subtract, integrate(true), integrate(false)],
            "trois au plus : la plus ancienne (la correction) part"
        );
    }

    #[test]
    fn le_json_fait_l_aller_retour_et_une_entree_illisible_est_ecartee() {
        let mut f = form(&Preset::Fit(Model::Polynomial { degree: 3 }));
        f.replace_original = false;
        f.setup.fit.residuals = true;
        let kept = RecentAnalysis::from_form(&f).unwrap();
        let json = kept.to_json();
        assert_eq!(RecentAnalysis::from_json(&json), Some(kept));
        assert_eq!(
            RecentAnalysis::from_json(&serde_json::json!({ "operation": { "kind": "inconnu" } })),
            None
        );
        assert_eq!(RecentAnalysis::from_json(&serde_json::json!(42)), None);
    }

    /// Une entrée dont l'opération porte un nombre non fini (#278) : écrite en
    /// JSON (`null`), elle ne se relit jamais en opération non finie — soit
    /// illisible (`None`, la paire perdue), soit relue valide (le champ
    /// facultatif devient absent). Les valeurs finies, elles, font l'aller-retour.
    #[test]
    fn une_entree_a_nombre_non_fini_ne_se_relit_jamais_non_finie() {
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
            // Paires : la lecture échoue, l'entrée est écartée.
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
            // Réglages facultatifs : relus absents, mais jamais non finis.
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
        // Le fini fait l'aller-retour exact.
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
