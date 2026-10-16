//! Raccourcis clavier de l'application : les constantes `SHORTCUT_*` (celles
//! que l'on consomme et celles, décoratives, qui ne servent qu'au rappel
//! affiché dans un menu), leurs rappels écrits en toutes lettres, et la
//! consommation d'annuler/rétablir dans le viewport actif.
//!
//! Sorti d'`app.rs` tel quel (#117). La consommation globale des raccourcis,
//! `collect_shortcuts`, reste dans `app.rs` (#115).
//!
//! **Registre unique des touches** (#231, K1) : toute touche que l'application
//! consomme est une constante de ce module (`SHORTCUT_*` pour les raccourcis,
//! `KEY_*` pour les touches nues d'un champ ou d'une liste), lue par
//! `input.consume_shortcut(&…)`. Aucun `consume_key` ailleurs (garde
//! `aucun_consume_key_hors_du_registre`, `tests/gardes.rs`). La fenêtre
//! Help › Keyboard Shortcuts est générée de [`HELP`], en bas de ce fichier :
//! un raccourci neuf s'y ajoute, sinon il reste invisible de l'utilisateur.

use eframe::egui;

use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

/// Échap — annule la saisie en cours, ferme la liste ou le dialogue.
pub(crate) const KEY_ESCAPE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Escape);
/// Entrée — valide la saisie en cours ou le dialogue.
pub(crate) const KEY_ENTER: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Enter);
/// Tab — valide la saisie et passe au champ suivant.
pub(crate) const KEY_TAB: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Tab);
/// Suppr — supprime la sélection (la grille d'abord la vide, K2).
pub(crate) const KEY_DELETE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete);
/// Retour arrière — vide la sélection de la grille.
pub(crate) const KEY_BACKSPACE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace);
/// ↑ — valeur voisine d'un champ nombre ou d'une liste.
pub(crate) const KEY_ARROW_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::ArrowUp);
/// ↓ — valeur voisine d'un champ nombre ou d'une liste.
pub(crate) const KEY_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::ArrowDown);
/// Maj+↑ — dix crans vers le haut dans un champ nombre.
pub(crate) const KEY_SHIFT_ARROW_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
/// Maj+↓ — dix crans vers le bas dans un champ nombre.
pub(crate) const KEY_SHIFT_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
/// Alt+↓ — ouvre la liste de valeurs proposées d'un champ nombre.
pub(crate) const KEY_ALT_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::ArrowDown);

/// Ctrl+Z — annuler.
pub(super) const SHORTCUT_UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Y — rétablir (convention Windows).
pub(super) const SHORTCUT_REDO_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
/// Ctrl+Maj+Z — rétablir (convention Unix / macOS). Les deux sont acceptées.
const SHORTCUT_REDO_Z: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);

/// Annuler/rétablir dans le viewport actif, racine ou classeur détaché.
pub(super) fn collect_undo_redo_shortcuts(ctx: &egui::Context, actions: &mut Vec<Action>) {
    ctx.input_mut(|input| {
        // Le rétablissement se teste avant l'annulation : Ctrl+Maj+Z
        // matcherait également Ctrl+Z avec les modificateurs en surplus.
        if input.consume_shortcut(&SHORTCUT_REDO_Z) || input.consume_shortcut(&SHORTCUT_REDO_Y) {
            actions.push(Action::Redo);
        } else if input.consume_shortcut(&SHORTCUT_UNDO) {
            actions.push(Action::Undo);
        }
    });
}

