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
//! ## La liste de dérogations n'est pas un état normal
//!
//! [`DEROGATIONS`] recense l'existant d'avant #41, **par fichier et par zone**,
//! chaque zone portant le ticket qui la convertit : #42 menus, menus
//! contextuels, barre d'état, titre de fenêtre ; #43 propriétés du graphe et
//! export ; #44 assistant d'import ; #45 classeur, grille, Project Manager,
//! notes, sélecteur de couleur, console de script. **Elle ne doit jamais
//! grossir**, elle se réduit zone par zone, et elle doit être **vide** à #48.
//!
//! Le test est strict **dans les deux sens** : un littéral de plus échoue (un
//! nouveau texte en dur), et un littéral de moins échoue aussi tant que le
//! compte n'est pas abaissé ici — la réduction se voit dans le diff, au lieu de
//! laisser une marge où un nouveau littéral se cacherait.
//!
//! ## Ce que la garde ne voit pas
//!
//! Un texte posé par un autre chemin : un `&str` bâti ailleurs puis passé en
//! variable, un titre de `modal_dialog_with` / `window_header` / `tab_strip`
//! (leurs arguments mêlent texte et identifiants), un nom de colonne ou
//! d'unité calculé. Les tickets de conversion étendent [`APPELS`] quand ils
//! traitent ces zones, et la capture en pseudo-langue (`PLOTYX_LANG=en-XA`)
//! montre ce qui a échappé : un texte sans `[[ ]]`.

use std::collections::BTreeMap;
use std::path::Path;

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
/// textuel dedans.
const TABLES: &[&str] = &["label", "title"];

/// Une dérogation : `nombre` littéraux tolérés dans les fonctions `fonctions`
/// du fichier `fichier`, jusqu'à la conversion du ticket `ticket`. Les
/// fonctions s'écrivent `Type::nom` quand elles sont dans un bloc `impl`.
struct Derogation {
    fichier: &'static str,
    zone: &'static str,
    ticket: &'static str,
    fonctions: &'static [&'static str],
    nombre: usize,
}

/// L'existant d'avant #41 : **213 littéraux**, 14 zones. À réduire, jamais à
/// grossir ; vide à #48. Rangée par ticket de conversion (#42 : 59, #43 : 45,
/// #44 : 71, #45 : 38) ; chaque ticket retire ses zones, et leur compte avec.
const DEROGATIONS: &[Derogation] = &[
    // --- #42 : menus, menus contextuels, actions, infobulles, titre de fenêtre
    Derogation {
        fichier: "app.rs",
        zone: "barre de menus",
        ticket: "#42",
        fonctions: &["PlotyxApp::menu_bar"],
        nombre: 34,
    },
    Derogation {
        fichier: "app.rs",
        zone: "menus contextuels de la grille et du graphe",
        ticket: "#42",
        fonctions: &["grid_edit_menu", "graph_context_menu"],
        nombre: 18,
    },
    Derogation {
        fichier: "app.rs",
        zone: "fenêtres détachées, disposition, infobulles d'onglets",
        ticket: "#42",
        fonctions: &[
            "PlotyxApp::show_detached_project_manager",
            "PlotyxApp::show_detached_tabs",
            "PlotyxApp::ui",
            "tab_strip",
        ],
        nombre: 7,
    },
    // --- #43 : propriétés du graphe et export ---------------------------------
    Derogation {
        fichier: "app.rs",
        zone: "propriétés du graphe : axes, séries, légende, mémo Typst",
        ticket: "#43",
        fonctions: &[
            "axis_editor",
            "series_editor",
            "legend_editor",
            "plot_settings_contents",
            "formula_memo",
        ],
        nombre: 22,
    },
    Derogation {
        fichier: "app.rs",
        zone: "textes libres et vue du graphe",
        ticket: "#43",
        fonctions: &["annotation_editor_ui", "graph_view_contents"],
        nombre: 6,
    },
    Derogation {
        fichier: "app.rs",
        zone: "pied commun des dialogues modaux",
        ticket: "#43",
        fonctions: &["modal_dialog_with"],
        nombre: 2,
    },
    Derogation {
        fichier: "state.rs",
        zone: "tables de libellés : onglets, axes, cibles du graphe",
        ticket: "#43",
        fonctions: &[
            "PlotSettingsTab::label",
            "PlotAxis::label",
            "PlotTarget::label",
        ],
        nombre: 15,
    },
    // --- #44 : assistant d'import ---------------------------------------------
    Derogation {
        fichier: "app.rs",
        zone: "assistant d'import : fenêtre, six étapes, aperçu",
        ticket: "#44",
        fonctions: &[
            "import_window",
            "import_step_files",
            "import_step_format",
            "import_step_header",
            "import_step_columns",
            "import_step_metadata",
            "import_step_destination",
            "import_line_preview_widget",
            "subheader_role_row",
            "data_end_row",
        ],
        nombre: 65,
    },
    Derogation {
        fichier: "state.rs",
        zone: "titres des étapes de l'assistant d'import",
        ticket: "#44",
        fonctions: &["ImportStep::title"],
        nombre: 6,
    },
    // --- #45 : classeur, grille, Project Manager, couleur, console -------------
    Derogation {
        fichier: "app.rs",
        zone: "grille : en-têtes de colonnes et de lignes, cellule de métadonnées",
        ticket: "#45",
        fonctions: &[
            "column_header_widget",
            "grid_contents",
            "row_number_header_widget",
            "metadata_cell_widget",
        ],
        nombre: 13,
    },
    Derogation {
        fichier: "app.rs",
        zone: "Project Manager",
        ticket: "#45",
        fonctions: &["project_manager_contents_from_tree", "project_tree_node_ui"],
        nombre: 4,
    },
    Derogation {
        fichier: "app.rs",
        zone: "console de script",
        ticket: "#45",
        fonctions: &["script_contents"],
        nombre: 5,
    },
    Derogation {
        fichier: "color_picker.rs",
        zone: "sélecteur de couleur",
        ticket: "#45",
        fonctions: &["color_button", "picker_contents"],
        nombre: 14,
    },
    Derogation {
        fichier: "state.rs",
        zone: "cible d'enregistrement des couleurs",
        ticket: "#45",
        fonctions: &["SaveTarget::label"],
        nombre: 2,
    },
];

