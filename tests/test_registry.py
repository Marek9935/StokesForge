"""`ObjectRegistry` : la clé survit au renommage et au déplacement.

C'est l'invariant sur lequel reposent le drag & drop par référence (phase 7)
et la console de script (phase 9). Un `QMimeData` qui transporte une clé, ou
une variable de console qui en garde une, doivent continuer de désigner le
bon objet après que l'utilisateur a renommé ou rangé celui-ci ailleurs.
"""

from __future__ import annotations

import gc

import pytest

from plotyx.core import Column, ObjectRegistry, Project, StructureError, UnknownKeyError


@pytest.fixture
def project() -> Project:
    project = Project("Demo")
    folder = project.add_folder("Manips")
    book = project.add_workbook("Book1", folder=folder)
    sheet = book.sheet("Sheet1")
    sheet.add_column("A", [1.0, 2.0, 3.0])
    sheet.add_column("B", [4.0, 5.0])
    return project


# -- registre nu ----------------------------------------------------------


def test_register_is_idempotent_and_keyed_by_the_object():
    registry = ObjectRegistry()
    column = Column("A")
    assert registry.register(column) == column.key
    registry.register(column)
    assert len(registry) == 1
    assert registry.resolve(column.key) is column
    assert column.key in registry


def test_unknown_key_raises_and_get_returns_default():
    registry = ObjectRegistry()
    with pytest.raises(UnknownKeyError):
        registry.resolve("inexistante")
    assert registry.get("inexistante") is None


def test_unregister_is_silent_on_an_absent_object():
    registry = ObjectRegistry()
    registry.unregister(Column("A"))
    assert len(registry) == 0


def test_clear_and_iteration():
    registry = ObjectRegistry()
    columns = [Column("A"), Column("B")]
    for column in columns:
        registry.register(column)
    assert set(registry.keys()) == {c.key for c in columns}
    assert set(iter(registry)) == {c.key for c in columns}
    registry.clear()
    assert len(registry) == 0


def test_registry_holds_weak_references():
    """Un objet oublié partout ailleurs ne fuit pas via le registre."""
    registry = ObjectRegistry()
    column = Column("A")
    key = registry.register(column)
    del column
    gc.collect()
    assert registry.get(key) is None


# -- intégration avec le projet -------------------------------------------


def test_the_whole_subtree_is_registered_on_attach(project):
    keys = {node.key for node in project.walk()}
    assert set(project.registry.keys()) == keys
    assert len(keys) == 6  # racine, dossier, classeur, feuille, 2 colonnes


def test_key_survives_a_rename(project):
    column = project.resolve_path("[Book1]Sheet1!A")
    key = column.key

    project.rename(column, "Temps")
    project.rename(column.parent, "Données")
    project.rename(project.find_window("Book1"), "Mesures")

    assert project.resolve_key(key) is column
    assert column.key == key
    assert project.path_of(column) == "/Manips/[Mesures]Données!Temps"
    assert project.resolve_path("/Manips/[Mesures]Données!Temps") is column


def test_key_survives_a_move_between_folders(project):
    book = project.find_window("Book1")
    column = project.resolve_path("[Book1]Sheet1!A")
    keys = {node.key for node in book.walk()}

    destination = project.add_folder("Archives")
    project.move(book, destination)

    assert project.resolve_key(column.key) is column
    assert {node.key for node in book.walk()} == keys
    assert project.path_of(column) == "/Archives/[Book1]Sheet1!A"


def test_key_survives_a_column_moved_to_another_sheet(project):
    book = project.find_window("Book1")
    target = book.add_sheet("Sheet2")
    column = project.resolve_path("[Book1]Sheet1!A")
    key = column.key

    project.move(column, target)

    assert project.resolve_key(key) is column
    assert project.path_of(column) == "/Manips/[Book1]Sheet2!A"
    assert "A" not in book.sheet("Sheet1")


def test_key_survives_reordering(project):
    column = project.resolve_path("[Book1]Sheet1!B")
    key = column.key
    column.parent.move_column(column, 0)
    assert project.resolve_key(key) is column


def test_removing_an_object_unregisters_its_whole_subtree(project):
    book = project.find_window("Book1")
    keys = [node.key for node in book.walk()]

    project.remove(book)

    for key in keys:
        assert key not in project.registry
    # ... mais l'objet vit tant qu'une commande d'annulation le retient,
    # et il retrouve exactement les mêmes clés une fois réinséré.
    project.add(book)
    assert [node.key for node in book.walk()] == keys
    assert project.resolve_key(keys[-1]) is not None


def test_key_of_refuses_a_foreign_object(project):
    with pytest.raises(StructureError):
        project.key_of(Column("Orpheline"))


def test_key_of_returns_the_object_key(project):
    column = project.resolve_path("[Book1]Sheet1!A")
    assert project.key_of(column) == column.key


def test_two_objects_never_share_a_key(project):
    keys = [node.key for node in project.walk()]
    assert len(keys) == len(set(keys))


def test_key_collision_is_reported(project):
    registry = ObjectRegistry()
    first, second = Column("A"), Column("B")
    registry.register(first)
    second._key = first.key
    with pytest.raises(ValueError):
        registry.register(second)
