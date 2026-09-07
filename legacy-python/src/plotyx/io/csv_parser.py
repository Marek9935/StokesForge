"""`CsvParser` : parseur texte générique, sans connaissance d'un instrument.

Pipeline de `parse()` :

1. Encodage (`chardet`, via `plotyx.io._text`).
2. Délimiteur (`csv.Sniffer`, avec repli sur un comptage de cohérence des
   lignes de fin de fichier — le `Sniffer` seul se fait piéger par des
   fichiers Jasco, où les lignes d'en-tête `CLE;valeur` n'ont pas le même
   nombre de champs que les lignes de données).
3. Ligne de démarrage des données : la première ligne dont le nombre de
   champs correspond à celui des dernières lignes du fichier *et* dont les
   champs sont majoritairement numériques. Tout ce qui précède est
   métadonnée à sauter (roadmap : « lignes d'en-tête/métadonnées à
   sauter »). Ce test au format numérique — pas seulement au nombre de
   champs — est ce qui laisse le fichier `##CLE=valeur` de l'IR (piège
   volontaire, ressemble à du JCAMP-DX) tomber correctement dans ce
   parseur générique plutôt que de faire mordre un parseur qui scorerait
   sur `##`.
4. Séparateur décimal : comparaison du nombre de champs qui se parsent en
   `float` tels quels contre le nombre qui s'y parsent une fois les
   virgules remplacées par des points, sur un échantillon de lignes de
   données.
5. Lecture complète via `pandas.read_csv`, en flux (`chunksize`) pour les
   gros fichiers — voir `_read_dataframe`.

Rien ici ne connaît « Jasco » : les indices `LOCALE`/`XYDATA` sont
spécifiques à `JascoParser`, qui score plus haut sur ces fichiers dans le
registre.
"""

from __future__ import annotations

import csv
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from plotyx.core.column import ColumnRole, ColumnType
from plotyx.io._text import decode_best_effort, detect_encoding, looks_binary
from plotyx.io.parser import FileParser, ImportCancelled, ImportOptions
from plotyx.io.result import ColumnImport, ImportResult, ImportWarning

__all__ = ["CsvParser"]

#: Sentinelle de délimiteur « espaces blancs » (regex, pas un caractère).
WHITESPACE_DELIMITER = r"\s+"

#: Candidats testés dans cet ordre de préférence à score de cohérence égal.
_DELIMITER_CANDIDATES = (";", "\t", ",", "|")

#: Au-delà de cette taille de fichier, la lecture bascule en flux par défaut.
_STREAM_THRESHOLD_BYTES = 5_000_000
_DEFAULT_CHUNK_ROWS = 20_000


def _split(line: str, delimiter: str) -> list[str]:
    if delimiter == WHITESPACE_DELIMITER:
        return line.split()
    return line.split(delimiter)


def _numeric_success_count(fields: list[str]) -> int:
    count = 0
    for field in fields:
        try:
            float(field.strip())
        except ValueError:
            continue
        count += 1
    return count


def _mode_field_count(lines: list[str], delimiter: str) -> int:
    counts: dict[int, int] = {}
    for line in lines:
        if not line.strip():
            continue
        counts[len(_split(line, delimiter))] = counts.get(len(_split(line, delimiter)), 0) + 1
    if not counts:
        return 0
    return max(counts.items(), key=lambda pair: pair[1])[0]


def _consistency(lines: list[str], delimiter: str) -> float:
    """Fraction des lignes non vides dont le compte de champs vaut le mode, `>1`."""
    non_empty = [line for line in lines if line.strip()]
    if not non_empty:
        return 0.0
    mode = _mode_field_count(non_empty, delimiter)
    if mode <= 1:
        return 0.0
    matching = sum(1 for line in non_empty if len(_split(line, delimiter)) == mode)
    return matching / len(non_empty)


