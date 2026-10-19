//! Acteur qui possède le `Project` de StokesForge et le sert à ses clients (l'UI,
//! le thread de script Python de `stokesforge-script`) par un canal de requêtes.
//!
//! # Rôle
//!
//! Ce crate rend concrète une décision d'architecture du socle
//! (`docs/archive/rustrewrite/roadmap.md` §6) : **le `Project` vit sur son propre
//! thread**. L'UI et le thread de script sont deux clients
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
//! Deux requêtes passent par `stokesforge-persistence` (frontière figée, format
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
//!    crate Python, alors même que `stokesforge-script` existe et l'appelle : le
//!    type d'un objet Python n'est pas nommable ici, ce qui rend le blocage
//!    mutuel GIL ↔ acteur structurellement impossible. Une liste blanche de
//!    dépendances (`tests/no_ui_dependency.rs`) force une décision consciente
//!    à toute nouvelle dépendance, plutôt que de compter sur une relecture.
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
//! | `BeginMacro` | ouvre un niveau d'imbrication supplémentaire, quel que soit l'émetteur, jusqu'à [`stokesforge_commands::MAX_MACRO_DEPTH`] | **refusée** (`ForeignMacro("begin_macro")`) si une macro est déjà ouverte par un *autre* émetteur ; un même émetteur peut toujours imbriquer la sienne |
//! | `EndMacro` | ferme le niveau courant en une entrée d'historique, quel que soit l'émetteur | **refusée** (`ForeignMacro("end_macro")`) si l'émetteur n'est pas le propriétaire — `CommandError::NotInMacro` reste réservé au cas où **aucune** macro n'est ouverte |
//! | `AbortMacro` | annule le niveau courant et tout son contenu, quel que soit l'émetteur — **y compris le travail déjà validé d'un autre client, silencieusement** : c'était exactement A4 | **refusée** (`ForeignMacro("abort_macro")`) si l'émetteur n'est pas le propriétaire : un `abort_macro` ne peut plus défaire le travail de quelqu'un d'autre |
//! | `Save` | refusée pour quiconque pendant une macro ouverte (A11, **palliatif déclaré** en vague 1 : la garde de `load_project` recopiée sans plus de nuance) | inchangé — le refus ne dépend pas de qui a ouvert la macro ; écrire sur le disque l'état intermédiaire d'une macro que *quelqu'un* pourrait encore avorter reste interdit à tous |
//! | `Load` | refusée pour quiconque pendant une macro ouverte, avant même de toucher au disque | inchangé, même raison que `Save` |
//! | `NewProject` | mêmes gardes que `Load` ; remplace l'identité, l'historique et les géométries | refusée pour quiconque pendant une macro |
//!
//! `Snapshot` et `Shutdown` ne mutent rien : `Snapshot` est une lecture pure,
//! toujours servie ; `Shutdown` ne passe pas par ce tableau mais par le
//! constat qui suit.
//!
//! # Géométrie des fenêtres
//!
//! Position et taille des fenêtres de l'UI (un classeur, un graphe, une
//! matrice, une note — tout nœud qui porte une `WindowMeta`) ne sont **pas**
//! un champ du modèle : ni un ajout à `WindowMeta`
//! (`crates/stokesforge-core/src/node.rs`), ni quoi que ce soit de persistant
//! dans `stokesforge-core`. Raison : déplacer ou redimensionner une fenêtre ne
//! doit **jamais** pousser d'entrée sur `History` — Ctrl+Z reste réservé aux
//! vraies mutations de données (invariant 1).
//!
//! L'acteur garde donc une table `NodeId -> `[`WindowGeometry`]` **à côté**
//! du `Project` (voir `actor::run`), mutée par
//! [`SessionHandle::set_window_geometry`] : une requête légère qui ne passe
//! ni par `History` ni par la `revision` qui accompagne undo/redo. Elle fait
//! en revanche avancer un compteur **dédié**, `Snapshot::geometry_revision`
//! — la question posée était : un client de rendu qui ne connaît la
//! position des fenêtres que par le `Snapshot` a quand même besoin de
//! savoir qu'elle a changé pour repeindre, sans que ça ressemble à une
//! mutation du modèle qui invaliderait un cache d'affichage (le
//! `(revision, rect, pixels_per_point, view_generation)` de
//! `stokesforge-render-egui`, voir `docs/rendu.md`) qui n'a aucune raison de
//! l'être. Séparer les deux compteurs referme cette question : `revision`
//! ne bouge jamais pour une géométrie, `geometry_revision` ne bouge jamais
//! pour une mutation du modèle.
//!
//! Suppression d'un nœud : son entrée de géométrie devient orpheline dès
//! qu'une commande (actuelle ou future) le retire de l'arbre. L'acteur la
//! purge automatiquement — `actor::purge_orphaned_geometries`, appelée après
//! tout `Execute`/`Undo`/`Redo` qui a effectivement changé l'arbre — plutôt
//! que de la laisser traîner : c'est un nettoyage sans risque d'une simple
//! entrée de `HashMap` non référencée, et la garder n'aurait aucun usage
//! (rouvrir ce même `NodeId` un jour n'a pas de sens, les identifiants ne
//! sont jamais réutilisés — invariant 7).
//!
//! Persistée malgré tout : [`SessionHandle::save`]/[`SessionHandle::load`]
//! passent par `stokesforge_persistence::save_with_geometries`/
//! `load_with_geometries`, qui écrivent/lisent ce bloc **séparé de l'arbre
//! annulable** dans le fichier `.stf` (`FileBody::window_geometries`,
//! `#[serde(default)]` — voir `docs/persistance.md`) — jamais mêlé aux
//! `NodeRecord`, jamais par `convert.rs`.
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
mod gauge;
mod recalc;
mod snapshot;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{NodeId, Project};

