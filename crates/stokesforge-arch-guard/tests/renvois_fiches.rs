//! Les skills de projet (`.claude/skills/stokesforge-*` et `run-stokesforge`)
//! ne recopient pas les règles : ils y renvoient par « `fichier.md` § Titre »
//! vers `docs/` ou `.claude/conventions.md`. Ce garde-fou vérifie que chaque renvoi,
//! dans chaque `SKILL.md` et chaque `references/*.md`, désigne un fichier et un
//! titre qui existent encore. Il garde aussi les liens `](references/x.md)` de
//! chaque `SKILL.md` : ils doivent pointer un fichier existant, et aucune fiche
//! de `references/` ne doit rester sans lien (orpheline).
//!
//! Pourquoi : les titres de `docs/` changeront quand la documentation sera
//! traduite en anglais ; une section renommée ou supprimée doit casser le test
//! au lieu de laisser une fiche muette. Les skills tiers (apex, skill-manager,
//! audit-test…) sont ignorés. Les fiches n'ont plus de nombre fixe : seuls un
//! plancher de sanité (au moins 3 skills, plus de 30 renvois) signale un format
//! qui aurait changé.
//!
//! Forme lue : un chemin entre apostrophes inverses terminé par `.md`, puis
//! ` § `, puis le titre, jusqu'à ` ; ` ou la fin de la ligne. Le titre est
//! accepté s'il est contenu dans un titre du fichier, ou s'il commence par un
//! titre du fichier (le renvoi peut ajouter « , point `x` » ou « (paragraphe… ) »).
//! Un `§ ` sans chemin juste avant est refusé : il serait invérifiable.
//!
//! Les contrôles prennent une racine et rendent leurs erreurs ; les témoins
//! (arbres factices dans le dossier temporaire) prouvent qu'ils sonnent.

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

/// Skills de projet : `stokesforge-*` et `run-stokesforge`, triés.
fn skills_projet(racine: &Path) -> Vec<PathBuf> {
    let mut skills: Vec<PathBuf> = fs::read_dir(racine.join(".claude/skills"))
        .expect("dossier .claude/skills")
        .map(|e| e.expect("entrée").path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("stokesforge-") || n == "run-stokesforge")
        })
        .collect();
    skills.sort();
    skills
}

