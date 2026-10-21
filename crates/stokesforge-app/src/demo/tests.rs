//! Les tests de la démonstration. Chacun a son **témoin** : un cas voisin où la
//! même vérification doit échouer, pour qu'un test qui ne vérifie plus rien se
//! voie (le test lui-même a été éprouvé par mutation : graine changée, graphe
//! à référence brisée, jeu gaussien remplacé par une constante, recalcul
//! retiré de `session_for_project`).

use std::collections::BTreeSet;

use stokesforge_analysis::ops::{Operation, OperationKind};
use stokesforge_core::{
    compute, resolve_spec, Column, ColumnType, NodeId, NodeKind, Project, Tree,
};
use stokesforge_plot::{AxisScale, PlotSpec, PlotType, SeriesAxis};
use stokesforge_session::Session;

use super::analyse::{self, Attendu, Jeu};
use super::bruit::Bruit;
use super::demo_project;
use crate::session_for_project;

/// Les graphes du projet, `(nom, spec)`, dans l'ordre des noms.
fn graphes(project: &Project) -> Vec<(String, PlotSpec)> {
    let mut graphes: Vec<(String, PlotSpec)> = project
        .tree()
        .iter()
        .filter_map(|(_, node)| match node.kind() {
            NodeKind::Graph { spec, .. } => Some((node.name().to_string(), spec.clone())),
            _ => None,
        })
        .collect();
    graphes.sort_by(|a, b| a.0.cmp(&b.0));
    graphes
}

fn colonne_par_chemin(project: &Project, chemin: &str) -> Column {
    let id = project.resolve_path(chemin).unwrap();
    stokesforge_core::resolve_column(project.tree(), id)
        .unwrap()
        .clone()
}

fn nombres(colonne: &Column) -> Vec<f64> {
    (0..colonne.len())
        .map(|ligne| colonne.values().number_at(ligne))
        .collect()
}

#[test]
fn les_colonnes_de_demo_et_le_script_suivent_les_lettres_de_la_grille() {
    let project = demo_project().unwrap();
    for (sheet, names) in [
        ("Sheet1", &["A", "B", "C"][..]),
        ("Sheet2", &["A"][..]),
        ("Spectre", &["A", "B"][..]),
    ] {
        let id = project.resolve_path(&format!("[Book1]{sheet}")).unwrap();
        let actual: Vec<_> = project
            .tree()
            .children(id)
            .unwrap()
            .iter()
            .map(|id| project.tree().node(*id).unwrap().name())
            .collect();
        assert_eq!(actual, names);
    }
    let path = crate::EXAMPLE_SCRIPT.split('"').nth(1).unwrap();
    let id = project.resolve_path(path).unwrap();
    assert_eq!(id, project.resolve_path("[Book1]Sheet1!A").unwrap());
    let column = stokesforge_core::resolve_column(project.tree(), id).unwrap();
    assert_eq!(column.len(), 5);
}

/// Les noms de fenêtre (classeur, graphe, matrice, note) sont uniques dans tout
/// le projet : le chemin `[Nom]` les désigne sans dossier.
fn doublons(project: &Project) -> Vec<String> {
    let mut vus = BTreeSet::new();
    project
        .tree()
        .iter()
        .filter(|(_, node)| node.kind().is_window())
        .map(|(_, node)| node.name().to_string())
        .filter(|nom| !vus.insert(nom.clone()))
        .collect()
}

#[test]
fn les_noms_de_fenetre_sont_uniques_dans_tout_le_projet() {
    let project = demo_project().unwrap();
    assert_eq!(doublons(&project), Vec::<String>::new());

    // Témoin : un second « Types », même rangé dans un autre dossier, se voit.
    let mut projet = demo_project().unwrap();
    let racine = projet.root();
    projet
        .add_graph(racine, Some("Types"), PlotSpec::empty(""))
        .unwrap();
    assert_eq!(doublons(&projet), vec!["Types".to_string()]);
}

fn dessine(project: &Project, spec: &PlotSpec) -> stokesforge_render_egui::DrawnPlot {
    let measurer = stokesforge_render_egui::TextMeasurer::default();
    let rect = eframe::egui::Rect::from_min_size(
        eframe::egui::Pos2::ZERO,
        eframe::egui::vec2(640.0, 480.0),
    );
    stokesforge_render_egui::collect_shapes(
        &mut Vec::new(),
        rect,
        &measurer,
        &resolve_spec(project.tree(), spec),
    )
    .unwrap()
}

