//! Garde-fous de la traduction (issue #41, chantier #39).
//!
//! # `aucun_libelle_en_dur`
//!
//! Aucun littéral de chaîne dans les appels qui posent un texte à l'écran
//! (`label`, `button`, `menu_button`, `checkbox`, `radio_value`,
//! `selectable_value`, `selectable_label`, `on_hover_text`, `heading`…), ni dans
//! les widgets bâtis par `Button::new("…")` / `RichText::new("…")`, ni dans les
//! tables `fn label()` / `fn title()` : tout texte passe par `tr!` avec une clé
//! du catalogue `en`. Modèle : `aucune_fenetre_n_est_une_egui_window_brute`
//! (`src/app.rs`) — le test lit les sources et refuse. Toutes les sources de
//! `src/` sont lues : un fichier nouveau est gardé sans rien déclarer.
//!
//! Un littéral sans aucune lettre (`"→"`, `" · "`, `"{}"`) n'est pas un texte à
//! traduire et passe ; la clé d'un `tr!("…")` non plus.
//!
//! Les sources sont lues par `support/sources.rs` (#116) : tout `src/`,
//! sous-dossiers compris, et le code de test retiré item par item — un
//! fichier qui alterne code et modules de test (`errors.rs`) reste gardé
//! après son premier `#[cfg(test)]`.
//!
//! ## Une garde absolue (#48)
//!
//! Aucune dérogation : un littéral textuel dans un de ces appels fait échouer le
//! test, quel que soit le fichier. (La liste d'avant #41, 213 littéraux en 14
//! zones, a été réduite à zéro zone par zone, #42 à #45, puis supprimée.)
//!
//! ## Ce que la garde ne voit pas
//!
//! Un texte posé par un autre chemin : un `&str` bâti ailleurs puis passé en
//! variable, un titre de `modal_dialog_with` / `window_header` / `tab_strip`
//! (leurs arguments mêlent texte et identifiants), un nom de colonne ou
//! d'unité calculé. [`ZONE_43`] garde plus strictement les fonctions de la
//! fenêtre des propriétés du graphe ; la capture en pseudo-langue
//! (`STOKESFORGE_LANG=en-XA`) montre ce qui a échappé : un texte sans `[[ ]]`.

use std::path::Path;

#[path = "support/sources.rs"]
mod sources;

use sources::{
    commence_par, corps, est_ident, fermante, fonctions, fonctions_nommees, ident_en, ligne_de,
    nom_court, Fichier, Masque,
};

/// Les appels `.nom(…)` dont un argument ne doit pas être un littéral textuel.
const APPELS: &[&str] = &[
    "label",
    "button",
    "small_button",
    "menu_button",
    "checkbox",
    "radio",
    "radio_value",
    "selectable_value",
    "selectable_label",
    "toggle_value",
    "on_hover_text",
    "on_disabled_hover_text",
    "heading",
    "hint_text",
    "selected_text",
    "with_title",
    "strong",
    "weak",
    "small",
    "monospace",
    "colored_label",
    "on_hover_text_at_pointer",
];

/// Les widgets bâtis par `Type::new("texte")` : `egui::Button::new("Ouvrir…")`
/// est la forme de presque toute entrée de menu, et `RichText::new(…)` celle
/// d'un texte stylé.
const CONSTRUCTEURS: &[&str] = &[
    "Button",
    "Checkbox",
    "CollapsingHeader",
    "Label",
    "RichText",
];

/// Les fonctions dont le corps est une table de libellés : aucun littéral
/// textuel dedans. Les quatre dernières sont les tables de l'assistant d'import
/// (#44) ; `encoding_label` et `column_role_label` n'y sont pas : leurs noms
/// techniques (`UTF-8`, `Windows-1252`, `X`, `Y`) sont des littéraux légitimes.
const TABLES: &[&str] = &[
    "label",
    "title",
    "separator_label",
    "thousands_label",
    "decimal_label",
    "column_kind_label",
];

// ---------------------------------------------------------------------------
// Les infractions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Infraction {
    fichier: String,
    /// `Type::nom` dans un bloc `impl`, `nom` sinon.
    fonction: String,
    ligne: usize,
    appel: String,
    extrait: String,
}

