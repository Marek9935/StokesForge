//! The "To:" and "Folder:" controls shared by the Import window and the Import
//! from Origin window (#537, R1): one painter, two windows.
//!
//! Both windows put two secondary buttons on the left of their footer (S-F6),
//! each with a menu: **To:** (where the workbooks go, then how the sheets share
//! worksheets) and **Folder:** (the project folders as an indented list, then
//! "New Folder…" with its name field). The menus are painted here from neutral
//! data and answer with a pick; each window applies the pick to its own state
//! (the classic window writes its import state, Origin pushes an action).

use eframe::egui;
use stokesforge_app_state::state::{ImportFolder, ImportFolderChoice};
use stokesforge_app_state::tr;
use stokesforge_import::FilesLayout;

use super::cursors;
use super::menu_sections::{menu_choice, MenuSections};
use crate::icons::{icon_image, Icon, IconRole};
use crate::theme;

/// One choice of "Where": a destination of the new workbooks.
pub(super) struct WhereOption<W> {
    /// What the window stores when it is picked.
    pub value: W,
    /// Its text.
    pub label: String,
    /// It is the current one.
    pub selected: bool,
    /// Greyed out, with this reason (the tooltip of a greyed entry).
    pub unavailable: Option<String>,
}

/// The sheet layouts of a window: a heading and the three classic choices.
pub(super) struct LayoutOptions {
    /// The heading of the section ("Multiple files", "Origin sheets").
    pub heading: String,
    /// Per file/sheet, side by side, shared X: in this order.
    pub labels: [String; 3],
    /// The current layout.
    pub current: FilesLayout,
}

/// What the "To:" menu answers.
pub(super) enum ToPick<W> {
    /// A destination.
    Where(W),
    /// A sheet layout.
    Layout(FilesLayout),
}

/// A button that opens a menu, with its caret: up in a footer, whose menus open
/// upward, down in the body of a window.
pub(super) fn menu_button(ui: &mut egui::Ui, text: String, caret: Icon) -> egui::Response {
    // The caret is an icon at the right of the label; the label leaves room for it.
    let response = ui.add(egui::Button::new(format!("{text}\u{2003}\u{2002}")));
    let size = crate::icons::ICON_SIZE;
    let rect = egui::Rect::from_center_size(
        egui::pos2(response.rect.right() - size, response.rect.center().y),
        egui::vec2(size, size),
    );
    icon_image(caret, IconRole::Neutral).paint_at(ui, rect);
    cursors::clickable(response)
}

/// The "To:" menu: the destinations, then `between` (a section of the classic
/// window, absent when `None`), then the layouts (absent when `None`).
pub(super) fn to_menu<W: Copy>(
    ui: &mut egui::Ui,
    wheres: &[WhereOption<W>],
    between: Option<&mut dyn FnMut(&mut egui::Ui)>,
    layouts: Option<&LayoutOptions>,
) -> Option<ToPick<W>> {
    let mut pick = None;
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        for option in wheres {
            match &option.unavailable {
                None => {
                    if menu_choice(ui, option.selected, option.label.as_str()).clicked() {
                        pick = Some(ToPick::Where(option.value));
                        ui.close();
                    }
                }
                Some(reason) => {
                    ui.add_enabled_ui(false, |ui| {
                        menu_choice(ui, false, option.label.as_str());
                    })
                    .response
                    .on_disabled_hover_text(reason);
                }
            }
        }
    });
    sections.section(ui, between.is_some(), |ui| {
        if let Some(between) = between {
            between(ui);
        }
    });
    sections.section(ui, layouts.is_some(), |ui| {
        let Some(layouts) = layouts else { return };
        ui.label(theme::dimmed_text(layouts.heading.as_str()));
        for (layout, text) in [
            FilesLayout::SheetPerFile,
            FilesLayout::SideBySide,
            FilesLayout::SharedX,
        ]
        .into_iter()
        .zip(&layouts.labels)
        {
            if menu_choice(ui, layouts.current == layout, text.as_str()).clicked() {
                pick = Some(ToPick::Layout(layout));
                ui.close();
            }
        }
    });
    pick
}

/// The text of the "Folder:" button: the path of the chosen folder, "Project"
/// for the root, the path of the parent then the typed name for a new folder.
pub(super) fn folder_label(choices: &[ImportFolderChoice], folder: &ImportFolder) -> String {
    let path_of = |id| {
        choices
            .iter()
            .find(|choice| choice.id == id)
            .map_or_else(|| tr!("import-folder-root"), |choice| choice.label.clone())
    };
    match folder {
        ImportFolder::Existing(id) => path_of(*id),
        ImportFolder::New { parent, name } => match name.trim() {
            "" => tr!("import-folder-new"),
            name => format!("{} / {name}", path_of(*parent)),
        },
    }
}

/// The "Folder:" menu: the folders as an indented list (root first), then
/// "New Folder…", which brings a name field into the menu.
pub(super) fn folder_menu(
    ui: &mut egui::Ui,
    choices: &[ImportFolderChoice],
    current: &ImportFolder,
) -> Option<ImportFolder> {
    let mut pick = None;
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        theme::page_scroll_area()
            .max_height(260.0)
            .auto_shrink([true, true])
            .id_salt("stokesforge_import_folder_list")
            .show(ui, |ui| {
                for choice in choices {
                    let selected =
                        matches!(current, ImportFolder::Existing(id) if *id == choice.id);
                    let name = if choice.depth == 0 {
                        choice.label.as_str()
                    } else {
                        choice.label.rsplit(" / ").next().unwrap_or(&choice.label)
                    };
                    let clicked = ui
                        .horizontal(|ui| {
                            ui.add_space(theme::SPACE_SECTION * 2.0 * choice.depth as f32);
                            menu_choice(ui, selected, name).clicked()
                        })
                        .inner;
                    if clicked {
                        pick = Some(ImportFolder::Existing(choice.id));
                        ui.close();
                    }
                }
            });
    });
    sections.section(ui, true, |ui| {
        let (new_selected, parent) = match current {
            ImportFolder::New { parent, .. } => (true, *parent),
            ImportFolder::Existing(id) => (false, *id),
        };
        if menu_choice(ui, new_selected, tr!("import-folder-new")).clicked() && !new_selected {
            pick = Some(ImportFolder::New {
                parent,
                name: String::new(),
            });
        }
        if let ImportFolder::New { parent, name } = current {
            let mut typed = name.clone();
            let field = egui::TextEdit::singleline(&mut typed)
                .hint_text(tr!("import-folder-name-hint"))
                .desired_width(180.0);
            let response = ui.add(field);
            let asked = egui::Id::new("stokesforge_import_folder_name_focused");
            if !ui.data(|data| data.get_temp::<bool>(asked).unwrap_or(false)) {
                response.request_focus();
                ui.data_mut(|data| data.insert_temp(asked, true));
            }
            if response.changed() {
                pick = Some(ImportFolder::New {
                    parent: *parent,
                    name: typed,
                });
            }
        } else {
            ui.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("stokesforge_import_folder_name_focused"),
                    false,
                )
            });
        }
    });
    pick
}
