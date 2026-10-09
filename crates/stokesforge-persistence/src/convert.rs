//! Conversions écrites à la main entre les structures de transport
//! (`schema.rs`) et le modèle en mémoire (`stokesforge_core`). Le format ne se
//! dérive pas du modèle (voir la doc de crate) : ces fonctions sont le seul
//! endroit où les deux mondes se rencontrent.

use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError, MixedCell, Node, NodeId,
    NodeKind, ValueBlock, WindowMeta,
};
use stokesforge_plot::{
    AnnotationSpec, AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle,
    PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec, ShapeKind, ShapeSpec, SymbolShape,
    SymbolSpec, SPEC_FORMAT_VERSION,
};

use crate::schema::{
    AnnotationSpecRecord, AxisScaleRecord, AxisSpecRecord, GroupNameRecord, KindRecord,
    LegendPositionRecord, LegendSpecRecord, LineSpecRecord, LineStyleRecord, NodeRecord,
    OverlaySpecRecord, PanelSpecRecord, PlotSpecRecord, PlotTypeRecord, RoleRecord,
    SavedColorsRecord, SavedGradientRecord, SeriesAxisRecord, SeriesSpecRecord, ShapeKindRecord,
    ShapeSpecRecord, SymbolShapeRecord, SymbolSpecRecord, TextAnchorRecord, TextFontRecord,
    TypeRecord, ValuesRecord, MIXED_TAG_EMPTY, MIXED_TAG_NUMBER, MIXED_TAG_TEXT,
};

/// Borne haute d'une coordonnée (`row`/`column`) de panneau lue d'un fichier
/// (B2, `guide-de-correction-robustesse.md` §F3/F4) : un écran ne fait pas
/// 10 000 lignes ou colonnes de sous-graphes. Au-delà, `record.row as usize`
/// ne panique pas lui-même, mais le débordement se déplace plus loin, dans
/// `PlotSpec::rows()`/`columns()` (`max + 1`).
pub(crate) const MAX_PANEL_POSITION: u64 = 10_000;

/// Borne haute de `histogram_bins` lue d'un fichier (B4, même section du
/// guide) — la même que celle que `stokesforge_plot::BinCount` impose désormais
/// au niveau du type (`stokesforge_plot::MAX_HISTOGRAM_BINS`) : un seul chiffre à
/// changer si jamais il devait bouger. `ConvertError::ZeroHistogramBins`
/// borne déjà le bas ; ceci borne le haut, avant que le rendu ne demande
/// l'allocation qui ferait `abort()` le processus (des dizaines de
/// gigaoctets, `edges` et `counts` confondus).
pub(crate) use stokesforge_plot::MAX_HISTOGRAM_BINS;

/// Erreur de décodage schéma → modèle qui n'est pas du ressort du cœur : une
/// longueur de buffer flottant incohérente, ou une colonne dont le
/// type et les valeurs décodées ne s'accordent pas (ne devrait jamais
/// survenir pour un fichier écrit par ce crate — signe d'un fichier
/// corrompu).
#[derive(Debug)]
pub(crate) enum ConvertError {
    /// Le buffer `Float` d'une colonne n'a pas une longueur multiple de 8
    /// (un `f64` fait 8 octets).
    BadFloatLength(usize),
    /// Une étiquette de `ValuesRecord::Mixed` n'est ni `0` (vide), ni `1`
    /// (nombre), ni `2` (texte) : rien ne dit alors dans lequel des deux
    /// tableaux de valeurs chercher les lignes suivantes.
    BadMixedTag(u8),
    /// Les trois tableaux de `ValuesRecord::Mixed` ne s'accordent pas : les
    /// étiquettes annoncent plus (ou moins) de nombres ou de textes que les
    /// tableaux n'en portent.
    BadMixedLength {
        /// Ce que les étiquettes annoncent.
        expected: usize,
        /// Ce que le tableau correspondant porte.
        found: usize,
        /// `"numbers"` ou `"texts"`, pour le message.
        what: &'static str,
    },
    /// `Column::from_block` a refusé la paire (type, valeurs) reconstruite.
    Core(CoreError),
    /// `histogram_bins` d'une série vaut 0 dans le fichier. `stokesforge-plot`
    /// exprime ce champ en `NonZeroU32` (décision R2 : zéro classe n'a pas de
    /// réparation intéressante, autant refuser de le construire) — ne
    /// survient jamais pour un fichier écrit par ce crate, signe d'un
    /// fichier corrompu ou écrit par un outil tiers.
    ZeroHistogramBins,
    /// `histogram_bins` dépasse [`MAX_HISTOGRAM_BINS`] (B4) : même esprit que
    /// `ZeroHistogramBins`, l'autre bout de l'intervalle.
    HistogramBinsTooLarge(u32),
    /// `row`/`column` d'un panneau dépasse [`MAX_PANEL_POSITION`] (B2).
    PanelPositionOutOfBounds {
        /// Ligne lue du fichier, hors bornes.
        row: u64,
        /// Colonne lue du fichier, hors bornes.
        column: u64,
    },
    /// Le `format_version` d'un `PlotSpec` lu du fichier dépasse
    /// [`SPEC_FORMAT_VERSION`], celui que ce lecteur connaît (C1) : accepter
    /// en silence exposerait un graphe dont ce lecteur ne peut pas garantir
    /// avoir compris toutes les variantes qu'il porte.
    UnsupportedSpecVersion {
        /// Version trouvée dans le fichier.
        found: u32,
        /// Version la plus élevée que ce lecteur sait interpréter.
        supported: u32,
    },
}

