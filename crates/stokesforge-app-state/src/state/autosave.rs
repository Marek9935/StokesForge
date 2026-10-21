//! Sauvegarde automatique et récupération au démarrage (#317).
//!
//! Toutes les `autosave_minutes` (5 par défaut, 1 à 60), si le projet a changé
//! depuis le dernier repère (démarrage, enregistrement, chargement, copie
//! précédente), `AppState` demande à l'acteur d'écrire le projet dans
//! l'**emplacement** de ce processus (`recovery::Slot`, un seul fichier écrasé).
//! C'est `handle.save` : il ne crée aucune entrée d'historique, ne change ni
//! `saved_revision`, ni le chemin, ni le titre « modifié », et n'écrit jamais
//! le `.stf` de l'utilisateur (invariant 1 : rien du modèle ne bouge).
//!
//! L'horloge est **injectée** (`poll_autosave(now)`) : l'application l'appelle
//! une fois par image depuis `logic()` et programme son réveil sur le délai
//! rendu ; un test avance `now` sans attendre. Une échéance tombée pendant une
//! macro, un script, un import ou un recalcul est repoussée de quelques
//! secondes, sans erreur : `handle.save` refuse pendant une macro, et un
//! recalcul fait attendre l'acteur.
//!
//! Un échec d'écriture (dossier impossible) dit **un seul** avertissement tant
//! qu'il dure ; la copie suivante, réussie, le rearme.
//!
//! Au démarrage, `scan_recovery` (appelé par le binaire racine seulement)
//! dresse la liste des copies de processus disparus, plus récentes que leur
//! `.stf` ; la fenêtre propose Open, Ignore et Delete (avec confirmation, la
//! suppression ne s'annule pas). Open charge la copie dans la fenêtre courante,
//! derrière « Enregistrer ? » s'il y a du travail, sous le **chemin d'origine**
//! (vide pour Untitled) et comme travail non enregistré ; la copie ouverte est
//! alors recopiée dans l'emplacement de ce processus, puis effacée. Ignore la
//! laisse sur disque : elle revient au démarrage suivant.
//!
//! L'emplacement disparaît à l'enregistrement, à la fermeture propre, et pour
//! New, Open d'un autre projet et Discard (qui sont des fermetures propres de
//! l'ancien projet). Session perdue : la copie est gardée.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::AppState;
use crate::errors;
use crate::recovery::{self, Found, Slot};
use crate::tr;

/// Délai de nouvel essai quand une échéance tombe pendant un travail en cours.
pub const AUTOSAVE_RETRY: Duration = Duration::from_secs(5);

/// L'état de la sauvegarde automatique et de la liste de récupération.
#[derive(Debug, Default)]
pub(super) struct AutosaveState {
    /// Prochaine échéance ; `None` : à armer au prochain `poll_autosave`.
    deadline: Option<Instant>,
    /// Les révisions déjà couvertes (enregistrées, chargées ou copiées) ;
    /// `None` : le projet est à copier quoi qu'il en soit.
    mark: Option<(u64, u64)>,
    /// L'emplacement de ce processus, une fois pris.
    slot: Option<Slot>,
    /// La dernière écriture a échoué (l'avertissement est déjà dit).
    failed: bool,
    /// Les copies proposées au démarrage, tant que la fenêtre est ouverte.
    found: Vec<Found>,
    /// La copie dont la suppression attend confirmation.
    pending_delete: Option<PathBuf>,
}

impl AutosaveState {
    pub(super) fn at(mark: (u64, u64)) -> Self {
        Self {
            mark: Some(mark),
            ..Self::default()
        }
    }
}

/// Une ligne de la fenêtre de récupération.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRow {
    /// La copie ; c'est la clé des actions Open, Ignore et Delete.
    pub path: PathBuf,
    /// « nom · date », prêt à peindre.
    pub text: String,
}

enum AutosaveError {
    Dir(std::io::Error),
    Session(stokesforge_session::SessionError),
}

impl AppState {
    /// La sauvegarde automatique est-elle active (préférence) ?
    pub fn autosave_enabled(&self) -> bool {
        self.object_manager_preferences.autosave()
    }

    /// Sa période, en minutes.
    pub fn autosave_minutes(&self) -> u32 {
        self.object_manager_preferences.autosave_minutes()
    }

