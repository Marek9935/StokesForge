"""Mécanisme d'observateur en Python pur (`plotyx.core.signals`)."""

from __future__ import annotations

import pytest

from plotyx.core.signals import Signal


def test_connect_emit_and_argument_passing():
    received = []
    signal = Signal("test")
    signal.connect(lambda *args, **kwargs: received.append((args, kwargs)))

    signal.emit(1, 2, kind="x")

    assert received == [((1, 2), {"kind": "x"})]


def test_slots_are_called_in_subscription_order():
    order = []
    signal = Signal()
    signal.connect(lambda: order.append("premier"))
    signal.connect(lambda: order.append("second"))

    signal.emit()

    assert order == ["premier", "second"]


def test_disconnect_stops_delivery_and_handle_is_idempotent():
    received = []
    signal = Signal()
    connection = signal.connect(received.append)

    signal.emit(1)
    connection.disconnect()
    connection.disconnect()  # idempotent
    signal.emit(2)

    assert received == [1]
    assert not connection.active
    assert len(signal) == 0


def test_disconnect_unknown_slot_raises():
    signal = Signal("s")
    with pytest.raises(KeyError):
        signal.disconnect(lambda: None)


def test_disconnect_all():
    signal = Signal()
    signal.connect(lambda: None)
    signal.connect(lambda: None)
    signal.disconnect()
    assert len(signal) == 0


def test_slot_may_disconnect_itself_during_emission():
    """Cas typique d'un pont UI qui se démonte sur notification."""
    calls = []
    signal = Signal()

    def once(value):
        calls.append(value)
        connection.disconnect()

    connection = signal.connect(once)
    other = []
    signal.connect(other.append)

    signal.emit("a")
    signal.emit("b")

    assert calls == ["a"]
    assert other == ["a", "b"]  # le second slot survit à la déconnexion du premier


def test_block_suspends_emission_and_restores_state():
    received = []
    signal = Signal()
    signal.connect(received.append)

    with signal.block():
        signal.emit("ignoré")
        assert signal.blocked
    signal.emit("reçu")

    assert received == ["reçu"]
    assert not signal.blocked


def test_connect_rejects_non_callable():
    with pytest.raises(TypeError):
        Signal().connect(42)


def test_same_slot_connected_twice_is_called_twice():
    received = []
    signal = Signal()
    signal.connect(received.append)
    signal.connect(received.append)
    signal.emit(1)
    assert received == [1, 1]
