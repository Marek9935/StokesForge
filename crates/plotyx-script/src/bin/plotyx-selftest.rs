//! `plotyx-selftest` — rejoue, **depuis une distribution empaquetée**, le
//! critère de fin de tout le socle, et sort en 0 ou en 1.
//!
//! Coquille volontairement mince : toute la logique vit dans
//! [`plotyx_script::run_selftest`], pour être aussi exercée par un test
//! `cargo` ordinaire (`crates/plotyx-script/tests/selftest.rs`). Ce binaire
//! se contente de localiser le préfixe Python embarqué à côté de
//! lui-même — [`EmbeddedPython::locate`] — d'appeler `run_selftest`,
//! d'imprimer le compte rendu, et de traduire le verdict en code de sortie.
//!
//! `--quiet` n'affiche que les étapes en échec et le verdict final, utile en
//! intégration continue : quand tout va bien, seule la dernière ligne compte.
//!
//! Ce binaire n'existe que pour être lancé depuis `dist/plotyx-app/` (à côté
//! de `plotyx-app`, dans la même disposition `lib/python`) — voir
//! `scripts/app-dist.sh`, qui le construit, l'assemble puis le relance en fin
//! de script dans un environnement nettoyé et piégé.

use std::process::ExitCode;

use plotyx_python::interpreter::EmbeddedPython;
use plotyx_script::run_selftest;

fn main() -> ExitCode {
    let quiet = std::env::args().skip(1).any(|arg| arg == "--quiet");

    let python = match EmbeddedPython::locate() {
        Ok(python) => python,
        Err(error) => {
            eprintln!("plotyx-selftest : préfixe Python embarqué introuvable : {error}");
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
        println!("=== plotyx-selftest : environnement ===");
        for (label, value) in &report.environment {
            println!("{label:<20}: {value}");
        }
        println!();
        println!("=== plotyx-selftest : étapes du critère de fin ===");
        println!("{}", report.render());
        println!();
    }

    if report.passed() {
        println!("plotyx-selftest : OK — critère de fin du socle vérifié depuis le paquet");
        ExitCode::SUCCESS
    } else {
        println!("plotyx-selftest : ÉCHEC — le critère de fin du socle n'est pas vérifié");
        ExitCode::FAILURE
    }
}
