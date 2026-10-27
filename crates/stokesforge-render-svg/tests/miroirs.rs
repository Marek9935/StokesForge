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
    // Les sous-graduations sont allumées par défaut (#344) : ces tests
    // comptent des tirets et partent d'une figure sans elles.
    for axe in [
        &mut panneau.bottom,
        &mut panneau.left,
        &mut panneau.top,
        &mut panneau.right,
    ] {
        axe.minor_ticks = false;
    }
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

/// Longueur d'un tiret.
fn longueur(s: &Segment) -> i32 {
    (s.2 - s.0).abs().max((s.3 - s.1).abs())
}

#[test]
fn les_sous_graduations_ajoutent_des_tirets_plus_courts_sans_bouger_les_principaux() {
    let sans = cadre_ferme("sous-sans", |_| {});
    let avec = cadre_ferme("sous-avec", |p| p.top.minor_ticks = true);
    let principaux = tirets_du_haut(&sans);
    let tous = tirets_du_haut(&avec);
    assert!(
        tous.len() > principaux.len(),
        "des sous-graduations s'ajoutent"
    );
    for tiret in &principaux {
        assert!(
            tous.contains(tiret),
            "le tiret principal {tiret:?} ne bouge pas"
        );
    }
    let longueur_principale = principaux.iter().map(longueur).max().expect("principal");
    for tiret in tous.iter().filter(|t| !principaux.contains(t)) {
        assert!(
            longueur(tiret) < longueur_principale,
            "{tiret:?} est plus court que les tirets principaux"
        );
        assert!(
            !principaux.iter().any(|p| p.0 == tiret.0),
            "une sous-graduation ne recouvre pas une graduation principale"
        );
    }
    // Seul le haut a changé : le bas et la gauche gardent leurs tirets.
    let haut = ligne_du_haut(&avec);
    let (_, avant) = lignes_et_tirets(&sans);
    let (_, apres) = lignes_et_tirets(&avec);
    for tiret in apres.iter().filter(|t| !avant.contains(t)) {
        assert!(
            tiret.1.min(tiret.3) <= haut && tiret.1.max(tiret.3) >= haut,
            "{tiret:?} n'est pas sur la ligne du haut"
        );
    }
}

#[test]
fn les_sous_graduations_du_bas_se_tracent_avec_les_tirets_de_plotters() {
    let sans = exporte("sous-bas-sans", |_| {});
    let avec = exporte("sous-bas-avec", |p| p.bottom.minor_ticks = true);
    let (_, avant) = lignes_et_tirets(&sans);
    let (_, apres) = lignes_et_tirets(&avec);
    assert!(apres.len() > avant.len());
    for tiret in &avant {
        assert!(apres.contains(tiret), "{tiret:?} est conservé");
    }
}

#[test]
fn les_sous_graduations_suivent_le_sens_des_tirets() {
    let haut = ligne_du_haut(&cadre_ferme("sous-sens-ref", |_| {}));
    let dedans = cadre_ferme("sous-sens-dedans", |p| {
        p.top.minor_ticks = true;
        p.top.tick_direction = TickDirection::In;
    });
    assert!(
        tirets_du_haut(&dedans)
            .iter()
            .all(|s| s.1.min(s.3) == haut && s.1.max(s.3) > haut),
        "toutes, sous-graduations comprises, vers l'intérieur"
    );
    let deux = cadre_ferme("sous-sens-deux", |p| {
        p.top.minor_ticks = true;
        p.top.tick_direction = TickDirection::Both;
    });
    assert!(tirets_du_haut(&deux)
        .iter()
        .all(|s| s.1.min(s.3) < haut && s.1.max(s.3) > haut));
}

#[test]
fn les_sous_graduations_se_tracent_seules_quand_les_tirets_principaux_sont_coupes() {
    let nu = cadre_ferme("sous-seules-ref", |p| p.top.ticks = false);
    let seules = cadre_ferme("sous-seules", |p| {
        p.top.ticks = false;
        p.top.minor_ticks = true;
    });
    let principaux = tirets_du_haut(&cadre_ferme("sous-seules-pleins", |_| {}));
    let longueur_principale = principaux.iter().map(longueur).max().expect("principal");
    assert!(
        tirets_du_haut(&nu).is_empty(),
        "ni principaux ni sous-tirets"
    );
    let petits = tirets_du_haut(&seules);
    assert!(!petits.is_empty(), "les sous-graduations restent");
    assert!(petits.iter().all(|t| longueur(t) < longueur_principale));
}

