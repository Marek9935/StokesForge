//! Figer l'API `pyst` (#331, lot 1) : paramètres en anglais, `redo`, `save` /
//! `save_as`, `export_graph` et `analysis.readiness`.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, NodeId, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    a: NodeId,
}

/// `[Book1]Sheet1` : `A` = 1, 2, 3 et `B` = 4, 5, 6.
fn banc() -> Banc {
    let mut project = Project::new("Figer").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
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

const HANDLES: &str = "a = pyst.resolve('[Book1]Sheet1!A')\nb = pyst.resolve('[Book1]Sheet1!B')\n";

/// Le script doit lever une exception du type `kind` ; rend son message.
fn refused(banc: &Banc, kind: &str, body: &str) -> String {
    refused_after(banc, kind, "", body)
}

/// Comme [`refused`], avec une préparation `setup` hors du `try` : une erreur
/// de la préparation ne doit pas passer pour le refus attendu.
fn refused_after(banc: &Banc, kind: &str, setup: &str, body: &str) -> String {
    let source = format!(
        "{HANDLES}{setup}try:\n    {body}\nexcept pyst.{kind} as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no {kind}')\nprint(caught)"
    );
    run(banc, &source).stdout.trim().to_owned()
}

#[test]
fn les_parametres_sont_en_anglais() {
    let b = banc();
    run(
        &b,
        r#"
import numpy as np
h = pyst.resolve(path='[Book1]Sheet1!A')
assert list(pyst.column(handle=h)) == [1.0, 2.0, 3.0]
pyst.set_column(handle=h, values=np.array([7.0, 8.0, 9.0]))
assert list(pyst.column(h)) == [7.0, 8.0, 9.0]
for call in (
    lambda: pyst.resolve(chemin='[Book1]Sheet1!A'),
    lambda: pyst.column(poignee=h),
    lambda: pyst.set_column(h, valeurs=np.array([1.0, 2.0, 3.0])),
):
    try:
        call()
    except TypeError:
        pass
    else:
        raise AssertionError('French keyword must be refused')
"#,
    );
    assert_eq!(floats(&b, b.a), vec![7.0, 8.0, 9.0]);
}

#[test]
fn redo_retablit_ce_que_undo_a_annule() {
    let b = banc();
    run(
        &b,
        &format!(
            "{HANDLES}assert pyst.redo() is False\n\
             pyst.set_values(a, [9.0, 9.0, 9.0])\n\
             assert pyst.undo() is True\n\
             assert list(pyst.values(a)) == [1.0, 2.0, 3.0]\n\
             assert pyst.redo() is True\n\
             assert list(pyst.values(a)) == [9.0, 9.0, 9.0]\n\
             assert pyst.redo() is False\n"
        ),
    );
    assert_eq!(floats(&b, b.a), vec![9.0, 9.0, 9.0]);
}

#[test]
fn save_sans_chemin_est_refuse_puis_save_as_fixe_le_chemin() {
    let b = banc();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("projet.stf");
    let message = refused(&b, "InvalidValueError", "pyst.save()");
    assert!(message.contains("save_as"), "{message}");
    assert!(!file.exists());

    run(
        &b,
        &format!(
            "{HANDLES}pyst.save_as({file:?})\n\
             assert pyst.info(a)['name'] == 'A'\n"
        ),
    );
    assert!(file.metadata().unwrap().len() > 0);
    assert_eq!(b.engine.project_path().as_deref(), Some(file.as_path()));

    // `save()` écrit maintenant au chemin retenu : supprimer le fichier le
    // prouve, car seul un enregistrement le refait.
    std::fs::remove_file(&file).unwrap();
    run(
        &b,
        &format!("{HANDLES}pyst.save()\nassert pyst.info(a)['name'] == 'A'"),
    );
    assert!(file.exists());
    // Le fichier est un projet relisible, avec les données du moment.
    b.session.load(&file).unwrap();
    let snapshot = b.session.snapshot().unwrap();
    let sheet = snapshot.tree.resolve("[Book1]Sheet1!A").unwrap();
    assert_eq!(
        snapshot
            .tree
            .node(sheet)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_ref(),
        &vec![1.0, 2.0, 3.0]
    );
}

#[test]
fn save_utilise_le_chemin_pose_par_lhote_et_valide_le_groupe() {
    let b = banc();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hote.stf");
    b.engine.set_project_path(Some(file.clone()));
    // L'écriture précède `save()` dans le même groupe : sans validation du
    // groupe, la session refuserait d'écrire (macro ouverte). Une erreur
    // ultérieure n'annule plus ce qui a été validé.
    let outcome = b
        .engine
        .run(&format!(
            "{HANDLES}pyst.set_values(a, [5.0, 5.0, 5.0])\npyst.save()\nraise RuntimeError('after')"
        ))
        .unwrap();
    assert!(outcome.error.is_some());
    assert!(file.exists());
    assert_eq!(floats(&b, b.a), vec![5.0, 5.0, 5.0]);
    b.session.load(&file).unwrap();
    assert_eq!(floats(&b, b.a), vec![5.0, 5.0, 5.0]);
}

fn graphe(b: &Banc) -> &'static str {
    let _ = b;
    "g = pyst.plot(b, a, name='Graph1', title='Titre')\n"
}

