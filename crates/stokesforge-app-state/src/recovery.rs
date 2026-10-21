//! Copies de secours : mort de l'acteur (#310) et sauvegarde automatique
//! périodique (#317), dans `recovery/` du dossier de données de l'application.
//!
//! Tout y est de la logique **pure** (dossier en paramètre), testable sans
//! fenêtre ni horloge ; `state/autosave.rs` n'y ajoute que la décision.
//!
//! # Mort de l'acteur (#310)
//!
//! Un panic du fil de l'acteur détruit le `Project` : il ne reste que le
//! dernier instantané que l'interface a relu (`Arc<Tree>` immuable, déjà
//! partagé). On le réécrit dans un fichier **neuf** — jamais sur le `.stf` de
//! l'utilisateur, que la copie peut précéder d'une commande (un panic en plein
//! recalcul laisse l'état d'avant).
//!
//! `Project::from_tree(.., 0)` reprend les identifiants au plus haut vivant :
//! ceux des nœuds supprimés peuvent revenir, sans effet puisque l'historique
//! est perdu avec l'acteur.
//!
//! # Sauvegarde automatique (#317)
//!
//! Un **emplacement par processus** : `<nom|Untitled>-<pid>.stf`, écrasé à
//! chaque échéance (l'écriture est atomique, temporaire puis renommage, côté
//! persistance), avec une **fiche** `.json` (chemin d'origine, heure) et un
//! verrou `.lock` tenu (`File::try_lock`) tant que le processus vit. Le
//! verrou distingue, au démarrage suivant, la copie d'un autre StokesForge
//! encore ouvert (qu'on ne propose pas) de celle d'un processus mort : le
//! système le libère à la fin du processus, plantage compris. Une copie n'est
//! proposée que si elle est **plus récente** que le `.stf` d'origine ; sans
//! fiche ou sans `.stf`, elle l'est toujours (une fiche illisible dégrade,
//! invariant 10, elle ne bloque jamais la récupération).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use stokesforge_core::Project;
use stokesforge_persistence::PersistenceError;
use stokesforge_session::Snapshot;

/// Pourquoi la copie de secours n'a pas pu être écrite.
#[derive(Debug)]
pub enum RecoveryError {
    /// Le dossier de secours n'a pas pu être créé.
    CreateDir(std::io::Error),
    /// L'écriture du fichier a échoué.
    Save(PersistenceError),
}

/// Dossier des copies de secours : `recovery/` dans le dossier de données de
/// l'application. `None` sous `cfg(test)` ou la feature `test-support` : un
/// test n'écrit jamais dans les vraies données de la machine (comme les
/// préférences, #250).
pub(crate) fn default_dir() -> Option<PathBuf> {
    if cfg!(any(test, feature = "test-support")) {
        return None;
    }
    directories::ProjectDirs::from("", "", "StokesForge")
        .map(|dirs| dirs.data_dir().join("recovery"))
}