#[test]
fn un_axe_logarithmique_a_ses_sous_graduations_aux_multiples() {
    let sans = exporte("sous-log-sans", |p| {
        p.left.scale = stokesforge_plot::AxisScale::Log
    });
    let avec = exporte("sous-log-avec", |p| {
        p.left.scale = stokesforge_plot::AxisScale::Log;
        p.left.minor_ticks = true;
    });
    let ajoutes = lignes_et_tirets(&avec).1.len() - lignes_et_tirets(&sans).1.len();
    assert!(ajoutes >= 8, "au moins 2 à 9 sur une décade : {ajoutes}");
}

#[test]
fn un_axe_neuf_a_ses_sous_graduations() {
    let eteintes = exporte("sous-defaut-eteintes", |_| {});
    let neuf = exporte("sous-defaut-neuf", |p| {
        p.bottom.minor_ticks = PanelSpec::at(0, 0).bottom.minor_ticks;
    });
    assert!(PanelSpec::at(0, 0).bottom.minor_ticks);
    assert!(lignes_et_tirets(&neuf).1.len() > lignes_et_tirets(&eteintes).1.len());
}

#[test]
fn un_nombre_de_sous_graduations_fixe_le_nombre_de_petits_tirets() {
    let tirets = |nombre: Option<u8>| {
        lignes_et_tirets(&exporte("sous-nombre", |p| {
            p.bottom.minor_ticks = true;
            p.bottom.minor_tick_count = nombre;
        }))
        .1
        .len()
    };
    let sans = lignes_et_tirets(&exporte("sous-nombre-sans", |_| {}))
        .1
        .len();
    let un = tirets(Some(1)) - sans;
    let neuf = tirets(Some(9)) - sans;
    assert!(
        un > 0 && neuf > 5 * un,
        "1 par intervalle : {un}, 9 : {neuf}"
    );
}

/// Tirets du bas : verticaux, courts, dont une extrémité est la ligne du bas.
fn tirets_du_bas(svg: &str) -> Vec<Segment> {
    let (lignes, tirets) = lignes_et_tirets(svg);
    let bas = lignes
        .iter()
        .filter(|(_, y0, _, y1)| y0 == y1)
        .map(|s| s.1)
        .max()
        .expect("ligne du bas");
    tirets
        .into_iter()
        .filter(|s| s.0 == s.2 && s.1.min(s.3) <= bas && s.1.max(s.3) >= bas)
        .collect()
}

fn plus_long(tirets: &[Segment]) -> i32 {
    tirets.iter().map(longueur).max().expect("des tirets")
}

#[test]
fn un_grand_tiret_regle_a_la_main_change_la_figure_et_la_longueur() {
    let defaut = exporte("avance-longueur-defaut", |_| {});
    let long = exporte("avance-longueur-10", |p| p.bottom.tick_length = Some(10.0));
    assert_ne!(defaut, long, "la longueur change la figure");
    assert_eq!(plus_long(&tirets_du_bas(&defaut)), 5);
    assert_eq!(plus_long(&tirets_du_bas(&long)), 10);
}

#[test]
fn un_petit_tiret_automatique_est_la_moitie_du_grand_tiret_effectif() {
    let sous = |regle: fn(&mut PanelSpec)| {
        let svg = exporte("avance-petit", |p| {
            p.bottom.minor_ticks = true;
            p.bottom.tick_length = Some(10.0);
            regle(p);
        });
        let principaux = tirets_du_bas(&svg)
            .into_iter()
            .filter(|t| longueur(t) == 10)
            .count();
        assert!(principaux > 0);
        tirets_du_bas(&svg)
            .iter()
            .map(longueur)
            .filter(|&l| l != 10)
            .max()
            .expect("des petits tirets")
    };
    assert_eq!(sous(|_| {}), 5, "grand 10, petit Auto : 5");
    assert_eq!(
        sous(|p| p.bottom.minor_tick_length = Some(3.0)),
        3,
        "petit réglé à la main"
    );
}

