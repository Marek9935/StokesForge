//! Sonde de pile : exécutée en sous-processus, parce qu'un débordement de
//! pile n'est **pas** une panique. Le runtime Rust le détecte, écrit
//! `has overflowed its stack` et appelle `abort()` : ni `catch_unwind` ni
//! `JoinHandle::join` ne rendent la main, et le processus entier meurt —
//! y compris le binaire de test s'il y vivait.
//!
//! Usage : `plotyx-sonde-pile <profondeur> <geste> [octets_de_pile]`
//!
//! `geste` ∈ { `construire`, `annuler`, `vider`, `lacher` }. La pile vaut par
//! défaut 2 Mio, la taille qu'un `std::thread` reçoit quand personne ne la
//! fixe — c'est-à-dire celle du fil de l'acteur de `plotyx-session`, qui
//! appelle `thread::Builder::new().name(…).spawn(…)` sans `stack_size`.

use plotyx_commands::{Command, History};
use plotyx_core::{Column, Project, ValueBlock};

/// Pile d'un `std::thread` quand `stack_size` n'est pas fixé.
const PILE_PAR_DEFAUT: usize = 2 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let profondeur: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(64);
    let geste = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "annuler".to_string());
    let pile: usize = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(PILE_PAR_DEFAUT);

    println!("profondeur = {profondeur}, geste = {geste}, pile = {pile} octets");

    let fil = std::thread::Builder::new()
        .name("sonde-pile".to_string())
        .stack_size(pile)
        .spawn(move || travail(profondeur, &geste))
        .expect("fil créé");
    match fil.join() {
        Ok(()) => println!("FIN NORMALE"),
        Err(_) => {
            println!("le fil a paniqué");
            std::process::exit(3);
        }
    }
}

fn travail(profondeur: usize, geste: &str) {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let colonne = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("colonne");

    let mut history = History::with_limit(0);
    for i in 0..profondeur {
        // B7 (`guide-de-correction-robustesse.md` §F4) — `begin_macro`
        // refuse désormais (`MacroTooDeep`) au-delà d'une profondeur bornée,
        // avant même d'approcher la frontière où `revert`/`Drop` (récursifs
        // sur `Entry::Macro`) feraient déborder la pile. Une sonde lancée
        // avec une `profondeur` au-delà de cette borne doit donc voir ce
        // refus, pas un crash — c'est exactement la preuve que le correctif
        // fonctionne.
        if let Err(err) = history.begin_macro(&format!("m{i}")) {
            println!("  begin_macro refusé au niveau {i} : {err} — REFUS PROPRE, pas de crash");
            return;
        }
    }
    history
        .execute(
            &mut project,
            Command::SetCells {
                column: colonne,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
            },
        )
        .expect("écriture acceptée");
    for _ in 0..profondeur {
        history.end_macro(&mut project).expect("fermeture");
    }
    println!(
        "  fermeture : OK, {} entrée(s) de premier niveau",
        history.len()
    );

    match geste {
        // Construit l'imbrication puis l'oublie sans la détruire : montre
        // que la BÂTIR ne déborde jamais (tout y est itératif). C'est la
        // parcourir — ou la détruire — qui déborde.
        "construire" => {
            std::mem::forget(history);
            println!("  construction seule : OK");
        }
        // `History::revert` récurse sur `Entry::Macro`.
        "annuler" => {
            let fait = history.undo(&mut project).expect("annulation");
            println!("  annulation : {fait}");
        }
        // `clear()` vide les piles : le `Drop` dérivé de `Entry` récurse.
        // C'est ce que `plotyx-session` appelle à chaque `load`.
        "vider" => {
            history.clear().expect("vidage");
            println!("  vidage : OK");
        }
        // Lâcher l'historique : même `Drop` récursif.
        "lacher" => {
            drop(history);
            println!("  lâcher : OK");
        }
        autre => {
            eprintln!("geste inconnu : {autre}");
            std::process::exit(2);
        }
    }
}
