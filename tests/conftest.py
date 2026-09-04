"""Fixtures communes.

Le seul point non évident est l'environnement graphique : une `QUndoStack`
est un `QObject`, ce qui suppose l'existence d'une `QCoreApplication`. Les
tests de la phase 2 s'appuient donc sur la fixture `qapp` de `pytest-qt`, et
la plateforme Qt est forcée en `offscreen` **avant** tout import de Qt, pour
que la suite tourne à l'identique sur une machine de dev et sur le runner
GitLab (qui n'a pas de serveur X).
"""

from __future__ import annotations

import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import numpy as np  # noqa: E402
import pytest  # noqa: E402

from plotyx.core import Column, ColumnRole, Project  # noqa: E402


@pytest.fixture
def project() -> Project:
    """Petit projet représentatif : deux feuilles, des colonnes hétérogènes."""
    project = Project("Demo")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("A", [1.0, 2.0, np.nan, 4.0], role=ColumnRole.X, unit="s")
    sheet.add_column("B", [10.0, 20.0])
    other = book.add_sheet("Sheet2")
    other.insert_column(0, Column("C", ["x", None, "z"], column_type="text"))
    return project


@pytest.fixture
def bus(qapp, project):
    """Bus branché sur le projet de la fixture `project`.

    `qapp` est demandé explicitement : une `QUndoStack` sans application Qt
    vivante est un piège à `RuntimeError` en fin de session.
    """
    from plotyx.commands import CommandBus

    return CommandBus(project)


@pytest.fixture
def sheet(project):
    return project.resolve_path("[Book1]Sheet1")
