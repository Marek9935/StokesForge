//! Les aides de test communes aux surfaces de `src/app/` (#128).
//!
//! Chaque surface écrivait ses propres copies de ces aides (jusqu'à quatre
//! pour `painted`, trois pour `contexte`) ; un correctif devait être refait
//! partout. Elles vivent ici une fois, et les modules `tests` les importent
//! par nom (`use super::super::test_support::{…}`, jamais `*` : voir
//! `aucun_import_global_hors_des_tests`, qui tolère les tests mais pas le
//! flou).
//!
//! Ce qui reste local à un module, volontairement : les bancs (`banc`), qui
//! montent chacun un état différent ; `grid::tests::Banc::image` et
//! `workbook::tests::cliquer`, qui pilotent un banc ou une fermeture de
//! peinture plutôt qu'un contexte nu ; et `menus::tests::contexte`, qui
//! ajoute les chargeurs d'images et AccessKit à la base d'ici.

use eframe::egui;
use stokesforge_core::{Column, NodeId, Project};

/// Un contexte aux polices et au thème de l'application : la largeur des
/// onglets, des étiquettes et des menus en dépend.
pub(super) fn contexte() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(stokesforge_render_egui::font_definitions());
    crate::theme::install(&context);
    context
}

/// L'entrée brute d'une image de `taille` avec `events`.
fn entree(taille: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, taille)),
        events,
        ..Default::default()
    }
}

/// Une image de `taille` avec `events`, où `contenu` dessine ; rend la sortie
/// complète, textures écartées. Le temps n'avance que de 1/60 s par image :
/// voir [`image`] pour les surfaces dont les animations doivent aller au bout.
pub(super) fn image_de(
    context: &egui::Context,
    taille: egui::Vec2,
    events: Vec<egui::Event>,
    contenu: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = context.run_ui(entree(taille, events), contenu);
    output.textures_delta.clear();
    output
}

/// Une image de 1400 × 900 avec `events`, 0,1 s après la précédente ; rend ce
/// qui a été peint. Le temps avance pour que les défilements animés aillent
/// au bout (la bande d'onglets montre l'onglet choisi par `scroll_to_rect`,
/// étalé sur 0,3 s au plus).
pub(super) fn image(
    context: &egui::Context,
    events: Vec<egui::Event>,
    contenu: impl FnMut(&mut egui::Ui),
) -> Vec<egui::epaint::ClippedShape> {
    let mut input = entree(egui::vec2(1400.0, 900.0), events);
    input.time = Some(context.input(|input| input.time) + 0.1);
    let mut output = context.run_ui(input, contenu);
    output.textures_delta.clear();
    output.shapes
}

/// Une touche pressée, sans modificateur.
pub(super) fn touche(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Un clic de `bouton` en `pos`, réparti sur trois images (survol, pression,
/// relâchement) comme le veut la détection de clic d'egui.
pub(super) fn cliquer(
    context: &egui::Context,
    pos: egui::Pos2,
    bouton: egui::PointerButton,
    mut contenu: impl FnMut(&mut egui::Ui),
) {
    image(context, vec![egui::Event::PointerMoved(pos)], &mut contenu);
    for pressed in [true, false] {
        image(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: bouton,
                pressed,
                modifiers: egui::Modifiers::default(),
            }],
            &mut contenu,
        );
    }
}

/// Un texte contenant `needle` a-t-il été peint ?
pub(super) fn painted(output: &egui::FullOutput, needle: &str) -> bool {
    output.shapes.iter().any(|painted| {
        matches!(&painted.shape,
            egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
    })
}

/// Un texte peint : son contenu, sa couleur effective et son centre.
#[derive(Debug)]
pub(super) struct Texte {
    pub(super) texte: String,
    pub(super) couleur: egui::Color32,
    pub(super) centre: egui::Pos2,
    /// Écrit en gras (`theme::bold`) : la graisse est une variation de police,
    /// pas une couleur.
    pub(super) gras: bool,
}

/// Tous les textes peints dans `shapes`, avec leur couleur effective.
pub(super) fn textes(shapes: &[egui::epaint::ClippedShape]) -> Vec<Texte> {
    shapes
        .iter()
        .filter_map(|peint| match &peint.shape {
            egui::Shape::Text(shape) => {
                let section = shape
                    .galley
                    .job
                    .sections
                    .first()
                    .map_or(egui::Color32::PLACEHOLDER, |s| s.format.color);
                let couleur =
                    shape
                        .override_text_color
                        .unwrap_or(if section == egui::Color32::PLACEHOLDER {
                            shape.fallback_color
                        } else {
                            section
                        });
                Some(Texte {
                    texte: shape.galley.job.text.clone(),
                    couleur,
                    centre: shape.pos + shape.galley.rect.center().to_vec2(),
                    gras: shape
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|s| s.format.coords == crate::theme::bold_variations()),
                })
            }
            _ => None,
        })
        .collect()
}

/// Le texte peint exactement égal à `cherche`, s'il y en a un seul.
pub(super) fn texte_exact<'a>(textes: &'a [Texte], cherche: &str) -> &'a Texte {
    let trouves: Vec<&Texte> = textes.iter().filter(|t| t.texte == cherche).collect();
    assert_eq!(
        trouves.len(),
        1,
        "{cherche:?} doit être peint une fois : {:?}",
        textes.iter().map(|t| &t.texte).collect::<Vec<_>>()
    );
    trouves[0]
}

/// Les libellés d'une liste rendue par `libelle`, tous distincts et non
/// vides : deux choix d'une même liste déroulante qui s'affichent pareil ne
/// se distinguent plus.
pub(super) fn distincts<T: std::fmt::Debug>(choix: &[T], libelle: impl Fn(&T) -> String) {
    let libelles: Vec<String> = choix.iter().map(&libelle).collect();
    for (i, a) in libelles.iter().enumerate() {
        assert!(!a.trim().is_empty(), "{:?} sans libellé", choix[i]);
        for (j, b) in libelles.iter().enumerate().skip(i + 1) {
            assert_ne!(a, b, "{:?} et {:?} s'affichent pareil", choix[i], choix[j]);
        }
    }
}

/// Un classeur à une feuille de deux colonnes `[0, 1]` sous la racine : rend
/// les nœuds de ces colonnes (x, y), de quoi tracer une courbe.
pub(super) fn colonnes_xy(project: &mut Project) -> (NodeId, NodeId) {
    let root = project.root();
    let book = project.add_empty_workbook(root, None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let x = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    let y = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    (x, y)
}
