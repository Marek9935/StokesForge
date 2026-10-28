//! Startup and driving of the **embedded** CPython interpreter.
//!
//! This module is the process's single entry point to Python. It resolves
//! the location of the embedded prefix, starts the interpreter **only once**
//! in isolated mode, and exposes a few proof functions (round trip of
//! `f64` arrays, versions, isolation state).
//!
//! # Isolation (pitfalls no. 4 and no. 5)
//!
//! `PYTHONHOME` alone isolates nothing: it lets `PYTHONPATH`, the user
//! `site-packages` (`~/.local/lib/python3.12/…`) and the system's `.pth` files
//! in. So we initialize through [`ffi::Py_InitializeFromConfig`] with a
//! [`ffi::PyConfig`] coming from `PyConfig_InitIsolatedConfig`, setting
//! `isolated`, `use_environment`, `user_site_directory`, `install_signal_handlers`
//! and above all `module_search_paths` **explicitly**. The old setters
//! `Py_SetPythonHome`/`Py_SetProgramName` (deprecated in 3.11, removed in 3.13)
//! are not used, nor is any environment variable.
//!
//! PyO3 0.29 offers **no** safe API for a custom initialization
//! configuration: `Python::initialize()` calls `Py_InitializeEx(0)`, which reads
//! the environment. Going through `pyo3::ffi` is therefore mandatory here — this is
//! the only `unsafe` of the initialization; the crate carries just one other,
//! the zero-copy borrow of `array.rs` (`PyArray1::borrow_from_array`).
//!
//! # End of life (pitfall no. 7)
//!
//! `Py_Finalize` is **never** called. Accepted consequence: Python `atexit`
//! handlers do not run.

use std::ffi::{CString, NulError};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use numpy::{PyArray1, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTracebackMethods};

/// Short version of the embedded interpreter, locked together with the numpy wheel
/// and the rust-numpy version (see `vendor/python/lock.env`, pitfall no. 6).
pub const PYTHON_XY: &str = "3.12";

/// Environment variable overriding the embedded prefix.
///
/// **Reserved for tests and development**: it lets us point to
/// `vendor/python/extracted/python` without assembling a distribution. It is not
/// a user mechanism — the shipped application always resolves its
/// prefix next to its executable, and nothing in the interface exposes it.
pub const HOME_ENV: &str = "STOKESFORGE_PYTHON_HOME";

/// Result of the initialization, remembered to make [`EmbeddedPython::initialize`]
/// idempotent: CPython can be started only once per process.
static INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Errors of the Python bridge.
#[derive(Debug, thiserror::Error)]
pub enum PythonError {
    /// The embedded prefix is missing or malformed (no `lib/python3.12`).
    #[error("embedded Python prefix not found: {0} (expected {0}/lib/python{PYTHON_XY})")]
    HomeNotFound(PathBuf),
    /// The interpreter could not start.
    #[error("could not initialise the embedded interpreter: {0}")]
    InitFailed(String),
    /// A Python exception was raised, with its formatted traceback.
    #[error("Python error:\n{0}")]
    Python(String),
}

/// Embedded Python prefix and its associated interpreter.
#[derive(Debug, Clone)]
pub struct EmbeddedPython {
    home: PathBuf,
}

impl EmbeddedPython {
    /// Resolves the embedded prefix and checks that it really contains a
    /// standard library.
    ///
    /// Resolution order:
    /// 1. the [`HOME_ENV`] variable if set (tests and development);
    /// 2. otherwise `<executable directory>/lib/python`, the layout produced
    ///    by `scripts/pyhello-dist.sh`.
    pub fn locate() -> Result<Self, PythonError> {
        let candidate = match std::env::var_os(HOME_ENV) {
            Some(value) => PathBuf::from(value),
            None => {
                let exe = std::env::current_exe()
                    .map_err(|err| PythonError::InitFailed(format!("current_exe: {err}")))?;
                let dir = exe.parent().ok_or_else(|| {
                    PythonError::InitFailed("executable without a parent directory".to_owned())
                })?;
                dir.join("lib").join("python")
            }
        };
        let embedded = Self::with_home(candidate);
        if embedded.stdlib_dir().is_dir() {
            Ok(embedded)
        } else {
            Err(PythonError::HomeNotFound(embedded.home))
        }
    }

