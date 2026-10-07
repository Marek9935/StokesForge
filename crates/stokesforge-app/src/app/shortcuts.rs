//! Raccourcis clavier de l'application : les constantes `SHORTCUT_*` (celles
//! que l'on consomme et celles, décoratives, qui ne servent qu'au rappel
//! affiché dans un menu), leurs rappels écrits en toutes lettres, et la
//! consommation d'annuler/rétablir dans le viewport actif.
//!
//! Sorti d'`app.rs` tel quel (#117). La consommation globale des raccourcis,
//! `collect_shortcuts`, reste dans `app.rs` (#115).

use eframe::egui;

use crate::state::Action;
use crate::tr;

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
