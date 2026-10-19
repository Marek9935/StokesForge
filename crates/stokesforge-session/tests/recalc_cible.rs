//! Recalcul ciblé des colonnes-formule (#193) : cas nommés, vus depuis l'API
//! publique de la session.
//!
//! Deux familles de vérifications, et la seconde est celle qui manque le
//! plus facilement :
//!
//! - **juste** : après le geste, la colonne-formule porte la valeur que son
//!   texte et ses sources imposent — un recalcul oublié échoue ici ;
//! - **ciblé** : une colonne-formule que le geste ne concerne pas n'est pas
//!   réécrite, ce qui se lit sur sa [`Column::version`] — un mémo qui
//!   recalculerait tout passerait la première famille sans broncher.
//!
//! Chaque test « ciblé » a son **témoin** : le même scénario en
//! [`FormulaRecalc::Full`] réécrit la colonne, donc change sa version. Sans
//! ce témoin, une version inchangée pourrait aussi bien venir d'une écriture
//! qui ne change pas la version (ce que le dépôt interdit, mais qu'aucun
//! autre test ne verrait ici).
//!
//! Le contrat complet — « le recalcul ciblé donne exactement le recalcul
//! complet » — est tenu par la batterie différentielle
//! `stokesforge-robustesse/tests/b22_recalcul_cible.rs` ; ces cas nommés
//! disent *pourquoi* chaque composante de l'empreinte existe
//! (`stokesforge-session/src/recalc.rs`, `Fingerprint`).

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnType, ColumnValues, MixedCell, NodeCopy, NodeId, Project,
    ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};

// -- montage -----------------------------------------------------------------

/// `Book1 → Sheet1` avec `A [1,2,3]`, `B [10,20,30]`, et deux colonnes vides
/// `C`, `D` à qui poser des formules.
struct Feuille {
    sheet: NodeId,
    a: NodeId,
    b: NodeId,
    c: NodeId,
    d: NodeId,
}

fn projet() -> (Project, Feuille) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("C");
    let d = project
        .add_column(sheet, None, Some("D"), Column::new(ColumnType::Numeric))
        .expect("D");
    (project, Feuille { sheet, a, b, c, d })
}

fn demarrer(mode: FormulaRecalc) -> (Session, SessionHandle, Feuille) {
    let (project, feuille) = projet();
    let session = Session::spawn_with_recalc(project, None, mode).expect("acteur démarré");
    let handle = session.handle();
    (session, handle, feuille)
}

fn formule(column: NodeId, texte: &str) -> Command {
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(texte.to_string()),
            ..Default::default()
        },
    }
}

fn saisir(column: NodeId, start: usize, valeurs: &[f64]) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(valeurs.to_vec()),
        expected_version: None,
    }
}

/// Une commande qui ne touche à aucune colonne : un dossier de plus à la
/// racine, nommé d'après `rang` pour rester unique.
fn sans_rapport(snapshot: &Snapshot, rang: usize) -> Command {
    Command::AddFolder {
        parent: snapshot.tree.root(),
        name: format!("Témoin{rang}"),
    }
}

fn instantane(handle: &SessionHandle) -> Snapshot {
    handle.snapshot().expect("instantané")
}

fn version(snapshot: &Snapshot, column: NodeId) -> u64 {
    snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("colonne présente")
        .version()
}

/// Cellules d'une colonne lues comme nombres ; `None` pour une cellule non
/// numérique (vide, texte, `"---"`).
fn nombres(snapshot: &Snapshot, column: NodeId) -> Vec<Option<f64>> {
    let colonne = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("colonne présente");
    match colonne.values() {
        ColumnValues::Float(values) => values.iter().map(|v| (!v.is_nan()).then_some(*v)).collect(),
        ColumnValues::Mixed(cells) => cells
            .iter()
            .map(|cell| match cell {
                MixedCell::Number(v) if !v.is_nan() => Some(*v),
                _ => None,
            })
            .collect(),
    }
}

/// Vrai si toutes les cellules de `column` sont le marqueur `"---"` d'un
/// résultat non calculé (ou `NaN` pour une colonne numérique).
fn tout_non_calcule(snapshot: &Snapshot, column: NodeId) -> bool {
    let colonne = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("colonne présente");
    (0..colonne.len()).all(|row| match colonne.get(row) {
        Cell::Float(v) => v.is_nan(),
        Cell::Text(Some(t)) => t == "---",
        Cell::Text(None) => false,
    })
}

fn some(values: &[f64]) -> Vec<Option<f64>> {
    values.iter().copied().map(Some).collect()
}

// -- ciblé : ce qui ne doit plus être réécrit ---------------------------------

