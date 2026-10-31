//! Hand-written conversions between the transport structures
//! (`schema.rs`) and the in-memory model (`stokesforge_core`). The format is not
//! derived from the model (see the crate doc): these functions are the only
//! place where the two worlds meet.

use stokesforge_core::column::MAX_COLUMN_ROWS;
use stokesforge_core::{
    Column, ColumnFormula, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError,
    FormulaPart, MixedCell, Node, NodeId, NodeKind, ValueBlock, WindowMeta,
};
use stokesforge_plot::{
    AnnotationSpec, AxisScale, AxisSpec, FreeFrame, LayerPlacement, LegendPosition, LegendSpec,
    LineSpec, LineStyle, PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec, ShapeKind,
    ShapeSpec, SymbolShape, SymbolSpec, TickDirection,
};

use stokesforge_import::recipe::ImportRecipe;

use crate::recipe_record::ImportSourceRecord;
use crate::schema::{
    AnnotationSpecRecord, AxisScaleRecord, AxisSpecRecord, FormulaPartRecord, FreeFrameRecord,
    GroupGradientRecord, GroupNameRecord, KindRecord, LayerPlacementRecord, LegendPositionRecord,
    LegendSpecRecord, LineSpecRecord, LineStyleRecord, NodeRecord, OverlaySpecRecord,
    PanelSpecRecord, PlotSpecRecord, PlotTypeRecord, RoleRecord, SavedColorsRecord,
    SavedGradientRecord, SeriesAxisRecord, SeriesSpecRecord, ShapeKindRecord, ShapeSpecRecord,
    SymbolShapeRecord, SymbolSpecRecord, TextAlignRecord, TextAnchorRecord, TextFontRecord,
    TickDirectionRecord, TypeRecord, ValuesRecord, WindowGeometryRecord, WindowModeRecord,
    WindowViewRecord, MIXED_TAG_EMPTY, MIXED_TAG_NUMBER, MIXED_TAG_TEXT,
};

/// Upper bound of a panel coordinate (`row`/`column`) read from a file
/// (B2, `guide-de-correction-robustesse.md` §F3/F4): a screen does not hold
/// 10,000 rows or columns of subgraphs. Beyond that, `record.row as usize`
/// does not panic by itself, but the overflow moves further on, into
/// `PlotSpec::rows()`/`columns()` (`max + 1`).
pub(crate) const MAX_PANEL_POSITION: u64 = 10_000;

/// Upper bound of `histogram_bins` read from a file (B4, same section of the
/// guide) — the same as the one `stokesforge_plot::BinCount` now imposes
/// at the type level (`stokesforge_plot::MAX_HISTOGRAM_BINS`): a single figure to
/// change if it ever had to move. `ConvertError::ZeroHistogramBins`
/// already bounds the low end; this one bounds the high end, before the renderer asks for
/// the allocation that would `abort()` the process (tens of
/// gigabytes, `edges` and `counts` together).
pub(crate) use stokesforge_plot::MAX_HISTOGRAM_BINS;