/// Le nom de copie d'un projet : son nom de fichier sans extension, `Untitled`
/// quand il n'a pas de chemin (#317).
pub fn stem_of(project_path: &str) -> String {
    Path::new(project_path).file_stem().map_or_else(
        || "Untitled".to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Écrit `snapshot` dans un fichier neuf de `dir` et rend son chemin. Le nom
/// reprend celui du projet (`project_path`) suivi de l'heure : deux plantages
/// ne s'écrasent pas, et `save` ne rencontre jamais un fichier existant. La
/// fiche est écrite aussi : cette copie se propose au démarrage suivant comme
/// celles de la sauvegarde automatique (#317).
pub fn write_copy(
    dir: &Path,
    project_path: &str,
    snapshot: &Snapshot,
) -> Result<PathBuf, RecoveryError> {
    std::fs::create_dir_all(dir).map_err(RecoveryError::CreateDir)?;
    let stem = stem_of(project_path);
    let seconds = now_seconds();
    let mut path = dir.join(format!("{stem}-{seconds}.stf"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem}-{seconds}-{n}.stf"));
        n += 1;
    }
    let project = Project::from_tree(snapshot.tree.clone(), 0);
    stokesforge_persistence::save_with_geometries(&project, &snapshot.window_geometries, &path)
        .map_err(RecoveryError::Save)?;
    write_sidecar(&path, project_path);
    Ok(path)
}

/// La fiche d'une copie : d'où elle vient et quand elle a été écrite.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Sidecar {
    /// Le chemin du projet au moment de l'écriture ; vide : jamais enregistré.
    origin: String,
    /// Secondes depuis l'époque Unix.
    at: u64,
}

fn sidecar_path(stf: &Path) -> PathBuf {
    stf.with_extension("json")
}

fn lock_path(stf: &Path) -> PathBuf {
    stf.with_extension("lock")
}

/// Écrit la fiche de `stf`. Un échec est ignoré : une copie sans fiche reste
/// proposée (sans chemin d'origine ni comparaison de date).
fn write_sidecar(stf: &Path, origin: &str) {
    let sidecar = Sidecar {
        origin: origin.to_string(),
        at: now_seconds(),
    };
    if let Ok(text) = serde_json::to_string(&sidecar) {
        let _ = std::fs::write(sidecar_path(stf), text);
    }
}

/// L'emplacement de sauvegarde automatique **de ce processus** : le verrou
/// tenu, et le chemin de la copie.
#[derive(Debug)]
pub struct Slot {
    stem: String,
    stf: PathBuf,
    // Tenu pour sa seule existence : le verrou tombe quand le fichier se ferme.
    lock: File,
}

impl Slot {
    /// Prend l'emplacement `<stem>-<pid>` de `dir` (créé au besoin) et pose son
    /// verrou. Un verrou déjà tenu (un autre processus, un pid réutilisé) est
    /// une erreur.
    pub fn acquire(dir: &Path, stem: &str, pid: u32) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let stf = dir.join(format!("{stem}-{pid}.stf"));
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path(&stf))?;
        lock.try_lock().map_err(|err| match err {
            std::fs::TryLockError::Error(io) => io,
            std::fs::TryLockError::WouldBlock => std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the recovery slot is held by another process",
            ),
        })?;
        // Une copie restée d'un processus mort de même pid (pid réutilisé) ne
        // doit pas être écrasée : elle est écartée sous un autre nom, et se
        // proposera comme les autres.
        if stf.exists() {
            let aside = (1..)
                .map(|n| dir.join(format!("{stem}-{pid}-{n}.stf")))
                .find(|path| !path.exists() && !sidecar_path(path).exists());
            if let Some(aside) = aside {
                let _ = std::fs::rename(&stf, &aside);
                let _ = std::fs::rename(sidecar_path(&stf), sidecar_path(&aside));
            }
        }
        Ok(Self {
            stem: stem.to_string(),
            stf,
            lock,
        })
    }

    /// Le nom de projet sous lequel l'emplacement est pris.
    pub fn stem(&self) -> &str {
        &self.stem
    }

    /// Le fichier de la copie.
    pub fn path(&self) -> &Path {
        &self.stf
    }

    /// Écrit la fiche, **avant** la copie : une copie existe toujours avec sa
    /// fiche.
    pub fn write_sidecar(&self, origin: &str) {
        write_sidecar(&self.stf, origin);
    }

    /// Supprime la copie, sa fiche et le verrou (fermeture propre,
    /// enregistrement). Le verrou est lâché avant d'effacer son fichier
    /// (Windows).
    pub fn remove(self) {
        let Self { stf, lock, .. } = self;
        drop(lock);
        remove_files(&stf);
    }
}

fn remove_files(stf: &Path) {
    let _ = std::fs::remove_file(stf);
    let _ = std::fs::remove_file(sidecar_path(stf));
    let _ = std::fs::remove_file(lock_path(stf));
}

