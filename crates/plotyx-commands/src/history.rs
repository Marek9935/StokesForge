//! Pile d'historique à deux natures et exécution des commandes.
//!
//! # Les deux natures
//!
//! - **structure** (arbre, noms, ordre, ajout/suppression de nœuds) →
//!   *snapshot persistant*. L'entrée retient `before` et `after`, deux
//!   `Arc<Tree>`. Annuler, c'est `replace_tree(before)`. Les deux arbres
//!   partagent physiquement tout ce qui n'a pas changé : garder les deux coûte
//!   un pointeur. L'arbre `before` retient les nœuds supprimés — il n'y a rien
//!   à libérer explicitement, et un nœud restauré revient sous son
//!   **identifiant d'origine** (`Project::replace_tree` ne fait pas reculer
//!   l'allocateur d'identifiants).
//! - **données** (contenu des colonnes) → *delta*. L'entrée retient les
//!   anciennes **et** les nouvelles valeurs des seules cellules écrites, et les
//!   rejoue dans l'arbre **courant**. Un snapshot retiendrait ici le buffer
//!   entier de la colonne (8 Mo pour un million de points) à chaque frappe.
//!
//! # Invariant LIFO
//!
//! Une entrée structurelle ne peut être annulée que si toutes les entrées
//! postérieures l'ont déjà été : restaurer un arbre entier écrase tout ce qui a
//! été écrit après lui. La pile le garantit par construction, et **dans une
//! macro les entrées s'annulent strictement en ordre inverse d'insertion**,
//! quelles que soient leurs natures — c'est ce qui fait tenir l'hybride.
//!
//! # Deux limites, la plus contraignante des deux gagnant
//!
//! Une limite comptée en entrées (`limit`) ne voit pas le motif mesuré par
//! `tests/memory_pinning.rs` : en alternant une opération de structure et
//! une écriture sur une même grosse colonne, chaque entrée structurelle
//! épingle une version complète du buffer que l'écriture suivante remplace —
//! cent renommages ne pèsent rien, une seule suppression de classeur pèse des
//! centaines de méga-octets. `byte_limit` borne directement les octets de
//! charge utile que l'historique est seul à maintenir vivants (voir
//! [`History::retained_bytes`] et [`History::evict`] pour la méthode et les
//! règles d'éviction).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use plotyx_core::{ColumnType, ColumnValues};
use plotyx_core::{CoreError, CoreResult, NodeId, Project, Tree, ValueBlock};

use crate::command::Command;
use crate::error::{CommandError, CommandResult};

