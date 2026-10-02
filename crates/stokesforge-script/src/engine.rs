//! Le fil de script : un **service permanent** qui possède l'interpréteur.
//!
//! # Un script, un groupe d'écritures
//!
//! Le contexte ouvre une macro exclusive à la première mutation. Une lecture
//! seule laisse la session disponible. À la fin, GIL relâché et interruption
//! désarmée, le groupe est validé ou avorté (exception, arrêt demandé). Une
//! annulation explicite depuis pyst est un point de validation du groupe.
//!
//! # Deux fils, pas un
//!
//! * le **fil de script** (`stokesforge-script`) démarre CPython, en devient donc le
//!   fil principal, et exécute les scripts l'un après l'autre ;
//! * un **fil d'interruption** (`stokesforge-script-irq`), minuscule, existe pour
//!   que [`ScriptEngine::interrupt`] ne bloque **jamais**. Lever une exception
//!   asynchrone dans le fil de script exige le GIL ; le prendre depuis le fil
//!   qui demande l'arrêt (l'interface, par exemple) pourrait l'y faire
//!   attendre. L'interface pose donc un drapeau et envoie un message ; c'est ce
//!   petit fil qui attend le GIL à sa place.
//!
//! # Deux drapeaux, pas un
//!
//! `busy` est posé par [`ScriptEngine::run`] **avant** l'envoi du travail : il
//! sert au refus [`ScriptError::Busy`]. `running` est posé par le fil de script
//! **une fois qu'il est vraiment entré dans le script**, et c'est lui que
//! [`ScriptEngine::is_running`] rend. La distinction n'est pas cosmétique :
//! elle garantit qu'une interruption demandée pendant que `is_running()` est
//! vrai ne peut plus être effacée par la remise à zéro de début d'exécution.
//!
//! # L'interruption, en deux moitiés
//!
//! L'interpréteur démarre **sans gestion des signaux** : aucun
//! `KeyboardInterrupt` ne sera levé tout seul. L'arrêt d'un script est donc
//! entièrement à notre charge, et se joue en deux moitiés complémentaires :
//!
//! 1. un **drapeau atomique** vérifié par chaque appel de la surface `pyst` —
//!    il attrape un script qui boucle en appelant l'API, sans surcoût ;
//! 2. **`PyThreadState_SetAsyncExc`** depuis le fil d'interruption, qui lève un
//!    `KeyboardInterrupt` dans le fil de script au prochain point de contrôle
//!    de l'interpréteur — il attrape une boucle Python **pure**.
//!
//! *Écarté :* un hook de trace via `sys.settrace`, qui coûterait à chaque ligne
//! exécutée, en permanence, pour une granularité dont on n'a pas besoin.
//!
//! **Limite à dire honnêtement** : ni l'un ni l'autre n'interrompt un appel
//! **natif** long. Une opération numpy sur dix millions de points ira à son
//! terme ; l'exception ne sera levée qu'en revenant dans l'interpréteur.
//! Un script qui avale les exceptions (`except: pass` dans une boucle Python
//! pure) résiste également à la seconde moitié — la première le rattrape dès
//! qu'il touche à `pyst`.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_long;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use pyo3::exceptions::{PyKeyboardInterrupt, PyRuntimeError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTracebackMethods};
use stokesforge_commands::MacroKind;
use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_session::SessionHandle;

use crate::error::ScriptError;
use crate::outcome::ScriptOutcome;
use crate::{bridge::HostError, capture, surface};

/// État partagé entre le fil de script, le fil d'interruption et les clients.
///
/// Rien ici n'est un objet Python : ces drapeaux traversent les fils sans
/// jamais toucher au GIL.
#[derive(Debug, Default)]
struct Shared {
    /// Vrai dès qu'un travail est accepté par [`ScriptEngine::run`] et jusqu'à
    /// ce que sa réponse soit prête. Sert au refus `Busy`.
    busy: AtomicBool,
    /// Vrai pendant que le fil de script est effectivement **dans** le script.
    running: AtomicBool,
    /// Vrai quand une interruption a été demandée pour l'exécution en cours.
    interrupt_requested: AtomicBool,
    /// Identifiant Python (`threading.get_ident()`) du fil de script, `0` tant
    /// que l'interpréteur n'a pas démarré.
    py_thread_ident: AtomicU64,
}

