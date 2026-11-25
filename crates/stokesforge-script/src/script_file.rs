//! Running a Python **file** (#552): the file environment of one run.
//!
//! A file run behaves like `python file.py`: its folder comes first in
//! `sys.path`, `__file__` and `sys.argv` describe it, `sys.modules["__main__"]`
//! is its namespace, and nothing is written next to it (`__pycache__`). The
//! process-global state it changes (`sys.path`, `sys.argv`,
//! `sys.modules`, `sys.dont_write_bytecode`) is put back afterwards, and the
//! modules imported from the script's folder are forgotten so that an edited
//! helper is read again at the next run.
//!
//! The work is done by a small Python helper, compiled once per call into a
//! private dictionary (never into `sys.modules`). [`FileRun::enter`] runs
//! **before** the engine flags the script as running and [`FileRun::leave`]
//! **after** it cleared them, so that an asynchronous exception (Stop) can never
//! fire inside the import machinery (see `engine::execute`).

use std::ffi::CString;
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

/// The helper, in Python: environment set-up, restoration and purge.
const HELPER: &str = r#"
import sys, os, types, importlib, importlib.machinery

def _norm(p):
    return os.path.normcase(os.path.abspath(os.fspath(p)))

def _under(p, root):
    try:
        return os.path.commonpath([p, root]) == root
    except ValueError:
        return False

def _origins(mod):
    out = []
    f = getattr(mod, "__file__", None)
    if isinstance(f, str):
        out.append(_norm(f))
    try:
        for entry in list(getattr(mod, "__path__", None) or ()):
            if isinstance(entry, str):
                out.append(_norm(entry))
    except Exception:
        pass
    return out

def purge(folder):
    """Forget the modules that live under `folder` (and not in the embedded prefix)."""
    root = _norm(folder)
    prefixes = {_norm(p) for p in (sys.prefix, sys.exec_prefix, sys.base_prefix, sys.base_exec_prefix)}
    suffixes = tuple(importlib.machinery.EXTENSION_SUFFIXES)
    doomed = []
    keep = set()
    for name, mod in list(sys.modules.items()):
        if name == "__main__" or mod is None:
            continue
        origins = _origins(mod)
        if not origins or any(_under(o, p) for o in origins for p in prefixes):
            continue
        if not any(_under(o, root) for o in origins):
            continue
        f = getattr(mod, "__file__", None)
        if isinstance(f, str) and f.endswith(suffixes):
            # A native extension cannot be initialised twice: keep it and
            # the packages that hold it.
            parts = name.split(".")
            for i in range(1, len(parts) + 1):
                keep.add(".".join(parts[:i]))
            continue
        doomed.append(name)
    for name in doomed:
        if name not in keep:
            sys.modules.pop(name, None)
    importlib.invalidate_caches()

_MISSING = object()

def enter(folder, path, builtins_module, pyst_module):
    # PyO3 hands a `pathlib.Path`; `sys.path` and `sys.argv` hold `str`.
    folder = os.fspath(folder)
    path = os.fspath(path)
    state = (
        sys.path,
        list(sys.path),
        getattr(sys, "argv", _MISSING),
        sys.modules.get("__main__", _MISSING),
        sys.dont_write_bytecode,
    )
    purge(folder)
    sys.path.insert(0, folder)
    sys.argv = [path]
    sys.dont_write_bytecode = True
    ns = types.ModuleType("__main__")
    ns.__builtins__ = builtins_module
    ns.pyst = pyst_module
    ns.__file__ = os.path.abspath(path)
    sys.modules["__main__"] = ns
    importlib.invalidate_caches()
    return state, ns

def leave(state, folder):
    path_obj, saved_path, argv, main, dont_write = state
    path_obj[:] = saved_path
    sys.path = path_obj
    if argv is _MISSING:
        if hasattr(sys, "argv"):
            del sys.argv
    else:
        sys.argv = argv
    if main is _MISSING:
        sys.modules.pop("__main__", None)
    else:
        sys.modules["__main__"] = main
    sys.dont_write_bytecode = dont_write
    purge(folder)
"#;

/// The environment of one file run, between [`FileRun::enter`] and
/// [`FileRun::leave`].
pub(crate) struct FileRun {
    /// The Python helper namespace (holds `leave`).
    helper: Py<PyDict>,
    /// What `enter` returned to restore.
    state: Py<PyAny>,
    /// The folder of the script, as given (no symlink resolved).
    folder: PathBuf,
    /// The namespace the file runs in: the `__dict__` of the fake `__main__`.
    namespace: Py<PyDict>,
    /// The fake `__main__` module itself (kept alive during the run).
    module: Py<PyAny>,
    /// The path as given by the caller, for `sys.argv` and tracebacks.
    path: PathBuf,
}

/// The folder whose modules are importable: the parent of the absolute path,
/// with no symlink resolved and no `\\?\` canonicalisation.
fn folder_of(absolute: &Path) -> PathBuf {
    absolute
        .parent()
        .map_or_else(|| absolute.to_path_buf(), Path::to_path_buf)
}

impl FileRun {
    /// Sets the environment up. Call with the engine's `running` flag still
    /// false.
    pub(crate) fn enter(py: Python<'_>, path: &Path) -> PyResult<Self> {
        let absolute = std::path::absolute(path)?;
        let folder = folder_of(&absolute);
        let helper = PyDict::new(py);
        let code = CString::new(HELPER)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("helper contains a null byte"))?;
        py.run(&code, Some(&helper), None)?;
        let enter = helper
            .get_item("enter")?
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("helper has no enter"))?;
        let builtins = py.import("builtins")?;
        let pyst = py.import("pyst")?;
        let result = enter.call1((folder.as_path(), absolute.as_path(), &builtins, &pyst))?;
        let (state, module): (Bound<'_, PyAny>, Bound<'_, PyAny>) = result.extract()?;
        let namespace = module.getattr("__dict__")?.cast_into::<PyDict>()?;
        Ok(Self {
            helper: helper.unbind(),
            state: state.unbind(),
            folder,
            namespace: namespace.unbind(),
            module: module.unbind(),
            path: path.to_path_buf(),
        })
    }

    /// Compiles `source` **from bytes** (BOM, CRLF, encoding cookie and
    /// `from __future__` are Python's own business) under the real file name
    /// and runs it in the namespace.
    pub(crate) fn run(&self, py: Python<'_>, source: &[u8]) -> PyResult<()> {
        let builtins = py.import("builtins")?;
        let bytes = PyBytes::new(py, source);
        let code = builtins.call_method1(
            "compile",
            (bytes, self.absolute().as_path(), "exec", 0, true),
        )?;
        builtins.call_method1("exec", (code, self.namespace.bind(py)))?;
        Ok(())
    }

    /// The absolute path of the script.
    fn absolute(&self) -> PathBuf {
        std::path::absolute(&self.path).unwrap_or_else(|_| self.path.clone())
    }

    /// Puts the process-global state back and forgets the script folder's
    /// modules. Call after `running` dropped and the pending asynchronous
    /// exception was cleared.
    pub(crate) fn leave(self, py: Python<'_>) -> PyResult<()> {
        let leave = self
            .helper
            .bind(py)
            .get_item("leave")?
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("helper has no leave"))?;
        leave.call1((self.state.bind(py), self.folder.as_path()))?;
        drop(self.module);
        Ok(())
    }
}