/// Une copie de secours proposée au démarrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Le fichier de la copie.
    pub stf: PathBuf,
    /// Le chemin du projet au moment de la copie ; `None` : jamais enregistré,
    /// ou fiche perdue.
    pub origin: Option<PathBuf>,
    /// Le nom à montrer : celui du fichier d'origine ; `None` : `Untitled`.
    pub name: Option<String>,
    /// Secondes Unix de l'écriture de la copie.
    pub saved_at: u64,
}

impl Found {
    /// Supprime la copie, sa fiche et son verrou orphelin.
    pub fn delete(&self) {
        remove_files(&self.stf);
    }
}

/// Le nom d'une copie sans fiche : celui du fichier sans son suffixe
/// numérique (`-pid` ou `-secondes`), `None` s'il ne reste rien.
fn name_from_copy(stf: &Path) -> Option<String> {
    let stem = stf.file_stem()?.to_string_lossy().into_owned();
    let trimmed = match stem.rsplit_once('-') {
        Some((head, tail))
            if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            head.to_string()
        }
        _ => stem,
    };
    (trimmed != "Untitled").then(|| format!("{trimmed}.stf"))
}

/// Une copie est-elle tenue par un processus vivant ?
fn is_held(stf: &Path) -> bool {
    let Ok(lock) = File::options().write(true).open(lock_path(stf)) else {
        return false;
    };
    matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Les copies à proposer dans `dir`, la plus récente d'abord : celles dont le
/// processus a disparu et qui sont plus récentes que leur `.stf`. Dossier
/// absent ou illisible : aucune.
pub fn scan(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "stf"))
        .filter(|stf| !is_held(stf))
        .filter_map(|stf| {
            let modified = std::fs::metadata(&stf).and_then(|m| m.modified()).ok()?;
            let sidecar: Option<Sidecar> = std::fs::read_to_string(sidecar_path(&stf))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
            let origin = sidecar
                .as_ref()
                .filter(|s| !s.origin.is_empty())
                .map(|s| PathBuf::from(&s.origin));
            // Pas plus récente que le `.stf` : il a été enregistré depuis.
            let origin_modified = origin
                .as_ref()
                .and_then(|o| std::fs::metadata(o).and_then(|m| m.modified()).ok());
            if origin_modified.is_some_and(|o| modified <= o) {
                return None;
            }
            let name = match &sidecar {
                Some(_) => origin
                    .as_ref()
                    .and_then(|o| o.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
                None => name_from_copy(&stf),
            };
            let saved_at = sidecar.as_ref().map_or_else(
                || {
                    modified
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs())
                },
                |s| s.at,
            );
            Some(Found {
                stf,
                origin,
                name,
                saved_at,
            })
        })
        .collect();
    found.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.stf.cmp(&b.stf)));
    found
}