/// Ctrl+S — enregistrer (dans le fichier déjà connu, sans dialogue).
pub(super) const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Ctrl+Maj+S — choisir une destination et y enregistrer.
pub(super) const SHORTCUT_SAVE_AS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::S,
);
/// Ctrl+O — choisir un projet à ouvrir.
pub(super) const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
/// Ctrl+N — nouveau projet (confirmation si des changements seraient perdus).
pub(super) const SHORTCUT_NEW_PROJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
/// Ctrl+E — exporter le graphe actif en SVG, PDF ou PNG (issue #34). Sans
/// effet quand l'onglet actif n'est pas un graphe.
pub(super) const SHORTCUT_EXPORT_GRAPH: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
/// Ctrl+" — ouvrir l'assistant d'import. Sur un clavier AZERTY, `"` est la
/// touche du chiffre 3 **sans** Maj : egui ne connaît pas de touche `"` et
/// retombe sur la touche physique, donc le raccourci se consomme comme
/// Ctrl+3. Sur un clavier QWERTY, `"` est Maj+apostrophe
/// ([`SHORTCUT_IMPORT_QWERTY`]).
pub(super) const SHORTCUT_IMPORT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num3);
/// Ctrl+Maj+' — le même raccourci d'import sur un clavier QWERTY, où `"`
/// s'obtient avec Maj.
pub(super) const SHORTCUT_IMPORT_QWERTY: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Quote,
);
/// Rappel affiché dans le menu : le caractère, pas le nom de la touche
/// physique.
pub(super) const SHORTCUT_IMPORT_TEXT: &str = "Ctrl+\"";
/// Ctrl+Entrée — exécuter le script de la console.
pub(super) const SHORTCUT_RUN_SCRIPT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// F2 — renommer la sélection du Project Manager ou de l’Object Manager.
pub(super) const SHORTCUT_RENAME_OBJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F2);
/// Ctrl+↑ — remonter la série sélectionnée d'un rang (Object Manager et
/// figure ; consommée par `series_shortcuts`).
pub(super) const SHORTCUT_MOVE_SERIES_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::ArrowUp);
/// Ctrl+↓ — descendre la série sélectionnée d'un rang.
pub(super) const SHORTCUT_MOVE_SERIES_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::ArrowDown);
/// F2 — éditer une cellule ou un champ d’en-tête dans la grille focalisée (#143).
pub(super) const SHORTCUT_EDIT_CELL: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F2);
/// Ctrl+D — dupliquer la feuille affichée du classeur actif. Pas de Suppr
/// pour supprimer une feuille : trop de faux positifs, le menu seul suffit.
pub(super) const SHORTCUT_DUPLICATE_SHEET: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::D);

/// Ctrl+D — dupliquer les objets sélectionnés du Project Manager.
pub(super) const SHORTCUT_DUPLICATE_OBJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::D);

/// Ctrl+C — copier la sélection de la grille.
///
/// # Ces cinq raccourcis-ci ne sont **jamais** consommés
///
/// Contrairement à [`SHORTCUT_UNDO`] et compagnie, les cinq constantes qui
/// suivent sont purement **décoratives** : elles n'existent que pour écrire
/// le rappel de raccourci à droite d'une entrée de menu contextuel
/// (`Context::format_shortcut`, qui rend « Ctrl+C » ou « ⌘C » selon la
/// plateforme). Aucun appel à `consume_shortcut` ne les prend, et il ne faut
/// pas essayer : `egui-winit` intercepte Ctrl+C/X/V à la traduction des
/// événements de la fenêtre et n'en laisse jamais passer de `Event::Key`
/// — un essai précédent, fondé sur l'hypothèse inverse, produisait des
/// raccourcis qui ne se déclenchaient jamais. Le bloc clavier de
/// [`grid_contents`](crate::app::grid::grid_contents) lit les `Event::Copy`/`Event::Cut`/`Event::Paste`
/// correspondants, et c'est là que vit la vraie logique.
pub(super) const SHORTCUT_COPY: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
/// Ctrl+Maj+C — copier en entier les colonnes touchées (décoratif, voir
/// [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_COPY_FULL_COLUMNS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::C,
);
/// Ctrl+A — tout sélectionner dans la grille et dans Edit (#159), même effet
/// que la case du coin.
pub(super) const SHORTCUT_SELECT_ALL: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::A);
/// Ctrl+X — couper (décoratif, voir [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_CUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::X);
/// Ctrl+V — coller à partir de l'ancre (décoratif, voir [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_PASTE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::V);
/// Ctrl+Maj+V — coller à l'identique (décoratif, voir [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_PASTE_ABSOLUTE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::V,
);

/// Rappel de raccourci pour Retour arrière, écrit en toutes lettres plutôt
/// que via `Context::format_shortcut`.
///
/// egui nomme ses touches en anglais (« Backspace », « Delete ») : les
/// passer par `format_shortcut` mettrait deux mots anglais au milieu d'un
/// menu entièrement français. Les raccourcis à modificateur, eux, se rendent
/// identiquement dans toutes les langues (« Ctrl+C ») et gagnent à passer
/// par `format_shortcut`, qui sait les adapter à macOS.
pub(super) fn shortcut_text_backspace() -> String {
    tr!("shortcut-backspace")
}

/// Pendant de [`shortcut_text_backspace`] pour Suppr.
pub(super) fn shortcut_text_delete() -> String {
    tr!("shortcut-delete")
}

/// Où un raccourci s'applique : les rubriques de Help › Keyboard Shortcuts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HelpArea {
    General,
    Workbook,
    Graph,
    Managers,
    Fields,
}

impl HelpArea {
    pub(super) const ALL: [HelpArea; 5] = [
        HelpArea::General,
        HelpArea::Workbook,
        HelpArea::Graph,
        HelpArea::Managers,
        HelpArea::Fields,
    ];

