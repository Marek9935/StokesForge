//! Rejoue, en bibliothèque, le **critère de fin** de tout le socle :
//!
//! > depuis un binaire empaqueté sur Linux : une cellule éditée s'annule par
//! > `Ctrl+Z` et se rétablit par `Ctrl+Y` ; `Ctrl+S` écrit un fichier que
//! > `Ctrl+O` rouvre à l'identique ; et un script Python lit une colonne
//! > sans copie, en écrit une autre via le bus — et `Ctrl+Z` annule ce que le
//! > script vient d'écrire.
//!
//! Toute la logique vit ici, pas dans `src/bin/stokesforge-selftest.rs` : c'est ce
//! qui permet à [`tests/selftest.rs`](../../tests/selftest.rs) d'exercer
//! exactement le même chemin qu'un utilisateur qui lance le binaire depuis la
//! distribution empaquetée, sous le harnais `cargo test` habituel.
//!
//! [`run_selftest`] construit lui-même le projet de démonstration (`Book1` /
//! `Sheet1` / colonnes `A` et `B`), fait tourner l'acteur et le fil de script
//! le temps de la vérification, puis arrête proprement les deux. Il ne
//! panique jamais : toute défaillance — y compris le démarrage de
//! l'interpréteur lui-même — devient un [`SelfTestStep`] en échec, jamais une
//! interruption du programme.

use std::path::PathBuf;

use stokesforge_commands::Command;
use stokesforge_core::{Column, CoreResult, NodeId, Project, Tree, ValueBlock};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_session::{Session, SessionHandle};

use crate::engine::ScriptEngine;

/// Noms des sept étapes, dans l'ordre où [`run_selftest`] les rejoue. Sert à
/// la fois à nommer une étape réussie et à remplir les étapes restées
/// **non vérifiées** quand une défaillance en amont empêche de les atteindre.
const STEP_NAMES: [&str; 7] = [
    "Interpréteur embarqué et isolé",
    "Édition, annulation, rétablissement",
    "Aller-retour disque",
    "Lecture sans copie depuis un script",
    "Écriture par le bus, annulée depuis l'hôte",
    "Annulation depuis Python",
    "Sortie et erreurs d'un script",
];

/// Une étape de la vérification, et son verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfTestStep {
    /// Nom de l'étape, un de [`STEP_NAMES`].
    pub name: String,
    /// Vrai si l'étape a réussi.
    pub passed: bool,
    /// Explication lisible du verdict : ce qui a été vérifié si l'étape
    /// passe, ce qui a été observé au lieu de l'attendu sinon.
    pub detail: String,
}

impl SelfTestStep {
    /// Étape réussie.
    fn ok(name: &str, detail: String) -> Self {
        Self {
            name: name.to_owned(),
            passed: true,
            detail,
        }
    }

    /// Étape en échec.
    fn fail(name: &str, detail: String) -> Self {
        Self {
            name: name.to_owned(),
            passed: false,
            detail,
        }
    }
}

/// Le compte rendu complet d'une vérification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfTestReport {
    /// Les étapes rejouées, dans l'ordre de [`STEP_NAMES`].
    pub steps: Vec<SelfTestStep>,
    /// Ce qui a été vérifié : préfixe embarqué, versions, isolation,
    /// nombre d'entrées de `sys.path`, exécutable courant. Une paire
    /// `(étiquette, valeur)` par ligne, dans l'ordre où elle a été établie.
    pub environment: Vec<(String, String)>,
}