/// Ce dont la surface `pyst` a besoin pour servir un appel : la session, et de
/// quoi savoir si l'on doit s'arrêter.
pub(crate) struct ScriptContext {
    /// Poignée cliente vers l'acteur qui possède le `Project`.
    session: SessionHandle,
    /// Drapeaux partagés avec le moteur.
    shared: Arc<Shared>,
    /// Ouverte seulement à la première écriture ; un script de lecture ne
    /// réserve jamais la session. Atomique pour rester utilisable GIL relâché.
    macro_open: AtomicBool,
}

impl ScriptContext {
    /// Poignée vers l'acteur.
    pub(crate) fn session(&self) -> &SessionHandle {
        &self.session
    }

    /// Ouvre le groupe exclusif du script. Rend vrai si l'appel vient de
    /// l'ouvrir : le pont doit alors relire et valider son instantané, car un
    /// chargement ou une écriture a pu précéder l'acquisition de la macro.
    pub(crate) fn begin_write(&self) -> Result<bool, HostError> {
        if self.macro_open.load(Ordering::Acquire) {
            return Ok(false);
        }
        self.session
            .begin_macro(MacroKind::Script)
            .map_err(HostError::from_session)?;
        self.macro_open.store(true, Ordering::Release);
        Ok(true)
    }

    /// Termine le groupe GIL relâché, y compris après une exception Python.
    /// Une interruption demandée entraîne l'abandon même si elle a été avalée
    /// par le script. Les imports ferment leurs seules macros imbriquées.
    pub(crate) fn finish_write(&self, commit: bool) -> Result<(), HostError> {
        if !self.macro_open.load(Ordering::Acquire) {
            return Ok(());
        }
        let result = if commit {
            self.session.end_macro()
        } else {
            self.session.abort_macro()
        };
        match result {
            Ok(_) => {
                self.macro_open.store(false, Ordering::Release);
                Ok(())
            }
            Err(error) => {
                if commit && self.session.abort_macro().is_ok() {
                    self.macro_open.store(false, Ordering::Release);
                }
                Err(HostError::from_session(error))
            }
        }
    }

    /// Première moitié du mécanisme d'interruption : chaque appel de la
    /// surface `pyst` passe par ici.
    pub(crate) fn check_interrupt(&self) -> PyResult<()> {
        if self.shared.interrupt_requested.load(Ordering::Acquire) {
            return Err(PyKeyboardInterrupt::new_err(
                "script interrupted at the request of StokesForge",
            ));
        }
        Ok(())
    }
}

thread_local! {
    /// Contexte du fil courant. Posé uniquement sur le fil de script : c'est ce
    /// qui rend la surface `pyst` inutilisable depuis un fil que le script
    /// aurait créé lui-même, avec un message clair plutôt qu'une course.
    static CONTEXT: RefCell<Option<Arc<ScriptContext>>> = const { RefCell::new(None) };
}

/// Contexte du fil courant, ou une erreur Python explicite.
pub(crate) fn current_context() -> PyResult<Arc<ScriptContext>> {
    CONTEXT.with(|cell| cell.borrow().clone()).ok_or_else(|| {
        PyRuntimeError::new_err(
            "the pyst surface can only be used from the StokesForge script thread",
        )
    })
}

/// Un travail à exécuter et le canal de réponse à usage unique qui lui est
/// propre.
struct Job {
    /// Source Python à exécuter.
    source: String,
    /// Canal sur lequel rendre le résultat.
    reply: mpsc::Sender<ScriptOutcome>,
}

/// Message envoyé au fil de script.
enum Message {
    /// Exécuter un script.
    Run(Job),
    /// Arrêter le fil.
    Stop,
}

