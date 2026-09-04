"""`JascoParser` : format clé/valeur + marqueur `XYDATA` des spectromètres Jasco.

Second parseur du registre, concurrent de `CsvParser` sur ces fichiers :
c'est ce qui valide que le scoring du registre tient avec plusieurs
parseurs (roadmap, phase 5). Il score nettement plus haut que le CSV
générique dès que `XYDATA` est présent — voir `can_parse`.

Le piège central du corpus : les fichiers combinent virgule décimale et
point-virgule (ou tabulation, selon `.csv` vs `.txt` — même famille
d'instrument, deux formats). `csv.Sniffer` seul se ferait piéger par les
lignes d'en-tête `CLE;valeur`. La détection du séparateur décimal ici
**n'utilise pas** le champ `LOCALE` (indice fort mais pas une règle : deux
locales différentes, 1036/fr-FR et 1040/it-IT, rencontrées dans le corpus,
toutes deux à virgule — d'autres ne le seraient pas) ; elle regarde
directement si les champs numériques de l'en-tête (`FIRSTX`, `LASTX`, ...)
contiennent une virgule.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np

from plotyx.core.column import ColumnRole, ColumnType
from plotyx.io._text import decode_best_effort, detect_encoding
from plotyx.io.parser import FileParser, ImportOptions
from plotyx.io.result import ColumnImport, ImportResult, ImportWarning

__all__ = ["JascoParser"]

#: Champs numériques de l'en-tête consultés pour deviner le séparateur décimal.
_DECIMAL_HINT_KEYS = ("FIRSTX", "LASTX", "FIRSTY", "MAXY", "MINY", "DELTAX")

#: Unités XUNITS/YUNITS Jasco connues → unité courte pour le modèle de colonne.
_X_UNITS = {"NANOMETERS": "nm", "1/CM": "cm-1", "MICROMETERS": "µm"}
_Y_UNITS = {"ABSORBANCE": "AU", "%T": "%", "%R": "%", "TRANSMITTANCE": "%"}


def _detect_header_delimiter(lines: list[str], data_marker: str = "XYDATA") -> str:
    """`;` ou tabulation : le premier des deux rencontré dans l'en-tête."""
    for line in lines:
        stripped = line.strip()
        if stripped.upper() == data_marker:
            break
        if "\t" in stripped:
            return "\t"
        if ";" in stripped:
            return ";"
    return ";"


def _detect_decimal(header: dict[str, str]) -> str:
    for key in _DECIMAL_HINT_KEYS:
        if "," in header.get(key, ""):
            return ","
    return "."


def _to_float(token: str, decimal: str) -> float:
    text = token.strip()
    if decimal == ",":
        text = text.replace(",", ".")
    return float(text)


class JascoParser(FileParser):
    """Parseur des spectres Jasco (UV-Vis, fluorescence) : en-tête + `XYDATA`."""

    name = "jasco"

    def can_parse(self, path: Path) -> float:
        try:
            raw = path.read_bytes()[:8192]
        except OSError:
            return 0.0
        text = raw.decode("latin-1", errors="replace").upper()
        if "XYDATA" not in text:
            return 0.0
        score = 0.6
        if "ORIGIN" in text and "JASCO" in text:
            score += 0.2
        if "FIRSTX" in text and "LASTX" in text:
            score += 0.1
        return min(score, 0.95)

    def parse(self, path: Path, options: ImportOptions | None = None) -> ImportResult:
        options = options or {}
        warnings: list[ImportWarning] = []
        try:
            raw = path.read_bytes()
        except OSError as error:
            return ImportResult(
                warnings=[ImportWarning("io_error", str(error), severity="error")],
                parser_name=self.name,
            )
        if not raw:
            return ImportResult(
                warnings=[ImportWarning("empty_file", "fichier vide", severity="error")],
                parser_name=self.name,
            )

        encoding_hint = options.get("encoding") or detect_encoding(raw, default="latin-1")
        text, used_encoding = decode_best_effort(raw, encoding_hint)
        if used_encoding != encoding_hint:
            warnings.append(
                ImportWarning(
                    "encoding_fallback",
                    f"encodage détecté {encoding_hint!r} invalide, repli sur {used_encoding!r}",
                )
            )
        lines = text.splitlines()

        delimiter = options.get("delimiter") or _detect_header_delimiter(lines)
        header: dict[str, str] = {}
        data_start: int | None = None
        for index, line in enumerate(lines):
            stripped = line.strip()
            if not stripped:
                continue
            if stripped.upper() == "XYDATA":
                data_start = index + 1
                break
            key, sep, value = stripped.partition(delimiter)
            if sep:
                header[key.strip().upper()] = value.strip()

        if data_start is None:
            return ImportResult(
                warnings=[
                    ImportWarning(
                        "no_xydata", "marqueur XYDATA absent du fichier", severity="error"
                    )
                ],
                metadata=dict(header),
                parser_name=self.name,
            )

        decimal = options.get("decimal") or _detect_decimal(header)
        if decimal == ",":
            warnings.append(
                ImportWarning(
                    "decimal_guess",
                    "séparateur décimal détecté : virgule (déduit des champs "
                    "numériques d'en-tête, pas de LOCALE)",
                )
            )

        xs: list[float] = []
        ys: list[float] = []
        bad_rows = 0
        for line in lines[data_start:]:
            if not line.strip():
                continue
            fields = line.strip().split(delimiter)
            if len(fields) < 2:
                bad_rows += 1
                continue
            try:
                x = _to_float(fields[0], decimal)
                y = _to_float(fields[1], decimal)
            except ValueError:
                bad_rows += 1
                continue
            xs.append(x)
            ys.append(y)

        if bad_rows:
            warnings.append(
                ImportWarning("unparsable_rows", f"{bad_rows} ligne(s) de données ignorée(s)")
            )

        if not xs:
            return ImportResult(
                warnings=[
                    *warnings,
                    ImportWarning(
                        "empty_data", "aucune donnée exploitable après XYDATA", severity="error"
                    ),
                ],
                metadata=dict(header),
                parser_name=self.name,
            )

        npoints_text = header.get("NPOINTS", "").strip()
        if npoints_text:
            try:
                expected = int(npoints_text)
            except ValueError:
                pass
            else:
                if expected != len(xs):
                    warnings.append(
                        ImportWarning(
                            "npoints_mismatch",
                            f"NPOINTS annonce {expected} point(s), {len(xs)} ligne(s) lue(s)",
                        )
                    )

        title = header.get("TITLE") or path.stem
        data_type = header.get("DATA TYPE", "")
        xunits = header.get("XUNITS", "")
        yunits = header.get("YUNITS", "")

        columns = [
            ColumnImport(
                name="X",
                values=np.array(xs, dtype=np.float64),
                column_type=ColumnType.NUMERIC,
                role=ColumnRole.X,
                unit=_X_UNITS.get(xunits.upper(), xunits.lower()),
                long_name=xunits.title(),
            ),
            ColumnImport(
                name=title,
                values=np.array(ys, dtype=np.float64),
                column_type=ColumnType.NUMERIC,
                role=ColumnRole.Y,
                unit=_Y_UNITS.get(yunits.upper(), yunits),
                long_name=data_type or yunits,
                comment=title,
            ),
        ]

        metadata: dict[str, Any] = dict(header)
        metadata["encoding"] = used_encoding
        metadata["delimiter"] = "tab" if delimiter == "\t" else delimiter
        metadata["decimal"] = decimal

        return ImportResult(
            columns=columns, metadata=metadata, warnings=warnings, parser_name=self.name
        )
