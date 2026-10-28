//! Capture of `sys.stdout` and `sys.stderr` while a script runs.
//!
//! # Why a per-thread switch rather than a `redirect_stdout`
//!
//! `sys.stdout` is **global to the interpreter**. The usual recipe (save,
//! replace, restore) is correct only if a single actor uses it at a time: two
//! script engines living in the same process (which happens in the test
//! binary, and will happen the day the application opens two projects) would
//! steal each other's restore.
//!
//! So a relay object is installed **once** in `sys.stdout` and `sys.stderr`,
//! and it does the switching: to the buffer of the **current thread** if there
//! is one ([`begin`] installs it, [`end`] removes it), otherwise to the
//! process's real output (`sys.__stdout__`). The object is therefore shared
//! and stateless; the only state lives in a `thread_local!`.
//!
//! Intended consequence: a `print` run outside a script (a future renderer,
//! for example) keeps going to the process output.

use std::cell::RefCell;

use pyo3::prelude::*;

/// Which of the two standard outputs this relay stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    /// `sys.stdout`.
    Out,
    /// `sys.stderr`.
    Err,
}

impl Stream {
    /// Name of the `sys` attribute this relay replaces.
    fn attribute(self) -> &'static str {
        match self {
            Stream::Out => "stdout",
            Stream::Err => "stderr",
        }
    }

    /// Name of the `sys` attribute that keeps the process's **real** output.
    fn original_attribute(self) -> &'static str {
        match self {
            Stream::Out => "__stdout__",
            Stream::Err => "__stderr__",
        }
    }
}

/// Buffers of the current thread, present only while a script runs.
#[derive(Debug, Default)]
struct Buffers {
    /// What was written to standard output.
    out: String,
    /// What was written to the error output.
    err: String,
}

thread_local! {
    /// Buffers of the current thread. `None` outside a script run: writes then
    /// go back to the process's real output.
    static BUFFERS: RefCell<Option<Buffers>> = const { RefCell::new(None) };
}

/// Opens the capture on the current thread. Overwrites any leftover buffer.
pub(crate) fn begin() {
    BUFFERS.with(|cell| *cell.borrow_mut() = Some(Buffers::default()));
}

/// Closes the capture on the current thread and returns `(stdout, stderr)`.
pub(crate) fn end() -> (String, String) {
    BUFFERS.with(|cell| {
        cell.borrow_mut()
            .take()
            .map(|buffers| (buffers.out, buffers.err))
            .unwrap_or_default()
    })
}

/// Appends `text` to the current thread's buffer. Returns `false` if there is none.
fn push(stream: Stream, text: &str) -> bool {
    BUFFERS.with(|cell| match cell.borrow_mut().as_mut() {
        Some(buffers) => {
            match stream {
                Stream::Out => buffers.out.push_str(text),
                Stream::Err => buffers.err.push_str(text),
            }
            true
        }
        None => false,
    })
}

/// Relay installed in `sys.stdout` / `sys.stderr`.
///
/// It has no state of its own (hence `frozen`): it carries only the output it
/// stands for. All the capture state lives in a `thread_local!`, which makes
/// the object shareable between several script engines without them
/// stepping on each other.
#[pyclass(frozen, module = "pyst", name = "_ScriptOutput")]
pub(crate) struct CaptureStream {
    /// Output this relay stands for.
    stream: Stream,
}

#[pymethods]
impl CaptureStream {
    /// Writes `text` to the current thread's buffer, or to the process's real
    /// output if no script runs on this thread. Returns the number of
    /// characters written, like `io.TextIOBase.write`.
    fn write(&self, py: Python<'_>, text: &str) -> PyResult<usize> {
        if !push(self.stream, text) {
            let original = py
                .import("sys")?
                .getattr(self.stream.original_attribute())?;
            if !original.is_none() {
                original.call_method1("write", (text,))?;
            }
        }
        Ok(text.chars().count())
    }

    /// Flushes the real output if the capture is not active; no effect
    /// otherwise (an in-memory buffer has nothing to flush).
    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        let capturing = BUFFERS.with(|cell| cell.borrow().is_some());
        if capturing {
            return Ok(());
        }
        let original = py
            .import("sys")?
            .getattr(self.stream.original_attribute())?;
        if !original.is_none() {
            original.call_method0("flush")?;
        }
        Ok(())
    }

    /// Always true: this relay is an output.
    fn writable(&self) -> bool {
        true
    }

    /// Always false: it is not read from.
    fn readable(&self) -> bool {
        false
    }

    /// Always false: this is not a seekable stream.
    fn seekable(&self) -> bool {
        false
    }

    /// Always false: a script's output is not a terminal, which keeps a
    /// library from trying to write color codes to it.
    fn isatty(&self) -> bool {
        false
    }

    /// Advertised encoding, for libraries that query it.
    #[getter]
    fn encoding(&self) -> &'static str {
        "utf-8"
    }

    /// Advertised encoding error policy.
    #[getter]
    fn errors(&self) -> &'static str {
        "strict"
    }

    /// Readable representation, so a script that inspects `sys.stdout`
    /// understands what it is looking at.
    fn __repr__(&self) -> String {
        format!("<StokesForge script output: {}>", self.stream.attribute())
    }
}

/// Installs the relay in `sys.stdout` and `sys.stderr` if it is not there yet.
///
/// Idempotent, and deliberately **permanent**: the relay is never removed.
/// Outside a capture it redirects to the real output, so leaving it in place
/// changes nobody's behavior.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    for stream in [Stream::Out, Stream::Err] {
        let current = sys.getattr(stream.attribute())?;
        if current.is_instance_of::<CaptureStream>() {
            continue;
        }
        sys.setattr(
            stream.attribute(),
            Bound::new(py, CaptureStream { stream })?,
        )?;
    }
    Ok(())
}
