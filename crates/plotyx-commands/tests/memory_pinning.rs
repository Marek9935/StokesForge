//! Mesure : combien de versions d'un buffer de colonne l'historique
//! d'annulation hybride garde-t-il vivantes, et combien de mémoire ça pèse ?
//!
//! # Le doute à trancher
//!
//! `History` (`src/history.rs`) retient une modification de **structure**
//! (renommer, déplacer, ajouter, supprimer un nœud) comme un couple
//! d'instantanés `Arc<Tree>` — cloner l'arbre est bon marché (une table de
//! paires `(NodeId, Arc<Node>)`, pas les nœuds). Une modification de
//! **données** (cellules d'une colonne) est retenue comme un delta : les
//! seules valeurs écrites, minuscules. La documentation du crate affirme que
//! les charges utiles retenues sont « à l'identique » dans les deux schémas.
//!
//! C'est vrai d'une colonne **supprimée** : les deux schémas la retiennent
//! (scénario 6). Ce n'est peut-être **pas** vrai d'une colonne **modifiée**
//! après coup : un `Arc<Tree>` capturé par une entrée structurelle retient
//! tous ses `Arc<Node>`, donc toutes les colonnes, donc leurs buffers
//! `Arc<Vec<f64>>`. Si cette colonne est ensuite écrite, la copie-à-l'écriture
//! (`Arc::make_mut`, dans `Column::set_values`) alloue un nouveau buffer — et
//! l'ancien reste vivant tant que l'entrée structurelle est sur la pile,
//! *même si personne ne le lit plus jamais*. En alternant structure et
//! écritures dans une grosse colonne, chaque entrée structurelle risque
//! d'épingler une version complète du buffer.
//!
//! **Un chiffre élevé** (des dizaines de méga-octets de buffers épinglés pour
//! quelques dizaines d'opérations) confirmerait que le choix hybride a un
//! coût mémoire réel et grandissant avec l'usage — un argument pour revoir la
//! limite d'historique, ou pour que les entrées structurelles ne capturent
//! pas les colonnes elles-mêmes. **Un chiffre proche de zéro** (des versions
//! qui ne dépassent jamais le nombre de colonnes vivantes) infirmerait
//! l'hypothèse et confirmerait la documentation actuelle.
//!
//! # Méthode : compter les buffers vivants, pas les octets alloués
//!
//! La mémoire du processus (RSS) est illisible ici : bruit de l'allocateur,
//! pages non rendues au système. On mesure à la place *exactement ce qui est
//! en jeu*, de deux façons indépendantes :
//!
//! - **Mesure 1** — avant chaque écriture, on relève l'adresse du buffer
//!   (`Arc::as_ptr`) et un `std::sync::Weak` obtenu par `Arc::downgrade`. À la
//!   fin d'un scénario, on compte combien de ces `Weak` sont encore
//!   promouvables (`upgrade().is_some()`) : c'est le nombre de versions
//!   *encore retenues*. Un `Weak` ne retient rien — c'est le point capital de
//!   la méthode : l'observation ne fausse jamais la mesure qu'elle sert à
//!   faire.
//! - **Mesure 2** — un `#[global_allocator]` de test enveloppe
//!   `std::alloc::System` et compte les octets actuellement vivants
//!   (incrémentés à `alloc`, décrémentés à `dealloc`, ajustés à `realloc`).
//!   Relevé avant et après chaque scénario, c'est une seconde vue,
//!   indépendante de la première ; si les deux concordent, le chiffre est
//!   solide.
//!
//! # Ce que les scénarios n'affirment pas
//!
//! Sauf au scénario 6 (où la survie du buffer d'une colonne supprimée tant
//! que son entrée est sur la pile est structurellement garantie par le code
//! — sans ça, pas d'annulation possible — et où sa libération après éviction
//! l'est tout autant), ces tests **affichent** des chiffres sans imposer de
//! seuil. La mesure sert à décider une architecture, pas à devenir un test
//! fragile qui casse au premier réglage de constante.
//!
//! # Organisation
//!
//! Chaque scénario est une fonction ordinaire qui construit son état, mesure,
//! imprime un compte rendu lisible, et rend une [`Row`]. Un `#[test]` dédié
//! par scénario permet de le rejouer isolément ; [`summary_table_all_scenarios`]
//! les rejoue tous dans l'ordre et imprime le tableau récapitulatif final —
//! c'est lui la source de vérité pour le tableau de chiffres. Un
//! `static MEASURE_LOCK` sérialise tous les tests de ce fichier : la mesure 2
//! est un compteur **par processus**, donc par binaire de test ; sans
//! sérialisation, deux scénarios `--ignored` tournant en parallèle sur les
//! threads par défaut du harnais fausseraient l'un l'autre.
//!
//! Ignorés (`#[ignore]`) : ces tests allouent des centaines de méga-octets et
//! ne doivent pas alourdir `cargo test`. Lancement explicite :
//! `cargo test -p plotyx-commands --release --test memory_pinning -- --ignored --nocapture`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use plotyx_commands::{Command, History};
use plotyx_core::{Column, Node, NodeId, Project, ValueBlock};

