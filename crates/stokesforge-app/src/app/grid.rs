//! La grille d'une feuille : table virtualisée ([`grid_contents`]), en-tête
//! empilé (lettre, nom long, unité, commentaire, formule, métadonnées),
//! gouttière des numéros de ligne, cellules, menu d'édition commun à toutes
//! ses zones ([`grid_edit_menu`]), focus clavier (Tab, Maj+Tab et les flèches
//! appartiennent à la grille, #38 : [`GRID_KEYS`]), barres de défilement
//! déportées et prise du glisser de colonnes vers un graphe, par le bord d'un
//! bloc de colonnes sélectionnées ([`column_grip_bands`], #160).
//!
//! Sorti d'`app.rs` tel quel (#120). Le classeur qui l'entoure (onglets de
//! feuille, menu Classeur) vit dans `workbook.rs`. Les autres modules ne lisent
//! d'ici que [`keyboard_focus_outside_grid`] (`app.rs` pour les raccourcis
//! globaux, `dialogs.rs`, `graph_window.rs`) et la visibilité de la ligne
//! « Metadata » (`workbook.rs`) ; [`role_label`] sert aussi au test du classeur
//! détaché (`window_detach.rs`).
//!
//! Les tests sont en fin de fichier : ils peignent le classeur sans fenêtre,
//! injectent clavier et souris, et lisent ce qui est peint (textes,
//! positions) ou les actions poussées, jamais le texte source.

use eframe::egui;
use egui_extras::{Column as TableColumn, TableBuilder};
use stokesforge_core::{ColumnRole, ColumnType, NodeId};
use stokesforge_plot::PlotType;

use crate::selection::{GridCursor, Selection};
use crate::state::{
    Action, AppState, ColumnDragPayload, GridMenu, HeaderField, MoveDirection, RowDragPayload,
};
use crate::{icons, tr};

use super::graph_targets::{send_rows_to_graph_menu, send_to_graph_menu};
use super::menu_sections::MenuSections;
use super::shortcuts::{
    shortcut_text_backspace, shortcut_text_delete, SHORTCUT_COPY, SHORTCUT_COPY_FULL_COLUMNS,
    SHORTCUT_CUT, SHORTCUT_EDIT_CELL, SHORTCUT_PASTE, SHORTCUT_PASTE_ABSOLUTE, SHORTCUT_SELECT_ALL,
};

/// Hauteur d'une ligne de la grille, en points.
const ROW_HEIGHT: f32 = 20.0;
/// Largeur par défaut d'une colonne de données dans la grille docquée. 54px
/// (plutôt que les 110px historiques) : demande explicite, pour loger
/// davantage de colonnes à l'écran — l'utilisateur peut toujours
/// redimensionner à la souris (`.resizable(true)`).
const DEFAULT_COLUMN_WIDTH: f32 = 54.0;
/// Largeur minimale d'une colonne de données après redimensionnement manuel.
/// Sous l'ancien plancher (56px) pour ne pas empêcher un retour à
/// [`DEFAULT_COLUMN_WIDTH`] une fois qu'on l'a élargie puis rétrécie.
const MIN_COLUMN_WIDTH: f32 = 28.0;
/// Largeur par défaut de la gouttière des numéros de ligne dans la grille
/// docquée — élargie (l'ancienne valeur, 56px, coupait déjà les libellés
/// d'en-tête « Long Name »/« Comments ») et redimensionnable comme les
/// colonnes de données plutôt que figée (`TableColumn::exact`).
const ROW_NUMBER_COLUMN_WIDTH: f32 = 90.0;
/// Largeur minimale de la gouttière des numéros de ligne après
/// redimensionnement manuel — assez pour garder `#` et quelques chiffres
/// lisibles.
const MIN_ROW_NUMBER_COLUMN_WIDTH: f32 = 32.0;
/// Hauteur d'une rangée secondaire de l'en-tête empilé (lettre, unité,
/// commentaire, formule).
const HEADER_SUBROW_HEIGHT: f32 = 18.0;
/// Hauteur de la rangée du nom long, légèrement plus haute — c'est le nom
/// principal de la colonne.
const HEADER_LONGNAME_HEIGHT: f32 = 20.0;
/// Hauteur totale de l'en-tête empilé de la grille docquée (lettre, nom
/// x', nom long, unité, commentaire, formule, métadonnées).
const HEADER_HEIGHT_STACKED: f32 = HEADER_LONGNAME_HEIGHT + 6.0 * HEADER_SUBROW_HEIGHT;

/// Hauteur de l'en-tête empilé selon que la ligne « Metadata » est affichée.
fn stacked_header_height(show_metadata: bool) -> f32 {
    if show_metadata {
        HEADER_HEIGHT_STACKED
    } else {
        HEADER_HEIGHT_STACKED - HEADER_SUBROW_HEIGHT
    }
}

/// La ligne « Metadata » de l'en-tête est-elle affichée ? Réglage de vue,
/// gardé dans la mémoire d'egui (un seul `Context` pour tous les viewports :
/// les classeurs flottants et détachés le suivent ensemble), jamais dans le
/// modèle ni l'historique. Affichée par défaut.
pub(super) fn metadata_row_visible(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::new("stokesforge_show_metadata_row")))
        .unwrap_or(true)
}

pub(super) fn set_metadata_row_visible(ctx: &egui::Context, visible: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("stokesforge_show_metadata_row"), visible);
    });
}

/// Demi-largeur, en points, de la bande de prise du glisser de colonnes : elle
/// déborde de part et d'autre du bord vertical d'un bloc de colonnes
/// sélectionnées (#160).
const COLUMN_GRIP_HALF_WIDTH: f32 = 3.0;

/// Étendue horizontale d'une colonne de la grille : `(colonne, gauche, droite)`.
type ColumnExtent = (NodeId, f32, f32);

/// Étendue verticale d'une ligne **peinte** de la grille : `(ligne, haut, bas)`.
/// La table ne peint que les lignes visibles (virtualisation).
type RowExtent = (usize, f32, f32);

/// Demi-hauteur, en points, de la bande de prise du glisser de lignes (#166).
const ROW_GRIP_HALF_HEIGHT: f32 = 3.0;

/// Bandes de prise du glisser de lignes vers un graphe (#166) : une au-dessus
/// et une au-dessous de chaque **bloc** de lignes sélectionnées qui se
/// suivent, sur son bord extérieur, de `left` à `right` — la gouttière des
/// numéros de ligne, qui sélectionne au clic, en est exclue par l'appelant.
/// Les blocs se lisent sur `selected` (croissant), pas sur ce qui est peint :
/// un bord n'existe que si sa ligne est visible ET qu'elle est vraiment le
/// bord du bloc, jamais parce que le bloc continue hors de l'écran. Les
/// bandes restent dans `top..bottom` (le corps de la table, sans l'en-tête).
fn row_grip_bands(
    extents: &[RowExtent],
    selected: &[usize],
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> Vec<egui::Rect> {
    let mut bands = Vec::new();
    let mut index = 0;
    while index < selected.len() {
        let first = selected[index];
        while index + 1 < selected.len() && selected[index + 1] == selected[index] + 1 {
            index += 1;
        }
        let last = selected[index];
        index += 1;
        let edges = [
            extents.iter().find(|e| e.0 == first).map(|e| e.1),
            extents.iter().find(|e| e.0 == last).map(|e| e.2),
        ];
        for y in edges.into_iter().flatten() {
            let band = egui::Rect::from_min_max(
                egui::pos2(left, (y - ROW_GRIP_HALF_HEIGHT).max(top)),
                egui::pos2(right, (y + ROW_GRIP_HALF_HEIGHT).min(bottom)),
            );
            if band.height() > 0.0 {
                bands.push(band);
            }
        }
    }
    bands
}

/// Bandes de prise du glisser de colonnes vers un graphe (#160) : une de part
/// et d'autre de chaque **bloc** de colonnes sélectionnées qui se suivent,
/// sur son bord extérieur. Elles couvrent la hauteur `top..bottom` — la
/// rangée des lettres, où les bords de colonne servent au redimensionnement,
/// en est exclue par l'appelant. Fonction pure : testable sans fenêtre, comme
/// le reste des décisions de ce module.
fn column_grip_bands(
    extents: &[ColumnExtent],
    selected: &[NodeId],
    top: f32,
    bottom: f32,
) -> Vec<egui::Rect> {
    let mut bands = Vec::new();
    let mut index = 0;
    while index < extents.len() {
        if !selected.contains(&extents[index].0) {
            index += 1;
            continue;
        }
        let first = index;
        while index + 1 < extents.len() && selected.contains(&extents[index + 1].0) {
            index += 1;
        }
        for x in [extents[first].1, extents[index].2] {
            bands.push(egui::Rect::from_min_max(
                egui::pos2(x - COLUMN_GRIP_HALF_WIDTH, top),
                egui::pos2(x + COLUMN_GRIP_HALF_WIDTH, bottom),
            ));
        }
        index += 1;
    }
    bands
}

/// Zone de défilement horizontale de la grille. Sa propre barre est masquée :
/// la barre visible est déportée **sous les onglets de feuille**
/// ([`show_grid_horizontal_bar`]), comme dans un tableur. Le défilement
/// (molette avec Maj, glisser) reste actif ; un décalage demandé par la barre
/// déportée à l'image précédente (`pending_offset`) est appliqué ici.
pub(super) fn show_horizontally_scrollable_grid<R>(
    ui: &mut egui::Ui,
    pending_offset: Option<f32>,
    add_grid: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let mut area = egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_offset {
        area = area.horizontal_scroll_offset(offset);
    }
    area.show(ui, add_grid)
}

/// Barre de défilement horizontale de la grille, déportée dans la bande
/// réservée **sous les onglets de feuille** (`strip`). Même principe que
/// [`show_grid_vertical_bar`] : une `ScrollArea` horizontale vide, de la
/// largeur de contenu de la grille, qui reflète son décalage ; un glisser sur
/// cette barre est mis en attente (`pending_id`) et appliqué à la grille à
/// l'image suivante ([`show_horizontally_scrollable_grid`]). La bande a la
/// largeur de la grille : pas de mise à l'échelle.
pub(super) fn show_grid_horizontal_bar<R>(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    grid: &egui::scroll_area::ScrollAreaOutput<R>,
) {
    if !strip.is_positive() {
        return;
    }
    let grid_offset = grid.state.offset.x;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    let bar = egui::ScrollArea::horizontal()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .horizontal_scroll_offset(grid_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(egui::vec2(grid.content_size.x, 0.0), egui::Sense::hover());
        });
    let dragged_offset = bar.state.offset.x;
    if (dragged_offset - grid_offset).abs() > 0.5 {
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, dragged_offset));
        ui.ctx().request_repaint();
    }
}

/// Rapport d'échelle entre la barre verticale de la grille et le corps de
/// la table qu'elle pilote : la barre couvre toute la hauteur de la fenêtre
/// (`bar_height`), le corps défilant seulement la sienne (`body_height`, sous
/// l'en-tête figé). Contenu et décalage de la barre sont ceux de la table
/// multipliés par ce rapport : la poignée garde ainsi la même proportion
/// (visible / total) et la même position relative que si elle ne couvrait
/// que le corps. `1.0` quand le corps n'a pas de hauteur.
fn grid_vertical_bar_scale(bar_height: f32, body_height: f32) -> f32 {
    if body_height > 0.0 {
        bar_height / body_height
    } else {
        1.0
    }
}

/// Barre de défilement verticale de la grille (B10), collée au bord droit de
/// la fenêtre plutôt qu'au bord droit de la table, et sur toute la hauteur
/// de la fenêtre (`strip`, en-tête et onglets de feuille compris).
///
/// La table (`TableBuilder`) défile verticalement elle-même — c'est ce qui
/// fige son en-tête — mais, dessinée dans la zone de défilement horizontale,
/// sa propre barre suivrait la largeur de ses colonnes. Sa barre est donc
/// masquée, et celle-ci en est le miroir : une `ScrollArea` verticale vide,
/// posée dans la bande réservée, dont la hauteur de contenu est celle du
/// corps de la table mise à l'échelle de la barre
/// ([`grid_vertical_bar_scale`]). Le décalage suit la table (molette,
/// glisser dans la table) ; inversement, un glisser sur cette barre est mis
/// en attente dans la mémoire (`pending_id`), ramené à l'échelle de la table,
/// et appliqué à celle-ci à l'image suivante (`grid_contents`,
/// `pending_vertical_offset`).
pub(super) fn show_grid_vertical_bar(
    ui: &mut egui::Ui,
    pending_id: egui::Id,
    strip: egui::Rect,
    table: &egui::scroll_area::ScrollAreaOutput<()>,
) {
    if !strip.is_positive() {
        return;
    }
    let scale = grid_vertical_bar_scale(strip.height(), table.inner_rect.height());
    let bar_offset = table.state.offset.y * scale;
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
    let bar = egui::ScrollArea::vertical()
        .id_salt(pending_id)
        .auto_shrink([false, false])
        .vertical_scroll_offset(bar_offset)
        .show(&mut bar_ui, |ui| {
            ui.allocate_exact_size(
                egui::vec2(0.0, table.content_size.y * scale),
                egui::Sense::hover(),
            );
        });
    let dragged_offset = bar.state.offset.y;
    if (dragged_offset - bar_offset).abs() > 0.5 {
        let table_offset = dragged_offset / scale;
        ui.memory_mut(|memory| memory.data.insert_temp(pending_id, table_offset));
        ui.ctx().request_repaint();
    }
}

/// Un pictogramme suivi d'un nom (un nom de dossier ou de fenêtre, choisi par
/// l'utilisateur : jamais traduit, R15).
fn icon_label(icon: &str, name: &str) -> String {
    format!("{icon} {name}")
}

/// Nom d'un type de colonne, tel que l'offre le menu de la rangée des lettres.
fn type_label(column_type: ColumnType) -> String {
    match column_type {
        ColumnType::Numeric => tr!("column-type-numeric"),
        ColumnType::DateTime => tr!("column-type-datetime"),
        ColumnType::Mixed => tr!("column-type-mixed"),
    }
}

/// Nom du rôle, affiché entre parenthèses après la lettre de colonne pour
/// rappeler son rôle de tracé et dans le sous-menu « Role » ([`stokesforge_core::ColumnRole`]) — `A(x)`, `B(y)`… Ce rôle
/// n'est pour l'instant lu par rien d'autre que la persistance
/// (`stokesforge-persistence`) : aucune résolution de série ne s'en sert encore
/// (voir `stokesforge_core::resolve`, qui détermine X/Y autrement) — l'afficher et
/// le rendre modifiable ici n'a donc, pour l'instant, qu'une valeur
/// informative, en avance sur son câblage au rendu.
pub(super) fn role_label(role: ColumnRole) -> String {
    match role {
        ColumnRole::X => tr!("workbook-role-x"),
        ColumnRole::Y => tr!("workbook-role-y"),
        ColumnRole::X2 => tr!("workbook-role-x2"),
        ColumnRole::Y2 => tr!("workbook-role-y2"),
        ColumnRole::Z => tr!("workbook-role-z"),
        ColumnRole::XError => tr!("column-role-x-error"),
        ColumnRole::YError => tr!("column-role-y-error"),
        ColumnRole::Label => tr!("workbook-role-label"),
        ColumnRole::None => tr!("workbook-role-none"),
    }
}

/// Lettre de colonne façon tableur, en base 26 : `0 → "A"`, `25 → "Z"`,
/// `26 → "AA"`, `701 → "ZZ"`… — l'ordre de la feuille, pas un nom stocké
/// dans le modèle.
///
/// Simple alias de [`stokesforge_core::spreadsheet_name`], qui portait déjà cet
/// algorithme (elle nomme les colonnes créées sans nom explicite). Ce module
/// en avait sa propre copie, de sorte que la lettre affichée en en-tête et
/// celle qui sert de nom par défaut à une colonne neuve venaient de deux
/// implémentations distinctes — et, depuis que les libellés d'historique
/// désignent eux aussi une colonne par sa lettre
/// (`stokesforge_commands::Command::label`), il en aurait fallu une troisième.
fn column_letter(index: usize) -> String {
    stokesforge_core::spreadsheet_name(index)
}

/// Vrai si `column` est la dernière colonne visible de `sheet` — condition
/// commune aux deux endroits où Tab doit ajouter une colonne plutôt que
/// revenir en début de ligne suivante (voir `Action::AddColumnAndSelect`) :
/// le raccourci clavier global de `grid_contents` et la validation d'édition
/// de `cell_widget`.
fn is_last_visible_column(state: &AppState, sheet: NodeId, column: NodeId) -> bool {
    state
        .visible_columns(sheet)
        .last()
        .is_some_and(|(last, ..)| *last == column)
}

/// Contenu du presse-papier **système**, lu à la demande — pour « Coller »
/// pris dans un menu contextuel, qui n'a pas d'`egui::Event::Paste` à se
/// mettre sous la dent.
///
/// Ctrl+V fonctionne sans ça parce qu'`egui-winit` lit lui-même le
/// presse-papier et pousse le texte dans l'événement ; egui, lui, n'expose
/// aucune API de **lecture** (seulement `Context::copy_text`, en écriture).
/// D'où cette lecture directe par `arboard` — la bibliothèque qu'`egui-winit`
/// utilise déjà en interne, donc déjà présente dans l'arbre de dépendances.
///
/// Une nouvelle poignée à chaque appel, plutôt qu'une gardée en vie : c'est
/// l'**écriture** qui exige sous X11 de garder le processus propriétaire du
/// presse-papier vivant, jamais la lecture, qui est un aller-retour unique.
/// Et un menu s'ouvre trop rarement pour que ce coût compte — raison pour
/// laquelle « Coller » n'est d'ailleurs jamais grisé : savoir s'il y a
/// quelque chose à coller demanderait cette lecture à *chaque image* où le
/// menu est ouvert. On tente, et on le dit si ça ne donne rien.
///
/// `None` si le presse-papier est inaccessible (pas de serveur d'affichage,
/// portail refusé…) ou s'il ne contient pas de texte (une image, par
/// exemple).
fn system_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Bloc d'édition commun à tous les menus contextuels de la grille : les
/// raccourcis clavier de [`grid_contents`], contextualisés à ce que la
/// sélection courante touche réellement (voir [`AppState::grid_menu`] et
/// [`GridMenu`]).
///
/// Le même bloc, dans le même ordre, sur les trois zones — cellule de
/// données, champ d'en-tête, rangée des lettres : seuls les libellés
/// changent, et « Supprimer et décaler » disparaît dans l'en-tête, où Suppr
/// n'a rien à faire remonter. Le menu de la rangée des lettres y ajoute
/// ensuite ce qui lui est propre (type et rôle de colonne, voir
/// [`column_header_widget`]).
///
/// N'ouvre rien si [`AppState::grid_menu`] rend `None` (plus de sélection
/// valide) : l'appelant doit alors s'abstenir de poser un menu, plutôt que
/// d'en poser un vide.
fn grid_edit_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    let mut sections = MenuSections::new();
    grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
    grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
}

