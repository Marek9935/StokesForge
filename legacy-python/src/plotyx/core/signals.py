"""Mécanisme de signal/observateur en **Python pur**.

Le README pose comme principe non négociable que `plotyx.core` ne dépend
pas de Qt, alors que la roadmap parle de « signaux Qt émis par le modèle ».
La contradiction se résout ici : le modèle émet des signaux Python purs, et
la couche `plotyx.ui` fera l'adaptation vers `QObject`/`Signal` (typiquement
un pont qui s'abonne à un `Signal` de `core` et ré-émet un signal Qt).

L'API est volontairement minimale et calquée sur celle de Qt pour rendre
cette adaptation triviale : `connect`, `disconnect`, `emit`, `blocked`.
"""

from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import contextmanager
from typing import Any

__all__ = ["Connection", "Signal"]


class Connection:
    """Poignée retournée par :meth:`Signal.connect`, pour se désabonner."""

    __slots__ = ("_signal", "_slot", "_alive")

    def __init__(self, signal: Signal, slot: Callable[..., Any]) -> None:
        self._signal = signal
        self._slot = slot
        self._alive = True

    @property
    def active(self) -> bool:
        return self._alive

    @property
    def slot(self) -> Callable[..., Any]:
        return self._slot

    def disconnect(self) -> None:
        """Désabonne le slot. Idempotent."""
        if self._alive:
            self._signal.disconnect(self._slot)
            self._alive = False


class Signal:
    """Signal synchrone : un ordre d'appel stable, pas de dépendance externe.

    Les slots sont conservés par référence forte (pas de weakref) : le
    modèle vit plus longtemps que la vue, et une déconnexion implicite
    silencieuse serait bien plus difficile à diagnostiquer qu'une fuite.
    C'est à l'abonné d'appeler `disconnect()`.
    """

    __slots__ = ("name", "_slots", "_blocked")

    def __init__(self, name: str = "") -> None:
        self.name = name
        self._slots: list[Callable[..., Any]] = []
        self._blocked = False

    def __repr__(self) -> str:  # pragma: no cover - confort de debug
        return f"<Signal {self.name!r} slots={len(self._slots)}>"

    def __len__(self) -> int:
        return len(self._slots)

    def connect(self, slot: Callable[..., Any]) -> Connection:
        """Abonne `slot`. Un même slot abonné deux fois est appelé deux fois."""
        if not callable(slot):
            raise TypeError("un slot doit être appelable")
        self._slots.append(slot)
        return Connection(self, slot)

    def disconnect(self, slot: Callable[..., Any] | None = None) -> None:
        """Désabonne `slot` (la première occurrence), ou tout si `slot` est None."""
        if slot is None:
            self._slots.clear()
            return
        try:
            self._slots.remove(slot)
        except ValueError:
            raise KeyError(f"slot non connecté au signal {self.name!r}") from None

    def emit(self, *args: Any, **kwargs: Any) -> None:
        """Appelle tous les slots, dans l'ordre d'abonnement.

        L'itération se fait sur une copie : un slot a le droit de se
        désabonner (ou d'en abonner un autre) pendant l'émission sans
        corrompre le parcours.
        """
        if self._blocked:
            return
        for slot in tuple(self._slots):
            slot(*args, **kwargs)

    @property
    def blocked(self) -> bool:
        return self._blocked

    @contextmanager
    def block(self) -> Iterator[Signal]:
        """Suspend les émissions le temps du bloc `with` (équivalent QSignalBlocker)."""
        previous = self._blocked
        self._blocked = True
        try:
            yield self
        finally:
            self._blocked = previous
