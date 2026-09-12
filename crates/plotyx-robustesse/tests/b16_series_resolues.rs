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
use plotyx_core::{Column, ColumnType, NodeId, Project};
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
fn b16_une_colonne_derreur_de_type_texte_est_une_reference_rompue() {
    // Une colonne texte n'est pas traçable : le module l'assimile à une
    // référence rompue plutôt qu'à une série vide. C'est le bon réflexe.
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
    assert!(resolue.broken());
    assert_eq!(resolue.missing, vec![MissingRole::YError]);
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
