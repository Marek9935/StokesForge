//! Icônes de l'interface : le seul endroit qui connaît les fichiers SVG.
//!
//! Les icônes sont du Phosphor duotone, cuites par `scripts/icons-build.sh`
//! en **une variante par (icône, rôle de couleur)** — `Image::tint` multiplie
//! les pixels et ne sait pas recolorer séparément trait et fond. Règles
//! d'emploi et palette : `docs/icones.md`.
//!
//! Tout affichage d'icône passe par [`icon_image`] ou [`icon_button`] : taille
//! ([`ICON_SIZE`]) et alignement uniformes, info-bulle obligatoire (texte déjà
//! traduit par l'appelant, via `tr!`). Aucun autre module ne cite
//! `assets/icons` ni un `.svg` — verrouillé par le test
//! `aucune_icone_hors_du_composant`.
//!
//! Les SVG sont embarqués (`include_bytes!`) et rastérisés **à la demande** par
//! `egui_extras` (une fois par taille × `pixels_per_point`, puis gardés en
//! texture) : seules les icônes réellement affichées coûtent. Le thème de
//! l'application est clair et fixe (`theme.rs`) : les couleurs de rôle sont
//! donc constantes, sans variante sombre.

use eframe::egui::{self, Response, Ui, WidgetText};

/// Côté d'une icône, en points. Unique : une icône plus grosse ou plus petite
/// que ses voisines est un défaut, pas un choix local.
pub const ICON_SIZE: f32 = 16.0;

/// Ce que dit la couleur d'une icône. `Neutral` par défaut ; les autres
/// rôles ne servent que quand la couleur porte une information que la forme
/// ne porte pas (types d'objets, action destructive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconRole {
    Neutral,
    Workbook,
    Graph,
    Matrix,
    Note,
    Folder,
    Danger,
}

/// Icônes disponibles — un enum fermé : ajouter une icône, c'est ajouter une
/// variante, ses lignes de [`VARIANTS`] et son entrée dans `icons.tsv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Close,
    Detach,
    Reattach,
    Maximize,
    Restore,
    CaretDown,
    CaretRight,
    CaretLeft,
    CaretUp,
    Add,
    Delete,
    Folder,
    Workbook,
    Graph,
    Matrix,
    Note,
    Open,
    Save,
    SaveAs,
    Import,
    Export,
    Pointer,
    ZoomTool,
    TextTool,
    ResetZoom,
    Properties,
    AddColumn,
    AddSheet,
    PlotLine,
    PlotScatter,
    PlotBar,
    Bold,
    Italic,
    Subscript,
    Superscript,
    Formula,
    ShapeRectangle,
    ShapeEllipse,
    ShapeLine,
    ShapeArrow,
}

macro_rules! svg {
    ($file:literal) => {
        (
            concat!("bytes://plotyx-icons/", $file),
            include_bytes!(concat!("../assets/icons/", $file)) as &[u8],
        )
    };
}

/// Variantes cuites. La première ligne d'une icône est son repli quand le
/// rôle demandé n'existe pas pour elle.
/// URI egui et octets d'un SVG embarqué.
type Svg = (&'static str, &'static [u8]);

const VARIANTS: &[(Icon, IconRole, Svg)] = &[
    (Icon::Close, IconRole::Neutral, svg!("x.neutral.svg")),
    (Icon::Close, IconRole::Danger, svg!("x.danger.svg")),
    (
        Icon::Detach,
        IconRole::Neutral,
        svg!("arrow-square-out.neutral.svg"),
    ),
    (
        Icon::Reattach,
        IconRole::Neutral,
        svg!("arrow-square-in.neutral.svg"),
    ),
    (
        Icon::Maximize,
        IconRole::Neutral,
        svg!("corners-out.neutral.svg"),
    ),
    (
        Icon::Restore,
        IconRole::Neutral,
        svg!("corners-in.neutral.svg"),
    ),
    (
        Icon::CaretDown,
        IconRole::Neutral,
        svg!("caret-down.neutral.svg"),
    ),
    (
        Icon::CaretRight,
        IconRole::Neutral,
        svg!("caret-right.neutral.svg"),
    ),
    (
        Icon::CaretLeft,
        IconRole::Neutral,
        svg!("caret-left.neutral.svg"),
    ),
    (
        Icon::CaretUp,
        IconRole::Neutral,
        svg!("caret-up.neutral.svg"),
    ),
    (Icon::Add, IconRole::Neutral, svg!("plus.neutral.svg")),
    (Icon::Delete, IconRole::Danger, svg!("trash.danger.svg")),
    (
        Icon::Folder,
        IconRole::Folder,
        svg!("folder-simple.folder.svg"),
    ),
    (
        Icon::Workbook,
        IconRole::Workbook,
        svg!("table.workbook.svg"),
    ),
    (Icon::Graph, IconRole::Graph, svg!("chart-line.graph.svg")),
    (Icon::Matrix, IconRole::Matrix, svg!("grid-nine.matrix.svg")),
    (Icon::Note, IconRole::Note, svg!("note.note.svg")),
    (
        Icon::Open,
        IconRole::Neutral,
        svg!("folder-open.neutral.svg"),
    ),
    (
        Icon::Save,
        IconRole::Neutral,
        svg!("floppy-disk.neutral.svg"),
    ),
    (
        Icon::SaveAs,
        IconRole::Neutral,
        svg!("floppy-disk-back.neutral.svg"),
    ),
    (
        Icon::Import,
        IconRole::Neutral,
        svg!("download-simple.neutral.svg"),
    ),
    (Icon::Export, IconRole::Neutral, svg!("export.neutral.svg")),
    (Icon::Pointer, IconRole::Neutral, svg!("cursor.neutral.svg")),
    (
        Icon::ZoomTool,
        IconRole::Neutral,
        svg!("magnifying-glass-plus.neutral.svg"),
    ),
    (
        Icon::TextTool,
        IconRole::Neutral,
        svg!("text-t.neutral.svg"),
    ),
    (
        Icon::ShapeRectangle,
        IconRole::Neutral,
        svg!("rectangle.neutral.svg"),
    ),
    (
        Icon::ShapeEllipse,
        IconRole::Neutral,
        svg!("circle.neutral.svg"),
    ),
    (
        Icon::ShapeLine,
        IconRole::Neutral,
        svg!("line-segment.neutral.svg"),
    ),
    (
        Icon::ShapeArrow,
        IconRole::Neutral,
        svg!("arrow-up-right.neutral.svg"),
    ),
    (
        Icon::ResetZoom,
        IconRole::Neutral,
        svg!("arrow-counter-clockwise.neutral.svg"),
    ),
    (
        Icon::Properties,
        IconRole::Neutral,
        svg!("sliders-horizontal.neutral.svg"),
    ),
    (
        Icon::AddColumn,
        IconRole::Neutral,
        svg!("columns-plus-right.neutral.svg"),
    ),
    (
        Icon::AddSheet,
        IconRole::Neutral,
        svg!("rows-plus-bottom.neutral.svg"),
    ),
    (
        Icon::PlotLine,
        IconRole::Neutral,
        svg!("chart-line.neutral.svg"),
    ),
    (
        Icon::PlotScatter,
        IconRole::Neutral,
        svg!("chart-scatter.neutral.svg"),
    ),
    (
        Icon::PlotBar,
        IconRole::Neutral,
        svg!("chart-bar.neutral.svg"),
    ),
    (Icon::Bold, IconRole::Neutral, svg!("text-b.neutral.svg")),
    (
        Icon::Italic,
        IconRole::Neutral,
        svg!("text-italic.neutral.svg"),
    ),
    (
        Icon::Subscript,
        IconRole::Neutral,
        svg!("text-subscript.neutral.svg"),
    ),
    (
        Icon::Superscript,
        IconRole::Neutral,
        svg!("text-superscript.neutral.svg"),
    ),
    (
        Icon::Formula,
        IconRole::Neutral,
        svg!("function.neutral.svg"),
    ),
];

