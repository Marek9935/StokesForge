//! Shared mechanics for the graceful degradation of an unknown enum variant
//! (C5, level 2, `guide-de-correction-robustesse.md` §F3).
//!
//! # The principle: capture by value first, choose afterwards
//!
//! `serde` normally derives a decoding of an externally-tagged enum that
//! **cannot recover**: as soon as the variant name read matches none of the
//! known variants, the failure comes straight back up — this was the bug
//! (C5, level 1 had already named it without fixing it: the error became
//! `UnknownVariant` rather than `Corrupt`, but the file was still lost
//! entirely).
//!
//! The fix: first decode the MessagePack value that follows the variant
//! name into a generic tree (`rmpv::Value`, which can represent
//! any value of the format without knowing the schema), re-encode it, then
//! *try* to decode it again as one of the known variants
//! — via `rmp_serde`, not `rmpv::ext::from_value`: the latter
//! represents Rust enums by variant index (`[index, content]`),
//! a different convention from the one this format writes
//! (« externally tagged » by name, `schema.rs` head doc); only `rmp_serde` —
//! the same decoder as for the rest of the file — can read back what this
//! format actually wrote. If this attempt fails — unknown variant, or
//! an unknown plot type buried three levels deeper in the spec
//! of a graph — the raw name and content are kept as they are in
//! a [`stokesforge_plot::RawVariant`] rather than making the caller fail.
//! This is what bounds the loss to **this single node** (or this single series)
//! instead of the whole file.
//!
//! # The accepted trade-off
//!
//! Going through `rmpv::Value` then re-encoding before retrying `rmp_serde`
//! copies the bytes of a `Float` column (a `Vec<u8>` already present in the
//! file) once or twice more than a direct decoding: a `load()`
//! allocates and copies the measurements of a column several times rather than
//! once. Nothing in this repository measures the cost of `load()`/
//! `save()` today (B17, `b4_allocator.rs` count other operations); this
//! is therefore not a fix that breaks an existing guard, only a
//! cost to watch if the format ends up carrying much larger
//! columns.
//!
//! # How the node becomes the same bytes again at the next save
//!
//! [`RawVariant::payload`] carries the associated content, already encoded in
//! MessagePack — empty for a variant without content (a future unit
//! variant, serialized as a plain string). `serialize_raw_variant`
//! re-injects these bytes as they are (decoded again into `rmpv::Value`, then
//! serialized by this same `Value`, which produces the same canonical encoding
//! as the one that wrote them — it is the same library on both sides):
//! nothing is renamed, reordered or normalized, which is the condition of
//! B20 (two successive saves byte-identical).

use serde::de::{DeserializeOwned, Error as _};
use serde::ser::{Error as _, SerializeMap};
use serde::{Deserialize, Deserializer, Serializer};
use stokesforge_plot::RawVariant;

/// Result of reading the tag of an externally-tagged enum: either a
/// plain string (unit variant, without content), or a single-entry
/// map `{name: content}`.
enum Tag {
    Bare(String),
    Tagged(String, rmpv::Value),
}

fn split_tag(value: &rmpv::Value) -> Result<Tag, String> {
    match value {
        rmpv::Value::String(name) => {
            let name = name
                .as_str()
                .ok_or_else(|| "variant name is not UTF-8".to_string())?;
            Ok(Tag::Bare(name.to_string()))
        }
        rmpv::Value::Map(entries) if entries.len() == 1 => {
            let (key, content) = &entries[0];
            let name = key
                .as_str()
                .ok_or_else(|| "variant key is not text".to_string())?;
            Ok(Tag::Tagged(name.to_string(), content.clone()))
        }
        other => Err(format!(
            "unexpected externally-tagged variant shape: {other:?}"
        )),
    }
}

/// Decodes an externally-tagged enum by first trying `Known` (today's
/// schema); on failure, captures the raw name and content in a
/// [`RawVariant`] rather than propagating the error.
pub(crate) fn deserialize_with_fallback<'de, D, Known, Public>(
    deserializer: D,
) -> Result<Public, D::Error>
where
    D: Deserializer<'de>,
    Known: DeserializeOwned + Into<Public>,
    Public: From<RawVariant>,
{
    let value = rmpv::Value::deserialize(deserializer)?;
    // Canonical re-encoding of the captured value, then a new attempt
    // through the real `rmp_serde` decoder (see the module head doc):
    // it, not `rmpv`, can interpret the externally-tagged tag
    // by name that this format writes everywhere else.
    //
    // The value is freed **before** the attempt: a column of several
    // hundred MB thus does not coexist in three copies (value, re-encoded
    // bytes, known record). The rare failure path re-reads the
    // value from the re-encoded bytes (#316).
    let Ok(reencoded) = rmp_serde::to_vec_named(&value) else {
        return raw_variant(&value)
            .map(Public::from)
            .map_err(D::Error::custom);
    };
    drop(value);
    if let Ok(known) = rmp_serde::from_slice::<Known>(&reencoded) {
        return Ok(known.into());
    }
    let value: rmpv::Value = rmp_serde::from_slice(&reencoded).map_err(D::Error::custom)?;
    drop(reencoded);
    raw_variant(&value)
        .map(Public::from)
        .map_err(D::Error::custom)
}

/// Raw name and content of a variant that cannot be read.
fn raw_variant(value: &rmpv::Value) -> Result<RawVariant, String> {
    let (name, payload) = match split_tag(value)? {
        Tag::Bare(name) => (name, Vec::new()),
        Tag::Tagged(name, content) => {
            let bytes = rmp_serde::to_vec_named(&content).map_err(|err| err.to_string())?;
            (name, bytes)
        }
    };
    Ok(RawVariant { name, payload })
}

/// Rewrites a captured variant identically: empty `payload` → the plain
/// string of the name (unit variant); otherwise → `{name: content}`, the content
/// being decoded again from the raw bytes then serialized by the given
/// `Serializer` (the caller's, not necessarily `rmp_serde` — `rmpv::Value`
/// serializes to any `serde` format).
pub(crate) fn serialize_raw_variant<S: Serializer>(
    raw: &RawVariant,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if raw.payload.is_empty() {
        return serializer.serialize_str(&raw.name);
    }
    let content: rmpv::Value = rmp_serde::from_slice(&raw.payload).map_err(S::Error::custom)?;
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry(&raw.name, &content)?;
    map.end()
}
