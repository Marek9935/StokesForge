//! The script thread: a **permanent service** that owns the interpreter.
//!
//! # One script, one write group
//!
//! The context opens an exclusive macro at the first mutation. A read-only
//! run leaves the session available. At the end, with the GIL released and the
//! interruption disarmed, the group is committed or aborted (exception, stop
//! requested). An explicit undo from pyst is a commit point of the group.
//!
//! # Two threads, not one
//!
//! * the **script thread** (`stokesforge-script`) starts CPython, thereby
//!   becomes its main thread, and runs the scripts one after the other;
//! * an **interruption thread** (`stokesforge-script-irq`), tiny, exists so
//!   that [`ScriptEngine::interrupt`] **never** blocks. Raising an asynchronous
//!   exception in the script thread requires the GIL; taking it from the thread
//!   that asks for the stop (the interface, for example) could make it wait
//!   there. The interface therefore sets a flag and sends a message; this
//!   small thread is the one that waits for the GIL in its place.
//!
//! # Two flags, not one
//!
//! `busy` is set by [`ScriptEngine::run`] **before** the work is sent: it
//! serves the [`ScriptError::Busy`] refusal. `running` is set by the script thread
//! **once it has really entered the script**, and it is what
//! [`ScriptEngine::is_running`] returns. The distinction is not cosmetic:
//! it guarantees that an interruption requested while `is_running()` is true
//! can no longer be erased by the start-of-run reset.
//!
//! # Interruption, in two halves
//!
//! The interpreter starts **without signal handling**: no
//! `KeyboardInterrupt` will be raised on its own. Stopping a script is therefore
//! entirely our job, and plays out in two complementary halves:
//!
//! 1. an **atomic flag** checked by each call of the `pyst` surface:
//!    it catches a script that loops while calling the API, at no extra cost;
//! 2. **`PyThreadState_SetAsyncExc`** from the interruption thread, which raises a
//!    `KeyboardInterrupt` in the script thread at the interpreter's next
//!    checkpoint: it catches a **pure** Python loop.
//!
//! *Ruled out:* a trace hook through `sys.settrace`, which would cost on every
//! line executed, permanently, for a granularity we do not need.
//!
//! **Limit to state honestly**: neither half interrupts a long **native**
//! call. A numpy operation on ten million points will run to completion; the
//! exception will be raised only on returning to the interpreter.
//! A script that swallows exceptions (`except: pass` in a pure Python loop)
//! also resists the second half; the first catches it as soon as
//! it touches `pyst`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_long;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};

use pyo3::exceptions::{PyKeyboardInterrupt, PyRuntimeError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTracebackMethods};
use stokesforge_commands::MacroKind;
use stokesforge_core::NodeId;
use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_session::SessionHandle;

use crate::error::ScriptError;
use crate::outcome::ScriptOutcome;
use crate::{bridge::HostError, capture, surface};

/// State shared between the script thread, the interruption thread and the clients.
///
/// Nothing here is a Python object: these flags cross threads without
/// ever touching the GIL.
#[derive(Debug, Default)]
struct Shared {
    /// True as soon as a job is accepted by [`ScriptEngine::run`] and until
    /// its reply is ready. Serves the `Busy` refusal.
    busy: AtomicBool,
    /// True while the script thread is actually **inside** the script.
    running: AtomicBool,
    /// True when an interruption was requested for the current run.
    interrupt_requested: AtomicBool,
    /// Number of the current (or last) run; incremented at the start of each
    /// script. An interruption message carries the number it targeted and strikes
    /// no other script.
    generation: AtomicU64,
    /// Python identifier (`threading.get_ident()`) of the script thread, `0` until
    /// the interpreter has started.
    py_thread_ident: AtomicU64,
    /// Current project path, for `pyst.save()`: set by the host
    /// ([`ScriptEngine::set_project_path`]) and updated by `pyst.save_as`.
    /// Pure Rust data; the session itself knows no path.
    project_path: Mutex<Option<PathBuf>>,
    /// Path that `pyst.save()` (without argument) refuses to overwrite: the source
    /// of a read that discarded data (application guard #309).
    /// `save_as` stays allowed.
    protected_path: Mutex<Option<PathBuf>>,
    /// `(model, geometry)` revision of the script's last save, which the host
    /// picks up ([`ScriptEngine::take_saved_revision`]).
    saved_revision: Mutex<Option<(u64, u64)>>,
}

