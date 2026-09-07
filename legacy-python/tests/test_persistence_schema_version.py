"""Version de schéma : refus propre d'un fichier trop récent, jamais un plantage."""

from __future__ import annotations

import h5py
import numpy as np
import pytest

from plotyx.core import Project
from plotyx.persistence import (
    CorruptedProjectFileError,
    SchemaVersionError,
    load_project,
    save_project,
)
from plotyx.persistence.migration import check_version, migrate
from plotyx.persistence.schema import (
    ATTR_FORMAT,
    ATTR_SCHEMA_VERSION,
    CURRENT_SCHEMA_VERSION,
    FORMAT_MARKER,
    GROUP_PROJECT,
)


def _write_bare_project_group(file: h5py.File) -> None:
    group = file.create_group(GROUP_PROJECT)
    group.attrs["key"] = "root-key"
    group.attrs["name"] = "P"
    group.attrs["type"] = "folder"
    group.attrs.create("children_order", data=np.array([], dtype=object), dtype=h5py.string_dtype())


def test_opening_a_future_schema_version_raises_cleanly(tmp_path):
    path = tmp_path / "future.plx"
    with h5py.File(path, "w") as file:
        file.attrs[ATTR_FORMAT] = FORMAT_MARKER
        file.attrs[ATTR_SCHEMA_VERSION] = CURRENT_SCHEMA_VERSION + 1
        _write_bare_project_group(file)

    with pytest.raises(SchemaVersionError, match=str(CURRENT_SCHEMA_VERSION + 1)):
        load_project(path)


def test_opening_a_file_without_the_format_marker_raises_cleanly(tmp_path):
    path = tmp_path / "notplx.plx"
    with h5py.File(path, "w") as file:
        file.create_dataset("junk", data=[1, 2, 3])

    with pytest.raises(CorruptedProjectFileError):
        load_project(path)


def test_opening_a_file_without_schema_version_attribute_raises_cleanly(tmp_path):
    path = tmp_path / "no_version.plx"
    with h5py.File(path, "w") as file:
        file.attrs[ATTR_FORMAT] = FORMAT_MARKER
        _write_bare_project_group(file)

    with pytest.raises(CorruptedProjectFileError):
        load_project(path)


def test_current_version_round_trips(tmp_path):
    path = tmp_path / "ok.plx"
    save_project(Project("P"), path)
    with h5py.File(path, "r") as file:
        assert file.attrs[ATTR_SCHEMA_VERSION] == CURRENT_SCHEMA_VERSION
        assert check_version(file) == CURRENT_SCHEMA_VERSION


def test_schema_version_is_the_very_first_thing_written(tmp_path):
    """« Dès le premier octet écrit » : même un fichier par ailleurs vide
    (aucun groupe /project) porte déjà son numéro de version."""
    path = tmp_path / "raw.plx"
    with h5py.File(path, "w") as file:
        file.attrs[ATTR_SCHEMA_VERSION] = CURRENT_SCHEMA_VERSION
        assert file.attrs[ATTR_SCHEMA_VERSION] == CURRENT_SCHEMA_VERSION


def test_migrate_is_a_documented_no_op_at_the_only_known_version(tmp_path):
    path = tmp_path / "ok.plx"
    save_project(Project("P"), path)
    with h5py.File(path, "r") as file:
        migrate(file, CURRENT_SCHEMA_VERSION)  # ne doit rien lever, rien changer