impl SelfTestReport {
    /// Vrai si toutes les étapes sont passées.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.steps.iter().all(|step| step.passed)
    }

    /// Rendu textuel, en français, une ligne par étape, préfixée d'un
    /// marqueur de réussite (`[OK]`) ou d'échec (`[ÉCHEC]`).
    #[must_use]
    pub fn render(&self) -> String {
        self.steps
            .iter()
            .map(|step| {
                let marker = if step.passed { "[OK]   " } else { "[ÉCHEC]" };
                format!("{marker} {} — {}", step.name, step.detail)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Rejoue le critère de fin du socle de bout en bout. Ne panique jamais :
/// une étape qui échoue est un [`SelfTestStep`] en échec, pas une
/// interruption du programme.
///
/// `python` dit où trouver l'interpréteur embarqué — typiquement
/// [`EmbeddedPython::locate`] depuis le binaire, ou
/// [`EmbeddedPython::with_home`] pointé sur le préfixe vendorisé depuis un
/// test.
#[must_use]
pub fn run_selftest(python: EmbeddedPython) -> SelfTestReport {
    // Les deux informations qui ne dépendent d'aucune étape : toujours
    // présentes, même si tout le reste échoue avant d'avoir pu s'exécuter.
    let mut environment = vec![
        (
            "préfixe Python embarqué".to_owned(),
            python.home().display().to_string(),
        ),
        (
            "exécutable courant".to_owned(),
            match std::env::current_exe() {
                Ok(path) => path.display().to_string(),
                Err(error) => format!("indisponible : {error}"),
            },
        ),
    ];
    let mut steps: Vec<SelfTestStep> = Vec::new();

    let (project, column_a, column_b) = match build_demo_project() {
        Ok(value) => value,
        Err(error) => {
            return abort(
                steps,
                environment,
                format!("préparation du projet de démonstration impossible : {error}"),
            );
        }
    };

    let actor = match Session::spawn(project, None) {
        Ok(actor) => actor,
        Err(error) => {
            return abort(
                steps,
                environment,
                format!("l'acteur de session n'a pas pu démarrer : {error}"),
            );
        }
    };
    let session = actor.handle();

    let engine = match ScriptEngine::spawn(session.clone(), python.clone()) {
        Ok(engine) => engine,
        Err(error) => {
            let _ = actor.shutdown();
            return abort(
                steps,
                environment,
                format!("le moteur de script n'a pas pu démarrer : {error}"),
            );
        }
    };

    steps.push(step_interpreter(&python, &mut environment));
    steps.push(step_edit_undo_redo(&session, column_a));
    steps.push(step_disk_roundtrip(&session, column_b));
    steps.push(step_zero_copy_read(&engine, &session, column_a));
    steps.push(step_bus_write_host_undo(&engine, &session, column_b));
    steps.push(step_undo_from_python(&engine, &session, column_b));
    steps.push(step_output_and_errors(&engine));

    // Arrêt best-effort : un arrêt qui se passe mal ne remet pas en cause ce
    // que les étapes ci-dessus ont déjà, positivement, vérifié.
    let _ = engine.shutdown();
    let _ = actor.shutdown();

    SelfTestReport { steps, environment }
}

/// Construit le projet de démonstration : `Book1`/`Sheet1`, colonnes
/// `A = [1.0, 2.0, 3.0]` et `B = [10.0, 20.0, 30.0]`.
fn build_demo_project() -> CoreResult<(Project, NodeId, NodeId)> {
    let mut project = Project::new("StokesForgeSelfTest")?;
    let root = project.root();
    project.add_workbook(root, Some("Book1"))?;
    let sheet = project.resolve_path("[Book1]Sheet1")?;
    let column_a =
        project.add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))?;
    let column_b = project.add_column(
        sheet,
        None,
        Some("B"),
        Column::from_f64(&[10.0, 20.0, 30.0]),
    )?;
    Ok((project, column_a, column_b))
}

/// Termine le compte rendu quand une défaillance survenue **avant** la
/// première étape empêche toute vérification : pousse une étape en échec
/// pour l'interpréteur (rien n'a pu être vérifié sans lui) et marque les six
/// autres comme non atteintes, sans jamais laisser le compte rendu
/// incomplet.
fn abort(
    mut steps: Vec<SelfTestStep>,
    environment: Vec<(String, String)>,
    detail: String,
) -> SelfTestReport {
    steps.push(SelfTestStep::fail(STEP_NAMES[0], detail));
    for name in &STEP_NAMES[1..] {
        steps.push(SelfTestStep::fail(
            name,
            "non atteinte : une défaillance en amont a empêché toute vérification".to_owned(),
        ));
    }
    SelfTestReport { steps, environment }
}

/// Étape 1 : version de l'interpréteur et de numpy, isolation, et
/// **aucune** entrée de `sys.path` hors du préfixe embarqué. Alimente aussi
/// `environment` avec ces quatre informations.
fn step_interpreter(
    python: &EmbeddedPython,
    environment: &mut Vec<(String, String)>,
) -> SelfTestStep {
    let name = STEP_NAMES[0];
    let mut problems: Vec<String> = Vec::new();

    match interpreter::python_version() {
        Ok(version) => environment.push(("version de Python".to_owned(), version)),
        Err(error) => {
            environment.push((
                "version de Python".to_owned(),
                format!("indisponible : {error}"),
            ));
            problems.push(format!("version de Python indisponible : {error}"));
        }
    }

    match interpreter::numpy_version() {
        Ok(version) => environment.push(("version de numpy".to_owned(), version)),
        Err(error) => {
            environment.push((
                "version de numpy".to_owned(),
                format!("indisponible : {error}"),
            ));
            problems.push(format!("version de numpy indisponible : {error}"));
        }
    }

    match interpreter::sys_flags_isolated() {
        Ok(true) => environment.push(("sys.flags.isolated".to_owned(), "vrai".to_owned())),
        Ok(false) => {
            environment.push(("sys.flags.isolated".to_owned(), "faux".to_owned()));
            problems
                .push("sys.flags.isolated est faux : l'interpréteur n'est pas isolé".to_owned());
        }
        Err(error) => {
            environment.push((
                "sys.flags.isolated".to_owned(),
                format!("indisponible : {error}"),
            ));
            problems.push(format!("sys.flags.isolated indisponible : {error}"));
        }
    }

    match interpreter::sys_path() {
        Ok(paths) => {
            environment.push(("entrées de sys.path".to_owned(), paths.len().to_string()));
            let intrus: Vec<&str> = paths
                .iter()
                .filter(|entry| !python.contains_path(entry))
                .map(String::as_str)
                .collect();
            if !intrus.is_empty() {
                problems.push(format!(
                    "sys.path contient {} entrée(s) hors du préfixe embarqué : {}",
                    intrus.len(),
                    intrus.join(", ")
                ));
            }
        }
        Err(error) => {
            environment.push((
                "entrées de sys.path".to_owned(),
                format!("indisponible : {error}"),
            ));
            problems.push(format!("sys.path indisponible : {error}"));
        }
    }

    if problems.is_empty() {
        SelfTestStep::ok(
            name,
            "version, isolation et sys.path exhaustif (aucune entrée hors du préfixe embarqué), \
             tous vérifiés"
                .to_owned(),
        )
    } else {
        SelfTestStep::fail(name, problems.join(" ; "))
    }
}

/// Valeur de la première ligne d'une colonne numérique, ou `None` si la
/// colonne est absente ou n'est pas numérique.
fn first_float(tree: &Tree, column: NodeId) -> Option<f64> {
    tree.node(column)?
        .as_column()?
        .float_values()?
        .first()
        .copied()
}

/// Valeurs courantes d'une colonne numérique, lues côté Rust via un
/// instantané frais.
fn column_values(session: &SessionHandle, column: NodeId) -> Result<Vec<f64>, String> {
    let snapshot = session
        .snapshot()
        .map_err(|error| format!("instantané impossible : {error}"))?;
    let node = snapshot
        .tree
        .node(column)
        .ok_or_else(|| format!("{column} n'existe plus dans le projet"))?;
    node.as_column()
        .and_then(|col| col.float_values())
        .map(|values| values.as_slice().to_vec())
        .ok_or_else(|| format!("{column} n'est pas une colonne numérique"))
}

/// Étape 2 : une écriture par le bus s'annule et se rétablit, bit à bit.
///
/// Termine par une seconde annulation qui ramène l'arbre à l'état pristine
/// d'avant cette étape : les étapes suivantes supposent `A = [1.0, 2.0, 3.0]`.
fn step_edit_undo_redo(session: &SessionHandle, column_a: NodeId) -> SelfTestStep {
    let name = STEP_NAMES[1];

    let before = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("instantané initial impossible : {error}"))
        }
    };

    if let Err(error) = session.execute(Command::SetCells {
        column: column_a,
        start: 0,
        values: ValueBlock::Float(vec![99.0]),
        expected_version: None,
    }) {
        return SelfTestStep::fail(name, format!("écriture par le bus refusée : {error}"));
    }

    let after_edit = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("instantané après édition impossible : {error}"),
            )
        }
    };
    if first_float(&after_edit, column_a) != Some(99.0) {
        return SelfTestStep::fail(
            name,
            format!(
                "valeur écrite absente : attendu 99.0, obtenu {:?}",
                first_float(&after_edit, column_a)
            ),
        );
    }

    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(name, "annulation sans effet : rien à annuler".to_owned())
        }
        Err(error) => return SelfTestStep::fail(name, format!("annulation refusée : {error}")),
    }
    let after_undo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("instantané après annulation impossible : {error}"),
            )
        }
    };
    if after_undo != before {
        return SelfTestStep::fail(
            name,
            "après annulation, l'arbre diffère bit à bit de l'état d'avant l'édition".to_owned(),
        );
    }

    match session.redo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(
                name,
                "rétablissement sans effet : rien à rétablir".to_owned(),
            )
        }
        Err(error) => return SelfTestStep::fail(name, format!("rétablissement refusé : {error}")),
    }
    let after_redo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("instantané après rétablissement impossible : {error}"),
            )
        }
    };
    if after_redo != after_edit {
        return SelfTestStep::fail(
            name,
            "après rétablissement, l'arbre diffère bit à bit de l'état d'après l'édition"
                .to_owned(),
        );
    }

    // Nettoyage : les étapes suivantes supposent A pristine.
    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(
                name,
                "annulation de nettoyage sans effet après vérification".to_owned(),
            )
        }
        Err(error) => {
            return SelfTestStep::fail(name, format!("annulation de nettoyage refusée : {error}"))
        }
    }
    let restored = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("instantané final impossible : {error}"))
        }
    };
    if restored != before {
        return SelfTestStep::fail(
            name,
            "l'état pristine n'a pas pu être restauré après la vérification".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "écriture par le bus, annulation et rétablissement bit à bit conformes".to_owned(),
    )
}