impl From<ColumnType> for TypeRecord {
    fn from(value: ColumnType) -> Self {
        match value {
            ColumnType::Numeric => TypeRecord::Numeric,
            ColumnType::DateTime => TypeRecord::DateTime,
            ColumnType::Mixed => TypeRecord::Mixed,
        }
    }
}

/// `TypeRecord::Text` — l'ancienne étiquette d'un type de colonne texte
/// homogène, retiré du modèle en mémoire au profit de `Mixed` — se relit en
/// [`ColumnType::Mixed`] : un fichier écrit avant ce changement reste
/// lisible, voir la doc de tête de [`record_to_kind`] pour la migration des
/// **valeurs** qui l'accompagne nécessairement (`ValuesRecord::Text` n'a pas
/// de pendant direct en `ColumnValues`).
impl From<TypeRecord> for ColumnType {
    fn from(value: TypeRecord) -> Self {
        match value {
            TypeRecord::Numeric => ColumnType::Numeric,
            TypeRecord::DateTime => ColumnType::DateTime,
            TypeRecord::Text => ColumnType::Mixed,
            TypeRecord::Mixed => ColumnType::Mixed,
        }
    }
}

impl From<ColumnRole> for RoleRecord {
    fn from(value: ColumnRole) -> Self {
        match value {
            ColumnRole::X => RoleRecord::X,
            ColumnRole::Y => RoleRecord::Y,
            ColumnRole::X2 => RoleRecord::X2,
            ColumnRole::Y2 => RoleRecord::Y2,
            ColumnRole::Z => RoleRecord::Z,
            ColumnRole::XError => RoleRecord::XError,
            ColumnRole::YError => RoleRecord::YError,
            ColumnRole::Label => RoleRecord::Label,
            ColumnRole::None => RoleRecord::None,
        }
    }
}

impl From<RoleRecord> for ColumnRole {
    fn from(value: RoleRecord) -> Self {
        match value {
            RoleRecord::X => ColumnRole::X,
            RoleRecord::Y => ColumnRole::Y,
            RoleRecord::X2 => ColumnRole::X2,
            RoleRecord::Y2 => ColumnRole::Y2,
            RoleRecord::Z => ColumnRole::Z,
            RoleRecord::XError => ColumnRole::XError,
            RoleRecord::YError => ColumnRole::YError,
            RoleRecord::Label => ColumnRole::Label,
            RoleRecord::None => ColumnRole::None,
        }
    }
}

// -- PlotSpec (phase R3) ------------------------------------------------

impl From<AxisScale> for AxisScaleRecord {
    fn from(value: AxisScale) -> Self {
        match value {
            AxisScale::Linear => AxisScaleRecord::Linear,
            AxisScale::Log => AxisScaleRecord::Log,
        }
    }
}

impl From<AxisScaleRecord> for AxisScale {
    fn from(value: AxisScaleRecord) -> Self {
        match value {
            AxisScaleRecord::Linear => AxisScale::Linear,
            AxisScaleRecord::Log => AxisScale::Log,
        }
    }
}

impl From<&PlotType> for PlotTypeRecord {
    fn from(value: &PlotType) -> Self {
        match value {
            PlotType::Scatter => PlotTypeRecord::Scatter,
            PlotType::Line => PlotTypeRecord::Line,
            PlotType::LineScatter => PlotTypeRecord::LineScatter,
            PlotType::Bar => PlotTypeRecord::Bar,
            PlotType::Histogram => PlotTypeRecord::Histogram,
            PlotType::ErrorBar => PlotTypeRecord::ErrorBar,
            // Le modèle en mémoire porte déjà le nom et le contenu bruts
            // (C5, niveau 2) : les faire traverser de nouveau à l'identique
            // est ce qui rend deux sauvegardes successives byte-identiques
            // (B20) même quand une série a un type de tracé inconnu.
            PlotType::Unknown(raw) => PlotTypeRecord::Unknown(raw.clone()),
        }
    }
}

impl From<PlotTypeRecord> for PlotType {
    fn from(value: PlotTypeRecord) -> Self {
        match value {
            PlotTypeRecord::Scatter => PlotType::Scatter,
            PlotTypeRecord::Line => PlotType::Line,
            PlotTypeRecord::LineScatter => PlotType::LineScatter,
            PlotTypeRecord::Bar => PlotType::Bar,
            PlotTypeRecord::Histogram => PlotType::Histogram,
            PlotTypeRecord::ErrorBar => PlotType::ErrorBar,
            PlotTypeRecord::Unknown(raw) => PlotType::Unknown(raw),
        }
    }
}

impl From<SeriesAxis> for SeriesAxisRecord {
    fn from(value: SeriesAxis) -> Self {
        match value {
            SeriesAxis::Left => SeriesAxisRecord::Left,
            SeriesAxis::Right => SeriesAxisRecord::Right,
        }
    }
}

impl From<SeriesAxisRecord> for SeriesAxis {
    fn from(value: SeriesAxisRecord) -> Self {
        match value {
            SeriesAxisRecord::Left => SeriesAxis::Left,
            SeriesAxisRecord::Right => SeriesAxis::Right,
        }
    }
}

