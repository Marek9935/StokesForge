//! La glu `eframe` : traduit clavier et souris en [`Action`] et peint
//! l'[`AppState`]. Aucune logique de modèle ne vit ici — tout ce qui décide
//! est dans [`crate::state`], qui se teste sans fenêtre. Ce module choisit
//! en revanche la **disposition** (docking `egui_dock`) et le **cadre**
//! (menus, raccourcis, sélecteur de fichiers `rfd`) qui l'entourent — des
//! décisions de présentation, pas de modèle, donc légitimement hors de
//! `state`.
//!
//! # Ce que Qt donnait gratuitement et qu'il faut écrire ici
//!
//! Une `QAction` reliée à un `QUndoStack` portait d'un coup le raccourci
//! clavier, l'entrée de menu, son grisage et son libellé (« Annuler
//! *Renommer* »). Ici les quatre sont séparés : le raccourci est déclaré et
//! consommé à la main ([`SHORTCUT_UNDO`] et compagnie), le grisage passe par
//! `add_enabled`, le libellé vient de [`AppState::undo_label`], et rien ne
//! garantit leur cohérence sinon la relecture.
//!
//! # Disposition dockable
//!
//! Les trois contenus qui, avant cette tâche, vivaient dans des panneaux
//! fixes — la grille, le(s) panneau(x) de graphe, la console de script —
//! sont désormais des onglets d'un [`egui_dock::DockState`] peint par une
//! [`egui_dock::DockArea`]. C'est la charpente de disposition décidée par
//! `docs/archive/rustrewrite/roadmap.md` §7 ; elle n'invente ni panneau
//! Properties ni grille à délégués (hors périmètre, voir `docs/application.md`).
//!
//! Conséquence directe : la limite « un seul graphe affiché à la fois » —
//! qui ne vivait que dans ce module (`state.rs` exposait déjà [`AppState::
//! graphs`], la liste complète, sans jamais l'avoir imposée) — disparaît
//! ici. Un onglet [`Tab::Graph`] par graphe ouvert, chacun avec son propre
//! [`GraphView`] (donc son propre cache de formes, la règle déjà en vigueur
//! phase R4) : [`PlotyxApp::graph_views`] remplace le champ `graph: GraphView`
//! unique d'avant cette tâche. Le menu « Graphes » de la barre de menu ouvre
//! ou focalise l'onglet d'un graphe du projet ; [`PlotyxApp::open_graph_tab`]
//! est le seul endroit qui décide « ouvrir » vs « focaliser ».
//!
//! # Sélecteur de fichiers natif
//!
//! [`tool_bar`](PlotyxApp::tool_bar) n'a plus de champ de texte pour le
//! chemin : les boutons « Enregistrer sous… » et « Ouvrir… » ouvrent un
//! `rfd::FileDialog` natif (modal, comme tout sélecteur de fichiers de
//! bureau — bloquer le rendu le temps que l'utilisateur choisisse un fichier
//! est le comportement attendu, à ne pas confondre avec le blocage qu'évite
//! `RunScript`, qui lui n'a pas de raison de durer). Ctrl+S et Ctrl+O
//! (et les entrées de menu équivalentes) continuent d'agir directement sur
//! le chemin déjà connu, sans dialogue : c'est le geste « enregistrer »/
//! « recharger » rapide d'un tableur, distinct de « enregistrer sous ».
//!
//! # Limites assumées du banc
//!
//! - **Les raccourcis sont consommés au niveau du contexte**, donc *avant*
//!   les widgets. Conséquence mesurable : Ctrl+Z pendant la saisie d'une
//!   cellule annule la dernière **commande du modèle**, pas la dernière
//!   frappe dans le champ — l'annulation interne des `TextEdit` d'egui est
//!   masquée. C'est le comportement attendu d'un tableur, et c'est aussi la
//!   limite : le banc ne sait pas distinguer « annuler ma saisie en cours »
//!   de « annuler la dernière commande ».
//! - **Une seule cellule est éditable à la fois.** La grille est virtualisée
//!   (`TableBuilder::body().rows()`), il n'est donc pas question de garder un
//!   tampon par cellule : une feuille peut avoir un million de lignes.
//!
//! # La console de script ne fige pas la fenêtre
//!
//! `ScriptEngine::run` **bloque** jusqu'au terme du script ; l'appeler depuis
//! cette boucle de rendu figerait la fenêtre entière, bouton « Arrêter »
//! compris — le défaut le plus visible que cette console puisse avoir. La
//! parade n'est **pas** ici : elle vit dans [`crate::state::AppState::
//! dispatch`], qui traite `RunScript` en lançant le travail sur un fil
//! détaché et en revenant aussitôt (voir la doc de tête de `crate::state`).
//! Ce module n'a donc que deux choses à faire, chaque image : appeler
//! [`AppState::poll_script`] pour récupérer un résultat prêt, et demander un
//! nouveau rendu ([`egui::Context::request_repaint`]) tant qu'un script
//! tourne — sans quoi, egui ne redessinant que sur événement d'entrée, la fin
//! du script n'apparaîtrait qu'au prochain mouvement de souris.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use egui_extras::{Column as TableColumn, TableBuilder};
use plotyx_core::{ColumnType, NodeId};
use plotyx_render_egui::{DrawnPlot, GraphView};

