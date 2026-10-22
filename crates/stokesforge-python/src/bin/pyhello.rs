//! `stokesforge-pyhello` — preuve d'empaquetage de la pile Python embarquée.
//!
//! Démarre l'interpréteur vendorisé, importe numpy, fait un aller simple puis
//! un aller-retour de tableau, et mesure les trois temps qui décident de
//! l'architecture : démarrage, envoi de 1 M `f64`, rapatriement de 1 M `f64`.
//!
//! Ce binaire n'existe que pour être lancé depuis `dist/pyhello/` dans un
//! environnement nettoyé (cf. `scripts/pyhello-dist.sh`) : sa sortie est la
//! preuve que l'isolation tient.

use std::process::ExitCode;
use std::time::Instant;

use stokesforge_python::interpreter::{self, EmbeddedPython, PythonError};

/// Taille des tableaux de mesure : assez grand pour que la copie soit mesurable.
const BENCH_LEN: usize = 1_000_000;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("stokesforge-pyhello : {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), PythonError> {
    let embedded = EmbeddedPython::locate()?;
    println!("préfixe embarqué      : {}", embedded.home().display());

    // (a) démarrage de l'interpréteur + import numpy.
    let start = Instant::now();
    embedded.initialize()?;
    let init_ms = start.elapsed().as_secs_f64() * 1e3;

    println!("version Python        : {}", interpreter::python_version()?);
    println!("version numpy         : {}", interpreter::numpy_version()?);
    println!(
        "sys.flags.isolated    : {}",
        interpreter::sys_flags_isolated()?
    );
    println!("sys.prefix            : {}", interpreter::sys_prefix()?);

    let path = interpreter::sys_path()?;
    println!("sys.path              : {} entrées", path.len());
    for (index, entry) in path.iter().take(3).enumerate() {
        println!("sys.path[{index}]           : {entry}");
    }

    // Auto-contrôle : la distribution se déclare cassée si une seule entrée de
    // `sys.path` sort du préfixe embarqué. C'est ce test, exécuté depuis un
    // environnement piégé, qui fait la preuve d'isolation.
    if let Some(intrus) = path.iter().find(|entry| !embedded.contains_path(entry)) {
        return Err(PythonError::InitFailed(format!(
            "sys.path contient une entrée hors du préfixe embarqué : {intrus}"
        )));
    }

    // Aller simple : copie Rust → numpy puis somme côté Python.
    let values: Vec<f64> = (0..BENCH_LEN).map(|index| index as f64).collect();
    let sum = interpreter::roundtrip_sum(&values)?;
    println!("roundtrip_sum(1M)     : {sum}");

    // Aller-retour complet : `arr * 2.0` côté Python puis rapatriement.
    let doubled = interpreter::roundtrip_vec(&values)?;
    println!(
        "roundtrip_vec(1M)     : {} valeurs, dernière = {:?}",
        doubled.len(),
        doubled.last()
    );
    println!(
        "roundtrip_vec(10)     : {:?}",
        interpreter::roundtrip_vec(&values[..10])?
    );

    // (b) et (c) : les deux moitiés de la copie, mesurées séparément.
    let timings = interpreter::measure_copy(&values)?;

    println!("--- mesures (ms) ---");
    println!("(a) init : interpréteur + import numpy : {init_ms:.1}");
    println!(
        "(b) aller  1M f64 (Rust → numpy, copie)  : {:.1}",
        timings.send_ms
    );
    println!(
        "(c) retour 1M f64 (numpy → Vec<f64>)     : {:.1}",
        timings.recv_ms
    );

    Ok(())
}
