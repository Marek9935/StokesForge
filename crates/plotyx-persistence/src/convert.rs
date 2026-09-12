//! Conversions écrites à la main entre les structures de transport
//! (`schema.rs`) et le modèle en mémoire (`plotyx_core`). Le format ne se
//! dérive pas du modèle (voir la doc de crate) : ces fonctions sont le seul
//! endroit où les deux mondes se rencontrent.


use plotyx_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError, Node, NodeId, NodeKind,
    ValueBlock, WindowMeta,
};
use plotyx_plot::{
    AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle, PanelSpec, PlotSpec,
    PlotType, SeriesAxis, SeriesSpec, SymbolShape, SymbolSpec, SPEC_FORMAT_VERSION,
};

use crate::schema::{
    AxisScaleRecord, AxisSpecRecord, KindRecord, LegendPositionRecord, LegendSpecRecord,
    LineSpecRecord, LineStyleRecord, NodeRecord, PanelSpecRecord, PlotSpecRecord, PlotTypeRecord,
    RoleRecord, SeriesAxisRecord, SeriesSpecRecord, SymbolShapeRecord, SymbolSpecRecord,
    TypeRecord, ValuesRecord,
};

/// Borne haute d'une coordonnée (`row`/`column`) de panneau lue d'un fichier
/// (B2, `guide-de-correction-robustesse.md` §F3/F4) : un écran ne fait pas
/// 10 000 lignes ou colonnes de sous-graphes. Au-delà, `record.row as usize`
/// ne panique pas lui-même, mais le débordement se déplace plus loin, dans
/// `PlotSpec::rows()`/`columns()` (`max + 1`).
pub(crate) const MAX_PANEL_POSITION: u64 = 10_000;

/// Borne haute de `histogram_bins` lue d'un fichier (B4, même section du
/// guide) — la même que celle que `plotyx_plot::BinCount` impose désormais
/// au niveau du type (`plotyx_plot::MAX_HISTOGRAM_BINS`) : un seul chiffre à
/// changer si jamais il devait bouger. `ConvertError::ZeroHistogramBins`
/// borne déjà le bas ; ceci borne le haut, avant que le rendu ne demande
/// l'allocation qui ferait `abort()` le processus (des dizaines de
/// gigaoctets, `edges` et `counts` confondus).
pub(crate) use plotyx_plot::MAX_HISTOGRAM_BINS;

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
    /// `Column::from_block` a refusé la paire (type, valeurs) reconstruite.
    Core(CoreError),
    /// `histogram_bins` d'une série vaut 0 dans le fichier. `plotyx-plot`
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
            ColumnType::Text => TypeRecord::Text,
        }
    }
}

impl From<TypeRecord> for ColumnType {
    fn from(value: TypeRecord) -> Self {
        match value {
            TypeRecord::Numeric => ColumnType::Numeric,
            TypeRecord::DateTime => ColumnType::DateTime,
            TypeRecord::Text => ColumnType::Text,
        }
    }
}