    pub(super) fn title(self) -> String {
        match self {
            HelpArea::General => tr!("help-area-general"),
            HelpArea::Workbook => tr!("help-area-workbook"),
            HelpArea::Graph => tr!("help-area-graph"),
            HelpArea::Managers => tr!("help-area-managers"),
            HelpArea::Fields => tr!("help-area-fields"),
        }
    }
}

/// Ce qui s'affiche à droite d'une ligne de l'aide.
pub(super) enum HelpKeys {
    /// Un raccourci à modificateur, écrit par `Context::format_shortcut`.
    Shortcut(&'static egui::KeyboardShortcut),
    /// Deux raccourcis équivalents.
    Either(
        &'static egui::KeyboardShortcut,
        &'static egui::KeyboardShortcut,
    ),
    /// Une touche ou un geste qui n'est pas un raccourci à modificateur
    /// (Suppr, la molette) : texte traduit.
    Text(fn() -> String),
}

/// Une ligne de Help › Keyboard Shortcuts.
pub(super) struct HelpEntry {
    pub(super) area: HelpArea,
    pub(super) label: fn() -> String,
    pub(super) keys: HelpKeys,
}

impl HelpEntry {
    /// Les touches de la ligne, telles qu'on les affiche.
    pub(super) fn keys_text(&self, ctx: &egui::Context) -> String {
        match &self.keys {
            HelpKeys::Shortcut(shortcut) => ctx.format_shortcut(shortcut),
            HelpKeys::Either(a, b) => tr!(
                "help-keys-either",
                first = ctx.format_shortcut(a),
                second = ctx.format_shortcut(b)
            ),
            HelpKeys::Text(text) => text(),
        }
    }
}

macro_rules! entry {
    ($area:ident, $label:literal, shortcut $shortcut:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Shortcut(&$shortcut),
        }
    };
    ($area:ident, $label:literal, either $a:expr, $b:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Either(&$a, &$b),
        }
    };
    ($area:ident, $label:literal, text $text:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Text($text),
        }
    };
}

