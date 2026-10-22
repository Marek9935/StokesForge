//! Dossier `Analyse` de la démonstration : **un classeur par famille de calcul**
//! du menu Analyse, chacun avec des données dont on connaît la réponse.
//!
//! Chaque [`Jeu`] porte ses données, l'opération qui leur convient et ce
//! qu'elle doit retrouver ([`Attendu`], avec une tolérance) : le test
//! `chaque_jeu_d_analyse_retrouve_ses_parametres` rejoue `core::compute` sur
//! chacun, donc une démo dont les données ne disent plus ce qu'elles
//! prétendent échoue au lieu de tromper l'utilisateur. Les données vivent dans
//! le code (pas de fichier) et leur bruit est à graine fixe ([`Bruit`]).
//!
//! Quelques jeux ([`Jeu::calcule`]) ont déjà leur calcul enregistré, construit
//! comme le fait `pyst` (`compute` puis [`CalculationDraft`]) : ses valeurs
//! apparaissent dans le volet Results dès l'ouverture, ses colonnes de sortie
//! s'insèrent après la colonne Y. Les autres attendent qu'on ouvre la fenêtre
//! d'analyse. Les noms de colonnes restent les lettres automatiques ; seuls les
//! titres (noms longs) et les unités sont lisibles.

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

use super::bruit::Bruit;

/// Ce qu'un jeu doit retrouver après son calcul. Lu par les tests seulement :
/// le binaire n'en fait rien, mais ces attentes **sont** la preuve que les
/// données disent ce qu'elles prétendent.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum Attendu {
    /// Une valeur du volet Results : clé, rang (plage, pic), valeur, tolérance
    /// absolue.
    Valeur(&'static str, Option<usize>, f64, f64),
    /// Une cellule d'une colonne de sortie : clé, ligne (0 = la première),
    /// valeur, tolérance absolue.
    Ligne(&'static str, usize, f64, f64),
}

/// Des données et l'opération qui les traite.
pub(super) struct Jeu {
    /// Nom du classeur : unique dans tout le projet.
    pub(super) nom: &'static str,
    /// Nom long et unité de X, puis de Y.
    pub(super) titres: [(&'static str, &'static str); 2],
    pub(super) x: Vec<f64>,
    pub(super) y: Vec<f64>,
    /// Incertitude de chaque point (ajustement pondéré).
    pub(super) sigma: Option<Vec<f64>>,
    /// La référence d'une correction, l'IRF d'une déconvolution : son X, son Y.
    pub(super) reference: Option<(Vec<f64>, Vec<f64>)>,
    pub(super) operation: Operation,
    pub(super) attendus: Vec<Attendu>,
    /// Le calcul est déjà enregistré dans le projet (volet Results).
    pub(super) calcule: bool,
}

/// Les colonnes d'un jeu une fois posées dans le projet.
pub(super) struct Colonnes {
    pub(super) x: NodeId,
    pub(super) y: NodeId,
    pub(super) sigma: Option<NodeId>,
    pub(super) reference: Option<AuxRef>,
}

impl Jeu {
    pub(super) fn series(&self, colonnes: &Colonnes) -> Vec<SeriesRef> {
        vec![SeriesRef {
            x: Some(colonnes.x),
            y: colonnes.y,
            sigma: colonnes.sigma,
        }]
    }
}

fn jeu(
    nom: &'static str,
    titres: [(&'static str, &'static str); 2],
    (x, y): (Vec<f64>, Vec<f64>),
    operation: Operation,
    attendus: Vec<Attendu>,
) -> Jeu {
    Jeu {
        nom,
        titres,
        x,
        y,
        sigma: None,
        reference: None,
        operation,
        attendus,
        calcule: false,
    }
}

/// `debut, debut + pas, …, fin` (bornes incluses).
fn grille(debut: f64, fin: f64, pas: f64) -> Vec<f64> {
    let n = ((fin - debut) / pas).round() as usize;
    (0..=n).map(|i| debut + i as f64 * pas).collect()
}

fn gaussienne(x: f64, hauteur: f64, centre: f64, largeur: f64) -> f64 {
    let u = (x - centre) / largeur;
    hauteur * (-u * u).exp()
}

/// Le profil de Voigt d'un pic, normalisé à `hauteur` à son sommet.
fn pic_voigt(x: f64, hauteur: f64, centre: f64, fwhm_gauss: f64, fwhm_lorentz: f64) -> f64 {
    hauteur * voigt::profile(x - centre, fwhm_gauss, fwhm_lorentz)
        / voigt::profile(0.0, fwhm_gauss, fwhm_lorentz)
}

fn ajustement(model: Model) -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model,
            params: Vec::new(),
        },
        range: None,
        residuals: true,
    }
}

