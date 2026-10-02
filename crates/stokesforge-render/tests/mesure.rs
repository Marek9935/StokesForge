//! Tâche R4.1 — ce qui verrouille la source unique de mesure.
//!
//! Trois propriétés, et une seule d'entre elles était évidente d'avance :
//!
//! 1. deux [`TextMeasurer`] distincts mesurent **identiquement**, parce
//!    qu'ils partent des mêmes `FontDefinitions`. C'est la précision 1 de la
//!    mesure R0 0.1 remise en dépôt : le danger n'est pas « deux instances »,
//!    c'est « deux jeux de polices » ;
//! 2. la mesure ne dépend pas du `pixels_per_point`. Si elle en dépendait,
//!    la figure exportée (à 1,0) et la figure écrite à l'écran (à 1,5 ou 2,0
//!    selon l'écran de l'utilisateur) n'auraient pas les mêmes marges — et
//!    rien ne le dirait ;
//! 3. la mesure est **différente** de l'heuristique naïve de Plotters. Sans
//!    ce témoin, les deux premiers tests passeraient encore si les deux
//!    chemins retombaient tous deux sur le défaut du trait, c'est-à-dire
//!    exactement le bug que la phase doit empêcher.
//!
//! Le jeu de chaînes est celui du spike R0 : des graduations réelles, des
//! unités, et du non-ASCII là où l'heuristique par octets se trompe le plus.

use plotters_backend::{text_anchor::Pos, BackendColor, BackendTextStyle, FontFamily, FontStyle};
use stokesforge_render::text::{estimate, TextMeasurer};

/// Un style de texte minimal, suffisant pour `estimate_text_size`.
///
/// `BackendTextStyle` est implémenté par `plotters::style::TextStyle`, mais
/// le construire ferait dépendre ce test de la façon dont le pipeline choisit
/// ses polices. On veut mesurer la mesure, pas le pipeline.
struct Style {
    family: &'static str,
    size: f64,
}

#[derive(Debug)]
struct JamaisDErreur;

impl std::fmt::Display for JamaisDErreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "jamais")
    }
}

impl std::error::Error for JamaisDErreur {}

impl BackendTextStyle for Style {
    type FontError = JamaisDErreur;

    fn color(&self) -> BackendColor {
        BackendColor {
            alpha: 1.0,
            rgb: (0, 0, 0),
        }
    }

    fn size(&self) -> f64 {
        self.size
    }

    fn style(&self) -> FontStyle {
        FontStyle::Normal
    }

    fn anchor(&self) -> Pos {
        Pos::default()
    }

    fn family(&self) -> FontFamily<'_> {
        FontFamily::from(self.family)
    }

    fn layout_box(&self, _text: &str) -> Result<((i32, i32), (i32, i32)), Self::FontError> {
        Err(JamaisDErreur)
    }

    fn draw<E, F: FnMut(i32, i32, BackendColor) -> Result<(), E>>(
        &self,
        _text: &str,
        _pos: (i32, i32),
        _draw: F,
    ) -> Result<Result<(), E>, Self::FontError> {
        Err(JamaisDErreur)
    }
}

/// Les chaînes du spike R0 : graduations, unités, notations scientifiques.
const CHAINES: &[&str] = &[
    "0",
    "0,5",
    "111",
    "WWW",
    "-1,25e-9",
    "λ_max",
    "µm",
    "cm⁻¹",
    "Temps (s)",
    "Intensité (u.a.)",
    "1 000 000",
];

const TAILLES: &[f64] = &[12.0, 14.0, 20.0, 30.0];
const FAMILLES: &[&str] = &["sans-serif", "serif", "monospace"];

fn toutes_les_mesures(measurer: &TextMeasurer) -> Vec<(u32, u32)> {
    let mut mesures = Vec::new();
    for famille in FAMILLES {
        for taille in TAILLES {
            for texte in CHAINES {
                mesures.push(estimate(
                    measurer,
                    texte,
                    &Style {
                        family: famille,
                        size: *taille,
                    },
                ));
            }
        }
    }
    mesures
}

#[test]
fn deux_instances_de_fonts_mesurent_a_lidentique() {
    let un = TextMeasurer::new(1.0);
    let deux = TextMeasurer::new(1.0);
    assert_eq!(
        toutes_les_mesures(&un),
        toutes_les_mesures(&deux),
        "deux `Fonts` construites sur les mêmes `FontDefinitions` doivent \
         mesurer à l'identique — sinon partager la définition ne suffit pas \
         et il faut revenir à un `Mutex<Fonts>` unique (architecture §4.3)"
    );
}

#[test]
fn la_mesure_ne_depend_pas_du_pixels_per_point() {
    let reference = toutes_les_mesures(&TextMeasurer::new(1.0));
    for ppp in [1.25_f32, 1.5, 2.0, 3.0] {
        assert_eq!(
            toutes_les_mesures(&TextMeasurer::new(ppp)),
            reference,
            "la mesure a bougé à pixels_per_point = {ppp} : l'export (1,0) \
             et l'écran (souvent 1,5 ou 2,0) ne réserveraient plus les mêmes \
             marges"
        );
    }
}

#[test]
fn la_mesure_nest_pas_lheuristique_naive_de_plotters() {
    // Le témoin. Reproduit `plotters/src/style/font/naive.rs` :
    // `em = size / 1.24 / 1.24`, largeur ≈ `em * 0,7 * nombre d'octets`.
    fn naive(text: &str, size: f64) -> u32 {
        let em = size / 1.24 / 1.24;
        (em * 0.7 * text.len() as f64).round() as u32
    }

    let measurer = TextMeasurer::new(1.0);
    let mesure = |texte: &str| {
        estimate(
            &measurer,
            texte,
            &Style {
                family: "sans-serif",
                size: 20.0,
            },
        )
        .0
    };

    // « 111 » et « WWW » ont le même nombre d'octets : l'heuristique naïve
    // leur donne la même largeur, une vraie mesure non.
    assert_eq!(naive("111", 20.0), naive("WWW", 20.0));
    assert_ne!(
        mesure("111"),
        mesure("WWW"),
        "la mesure traite « 111 » et « WWW » comme du monospace : la \
         surcharge d'estimate_text_size n'est pas en vigueur"
    );

    // « λ » fait deux octets et un seul glyphe : l'heuristique le compte
    // double.
    assert!(
        mesure("λ") < naive("λ", 20.0),
        "« λ » mesuré {} px, heuristique naïve {} px — la mesure devrait être \
         nettement plus étroite",
        mesure("λ"),
        naive("λ", 20.0)
    );
}

#[test]
fn la_hauteur_ne_depend_pas_du_contenu_pour_une_meme_taille() {
    // Propriété utilisée par le pipeline : `DrawingArea::titled` fait
    // descendre la zone de tracé de la hauteur du titre. Si cette hauteur
    // variait avec le texte, deux graphes de titres différents n'auraient
    // pas la même zone — ce qui est *correct*, mais il faut le savoir avant
    // d'écrire un test d'accord qui compare deux figures de titres
    // différents. Elle ne varie pas : epaint rend une hauteur de ligne.
    let measurer = TextMeasurer::new(1.0);
    let hauteur = |texte: &str| {
        estimate(
            &measurer,
            texte,
            &Style {
                family: "sans-serif",
                size: 24.0,
            },
        )
        .1
    };
    let reference = hauteur("Titre");
    for texte in CHAINES {
        assert_eq!(
            hauteur(texte),
            reference,
            "hauteur variable sur « {texte} »"
        );
    }
}