/// `AAAA-MM-JJ HH:MM` (UTC) d'un instant en secondes Unix : l'application n'a
/// pas de bibliothèque de dates, et l'heure locale ne lui est pas connue.
pub fn format_utc(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Jours depuis 1970-01-01 -> date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    /// Un `.stf` quelconque (contenu sans importance pour `scan`).
    fn touch(path: &Path, age_seconds: u64) {
        std::fs::write(path, b"x").unwrap();
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age_seconds))
            .unwrap();
    }

    fn copy_with_sidecar(dir: &Path, name: &str, origin: &str, age: u64) -> PathBuf {
        let stf = dir.join(name);
        touch(&stf, age);
        write_sidecar(&stf, origin);
        stf
    }

    #[test]
    fn la_date_civile_est_juste() {
        assert_eq!(format_utc(0), "1970-01-01 00:00");
        assert_eq!(
            format_utc(951_782_400 + 3600 * 13 + 60 * 5),
            "2000-02-29 13:05"
        );
        assert_eq!(format_utc(1_792_598_400), "2026-10-21 16:00");
    }

    #[test]
    fn le_nom_d_une_copie_sans_chemin_est_untitled() {
        assert_eq!(stem_of(""), "Untitled");
        assert_eq!(stem_of("/a/b/mesures.stf"), "mesures");
    }

    #[test]
    fn plus_ancienne_que_le_stf_exclue_plus_recente_listee() {
        let dir = tempfile::tempdir().unwrap();
        let rec = dir.path().join("recovery");
        std::fs::create_dir_all(&rec).unwrap();
        let user = dir.path().join("travail.stf");
        touch(&user, 100);
        // Copie écrite il y a 200 s : plus ancienne que le .stf (100 s).
        copy_with_sidecar(&rec, "ancienne-1.stf", user.to_str().unwrap(), 200);
        // Copie écrite il y a 10 s : plus récente.
        let recente = copy_with_sidecar(&rec, "recente-2.stf", user.to_str().unwrap(), 10);
        let found = scan(&rec);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].stf, recente);
        assert_eq!(found[0].origin.as_deref(), Some(user.as_path()));
        assert_eq!(found[0].name.as_deref(), Some("travail.stf"));
    }

    #[test]
    fn sans_stf_ou_sans_fiche_la_copie_est_listee() {
        let dir = tempfile::tempdir().unwrap();
        let rec = dir.path();
        // Origine disparue du disque.
        copy_with_sidecar(rec, "perdu-1.stf", "/nulle/part/perdu.stf", 5);
        // Jamais enregistré : origine vide.
        copy_with_sidecar(rec, "Untitled-2.stf", "", 5);
        // Sans fiche (copie de #310 d'avant la fiche) : nom tiré du fichier.
        touch(&rec.join("ancien-1700000000.stf"), 5);
        // Fiche illisible : dégradée comme une absence (invariant 10).
        touch(&rec.join("abime-3.stf"), 5);
        std::fs::write(rec.join("abime-3.json"), "{pas du json").unwrap();
        let mut names: Vec<_> = scan(rec).into_iter().map(|f| f.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                None,
                Some("abime.stf".to_string()),
                Some("ancien.stf".to_string()),
                Some("perdu.stf".to_string()),
            ]
        );
    }

    #[test]
    fn une_copie_de_secours_de_310_est_listee_avec_sa_fiche() {
        let dir = tempfile::tempdir().unwrap();
        let session =
            stokesforge_session::Session::spawn(Project::new("Demo").unwrap(), None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let path = write_copy(dir.path(), "/x/travail.stf", &snapshot).unwrap();
        assert!(sidecar_path(&path).exists());
        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stf, path);
        assert_eq!(found[0].name.as_deref(), Some("travail.stf"));
        session.shutdown().unwrap();
    }

    #[test]
    fn un_verrou_tenu_exclut_la_copie_et_sa_chute_la_libere() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Slot::acquire(dir.path(), "projet", 4242).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        assert!(scan(dir.path()).is_empty(), "tenue par un processus vivant");
        assert!(
            Slot::acquire(dir.path(), "projet", 4242).is_err(),
            "un emplacement ne se prend pas deux fois"
        );
        let path = slot.path().to_path_buf();
        drop(slot);
        let found = scan(dir.path());
        assert_eq!(found.len(), 1, "le verrou lâché, la copie se propose");
        assert_eq!(found[0].stf, path);
    }

    #[test]
    fn remove_et_delete_effacent_copie_fiche_et_verrou() {
        let dir = tempfile::tempdir().unwrap();
        let slot = Slot::acquire(dir.path(), "a", 1).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        slot.remove();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let slot = Slot::acquire(dir.path(), "b", 2).unwrap();
        slot.write_sidecar("");
        touch(slot.path(), 5);
        drop(slot);
        let found = scan(dir.path());
        assert_eq!(found.len(), 1);
        found[0].delete();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
