//! Contrat de navigation et de lecture du projet depuis `pyst` (issue #66).
//! Les mêmes instantanés et poignées servent à une console et à une session
//! sans interface ; aucun test ne dépend d'une feuille active ou d'une fenêtre.

#![cfg(feature = "python")]

use std::path::PathBuf;

use stokesforge_commands::Command;
use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, NodeId, Project, ValueBlock,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    engine: ScriptEngine,
    session: SessionHandle,
    _actor: Session,
    book: NodeId,
    empty_folder: NodeId,
    number: NodeId,
    mixed: NodeId,
}

fn banc() -> Banc {
    let mut project = Project::new("Lecture").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Manips").unwrap();
    let empty_folder = project.add_folder(root, "Vide").unwrap();
    let book = project.add_workbook(folder, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project.add_worksheet(book, Some("Vide")).unwrap();
    let number = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(vec![1.0, -0.0, f64::NAN]),
                ColumnMeta {
                    role: ColumnRole::X,
                    long_name: "Température".into(),
                    unit: "°C".into(),
                    comment: "Essai μ".into(),
                    display_format: ".2f".into(),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[
                Some("étiquette".into()),
                None,
                Some("".into()),
                Some("12.5".into()),
            ]),
        )
        .unwrap();
    let mixed = project
        .add_column(
            sheet,
            None,
            Some("M"),
            Column::from_mixed(&[
                MixedCell::Empty,
                MixedCell::Text("".into()),
                MixedCell::Number(f64::NAN),
                MixedCell::Number(-0.0),
                MixedCell::Text("12.5".into()),
                MixedCell::Number(f64::INFINITY),
                MixedCell::Text("μ\nλ".into()),
            ]),
        )
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("Date"),
            Column::from_block(
                ColumnType::DateTime,
                ValueBlock::Float(vec![0.0, 1_700_000_000.0]),
                ColumnMeta::default(),
            )
            .unwrap(),
        )
        .unwrap();
    project
        .add_column(sheet, None, Some("Empty"), Column::from_mixed(&[]))
        .unwrap();
    project.add_note(root, Some("Note1"), "Note").unwrap();
    project
        .add_graph(root, Some("Graph1"), Default::default())
        .unwrap();
    project.add_matrix(root, Some("Matrix1")).unwrap();

    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home)).unwrap();
    Banc {
        engine,
        session,
        _actor: actor,
        book,
        empty_folder,
        number,
        mixed,
    }
}

fn executer(banc: &Banc, source: &str) {
    let outcome = banc.engine.run(source).unwrap();
    assert!(
        outcome.error.is_none(),
        "{}",
        outcome.error.unwrap_or_default()
    );
}

