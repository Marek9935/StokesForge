//! Batterie 21 — trois invariants que rien ne tient sur le chemin d'écriture.
//!
//! Même méthode que `b17` : on compte des **octets alloués**, pas des
//! millisecondes, pour que l'assertion dise « ce coût ne doit pas croître avec
//! X » et reste vraie sur n'importe quelle machine. Les mesures de temps
//! correspondantes sont dans le rapport.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use plotyx_commands::{Command, History};
use plotyx_core::{Column, ColumnType, CoreError, NodeId, Project, ValueBlock};

// -- allocateur compteur (même schéma que b17) ----------------------------

thread_local! {
    static OCTETS: Cell<usize> = const { Cell::new(0) };
}

struct Compteur;

// SAFETY: délégation à `System` sans changer pointeur ni `Layout` ; le
// comptage n'écrit que dans une cellule locale au fil, qui n'alloue jamais.
unsafe impl GlobalAlloc for Compteur {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = OCTETS.try_with(|octets| octets.set(octets.get() + layout.size()));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, taille: usize) -> *mut u8 {
        let _ = OCTETS.try_with(|octets| octets.set(octets.get() + taille));
        unsafe { System.realloc(ptr, layout, taille) }
    }
}

#[global_allocator]
static ALLOCATEUR: Compteur = Compteur;

fn octets_de<T>(travail: impl FnOnce() -> T) -> (T, usize) {
    let avant = OCTETS.with(Cell::get);
    let resultat = travail();
    (resultat, OCTETS.with(Cell::get) - avant)
}

// -- banc -----------------------------------------------------------------

/// Une feuille de `colonnes` colonnes, et un historique rempli de `entrees`
/// entrées **structurelles** (chacune retient deux arbres, donc toutes les
/// colonnes du projet).
fn banc(colonnes: usize, entrees: usize) -> (Project, History, NodeId) {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let cible = project
        .add_column(sheet, None, Some("cible"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne cible");
    for i in 0..colonnes {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("c{i}")),
                Column::from_f64(&[1.0, 2.0]),
            )
            .expect("colonne");
    }
    let mut history = History::new();
    for i in 0..entrees {
        history
            .execute(
                &mut project,
                Command::AddColumn {
                    sheet,
                    index: None,
                    name: Some(format!("s{i}")),
                    column: Column::new(ColumnType::Numeric),
                },
            )
            .expect("entrée structurelle");
    }
    (project, history, cible)
}

