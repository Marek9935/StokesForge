"""Durée de vie des objets supprimés — le point délicat de la phase 2.

`ObjectRegistry` référence faiblement et les clés sont des UUID de naissance.
Un objet retiré du projet sort donc du registre et n'est plus retenu que par
la commande qui l'a retiré. Deux échecs symétriques sont possibles :

* **la fuite** — une pile d'undo non bornée retient indéfiniment des
  colonnes qui peuvent peser plusieurs dizaines de méga-octets ;
* **la clé morte** — l'objet est libéré trop tôt et une clé détenue
  ailleurs (drag & drop de phase 7, variable de console de phase 9) ne
  résout plus alors qu'une annulation était encore possible.

Ces tests vérifient les deux pour de vrai, avec `weakref` et `gc`, et pas
seulement par affirmation dans un docstring.
"""

from __future__ import annotations

import gc
import weakref

import numpy as np
import pytest

from plotyx.commands import (
    AddColumnCommand,
    CommandBus,
    CommandReleasedError,
    DeleteColumnCommand,
    SetCellsCommand,
)
from plotyx.core import Column, UnknownKeyError


def _weakref_to_deleted_column(bus, sheet, name="B"):
    """Supprime une colonne par commande et rend une référence faible dessus."""
    ref = weakref.ref(sheet.column(name))
    bus.push(DeleteColumnCommand(sheet, name))
    gc.collect()
    return ref


def test_a_deleted_column_stays_alive_while_the_undo_is_possible(bus, sheet):
    ref = _weakref_to_deleted_column(bus, sheet)
    assert ref() is not None
    assert bus.retained_objects() == (ref(),)


def test_a_deleted_column_leaves_the_registry_but_not_the_command(bus, sheet):
    column = sheet.column("B")
    key = column.key
    del column
    ref = _weakref_to_deleted_column(bus, sheet)

    with pytest.raises(UnknownKeyError):
        bus.project.resolve_key(key)

    bus.undo()
    assert bus.project.resolve_key(key) is ref()
    assert bus.retained_objects() == ()


def test_eviction_by_undo_limit_makes_the_object_collectable(qapp, project):
    """Le cœur du sujet : `setUndoLimit` doit vraiment libérer la mémoire."""
    bus = CommandBus(project, undo_limit=2)
    sheet = project.resolve_path("[Book1]Sheet1")
    ref = _weakref_to_deleted_column(bus, sheet)
    assert ref() is not None

    # Deux commandes de plus : la suppression tombe du bas de la pile.
    bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))
    bus.push(SetCellsCommand(sheet.column("A"), 1, [2.0]))
    gc.collect()

    assert bus.count == 2
    assert bus.retained_objects() == ()
    assert ref() is None, "la colonne évincée de la pile fuit"


def test_a_big_column_is_really_freed_on_eviction(qapp, project):
    """Contrôle grandeur nature : un tableau volumineux, pas un objet vide."""
    bus = CommandBus(project, undo_limit=1)
    sheet = project.resolve_path("[Book1]Sheet1")
    big = Column("Big", np.arange(250_000, dtype=np.float64))
    bus.push(AddColumnCommand(sheet, big))
    array_ref = weakref.ref(big._values)
    del big

    bus.push(DeleteColumnCommand(sheet, "Big"))
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    gc.collect()

    assert array_ref() is None, "le tableau de 2 Mo est toujours retenu"


def test_truncating_the_redo_branch_releases_what_it_held(bus, sheet):
    """Un ajout annulé retient la colonne ; écraser la branche la libère."""
    column = Column("D", [1.0, 2.0])
    ref = weakref.ref(column)
    bus.push(AddColumnCommand(sheet, column))
    del column
    bus.undo()
    gc.collect()
    assert ref() is not None
    assert bus.retained_objects() == (ref(),)

    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    gc.collect()
    assert ref() is None


def test_clear_releases_everything(bus, sheet):
    ref = _weakref_to_deleted_column(bus, sheet)
    assert ref() is not None

    bus.clear()
    gc.collect()
    assert ref() is None
    assert bus.retained_objects() == ()


def test_an_aborted_macro_releases_its_objects(bus, sheet):
    column = Column("D", [1.0])
    ref = weakref.ref(column)
    with pytest.raises(ValueError, match="boum"):
        with bus.macro("raté"):
            bus.push(AddColumnCommand(sheet, column))
            del column
            raise ValueError("boum")
    gc.collect()
    assert ref() is None
    assert bus.retained_objects() == ()


def test_a_macro_evicted_releases_all_of_its_children(qapp, project):
    bus = CommandBus(project, undo_limit=1)
    sheet = project.resolve_path("[Book1]Sheet1")
    with bus.macro("Suppression massive"):
        bus.push(DeleteColumnCommand(sheet, "A"))
        bus.push(DeleteColumnCommand(sheet, "B"))
    refs = [weakref.ref(obj) for obj in bus.retained_objects()]
    assert len(refs) == 2

    bus.push(AddColumnCommand(sheet, Column("Z")))
    gc.collect()
    assert bus.retained_objects() == ()
    assert [ref() for ref in refs] == [None, None]


def test_a_released_command_refuses_to_run_again(bus, sheet):
    command = bus.push(DeleteColumnCommand(sheet, "B"))
    command.release()
    assert command.released
    with pytest.raises(CommandReleasedError):
        command.redo()
    with pytest.raises(CommandReleasedError):
        command.undo()


def test_retained_objects_lists_pending_macro_members(bus, sheet):
    bus.begin_macro("En cours")
    bus.push(DeleteColumnCommand(sheet, "B"))
    assert len(bus.retained_objects()) == 1
    bus.end_macro()
    assert len(bus.retained_objects()) == 1
