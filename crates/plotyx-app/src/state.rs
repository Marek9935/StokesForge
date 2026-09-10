//! L'état du banc d'essai, **sans aucune dépendance à egui**.
//!
//! # Pourquoi ce découpage
//!
//! Une fenêtre ne se teste pas en intégration continue. Toute la logique du
//! banc vit donc ici, pilotée par un enum d'[`Action`] ; la couche `eframe`
//! (module [`crate::app`]) ne fait que traduire clavier et souris en actions
//! et peindre l'état. C'est ce qui rend la boucle complète — modèle →
//! commande → annulation → sauvegarde → rechargement — vérifiable
//! automatiquement (`tests/shell.rs`).
//!
//! # Deux règles de conduite
//!
//! 1. [`AppState::dispatch`] **ne rend jamais d'erreur** : toute erreur
//!    devient un [`StatusMessage`] affiché à l'utilisateur. Un banc d'essai
//!    qui meurt sur une saisie invalide ne prouve rien.
//! 2. L'instantané détenu est **figé** (l'arbre du modèle est persistant) : il
//!    faut le redemander à l'acteur après chaque mutation, ce que fait
//!    [`AppState::refresh`], appelé par `dispatch` après toute action qui a pu
//!    changer le modèle.
//!
//! # Le script Python (feature `python`)
//!
//! Trois décisions, prises ici et nulle part ailleurs :
//!
//! 1. **L'interpréteur ne démarre jamais à l'ouverture de la fenêtre.**
//!    [`AppState::new`] ne construit ni ne touche à `plotyx_script::
//!    ScriptEngine` ; seul le premier [`Action::RunScript`] le fait, via
//!    `AppState::run_script`. Le temps de démarrage de l'interpréteur est
//!    l'un des motifs du portage — le perdre à l'ouverture annulerait le
//!    bénéfice.
//! 2. **`RunScript` ne bloque jamais l'appelant.** `ScriptEngine::run`, lui,
//!    bloque jusqu'au terme du script — l'appeler depuis la boucle de rendu
//!    figerait la fenêtre, y compris le bouton « Arrêter ». `run_script` lance
//!    donc (la création paresseuse du moteur comprise) sur un **fil détaché**
//!    et revient aussitôt ; [`AppState::poll_script`], à appeler à **chaque
//!    image** par [`crate::app`] (et par un test qui attend une exécution),
//!    sonde le résultat par `try_recv` et l'applique dès qu'il est prêt —
//!    rafraîchissement de l'instantané compris. C'est ce qui rend `dispatch`
//!    sûr à appeler depuis la boucle de rendu pour **toutes** les actions,
//!    script inclus, sans cas particulier côté `crate::app`.
//! 3. **Une exception de script n'est pas une erreur d'application.** Elle
//!    atterrit dans la sortie ([`AppState::script_output`]) avec sa trace
//!    d'appels ; le message de statut posé par `poll_script` reste neutre
//!    (`is_error: false`), et l'état reste utilisable.

use std::path::PathBuf;
#[cfg(feature = "python")]
use std::sync::mpsc;
use std::sync::Arc;
#[cfg(feature = "python")]
use std::thread;

use plotyx_commands::Command;
use plotyx_core::{Cell, Column, ColumnType, Node, NodeId, Tree, ValueBlock};
#[cfg(feature = "python")]
use plotyx_python::interpreter::EmbeddedPython;
#[cfg(feature = "python")]
use plotyx_script::{ScriptEngine, ScriptOutcome};
use plotyx_session::{SessionHandle, SessionResult, Snapshot};

