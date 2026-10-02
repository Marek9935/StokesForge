//! Les chemins sous-pixel de l'export (issue #91).
//!
//! `plotters-svg` n'écrit que des entiers. Un chemin sous-pixel
//! ([`stokesforge_render::subpixel`]) lui est donc confié en points fixes, avec
//! un drapeau dans `stroke-width` ; une fois le document fini, [`resolve`]
//! retrouve ces `<polyline>` et réécrit épaisseur et sommets en pixels
//! décimaux. Même lecture de la convention que le backend écran : une
//! fonctionnalité de tracé n'entre jamais dans un seul backend.

use stokesforge_render::subpixel::{split_width, SCALE};

/// Réécrit les `<polyline>` sous-pixel du document ; le reste est inchangé.
/// Un document sans chemin sous-pixel est rendu tel quel.
pub fn resolve(svg: String) -> String {
    const TAG: &str = "<polyline ";
    if !svg.contains(TAG) {
        return svg;
    }
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg.as_str();
    while let Some(start) = rest.find(TAG) {
        let (before, from_tag) = rest.split_at(start);
        out.push_str(before);
        let end = from_tag.find('>').map_or(from_tag.len(), |at| at + 1);
        let (tag, after) = from_tag.split_at(end);
        out.push_str(&rewrite(tag).unwrap_or_else(|| tag.to_owned()));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// La valeur de l'attribut `name="…"` de `tag`, avec sa plage dans `tag`.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<(&'a str, std::ops::Range<usize>)> {
    let key = format!(" {name}=\"");
    let from = tag.find(&key)? + key.len();
    let len = tag[from..].find('"')?;
    Some((&tag[from..from + len], from..from + len))
}

fn rewrite(tag: &str) -> Option<String> {
    let (width, width_range) = attribute(tag, "stroke-width")?;
    let (real, fixed) = split_width(width.parse().ok()?);
    if !fixed {
        return None;
    }
    let (points, points_range) = attribute(tag, "points")?;
    let decoded: Option<Vec<String>> = points
        .split_whitespace()
        .map(|pair| {
            let (x, y) = pair.split_once(',')?;
            let scale = f64::from(SCALE);
            Some(format!(
                "{},{}",
                x.parse::<f64>().ok()? / scale,
                y.parse::<f64>().ok()? / scale
            ))
        })
        .collect();
    let points = decoded?.join(" ");
    // Les deux plages se suivent dans l'ordre d'écriture de `plotters-svg`
    // (épaisseur avant sommets), mais on ne s'y fie pas.
    let mut edits = [(width_range, real.to_string()), (points_range, points)];
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut tag = tag.to_owned();
    for (range, text) in edits {
        tag.replace_range(range, &text);
    }
    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_render::subpixel::stroke_width;

    #[test]
    fn une_polyline_sous_pixel_retrouve_ses_pixels_decimaux_et_son_epaisseur() {
        let flagged = stroke_width(3);
        let svg = format!(
            "<svg><polyline fill=\"none\" stroke=\"#000\" stroke-width=\"{flagged}\" points=\"16,32 24,40 \"/></svg>"
        );
        let done = resolve(svg);
        assert!(done.contains("stroke-width=\"3\""), "{done}");
        assert!(done.contains("points=\"1,2 1.5,2.5\""), "{done}");
    }

    #[test]
    fn une_polyline_ordinaire_et_le_reste_du_document_sont_intacts() {
        let svg = "<svg><polyline stroke-width=\"2\" points=\"1,2 3,4 \"/><rect/></svg>".to_owned();
        assert_eq!(resolve(svg.clone()), svg);
    }
}
