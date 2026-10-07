//! Le classeur : [`book_contents`], contenu commun d'un [`Tab::Book`]
//! flottant **ou** détaché (une seule écriture : il ne dépend que de
//! `(ui, état, actions, classeur)`, jamais d'un champ d'`app`), ses onglets de
//! feuille ([`sheet_tabs`]) et le menu Classeur ([`workbook_menu`]). La grille
//! elle-même vit dans `grid.rs`.
//!
//! Sorti d'`app.rs` tel quel (#120). Les tests, en fin de fichier, portent
//! sur la disposition (grille, onglets, barres déportées, rognage) et les
//! menus du classeur ; ceux du clavier et de l'en-tête sont dans `grid.rs`.
//!
//! [`Tab::Book`]: super::tab::Tab::Book

use eframe::egui;
use stokesforge_core::NodeId;

use crate::state::{Action, AppState};
use crate::{theme, tr};

use super::file_drop::claim_file_drop;
use super::graph_targets::send_to_graph_menu;
use super::grid::{
    columns_menu_contents, grid_contents, metadata_row_visible, plot_menu,
    set_metadata_row_visible, show_grid_horizontal_bar, show_grid_vertical_bar,
    show_horizontally_scrollable_grid,
};
use super::menu_sections::MenuSections;
use super::shortcuts::SHORTCUT_DUPLICATE_SHEET;
use super::tab_strip::{
    tab_strip_with, TabEvent, TabOps, TabStripOutput, TabStripSide, TAB_STRIP_HEIGHT,
};

/// Menu « Classeur » : les gestes sur la feuille affichée, miroir du menu
/// contextuel des onglets et du raccourci Ctrl+D (F2 édite les cellules, jamais le nom des feuilles, #130/#143).
/// Le menu du haut **enveloppe** les mêmes contenus que les clics droits
/// (#152) : Feuille ▸, Colonnes ▸ et Tracer ▸ nomment la cible que le clic
/// droit, lui, désigne déjà.
pub(super) fn workbook_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    book: NodeId,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let columns = state
        .selection(sheet)
        .map(|selection| selection.columns(state.tree(), sheet))
        .unwrap_or_default();
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        ui.menu_button(tr!("menu-sheet"), |ui| {
            sheet_menu(ui, ctx, state, book, sheet, actions);
        });
        ui.menu_button(tr!("menu-columns"), |ui| {
            columns_menu_contents(ui, state, actions, sheet, &columns);
        });
        plot_menu(ui, actions, sheet, &columns);
        send_to_graph_menu(ui, state.tree(), actions, sheet, &columns);
    });
    sections.section(ui, true, |ui| {
        let mut show = metadata_row_visible(ctx);
        if ui
            .checkbox(&mut show, tr!("menu-window-show-metadata-row"))
            .on_hover_text(tr!("menu-window-show-metadata-row", "hint"))
            .changed()
        {
            set_metadata_row_visible(ctx, show);
        }
    });
}

/// Contenu du menu Feuille : Add Worksheet ─ Rename · Duplicate ─ Delete,
/// dans l'ordre du clic droit des onglets.
fn sheet_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &AppState,
    book: NodeId,
    sheet: NodeId,
    actions: &mut Vec<Action>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui.button(tr!("workbook-worksheet-add")).clicked() {
            actions.push(Action::AddWorksheet { book });
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add(egui::Button::new(tr!("action-rename-sheet")))
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
    });
    sections.section(ui, true, |ui| {
        let can_delete = state.can_delete_sheet(sheet);
        let delete = ui
            .add_enabled(can_delete, egui::Button::new(tr!("action-delete-sheet")))
            .on_hover_text(tr!("action-delete-sheet", "hint"))
            .on_disabled_hover_text(tr!("action-delete-sheet", "disabled"));
        if delete.clicked() {
            actions.push(Action::RequestDeleteSheet(sheet));
            ui.close();
        }
    });
}

