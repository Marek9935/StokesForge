//! Header reading and routing of the body by version (S4, #323).
//!
//! The body is never decoded without going through [`decode_body`]: it is the
//! only place that knows a version. The day the format changes meaning
//! (removal, rename, change of meaning of a field), `FORMAT_VERSION`
//! goes up, the type of the old version is copied here (`schema/v1.rs`…) and
//! its `match` arm migrates it to the current `FileBody`. `docs/persistence.md`,
//! « Version rule », says when to bump the number and when to stay additive.

use crate::schema::{FileBody, FORMAT_VERSION, MAGIC};

/// What the header of a file says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Header {
    /// No `STKF` signature.
    NotStokesForge,
    /// Signature present, but not the four version bytes.
    Truncated,
    /// Version 0: never existed.
    ZeroVersion,
    /// Version newer than this reader.
    TooNew(u32),
    /// Version that this reader knows how to decode.
    Known(u32),
}

/// Reads the header (signature, then little-endian `u32` version).
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

/// Bytes of the body, after the header.
pub(crate) fn payload(bytes: &[u8]) -> &[u8] {
    &bytes[MAGIC.len() + 4..]
}

/// Failure to decode a body.
#[derive(Debug)]
pub(crate) enum BodyError {
    /// Invalid MessagePack for this version.
    Decode(rmp_serde::decode::Error),
    /// Version without a decoder: cannot happen after [`read_header`].
    UnknownVersion(u32),
}

/// Decodes the body written by version `version` into the current `FileBody`.
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