/// What the `pyst` surface needs to serve a call: the session, and
/// what it needs to know whether to stop.
pub(crate) struct ScriptContext {
    /// Client handle to the actor that owns the `Project`.
    session: SessionHandle,
    /// Flags shared with the engine.
    shared: Arc<Shared>,
    /// Opened only at the first write; a read script never
    /// reserves the session. Atomic to stay usable with the GIL released.
    macro_open: AtomicBool,
    /// #312: version of each column as it was **read** by the
    /// script (`pyst.column`, `pyst.values`), keyed by `(node, handle
    /// epoch)`. Pure Rust data (invariant 9). The context lives as long as
    /// the script thread: the table survives from one console input to the next.
    read_versions: Mutex<HashMap<(NodeId, u64), u64>>,
}

impl ScriptContext {
    /// Retains the version read of a column (the most recent read wins).
    pub(crate) fn remember_version(&self, id: NodeId, epoch: u64, version: u64) {
        if let Ok(mut table) = self.read_versions.lock() {
            table.insert((id, epoch), version);
        }
    }

    /// Version retained for the column, if the script read it.
    pub(crate) fn remembered_version(&self, id: NodeId, epoch: u64) -> Option<u64> {
        self.read_versions
            .lock()
            .ok()
            .and_then(|table| table.get(&(id, epoch)).copied())
    }

    /// Forgets all the retained versions (after an `undo`).
    pub(crate) fn forget_versions(&self) {
        if let Ok(mut table) = self.read_versions.lock() {
            table.clear();
        }
    }

    /// Current project path (`None`: not saved yet).
    pub(crate) fn project_path(&self) -> Option<PathBuf> {
        self.shared
            .project_path
            .lock()
            .ok()
            .and_then(|path| path.clone())
    }

    /// Path that `save()` without argument refuses to overwrite.
    pub(crate) fn protected_path(&self) -> Option<PathBuf> {
        self.shared
            .protected_path
            .lock()
            .ok()
            .and_then(|path| path.clone())
    }

    /// Retains the revision saved by the script.
    pub(crate) fn record_saved(&self, revision: (u64, u64)) {
        if let Ok(mut saved) = self.shared.saved_revision.lock() {
            *saved = Some(revision);
        }
    }

    /// Retains the project path after a `pyst.save_as`.
    pub(crate) fn set_project_path(&self, path: PathBuf) {
        if let Ok(mut current) = self.shared.project_path.lock() {
            *current = Some(path);
        }
    }

    /// Handle to the actor.
    pub(crate) fn session(&self) -> &SessionHandle {
        &self.session
    }

    /// Opens the script's exclusive group. Returns true if the call just
    /// opened it: the bridge must then read again and validate its snapshot, since a
    /// load or a write may have preceded the macro acquisition.
    pub(crate) fn begin_write(&self) -> Result<bool, HostError> {
        if self.macro_open.load(Ordering::Acquire) {
            return Ok(false);
        }
        self.session
            .begin_macro(MacroKind::Script)
            .map_err(HostError::from_session)?;
        self.macro_open.store(true, Ordering::Release);
        Ok(true)
    }

