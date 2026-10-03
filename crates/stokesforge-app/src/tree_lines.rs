//! Connecteurs d'arbre en L (├ └ │) partagés par l'Object Manager et le
//! Project Manager (#112).
//!
//! Le trait d'indentation d'egui (`indent_has_left_vline`) descend sur toute la
//! hauteur du corps d'un nœud, petits-enfants compris. Ici, chaque corps de
//! nœud rapporte le centre vertical de ses enfants directs ; le trait vertical
//! s'arrête au dernier d'entre eux et chaque enfant reçoit un court trait
//! horizontal. Le `│` qui traverse le sous-arbre d'un enfant n'existe que
//! parce que ce trait vertical couvre aussi les lignes qui suivent.
//!
//! Tout est peint au `Painter` (aucun glyphe), avec le trait de contour du thème.

use eframe::egui::{Pos2, Ui};

/// À appeler une fois à la racine d'un arbre : coupe la ligne d'indentation
/// d'egui, remplacée par [`tree_children`].
pub fn begin_tree(ui: &mut Ui) {
    ui.visuals_mut().indent_has_left_vline = false;
}

/// Centres verticaux des enfants directs d'un nœud, remplis par le corps.
#[derive(Default)]
pub struct TreeBranch {
    ys: Vec<f32>,
}

impl TreeBranch {
    /// Déclare un enfant direct dont la rangée a son centre en `y` (pour un
    /// nœud dépliable, le centre de son en-tête, pas de son corps).
    pub fn mark(&mut self, y: f32) {
        self.ys.push(y);
    }
}

/// Segments d'un niveau : le vertical de `top` au dernier enfant, puis un
/// horizontal de `x` à `stub_end` par enfant. Vide sans enfant.
pub fn connector_segments(top: f32, x: f32, stub_end: f32, ys: &[f32]) -> Vec<[Pos2; 2]> {
    let Some(last) = ys.iter().copied().reduce(f32::max) else {
        return Vec::new();
    };
    let mut segments = vec![[Pos2::new(x, top.min(last)), Pos2::new(x, last)]];
    segments.extend(
        ys.iter()
            .map(|&y| [Pos2::new(x, y), Pos2::new(stub_end.max(x), y)]),
    );
    segments
}

/// Corps d'un nœud : `add` dessine les enfants et déclare leur rangée par
/// [`TreeBranch::mark`] ; les connecteurs sont peints ensuite.
pub fn tree_children<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui, &mut TreeBranch) -> R) -> R {
    let cursor = ui.cursor();
    let line_x = ui
        .painter()
        .round_to_pixel_center(cursor.left() - ui.spacing().indent * 0.5);
    let top = cursor.top();
    let stub_end = cursor.left() - 2.0;
    let mut branch = TreeBranch::default();
    let result = add(ui, &mut branch);
    let painter = ui.painter();
    let ys: Vec<f32> = branch
        .ys
        .iter()
        .map(|&y| painter.round_to_pixel_center(y))
        .collect();
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    for [from, to] in connector_segments(top, line_x, stub_end, &ys) {
        painter.line_segment([from, to], stroke);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_enfant_aucun_trait() {
        assert!(connector_segments(0.0, 5.0, 12.0, &[]).is_empty());
    }

    #[test]
    fn le_vertical_s_arrete_au_dernier_enfant() {
        let segments = connector_segments(10.0, 5.0, 12.0, &[20.0, 40.0, 70.0]);
        assert_eq!(segments[0], [Pos2::new(5.0, 10.0), Pos2::new(5.0, 70.0)]);
        // Un trait horizontal par enfant, du vertical vers la rangée.
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[3], [Pos2::new(5.0, 70.0), Pos2::new(12.0, 70.0)]);
    }
}
