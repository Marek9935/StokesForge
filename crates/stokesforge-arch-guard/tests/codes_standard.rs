//! Les codes `S-XN` du standard d'interface (`docs/standard-interface.md`,
//! « Index des règles ») sont cités partout : commentaires, docs, skills,
//! messages de tests. Le préfixe `S-` a été posé par script (#301, F3/F4) ;
//! rien ne vérifiait les noms. Ce garde-fou lit l'index, puis refuse tout
//! `S-XN` cité dans le dépôt qui n'y figure pas (faute de frappe, règle
//! supprimée, code d'une autre famille préfixé à tort).
//!
//! Ne détecte pas un code *existant* mais mal choisi, ni une citation
//! restée sans préfixe : ces deux cas restent à la relecture.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Un code `S-` + une lettre majuscule + des chiffres, au début de `s`.
fn code_en_tete(s: &str) -> Option<&str> {
    let reste = s.strip_prefix("S-")?;
    if !reste.as_bytes().first()?.is_ascii_uppercase() {
        return None;
    }
    let chiffres = reste.as_bytes()[1..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    (chiffres > 0).then(|| &s[..3 + chiffres])
}

/// Les codes cités dans `texte`, hors mots plus longs (`ABS-C1`).
fn codes_cites(texte: &str) -> Vec<&str> {
    let octets = texte.as_bytes();
    texte
        .match_indices("S-")
        .filter(|(i, _)| {
            *i == 0 || !(octets[i - 1].is_ascii_alphanumeric() || octets[i - 1] == b'_')
        })
        .filter_map(|(i, _)| code_en_tete(&texte[i..]))
        .collect()
}

/// Les codes de l'index : lignes de tableau `| S-XN | … |`.
fn index(standard: &str) -> BTreeSet<String> {
    standard
        .lines()
        .filter_map(|l| l.strip_prefix("| "))
        .filter_map(code_en_tete)
        .map(str::to_string)
        .collect()
}

fn inconnus<'a>(texte: &'a str, connus: &BTreeSet<String>) -> Vec<&'a str> {
    codes_cites(texte)
        .into_iter()
        .filter(|c| !connus.contains(*c))
        .collect()
}

fn fichiers(dossier: &Path, sortie: &mut Vec<PathBuf>) {
    for entree in fs::read_dir(dossier).expect("lecture de dossier") {
        let chemin = entree.expect("entrée").path();
        let nom = chemin.file_name().unwrap().to_string_lossy().to_string();
        if chemin.is_dir() {
            if !matches!(
                nom.as_str(),
                "target" | ".git" | "archive" | "worktrees" | "vendor"
            ) {
                fichiers(&chemin, sortie);
            }
        } else if matches!(
            chemin.extension().and_then(|e| e.to_str()),
            Some("md" | "rs" | "ftl" | "html" | "toml" | "py" | "sh" | "yml")
        ) {
            sortie.push(chemin);
        }
    }
}

#[test]
fn les_codes_s_cites_existent_dans_l_index_du_standard() {
    let racine = racine();
    let standard = fs::read_to_string(racine.join("docs/standard-interface.md")).expect("standard");
    let connus = index(&standard);
    assert!(connus.len() >= 89, "index mal lu : {} codes", connus.len());
    assert!(connus.contains("S-N1") && connus.contains("S-R16"));

    let mut liste = vec![racine.join("CLAUDE.md")];
    for dossier in ["docs", ".claude", "crates"] {
        fichiers(&racine.join(dossier), &mut liste);
    }
    let (mut citations, mut erreurs) = (0, Vec::new());
    for fichier in &liste {
        let Ok(texte) = fs::read_to_string(fichier) else {
            continue;
        };
        citations += codes_cites(&texte).len();
        for (n, ligne) in texte.lines().enumerate() {
            for code in inconnus(ligne, &connus) {
                erreurs.push(format!("{}:{} : {code}", fichier.display(), n + 1));
            }
        }
    }
    assert!(citations > 300, "trop peu de citations lues : {citations}");
    assert!(
        erreurs.is_empty(),
        "codes absents de l'index du standard :\n{}",
        erreurs.join("\n")
    );
}

#[test]
fn le_garde_voit_un_code_inconnu_et_ignore_les_mots_plus_longs() {
    let connus: BTreeSet<String> = ["S-N1".to_string()].into();
    assert_eq!(inconnus("règle S-N1 et S-N2.", &connus), ["S-N2"]);
    let faux = format!("S-{}99", 'Z');
    assert_eq!(
        inconnus(&format!("{faux}, (S-N1)"), &connus),
        [faux.as_str()]
    );
    assert!(inconnus("ABS-N7 et S-n3 et S-12", &connus).is_empty());
}
