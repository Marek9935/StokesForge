//! Déterminisme des résultats d'analyse entre plateformes (#332).
//!
//! Les fonctions transcendantes de `f64` délèguent à la libm de la plateforme
//! (CRT MSVC sous Windows, glibc sous Linux), qui diffèrent au dernier ulp ;
//! une décomposition en pics itérative convergeait sous Linux et pas sous
//! Windows (#290). `stokesforge-analysis` passe donc par le crate `libm` (Rust
//! pur), gardé par `clippy.toml`. Les bits figés ci-dessous valent pour toutes
//! les plateformes ; la CI Linux les confirme.

use stokesforge_analysis::error::AnalysisError;
use stokesforge_analysis::expr::Expr;
use stokesforge_analysis::ops::{run, AutoDetect, Operation, SeriesInput};
use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};

/// La série en dents de scie de #290 : 40 points, x = 0..39, y = 0 aux lignes
/// paires et 100 aux lignes impaires.
fn dents_de_scie() -> SeriesInput {
    SeriesInput {
        x: (0..40).map(f64::from).collect(),
        y: (0..40)
            .map(|i| if i % 2 == 0 { 0.0 } else { 100.0 })
            .collect(),
        sigma: None,
    }
}

/// Des paliers de cinq lignes à 0 puis à 100 (R² faible) : la décomposition
/// en pics converge. C'est la série du test d'état #290 depuis #332.
fn paliers() -> SeriesInput {
    SeriesInput {
        x: (0..40).map(f64::from).collect(),
        y: (0..40)
            .map(|i| if (i / 5) % 2 == 0 { 0.0 } else { 100.0 })
            .collect(),
        sigma: None,
    }
}

fn decomposition_en_pics() -> Operation {
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

fn evalue(src: &str, x: f64, p: &[f64]) -> f64 {
    Expr::parse(src).unwrap().eval(x, p)
}

/// Relevé de la décomposition en pics des paliers : (clé, indice, valeur,
/// incertitude), en bits `f64`.
#[allow(clippy::type_complexity)]
const PALIERS_ATTENDUS: [(&str, Option<usize>, u64, Option<u64>); 20] = [
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
fn la_decomposition_en_pics_des_paliers_donne_les_memes_bits_partout() {
    let sorties = run(&decomposition_en_pics(), &[paliers()], None).unwrap();
    let obtenu: Vec<(&str, Option<usize>, u64, Option<u64>)> = sorties[0]
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
    assert_eq!(obtenu, PALIERS_ATTENDUS);
}

/// La dent de scie de #290 ne converge pas en 400 itérations avec `libm`, et
/// c'est vrai sur toutes les plateformes : le test d'état #290 utilise donc les
/// paliers, qui convergent.
#[test]
fn la_dent_de_scie_ne_converge_sur_aucune_plateforme() {
    let erreur = run(&decomposition_en_pics(), &[dents_de_scie()], None).unwrap_err();
    assert!(
        matches!(erreur, AnalysisError::NotConverged { iterations: 400 }),
        "{erreur:?}"
    );
}

#[test]
fn les_fonctions_d_expression_donnent_les_memes_bits_partout() {
    let attendus: [(&str, f64, &[f64], u64); 5] = [
        ("exp(x)", 0.5, &[], 0x3ffa_6129_8e1e_069c),
        ("ln(x)", 3.0, &[], 0x3ff1_93ea_7aad_030a),
        ("sin(x)", 1e6, &[], 0xbfd6_664b_2568_d867),
        ("pow(x, a)", 2.5, &[1.7], 0x4012_fdcf_53f3_e6f3),
        ("atan2(x, a)", 1.0, &[-3.0], 0x4006_8f09_5fdf_593c),
    ];
    for (source, x, p, bits) in attendus {
        let obtenu = evalue(source, x, p);
        assert_eq!(obtenu.to_bits(), bits, "{source} en {x} : {obtenu:?}");
    }
}

/// Empreinte FNV-1a des bits d'une fonction balayée sur 256 abscisses : un
/// seul point proche d'une arête d'arrondi suffit à la faire bouger, ce que
/// cinq valeurs isolées ne garantissent pas.
fn empreinte(source: &str, p: &[f64], abscisse: impl Fn(usize) -> f64) -> u64 {
    (0..256).fold(0xcbf2_9ce4_8422_2325_u64, |h, i| {
        let bits = evalue(source, abscisse(i), p).to_bits();
        (h ^ bits).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn le_balayage_des_fonctions_d_expression_donne_les_memes_bits_partout() {
    let pas = |i: usize| (i as f64 - 128.0) * 0.173 + 0.011;
    let positif = |i: usize| 0.0137 * (i as f64 + 1.0);
    let obtenu = [
        ("exp(x)", empreinte("exp(x)", &[], pas)),
        ("ln(x)", empreinte("ln(x)", &[], positif)),
        ("sin(x)", empreinte("sin(x)", &[], |i| pas(i) * 997.0)),
        ("pow(x, a)", empreinte("pow(x, a)", &[1.7], positif)),
        ("atan2(x, a)", empreinte("atan2(x, a)", &[-3.0], pas)),
        ("tanh(x)", empreinte("tanh(x)", &[], pas)),
    ];
    let attendu = [
        ("exp(x)", 0xb645_35ea_6664_ce0b),
        ("ln(x)", 0x065c_a518_45f6_7c01),
        ("sin(x)", 0x8924_bcd5_724e_304d),
        ("pow(x, a)", 0x6c99_0481_a84f_e677),
        ("atan2(x, a)", 0xab38_4b41_92ae_7740),
        ("tanh(x)", 0x1b6c_37db_5200_3baa),
    ];
    assert_eq!(obtenu, attendu);
}
