//! Le classeur : [`book_contents`], contenu commun d'un [`Tab::Book`]
//! flottant **ou** détaché (une seule écriture : il ne dépend que de
//! `(ui, état, actions, classeur)`, jamais d'un champ d'`app`), ses onglets de
//! feuille ([`sheet_tabs`]) et le menu Classeur ([`workbook_menu`]). La grille
//! elle-même vit dans `grid.rs`.
//!
//! Sorti d'`app.rs` tel quel (#120).
//!
//! [`Tab::Book`]: super::tab::Tab::Book

use eframe::egui;
use stokesforge_core::NodeId;

use crate::state::{Action, AppState};
use crate::{theme, tr};

use super::claim_file_drop;
use super::grid::{
    grid_contents, show_grid_horizontal_bar, show_grid_vertical_bar,
    show_horizontally_scrollable_grid,
};
use super::shortcuts::{SHORTCUT_DUPLICATE_SHEET, SHORTCUT_RENAME_SHEET};
use super::tab_strip::{tab_strip_with, TabEvent, TabOps, TabStripOutput, TAB_STRIP_HEIGHT};

/// Menu « Classeur » : les gestes sur la feuille affichée, miroir du menu
/// contextuel des onglets et des raccourcis F2 / Ctrl+D.
pub(super) fn workbook_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    book: NodeId,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    if ui
        .add(
            egui::Button::new(tr!("action-rename-sheet"))
                .shortcut_text(ctx.format_shortcut(&SHORTCUT_RENAME_SHEET)),
        )
        .on_hover_text(tr!("action-rename-sheet", "hint"))
        .clicked()
    {
        actions.push(Action::BeginRenameSheet(sheet));
        ui.close();
    }
    if ui
        .add(
            egui::Button::new(tr!("action-duplicate-sheet"))
                .shortcut_text(ctx.format_shortcut(&SHORTCUT_DUPLICATE_SHEET)),
        )
        .on_hover_text(tr!("action-duplicate-sheet", "hint"))
        .clicked()
    {
        actions.push(Action::DuplicateSheet(sheet));
        ui.close();
    }
    let can_delete = state.can_delete_sheet(sheet);
    let delete = ui
        .add_enabled(can_delete, egui::Button::new(tr!("action-delete-sheet")))
        .on_hover_text(tr!("action-delete-sheet", "hint"))
        .on_disabled_hover_text(tr!("action-delete-sheet", "disabled"));
    if delete.clicked() {
        actions.push(Action::RequestDeleteSheet(sheet));
        ui.close();
    }
    ui.separator();
    if ui.button(tr!("workbook-worksheet-add")).clicked() {
        actions.push(Action::AddWorksheet { book });
        ui.close();
    }
}

/// Bande d'onglets des feuilles d'un classeur : [`tab_strip`] avec, en plus,
/// « Ajouter une feuille » au clic droit.
fn sheet_tabs(
    ui: &mut egui::Ui,
    sheets: &[(NodeId, String)],
    selected: NodeId,
    on_select: impl FnMut(NodeId),
    on_add: impl FnMut(),
    ops: Option<&TabOps<NodeId>>,
    on_event: impl FnMut(TabEvent<NodeId>),
) -> TabStripOutput {
    let add_label = tr!("workbook-worksheet-add");
    tab_strip_with(
        ui,
        sheets,
        selected,
        on_select,
        Some(&add_label),
        on_add,
        ops,
        on_event,
    )
}

