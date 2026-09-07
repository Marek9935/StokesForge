"""`WorksheetView` : copier/coller, sélection, menus contextuels.

Toujours la même règle d'or : chaque scénario de ce fichier tourne sous
`mutation_guard()`, colonne insérée/supprimée comprise, pour s'assurer que
la vue ne fait jamais qu'un seul geste — pousser une commande.

Chaque test montre réellement la vue (`show()`), ce qui est nécessaire
pour que les en-têtes calculent une géométrie exploitable (clic de section,
menu contextuel). La fixture ferme et dépile explicitement ce qu'elle a
créé plutôt que de compter sur le ramassage automatique en fin de session :
Qt ne détruit un widget marqué `deleteLater()` qu'au prochain passage de la
boucle d'évènements, et laisser plusieurs `QTableView` montrées s'empiler
jusqu'au `gc.collect()` forcé de fin de session pytest s'est avéré, à
l'usage, faire planter l'interpréteur (destruction Shiboken en cascade
dans un ordre que Qt n'attend pas).
"""

from __future__ import annotations

import numpy as np
import pytest
from PySide6.QtWidgets import QApplication

from plotyx.commands import mutation_guard
from plotyx.ui.worksheet_model import WorksheetTableModel
from plotyx.ui.worksheet_view import WorksheetView


@pytest.fixture
def view_model(qtbot, bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    view = WorksheetView(model)
    qtbot.addWidget(view)
    view.resize(400, 300)
    view.show()
    qtbot.waitExposed(view)
    yield view, model
    view.close()
    model.disconnect_signals()
    qtbot.wait(0)


def test_copy_selection_writes_tab_separated_text_to_the_clipboard(view_model):
    view, model = view_model
    view.setCurrentIndex(model.index(0, 0))
    view.selectionModel().select(
        model.index(0, 0), view.selectionModel().SelectionFlag.ClearAndSelect
    )
    view.selectionModel().select(model.index(0, 1), view.selectionModel().SelectionFlag.Select)
    view.copy_selection()
    text = QApplication.clipboard().text()
    assert "1.0" in text and "10.0" in text


def test_paste_at_current_writes_through_a_command_macro(view_model, bus, sheet):
    view, model = view_model
    QApplication.clipboard().setText("7\t8\n9\t10")
    view.setCurrentIndex(model.index(0, 0))
    with mutation_guard():
        view.paste_at_current()
    assert sheet.column("A").values[0] == 7.0
    assert sheet.column("B").values[0] == 8.0
    assert sheet.column("A").values[1] == 9.0
    assert sheet.column("B").values[1] == 10.0
    assert bus.can_undo
    bus.undo()  # toute la macro s'annule d'un coup
    assert sheet.column("A").values[0] == 1.0


def test_clear_selection_writes_missing_values_via_commands(view_model, sheet):
    view, model = view_model
    view.selectionModel().select(
        model.index(0, 0), view.selectionModel().SelectionFlag.ClearAndSelect
    )
    with mutation_guard():
        view.clear_selection()
    assert np.isnan(sheet.column("A").values[0])


def test_clear_selection_ignores_cells_beyond_column_length(view_model, bus):
    view, model = view_model
    # B n'a que 2 lignes ; la ligne 3 est hors de sa longueur, donc sans
    # commande à pousser (rien à effacer qui existe).
    view.selectionModel().select(
        model.index(3, 1), view.selectionModel().SelectionFlag.ClearAndSelect
    )
    with mutation_guard():
        view.clear_selection()
    assert bus.count == 0


def test_select_column_via_header_click(view_model):
    view, _model = view_model
    view.selectColumn(0)
    selected_columns = {index.column() for index in view.selectedIndexes()}
    assert selected_columns == {0}


def test_delete_key_clears_the_selection(view_model, sheet):
    from PySide6.QtCore import Qt
    from PySide6.QtTest import QTest

    view, model = view_model
    view.setCurrentIndex(model.index(0, 0))
    view.selectionModel().select(
        model.index(0, 0), view.selectionModel().SelectionFlag.ClearAndSelect
    )
    with mutation_guard():
        QTest.keyClick(view, Qt.Key.Key_Delete)
    assert np.isnan(sheet.column("A").values[0])


def _trigger_and_close(app, text_substring):
    """Programme le déclenchement d'une action de menu contextuel ouvert.

    `QMenu.exec()` bloque dans une boucle d'évènements modale : on ne peut
    pas simuler un vrai clic en headless, donc on programme, via un timer à
    délai nul, le déclenchement de l'action puis la fermeture du menu — ça
    s'exécute pendant que `exec()` tourne, exactement comme un clic le
    ferait.
    """
    from PySide6.QtCore import QTimer

    def _do():
        popup = app.activePopupWidget()
        assert popup is not None, "aucun menu contextuel ouvert"
        for action in popup.actions():
            if text_substring in action.text():
                action.trigger()
                popup.close()
                return
        popup.close()
        raise AssertionError(f"aucune action ne contient {text_substring!r}")

    QTimer.singleShot(0, _do)


def test_column_context_menu_delete_pushes_a_delete_command(view_model, sheet):
    view, _model = view_model
    _trigger_and_close(QApplication, "Supprimer")
    with mutation_guard():
        view._column_menu(view.horizontalHeader().rect().center())
    assert sheet.column_count == 1


def test_row_context_menu_insert_uses_a_per_column_macro(view_model, bus, sheet):
    view, _model = view_model
    before = bus.count
    _trigger_and_close(QApplication, "Insérer")
    with mutation_guard():
        view._row_menu(view.verticalHeader().rect().center())
    assert bus.count == before + 1  # une seule entrée d'historique : la macro
    assert sheet.row_count == 5
