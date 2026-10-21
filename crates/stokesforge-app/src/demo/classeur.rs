//! Les classeurs de la démonstration : `Book1` (celui du script d'exemple, ses
//! lettres et ses valeurs ne bougent pas) et le dossier `Classeurs` (les types
//! de colonnes, les mesures qui nourrissent les graphes, une matrice, une note).
//!
//! Tout se construit sur un `Project` neuf, **avant** `Session::spawn`
//! (invariant 1 : ce n'est pas une mutation du modèle d'une session). Les
//! colonnes à formule sont seulement *déclarées* ici ; leurs valeurs, jamais
//! écrites à la main (invariant 11), viennent du recalcul que
//! `session_for_project` demande au démarrage.

use anyhow::Context as _;
use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, NodeId, Project, ValueBlock,
};

use super::analyse::colonne_titree;
use super::bruit::Bruit;
use super::set_x_role;

/// Les colonnes de `Book1` que les graphes tracent.
pub(super) struct Book1 {
    pub(super) spectre_x: NodeId,
    pub(super) spectre_y: NodeId,
    /// Une colonne de `Cinetique` : les séries « ligne » désignent la feuille
    /// par n'importe laquelle de ses colonnes.
    pub(super) cinetique: NodeId,
}

/// Les colonnes du classeur `Mesures`, que les graphes `Types`, `Panneaux`,
/// `Axes` et `Groupes` tracent.
pub(super) struct Mesures {
    pub(super) temps: NodeId,
    /// Strictement positive : elle passe en échelle logarithmique (invariant 10).
    pub(super) decroissance: NodeId,
    /// Strictement positive, elle aussi.
    pub(super) croissance: NodeId,
    pub(super) erreur: NodeId,
    pub(super) bruit: NodeId,
    pub(super) oscillation: NodeId,
    pub(super) barres: (NodeId, NodeId),
    pub(super) familles_x: NodeId,
    pub(super) familles_y: Vec<NodeId>,
}

/// `Book1`, tel qu'il est depuis les débuts de la démonstration.
///
/// Il est construit pour que les cas tordus du modèle soient visibles dès
/// l'ouverture, pas seulement dans les tests : une valeur manquante numérique
/// (`NaN`), les deux valeurs texte que rien ne distingue à l'écran (`None` et
/// `Some("")`), et surtout des **colonnes de longueurs différentes** — une
/// feuille n'est pas un tableau rectangulaire.
/// Les noms de colonnes restent automatiques (A, B, C…) pour que le script
/// d'exemple désigne la même colonne que la lettre visible dans la grille.
pub(super) fn book1(project: &mut Project) -> anyhow::Result<Book1> {
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .context("classeur Book1")?;
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .context("lecture de Book1")?
        .context("Sheet1 créée avec le classeur")?;
    let a1 = project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .context("colonne A de Sheet1")?;
    set_x_role(project, a1)?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[10.0, 12.5, f64::NAN, 18.0, 21.5]),
        )
        .context("colonne B de Sheet1")?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .context("colonne C de Sheet1")?;
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .context("feuille Sheet2")?;
    project
        .add_column(sheet2, None, None, Column::from_f64(&[7.0, 8.0]))
        .context("colonne A de Sheet2")?;
    // Deux feuilles vides de plus — Book1 n'en montrait que trois, pas assez
    // pour éprouver à l'œil le défilement de la bande d'onglets (F8) une
    // fois qu'elle ne rétrécit plus ses onglets.
    project
        .add_worksheet(book, Some("Sheet3"))
        .context("feuille Sheet3")?;
    project
        .add_worksheet(book, Some("Sheet4"))
        .context("feuille Sheet4")?;

    // Le graphe de la phase R4. Ses colonnes sont **les siennes** et non
    // A/B de Sheet1 : celles-là portent volontairement un `NaN` et des
    // longueurs inégales pour éprouver la grille, ce qui donnerait une
    // courbe difficile à lire. Un banc doit montrer les cas tordus du
    // modèle *et* un rendu qu'on puisse juger à l'œil.
    let spectre = project
        .add_worksheet(book, Some("Spectre"))
        .context("feuille Spectre")?;
    let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            let a = (x - 520.0) / 38.0;
            let b = (x - 610.0) / 22.0;
            0.85 * (-a * a).exp() + 0.32 * (-b * b).exp() + 0.04
        })
        .collect();
    let x = project
        .add_column(spectre, None, None, Column::from_f64(&xs))
        .context("colonne A de Spectre")?;
    set_x_role(project, x)?;
    let y = project
        .add_column(spectre, None, None, Column::from_f64(&ys))
        .context("colonne B de Spectre")?;
    // Une cinétique pour éprouver le glisser de lignes (#166) : une colonne par
    // instant (ligne x' = temps en s), une ligne par longueur d'onde. Sélectionner
    // des numéros de ligne puis attraper le bord du bloc trace une courbe par
    // ligne, contre les x'.
    let cinetique = project
        .add_worksheet(book, Some("Cinetique"))
        .context("feuille Cinetique")?;
    let mut derniere = None;
    for temps in [0.0, 30.0, 60.0, 120.0, 240.0] {
        let valeurs: Vec<f64> = (0..8)
            .map(|ligne| {
                let amplitude = 0.2 + 0.1 * ligne as f64;
                amplitude * (1.0 - (-temps / (60.0 + 20.0 * ligne as f64)).exp())
            })
            .collect();
        let colonne = project
            .add_column(cinetique, None, None, Column::from_f64(&valeurs))
            .context("colonne de Cinetique")?;
        let mut meta = project
            .tree()
            .node(colonne)
            .and_then(|node| node.as_column())
            .map(|column| column.meta().clone())
            .unwrap_or_default();
        meta.x_prime = Some(temps);
        meta.unit = "u.a.".into();
        project
            .set_column_meta(colonne, meta)
            .context("x' de Cinetique")?;
        derniere = Some(colonne);
    }
    Ok(Book1 {
        spectre_x: x,
        spectre_y: y,
        cinetique: derniere.context("Cinetique a des colonnes")?,
    })
}

