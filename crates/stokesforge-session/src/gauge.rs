//! Jauge du recalcul interruptible (#239) : le seul état partagé entre
//! l'acteur et ses clients qui ne passe **pas** par le canal FIFO.
//!
//! Pourquoi hors du canal : pendant un recalcul lourd l'acteur ne lit plus ses
//! requêtes, donc un « Stop » envoyé par le canal attendrait la fin du
//! recalcul qu'il veut interrompre, et un instantané demandé pour peindre
//! l'indicateur bloquerait l'interface. La jauge se lit et s'écrit sans
//! l'acteur : un mutex pour l'état (avec une condition pour attendre la
//! fin), un atomique pour le Stop, lu par le recalcul entre deux colonnes.
//!
//! Un recalcul porte un numéro : un Stop tardif — demandé juste après la fin
//! du recalcul qu'il visait — ne peut pas arrêter le suivant.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Comment s'est terminé un recalcul interruptible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecalcEnd {
    /// Mené à son terme.
    Done,
    /// Interrompu par Stop : la commande qui l'avait déclenché a été annulée
    /// (comme par Ctrl+Z) et ce qui avait changé a été recalculé.
    Stopped,
}

#[derive(Debug, Default)]
struct State {
    /// Numéro du prochain recalcul (0 est réservé : « aucun »).
    last: u64,
    /// Le recalcul en cours.
    running: Option<u64>,
    /// La fin du dernier recalcul, tant que personne ne l'a relevée.
    finished: Option<RecalcEnd>,
}

/// Voir la doc du module.
#[derive(Debug, Default)]
pub(crate) struct RecalcGauge {
    state: Mutex<State>,
    idle: Condvar,
    stop: AtomicU64,
}

impl RecalcGauge {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ouvre un recalcul et rend son numéro.
    pub(crate) fn begin(&self) -> u64 {
        let mut state = self.lock();
        state.last += 1;
        state.running = Some(state.last);
        state.finished = None;
        state.last
    }

    /// Clôt le recalcul `id` et réveille ceux qui attendent.
    pub(crate) fn finish(&self, id: u64, end: RecalcEnd) {
        let mut state = self.lock();
        if state.running == Some(id) {
            state.running = None;
            state.finished = Some(end);
        }
        self.idle.notify_all();
    }

    pub(crate) fn running(&self) -> Option<u64> {
        self.lock().running
    }

    /// Demande l'arrêt du recalcul `id` ; faux s'il n'est plus en cours.
    pub(crate) fn request_stop(&self, id: u64) -> bool {
        let state = self.lock();
        if state.running == Some(id) {
            self.stop.store(id, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Lu par le recalcul entre deux colonnes.
    pub(crate) fn stop_requested(&self, id: u64) -> bool {
        self.stop.load(Ordering::Acquire) == id
    }

    /// Attend la fin du recalcul en cours, au plus `timeout` ; vrai s'il n'y
    /// en a (plus) aucun.
    pub(crate) fn wait_idle(&self, timeout: Duration) -> bool {
        let state = self.lock();
        let (state, _) = self
            .idle
            .wait_timeout_while(state, timeout, |state| state.running.is_some())
            .unwrap_or_else(PoisonError::into_inner);
        state.running.is_none()
    }

    /// Relève la fin du dernier recalcul (une seule fois).
    pub(crate) fn take_finished(&self) -> Option<RecalcEnd> {
        self.lock().finished.take()
    }
}

#[cfg(test)]
impl RecalcGauge {
    /// Pose l'arrêt sur le prochain recalcul : son premier point d'arrêt le
    /// voit, ce qui rend déterministe un test qui, sinon, devrait battre le
    /// recalcul de vitesse.
    pub(crate) fn stop_next_for_test(&self) {
        let next = self.lock().last + 1;
        self.stop.store(next, Ordering::Release);
    }
}
