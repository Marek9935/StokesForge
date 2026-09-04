"""Import de fichiers — phase 5.

Paquet sans Qt, au même titre que `plotyx.core` : le parsing est de la
logique métier, testable en CLI, jamais couplée à un widget. Le wizard
(`plotyx.ui.import_wizard`) et le fil d'exécution en arrière-plan
(`plotyx.ui.import_worker`) sont les seuls endroits qui ont le droit
d'importer Qt pour cette fonctionnalité.

Architecture : un registre de parseurs pluggables (`FileParser` scoré par
`can_parse()`), jamais un `if extension == ...` monolithique. Un parseur ne
lève jamais d'exception fatale pour un fichier mal formé : il rend un
`ImportResult` avec des `ImportWarning` structurés, à charge de l'appelant
(wizard, ou le test `corpus`) de décider quoi en faire.
"""

from plotyx.io.csv_parser import CsvParser
from plotyx.io.jasco_parser import JascoParser
from plotyx.io.parser import FileParser, ImportCancelled
from plotyx.io.registry import ParserRegistry, default_registry
from plotyx.io.result import ColumnImport, ImportResult, ImportWarning

__all__ = [
    "ColumnImport",
    "CsvParser",
    "FileParser",
    "ImportCancelled",
    "ImportResult",
    "ImportWarning",
    "JascoParser",
    "ParserRegistry",
    "default_registry",
]