/// Les positions d'ouverture des littéraux **textuels** de `masque[debut..fin]`
/// — ceux qui portent au moins une lettre — sans ceux d'un `tr!(…)`, dont le
/// littéral est la clé du catalogue.
fn litteraux_textuels(masque: &[char], debut: usize, fin: usize) -> Vec<usize> {
    let mut trouves = Vec::new();
    let mut i = debut;
    while i < fin {
        if commence_par(masque, i, "tr!(") && (i == 0 || !est_ident(masque[i - 1])) {
            match fermante(masque, i + 3) {
                Some(f) => {
                    i = f + 1;
                    continue;
                }
                None => break,
            }
        }
        if masque[i] == '"' {
            let mut j = i + 1;
            while j < fin && masque[j] != '"' {
                j += 1;
            }
            if masque[i + 1..j.min(fin)].contains(&'A') {
                trouves.push(i);
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    trouves
}

/// Le littéral tel qu'écrit, depuis son guillemet ouvrant.
fn extrait(source: &Masque, ouverture: usize) -> String {
    let mut fin = ouverture + 1;
    while fin < source.masque.len() && source.masque[fin] != '"' {
        fin += 1;
    }
    source.original[ouverture..=fin.min(source.original.len() - 1)]
        .iter()
        .collect()
}

/// Toutes les infractions d'un fichier, code de test exclu.
fn infractions(
    fichier: &Fichier,
    appels: &[&str],
    constructeurs: &[&str],
    tables: &[&str],
) -> Vec<Infraction> {
    let source = fichier.production();
    let masque = &source.masque;
    let chemin = format!("src/{}", fichier.chemin);
    let fns = fonctions(masque);
    let fonction_a = |position: usize| -> String {
        fns.iter()
            .rev()
            .find(|(p, _)| *p < position)
            .map_or_else(|| "?".to_string(), |(_, nom)| nom.clone())
    };

    // (parenthèse ouvrante, position de l'appel, appel)
    let mut appels_trouves: Vec<(usize, usize, String)> = Vec::new();
    for i in 0..masque.len() {
        if masque[i] == '.' {
            let nom = ident_en(masque, i + 1);
            let apres = i + 1 + nom.chars().count();
            if appels.contains(&nom.as_str()) && masque.get(apres) == Some(&'(') {
                appels_trouves.push((apres, i, format!(".{nom}(")));
            }
        } else if masque[i].is_uppercase() && (i == 0 || !est_ident(masque[i - 1])) {
            let nom = ident_en(masque, i);
            let apres = i + nom.chars().count();
            if constructeurs.contains(&nom.as_str()) && commence_par(masque, apres, "::new(") {
                appels_trouves.push((apres + 5, i, format!("{nom}::new(")));
            }
        }
    }

    let mut trouvees = Vec::new();
    // Un texte dans `label(RichText::new("…"))` n'est compté qu'une fois.
    let mut vus = std::collections::BTreeSet::new();
    for (parenthese, position, appel) in appels_trouves {
        let Some(f) = fermante(masque, parenthese) else {
            continue;
        };
        for ouverture in litteraux_textuels(masque, parenthese + 1, f) {
            if vus.insert(ouverture) {
                trouvees.push(Infraction {
                    fichier: chemin.clone(),
                    fonction: fonction_a(position),
                    ligne: ligne_de(&source.original, ouverture),
                    appel: appel.clone(),
                    extrait: extrait(&source, ouverture),
                });
            }
        }
    }

    // Les tables `fn label()` / `fn title()` : tout littéral textuel du corps.
    for (position, qualifie) in &fns {
        let nom = nom_court(qualifie);
        if !tables.contains(&nom) {
            continue;
        }
        let Some((accolade, f)) = corps(masque, *position) else {
            continue;
        };
        for ouverture in litteraux_textuels(masque, accolade + 1, f) {
            if vus.insert(ouverture) {
                trouvees.push(Infraction {
                    fichier: chemin.clone(),
                    fonction: qualifie.clone(),
                    ligne: ligne_de(&source.original, ouverture),
                    appel: format!("fn {nom}()"),
                    extrait: extrait(&source, ouverture),
                });
            }
        }
    }
    trouvees
}

// ---------------------------------------------------------------------------
// La garde
// ---------------------------------------------------------------------------

/// Les infractions de `sources`, une ligne chacune ; vide si tout est en règle.
fn ecarts(sources: &[Fichier]) -> Vec<String> {
    let mut ecarts = Vec::new();
    for fichier in sources {
        for infraction in infractions(fichier, APPELS, CONSTRUCTEURS, TABLES) {
            ecarts.push(format!(
                "{}:{} dans `{}` : {} {} — texte en dur, passer par `tr!` avec une clé \
                 de `i18n/en/stokesforge_app.ftl` (skill `stokesforge-textes-interface`)",
                infraction.fichier,
                infraction.ligne,
                infraction.fonction,
                infraction.appel,
                infraction.extrait
            ));
        }
    }
    ecarts
}

/// Aucun texte d'interface écrit en dur dans `src/` : tout passe par `tr!`.
#[test]
fn aucun_libelle_en_dur() {
    let ecarts = ecarts(&sources::sources());
    assert!(
        ecarts.is_empty(),
        "{} écart(s) :\n  {}",
        ecarts.len(),
        ecarts.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// La zone #43, gardée sans réserve
// ---------------------------------------------------------------------------

/// Les fonctions de la fenêtre des propriétés du graphe, de
/// l'éditeur de texte sur le graphique et du pied commun des dialogues
/// modaux (#43). Zéro dérogation, et plus strictement que [`APPELS`] : **aucun**
/// littéral textuel dans leur corps, quelle que soit la forme de l'appel
/// (`strong`, `weak`, `color_field`, `text_field`, `set_title`, un argument de
/// fonction libre…) — seuls passent les littéraux qui sont des identifiants
/// (`[a-z0-9_./-]` : sels d'identifiants egui, adresses), les sigles de format
/// (`SVG`) et les clés de `tr!`.
/// À fusionner dans la garde générale quand [`APPELS`] saura voir ces formes
/// (#48).
///
/// Une entrée nomme une fonction par son dernier segment (`modal_dialog` :
/// fonction libre ou méthode `Type::modal_dialog`), cherchée dans **toutes** les
/// sources ; une entrée qu'on ne trouve nulle part fait échouer la garde (#116)
/// — une fonction renommée ne sort pas de la zone en silence.
const ZONE_43: &[&str] = &[
    "plot_settings_window",
    "plot_settings_contents",
    "theme_editor",
    "formula_memo",
    "text_field",
    "axis_editor",
    "series_editor",
    "legend_editor",
    "plot_type_label",
    "line_style_label",
    "symbol_shape_label",
    "legend_position_label",
    "annotation_editor_ui",
    "graph_view_contents",
    "request_graph_export",
    "modal_dialog",
    "modal_dialog_with",
];

/// Les littéraux textuels, hors identifiants, du corps des fonctions `zone` de
/// `source` (vue de production) : (fonction, ligne, littéral).
fn litteraux_nus(source: &Masque, zone: &[&str]) -> Vec<(String, usize, String)> {
    let masque = &source.masque;
    let mut trouves = Vec::new();
    for (position, nom) in fonctions(masque) {
        if !zone.contains(&nom_court(&nom)) {
            continue;
        }
        let Some((accolade, fin)) = corps(masque, position) else {
            continue;
        };
        for ouverture in litteraux_textuels(masque, accolade + 1, fin) {
            let litteral = extrait(source, ouverture);
            let contenu = &litteral[1..litteral.len().saturating_sub(1)];
            let identifiant = contenu
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_./-".contains(c));
            // Un sigle de format (`SVG`, `PDF`) n'a pas de traduction.
            let sigle = contenu
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            if !identifiant && !sigle {
                trouves.push((nom.clone(), ligne_de(&source.original, ouverture), litteral));
            }
        }
    }
    trouves
}

/// Les entrées de `zone` qu'aucune fonction de production de `sources` ne porte.
fn absentes<'z>(sources: &[Fichier], zone: &[&'z str]) -> Vec<&'z str> {
    zone.iter()
        .copied()
        .filter(|nom| fonctions_nommees(sources, nom).is_empty())
        .collect()
}

/// Les littéraux nus de la zone dans `sources`, une ligne chacun.
fn ecarts_de_la_zone(sources: &[Fichier], zone: &[&str]) -> Vec<String> {
    let mut ecarts = Vec::new();
    for fichier in sources {
        for (fonction, ligne, litteral) in litteraux_nus(&fichier.production(), zone) {
            ecarts.push(format!(
                "src/{}:{ligne} dans `{fonction}` : {litteral}",
                fichier.chemin
            ));
        }
    }
    ecarts
}

fn nus(code: &str, zone: &[&str]) -> Vec<(String, usize, String)> {
    litteraux_nus(&fichier("t.rs", code).production(), zone)
}

fn fichier(chemin: &str, texte: &str) -> Fichier {
    Fichier {
        chemin: chemin.to_string(),
        texte: texte.to_string(),
        de_test: false,
    }
}

/// La fenêtre des propriétés du graphe, ses onglets, l'éditeur de texte et le
/// pied des dialogues modaux sont **entièrement** passés par `tr!` : aucune
/// dérogation (#43).
#[test]
fn les_proprietes_du_graphe_et_l_export_sont_sans_litteral() {
    let sources = sources::sources();
    let absentes = absentes(&sources, ZONE_43);
    assert!(
        absentes.is_empty(),
        "fonction(s) de la zone #43 introuvable(s) dans src/ : {absentes:?} — renommée ? \
         mettre à jour ZONE_43 plutôt que la laisser sortir de la garde"
    );
    let ecarts = ecarts_de_la_zone(&sources, ZONE_43);
    assert!(
        ecarts.is_empty(),
        "{} texte(s) en dur dans la zone #43 :\n  {}",
        ecarts.len(),
        ecarts.join("\n  ")
    );
}

/// Le témoin de la garde de la zone : un littéral dans une forme d'appel que
/// [`APPELS`] ne voit pas (`strong`, un argument de fonction libre) est refusé ;
/// un identifiant et une clé de `tr!` passent ; une fonction hors zone aussi.
#[test]
fn la_garde_de_la_zone_43_voit_les_formes_que_appels_ignore() {
    let zone = &["page"];
    for code in [
        r#"fn page(ui: &mut Ui) { ui.strong("Titre"); }"#,
        r#"fn page(ui: &mut Ui) { champ(ui, "Couleur", &mut x); }"#,
        r#"fn page(ui: &mut Ui) { let _ = ui.hyperlink("lien"); let _ = Some("Auto"); }"#,
        r#"fn page(ui: &mut Ui) { let _ = format!("Axe {}", n); }"#,
    ] {
        assert_eq!(nus(code, zone).len(), 1, "non vu : {code}");
    }
    for code in [
        r#"fn page(ui: &mut Ui) { ui.strong(tr!("plot-tab-axes")); }"#,
        r#"fn page(ui: &mut Ui) { ui.id_salt("plot_settings_series_list"); }"#,
        r#"fn page(ui: &mut Ui) { let _ = tr!("a", url = "typst.app/docs/reference/math"); }"#,
        r#"fn ailleurs(ui: &mut Ui) { ui.strong("Titre"); }"#,
        r#"fn page(d: Dialog) { d.add_filter("SVG", &["svg"]); }"#,
        "fn page(ui: &mut Ui) { let _ = \"→\"; }",
    ] {
        assert!(nus(code, zone).is_empty(), "faux positif : {code}");
    }
}

/// Le témoin de #116 : une fonction de la zone déplacée hors d'`app.rs` —
/// dans un module de `src/app/`, ou devenue méthode — reste gardée ; sa copie
/// de test ne l'est pas ; une entrée de la zone introuvable est signalée.
#[test]
fn la_zone_43_suit_ses_fonctions_hors_d_app_rs() {
    let sources = [
        fichier(
            "app/proprietes.rs",
            "pub(crate) fn page(ui: &mut Ui) {\n    ui.strong(\"Titre\");\n}\n\
             #[cfg(test)]\nmod tests { fn page(ui: &mut Ui) { ui.strong(\"Test\"); } }",
        ),
        fichier(
            "app/methodes.rs",
            "impl App {\n    fn export(&mut self) { champ(\"Format\"); }\n}",
        ),
    ];
    let ecarts = ecarts_de_la_zone(&sources, &["page", "export"]);
    assert_eq!(
        ecarts,
        [
            "src/app/proprietes.rs:2 dans `page` : \"Titre\"",
            "src/app/methodes.rs:2 dans `App::export` : \"Format\"",
        ]
    );
    assert_eq!(
        absentes(&sources, &["page", "export", "disparue"]),
        ["disparue"]
    );
    // Une fonction qui n'existe plus qu'en test est introuvable, pas gardée.
    let seulement_en_test = [fichier(
        "app/a.rs",
        "#[cfg(test)]\nmod tests { fn page() {} }",
    )];
    assert_eq!(absentes(&seulement_en_test, &["page"]), ["page"]);
}

// ---------------------------------------------------------------------------
// Les témoins : sans eux, une garde qui ne voit rien passerait pour verte
// ---------------------------------------------------------------------------

fn trouve(code: &str) -> Vec<Infraction> {
    infractions(&fichier("t.rs", code), APPELS, CONSTRUCTEURS, TABLES)
}

#[test]
fn la_garde_voit_un_litteral_dans_chaque_forme_d_appel() {
    for code in [
        r#"fn f(ui: &mut Ui) { ui.label("Ouvrir"); }"#,
        r#"fn f(ui: &mut Ui) { ui.menu_button("Fichier", |ui| {}); }"#,
        r#"fn f(ui: &mut Ui) { ui.checkbox(&mut b, "Afficher"); }"#,
        r#"fn f(ui: &mut Ui) { ui.add(egui::Button::new("Importer…")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(egui::RichText::new("Titre").strong()); }"#,
        r#"fn f(ui: &mut Ui) { ui.button(x).on_hover_text("Aide"); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("Axe {}", n)); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("{} {}", tr!("a"), "b")); }"#,
        r##"fn f(ui: &mut Ui) { ui.label(r#"brut"#); }"##,
        r#"fn f() { let _ = ViewportBuilder::default().with_title("StokesForge"); }"#,
    ] {
        assert_eq!(trouve(code).len(), 1, "non vu : {code}");
    }
}

#[test]
fn un_label_a_deux_branches_compte_chaque_litteral() {
    let code = r#"fn f(ui: &mut Ui) { ui.label(if a { "Oui" } else { "Non" }); }"#;
    assert_eq!(trouve(code).len(), 2);
}

#[test]
fn un_texte_stylé_dans_un_label_n_est_compte_qu_une_fois() {
    let code = r#"fn f(ui: &mut Ui) { ui.label(RichText::new("Titre")); }"#;
    assert_eq!(trouve(code).len(), 1);
}

#[test]
fn la_garde_laisse_passer_ce_qui_n_est_pas_un_libelle() {
    for code in [
        r#"fn f(ui: &mut Ui) { ui.label(tr!("menu-file")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(tr!("witness-count", count = 2)); }"#,
        r#"fn f(ui: &mut Ui) { ui.add(egui::Button::new(tr!("menu-file"))); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(nom); }"#,
        r#"fn f(ui: &mut Ui) { ui.label("→"); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(" · "); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("{}", n)); }"#,
        r#"fn f(ui: &mut Ui) { ui.colored_label(c, format!("{code} {label}")); }"#,
        r#"fn f(ui: &mut Ui) { ui.separator(); ui.id_salt("graphe"); }"#,
        "fn f(ui: &mut Ui) {\n    // ui.label(\"commentaire\");\n}",
        "fn f(ui: &mut Ui) { let _ = \"ui.label(\\\"dans une chaîne\\\")\"; }",
        r#"fn f(ui: &mut Ui) { let c = '"'; ui.label(tr!("a")); }"#,
        "fn f(ui: &mut Ui) { ui.label(tr!(\"a\")); }\n#[cfg(test)]\nmod t { fn g(ui: &mut Ui) { ui.label(\"test\"); } }",
    ] {
        assert!(trouve(code).is_empty(), "faux positif : {code}");
    }
}

#[test]
fn la_garde_lit_les_tables_de_libelles() {
    let code = "impl Onglet {\n    pub fn label(self) -> &'static str {\n        match self {\n            Onglet::A => \"Axes\",\n            Onglet::B => \"Séries\",\n        }\n    }\n}\n";
    let trouvees = trouve(code);
    assert_eq!(trouvees.len(), 2);
    assert!(trouvees.iter().all(|i| i.fonction == "Onglet::label"));
    let converti = "impl Onglet {\n    pub fn label(self) -> String {\n        match self {\n            Onglet::A => tr!(\"tab-axes\"),\n        }\n    }\n}\n";
    assert!(trouve(converti).is_empty());
    // Un `;` de tableau dans la signature ne cache pas le corps (#116).
    let tableau = "impl Onglet {\n    fn label(self, _c: [u8; 4]) -> &'static str {\n        \"Axes\"\n    }\n}\n";
    assert_eq!(trouve(tableau).len(), 1);
    // Une autre fonction ne lit pas ses littéraux comme une table.
    assert!(trouve("fn nom() -> &'static str { \"Axes\" }").is_empty());
}

#[test]
fn la_garde_nomme_la_fonction_et_le_bloc_impl() {
    let code = "impl<T: Copy> Fenetre<T> {\n    fn menu(&self, ui: &mut Ui) { ui.label(\"x y\"); }\n}\nfn libre(ui: &mut Ui) { ui.label(\"x y\"); }\nimpl Dessin for Fenetre {\n    fn peint(&self, ui: &mut Ui) { ui.label(\"x y\"); }\n}\n";
    let noms: Vec<String> = trouve(code).into_iter().map(|i| i.fonction).collect();
    assert_eq!(noms, ["Fenetre::menu", "libre", "Fenetre::peint"]);
}

#[test]
fn la_garde_rapporte_chaque_litteral_avec_son_fichier_et_sa_fonction() {
    let sources = vec![fichier(
        "t.rs",
        "fn menu(ui: &mut Ui) { ui.label(\"un\"); ui.label(\"deux\"); }\nfn autre(ui: &mut Ui) { ui.label(tr!(\"ok\")); }\n",
    )];
    let rapport = ecarts(&sources);
    assert_eq!(rapport.len(), 2, "{rapport:?}");
    assert!(rapport
        .iter()
        .all(|e| e.contains("src/t.rs:1") && e.contains("`menu`")));
}

/// Le témoin de #116 : un libellé en dur dans un sous-dossier de `src/`, ou
/// placé après un module de test (`errors.rs` alterne les deux), est vu ; le
/// même dans un fichier de test ne l'est pas.
#[test]
fn la_garde_voit_les_sous_dossiers_et_le_code_apres_un_module_de_test() {
    let code = "mod a {\n    #[cfg(test)]\n    mod tests { fn t(ui: &mut Ui) { ui.label(\"test\"); } }\n}\n\
                fn menu(ui: &mut Ui) { ui.label(\"Ouvrir\"); }\n";
    let rapport = ecarts(&[fichier("app/grille/menu.rs", code)]);
    assert_eq!(rapport.len(), 1, "{rapport:?}");
    assert!(
        rapport[0].starts_with("src/app/grille/menu.rs:5 dans `menu`"),
        "{rapport:?}"
    );
    let mut de_test = fichier("app/grille/tests.rs", code);
    de_test.de_test = true;
    assert!(ecarts(&[de_test]).is_empty());
}

/// Les dépendances de traduction n'existent que dans `stokesforge-app` : aucun
/// crate du dessous ne traduit (chantier #39). Les listes blanches de
/// `no_ui_dependency` en couvrent déjà une partie ; celui-ci couvre le reste du
/// workspace.
#[test]
fn les_dependances_de_traduction_restent_dans_stokesforge_app() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let interdites = [
        "i18n-embed",
        "i18n-embed-fl",
        "fluent",
        "fluent-syntax",
        "rust-embed",
    ];
    let mut vus = 0;
    for entree in std::fs::read_dir(crates).expect("dossier crates").flatten() {
        let nom = entree.file_name().to_string_lossy().into_owned();
        let manifeste = entree.path().join("Cargo.toml");
        let Ok(contenu) = std::fs::read_to_string(&manifeste) else {
            continue;
        };
        vus += 1;
        if nom == "stokesforge-app" {
            continue;
        }
        for ligne in contenu.lines() {
            let cle = ligne.split('=').next().unwrap_or("").trim();
            assert!(
                !interdites.contains(&cle),
                "{nom} dépend de `{cle}` : seule l'application traduit (#39)"
            );
        }
    }
    assert!(vus > 10, "le workspace n'a pas été parcouru");
}
