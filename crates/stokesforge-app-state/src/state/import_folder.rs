//! The folder of an import (#537): where the new workbooks of the Import and
//! Import from Origin windows are created.
//!
//! The window offers the project folders and "New Folder…". The choice is data
//! ([`ImportFolder`]): an existing folder, or a name typed in the window. A new
//! folder is created only when Import runs, **inside the same "Import" history
//! entry** as the workbooks, so one Undo removes both. A folder that is gone
//! when Import runs (script, undo) means the root, and a message says so.

use stokesforge_commands::Command;
use stokesforge_core::{check_name, same_name_ignoring_case, NodeId};
use stokesforge_importer::ImportItem;

use super::{AppState, ImportFolder, ImportFolderChoice};
use crate::tr;

/// A file as the messages name it: its name without the extension, followed by
/// the block rank when the file holds several.
pub(super) fn import_item_label(item: &ImportItem) -> String {
    let stem = item
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("file");
    match item.block {
        Some(block) => format!("{stem}_{}", block + 1),
        None => stem.to_string(),
    }
}

impl AppState {
    /// The folder of the imports that have no window to choose it (menu, drop,
    /// recent files): the one a new window would go into, the folder selected
    /// in the Project Manager (the root when none is).
    pub(super) fn import_default_folder(&self) -> ImportFolder {
        ImportFolder::Existing(self.creation_folder())
    }

    /// The folder the visible import window targets: the Import window's, else
    /// the Origin chooser's, `None` when neither is open.
    #[must_use]
    pub fn import_folder(&self) -> Option<&ImportFolder> {
        match (&self.import_state, &self.origin_import) {
            (Some(import), _) => Some(&import.folder),
            (None, Some(chooser)) => Some(&chooser.folder),
            _ => None,
        }
    }

    /// [`Action::SetImportFolder`](super::Action::SetImportFolder).
    pub(super) fn set_import_folder(&mut self, folder: ImportFolder) {
        match (&mut self.import_state, &mut self.origin_import) {
            (Some(import), _) => import.folder = folder,
            (None, Some(chooser)) => chooser.folder = folder,
            _ => {}
        }
    }

    /// Every folder of the project, root first, in tree order: the list the
    /// window offers next to "New Folder…".
    #[must_use]
    pub fn import_folder_choices(&self) -> Vec<ImportFolderChoice> {
        let tree = &self.snapshot.tree;
        let mut out = Vec::new();
        let mut stack = vec![(tree.root(), 0usize, String::new())];
        while let Some((id, depth, path)) = stack.pop() {
            let Some(node) = tree.node(id) else {
                continue;
            };
            let label = if depth == 0 {
                tr!("import-folder-root")
            } else {
                path.clone()
            };
            out.push(ImportFolderChoice { id, depth, label });
            for &child in node.children().iter().rev() {
                let Some(child_node) = tree.node(child) else {
                    continue;
                };
                if child_node.kind().is_folder() {
                    let child_path = if depth == 0 {
                        child_node.name().to_string()
                    } else {
                        format!("{path} / {}", child_node.name())
                    };
                    stack.push((child, depth + 1, child_path));
                }
            }
        }
        out
    }

    /// Why Import is greyed out because of the folder: a new folder needs a
    /// valid name that its parent does not already hold. `None` when the folder
    /// is fine (and when the parent is gone: the root then takes it).
    #[must_use]
    pub fn import_folder_blocker(&self, folder: &ImportFolder) -> Option<String> {
        let ImportFolder::New { parent, name } = folder else {
            return None;
        };
        if name.trim().is_empty() || check_name(name.trim()).is_err() {
            return Some(tr!("disabled-import-folder-name-empty"));
        }
        let tree = &self.snapshot.tree;
        let taken = tree.node(*parent).is_some_and(|parent| {
            parent.children().iter().any(|&child| {
                tree.node(child)
                    .is_some_and(|node| same_name_ignoring_case(node.name(), name.trim()))
            })
        });
        taken.then(|| tr!("disabled-import-folder-name-taken"))
    }

    /// The folder the new workbooks go into, inside the open "Import" macro:
    /// a gone folder is the root (`gone` says so), a new folder is created here.
    pub(super) fn import_parent(&mut self, folder: &ImportFolder, gone: &mut bool) -> NodeId {
        let root = self.snapshot.tree.root();
        let usable = |state: &Self, id: NodeId| {
            state
                .snapshot
                .tree
                .node(id)
                .is_some_and(|node| node.kind().is_folder() || id == root)
        };
        match folder {
            ImportFolder::Existing(id) if usable(self, *id) => *id,
            ImportFolder::Existing(_) => {
                *gone = true;
                root
            }
            ImportFolder::New { parent, name } => {
                let parent = if usable(self, *parent) {
                    *parent
                } else {
                    *gone = true;
                    root
                };
                let previous = self
                    .snapshot
                    .tree
                    .node(parent)
                    .and_then(|node| node.children().last().copied());
                self.execute(Command::AddFolder {
                    parent,
                    name: name.trim().to_string(),
                });
                let created = self
                    .snapshot
                    .tree
                    .node(parent)
                    .and_then(|node| node.children().last().copied())
                    .filter(|last| Some(*last) != previous);
                created.unwrap_or(parent)
            }
        }
    }

    /// The folder of a reopened import: a new folder that the import made and
    /// that is still there (the import was not undone) is now an existing one.
    pub(super) fn reopened_folder(&self, folder: &ImportFolder) -> ImportFolder {
        let ImportFolder::New { parent, name } = folder else {
            return folder.clone();
        };
        let tree = &self.snapshot.tree;
        let found = tree.node(*parent).and_then(|node| {
            node.children().iter().copied().find(|&child| {
                tree.node(child).is_some_and(|node| {
                    node.kind().is_folder() && same_name_ignoring_case(node.name(), name.trim())
                })
            })
        });
        found.map_or_else(|| folder.clone(), ImportFolder::Existing)
    }
}
