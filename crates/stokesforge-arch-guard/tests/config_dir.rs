//! Une seule règle « dossier de configuration de StokesForge » (#332).
//!
//! `stokesforge_import::config_dir` (variable `STOKESFORGE_CONFIG_DIR`, sinon
//! `directories::ProjectDirs`) est la seule porte vers le profil de
//! l'utilisateur. Un `ProjectDirs::from` ailleurs contourne la variable : les
//! tests, qui redirigent le dossier par elle, écriraient dans le vrai profil
//! (sous Windows `directories` ignore `XDG_CONFIG_HOME`). Cette garde lit
//! toutes les sources de `crates/` et refuse l'appel hors du module.
//!
//! Les lignes de commentaire sont ignorées (une doc peut nommer l'appel). Ne
//! voit pas un autre moyen de calculer un chemin de profil (`dirs`, `APPDATA`
//! lu à la main) : relecture.

use std::fs;
use std::path::{Path, PathBuf};

/// L'appel interdit hors du module (écrit en deux morceaux : ce fichier ne se
/// compte pas lui-même, en plus d'être exclu par son chemin).
const APPEL: &str = concat!("ProjectDirs", "::from");

/// Le module qui porte la règle, relatif à la racine du dépôt.
const MODULE: &str = "crates/stokesforge-import/src/config_dir.rs";

/// Cette garde, relative à la racine : elle cite l'appel dans ses témoins.
const GARDE: &str = "crates/stokesforge-arch-guard/tests/config_dir.rs";

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(dossier: &Path, sortie: &mut Vec<PathBuf>) {
    for entree in fs::read_dir(dossier).expect("lecture de dossier") {
        let chemin = entree.expect("entrée").path();
        if chemin.is_dir() {
            if chemin.file_name().is_some_and(|nom| nom != "target") {
                sources(&chemin, sortie);
            }
        } else if chemin.extension().is_some_and(|e| e == "rs") {
            sortie.push(chemin);
        }
    }
}

/// Les numéros des lignes de code (hors commentaire) qui font l'appel.
fn lignes_fautives(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, ligne)| !ligne.trim_start().starts_with("//"))
        .filter(|(_, ligne)| ligne.contains(APPEL))
        .map(|(n, _)| n + 1)
        .collect()
}

/// Parmi des sources `(chemin relatif à la racine, texte)`, les endroits où
/// l'appel est fait hors du module et de cette garde.
fn fautifs(sources: &[(String, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(chemin, _)| chemin != MODULE && chemin != GARDE)
        .flat_map(|(chemin, texte)| {
            lignes_fautives(texte)
                .into_iter()
                .map(move |n| format!("{chemin}:{n}"))
        })
        .collect()
}

#[test]
fn project_dirs_n_est_appele_que_par_le_module_config_dir() {
    let racine = racine();
    let mut fichiers = Vec::new();
    sources(&racine.join("crates"), &mut fichiers);
    let lues: Vec<(String, String)> = fichiers
        .iter()
        .map(|chemin| {
            let relatif = chemin
                .strip_prefix(&racine)
                .expect("sous la racine")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            (relatif, fs::read_to_string(chemin).expect("source lisible"))
        })
        .collect();
    assert!(
        lues.len() > 100,
        "trop peu de sources lues : {}",
        lues.len()
    );

    let module = lues
        .iter()
        .find(|(chemin, _)| chemin == MODULE)
        .expect("le module config_dir existe");
    assert_eq!(
        lignes_fautives(&module.1).len(),
        1,
        "le module fait l'appel, une seule fois"
    );
    let fautifs = fautifs(&lues);
    assert!(
        fautifs.is_empty(),
        "ProjectDirs::from hors de {MODULE} : passer par \
         `stokesforge_import::config_dir::config_dir()` (#332)\n{}",
        fautifs.join("\n")
    );
}

#[test]
fn la_garde_refuse_un_appel_hors_du_module_et_ignore_les_commentaires() {
    let appel = format!("let d = directories::{APPEL}(\"\", \"\", \"X\");");
    let factice = vec![
        (
            "crates/stokesforge-app-state/src/fausse.rs".to_string(),
            format!("fn f() {{\n    {appel}\n}}\n"),
        ),
        (MODULE.to_string(), format!("fn permis() {{ {appel} }}\n")),
        (
            "crates/stokesforge-app/src/doc.rs".to_string(),
            format!("/// {appel}\n// {appel}\nfn rien() {{}}\n"),
        ),
    ];
    assert_eq!(
        fautifs(&factice),
        ["crates/stokesforge-app-state/src/fausse.rs:2"],
        "seule la source factice hors module, en code, est refusée"
    );
}
