//! Cadre commun des fenêtres de contenu : barre de titre (`window_header`),
//! cadre et contour (`floating_window_frame`), poignées de redimensionnement,
//! et la pose d'une fenêtre dans une couche egui (`show_window_in_layer`).
//!
//! Déplacé tel quel d'`app.rs` (#100, étape 1 ; la suite — placement, pile,
//! détachement, masquage, maximisation — est #101), sans changement de
//! comportement : c'est la brique que partagent les fenêtres flottantes, les
//! fenêtres détachées et les dialogues modaux. Sous-module d'`app` pour garder
//! l'accès à ses types privés (`Tab`, constantes d'icônes…).

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole, ICON_SIZE};

use super::tab::Tab;

/// Hauteur de la bande d'en-tête peinte par [`window_header`] — partagée
/// avec le calcul de la taille repliée d'une fenêtre flottante (B3,
/// `retours-2026-09-23-bugs.md`) : un contour ou une taille
/// forcée qui ne reprend pas cette même valeur redevient trop grand dès que
/// le contenu est masqué.
pub(super) const WINDOW_HEADER_HEIGHT: f32 = 28.0;

/// Marge intérieure du `Frame` d'une fenêtre de contenu, flottante ou
/// détachée — voir [`floating_window_frame`] et le `Frame` construit dans
/// [`StokesForgeApp::show_detached_tabs`]. Sans marge verticale : la bande
/// d'en-tête (`WINDOW_HEADER_HEIGHT`) et les séparateurs du contenu
/// fournissent déjà leur propre espacement, la marge par défaut d'egui
/// (`Frame::window`/`Frame::central_panel`) n'ajoutait donc qu'un bandeau
/// blanc mort au-dessus et en dessous, sans rôle. La marge horizontale reste
/// pour ne pas coller le texte au bord.
pub(super) const WINDOW_CONTENT_MARGIN: egui::Margin = egui::Margin {
    left: 6,
    right: 6,
    top: 0,
    bottom: 0,
};

/// Trait du contour de surbrillance — la couleur de sélection du thème pour
/// la fenêtre active, le trait par défaut d'egui sinon. Partagé entre
/// [`paint_active_window_outline`] (fenêtres flottantes internes et fenêtres
/// détachées, B1/B3, B7) : même logique de surbrillance des deux côtés, seule
/// la notion de « active » diffère (`is_active` calculé depuis `active_tab`
/// pour les unes, depuis le focus OS du viewport pour les autres).
pub(super) fn active_window_stroke(style: &egui::Style, is_active: bool) -> egui::Stroke {
    if is_active {
        style.visuals.selection.stroke
    } else {
        style.visuals.window_stroke()
    }
}

/// `Frame` d'une fenêtre flottante — le `Frame::window` habituel, sans
/// recoloration selon l'activité. [`show_floating_window`] le peint sur le
/// rectangle de la fenêtre ; l'état actif est peint par-dessus, sur ce même
/// rectangle, par [`paint_active_window_outline`].
pub(super) fn floating_window_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style).inner_margin(WINDOW_CONTENT_MARGIN)
}

/// Peint le contour de surbrillance d'une fenêtre active sur `rect`, le
/// rectangle même de la fenêtre. Les appelants passent un `Painter` de la
/// bonne couche : celle de la fenêtre côté MDI ([`show_floating_window`]),
/// la couche de fond du viewport côté fenêtre OS détachée.
pub(super) fn paint_active_window_outline(
    painter: &egui::Painter,
    rect: egui::Rect,
    corner_radius: egui::CornerRadius,
    style: &egui::Style,
) {
    painter.rect_stroke(
        rect,
        corner_radius,
        active_window_stroke(style, true),
        egui::StrokeKind::Inside,
    );
}

#[derive(Default)]
pub(super) struct WindowHeaderActions {
    pub(super) maximize: bool,
    /// Détacher (placement [`HeaderPlacement::Floating`]) ou réattacher
    /// (placement [`HeaderPlacement::Detached`]) — le glyphe et l'info-bulle
    /// changent avec `placement`, mais c'est toujours l'appelant qui
    /// interprète ce booléen selon le contexte d'où il peint l'en-tête.
    pub(super) detach: bool,
    pub(super) close: bool,
    pub(super) toggle_collapse: bool,
    /// Delta de glissé du titre pendant l'image — utilisé tel quel par
    /// l'appelant MDI (`window_positions`). Sans effet pratique pour
    /// l'appelant détaché, qui préfère `title_drag_started`.
    pub(super) drag_delta: egui::Vec2,
    /// Vrai à l'image où le glissé du titre démarre — c'est celui que
    /// l'appelant détaché utilise pour déclencher
    /// `ViewportCommand::StartDrag` une seule fois, la fenêtre OS prenant
    /// ensuite la main sur le geste (`drag_delta` ne serait plus fiable une
    /// fois l'OS aux commandes du pointeur).
    pub(super) title_drag_started: bool,
}

/// D'où [`window_header`] est peint — pilote la couleur des icônes, le
/// glyphe « détacher »/« réattacher » et quels boutons apparaissent. Deux
/// contextes, une seule fonction : c'est la barre elle-même qui doit être
/// identique, pas seulement son apparence générale (voir la doc de tête du
/// module, § détachement générique).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HeaderPlacement {
    /// Fenêtre flottante dans le `CentralPanel` (MDI).
    Floating,
    /// Fenêtre OS séparée (`StokesForgeApp::show_detached_tabs`), sans
    /// décoration native — cette barre est tout ce que l'utilisateur a pour
    /// la déplacer, la redimensionner par ses icônes de maximisation, ou la
    /// fermer/réattacher.
    Detached,
    /// Dialogue modal ([`modal_dialog`]) : ni repli, ni détachement, ni
    /// maximisation — le titre, la croix, et le glisser.
    Dialog,
}

/// Une icône d'en-tête peinte au pixel près, comme le titre et le chevron de
/// repli — jamais un `Button` (voir la doc de tête de [`window_header`] pour
/// la raison précise). `rect` est déjà borné à la hauteur de la barre ; le
/// fond arrondi au survol imite l'affordance d'un bouton sans réintroduire
/// son padding natif.
pub(super) fn header_icon(
    header: &mut egui::Ui,
    base_id: egui::Id,
    id_source: &str,
    rect: egui::Rect,
    icon: Icon,
    tooltip: &str,
) -> bool {
    let response = header.interact(rect, base_id.with(id_source), egui::Sense::click());
    if response.hovered() {
        header
            .painter()
            .rect_filled(rect, 4.0, header.visuals().widgets.hovered.bg_fill);
    }
    icon_image(icon, IconRole::Neutral).paint_at(
        header,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(ICON_SIZE)),
    );
    response.on_hover_text(tooltip).clicked()
}