// ===========================================================================
// Allocateur compteur — mesure 2
// ===========================================================================

/// Octets actuellement vivants (incrémenté à `alloc`/`alloc_zeroed`,
/// décrémenté à `dealloc`, ajusté à `realloc`).
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Octets alloués depuis le départ du processus, jamais décrémenté : sert à
/// distinguer un pic transitoire (alloué puis libéré aussitôt) d'un chiffre
/// réellement épinglé — voir le commentaire du scénario 1.
static TOTAL_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// Enveloppe `System` : ne change aucun comportement d'allocation, ne fait
/// qu'observer les tailles qui transitent.
struct CountingAllocator;

// Sûr : chaque méthode ne fait que déléguer à `System` (dont les garanties de
// sécurité sont celles du système standard) et incrémenter/décrémenter des
// compteurs atomiques à partir des tailles réellement passées/rendues — aucun
// pointeur n'est déréférencé ni construit ici.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
            TOTAL_ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
            TOTAL_ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            let old_size = layout.size();
            if new_size > old_size {
                let grew = new_size - old_size;
                LIVE_BYTES.fetch_add(grew, Ordering::SeqCst);
                TOTAL_ALLOCATED.fetch_add(grew, Ordering::SeqCst);
            } else if new_size < old_size {
                LIVE_BYTES.fetch_sub(old_size - new_size, Ordering::SeqCst);
            }
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn live_bytes() -> usize {
    LIVE_BYTES.load(Ordering::SeqCst)
}

fn total_allocated() -> usize {
    TOTAL_ALLOCATED.load(Ordering::SeqCst)
}

/// Sérialise tous les tests de ce fichier : la mesure 2 (compteurs
/// atomiques du processus) n'a de sens que si un seul scénario s'exécute à
/// la fois. Le poison est récupéré plutôt que propagé : un scénario qui
/// paniquerait (assertion du scénario 6, par exemple) ne doit pas faire
/// échouer en cascade tous les autres.
static MEASURE_LOCK: Mutex<()> = Mutex::new(());

fn measure_lock() -> std::sync::MutexGuard<'static, ()> {
    MEASURE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ===========================================================================
// Empreinte d'une version de buffer — mesure 1
// ===========================================================================

/// Empreinte du buffer `f64` d'une colonne à un instant donné : son adresse
/// (pour regrouper les empreintes qui désignent la même version) et un
/// `Weak` qui dira, plus tard, si CETTE version précise est encore vivante.
/// Un `Weak` ne retient rien : l'observer ne fausse jamais la mesure qu'il
/// sert à faire.
struct BufferFingerprint {
    ptr: usize,
    weak: Weak<Vec<f64>>,
    len: usize,
}

/// Empreinte du buffer de `column` dans l'état courant de `project`.
fn fingerprint(project: &Project, column: NodeId) -> BufferFingerprint {
    let arc = project
        .tree()
        .node(column)
        .and_then(Node::as_column)
        .and_then(Column::float_values)
        .cloned()
        .expect("colonne numérique existante");
    BufferFingerprint {
        ptr: Arc::as_ptr(&arc) as usize,
        weak: Arc::downgrade(&arc),
        len: arc.len(),
    }
}

/// Parmi des empreintes prises à des instants divers (la même version peut
/// avoir été observée plusieurs fois), combien de versions **distinctes**
/// sont encore vivantes à l'instant de l'appel, et combien d'octets ça
/// représente.
fn live_versions(fingerprints: &[BufferFingerprint]) -> (usize, usize) {
    let mut seen = HashSet::new();
    let mut versions = 0usize;
    let mut bytes = 0usize;
    for fp in fingerprints {
        if !seen.insert(fp.ptr) {
            continue; // même version déjà comptée
        }
        if fp.weak.upgrade().is_some() {
            versions += 1;
            bytes += fp.len * std::mem::size_of::<f64>();
        }
    }
    (versions, bytes)
}

/// Somme de [`live_versions`] sur la colonne cible et sur les colonnes
/// témoins.
fn total_live(target: &[BufferFingerprint], others: &[Vec<BufferFingerprint>]) -> (usize, usize) {
    let (mut versions, mut bytes) = live_versions(target);
    for fingerprints in others {
        let (v, b) = live_versions(fingerprints);
        versions += v;
        bytes += b;
    }
    (versions, bytes)
}

