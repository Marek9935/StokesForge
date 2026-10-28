//! Capture of an unknown enum variant at read time (C5, level 2,
//! `guide-de-correction-robustesse.md` §F3).
//!
//! A `.stf` file written by a newer version of StokesForge can carry
//! a variant that an older reader does not know (a new plot type,
//! a new node kind). Rather than failing the whole
//! deserialization — which `serde` does by default for an externally
//! tagged enum — `stokesforge-persistence` captures the variant name and its
//! encoded content, and carries them here so that the rest of the model (and
//! the display) can degrade gracefully instead of refusing the whole
//! file.
//!
//! This type knows neither `serde` nor the MessagePack format: `stokesforge-plot` and
//! `stokesforge-core` may not depend on them
//! (`tests/no_ui_dependency.rs`). It only carries opaque bytes, already
//! encoded as read from the file; only `stokesforge-persistence` knows how to
//! reinterpret them, on read as on rewrite — this is what guarantees
//! that an unknown variant survives two successive saves
//! unchanged (B20).

/// Raw name and content of an enum variant that this reader does not
/// know.
#[derive(Debug, Clone, PartialEq)]
pub struct RawVariant {
    /// Variant name as written in the file.
    pub name: String,
    /// Associated content, MessagePack-encoded as read — empty for a
    /// variant with no content (a future unit variant).
    pub payload: Vec<u8>,
}