use crate::state::{Action, AppState};

/// Variable d'environnement du mécanisme d'auto-fermeture (voir
/// [`PlotyxApp::new`]).
pub const AUTOQUIT_ENV: &str = "PLOTYX_APP_AUTOQUIT_MS";
/// Variable d'environnement du mécanisme de capture (voir
/// [`PlotyxApp::screenshot`]).
pub const SCREENSHOT_ENV: &str = "PLOTYX_APP_SCREENSHOT";
/// Images laissées passer avant de demander une capture.
///
/// egui résout la taille de ses panneaux sur deux ou trois images : capturer
/// la première montrerait une mise en page transitoire — et, pour un onglet
/// de graphe, un compteur de cache qui n'a pas encore eu l'occasion de
/// servir une image inchangée. On regarde le régime établi, pas le
/// démarrage.
const SCREENSHOT_SETTLE_FRAMES: u32 = 8;

/// Ctrl+Z — annuler.
pub const SHORTCUT_UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Y — rétablir (convention Windows).
pub const SHORTCUT_REDO_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
/// Ctrl+Maj+Z — rétablir (convention Unix / macOS). Les deux sont acceptées.
pub const SHORTCUT_REDO_Z: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);
/// Ctrl+S — enregistrer (dans le fichier déjà connu, sans dialogue).
pub const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Ctrl+O — ouvrir (recharger depuis le chemin déjà connu, sans dialogue).
pub const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
/// Ctrl+Entrée — exécuter le script de la console.
pub const SHORTCUT_RUN_SCRIPT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// Hauteur d'une ligne de la grille, en points.
const ROW_HEIGHT: f32 = 20.0;
/// Hauteur de la ligne d'en-tête, en points.
const HEADER_HEIGHT: f32 = 22.0;
/// Hauteur de la zone de sortie du script, en points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;
/// Fraction de hauteur laissée à la moitié haute (grille/graphes) du plan
/// principal quand la console de script est amarrée en bas au démarrage —
/// pendant `PlotyxApp::new`, choisie pour approcher la même proportion que
/// l'ancien panneau fixe (`SCRIPT_OUTPUT_HEIGHT` sur la hauteur par défaut de
/// la fenêtre, `main.rs`). L'utilisateur peut ensuite redimensionner ou
/// redisposer librement : ce n'est qu'un point de départ.
const INITIAL_TOP_FRACTION: f32 = 0.78;
/// Fraction de largeur laissée à la grille quand un graphe est ouvert au
/// démarrage — même logique que [`INITIAL_TOP_FRACTION`], approchant l'ancien
/// `GRAPH_PANEL_WIDTH` fixe.
const INITIAL_GRID_FRACTION: f32 = 0.58;

/// Un onglet du plan de disposition. Trois variantes seulement — la grille,
/// la console de script, et un graphe donné — parce que c'est tout ce que ce
/// banc affiche aujourd'hui (voir la doc de tête du module : pas de panneau
/// Properties ni de grille à délégués ici).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    /// La feuille sélectionnée, en grille virtualisée.
    Grid,
    /// La console de script Python.
    Script,
    /// Un graphe ouvert, identifié par son nœud.
    Graph(NodeId),
}

