"""Autosave périodique et recovery — hors du fichier projet de l'utilisateur."""

from __future__ import annotations

import uuid

import pytest

from plotyx.core import Project
from plotyx.persistence import (
    autosave_dir,
    autosave_path,
    autosave_source,
    discard_autosave,
    list_recoverable,
    recover,
    write_autosave,
)


@pytest.fixture(autouse=True)
def isolated_autosave_dir(tmp_path, monkeypatch):
    """Chaque test a son propre répertoire d'autosave : la machine de dev
    (ou le runner CI, en cas d'exécution parallèle) ne doit jamais faire
    fuiter un fichier d'un test vers un autre."""
    monkeypatch.setenv("PLOTYX_AUTOSAVE_DIR", str(tmp_path / "autosave"))
    return tmp_path


def test_autosave_dir_is_created_on_demand(isolated_autosave_dir):
    directory = autosave_dir()
    assert directory.is_dir()
    assert directory == isolated_autosave_dir / "autosave"


def test_write_autosave_creates_a_recoverable_file(isolated_autosave_dir):
    session = uuid.uuid4().hex
    project = Project("Demo")
    path = write_autosave(project, session, None)
    assert path.exists()
    assert path == autosave_path(session)


def test_written_autosave_is_a_faithful_snapshot(isolated_autosave_dir):
    session = uuid.uuid4().hex
    project = Project("Demo")
    project.add_workbook("Book1").sheet("Sheet1").add_column("A", [1.0, 2.0])
    path = write_autosave(project, session, None)
    recovered = recover(path)
    assert recovered.find_window("Book1") is not None


def test_list_recoverable_excludes_the_given_session(isolated_autosave_dir):
    mine = uuid.uuid4().hex
    theirs = uuid.uuid4().hex
    write_autosave(Project("Mine"), mine, None)
    write_autosave(Project("Theirs"), theirs, None)

    all_files = list_recoverable()
    assert len(all_files) == 2

    excluding_mine = list_recoverable(exclude_session=mine)
    assert autosave_path(mine) not in excluding_mine
    assert autosave_path(theirs) in excluding_mine


def test_discard_autosave_removes_only_that_sessions_file(isolated_autosave_dir):
    mine = uuid.uuid4().hex
    theirs = uuid.uuid4().hex
    write_autosave(Project("Mine"), mine, None)
    write_autosave(Project("Theirs"), theirs, None)

    discard_autosave(mine)

    assert not autosave_path(mine).exists()
    assert autosave_path(theirs).exists()


def test_discard_autosave_is_silent_when_nothing_to_discard(isolated_autosave_dir):
    discard_autosave(uuid.uuid4().hex)  # ne doit pas lever


def test_autosave_source_records_the_original_path(isolated_autosave_dir, tmp_path):
    session = uuid.uuid4().hex
    source = tmp_path / "real_project.plx"
    path = write_autosave(Project("Demo"), session, source)
    assert autosave_source(path) == str(source)


def test_autosave_source_is_empty_for_a_never_saved_project(isolated_autosave_dir):
    session = uuid.uuid4().hex
    path = write_autosave(Project("Demo"), session, None)
    assert autosave_source(path) == ""
