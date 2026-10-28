//! Battery 6: concurrent access to the shared state (the session actor).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use stokesforge_commands::{Command, CommandError, MacroKind};
use stokesforge_core::{Column, ColumnType, CoreError, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, SessionError, SessionHandle};

fn session(column_count: usize, row_count: usize) -> (Session, NodeId, Vec<NodeId>) {
    let mut p = Project::new("C").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let mut cols = Vec::new();
    for i in 0..column_count {
        let row_values: Vec<f64> = (0..row_count).map(|r| (i * 1000 + r) as f64).collect();
        cols.push(
            p.add_column(
                sheet,
                None,
                Some(&format!("C{i}")),
                Column::from_f64(&row_values),
            )
            .unwrap(),
        );
    }
    let sheet_id = sheet;
    (Session::spawn(p, None).unwrap(), sheet_id, cols)
}

fn length_of(h: &SessionHandle, col: NodeId) -> usize {
    h.snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .len()
}

fn values_of(h: &SessionHandle, col: NodeId) -> Vec<f64> {
    match h
        .snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        stokesforge_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("text column"),
    }
}

#[test]
fn b6_concurrent_writes_stay_serialized() {
    let (s, _sheet, cols) = session(4, 50);
    let barrier = Arc::new(Barrier::new(8));
    let mut threads = Vec::new();
    for t in 0..8usize {
        let h = s.handle();
        let b = Arc::clone(&barrier);
        let cols = cols.clone();
        threads.push(std::thread::spawn(move || {
            b.wait();
            let mut ok = 0usize;
            for i in 0..200u64 {
                let col = cols[(t + i as usize) % cols.len()];
                let v = (t as f64) * 1e6 + i as f64;
                if h.execute(Command::SetCells {
                    column: col,
                    start: (i % 50) as usize,
                    values: ValueBlock::Float(vec![v]),
                    expected_version: None,
                })
                .is_ok()
                {
                    ok += 1;
                }
            }
            ok
        }));
    }
    let total: usize = threads.into_iter().map(|f| f.join().unwrap()).sum();
    assert_eq!(total, 8 * 200, "commands were lost");
    let h = s.handle();
    let snap = h.snapshot().unwrap();
    assert_eq!(
        snap.revision as usize, total,
        "the revision does not count the mutations"
    );
    for c in &cols {
        assert_eq!(length_of(&h, *c), 50, "a column changed length");
    }
    s.shutdown().unwrap();
}

#[test]
fn b6_snapshot_is_frozen_despite_the_writes() {
    let (s, _sheet, cols) = session(1, 1000);
    let h = s.handle();
    let snap = h.snapshot().unwrap();
    let before: Vec<f64> = match snap
        .tree
        .node(cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        stokesforge_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!(),
    };
    for i in 0..500u64 {
        h.execute(Command::SetCells {
            column: cols[0],
            start: i as usize,
            values: ValueBlock::Float(vec![-1.0]),
            expected_version: None,
        })
        .unwrap();
    }
    let after: Vec<f64> = match snap
        .tree
        .node(cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        stokesforge_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!(),
    };
    assert_eq!(before, after, "the snapshot moved under its holder's feet");
    assert!(snap.revision < h.snapshot().unwrap().revision);
    s.shutdown().unwrap();
}