/// Ce que l'utilisateur peut demander. La couche fenêtre ne fait que produire
/// ces valeurs ; elle ne touche jamais l'acteur directement.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Annule la dernière entrée d'historique.
    Undo,
    /// Rétablit la dernière entrée annulée.
    Redo,
    /// Enregistre le projet dans le fichier courant ([`AppState::path`]).
    Save,
    /// Recharge le projet depuis le fichier courant.
    Load,
    /// Sélectionne la feuille affichée dans la grille.
    SelectSheet(NodeId),
    /// Valide la saisie d'une cellule : le texte tel que tapé, à convertir
    /// selon le type de la colonne.
    CommitCell {
        /// Colonne visée.
        column: NodeId,
        /// Ligne visée.
        row: usize,
        /// Texte saisi, non converti.
        text: String,
    },
    /// Ajoute une colonne en fin de la feuille affichée.
    AddColumn,
    /// Insère `count` lignes à `at` dans la feuille affichée.
    InsertRows {
        /// Ligne d'insertion.
        at: usize,
        /// Nombre de lignes insérées.
        count: usize,
    },
    /// Supprime `count` lignes à `at` dans la feuille affichée.
    DeleteRows {
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes supprimées.
        count: usize,
    },
    /// Lance le script actuellement dans la zone de saisie
    /// ([`AppState::script_source`]).
    ///
    /// **Ne bloque pas** : voir la section « Le script Python » en tête de ce
    /// module. Sans la feature `python`, produit un [`StatusMessage`]
    /// explicite et n'envoie aucune commande.
    RunScript,
    /// Demande l'arrêt du script en cours. Sans effet s'il n'y en a pas.
    /// Ne bloque pas.
    InterruptScript,
}

/// Un message à afficher à l'utilisateur, avec sa gravité.
///
/// `is_error` sépare deux choses que Qt distinguait par le grisage d'une
/// action : « impossible pour l'instant » (annuler à vide) n'est **pas** une
/// erreur, alors qu'un chemin illisible en est une.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusMessage {
    /// Texte affiché, en français.
    pub text: String,
    /// Vrai si le message signale un échec.
    pub is_error: bool,
}

/// Ce qu'une exécution de script a produit, tel que l'état applicatif le
/// présente.
///
/// Copie **volontairement indépendante** de `plotyx_script::ScriptOutcome` :
/// `state.rs` doit rester lisible, et sa surface publique stable, que la
/// feature `python` soit active ou non — [`AppState::script_output`] rend ce
/// type dans les deux configurations plutôt que d'exposer un type qui
/// n'existerait que sous la feature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptOutput {
    /// Ce que le script a écrit sur la sortie standard (`print`).
    pub stdout: String,
    /// Ce que le script a écrit sur la sortie d'erreur.
    pub stderr: String,
    /// Message d'erreur **avec sa trace d'appels Python complète**, si le
    /// script a levé une exception. Une exception de script n'est pas une
    /// erreur d'application : ce champ n'influence jamais [`StatusMessage::
    /// is_error`].
    pub error: Option<String>,
    /// Vrai si l'exécution s'est arrêtée sur une interruption demandée
    /// ([`Action::InterruptScript`]).
    pub interrupted: bool,
}

#[cfg(feature = "python")]
impl From<ScriptOutcome> for ScriptOutput {
    fn from(outcome: ScriptOutcome) -> Self {
        Self {
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: outcome.error,
            interrupted: outcome.interrupted,
        }
    }
}

/// Saisie en cours dans une cellule.
///
/// Une seule cellule est éditable à la fois : c'est ce qui permet à la grille
/// virtualisée de ne garder qu'un tampon, et non un par cellule visible. Le
/// tampon n'est envoyé au modèle qu'à la validation (perte de focus ou
/// Entrée), jamais à chaque frappe — sinon l'historique compterait une entrée
/// par caractère.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    /// Colonne en cours de saisie.
    pub column: NodeId,
    /// Ligne en cours de saisie.
    pub row: usize,
    /// Texte tapé jusqu'ici.
    pub buffer: String,
}

