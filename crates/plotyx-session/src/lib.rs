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
//! # Ce qu'une macro ouverte fait des huit requêtes, et ce que change l'identité de l'émetteur
//!
//! F6 (`guide-de-correction-robustesse.md` §2, §6) : tout ce qu'un client
//! sait du projet est une copie potentiellement périmée, donc toute
//! hypothèse doit voyager dans la requête et être vérifiée par l'acteur au
//! moment de l'appliquer — jamais avant. Cette table referme la question que
//! le palliatif d'A11 (vague 1 de la campagne) avait seulement mise en
//! évidence sans y répondre : *lesquelles des huit requêtes une macro
//! ouverte doit-elle suspendre, et pour qui ?*
//!
//! Chaque [`SessionHandle`] porte désormais une identité stable (`ClientId`,
//! privé au crate — voir sa doc) : deux `.clone()` d'une même poignée restent le même
//! émetteur (un script qui clone sa poignée pour un thread reste « lui-même »)
//! ; deux poignées issues de deux appels **indépendants** à [`Session::handle`]
//! sont deux émetteurs distincts, même dans le même processus.
//!
//! | Requête | Pendant une macro ouverte, aujourd'hui | Ce que l'identité de l'émetteur y change |
//! |---|---|---|
//! | `Execute` | absorbée dans la macro courante, quel que soit l'émetteur (A4 : c'était le bug — le travail d'un client s'infiltrait dans la macro d'un autre) | **refusée** (`CommandError::ForeignMacro("execute")`) si l'émetteur n'est pas le propriétaire de la macro ; absorbée normalement sinon |
//! | `Undo` | refusée pour **quiconque** (`CommandError::MacroOpen("undo")`) : annuler pendant une macro, la sienne ou celle d'un autre, laisserait un état incohérent | inchangé — ce refus ne dépend jamais de qui a ouvert la macro |
//! | `Redo` | refusée pour quiconque (`CommandError::MacroOpen("redo")`), même raison qu'`Undo` | inchangé |
//! | `BeginMacro` | ouvre un niveau d'imbrication supplémentaire, quel que soit l'émetteur, jusqu'à [`plotyx_commands::MAX_MACRO_DEPTH`] | **refusée** (`ForeignMacro("begin_macro")`) si une macro est déjà ouverte par un *autre* émetteur ; un même émetteur peut toujours imbriquer la sienne |
//! | `EndMacro` | ferme le niveau courant en une entrée d'historique, quel que soit l'émetteur | **refusée** (`ForeignMacro("end_macro")`) si l'émetteur n'est pas le propriétaire — `CommandError::NotInMacro` reste réservé au cas où **aucune** macro n'est ouverte |
//! | `AbortMacro` | annule le niveau courant et tout son contenu, quel que soit l'émetteur — **y compris le travail déjà validé d'un autre client, silencieusement** : c'était exactement A4 | **refusée** (`ForeignMacro("abort_macro")`) si l'émetteur n'est pas le propriétaire : un `abort_macro` ne peut plus défaire le travail de quelqu'un d'autre |
//! | `Save` | refusée pour quiconque pendant une macro ouverte (A11, **palliatif déclaré** en vague 1 : la garde de `load_project` recopiée sans plus de nuance) | inchangé — le refus ne dépend pas de qui a ouvert la macro ; écrire sur le disque l'état intermédiaire d'une macro que *quelqu'un* pourrait encore avorter reste interdit à tous |
//! | `Load` | refusée pour quiconque pendant une macro ouverte, avant même de toucher au disque | inchangé, même raison que `Save` |
//!
//! `Snapshot` et `Shutdown` ne mutent rien : `Snapshot` est une lecture pure,
//! toujours servie ; `Shutdown` ne passe pas par ce tableau mais par le
//! constat qui suit.
//!
//! **`Session::shutdown` pendant une macro ouverte — constat non
//! reproductible, corrigé.** Une macro ouverte à l'arrêt de l'acteur n'avait
//! aucune garde : `Request::Shutdown` rendait directement le `Project`
//! courant, macro comprise — ses mutations, déjà appliquées au modèle en
//! mémoire (une macro n'empêche jamais `Project` d'être muté, elle ne fait
//! que différer l'entrée d'historique), se retrouvaient **commises en
//! silence**, sans jamais être devenues une entrée annulable et sans que
//! `end_macro` ait été appelé. Même absence de garde qu'A11, mais à la
//! sortie plutôt qu'à la sauvegarde — et sans reproduction possible (le banc
//! ne peut pas observer un `Project` qui n'existe plus après l'arrêt de
//! l'acteur, voir `guide-de-correction-robustesse.md` §7.3). La correction
//! retenue, triviale une fois A4 en place : `actor::run` annule (plutôt que
//! de commettre) toute macro encore ouverte à la réception de `Shutdown`,
//! niveau par niveau (`History::abort_macro`, sans distinction de
//! propriétaire — il n'y a plus de client à qui répondre) — une macro
//! ouverte à l'arrêt est désormais **annulée, pas commise**.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod actor;
mod error;
mod snapshot;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use plotyx_commands::Command;
use plotyx_core::Project;

