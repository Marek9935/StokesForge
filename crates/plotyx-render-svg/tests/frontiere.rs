//! Les frontières de l'export, vérifiées par le compilateur plutôt que par
//! la relecture — même discipline que
//! `plotyx-core/tests/no_ui_dependency.rs` et
//! `plotyx-plot/tests/no_ui_dependency.rs`.
//!
//! Trois propriétés, et la troisième est la plus importante des trois parce
//! qu'elle est la seule dont l'échec serait invisible.

use std::collections::{BTreeMap, BTreeSet};

/// Dépendances directes autorisées. Toute addition dans `Cargo.toml` doit
/// être ajoutée ici : c'est ce qui force une décision consciente plutôt
/// qu'un import qui se glisse.
const DEPENDANCES_DIRECTES: &[&str] = &[
    "plotyx-core",   // ResolvedPlot, Tree
    "plotyx-plot",   // PlotSpec
    "plotyx-render", // mesure de texte partagée + pipeline commun
    "plotters",      // feature svg_backend
    "plotters-backend",
    "thiserror",
];

/// Ce qu'un crate d'export ne doit jamais tirer, même de loin.
///
/// « Le renderer se teste **sans serveur graphique**, par l'export SVG »
/// (`roadmap-rendu.md` §9) : cette liste est ce qui rend la phrase vraie.
/// `epaint` n'y est pas — il est explicitement voulu (§4.4 : la surcharge de
/// `estimate_text_size` est obligatoire ici aussi), et c'est toute la
/// subtilité du découpage : la mesure d'egui sans la pile d'egui.
const INTERDITS: &[&str] = &[
    "egui",
    "eframe",
    "egui-winit",
    "egui-wgpu",
    "egui_extras",
    "egui_glow",
    "winit",
    "wgpu",
    "glow",
];

fn racine() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("le crate vit dans crates/<nom>")
        .to_path_buf()
}

fn lock() -> toml::Value {
    let path = racine().join("Cargo.lock");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} illisible : {err}", path.display()))
        .parse()
        .expect("Cargo.lock valide")
}

/// Le graphe `paquet → dépendances` tel que le verrou le fige.
///
/// Cargo range les dépendances de développement dans la même liste : la
/// vérification est donc plus stricte que « dépendances normales », ce qui
/// est exactement ce qu'on veut ici (un test qui tirerait egui ferait
/// mentir `cargo tree -p plotyx-render-svg`).
fn graphe(lock: &toml::Value) -> BTreeMap<String, Vec<String>> {
    let mut graphe = BTreeMap::new();
    let Some(paquets) = lock.get("package").and_then(|p| p.as_array()) else {
        panic!("Cargo.lock sans section [[package]]");
    };
    for paquet in paquets {
        let Some(nom) = paquet.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let deps = paquet
            .get("dependencies")
            .and_then(|d| d.as_array())
            .map(|liste| {
                liste
                    .iter()
                    .filter_map(|d| d.as_str())
                    // Une entrée de verrou peut être « nom version » ou
                    // « nom version (source) » quand plusieurs versions
                    // coexistent.
                    .map(|d| d.split_whitespace().next().unwrap_or(d).to_string())
                    .collect()
            })
            .unwrap_or_default();
        graphe.insert(nom.to_string(), deps);
    }
    graphe
}

fn fermeture(graphe: &BTreeMap<String, Vec<String>>, depart: &str) -> BTreeSet<String> {
    let mut vus = BTreeSet::new();
    let mut pile = vec![depart.to_string()];
    while let Some(nom) = pile.pop() {
        if !vus.insert(nom.clone()) {
            continue;
        }
        if let Some(deps) = graphe.get(&nom) {
            pile.extend(deps.iter().cloned());
        }
    }
    vus
}

#[test]
fn seules_les_dependances_directes_prevues_sont_declarees() {
    let manifest: toml::Value =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("Cargo.toml lisible")
            .parse()
            .expect("Cargo.toml valide");
    let deps = manifest
        .get("dependencies")
        .and_then(|d| d.as_table())
        .expect("section [dependencies]");
    for nom in deps.keys() {
        assert!(
            DEPENDANCES_DIRECTES.contains(&nom.as_str()),
            "dépendance `{nom}` non listée : l'export doit rester sans pile UI \
             (architecture-choisie.md §8)"
        );
    }
}

#[test]
fn ni_egui_ni_eframe_ni_winit_meme_transitivement() {
    let lock = lock();
    let graphe = graphe(&lock);
    let atteignables = fermeture(&graphe, "plotyx-render-svg");
    let fautifs: Vec<&str> = INTERDITS
        .iter()
        .copied()
        .filter(|nom| atteignables.contains(*nom))
        .collect();
    assert!(
        fautifs.is_empty(),
        "plotyx-render-svg atteint {fautifs:?} — l'export cesserait de se \
         tester sans serveur graphique, et la frontière du §8 serait perdue"
    );
}

#[test]
fn epaint_est_bien_present_et_en_un_seul_exemplaire() {
    let lock = lock();
    let graphe = graphe(&lock);
    assert!(
        fermeture(&graphe, "plotyx-render-svg").contains("epaint"),
        "plotyx-render-svg n'atteint plus epaint : il ne mesure donc plus \
         comme l'écran, et `estimate_text_size` est retombé sur \
         l'heuristique par octets (architecture-choisie.md §4.4)"
    );

    // **Le test qui compte.** Deux versions d'`epaint` dans le verrou — l'une
    // tirée par `egui` pour le backend écran, l'autre en direct pour
    // l'export — seraient deux jeux de polices, donc deux mesures, donc des
    // axes décalés. Et rien, absolument rien, ne le signalerait : le test
    // d'accord lui-même passerait, puisqu'il construit ses deux mesureurs
    // depuis le même crate compilé.
    let paquets = lock
        .get("package")
        .and_then(|p| p.as_array())
        .expect("section [[package]]");
    let versions: Vec<&str> = paquets
        .iter()
        .filter(|p| p.get("name").and_then(|n| n.as_str()) == Some("epaint"))
        .filter_map(|p| p.get("version").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(
        versions.len(),
        1,
        "epaint résolu en plusieurs versions ({versions:?}) : le backend \
         écran et l'export mesureraient avec des polices différentes sans \
         que rien ne le dise"
    );
}