/// Joue le scénario « une formule, puis une commande sans rapport » et rend
/// `(version avant, version après)` de la colonne-formule.
fn versions_autour_d_une_commande_sans_rapport(mode: FormulaRecalc) -> (u64, u64) {
    let (_session, handle, f) = demarrer(mode);
    handle.execute(formule(f.c, "A+B")).expect("formule");
    let avant = instantane(&handle);
    handle
        .execute(sans_rapport(&avant, 0))
        .expect("commande sans rapport");
    let apres = instantane(&handle);
    assert_eq!(nombres(&apres, f.c), some(&[11.0, 22.0, 33.0]));
    (version(&avant, f.c), version(&apres, f.c))
}

/// Le cœur de #193 : changer quelque chose qui n'est la source d'aucune
/// formule ne réécrit plus aucune colonne-formule.
#[test]
fn une_commande_sans_rapport_ne_reecrit_aucune_formule() {
    let (avant, apres) = versions_autour_d_une_commande_sans_rapport(FormulaRecalc::Targeted);
    assert_eq!(avant, apres, "C n'avait aucune raison d'être réécrite");

    // Témoin : le recalcul complet la réécrit, et la version le montre.
    let (avant, apres) = versions_autour_d_une_commande_sans_rapport(FormulaRecalc::Full);
    assert_ne!(avant, apres, "témoin : le recalcul complet réécrit C");
}

/// Modifier A recalcule les formules qui lisent A, et seulement elles.
#[test]
fn modifier_une_source_ne_recalcule_que_ses_dependantes() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = demarrer(mode);
        handle.execute(formule(f.c, "A*2")).expect("C = A*2");
        handle.execute(formule(f.d, "B+1")).expect("D = B+1");
        let avant = instantane(&handle);

        handle
            .execute(saisir(f.a, 0, &[100.0]))
            .expect("saisie dans A");
        let apres = instantane(&handle);

        assert_eq!(nombres(&apres, f.c), some(&[200.0, 4.0, 6.0]));
        assert_eq!(nombres(&apres, f.d), some(&[11.0, 21.0, 31.0]));
        assert_ne!(version(&avant, f.c), version(&apres, f.c), "C lit A");
        match mode {
            FormulaRecalc::Targeted => assert_eq!(
                version(&avant, f.d),
                version(&apres, f.d),
                "D ne lit pas A : elle n'est pas réécrite"
            ),
            FormulaRecalc::Full => assert_ne!(
                version(&avant, f.d),
                version(&apres, f.d),
                "témoin : le recalcul complet réécrit D"
            ),
        }
    }
}

/// Joue « D = C+1, C = A*2, saisie dans A, puis une commande sans rapport »
/// et rend vrai si C et D ont gardé leur version pendant cette dernière.
fn chaine_inchangee_par_une_commande_sans_rapport(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = demarrer(mode);
    // D lit C qui lit A ; C est posée **après** D, pour que l'ordre de
    // déclaration ne coïncide pas avec l'ordre de calcul.
    handle.execute(formule(f.d, "C+1")).expect("D = C+1");
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(saisir(f.a, 1, &[7.0]))
        .expect("saisie dans A");
    let avant = instantane(&handle);
    assert_eq!(nombres(&avant, f.c), some(&[2.0, 14.0, 6.0]));
    assert_eq!(nombres(&avant, f.d), some(&[3.0, 15.0, 7.0]));

    handle
        .execute(sans_rapport(&avant, 0))
        .expect("commande sans rapport");
    let apres = instantane(&handle);
    version(&avant, f.c) == version(&apres, f.c) && version(&avant, f.d) == version(&apres, f.d)
}

/// Une deuxième passe immédiate ne réécrit rien : l'ordre des étapes du
/// recalcul (illisibles, types, cycles, ordre topologique) ne laisse aucune
/// colonne écrite après qu'une colonne qui la lit a pris son empreinte.
/// Sans cette propriété, chaque commande recalculerait une chaîne entière.
#[test]
fn une_chaine_recalculee_est_stable_a_la_commande_suivante() {
    assert!(chaine_inchangee_par_une_commande_sans_rapport(
        FormulaRecalc::Targeted
    ));
    assert!(
        !chaine_inchangee_par_une_commande_sans_rapport(FormulaRecalc::Full),
        "témoin : le recalcul complet réécrit la chaîne"
    );
}

// -- juste : chaque composante de l'empreinte a sa raison ---------------------