/// Presse-papiers de la grille, **un seul groupe** (#152) : Cut · Copy ·
/// Copy with Header · Paste · Paste in Place. Pas d'Undo/Redo au clic droit :
/// Edit et le clavier suffisent.
fn grid_clipboard_section(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    let ctx = ui.ctx().clone();
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                menu.copy_enabled,
                egui::Button::new(tr!("action-cut"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_CUT)),
            )
            .on_disabled_hover_text(tr!("action-copy", "disabled"))
            .clicked()
        {
            // Couper = copier puis vider, exactement comme le bloc clavier :
            // `copy_selection` pose le presse-papier interne *et* rend le texte
            // à pousser au presse-papier système.
            if let Some(text) = state.copy_selection(sheet) {
                ctx.copy_text(text);
            }
            state.forget_delete_repeat(sheet);
            actions.push(Action::ClearSelectedCells { sheet });
            ui.close();
        }
        if ui
            .add_enabled(
                menu.copy_enabled,
                egui::Button::new(tr!("action-copy"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY)),
            )
            .on_disabled_hover_text(tr!("action-copy", "disabled"))
            .clicked()
        {
            if let Some(text) = state.copy_selection(sheet) {
                ctx.copy_text(text);
            }
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-copy-with-header"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_COPY_FULL_COLUMNS)),
            )
            .on_hover_text(tr!("action-copy-with-header", "hint"))
            .clicked()
        {
            if let Some(text) = state.copy_full_columns(sheet) {
                ctx.copy_text(text);
            }
            ui.close();
        }
        if ui
            .add_enabled(
                menu.paste_enabled,
                egui::Button::new(tr!("action-paste"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE)),
            )
            .on_disabled_hover_text(tr!("action-paste", "disabled"))
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            match system_clipboard_text() {
                Some(text) => actions.push(Action::Paste { sheet, text }),
                None => state.notice(tr!("status-clipboard-unreadable")),
            }
            ui.close();
        }
        if ui
            .add_enabled(
                menu.paste_enabled && menu.paste_absolute_enabled,
                egui::Button::new(tr!("action-paste-in-place"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_PASTE_ABSOLUTE)),
            )
            .on_hover_text(tr!("action-paste-in-place", "hint"))
            .on_disabled_hover_text(if menu.paste_enabled {
                tr!("action-paste-in-place", "disabled")
            } else {
                tr!("action-paste", "disabled")
            })
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            actions.push(Action::PasteAbsolute { sheet });
            ui.close();
        }
    });
}

/// Effacer et supprimer : Clear · Delete (Shift Up).
fn grid_erase_section(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    menu: &GridMenu,
) {
    sections.section(ui, true, |ui| {
        if ui
            .add(egui::Button::new(tr!("action-clear")).shortcut_text(shortcut_text_backspace()))
            .on_hover_text(tr!("action-clear", "hint"))
            .clicked()
        {
            state.forget_delete_repeat(sheet);
            actions.push(Action::ClearSelectedCells { sheet });
            ui.close();
        }
        if menu.offers_delete_shift
            && ui
                .add(
                    egui::Button::new(tr!("action-delete-shift-up"))
                        .shortcut_text(shortcut_text_delete()),
                )
                .on_hover_text(tr!("action-delete-shift-up", "hint"))
                .clicked()
        {
            // Passer par l'action directement, sans `consume_delete_key` : un
            // menu contextuel est un geste explicite et désigné, là où la
            // double pression sur Suppr n'existe que pour protéger d'une frappe
            // réflexe. Le compteur en cours est quand même oublié, pour qu'une
            // pression sur Suppr juste après ne parte pas du mauvais temps.
            state.forget_delete_repeat(sheet);
            actions.push(Action::DeleteSelectedCells { sheet });
            ui.close();
        }
    });
}

/// En-tête empilée d'une colonne : lettre (clic → sélectionne la colonne
/// entière), puis nom long, unité, commentaire et formule — les quatre
/// champs de [`stokesforge_core::ColumnMeta`], jusqu'ici stockés sans jamais être
/// montrés côté UI. Chacun bascule affichage/édition au double-clic, comme
/// [`cell_widget`].
///
/// La rangée des lettres porte un fond distinct
/// ([`crate::theme::header_letter_row_fill`]) — elle seule sélectionne la
/// colonne au clic, les quatre rangées de champs qui suivent n'affichent
/// plus qu'un libellé grisé répété par colonne (leur nom de champ vit une
/// seule fois dans la gouttière des numéros de ligne, voir
/// [`row_number_header_widget`]) : elles n'affichent que la **valeur** du
/// champ, vide si le champ ne l'est pas.
fn column_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    column_index: usize,
) {
    ui.vertical(|ui| {
        // Sans ceci, l'espacement par défaut d'egui entre enfants d'un
        // `vertical` (`Spacing::item_spacing.y`) laisse un mince filet blanc
        // entre chaque rangée empilée — la couleur de fond ne remplit alors
        // pas vraiment la case, elle y flotte.
        ui.spacing_mut().item_spacing.y = 0.0;
        let is_column_selected =
            column_letter_widget(ui, state, actions, sheet, column, column_index);
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::XPrime,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        header_field_widget(
            ui,
            state,
            actions,
            sheet,
            column,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            is_column_selected,
        );
        if metadata_row_visible(ui.ctx()) {
            metadata_cell_widget(ui, state, actions, column);
        }
    });
}

/// Rangée des lettres : sélection de colonnes, puis menu sur la sélection.
fn column_letter_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    column_index: usize,
) -> bool {
    let letter = column_letter(column_index);
    let is_column_selected = state.is_column_selected(sheet, column);
    let touched = state.column_touched_by_selection(sheet, column);
    let role = state.column_meta(column).role;
    ui.scope(|ui| {
        ui.set_min_height(HEADER_SUBROW_HEIGHT);
        ui.set_max_height(HEADER_SUBROW_HEIGHT);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, 0.0, gutter_fill(is_column_selected, touched));
        header_row_separator(ui, rect);
        // Toute la largeur de la rangée sélectionne la colonne — pas
        // seulement la lettre —, glisser ou Maj-clic étend à plusieurs
        // colonnes (même logique que la multi-sélection de cellules de
        // données, voir `cell_widget`), au clic principal comme au clic
        // secondaire (`response.secondary_clicked()` couvre aussi
        // `Sense::click_and_drag()`) : le clic droit doit pouvoir
        // sélectionner *et* ouvrir le menu contextuel en un seul geste,
        // sans exiger que la colonne soit déjà sélectionnée au
        // préalable.
        let id = ui
            .id()
            .with(("stokesforge_column_letter_row", column.raw()));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        let letter_text = format!("{letter}({})", role_label(role));
        // Colonne d'un calcul pas à jour (#181, M9 a) : lettre teintée, la
        // raison dans l'info-bulle.
        let stale_text = state.column_stale_text(column);
        let letter_color = if stale_text.is_some() {
            crate::theme::stale_color(ui.visuals().dark_mode)
        } else {
            crate::theme::gutter_text_color()
        };
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(letter_text).color(letter_color))
                .selectable(false),
        );
        if let Some(text) = stale_text {
            response.clone().on_hover_text(text);
        }
        let modifiers = ui.input(|i| i.modifiers);
        let press_here = press_started_in(ui, rect);
        if response.clicked() || response.drag_started() {
            // Ctrl+clic bascule la colonne, Maj+clic prend la plage ancre →
            // colonne, Ctrl+Maj+clic l'ajoute à la sélection (#159).
            match (modifiers.command, modifiers.shift) {
                (true, true) => state.extend_column_selection(sheet, column),
                (true, false) => state.toggle_column_selection(sheet, column),
                (false, true) => state.extend_column_selection_replacing(sheet, column),
                (false, false) => state.select_column(sheet, column),
            }
        } else if response.secondary_clicked() && !is_column_selected {
            // `&& !is_column_selected` : un clic droit *dans* la sélection
            // (même non contiguë) la laisse intacte et le menu agit sur
            // toute la sélection ; hors de la sélection, il la remplace par
            // la colonne visée (#152, #159).
            state.select_column(sheet, column);
        } else if response.contains_pointer()
            && !press_here
            && !grid_drag_in_progress(ui.ctx())
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // Glisser parti d'une autre lettre : étendre au survol. Un appui
            // commencé sur cette lettre ne relève pas d'ici — sinon un
            // Ctrl+clic, qui bascule au relâchement, aurait déjà ajouté la
            // plage ancre → colonne pendant l'appui.
            state.extend_column_selection(sheet, column);
        }
        // Colonnes visées par le menu contextuel : toute la plage
        // sélectionnée si `column` en fait partie (menu ouvert sur une
        // sélection multiple déjà posée), sinon `column` seule — un
        // clic droit hors de la plage sélectionnée l'a déjà réduite à
        // elle-même ci-dessus.
        let target_columns: Vec<NodeId> = state
            .selection(sheet)
            .map(|selection| selection.columns(state.tree(), sheet))
            .filter(|columns| columns.contains(&column))
            .unwrap_or_else(|| vec![column]);
        response.context_menu(|ui| {
            column_properties_menu(ui, state, actions, sheet, &target_columns);
        });
    });
    is_column_selected
}

/// Menu de la rangée des lettres (#152) : tracer et ajouter, presse-papiers,
/// type et rôle des colonnes, puis effacer — à plat, sans enveloppe
/// « Columns ▸ ». Calcul paresseux de [`AppState::grid_menu`], pour la même
/// raison que dans `cell_widget`.
fn column_properties_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    let menu = state.grid_menu(sheet);
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        plot_menu(ui, actions, sheet, target_columns);
        send_to_graph_menu(ui, state.tree(), actions, sheet, target_columns);
        add_column_entry(ui, sheet, actions);
        super::analysis_menu::analysis_submenu(
            ui,
            state,
            actions,
            super::analysis_menu::AnalysisTarget {
                sheet,
                columns: target_columns,
            },
        );
    });
    if let Some(menu) = &menu {
        grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
    }
    sections.section(ui, true, |ui| {
        column_type_role_menus(ui, state, actions, target_columns);
    });
    super::analysis_menu::calculation_section(ui, &mut sections, state, actions, target_columns);
    if let Some(menu) = &menu {
        grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
    }
}

/// Menu d'une sélection de **lignes entières** d'une feuille à x' (#166) :
/// Plot et Send to tracent les lignes (une série par ligne, contre les x'),
/// puis presse-papiers et effacer comme pour les colonnes.
fn row_properties_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    payload: &RowDragPayload,
) {
    let sheet = payload.sheet;
    let menu = state.grid_menu(sheet);
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        plot_rows_menu(ui, actions, payload);
        send_rows_to_graph_menu(ui, state.tree(), actions, payload);
    });
    if let Some(menu) = &menu {
        grid_clipboard_section(ui, &mut sections, state, actions, sheet, menu);
        grid_erase_section(ui, &mut sections, state, actions, sheet, menu);
    }
}

/// « Plot » d'une sélection de lignes : les mêmes types de tracé que
/// [`plot_menu`], poussant [`Action::PlotRows`].
pub(super) fn plot_rows_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    payload: &RowDragPayload,
) {
    ui.menu_button(tr!("workbook-plot-menu"), |ui| {
        for (label, plot_type) in [
            (tr!("plot-type-line"), PlotType::Line),
            (tr!("plot-type-scatter"), PlotType::Scatter),
            (tr!("plot-type-line-symbol"), PlotType::LineScatter),
            (tr!("plot-type-bar"), PlotType::Bar),
        ] {
            if ui.button(label).clicked() {
                actions.push(Action::PlotRows {
                    sheet: payload.sheet,
                    rows: payload.rows.clone(),
                    plot_type,
                });
                ui.close();
            }
        }
    });
}

/// Le clic droit sur une case de la grille (cellule, numéro de ligne, champ
/// d'en-tête, coin). Sur une sélection de **colonnes ou de lignes entières**,
/// c'est le menu des lettres de colonnes — Plot, Send to, presse-papiers, type
/// et rôle, effacer — appliqué aux colonnes de la sélection ; sur une plage de
/// cellules ou de champs, presse-papiers et effacer seulement.
fn selection_context_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    let whole = state
        .selection(sheet)
        .filter(|selection| selection.is_columns() || selection.is_full_rows())
        .map(|selection| selection.columns(state.tree(), sheet));
    // Des lignes entières sur une feuille à x' (#166) : tracer des **lignes**,
    // pas toutes les colonnes qu'elles traversent.
    if let Some(payload) = state.row_drag_payload(sheet) {
        row_properties_menu(ui, state, actions, &payload);
        return;
    }
    match whole {
        Some(columns) if !columns.is_empty() => {
            column_properties_menu(ui, state, actions, sheet, &columns);
        }
        _ => {
            if let Some(menu) = state.grid_menu(sheet) {
                grid_edit_menu(ui, state, actions, sheet, &menu);
            }
        }
    }
}

/// Ajout disponible même sans sélection, commun au classeur et au vide de grille.
fn add_column_entry(ui: &mut egui::Ui, sheet: NodeId, actions: &mut Vec<Action>) {
    if ui.button(tr!("workbook-column-add")).clicked() {
        actions.push(Action::AddColumn { sheet });
        ui.close();
    }
}

/// Type et rôle des colonnes visées : même cible et mêmes actions dans le
/// menu Classeur et au clic droit. Une sélection hétérogène ne coche aucun
/// choix.
pub(super) fn column_type_role_menus(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    target_columns: &[NodeId],
) {
    let columns: Vec<_> = target_columns
        .iter()
        .filter_map(|id| state.tree().node(*id)?.as_column())
        .collect();
    let first = columns.first();
    let column_type = first
        .map(|column| column.column_type())
        .filter(|kind| columns.iter().all(|column| column.column_type() == *kind));
    let role = first
        .map(|column| column.meta().role)
        .filter(|role| columns.iter().all(|column| column.meta().role == *role));
    ui.add_enabled_ui(!target_columns.is_empty(), |ui| {
        // Choix manuel du type de colonne — conversion destructrice
        // assumée (`Action::SetColumnType`/`Command::
        // SetColumnType`, `stokesforge-core::Column::set_column_type`) :
        // texte non numérique effacé en passant en `Numeric`, comme
        // pour toute saisie directe dans une colonne numérique.
        ui.menu_button(tr!("column-type"), |ui| {
            for candidate in [ColumnType::Numeric, ColumnType::Mixed] {
                if ui
                    .selectable_label(column_type == Some(candidate), type_label(candidate))
                    .clicked()
                {
                    if Some(candidate) != column_type {
                        actions.push(Action::SetColumnsType {
                            columns: target_columns.to_vec(),
                            column_type: candidate,
                        });
                    }
                    ui.close();
                }
            }
        });
        // Le rôle décrit les colonnes, et reste partagé avec le
        // menu Classeur comme le type.
        ui.menu_button(tr!("column-role"), |ui| {
            for candidate in [
                ColumnRole::X,
                ColumnRole::Y,
                ColumnRole::X2,
                ColumnRole::Y2,
                ColumnRole::Z,
                ColumnRole::XError,
                ColumnRole::YError,
                ColumnRole::Label,
                ColumnRole::None,
            ] {
                if ui
                    .selectable_label(role == Some(candidate), role_label(candidate))
                    .clicked()
                {
                    if Some(candidate) != role {
                        actions.push(Action::SetColumnsRole {
                            columns: target_columns.to_vec(),
                            role: candidate,
                        });
                    }
                    ui.close();
                }
            }
        });
    });
}

/// Créer un graphe depuis les colonnes visées — chantier « tracer depuis un
/// classeur ». La règle d'appariement X/Y (chaque Y prend la colonne X la
/// plus proche à sa gauche) vit dans `AppState::plot_columns`/
/// `series_from_columns` ; ce menu ne fait que choisir le type de tracé et
/// pousser l'action sur `target_columns`. Premier niveau du clic droit des
/// lettres comme du menu Classeur (#152).
pub(super) fn plot_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    ui.add_enabled_ui(!target_columns.is_empty(), |ui| {
        ui.menu_button(tr!("workbook-plot-menu"), |ui| {
            for (label, plot_type) in [
                (tr!("plot-type-line"), PlotType::Line),
                (tr!("plot-type-scatter"), PlotType::Scatter),
                (tr!("plot-type-line-symbol"), PlotType::LineScatter),
                (tr!("plot-type-bar"), PlotType::Bar),
            ] {
                if ui.button(label).clicked() {
                    actions.push(Action::PlotColumns {
                        sheet,
                        columns: target_columns.to_vec(),
                        plot_type,
                    });
                    ui.close();
                }
            }
        });
    });
}

/// Menu Colonnes de la barre Classeur : Add Column, puis Type et Rôle.
pub(super) fn columns_menu_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    target_columns: &[NodeId],
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| add_column_entry(ui, sheet, actions));
    sections.section(ui, true, |ui| {
        column_type_role_menus(ui, state, actions, target_columns);
    });
}

/// Case « Métadonnées » de l'en-tête : le résumé des métadonnées du fichier
/// d'origine (« 23 clés »), en lecture seule. Un clic ouvre la fenêtre qui les
/// liste ([`column_metadata_window`](crate::app::plot_settings::column_metadata_window)). Hors sélection de champ : il n'y a rien
/// à copier, effacer ni éditer.
fn metadata_cell_widget(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    column: NodeId,
) {
    ui.scope(|ui| {
        ui.set_min_height(HEADER_SUBROW_HEIGHT);
        ui.set_max_height(HEADER_SUBROW_HEIGHT);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let summary = state.file_metadata_summary(column);
        if summary.is_empty() {
            return;
        }
        let response = ui.interact(
            rect,
            ui.id().with(("stokesforge_metadata_cell", column.raw())),
            egui::Sense::click(),
        );
        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(icon_label("📋", &summary))
                    .color(crate::theme::gutter_text_color()),
            )
            .selectable(false)
            .truncate(),
        );
        if response.clicked() {
            actions.push(Action::ViewColumnMetadata { column });
        }
        response.on_hover_text(tr!("workbook-metadata-summary", "hint"));
    });
}