def guess_delimiter(lines: list[str]) -> str:
    """Délimiteur le plus probable.

    Le comptage de cohérence (même nombre de champs sur les lignes de fin
    de fichier) passe **avant** `csv.Sniffer` : sur un fichier à virgule
    décimale et délimiteur `;` sans en-tête, la virgule décimale forme
    elle-même un « délimiteur » parfaitement cohérent (une par ligne, à
    position fixe), et `Sniffer` s'y laisse prendre systématiquement — piège
    du corpus réel (analytikjena). Le comptage manuel, à égalité de score,
    préfère `;` puis TAB puis `,` (l'ordre de `_DELIMITER_CANDIDATES`), ce
    qui tranche cette ambiguïté dans le bon sens. `Sniffer` ne sert plus que
    de filet quand le comptage manuel est trop faible pour trancher seul.
    """
    tail = [line for line in lines[-30:] if line.strip()]
    if not tail:
        tail = [line for line in lines if line.strip()]
    best_delimiter, best_score = None, 0.0
    for delimiter in _DELIMITER_CANDIDATES:
        score = _consistency(tail, delimiter)
        if score > best_score:
            best_delimiter, best_score = delimiter, score
    if best_delimiter is not None and best_score >= 0.6:
        return best_delimiter
    sample = "\n".join(tail)
    if sample:
        try:
            dialect = csv.Sniffer().sniff(sample, delimiters=";,\t|")
        except csv.Error:
            pass
        else:
            if _consistency(tail, dialect.delimiter) > best_score:
                return dialect.delimiter
    if best_delimiter is not None and best_score >= 0.5:
        return best_delimiter
    return WHITESPACE_DELIMITER


def _numeric_ratio(fields: list[str]) -> float:
    if not fields:
        return 0.0
    return _numeric_success_count(fields) / len(fields)


def _numeric_ratio_comma(fields: list[str]) -> float:
    converted = [
        f.replace(",", ".", 1) if f.count(",") == 1 and "." not in f else f for f in fields
    ]
    return _numeric_ratio(converted)


def _is_float(token: str) -> bool:
    try:
        float(token.strip())
    except ValueError:
        return False
    return True


def _is_float_comma(token: str) -> bool:
    text = token.strip()
    if text.count(",") == 1 and "." not in text:
        text = text.replace(",", ".")
    return _is_float(text)


def detect_data_start(lines: list[str], delimiter: str) -> int:
    """Index de la première ligne de données, ou `0` si rien ne s'en distingue.

    Une ligne ne compte comme donnée que si son **premier** champ (la
    colonne X) est lui-même numérique — pas seulement une majorité de
    champs. Sans ce garde-fou, une ligne d'en-tête façon
    `wavelength\t0\t1\t2\t...` (le corpus réel en contient : les libellés de
    colonnes d'une matrice large sont eux-mêmes des entiers) se ferait
    prendre pour une ligne de données, `wavelength` mis à part.
    """
    non_empty = [line for line in lines if line.strip()]
    expected = _mode_field_count(non_empty[-30:] or non_empty, delimiter)
    if expected <= 0:
        return 0
    for index, line in enumerate(lines):
        if not line.strip():
            continue
        fields = _split(line, delimiter)
        if len(fields) != expected:
            continue
        if not (_is_float(fields[0]) or _is_float_comma(fields[0])):
            continue
        ratio = max(_numeric_ratio(fields), _numeric_ratio_comma(fields))
        if ratio >= 0.8:
            return index
    return 0


def detect_header(
    lines: list[str], delimiter: str, data_start: int
) -> tuple[int | None, list[str] | None]:
    """Ligne d'en-tête juste avant les données.

    Deux indices, l'un suffit : un nombre de champs identique à la ligne de
    données *et* soit une majorité de champs non numériques, soit un
    premier champ non numérique. Ce deuxième cas couvre le nom de colonne X
    d'une matrice large (`wavelength\t0\t1\t2\t...`) : le reste de la ligne
    ressemble à des nombres (ce sont des libellés de colonnes), mais aucune
    ligne de données réelle n'a de premier champ non numérique.
    """
    if data_start <= 0:
        return None, None
    candidate_index = data_start - 1
    candidate = lines[candidate_index]
    if not candidate.strip():
        return None, None
    fields = _split(candidate, delimiter)
    data_fields = _split(lines[data_start], delimiter)
    if len(fields) != len(data_fields):
        return None, None
    first_numeric = _is_float(fields[0]) or _is_float_comma(fields[0])
    if first_numeric and _numeric_ratio(fields) >= 0.5:
        return None, None
    return candidate_index, [f.strip() for f in fields]


