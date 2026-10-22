//! Binaire du banc d'essai : démarre l'acteur de session, construit l'état et
//! ouvre la fenêtre.
//!
//! Usage : `stokesforge-app [chemin]`. Si `chemin` existe, le projet est chargé
//! depuis ce fichier (et marqué enregistré) ; sinon un projet de démonstration
//! est construit et `chemin` devient simplement la cible d'un futur
//! enregistrement. Sans argument, la démonstration n'a **pas** de chemin : le
//! premier Ctrl+S ouvre le sélecteur, qui suggère `demo.stf` (#315).
//!
//! Le projet de démonstration vit dans le module [`demo`] (code, pas fichier) :
//! `Book1` et `Graph1` pour le script d'exemple, puis des classeurs de types de
//! colonnes, un dossier d'analyses (dont quelques calculs déjà faits) et six
//! graphes qui éprouvent le tracé. Ses colonnes à formule n'ont leurs valeurs
//! qu'après le recalcul demandé par `session_for_project`.

// Windows, release : sous-système fenêtré, pas de console noire derrière la
// fenêtre (#332). `console_windows::attach_parent` rend les messages au terminal
// qui a lancé l'application, s'il y en a un.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(all(windows, not(debug_assertions)))]
mod console_windows;
mod demo;

use std::path::PathBuf;

use anyhow::Context as _;
use stokesforge_app::app::StokesForgeApp;
use stokesforge_app_state::state::AppState;
use stokesforge_core::Project;
use stokesforge_session::{LoadReport, Session};