/// La propagation suit la chaîne dans la même passe : recalculer C change
/// sa version, donc D, qui lit C, se voit à son tour à recalculer.
#[test]
fn la_propagation_suit_la_chaine_et_revient_par_annuler_retablir() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle.execute(formule(f.d, "C+1")).expect("D = C+1");

    handle
        .execute(saisir(f.a, 2, &[50.0]))
        .expect("saisie dans A");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), some(&[2.0, 4.0, 100.0]));
    assert_eq!(nombres(&s, f.d), some(&[3.0, 5.0, 101.0]));

    handle.undo().expect("annuler la saisie");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), some(&[2.0, 4.0, 6.0]));
    assert_eq!(nombres(&s, f.d), some(&[3.0, 5.0, 7.0]));

    handle.redo().expect("rétablir la saisie");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), some(&[2.0, 4.0, 100.0]));
    assert_eq!(nombres(&s, f.d), some(&[3.0, 5.0, 101.0]));
}

/// Changer le texte de la formule, sans toucher à aucune donnée, recalcule
/// (composante `source`) ; annuler ce changement aussi.
#[test]
fn changer_le_texte_de_la_formule_recalcule() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle.execute(formule(f.c, "A*3")).expect("C = A*3");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[3.0, 6.0, 9.0]));

    handle.undo().expect("annuler");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Déplacer une colonne change ce que désignent les lettres : la formule
/// reste « A*2 » mais A est désormais une autre colonne (composante
/// « colonne désignée », par identifiant et non par lettre).
#[test]
fn deplacer_une_source_change_ce_que_designe_la_lettre() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(Command::Move {
            id: f.b,
            new_parent: f.sheet,
            index: 0,
        })
        .expect("B passe en tête");
    // Ni A ni B n'ont été écrites : seule la désignation a changé.
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );

    handle.undo().expect("annuler le déplacement");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Déplacer une source vers une autre feuille : la lettre désigne la
/// colonne suivante, ou plus rien.
#[test]
fn deplacer_une_source_vers_une_autre_feuille() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.d, "B+1")).expect("D = B+1");
    let book = instantane(&handle)
        .tree
        .parent(f.sheet)
        .expect("arbre cohérent")
        .expect("la feuille a un classeur");
    handle
        .execute(Command::AddWorksheet { book, name: None })
        .expect("seconde feuille");
    let autre = *instantane(&handle)
        .tree
        .children(book)
        .expect("classeur")
        .last()
        .expect("seconde feuille présente");
    handle
        .execute(Command::Move {
            id: f.a,
            new_parent: autre,
            index: 0,
        })
        .expect("A part dans l'autre feuille");
    // Feuille : B, C, D → « B » désigne C, vide : D lit une colonne vide et
    // en prend la longueur (#194).
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.d), vec![]);

    handle.undo().expect("annuler le déplacement");
    assert_eq!(
        nombres(&instantane(&handle), f.d),
        some(&[11.0, 21.0, 31.0])
    );
}

/// Une saisie dans une colonne-formule est refusée (`FormulaColumnReadOnly`),
/// mais deux commandes la modifient quand même sans toucher à ses sources :
/// changer son type et y supprimer des cellules. Le recalcul doit les
/// défaire, comme avant #193 — c'est la composante « version de la colonne
/// elle-même » de l'empreinte, la seule qui change ici.
#[test]
fn une_colonne_formule_modifiee_directement_est_recalculee() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    let refus = handle.execute(saisir(f.c, 0, &[999.0]));
    assert!(refus.is_err(), "la saisie directe reste refusée");

    handle
        .execute(Command::DeleteCells {
            columns: vec![f.c],
            at: 0,
            count: 1,
        })
        .expect("supprimer une cellule de C");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));

    handle
        .execute(Command::SetColumnType {
            column: f.c,
            column_type: ColumnType::Numeric,
        })
        .expect("changer le type de C");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), some(&[2.0, 4.0, 6.0]));
    assert_eq!(
        s.tree
            .node(f.c)
            .and_then(|node| node.as_column())
            .map(Column::column_type),
        Some(ColumnType::Mixed),
        "le recalcul rend à C le type que sa formule impose"
    );

    // Annuler la suppression réinsère une cellule dans C, qui passe à
    // quatre lignes ; le recalcul la ramène à la longueur de A (#194).
    handle.undo().expect("annuler le changement de type");
    handle.undo().expect("annuler la suppression");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Le résultat a la longueur de la plus longue des colonnes que la formule
