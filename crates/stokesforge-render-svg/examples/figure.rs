//! **Visible** off-screen rendering — task R4.6 (export half), extended by
//! R5 to the realistic graph.
//!
//! ```sh
//! cargo run -p stokesforge-render-svg --example figure             # ./campaign.svg
//! cargo run -p stokesforge-render-svg --example figure /tmp/a.svg
//! cargo run -p stokesforge-render-svg --example figure /tmp/a.svg minimal
//! ```
//!
//! The "contract first" sequencing has a known flaw: five weeks
//! without seeing anything move. On a rendering work package, visual feedback is
//! not a comfort — it is what brings up the errors that no test
//! formulates (`roadmap-rendu.md` §4.2). This example is the half that can be
//! looked at **without a graphics server**; the other half is the test bench
//! window (`cargo run -p stokesforge-app`).
//!
//! The graph comes from [`stokesforge_render::sample`], and it is **the same** as
//! the agreement test's — on purpose, and now by construction rather than
//! by copying: what is looked at here is exactly what
//! `stokesforge-render-egui/tests/agreement.rs` compares between the two backends.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;

/// The realistic graph has five panels: at 900 × 600 they would be unreadable.
const SIZE: (u32, u32) = (1280, 860);
/// The minimal graph of R4, for its part, fits in the original size.
const MINIMAL_SIZE: (u32, u32) = (900, 600);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let target = args.next().unwrap_or_else(|| "campaign.svg".into());
    let minimal = args.next().is_some_and(|word| word == "minimal");

    let (sample, size) = if minimal {
        (sample::minimal()?, MINIMAL_SIZE)
    } else {
        (sample::realistic()?, SIZE)
    };

    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let measurer = TextMeasurer::default();
    let document = stokesforge_render_svg::render_to_string(&plot, size, &measurer)?;
    std::fs::write(&target, document)?;

    let broken = plot.broken_series().count();
    println!(
        "written: {target} — {} panels, {} series, {} points{}",
        sample.spec.panels.len(),
        sample.spec.series_count(),
        plot.point_count(),
        if broken > 0 {
            format!(", of which {broken} with a broken reference")
        } else {
            String::new()
        }
    );
    Ok(())
}