/// #103 : aucun graphe de la démo ne réserve de titre, comme les graphes créés
/// par l'utilisateur ; un titre explicite reste possible et se dessine.
#[test]
fn aucun_graphe_de_la_demo_ne_reserve_de_titre_mais_un_titre_explicite_reste_possible() {
    let project = demo_project().unwrap();
    let tous = graphes(&project);
    assert_eq!(tous.len(), 7, "Graph1 et les six graphes de démonstration");
    for (nom, spec) in &tous {
        assert!(
            spec.title.is_empty(),
            "{nom} : la démo ne donne pas de titre par défaut"
        );
        let dessin = dessine(&project, spec);
        assert_eq!(dessin.title_height, 0, "{nom}");
        assert!(dessin.title.is_none(), "{nom}");
    }

    // Témoin : le retrait du défaut ne désactive pas le rendu d'un titre choisi.
    let (_, graph1) = tous.iter().find(|(nom, _)| nom == "Graph1").unwrap();
    let sans = dessine(&project, graph1);
    let mut titre = graph1.clone();
    titre.title = "Spectre choisi".into();
    let avec = dessine(&project, &titre);
    assert!(avec.title_height > 0);
    assert!(avec.title.is_some());
    assert!(avec.panels[0].pixel_origin.1 > sans.panels[0].pixel_origin.1);
}

#[test]
fn chaque_graphe_se_resout_sans_reference_brisee_et_se_dessine() {
    let project = demo_project().unwrap();
    for (nom, spec) in graphes(&project) {
        let resolu = resolve_spec(project.tree(), &spec);
        assert_eq!(resolu.broken_series().count(), 0, "{nom}");
        assert!(resolu.point_count() > 0, "{nom} ne trace aucun point");
        dessine(&project, &spec);
    }

    // Témoin : une colonne supprimée casse la série qui la cite.
    let mut projet = demo_project().unwrap();
    let (_, spec) = graphes(&projet)
        .into_iter()
        .find(|(nom, _)| nom == "Panneaux")
        .unwrap();
    let victime: NodeId = spec.referenced_columns()[0];
    projet.remove(victime).unwrap();
    assert!(
        resolve_spec(projet.tree(), &spec).broken_series().count() > 0,
        "la détection d'une référence brisée ne voit plus rien"
    );
}

/// Les valeurs d'une colonne de série, par identifiant.
fn valeurs(tree: &Tree, colonne: NodeId) -> Vec<f64> {
    nombres(stokesforge_core::resolve_column(tree, colonne).unwrap())
}

/// Les séries d'un axe en échelle logarithmique qui portent une valeur ≤ 0 ou
/// non finie (invariant 10) : `(graphe, nom de série)`.
fn series_log_non_positives(tree: &Tree, nom: &str, spec: &PlotSpec) -> Vec<(String, String)> {
    let mut fautives = Vec::new();
    for panneau in spec.panels.as_slice() {
        for serie in &panneau.series {
            let (axe_y, axe_x) = (
                panneau.axis(serie.axis.side()).scale == AxisScale::Log,
                panneau.bottom.scale == AxisScale::Log,
            );
            let mut lues = Vec::new();
            if axe_y {
                lues.extend(valeurs(tree, serie.y));
            }
            if let (true, Some(x)) = (axe_x, serie.x) {
                lues.extend(valeurs(tree, x));
            }
            if lues.iter().any(|v| v.is_nan() || *v <= 0.0) {
                fautives.push((nom.to_string(), serie.name.clone()));
            }
        }
    }
    fautives
}

#[test]
fn les_axes_logarithmiques_ne_portent_que_des_donnees_strictement_positives() {
    let project = demo_project().unwrap();
    let mut echelles_log = 0;
    for (nom, spec) in graphes(&project) {
        echelles_log += spec
            .panels
            .as_slice()
            .iter()
            .filter(|p| p.left.scale == AxisScale::Log || p.bottom.scale == AxisScale::Log)
            .count();
        assert_eq!(
            series_log_non_positives(project.tree(), &nom, &spec),
            Vec::new()
        );
    }
    assert!(
        echelles_log > 0,
        "la démo montre au moins un axe logarithmique"
    );

    // Témoin : la première colonne de la cinétique (t = 0) contient des zéros,
    // et la vérification les trouve.
    let cinetique = project.resolve_path("[Book1]Cinetique").unwrap();
    let colonne = project.tree().children(cinetique).unwrap()[0];
    let mut spec = PlotSpec::empty("");
    let mut panneau = stokesforge_plot::PanelSpec::at(0, 0);
    panneau.left.scale = AxisScale::Log;
    panneau
        .series
        .push(stokesforge_plot::SeriesSpec::new(colonne));
    spec = spec.with_panels(panneau);
    assert_eq!(
        series_log_non_positives(project.tree(), "Témoin", &spec).len(),
        1
    );
}

