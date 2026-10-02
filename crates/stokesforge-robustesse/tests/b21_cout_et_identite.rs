//! Batterie 21 — trois invariants que rien ne tient sur le chemin d'écriture.
//!
//! Même méthode que `b17` : on compte des **octets alloués**, pas des
//! millisecondes, pour que l'assertion dise « ce coût ne doit pas croître avec
//! X » et reste vraie sur n'importe quelle machine. Les mesures de temps
//! correspondantes sont dans le rapport.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use stokesforge_commands::{Command, History};
use stokesforge_core::{Column, ColumnType, CoreError, MixedCell, NodeId, Project, ValueBlock};

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
                    expected_version: None,
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

/// Une feuille d'une seule colonne numérique de `taille` valeurs, prête pour
/// une saisie tolérante (`Command::SetCellsPromoting`).
fn banc_promotion(taille: usize) -> (Project, History, NodeId) {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let valeurs: Vec<f64> = (0..taille).map(|i| i as f64).collect();
    let cible = project
        .add_column(sheet, None, Some("cible"), Column::from_f64(&valeurs))
        .expect("colonne cible");
    (project, History::new(), cible)
}

/// Octets alloués par la **première** saisie tolérante d'un texte dans une
/// colonne numérique de `taille` valeurs : chemin 3 de `SetCellsPromoting`
/// (la colonne n'est pas encore mixte) — celui qui promeut la colonne, donc
/// qui prend un instantané complet de son buffer, comme `SetColumnType`.
fn cout_dune_premiere_promotion(taille: usize) -> usize {
    let (mut project, mut history, cible) = banc_promotion(taille);
    let ((), octets) = octets_de(|| {
        history
            .execute(
                &mut project,
                Command::SetCellsPromoting {
                    column: cible,
                    start: 0,
                    values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                    expected_version: None,
                },
            )
            .expect("saisie tolérante acceptée")
    });
    octets
}