/// Delta d'une entrée de données : tout ce qu'il faut pour annuler **et**
/// rétablir, sans retenir d'arbre.
#[derive(Debug, Clone, PartialEq)]
pub enum DataDelta {
    /// Écriture de cellules : `before` (+ `before_length`) pour annuler,
    /// `after` pour rétablir.
    Cells {
        /// Colonne écrite.
        column: NodeId,
        /// Première ligne écrite.
        start: usize,
        /// Valeurs écrasées (partie recouvrant des lignes qui existaient).
        before: ValueBlock,
        /// Longueur de la colonne avant l'écriture.
        before_length: usize,
        /// Valeurs écrites.
        after: ValueBlock,
    },
    /// Insertion de lignes : les colonnes **effectivement** allongées, relevées
    /// avant l'application.
    InsertRows {
        /// Colonnes qui possédaient une ligne `at` au moment de l'insertion.
        columns: Vec<NodeId>,
        /// Ligne d'insertion.
        at: usize,
        /// Nombre de lignes insérées.
        count: usize,
    },
    /// Suppression de lignes : les valeurs retirées, par colonne touchée.
    DeleteRows {
        /// Feuille visée (conservée pour l'inspection ; l'annulation opère
        /// colonne par colonne).
        sheet: NodeId,
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes demandé (l'annulation utilise la taille réellement
        /// retirée, qui peut être plus petite en fin de colonne).
        count: usize,
        /// Valeurs retirées, par colonne touchée, dans l'ordre de la feuille.
        removed: Vec<(NodeId, ValueBlock)>,
    },
    /// Seul cas de snapshot de données assumé : la conversion de type est
    /// destructrice dans les deux sens (« 3,5 » → 3.5 → « 3.5 »), donc ni
    /// l'ancien ni le nouveau contenu ne se recalculent depuis l'autre.
    ColumnType {
        /// Colonne convertie.
        column: NodeId,
        /// Type et valeurs d'avant.
        before: (ColumnType, ColumnValues),
        /// Type et valeurs d'après.
        after: (ColumnType, ColumnValues),
    },
}

/// Une entrée d'historique : un snapshot de structure, un delta de données, ou
/// un groupe (macro) qui s'annule d'un bloc.
#[derive(Debug, Clone)]
pub enum Entry {
    /// Snapshot de structure.
    Structural {
        /// Arbre d'avant la commande.
        before: Arc<Tree>,
        /// Arbre d'après la commande.
        after: Arc<Tree>,
        /// Libellé d'historique.
        label: String,
    },
    /// Delta de données.
    Data {
        /// Ce qu'il faut rejouer dans l'arbre courant.
        delta: DataDelta,
        /// Libellé d'historique.
        label: String,
    },
    /// Groupe d'entrées, annulé en ordre inverse et rétabli en ordre direct.
    Macro {
        /// Entrées dans leur ordre d'application.
        entries: Vec<Entry>,
        /// Libellé du groupe.
        label: String,
    },
}

impl Entry {
    /// Libellé affichable de l'entrée (« Annuler : ... »).
    pub fn label(&self) -> &str {
        match self {
            Entry::Structural { label, .. }
            | Entry::Data { label, .. }
            | Entry::Macro { label, .. } => label,
        }
    }
}

/// Empreinte mémoire d'une entrée : pour chaque buffer de colonne qu'elle
/// retient — directement (un delta de données porte ses propres valeurs) ou
/// par l'arbre qu'elle capture (un instantané de structure porte les
/// colonnes de tout l'arbre) — l'adresse de son allocation et sa taille en
/// octets.
///
/// L'adresse ne sert qu'à identifier un buffer, jamais à le déréférencer :
/// c'est ce qui permet de dédupliquer les buffers partagés entre plusieurs
/// entrées, ou entre une entrée et l'arbre courant, sans jamais lire la
/// mémoire qu'elle désigne.
///
/// Calculée une fois pour toutes au moment de l'empilement de l'entrée :
/// l'arbre étant immuable, cet ensemble ne change plus jamais ensuite (voir
/// [`footprint_of`]).
type Footprint = HashMap<usize, usize>;

/// Une entrée d'historique accompagnée de son empreinte mémoire.
///
/// Ce couplage vit à côté de [`Entry`] plutôt que dans ses variantes : c'est
/// un détail de comptabilité propre à `History` (il ne sert ni à annuler ni
/// à rétablir), et le garder hors de `Entry` laisse l'API publique de ce
/// type inchangée.
#[derive(Debug)]
struct Pinned {
    /// L'entrée elle-même — ce que `revert`/`reapply` consomment.
    entry: Entry,
    /// Son empreinte, calculée une fois pour toutes à l'empilement.
    footprint: Footprint,
}

/// Limite d'historique par défaut, en nombre d'entrées : la profondeur
/// d'annulation attendue, l'ordre de grandeur d'un tableur. Éviction
/// silencieuse de la plus ancienne (`questions-ouvertes.md`, question 7 —
/// recommandation appliquée, à confirmer).
pub const DEFAULT_LIMIT: usize = 100;

/// Limite d'historique par défaut, en octets de charge utile retenue
/// (512 Mio). Protège le motif que `DEFAULT_LIMIT` seul ne voit pas : voir
/// la section « Deux limites » en tête de module.
pub const DEFAULT_BYTE_LIMIT: usize = 512 * 1024 * 1024;

/// Pile d'historique à deux natures.
///
/// Elle **ne possède pas** le `Project` : chaque opération le reçoit en
/// `&mut`. C'est l'acteur de `plotyx-session` qui détient l'un et l'autre, ce
/// qui évite d'enfermer le modèle dans l'historique.
#[derive(Debug)]
pub struct History {
    /// Entrées annulables, la plus récente en fin, chacune accompagnée de
    /// l'empreinte mémoire calculée à son empilement.
    undo: Vec<Pinned>,
    /// Entrées rétablissables, la prochaine en fin.
    redo: Vec<Pinned>,
    /// Nombre maximal d'entrées de premier niveau ; `0` = illimité.
    limit: usize,
    /// Octets maximaux de charge utile retenue ; `0` = illimité.
    byte_limit: usize,
    /// Octets de charge utile retenus, tels que la dernière évaluation (le
    /// dernier empilement) les a établis — voir [`History::retained_bytes`].
    retained_bytes: usize,
    /// Macros ouvertes, de la plus externe à la plus interne : libellé et
    /// entrées déjà empreintées, accumulées avant de devenir une seule
    /// entrée `Macro` à la fermeture.
    macros: Vec<(String, Vec<Pinned>)>,
    /// Pile de rétablissement telle qu'elle était avant l'ouverture de la
    /// macro la plus externe (`None` hors macro). Sauvegardée à
    /// `begin_macro`, elle est restaurée si cette macro est avortée — ou
    /// close vide — et jetée si elle aboutit avec au moins une entrée : une
    /// macro avortée ne doit laisser aucune trace, y compris sur le redo.
    saved_redo: Option<Vec<Pinned>>,
}

impl History {
    /// Pile neuve, aux deux limites par défaut ([`DEFAULT_LIMIT`] entrées,
    /// [`DEFAULT_BYTE_LIMIT`] octets retenus).
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_LIMIT, DEFAULT_BYTE_LIMIT)
    }

    /// Pile neuve limitée à `limit` entrées (`0` = illimité) ; garde
    /// [`DEFAULT_BYTE_LIMIT`] pour la limite en octets.
    pub fn with_limit(limit: usize) -> Self {
        Self::with_limits(limit, DEFAULT_BYTE_LIMIT)
    }

    /// Pile neuve avec les deux limites explicites. `0` = illimité, pour
    /// l'une comme pour l'autre. La plus contraignante des deux gagne :
    /// après chaque empilement, l'éviction continue tant que l'une ou
    /// l'autre est dépassée (voir [`History::evict`]).
    pub fn with_limits(limit: usize, byte_limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
            byte_limit,
            retained_bytes: 0,
            macros: Vec::new(),
            saved_redo: None,
        }
    }

    /// Limite courante en nombre d'entrées ; `0` = illimité.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Limite courante en octets de charge utile retenue ; `0` = illimitée.
    pub fn byte_limit(&self) -> usize {
        self.byte_limit
    }

    /// Octets de charge utile que l'historique est seul à maintenir vivants,
    /// tels que la dernière évaluation les a établis.
    ///
    /// L'évaluation a lieu à chaque empilement (voir [`History::evict`]) :
    /// `undo`/`redo`/`begin_macro`/`abort_macro` seuls ne la redéclenchent
    /// pas (aucun d'eux n'ajoute ni ne retire d'entrée du total retenu — ils
    /// ne font que déplacer des entrées déjà comptées entre les deux piles,
    /// ou les rejouer dans le projet). Une conséquence : juste après une
    /// série d'annulations, la valeur rendue peut être **légèrement en
    /// retard** sur la réalité (l'arbre courant a changé, donc ce qu'il
    /// « couvre » gratuitement aussi) jusqu'à la commande suivante.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Change la limite en nombre d'entrées. `Err(MacroOpen)` en macro ;
    /// `Err(StackNotEmpty)` si `undo` ou `redo` n'est pas vide — changer la
    /// limite au milieu d'un historique demanderait de décider quoi évincer,
    /// et cette décision n'appartient pas à la pile.
    pub fn set_limit(&mut self, limit: usize) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("set_limit"));
        }
        if !self.undo.is_empty() || !self.redo.is_empty() {
            return Err(CommandError::StackNotEmpty);
        }
        self.limit = limit;
        Ok(())
    }

    /// Change la limite en octets de charge utile retenue. Mêmes règles que
    /// [`History::set_limit`] : `Err(MacroOpen)` en macro, `Err(StackNotEmpty)`
    /// si une pile n'est pas vide.
    pub fn set_byte_limit(&mut self, bytes: usize) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("set_byte_limit"));
        }
        if !self.undo.is_empty() || !self.redo.is_empty() {
            return Err(CommandError::StackNotEmpty);
        }
        self.byte_limit = bytes;
        Ok(())
    }

    /// Applique `command` puis l'empile (ou l'ajoute à la macro ouverte).
    ///
    /// **Application avant empilement** : en cas d'`Err`, ni l'arbre ni la pile
    /// n'ont changé — l'arbre d'avant est remis par sécurité, sans se reposer
    /// sur la promesse d'atomicité des primitives du cœur.
    ///
    /// En cas de succès la pile de rétablissement est vidée, **y compris en
    /// macro** : dès qu'une nouvelle mutation est appliquée, ce qui avait été
    /// annulé n'a plus de branche où revenir.
    pub fn execute(&mut self, project: &mut Project, command: Command) -> CommandResult<()> {
        let before = Arc::clone(project.tree());
        let label = command.label(&before);
        match Self::apply(project, command, &before, label) {
            Ok(entry) => {
                self.push(entry, project);
                Ok(())
            }
            Err(err) => {
                // Les primitives de `Project` promettent de ne rien changer en
                // cas d'échec, mais rétablir `before` est gratuit (un pointeur)
                // et rend la garantie indépendante du cœur.
                project.replace_tree(before);
                Err(err)
            }
        }
    }

    /// Annule l'entrée du sommet. `Ok(false)` si la pile est vide — ce n'est
    /// pas une erreur. `Err(MacroOpen)` si une macro est ouverte.
    ///
    /// Une erreur rendue pendant l'annulation d'une entrée `Data` ne devrait
    /// pas arriver (l'invariant LIFO garantit que l'état est celui attendu).
    /// Si elle survient, la seule garantie est que **l'entrée n'a pas été
    /// déplacée entre les piles** : l'arbre, lui, peut être à moitié annulé.
    pub fn undo(&mut self, project: &mut Project) -> CommandResult<bool> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("undo"));
        }
        let Some(pinned) = self.undo.last() else {
            return Ok(false);
        };
        Self::revert(project, &pinned.entry)?;
        if let Some(pinned) = self.undo.pop() {
            self.redo.push(pinned);
        }
        Ok(true)
    }

    /// Rétablit l'entrée annulée la plus récente. Mêmes conventions que
    /// [`History::undo`].
    pub fn redo(&mut self, project: &mut Project) -> CommandResult<bool> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("redo"));
        }
        let Some(pinned) = self.redo.last() else {
            return Ok(false);
        };
        Self::reapply(project, &pinned.entry)?;
        if let Some(pinned) = self.redo.pop() {
            self.undo.push(pinned);
        }
        Ok(true)
    }

    /// Vrai s'il y a quelque chose à annuler.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Vrai s'il y a quelque chose à rétablir.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Libellé de la prochaine annulation.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|pinned| pinned.entry.label())
    }

    /// Libellé du prochain rétablissement.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|pinned| pinned.entry.label())
    }

    /// Nombre d'entrées annulables (une macro compte pour une).
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// Vrai si rien n'est annulable.
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Nombre d'entrées rétablissables.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Ouvre une macro : les commandes suivantes s'y accumulent au lieu d'être
    /// empilées. Les macros s'imbriquent.
    ///
    /// À l'ouverture de la macro **la plus externe** (`macro_depth() == 0 →
    /// 1`), la pile de rétablissement courante est mise de côté : elle
    /// pourra être restaurée si cette macro est avortée, voir
    /// [`History::abort_macro`].
    pub fn begin_macro(&mut self, label: &str) {
        if self.macros.is_empty() {
            self.saved_redo = Some(std::mem::take(&mut self.redo));
        }
        self.macros.push((label.to_string(), Vec::new()));
    }

    /// Ferme la macro courante : elle devient **une seule** entrée `Macro`,
    /// empilée normalement (ou ajoutée à la macro parente s'il y en a une).
    /// Une macro vide ne laisse **aucune** entrée. `Err(NotInMacro)` sinon.
    ///
    /// L'empreinte de l'entrée `Macro` est l'union de celles, déjà connues,
    /// de ses entrées — une fusion de tables de hachage, jamais un nouveau
    /// parcours d'arbre.
    ///
    /// À la fermeture de la macro **la plus externe**, la pile de
    /// rétablissement sauvegardée à l'ouverture est jetée : le redo est bel
    /// et bien perdu quand la macro aboutit avec au moins une entrée, comme
    /// pour toute commande. **Sauf** si la macro est vide (aucune entrée
    /// appliquée) : elle n'a rien changé, la sauvegarde est alors restaurée.
    pub fn end_macro(&mut self, project: &mut Project) -> CommandResult<()> {
        let Some((label, entries)) = self.macros.pop() else {
            return Err(CommandError::NotInMacro);
        };
        let is_outermost = self.macros.is_empty();
        if entries.is_empty() {
            if is_outermost {
                self.restore_saved_redo();
            }
            return Ok(());
        }
        if is_outermost {
            self.saved_redo = None;
        }
        let footprint = union_footprint(entries.iter().map(|pinned| &pinned.footprint));
        let entry = Entry::Macro {
            entries: entries.into_iter().map(|pinned| pinned.entry).collect(),
            label,
        };
        self.push_pinned(Pinned { entry, footprint }, project);
        Ok(())
    }

    /// Annule, en ordre inverse, tout ce que la macro courante a appliqué, puis
    /// la jette. Une macro imbriquée avortée ne touche **pas** ce que la macro
    /// parente a déjà appliqué. `Err(NotInMacro)` sinon.
    ///
    /// Si une annulation échoue en cours de route, l'erreur remonte et la macro
    /// est déjà retirée : l'arbre peut alors être à moitié défait (même
    /// réserve que [`History::undo`]).
    ///
    /// Une macro avortée ne laisse **aucune** trace, y compris sur le redo :
    /// quand c'est la macro **la plus externe** qui est avortée, la pile de
    /// rétablissement sauvegardée à `begin_macro` est restaurée une fois ses
    /// entrées annulées — ce que la macro avait pu rendre rétablissable
    /// entre-temps (chaque `execute` vide le redo, même en macro) disparaît,
    /// et ce qui l'était **avant** la macro revient.
    pub fn abort_macro(&mut self, project: &mut Project) -> CommandResult<()> {
        let Some((_, entries)) = self.macros.pop() else {
            return Err(CommandError::NotInMacro);
        };
        for pinned in entries.iter().rev() {
            Self::revert(project, &pinned.entry)?;
        }
        if self.macros.is_empty() {
            self.restore_saved_redo();
        }
        Ok(())
    }

    /// Vrai si au moins une macro est ouverte.
    pub fn in_macro(&self) -> bool {
        !self.macros.is_empty()
    }

    /// Nombre de macros ouvertes (imbrication).
    pub fn macro_depth(&self) -> usize {
        self.macros.len()
    }

    /// Vide les deux piles. `Err(MacroOpen)` en macro : jeter l'historique
    /// sous une macro ouverte laisserait un groupe sans rien où atterrir.
    pub fn clear(&mut self) -> CommandResult<()> {
        if self.in_macro() {
            return Err(CommandError::MacroOpen("clear"));
        }
        self.undo.clear();
        self.redo.clear();
        // Exact, pas une simple économie de recalcul : l'union de deux piles
        // vides est vide.
        self.retained_bytes = 0;
        Ok(())
    }

    // -- interne --

    /// Restaure la pile de rétablissement sauvegardée à l'ouverture de la
    /// macro la plus externe, si `saved_redo` en retient une. Appelée à
    /// `abort_macro` et à `end_macro` d'une macro vide, toutes deux au niveau
    /// le plus externe.
    fn restore_saved_redo(&mut self) {
        if let Some(saved) = self.saved_redo.take() {
            self.redo = saved;
        }
    }

    /// Empile une entrée fraîchement appliquée (hors macro) : la pile de
    /// rétablissement est vidée, l'empreinte de l'entrée est calculée une
    /// fois pour toutes, puis l'entrée rejoint la macro ouverte ou la pile
    /// principale.
    fn push(&mut self, entry: Entry, project: &Project) {
        self.redo.clear();
        let footprint = footprint_of(&entry);
        self.push_pinned(Pinned { entry, footprint }, project);
    }

    /// Range une entrée déjà empreintée dans la macro ouverte, ou au sommet
    /// de la pile principale (avec éviction). **Rien ne change au milieu
    /// d'une macro ouverte** : ni éviction, ni recalcul des octets retenus —
    /// seule la fermeture de la macro la plus externe qui aboutit avec au
    /// moins une entrée (voir [`History::end_macro`]) rejoint la pile
    /// principale et déclenche [`History::evict`].
    fn push_pinned(&mut self, pinned: Pinned, project: &Project) {
        if let Some((_, entries)) = self.macros.last_mut() {
            entries.push(pinned);
        } else {
            self.undo.push(pinned);
            self.evict(project);
        }
    }

    /// Retire silencieusement les entrées les plus anciennes de `undo` tant
    /// que l'une des deux limites (nombre d'entrées, octets retenus) est
    /// dépassée, et recalcule [`History::retained_bytes`] à chaque retrait
    /// (voir [`History::recompute_retained_bytes`]).
    ///
    /// **La dernière entrée restante n'est jamais évincée**, même si elle
    /// dépasse à elle seule la limite en octets : une opération lourde doit
    /// rester annulable juste après avoir été faite, sinon la limite en
    /// octets produit exactement le désagrément qu'elle cherche à éviter
    /// (plus rien à annuler après la commande qui vient de s'exécuter). Une
    /// pile à une entrée peut donc rester durablement au-dessus du plafond
    /// en octets si cette entrée seule le dépasse déjà.
    ///
    /// L'éviction ne touche **jamais** la pile de rétablissement (`redo`) :
    /// elle est vidée par le prochain empilement (voir [`History::push`]),
    /// comme aujourd'hui pour la limite en nombre d'entrées. Le total retenu
    /// est l'union des deux piles (voir
    /// [`History::recompute_retained_bytes`]), donc **une longue branche de
    /// rétablissement peut maintenir le total au-dessus du plafond en
    /// octets jusqu'à la commande suivante** — ce n'est pas un oubli : c'est
    /// le prix de ne jamais faire disparaître une branche de rétablissement
    /// autrement qu'à la prochaine mutation.
    fn evict(&mut self, project: &Project) {
        self.recompute_retained_bytes(project);
        while self.undo.len() > 1 {
            let over_count = self.limit != 0 && self.undo.len() > self.limit;
            let over_bytes = self.byte_limit != 0 && self.retained_bytes > self.byte_limit;
            if !over_count && !over_bytes {
                break;
            }
            // La pile reste courte (une centaine d'entrées par défaut) : le
            // décalage d'un `Vec` est sans conséquence face au coût d'un
            // `VecDeque` partout ailleurs.
            self.undo.remove(0);
            self.recompute_retained_bytes(project);
        }
    }

    /// Recalcule [`History::retained_bytes`] : l'union des empreintes déjà
    /// mémorisées par chaque entrée des deux piles (`undo` et `redo` — une
    /// entrée de rétablissement maintient ses buffers vivants tout autant
    /// qu'une entrée annulable), moins les buffers présents dans l'arbre
    /// **courant** (un buffer que l'arbre courant utilise encore serait là
    /// de toute façon : ce n'est pas un coût de l'historique).
    ///
    /// Coûte un parcours de l'arbre courant — un seul, pour connaître ses
    /// buffers, jamais ceux des entrées historiques — plus une fusion de
    /// tables de hachage à l'échelle du nombre total d'entrées × colonnes
    /// par entrée : quelques milliers d'éléments au pire, pas un nouveau
    /// parcours des arbres retenus par les entrées, déjà mémorisés une fois
    /// pour toutes à leur empilement (voir [`footprint_of`]).
    fn recompute_retained_bytes(&mut self, project: &Project) {
        let mut union = Footprint::new();
        for pinned in self.undo.iter().chain(self.redo.iter()) {
            union.extend(pinned.footprint.iter().map(|(&ptr, &bytes)| (ptr, bytes)));
        }
        let current = current_tree_pointers(project.tree());
        self.retained_bytes = union
            .into_iter()
            .filter(|(ptr, _)| !current.contains(ptr))
            .map(|(_, bytes)| bytes)
            .sum();
    }

    /// Applique une commande et construit son entrée d'historique.
    fn apply(
        project: &mut Project,
        command: Command,
        before: &Arc<Tree>,
        label: String,
    ) -> CommandResult<Entry> {
        match command {
            Command::SetCells {
                column,
                start,
                values,
            } => {
                // Le bloc écrit est cloné avant application : c'est le `after`
                // du delta, ce qu'il faudra réécrire pour rétablir.
                let after = values.clone();
                let delta = project.set_values(column, start, values)?;
                Ok(Entry::Data {
                    delta: DataDelta::Cells {
                        column,
                        start,
                        before: delta.previous,
                        before_length: delta.previous_length,
                        after,
                    },
                    label,
                })
            }
            Command::AddColumn {
                sheet,
                index,
                name,
                column,
            } => {
                project.add_column(sheet, index, name.as_deref(), column)?;
                Ok(structural(before, project, label))
            }
            Command::DeleteColumn { column } => {
                // Vérifié **avant** d'appeler `remove`, qui accepterait
                // n'importe quel nœud : une commande nommée « supprimer une
                // colonne » ne doit pas pouvoir emporter une feuille entière.
                let node = before.get(column)?;
                if !node.kind().is_column() {
                    return Err(CoreError::Structure(format!(
                        "{column} n'est pas une colonne mais {} : suppression de colonne refusée",
                        node.kind().with_article()
                    ))
                    .into());
                }
                project.remove(column)?;
                Ok(structural(before, project, label))
            }
            Command::Rename { id, name } => {
                project.rename(id, &name)?;
                Ok(structural(before, project, label))
            }
            Command::Move {
                id,
                new_parent,
                index,
            } => {
                project.move_node(id, new_parent, index)?;
                Ok(structural(before, project, label))
            }
            Command::SetColumnType {
                column,
                column_type,
            } => {
                let before_state = project.set_column_type(column, column_type)?;
                let after_state = column_state(project, column)?;
                Ok(Entry::Data {
                    delta: DataDelta::ColumnType {
                        column,
                        before: before_state,
                        after: after_state,
                    },
                    label,
                })
            }
            Command::InsertRows { sheet, at, count } => {
                // Les colonnes effectivement allongées sont rendues par
                // `Project::insert_rows` lui-même — la règle « `at < len` »
                // ne vit qu'à cet unique endroit, dans le cœur — et
                // l'annulation les reprend une à une (`delete_column_rows`)
                // plutôt que de rappeler `delete_rows` au niveau feuille.
                // C'est ce qui rend l'annulation symétrique de celle de
                // `DeleteRows`, où le niveau feuille est de toute façon
                // impossible (une colonne vidée jusqu'à sa fin n'a plus de
                // ligne `at`).
                let columns = project.insert_rows(sheet, at, count)?;
                Ok(Entry::Data {
                    delta: DataDelta::InsertRows { columns, at, count },
                    label,
                })
            }
            Command::DeleteRows { sheet, at, count } => {
                let removed = project.delete_rows(sheet, at, count)?;
                Ok(Entry::Data {
                    delta: DataDelta::DeleteRows {
                        sheet,
                        at,
                        count,
                        removed,
                    },
                    label,
                })
            }
        }
    }

    /// Annule une entrée. Une macro s'annule en ordre **inverse**.
    fn revert(project: &mut Project, entry: &Entry) -> CommandResult<()> {
        match entry {
            Entry::Structural { before, .. } => {
                project.replace_tree(Arc::clone(before));
                Ok(())
            }
            Entry::Data { delta, .. } => Self::revert_delta(project, delta),
            Entry::Macro { entries, .. } => {
                for entry in entries.iter().rev() {
                    Self::revert(project, entry)?;
                }
                Ok(())
            }
        }
    }

    /// Rétablit une entrée. Une macro se rejoue en ordre **direct**.
    fn reapply(project: &mut Project, entry: &Entry) -> CommandResult<()> {
        match entry {
            Entry::Structural { after, .. } => {
                project.replace_tree(Arc::clone(after));
                Ok(())
            }
            Entry::Data { delta, .. } => Self::reapply_delta(project, delta),
            Entry::Macro { entries, .. } => {
                for entry in entries {
                    Self::reapply(project, entry)?;
                }
                Ok(())
            }
        }
    }

    /// Rejoue un delta de données à l'envers, dans l'arbre courant.
    fn revert_delta(project: &mut Project, delta: &DataDelta) -> CommandResult<()> {
        match delta {
            DataDelta::Cells {
                column,
                start,
                before,
                before_length,
                ..
            } => {
                // Deux temps : réécrire ce qui a été écrasé, puis rendre à la
                // colonne sa longueur (une écriture au-delà de la fin l'avait
                // allongée, et `set_values` ne raccourcit jamais).
                project.set_values(*column, *start, before.clone())?;
                project.set_length(*column, *before_length)?;
                Ok(())
            }
            DataDelta::InsertRows { columns, at, count } => {
                for column in columns {
                    project.delete_column_rows(*column, *at, *count)?;
                }
                Ok(())
            }
            DataDelta::DeleteRows { at, removed, .. } => {
                for (column, block) in removed {
                    project.insert_column_rows(*column, *at, block.len())?;
                    project.set_values(*column, *at, block.clone())?;
                }
                Ok(())
            }
            DataDelta::ColumnType { column, before, .. } => {
                project.restore_column(*column, before.0, before.1.clone())?;
                Ok(())
            }
        }
    }

    /// Rejoue un delta de données à l'endroit, dans l'arbre courant.
    fn reapply_delta(project: &mut Project, delta: &DataDelta) -> CommandResult<()> {
        match delta {
            DataDelta::Cells {
                column,
                start,
                after,
                ..
            } => {
                // La longueur suit l'écriture : pas de `set_length` ici.
                project.set_values(*column, *start, after.clone())?;
                Ok(())
            }
            DataDelta::InsertRows { columns, at, count } => {
                for column in columns {
                    project.insert_column_rows(*column, *at, *count)?;
                }
                Ok(())
            }
            DataDelta::DeleteRows { at, removed, .. } => {
                // Colonne par colonne, et non `delete_rows` au niveau feuille :
                // seules les colonnes réellement touchées la première fois
                // doivent l'être à nouveau.
                for (column, block) in removed {
                    project.delete_column_rows(*column, *at, block.len())?;
                }
                Ok(())
            }
            DataDelta::ColumnType { column, after, .. } => {
                project.restore_column(*column, after.0, after.1.clone())?;
                Ok(())
            }
        }
    }
}