/// Message envoyé au fil d'interruption.
enum InterruptMessage {
    /// Lever une exception asynchrone dans le fil de script.
    Now,
    /// Arrêter le fil d'interruption.
    Stop,
}

/// Le fil de script : un service permanent, créé une fois, qui possède
/// l'interpréteur.
///
/// `run` est appelable depuis n'importe quel fil ; c'est ainsi que l'interface
/// l'appellera, sans jamais toucher elle-même à Python.
pub struct ScriptEngine {
    /// Canal FIFO vers le fil de script.
    jobs: mpsc::Sender<Message>,
    /// Canal vers le fil d'interruption.
    interrupts: mpsc::Sender<InterruptMessage>,
    /// Drapeaux partagés.
    shared: Arc<Shared>,
    /// Fil de script, `None` après [`ScriptEngine::shutdown`].
    script_thread: Option<JoinHandle<()>>,
    /// Fil d'interruption, `None` après [`ScriptEngine::shutdown`].
    interrupt_thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl ScriptEngine {
    /// Démarre le fil de script et initialise l'interpréteur embarqué.
    ///
    /// `python` dit où trouver l'interpréteur ; `session` est la poignée vers
    /// l'acteur que les scripts piloteront. Bloque jusqu'à ce que
    /// l'interpréteur soit prêt, de sorte qu'un `Ok` signifie « la surface
    /// `pyst` est posée et un script peut partir ».
    ///
    /// # Erreurs
    ///
    /// [`ScriptError::Thread`] si le système refuse de créer un fil,
    /// [`ScriptError::Interpreter`] si CPython ne démarre pas,
    /// [`ScriptError::Install`] si la surface `pyst` n'a pas pu être posée.
    pub fn spawn(session: SessionHandle, python: EmbeddedPython) -> Result<Self, ScriptError> {
        // Le moteur est toujours un émetteur distinct de son appelant : une
        // simple clone fournie par l'hôte ne doit pas absorber ses écritures
        // dans la macro du script (ni permettre au script d'avorter la sienne).
        let session = session.with_new_identity();
        let shared = Arc::new(Shared::default());
        let (jobs_tx, jobs_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();

        let script_shared = Arc::clone(&shared);
        let script_thread = thread::Builder::new()
            .name("stokesforge-script".to_owned())
            .spawn(move || script_main(python, session, jobs_rx, script_shared, &init_tx))?;

        match init_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                // Le fil s'arrête tout seul après avoir signalé son échec.
                let _ = script_thread.join();
                return Err(error);
            }
            // Le fil a disparu sans rien dire : seule une panique le permet.
            Err(_) => {
                let _ = script_thread.join();
                return Err(ScriptError::Stopped);
            }
        }

        let (interrupts_tx, interrupts_rx) = mpsc::channel();
        let interrupt_shared = Arc::clone(&shared);
        let interrupt_thread = match thread::Builder::new()
            .name("stokesforge-script-irq".to_owned())
            .spawn(move || interrupt_main(&interrupts_rx, &interrupt_shared))
        {
            Ok(thread) => thread,
            Err(error) => {
                // Ne pas laisser derrière soi un fil de script orphelin.
                let _ = jobs_tx.send(Message::Stop);
                let _ = script_thread.join();
                return Err(ScriptError::Thread(error));
            }
        };

        Ok(Self {
            jobs: jobs_tx,
            interrupts: interrupts_tx,
            shared,
            script_thread: Some(script_thread),
            interrupt_thread: Some(interrupt_thread),
        })
    }

