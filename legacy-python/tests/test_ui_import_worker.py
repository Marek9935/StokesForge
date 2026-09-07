"""`ImportWorker` : parsing hors du thread UI, progression, annulation.

Premier test du motif `QThread` réutilisé en phase 8 — la logique de test
(démarrer, attendre un signal, vérifier l'état) sert de gabarit.
"""

from __future__ import annotations

from pathlib import Path

from plotyx.io.registry import default_registry
from plotyx.ui.import_worker import ImportWorker

FIXTURES = Path(__file__).parent / "fixtures" / "import"


def _write_big_csv(path: Path, rows: int) -> None:
    lines = ["X,Y"] + [f"{i},{i * 2.0}" for i in range(rows)]
    path.write_text("\n".join(lines) + "\n")


# -- appels directs à `run()` -------------------------------------------------
#
# `QThread.run()` s'exécute dans un vrai thread OS géré par Qt, pas par le
# module `threading` de Python : l'instrumentation de couverture par défaut
# (`sys.settrace` posé sur le thread principal) ne le voit pas. Les tests
# ci-dessus vérifient le comportement *réel* en thread séparé (le motif
# `qtbot.waitSignal`, à reproduire en phase 8) ; ceux-ci appellent `run()`
# directement, dans le thread de test, pour que la logique de contrôle
# (boucle, annulation précoce, erreur par fichier) soit mesurée.


def test_run_directly_processes_all_files():
    registry = default_registry()
    paths = [FIXTURES / "generic_named_header.csv", FIXTURES / "rpe_nohead_tab.asc"]
    worker = ImportWorker(registry, paths)
    events: list[tuple] = []
    worker.file_finished.connect(lambda p, r: events.append(("finished", p, r)))
    worker.finished_all.connect(lambda: events.append(("all",)))

    worker.run()

    assert ("all",) in events
    finished_paths = {p for event in events if event[0] == "finished" for p in (event[1],)}
    assert finished_paths == {str(p) for p in paths}


def test_run_directly_stops_immediately_if_cancelled_before_start():
    registry = default_registry()
    worker = ImportWorker(registry, [FIXTURES / "generic_named_header.csv"])
    worker.cancel()
    events: list[str] = []
    worker.cancelled.connect(lambda: events.append("cancelled"))
    worker.file_finished.connect(lambda *_: events.append("finished"))

    worker.run()

    assert events == ["cancelled"]


def test_run_directly_reports_a_fatal_error_and_continues(tmp_path, monkeypatch):
    registry = default_registry()
    good = FIXTURES / "generic_named_header.csv"
    bad = tmp_path / "boom.csv"
    bad.write_text("x,y\n1,2\n")
    original_parse = registry.parse

    def _parse(path, options=None):
        if Path(path) == bad:
            raise RuntimeError("panne simulée")
        return original_parse(path, options)

    monkeypatch.setattr(registry, "parse", _parse)
    worker = ImportWorker(registry, [bad, good])
    failures: list[tuple[str, str]] = []
    successes: list[str] = []
    worker.failed.connect(lambda p, m: failures.append((p, m)))
    worker.file_finished.connect(lambda p, _r: successes.append(p))

    worker.run()

    assert failures == [(str(bad), "panne simulée")]
    assert successes == [str(good)]


def test_worker_parses_multiple_files_and_emits_results(qtbot):
    registry = default_registry()
    paths = [
        FIXTURES / "generic_named_header.csv",
        FIXTURES / "jasco_uvvis_semicolon.csv",
    ]
    worker = ImportWorker(registry, paths)
    results = {}
    worker.file_finished.connect(lambda p, r: results.__setitem__(p, r))

    with qtbot.waitSignal(worker.finished_all, timeout=5000):
        worker.start()

    assert set(results) == {str(p) for p in paths}
    assert all(r.columns for r in results.values())
    worker.wait()


def test_worker_progress_reaches_one(qtbot, tmp_path):
    big = tmp_path / "big.csv"
    _write_big_csv(big, 4_000)
    registry = default_registry()
    worker = ImportWorker(registry, [big], {"chunk_size": 500})
    seen: list[float] = []
    worker.progress.connect(seen.append)

    with qtbot.waitSignal(worker.finished_all, timeout=10000):
        worker.start()

    assert seen
    assert seen[-1] == 1.0
    worker.wait()


def test_worker_can_be_cancelled_mid_stream(qtbot, tmp_path):
    big = tmp_path / "big.csv"
    _write_big_csv(big, 200_000)
    registry = default_registry()
    worker = ImportWorker(registry, [big], {"chunk_size": 200})

    def _cancel_on_first_progress(_fraction: float) -> None:
        worker.cancel()

    worker.progress.connect(_cancel_on_first_progress)
    finished = {"emitted": False}
    worker.file_finished.connect(lambda *_: finished.__setitem__("emitted", True))

    with qtbot.waitSignal(worker.cancelled, timeout=10000):
        worker.start()

    assert not finished["emitted"], "un import annulé ne doit produire aucun résultat exploité"
    worker.wait()


def test_worker_reports_fatal_errors_without_stopping_other_files(qtbot, tmp_path, monkeypatch):
    registry = default_registry()
    good = FIXTURES / "generic_named_header.csv"
    bad = tmp_path / "boom.csv"
    bad.write_text("x,y\n1,2\n")

    original_parse = registry.parse

    def _parse(path, options=None):
        if Path(path) == bad:
            raise RuntimeError("panne simulée")
        return original_parse(path, options)

    monkeypatch.setattr(registry, "parse", _parse)
    worker = ImportWorker(registry, [bad, good])
    failures = []
    successes = []
    worker.failed.connect(lambda p, m: failures.append((p, m)))
    worker.file_finished.connect(lambda p, r: successes.append(p))

    with qtbot.waitSignal(worker.finished_all, timeout=5000):
        worker.start()

    assert failures and failures[0][0] == str(bad)
    assert successes == [str(good)]
    worker.wait()
