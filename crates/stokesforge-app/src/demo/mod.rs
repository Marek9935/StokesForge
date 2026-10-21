//! Le projet de démonstration : de quoi essayer beaucoup de fonctionnalités dès
//! l'ouverture, sans fichier (décision de l'utilisateur : la démo vit dans le
//! code). Module du **binaire**, pas de la bibliothèque : l'application n'en a
//! pas besoin, seul `main` l'ouvre.
//!
//! ```text
//! Demo
//! ├── Book1 (Sheet1…4, Spectre, Cinetique) et Graph1   — inchangés
//! ├── Classeurs : Types, Mesures, Matrice, Lisez-moi   (classeur.rs)
//! ├── Analyse   : un classeur par famille de calcul    (analyse.rs)
//! └── Panneaux, Axes, TypesTrace, Annotations, Groupes, Lignes   (graphes.rs)
//! ```
//!
//! `bruit.rs` donne le bruit pseudo-aléatoire à graine fixe : la démo est la
//! même à chaque lancement, ce que les tests épinglent.
//!
//! # Contraintes qui ont fixé la forme
//!
//! - **Un `Project` neuf, avant `Session::spawn`** (invariant 1) : on peut
//!   alors appeler les primitives de `stokesforge-core`, jamais ensuite. Les
//!   calculs déjà faits passent par `add_calculation` sur ce même projet neuf.
//! - **Les colonnes à formule ne reçoivent aucune valeur ici** (invariant 11,
//!   #193) : seule la formule est posée ; `session_for_project` demande un
//!   recalcul complet juste après le démarrage de l'acteur, sans quoi ces
//!   colonnes resteraient vides.
//! - **Les noms de fenêtre sont uniques dans tout le projet**, pas seulement
//!   entre frères : le chemin `[Nom]` les désigne sans dossier. C'est pourquoi
//!   le graphe des types de tracé s'appelle `TypesTrace` (le classeur `Types`
//!   existe déjà).
//! - **Les jeux à axe logarithmique sont strictement positifs** (invariant 10).
//! - Quelques centaines de points par jeu au plus : la sauvegarde automatique
//!   (#317) copie la démo.
//! - Les noms de colonnes restent les lettres automatiques (A, B, C…) ; les
//!   titres lisibles sont des noms longs.

mod analyse;
mod bruit;
mod classeur;
mod graphes;

#[cfg(test)]
mod tests;

use anyhow::Context as _;
use stokesforge_core::{NodeId, Project};

/// Donne le rôle X (abscisse) à une colonne de la démonstration : la
/// première colonne d'une feuille est l'axe des abscisses.
fn set_x_role(project: &mut Project, column: NodeId) -> anyhow::Result<()> {
    let mut meta = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(|column| column.meta().clone())
        .unwrap_or_default();
    meta.role = stokesforge_core::ColumnRole::X;
    project
        .set_column_meta(column, meta)
        .context("rôle X de la démonstration")?;
    Ok(())
}

/// Projet de démonstration (voir la documentation du module). `Book1` et
/// `Graph1` d'abord, dans cet ordre : le script d'exemple lit `[Book1]Sheet1!A`.
pub(crate) fn demo_project() -> anyhow::Result<Project> {
    let mut project = Project::new("Demo").context("projet de démonstration")?;
    let book1 = classeur::book1(&mut project)?;
    graphes::graph1(&mut project, &book1)?;
    let mesures = classeur::dossier(&mut project)?;
    analyse::dossier(&mut project)?;
    graphes::autres(&mut project, &book1, &mesures)?;
    Ok(project)
}