/// Bande d'onglets des feuilles d'un classeur : [`tab_strip`](crate::app::tab_strip::tab_strip) avec, en plus,
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
        TabStripSide::Bottom,
        sheets,
        selected,
        on_select,
        Some(&add_label),
        on_add,
        ops,
        on_event,
    )
}

/// Contenu commun d'un [`Tab::Book`](crate::app::tab::Tab::Book) flottant ou détaché : la grille
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
    // Le viewport racine consomme déjà ce geste sur le classeur actif.
    // Dans une fenêtre OS séparée, le même contenu reçoit son propre clavier.
    if ui.ctx().viewport_id() != egui::ViewportId::ROOT
        && !ui.ctx().text_edit_focused()
        && state.renaming_sheet().is_none()
        && ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_DUPLICATE_SHEET))
    {
        actions.push(Action::DuplicateSheet(selected));
    }
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

#[cfg(test)]
mod tests {
    //! Tests du classeur, sans fenêtre : disposition (grille, onglets de
    //! feuille, barres de défilement déportées), rognage, menus. Ils lisent
    //! ce qui est peint (rectangles, textes, rognages) ou les actions
    //! poussées, jamais le texte source.

    use super::super::window_chrome::show_floating_window;
    use super::*;
    use stokesforge_core::{Column, Project};
    use stokesforge_session::Session;

    /// Un classeur d'une feuille, `colonnes` colonnes de `lignes` lignes.
    fn classeur(colonnes: usize, lignes: usize) -> (Session, NodeId) {
        let mut project = Project::new("Classeur").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let valeurs: Vec<f64> = (0..lignes).map(|ligne| ligne as f64).collect();
        for _ in 0..colonnes {
            project
                .add_column(sheet, None, None, Column::from_f64(&valeurs))
                .unwrap();
        }
        (Session::spawn(project, None).unwrap(), book)
    }