impl From<LineStyle> for LineStyleRecord {
    fn from(value: LineStyle) -> Self {
        match value {
            LineStyle::None => LineStyleRecord::None,
            LineStyle::Solid => LineStyleRecord::Solid,
            LineStyle::Dash => LineStyleRecord::Dash,
            LineStyle::Dot => LineStyleRecord::Dot,
            LineStyle::DashDot => LineStyleRecord::DashDot,
        }
    }
}

impl From<LineStyleRecord> for LineStyle {
    fn from(value: LineStyleRecord) -> Self {
        match value {
            LineStyleRecord::None => LineStyle::None,
            LineStyleRecord::Solid => LineStyle::Solid,
            LineStyleRecord::Dash => LineStyle::Dash,
            LineStyleRecord::Dot => LineStyle::Dot,
            LineStyleRecord::DashDot => LineStyle::DashDot,
        }
    }
}

impl From<SymbolShape> for SymbolShapeRecord {
    fn from(value: SymbolShape) -> Self {
        match value {
            SymbolShape::None => SymbolShapeRecord::None,
            SymbolShape::Circle => SymbolShapeRecord::Circle,
            SymbolShape::Square => SymbolShapeRecord::Square,
            SymbolShape::Triangle => SymbolShapeRecord::Triangle,
            SymbolShape::Diamond => SymbolShapeRecord::Diamond,
            SymbolShape::Plus => SymbolShapeRecord::Plus,
            SymbolShape::Cross => SymbolShapeRecord::Cross,
        }
    }
}

impl From<SymbolShapeRecord> for SymbolShape {
    fn from(value: SymbolShapeRecord) -> Self {
        match value {
            SymbolShapeRecord::None => SymbolShape::None,
            SymbolShapeRecord::Circle => SymbolShape::Circle,
            SymbolShapeRecord::Square => SymbolShape::Square,
            SymbolShapeRecord::Triangle => SymbolShape::Triangle,
            SymbolShapeRecord::Diamond => SymbolShape::Diamond,
            SymbolShapeRecord::Plus => SymbolShape::Plus,
            SymbolShapeRecord::Cross => SymbolShape::Cross,
        }
    }
}

impl From<LegendPosition> for LegendPositionRecord {
    fn from(value: LegendPosition) -> Self {
        match value {
            LegendPosition::TopLeft => LegendPositionRecord::TopLeft,
            LegendPosition::TopRight => LegendPositionRecord::TopRight,
            LegendPosition::BottomLeft => LegendPositionRecord::BottomLeft,
            LegendPosition::BottomRight => LegendPositionRecord::BottomRight,
        }
    }
}

impl From<LegendPositionRecord> for LegendPosition {
    fn from(value: LegendPositionRecord) -> Self {
        match value {
            LegendPositionRecord::TopLeft => LegendPosition::TopLeft,
            LegendPositionRecord::TopRight => LegendPosition::TopRight,
            LegendPositionRecord::BottomLeft => LegendPosition::BottomLeft,
            LegendPositionRecord::BottomRight => LegendPosition::BottomRight,
        }
    }
}

impl From<&LineSpec> for LineSpecRecord {
    fn from(value: &LineSpec) -> Self {
        Self {
            color: value.color.clone(),
            style: value.style.into(),
            width: value.width,
        }
    }
}

impl From<LineSpecRecord> for LineSpec {
    fn from(value: LineSpecRecord) -> Self {
        Self {
            color: value.color,
            style: value.style.into(),
            width: value.width,
        }
    }
}

impl From<&SymbolSpec> for SymbolSpecRecord {
    fn from(value: &SymbolSpec) -> Self {
        Self {
            shape: value.shape.into(),
            size: value.size,
            color: value.color.clone(),
            edge_color: value.edge_color.clone(),
        }
    }
}

impl From<SymbolSpecRecord> for SymbolSpec {
    fn from(value: SymbolSpecRecord) -> Self {
        Self {
            shape: value.shape.into(),
            size: value.size,
            color: value.color,
            edge_color: value.edge_color,
        }
    }
}

fn font_to_record(font: &stokesforge_plot::TextFont) -> TextFontRecord {
    TextFontRecord {
        family: font.family.clone(),
        size: font.size,
    }
}

fn record_to_font(record: TextFontRecord) -> stokesforge_plot::TextFont {
    stokesforge_plot::TextFont {
        family: record.family,
        size: record.size,
    }
}

impl From<&AxisSpec> for AxisSpecRecord {
    fn from(value: &AxisSpec) -> Self {
        Self {
            label: value.label.clone(),
            unit: value.unit.clone(),
            scale: value.scale.into(),
            auto_range: value.auto_range,
            range_min: value.range_min,
            range_max: value.range_max,
            grid: value.grid,
            visible: value.visible,
            inverted: value.inverted,
            title_color: value.title_color.clone(),
            title_visible: value.title_visible,
            title_anchor: value.title_anchor.map(anchor_to_record),
            title_font: font_to_record(&value.title_font),
            tick_color: value.tick_color.clone(),
            mirror: value.mirror,
            tick_labels: value.tick_labels,
        }
    }
}