/// Une réponse impulsionnelle gaussienne sur le même pas que les signaux de
/// temps : 0 à 3 ns par 0,1 ns, centrée à 1,5 ns.
fn reponse_instrument() -> (Vec<f64>, Vec<f64>) {
    let t = grille(0.0, 3.0, 0.1);
    let y = t
        .iter()
        .map(|&t| gaussienne(t, 1.0, 1.5, 0.4 / (2.0 * LN_2.sqrt())))
        .collect();
    (t, y)
}

/// Le signal d'une somme d'exponentielles vue à travers `reponse` : le modèle
/// de la reconvolution (`A·Σ h_i·e^(−(k−i)·pas/τ)`, `h` normalisée à 1).
fn decroissance_mesuree(
    t: &[f64],
    reponse: &(Vec<f64>, Vec<f64>),
    fond: f64,
    composantes: &[(f64, f64)],
) -> Vec<f64> {
    let pas = t[1] - t[0];
    let mut h: Vec<f64> = t
        .iter()
        .map(|&t| {
            reponse
                .0
                .iter()
                .position(|&x| (x - t).abs() < 1e-9)
                .map_or(0.0, |i| reponse.1[i])
        })
        .collect();
    let somme: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= somme);
    let mut sortie = vec![fond; t.len()];
    for &(amplitude, tau) in composantes {
        let facteur = (-pas / tau).exp();
        let mut c = 0.0;
        for (k, s) in sortie.iter_mut().enumerate() {
            c = facteur * c + h[k];
            *s += amplitude * c;
        }
    }
    sortie
}

// ── Les jeux, famille par famille ──────────────────────────────────────

/// Un pic posé sur une pente qui s'incurve : la ligne de base se règle sur les
/// deux flancs sans pic (zones, degré 2).
fn baseline() -> Jeu {
    let mut bruit = Bruit::new(11);
    let x = grille(0.0, 100.0, 0.5);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| 0.8 + 0.03 * x + 0.0006 * x * x + gaussienne(x, 4.0, 50.0, 6.0))
        .collect();
    jeu(
        "Baseline",
        [("Déplacement", "mm"), ("Signal", "mV")],
        (x, bruit.bruite(&propre, 0.05)),
        Operation::Baseline {
            method: BaselineMethod::Zones {
                zones: vec![(0.0, 30.0), (70.0, 100.0)],
                degree: 2,
            },
            op: BaselineOp::Subtract,
            keep_baseline: true,
        },
        vec![Attendu::Ligne("corrected", 100, 4.0, 0.15)],
    )
}

/// Un pic bruité que Savitzky-Golay lisse sans l'abaisser : le sommet garde sa
/// hauteur (10) là où une moyenne de la même fenêtre l'écraserait.
fn lissage() -> Jeu {
    let mut bruit = Bruit::new(14);
    let x = grille(0.0, 100.0, 0.5);
    let propre: Vec<f64> = x.iter().map(|&x| gaussienne(x, 10.0, 50.0, 8.0)).collect();
    jeu(
        "Smooth",
        [("Déplacement", "mm"), ("Signal", "mV")],
        (x, bruit.bruite(&propre, 0.5)),
        Operation::Smooth {
            spec: SmoothSpec {
                method: SmoothMethod::SavitzkyGolay { order: 3 },
                window: SmoothWindow::Points(15),
            },
        },
        vec![
            Attendu::Valeur("count.window", None, 15.0, 0.0),
            Attendu::Ligne("smoothed", 100, 10.0, 0.5),
            Attendu::Ligne("smoothed", 0, 0.0, 0.5),
        ],
    )
}

