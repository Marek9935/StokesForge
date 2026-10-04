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
//!   `use …::*` hors du code de test (décision 2 de #115).
//!
//! Chaque garde a son **témoin** : le motif interdit, posé dans un autre
//! fichier qu'`app.rs`, est bien refusé — sans lui, une garde qui ne voit rien
//! passerait pour verte. Les témoins du lecteur de sources lui-même sont en fin
//! de fichier.

#[path = "support/sources.rs"]
mod sources;

use sources::{commence_par, fonction_unique, ligne_de, mot_en, Fichier};

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

/// Les imports globaux encore tolérés, `(fichier, arbre d'import)` : les
/// `window_*.rs` de #101, sous-modules d'`app` qui empruntent ses types privés
/// par `use super::*`, et leurs réexportations dans `app.rs`. #117 les
/// convertit en vrais modules et **vide cette liste** ; une dérogation qui ne
/// correspond plus à aucun import fait échouer la garde.
const DEROGATIONS_117: &[(&str, &str)] = &[
    ("app.rs", "window_chrome::*"),
    ("app.rs", "window_detach::*"),
    ("app.rs", "window_placement::*"),
    ("app.rs", "window_stack::*"),
    ("window_chrome.rs", "super::*"),
    ("window_detach.rs", "super::*"),
    ("window_placement.rs", "super::*"),
    ("window_stack.rs", "super::*"),
];

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

/// Les écarts : imports globaux sans dérogation, puis dérogations périmées.
fn ecarts_d_imports(sources: &[Fichier], derogations: &[(&str, &str)]) -> Vec<String> {
    let trouves = imports_globaux(sources);
    // Une dérogation excuse **un** import : un second `use super::*` dans le
    // même fichier est un écart.
    let mut excuses = vec![false; derogations.len()];
    let mut ecarts: Vec<String> = trouves
        .iter()
        .filter(|t| {
            let libre =
                derogations
                    .iter()
                    .zip(excuses.iter_mut())
                    .find(|((chemin, arbre), pris)| {
                        !**pris && *chemin == t.chemin && *arbre == t.arbre
                    });
            match libre {
                Some((_, pris)) => {
                    *pris = true;
                    false
                }
                None => true,
            }
        })
        .map(|t| {
            format!(
                "src/{}:{} : `use {};` — import global hors des tests : nommer ce \
                 qu'on importe (décision 2 de #115)",
                t.chemin, t.ligne, t.arbre
            )
        })
        .collect();
    for ((chemin, arbre), pris) in derogations.iter().zip(excuses) {
        if !pris {
            ecarts.push(format!(
                "dérogation périmée ({chemin}, `use {arbre};`) : plus aucun import ne lui \
                 correspond, la retirer de DEROGATIONS_117"
            ));
        }
    }
    ecarts
}

/// Décision 2 de #115 : des vrais modules, à imports explicites. Aucun
/// `use …::*` — `use super::*` en tête — hors du code `#[cfg(test)]`, dans
/// tout `src/` (pas seulement `src/app/`) : un `use super::*` en production
/// fait d'un module un simple découpage de fichier, qui voit tout de son
/// parent sans rien déclarer.
#[test]
fn aucun_import_global_hors_des_tests() {
    let ecarts = ecarts_d_imports(&sources::sources(), DEROGATIONS_117);
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

/// Une dérogation excuse son import, et seulement lui ; une dérogation sans
/// import est périmée et fait échouer la garde (#117 doit vider la liste).
#[test]
fn une_derogation_excuse_son_import_et_perime_sans_lui() {
    let sources = [
        fichier(
            "window_chrome.rs",
            "use super::*;\nfn f() {}\nuse super::*;",
        ),
        fichier("app/grille.rs", "use super::*;"),
    ];
    let derogations = [
        ("window_chrome.rs", "super::*"),
        ("window_stack.rs", "super::*"),
    ];
    let ecarts = ecarts_d_imports(&sources, &derogations);
    assert_eq!(ecarts.len(), 3, "{ecarts:?}");
    assert!(
        ecarts[0].starts_with("src/window_chrome.rs:3"),
        "une dérogation n'excuse qu'un import : {ecarts:?}"
    );
    assert!(ecarts[1].starts_with("src/app/grille.rs:1"), "{ecarts:?}");
    assert!(
        ecarts[2].contains("périmée") && ecarts[2].contains("window_stack.rs"),
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
    for attendu in ["app.rs", "lib.rs", "i18n/pseudo.rs", "window_chrome.rs"] {
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