/// Ligne de séparation fine sous une rangée de l'en-tête empilé (lettre, ou
/// l'un des quatre champs) : sans elle, les quatre rangées de champs
/// n'avaient aucun remplissage propre et se confondaient visuellement les
/// unes dans les autres (contrairement aux lignes de données, zébrées par
/// `TableBuilder::striped`).
fn header_row_separator(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// Fond d'une case de la gouttière de la grille, à trois paliers — voir
/// [`crate::theme::header_letter_row_fill`]/[`crate::theme::
/// header_letter_row_touched_fill`]/[`crate::theme::
/// header_letter_row_selected_fill`] : repos, concernée par la sélection en
/// cours sans l'être directement, ou sélectionnée elle-même (le palier le
/// plus fort l'emporte). Partagé par [`column_header_widget`], [`row_number_
/// widget`], [`header_row_label_widget`] et le « # » de [`row_number_header_
/// widget`].
fn gutter_fill(selected: bool, touched: bool) -> egui::Color32 {
    if selected {
        crate::theme::header_letter_row_selected_fill()
    } else if touched {
        crate::theme::header_letter_row_touched_fill()
    } else {
        crate::theme::header_letter_row_fill()
    }
}

/// Une rangée éditable de l'en-tête empilé, pour un seul champ de
/// [`stokesforge_core::ColumnMeta`] — le même schéma sélection/édition que
/// [`cell_widget`] pour une cellule de données : simple clic sélectionne
/// (surlignage identique), double-clic édite ; piloté par [`AppState::
/// header_editing`]/[`AppState::select_header_field`] plutôt que par
/// [`AppState::editing`]/[`AppState::select_cell`].
///
/// N'affiche que la **valeur** du champ, vide si le champ l'est — pas de
/// placeholder répété par colonne : le libellé du champ (« Long Name »,
/// « Units », « Comments », « F(x)= ») vit une seule fois dans la gouttière
/// des numéros de ligne, voir [`row_number_header_widget`].
#[allow(clippy::too_many_arguments)]
fn header_field_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    field: HeaderField,
    height: f32,
    column_selected: bool,
) {
    // `actions` ne sert pas à la validation de la saisie — elle passe par
    // `AppState::commit_header_edit`, appelé directement (mutation
    // transitoire, comme `commit_edit`) — mais bien au menu contextuel
    // ci-dessous, qui pousse des actions comme partout ailleurs.
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let rect = ui.available_rect_before_wrap();
        header_row_separator(ui, rect);
        let editing_here = state
            .header_editing()
            .is_some_and(|edit| edit.column == column && edit.field == field);
        if editing_here {
            let Some(edit) = state.header_editing_mut() else {
                return;
            };
            let response = ui.add(
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    // #38 : même filtre que la cellule — voir `GRID_KEYS`.
                    .event_filter(GRID_KEYS),
            );
            // Même piège que [`cell_widget`] : lire `lost_focus()` avant de
            // décider de redemander le focus, sous peine de se le redonner
            // à soi-même juste après qu'Entrée l'ait fait perdre en interne
            // au `TextEdit`, et de ne plus jamais valider.
            let lost_focus = response.lost_focus();
            if !response.has_focus() && !lost_focus {
                response.request_focus();
            }
            // Tab ne fait plus perdre le focus au champ (filtre `GRID_KEYS`) :
            // même lecture explicite que dans [`cell_widget`].
            let tab_pressed =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Tab));
            if tab_pressed {
                response.surrender_focus();
            }
            if lost_focus || tab_pressed {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    state.cancel_header_edit();
                } else {
                    // `consume_key`, pas `key_pressed` : Tab valide *et*
                    // déplace la sélection tout de suite (`move_selection_tab`/
                    // `AddColumnAndSelectHeaderField` ci-dessous), avant que
                    // le bloc de raccourcis clavier de `grid_contents` (qui
                    // tourne après, une fois l'édition refermée par ce
                    // `return`) ne revoie passer le même Tab — même piège
                    // que `cell_widget`.
                    let shift = ui.input(|i| i.modifiers.shift);
                    let tab = ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                    });
                    state.commit_header_edit();
                    if tab && !shift && is_last_visible_column(state, sheet, column) {
                        // Tab valide sur n'importe quel champ d'en-tête de
                        // la dernière colonne (#52) : même geste que Tab en dernière colonne
                        // d'une ligne de données — ajoute une colonne plutôt
                        // que d'entrer dans les données (voir
                        // `Action::AddColumnAndSelectHeaderField`).
                        actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                    } else if tab {
                        state.move_selection_tab(sheet, shift);
                    }
                }
            }
            return;
        }
        let text = header_field_text(state, column, field);
        let id = ui.id().with((column.raw(), field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        // Surlignée si ce champ précis est sélectionné, ou si c'est toute la
        // colonne qui l'est (sélection faite depuis la rangée des lettres,
        // voir `column_header_widget`) : la sélection d'une colonne doit se
        // voir sur ses 5 rangées d'en-tête, pas seulement sur la rangée des
        // lettres.
        let selected = column_selected || state.is_header_field_selected(sheet, column, field);
        if selected {
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.visuals().selection.bg_fill.linear_multiply(0.35),
            );
        }
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(text).weak())
                .selectable(false)
                .truncate(),
        );
        if response.double_clicked() {
            state.begin_header_edit(column, field);
            return;
        }
        // Même logique de multi-sélection à la souris que pour les
        // cellules de données (clic/glisser/Maj-clic) — voir le
        // commentaire détaillé de `cell_widget`.
        let modifiers = ui.input(|i| i.modifiers);
        let (command, shift) = (modifiers.command, modifiers.shift);
        let dragging = response.contains_pointer()
            && !press_started_in(ui, rect)
            && !grid_drag_in_progress(ui.ctx())
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some();
        if response.drag_started() && command {
            state.add_header_field_selection(sheet, column, field);
        } else if response.clicked() && command && !shift {
            state.toggle_header_field_selection(sheet, column, field);
        } else if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_field(sheet, column, field);
        } else if (response.clicked() && shift) || dragging {
            state.extend_header_field_selection(sheet, column, field);
        } else if response.secondary_clicked() && !selected {
            // Même règle que la rangée des lettres : un clic droit *dans*
            // la sélection ouvre le menu sur elle sans la toucher ; un clic
            // droit en dehors la réduit d'abord à la case visée, pour que
            // le menu agisse sur ce qu'on vient de désigner et pas sur une
            // sélection oubliée ailleurs.
            state.select_header_field(sheet, column, field);
        }
        // Calcul paresseux, pour la même raison que dans `cell_widget`.
        response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
    });
}

/// Gouttière des numéros de ligne, côté en-tête empilée : le « # » sur la
/// rangée des lettres — un clic sélectionne tout le tableau (voir
/// [`AppState::select_all`]), même geste qu'un clic sur une lettre de
/// colonne ou un numéro de ligne mais pour l'ensemble — puis, aligné avec
/// chacune des quatre rangées de champs de [`column_header_widget`], le
/// **libellé** du champ concerné (« Long Name », « Units », « Comments »,
/// « F(x)= », repris de l'ancienne UI Qt), une seule fois plutôt que répété
/// en grisé dans chaque colonne. Les cellules par colonne n'affichent plus
/// que la valeur (voir [`header_field_widget`]).
///
/// Homogène de haut en bas avec le reste de la gouttière : chaque rangée
/// porte le même fond que le « # » ([`gutter_fill`]) — les quatre rangées de
/// libellés restaient auparavant transparentes, seul le « # » du dessus
/// rompant le fond de la colonne. Chaque libellé sélectionne désormais son
/// champ pour **toutes** les colonnes visibles de la feuille (glisser/
/// Maj-clic étend à plusieurs champs), symétrique de [`row_number_widget`]
/// côté données — voir [`header_row_label_widget`]. Les libellés longs
/// restent sur une ligne (#125) : egui les tronque et affiche le texte
/// complet au survol, sans déborder sur une rangée voisine.
fn row_number_header_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.scope(|ui| {
            ui.set_min_height(HEADER_SUBROW_HEIGHT);
            ui.set_max_height(HEADER_SUBROW_HEIGHT);
            let selected = state.is_all_selected(sheet);
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, 0.0, gutter_fill(selected, false));
            header_row_separator(ui, rect);
            let id = ui.id().with("stokesforge_select_all");
            let response = ui
                .interact(rect, id, egui::Sense::click())
                .on_hover_text(tr!("action-select-all"))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.put(
                rect,
                egui::Label::new(egui::RichText::new("#").color(crate::theme::gutter_text_color()))
                    .selectable(false),
            );
            if response.clicked() && selected {
                // Un second clic désélectionne (#159).
                state.deselect_all(sheet);
            } else if response.clicked() || (response.secondary_clicked() && !selected) {
                // `&& !selected` : un clic droit alors que tout est déjà
                // sélectionné ouvre le menu sans y toucher — même règle que
                // dans les autres cases de la gouttière (voir
                // `column_header_widget`).
                state.select_all(sheet);
            }
            response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
        });
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::XPrime,
            HEADER_SUBROW_HEIGHT,
            &tr!("column-x-prime"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::LongName,
            HEADER_LONGNAME_HEIGHT,
            &tr!("column-long-name"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Unit,
            HEADER_SUBROW_HEIGHT,
            &tr!("column-units"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Comment,
            HEADER_SUBROW_HEIGHT,
            &tr!("column-comments"),
        );
        header_row_label_widget(
            ui,
            state,
            actions,
            sheet,
            HeaderField::Formula,
            HEADER_SUBROW_HEIGHT,
            &tr!("workbook-header-formula"),
        );
        if metadata_row_visible(ui.ctx()) {
            ui.scope(|ui| {
                ui.set_min_height(HEADER_SUBROW_HEIGHT);
                ui.set_max_height(HEADER_SUBROW_HEIGHT);
                let rect = ui.available_rect_before_wrap();
                ui.painter()
                    .rect_filled(rect, 0.0, gutter_fill(false, false));
                header_row_separator(ui, rect);
                let response = ui.put(
                    rect,
                    egui::Label::new(
                        egui::RichText::new(tr!("common-metadata"))
                            .color(crate::theme::gutter_text_color()),
                    )
                    .selectable(false)
                    .truncate()
                    .sense(egui::Sense::click()),
                );
                // Miroir du menu Fenêtre : rien n'est atteignable au seul
                // clic droit.
                response.context_menu(|ui| {
                    if ui.button(tr!("workbook-metadata-hide")).clicked() {
                        set_metadata_row_visible(ui.ctx(), false);
                        ui.close();
                    }
                });
            });
        }
    });
}

/// Une rangée de la gouttière côté en-tête (voir [`row_number_header_widget`]) :
/// le libellé d'un champ, à la hauteur exacte de la rangée de
/// [`column_header_widget`] qu'elle longe — et, depuis que la sélection de
/// champ entier existe, un vrai geste de sélection, miroir exact de
/// [`row_number_widget`] pour les lignes de données et de
/// [`column_header_widget`] pour les colonnes : toute la largeur du libellé
/// sélectionne le champ pour **toutes** les colonnes visibles de la feuille
/// (glisser/Maj-clic étend à plusieurs champs), le clic droit ouvre le même
/// menu contextuel d'édition ([`grid_edit_menu`]).
fn header_row_label_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    field: HeaderField,
    height: f32,
    label: &str,
) {
    ui.scope(|ui| {
        ui.set_min_height(height);
        ui.set_max_height(height);
        let selected = state.is_header_row_selected(sheet, field);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, 0.0, gutter_fill(selected, false));
        header_row_separator(ui, rect);
        let id = ui.id().with(("stokesforge_header_row_label", field as u8));
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(label).color(crate::theme::gutter_text_color()))
                .selectable(false)
                .truncate(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started() || (response.clicked() && !shift) {
            state.select_header_row(sheet, field);
        } else if response.clicked() && shift {
            state.extend_header_row_selection(sheet, field);
        } else if response.contains_pointer()
            && ui.input(|i| i.pointer.primary_down())
            && state.selection(sheet).is_some()
        {
            // Même geste que pour les cellules de données et la rangée des
            // lettres — étendre pendant le glisser, pas seulement au clic de
            // départ (voir le commentaire détaillé de `cell_widget`).
            state.extend_header_row_selection(sheet, field);
        } else if response.secondary_clicked() && !selected {
            // Clic droit hors de la sélection : la réduire à ce champ avant
            // d'ouvrir le menu — même règle que dans les trois autres zones.
            state.select_header_row(sheet, field);
        }
        // `grid_menu` **dans** la fermeture, jamais avant — même raison que
        // dans `cell_widget`/`row_number_widget` : calculée dehors, elle
        // serait recalculée à chaque image pour chacune des 4 rangées.
        response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
    });
}

/// Valeur actuelle d'un champ d'en-tête, telle qu'affichée hors édition.
fn header_field_text(state: &AppState, column: NodeId, field: HeaderField) -> String {
    state.header_field_display_text(column, field)
}

/// Touches que la grille réclame à egui tant qu'elle a le focus clavier (#38) :
/// Tab et les flèches. egui les traite au **début** de l'image
/// (`Focus::begin_pass`) pour déplacer son propre focus de widget en widget,
/// sauf si le widget focalisé les réclame par ce filtre — les lire ensuite par
/// `key_pressed`, comme le faisait la grille, ne retire rien : la sélection
/// avançait (ou pas), et le focus partait quand même dans le reste de
/// l'interface. Échap reste à egui : il rend le focus.
const GRID_KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// Clé mémoire egui du dernier ancrage de focus qu'une grille a réclamé
/// ([`claim_grid_focus`]) — de quoi reconnaître, depuis le reste de
/// l'application, que le focus clavier est à la grille et non à un champ texte.
fn grid_focus_owner_key() -> egui::Id {
    egui::Id::new("stokesforge_grid_focus_owner")
}

/// Identifiant du widget d'ancrage du focus de la grille de `sheet` — un par
/// feuille et par `Ui`, pour que le classeur flottant et sa copie détachée
/// ne se disputent pas le même identifiant.
fn grid_focus_id(ui: &egui::Ui, sheet: NodeId) -> egui::Id {
    ui.id().with(("grid_focus", sheet.raw()))
}

/// La grille prend le focus clavier : sa sélection reçoit désormais Tab et les
/// flèches (voir [`GRID_KEYS`]).
fn claim_grid_focus(ctx: &egui::Context, anchor: egui::Id) {
    ctx.memory_mut(|memory| memory.request_focus(anchor));
    ctx.data_mut(|data| data.insert_temp(grid_focus_owner_key(), anchor));
}

/// Vrai si un widget **autre qu'une grille** a le focus clavier — typiquement
/// un champ texte, qui garde alors Entrée, Suppr, Tab… Remplace le test
/// `focused().is_some()` partout où la grille, elle, peut légitimement tenir le
/// focus (#38) : sans lui, une grille focalisée ferait passer ses voisins pour
/// des champs en cours de saisie.
pub(super) fn keyboard_focus_outside_grid(ctx: &egui::Context) -> bool {
    let Some(focused) = ctx.memory(|memory| memory.focused()) else {
        return false;
    };
    let owner: Option<egui::Id> = ctx.data(|data| data.get_temp(grid_focus_owner_key()));
    owner != Some(focused)
}

/// La grille, en lignes **virtualisées** : seules les lignes visibles sont
/// construites, sans quoi une feuille d'un million de lignes bloquerait
/// chaque image. Scopée à `sheet` — voir [`book_contents`](crate::app::workbook::book_contents), son seul
/// appelant.
///
/// En-tête empilée (lettre A/B/C, nom long, unité, commentaire, formule —
/// voir [`column_header_widget`]), sélection de cellules/colonnes à la
/// souris (voir [`cell_widget`]), et raccourcis Ctrl+C/X/V/Suppr/Retour
/// arrière scopés à cette grille : ils ne sont consommés que si aucune
/// cellule ni aucun champ d'en-tête n'est en édition, **ni la fenêtre
/// d'import ouverte**, pour ne jamais voler le copier/coller natif d'un
/// `TextEdit` en cours de saisie — la fenêtre d'import est un dialogue modal
/// flottant peint par-dessus cette grille, toujours présente dessous tant
/// qu'il est ouvert (voir la garde plus bas, dans le corps de la fonction).
pub(super) fn grid_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    pending_vertical_offset: Option<f32>,
) -> Option<egui::scroll_area::ScrollAreaOutput<()>> {
    let columns = state.visible_columns(sheet);
    let row_count = state.row_count(sheet);
    if columns.is_empty() {
        let empty = ui.centered_and_justified(|ui| {
            ui.weak(tr!("workbook-grid-no-columns"));
        });
        empty
            .response
            .context_menu(|ui| add_column_entry(ui, sheet, actions));
        return None;
    }
    let (grid_rect, focus_anchor) = prepare_grid(ui, sheet);
    let was_editing = state.editing().is_some() || state.header_editing().is_some();
    let mut extents: Vec<ColumnExtent> = Vec::new();
    let mut row_extents: Vec<RowExtent> = Vec::new();
    let header_height = stacked_header_height(metadata_row_visible(ui.ctx()));
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::initial(ROW_NUMBER_COLUMN_WIDTH).at_least(MIN_ROW_NUMBER_COLUMN_WIDTH))
        .columns(
            TableColumn::initial(DEFAULT_COLUMN_WIDTH).at_least(MIN_COLUMN_WIDTH),
            columns.len(),
        )
        .min_scrolled_height(0.0)
        // La table occupe l'espace laissé par le panneau d'onglets, même
        // quand sa feuille contient peu de lignes : la grille garde ainsi
        // sa hauteur et son défilement vertical.
        .auto_shrink([false, false])
        // Sa barre verticale n'est pas dessinée ici : dans la zone de
        // défilement horizontale, elle suivrait le bord droit de la table et
        // sortirait de la vue sur une grille large (B10). Le défilement
        // (molette, glisser) reste actif ; la barre visible est portée par
        // [`show_grid_vertical_bar`], collée au bord droit de la fenêtre.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(offset) = pending_vertical_offset {
        table = table.vertical_scroll_offset(offset);
    }
    let table_output = table
        .header(header_height, |header| {
            grid_table_header(header, state, actions, sheet, &columns, &mut extents);
        })
        .body(|body| {
            grid_table_body(
                body,
                state,
                actions,
                sheet,
                &columns,
                row_count,
                &mut row_extents,
            );
        });

    let last_column_right = extents
        .last()
        .map_or(grid_rect.left(), |&(_, _, right)| right);
    grid_blank_column_target(ui, actions, sheet, grid_rect, last_column_right);
    grid_column_drag_source(ui, state, sheet, grid_rect, &extents);
    grid_row_drag_source(
        ui,
        state,
        sheet,
        (grid_rect.right(), grid_rect.top() + header_height),
        grid_rect.bottom(),
        &extents,
        &row_extents,
    );
    grid_keyboard(
        ui,
        state,
        actions,
        sheet,
        grid_rect,
        focus_anchor,
        was_editing,
    );
    Some(table_output)
}