def detect_decimal(lines: list[str], data_start: int, delimiter: str) -> str:
    """`'.'` ou `','`, par comparaison du taux de succès `float()` sur un échantillon."""
    if delimiter == ",":
        return "."  # ambiguïté : la virgule est déjà le délimiteur
    sample = [line for line in lines[data_start : data_start + 50] if line.strip()]
    if not sample:
        return "."
    dot_hits = sum(_numeric_success_count(_split(line, delimiter)) for line in sample)
    comma_hits = 0
    for line in sample:
        fields = _split(line, delimiter)
        converted = [
            f.replace(",", ".", 1) if f.count(",") == 1 and "." not in f else f for f in fields
        ]
        comma_hits += _numeric_success_count(converted)
    return "," if comma_hits > dot_hits else "."


def _generated_names(count: int) -> list[str]:
    if count <= 0:
        return []
    names = ["X"]
    for i in range(1, count):
        names.append("Y" if count == 2 else f"Y{i}")
    return names


class CsvParser(FileParser):
    """Parseur CSV/texte générique : sniffing complet, aucun format connu à l'avance."""

    name = "csv"

    def can_parse(self, path: Path) -> float:
        try:
            raw = path.read_bytes()[:65536]
        except OSError:
            return 0.0
        if not raw or looks_binary(raw):
            return 0.0
        suffix = path.suffix.lower().lstrip(".")
        score = {"csv": 0.5, "txt": 0.4, "dat": 0.35, "asc": 0.4}.get(suffix, 0.15)
        try:
            encoding = detect_encoding(raw)
            text, _ = decode_best_effort(raw, encoding)
            lines = text.splitlines()
            if len(lines) >= 1:
                delimiter = guess_delimiter(lines)
                data_start = detect_data_start(lines, delimiter)
                if data_start < len(lines):
                    score += 0.3
        except Exception:  # noqa: BLE001 - le scoring ne doit jamais lever
            pass
        return min(score, 0.85)

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
        if looks_binary(raw):
            return ImportResult(
                warnings=[
                    ImportWarning(
                        "binary_file",
                        f"{path.name!r} ressemble à du binaire, pas du texte",
                        severity="error",
                    )
                ],
                parser_name=self.name,
            )

        encoding_override = options.get("encoding")
        detected_encoding = encoding_override or detect_encoding(raw)
        text, used_encoding = decode_best_effort(raw, detected_encoding)
        if used_encoding != detected_encoding:
            warnings.append(
                ImportWarning(
                    "encoding_fallback",
                    f"encodage détecté {detected_encoding!r} invalide sur ce fichier, "
                    f"repli sur {used_encoding!r}",
                )
            )
        lines = text.splitlines()
        if not any(line.strip() for line in lines):
            return ImportResult(
                warnings=[
                    *warnings,
                    ImportWarning("empty_file", "aucune ligne exploitable", severity="error"),
                ],
                parser_name=self.name,
            )

        delimiter = options.get("delimiter") or guess_delimiter(lines)
        data_start = (
            options["skiprows"] if "skiprows" in options else detect_data_start(lines, delimiter)
        )
        header_index, header_fields = (None, None)
        if "header" in options:
            header_fields = list(options["header"]) if options["header"] else None
        else:
            header_index, header_fields = detect_header(lines, delimiter, data_start)

        decimal = options.get("decimal") or detect_decimal(lines, data_start, delimiter)
        if decimal == ",":
            warnings.append(
                ImportWarning(
                    "decimal_guess",
                    "séparateur décimal détecté : virgule (aucune donnée n'a été "
                    "modifiée, seule la lecture en tient compte)",
                )
            )

        skip_lines = data_start
        metadata: dict[str, Any] = {
            "encoding": used_encoding,
            "delimiter": "whitespace" if delimiter == WHITESPACE_DELIMITER else delimiter,
            "decimal": decimal,
            "skipped_lines": skip_lines,
        }
        if header_index is not None:
            metadata["header_line"] = lines[header_index].strip()

        try:
            df = self._read_dataframe(path, used_encoding, delimiter, decimal, skip_lines, options)
        except ImportCancelled:
            raise
        except Exception as error:  # noqa: BLE001 - jamais d'exception fatale hors annulation
            return ImportResult(
                warnings=[
                    *warnings,
                    ImportWarning("parse_error", f"lecture impossible : {error}", severity="error"),
                ],
                metadata=metadata,
                parser_name=self.name,
            )

        if df.empty or df.shape[1] == 0:
            return ImportResult(
                warnings=[
                    *warnings,
                    ImportWarning("empty_data", "aucune donnée numérique lue", severity="error"),
                ],
                metadata=metadata,
                parser_name=self.name,
            )

        names = header_fields if header_fields else _generated_names(df.shape[1])
        if len(names) < df.shape[1]:
            names = names + [f"Y{i}" for i in range(len(names), df.shape[1])]
        df.columns = names[: df.shape[1]]

        columns: list[ColumnImport] = []
        non_numeric_total = 0
        for position, col_name in enumerate(df.columns):
            series = pd.to_numeric(df[col_name], errors="coerce")
            non_numeric_total += int(series.isna().sum() - df[col_name].isna().sum())
            role = ColumnRole.X if position == 0 else ColumnRole.Y
            columns.append(
                ColumnImport(
                    name=str(col_name),
                    values=series.to_numpy(dtype=np.float64),
                    column_type=ColumnType.NUMERIC,
                    role=role,
                )
            )
        if non_numeric_total > 0:
            warnings.append(
                ImportWarning(
                    "non_numeric_values",
                    f"{non_numeric_total} valeur(s) non numérique(s) remplacée(s) par NaN",
                )
            )

        return ImportResult(
            columns=columns, metadata=metadata, warnings=warnings, parser_name=self.name
        )

    # -- lecture (flux pour les gros fichiers) -----------------------------

    def _read_dataframe(
        self,
        path: Path,
        encoding: str,
        delimiter: str,
        decimal: str,
        skiprows: int,
        options: ImportOptions,
    ) -> pd.DataFrame:
        sep = delimiter
        read_kwargs: dict[str, Any] = {
            "sep": sep,
            "encoding": encoding,
            "decimal": decimal,
            "skiprows": skiprows,
            "header": None,
            "engine": "python",
            "on_bad_lines": "skip",
        }
        chunk_size = options.get("chunk_size")
        force_stream = chunk_size is not None
        if not force_stream and path.stat().st_size < _STREAM_THRESHOLD_BYTES:
            return pd.read_csv(path, **read_kwargs)

        chunk_size = chunk_size or _DEFAULT_CHUNK_ROWS
        should_cancel = options.get("should_cancel")
        progress = options.get("progress")
        total_rows = self._count_data_rows(path, skiprows)

        frames: list[pd.DataFrame] = []
        read_rows = 0
        for chunk in pd.read_csv(path, chunksize=chunk_size, **read_kwargs):
            if should_cancel is not None and should_cancel():
                raise ImportCancelled(f"import de {path.name!r} annulé")
            frames.append(chunk)
            read_rows += len(chunk)
            if progress is not None:
                fraction = read_rows / total_rows if total_rows else 1.0
                progress(min(fraction, 1.0))
        if progress is not None:
            progress(1.0)
        if not frames:
            return pd.DataFrame()
        return pd.concat(frames, ignore_index=True)

    @staticmethod
    def _count_data_rows(path: Path, skiprows: int) -> int:
        """Compte de lignes en un passage binaire — rapide même sur un gros fichier."""
        total = 0
        with path.open("rb") as handle:
            for _ in handle:
                total += 1
        return max(total - skiprows, 0)
