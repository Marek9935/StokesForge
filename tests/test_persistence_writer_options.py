"""Options d'écriture : compression, colonnes vides, dataset vs groupe."""

from __future__ import annotations

import h5py
import numpy as np

from plotyx.core import Project
from plotyx.persistence import load_project, save_project
from tests.model_state import assert_same_state, snapshot


def _spectroscopy_like_project(n: int = 50_000) -> Project:
    """Signal synthétique façon spectro : baseline lente + pics + bruit."""
    rng = np.random.default_rng(0)
    x = np.linspace(0, 1000, n)
    y = 100 + 0.001 * x
    for center, amp, width in [(200, 500, 5), (450, 800, 3), (700, 300, 8)]:
        y += amp * np.exp(-0.5 * ((x - center) / width) ** 2)
    y += rng.normal(0, 0.5, n)

    project = Project("Spectro")
    sheet = project.add_workbook("Book1").sheet("Sheet1")
    sheet.add_column("wavelength", x)
    sheet.add_column("intensity", y)
    return project


def test_gzip_shuffle_meaningfully_shrinks_realistic_float_data(tmp_path):
    """Chiffre mesuré (voir le rapport de phase 4) : ~40% sur un signal
    spectroscopique synthétique de 50k points x 2 colonnes. On vérifie ici
    une réduction substantielle, pas un pourcentage exact fragile."""
    project = _spectroscopy_like_project()

    uncompressed = tmp_path / "raw.plx"
    compressed = tmp_path / "gzip.plx"
    save_project(project, uncompressed, compression=None)
    save_project(project, compressed, compression="gzip", compression_opts=4)

    size_raw = uncompressed.stat().st_size
    size_gzip = compressed.stat().st_size
    assert size_gzip < size_raw * 0.85, (size_raw, size_gzip)

    # Et le contenu reste identique, compression ou pas.
    assert_same_state(snapshot(project), snapshot(load_project(compressed)))


def test_lzf_compression_option_round_trips(tmp_path):
    project = _spectroscopy_like_project(n=2_000)
    path = tmp_path / "lzf.plx"
    save_project(project, path, compression="lzf", compression_opts=None)
    assert_same_state(snapshot(project), snapshot(load_project(path)))


def test_empty_column_is_not_chunked_or_compressed(tmp_path):
    """Un dataset de taille 0 ne peut pas être chunké : on ne doit pas
    tenter de le compresser, seulement l'écrire tel quel."""
    project = Project("P")
    project.add_workbook("Book1").sheet("Sheet1").add_column("Empty", [])
    path = tmp_path / "empty_col.plx"
    save_project(project, path)  # ne doit pas lever

    with h5py.File(path, "r") as file:
        dataset = file["project"]["Book1"]["Sheet1"]["Empty"]
        assert dataset.shape == (0,)
        assert dataset.compression is None

    reloaded = load_project(path)
    assert len(reloaded.resolve_path("[Book1]Sheet1!Empty")) == 0


def test_numeric_column_is_a_plain_dataset(tmp_path):
    project = Project("P")
    project.add_workbook("Book1").sheet("Sheet1").add_column("N", [1.0, 2.0])
    path = tmp_path / "numeric.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file:
        obj = file["project"]["Book1"]["Sheet1"]["N"]
        assert isinstance(obj, h5py.Dataset)


def test_text_column_is_a_group_with_values_and_missing_datasets(tmp_path):
    from plotyx.core import Column

    project = Project("P")
    sheet = project.add_workbook("Book1").sheet("Sheet1")
    sheet.insert_column(0, Column("T", ["a", None], column_type="text"))
    path = tmp_path / "text.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file:
        obj = file["project"]["Book1"]["Sheet1"]["T"]
        assert isinstance(obj, h5py.Group)
        assert set(obj.keys()) == {"values", "missing"}


def test_no_compression_option_produces_uncompressed_datasets(tmp_path):
    project = _spectroscopy_like_project(n=1_000)
    path = tmp_path / "raw.plx"
    save_project(project, path, compression=None)
    with h5py.File(path, "r") as file:
        dataset = file["project"]["Book1"]["Sheet1"]["wavelength"]
        assert dataset.compression is None