/// Inatteignable tant que chaque icône a une variante (test `le_tableau_…`).
const FALLBACK: Svg = svg!("x.neutral.svg");

fn variant(icon: Icon, role: IconRole) -> (&'static str, &'static [u8]) {
    let mut of_icon = VARIANTS.iter().filter(|(i, _, _)| *i == icon);
    of_icon
        .clone()
        .find(|(_, r, _)| *r == role)
        .or_else(|| of_icon.next())
        .map_or(FALLBACK, |(_, _, file)| *file)
}

/// L'icône comme image egui, à la taille standard.
pub fn icon_image(icon: Icon, role: IconRole) -> egui::Image<'static> {
    let (uri, bytes) = variant(icon, role);
    egui::Image::from_bytes(uri, bytes).fit_to_exact_size(egui::Vec2::splat(ICON_SIZE))
}

/// Bouton-icône sans cadre. `tooltip` est obligatoire : une icône seule ne
/// dit pas ce qu'elle fait (texte issu de `tr!`, fourni par l'appelant).
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    role: IconRole,
    tooltip: impl Into<WidgetText>,
) -> Response {
    ui.add(egui::Button::image(icon_image(icon, role)).frame(false))
        .on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn aucune_icone_hors_du_composant() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut fautifs = Vec::new();
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("icons.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (index, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if code.contains("assets/icons") || code.contains("plotyx-icons") {
                    fautifs.push(format!("{}:{}", path.display(), index + 1));
                }
            }
        }
        assert!(
            fautifs.is_empty(),
            "icône chargée hors de icons.rs — passer par icons::icon_image / \
             icons::icon_button : {fautifs:?}"
        );
    }

    /// `icons.tsv` (ce que le script produit) et `VARIANTS` (ce que le code
    /// emploie) disent la même chose, et chaque fichier cuit est un SVG.
    #[test]
    fn le_tableau_des_variantes_suit_icons_tsv() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
        let mut attendus = BTreeSet::new();
        for line in std::fs::read_to_string(dir.join("icons.tsv"))
            .unwrap()
            .lines()
        {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, roles) = line.split_once('\t').unwrap();
            for role in roles.split(',') {
                attendus.insert(format!("{name}.{role}.svg"));
            }
        }
        let employes: BTreeSet<String> = VARIANTS
            .iter()
            .map(|(_, _, (uri, _))| uri.rsplit('/').next().unwrap().to_owned())
            .collect();
        assert_eq!(employes, attendus);
        for (_, _, (uri, bytes)) in VARIANTS {
            assert!(
                std::str::from_utf8(bytes).unwrap().starts_with("<svg"),
                "{uri}"
            );
        }
    }

    /// Chaque variante se rastérise vraiment (chargeur SVG installé).
    #[test]
    fn chaque_icone_se_charge() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        for (icon, role, _) in VARIANTS {
            let image = icon_image(*icon, *role);
            let mut loaded = None;
            for _ in 0..50 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    loaded = Some(
                        image
                            .clone()
                            .load_for_size(ui.ctx(), egui::Vec2::splat(ICON_SIZE)),
                    );
                });
                output.textures_delta.clear();
                if matches!(loaded, Some(Ok(egui::load::TexturePoll::Ready { .. }))) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(
                matches!(loaded, Some(Ok(egui::load::TexturePoll::Ready { .. }))),
                "{icon:?}/{role:?} ne se charge pas : {:?}",
                loaded.as_ref().map(|r| r.as_ref().err())
            );
        }
    }
}
