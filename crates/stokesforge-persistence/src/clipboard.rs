//! Transport textuel entre instances, sans version de format (#105).
//! MessagePack réutilise les conversions du `.stf` ; l'hexadécimal permet de
//! traverser le presse-papiers texte d'egui, y compris le récepteur Wayland.
//! Chaque objet est validé comme un arbre indépendant avant d'être capturé.

use std::fmt::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use stokesforge_core::{NodeCopy, NodeId};

use crate::schema::{FileBody, KindRecord, NodeRecord};
use crate::{build_project, convert, PersistenceError, PersistenceResult, MAX_NODE_ID};

/// Taille maximale du texte publié : 64 Mio, hexadécimal compris.
pub const MAX_OBJECT_CLIPBOARD_BYTES: usize = 64 * 1024 * 1024;

/// Le sérialiseur s’arrête au plafond avant de construire un énorme texte.
struct ClipboardBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for ClipboardBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("object clipboard exceeds 64 MiB"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const PREFIX: &str = "StokesForge Objects\n";

#[derive(Serialize, Deserialize)]
struct ClipboardRecord {
    origin: String,
    objects: Vec<Vec<NodeRecord>>,
    #[serde(default)]
    graph_data: Vec<Vec<(u64, Vec<NodeRecord>)>>,
}

/// Reconnaît un transport d'objets avant qu'un tableur ne le traite en texte.
pub fn is_object_clipboard(text: &str) -> bool {
    text.starts_with(PREFIX)
}

/// Encode uniquement les objets capturés, sans fichier temporaire.
pub fn encode_objects(origin: &str, copies: &[NodeCopy]) -> PersistenceResult<String> {
    let record = ClipboardRecord {
        origin: origin.into(),
        objects: copies.iter().map(copy_records).collect(),
        graph_data: copies
            .iter()
            .map(|copy| {
                copy.graph_data()
                    .iter()
                    .map(|(id, data)| (id.raw(), copy_records(data)))
                    .collect()
            })
            .collect(),
    };
    let mut buffer = ClipboardBuffer {
        bytes: Vec::new(),
        limit: (MAX_OBJECT_CLIPBOARD_BYTES - PREFIX.len()) / 2,
    };
    record
        .serialize(&mut rmp_serde::Serializer::new(&mut buffer).with_struct_map())
        .map_err(|err| corrupt(err.to_string()))?;
    let bytes = buffer.bytes;
    let mut text = String::with_capacity(PREFIX.len() + bytes.len() * 2);
    text.push_str(PREFIX);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    Ok(text)
}

/// Décode et valide le transport ; l'origine sert uniquement à reconnaître
/// les références internes au projet courant, jamais à comparer ses IDs.
pub fn decode_objects(text: &str) -> PersistenceResult<(String, Vec<NodeCopy>)> {
    if text.len() > MAX_OBJECT_CLIPBOARD_BYTES {
        return Err(corrupt("object clipboard exceeds 64 MiB".into()));
    }
    let payload = text
        .strip_prefix(PREFIX)
        .ok_or_else(|| corrupt("not an object clipboard".into()))?;
    if payload.len() % 2 != 0 {
        return Err(corrupt("truncated object clipboard".into()));
    }
    let bytes = payload
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let digit = |byte: u8| (byte as char).to_digit(16).map(|n| n as u8);
            match (digit(pair[0]), digit(pair[1])) {
                (Some(a), Some(b)) => Ok(a * 16 + b),
                _ => Err(corrupt("invalid hexadecimal data".into())),
            }
        })
        .collect::<PersistenceResult<Vec<_>>>()?;
    let record: ClipboardRecord =
        rmp_serde::from_slice(&bytes).map_err(|err| corrupt(err.to_string()))?;
    if record.objects.is_empty() {
        return Err(corrupt("empty object clipboard".into()));
    }
    if !record.graph_data.is_empty() && record.graph_data.len() != record.objects.len() {
        return Err(corrupt("invalid graph data count".into()));
    }
    let mut data = record.graph_data.into_iter();
    let mut copies = Vec::new();
    for nodes in record.objects {
        let copy = decode_copy(nodes)?;
        let dependencies = data
            .next()
            .unwrap_or_default()
            .into_iter()
            .map(|(graph, nodes)| Ok((NodeId::from_raw(graph), decode_copy(nodes)?)))
            .collect::<PersistenceResult<Vec<_>>>()?;
        copies.push(copy.with_graph_data(dependencies).map_err(|source| {
            PersistenceError::InvalidModel {
                path: "clipboard".into(),
                source,
            }
        })?);
    }
    Ok((record.origin, copies))
}

fn copy_records(copy: &NodeCopy) -> Vec<NodeRecord> {
    copy.nodes()
        .iter()
        .map(|node| NodeRecord {
            id: node.id().raw(),
            name: node.name().into(),
            parent: node.parent().map(NodeId::raw),
            kind: convert::kind_to_record(node.kind()),
        })
        .collect()
}

fn decode_copy(mut nodes: Vec<NodeRecord>) -> PersistenceResult<NodeCopy> {
    let root = nodes
        .first()
        .ok_or_else(|| corrupt("empty object".into()))?
        .id;
    let wrapper = nodes
        .iter()
        .map(|n| n.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .filter(|id| *id <= MAX_NODE_ID)
        .ok_or_else(|| corrupt("invalid object identifiers".into()))?;
    nodes[0].parent = Some(wrapper);
    nodes.insert(
        0,
        NodeRecord {
            id: wrapper,
            name: "Clipboard".into(),
            parent: None,
            kind: KindRecord::Folder {
                children: vec![root],
            },
        },
    );
    let project = build_project(
        FileBody {
            root: wrapper,
            next_id_hint: wrapper,
            nodes,
            saved_colors: Default::default(),
            window_geometries: Default::default(),
            save_counter: 0,
        },
        Path::new("clipboard"),
    )?;
    NodeCopy::capture(project.tree(), NodeId::from_raw(root)).map_err(|source| {
        PersistenceError::InvalidModel {
            path: "clipboard".into(),
            source,
        }
    })
}

fn corrupt(detail: String) -> PersistenceError {
    PersistenceError::Corrupt {
        path: "clipboard".into(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn le_plafond_arrete_le_serialiseur_avant_allocation() {
        let mut buffer = ClipboardBuffer {
            bytes: Vec::new(),
            limit: 4,
        };
        buffer.write_all(&[1, 2, 3, 4]).unwrap();
        assert!(buffer.write_all(&[5]).is_err());
        assert_eq!(buffer.bytes, [1, 2, 3, 4]);
        assert!(vec![0; 16]
            .serialize(&mut rmp_serde::Serializer::new(&mut buffer))
            .is_err());
        assert_eq!(buffer.bytes.len(), 4);
    }
}