    /// Builds a prefix without checking (useful for tests).
    #[must_use]
    pub fn with_home(home: PathBuf) -> Self {
        Self { home }
    }

    /// Root of the embedded prefix.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// True if `entry` (a `sys.path` entry) is under the prefix.
    /// Compared by components, not by string prefix: on Windows,
    /// the prefix may arrive as `C:/…` (Git Bash) when `sys.path` carries
    /// `C:\…` (#332).
    #[must_use]
    pub fn contains_path(&self, entry: &str) -> bool {
        Path::new(entry).starts_with(&self.home)
    }

    /// The standard library: `<home>/lib/python3.12` on Unix,
    /// `<home>/Lib` in the Windows python-build-standalone distribution.
    fn stdlib_dir(&self) -> PathBuf {
        #[cfg(windows)]
        return self.home.join("Lib");
        #[cfg(not(windows))]
        return self.home.join("lib").join(format!("python{PYTHON_XY}"));
    }

    /// The extension modules: `lib-dynload` on Unix, `<home>/DLLs`
    /// (`.pyd`) on Windows.
    fn dynload_dir(&self) -> PathBuf {
        #[cfg(windows)]
        return self.home.join("DLLs");
        #[cfg(not(windows))]
        return self.stdlib_dir().join("lib-dynload");
    }

    /// The three directories injected into `sys.path`, in order.
    ///
    /// Nothing else gets in: no `PYTHONPATH`, no `~/.local`, no `.pth` files
    /// of a system Python.
    fn search_paths(&self) -> [PathBuf; 3] {
        let stdlib = self.stdlib_dir();
        let dynload = self.dynload_dir();
        let site = stdlib.join("site-packages");
        [stdlib, dynload, site]
    }

    /// Starts the interpreter, **only once per process**, in isolated mode,
    /// without signal handling and without finalization, then imports numpy.
    ///
    /// Idempotent: later calls return the same result without redoing
    /// anything. The first caller fixes the prefix used by the process.
    pub fn initialize(&self) -> Result<(), PythonError> {
        let outcome = INIT.get_or_init(|| {
            // SAFETY: called only once, before any other use of the
            // C API, from the thread that will become CPython's main thread.
            unsafe { start_interpreter(self) }?;
            // The numpy import is part of the initialization measurement: it is the
            // real cost of "Python ready to serve" for StokesForge.
            import_numpy()
        });
        match outcome {
            Ok(()) => Ok(()),
            Err(message) => Err(PythonError::InitFailed(message.clone())),
        }
    }

    /// True if the interpreter was started successfully in this process.
    #[must_use]
    pub fn is_initialized() -> bool {
        matches!(INIT.get(), Some(Ok(())))
    }
}