    /// Voir [`super::Action::SetAutosave`].
    pub(super) fn set_autosave(&mut self, on: bool) {
        self.autosave.deadline = None;
        if let Err(error) = self.object_manager_preferences.save_autosave(on) {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = errors::io::error_message(&error)
            ));
        }
    }

    /// Voir [`super::Action::SetAutosaveMinutes`] : la nouvelle période compte
    /// à partir de maintenant.
    pub(super) fn set_autosave_minutes(&mut self, minutes: u32) {
        self.autosave.deadline = None;
        if let Err(error) = self
            .object_manager_preferences
            .save_autosave_minutes(minutes)
        {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = errors::io::error_message(&error)
            ));
        }
    }

    /// Les révisions du projet tel qu'il est maintenant.
    fn autosave_revisions(&self) -> (u64, u64) {
        (
            self.snapshot.revision.max(self.applied_revision),
            self.snapshot.geometry_revision,
        )
    }

    /// Le projet vient d'être enregistré, chargé ou remplacé : il n'y a plus
    /// rien à copier avant le prochain changement.
    pub(super) fn autosave_mark_clean(&mut self) {
        self.autosave.mark = Some(self.autosave_revisions());
    }

    /// Supprime la copie de ce processus : le projet est enregistré, ou
    /// refermé proprement.
    pub(super) fn autosave_remove_slot(&mut self) {
        if let Some(slot) = self.autosave.slot.take() {
            slot.remove();
        }
    }

    /// À appeler à chaque image (`logic()` de la fenêtre racine) avec l'heure
    /// courante. Écrit la copie si l'échéance est tombée et que le projet a
    /// changé ; rend le délai jusqu'à la prochaine échéance (pour programmer le
    /// réveil), `None` si la sauvegarde est coupée.
    pub fn poll_autosave(&mut self, now: Instant) -> Option<Duration> {
        if !self.autosave_enabled() || self.session_lost {
            self.autosave.deadline = None;
            return None;
        }
        let period = Duration::from_secs(u64::from(self.autosave_minutes()) * 60);
        let deadline = *self.autosave.deadline.get_or_insert(now + period);
        if now < deadline {
            return Some(deadline - now);
        }
        let busy = self.snapshot.history.in_macro
            || self.script_running()
            || self.import_job.is_some()
            || self.recalc_since.is_some();
        if busy {
            self.autosave.deadline = Some(now + AUTOSAVE_RETRY);
            return Some(AUTOSAVE_RETRY);
        }
        self.autosave.deadline = Some(now + period);
        if self.autosave.mark != Some(self.autosave_revisions()) {
            match self.write_autosave() {
                Ok(()) => self.autosave.failed = false,
                Err(error) => {
                    if !self.autosave.failed {
                        let cause = match &error {
                            AutosaveError::Dir(io) => errors::io::error_message(io),
                            AutosaveError::Session(err) => errors::session::error_message(err),
                        };
                        self.warn(tr!("status-autosave-failed", cause = cause));
                    }
                    self.autosave.failed = true;
                }
            }
        }
        Some(period)
    }

    /// Écrit le projet dans l'emplacement de ce processus (changé de nom si le
    /// projet a changé de chemin). Sans dossier de secours (tests) : rien.
    fn write_autosave(&mut self) -> Result<(), AutosaveError> {
        let Some(dir) = self.recovery_dir.clone() else {
            return Ok(());
        };
        let stem = recovery::stem_of(&self.path);
        if self
            .autosave
            .slot
            .as_ref()
            .is_some_and(|slot| slot.stem() != stem)
        {
            self.autosave_remove_slot();
        }
        let slot = match self.autosave.slot.take() {
            Some(slot) => slot,
            None => Slot::acquire(&dir, &stem, std::process::id()).map_err(AutosaveError::Dir)?,
        };
        slot.write_sidecar(&self.path);
        let result = self.handle.save(slot.path());
        self.autosave.slot = Some(slot);
        result.map_err(AutosaveError::Session)?;
        self.autosave_mark_clean();
        Ok(())
    }

    /// Dresse la liste des copies à proposer (dossier de secours connu).
    /// Appelé au démarrage par le binaire racine.
    pub fn scan_recovery(&mut self) {
        if let Some(dir) = &self.recovery_dir {
            self.autosave.found = recovery::scan(dir);
        }
    }

    /// Les lignes de la fenêtre de récupération ; `None` : rien à proposer,
    /// la fenêtre est fermée.
    pub fn recovery_rows(&self) -> Option<Vec<RecoveryRow>> {
        if self.autosave.found.is_empty() {
            return None;
        }
        Some(
            self.autosave
                .found
                .iter()
                .map(|found| RecoveryRow {
                    path: found.stf.clone(),
                    text: Self::recovery_text(found),
                })
                .collect(),
        )
    }

    fn recovery_text(found: &Found) -> String {
        tr!(
            "recovery-row",
            name = found
                .name
                .clone()
                .unwrap_or_else(|| tr!("recovery-untitled")),
            date = recovery::format_utc(found.saved_at)
        )
    }

    /// Le nom de la copie dont la suppression attend confirmation.
    pub fn pending_recovery_delete(&self) -> Option<String> {
        let path = self.autosave.pending_delete.as_ref()?;
        let found = self.autosave.found.iter().find(|f| &f.stf == path)?;
        Some(
            found
                .name
                .clone()
                .unwrap_or_else(|| tr!("recovery-untitled")),
        )
    }

    fn recovery_found(&self, path: &PathBuf) -> Option<&Found> {
        self.autosave.found.iter().find(|f| &f.stf == path)
    }

    /// Voir [`super::Action::RecoveryOpen`] : le même chemin que File › Open…,
    /// donc la même question « Enregistrer ? » s'il y a du travail.
    pub(super) fn recovery_open(&mut self, path: PathBuf) {
        if self.recovery_found(&path).is_some() {
            self.request_unsaved(super::UnsavedGesture::OpenRecovery(path), false);
        }
    }

    /// Ouvre la copie `path` dans la fenêtre courante (réponse à la question
    /// « Enregistrer ? » faite, ou rien à perdre).
    pub(super) fn open_recovery(&mut self, path: PathBuf) {
        let Some(found) = self.recovery_found(&path).cloned() else {
            return;
        };
        let origin = found
            .origin
            .as_ref()
            .map(|o| o.display().to_string())
            .unwrap_or_default();
        let previous = std::mem::replace(&mut self.path, found.stf.display().to_string());
        if !self.load() {
            self.path = previous;
            return;
        }
        // Le projet est celui de la copie, sous le chemin d'où elle vient : un
        // travail non enregistré (le `.stf` d'origine ne l'a pas).
        self.path = origin;
        self.saved_revision = None;
        self.discarded_source = None;
        self.autosave.mark = None;
        // Recopié chez nous avant d'effacer la copie ouverte : un plantage
        // juste après ne perd rien.
        if self.write_autosave().is_ok() && self.recovery_dir.is_some() {
            // Les deux ne sont jamais le même fichier (acquire écarte l'ancien).
            found.delete();
            self.autosave.found.retain(|f| f.stf != found.stf);
        }
        let text = tr!(
            "status-recovery-opened",
            name = found.name.unwrap_or_else(|| tr!("recovery-untitled"))
        );
        self.notice(text);
    }

    /// Voir [`super::Action::RecoveryIgnore`] : la copie reste sur disque.
    pub(super) fn recovery_ignore(&mut self, path: &PathBuf) {
        self.autosave.found.retain(|f| &f.stf != path);
    }

    /// Voir [`super::Action::RecoveryDelete`] : demande confirmation.
    pub(super) fn recovery_request_delete(&mut self, path: PathBuf) {
        if self.recovery_found(&path).is_some() {
            self.autosave.pending_delete = Some(path);
        }
    }

    /// Voir [`super::Action::ConfirmRecoveryDelete`].
    pub(super) fn recovery_confirm_delete(&mut self, accept: bool) {
        let Some(path) = self.autosave.pending_delete.take() else {
            return;
        };
        if !accept {
            return;
        }
        if let Some(found) = self.recovery_found(&path) {
            found.delete();
        }
        self.autosave.found.retain(|f| f.stf != path);
    }

    /// Voir [`super::Action::CloseRecovery`] : chaque copie est ignorée.
    pub(super) fn recovery_close(&mut self) {
        self.autosave.found.clear();
        self.autosave.pending_delete = None;
    }

    /// Une confirmation de suppression attend-elle ? (dialogue bloquant)
    pub(super) fn recovery_delete_pending(&self) -> bool {
        self.autosave.pending_delete.is_some()
    }
}