/// Prépare les espacements et l'ancrage focalisable, sans créer de nouvelle Ui.
fn prepare_grid(ui: &mut egui::Ui, sheet: NodeId) -> (egui::Rect, egui::Id) {
    // Sans ceci, l'espacement vertical par défaut d'egui entre les rangées
    // de la table (`Spacing::item_spacing.y`, dont `TableBuilder` se sert en
    // interne pour séparer les lignes) laisse un mince filet blanc entre
    // chacune — la couleur de fond de la gouttière ne remplit alors pas
    // vraiment la case, elle y flotte (numéros de ligne notamment,
    // sensibles à l'œil car peints d'un aplat plein plutôt qu'un simple
    // texte). `item_spacing.x` reste intact : il porte l'espacement entre
    // colonnes, dont dépendent les poignées de redimensionnement.
    ui.spacing_mut().item_spacing.y = 0.0;
    // Capturé *avant* la table, dont `.auto_shrink([false, false])` occupe
    // justement tout cet espace disponible : sert de référence à la zone de
    // prise du glisser de colonnes plus bas ([`column_grip_bands`]).
    let grid_rect = ui.available_rect_before_wrap();
    // Ancrage du focus clavier de la grille (#38) : un widget focalisable
    // **seulement tant qu'il a le focus** (sinon un Tab tombant sur un
    // clavier sans focus l'y ferait entrer de lui-même). Il porte le filtre
    // d'événements [`GRID_KEYS`] — voir la doc de cette constante. La grille
    // le réclame par [`claim_grid_focus`] après un appui dans son rectangle
    // ou la fin d'une édition (plus bas) ; egui le lui retire seul à un appui
    // ailleurs ou si la grille cesse d'être peinte. Le filtre n'est pris en
    // compte qu'à l'image qui suit la prise de focus (`set_focus_lock_filter`
    // exige `had_focus_last_frame`) : un Tab dans l'image même du clic n'est
    // pas une frappe réaliste.
    let focus_anchor = grid_focus_id(ui, sheet);
    let anchor_focused = ui.memory(|memory| memory.has_focus(focus_anchor));
    ui.interact(
        grid_rect,
        focus_anchor,
        if anchor_focused {
            egui::Sense::focusable_noninteractive()
        } else {
            egui::Sense::hover()
        },
    );
    if anchor_focused {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(focus_anchor, GRID_KEYS));
    }
    (grid_rect, focus_anchor)
}

/// L'en-tête dispose les colonnes et relève la borne de la zone libre.
fn grid_table_header(
    mut header: egui_extras::TableRow<'_, '_>,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[(NodeId, String, ColumnType)],
    extents: &mut Vec<ColumnExtent>,
) {
    header.col(|ui| {
        row_number_header_widget(ui, state, actions, sheet);
    });
    for (index, (id, _, _)) in columns.iter().enumerate() {
        header.col(|ui| {
            let rect = ui.max_rect();
            extents.push((*id, rect.left(), rect.right()));
            column_header_widget(ui, state, actions, sheet, *id, index);
        });
    }
}

/// Le corps garde la virtualisation des lignes au même endroit dans la table.
fn grid_table_body(
    body: egui_extras::TableBody<'_>,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[(NodeId, String, ColumnType)],
    row_count: usize,
    row_extents: &mut Vec<RowExtent>,
) {
    body.rows(ROW_HEIGHT, row_count, |mut row| {
        let index = row.index();
        row.col(|ui| {
            let rect = ui.max_rect();
            row_extents.push((index, rect.top(), rect.bottom()));
            row_number_widget(ui, state, actions, sheet, index);
        });
        for (id, _, _) in columns {
            row.col(|ui| {
                cell_widget(ui, state, actions, sheet, *id, index);
            });
        }
    });
}

fn grid_blank_column_target(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    grid_rect: egui::Rect,
    last_column_right: f32,
) {
    // La portion libre après la dernière colonne est une cible distincte
    // des cellules et de leurs menus. La borne vient de l'en-tête réellement
    // disposé (y compris les largeurs redimensionnées), pas des largeurs par défaut.
    if last_column_right < grid_rect.right() {
        let rect = egui::Rect::from_min_max(
            egui::pos2(last_column_right, grid_rect.top()),
            grid_rect.right_bottom(),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("add_column_blank", sheet)),
            egui::Sense::click(),
        );
        response.context_menu(|ui| add_column_entry(ui, sheet, actions));
    }
}

/// Glisser une sélection de **colonnes entières** vers un graphe (#160), en
/// attrapant le bord extérieur d'un bloc de colonnes sélectionnées, comme la
/// sélection d'Excel : curseur « main » au survol, « main fermée » pendant le
/// glisser. Les bandes ([`column_grip_bands`]) sont posées **après** la table,
/// donc au-dessus des cellules et des poignées de redimensionnement : elles
/// n'existent que sur les bords des colonnes sélectionnées, hors de la rangée
/// des lettres, si bien que le glisser d'extension de sélection des cellules
/// et le redimensionnement gardent tout le reste de la surface.
///
/// La charge utile ([`AppState::column_drag_payload`]) emporte toutes les
/// colonnes sélectionnées, dans l'ordre de la feuille. Échap annule le
/// glisser ; un repère (nombre de colonnes) suit le pointeur.
fn grid_column_drag_source(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    grid_rect: egui::Rect,
    extents: &[ColumnExtent],
) {
    let ctx = ui.ctx().clone();
    let dragging = egui::DragAndDrop::payload::<ColumnDragPayload>(&ctx)
        .filter(|payload| payload.sheet == sheet);
    if let Some(payload) = &dragging {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(&ctx);
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            column_drag_marker(&ctx, sheet, payload.columns.len());
        }
        return;
    }
    let Some(payload) = state.column_drag_payload(sheet) else {
        return;
    };
    // Sous la rangée des lettres : ses bords de colonne redimensionnent.
    let top = grid_rect.top() + HEADER_SUBROW_HEIGHT;
    for (index, band) in column_grip_bands(extents, &payload.columns, top, grid_rect.bottom())
        .into_iter()
        .enumerate()
    {
        let response = ui.interact(
            band,
            ui.id().with(("column_grip", sheet, index)),
            egui::Sense::drag(),
        );
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if response.drag_started() {
            response.dnd_set_drag_payload(payload.clone());
        }
    }
}

/// Glisser une sélection de **lignes entières** vers un graphe (#166), par le
/// bord haut ou bas d'un bloc de lignes sélectionnées — mêmes curseurs, même
/// repère et même Échap que pour les colonnes ([`grid_column_drag_source`]).
/// Proposé seulement quand la feuille porte une ligne x' (voir
/// [`AppState::row_drag_payload`]) : sans x', une ligne n'a pas d'abscisse.
/// `body_top`/`right` bornent la table (sous l'en-tête) ; la gouttière des
/// numéros de ligne, à gauche de la première colonne, reste libre pour
/// sélectionner.
fn grid_row_drag_source(
    ui: &mut egui::Ui,
    state: &AppState,
    sheet: NodeId,
    (right, body_top): (f32, f32),
    bottom: f32,
    columns: &[ColumnExtent],
    rows: &[RowExtent],
) {
    let ctx = ui.ctx().clone();
    let dragging =
        egui::DragAndDrop::payload::<RowDragPayload>(&ctx).filter(|payload| payload.sheet == sheet);
    if let Some(payload) = &dragging {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(&ctx);
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            row_drag_marker(&ctx, sheet, payload.rows.len());
        }
        return;
    }
    let Some(payload) = state.row_drag_payload(sheet) else {
        return;
    };
    let Some(left) = columns.first().map(|column| column.1) else {
        return;
    };
    for (index, band) in row_grip_bands(rows, &payload.rows, left, right, body_top, bottom)
        .into_iter()
        .enumerate()
    {
        let response = ui.interact(
            band,
            ui.id().with(("row_grip", sheet, index)),
            egui::Sense::drag(),
        );
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if response.drag_started() {
            response.dnd_set_drag_payload(payload.clone());
        }
    }
}

/// Repère qui suit le pointeur pendant le glisser de lignes.
fn row_drag_marker(ctx: &egui::Context, sheet: NodeId, count: usize) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new(("row_drag_marker", sheet)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::icon_image(
                        icons::Icon::PlotLine,
                        icons::IconRole::Neutral,
                    ));
                    ui.label(tr!("grid-drag-rows", count = count));
                });
            });
        });
}

/// Repère qui suit le pointeur pendant le glisser de colonnes : un
/// pictogramme du composant d'icônes et le nombre de colonnes emportées.
fn column_drag_marker(ctx: &egui::Context, sheet: NodeId, count: usize) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new(("column_drag_marker", sheet)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::icon_image(
                        icons::Icon::PlotLine,
                        icons::IconRole::Neutral,
                    ));
                    ui.label(tr!("grid-drag-columns", count = count));
                });
            });
        });
}

/// Après la peinture et le dépôt : reprise du focus, puis saisie, navigation et édition.
fn grid_keyboard(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    grid_rect: egui::Rect,
    focus_anchor: egui::Id,
    was_editing: bool,
) {
    // Prise du focus clavier (#38), **avant** le bloc de raccourcis, pour que
    // la touche qui suit un clic soit déjà celle de la grille. Deux cas : un
    // appui dans le rectangle de la grille (clic, glisser, menu contextuel),
    // sauf s'il ouvre ou prolonge une édition, dont le champ texte réclame le
    // focus lui-même ; et la fin d'une édition qui a rendu le focus à
    // personne (Entrée, Tab — voir [`cell_widget`] —, Échap) : sans lui, le
    // Tab suivant repartirait dans le reste de l'interface. Un focus tenu par
    // un autre widget (clic dans un autre champ) n'est jamais repris.
    let editing_now = state.editing().is_some() || state.header_editing().is_some();
    // Appui **ou** relâchement : egui rend le focus de l'ancrage au clic
    // (`SurrenderFocusOn::Clicks`, son réglage par défaut), donc au
    // relâchement, parce que l'ancrage n'est pas sous le curseur ; l'appui
    // seul ne suffit pas pour un clic simple.
    let pressed_in_grid = ui
        .input(|input| input.pointer.any_pressed() || input.pointer.any_released())
        && ui.rect_contains_pointer(grid_rect);
    let edit_just_ended =
        was_editing && !editing_now && ui.ctx().memory(|memory| memory.focused()).is_none();
    if !editing_now && (pressed_in_grid || edit_just_ended) {
        claim_grid_focus(ui.ctx(), focus_anchor);
    }
    let focus_free = ui
        .ctx()
        .memory(|memory| memory.focused())
        .is_none_or(|focused| focused == focus_anchor);

    // Raccourcis clavier de la grille — voir la doc de tête de cette
    // fonction pour la garde d'édition.
    //
    // `state.import_state().is_none()` est la troisième condition de cette
    // garde, ajoutée avec la refonte de la fenêtre d'import : `consume_key`
    // ci-dessous agit sur l'`InputState` du `Context` entier, pas sur celui
    // d'une `Ui` — donc pas seulement sur cette grille. Tant que la fenêtre
    // d'import (un `egui::Window` flottant, peint *après* les fenêtres
    // flottantes de contenu — voir `StokesForgeApp::ui`) est ouverte, cette
    // grille continue d'être peinte
    // dessous à chaque image ; sans cette condition, Retour arrière/Suppr
    // tapés dans un champ texte de la fenêtre d'import étaient retirés d'ici
    // avant même que ce champ ait pu les voir : plus aucune suppression de
    // caractère possible sans passer par une sélection (qui, elle, remplace
    // le texte sélectionné via l'insertion d'`Event::Text`, jamais
    // consommée ici) — c'était le bug connu du champ de texte du module
    // d'import, corrigé ici plutôt que dans la fenêtre elle-même parce que
    // c'est ici qu'il naissait.
    //
    // #17 : même vol, cette fois dans le dialogue des propriétés du graphe —
    // la liste de conditions par fenêtre ne pouvait que rater la suivante.
    // `focus_free` (aucun widget n'a le focus clavier, ou c'est l'ancrage de
    // **cette** grille — voir `GRID_KEYS`, #38 ; `egui_wants_keyboard_input`
    // ne le distinguait pas d'un champ texte) couvre d'un coup ce dialogue,
    // l'import et tout futur dialogue ; `import_state` reste pour le cas d'un
    // import ouvert sans champ focalisé.
    if state.editing().is_none()
        && state.header_editing().is_none()
        && state.import_state().is_none()
        && focus_free
    {
        // #143 : F2 appartient uniquement à la grille focalisée. Les autres
        // raccourcis gardent leur secours sans focus, mais F2 doit rester
        // disponible pour les gestionnaires. Réutiliser l'éditeur du double-clic.
        if ui.ctx().memory(|memory| memory.focused()) == Some(focus_anchor)
            && ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_EDIT_CELL))
        {
            let target = state.selection(sheet).and_then(|selection| {
                let rank = selection.full_rank_range()?.0;
                let column = *selection.columns(state.tree(), sheet).first()?;
                Some((column, rank))
            });
            if let Some((column, rank)) = target {
                if rank < 0 {
                    state.begin_header_edit(column, HeaderField::from_rank(rank));
                } else {
                    state.begin_edit(column, rank as usize);
                }
            }
            return;
        }
        // Ctrl+A : même effet que la case du coin (#159). Échap lève la
        // sélection, comme un second clic sur la case du coin.
        if ui.input_mut(|input| input.consume_shortcut(&SHORTCUT_SELECT_ALL)) {
            actions.push(Action::SelectAll { sheet });
        } else if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            state.deselect_all(sheet);
        }
        start_grid_typing(ui, state, sheet);
        navigate_grid_selection(ui, state, actions, sheet);
        grid_clipboard_shortcuts(ui, state, actions, sheet);
    }
}

fn start_grid_typing(ui: &egui::Ui, state: &mut AppState, sheet: NodeId) {
    // Taper sur une cellule ou un champ d'en-tête sélectionné (simple
    // clic, pas encore en édition) démarre l'édition en remplaçant tout
    // son contenu par ce qui est tapé — comportement standard de
    // tableur, le même pour les deux. Ne se déclenche que pour une
    // sélection réduite à une seule cellule ([`Selection::single_cell`])
    // côté données : taper sur une plage ne doit jamais écraser
    // plusieurs cellules à la fois — un champ d'en-tête, lui, n'est
    // jamais une plage. `egui::Event::Text` ne porte que du texte
    // imprimable — jamais une touche de contrôle ou de navigation, qui
    // n'apparaît que dans `events` comme un `Event::Key` — donc pas de
    // filtrage supplémentaire à faire ici.
    let typed: String = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    });
    if !typed.is_empty() {
        if let Some((column, row)) = state.selection(sheet).and_then(Selection::single_cell) {
            state.begin_edit(column, row);
            if let Some(edit) = state.editing_mut() {
                edit.buffer = typed;
            }
        } else if let Some((column, field)) = state.selected_header_field(sheet) {
            state.begin_header_edit(column, field);
            if let Some(edit) = state.header_editing_mut() {
                edit.buffer = typed;
            }
        }
    }
}

fn navigate_grid_selection(
    ui: &egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    // Déplacement clavier d'une cellule déjà sélectionnée, sans
    // édition en cours (pendant l'édition, ces mêmes touches sont
    // gérées par `cell_widget`, qui valide *puis* déplace : voir sa
    // doc). Flèches directionnelles + Maj pour étendre le rectangle ;
    // Tab/Maj+Tab avec retour à la ligne aux bords ; Entrée/Maj+Entrée
    // comme équivalents non-éditants de Bas/Haut. Sans effet si la
    // sélection courante n'est pas un rectangle de cellules (colonne
    // entière ou champ d'en-tête) — voir `Selection::cursor`.
    let shift = ui.input(|i| i.modifiers.shift);
    let (up, down, left, right, tab, enter) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::ArrowLeft),
            input.key_pressed(egui::Key::ArrowRight),
            input.key_pressed(egui::Key::Tab),
            input.key_pressed(egui::Key::Enter),
        )
    });
    if up {
        state.move_selection(sheet, MoveDirection::Up, shift);
    }
    if down {
        state.move_selection(sheet, MoveDirection::Down, shift);
    }
    if left {
        state.move_selection(sheet, MoveDirection::Left, shift);
    }
    if right {
        state.move_selection(sheet, MoveDirection::Right, shift);
    }
    if tab && !shift {
        // Tab sur la dernière colonne d'une ligne de données, ou sur
        // n'importe quel champ d'en-tête de la dernière colonne : même geste — ajoute une colonne plutôt que de
        // revenir en début de ligne suivante ou d'entrer dans les
        // données, puis sélectionne tout de suite la case qui y
        // correspondra (voir `Action::AddColumnAndSelect`/
        // `Action::AddColumnAndSelectHeaderField`, qui ne peuvent la
        // poser qu'une fois la colonne réellement créée, contrairement à
        // une ligne). Valait seulement pour Formule avant #52 (Tab
        // circulait d'abord colonne par colonne pour un même champ) ;
        // décision levée par l'utilisateur.
        let cursor = state.selection(sheet).and_then(Selection::grid_cursor);
        let needs_new_column = match cursor {
            Some(GridCursor::Data(column, _) | GridCursor::Header(column, _)) => {
                is_last_visible_column(state, sheet, column)
            }
            None => false,
        };
        if needs_new_column {
            match cursor {
                Some(GridCursor::Data(_, row)) => {
                    actions.push(Action::AddColumnAndSelect { sheet, row });
                }
                Some(GridCursor::Header(_, field)) => {
                    actions.push(Action::AddColumnAndSelectHeaderField { sheet, field });
                }
                None => {}
            }
        } else {
            state.move_selection_tab(sheet, false);
        }
    } else if tab {
        state.move_selection_tab(sheet, true);
    }
    if enter && shift {
        state.move_selection(sheet, MoveDirection::Up, false);
    } else if enter {
        // Entrée sur la dernière ligne de la feuille, cellule déjà
        // vide et jamais éditée dans cette frappe : `move_selection`
        // seul ne ferait rien (`Down` est borné à `row_count - 1`,
        // voir sa doc). Même geste que `CommitCellAndAdvance` sur une
        // cellule qu'on vient de remplir — ajoute la ligne, puis
        // sélectionne tout de suite la cellule qui y correspondra
        // (elle sera créée avant le prochain rendu, voir `cell_widget`).
        let data_cursor = state
            .selection(sheet)
            .and_then(Selection::grid_cursor)
            .and_then(|cursor| match cursor {
                GridCursor::Data(column, row) => Some((column, row)),
                GridCursor::Header(..) => None,
            });
        let at_last_row = data_cursor.is_some_and(|(_, row)| row + 1 >= state.row_count(sheet));
        if at_last_row {
            if let Some((column, row)) = data_cursor {
                actions.push(Action::InsertRows {
                    sheet,
                    at: row + 1,
                    count: 1,
                });
                state.select_cell(sheet, column, row + 1);
            }
        } else {
            state.move_selection(sheet, MoveDirection::Down, false);
        }
    }
}