impl From<AxisSpecRecord> for AxisSpec {
    fn from(value: AxisSpecRecord) -> Self {
        Self {
            label: value.label,
            unit: value.unit,
            scale: value.scale.into(),
            auto_range: value.auto_range,
            range_min: value.range_min,
            range_max: value.range_max,
            grid: value.grid,
            visible: value.visible,
            inverted: value.inverted,
            title_color: value.title_color,
            title_visible: value.title_visible,
            title_anchor: value.title_anchor.map(record_to_anchor),
            title_font: record_to_font(value.title_font),
            tick_color: value.tick_color,
            mirror: value.mirror,
            tick_labels: value.tick_labels,
        }
    }
}

impl From<&LegendSpec> for LegendSpecRecord {
    fn from(value: &LegendSpec) -> Self {
        Self {
            visible: value.visible,
            position: value.position.into(),
            anchor: value.anchor.map(anchor_to_record),
            font: font_to_record(&value.font),
            color: value.color.clone(),
        }
    }
}

impl From<LegendSpecRecord> for LegendSpec {
    fn from(value: LegendSpecRecord) -> Self {
        Self {
            visible: value.visible,
            position: value.position.into(),
            anchor: value.anchor.map(record_to_anchor),
            font: record_to_font(value.font),
            color: value.color,
        }
    }
}

/// `SeriesSpec` → enregistrement. Infaillible : le modèle en mémoire est
/// toujours représentable (même patron que `kind_to_record`).
fn series_spec_to_record(series: &SeriesSpec) -> SeriesSpecRecord {
    SeriesSpecRecord {
        y: series.y.raw(),
        x: series.x.map(NodeId::raw),
        x_error: series.x_error.map(NodeId::raw),
        y_error: series.y_error.map(NodeId::raw),
        plot_type: (&series.plot_type).into(),
        name: series.name.clone(),
        line: (&series.line).into(),
        symbol: (&series.symbol).into(),
        axis: series.axis.into(),
        visible: series.visible,
        show_in_legend: series.show_in_legend,
        bar_width: series.bar_width,
        histogram_bins: series.histogram_bins.get(),
        row: series.row.map(|row| row as u64),
        group: series.group,
    }
}

/// Enregistrement → `SeriesSpec`. `histogram_bins` doit être non nul
/// ([`ConvertError::ZeroHistogramBins`]) et ne pas dépasser
/// [`MAX_HISTOGRAM_BINS`] ([`ConvertError::HistogramBinsTooLarge`], B4).
fn record_to_series_spec(record: SeriesSpecRecord) -> Result<SeriesSpec, ConvertError> {
    // Zéro et « trop grand » sont deux erreurs distinctes malgré
    // `BinCount::new` qui rend `None` pour les deux : c'est ce qui manque au
    // fichier (rien vs. une valeur absurde), et les deux méritent un message
    // différent.
    if record.histogram_bins == 0 {
        return Err(ConvertError::ZeroHistogramBins);
    }
    if record.histogram_bins > MAX_HISTOGRAM_BINS {
        return Err(ConvertError::HistogramBinsTooLarge(record.histogram_bins));
    }
    // Les deux bornes viennent d'être vérifiées ci-dessus : `BinCount::new`
    // ne peut pas rendre `None` ici.
    #[allow(clippy::expect_used)]
    let histogram_bins = stokesforge_plot::BinCount::new(record.histogram_bins)
        .expect("bornes déjà vérifiées ci-dessus");
    Ok(SeriesSpec {
        y: NodeId::from_raw(record.y),
        x: record.x.map(NodeId::from_raw),
        x_error: record.x_error.map(NodeId::from_raw),
        y_error: record.y_error.map(NodeId::from_raw),
        plot_type: record.plot_type.into(),
        name: record.name,
        line: record.line.into(),
        symbol: record.symbol.into(),
        axis: record.axis.into(),
        visible: record.visible,
        show_in_legend: record.show_in_legend,
        bar_width: record.bar_width,
        histogram_bins,
        row: record.row.and_then(|row| usize::try_from(row).ok()),
        group: record.group,
    })
}

/// `TextAnchor` → enregistrement. Infaillible.
fn anchor_to_record(anchor: stokesforge_plot::TextAnchor) -> TextAnchorRecord {
    TextAnchorRecord {
        x: anchor.x,
        y: anchor.y,
        axis: anchor.axis.into(),
    }
}

/// Enregistrement → `TextAnchor`. Infaillible : une ancre non finie est
/// gardée telle quelle, c'est le rendu qui l'ignore (`TextAnchor::is_finite`).
fn record_to_anchor(record: TextAnchorRecord) -> stokesforge_plot::TextAnchor {
    stokesforge_plot::TextAnchor {
        x: record.x,
        y: record.y,
        axis: record.axis.into(),
    }
}

