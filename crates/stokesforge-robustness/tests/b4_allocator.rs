//! Battery 4: the identifier allocator at the u64 boundary.

use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use stokesforge_core::IdAllocator;
use stokesforge_persistence::load;
use tempfile::TempDir;

#[test]
fn b4_allocator_saturates_then_refuses() {
    // B1 (`guide-de-correction-robustesse.md` §F2/F3/F4): `next()` now
    // returns `CoreResult<NodeId>`: at the `u64::MAX` boundary, it refuses
    // instead of silently overflowing (`self.next += 1` on `u64::MAX`, which
    // panicked in debug and made the counter go **backwards** in release:
    // two nodes under the same identifier).
    //
    // `resume` saturates on purpose (existing unit test of the crate:
    // `resume_does_not_panic_at_the_u64_boundary`, which only calls `peek`).
    let mut alloc = IdAllocator::resume(u64::MAX, u64::MAX);
    assert_eq!(alloc.peek(), u64::MAX);
    let first = alloc.next();
    println!("next() at the u64::MAX boundary → {first:?}");
    assert!(
        first.is_err(),
        "the allocator must refuse rather than overflow or go backwards: {first:?}"
    );
    // Refuses again, without having moved: never a regression, never a
    // reassignment.
    let second = alloc.next();
    assert!(
        second.is_err(),
        "an exhausted allocator stays exhausted: {second:?}"
    );
}

// --- Minimal schema duplicated from `stokesforge-persistence`, to forge a
// `.stf` file ---------------------------------------------------------------
//
// Same legitimacy as in `b3_hostile_files.rs` (see its note at the top of
// the file): the format is self-describing (named maps), so a declaration
// that agrees on the field names produces the same bytes as the real schema.
// Only the strict minimum is taken here (a root `Folder` and a child
// `Folder`) to carry a node identifier beyond the read bound.
#[derive(Serialize)]
struct FileBody {
    root: u64,
    next_id_hint: u64,
    nodes: Vec<NodeRecord>,
}
#[derive(Serialize)]
struct NodeRecord {
    id: u64,
    name: String,
    parent: Option<u64>,
    kind: KindRecord,
}
#[derive(Serialize)]
enum KindRecord {
    Folder { children: Vec<u64> },
}

fn write_file(dir: &TempDir, name: &str, body: &FileBody) -> PathBuf {
    let payload = rmp_serde::to_vec_named(body).unwrap();
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"STKF").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    f.sync_all().unwrap();
    path
}

// -- Why the earlier reproduction in this file was replaced --
//
// This spot used to hold `b4_project_loaded_with_max_id_then_creation`
// (B1, `guide-de-correction-robustesse.md` §F3/F4): a `Project::from_tree`
// built **directly in memory** (`Tree::from_nodes`/`Node::new`, without going
// through the disk) with a node of identifier `u64::MAX`, then an
// `add_folder` that was expected to succeed.
//
// This scenario can no longer be reached by a real file since B1 was fixed
// on both sides:
// - `IdAllocator::next` (`stokesforge-core/src/id.rs`) refuses via
//   `checked_add` at the `u64::MAX` boundary instead of overflowing or going
//   backwards;
// - `stokesforge_persistence::load` (`lib.rs`, `check_node_id`) refuses on
//   read any node identifier (or `next_id_hint`) beyond `MAX_NODE_ID`
//   (10¹²), very far from `u64::MAX`, but well before the file could bring
//   into memory a `Project` with an identifier near the real boundary.
//
// A real `.stf` file can therefore no longer produce the state that the old
// test built by hand: the "permanent refusal to write" it observed was not
// a bug: if `u64::MAX` is already taken, mathematically nothing is left to
// hand out. What remains to be guaranteed, and what the test below checks
// instead, is that the path that *could* bring such an identifier into
// memory, a tampered file, is cut on read, cleanly (an error, not a panic,
// not a success).
#[test]
fn b4_a_node_identifier_beyond_the_bound_is_refused_on_read() {
    let dir = TempDir::new().unwrap();

    // `MAX_NODE_ID` (`stokesforge-persistence/src/lib.rs`) is private to the
    // crate; it is reproduced here literally: it is the bound documented in
    // the error message checked below, not an implementation detail we guess.
    const MAX_NODE_ID: u64 = 1_000_000_000_000;
    let out_of_range_id = MAX_NODE_ID + 1;

    // The root and `next_id_hint` stay well below the bound: only the child
    // node exceeds it, to isolate precisely what `check_node_id` must catch
    // on a node identifier (not on the counter).
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".into(),
                parent: None,
                kind: KindRecord::Folder {
                    children: vec![out_of_range_id],
                },
            },
            NodeRecord {
                id: out_of_range_id,
                name: "A".into(),
                parent: Some(1),
                kind: KindRecord::Folder { children: vec![] },
            },
        ],
    };
    let path = write_file(&dir, "out_of_range_id.stf", &body);

    // `load` must refuse cleanly: no panic (this is the actor thread in a
    // session), no success (that would reopen exactly the scenario B1
    // closed). If `MAX_NODE_ID` did not exist or were not applied to every
    // node identifier (and not only to `next_id_hint`), this file would be
    // accepted and this assertion would fail: that is what makes this test a
    // real guard, not a test that passes by construction.
    let result = std::panic::catch_unwind(|| load(&path));
    assert!(
        result.is_ok(),
        "load() panicked on a node identifier beyond MAX_NODE_ID"
    );
    let result = result.unwrap();
    let error = match result {
        Err(e) => e,
        Ok(_) => panic!(
            "a node identifier ({out_of_range_id}) beyond MAX_NODE_ID \
             ({MAX_NODE_ID}) was ACCEPTED on read"
        ),
    };
    println!("out-of-range identifier on read → {error}");
    let message = error.to_string();
    assert!(
        message.contains(&MAX_NODE_ID.to_string()) && message.contains("limit"),
        "the error message should cite the accepted bound ({MAX_NODE_ID}): {message}"
    );
}
