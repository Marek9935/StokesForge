//! Type d'erreur du moteur de script.
//!
//! Ces erreurs sont celles de l'**hôte** — démarrage du fil, disponibilité du
//! moteur. Une erreur *du script lui-même* n'en est pas une : elle atterrit
//! dans [`crate::ScriptOutcome::error`] avec sa trace d'appels Python, parce
//! qu'un script qui lève une exception est un résultat, pas une panne du
//! moteur.

use plotyx_python::interpreter::PythonError;

/// Erreur de l'hôte autour de l'exécution d'un script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// Le système d'exploitation a refusé de créer le fil de script.
    #[error("le fil de script n'a pas pu être créé : {0}")]
    Thread(#[from] std::io::Error),
    /// L'interpréteur CPython embarqué n'a pas pu démarrer.
    #[error(transparent)]
    Interpreter(#[from] PythonError),
    /// L'interpréteur a démarré mais la surface `pyx` n'a pas pu être posée.
    #[error("échec d'installation de la surface pyx :\n{0}")]
    Install(String),
    /// Un script est déjà en cours d'exécution : le socle en autorise **un à
    /// la fois**.
    #[error("un script est déjà en cours d'exécution")]
    Busy,
    /// Le fil de script est arrêté (après [`crate::ScriptEngine::shutdown`],
    /// ou parce qu'il a paniqué) : plus aucun script ne peut être exécuté.
    #[error("le fil de script est arrêté")]
    Stopped,
}