    /// Ends the group with the GIL released, including after a Python exception.
    /// A requested interruption causes an abort even if the script
    /// swallowed it. Imports close only their own nested macros.
    pub(crate) fn finish_write(&self, commit: bool) -> Result<(), HostError> {
        if !self.macro_open.load(Ordering::Acquire) {
            return Ok(());
        }
        let result = if commit {
            self.session.end_macro()
        } else {
            self.session.abort_macro()
        };
        match result {
            Ok(_) => {
                self.macro_open.store(false, Ordering::Release);
                if !commit {
                    // #312: the abort undoes the script's writes: the
                    // retained versions describe nothing anymore.
                    self.forget_versions();
                }
                Ok(())
            }
            Err(error) => {
                if commit && self.session.abort_macro().is_ok() {
                    self.macro_open.store(false, Ordering::Release);
                }
                Err(HostError::from_session(error))
            }
        }
    }

    /// First half of the interruption mechanism: every call of the
    /// `pyst` surface goes through here.
    pub(crate) fn check_interrupt(&self) -> PyResult<()> {
        if self.shared.interrupt_requested.load(Ordering::Acquire) {
            return Err(PyKeyboardInterrupt::new_err(
                "script interrupted at the request of StokesForge",
            ));
        }
        Ok(())
    }
}

thread_local! {
    /// Context of the current thread. Set only on the script thread: this is what
    /// makes the `pyst` surface unusable from a thread that the script
    /// created itself, with a clear message rather than a race.
    static CONTEXT: RefCell<Option<Arc<ScriptContext>>> = const { RefCell::new(None) };
}

/// Context of the current thread, or an explicit Python error.
pub(crate) fn current_context() -> PyResult<Arc<ScriptContext>> {
    CONTEXT.with(|cell| cell.borrow().clone()).ok_or_else(|| {
        PyRuntimeError::new_err(
            "the pyst surface can only be used from the StokesForge script thread",
        )
    })
}

/// A job to run and the single-use reply channel that is
/// its own.
struct Job {
    /// Python source to run.
    source: String,
    /// Channel on which to return the result.
    reply: mpsc::Sender<ScriptOutcome>,
}

/// Message sent to the script thread.
enum Message {
    /// Run a script.
    Run(Job),
    /// Stop the thread.
    Stop,
}

/// Message sent to the interruption thread.
enum InterruptMessage {
    /// Raise an asynchronous exception in the script thread, if it is still
    /// the run carrying this number.
    Now(u64),
    /// Stop the interruption thread.
    Stop,
}

/// The script thread: a permanent service, created once, that owns
/// the interpreter.
///
/// `run` can be called from any thread; this is how the interface
/// will call it, without ever touching Python itself.
pub struct ScriptEngine {
    /// FIFO channel to the script thread.
    jobs: mpsc::Sender<Message>,
    /// Channel to the interruption thread.
    interrupts: mpsc::Sender<InterruptMessage>,
    /// Shared flags.
    shared: Arc<Shared>,
    /// Script thread, `None` after [`ScriptEngine::shutdown`].
    script_thread: Option<JoinHandle<()>>,
    /// Interruption thread, `None` after [`ScriptEngine::shutdown`].
    interrupt_thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl ScriptEngine {
    /// Starts the script thread and initializes the embedded interpreter.
    ///
    /// `python` says where to find the interpreter; `session` is the handle to
    /// the actor that scripts will drive. Blocks until the interpreter is ready,
    /// so that an `Ok` means "the `pyst` surface is installed and a script can
    /// start".
    ///
    /// # Errors
    ///
    /// [`ScriptError::Thread`] if the system refuses to create a thread,
    /// [`ScriptError::Interpreter`] if CPython does not start,
    /// [`ScriptError::Install`] if the `pyst` surface could not be installed.
    pub fn spawn(session: SessionHandle, python: EmbeddedPython) -> Result<Self, ScriptError> {
        // The engine is always a sender distinct from its caller: a plain
        // clone provided by the host must not absorb its writes into the
        // script's macro (nor let the script abort the host's own).
        let session = session.with_new_identity();
        let shared = Arc::new(Shared::default());
        let (jobs_tx, jobs_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();

        let script_shared = Arc::clone(&shared);
        let script_thread = thread::Builder::new()
            .name("stokesforge-script".to_owned())
            .spawn(move || script_main(python, session, jobs_rx, script_shared, &init_tx))?;

        match init_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                // The thread stops by itself after reporting its failure.
                let _ = script_thread.join();
                return Err(error);
            }
            // The thread vanished without a word: only a panic can do that.
            Err(_) => {
                let _ = script_thread.join();
                return Err(ScriptError::Stopped);
            }
        }

        let (interrupts_tx, interrupts_rx) = mpsc::channel();
        let interrupt_shared = Arc::clone(&shared);
        let interrupt_thread = match thread::Builder::new()
            .name("stokesforge-script-irq".to_owned())
            .spawn(move || interrupt_main(&interrupts_rx, &interrupt_shared))
        {
            Ok(thread) => thread,
            Err(error) => {
                // Do not leave an orphaned script thread behind.
                let _ = jobs_tx.send(Message::Stop);
                let _ = script_thread.join();
                return Err(ScriptError::Thread(error));
            }
        };

        Ok(Self {
            jobs: jobs_tx,
            interrupts: interrupts_tx,
            shared,
            script_thread: Some(script_thread),
            interrupt_thread: Some(interrupt_thread),
        })
    }