/// Les raccourcis que l'aide montre, dans l'ordre d'affichage de chaque
/// rubrique. Un raccourci neuf du registre s'ajoute ici, sinon l'utilisateur
/// ne le découvre pas (K5).
pub(super) const HELP: &[HelpEntry] = &[
    entry!(General, "help-undo", shortcut SHORTCUT_UNDO),
    entry!(General, "help-redo", either SHORTCUT_REDO_Y, SHORTCUT_REDO_Z),
    entry!(General, "help-new-project", shortcut SHORTCUT_NEW_PROJECT),
    entry!(General, "help-open", shortcut SHORTCUT_OPEN),
    entry!(General, "help-save", shortcut SHORTCUT_SAVE),
    entry!(General, "help-save-as", shortcut SHORTCUT_SAVE_AS),
    entry!(
        General,
        "help-import",
        text || SHORTCUT_IMPORT_TEXT.to_string()
    ),
    entry!(General, "help-export-graph", shortcut SHORTCUT_EXPORT_GRAPH),
    entry!(General, "help-run-script", shortcut SHORTCUT_RUN_SCRIPT),
    entry!(Workbook, "help-edit-cell", shortcut SHORTCUT_EDIT_CELL),
    entry!(Workbook, "help-select-all", shortcut SHORTCUT_SELECT_ALL),
    entry!(Workbook, "help-copy", shortcut SHORTCUT_COPY),
    entry!(Workbook, "help-copy-full-columns", shortcut SHORTCUT_COPY_FULL_COLUMNS),
    entry!(Workbook, "help-cut", shortcut SHORTCUT_CUT),
    entry!(Workbook, "help-paste", shortcut SHORTCUT_PASTE),
    entry!(Workbook, "help-paste-absolute", shortcut SHORTCUT_PASTE_ABSOLUTE),
    entry!(Workbook, "help-clear-cells", text shortcut_text_delete),
    entry!(Workbook, "help-duplicate-sheet", shortcut SHORTCUT_DUPLICATE_SHEET),
    entry!(Graph, "help-copy-graph", shortcut SHORTCUT_COPY),
    entry!(Graph, "help-delete-object", text shortcut_text_delete),
    entry!(Graph, "help-move-series-up", shortcut SHORTCUT_MOVE_SERIES_UP),
    entry!(Graph, "help-move-series-down", shortcut SHORTCUT_MOVE_SERIES_DOWN),
    entry!(Graph, "help-zoom", text || tr!("help-keys-ctrl-wheel")),
    entry!(Managers, "help-rename", shortcut SHORTCUT_RENAME_OBJECT),
    entry!(Managers, "help-delete-selection", text shortcut_text_delete),
    entry!(Managers, "help-duplicate", shortcut SHORTCUT_DUPLICATE_OBJECT),
    entry!(Fields, "help-validate", text || tr!("help-keys-enter")),
    entry!(Fields, "help-cancel", text || tr!("help-keys-escape")),
    entry!(Fields, "help-next-field", text || tr!("help-keys-tab")),
    entry!(Fields, "help-step-value", text || tr!("help-keys-arrows")),
    entry!(
        Fields,
        "help-step-value-ten",
        text || tr!("help-keys-shift-arrows")
    ),
    entry!(Fields, "help-open-list", shortcut KEY_ALT_ARROW_DOWN),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Ce que [`collect_undo_redo_shortcuts`] tire d'une image où `key` est
    /// pressée avec `modifiers`, puis ce qu'il en reste pour un second
    /// lecteur dans la même image.
    fn actions_pour(key: egui::Key, modifiers: egui::Modifiers) -> (Vec<Action>, Vec<Action>) {
        let context = egui::Context::default();
        let mut premieres = Vec::new();
        let mut restes = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| {
                collect_undo_redo_shortcuts(ui.ctx(), &mut premieres);
                collect_undo_redo_shortcuts(ui.ctx(), &mut restes);
            },
        );
        output.textures_delta.clear();
        (premieres, restes)
    }

    /// Ctrl+Z annule ; Ctrl+Y et Ctrl+Maj+Z rétablissent. Ctrl+Maj+Z
    /// contient Ctrl+Z : il doit rétablir **sans** annuler (le rétablissement
    /// se teste d'abord). Le raccourci est consommé : un second lecteur de la
    /// même image n'en voit rien, et Z sans Ctrl n'est pas un raccourci.
    #[test]
    fn annuler_et_retablir_se_lisent_au_clavier_une_seule_fois() {
        let ctrl = egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL);
        let ctrl_maj = ctrl.plus(egui::Modifiers::SHIFT);
        for (key, modifiers, attendu) in [
            (egui::Key::Z, ctrl, vec![Action::Undo]),
            (egui::Key::Y, ctrl, vec![Action::Redo]),
            (egui::Key::Z, ctrl_maj, vec![Action::Redo]),
            (egui::Key::Z, egui::Modifiers::NONE, vec![]),
            (egui::Key::A, ctrl, vec![]),
        ] {
            let (premieres, restes) = actions_pour(key, modifiers);
            assert_eq!(premieres, attendu, "{key:?} {modifiers:?}");
            assert!(restes.is_empty(), "{key:?} {modifiers:?} lu deux fois");
        }
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;

    /// Chaque rubrique de l'aide a des lignes, chaque ligne un texte de
    /// touches non vide, dans les deux langues.
    #[test]
    fn l_aide_des_raccourcis_est_complete() {
        let ctx = egui::Context::default();
        for area in HelpArea::ALL {
            assert!(
                HELP.iter().any(|entry| entry.area == area),
                "rubrique vide : {area:?}"
            );
        }
        for entry in HELP {
            assert!(!(entry.label)().is_empty());
            assert!(!entry.keys_text(&ctx).is_empty());
        }
    }

    /// L'aide est générée du registre : chaque constante `SHORTCUT_*`
    /// consommée y figure. Un raccourci oublié ferait échouer ce test.
    #[test]
    fn chaque_raccourci_consomme_figure_dans_l_aide() {
        let ctx = egui::Context::default();
        let shown: Vec<String> = HELP.iter().map(|entry| entry.keys_text(&ctx)).collect();
        for shortcut in [
            &SHORTCUT_UNDO,
            &SHORTCUT_REDO_Y,
            &SHORTCUT_SAVE,
            &SHORTCUT_SAVE_AS,
            &SHORTCUT_OPEN,
            &SHORTCUT_NEW_PROJECT,
            &SHORTCUT_EXPORT_GRAPH,
            &SHORTCUT_RUN_SCRIPT,
            &SHORTCUT_RENAME_OBJECT,
            &SHORTCUT_EDIT_CELL,
            &SHORTCUT_MOVE_SERIES_UP,
            &SHORTCUT_MOVE_SERIES_DOWN,
            &SHORTCUT_DUPLICATE_SHEET,
            &SHORTCUT_DUPLICATE_OBJECT,
            &SHORTCUT_SELECT_ALL,
        ] {
            let text = ctx.format_shortcut(shortcut);
            assert!(
                shown.iter().any(|keys| keys.contains(&text)),
                "{text} absent de l'aide"
            );
        }
    }
}
