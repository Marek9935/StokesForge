"""Validation sur le corpus réel — `pytest -m corpus`, absent de la CI.

Lecture seule stricte : n'ouvre les fichiers qu'en lecture, ne modifie ni ne
déplace jamais rien sous `CORPUS_ROOT`. Le corpus est constitué de données de
thèse non publiées ; ce test se contente d'en lire des extraits pour mesurer
un taux de réussite, jamais de le committer ni d'en afficher le contenu.

Marqué `corpus` (voir `pyproject.toml`) : se saute proprement si le
répertoire est absent (poste de dev sans le point de montage NextCloud, CI).
"""

from __future__ import annotations

import random
from pathlib import Path

import pytest

from plotyx.io.registry import default_registry

pytestmark = pytest.mark.corpus

CORPUS_ROOT = Path("/home/aliocha/NextCloud/These/Caracterisation/Data")

#: Formats autorisés par la consigne : jamais les propriétaires (jws, spc, opju, ...).
ALLOWED_EXTENSIONS = {".txt", ".csv", ".dat", ".asc"}

#: Un sous-répertoire par instrument recensé dans le relevé du corpus.
INSTRUMENT_DIRS = ("V770", "V730", "JascoFluo", "analytikjena", "RPE", "IR", "IrradiationSetup")

#: Échantillon par instrument : le corpus compte 39 178 fichiers, on ne le
#: parcourt pas en entier — un tirage fixe (graine constante) par instrument
#: suffit à mesurer un taux représentatif sans ralentir la suite.
SAMPLE_PER_INSTRUMENT = 40

#: Seuil de sortie de phase : la roadmap vise ~12/15 (80 %) sans intervention manuelle.
MINIMUM_SUCCESS_RATE = 0.80


def _sample_files(directory: Path, rng: random.Random, count: int) -> list[Path]:
    files = [
        p for p in directory.rglob("*") if p.is_file() and p.suffix.lower() in ALLOWED_EXTENSIONS
    ]
    return rng.sample(files, min(count, len(files)))


def test_corpus_import_success_rate():
    if not CORPUS_ROOT.exists():
        pytest.skip(f"corpus réel absent sur cette machine : {CORPUS_ROOT}")

    registry = default_registry()
    rng = random.Random(20260904)  # déterministe : le taux ne varie pas d'une exécution à l'autre

    per_instrument: dict[str, tuple[int, int]] = {}
    total = 0
    total_ok = 0

    for name in INSTRUMENT_DIRS:
        directory = CORPUS_ROOT / name
        if not directory.exists():
            continue
        sample = _sample_files(directory, rng, SAMPLE_PER_INSTRUMENT)
        ok = 0
        for path in sample:
            try:
                result = registry.parse(path)  # lecture seule : `Path.read_bytes()` uniquement
            except Exception:
                continue
            if result.columns:
                ok += 1
        per_instrument[name] = (ok, len(sample))
        total += len(sample)
        total_ok += ok

    assert total > 0, "aucun fichier échantillonné : vérifier l'arborescence attendue du corpus"

    rate = total_ok / total
    report = ", ".join(f"{name}={ok}/{count}" for name, (ok, count) in per_instrument.items())
    print(f"\nTaux d'import réel (échantillon, sans intervention manuelle) : "
          f"{total_ok}/{total} ({rate:.1%})\n  détail par instrument : {report}")

    assert rate >= MINIMUM_SUCCESS_RATE, (
        f"taux d'import du corpus réel trop bas : {rate:.1%} (seuil {MINIMUM_SUCCESS_RATE:.0%}) "
        f"— {report}"
    )