/// Rejoue le calcul d'un jeu sur des colonnes neuves et rend ce qui s'écarte de
/// ce qu'il doit retrouver (vide : tout va bien).
fn ecarts(jeu: &Jeu) -> Vec<String> {
    let mut project = Project::new("Essai").unwrap();
    let racine = project.root();
    let colonnes = analyse::construit(&mut project, racine, jeu).unwrap();
    let calcule = match compute(
        project.tree(),
        &jeu.operation,
        &jeu.series(&colonnes),
        colonnes.reference.as_ref(),
    ) {
        Ok(calcule) => calcule,
        Err(erreur) => return vec![format!("{} : {erreur}", jeu.nom)],
    };
    let mut ecarts = Vec::new();
    for attendu in &jeu.attendus {
        match *attendu {
            Attendu::Valeur(cle, indice, valeur, tolerance) => {
                let lue = calcule.results[0]
                    .values
                    .iter()
                    .find(|v| v.key == cle && v.index == indice)
                    .map(|v| v.value);
                if !lue.is_some_and(|lue| (lue - valeur).abs() <= tolerance) {
                    ecarts.push(format!(
                        "{} : {cle} {indice:?} = {lue:?}, attendu {valeur} ± {tolerance}",
                        jeu.nom
                    ));
                }
            }
            Attendu::Ligne(cle, ligne, valeur, tolerance) => {
                let lue = calcule
                    .columns
                    .iter()
                    .find(|c| c.key == cle)
                    .and_then(|c| c.data.get(ligne).copied());
                if !lue.is_some_and(|lue| (lue - valeur).abs() <= tolerance) {
                    ecarts.push(format!(
                        "{} : {cle}[{ligne}] = {lue:?}, attendu {valeur} ± {tolerance}",
                        jeu.nom
                    ));
                }
            }
        }
    }
    ecarts
}

#[test]
fn chaque_jeu_d_analyse_retrouve_ses_parametres() {
    for jeu in analyse::jeux() {
        assert_eq!(ecarts(&jeu), Vec::<String>::new());
    }

    // Témoin : le jeu gaussien dont les données sont remplacées par une
    // constante ne retrouve plus ses paramètres (ou son ajustement échoue).
    let mut constant = analyse::jeux()
        .into_iter()
        .find(|j| j.nom == "FitGaussienne")
        .unwrap();
    constant.y = vec![1.0; constant.x.len()];
    assert!(!ecarts(&constant).is_empty());
}

#[test]
fn les_jeux_couvrent_toutes_les_familles_d_operation() {
    let familles = |jeux: &[Jeu]| -> BTreeSet<&'static str> {
        jeux.iter().map(|j| j.operation.kind().key()).collect()
    };
    let attendues: BTreeSet<&'static str> =
        OperationKind::ALL.iter().map(|kind| kind.key()).collect();
    assert_eq!(familles(&analyse::jeux()), attendues);

    // Témoin : sans le jeu de pics, la famille manque.
    let sans_pics: Vec<Jeu> = analyse::jeux()
        .into_iter()
        .filter(|j| !matches!(j.operation, Operation::Peaks { .. }))
        .collect();
    assert_ne!(familles(&sans_pics), attendues);
}

#[test]
fn les_jeux_sont_poses_dans_le_projet_et_certains_ont_leur_calcul() {
    let project = demo_project().unwrap();
    let tree = project.tree();
    for jeu in analyse::jeux() {
        let classeur = project.resolve_path(&format!("[{}]", jeu.nom)).unwrap();
        assert!(tree.node(classeur).is_some(), "{}", jeu.nom);
    }
    // Les calculs déjà faits : visibles dans le volet Results (résultats gardés).
    let faits = tree.calculations();
    let attendus = analyse::jeux().iter().filter(|j| j.calcule).count();
    assert_eq!(faits.len(), attendus);
    assert!(attendus >= 3);
    for calcul in faits.values() {
        assert!(calcul.keep_results);
        assert!(!calcul.results.is_empty());
        assert!(calcul.results.iter().all(|r| !r.values.is_empty()));
        assert!(calcul.live_outputs(tree).count() > 0);
        assert!(calcul.stale.is_none());
    }
}