    /// Exécute une source Python et **bloque** jusqu'à son terme.
    ///
    /// Appelable depuis n'importe quel fil. L'espace de noms est **conservé
    /// d'une exécution à l'autre**, comme dans une console : une variable
    /// posée par un script reste visible du suivant. C'est ce qui permet de
    /// garder une poignée d'un script à l'autre — et c'est aussi pourquoi une
    /// vue numpy laissée dans une variable **retient l'ancien buffer** d'une
    /// colonne jusqu'à ce qu'elle soit remplacée.
    ///
    /// # Erreurs
    ///
    /// [`ScriptError::Busy`] si un script tourne déjà (le socle en autorise un
    /// à la fois), [`ScriptError::Stopped`] si le fil de script est arrêté.
    /// Une exception **du script** n'est pas une erreur ici : elle est rendue
    /// dans [`ScriptOutcome::error`], avec sa trace d'appels.
    pub fn run(&self, source: &str) -> Result<ScriptOutcome, ScriptError> {
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ScriptError::Busy);
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        let job = Job {
            source: source.to_owned(),
            reply: reply_tx,
        };
        if self.jobs.send(Message::Run(job)).is_err() {
            self.shared.busy.store(false, Ordering::Release);
            return Err(ScriptError::Stopped);
        }
        reply_rx.recv().map_err(|_| {
            self.shared.busy.store(false, Ordering::Release);
            ScriptError::Stopped
        })
    }

    /// Demande l'arrêt du script en cours. Sans effet s'il n'y en a pas.
    ///
    /// **Ne bloque pas** : pose un drapeau et poste un message. C'est le fil
    /// d'interruption qui attendra le GIL pour lever l'exception asynchrone.
    pub fn interrupt(&self) {
        self.shared
            .interrupt_requested
            .store(true, Ordering::Release);
        let _ = self.interrupts.send(InterruptMessage::Now);
    }

    /// Vrai pendant qu'un script s'exécute.
    ///
    /// Passe à vrai quand le fil de script entre effectivement dans le script,
    /// pas dès l'appel de [`ScriptEngine::run`] : c'est ce décalage qui rend
    /// une interruption demandée « pendant que ça tourne » impossible à
    /// perdre.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// Arrête les deux fils et attend leur fin.
    ///
    /// Si un script tourne encore, son exécution est menée à son terme avant
    /// l'arrêt — l'arrêt n'est pas une interruption. Appelez
    /// [`ScriptEngine::interrupt`] d'abord si vous voulez couper court.
    ///
    /// L'interpréteur n'est **pas** finalisé : `Py_Finalize` est une source
    /// connue de plantages en sortie et n'apporte rien à un processus qui se
    /// termine. Conséquence assumée, la même que dans `stokesforge-python` : les
    /// `atexit` Python ne s'exécutent jamais.
    ///
    /// # Erreurs
    ///
    /// [`ScriptError::Stopped`] si l'un des deux fils a paniqué.
    pub fn shutdown(mut self) -> Result<(), ScriptError> {
        let _ = self.jobs.send(Message::Stop);
        let _ = self.interrupts.send(InterruptMessage::Stop);
        let mut panicked = false;
        if let Some(thread) = self.script_thread.take() {
            panicked |= thread.join().is_err();
        }
        if let Some(thread) = self.interrupt_thread.take() {
            panicked |= thread.join().is_err();
        }
        if panicked {
            return Err(ScriptError::Stopped);
        }
        Ok(())
    }
}

impl Drop for ScriptEngine {
    /// Demande l'arrêt des deux fils **sans les attendre**.
    ///
    /// Attendre serait piégeux : un moteur lâché pendant qu'un script boucle
    /// sans fin bloquerait le fil qui le lâche. Qui veut la garantie que tout
    /// est fini appelle [`ScriptEngine::shutdown`].
    fn drop(&mut self) {
        let _ = self.jobs.send(Message::Stop);
        let _ = self.interrupts.send(InterruptMessage::Stop);
    }
}