#[test]
fn navigation_ordonnees_metadonnees_et_longueurs_sans_mutation() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    executer(
        &banc,
        r#"
def noms(handles):
    return [pyst.info(h)['name'] for h in handles]
racine = pyst.root()
assert pyst.info(racine) == dict(name='Lecture', kind='folder', parent=None, row_count=None, column=None)
assert noms(pyst.children()) == ['Manips', 'Vide', 'Note1', 'Graph1', 'Matrix1']
assert noms(pyst.children(racine)) == noms(pyst.children())
folder = pyst.children()[0]
book = pyst.children(folder)[0]
assert pyst.info(book)['kind'] == 'workbook'
assert pyst.info(pyst.info(book)['parent'])['name'] == 'Manips'
assert noms(pyst.children(book)) == ['Sheet1', 'Vide']
sheet, empty_sheet = pyst.children(book)
assert pyst.info(sheet)['kind'] == 'worksheet'
assert pyst.info(sheet)['row_count'] == 7
assert pyst.info(empty_sheet)['row_count'] == 0
assert pyst.children(empty_sheet) == []
assert pyst.children(pyst.children()[1]) == []
assert noms(pyst.children(sheet)) == ['A', 'T', 'M', 'Date', 'Empty']
a = pyst.children(sheet)[0]
description = pyst.info(a)
assert description['name'] == 'A' and description['kind'] == 'column'
assert description['row_count'] == 3
assert pyst.info(description['parent'])['name'] == 'Sheet1'
meta = description['column']
assert set(meta) == {'type', 'role', 'long_name', 'unit', 'comment', 'display_format', 'formula', 'version'}
assert meta['type'] == 'numeric' and meta['role'] == 'x'
assert meta['long_name'] == 'Température' and meta['unit'] == '°C'
assert meta['comment'] == 'Essai μ' and meta['display_format'] == '.2f'
assert meta['formula'] is None and isinstance(meta['version'], int)
assert pyst.info(pyst.resolve('[Book1]Sheet1!Date'))['column']['type'] == 'datetime'
assert pyst.info(pyst.resolve('[Book1]Sheet1!T'))['column']['type'] == 'mixed'
assert pyst.children(a) == []
for name, kind in [('Note1', 'note'), ('Graph1', 'graph'), ('Matrix1', 'matrix')]:
    h = pyst.resolve(f'[{name}]')
    assert pyst.info(h)['kind'] == kind
    assert pyst.info(h)['column'] is None and pyst.info(h)['row_count'] is None
    assert pyst.children(h) == []
# Le dictionnaire et la liste sont des valeurs détachées du modèle.
meta['unit'] = 'K'
description['name'] = 'Autre'
colonnes = pyst.children(sheet)
colonnes.clear()
assert pyst.info(a)['column']['unit'] == '°C'
assert pyst.info(a)['name'] == 'A' and len(pyst.children(sheet)) == 5
assert not hasattr(a, 'id') and not hasattr(a, 'epoch')
try:
    pyst.Handle()
except TypeError:
    pass
else:
    raise AssertionError('poignée constructible depuis Python')
"#,
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

#[test]
fn cellules_texte_mixtes_et_numeriques_conservees_exactement() {
    let banc = banc();
    executer(
        &banc,
        r#"
import math
import numpy
texte = pyst.values(pyst.resolve('[Book1]Sheet1!T'))
assert texte == ('étiquette', None, '', '12.5')
m = pyst.resolve('[Book1]Sheet1!M')
cellules = pyst.values(m)
assert isinstance(cellules, tuple) and len(cellules) == 7
assert cellules[0] is None and cellules[1] == '' and math.isnan(cellules[2])
assert cellules[3] == 0 and math.copysign(1, cellules[3]) == -1
assert cellules[4] == '12.5' and type(cellules[4]) is str
assert math.isinf(cellules[5]) and cellules[6] == 'μ\nλ'
try:
    cellules[0] = 42
except TypeError:
    pass
else:
    raise AssertionError('tuple modifiable')
assert pyst.values(pyst.resolve('[Book1]Sheet1!Empty')) == ()
a = pyst.resolve('[Book1]Sheet1!A')
nombres = pyst.values(a)
assert nombres[0] == 1 and math.copysign(1, nombres[1]) == -1 and math.isnan(nombres[2])
vue = pyst.column(a)
assert isinstance(vue, numpy.ndarray) and vue.dtype == numpy.float64 and not vue.flags.writeable
assert numpy.shares_memory(vue, pyst.column(a))
date = pyst.resolve('[Book1]Sheet1!Date')
assert pyst.values(date) == (0., 1700000000.)
assert list(pyst.column(date)) == [0., 1700000000.]
"#,
    );
    banc.session
        .execute(Command::SetCells {
            column: banc.mixed,
            start: 0,
            values: ValueBlock::Mixed(vec![MixedCell::Text("nouveau".into())]),
            expected_version: None,
        })
        .unwrap();
    executer(&banc, "assert cellules[0] is None\nassert pyst.values(m)[0] == 'nouveau'\nassert texte == ('étiquette', None, '', '12.5')");
}

#[test]
fn lectures_refusent_les_mauvaises_cibles_avec_les_exceptions_du_pont() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    executer(
        &banc,
        r#"
for h in [pyst.root(), pyst.resolve('[Book1]'), pyst.resolve('[Book1]Sheet1'), pyst.resolve('[Note1]')]:
    try:
        pyst.values(h)
    except pyst.InvalidValueError as e:
        assert 'not a column' in str(e)
    else:
        raise AssertionError('une lecture de cellules accepte un autre objet')
for path in ['[Absent]', '[Book1]Absent', '[Book1]Sheet1!Absent', '[malforme']:
    try:
        pyst.resolve(path)
    except pyst.UnknownPathError:
        pass
    else:
        raise AssertionError('chemin inconnu accepté')
"#,
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.history, before.history);
}

