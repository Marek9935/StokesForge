//! `stokesforge-selftest` — rejoue, **depuis une distribution empaquetée**, le
//! critère de fin de tout le socle, et sort en 0 ou en 1.
//!
//! Coquille volontairement mince : toute la logique vit dans
//! [`stokesforge_script::run_selftest`], pour être aussi exercée par un test
//! `cargo` ordinaire (`crates/stokesforge-script/tests/selftest.rs`). Ce binaire
//! se contente de localiser le préfixe Python embarqué à côté de
//! lui-même — [`EmbeddedPython::locate`] — d'appeler `run_selftest`,
//! d'imprimer le compte rendu, et de traduire le verdict en code de sortie.
//!
//! `--quiet` n'affiche que les étapes en échec et le verdict final, utile en
//! intégration continue : quand tout va bien, seule la dernière ligne compte.
//!
//! Ce binaire n'existe que pour être lancé depuis `dist/stokesforge-app/` (à côté
//! de `stokesforge-app`, dans la même disposition `lib/python`) — voir
//! `scripts/app-dist.sh`, qui le construit, l'assemble puis le relance en fin
//! de script dans un environnement nettoyé et piégé.

use std::process::ExitCode;

use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_script::run_selftest;

fn main() -> ExitCode {
    let quiet = std::env::args().skip(1).any(|arg| arg == "--quiet");

    let python = match EmbeddedPython::locate() {
        Ok(python) => python,
        Err(error) => {
            eprintln!("stokesforge-selftest : préfixe Python embarqué introuvable : {error}");
            return ExitCode::FAILURE;
        }
    };

    let report = run_selftest(python);

    if quiet {
        for step in &report.steps {
            if !step.passed {
                println!("[ÉCHEC] {} — {}", step.name, step.detail);
            }
        }
    } else {
        println!("=== stokesforge-selftest : environnement ===");
        for (label, value) in &report.environment {
            println!("{label:<20}: {value}");
        }
        println!();
        println!("=== stokesforge-selftest : étapes du critère de fin ===");
        println!("{}", report.render());
        println!();
    }

    if report.passed() {
        println!("stokesforge-selftest : OK — critère de fin du socle vérifié depuis le paquet");
        ExitCode::SUCCESS
    } else {
        println!("stokesforge-selftest : ÉCHEC — le critère de fin du socle n'est pas vérifié");
        ExitCode::FAILURE
    }
}
