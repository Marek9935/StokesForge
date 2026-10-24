//! Démarrage et pilotage de l'interpréteur CPython **embarqué**.
//!
//! Ce module est le point d'entrée unique du processus vers Python. Il résout
//! l'emplacement du préfixe embarqué, démarre l'interpréteur **une seule fois**
//! en mode isolé, et expose quelques fonctions de preuve (aller-retour de
//! tableaux `f64`, versions, état d'isolation).
//!
//! # Isolation (pièges n°4 et n°5)
//!
//! `PYTHONHOME` seul n'isole rien : il laisse entrer `PYTHONPATH`, le
//! `site-packages` utilisateur (`~/.local/lib/python3.12/…`) et les `.pth` du
//! système. On initialise donc par [`ffi::Py_InitializeFromConfig`] avec une
//! [`ffi::PyConfig`] issue de `PyConfig_InitIsolatedConfig`, en fixant
//! `isolated`, `use_environment`, `user_site_directory`, `install_signal_handlers`
//! et surtout `module_search_paths` **explicitement**. Les anciens setters
//! `Py_SetPythonHome`/`Py_SetProgramName` (dépréciés en 3.11, supprimés en 3.13)
//! ne sont pas utilisés, ni aucune variable d'environnement.
//!
//! PyO3 0.29 n'offre **aucune** API sûre pour une configuration d'initialisation
//! personnalisée : `Python::initialize()` appelle `Py_InitializeEx(0)`, qui lit
//! l'environnement. Le passage par `pyo3::ffi` est donc obligatoire ici — c'est
//! le seul `unsafe` de l'initialisation ; le crate n'en porte qu'un autre,
//! l'emprunt sans copie de `array.rs` (`PyArray1::borrow_from_array`).
//!
//! # Fin de vie (piège n°7)
//!
//! `Py_Finalize` n'est **jamais** appelé. Conséquence assumée : les `atexit`
//! Python ne s'exécutent pas.

use std::ffi::{CString, NulError};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use numpy::{PyArray1, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTracebackMethods};

/// Version courte de l'interpréteur embarqué, verrouillée avec la roue numpy
/// et la version de rust-numpy (cf. `vendor/python/lock.env`, piège n°6).
pub const PYTHON_XY: &str = "3.12";

/// Variable d'environnement de surcharge du préfixe embarqué.
///
/// **Réservée aux tests et au développement** : elle permet de pointer sur
/// `vendor/python/extracted/python` sans assembler une distribution. Ce n'est
/// pas un mécanisme utilisateur — l'application livrée résout toujours son
/// préfixe à côté de son exécutable, et rien dans l'interface ne l'expose.
pub const HOME_ENV: &str = "STOKESFORGE_PYTHON_HOME";

/// Résultat de l'initialisation, mémorisé pour rendre [`EmbeddedPython::initialize`]
/// idempotent : CPython ne peut être démarré qu'une fois par processus.
static INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Erreurs du pont Python.
#[derive(Debug, thiserror::Error)]
pub enum PythonError {
    /// Le préfixe embarqué est absent ou mal formé (pas de `lib/python3.12`).
    #[error("embedded Python prefix not found: {0} (expected {0}/lib/python{PYTHON_XY})")]
    HomeNotFound(PathBuf),
    /// L'interpréteur n'a pas pu démarrer.
    #[error("could not initialise the embedded interpreter: {0}")]
    InitFailed(String),
    /// Une exception Python est remontée, avec sa trace formatée.
    #[error("Python error:\n{0}")]
    Python(String),
}

/// Préfixe Python embarqué et interpréteur associé.
#[derive(Debug, Clone)]
pub struct EmbeddedPython {
    home: PathBuf,
}

impl EmbeddedPython {
    /// Résout le préfixe embarqué et vérifie qu'il contient bien une
    /// bibliothèque standard.
    ///
    /// Ordre de résolution :
    /// 1. la variable [`HOME_ENV`] si elle est posée (tests et développement) ;
    /// 2. sinon `<répertoire de l'exécutable>/lib/python`, disposition produite
    ///    par `scripts/pyhello-dist.sh`.
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

    /// Construit un préfixe sans vérification (utile pour les tests).
    #[must_use]
    pub fn with_home(home: PathBuf) -> Self {
        Self { home }
    }

    /// Racine du préfixe embarqué.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Vrai si `entry` (une entrée de `sys.path`) est sous le préfixe.
    /// Comparaison par composants, pas par préfixe de chaîne : sous Windows,
    /// le préfixe peut arriver en `C:/…` (Git Bash) quand `sys.path` porte
    /// `C:\…` (#332).
    #[must_use]
    pub fn contains_path(&self, entry: &str) -> bool {
        Path::new(entry).starts_with(&self.home)
    }

    /// La bibliothèque standard : `<home>/lib/python3.12` sous Unix,
    /// `<home>/Lib` dans la distribution python-build-standalone de Windows.
    fn stdlib_dir(&self) -> PathBuf {
        #[cfg(windows)]
        return self.home.join("Lib");
        #[cfg(not(windows))]
        return self.home.join("lib").join(format!("python{PYTHON_XY}"));
    }

