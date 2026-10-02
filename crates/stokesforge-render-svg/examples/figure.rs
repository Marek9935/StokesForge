//! Rendu **visible** hors écran — tâche R4.6 (moitié export), élargie par
//! R5 au graphe réaliste.
//!
//! ```sh
//! cargo run -p stokesforge-render-svg --example figure             # ./campagne.svg
//! cargo run -p stokesforge-render-svg --example figure /tmp/a.svg
//! cargo run -p stokesforge-render-svg --example figure /tmp/a.svg minimal
//! ```
//!
//! Le séquencement « le contrat d'abord » a un défaut connu : cinq semaines
//! sans rien voir bouger. Sur un chantier de rendu, le retour visuel n'est
//! pas un confort — c'est ce qui fait remonter les erreurs qu'aucun test ne
//! formule (`roadmap-rendu.md` §4.2). Cet exemple est la moitié qui se
//! regarde **sans serveur graphique** ; l'autre moitié est la fenêtre du
//! banc d'essai (`cargo run -p stokesforge-app`).
//!
//! Le graphe vient de [`stokesforge_render::sample`], et c'est **le même** que
//! celui du test d'accord — à dessein, et désormais par construction plutôt
//! que par recopie : ce qu'on regarde ici est exactement ce que
//! `stokesforge-render-egui/tests/accord.rs` compare entre les deux backends.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;

/// Le graphe réaliste a cinq panneaux : à 900 × 600 ils seraient illisibles.
const TAILLE: (u32, u32) = (1280, 860);
/// Le graphe minimal de R4, lui, tient dans la taille d'origine.
const TAILLE_MINIMALE: (u32, u32) = (900, 600);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let cible = args.next().unwrap_or_else(|| "campagne.svg".into());
    let minimal = args.next().is_some_and(|mot| mot == "minimal");

    let (echantillon, taille) = if minimal {
        (sample::minimal()?, TAILLE_MINIMALE)
    } else {
        (sample::realistic()?, TAILLE)
    };

    let plot = resolve_spec(echantillon.project.tree(), &echantillon.spec);
    let measurer = TextMeasurer::default();
    let document = stokesforge_render_svg::render_to_string(&plot, taille, &measurer)?;
    std::fs::write(&cible, document)?;

    let brisees = plot.broken_series().count();
    println!(
        "écrit : {cible} — {} panneaux, {} séries, {} points{}",
        echantillon.spec.panels.len(),
        echantillon.spec.series_count(),
        plot.point_count(),
        if brisees > 0 {
            format!(", dont {brisees} à référence brisée")
        } else {
            String::new()
        }
    );
    Ok(())
}