/// Les polylignes du SVG : `(stroke, stroke-width, segment)`.
fn traits(svg: &str) -> Vec<(String, String, Segment)> {
    let attribut = |ligne: &str, nom: &str| {
        ligne
            .split(&format!("{nom}=\""))
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or_default()
            .to_string()
    };
    svg.lines()
        .filter(|l| l.starts_with("<polyline"))
        .filter_map(|l| {
            let seg = segments(l).first().copied()?;
            Some((attribut(l, "stroke"), attribut(l, "stroke-width"), seg))
        })
        .collect()
}

#[test]
fn la_couleur_des_tirets_ne_touche_ni_la_ligne_ni_les_nombres() {
    let svg = exporte("avance-couleur", |p| {
        p.bottom.tick_color = "#0000ff".into();
        p.bottom.tick_mark_color = "#ff0000".into();
    });
    let (lignes, tirets) = lignes_et_tirets(&svg);
    let bas = tirets_du_bas(&svg);
    assert!(!bas.is_empty());
    for (couleur, _, seg) in traits(&svg) {
        if bas.contains(&seg) {
            assert_eq!(couleur, "#FF0000", "tiret du bas {seg:?}");
        } else if lignes.contains(&seg) && seg.1 == seg.3 && seg.1 > 200 {
            assert_eq!(couleur, "#0000FF", "ligne du bas {seg:?}");
        }
    }
    assert!(!tirets.is_empty());
    assert!(
        svg.contains("fill=\"#0000FF\""),
        "les nombres gardent la couleur de l'échelle"
    );
    assert!(!svg.contains("fill=\"#FF0000\""), "aucun nombre rouge");
}

#[test]
fn l_epaisseur_des_tirets_ne_touche_pas_la_ligne_d_axe() {
    let svg = exporte("avance-epaisseur", |p| p.bottom.tick_width = Some(3.0));
    let bas = tirets_du_bas(&svg);
    for (_, epaisseur, seg) in traits(&svg) {
        if bas.contains(&seg) {
            assert_eq!(epaisseur, "3", "tiret du bas {seg:?}");
        } else if longueur(&seg) > 100 && seg.1 == seg.3 {
            assert_eq!(epaisseur, "1", "ligne {seg:?}");
        }
    }
}

#[test]
fn des_reglages_de_tirets_hors_bornes_ne_font_pas_planter_et_sont_bornes() {
    // Longueurs et épaisseurs venues d'un fichier ou d'un script : n'importe quoi.
    for (nom, longueur_demandee, epaisseur) in [
        ("nan", f64::NAN, f64::NAN),
        ("negatif", -3.0, 0.0),
        ("immense", 1e30, 1e9),
        ("infini", f64::INFINITY, f64::NEG_INFINITY),
    ] {
        let svg = exporte(&format!("avance-borne-{nom}"), |p| {
            p.bottom.minor_ticks = true;
            p.bottom.tick_length = Some(longueur_demandee);
            p.bottom.minor_tick_length = Some(longueur_demandee);
            p.bottom.tick_width = Some(epaisseur);
        });
        for (_, largeur, _) in traits(&svg) {
            let largeur: u32 = largeur.parse().expect("épaisseur entière");
            assert!((1..=64).contains(&largeur), "{nom} : épaisseur {largeur}");
        }
        for seg in tirets_du_bas(&svg) {
            assert!(longueur(&seg) <= 50, "{nom} : tiret {seg:?} trop long");
        }
    }
    // Une longueur négative n'est pas « Auto » mais zéro : aucun tiret.
    let nul = exporte("avance-borne-zero", |p| p.bottom.tick_length = Some(-3.0));
    assert!(tirets_du_bas(&nul).is_empty());
    // NaN se lit comme Auto.
    let defaut = exporte("avance-borne-defaut", |_| {});
    let nan = exporte("avance-borne-nan", |p| {
        p.bottom.tick_length = Some(f64::NAN)
    });
    assert_eq!(tirets_du_bas(&nan), tirets_du_bas(&defaut));
}