/// Comme [`live_versions`], mais **exclut** la version dont l'adresse est
/// `current_ptr` : c'est le pendant, mesuré indépendamment (par des `Weak`
/// promouvables, jamais en interrogeant `History`), de ce que
/// `History::retained_bytes` calcule par soustraction du footprint de
/// l'arbre courant — un buffer que l'arbre courant utilise encore n'est pas
/// un coût de l'historique, il serait là de toute façon. Sert au scénario 7
/// à comparer les deux mesures sur la même base (« combien de versions
/// *en trop* », pas « combien de versions en tout »).
fn historical_only_live(fingerprints: &[BufferFingerprint], current_ptr: usize) -> (usize, usize) {
    let mut seen = HashSet::new();
    let mut versions = 0usize;
    let mut bytes = 0usize;
    for fp in fingerprints {
        if fp.ptr == current_ptr {
            continue; // la version courante n'est pas un coût de l'historique
        }
        if !seen.insert(fp.ptr) {
            continue; // même version déjà comptée
        }
        if fp.weak.upgrade().is_some() {
            versions += 1;
            bytes += fp.len * std::mem::size_of::<f64>();
        }
    }
    (versions, bytes)
}

/// Octets « historique seul » (voir [`historical_only_live`]) pour la cible
/// et l'ensemble des témoins d'un [`AlternatingRun`] — la mesure
/// indépendante à comparer à `run.history.retained_bytes()`.
fn independent_historical_bytes(run: &AlternatingRun) -> usize {
    let current_target = fingerprint(&run.project, run.target).ptr;
    let (_, mut bytes) = historical_only_live(&run.target_fingerprints, current_target);
    for (fingerprints, &column) in run.other_fingerprints.iter().zip(run.others.iter()) {
        let current = fingerprint(&run.project, column).ptr;
        let (_, b) = historical_only_live(fingerprints, current);
        bytes += b;
    }
    bytes
}

// ===========================================================================
// Fixture : une feuille de 5 colonnes de 1 000 000 f64
// ===========================================================================

/// Colonne de référence : 1 000 000 `f64`, ~7,6 Mio (8 000 000 octets).
const ROWS: usize = 1_000_000;

/// Nombre de couples [renommer, écrire] joués dans le scénario alterné.
const WRITE_ITERATIONS: usize = 20;

/// Limite d'historique du scénario 3 : assez petite pour voir si elle borne
/// vraiment le nombre de versions vivantes.
const BOUNDED_HISTORY_LIMIT: usize = 5;

/// Limite en octets du scénario 7 : 64 Mio, assez petite pour observer si
/// `History::with_limits` borne réellement le total retenu à cette hauteur —
/// et à quel ordre de grandeur (environ 8 versions de la colonne de
/// référence, 64 / 7,6 ≈ 8,4, contre 21 sans elle au scénario 2).
const BYTE_BOUNDED_LIMIT: usize = 64 * 1024 * 1024;

/// Taille en octets d'une colonne de référence (`ROWS` flottants).
fn column_bytes() -> usize {
    ROWS * std::mem::size_of::<f64>()
}

/// `Book1/Sheet1` avec 5 colonnes de `ROWS` flottants : `target`, celle qu'on
/// va renommer et écrire, et 4 colonnes témoins jamais touchées — pour voir
/// si elles sont épinglées elles aussi, ou seulement celle qu'on écrit.
struct Fixture {
    project: Project,
    sheet: NodeId,
    target: NodeId,
    others: Vec<NodeId>,
}

fn build_fixture() -> Fixture {
    let mut project = Project::new("MesureMemoire").expect("projet de mesure");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille Sheet1");

    let values: Vec<f64> = (0..ROWS).map(|row| row as f64).collect();
    let target = project
        .add_column(sheet, None, Some("Target"), Column::from_f64(&values))
        .expect("colonne cible (1 000 000 f64)");
    let others: Vec<NodeId> = ["Witness1", "Witness2", "Witness3", "Witness4"]
        .into_iter()
        .map(|name| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&values))
                .expect("colonne témoin")
        })
        .collect();

    Fixture {
        project,
        sheet,
        target,
        others,
    }
}

// ===========================================================================
// Pilote partagé : structure et écritures alternées sous `History`
// ===========================================================================

/// État d'une exécution du scénario alterné : le projet et l'historique
/// (pour que l'appelant puisse continuer à s'en servir — le scénario 4
/// annule après coup), les empreintes collectées en cours de route, et les
/// identifiants des colonnes cible/témoins (pour retrouver, après coup, la
/// version *courante* de chacune — nécessaire à la mesure indépendante « à
/// combien concorde `History::retained_bytes` » du scénario 7).
struct AlternatingRun {
    project: Project,
    history: History,
    target: NodeId,
    others: Vec<NodeId>,
    target_fingerprints: Vec<BufferFingerprint>,
    other_fingerprints: Vec<Vec<BufferFingerprint>>,
}

