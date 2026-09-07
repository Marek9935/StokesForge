"""Performance de `WorksheetTableModel` sur 1 million de lignes × 20 colonnes.

`data()` doit rester à coût constant par cellule (lecture directe dans le
`ndarray` sous-jacent, sans copie de colonne) : ce test mesure des accès à
la volée façon défilement — plusieurs fenêtres de lignes, colonne par
colonne — plutôt que d'imprimer une impression subjective de fluidité.
"""

from __future__ import annotations

import time

import numpy as np
import pytest

from plotyx.commands import CommandBus
from plotyx.core.project import Project
from plotyx.ui.worksheet_model import WorksheetTableModel

ROWS = 1_000_000
COLUMNS = 20
VIEWPORT_ROWS = 50
N_SCROLL_STEPS = 200  # simule 200 positions de défilement différentes


@pytest.fixture(scope="module")
def big_sheet():
    project = Project("Perf")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    rng = np.arange(ROWS, dtype=np.float64)
    for i in range(COLUMNS):
        sheet.add_column(f"C{i}", rng)  # construction directe : pas une commande UI
    return project, sheet


def test_sheet_has_the_expected_shape(big_sheet):
    _project, sheet = big_sheet
    assert sheet.row_count == ROWS
    assert sheet.column_count == COLUMNS


def test_row_and_column_count_are_o1(qapp, big_sheet):
    """`rowCount`/`columnCount` ne doivent pas parcourir les colonnes."""
    project, sheet = big_sheet
    bus = CommandBus(project)
    model = WorksheetTableModel(sheet, bus)
    try:
        start = time.perf_counter()
        for _ in range(10_000):
            model.rowCount()
            model.columnCount()
        elapsed = time.perf_counter() - start
    finally:
        model.disconnect_signals()
    assert elapsed < 0.5, f"rowCount/columnCount trop lents : {elapsed:.3f}s pour 10000 appels"


def test_scrolling_the_full_sheet_reads_data_without_copying_columns(qapp, big_sheet):
    """Simule un défilement : `VIEWPORT_ROWS` × `COLUMNS` cellules lues à
    `N_SCROLL_STEPS` positions réparties sur toute la feuille (1M lignes)."""
    project, sheet = big_sheet
    bus = CommandBus(project)
    model = WorksheetTableModel(sheet, bus)
    try:
        offsets = np.linspace(0, ROWS - VIEWPORT_ROWS, N_SCROLL_STEPS, dtype=int)
        start = time.perf_counter()
        touched = 0
        for offset in offsets:
            for row in range(int(offset), int(offset) + VIEWPORT_ROWS):
                for col in range(COLUMNS):
                    model.data(model.index(row, col))
                    touched += 1
        elapsed = time.perf_counter() - start
    finally:
        model.disconnect_signals()

    total_cells = N_SCROLL_STEPS * VIEWPORT_ROWS * COLUMNS
    assert touched == total_cells
    per_cell_us = (elapsed / total_cells) * 1e6
    # Rapporté dans le compte rendu de phase : temps réel mesuré, pas une
    # impression. Le seuil est volontairement large (calcul headless, CI
    # partagée) — l'important est l'absence de dépendance à `ROWS`.
    assert elapsed < 5.0, (
        f"défilement simulé trop lent : {elapsed:.3f}s pour {total_cells} "
        f"cellules ({per_cell_us:.2f} µs/cellule)"
    )


def test_data_access_time_does_not_grow_with_row_offset(qapp, big_sheet):
    """Lire près du début ou près de la fin de 1M lignes doit coûter pareil —
    la preuve qu'il n'y a pas de parcours linéaire caché dans `data()`."""
    project, sheet = big_sheet
    bus = CommandBus(project)
    model = WorksheetTableModel(sheet, bus)
    try:

        def _time_window(start_row: int) -> float:
            t0 = time.perf_counter()
            for row in range(start_row, start_row + VIEWPORT_ROWS):
                for col in range(COLUMNS):
                    model.data(model.index(row, col))
            return time.perf_counter() - t0

        near_start = _time_window(0)
        near_end = _time_window(ROWS - VIEWPORT_ROWS - 1)
    finally:
        model.disconnect_signals()

    # Large marge : on ne cherche pas à prouver une constante stricte, mais
    # l'absence d'un facteur qui grossirait avec l'offset (O(n) caché).
    assert near_end < near_start * 20 + 0.05
