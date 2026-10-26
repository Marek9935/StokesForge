//! Axes miroirs (#176) vus dans l'export : l'échelle du haut et de droite
//! reprend celle du bas et de gauche, sans titre, et peut se limiter au trait.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec, TickDirection};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn exporte(nom: &str, regle: impl FnOnce(&mut PanelSpec)) -> String {
    let mut project = Project::new("Miroirs").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let mut colonne = |nom: &str, valeurs: Vec<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(valeurs),
            ColumnMeta::default(),
        )
        .expect("colonne");
        project
            .add_column(sheet, None, Some(nom), column)
            .expect("colonne")
    };
    let x = colonne("X", vec![0.0, 400.0, 800.0]);
    let y = colonne("Y", vec![10.0, 20.0, 30.0]);
    let mut panneau = PanelSpec::at(0, 0);
    panneau.bottom.label = "Temps".into();
    panneau.series.push(SeriesSpec::new_xy(x, y));
    regle(&mut panneau);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panneau]));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(format!("{nom}.svg")), &svg).ok();
    }
    svg
}

fn occurrences(svg: &str, graduation: &str) -> usize {
    svg.matches(&format!("\n{graduation}\n")).count()
}

#[test]
fn les_axes_miroirs_repetent_les_graduations_sans_titre() {
    let sans = exporte("miroirs-sans", |_| {});
    let avec = exporte("miroirs-avec", |p| {
        p.top.mirror = true;
        p.right.mirror = true;
    });
    assert_eq!(occurrences(&avec, "800"), 2 * occurrences(&sans, "800"));
    assert_eq!(occurrences(&avec, "30"), 2 * occurrences(&sans, "30"));
    assert_eq!(
        avec.matches("Temps").count(),
        sans.matches("Temps").count(),
        "aucun titre sur les miroirs"
    );
}

#[test]
fn un_miroir_au_trait_seul_garde_les_tirets_sans_les_nombres() {
    let nus = exporte("miroirs-nus", |p| {
        p.top.mirror = true;
        p.top.tick_labels = false;
        p.right.mirror = true;
        p.right.tick_labels = false;
    });
    let sans = exporte("miroirs-sans", |_| {});
    assert_eq!(occurrences(&nus, "800"), occurrences(&sans, "800"));
    assert_eq!(occurrences(&nus, "30"), occurrences(&sans, "30"));
}

/// Un segment d'un trait de deux points : `(x0, y0, x1, y1)`.
type Segment = (i32, i32, i32, i32);

fn segments(svg: &str) -> Vec<Segment> {
    svg.lines()
        .filter(|ligne| ligne.starts_with("<polyline"))
        .filter_map(|ligne| {
            let points = ligne.split("points=\"").nth(1)?.split('"').next()?;
            let nombres: Vec<i32> = points
                .split([' ', ','])
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse::<f64>().ok().map(|v| v as i32))
                .collect();
            (nombres.len() == 4).then(|| (nombres[0], nombres[1], nombres[2], nombres[3]))
        })
        .collect()
}

/// Lignes d'axe (longs traits horizontaux ou verticaux) et tirets (courts).
fn lignes_et_tirets(svg: &str) -> (Vec<Segment>, Vec<Segment>) {
    segments(svg).into_iter().partition(|(x0, y0, x1, y1)| {
        (x1 - x0).abs().max((y1 - y0).abs()) > 100 && (x0 == x1 || y0 == y1)
    })
}

/// Ordonnée de la ligne d'axe du haut, et son abscisse de départ.
fn ligne_du_haut(svg: &str) -> i32 {
    let (lignes, _) = lignes_et_tirets(svg);
    lignes
        .iter()
        .filter(|(_, y0, _, y1)| y0 == y1)
        .map(|s| s.1)
        .min()
        .expect("ligne du haut")
}

fn cadre_ferme(nom: &str, regle: impl FnOnce(&mut PanelSpec)) -> String {
    exporte(nom, |p| {
        p.top.mirror = true;
        p.right.mirror = true;
        regle(p);
    })
}

/// Tirets du haut : verticaux, courts, dont une extrémité est la ligne du haut.
fn tirets_du_haut(svg: &str) -> Vec<Segment> {
    let haut = ligne_du_haut(svg);
    let (_, tirets) = lignes_et_tirets(svg);
    tirets
        .into_iter()
        .filter(|s| s.0 == s.2 && s.0 > 76)
        .filter(|s| s.1.min(s.3) <= haut && s.1.max(s.3) >= haut)
        .collect()
}