fn grid_clipboard_shortcuts(
    ui: &egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
) {
    // Ctrl+C/X/V n'apparaissent **jamais** comme un `egui::Event::Key`
    // ordinaire (donc jamais via `consume_key`/`consume_shortcut`) :
    // `egui-winit` les intercepte lui-même à la traduction des
    // événements de la fenêtre et pousse à la place un `Event::Copy`/
    // `Event::Cut`/`Event::Paste(String)` (ce dernier porte déjà le
    // contenu du presse-papier système, lu par `egui-winit` via
    // `arboard` en interne — inutile de le relire nous-mêmes). Ça vaut
    // aussi pour Ctrl+Maj+C/Ctrl+Maj+V : `egui-winit::is_copy_command`/
    // `is_paste_command` ne testent que `modifiers.command`, jamais
    // `shift` (voir `egui-winit/src/lib.rs`), donc Maj enfoncée ou non
    // produit le **même** `Event::Copy`/`Event::Paste` — jamais un
    // `Event::Key` à capturer par un raccourci séparé (un essai
    // précédent le supposait à tort : `consume_shortcut` sur un
    // raccourci Ctrl+Maj+C/V ne matchait jamais). On distingue donc les
    // deux en lisant `input.modifiers.shift` au moment de l'événement —
    // toujours à jour à ce point, `ModifiersChanged` précédant la
    // frappe de la touche elle-même. Retour arrière/Suppr, eux, n'ont
    // pas de modificateur et restent des `Event::Key` ordinaires,
    // capturés via `consume_key` comme avant. Suppr est le seul des
    // deux à ne pas dispatcher une action fixe : `AppState::
    // consume_delete_key` décide si c'est un vidage ou une suppression
    // selon qu'il s'agit ou non de la deuxième pression consécutive sur
    // la même sélection (voir sa doc).
    let (mut copy, mut cut, mut paste_text, mut copy_full_columns, mut paste_absolute) =
        (false, false, None, false, false);
    ui.input(|input| {
        let shift = input.modifiers.shift;
        for event in &input.events {
            match event {
                egui::Event::Copy if shift => copy_full_columns = true,
                egui::Event::Copy => copy = true,
                egui::Event::Cut => cut = true,
                egui::Event::Paste(_) if shift => paste_absolute = true,
                egui::Event::Paste(text) => paste_text = Some(text.clone()),
                _ => {}
            }
        }
    });
    let (delete, backspace) = ui.input_mut(|input| {
        (
            input.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
            input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
        )
    });
    // Toute la matrice de comportement (une seule case copiée devient du
    // texte brut, un rectangle devient relatif, Ctrl+Maj+C promeut les
    // colonnes touchées en entier, un collage se décale sur l'ancre ou
    // rejoue ses rangs d'origine…) vit dans `AppState` — voir sa doc de
    // tête, section copier/coller. Cette couche ne fait que traduire
    // clavier/souris en appels, la même façon quelle que soit la sorte
    // de sélection courante : plus de branche séparée pour un champ
    // d'en-tête sélectionné, `AppState::copy_selection`/`AppState::
    // paste`/`Action::ClearSelectedCells`/`Action::DeleteSelectedCells`
    // savent déjà tous distinguer en-tête et données par eux-mêmes.
    if copy_full_columns {
        if let Some(text) = state.copy_full_columns(sheet) {
            ui.ctx().copy_text(text);
        }
    } else if copy || cut {
        match state.copy_selection(sheet) {
            Some(text) => ui.ctx().copy_text(text),
            // Copie ambiguë (#159) : Couper ne vide rien, il n'a rien copié.
            None => cut = false,
        }
    }
    if cut || backspace {
        // Une autre mutation de la grille s'intercale entre deux
        // pressions sur Suppr : la deuxième doit repartir au premier
        // temps (vider), pas supprimer par surprise — voir la doc de
        // `AppState::consume_delete_key`.
        state.forget_delete_repeat(sheet);
        actions.push(Action::ClearSelectedCells { sheet });
    }
    if delete {
        if let Some(action) = state.consume_delete_key(sheet) {
            actions.push(action);
        }
    }
    if paste_absolute {
        state.forget_delete_repeat(sheet);
        actions.push(Action::PasteAbsolute { sheet });
    } else if let Some(text) = paste_text {
        state.forget_delete_repeat(sheet);
        actions.push(Action::Paste { sheet, text });
    }
}

/// Case de la gouttière des numéros de ligne, dans le **corps** de la table
/// — miroir exact de la rangée des lettres de colonne
/// ([`column_header_widget`]) mais pour les lignes : toute la largeur de la
/// case sélectionne **toute la ligne** (glisser/Maj-clic étend à plusieurs
/// lignes, même logique de multi-sélection que [`cell_widget`]/
/// [`column_header_widget`]) et ouvre le même menu contextuel d'édition
/// ([`grid_edit_menu`]).
///
/// Fond à trois paliers comme la rangée des lettres ([`gutter_fill`]) —
/// repos, touchée par une sélection de données sans l'être elle-même
/// ([`AppState::row_touched_by_selection`]), ou directement sélectionnée —
/// pour que la gouttière se reconnaisse comme une zone de contrôle au
/// premier coup d'œil, et que la ligne d'une cellule sélectionnée se
/// remarque même sans avoir cliqué la gouttière elle-même. Cohérent avec le
/// choix déjà fait pour `column_header_widget` et pour le « # » de
/// [`row_number_header_widget`] juste au-dessus.
fn row_number_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    row: usize,
) {
    let selected = state.is_row_selected(sheet, row);
    let touched = state.row_touched_by_selection(sheet, row);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(rect, 0.0, gutter_fill(selected, touched));
    let id = ui.id().with(("stokesforge_row_number", row));
    let response = ui
        .interact(rect, id, egui::Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.put(
        rect,
        egui::Label::new(
            egui::RichText::new(format!("{}", row + 1)).color(crate::theme::gutter_text_color()),
        )
        .selectable(false),
    );
    let modifiers = ui.input(|i| i.modifiers);
    if response.clicked() || response.drag_started() {
        // Ctrl+clic bascule la ligne, Maj+clic prend la plage ancre → ligne,
        // Ctrl+Maj+clic l'ajoute à la sélection (#159) — comme les lettres.
        match (modifiers.command, modifiers.shift) {
            (true, true) => state.extend_row_selection(sheet, row),
            (true, false) => state.toggle_row_selection(sheet, row),
            (false, true) => state.extend_row_selection_replacing(sheet, row),
            (false, false) => state.select_row(sheet, row),
        }
    } else if response.contains_pointer()
        && !press_started_in(ui, rect)
        && !grid_drag_in_progress(ui.ctx())
        && ui.input(|i| i.pointer.primary_down())
        && state.selection(sheet).is_some()
    {
        // Même geste que pour les cellules de données et la rangée des
        // lettres — étendre pendant le glisser, pas seulement au clic de
        // départ (voir le commentaire détaillé de `cell_widget`).
        state.extend_row_selection(sheet, row);
    } else if response.secondary_clicked() && !selected {
        // Clic droit hors de la sélection : la réduire à cette ligne avant
        // d'ouvrir le menu — même règle que pour une cellule ou un champ
        // d'en-tête (voir `cell_widget`/`header_field_widget`).
        state.select_row(sheet, row);
    }
    // `grid_menu` **dans** la fermeture, jamais avant — même raison que dans
    // `cell_widget` : calculée dehors, elle serait recalculée à chaque image
    // pour chaque ligne visible.
    response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
}

/// Peint une cellule et récolte les actions qu'elle produit.
///
/// Une seule cellule à la fois porte un vrai tampon (celui de
/// [`AppState::editing`]) ; toutes les autres sont peintes comme un `Label`
/// surligné si sélectionné, sans jamais accaparer le focus clavier — ce qui
/// garde le coût mémoire de la grille indépendant du nombre de lignes, et
/// laisse le clic simple à la sélection plutôt qu'à l'édition.
///
/// - Simple clic (ou glisser à partir d'ici) : sélectionne cette cellule,
///   ancre d'un futur rectangle.
/// - Shift-clic, ou survol pendant qu'un glisser est en cours ailleurs dans
///   la grille : étend la sélection jusqu'ici.
/// - Double-clic : ouvre l'édition.
///
/// Le contenu n'est envoyé au modèle qu'à la **perte de focus** de l'édition
/// — ce que provoque aussi la touche Entrée — jamais à chaque frappe : sinon
/// l'historique compterait une entrée par caractère. Perdre le focus par
/// Entrée pousse [`Action::CommitCellAndAdvance`] plutôt que [`Action::
/// CommitCell`] : c'est elle qui ajoute une ligne quand on était sur la
/// dernière de la colonne.
fn cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    // L'identifiant est ancré sur (colonne, ligne) et non sur la position à
    // l'écran : avec des lignes virtualisées, faire défiler déplacerait sinon
    // le focus d'une cellule à l'autre.
    ui.push_id((column.raw(), row), |ui| {
        let editing_here = state
            .editing()
            .is_some_and(|edit| edit.column == column && edit.row == row);

        if editing_here {
            edit_cell_widget(ui, state, actions, sheet, column, row);
            return;
        }
        display_cell_widget(ui, state, actions, sheet, column, row);
    });
}

/// Champ de saisie : la validation consomme Tab/Entrée avant le clavier de grille.
fn edit_cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    let Some(edit) = state.editing_mut() else {
        return;
    };
    let response = ui.add(
        egui::TextEdit::singleline(&mut edit.buffer)
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY)
            // #38 : Tab ne doit pas faire sortir le focus du champ
            // vers le reste de l'interface — voir `GRID_KEYS` ; c'est
            // ce widget qui le lit (`tab_pressed`) pour valider.
            .event_filter(GRID_KEYS),
    );
    // `Response::has_focus`/`lost_focus` sont recalculés à chaque
    // appel à partir de l'état *courant* du focus (pas un instantané
    // figé) : il faut donc lire `lost_focus()` **avant** de
    // potentiellement redemander le focus, sous peine de se
    // redonner nous-mêmes le focus qu'on vient de perdre (Entrée
    // fait perdre le focus au `TextEdit` en interne) et de faire
    // rentrer `lost_focus()` à `false` — plus aucune validation
    // n'aurait alors lieu, la cellule restant bloquée en édition et,
    // avec elle, `AppState::editing()` bloquant à son tour tous les
    // raccourcis clavier de la grille (voir la garde en tête de
    // `grid_contents`).
    let lost_focus = response.lost_focus();
    // Sans ceci, un `TextEdit` qui vient d'apparaître (édition
    // ouverte par double-clic ou, depuis le bug 1, par une frappe
    // sur une cellule sélectionnée) ne reçoit le focus clavier
    // qu'après un clic dessus — egui ne le lui donne pas
    // automatiquement à l'apparition.
    if !response.has_focus() && !lost_focus {
        response.request_focus();
    }
    // Avec le filtre `GRID_KEYS`, Tab ne fait plus perdre le focus au
    // champ (c'était lui, auparavant, qui signalait la validation) :
    // on lit la touche, et on rend le focus nous-mêmes — la grille le
    // reprend à la fin de l'édition (`grid_contents`).
    let tab_pressed = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Tab));
    if tab_pressed {
        response.surrender_focus();
    }
    if !lost_focus && !tab_pressed {
        return;
    }
    // Échap referme la saisie sans rien écrire ; toute autre perte
    // de focus (clic ailleurs, Tab, Entrée) valide — et, pour
    // Entrée/Maj+Entrée/Tab/Maj+Tab, déplace aussi la sélection vers
    // la cellule suivante, comme dans tout tableur (déplacement
    // clavier complet : voir le bloc de raccourcis hors édition dans
    // `grid_contents` pour les flèches directionnelles, qui ne
    // s'appliquent qu'à une cellule déjà sélectionnée mais pas en
    // édition — `TextEdit` capture lui-même les flèches pour
    // déplacer le curseur de texte pendant la saisie).
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        state.cancel_edit();
        return;
    }
    let shift = ui.input(|i| i.modifiers.shift);
    // `consume_key`, pas `key_pressed` : cette cellule valide *et*
    // déplace la sélection tout de suite (`select_cell`/
    // `move_selection*` ci-dessous), avant que le bloc de
    // raccourcis clavier de `grid_contents` (qui tourne après, une
    // fois `state.editing()` redevenu `None` par le `cancel_edit`
    // ci-dessous) ne voie passer la même image. Sans consommer la
    // touche ici, ce bloc plus tard dans la même image relisait le
    // même Entrée/Tab (`key_pressed` ne retire rien des
    // événements) et déplaçait la sélection une deuxième fois —
    // Entrée sur la ligne 2 atterrissait sur la ligne 4, pas 3.
    // `Modifiers::NONE` matche Entrée avec ou sans Maj
    // (`Modifiers::matches_logically` ignore Maj/Alt en trop quand
    // le motif ne les demande pas).
    let enter = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let tab = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    let pending = state
        .editing()
        .filter(|edit| edit.column == column && edit.row == row)
        .map(|edit| edit.buffer.clone());
    if let Some(text) = pending {
        state.cancel_edit();
        if enter && !shift {
            // `CommitCellAndAdvance` ajoute déjà une ligne quand
            // `row` était la dernière de la colonne ; sélectionner
            // `row + 1` sans attendre que cette commande (appliquée
            // après le rendu de cette image) ait créé la ligne est
            // sûr : soit elle existait déjà, soit elle est sur le
            // point de l'être.
            actions.push(Action::CommitCellAndAdvance {
                column,
                row,
                text,
                sheet,
            });
            state.select_cell(sheet, column, row + 1);
        } else {
            actions.push(Action::CommitCell { column, row, text });
            if enter {
                // Maj+Entrée : remonte d'une ligne, jamais d'ajout.
                state.move_selection(sheet, MoveDirection::Up, false);
            } else if tab && !shift && is_last_visible_column(state, sheet, column) {
                // Même geste que dans le bloc de raccourcis hors
                // édition : Tab valide sur la dernière colonne ajoute
                // une colonne plutôt que de revenir en début de ligne
                // suivante (voir `Action::AddColumnAndSelect`).
                actions.push(Action::AddColumnAndSelect { sheet, row });
            } else if tab {
                state.move_selection_tab(sheet, shift);
            }
        }
    }
}

/// Cellule au repos : affichage, sélection et menu contextuel paresseux.
fn display_cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    column: NodeId,
    row: usize,
) {
    let text = state.cell_text(column, row);
    let stale = state.column_staleness(column).is_some();
    let selected = state.is_selected(sheet, column, row);
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, ui.id().with("cell"), egui::Sense::click_and_drag());
    if selected {
        ui.painter().rect_filled(
            rect,
            0.0,
            ui.visuals().selection.bg_fill.linear_multiply(0.35),
        );
    }
    // `.truncate()` plutôt que le retour à la ligne par défaut d'un
    // `Label` : une valeur trop large pour sa colonne est coupée à
    // droite (avec un léger fondu), jamais renvoyée sur une seconde
    // ligne qui casserait la hauteur uniforme des rangées de la table.
    //
    // Alignement à la tableur, dans le **corps** seulement (jamais la
    // gouttière ni l'en-tête, qui ne portent pas de valeur de colonne) :
    // les nombres à droite, le texte à gauche — `AppState::
    // cell_is_numeric` tranche par cellule, pas par colonne, pour
    // qu'une colonne `Mixed` aligne chaque ligne selon sa propre valeur.
    //
    // `Label::halign` seul ne suffit pas ici : `ui.put` place le widget
    // dans un `Layout::centered_and_justified`, qui justifie le texte
    // (l'étire pour remplir la largeur) avant que `halign` n'ait voix au
    // chapitre — le résultat restait visuellement centré. Un vrai
    // `Layout` directionnel (`right_to_left`/`left_to_right`), comme
    // celui déjà posé sur toute la table (`cell_layout` du
    // `TableBuilder`), aligne réellement le texte contre le bord voulu.
    let layout = if state.cell_is_numeric(column, row) {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(layout),
        |ui| {
            // Une colonne pas à jour est entièrement grisée (#181, M9 a).
            let text = if stale {
                egui::RichText::new(text).color(ui.visuals().weak_text_color())
            } else {
                egui::RichText::new(text)
            };
            ui.add(egui::Label::new(text).selectable(false).truncate());
        },
    );

    if response.double_clicked() {
        state.begin_edit(column, row);
        return;
    }
    let modifiers = ui.input(|i| i.modifiers);
    let (command, shift) = (modifiers.command, modifiers.shift);
    if response.drag_started() && command {
        // Ctrl+glisser : un nouveau rectangle, les précédents restent (#159).
        state.add_cell_selection(sheet, column, row);
    } else if response.clicked() && command && !shift {
        state.toggle_cell_selection(sheet, column, row);
    } else if response.drag_started() || (response.clicked() && !shift) {
        state.select_cell(sheet, column, row);
    } else if response.clicked() && shift {
        state.extend_selection(sheet, column, row);
    } else if response.contains_pointer()
        && !press_started_in(ui, rect)
        && !grid_drag_in_progress(ui.ctx())
        && ui.input(|i| i.pointer.primary_down())
        && state.selection(sheet).is_some()
    {
        // `response.dragged()` ne se déclenche que sur le widget qui a
        // *commencé* le glisser — jamais sur les cellules simplement
        // survolées ensuite —, d'où ce test manuel plutôt que
        // `dragged()` pour étendre la sélection pendant qu'on glisse.
        //
        // `response.hovered()` ne convient pas non plus : tant qu'un
        // glisser est en cours *ailleurs* (sur la cellule de départ),
        // egui ne considère plus que ce widget d'origine comme
        // « survolé » — aucune autre cellule ne redevient `hovered()`
        // pendant tout le glisser, même sous le curseur (`egui::
        // Context::interact`, doc de `InteractionSnapshot::hovered` :
        // « While dragging or clicking something, nothing else is
        // hovered »). `contains_pointer()` reste vrai indépendamment de
        // ce qui est en train d'être glissé ailleurs — c'est le test
        // qu'il faut ici.
        state.extend_selection(sheet, column, row);
    } else if response.secondary_clicked() && !selected {
        // Clic droit hors de la sélection : la réduire à cette cellule
        // avant d'ouvrir le menu — même règle que dans l'en-tête et sur
        // la rangée des lettres (voir `header_field_widget`).
        state.select_cell(sheet, column, row);
    }
    // `grid_menu` **dans** la fermeture, jamais avant : elle ne tourne
    // que tant que le menu est ouvert. Calculée dehors, elle serait
    // recalculée à chaque image pour *chaque cellule visible* — et elle
    // parcourt l'arbre (`Selection::columns`), ce qui en ferait un coût
    // quadratique en nombre de colonnes sur une grille qui ne fait que
    // s'afficher.
    response.context_menu(|ui| selection_context_menu(ui, state, actions, sheet));
}