/// Pose la métadonnée `meta` sur `colonne`, sans toucher au reste.
fn pose_meta(
    project: &mut Project,
    colonne: NodeId,
    modifie: impl FnOnce(&mut ColumnMeta),
) -> anyhow::Result<()> {
    let mut meta = project
        .tree()
        .node(colonne)
        .and_then(|node| node.as_column())
        .map(|c| c.meta().clone())
        .context("colonne à annoter")?;
    modifie(&mut meta);
    project
        .set_column_meta(colonne, meta)
        .context("métadonnée de colonne")?;
    Ok(())
}

/// Une colonne à formule : la colonne est posée vide, puis la formule (saisie
/// en **lettres**) est résolue contre les colonnes déjà présentes de la feuille.
fn colonne_formule(
    project: &mut Project,
    feuille: NodeId,
    texte: &str,
    titre: &str,
) -> anyhow::Result<NodeId> {
    let colonne = project
        .add_column(feuille, None, None, Column::new(ColumnType::Numeric))
        .context("colonne à formule")?;
    let formule = project
        .tree()
        .formula_from_letters(colonne, texte)
        .with_context(|| format!("formule {texte}"))?;
    pose_meta(project, colonne, |meta| {
        meta.formula = Some(formule);
        meta.long_name = titre.to_string();
    })?;
    Ok(colonne)
}

