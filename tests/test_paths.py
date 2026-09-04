"""Grammaire de chemins Origin `[Book1]Sheet1!ColA`."""

from __future__ import annotations

import pytest

from plotyx.core import PathError, format_path, parse_path


def test_full_column_path():
    path = parse_path("[Book1]Sheet1!ColA")
    assert (path.window, path.sheet, path.column) == ("Book1", "Sheet1", "ColA")
    assert path.folders == ()
    assert path.depth == "column"


def test_worksheet_path():
    path = parse_path("[Book1]Sheet1")
    assert (path.window, path.sheet, path.column) == ("Book1", "Sheet1", None)
    assert path.depth == "sheet"


def test_window_path():
    path = parse_path("[Graph1]")
    assert (path.window, path.sheet) == ("Graph1", None)
    assert path.depth == "window"


def test_folder_paths():
    assert parse_path("/").folders == ()
    assert parse_path("/").rooted
    assert parse_path("/Manips/Jour1").folders == ("Manips", "Jour1")
    assert parse_path("Manips/Jour1").folders == ("Manips", "Jour1")
    assert not parse_path("Manips").rooted
    assert parse_path("/Manips").depth == "folder"


def test_path_with_folder_prefix():
    path = parse_path("/Manips/Jour1/[Book1]Sheet1!ColA")
    assert path.folders == ("Manips", "Jour1")
    assert path.window == "Book1"
    assert path.rooted


def test_surrounding_whitespace_is_tolerated():
    assert parse_path("  [Book1]Sheet1  ").sheet == "Sheet1"


@pytest.mark.parametrize(
    "text",
    [
        "",
        "   ",
        "[Book1",
        "Book1]Sheet1",
        "[]Sheet1",
        "[Book1]!ColA",
        "Manips[Book1]Sheet1",
        "[Book1]Sheet1!",
        "[Book1]Sheet[2]",
        "a!b",
    ],
)
def test_malformed_paths_are_refused(text):
    with pytest.raises(PathError):
        parse_path(text)


def test_non_string_is_refused():
    with pytest.raises(PathError):
        parse_path(42)


@pytest.mark.parametrize(
    "text",
    [
        "/",
        "/Manips",
        "/Manips/Jour1",
        "[Book1]",
        "[Book1]Sheet1",
        "[Book1]Sheet1!ColA",
        "/Manips/[Book1]Sheet1!ColA",
    ],
)
def test_format_is_the_inverse_of_parse(text):
    assert format_path(parse_path(text)) == text
    assert str(parse_path(text)) == text
