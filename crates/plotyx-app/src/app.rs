//! La glu `eframe` : traduit clavier et souris en [`Action`] et peint
//! l'[`AppState`]. Aucune logique ne vit ici — tout ce qui décide est dans
//! [`crate::state`], qui se teste sans fenêtre.
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
//! # Limites assumées du banc
//!
//! - **Pas de sélecteur de fichiers natif.** Le chemin se tape dans un champ
//!   de texte. Un vrai sélecteur veut une dépendance de plus (`rfd`) et, sous
//!   Wayland, un portail système : deux choses qu'on ne veut pas faire entrer
//!   dans la première intégration bout en bout.
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

use std::time::{Duration, Instant};

use eframe::egui;
use egui_extras::{Column as TableColumn, TableBuilder};
use plotyx_core::{ColumnType, NodeId};
use plotyx_render_egui::GraphView;

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
/// la première montrerait une mise en page transitoire — et, pour le panneau
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
/// Ctrl+S — enregistrer.
pub const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Ctrl+O — ouvrir (recharger depuis le chemin affiché).
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
/// Largeur par défaut du panneau de graphe, en points.
const GRAPH_PANEL_WIDTH: f32 = 460.0;

/// L'application `eframe` : l'état du banc, plus le strict nécessaire au
/// rendu.
pub struct PlotyxApp {
    /// L'état, seul dépositaire de la logique.
    state: AppState,
    /// La vue de graphe et son cache de formes (phase R4). Une seule : le
    /// banc n'affiche qu'un graphe à la fois, et un cache par vue est la
    /// règle (`plotyx_render_egui::GraphView`).
    graph: GraphView,
    /// Le graphe affiché, s'il y en a un. `None` tant qu'aucun n'a été
    /// choisi ni trouvé.
    shown_graph: Option<NodeId>,
    /// Fichier où déposer une capture de la fenêtre, s'il en est demandé une
    /// (mécanisme de vérification, voir [`PlotyxApp::screenshot`]).
    screenshot: Option<std::path::PathBuf>,
    /// Images restant à laisser passer avant de demander la capture.
    screenshot_delay: u32,
    /// Délai d'auto-fermeture, s'il est demandé (mécanisme de vérification).
    autoquit: Option<Duration>,
    /// Instant du premier rendu, d'où se mesure le délai d'auto-fermeture.
    first_frame: Option<Instant>,
    /// Vrai si la console de script est dépliée. Pur affichage : ça ne pilote
    /// aucune décision de [`AppState`], donc ça ne vit pas là-bas.
    script_panel_open: bool,
}