impl From<ColumnRole> for RoleRecord {
    fn from(value: ColumnRole) -> Self {
        match value {
            ColumnRole::X => RoleRecord::X,
            ColumnRole::Y => RoleRecord::Y,
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

impl From<PlotType> for PlotTypeRecord {
    fn from(value: PlotType) -> Self {
        match value {
            PlotType::Scatter => PlotTypeRecord::Scatter,
            PlotType::Line => PlotTypeRecord::Line,
            PlotType::LineScatter => PlotTypeRecord::LineScatter,
            PlotType::Bar => PlotTypeRecord::Bar,
            PlotType::Histogram => PlotTypeRecord::Histogram,
            PlotType::ErrorBar => PlotTypeRecord::ErrorBar,
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
        }
    }
}

impl From<&LegendSpec> for LegendSpecRecord {
    fn from(value: &LegendSpec) -> Self {
        Self {
            visible: value.visible,
            position: value.position.into(),
        }
    }
}

impl From<LegendSpecRecord> for LegendSpec {
    fn from(value: LegendSpecRecord) -> Self {
        Self {
            visible: value.visible,
            position: value.position.into(),
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
        plot_type: series.plot_type.into(),
        name: series.name.clone(),
        line: (&series.line).into(),
        symbol: (&series.symbol).into(),
        axis: series.axis.into(),
        visible: series.visible,
        show_in_legend: series.show_in_legend,
        bar_width: series.bar_width,
        histogram_bins: series.histogram_bins.get(),
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
    let histogram_bins = plotyx_plot::BinCount::new(record.histogram_bins)
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
    })
}

/// `PanelSpec` → enregistrement. Infaillible.
fn panel_spec_to_record(panel: &PanelSpec) -> PanelSpecRecord {
    PanelSpecRecord {
        title: panel.title.clone(),
        series: panel.series.iter().map(series_spec_to_record).collect(),
        bottom: (&panel.bottom).into(),
        left: (&panel.left).into(),
        right: (&panel.right).into(),
        top: (&panel.top).into(),
        legend: (&panel.legend).into(),
        row: panel.row as u64,
        column: panel.column as u64,
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
        series,
        bottom: record.bottom.into(),
        left: record.left.into(),
        right: record.right.into(),
        top: record.top.into(),
        legend: record.legend.into(),
        row: record.row as usize,
        column: record.column as usize,
    })
}

/// `PlotSpec` → enregistrement. Infaillible.
pub(crate) fn plot_spec_to_record(spec: &PlotSpec) -> PlotSpecRecord {
    PlotSpecRecord {
        title: spec.title.clone(),
        panels: spec.panels.iter().map(panel_spec_to_record).collect(),
        background: spec.background.clone(),
        format_version: spec.format_version,
    }
}

/// Enregistrement → `PlotSpec`. `panels` vide n'est pas un échec : c'est le
/// même cas que `Panels::new(Vec::new())` côté `plotyx-plot`, qui fournit un
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

/// Nature d'un nœud → enregistrement de transport. Infaillible : le modèle
/// en mémoire est toujours représentable.
pub(crate) fn kind_to_record(kind: &NodeKind) -> KindRecord {
    match kind {
        NodeKind::Folder { children } => KindRecord::Folder {
            children: children.iter().map(|id| id.raw()).collect(),
        },
        NodeKind::Workbook { sheets, meta } => KindRecord::Workbook {
            sheets: sheets.iter().map(|id| id.raw()).collect(),
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
                ColumnValues::Text(values) => ValuesRecord::Text(values.as_ref().clone()),
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
            }
        }
        NodeKind::Graph { meta, spec } => KindRecord::Graph {
            label: meta.label.clone(),
            comment: meta.comment.clone(),
            spec: plot_spec_to_record(spec),
        },
        NodeKind::Matrix { meta } => KindRecord::Matrix {
            label: meta.label.clone(),
            comment: meta.comment.clone(),
        },
        NodeKind::Note { meta, text } => KindRecord::Note {
            label: meta.label.clone(),
            comment: meta.comment.clone(),
            text: text.clone(),
        },
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
            label,
            comment,
        } => NodeKind::Workbook {
            sheets: sheets.into_iter().map(NodeId::from_raw).collect(),
            meta: WindowMeta { label, comment },
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
        } => {
            let column_type: ColumnType = column_type.into();
            let block = match values {
                ValuesRecord::Float(bytes) => ValueBlock::Float(decode_floats(&bytes)?),
                ValuesRecord::Text(values) => ValueBlock::Text(values),
            };
            let meta = ColumnMeta {
                role: role.into(),
                unit,
                long_name,
                comment,
                display_format,
            };
            // `from_block_with_version`, pas `from_block` : le compteur doit
            // traverser le disque, sinon il ment après un rechargement.
            let column = Column::from_block_with_version(column_type, block, meta, version)
                .map_err(ConvertError::Core)?;
            NodeKind::Column(column)
        }
        KindRecord::Graph {
            label,
            comment,
            spec,
        } => NodeKind::Graph {
            meta: WindowMeta { label, comment },
            spec: record_to_plot_spec(spec)?,
        },
        KindRecord::Matrix { label, comment } => NodeKind::Matrix {
            meta: WindowMeta { label, comment },
        },
        KindRecord::Note {
            label,
            comment,
            text,
        } => NodeKind::Note {
            meta: WindowMeta { label, comment },
            text,
        },
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
