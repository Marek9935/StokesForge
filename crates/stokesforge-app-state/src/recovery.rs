//! Copie de secours quand l'acteur de session meurt (#310).
//!
//! Un panic du fil de l'acteur détruit le `Project` : il ne reste que le
//! dernier instantané que l'interface a relu (`Arc<Tree>` immuable, déjà
//! partagé). On le réécrit dans un fichier **neuf** du dossier de données de
//! l'application — jamais sur le `.stf` de l'utilisateur, que la copie peut
//! précéder d'une commande (un panic en plein recalcul laisse l'état d'avant).
//!
//! `Project::from_tree(.., 0)` reprend les identifiants au plus haut vivant :
//! ceux des nœuds supprimés peuvent revenir, sans effet puisque l'historique
//! est perdu avec l'acteur.

use std::path::{Path, PathBuf};

use stokesforge_core::Project;
use stokesforge_persistence::PersistenceError;
use stokesforge_session::Snapshot;

/// Pourquoi la copie de secours n'a pas pu être écrite.
#[derive(Debug)]
pub enum RecoveryError {
    /// Le dossier de secours n'a pas pu être créé.
    CreateDir(std::io::Error),
    /// L'écriture du fichier a échoué.
    Save(PersistenceError),
}

/// Dossier des copies de secours : `recovery/` dans le dossier de données de
/// l'application. `None` sous `cfg(test)` ou la feature `test-support` : un
/// test n'écrit jamais dans les vraies données de la machine (comme les
/// préférences, #250).
pub(crate) fn default_dir() -> Option<PathBuf> {
    if cfg!(any(test, feature = "test-support")) {
        return None;
    }
    directories::ProjectDirs::from("", "", "StokesForge")
        .map(|dirs| dirs.data_dir().join("recovery"))
}

/// Écrit `snapshot` dans un fichier neuf de `dir` et rend son chemin. Le nom
/// reprend celui du projet (`project_path`) suivi de l'heure : deux plantages
/// ne s'écrasent pas, et `save` ne rencontre jamais un fichier existant.
pub fn write_copy(
    dir: &Path,
    project_path: &str,
    snapshot: &Snapshot,
) -> Result<PathBuf, RecoveryError> {
    std::fs::create_dir_all(dir).map_err(RecoveryError::CreateDir)?;
    let stem = Path::new(project_path).file_stem().map_or_else(
        || "project".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut path = dir.join(format!("{stem}-{seconds}.stf"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem}-{seconds}-{n}.stf"));
        n += 1;
    }
    let project = Project::from_tree(snapshot.tree.clone(), 0);
    stokesforge_persistence::save_with_geometries(&project, &snapshot.window_geometries, &path)
        .map_err(RecoveryError::Save)?;
    Ok(path)
}
