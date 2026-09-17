//! Batterie 16 — ce qu'une série devient quand ses colonnes n'ont pas la même
//! longueur.
//!
//! La règle est décidée et documentée (`resolve.rs`) : « le tracé porte sur le
//! **préfixe commun** plutôt que d'être refusé ». Elle est portée telle quelle
//! de `legacy-python/src/plotyx/plot/resolve.py`, dont le commentaire nomme
//! même le cas visé : « c'est le cas courant d'une colonne d'erreur plus courte
//! que le signal ». **Rien de ce qui suit n'est donc une régression du port**
//! — c'est une décision héritée dont la conséquence n'a jamais été chiffrée.
//!
//! Ce que cette batterie met en face de cette règle, c'est l'autre promesse du
//! même module : une série ne doit jamais être « silencieusement vide », et
//! l'état dégradé doit être **détectable**. Les deux ne tiennent pas ensemble.

use plotyx_core::resolve::{resolve_series, MissingRole};
use plotyx_core::{Column, ColumnType, MixedCell, NodeId, Project};
use plotyx_plot::{PlotType, SeriesSpec};

/// `[Book1]Sheet1` avec `Y` de `points` lignes, plus les colonnes d'erreur
/// que le test demande.
struct Banc {
    project: Project,
    sheet: NodeId,
    y: NodeId,
}

impl Banc {
    fn new(points: usize) -> Self {
        let mut project = Project::new("Demo").expect("projet valide");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        let valeurs: Vec<f64> = (0..points).map(|i| i as f64).collect();
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&valeurs))
            .expect("colonne Y");
        Self { project, sheet, y }
    }

    fn colonne(&mut self, nom: &str, valeurs: &[f64]) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(nom), Column::from_f64(valeurs))
            .expect("colonne")
    }

    fn vide(&mut self, nom: &str) -> NodeId {
        self.project
            .add_column(
                self.sheet,
                None,
                Some(nom),
                Column::new(ColumnType::Numeric),
            )
            .expect("colonne vide")
    }

    fn vide_mixte(&mut self, nom: &str) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(nom), Column::new(ColumnType::Mixed))
            .expect("colonne mixte vide")
    }

    /// Colonne mixte : `nombres[i]` si `Some`, sinon un texte — de quoi
    /// mélanger cellules numériques et non traçables dans la même colonne.
    fn colonne_mixte(&mut self, nom: &str, cellules: &[MixedCell]) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(nom), Column::from_mixed(cellules))
            .expect("colonne mixte")
    }

    fn serie_avec_erreur(&self, y_error: NodeId) -> SeriesSpec {
        SeriesSpec {
            plot_type: PlotType::ErrorBar,
            y_error: Some(y_error),
            ..SeriesSpec::new(self.y)
        }
    }
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b16_une_serie_complete_se_resout_entierement() {
    let mut banc = Banc::new(1000);
    let erreur = banc.colonne("dY", &vec![0.1; 1000]);
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);
    assert!(resolue.ok());
    assert!(!resolue.truncated);
    assert_eq!(resolue.point_count(), 1000);
}

#[test]
fn b16_une_colonne_derreur_supprimee_est_une_reference_rompue_detectable() {
    let mut banc = Banc::new(1000);
    let erreur = banc.colonne("dY", &vec![0.1; 1000]);
    let serie = banc.serie_avec_erreur(erreur);
    banc.project.remove(erreur).expect("suppression");
    let resolue = resolve_series(banc.project.tree(), &serie);
    assert!(
        resolue.broken(),
        "une référence rompue doit être détectable"
    );
    assert_eq!(resolue.missing, vec![MissingRole::YError]);
    assert!(!resolue.ok());
}

#[test]
fn b16_une_colonne_derreur_mixte_complete_se_resout_entierement() {
    // Colonne d'erreur mixte, mais entièrement numérique en pratique (pas de
    // texte ni de cellule vide) : elle doit se comporter comme n'importe
    // quelle colonne numérique une fois passée par `numeric_view`.
    let mut banc = Banc::new(1000);
    let cellules: Vec<MixedCell> = (0..1000)
        .map(|i| MixedCell::Number(i as f64 * 0.1))
        .collect();
    let erreur = banc.colonne_mixte("dY", &cellules);
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);
    assert!(resolue.ok());
    assert!(!resolue.truncated);
    assert_eq!(resolue.point_count(), 1000);
}

#[test]
fn b16_une_colonne_mixte_avec_du_texte_se_resout_au_prefixe_commun_avec_des_nan() {
    // Le cas d'usage motivant du chantier : une colonne mixte comme X, plus
    // courte de moitié que Y, avec du texte et une cellule vide mélangés aux
    // nombres. `numeric_view` traduit texte/vide en `NaN` (le renderer les
    // écarte déjà) ; la troncature au préfixe commun, elle, obéit à la même
    // règle qu'une colonne homogène.
    let mut banc = Banc::new(1000);
    let mut cellules: Vec<MixedCell> = (0..500).map(|i| MixedCell::Number(i as f64)).collect();
    cellules[10] = MixedCell::Text("n/a".to_string());
    cellules[20] = MixedCell::Empty;
    let x = banc.colonne_mixte("X", &cellules);
    let serie = SeriesSpec {
        plot_type: PlotType::Scatter,
        x: Some(x),
        ..SeriesSpec::new(banc.y)
    };
    let resolue = resolve_series(banc.project.tree(), &serie);
    assert!(
        resolue.ok(),
        "la série reste traçable malgré le texte/le vide"
    );
    assert!(
        resolue.truncated,
        "X (500 lignes) est plus court que Y (1000) : le préfixe commun doit être signalé"
    );
    assert_eq!(resolue.point_count(), 500);
}