/// lit (#194) : allonger A allonge C = A*2, annuler l'allongement la
/// raccourcit. Avant #194, la longueur était celle de la feuille, comptée
/// colonne-formule comprise : C, une fois allongée, ne raccourcissait plus.
#[test]
fn le_resultat_suit_la_longueur_des_colonnes_lues() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(saisir(f.a, 4, &[5.0]))
        .expect("A descend à 5 lignes");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        vec![Some(2.0), Some(4.0), Some(6.0), None, Some(10.0)]
    );
    handle.undo().expect("annuler l'allongement de A");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));

    // Deux sources de longueurs différentes : la plus longue l'emporte.
    handle.execute(formule(f.d, "A+B")).expect("D = A+B");
    handle
        .execute(saisir(f.b, 3, &[40.0]))
        .expect("B descend à 4 lignes");
    let s = instantane(&handle);
    assert_eq!(
        nombres(&s, f.d),
        vec![Some(11.0), Some(22.0), Some(33.0), None]
    );
    assert_eq!(nombres(&s, f.c).len(), 3, "C ne lit pas B");
}

/// Une formule sans référence (constante) a la longueur des colonnes
/// ordinaires, et la perd avec elles (#194).
#[test]
fn une_constante_suit_les_colonnes_ordinaires() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "1+2")).expect("C = 1+2");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[3.0, 3.0, 3.0]));
    handle
        .execute(saisir(f.d, 4, &[1.0]))
        .expect("D descend à 5 lignes");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[3.0; 5]));
    handle.undo().expect("annuler");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[3.0, 3.0, 3.0]));
}

/// Joue « allonger D, que la formule ne lit pas » et rend
/// `(C après, C est-elle restée à sa version ?)`.
fn allonger_une_colonne_non_lue(mode: FormulaRecalc) -> (Vec<Option<f64>>, bool) {
    let (_session, handle, f) = demarrer(mode);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    let avant = instantane(&handle);
    handle
        .execute(saisir(f.d, 4, &[1.0]))
        .expect("D descend à 5 lignes");
    let apres = instantane(&handle);
    (
        nombres(&apres, f.c),
        version(&avant, f.c) == version(&apres, f.c),
    )
}

/// Allonger une colonne que la formule ne lit pas ne l'allonge plus (#194),
/// et le ciblé ne la réécrit même pas ; témoin : le complet la réécrit, à
/// l'identique.
#[test]
fn allonger_une_colonne_non_lue_ne_change_pas_la_formule() {
    let (cible, epargnee) = allonger_une_colonne_non_lue(FormulaRecalc::Targeted);
    assert_eq!(cible, some(&[2.0, 4.0, 6.0]));
    assert!(epargnee, "C n'a pas été réécrite");
    let (complet, epargnee) = allonger_une_colonne_non_lue(FormulaRecalc::Full);
    assert_eq!(complet, cible);
    assert!(!epargnee, "témoin : le recalcul complet la réécrit");
}

/// Une formule illisible prend la longueur des colonnes ordinaires de la
/// feuille et la suit quand l'une d'elles grandit (issue `Unparsable`,
/// composante « longueur »), puis, corrigée, celle des colonnes qu'elle lit
/// (#194).
#[test]
fn une_formule_illisible_suit_la_feuille_puis_se_corrige() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*")).expect("C illisible");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), vec![None, None, None]);

    handle
        .execute(saisir(f.d, 4, &[1.0]))
        .expect("D descend à 5 lignes");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c).len(), 5, "la colonne suit la feuille");

    // Et raccourcit avec elle : la longueur « ordinaire » ne compte pas la
    // colonne-formule elle-même, qui sinon s'entretiendrait à cinq lignes
    // (#194).
    handle.undo().expect("annuler l'allongement de D");
    assert_eq!(nombres(&instantane(&handle), f.c).len(), 3);
    handle.redo().expect("rétablir");

    handle.execute(formule(f.c, "A*2")).expect("C corrigée");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Un cycle créé **par une autre colonne** bloque une formule dont ni le
/// texte ni les sources n'ont changé de contenu (issue `Blocked`), et le
/// casser la rend calculable.
#[test]
fn un_cycle_cree_ailleurs_bloque_la_formule_et_se_defait() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "B*2")).expect("C = B*2");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );

    handle
        .execute(formule(f.b, "C+1"))
        .expect("B = C+1 : cycle");
    let s = instantane(&handle);
    assert!(tout_non_calcule(&s, f.c), "C est prise dans le cycle");
    assert!(tout_non_calcule(&s, f.b), "B aussi");

    handle.undo().expect("annuler la formule de B");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );
}