/// Le classeur `Types` : un exemplaire de chaque type de colonne et des
/// colonnes à formule.
fn types(project: &mut Project, dossier: NodeId) -> anyhow::Result<()> {
    let classeur = project
        .add_workbook(dossier, Some("Types"))
        .context("classeur Types")?;
    let feuille = project
        .tree()
        .children(classeur)?
        .first()
        .copied()
        .context("feuille du classeur Types")?;
    let temps: Vec<f64> = (0..12).map(f64::from).collect();
    colonne_titree(project, feuille, &temps, ("Temps", "s"), ColumnRole::X)?;

    // Numérique, avec des valeurs manquantes (`NaN`) et une métadonnée complète :
    // unité, nom long, commentaire et format.
    let nan = f64::NAN;
    let temperature = [
        20.5, 21.0, nan, 22.4, 23.1, nan, 24.0, 24.6, 25.3, 25.1, 26.0, 26.4,
    ];
    let colonne = colonne_titree(
        project,
        feuille,
        &temperature,
        ("Température", "°C"),
        ColumnRole::Y,
    )?;
    pose_meta(project, colonne, |meta| {
        meta.comment = "Capteur 2 : deux mesures manquantes (NaN).".into();
        meta.display_format = "0.00".into();
    })?;

    // Mixte, texte seulement : `None` et `Some("")` s'affichent pareil.
    let etiquettes = [
        Some("échantillon A".to_string()),
        None,
        Some(String::new()),
        Some("échantillon D".to_string()),
        Some("A12".to_string()),
    ];
    let colonne = project
        .add_column(feuille, None, None, Column::from_text(&etiquettes))
        .context("colonne mixte de texte")?;
    pose_meta(project, colonne, |meta| {
        meta.long_name = "Étiquette".into();
        meta.role = ColumnRole::Label;
    })?;

    // Mixte mêlé : des nombres, des textes et des cases vides sur la même colonne.
    let lectures = [
        MixedCell::Number(1.5),
        MixedCell::Text("n.d.".into()),
        MixedCell::Empty,
        MixedCell::Number(4.0),
        MixedCell::Text("<LD".into()),
        MixedCell::Number(6.5),
        MixedCell::Empty,
        MixedCell::Number(8.0),
    ];
    let lecture = project
        .add_column(feuille, None, None, Column::from_mixed(&lectures))
        .context("colonne mixte mêlée")?;
    pose_meta(project, lecture, |meta| {
        meta.long_name = "Lecture manuelle".into();
        meta.unit = "mg/L".into();
    })?;

    // Date et heure : des secondes POSIX, une mesure toutes les heures.
    let debut = 1_790_841_600.0; // 2026-10-01 08:00 UTC
    let horodatage: Vec<f64> = (0..12).map(|i| debut + 3600.0 * f64::from(i)).collect();
    let colonne = project
        .add_column(
            feuille,
            None,
            None,
            Column::from_block(
                ColumnType::DateTime,
                ValueBlock::Float(horodatage),
                ColumnMeta {
                    long_name: "Horodatage".into(),
                    ..ColumnMeta::default()
                },
            )
            .context("colonne date et heure")?,
        )
        .context("colonne date et heure")?;
    pose_meta(project, colonne, |meta| meta.role = ColumnRole::None)?;

    // Les formules lisent des lettres (A = Temps, B = Température, D = lecture).
    colonne_formule(project, feuille, "A*2+1", "2·t + 1")?;
    colonne_formule(project, feuille, "(B-A)/2", "(T − t) / 2")?;
    colonne_formule(project, feuille, "D+1", "Lecture + 1")?;
    Ok(())
}

