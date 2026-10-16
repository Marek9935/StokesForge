//! Binaire du banc d'essai : démarre l'acteur de session, construit l'état et
//! ouvre la fenêtre.
//!
//! Usage : `stokesforge-app [chemin]`. Si `chemin` existe, le projet est chargé
//! depuis ce fichier ; sinon un projet de démonstration est construit et
//! `chemin` devient simplement la cible d'un futur enregistrement.

use std::path::PathBuf;

use anyhow::Context as _;
use stokesforge_app::app::StokesForgeApp;
use stokesforge_app_state::state::AppState;
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_session::Session;

/// Nom du fichier proposé par défaut quand aucun chemin n'est donné.
const DEFAULT_FILE: &str = "demo.stf";

/// Script d'exemple qui pré-remplit la zone de script : il fonctionne tel
/// quel sur le projet de démonstration ([`demo_project`]) et illustre le
/// critère de fin du socle (lecture sans copie d'une colonne). Affiché même
/// sans la feature `python` : un banc d'essai doit montrer ce qui manque, pas
/// le cacher.
const EXAMPLE_SCRIPT: &str = "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
v = pyst.column(a)\n\
print(\"longueur :\", len(v), \"premier :\", v[0])\n";

fn main() -> anyhow::Result<()> {
    let requested = std::env::args().nth(1).map(PathBuf::from);
    let (session, path) = session_for_project(requested)?;
    let mut state = AppState::new(session.handle(), path.display().to_string())
        .context("premier instantané du projet")?;
    state.set_script_source(EXAMPLE_SCRIPT.to_string());
    configure_script_engine(&mut state);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(state.window_title())
            .with_inner_size([1100.0, 700.0]),
        event_loop_builder: Some(Box::new(configure_window_backend)),
        // egui-wgpu met par défaut deux images en file d'attente (débit
        // maximal) ; une seule (faible latence) retire une image de retard
        // entre le pointeur et ce qui est peint — sensible au déplacement
        // d'une fenêtre, que rien ne masque. Le coût d'une image est ~0,3 ms
        // en release (`mesure_deplacement_de_fenetre`) : le débit n'est pas
        // ce qui manque.
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default()
            .with_surface_config(eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY),
        ..Default::default()
    };
    // `run_native` bloque jusqu'à la fermeture de la fenêtre ; `session` reste
    // donc vivante pendant toute la durée de l'application.
    let outcome = eframe::run_native(
        "stokesforge-app",
        options,
        Box::new(|cc| {
            // Tâche R4.1 : poser explicitement **les** définitions de police
            // du projet sur le contexte, plutôt que de compter sur le fait
            // qu'egui a par défaut les mêmes. C'est ce qui fait de l'accord
            // écran/export une propriété et non une coïncidence — si egui
            // changeait son défaut, la figure resterait la même des deux
            // côtés.
            cc.egui_ctx
                .set_fonts(stokesforge_render_egui::font_definitions());
            // Thème maison, clair, indépendant du thème système (voir la doc
            // de tête de `stokesforge_app::theme`) — posé ici, une seule fois, aux
            // côtés des polices : les deux relèvent du même geste (« ce que
            // l'appli impose sur le `Context` avant le premier rendu »).
            stokesforge_app::theme::install(&cc.egui_ctx);
            let mut app = StokesForgeApp::new(state);
            app.load_toolbar_layout(&stokesforge_app_state::preferences::Preferences::load());
            Ok(Box::new(app))
        }),
    );

    // L'arrêt propre de l'acteur passe avant la remontée d'une erreur de
    // fenêtre : sans cela, le thread survivrait à la sortie du processus.
    let shutdown = session.shutdown();
    outcome.map_err(|err| anyhow::anyhow!("échec de la fenêtre : {err}"))?;
    shutdown.map_err(|err| anyhow::anyhow!("arrêt de la session : {err}"))?;
    Ok(())
}