/// Un signal et sa référence **sur d'autres abscisses** : la référence est
/// interpolée aux X du signal.
fn correct() -> Jeu {
    let mut bruit = Bruit::new(12);
    let lampe = |x: f64| 200.0 + gaussienne(x, 1000.0, 600.0, 250.0);
    let transmission = |x: f64| 1.0 - gaussienne(x, 0.6, 550.0, 20.0);
    let x = grille(400.0, 800.0, 5.0);
    let propre: Vec<f64> = x.iter().map(|&x| lampe(x) * transmission(x)).collect();
    let xr = grille(380.0, 820.0, 10.0);
    let yr = xr.iter().map(|&x| lampe(x)).collect();
    let mut jeu = jeu(
        "Correct",
        [("Longueur d'onde", "nm"), ("Signal", "u.a.")],
        (x, bruit.bruite(&propre, 3.0)),
        Operation::Correct {
            op: CorrectOp::Divide,
        },
        vec![
            Attendu::Ligne("corrected", 30, 0.4, 0.03),
            Attendu::Ligne("corrected", 0, 1.0, 0.03),
        ],
    );
    jeu.reference = Some((xr, yr));
    jeu
}

/// Deux pics, deux plages d'intégration : aire de chacun, intégrale cumulée et
/// normalisée.
fn integrate() -> Jeu {
    let mut bruit = Bruit::new(13);
    let x = grille(0.0, 20.0, 0.1);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| gaussienne(x, 2.0, 6.0, 1.0) + gaussienne(x, 1.2, 14.0, 1.5))
        .collect();
    let mut jeu = jeu(
        "Integrate",
        [("Temps de rétention", "min"), ("Absorbance", "u.a.")],
        (x, bruit.bruite(&propre, 0.01)),
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
            Attendu::Valeur("area", Some(1), 2.0 * PI.sqrt(), 0.05),
            Attendu::Valeur("area", Some(2), 1.2 * 1.5 * PI.sqrt(), 0.05),
        ],
    );
    jeu.calcule = true;
    jeu
}

/// Une droite pondérée : l'incertitude croît avec X, la colonne σ la porte.
fn ajustement_lineaire() -> Jeu {
    let mut bruit = Bruit::new(14);
    let x = grille(0.0, 20.0, 0.5);
    let sigma: Vec<f64> = x.iter().map(|&x| 0.15 + 0.01 * x).collect();
    let y = x
        .iter()
        .zip(&sigma)
        .map(|(&x, &s)| 2.0 + 0.5 * x + bruit.gauss(s))
        .collect();
    let mut jeu = jeu(
        "FitLineaire",
        [("Concentration", "mmol/L"), ("Absorbance", "u.a.")],
        (x, y),
        ajustement(Model::Linear),
        vec![
            Attendu::Valeur("param.intercept", None, 2.0, 0.3),
            Attendu::Valeur("param.slope", None, 0.5, 0.03),
        ],
    );
    jeu.sigma = Some(sigma);
    jeu
}

fn ajustement_polynome() -> Jeu {
    let mut bruit = Bruit::new(15);
    let x = grille(-3.0, 3.0, 0.1);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| 1.0 - 2.0 * x + 0.5 * x * x + 0.3 * x * x * x)
        .collect();
    let mut jeu = jeu(
        "FitPolynome",
        [("Tension", "V"), ("Courant", "mA")],
        (x, bruit.bruite(&propre, 0.1)),
        ajustement(Model::Polynomial { degree: 3 }),
        vec![
            Attendu::Valeur("param.c0", None, 1.0, 0.1),
            Attendu::Valeur("param.c1", None, -2.0, 0.1),
            Attendu::Valeur("param.c2", None, 0.5, 0.1),
            Attendu::Valeur("param.c3", None, 0.3, 0.05),
        ],
    );
    jeu.calcule = true;
    jeu
}

fn ajustement_exponentielle() -> Jeu {
    let mut bruit = Bruit::new(16);
    let x = grille(0.0, 10.0, 0.1);
    let propre: Vec<f64> = x.iter().map(|&x| 0.5 + 4.0 * (-x / 2.2).exp()).collect();
    jeu(
        "FitExponentielle",
        [("Temps", "s"), ("Intensité", "u.a.")],
        (x, bruit.bruite(&propre, 0.05)),
        ajustement(Model::ExponentialOffset),
        vec![
            Attendu::Valeur("param.y0", None, 0.5, 0.15),
            Attendu::Valeur("param.A", None, 4.0, 0.2),
            Attendu::Valeur("param.tau", None, 2.2, 0.2),
        ],
    )
}