/// Schema → model decoding error that is not the core's business: an
/// inconsistent float buffer length, or a column whose
/// decoded type and values do not agree (should never
/// happen for a file written by this crate — sign of a corrupted
/// file).
#[derive(Debug)]
pub(crate) enum ConvertError {
    /// The `Float` buffer of a column has a length that is not a multiple of 8
    /// (an `f64` is 8 bytes).
    BadFloatLength(usize),
    /// A tag of `ValuesRecord::Mixed` is neither `0` (empty), nor `1`
    /// (number), nor `2` (text): nothing then says in which of the two
    /// value arrays to look for the following rows.
    BadMixedTag(u8),
    /// The three arrays of `ValuesRecord::Mixed` do not agree: the
    /// tags announce more (or fewer) numbers or texts than the
    /// arrays carry.
    BadMixedLength {
        /// What the tags announce.
        expected: usize,
        /// What the corresponding array carries.
        found: usize,
        /// `"numbers"` or `"texts"`, for the message.
        what: &'static str,
    },
    /// A column of the file announces more rows than
    /// `stokesforge_core::column::MAX_COLUMN_ROWS` (#316): refused before
    /// any allocation, so that a forged file does not reserve
    /// gigabytes.
    TooManyRows {
        /// Number of rows announced by the file.
        found: usize,
        /// Accepted cap.
        limit: usize,
    },
    /// `Column::from_block` refused the rebuilt (type, values) pair.
    Core(CoreError),
    /// `histogram_bins` of a series is 0 in the file. `stokesforge-plot`
    /// expresses this field as `NonZeroU32` (decision R2: zero bins has no
    /// interesting repair, better to refuse to build it) — never
    /// happens for a file written by this crate, sign of a
    /// corrupted file or one written by a third-party tool.
    ZeroHistogramBins,
    /// `histogram_bins` exceeds [`MAX_HISTOGRAM_BINS`] (B4): same spirit as
    /// `ZeroHistogramBins`, the other end of the interval.
    HistogramBinsTooLarge(u32),
    /// `row`/`column` of a panel exceeds [`MAX_PANEL_POSITION`] (B2).
    PanelPositionOutOfBounds {
        /// Row read from the file, out of bounds.
        row: u64,
        /// Column read from the file, out of bounds.
        column: u64,
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

impl From<TypeRecord> for ColumnType {
    fn from(value: TypeRecord) -> Self {
        match value {
            TypeRecord::Numeric => ColumnType::Numeric,
            TypeRecord::DateTime => ColumnType::DateTime,
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
            ColumnRole::X3 => RoleRecord::X3,
            ColumnRole::Y3 => RoleRecord::Y3,
            ColumnRole::X4 => RoleRecord::X4,
            ColumnRole::Y4 => RoleRecord::Y4,
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
            RoleRecord::X3 => ColumnRole::X3,
            RoleRecord::Y3 => ColumnRole::Y3,
            RoleRecord::X4 => ColumnRole::X4,
            RoleRecord::Y4 => ColumnRole::Y4,
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
            // The in-memory model already carries the raw name and content
            // (C5, level 2): making them go through again identically
            // is what makes two successive saves byte-identical
            // (B20) even when a series has an unknown plot type.
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

impl From<stokesforge_plot::TextAlign> for TextAlignRecord {
    fn from(value: stokesforge_plot::TextAlign) -> Self {
        match value {
            stokesforge_plot::TextAlign::Left => Self::Left,
            stokesforge_plot::TextAlign::Center => Self::Center,
            stokesforge_plot::TextAlign::Right => Self::Right,
        }
    }
}

impl From<TextAlignRecord> for stokesforge_plot::TextAlign {
    fn from(value: TextAlignRecord) -> Self {
        match value {
            TextAlignRecord::Left => Self::Left,
            TextAlignRecord::Center => Self::Center,
            TextAlignRecord::Right => Self::Right,
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
        width: font.width,
        align: font.align.into(),
    }
}

fn record_to_font(record: TextFontRecord) -> stokesforge_plot::TextFont {
    stokesforge_plot::TextFont {
        family: record.family,
        size: record.size,
        width: record.width,
        align: record.align.into(),
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
            ticks: value.ticks,
            minor_ticks: value.minor_ticks,
            minor_tick_count: value.minor_tick_count,
            label_every: value.label_every,
            tick_length: value.tick_length,
            minor_tick_length: value.minor_tick_length,
            tick_width: value.tick_width,
            tick_mark_color: value.tick_mark_color.clone(),
            tick_direction: match value.tick_direction {
                TickDirection::Out => TickDirectionRecord::Out,
                TickDirection::In => TickDirectionRecord::In,
                TickDirection::Both => TickDirectionRecord::Both,
            },
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
            ticks: value.ticks,
            minor_ticks: value.minor_ticks,
            minor_tick_count: value.minor_tick_count,
            label_every: value.label_every,
            tick_length: value.tick_length,
            minor_tick_length: value.minor_tick_length,
            tick_width: value.tick_width,
            tick_mark_color: value.tick_mark_color,
            tick_direction: match value.tick_direction {
                TickDirectionRecord::Out => TickDirection::Out,
                TickDirectionRecord::In => TickDirection::In,
                TickDirectionRecord::Both => TickDirection::Both,
            },
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

/// `SeriesSpec` → record. Infallible: the in-memory model is
/// always representable (same pattern as `kind_to_record`).
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

/// Record → `SeriesSpec`. `histogram_bins` must be non-zero
/// ([`ConvertError::ZeroHistogramBins`]) and not exceed
/// [`MAX_HISTOGRAM_BINS`] ([`ConvertError::HistogramBinsTooLarge`], B4).
fn record_to_series_spec(record: SeriesSpecRecord) -> Result<SeriesSpec, ConvertError> {
    // Zero and "too large" are two distinct errors even though
    // `BinCount::new` returns `None` for both: it is what the
    // file lacks (nothing vs. an absurd value), and both deserve a different
    // message.
    if record.histogram_bins == 0 {
        return Err(ConvertError::ZeroHistogramBins);
    }
    if record.histogram_bins > MAX_HISTOGRAM_BINS {
        return Err(ConvertError::HistogramBinsTooLarge(record.histogram_bins));
    }
    // Both bounds have just been checked above: `BinCount::new`
    // cannot return `None` here.
    #[allow(clippy::expect_used)]
    let histogram_bins = stokesforge_plot::BinCount::new(record.histogram_bins)
        .expect("bounds already checked above");
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

/// `TextAnchor` → record. Infallible.
fn anchor_to_record(anchor: stokesforge_plot::TextAnchor) -> TextAnchorRecord {
    TextAnchorRecord {
        x: anchor.x,
        y: anchor.y,
        axis: anchor.axis.into(),
    }
}

/// Record → `TextAnchor`. Infallible: a non-finite anchor is
/// kept as is, it is the renderer that ignores it (`TextAnchor::is_finite`).
fn record_to_anchor(record: TextAnchorRecord) -> stokesforge_plot::TextAnchor {
    stokesforge_plot::TextAnchor {
        x: record.x,
        y: record.y,
        axis: record.axis.into(),
    }
}

/// `PanelSpec` → record. Infallible.
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
            layer: overlay.layer,
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
                width: annotation.width,
                align: annotation.align.into(),
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
        group_gradients: panel
            .group_gradients
            .iter()
            .map(|(group, stops)| GroupGradientRecord {
                group: *group,
                stops: stops.clone(),
            })
            .collect(),
        free_frame: panel.free_frame.map(|frame| FreeFrameRecord {
            left: frame.left,
            top: frame.top,
            width: frame.width,
            height: frame.height,
        }),
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

/// Record → `PanelSpec`. Propagates the failure of the first faulty
/// series, if there is one, and refuses a position that exceeds
/// [`MAX_PANEL_POSITION`] (B2) before the cast to `usize` — an out-of-bounds
/// `u64` does not panic here, but makes `PlotSpec::rows()`/
/// `columns()` overflow further on (`max + 1`).
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
            layer: overlay.layer,
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
                width: annotation.width,
                align: annotation.align.into(),
            })
            .collect(),
        shapes: record.shapes.into_iter().map(record_to_shape).collect(),
        group_names: record
            .group_names
            .into_iter()
            .filter(|entry| !entry.name.is_empty())
            .map(|entry| (entry.group, entry.name))
            .collect(),
        group_gradients: record
            .group_gradients
            .into_iter()
            .filter(|entry| !entry.stops.is_empty())
            .map(|entry| (entry.group, entry.stops))
            .collect(),
        free_frame: record.free_frame.map(|frame| FreeFrame {
            left: frame.left,
            top: frame.top,
            width: frame.width,
            height: frame.height,
        }),
    })
}

/// Saved colors → record. Infallible.
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

/// Record → saved colors. Infallible: an unreadable string
/// stays as it is (the renderer replaces it with a fallback
/// color), a list that is too long is bounded by `Tree::with_saved_colors`.
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

/// `PlotSpec` → record. Infallible.
pub(crate) fn plot_spec_to_record(spec: &PlotSpec) -> PlotSpecRecord {
    PlotSpecRecord {
        title: spec.title.clone(),
        panels: spec.panels.iter().map(panel_spec_to_record).collect(),
        background: spec.background.clone(),
        title_color: spec.title_color.clone(),
        title_visible: spec.title_visible,
        title_anchor: spec.title_anchor.map(anchor_to_record),
        title_font: font_to_record(&spec.title_font),
        stale_warning: spec.stale_warning,
        decimal_comma: spec.decimal_comma,
        frame_widths: spec.frame_widths.clone(),
        frame_heights: spec.frame_heights.clone(),
        fixed_figure: spec.fixed_figure,
        figure_left: spec.figure_left,
        figure_top: spec.figure_top,
        layer_placement: match spec.layer_placement {
            LayerPlacement::Grid => LayerPlacementRecord::Grid,
            LayerPlacement::Free => LayerPlacementRecord::Free,
        },
    }
}

/// Record → `PlotSpec`. An empty `panels` is not a failure: it is the
/// same case as `Panels::new(Vec::new())` on the `stokesforge-plot` side, which supplies a
/// default panel rather than refusing — the `.into()` conversion on
/// `Vec<PanelSpec>` already does it.
pub(crate) fn record_to_plot_spec(record: PlotSpecRecord) -> Result<PlotSpec, ConvertError> {
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
        stale_warning: record.stale_warning,
        decimal_comma: record.decimal_comma,
        frame_widths: record.frame_widths,
        frame_heights: record.frame_heights,
        fixed_figure: record.fixed_figure,
        figure_left: record.figure_left,
        figure_top: record.figure_top,
        layer_placement: match record.layer_placement {
            LayerPlacementRecord::Grid => LayerPlacement::Grid,
            LayerPlacementRecord::Free => LayerPlacement::Free,
        },
    })
}

/// Encodes an `f64` buffer into raw little-endian bytes, 8 bytes per
/// value, `NaN` preserved bit for bit (`to_le_bytes` keeps the exact
/// bit pattern, `NaN`s included).
fn encode_floats(values: &[f64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Decodes a raw little-endian byte buffer into `f64`. `Err` if the
/// length is not a multiple of 8.
/// Refuses a column of more than `limit` rows, before any allocation.
fn check_rows(len: usize, limit: usize) -> Result<(), ConvertError> {
    if len > limit {
        return Err(ConvertError::TooManyRows { found: len, limit });
    }
    Ok(())
}

fn decode_floats(bytes: &[u8], limit: usize) -> Result<Vec<f64>, ConvertError> {
    check_rows(bytes.len() / 8, limit)?;
    if !bytes.len().is_multiple_of(8) {
        return Err(ConvertError::BadFloatLength(bytes.len()));
    }
    let (chunks, _remainder) = bytes.as_chunks::<8>();
    Ok(chunks.iter().copied().map(f64::from_le_bytes).collect())
}

/// Splits a mixed-type buffer into the three parallel arrays of
/// [`ValuesRecord::Mixed`]: one tag per row, then only the values
/// that exist. Infallible — the in-memory model is always
/// representable.
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

/// Rebuilds a mixed-type buffer from the three parallel arrays.
///
/// `Err` on any inconsistency between them — unknown tag, or an array that
/// does not carry exactly what the tags announce. Same policy
/// as [`decode_floats`]: a file written by this crate never produces
/// these cases, and there is no sensible repair to attempt (slipping by one
/// row would shift all the following ones, silently).
fn decode_mixed(
    tags: &[u8],
    numbers: &[u8],
    texts: Vec<String>,
    limit: usize,
) -> Result<Vec<MixedCell>, ConvertError> {
    check_rows(tags.len(), limit)?;
    let numbers = decode_floats(numbers, limit)?;
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
            // The two `unwrap_or` do not happen: the counts
            // above have just been checked exact. Defensive guard
            // rather than an `expect` (invariant 10).
            MIXED_TAG_NUMBER => Ok(numbers.next().map_or(MixedCell::Empty, MixedCell::Number)),
            MIXED_TAG_TEXT => Ok(texts.next().map_or(MixedCell::Empty, MixedCell::Text)),
            other => Err(ConvertError::BadMixedTag(other)),
        })
        .collect()
}

/// Kind of a node → transport record. Infallible: the in-memory
/// model is always representable.
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
                formula_parts: meta.formula.as_ref().map(|formula| {
                    formula
                        .parts()
                        .iter()
                        .map(|part| match part {
                            FormulaPart::Text(text) => FormulaPartRecord::Text(text.clone()),
                            FormulaPart::Column(id) => FormulaPartRecord::Column(id.raw()),
                        })
                        .collect()
                }),
                file_metadata: meta
                    .file_metadata
                    .iter()
                    .map(|line| (line.key.clone(), line.value.clone()))
                    .collect(),
                x_prime: meta.x_prime,
                // An unreadable recipe (column crafted by hand) is written
                // without a source: the save never stops on that.
                import_source: meta.import_source.as_ref().and_then(|source| {
                    let recipe = ImportRecipe::from_json(&source.recipe_json).ok()?;
                    Some(Box::new(ImportSourceRecord {
                        path: source.path.clone(),
                        recipe: (&recipe).into(),
                        column: source.column as u64,
                        block: source.block as u64,
                    }))
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
        // The model already carries the raw name and content (C5, level 2):
        // making them go through identically is what makes two
        // successive saves byte-identical (B20) even for a node
        // of an unknown kind.
        NodeKind::Unknown(raw) => KindRecord::Unknown(raw.clone()),
    }
}

/// Transport record → kind of a node.
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
            formula_parts,
            file_metadata,
            import_source,
            x_prime,
        } => {
            let column_type: ColumnType = column_type.into();
            let block = match values {
                ValuesRecord::Float(bytes) => {
                    ValueBlock::Float(decode_floats(&bytes, MAX_COLUMN_ROWS)?)
                }
                ValuesRecord::Mixed {
                    tags,
                    numbers,
                    texts,
                } => ValueBlock::Mixed(decode_mixed(&tags, &numbers, texts, MAX_COLUMN_ROWS)?),
            };
            let meta = ColumnMeta {
                role: role.into(),
                unit,
                long_name,
                comment,
                display_format,
                // The identifiers read by the formula have already passed
                // `check_node_id` (`build_project_reported`). A column that
                // is no longer in the sheet degrades at calculation (`#REF`),
                // never into a panic (invariant 10).
                formula: formula_parts.map(|parts| {
                    ColumnFormula::from_parts(
                        parts
                            .into_iter()
                            .map(|part| match part {
                                FormulaPartRecord::Text(text) => FormulaPart::Text(text),
                                FormulaPartRecord::Column(id) => {
                                    FormulaPart::Column(NodeId::from_raw(id))
                                }
                            })
                            .collect(),
                    )
                }),
                // A file can carry any float: only a finite
                // value is an abscissa.
                x_prime: x_prime.filter(|value| value.is_finite()),
                file_metadata: file_metadata
                    .into_iter()
                    .map(|(key, value)| stokesforge_core::MetaLine { key, value })
                    .collect(),
                import_source: import_source.map(|source| {
                    let source = *source;
                    stokesforge_core::ImportSource {
                        path: source.path,
                        recipe_json: ImportRecipe::from(source.recipe).to_json(),
                        column: usize::try_from(source.column).unwrap_or(usize::MAX),
                        block: usize::try_from(source.block).unwrap_or(usize::MAX),
                    }
                }),
            };
            // `from_block_with_version`, not `from_block`: the counter must
            // go through the disk, otherwise it lies after a reload.
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

/// Node → transport record. Infallible.
pub(crate) fn node_to_record(node: &Node) -> NodeRecord {
    NodeRecord {
        id: node.id().raw(),
        name: node.name().to_string(),
        parent: node.parent().map(NodeId::raw),
        kind: kind_to_record(node.kind()),
    }
}

impl From<crate::WindowMode> for WindowModeRecord {
    fn from(mode: crate::WindowMode) -> Self {
        match mode {
            crate::WindowMode::Open => WindowModeRecord::Open,
            crate::WindowMode::Maximized => WindowModeRecord::Maximized,
            crate::WindowMode::Closed => WindowModeRecord::Closed,
        }
    }
}

impl From<WindowModeRecord> for crate::WindowMode {
    fn from(mode: WindowModeRecord) -> Self {
        match mode {
            WindowModeRecord::Open => crate::WindowMode::Open,
            WindowModeRecord::Maximized => crate::WindowMode::Maximized,
            WindowModeRecord::Closed => crate::WindowMode::Closed,
        }
    }
}

impl From<crate::WindowGeometry> for WindowGeometryRecord {
    fn from(g: crate::WindowGeometry) -> Self {
        WindowGeometryRecord {
            x: g.x,
            y: g.y,
            width: g.width,
            height: g.height,
            view: g.view.map(|v| WindowViewRecord {
                mode: v.mode.into(),
                collapsed: v.collapsed,
                stack: v.stack,
            }),
        }
    }
}

impl From<WindowGeometryRecord> for crate::WindowGeometry {
    fn from(g: WindowGeometryRecord) -> Self {
        crate::WindowGeometry {
            x: g.x,
            y: g.y,
            width: g.width,
            height: g.height,
            view: g.view.map(|v| crate::WindowView {
                mode: v.mode.into(),
                collapsed: v.collapsed,
                stack: v.stack,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_rows_refuses_only_beyond_the_limit() {
        assert!(check_rows(3, 3).is_ok());
        assert!(matches!(
            check_rows(4, 3),
            Err(ConvertError::TooManyRows { found: 4, limit: 3 })
        ));
    }

    #[test]
    fn column_decoders_refuse_beyond_the_row_limit() {
        assert!(decode_floats(&[0u8; 24], 3).is_ok());
        assert!(matches!(
            decode_floats(&[0u8; 32], 3),
            Err(ConvertError::TooManyRows { .. })
        ));
        assert!(decode_mixed(&[0, 0, 0], &[], vec![], 3).is_ok());
        assert!(matches!(
            decode_mixed(&[0, 0, 0, 0], &[], vec![], 3),
            Err(ConvertError::TooManyRows { .. })
        ));
    }

    /// #398: a Free graph round-trips unsanitized: negative left, a hostile
    /// size kept as written, an overlay without frame. Witness: a field
    /// dropped by one direction of the conversion.
    #[test]
    fn a_free_graph_round_trips_as_written() {
        let base = PanelSpec {
            free_frame: Some(FreeFrame {
                left: -75.5,
                top: 12.0,
                width: 1e300,
                height: 200.0,
            }),
            ..PanelSpec::at(0, 0)
        };
        let overlay = PanelSpec::overlay_on(&base, stokesforge_plot::OverlaySpec::new(true, true));
        let spec = PlotSpec {
            layer_placement: LayerPlacement::Free,
            ..PlotSpec::empty("free").with_panels(vec![base, overlay])
        };
        let back = record_to_plot_spec(plot_spec_to_record(&spec)).unwrap();
        assert_eq!(back, spec);
        assert_eq!(back.layer_placement, LayerPlacement::Free);
        assert_eq!(back.panels[1].free_frame, None);
    }

    /// A layer with two groups of two series, the first named "Café".
    fn layer_with_two_groups() -> PanelSpec {
        let mut panel = PanelSpec::default();
        for pair in [[1, 2], [3, 4]] {
            panel.push_series_batch(
                pair.iter()
                    .map(|n| SeriesSpec::new(NodeId::from_raw(*n)))
                    .collect(),
            );
        }
        assert!(panel.rename_group(1, "Café"));
        panel
    }

    fn groups(panel: &PanelSpec) -> Vec<Option<u32>> {
        panel.series.iter().map(|s| s.group).collect()
    }

    /// #209: an empty group name read from the file is dropped (the group
    /// keeps its automatic name); witness: the neighbouring non-empty name stays.
    #[test]
    fn an_empty_group_name_is_dropped_at_load() {
        let mut record = panel_spec_to_record(&layer_with_two_groups());
        record.group_names.push(GroupNameRecord {
            group: 2,
            name: String::new(),
        });
        let panel = record_to_panel_spec(record).unwrap();
        assert_eq!(
            panel.group_names,
            [(1, "Café".to_string())].into(),
            "only the empty name disappears"
        );
        assert_eq!(groups(&panel), [Some(1), Some(1), Some(2), Some(2)]);
    }

    /// #209: a file written before group names (#197), without the field
    /// `group_names`, reads back with its groups and without a name; witness: the same
    /// record with the field keeps the name.
    #[test]
    fn a_layer_without_group_names_reads_back_with_its_groups() {
        let bytes = rmp_serde::to_vec_named(&panel_spec_to_record(&layer_with_two_groups()))
            .expect("encoding");
        let reloaded: PanelSpecRecord = rmp_serde::from_slice(&bytes).expect("decoding");
        let panel = record_to_panel_spec(reloaded).unwrap();
        assert_eq!(panel.group_names, [(1, "Café".to_string())].into());

        let mut value = rmpv::decode::read_value(&mut std::io::Cursor::new(bytes)).unwrap();
        let rmpv::Value::Map(fields) = &mut value else {
            panic!("a layer is written as a map");
        };
        let before = fields.len();
        fields.retain(|(key, _)| key.as_str() != Some("group_names"));
        assert_eq!(fields.len(), before - 1, "the field existed");
        let mut old = Vec::new();
        rmpv::encode::write_value(&mut old, &value).unwrap();
        let reloaded: PanelSpecRecord =
            rmp_serde::from_slice(&old).expect("an old layer reads back");
        let panel = record_to_panel_spec(reloaded).unwrap();
        assert!(panel.group_names.is_empty());
        assert_eq!(groups(&panel), [Some(1), Some(1), Some(2), Some(2)]);
    }
}