    /// Tells the engine under which path the project is saved (`None`: it is not
    /// yet). This is what `pyst.save()` writes; the host calls it
    /// before each run. Touches neither the session nor Python.
    pub fn set_project_path(&self, path: Option<PathBuf>) {
        if let Ok(mut current) = self.shared.project_path.lock() {
            *current = path;
        }
    }

    /// Forbids `pyst.save()` (without argument) from overwriting `path`: the host
    /// puts there the source of a read that discarded data. `pyst.save_as`
    /// stays allowed. To be set again before each run.
    pub fn set_protected_path(&self, path: Option<PathBuf>) {
        if let Ok(mut current) = self.shared.protected_path.lock() {
            *current = path;
        }
    }

    /// `(model, geometry)` revision of the script's last `save`/`save_as`
    /// since the last call, once only: the host takes it so as not to
    /// leave the project "modified".
    pub fn take_saved_revision(&self) -> Option<(u64, u64)> {
        self.shared
            .saved_revision
            .lock()
            .ok()
            .and_then(|mut saved| saved.take())
    }

    /// The project's current path as the script sees it: the one set by
    /// [`Self::set_project_path`], or that of a `pyst.save_as` since.
    pub fn project_path(&self) -> Option<PathBuf> {
        self.shared
            .project_path
            .lock()
            .ok()
            .and_then(|path| path.clone())
    }

