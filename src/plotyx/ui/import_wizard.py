"""`ImportWizardDialog` : le seul chemin d'entrée pour importer des fichiers.

Jamais d'import à l'aveugle : le dialogue montre toujours un aperçu (ou un
message d'erreur explicite) avant que le bouton « Importer » ne fasse quoi
que ce soit, et les overrides manuels (encodage, délimiteur, séparateur
décimal, lignes à sauter) passent par le même chemin que la détection
automatique — `FileParser.parse(path, options)`.

Import réel : délégué à `ImportWorker` (`QThread`), avec barre de
progression et bouton d'annulation, pour ne jamais geler l'UI sur un gros
fichier. Une fois tous les fichiers parsés, les colonnes obtenues sont
poussées dans **une seule macro** du bus (`apply_import_results`) — le
drag & drop (`MainWindow`) alimente ce même dialogue.
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QLabel,
    QListWidget,
    QMessageBox,
    QProgressBar,
    QPushButton,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from plotyx.commands import CommandBus
from plotyx.core.worksheet import Worksheet
from plotyx.io.parser import ImportOptions
from plotyx.io.registry import ParserRegistry, default_registry
from plotyx.io.result import ImportResult
from plotyx.ui.import_apply import apply_import_results
from plotyx.ui.import_worker import ImportWorker

__all__ = ["ImportWizardDialog"]

#: Au-delà de cette taille, pas d'aperçu synchrone : l'import se fait directement en arrière-plan.
_PREVIEW_SIZE_LIMIT = 5_000_000
_PREVIEW_ROWS = 200


class ImportWizardDialog(QDialog):
    """Sélection de fichiers, overrides, aperçu tableau, puis import en macro."""

    def __init__(
        self,
        sheet: Worksheet,
        bus: CommandBus,
        paths: list[Path | str] | None = None,
        registry: ParserRegistry | None = None,
        parent=None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName("importWizardDialog")
        self.setWindowTitle("Importer des fichiers")
        self.resize(760, 520)
        self._sheet = sheet
        self._bus = bus
        self._registry = registry or default_registry()
        self._paths: list[Path] = [Path(p) for p in (paths or [])]
        self._worker: ImportWorker | None = None
        self._results: dict[Path, ImportResult] = {}
        self._failed: dict[Path, str] = {}

        self._build_ui()
        self._refresh_file_list()
        self._preview_selected()

    # -- construction --------------------------------------------------------

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)

        files_row = QHBoxLayout()
        self._file_list = QListWidget()
        self._file_list.setObjectName("importFileList")
        self._file_list.currentRowChanged.connect(lambda _row: self._preview_selected())
        files_row.addWidget(self._file_list, 1)

        buttons_col = QVBoxLayout()
        add_button = QPushButton("Ajouter…")
        add_button.setObjectName("importAddFilesButton")
        add_button.clicked.connect(self._on_add_files)
        remove_button = QPushButton("Retirer")
        remove_button.setObjectName("importRemoveFileButton")
        remove_button.clicked.connect(self._on_remove_file)
        buttons_col.addWidget(add_button)
        buttons_col.addWidget(remove_button)
        buttons_col.addStretch(1)
        files_row.addLayout(buttons_col)
        layout.addLayout(files_row)

        overrides = QFormLayout()
        self._encoding_combo = QComboBox()
        self._encoding_combo.setObjectName("importEncodingCombo")
        self._encoding_combo.setEditable(True)
        self._encoding_combo.addItems(["(auto)", "utf-8", "latin-1", "cp1252"])
        self._delimiter_combo = QComboBox()
        self._delimiter_combo.setObjectName("importDelimiterCombo")
        self._delimiter_combo.addItems(["(auto)", ";", ",", "\\t", "espace"])
        self._decimal_combo = QComboBox()
        self._decimal_combo.setObjectName("importDecimalCombo")
        self._decimal_combo.addItems(["(auto)", ".", ","])
        self._skiprows_spin = QSpinBox()
        self._skiprows_spin.setObjectName("importSkiprowsSpin")
        self._skiprows_spin.setRange(-1, 100_000)
        self._skiprows_spin.setValue(-1)
        self._skiprows_spin.setSpecialValueText("(auto)")
        overrides.addRow("Encodage :", self._encoding_combo)
        overrides.addRow("Délimiteur :", self._delimiter_combo)
        overrides.addRow("Séparateur décimal :", self._decimal_combo)
        overrides.addRow("Lignes à sauter :", self._skiprows_spin)
        layout.addLayout(overrides)

        for widget in (self._encoding_combo, self._delimiter_combo, self._decimal_combo):
            widget.currentIndexChanged.connect(lambda _i: self._preview_selected())
        self._skiprows_spin.valueChanged.connect(lambda _v: self._preview_selected())

        self._preview_table = QTableWidget()
        self._preview_table.setObjectName("importPreviewTable")
        layout.addWidget(self._preview_table, 1)

        self._warnings_label = QLabel("")
        self._warnings_label.setObjectName("importWarningsLabel")
        self._warnings_label.setWordWrap(True)
        layout.addWidget(self._warnings_label)

        self._progress_bar = QProgressBar()
        self._progress_bar.setObjectName("importProgressBar")
        self._progress_bar.setRange(0, 100)
        self._progress_bar.hide()
        layout.addWidget(self._progress_bar)

        cancel_row = QHBoxLayout()
        self._cancel_import_button = QPushButton("Annuler l'import")
        self._cancel_import_button.setObjectName("importCancelButton")
        self._cancel_import_button.hide()
        self._cancel_import_button.clicked.connect(self._on_cancel_import)
        cancel_row.addWidget(self._cancel_import_button)
        cancel_row.addStretch(1)
        layout.addLayout(cancel_row)

        self._buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel)
        self._import_button = self._buttons.addButton(
            "Importer", QDialogButtonBox.ButtonRole.AcceptRole
        )
        self._import_button.setObjectName("importAcceptButton")
        self._import_button.clicked.connect(self._on_import)
        self._buttons.rejected.connect(self.reject)
        layout.addWidget(self._buttons)

    # -- fichiers -----------------------------------------------------------

    def _refresh_file_list(self) -> None:
        current = self._file_list.currentRow()
        self._file_list.blockSignals(True)
        self._file_list.clear()
        for path in self._paths:
            self._file_list.addItem(path.name)
        if self._paths:
            self._file_list.setCurrentRow(max(0, min(current, len(self._paths) - 1)))
        self._file_list.blockSignals(False)

    def _on_add_files(self) -> None:
        files, _ = QFileDialog.getOpenFileNames(self, "Choisir des fichiers à importer")
        if not files:
            return
        self.add_paths(files)

    def add_paths(self, paths) -> None:
        """Ajoute des fichiers (utilisé par le dépose OS) et rafraîchit l'aperçu."""
        changed = False
        for raw in paths:
            path = Path(raw)
            if path not in self._paths:
                self._paths.append(path)
                changed = True
        if changed:
            self._refresh_file_list()
            self._preview_selected()

    def _on_remove_file(self) -> None:
        row = self._file_list.currentRow()
        if 0 <= row < len(self._paths):
            del self._paths[row]
            self._refresh_file_list()
            self._preview_selected()

    @property
    def paths(self) -> tuple[Path, ...]:
        return tuple(self._paths)

    # -- overrides -----------------------------------------------------------

    def _current_options(self) -> ImportOptions:
        options: ImportOptions = {}
        encoding = self._encoding_combo.currentText().strip()
        if encoding and encoding != "(auto)":
            options["encoding"] = encoding
        delimiter = self._delimiter_combo.currentText()
        if delimiter and delimiter != "(auto)":
            options["delimiter"] = {"\\t": "\t", "espace": " "}.get(delimiter, delimiter)
        decimal = self._decimal_combo.currentText()
        if decimal and decimal != "(auto)":
            options["decimal"] = decimal
        if self._skiprows_spin.value() >= 0:
            options["skiprows"] = self._skiprows_spin.value()
        return options

    # -- aperçu ----------------------------------------------------------------

    def _selected_path(self) -> Path | None:
        row = self._file_list.currentRow()
        if 0 <= row < len(self._paths):
            return self._paths[row]
        return None

    def _preview_selected(self) -> None:
        self._preview_table.clear()
        self._preview_table.setRowCount(0)
        self._preview_table.setColumnCount(0)
        self._warnings_label.setText("")
        path = self._selected_path()
        if path is None:
            return
        try:
            size = path.stat().st_size
        except OSError as error:
            self._warnings_label.setText(f"Fichier illisible : {error}")
            return
        if size > _PREVIEW_SIZE_LIMIT:
            self._warnings_label.setText(
                "Fichier volumineux : pas d'aperçu synchrone — l'import se fera en "
                "arrière-plan, avec barre de progression."
            )
            return
        result = self._registry.parse(path, self._current_options())
        self._show_preview(result)

    def _show_preview(self, result: ImportResult) -> None:
        columns = result.columns
        self._preview_table.setColumnCount(len(columns))
        self._preview_table.setHorizontalHeaderLabels([c.name for c in columns])
        row_count = min(_PREVIEW_ROWS, max((len(c.values) for c in columns), default=0))
        self._preview_table.setRowCount(row_count)
        for col_index, column in enumerate(columns):
            for row_index in range(row_count):
                value = column.values[row_index] if row_index < len(column.values) else float("nan")
                self._preview_table.setItem(row_index, col_index, QTableWidgetItem(f"{value}"))
        if result.warnings:
            self._warnings_label.setText(
                "\n".join(f"[{w.severity}] {w.message}" for w in result.warnings)
            )
        elif not columns:
            self._warnings_label.setText("Aucune colonne détectée dans ce fichier.")

    # -- import ------------------------------------------------------------

    def _on_import(self) -> None:
        if not self._paths:
            QMessageBox.warning(self, "Import", "Aucun fichier sélectionné.")
            return
        self._import_button.setEnabled(False)
        self._progress_bar.show()
        self._progress_bar.setValue(0)
        self._cancel_import_button.show()
        self._results = {}
        self._failed = {}
        self._worker = ImportWorker(
            self._registry, list(self._paths), self._current_options(), self
        )
        self._worker.progress.connect(self._on_progress)
        self._worker.file_finished.connect(self._on_file_finished)
        self._worker.failed.connect(self._on_file_failed)
        self._worker.finished_all.connect(self._on_import_finished)
        self._worker.cancelled.connect(self._on_import_cancelled)
        self._worker.start()

    def _on_cancel_import(self) -> None:
        if self._worker is not None:
            self._worker.cancel()

    def _on_progress(self, fraction: float) -> None:
        self._progress_bar.setValue(int(fraction * 100))

    def _on_file_finished(self, path_str: str, result: ImportResult) -> None:
        self._results[Path(path_str)] = result

    def _on_file_failed(self, path_str: str, message: str) -> None:
        self._failed[Path(path_str)] = message

    def _on_import_cancelled(self) -> None:
        self._progress_bar.hide()
        self._cancel_import_button.hide()
        self._import_button.setEnabled(True)
        self._worker = None

    def _on_import_finished(self) -> None:
        self._progress_bar.hide()
        self._cancel_import_button.hide()
        self._worker = None

        usable = {path: result for path, result in self._results.items() if result.columns}
        if usable:
            apply_import_results(
                self._bus, self._sheet, usable, macro_text=f"Import ({len(usable)} fichier(s))"
            )

        failed_paths = dict(self._failed)
        for path, result in self._results.items():
            if not result.columns:
                messages = "; ".join(w.message for w in result.warnings) or "échec inconnu"
                failed_paths[path] = messages
        if failed_paths:
            details = "\n".join(f"{p.name} : {m}" for p, m in list(failed_paths.items())[:10])
            QMessageBox.warning(
                self,
                "Import partiel",
                f"{len(failed_paths)} fichier(s) non importé(s) :\n{details}",
            )

        self._import_button.setEnabled(True)
        if usable or not failed_paths:
            self.accept()

    # -- cycle de vie ---------------------------------------------------------

    def _shutdown_worker(self) -> None:
        if self._worker is not None and self._worker.isRunning():
            self._worker.cancel()
            self._worker.wait(5000)

    def reject(self) -> None:  # noqa: N802 - API Qt
        self._shutdown_worker()
        super().reject()

    def closeEvent(self, event) -> None:  # noqa: N802 - API Qt
        self._shutdown_worker()
        super().closeEvent(event)