use actor::Request;
use gauge::RecalcGauge;

pub use error::{SessionError, SessionResult};
pub use gauge::RecalcEnd;
pub use recalc::FormulaRecalc;
pub use snapshot::{HistoryState, Revision, Snapshot};

/// Position et taille d'une fenêtre de l'UI — voir « Géométrie des fenêtres »
/// ci-dessus pour ce qu'elle est et pourquoi elle vit ici plutôt que dans le
/// modèle. Réutilise directement le type public de `stokesforge-persistence` : la
/// doc de tête de ce type explique pourquoi ce n'est pas une duplication
/// (persistance est **sous** ce crate dans le graphe de dépendances et ne
/// peut donc pas nommer un type défini ici).
pub use stokesforge_persistence::{WindowGeometry, WindowMode, WindowView};

/// Ce qu'un `Load` a écarté sans échouer (calculs illisibles, #278) : données
/// neutres de `stokesforge-persistence`, mises en phrase par l'application.
pub use stokesforge_persistence::{DiscardedCalculation, LoadReport};

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
    /// Jauge du recalcul interruptible (#239), commune à toutes les poignées.
    gauge: Arc<RecalcGauge>,
}

impl SessionHandle {
    /// Poignée vers la même session, mais avec une identité d'émetteur
    /// **neuve** (A4) : contrairement à `.clone()`, elle n'est pas le même
    /// client. C'est ce qu'il faut à tout composant qui écrit en parallèle
    /// d'une macro ouverte par un autre — fenêtre détachée, script : l'acteur
    /// refuse alors ses écritures (`ForeignMacro`) au lieu de les absorber
    /// dans la macro de l'autre, où l'annulation de celle-ci les emporterait
    /// (issue #21).
    pub fn with_new_identity(&self) -> SessionHandle {
        SessionHandle {
            tx: self.tx.clone(),
            client: ClientId::fresh(),
            gauge: Arc::clone(&self.gauge),
        }
    }

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
            interruptible: false,
        })?
        .map_err(SessionError::from)
    }

    /// Comme [`SessionHandle::execute`], mais rend la main dès la commande
    /// appliquée (#239) : le recalcul des formules continue dans l'acteur et
    /// peut être arrêté par [`SessionHandle::stop_recalc`]. Les requêtes
    /// suivantes attendent, comme toujours, la fin du recalcul ; pour ne pas
    /// bloquer, voir [`SessionHandle::recalc_running`]. La révision rendue
    /// est celle d'avant recalcul ; la fin du recalcul en ajoute une.
    pub fn execute_interruptible(&self, command: Command) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::Execute {
            command,
            client,
            reply,
            interruptible: true,
        })?
        .map_err(SessionError::from)
    }

    /// Numéro du recalcul interruptible en cours, s'il y en a un. Ne passe
    /// pas par l'acteur : ne bloque jamais.
    pub fn recalc_running(&self) -> Option<u64> {
        self.gauge.running()
    }

    /// Attend au plus `timeout` la fin du recalcul en cours ; vrai s'il n'y
    /// en a (plus) aucun.
    pub fn wait_recalc(&self, timeout: std::time::Duration) -> bool {
        self.gauge.wait_idle(timeout)
    }

    /// Demande l'arrêt du recalcul `id` (#239) : il s'interrompt entre deux
    /// colonnes, la commande qui l'avait déclenché est annulée comme par
    /// Ctrl+Z, puis ce qui avait changé est recalculé. Faux si ce recalcul
    /// est déjà fini. Ne bloque jamais.
    pub fn stop_recalc(&self, id: u64) -> bool {
        self.gauge.request_stop(id)
    }

    /// Relève la fin du dernier recalcul interruptible, une seule fois.
    pub fn take_recalc_end(&self) -> Option<RecalcEnd> {
        self.gauge.take_finished()
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

    /// Ouvre une macro de nature `kind` — voir la table en tête de crate.
    /// Refusée (`Command(ForeignMacro("begin_macro"))`) si une macro est déjà
    /// ouverte par un *autre* émetteur ; un même émetteur peut toujours
    /// imbriquer la sienne, jusqu'à [`stokesforge_commands::MAX_MACRO_DEPTH`]
    /// niveaux (`Err(MacroTooDeep)` au-delà).
    pub fn begin_macro(&self, kind: MacroKind) -> SessionResult<Revision> {
        let client = self.client;
        self.request(|reply| Request::BeginMacro {
            kind,
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

    /// Relance des calculs d'analyse (#181) : les `ids` donnés, ou — liste
    /// vide — tous ceux qui sont périmés. Hors historique, comme le recalcul
    /// automatique ; la révision avance. Un calcul dont une entrée a disparu
    /// ou dont l'opération échoue garde ses dernières valeurs et se marque
    /// périmé (`Staleness`).
    pub fn recalculate(&self, ids: &[NodeId]) -> SessionResult<Revision> {
        let client = self.client;
        let ids = ids.to_vec();
        self.request(|reply| Request::Recalculate { ids, client, reply })?
            .map_err(SessionError::from)
    }

    /// Demande un instantané de l'état courant. Bon marché : ne clone pas
    /// l'arbre.
    pub fn snapshot(&self) -> SessionResult<Snapshot> {
        self.request(|reply| Request::Snapshot { reply })
    }

    /// Pose (ou remplace) la géométrie de la fenêtre de `node` — voir
    /// « Géométrie des fenêtres » en tête de crate. Ne touche **ni**
    /// `History` **ni** `revision` : ce n'est pas une mutation du modèle, il
    /// n'y a rien à annuler. Rend la nouvelle valeur de
    /// [`Snapshot::geometry_revision`], le compteur dédié qui permet à un
    /// client de rendu de savoir que la géométrie a changé sans confondre ça
    /// avec un changement du modèle. Toujours acceptée, y compris pendant
    /// une macro ouverte par quelqu'un d'autre (ce n'est ni `Execute`, ni
    /// `Undo`/`Redo`, ni `Save`/`Load` : rien dans la table de tête de crate
    /// ne la concerne).
    pub fn set_window_geometry(
        &self,
        node: NodeId,
        geometry: WindowGeometry,
    ) -> SessionResult<u64> {
        self.request(|reply| Request::SetWindowGeometry {
            node,
            geometry,
            reply,
        })
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
        self.load_with_report(path)
            .map(|(revision, _report)| revision)
    }

    /// Comme [`Self::load`], mais rend aussi le [`LoadReport`] de ce que la
    /// lecture a écarté sans échouer (calculs illisibles, #278), pour que
    /// l'appelant le signale.
    pub fn load_with_report(&self, path: &Path) -> SessionResult<(Revision, LoadReport)> {
        self.request(|reply| Request::Load {
            path: path.to_path_buf(),
            reply,
        })?
    }

    /// Crée un projet vide et remplace l'identité courante. Vide l'historique
    /// et les géométries ; refuse une macro ouverte, comme [`Self::load`].
    pub fn new_project(&self) -> SessionResult<Revision> {
        self.request(|reply| Request::NewProject { reply })?
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
    /// (`None` → [`stokesforge_commands::DEFAULT_LIMIT`]). `Err` si le système
    /// d'exploitation refuse de créer le thread.
    pub fn spawn(project: Project, history_limit: Option<usize>) -> std::io::Result<Self> {
        Self::spawn_with_recalc(project, history_limit, FormulaRecalc::Targeted)
    }

    /// Comme [`Session::spawn`], en choisissant comment l'acteur recalcule les
    /// colonnes-formule. [`FormulaRecalc::Full`] n'existe que comme oracle du
    /// test différentiel (`stokesforge-robustesse`, batterie `b22`) : une
    /// session de production prend [`Session::spawn`].
    pub fn spawn_with_recalc(
        project: Project,
        history_limit: Option<usize>,
        recalc: FormulaRecalc,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let gauge = Arc::new(RecalcGauge::default());
        let actor_gauge = Arc::clone(&gauge);
        let thread = thread::Builder::new()
            .name("stokesforge-session".to_string())
            .spawn(move || actor::run(project, history_limit, recalc, actor_gauge, rx))?;
        Ok(Self {
            handle: SessionHandle {
                tx,
                client: ClientId::fresh(),
                gauge,
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
            gauge: Arc::clone(&self.handle.gauge),
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

    /// `A [1,2,3]` et une colonne `C` vide, pour y poser une formule.
    fn feuille() -> (Session, NodeId, NodeId) {
        let mut project = Project::new("Demo").expect("projet");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        project
            .add_column(
                sheet,
                None,
                Some("A"),
                stokesforge_core::Column::from_f64(&[1.0, 2.0, 3.0]),
            )
            .expect("A");
        let c = project
            .add_column(
                sheet,
                None,
                Some("C"),
                stokesforge_core::Column::new(stokesforge_core::ColumnType::Numeric),
            )
            .expect("C");
        (Session::spawn(project, None).expect("acteur"), sheet, c)
    }

    fn formule_c(c: NodeId) -> Command {
        Command::SetColumnMeta {
            column: c,
            meta: stokesforge_core::ColumnMeta {
                formula: Some("A*2".into()),
                ..Default::default()
            },
        }
    }

    /// #239 : sans Stop, la commande est appliquée, le recalcul s'achève et la
    /// fin est relevée une fois ; chaque étape ajoute une révision.
    #[test]
    fn un_recalcul_interruptible_non_arrete_s_acheve_normalement() {
        let (session, _, c) = feuille();
        let handle = session.handle();
        let before = handle.snapshot().expect("instantané").revision;
        handle
            .execute_interruptible(formule_c(c))
            .expect("commande");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Done));
        assert_eq!(handle.take_recalc_end(), None, "relevée une seule fois");
        let snapshot = handle.snapshot().expect("instantané");
        assert_eq!(
            snapshot.revision,
            before + 2,
            "commande, puis fin de recalcul"
        );
        assert!(snapshot.history.can_undo);
        let column = snapshot.tree.node(c).and_then(|n| n.as_column()).unwrap();
        assert!(column.meta().formula.is_some());
    }

    /// #239 : Stop annule la commande comme Ctrl+Z (historique intact : rien
    /// à rétablir de plus que ce qu'un Undo aurait laissé), et la fin est
    /// `Stopped`. L'arrêt est posé d'avance sur le prochain recalcul, ce qui
    /// le rend déterministe : le premier point d'arrêt le voit.
    #[test]
    fn stop_annule_la_commande_qui_a_declenche_le_recalcul() {
        let (session, _, c) = feuille();
        let handle = session.handle();
        let first = handle.snapshot().expect("instantané");
        handle.gauge.stop_next_for_test();
        handle
            .execute_interruptible(formule_c(c))
            .expect("commande");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Stopped));
        let snapshot = handle.snapshot().expect("instantané");
        let column = snapshot.tree.node(c).and_then(|n| n.as_column()).unwrap();
        assert!(column.meta().formula.is_none(), "la formule est défaite");
        assert!(!snapshot.history.can_undo, "la commande a quitté la pile");
        assert!(snapshot.history.can_redo, "comme après un Ctrl+Z");
        assert!(snapshot.revision > first.revision);
    }

    /// Un Stop tardif (le recalcul visé est fini) ne fait rien — et surtout
    /// n'arrête pas le suivant.
    #[test]
    fn un_stop_tardif_n_arrete_pas_le_recalcul_suivant() {
        let (session, _, c) = feuille();
        let handle = session.handle();
        handle
            .execute_interruptible(formule_c(c))
            .expect("commande");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        let finished = 1; // numéro du premier recalcul
        assert!(!handle.stop_recalc(finished), "déjà fini");
        handle
            .execute_interruptible(Command::AddFolder {
                parent: handle.snapshot().expect("instantané").tree.root(),
                name: "Dossier".into(),
            })
            .expect("commande");
        assert!(handle.wait_recalc(std::time::Duration::from_secs(5)));
        assert_eq!(handle.take_recalc_end(), Some(RecalcEnd::Done));
    }
}
