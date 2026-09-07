"""En-tête de colonne façon Origin : nom, unité, label long, sur 3 lignes.

`QHeaderView` ne sait peindre qu'une seule ligne de texte ; ce module en
fournit une variante qui lit le rôle `HEADER_LINES_ROLE` du modèle
(`WorksheetTableModel`) et peint les trois lignes lui-même. C'est le seul
endroit du paquet qui connaît la mise en forme visuelle de l'en-tête ; le
modèle, lui, ne fait que fournir le texte.
"""

from __future__ import annotations

from PySide6.QtCore import QRect, QSize, Qt
from PySide6.QtGui import QFontMetrics, QPainter
from PySide6.QtWidgets import QHeaderView, QWidget

from plotyx.ui.worksheet_model import HEADER_LINES_ROLE

__all__ = ["RichHorizontalHeader"]


class RichHorizontalHeader(QHeaderView):
    """En-tête horizontal multi-lignes (nom, unité, label long)."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(Qt.Orientation.Horizontal, parent)
        self.setSectionsClickable(True)
        self.setHighlightSections(True)

    def sizeHint(self) -> QSize:  # noqa: N802 - API Qt
        base = super().sizeHint()
        line_height = QFontMetrics(self.font()).height()
        return QSize(base.width(), line_height * 3 + 12)

    def paintSection(  # noqa: N802 - API Qt
        self, painter: QPainter, rect: QRect, logical_index: int
    ) -> None:
        painter.save()
        super().paintSection(painter, rect, logical_index)
        painter.restore()

        model = self.model()
        if model is None:
            return
        lines = model.headerData(
            logical_index, Qt.Orientation.Horizontal, HEADER_LINES_ROLE
        )
        if not lines:
            return

        painter.save()
        painter.setClipRect(rect)
        base_font = painter.font()
        fm = QFontMetrics(base_font)
        line_height = fm.height()
        y = rect.top() + 2
        italic_font = base_font
        italic_font.setItalic(True)
        bold_font = painter.font()
        bold_font.setBold(True)
        fonts = [bold_font, base_font, italic_font]
        for line_index, text in enumerate(lines):
            if text:
                painter.setFont(fonts[min(line_index, len(fonts) - 1)])
                line_rect = QRect(rect.left() + 4, y, rect.width() - 8, line_height)
                painter.drawText(
                    line_rect,
                    int(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter),
                    text,
                )
            y += line_height
        painter.restore()
