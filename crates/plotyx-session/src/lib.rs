//! Acteur qui possède le `Project` de PlotYx et le sert à ses clients (l'UI,
//! un futur thread de script Python) par un canal de requêtes.
//!
//! # Rôle
//!
//! Ce crate rend concrète une décision d'architecture du socle
//! (`docs/rustrewrite/roadmap.md` §6) : **le `Project` vit sur son propre
//! thread**. L'UI et un futur thread de script sont deux clients
//! symétriques : ils envoient des requêtes et reçoivent des instantanés
//! immuables ([`Snapshot`]). L'acteur détient le **seul** `&mut Project` du
//! processus, ce qui transforme la règle « aucune mutation hors d'une
//! commande » en propriété de propriété — vérifiable par le compilateur —
//! plutôt qu'en convention à surveiller.
//!
//! # Protocole
//!
//! Un canal de requêtes **FIFO** (`std::sync::mpsc`, ordre déterministe) ;
//! chaque requête reçoit **une** réponse, sur un canal à usage unique créé
//! par le client pour cette requête. L'UI garde le dernier [`Snapshot`] reçu
//! et peint depuis lui sans verrou ; un client sait si son instantané est
//! périmé en comparant la `revision` (compteur monotone), jamais en
//! comparant les arbres entre eux.
//!
//! Deux requêtes passent par `plotyx-persistence` (frontière figée, format
//! provisoire — voir la doc de ce crate) : [`SessionHandle::save`] écrit le
//! projet courant sans toucher à la révision ni à l'historique, et
//! [`SessionHandle::load`] **remplace** le projet courant par un projet
//! neuf chargé depuis le disque et **réinitialise l'historique**
//! (`undo`/`redo` redeviennent vides) — un ancien instantané n'a alors plus
//! aucun rapport avec l'arbre courant, y compris ses identifiants de nœud,
//! qui repartent de la valeur persistée dans le fichier. `load` est refusé
//! (`Err(SessionError::Command(CommandError::MacroOpen(_)))`) si une macro
//! est ouverte ; dans ce cas, comme pour tout échec de `save`/`load`, ni le
//! projet ni l'historique ne sont modifiés.
//!
//! # Deux invariants tenus par construction
//!
//! 1. **L'acteur ne touche jamais à Python.** Ce crate ne dépend d'aucun
//!    crate Python — il n'en existe pas encore, mais la garantie porte sur
//!    l'avenir : une liste blanche de dépendances (`tests/no_ui_dependency.rs`)
//!    force une décision consciente à toute nouvelle dépendance, plutôt que
//!    de compter sur une relecture.
//! 2. **Aucun objet non-`Send` ne traverse le canal.** Requêtes et réponses
//!    sont du Rust pur, `Send + 'static` — vérifié par un test de
//!    compilation statique (`assert_send`, dans ce module et dans
//!    `actor`).
//!
//! # Limite connue
//!
//! Une macro est **globale à la session** : le protocole ne porte pas
//! l'identité de l'émetteur d'une requête. Si un client ouvre une macro
//! (`begin_macro`), les commandes envoyées par d'autres clients avant sa
//! fermeture (`end_macro`/`abort_macro`) y sont absorbées comme si elles en
//! faisaient partie — l'acteur ne peut pas distinguer « ma macro » de
//! « une macro ouverte par quelqu'un d'autre ». Ajouter cette identité au
//! protocole (et refuser les commandes d'un émetteur qui n'a pas ouvert la
//! macro en cours) reste possible plus tard ; ce n'est pas fait ici.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod actor;
mod error;
mod snapshot;

use std::path::Path;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use plotyx_commands::Command;
use plotyx_core::Project;

use actor::Request;

pub use error::{SessionError, SessionResult};
pub use snapshot::{HistoryState, Revision, Snapshot};

/// Poignée cliente vers une [`Session`] : clonable, envoyable à d'autres
/// threads. Chaque méthode envoie une requête sur le canal FIFO de l'acteur
/// et **bloque** jusqu'à la réponse. Rend `Err(SessionError::Disconnected)`
/// si l'acteur est terminé (arrêté via [`Session::shutdown`], ou en
/// panique).
#[derive(Debug, Clone)]
pub struct SessionHandle {
    tx: mpsc::Sender<Request>,
}

