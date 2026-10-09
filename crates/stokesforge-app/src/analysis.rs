//! Le modèle de la fenêtre d'analyse (#181) : ce que l'utilisateur règle pour
//! lancer un calcul (ligne de base, correction, intégrale, ajustement, pics,
//! déconvolution), **sans egui**.
//!
//! La fenêtre (`app/analysis_dialog.rs`) ne fait que peindre ce formulaire et
//! pousser des [`Action`](crate::state::Action) ; tout ce qui se décide — les
//! valeurs par défaut d'une opération, ce qui manque pour valider, l'opération
//! que les réglages décrivent, les colonnes produites et leurs noms — se lit
//! ici et se teste sans fenêtre (`tests/analysis.rs`).
//!
//! # Un réglage par famille, tous gardés
//!
//! Le formulaire garde les réglages de **chaque** famille en même temps
//! ([`Setup`]) : passer de « Reconvolution » à « Direct Deconvolution », ou d'une
//! ligne de base par deux points à des zones, ne perd rien de ce qui avait été
//! posé. [`AnalysisForm::operation`] ne lit que la famille active. Une
//! [`Operation`] n'est construite qu'une fois les réglages complets : une ligne
//! de base à un point n'est pas une opération, c'est un [`Issue`].
//!
//! # Aperçu et validation
//!
//! [`AnalysisForm::preview`] calcule la série affichée (toutes les séries pour
//! un ajustement global) avec les colonnes d'aide forcées — la ligne de base et
//! les résidus se montrent même si la colonne ne sera pas écrite.
//! [`AnalysisForm::draft`] calcule le lot entier, ne garde que les colonnes
//! demandées et leur donne un nom long (M6) ; c'est lui que la commande
//! `AddCalculation` / `UpdateCalculation` reçoit.

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp, Interpolation};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
use stokesforge_analysis::fit::{FitSpec, ParamSetting};
use stokesforge_analysis::integrate::{IntegrateSpec, Under};
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{AutoDetect, Operation, OperationKind, HEAVY_BATCH};
use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
use stokesforge_analysis::AnalysisError;
use stokesforge_core::{
    compute, AuxRef, Calculation, CalculationDraft, ComputeError, Computed, NodeId, OutputDraft,
    SeriesRef, Tree,
};

use crate::tr;

/// Nombre de points moyennés par défaut quand un point est aimanté sur la
/// courbe (M4 b).
pub const DEFAULT_AVERAGE: usize = 5;

/// Bornes du réglage « moyenne sur N points » (molette).
pub const AVERAGE_RANGE: std::ops::RangeInclusive<usize> = 1..=101;

/// Une entrée du menu Analysis : la famille **et** la méthode préchoisie
/// (M1 B). La fenêtre sait changer de méthode, pour « Edit Calculation… ».
#[derive(Debug, Clone, PartialEq)]
pub enum Preset {
    /// Ligne de base par deux points.
    BaselineLine,
    /// Ligne de base polynomiale sur des zones.
    BaselinePolynomial,
    /// Ligne de base par points d'ancrage.
    BaselineAnchors,
    /// Correction par une colonne de référence.
    Correct,
    /// Intégrale sur une ou plusieurs plages.
    Integrate,
    /// Ajustement, le modèle préchoisi.
    Fit(Model),
    /// Décomposition en pics.
    Peaks,
    /// Reconvolution par une réponse instrumentale.
    Reconvolution,
    /// Déconvolution directe.
    DirectDeconvolution,
}

impl Preset {
    /// La famille d'opération que ce choix lance.
    pub fn kind(&self) -> OperationKind {
        match self {
            Preset::BaselineLine | Preset::BaselinePolynomial | Preset::BaselineAnchors => {
                OperationKind::Baseline
            }
            Preset::Correct => OperationKind::Correct,
            Preset::Integrate => OperationKind::Integrate,
            Preset::Fit(_) => OperationKind::Fit,
            Preset::Peaks => OperationKind::Peaks,
            Preset::Reconvolution => OperationKind::Reconvolution,
            Preset::DirectDeconvolution => OperationKind::DirectDeconvolution,
        }
    }