fn ajustement_exponentielle_double() -> Jeu {
    let mut bruit = Bruit::new(17);
    let x = grille(0.0, 30.0, 0.3);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| 0.2 + 3.0 * (-x / 1.2).exp() + 2.0 * (-x / 9.0).exp())
        .collect();
    jeu(
        "FitExponentielle2",
        [("Temps", "ns"), ("Intensité", "u.a.")],
        (x, bruit.bruite(&propre, 0.02)),
        ajustement(Model::Exponential2),
        vec![
            Attendu::Valeur("param.tau1", None, 1.2, 0.3),
            Attendu::Valeur("param.tau2", None, 9.0, 1.5),
        ],
    )
}

fn ajustement_puissance() -> Jeu {
    let mut bruit = Bruit::new(18);
    let x = grille(1.0, 50.0, 1.0);
    let y = x
        .iter()
        .map(|&x| 2.5 * x.powf(0.7) * (1.0 + bruit.gauss(0.02)))
        .collect();
    jeu(
        "FitPuissance",
        [("Fréquence", "Hz"), ("Amplitude", "u.a.")],
        (x, y),
        ajustement(Model::Power),
        vec![
            Attendu::Valeur("param.a", None, 2.5, 0.2),
            Attendu::Valeur("param.b", None, 0.7, 0.03),
        ],
    )
}

/// X strictement positif : le logarithme n'est défini que pour `x > 0`.
fn ajustement_logarithme() -> Jeu {
    let mut bruit = Bruit::new(19);
    let x = grille(1.0, 100.0, 1.0);
    let propre: Vec<f64> = x.iter().map(|&x| 3.0 + 1.8 * x.ln()).collect();
    jeu(
        "FitLogarithme",
        [("Dose", "mg"), ("Réponse", "%")],
        (x, bruit.bruite(&propre, 0.1)),
        ajustement(Model::Logarithm),
        vec![
            Attendu::Valeur("param.a", None, 3.0, 0.15),
            Attendu::Valeur("param.b", None, 1.8, 0.05),
        ],
    )
}

fn ajustement_gaussien() -> Jeu {
    let mut bruit = Bruit::new(20);
    let x = grille(-5.0, 5.0, 0.1);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| 0.1 + gaussienne(x, 3.2, 0.4, 1.8 / (2.0 * LN_2.sqrt())))
        .collect();
    let mut jeu = jeu(
        "FitGaussienne",
        [("Énergie", "eV"), ("Intensité", "u.a.")],
        (x, bruit.bruite(&propre, 0.05)),
        ajustement(Model::Gaussian),
        vec![
            Attendu::Valeur("param.y0", None, 0.1, 0.05),
            Attendu::Valeur("param.A", None, 3.2, 0.1),
            Attendu::Valeur("param.xc", None, 0.4, 0.03),
            Attendu::Valeur("param.fwhm", None, 1.8, 0.1),
        ],
    );
    jeu.calcule = true;
    jeu
}

fn ajustement_lorentzien() -> Jeu {
    let mut bruit = Bruit::new(21);
    let x = grille(-10.0, 10.0, 0.2);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| 0.05 + 2.4 / (1.0 + 4.0 * (x + 0.6) * (x + 0.6) / (2.2 * 2.2)))
        .collect();
    jeu(
        "FitLorentzienne",
        [("Fréquence", "MHz"), ("Intensité", "u.a.")],
        (x, bruit.bruite(&propre, 0.04)),
        ajustement(Model::Lorentzian),
        vec![
            Attendu::Valeur("param.A", None, 2.4, 0.15),
            Attendu::Valeur("param.xc", None, -0.6, 0.05),
            Attendu::Valeur("param.fwhm", None, 2.2, 0.15),
        ],
    )
}