/// L'application `eframe` : l'état du banc, la disposition dockable, et le
/// strict nécessaire au rendu.
pub struct PlotyxApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// Le plan de disposition : quels onglets, où, dans quel agencement de
    /// panneaux amarrables. Persistant d'une image à l'autre — c'est ce qui
    /// retient les glissers-déposés de l'utilisateur.
    dock_state: DockState<Tab>,
    /// Une vue de graphe (et son cache de formes, phase R4) par graphe
    /// **ouvert** — c'est-à-dire ayant eu un onglet au moins une fois,
    /// l'entrée survit à la fermeture de l'onglet. Un cache par vue est la
    /// règle (`plotyx_render_egui::GraphView`) ; avec plusieurs graphes
    /// affichables simultanément, une seule vue ne suffit plus.
    graph_views: HashMap<NodeId, GraphView>,
    /// Fichier où déposer une capture de la fenêtre, s'il en est demandé une
    /// (mécanisme de vérification, voir [`PlotyxApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Images restant à laisser passer avant de demander la capture.
    screenshot_delay: u32,
    /// Délai d'auto-fermeture, s'il est demandé (mécanisme de vérification).
    autoquit: Option<Duration>,
    /// Instant du premier rendu, d'où se mesure le délai d'auto-fermeture.
    first_frame: Option<Instant>,
}

