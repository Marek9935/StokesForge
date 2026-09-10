//! Boucle de l'acteur : le seul endroit du processus qui détient un
//! `&mut Project`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use plotyx_commands::{Command, CommandError, CommandResult, History};
use plotyx_core::Project;

use crate::error::{SessionError, SessionResult};
use crate::snapshot::{HistoryState, Revision, Snapshot};

/// Requête envoyée à l'acteur par un [`crate::SessionHandle`]. Privée au
/// crate : les clients ne construisent jamais de `Request` eux-mêmes, ils
/// passent par les méthodes de `SessionHandle`, qui portent chacune leur
/// canal de réponse à usage unique.
pub(crate) enum Request {
    /// Exécute une commande. Répond la nouvelle révision, ou l'erreur rendue
    /// par `History::execute` — l'arbre et l'historique restent alors
    /// inchangés.
    Execute {
        /// Commande à appliquer.
        command: Command,
        /// Réponse : nouvelle révision, ou erreur.
        reply: Sender<CommandResult<Revision>>,
    },
    /// Annule l'entrée du sommet. `Ok(None)` si la pile est vide.
    Undo {
        /// Réponse : nouvelle révision si quelque chose a été annulé.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Rétablit l'entrée annulée la plus récente. `Ok(None)` si rien à rétablir.
    Redo {
        /// Réponse : nouvelle révision si quelque chose a été rétabli.
        reply: Sender<CommandResult<Option<Revision>>>,
    },
    /// Ouvre une macro. Toujours un succès (voir `History::begin_macro`).
    BeginMacro {
        /// Libellé de la macro.
        label: String,
        /// Réponse : nouvelle révision (l'état de l'historique a changé).
        reply: Sender<Revision>,
    },
    /// Ferme la macro courante.
    EndMacro {
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Annule la macro courante et tout ce qu'elle avait appliqué.
    AbortMacro {
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Demande un instantané de l'état courant.
    Snapshot {
        /// Réponse : l'instantané demandé.
        reply: Sender<Snapshot>,
    },
    /// Sauvegarde le projet courant vers `path`. Ne change pas la révision :
    /// sauvegarder ne modifie pas le modèle.
    Save {
        /// Chemin de destination.
        path: PathBuf,
        /// Réponse : succès, ou erreur de persistance.
        reply: Sender<SessionResult<()>>,
    },
    /// Charge un projet complet depuis `path` et le substitue au projet
    /// courant. Vide l'historique. Refusé si une macro est ouverte.
    Load {
        /// Chemin source.
        path: PathBuf,
        /// Réponse : nouvelle révision, ou erreur (persistance, ou
        /// `Command(MacroOpen)` si une macro est ouverte).
        reply: Sender<SessionResult<Revision>>,
    },
    /// Demande l'arrêt de l'acteur.
    Shutdown,
}

/// Boucle de l'acteur : sert les requêtes dans l'ordre FIFO où elles arrivent
/// sur `rx`, jusqu'à ce que tous les émetteurs soient lâchés ou qu'un
/// `Shutdown` arrive. Rend le `Project` final, pour que l'appelant
/// (`Session::shutdown`) puisse le récupérer.
pub(crate) fn run(
    mut project: Project,
    history_limit: Option<usize>,
    rx: Receiver<Request>,
) -> Project {
    let mut history = match history_limit {
        Some(limit) => History::with_limit(limit),
        None => History::new(),
    };
    let mut revision: Revision = 0;

    for request in rx {
        match request {
            Request::Execute { command, reply } => {
                let outcome = history.execute(&mut project, command).map(|()| {
                    revision += 1;
                    revision
                });
                let _ = reply.send(outcome);
            }
            Request::Undo { reply } => {
                let outcome = history.undo(&mut project).map(|reverted| {
                    reverted.then(|| {
                        revision += 1;
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::Redo { reply } => {
                let outcome = history.redo(&mut project).map(|restored| {
                    restored.then(|| {
                        revision += 1;
                        revision
                    })
                });
                let _ = reply.send(outcome);
            }
            Request::BeginMacro { label, reply } => {
                // Toujours un succès : l'état de l'historique (`in_macro`) a
                // changé, donc la révision avance même sans mutation du
                // modèle.
                history.begin_macro(&label);
                revision += 1;
                let _ = reply.send(revision);
            }
            Request::EndMacro { reply } => {
                let outcome = history.end_macro(&mut project).map(|()| {
                    revision += 1;
                    revision
                });
                let _ = reply.send(outcome);
            }
            Request::AbortMacro { reply } => {
                let outcome = history.abort_macro(&mut project).map(|()| {
                    revision += 1;
                    revision
                });
                let _ = reply.send(outcome);
            }
            Request::Snapshot { reply } => {
                let snapshot = build_snapshot(&project, &history, revision);
                let _ = reply.send(snapshot);
            }
            Request::Save { path, reply } => {
                // Ne touche ni au projet ni à l'historique : la révision
                // n'avance pas, sauvegarder ne change pas le modèle.
                let outcome = plotyx_persistence::save(&project, &path).map_err(SessionError::from);
                let _ = reply.send(outcome);
            }
            Request::Load { path, reply } => {
                let outcome = load_project(&mut project, &mut history, &path).map(|()| {
                    revision += 1;
                    revision
                });
                let _ = reply.send(outcome);
            }
            Request::Shutdown => break,
        }
    }

    project
}

/// Charge un projet complet depuis `path` et le substitue à `*project` ; vide
/// l'historique. Refuse (`Err(Command(MacroOpen))`) si une macro est ouverte,
/// **avant** de toucher au disque : dans ce cas ni le projet ni l'historique
/// ne sont affectés. Si le chargement lui-même échoue (fichier absent,
/// corrompu, ...), l'erreur remonte telle quelle et, là aussi, ni le projet
/// ni l'historique n'ont été touchés.
fn load_project(project: &mut Project, history: &mut History, path: &Path) -> SessionResult<()> {
    if history.in_macro() {
        return Err(SessionError::Command(CommandError::MacroOpen("load")));
    }
    let loaded = plotyx_persistence::load(path)?;
    *project = loaded;
    // L'acteur est mono-thread et rien n'a pu rouvrir de macro entre la
    // vérification ci-dessus et cet appel : `clear()` ne peut pas échouer
    // ici. On ignore tout de même le résultat plutôt que d'appeler
    // `unwrap`/`expect`, bannis hors tests.
    let _ = history.clear();
    Ok(())
}

/// Construit l'instantané courant : l'arbre est partagé (`Arc::clone`), rien
/// n'est recopié.
fn build_snapshot(project: &Project, history: &History, revision: Revision) -> Snapshot {
    Snapshot {
        tree: Arc::clone(project.tree()),
        revision,
        history: HistoryState {
            can_undo: history.can_undo(),
            can_redo: history.can_redo(),
            undo_label: history.undo_label().map(str::to_string),
            redo_label: history.redo_label().map(str::to_string),
            in_macro: history.in_macro(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N'existe que pour que le compilateur vérifie l'invariant `Send` — ce
    /// n'est pas un test comportemental.
    fn assert_send<T: Send>() {}

    #[test]
    fn request_is_send() {
        assert_send::<Request>();
    }
}
