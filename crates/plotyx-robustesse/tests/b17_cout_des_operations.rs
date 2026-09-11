//! Batterie 17 — ce que coûtent les opérations courantes quand le projet
//! grandit.
//!
//! # Pourquoi compter des octets alloués plutôt que des millisecondes
//!
//! Un budget en temps dépend de la machine, de la charge et du profil de
//! compilation : mis en CI, il devient soit trop lâche pour dire quelque
//! chose, soit instable. Les **allocations**, elles, sont déterministes : un
//! allocateur global compteur (installé pour ce binaire de test seulement, et
//! par fil pour que les tests restent parallélisables) mesure exactement le
//! même nombre partout.
//!
//! La forme de l'assertion suit : jamais « moins de X octets », toujours « ce
//! coût ne doit pas croître avec la taille du projet ». C'est la propriété qui
//! compte, et c'est celle qui est fausse.
//!
//! Les mesures de temps correspondantes sont dans le rapport ; elles ne sont
//! pas ici, exprès.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use plotyx_core::{Column, ColumnType, NodeId, Project};

// -- allocateur compteur --------------------------------------------------

thread_local! {
    /// Octets demandés par le fil courant depuis son démarrage. `Cell<usize>`
    /// sans destructeur et initialisé en `const` : la variable ne peut pas
    /// elle-même allouer, ce qui interdit toute récursion dans l'allocateur.
    static OCTETS: Cell<usize> = const { Cell::new(0) };
}

struct Compteur;