    /// Les modèles proposés par `Fit ▸` (le modèle libre se saisit dans la
    /// fenêtre, il n'a pas d'entrée de menu qui le préremplisse).
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

/// La forme d'une ligne de base (M10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineShape {
    /// Une droite par deux points posés.
    TwoPoints,
    /// Un polynôme ajusté sur des zones de fond.
    Zones,
    /// Une courbe qui passe par des points d'ancrage.
    Anchors,
}

/// Réglages de la ligne de base.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineSetup {
    pub shape: BaselineShape,
    /// Les deux points de la droite, ou les points d'ancrage.
    pub points: Vec<(f64, f64)>,
    /// Les zones de fond.
    pub zones: Vec<(f64, f64)>,
    /// Degré du polynôme des zones (1 : droite).
    pub degree: u32,
    pub interpolation: Interpolation,
    pub op: BaselineOp,
    /// Écrire aussi la ligne de base dans une colonne.
    pub baseline_column: bool,
}

impl Default for BaselineSetup {
    fn default() -> Self {
        Self {
            shape: BaselineShape::Anchors,
            points: Vec::new(),
            zones: Vec::new(),
            degree: 1,
            interpolation: Interpolation::Segments,
            op: BaselineOp::Subtract,
            baseline_column: false,
        }
    }
}

/// Réglages de l'intégrale.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrateSetup {
    /// Plages `(de, à)` ; vide : la série entière.
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

/// Réglages de l'ajustement.
#[derive(Debug, Clone, PartialEq)]
pub struct FitSetup {
    pub model: Model,
    /// Un réglage par paramètre du modèle (voir [`FitSetup::reset_params`]).
    pub params: Vec<ParamSetting>,
    /// Plage de X ajustée ; `None` : toute la série.
    pub range: Option<(f64, f64)>,
    pub residuals: bool,
    /// Pondérer par la colonne d'erreur de chaque série (`1/σ`).
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
    /// Le nombre de paramètres du modèle courant (0 si un modèle libre ne se
    /// compile pas encore).
    pub fn param_count(&self) -> usize {
        self.model.compile().map_or(0, |m| m.param_count())
    }

    /// Les noms des paramètres du modèle courant.
    pub fn param_names(&self) -> Vec<String> {
        self.model.compile().map_or_else(
            |_| Vec::new(),
            |m| m.params().iter().map(|p| p.name.clone()).collect(),
        )
    }

    /// Remet les réglages par paramètre à leur défaut, à la bonne longueur
    /// (changement de modèle, de degré ou d'expression).
    pub fn reset_params(&mut self) {
        self.params = vec![ParamSetting::default(); self.param_count()];
    }

    /// Remplit les valeurs de départ avec l'estimation automatique du modèle
    /// sur les points `(x, y)` donnés, restreints à la plage ajustée (bouton
    /// « Auto », #204). Sans effet si le modèle ne se compile pas.
    pub fn auto_initial(&mut self, x: &[f64], y: &[f64]) {
        let Ok(compiled) = self.model.compile() else {
            return;
        };
        let (lo, hi) = self
            .range
            .map_or((f64::NEG_INFINITY, f64::INFINITY), |(a, b)| {
                (a.min(b), a.max(b))
            });
        let (xs, ys): (Vec<f64>, Vec<f64>) = x
            .iter()
            .zip(y)
            .filter(|(x, _)| **x >= lo && **x <= hi)
            .map(|(x, y)| (*x, *y))
            .unzip();
        let guess = compiled.guess(&stokesforge_analysis::series::Series::new(xs, ys));
        self.params
            .resize(compiled.param_count(), ParamSetting::default());
        for (setting, value) in self.params.iter_mut().zip(guess) {
            if !setting.fixed {
                setting.initial = Some(value);
            }
        }
    }

    /// Le modèle change : réglages remis à zéro.
    pub fn set_model(&mut self, model: Model) {
        self.model = model;
        self.reset_params();
    }

    /// Au moins un paramètre est partagé : ajustement global.
    pub fn is_global(&self) -> bool {
        self.params.iter().any(|p| p.shared)
    }
}