#[test]
fn le_bruit_et_la_demo_sont_deterministes() {
    // Valeurs épinglées : changer la graine ou l'algorithme les déplace.
    let mut bruit = Bruit::new(7);
    let epingle: Vec<f64> = (0..3).map(|_| bruit.gauss(1.0)).collect();
    assert_eq!(epingle.len(), 3);
    for (lu, attendu) in epingle.iter().zip(EPINGLE_BRUIT) {
        assert!((lu - attendu).abs() < 1e-9, "{lu} ≠ {attendu}");
    }
    let premier = colonne_par_chemin(&demo_project().unwrap(), "[Mesures]Sheet1!B");
    let second = colonne_par_chemin(&demo_project().unwrap(), "[Mesures]Sheet1!B");
    assert_eq!(nombres(&premier), nombres(&second));
    assert!((nombres(&premier)[0] - EPINGLE_MESURE).abs() < 1e-9);

    // Témoin : une autre graine ne donne pas les mêmes valeurs.
    let mut autre = Bruit::new(8);
    let different: Vec<f64> = (0..3).map(|_| autre.gauss(1.0)).collect();
    assert!(different
        .iter()
        .zip(EPINGLE_BRUIT)
        .all(|(lu, attendu)| (lu - attendu).abs() > 1e-9));
}

const EPINGLE_BRUIT: [f64; 3] = [0.312_483_526_6, 0.594_715_800_3, -0.104_312_865_4];
const EPINGLE_MESURE: f64 = 94.427_690_068_6;

#[test]
fn le_demarrage_de_la_demo_recalcule_les_colonnes_a_formule() {
    let (session, _, _) = session_for_project(
        None,
        stokesforge_app_state::preferences::DEFAULT_MAX_FILE_BYTES,
    )
    .unwrap();
    let instantane = session.handle().snapshot().unwrap();
    let tree = instantane.tree;
    let colonne = |nom: &str| -> Vec<f64> {
        let types = project_feuille(&tree, "Types");
        let id = tree.child_by_name(types, nom).unwrap().unwrap();
        valeurs(&tree, id)
    };
    // F = A*2+1 sur A = 0, 1, 2 …
    let f = colonne("F");
    assert_eq!(f.len(), 12);
    assert_eq!(&f[..3], &[1.0, 3.0, 5.0]);
    // G = (B-A)/2 : NaN là où B est manquante.
    let g = colonne("G");
    assert_eq!(g[0], 10.25);
    assert!(g[2].is_nan());
    // H = D+1 lit la colonne mixte : un nombre donne un nombre, un texte NaN.
    let h = colonne("H");
    assert_eq!(h[0], 2.5);
    assert!(h[1].is_nan());
    session.shutdown().unwrap();

    // Témoin : sans session ni recalcul, la colonne à formule est vide — c'est
    // ce que verrait l'utilisateur si `recalculate` sortait de
    // `session_for_project`.
    let brut = demo_project().unwrap();
    assert_eq!(colonne_par_chemin(&brut, "[Types]Sheet1!F").len(), 0);
}

/// La première feuille du classeur `nom`.
fn project_feuille(tree: &Tree, nom: &str) -> NodeId {
    let classeur = tree
        .iter()
        .find(|(_, node)| node.name() == nom && node.kind().is_window())
        .map(|(id, _)| id)
        .unwrap();
    tree.children(classeur).unwrap()[0]
}

#[test]
fn la_demo_fait_l_aller_retour_stf_par_la_session() {
    let dossier = tempfile::tempdir().unwrap();
    let chemin = dossier.path().join("demo.stf");
    let (session, _, _) = session_for_project(
        None,
        stokesforge_app_state::preferences::DEFAULT_MAX_FILE_BYTES,
    )
    .unwrap();
    let avant = session.handle().snapshot().unwrap();
    session.handle().save(&chemin).unwrap();
    session.shutdown().unwrap();

    let relue = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let (_, rapport) = relue
        .handle()
        .load_with_limit(
            &chemin,
            stokesforge_app_state::preferences::DEFAULT_MAX_FILE_BYTES,
        )
        .unwrap();
    assert!(rapport.is_clean(), "{rapport:?}");
    let apres = relue.handle().snapshot().unwrap();
    assert_eq!(apres.tree.iter().count(), avant.tree.iter().count());

    let comparer = |tree: &Tree| -> Vec<(String, PlotSpec)> {
        let mut graphes: Vec<(String, PlotSpec)> = tree
            .iter()
            .filter_map(|(_, node)| match node.kind() {
                NodeKind::Graph { spec, .. } => Some((node.name().to_string(), spec.clone())),
                _ => None,
            })
            .collect();
        graphes.sort_by(|a, b| a.0.cmp(&b.0));
        graphes
    };
    let (gauche, droite) = (comparer(&avant.tree), comparer(&apres.tree));
    assert_eq!(gauche.len(), 7);
    assert_eq!(gauche.len(), droite.len());
    for ((nom, a), (autre, b)) in gauche.iter().zip(&droite) {
        assert_eq!(nom, autre);
        assert_eq!(a, b, "le graphe {nom} a changé à l'aller-retour");
    }
    relue.shutdown().unwrap();

    // Témoin : la comparaison voit une différence de spec.
    let mut modifie = gauche[0].1.clone();
    modifie.background = "#123456".into();
    assert_ne!(modifie, droite[0].1);
}