    /// Runs a Python source and **blocks** until it ends.
    ///
    /// Callable from any thread. The namespace is **kept
    /// from one run to the next**, as in a console: a variable
    /// set by a script stays visible to the next. This is what lets a
    /// handle be kept from one script to another, and also why a numpy
    /// view left in a variable **retains the old buffer** of a
    /// column until it is replaced.
    ///
    /// # Errors
    ///
    /// [`ScriptError::Busy`] if a script is already running (the foundation allows one
    /// at a time), [`ScriptError::Stopped`] if the script thread is stopped.
    /// An exception **of the script** is not an error here: it is returned
    /// in [`ScriptOutcome::error`], with its traceback.
    pub fn run(&self, source: &str) -> Result<ScriptOutcome, ScriptError> {
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ScriptError::Busy);
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        let job = Job {
            source: source.to_owned(),
            reply: reply_tx,
        };
        if self.jobs.send(Message::Run(job)).is_err() {
            self.shared.busy.store(false, Ordering::Release);
            return Err(ScriptError::Stopped);
        }
        reply_rx.recv().map_err(|_| {
            self.shared.busy.store(false, Ordering::Release);
            ScriptError::Stopped
        })
    }

    /// Requests the running script to stop. No effect if there is none.
    ///
    /// **Does not block**: sets a flag and posts a message. The interruption
    /// thread is the one that will wait for the GIL to raise the asynchronous exception.
    pub fn interrupt(&self) {
        self.shared
            .interrupt_requested
            .store(true, Ordering::Release);
        let generation = self.shared.generation.load(Ordering::Acquire);
        let _ = self.interrupts.send(InterruptMessage::Now(generation));
    }

    /// True while a script is running.
    ///
    /// Becomes true when the script thread actually enters the script,
    /// not as soon as [`ScriptEngine::run`] is called: this offset is what makes
    /// an interruption requested "while it is running" impossible to
    /// lose.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// Stops both threads and waits for them to end.
    ///
    /// If a script is still running, its run is carried to completion before
    /// the stop; the stop is not an interruption. Call
    /// [`ScriptEngine::interrupt`] first if you want to cut it short.
    ///
    /// The interpreter is **not** finalized: `Py_Finalize` is a known
    /// source of crashes at exit and brings nothing to a process that
    /// ends. Accepted consequence, the same as in `stokesforge-python`: Python
    /// `atexit` handlers never run.
    ///
    /// # Errors
    ///
    /// [`ScriptError::Stopped`] if either thread panicked.
    pub fn shutdown(mut self) -> Result<(), ScriptError> {
        let _ = self.jobs.send(Message::Stop);
        let _ = self.interrupts.send(InterruptMessage::Stop);
        let mut panicked = false;
        if let Some(thread) = self.script_thread.take() {
            panicked |= thread.join().is_err();
        }
        if let Some(thread) = self.interrupt_thread.take() {
            panicked |= thread.join().is_err();
        }
        if panicked {
            return Err(ScriptError::Stopped);
        }
        Ok(())
    }
}

impl Drop for ScriptEngine {
    /// Requests both threads to stop **without waiting for them**.
    ///
    /// Waiting would be a trap: an engine dropped while a script loops
    /// forever would block the thread that drops it. Whoever wants the guarantee that
    /// everything is over calls [`ScriptEngine::shutdown`].
    fn drop(&mut self) {
        let _ = self.jobs.send(Message::Stop);
        let _ = self.interrupts.send(InterruptMessage::Stop);
    }
}

/// Script thread loop.
fn script_main(
    python: EmbeddedPython,
    session: SessionHandle,
    jobs: mpsc::Receiver<Message>,
    shared: Arc<Shared>,
    init: &mpsc::Sender<Result<(), ScriptError>>,
) {
    let globals = match start(&python, session, &shared) {
        Ok(globals) => {
            let _ = init.send(Ok(()));
            globals
        }
        Err(error) => {
            let _ = init.send(Err(error));
            return;
        }
    };

    for message in jobs {
        match message {
            Message::Stop => break,
            Message::Run(job) => {
                let outcome = execute(&job.source, &globals, &shared);
                // The `busy` flag drops BEFORE the reply: otherwise a client
                // that relaunches at once would still see the engine busy.
                shared.busy.store(false, Ordering::Release);
                let _ = job.reply.send(outcome);
            }
        }
    }

    // The Python objects held by this thread are dropped with the GIL in hand, never
    // as they come: decrementing a reference count is a Python
    // operation.
    Python::attach(move |_py| drop(globals));
}

