//! Contrat des écritures Python (#67) : commandes existantes, un groupe par
//! exécution, et retour arrière après une erreur ou une interruption.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use stokesforge_commands::{Command, CommandError, HistoryLabel, MacroKind};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionError, SessionHandle};

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
book = pyst.create_workbook('Résultats')
assert pyst.children(book) == []
sheet = pyst.create_worksheet(book, 'Calculs')
x = pyst.create_column(sheet, 'Temps')
y = pyst.create_column(sheet, 'Signal')
t = pyst.create_column(sheet, 'Texte', column_type='mixed')
d = pyst.create_column(sheet, 'Date', column_type='datetime')
pyst.set_values(x, [0, 1, 2])
pyst.set_values(y, [10.0, 20.0])
pyst.set_values(t, ['12', '', None, 'µ'])
pyst.set_values(d, [0.0, None])
pyst.set_column_meta(x, role='x', long_name='Temps écoulé', unit='s', comment='Essai', display_format='.2f')
assert pyst.info(x)['column']['unit'] == 's'
pyst.rename(y, 'Intensité')
assert pyst.info(y)['name'] == 'Intensité'
pyst.remove(d)
assert len(pyst.children(sheet)) == 3
assert pyst.info(sheet)['row_count'] == 4
assert pyst.info(x)['column']['role'] == 'x'
assert pyst.values(t) == ('12', '', None, 'µ')
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
a = pyst.resolve('[Book1]Sheet1!A')
old = pyst.column(a)
pyst.set_column_meta(a, unit='s')
pyst.set_values(a, np.arange(12., dtype=np.float64)[::2])
assert pyst.values(a) == (0., 2., 4., 6., 8., 10.)
pyst.set_values(a, [float('nan'), -0.0])
assert np.isnan(pyst.values(a)[0])
assert np.signbit(pyst.values(a)[1])
assert list(old) == [1., 2., 3.]
assert pyst.info(a)['column']['unit'] == 's'
assert pyst.info(pyst.resolve('[Book1]Sheet1!B'))['row_count'] == 5
"#,
    );
    assert_eq!(floats(&banc, banc.b), vec![9.0; 5]);
    assert_eq!(floats(&banc, banc.a).len(), 2);
    banc.session.undo().unwrap();
    assert_eq!(floats(&banc, banc.a), vec![1., 2., 3.]);
    banc.session.redo().unwrap();
    assert_eq!(floats(&banc, banc.a)[1].to_bits(), (-0.0_f64).to_bits());
    run(&banc, "pyst.set_values(a, [])");
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
sheet = pyst.info(pyst.resolve('[Book1]Sheet1!A'))['parent']
t = pyst.create_column(sheet, column_type='mixed')
source = [None, '', '12', float('nan'), -0.0, 2]
pyst.set_values(t, iter(source))
source[2] = 'modifié après la copie'
v = pyst.values(t)
assert v[:3] == (None, '', '12')
assert math.isnan(v[3])
assert math.copysign(1., v[4]) == -1.
assert v[5] == 2.
pyst.set_values(t, np.array([float('nan'), -0.0]))
v = pyst.values(t)
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
        "pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [4.])",
    );
    banc.session.undo().unwrap();
    let before = banc.session.snapshot().unwrap();
    let outcome = banc
        .engine
        .run(
            r#"
pyst.create_workbook('À annuler')
pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [7., 8., 9., 10.])
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
    run(&banc, "assert pyst.children()");
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
a = pyst.resolve('[Book1]Sheet1!A')
for values in (['texte'], [object()], [2**53 + 1], 'texte', np.array([1], dtype=np.int64), np.ones((2, 2)), ma.array([1.], mask=[True]), np.broadcast_to(np.array([1.]), (100_000_001,))):
    try:
        pyst.set_values(a, values)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('valeur invalide acceptée')
class TropLong:
    def __len__(self): return 100_000_001
    def __iter__(self): raise AssertionError('le plafond doit précéder la copie')
try:
    pyst.set_values(a, TropLong())
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('plafond ignoré')
for name in ('', 'x' * 256, 'B', 'x!y'):
    try:
        pyst.rename(a, name)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('nom invalide accepté')
for changes in ({'role': 'inconnu'}, {'type': 'mixed'}, {'version': 1}, {'unit': None}):
    try:
        pyst.set_column_meta(a, **changes)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('métadonnée invalide acceptée')
try:
    pyst.create_worksheet(a)
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('mauvais parent accepté')
try:
    pyst.remove(pyst.root())
except pyst.InvalidValueError:
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
    run(&banc, "a = pyst.resolve('[Book1]Sheet1!A')\npyst.remove(a)");
    run(
        &banc,
        r#"
try:
    pyst.rename(a, 'Périmée')
except pyst.BrokenReferenceError:
    pass
else:
    raise AssertionError('poignée supprimée acceptée')
"#,
    );
    banc.session.undo().unwrap();
    run(&banc, "pyst.rename(a, 'Renommée')");
    banc.session.undo().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.stf");
    banc.session.save(&path).unwrap();
    banc.session.load(&path).unwrap();
    let before = banc.session.snapshot().unwrap();
    run(
        &banc,
        r#"
for action in (lambda: pyst.rename(a, 'Périmée'), lambda: pyst.remove(a), lambda: pyst.set_values(a, [8.]), lambda: pyst.set_column_meta(a, unit='s'), lambda: pyst.create_workbook(parent=a), lambda: pyst.create_worksheet(a), lambda: pyst.create_column(a)):
    try:
        action()
    except pyst.BrokenReferenceError:
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
b = pyst.resolve('[Book1]Sheet1!B')
pyst.set_column_meta(b, unit='s', role='y')
assert pyst.info(b)['column']['comment'] == 'À préserver'
pyst.set_column_meta(b, formula='A * 2')
assert pyst.info(b)['column']['formula'] == 'A * 2'
try:
    pyst.set_values(b, [123.])
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('colonne-formule écrite')
pyst.set_column_meta(b, formula=None)
pyst.set_values(b, [123.])
"#,
    );
    run(
        &banc,
        "assert pyst.values(b) == (123.,)\nassert pyst.info(b)['column']['type'] == 'mixed'",
    );
    banc.session.undo().unwrap();
    assert_eq!(floats(&banc, banc.b), vec![9.; 5]);
}

#[test]
fn lecture_sans_macro_et_annulation_explicite_par_groupes() {
    let banc = banc();
    let before = banc.session.snapshot().unwrap();
    run(&banc, "pyst.values(pyst.resolve('[Book1]Sheet1!A'))");
    assert_eq!(banc.session.snapshot().unwrap().revision, before.revision);
    run(
        &banc,
        r#"
a = pyst.resolve('[Book1]Sheet1!A')
pyst.set_values(a, [4.])
pyst.set_values(a, [5.])
assert pyst.undo()
assert pyst.values(a) == (1., 2., 3.)
assert not pyst.undo()
pyst.set_values(a, [6.])
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
a = pyst.resolve('[Book1]Sheet1!A')
pyst.create_workbook('Interrompu')
pyst.set_values(a, [4., 5., 6., 7.])
while True:
    pyst.set_values(a, [8., 9.])
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
        "pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [42.])",
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
sheets = pyst.import_file({path:?})
column = pyst.children(sheets[0])[0]
source = pyst.info(column)['column']['comment']
pyst.set_column_meta(column, unit='s')
assert pyst.info(column)['column']['comment'] == source
pyst.rename(sheets[0], 'Importée')
pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [99.])
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