    /// Les modules d'extension : `lib-dynload` sous Unix, `<home>/DLLs`
    /// (`.pyd`) sous Windows.
    fn dynload_dir(&self) -> PathBuf {
        #[cfg(windows)]
        return self.home.join("DLLs");
        #[cfg(not(windows))]
        return self.stdlib_dir().join("lib-dynload");
    }

    /// Les trois répertoires injectés dans `sys.path`, dans l'ordre.
    ///
    /// Rien d'autre n'y entre : ni `PYTHONPATH`, ni `~/.local`, ni les `.pth`
    /// d'un Python système.
    fn search_paths(&self) -> [PathBuf; 3] {
        let stdlib = self.stdlib_dir();
        let dynload = self.dynload_dir();
        let site = stdlib.join("site-packages");
        [stdlib, dynload, site]
    }

    /// Démarre l'interpréteur, **une seule fois par processus**, en mode isolé,
    /// sans gestion des signaux et sans finalisation, puis importe numpy.
    ///
    /// Idempotent : les appels suivants renvoient le même résultat sans rien
    /// refaire. Le premier appelant fixe le préfixe utilisé par le processus.
    pub fn initialize(&self) -> Result<(), PythonError> {
        let outcome = INIT.get_or_init(|| {
            // SAFETY : appelé une seule fois, avant tout autre usage de la
            // C-API, depuis le fil qui deviendra le fil principal de CPython.
            unsafe { start_interpreter(self) }?;
            // Import de numpy inclus dans la mesure d'initialisation : c'est le
            // coût réel de « Python prêt à servir » pour StokesForge.
            import_numpy()
        });
        match outcome {
            Ok(()) => Ok(()),
            Err(message) => Err(PythonError::InitFailed(message.clone())),
        }
    }

    /// Vrai si l'interpréteur a été démarré avec succès dans ce processus.
    #[must_use]
    pub fn is_initialized() -> bool {
        matches!(INIT.get(), Some(Ok(())))
    }
}

/// Démarre CPython à partir d'une `PyConfig` isolée.
///
/// # Safety
///
/// Ne doit être appelée qu'une fois, avant tout autre appel à la C-API Python.
unsafe fn start_interpreter(embedded: &EmbeddedPython) -> Result<(), String> {
    let home = to_cstring(&embedded.home)?;
    let paths = embedded.search_paths();

    let mut config: ffi::PyConfig = std::mem::zeroed();
    let config_ptr: *mut ffi::PyConfig = &mut config;
    ffi::PyConfig_InitIsolatedConfig(config_ptr);

    // Verrouillage explicite de l'isolation. `PyConfig_InitIsolatedConfig` pose
    // déjà la plupart de ces valeurs ; on les réaffirme pour que l'intention
    // soit lisible et ne dépende pas des défauts d'une version de CPython.
    (*config_ptr).isolated = 1;
    (*config_ptr).use_environment = 0;
    (*config_ptr).user_site_directory = 0;
    (*config_ptr).install_signal_handlers = 0;
    (*config_ptr).parse_argv = 0;
    (*config_ptr).pathconfig_warnings = 0;

    // `home` : sert de `sys.prefix`/`sys.exec_prefix`.
    let status = ffi::PyConfig_SetBytesString(
        config_ptr,
        std::ptr::addr_of_mut!((*config_ptr).home),
        home.as_ptr(),
    );
    if let Err(message) = check_status(status, "PyConfig.home") {
        ffi::PyConfig_Clear(config_ptr);
        return Err(message);
    }

    // `module_search_paths` : c'est ce champ, et lui seul, qui fabrique
    // `sys.path`. Avec `module_search_paths_set = 1`, CPython ne calcule plus
    // rien et n'ajoute plus rien depuis l'environnement.
    (*config_ptr).module_search_paths_set = 1;
    for path in &paths {
        if let Err(message) = append_search_path(config_ptr, path) {
            ffi::PyConfig_Clear(config_ptr);
            return Err(message);
        }
    }

    let status = ffi::Py_InitializeFromConfig(config_ptr);
    // La config est copiée par CPython : on peut la libérer aussitôt.
    ffi::PyConfig_Clear(config_ptr);
    check_status(status, "Py_InitializeFromConfig")?;

    // L'initialisation laisse le GIL pris par ce fil. On le relâche pour que
    // `Python::attach` (donc `PyGILState_Ensure`) fonctionne depuis n'importe
    // quel fil, y compris celui-ci. Même geste que `Python::initialize` de PyO3.
    ffi::PyEval_SaveThread();
    Ok(())
}

