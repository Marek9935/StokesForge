//! `pyst.set_graph` (#331, lot 2, S4) : réglages d'un graphe existant par les
//! clés de Graph Properties, en une seule écriture annulable.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{AxisScale, LegendPosition, PlotSpec};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

fn banc() -> Banc {
    let mut project = Project::new("Graphe").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
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
    }
}

fn run(banc: &Banc, source: &str) -> ScriptOutcome {
    let outcome = banc.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

/// Crée le graphe `g` (un tracé de B en fonction de A) dans un script à part.
const GRAPH: &str = "a = pyst.resolve('[Book1]Sheet1!A')\nb = pyst.resolve('[Book1]Sheet1!B')\ng = pyst.plot(b, a, name='G')\n";

fn graph_spec(banc: &Banc) -> PlotSpec {
    let snapshot = banc.session.snapshot().unwrap();
    let id: NodeId = snapshot.tree.resolve("[G]").unwrap();
    match snapshot.tree.node(id).unwrap().kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        other => panic!("{other:?}"),
    }
}

fn refused(banc: &Banc, body: &str) -> String {
    let source = format!(
        "g = pyst.resolve('[G]')\ntry:\n    {body}\nexcept pyst.InvalidValueError as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no InvalidValueError')\nprint(caught)"
    );
    run(banc, &source).stdout.trim().to_owned()
}

#[test]
fn set_graph_regle_le_graphe_et_le_panneau() {
    let b = banc();
    run(&b, GRAPH);
    run(
        &b,
        "g = pyst.resolve('[G]')\n\
         pyst.set_graph(g, title='T', title_visible=True, stale_warning=False, decimal_comma=True,\n\
             background='#102030', title_color='#FF0000', panel_title='P',\n\
             x_title='Time', y_title='Signal', x_unit='s', y_unit='V',\n\
             x_scale='log', y_scale='linear', x_from=1, x_to=100,\n\
             y_from=10, y_to=0, x_grid=False, y_grid=True, y_inverted=True,\n\
             x_visible=True, y_visible=True,\n\
             legend_visible=False, legend_position='bottom_left')",
    );
    let spec = graph_spec(&b);
    assert_eq!(spec.title, "T");
    assert!(spec.title_visible);
    assert!(!spec.stale_warning);
    assert!(spec.decimal_comma);
    assert_eq!(spec.background, "#102030");
    assert_eq!(spec.title_color, "#FF0000");
    let panel = &spec.panels[0];
    assert_eq!(panel.title, "P");
    assert_eq!(panel.bottom.label, "Time");
    assert_eq!(panel.left.label, "Signal");
    assert_eq!(panel.bottom.unit, "s");
    assert_eq!(panel.left.unit, "V");
    assert_eq!(panel.bottom.scale, AxisScale::Log);
    assert_eq!(panel.left.scale, AxisScale::Linear);
    assert!(!panel.bottom.auto_range);
    assert_eq!(
        (panel.bottom.range_min, panel.bottom.range_max),
        (Some(1.0), Some(100.0))
    );
    assert_eq!(
        (panel.left.range_min, panel.left.range_max),
        (Some(10.0), Some(0.0))
    );
    assert!(!panel.bottom.grid);
    assert!(panel.left.grid);
    assert!(panel.left.inverted);
    assert!(!panel.legend.visible);
    assert_eq!(panel.legend.position, LegendPosition::BottomLeft);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), legend_position='top_left')",
    );
    assert_eq!(
        graph_spec(&b).panels[0].legend.position,
        LegendPosition::TopLeft
    );
}

#[test]
fn set_graph_ne_touche_pas_aux_cles_absentes_et_vise_l_axe_de_droite() {
    let b = banc();
    run(&b, GRAPH);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), title='T', x_title='X')",
    );
    let before = graph_spec(&b);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), axis='right', y_title='Right', y_scale='log')",
    );
    let after = graph_spec(&b);
    assert_eq!(after.title, "T");
    assert_eq!(after.panels[0].bottom.label, "X");
    assert_eq!(after.panels[0].right.label, "Right");
    assert_eq!(after.panels[0].right.scale, AxisScale::Log);
    assert_eq!(after.panels[0].left, before.panels[0].left);
}

#[test]
fn set_graph_est_une_seule_ecriture_annulable() {
    let b = banc();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    // `undo` dans le même script : tout ce que `set_graph` a posé revient d'un coup.
    run(
        &b,
        "g = pyst.resolve('[G]')\n\
         pyst.set_graph(g, title='T', x_title='X', y_scale='log', legend_visible=False)\n\
         assert pyst.undo() is True\n",
    );
    assert_eq!(graph_spec(&b), before);
    // Et par un Ctrl+Z de l'interface après l'exécution : une seule entrée.
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), title='T', x_title='X', y_scale='log')",
    );
    assert_eq!(graph_spec(&b).title, "T");
    let snapshot = b.session.snapshot().unwrap();
    assert!(snapshot.history.can_undo);
    b.session.undo().unwrap();
    assert_eq!(graph_spec(&b), before);
}