/// Script d'exemple qui pré-remplit la zone de script : il fonctionne tel
/// quel sur le projet de démonstration ([`demo::demo_project`]) et illustre le
/// critère de fin du socle (lecture sans copie d'une colonne). Affiché même
/// sans la feature `python` : un banc d'essai doit montrer ce qui manque, pas
/// le cacher.
const EXAMPLE_SCRIPT: &str = "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
v = pyst.column(a)\n\
print(\"longueur :\", len(v), \"premier :\", v[0])\n";

fn main() -> anyhow::Result<()> {
    #[cfg(all(windows, not(debug_assertions)))]
    console_windows::attach_parent();
    let requested = std::env::args().nth(1).map(PathBuf::from);
    let (session, path, report) = session_for_project(
        requested,
        stokesforge_app_state::preferences::Preferences::load().load_limits(),
    )?;
    let mut state = AppState::new(session.handle(), path.display().to_string())
        .context("premier instantané du projet")?;
    if let Some(report) = &report {
        state.mark_loaded(report);
    }
    // Copies de secours de processus disparus (#317) : la fenêtre racine seule
    // les propose.
    state.scan_recovery();
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
///
/// Rend aussi le rapport de lecture si le projet a été **chargé** depuis le
/// chemin (`None` sinon) : seul ce cas pose le repère « enregistré », et le
/// rapport est transmis à `mark_loaded` pour que l'écarté soit dit (#309). Sans argument, la démonstration n'a pas de
/// chemin, donc un `demo.stf` du dossier courant n'est ni lu ni écrasé (#315).
fn session_for_project(
    requested: Option<PathBuf>,
    max_limits: stokesforge_app_state::preferences::LoadLimits,
) -> anyhow::Result<(Session, PathBuf, Option<LoadReport>)> {
    let load_existing = requested.as_ref().is_some_and(|path| path.exists());
    let path = requested.unwrap_or_default();
    let project = if load_existing {
        Project::new("Chargement")?
    } else {
        demo::demo_project()?
    };
    let session = Session::spawn(project, None).context("démarrage du thread de session")?;
    if !load_existing {
        // Les colonnes à formule de la démonstration ne portent que leur
        // formule (invariant 11) : un recalcul complet, tout de suite, leur
        // donne leurs valeurs avant le premier instantané.
        session
            .handle()
            .recalculate(&[])
            .context("recalcul de la démonstration")?;
    }
    let report = if load_existing {
        let (_, report) = session
            .handle()
            .load_with_limits(&path, max_limits)
            .with_context(|| format!("chargement de {}", path.display()))?;
        Some(report)
    } else {
        None
    };
    Ok((session, path, report))
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

#[cfg(test)]
mod startup_tests {
    use super::*;
    use stokesforge_core::Column;
    use stokesforge_session::WindowGeometry;

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

        let (loaded, actual_path, report) = session_for_project(
            Some(path.clone()),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        assert!(
            report.is_some_and(|r| r.is_clean()),
            "un fichier existant est chargé"
        );
        let snapshot = loaded.handle().snapshot().unwrap();
        assert_eq!(actual_path, path);
        assert_eq!(snapshot.window_geometries.get(&book), Some(&geometry));
        assert!(snapshot.tree.node(book).is_some());
        loaded.shutdown().unwrap();
    }

    /// #325 : une largeur de colonne enregistrée dans le `.stf` est celle de
    /// l'instantané au démarrage (même chemin que les géométries).
    #[test]
    fn le_demarrage_restaure_les_largeurs_de_colonne_du_fichier() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("largeurs.stf");
        let mut project = Project::new("Largeurs").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let column = project
            .add_column(
                sheet,
                None,
                Some("A"),
                stokesforge_core::Column::from_f64(&[1.0]),
            )
            .unwrap();
        let original = Session::spawn(project, None).unwrap();
        original.handle().set_column_width(column, 91.5).unwrap();
        original.handle().save(&path).unwrap();
        original.shutdown().unwrap();

        let (loaded, _, _) = session_for_project(
            Some(path),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        let snapshot = loaded.handle().snapshot().unwrap();
        assert_eq!(snapshot.column_widths.get(&column), Some(&91.5));
        loaded.shutdown().unwrap();
    }

    /// #315 : sans argument, la démonstration n'a pas de chemin et n'est pas
    /// « chargée » : un `demo.stf` du dossier courant ne doit être ni pris pour
    /// le projet ni écrasé sans contrôle.
    #[test]
    fn sans_argument_la_demo_n_a_ni_chemin_ni_repere_charge() {
        let (session, path, report) = session_for_project(
            None,
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        assert!(path.as_os_str().is_empty());
        assert!(report.is_none());
        session.shutdown().unwrap();
    }

    /// Un chemin absent est la cible d'un futur enregistrement, pas un chargement.
    #[test]
    fn un_chemin_absent_n_est_pas_un_chargement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("absent.stf");
        let (session, actual, report) = session_for_project(
            Some(path.clone()),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        assert_eq!(actual, path);
        assert!(report.is_none());
        session.shutdown().unwrap();
    }

    /// #309 : le rapport de la lecture de démarrage est rendu à l'appelant ;
    /// un calcul écarté n'y est pas perdu.
    #[test]
    fn le_demarrage_rend_le_rapport_des_calculs_ecartes() {
        use stokesforge_analysis::correct::CorrectOp;
        use stokesforge_analysis::ops::Operation;
        use stokesforge_core::{
            CalculationDraft, ColumnMeta, OutputDraft, SeriesRef, SeriesResult,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ecarte.stf");
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        project
            .add_calculation(CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Subtract,
                },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![0.0, 1.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: false,
            })
            .unwrap();
        let writer = Session::spawn(project, None).unwrap();
        writer.handle().save(&path).unwrap();
        writer.shutdown().unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes
            .windows(8)
            .position(|w| w == b"subtract")
            .expect("l'opération est dans le fichier");
        bytes[at..at + 8].copy_from_slice(b"subtraXX");
        std::fs::write(&path, bytes).unwrap();

        let (session, _, report) = session_for_project(
            Some(path),
            stokesforge_app_state::preferences::default_load_limits(),
        )
        .unwrap();
        let report = report.expect("fichier chargé");
        assert!(!report.is_clean());
        assert_eq!(report.discarded_calculations.len(), 1);
        session.shutdown().unwrap();
    }
}