/// Garde de nettoyage : supprime le fichier temporaire à la fin de l'étape 3,
/// que celle-ci réussisse ou échoue.
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Étape 3 : aller-retour disque, égal bit à bit à ce qui a été sauvegardé —
/// y compris après une modification intermédiaire, que le rechargement doit
/// écraser entièrement.
fn step_disk_roundtrip(session: &SessionHandle, column_b: NodeId) -> SelfTestStep {
    let name = STEP_NAMES[2];

    let before = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("instantané initial impossible : {error}"))
        }
    };

    let path =
        std::env::temp_dir().join(format!("stokesforge-selftest-{}.stf", std::process::id()));
    let _guard = TempFileGuard(path.clone());

    if let Err(error) = session.save(&path) {
        return SelfTestStep::fail(name, format!("sauvegarde refusée : {error}"));
    }

    // Modifie autre chose que ce qui vient d'être sauvegardé : la relecture
    // doit tout écraser, y compris cette modification intermédiaire.
    if let Err(error) = session.execute(Command::SetCells {
        column: column_b,
        start: 0,
        values: ValueBlock::Float(vec![-1.0]),
        expected_version: None,
    }) {
        return SelfTestStep::fail(
            name,
            format!("modification intermédiaire refusée : {error}"),
        );
    }

    if let Err(error) = session.load(&path) {
        return SelfTestStep::fail(name, format!("rechargement refusé : {error}"));
    }

    let after = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("instantané après rechargement impossible : {error}"),
            )
        }
    };

    if after != before {
        return SelfTestStep::fail(
            name,
            "l'arbre rechargé diffère bit à bit de celui sauvegardé".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        format!(
            "aller-retour disque bit à bit conforme, y compris après une modification écrasée par \
             le rechargement ({})",
            path.display()
        ),
    )
}

