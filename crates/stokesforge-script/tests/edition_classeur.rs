//! Édition de classeur depuis `pyst` (#331, lot 2, S5) : `insert_rows`,
//! `delete_rows`, `set_column_type`, `move`, `duplicate`, `set_note`. Chaque
//! appel passe par une commande annulable ; `pyst.undo()` rend l'état d'avant.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, MixedCell, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

/// `[Book1]Sheet1` : `A` = 1, 2, 3 ; `B` = 4, 5, 6 ; `C` mixte = "x", vide, 3.
/// Un dossier `Dossier` et une note `Note1` à la racine.
fn banc() -> Banc {
    let mut project = Project::new("Édition").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("C"),
            Column::from_mixed(&[
                MixedCell::Text("x".into()),
                MixedCell::Empty,
                MixedCell::Number(3.0),
            ]),
        )
        .unwrap();
    project.add_folder(project.root(), "Dossier").unwrap();
    project
        .add_note(project.root(), Some("Note1"), "avant")
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());
    Banc {
        engine,
        session,
        _actor: actor,
    }
}

fn run(banc: &Banc, source: &str) -> ScriptOutcome {
    let outcome = banc.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

const HANDLES: &str = "import math\n\
    sheet = pyst.resolve('[Book1]Sheet1')\n\
    a = pyst.resolve('[Book1]Sheet1!A')\n\
    b = pyst.resolve('[Book1]Sheet1!B')\n\
    c = pyst.resolve('[Book1]Sheet1!C')\n";

/// Exécute `body` après les poignées usuelles.
fn script(banc: &Banc, body: &str) -> ScriptOutcome {
    run(banc, &format!("{HANDLES}{body}"))
}

/// Le script doit lever `kind` ; rend le message.
fn refused(banc: &Banc, kind: &str, body: &str) -> String {
    let source = format!(
        "{HANDLES}try:\n    {body}\nexcept pyst.{kind} as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no {kind}')\nprint(caught)"
    );
    run(banc, &source).stdout.trim().to_owned()
}

#[test]
fn insert_rows_insere_des_lignes_vides_et_s_annule() {
    let b = banc();
    script(
        &b,
        "pyst.insert_rows(sheet, 1, 2)\n\
         va = pyst.values(a)\n\
         assert len(va) == 5 and va[0] == 1.0 and va[3] == 2.0 and va[4] == 3.0, va\n\
         assert math.isnan(va[1]) and math.isnan(va[2])\n\
         vc = pyst.values(c)\n\
         assert vc == ('x', None, None, None, 3.0), vc\n\
         pyst.insert_rows(sheet, 5)\n\
         assert len(pyst.values(b)) == 6\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\nassert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         assert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn les_ecritures_de_lignes_ne_perimment_pas_la_version_lue_par_le_script() {
    let b = banc();
    // Lire `a`, changer ses lignes, puis réécrire : pas de ConcurrentWriteError.
    script(
        &b,
        "pyst.values(a)\npyst.insert_rows(sheet, 0)\npyst.set_values(a, [9.0, 9.0, 9.0, 9.0])\n\
         pyst.values(b)\npyst.delete_rows(sheet, 0)\npyst.set_values(b, [7.0])\n\
         pyst.values(c)\npyst.set_column_type(c, 'mixed')\npyst.set_values(c, ['z'])\n",
    );
}

#[test]
fn insert_rows_refuse_ce_qui_n_est_pas_une_feuille_ou_deborde() {
    let b = banc();
    let message = refused(&b, "InvalidValueError", "pyst.insert_rows(a, 0)");
    assert!(message.contains("not a worksheet"), "{message}");
    refused(&b, "InvalidValueError", "pyst.insert_rows(sheet, 10, 1)");
    let message = refused(&b, "InvalidValueError", "pyst.insert_rows(sheet, 0, 10**9)");
    assert!(message.contains("exceeds the limit"), "{message}");
    refused(
        &b,
        "InvalidValueError",
        "pyst.delete_rows(pyst.resolve('[Note1]'), 0, 1)",
    );
    script(&b, "assert pyst.values(a) == (1.0, 2.0, 3.0)");
}

#[test]
fn delete_rows_supprime_des_lignes_et_s_annule() {
    let b = banc();
    script(
        &b,
        "pyst.delete_rows(sheet, 0)\n\
         assert pyst.values(a) == (2.0, 3.0) and pyst.values(b) == (5.0, 6.0)\n\
         assert pyst.values(c) == (None, 3.0)\n\
         pyst.delete_rows(sheet, 0, 2)\n\
         assert pyst.values(a) == ()\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\nassert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         assert pyst.values(b) == (4.0, 5.0, 6.0)\nassert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn set_column_type_convertit_et_s_annule() {
    let b = banc();
    script(
        &b,
        "pyst.set_column_type(a, 'mixed')\n\
         assert pyst.info(a)['column']['type'] == 'mixed'\n\
         assert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         pyst.set_column_type(c, 'numeric')\n\
         assert pyst.info(c)['column']['type'] == 'numeric'\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         assert pyst.info(a)['column']['type'] == 'numeric'\n\
         assert pyst.info(c)['column']['type'] == 'mixed'\n\
         assert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn set_column_type_refuse_un_type_inconnu_ou_un_non_colonne() {
    let b = banc();
    let message = refused(&b, "InvalidValueError", "pyst.set_column_type(a, 'text')");
    assert!(message.contains("unknown column type"), "{message}");
    refused(
        &b,
        "InvalidValueError",
        "pyst.set_column_type(sheet, 'mixed')",
    );
    script(&b, "assert pyst.info(a)['column']['type'] == 'numeric'");
}

#[test]
fn move_reordonne_et_deplace_sans_changer_la_poignee() {
    let b = banc();
    script(
        &b,
        "pyst.move(a, sheet)\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['B', 'C', 'A']\n\
         pyst.move(c, sheet, 0)\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['C', 'B', 'A']\n\
         note = pyst.resolve('[Note1]')\n\
         folder = pyst.resolve('Dossier')\n\
         pyst.move(note, folder)\n\
         assert pyst.info(pyst.info(note)['parent'])['name'] == 'Dossier'\n\
         assert pyst.resolve('/Dossier/[Note1]') is not None\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['A', 'B', 'C']\n\
         assert pyst.info(pyst.info(pyst.resolve('[Note1]'))['parent'])['name'] == pyst.info(pyst.root())['name']\n",
    );
}

#[test]
fn move_refuse_un_parent_inadapte_un_cycle_ou_un_index_hors_limites() {
    let b = banc();
    for body in [
        "pyst.move(a, pyst.resolve('Dossier'))",
        "pyst.move(pyst.resolve('Dossier'), pyst.resolve('Dossier'))",
        "pyst.move(a, sheet, 7)",
        "pyst.move(pyst.root(), pyst.resolve('Dossier'))",
    ] {
        refused(&b, "InvalidValueError", body);
    }
    script(
        &b,
        "assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['A', 'B', 'C']",
    );
}

#[test]
fn duplicate_copie_une_feuille_juste_apres_l_originale() {
    let b = banc();
    script(
        &b,
        "copy = pyst.duplicate(sheet)\n\
         book = pyst.info(sheet)['parent']\n\
         assert [pyst.info(h)['name'] for h in pyst.children(book)] == ['Sheet1', 'Sheet1 copy 1']\n\
         ca = pyst.children(copy)[0]\n\
         assert pyst.values(ca) == (1.0, 2.0, 3.0)\n\
         pyst.set_values(ca, [0.0])\n\
         assert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         named = pyst.duplicate(sheet, name='Autre')\n\
         assert pyst.info(named)['name'] == 'Autre'\n\
         again = pyst.duplicate(sheet)\n\
         assert pyst.info(again)['name'] == 'Sheet1 copy 2'\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         book = pyst.info(sheet)['parent']\n\
         assert [pyst.info(h)['name'] for h in pyst.children(book)] == ['Sheet1']\n",
    );
}

#[test]
fn duplicate_copie_aussi_une_note_et_refuse_la_racine_ou_un_nom_pris() {
    let b = banc();
    script(
        &b,
        "note = pyst.resolve('[Note1]')\n\
         copy = pyst.duplicate(note)\n\
         assert pyst.info(copy)['name'] == 'Note1 copy 1' and pyst.info(copy)['kind'] == 'note'\n\
         pyst.set_note(copy, 'changé')\n",
    );
    refused(&b, "InvalidValueError", "pyst.duplicate(pyst.root())");
    refused(
        &b,
        "InvalidValueError",
        "pyst.duplicate(pyst.resolve('[Note1]'), name='Note1')",
    );
    refused(
        &b,
        "InvalidValueError",
        "pyst.duplicate(pyst.resolve('[Note1]'), name='')",
    );
}

#[test]
fn set_note_remplace_le_texte_et_s_annule() {
    let b = banc();
    let text = |b: &Banc| {
        let snapshot = b.session.snapshot().unwrap();
        let id = snapshot.tree.resolve("[Note1]").unwrap();
        match snapshot.tree.node(id).unwrap().kind() {
            stokesforge_core::NodeKind::Note { text, .. } => text.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(text(&b), "avant");
    script(
        &b,
        "pyst.set_note(pyst.resolve('[Note1]'), 'ligne 1\\nligne 2')",
    );
    assert_eq!(text(&b), "ligne 1\nligne 2");
    script(&b, "assert pyst.undo() is True");
    assert_eq!(text(&b), "avant");
    let message = refused(&b, "InvalidValueError", "pyst.set_note(a, 'x')");
    assert!(message.contains("not a note"), "{message}");
}
