//! Battery 9: the script thread under stress: corruption through a masked
//! array, race with the interface, interruption, and assorted pressures.
#![cfg(feature = "python")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use stokesforge_commands::Command;
use stokesforge_core::{Column, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_script::{ScriptEngine, ScriptError};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    _session: Session,
    handle: SessionHandle,
    engine: ScriptEngine,
    a: NodeId,
    b: NodeId,
    sheet: NodeId,
}

static BENCH: OnceLock<std::sync::Mutex<Bench>> = OnceLock::new();

fn bench() -> &'static std::sync::Mutex<Bench> {
    BENCH.get_or_init(|| {
        let mut p = Project::new("PY").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let a = p
            .add_column(
                sheet,
                None,
                Some("A"),
                Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            )
            .unwrap();
        let b = p
            .add_column(sheet, None, Some("B"), Column::from_f64(&vec![0.0; 1000]))
            .unwrap();
        p.add_column(sheet, None, Some("Disposable"), Column::from_f64(&[1.0]))
            .unwrap();
        let session = Session::spawn(p, None).unwrap();
        let handle = session.handle();
        let python = EmbeddedPython::locate().expect("vendored interpreter not found");
        let engine = ScriptEngine::spawn(handle.clone(), python).expect("engine start");
        std::sync::Mutex::new(Bench {
            _session: session,
            handle,
            engine,
            a,
            b,
            sheet,
        })
    })
}

fn run(source: &str) -> stokesforge_script::ScriptOutcome {
    let g = bench().lock().unwrap();
    g.engine.run(source).expect("the engine accepts the work")
}

fn ids() -> (NodeId, NodeId, NodeId) {
    let g = bench().lock().unwrap();
    (g.a, g.b, g.sheet)
}

fn session_handle() -> SessionHandle {
    bench().lock().unwrap().handle.clone()
}

fn values_of(col: NodeId) -> Vec<f64> {
    match session_handle()
        .snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("text column"),
    }
}

fn reset_column(col: NodeId, v: Vec<f64>) {
    session_handle()
        .execute(Command::SetCells {
            column: col,
            start: 0,
            values: ValueBlock::Float(v),
            expected_version: None,
        })
        .unwrap();
}

// ======================================================== masked array

#[test]
fn b9_masked_array_is_refused_rather_than_discovered() {
    // A1 (`guide-de-correction-robustesse.md` §F1), decided: a value that
    // cannot be represented in the model's convention is REFUSED, never
    // silently converted. A `np.ma.masked_array` that carries at least one
    // masked value is therefore refused by `pyst.set_column`, including here
    // where the masked positions already coincide with `NaN`s. The old
    // version of this test accepted the write (it only checked that the
    // accepted result did contain `NaN`s), which has not been the wanted
    // behavior since A1.
    let (a, _b, _s) = ids();
    reset_column(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!A')\n\
         measures = np.ma.masked_invalid(np.array([10.0, np.nan, 30.0, np.nan, 50.0]))\n\
         print('what Python sees  :', list(measures.filled(np.nan)))\n\
         print('data under mask   :', list(measures.data))\n\
         try:\n\
         \x20   pyst.set_column(h, measures)\n\
         \x20   print('refused: no')\n\
         except pyst.InvalidValueError as exc:\n\
         \x20   print('refused: yes', exc)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(
        out.stdout.contains("refused: yes"),
        "a masked array must be refused, even when the masked positions \
         coincide with NaNs already present: {}",
        out.stdout
    );
    let v = values_of(a);
    println!("what the model holds: {v:?}");
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "the refused write must not have changed anything in the model: {v:?}"
    );
    reset_column(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
}

#[test]
fn b9_masked_array_overwrites_missing_cells_with_values() {
    // A1: THE case that matters: a mask placed on otherwise valid data,
    // numpy's standard idiom for "measure to discard"
    // (`np.ma.masked_where(quality < 3, measures)`). Fixed: refused by
    // `pyst.set_column` (`pyst.InvalidValueError`), rather than written as
    // ordinary data.
    let (a, _b, _s) = ids();
    reset_column(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!A')\n\
         measures = np.array([10.0, 20.0, 30.0, 40.0, 50.0])\n\
         quality = np.array([5, 1, 5, 1, 5])\n\
         m = np.ma.masked_where(quality < 3, measures)\n\
         print('mask        :', list(m.mask))\n\
         print('seen from Python:', str(m))\n\
         print('mean        :', float(m.mean()))\n\
         try:\n\
         \x20   pyst.set_column(h, m)\n\
         \x20   print('refused: no')\n\
         except pyst.InvalidValueError as exc:\n\
         \x20   print('refused: yes', exc)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(
        out.stdout.contains("refused: yes"),
        "the two masked measures (20 and 40) must be refused, not \
         written as ordinary data: {}",
        out.stdout
    );
    let v = values_of(a);
    println!("what the model holds: {v:?}");
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "the refused write must not have changed anything in the model: {v:?}"
    );
    reset_column(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
}