/// Un glisser de colonnes ou de lignes vers un graphe est-il en cours ? Tant
/// qu'il dure, survoler d'autres cases n'étend plus la sélection : sinon elle
/// serait remplacée « à la volée » par une sélection de cellules (#160, #166).
fn grid_drag_in_progress(ctx: &egui::Context) -> bool {
    egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ctx)
        || egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(ctx)
}

/// Vrai si l'appui en cours a commencé dans `rect`. L'extension de sélection
/// au survol ne concerne que les cases où le glisser n'a **pas** commencé :
/// sur la case d'origine, un Ctrl+clic (qui bascule au relâchement) aurait
/// sinon déjà étendu la sélection pendant l'appui (#159).
fn press_started_in(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|i| i.pointer.press_origin())
        .is_some_and(|origin| rect.contains(origin))
}

#[cfg(test)]
mod tests {
    //! Tests de la grille, sans fenêtre : un `egui::Context` nu, des
    //! événements injectés, et la lecture de ce qui est **peint** (textes,
    //! positions) ou des actions poussées — jamais du texte source.
    //!
    //! Le banc peint le classeur entier ([`book_contents`]) plutôt que la seule
    //! grille : c'est le contenu commun du classeur flottant et du classeur
    //! détaché, donc le chemin qu'emprunte l'utilisateur dans les deux
    //! fenêtres.
    use super::super::test_support::image_de;

    use super::super::workbook::book_contents;
    use super::*;
    use crate::theme;
    use stokesforge_core::{Column, Project};
    use stokesforge_session::Session;

