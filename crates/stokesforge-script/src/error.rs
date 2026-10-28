//! Error type of the script engine.
//!
//! These errors belong to the **host**: thread startup, engine availability.
//! An error *of the script itself* is not one of them: it lands in
//! [`crate::ScriptOutcome::error`] with its Python traceback, because a script
//! that raises an exception is a result, not an engine failure.

use stokesforge_python::interpreter::PythonError;

/// Host error around the execution of a script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The operating system refused to create the script thread.
    #[error("could not create the script thread: {0}")]
    Thread(#[from] std::io::Error),
    /// The embedded CPython interpreter could not start.
    #[error(transparent)]
    Interpreter(#[from] PythonError),
    /// The interpreter started but the `pyst` surface could not be installed.
    #[error("could not install the pyst surface:\n{0}")]
    Install(String),
    /// A script is already running: the foundation allows **one at a
    /// time**.
    #[error("a script is already running")]
    Busy,
    /// The script thread is stopped (after [`crate::ScriptEngine::shutdown`],
    /// or because it panicked): no script can run anymore.
    #[error("the script thread is stopped")]
    Stopped,
}
