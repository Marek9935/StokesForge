//! What a script run produced.

/// Result of a script run.
///
/// The script's output is **captured**, never mixed with that of the host
/// process: `print` lands in [`ScriptOutcome::stdout`].
///
/// # Known limit: the output is not streamed as it is produced
///
/// In the foundation, `stdout` and `stderr` can be read only **after** the
/// script ends: a long run shows nothing before it finishes. This is an
/// accepted limit, not an oversight: incremental streaming would need a
/// progress channel that this foundation's protocol does not carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptOutcome {
    /// What the script wrote to standard output (`print`), never lost in
    /// that of the process.
    pub stdout: String,
    /// What the script wrote to the error output.
    pub stderr: String,
    /// Error message **with its full Python traceback**, if the script
    /// failed.
    ///
    /// When [`ScriptOutcome::interrupted`] is true, this field normally
    /// holds the traceback of the `KeyboardInterrupt` that stopped the script:
    /// it is information (*where* the script stopped), not a failure to
    /// show in red.
    pub error: Option<String>,
    /// True if the run stopped because an interruption was requested while
    /// it was running.
    pub interrupted: bool,
}

impl ScriptOutcome {
    /// True if the script ended with no exception and no interruption.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.error.is_none() && !self.interrupted
    }
}