/// Octets alloués par une écriture d'une valeur, dans ce banc.
fn cout_dune_ecriture(colonnes: usize, entrees: usize) -> usize {
    let (mut project, mut history, cible) = banc(colonnes, entrees);
    let ecrire = |project: &mut Project, history: &mut History, valeur: f64| {
        history
            .execute(
                project,
                Command::SetCells {
                    column: cible,
                    start: 0,
                    values: ValueBlock::Float(vec![valeur]),
                },
            )
            .expect("écriture acceptée");
    };
    ecrire(&mut project, &mut history, 3.0); // tour à blanc
    let ((), octets) = octets_de(|| {
        for _ in 0..10 {
            ecrire(&mut project, &mut history, 4.0);
        }
    });
    octets / 10
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b21_les_noms_reserves_par_la_grammaire_restent_refuses() {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    for nom in ["", "  ", " A", "A ", "A!B", "[A]", "a/b", "A]"] {
        assert!(
            matches!(
                project.add_folder(root, nom),
                Err(CoreError::InvalidName(_))
            ),
            "attendu InvalidName pour {nom:?}"
        );
    }
}

#[test]
fn b21_le_cout_dune_ecriture_ne_depend_pas_de_la_longueur_de_la_colonne() {
    // Le contre-exemple utile : ce qui *devrait* faire varier le coût le fait,
    // et proportionnellement. C'est ce qui rend l'assertion des reproductions
    // lisible — le coût suit ce qu'on écrit, pas ce qui l'entoure.
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let colonne = project
        .add_column(sheet, None, Some("A"), Column::new(ColumnType::Numeric))
        .expect("colonne");
    let (_, petit) = octets_de(|| {
        project
            .set_values(colonne, 0, ValueBlock::Float(vec![1.0; 10]))
            .expect("écriture")
    });
    let (_, grand) = octets_de(|| {
        project
            .set_values(colonne, 0, ValueBlock::Float(vec![1.0; 10_000]))
            .expect("écriture")
    });
    println!("écrire 10 valeurs : {petit} octets ; 10 000 valeurs : {grand} octets");
    assert!(grand > petit, "écrire mille fois plus doit coûter plus");
}

// -- reproductions --------------------------------------------------------

#[test]
fn b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique() {
    let sans = cout_dune_ecriture(500, 0);
    let avec = cout_dune_ecriture(500, 100);
    println!("500 colonnes, historique vide       : {sans:>9} octets par écriture");
    println!("500 colonnes, 100 entrées empilées  : {avec:>9} octets par écriture");
    println!(
        "mesures complémentaires (hors test) :\n  \
            50 colonnes,   0 entrées :      3 839 octets\n  \
            50 colonnes, 100 entrées :     45 533 octets\n  \
           500 colonnes,   0 entrées :     37 503 octets\n  \
           500 colonnes, 100 entrées :    159 481 octets\n  \
         2 000 colonnes, 100 entrées :    357 145 octets — pour écrire UN nombre"
    );
    assert!(
        avec <= sans * 2,
        "remplir l'historique multiplie par {:.1} le coût d'une écriture qui ne le \
         concerne pas : `History::evict` appelle `recompute_retained_bytes` à **chaque** \
         empilement, qui refait l'union des empreintes de toutes les entrées des deux \
         piles puis soustrait les buffers de l'arbre courant — lequel est parcouru \
         entièrement. Le coût d'une frappe dans la grille suit donc la taille du projet \
         ET la profondeur de l'historique",
        avec as f64 / sans as f64
    );
}

#[test]
#[ignore = "reproduction D3 (diagnostic révisé) — le passage en O(entrées) de History::evict \
            (voir b21_le_cout_dune_ecriture_ne_depend_pas_de_lhistorique, désormais vert) a \
            supprimé l'appel à `current_tree_pointers` sur ce chemin, mais le coût par colonne \
            persiste : `History::execute` clone `project.tree()` dans `before` AVANT d'appliquer \
            la commande et le garde vivant pendant toute son application (pour le label, et pour \
            `Entry::Structural`/`just_orphaned_by_write`) ; `Arc::make_mut(&mut self.tree)`, \
            appelé par toute primitive de mutation (`Project::set_values`, etc.), voit alors un \
            compte de références de 2 et clone la `Tree` ENTIÈRE — sa HashMap de TOUS les nœuds \
            — plutôt que de muter en place. Correct pour une commande structurelle (l'Entry en a \
            réellement besoin), inutile pour une commande de données (SetCells, InsertRows, ...), \
            qui ne retient jamais l'arbre — seulement son delta. Un correctif ne tenant plus dans \
            « quelques lignes » : il faut que `History::execute` cesse de cloner `before` pour les \
            commandes de données (calculer le label par un simple emprunt, capturer le pointeur du \
            buffer déjà présent pour `just_orphaned_by_write` avant la mutation plutôt qu'après), \
            sans affaiblir la garantie « aucune mutation partielle sur erreur » que `project.\
            replace_tree(before)` offre aujourd'hui gratuitement. Non tenté ici : c'est un \
            changement du chemin d'exécution central de l'annulation, à valider à part"]
fn b21_le_cout_dune_ecriture_ne_depend_pas_du_nombre_de_colonnes() {
    let petit = cout_dune_ecriture(50, 0);
    let grand = cout_dune_ecriture(500, 0);
    println!(" 50 colonnes, historique vide : {petit:>9} octets par écriture");
    println!("500 colonnes, historique vide : {grand:>9} octets par écriture");
    assert!(
        grand <= petit * 3,
        "dix fois plus de colonnes, {:.1}× plus cher, pour écrire exactement la même \
         valeur dans la même colonne : `History::execute` garde `before` (un clone de \
         `project.tree()`) vivant pendant toute l'application de la commande, ce qui force \
         `Arc::make_mut` à cloner la `Tree` entière plutôt que de muter en place — voir le \
         diagnostic complet sur `#[ignore]` de ce test",
        grand as f64 / petit as f64
    );
}

#[test]
fn b21_les_noms_de_controle_sont_refuses() {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    let mut acceptes = Vec::new();
    for (etiquette, nom) in [
        ("saut de ligne", "A\nB".to_string()),
        ("tabulation", "A\tB".to_string()),
        ("octet nul", "A\u{0}B".to_string()),
        ("inversion de sens (RLO)", "A\u{202e}B".to_string()),
        ("espace de largeur nulle", "A\u{200b}B".to_string()),
        ("cent mille caractères", "x".repeat(100_000)),
    ] {
        if let Ok(id) = project.add_folder(root, &nom) {
            let chemin = project.path_of(id).unwrap_or_default();
            acceptes.push((etiquette, nom.chars().count(), chemin.chars().count()));
        }
    }
    println!("acceptés (étiquette, longueur du nom, longueur du chemin) : {acceptes:?}");
    assert!(
        acceptes.is_empty(),
        "ces noms sont acceptés, persistés, et ressortent tels quels de `path_of` : \
         un nom porteur d'un octet nul ou d'un saut de ligne casse tout affichage \
         tabulaire et tout message d'erreur qui le cite ; une marque d'inversion de \
         sens retourne visuellement un chemin entier ; et aucune borne de longueur \
         n'existe, ni ici ni sur `unit`, `long_name`, `comment`, `display_format` ou le \
         texte d'une note"
    );
}

#[test]
fn b21_deux_projets_nattribuent_jamais_le_meme_identifiant() {
    // B9 (`guide-de-correction-robustesse.md` §F2) — corrigée en retirant
    // `#[derive(Clone)]` de `Project` (rien dans le dépôt n'en avait besoin :
    // seul `Arc<Tree>` doit être clonable, gratuitement). La reproduction
    // d'origine ne peut plus s'écrire du tout : `Project::clone()` ne
    // compile plus, ce qui est une preuve plus forte qu'un test qui passe.
    // Cette fonction garde la place (§1 du guide : on ne supprime jamais une
    // reproduction, on réécrit son assertion) et vérifie ce que la
    // duplication de l'allocateur aurait pu casser : des identifiants
    // strictement croissants, jamais réattribués, sur la vie d'un projet.
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    let mut deja_vus = Vec::new();
    for nom in ["M", "N", "O", "P"] {
        let id = project.add_folder(root, nom).expect("dossier");
        assert!(
            !deja_vus.contains(&id),
            "identifiant {id} réattribué parmi {deja_vus:?}"
        );
        deja_vus.push(id);
    }
}