/// Starts the interpreter, installs the `pyst` surface, the output capture and
/// the persistent namespace.
fn start(
    python: &EmbeddedPython,
    session: SessionHandle,
    shared: &Arc<Shared>,
) -> Result<Py<PyDict>, ScriptError> {
    python.initialize()?;

    let context = Arc::new(ScriptContext {
        session,
        shared: Arc::clone(shared),
        macro_open: AtomicBool::new(false),
        read_versions: Mutex::new(HashMap::new()),
    });
    CONTEXT.with(|cell| *cell.borrow_mut() = Some(context));

    Python::attach(|py| -> PyResult<Py<PyDict>> {
        // The thread's Python identifier, the only argument that
        // `PyThreadState_SetAsyncExc` knows how to recognize.
        let ident = py
            .import("threading")?
            .call_method0("get_ident")?
            .extract::<u64>()?;
        shared.py_thread_ident.store(ident, Ordering::Release);

        surface::install(py)?;
        capture::install(py)?;
        // `traceback` is imported here, once and for all: on the error
        // path, `py.import` then boils down to a read of
        // `sys.modules`, without taking CPython's import lock. An
        // import at the moment of formatting a traceback would be the worst place
        // to get stuck.
        py.import("traceback")?;

        let globals = PyDict::new(py);
        globals.set_item("__name__", "__main__")?;
        globals.set_item("__builtins__", py.import("builtins")?)?;
        globals.set_item("pyst", py.import("pyst")?)?;
        Ok(globals.unbind())
    })
    .map_err(|error| ScriptError::Install(Python::attach(|py| format_exception(py, &error))))
}

/// Runs a source, captures its outputs, formats its possible exception.
fn execute(source: &str, globals: &Py<PyDict>, shared: &Shared) -> ScriptOutcome {
    // Reset BEFORE `running` becomes true: an interruption
    // requested while `is_running()` is already true can no longer be
    // erased by this line.
    shared.interrupt_requested.store(false, Ordering::Release);
    shared.generation.fetch_add(1, Ordering::AcqRel);

    Python::attach(|py| {
        clear_pending_async_exception(py, shared);
        capture::begin();
        shared.running.store(true, Ordering::Release);

        let mut result = match CString::new(source) {
            Ok(code) => py.run(&code, Some(globals.bind(py)), None),
            Err(_) => Err(pyo3::exceptions::PyValueError::new_err(
                "the script source contains a null byte",
            )),
        };

        // These two lines form a whole, and their order is the core of the
        // interruption's safety. They run with the GIL in hand, and the
        // interruption thread rereads `running` with the GIL in hand too: as soon as
        // `running` drops, no asynchronous exception can be armed anymore, and the
        // one that already was is removed before any Python
        // instruction runs to format the result.
        //
        // Without this, an interruption arriving just before the end of a script
        // could fire **inside** an `import` of the error formatting, and leave
        // CPython's global import lock taken forever, that is, freeze the
        // whole interpreter. This is the "naive misuse" that the documentation of
        // `PyThreadState_SetAsyncExc` warns against.
        shared.running.store(false, Ordering::Release);
        clear_pending_async_exception(py, shared);

        let interrupted = shared.interrupt_requested.load(Ordering::Acquire);
        // No Python call during closing: the asynchronous exception
        // is disarmed and the actor receives only Rust data.
        if let Ok(context) = current_context() {
            if let Err(error) = py.detach(|| context.finish_write(result.is_ok() && !interrupted)) {
                result = Err(surface::to_py_error(error));
            }
        }
        let (stdout, stderr) = capture::end();

        ScriptOutcome {
            stdout,
            stderr,
            error: result.err().map(|error| format_exception(py, &error)),
            interrupted,
        }
    })
}

/// Formats a Python exception with its full traceback, chaining
/// included: this is what the user must read to understand their error.
fn format_exception(py: Python<'_>, error: &PyErr) -> String {
    if let Ok(text) = format_with_traceback_module(py, error) {
        return text;
    }
    // Fallback if the `traceback` module is unavailable: the raw trace is
    // better than nothing.
    match error.traceback(py).and_then(|trace| trace.format().ok()) {
        Some(trace) => format!("{trace}{error}"),
        None => error.to_string(),
    }
}

