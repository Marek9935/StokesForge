//! Les fiches `.claude/skills/stokesforge-feature/references/*.md` ne portent
//! aucune règle (F1, #301) : seulement des renvois « `fichier.md` § Titre » vers
//! `docs/` ou `.claude/conventions.md`. Ce garde-fou vérifie que chaque renvoi
//! désigne un fichier et un titre qui existent encore, pour qu'une section
//! renommée ou supprimée casse le test au lieu de laisser une fiche muette.
//!
//! Forme lue : un chemin entre apostrophes inverses terminé par `.md`, puis
//! ` § `, puis le titre, jusqu'à ` ; ` ou la fin de la ligne. Le titre est
//! accepté s'il est contenu dans un titre du fichier, ou s'il commence par un
//! titre du fichier (le renvoi peut ajouter « , point `x` » ou « (paragraphe… ) »).
//! Un `§ ` sans chemin juste avant est refusé : il serait invérifiable.
//! Le `SKILL.md` et la fiche listée doivent aussi exister.

use std::fs;
use std::path::{Path, PathBuf};

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn titres(fichier: &Path) -> Vec<String> {
    fs::read_to_string(fichier)
        .unwrap_or_else(|e| panic!("lecture de {} : {e}", fichier.display()))
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .collect()
}

/// Renvois `(chemin, titre)` d'une ligne ; `Err` pour un `§ ` sans chemin.
fn renvois(ligne: &str) -> Result<Vec<(String, String)>, String> {
    let mut trouves = Vec::new();
    let mut reste = ligne;
    while let Some(i) = reste.find(" § ") {
        let avant = &reste[..i];
        let chemin = avant
            .strip_suffix("`")
            .and_then(|a| a.rfind('`').map(|j| &a[j + 1..]))
            .filter(|c| c.ends_with(".md"))
            .ok_or_else(|| format!("« § » sans chemin .md devant : {ligne}"))?;
        let apres = &reste[i + " § ".len()..];
        let fin = apres.find(" ; ").unwrap_or(apres.len());
        let titre = apres[..fin].trim().trim_end_matches('.').trim();
        trouves.push((chemin.to_string(), titre.to_string()));
        reste = &apres[fin..];
    }
    Ok(trouves)
}

fn titre_valide(titre: &str, dans: &[String]) -> bool {
    dans.iter().any(|t| {
        t.contains(titre)
            || (titre.starts_with(t.as_str())
                && titre[t.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric()))
    })
}

#[test]
fn les_renvois_des_fiches_pointent_des_sections_existantes() {
    let dossier = racine().join(".claude/skills/stokesforge-feature/references");
    let mut fiches: Vec<PathBuf> = fs::read_dir(&dossier)
        .expect("dossier references/")
        .map(|e| e.expect("entrée").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    fiches.sort();
    assert!(fiches.len() >= 10, "fiches introuvables : {fiches:?}");

    let mut total = 0;
    let mut erreurs = Vec::new();
    for fiche in &fiches {
        let texte = fs::read_to_string(fiche).expect("lecture de la fiche");
        for ligne in texte.lines() {
            match renvois(ligne) {
                Err(e) => erreurs.push(format!("{} : {e}", fiche.display())),
                Ok(liste) => {
                    for (chemin, titre) in liste {
                        total += 1;
                        let cible = racine().join(&chemin);
                        if !cible.is_file() {
                            erreurs.push(format!("{} : fichier absent {chemin}", fiche.display()));
                        } else if !titre_valide(&titre, &titres(&cible)) {
                            erreurs.push(format!(
                                "{} : aucun titre « {titre} » dans {chemin}",
                                fiche.display()
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        total > 30,
        "trop peu de renvois lus ({total}) : format changé ?"
    );
    assert!(
        erreurs.is_empty(),
        "renvois cassés :\n{}",
        erreurs.join("\n")
    );
}

#[test]
fn le_skill_liste_des_fiches_qui_existent() {
    let skill = racine().join(".claude/skills/stokesforge-feature");
    let texte = fs::read_to_string(skill.join("SKILL.md")).expect("SKILL.md");
    let mut listees = 0;
    for morceau in texte.split("](references/").skip(1) {
        let nom = morceau.split(')').next().expect("lien");
        assert!(
            skill.join("references").join(nom).is_file(),
            "fiche listée mais absente : {nom}"
        );
        listees += 1;
    }
    assert!(listees >= 11, "fiches listées : {listees}");
    for entree in fs::read_dir(skill.join("references")).expect("references/") {
        let nom = entree
            .expect("entrée")
            .file_name()
            .to_string_lossy()
            .to_string();
        assert!(
            texte.contains(&format!("](references/{nom})")),
            "fiche non listée dans SKILL.md : {nom}"
        );
    }
}