/// Pose le cycle « C = B*2, B = C+1 » et rend vrai si une commande sans
/// rapport laisse les versions de B et C intactes.
fn cycle_inchange_par_une_commande_sans_rapport(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = demarrer(mode);
    handle.execute(formule(f.c, "B*2")).expect("C = B*2");
    handle
        .execute(formule(f.b, "C+1"))
        .expect("B = C+1 : cycle");
    let avant = instantane(&handle);
    assert!(tout_non_calcule(&avant, f.b) && tout_non_calcule(&avant, f.c));
    handle
        .execute(sans_rapport(&avant, 0))
        .expect("sans rapport");
    let apres = instantane(&handle);
    version(&avant, f.b) == version(&apres, f.b) && version(&avant, f.c) == version(&apres, f.c)
}

/// Un cycle stable ne se réécrit pas à chaque commande : ses membres ne se
/// renvoient pas leurs versions (les sources d'une formule bloquée
/// n'entrent pas dans son empreinte).
#[test]
fn un_cycle_stable_ne_se_reecrit_pas() {
    assert!(cycle_inchange_par_une_commande_sans_rapport(
        FormulaRecalc::Targeted
    ));
    assert!(
        !cycle_inchange_par_une_commande_sans_rapport(FormulaRecalc::Full),
        "témoin : le recalcul complet réécrit le cycle"
    );
}

/// Un cycle suit la longueur de la feuille : l'allonger par une colonne
/// qu'aucune formule ne lit allonge ses membres (issue `Blocked`, composante
/// « nombre de lignes »).
#[test]
fn un_cycle_suit_la_longueur_de_la_feuille() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "B*2")).expect("C = B*2");
    handle
        .execute(formule(f.b, "C+1"))
        .expect("B = C+1 : cycle");
    handle
        .execute(saisir(f.d, 4, &[1.0]))
        .expect("D descend à 5 lignes");
    let s = instantane(&handle);
    for membre in [f.b, f.c] {
        assert_eq!(nombres(&s, membre).len(), 5, "le cycle suit la feuille");
        assert!(tout_non_calcule(&s, membre));
    }
}

/// Une formule bloquée ou illisible, modifiée directement (suppression de
/// cellules), est réécrite (composante « version de la colonne elle-même »
/// pour les issues `Blocked` et `Unparsable`) : sans quoi elle garderait
/// une ligne de moins que la feuille.
#[test]
fn une_formule_bloquee_ou_illisible_modifiee_directement_est_reecrite() {
    for (texte, cycle) in [("B*2", true), ("A*", false)] {
        let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
        handle.execute(formule(f.c, texte)).expect("formule de C");
        if cycle {
            handle
                .execute(formule(f.b, "C+1"))
                .expect("B = C+1 : cycle");
        }
        handle
            .execute(Command::DeleteCells {
                columns: vec![f.c],
                at: 0,
                count: 1,
            })
            .expect("supprimer une cellule de C");
        let s = instantane(&handle);
        assert_eq!(nombres(&s, f.c).len(), 3, "{texte} : C suit la feuille");
        assert!(tout_non_calcule(&s, f.c), "{texte}");
    }
}

/// Changer le type d'une source (saisie de texte qui promeut en mixte)
/// recalcule : la cellule texte devient un résultat non calculé.
#[test]
fn une_source_promue_en_texte_recalcule() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(Command::SetCellsPromoting {
            column: f.a,
            start: 1,
            values: ValueBlock::Mixed(vec![MixedCell::Text("abc".into())]),
            expected_version: None,
        })
        .expect("texte dans A");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.c), vec![Some(2.0), None, Some(6.0)]);
}

/// Avorter une macro défait d'un bloc des écritures déjà recalculées : les
/// formules reviennent avec elles.
#[test]
fn avorter_une_macro_recalcule() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle.begin_macro(MacroKind::PasteCells).expect("macro");
    handle.execute(saisir(f.a, 0, &[5.0, 5.0])).expect("saisie");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[10.0, 10.0, 6.0]));
    handle.abort_macro().expect("avorter");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Supprimer une colonne-formule puis annuler : elle revient juste. (Une
/// source modifiée « entre-temps » ne peut pas l'être : l'historique est
/// strictement LIFO, annuler la suppression défait d'abord ce qui a suivi.)
#[test]
fn une_colonne_formule_restauree_est_juste() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(Command::DeleteColumn { column: f.c })
        .expect("supprimer C");
    // La lettre A désigne toujours A : la suppression de C (après A) ne
    // décale rien avant elle.
    handle.undo().expect("restaurer C");
    assert_eq!(nombres(&instantane(&handle), f.c), some(&[2.0, 4.0, 6.0]));
}