impl Default for History {
    /// Pile neuve aux deux limites par défaut ([`DEFAULT_LIMIT`],
    /// [`DEFAULT_BYTE_LIMIT`]).
    fn default() -> Self {
        Self::new()
    }
}

/// Entrée structurelle : l'arbre d'avant et celui d'après, deux pointeurs.
fn structural(before: &Arc<Tree>, project: &Project, label: String) -> Entry {
    Entry::Structural {
        before: Arc::clone(before),
        after: Arc::clone(project.tree()),
        label,
    }
}

/// Type et valeurs actuels d'une colonne (snapshot d'un changement de type).
fn column_state(project: &Project, column: NodeId) -> CoreResult<(ColumnType, ColumnValues)> {
    let node = project.tree().get(column)?;
    let target = node
        .as_column()
        .ok_or_else(|| CoreError::Structure(format!("{column} n'est pas une colonne")))?;
    Ok((target.column_type(), target.values().clone()))
}

// ===========================================================================
// Empreinte mémoire des entrées
// ===========================================================================
//
// Le principe : chaque entrée connaît, une fois pour toutes calculée à son
// empilement, l'ensemble des buffers qu'elle retient — une collection de
// paires (adresse, octets), l'adresse ne servant que d'identité. Le total
// retenu par `History` (voir `History::recompute_retained_bytes`) est
// ensuite l'union de ces ensembles déjà connus, moins les buffers de l'arbre
// courant : une opération sur des tables de hachage, jamais un nouveau
// parcours des arbres retenus par les entrées.