/// `PanelSpec` → enregistrement. Infaillible.
fn panel_spec_to_record(panel: &PanelSpec) -> PanelSpecRecord {
    PanelSpecRecord {
        title: panel.title.clone(),
        title_visible: panel.title_visible,
        title_anchor: panel.title_anchor.map(anchor_to_record),
        title_font: font_to_record(&panel.title_font),
        title_color: panel.title_color.clone(),
        series: panel.series.iter().map(series_spec_to_record).collect(),
        bottom: (&panel.bottom).into(),
        left: (&panel.left).into(),
        right: (&panel.right).into(),
        top: (&panel.top).into(),
        legend: (&panel.legend).into(),
        overlay: panel.overlay.map(|overlay| OverlaySpecRecord {
            x_top: overlay.x_top,
            y_right: overlay.y_right,
        }),
        row: panel.row as u64,
        column: panel.column as u64,
        annotations: panel
            .annotations
            .iter()
            .map(|annotation| AnnotationSpecRecord {
                text: annotation.text.clone(),
                visible: annotation.visible,
                x: annotation.x,
                y: annotation.y,
                axis: annotation.axis.into(),
                color: annotation.color.clone(),
                size: annotation.size,
                rotation: annotation.rotation,
                font: annotation.font.clone(),
            })
            .collect(),
        shapes: panel.shapes.iter().map(shape_to_record).collect(),
        group_names: panel
            .group_names
            .iter()
            .map(|(group, name)| GroupNameRecord {
                group: *group,
                name: name.clone(),
            })
            .collect(),
    }
}

fn shape_to_record(shape: &ShapeSpec) -> ShapeSpecRecord {
    ShapeSpecRecord {
        kind: match shape.kind {
            ShapeKind::Rectangle => ShapeKindRecord::Rectangle,
            ShapeKind::Ellipse => ShapeKindRecord::Ellipse,
            ShapeKind::Line => ShapeKindRecord::Line,
            ShapeKind::Arrow => ShapeKindRecord::Arrow,
        },
        x1: shape.x1,
        y1: shape.y1,
        x2: shape.x2,
        y2: shape.y2,
        axis: shape.axis.into(),
        line_color: shape.line_color.clone(),
        line_width: shape.line_width,
        fill_color: shape.fill_color.clone(),
        visible: shape.visible,
    }
}

fn record_to_shape(record: ShapeSpecRecord) -> ShapeSpec {
    ShapeSpec {
        kind: match record.kind {
            ShapeKindRecord::Rectangle => ShapeKind::Rectangle,
            ShapeKindRecord::Ellipse => ShapeKind::Ellipse,
            ShapeKindRecord::Line => ShapeKind::Line,
            ShapeKindRecord::Arrow => ShapeKind::Arrow,
        },
        x1: record.x1,
        y1: record.y1,
        x2: record.x2,
        y2: record.y2,
        axis: record.axis.into(),
        line_color: record.line_color,
        line_width: record.line_width,
        fill_color: record.fill_color,
        visible: record.visible,
    }
}

/// Enregistrement → `PanelSpec`. Propage l'échec de la première série
/// fautive, s'il y en a une, et refuse une position qui dépasse
/// [`MAX_PANEL_POSITION`] (B2) avant le cast vers `usize` — un `u64` hors
/// bornes ne panique pas ici, mais fait déborder `PlotSpec::rows()`/
/// `columns()` plus loin (`max + 1`).
fn record_to_panel_spec(record: PanelSpecRecord) -> Result<PanelSpec, ConvertError> {
    if record.row > MAX_PANEL_POSITION || record.column > MAX_PANEL_POSITION {
        return Err(ConvertError::PanelPositionOutOfBounds {
            row: record.row,
            column: record.column,
        });
    }
    let series = record
        .series
        .into_iter()
        .map(record_to_series_spec)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PanelSpec {
        title: record.title,
        title_visible: record.title_visible,
        title_anchor: record.title_anchor.map(record_to_anchor),
        title_font: record_to_font(record.title_font),
        title_color: record.title_color,
        series,
        bottom: record.bottom.into(),
        left: record.left.into(),
        right: record.right.into(),
        top: record.top.into(),
        legend: record.legend.into(),
        overlay: record.overlay.map(|overlay| stokesforge_plot::OverlaySpec {
            x_top: overlay.x_top,
            y_right: overlay.y_right,
        }),
        row: record.row as usize,
        column: record.column as usize,
        annotations: record
            .annotations
            .into_iter()
            .map(|annotation| AnnotationSpec {
                text: annotation.text,
                visible: annotation.visible,
                x: annotation.x,
                y: annotation.y,
                axis: annotation.axis.into(),
                color: annotation.color,
                size: annotation.size,
                rotation: annotation.rotation,
                font: annotation.font,
            })
            .collect(),
        shapes: record.shapes.into_iter().map(record_to_shape).collect(),
        group_names: record
            .group_names
            .into_iter()
            .filter(|entry| !entry.name.is_empty())
            .map(|entry| (entry.group, entry.name))
            .collect(),
    })
}

/// Couleurs enregistrées → enregistrement. Infaillible.
pub(crate) fn saved_colors_to_record(colors: &stokesforge_plot::SavedColors) -> SavedColorsRecord {
    SavedColorsRecord {
        curves: colors.curves.clone(),
        text: colors.text.clone(),
        backgrounds: colors.backgrounds.clone(),
        gradients: colors
            .gradients
            .iter()
            .map(|gradient| SavedGradientRecord {
                name: gradient.name.clone(),
                stops: gradient.stops.clone(),
            })
            .collect(),
    }
}

/// Enregistrement → couleurs enregistrées. Infaillible : une chaîne
/// illisible reste telle quelle (le rendu la remplace par une couleur de
/// repli), une liste trop longue est bornée par `Tree::with_saved_colors`.
pub(crate) fn record_to_saved_colors(record: SavedColorsRecord) -> stokesforge_plot::SavedColors {
    stokesforge_plot::SavedColors {
        curves: record.curves,
        text: record.text,
        backgrounds: record.backgrounds,
        gradients: record
            .gradients
            .into_iter()
            .map(|gradient| stokesforge_plot::SavedGradient {
                name: gradient.name,
                stops: gradient.stops,
            })
            .collect(),
    }
}

