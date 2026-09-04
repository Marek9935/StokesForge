"""`Project` : arbre, résolution de chemins, déplacements, unicité des noms."""

from __future__ import annotations

import pytest

from plotyx.core import (
    Column,
    DuplicateNameError,
    Folder,
    Graph,
    Matrix,
    Note,
    PathError,
    Project,
    StructureError,
    Workbook,
    Worksheet,
)


@pytest.fixture
def project() -> Project:
    project = Project("Demo")
    folder = project.add_folder("Manips")
    book = project.add_workbook("Book1", folder=folder)
    sheet = book.sheet("Sheet1")
    sheet.add_column("Temps", [0.0, 1.0, 2.0])
    sheet.add_column("Signal", [3.0, 4.0])
    return project


# -- construction de l'arbre ----------------------------------------------


def test_a_new_project_has_only_a_root_folder():
    project = Project("Vide")
    assert project.name == "Vide"
    assert isinstance(project.root, Folder)
    assert project.root.children() == ()
    assert project.path_of(project.root) == "/"
    assert len(project.registry) == 1


def test_a_new_workbook_comes_with_one_sheet():
    project = Project()
    book = project.add_workbook()
    assert book.name == "Book1"
    assert book.sheet_names() == ("Sheet1",)


def test_generated_window_names_avoid_collisions():
    project = Project()
    assert project.add_workbook().name == "Book1"
    assert project.add_workbook().name == "Book2"
    assert project.add_graph().name == "Graph1"
    assert project.add_matrix().name == "MBook1"
    assert project.add_note(text="bonjour").name == "Note1"
    assert project.add_note().text == ""


def test_minimal_nodes_carry_identity_and_metadata():
    project = Project()
    graph = project.add_graph("Fig1")
    graph.label = "Figure 1"
    graph.metadata["source"] = "manip du mardi"
    assert isinstance(graph, Graph)
    assert project.path_of(graph) == "/[Fig1]"
    assert project.resolve_path("[Fig1]") is graph
    assert graph.metadata["source"] == "manip du mardi"


def test_windows_and_walk(project):
    names = {window.name for window in project.windows()}
    assert names == {"Book1"}
    assert len(list(project.walk())) == 6


def test_add_into_an_empty_folder_does_not_fall_back_to_the_root():
    """Un `Folder` vide est falsy : le rangement doit tester `is None`."""
    project = Project()
    empty = project.add_folder("Vide")
    book = project.add_workbook("Book1", folder=empty)
    assert book.parent is empty
    assert project.path_of(book) == "/Vide/[Book1]"


# -- unicité des noms -----------------------------------------------------


def test_window_names_are_unique_project_wide(project):
    other = project.add_folder("Autre")
    with pytest.raises(DuplicateNameError):
        project.add_workbook("Book1", folder=other)


def test_folder_names_are_unique_among_siblings_only():
    project = Project()
    a = project.add_folder("Manips")
    project.add_folder("Jour1", folder=a)
    b = project.add_folder("Autre")
    project.add_folder("Jour1", folder=b)  # même nom, autre parent : accepté
    with pytest.raises(DuplicateNameError):
        project.add_folder("Manips")


def test_renaming_a_window_onto_an_existing_name_is_refused(project):
    project.add_graph("Fig1")
    with pytest.raises(DuplicateNameError):
        project.rename(project.find_window("Fig1"), "Book1")


def test_renaming_a_window_to_its_own_name_is_allowed(project):
    book = project.find_window("Book1")
    assert project.rename(book, "Book1") == "Book1"


def test_rename_returns_the_previous_name_for_undo(project):
    book = project.find_window("Book1")
    previous = project.rename(book, "Mesures")
    assert previous == "Book1"
    project.rename(book, previous)
    assert book.name == "Book1"


def test_rename_of_a_detached_object_still_works():
    column = Column("A")
    assert column._set_name("B") == "A"
    assert Project().rename(column, "C") == "B"


# -- résolution de chemins ------------------------------------------------


def test_resolve_every_level(project):
    assert project.resolve_path("/") is project.root
    assert project.resolve_path("/Manips").name == "Manips"
    assert isinstance(project.resolve_path("[Book1]"), Workbook)
    assert isinstance(project.resolve_path("[Book1]Sheet1"), Worksheet)
    assert isinstance(project.resolve_path("[Book1]Sheet1!Temps"), Column)


def test_resolve_accepts_a_parsed_path(project):
    from plotyx.core import parse_path

    assert project.resolve_path(parse_path("[Book1]Sheet1!Temps")).name == "Temps"


def test_folder_prefix_is_optional_but_checked(project):
    column = project.resolve_path("[Book1]Sheet1!Temps")
    assert project.resolve_path("/Manips/[Book1]Sheet1!Temps") is column
    with pytest.raises(PathError, match="n'est pas dans le dossier"):
        project.resolve_path("/Ailleurs/[Book1]Sheet1!Temps")


@pytest.mark.parametrize(
    "path",
    [
        "[Absent]",
        "[Book1]SheetX",
        "[Book1]Sheet1!Absente",
        "/Inconnu",
        "/Manips/Inconnu",
    ],
)
def test_unresolvable_paths_raise(project, path):
    with pytest.raises(PathError):
        project.resolve_path(path)


def test_a_sheet_cannot_be_asked_of_a_graph(project):
    project.add_graph("Fig1")
    with pytest.raises(PathError, match="pas un classeur"):
        project.resolve_path("[Fig1]Sheet1")


def test_a_folder_is_not_a_window(project):
    with pytest.raises(PathError):
        project.resolve_path("/Manips/Book1/Inconnu")


def test_path_of_is_the_inverse_of_resolve(project):
    for node in project.walk():
        if node is project.root:
            continue
        assert project.resolve_path(project.path_of(node)) is node