impl SessionHandle {
    /// Envoie une requête construite par `build` (qui reçoit l'émetteur du
    /// canal de réponse à usage unique de cet appel) et bloque jusqu'à la
    /// réponse.
    fn request<T>(&self, build: impl FnOnce(mpsc::Sender<T>) -> Request) -> SessionResult<T> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(build(reply_tx))
            .map_err(|_| SessionError::Disconnected)?;
        reply_rx.recv().map_err(|_| SessionError::Disconnected)
    }

    /// Exécute une commande. Rend la nouvelle révision, ou l'erreur du bus de
    /// commandes — l'arbre et l'historique restent alors inchangés.
    pub fn execute(&self, command: Command) -> SessionResult<Revision> {
        self.request(|reply| Request::Execute { command, reply })?
            .map_err(SessionError::from)
    }

    /// Annule l'entrée du sommet. `Ok(None)` si la pile est vide.
    pub fn undo(&self) -> SessionResult<Option<Revision>> {
        self.request(|reply| Request::Undo { reply })?
            .map_err(SessionError::from)
    }

    /// Rétablit l'entrée annulée la plus récente. `Ok(None)` si rien à
    /// rétablir.
    pub fn redo(&self) -> SessionResult<Option<Revision>> {
        self.request(|reply| Request::Redo { reply })?
            .map_err(SessionError::from)
    }

    /// Ouvre une macro portant `label` (voir la limite documentée en tête de
    /// crate : une macro est globale à la session).
    pub fn begin_macro(&self, label: &str) -> SessionResult<Revision> {
        let label = label.to_string();
        self.request(|reply| Request::BeginMacro { label, reply })?
            .map_err(SessionError::from)
    }

    /// Ferme la macro courante en une seule entrée d'historique.
    /// `Err(Command(NotInMacro))` hors macro.
    pub fn end_macro(&self) -> SessionResult<Revision> {
        self.request(|reply| Request::EndMacro { reply })?
            .map_err(SessionError::from)
    }

    /// Annule la macro courante et tout ce qu'elle avait appliqué.
    /// `Err(Command(NotInMacro))` hors macro.
    pub fn abort_macro(&self) -> SessionResult<Revision> {
        self.request(|reply| Request::AbortMacro { reply })?
            .map_err(SessionError::from)
    }

    /// Demande un instantané de l'état courant. Bon marché : ne clone pas
    /// l'arbre.
    pub fn snapshot(&self) -> SessionResult<Snapshot> {
        self.request(|reply| Request::Snapshot { reply })
    }

    /// Sauvegarde le projet courant vers `path`. Ne change pas la révision :
    /// sauvegarder ne modifie pas le modèle.
    pub fn save(&self, path: &Path) -> SessionResult<()> {
        self.request(|reply| Request::Save {
            path: path.to_path_buf(),
            reply,
        })?
    }

    /// Charge un projet complet depuis `path` et le substitue au projet
    /// courant : l'historique est vidé, la révision avance. `Err(Command(
    /// MacroOpen))` si une macro est ouverte — dans ce cas, comme pour tout
    /// échec, ni le projet ni l'historique ne sont touchés.
    pub fn load(&self, path: &Path) -> SessionResult<Revision> {
        self.request(|reply| Request::Load {
            path: path.to_path_buf(),
            reply,
        })?
    }
}

/// La session : le thread de l'acteur et une poignée vers lui. Construire une
/// `Session` démarre le thread immédiatement.
#[derive(Debug)]
pub struct Session {
    handle: SessionHandle,
    thread: JoinHandle<Project>,
}

impl Session {
    /// Démarre l'acteur avec `project` et la limite d'historique donnée
    /// (`None` → [`plotyx_commands::DEFAULT_LIMIT`]). `Err` si le système
    /// d'exploitation refuse de créer le thread.
    pub fn spawn(project: Project, history_limit: Option<usize>) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("plotyx-session".to_string())
            .spawn(move || actor::run(project, history_limit, rx))?;
        Ok(Self {
            handle: SessionHandle { tx },
            thread,
        })
    }

    /// Clone de la poignée cliente vers cette session.
    pub fn handle(&self) -> SessionHandle {
        self.handle.clone()
    }

    /// Demande l'arrêt de l'acteur et attend la fin de son thread ; rend le
    /// `Project` final. Les autres poignées reçoivent ensuite
    /// `Err(Disconnected)` à leur prochain appel. `Err(Disconnected)` ici si
    /// le thread de l'acteur a paniqué.
    ///
    /// Lâcher une `Session` (et toutes les poignées qui en descendent) sans
    /// appeler `shutdown` termine aussi le thread : la boucle de l'acteur
    /// s'arrête dès que le dernier émetteur du canal de requêtes est
    /// abandonné (`for request in rx`, `tests/session.rs`).
    pub fn shutdown(self) -> SessionResult<Project> {
        // Échec d'envoi ignoré : l'acteur peut déjà être terminé si toutes
        // les autres poignées ont aussi été lâchées entre-temps.
        let _ = self.handle.tx.send(Request::Shutdown);
        self.thread.join().map_err(|_| SessionError::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N'existe que pour que le compilateur vérifie l'invariant `Send` — ce
    /// n'est pas un test comportemental.
    fn assert_send<T: Send>() {}

    #[test]
    fn public_types_are_send() {
        assert_send::<SessionHandle>();
        assert_send::<Snapshot>();
        assert_send::<Command>();
    }
}