impl PlotyxApp {
    /// Construit l'application autour d'un état.
    ///
    /// # Disposition initiale
    ///
    /// La grille occupe le plan principal ; la console de script est amarrée
    /// en bas (comme l'ancien panneau fixe) ; si le projet contient au moins
    /// un graphe, le premier est ouvert à droite de la grille (comme l'ancien
    /// comportement par défaut — montrer quelque chose sans qu'on ait à
    /// cliquer). Tout ceci n'est qu'un point de départ : `egui_dock` retient
    /// ensuite les redispositions de l'utilisateur.
    ///
    /// # Auto-fermeture
    ///
    /// Si la variable d'environnement `PLOTYX_APP_AUTOQUIT_MS` contient un
    /// entier, la fenêtre se ferme d'elle-même après ce délai en
    /// millisecondes, mesuré depuis le premier rendu. **C'est un mécanisme de
    /// vérification, pas une fonctionnalité** : il permet de prouver en ligne
    /// de commande que la fenêtre s'ouvre et peint réellement — ce qu'aucun
    /// test `cargo` ne peut faire — sans qu'un humain ait à cliquer. Une
    /// valeur illisible est ignorée en silence : le banc ne doit pas refuser
    /// de démarrer à cause d'un outillage de vérification.
    pub fn new(state: AppState) -> Self {
        let autoquit = std::env::var(AUTOQUIT_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .map(Duration::from_millis);

        let mut dock_state = DockState::new(vec![Tab::Grid]);
        let surface = dock_state.main_surface_mut();
        let [top, _script] =
            surface.split_below(NodeIndex::root(), INITIAL_TOP_FRACTION, vec![Tab::Script]);
        if let Some((first_graph, _)) = state.graphs().into_iter().next() {
            surface.split_right(top, INITIAL_GRID_FRACTION, vec![Tab::Graph(first_graph)]);
        }

        Self {
            state,
            dock_state,
            graph_views: HashMap::new(),
            autoquit,
            first_frame: None,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(std::path::PathBuf::from),
            screenshot_delay: SCREENSHOT_SETTLE_FRAMES,
        }
    }

    /// Consomme les raccourcis au niveau du contexte et en fait des actions.
    fn collect_shortcuts(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        ctx.input_mut(|input| {
            if input.consume_shortcut(&SHORTCUT_SAVE) {
                actions.push(Action::Save);
            }
            if input.consume_shortcut(&SHORTCUT_OPEN) {
                actions.push(Action::Load);
            }
            if input.consume_shortcut(&SHORTCUT_RUN_SCRIPT) {
                actions.push(Action::RunScript);
            }
            // Le rétablissement se teste avant l'annulation : sans cela,
            // Ctrl+Maj+Z serait avalé par le raccourci Ctrl+Z, dont les
            // modificateurs sont compatibles.
            if input.consume_shortcut(&SHORTCUT_REDO_Z) || input.consume_shortcut(&SHORTCUT_REDO_Y)
            {
                actions.push(Action::Redo);
            } else if input.consume_shortcut(&SHORTCUT_UNDO) {
                actions.push(Action::Undo);
            }
        });
    }

    /// Barre de menu : « Fichier », « Édition » et « Graphes ».
    ///
    /// « Graphes » liste tous les graphes du projet (pas seulement ceux déjà
    /// ouverts) : cliquer un graphe déjà ouvert focalise son onglet, cliquer
    /// un graphe fermé lui en ouvre un — [`PlotyxApp::open_graph_tab`] fait
    /// les deux. C'est le seul endroit qui ouvre un onglet de graphe hors du
    /// point de départ posé par [`PlotyxApp::new`].
    fn menu_bar(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
        open_graphs: &mut Vec<NodeId>,
    ) {
        egui::Panel::top("plotyx_menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui
                        .add(
                            egui::Button::new("Ouvrir")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_OPEN)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Load);
                        ui.close();
                    }
                    if ui
                        .add(
                            egui::Button::new("Enregistrer")
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Save);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
                ui.menu_button("Édition", |ui| {
                    // Le libellé complet (« Annuler Saisie dans A ») et le
                    // grisage sont deux lignes distinctes ici ; Qt les tenait
                    // ensemble dans une seule `QAction`.
                    let undo = self
                        .state
                        .undo_label()
                        .unwrap_or_else(|| "Annuler".to_string());
                    if ui
                        .add_enabled(
                            self.state.can_undo(),
                            egui::Button::new(undo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Undo);
                        ui.close();
                    }
                    let redo = self
                        .state
                        .redo_label()
                        .unwrap_or_else(|| "Rétablir".to_string());
                    if ui
                        .add_enabled(
                            self.state.can_redo(),
                            egui::Button::new(redo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_REDO_Y)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Redo);
                        ui.close();
                    }
                });
                ui.menu_button("Graphes", |ui| {
                    let graphs = self.state.graphs();
                    if graphs.is_empty() {
                        ui.weak("Aucun graphe dans ce projet.");
                    }
                    for (id, path) in graphs {
                        if ui.button(path).clicked() {
                            open_graphs.push(id);
                            ui.close();
                        }
                    }
                });
            });
        });
    }

    /// Bandeau du chemin de fichier (lecture seule) et des actions de
    /// structure. « Enregistrer sous… » et « Ouvrir… » passent par un
    /// sélecteur de fichiers natif (`rfd`) ; voir la doc de tête du module.
    fn tool_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::top("plotyx_tool_bar")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fichier :");
                    ui.weak(self.state.path());
                    if ui.button("Enregistrer sous…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Enregistrer le projet")
                            .add_filter("Projet PlotYx", &["plx", "plotyx"])
                            .set_file_name(suggested_file_name(self.state.path()))
                            .save_file()
                        {
                            self.state.set_path(path.display().to_string());
                            actions.push(Action::Save);
                        }
                    }
                    if ui.button("Enregistrer").clicked() {
                        actions.push(Action::Save);
                    }
                    if ui.button("Ouvrir…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Ouvrir un projet")
                            .add_filter("Projet PlotYx", &["plx", "plotyx"])
                            .pick_file()
                        {
                            self.state.set_path(path.display().to_string());
                            actions.push(Action::Load);
                        }
                    }
                    ui.separator();
                    if ui.button("Ajouter une colonne").clicked() {
                        actions.push(Action::AddColumn);
                    }
                    if ui.button("Insérer 1 ligne").clicked() {
                        // B5 (`guide-de-correction-robustesse.md` §F5) —
                        // `at: 0` en dur n'insérait jamais rien sur une
                        // feuille neuve : `row_targets` exige `at < len`, et
                        // une colonne fraîchement créée a `len == 0`. Ajouter
                        // une ligne en fin de feuille (`at == row_count`) est
                        // le geste que le bouton promet, et c'est la plage
                        // que `Project::insert_rows` accepte désormais.
                        actions.push(Action::InsertRows {
                            at: self.state.row_count(),
                            count: 1,
                        });
                    }
                    if ui.button("Supprimer 1 ligne").clicked() {
                        actions.push(Action::DeleteRows { at: 0, count: 1 });
                    }
                });
            });
    }

    /// Explorateur des feuilles du projet.
    fn sheet_panel(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::left("plotyx_sheets")
            .default_size(190.0)
            .show(ui, |ui| {
                ui.heading("Feuilles");
                ui.separator();
                let selected = self.state.selected_sheet();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (id, path) in self.state.sheets() {
                        if ui.selectable_label(selected == Some(id), path).clicked() {
                            actions.push(Action::SelectSheet(id));
                        }
                    }
                });
            });
    }

    /// Barre d'état : dernier message, révision, dimensions.
    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("plotyx_status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match self.state.status() {
                        Some(status) if status.is_error => {
                            ui.colored_label(ui.visuals().error_fg_color, &status.text);
                        }
                        Some(status) => {
                            ui.label(&status.text);
                        }
                        None => {
                            ui.weak("prêt");
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!(
                            "révision {} — {} lignes × {} colonnes",
                            self.state.snapshot().revision,
                            self.state.row_count(),
                            self.state.visible_columns().len()
                        ));
                    });
                });
            });
    }

    /// Ouvre l'onglet du graphe `id`, ou le focalise s'il est déjà ouvert.
    ///
    /// Le seul endroit qui décide entre les deux : un double-clic sur un
    /// graphe déjà ouvert dans le menu « Graphes » ne doit pas dupliquer son
    /// onglet, il doit l'amener au premier plan.
    fn open_graph_tab(&mut self, id: NodeId) {
        let tab = Tab::Graph(id);
        if let Some(path) = self.dock_state.find_tab(&tab) {
            let _ = self.dock_state.set_active_tab(path);
            self.dock_state
                .set_focused_node_and_surface(path.node_path());
        } else {
            self.dock_state.push_to_focused_leaf(tab);
        }
    }

    /// Dépose une capture de la fenêtre dans le fichier nommé par
    /// `PLOTYX_APP_SCREENSHOT`, puis n'en redemande plus.
    ///
    /// **Mécanisme de vérification, pas une fonctionnalité** — exactement
    /// comme l'auto-fermeture, et pour un besoin voisin apparu avec la
    /// phase R4 : sur un chantier de rendu, « la fenêtre s'ouvre et ne
    /// plante pas » ne suffit plus, il faut pouvoir *regarder* ce qu'elle
    /// peint. Les tests couvrent la géométrie (`plotyx-render-egui`), pas
    /// l'apparence ; et une capture prise par la fenêtre elle-même est le
    /// seul moyen d'y accéder sous Wayland, où aucun outil externe ne peut
    /// photographier une fenêtre donnée.
    ///
    /// Le format est **PPM binaire** (`P6`), sept lignes de code et aucune
    /// dépendance nouvelle. Le convertir en PNG est l'affaire d'un
    /// `magick capture.ppm capture.png` ; faire entrer un encodeur PNG dans
    /// le banc pour un outil de vérification serait payer cher un confort.
    ///
    /// Une erreur d'écriture est signalée sur la sortie d'erreur et rien de
    /// plus : le banc ne doit pas s'arrêter parce qu'un outillage de
    /// vérification a échoué.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        if self.screenshot_delay > 0 {
            self.screenshot_delay -= 1;
            ctx.request_repaint();
            return;
        }
        // Une capture arrive à l'image *suivant* la demande : tant qu'on n'a
        // rien reçu, on redemande.
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            return;
        };
        self.screenshot = None;

        let [width, height] = image.size;
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in &image.pixels {
            let [r, g, b, _] = pixel.to_srgba_unmultiplied();
            ppm.extend_from_slice(&[r, g, b]);
        }
        match std::fs::write(&path, ppm) {
            Ok(()) => eprintln!("[plotyx-app] capture écrite : {}", path.display()),
            Err(err) => eprintln!(
                "[plotyx-app] capture impossible dans {} : {err}",
                path.display()
            ),
        }
    }

    /// Ferme la fenêtre si le délai d'auto-fermeture est écoulé.
    fn autoquit(&mut self, ctx: &egui::Context) {
        let Some(delay) = self.autoquit else {
            return;
        };
        let start = *self.first_frame.get_or_insert_with(Instant::now);
        let elapsed = start.elapsed();
        if elapsed >= delay {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Sans événement d'entrée, egui ne redessine pas : il faut lui
            // demander de se réveiller, sinon l'échéance n'arrive jamais.
            ctx.request_repaint_after(delay - elapsed);
        }
    }
}