def test_path_of_refuses_a_foreign_object(project):
    with pytest.raises(StructureError):
        project.path_of(Column("Orpheline"))


def test_path_of_an_empty_worksheet_or_column_is_still_resolvable(project):
    """Découvert par la phase 3 : `Worksheet`/`Column` définissent `__len__`,

    donc une feuille ou une colonne fraîchement créée, encore vide, est
    *falsy*. `path_of` testait `if sheet`/`if column` plutôt que
    `is not None` — exactement le piège qu'un commentaire de `Project.add`
    met déjà en garde contre, un peu plus haut dans ce même fichier. Une
    feuille neuve sans colonnes, ou une colonne neuve sans valeurs, doit
    rester adressable par chemin avant toute donnée.
    """
    book = project.find_window("Book1")
    empty_sheet = book.add_sheet("VideSheet")
    assert project.path_of(empty_sheet) == "/Manips/[Book1]VideSheet"

    empty_column = empty_sheet.add_column("Z")
    assert len(empty_column) == 0
    assert project.path_of(empty_column) == "/Manips/[Book1]VideSheet!Z"


# -- déplacements ---------------------------------------------------------


def test_move_a_workbook_between_folders(project):
    book = project.find_window("Book1")
    destination = project.add_folder("Archives")

    old_parent, old_index = project.move(book, destination)

    assert book.parent is destination
    assert project.path_of(book) == "/Archives/[Book1]"
    project.move(book, old_parent, old_index)  # annulation par delta
    assert project.path_of(book) == "/Manips/[Book1]"


def test_move_a_sheet_between_workbooks(project):
    source = project.find_window("Book1")
    target = project.add(Workbook("Book2", sheets=[]))
    sheet = source.sheet("Sheet1")

    project.move(sheet, target)

    assert sheet.parent is target
    assert source.sheet_names() == ()
    assert project.resolve_path("[Book2]Sheet1!Temps").name == "Temps"


def test_move_rejects_an_incompatible_container(project):
    book = project.find_window("Book1")
    sheet = book.sheet("Sheet1")
    with pytest.raises(StructureError):
        project.move(sheet, project.root)
    with pytest.raises(StructureError):
        project.move(book, sheet)
    assert sheet.parent is book  # rien n'a bougé


def test_move_rejects_a_cycle(project):
    inner = project.add_folder("Jour1", folder=project.resolve_path("/Manips"))
    with pytest.raises(StructureError):
        project.move(project.resolve_path("/Manips"), inner)


def test_move_rejects_a_name_collision_without_detaching(project):
    book = project.find_window("Book1")
    other = project.add_workbook("Book2")  # possède déjà une feuille "Sheet1"
    sheet = book.sheet("Sheet1")

    with pytest.raises(DuplicateNameError):
        project.move(sheet, other)
    assert sheet.parent is book


def test_move_of_a_detached_object_is_refused():
    project = Project()
    with pytest.raises(StructureError):
        project.move(Column("A"), project.root)


def test_remove_of_a_detached_object_is_refused():
    with pytest.raises(StructureError):
        Project().remove(Column("A"))


def test_remove_returns_index_and_node_for_undo(project):
    book = project.find_window("Book1")
    sheet = book.sheet("Sheet1")
    index, removed = project.remove(sheet)
    assert (index, removed) == (0, sheet)
    book.insert_sheet(index, sheet)
    assert book.sheet_names() == ("Sheet1",)


def test_a_folder_only_accepts_folders_and_windows():
    project = Project()
    with pytest.raises(StructureError):
        project.add(Worksheet("Sheet1"))


def test_an_object_belongs_to_a_single_container():
    project = Project()
    book = project.add_workbook("Book1")
    with pytest.raises(StructureError):
        project.add(book)


def test_workbook_sheet_primitives():
    book = Workbook("Book1")
    second = book.add_sheet()
    assert book.sheet_names() == ("Sheet1", "Sheet2")
    assert book.sheet(1) is second
    assert book.sheet(second) is second
    assert book.move_sheet("Sheet2", 0) == 1
    assert book.sheet_names() == ("Sheet2", "Sheet1")
    assert book.rename_sheet("Sheet1", "Résultats") == "Sheet1"
    with pytest.raises(DuplicateNameError):
        book.rename_sheet("Résultats", "Sheet2")
    with pytest.raises(KeyError):
        book.sheet("Absente")
    with pytest.raises(StructureError):
        book.insert_sheet(0, Column("A"))
    index, sheet = book.remove_sheet("Résultats")
    assert (index, sheet.name) == (1, "Résultats")
    assert len(book) == 1


def test_folder_child_helpers():
    folder = Folder("F", [Graph("G1"), Matrix("M1"), Note("N1")])
    assert len(folder) == 3
    assert [child.name for child in folder] == ["G1", "M1", "N1"]
    assert folder.child("G1").name == "G1"
    assert len(folder.windows) == 3
    assert folder.folders == ()
    with pytest.raises(KeyError):
        folder.child("Absent")
    with pytest.raises(KeyError):
        folder.folder("G1")
    assert folder.move_child("N1", 0) == 2
    assert folder.remove_child("N1")[0] == 0
    with pytest.raises(StructureError):
        folder.insert_child(0, folder)


def test_rename_child_refuses_a_stranger():
    folder = Folder("F")
    with pytest.raises(KeyError):
        folder.rename_child(Graph("G1"), "G2")


def test_ancestors_and_root(project):
    column = project.resolve_path("[Book1]Sheet1!Temps")
    names = [node.name for node in column.ancestors()]
    assert names == ["Sheet1", "Book1", "Manips", "Demo"]
    assert column.root is project.root
    assert column.project is project
    assert Column("Orpheline").project is None