/// Réglages de la décomposition en pics.
#[derive(Debug, Clone, PartialEq)]
pub struct PeaksSetup {
    pub shape: PeakShape,
    pub background: PeakBackground,
    /// Positions de départ posées à la main ; vides, la détection les trouve.
    pub seeds: Vec<PeakSeed>,
    pub link_widths: bool,
    /// Demi-largeur (en X) dans laquelle chaque position reste bornée autour de
    /// son point de départ ; `None` : positions libres (#204).
    pub center_window: Option<f64>,
    /// Proéminence minimale pour la détection, en % de l'étendue de Y.
    pub prominence_percent: f64,
    pub max_peaks: usize,
    pub range: Option<(f64, f64)>,
    pub residuals: bool,
    /// Une colonne par pic.
    pub peak_columns: bool,
    /// Une colonne pour la somme des pics (fond compris).
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

/// Réglages de la reconvolution.
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
    /// Nombre de paramètres : fond, une amplitude et une durée de vie par
    /// composante, puis le décalage.
    pub fn param_count(&self) -> usize {
        2 + 2 * self.spec.components
    }

    /// Ajuste la longueur de la liste des réglages au nombre de composantes.
    /// Le décalage est le **dernier** paramètre : on le met de côté, on
    /// redimensionne les composantes, puis on le remet en queue — sinon ses
    /// réglages (départ, fixé, bornes) glisseraient sur la composante ajoutée.
    pub fn resize_params(&mut self) {
        let count = self.param_count();
        let shift = self.spec.params.pop().unwrap_or_default();
        self.spec.params.resize(count - 1, ParamSetting::default());
        self.spec.params.push(shift);
    }
}

/// L'algorithme de la déconvolution directe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectAlgorithm {
    RichardsonLucy,
    Wiener,
}

/// Réglages de la déconvolution directe.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectSetup {
    pub algorithm: DirectAlgorithm,
    pub iterations: usize,
    /// Rapport bruit/signal de Wiener ; `None` : estimé.
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

/// Les réglages de toutes les familles, gardés ensemble.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub baseline: BaselineSetup,
    pub correct: CorrectOp,
    pub integrate: IntegrateSetup,
    pub fit: FitSetup,
    pub peaks: PeaksSetup,
    pub reconv: ReconvSetup,
    pub direct: DirectSetup,
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
        }
    }
}

/// L'outil actif de l'aperçu (M4 a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTool {
    /// Un clic pose un point (ancrage, graine de pic, borne).
    #[default]
    Points,
    /// Glisser pose une zone ou une plage.
    Zone,
    /// Glisser trace un rectangle de zoom.
    Zoom,
}

/// Ce qui manque pour valider ou pour montrer l'aperçu.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// Aucune série à traiter.
    NoSeries,
    /// La correction attend une colonne de référence.
    NeedsReference,
    /// Une déconvolution attend une réponse instrumentale.
    NeedsResponse,
    /// Pas assez de points posés.
    NeedsPoints { needed: usize, have: usize },
    /// Une zone de fond au moins.
    NeedsZone,
    /// Le modèle libre est illisible (`Some`) ou sans paramètre (`None`).
    BadModel(Option<stokesforge_analysis::expr::ExprError>),
    /// Une colonne citée n'existe plus.
    MissingColumn,
    /// Le calcul lui-même a échoué.
    Failed(AnalysisError),
}

/// Le formulaire complet d'une fenêtre d'analyse.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisForm {
    /// La famille active ; les réglages des autres sont gardés dans `setup`.
    pub kind: OperationKind,
    pub setup: Setup,
    pub series: Vec<SeriesRef>,
    /// Référence d'une correction, réponse instrumentale d'une déconvolution.
    pub aux: Option<AuxRef>,
    /// « Keep in Results » plutôt que « Show » (M5).
    pub keep_results: bool,
    /// Recalcul automatique (M9 b).
    pub auto_recalc: bool,
    /// L'utilisateur a touché la case : un changement de famille ne la remet
    /// plus au défaut de l'opération.
    pub auto_recalc_chosen: bool,
    /// Le calcul qu'on modifie (« Edit Calculation… »).
    pub editing: Option<NodeId>,
    /// Le graphe d'où l'on vient : « Add to graph » y range les colonnes.
    pub graph: Option<NodeId>,
    pub add_to_graph: bool,
    /// La série montrée dans l'aperçu.
    pub current: usize,
    pub tool: PreviewTool,
    /// Moyenne locale d'un point aimanté (M4 b).
    pub average: usize,
}

