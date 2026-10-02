//! Type d'erreur du moteur de script.
//!
//! Ces erreurs sont celles de l'**hôte** — démarrage du fil, disponibilité du
//! moteur. Une erreur *du script lui-même* n'en est pas une : elle atterrit
//! dans [`crate::ScriptOutcome::error`] avec sa trace d'appels Python, parce
//! qu'un script qui lève une exception est un résultat, pas une panne du
//! moteur.

use stokesforge_python::interpreter::PythonError;

/// Erreur de l'hôte autour de l'exécution d'un script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// Le système d'exploitation a refusé de créer le fil de script.
    #[error("could not create the script thread: {0}")]
    Thread(#[from] std::io::Error),
    /// L'interpréteur CPython embarqué n'a pas pu démarrer.
    #[error(transparent)]
    Interpreter(#[from] PythonError),
    /// L'interpréteur a démarré mais la surface `pyst` n'a pas pu être posée.
    #[error("could not install the pyst surface:\n{0}")]
    Install(String),
    /// Un script est déjà en cours d'exécution : le socle en autorise **un à
    /// la fois**.
    #[error("a script is already running")]
    Busy,
    /// Le fil de script est arrêté (après [`crate::ScriptEngine::shutdown`],
    /// ou parce qu'il a paniqué) : plus aucun script ne peut être exécuté.
    #[error("the script thread is stopped")]
    Stopped,
}
