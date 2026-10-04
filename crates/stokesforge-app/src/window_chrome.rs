//! Cadre commun des fenêtres de contenu : barre de titre (`window_header`),
//! cadre et contour (`floating_window_frame`), poignées de redimensionnement,
//! et la pose d'une fenêtre dans une couche egui (`show_window_in_layer`).
//!
//! Déplacé tel quel d'`app.rs` (#100, étape 1 ; la suite — placement, pile,
//! détachement, masquage, maximisation — est #101), sans changement de
//! comportement : c'est la brique que partagent les fenêtres flottantes, les
//! fenêtres détachées et les dialogues modaux. Sous-module d'`app` pour garder
//! l'accès à ses types privés (`Tab`, constantes d'icônes…).

use super::*;

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
pub(super) const CORNER_REACH: f32 = 12.0;

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