/// Boucle du fil de script.
fn script_main(
    python: EmbeddedPython,
    session: SessionHandle,
    jobs: mpsc::Receiver<Message>,
    shared: Arc<Shared>,
    init: &mpsc::Sender<Result<(), ScriptError>>,
) {
    let globals = match start(&python, session, &shared) {
        Ok(globals) => {
            let _ = init.send(Ok(()));
            globals
        }
        Err(error) => {
            let _ = init.send(Err(error));
            return;
        }
    };

    for message in jobs {
        match message {
            Message::Stop => break,
            Message::Run(job) => {
                let outcome = execute(&job.source, &globals, &shared);
                // Le drapeau `busy` tombe AVANT la réponse : sinon un client
                // qui relance aussitôt verrait encore le moteur occupé.
                shared.busy.store(false, Ordering::Release);
                let _ = job.reply.send(outcome);
            }
        }
    }

    // Les objets Python détenus par ce fil se détruisent GIL en main, jamais
    // au fil de l'eau : décrémenter un compteur de références est une
    // opération Python.
    Python::attach(move |_py| drop(globals));
}

/// Démarre l'interpréteur, pose la surface `pyst`, la capture de sortie et
/// l'espace de noms persistant.
fn start(
    python: &EmbeddedPython,
    session: SessionHandle,
    shared: &Arc<Shared>,
) -> Result<Py<PyDict>, ScriptError> {
    python.initialize()?;

    let context = Arc::new(ScriptContext {
        session,
        shared: Arc::clone(shared),
        macro_open: AtomicBool::new(false),
    });
    CONTEXT.with(|cell| *cell.borrow_mut() = Some(context));

    Python::attach(|py| -> PyResult<Py<PyDict>> {
        // L'identifiant Python du fil, seul argument que
        // `PyThreadState_SetAsyncExc` sache reconnaître.
        let ident = py
            .import("threading")?
            .call_method0("get_ident")?
            .extract::<u64>()?;
        shared.py_thread_ident.store(ident, Ordering::Release);

        surface::install(py)?;
        capture::install(py)?;
        // `traceback` est importé ici, une fois pour toutes : sur le chemin
        // d'erreur, `py.import` se réduit alors à une lecture de
        // `sys.modules`, sans prendre le verrou d'importation de CPython. Un
        // import au moment de formater une trace serait le pire endroit pour
        // se bloquer.
        py.import("traceback")?;

        let globals = PyDict::new(py);
        globals.set_item("__name__", "__main__")?;
        globals.set_item("__builtins__", py.import("builtins")?)?;
        globals.set_item("pyst", py.import("pyst")?)?;
        Ok(globals.unbind())
    })
    .map_err(|error| ScriptError::Install(Python::attach(|py| format_exception(py, &error))))
}

/// Exécute une source, capture ses sorties, formate son éventuelle exception.
fn execute(source: &str, globals: &Py<PyDict>, shared: &Shared) -> ScriptOutcome {
    // Remise à zéro AVANT que `running` ne passe à vrai : une interruption
    // demandée alors que `is_running()` est déjà vrai ne peut plus être
    // effacée par cette ligne.
    shared.interrupt_requested.store(false, Ordering::Release);

    Python::attach(|py| {
        clear_pending_async_exception(py, shared);
        capture::begin();
        shared.running.store(true, Ordering::Release);

        let mut result = match CString::new(source) {
            Ok(code) => py.run(&code, Some(globals.bind(py)), None),
            Err(_) => Err(pyo3::exceptions::PyValueError::new_err(
                "the script source contains a null byte",
            )),
        };

        // Ces deux lignes forment un tout, et leur ordre est le cœur de la
        // sûreté de l'interruption. Elles s'exécutent GIL en main, et le fil
        // d'interruption relit `running` GIL en main lui aussi : dès que
        // `running` retombe, plus aucune exception asynchrone ne peut être
        // armée, et celle qui l'était déjà est retirée avant que la moindre
        // instruction Python ne soit exécutée pour formater le résultat.
        //
        // Sans cela, une interruption arrivée juste avant la fin d'un script
        // pouvait se déclencher **à l'intérieur** d'un `import` du formatage
        // d'erreur, et laisser le verrou d'importation global de CPython pris
        // pour toujours — c'est-à-dire figer tout l'interpréteur. C'est le
        // « naive misuse » contre lequel la documentation de
        // `PyThreadState_SetAsyncExc` met en garde.
        shared.running.store(false, Ordering::Release);
        clear_pending_async_exception(py, shared);

        let interrupted = shared.interrupt_requested.load(Ordering::Acquire);
        // Aucun appel Python pendant la fermeture : l'exception asynchrone
        // est désarmée et l'acteur ne reçoit que des données Rust.
        if let Ok(context) = current_context() {
            if let Err(error) = py.detach(|| context.finish_write(result.is_ok() && !interrupted)) {
                result = Err(surface::to_py_error(error));
            }
        }
        let (stdout, stderr) = capture::end();

        ScriptOutcome {
            stdout,
            stderr,
            error: result.err().map(|error| format_exception(py, &error)),
            interrupted,
        }
    })
}