impl AnalysisForm {
    /// Un formulaire neuf pour `preset` sur `series`.
    pub fn new(preset: &Preset, series: Vec<SeriesRef>, keep_results: bool) -> Self {
        let mut setup = Setup::default();
        match preset {
            Preset::BaselineLine => setup.baseline.shape = BaselineShape::TwoPoints,
            Preset::BaselinePolynomial => {
                setup.baseline.shape = BaselineShape::Zones;
                setup.baseline.degree = 3;
            }
            Preset::BaselineAnchors => setup.baseline.shape = BaselineShape::Anchors,
            Preset::Fit(model) => setup.fit.set_model(model.clone()),
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
            editing: None,
            graph: None,
            add_to_graph: false,
            current: 0,
            tool: PreviewTool::default(),
            average: DEFAULT_AVERAGE,
        };
        form.apply_default_auto();
        form.tool = form.natural_tool();
        form
    }

    /// Les deux formulaires donnent-ils le même aperçu ? L'outil de la souris, la
    /// moyenne locale, la destination et les cases de sortie ne changent pas ce
    /// qui est calculé : l'aperçu ne se recalcule pas pour eux.
    pub fn same_preview(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.current == other.current
            && self.aux == other.aux
            && self.series == other.series
            && self.setup == other.setup
    }

    /// Le formulaire qui rouvre un calcul existant (« Edit Calculation… »).
    pub fn from_calculation(id: NodeId, calc: &Calculation) -> Self {
        let mut form = Self::new(
            &preset_of(&calc.operation),
            calc.series.clone(),
            calc.keep_results,
        );
        form.editing = Some(id);
        form.aux = calc.aux.clone();
        form.auto_recalc = calc.auto_recalc;
        form.auto_recalc_chosen = true;
        form.setup.load(&calc.operation);
        // La pondération n'est pas dans l'opération : elle se lit dans les séries.
        form.setup.fit.weighted = calc.series.iter().any(|s| s.sigma.is_some());
        let has = |key: &str| calc.outputs.iter().any(|o| o.key == key);
        form.setup.peaks.peak_columns = has("peak");
        form.setup.peaks.sum_column = has("peaks_sum");
        form
    }

    /// Change de famille ; le recalcul automatique retrouve le défaut de la
    /// nouvelle opération tant que l'utilisateur n'a pas choisi.
    pub fn set_kind(&mut self, kind: OperationKind) {
        self.kind = kind;
        self.apply_default_auto();
    }

    /// Remet la case « recalcul automatique » au défaut de l'opération et du
    /// nombre de séries (reconvolution, déconvolution et gros lots : manuel).
    pub fn apply_default_auto(&mut self) {
        if self.auto_recalc_chosen {
            return;
        }
        self.auto_recalc = self.auto_by_default();
    }

    /// Le défaut de M9 b pour la famille et le nombre de séries actuels.
    pub fn auto_by_default(&self) -> bool {
        !matches!(
            self.kind,
            OperationKind::Reconvolution | OperationKind::DirectDeconvolution
        ) && self.series.len() <= HEAVY_BATCH
    }

    /// Cette famille lit une colonne annexe.
    pub fn needs_aux(&self) -> bool {
        matches!(
            self.kind,
            OperationKind::Correct
                | OperationKind::Reconvolution
                | OperationKind::DirectDeconvolution
        )
    }

    /// L'ajustement lie des paramètres entre les séries : l'aperçu les calcule
    /// toutes.
    pub fn is_global(&self) -> bool {
        self.kind == OperationKind::Fit && self.setup.fit.is_global() && self.series.len() > 1
    }

    /// Les séries sont nommées une par une dans l'aperçu (`◀ ▶`).
    pub fn clamp_current(&mut self) {
        if self.current >= self.series.len() {
            self.current = self.series.len().saturating_sub(1);
        }
    }

    /// Ce qui empêche de valider, sans consulter les données.
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