    /// Centre du premier texte peint égal à `needle`.
    fn centre_du_texte(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == needle => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("aucun texte peint ne porte {needle:?}"))
    }

    /// Clic gauche en `pos`, réparti sur trois images (survol, pression,
    /// relâchement).
    fn cliquer(paint: &mut impl FnMut(Vec<egui::Event>) -> egui::FullOutput, pos: egui::Pos2) {
        paint(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    // Un classeur coupé par le bas de la vue : sa bande d'onglets de
    // feuille, posée au bas de la fenêtre, ne doit pas être dessinée sous la
    // vue (sur la barre de défilement ou la console). `egui::Panel` remplace
    // le rognage de son parent au lieu de le combiner : c'est lui qui
    // faisait déborder les onglets.
    #[test]
    fn un_classeur_coupe_par_le_bas_de_la_vue_ne_dessine_rien_dessous() {
        let (session, book) = classeur(2, 2);
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let id = egui::Id::new("classeur_coupe");
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1400.0, 400.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(500.0, 400.0));
        let mut actions = Vec::new();
        let mut output = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1000.0),
                )),
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    window_rect,
                    viewport,
                    false,
                    |ui| book_contents(ui, &mut state, &mut actions, book),
                    |_, _| {},
                );
            });
            frame.textures_delta.clear();
            output = Some(frame);
        }
        let output = output.unwrap();
        assert!(!output.shapes.is_empty());
        // Une forme dont le rognage est vide (onglets entièrement sous la
        // vue) ne dessine rien : seule compte une découpe non vide.
        for painted in output
            .shapes
            .iter()
            .filter(|painted| painted.clip_rect.is_positive())
        {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "une forme du classeur est découpée hors de la vue : {:?}",
                painted.clip_rect,
            );
        }
    }

    /// Rectangles pleins peints par `shapes`, à plat, avec leur rognage.
    fn rectangles_et_rognages(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        fn visiter(
            shape: &egui::Shape,
            clip: egui::Rect,
            sortie: &mut Vec<(egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Rect(rect) => sortie.push((rect.rect, clip)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visiter(shape, clip, sortie);
                    }
                }
                _ => {}
            }
        }
        let mut sortie = Vec::new();
        for painted in shapes {
            visiter(&painted.shape, painted.clip_rect, &mut sortie);
        }
        sortie
    }

    /// Écran de 300 x 320 : `book_contents` d'un classeur de 12 colonnes
    /// (bien plus larges que l'écran) et 200 lignes, peint six images, la
    /// cinquième (indice 4) recevant `events`. Renvoie l'écran et les
    /// rectangles (avec rognage) de la dernière image.
    fn peindre_classeur_large(
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Vec<(egui::Rect, egui::Rect)>) {
        peindre_classeur(12, events)
    }

    /// [`peindre_classeur_large`] pour un classeur de `colonnes` colonnes.
    fn peindre_classeur(
        colonnes: usize,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, Vec<(egui::Rect, egui::Rect)>) {
        let (session, book) = classeur(colonnes, 200);
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 320.0));
        let mut actions = Vec::new();
        let mut derniere = None;
        for frame_index in 0..6 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: if frame_index == 4 {
                    events.clone()
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut frame = context.run_ui(input, |ui| {
                book_contents(ui, &mut state, &mut actions, book);
            });
            frame.textures_delta.clear();
            derniere = Some(frame);
        }
        (screen, rectangles_et_rognages(&derniere.unwrap().shapes))
    }

    /// La poignée de la barre verticale (pas sa piste) : mince, courte, dans la bande de
    /// droite de l'écran.
    fn poignees_de_barre(
        screen: egui::Rect,
        rectangles: &[(egui::Rect, egui::Rect)],
    ) -> Vec<(egui::Rect, egui::Rect)> {
        let largeur_barre = theme::scroll_bar_allocated_width();
        rectangles
            .iter()
            .copied()
            .filter(|(rect, _)| {
                rect.width() <= largeur_barre
                    && rect.height() >= 11.9
                    && rect.height() < 100.0
                    && rect.left() >= screen.right() - largeur_barre - 0.5
            })
            .collect()
    }

    /// B10 : la barre de
    /// défilement verticale de la grille est plaquée au bord droit de la
    /// fenêtre, même quand les colonnes débordent (elle suivait auparavant
    /// la largeur des colonnes et sortait de la vue).
    #[test]
    fn la_barre_verticale_de_la_grille_reste_au_bord_droit_de_la_fenetre() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let poignees = poignees_de_barre(screen, &rectangles);
        assert!(
            !poignees.is_empty(),
            "la poignée de la barre verticale doit être dans la bande de droite de la fenêtre \
             ({screen:?}) ; rectangles peints : {rectangles:?}",
        );
        for (rect, clip) in poignees {
            assert!(
                screen.contains_rect(rect) && clip.is_positive(),
                "la poignée {rect:?} sort de la fenêtre {screen:?} (rognage {clip:?})"
            );
        }
    }

    /// La barre déportée au bord de la fenêtre suit le défilement de la
    /// table (molette sur la table) : sa poignée descend.
    #[test]
    fn la_barre_verticale_de_la_grille_suit_la_molette_sur_la_table() {
        let (screen, repos) = peindre_classeur_large(Vec::new());
        let avant = poignees_de_barre(screen, &repos);
        let (_, defile) = peindre_classeur_large(vec![
            egui::Event::PointerMoved(egui::pos2(100.0, 150.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let apres = poignees_de_barre(screen, &defile);
        assert_eq!(avant.len(), 1, "une poignée au repos : {avant:?}");
        assert_eq!(apres.len(), 1, "une poignée après défilement : {apres:?}");
        assert!(
            apres[0].0.top() > avant[0].0.top() + 1.0,
            "la poignée doit descendre : {:?} -> {:?}",
            avant[0].0,
            apres[0].0
        );
    }

    /// l'écran, en-tête figé et onglets de feuille compris.
    #[test]
    fn la_barre_verticale_de_la_grille_couvre_toute_la_hauteur_de_la_fenetre() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let largeur_barre = theme::scroll_bar_allocated_width();
        let pistes: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.width() <= largeur_barre
                    && rect.left() >= screen.right() - largeur_barre - 0.5
                    && rect.height() >= screen.height() - largeur_barre
            })
            .collect();
        assert!(
            !pistes.is_empty(),
            "une piste de barre verticale sur toute la hauteur de {screen:?} ; \
             rectangles peints : {rectangles:?}"
        );
    }

    /// La barre horizontale de la grille est déportée **sous** les onglets de
    /// feuille : sa piste occupe le bas de la fenêtre, sur presque toute sa
    /// largeur (moins la bande de la barre verticale).
    #[test]
    fn la_barre_horizontale_de_la_grille_est_sous_les_onglets() {
        let (screen, rectangles) = peindre_classeur_large(Vec::new());
        let hauteur_barre = theme::scroll_bar_allocated_width();
        let pistes: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.height() <= hauteur_barre
                    && rect.top() >= screen.bottom() - hauteur_barre - 0.5
                    && rect.width() >= screen.width() - 2.0 * hauteur_barre
            })
            .collect();
        assert!(
            !pistes.is_empty(),
            "une piste de barre horizontale en bas de {screen:?}, sous les onglets ; \
             rectangles peints : {rectangles:?}"
        );
    }

    #[test]
    fn clic_droit_dans_le_vide_de_grille_et_sur_les_onglets_propose_les_ajouts() {
        let mut project = Project::new("Menus").unwrap();
        let book = project.add_workbook(project.root(), None).unwrap();
        let sheet = project.tree().node(book).unwrap().children()[0];
        project
            .add_column(sheet, None, None, Column::from_f64(&[1.0]))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();

        for (on_tabs, label) in [
            (false, tr!("workbook-column-add")),
            (true, tr!("workbook-worksheet-add")),
        ] {
            let context = egui::Context::default();
            let mut state =
                AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            let mut actions = Vec::new();
            {
                let mut paint = |events: Vec<egui::Event>| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600.0, 500.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        book_contents(ui, &mut state, &mut actions, book)
                    });
                    output.textures_delta.clear();
                    output
                };
                let first = paint(Vec::new());
                let tab_is_visible = first.shapes.iter().any(|painted| match &painted.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == "Sheet1" => painted
                        .clip_rect
                        .contains(shape.pos + shape.galley.rect.center().to_vec2()),
                    _ => false,
                });
                assert!(
                    tab_is_visible,
                    "la bande d'onglets doit rester dans la fenêtre"
                );
                let pos = if on_tabs {
                    centre_du_texte(&first.shapes, "Sheet1")
                } else {
                    egui::pos2(450.0, 180.0)
                };
                paint(vec![egui::Event::PointerMoved(pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                // Le clic droit désigne déjà la cible : l'ajout est direct,
                // sans sous-menu « Sheet ▸ » ni « Columns ▸ » (#152).
                let output = paint(Vec::new());
                assert!(
                    output.shapes.iter().any(|painted| match &painted.shape {
                        egui::Shape::Text(shape) => shape.galley.job.text == *label,
                        _ => false,
                    }),
                    "le menu contextuel doit proposer {label} : {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|painted| match &painted.shape {
                            egui::Shape::Text(shape) => Some(shape.galley.job.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                );
                let menu_pos = centre_du_texte(&output.shapes, &label);
                paint(vec![egui::Event::PointerMoved(menu_pos)]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                paint(vec![egui::Event::PointerButton {
                    pos: menu_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            if on_tabs {
                assert!(
                    matches!(actions.as_slice(), [Action::AddWorksheet { book: id }] if *id == book)
                );
            } else {
                assert!(
                    matches!(actions.as_slice(), [Action::AddColumn { sheet: id }] if *id == sheet)
                );
            }
        }
    }

    /// #51 : la bande de la barre horizontale n'est réservée que si la grille
    /// déborde en largeur. Une seule colonne étroite : aucune piste au bas de
    /// la fenêtre (témoin de `la_barre_horizontale_de_la_grille_est_sous_les_onglets`,
    /// dont le filtre de rectangles pourrait sinon attraper n'importe quoi).
    #[test]
    fn sans_debordement_aucune_barre_horizontale_n_est_reservee() {
        let (screen, rectangles) = peindre_classeur(1, Vec::new());
        let hauteur_barre = theme::scroll_bar_allocated_width();
        let pistes: Vec<_> = rectangles
            .iter()
            .filter(|(rect, _)| {
                rect.height() <= hauteur_barre
                    && rect.top() >= screen.bottom() - hauteur_barre - 0.5
                    && rect.width() >= screen.width() - 2.0 * hauteur_barre
            })
            .collect();
        assert!(
            pistes.is_empty(),
            "une colonne tient dans {screen:?} : pas de piste horizontale ; {pistes:?}"
        );
    }

    /// Deux feuilles : leurs onglets sont peints sous la grille (sous la
    /// première ligne de données), et un clic sur le second affiche sa
    /// feuille.
    #[test]
    fn les_onglets_de_feuille_sont_sous_la_grille_et_choisissent_la_feuille() {
        let mut project = Project::new("Feuilles").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let premiere = project.add_worksheet(book, None).unwrap();
        let seconde = project.add_worksheet(book, None).unwrap();
        for sheet in [premiere, seconde] {
            project
                .add_column(sheet, None, None, Column::from_f64(&[42.0]))
                .unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let noms: Vec<String> = state
            .worksheets_of(book)
            .into_iter()
            .map(|(_, nom)| nom)
            .collect();
        assert_eq!(noms.len(), 2);
        assert_eq!(state.selected_sheet_in(book), Some(premiere));
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&context);
        let mut actions = Vec::new();
        let mut paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                book_contents(ui, &mut state, &mut actions, book);
            });
            output.textures_delta.clear();
            output
        };
        paint(Vec::new());
        let output = paint(Vec::new());
        let donnee = centre_du_texte(&output.shapes, "42");
        for nom in &noms {
            let onglet = centre_du_texte(&output.shapes, nom);
            assert!(
                onglet.y > donnee.y,
                "l'onglet {nom:?} ({onglet:?}) doit être sous la grille ({donnee:?})"
            );
        }
        let seconde_pos = centre_du_texte(&output.shapes, &noms[1]);
        cliquer(&mut paint, seconde_pos);
        assert_eq!(
            state.selected_sheet_in(book),
            Some(seconde),
            "le clic sur l'onglet affiche sa feuille"
        );
    }

    /// Menu Classeur : « Ajouter une feuille » pousse son action ;
    /// « Supprimer la feuille » est grisé quand le classeur n'a qu'une
    /// feuille (un clic n'y pousse rien), actif sinon.
    #[test]
    fn le_menu_classeur_grise_la_suppression_de_la_derniere_feuille() {
        for feuilles in [1, 2] {
            let mut project = Project::new("Menu").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let mut sheets = Vec::new();
            for _ in 0..feuilles {
                sheets.push(project.add_worksheet(book, None).unwrap());
            }
            let sheet = sheets[0];
            let session = Session::spawn(project, None).unwrap();
            let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut actions = Vec::new();
            let mut paint = |events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let ctx = ui.ctx().clone();
                    sheet_menu(ui, &ctx, &state, book, sheet, &mut actions);
                });
                output.textures_delta.clear();
                output
            };
            let output = paint(Vec::new());
            let supprimer = centre_du_texte(&output.shapes, &tr!("action-delete-sheet"));
            let ajouter = centre_du_texte(&output.shapes, &tr!("workbook-worksheet-add"));
            cliquer(&mut paint, supprimer);
            cliquer(&mut paint, ajouter);
            let suppression = actions
                .iter()
                .any(|action| matches!(action, Action::RequestDeleteSheet(s) if *s == sheet));
            assert_eq!(
                suppression,
                feuilles > 1,
                "{feuilles} feuille(s) : suppression demandée = {suppression} ({actions:?})"
            );
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, Action::AddWorksheet { book: b } if *b == book)),
                "{actions:?}"
            );
        }
    }
}