// ============================================================ concurrency

#[test]
fn b9_race_between_script_and_interface_on_the_length() {
    // The script writes a whole column in a loop; the interface inserts and
    // deletes rows at the same time. The contract of `pyst.set_column` is
    // "never lengthens or truncates". It is checked continuously.
    //
    // A write whose column has changed since the read is refused at once with
    // `ConcurrentWriteError` (`docs/python-and-scripting.md`, "Read version"):
    // that is the contract, not an anomaly. The script counts it as a
    // refusal, like `InvalidValueError`. The number of refusals depends on
    // scheduling and is not asserted.
    //
    // The interface, for its part, can be refused: as long as the script
    // holds its macro open, the actor refuses the commands of another client
    // (`ForeignMacro("execute")`, `stokesforge-session`, header table). An
    // accepted deletion whose reinsertion is refused therefore leaves one row
    // fewer, through no fault of the script (CI finding of 2026-10-26: 999
    // rows). The oracle counts what the interface actually succeeded at: the
    // length must equal 1000 - deletions + insertions, exactly.
    //
    // Separate sheet: the interface inserts and deletes rows AT SHEET LEVEL.
    // On the bench sheet, it would shift and lengthen the columns `A` and
    // `Disposable` that the following tests read ("expected length 1001 ...
    // got 1000", CI finding of 2026-10-26, tag rc1): this test did not run
    // as long as it was `#[ignore]`.
    let (_a, _b, bench_sheet) = ids();
    let h0 = session_handle();
    let book = h0
        .snapshot()
        .unwrap()
        .tree
        .parent(bench_sheet)
        .unwrap()
        .unwrap();
    h0.execute(Command::AddWorksheet {
        book,
        name: Some("Race".into()),
    })
    .unwrap();
    let sheet = h0.snapshot().unwrap().tree.resolve("[Book1]Race").unwrap();
    assert_ne!(sheet, bench_sheet);
    h0.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Race".into()),
        column: Column::from_f64(&vec![0.0; 1000]),
    })
    .unwrap();
    let b = h0
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Book1]Race!Race")
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let violations = Arc::new(AtomicUsize::new(0));
    let stop2 = Arc::clone(&stop);
    let viol2 = Arc::clone(&violations);
    let h = session_handle();
    let shaker = std::thread::spawn(move || {
        let mut rounds = 0usize;
        let (mut removed, mut reinserted) = (0i64, 0i64);
        while !stop2.load(Ordering::Relaxed) {
            if h.execute(Command::DeleteRows {
                sheet,
                at: 0,
                count: 1,
            })
            .is_ok()
            {
                removed += 1;
            }
            if h.execute(Command::InsertRows {
                sheet,
                at: 0,
                count: 1,
            })
            .is_ok()
            {
                reinserted += 1;
            }
            let n = h
                .snapshot()
                .unwrap()
                .tree
                .node(b)
                .map(|n| n.as_column().unwrap().len())
                .unwrap_or(0);
            if n != 1000 {
                viol2.fetch_add(1, Ordering::Relaxed);
            }
            rounds += 1;
        }
        (rounds, removed - reinserted)
    });

    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Race!Race')\n\
         refused = 0\n\
         for i in range(400):\n\
         \x20   v = pyst.column(h)\n\
         \x20   try:\n\
         \x20       pyst.set_column(h, np.asarray(v, dtype=np.float64) + 1.0)\n\
         \x20   except (pyst.InvalidValueError, pyst.ConcurrentWriteError):\n\
         \x20       refused += 1\n\
         print('refused', refused)\n");
    stop.store(true, Ordering::Relaxed);
    let (rounds, net_removed) = shaker.join().unwrap();
    println!(
        "{} rounds of the interface, script output: {}",
        rounds,
        out.stdout.trim()
    );
    assert!(out.error.is_none(), "the script blew up: {:?}", out.error);
    let n = values_of(b).len();
    let expected = 1000 - net_removed;
    let drift = n as i64 - expected;
    println!(
        "length {n}, expected {expected} (interface: {net_removed} row(s) fewer), drift {drift:+}"
    );
    h0.execute(Command::DeleteColumn { column: b }).unwrap();
    assert_eq!(
        n as i64, expected,
        "the column has {n} rows after the race, {expected} expected ({drift:+}): \
         the script (\"never lengthens or truncates\", exact length checked before \
         sending) must not change its size, only the interface can, \
         by what it succeeded at"
    );
}