// SAFETY: chaque méthode délègue à `System` sans changer ni le pointeur ni le
// `Layout` ; le comptage n'est qu'un effet de bord sur une cellule locale au
// fil, qui n'alloue jamais.
unsafe impl GlobalAlloc for Compteur {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with` : pendant la destruction des variables de fil, la
        // variable n'est plus accessible — on ne compte alors rien.
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

/// Octets alloués par `travail`, sur le fil courant.
fn octets_de<T>(travail: impl FnOnce() -> T) -> (T, usize) {
    let avant = OCTETS.with(Cell::get);
    let resultat = travail();
    (resultat, OCTETS.with(Cell::get) - avant)
}

// -- bancs ----------------------------------------------------------------

/// Un projet d'un classeur peuplé, plus `classeurs` classeurs vides — de quoi
/// faire varier la **taille totale** du projet sans toucher au chemin résolu.
fn projet(classeurs: usize) -> Project {
    let mut project = Project::new("D").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    for i in 0..5 {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("c{i}")),
                Column::new(ColumnType::Numeric),
            )
            .expect("colonne");
    }
    for i in 0..classeurs {
        project
            .add_empty_workbook(root, Some(&format!("B{i}")))
            .expect("classeur vide");
    }
    project
}

/// Ajoute `n` colonnes à une feuille neuve, avec ou sans nom imposé.
fn ajouter_colonnes(n: usize, nom_impose: bool) -> usize {
    let ((), octets) = octets_de(|| {
        let mut project = Project::new("D").expect("projet valide");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet: NodeId = project.resolve_path("[Book1]Sheet1").expect("feuille");
        for i in 0..n {
            let nom = nom_impose.then(|| format!("c{i}"));
            project
                .add_column(
                    sheet,
                    None,
                    nom.as_deref(),
                    Column::new(ColumnType::Numeric),
                )
                .expect("colonne");
        }
    });
    octets
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b17_lallocateur_compteur_mesure_bien_quelque_chose() {
    // Sans ce test, une régression qui casserait le comptage rendrait toutes
    // les autres assertions de cette batterie vraies pour la mauvaise raison.
    let (_, octets) = octets_de(|| vec![0u8; 4096]);
    assert!(
        octets >= 4096,
        "l'allocateur compteur doit voir une allocation de 4 kio (vu : {octets})"
    );
    let (_, rien) = octets_de(|| 1 + 1);
    assert_eq!(rien, 0, "une addition n'alloue pas");
}

#[test]
fn b17_ecrire_dans_une_colonne_ne_depend_pas_de_la_taille_du_projet() {
    use plotyx_core::ValueBlock;
    let mut petit = projet(10);
    let mut grand = projet(1000);
    let cible_petit = petit.resolve_path("[Book1]Sheet1!c0").expect("colonne");
    let cible_grand = grand.resolve_path("[Book1]Sheet1!c0").expect("colonne");

    let (_, a) = octets_de(|| petit.set_values(cible_petit, 0, ValueBlock::Float(vec![1.0; 64])));
    let (_, b) = octets_de(|| grand.set_values(cible_grand, 0, ValueBlock::Float(vec![1.0; 64])));
    println!(
        "écriture : {a} octets dans un projet de {} nœuds, {b} dans un de {} nœuds",
        petit.tree().len(),
        grand.tree().len()
    );
    assert!(
        b <= a * 4,
        "écrire dans une colonne coûte {b} octets sur un grand projet contre {a} sur un \
         petit : l'écriture doit dépendre du bloc écrit, pas de la taille du projet"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b17_resoudre_un_chemin_ne_depend_pas_de_la_taille_du_projet() {
    let petit = projet(10);
    let grand = projet(1000);
    let chemin = "[Book1]Sheet1!c0"; // strictement le même chemin, même profondeur

    // Un tour à blanc : la première résolution d'un projet neuf peut payer des
    // caches d'allocateur qui n'ont rien à voir avec la mesure.
    let _ = petit.resolve_path(chemin);
    let _ = grand.resolve_path(chemin);

    let (_, a) = octets_de(|| {
        for _ in 0..100 {
            petit.resolve_path(chemin).expect("chemin résolu");
        }
    });
    let (_, b) = octets_de(|| {
        for _ in 0..100 {
            grand.resolve_path(chemin).expect("chemin résolu");
        }
    });
    println!(
        "{} nœuds : {} octets par résolution — {} nœuds : {} octets par résolution",
        petit.tree().len(),
        a / 100,
        grand.tree().len(),
        b / 100
    );
    assert!(
        b <= a * 4,
        "le projet est {:.0}× plus gros et la résolution du même chemin alloue {:.0}× plus \
         ({} octets contre {}) : `Tree::find_window` appelle `windows()`, qui appelle \
         `walk_all()`, qui construit un `Vec` de TOUS les nœuds du projet — à chaque \
         appel. C'est l'opération que le pont Python fait avant chaque accès",
        grand.tree().len() as f64 / petit.tree().len() as f64,
        b as f64 / a as f64,
        b / 100,
        a / 100
    );
}

#[test]
fn b17_nommer_une_colonne_automatiquement_ne_coute_pas_plus_que_la_nommer() {
    let mut ecarts = Vec::new();
    for n in [100usize, 200, 400] {
        let auto = ajouter_colonnes(n, false);
        let explicite = ajouter_colonnes(n, true);
        let facteur = auto as f64 / explicite as f64;
        println!(
            "{n:>4} colonnes : nom automatique {auto:>9} octets, nom imposé {explicite:>7} \
             octets ({facteur:.0}×)"
        );
        ecarts.push((n, facteur));
    }
    // Le facteur double quand n double : c'est la signature du cube.
    let pire = ecarts.last().copied().unwrap_or((0, 0.0));
    assert!(
        pire.1 <= 4.0,
        "à {} colonnes, le nommage automatique coûte {:.0}× le nommage imposé, et ce \
         facteur double à chaque doublement de n ({ecarts:?}) : `next_column_name` \
         essaie les noms A, B, … dans l'ordre et compare chaque candidat à TOUS les \
         noms déjà pris — O(n²) par colonne, O(n³) pour la feuille. Mesuré hors test, \
         en `--release` : 4 000 colonnes à nom automatique prennent 17,8 s contre \
         0,19 s à nom imposé",
        pire.0,
        pire.1
    );
}
