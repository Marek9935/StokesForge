//! Lecture de l'en-tête et aiguillage du corps selon la version (S4, #323).
//!
//! Le corps ne se décode jamais sans passer par [`decode_body`] : c'est le
//! seul endroit qui connaît une version. Le jour où le format change de sens
//! (retrait, renommage, changement de sens d'un champ), `FORMAT_VERSION`
//! monte, le type de l'ancienne version est copié ici (`schema/v1.rs`…) et
//! son bras du `match` le migre vers le `FileBody` courant. `docs/persistance.md`,
//! « Règle de version », dit quand monter le numéro et quand rester additif.

use crate::schema::{FileBody, FORMAT_VERSION, MAGIC};

/// Ce que dit l'en-tête d'un fichier.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Header {
    /// Pas de signature `STKF`.
    NotStokesForge,
    /// Signature présente, mais pas les quatre octets de version.
    Truncated,
    /// Version 0 : n'a jamais existé.
    ZeroVersion,
    /// Version plus récente que ce lecteur.
    TooNew(u32),
    /// Version que ce lecteur sait décoder.
    Known(u32),
}

/// Lit l'en-tête (signature, puis version `u32` little-endian).
pub(crate) fn read_header(bytes: &[u8]) -> Header {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC.as_slice() {
        return Header::NotStokesForge;
    }
    let Some(raw) = bytes.get(MAGIC.len()..MAGIC.len() + 4) else {
        return Header::Truncated;
    };
    let mut version_bytes = [0u8; 4];
    version_bytes.copy_from_slice(raw);
    match u32::from_le_bytes(version_bytes) {
        0 => Header::ZeroVersion,
        found if found > FORMAT_VERSION => Header::TooNew(found),
        found => Header::Known(found),
    }
}

/// Octets du corps, après l'en-tête.
pub(crate) fn payload(bytes: &[u8]) -> &[u8] {
    &bytes[MAGIC.len() + 4..]
}

/// Échec du décodage d'un corps.
#[derive(Debug)]
pub(crate) enum BodyError {
    /// MessagePack invalide pour cette version.
    Decode(rmp_serde::decode::Error),
    /// Version sans décodeur : ne peut pas arriver après [`read_header`].
    UnknownVersion(u32),
}

/// Décode le corps écrit par la version `version` en `FileBody` courant.
pub(crate) fn decode_body(version: u32, payload: &[u8]) -> Result<FileBody, BodyError> {
    match version {
        1 => rmp_serde::from_slice::<FileBody>(payload).map_err(BodyError::Decode),
        other => Err(BodyError::UnknownVersion(other)),
    }
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::Decode(source) => source.fmt(f),
            BodyError::UnknownVersion(version) => {
                write!(f, "no decoder for format version {version}")
            }
        }
    }
}