/// Duplique la feuille, saisit dans la copie de A, et rend vrai si la
/// colonne-formule **originale** a gardé sa version.
fn original_inchange_par_une_saisie_dans_la_copie(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = demarrer(mode);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(Command::DuplicateWorksheet {
            sheet: f.sheet,
            name: "Copie".into(),
        })
        .expect("dupliquer");
    let s = instantane(&handle);
    let book = s.tree.parent(f.sheet).expect("arbre").expect("classeur");
    let copie = s
        .tree
        .children(book)
        .expect("classeur")
        .iter()
        .copied()
        .find(|&id| id != f.sheet)
        .expect("copie présente");
    let colonnes = s.tree.children(copie).expect("feuille").to_vec();
    let (a2, c2) = (colonnes[0], colonnes[2]);

    handle
        .execute(saisir(a2, 0, &[9.0]))
        .expect("saisie dans la copie");
    let apres = instantane(&handle);
    assert_eq!(nombres(&apres, c2), some(&[18.0, 4.0, 6.0]));
    assert_eq!(nombres(&apres, f.c), some(&[2.0, 4.0, 6.0]));
    version(&s, f.c) == version(&apres, f.c)
}

/// Une feuille dupliquée calcule ses propres formules sur ses propres
/// colonnes : le mémo est tenu par identifiant, pas par nom ni par lettre,
/// et l'original n'est pas réécrit par une saisie dans la copie.
#[test]
fn une_feuille_dupliquee_a_ses_propres_formules() {
    assert!(original_inchange_par_une_saisie_dans_la_copie(
        FormulaRecalc::Targeted
    ));
    assert!(
        !original_inchange_par_une_saisie_dans_la_copie(FormulaRecalc::Full),
        "témoin : le recalcul complet réécrit l'original"
    );
}

/// Un projet construit comme [`projet`] — donc aux mêmes identifiants — où
/// A et C portent les versions `version_a` et `version_c` avec un autre
/// contenu : A vaut `[5, 5, 5]` et C, qui garde la formule `A*2`, porte des
/// zéros qu'aucun calcul ne justifie. C'est la coïncidence qu'un mémo non
/// vidé prendrait pour « rien n'a changé ». `add_column` garde la version de
/// la colonne qu'on lui donne.
fn projet_truque(f: &Feuille, version_a: u64, version_c: u64, meta_c: ColumnMeta) -> Project {
    let mut faux = Project::new("Demo").expect("projet valide");
    let root = faux.root();
    faux.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = faux.resolve_path("[Book1]Sheet1").expect("feuille");
    let fausse_a = Column::from_block_with_version(
        ColumnType::Numeric,
        ValueBlock::Float(vec![5.0, 5.0, 5.0]),
        ColumnMeta::default(),
        version_a,
    )
    .expect("A truquée");
    let fausse_c = Column::from_block_with_version(
        ColumnType::Mixed,
        ValueBlock::Mixed(vec![MixedCell::Number(0.0); 3]),
        meta_c,
        version_c,
    )
    .expect("C truquée");
    let ids = [
        faux.add_column(sheet, None, Some("A"), fausse_a),
        faux.add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        ),
        faux.add_column(sheet, None, Some("C"), fausse_c),
        faux.add_column(sheet, None, Some("D"), Column::new(ColumnType::Numeric)),
    ]
    .map(|id| id.expect("colonne"));
    assert_eq!(ids, [f.a, f.b, f.c, f.d], "construction déterministe");
    faux
}

/// Pose `C = A*2` et rend `(version de A, version de C, métadonnées de C)`
/// telles que le mémo vient de les noter.
fn memo_de_c(handle: &SessionHandle, f: &Feuille) -> (u64, u64, ColumnMeta) {
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    let s = instantane(handle);
    assert_eq!(nombres(&s, f.c), some(&[2.0, 4.0, 6.0]));
    let c = s
        .tree
        .node(f.c)
        .and_then(|node| node.as_column())
        .expect("C");
    assert_eq!(c.column_type(), ColumnType::Mixed);
    (version(&s, f.a), c.version(), c.meta().clone())
}

/// `Load` vide le mémo. Le fichier chargé reprend tout ce que le mémo
/// compare — mêmes identifiants, mêmes versions, même texte — avec une
/// autre source A et des valeurs de C fausses ([`projet_truque`]). Seul
/// l'oubli du mémo force le recalcul : un `.stf` écrit ailleurs (autre
/// processus, version antérieure) n'a aucune raison de partager la
/// numérotation de versions de ce projet-ci.
#[test]
fn le_chargement_oublie_le_memo_du_projet_remplace() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    let (version_a, version_c, meta_c) = memo_de_c(&handle, &f);
    let faux = projet_truque(&f, version_a, version_c, meta_c);

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let chemin = dir.path().join("faux.stf");
    stokesforge_persistence::save(&faux, &chemin).expect("écrire le fichier");

    handle.load(&chemin).expect("charger");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.a), some(&[5.0, 5.0, 5.0]));
    assert_eq!(nombres(&s, f.c), some(&[10.0, 10.0, 10.0]));
}