/// Empreinte d'une entrée fraîchement construite par [`History::apply`] —
/// toujours `Structural` ou `Data`, jamais `Macro` (une macro n'est produite
/// que par [`History::end_macro`], à partir des empreintes déjà connues de
/// ses entrées, sans repasser par cette fonction).
fn footprint_of(entry: &Entry) -> Footprint {
    match entry {
        Entry::Structural { before, after, .. } => {
            // Structure : le buffer de chaque colonne de l'arbre d'avant
            // **et** de l'arbre d'après — un arbre immuable retient tous ses
            // nœuds, donc toutes ses colonnes.
            let mut out = Footprint::new();
            insert_tree_footprint(&mut out, before);
            insert_tree_footprint(&mut out, after);
            out
        }
        // Données : pas d'arbre retenu, donc pas de buffer de colonne — le
        // coût propre est celui des valeurs du delta (anciennes et
        // nouvelles), directement connu.
        Entry::Data { delta, .. } => data_delta_footprint(delta),
        Entry::Macro { entries, .. } => {
            // N'arrive pas en pratique (voir la doc de la fonction) : filet
            // de sécurité correct — l'union récursive des enfants — plutôt
            // qu'un panic.
            let mut out = Footprint::new();
            for child in entries {
                out.extend(footprint_of(child));
            }
            out
        }
    }
}

