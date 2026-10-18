//! `pyst.analysis` (#181) : les mêmes calculs que le menu Analysis, depuis un
//! script — mêmes colonnes, même trace, un seul groupe d'annulation.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_analysis::ops::OperationKind;
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    sheet: NodeId,
}

/// Une feuille `[Book1]Sheet1` : X = 0..20, Y = 2x + 1, Ref = 2.
fn banc() -> Banc {
    let mut project = Project::new("Analyse").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let xs: Vec<f64> = (0..21).map(f64::from).collect();
    let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&xs))
        .unwrap();
    project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&ys))
        .unwrap();
    project
        .add_column(sheet, None, Some("Ref"), Column::from_f64(&[2.0; 21]))
        .unwrap();
    project
        .set_column_meta(
            x,
            ColumnMeta {
                role: ColumnRole::X,
                ..ColumnMeta::default()
            },
        )
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
        sheet,
    }
}

const SETUP: &str = "
x = pyst.resolve('[Book1]Sheet1!X')
y = pyst.resolve('[Book1]Sheet1!Y')
ref = pyst.resolve('[Book1]Sheet1!Ref')
";

fn run(banc: &Banc, source: &str) -> ScriptOutcome {
    let outcome = banc.engine.run(&format!("{SETUP}{source}")).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

fn columns(banc: &Banc) -> usize {
    let snapshot = banc.session.snapshot().unwrap();
    snapshot.tree.children(banc.sheet).unwrap().len()
}

#[test]
fn un_ajustement_rend_les_parametres_et_ecrit_une_colonne_annulable() {
    let banc = banc();
    let before = columns(&banc);
    let outcome = run(
        &banc,
        r#"
r = pyst.analysis.fit(y, x, model='linear', residuals=True)
assert abs(r['values']['param.slope'] - 2.0) < 1e-6, r['values']
assert abs(r['values']['param.intercept'] - 1.0) < 1e-6, r['values']
assert 'param.slope' in r['errors']
assert set(r['columns']) == {'fit', 'residuals'}, r['columns']
assert abs(pyst.column(r['columns']['fit'])[3] - 7.0) < 1e-6
print('ok')
"#,
    );
    assert!(outcome.stdout.contains("ok"), "{}", outcome.stdout);
    assert_eq!(columns(&banc), before + 2);
    let snapshot = banc.session.snapshot().unwrap();
    let calc = snapshot.tree.calculations().values().next().unwrap();
    assert_eq!(calc.operation.kind(), OperationKind::Fit);
    assert!(calc.auto_recalc);
    // Un seul Ctrl+Z annule le calcul et ses colonnes.
    banc.session.undo().unwrap();
    assert_eq!(columns(&banc), before);
    assert!(banc
        .session
        .snapshot()
        .unwrap()
        .tree
        .calculations()
        .is_empty());
}

#[test]
fn integrale_correction_ligne_de_base_et_pics_ecrivent_leurs_colonnes() {
    let banc = banc();
    let outcome = run(
        &banc,
        r#"
i = pyst.analysis.integrate(y, x, cumulative=True)
assert abs(i['values']['area.total'] - 420.0) < 1e-6, i['values']
assert 'cumulative' in i['columns']
c = pyst.analysis.correct(y, ref, x, op='divide')
assert abs(pyst.column(c['columns']['corrected'])[4] - 4.5) < 1e-9
b = pyst.analysis.baseline(y, x, method='line', points=[(0, 1), (20, 41)], keep_baseline=True)
assert set(b['columns']) == {'corrected', 'baseline'}
assert max(abs(v) for v in pyst.column(b['columns']['corrected'])) < 1e-9
print('ok')
"#,
    );
    assert!(outcome.stdout.contains("ok"));
    let snapshot = banc.session.snapshot().unwrap();
    assert_eq!(snapshot.tree.calculations().len(), 3);
    // Les noms longs disent la source.
    let corrected = snapshot
        .tree
        .calculations()
        .values()
        .find(|c| c.operation.kind() == OperationKind::Correct)
        .unwrap()
        .outputs[0]
        .column
        .unwrap();
    let meta = snapshot
        .tree
        .node(corrected)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    assert_eq!(meta.long_name, "Y (corrected)");
}

#[test]
fn un_argument_invalide_leve_invalid_value_error_sans_rien_ecrire() {
    let banc = banc();
    let before = columns(&banc);
    for bad in [
        "pyst.analysis.fit(y, x, model='a*')",
        "pyst.analysis.fit(y, x, model='linear', initial={'zz': 1.0})",
        "pyst.analysis.baseline(y, x, method='line', points=[(0, 1)])",
        "pyst.analysis.baseline(y, x, op='nope')",
        "pyst.analysis.integrate(y, x, under='sideways')",
        "pyst.analysis.peaks(y, x, prominence=0.0)",
        "pyst.analysis.reconvolve(y, ref, x, components=9)",
    ] {
        run(
            &banc,
            &format!(
                "
try:
    {bad}
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('call accepted')
"
            ),
        );
    }
    assert_eq!(columns(&banc), before);
    assert!(banc
        .session
        .snapshot()
        .unwrap()
        .tree
        .calculations()
        .is_empty());
}

#[test]
fn une_colonne_de_calcul_refuse_l_ecriture_directe() {
    let banc = banc();
    run(
        &banc,
        r#"
r = pyst.analysis.integrate(y, x, cumulative=True)
col = r['columns']['cumulative']
try:
    pyst.set_values(col, [1.0, 2.0])
except pyst.InvalidValueError as e:
    assert 'calculation' in str(e), e
else:
    raise AssertionError('write accepted')
"#,
    );
}

#[test]
fn le_calcul_se_recalcule_quand_la_source_change() {
    let banc = banc();
    run(
        &banc,
        r#"
r = pyst.analysis.correct(y, ref, x, op='divide')
out = r['columns']['corrected']
assert pyst.column(out)[2] == 2.5
# La référence est figée en copie (#248) : modifier l'original ne change rien.
pyst.set_values(ref, [4.0] * 21)
assert pyst.column(out)[2] == 2.5, list(pyst.column(out))[:4]
# La source, elle, est relue : y / 2 = 5 / 2.
pyst.set_values(y, [10.0] * 21)
assert pyst.column(out)[2] == 5.0, list(pyst.column(out))[:4]
"#,
    );
}

#[test]
fn les_pics_sont_detectes_et_chaque_pic_a_sa_colonne() {
    let banc = banc();
    let outcome = run(
        &banc,
        r#"
import numpy as np
book = pyst.create_workbook('Spectre')
sheet = pyst.create_worksheet(book, 'S')
sx = pyst.create_column(sheet, 'Lambda')
sy = pyst.create_column(sheet, 'Counts')
xs = np.linspace(0, 10, 201)
ys = 3 * np.exp(-0.5 * ((xs - 3) / 0.4) ** 2) + 2 * np.exp(-0.5 * ((xs - 7) / 0.4) ** 2) + 0.1
pyst.set_values(sx, xs)
pyst.set_values(sy, ys)
pyst.set_column_meta(sx, role='x')
r = pyst.analysis.peaks(sy, sx, shape='gaussian', background='constant', max_peaks=2)
assert set(r['columns']) >= {'peak.1', 'peak.2', 'peaks_sum', 'background'}, r['columns']
centers = sorted([r['values']['peak.center.1'], r['values']['peak.center.2']])
assert abs(centers[0] - 3) < 0.05 and abs(centers[1] - 7) < 0.05, centers
print('ok')
"#,
    );
    assert!(outcome.stdout.contains("ok"), "{}", outcome.stdout);
}

/// Les colonnes écrites par le dernier calcul : `(clé, nom long, unité, commentaire)`,
/// triées par clé (le rang d'un pic est dans la clé : `peak` tout court, le
/// nom long dit le numéro).
fn sorties(banc: &Banc) -> Vec<(String, String, String, String)> {
    let snapshot = banc.session.snapshot().unwrap();
    let (_, calc) = snapshot.tree.calculations().iter().next_back().unwrap();
    let mut out: Vec<_> = calc
        .outputs
        .iter()
        .map(|o| {
            let meta = snapshot
                .tree
                .node(o.column.unwrap())
                .unwrap()
                .as_column()
                .unwrap()
                .meta()
                .clone();
            (o.key.clone(), meta.long_name, meta.unit, meta.comment)
        })
        .collect();
    out.sort();
    out
}

/// #256 : les noms, l'unité et le commentaire sont ceux de la fenêtre
/// d'analyse (règle unique de `stokesforge-core`) — mêmes attentes que
/// `the_saved_batch_names_columns_after_their_source` côté interface.
#[test]
fn la_ligne_de_base_porte_les_noms_l_unite_et_le_commentaire_de_l_interface() {
    let banc = banc();
    run(
        &banc,
        "
pyst.set_column_meta(y, long_name='Intensity', unit='counts')
pyst.analysis.baseline(y, x, method='line', points=[(0, 1), (20, 41)], keep_baseline=True)
",
    );
    let sorties = sorties(&banc);
    let colonne = |cle: &str| sorties.iter().find(|s| s.0 == cle).unwrap().clone();
    // La source s'appelle « Y » : le nom court de la correction est « Y ».
    assert_eq!(colonne("corrected").1, "Intensity (Y-corr)");
    assert_eq!(colonne("baseline").1, "Baseline of Y");
    for cle in ["corrected", "baseline"] {
        assert_eq!(colonne(cle).2, "counts", "{cle}");
        assert_eq!(colonne(cle).3, "Baseline", "{cle}");
    }
    assert_eq!(sorties.len(), 2);
}

#[test]
fn sans_ligne_de_base_ecrite_seule_la_courbe_corrigee_est_ajoutee() {
    let banc = banc();
    run(
        &banc,
        "pyst.analysis.baseline(y, x, method='line', points=[(0, 1), (20, 41)])",
    );
    let sorties = sorties(&banc);
    assert_eq!(sorties.len(), 1, "{sorties:?}");
    // Sans nom long, le nom court sert des deux côtés.
    assert_eq!(sorties[0].1, "Y (Y-corr)");
}

const DEUX_PICS: &str = "
import numpy as np
book = pyst.create_workbook('Spectre')
sheet = pyst.create_worksheet(book, 'S')
sx = pyst.create_column(sheet, 'Lambda')
sy = pyst.create_column(sheet, 'Counts')
xs = np.linspace(0, 10, 201)
ys = 3 * np.exp(-0.5 * ((xs - 3) / 0.4) ** 2) + 2 * np.exp(-0.5 * ((xs - 7) / 0.4) ** 2) + 0.1
pyst.set_values(sx, xs)
pyst.set_values(sy, ys)
pyst.set_column_meta(sx, role='x')
";

/// #256 : `peaks` a les deux options de la fenêtre, cochées par défaut.
#[test]
fn les_pics_ecrivent_par_defaut_les_pics_le_fond_et_la_somme() {
    let banc = banc();
    run(
        &banc,
        &format!("{DEUX_PICS}\nr = pyst.analysis.peaks(sy, sx, max_peaks=2)\nprint(sorted(r['columns']))"),
    );
    let sorties = sorties(&banc);
    let noms: Vec<(&str, &str)> = sorties
        .iter()
        .map(|s| (s.0.as_str(), s.1.as_str()))
        .collect();
    assert_eq!(noms.len(), 4, "{noms:?}");
    for attendu in [
        ("background", "Background of Counts"),
        ("peaks_sum", "Sum of peaks of Counts"),
    ] {
        assert!(noms.contains(&attendu), "{noms:?}");
    }
    let pics: Vec<&str> = sorties
        .iter()
        .filter(|s| s.0 == "peak")
        .map(|s| s.1.as_str())
        .collect();
    assert_eq!(pics.len(), 2);
    assert!(pics.contains(&"Peak 1 of Counts") && pics.contains(&"Peak 2 of Counts"));
    assert!(sorties.iter().all(|s| s.3 == "Peak Decomposition"));
}

#[test]
fn les_options_des_pics_retirent_les_colonnes_comme_les_cases_de_la_fenetre() {
    let cles = |options: &str| {
        let banc = banc();
        run(
            &banc,
            &format!("{DEUX_PICS}\npyst.analysis.peaks(sy, sx, max_peaks=2{options})"),
        );
        let mut cles: Vec<String> = sorties(&banc).into_iter().map(|s| s.0).collect();
        cles.dedup();
        cles
    };
    assert_eq!(cles(", peak_columns=False"), ["peaks_sum"]);
    assert_eq!(
        cles(", sum_column=False"),
        ["background", "peak"],
        "pas de somme"
    );
    assert!(cles(", peak_columns=False, sum_column=False").is_empty());
}

/// #204 : `"exponential"` est l'exponentielle simple (`A·exp(−x/τ)`), comme dans
/// le menu Fit ; l'avec-palier et la double ont leur propre nom ; la fenêtre
/// de centre borne les pics.
#[test]
fn les_trois_exponentielles_et_la_fenetre_de_centre_ont_leur_nom() {
    let banc = banc();
    let outcome = run(
        &banc,
        r#"
import numpy as np
book = pyst.create_workbook('Declin')
sheet = pyst.create_worksheet(book, 'S')
t = pyst.create_column(sheet, 'T')
v = pyst.create_column(sheet, 'V')
ts = np.linspace(0, 10, 201)
pyst.set_values(t, ts)
pyst.set_column_meta(t, role='x')
pyst.set_values(v, 4.0 * np.exp(-ts / 2.5) + 1.0)
simple = pyst.analysis.fit(v, t, model='exponential', initial={'A': 4.0, 'tau': 2.5}, fixed=['A'])
assert sorted(k for k in simple['values'] if k.startswith('param.')) == ['param.A', 'param.tau'], simple['values']
offset = pyst.analysis.fit(v, t, model='exponential_offset')
assert abs(offset['values']['param.y0'] - 1.0) < 1e-4, offset['values']
assert abs(offset['values']['param.tau'] - 2.5) < 1e-4, offset['values']
double = pyst.analysis.fit(v, t, model='exponential2')
assert 'param.tau2' in double['values'], double['values']
ps = 3 * np.exp(-0.5 * ((ts - 3) / 0.4) ** 2) + 0.1
pyst.set_values(v, ps)
free = pyst.analysis.peaks(v, t, centers=[3.2], max_peaks=1)
bound = pyst.analysis.peaks(v, t, centers=[3.2], center_window=0.1, max_peaks=1)
assert abs(bound['values']['peak.center.1'] - 3.2) <= 0.1 + 1e-9, bound['values']
assert abs(free['values']['peak.center.1'] - 3.0) < 0.05, free['values']
print('ok')
"#,
    );
    assert!(outcome.stdout.contains("ok"), "{}", outcome.stdout);
}

/// #201 : `y` peut être une liste — un seul calcul multi-séries, rendu série par
/// série, annulé d'un seul coup ; `shared=[…]` en fait un ajustement global.
#[test]
fn un_lot_est_un_seul_calcul_multi_series_annulable_d_un_coup() {
    let banc = banc();
    let outcome = run(
        &banc,
        r#"
import numpy as np
book = pyst.create_workbook('Lot')
sheet = pyst.create_worksheet(book, 'S')
t = pyst.create_column(sheet, 'T')
pyst.set_values(t, np.linspace(0, 10, 101))
pyst.set_column_meta(t, role='x')
ts = np.linspace(0, 10, 101)
ys = []
for k, amp in enumerate([2.0, 5.0, 9.0]):
    c = pyst.create_column(sheet, 'Y%d' % k)
    pyst.set_values(c, amp * np.exp(-ts / 3.0))
    ys.append(c)
r = pyst.analysis.fit(ys, t, model='exponential')
assert len(r['series']) == 3, r
for item, amp in zip(r['series'], [2.0, 5.0, 9.0]):
    assert abs(item['values']['param.A'] - amp) < 1e-4, item['values']
    assert 'fit' in item['columns'], item['columns']
g = pyst.analysis.fit(ys, [t, t, t], model='exponential', shared=['tau'])
taus = [item['values']['param.tau'] for item in g['series']]
assert max(taus) - min(taus) < 1e-12, taus
assert abs(taus[0] - 3.0) < 1e-4, taus
print('ok')
"#,
    );
    assert!(outcome.stdout.contains("ok"), "{}", outcome.stdout);
    let snapshot = banc.session.snapshot().unwrap();
    let calcs: Vec<_> = snapshot.tree.calculations().values().collect();
    assert_eq!(calcs.len(), 2, "un calcul par appel, pas par série");
    assert!(calcs.iter().all(|c| c.series.len() == 3));
    // Le script entier est un groupe d'annulation : un seul Ctrl+Z défait les
    // deux lots et leurs colonnes.
    banc.session.undo().unwrap();
    assert!(banc
        .session
        .snapshot()
        .unwrap()
        .tree
        .calculations()
        .is_empty());
}

#[test]
fn un_lot_mal_forme_est_refuse_avec_un_message() {
    let banc = banc();
    for (source, needle) in [
        ("pyst.analysis.fit([], x)", "empty"),
        ("pyst.analysis.fit([y, y], [x])", "one per series"),
        ("pyst.analysis.fit([y, 3], x)", "only handles"),
        ("pyst.analysis.fit(3, x)", "handle or a list"),
    ] {
        let outcome = banc.engine.run(&format!("{SETUP}{source}")).unwrap();
        let error = format!("{:?}", outcome.error);
        assert!(outcome.error.is_some(), "{source} devait échouer");
        assert!(error.contains(needle), "{source} : {error}");
    }
}

/// scipy fait partie de la pile embarquée (#191) : un script `pyst` ajuste une
/// exponentielle avec `scipy.optimize.curve_fit` et retrouve les paramètres,
/// puis les écrit dans une colonne annulable. L'interface n'en dépend pas ;
/// c'est le témoin que la roue est bien chargée depuis le préfixe embarqué.
#[test]
fn scipy_ajuste_une_exponentielle_depuis_un_script() {
    let banc = banc();
    let before = columns(&banc);
    let outcome = run(
        &banc,
        r#"
import numpy as np
import scipy
from scipy.optimize import curve_fit

assert 'site-packages' in scipy.__file__, scipy.__file__
t = np.linspace(0.0, 10.0, 80)
signal = 1.5 + 4.0 * np.exp(-t / 2.5)

def model(t, offset, amplitude, tau):
    return offset + amplitude * np.exp(-t / tau)

popt, _ = curve_fit(model, t, signal, p0=(1.0, 1.0, 1.0))
assert np.allclose(popt, (1.5, 4.0, 2.5), rtol=1e-6), popt

book = pyst.resolve('[Book1]Sheet1')
col = pyst.create_column(book, 'curve_fit')
pyst.set_values(col, model(np.arange(21.0), *popt))
assert abs(pyst.column(col)[0] - 5.5) < 1e-6
print('scipy', scipy.__version__)
"#,
    );
    assert!(outcome.stdout.contains("scipy 1."), "{}", outcome.stdout);
    assert_eq!(columns(&banc), before + 1);
    banc.session.undo().unwrap();
    assert_eq!(columns(&banc), before);
}

#[test]
fn ligne_de_base_constante_par_point_ou_moyenne_de_zone() {
    let banc = banc();
    let before = columns(&banc);
    run(
        &banc,
        r#"
p = pyst.analysis.baseline(y, x, method='constant', points=[(5., 4.)], keep_baseline=True)
z = pyst.analysis.baseline(y, x, method='constant', zones=[(1., 3.)])
assert p['values']['baseline.slope'] == 0.
assert p['values']['baseline.intercept'] == 4.
assert z['values']['baseline.intercept'] == 5.
assert pyst.column(p['columns']['corrected'])[0] == -3.
assert pyst.column(z['columns']['corrected'])[0] == -4.
for kwargs in [dict(points=[]), dict(points=[(0., 1.), (1., 2.)]), dict(points=[(0., 1.)], zones=[(1., 2.)]), dict(zones=[(1., 2.), (3., 4.)]), dict(points=[(0., float('nan'))])]:
    try:
        pyst.analysis.baseline(y, x, method='constant', **kwargs)
        assert False, kwargs
    except pyst.InvalidValueError:
        pass
"#,
    );
    assert_eq!(columns(&banc), before + 3);
    banc.session.undo().unwrap();
    assert_eq!(columns(&banc), before);
}

#[test]
fn remplacement_python_utilise_la_colonne_source_et_le_groupe_annulable() {
    let b = banc();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        r#"
r = pyst.analysis.baseline(y, x, method='constant', points=[(0.0, 1.0)], replace_original=True)
assert repr(r['columns']['corrected']) == repr(y)
assert abs(pyst.column(y)[3] - 6.0) < 1e-10
r = pyst.analysis.correct(y, ref, x, op='divide', replace_original=True)
assert repr(r['columns']['corrected']) == repr(y)
assert abs(pyst.column(y)[3] - 3.0) < 1e-10
"#,
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(columns(&b), 3);
    assert!(after.tree.calculations().is_empty());
    assert_eq!(after.tree.correction_traces().len(), 2);
    b.session.undo().unwrap();
    assert_eq!(b.session.snapshot().unwrap().tree, before.tree);
    b.session.redo().unwrap();
    assert_eq!(b.session.snapshot().unwrap().tree, after.tree);
}
