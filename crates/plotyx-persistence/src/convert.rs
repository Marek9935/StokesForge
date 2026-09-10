//! Conversions écrites à la main entre les structures de transport
//! (`schema.rs`) et le modèle en mémoire (`plotyx_core`). Le format ne se
//! dérive pas du modèle (voir la doc de crate) : ces fonctions sont le seul
//! endroit où les deux mondes se rencontrent.

use plotyx_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CoreError, Node, NodeId, NodeKind,
    ValueBlock, WindowMeta,
};

use crate::schema::{KindRecord, NodeRecord, RoleRecord, TypeRecord, ValuesRecord};

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
        NodeKind::Graph { meta } => KindRecord::Graph {
            label: meta.label.clone(),
            comment: meta.comment.clone(),
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
        KindRecord::Graph { label, comment } => NodeKind::Graph {
            meta: WindowMeta { label, comment },
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