/// Union de plusieurs empreintes déjà connues (les entrées d'une macro qui
/// se referme, par exemple) : une fusion de tables de hachage, jamais un
/// nouveau parcours d'arbre.
fn union_footprint<'a>(footprints: impl Iterator<Item = &'a Footprint>) -> Footprint {
    let mut out = Footprint::new();
    for footprint in footprints {
        out.extend(footprint.iter().map(|(&ptr, &bytes)| (ptr, bytes)));
    }
    out
}

/// Ajoute à `out` le buffer de chaque colonne de `tree`.
fn insert_tree_footprint(out: &mut Footprint, tree: &Tree) {
    for (_, node) in tree.iter() {
        if let Some(column) = node.as_column() {
            insert_column_footprint(out, column.values());
        }
    }
}

/// Ajoute à `out` le buffer d'une colonne : adresse et taille en octets.
fn insert_column_footprint(out: &mut Footprint, values: &ColumnValues) {
    let (ptr, bytes) = column_footprint(values);
    out.insert(ptr, bytes);
}

/// Adresse (identité — jamais déréférencée) et taille en octets du buffer
/// d'une colonne. `8 × longueur` pour un buffer de flottants ; pour une
/// colonne texte, une estimation (voir [`text_buffer_bytes`]), le coût exact
/// d'allocation par `String` n'étant pas modélisable indépendamment de
/// l'allocateur.
fn column_footprint(values: &ColumnValues) -> (usize, usize) {
    match values {
        ColumnValues::Float(arc) => (
            Arc::as_ptr(arc) as usize,
            arc.len() * std::mem::size_of::<f64>(),
        ),
        ColumnValues::Text(arc) => (Arc::as_ptr(arc) as usize, text_buffer_bytes(arc)),
    }
}

