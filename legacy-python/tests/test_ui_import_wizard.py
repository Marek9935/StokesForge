"""`ImportWizardDialog` : aperçu, overrides, import réel en arrière-plan."""

from __future__ import annotations

from pathlib import Path

import pytest

from plotyx.io.registry import default_registry
from plotyx.ui.import_wizard import ImportWizardDialog

FIXTURES = Path(__file__).parent / "fixtures" / "import"


@pytest.fixture
def wizard(qtbot, bus, sheet):
    def _make(paths=None):
        dialog = ImportWizardDialog(sheet, bus, paths, default_registry())
        qtbot.addWidget(dialog)
        return dialog

    return _make


def test_preview_populates_table_for_a_readable_file(qtbot, wizard):
    dialog = wizard([FIXTURES / "generic_named_header.csv"])
    assert dialog._preview_table.columnCount() == 2
    assert dialog._preview_table.rowCount() > 0
    assert dialog._preview_table.horizontalHeaderItem(0).text() == "Time"
    dialog.reject()


def test_preview_shows_structured_warnings(qtbot, wizard):
    dialog = wizard([FIXTURES / "jasco_npoints_mismatch.txt"])
    assert "NPOINTS" in dialog._warnings_label.text() or "99" in dialog._warnings_label.text()
    dialog.reject()


def test_preview_reports_unreadable_binary_file(qtbot, wizard):
    dialog = wizard([FIXTURES / "proprietary_binary.dat"])
    assert dialog._preview_table.columnCount() == 0
    assert dialog._warnings_label.text()
    dialog.reject()


def test_add_and_remove_files_refreshes_list(qtbot, wizard):
    dialog = wizard([])
    dialog.add_paths([FIXTURES / "generic_named_header.csv", FIXTURES / "rpe_nohead_tab.asc"])
    assert dialog._file_list.count() == 2
    dialog._file_list.setCurrentRow(0)
    dialog._on_remove_file()
    assert dialog._file_list.count() == 1
    dialog.reject()


def test_add_paths_ignores_duplicates(qtbot, wizard):
    path = FIXTURES / "generic_named_header.csv"
    dialog = wizard([path])
    dialog.add_paths([path])
    assert len(dialog.paths) == 1
    dialog.reject()


def test_import_applies_a_single_macro_and_closes(qtbot, wizard, bus, sheet):
    dialog = wizard(
        [FIXTURES / "generic_named_header.csv", FIXTURES / "jasco_uvvis_semicolon.csv"]
    )
    before = bus.count
    with qtbot.waitSignal(dialog.finished, timeout=5000):
        dialog._on_import()
    assert bus.count == before + 1  # une seule entrée d'historique pour les deux fichiers
    assert dialog.result() == 1  # QDialog.Accepted
    assert "Time" in sheet.column_names()


def test_import_with_no_files_warns_and_does_nothing(qtbot, wizard, monkeypatch, bus):
    dialog = wizard([])
    warned = {}
    monkeypatch.setattr(
        "plotyx.ui.import_wizard.QMessageBox.warning",
        lambda *a, **k: warned.setdefault("called", True),
    )
    before = bus.count
    dialog._on_import()
    assert warned.get("called")
    assert bus.count == before
    dialog.reject()


def test_cancel_import_adds_no_columns(qtbot, wizard, bus, sheet, tmp_path):
    big = tmp_path / "big.csv"
    lines = ["X,Y"] + [f"{i},{i * 2.0}" for i in range(200_000)]
    big.write_text("\n".join(lines) + "\n")

    dialog = wizard([big])
    dialog._current_options = lambda: {"chunk_size": 200}
    before_names = set(sheet.column_names())
    before_count = bus.count

    # Annule dès la première progression rapportée, avant la moindre commande.
    original_on_progress = dialog._on_progress

    def _on_progress_then_cancel(fraction: float) -> None:
        original_on_progress(fraction)
        dialog._on_cancel_import()

    dialog._on_progress = _on_progress_then_cancel
    dialog._on_import()

    with qtbot.waitSignal(dialog._worker.cancelled, timeout=15000):
        pass
    qtbot.wait(50)

    assert set(sheet.column_names()) == before_names
    assert bus.count == before_count
    dialog.reject()


def test_dialog_shutdown_cancels_a_running_worker(qtbot, wizard, tmp_path):
    big = tmp_path / "big.csv"
    lines = ["X,Y"] + [f"{i},{i * 2.0}" for i in range(200_000)]
    big.write_text("\n".join(lines) + "\n")
    dialog = wizard([big])
    dialog._current_options = lambda: {"chunk_size": 200}
    dialog._on_import()
    assert dialog._worker is not None
    dialog._shutdown_worker()
    assert dialog._worker.isFinished() or not dialog._worker.isRunning()