/// Fichiers `.md` à scanner d'un skill : `SKILL.md` et `references/*.md`.
fn fichiers_md(skill: &Path) -> Vec<PathBuf> {
    let mut fichiers = vec![skill.join("SKILL.md")];
    if let Ok(dir) = fs::read_dir(skill.join("references")) {
        let mut refs: Vec<PathBuf> = dir
            .map(|e| e.expect("entrée").path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        refs.sort();
        fichiers.extend(refs);
    }
    fichiers
}

/// Nombre de renvois lus et erreurs de tous les skills de projet.
fn lire_renvois(racine: &Path) -> (usize, Vec<String>) {
    let mut total = 0;
    let mut erreurs = Vec::new();
    for skill in skills_projet(racine) {
        for fichier in fichiers_md(&skill) {
            let texte = match fs::read_to_string(&fichier) {
                Ok(t) => t,
                Err(e) => {
                    erreurs.push(format!("{} : lecture impossible : {e}", fichier.display()));
                    continue;
                }
            };
            for ligne in texte.lines() {
                match renvois(ligne) {
                    Err(e) => erreurs.push(format!("{} : {e}", fichier.display())),
                    Ok(liste) => {
                        for (chemin, titre) in liste {
                            total += 1;
                            let cible = racine.join(&chemin);
                            if !cible.is_file() {
                                erreurs.push(format!(
                                    "{} : fichier absent {chemin}",
                                    fichier.display()
                                ));
                            } else if !titre_valide(&titre, &titres(&cible)) {
                                erreurs.push(format!(
                                    "{} : aucun titre « {titre} » dans {chemin}",
                                    fichier.display()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    (total, erreurs)
}

/// Erreurs des renvois « `chemin.md` § Titre » de tous les skills de projet.
fn erreurs_renvois(racine: &Path) -> Vec<String> {
    lire_renvois(racine).1
}

/// Erreurs des liens `](references/x)` : lien mort ou fiche orpheline.
fn erreurs_liens(racine: &Path) -> Vec<String> {
    let mut erreurs = Vec::new();
    for skill in skills_projet(racine) {
        let Ok(texte) = fs::read_to_string(skill.join("SKILL.md")) else {
            erreurs.push(format!("{} : SKILL.md illisible", skill.display()));
            continue;
        };
        for morceau in texte.split("](references/").skip(1) {
            let nom = morceau.split(')').next().unwrap_or("");
            if !skill.join("references").join(nom).is_file() {
                erreurs.push(format!(
                    "{} : fiche listée mais absente : {nom}",
                    skill.display()
                ));
            }
        }
        let Ok(dir) = fs::read_dir(skill.join("references")) else {
            continue;
        };
        for entree in dir {
            let entree = entree.expect("entrée");
            if !entree.path().is_file() {
                continue;
            }
            let nom = entree.file_name().to_string_lossy().to_string();
            if !texte.contains(&format!("](references/{nom})")) {
                erreurs.push(format!(
                    "{} : fiche non listée dans SKILL.md : {nom}",
                    skill.display()
                ));
            }
        }
    }
    erreurs
}

#[test]
fn les_renvois_des_skills_pointent_des_sections_existantes() {
    let racine = racine();
    let skills = skills_projet(&racine).len();
    assert!(skills >= 3, "skills de projet introuvables : {skills}");
    let (total, erreurs) = lire_renvois(&racine);
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
fn les_skills_listent_des_fiches_qui_existent_sans_orpheline() {
    let erreurs = erreurs_liens(&racine());
    assert!(
        erreurs.is_empty(),
        "liens de fiches cassés :\n{}",
        erreurs.join("\n")
    );
}

// ---- Témoins : arbres factices ----

/// Arbre factice dans le dossier temporaire, supprimé à la fin.
struct Arbre(PathBuf);

impl Arbre {
    fn nouveau(nom: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("stokesforge-renvois-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("création de l'arbre");
        Arbre(dir)
    }

    fn ecrire(&self, chemin: &str, contenu: &str) {
        let p = self.0.join(chemin);
        fs::create_dir_all(p.parent().expect("parent")).expect("dossiers");
        fs::write(p, contenu).expect("écriture");
    }

    /// Arbre correct : un doc, trois skills dont un lié à une fiche.
    fn correct(nom: &str) -> Self {
        let a = Self::nouveau(nom);
        a.ecrire("docs/a.md", "# Doc\n\n## Section réelle\n");
        a.ecrire(
            ".claude/skills/stokesforge-un/SKILL.md",
            "Voir `docs/a.md` § Section réelle\n[f](references/f.md)\n",
        );
        a.ecrire(
            ".claude/skills/stokesforge-un/references/f.md",
            "Voir `docs/a.md` § Section réelle ; `docs/a.md` § Doc\n",
        );
        a.ecrire(".claude/skills/stokesforge-deux/SKILL.md", "Rien.\n");
        a.ecrire(".claude/skills/run-stokesforge/SKILL.md", "Rien.\n");
        a
    }

    fn skill(&self) -> &'static str {
        ".claude/skills/stokesforge-un"
    }
}

impl Drop for Arbre {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn contient(erreurs: &[String], morceau: &str) -> bool {
    erreurs.iter().any(|e| e.contains(morceau))
}

#[test]
fn temoin_arbre_correct_sans_erreur() {
    let a = Arbre::correct("correct");
    assert_eq!(skills_projet(&a.0).len(), 3);
    let (total, erreurs) = lire_renvois(&a.0);
    assert_eq!(total, 3);
    assert!(erreurs.is_empty(), "{erreurs:?}");
    assert!(erreurs_liens(&a.0).is_empty());
}

#[test]
fn temoin_titre_absent_signale() {
    let a = Arbre::correct("titre");
    a.ecrire(
        &format!("{}/references/f.md", a.skill()),
        "Voir `docs/a.md` § Section disparue\n",
    );
    assert!(contient(
        &erreurs_renvois(&a.0),
        "aucun titre « Section disparue »"
    ));
}

#[test]
fn temoin_fichier_absent_signale() {
    let a = Arbre::correct("fichier");
    a.ecrire(
        &format!("{}/SKILL.md", a.skill()),
        "Voir `docs/inconnu.md` § Doc\n[f](references/f.md)\n",
    );
    assert!(contient(
        &erreurs_renvois(&a.0),
        "fichier absent docs/inconnu.md"
    ));
}

#[test]
fn temoin_paragraphe_sans_chemin_signale() {
    let a = Arbre::correct("sanschemin");
    a.ecrire(
        &format!("{}/references/f.md", a.skill()),
        "Voir la section § Doc\n",
    );
    assert!(contient(&erreurs_renvois(&a.0), "sans chemin .md"));
}

#[test]
fn temoin_lien_vers_fiche_absente_signale() {
    let a = Arbre::correct("lienmort");
    a.ecrire(
        &format!("{}/SKILL.md", a.skill()),
        "[f](references/f.md) [g](references/g.md)\n",
    );
    let erreurs = erreurs_liens(&a.0);
    assert!(
        contient(&erreurs, "fiche listée mais absente : g.md"),
        "{erreurs:?}"
    );
}

#[test]
fn temoin_fiche_orpheline_signalee() {
    let a = Arbre::correct("orpheline");
    a.ecrire(&format!("{}/references/orpheline.md", a.skill()), "Rien.\n");
    let erreurs = erreurs_liens(&a.0);
    assert!(
        contient(&erreurs, "fiche non listée dans SKILL.md : orpheline.md"),
        "{erreurs:?}"
    );
}

#[test]
fn temoin_skill_tiers_ignore() {
    let a = Arbre::correct("tiers");
    a.ecrire(
        ".claude/skills/apex/SKILL.md",
        "Voir `docs/inconnu.md` § Rien et § sans chemin\n[x](references/absent.md)\n",
    );
    a.ecrire(".claude/skills/apex/references/orpheline.md", "Rien.\n");
    assert!(erreurs_renvois(&a.0).is_empty());
    assert!(erreurs_liens(&a.0).is_empty());
    assert_eq!(skills_projet(&a.0).len(), 3);
}
