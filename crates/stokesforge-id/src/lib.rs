//! Identity of the project tree nodes — [`NodeId`] alone, without the rest
//! of the model.
//!
//! # Why this crate exists (phase R3)
//!
//! `NodeId` lived in `stokesforge-core` until phase R3
//! (`docs/archive/newarchitecture/roadmap-rendu.md`), which had to make
//! `NodeKind::Graph` carry a `stokesforge_plot::PlotSpec`. `stokesforge-plot`
//! only refers to the model by `NodeId` — its only dependency until then was
//! the whole of `stokesforge-core`, for this one type. Making `stokesforge-core` depend on
//! `stokesforge-plot` **while keeping** `stokesforge-plot → stokesforge-core` would have been a
//! direct Cargo cycle (checked empirically: `cargo check` refuses with
//! *cyclic package dependency*, since Cargo does not allow circular
//! normal dependencies, unlike development dependencies).
//!
//! The clean way out is to move `NodeId` down — and only it, not
//! `IdAllocator`, which stays in `stokesforge-core` — below both crates:
//!
//! ```text
//! stokesforge-id  (this crate, zero dependencies)
//!   ↑              ↑
//! stokesforge-plot   stokesforge-core → stokesforge-plot
//! ```
//!
//! `stokesforge-core` re-exports this type unchanged (`pub use stokesforge_id::NodeId`):
//! no existing caller of `stokesforge_core::NodeId` has anything to
//! change. Only `stokesforge-plot` changes the address of its dependency, from
//! `stokesforge-core` (which gave it access to much more than `NodeId`) to this
//! minimal crate — a smaller surface, not a wider one.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

/// Identity of a node: a monotonic counter, never reused over the whole life
/// of a project (reloads included). Opaque to clients; it is also the key
/// the script API exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

impl NodeId {
    /// Rebuilds an identifier from its raw value (persistence only).
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw value (persistence only).
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_as_expected() {
        assert_eq!(NodeId::from_raw(412).to_string(), "node 412");
    }

    #[test]
    fn from_raw_and_raw_are_inverse() {
        let id = NodeId::from_raw(42);
        assert_eq!(NodeId::from_raw(id.raw()), id);
    }
}