/// `WRITE_ITERATIONS` fois [renommer la colonne cible, écrire une cellule
/// dans la cible], sous `History` limité à `limit` entrées (`0` =
/// illimité), à la limite en octets par défaut de `History` (512 Mio — sans
/// effet à cette échelle, voir le scénario 7 pour une limite en octets
/// serrée). Empreinte le buffer cible juste **avant** chaque écriture —
/// c'est cette version-là qu'un renommage ultérieur risque d'épingler dans
/// son instantané `before` — et les colonnes témoins à chaque itération,
/// pour vérifier qu'elles ne gagnent jamais de version supplémentaire.
fn run_alternating(limit: usize) -> AlternatingRun {
    run_alternating_history(History::with_limit(limit))
}

/// Comme [`run_alternating`], mais avec les deux limites de `History`
/// explicites — c'est ce que le scénario 7 utilise pour observer si la
/// limite en octets borne réellement le total retenu.
fn run_alternating_with_limits(limit: usize, byte_limit: usize) -> AlternatingRun {
    run_alternating_history(History::with_limits(limit, byte_limit))
}

/// Pilote commun à [`run_alternating`] et [`run_alternating_with_limits`] :
/// même déroulé, `History` déjà construit avec les limites voulues.
fn run_alternating_history(mut history: History) -> AlternatingRun {
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        history
            .execute(
                &mut project,
                Command::Rename {
                    id: target,
                    name: format!("Target{i}"),
                },
            )
            .expect("renommage accepté");
        history
            .execute(
                &mut project,
                Command::SetCells {
                    column: target,
                    start: i,
                    values: ValueBlock::Float(vec![i as f64]),
                },
            )
            .expect("écriture acceptée");
        for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
            fingerprints.push(fingerprint(&project, column));
        }
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    AlternatingRun {
        project,
        history,
        target,
        others,
        target_fingerprints,
        other_fingerprints,
    }
}

// ===========================================================================
// Tableau récapitulatif
// ===========================================================================