#[test]
fn b9_handle_to_a_deleted_then_restored_column() {
    let h = session_handle();
    // Column created here, not in the bench: the other tests insert and
    // delete rows AT SHEET LEVEL, which would empty a shared short column and
    // blur the observation.
    let (_a, _b, sheet) = ids();
    h.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Disposable2".into()),
        column: Column::from_f64(&[1.0, 2.0, 3.0]),
    })
    .unwrap();
    let disposable = h
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Book1]Sheet1!Disposable2")
        .unwrap();
    let out = run("h = pyst.resolve('[Book1]Sheet1!Disposable2')\nprint('resolved')\n");
    assert!(out.error.is_none(), "{:?}", out.error);

    h.execute(Command::DeleteColumn { column: disposable })
        .unwrap();
    let out = run(
        "try:\n    pyst.column(h)\n    print('READ DESPITE THE DELETION')\n\
         except pyst.BrokenReferenceError:\n    print('BrokenReferenceError')\n",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(
        out.stdout.contains("BrokenReferenceError"),
        "a deleted column is still readable: {}",
        out.stdout
    );

    h.undo().unwrap();
    let out = run("print([float(x) for x in pyst.column(h)])\n");
    assert!(
        out.error.is_none(),
        "the handle did not survive the undo: {:?}",
        out.error
    );
    println!("after undo: {}", out.stdout.trim());
    assert_eq!(
        out.stdout.trim(),
        "[1.0, 2.0, 3.0]",
        "the column restored by the undo did not get its values back"
    );
}

// ============================================================ interruption

#[test]
fn b9_interruption_of_a_pure_python_loop() {
    let g = bench().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        s.spawn(|| {
            // Waits for the script to really have entered, then interrupts.
            let started = std::time::Instant::now();
            while !engine.is_running() && started.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            engine.interrupt();
        });
        let out = engine
            .run("i = 0\nwhile True:\n    i += 1\n")
            .expect("accepted");
        assert!(out.interrupted, "the script was not marked as interrupted");
        assert!(
            out.error
                .as_deref()
                .unwrap_or("")
                .contains("KeyboardInterrupt"),
            "unexpected error: {:?}",
            out.error
        );
    });
    // And the engine must stay usable afterwards.
    let out = engine.run("print('alive')").expect("accepted");
    assert!(
        out.error.is_none(),
        "the engine is broken after the interruption: {:?}",
        out.error
    );
    assert_eq!(out.stdout.trim(), "alive");
    assert!(
        !out.interrupted,
        "the interruption spilled over to the next script"
    );
}

#[test]
fn b9_interruption_of_a_loop_that_swallows_exceptions() {
    let g = bench().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        s.spawn(|| {
            let started = std::time::Instant::now();
            while !engine.is_running() && started.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            engine.interrupt();
        });
        // A loop that swallows everything BUT touches `pyst`: the first half of
        // the mechanism (flag checked by every call) must catch it.
        let out = engine
            .run(
                "while True:\n\
                 \x20   try:\n\
                 \x20       pyst.resolve('[Book1]Sheet1!A')\n\
                 \x20   except pyst.UnknownPathError:\n\
                 \x20       pass\n",
            )
            .expect("accepted");
        assert!(
            out.interrupted,
            "a loop that swallows exceptions but calls pyst was not stopped"
        );
    });
    let out = engine.run("print('still alive')").expect("accepted");
    assert!(out.error.is_none(), "{:?}", out.error);
}

#[test]
fn b9_engine_refuses_a_second_concurrent_script() {
    let g = bench().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        let h = s.spawn(|| {
            let started = std::time::Instant::now();
            while !engine.is_running() && started.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            let r = engine.run("print('intruder')");
            let verdict = matches!(r, Err(ScriptError::Busy));
            engine.interrupt();
            verdict
        });
        let _ = engine.run("i = 0\nwhile True:\n    i += 1\n");
        assert!(
            h.join().unwrap(),
            "a second script was accepted during the first"
        );
    });
}