/// L'état du banc d'essai : une poignée vers l'acteur, le dernier instantané
/// reçu, ce qui est sélectionné, le chemin de fichier courant, le dernier
/// message et la saisie en cours.
pub struct AppState {
    /// Poignée cliente vers l'acteur qui possède le `Project`.
    handle: SessionHandle,
    /// Dernier instantané reçu : figé, à rafraîchir après toute mutation.
    snapshot: Snapshot,
    /// Feuille affichée dans la grille, si l'arbre en contient une.
    selected_sheet: Option<NodeId>,
    /// Chemin du fichier de travail (saisi à la main, cf. `crate::app`).
    path: String,
    /// Dernier message destiné à l'utilisateur.
    status: Option<StatusMessage>,
    /// Saisie de cellule en cours, s'il y en a une.
    editing: Option<CellEdit>,
    /// Texte de la zone de script.
    script_source: String,
    /// Sortie de la dernière exécution de script, s'il y en a eu une.
    script_output: Option<ScriptOutput>,
    /// Descripteur de l'interpréteur embarqué, posé par [`AppState::
    /// set_python`]. `None` tant qu'il n'a pas été posé — l'interpréteur
    /// lui-même n'est démarré ni ici ni là, seulement au premier script.
    #[cfg(feature = "python")]
    python: Option<EmbeddedPython>,
    /// Le moteur de script, une fois démarré (au premier [`Action::
    /// RunScript`] réussi). Partagé par `Arc` avec le fil détaché qui
    /// exécute le script en cours, s'il y en a un.
    #[cfg(feature = "python")]
    engine: Option<Arc<ScriptEngine>>,
    /// Canal de réponse d'une exécution en cours sur un fil détaché.
    /// `Some` tant que [`AppState::poll_script`] n'a pas reçu de réponse.
    #[cfg(feature = "python")]
    pending_script: Option<mpsc::Receiver<PendingScriptResult>>,
}

/// Ce que le fil détaché d'exécution rend à [`AppState::poll_script`] : le
/// moteur (nouvellement créé ou réutilisé, pour que l'appelant le mette en
/// cache) et le résultat du script, déjà réduit à un message si l'hôte a
/// refusé ou échoué — une [`plotyx_script::ScriptError`] n'a pas à traverser
/// ce canal, `state.rs` n'en a pas besoin au-delà de son message.
#[cfg(feature = "python")]
type PendingScriptResult = (Option<Arc<ScriptEngine>>, Result<ScriptOutcome, String>);

impl AppState {
    /// Construit l'état depuis une poignée d'acteur ; sélectionne la première
    /// feuille trouvée en parcours préfixe. `Err` seulement si l'acteur ne
    /// répond pas — il n'y a alors rien à afficher du tout.
    ///
    /// Ne touche **jamais** à l'interpréteur Python : voir la section « Le
    /// script Python » en tête de ce module.
    pub fn new(handle: SessionHandle, path: String) -> SessionResult<Self> {
        let snapshot = handle.snapshot()?;
        let mut state = Self {
            handle,
            snapshot,
            selected_sheet: None,
            path,
            status: None,
            editing: None,
            script_source: String::new(),
            script_output: None,
            #[cfg(feature = "python")]
            python: None,
            #[cfg(feature = "python")]
            engine: None,
            #[cfg(feature = "python")]
            pending_script: None,
        };
        state.revalidate_selection();
        Ok(state)
    }