#[test]
fn b16_une_colonne_derreur_entierement_textuelle_resout_sans_le_moindre_point() {
    // Il n'y a plus de type de colonne non traçable (`Text` a disparu au
    // profit de `Mixed`, voir `plotyx_core::resolve`) : une colonne
    // entièrement textuelle résout sa série normalement, simplement sans
    // aucun point numérique — ce n'est plus une référence rompue.
    let mut banc = Banc::new(1000);
    let texte = banc
        .project
        .add_column(
            banc.sheet,
            None,
            Some("dY"),
            Column::from_text(&[Some("0.1".to_string())]),
        )
        .expect("colonne texte");
    let serie = banc.serie_avec_erreur(texte);
    let resolue = resolve_series(banc.project.tree(), &serie);
    assert!(!resolue.broken());
    assert!(resolue.missing.is_empty());
    let y_error = resolue.y_error.expect("ordonnées d'erreur résolues");
    assert!(y_error.as_slice().iter().all(|value| value.is_nan()));
}

// -- reproductions --------------------------------------------------------

#[test]
fn b16_une_colonne_derreur_courte_ne_tronque_pas_les_donnees() {
    let mut banc = Banc::new(1000);
    let erreur = banc.colonne("dY", &[0.1, 0.2, 0.3]);
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);

    println!(
        "Y a 1000 points, dY en a 3 → série résolue : {} points, ok = {}, truncated = {}, \
         missing = {:?}",
        resolue.point_count(),
        resolue.ok(),
        resolue.truncated,
        resolue.missing
    );
    assert_eq!(
        resolue.point_count(),
        1000,
        "les colonnes d'erreur sont des annotations par point, pas des coordonnées : \
         `x` et `y` vont par paires et se tronquent ensemble, mais une barre d'erreur \
         manquante devrait valoir `NaN` (la valeur manquante du modèle) et non \
         raccourcir le signal. Ici la courbe est tracée avec 3 points sur 1000, et \
         `ok()` rend `true`"
    );
}

#[test]
fn b16_une_colonne_derreur_mixte_courte_ne_tronque_pas_les_donnees() {
    // Même anomalie que `b16_une_colonne_derreur_courte_ne_tronque_pas_les_donnees`,
    // mais avec une colonne d'erreur mixte (texte + nombres) plutôt
    // qu'homogène : le passage par `numeric_view` ne change rien à la règle
    // de troncature du préfixe commun, bonne ou mauvaise.
    let mut banc = Banc::new(1000);
    let cellules = vec![
        MixedCell::Number(0.1),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Number(0.3),
    ];
    let erreur = banc.colonne_mixte("dY", &cellules);
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);

    println!(
        "Y a 1000 points, dY (mixte) en a 3 → série résolue : {} points, ok = {}, \
         truncated = {}, missing = {:?}",
        resolue.point_count(),
        resolue.ok(),
        resolue.truncated,
        resolue.missing
    );
    assert_eq!(
        resolue.point_count(),
        1000,
        "une colonne d'erreur mixte trop courte tronque le signal à 3 points sur 1000, \
         exactement comme son équivalent homogène — la conversion par `numeric_view` \
         n'isole pas ce cas de l'anomalie déjà documentée"
    );
}

#[test]
fn b16_une_colonne_derreur_mixte_vide_ne_fait_pas_disparaitre_la_serie() {
    let mut banc = Banc::new(1000);
    let erreur = banc.vide_mixte("dY");
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);

    println!(
        "Y a 1000 points, dY (mixte) est vide → {} points, ok = {}, broken = {}, \
         missing = {:?}, truncated = {}",
        resolue.point_count(),
        resolue.ok(),
        resolue.broken(),
        resolue.missing,
        resolue.truncated
    );
    assert!(
        resolue.broken() || resolue.point_count() > 0,
        "même anomalie que la colonne d'erreur vide homogène, ici avec une colonne mixte \
         vide : la série disparaît silencieusement (zéro point, `broken()` faux)"
    );
}

#[test]
fn b16_une_colonne_derreur_vide_ne_fait_pas_disparaitre_la_serie() {
    let mut banc = Banc::new(1000);
    let erreur = banc.vide("dY");
    let serie = banc.serie_avec_erreur(erreur);
    let resolue = resolve_series(banc.project.tree(), &serie);

    println!(
        "Y a 1000 points, dY est vide → {} points, ok = {}, broken = {}, missing = {:?}, \
         truncated = {}",
        resolue.point_count(),
        resolue.ok(),
        resolue.broken(),
        resolue.missing,
        resolue.truncated
    );
    assert!(
        resolue.broken() || resolue.point_count() > 0,
        "la série a disparu — zéro point — et `broken()` rend `false` avec `missing` \
         vide : `ResolvedPlot::broken_series()`, le seul diagnostic que l'API offre, \
         ne la listera pas. C'est exactement l'état que la documentation de tête de \
         `resolve.rs` promet d'éviter : « jamais une série silencieusement vide ». \
         Ajouter une colonne d'erreur à une série et oublier de la remplir suffit"
    );
}
