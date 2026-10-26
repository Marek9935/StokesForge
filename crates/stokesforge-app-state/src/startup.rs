//! Que montrer au démarrage (#333) : la décision, sans fenêtre ni disque.
//!
//! La démonstration n'est pas un décor permanent. Elle s'offre **une fois**, à
//! la toute première ouverture : écrite dans `demo.stf`, à côté des
//! préférences, puis ouverte depuis ce fichier, que l'utilisateur retrouve
//! ensuite par « Ouvrir ». Les lancements suivants partent d'un projet vide,
//! comme Fichier > Nouveau.
//!
//! Le développement garde la démonstration à chaque lancement par
//! `STOKESFORGE_DEMO=1` (captures, essais, mesures) : le binaire lit
//! l'environnement et passe le drapeau à [`plan`], qui n'y touche jamais, pour
//! se tester sans `set_var`.
//!
//! Règles, dans l'ordre :
//! 1. un chemin existant est chargé, toujours ;
//! 2. `STOKESFORGE_DEMO=1` : la démonstration en mémoire, sans marqueur ;
//! 3. un chemin absent est la cible d'un futur enregistrement, sur un projet
//!    vide (ce n'est pas une « première ouverture ») ;
//! 4. sans argument : la démonstration si le marqueur de première ouverture
//!    manque, sinon un projet vide.
//!
//! Un `demo.stf` du dossier courant n'est jamais lu (#315) : seul celui du
//! dossier de configuration compte.

use std::path::{Path, PathBuf};

use crate::preferences::Preferences;

/// Ce qu'il faut construire au démarrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupSource {
    /// Charger ce fichier existant.
    Load(PathBuf),
    /// Écrire la démonstration dans ce fichier, puis l'ouvrir depuis lui. Si
    /// l'écriture ou la lecture échoue, le binaire garde la démonstration en
    /// mémoire, sans chemin.
    DemoFile(PathBuf),
    /// La démonstration en mémoire ; le chemin est la cible d'un futur
    /// enregistrement (vide s'il n'y en a pas).
    DemoMemory(PathBuf),
    /// Un projet vide ; même chemin cible.
    Empty(PathBuf),
}

/// Le plan de démarrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    /// Le projet à construire.
    pub source: StartupSource,
    /// Pré-remplir la zone de script avec le script d'exemple : il lit
    /// `[Book1]Sheet1!A`, qui n'existe que dans la démonstration.
    pub example_script: bool,
    /// Poser le marqueur « première ouverture faite » une fois la fenêtre
    /// construite.
    pub mark_first_run: bool,
}

/// Décide quoi ouvrir. `requested` est l'argument de la ligne de commande,
/// `requested_exists` dit s'il désigne un fichier existant, `demo_forced` vient
/// de `STOKESFORGE_DEMO=1`.
pub fn plan(
    requested: Option<&Path>,
    requested_exists: bool,
    demo_forced: bool,
    preferences: &Preferences,
) -> Startup {
    let target = requested.map(Path::to_path_buf).unwrap_or_default();
    if let (Some(path), true) = (requested, requested_exists) {
        return Startup {
            source: StartupSource::Load(path.to_path_buf()),
            example_script: false,
            mark_first_run: false,
        };
    }
    if demo_forced {
        return Startup {
            source: StartupSource::DemoMemory(target),
            example_script: true,
            mark_first_run: false,
        };
    }
    if requested.is_some() || preferences.first_run_done() {
        return Startup {
            source: StartupSource::Empty(target),
            example_script: false,
            mark_first_run: false,
        };
    }
    // Première ouverture : le fichier dans le dossier de configuration, ou, sans
    // dossier de configuration, la démonstration en mémoire (le marqueur ne
    // pourra pas s'écrire : elle reviendra).
    let source = match preferences.demo_file_path() {
        Some(file) => StartupSource::DemoFile(file),
        None => StartupSource::DemoMemory(PathBuf::new()),
    };
    Startup {
        source,
        example_script: true,
        mark_first_run: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preferences(done: bool) -> (tempfile::TempDir, Preferences) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            if done {
                r#"{"first_run_done":true}"#
            } else {
                "{}"
            },
        )
        .unwrap();
        (dir, Preferences::load_from(Some(path)))
    }

    #[test]
    fn sans_argument_ni_marqueur_la_demo_s_ouvre_depuis_son_fichier() {
        let (dir, prefs) = preferences(false);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(
            startup.source,
            StartupSource::DemoFile(dir.path().join("demo.stf"))
        );
        assert!(startup.example_script);
        assert!(startup.mark_first_run);
    }

    #[test]
    fn avec_le_marqueur_le_projet_est_vide() {
        let (_dir, prefs) = preferences(true);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(startup.source, StartupSource::Empty(PathBuf::new()));
        assert!(!startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn la_demo_forcee_ignore_le_marqueur_et_n_en_pose_pas() {
        let (_dir, prefs) = preferences(true);
        let startup = plan(None, false, true, &prefs);
        assert_eq!(startup.source, StartupSource::DemoMemory(PathBuf::new()));
        assert!(startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn sans_dossier_de_configuration_la_demo_reste_en_memoire() {
        let prefs = Preferences::load_from(None);
        let startup = plan(None, false, false, &prefs);
        assert_eq!(startup.source, StartupSource::DemoMemory(PathBuf::new()));
        assert!(startup.example_script);
    }

    #[test]
    fn un_chemin_existant_est_toujours_charge_meme_si_la_demo_est_forcee() {
        let (_dir, prefs) = preferences(false);
        let path = Path::new("projet.stf");
        let startup = plan(Some(path), true, true, &prefs);
        assert_eq!(startup.source, StartupSource::Load(path.to_path_buf()));
        assert!(!startup.example_script);
        assert!(!startup.mark_first_run);
    }

    #[test]
    fn un_chemin_absent_donne_un_projet_vide_qui_le_vise_sans_marqueur() {
        let (_dir, prefs) = preferences(false);
        let path = Path::new("nouveau.stf");
        let startup = plan(Some(path), false, false, &prefs);
        assert_eq!(startup.source, StartupSource::Empty(path.to_path_buf()));
        assert!(!startup.mark_first_run);
    }
}