/// Formats through `traceback.format_exception`, as the interpreter would.
fn format_with_traceback_module(py: Python<'_>, error: &PyErr) -> PyResult<String> {
    let parts = py.import("traceback")?.call_method1(
        "format_exception",
        (error.get_type(py), error.value(py), error.traceback(py)),
    )?;
    Ok(parts.extract::<Vec<String>>()?.concat())
}

/// Removes the asynchronous exception possibly pending on the script
/// thread.
fn clear_pending_async_exception(py: Python<'_>, shared: &Shared) {
    let ident = shared.py_thread_ident.load(Ordering::Acquire);
    if ident == 0 {
        return;
    }
    let _ = py;
    // SAFETY: `PyThreadState_SetAsyncExc` requires the GIL (the `py` token is
    // the proof) and a thread identifier; an unknown identifier is a documented
    // no-op (the function then returns 0). The null second argument
    // means "remove the pending exception", transfers no
    // reference and cannot fail.
    unsafe {
        ffi::PyThreadState_SetAsyncExc(ident as c_long, std::ptr::null_mut());
    }
}

/// Interruption thread loop.
fn interrupt_main(messages: &mpsc::Receiver<InterruptMessage>, shared: &Arc<Shared>) {
    while let Ok(message) = messages.recv() {
        match message {
            InterruptMessage::Stop => break,
            InterruptMessage::Now(target) => {
                let ident = shared.py_thread_ident.load(Ordering::Acquire);
                if ident == 0 {
                    continue;
                }
                Python::attach(|py| {
                    // `running` is reread **with the GIL in hand**, and the script thread
                    // sets it to false with the GIL in hand too: the two gestures are
                    // therefore ordered with respect to each other. This is what
                    // guarantees that no asynchronous exception can be armed after a script
                    // ends; a test outside the script would let through an interruption that would
                    // blow up in the next script, or worse, in the middle of a host
                    // `import`.
                    // The number also rules out the **next** script: a `Now` left
                    // queued after an interruption already served by
                    // `check_interrupt` no longer targets the current run.
                    if !shared.running.load(Ordering::Acquire)
                        || shared.generation.load(Ordering::Acquire) != target
                    {
                        return;
                    }
                    let exception = py.get_type::<PyKeyboardInterrupt>();
                    // SAFETY: the GIL is held (`py` token), the identifier
                    // is that of the script thread read after the interpreter started, and
                    // `exception` is a live type object for the duration of the call;
                    // `PyThreadState_SetAsyncExc` steals no
                    // reference. An identifier that became unknown is a documented
                    // no-op.
                    unsafe {
                        ffi::PyThreadState_SetAsyncExc(ident as c_long, exception.as_ptr());
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use stokesforge_core::Project;
    use stokesforge_python::interpreter::{self, EmbeddedPython};
    use stokesforge_session::Session;

    /// #215: a `Now` left queued after a finished script does not strike the next
    /// script. It is posted by hand, once the next one has started.
    #[test]
    fn interruption_perimee_ne_frappe_pas_le_script_suivant() {
        let actor = Session::spawn(Project::new("Course").unwrap(), None).unwrap();
        let prefix = std::env::var_os(interpreter::HOME_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../vendor/python/extracted/python")
            });
        let engine = Arc::new(
            ScriptEngine::spawn(actor.handle(), EmbeddedPython::with_home(prefix)).unwrap(),
        );
        assert!(engine.run("x = 1").unwrap().error.is_none());
        let stale = engine.shared.generation.load(Ordering::Acquire);

        let poster = {
            let engine = Arc::clone(&engine);
            thread::spawn(move || {
                while !engine.is_running() {
                    thread::yield_now();
                }
                let _ = engine.interrupts.send(InterruptMessage::Now(stale));
            })
        };
        let outcome = engine
            .run("import time\nfor _ in range(30):\n    time.sleep(0.01)")
            .unwrap();
        poster.join().unwrap();
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert!(!outcome.interrupted);
    }
}