/// Une ligne du tableau récapitulatif final.
struct Row {
    name: &'static str,
    /// Versions vivantes du buffer, mesure 1 (cible + témoins).
    live_versions: usize,
    /// Octets épinglés, mesure 1 (cible + témoins).
    pinned_bytes: usize,
    /// Delta d'octets vivants selon l'allocateur, mesure 2, du début à la fin
    /// du scénario (projet et historique encore vivants au relevé final).
    allocator_bytes: usize,
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn print_row(row: &Row) {
    println!(
        "  => {} : {} version(s) vivante(s), {:.1} Mo épinglés (mesure 1), {:.1} Mo vivants Δ (mesure 2)",
        row.name,
        row.live_versions,
        mb(row.pinned_bytes),
        mb(row.allocator_bytes)
    );
}

fn print_summary_table(rows: &[Row]) {
    let baseline = rows
        .iter()
        .find(|row| row.name.starts_with('5'))
        .map(|row| row.pinned_bytes);

    println!();
    println!(
        "colonne de référence : {ROWS} f64 ({:.1} Mo) ; {WRITE_ITERATIONS} itérations par scénario alterné",
        mb(ROWS * std::mem::size_of::<f64>())
    );
    println!(
        "================================ Tableau récapitulatif ================================"
    );
    println!(
        "{:<46} {:>9} {:>15} {:>15} {:>11}",
        "Scénario", "Versions", "Mesure1 (Mo)", "Mesure2 (Mo)", "Rapport/S5"
    );
    for row in rows {
        // Le rapport au scénario 5 n'a de sens que pour les scénarios 1 à 5
        // (même construction : cible + 4 témoins, mêmes 20 itérations). Le
        // scénario 6 mesure autre chose (une colonne supprimée) et n'est pas
        // comparable terme à terme.
        let comparable = row
            .name
            .chars()
            .next()
            .is_some_and(|c| ('1'..='5').contains(&c));
        let ratio = match (comparable, baseline) {
            (true, Some(base)) if base > 0 => {
                format!("{:.2}x", row.pinned_bytes as f64 / base as f64)
            }
            _ => "—".to_string(),
        };
        println!(
            "{:<46} {:>9} {:>15.1} {:>15.1} {:>11}",
            row.name,
            row.live_versions,
            mb(row.pinned_bytes),
            mb(row.allocator_bytes),
            ratio
        );
    }
    println!(
        "========================================================================================="
    );
}

// ===========================================================================
// Scénario 1 — écritures seules, sans opération de structure
// ===========================================================================

/// 20 écritures d'une seule cellule dans la colonne cible, sans la moindre
/// opération de structure. Hypothèse : une entrée `Data` ne retient jamais de
/// buffer entier (seulement les valeurs écrites, minuscules) — aucune
/// ancienne version du buffer ne devrait donc rester épinglée à la fin.
fn scenario_1_writes_only() -> Row {
    let before = live_bytes();
    let total_before = total_allocated();
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut history = History::with_limit(0);
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        history
            .execute(
                &mut project,
                Command::SetCells {
                    column: target,
                    start: i,
                    values: ValueBlock::Float(vec![i as f64]),
                },
            )
            .expect("écriture acceptée");
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    let (target_versions, _) = live_versions(&target_fingerprints);
    let (total_versions, pinned_bytes) = total_live(&target_fingerprints, &other_fingerprints);
    let after = live_bytes();
    let total_after = total_allocated();

    println!("\n== Scénario 1 — écritures seules (pas de structure) ==");
    println!(
        "  versions vivantes du buffer cible : {target_versions} (rien ne devrait rester épinglé au-delà de la version courante)"
    );
    println!(
        "  total (cible + 4 témoins) : {total_versions} versions, {:.1} Mo épinglés (mesure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2) ; {:.1} Mo alloués au total durant le scénario",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before)),
        mb(total_after.saturating_sub(total_before))
    );
    println!(
        "  note : même sans aucune entrée structurelle, `History::execute` clone l'`Arc<Tree>` \
         courant avant d'appliquer la commande (garantie de restauration en cas d'échec) ; ce clone \
         transitoire force une copie-à-l'écriture du buffer à CHAQUE écriture (visible dans le total \
         alloué), même si rien n'en reste épinglé à la fin."
    );

    Row {
        name: "1. Écritures seules",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scénario 2 — structure et écritures alternées (le scénario suspecté)
// ===========================================================================

/// `WRITE_ITERATIONS` fois [renommer la cible, écrire une cellule dans la
/// cible], historique illimité. Hypothèse : chaque renommage capture, dans
/// son instantané `before`, la version du buffer que l'écriture précédente
/// venait de créer — elle reste épinglée tant que cette entrée structurelle
/// est sur la pile.
fn scenario_2_alternating() -> Row {
    let before = live_bytes();
    let run = run_alternating(0);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let witness_versions: Vec<usize> = run
        .other_fingerprints
        .iter()
        .map(|fingerprints| live_versions(fingerprints).0)
        .collect();
    let after = live_bytes();

    println!("\n== Scénario 2 — structure et écritures alternées (historique illimité) ==");
    println!(
        "  versions vivantes du buffer cible : {target_versions} ({:.1} Mo, mesure 1)",
        mb(target_bytes)
    );
    println!(
        "  colonnes témoins (jamais écrites) : {witness_versions:?} version(s) vivante(s) chacune (attendu : 1 partout)"
    );
    println!(
        "  total (cible + témoins) : {total_versions} versions, {:.1} Mo épinglés (mesure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "2. Structure + écritures alternées",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scénario 3 — même scénario, historique borné
// ===========================================================================

/// Même alternance que le scénario 2, mais `History::with_limit(BOUNDED_HISTORY_LIMIT)` :
/// la limite d'historique protège-t-elle réellement, et à quelle hauteur ?
fn scenario_3_bounded_history() -> Row {
    let before = live_bytes();
    let run = run_alternating(BOUNDED_HISTORY_LIMIT);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let after = live_bytes();

    println!(
        "\n== Scénario 3 — même scénario, historique borné à {BOUNDED_HISTORY_LIMIT} entrées =="
    );
    println!(
        "  versions vivantes du buffer cible : {target_versions} ({:.1} Mo, mesure 1)",
        mb(target_bytes)
    );
    println!(
        "  total (cible + témoins) : {total_versions} versions, {:.1} Mo épinglés (mesure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "3. Même scénario, historique à 5",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scénario 4 — effet de l'annulation
// ===========================================================================

/// Rejoue le scénario 2, puis annule tout l'historique (`undo` jusqu'à
/// `Ok(false)`), et recompte les **mêmes** empreintes prises pendant la
/// construction.
///
/// Deux affirmations, pas une seule — distinction qu'une première version de
/// ce test manquait, et que la mesure elle-même a mise au jour (voir le
/// rapport de tâche) :
///
/// - les `WRITE_ITERATIONS` empreintes prises **avant chaque écriture**
///   désignent chacune une version qu'un renommage *suivant* capture dans son
///   instantané `before`. Leur survie après annulation est structurellement
///   garantie : `History::undo` retire l'entrée de `undo` et la pousse sur
///   `redo` (`Vec::pop` puis `Vec::push`), il ne la détruit jamais ;
/// - la toute **dernière** empreinte, prise après la boucle, désigne l'état
///   courant au moment de la mesure — mais aucun renommage ne suit pour la
///   capturer dans une entrée. Rien ne la retient : annuler l'opération qui
///   l'a produite l'abandonne, et elle doit disparaître. Ce n'est pas une
///   fuite du mécanisme de rétablissement, c'est l'absence de mécanisme pour
///   cette version précise.
fn scenario_4_after_undo() -> Row {
    let before = live_bytes();
    let mut run = run_alternating(0);

    let (captured, tail) = run.target_fingerprints.split_at(WRITE_ITERATIONS);
    let tail_fingerprint = &tail[0];
    let (captured_versions_before, _) = live_versions(captured);
    let tail_alive_before = tail_fingerprint.weak.upgrade().is_some();
    let (versions_before_undo, bytes_before_undo) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);

    let mut steps = 0usize;
    while run
        .history
        .undo(&mut run.project)
        .expect("annulation acceptée")
    {
        steps += 1;
    }

    let (captured_versions_after, _) = live_versions(captured);
    let tail_alive_after = tail_fingerprint.weak.upgrade().is_some();
    let (versions_after_undo, bytes_after_undo) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let after = live_bytes();

    println!("\n== Scénario 4 — après annulation complète du scénario 2 ==");
    println!("  {steps} annulation(s) effectuée(s) jusqu'à Ok(false)");
    println!(
        "  total (cible + témoins), mesure 1 : {versions_before_undo} versions avant -> {versions_after_undo} après \
         ({:.1} Mo -> {:.1} Mo)",
        mb(bytes_before_undo),
        mb(bytes_after_undo)
    );
    println!(
        "  versions capturées par une entrée structurelle (les {WRITE_ITERATIONS} premières écritures) : \
         {captured_versions_before} avant -> {captured_versions_after} après"
    );
    assert_eq!(
        captured_versions_after, captured_versions_before,
        "une version retenue par une entrée structurelle ne doit pas disparaître à l'annulation : \
         `History::undo` déplace l'entrée vers `redo`, il ne la détruit jamais"
    );
    println!(
        "  version « courante », retenue par aucune entrée : vivante avant = {tail_alive_before}, \
         après = {tail_alive_after} (attendu : redevient fausse — personne ne la retenait, l'annulation \
         abandonne l'état qui la portait)"
    );
    println!(
        "  -> confirmé : tout ce qu'une entrée structurelle retient reste épinglé pour le \
         rétablissement (redo) ; seule la version courante, jamais capturée par aucune entrée, \
         disparaît."
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2, du début du scénario à après l'annulation)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "4. Après annulation complète (scénario 2)",
        live_versions: versions_after_undo,
        pinned_bytes: bytes_after_undo,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scénario 5 — comparaison honnête, sans History
// ===========================================================================

/// Le même scénario alterné que le scénario 2, mais appliqué **directement**
/// au `Project`, sans `History` du tout. C'est la borne basse : ce que
/// coûterait un historique qui ne retiendrait aucun buffer. Le rapport entre
/// le scénario 2 et celui-ci est le surcoût réel du choix hybride — c'est le
/// chiffre que la décision attend.
fn scenario_5_no_history() -> Row {
    let before = live_bytes();
    let Fixture {
        mut project,
        sheet: _,
        target,
        others,
    } = build_fixture();
    let mut target_fingerprints = Vec::with_capacity(WRITE_ITERATIONS + 1);
    let mut other_fingerprints: Vec<Vec<BufferFingerprint>> =
        others.iter().map(|_| Vec::new()).collect();

    for i in 0..WRITE_ITERATIONS {
        target_fingerprints.push(fingerprint(&project, target));
        project
            .rename(target, &format!("Target{i}"))
            .expect("renommage direct accepté");
        project
            .set_values(target, i, ValueBlock::Float(vec![i as f64]))
            .expect("écriture directe acceptée");
        for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
            fingerprints.push(fingerprint(&project, column));
        }
    }
    target_fingerprints.push(fingerprint(&project, target));
    for (fingerprints, &column) in other_fingerprints.iter_mut().zip(others.iter()) {
        fingerprints.push(fingerprint(&project, column));
    }

    let (target_versions, _) = live_versions(&target_fingerprints);
    let (total_versions, pinned_bytes) = total_live(&target_fingerprints, &other_fingerprints);
    let after = live_bytes();

    println!("\n== Scénario 5 — même scénario, sans History (borne basse) ==");
    println!(
        "  versions vivantes du buffer cible : {target_versions} (personne d'autre ne clone l'arbre : mutation en place attendue)"
    );
    println!(
        "  total (cible + témoins) : {total_versions} versions, {:.1} Mo épinglés (mesure 1)",
        mb(pinned_bytes)
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    Row {
        name: "5. Même scénario, sans History (borne basse)",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Scénario 6 — suppression, le témoin
// ===========================================================================

/// Limite d'historique du scénario 6 : assez serrée (1 entrée) pour évincer
/// l'entrée de suppression dès la commande suivante.
const DELETION_HISTORY_LIMIT: usize = 1;

/// Ajoute une colonne de 1 000 000 `f64`, la supprime par `Command::DeleteColumn`
/// (son buffer doit rester vivant : c'est nécessaire à l'annulation), puis
/// pousse une commande de plus pour évincer cette entrée (limite = 1) et
/// vérifie que le buffer est enfin libéré. Contrairement aux autres
/// scénarios, ces deux assertions **ne sont pas des hypothèses** : elles
/// découlent directement du code (`Entry::Structural` retient `before`, qui
/// retient le nœud supprimé et donc son buffer ; `History::evict` retire
/// l'entrée la plus ancienne et relâche ses `Arc`).
fn scenario_6_deletion_witness() -> (Row, Row) {
    let before = live_bytes();
    let Fixture {
        mut project,
        sheet,
        target,
        others: _,
    } = build_fixture();
    let mut history = History::with_limit(DELETION_HISTORY_LIMIT);

    let values: Vec<f64> = (0..ROWS).map(|row| row as f64).collect();
    let victim = project
        .add_column(sheet, None, Some("Victim"), Column::from_f64(&values))
        .expect("colonne à supprimer");
    drop(values);

    let victim_fingerprint = fingerprint(&project, victim);

    history
        .execute(&mut project, Command::DeleteColumn { column: victim })
        .expect("suppression acceptée");

    let alive_after_delete = victim_fingerprint.weak.upgrade().is_some();
    let after_delete = live_bytes();
    assert!(
        alive_after_delete,
        "le buffer de la colonne supprimée doit rester vivant tant que son entrée \
         est sur la pile — sans ça, annuler la suppression serait impossible"
    );

    println!("\n== Scénario 6 — suppression, le témoin ==");
    println!(
        "  après suppression (entrée encore sur la pile) : buffer vivant = {alive_after_delete}"
    );

    // Pousse une commande neutre de plus : avec une limite de 1, ça évince
    // l'entrée de suppression, qui est la plus ancienne.
    history
        .execute(
            &mut project,
            Command::Rename {
                id: target,
                name: "AfterEviction".to_string(),
            },
        )
        .expect("renommage accepté");

    let alive_after_eviction = victim_fingerprint.weak.upgrade().is_some();
    let after_eviction = live_bytes();
    assert!(
        !alive_after_eviction,
        "une fois l'entrée de suppression évincée de l'historique, plus rien ne \
         retient le buffer : il doit être libéré"
    );

    println!(
        "  après éviction (limite = {DELETION_HISTORY_LIMIT}, une commande de plus poussée) : buffer vivant = {alive_after_eviction}"
    );
    println!(
        "  allocateur : {:.1} Mo avant le scénario, {:.1} Mo après suppression, {:.1} Mo après éviction",
        mb(before),
        mb(after_delete),
        mb(after_eviction)
    );

    let victim_bytes = victim_fingerprint.len * std::mem::size_of::<f64>();
    let row_delete = Row {
        name: "6a. Suppression (entrée encore sur la pile)",
        live_versions: 1,
        pinned_bytes: victim_bytes,
        allocator_bytes: after_delete.saturating_sub(before),
    };
    let row_evict = Row {
        name: "6b. Après éviction (limite = 1)",
        live_versions: 0,
        pinned_bytes: 0,
        allocator_bytes: after_eviction.saturating_sub(before),
    };
    (row_delete, row_evict)
}

// ===========================================================================
// Scénario 7 — la seconde limite (octets) prouvée sur le motif mesuré
// ===========================================================================

/// Même alternance que le scénario 2, mais sous
/// `History::with_limits(100, BYTE_BOUNDED_LIMIT)` : la seconde limite
/// (octets de charge utile retenue), ajoutée à côté de la limite en nombre
/// d'entrées, borne-t-elle réellement le motif mesuré au scénario 2, et à
/// quelle hauteur ?
///
/// Deux affirmations, chacune sur une mesure différente :
///
/// - le **total mesuré indépendamment** (les `Weak` promouvables de ce
///   fichier, comptés hors de la version *courante* de chaque colonne —
///   voir [`independent_historical_bytes`], le pendant mesuré de ce que
///   `History::retained_bytes` calcule) ne dépasse pas `BYTE_BOUNDED_LIMIT`
///   au-delà d'une marge documentée ;
/// - cette même mesure indépendante et `History::retained_bytes()` — deux
///   calculs indépendants du même total — **concordent à mieux que 10 %**.
///   Si elles divergeaient davantage, ce ne serait pas corrigé en ajustant
///   un seuil : ce serait un résultat à rapporter tel quel.
fn scenario_7_byte_bounded_history() -> Row {
    let before = live_bytes();
    let run = run_alternating_with_limits(100, BYTE_BOUNDED_LIMIT);
    let (target_versions, target_bytes) = live_versions(&run.target_fingerprints);
    let (total_versions, pinned_bytes) =
        total_live(&run.target_fingerprints, &run.other_fingerprints);
    let historical = independent_historical_bytes(&run);
    let reported = run.history.retained_bytes();
    let after = live_bytes();

    println!(
        "\n== Scénario 7 — même scénario, historique borné à {} Mio de charge utile retenue ==",
        mb(BYTE_BOUNDED_LIMIT)
    );
    println!(
        "  versions vivantes du buffer cible : {target_versions} ({:.1} Mo, mesure 1) \
         — attendu : de l'ordre de {} (plafond / taille de colonne, arrondi), pas {}",
        mb(target_bytes),
        (BYTE_BOUNDED_LIMIT / column_bytes()) + 1, // +1 : la version courante, jamais un coût
        WRITE_ITERATIONS + 1
    );
    println!(
        "  total (cible + témoins) : {total_versions} versions, {:.1} Mo épinglés (mesure 1, \
         inclut les versions courantes des 5 colonnes — pas un coût d'historique)",
        mb(pinned_bytes)
    );
    println!(
        "  total « historique seul » (mesure indépendante, hors versions courantes) : {:.2} Mo, \
         face à un plafond de {:.1} Mo",
        mb(historical),
        mb(BYTE_BOUNDED_LIMIT)
    );
    println!(
        "  History::retained_bytes() rapporté : {:.2} Mo",
        mb(reported)
    );
    println!(
        "  allocateur : {:.1} -> {:.1} Mo vivants (Δ {:.1} Mo, mesure 2)",
        mb(before),
        mb(after),
        mb(after.saturating_sub(before))
    );

    // Marge documentée : les deltas de données (avant + après, quelques
    // `f64` par écriture) pèsent quelques centaines d'octets sur
    // `WRITE_ITERATIONS` itérations — négligeables face au plafond, mais pas
    // nuls — et l'éviction, strictement FIFO par construction, ne vise que
    // la limite en octets au granule d'une entrée entière (~7,6 Mo ici) : le
    // dernier retrait peut laisser le total jusqu'à `column_bytes()` en
    // dessous du plafond, jamais au-dessus, sauf si la règle « jamais
    // évincer la dernière entrée » devait un jour être atteinte ici (elle ne
    // l'est pas : une quarantaine d'entrées survivent au plafond). Une marge
    // d'1 Mio couvre confortablement les deltas sans masquer un dépassement
    // réel.
    let margin = 1024 * 1024;
    assert!(
        historical <= BYTE_BOUNDED_LIMIT + margin,
        "le total mesuré indépendamment ({:.2} Mo) dépasse le plafond ({:.1} Mo) au-delà de la \
         marge documentée ({:.2} Mo)",
        mb(historical),
        mb(BYTE_BOUNDED_LIMIT),
        mb(margin)
    );

    // Concordance entre les deux mesures indépendantes du même total.
    let relative_diff = if historical > 0 {
        (reported as f64 - historical as f64).abs() / historical as f64
    } else {
        f64::from(u8::from(reported > 0))
    };
    println!(
        "  concordance History::retained_bytes() / mesure indépendante : écart {:.2} %",
        relative_diff * 100.0
    );
    assert!(
        relative_diff < 0.10,
        "History::retained_bytes() ({:.2} Mo) et la mesure indépendante ({:.2} Mo) divergent de \
         {:.1} %, au-delà des 10 % attendus",
        mb(reported),
        mb(historical),
        relative_diff * 100.0
    );

    Row {
        name: "7. Historique borné à 64 Mo de charge utile",
        live_versions: target_versions,
        pinned_bytes,
        allocator_bytes: after.saturating_sub(before),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_1_writes_only_pins_nothing_extra() {
    let _guard = measure_lock();
    print_row(&scenario_1_writes_only());
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_2_alternating_structure_and_writes() {
    let _guard = measure_lock();
    print_row(&scenario_2_alternating());
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_3_bounded_history_caps_pinned_versions() {
    let _guard = measure_lock();
    print_row(&scenario_3_bounded_history());
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_4_undo_keeps_everything_for_redo() {
    let _guard = measure_lock();
    print_row(&scenario_4_after_undo());
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_5_baseline_without_history() {
    let _guard = measure_lock();
    print_row(&scenario_5_no_history());
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_6_deleted_column_freed_only_after_eviction() {
    let _guard = measure_lock();
    let (row_delete, row_evict) = scenario_6_deletion_witness();
    print_row(&row_delete);
    print_row(&row_evict);
}

#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn scenario_7_byte_limit_caps_the_measured_pattern() {
    let _guard = measure_lock();
    print_row(&scenario_7_byte_bounded_history());
}

/// Rejoue les sept scénarios dans l'ordre et imprime le tableau récapitulatif
/// final — c'est ce test qui fait foi pour le tableau de chiffres.
#[test]
#[ignore = "alloue des centaines de mégaoctets ; lancer explicitement avec --ignored"]
fn summary_table_all_scenarios() {
    let _guard = measure_lock();

    let mut rows = Vec::with_capacity(8);
    rows.push(scenario_1_writes_only());
    rows.push(scenario_2_alternating());
    rows.push(scenario_3_bounded_history());
    rows.push(scenario_4_after_undo());
    rows.push(scenario_5_no_history());
    let (row_6a, row_6b) = scenario_6_deletion_witness();
    rows.push(row_6a);
    rows.push(row_6b);
    rows.push(scenario_7_byte_bounded_history());

    print_summary_table(&rows);
}