#[test]
fn b6_one_clients_macro_swallows_another_clients_work() {
    // Limit documented originally at the top of `stokesforge-session`: a
    // macro was global to the session. Its exact consequence was measured
    // here.
    //
    // A4 (`guide-de-correction-robustesse.md` §F6), decided: each
    // `SessionHandle` obtained from an independent call to
    // `Session::handle()` carries a distinct identity (`ClientId`), and the
    // actor refuses (`CommandError::ForeignMacro`) that an `Execute` from a
    // sender other than the owner of the current macro be absorbed into it.
    // The assertion therefore changes shape: originally, the write by
    // `script` was accepted (absorbed into the macro of `ui`) then silently
    // undone by `abort_macro`, which was the bug. With the fix, that same
    // write is refused **when it is executed**, before even entering the
    // model: there is nothing left to undo. Both halves of the guarantee are
    // checked: the immediate refusal during another client's macro, then
    // that `script` works normally again once that macro is closed; its work
    // never depended on the fate of the macro of `ui`.
    let (s, _sheet, cols) = session(2, 3);
    let ui = s.handle();
    let script = s.handle();
    let initial_value = values_of(&script, cols[1])[0];

    ui.begin_macro(MacroKind::Import).unwrap();
    let refusal = script.execute(Command::SetCells {
        column: cols[1],
        start: 0,
        values: ValueBlock::Float(vec![42.0, 43.0, 44.0]),
        expected_version: None,
    });
    assert!(
        matches!(
            refusal,
            Err(SessionError::Command(CommandError::ForeignMacro("execute")))
        ),
        "the write by `script` should have been refused while the macro of `ui` is \
         open, not {refusal:?}"
    );
    assert_eq!(
        values_of(&script, cols[1])[0],
        initial_value,
        "a refused `Execute` should never have touched the model"
    );

    ui.abort_macro().unwrap();

    // `script` was never absorbed by the macro of `ui`: once that macro is
    // closed (here aborted), it retries and succeeds normally.
    script
        .execute(Command::SetCells {
            column: cols[1],
            start: 0,
            values: ValueBlock::Float(vec![42.0, 43.0, 44.0]),
            expected_version: None,
        })
        .unwrap();

    let after = values_of(&script, cols[1])[0];
    assert_eq!(
        after, 42.0,
        "the work of `script`, applied after the macro of `ui` was closed, must stay \
         at 42.0: it was never part of that macro and so must never be undone \
         by its abandonment (observed value: {after})"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_toctou_between_snapshot_and_command() {
    // Reproduces exactly the sequence of `stokesforge_script::bridge::set_column`:
    //   1. snapshot   2. read `Column::version`   3. send the command
    // Another client can slip in between 2 and 3.
    //
    // A2 (`guide-de-correction-robustesse.md` §F6), decided: compare
    // `Column::version` (which changes on ANY modification, not only a length
    // change), checked by the actor when it APPLIES the command, never
    // before. This reproduction originally carried a **length** check made by
    // the client before sending (the bug A2 fixes); its assertion then
    // waited for the column to change length to prove that this check had
    // been bypassed. The fix changes what the bridge checks: no longer the
    // length seen before sending, but `expected_version` checked by the actor
    // on application. The assertion therefore changes shape: we now check
    // that a write built on a stale `expected_version` is refused with the
    // dedicated error, and that the model did not move under that refusal.
    let (s, sheet, cols) = session(1, 10);
    let client = s.handle();
    let other = s.handle();
    let col = cols[0];

    let snap = client.snapshot().unwrap();
    let seen_column = snap.tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(seen_column.len(), 10);
    let seen_version = seen_column.version();
    let new_values: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();

    other
        .execute(Command::DeleteRows {
            sheet,
            at: 5,
            count: 5,
        })
        .unwrap();
    assert_eq!(length_of(&other, col), 5);

    let r = client.execute(Command::SetCells {
        column: col,
        start: 0,
        values: ValueBlock::Float(new_values),
        expected_version: Some(seen_version),
    });
    assert!(
        matches!(
            r,
            Err(SessionError::Command(CommandError::Core(
                CoreError::ConcurrentModification { .. }
            )))
        ),
        "the write built on version {seen_version} (that of the snapshot) should have been \
         refused for a concurrent version, not {r:?}"
    );

    let final_length = length_of(&client, col);
    assert_eq!(
        final_length, 5,
        "the refusal based on `expected_version` still let the column change \
         length: it went from 5 to {final_length} rows"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_stopped_actor_returns_disconnected_and_nothing_worse() {
    let (s, _sheet, cols) = session(1, 3);
    let h = s.handle();
    let project = s.shutdown().unwrap();
    assert_eq!(project.tree().len(), 4);
    assert!(matches!(h.snapshot(), Err(SessionError::Disconnected)));
    assert!(matches!(h.undo(), Err(SessionError::Disconnected)));
    assert!(matches!(h.redo(), Err(SessionError::Disconnected)));
    assert!(matches!(
        h.begin_macro(MacroKind::Import),
        Err(SessionError::Disconnected)
    ));
    assert!(matches!(h.end_macro(), Err(SessionError::Disconnected)));
    assert!(matches!(
        h.execute(Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![1.0]),
            expected_version: None,
        }),
        Err(SessionError::Disconnected)
    ));
}

#[test]
fn b6_concurrent_undo_does_not_double_count() {
    let (s, _sheet, cols) = session(1, 10);
    let h = s.handle();
    for i in 0..100u64 {
        h.execute(Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![i as f64]),
            expected_version: None,
        })
        .unwrap();
    }
    let undone = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(4));
    let mut threads = Vec::new();
    for _ in 0..4 {
        let h = s.handle();
        let b = Arc::clone(&barrier);
        let n = Arc::clone(&undone);
        threads.push(std::thread::spawn(move || {
            b.wait();
            while let Ok(Some(_)) = h.undo() {
                n.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for f in threads {
        f.join().unwrap();
    }
    assert_eq!(
        undone.load(Ordering::Relaxed),
        100,
        "four clients undoing at the same time did not undo exactly the 100 steps"
    );
    assert!(!h.snapshot().unwrap().history.can_undo);
    s.shutdown().unwrap();
}

/// Simulated handle: exactly what `stokesforge_script::surface::Handle`
/// remembers (`NodeId` + `epoch`), and exactly the check that
/// `stokesforge_script::bridge::{float_buffer, set_column}` apply **before**
/// looking up `id` in the current tree (A3,
/// `guide-de-correction-robustesse.md` §F2/F6). That module is private to the
/// `stokesforge-script` crate and behind the `python` feature, unreachable
/// from `stokesforge-robustesse` without it, so this reproduction replays the
/// same logic by hand, on the public primitives of `stokesforge-session`
/// (`Snapshot::epoch`, added by this fix), to stay runnable without the
/// embedded interpreter. It is faithful: the real bridge logic does nothing
/// more than this.
struct SimulatedHandle {
    id: NodeId,
    epoch: u64,
}

impl SimulatedHandle {
    fn resolve(h: &SessionHandle, id: NodeId) -> Self {
        let epoch = h.snapshot().unwrap().epoch;
        SimulatedHandle { id, epoch }
    }

    /// Writes through the command, **unless** the remembered epoch no longer
    /// matches that of the current snapshot: refusal **before** even looking
    /// at the tree, for the precise reason this reproduction exists: the
    /// `NodeId` may, by coincidence, name another object in the new project.
    fn set_column(&self, h: &SessionHandle, values: Vec<f64>) -> Result<(), &'static str> {
        let snapshot = h.snapshot().unwrap();
        if snapshot.epoch != self.epoch {
            return Err("stale epoch: a load has happened since this handle was obtained");
        }
        h.execute(Command::SetCells {
            column: self.id,
            start: 0,
            values: ValueBlock::Float(values),
            expected_version: None,
        })
        .map(|_| ())
        .map_err(|_| "refused by the actor")
    }
}

#[test]
fn b6_handle_survives_a_load_and_names_another_object() {
    // Two DIFFERENT projects of the same shape: the node identifiers are the
    // same (the allocator restarts from the file), the data is not.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("other.stf");
    {
        let mut p = Project::new("OTHER").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        // Same position, same identifier, entirely different content.
        p.add_column(
            sheet,
            None,
            Some("C0"),
            Column::from_f64(&[-7.0, -8.0, -9.0, -10.0, -11.0]),
        )
        .unwrap();
        p.add_column(sheet, None, Some("C1"), Column::from_f64(&[0.0; 5]))
            .unwrap();
        stokesforge_persistence::save(&p, &path).unwrap();
    }

    let (s, _sheet, cols) = session(2, 5);
    let client = s.handle();
    let col = cols[0];
    // The handle a script would hold: `resolve()` produces it before the
    // reload, with the epoch in force at that moment.
    let handle = SimulatedHandle::resolve(&client, col);

    assert_eq!(values_of(&client, col)[0], 0.0, "setup");

    // Another client (the UI, a File > Open menu) loads a project.
    client.load(&path).unwrap();

    // The model, for its part, neither refuses nor reports anything: the
    // `NodeId` now names a column of another project. That is precisely why
    // the check must be made by the handle, not hoped for from the model
    // (F6: "every assumption must travel in the request", and here the
    // SENDER carries the assumption, even before sending).
    let after = values_of(&client, col);
    println!("the NodeId now names: {after:?}");

    let r = handle.set_column(&client, vec![999.0]);
    assert!(
        r.is_err(),
        "a write through a handle from before the reload was ACCEPTED: \
         it would have written into the column \"{}\" of the freshly loaded project",
        client.snapshot().unwrap().tree.node(col).unwrap().name()
    );
    // The refusal must come from the epoch, **before** the command reaches
    // the actor, not from an accidental refusal by the model (which would
    // accept this write just fine, see `after` above).
    assert_eq!(
        r,
        Err("stale epoch: a load has happened since this handle was obtained")
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_write_to_a_node_deleted_by_another_client() {
    let (s, _sheet, cols) = session(2, 5);
    let a = s.handle();
    let b = s.handle();
    a.execute(Command::DeleteColumn { column: cols[0] })
        .unwrap();
    let r = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
        expected_version: None,
    });
    assert!(r.is_err(), "write accepted on a deleted column");
    a.undo().unwrap();
    let r2 = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
        expected_version: None,
    });
    assert!(
        r2.is_ok(),
        "the handle did not survive deletion + undo: {r2:?}"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_incompatible_type_between_two_clients() {
    let (s, _sheet, cols) = session(1, 5);
    let a = s.handle();
    let b = s.handle();
    a.execute(Command::SetColumnType {
        column: cols[0],
        column_type: ColumnType::Mixed,
    })
    .unwrap();
    let r = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
        expected_version: None,
    });
    assert!(
        r.is_err(),
        "a numeric block was written into a column that became mixed"
    );
    s.shutdown().unwrap();
}
