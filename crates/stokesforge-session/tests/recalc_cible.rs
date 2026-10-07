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
    Cell, Column, ColumnMeta, ColumnType, ColumnValues, MixedCell, NodeId, Project, ValueBlock,
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

/// Une deuxième passe immédiate ne réécrit rien : l'ordre des étapes du
/// recalcul (illisibles, types, cycles, ordre topologique) ne laisse aucune
/// colonne écrite après qu'une colonne qui la lit a pris son empreinte.
/// Sans cette propriété, chaque commande recalculerait une chaîne entière.
#[test]
fn une_chaine_recalculee_est_stable_a_la_commande_suivante() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
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
    assert_eq!(version(&avant, f.c), version(&apres, f.c));
    assert_eq!(version(&avant, f.d), version(&apres, f.d));
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
    // Feuille : B, C, D → « B » désigne C, vide, et D ne lit plus rien
    // de numérique.
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.d), vec![None, None, None]);

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
    // quatre lignes et allonge la feuille : la colonne-formule les garde
    // (voir `allonger_la_feuille_ailleurs_allonge_la_formule`).
    handle.undo().expect("annuler le changement de type");
    handle.undo().expect("annuler la suppression");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        vec![Some(2.0), Some(4.0), Some(6.0), None]
    );
}

/// Joue « allonger la feuille par D, puis annuler » et rend la colonne C
/// après chaque étape.
fn allonger_puis_annuler(mode: FormulaRecalc) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let (_session, handle, f) = demarrer(mode);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    handle
        .execute(saisir(f.d, 4, &[1.0]))
        .expect("D descend à 5 lignes");
    let allongee = nombres(&instantane(&handle), f.c);
    handle.undo().expect("annuler");
    (allongee, nombres(&instantane(&handle), f.c))
}

/// Allonger la feuille par une colonne qu'aucune formule ne lit allonge
/// quand même le résultat (composante « nombre de lignes »).
///
/// Après l'annulation, la colonne-formule garde ses cinq lignes : le nombre
/// de lignes de la feuille compte la colonne-formule elle-même, déjà
/// allongée. C'est le comportement d'avant #193, que le recalcul ciblé doit
/// reproduire tel quel — d'où la comparaison au mode complet plutôt qu'une
/// valeur écrite en dur.
#[test]
fn allonger_la_feuille_ailleurs_allonge_la_formule() {
    let (allongee, annulee) = allonger_puis_annuler(FormulaRecalc::Targeted);
    assert_eq!(allongee, vec![Some(2.0), Some(4.0), Some(6.0), None, None]);
    assert_eq!(
        (allongee, annulee),
        allonger_puis_annuler(FormulaRecalc::Full),
        "le ciblé rend exactement ce que rend le complet"
    );
}

/// Une formule illisible est réécrite à la bonne longueur quand la feuille
/// grandit (issue `Unparsable`, composante « nombre de lignes »), puis
/// calculée une fois corrigée.
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

    handle.execute(formule(f.c, "A*2")).expect("C corrigée");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        vec![Some(2.0), Some(4.0), Some(6.0), None, None]
    );
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

    // Un cycle stable ne se réécrit pas à chaque commande : ses membres ne
    // se renvoient pas leurs versions.
    handle.execute(sans_rapport(&s, 0)).expect("sans rapport");
    let apres = instantane(&handle);
    assert_eq!(version(&s, f.b), version(&apres, f.b));
    assert_eq!(version(&s, f.c), version(&apres, f.c));

    handle.undo().expect("annuler la commande sans rapport");
    handle.undo().expect("annuler la formule de B");
    assert_eq!(
        nombres(&instantane(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );
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

/// Supprimer une colonne-formule puis annuler : elle revient juste, et une
/// source modifiée entre-temps est prise en compte.
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

/// Une feuille dupliquée calcule ses propres formules sur ses propres
/// colonnes : le mémo est tenu par identifiant, pas par nom ni par lettre.
#[test]
fn une_feuille_dupliquee_a_ses_propres_formules() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
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
    assert_eq!(
        version(&s, f.c),
        version(&apres, f.c),
        "l'original n'est pas concerné"
    );
}

/// `Load` vide le mémo. Le fichier chargé est fabriqué pour que tout ce que
/// le mémo compare coïncide avec le projet courant — mêmes identifiants,
/// mêmes versions, même texte — alors que la source A a d'autres valeurs et
/// que C porte des valeurs fausses. Seul l'oubli du mémo force le recalcul :
/// un `.stf` écrit ailleurs (autre processus, version antérieure) n'a aucune
/// raison de partager la numérotation de versions de ce projet-ci.
#[test]
fn le_chargement_oublie_le_memo_du_projet_remplace() {
    let (_session, handle, f) = demarrer(FormulaRecalc::Targeted);
    handle.execute(formule(f.c, "A*2")).expect("C = A*2");
    let s = instantane(&handle);
    let colonne = |id: NodeId| {
        s.tree
            .node(id)
            .and_then(|node| node.as_column())
            .expect("colonne")
            .clone()
    };
    let (vraie_a, vraie_c) = (colonne(f.a), colonne(f.c));
    assert_eq!(vraie_c.column_type(), ColumnType::Mixed);
    assert!(vraie_c.meta().formula.is_some());

    // Même construction, donc mêmes identifiants ; A et C reçoivent les
    // versions que le mémo a notées, avec un autre contenu (`add_column`
    // garde la version de la colonne qu'on lui donne).
    let mut faux = Project::new("Demo").expect("projet valide");
    let root = faux.root();
    faux.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = faux.resolve_path("[Book1]Sheet1").expect("feuille");
    let fausse_a = Column::from_block_with_version(
        ColumnType::Numeric,
        ValueBlock::Float(vec![5.0, 5.0, 5.0]),
        ColumnMeta::default(),
        vraie_a.version(),
    )
    .expect("A truquée");
    let fausse_c = Column::from_block_with_version(
        ColumnType::Mixed,
        ValueBlock::Mixed(vec![MixedCell::Number(0.0); 3]),
        vraie_c.meta().clone(),
        vraie_c.version(),
    )
    .expect("C truquée");
    let ids = [
        faux.add_column(sheet, None, Some("A"), fausse_a),
        faux.add_column(sheet, None, Some("B"), colonne(f.b)),
        faux.add_column(sheet, None, Some("C"), fausse_c),
        faux.add_column(sheet, None, Some("D"), colonne(f.d)),
    ]
    .map(|id| id.expect("colonne"));
    assert_eq!(ids, [f.a, f.b, f.c, f.d], "construction déterministe");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let chemin = dir.path().join("faux.stf");
    stokesforge_persistence::save(&faux, &chemin).expect("écrire le fichier");

    handle.load(&chemin).expect("charger");
    let s = instantane(&handle);
    assert_eq!(nombres(&s, f.a), some(&[5.0, 5.0, 5.0]));
    assert_eq!(nombres(&s, f.c), some(&[10.0, 10.0, 10.0]));
}