/// Wayland natif par défaut (#72). Les correctifs winit/smithay-clipboard
/// partagent le récepteur de dépôt avec le presse-papiers, pour chaque viewport.
/// STOKESFORGE_X11=1 conserve une échappatoire X11/XWayland ; sans override,
/// winit choisit Wayland puis X11 selon les connexions disponibles. L'ancien
/// STOKESFORGE_WAYLAND n'est plus nécessaire.
#[cfg(target_os = "linux")]
fn configure_window_backend(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    use winit::platform::x11::EventLoopBuilderExtX11;
    if std::env::var_os("STOKESFORGE_X11").is_some_and(|value| value == "1") {
        builder.with_x11();
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_window_backend(_builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {}

/// Le chargement passe par l'acteur pour restaurer aussi les géométries,
/// conservées à côté du modèle. Charger seulement `Project` les perdrait.
fn session_for_project(requested: Option<PathBuf>) -> anyhow::Result<(Session, PathBuf)> {
    let load_existing = requested.as_ref().is_some_and(|path| path.exists());
    let path = requested.unwrap_or_else(|| PathBuf::from(DEFAULT_FILE));
    let project = if load_existing {
        Project::new("Chargement")?
    } else {
        demo_project()?
    };
    let session = Session::spawn(project, None).context("démarrage du thread de session")?;
    if load_existing {
        session
            .handle()
            .load(&path)
            .with_context(|| format!("chargement de {}", path.display()))?;
    }
    Ok((session, path))
}

/// Localise l'interpréteur embarqué et le pose sur `state`, **sans le
/// démarrer** — `EmbeddedPython::locate`/`with_home` ne touchent pas à
/// CPython, seul le premier script le fait (voir `stokesforge_app_state::state`, section
/// « Le script Python »). C'est délibérément le binaire, pas `AppState`, qui
/// décide d'où vient l'interpréteur : `AppState` reste indépendante de la
/// disposition d'une distribution empaquetée.
///
/// Un préfixe introuvable (dev sans `STOKESFORGE_PYTHON_HOME`, ou binaire non
/// empaqueté) n'est **pas** fatal : la fenêtre s'ouvre quand même, la
/// console de script reste visible et [`AppState::script_available`] reste
/// vrai (le pont est compilé) — c'est au premier essai d'exécution que
/// l'absence d'interpréteur se rapporte, comme toute autre erreur de ce banc.
#[cfg(feature = "python")]
fn configure_script_engine(state: &mut AppState) {
    match stokesforge_python::interpreter::EmbeddedPython::locate() {
        Ok(python) => state.set_python(python),
        Err(err) => {
            eprintln!(
                "[stokesforge-app] pont Python compilé, mais interpréteur introuvable : {err}"
            );
        }
    }
}

/// Sans la feature `python`, il n'y a rien à configurer : `AppState` n'a même
/// pas de champ pour un interpréteur.
#[cfg(not(feature = "python"))]
fn configure_script_engine(_state: &mut AppState) {}

/// Donne le rôle X (abscisse) à une colonne de la démonstration : la
/// première colonne d'une feuille est l'axe des abscisses.
fn set_x_role(project: &mut Project, column: NodeId) -> anyhow::Result<()> {
    let mut meta = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(|column| column.meta().clone())
        .unwrap_or_default();
    meta.role = stokesforge_core::ColumnRole::X;
    project
        .set_column_meta(column, meta)
        .context("rôle X de la démonstration")?;
    Ok(())
}

/// Projet de démonstration.
///
/// Il est construit pour que les cas tordus du modèle soient visibles dès
/// l'ouverture, pas seulement dans les tests : une valeur manquante numérique
/// (`NaN`), les deux valeurs texte que rien ne distingue à l'écran (`None` et
/// `Some("")`), et surtout des **colonnes de longueurs différentes** — une
/// feuille n'est pas un tableau rectangulaire.
/// Les noms de colonnes restent automatiques (A, B, C…) pour que le script
/// d'exemple désigne la même colonne que la lettre visible dans la grille.
fn demo_project() -> anyhow::Result<Project> {
    let mut project = Project::new("Demo").context("projet de démonstration")?;
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .context("classeur Book1")?;
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .context("lecture de Book1")?
        .context("Sheet1 créée avec le classeur")?;
    let a1 = project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .context("colonne A de Sheet1")?;
    set_x_role(&mut project, a1)?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[10.0, 12.5, f64::NAN, 18.0, 21.5]),
        )
        .context("colonne B de Sheet1")?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .context("colonne C de Sheet1")?;
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .context("feuille Sheet2")?;
    project
        .add_column(sheet2, None, None, Column::from_f64(&[7.0, 8.0]))
        .context("colonne A de Sheet2")?;
    // Deux feuilles vides de plus — Book1 n'en montrait que trois, pas assez
    // pour éprouver à l'œil le défilement de la bande d'onglets (F8) une
    // fois qu'elle ne rétrécit plus ses onglets.
    project
        .add_worksheet(book, Some("Sheet3"))
        .context("feuille Sheet3")?;
    project
        .add_worksheet(book, Some("Sheet4"))
        .context("feuille Sheet4")?;

    // Le graphe de la phase R4. Ses colonnes sont **les siennes** et non
    // A/B de Sheet1 : celles-là portent volontairement un `NaN` et des
    // longueurs inégales pour éprouver la grille, ce qui donnerait une
    // courbe difficile à lire. Un banc doit montrer les cas tordus du
    // modèle *et* un rendu qu'on puisse juger à l'œil.
    let spectre = project
        .add_worksheet(book, Some("Spectre"))
        .context("feuille Spectre")?;
    let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            let a = (x - 520.0) / 38.0;
            let b = (x - 610.0) / 22.0;
            0.85 * (-a * a).exp() + 0.32 * (-b * b).exp() + 0.04
        })
        .collect();
    let x = project
        .add_column(spectre, None, None, Column::from_f64(&xs))
        .context("colonne A de Spectre")?;
    set_x_role(&mut project, x)?;
    let y = project
        .add_column(spectre, None, None, Column::from_f64(&ys))
        .context("colonne B de Spectre")?;
    // Une cinétique pour éprouver le glisser de lignes (#166) : une colonne par
    // instant (ligne x' = temps en s), une ligne par longueur d'onde. Sélectionner
    // des numéros de ligne puis attraper le bord du bloc trace une courbe par
    // ligne, contre les x'.
    let cinetique = project
        .add_worksheet(book, Some("Cinetique"))
        .context("feuille Cinetique")?;
    for temps in [0.0, 30.0, 60.0, 120.0, 240.0] {
        let valeurs: Vec<f64> = (0..8)
            .map(|ligne| {
                let amplitude = 0.2 + 0.1 * ligne as f64;
                amplitude * (1.0 - (-temps / (60.0 + 20.0 * ligne as f64)).exp())
            })
            .collect();
        let colonne = project
            .add_column(cinetique, None, None, Column::from_f64(&valeurs))
            .context("colonne de Cinetique")?;
        let mut meta = project
            .tree()
            .node(colonne)
            .and_then(|node| node.as_column())
            .map(|column| column.meta().clone())
            .unwrap_or_default();
        meta.x_prime = Some(temps);
        meta.unit = "u.a.".into();
        project
            .set_column_meta(colonne, meta)
            .context("x' de Cinetique")?;
    }
    project
        .add_graph(root, Some("Graph1"), demo_spec(x, y))
        .context("graphe de démonstration")?;
    Ok(project)
}

