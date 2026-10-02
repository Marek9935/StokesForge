//! Spike optionnel #69 : runtime isolé, SVG stable et conversion d'export.
use plotyx_core::Project;
use plotyx_python::interpreter::{self, EmbeddedPython};
use plotyx_script::ScriptEngine;
use plotyx_session::Session;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let prefix =
        PathBuf::from(std::env::var_os(interpreter::HOME_ENV).expect("runtime du spike requis"));
    let session = Session::spawn(Project::new("Matplotlib").unwrap(), None).unwrap();
    let start = Instant::now();
    let engine =
        ScriptEngine::spawn(session.handle(), EmbeddedPython::with_home(prefix.clone())).unwrap();
    let init_ms = start.elapsed().as_secs_f64() * 1000.;
    let dir = tempfile::tempdir().unwrap();
    let svg = dir.path().join("figure.svg");
    let second = dir.path().join("second.svg");
    let png = dir.path().join("agg.png");
    let source = format!(
        r#"
import os, sys, time
os.environ['MPLCONFIGDIR'] = {cfg:?}
t0 = time.perf_counter()
import numpy as np
import matplotlib
from matplotlib.figure import Figure
from matplotlib.backends.backend_svg import FigureCanvasSVG
from matplotlib.backends.backend_agg import FigureCanvasAgg
print('Matplotlib', matplotlib.__version__, 'import_ms', (time.perf_counter()-t0)*1000)
assert matplotlib.__version__ == '3.11.2'
assert matplotlib.__file__.startswith({prefix:?})
assert sys.flags.isolated == 1
matplotlib.rcParams['svg.hashsalt'] = 'plotyx-spike'
book = pyx.create_workbook('Entrée')
sheet = pyx.create_worksheet(book)
x = pyx.create_column(sheet)
y = pyx.create_column(sheet)
pyx.set_values(x, np.arange(4., dtype=np.float64))
pyx.set_values(y, pyx.column(x)**2)
fig = Figure(figsize=(4,3), dpi=100)
FigureCanvasSVG(fig)
ax = fig.subplots()
ax.plot(pyx.column(x), pyx.column(y), color='#123456', marker='o')
ax.set_title(r'$x^2$ — PlotYx')
fig.savefig({svg:?}, format='svg', metadata={{'Date':None}})
fig.savefig({second:?}, format='svg', metadata={{'Date':None}})
FigureCanvasAgg(fig)
fig.savefig({png:?}, format='png')
assert pyx.values(y) == (0.,1.,4.,9.)
"#,
        cfg = dir.path().to_str().unwrap(),
        prefix = prefix.to_str().unwrap(),
        svg = svg.to_str().unwrap(),
        second = second.to_str().unwrap(),
        png = png.to_str().unwrap()
    );
    let out = engine.run(&source).unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("initialisation_moteur_ms={init_ms:.3}\n{}", out.stdout);
    let document = std::fs::read_to_string(&svg).unwrap();
    assert!(document.contains("<svg"));
    assert_eq!(document, std::fs::read_to_string(&second).unwrap());
    let converted = plotyx_render_svg::png::document_to_png(&document, 1.).unwrap();
    assert!(converted.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(std::fs::read(&png)
        .unwrap()
        .starts_with(b"\x89PNG\r\n\x1a\n"));
    let pdf = plotyx_render_svg::pdf::document_to_pdf(&document).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    std::fs::copy(&svg, "/tmp/plotyx69-spike.svg").unwrap();
    std::fs::write("/tmp/plotyx69-spike.png", converted).unwrap();
    session.handle().undo().unwrap();
    assert!(session
        .handle()
        .snapshot()
        .unwrap()
        .tree
        .node(session.handle().snapshot().unwrap().tree.root())
        .unwrap()
        .children()
        .is_empty());
    engine.shutdown().unwrap();
    session.shutdown().unwrap();
}
