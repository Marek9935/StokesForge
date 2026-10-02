//! Type d'erreur unique du crate.

use std::path::PathBuf;

use stokesforge_core::CoreError;

/// Erreur de sauvegarde ou de chargement d'un [`stokesforge_core::Project`].
///
/// Ne dérive pas `PartialEq` : `std::io::Error` ne l'implémente pas. Les
/// tests comparent les variantes avec `matches!`.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// Échec d'entrée/sortie sur le fichier.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// Chemin visé par l'opération.
        path: PathBuf,
        /// Erreur système d'origine.
        #[source]
        source: std::io::Error,
    },
    /// Le fichier ne commence pas par la signature `STKF` : ce n'est pas un
    /// fichier StokesForge (ou il est vide / tronqué avant la signature).
    #[error("{path} is not a StokesForge file")]
    NotAStokesForgeFile {
        /// Chemin du fichier lu.
        path: PathBuf,
    },
    /// Le fichier porte un numéro de version supérieur à celui que ce
    /// lecteur sait comprendre. Le corps n'a **pas** été lu.
    #[error("{path} was written by a newer version (format {found}, reader supports {supported})")]
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
    #[error("corrupt file {path}: {detail}")]
    Corrupt {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Description de l'anomalie.
        detail: String,
    },
    /// Le contenu lu ne forme pas un modèle valide (`Tree::from_nodes` ou
    /// `Node::with_parent` l'a refusé).
    #[error("invalid model in {path}: {source}")]
    InvalidModel {
        /// Chemin du fichier lu.
        path: PathBuf,
        /// Erreur du cœur qui a refusé la reconstruction.
        #[source]
        source: CoreError,
    },
    /// Le corps ne s'est pas décodé à cause d'une **variante d'énumération**
    /// que ce lecteur ne connaît pas (un type de trace, un type de colonne, …
    /// ajouté par une version plus récente de StokesForge) — distinct de
    /// [`Corrupt`](Self::Corrupt) : le fichier n'est pas endommagé, il est
    /// simplement plus récent que ce lecteur. C5 (niveau 1,
    /// `guide-de-correction-robustesse.md` §F3) : reconnaître ce cas plutôt
    /// que de le confondre avec une vraie corruption ; la dégradation
    /// gracieuse (garder ce qui se comprend, niveau 2) reste un chantier
    /// séparé.
    #[error("{path} was written by a newer version of StokesForge (unknown variant): {detail}")]
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
        "{path} changed since this process last loaded or saved it \
         (expected save counter {expected}, found {found}): reload before overwriting"
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
