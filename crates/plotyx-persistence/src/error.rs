//! Type d'erreur unique du crate.

use std::path::PathBuf;

use plotyx_core::CoreError;

/// Erreur de sauvegarde ou de chargement d'un [`plotyx_core::Project`].
///
/// Ne dérive pas `PartialEq` : `std::io::Error` ne l'implémente pas. Les
/// tests comparent les variantes avec `matches!`.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// Échec d'entrée/sortie sur le fichier.
    #[error("entrée/sortie sur {path} : {source}")]
    Io {
        /// Chemin visé par l'opération.
        path: PathBuf,
        /// Erreur système d'origine.
        #[source]
        source: std::io::Error,
    },
    /// Le fichier ne commence pas par la signature `PLYX` : ce n'est pas un
    /// fichier PlotYx (ou il est vide / tronqué avant la signature).
    #[error("{path} n'est pas un fichier PlotYx")]
    NotAPlotyxFile {
        /// Chemin du fichier lu.
        path: PathBuf,
    },
    /// Le fichier porte un numéro de version supérieur à celui que ce
    /// lecteur sait comprendre. Le corps n'a **pas** été lu.
    #[error(
        "{path} a été écrit par une version plus récente (format {found}, lecteur {supported})"
    )]
    UnsupportedVersion {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Numéro de version trouvé dans le fichier.
        found: u32,
        /// Numéro de version le plus élevé que ce lecteur sait décoder.
        supported: u32,
    },
    /// Le corps du fichier n'a pas pu être décodé (MessagePack invalide,
    /// tronqué, ou buffer de colonne `Float` de longueur non multiple de 8).
    #[error("fichier corrompu {path} : {detail}")]
    Corrupt {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Description de l'anomalie.
        detail: String,
    },
    /// Le contenu lu ne forme pas un modèle valide (`Tree::from_nodes` ou
    /// `Node::with_parent` l'a refusé).
    #[error("modèle invalide dans {path} : {source}")]
    InvalidModel {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Erreur du cœur qui a refusé la reconstruction.
        #[source]
        source: CoreError,
    },
    /// Le corps ne s'est pas décodé à cause d'une **variante d'énumération**
    /// que ce lecteur ne connaît pas (un type de trace, un type de colonne, …
    /// ajouté par une version plus récente de PlotYx) — distinct de
    /// [`Corrupt`](Self::Corrupt) : le fichier n'est pas endommagé, il est
    /// simplement plus récent que ce lecteur. C5 (niveau 1,
    /// `guide-de-correction-robustesse.md` §F3) : reconnaître ce cas plutôt
    /// que de le confondre avec une vraie corruption ; la dégradation
    /// gracieuse (garder ce qui se comprend, niveau 2) reste un chantier
    /// séparé.
    #[error(
        "{path} a été écrit par une version plus récente de PlotYx (variante inconnue) : {detail}"
    )]
    UnknownVariant {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Message d'origine de `rmp_serde`/`serde`, qui nomme la variante.
        detail: String,
    },
    /// `save` a détecté que le fichier visé a été modifié — par une autre
    /// fenêtre, un script, ou tout autre écrivain — depuis la dernière fois
    /// que CE processus l'a chargé ou sauvegardé. B8
    /// (`guide-de-correction-robustesse.md` §F2) : sans ce contrôle,
    /// l'écriture écraserait silencieusement le travail de l'autre écrivain.
    #[error(
        "{path} a changé depuis le dernier chargement/sauvegarde par ce processus \
         (compteur de sauvegarde attendu {expected}, trouvé {found}) : recharger avant \
         d'écraser"
    )]
    ConcurrentModification {
        /// Chemin du fichier visé par `save`.
        path: PathBuf,
        /// Compteur de sauvegarde que ce processus attendait.
        expected: u64,
        /// Compteur de sauvegarde réellement présent sur le disque.
        found: u64,
    },
}

/// Alias de résultat du crate.
pub type PersistenceResult<T> = Result<T, PersistenceError>;