#[test]
fn set_graph_sans_effet_n_ecrit_rien() {
    let b = banc();
    run(&b, GRAPH);
    let history = b.session.snapshot().unwrap().history;
    run(
        &b,
        "g = pyst.resolve('[G]')\npyst.set_graph(g)\npyst.set_graph(g, legend_visible=True)",
    );
    assert_eq!(b.session.snapshot().unwrap().history, history);
}

#[test]
fn set_graph_refuse_sans_rien_ecrire() {
    let b = banc();
    run(&b, GRAPH);
    let before = graph_spec(&b);
    let history = b.session.snapshot().unwrap().history;
    let unknown = refused(&b, "pyst.set_graph(g, title='ok', titel='x')");
    assert!(unknown.contains("unknown graph option: titel"), "{unknown}");
    for body in [
        "pyst.set_graph(g, title=3)",
        "pyst.set_graph(g, x_scale='sqrt')",
        "pyst.set_graph(g, x_from=1)",
        "pyst.set_graph(g, x_from=2, x_to=2)",
        "pyst.set_graph(g, x_from=float('nan'), x_to=3)",
        "pyst.set_graph(g, x_scale='log', x_from=-1, x_to=5)",
        "pyst.set_graph(g, x_from=1, x_to=5, x_auto_range=True)",
        "pyst.set_graph(g, legend_visible='yes')",
        "pyst.set_graph(g, legend_position='middle')",
        "pyst.set_graph(g, background='red')",
        "pyst.set_graph(g, axis='top')",
        "pyst.set_graph(g, panel=(3, 3), title='x')",
        "pyst.set_graph(pyst.resolve('[Book1]Sheet1!A'), title='x')",
    ] {
        refused(&b, body);
    }
    assert_eq!(graph_spec(&b), before);
    assert_eq!(b.session.snapshot().unwrap().history, history);
}

#[test]
fn set_graph_rend_le_cadrage_automatique() {
    let b = banc();
    run(&b, GRAPH);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_from=1, x_to=2)");
    assert!(!graph_spec(&b).panels[0].bottom.auto_range);
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_auto_range=True)");
    assert!(graph_spec(&b).panels[0].bottom.auto_range);
}

#[test]
fn chaque_cle_annoncee_par_l_erreur_est_reellement_acceptee() {
    let b = banc();
    run(&b, GRAPH);
    let message = refused(&b, "pyst.set_graph(g, nope=1)");
    let list = message
        .split("(accepted: ")
        .nth(1)
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{message}"));
    let keys: Vec<&str> = list.split(", ").collect();
    assert!(keys.len() >= 20, "{keys:?}");
    for key in keys {
        // Un objet d'un type qu'aucune clé n'accepte : le refus porte sur la
        // valeur, jamais sur « unknown graph option ».
        let source = format!(
            "g = pyst.resolve('[G]')\ntry:\n    pyst.set_graph(g, {key}=object())\nexcept pyst.InvalidValueError as error:\n    print(str(error))"
        );
        let out = run(&b, &source).stdout;
        assert!(
            !out.is_empty() && !out.contains("unknown graph option"),
            "{key}: {out}"
        );
    }
}

/// #344 : le sens se lit sur l'ordre des bornes. `x_inverted=True` sur des
/// bornes croissantes les échange ; `x_from > x_to` donne un axe inversé ;
/// les clés `pyst` elles-mêmes ne changent pas.
#[test]
fn set_graph_oriente_les_axes_d_apres_l_ordre_des_bornes() {
    let b = banc();
    run(&b, GRAPH);
    run(
        &b,
        "pyst.set_graph(pyst.resolve('[G]'), x_from=1, x_to=5, y_from=7, y_to=2)",
    );
    let panel = &graph_spec(&b).panels[0];
    assert!(!panel.bottom.inverted);
    assert!(panel.left.inverted, "y_from=7 > y_to=2 : axe inversé");
    assert_eq!(
        (panel.left.range_min, panel.left.range_max),
        (Some(7.0), Some(2.0))
    );
    run(&b, "pyst.set_graph(pyst.resolve('[G]'), x_inverted=True)");
    let panel = &graph_spec(&b).panels[0];
    assert!(panel.bottom.inverted);
    assert_eq!(
        (panel.bottom.range_min, panel.bottom.range_max),
        (Some(5.0), Some(1.0)),
        "(1, 5) devient (5, 1)"
    );
}