/// Ajoute un répertoire à `config.module_search_paths`.
///
/// # Safety
///
/// `config_ptr` doit pointer sur une `PyConfig` valide en cours de construction.
unsafe fn append_search_path(config_ptr: *mut ffi::PyConfig, path: &Path) -> Result<(), String> {
    let bytes = to_cstring(path)?;
    // `Py_DecodeLocale` est l'API documentée pour fabriquer un `wchar_t*` avant
    // l'initialisation ; la mémoire rendue se libère par `PyMem_RawFree`.
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

/// Traduit un `PyStatus` en `Result`.
fn check_status(status: ffi::PyStatus, what: &str) -> Result<(), String> {
    // SAFETY : `PyStatus_Exception` ne fait qu'inspecter la structure passée par
    // valeur ; les pointeurs de message appartiennent à CPython et restent
    // valides le temps de la lecture.
    let failed = unsafe { ffi::PyStatus_Exception(status) } != 0;
    if !failed {
        return Ok(());
    }
    let detail = if status.err_msg.is_null() {
        "no message".to_owned()
    } else {
        // SAFETY : pointeur non nul fourni par CPython sur une chaîne C statique.
        unsafe { std::ffi::CStr::from_ptr(status.err_msg) }
            .to_string_lossy()
            .into_owned()
    };
    Err(format!("{what}: {detail}"))
}

/// Importe numpy une fois l'interpréteur démarré ; échoue avec la trace Python.
fn import_numpy() -> Result<(), String> {
    Python::attach(|py| match py.import("numpy") {
        Ok(_) => Ok(()),
        Err(err) => Err(format!("import numpy: {}", format_error(py, &err))),
    })
}

/// Convertit un chemin en `CString` pour les API `*_SetBytesString`.
fn to_cstring(path: &Path) -> Result<CString, String> {
    let text = path
        .to_str()
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))?;
    CString::new(text).map_err(|err: NulError| format!("path contains a null byte: {err}"))
}

/// Prend le GIL et convertit toute exception Python en [`PythonError::Python`]
/// avec sa trace formatée.
pub(crate) fn attached<T>(
    f: impl for<'py> FnOnce(Python<'py>) -> PyResult<T>,
) -> Result<T, PythonError> {
    Python::attach(|py| f(py).map_err(|err| PythonError::Python(format_error(py, &err))))
}

/// Formate une exception Python avec sa trace, comme le ferait `traceback.print_exc`.
fn format_error(py: Python<'_>, err: &PyErr) -> String {
    match err.traceback(py).and_then(|tb| tb.format().ok()) {
        Some(trace) => format!("{trace}{err}"),
        None => err.to_string(),
    }
}

/// Aller simple : copie `values` dans un `ndarray` numpy, y calcule `np.sum`.
///
/// La copie est **assumée** ici : la lecture sans copie (emprunt d'un
/// `Arc<Vec<f64>>` côté Rust) est la tâche 5.2.
pub fn roundtrip_sum(values: &[f64]) -> Result<f64, PythonError> {
    attached(|py| {
        let array = PyArray1::from_slice(py, values);
        let numpy = py.import("numpy")?;
        numpy.call_method1("sum", (array,))?.extract::<f64>()
    })
}

/// Aller-retour complet : copie vers numpy, `arr * 2.0` côté Python, rapatriement.
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

/// Temps des deux moitiés d'un aller-retour, en millisecondes.
#[derive(Debug, Clone, Copy)]
pub struct CopyTimings {
    /// Rust → numpy : allocation du `ndarray` et copie des octets.
    pub send_ms: f64,
    /// numpy → Rust : copie vers un `Vec<f64>` neuf.
    pub recv_ms: f64,
}

/// Mesure séparément l'aller et le retour d'une **copie** de `values`, dans une
/// seule prise du GIL, sans calcul Python entre les deux.
///
/// C'est le chiffre qui décide de l'intérêt du zéro-copie (tâche 5.2).
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

        // Garde-fou : empêche l'optimiseur d'escamoter la copie retour.
        debug_assert_eq!(back.len(), values.len());
        std::hint::black_box(&back);

        Ok(CopyTimings { send_ms, recv_ms })
    })
}

/// Version de numpy vue par l'interpréteur embarqué.
pub fn numpy_version() -> Result<String, PythonError> {
    attached(|py| {
        py.import("numpy")?
            .getattr("__version__")?
            .extract::<String>()
    })
}

/// Version de l'interpréteur embarqué, au format `X.Y.Z`.
pub fn python_version() -> Result<String, PythonError> {
    attached(|py| {
        py.import("platform")?
            .call_method0("python_version")?
            .extract::<String>()
    })
}

/// `sys.prefix` — doit pointer dans le préfixe embarqué.
pub fn sys_prefix() -> Result<String, PythonError> {
    attached(|py| py.import("sys")?.getattr("prefix")?.extract::<String>())
}

/// `sys.path` — preuve d'isolation : ne doit contenir que le préfixe embarqué.
pub fn sys_path() -> Result<Vec<String>, PythonError> {
    attached(|py| py.import("sys")?.getattr("path")?.extract::<Vec<String>>())
}

/// `sys.flags.isolated` — preuve que le mode isolé est actif.
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
