//! Garde-fous d'architecture de `stokesforge-app` qui lisent le code (#116,
//! chantier #115).
//!
//! Aucun ne nomme un fichier : tous lisent la même liste, récursive, de
//! `src/` (`support/sources.rs`), et cherchent une fonction **par son nom**, où
//! qu'elle vive. Déplacer du code d'`app.rs` vers `src/app/` ne fait sortir
//! aucune règle de sa garde ; une fonction attendue qu'on ne trouve plus fait
//! échouer la garde au lieu de la laisser passer en silence.
//!
//! - `aucune_fenetre_n_est_une_egui_window_brute` : toute fenêtre passe par
//!   la brique commune (`show_window_in_layer`, `modal_dialog_with`) ;
//! - `la_fenetre_des_proprietes_est_construite_comme_les_autres` (#13) ;
//! - `aucun_import_global_hors_des_tests` : imports explicites, aucun
//!   `use …::*` hors du code de test (décision 2 de #115), sans dérogation ;
//! - `aucun_separateur_brut_dans_un_menu` (#155) : un trait de menu ne se pose
//!   que par la brique de sections (`menu_sections.rs`), sans dérogation ;
//! - `aucun_bouton_selectionnable_hors_d_un_menu` (#169) : le fond de bouton
//!   sélectionné d'egui est réservé aux choix exclusifs d'un menu, d'une liste
//!   déroulante ou du sélecteur de couleur ; une liste passe par `list_row`,
//!   un jeu d'onglets par `tab_strip` ;
//! - `aucune_couleur_en_dur_hors_du_theme` (#169) : une couleur d'interface
//!   vient de `theme.rs` (ou des `Visuals` qu'il pose), jamais d'un
//!   `Color32::from_rgb` écrit dans une surface ;
//! - `aucun_separateur_avant_un_titre_de_section` (#169) : un titre de
//!   section (`theme::section_title`) sépare seul ses groupes.
//!
//! Chaque garde a son **témoin** : le motif interdit, posé dans un autre
//! fichier qu'`app.rs`, est bien refusé — sans lui, une garde qui ne voit rien
//! passerait pour verte. Les témoins du lecteur de sources lui-même sont en fin
//! de fichier.

#[path = "support/sources.rs"]
mod sources;

use sources::{
    commence_par, corps, est_ident, fermante, fonction_unique, fonctions, ident_en, ligne_de,
    mot_en, nom_court, Fichier,
};

fn fichier(chemin: &str, texte: &str) -> Fichier {
    Fichier {
        chemin: chemin.to_string(),
        texte: texte.to_string(),
        de_test: false,
    }
}

// ---------------------------------------------------------------------------
// Aucune `egui::Window` brute
// ---------------------------------------------------------------------------

/// Les constructions `Window::new(` du code de production, `chemin:ligne`.
/// Toute forme est vue : `egui::Window::new(`, et `Window::new(` après un
/// `use egui::Window`.
fn fenetres_brutes(sources: &[Fichier]) -> Vec<String> {
    let mut trouvees = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        for i in 0..source.masque.len() {
            if mot_en(&source.masque, i, "Window")
                && commence_par(&source.masque, i + "Window".len(), "::new(")
            {
                trouvees.push(format!(
                    "src/{}:{}",
                    fichier.chemin,
                    ligne_de(&source.original, i)
                ));
            }
        }
    }
    trouvees
}

/// Règle du projet : **toute** fenêtre de l'application passe par la brique
/// commune (`show_window_in_layer`, via `modal_dialog_with` pour un dialogue),
/// jamais par une `egui::Window` brute. L'assistant d'import a déjà dérogé
/// à cette règle une fois : ce garde-fou l'empêche de se reproduire.
#[test]
fn aucune_fenetre_n_est_une_egui_window_brute() {
    let sources = sources::sources();
    let fautives = fenetres_brutes(&sources);
    assert!(
        fautives.is_empty(),
        "une fenêtre est construite par `egui::Window` brute : passer par \
         `show_window_in_layer` / `modal_dialog_with` (voir `stokesforge-feature`) : {fautives:?}"
    );
    let import = fonction_unique(&sources, "import_window");
    assert!(
        import.corps.contains("modal_dialog_with("),
        "l'assistant d'import ({}:{}) est un dialogue modal",
        import.chemin,
        import.ligne
    );
}