/// `PlotSpec` → enregistrement. Infaillible.
pub(crate) fn plot_spec_to_record(spec: &PlotSpec) -> PlotSpecRecord {
    PlotSpecRecord {
        title: spec.title.clone(),
        panels: spec.panels.iter().map(panel_spec_to_record).collect(),
        background: spec.background.clone(),
        title_color: spec.title_color.clone(),
        title_visible: spec.title_visible,
        title_anchor: spec.title_anchor.map(anchor_to_record),
        title_font: font_to_record(&spec.title_font),
        format_version: spec.format_version,
    }
}

/// Enregistrement → `PlotSpec`. `panels` vide n'est pas un échec : c'est le
/// même cas que `Panels::new(Vec::new())` côté `stokesforge-plot`, qui fournit un
/// panneau par défaut plutôt que de refuser — la conversion `.into()` sur
/// `Vec<PanelSpec>` le fait déjà.
///
/// `format_version` **est** vérifié (C1) : un `PlotSpec` plus récent que
/// [`SPEC_FORMAT_VERSION`] est refusé plutôt qu'accepté en silence — sans
/// quoi une variante que ce lecteur ne connaît pas pourrait se cacher
/// n'importe où dans l'arbre qui suit. C'est délibérément une simple
/// comparaison de champ, pas la dégradation gracieuse d'une variante
/// inconnue (C5, niveau 2, hors scope ici) : « un champ n'oblige pas à
/// bouger le numéro de version, une variante l'oblige » — voir le guide,
/// piège F3.
pub(crate) fn record_to_plot_spec(record: PlotSpecRecord) -> Result<PlotSpec, ConvertError> {
    if record.format_version > SPEC_FORMAT_VERSION {
        return Err(ConvertError::UnsupportedSpecVersion {
            found: record.format_version,
            supported: SPEC_FORMAT_VERSION,
        });
    }
    let panels = record
        .panels
        .into_iter()
        .map(record_to_panel_spec)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlotSpec {
        title: record.title,
        panels: panels.into(),
        background: record.background,
        title_color: record.title_color,
        title_visible: record.title_visible,
        title_anchor: record.title_anchor.map(record_to_anchor),
        title_font: record_to_font(record.title_font),
        format_version: record.format_version,
    })
}

/// Encode un buffer de `f64` en octets bruts little-endian, 8 octets par
/// valeur, `NaN` préservé bit à bit (`to_le_bytes` conserve le motif binaire
/// exact, y compris pour les `NaN`).
fn encode_floats(values: &[f64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Décode un buffer d'octets bruts little-endian en `f64`. `Err` si la
/// longueur n'est pas un multiple de 8.
fn decode_floats(bytes: &[u8]) -> Result<Vec<f64>, ConvertError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(ConvertError::BadFloatLength(bytes.len()));
    }
    let (chunks, _remainder) = bytes.as_chunks::<8>();
    Ok(chunks.iter().copied().map(f64::from_le_bytes).collect())
}

/// Éclate un buffer à types mixtes en les trois tableaux parallèles de
/// [`ValuesRecord::Mixed`] : une étiquette par ligne, puis les seules valeurs
/// qui existent. Infaillible — le modèle en mémoire est toujours
/// représentable.
fn encode_mixed(values: &[MixedCell]) -> ValuesRecord {
    let mut tags = Vec::with_capacity(values.len());
    let mut numbers = Vec::new();
    let mut texts = Vec::new();
    for cell in values {
        match cell {
            MixedCell::Empty => tags.push(MIXED_TAG_EMPTY),
            MixedCell::Number(value) => {
                tags.push(MIXED_TAG_NUMBER);
                numbers.extend_from_slice(&value.to_le_bytes());
            }
            MixedCell::Text(text) => {
                tags.push(MIXED_TAG_TEXT);
                texts.push(text.clone());
            }
        }
    }
    ValuesRecord::Mixed {
        tags,
        numbers,
        texts,
    }
}

/// Recompose un buffer à types mixtes depuis les trois tableaux parallèles.
///
/// `Err` sur toute incohérence entre eux — étiquette inconnue, ou tableau qui
/// ne porte pas exactement ce que les étiquettes annoncent. Même politique
/// que [`decode_floats`] : un fichier écrit par ce crate ne produit jamais
/// ces cas, et il n'y a pas de réparation sensée à tenter (glisser d'une
/// ligne décalerait toutes les suivantes, en silence).
fn decode_mixed(
    tags: &[u8],
    numbers: &[u8],
    texts: Vec<String>,
) -> Result<Vec<MixedCell>, ConvertError> {
    let numbers = decode_floats(numbers)?;
    let expected_numbers = tags.iter().filter(|&&tag| tag == MIXED_TAG_NUMBER).count();
    if expected_numbers != numbers.len() {
        return Err(ConvertError::BadMixedLength {
            expected: expected_numbers,
            found: numbers.len(),
            what: "numbers",
        });
    }
    let expected_texts = tags.iter().filter(|&&tag| tag == MIXED_TAG_TEXT).count();
    if expected_texts != texts.len() {
        return Err(ConvertError::BadMixedLength {
            expected: expected_texts,
            found: texts.len(),
            what: "texts",
        });
    }
    let mut numbers = numbers.into_iter();
    let mut texts = texts.into_iter();
    tags.iter()
        .map(|&tag| match tag {
            MIXED_TAG_EMPTY => Ok(MixedCell::Empty),
            // Les deux `unwrap_or` ne se produisent pas : les comptes
            // ci-dessus viennent d'être vérifiés exacts. Garde défensive
            // plutôt qu'un `expect` (invariant 10).
            MIXED_TAG_NUMBER => Ok(numbers.next().map_or(MixedCell::Empty, MixedCell::Number)),
            MIXED_TAG_TEXT => Ok(texts.next().map_or(MixedCell::Empty, MixedCell::Text)),
            other => Err(ConvertError::BadMixedTag(other)),
        })
        .collect()
}