impl eframe::App for PlotyxApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // `Context` est un `Arc` : le cloner coûte un compteur, et évite
        // d'emprunter `ui` en lecture pendant qu'on le peint.
        let ctx = ui.ctx().clone();
        let mut actions: Vec<Action> = Vec::new();
        let mut open_graphs: Vec<NodeId> = Vec::new();

        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `crate::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();

        self.collect_shortcuts(&ctx, &mut actions);
        self.menu_bar(ui, &ctx, &mut actions, &mut open_graphs);
        self.tool_bar(ui, &mut actions);
        self.status_bar(ui);
        self.sheet_panel(ui, &mut actions);

        // Emprunts disjoints des champs (édition 2021) : la disposition et
        // les contenus qu'elle amarre ont besoin, chacun, d'un accès distinct
        // à `self`, simultanément le temps de peindre une image.
        let Self {
            state,
            dock_state,
            graph_views,
            ..
        } = self;
        egui::CentralPanel::default().show(ui, |ui| {
            let mut viewer = AppTabViewer {
                state,
                actions: &mut actions,
                graph_views,
                ctx: &ctx,
            };
            DockArea::new(dock_state)
                .style(Style::from_egui(ui.style()))
                .show_inside(ui, &mut viewer);
        });

        // Les onglets de graphe demandés depuis le menu, et les actions
        // récoltées par tous les panneaux et onglets, ne sont appliqués
        // qu'une fois l'image peinte : muter le plan de disposition ou le
        // modèle en plein rendu invaliderait ce que la `DockArea` est en
        // train de parcourir.
        for id in open_graphs {
            self.open_graph_tab(id);
        }
        for action in actions {
            self.state.dispatch(action);
        }

        self.screenshot(&ctx);
        self.autoquit(&ctx);
    }
}

/// Le nom de fichier suggéré par défaut au sélecteur « Enregistrer sous… » :
/// le nom du chemin courant s'il en a un, sinon un nom de repli.
fn suggested_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "demo.plotyx".to_string())
}

/// Peint le contenu de chaque onglet du plan de disposition.
///
/// Emprunte les champs de [`PlotyxApp`] plutôt que l'application entière
/// (voir `PlotyxApp::ui`) : c'est ce qui permet à la `DockArea`
/// (`&mut dock_state`) et à ce visiteur (`&mut state`, `&mut graph_views`)
/// de coexister le temps d'une image.
struct AppTabViewer<'a> {
    state: &'a mut AppState,
    actions: &'a mut Vec<Action>,
    graph_views: &'a mut HashMap<NodeId, GraphView>,
    ctx: &'a egui::Context,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            Tab::Grid => "Feuille".into(),
            Tab::Script => "Script Python".into(),
            Tab::Graph(id) => self
                .state
                .graphs()
                .into_iter()
                .find(|(graph_id, _)| *graph_id == id)
                .map(|(_, path)| path)
                .unwrap_or_else(|| format!("Graphe {id} (supprimé)"))
                .into(),
        }
    }

    // Les onglets structurels (grille, script) ne se ferment pas : ils n'ont
    // pas d'équivalent « rouvrir » dans ce banc volontairement pauvre. Un
    // onglet de graphe, lui, se ferme et se rouvre librement — c'est
    // `PlotyxApp::open_graph_tab`, via le menu « Graphes », qui rouvre.
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        matches!(tab, Tab::Graph(_))
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match *tab {
            Tab::Grid => grid_contents(ui, self.state, self.actions),
            Tab::Script => script_contents(ui, self.ctx, self.state, self.actions),
            Tab::Graph(id) => graph_contents(ui, self.state, self.graph_views, id),
        }
    }
}

/// Suffixe affiché sous le nom d'une colonne pour rappeler son type.
fn type_suffix(column_type: ColumnType) -> &'static str {
    match column_type {
        ColumnType::Numeric => "num",
        ColumnType::DateTime => "date",
        ColumnType::Text => "texte",
    }
}

/// La grille, en lignes **virtualisées** : seules les lignes visibles sont
/// construites, sans quoi une feuille d'un million de lignes bloquerait
/// chaque image. Contenu de [`Tab::Grid`].
fn grid_contents(ui: &mut egui::Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let columns = state.visible_columns();
    let row_count = state.row_count();
    if columns.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak("Aucune colonne à afficher : choisissez une feuille à gauche.");
        });
        return;
    }
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(TableColumn::exact(56.0))
        .columns(TableColumn::initial(110.0).at_least(56.0), columns.len())
        .min_scrolled_height(0.0)
        .header(HEADER_HEIGHT, |mut header| {
            header.col(|ui| {
                ui.strong("#");
            });
            for (_, name, column_type) in &columns {
                header.col(|ui| {
                    ui.strong(name);
                    ui.weak(type_suffix(*column_type));
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, row_count, |mut row| {
                let index = row.index();
                row.col(|ui| {
                    ui.weak(format!("{}", index + 1));
                });
                for (id, _, _) in &columns {
                    row.col(|ui| {
                        cell_widget(ui, state, actions, *id, index);
                    });
                }
            });
        });
}

/// Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
/// sortie. Contenu de [`Tab::Script`].
///
/// Ne bloque jamais la fenêtre : voir la doc de tête de ce module.
/// `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
/// quelle autre action, appliquées après le rendu par la boucle
/// principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
/// bloquent pas.
fn script_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let running = state.script_running();
    if running {
        // Sans ça, egui ne redessine que sur événement d'entrée : la fin du
        // script n'apparaîtrait qu'au prochain geste de l'utilisateur.
        ctx.request_repaint();
    }

    ui.horizontal(|ui| {
        ui.heading("Script Python");
        if !state.script_available() {
            ui.weak(
                "— compilé sans le pont Python : la surface pyx n'est pas disponible dans ce binaire.",
            );
        }
    });
    ui.separator();

    let available = state.script_available() && !running;
    let mut source = state.script_source().to_string();
    let edit = ui.add_enabled(
        available,
        egui::TextEdit::multiline(&mut source)
            .code_editor()
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    if edit.changed() {
        state.set_script_source(source);
    }

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                egui::Button::new("Exécuter")
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new("Arrêter"))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            ui.spinner();
            ui.weak("exécution en cours…");
        }
    });

    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match state.script_output() {
            None => {
                ui.weak("Aucune exécution pour l'instant.");
            }
            Some(output) => {
                let mut empty = true;
                if !output.stdout.is_empty() {
                    empty = false;
                    ui.label("Sortie standard :");
                    ui.monospace(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    empty = false;
                    ui.label("Sortie d'erreur :");
                    ui.monospace(&output.stderr);
                }
                if let Some(error) = &output.error {
                    empty = false;
                    ui.colored_label(ui.visuals().warn_fg_color, "Trace d'appels :");
                    ui.monospace(error);
                }
                if empty {
                    ui.weak("(sortie vide)");
                }
            }
        });
}