/// Le témoin : une fenêtre brute écrite hors d'`app.rs`, sous chacune de ses
/// formes, est refusée ; ce qui n'en construit pas passe.
#[test]
fn la_garde_des_fenetres_voit_une_window_brute_hors_d_app_rs() {
    for code in [
        "fn f(ctx: &Context) { egui::Window::new(titre).show(ctx, |_| {}); }",
        "use egui::Window;\nfn f(ctx: &Context) { Window::new(titre).show(ctx, |_| {}); }",
        "fn f() { eframe::egui::containers::Window::new(t); }",
        // Du code de production placé après un module de test reste lu.
        "#[cfg(test)]\nmod tests {}\nfn f() { egui::Window::new(t); }",
    ] {
        let trouvees = fenetres_brutes(&[fichier("app/import.rs", code)]);
        assert_eq!(trouvees.len(), 1, "non vue : {code}");
        assert!(
            trouvees[0].starts_with("src/app/import.rs:"),
            "{trouvees:?}"
        );
    }
    for code in [
        "// egui::Window::new(t)\nfn f() {}",
        "fn f() { let _ = \"egui::Window::new(\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { egui::Window::new(t); } }",
        "fn f(n: &Node) { if let ProjectTreeNode::Window { id, .. } = n {} }",
        "fn f() { MyWindow::new(t); }",
    ] {
        assert!(
            fenetres_brutes(&[fichier("app/import.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
}

/// #13 : la fenêtre des propriétés n'écrit pas son propre cadre — elle passe
/// par `modal_dialog`, qui délègue à `modal_dialog_with`, qui pose la fenêtre
/// par la brique des fenêtres de contenu (`show_window_in_layer`,
/// `window_header`). Chaque fonction est cherchée par son nom, où qu'elle vive.
#[test]
fn la_fenetre_des_proprietes_est_construite_comme_les_autres() {
    let sources = sources::sources();
    let proprietes = fonction_unique(&sources, "plot_settings_window");
    assert!(
        !proprietes.corps.contains("Window::new("),
        "plus de Window brute"
    );
    assert!(proprietes.corps.contains("modal_dialog("));
    let dialogue = fonction_unique(&sources, "modal_dialog");
    assert!(dialogue.corps.contains("modal_dialog_with("));
    let brique = fonction_unique(&sources, "modal_dialog_with");
    assert!(brique.corps.contains("show_window_in_layer("));
    assert!(brique.corps.contains("window_header("));
}

/// Le témoin de la recherche par nom : une fonction déplacée dans
/// `src/app/…` est trouvée ; une copie de test ne compte pas ; une fonction
/// devenue méthode reste trouvée ; une signature à tableau (`[u8; 4]`) n'est
/// pas prise pour une déclaration sans corps.
#[test]
fn une_fonction_est_trouvee_hors_d_app_rs_par_son_nom() {
    let sources = [
        fichier("app.rs", "impl App { fn autre(&self) { modal_dialog_with(); } }"),
        fichier(
            "app/dialogue.rs",
            "pub(crate) fn modal_dialog(ctx: &Context) {\n    modal_dialog_with(ctx); // window_header(\n}\n\
             #[cfg(test)]\nmod tests { fn modal_dialog() {} }",
        ),
    ];
    let trouvee = fonction_unique(&sources, "modal_dialog");
    assert_eq!(trouvee.chemin, "app/dialogue.rs");
    assert_eq!(trouvee.ligne, 1);
    assert!(trouvee.corps.contains("modal_dialog_with("));
    assert!(
        !trouvee.corps.contains("window_header("),
        "un commentaire n'est pas un appel"
    );
    let methode = [fichier(
        "app/fenetres.rs",
        "impl StokesForgeApp {\n    fn import_window(&mut self, cles: [u8; 4]) -> [u8; 2] {\n        modal_dialog_with();\n    }\n}",
    )];
    let trouvee = fonction_unique(&methode, "import_window");
    assert_eq!(trouvee.qualifie, "StokesForgeApp::import_window");
    assert!(trouvee.corps.contains("modal_dialog_with("));
}

/// Une fonction introuvable fait échouer la garde : elle ne sort pas de la
/// règle en silence.
#[test]
#[should_panic(expected = "trouvée 0 fois")]
fn une_fonction_introuvable_fait_echouer_la_garde() {
    fonction_unique(
        &[fichier("app/grille.rs", "fn autre() {}")],
        "import_window",
    );
}

/// Deux fonctions libres du même nom (deux modules) : la garde ne choisit pas.
#[test]
#[should_panic(expected = "trouvée 2 fois")]
fn une_fonction_en_double_fait_echouer_la_garde() {
    fonction_unique(
        &[
            fichier("app/a.rs", "fn import_window() {}"),
            fichier("app/b.rs", "fn import_window() {}"),
        ],
        "import_window",
    );
}

// ---------------------------------------------------------------------------
// Aucun import global hors des tests
// ---------------------------------------------------------------------------

/// Un import global : son fichier, sa ligne, son arbre d'import sans blancs
/// (`super::*`, `super::{a,b::*}`).
#[derive(Debug, PartialEq, Eq)]
struct ImportGlobal {
    chemin: String,
    ligne: usize,
    arbre: String,
}

/// Les `use …*` du code de production (`pub use` compris).
fn imports_globaux(sources: &[Fichier]) -> Vec<ImportGlobal> {
    let mut trouves = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        let masque = &source.masque;
        for i in 0..masque.len() {
            if !mot_en(masque, i, "use") {
                continue;
            }
            let fin = (i..masque.len())
                .find(|k| masque[*k] == ';')
                .unwrap_or(masque.len());
            let arbre: String = masque[i + 3..fin]
                .iter()
                .filter(|c| !c.is_whitespace())
                .collect();
            if arbre.contains('*') {
                trouves.push(ImportGlobal {
                    chemin: fichier.chemin.clone(),
                    ligne: ligne_de(&source.original, i),
                    arbre,
                });
            }
        }
    }
    trouves
}

/// Les écarts, un par import global ; vide si tout est en règle.
fn ecarts_d_imports(sources: &[Fichier]) -> Vec<String> {
    imports_globaux(sources)
        .iter()
        .map(|t| {
            format!(
                "src/{}:{} : `use {};` — import global hors des tests : nommer ce \
                 qu'on importe (décision 2 de #115)",
                t.chemin, t.ligne, t.arbre
            )
        })
        .collect()
}

/// Décision 2 de #115 : des vrais modules, à imports explicites. Aucun
/// `use …::*` — `use super::*` en tête — hors du code `#[cfg(test)]`, dans
/// tout `src/` (pas seulement `src/app/`) : un `use super::*` en production
/// fait d'un module un simple découpage de fichier, qui voit tout de son
/// parent sans rien déclarer. Garde absolue : les dérogations des
/// `window_*.rs` de #101 ont été levées par #117.
#[test]
fn aucun_import_global_hors_des_tests() {
    let ecarts = ecarts_d_imports(&sources::sources());
    assert!(
        ecarts.is_empty(),
        "{} écart(s) :\n  {}",
        ecarts.len(),
        ecarts.join("\n  ")
    );
}

/// Le témoin : chaque forme d'import global, posée dans un module de
/// `src/app/`, est refusée.
#[test]
fn la_garde_des_imports_voit_chaque_forme_d_import_global() {
    for (code, arbre, ligne) in [
        ("use super::*;", "super::*", 1),
        ("pub(crate) use super::*;", "super::*", 1),
        ("use super::grille::*;", "super::grille::*", 1),
        ("use super::{Tab, grille::*};", "super::{Tab,grille::*}", 1),
        ("use crate::state::*;", "crate::state::*", 1),
        ("use egui::*;", "egui::*", 1),
        ("fn f() {\n    use LineStyle::*;\n}", "LineStyle::*", 2),
        // Le code placé après un module de test reste du code de production
        // (`errors.rs` alterne les deux).
        (
            "mod a {\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n    }\n}\nuse super::*;",
            "super::*",
            7,
        ),
    ] {
        let trouves = imports_globaux(&[fichier("app/import.rs", code)]);
        assert_eq!(trouves.len(), 1, "non vu : {code}");
        assert_eq!(trouves[0].chemin, "app/import.rs");
        assert_eq!(trouves[0].arbre, arbre);
        assert_eq!(trouves[0].ligne, ligne, "{code}");
    }
}

#[test]
fn la_garde_des_imports_laisse_passer_le_code_de_test_et_les_imports_nommes() {
    for code in [
        "use super::{Tab, WindowKind};",
        "use std::ops::Mul;",
        "// use super::*;",
        "fn f() { let _ = \"use super::*;\"; }",
        "#[cfg(test)]\nmod tests {\n    use super::*;\n}",
        "#[cfg(test)]\nuse super::*;",
    ] {
        assert!(
            imports_globaux(&[fichier("app/import.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
    let mut de_test = fichier("app/import/tests.rs", "use super::*;");
    de_test.de_test = true;
    assert!(imports_globaux(&[de_test]).is_empty());
}

/// Le rapport nomme chaque import, un écart par import — deux dans le même
/// fichier font deux écarts.
#[test]
fn la_garde_des_imports_rapporte_chaque_import() {
    let sources = [
        fichier(
            "app/window_chrome.rs",
            "use super::*;\nfn f() {}\nuse super::*;",
        ),
        fichier("app/grille.rs", "use crate::state::{Action, *};"),
    ];
    let ecarts = ecarts_d_imports(&sources);
    assert_eq!(ecarts.len(), 3, "{ecarts:?}");
    assert!(
        ecarts[0].starts_with("src/app/window_chrome.rs:1"),
        "{ecarts:?}"
    );
    assert!(
        ecarts[1].starts_with("src/app/window_chrome.rs:3"),
        "{ecarts:?}"
    );
    assert!(
        ecarts[2].starts_with("src/app/grille.rs:1")
            && ecarts[2].contains("crate::state::{Action,*}"),
        "{ecarts:?}"
    );
}

// ---------------------------------------------------------------------------
// Le lecteur de sources
// ---------------------------------------------------------------------------

fn ecrire(racine: &std::path::Path, chemin: &str, texte: &str) {
    let chemin = racine.join(chemin);
    std::fs::create_dir_all(chemin.parent().expect("dossier")).expect("dossier");
    std::fs::write(chemin, texte).expect("écriture");
}

/// La lecture descend dans les sous-dossiers, ne lit que les `.rs`, et rend
/// des chemins relatifs triés.
#[test]
fn la_lecture_descend_dans_les_sous_dossiers() {
    let dossier = tempfile::tempdir().expect("dossier temporaire");
    ecrire(dossier.path(), "lib.rs", "mod app;");
    ecrire(dossier.path(), "app.rs", "mod grille;");
    ecrire(dossier.path(), "app/grille.rs", "");
    ecrire(dossier.path(), "app/grille/cellule.rs", "");
    ecrire(dossier.path(), "app/notes.md", "");
    let chemins: Vec<String> = sources::lire(dossier.path())
        .into_iter()
        .map(|f| f.chemin)
        .collect();
    assert_eq!(
        chemins,
        ["app.rs", "app/grille.rs", "app/grille/cellule.rs", "lib.rs"]
    );
}

/// Sur le vrai dépôt : les sous-dossiers de `src/` sont lus (`i18n/pseudo.rs`
/// échappait à `aucun_libelle_en_dur` avant #116), et aucun fichier de
/// production n'est pris pour du test.
#[test]
fn la_vraie_lecture_voit_les_sous_dossiers_de_src() {
    let sources = sources::sources();
    for attendu in ["app.rs", "lib.rs", "i18n/pseudo.rs", "app/window_chrome.rs"] {
        let fichier = sources
            .iter()
            .find(|f| f.chemin == attendu)
            .unwrap_or_else(|| panic!("{attendu} non lu"));
        assert!(!fichier.de_test, "{attendu} pris pour du test");
    }
}

/// Un module de test écrit dans son propre fichier (`#[cfg(test)] mod x;`,
/// avec ou sans `#[path]`, attribut avant ou après) rend ce fichier entier de
/// test, comme les modules qu'il déclare à son tour ; les autres restent de
/// production.
#[test]
fn un_module_de_test_dans_son_propre_fichier_est_du_test() {
    let dossier = tempfile::tempdir().expect("dossier temporaire");
    ecrire(
        dossier.path(),
        "lib.rs",
        "mod app;\n#[cfg(test)]\nmod tests;\n",
    );
    ecrire(
        dossier.path(),
        "app.rs",
        "#[path = \"app_tests.rs\"]\n#[cfg(test)]\nmod t;\n#[cfg(test)] pub(crate) mod aide;\nmod grille;\n",
    );
    ecrire(dossier.path(), "app/grille.rs", "use super::*;");
    ecrire(dossier.path(), "app/aide.rs", "use super::*;");
    ecrire(dossier.path(), "app_tests.rs", "use super::*;");
    ecrire(dossier.path(), "tests.rs", "mod outils;\n");
    ecrire(dossier.path(), "tests/outils.rs", "use super::*;");
    let sources = sources::lire(dossier.path());
    let de_test: Vec<&str> = sources
        .iter()
        .filter(|f| f.de_test)
        .map(|f| f.chemin.as_str())
        .collect();
    assert_eq!(
        de_test,
        ["app/aide.rs", "app_tests.rs", "tests.rs", "tests/outils.rs"]
    );
    // Et la garde des imports n'y voit que le module de production.
    let trouves = imports_globaux(&sources);
    assert_eq!(trouves.len(), 1, "{trouves:?}");
    assert_eq!(trouves[0].chemin, "app/grille.rs");
}

/// La vue de production blanchit **chaque** item `#[cfg(test)]` — module,
/// fonction, `use` — et garde le code qui les suit ainsi que les numéros de
/// ligne.
#[test]
fn la_production_blanchit_chaque_item_de_test_et_garde_le_reste() {
    let texte = "fn avant() {}\n\
                 #[cfg(test)]\n\
                 mod tests {\n    fn TEST_UN() {}\n}\n\
                 fn milieu() {}\n\
                 #[cfg(test)]\n#[test]\nfn TEST_DEUX() { let _ = [0u8; 2]; }\n\
                 #[cfg(test)] use TEST_TROIS;\n\
                 fn apres() {}\n";
    let production = fichier("app/a.rs", texte).production();
    let code = production.code_en_texte();
    for garde in ["fn avant", "fn milieu", "fn apres"] {
        assert!(code.contains(garde), "{garde} perdu :\n{code}");
    }
    assert!(!code.contains("TEST_"), "du test reste :\n{code}");
    assert_eq!(code.lines().count(), texte.lines().count());
    let ligne_apres = code.lines().position(|l| l.contains("fn apres"));
    assert_eq!(
        ligne_apres,
        texte.lines().position(|l| l.contains("fn apres"))
    );
}

/// Un `#[cfg(test)]` sur un champ, un bras de `match` ou un argument ne blanchit
/// que cet élément : le code de production qui le suit reste lu.
#[test]
fn un_cfg_test_hors_item_n_avale_pas_la_suite() {
    for texte in [
        "pub struct S {\n    a: u8,\n    #[cfg(test)]\n    pub TEST_CHAMP: u8,\n}\nfn PRODUCTION() {}\n",
        "pub struct S {\n    #[cfg(test)]\n    TEST_DERNIER: HashMap<u8, u8>\n}\nfn PRODUCTION() {}\n",
        "fn f(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n        0 => TEST_BRAS,\n        _ => { PRODUCTION }\n    }\n}\n",
        "fn f() -> S {\n    S {\n        #[cfg(test)]\n        hook: TEST_INIT,\n        a: PRODUCTION,\n    }\n}\n",
        "fn f() {\n    appel(\n        #[cfg(test)]\n        TEST_ARG,\n    );\n    PRODUCTION();\n}\n",
    ] {
        let code = fichier("app/a.rs", texte).production().code_en_texte();
        assert!(code.contains("PRODUCTION"), "production avalée :\n{code}");
        assert!(!code.contains("TEST_"), "du test reste :\n{code}");
    }
}

/// Un `#![cfg(test)]` en tête de fichier le rend entier de test, et les
/// modules qu'il déclare avec lui ; un `#[path]` qui sort de `src/` ne désigne
/// aucune source lue.
#[test]
fn un_attribut_interieur_et_un_chemin_hors_de_src_sont_compris() {
    let dossier = tempfile::tempdir().expect("dossier temporaire");
    ecrire(
        dossier.path(),
        "lib.rs",
        "mod outils;\nmod essais;\n#[cfg(test)]\n#[path = \"../outils.rs\"]\nmod h;\n",
    );
    ecrire(dossier.path(), "outils.rs", "use super::*;");
    ecrire(
        dossier.path(),
        "essais.rs",
        "//! Essais.\n#![allow(dead_code)]\n#![cfg(test)]\nuse super::*;\nmod aide;\n",
    );
    ecrire(dossier.path(), "essais/aide.rs", "use super::*;");
    let sources = sources::lire(dossier.path());
    let de_test: Vec<&str> = sources
        .iter()
        .filter(|f| f.de_test)
        .map(|f| f.chemin.as_str())
        .collect();
    assert_eq!(de_test, ["essais.rs", "essais/aide.rs"]);
    let trouves = imports_globaux(&sources);
    assert_eq!(trouves.len(), 1, "{trouves:?}");
    assert_eq!(trouves[0].chemin, "outils.rs");
}

// ---------------------------------------------------------------------------
// Aucun séparateur brut dans un menu (#155)
// ---------------------------------------------------------------------------

/// Les étendues de code qui peignent un menu : le corps de toute fonction dont
/// le nom contient « menu », et les arguments de `context_menu(`,
/// `menu_button(` et `SubMenuButton…::ui(`.
fn zones_de_menu(masque: &[char]) -> Vec<(usize, usize)> {
    let mut zones = Vec::new();
    for (position, nom) in fonctions(masque) {
        if nom_court(&nom).contains("menu") {
            zones.extend(corps(masque, position));
        }
    }
    for motif in ["context_menu(", "menu_button(", "SubMenuButton"] {
        for i in 0..masque.len() {
            if commence_par(masque, i, motif) {
                let ouvrante = (i..masque.len()).find(|k| masque[*k] == '(');
                if let Some((ouvrante, fermante)) =
                    ouvrante.and_then(|o| Some((o, fermante(masque, o)?)))
                {
                    zones.push((ouvrante, fermante));
                }
            }
        }
    }
    zones
}

/// Les `.separator()` posés à la main dans un menu, `chemin:ligne`. La brique
/// de sections (`section_separator`, `menu_sections.rs`) est la seule à en
/// poser : son nom ne contient pas « menu », et ses tests sont du code de test.
fn separateurs_bruts_dans_un_menu(sources: &[Fichier]) -> Vec<String> {
    let mut trouves = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        let zones = zones_de_menu(&source.masque);
        for i in 0..source.masque.len() {
            if commence_par(&source.masque, i, ".separator()")
                && zones.iter().any(|(debut, fin)| *debut < i && i < *fin)
            {
                trouves.push(format!(
                    "src/{}:{}",
                    fichier.chemin,
                    ligne_de(&source.original, i)
                ));
            }
        }
    }
    trouves
}

/// Règle de `docs/menus.md` : les traits d'un menu se posent **seulement** par
/// `MenuSections::section`, qui ne les met qu'entre deux sections présentes.
/// Un `ui.separator()` à la main laisse un trait orphelin dès qu'une section
/// disparaît. Aucune dérogation.
#[test]
fn aucun_separateur_brut_dans_un_menu() {
    let sources = sources::sources();
    let fautifs = separateurs_bruts_dans_un_menu(&sources);
    assert!(
        fautifs.is_empty(),
        "un menu pose un séparateur à la main : passer par `MenuSections::section` \
         (`app/menu_sections.rs`, `docs/menus.md`) : {fautifs:?}"
    );
    // La brique existe, et c'est elle qui trace le trait.
    let brique = fonction_unique(&sources, "section_separator");
    assert!(brique.corps.contains("Separator"));
}

/// Le témoin : un trait à la main est vu dans une fonction « menu », dans une
/// fermeture de clic droit et dans un sous-menu ; un trait hors de tout menu,
/// un commentaire et un texte ne comptent pas.
#[test]
fn la_garde_des_separateurs_voit_un_trait_a_la_main_dans_un_menu() {
    for code in [
        "fn fichier_menu(ui: &mut Ui) { ui.separator(); }",
        "impl App { fn edit_menu(&self, ui: &mut Ui) { ui.separator(); } }",
        "fn f(r: Response) { r.context_menu(|ui| { ui.button(a); ui.separator(); }); }",
        "fn f(ui: &mut Ui) { ui.menu_button(t, |ui| { ui.separator(); }); }",
    ] {
        assert_eq!(
            separateurs_bruts_dans_un_menu(&[fichier("app/m.rs", code)]).len(),
            1,
            "non vu : {code}"
        );
    }
    for code in [
        "fn barre(ui: &mut Ui) { ui.separator(); }",
        "fn section_separator(ui: &mut Ui) { ui.add(Separator::default()); }",
        "fn f_menu(ui: &mut Ui) { // ui.separator();\n let _ = \"ui.separator()\"; }",
        "fn f_menu() {}\n#[cfg(test)]\nmod tests { fn g_menu(ui: &mut Ui) { ui.separator(); } }",
    ] {
        assert!(
            separateurs_bruts_dans_un_menu(&[fichier("app/m.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// Fond de bouton sélectionné : réservé aux choix exclusifs (#169)
// ---------------------------------------------------------------------------

/// Les étendues où un bouton sélectionnable d'egui est à sa place : les menus
/// ([`zones_de_menu`]), le contenu d'une liste déroulante (arguments de
/// `.show_ui(`) et le corps d'une fonction de fenêtre surgissante (nom qui
/// contient « popup » ou « picker » : le sélecteur de couleur).
fn zones_de_choix(masque: &[char]) -> Vec<(usize, usize)> {
    let mut zones = zones_de_menu(masque);
    for (position, nom) in fonctions(masque) {
        let nom = nom_court(&nom);
        if nom.contains("popup") || nom.contains("picker") {
            zones.extend(corps(masque, position));
        }
    }
    for i in 0..masque.len() {
        if commence_par(masque, i, ".show_ui(") {
            let ouvrante = i + ".show_ui".len();
            if let Some(fin) = fermante(masque, ouvrante) {
                zones.push((ouvrante, fin));
            }
        }
    }
    zones
}

/// Les boutons sélectionnables (`selectable_label(`, `selectable_value(`,
/// `Button::selectable(`) posés hors d'un menu, d'une liste déroulante ou du
/// sélecteur de couleur, `chemin:ligne`.
fn boutons_selectionnables_hors_d_un_choix(sources: &[Fichier]) -> Vec<String> {
    let mut trouves = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        let zones = zones_de_choix(&source.masque);
        for i in 0..source.masque.len() {
            let vu = [
                ".selectable_label(",
                ".selectable_value(",
                "Button::selectable(",
            ]
            .iter()
            .any(|motif| commence_par(&source.masque, i, motif));
            if vu && !zones.iter().any(|(debut, fin)| *debut < i && i < *fin) {
                trouves.push(format!(
                    "src/{}:{}",
                    fichier.chemin,
                    ligne_de(&source.original, i)
                ));
            }
        }
    }
    trouves
}

/// Règle #169 (`docs/application.md`, § Onglets, listes et sélection) : le
/// fond vert arrondi d'un bouton sélectionné d'egui ne marque qu'un **choix
/// exclusif** dans un menu, une liste déroulante ou le sélecteur de couleur.
/// Une entrée de liste ou d'arbre est une ligne de liste (`list_row`,
/// `ListRowBackground`) ; un jeu d'onglets, la bande `tab_strip`. Sans
/// dérogation.
#[test]
fn aucun_bouton_selectionnable_hors_d_un_menu() {
    let sources = sources::sources();
    let fautifs = boutons_selectionnables_hors_d_un_choix(&sources);
    assert!(
        fautifs.is_empty(),
        "bouton sélectionnable hors d'un menu ou d'une liste déroulante : une liste \
         passe par `list_row` (`app/list_row.rs`), un jeu d'onglets par `tab_strip` : \
         {fautifs:?}"
    );
    // Les briques existent.
    assert!(fonction_unique(&sources, "list_row")
        .corps
        .contains("allocate_exact_size"));
    fonction_unique(&sources, "tab_strip_with");
}

/// Le témoin : une rangée de boutons sélectionnables (des onglets à la main)
/// et une liste de `selectable_label` sont vues ; un choix dans une liste
/// déroulante, un menu ou le sélecteur de couleur passe.
#[test]
fn la_garde_des_boutons_selectionnables_voit_une_rangee_a_la_main() {
    for code in [
        "fn page(ui: &mut Ui) { ui.horizontal(|ui| { for a in axes { ui.selectable_label(a == b, t); } }); }",
        "fn liste(ui: &mut Ui) { ui.selectable_value(&mut x, 1, t); }",
        "fn ligne(ui: &mut Ui) { ui.add(egui::Button::selectable(s, t)); }",
    ] {
        assert_eq!(
            boutons_selectionnables_hors_d_un_choix(&[fichier("app/p.rs", code)]).len(),
            1,
            "non vu : {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ComboBox::new(a, b).show_ui(ui, |ui| { ui.selectable_value(&mut x, 1, t); }); }",
        "fn type_menu(ui: &mut Ui) { ui.selectable_label(a, t); }",
        "fn f(r: Response) { r.context_menu(|ui| { ui.selectable_label(a, t); }); }",
        "fn picker_contents(ui: &mut Ui) { ui.selectable_value(&mut s, A, t); }",
        "fn f(ui: &mut Ui) { // ui.selectable_label(a, t)\n let _ = \".selectable_label(\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.selectable_label(a, t); } }",
    ] {
        assert!(
            boutons_selectionnables_hors_d_un_choix(&[fichier("app/p.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// Couleurs : seulement par `theme.rs` (#169)
// ---------------------------------------------------------------------------

/// Les constantes de `Color32` qui ne portent pas de choix de palette : le
/// blanc, le noir, le transparent et le témoin « pas de couleur » d'egui.
const COULEURS_NEUTRES: [&str; 4] = ["WHITE", "BLACK", "TRANSPARENT", "PLACEHOLDER"];

/// Les fichiers qui fabriquent des couleurs par métier : la palette
/// (`theme.rs`) et le sélecteur de couleur, qui peint les couleurs choisies
/// par l'utilisateur (nuancier, roue, damier de l'opacité).
const FABRIQUES_DE_COULEUR: [&str; 2] = ["theme.rs", "color_picker.rs"];

/// Les couleurs écrites en dur hors de la palette, `chemin:ligne` :
/// `Color32::from_…(` et toute constante nommée de `Color32` autre que
/// [`COULEURS_NEUTRES`].
fn couleurs_en_dur(sources: &[Fichier]) -> Vec<String> {
    let mut trouvees = Vec::new();
    for fichier in sources {
        if FABRIQUES_DE_COULEUR.contains(&fichier.chemin.as_str()) {
            continue;
        }
        let source = fichier.production();
        for i in 0..source.masque.len() {
            if !mot_en(&source.masque, i, "Color32")
                || !commence_par(&source.masque, i + "Color32".len(), "::")
            {
                continue;
            }
            let nom = ident_en(&source.masque, i + "Color32::".len());
            let constante = !nom.is_empty()
                && nom
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if nom.starts_with("from_") || (constante && !COULEURS_NEUTRES.contains(&nom.as_str()))
            {
                trouvees.push(format!(
                    "src/{}:{}",
                    fichier.chemin,
                    ligne_de(&source.original, i)
                ));
            }
        }
    }
    trouvees
}

/// Règle #169 : une couleur d'état ou de chrome (accent, survol, filet,
/// erreur, dépôt…) vient d'une fonction de `theme.rs` ou des `Visuals` que
/// `theme::install` pose (`error_fg_color`, `warn_fg_color`,
/// `selection`…), jamais d'une teinte écrite dans une surface : changer la
/// palette se fait alors à un seul endroit. Sans dérogation hors des deux
/// fabriques de [`FABRIQUES_DE_COULEUR`].
#[test]
fn aucune_couleur_en_dur_hors_du_theme() {
    let fautifs = couleurs_en_dur(&sources::sources());
    assert!(
        fautifs.is_empty(),
        "couleur écrite en dur hors de theme.rs : ajouter une fonction de palette \
         dans `theme.rs` ou lire les `Visuals` : {fautifs:?}"
    );
}

/// Le témoin : une teinte `from_rgb`, `from_gray` ou nommée est vue, dans
/// toutes ses formes ; les neutres, un commentaire, du code de test et les
/// deux fabriques passent.
#[test]
fn la_garde_des_couleurs_voit_une_teinte_en_dur() {
    for code in [
        "fn f() -> Color32 { egui::Color32::from_rgb(1, 2, 3) }",
        "fn f() -> Color32 { Color32::from_gray(0xEC) }",
        "fn f() -> Color32 { Color32::from_rgba_unmultiplied(1, 2, 3, 4) }",
        "fn f(ui: &Ui) { ui.colored_label(egui::Color32::RED, t); }",
        "fn f() -> Color32 { Color32::LIGHT_BLUE }",
    ] {
        assert_eq!(
            couleurs_en_dur(&[fichier("app/c.rs", code)]).len(),
            1,
            "non vu : {code}"
        );
    }
    for code in [
        "fn f() -> Color32 { Color32::WHITE }",
        "fn f(c: Color32) -> bool { c == egui::Color32::PLACEHOLDER }",
        "fn f() -> Color32 { theme::accent() } // Color32::from_rgb(1, 2, 3)",
        "fn f() -> MyColor32 { MyColor32::from_rgb(1, 2, 3) }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { Color32::from_rgb(1, 2, 3); } }",
    ] {
        assert!(
            couleurs_en_dur(&[fichier("app/c.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
    for chemin in FABRIQUES_DE_COULEUR {
        assert!(
            couleurs_en_dur(&[fichier(chemin, "fn f() { Color32::from_rgb(1, 2, 3); }")])
                .is_empty()
        );
    }
}

// ---------------------------------------------------------------------------
// Titres de section : sans séparateur redondant (#169)
// ---------------------------------------------------------------------------

/// Les `.separator()` suivis directement d'un titre de section
/// (`section_title(` ou un libellé en `theme::bold`), `chemin:ligne`.
fn separateurs_avant_un_titre(sources: &[Fichier]) -> Vec<String> {
    let mut trouves = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        let masque = &source.masque;
        for i in 0..masque.len() {
            if !commence_par(masque, i, ".separator();") {
                continue;
            }
            let mut k = i + ".separator();".len();
            while k < masque.len() && masque[k].is_whitespace() {
                k += 1;
            }
            // Le chemin éventuel (`crate::theme::`, `theme::`) avant le nom.
            let mut nom_debut = k;
            while nom_debut < masque.len()
                && (est_ident(masque[nom_debut]) || masque[nom_debut] == ':')
            {
                nom_debut += 1;
            }
            let appel: String = masque[k..nom_debut].iter().collect();
            let titre = nom_court(&appel) == "section_title"
                || (commence_par(masque, k, "ui.label(")
                    && commence_par(masque, k + "ui.label(".len(), "crate::theme::bold("))
                || (commence_par(masque, k, "ui.label(")
                    && commence_par(masque, k + "ui.label(".len(), "theme::bold("));
            if titre {
                trouves.push(format!(
                    "src/{}:{}",
                    fichier.chemin,
                    ligne_de(&source.original, i)
                ));
            }
        }
    }
    trouves
}

/// Règle #169 : un titre de section de formulaire passe par
/// `theme::section_title` (espace + gras) et sépare seul ses groupes ; un
/// filet juste avant lui double la séparation et rend la page plus lourde
/// que la maquette. Sans dérogation.
#[test]
fn aucun_separateur_avant_un_titre_de_section() {
    let sources = sources::sources();
    let fautifs = separateurs_avant_un_titre(&sources);
    assert!(
        fautifs.is_empty(),
        "séparateur juste avant un titre de section : le titre suffit \
         (`theme::section_title`) : {fautifs:?}"
    );
    assert!(fonction_unique(&sources, "section_title")
        .corps
        .contains("bold("));
}

/// Le témoin : un filet suivi d'un titre, sous toutes ses formes, est vu ;
/// un filet suivi d'autre chose ou un titre seul passent.
#[test]
fn la_garde_des_titres_voit_un_filet_avant_un_titre() {
    for code in [
        "fn f(ui: &mut Ui) { ui.separator();\n    section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); crate::theme::section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(crate::theme::bold(t)); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(theme::bold(t)); }",
    ] {
        assert_eq!(
            separateurs_avant_un_titre(&[fichier("app/s.rs", code)]).len(),
            1,
            "non vu : {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(t); }",
        "fn f(ui: &mut Ui) { ui.separator(); // section_title(ui, t)\n ui.checkbox(a, b); }",
    ] {
        assert!(
            separateurs_avant_un_titre(&[fichier("app/s.rs", code)]).is_empty(),
            "faux positif : {code}"
        );
    }
}