/// Un modèle écrit à la main : saturation de Michaelis-Menten.
fn ajustement_libre() -> Jeu {
    let mut bruit = Bruit::new(22);
    let x = grille(0.2, 10.0, 0.2);
    let propre: Vec<f64> = x.iter().map(|&x| 5.0 * x / (1.5 + x)).collect();
    jeu(
        "FitLibre",
        [("Substrat", "mmol/L"), ("Vitesse", "µmol/min")],
        (x, bruit.bruite(&propre, 0.08)),
        ajustement(Model::Custom {
            formula: "a*x/(b+x)".into(),
        }),
        vec![
            Attendu::Valeur("param.a", None, 5.0, 0.3),
            Attendu::Valeur("param.b", None, 1.5, 0.2),
        ],
    )
}

/// Trois pics de Voigt qui se chevauchent sur un fond linéaire.
fn pics() -> Jeu {
    let mut bruit = Bruit::new(23);
    let x = grille(0.0, 100.0, 0.5);
    let propre: Vec<f64> = x
        .iter()
        .map(|&x| {
            0.3 + 0.004 * x
                + pic_voigt(x, 3.0, 40.0, 5.0, 3.0)
                + pic_voigt(x, 2.0, 55.0, 4.0, 3.0)
                + pic_voigt(x, 1.2, 62.0, 4.0, 3.0)
        })
        .collect();
    jeu(
        "Peaks",
        [("Déplacement Raman", "cm⁻¹"), ("Intensité", "cps")],
        (x, bruit.bruite(&propre, 0.03)),
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
            Attendu::Valeur("peak.center", Some(1), 40.0, 0.5),
            Attendu::Valeur("peak.center", Some(2), 55.0, 0.5),
            Attendu::Valeur("peak.center", Some(3), 62.0, 0.8),
        ],
    )
}

/// Le signal d'une double décroissance vue à travers l'IRF, et cette IRF sur le
/// même pas (0,1 ns) : la grille est uniforme, comme l'exigent la
/// reconvolution et la déconvolution directe.
fn decroissance(nom: &'static str, graine: u64, composantes: &[(f64, f64)]) -> Jeu {
    let mut bruit = Bruit::new(graine);
    let t = grille(0.0, 25.5, 0.1);
    let reponse = reponse_instrument();
    let propre = decroissance_mesuree(&t, &reponse, 2.0, composantes);
    let mut jeu = jeu(
        nom,
        [("Temps", "ns"), ("Intensité", "cps")],
        (t, bruit.bruite(&propre, 1.5)),
        Operation::Reconvolution {
            spec: ReconvSpec {
                components: composantes.len(),
                params: Vec::new(),
                shift_initial: 0.0,
                fit_shift: false,
                window: None,
            },
            residuals: true,
        },
        Vec::new(),
    );
    jeu.reference = Some(reponse);
    jeu
}

fn reconvolution() -> Jeu {
    let mut jeu = decroissance("Reconvolution", 24, &[(800.0, 0.9), (300.0, 4.5)]);
    jeu.attendus = vec![
        Attendu::Valeur("param.tau1", None, 0.9, 0.15),
        Attendu::Valeur("param.tau2", None, 4.5, 0.8),
    ];
    jeu
}

fn deconvolution() -> Jeu {
    let mut jeu = decroissance("Deconvolution", 25, &[(500.0, 2.0)]);
    jeu.operation = Operation::DirectDeconvolution {
        method: DirectMethod::RichardsonLucy { iterations: 40 },
        shift: 0.0,
    };
    // La décroissance de l'objet, `500·e^(−t/2)`, relue à t = 2 ns.
    jeu.attendus = vec![Attendu::Ligne(
        "deconvolved",
        20,
        500.0 * (-1.0f64).exp(),
        25.0,
    )];
    jeu
}

/// Tous les jeux, dans l'ordre des familles du menu Analyse.
pub(super) fn jeux() -> Vec<Jeu> {
    vec![
        baseline(),
        correct(),
        lissage(),
        integrate(),
        ajustement_lineaire(),
        ajustement_polynome(),
        ajustement_exponentielle(),
        ajustement_exponentielle_double(),
        ajustement_puissance(),
        ajustement_logarithme(),
        ajustement_gaussien(),
        ajustement_lorentzien(),
        ajustement_libre(),
        pics(),
        reconvolution(),
        deconvolution(),
    ]
}