/// Taille minimale d'une fenêtre de contenu, flottante ou détachée — assez
/// pour garder l'en-tête (icônes + titre) et un fragment de contenu lisibles,
/// pas une contrainte scientifique.
pub(super) const MIN_CONTENT_WINDOW_SIZE: egui::Vec2 = egui::vec2(220.0, 140.0);

/// Calque des fenêtres flottantes : celui qu'egui réserve aux fenêtres
/// qu'on réordonne au clic (`Order::Middle`), au-dessus des panneaux fixes
/// (`Order::Background`) et **sous** les menus, listes déroulantes et
/// infobulles (`Order::Foreground`) — un menu contextuel ouvert dans une
/// fenêtre doit passer devant ses voisines.
pub(super) const FLOATING_WINDOW_ORDER: egui::Order = egui::Order::Middle;

/// Épaisseur des poignées de redimensionnement, posées **hors** du cadre
/// (voir [`floating_resize_handles`]) et comptées dans la zone cliquable de
/// la fenêtre par [`show_floating_window`].
pub(super) const RESIZE_BORDER: f32 = 6.0;

/// Longueur, le long de chaque côté, des deux branches d'une poignée de coin
/// (voir [`floating_resize_handles`]) : le coin se vise sur ~18 points
/// au lieu de 6.
const CORNER_REACH: f32 = 12.0;

/// Peint une fenêtre flottante de l'espace de travail, cadre compris, dans
/// son propre calque. `window_rect` est le rectangle écran de la fenêtre,
/// `viewport_rect` la portion visible de l'espace de travail (ce que les
/// panneaux fixes laissent). Rend `None` quand la fenêtre est entièrement
/// hors de la vue : rien n'est alors peint ni rendu cliquable.
///
/// # Pourquoi une `egui::Area` et pas une `egui::Window`
///
/// `egui::Window` est une boîte de dialogue : barre de titre, glisser,
/// redimensionnement, repli et bornage à elle. Toutes ces fonctions avaient
/// été coupées une à une puis refaites à la main ([`window_header`],
/// [`floating_resize_handles`], [`workspace_to_screen`]…), et chaque image
/// devait accorder deux rectangles : celui qu'on demandait et celui
/// qu'egui recalait. Les bugs 1 et 2 de
/// `retours-2026-09-24-bugs.md` (contour décalé d'une
/// fenêtre recalée, fenêtre peinte par-dessus les panneaux fixes) étaient
/// deux formes de ce désaccord. `egui::Area` est la brique nue sous
/// `Window` : un calque, une position, un ordre au clic — rien d'autre. Le
/// cadre, le contour et le rognage sont posés ici, sur le **même**
/// rectangle.
///
/// # La zone cliquable est la partie visible
///
/// egui choisit la fenêtre sous le pointeur d'après le rectangle de son
/// `Area` (`Memory::layer_id_at`), pas d'après ce qui est peint. L'`Area`
/// est donc taillée à la seule partie visible de la fenêtre (poignées
/// comprises) : une fenêtre à cheval sur le bord de la vue ne capte aucun
/// clic destiné au Project Manager ou à la console, et son dessin est rogné
/// au même bord (`constrain_to(viewport_rect)`, qui donne aussi à `Area` son
/// rectangle de découpe).
pub(super) fn show_floating_window<R>(
    ctx: &egui::Context,
    id: egui::Id,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    show_window_in_layer(
        ctx,
        id,
        FLOATING_WINDOW_ORDER,
        window_rect,
        viewport_rect,
        is_active,
        add_contents,
        add_resize_handles,
    )
}

/// [`show_floating_window`] dans l'ordre de calque `order` : la seule
/// écriture du cadre, du contour et du rognage d'une fenêtre, que se
/// partagent les fenêtres de l'espace de travail (`Order::Middle`) et les
/// dialogues modaux ([`modal_dialog`], au-dessus).
#[allow(clippy::too_many_arguments)]
pub(super) fn show_window_in_layer<R>(
    ctx: &egui::Context,
    id: egui::Id,
    order: egui::Order,
    window_rect: egui::Rect,
    viewport_rect: egui::Rect,
    is_active: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
    add_resize_handles: impl FnOnce(&egui::Ui, egui::Rect),
) -> Option<R> {
    let area_rect = window_rect.expand(RESIZE_BORDER).intersect(viewport_rect);
    if !window_rect.intersects(viewport_rect) || !area_rect.is_positive() {
        return None;
    }
    let response = egui::Area::new(id)
        .order(order)
        .movable(false)
        .fade_in(false)
        .fixed_pos(area_rect.min)
        .constrain_to(viewport_rect)
        .show(ctx, |ui| {
            // La taille de l'`Area` est celle de son contenu : on la fixe à
            // la partie visible, le contenu réel vivant dans un sous-`Ui` qui
            // ne la fait pas grandir.
            ui.set_min_size(area_rect.size());
            let style = ui.style().clone();
            let frame = floating_window_frame(&style);
            let content_rect = window_rect - frame.total_margin();
            ui.painter().add(frame.paint(content_rect));
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            content.set_clip_rect(content_rect.intersect(ui.clip_rect()));
            let inner = add_contents(&mut content);
            if is_active {
                paint_active_window_outline(
                    ui.painter(),
                    window_rect,
                    style.visuals.window_corner_radius,
                    &style,
                );
            }
            add_resize_handles(ui, window_rect);
            inner
        });
    Some(response.inner)
}