/// `NewProject` vide le mémo. Une passe ordinaire purge déjà les entrées
/// des colonnes absentes, ce qui suffit tant que la première requête du
/// projet neuf ne crée aucune colonne-formule ; le cas qui exige l'oubli
/// est donc un **collage** comme toute première requête. Le projet neuf
/// repart des mêmes identifiants : le classeur collé ([`projet_truque`],
/// capturé avec ses versions comme le fait Copier) retombe sur les
/// identifiants et les versions que le mémo a notés.
#[test]
fn un_nouveau_projet_oublie_le_memo_de_l_ancien() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    let (version_a, version_c, meta_c) = memo_de_c(&handle, &f);
    let faux = projet_truque(&f, version_a, version_c, meta_c);
    let book = faux.resolve_path("[Book1]").expect("classeur truqué");
    let copie = NodeCopy::capture(faux.tree(), book).expect("capture");

    handle.new_project().expect("nouveau projet");
    let racine = instantane(&handle).tree.root();
    handle
        .execute(Command::InsertCopy {
            parent: racine,
            copy: copie,
            name: "Book1".into(),
        })
        .expect("coller le classeur, première requête du projet neuf");

    let s = instantane(&handle);
    // La coïncidence tient : la colonne collée porte l'identifiant de C, et
    // A, que le recalcul n'écrit pas, la version que le mémo a notée.
    assert!(
        s.tree.node(f.c).and_then(|node| node.as_column()).is_some(),
        "C collée sous l'identifiant que le mémo connaît"
    );
    assert_eq!(version(&s, f.a), version_a);
    assert_eq!(nombres(&s, f.a), some(&[5.0, 5.0, 5.0]));
    assert_eq!(nombres(&s, f.c), some(&[10.0, 10.0, 10.0]));
}

/// Une colonne ajoutée avec une version déjà portée (venue d'un autre
/// projet, d'un instantané d'avant un chargement, d'un fichier) ne doit pas
/// voir ce numéro resservir pour un autre de ses contenus : le compteur du
/// projet passe au-delà à l'insertion, comme pour un collage. Sinon, A
/// ajoutée en version 40 retombe sur 40 après quelques écritures, avec un
/// autre contenu, et C = A*2 se croit à jour (faux « propre » trouvé en
/// relecture de #193).
fn colonne_ajoutee_avec_sa_version(mode: FormulaRecalc) -> (Vec<Option<f64>>, u64) {
    const V: u64 = 40;
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let session = Session::spawn_with_recalc(project, None, mode).expect("acteur démarré");
    let handle = session.handle();
    let ajouter = |nom: &str, colonne: Column| {
        handle
            .execute(Command::AddColumn {
                sheet,
                index: None,
                name: Some(nom.into()),
                column: colonne,
            })
            .expect("colonne");
    };
    ajouter(
        "A",
        Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0, 2.0, 3.0]),
            ColumnMeta::default(),
            V,
        )
        .expect("A en version 40"),
    );
    ajouter("B", Column::from_f64(&[0.0]));
    let mut c = Column::new(ColumnType::Numeric);
    c.set_meta(ColumnMeta {
        formula: Some("A*2".into()),
        ..Default::default()
    });
    ajouter("C", c);
    let ids = instantane(&handle)
        .tree
        .children(sheet)
        .expect("feuille")
        .to_vec();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    assert_eq!(nombres(&instantane(&handle), c), some(&[2.0, 4.0, 6.0]));

    // Des saisies dans B, que C ne lit pas, amènent le compteur jusqu'à V.
    while version(&instantane(&handle), b) < V - 1 {
        handle.execute(saisir(b, 0, &[1.0])).expect("saisie dans B");
    }
    handle
        .execute(saisir(a, 0, &[100.0]))
        .expect("saisie dans A");
    let s = instantane(&handle);
    (nombres(&s, c), version(&s, a))
}

#[test]
fn une_colonne_ajoutee_avec_sa_version_ne_fait_pas_resservir_ce_numero() {
    let (complet, _) = colonne_ajoutee_avec_sa_version(FormulaRecalc::Full);
    assert_eq!(complet, some(&[200.0, 4.0, 6.0]), "oracle");
    let (cible, version_a) = colonne_ajoutee_avec_sa_version(FormulaRecalc::Targeted);
    assert!(version_a > 40, "A ne retombe pas sur 40 : {version_a}");
    assert_eq!(cible, complet, "le ciblé rend le complet");
}