/// Une courbe et deux axes linéaires pour juger le rendu à l'œil.
/// Comme les graphes créés par l'utilisateur, la démo n'ajoute aucun titre
/// par défaut (#103) ; les libellés d'axe et le nom de série restent explicites.
fn demo_spec(x: NodeId, y: NodeId) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "λ".into(),
        unit: "nm".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Absorbance mesurée".into(),
        unit: "u.a.".into(),
        ..AxisSpec::default()
    };
    let mut serie = SeriesSpec::new_xy(x, y);
    serie.name = "Échantillon µ1".into();
    serie.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(serie);
    PlotSpec::empty("").with_panels(panel)
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use stokesforge_session::WindowGeometry;

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
        let path = EXAMPLE_SCRIPT.split('"').nth(1).unwrap();
        let id = project.resolve_path(path).unwrap();
        assert_eq!(id, project.resolve_path("[Book1]Sheet1!A").unwrap());
        let column = stokesforge_core::resolve_column(project.tree(), id).unwrap();
        assert_eq!(column.len(), 5);
    }

    /// #103 : la démo suit le même défaut que les graphes créés par l'utilisateur.
    #[test]
    fn la_demo_ne_reserve_pas_de_titre_mais_un_titre_explicite_reste_possible() {
        let project = demo_project().unwrap();
        let graph = project.resolve_path("[Graph1]").unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            project.tree().node(graph).unwrap().kind()
        else {
            panic!("Graph1 doit être un graphe");
        };
        assert!(
            spec.title.is_empty(),
            "la démo ne donne pas de titre par défaut"
        );

        let measurer = stokesforge_render_egui::TextMeasurer::default();
        let rect = eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(640.0, 480.0),
        );
        let draw = |spec: &PlotSpec| {
            stokesforge_render_egui::collect_shapes(
                &mut Vec::new(),
                rect,
                &measurer,
                &stokesforge_core::resolve_spec(project.tree(), spec),
            )
            .unwrap()
        };
        let empty = draw(spec);
        assert_eq!(empty.title_height, 0);
        assert!(empty.title.is_none());

        // Témoin : le retrait du défaut ne désactive pas le rendu d'un titre choisi.
        let mut titled = spec.clone();
        titled.title = "Spectre choisi".into();
        let explicit = draw(&titled);
        assert!(explicit.title_height > 0);
        assert!(explicit.title.is_some());
        assert!(explicit.panels[0].pixel_origin.1 > empty.panels[0].pixel_origin.1);
    }

    #[test]
    fn le_demarrage_restaure_les_geometries_du_fichier() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("geometries.stf");
        let mut project = Project::new("Disposition sauvegardée").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let geometry = WindowGeometry {
            x: 42.0,
            y: 67.0,
            width: 520.0,
            height: 380.0,
            view: None,
        };
        let original = Session::spawn(project, None).unwrap();
        original
            .handle()
            .set_window_geometry(book, geometry)
            .unwrap();
        original.handle().save(&path).unwrap();
        original.shutdown().unwrap();

        let (loaded, actual_path) = session_for_project(Some(path.clone())).unwrap();
        let snapshot = loaded.handle().snapshot().unwrap();
        assert_eq!(actual_path, path);
        assert_eq!(snapshot.window_geometries.get(&book), Some(&geometry));
        assert!(snapshot.tree.node(book).is_some());
        loaded.shutdown().unwrap();
    }
}