    /// Les bandes de prise bordent le **bloc** de colonnes sélectionnées —
    /// une de part et d'autre —, et seulement lui : un bloc qui n'est pas
    /// contigu en a deux paires, les colonnes non sélectionnées aucune.
    #[test]
    fn les_bandes_de_prise_bordent_chaque_bloc_de_colonnes_selectionnees() {
        let ids: Vec<NodeId> = (1..=5).map(NodeId::from_raw).collect();
        let extents: Vec<ColumnExtent> = ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, 100.0 * i as f32, 100.0 * (i + 1) as f32))
            .collect();
        let half = COLUMN_GRIP_HALF_WIDTH;

        let bands = column_grip_bands(&extents, &[ids[1], ids[2]], 20.0, 200.0);
        assert_eq!(bands.len(), 2, "un bloc : ses deux bords");
        assert_eq!(
            (bands[0].left(), bands[0].right()),
            (100.0 - half, 100.0 + half)
        );
        assert_eq!(
            (bands[1].left(), bands[1].right()),
            (300.0 - half, 300.0 + half)
        );
        assert_eq!((bands[0].top(), bands[0].bottom()), (20.0, 200.0));

        let bands = column_grip_bands(&extents, &[ids[0], ids[3], ids[4]], 20.0, 200.0);
        assert_eq!(bands.len(), 4, "deux blocs non contigus : deux paires");
        assert_eq!(bands[2].center().x, 300.0);
        assert_eq!(bands[3].center().x, 500.0);

        assert!(column_grip_bands(&extents, &[], 20.0, 200.0).is_empty());
    }

    /// #166 : les bandes de prise des lignes bordent chaque **bloc** de lignes
    /// sélectionnées — au-dessus de la première, au-dessous de la dernière —,
    /// dans les bornes du corps de la table ; un bord n'existe que si sa ligne
    /// est peinte ET qu'elle borde vraiment le bloc.
    #[test]
    fn les_bandes_de_prise_bordent_chaque_bloc_de_lignes_selectionnees() {
        let extents: Vec<RowExtent> = (3..8)
            .map(|row| (row, 20.0 * row as f32, 20.0 * (row + 1) as f32))
            .collect();
        let half = ROW_GRIP_HALF_HEIGHT;

        let bands = row_grip_bands(&extents, &[4, 5], 50.0, 400.0, 0.0, 1000.0);
        assert_eq!(bands.len(), 2, "un bloc : ses deux bords");
        assert_eq!(
            (bands[0].top(), bands[0].bottom()),
            (80.0 - half, 80.0 + half)
        );
        assert_eq!(
            (bands[1].top(), bands[1].bottom()),
            (120.0 - half, 120.0 + half)
        );
        assert_eq!((bands[0].left(), bands[0].right()), (50.0, 400.0));

        // Deux blocs non contigus : deux paires.
        assert_eq!(
            row_grip_bands(&extents, &[3, 5, 6], 50.0, 400.0, 0.0, 1000.0).len(),
            4
        );
        // Le bloc continue hors de l'écran (lignes 1..=4, la ligne 1 n'est pas
        // peinte) : pas de bord haut inventé, seulement le bord bas réel.
        let bands = row_grip_bands(&extents, &[1, 2, 3, 4], 50.0, 400.0, 0.0, 1000.0);
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].center().y, 100.0);
        // Une bande ne déborde pas sur l'en-tête (`top`) ni sous la table.
        let bands = row_grip_bands(&extents, &[3], 50.0, 400.0, 61.0, 1000.0);
        assert_eq!(bands[0].top(), 61.0, "bord haut rogné sous l'en-tête");
        assert!(row_grip_bands(&extents, &[], 50.0, 400.0, 0.0, 1000.0).is_empty());
    }

    /// Un bouton voisin (« avant », premier widget focalisable venu de
    /// l'image, donc celui qu'egui choisirait sur un Tab) puis un classeur de
    /// trois colonnes de quatre lignes, valeurs 101..104, 201..204, 301..304.
    struct Banc {
        _session: Session,
        context: egui::Context,
        state: AppState,
        actions: Vec<Action>,
        book: NodeId,
        sheet: NodeId,
        columns: Vec<NodeId>,
        voisin_focalise: bool,
        dernier: Option<egui::FullOutput>,
    }

    impl Banc {
        fn new() -> Self {
            let mut project = Project::new("Clavier").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let sheet = project.add_worksheet(book, None).unwrap();
            let columns = (1..=3)
                .map(|c| {
                    let valeurs: Vec<f64> = (1..=4).map(|r| f64::from(c * 100 + r)).collect();
                    project
                        .add_column(sheet, None, None, Column::from_f64(&valeurs))
                        .unwrap()
                })
                .collect();
            let session = Session::spawn(project, None).unwrap();
            let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            theme::install(&context);
            let mut banc = Banc {
                _session: session,
                context,
                state,
                actions: Vec::new(),
                book,
                sheet,
                columns,
                voisin_focalise: false,
                dernier: None,
            };
            for _ in 0..3 {
                banc.image(Vec::new());
            }
            banc
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            self.actions.clear();
            let (state, actions, book) = (&mut self.state, &mut self.actions, self.book);
            let mut voisin = false;
            let sortie = image_de(&self.context, egui::vec2(700.0, 420.0), events, |ui| {
                voisin = ui.button("avant").has_focus();
                book_contents(ui, state, actions, book);
            });
            self.voisin_focalise = voisin;
            self.dernier = Some(sortie);
        }

        fn touche(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            // `ModifiersChanged` explicite : un `Event::Key` ne met pas à jour
            // `InputState::modifiers`, que la grille lit pour Maj.
            let touche = |pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            };
            self.image(vec![egui::Event::ModifiersChanged(modifiers), touche(true)]);
            self.image(vec![
                touche(false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ]);
            // Une image de plus, sans événement : le focus rendu à un autre
            // widget par `end_pass` se verrait ici.
            self.image(Vec::new());
        }

        fn tab(&mut self) {
            self.touche(egui::Key::Tab, egui::Modifiers::NONE);
        }

        fn maj_tab(&mut self) {
            self.touche(egui::Key::Tab, egui::Modifiers::SHIFT);
        }

        /// Centre du texte peint exactement égal à `texte` à la dernière
        /// image, s'il y en a un.
        fn centre_de(&self, texte: &str) -> Option<egui::Pos2> {
            self.dernier
                .as_ref()
                .unwrap()
                .shapes
                .iter()
                .find_map(|peint| match &peint.shape {
                    egui::Shape::Text(shape) if shape.galley.job.text == texte => {
                        Some(shape.pos + shape.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
        }

        /// Tous les textes peints à la dernière image.
        fn textes(&self) -> Vec<String> {
            self.dernier
                .as_ref()
                .unwrap()
                .shapes
                .iter()
                .filter_map(|peint| match &peint.shape {
                    egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                    _ => None,
                })
                .collect()
        }

        /// Clic sur le texte peint `texte`, comme à la souris, du bouton
        /// `bouton`.
        fn clic_bouton(&mut self, texte: &str, bouton: egui::PointerButton) {
            let position = self
                .centre_de(texte)
                .unwrap_or_else(|| panic!("aucun texte peint ne porte {texte:?}"));
            self.image(vec![egui::Event::PointerMoved(position)]);
            for pressed in [true, false] {
                self.image(vec![egui::Event::PointerButton {
                    pos: position,
                    button: bouton,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            // Laisse passer les images de retombée (focus, relâchement).
            self.image(Vec::new());
        }

        /// Clic gauche sur le texte peint `texte`, `modifiers` enfoncés
        /// (`ModifiersChanged` explicite : la grille lit `InputState::modifiers`).
        fn clic_avec(&mut self, texte: &str, modifiers: egui::Modifiers) {
            let position = self
                .centre_de(texte)
                .unwrap_or_else(|| panic!("aucun texte peint ne porte {texte:?}"));
            self.image(vec![
                egui::Event::PointerMoved(position),
                egui::Event::ModifiersChanged(modifiers),
            ]);
            for pressed in [true, false] {
                self.image(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                }]);
            }
            self.image(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
            self.image(Vec::new());
        }

        /// Clic gauche sur la cellule peinte portant `texte`.
        fn clic_sur(&mut self, texte: &str) {
            self.clic_bouton(texte, egui::PointerButton::Primary);
        }

        fn curseur(&self) -> Option<GridCursor> {
            self.state
                .selection(self.sheet)
                .and_then(Selection::grid_cursor)
        }
    }

    // --- Focus clavier (#38) ---------------------------------------------
    //
    // Tab et les flèches appartiennent à la grille. egui les traite au début
    // de l'image (`Focus::begin_pass`) pour déplacer **son** focus, sauf si le
    // widget focalisé les réclame par un `EventFilter` ([`GRID_KEYS`]) : sans
    // filtre, Tab faisait sortir le focus du tableau vers le reste de
    // l'interface au lieu d'avancer la sélection de cellule en cellule.

    #[test]
    fn f2_edite_la_cellule_comme_un_double_clic() {
        let mut saisies = Vec::new();
        for clavier in [false, true] {
            let mut banc = Banc::new();
            banc.clic_sur("101");
            if clavier {
                banc.touche(egui::Key::F2, egui::Modifiers::NONE);
            } else {
                banc.clic_sur("101");
            }
            let edit = banc
                .state
                .editing()
                .unwrap_or_else(|| panic!("le geste ouvre l'éditeur (clavier : {clavier})"));
            assert_eq!(
                (edit.column, edit.row, edit.buffer.as_str()),
                (banc.columns[0], 0, "101")
            );
            assert!(
                keyboard_focus_outside_grid(&banc.context),
                "le champ a le focus"
            );
            banc.image(vec![egui::Event::Text("7".into())]);
            let saisie = banc.state.editing().unwrap().buffer.clone();
            assert_ne!(saisie, "101");
            saisies.push(saisie.clone());
            banc.image(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert!(banc.state.editing().is_none());
            assert!(banc.actions.iter().any(|action| matches!(action,
                Action::CommitCellAndAdvance { column, row: 0, text, .. }
                if *column == banc.columns[0] && text == &saisie)));
        }
        assert_eq!(
            saisies[0], saisies[1],
            "même focus et même curseur de saisie"
        );
    }

    #[test]
    fn f2_edite_chaque_champ_d_en_tete_comme_un_double_clic() {
        for field in [
            HeaderField::LongName,
            HeaderField::Unit,
            HeaderField::Comment,
            HeaderField::Formula,
        ] {
            let mut saisies = Vec::new();
            for clavier in [false, true] {
                let mut banc = Banc::new();
                let column = banc.columns[0];
                banc.state.begin_header_edit(column, field);
                banc.state.header_editing_mut().unwrap().buffer = "9".into();
                banc.state.commit_header_edit();
                banc.image(Vec::new());
                banc.clic_sur("9");
                if clavier {
                    banc.touche(egui::Key::F2, egui::Modifiers::NONE);
                } else {
                    banc.clic_sur("9");
                }
                let edit = banc
                    .state
                    .header_editing()
                    .expect("le geste ouvre le champ");
                assert_eq!(
                    (edit.column, edit.field, edit.buffer.as_str()),
                    (column, field, "9")
                );
                assert!(banc.context.text_edit_focused());
                banc.image(vec![egui::Event::Text("7".into())]);
                let saisie = banc.state.header_editing().unwrap().buffer.clone();
                assert_ne!(saisie, "9");
                banc.touche(egui::Key::Enter, egui::Modifiers::NONE);
                assert!(banc.state.header_editing().is_none());
                assert_eq!(header_field_text(&banc.state, column, field), saisie);
                saisies.push(saisie.clone());
                // Entrée peut déplacer la sélection : revenir au champ validé
                // pour éprouver ensuite son annulation, comme avec un double-clic.
                banc.state.select_header_field(banc.sheet, column, field);
                banc.touche(egui::Key::F2, egui::Modifiers::NONE);
                banc.image(vec![egui::Event::Text("8".into())]);
                assert_ne!(banc.state.header_editing().unwrap().buffer, saisie);
                banc.touche(egui::Key::Escape, egui::Modifiers::NONE);
                assert!(banc.state.header_editing().is_none());
                assert_eq!(header_field_text(&banc.state, column, field), saisie);
            }
            assert_eq!(saisies[0], saisies[1], "même saisie pour {field:?}");
        }
    }

    #[test]
    fn f2_edite_le_coin_superieur_gauche_d_une_plage_d_en_tete_inversee() {
        let mut banc = Banc::new();
        banc.clic_sur("303");
        banc.state
            .select_header_field(banc.sheet, banc.columns[2], HeaderField::Comment);
        banc.state.extend_header_field_selection(
            banc.sheet,
            banc.columns[0],
            HeaderField::LongName,
        );
        banc.touche(egui::Key::F2, egui::Modifiers::NONE);
        let edit = banc.state.header_editing().unwrap();
        assert_eq!(
            (edit.column, edit.field),
            (banc.columns[0], HeaderField::LongName)
        );
        assert!(banc.state.editing().is_none());
    }

    #[test]
    fn f2_edite_le_coin_superieur_gauche_d_une_plage_inversee() {
        let mut banc = Banc::new();
        banc.clic_sur("303");
        banc.state.extend_selection(banc.sheet, banc.columns[0], 0);
        banc.touche(egui::Key::F2, egui::Modifiers::NONE);
        let edit = banc.state.editing().unwrap();
        assert_eq!(
            (edit.column, edit.row, edit.buffer.as_str()),
            (banc.columns[0], 0, "101")
        );
    }

    #[test]
    fn f2_annule_sans_ecrire_et_ne_vole_pas_le_focus_voisin() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        banc.touche(egui::Key::F2, egui::Modifiers::NONE);
        assert!(banc.state.editing().is_some());
        banc.image(vec![egui::Event::Text("7".into())]);
        banc.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(banc.actions.is_empty(), "Échap ne soumet aucune commande");
        assert!(banc.state.editing().is_none());
        assert_eq!(banc.state.cell_text(banc.columns[0], 0), "101");
        banc.clic_sur("avant");
        banc.touche(egui::Key::F2, egui::Modifiers::NONE);
        assert!(banc.state.editing().is_none());
    }

    #[test]
    fn tab_hors_edition_avance_la_selection_sans_sortir_de_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[0], 0)),
            "le clic sélectionne la cellule"
        );
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Tab avance d'une colonne"
        );
        assert!(
            !banc.voisin_focalise,
            "Tab a fait sortir le focus de la grille vers le widget voisin"
        );
        banc.tab();
        assert_eq!(banc.curseur(), Some(GridCursor::Data(banc.columns[2], 0)));
        banc.maj_tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Maj+Tab recule"
        );
        assert!(!banc.voisin_focalise);
    }

    #[test]
    fn les_fleches_deplacent_la_selection_et_gardent_le_focus_dans_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("102");
        for (key, attendu) in [
            (egui::Key::ArrowRight, GridCursor::Data(banc.columns[1], 1)),
            (egui::Key::ArrowDown, GridCursor::Data(banc.columns[1], 2)),
            (egui::Key::ArrowLeft, GridCursor::Data(banc.columns[0], 2)),
            (egui::Key::ArrowUp, GridCursor::Data(banc.columns[0], 1)),
        ] {
            banc.touche(key, egui::Modifiers::NONE);
            assert_eq!(banc.curseur(), Some(attendu), "{key:?}");
            assert!(!banc.voisin_focalise, "{key:?} a déplacé le focus egui");
        }
    }

    #[test]
    fn tab_pendant_l_edition_valide_et_garde_le_focus_dans_la_grille() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        // Une frappe sur une cellule sélectionnée ouvre l'édition.
        banc.image(vec![egui::Event::Text("7".to_string())]);
        banc.image(Vec::new());
        banc.image(Vec::new());
        assert!(banc.state.editing().is_some(), "la frappe ouvre l'édition");
        banc.tab();
        assert!(banc.state.editing().is_none(), "Tab valide la saisie");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[1], 0)),
            "Tab valide *et* avance"
        );
        // Et la grille garde la main pour les Tab suivants, hors édition.
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Data(banc.columns[2], 0)),
            "après la validation, Tab continue d'avancer dans la grille"
        );
        assert!(!banc.voisin_focalise);
    }

    /// Tab validé dans une cellule pousse la saisie au modèle
    /// (`Action::CommitCell`) dans l'image même de la touche : la valeur tapée
    /// n'est pas perdue en chemin.
    #[test]
    fn tab_pendant_l_edition_pousse_la_saisie_dans_l_image_de_la_touche() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        banc.image(vec![egui::Event::Text("7".to_string())]);
        banc.image(Vec::new());
        banc.image(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let colonne = banc.columns[0];
        assert!(
            banc.actions.iter().any(|action| matches!(
                action,
                Action::CommitCell { column, row: 0, text } if *column == colonne && text == "7"
            )),
            "la saisie doit partir au modèle : {:?}",
            banc.actions
        );
    }

    /// #52 : Tab sur la dernière colonne ajoute une colonne quel que soit le
    /// champ d'en-tête, hors édition comme en édition — plus seulement Formule.
    #[test]
    fn tab_en_derniere_colonne_ajoute_une_colonne_pour_chaque_champ_d_en_tete() {
        for editer in [false, true] {
            for field in [
                HeaderField::LongName,
                HeaderField::Unit,
                HeaderField::Comment,
                HeaderField::Formula,
            ] {
                let mut banc = Banc::new();
                let (sheet, derniere) = (banc.sheet, banc.columns[2]);
                // Le clic donne le focus à la grille (sinon Tab n'est pas à elle).
                banc.clic_sur("301");
                banc.state.select_header_field(sheet, derniere, field);
                if editer {
                    banc.state.begin_header_edit(derniere, field);
                }
                for _ in 0..3 {
                    banc.image(Vec::new());
                }
                banc.image(vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                assert!(
                    banc.actions.iter().any(|action| matches!(
                        action,
                        Action::AddColumnAndSelectHeaderField { field: f, .. } if *f == field
                    )),
                    "Tab sur {field:?} (édition : {editer}) doit ajouter une colonne"
                );
            }
        }
    }

    #[test]
    fn tab_pendant_l_edition_d_un_champ_d_en_tete_valide_et_reste_dans_la_grille() {
        let mut banc = Banc::new();
        let (sheet, colonne) = (banc.sheet, banc.columns[0]);
        banc.state
            .select_header_field(sheet, colonne, HeaderField::LongName);
        banc.state.begin_header_edit(colonne, HeaderField::LongName);
        for _ in 0..3 {
            banc.image(Vec::new());
        }
        assert!(banc.state.header_editing().is_some());
        banc.tab();
        assert!(banc.state.header_editing().is_none(), "Tab valide le champ");
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Header(banc.columns[1], HeaderField::LongName)),
            "Tab valide *et* passe à la colonne suivante"
        );
        banc.tab();
        assert_eq!(
            banc.curseur(),
            Some(GridCursor::Header(banc.columns[2], HeaderField::LongName)),
            "puis continue hors édition"
        );
        assert!(!banc.voisin_focalise);
    }

    /// Le focus de la grille ne se retient pas : un clic ailleurs le rend, et
    /// Tab reprend alors son rôle d'egui (déplacer le focus), sans toucher la
    /// sélection de la grille.
    #[test]
    fn un_clic_hors_de_la_grille_lui_retire_tab() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        assert!(!keyboard_focus_outside_grid(&banc.context));
        assert!(
            banc.context.memory(|memory| memory.focused()).is_some(),
            "un clic dans la grille lui donne le focus clavier"
        );
        banc.clic_sur("avant");
        let avant = banc.curseur();
        banc.tab();
        assert_eq!(
            banc.curseur(),
            avant,
            "Tab, hors de la grille, ne déplace pas sa sélection"
        );
    }

    /// Le focus de la grille n'est pas un champ texte pour le reste de
    /// l'application (Entrée d'un dialogue, Suppr d'une annotation) : seul un
    /// autre widget focalisé l'est.
    #[test]
    fn le_focus_de_la_grille_n_est_pas_un_champ_en_cours_de_saisie() {
        let mut banc = Banc::new();
        assert!(!keyboard_focus_outside_grid(&banc.context), "aucun focus");
        banc.clic_sur("101");
        assert!(
            !keyboard_focus_outside_grid(&banc.context),
            "focus de la grille"
        );
        banc.context
            .memory_mut(|memory| memory.request_focus(egui::Id::new("champ_etranger")));
        assert!(keyboard_focus_outside_grid(&banc.context), "focus étranger");
    }

    /// Le focus clavier d'un champ étranger à la grille (ici un champ texte
    /// d'un dialogue) reste le sien : ni Tab ni flèche ne déplacent la
    /// sélection. Le témoin est `les_fleches_deplacent_la_selection_…` et
    /// `tab_hors_edition_…` : mêmes touches, grille focalisée, sélection
    /// déplacée.
    #[test]
    fn un_champ_etranger_focalise_garde_tab_et_les_fleches() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        let champ = egui::Id::new("champ_etranger");
        let avant = banc.curseur();
        let mut texte = String::new();
        for key in [egui::Key::ArrowRight, egui::Key::Tab] {
            // Trois images pour que le champ soit posé et focalisé, puis la touche.
            for frame in 0..4 {
                let events = if frame == 3 {
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    Vec::new()
                };
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 420.0),
                    )),
                    events,
                    ..Default::default()
                };
                let (state, actions, book) = (&mut banc.state, &mut banc.actions, banc.book);
                let mut sortie = banc.context.run_ui(input, |ui| {
                    egui::Area::new(egui::Id::new("dialogue")).show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut texte).id(champ));
                    });
                    if frame < 3 {
                        ui.ctx().memory_mut(|memory| memory.request_focus(champ));
                    }
                    book_contents(ui, state, actions, book);
                });
                sortie.textures_delta.clear();
            }
            assert_eq!(
                banc.curseur(),
                avant,
                "{key:?} : la grille a volé la touche au champ texte focalisé"
            );
        }
    }

    /// #17 : la grille reste peinte sous un dialogue (Propriétés du graphe,
    /// import…) ; tant qu'un champ texte y a le focus, Retour arrière et
    /// Suppr lui appartiennent — la grille ne doit ni les consommer ni vider
    /// la cellule sélectionnée. Témoin : le même banc sans focus au champ
    /// vide bien la cellule (sans lui, une grille sourde à Retour arrière
    /// passerait le test).
    #[test]
    fn un_champ_texte_focalise_garde_retour_arriere_et_suppr() {
        for champ_focalise in [true, false] {
            let mut project = Project::new("Clavier").unwrap();
            let root = project.root();
            let book = project.add_empty_workbook(root, None).unwrap();
            let sheet = project.add_worksheet(book, None).unwrap();
            let column = project
                .add_column(sheet, None, None, Column::from_f64(&[1.0, 2.0]))
                .unwrap();
            let session = Session::spawn(project, None).unwrap();
            let mut state =
                AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
            state.select_cell(sheet, column, 0);
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut titre = String::from("Titre");
            let mut actions = Vec::new();
            for frame in 0..3 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                };
                if frame == 2 {
                    for key in [egui::Key::Backspace, egui::Key::Delete] {
                        input.events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(input, |ui| {
                    let field = egui::Id::new("champ_dialogue");
                    egui::Area::new(egui::Id::new("dialogue")).show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut titre).id(field));
                    });
                    if champ_focalise {
                        ui.ctx().memory_mut(|memory| memory.request_focus(field));
                    }
                    actions.clear();
                    grid_contents(ui, &mut state, &mut actions, sheet, None);
                });
                output.textures_delta.clear();
            }
            let vide = actions.iter().any(|action| {
                matches!(
                    action,
                    Action::ClearSelectedCells { .. } | Action::DeleteSelectedCells { .. }
                )
            });
            if champ_focalise {
                assert!(
                    !vide,
                    "la grille a volé Retour arrière/Suppr au champ texte : {actions:?}"
                );
            } else {
                assert!(
                    vide,
                    "témoin : sans champ focalisé, Retour arrière vide la cellule : {actions:?}"
                );
            }
        }
    }

    // --- En-tête empilé et gouttière --------------------------------------

    /// La ligne « Metadata » est affichée par défaut, se masque et se réaffiche
    /// (réglage de vue partagé par tous les viewports), et l'en-tête empilé
    /// perd exactement une sous-ligne quand elle est masquée.
    #[test]
    fn la_ligne_metadata_se_masque_et_se_reaffiche() {
        let context = egui::Context::default();
        assert!(metadata_row_visible(&context));
        set_metadata_row_visible(&context, false);
        assert!(!metadata_row_visible(&context));
        assert_eq!(
            stacked_header_height(true) - stacked_header_height(false),
            HEADER_SUBROW_HEIGHT
        );
        set_metadata_row_visible(&context, true);
        assert!(metadata_row_visible(&context));
    }

    /// L'en-tête empilé, tel que peint : la gouttière porte « # » sur la
    /// rangée des lettres, puis chaque libellé de champ à la hauteur de sa
    /// rangée (nom long un peu plus haut que les autres), la lettre de
    /// colonne et son rôle sur la même rangée que « # », la première ligne
    /// de données dessous. Masquer la ligne « Metadata » retire son libellé
    /// et remonte les données d'une sous-rangée exactement — le témoin que la
    /// position lue suit bien la hauteur de l'en-tête.
    #[test]
    fn l_en_tete_empile_aligne_ses_libelles_sur_ses_rangees() {
        let mut banc = Banc::new();
        let lettre = format!(
            "A({})",
            role_label(banc.state.column_meta(banc.columns[0]).role)
        );
        let libelles = [
            "#".to_string(),
            tr!("column-x-prime"),
            tr!("column-long-name"),
            tr!("column-units"),
            tr!("column-comments"),
            tr!("workbook-header-formula"),
            tr!("common-metadata"),
            "1".to_string(),
        ];
        let y: Vec<f32> = libelles
            .iter()
            .map(|libelle| {
                banc.centre_de(libelle)
                    .unwrap_or_else(|| panic!("{libelle:?} absent : {:?}", banc.textes()))
                    .y
            })
            .collect();
        // Écart entre centres de deux rangées voisines : demi-hauteur de l'une
        // plus demi-hauteur de l'autre.
        let ecarts = [
            HEADER_SUBROW_HEIGHT,
            (HEADER_SUBROW_HEIGHT + HEADER_LONGNAME_HEIGHT) / 2.0,
            (HEADER_LONGNAME_HEIGHT + HEADER_SUBROW_HEIGHT) / 2.0,
            HEADER_SUBROW_HEIGHT,
            HEADER_SUBROW_HEIGHT,
            HEADER_SUBROW_HEIGHT,
        ];
        for (index, ecart) in ecarts.iter().enumerate() {
            let mesure = y[index + 1] - y[index];
            assert!(
                (mesure - ecart).abs() <= 0.5,
                "{:?} → {:?} : écart {mesure}, attendu {ecart}",
                libelles[index],
                libelles[index + 1]
            );
        }
        assert!(
            y[7] > y[6],
            "la première ligne de données est sous l'en-tête"
        );
        let y_lettre = banc
            .centre_de(&lettre)
            .unwrap_or_else(|| panic!("{lettre:?} absent : {:?}", banc.textes()))
            .y;
        assert!(
            (y_lettre - y[0]).abs() <= 0.5,
            "la lettre {lettre:?} et « # » partagent la rangée des lettres"
        );

        set_metadata_row_visible(&banc.context, false);
        banc.image(Vec::new());
        banc.image(Vec::new());
        assert!(
            banc.centre_de(&tr!("common-metadata")).is_none(),
            "la ligne « Metadata » masquée ne peint plus son libellé"
        );
        let y_donnees = banc.centre_de("1").expect("première ligne").y;
        assert!(
            (y[7] - y_donnees - HEADER_SUBROW_HEIGHT).abs() <= 0.5,
            "les données remontent d'une sous-rangée : {} → {y_donnees}",
            y[7]
        );
    }

    /// #125 : les traductions longues restent dans leur rangée, même
    /// lorsque la gouttière est rétrécie à sa largeur minimale.
    #[test]
    fn les_libelles_de_gouttiere_restent_sur_une_seule_ligne() {
        use crate::i18n::{build_loader, Language};
        let mut banc = Banc::new();
        for language in [Language::English, Language::French, Language::PseudoEnglish] {
            let loader = build_loader(language);
            let libelles = [
                tr!(in loader; "column-x-prime"),
                tr!(in loader; "column-long-name"),
                tr!(in loader; "column-units"),
                tr!(in loader; "column-comments"),
                tr!(in loader; "workbook-header-formula"),
                tr!(in loader; "common-metadata"),
            ];
            for width in [ROW_NUMBER_COLUMN_WIDTH, MIN_ROW_NUMBER_COLUMN_WIDTH] {
                let mut output = banc.context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_width(width);
                        row_number_header_widget(ui, &mut banc.state, &mut Vec::new(), banc.sheet);
                        // Les libellés de champ empruntent exactement le même widget.
                        for label in &libelles {
                            header_row_label_widget(
                                ui,
                                &mut banc.state,
                                &mut Vec::new(),
                                banc.sheet,
                                HeaderField::LongName,
                                HEADER_LONGNAME_HEIGHT,
                                label,
                            );
                        }
                    },
                );
                output.textures_delta.clear();
                let mut found = 0;
                for painted in &output.shapes {
                    if let egui::Shape::Text(text) = &painted.shape {
                        if libelles.contains(&text.galley.text().to_string())
                            || text.galley.text() == tr!("common-metadata")
                        {
                            found += 1;
                            assert_eq!(
                                text.galley.rows.len(),
                                1,
                                "{language:?}, {width} px : {:?}",
                                text.galley.text()
                            );
                        }
                    }
                }
                assert!(
                    found >= 6,
                    "les cinq traductions et la rangée Metadata sont réellement peintes"
                );
            }
        }
    }

    /// Le fond de la gouttière a trois paliers distincts, et « sélectionnée »
    /// l'emporte sur « concernée par la sélection ».
    #[test]
    fn le_fond_de_la_gouttiere_a_trois_paliers() {
        let repos = gutter_fill(false, false);
        let touchee = gutter_fill(false, true);
        let choisie = gutter_fill(true, false);
        assert_ne!(repos, touchee);
        assert_ne!(repos, choisie);
        assert_ne!(touchee, choisie);
        assert_eq!(gutter_fill(true, true), choisie);
    }

    /// Chaque type et chaque rôle proposé par le menu de la rangée des
    /// lettres a un libellé non vide et distinct : deux entrées au même nom
    /// rendraient le choix ambigu.
    #[test]
    fn les_libelles_de_type_et_de_role_sont_distincts() {
        let types: Vec<String> = [ColumnType::Numeric, ColumnType::DateTime, ColumnType::Mixed]
            .into_iter()
            .map(type_label)
            .collect();
        let roles: Vec<String> = [
            ColumnRole::X,
            ColumnRole::Y,
            ColumnRole::X2,
            ColumnRole::Y2,
            ColumnRole::Z,
            ColumnRole::XError,
            ColumnRole::YError,
            ColumnRole::Label,
            ColumnRole::None,
        ]
        .into_iter()
        .map(role_label)
        .collect();
        for libelles in [&types, &roles] {
            let distincts: std::collections::HashSet<&String> = libelles.iter().collect();
            assert_eq!(distincts.len(), libelles.len(), "{libelles:?}");
            assert!(libelles.iter().all(|libelle| !libelle.is_empty()));
        }
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(26), "AA");
    }

    // --- Menu d'édition ---------------------------------------------------

    /// Le même bloc d'édition au clic droit sur une cellule et sur un
    /// libellé de champ de la gouttière, à une différence près : « Supprimer
    /// et décaler » n'existe pas dans l'en-tête, où Suppr n'a rien à faire
    /// remonter. Un clic droit hors de la sélection la réduit d'abord à la
    /// case visée.
    #[test]
    fn le_menu_d_edition_est_le_meme_sur_cellule_et_en_tete_sauf_supprimer_et_decaler() {
        let communs = [
            tr!("action-cut"),
            tr!("action-copy"),
            tr!("action-copy-with-header"),
            tr!("action-paste"),
            tr!("action-paste-in-place"),
            tr!("action-clear"),
        ];
        let decaler = tr!("action-delete-shift-up");

        let mut cellule = Banc::new();
        cellule.clic_bouton("102", egui::PointerButton::Secondary);
        let textes = cellule.textes();
        for entree in communs.iter().chain([&decaler]) {
            assert!(
                textes.contains(entree),
                "cellule : {entree:?} absent de {textes:?}"
            );
        }
        for absent in [tr!("history-undo"), tr!("history-redo")] {
            assert!(
                !textes.contains(&absent),
                "Undo et Redo restent à Edit et au clavier : {textes:?}"
            );
        }
        assert_eq!(
            cellule.curseur(),
            Some(GridCursor::Data(cellule.columns[0], 1)),
            "le clic droit sélectionne la cellule visée"
        );

        let mut en_tete = Banc::new();
        en_tete.clic_bouton(&tr!("column-units"), egui::PointerButton::Secondary);
        let textes = en_tete.textes();
        for entree in &communs {
            assert!(
                textes.contains(entree),
                "en-tête : {entree:?} absent de {textes:?}"
            );
        }
        assert!(
            !textes.contains(&decaler),
            "« Supprimer et décaler » n'a rien à faire dans l'en-tête : {textes:?}"
        );
    }

    /// Sur une sélection de colonnes (ou de lignes) entières, le clic droit
    /// d'une cellule est celui des lettres : Plot, type et rôle comprises ; sur
    /// une cellule seule, il reste celui de l'édition (témoin).
    #[test]
    fn le_clic_droit_d_une_selection_de_colonnes_ou_de_lignes_est_celui_des_lettres() {
        let propres_aux_lettres = [tr!("workbook-plot-menu"), tr!("column-type")];

        let mut colonnes = Banc::new();
        colonnes
            .state
            .select_column(colonnes.sheet, colonnes.columns[0]);
        colonnes.image(Vec::new());
        colonnes.clic_bouton("102", egui::PointerButton::Secondary);
        let textes = colonnes.textes();
        for entree in propres_aux_lettres
            .iter()
            .chain([&tr!("action-copy"), &tr!("action-clear")])
        {
            assert!(
                textes.contains(entree),
                "colonne sélectionnée : {entree:?} absent de {textes:?}"
            );
        }
        assert!(
            colonnes
                .state
                .is_column_selected(colonnes.sheet, colonnes.columns[0]),
            "le clic droit dans la sélection la laisse intacte"
        );

        let mut lignes = Banc::new();
        lignes.state.select_row(lignes.sheet, 1);
        lignes.image(Vec::new());
        lignes.clic_bouton("102", egui::PointerButton::Secondary);
        let textes = lignes.textes();
        for entree in &propres_aux_lettres {
            assert!(
                textes.contains(entree),
                "ligne sélectionnée : {entree:?} absent de {textes:?}"
            );
        }

        let mut cellule = Banc::new();
        cellule.clic_bouton("102", egui::PointerButton::Secondary);
        let textes = cellule.textes();
        for entree in &propres_aux_lettres {
            assert!(
                !textes.contains(entree),
                "témoin : une cellule seule n'a pas {entree:?} : {textes:?}"
            );
        }
    }

    /// Choisir « Effacer » dans le menu d'une cellule pousse le vidage de la
    /// sélection, comme Retour arrière.
    #[test]
    fn effacer_dans_le_menu_d_une_cellule_vide_la_selection() {
        let mut banc = Banc::new();
        banc.clic_bouton("102", egui::PointerButton::Secondary);
        let position = banc.centre_de(&tr!("action-clear")).expect("menu ouvert");
        let sheet = banc.sheet;
        banc.image(vec![egui::Event::PointerMoved(position)]);
        banc.image(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        banc.image(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(
            banc.actions.iter().any(
                |action| matches!(action, Action::ClearSelectedCells { sheet: s } if *s == sheet)
            ),
            "{:?}",
            banc.actions
        );
    }

    // --- Barres de défilement ---------------------------------------------

    #[test]
    fn la_barre_verticale_de_la_grille_est_a_lechelle_du_corps_de_la_table() {
        assert_eq!(grid_vertical_bar_scale(320.0, 160.0), 2.0);
        assert_eq!(grid_vertical_bar_scale(320.0, 0.0), 1.0);
    }

    /// Largeur de contenu, largeur visible et décalage horizontal de la zone
    /// défilable autour d'une table de `colonnes` colonnes de 110 points, sur
    /// un écran de 220, `decalage` étant celui qu'aurait demandé la barre
    /// déportée à l'image précédente.
    fn zone_defilable(colonnes: usize, decalage: Option<f32>) -> (f32, f32, f32) {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(220.0, 180.0),
            )),
            ..Default::default()
        };
        let mut scroll = None;
        let mut output = context.run_ui(input, |ui| {
            scroll = Some(show_horizontally_scrollable_grid(ui, decalage, |ui| {
                TableBuilder::new(ui)
                    .columns(TableColumn::initial(110.0).at_least(56.0), colonnes)
                    .auto_shrink([false, false])
                    .header(20.0, |mut header| {
                        for _ in 0..colonnes {
                            header.col(|ui| {
                                ui.label("Colonne");
                            });
                        }
                    })
                    .body(|_| {});
            }));
        });
        output.textures_delta.clear();
        let scroll = scroll.expect("la grille doit être dessinée");
        (
            scroll.content_size.x,
            scroll.inner_rect.width(),
            scroll.state.offset.x,
        )
    }

    /// Des colonnes plus larges que l'écran débordent dans la zone
    /// défilable ; une seule colonne étroite n'y déborde pas (témoin : la
    /// mesure distingue bien les deux cas).
    #[test]
    fn la_grille_deborde_horizontalement_dans_sa_zone_defilable() {
        let (contenu, visible, _) = zone_defilable(5, None);
        assert!(
            contenu > visible,
            "les colonnes doivent pouvoir défiler dans le rectangle visible : {contenu:?} > {visible:?}",
        );
        let (contenu, visible, _) = zone_defilable(1, None);
        assert!(
            contenu <= visible + 0.5,
            "témoin : une colonne tient dans l'écran : {contenu:?} <= {visible:?}",
        );
    }

    /// Le décalage demandé par la barre horizontale déportée (sous les
    /// onglets de feuille) est appliqué à la grille ; sans débordement, il
    /// n'y a rien à décaler (témoin : le décalage lu n'est pas un simple
    /// écho de la demande).
    #[test]
    fn le_decalage_de_la_barre_deportee_s_applique_a_la_grille() {
        let (_, _, decalage) = zone_defilable(5, Some(60.0));
        assert!(
            (decalage - 60.0).abs() < 0.5,
            "la grille doit défiler de 60 points : {decalage}"
        );
        let (_, _, decalage) = zone_defilable(1, Some(60.0));
        assert!(
            decalage.abs() < 0.5,
            "témoin : rien à faire défiler, décalage {decalage}"
        );
    }
    /// Le clic droit dans une plage de colonnes la conserve. Un changement
    /// de type vise chacune de ces colonnes une fois, sans toucher la voisine.
    #[test]
    fn le_type_dans_le_menu_des_lettres_vise_toute_la_selection() {
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.state
            .extend_column_selection(banc.sheet, banc.columns[1]);
        banc.state.dispatch(Action::SetColumnType {
            column: banc.columns[0],
            column_type: ColumnType::Mixed,
        });
        banc.image(Vec::new());
        let lettre = format!("B({})", role_label(ColumnRole::Y));
        banc.clic_bouton(&lettre, egui::PointerButton::Secondary);
        assert!(banc.state.is_column_selected(banc.sheet, banc.columns[0]));
        assert!(banc.state.is_column_selected(banc.sheet, banc.columns[1]));
        assert!(
            !banc.state.is_column_selected(banc.sheet, banc.columns[2]),
            "témoin : la voisine n'est pas sélectionnée"
        );
        assert!(banc.centre_de(&type_label(ColumnType::Numeric)).is_none());
        banc.clic_sur(&tr!("column-type"));
        let position = banc
            .centre_de(&type_label(ColumnType::Numeric))
            .expect("menu du type ouvert");
        banc.image(vec![egui::Event::PointerMoved(position)]);
        for pressed in [true, false] {
            banc.image(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        assert_eq!(
            banc.actions,
            vec![Action::SetColumnsType {
                columns: banc.columns[..2].to_vec(),
                column_type: ColumnType::Numeric,
            }],
            "une seule action, sur les deux colonnes sélectionnées, pas la voisine"
        );
    }

    // --- Sélection multiple (#159) ------------------------------------------

    fn lettre(banc: &Banc, index: usize) -> String {
        let role = banc.state.column_meta(banc.columns[index]).role;
        format!("{}({})", column_letter(index), role_label(role))
    }

    #[test]
    fn ctrl_clic_sur_les_lettres_pose_une_selection_non_contigue() {
        let mut banc = Banc::new();
        let (a, c) = (lettre(&banc, 0), lettre(&banc, 2));
        banc.clic_sur(&a);
        banc.clic_avec(&c, egui::Modifiers::COMMAND);
        let sheet = banc.sheet;
        assert!(banc.state.is_column_selected(sheet, banc.columns[0]));
        assert!(!banc.state.is_column_selected(sheet, banc.columns[1]));
        assert!(banc.state.is_column_selected(sheet, banc.columns[2]));
        // Un second Ctrl+clic retire la colonne.
        banc.clic_avec(&c, egui::Modifiers::COMMAND);
        assert!(!banc.state.is_column_selected(sheet, banc.columns[2]));
        assert!(banc.state.is_column_selected(sheet, banc.columns[0]));
        // Un clic simple remplace tout.
        banc.clic_avec(&c, egui::Modifiers::COMMAND);
        banc.clic_sur(&lettre(&banc, 1));
        assert!(!banc.state.is_column_selected(sheet, banc.columns[0]));
        assert!(banc.state.is_column_selected(sheet, banc.columns[1]));
    }

    #[test]
    fn maj_clic_remplace_et_ctrl_maj_clic_ajoute_une_plage() {
        let mut banc = Banc::new();
        let sheet = banc.sheet;
        let (a, b, c) = (lettre(&banc, 0), lettre(&banc, 1), lettre(&banc, 2));
        banc.clic_sur(&a);
        banc.clic_avec(&b, egui::Modifiers::COMMAND);
        // Maj+clic : plage ancre (B) → C, A disparaît.
        banc.clic_avec(&c, egui::Modifiers::SHIFT);
        assert!(!banc.state.is_column_selected(sheet, banc.columns[0]));
        assert!(banc.state.is_column_selected(sheet, banc.columns[1]));
        assert!(banc.state.is_column_selected(sheet, banc.columns[2]));
        // Ctrl+Maj+clic : l'ajoute aux colonnes déjà posées.
        banc.clic_sur(&a);
        banc.clic_avec(&c, egui::Modifiers::COMMAND);
        banc.clic_avec(&b, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        for column in &banc.columns {
            assert!(banc.state.is_column_selected(sheet, *column));
        }
    }

    #[test]
    fn la_case_du_coin_selectionne_tout_puis_deselectionne() {
        let mut banc = Banc::new();
        let sheet = banc.sheet;
        banc.clic_sur("#");
        assert!(banc.state.is_all_selected(sheet));
        banc.clic_sur("#");
        assert!(banc.state.selection(sheet).is_none());
    }

    #[test]
    fn ctrl_a_dans_la_grille_pousse_select_all_et_echap_deselectionne() {
        let mut banc = Banc::new();
        banc.clic_sur("101");
        banc.image(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
        ]);
        assert!(banc
            .actions
            .contains(&Action::SelectAll { sheet: banc.sheet }));
        banc.image(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
        banc.touche(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(banc.state.selection(banc.sheet).is_none());
    }

    #[test]
    fn ctrl_clic_sur_les_cellules_et_les_lignes_compose_une_selection_multiple() {
        let mut banc = Banc::new();
        let sheet = banc.sheet;
        banc.clic_sur("101");
        banc.clic_avec("303", egui::Modifiers::COMMAND);
        assert!(banc.state.is_selected(sheet, banc.columns[0], 0));
        assert!(banc.state.is_selected(sheet, banc.columns[2], 2));
        assert!(!banc.state.is_selected(sheet, banc.columns[1], 1));
        // Ctrl+clic sur une cellule déjà sélectionnée la retire.
        banc.clic_avec("101", egui::Modifiers::COMMAND);
        assert!(!banc.state.is_selected(sheet, banc.columns[0], 0));
        assert!(banc.state.is_selected(sheet, banc.columns[2], 2));
        // Les numéros de ligne : une seule sorte à la fois.
        banc.clic_sur("1");
        banc.clic_avec("3", egui::Modifiers::COMMAND);
        assert!(banc.state.is_row_selected(sheet, 0));
        assert!(!banc.state.is_row_selected(sheet, 1));
        assert!(banc.state.is_row_selected(sheet, 2));
        assert!(!banc.state.is_selected(sheet, banc.columns[2], 1));
    }

    // --- Glisser de colonnes vers un graphe (#160) ---------------------------

    /// Bord droit du bloc : un point à 1 pt à l'intérieur de la dernière
    /// colonne (les bandes de prise débordent de 3 pt de part et d'autre).
    fn bord_droit(banc: &Banc, derniere: usize, y: f32) -> egui::Pos2 {
        let lettre = lettre(banc, derniere);
        let centre = banc.centre_de(&lettre).expect("lettre peinte");
        egui::pos2(centre.x + DEFAULT_COLUMN_WIDTH / 2.0 - 1.0, y)
    }

    fn charge_posee(banc: &Banc) -> bool {
        egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(&banc.context)
    }

    /// Appuie en `depart` puis glisse de 30 pt vers la droite.
    fn glisse_depuis(banc: &mut Banc, depart: egui::Pos2) {
        banc.image(vec![egui::Event::PointerMoved(depart)]);
        banc.image(vec![egui::Event::PointerButton {
            pos: depart,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for pas in 1..=3 {
            banc.image(vec![egui::Event::PointerMoved(
                depart + egui::vec2(10.0 * pas as f32, 0.0),
            )]);
        }
    }

    #[test]
    fn attraper_le_bord_d_un_bloc_de_colonnes_emporte_toute_la_selection() {
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.state
            .extend_column_selection(banc.sheet, banc.columns[1]);
        banc.image(Vec::new());
        let y = banc.centre_de("101").expect("cellule peinte").y;
        let depart = bord_droit(&banc, 1, y);

        glisse_depuis(&mut banc, depart);

        assert!(charge_posee(&banc), "le bord du bloc démarre le glisser");
        let charge =
            egui::DragAndDrop::payload::<ColumnDragPayload>(&banc.context).expect("charge posée");
        assert_eq!(charge.sheet, banc.sheet);
        assert_eq!(charge.columns, banc.columns[..2]);
    }

    #[test]
    fn echap_annule_le_glisser_de_colonnes() {
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.image(Vec::new());
        let y = banc.centre_de("101").unwrap().y;
        let depart = bord_droit(&banc, 0, y);
        glisse_depuis(&mut banc, depart);
        assert!(charge_posee(&banc));

        banc.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        assert!(!charge_posee(&banc));
    }

    #[test]
    fn des_cellules_ou_le_milieu_d_une_colonne_ne_demarrent_pas_le_glisser() {
        // Une sélection de cellules n'offre pas le geste, même au bord.
        let mut banc = Banc::new();
        banc.clic_sur("101");
        let y = banc.centre_de("101").unwrap().y;
        let depart = bord_droit(&banc, 0, y);
        glisse_depuis(&mut banc, depart);
        assert!(!charge_posee(&banc), "cellules sélectionnées");

        // Une colonne entière, mais loin de son bord : le glisser de cellules
        // reste celui de l'extension de sélection.
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.image(Vec::new());
        let milieu = banc.centre_de("201").expect("cellule peinte");
        glisse_depuis(&mut banc, milieu);
        assert!(!charge_posee(&banc), "milieu de colonne");
        assert!(
            banc.state.is_selected(banc.sheet, banc.columns[1], 0)
                || banc.state.selection(banc.sheet).is_some(),
            "le glisser a étendu ou posé une sélection de cellules"
        );
    }

    #[test]
    fn la_rangee_des_lettres_garde_le_redimensionnement() {
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.image(Vec::new());
        let lettre = banc.centre_de(&lettre(&banc, 0)).expect("lettre peinte");
        let depart = bord_droit(&banc, 0, lettre.y);
        glisse_depuis(&mut banc, depart);
        assert!(
            !charge_posee(&banc),
            "le bord dans la rangée des lettres redimensionne, il n'attrape pas"
        );
    }

    // --- Glisser de lignes vers un graphe (#166) ------------------------------

    fn charge_de_lignes_posee(banc: &Banc) -> bool {
        egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(&banc.context)
    }

    /// Donne une valeur x' à la première colonne du banc.
    fn pose_x_prime(banc: &mut Banc, text: &str) {
        banc.state
            .begin_header_edit(banc.columns[0], HeaderField::XPrime);
        if let Some(edit) = banc.state.header_editing_mut() {
            edit.buffer = text.to_string();
        }
        banc.state.commit_header_edit();
    }

    /// Un point à 1 pt au-dessus du bord bas de la ligne `row`, au milieu de la
    /// colonne `column`.
    fn bord_bas(banc: &Banc, cell: &str) -> egui::Pos2 {
        let centre = banc.centre_de(cell).expect("cellule peinte");
        egui::pos2(centre.x, centre.y + ROW_HEIGHT / 2.0 - 1.0)
    }

    #[test]
    fn attraper_le_bord_d_un_bloc_de_lignes_emporte_toute_la_selection() {
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.state.select_row(banc.sheet, 0);
        banc.state.extend_row_selection(banc.sheet, 1);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "202");

        glisse_depuis(&mut banc, depart);

        assert!(
            charge_de_lignes_posee(&banc),
            "le bord du bloc démarre le glisser"
        );
        assert!(!charge_posee(&banc), "ce n'est pas un glisser de colonnes");
        let charge =
            egui::DragAndDrop::payload::<RowDragPayload>(&banc.context).expect("charge posée");
        assert_eq!(
            (charge.sheet, charge.rows.clone()),
            (banc.sheet, vec![0, 1])
        );
    }

    #[test]
    fn echap_annule_le_glisser_de_lignes() {
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.state.select_row(banc.sheet, 0);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "201");
        glisse_depuis(&mut banc, depart);
        assert!(charge_de_lignes_posee(&banc));

        banc.image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        assert!(!charge_de_lignes_posee(&banc));
    }

    #[test]
    fn sans_ligne_x_prime_ou_loin_du_bord_le_glisser_de_lignes_n_existe_pas() {
        // Sans valeur x' : le geste n'est pas offert, même au bord.
        let mut banc = Banc::new();
        banc.state.select_row(banc.sheet, 0);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "201");
        glisse_depuis(&mut banc, depart);
        assert!(!charge_de_lignes_posee(&banc), "sans x'");

        // Avec x', mais au milieu de la ligne : c'est l'extension de sélection.
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.state.select_row(banc.sheet, 0);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let milieu = banc.centre_de("201").expect("cellule peinte");
        glisse_depuis(&mut banc, milieu);
        assert!(!charge_de_lignes_posee(&banc), "milieu de ligne");
    }

    /// Un glisser rapide : le pointeur quitte la bande et traverse d'autres
    /// cellules dès la première image de mouvement. L'extension de sélection au
    /// survol ne doit pas défaire la sélection de lignes avant que le glisser
    /// ne démarre.
    #[test]
    fn un_glisser_de_lignes_rapide_garde_la_selection_et_emporte_les_lignes() {
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.state.select_row(banc.sheet, 0);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "201");
        banc.image(vec![egui::Event::PointerMoved(depart)]);
        banc.image(vec![egui::Event::PointerButton {
            pos: depart,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for pas in 1..=4 {
            banc.image(vec![egui::Event::PointerMoved(
                depart + egui::vec2(60.0 * pas as f32, 25.0 * pas as f32),
            )]);
        }
        assert!(charge_de_lignes_posee(&banc), "le glisser rapide a démarré");
    }

    /// Le geste complet de l'utilisateur : clic sur le numéro de ligne, puis
    /// prise du bord bas de la ligne sélectionnée.
    #[test]
    fn clic_sur_le_numero_de_ligne_puis_prise_du_bord() {
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.clic_sur("2");
        assert!(banc.state.is_row_selected(banc.sheet, 1));
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "202");
        glisse_depuis(&mut banc, depart);
        assert!(charge_de_lignes_posee(&banc));
    }

    /// Pendant le glisser de lignes, survoler d'autres cellules ne remplace pas
    /// la sélection de lignes par une sélection de cellules.
    #[test]
    fn le_glisser_de_lignes_garde_la_selection_de_lignes() {
        let mut banc = Banc::new();
        pose_x_prime(&mut banc, "5");
        banc.state.select_row(banc.sheet, 1);
        banc.image(Vec::new());
        banc.image(Vec::new());
        let depart = bord_bas(&banc, "202");
        banc.image(vec![egui::Event::PointerMoved(depart)]);
        banc.image(vec![egui::Event::PointerButton {
            pos: depart,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let cible = banc.centre_de("303").expect("cellule peinte");
        for pas in 1..=4 {
            let t = pas as f32 / 4.0;
            banc.image(vec![egui::Event::PointerMoved(depart.lerp(cible, t))]);
            banc.image(Vec::new());
        }
        assert!(charge_de_lignes_posee(&banc));
        assert!(
            banc.state.is_row_selected(banc.sheet, 1),
            "la ligne reste sélectionnée pendant le glisser"
        );
    }

    /// Même garantie pour un glisser de colonnes.
    #[test]
    fn le_glisser_de_colonnes_garde_la_selection_de_colonnes() {
        let mut banc = Banc::new();
        banc.state.select_column(banc.sheet, banc.columns[0]);
        banc.image(Vec::new());
        let y = banc.centre_de("101").expect("cellule peinte").y;
        let depart = bord_droit(&banc, 0, y);
        banc.image(vec![egui::Event::PointerMoved(depart)]);
        banc.image(vec![egui::Event::PointerButton {
            pos: depart,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let cible = banc.centre_de("303").expect("cellule peinte");
        for pas in 1..=4 {
            let t = pas as f32 / 4.0;
            banc.image(vec![egui::Event::PointerMoved(depart.lerp(cible, t))]);
            banc.image(Vec::new());
        }
        assert!(charge_posee(&banc));
        assert!(banc.state.is_column_selected(banc.sheet, banc.columns[0]));
    }
}