// -- formule trop profonde (#289) ---------------------------------------------

/// Chaîne de `termes` fois `A`, jointe par `+`. Tailles codées en dur dans
/// les tests : 1001 termes passent, 1002 sont refusées (`MAX_DEPTH = 1000`).
fn somme_de_a(termes: usize) -> String {
    vec!["A"; termes].join("+")
}

/// Une formule refusée au parse par `FormulaError::TooDeep` n'est pas une
/// panne de l'acteur : la colonne est un bloc non calculé de la longueur de
/// la feuille (issue `Unparsable`), l'acteur répond encore, et la formule
/// corrigée est recalculée aussitôt (invariant 11) — dans les deux modes.
#[test]
fn une_formule_trop_profonde_est_un_bloc_non_calcule_puis_se_corrige() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = demarrer(mode);

        // Juste sous le plafond : calculée, sur le fil de l'acteur (2 Mio).
        handle
            .execute(formule(f.c, &somme_de_a(1001)))
            .expect("1001 termes");
        assert_eq!(
            nombres(&instantane(&handle), f.c),
            some(&[1001.0, 2002.0, 3003.0]),
            "{mode:?}"
        );

        // Un terme de trop : refusée, bloc non calculé de 3 lignes.
        handle
            .execute(formule(f.c, &somme_de_a(1002)))
            .expect("1002 termes");
        let s = instantane(&handle);
        assert_eq!(nombres(&s, f.c), vec![None, None, None], "{mode:?}");
        assert!(tout_non_calcule(&s, f.c), "{mode:?}");

        // Corrigée : recalculée, bien que la colonne soit déjà « à jour ».
        handle.execute(formule(f.c, "A+B")).expect("corrigée");
        assert_eq!(
            nombres(&instantane(&handle), f.c),
            some(&[11.0, 22.0, 33.0]),
            "{mode:?}"
        );

        // Trop profonde par les parenthèses, puis par les signes, sans que
        // l'acteur ne tombe : à chaque fois un bloc non calculé.
        for trop_profonde in [
            format!("{}A{}", "(".repeat(101), ")".repeat(101)),
            format!("{}A", "-".repeat(101)),
            format!("{}A", "-".repeat(100_000)),
        ] {
            handle
                .execute(formule(f.c, &trop_profonde))
                .expect("trop profonde");
            let s = instantane(&handle);
            assert!(tout_non_calcule(&s, f.c), "{mode:?}");
            assert_eq!(nombres(&s, f.c).len(), 3, "{mode:?}");
        }

        // L'annulation rend la dernière formule valide et ses valeurs.
        handle.undo().expect("annuler");
        handle.undo().expect("annuler");
        handle.undo().expect("annuler");
        assert_eq!(
            nombres(&instantane(&handle), f.c),
            some(&[11.0, 22.0, 33.0]),
            "{mode:?}"
        );
    }
}

/// Un bloc non calculé par `TooDeep` suit la longueur de la feuille comme
/// toute formule illisible, et une formule qui le lit est elle-même non
/// calculée tant qu'il l'est (le parse refusé n'entre pas dans le graphe),
/// puis se rattrape quand la source est corrigée.
#[test]
fn une_formule_trop_profonde_suit_la_feuille_et_rattrape_ses_lecteurs() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = demarrer(mode);
        handle
            .execute(formule(f.c, &somme_de_a(1002)))
            .expect("C trop profonde");
        handle.execute(formule(f.d, "C+1")).expect("D = C+1");
        let s = instantane(&handle);
        assert!(tout_non_calcule(&s, f.c), "{mode:?}");
        assert!(tout_non_calcule(&s, f.d), "{mode:?}");

        // La feuille grandit (B passe à 5 lignes) : le bloc suit.
        handle
            .execute(saisir(f.b, 4, &[1.0]))
            .expect("B descend à 5 lignes");
        let s = instantane(&handle);
        assert_eq!(nombres(&s, f.c).len(), 5, "{mode:?}");
        assert!(tout_non_calcule(&s, f.c), "{mode:?}");

        // C corrigée : D, qui la lit, suit dans la même passe.
        handle.execute(formule(f.c, "A*2")).expect("C corrigée");
        let s = instantane(&handle);
        assert_eq!(nombres(&s, f.c)[..3], some(&[2.0, 4.0, 6.0]), "{mode:?}");
        assert_eq!(nombres(&s, f.d)[..3], some(&[3.0, 5.0, 7.0]), "{mode:?}");
    }
}
