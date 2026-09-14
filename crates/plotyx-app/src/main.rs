//! Binaire du banc d'essai : démarre l'acteur de session, construit l'état et
//! ouvre la fenêtre.
//!
//! Usage : `plotyx-app [chemin]`. Si `chemin` existe, le projet est chargé
//! depuis ce fichier ; sinon un projet de démonstration est construit et
//! `chemin` devient simplement la cible d'un futur enregistrement.

use std::path::PathBuf;

use anyhow::Context as _;
use plotyx_app::app::PlotyxApp;
use plotyx_app::state::AppState;
use plotyx_core::{Column, NodeId, Project};
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_session::Session;

/// Nom du fichier proposé par défaut quand aucun chemin n'est donné.
const DEFAULT_FILE: &str = "demo.plotyx";

/// Script d'exemple qui pré-remplit la zone de script : il fonctionne tel
/// quel sur le projet de démonstration ([`demo_project`]) et illustre le
/// critère de fin du socle (lecture sans copie d'une colonne). Affiché même
/// sans la feature `python` : un banc d'essai doit montrer ce qui manque, pas
/// le cacher.
const EXAMPLE_SCRIPT: &str = "a = pyx.resolve(\"[Book1]Sheet1!Temps\")\n\
v = pyx.column(a)\n\
print(\"longueur :\", len(v), \"premier :\", v[0])\n";

fn main() -> anyhow::Result<()> {
    let requested = std::env::args().nth(1).map(PathBuf::from);
    let (project, path) = match requested {
        Some(path) if path.exists() => {
            let project = plotyx_persistence::load(&path)
                .with_context(|| format!("chargement de {}", path.display()))?;
            (project, path)
        }
        Some(path) => (demo_project()?, path),
        None => (demo_project()?, PathBuf::from(DEFAULT_FILE)),
    };

    let session = Session::spawn(project, None).context("démarrage du thread de session")?;
    let mut state = AppState::new(session.handle(), path.display().to_string())
        .context("premier instantané du projet")?;
    state.set_script_source(EXAMPLE_SCRIPT.to_string());
    configure_script_engine(&mut state);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("PlotYx — banc d'essai")
            .with_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    // `run_native` bloque jusqu'à la fermeture de la fenêtre ; `session` reste
    // donc vivante pendant toute la durée de l'application.
    let outcome = eframe::run_native(
        "plotyx-app",
        options,
        Box::new(|cc| {
            // Tâche R4.1 : poser explicitement **les** définitions de police
            // du projet sur le contexte, plutôt que de compter sur le fait
            // qu'egui a par défaut les mêmes. C'est ce qui fait de l'accord
            // écran/export une propriété et non une coïncidence — si egui
            // changeait son défaut, la figure resterait la même des deux
            // côtés.
            cc.egui_ctx
                .set_fonts(plotyx_render_egui::font_definitions());
            // Thème maison, clair, indépendant du thème système (voir la doc
            // de tête de `plotyx_app::theme`) — posé ici, une seule fois, aux
            // côtés des polices : les deux relèvent du même geste (« ce que
            // l'appli impose sur le `Context` avant le premier rendu »).
            plotyx_app::theme::install(&cc.egui_ctx);
            Ok(Box::new(PlotyxApp::new(state)))
        }),
    );

    // L'arrêt propre de l'acteur passe avant la remontée d'une erreur de
    // fenêtre : sans cela, le thread survivrait à la sortie du processus.
    let shutdown = session.shutdown();
    outcome.map_err(|err| anyhow::anyhow!("échec de la fenêtre : {err}"))?;
    shutdown.map_err(|err| anyhow::anyhow!("arrêt de la session : {err}"))?;
    Ok(())
}

/// Localise l'interpréteur embarqué et le pose sur `state`, **sans le
/// démarrer** — `EmbeddedPython::locate`/`with_home` ne touchent pas à
/// CPython, seul le premier script le fait (voir `plotyx_app::state`, section
/// « Le script Python »). C'est délibérément le binaire, pas `AppState`, qui
/// décide d'où vient l'interpréteur : `AppState` reste indépendante de la
/// disposition d'une distribution empaquetée.
///
/// Un préfixe introuvable (dev sans `PLOTYX_PYTHON_HOME`, ou binaire non
/// empaqueté) n'est **pas** fatal : la fenêtre s'ouvre quand même, la
/// console de script reste visible et [`AppState::script_available`] reste
/// vrai (le pont est compilé) — c'est au premier essai d'exécution que
/// l'absence d'interpréteur se rapporte, comme toute autre erreur de ce banc.
#[cfg(feature = "python")]
fn configure_script_engine(state: &mut AppState) {
    match plotyx_python::interpreter::EmbeddedPython::locate() {
        Ok(python) => state.set_python(python),
        Err(err) => {
            eprintln!("[plotyx-app] pont Python compilé, mais interpréteur introuvable : {err}");
        }
    }
}

/// Sans la feature `python`, il n'y a rien à configurer : `AppState` n'a même
/// pas de champ pour un interpréteur.
#[cfg(not(feature = "python"))]
fn configure_script_engine(_state: &mut AppState) {}

/// Projet de démonstration.
///
/// Il est construit pour que les cas tordus du modèle soient visibles dès
/// l'ouverture, pas seulement dans les tests : une valeur manquante numérique
/// (`NaN`), les deux valeurs texte que rien ne distingue à l'écran (`None` et
/// `Some("")`), et surtout des **colonnes de longueurs différentes** — une
/// feuille n'est pas un tableau rectangulaire.
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
    project
        .add_column(
            sheet1,
            None,
            Some("Temps"),
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .context("colonne Temps")?;
    project
        .add_column(
            sheet1,
            None,
            Some("Signal"),
            Column::from_f64(&[10.0, 12.5, f64::NAN, 18.0, 21.5]),
        )
        .context("colonne Signal")?;
    project
        .add_column(
            sheet1,
            None,
            Some("Note"),
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .context("colonne Note")?;
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .context("feuille Sheet2")?;
    project
        .add_column(sheet2, None, Some("Court"), Column::from_f64(&[7.0, 8.0]))
        .context("colonne Court")?;

    // Le graphe de la phase R4. Ses colonnes sont **les siennes** et non
    // `Temps`/`Signal` : celles-là portent volontairement un `NaN` et des
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
        .add_column(spectre, None, Some("Lambda"), Column::from_f64(&xs))
        .context("colonne Lambda")?;
    let y = project
        .add_column(spectre, None, Some("Absorbance"), Column::from_f64(&ys))
        .context("colonne Absorbance")?;
    project
        .add_graph(root, Some("Graph1"), demo_spec(x, y))
        .context("graphe de démonstration")?;
    Ok(project)
}

/// Le `PlotSpec` du graphe de démonstration : une `LineSeries`, axes
/// linéaires, pas de légende — exactement le « graphe minimal » de la phase
/// R4 (`docs/archive/newarchitecture/roadmap-rendu.md`).
fn demo_spec(x: NodeId, y: NodeId) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "λ".into(),
        unit: "nm".into(),
        grid: true,
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Absorbance mesurée".into(),
        unit: "u.a.".into(),
        grid: true,
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
    PlotSpec {
        title: "Spectre d'absorption — λ_max".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel)
}