#[test]
fn export_graph_ecrit_svg_pdf_et_png_comme_lapplication() {
    let b = banc();
    let dir = tempfile::tempdir().unwrap();
    let (svg, pdf, png, png1) = (
        dir.path().join("f.svg"),
        dir.path().join("f.PDF"),
        dir.path().join("f.png"),
        dir.path().join("f1.png"),
    );
    run(
        &b,
        &format!(
            "{HANDLES}{}pyst.export_graph(g, {svg:?})\n\
             pyst.export_graph(g, {pdf:?})\n\
             pyst.export_graph(g, {png:?})\n\
             pyst.export_graph(g, {png1:?}, png_scale=1.0)\n",
            graphe(&b)
        ),
    );
    // Même rendu que l'application : mêmes octets que `export_bytes` sur le
    // spec du graphe, taille et mode de texte par défaut.
    let snapshot = b.session.snapshot().unwrap();
    let node = snapshot.tree.resolve("[Graph1]").unwrap();
    let spec = match snapshot.tree.node(node).unwrap().kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        other => panic!("{other:?}"),
    };
    let plot = stokesforge_core::resolve::resolve_spec(&snapshot.tree, &spec);
    let expected = stokesforge_render_svg::export_bytes(
        &plot,
        stokesforge_render_svg::DEFAULT_SIZE,
        &stokesforge_render_svg::SvgTextMeasurer::default(),
        &stokesforge_render_svg::FontChoice::default(),
        stokesforge_render_svg::ExportFormat::Svg {
            text: stokesforge_render_svg::SvgTextMode::Text,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&svg).unwrap(), expected);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let big = std::fs::read(&png).unwrap();
    let small = std::fs::read(&png1).unwrap();
    assert!(big.starts_with(b"\x89PNG"));
    let width = |bytes: &[u8]| u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(width(&big), 4 * width(&small), "png_scale agrandit le PNG");
}

#[test]
fn export_graph_refuse_extension_inconnue_echelle_nulle_et_non_graphe() {
    let b = banc();
    let dir = tempfile::tempdir().unwrap();
    let jpg = dir.path().join("f.jpg");
    let (png, svg) = (dir.path().join("f.png"), dir.path().join("f.svg"));
    let setup = graphe(&b);
    let message = refused_after(
        &b,
        "InvalidValueError",
        setup,
        &format!("pyst.export_graph(g, {jpg:?})"),
    );
    assert!(message.contains(".svg"), "{message}");
    assert!(!jpg.exists());
    // Refusé aussi pour un format qui ignore l'échelle (le SVG) : un
    // paramètre faux n'est pas toléré en silence.
    for path in [&png, &svg] {
        let message = refused_after(
            &b,
            "InvalidValueError",
            "",
            &format!("pyst.export_graph(g, {path:?}, png_scale=0.0)"),
        );
        assert!(message.contains("png_scale"), "{message}");
        assert!(!path.exists());
    }
    let message = refused(
        &b,
        "InvalidValueError",
        &format!("pyst.export_graph(a, {svg:?})"),
    );
    assert!(message.contains("not a graph"), "{message}");
    assert!(!svg.exists());
}

#[test]
fn readiness_dit_ce_qui_manque_sans_rien_ecrire() {
    let b = banc();
    let before = b.session.snapshot().unwrap();
    run(
        &b,
        "nan = float('nan')\n\
         r = pyst.analysis.readiness('baseline', method='line', points=[(0., 0.), (nan, 0.)])\n\
         assert r['ready'] is False and r['reason'] == 'needs_points', r\n\
         assert (r['needed'], r['have']) == (2, 1), r\n\
         assert pyst.analysis.readiness('baseline', method='line', points=[(0., 0.), (1., 1.)])['ready'] is True\n\
         r = pyst.analysis.readiness('baseline', method='zones', zones=[(nan, 1.)])\n\
         assert r['reason'] == 'needs_zone', r\n\
         r = pyst.analysis.readiness('integrate', ranges=[(nan, 1.)])\n\
         assert r['reason'] == 'needs_zone', r\n\
         assert pyst.analysis.readiness('integrate')['ready'] is True\n\
         r = pyst.analysis.readiness('fit', model='a*')\n\
         assert r['reason'] == 'bad_expression' and r['message'], r\n\
         assert pyst.analysis.readiness('fit')['ready'] is True\n\
         r = pyst.analysis.readiness('fit', model='1+2')\n\
         assert r['reason'] == 'no_parameter', r\n\
         assert pyst.analysis.readiness('peaks')['ready'] is True\n",
    );
    for body in [
        "pyst.analysis.readiness('fit', modle='linear')",
        "pyst.analysis.readiness('peaks', prominence=0.1)",
        "pyst.analysis.readiness('nothing')",
        "pyst.analysis.readiness('integrate', ranges='x')",
    ] {
        refused(&b, "InvalidValueError", body);
    }
    // Lecture seule : ni projet ni historique touchés.
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}