// ---------------------------------------------------------------------------
// Lecture des sources
// ---------------------------------------------------------------------------

/// Une source dont les commentaires sont blanchis et le contenu des chaînes
/// remplacé : `A` pour une lettre, `_` pour le reste. Même longueur (en
/// caractères) que l'original, donc mêmes positions.
struct Source {
    masque: Vec<char>,
    original: Vec<char>,
}

fn est_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn masquer(texte: &str) -> Source {
    let original: Vec<char> = texte.chars().collect();
    let mut masque = original.clone();
    let n = original.len();
    let mut i = 0;
    while i < n {
        let c = original[i];
        let suivant = original.get(i + 1).copied();
        // Commentaire de ligne.
        if c == '/' && suivant == Some('/') {
            while i < n && original[i] != '\n' {
                masque[i] = ' ';
                i += 1;
            }
            continue;
        }
        // Commentaire de bloc, imbriqué.
        if c == '/' && suivant == Some('*') {
            let mut profondeur = 0;
            while i < n {
                if original[i] == '/' && original.get(i + 1) == Some(&'*') {
                    profondeur += 1;
                    masque[i] = ' ';
                    masque[i + 1] = ' ';
                    i += 2;
                } else if original[i] == '*' && original.get(i + 1) == Some(&'/') {
                    profondeur -= 1;
                    masque[i] = ' ';
                    masque[i + 1] = ' ';
                    i += 2;
                    if profondeur == 0 {
                        break;
                    }
                } else {
                    if original[i] != '\n' {
                        masque[i] = ' ';
                    }
                    i += 1;
                }
            }
            continue;
        }
        let precedent_ident = i > 0 && est_ident(original[i - 1]);
        // Chaîne brute : r"…", r#"…"#, br#"…"#.
        if (c == 'r' || (c == 'b' && suivant == Some('r'))) && !precedent_ident {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut dieses = 0;
            while original.get(j) == Some(&'#') {
                dieses += 1;
                j += 1;
            }
            if original.get(j) == Some(&'"') {
                j += 1;
                while j < n {
                    if original[j] == '"'
                        && (0..dieses).all(|k| original.get(j + 1 + k) == Some(&'#'))
                    {
                        break;
                    }
                    masquer_caractere(&original, &mut masque, j);
                    j += 1;
                }
                i = (j + 1 + dieses).min(n);
                continue;
            }
        }
        // Chaîne ordinaire.
        if c == '"' {
            let mut j = i + 1;
            while j < n && original[j] != '"' {
                if original[j] == '\\' {
                    masque[j] = '_';
                    j += 1;
                    if j < n {
                        let echappe = original[j];
                        masquer_neutre(&original, &mut masque, j);
                        j += 1;
                        if echappe == 'u' && original.get(j) == Some(&'{') {
                            while j < n && original[j] != '}' {
                                masquer_neutre(&original, &mut masque, j);
                                j += 1;
                            }
                            if j < n {
                                masquer_neutre(&original, &mut masque, j);
                                j += 1;
                            }
                        }
                    }
                    continue;
                }
                masquer_caractere(&original, &mut masque, j);
                j += 1;
            }
            i = (j + 1).min(n);
            continue;
        }
        // Littéral de caractère (et non une durée de vie).
        if c == '\'' {
            if suivant == Some('\\') {
                let mut j = i + 2;
                while j < n && original[j] != '\'' {
                    masque[j] = '_';
                    j += 1;
                }
                masque[i + 1] = '_';
                i = (j + 1).min(n);
                continue;
            }
            if original.get(i + 2) == Some(&'\'') {
                masque[i + 1] = '_';
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    Source { masque, original }
}

/// Dans une chaîne : une lettre devient `A`, le reste `_`, sauf les fins de ligne.
fn masquer_caractere(original: &[char], masque: &mut [char], i: usize) {
    masque[i] = if original[i] == '\n' {
        '\n'
    } else if original[i].is_alphabetic() {
        'A'
    } else {
        '_'
    };
}

/// Dans une séquence d'échappement : jamais une lettre de texte.
fn masquer_neutre(original: &[char], masque: &mut [char], i: usize) {
    masque[i] = if original[i] == '\n' { '\n' } else { '_' };
}

/// Position du délimiteur fermant qui répond à celui ouvert en `ouvrante`.
fn fermante(masque: &[char], ouvrante: usize) -> Option<usize> {
    let mut profondeur = 0usize;
    for (i, c) in masque.iter().enumerate().skip(ouvrante) {
        match c {
            '(' | '[' | '{' => profondeur += 1,
            ')' | ']' | '}' => {
                profondeur = profondeur.checked_sub(1)?;
                if profondeur == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn commence_par(masque: &[char], position: usize, motif: &str) -> bool {
    motif
        .chars()
        .enumerate()
        .all(|(k, c)| masque.get(position + k) == Some(&c))
}

fn ident_en(masque: &[char], position: usize) -> String {
    masque[position..]
        .iter()
        .take_while(|c| est_ident(**c))
        .collect()
}

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

/// Les fonctions du fichier : position du `fn`, nom qualifié par le bloc `impl`
/// qui la contient.
fn fonctions(masque: &[char]) -> Vec<(usize, String)> {
    let impls = blocs_impl(masque);
    let mut trouvees = Vec::new();
    for i in 0..masque.len() {
        if commence_par(masque, i, "fn ") && (i == 0 || !est_ident(masque[i - 1])) {
            let nom = ident_en(masque, i + 3);
            if nom.is_empty() {
                continue;
            }
            let qualifie = impls
                .iter()
                .find(|(debut, fin, _)| *debut < i && i < *fin)
                .map_or(nom.clone(), |(_, _, cible)| format!("{cible}::{nom}"));
            trouvees.push((i, qualifie));
        }
    }
    trouvees
}

/// Les blocs `impl` : (début, fin, type visé).
fn blocs_impl(masque: &[char]) -> Vec<(usize, usize, String)> {
    let mut blocs = Vec::new();
    for i in 0..masque.len() {
        if !commence_par(masque, i, "impl")
            || (i > 0 && est_ident(masque[i - 1]))
            || masque.get(i + 4).is_some_and(|c| est_ident(*c))
        {
            continue;
        }
        let Some(accolade) = (i..masque.len()).find(|k| masque[*k] == '{' || masque[*k] == ';')
        else {
            continue;
        };
        if masque[accolade] != '{' {
            continue;
        }
        let Some(fin) = fermante(masque, accolade) else {
            continue;
        };
        let entete: String = masque[i + 4..accolade].iter().collect();
        blocs.push((i, fin, cible_de_impl(&entete)));
    }
    blocs
}

/// Le type visé par l'en-tête d'un `impl` : `Type` pour `impl Type`,
/// `impl<T> Type<T>`, `impl Trait for Type`.
fn cible_de_impl(entete: &str) -> String {
    let mut reste = entete.trim();
    if let Some((_, apres)) = reste.rsplit_once(" for ") {
        reste = apres.trim();
    } else if reste.starts_with('<') {
        let mut profondeur = 0usize;
        for (k, c) in reste.char_indices() {
            match c {
                '<' => profondeur += 1,
                '>' => {
                    profondeur = profondeur.saturating_sub(1);
                    if profondeur == 0 {
                        reste = reste[k + 1..].trim();
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    reste.chars().take_while(|c| est_ident(*c)).collect()
}

fn ligne_de(original: &[char], position: usize) -> usize {
    1 + original[..position].iter().filter(|c| **c == '\n').count()
}

/// Le littéral tel qu'écrit, depuis son guillemet ouvrant.
fn extrait(source: &Source, ouverture: usize) -> String {
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
    fichier: &str,
    texte: &str,
    appels: &[&str],
    constructeurs: &[&str],
    tables: &[&str],
) -> Vec<Infraction> {
    let source = masquer(texte);
    let fin_production = (0..source.masque.len())
        .find(|i| commence_par(&source.masque, *i, "#[cfg(test)]"))
        .unwrap_or(source.masque.len());
    let masque = &source.masque[..fin_production];
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
                    fichier: fichier.to_string(),
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
        let nom = qualifie.rsplit("::").next().unwrap_or(qualifie);
        if !tables.contains(&nom) {
            continue;
        }
        let Some(accolade) = (*position..masque.len()).find(|k| masque[*k] == '{') else {
            continue;
        };
        let Some(f) = fermante(masque, accolade) else {
            continue;
        };
        for ouverture in litteraux_textuels(masque, accolade + 1, f) {
            if vus.insert(ouverture) {
                trouvees.push(Infraction {
                    fichier: fichier.to_string(),
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

fn sources_de_l_application() -> Vec<(String, String)> {
    let dossier = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut fichiers: Vec<_> = std::fs::read_dir(&dossier)
        .expect("dossier src")
        .filter_map(Result::ok)
        .map(|entree| entree.path())
        .filter(|chemin| chemin.extension().is_some_and(|e| e == "rs"))
        .collect();
    fichiers.sort();
    fichiers
        .into_iter()
        .map(|chemin| {
            let nom = chemin
                .file_name()
                .expect("nom")
                .to_string_lossy()
                .into_owned();
            let texte = std::fs::read_to_string(&chemin).expect("lecture");
            (nom, texte)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// La garde
// ---------------------------------------------------------------------------

/// Compare les infractions de `sources` à la liste `derogations` ; rend la
/// liste des écarts, vide si tout est en règle.
fn ecarts(sources: &[(String, String)], derogations: &[Derogation]) -> Vec<String> {
    let mut ecarts = Vec::new();

    // Une fonction ne figure que dans une seule zone.
    let mut zone_de: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for derogation in derogations {
        for fonction in derogation.fonctions {
            if let Some(autre) = zone_de.insert((derogation.fichier, fonction), derogation.zone) {
                ecarts.push(format!(
                    "{} : `{fonction}` est dans deux zones (« {autre} » et « {} »)",
                    derogation.fichier, derogation.zone
                ));
            }
        }
    }

    let mut comptes: BTreeMap<(String, &str), usize> = BTreeMap::new();
    for (fichier, texte) in sources {
        for infraction in infractions(fichier, texte, APPELS, CONSTRUCTEURS, TABLES) {
            let zone = derogations.iter().find(|d| {
                d.fichier == fichier && d.fonctions.contains(&infraction.fonction.as_str())
            });
            match zone {
                Some(zone) => *comptes.entry((fichier.clone(), zone.zone)).or_default() += 1,
                None => ecarts.push(format!(
                    "{fichier}:{} dans `{}` : {} {} — texte en dur, passer par `tr!` avec une clé \
                     de `i18n/en/plotyx_app.ftl` (skill `plotyx-textes-interface`)",
                    infraction.ligne, infraction.fonction, infraction.appel, infraction.extrait
                )),
            }
        }
    }

    for derogation in derogations {
        let trouve = comptes
            .get(&(derogation.fichier.to_string(), derogation.zone))
            .copied()
            .unwrap_or(0);
        if trouve > derogation.nombre {
            ecarts.push(format!(
                "{} / {} ({}) : {trouve} littéral(aux) pour {} toléré(s) — un texte en dur de plus",
                derogation.fichier, derogation.zone, derogation.ticket, derogation.nombre
            ));
        } else if trouve < derogation.nombre {
            ecarts.push(format!(
                "{} / {} ({}) : {trouve} littéral(aux) pour {} déclaré(s) — abaisser le compte \
                 dans `DEROGATIONS` (ou retirer la zone si elle est à 0)",
                derogation.fichier, derogation.zone, derogation.ticket, derogation.nombre
            ));
        }
    }
    ecarts
}

/// Aucun texte d'interface écrit en dur dans `src/`, hors la liste des
/// dérogations d'avant #41 (voir la doc du module : elle se réduit, elle ne
/// grossit pas, et elle est vide à #48).
#[test]
fn aucun_libelle_en_dur() {
    let ecarts = ecarts(&sources_de_l_application(), DEROGATIONS);
    assert!(
        ecarts.is_empty(),
        "{} écart(s) :\n  {}",
        ecarts.len(),
        ecarts.join("\n  ")
    );
}

/// Le total de la liste, pour qu'un relecteur le voie d'un coup d'œil.
#[test]
fn le_total_des_derogations_se_lit() {
    let total: usize = DEROGATIONS.iter().map(|d| d.nombre).sum();
    eprintln!(
        "[i18n] dérogations : {total} littéral(aux) en {} zone(s)",
        DEROGATIONS.len()
    );
    for derogation in DEROGATIONS {
        eprintln!(
            "[i18n]   {:<14} {:<48} {:>4}  {}",
            derogation.fichier, derogation.zone, derogation.nombre, derogation.ticket
        );
    }
}

// ---------------------------------------------------------------------------
// Les témoins : sans eux, une garde qui ne voit rien passerait pour verte
// ---------------------------------------------------------------------------

fn trouve(code: &str) -> Vec<Infraction> {
    infractions("t.rs", code, APPELS, CONSTRUCTEURS, TABLES)
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
        r#"fn f() { let _ = ViewportBuilder::default().with_title("PlotYx"); }"#,
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
fn la_comparaison_refuse_un_litteral_hors_liste_et_un_compte_trop_haut_ou_trop_bas() {
    let sources = vec![(
        "t.rs".to_string(),
        "fn menu(ui: &mut Ui) { ui.label(\"un\"); ui.label(\"deux\"); }\nfn autre(ui: &mut Ui) { ui.label(\"trois\"); }\n"
            .to_string(),
    )];
    let zone = |nombre| Derogation {
        fichier: "t.rs",
        zone: "menus",
        ticket: "#42",
        fonctions: &["menu"],
        nombre,
    };
    // `autre` n'est dans aucune zone : écart. La zone « menus » est juste.
    let rapport = ecarts(&sources, &[zone(2)]);
    assert_eq!(rapport.len(), 1, "{rapport:?}");
    assert!(rapport[0].contains("`autre`"));
    // Compte trop bas dans la liste : un littéral de plus que toléré.
    assert!(ecarts(&sources, &[zone(1)])
        .iter()
        .any(|e| e.contains("de plus")));
    // Compte trop haut : la réduction n'a pas été reportée.
    assert!(ecarts(&sources, &[zone(3)])
        .iter()
        .any(|e| e.contains("abaisser")));
}

/// Les dépendances de traduction n'existent que dans `plotyx-app` : aucun
/// crate du dessous ne traduit (chantier #39). Les listes blanches de
/// `no_ui_dependency` en couvrent déjà une partie ; celui-ci couvre le reste du
/// workspace.
#[test]
fn les_dependances_de_traduction_restent_dans_plotyx_app() {
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
        if nom == "plotyx-app" {
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
