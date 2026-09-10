//! Les huit primitives du socle.
//!
//! Une commande est une **valeur**, pas un objet à état : elle ne retient ni
//! `Project`, ni `Tree`, ni le moindre `Arc`. C'est ce qui lui permet de
//! traverser un canal inter-threads telle quelle, de l'UI vers l'acteur qui
//! détient l'unique `&mut Project` du processus.
//!
//! Tout ce qu'il faut pour annuler est calculé **à l'application**, par
//! `History`, pas porté par la commande.

use plotyx_core::{Column, ColumnType, NodeId, Tree, ValueBlock};

/// Une des huit mutations du modèle exprimables par l'utilisateur.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Écrit `values` dans `column` à partir de la ligne `start`, en
    /// allongeant la colonne si besoin.
    SetCells {
        /// Colonne visée.
        column: NodeId,
        /// Première ligne écrite.
        start: usize,
        /// Bloc écrit ; sa variante doit correspondre au type de la colonne.
        values: ValueBlock,
    },
    /// Ajoute une colonne à `sheet`. `index: None` → en fin ;
    /// `name: None` → premier nom libre façon tableur.
    AddColumn {
        /// Feuille d'accueil.
        sheet: NodeId,
        /// Position d'insertion, ou fin de feuille.
        index: Option<usize>,
        /// Nom demandé, ou nom automatique.
        name: Option<String>,
        /// Contenu initial de la colonne.
        column: Column,
    },
    /// Supprime une colonne et ses valeurs.
    DeleteColumn {
        /// Colonne à supprimer.
        column: NodeId,
    },
    /// Renomme un nœud quelconque (colonne, feuille, classeur, dossier...).
    Rename {
        /// Nœud à renommer.
        id: NodeId,
        /// Nouveau nom.
        name: String,
    },
    /// Déplace un nœud sous un nouveau parent, à une position donnée.
    Move {
        /// Nœud à déplacer.
        id: NodeId,
        /// Parent d'accueil.
        new_parent: NodeId,
        /// Position dans les enfants du parent d'accueil.
        index: usize,
    },
    /// Change le type d'une colonne (conversion destructrice des valeurs).
    SetColumnType {
        /// Colonne visée.
        column: NodeId,
        /// Type demandé.
        column_type: ColumnType,
    },
    /// Insère des lignes au niveau feuille (colonnes telles que `at < len`).
    InsertRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Ligne d'insertion.
        at: usize,
        /// Nombre de lignes insérées.
        count: usize,
    },
    /// Supprime des lignes au niveau feuille (colonnes telles que `at < len`).
    DeleteRows {
        /// Feuille visée.
        sheet: NodeId,
        /// Première ligne supprimée.
        at: usize,
        /// Nombre de lignes supprimées.
        count: usize,
    },
}

impl Command {
    /// Libellé d'historique, en français, à calculer sur l'arbre **avant**
    /// application : c'est là que les noms visés existent encore (une colonne
    /// supprimée n'a plus de nom après coup).
    ///
    /// Cette fonction ne peut pas échouer : si le nœud est inconnu, elle rend
    /// un libellé générique (« Saisie », « Renommer en Z »...). C'est
    /// l'application de la commande qui échouera, avec la bonne erreur ; un
    /// libellé n'est pas l'endroit où signaler un problème.
    pub fn label(&self, tree: &Tree) -> String {
        match self {
            Command::SetCells { column, .. } => match name_of(tree, *column) {
                Some(name) => format!("Saisie dans {name}"),
                None => "Saisie".to_string(),
            },
            Command::AddColumn { .. } => "Ajouter une colonne".to_string(),
            Command::DeleteColumn { column } => match name_of(tree, *column) {
                Some(name) => format!("Supprimer la colonne {name}"),
                None => "Supprimer la colonne".to_string(),
            },
            Command::Rename { id, name } => match name_of(tree, *id) {
                Some(previous) => format!("Renommer {previous} en {name}"),
                None => format!("Renommer en {name}"),
            },
            Command::Move { id, .. } => match name_of(tree, *id) {
                Some(name) => format!("Déplacer {name}"),
                None => "Déplacer".to_string(),
            },
            Command::SetColumnType { column, .. } => match name_of(tree, *column) {
                Some(name) => format!("Changer le type de {name}"),
                None => "Changer le type".to_string(),
            },
            Command::InsertRows { count, .. } => {
                format!("Insérer {count} {}", rows_word(*count))
            }
            Command::DeleteRows { count, .. } => {
                format!("Supprimer {count} {}", rows_word(*count))
            }
        }
    }
}

/// Nom d'un nœud s'il existe dans cet arbre.
fn name_of(tree: &Tree, id: NodeId) -> Option<&str> {
    tree.node(id).map(plotyx_core::Node::name)
}

/// « ligne » ou « lignes » : en français, 0 et 1 restent au singulier.
fn rows_word(count: usize) -> &'static str {
    if count > 1 {
        "lignes"
    } else {
        "ligne"
    }
}
