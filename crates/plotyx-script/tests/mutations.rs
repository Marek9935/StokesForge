//! Contrat des écritures Python (#67) : commandes existantes, un groupe par
//! exécution, et retour arrière après une erreur ou une interruption.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use plotyx_commands::{Command, CommandError, HistoryLabel, MacroKind};
use plotyx_core::{Column, ColumnMeta, NodeId, Project, ValueBlock};
use plotyx_python::interpreter::{self, EmbeddedPython};
use plotyx_script::{ScriptEngine, ScriptOutcome};
use plotyx_session::{Session, SessionError, SessionHandle};

struct Banc {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    a: NodeId,
    b: NodeId,
}

fn banc() -> Banc {
    let mut project = Project::new("Écriture").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[9.0; 5]))
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());
    Banc {
        engine,
        session,
        _actor: actor,
        a,
        b,
    }
}

fn run(banc: &Banc, source: &str) -> ScriptOutcome {
    let outcome = banc.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

fn floats(banc: &Banc, id: NodeId) -> Vec<f64> {
    banc.session
        .snapshot()
        .unwrap()
        .tree
        .node(id)
        .unwrap()
        .as_column()
        .unwrap()
        .float_values()
        .unwrap()
        .as_ref()
        .clone()
}

#[test]
fn construit_renomme_remplit_et_supprime_en_un_seul_groupe() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    run(
        &banc,
        r#"
book = pyx.create_workbook('Résultats')
assert pyx.children(book) == []
sheet = pyx.create_worksheet(book, 'Calculs')
x = pyx.create_column(sheet, 'Temps')
y = pyx.create_column(sheet, 'Signal')
t = pyx.create_column(sheet, 'Texte', column_type='mixed')
d = pyx.create_column(sheet, 'Date', column_type='datetime')
pyx.set_values(x, [0, 1, 2])
pyx.set_values(y, [10.0, 20.0])
pyx.set_values(t, ['12', '', None, 'µ'])
pyx.set_values(d, [0.0, None])
pyx.set_column_meta(x, role='x', long_name='Temps écoulé', unit='s', comment='Essai', display_format='.2f')
assert pyx.info(x)['column']['unit'] == 's'
pyx.rename(y, 'Intensité')
assert pyx.info(y)['name'] == 'Intensité'
pyx.remove(d)
assert len(pyx.children(sheet)) == 3
assert pyx.info(sheet)['row_count'] == 4
assert pyx.info(x)['column']['role'] == 'x'
assert pyx.values(t) == ('12', '', None, 'µ')
"#,
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(
        after.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    assert!(!after.history.in_macro);
    assert_ne!(*after.tree, *before.tree);
    banc.session.undo().unwrap();
    let undone = banc.session.snapshot().unwrap();
    assert_eq!(*undone.tree, *before.tree);
    assert!(
        !undone.history.can_undo,
        "un seul Ctrl+Z annule toute l'exécution"
    );
    banc.session.redo().unwrap();
    assert_eq!(*banc.session.snapshot().unwrap().tree, *after.tree);
}

#[test]
fn remplace_les_longueurs_sans_toucher_aux_autres_colonnes_et_garde_les_vues() {
    let banc = banc();
    run(
        &banc,
        r#"
import numpy as np
a = pyx.resolve('[Book1]Sheet1!A')
old = pyx.column(a)
pyx.set_column_meta(a, unit='s')
pyx.set_values(a, np.arange(12., dtype=np.float64)[::2])
assert pyx.values(a) == (0., 2., 4., 6., 8., 10.)
pyx.set_values(a, [float('nan'), -0.0])
assert np.isnan(pyx.values(a)[0])
assert np.signbit(pyx.values(a)[1])
assert list(old) == [1., 2., 3.]
assert pyx.info(a)['column']['unit'] == 's'
assert pyx.info(pyx.resolve('[Book1]Sheet1!B'))['row_count'] == 5
"#,
    );
    assert_eq!(floats(&banc, banc.b), vec![9.0; 5]);
    assert_eq!(floats(&banc, banc.a).len(), 2);
    banc.session.undo().unwrap();
    assert_eq!(floats(&banc, banc.a), vec![1., 2., 3.]);
    banc.session.redo().unwrap();
    assert_eq!(floats(&banc, banc.a)[1].to_bits(), (-0.0_f64).to_bits());
    run(&banc, "pyx.set_values(a, [])");
    assert!(floats(&banc, banc.a).is_empty());
    banc.session.undo().unwrap();
    assert_eq!(floats(&banc, banc.a).len(), 2);
}

#[test]
fn cellules_mixtes_ne_sont_ni_reanalysees_ni_normalisees() {
    let banc = banc();
    run(
        &banc,
        r#"
import math
import numpy as np
sheet = pyx.info(pyx.resolve('[Book1]Sheet1!A'))['parent']
t = pyx.create_column(sheet, column_type='mixed')
source = [None, '', '12', float('nan'), -0.0, 2]
pyx.set_values(t, iter(source))
source[2] = 'modifié après la copie'
v = pyx.values(t)
assert v[:3] == (None, '', '12')
assert math.isnan(v[3])
assert math.copysign(1., v[4]) == -1.
assert v[5] == 2.
pyx.set_values(t, np.array([float('nan'), -0.0]))
v = pyx.values(t)
assert math.isnan(v[0]) and v[0] is not None
assert math.copysign(1., v[1]) == -1.
"#,
    );
}

#[test]
fn erreur_non_rattrapee_annule_aussi_les_creations_et_preserve_redo() {
    let banc = banc();
    run(
        &banc,
        "pyx.set_values(pyx.resolve('[Book1]Sheet1!A'), [4.])",
    );
    banc.session.undo().unwrap();
    let before = banc.session.snapshot().unwrap();
    let outcome = banc
        .engine
        .run(
            r#"
pyx.create_workbook('À annuler')
pyx.set_values(pyx.resolve('[Book1]Sheet1!A'), [7., 8., 9., 10.])
raise RuntimeError('échec volontaire')
"#,
        )
        .unwrap();
    assert!(outcome.error.unwrap().contains("échec volontaire"));
    let after = banc.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    banc.session.redo().unwrap();
    assert_eq!(floats(&banc, banc.a), vec![4.]);
    run(&banc, "assert pyx.children()");
}

#[test]
fn refuse_types_noms_et_plafonds_sans_ecriture_partielle() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    run(
        &banc,
        r#"
import numpy as np
import numpy.ma as ma
a = pyx.resolve('[Book1]Sheet1!A')
for values in (['texte'], [object()], [2**53 + 1], 'texte', np.array([1], dtype=np.int64), np.ones((2, 2)), ma.array([1.], mask=[True]), np.broadcast_to(np.array([1.]), (100_000_001,))):
    try:
        pyx.set_values(a, values)
    except pyx.InvalidValueError:
        pass
    else:
        raise AssertionError('valeur invalide acceptée')
class TropLong:
    def __len__(self): return 100_000_001
    def __iter__(self): raise AssertionError('le plafond doit précéder la copie')
try:
    pyx.set_values(a, TropLong())
except pyx.InvalidValueError:
    pass
else:
    raise AssertionError('plafond ignoré')
for name in ('', 'x' * 256, 'B', 'x!y'):
    try:
        pyx.rename(a, name)
    except pyx.InvalidValueError:
        pass
    else:
        raise AssertionError('nom invalide accepté')
for changes in ({'role': 'inconnu'}, {'type': 'mixed'}, {'version': 1}, {'unit': None}):
    try:
        pyx.set_column_meta(a, **changes)
    except pyx.InvalidValueError:
        pass
    else:
        raise AssertionError('métadonnée invalide acceptée')
try:
    pyx.create_worksheet(a)
except pyx.InvalidValueError:
    pass
else:
    raise AssertionError('mauvais parent accepté')
try:
    pyx.remove(pyx.root())
except pyx.InvalidValueError:
    pass
else:
    raise AssertionError('racine supprimée')
"#,
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert!(!after.history.can_undo);
    assert!(!after.history.in_macro);
}

#[test]
fn suppression_et_load_ne_rendent_pas_une_poignee_perimee_valide() {
    let banc = banc();
    run(&banc, "a = pyx.resolve('[Book1]Sheet1!A')\npyx.remove(a)");
    run(
        &banc,
        r#"
try:
    pyx.rename(a, 'Périmée')
except pyx.BrokenReferenceError:
    pass
else:
    raise AssertionError('poignée supprimée acceptée')
"#,
    );
    banc.session.undo().unwrap();
    run(&banc, "pyx.rename(a, 'Renommée')");
    banc.session.undo().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.plx");
    banc.session.save(&path).unwrap();
    banc.session.load(&path).unwrap();
    let before = banc.session.snapshot().unwrap();
    run(
        &banc,
        r#"
for action in (lambda: pyx.rename(a, 'Périmée'), lambda: pyx.remove(a), lambda: pyx.set_values(a, [8.]), lambda: pyx.set_column_meta(a, unit='s'), lambda: pyx.create_workbook(parent=a), lambda: pyx.create_worksheet(a), lambda: pyx.create_column(a)):
    try:
        action()
    except pyx.BrokenReferenceError:
        pass
    else:
        raise AssertionError('ancienne époque acceptée')
"#,
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn metadonnees_partielles_preservent_les_champs_et_la_formule_est_protegee() {
    let banc = banc();
    banc.session
        .execute(Command::SetColumnMeta {
            column: banc.b,
            meta: ColumnMeta {
                comment: "À préserver".into(),
                ..Default::default()
            },
        })
        .unwrap();
    run(
        &banc,
        r#"
b = pyx.resolve('[Book1]Sheet1!B')
pyx.set_column_meta(b, unit='s', role='y')
assert pyx.info(b)['column']['comment'] == 'À préserver'
pyx.set_column_meta(b, formula='A * 2')
assert pyx.info(b)['column']['formula'] == 'A * 2'
try:
    pyx.set_values(b, [123.])
except pyx.InvalidValueError:
    pass
else:
    raise AssertionError('colonne-formule écrite')
pyx.set_column_meta(b, formula=None)
pyx.set_values(b, [123.])
"#,
    );
    run(
        &banc,
        "assert pyx.values(b) == (123.,)\nassert pyx.info(b)['column']['type'] == 'mixed'",
    );
    banc.session.undo().unwrap();
    assert_eq!(floats(&banc, banc.b), vec![9.; 5]);
}

#[test]
fn lecture_sans_macro_et_annulation_explicite_par_groupes() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    run(&banc, "pyx.values(pyx.resolve('[Book1]Sheet1!A'))");
    assert_eq!(banc.session.snapshot().unwrap().revision, before.revision);
    run(
        &banc,
        r#"
a = pyx.resolve('[Book1]Sheet1!A')
pyx.set_values(a, [4.])
pyx.set_values(a, [5.])
assert pyx.undo()
assert pyx.values(a) == (1., 2., 3.)
assert not pyx.undo()
pyx.set_values(a, [6.])
"#,
    );
    assert_eq!(floats(&banc, banc.a), vec![6.]);
    banc.session.undo().unwrap();
    assert_eq!(*banc.session.snapshot().unwrap().tree, *before.tree);
}

#[test]
fn interruption_annule_le_groupe_et_libere_la_session() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    let engine = Arc::clone(&banc.engine);
    let job = std::thread::spawn(move || {
        engine
            .run(
                r#"
a = pyx.resolve('[Book1]Sheet1!A')
pyx.create_workbook('Interrompu')
pyx.set_values(a, [4., 5., 6., 7.])
while True:
    pyx.set_values(a, [8., 9.])
"#,
            )
            .unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = banc.session.snapshot().unwrap();
        if snapshot
            .tree
            .node(banc.a)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_slice()
            != [1., 2., 3.]
        {
            assert!(snapshot.history.in_macro);
            assert!(matches!(
                banc.session.execute(Command::SetCells {
                    column: banc.b,
                    start: 0,
                    values: ValueBlock::Float(vec![20.]),
                    expected_version: None
                }),
                Err(SessionError::Command(CommandError::ForeignMacro("execute")))
            ));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "le script n'a pas commencé à écrire"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    banc.engine.interrupt();
    let outcome = job.join().unwrap();
    assert!(outcome.interrupted);
    let after = banc.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    run(
        &banc,
        "pyx.set_values(pyx.resolve('[Book1]Sheet1!A'), [42.])",
    );
    assert_eq!(floats(&banc, banc.a), vec![42.]);
}

#[test]
fn imports_et_ecritures_partagent_le_groupe_du_script() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mesures.csv");
    std::fs::write(&path, "x;y\n1;2\n3;4\n").unwrap();
    run(
        &banc,
        &format!(
            r#"
sheets = pyx.import_file({path:?})
column = pyx.children(sheets[0])[0]
source = pyx.info(column)['column']['comment']
pyx.set_column_meta(column, unit='s')
assert pyx.info(column)['column']['comment'] == source
pyx.rename(sheets[0], 'Importée')
pyx.set_values(pyx.resolve('[Book1]Sheet1!A'), [99.])
"#,
            path = path.to_str().unwrap()
        ),
    );
    let after = banc.session.snapshot().unwrap();
    assert_eq!(
        after.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    banc.session.undo().unwrap();
    assert_eq!(*banc.session.snapshot().unwrap().tree, *before.tree);
    assert!(!banc.session.snapshot().unwrap().history.can_undo);
}