/// Un graphe ouvert — le **rendu visible** de la phase R4 (tâche 4.6).
/// Contenu d'un [`Tab::Graph`].
///
/// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout ce
/// banc, c'est la preuve que l'intégration bout en bout tient. Ce qu'il
/// montre vraiment, et qu'aucun test ne montre :
///
/// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
/// - le compteur de cache, qui rend visible la tâche R4.3. Au repos,
///   `succès` monte et `calculs` ne bouge plus : une image sans changement
///   ne rejoue pas le pipeline Plotters. Redimensionner l'onglet, ou
///   modifier une cellule, fait repartir `calculs` d'un cran — c'est la clé
///   `(revision, rect, pixels_per_point)` au travail.
fn graph_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    let available = ui.available_size_before_wrap();
    // R7 : la navigation (molette, glissé, rectangle élastique) a besoin de
    // bien plus que `hover()` — `GraphView::paint` fait sa propre
    // `ui.interact` sur ce rectangle avec `click_and_drag()` ; le `Sense`
    // posé ici doit au moins ne pas lui faire concurrence.
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - 24.0).max(120.0)),
        egui::Sense::click_and_drag(),
    );
    match view.paint(ui, rect, state.snapshot(), graph_id) {
        Ok(_) => {
            let cache = view.cache();
            ui.weak(format!(
                "cache : {} succès / {} calculs — {} formes",
                cache.hits(),
                cache.misses(),
                cache.shapes().len()
            ));
            if let Some(drawn) = view.last_drawn() {
                ui.weak(figure_summary(drawn));
            }
            if let Some(hit) = view.hovered_point() {
                ui.weak(format!(
                    "point désigné : panneau {}, série {}, indice {} — ({:.4}, {:.4}), à {:.1} px",
                    hit.panel, hit.series, hit.point_index, hit.data.0, hit.data.1, hit.distance
                ));
            }
            if ui.small_button("réinitialiser le zoom").clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(egui::Color32::RED, format!("rendu : {err}"));
        }
    }
}

