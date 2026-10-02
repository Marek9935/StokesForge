//! Sonde d'allocation : exécutée en sous-processus par la batterie 5, parce
//! qu'un échec d'allocation Rust est un `abort()`, pas une panique rattrapable.
//!
//! Usage : sonde_alloc <cas> [argument]

use stokesforge_core::{Column, Project, ValueBlock};

fn projet() -> (Project, stokesforge_core::NodeId, stokesforge_core::NodeId) {
    let mut p = Project::new("S").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let col = p
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    (p, sheet, col)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cas = args.get(1).map(String::as_str).unwrap_or("");
    let n: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_usize << 45);

    match cas {
        // `SetCells { start: n }` sur une colonne de 3 lignes.
        "set_values_start" => {
            let (mut p, _sheet, col) = projet();
            println!("avant : longueur = {}", longueur(&p, col));
            let r = p.set_values(col, n, ValueBlock::Float(vec![7.0]));
            match r {
                Ok(delta) => println!(
                    "ACCEPTÉ : longueur = {}, delta.previous_length = {}",
                    longueur(&p, col),
                    delta.previous_length
                ),
                Err(e) => println!("refusé : {e}"),
            }
        }
        // `set_length(n)`
        "set_length" => {
            let (mut p, _sheet, col) = projet();
            let r = p.set_length(col, n);
            match r {
                Ok(_) => println!("ACCEPTÉ : longueur = {}", longueur(&p, col)),
                Err(e) => println!("refusé : {e}"),
            }
        }
        // `insert_rows(sheet, 0, n)`
        "insert_rows" => {
            let (mut p, sheet, col) = projet();
            let r = p.insert_rows(sheet, 0, n);
            match r {
                Ok(v) => println!(
                    "ACCEPTÉ : {} colonne(s) touchée(s), longueur = {}",
                    v.len(),
                    longueur(&p, col)
                ),
                Err(e) => println!("refusé : {e}"),
            }
        }
        // Par le bus de commandes, avec historique : mesure aussi le delta.
        "commande_setcells" => {
            use stokesforge_commands::{Command, History};
            let (mut p, _sheet, col) = projet();
            let mut h = History::new();
            let r = h.execute(
                &mut p,
                Command::SetCells {
                    column: col,
                    start: n,
                    values: ValueBlock::Float(vec![7.0]),
                    expected_version: None,
                },
            );
            match r {
                Ok(()) => {
                    println!("ACCEPTÉ : longueur = {}", longueur(&p, col));
                    println!("octets retenus par l'historique = {}", h.retained_bytes());
                    let ok = h.undo(&mut p).unwrap();
                    println!("undo = {ok}, longueur après = {}", longueur(&p, col));
                }
                Err(e) => println!("refusé : {e}"),
            }
        }
        // `histogram(bins)` : `bins` vient du fichier sans plafond.
        "histogram_bins" => {
            use std::num::NonZeroU32;
            let bins = NonZeroU32::new(n.min(u32::MAX as usize) as u32).unwrap_or(NonZeroU32::MIN);
            println!("bins = {bins}");
            let (edges, counts) = stokesforge_core::resolve::histogram(&[1.0, 2.0, 3.0], bins);
            println!("ACCEPTÉ : {} bornes, {} classes", edges.len(), counts.len());
        }
        autre => {
            eprintln!("cas inconnu : {autre}");
            std::process::exit(2);
        }
    }
    println!("FIN NORMALE");
}

fn longueur(p: &Project, col: stokesforge_core::NodeId) -> usize {
    p.tree().node(col).unwrap().as_column().unwrap().len()
}