/// Adresses des buffers de colonnes présents dans `tree` — le côté droit de
/// la soustraction : un buffer que l'arbre courant utilise encore n'est pas
/// un coût de l'historique, il serait là de toute façon.
fn current_tree_pointers(tree: &Tree) -> HashSet<usize> {
    tree.iter()
        .filter_map(|(_, node)| node.as_column())
        .map(|column| column_footprint(column.values()).0)
        .collect()
}

/// Empreinte d'un delta de données : pas d'arbre retenu, donc pas de buffer
/// de colonne partagé — seules les valeurs propres au delta comptent
/// (anciennes **et** nouvelles, quand elles existent), directement connues
/// sans qu'il soit besoin de les dédupliquer avec quoi que ce soit d'autre
/// (elles ne sont jamais partagées avec l'arbre).
fn data_delta_footprint(delta: &DataDelta) -> Footprint {
    let mut out = Footprint::new();
    match delta {
        DataDelta::Cells { before, after, .. } => {
            insert_block_footprint(&mut out, before);
            insert_block_footprint(&mut out, after);
        }
        DataDelta::InsertRows { .. } => {
            // Rien à retenir : `InsertRows` ne relève que les colonnes
            // touchées, la ligne et le compte — aucune valeur stockée (les
            // lignes insérées sont manquantes par construction).
        }
        DataDelta::DeleteRows { removed, .. } => {
            for (_, block) in removed {
                insert_block_footprint(&mut out, block);
            }
        }
        DataDelta::ColumnType { before, after, .. } => {
            // Seul delta de données qui porte un `ColumnValues` (donc un
            // `Arc` potentiellement partagé avec l'arbre courant, par
            // exemple juste après la conversion) plutôt qu'un `ValueBlock`
            // possédé en propre : même fonction que pour une colonne
            // d'arbre, pour que la déduplication par adresse joue son rôle.
            insert_column_footprint(&mut out, &before.1);
            insert_column_footprint(&mut out, &after.1);
        }
    }
    out
}