/// Formate une exception Python avec sa trace d'appels complète, chaînage
/// compris — c'est ce que l'utilisateur doit lire pour comprendre son erreur.
fn format_exception(py: Python<'_>, error: &PyErr) -> String {
    if let Ok(text) = format_with_traceback_module(py, error) {
        return text;
    }
    // Repli si le module `traceback` est indisponible : la trace brute reste
    // meilleure que rien.
    match error.traceback(py).and_then(|trace| trace.format().ok()) {
        Some(trace) => format!("{trace}{error}"),
        None => error.to_string(),
    }
}

/// Formate par `traceback.format_exception`, comme le ferait l'interpréteur.
fn format_with_traceback_module(py: Python<'_>, error: &PyErr) -> PyResult<String> {
    let parts = py.import("traceback")?.call_method1(
        "format_exception",
        (error.get_type(py), error.value(py), error.traceback(py)),
    )?;
    Ok(parts.extract::<Vec<String>>()?.concat())
}

/// Retire l'exception asynchrone éventuellement en attente sur le fil de
/// script.
fn clear_pending_async_exception(py: Python<'_>, shared: &Shared) {
    let ident = shared.py_thread_ident.load(Ordering::Acquire);
    if ident == 0 {
        return;
    }
    let _ = py;
    // SAFETY : `PyThreadState_SetAsyncExc` exige le GIL — le jeton `py` en est
    // la preuve — et un identifiant de fil ; un identifiant inconnu est un
    // no-op documenté (la fonction rend alors 0). Le second argument nul
    // signifie « retire l'exception en attente », ne transfère aucune
    // référence et ne peut pas échouer.
    unsafe {
        ffi::PyThreadState_SetAsyncExc(ident as c_long, std::ptr::null_mut());
    }
}

/// Boucle du fil d'interruption.
fn interrupt_main(messages: &mpsc::Receiver<InterruptMessage>, shared: &Arc<Shared>) {
    while let Ok(message) = messages.recv() {
        match message {
            InterruptMessage::Stop => break,
            InterruptMessage::Now => {
                let ident = shared.py_thread_ident.load(Ordering::Acquire);
                if ident == 0 {
                    continue;
                }
                Python::attach(|py| {
                    // `running` est relu **GIL en main**, et le fil de script
                    // le remet à faux GIL en main lui aussi : les deux gestes
                    // sont donc ordonnés l'un par rapport à l'autre. C'est ce
                    // qui garantit qu'aucune exception asynchrone ne peut être
                    // armée après la fin d'un script — un test hors script
                    // laisserait passer une interruption qui exploserait dans
                    // le script suivant, ou pire, au milieu d'un `import` de
                    // l'hôte.
                    if !shared.running.load(Ordering::Acquire) {
                        return;
                    }
                    let exception = py.get_type::<PyKeyboardInterrupt>();
                    // SAFETY : le GIL est détenu (jeton `py`), l'identifiant
                    // est celui du fil de script relevé après le démarrage de
                    // l'interpréteur, et `exception` est un type-objet vivant
                    // le temps de l'appel — `PyThreadState_SetAsyncExc` ne vole
                    // aucune référence. Un identifiant devenu inconnu est un
                    // no-op documenté.
                    unsafe {
                        ffi::PyThreadState_SetAsyncExc(ident as c_long, exception.as_ptr());
                    }
                });
            }
        }
    }
}