impl PlotyxApp {
    /// Construit l'application autour d'un état.
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
        Self {
            state,
            autoquit,
            first_frame: None,
            script_panel_open: true,
            graph: GraphView::new(),
            shown_graph: None,
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

    /// Barre de menu : « Fichier » et « Édition ».
    fn menu_bar(&self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
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
            });
        });
    }

    /// Bandeau du chemin de fichier et des actions de structure.
    fn tool_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let mut path = self.state.path().to_string();
        let mut edited = false;
        egui::Panel::top("plotyx_tool_bar")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fichier :");
                    // Pas de sélecteur natif : voir la limite documentée en tête
                    // de module.
                    edited = ui
                        .add(
                            egui::TextEdit::singleline(&mut path)
                                .hint_text("chemin du fichier .plotyx")
                                .desired_width(360.0),
                        )
                        .changed();
                    if ui.button("Enregistrer").clicked() {
                        actions.push(Action::Save);
                    }
                    if ui.button("Ouvrir").clicked() {
                        actions.push(Action::Load);
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
        if edited {
            self.state.set_path(path);
        }
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

    /// Console de script : zone de saisie, boutons « Exécuter »/« Arrêter »,
    /// sortie. Redimensionnable (bord du panneau) et repliable (bouton ▾/▸
    /// dans son en-tête).
    ///
    /// Ne bloque jamais la fenêtre : voir la doc de tête de ce module.
    /// `Action::RunScript`/`InterruptScript` sont poussées comme n'importe
    /// quelle autre action, appliquées après le rendu par la boucle
    /// principale — c'est [`AppState::dispatch`] qui garantit qu'elles ne
    /// bloquent pas.
    fn script_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::Panel::bottom("plotyx_script")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                let running = self.state.script_running();
                if running {
                    // Sans ça, egui ne redessine que sur événement d'entrée :
                    // la fin du script n'apparaîtrait qu'au prochain geste de
                    // l'utilisateur.
                    ctx.request_repaint();
                }

                ui.horizontal(|ui| {
                    let toggle = if self.script_panel_open { "▾" } else { "▸" };
                    if ui.button(toggle).clicked() {
                        self.script_panel_open = !self.script_panel_open;
                    }
                    ui.heading("Script Python");
                    if !self.state.script_available() {
                        ui.weak("— compilé sans le pont Python : la surface pyx n'est pas disponible dans ce binaire.");
                    }
                });
                if !self.script_panel_open {
                    return;
                }
                ui.separator();

                let available = self.state.script_available() && !running;
                let mut source = self.state.script_source().to_string();
                let edit = ui.add_enabled(
                    available,
                    egui::TextEdit::multiline(&mut source)
                        .code_editor()
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                if edit.changed() {
                    self.state.set_script_source(source);
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
                    .show(ui, |ui| match self.state.script_output() {
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
            });
    }

    /// La grille, en lignes **virtualisées** : seules les lignes visibles sont
    /// construites, sans quoi une feuille d'un million de lignes bloquerait
    /// chaque image.
    fn grid(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let columns = self.state.visible_columns();
        let row_count = self.state.row_count();
        let state = &mut self.state;
        egui::CentralPanel::default().show(ui, |ui| {
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
        });
    }

    /// Panneau de graphe — le **rendu visible** de la phase R4 (tâche 4.6).
    ///
    /// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout
    /// ce banc, c'est la preuve que l'intégration bout en bout tient. Ce
    /// qu'il montre vraiment, et qu'aucun test ne montre :
    ///
    /// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
    /// - le compteur de cache, qui rend visible la tâche R4.3. Au repos,
    ///   `succès` monte et `calculs` ne bouge plus : une image sans
    ///   changement ne rejoue pas le pipeline Plotters. Redimensionner la
    ///   fenêtre, ou modifier une cellule, fait repartir `calculs` d'un cran
    ///   — c'est la clé `(revision, rect, pixels_per_point)` au travail.
    fn graph_panel(&mut self, ui: &mut egui::Ui) {
        let graphs = self.state.graphs();
        if graphs.is_empty() {
            return;
        }
        // Par défaut, le premier graphe du projet : le banc doit montrer
        // quelque chose sans qu'on ait à cliquer.
        if !self
            .shown_graph
            .is_some_and(|shown| graphs.iter().any(|(id, _)| *id == shown))
        {
            self.shown_graph = graphs.first().map(|(id, _)| *id);
        }

        let state = &self.state;
        let view = &mut self.graph;
        let shown = &mut self.shown_graph;
        egui::Panel::right("plotyx_graph")
            .default_size(GRAPH_PANEL_WIDTH)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Graphe");
                    for (id, path) in &graphs {
                        if ui.selectable_label(*shown == Some(*id), path).clicked() {
                            *shown = Some(*id);
                        }
                    }
                });
                ui.separator();

                let Some(graph) = *shown else {
                    return;
                };
                let available = ui.available_size_before_wrap();
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(available.x, (available.y - 24.0).max(120.0)),
                    egui::Sense::hover(),
                );
                match view.paint(ui.painter(), rect, state.snapshot(), graph) {
                    Ok(_) => {
                        let cache = view.cache();
                        ui.weak(format!(
                            "cache : {} succès / {} calculs — {} formes",
                            cache.hits(),
                            cache.misses(),
                            cache.shapes().len()
                        ));
                        if let Some(drawn) = view.last_drawn() {
                            ui.weak(Self::figure_summary(drawn));
                        }
                    }
                    Err(err) => {
                        ui.colored_label(egui::Color32::RED, format!("rendu : {err}"));
                    }
                }
            });
    }

    /// Ce que la figure a réellement tracé, en une ligne.
    ///
    /// Ajouté en R5 : avec les panneaux multiples, les types de tracé et la
    /// décimation, « la figure s'affiche » ne dit plus grand-chose. Les
    /// quatre compteurs du `DrawnPlot` disent lesquels des chemins de la
    /// phase ont été empruntés — et c'est le seul endroit où on peut les
    /// lire sur des données réelles plutôt que sur une fixture de test.
    fn figure_summary(drawn: &plotyx_render_egui::DrawnPlot) -> String {
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

        // Sonde un script terminé sur son fil détaché **avant** de peindre :
        // sans notification poussée de l'acteur, c'est le seul endroit qui
        // rafraîchit l'instantané après un script (voir la doc de tête de
        // `crate::state`). Ne bloque jamais, même sans script en cours.
        self.state.poll_script();

        self.collect_shortcuts(&ctx, &mut actions);
        self.menu_bar(ui, &ctx, &mut actions);
        self.tool_bar(ui, &mut actions);
        self.status_bar(ui);
        self.sheet_panel(ui, &mut actions);
        self.script_panel(ui, &ctx, &mut actions);
        self.graph_panel(ui);
        self.grid(ui, &mut actions);

        // Les actions ne sont appliquées qu'une fois l'image peinte : muter le
        // modèle en plein rendu invaliderait la liste de colonnes que la
        // grille est en train de parcourir.
        for action in actions {
            self.state.dispatch(action);
        }

        self.screenshot(&ctx);
        self.autoquit(&ctx);
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