/// Ajoute à `out` le bloc `block` : l'adresse de son `Vec` (jamais partagée
/// avec l'arbre — un `ValueBlock` de delta est toujours possédé en propre,
/// donc jamais nette par soustraction contre l'arbre courant) et sa taille
/// en octets.
fn insert_block_footprint(out: &mut Footprint, block: &ValueBlock) {
    let (ptr, bytes) = block_footprint(block);
    out.insert(ptr, bytes);
}

/// Adresse et taille en octets d'un `ValueBlock`.
fn block_footprint(block: &ValueBlock) -> (usize, usize) {
    match block {
        ValueBlock::Float(values) => (
            values.as_ptr() as usize,
            values.len() * std::mem::size_of::<f64>(),
        ),
        ValueBlock::Text(values) => (values.as_ptr() as usize, text_buffer_bytes(values)),
    }
}

/// Coût fixe estimé par élément d'un buffer texte : approxime le stockage
/// d'un `Option<String>` dans le `Vec` (24 octets sur une plateforme 64
/// bits — `String` est `{ptr, len, cap}`, et `Option` s'y loge sans octet
/// supplémentaire grâce à la niche du pointeur). Le surcoût propre de
/// l'allocateur pour chaque `String` individuelle, variable d'un allocateur
/// à l'autre, n'est pas modélisé : c'est une estimation raisonnable, pas une
/// mesure exacte.
const TEXT_ELEMENT_OVERHEAD: usize = std::mem::size_of::<Option<String>>();

/// Estimation de la taille d'un buffer texte : somme des octets UTF-8 des
/// chaînes présentes, plus [`TEXT_ELEMENT_OVERHEAD`] par élément (présent ou
/// manquant).
fn text_buffer_bytes(values: &[Option<String>]) -> usize {
    values.iter().fold(0usize, |total, value| {
        total + TEXT_ELEMENT_OVERHEAD + value.as_deref().map_or(0, str::len)
    })
}
