"""Choix de la plateforme Qt sous Linux (voir `plotyx.app.select_linux_platform`)."""

from __future__ import annotations

import sys

import pytest

from plotyx.app import select_linux_platform

pytestmark = pytest.mark.skipif(
    sys.platform != "linux", reason="le contournement ne concerne que Linux"
)


def test_wayland_session_is_switched_to_xcb():
    env = {"XDG_SESSION_TYPE": "wayland", "DISPLAY": ":0"}
    assert select_linux_platform(env) == "xcb"
    assert env["QT_QPA_PLATFORM"] == "xcb"


def test_an_explicit_choice_by_the_user_always_wins():
    """Y compris pour revenir à Wayland en connaissance de cause."""
    env = {
        "XDG_SESSION_TYPE": "wayland",
        "DISPLAY": ":0",
        "QT_QPA_PLATFORM": "wayland",
    }
    assert select_linux_platform(env) is None
    assert env["QT_QPA_PLATFORM"] == "wayland"


def test_an_x11_session_is_left_alone():
    env = {"XDG_SESSION_TYPE": "x11", "DISPLAY": ":0"}
    assert select_linux_platform(env) is None
    assert "QT_QPA_PLATFORM" not in env


def test_wayland_without_xwayland_is_left_alone():
    """Sans DISPLAY, imposer xcb empêcherait l'application de démarrer."""
    env = {"XDG_SESSION_TYPE": "wayland"}
    assert select_linux_platform(env) is None
    assert "QT_QPA_PLATFORM" not in env