/// Ce que la figure a réellement tracé, en une ligne.
///
/// Ajouté en R5 : avec les panneaux multiples, les types de tracé et la
/// décimation, « la figure s'affiche » ne dit plus grand-chose. Les quatre
/// compteurs du `DrawnPlot` disent lesquels des chemins de la phase ont été
/// empruntés — et c'est le seul endroit où on peut les lire sur des données
/// réelles plutôt que sur une fixture de test.
fn figure_summary(drawn: &DrawnPlot) -> String {
    let mut ligne = format!(
        "figure : {} panneau(x), {} série(s) tracée(s)",
        drawn.panels.len(),
        drawn.drawn_series
    );
    if drawn.broken_series > 0 {
        ligne.push_str(&format!(
            " — {} référence(s) brisée(s)",
            drawn.broken_series
        ));
    }
    if drawn.unknown_series > 0 {
        ligne.push_str(&format!(
            " — {} type(s) de tracé inconnu(s)",
            drawn.unknown_series
        ));
    }
    if drawn.decimated_points > 0 {
        ligne.push_str(&format!(" — {} points décimés", drawn.decimated_points));
    }
    ligne
}

/// Peint une cellule et récolte les actions qu'elle produit.
///
/// Une seule cellule à la fois porte un vrai tampon (celui de
/// [`AppState::editing`]) ; toutes les autres sont peintes sur une chaîne
/// jetable, ce qui garde le coût mémoire de la grille indépendant du nombre
/// de lignes.
///
/// Le contenu n'est envoyé au modèle qu'à la **perte de focus** — ce que
/// provoque aussi la touche Entrée — jamais à chaque frappe : sinon
/// l'historique compterait une entrée par caractère.
fn cell_widget(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    column: NodeId,
    row: usize,
) {
    // L'identifiant est ancré sur (colonne, ligne) et non sur la position à
    // l'écran : avec des lignes virtualisées, faire défiler déplacerait sinon
    // le focus d'une cellule à l'autre.
    let inner = ui.push_id((column.raw(), row), |ui| {
        let editing_here = state
            .editing()
            .is_some_and(|edit| edit.column == column && edit.row == row);

        if editing_here {
            let edit = state.editing_mut()?;
            let response = ui.add(
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            Some((response, None))
        } else {
            let original = state.cell_text(column, row);
            let mut shown = original.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut shown)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            Some((response, Some((original, shown))))
        }
    });

    let Some((response, typed)) = inner.inner else {
        return;
    };

    if response.gained_focus() {
        state.begin_edit(column, row);
        // Si l'image qui a donné le focus portait déjà une frappe, elle a
        // atterri dans la chaîne jetable : on la reprend, sinon le premier
        // caractère serait perdu.
        if let Some((original, shown)) = typed {
            if shown != original {
                if let Some(edit) = state.editing_mut() {
                    edit.buffer = shown;
                }
            }
        }
        return;
    }

    if !response.lost_focus() {
        return;
    }

    // Échap referme la saisie sans rien écrire ; toute autre perte de focus
    // (clic ailleurs, Tab, Entrée) valide.
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        state.cancel_edit();
        return;
    }

    let pending = state
        .editing()
        .filter(|edit| edit.column == column && edit.row == row)
        .map(|edit| edit.buffer.clone());
    if let Some(text) = pending {
        state.cancel_edit();
        actions.push(Action::CommitCell { column, row, text });
    }
}