/// Starts CPython from an isolated `PyConfig`.
///
/// # Safety
///
/// Must be called only once, before any other call to the Python C API.
unsafe fn start_interpreter(embedded: &EmbeddedPython) -> Result<(), String> {
    let home = to_cstring(&embedded.home)?;
    let paths = embedded.search_paths();

    let mut config: ffi::PyConfig = std::mem::zeroed();
    let config_ptr: *mut ffi::PyConfig = &mut config;
    ffi::PyConfig_InitIsolatedConfig(config_ptr);

    // Explicit lock-down of the isolation. `PyConfig_InitIsolatedConfig` already
    // sets most of these values; we restate them so that the intent is
    // readable and does not depend on the defaults of a CPython version.
    (*config_ptr).isolated = 1;
    (*config_ptr).use_environment = 0;
    (*config_ptr).user_site_directory = 0;
    (*config_ptr).install_signal_handlers = 0;
    (*config_ptr).parse_argv = 0;
    (*config_ptr).pathconfig_warnings = 0;

    // `home`: serves as `sys.prefix`/`sys.exec_prefix`.
    let status = ffi::PyConfig_SetBytesString(
        config_ptr,
        std::ptr::addr_of_mut!((*config_ptr).home),
        home.as_ptr(),
    );
    if let Err(message) = check_status(status, "PyConfig.home") {
        ffi::PyConfig_Clear(config_ptr);
        return Err(message);
    }

    // `module_search_paths`: this field, and only this one, builds
    // `sys.path`. With `module_search_paths_set = 1`, CPython computes
    // nothing more and adds nothing more from the environment.
    (*config_ptr).module_search_paths_set = 1;
    for path in &paths {
        if let Err(message) = append_search_path(config_ptr, path) {
            ffi::PyConfig_Clear(config_ptr);
            return Err(message);
        }
    }

    let status = ffi::Py_InitializeFromConfig(config_ptr);
    // The config is copied by CPython: we can free it right away.
    ffi::PyConfig_Clear(config_ptr);
    check_status(status, "Py_InitializeFromConfig")?;

    // Initialization leaves the GIL held by this thread. We release it so that
    // `Python::attach` (hence `PyGILState_Ensure`) works from any
    // thread, this one included. Same gesture as PyO3's `Python::initialize`.
    ffi::PyEval_SaveThread();
    Ok(())
}

/// Appends a directory to `config.module_search_paths`.
///
/// # Safety
///
/// `config_ptr` must point to a valid `PyConfig` under construction.
unsafe fn append_search_path(config_ptr: *mut ffi::PyConfig, path: &Path) -> Result<(), String> {
    let bytes = to_cstring(path)?;
    // `Py_DecodeLocale` is the documented API to build a `wchar_t*` before
    // initialization; the returned memory is freed with `PyMem_RawFree`.
    let wide = ffi::Py_DecodeLocale(bytes.as_ptr(), std::ptr::null_mut());
    if wide.is_null() {
        return Err(format!("Py_DecodeLocale failed on {}", path.display()));
    }
    let status = ffi::PyWideStringList_Append(
        std::ptr::addr_of_mut!((*config_ptr).module_search_paths),
        wide,
    );
    ffi::PyMem_RawFree(wide.cast());
    check_status(status, "PyWideStringList_Append")
}

/// Translates a `PyStatus` into a `Result`.
fn check_status(status: ffi::PyStatus, what: &str) -> Result<(), String> {
    // SAFETY: `PyStatus_Exception` only inspects the structure passed by
    // value; the message pointers belong to CPython and stay valid for the
    // duration of the read.
    let failed = unsafe { ffi::PyStatus_Exception(status) } != 0;
    if !failed {
        return Ok(());
    }
    let detail = if status.err_msg.is_null() {
        "no message".to_owned()
    } else {
        // SAFETY: non-null pointer provided by CPython to a static C string.
        unsafe { std::ffi::CStr::from_ptr(status.err_msg) }
            .to_string_lossy()
            .into_owned()
    };
    Err(format!("{what}: {detail}"))
}

/// Imports numpy once the interpreter has started; fails with the Python traceback.
fn import_numpy() -> Result<(), String> {
    Python::attach(|py| match py.import("numpy") {
        Ok(_) => Ok(()),
        Err(err) => Err(format!("import numpy: {}", format_error(py, &err))),
    })
}

/// Converts a path into a `CString` for the `*_SetBytesString` APIs.
fn to_cstring(path: &Path) -> Result<CString, String> {
    let text = path
        .to_str()
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))?;
    CString::new(text).map_err(|err: NulError| format!("path contains a null byte: {err}"))
}

/// Takes the GIL and converts any Python exception into [`PythonError::Python`]
/// with its formatted traceback.
pub(crate) fn attached<T>(
    f: impl for<'py> FnOnce(Python<'py>) -> PyResult<T>,
) -> Result<T, PythonError> {
    Python::attach(|py| f(py).map_err(|err| PythonError::Python(format_error(py, &err))))
}