use actor::Request;

pub use error::{SessionError, SessionResult};
pub use snapshot::{HistoryState, Revision, Snapshot};

/// Identité stable et comparable d'un émetteur de requêtes (A4,
/// `guide-de-correction-robustesse.md` §F6).
///
/// Un compteur atomique global, incrémenté à chaque appel **indépendant** à
/// [`Session::handle`] — jamais à `Clone` : deux `.clone()` d'une même
/// poignée partagent la même identité (un script qui clone sa poignée pour
/// la passer à un thread reste « le même émetteur » aux yeux de l'acteur),
/// alors que deux poignées issues de deux appels distincts à `handle()` sont
/// deux émetteurs différents, même s'ils vivent dans le même processus.
/// C'est cette identité qui permet à l'acteur de refuser qu'une macro
/// ouverte par l'un absorbe ou soit close par l'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ClientId(u64);

impl ClientId {
    /// Attribue une identité neuve, jamais réutilisée sur la vie du
    /// processus (`AtomicU64`, `Ordering::Relaxed` : seule l'unicité compte,
    /// pas un ordre entre threads).
    fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        ClientId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Poignée cliente vers une [`Session`] : clonable, envoyable à d'autres
/// threads. Chaque méthode envoie une requête sur le canal FIFO de l'acteur
/// et **bloque** jusqu'à la réponse. Rend `Err(SessionError::Disconnected)`
/// si l'acteur est terminé (arrêté via [`Session::shutdown`], ou en
/// panique).
#[derive(Debug, Clone)]
pub struct SessionHandle {
    tx: mpsc::Sender<Request>,
    /// Identité de cet émetteur (A4) : partagée par tous les `.clone()` de
    /// cette poignée, distincte de celle de toute poignée issue d'un autre
    /// appel à [`Session::handle`].
    client: ClientId,
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
    /// commandes — l'arbre et l'historique restent alors inchangés. Refusée
    /// (`Command(ForeignMacro("execute"))`) si une macro est ouverte par un
    /// autre émetteur (A4, voir la table en tête de crate).
    pub fn execute(&self, command: Command) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::Execute {
            command,
            client,
            reply,
        })?
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

    /// Ouvre une macro portant `label` — voir la table en tête de crate.
    /// Refusée (`Command(ForeignMacro("begin_macro"))`) si une macro est déjà
    /// ouverte par un *autre* émetteur ; un même émetteur peut toujours
    /// imbriquer la sienne, jusqu'à [`plotyx_commands::MAX_MACRO_DEPTH`]
    /// niveaux (`Err(MacroTooDeep)` au-delà).
    pub fn begin_macro(&self, label: &str) -> SessionResult<Revision> {
        let label = label.to_string();
        let client = self.client;
        self.request(|reply| Request::BeginMacro {
            label,
            client,
            reply,
        })?
        .map_err(SessionError::from)
    }

    /// Ferme la macro courante en une seule entrée d'historique.
    /// `Err(Command(NotInMacro))` si aucune macro n'est ouverte ;
    /// `Err(Command(ForeignMacro("end_macro")))` si une macro est ouverte
    /// mais par un autre émetteur (A4, voir la table en tête de crate).
    pub fn end_macro(&self) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::EndMacro { client, reply })?
            .map_err(SessionError::from)
    }

    /// Annule la macro courante et tout ce qu'elle avait appliqué.
    /// `Err(Command(NotInMacro))` si aucune macro n'est ouverte ;
    /// `Err(Command(ForeignMacro("abort_macro")))` si une macro est ouverte
    /// mais par un autre émetteur — un `abort_macro` ne peut plus défaire le
    /// travail de quelqu'un d'autre (A4, voir la table en tête de crate).
    pub fn abort_macro(&self) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::AbortMacro { client, reply })?
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
            handle: SessionHandle {
                tx,
                client: ClientId::fresh(),
            },
            thread,
        })
    }

    /// Poignée cliente **neuve** vers cette session : un émetteur distinct de
    /// tout autre appel à `handle()` (A4, voir la table en tête de crate).
    /// `.clone()` sur la poignée rendue reste le même émetteur ; seul un
    /// nouvel appel à `handle()` en crée un autre.
    pub fn handle(&self) -> SessionHandle {
        SessionHandle {
            tx: self.handle.tx.clone(),
            client: ClientId::fresh(),
        }
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
