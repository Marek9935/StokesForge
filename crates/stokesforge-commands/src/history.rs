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

use stokesforge_core::{ColumnMeta, ColumnType, ColumnValues, MixedCell, WindowMeta};
use stokesforge_core::{CoreError, CoreResult, NodeId, Project, Tree, ValueBlock};
use stokesforge_plot::{PlotSpec, SavedColors};

use crate::command::Command;
use crate::error::{CommandError, CommandResult};
use crate::label::{HistoryLabel, MacroKind};

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
        /// Graphes dont des séries « ligne » (`row = Some(r)`) ont suivi
        /// l'insertion : `(graphe, spec avant, spec après)`, capturés à
        /// l'application (invariant 8). Annuler remet l'avant, rétablir
        /// l'après — aucun inverse recalculé.
        plots: Vec<(NodeId, PlotSpec, PlotSpec)>,
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
        /// Graphes dont des séries « ligne » ont suivi la suppression :
        /// `(graphe, spec avant, spec après)`. Même règle que pour
        /// `InsertRows`.
        plots: Vec<(NodeId, PlotSpec, PlotSpec)>,
    },
    /// Suppression de cellules par colonnes explicites, indépendamment les
    /// unes des autres (touche Suppr sur une sélection qui ne couvre pas
    /// nécessairement toute la feuille) — le pendant de `DeleteRows` pour
    /// `Command::DeleteCells`. Mêmes règles d'annulation que `DeleteRows` :
    /// colonne par colonne, jamais au niveau feuille.
    DeleteCells {
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes demandé (l'annulation utilise la taille
        /// réellement retirée par colonne, comme pour `DeleteRows`).
        count: usize,
        /// Valeurs retirées, par colonne effectivement touchée, dans l'ordre
        /// où `Command::DeleteCells::columns` les présentait.
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
    /// Remplacement des métadonnées d'une fenêtre : ni un instantané de
    /// structure (aucun nœud ne bouge), ni un gros buffer — `before`/`after`
    /// directement, comme `ColumnType`.
    WindowMeta {
        /// Fenêtre visée.
        id: NodeId,
        /// Métadonnées d'avant.
        before: WindowMeta,
        /// Métadonnées d'après.
        after: WindowMeta,
    },
    /// Remplacement du texte d'une note.
    NoteText {
        /// Note visée.
        id: NodeId,
        /// Texte d'avant.
        before: String,
        /// Texte d'après.
        after: String,
    },
    /// Remplacement des métadonnées d'une colonne (rôle, unité, nom long,
    /// commentaire, format d'affichage) — distinct de `ColumnType` : ce
    /// delta ne touche en général jamais aux valeurs de la colonne, seulement
    /// à ce petit enregistrement d'affichage.
    ///
    /// Exception : `values_before`. Quand cette commande fait passer
    /// `formula` de `None` à `Some(_)`, la colonne cesse d'être une source de
    /// vérité et devient, dans l'instant qui suit (`stokesforge-session::recalc::
    /// recalc_formulas`, hors historique — voir sa doc de tête), une colonne
    /// recalculée dont le contenu précédent est **écrasé sans qu'aucune
    /// commande ne le rejoue**. Sans capturer ce contenu ici, annuler cette
    /// entrée retire bien la formule (`before`) mais laisse les valeurs
    /// calculées en place : `recalc_formulas` ne recalcule que les colonnes
    /// qui *portent encore* une formule, elle ignore silencieusement une
    /// colonne qui vient d'en perdre une — c'était le bug. `None` dans tous
    /// les autres cas (formule déjà active avant cette commande, formule
    /// retirée, ou aucun changement de formule) : ces transitions n'ont
    /// besoin de rien de plus, le recalcul se rechargeant de lui-même sur la
    /// base de la formule — encore active de part et d'autre de
    /// l'annulation/rétablissement — sans jamais perdre de donnée utilisateur.
    ColumnMeta {
        /// Colonne visée.
        column: NodeId,
        /// Métadonnées d'avant (boîtées : `ColumnMeta` porte les métadonnées de
        /// fichier et la source d'import, bien plus gros que les autres deltas).
        before: Box<ColumnMeta>,
        /// Métadonnées d'après.
        after: Box<ColumnMeta>,
        /// Type et valeurs de la colonne juste avant cette commande,
        /// seulement quand `before.formula` était `None` et
        /// `after.formula` devient `Some(_)` — voir la doc ci-dessus.
        values_before: Option<(ColumnType, ColumnValues)>,
    },
    /// Remplacement du `PlotSpec` d'un graphe. Un spec ne porte que des
    /// identités et du style, jamais de donnée (voir la tête de
    /// `stokesforge-plot/src/lib.rs`) : `before`/`after` directement, comme pour
    /// les trois deltas ci-dessus.
    PlotSpec {
        /// Graphe visé.
        graph: NodeId,
        /// Spec d'avant.
        before: PlotSpec,
        /// Spec d'après.
        after: PlotSpec,
    },
    /// Remplacement des couleurs enregistrées dans le projet — quelques
    /// chaînes, `before`/`after` directement.
    SavedColors {
        /// Couleurs d'avant.
        before: SavedColors,
        /// Couleurs d'après.
        after: SavedColors,
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
        label: HistoryLabel,
    },
    /// Delta de données.
    Data {
        /// Ce qu'il faut rejouer dans l'arbre courant. `Box`é : depuis
        /// l'ajout de `ColumnMeta::formula` (colonnes calculées), la
        /// variante `DataDelta::ColumnMeta { before, after }` porte deux
        /// `ColumnMeta` complètes et fait de `DataDelta` la plus grosse
        /// variante d'`Entry` de loin — `clippy::large_enum_variant`
        /// (déclenché à cet ajout) recommande cette indirection plutôt que
        /// de faire porter à *toutes* les entrées de structure/macro le coût
        /// mémoire du plus gros delta possible.
        delta: Box<DataDelta>,
        /// Libellé d'historique.
        label: HistoryLabel,
    },
    /// Groupe d'entrées, annulé en ordre inverse et rétabli en ordre direct.
    Macro {
        /// Entrées dans leur ordre d'application.
        entries: Vec<Entry>,
        /// Libellé du groupe : toujours [`HistoryLabel::Macro`].
        label: HistoryLabel,
    },
}

impl Entry {
    /// Libellé affichable de l'entrée (« Annuler : ... »).
    pub fn label(&self) -> &HistoryLabel {
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

/// Ce que `just_orphaned_by_write` a besoin de savoir d'avant l'application
/// d'une commande de données : pour chaque colonne qu'elle pourrait toucher,
/// l'adresse et la taille du buffer qu'elle porte à cet instant.
///
/// Capturée par [`prior_columns`], par un simple emprunt de `project.tree()`
/// **avant** d'appliquer la commande — jamais l'arbre entier, jamais gardé
/// vivant au-delà de l'appel qui la construit (voir la doc de
/// [`History::execute`], D3 (2ᵉ moitié) du guide) : c'est précisément ce qui
/// permet à `Arc::make_mut` de muter la `Tree` en place plutôt que de la
/// cloner.
type PriorColumns = HashMap<NodeId, (usize, usize)>;

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

/// Ce qu'on sait, en permanence, d'un buffer que **au moins une** entrée de
/// `undo` (l'invariant « `redo` est toujours vide entre deux commandes » —
/// voir la doc de [`History::push`] — vaut ici aussi, donc `redo` n'a jamais
/// besoin d'être suivi séparément) retient dans son empreinte : combien
/// d'entrées le retiennent, et s'il a déjà quitté l'arbre courant.
///
/// C'est la contrepartie *vivante*, tenue à jour buffer par buffer, de ce
/// que [`History::recompute_retained_bytes`] recalculait auparavant depuis
/// zéro à chaque commande — voir [`History::pin_footprint`] et
/// [`History::unpin_footprint`] pour comment elle est maintenue, et la
/// section « Le suivi incrémental (anomalie D3) » sur [`History::evict`]
/// pour pourquoi.
#[derive(Debug, Clone, Copy)]
struct PinnedBuffer {
    /// Octets du buffer — copiés depuis la `Footprint` de l'entrée qui l'a
    /// introduit, jamais recalculés.
    bytes: usize,
    /// Nombre d'entrées de `undo` qui le retiennent dans leur empreinte.
    refcount: usize,
    /// Vrai s'il a déjà quitté l'arbre courant (compté dans
    /// `retained_bytes`) ; faux s'il lui est encore commun (gratuit — pas un
    /// coût de l'historique).
    orphaned: bool,
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

/// Profondeur maximale d'imbrication des macros (B7,
/// `guide-de-correction-robustesse.md` §F4 — option 1, retenue par le
/// guide : une constante nommée et une erreur, plutôt que de rendre
/// `revert`/`Drop` itératifs sur `Entry::Macro`, « du code délicat sur un
/// enum récursif »). Rien ne bornait l'imbrication avant ce correctif :
/// `begin_macro`/`end_macro` sont eux-mêmes itératifs (les construire ne
/// déborde jamais, la sonde le vérifie jusqu'à 200 000 niveaux), mais
/// `History::revert` et le `Drop` dérivé d'`Entry` récursent sur
/// `Entry::Macro` — au-delà de ~10 891 niveaux, l'un ou l'autre déborde la
/// pile du fil de l'acteur (2 Mio par défaut) et fait `abort()` le
/// processus entier, y compris tout travail non sauvegardé (voir
/// `stokesforge-robustesse/src/bin/sonde_pile.rs`). Mille niveaux couvrent trois
/// ordres de grandeur au-dessus de tout usage plausible (§7 du guide) tout
/// en laissant une marge large sous le seuil mesuré de débordement.
pub const MAX_MACRO_DEPTH: usize = 1000;

/// Pile d'historique à deux natures.
///
/// Elle **ne possède pas** le `Project` : chaque opération le reçoit en
/// `&mut`. C'est l'acteur de `stokesforge-session` qui détient l'un et l'autre, ce
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
    /// Octets de charge utile retenus — exact, maintenu à jour en continu
    /// (voir [`History::retained_bytes`], et le suivi incrémental documenté
    /// sur [`History::evict`]).
    retained_bytes: usize,
    /// Comptabilité vivante, buffer par buffer, de ce que retient `undo` —
    /// voir [`PinnedBuffer`]. `retained_bytes` est, en permanence, la somme
    /// des `bytes` de ses entrées `orphaned`.
    pinned: HashMap<usize, PinnedBuffer>,
    /// Macros ouvertes, de la plus externe à la plus interne : libellé et
    /// entrées déjà empreintées, accumulées avant de devenir une seule
    /// entrée `Macro` à la fermeture.
    macros: Vec<(MacroKind, Vec<Pinned>)>,
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
            pinned: HashMap::new(),
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

    /// Octets de charge utile que l'historique est seul à maintenir
    /// vivants — **exact**, en permanence (voir le suivi incrémental
    /// documenté sur [`History::evict`] : `undo`/`redo`/`begin_macro`/
    /// `abort_macro` n'ont pas besoin de le redéclencher, puisqu'aucun
    /// d'eux n'ajoute, ne retire, ni ne change le statut « épinglé/orphelin »
    /// d'aucun buffer — ils ne font que déplacer des entrées déjà comptées
    /// entre les deux piles, ou les rejouer dans le projet).
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
    /// **Deux chemins, un seul et même contrat.** Le contrat — en cas
    /// d'`Err`, ni l'arbre ni la pile n'ont changé — est identique pour les
    /// deux natures ; seul le **mécanisme** qui le tient diffère (D3 (2ᵉ
    /// moitié), `guide-de-correction-robustesse.md` §6 « ne pas supprimer la
    /// comptabilité, seulement sa fréquence ») :
    ///
    /// - **structure** : `before` (un clone d'`Arc<Tree>`, un pointeur) est
    ///   gardé vivant pendant toute l'application, puis remis par sécurité
    ///   sur `Err` (`project.replace_tree(before)`), sans se reposer sur la
    ///   promesse d'atomicité des primitives du cœur. `Entry::Structural` en
    ///   a de toute façon besoin pour `before`/`after` : le coût est déjà
    ///   payé, l'assurance est gratuite.
    /// - **données** : garder `before` vivant forcerait `Arc::make_mut` (vu
    ///   par `Project::set_values` et consorts) à cloner la `Tree` entière
    ///   au lieu de muter en place — tout le sens de D3. On ne le fait donc
    ///   pas ; le contrat repose alors sur ce que chaque primitive de
    ///   donnée *valide avant de muter* (`Column::set_values`,
    ///   `Column::insert_rows`... — testé dans `stokesforge-core`) : un rejet n'a
    ///   jamais commencé à écrire, donc rien n'est à moitié fait à
    ///   restaurer. Seul ce qu'il faut pour `just_orphaned_by_write` (le
    ///   pointeur du buffer déjà présent dans la ou les colonnes visées) est
    ///   capturé, par un simple emprunt, **avant** d'appliquer la commande —
    ///   jamais l'arbre.
    ///
    /// En cas de succès la pile de rétablissement est vidée, **y compris en
    /// macro** : dès qu'une nouvelle mutation est appliquée, ce qui avait été
    /// annulé n'a plus de branche où revenir.
    pub fn execute(&mut self, project: &mut Project, command: Command) -> CommandResult<()> {
        self.execute_recorded(project, command).map(|_| ())
    }

    /// Comme [`execute`](Self::execute), mais dit si la commande a **rangé une
    /// entrée** (en pile, ou dans la macro ouverte) : `Ok(false)` pour une
    /// commande acceptée mais sans effet (B5), qui n'occupe aucun cran.
    ///
    /// L'acteur s'en sert pour que Stop n'annule que la commande qu'il vient
    /// d'exécuter : sans ce booléen, un `undo` défaisait l'action *précédente*.
    /// On ne le déduit pas de `len()` avant/après — l'éviction (limite
    /// d'entrées ou d'octets) laisse la longueur inchangée, et une entrée rangée
    /// dans une macro ne la change pas non plus.
    pub fn execute_recorded(
        &mut self,
        project: &mut Project,
        command: Command,
    ) -> CommandResult<bool> {
        if command.is_structural() {
            let before = Arc::clone(project.tree());
            let label = command.label(&before);
            match Self::apply_structural(project, command, &before, label) {
                Ok(entry) => {
                    // B5 + le cas Rename (`guide-de-correction-robustesse.md`
                    // §F5) — une commande acceptée qui n'a RIEN changé n'occupe
                    // pas de cran d'annulation : « Renommer M en M »
                    // (consigné sans identifiant ni reproduction en §7.3 du
                    // guide) — une entrée structurelle dont `before == after`
                    // couvre ce cas et tout autre qui lui ressemble, sans le
                    // nommer un par un. Rien n'est à défaire : on rend
                    // simplement `Ok` sans empiler.
                    if entry_has_no_effect(&entry) {
                        return Ok(false);
                    }
                    self.push(entry, &PriorColumns::new(), project);
                    Ok(true)
                }
                Err(err) => {
                    // Les primitives de `Project` promettent de ne rien changer en
                    // cas d'échec, mais rétablir `before` est gratuit (un pointeur)
                    // et rend la garantie indépendante du cœur.
                    project.replace_tree(before);
                    Err(err)
                }
            }
        } else {
            // Pas de clone de l'arbre ici : voir la doc ci-dessus. `label`
            // et `prior` ne font qu'emprunter `project.tree()`, jamais le
            // retenir au-delà de cette ligne.
            let label = command.label(project.tree());
            let prior = prior_columns(project.tree(), &command);
            match Self::apply_data(project, command, label) {
                Ok(entry) => {
                    // B5 (`guide-de-correction-robustesse.md` §F5) —
                    // `InsertRows`/`DeleteRows` sans colonne cible (voir
                    // `Project::row_targets`) n'occupent pas de cran
                    // d'annulation.
                    if entry_has_no_effect(&entry) {
                        return Ok(false);
                    }
                    self.push(entry, &prior, project);
                    Ok(true)
                }
                // Aucun `replace_tree` à faire ici : voir la doc ci-dessus —
                // une primitive de donnée qui échoue n'a rien muté.
                Err(err) => Err(err),
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
            // `pinned`/`retained_bytes` comptent l'union de `undo` et
            // `redo` (voir leur doc) : déplacer une entrée de l'une à
            // l'autre ne change ni ce qu'elle retient, ni son statut
            // épinglé/orphelin — rien à mettre à jour ici.
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
    pub fn undo_label(&self) -> Option<&HistoryLabel> {
        self.undo.last().map(|pinned| pinned.entry.label())
    }

    /// Libellé du prochain rétablissement.
    pub fn redo_label(&self) -> Option<&HistoryLabel> {
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
    /// empilées. Les macros s'imbriquent, jusqu'à [`MAX_MACRO_DEPTH`]
    /// niveaux : au-delà, `Err(MacroTooDeep)` (B7,
    /// `guide-de-correction-robustesse.md` §F4) plutôt que d'empiler encore
    /// — voir la doc de [`MAX_MACRO_DEPTH`] pour ce que cette limite évite.
    /// Un refus ne modifie rien : ni `self.macros`, ni `self.saved_redo`.
    ///
    /// À l'ouverture de la macro **la plus externe** (`macro_depth() == 0 →
    /// 1`), la pile de rétablissement courante est mise de côté : elle
    /// pourra être restaurée si cette macro est avortée, voir
    /// [`History::abort_macro`].
    pub fn begin_macro(&mut self, kind: MacroKind) -> CommandResult<()> {
        if self.macros.len() >= MAX_MACRO_DEPTH {
            return Err(CommandError::MacroTooDeep(MAX_MACRO_DEPTH));
        }
        if self.macros.is_empty() {
            self.saved_redo = Some(std::mem::take(&mut self.redo));
        }
        self.macros.push((kind, Vec::new()));
        Ok(())
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
        let Some((kind, entries)) = self.macros.pop() else {
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
            // Le redo sauvegardé à l'ouverture est bel et bien perdu : ses
            // buffers doivent quitter la comptabilité (`self.pinned`)
            // exactement comme s'il avait été vidé par un `push` ordinaire
            // (voir [`History::push`]) — sans ça, `retained_bytes`
            // continuerait de compter des buffers qu'aucune entrée vivante
            // ne retient plus.
            if let Some(discarded) = self.saved_redo.take() {
                for pinned in discarded {
                    self.unpin_footprint(&pinned.footprint);
                }
            }
        }
        let footprint = union_footprint(entries.iter().map(|pinned| &pinned.footprint));
        let entry = Entry::Macro {
            entries: entries.into_iter().map(|pinned| pinned.entry).collect(),
            label: HistoryLabel::Macro(kind),
        };
        // `prior` n'est lu que sur la branche `Data` de `push_pinned` ; une
        // entrée `Macro` retombe sur `resync_pinned`, qui ne le regarde pas.
        self.push_pinned(Pinned { entry, footprint }, &PriorColumns::new(), project);
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
        self.pinned.clear();
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
    ///
    /// `prior` est ce que [`prior_columns`] a capturé, par emprunt, sur
    /// l'arbre **juste avant** cette commande — vide pour une entrée
    /// `Structural` (elle ne s'en sert pas, voir [`History::push_pinned`]).
    /// Une entrée `Data` ne garde pas trace de `prior` elle-même (elle ne
    /// retient que son delta), mais [`History::push_pinned`] en a besoin
    /// pour savoir si l'écriture vient d'orpheliner l'ancien buffer d'une
    /// colonne qu'une entrée *antérieure* retenait déjà (voir sa doc).
    fn push(&mut self, entry: Entry, prior: &PriorColumns, project: &Project) {
        // Le redo est perdu : ses buffers doivent quitter la comptabilité
        // exactement comme ceux d'une macro abandonnée avec succès (voir
        // `History::end_macro`) — sans ça, `retained_bytes` continuerait de
        // compter des buffers qu'aucune entrée vivante ne retient plus.
        let discarded = std::mem::take(&mut self.redo);
        for pinned in discarded {
            self.unpin_footprint(&pinned.footprint);
        }
        let footprint = footprint_of(&entry);
        self.push_pinned(Pinned { entry, footprint }, prior, project);
    }

    /// Range une entrée déjà empreintée dans la macro ouverte, ou au sommet
    /// de la pile principale (avec mise à jour de la comptabilité puis
    /// éviction). **Rien ne change au milieu d'une macro ouverte** : ni
    /// éviction, ni comptabilité des octets retenus — seule la fermeture de
    /// la macro la plus externe qui aboutit avec au moins une entrée (voir
    /// [`History::end_macro`]) rejoint la pile principale.
    ///
    /// # Le suivi incrémental (anomalie D3)
    ///
    /// Avant ce correctif, atterrir ici déclenchait, à **chaque** commande —
    /// y compris une simple écriture de cellule — un recalcul complet de
    /// `retained_bytes` : parcourir l'arbre courant en entier (toutes les
    /// colonnes du projet) puis refaire l'union des empreintes de toutes les
    /// entrées des deux piles. Le coût d'une frappe suivait donc la taille
    /// du projet *et* la profondeur de l'historique, même quand aucune
    /// éviction n'avait lieu (voir
    /// `stokesforge-robustesse/tests/b21_cout_et_identite.rs`, reproductions
    /// `b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique` et
    /// `..._du_nombre_de_colonnes`).
    ///
    /// La correction ne relâche **jamais** l'exactitude de `retained_bytes`
    /// (voir `tests/memory_pinning.rs`) : au lieu de la recalculer depuis
    /// zéro, elle la maintient **à jour buffer par buffer**, dans
    /// [`History::pinned`], à chaque empilement — sans jamais parcourir
    /// l'arbre entier ni relire l'historique complet :
    ///
    /// - une entrée `Structural` capture déjà `before` et `after` dans ses
    ///   deux champs : un buffer de sa propre empreinte est orphelin s'il
    ///   n'est pas dans `after` (donc plus dans l'arbre courant), vivant
    ///   sinon. Un seul parcours de `after` (inévitable : c'est déjà le
    ///   travail que fait [`footprint_of`] pour construire l'empreinte de
    ///   *cette* entrée, pas un nouveau parcours ajouté par ce correctif) ;
    /// - une entrée `Data` ne partage jamais son propre delta avec l'arbre
    ///   (voir [`footprint_of`]) : toute son empreinte est orpheline dès
    ///   l'empilement. Ce qui *peut* changer ailleurs, c'est qu'elle vient
    ///   de remplacer, par copie-à-l'écriture, le buffer que sa colonne
    ///   avait *avant* elle — si ce buffer était déjà épinglé par une entrée
    ///   plus ancienne (une recherche par identifiant dans `before`, O(1),
    ///   jamais un parcours), il vient de quitter l'arbre : voir
    ///   [`just_orphaned_by_write`] ;
    /// - évincer une entrée (ci-dessous) retire simplement sa part du
    ///   compte, buffer par buffer (voir [`History::unpin_footprint`]).
    ///
    /// Le seul cas non modélisé ici est la fermeture d'une macro
    /// (`Entry::Macro`) : elle agrège des sous-commandes déjà appliquées à
    /// des instants différents, et retrouver *quand* chacune a orpheliné
    /// quoi demanderait de rejouer leur avant/après un par un. Comme les
    /// macros ne sont pas déclenchées par frappe — l'anomalie mesurée —
    /// cette entrée retombe sur l'ancien calcul exhaustif
    /// ([`History::resync_pinned`]), qui reconstruit `self.pinned` en même
    /// temps qu'il recalcule `retained_bytes`, pour que le suivi incrémental
    /// reste juste ensuite.
    fn push_pinned(&mut self, pinned: Pinned, prior: &PriorColumns, project: &Project) {
        if let Some((_, entries)) = self.macros.last_mut() {
            entries.push(pinned);
            return;
        }
        match &pinned.entry {
            Entry::Macro { .. } => {
                self.undo.push(pinned);
                self.resync_pinned(project);
            }
            Entry::Data { delta, .. } => {
                let just_orphaned = just_orphaned_by_write(prior, delta);
                self.mark_orphaned(&just_orphaned);
                // Une entrée `Data` ne partage jamais son delta avec
                // l'arbre : chacun de ses buffers propres est orphelin dès
                // son introduction (voir la doc ci-dessus).
                self.pin_footprint(&pinned.footprint, |_ptr| true);
                self.undo.push(pinned);
            }
            Entry::Structural { .. } => {
                // `after` (le champ de l'entrée) et `project.tree()` sont le
                // même arbre à cet instant : rien n'a pu s'exécuter entre la
                // fin de `apply` et cet appel.
                let current = current_tree_pointers(project.tree());
                self.pin_footprint(&pinned.footprint, |ptr| !current.contains(&ptr));
                self.undo.push(pinned);
            }
        }
        self.evict();
    }

    /// Pour chaque buffer de `footprint` : incrémente son compteur de
    /// référence s'il est déjà connu de [`History::pinned`] (son statut
    /// épinglé/orphelin, décidé à sa toute première entrée, ne change pas ici
    /// — voir [`History::mark_orphaned`] pour la seule façon dont un buffer
    /// déjà vivant peut ensuite devenir orphelin) ; sinon l'y introduit,
    /// orphelin si `is_orphan` le dit vrai pour ce pointeur (et alors compté
    /// tout de suite dans `retained_bytes`), vivant sinon.
    fn pin_footprint(&mut self, footprint: &Footprint, is_orphan: impl Fn(usize) -> bool) {
        for (&ptr, &bytes) in footprint {
            match self.pinned.entry(ptr) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    occupied.get_mut().refcount += 1;
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    let orphaned = is_orphan(ptr);
                    if orphaned {
                        self.retained_bytes += bytes;
                    }
                    vacant.insert(PinnedBuffer {
                        bytes,
                        refcount: 1,
                        orphaned,
                    });
                }
            }
        }
    }

    /// Pour chaque buffer de `just_left`, s'il est déjà épinglé (par une
    /// entrée plus ancienne) et encore marqué vivant, le fait basculer
    /// orphelin et ajoute ses octets à `retained_bytes`. Un buffer absent de
    /// `self.pinned` n'est pas un coût de l'historique — il n'a jamais été
    /// épinglé — et ne fait rien ici.
    fn mark_orphaned(&mut self, just_left: &Footprint) {
        for &ptr in just_left.keys() {
            if let Some(buffer) = self.pinned.get_mut(&ptr) {
                if !buffer.orphaned {
                    buffer.orphaned = true;
                    self.retained_bytes += buffer.bytes;
                }
            }
        }
    }

    /// Retire la part d'une entrée évincée (ou d'une pile jetée) dans
    /// [`History::pinned`] : décrémente le compteur de référence de chacun
    /// de ses buffers, et quand il tombe à zéro (plus aucune entrée ne le
    /// retient), l'ôte de la comptabilité — en retranchant ses octets de
    /// `retained_bytes` s'il y était compté (orphelin).
    fn unpin_footprint(&mut self, footprint: &Footprint) {
        for &ptr in footprint.keys() {
            if let std::collections::hash_map::Entry::Occupied(mut occupied) =
                self.pinned.entry(ptr)
            {
                let buffer = occupied.get_mut();
                buffer.refcount -= 1;
                if buffer.refcount == 0 {
                    if buffer.orphaned {
                        self.retained_bytes -= buffer.bytes;
                    }
                    occupied.remove();
                }
            }
        }
    }

    /// Retire silencieusement les entrées les plus anciennes de `undo` tant
    /// que l'une des deux limites (nombre d'entrées, octets retenus) est
    /// dépassée. `retained_bytes` est déjà exact en entrée (voir
    /// [`History::push_pinned`]) : évincer une entrée n'a plus qu'à
    /// retrancher sa part (voir [`History::unpin_footprint`]), jamais à
    /// reparcourir quoi que ce soit.
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
    /// elle est vidée par le prochain empilement (voir [`History::push`]).
    /// Le total retenu est l'union des deux piles, donc **une longue branche
    /// de rétablissement peut maintenir le total au-dessus du plafond en
    /// octets jusqu'à la commande suivante** — ce n'est pas un oubli : c'est
    /// le prix de ne jamais faire disparaître une branche de rétablissement
    /// autrement qu'à la prochaine mutation.
    fn evict(&mut self) {
        while self.undo.len() > 1 {
            let over_count = self.limit != 0 && self.undo.len() > self.limit;
            let over_bytes = self.byte_limit != 0 && self.retained_bytes > self.byte_limit;
            if !over_count && !over_bytes {
                break;
            }
            // La pile reste courte (une centaine d'entrées par défaut) : le
            // décalage d'un `Vec` est sans conséquence face au coût d'un
            // `VecDeque` partout ailleurs.
            let evicted = self.undo.remove(0);
            self.unpin_footprint(&evicted.footprint);
        }
    }

    /// Filet de sécurité : recalcule [`History::retained_bytes`] **et**
    /// reconstruit [`History::pinned`] entièrement depuis zéro — l'ancien
    /// calcul exhaustif (parcours complet de l'arbre courant, union de
    /// toutes les empreintes de `undo` et `redo`), plus la reconstruction du
    /// suivi incrémental sur lequel [`History::push_pinned`] s'appuie
    /// ensuite. Réservé au seul cas que ce suivi ne modélise pas
    /// (l'agrégation d'une macro à sa fermeture, voir
    /// [`History::push_pinned`]) : jamais appelé sur le chemin d'une
    /// commande simple, jamais le coût d'une frappe ordinaire.
    fn resync_pinned(&mut self, project: &Project) {
        self.pinned.clear();
        let current = current_tree_pointers(project.tree());
        for pinned in self.undo.iter().chain(self.redo.iter()) {
            for (&ptr, &bytes) in &pinned.footprint {
                self.pinned
                    .entry(ptr)
                    .and_modify(|buffer| buffer.refcount += 1)
                    .or_insert(PinnedBuffer {
                        bytes,
                        refcount: 1,
                        orphaned: !current.contains(&ptr),
                    });
            }
        }
        self.retained_bytes = self
            .pinned
            .values()
            .filter(|buffer| buffer.orphaned)
            .map(|buffer| buffer.bytes)
            .sum();
    }

    /// Applique une commande **structurelle** et construit son entrée
    /// d'historique. `before` est l'`Arc<Tree>` que [`History::execute`] a
    /// cloné avant d'appeler cette fonction — nécessaire ici : `Entry::
    /// Structural` le retient, et `DeleteColumn` doit lire le nœud visé tel
    /// qu'il était avant toute mutation.
    ///
    /// N'est jamais appelée avec une commande de données — `execute` aiguille
    /// sur [`Command::is_structural`] avant d'appeler l'une ou l'autre.
    fn apply_structural(
        project: &mut Project,
        command: Command,
        before: &Arc<Tree>,
        label: HistoryLabel,
    ) -> CommandResult<Entry> {
        match command {
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
                        "{column} is not a column but {}: column deletion refused",
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
            Command::AddFolder { parent, name } => {
                project.add_folder(parent, &name)?;
                Ok(structural(before, project, label))
            }
            Command::AddWorkbook { parent, name } => {
                project.add_workbook(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddEmptyWorkbook { parent, name } => {
                project.add_empty_workbook(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddWorksheet { book, name } => {
                project.add_worksheet(book, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::DuplicateWorksheet { sheet, name } => {
                project.duplicate_worksheet(sheet, &name)?;
                Ok(structural(before, project, label))
            }
            Command::InsertCopy { parent, copy, name } => {
                project.insert_copy(parent, &copy, &name)?;
                Ok(structural(before, project, label))
            }
            Command::AddGraph { parent, name, spec } => {
                project.add_graph(parent, name.as_deref(), spec)?;
                Ok(structural(before, project, label))
            }
            Command::AddMatrix { parent, name } => {
                project.add_matrix(parent, name.as_deref())?;
                Ok(structural(before, project, label))
            }
            Command::AddNote { parent, name, text } => {
                project.add_note(parent, name.as_deref(), &text)?;
                Ok(structural(before, project, label))
            }
            Command::RemoveNode { id } => {
                project.remove(id)?;
                Ok(structural(before, project, label))
            }
            Command::ReplaceAnalysisData {
                draft,
                expected_versions,
            } => {
                project.replace_analysis_data(draft, &expected_versions)?;
                Ok(structural(before, project, label))
            }
            Command::AddCalculation { draft } => {
                project.add_calculation(draft)?;
                Ok(structural(before, project, label))
            }
            Command::RestoreOriginalData { trace } => {
                project.restore_original_data(trace)?;
                Ok(structural(before, project, label))
            }
            Command::UpdateCalculation { id, draft } => {
                project.update_calculation(id, draft)?;
                Ok(structural(before, project, label))
            }
            Command::RemoveCalculation { id, with_columns } => {
                project.remove_calculation(id, with_columns)?;
                Ok(structural(before, project, label))
            }
            Command::SetCells { .. }
            | Command::SetCellsPromoting { .. }
            | Command::SetColumnType { .. }
            | Command::InsertRows { .. }
            | Command::DeleteRows { .. }
            | Command::DeleteCells { .. }
            | Command::SetWindowMeta { .. }
            | Command::SetNoteText { .. }
            | Command::SetColumnMeta { .. }
            | Command::SetPlotSpec { .. }
            | Command::EditPlotSpec { .. }
            | Command::SetSavedColors { .. } => {
                unreachable!("apply_structural appelée avec une commande de données")
            }
        }
    }

    /// Applique une commande de **données** et construit son entrée
    /// d'historique. Aucun `Arc<Tree>` en paramètre — voir la doc de
    /// [`History::execute`], D3 (2ᵉ moitié) du guide : le tenir vivant ici
    /// forcerait `Arc::make_mut` à cloner la `Tree` entière plutôt que de
    /// muter en place. Tout ce que ces commandes lisent avant de muter (la
    /// version d'une colonne pour `SetCells`) passe par un simple emprunt de
    /// `project.tree()`, jamais retenu au-delà de la ligne qui le lit.
    ///
    /// N'est jamais appelée avec une commande structurelle — voir
    /// [`History::apply_structural`].
    fn apply_data(
        project: &mut Project,
        command: Command,
        label: HistoryLabel,
    ) -> CommandResult<Entry> {
        match command {
            Command::SetCells {
                column,
                start,
                values,
                expected_version,
            } => {
                check_writable_cells(project, column, expected_version)?;
                Ok(Entry::Data {
                    delta: Box::new(write_cells(project, column, start, values)?),
                    label,
                })
            }
            Command::SetCellsPromoting {
                column,
                start,
                values,
                expected_version,
            } => {
                check_writable_cells(project, column, expected_version)?;
                let column_type = project
                    .tree()
                    .get(column)
                    .ok()
                    .and_then(|node| node.as_column())
                    .map(stokesforge_core::Column::column_type);
                let delta = match column_type {
                    // Chemin 1 — le bloc convient déjà, ou la colonne n'en
                    // est pas une (laisser `set_values` rendre l'erreur
                    // naturelle plutôt que d'en inventer une ici, même
                    // discipline que pour A2 ci-dessus).
                    None => write_cells(project, column, start, values)?,
                    Some(current) if values.fits(current) => {
                        write_cells(project, column, start, values)?
                    }
                    // Chemin 2 — la colonne est déjà mixte : seul le bloc se
                    // convertit, et l'écriture reste un delta de cellules,
                    // proportionnel à ce qui est écrit.
                    Some(ColumnType::Mixed) => write_cells(
                        project,
                        column,
                        start,
                        ValueBlock::Mixed(values.into_mixed()),
                    )?,
                    // Chemin 3 — promotion. Le delta est celui de
                    // `SetColumnType` : deux instantanés de colonne, parce
                    // que la conversion n'est pas réversible par recalcul
                    // (voir `DataDelta::ColumnType`). L'écriture qui suit est
                    // **dans** le même instantané `after`, donc dans la même
                    // entrée d'historique : une seule annulation ramène à la
                    // fois le type et les valeurs d'avant.
                    Some(_) => {
                        let before = project.set_column_type(column, ColumnType::Mixed)?;
                        project.set_values(
                            column,
                            start,
                            ValueBlock::Mixed(values.into_mixed()),
                        )?;
                        let after = column_state(project, column)?;
                        DataDelta::ColumnType {
                            column,
                            before,
                            after,
                        }
                    }
                };
                Ok(Entry::Data {
                    delta: Box::new(delta),
                    label,
                })
            }
            Command::SetColumnType {
                column,
                column_type,
            } => {
                check_not_calculated(project, column)?;
                let before_state = project.set_column_type(column, column_type)?;
                let after_state = column_state(project, column)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::ColumnType {
                        column,
                        before: before_state,
                        after: after_state,
                    }),
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
                // Les séries « ligne » du projet suivent : specs relevés avant
                // la mutation, réécrits après (par `set_plot_spec`).
                let moved = shifted_row_specs(project.tree(), sheet, |r| {
                    (r >= at).then(|| r.saturating_add(count))
                });
                let columns = project.insert_rows(sheet, at, count)?;
                let plots = write_row_specs(project, moved)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::InsertRows {
                        columns,
                        at,
                        count,
                        plots,
                    }),
                    label,
                })
            }
            Command::DeleteRows { sheet, at, count } => {
                // Une série « ligne » après la plage supprimée recule de
                // `count`. Une série dont la ligne est dans la plage supprimée
                // reste telle quelle (décision ouverte, #307 D1) : elle trace
                // alors la ligne qui a pris sa place, ou rien au-delà de la
                // feuille (NaN, invariant 10).
                let moved = shifted_row_specs(project.tree(), sheet, |r| {
                    (r >= at.saturating_add(count)).then(|| r - count)
                });
                let removed = project.delete_rows(sheet, at, count)?;
                let plots = write_row_specs(project, moved)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::DeleteRows {
                        sheet,
                        at,
                        count,
                        removed,
                        plots,
                    }),
                    label,
                })
            }
            Command::DeleteCells { columns, at, count } => {
                // Avant toute mutation : un refus ne laisse aucun état partiel.
                for column in &columns {
                    check_not_calculated(project, *column)?;
                }
                // Colonne par colonne, indépendamment — contrairement à
                // `DeleteRows`, `columns` n'est pas nécessairement "toutes
                // les colonnes de la feuille" : c'est la sélection explicite
                // de l'appelant (une colonne sans ligne `at`, ou listée en
                // double, est ignorée silencieusement, comme `row_targets`
                // le fait déjà pour `DeleteRows`).
                let mut removed = Vec::new();
                let mut seen = HashSet::new();
                for column in columns {
                    if !seen.insert(column) {
                        continue;
                    }
                    let has_row = project
                        .tree()
                        .get(column)
                        .ok()
                        .and_then(|node| node.as_column())
                        .is_some_and(|c| at < c.len());
                    if count == 0 || !has_row {
                        continue;
                    }
                    let block = project.delete_column_rows(column, at, count)?;
                    removed.push((column, block));
                }
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::DeleteCells { at, count, removed }),
                    label,
                })
            }
            Command::SetWindowMeta { id, meta } => {
                let before = project.set_window_meta(id, meta.clone())?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::WindowMeta {
                        id,
                        before,
                        after: meta,
                    }),
                    label,
                })
            }
            Command::SetNoteText { id, text } => {
                let before = project.set_note_text(id, &text)?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::NoteText {
                        id,
                        before,
                        after: text,
                    }),
                    label,
                })
            }
            Command::SetColumnMeta { column, meta } => {
                // Une formule sur une colonne de sortie d'un calcul : deux
                // sources de vérité pour les mêmes cellules.
                if meta.formula.is_some() {
                    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
                        return Err(CoreError::CalculatedColumnReadOnly {
                            column,
                            calculation,
                        }
                        .into());
                    }
                }
                let before = project.set_column_meta(column, meta.clone())?;
                // Voir la doc de `DataDelta::ColumnMeta::values_before` :
                // capturé seulement à la transition « pas de formule avant,
                // formule après » — c'est la colonne qui perd, dans
                // l'instant, sa qualité de source de vérité au profit du
                // recalcul (`stokesforge-session::recalc::recalc_formulas`, hors
                // historique). Lu **après** `set_column_meta` : cet appel ne
                // touche jamais aux valeurs, seulement à `meta`, donc l'ordre
                // ne change rien à ce qui est capturé ici.
                let values_before = if before.formula.is_none() && meta.formula.is_some() {
                    Some(column_state(project, column)?)
                } else {
                    None
                };
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::ColumnMeta {
                        column,
                        before: Box::new(before),
                        after: Box::new(meta),
                        values_before,
                    }),
                    label,
                })
            }
            Command::SetPlotSpec { graph, spec } | Command::EditPlotSpec { graph, spec, .. } => {
                let before = project.set_plot_spec(graph, spec.clone())?;
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::PlotSpec {
                        graph,
                        before,
                        after: spec,
                    }),
                    label,
                })
            }
            Command::SetSavedColors { colors } => {
                let before = project.set_saved_colors(colors.clone());
                Ok(Entry::Data {
                    delta: Box::new(DataDelta::SavedColors {
                        before,
                        after: colors.bounded(),
                    }),
                    label,
                })
            }
            Command::AddColumn { .. }
            | Command::DeleteColumn { .. }
            | Command::Rename { .. }
            | Command::Move { .. }
            | Command::AddFolder { .. }
            | Command::AddWorkbook { .. }
            | Command::AddEmptyWorkbook { .. }
            | Command::AddWorksheet { .. }
            | Command::DuplicateWorksheet { .. }
            | Command::InsertCopy { .. }
            | Command::AddGraph { .. }
            | Command::AddMatrix { .. }
            | Command::AddNote { .. }
            | Command::RemoveNode { .. }
            | Command::ReplaceAnalysisData { .. }
            | Command::AddCalculation { .. }
            | Command::RestoreOriginalData { .. }
            | Command::UpdateCalculation { .. }
            | Command::RemoveCalculation { .. } => {
                unreachable!("apply_data appelée avec une commande structurelle")
            }
        }
    }

    /// Annule une entrée. Une macro s'annule en ordre **inverse**.
    fn revert(project: &mut Project, entry: &Entry) -> CommandResult<()> {
        match entry {
            Entry::Structural { before, .. } => {
                // A7 (`guide-de-correction-robustesse.md` §F2) —
                // `replace_tree` restaure l'arbre `before` tel quel,
                // compteurs de version compris : une colonne peut alors
                // reculer vers un numéro qu'un contenu différent (le sien,
                // à un autre instant de son histoire) porte déjà. On
                // mémorise l'arbre qu'on quitte avant de le remplacer, pour
                // pouvoir comparer les deux ensuite — voir
                // `restamp_regressed_versions`.
                let discarded = Arc::clone(project.tree());
                project.replace_tree(Arc::clone(before));
                restamp_regressed_versions(project, &discarded, before)?;
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
            DataDelta::InsertRows {
                columns,
                at,
                count,
                plots,
            } => {
                for column in columns {
                    project.delete_column_rows(*column, *at, *count)?;
                }
                for (graph, before, _) in plots {
                    project.set_plot_spec(*graph, before.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteRows {
                at, removed, plots, ..
            } => {
                for (column, block) in removed {
                    project.insert_column_rows(*column, *at, block.len())?;
                    project.set_values(*column, *at, block.clone())?;
                }
                for (graph, before, _) in plots {
                    project.set_plot_spec(*graph, before.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteCells { at, removed, .. } => {
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
            DataDelta::WindowMeta { id, before, .. } => {
                project.set_window_meta(*id, before.clone())?;
                Ok(())
            }
            DataDelta::NoteText { id, before, .. } => {
                project.set_note_text(*id, before)?;
                Ok(())
            }
            DataDelta::ColumnMeta {
                column,
                before,
                values_before,
                ..
            } => {
                project.set_column_meta(*column, ColumnMeta::clone(before))?;
                // Voir la doc de `DataDelta::ColumnMeta::values_before` : la
                // colonne redevient une source de vérité ordinaire, ses
                // valeurs d'avant la formule doivent revenir avec elle — sans
                // quoi le recalcul qui suit (`stokesforge-session::recalc::
                // recalc_formulas`) ignore une colonne qui n'a plus de
                // formule et laisse les valeurs calculées en place.
                if let Some((column_type, values)) = values_before {
                    project.restore_column(*column, *column_type, values.clone())?;
                }
                Ok(())
            }
            DataDelta::PlotSpec { graph, before, .. } => {
                project.set_plot_spec(*graph, before.clone())?;
                Ok(())
            }
            DataDelta::SavedColors { before, .. } => {
                project.set_saved_colors(before.clone());
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
            DataDelta::InsertRows {
                columns,
                at,
                count,
                plots,
            } => {
                for column in columns {
                    project.insert_column_rows(*column, *at, *count)?;
                }
                for (graph, _, after) in plots {
                    project.set_plot_spec(*graph, after.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteRows {
                at, removed, plots, ..
            } => {
                // Colonne par colonne, et non `delete_rows` au niveau feuille :
                // seules les colonnes réellement touchées la première fois
                // doivent l'être à nouveau.
                for (column, block) in removed {
                    project.delete_column_rows(*column, *at, block.len())?;
                }
                for (graph, _, after) in plots {
                    project.set_plot_spec(*graph, after.clone())?;
                }
                Ok(())
            }
            DataDelta::DeleteCells { at, removed, .. } => {
                // Colonne par colonne, comme pour `DeleteRows` : seules les
                // colonnes réellement touchées la première fois le sont à
                // nouveau.
                for (column, block) in removed {
                    project.delete_column_rows(*column, *at, block.len())?;
                }
                Ok(())
            }
            DataDelta::ColumnType { column, after, .. } => {
                project.restore_column(*column, after.0, after.1.clone())?;
                Ok(())
            }
            DataDelta::WindowMeta { id, after, .. } => {
                project.set_window_meta(*id, after.clone())?;
                Ok(())
            }
            DataDelta::NoteText { id, after, .. } => {
                project.set_note_text(*id, after)?;
                Ok(())
            }
            DataDelta::ColumnMeta { column, after, .. } => {
                project.set_column_meta(*column, ColumnMeta::clone(after))?;
                Ok(())
            }
            DataDelta::PlotSpec { graph, after, .. } => {
                project.set_plot_spec(*graph, after.clone())?;
                Ok(())
            }
            DataDelta::SavedColors { after, .. } => {
                project.set_saved_colors(after.clone());
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
fn structural(before: &Arc<Tree>, project: &Project, label: HistoryLabel) -> Entry {
    Entry::Structural {
        before: Arc::clone(before),
        after: Arc::clone(project.tree()),
        label,
    }
}

/// Vrai si `entry`, quoique produite par une commande acceptée, n'a
/// concrètement rien changé (F5, `guide-de-correction-robustesse.md`) : une
/// entrée pareille ne doit pas occuper de cran d'annulation.
///
/// - `Structural` : `before == after` couvre tout command structurelle
///   qui laisse l'arbre identique — `Rename` vers le nom déjà porté
///   (« Renommer M en M », §7.3 du guide) en est un cas, `Move` vers sa
///   position courante en serait un autre, sans qu'aucun des deux ait
///   besoin d'être reconnu par son nom : la comparaison structurelle des
///   deux arbres (`Tree` dérive `PartialEq`) les attrape tous les deux de
///   la même façon.
/// - `Data` : seuls `InsertRows`/`DeleteRows` peuvent cibler zéro colonne
///   (voir `Project::row_targets`) — `Cells` et `ColumnType` écrivent
///   toujours quelque chose dès qu'`apply` les a acceptés.
/// - `Macro` : une macro refermée avec au moins une entrée a, par
///   construction, appliqué au moins un delta réel (`end_macro` filtre déjà
///   les macros vides) — jamais un no-op à ce niveau.
fn entry_has_no_effect(entry: &Entry) -> bool {
    match entry {
        Entry::Structural { before, after, .. } => before == after,
        Entry::Data { delta, .. } => match delta.as_ref() {
            DataDelta::InsertRows { columns, .. } => columns.is_empty(),
            DataDelta::DeleteRows { removed, .. } => removed.is_empty(),
            DataDelta::DeleteCells { removed, .. } => removed.is_empty(),
            // Comme `Cells`/`ColumnType` : dès qu'`apply` les a acceptées,
            // ces quatre commandes ont toujours remplacé quelque chose (un
            // remplacement par une valeur égale reste un remplacement, pas
            // un cas particulier à détecter ici).
            DataDelta::Cells { .. }
            | DataDelta::ColumnType { .. }
            | DataDelta::WindowMeta { .. }
            | DataDelta::NoteText { .. }
            | DataDelta::ColumnMeta { .. }
            | DataDelta::PlotSpec { .. }
            | DataDelta::SavedColors { .. } => false,
        },
        Entry::Macro { .. } => false,
    }
}

/// Une colonne produite par un calcul d'analyse (#181) refuse aussi les
/// écritures qui changeraient sa forme (type, cellules supprimées) : le
/// recalcul les défairait, ou échouerait sans le dire (`TypeMismatch` sur une
/// colonne passée en texte).
fn check_not_calculated(project: &Project, column: NodeId) -> CommandResult<()> {
    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
        return Err(CoreError::CalculatedColumnReadOnly {
            column,
            calculation,
        }
        .into());
    }
    Ok(())
}

/// Les deux refus que toute écriture de cellules — `SetCells` comme
/// `SetCellsPromoting` — doit opposer **avant** de toucher au modèle, pour
/// qu'un échec ne laisse aucun état partiel.
///
/// 1. **A2** (`guide-de-correction-robustesse.md` §F6) — la condition
///    optimiste `expected_version`, vérifiée ICI, sur l'arbre courant au
///    moment d'appliquer cette commande (pas un instantané que l'appelant a
///    lu plus tôt), jamais avant l'envoi : c'est tout le sens du correctif.
/// 2. **Colonnes calculées** (v1, `stokesforge_core::formula`) — une
///    colonne-formule n'accepte l'écriture directe de cellules d'aucun
///    client : ses valeurs ne sont recalculées que par
///    `stokesforge-session::actor` après coup, en dehors de l'historique.
///    Même refus pour une colonne produite par un calcul d'analyse (#181).
///
/// Une colonne absente ou d'un autre type n'est traitée ni par l'un ni par
/// l'autre : c'est `Project::set_values`, ensuite, qui rendra l'erreur
/// naturelle (`UnknownNode`/`Structure`) — laquelle ne doit pas se faire
/// maquiller en désaccord de version.
fn check_writable_cells(
    project: &Project,
    column: NodeId,
    expected_version: Option<u64>,
) -> CommandResult<()> {
    let target = project
        .tree()
        .get(column)
        .ok()
        .and_then(|node| node.as_column());
    if let (Some(expected), Some(found)) = (
        expected_version,
        target.map(stokesforge_core::Column::version),
    ) {
        if found != expected {
            return Err(CoreError::ConcurrentModification {
                column,
                expected,
                found,
            }
            .into());
        }
    }
    if target.is_some_and(|column| column.meta().formula.is_some()) {
        return Err(CoreError::FormulaColumnReadOnly { column }.into());
    }
    if let Some((calculation, _)) = project.tree().calculation_of_column(column) {
        return Err(CoreError::CalculatedColumnReadOnly {
            column,
            calculation,
        }
        .into());
    }
    Ok(())
}

/// Écrit `values` et construit le [`DataDelta::Cells`] correspondant. Le bloc
/// est cloné avant application : c'est le `after` du delta, ce qu'il faudra
/// réécrire pour rétablir.
fn write_cells(
    project: &mut Project,
    column: NodeId,
    start: usize,
    values: ValueBlock,
) -> CommandResult<DataDelta> {
    let after = values.clone();
    let delta = project.set_values(column, start, values)?;
    Ok(DataDelta::Cells {
        column,
        start,
        before: delta.previous,
        before_length: delta.previous_length,
        after,
    })
}

/// Type et valeurs actuels d'une colonne (snapshot d'un changement de type).
/// Specs des graphes dont au moins une série « ligne » (`row = Some(r)`,
/// ancre `y` dans `sheet`) bouge selon `shift` (`Some(nouvelle ligne)` ou
/// `None` pour la laisser). Pur : lit l'arbre, ne mute rien. Une ancre
/// rompue (colonne absente) est ignorée. Rend `(graphe, avant, après)` pour
/// les seuls graphes modifiés.
fn shifted_row_specs(
    tree: &Tree,
    sheet: NodeId,
    shift: impl Fn(usize) -> Option<usize>,
) -> Vec<(NodeId, PlotSpec, PlotSpec)> {
    let mut out = Vec::new();
    for id in tree.walk_all() {
        let Ok(node) = tree.get(id) else { continue };
        let stokesforge_core::NodeKind::Graph { spec, .. } = node.kind() else {
            continue;
        };
        let mut after = spec.clone();
        let mut changed = false;
        for panel in after.panels.iter_mut() {
            for series in &mut panel.series {
                let Some(row) = series.row else { continue };
                if tree.parent(series.y).ok().flatten() != Some(sheet) {
                    continue;
                }
                if let Some(new_row) = shift(row) {
                    series.row = Some(new_row);
                    changed = true;
                }
            }
        }
        if changed {
            out.push((id, spec.clone(), after));
        }
    }
    out
}

/// Écrit les specs relevés par [`shifted_row_specs`] (par
/// `Project::set_plot_spec`, invariant 1) et rend le delta `plots`.
fn write_row_specs(
    project: &mut Project,
    moved: Vec<(NodeId, PlotSpec, PlotSpec)>,
) -> CoreResult<Vec<(NodeId, PlotSpec, PlotSpec)>> {
    for (graph, _, after) in &moved {
        project.set_plot_spec(*graph, after.clone())?;
    }
    Ok(moved)
}

fn column_state(project: &Project, column: NodeId) -> CoreResult<(ColumnType, ColumnValues)> {
    let node = project.tree().get(column)?;
    let target = node
        .as_column()
        .ok_or_else(|| CoreError::Structure(format!("{column} is not a column")))?;
    Ok((target.column_type(), target.values().clone()))
}

// ===========================================================================
// A7 — le compteur de version d'une colonne ne recule jamais (§F2)
// ===========================================================================
//
// `Entry::Structural::revert` remplace l'arbre entier (`Project::replace_tree`),
// ce qui restaure aussi le compteur de version de chaque colonne qu'il porte
// — voir la doctrine des estampilles du guide (§F2, tableau) : une version de
// contenu doit être monotone et ne jamais désigner deux contenus. Restaurer un
// ancien arbre peut faire reculer le compteur d'une colonne dont le contenu a,
// entre-temps, divergé (par une écriture normale, ou par l'annulation d'une
// écriture normale — « annuler est une écriture de plus », §6 du guide) :
// une écriture future repartant de ce numéro plus bas peut alors retomber sur
// un numéro déjà porté par un autre contenu de cette même colonne.
//
// Deux versions antérieures de ce correctif calculaient un numéro « frais »
// localement — d'abord depuis les deux arbres directement impliqués dans la
// restauration, puis depuis tout ce que l'historique retenait encore — et
// toutes deux se sont avérées insuffisantes : un balayage aléatoire
// (`b14_une_version_ne_designe_jamais_deux_contenus`) a trouvé qu'une
// écriture ORDINAIRE (jamais passée par un `revert`) peut, de son côté,
// faire progresser une colonne jusqu'à un numéro que ni l'un ni l'autre
// calcul local ne pouvait prévoir. Le seul calcul qui ferme la question pour
// de bon est un compteur unique tenu par `Project` lui-même
// (`Project::next_column_version`, doctrine des estampilles du guide :
// « attribuée par le projet ») et consulté par **toute** écriture de
// colonne, ordinaire ou restauration — jamais un calcul dérivé d'un
// sous-ensemble de ce que le projet a vu.

/// Ré-attribue un numéro de version **frais** (tiré de [`Project::
/// next_column_version`]) à chaque colonne dont le buffer n'est **pas** le
/// même objet dans `discarded` et `restored` — appelée juste après que
/// `revert` a remplacé l'arbre courant (`discarded`, celui qu'on quitte) par
/// `restored`.
///
/// Comparée par **identité de pointeur** (voir [`column_footprint`]), jamais
/// par égalité de valeurs : deux buffers distincts qui contiendraient par
/// hasard les mêmes octets restent deux allocations différentes, donc deux
/// moments distincts de l'histoire de cette colonne (la copie-à-l'écriture
/// en crée une nouvelle dès qu'un buffer partagé est modifié — voir
/// `Column::set_values`) ; son compteur a donc pu avancer sans que
/// `restored` (figé au moment où l'entrée a été empilée) le sache, et
/// pourrait reculer si on ne le touche pas. Une colonne dont le buffer est
/// resté le **même** objet dans les deux arbres n'a subi aucune écriture
/// entre les deux : rien n'a pu avancer, rien ne peut reculer, et la
/// laisser intacte évite un `Arc::make_mut` pour rien (D3 du guide veille
/// déjà sur ce genre de coût). Une colonne absente de `discarded` (elle
/// vient de réapparaître) ou de `restored` (elle disparaît) n'est jamais
/// concernée : voir le corps.
fn restamp_regressed_versions(
    project: &mut Project,
    discarded: &Tree,
    restored: &Tree,
) -> CommandResult<()> {
    let mut targets = Vec::new();
    for (id, node) in restored.iter() {
        let Some(restored_column) = node.as_column() else {
            continue;
        };
        // Absente de `discarded` : elle vient de réapparaître (annulation
        // d'une suppression, par exemple) — rien n'a pu s'écrire dans un
        // nœud qui n'existait pas encore, son compteur ne peut pas avoir
        // régressé.
        let Some(discarded_column) = discarded.node(id).and_then(|node| node.as_column()) else {
            continue;
        };
        let same_buffer = column_footprint(discarded_column.values()).0
            == column_footprint(restored_column.values()).0;
        if !same_buffer {
            targets.push(id);
        }
    }
    for id in targets {
        let fresh = project.next_column_version();
        project.restamp_column_version(id, fresh)?;
    }
    Ok(())
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
/// colonne mixte, une estimation (voir [`mixed_buffer_bytes`]), le coût
/// exact d'allocation par `String` n'étant pas modélisable indépendamment de
/// l'allocateur.
fn column_footprint(values: &ColumnValues) -> (usize, usize) {
    match values {
        ColumnValues::Float(arc) => (
            Arc::as_ptr(arc) as usize,
            arc.len() * std::mem::size_of::<f64>(),
        ),
        ColumnValues::Mixed(arc) => (Arc::as_ptr(arc) as usize, mixed_buffer_bytes(arc)),
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
        DataDelta::DeleteRows { removed, .. } | DataDelta::DeleteCells { removed, .. } => {
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
        // Métadonnées de fenêtre/note et spec de graphe : aucun buffer
        // partagé avec l'arbre à comptabiliser, contrairement à
        // `ColumnType` — juste quelques chaînes ou identités, jamais le
        // volume qui justifie ce mécanisme (colonnes de données). Même
        // choix que `InsertRows` ci-dessus.
        DataDelta::WindowMeta { .. }
        | DataDelta::NoteText { .. }
        | DataDelta::PlotSpec { .. }
        | DataDelta::SavedColors { .. } => {}
        // `ColumnMeta` rejoint `ColumnType` dès qu'elle porte `values_before`
        // (voir sa doc) : c'est alors, elle aussi, un `ColumnValues`
        // potentiellement partagé avec l'arbre courant.
        DataDelta::ColumnMeta { values_before, .. } => {
            if let Some((_, values)) = values_before {
                insert_column_footprint(&mut out, values);
            }
        }
    }
    out
}

/// Colonnes que `command` pourrait toucher, avec l'adresse et la taille du
/// buffer que chacune porte **avant** son application — un simple emprunt de
/// `tree`, jamais gardé vivant au-delà de cet appel (voir [`PriorColumns`] et
/// la doc de [`History::execute`], D3 (2ᵉ moitié) du guide).
///
/// Cible exactement la ou les colonnes que la commande *nomme* (`column`
/// pour `SetCells`/`SetColumnType`) ou, pour `InsertRows`/`DeleteRows` qui ne
/// connaissent leurs cibles exactes qu'une fois `Project::row_targets`
/// consulté (privé à `stokesforge-core`), **toutes** les colonnes de la feuille
/// visée — un sur-ensemble borné par la taille de cette feuille, jamais par
/// celle du projet. [`just_orphaned_by_write`] n'y cherchera ensuite que les
/// colonnes que le delta, une fois la commande appliquée, dit réellement
/// touchées : le sur-ensemble ne fait jamais basculer orphelin un buffer qui
/// ne l'est pas.
///
/// Vide pour une commande structurelle (elle n'est jamais appelée avec l'une
/// d'elles depuis [`History::execute`], mais rester total ici évite un
/// panique pour un coût nul).
fn prior_columns(tree: &Tree, command: &Command) -> PriorColumns {
    let mut out = PriorColumns::new();
    let mut record = |id: NodeId| {
        if let Some(column) = tree.node(id).and_then(|node| node.as_column()) {
            out.insert(id, column_footprint(column.values()));
        }
    };
    match command {
        Command::SetCells { column, .. }
        | Command::SetCellsPromoting { column, .. }
        | Command::SetColumnType { column, .. } => {
            record(*column);
        }
        Command::InsertRows { sheet, .. } | Command::DeleteRows { sheet, .. } => {
            if let Ok(children) = tree.children(*sheet) {
                for &id in children {
                    record(id);
                }
            }
        }
        Command::DeleteCells { columns, .. } => {
            for &id in columns {
                record(id);
            }
        }
        // Aucune de ces commandes ne cible une colonne dont il faudrait
        // relever le buffer avant application : les huit structurelles
        // n'atteignent jamais cette fonction (voir la doc ci-dessus), et les
        // quatre commandes de métadonnées/spec ne touchent jamais aux
        // valeurs d'une colonne — `SetColumnMeta` remplace `ColumnMeta`, pas
        // `ColumnValues`.
        Command::AddColumn { .. }
        | Command::DeleteColumn { .. }
        | Command::Rename { .. }
        | Command::Move { .. }
        | Command::AddFolder { .. }
        | Command::AddWorkbook { .. }
        | Command::AddEmptyWorkbook { .. }
        | Command::AddWorksheet { .. }
        | Command::DuplicateWorksheet { .. }
        | Command::InsertCopy { .. }
        | Command::AddGraph { .. }
        | Command::AddMatrix { .. }
        | Command::AddNote { .. }
        | Command::RemoveNode { .. }
        | Command::ReplaceAnalysisData { .. }
        | Command::AddCalculation { .. }
        | Command::RestoreOriginalData { .. }
        | Command::UpdateCalculation { .. }
        | Command::RemoveCalculation { .. }
        | Command::SetWindowMeta { .. }
        | Command::SetNoteText { .. }
        | Command::SetColumnMeta { .. }
        | Command::SetPlotSpec { .. }
        | Command::EditPlotSpec { .. }
        | Command::SetSavedColors { .. } => {}
    }
    out
}

/// Buffer(s) que les colonnes touchées par `delta` avaient dans `prior`
/// (capturé par [`prior_columns`] avant cette commande) — ce que la
/// copie-à-l'écriture vient de remplacer. Un buffer qui n'y figure pas
/// n'était pas déjà épinglé (voir [`History::pin_footprint`], qui ne fait
/// rien pour un pointeur inconnu) : le chercher est donc sûr même quand rien
/// n'a besoin de basculer orphelin.
///
/// Une seule recherche par identifiant dans `prior` par colonne touchée —
/// jamais un parcours de l'arbre — d'où le nom : `just_orphaned_by_write`
/// donne les buffers qui *viennent de* quitter l'arbre courant à l'instant
/// de cette écriture, pas tous ceux qu'une entrée plus ancienne retient.
fn just_orphaned_by_write(prior: &PriorColumns, delta: &DataDelta) -> Footprint {
    let mut out = Footprint::new();
    let previous_buffer_of = |out: &mut Footprint, column: NodeId| {
        if let Some(&(ptr, bytes)) = prior.get(&column) {
            out.insert(ptr, bytes);
        }
    };
    match delta {
        DataDelta::Cells { column, .. } | DataDelta::ColumnType { column, .. } => {
            previous_buffer_of(&mut out, *column);
        }
        DataDelta::InsertRows { columns, .. } => {
            for &column in columns {
                previous_buffer_of(&mut out, column);
            }
        }
        DataDelta::DeleteRows { removed, .. } | DataDelta::DeleteCells { removed, .. } => {
            for (column, _) in removed {
                previous_buffer_of(&mut out, *column);
            }
        }
        // Voir `prior_columns` et `data_delta_footprint` : ces quatre deltas
        // ne portent jamais de buffer de colonne, rien à orpheliner ici.
        DataDelta::WindowMeta { .. }
        | DataDelta::NoteText { .. }
        | DataDelta::ColumnMeta { .. }
        | DataDelta::PlotSpec { .. }
        | DataDelta::SavedColors { .. } => {}
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
        ValueBlock::Mixed(values) => (values.as_ptr() as usize, mixed_buffer_bytes(values)),
    }
}

/// Coût fixe par élément d'un buffer à types mixtes : la taille d'une
/// [`MixedCell`] dans le `Vec`, soit celle de sa plus grosse variante
/// (`String`, `{ptr, len, cap}`) plus le discriminant — un `f64` n'ouvre
/// aucune niche exploitable, contrairement au pointeur d'une `String`, donc
/// ce n'est pas la taille d'un `Option<String>`.
const MIXED_ELEMENT_OVERHEAD: usize = std::mem::size_of::<MixedCell>();

/// Estimation de la taille d'un buffer à types mixtes : somme des octets
/// UTF-8 des textes présents, plus [`MIXED_ELEMENT_OVERHEAD`] par cellule,
/// quelle que soit sa variante.
fn mixed_buffer_bytes(values: &[MixedCell]) -> usize {
    values.iter().fold(0usize, |total, cell| {
        total
            + MIXED_ELEMENT_OVERHEAD
            + match cell {
                MixedCell::Text(text) => text.len(),
                MixedCell::Empty | MixedCell::Number(_) => 0,
            }
    })
}