/// Octets alloués par une saisie tolérante dans une colonne de `taille`
/// cellules **déjà** mixte dès sa création — chemin 1/2 de
/// `SetCellsPromoting` (aucune promotion à faire), sur une colonne dont le
/// buffer n'est encore partagé avec aucune entrée d'historique. Isolé du
/// scénario « promotion puis nouvelle saisie » : y enchaîner deux
/// `SetCellsPromoting` sous le même `History` mesurerait un **autre** coût,
/// bien réel mais non pertinent ici — la copie-à-l'écriture que
/// `Arc::make_mut` déclenche parce que l'entrée de la promotion retient
/// encore le buffer courant (voir la doc de `History::apply_data`, chemin
/// 3) ; ce coût-là existe pour n'importe quelle colonne réécrite juste après
/// une entrée qui capture son buffer, mixte ou pas, et ce n'est pas la
/// propriété que ce test vérifie.
fn cout_dune_saisie_sur_colonne_deja_mixte(taille: usize) -> usize {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let cellules: Vec<MixedCell> = (0..taille).map(|i| MixedCell::Number(i as f64)).collect();
    let cible = project
        .add_column(sheet, None, Some("cible"), Column::from_mixed(&cellules))
        .expect("colonne déjà mixte");
    let mut history = History::new();
    let ((), octets) = octets_de(|| {
        history
            .execute(
                &mut project,
                Command::SetCellsPromoting {
                    column: cible,
                    start: 0,
                    values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                    expected_version: None,
                },
            )
            .expect("saisie tolérante acceptée sur colonne déjà mixte")
    });
    octets
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b21_promouvoir_une_colonne_deja_mixte_ne_paie_plus_linstantane() {
    // Doc de `Command::SetCellsPromoting` (`command.rs`) : le chemin qui
    // promeut une colonne (instantané complet, comme `SetColumnType`) « n'a
    // lieu qu'une fois par colonne, la deuxième saisie tolérante empruntant
    // le chemin 2 » — jusqu'ici affirmé seulement par un test fonctionnel de
    // `shell.rs` (undo restaure type et valeur), jamais mesuré. Ce test
    // mesure le coût de la promotion (chemin 3) sur une grande colonne, et
    // le compare à celui d'une saisie tolérante sur une colonne de la même
    // taille mais **déjà** mixte (chemins 1/2, pas de promotion à faire) :
    // le premier doit dépendre de la taille de la colonne, le second non.
    const TAILLE: usize = 200_000;

    let promotion = cout_dune_premiere_promotion(TAILLE);
    let deja_mixte = cout_dune_saisie_sur_colonne_deja_mixte(TAILLE);
    println!(
        "colonne de {TAILLE} valeurs : promotion (chemin 3) = {promotion} octets, saisie sur \
         colonne déjà mixte (chemins 1/2) = {deja_mixte} octets"
    );

    // Le chemin 3 (promotion) prend un instantané du buffer : de l'ordre de
    // `taille` octets, pas de quelques dizaines.
    assert!(
        promotion >= TAILLE,
        "la première saisie tolérante (chemin 3, promotion) devrait coûter au moins un \
         instantané du buffer ({TAILLE} valeurs) : {promotion} octets vus — si ce n'est plus \
         vrai, `SetCellsPromoting` a peut-être cessé de prendre l'instantané attendu par \
         `SetColumnType`, à vérifier avant de considérer ce chiffre comme une amélioration"
    );
    // Une colonne déjà mixte ne paie jamais l'instantané : quelques centaines
    // d'octets pour une cellule texte, pas 200 000, quelle que soit sa taille.
    assert!(
        deja_mixte < TAILLE / 10,
        "la saisie tolérante sur une colonne déjà mixte a coûté {deja_mixte} octets pour une \
         colonne de {TAILLE} valeurs : elle ne devrait pas dépendre de la taille de la colonne \
         — sans quoi le chemin coûteux de la promotion aurait lieu à chaque saisie, pas une \
         fois par colonne"
    );
}

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
// D3 (2ᵉ moitié) — corrigée : `History::execute` n'aiguille plus sur un seul
// chemin. Pour une commande de données, il ne clone plus `project.tree()`
// dans `before` : le libellé et le pointeur du buffer déjà présent (pour
// `just_orphaned_by_write`) sont capturés par simple emprunt, avant la
// mutation. `Arc::make_mut(&mut self.tree)` voit alors un compte de
// références de 1 et mute la `Tree` en place au lieu de la cloner. La
// garantie « aucune mutation partielle sur erreur » survit sans ce clone :
// les primitives de données de `Project` valident intégralement avant de
// muter (voir `Column::set_values`/`insert_rows`/`delete_rows`, testées en
// ce sens dans `stokesforge-core`), donc un rejet n'a jamais commencé à écrire.
// Les commandes structurelles, elles, continuent de cloner `before` — leur
// `Entry::Structural` en a un besoin réel (undo/redo par remplacement
// d'arbre). Voir `History::execute` et `History::apply_data`.
fn b21_le_cout_dune_ecriture_ne_depend_pas_du_nombre_de_colonnes() {
    let petit = cout_dune_ecriture(50, 0);
    let grand = cout_dune_ecriture(500, 0);
    println!(" 50 colonnes, historique vide : {petit:>9} octets par écriture");
    println!("500 colonnes, historique vide : {grand:>9} octets par écriture");
    assert!(
        grand <= petit * 3,
        "dix fois plus de colonnes, {:.1}× plus cher, pour écrire exactement la même \
         valeur dans la même colonne : régression de D3 (2ᵉ moitié) — `History::execute` \
         garde de nouveau `before` (un clone de `project.tree()`) vivant pendant \
         l'application d'une commande de données, ce qui force `Arc::make_mut` à cloner la \
         `Tree` entière plutôt que de muter en place — voir la doc de `History::execute` et \
         `History::apply_data`",
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