/// Une colonne numérique titrée : nom long, unité et rôle (le nom reste la
/// lettre automatique).
pub(super) fn colonne_titree(
    project: &mut Project,
    feuille: NodeId,
    valeurs: &[f64],
    (titre, unite): (&str, &str),
    role: ColumnRole,
) -> anyhow::Result<NodeId> {
    let mut colonne = Column::from_f64(valeurs);
    colonne.set_meta(ColumnMeta {
        role,
        long_name: titre.to_string(),
        unit: unite.to_string(),
        ..ColumnMeta::default()
    });
    project
        .add_column(feuille, None, None, colonne)
        .with_context(|| format!("colonne « {titre} »"))
}

/// Pose le classeur d'un jeu sous `parent` : X, Y, puis l'incertitude et la
/// référence si le jeu en a.
pub(super) fn construit(
    project: &mut Project,
    parent: NodeId,
    jeu: &Jeu,
) -> anyhow::Result<Colonnes> {
    let classeur = project
        .add_workbook(parent, Some(jeu.nom))
        .with_context(|| format!("classeur {}", jeu.nom))?;
    let feuille = project
        .tree()
        .children(classeur)?
        .first()
        .copied()
        .context("feuille du classeur d'analyse")?;
    let x = colonne_titree(project, feuille, &jeu.x, jeu.titres[0], ColumnRole::X)?;
    let y = colonne_titree(project, feuille, &jeu.y, jeu.titres[1], ColumnRole::Y)?;
    let sigma = jeu
        .sigma
        .as_ref()
        .map(|valeurs| {
            colonne_titree(
                project,
                feuille,
                valeurs,
                ("Incertitude", jeu.titres[1].1),
                ColumnRole::YError,
            )
        })
        .transpose()?;
    let reference = jeu
        .reference
        .as_ref()
        .map(|(rx, ry)| -> anyhow::Result<AuxRef> {
            let (titre_x, titre_y) = if matches!(jeu.operation, Operation::Correct { .. }) {
                (("Longueur d'onde (réf.)", "nm"), ("Référence", "u.a."))
            } else {
                (("Temps (IRF)", "ns"), ("Réponse de l'appareil", "u.a."))
            };
            Ok(AuxRef {
                x: Some(colonne_titree(
                    project,
                    feuille,
                    rx,
                    titre_x,
                    ColumnRole::X,
                )?),
                y: colonne_titree(project, feuille, ry, titre_y, ColumnRole::Y)?,
            })
        })
        .transpose()?;
    Ok(Colonnes {
        x,
        y,
        sigma,
        reference,
    })
}

/// Enregistre le calcul du jeu comme le fait `pyst` : `compute`, puis un
/// brouillon dont les colonnes de sortie portent le nom long, l'unité et le
/// commentaire que donne le cœur.
fn enregistre_calcul(project: &mut Project, jeu: &Jeu, colonnes: &Colonnes) -> anyhow::Result<()> {
    let series = jeu.series(colonnes);
    let calcule = compute(
        project.tree(),
        &jeu.operation,
        &series,
        colonnes.reference.as_ref(),
    )
    .with_context(|| format!("calcul de {}", jeu.nom))?;
    let kind = jeu.operation.kind();
    let outputs = calcule
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
            operation: jeu.operation.clone(),
            series,
            aux: colonnes.reference.clone(),
            outputs,
            results: calcule.results,
            keep_results: true,
            auto_recalc: jeu.operation.auto_recalc_by_default(1),
            low_r2_dialog: true,
        })
        .with_context(|| format!("enregistrement du calcul de {}", jeu.nom))?;
    Ok(())
}

/// Le dossier `Analyse` : un classeur par jeu, et le calcul de ceux qui l'ont.
pub(super) fn dossier(project: &mut Project) -> anyhow::Result<NodeId> {
    let racine = project.root();
    let dossier = project
        .add_folder(racine, "Analyse")
        .context("dossier Analyse")?;
    for jeu in jeux() {
        let colonnes = construit(project, dossier, &jeu)?;
        if jeu.calcule {
            enregistre_calcul(project, &jeu, &colonnes)?;
        }
    }
    Ok(dossier)
}
