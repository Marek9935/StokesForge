//! Integration tests of the embedded Python bridge.
//!
//! They run only with the `python` feature and require the vendored
//! prefix: run `scripts/python-vendor.sh` then `scripts/python-test.sh`.
//!
//! CPython starts only once per process: all the tests of a single
//! test binary therefore share **the same** interpreter. [`prepare`] is the
//! single entry point, guarded by a `Once`, which sets the `PYTHONPATH`
//! trap **before** any initialization — so the proof of isolation
//! holds whatever the order in which the tests run.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Once;

use stokesforge_python::interpreter::{self, EmbeddedPython};

/// Trap value injected into `PYTHONPATH`: it must never appear
/// in `sys.path`.
const PYTHONPATH_TRAP: &str = "/tmp/trap-stokesforge-pythonpath";

/// Version lock, read at compile time: a single source of truth.
const LOCK_ENV: &str = include_str!("../../../vendor/python/lock.env");

static PREPARE: Once = Once::new();

/// Reads a key of the `vendor/python/lock.env` lock.
fn locked(key: &str) -> String {
    LOCK_ENV
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_else(|| panic!("key {key} missing from vendor/python/lock.env"))
}

/// Resolves the embedded prefix: `STOKESFORGE_PYTHON_HOME` if set by the test
/// script, otherwise `vendor/python/extracted/python` relative to the manifest.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

/// Sets the environment trap then starts the interpreter, only once.
fn prepare() -> EmbeddedPython {
    let embedded = EmbeddedPython::with_home(home());
    PREPARE.call_once(|| {
        // Set BEFORE initialization: it is the only moment when CPython
        // could read it. In isolated mode, it does not read it.
        std::env::set_var("PYTHONPATH", PYTHONPATH_TRAP);
        embedded
            .initialize()
            .expect("the embedded interpreter must start");
    });
    embedded
}

/// #332: membership of the prefix is judged by components — neither a neighbor
/// with an extended name, nor (on Windows) a difference of separator.
#[test]
fn contains_path_compares_by_components() {
    let embedded = EmbeddedPython::with_home(PathBuf::from("/opt/sf/python"));
    assert!(embedded.contains_path("/opt/sf/python"));
    assert!(embedded.contains_path("/opt/sf/python/lib/site-packages"));
    assert!(!embedded.contains_path("/opt/sf/python3/lib"));
    assert!(!embedded.contains_path("/usr/lib/python3.12"));

    #[cfg(windows)]
    {
        let embedded = EmbeddedPython::with_home(PathBuf::from("C:/dev/sf/python"));
        assert!(embedded.contains_path(r"C:\dev\sf\python\Lib\site-packages"));
        assert!(!embedded.contains_path(r"C:\dev\sf\python3\Lib"));
    }
}

#[test]
fn idempotent_initialization() {
    let embedded = prepare();
    assert!(embedded.initialize().is_ok(), "second call");
    assert!(embedded.initialize().is_ok(), "third call");
    assert!(EmbeddedPython::is_initialized());
}

#[test]
fn locate_refuses_a_missing_prefix() {
    let bogus = EmbeddedPython::with_home(PathBuf::from("/does/not/exist"));
    assert!(!bogus.home().join("lib").exists());
}

#[test]
fn python_version_is_locked() {
    prepare();
    let version = interpreter::python_version().expect("Python version");
    let expected = locked("PYTHON_VERSION");
    assert!(
        version.starts_with(&expected),
        "version {version}, expected {expected}"
    );
}

#[test]
fn numpy_version_is_locked() {
    prepare();
    let version = interpreter::numpy_version().expect("numpy version");
    assert_eq!(version, locked("NUMPY_VERSION"));
}

#[test]
fn one_way_sum() {
    prepare();
    let sum = interpreter::roundtrip_sum(&[1.0, 2.0, 3.0]).expect("roundtrip_sum");
    assert!((sum - 6.0).abs() < f64::EPSILON, "sum = {sum}");
}

#[test]
fn round_trip_doubles() {
    prepare();
    let output = interpreter::roundtrip_vec(&[1.0, 2.0]).expect("roundtrip_vec");
    assert_eq!(output, vec![2.0, 4.0]);
}

#[test]
fn empty_round_trip() {
    prepare();
    let output = interpreter::roundtrip_vec(&[]).expect("empty roundtrip_vec");
    assert!(output.is_empty());
}

#[test]
fn isolated_mode_is_active() {
    prepare();
    assert!(
        interpreter::sys_flags_isolated().expect("sys.flags.isolated"),
        "the interpreter must run in isolated mode"
    );
}

#[test]
fn sys_path_contains_only_the_embedded_prefix() {
    let embedded = prepare();
    let prefix = embedded
        .home()
        .canonicalize()
        .expect("the embedded prefix must exist");
    // Comparison of canonical paths, not strings: on Windows,
    // `canonicalize` returns the form `\\?\C:\…` whereas `sys.path` carries
    // `C:\…` (#332). An entry that cannot be found does not canonicalize and
    // therefore stays foreign.
    let in_the_prefix = |path: &str| {
        std::path::Path::new(path)
            .canonicalize()
            .is_ok_and(|c| c.starts_with(&prefix))
    };

    let paths = interpreter::sys_path().expect("sys.path");
    assert!(!paths.is_empty());
    for path in &paths {
        assert!(
            !path.starts_with("/usr"),
            "sys.path contains a system path: {path}"
        );
        assert!(
            !path.contains("/.local/"),
            "sys.path contains a user site: {path}"
        );
        assert!(
            !path.contains("site-packages") || in_the_prefix(path),
            "sys.path contains a foreign site-packages: {path}"
        );
    }
}

#[test]
fn pythonpath_is_ignored() {
    prepare();
    let paths = interpreter::sys_path().expect("sys.path");
    assert_eq!(
        std::env::var("PYTHONPATH").as_deref(),
        Ok(PYTHONPATH_TRAP),
        "the trap must really be set in the process environment"
    );
    assert!(
        !paths.iter().any(|c| c == PYTHONPATH_TRAP),
        "PYTHONPATH leaked into sys.path: {paths:?}"
    );
}

#[test]
fn sys_prefix_points_to_the_embedded_prefix() {
    let embedded = prepare();
    let prefix = interpreter::sys_prefix().expect("sys.prefix");
    let wanted = embedded.home().to_string_lossy().into_owned();
    assert_eq!(prefix, wanted);
}