    /// Dernier instantané reçu.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Arbre du dernier instantané.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.snapshot.tree
    }

    /// Feuille affichée, si l'arbre en contient une.
    pub fn selected_sheet(&self) -> Option<NodeId> {
        self.selected_sheet
    }

    /// Chemin du fichier de travail.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Remplace le chemin du fichier de travail.
    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }

    /// Dernier message destiné à l'utilisateur.
    pub fn status(&self) -> Option<&StatusMessage> {
        self.status.as_ref()
    }

    /// Texte actuel de la zone de script.
    pub fn script_source(&self) -> &str {
        &self.script_source
    }

    /// Remplace le texte de la zone de script.
    pub fn set_script_source(&mut self, source: String) {
        self.script_source = source;
    }

    /// Sortie de la dernière exécution de script, s'il y en a eu une.
    pub fn script_output(&self) -> Option<&ScriptOutput> {
        self.script_output.as_ref()
    }

    /// Vrai si ce binaire embarque le pont Python (feature `python`).
    ///
    /// Reflète uniquement la **compilation**, pas la localisation d'un
    /// interpréteur : un binaire compilé avec la feature mais qui n'a pas
    /// trouvé son préfixe embarqué ([`AppState::set_python`] jamais appelé)
    /// reste `true` ici, et [`Action::RunScript`] l'explique à l'exécution —
    /// le même choix que pour toute autre action de ce banc (échouer avec un
    /// message plutôt que griser par anticipation).
    #[must_use]
    pub fn script_available(&self) -> bool {
        cfg!(feature = "python")
    }

    /// Vrai pendant qu'un script s'exécute (entre [`Action::RunScript`] et le
    /// [`AppState::poll_script`] qui en récupère le résultat).
    #[must_use]
    pub fn script_running(&self) -> bool {
        #[cfg(feature = "python")]
        {
            self.pending_script.is_some()
        }
        #[cfg(not(feature = "python"))]
        {
            false
        }
    }

    /// Pose le descripteur de l'interpréteur embarqué à utiliser au premier
    /// script. **Ne démarre rien** : `EmbeddedPython::locate`/`with_home` ne
    /// touchent pas à CPython, seul `ScriptEngine::spawn` le fait — et lui
    /// n'est appelé que depuis [`AppState::run_script`].
    #[cfg(feature = "python")]
    pub fn set_python(&mut self, python: EmbeddedPython) {
        self.python = Some(python);
    }

    /// Libellé de l'action « Annuler » : `Some("Annuler Saisie dans A")`, ou
    /// `None` si la pile est vide (l'entrée de menu est alors grisée).
    pub fn undo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .undo_label
            .as_ref()
            .map(|label| format!("Annuler {label}"))
    }

    /// Libellé de l'action « Rétablir », `None` si rien à rétablir.
    pub fn redo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .redo_label
            .as_ref()
            .map(|label| format!("Rétablir {label}"))
    }

    /// Vrai s'il y a quelque chose à annuler.
    pub fn can_undo(&self) -> bool {
        self.snapshot.history.can_undo
    }

    /// Vrai s'il y a quelque chose à rétablir.
    pub fn can_redo(&self) -> bool {
        self.snapshot.history.can_redo
    }

    /// Colonnes de la feuille affichée, dans l'ordre du modèle :
    /// `(NodeId, nom, type)`. Vide s'il n'y a pas de feuille sélectionnée.
    pub fn visible_columns(&self) -> Vec<(NodeId, String, ColumnType)> {
        let Some(sheet) = self.selected_sheet else {
            return Vec::new();
        };
        let Some(node) = self.snapshot.tree.node(sheet) else {
            return Vec::new();
        };
        node.children()
            .iter()
            .filter_map(|id| {
                let child = self.snapshot.tree.node(*id)?;
                let column = child.as_column()?;
                Some((*id, child.name().to_string(), column.column_type()))
            })
            .collect()
    }

    /// Nombre de lignes de la feuille affichée — le **maximum** des longueurs
    /// de ses colonnes, pas la longueur de la première : une feuille n'est pas
    /// un tableau rectangulaire. 0 s'il n'y a pas de feuille sélectionnée.
    pub fn row_count(&self) -> usize {
        self.selected_sheet
            .and_then(|sheet| self.snapshot.tree.row_count(sheet).ok())
            .unwrap_or(0)
    }

    /// Texte à afficher dans une cellule : `""` pour une valeur manquante
    /// (`NaN` en numérique, `None` en texte) ou hors bornes, sinon la valeur
    /// formatée.
    ///
    /// Le formatage numérique est le `Display` de `f64` (`1.5` → « 1.5 »,
    /// `1.0` → « 1 »). Ce choix est délibéré pour un banc d'essai : il est
    /// court, lisible, et n'invente pas de politique d'arrondi. Le vrai
    /// formatage appartiendra à `ColumnMeta::display_format`, que cette phase
    /// ne lit pas.
    pub fn cell_text(&self, column: NodeId, row: usize) -> String {
        let Some(column) = self.snapshot.tree.node(column).and_then(Node::as_column) else {
            return String::new();
        };
        match column.get(row) {
            Cell::Float(value) if value.is_nan() => String::new(),
            Cell::Float(value) => format!("{value}"),
            Cell::Text(None) => String::new(),
            Cell::Text(Some(text)) => text,
        }
    }

    /// Saisie de cellule en cours, s'il y en a une.
    pub fn editing(&self) -> Option<&CellEdit> {
        self.editing.as_ref()
    }

    /// Saisie en cours, modifiable : la couche fenêtre y branche son
    /// `TextEdit` pour que les frappes atterrissent dans le tampon sans
    /// toucher au modèle.
    pub fn editing_mut(&mut self) -> Option<&mut CellEdit> {
        self.editing.as_mut()
    }

    /// Ouvre la saisie d'une cellule, tampon initialisé au texte affiché.
    /// Remplace sans valider une éventuelle saisie précédente : c'est à
    /// l'appelant de valider avant, s'il le souhaite.
    pub fn begin_edit(&mut self, column: NodeId, row: usize) {
        let buffer = self.cell_text(column, row);
        self.editing = Some(CellEdit {
            column,
            row,
            buffer,
        });
    }

    /// Abandonne la saisie en cours sans rien écrire.
    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    /// Valide la saisie en cours (équivaut à [`Action::CommitCell`] sur le
    /// tampon). Sans effet s'il n'y a pas de saisie ouverte.
    pub fn commit_edit(&mut self) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        self.dispatch(Action::CommitCell {
            column: edit.column,
            row: edit.row,
            text: edit.buffer,
        });
    }

    /// Les feuilles du projet, pour l'explorateur : `(NodeId, chemin
    /// lisible)`, en ordre de parcours préfixe.
    ///
    /// Le chemin est reconstruit ici (« Book1 / Sheet1 ») plutôt qu'obtenu de
    /// `Project::path_of` : cette méthode vit sur `Project`, que l'UI ne
    /// détient pas — elle n'a qu'un `Arc<Tree>`. C'est un manque relevé par
    /// cette phase, pas un choix esthétique.
    pub fn sheets(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| node.kind().is_worksheet())
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Chemin lisible d'un nœud : les noms de ses ancêtres (racine exclue)
    /// puis le sien, séparés par « / ».
    fn readable_path(&self, id: NodeId) -> String {
        let tree = &self.snapshot.tree;
        let root = tree.root();
        let mut parts: Vec<String> = tree
            .ancestors(id)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .filter(|ancestor| *ancestor != root)
            .filter_map(|ancestor| tree.node(ancestor).map(|node| node.name().to_string()))
            .collect();
        if let Some(node) = tree.node(id) {
            parts.push(node.name().to_string());
        }
        parts.join(" / ")
    }

    /// Applique une action. Ne rend jamais d'erreur : toute erreur devient un
    /// [`StatusMessage`]. Rafraîchit l'instantané après toute action qui a pu
    /// changer le modèle.
    ///
    /// **Ne bloque jamais**, `RunScript` compris : voir la section « Le
    /// script Python » en tête de ce module. C'est ce qui rend cette méthode
    /// sûre à appeler depuis la boucle de rendu pour toute action.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Save => self.save(),
            Action::Load => self.load(),
            Action::SelectSheet(id) => self.select_sheet(id),
            Action::CommitCell { column, row, text } => self.commit_cell(column, row, text),
            Action::AddColumn => self.add_column(),
            Action::InsertRows { at, count } => self.rows(at, count, true),
            Action::DeleteRows { at, count } => self.rows(at, count, false),
            Action::RunScript => self.run_script(),
            Action::InterruptScript => self.interrupt_script(),
        }
    }

    /// Rafraîchit l'instantané depuis l'acteur et revalide la sélection.
    ///
    /// La revalidation n'est pas réservée au rechargement : c'est le seul
    /// endroit qui garantit que la feuille affichée existe encore, quelle que
    /// soit la façon dont l'arbre a changé (y compris par un autre client de
    /// l'acteur). Un échec de rafraîchissement écrase le message courant —
    /// l'acteur injoignable est plus grave que ce qu'on allait dire.
    pub fn refresh(&mut self) {
        match self.handle.snapshot() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.revalidate_selection();
            }
            Err(err) => self.fail(format!("Session injoignable : {err}")),
        }
    }

    /// Sonde le fil détaché d'une exécution de script en cours, sans jamais
    /// bloquer. À appeler à **chaque image** (voir [`crate::app`]) : sans
    /// notification poussée de l'acteur, c'est le seul endroit qui rafraîchit
    /// l'instantané après un script — l'oublier laisserait la grille figée
    /// alors que le modèle a changé.
    ///
    /// Sans effet si aucun script n'est en cours (y compris sans la feature
    /// `python`, où cette méthode ne fait jamais rien) : sûr à appeler à
    /// chaque image sans test préalable.
    pub fn poll_script(&mut self) {
        #[cfg(feature = "python")]
        {
            let received = match &self.pending_script {
                Some(receiver) => match receiver.try_recv() {
                    Ok(reply) => Some(reply),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending_script = None;
                        self.fail(
                            "Le fil de script a disparu sans répondre (a-t-il paniqué ?)."
                                .to_string(),
                        );
                        return;
                    }
                },
                None => None,
            };
            let Some((engine, result)) = received else {
                return;
            };
            self.pending_script = None;
            if let Some(engine) = engine {
                self.engine = Some(engine);
            }
            match result {
                Ok(outcome) => self.finish_script_run(outcome),
                Err(message) => self.fail(message),
            }
        }
    }

    // -- détail interne ----------------------------------------------------

    /// Si la feuille sélectionnée a disparu (rechargement, suppression), en
    /// choisit une autre ; `None` si l'arbre n'a aucune feuille.
    fn revalidate_selection(&mut self) {
        let tree = &self.snapshot.tree;
        let still_valid = self
            .selected_sheet
            .is_some_and(|id| tree.node(id).is_some_and(|node| node.kind().is_worksheet()));
        if still_valid {
            return;
        }
        self.selected_sheet = tree.walk_all().into_iter().find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.kind().is_worksheet())
        });
        // Le tampon de saisie pointe un nœud de l'arbre précédent : il n'a
        // plus de sens ici.
        self.editing = None;
    }

    /// Enregistre un message informatif.
    fn notice(&mut self, text: String) {
        self.status = Some(StatusMessage {
            text,
            is_error: false,
        });
    }

    /// Enregistre un message d'échec.
    fn fail(&mut self, text: String) {
        self.status = Some(StatusMessage {
            text,
            is_error: true,
        });
    }

    /// Envoie une commande à l'acteur et rend compte du résultat.
    fn execute(&mut self, command: Command) {
        // Le libellé se calcule sur l'arbre d'**avant** : après coup, une
        // colonne supprimée n'a plus de nom.
        let label = command.label(&self.snapshot.tree);
        match self.handle.execute(command) {
            Ok(_) => {
                self.notice(label);
                self.refresh();
            }
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn undo(&mut self) {
        // Le libellé disparaît de la pile dès que l'annulation est faite : il
        // faut le lire avant.
        let label = self.snapshot.history.undo_label.clone();
        match self.handle.undo() {
            Ok(Some(_)) => {
                let label = label.unwrap_or_else(|| "la dernière action".to_string());
                self.notice(format!("Annulé : {label}"));
                self.refresh();
            }
            Ok(None) => self.notice("Rien à annuler".to_string()),
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn redo(&mut self) {
        let label = self.snapshot.history.redo_label.clone();
        match self.handle.redo() {
            Ok(Some(_)) => {
                let label = label.unwrap_or_else(|| "la dernière action".to_string());
                self.notice(format!("Rétabli : {label}"));
                self.refresh();
            }
            Ok(None) => self.notice("Rien à rétablir".to_string()),
            Err(err) => self.fail(format!("{err}")),
        }
    }

    fn save(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.save(&path) {
            Ok(()) => {
                let text = format!("Enregistré dans {}", self.path);
                self.notice(text);
            }
            Err(err) => self.fail(format!("Échec de l'enregistrement : {err}")),
        }
    }

    fn load(&mut self) {
        let path = PathBuf::from(&self.path);
        match self.handle.load(&path) {
            Ok(_) => {
                // Les identifiants du fichier n'ont aucun rapport avec ceux de
                // l'arbre précédent : la sélection doit être revalidée, ce que
                // fait `refresh`.
                self.editing = None;
                let text = format!("Chargé depuis {}", self.path);
                self.notice(text);
                self.refresh();
            }
            Err(err) => self.fail(format!("Échec du chargement : {err}")),
        }
    }

    fn select_sheet(&mut self, id: NodeId) {
        let is_sheet = self
            .snapshot
            .tree
            .node(id)
            .is_some_and(|node| node.kind().is_worksheet());
        if is_sheet {
            self.selected_sheet = Some(id);
            self.editing = None;
            self.status = None;
        } else {
            self.fail(format!("{id} n'est pas une feuille de ce projet"));
        }
    }

    fn commit_cell(&mut self, column: NodeId, row: usize, text: String) {
        let Some(kind) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(Column::column_type)
        else {
            self.fail(format!("{column} n'est pas une colonne de ce projet"));
            return;
        };
        let values = match kind {
            ColumnType::Numeric | ColumnType::DateTime => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    // Blanc = valeur manquante, pas zéro : c'est la façon
                    // d'effacer une cellule numérique.
                    ValueBlock::Float(vec![f64::NAN])
                } else {
                    match trimmed.parse::<f64>() {
                        Ok(value) => ValueBlock::Float(vec![value]),
                        Err(_) => {
                            self.fail(format!("« {trimmed} » n'est pas un nombre"));
                            return;
                        }
                    }
                }
            }
            // En texte, `""` est la valeur manquante ; le modèle distingue
            // `None` de `Some("")`, mais rien dans une cellule de saisie ne
            // permet de taper l'un plutôt que l'autre. Le banc choisit `None`.
            ColumnType::Text => {
                if text.is_empty() {
                    ValueBlock::Text(vec![None])
                } else {
                    ValueBlock::Text(vec![Some(text)])
                }
            }
        };
        self.execute(Command::SetCells {
            column,
            start: row,
            values,
        });
    }

    fn add_column(&mut self) {
        let Some(sheet) = self.selected_sheet else {
            self.fail("Aucune feuille sélectionnée".to_string());
            return;
        };
        self.execute(Command::AddColumn {
            sheet,
            index: None,
            name: None,
            column: Column::new(ColumnType::Numeric),
        });
    }

    /// Insertion (`insert`) ou suppression de lignes sur la feuille affichée.
    fn rows(&mut self, at: usize, count: usize, insert: bool) {
        let Some(sheet) = self.selected_sheet else {
            self.fail("Aucune feuille sélectionnée".to_string());
            return;
        };
        let command = if insert {
            Command::InsertRows { sheet, at, count }
        } else {
            Command::DeleteRows { sheet, at, count }
        };
        self.execute(command);
    }

    /// Refuse d'exécuter un script : ce binaire n'embarque pas le pont Python
    /// (critère 1 de `tests/shell.rs`). N'envoie aucune commande, la
    /// révision ne bouge pas.
    #[cfg(not(feature = "python"))]
    fn run_script(&mut self) {
        self.fail(
            "Impossible d'exécuter un script : ce binaire est compilé sans le pont Python \
             (feature `python` absente)."
                .to_string(),
        );
    }

    /// Démarre une exécution de script sur un fil détaché, en réutilisant le
    /// moteur s'il existe déjà ou en le créant paresseusement sinon — la
    /// création comme l'exécution se font **hors du fil appelant**, pour que
    /// même le tout premier script (coût de démarrage de l'interpréteur
    /// compris) ne fige jamais la fenêtre. Le résultat n'est récupéré que par
    /// [`AppState::poll_script`].
    #[cfg(feature = "python")]
    fn run_script(&mut self) {
        if self.pending_script.is_some() {
            self.fail("Un script est déjà en cours d'exécution.".to_string());
            return;
        }
        let Some(python) = self.python.clone() else {
            self.fail(
                "Aucun interpréteur Python n'a été localisé pour ce binaire \
                 (PLOTYX_PYTHON_HOME, ou dist/.../lib/python)."
                    .to_string(),
            );
            return;
        };
        let existing_engine = self.engine.clone();
        let handle = self.handle.clone();
        let source = self.script_source.clone();
        let (reply_tx, reply_rx) = mpsc::channel();

        let spawned = thread::Builder::new()
            .name("plotyx-app-script".to_owned())
            .spawn(move || {
                let engine = match existing_engine {
                    Some(engine) => Ok(engine),
                    None => ScriptEngine::spawn(handle, python)
                        .map(Arc::new)
                        .map_err(|err| err.to_string()),
                };
                let reply = match engine {
                    Ok(engine) => {
                        let outcome = engine.run(&source).map_err(|err| err.to_string());
                        (Some(engine), outcome)
                    }
                    Err(message) => (None, Err(message)),
                };
                // Personne à prévenir si `AppState` a disparu entre-temps
                // (fenêtre fermée pendant l'exécution) : le canal est alors
                // simplement lâché.
                let _ = reply_tx.send(reply);
            });

        match spawned {
            Ok(_detached) => {
                self.pending_script = Some(reply_rx);
                self.notice("Script en cours d'exécution…".to_string());
            }
            Err(err) => self.fail(format!("Impossible de démarrer le fil de script : {err}")),
        }
    }

    /// Applique le résultat d'un script terminé : mémorise sa sortie, pose un
    /// message de statut **neutre** — une exception de script n'est pas une
    /// erreur d'application, sa trace se lit dans la sortie, pas en rouge
    /// dans la barre d'état — puis rafraîchit l'instantané (c'est le piège de
    /// cette tâche : sans ce geste, la grille resterait figée alors que le
    /// modèle a changé).
    ///
    /// Le message est posé **avant** `refresh`, comme dans [`AppState::
    /// execute`] : si le rafraîchissement échoue (acteur injoignable), il
    /// écrase ce message neutre par l'erreur — plus grave, elle doit
    /// l'emporter, pas l'inverse.
    #[cfg(feature = "python")]
    fn finish_script_run(&mut self, outcome: ScriptOutcome) {
        let interrupted = outcome.interrupted;
        let failed = outcome.error.is_some();
        self.script_output = Some(ScriptOutput::from(outcome));
        self.notice(
            if interrupted {
                "Script interrompu."
            } else if failed {
                "Script terminé avec une exception : voir la sortie."
            } else {
                "Script exécuté."
            }
            .to_string(),
        );
        self.refresh();
    }

    /// Refuse d'interrompre un script : ce binaire n'embarque pas le pont
    /// Python.
    #[cfg(not(feature = "python"))]
    fn interrupt_script(&mut self) {
        self.fail(
            "Impossible d'interrompre un script : ce binaire est compilé sans le pont Python \
             (feature `python` absente)."
                .to_string(),
        );
    }

    /// Demande l'arrêt du script en cours. Ne bloque pas : pose un drapeau
    /// que le fil détaché consulte à son prochain point de contrôle (voir
    /// `plotyx_script::ScriptEngine::interrupt`). Sans effet s'il n'y a pas
    /// de script en cours.
    #[cfg(feature = "python")]
    fn interrupt_script(&mut self) {
        // L'`Arc` est cloné puis le prêt relâché avant `notice` : les deux
        // champs de `self` ne peuvent pas être empruntés en même temps que
        // `self.notice` a besoin d'un emprunt exclusif.
        let engine = self.engine.clone();
        match engine {
            Some(engine) if self.pending_script.is_some() => {
                engine.interrupt();
                self.notice("Interruption demandée.".to_string());
            }
            _ => self.notice("Aucun script en cours à interrompre.".to_string()),
        }
    }
}