    /// Les réglages propres à la famille active qui manquent.
    fn setup_issues(&self) -> Option<Issue> {
        match self.kind {
            OperationKind::Baseline => {
                let b = &self.setup.baseline;
                match b.shape {
                    BaselineShape::TwoPoints | BaselineShape::Anchors if b.points.len() < 2 => {
                        Some(Issue::NeedsPoints {
                            needed: 2,
                            have: b.points.len(),
                        })
                    }
                    BaselineShape::Zones if b.zones.is_empty() => Some(Issue::NeedsZone),
                    _ => None,
                }
            }
            OperationKind::Fit => match self.setup.fit.model.compile() {
                Err(AnalysisError::BadExpression(error)) => Some(Issue::BadModel(Some(error))),
                Err(other) => Some(Issue::Failed(other)),
                Ok(m) if m.param_count() == 0 => Some(Issue::BadModel(None)),
                Ok(_) => None,
            },
            _ => None,
        }
    }

    /// L'opération que les réglages décrivent, ou `None` tant qu'un réglage
    /// manque. `for_preview` force les colonnes d'aide (ligne de base,
    /// résidus) pour qu'elles se montrent même non écrites.
    pub fn operation_for(&self, for_preview: bool) -> Option<Operation> {
        if self.setup_issues().is_some() {
            return None;
        }
        let s = &self.setup;
        Some(match self.kind {
            OperationKind::Baseline => {
                let b = &s.baseline;
                let method = match b.shape {
                    BaselineShape::TwoPoints => BaselineMethod::Line {
                        p1: b.points[0],
                        p2: b.points[1],
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
        })
    }

    /// L'opération qui sera enregistrée.
    pub fn operation(&self) -> Option<Operation> {
        self.operation_for(false)
    }

    /// Les séries telles que le calcul les lit : la colonne d'erreur ne compte
    /// que pour un ajustement pondéré.
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

    /// Les séries que l'aperçu calcule : la série montrée, ou toutes pour un
    /// ajustement global (les paramètres partagés les lient).
    fn preview_series(&self) -> Vec<SeriesRef> {
        let all = self.effective_series();
        if self.is_global() {
            all
        } else {
            all.get(self.current).cloned().into_iter().collect()
        }
    }

    /// Le calcul de l'aperçu : colonnes d'aide comprises. Les indices de
    /// série des colonnes rendues sont ceux de [`Self::preview_rank`].
    pub fn preview(&self, tree: &Tree) -> Result<Computed, Issue> {
        if let Some(issue) = self.issues().into_iter().next() {
            return Err(issue);
        }
        let op = self.operation_for(true).ok_or(Issue::NoSeries)?;
        let series = self.preview_series();
        run(tree, &op, &series, self.aux.as_ref())
    }

    /// Le rang, dans `preview`, de la série montrée.
    pub fn preview_rank(&self) -> usize {
        if self.is_global() {
            self.current
        } else {
            0
        }
    }

    /// Ce qui s'écrit : la sortie `key` est-elle demandée ?
    pub fn wants_output(&self, key: &str) -> bool {
        match (self.kind, key) {
            (OperationKind::Baseline, "baseline") => self.setup.baseline.baseline_column,
            (OperationKind::Integrate, "cumulative") => self.setup.integrate.cumulative,
            (OperationKind::Integrate, "normalized") => self.setup.integrate.normalize,
            (OperationKind::Fit, "residuals") => self.setup.fit.residuals,
            (OperationKind::Peaks, "residuals") => self.setup.peaks.residuals,
            (OperationKind::Peaks, "peak" | "background") => self.setup.peaks.peak_columns,
            (OperationKind::Peaks, "peaks_sum") => self.setup.peaks.sum_column,
            (OperationKind::Reconvolution, "residuals") => self.setup.reconv.residuals,
            _ => true,
        }
    }

    /// Le lot à enregistrer : toutes les séries calculées, les colonnes
    /// demandées avec leur nom long (M6), les valeurs si on les garde.
    ///
    /// En modification, une colonne qui existe déjà garde ses métadonnées (nom
    /// long, unité, commentaire que l'utilisateur a pu retoucher).
    pub fn draft(&self, tree: &Tree) -> Result<CalculationDraft, Issue> {
        if let Some(issue) = self.issues().into_iter().next() {
            return Err(issue);
        }
        let operation = self.operation().ok_or(Issue::NoSeries)?;
        let series = self.effective_series();
        let computed = run(tree, &operation, &series, self.aux.as_ref())?;
        let existing = self
            .editing
            .and_then(|id| tree.calculation(id))
            .map(|c| c.outputs.clone())
            .unwrap_or_default();
        let outputs = computed
            .columns
            .into_iter()
            .filter(|c| self.wants_output(&c.key))
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
        Ok(CalculationDraft {
            operation,
            series,
            aux: self.aux.clone(),
            outputs,
            results: if self.keep_results {
                computed.results
            } else {
                Vec::new()
            },
            keep_results: self.keep_results,
            auto_recalc: self.auto_recalc,
        })
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

/// Le nom court d'une colonne source (« B »), à défaut le vide.
fn short_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .map(|n| n.name().to_string())
        .unwrap_or_default()
}

/// Le nom long d'une colonne source, ou à défaut son nom court.
fn long_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().long_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| short_name(tree, column))
}

/// Les métadonnées d'une colonne produite (M6) : le nom long dit la sortie et
/// la source ; l'unité reprend celle de la source quand la sortie est de même
/// nature ; le commentaire nomme l'opération.
pub fn output_meta(
    tree: &Tree,
    kind: OperationKind,
    series: &SeriesRef,
    key: &str,
    index: Option<usize>,
) -> stokesforge_core::ColumnMeta {
    let source = series.y;
    let column = short_name(tree, source);
    let name = long_name(tree, source);
    let long = match (kind, key) {
        (OperationKind::Baseline, "corrected") => {
            tr!(
                "analysis-output-baseline-corrected",
                name = name,
                column = column
            )
        }
        (_, "corrected") => tr!("analysis-output-corrected", name = name),
        (_, "baseline") => tr!("analysis-output-baseline", column = column),
        (_, "cumulative") => tr!("analysis-output-cumulative", column = column),
        (_, "normalized") => tr!("analysis-output-normalized", column = column),
        (_, "fit") => tr!("analysis-output-fit", name = name),
        (_, "residuals") => tr!("analysis-output-residuals", name = name),
        (_, "peak") => tr!(
            "analysis-output-peak",
            index = i64::try_from(index.unwrap_or(1)).unwrap_or(1),
            column = column
        ),
        (_, "background") => tr!("analysis-output-background", column = column),
        (_, "peaks_sum") => tr!("analysis-output-peaks-sum", column = column),
        (_, "deconvolved") => tr!("analysis-output-deconvolved", name = name),
        _ => name.clone(),
    };
    let unit = match key {
        "cumulative" | "normalized" | "peak" | "background" | "peaks_sum" | "residuals" | "fit"
        | "baseline" | "corrected" | "deconvolved" => source_unit(tree, source),
        _ => String::new(),
    };
    stokesforge_core::ColumnMeta {
        long_name: long,
        unit,
        comment: kind_title(kind),
        ..stokesforge_core::ColumnMeta::default()
    }
}

fn source_unit(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().unit.clone())
        .unwrap_or_default()
}

/// Le nom d'une famille d'opération, dans la langue courante.
pub fn kind_title(kind: OperationKind) -> String {
    match kind {
        OperationKind::Baseline => tr!("analysis-kind-baseline"),
        OperationKind::Correct => tr!("analysis-kind-correct"),
        OperationKind::Integrate => tr!("analysis-kind-integrate"),
        OperationKind::Fit => tr!("analysis-kind-fit"),
        OperationKind::Peaks => tr!("analysis-kind-peaks"),
        OperationKind::Reconvolution => tr!("analysis-kind-reconvolution"),
        OperationKind::DirectDeconvolution => tr!("analysis-kind-direct-deconvolution"),
    }
}

/// Le préréglage qui rouvre une opération existante.
pub fn preset_of(op: &Operation) -> Preset {
    match op {
        Operation::Baseline { method, .. } => match method {
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
    }
}

impl Setup {
    /// Reprend les réglages d'une opération existante (modification).
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

/// Ce qui manque ou ce qui a échoué, en phrase de l'interface.
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
    /// Les noms des paramètres, dans l'ordre des réglages : fond, une
    /// amplitude et une durée de vie par composante, puis le décalage.
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
    /// L'outil qui sert le plus souvent à la famille et à la forme courantes :
    /// des zones pour les plages et le fond, des points sinon.
    pub fn natural_tool(&self) -> PreviewTool {
        match self.kind {
            OperationKind::Baseline if self.setup.baseline.shape == BaselineShape::Zones => {
                PreviewTool::Zone
            }
            OperationKind::Integrate | OperationKind::Fit | OperationKind::Reconvolution => {
                PreviewTool::Zone
            }
            _ => PreviewTool::Points,
        }
    }

    /// Un clic de l'outil « points » en `(x, y)` : un point d'ancrage ou de
    /// droite, ou la position de départ d'un pic.
    pub fn add_point(&mut self, x: f64, y: f64) {
        match self.kind {
            OperationKind::Baseline => {
                let b = &mut self.setup.baseline;
                match b.shape {
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

    /// Une zone tracée de `a` à `b` : zone de fond, plage d'intégration,
    /// plage d'ajustement ou fenêtre de reconvolution.
    pub fn add_zone(&mut self, a: f64, b: f64) {
        let range = (a.min(b), a.max(b));
        if range.0 == range.1 {
            return;
        }
        match self.kind {
            OperationKind::Baseline if self.setup.baseline.shape == BaselineShape::Zones => {
                self.setup.baseline.zones.push(range);
            }
            OperationKind::Integrate => self.setup.integrate.ranges.push(range),
            OperationKind::Fit => self.setup.fit.range = Some(range),
            OperationKind::Peaks => self.setup.peaks.range = Some(range),
            OperationKind::Reconvolution => self.setup.reconv.spec.window = Some(range),
            _ => {}
        }
    }

    /// Retire ce que le clic droit vise en `x` : le point ou le pic à moins de
    /// `tolerance`, ou la zone qui contient `x`. Rend vrai si quelque chose a
    /// été retiré.
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
                if b.shape == BaselineShape::Zones {
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

/// Les valeurs de X et de Y de **chaque ligne** d'une série, `NaN` pour une
/// cellule vide ou textuelle ; sans colonne X, le numéro de ligne à partir de 1
/// (comme le calcul).
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

/// Les points `(x, y)` finis d'une série, dans l'ordre des lignes.
pub fn series_points(tree: &Tree, series: &SeriesRef) -> (Vec<f64>, Vec<f64>) {
    let (xs, ys) = series_rows(tree, series);
    xs.into_iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .unzip()
}

/// Le nom d'une série pour la fenêtre : « Feuille / nom long (B) ».
pub fn series_label(tree: &Tree, series: &SeriesRef) -> String {
    let short = short_name(tree, series.y);
    let long = long_name(tree, series.y);
    if long == short {
        short
    } else {
        format!("{long} ({short})")
    }
}

/// Les colonnes Y que la fenêtre propose comme référence ou comme réponse
/// instrumentale : celles du classeur de la première série, hors les séries
/// traitées elles-mêmes. Le nom dit la feuille.
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

/// La colonne annexe `y` avec le X le plus proche à sa gauche dans sa feuille.
pub fn aux_for(tree: &Tree, y: NodeId) -> AuxRef {
    let x = tree.node(y).and_then(|n| n.parent()).and_then(|sheet| {
        let columns = tree.children(sheet).ok()?;
        let at = columns.iter().position(|c| *c == y)?;
        columns[..at].iter().rev().copied().find(|c| {
            tree.node(*c)
                .and_then(|n| n.as_column())
                .is_some_and(|col| col.meta().role == stokesforge_core::ColumnRole::X)
        })
    });
    AuxRef { x, y }
}

/// La formule d'un modèle, en Typst, pour la fenêtre d'ajustement (#204). Les
/// noms de paramètres sont ceux du tableau (`A`, `tau`, `y0`…). `None` pour le
/// modèle libre : sa formule est celle que l'utilisateur écrit.
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

/// L'unité d'une valeur calculée : pour une aire, le produit des unités de X et
/// de Y (`nm·counts`), sans conversion, écrite seulement si les deux sont
/// renseignées (#187, #204) ; vide pour tout le reste.
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

/// Une valeur et son erreur, suivies de leur unité quand elle existe.
pub fn with_unit(text: String, unit: &str) -> String {
    if unit.is_empty() {
        text
    } else {
        format!("{text} {unit}")
    }
}

/// Le libellé d'une valeur calculée (`area`, `param.tau`, `stat.r_squared`…),
/// avec son rang pour les valeurs par plage ou par pic. Une clé inconnue
/// s'affiche brute : rien ne disparaît.
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
