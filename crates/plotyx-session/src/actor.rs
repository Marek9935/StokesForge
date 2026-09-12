//! Boucle de l'acteur : le seul endroit du processus qui détient un
//! `&mut Project`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use plotyx_commands::{Command, CommandError, CommandResult, History};
use plotyx_core::Project;

use crate::error::{SessionError, SessionResult};
use crate::snapshot::{HistoryState, Revision, Snapshot};
use crate::ClientId;

/// Requête envoyée à l'acteur par un [`crate::SessionHandle`]. Privée au
/// crate : les clients ne construisent jamais de `Request` eux-mêmes, ils
/// passent par les méthodes de `SessionHandle`, qui portent chacune leur
/// canal de réponse à usage unique.
pub(crate) enum Request {
    /// Exécute une commande. Répond la nouvelle révision, ou l'erreur rendue
    /// par `History::execute` — l'arbre et l'historique restent alors
    /// inchangés. Refusée (`ForeignMacro`) si une macro est ouverte par un
    /// autre émetteur que `client` (A4, voir la table en tête de crate).
    Execute {
        /// Commande à appliquer.
        command: Command,
        /// Émetteur de la requête.
        client: ClientId,
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
    /// Ouvre une macro. `Err(MacroTooDeep)` au-delà de
    /// `History::MAX_MACRO_DEPTH` niveaux d'imbrication (B7,
    /// `guide-de-correction-robustesse.md` §F4). `Err(ForeignMacro)` si une
    /// macro est déjà ouverte par un autre émetteur que `client` (A4) — un
    /// même émetteur peut toujours imbriquer la sienne.
    BeginMacro {
        /// Libellé de la macro.
        label: String,
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision (l'état de l'historique a changé), ou
        /// l'erreur de `History::begin_macro`.
        reply: Sender<CommandResult<Revision>>,
    },
    /// Ferme la macro courante. `Err(ForeignMacro)` si l'émetteur n'est pas
    /// celui qui l'a ouverte (A4).
    EndMacro {
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`,
        /// `ForeignMacro`).
        reply: Sender<CommandResult<Revision>>,
    },
    /// Annule la macro courante et tout ce qu'elle avait appliqué.
    /// `Err(ForeignMacro)` si l'émetteur n'est pas celui qui l'a ouverte —
    /// c'est exactement le refus qui ferme A4 : un `abort_macro` ne peut
    /// plus défaire le travail validé d'un autre client.
    AbortMacro {
        /// Émetteur de la requête.
        client: ClientId,
        /// Réponse : nouvelle révision, ou erreur (`NotInMacro`,
        /// `ForeignMacro`).
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
    // A3 (`guide-de-correction-robustesse.md` §F2/F6) — époque du projet
    // courant : locale à ce fil, **jamais persistée** (voir la doc de
    // `Snapshot::epoch`). N'avance que sur un `Load` qui réussit : ni
    // `Execute`/`Undo`/`Redo`, qui font déjà avancer `revision`, ni les
    // macros ne changent l'identité du projet.
    let mut epoch: u64 = 0;
    // A4 (`guide-de-correction-robustesse.md` §F6) — émetteur qui a ouvert la
    // macro courante, `None` hors macro. Remis à `None` dès que
    // `history.in_macro()` redevient faux, que la fermeture ait réussi ou
    // non (voir les branches `EndMacro`/`AbortMacro` : `History::end_macro`/
    // `abort_macro` peuvent avoir dépilé un niveau avant qu'une erreur ne
    // remonte).
    let mut macro_owner: Option<ClientId> = None;

    for request in rx {
        match request {
            Request::Execute {
                command,
                client,
                reply,
            } => {
                // A4 — une macro ouverte par un autre émetteur n'absorbe plus
                // silencieusement le travail de celui-ci : c'était le bug.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("execute"))
                } else {
                    history.execute(&mut project, command).map(|()| {
                        revision += 1;
                        revision
                    })
                };
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
            Request::BeginMacro {
                label,
                client,
                reply,
            } => {
                // A4 — une macro déjà ouverte par un autre émetteur refuse
                // qu'on s'y imbrique ; le même émetteur le peut toujours
                // (jusqu'à `MAX_MACRO_DEPTH`, B7 ci-dessous).
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("begin_macro"))
                } else {
                    // B7 (`guide-de-correction-robustesse.md` §F4) —
                    // `begin_macro` peut refuser (`MacroTooDeep`) : la
                    // révision n'avance que si l'imbrication a réellement
                    // changé d'état. Un succès, en revanche, avance la
                    // révision même sans mutation du modèle (l'état de
                    // l'historique, `in_macro`, a changé).
                    let result = history.begin_macro(&label);
                    if result.is_ok() {
                        macro_owner = Some(client);
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::EndMacro { client, reply } => {
                // A4 — `NotInMacro` reste réservé au cas où aucune macro
                // n'est ouverte ; `ForeignMacro` couvre le cas où une macro
                // EST ouverte, mais pas par `client`.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("end_macro"))
                } else {
                    let result = history.end_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::AbortMacro { client, reply } => {
                // A4 — c'est ici que le bug se produisait : `abort_macro`
                // défaisait le travail d'un autre client sans le refuser.
                let outcome = if history.in_macro() && macro_owner != Some(client) {
                    Err(CommandError::ForeignMacro("abort_macro"))
                } else {
                    let result = history.abort_macro(&mut project);
                    if !history.in_macro() {
                        macro_owner = None;
                    }
                    result.map(|()| {
                        revision += 1;
                        revision
                    })
                };
                let _ = reply.send(outcome);
            }
            Request::Snapshot { reply } => {
                let snapshot = build_snapshot(&project, &history, revision, epoch);
                let _ = reply.send(snapshot);
            }
            Request::Save { path, reply } => {
                // A11 — palliatif déclaré (guide-de-correction-robustesse.md
                // §F6) : recopie de la garde de `load_project`, pas une
                // réponse à « lesquelles des huit requêtes une macro ouverte
                // doit-elle suspendre ? ». Sans elle, sauvegarder pendant une
                // macro écrit sur le disque un état qu'un `abort_macro`
                // ultérieur défera en mémoire — le fichier garde alors ce que
                // l'annulation a effacé. Ne touche ni au projet ni à
                // l'historique : la révision n'avance pas, sauvegarder ne
                // change pas le modèle. Refusée pour **quiconque** pendant
                // une macro, pas seulement son propriétaire (voir la table de
                // tête de crate) : l'état intermédiaire d'une macro que
                // n'importe qui pourrait encore avorter ne doit pas atteindre
                // le disque.
                let outcome = if history.in_macro() {
                    Err(SessionError::Command(CommandError::MacroOpen("save")))
                } else {
                    plotyx_persistence::save(&project, &path).map_err(SessionError::from)
                };
                let _ = reply.send(outcome);
            }
            Request::Load { path, reply } => {
                let outcome = load_project(&mut project, &mut history, &path).map(|()| {
                    revision += 1;
                    // A3 — l'identité du projet change : une poignée
                    // (`plotyx_script::surface::Handle`) qui a mémorisé
                    // l'ancienne époque ne doit plus pouvoir désigner un
                    // `NodeId` du projet neuf sans être détectée. Un `Load`
                    // vide aussi l'historique (`load_project`), donc il n'y a
                    // plus de macro ouverte à ce point : pas besoin de
                    // remettre `macro_owner` à `None` ici, `load_project`
                    // l'a déjà refusé si une macro était ouverte.
                    epoch += 1;
                    revision
                });
                let _ = reply.send(outcome);
            }
            Request::Shutdown => {
                // Constat non reproductible (guide-de-correction-robustesse.md
                // §7.3, table de tête de `plotyx-session`) : une macro encore
                // ouverte à l'arrêt ne doit pas être commise en silence. Elle
                // est annulée, niveau par niveau, comme si son propriétaire
                // l'avait abandonnée avant de partir — il n'y a plus de
                // client à qui répondre, donc pas de vérification de
                // propriétaire ici (`abort_macro` prend `&mut project`
                // directement, hors du protocole `Request`). Une erreur en
                // cours de dépilement (cas déjà documenté par
                // `History::abort_macro` : « l'arbre peut alors être à moitié
                // défait ») ne doit pas transformer cette boucle en boucle
                // infinie : elle arrête le dépilement plutôt que de
                // l'ignorer.
                while history.in_macro() {
                    if history.abort_macro(&mut project).is_err() {
                        break;
                    }
                }
                break;
            }
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
fn build_snapshot(
    project: &Project,
    history: &History,
    revision: Revision,
    epoch: u64,
) -> Snapshot {
    Snapshot {
        tree: Arc::clone(project.tree()),
        revision,
        history: HistoryState {
            // C6 — pendant une macro, `undo` est toujours refusé
            // (`Err(MacroOpen)`) quoi que dise la pile : l'instantané ne doit
            // pas annoncer une annulation que l'acteur refusera.
            can_undo: history.can_undo() && !history.in_macro(),
            can_redo: history.can_redo(),
            undo_label: history.undo_label().map(str::to_string),
            redo_label: history.redo_label().map(str::to_string),
            in_macro: history.in_macro(),
        },
        epoch,
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
