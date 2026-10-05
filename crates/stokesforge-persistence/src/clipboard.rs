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

const PREFIX: &str = "StokesForge Objects\n";

#[derive(Serialize, Deserialize)]
struct ClipboardRecord {
    origin: String,
    objects: Vec<Vec<NodeRecord>>,
}

/// Reconnaît un transport d'objets avant qu'un tableur ne le traite en texte.
pub fn is_object_clipboard(text: &str) -> bool {
    text.starts_with(PREFIX)
}

/// Encode uniquement les objets capturés, sans fichier temporaire.
pub fn encode_objects(origin: &str, copies: &[NodeCopy]) -> PersistenceResult<String> {
    let record = ClipboardRecord {
        origin: origin.into(),
        objects: copies
            .iter()
            .map(|copy| {
                copy.nodes()
                    .iter()
                    .map(|node| NodeRecord {
                        id: node.id().raw(),
                        name: node.name().into(),
                        parent: node.parent().map(NodeId::raw),
                        kind: convert::kind_to_record(node.kind()),
                    })
                    .collect()
            })
            .collect(),
    };
    let bytes = rmp_serde::to_vec_named(&record).map_err(|err| corrupt(err.to_string()))?;
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
    let mut copies = Vec::new();
    for mut nodes in record.objects {
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
        copies.push(
            NodeCopy::capture(project.tree(), NodeId::from_raw(root)).map_err(|source| {
                PersistenceError::InvalidModel {
                    path: "clipboard".into(),
                    source,
                }
            })?,
        );
    }
    Ok((record.origin, copies))
}

fn corrupt(detail: String) -> PersistenceError {
    PersistenceError::Corrupt {
        path: "clipboard".into(),
        detail,
    }
}