/// Étape 4 : la vue numpy prise par un script pointe **exactement** sur le
/// buffer Rust de la colonne — pas une copie aux valeurs justes.
fn step_zero_copy_read(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_a: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[3];

    let rust_address = match session.snapshot() {
        Ok(snapshot) => match snapshot
            .tree
            .node(column_a)
            .and_then(|node| node.as_column())
            .and_then(|column| column.float_values())
        {
            Some(values) => values.as_ptr() as usize,
            None => {
                return SelfTestStep::fail(
                    name,
                    "colonne A introuvable ou non numérique côté Rust".to_owned(),
                )
            }
        },
        Err(error) => return SelfTestStep::fail(name, format!("instantané impossible : {error}")),
    };

    let outcome = match engine.run(
        "v = pyst.column(pyst.resolve(\"[Book1]Sheet1!A\"))\n\
         print(v.__array_interface__[\"data\"][0])\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("le moteur a refusé le script : {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("le script a échoué : {error}"));
    }

    let printed = outcome.stdout.trim();
    let numpy_address: usize = match printed.parse() {
        Ok(value) => value,
        Err(_) => {
            return SelfTestStep::fail(
                name,
                format!("adresse illisible dans la sortie du script : {printed:?}"),
            )
        }
    };

    if numpy_address != rust_address {
        return SelfTestStep::fail(
            name,
            format!(
                "numpy regarde une autre adresse que le Vec du modèle (rust=0x{rust_address:x}, \
                 numpy=0x{numpy_address:x}) : une copie silencieuse aux valeurs justes rendrait \
                 exactement ce symptôme"
            ),
        );
    }

    SelfTestStep::ok(
        name,
        format!("vue numpy sans copie confirmée : même adresse (0x{rust_address:x}) côté Rust et côté numpy"),
    )
}