/// Huit zones fines de redimensionnement pour une fenêtre flottante MDI,
/// posées entièrement à l'**extérieur** de `rect` (sur [`RESIZE_BORDER`])
/// pour ne pas disputer le survol au contenu — les barres de défilement de
/// la grille longent le bord. Elles sont peintes dans le calque de la
/// fenêtre ([`show_floating_window`] compte ce liseré dans sa zone
/// cliquable) : la poignée d'une fenêtre ne passe donc jamais devant une
/// autre fenêtre posée par-dessus.
pub(super) fn floating_resize_handles(
    ui: &egui::Ui,
    tab: impl std::hash::Hash + std::fmt::Debug,
    rect: egui::Rect,
    position: &mut egui::Pos2,
    size: &mut egui::Vec2,
) {
    const BORDER: f32 = RESIZE_BORDER;
    // (source, zone, curseur, ouest, est, nord, sud) — les quatre derniers
    // indiquent quels côtés ce glissé affecte, combinés pour les coins.
    let mut zones: Vec<(String, egui::Rect, egui::CursorIcon, bool, bool, bool, bool)> = vec![
        (
            "n".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() - BORDER),
                egui::pos2(rect.right(), rect.top()),
            ),
            egui::CursorIcon::ResizeNorth,
            false,
            false,
            true,
            false,
        ),
        (
            "s".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom() + BORDER),
            ),
            egui::CursorIcon::ResizeSouth,
            false,
            false,
            false,
            true,
        ),
        (
            "w".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.left() - BORDER, rect.top()),
                egui::pos2(rect.left(), rect.bottom()),
            ),
            egui::CursorIcon::ResizeWest,
            true,
            false,
            false,
            false,
        ),
        (
            "e".into(),
            egui::Rect::from_min_max(
                egui::pos2(rect.right(), rect.top()),
                egui::pos2(rect.right() + BORDER, rect.bottom()),
            ),
            egui::CursorIcon::ResizeEast,
            false,
            true,
            false,
            false,
        ),
    ];
    // Coins : un carré de 6 × 6 hors du cadre est trop petit à viser (#75).
    // Chaque coin est donc un « L » posé le long des deux côtés voisins,
    // `CORNER_REACH` de long, toujours hors du cadre (les barres de
    // défilement du contenu ne sont pas disputées). Ajoutés après les côtés,
    // ils l'emportent là où ils se recouvrent.
    for (name, x, y, cursor, west, north) in [
        (
            "nw",
            rect.left(),
            rect.top(),
            egui::CursorIcon::ResizeNwSe,
            true,
            true,
        ),
        (
            "ne",
            rect.right(),
            rect.top(),
            egui::CursorIcon::ResizeNeSw,
            false,
            true,
        ),
        (
            "sw",
            rect.left(),
            rect.bottom(),
            egui::CursorIcon::ResizeNeSw,
            true,
            false,
        ),
        (
            "se",
            rect.right(),
            rect.bottom(),
            egui::CursorIcon::ResizeNwSe,
            false,
            false,
        ),
    ] {
        // Sens du coin : -1 vers l'extérieur, +1 vers l'intérieur, par axe.
        let sx = if west { 1.0 } else { -1.0 };
        let sy = if north { 1.0 } else { -1.0 };
        let horizontal = egui::Rect::from_two_pos(
            egui::pos2(x - sx * BORDER, y - sy * BORDER),
            egui::pos2(x + sx * CORNER_REACH, y),
        );
        let vertical = egui::Rect::from_two_pos(
            egui::pos2(x - sx * BORDER, y - sy * BORDER),
            egui::pos2(x, y + sy * CORNER_REACH),
        );
        for (suffix, zone) in [("h", horizontal), ("v", vertical)] {
            zones.push((
                format!("{name}-{suffix}"),
                zone,
                cursor,
                west,
                !west,
                north,
                !north,
            ));
        }
    }
    let base_id = egui::Id::new(("floating_resize_handle", tab));
    for (source, zone, cursor, west, east, north, south) in zones {
        let response = ui.interact(zone, base_id.with(source), egui::Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        let delta = response.drag_delta();
        if delta == egui::Vec2::ZERO {
            continue;
        }
        if west {
            let new_width = (size.x - delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
            position.x += size.x - new_width;
            size.x = new_width;
        }
        if east {
            size.x = (size.x + delta.x).max(MIN_CONTENT_WINDOW_SIZE.x);
        }
        if north {
            let new_height = (size.y - delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
            position.y += size.y - new_height;
            size.y = new_height;
        }
        if south {
            size.y = (size.y + delta.y).max(MIN_CONTENT_WINDOW_SIZE.y);
        }
    }
}

/// Bordure invisible de redimensionnement pour une fenêtre détachée sans
/// décoration (`.with_decorations(false)`, voir la doc de tête de
/// [`StokesForgeApp::show_detached_tabs`]) : huit zones fines aux bords/coins du
/// viewport, qui délèguent le redimensionnement à l'OS dès que le glissé
/// démarre (`ViewportCommand::BeginResize`) — même principe que
/// `ViewportCommand::StartDrag` pour le déplacement du titre : aucun calcul
/// de delta à notre charge, l'OS/le gestionnaire de fenêtres prend la main.
pub(super) fn detached_resize_handles(ui: &egui::Ui) {
    const BORDER: f32 = 6.0;
    let rect = ui.ctx().input(|input| input.content_rect());
    let zones: [(&str, egui::Rect, egui::CursorIcon, egui::ResizeDirection); 8] = [
        (
            "n",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.right(), rect.top() + BORDER),
            ),
            egui::CursorIcon::ResizeNorth,
            egui::ResizeDirection::North,
        ),
        (
            "s",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                rect.right_bottom(),
            ),
            egui::CursorIcon::ResizeSouth,
            egui::ResizeDirection::South,
        ),
        (
            "w",
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + BORDER, rect.bottom()),
            ),
            egui::CursorIcon::ResizeWest,
            egui::ResizeDirection::West,
        ),
        (
            "e",
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - BORDER, rect.top()),
                rect.right_bottom(),
            ),
            egui::CursorIcon::ResizeEast,
            egui::ResizeDirection::East,
        ),
        (
            "nw",
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(BORDER, BORDER)),
            egui::CursorIcon::ResizeNwSe,
            egui::ResizeDirection::NorthWest,
        ),
        (
            "ne",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.top()),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            egui::ResizeDirection::NorthEast,
        ),
        (
            "sw",
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNeSw,
            egui::ResizeDirection::SouthWest,
        ),
        (
            "se",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - BORDER, rect.bottom() - BORDER),
                egui::vec2(BORDER, BORDER),
            ),
            egui::CursorIcon::ResizeNwSe,
            egui::ResizeDirection::SouthEast,
        ),
    ];
    let base_id = egui::Id::new("detached_resize_handle");
    for (source, zone, cursor, direction) in zones {
        let response = ui.interact(zone, base_id.with(source), egui::Sense::drag());
        if response.hovered() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// Largeur d'une icône d'action de la barre de titre (croix, détacher,
/// maximiser).
pub(super) const HEADER_ACTION_WIDTH: f32 = 24.0;

/// Détacher et maximiser sont-ils peints ? Une seule source pour le dessin
/// et pour la largeur réservée au titre (#50).
pub(super) fn shows_detach_and_maximize(placement: HeaderPlacement, tab: Option<Tab>) -> bool {
    match placement {
        HeaderPlacement::Floating => matches!(tab, Some(Tab::Book(_) | Tab::Graph(_))),
        HeaderPlacement::Detached => true,
        HeaderPlacement::Dialog => false,
    }
}

/// Largeur que les icônes réellement peintes retranchent au titre : la
/// croix seule, ou la croix plus détacher et maximiser (pas de 26 px, marge
/// de 2 px comprise).
pub(super) fn header_actions_width(with_detach_and_maximize: bool) -> f32 {
    let icons = if with_detach_and_maximize { 3.0 } else { 1.0 };
    2.0 + icons * (HEADER_ACTION_WIDTH + 2.0)
}

/// Barre de titre des fenêtres de contenu — la même base de code pour les
/// fenêtres flottantes MDI et les fenêtres OS détachées (`placement`, voir
/// [`HeaderPlacement`]). Les actions et la croix sont peintes de la même
/// façon que le titre (voir [`header_icon`]) et forment un seul groupe
/// visuel ; cela évite aussi une `Area` superposée, qui se décale quand une
/// fenêtre passe derrière une autre.
pub(super) fn window_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    title: &str,
    tab: Option<Tab>,
    collapsed: bool,
    placement: HeaderPlacement,
) -> WindowHeaderActions {
    const COLLAPSE_WIDTH: f32 = 24.0;

    let mut actions = WindowHeaderActions::default();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), WINDOW_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let mut header = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    // B5 (`retours-2026-09-23-bugs.md`) : l'icône de repli
    // était réservée à `Tab::Graph`, sans justification documentée — une
    // extension jamais faite plutôt qu'un choix. `Matrix`/`Note` restent
    // exclus faute de raison de les y ajouter.
    let collapse_width = if matches!(tab, Some(Tab::Graph(_) | Tab::Book(_))) {
        COLLAPSE_WIDTH
    } else {
        0.0
    };
    if collapse_width > 0.0 {
        let collapse_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(collapse_width, rect.height()));
        let content_label = match tab {
            Some(Tab::Book(_)) => "classeur",
            _ => "graphe",
        };
        let (icon, tooltip) = if collapsed {
            (
                Icon::CaretRight,
                format!("Afficher le contenu du {content_label}"),
            )
        } else {
            (
                Icon::CaretDown,
                format!("Masquer le contenu du {content_label}"),
            )
        };
        if header_icon(&mut header, id, "collapse", collapse_rect, icon, &tooltip) {
            actions.toggle_collapse = true;
        }
    }
    let action_rect = |offset: f32| {
        egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - offset - HEADER_ACTION_WIDTH,
                rect.top() + 2.0,
            ),
            egui::vec2(HEADER_ACTION_WIDTH, rect.height() - 4.0),
        )
    };
    if header_icon(
        &mut header,
        id,
        "close",
        action_rect(2.0),
        Icon::Close,
        "Fermer la fenêtre",
    ) {
        actions.close = true;
    }
    // En MDI, détacher/maximiser restent réservés à Book/Graph (Matrix/Note
    // n'ont pas ce chemin d'UI aujourd'hui, même si `show_detached_tabs` les
    // gère déjà génériquement). En fenêtre détachée, les quatre natures
    // gardent réattacher/maximiser en toutes circonstances — c'est ce qui
    // remplace la barre de titre native de l'OS retirée avec elle
    // (`.with_decorations(false)`), donc pas question d'en perdre une
    // capacité pour certains onglets.
    let show_detach_and_maximize = shows_detach_and_maximize(placement, tab);
    if show_detach_and_maximize {
        let (detach_icon, detach_tooltip) = match placement {
            HeaderPlacement::Floating | HeaderPlacement::Dialog => {
                (Icon::Detach, "Détacher dans une fenêtre")
            }
            HeaderPlacement::Detached => (Icon::Reattach, "Réattacher à la fenêtre principale"),
        };
        if header_icon(
            &mut header,
            id,
            "detach",
            action_rect(28.0),
            detach_icon,
            detach_tooltip,
        ) {
            actions.detach = true;
        }
        if header_icon(
            &mut header,
            id,
            "maximize",
            action_rect(54.0),
            Icon::Maximize,
            "Maximiser la fenêtre",
        ) {
            actions.maximize = true;
        }
    }
    // La zone de glisser va de la flèche de repli aux icônes réellement
    // peintes (#50) ; le titre reste centré dans l'en-tête et n'est rogné de
    // chaque côté que par ce qui s'y trouve vraiment : la flèche à gauche, les
    // icônes à droite (rien ne rogne un côté vide).
    let actions_width = header_actions_width(show_detach_and_maximize);
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + collapse_width, rect.top()),
        egui::pos2(rect.right() - actions_width, rect.bottom()),
    );
    let clip_rect = title_rect;
    header.painter().with_clip_rect(clip_rect).text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::TextStyle::Heading.resolve(header.style()),
        header.visuals().text_color(),
    );
    let drag_rect = title_rect;
    let drag = ui.interact(
        drag_rect,
        id.with("content_title_drag"),
        egui::Sense::drag(),
    );
    if drag.drag_started() {
        actions.title_drag_started = true;
    }
    if drag.dragged() {
        actions.drag_delta = ui.input(|input| input.pointer.delta());
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    actions
}