/// Nature d'un nœud → enregistrement de transport. Infaillible : le modèle
/// en mémoire est toujours représentable.
pub(crate) fn kind_to_record(kind: &NodeKind) -> KindRecord {
    match kind {
        NodeKind::Folder { children } => KindRecord::Folder {
            children: children.iter().map(|id| id.raw()).collect(),
        },
        NodeKind::Workbook { sheets, meta } => KindRecord::Workbook {
            sheets: sheets.iter().map(|id| id.raw()).collect(),
            hidden: meta.hidden,
            label: meta.label.clone(),
            comment: meta.comment.clone(),
        },
        NodeKind::Worksheet { columns } => KindRecord::Worksheet {
            columns: columns.iter().map(|id| id.raw()).collect(),
        },
        NodeKind::Column(column) => {
            let meta = column.meta();
            let values = match column.values() {
                ColumnValues::Float(values) => ValuesRecord::Float(encode_floats(values)),
                ColumnValues::Mixed(values) => encode_mixed(values),
            };
            KindRecord::Column {
                column_type: column.column_type().into(),
                role: meta.role.into(),
                unit: meta.unit.clone(),
                long_name: meta.long_name.clone(),
                comment: meta.comment.clone(),
                display_format: meta.display_format.clone(),
                values,
                version: column.version(),
                formula: meta.formula.clone(),
                file_metadata: meta
                    .file_metadata
                    .iter()
                    .map(|line| (line.key.clone(), line.value.clone()))
                    .collect(),
                x_prime: meta.x_prime,
                import_source: meta.import_source.as_ref().map(|source| {
                    (
                        source.path.clone(),
                        source.recipe_json.clone(),
                        source.column as u64,
                        source.block as u64,
                    )
                }),
            }
        }
        NodeKind::Graph { meta, spec } => KindRecord::Graph {
            hidden: meta.hidden,
            label: meta.label.clone(),
            comment: meta.comment.clone(),
            spec: plot_spec_to_record(spec),
        },
        NodeKind::Matrix { meta } => KindRecord::Matrix {
            hidden: meta.hidden,
            label: meta.label.clone(),
            comment: meta.comment.clone(),
        },
        NodeKind::Note { meta, text } => KindRecord::Note {
            hidden: meta.hidden,
            label: meta.label.clone(),
            comment: meta.comment.clone(),
            text: text.clone(),
        },
        // Le modèle porte déjà le nom et le contenu bruts (C5, niveau 2) :
        // les faire traverser à l'identique est ce qui rend deux
        // sauvegardes successives byte-identiques (B20) même pour un nœud
        // d'une nature inconnue.
        NodeKind::Unknown(raw) => KindRecord::Unknown(raw.clone()),
    }
}