/// Étape 5 : le script écrit `B` à partir de la vue de `A` par
/// `pyst.set_column`, et une annulation **côté hôte** — comme le ferait
/// `Ctrl+Z` dans la fenêtre — rétablit `B` et l'arbre entier, bit à bit.
fn step_bus_write_host_undo(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_b: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[4];

    let before_script = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("instantané initial impossible : {error}"))
        }
    };

    let outcome = match engine.run(
        "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
         b = pyst.resolve(\"[Book1]Sheet1!B\")\n\
         pyst.set_column(b, pyst.column(a))\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("le moteur a refusé le script : {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("le script a échoué : {error}"));
    }

    match column_values(session, column_b) {
        Ok(values) if values == [1.0, 2.0, 3.0] => {}
        Ok(values) => {
            return SelfTestStep::fail(
                name,
                format!("B après le script : attendu [1.0, 2.0, 3.0], obtenu {values:?}"),
            )
        }
        Err(detail) => return SelfTestStep::fail(name, detail),
    }

    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(
                name,
                "l'annulation côté hôte n'a rien trouvé à annuler".to_owned(),
            )
        }
        Err(error) => {
            return SelfTestStep::fail(name, format!("l'annulation côté hôte a échoué : {error}"))
        }
    }

    let after_undo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("instantané après annulation impossible : {error}"),
            )
        }
    };
    if after_undo != before_script {
        return SelfTestStep::fail(
            name,
            "après l'annulation côté hôte, l'arbre diffère bit à bit de celui d'avant le script"
                .to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "écriture par le bus annulée depuis l'hôte : B rétabli et arbre égal bit à bit à celui \
         d'avant le script"
            .to_owned(),
    )
}

/// Étape 6 : `pyst.undo()` défait ce que le script vient d'écrire, et un
/// second appel sur une pile vide rend `False` sans lever d'exception.
fn step_undo_from_python(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_b: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[5];

    let outcome = match engine.run(
        "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
         b = pyst.resolve(\"[Book1]Sheet1!B\")\n\
         pyst.set_column(b, pyst.column(a))\n\
         print(pyst.undo())\n\
         print(pyst.undo())\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("le moteur a refusé le script : {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("le script a échoué : {error}"));
    }

    let lines: Vec<&str> = outcome.stdout.lines().map(str::trim).collect();
    if lines != ["True", "False"] {
        return SelfTestStep::fail(
            name,
            format!("sortie attendue [\"True\", \"False\"], obtenue {lines:?}"),
        );
    }

    match column_values(session, column_b) {
        Ok(values) if values == [10.0, 20.0, 30.0] => {}
        Ok(values) => {
            return SelfTestStep::fail(
                name,
                format!(
                    "B après la double annulation : attendu [10.0, 20.0, 30.0], obtenu {values:?}"
                ),
            )
        }
        Err(detail) => return SelfTestStep::fail(name, detail),
    }

    SelfTestStep::ok(
        name,
        "pyst.undo() défait l'écriture du script, puis rend False sur une pile vide sans lever \
         d'exception"
            .to_owned(),
    )
}

/// Étape 7 : la sortie standard d'un script est capturée, une exception rend
/// une trace complète, et le moteur reste utilisable après.
fn step_output_and_errors(engine: &ScriptEngine) -> SelfTestStep {
    let name = STEP_NAMES[6];
    const MARKER: &str = "STOKESFORGE-SELFTEST-2f8e19a4";

    let printed = match engine.run(&format!("print(\"{MARKER}\")\n")) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("le moteur a refusé le script : {error}"))
        }
    };
    if let Some(error) = &printed.error {
        return SelfTestStep::fail(
            name,
            format!("un simple print ne doit pas échouer : {error}"),
        );
    }
    if !printed.stdout.contains(MARKER) {
        return SelfTestStep::fail(name, "le print du script n'a pas été capturé".to_owned());
    }

    let failing = match engine.run("1 / 0\n") {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("le moteur a refusé le script en échec : {error}"),
            )
        }
    };
    let trace = match &failing.error {
        Some(trace) => trace,
        None => {
            return SelfTestStep::fail(
                name,
                "une exception aurait dû être rendue par le script".to_owned(),
            )
        }
    };
    if !trace.contains("Traceback") || !trace.contains("ZeroDivisionError") {
        return SelfTestStep::fail(
            name,
            format!(
                "trace incomplète : attendu \"Traceback\" et \"ZeroDivisionError\", obtenu {trace:?}"
            ),
        );
    }

    let after = match engine.run("print(\"moteur encore utilisable\")\n") {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("le moteur ne survit pas à un script en échec : {error}"),
            )
        }
    };
    if after.error.is_some() || !after.stdout.contains("moteur encore utilisable") {
        return SelfTestStep::fail(
            name,
            "le script suivant l'erreur ne s'exécute pas normalement".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "print capturé, trace d'exception complète (type et \"Traceback\"), moteur utilisable après \
         l'erreur"
            .to_owned(),
    )
}