/// Le classeur `Mesures` : les données des graphes (décroissance, croissance
/// strictement positives pour l'échelle logarithmique, barres d'erreur,
/// histogramme, barres, famille de six courbes).
fn mesures(project: &mut Project, dossier: NodeId) -> anyhow::Result<Mesures> {
    let mut bruit = Bruit::new(7);
    let classeur = project
        .add_workbook(dossier, Some("Mesures"))
        .context("classeur Mesures")?;
    let feuille = project
        .tree()
        .children(classeur)?
        .first()
        .copied()
        .context("feuille du classeur Mesures")?;
    let t: Vec<f64> = (1..=60).map(f64::from).collect();
    let decroissance: Vec<f64> = t
        .iter()
        .map(|t| 100.0 * (-t / 15.0).exp() * (1.0 + bruit.gauss(0.03)))
        .collect();
    let croissance: Vec<f64> = t
        .iter()
        .map(|t| 0.5 * t.powf(1.3) * (1.0 + bruit.gauss(0.03)))
        .collect();
    let incertitude: Vec<f64> = decroissance.iter().map(|v| 0.06 * v).collect();
    let histogramme: Vec<f64> = (0..300).map(|_| 5.0 + bruit.gauss(1.0)).collect();
    let oscillation: Vec<f64> = t.iter().map(|t| 25.0 + 20.0 * (t / 6.0).sin()).collect();

    let temps = colonne_titree(project, feuille, &t, ("Temps", "s"), ColumnRole::X)?;
    let decroissance = colonne_titree(
        project,
        feuille,
        &decroissance,
        ("Décroissance", "mV"),
        ColumnRole::Y,
    )?;
    let croissance = colonne_titree(
        project,
        feuille,
        &croissance,
        ("Croissance", "mV"),
        ColumnRole::Y,
    )?;
    let erreur = colonne_titree(
        project,
        feuille,
        &incertitude,
        ("Incertitude", "mV"),
        ColumnRole::YError,
    )?;
    let bruit_col = colonne_titree(
        project,
        feuille,
        &histogramme,
        ("Bruit du détecteur", "mV"),
        ColumnRole::Y,
    )?;
    let oscillation = colonne_titree(
        project,
        feuille,
        &oscillation,
        ("Oscillation", "°C"),
        ColumnRole::Y,
    )?;

    let barres = project
        .add_worksheet(classeur, Some("Barres"))
        .context("feuille Barres")?;
    let numeros: Vec<f64> = (1..=8).map(f64::from).collect();
    let rendements = [62.0, 71.0, 55.0, 80.0, 68.0, 90.0, 47.0, 76.0];
    let barres_x = colonne_titree(
        project,
        barres,
        &numeros,
        ("Échantillon", ""),
        ColumnRole::X,
    )?;
    let barres_y = colonne_titree(
        project,
        barres,
        &rendements,
        ("Rendement", "%"),
        ColumnRole::Y,
    )?;

    let familles = project
        .add_worksheet(classeur, Some("Familles"))
        .context("feuille Familles")?;
    let x: Vec<f64> = (0..=50).map(|i| f64::from(i) * 0.2).collect();
    let familles_x = colonne_titree(
        project,
        familles,
        &x,
        ("Temps de réaction", "h"),
        ColumnRole::X,
    )?;
    let mut familles_y = Vec::new();
    for (rang, tau) in [0.8, 1.5, 2.5, 4.0, 6.0, 9.0].into_iter().enumerate() {
        let y: Vec<f64> = x
            .iter()
            .map(|x| (1.0 + 0.2 * rang as f64) * (1.0 - (-x / tau).exp()))
            .collect();
        familles_y.push(colonne_titree(
            project,
            familles,
            &y,
            ("Conversion", "mol/L"),
            ColumnRole::Y,
        )?);
    }
    Ok(Mesures {
        temps,
        decroissance,
        croissance,
        erreur,
        bruit: bruit_col,
        oscillation,
        barres: (barres_x, barres_y),
        familles_x,
        familles_y,
    })
}

/// Le texte de la note : ce que contient la démonstration et où le chercher.
const NOTE: &str = "\
Projet de démonstration de StokesForge.\n\
\n\
Book1 / Graph1 : le classeur du script d'exemple (Sheet1!A, une valeur manquante, \
des colonnes de longueurs différentes, une cinétique pour glisser des lignes).\n\
Classeurs : Types (un exemplaire de chaque type de colonne, des formules), Mesures \
(les données des graphes), une matrice, cette note.\n\
Analyse : un classeur par famille de calcul (ligne de base, correction, intégrale, \
ajustements, pics, reconvolution, déconvolution) ; Integrate, FitPolynome et \
FitGaussienne ont déjà leur calcul, visible dans le volet Results.\n\
Graphes : Panneaux, Axes, Types, Annotations, Groupes, Lignes.\n\
\n\
Tout est construit par le code (aucun fichier) ; le bruit est à graine fixe, donc \
identique à chaque lancement.";

/// Le dossier `Classeurs` : `Types`, `Mesures`, une matrice et la note.
pub(super) fn dossier(project: &mut Project) -> anyhow::Result<Mesures> {
    let racine = project.root();
    let dossier = project
        .add_folder(racine, "Classeurs")
        .context("dossier Classeurs")?;
    types(project, dossier)?;
    let mesures = mesures(project, dossier)?;
    project
        .add_matrix(dossier, Some("Matrice"))
        .context("matrice de démonstration")?;
    project
        .add_note(dossier, Some("Lisez-moi"), NOTE)
        .context("note de démonstration")?;
    Ok(mesures)
}
