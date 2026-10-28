//! Identity of the project tree nodes.
//!
//! `NodeId` itself lives in `stokesforge-id`, a minimal crate with no dependency:
//! see its top-level documentation for the reason (a Cargo cycle to break in
//! phase R3, between this crate and `stokesforge-plot`). This module only
//! re-exports it — `stokesforge_core::NodeId` names exactly the same type
//! as before, no caller has to change — and holds the allocator, which
//! stays here: it only needs to build `NodeId`s, never `stokesforge-plot`.
pub use stokesforge_id::NodeId;

use crate::error::{CoreError, CoreResult};

/// Identifier allocator: `next()` returns strictly increasing values.
/// It is never moved back — the `Project` (next slice) owns it,
/// outside the tree, so that an undo does not move it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    /// First identifier returned: `NodeId::from_raw(1)`. Zero is never assigned.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Resumes after a load. `hint` is the **next identifier to assign**, as
    /// [`IdAllocator::peek`] returns it and as persistence writes it; `highest_seen` is the
    /// largest identifier actually read from the file. The counter restarts at
    /// `max(hint, highest_seen + 1)` — the second half is a free safeguard against a file
    /// whose persisted counter is inconsistent with its content.
    pub fn resume(hint: u64, highest_seen: u64) -> Self {
        Self {
            next: hint.max(highest_seen.saturating_add(1)),
        }
    }

    /// Assigns the next identifier. `Err(IdentifiersExhausted)` if `u64`
    /// is exhausted, rather than overflowing silently (B1,
    /// `guide-de-correction-robustesse.md` §F2/F3/F4): before this fix,
    /// `self.next += 1` on `u64::MAX` overflowed — a panic in debug, a
    /// counter that **goes backwards** in release, opening the door to two nodes
    /// under the same identifier. `self.next` is incremented only after
    /// the check, so a failing call leaves the allocator unchanged
    /// (and every later call fails the same way, never a regression).
    // `IdAllocator` is not an iterator (there is no end, no `Iterator::next`
    // to honor); the name `next` stays the clearest for this API.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> CoreResult<NodeId> {
        let id = NodeId::from_raw(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or(CoreError::IdentifiersExhausted)?;
        Ok(id)
    }

    /// Value of the next identifier that would be returned (to persist).
    pub fn peek(&self) -> u64 {
        self.next
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_allocator_starts_at_one() {
        let mut alloc = IdAllocator::new();
        assert_eq!(alloc.next().unwrap().raw(), 1);
    }

    #[test]
    fn successive_ids_are_strictly_increasing() {
        let mut alloc = IdAllocator::new();
        let a = alloc.next().unwrap();
        let b = alloc.next().unwrap();
        assert!(b > a);
    }

    #[test]
    fn resume_uses_the_hint_when_nothing_contradicts_it() {
        // The file says "the next one is 10"; its largest identifier
        // read is 3, which contradicts nothing: the counter does restart at 10.
        let mut alloc = IdAllocator::resume(10, 3);
        assert_eq!(alloc.next().unwrap().raw(), 10);
    }

    #[test]
    fn resume_falls_back_to_the_belt_when_the_hint_is_stale() {
        // The persisted counter (3) is inconsistent with the content of the
        // file (largest identifier read: 10): it would return an
        // identifier already assigned. The safeguard catches it above.
        let mut alloc = IdAllocator::resume(3, 10);
        assert_eq!(alloc.next().unwrap().raw(), 11);
    }

    #[test]
    fn resume_of_an_empty_project_starts_at_one() {
        // Empty project: no identifier read, so `highest_seen == 0`
        // (zero is never assigned).
        let mut alloc = IdAllocator::resume(1, 0);
        assert_eq!(alloc.next().unwrap().raw(), 1);
    }

    #[test]
    fn resume_does_not_panic_at_the_u64_boundary() {
        // `highest_seen.saturating_add(1)` must saturate rather than overflow;
        // a `highest_seen` of `u64::MAX` is unreachable in practice, but
        // the crate forbids itself any panic.
        let alloc = IdAllocator::resume(u64::MAX, u64::MAX);
        assert_eq!(alloc.peek(), u64::MAX);
    }

    #[test]
    fn peek_does_not_consume() {
        let alloc = IdAllocator::new();
        assert_eq!(alloc.peek(), 1);
        assert_eq!(alloc.peek(), 1);
    }

    // `display_formats_as_expected` and `from_raw_and_raw_are_inverse` moved
    // to `stokesforge-id` with the type they test (phase R3).

    #[test]
    fn default_matches_new() {
        assert_eq!(IdAllocator::default(), IdAllocator::new());
    }
}