/// Formats a Python exception with its traceback, as `traceback.print_exc` would.
fn format_error(py: Python<'_>, err: &PyErr) -> String {
    match err.traceback(py).and_then(|tb| tb.format().ok()) {
        Some(trace) => format!("{trace}{err}"),
        None => err.to_string(),
    }
}

/// One way: copies `values` into a numpy `ndarray`, computes `np.sum` there.
///
/// The copy is **accepted** here: zero-copy reading (borrowing an
/// `Arc<Vec<f64>>` on the Rust side) is task 5.2.
pub fn roundtrip_sum(values: &[f64]) -> Result<f64, PythonError> {
    attached(|py| {
        let array = PyArray1::from_slice(py, values);
        let numpy = py.import("numpy")?;
        numpy.call_method1("sum", (array,))?.extract::<f64>()
    })
}

/// Full round trip: copy to numpy, `arr * 2.0` on the Python side, bring back.
pub fn roundtrip_vec(values: &[f64]) -> Result<Vec<f64>, PythonError> {
    attached(|py| {
        let array = PyArray1::from_slice(py, values);
        let locals = PyDict::new(py);
        locals.set_item("arr", &array)?;
        let doubled = py
            .eval(c"arr * 2.0", None, Some(&locals))?
            .cast_into::<PyArray1<f64>>()?;
        doubled
            .to_vec()
            .map_err(|err| PyValueError::new_err(err.to_string()))
    })
}

/// Timings of the two halves of a round trip, in milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct CopyTimings {
    /// Rust → numpy: allocation of the `ndarray` and copy of the bytes.
    pub send_ms: f64,
    /// numpy → Rust: copy to a fresh `Vec<f64>`.
    pub recv_ms: f64,
}

/// Measures separately the way there and the way back of a **copy** of `values`, in a
/// single GIL acquisition, with no Python computation in between.
///
/// This is the figure that decides the value of zero-copy (task 5.2).
pub fn measure_copy(values: &[f64]) -> Result<CopyTimings, PythonError> {
    attached(|py| {
        let before_send = Instant::now();
        let array = PyArray1::from_slice(py, values);
        let send_ms = before_send.elapsed().as_secs_f64() * 1e3;

        let before_recv = Instant::now();
        let back = array
            .to_vec()
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        let recv_ms = before_recv.elapsed().as_secs_f64() * 1e3;

        // Guard: keeps the optimizer from eliding the return copy.
        debug_assert_eq!(back.len(), values.len());
        std::hint::black_box(&back);

        Ok(CopyTimings { send_ms, recv_ms })
    })
}

/// numpy version seen by the embedded interpreter.
pub fn numpy_version() -> Result<String, PythonError> {
    attached(|py| {
        py.import("numpy")?
            .getattr("__version__")?
            .extract::<String>()
    })
}

/// Version of the embedded interpreter, in `X.Y.Z` format.
pub fn python_version() -> Result<String, PythonError> {
    attached(|py| {
        py.import("platform")?
            .call_method0("python_version")?
            .extract::<String>()
    })
}

/// `sys.prefix` — must point inside the embedded prefix.
pub fn sys_prefix() -> Result<String, PythonError> {
    attached(|py| py.import("sys")?.getattr("prefix")?.extract::<String>())
}

/// `sys.path` — proof of isolation: must contain only the embedded prefix.
pub fn sys_path() -> Result<Vec<String>, PythonError> {
    attached(|py| py.import("sys")?.getattr("path")?.extract::<Vec<String>>())
}

/// `sys.flags.isolated` — proof that isolated mode is active.
pub fn sys_flags_isolated() -> Result<bool, PythonError> {
    attached(|py| {
        let flag = py
            .import("sys")?
            .getattr("flags")?
            .getattr("isolated")?
            .extract::<i64>()?;
        Ok(flag != 0)
    })
}