/// Pose le contenu d'une fenêtre dans l'espace qui reste, rogné à cet espace :
/// un contenu trop grand ne déborde ni ne repousse le cadre.
pub(super) fn show_window_contents_bounded(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let rect = ui.available_rect_before_wrap();
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    content.set_clip_rect(ui.clip_rect().intersect(rect));
    add_contents(&mut content);
    ui.advance_cursor_after_rect(rect);
}

/// `pub(super)` : [`tests::find_icon_centers`] sert aussi aux tests d'`app.rs`
/// qui peignent l'en-tête d'une fenêtre détachée.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use stokesforge_core::NodeId;

    /// Centres des icônes peintes (images : rectangles à texture), de gauche à
    /// droite. Les chargeurs d'images doivent être installés sur le contexte,
    /// et l'image avoir déjà tourné une fois pour que la texture soit prête.
    pub(in crate::app) fn find_icon_centers(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Vec<egui::Pos2> {
        let mut centers: Vec<egui::Pos2> = shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect.center()),
                _ => None,
            })
            .collect();
        centers.sort_by(|a, b| a.x.total_cmp(&b.x));
        centers
    }

    /// Largeur de l'écran des tests d'en-tête : l'en-tête la prend toute.
    const LARGEUR_EN_TETE: f32 = 400.0;

    /// Peint [`window_header`] seul, sur toute la largeur de l'écran, sur
    /// `events.len() + 1` images (la première sans événement : la texture SVG
    /// des icônes n'est prête qu'à la seconde). Rend la sortie de la dernière
    /// image et les actions de chaque image.
    fn peindre_en_tete(
        context: &egui::Context,
        title: &str,
        tab: Option<Tab>,
        placement: HeaderPlacement,
        events: Vec<Vec<egui::Event>>,
    ) -> (egui::FullOutput, Vec<WindowHeaderActions>) {
        egui_extras::install_image_loaders(context);
        let mut output = egui::FullOutput::default();
        let mut actions = Vec::new();
        for events in std::iter::once(Vec::new()).chain(events) {
            output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(LARGEUR_EN_TETE, 120.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    actions.push(window_header(
                        ui,
                        egui::Id::new("en_tete_de_test"),
                        title,
                        tab,
                        false,
                        placement,
                    ));
                },
            );
            output.textures_delta.clear();
        }
        (output, actions)
    }

    /// Les icônes d'un en-tête au repos, de gauche à droite.
    fn icones_de_l_en_tete(tab: Option<Tab>, placement: HeaderPlacement) -> Vec<egui::Pos2> {
        let context = egui::Context::default();
        let (output, _) = peindre_en_tete(&context, "Titre", tab, placement, vec![Vec::new()]);
        find_icon_centers(&output.shapes)
    }

    // B7 (`retours-2026-09-23-bugs.md`) : jusque-là, la
    // surbrillance recolorait le trait du `Frame` de la fenêtre
    // (`floating_window_frame`) plutôt que de peindre un second contour par
    // dessus — mais ce trait suit `content_ui.min_rect()`, pas la taille
    // demandée, et débordait donc quand le contenu dépassait (bande
    // d'outils qui ne s'enroule pas). Le `Frame` ne code plus l'activité :
    // il garde toujours le trait par défaut d'egui, quel que soit l'état.
    #[test]
    fn le_frame_de_la_fenetre_flottante_garde_toujours_le_trait_par_defaut() {
        let style = egui::Style::default();

        let frame = floating_window_frame(&style);

        assert_eq!(
            frame.stroke.color,
            style.visuals.window_stroke().color,
            "le Frame ne recolore plus selon l'activité (B7) : le contour actif est peint à \
             part par `paint_active_window_outline`, sur le rectangle demandé plutôt que celui \
             qu'egui recalcule depuis le contenu"
        );
        assert_eq!(
            frame.inner_margin, WINDOW_CONTENT_MARGIN,
            "la marge du cadre est celle des fenêtres de contenu, sans bandeau vertical mort"
        );
    }

    // Le trait recoloré vit maintenant dans `active_window_stroke`, utilisé
    // par [`paint_active_window_outline`] plutôt que par le `Frame` — même
    // logique de distinction active/inactive qu'avant B7, déplacée d'une
    // fonction à l'autre.
    #[test]
    fn active_window_stroke_distingue_toujours_actif_et_inactif() {
        let style = egui::Style::default();

        let inactif = active_window_stroke(&style, false);
        let actif = active_window_stroke(&style, true);

        assert_eq!(
            inactif.color,
            style.visuals.window_stroke().color,
            "une fenêtre inactive garde le trait par défaut d'egui"
        );
        assert_eq!(
            actif.color, style.visuals.selection.stroke.color,
            "une fenêtre active reprend la couleur de sélection du thème"
        );
        assert_ne!(
            actif.color, inactif.color,
            "sans couleurs distinctes, rien ne montrerait quelle fenêtre est active"
        );
    }

    /// Peint des fenêtres flottantes par le chemin de production
    /// ([`show_floating_window`]), sur une image, et rend la sortie d'egui.
    /// Deux images au moins sont nécessaires : la première est la passe de
    /// mesure d'une `Area` neuve, qui ne peint rien.
    fn peindre_fenetres(
        context: &egui::Context,
        fenetres: &[(egui::Id, egui::Rect)],
        viewport: egui::Rect,
        actives: bool,
        contenu: impl Fn(&mut egui::Ui),
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 1000.0),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            for &(id, rect) in fenetres {
                show_floating_window(ui.ctx(), id, rect, viewport, actives, &contenu, |_, _| {});
            }
        });
        output.textures_delta.clear();
        output
    }

    /// Contenu plus large que n'importe quelle fenêtre de test — comme un
    /// classeur à plusieurs colonnes.
    fn contenu_debordant(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for index in 0..10 {
                ui.label(format!("colonne {index}"));
                ui.add_space(80.0);
            }
        });
    }

    /// Tous les rectangles peints, `Shape::Vec` dépliés (le cadre d'egui
    /// arrive en `Vec[ombre, rectangle]`).
    fn rectangles_peints(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
        fn deplier(shape: &egui::Shape, out: &mut Vec<egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Rect(rect) => out.push(rect.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| deplier(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for painted in shapes {
            deplier(&painted.shape, &mut out);
        }
        out
    }

    // Bug 1 (`retours-2026-09-24-bugs.md`), la conséquence, et B1/B3/B4/B7 du
    // rapport du 2026-09-23 : le cadre, le contour actif, la zone cliquable et
    // le rognage du contenu suivent tous le même rectangle, que le contenu
    // soit court ou déborde.
    #[test]
    fn le_cadre_le_contour_et_la_zone_cliquable_suivent_le_meme_rectangle() {
        let style = egui::Style::default();
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 80.0), egui::vec2(220.0, 180.0));
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
        for deborde in [false, true] {
            let context = egui::Context::default();
            let id = egui::Id::new(("meme_rectangle", deborde));
            let contenu = |ui: &mut egui::Ui| {
                if deborde {
                    contenu_debordant(ui);
                } else {
                    ui.label("contenu court");
                }
            };
            peindre_fenetres(&context, &[(id, window_rect)], viewport, true, contenu);
            let output = peindre_fenetres(&context, &[(id, window_rect)], viewport, true, contenu);

            let rectangles = rectangles_peints(&output.shapes);
            let cadre = rectangles
                .iter()
                .find(|rect| rect.fill == style.visuals.window_fill())
                .expect("le cadre doit être peint");
            let contour = rectangles
                .iter()
                .find(|rect| rect.stroke.color == style.visuals.selection.stroke.color)
                .expect("le contour actif doit être peint");
            assert_eq!(cadre.rect, window_rect, "deborde={deborde}");
            assert_eq!(contour.rect, window_rect, "deborde={deborde}");
            assert_eq!(
                context.memory(|memory| memory.area_rect(id)),
                Some(window_rect.expand(RESIZE_BORDER)),
                "la zone cliquable est la fenêtre plus ses poignées, deborde={deborde}"
            );
            let mut textes = 0;
            for painted in &output.shapes {
                if let egui::Shape::Text(text) = &painted.shape {
                    textes += 1;
                    assert!(
                        window_rect.contains_rect(painted.clip_rect),
                        "{:?} déborde de la fenêtre : découpe {:?}",
                        text.galley.job.text,
                        painted.clip_rect,
                    );
                }
            }
            // Sans texte peint, la boucle ci-dessus ne vérifierait rien.
            assert!(textes > 0, "le contenu doit être peint, deborde={deborde}");
        }

        // Le témoin : la même fenêtre inactive est encadrée, mais aucun
        // rectangle ne porte la couleur de sélection — c'est bien le contour
        // actif que la recherche ci-dessus a trouvé, pas un autre trait.
        let context = egui::Context::default();
        let id = egui::Id::new("meme_rectangle_inactive");
        let contenu = |ui: &mut egui::Ui| {
            ui.label("contenu court");
        };
        peindre_fenetres(&context, &[(id, window_rect)], viewport, false, contenu);
        let output = peindre_fenetres(&context, &[(id, window_rect)], viewport, false, contenu);
        let rectangles = rectangles_peints(&output.shapes);
        assert!(rectangles
            .iter()
            .any(|rect| rect.fill == style.visuals.window_fill() && rect.rect == window_rect));
        assert!(
            !rectangles
                .iter()
                .any(|rect| rect.stroke.color == style.visuals.selection.stroke.color),
            "une fenêtre inactive n'a pas de contour de sélection"
        );
    }

    // Bug 2 (`retours-2026-09-24-bugs.md`) : une fenêtre à
    // cheval sur le bord de la vue (espace de travail défilé) peignait
    // par-dessus le Project Manager, et y captait les clics.
    #[test]
    fn une_fenetre_a_cheval_sur_le_bord_de_la_vue_est_rognee_et_laisse_passer_les_clics() {
        let context = egui::Context::default();
        let id = egui::Id::new("a_cheval");
        // La vue commence à x = 400 (à gauche : le Project Manager).
        let viewport = egui::Rect::from_min_max(egui::pos2(400.0, 0.0), egui::pos2(1400.0, 1000.0));
        let window_rect =
            egui::Rect::from_min_size(egui::pos2(250.0, 100.0), egui::vec2(500.0, 400.0));
        let fenetres = [(id, window_rect)];
        peindre_fenetres(&context, &fenetres, viewport, true, contenu_debordant);
        let output = peindre_fenetres(&context, &fenetres, viewport, true, contenu_debordant);

        assert!(!output.shapes.is_empty());
        for painted in &output.shapes {
            assert!(
                viewport.contains_rect(painted.clip_rect),
                "une forme de la fenêtre est découpée hors de la vue : {:?}",
                painted.clip_rect,
            );
        }
        let calque = egui::LayerId::new(FLOATING_WINDOW_ORDER, id);
        assert_ne!(
            context.layer_id_at(egui::pos2(300.0, 300.0)),
            Some(calque),
            "sous le Project Manager, la partie cachée de la fenêtre ne doit capter aucun clic"
        );
        assert_eq!(context.layer_id_at(egui::pos2(500.0, 300.0)), Some(calque));
    }

    /// Une fenêtre entièrement hors de la portion visible de l'espace de
    /// travail (défilé ailleurs) n'est ni peinte ni cliquable, et son contenu
    /// n'est même pas construit ; sa voisine visible, si (témoin).
    #[test]
    fn une_fenetre_hors_de_la_vue_n_est_ni_peinte_ni_cliquable() {
        let context = egui::Context::default();
        let viewport = egui::Rect::from_min_max(egui::pos2(400.0, 0.0), egui::pos2(1400.0, 1000.0));
        let cachee = egui::Id::new("hors_vue");
        let visible = egui::Id::new("dans_la_vue");
        // À gauche de la vue, poignées comprises : rien n'en dépasse.
        let cachee_rect =
            egui::Rect::from_min_size(egui::pos2(50.0, 100.0), egui::vec2(300.0, 200.0));
        let visible_rect =
            egui::Rect::from_min_size(egui::pos2(600.0, 100.0), egui::vec2(300.0, 200.0));
        let mut rendus = Vec::new();
        // Deux images : la première est la passe de mesure d'une `Area` neuve.
        for _ in 0..2 {
            rendus.clear();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    for (id, rect) in [(cachee, cachee_rect), (visible, visible_rect)] {
                        rendus.push(show_floating_window(
                            ui.ctx(),
                            id,
                            rect,
                            viewport,
                            true,
                            |_| id,
                            |_, _| {},
                        ));
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(rendus, vec![None, Some(visible)]);
        assert_eq!(
            context.memory(|memory| memory.area_rect(cachee)),
            None,
            "aucune zone cliquable n'est posée pour la fenêtre hors de la vue"
        );
        assert_ne!(
            context.layer_id_at(cachee_rect.center()),
            Some(egui::LayerId::new(FLOATING_WINDOW_ORDER, cachee))
        );
        assert_eq!(
            context.layer_id_at(visible_rect.center()),
            Some(egui::LayerId::new(FLOATING_WINDOW_ORDER, visible))
        );
    }

    #[test]
    fn len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche() {
        let context = egui::Context::default();
        let (output, _) = peindre_en_tete(
            &context,
            "Graph1",
            Some(Tab::Graph(NodeId::from_raw(1))),
            HeaderPlacement::Floating,
            Vec::new(),
        );
        let icons = find_icon_centers(&output.shapes);

        let title = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Graph1" => Some(shape),
                _ => None,
            })
            .expect("le titre doit être peint");
        let title_center = title.pos.x + title.galley.rect.center().x;
        assert!(
            (title_center - LARGEUR_EN_TETE / 2.0).abs() < f32::EPSILON,
            "le titre doit être centré dans l'en-tête, pas après la flèche"
        );
        let collapse = icons.first().expect("la flèche de repli doit être peinte");
        assert!(
            collapse.x < 24.0,
            "la flèche de repli doit rester dans le coin supérieur gauche"
        );

        // Régression : les icônes d'action étaient posées via `Button::
        // small()`, un widget avec son propre cadre/padding qui ne
        // s'aligne pas pixel pour pixel sur le titre peint brut — d'où un
        // désalignement visible malgré des rects nominalement centrés sur
        // la même ligne (voir la doc de tête de `window_header`).
        // `header_icon` peint désormais tout de la même façon : les centres
        // verticaux doivent coïncider exactement.
        let title_center_y = title.pos.y + title.galley.rect.center().y;
        let close_center_y = icons.last().expect("le bouton fermer doit être peint").y;
        assert!(
            (title_center_y - close_center_y).abs() < f32::EPSILON,
            "le titre et les icônes d'action doivent partager le même centre vertical"
        );
    }

    // B5 (`retours-2026-09-23-bugs.md`) : l'icône « Masquer le
    // contenu de… » était réservée à `Tab::Graph` sans raison documentée —
    // vérifie qu'elle est bien peinte pour `Tab::Book`, et toujours absente
    // pour `Tab::Matrix`/`Tab::Note`, qui n'ont pas ce contrôle.
    #[test]
    fn licone_de_repli_est_peinte_pour_les_classeurs_mais_pas_les_matrices() {
        // Une icône dans le coin gauche (la croix, elle, est à droite).
        let fleche_de_repli = |tab: Tab| {
            icones_de_l_en_tete(Some(tab), HeaderPlacement::Floating)
                .first()
                .is_some_and(|center| center.x < 24.0)
        };

        let node = NodeId::from_raw(1);
        assert!(
            fleche_de_repli(Tab::Book(node)),
            "un classeur doit porter l'icône de repli"
        );
        assert!(
            fleche_de_repli(Tab::Graph(node)),
            "un graphe doit garder l'icône de repli"
        );
        assert!(
            !fleche_de_repli(Tab::Matrix(node)),
            "une matrice n'a pas ce contrôle"
        );
        assert!(
            !fleche_de_repli(Tab::Note(node)),
            "une note n'a pas ce contrôle"
        );
    }

    /// #50 : un dialogue ne peint que la croix, il ne doit donc réserver que
    /// sa place ; la largeur retranchée au titre suit les icônes peintes.
    #[test]
    fn la_largeur_reservee_suit_les_icones_peintes() {
        let id = NodeId::from_raw(1);
        let croix = header_actions_width(shows_detach_and_maximize(HeaderPlacement::Dialog, None));
        let trois = header_actions_width(shows_detach_and_maximize(
            HeaderPlacement::Floating,
            Some(Tab::Graph(id)),
        ));
        let matrice = header_actions_width(shows_detach_and_maximize(
            HeaderPlacement::Floating,
            Some(Tab::Matrix(id)),
        ));
        assert_eq!(croix, 28.0);
        assert_eq!(trois, 80.0);
        assert_eq!(matrice, croix);
        assert!(shows_detach_and_maximize(HeaderPlacement::Detached, None));
    }

    /// #50, côté dessin : le nombre d'icônes réellement peintes à droite de
    /// l'en-tête est celui que [`shows_detach_and_maximize`] annonce, et
    /// toutes tiennent dans la largeur que [`header_actions_width`] retranche
    /// au titre — sans quoi le titre passerait sous une icône (ou serait
    /// rogné pour une icône absente).
    #[test]
    fn les_icones_peintes_a_droite_sont_celles_que_la_largeur_reserve() {
        let id = NodeId::from_raw(1);
        for (tab, placement) in [
            (Some(Tab::Graph(id)), HeaderPlacement::Floating),
            (Some(Tab::Book(id)), HeaderPlacement::Floating),
            (Some(Tab::Matrix(id)), HeaderPlacement::Floating),
            (Some(Tab::Note(id)), HeaderPlacement::Floating),
            (Some(Tab::Matrix(id)), HeaderPlacement::Detached),
            (Some(Tab::Graph(id)), HeaderPlacement::Detached),
            (None, HeaderPlacement::Dialog),
        ] {
            let a_droite: Vec<_> = icones_de_l_en_tete(tab, placement)
                .into_iter()
                .filter(|center| center.x > LARGEUR_EN_TETE / 2.0)
                .collect();
            let trois = shows_detach_and_maximize(placement, tab);
            assert_eq!(a_droite.len(), if trois { 3 } else { 1 }, "{tab:?}");
            let reserve = header_actions_width(trois);
            for center in a_droite {
                assert!(
                    center.x - ICON_SIZE / 2.0 >= LARGEUR_EN_TETE - reserve,
                    "{tab:?} : une icône ({center:?}) déborde sur la place du titre ({reserve})"
                );
            }
        }
        // Un dialogue n'a pas non plus de flèche de repli à gauche.
        assert!(icones_de_l_en_tete(None, HeaderPlacement::Dialog)
            .iter()
            .all(|center| center.x > LARGEUR_EN_TETE / 2.0));
    }

    /// Les gestes de la barre de titre remontent à l'appelant : glisser le
    /// titre donne son déplacement (et le signal de départ, une seule fois),
    /// cliquer la croix demande la fermeture. Le témoin : glisser le titre ne
    /// ferme rien.
    #[test]
    fn glisser_le_titre_deplace_et_cliquer_la_croix_ferme() {
        let tab = Some(Tab::Graph(NodeId::from_raw(1)));
        let bouton = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        let context = egui::Context::default();
        let depart = egui::pos2(LARGEUR_EN_TETE / 2.0, WINDOW_HEADER_HEIGHT / 2.0);
        let arrivee = depart + egui::vec2(30.0, 10.0);
        let (_, actions) = peindre_en_tete(
            &context,
            "Titre",
            tab,
            HeaderPlacement::Floating,
            vec![
                vec![egui::Event::PointerMoved(depart)],
                vec![bouton(depart, true)],
                vec![egui::Event::PointerMoved(arrivee)],
                vec![bouton(arrivee, false)],
            ],
        );
        let deplacement = actions
            .iter()
            .fold(egui::Vec2::ZERO, |somme, action| somme + action.drag_delta);
        assert_eq!(deplacement, arrivee - depart);
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.title_drag_started)
                .count(),
            1
        );
        assert!(actions.iter().all(|action| !action.close
            && !action.detach
            && !action.maximize
            && !action.toggle_collapse));

        let context = egui::Context::default();
        let croix = *icones_de_l_en_tete(tab, HeaderPlacement::Floating)
            .last()
            .expect("la croix est la dernière icône à droite");
        let (_, actions) = peindre_en_tete(
            &context,
            "Titre",
            tab,
            HeaderPlacement::Floating,
            vec![
                vec![egui::Event::PointerMoved(croix)],
                vec![bouton(croix, true)],
                vec![bouton(croix, false)],
            ],
        );
        assert!(actions.iter().any(|action| action.close));
        assert!(actions
            .iter()
            .all(|action| action.drag_delta == egui::Vec2::ZERO && !action.detach));
    }

    /// Glisse, sur une fenêtre de `(300, 300)`, `400 × 300`, la poignée de
    /// redimensionnement visée en `prise` de `deplacement`, puis relâche et
    /// peint une image de plus sans événement. Rend la position et la taille
    /// après le glissé, puis après l'image de repos.
    fn glisser_poignee(
        prise: impl FnOnce(egui::Rect) -> egui::Pos2,
        deplacement: egui::Vec2,
    ) -> ((egui::Pos2, egui::Vec2), (egui::Pos2, egui::Vec2)) {
        let tab = Tab::Graph(NodeId::from_raw(1));
        let context = egui::Context::default();
        let state = std::cell::Cell::new((egui::pos2(300.0, 300.0), egui::vec2(400.0, 300.0)));

        let paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let (mut position, mut size) = state.get();
            let rect = egui::Rect::from_min_size(position, size);
            let mut output = context.run_ui(input, |ui| {
                floating_resize_handles(ui, tab, rect, &mut position, &mut size);
            });
            output.textures_delta.clear();
            state.set((position, size));
        };

        paint(Vec::new());
        let (position, size) = state.get();
        let depart = prise(egui::Rect::from_min_size(position, size));
        paint(vec![egui::Event::PointerMoved(depart)]);
        paint(vec![egui::Event::PointerButton {
            pos: depart,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let arrivee = depart + deplacement;
        paint(vec![egui::Event::PointerMoved(arrivee)]);
        let apres_glisse = state.get();
        paint(vec![egui::Event::PointerButton {
            pos: arrivee,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(Vec::new());
        (apres_glisse, state.get())
    }

    /// Régression (§3, redimensionnement par les 4 côtés) : `egui::Window`
    /// coupe `resize_left`/`resize_top` en interne dès que
    /// `.drag_area(WindowDrag::Off)` est posé (nécessaire au glissé de la
    /// barre de titre maison) — les deux dépendent d'`Area::is_movable()`,
    /// qu'`Off` force à `false`, sans moyen de les garder actifs séparément.
    /// D'où [`floating_resize_handles`], qui remplace entièrement le
    /// redimensionnement natif. Ce test glisse sa poignée ouest et vérifie
    /// que la position *et* la taille suivent, puis qu'une image sans
    /// événement ne fait rien sauter en arrière.
    #[test]
    fn le_redimensionnement_par_le_bord_gauche_deplace_et_retrecit() {
        let ((position, size), repos) = glisser_poignee(
            |rect| egui::pos2(rect.left() - 3.0, rect.center().y),
            egui::vec2(-40.0, 0.0),
        );
        assert!(
            (position.x - 260.0).abs() < f32::EPSILON,
            "le bord ouest doit déplacer le coin de la fenêtre de la même distance que le glissé, position={position:?}"
        );
        assert!(
            (size.x - 440.0).abs() < f32::EPSILON,
            "le bord ouest doit élargir la fenêtre d'autant que le coin recule, size={size:?}"
        );
        assert_eq!(
            (position.y, size.y),
            (300.0, 300.0),
            "un bord vertical ne touche pas à la hauteur"
        );
        // Image suivante sans aucun événement : rien ne doit plus bouger —
        // contrairement au bug natif, il n'y a ici aucun mécanisme qui
        // pourrait faire sauter la fenêtre en arrière (`position`/`size`
        // sont notre propre état, pas une mémoire interne qu'un autre
        // appel pourrait écraser).
        assert_eq!(repos, (position, size));
    }

    /// Rétrécir par le bord ouest s'arrête à [`MIN_CONTENT_WINDOW_SIZE`], et
    /// le bord **est** reste alors en place : la fenêtre ne glisse pas vers la
    /// droite une fois sa taille minimale atteinte.
    #[test]
    fn retrecir_par_le_bord_gauche_s_arrete_a_la_taille_minimale() {
        let bord_est = 300.0 + 400.0;
        let ((position, size), _) = glisser_poignee(
            |rect| egui::pos2(rect.left() - 3.0, rect.center().y),
            egui::vec2(600.0, 0.0),
        );
        assert_eq!(size.x, MIN_CONTENT_WINDOW_SIZE.x);
        assert_eq!(position.x + size.x, bord_est);
    }

    /// #75 : un coin se vise le long de ses deux côtés voisins, sur
    /// `CORNER_REACH`, pas seulement dans un carré de 6 × 6. Saisi sur le
    /// bord haut **près** du coin nord-ouest, le glissé redimensionne sur les
    /// deux axes ; le témoin, saisi au milieu du même bord, ne touche qu'à la
    /// hauteur.
    #[test]
    fn un_coin_se_saisit_le_long_de_ses_deux_cotes() {
        let deplacement = egui::vec2(-20.0, -30.0);
        let ((position, size), _) = glisser_poignee(
            |rect| egui::pos2(rect.left() + CORNER_REACH - 2.0, rect.top() - 3.0),
            deplacement,
        );
        assert_eq!(position, egui::pos2(280.0, 270.0), "coin nord-ouest");
        assert_eq!(size, egui::vec2(420.0, 330.0), "coin nord-ouest");

        let ((position, size), _) = glisser_poignee(
            |rect| egui::pos2(rect.center().x, rect.top() - 3.0),
            deplacement,
        );
        assert_eq!(position, egui::pos2(300.0, 270.0), "bord nord seul");
        assert_eq!(size, egui::vec2(400.0, 330.0), "bord nord seul");
    }
}