/// Contenu commun d'un [`Tab::Book`] flottant ou détaché : la grille
/// ([`grid_contents`]) scopée à la feuille choisie *dans ce classeur*, et
/// la sous-navigation entre les feuilles — des onglets compacts en **bas**
/// de la grille (choix assumé, pas une convention préexistante du crate :
/// aucun autre onglet de sheet de ce genre n'existait à imiter). Les
/// boutons « Insérer/Supprimer 1 ligne » ont disparu : l'ajout de ligne se
/// fait désormais en pressant Entrée sur la dernière cellule d'une colonne
/// ([`Action::CommitCellAndAdvance`]), la suppression par la touche Suppr
/// sur la sélection ([`Action::DeleteSelectedCells`]).
pub(super) fn book_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    book: NodeId,
) {
    claim_file_drop(ui, book);
    let sheets = state.worksheets_of(book);
    let selected = state.selected_sheet_in(book).unwrap_or(book);
    let mut chosen_sheet = None;
    let mut add_sheet = false;
    // La bande d'onglets est retranchée du rectangle du classeur **avant**
    // de construire la grille : réserver une hauteur dans `allocate_ui` ne
    // suffisait pas, sa `ScrollArea` pouvait ensuite faire grandir la taille
    // consommée et repousser les onglets hors du cadre. Pas d'`egui::Panel`
    // pour ce découpage : il remplace le rognage de son parent par le sien
    // au lieu de le combiner, et dessinait les onglets sous la vue quand la
    // fenêtre y était coupée (`show_floating_window`). Deux sous-`Ui` bornés,
    // chacun rogné à l'intersection de son rectangle et du rognage parent.
    let area = ui.available_rect_before_wrap();
    // Le bord droit de la fenêtre est réservé, sur toute sa hauteur, à la
    // barre de défilement verticale (B10) : la zone de défilement
    // horizontale et les onglets de feuille ne portent que le reste, et la
    // barre reste plaquée au bord de la fenêtre quelle que soit la largeur
    // des colonnes. Le bas est réservé à la barre horizontale de la grille,
    // **sous** les onglets de feuille, comme dans un tableur. Du haut vers le
    // bas : grille, onglets, barre horizontale.
    let bar_width = theme::scroll_bar_allocated_width();
    let (main_area, vertical_bar_strip) =
        area.split_left_right_at_x(area.right() - bar_width.min(area.width()));
    // #51 : la bande de la barre horizontale n'est réservée que si la grille
    // déborde en largeur — l'état vient de l'image précédente (la hauteur de
    // la bande ne change pas la largeur du contenu : pas d'oscillation).
    let horizontal_bar_present_id = ui.id().with("grid_horizontal_bar_present");
    let horizontal_bar_present = ui
        .memory(|memory| memory.data.get_temp::<bool>(horizontal_bar_present_id))
        .unwrap_or(false);
    let horizontal_bar_height = if horizontal_bar_present {
        bar_width
    } else {
        0.0
    };
    let (upper_area, horizontal_bar_strip) = main_area
        .split_top_bottom_at_y(main_area.bottom() - horizontal_bar_height.min(main_area.height()));
    let tabs_height = TAB_STRIP_HEIGHT.min(upper_area.height());
    let (grid_rect, tabs_rect) =
        upper_area.split_top_bottom_at_y(upper_area.bottom() - tabs_height);
    // Avec la barre, les onglets descendent de la moitié de la bande : l'espace
    // sous eux est réduit de moitié et le même espace apparaît au-dessus
    // (#51). Sans barre, aucune réserve ni décalage.
    let tabs_rect = tabs_rect.translate(egui::vec2(0.0, horizontal_bar_height / 2.0));
    let mut bounded = |rect: egui::Rect| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        child
    };
    let mut tabs_ui = bounded(tabs_rect);
    let mut grid_ui = bounded(grid_rect);
    let sheet_ops = TabOps {
        renaming: state.renaming_sheet(),
        can_delete: sheets.len() > 1,
    };
    sheet_tabs(
        &mut tabs_ui,
        &sheets,
        selected,
        |id| chosen_sheet = Some(id),
        || add_sheet = true,
        Some(&sheet_ops),
        |event| {
            actions.push(match event {
                TabEvent::BeginRename(sheet) => Action::BeginRenameSheet(sheet),
                TabEvent::CommitRename(sheet, name) => Action::CommitRenameSheet { sheet, name },
                TabEvent::CancelRename => Action::CancelRenameSheet,
                TabEvent::Duplicate(sheet) => Action::DuplicateSheet(sheet),
                TabEvent::Delete(sheet) => Action::RequestDeleteSheet(sheet),
            })
        },
    );
    if sheets.is_empty() {
        grid_ui.centered_and_justified(|ui| {
            ui.weak(tr!("workbook-no-worksheets"));
        });
    } else {
        let vertical_bar_id = grid_ui.id().with("grid_vertical_bar");
        let horizontal_bar_id = grid_ui.id().with("grid_horizontal_bar");
        let pending_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(vertical_bar_id));
        let pending_horizontal_offset: Option<f32> =
            grid_ui.memory_mut(|memory| memory.data.remove_temp(horizontal_bar_id));
        let scroll =
            show_horizontally_scrollable_grid(&mut grid_ui, pending_horizontal_offset, |ui| {
                grid_contents(ui, state, actions, selected, pending_offset)
            });
        let overflows = scroll.content_size.x > scroll.inner_rect.width() + 0.5;
        if overflows != horizontal_bar_present {
            grid_ui.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(horizontal_bar_present_id, overflows)
            });
            grid_ui.ctx().request_repaint();
        }
        let mut horizontal_bar_ui = bounded(horizontal_bar_strip);
        show_grid_horizontal_bar(
            &mut horizontal_bar_ui,
            horizontal_bar_id,
            horizontal_bar_strip,
            &scroll,
        );
        if let Some(table) = scroll.inner {
            let mut bar_ui = bounded(vertical_bar_strip);
            show_grid_vertical_bar(&mut bar_ui, vertical_bar_id, vertical_bar_strip, &table);
        }
    }
    ui.advance_cursor_after_rect(area);
    if let Some(id) = chosen_sheet {
        state.dispatch(Action::SelectSheet(id));
    }
    if add_sheet {
        actions.push(Action::AddWorksheet { book });
    }
}