/// Enregistrement de transport → nature d'un nœud.
pub(crate) fn record_to_kind(record: KindRecord) -> Result<NodeKind, ConvertError> {
    Ok(match record {
        KindRecord::Folder { children } => NodeKind::Folder {
            children: children.into_iter().map(NodeId::from_raw).collect(),
        },
        KindRecord::Workbook {
            sheets,
            hidden,
            label,
            comment,
        } => NodeKind::Workbook {
            sheets: sheets.into_iter().map(NodeId::from_raw).collect(),
            meta: WindowMeta {
                label,
                comment,
                hidden,
            },
        },
        KindRecord::Worksheet { columns } => NodeKind::Worksheet {
            columns: columns.into_iter().map(NodeId::from_raw).collect(),
        },
        KindRecord::Column {
            column_type,
            role,
            unit,
            long_name,
            comment,
            display_format,
            values,
            version,
            formula,
            file_metadata,
            import_source,
            x_prime,
        } => {
            let column_type: ColumnType = column_type.into();
            let block = match values {
                ValuesRecord::Float(bytes) => ValueBlock::Float(decode_floats(&bytes)?),
                // Migration de compatibilité : un fichier écrit avant le
                // retrait du type `Text` homogène porte encore cette
                // étiquette. `TypeRecord::Text` s'est déjà relu en
                // `ColumnType::Mixed` ci-dessus ; ici, chaque `Option<String>`
                // devient la cellule mixte équivalente **telle quelle**, sans
                // réanalyse numérique (`None → Empty`, `Some(s) → Text(s)`)
                // — une colonne texte d'hier est, cellule pour cellule, une
                // colonne mixte entièrement textuelle aujourd'hui.
                ValuesRecord::Text(values) => ValueBlock::Mixed(
                    values
                        .into_iter()
                        .map(|value| match value {
                            None => MixedCell::Empty,
                            Some(text) => MixedCell::Text(text),
                        })
                        .collect(),
                ),
                ValuesRecord::Mixed {
                    tags,
                    numbers,
                    texts,
                } => ValueBlock::Mixed(decode_mixed(&tags, &numbers, texts)?),
            };
            let meta = ColumnMeta {
                role: role.into(),
                unit,
                long_name,
                comment,
                display_format,
                formula,
                // Un fichier peut porter n'importe quel flottant : seule une
                // valeur finie est une abscisse.
                x_prime: x_prime.filter(|value| value.is_finite()),
                file_metadata: file_metadata
                    .into_iter()
                    .map(|(key, value)| stokesforge_core::MetaLine { key, value })
                    .collect(),
                import_source: import_source.map(|(path, recipe_json, column, block)| {
                    stokesforge_core::ImportSource {
                        path,
                        recipe_json,
                        column: usize::try_from(column).unwrap_or(usize::MAX),
                        block: usize::try_from(block).unwrap_or(usize::MAX),
                    }
                }),
            };
            // `from_block_with_version`, pas `from_block` : le compteur doit
            // traverser le disque, sinon il ment après un rechargement.
            let column = Column::from_block_with_version(column_type, block, meta, version)
                .map_err(ConvertError::Core)?;
            NodeKind::Column(column)
        }
        KindRecord::Graph {
            hidden,
            label,
            comment,
            spec,
        } => NodeKind::Graph {
            meta: WindowMeta {
                label,
                comment,
                hidden,
            },
            spec: record_to_plot_spec(spec)?,
        },
        KindRecord::Matrix {
            hidden,
            label,
            comment,
        } => NodeKind::Matrix {
            meta: WindowMeta {
                label,
                comment,
                hidden,
            },
        },
        KindRecord::Note {
            hidden,
            label,
            comment,
            text,
        } => NodeKind::Note {
            meta: WindowMeta {
                label,
                comment,
                hidden,
            },
            text,
        },
        KindRecord::Unknown(raw) => NodeKind::Unknown(raw),
    })
}

/// Nœud → enregistrement de transport. Infaillible.
pub(crate) fn node_to_record(node: &Node) -> NodeRecord {
    NodeRecord {
        id: node.id().raw(),
        name: node.name().to_string(),
        parent: node.parent().map(NodeId::raw),
        kind: kind_to_record(node.kind()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un calque à deux groupes de deux séries, le premier nommé « Été ».
    fn calque_a_deux_groupes() -> PanelSpec {
        let mut panel = PanelSpec::default();
        for pair in [[1, 2], [3, 4]] {
            panel.push_series_batch(
                pair.iter()
                    .map(|n| SeriesSpec::new(NodeId::from_raw(*n)))
                    .collect(),
            );
        }
        assert!(panel.rename_group(1, "Été"));
        panel
    }

    fn groupes(panel: &PanelSpec) -> Vec<Option<u32>> {
        panel.series.iter().map(|s| s.group).collect()
    }

    /// #209 : un nom de groupe vide lu dans le fichier est écarté (le groupe
    /// garde son nom automatique) ; témoin : le nom non vide voisin reste.
    #[test]
    fn un_nom_de_groupe_vide_est_ecarte_au_chargement() {
        let mut record = panel_spec_to_record(&calque_a_deux_groupes());
        record.group_names.push(GroupNameRecord {
            group: 2,
            name: String::new(),
        });
        let panel = record_to_panel_spec(record).unwrap();
        assert_eq!(
            panel.group_names,
            [(1, "Été".to_string())].into(),
            "seul le nom vide disparaît"
        );
        assert_eq!(groupes(&panel), [Some(1), Some(1), Some(2), Some(2)]);
    }

    /// #209 : un fichier écrit avant les noms de groupe (#197), sans le champ
    /// `group_names`, se relit avec ses groupes et sans nom ; témoin : le même
    /// enregistrement avec le champ garde le nom.
    #[test]
    fn un_calque_sans_noms_de_groupe_se_relit_avec_ses_groupes() {
        let octets = rmp_serde::to_vec_named(&panel_spec_to_record(&calque_a_deux_groupes()))
            .expect("encodage");
        let relu: PanelSpecRecord = rmp_serde::from_slice(&octets).expect("relecture");
        let panel = record_to_panel_spec(relu).unwrap();
        assert_eq!(panel.group_names, [(1, "Été".to_string())].into());

        let mut valeur = rmpv::decode::read_value(&mut std::io::Cursor::new(octets)).unwrap();
        let rmpv::Value::Map(champs) = &mut valeur else {
            panic!("un calque s'écrit en table");
        };
        let avant = champs.len();
        champs.retain(|(cle, _)| cle.as_str() != Some("group_names"));
        assert_eq!(champs.len(), avant - 1, "le champ existait");
        let mut ancien = Vec::new();
        rmpv::encode::write_value(&mut ancien, &valeur).unwrap();
        let relu: PanelSpecRecord =
            rmp_serde::from_slice(&ancien).expect("un ancien calque se relit");
        let panel = record_to_panel_spec(relu).unwrap();
        assert!(panel.group_names.is_empty());
        assert_eq!(groupes(&panel), [Some(1), Some(1), Some(2), Some(2)]);
    }
}