// ================================================================ miscellaneous

#[test]
fn b9_kept_view_retains_the_old_buffer() {
    // Documented consequence: a view left in a variable retains the previous
    // buffer. It is checked that this is a stale value and never an invalid
    // access, on a buffer big enough for it to be visible.
    let (_a, b, _s) = ids();
    reset_column(b, vec![0.0; 1000]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!B')\n\
         kept = pyst.column(h)\n\
         sum_before = float(np.sum(kept))\n\
         pyst.set_column(h, np.ones(1000))\n\
         print('sum of the kept view :', float(np.sum(kept)))\n\
         print('sum reread           :', float(np.sum(pyst.column(h))))\n\
         print('unchanged :', float(np.sum(kept)) == sum_before)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(out.stdout.contains("unchanged : True"), "{}", out.stdout);
    assert!(
        out.stdout.contains("sum reread           : 1000.0"),
        "{}",
        out.stdout
    );
    reset_column(b, vec![0.0; 1000]);
}

#[test]
fn b9_empty_column_and_large_column() {
    let h = session_handle();
    let (_a, _b, sheet) = ids();
    // Empty column: the numpy view must be an empty array, not an error.
    h.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Empty".into()),
        column: Column::new(ColumnType::Numeric),
    })
    .unwrap();
    let out = run("import numpy as np\n\
         v = pyst.column(pyst.resolve('[Book1]Sheet1!Empty'))\n\
         print('size', v.size, 'dtype', v.dtype)\n\
         pyst.set_column(pyst.resolve('[Book1]Sheet1!Empty'), np.zeros(0))\n\
         print('empty write ok')\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout.trim());
    assert!(out.stdout.contains("size 0"), "{}", out.stdout);
}

#[test]
fn b9_script_that_keeps_hundreds_of_views() {
    let (_a, b, _s) = ids();
    reset_column(b, vec![0.0; 1000]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!B')\n\
         views = []\n\
         for i in range(300):\n\
         \x20   views.append(pyst.column(h))\n\
         \x20   pyst.set_column(h, np.full(1000, float(i)))\n\
         print('views kept :', len(views))\n\
         print('first :', float(views[0][0]), 'last :', float(views[-1][0]))\n\
         views.clear()\n\
         print('released')\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(out.stdout.contains("released"));
}

#[test]
fn b9_host_error_does_not_leave_the_engine_in_a_shaky_state() {
    for _ in 0..50 {
        let out = run(
            "try:\n    pyst.resolve('[Missing]X!Y')\nexcept pyst.UnknownPathError:\n    pass\n",
        );
        assert!(out.error.is_none(), "{:?}", out.error);
    }
    let out = run("print('still there')");
    assert_eq!(out.stdout.trim(), "still there");
}

#[test]
fn b9_swallowed_interruption_also_undoes_the_graph_and_the_values() {
    // Clean bench: do not hold the lock of the shared bench while waiting for
    // the engine.
    let mut project = Project::new("Stop").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1., 2.]))
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let handle = actor.handle();
    let before = handle.snapshot().unwrap();
    let engine =
        Arc::new(ScriptEngine::spawn(handle.clone(), EmbeddedPython::locate().unwrap()).unwrap());
    let worker = Arc::clone(&engine);
    let job = std::thread::spawn(move || {
        worker
            .run(
                r#"
a = pyst.resolve('[Book1]Sheet1!A')
try:
    pyst.set_values(a, [9.,10.,11.])
    g = pyst.plot(a, name='To undo')
    pyst.add_shape(g, 'rectangle', 0, 0, 1, 1)
    while True: pass
except KeyboardInterrupt:
    print('interruption swallowed')
"#,
            )
            .unwrap()
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = handle.snapshot().unwrap();
        if snapshot.tree.resolve("[To undo]").ok().is_some_and(|id| {
            matches!(snapshot.tree.node(id).unwrap().kind(), stokesforge_core::NodeKind::Graph { spec, .. } if !spec.panels[0].shapes.is_empty())
        })        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "graph and object not created"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    engine.interrupt();
    let outcome = job.join().unwrap();
    assert!(outcome.interrupted);
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.stdout.contains("interruption swallowed"));
    let after = handle.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    let followup = engine
        .run("assert pyst.values(pyst.resolve('[Book1]Sheet1!A')) == (1.,2.)")
        .unwrap();
    assert!(followup.error.is_none());
}