#[test]
fn la_demo_couvre_tous_les_types_de_trace_et_les_reglages_de_tracé() {
    let types_couverts = |project: &Project| -> Vec<PlotType> {
        let mut types: Vec<PlotType> = Vec::new();
        for (_, spec) in graphes(project) {
            for (_, serie) in spec.iter_series() {
                if !types.contains(&serie.plot_type) {
                    types.push(serie.plot_type.clone());
                }
            }
        }
        types
    };
    let attendus = [
        PlotType::Scatter,
        PlotType::Line,
        PlotType::LineScatter,
        PlotType::Bar,
        PlotType::Histogram,
        PlotType::ErrorBar,
    ];
    let couverts = types_couverts(&demo_project().unwrap());
    for attendu in &attendus {
        assert!(couverts.contains(attendu), "{attendu:?} absent de la démo");
    }
    assert!(!couverts.iter().any(|t| matches!(t, PlotType::Unknown(_))));

    // Témoin : `Book1` et `Graph1` seuls ne couvrent pas tout.
    let mut mince = Project::new("Mince").unwrap();
    let book1 = super::classeur::book1(&mut mince).unwrap();
    super::graphes::graph1(&mut mince, &book1).unwrap();
    assert!(!attendus.iter().all(|t| types_couverts(&mince).contains(t)));

    // Les réglages promis : barres d'erreur, axe de droite, calque, groupe,
    // séries « ligne », annotations, quatre formes.
    let specs: Vec<PlotSpec> = graphes(&demo_project().unwrap())
        .into_iter()
        .map(|(_, spec)| spec)
        .collect();
    let panneaux = || specs.iter().flat_map(|s| s.panels.as_slice().iter());
    assert!(panneaux()
        .flat_map(|p| &p.series)
        .any(|s| s.y_error.is_some()));
    assert!(panneaux()
        .flat_map(|p| &p.series)
        .any(|s| s.axis == SeriesAxis::Right));
    assert!(panneaux().any(|p| p.overlay == Some(stokesforge_plot::OverlaySpec::SECOND_AXES)));
    assert!(panneaux().any(|p| !p.group_ids().is_empty() && !p.group_gradients.is_empty()));
    assert!(panneaux().flat_map(|p| &p.series).any(|s| s.row.is_some()));
    assert!(panneaux().any(|p| p.annotations.iter().any(|a| a.text.contains('$'))));
    let formes: BTreeSet<String> = panneaux()
        .flat_map(|p| &p.shapes)
        .map(|f| format!("{:?}", f.kind))
        .collect();
    assert_eq!(formes.len(), 4);
}

#[test]
fn les_types_de_colonne_de_la_demo_sont_tous_la() {
    let project = demo_project().unwrap();
    let types = |chemin: &str| colonne_par_chemin(&project, chemin).column_type();
    assert_eq!(types("[Types]Sheet1!A"), ColumnType::Numeric);
    assert_eq!(types("[Types]Sheet1!C"), ColumnType::Mixed);
    assert_eq!(types("[Types]Sheet1!D"), ColumnType::Mixed);
    assert_eq!(types("[Types]Sheet1!E"), ColumnType::DateTime);
    let temperature = colonne_par_chemin(&project, "[Types]Sheet1!B");
    assert!(nombres(&temperature).iter().any(|v| v.is_nan()));
    let meta = temperature.meta();
    assert!(
        !meta.unit.is_empty()
            && !meta.long_name.is_empty()
            && !meta.comment.is_empty()
            && !meta.display_format.is_empty()
    );
    for lettre in ["F", "G", "H"] {
        assert!(
            colonne_par_chemin(&project, &format!("[Types]Sheet1!{lettre}"))
                .meta()
                .formula
                .is_some()
        );
    }
}