#[test]
fn poignees_de_navigation_survivent_au_renommage_deplacement_et_annulation() {
    let banc = banc();
    executer(
        &banc,
        "book = pyst.children(pyst.children()[0])[0]\na = pyst.children(pyst.children(book)[0])[0]",
    );
    banc.session
        .execute(Command::Rename {
            id: banc.number,
            name: "Renommee".into(),
        })
        .unwrap();
    banc.session
        .execute(Command::Move {
            id: banc.book,
            new_parent: banc.empty_folder,
            index: 0,
        })
        .unwrap();
    executer(
        &banc,
        r#"
assert pyst.info(a)['name'] == 'Renommee'
assert pyst.values(a)[0] == 1
assert pyst.info(pyst.info(book)['parent'])['name'] == 'Vide'
assert pyst.info(pyst.children(pyst.children(book)[0])[0])['name'] == 'Renommee'
"#,
    );
    banc.session
        .execute(Command::DeleteColumn {
            column: banc.number,
        })
        .unwrap();
    executer(
        &banc,
        r#"
for lecture in [pyst.info, pyst.children, pyst.values, pyst.column]:
    try:
        lecture(a)
    except pyst.BrokenReferenceError:
        pass
    else:
        raise AssertionError('poignée supprimée acceptée')
"#,
    );
    banc.session.undo().unwrap();
    executer(
        &banc,
        "assert pyst.info(a)['name'] == 'Renommee'\nassert pyst.values(a)[0] == 1",
    );
}

#[test]
fn chargement_invalide_toutes_les_lectures_meme_si_les_identifiants_reapparaissent() {
    let banc = banc();
    executer(
        &banc,
        r#"
racine = pyst.root()
book = pyst.children(pyst.children()[0])[0]
sheet = pyst.children(book)[0]
a, texte, mixte, date, vide = pyst.children(sheet)
parent = pyst.info(a)['parent']
"#,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identique.stf");
    banc.session.save(&path).unwrap();
    banc.session.load(&path).unwrap();
    executer(
        &banc,
        r#"
for h in [racine, book, sheet, parent, a, texte, mixte, date, vide]:
    for lecture in [pyst.info, pyst.children, pyst.values, pyst.column]:
        try:
            lecture(h)
        except pyst.BrokenReferenceError as e:
            assert 'loaded' in str(e)
        else:
            raise AssertionError('une ancienne poignée désigne le projet rechargé')
assert pyst.info(pyst.root())['name'] == 'Lecture'
assert len(pyst.children()) == 5
assert pyst.values(pyst.resolve('[Book1]Sheet1!T')) == ('étiquette', None, '', '12.5')
"#,
    );
}

#[test]
fn metadonnees_relues_et_roles_de_colonne_stables() {
    let banc = banc();
    for (role, expected) in [
        (ColumnRole::X, "x"),
        (ColumnRole::Y, "y"),
        (ColumnRole::Z, "z"),
        (ColumnRole::XError, "x_error"),
        (ColumnRole::YError, "y_error"),
        (ColumnRole::Label, "label"),
        (ColumnRole::None, "none"),
    ] {
        banc.session
            .execute(Command::SetColumnMeta {
                column: banc.number,
                meta: ColumnMeta {
                    role,
                    unit: "K".into(),
                    formula: Some("2+3".into()),
                    ..Default::default()
                },
            })
            .unwrap();
        executer(&banc, &format!("meta = pyst.info(pyst.resolve('[Book1]Sheet1!A'))['column']\nassert meta['role'] == {expected:?}\nassert meta['unit'] == 'K'\nassert meta['formula'] == '2+3'"));
    }
}

#[test]
fn session_fermee_refuse_aussi_la_racine_et_les_lectures_sans_poignee() {
    let Banc {
        engine,
        _actor: actor,
        ..
    } = banc();
    let outcome = engine
        .run("racine = pyst.root()\na = pyst.resolve('[Book1]Sheet1!A')")
        .unwrap();
    assert!(outcome.error.is_none());
    actor.shutdown().unwrap();
    let outcome = engine
        .run(
            r#"
for lecture in [pyst.root, pyst.children, lambda: pyst.children(racine),
                lambda: pyst.info(a), lambda: pyst.values(a), lambda: pyst.column(a)]:
    try:
        lecture()
    except pyst.BrokenReferenceError as e:
        assert 'session is closed' in str(e)
    else:
        raise AssertionError('la lecture ignore la fermeture de la session')
"#,
        )
        .unwrap();
    assert!(
        outcome.error.is_none(),
        "{}",
        outcome.error.unwrap_or_default()
    );
}