#[test]
fn un_trait_nu_garde_la_ligne_sans_tirets_ni_nombres() {
    let ferme = cadre_ferme("trait-nu-ref", |_| {});
    let nu = cadre_ferme("trait-nu", |p| {
        for axe in [&mut p.top, &mut p.right] {
            axe.ticks = false;
            axe.tick_labels = false;
        }
    });
    let sans = exporte("trait-nu-sans", |_| {});
    let (lignes_nu, tirets_nu) = lignes_et_tirets(&nu);
    let (lignes_ferme, tirets_ferme) = lignes_et_tirets(&ferme);
    assert_eq!(
        lignes_nu.len(),
        lignes_ferme.len(),
        "les quatre lignes restent"
    );
    assert_eq!(lignes_nu.len(), 4);
    // Seuls le bas et la gauche gardent leurs tirets.
    assert_eq!(tirets_nu.len(), lignes_et_tirets(&sans).1.len());
    assert!(tirets_ferme.len() > tirets_nu.len());
    assert_eq!(occurrences(&nu, "800"), occurrences(&sans, "800"));
    assert_eq!(occurrences(&nu, "30"), occurrences(&sans, "30"));
}

#[test]
fn les_tirets_vers_l_interieur_et_des_deux_cotes_suivent_la_zone_de_trace() {
    let defaut = cadre_ferme("tirets-defaut", |_| {});
    let dehors = tirets_du_haut(&defaut);
    let haut = ligne_du_haut(&defaut);
    assert!(!dehors.is_empty());
    assert!(
        dehors.iter().all(|s| s.1.max(s.3) == haut),
        "par défaut, vers l'extérieur : au-dessus de la ligne du haut"
    );

    let dedans = cadre_ferme("tirets-dedans", |p| {
        p.top.tick_direction = TickDirection::In
    });
    let tirets = tirets_du_haut(&dedans);
    assert_eq!(tirets.len(), dehors.len());
    assert!(
        tirets
            .iter()
            .all(|s| s.1.min(s.3) == haut && s.1.max(s.3) > haut),
        "vers l'intérieur : sous la ligne du haut, dans la zone de tracé"
    );

    let deux = cadre_ferme("tirets-deux", |p| {
        p.top.tick_direction = TickDirection::Both
    });
    let tirets = tirets_du_haut(&deux);
    assert_eq!(tirets.len(), dehors.len());
    assert!(
        tirets
            .iter()
            .all(|s| s.1.min(s.3) < haut && s.1.max(s.3) > haut),
        "des deux côtés : de part et d'autre de la ligne"
    );
    // La moitié extérieure est exactement le tiret de Plotters.
    for sortant in &dehors {
        assert!(
            tirets
                .iter()
                .any(|s| s.0 == sortant.0 && s.1.min(s.3) == sortant.1.min(sortant.3)),
            "le tiret {sortant:?} se retrouve dans « des deux côtés »"
        );
    }
}

#[test]
fn les_nombres_gardent_leur_place_quand_les_tirets_changent() {
    let defaut = cadre_ferme("nombres-defaut", |_| {});
    let sans = cadre_ferme("nombres-sans-tirets", |p| {
        p.top.ticks = false;
        p.right.ticks = false;
    });
    // Mêmes textes, aux mêmes positions : seuls les tirets disparaissent.
    let textes = |svg: &str| -> Vec<String> {
        svg.lines()
            .filter(|l| l.starts_with("<text"))
            .map(String::from)
            .collect()
    };
    assert_eq!(textes(&defaut), textes(&sans));
    assert!(tirets_du_haut(&sans).is_empty());
}

#[test]
fn un_axe_aux_tirets_par_defaut_garde_ceux_de_plotters_a_cote_d_un_axe_modifie() {
    let defaut = cadre_ferme("voisin-defaut", |_| {});
    let mixte = cadre_ferme("voisin-mixte", |p| p.top.tick_direction = TickDirection::In);
    // Les tirets de droite (courts traits horizontaux dans la bande de droite)
    // sont tracés par le pipeline dans un cas et par Plotters dans l'autre :
    // ils doivent se superposer au pixel.
    let droite = |svg: &str| -> Vec<Segment> {
        let (_, tirets) = lignes_et_tirets(svg);
        let mut v: Vec<Segment> = tirets
            .into_iter()
            .filter(|s| s.1 == s.3 && s.0 >= 400)
            .collect();
        v.sort_unstable();
        v
    };
    assert!(!droite(&defaut).is_empty());
    assert_eq!(droite(&defaut), droite(&mixte));
}
