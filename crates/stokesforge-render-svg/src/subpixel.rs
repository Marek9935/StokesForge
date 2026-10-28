//! The export's subpixel paths (issue #91).
//!
//! `plotters-svg` writes only integers. A subpixel path
//! ([`stokesforge_render::subpixel`]) is therefore handed to it in fixed point,
//! with a flag in `stroke-width`; once the document is finished, [`resolve`]
//! finds those `<polyline>`s again and rewrites width and vertices in
//! decimal pixels. Same reading of the convention as the screen backend: a
//! drawing feature never goes into a single backend.

use stokesforge_render::subpixel::{split_width, SCALE};

/// Rewrites the document's subpixel `<polyline>`s; the rest is unchanged.
/// A document without a subpixel path is returned as is.
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

/// The value of the `name="…"` attribute of `tag`, with its range in `tag`.
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
    // The two ranges follow each other in `plotters-svg`'s writing order
    // (width before vertices), but we do not rely on it.
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
    fn a_subpixel_polyline_gets_back_its_decimal_pixels_and_its_width() {
        let flagged = stroke_width(3);
        let svg = format!(
            "<svg><polyline fill=\"none\" stroke=\"#000\" stroke-width=\"{flagged}\" points=\"16,32 24,40 \"/></svg>"
        );
        let done = resolve(svg);
        assert!(done.contains("stroke-width=\"3\""), "{done}");
        assert!(done.contains("points=\"1,2 1.5,2.5\""), "{done}");
    }

    #[test]
    fn an_ordinary_polyline_and_the_rest_of_the_document_are_intact() {
        let svg = "<svg><polyline stroke-width=\"2\" points=\"1,2 3,4 \"/><rect/></svg>".to_owned();
        assert_eq!(resolve(svg.clone()), svg);
    }
}
