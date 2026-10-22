//! Garde de l'API figée V1 de `pyst` (#331, lot 3).
//!
//! La liste de référence est la section « API figée V1 » de
//! `docs/python-et-script.md`, en blocs ```` ```pyst-api <genre> <nom> ````.
//! Ce test **lit ces blocs** (il n'en garde aucune copie) et les compare au
//! module réellement construit :
//!
//! - **surface** : noms publics et nature (fonction, classe, exception,
//!   sous-module) de `pyst` et `pyst.analysis`, et la signature que rend
//!   `inspect.signature` de chaque fonction (si elle échouait, le test
//!   passerait à vide : c'est donc vérifié) ; `Handle` n'a rien de public ;
//! - **options** : les clés acceptées par `plot`, `set_graph` et
//!   `set_column_meta` (le message de « clé inconnue » les énumère, depuis les
//!   constantes `PLOT_OPTIONS`, `SET_GRAPH_OPTIONS`, `COLUMN_META_FIELDS`) ;
//! - **retours** : les clés des dictionnaires rendus, sur un projet d'exemple ;
//! - **valeurs énumérées** : chaque valeur de la liste est acceptée (en entrée)
//!   ou rendue (en sortie), et une valeur hors liste est refusée.
//!
//! Un changement de surface exige une décision explicite et la mise à jour de
//! la liste, dans le même commit que le code ; le message d'échec le dit.
//! Il remplace l'ancien `la_surface_expose_vingt_huit_appels_et_quatre_erreurs`.
#![cfg(feature = "python")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::Session;

/// Ce que le message d'échec rappelle à qui touche à la surface.
const CONSIGNE: &str = "La surface `pyst` ne correspond plus a la liste figee \
\"API figee V1\" de docs/python-et-script.md (#331). Un changement de surface \
(appel, parametre, defaut, exception, cle, valeur enumeree) exige une DECISION \
explicite et la mise a jour de cette liste dans le meme commit que le code.";

// --- La liste, lue dans la doc ------------------------------------------------

struct Bloc {
    genre: String,
    nom: String,
    lignes: Vec<String>,
}

fn chemin_doc() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/python-et-script.md")
}

/// Tous les blocs ```` ```pyst-api <genre> <nom> ```` de la doc, dans l'ordre.
fn blocs() -> Vec<Bloc> {
    let texte = std::fs::read_to_string(chemin_doc()).expect("docs/python-et-script.md lisible");
    let mut blocs = Vec::new();
    let mut courant: Option<Bloc> = None;
    for ligne in texte.lines() {
        if let Some(bloc) = courant.as_mut() {
            if ligne.trim_end() == "```" {
                blocs.push(courant.take().expect("bloc ouvert"));
            } else if !ligne.trim().is_empty() {
                bloc.lignes.push(ligne.trim_end().to_owned());
            }
        } else if let Some(entete) = ligne.strip_prefix("```pyst-api ") {
            let mut mots = entete.split_whitespace();
            let genre = mots.next().expect("genre du bloc pyst-api").to_owned();
            let nom = mots.next().expect("nom du bloc pyst-api").to_owned();
            assert!(mots.next().is_none(), "en-tete de bloc inattendu : {ligne}");
            courant = Some(Bloc {
                genre,
                nom,
                lignes: Vec::new(),
            });
        }
    }
    assert!(courant.is_none(), "bloc pyst-api non ferme");
    blocs
}

/// Les lignes du bloc `genre nom`, sans doublon ; `None` si le bloc manque.
fn bloc(blocs: &[Bloc], genre: &str, nom: &str) -> Option<BTreeSet<String>> {
    let mut trouves = blocs.iter().filter(|b| b.genre == genre && b.nom == nom);
    let premier = trouves.next()?;
    assert!(
        trouves.next().is_none(),
        "bloc pyst-api {genre} {nom} present deux fois"
    );
    let lignes: BTreeSet<String> = premier.lignes.iter().cloned().collect();
    assert_eq!(
        lignes.len(),
        premier.lignes.len(),
        "doublon dans le bloc pyst-api {genre} {nom}"
    );
    Some(lignes)
}

fn noms(blocs: &[Bloc], genre: &str) -> BTreeSet<String> {
    blocs
        .iter()
        .filter(|b| b.genre == genre)
        .map(|b| b.nom.clone())
        .collect()
}

/// Ajoute à `ecarts` ce qui manque et ce qui dépasse entre `attendu` (la doc)
/// et `reel` (le module).
fn comparer(
    ecarts: &mut Vec<String>,
    titre: &str,
    attendu: Option<BTreeSet<String>>,
    reel: &BTreeSet<String>,
) {
    let Some(attendu) = attendu else {
        ecarts.push(format!("{titre} : bloc absent de la doc"));
        return;
    };
    let en_trop: Vec<_> = reel.difference(&attendu).collect();
    let manquants: Vec<_> = attendu.difference(reel).collect();
    if !en_trop.is_empty() {
        ecarts.push(format!(
            "{titre} : dans le code, pas dans la doc : {en_trop:?}"
        ));
    }
    if !manquants.is_empty() {
        ecarts.push(format!(
            "{titre} : dans la doc, pas dans le code : {manquants:?}"
        ));
    }
}

fn exiger_sans_ecart(ecarts: Vec<String>) {
    assert!(
        ecarts.is_empty(),
        "{CONSIGNE}\n  - {}",
        ecarts.join("\n  - ")
    );
}

// --- Le banc --------------------------------------------------------------------

struct Banc {
    engine: Arc<ScriptEngine>,
    _actor: Session,
}

/// `[Book1]Sheet1` : `A` = 1, 2, 3 et `B` = 4, 5, 6 ; un dossier, une matrice
/// et une note à la racine (les natures que `pyst` ne sait pas créer).
fn banc() -> Banc {
    let mut project = Project::new("Surface").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .unwrap();
    project.add_folder(root, "Dossier").unwrap();
    project.add_matrix(root, Some("Mat")).unwrap();
    project.add_note(root, Some("Note1"), "texte").unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(actor.handle(), EmbeddedPython::with_home(prefix)).unwrap());
    Banc {
        engine,
        _actor: actor,
    }
}

/// Préparation commune des scripts : poignées, colonnes longues (lissage), un
/// graphe `Gen`, un dossier temporaire.
const PREAMBLE: &str = "
import os, tempfile
a = pyst.resolve('[Book1]Sheet1!A')
b = pyst.resolve('[Book1]Sheet1!B')
sh = pyst.resolve('[Book1]Sheet1')
big = pyst.create_column(sh)
pyst.set_values(big, [float((i * i) % 7) for i in range(30)])
bigx = pyst.create_column(sh)
pyst.set_values(bigx, [float(i) for i in range(30)])
g = pyst.plot(b, a)
tmp = tempfile.mkdtemp()
";

/// Exécute `script` (après [`PREAMBLE`]) et rend ses lignes `ETIQUETTE|reste`
/// regroupées par étiquette.
fn sortie(banc: &Banc, script: &str) -> BTreeMap<String, Vec<String>> {
    let source = format!("{PREAMBLE}{script}");
    let outcome = banc.engine.run(&source).expect("moteur disponible");
    assert!(
        outcome.error.is_none(),
        "le script de garde a echoue : {:?}\n{}",
        outcome.error,
        outcome.stdout
    );
    let mut sortie: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ligne in outcome.stdout.lines() {
        if let Some((etiquette, reste)) = ligne.split_once('|') {
            sortie
                .entry(etiquette.to_owned())
                .or_default()
                .push(reste.to_owned());
        }
    }
    sortie
}

fn ensemble(liste: &str) -> BTreeSet<String> {
    liste
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn un<'a>(sortie: &'a BTreeMap<String, Vec<String>>, etiquette: &str) -> &'a str {
    let lignes = sortie
        .get(etiquette)
        .unwrap_or_else(|| panic!("etiquette {etiquette} absente de la sortie"));
    assert_eq!(lignes.len(), 1, "etiquette {etiquette} : {lignes:?}");
    &lignes[0]
}

/// Littéral Python (liste de chaînes) pour des valeurs d'identifiants.
fn liste_python(valeurs: &BTreeSet<String>) -> String {
    let elements: Vec<String> = valeurs.iter().map(|v| format!("'{v}'")).collect();
    format!("[{}]", elements.join(", "))
}

// --- 1. Noms, natures, signatures ----------------------------------------------

/// Deux défauts que PyO3 ne sait pas écrire (un tuple, une constante) : Python
/// rend `Ellipsis`. La doc porte la vraie valeur ; on la rapproche ici.
const DEFAUTS_ELLIPSE: &[(&str, &str)] = &[
    ("panel=(0, 0)", "panel=Ellipsis"),
    ("png_scale=4.0", "png_scale=Ellipsis"),
];

fn comme_python(ligne: &str) -> String {
    let mut ligne = ligne.to_owned();
    if ligne.starts_with("function ") {
        for (doc, python) in DEFAUTS_ELLIPSE {
            ligne = ligne.replace(doc, python);
        }
    }
    ligne
}

const SCRIPT_SURFACE: &str = "
import inspect
for modname, mod in (('pyst', pyst), ('pyst.analysis', pyst.analysis)):
    for n in sorted(dir(mod)):
        if n.startswith('_'):
            continue
        o = getattr(mod, n)
        if inspect.ismodule(o):
            ligne = 'module ' + n
        elif inspect.isclass(o) and issubclass(o, BaseException):
            ligne = ('exception ' if issubclass(o, Exception) else 'exception-hors-Exception ') + n
        elif inspect.isclass(o):
            ligne = 'class ' + n
        elif callable(o):
            try:
                ligne = 'function ' + n + str(inspect.signature(o))
            except Exception as e:
                ligne = 'function %s<signature illisible : %r>' % (n, e)
        else:
            ligne = 'autre ' + n
        print(modname + '|' + ligne)
sans_doc = [m.__name__ + '.' + n for m in (pyst, pyst.analysis) for n in dir(m)
            if not n.startswith('_') and callable(getattr(m, n)) and not getattr(getattr(m, n), '__doc__', None)]
sans_doc += [m.__name__ for m in (pyst, pyst.analysis) if not m.__doc__]
print('SANSDOC|' + ','.join(sans_doc))
print('HANDLE|' + ','.join(n for n in dir(pyst.Handle) if not n.startswith('_')))
";

#[test]
fn la_surface_pyst_est_celle_de_la_liste_figee() {
    let banc = banc();
    let blocs = blocs();
    let sortie = sortie(&banc, SCRIPT_SURFACE);
    let mut ecarts = Vec::new();

    for module in ["pyst", "pyst.analysis"] {
        let reel: BTreeSet<String> = sortie
            .get(module)
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default();
        // Une signature illisible ferait passer le test à vide : refus net.
        let illisibles: Vec<_> = reel.iter().filter(|l| l.contains("illisible")).collect();
        assert!(
            illisibles.is_empty(),
            "{CONSIGNE}\ninspect.signature echoue : {illisibles:?}"
        );
        let fonctions = reel.iter().filter(|l| l.starts_with("function ")).count();
        assert!(
            fonctions >= if module == "pyst" { 30 } else { 8 },
            "{module} : {fonctions} fonctions seulement, la garde passerait a vide"
        );
        let attendu = bloc(&blocs, "module", module)
            .map(|b| b.iter().map(|l| comme_python(l)).collect::<BTreeSet<_>>());
        comparer(&mut ecarts, &format!("module {module}"), attendu, &reel);
    }

    // L'ancien garde exigeait aussi une docstring à chaque appel (en français, S8).
    let sans_doc = un(&sortie, "SANSDOC");
    if !sans_doc.is_empty() {
        ecarts.push(format!("sans docstring : {sans_doc}"));
    }

    let publics = un(&sortie, "HANDLE");
    if !publics.is_empty() {
        ecarts.push(format!(
            "Handle doit rester opaque, attributs publics : {publics}"
        ));
    }
    exiger_sans_ecart(ecarts);
}

// --- 2. Clés d'options -----------------------------------------------------------

/// Pour chaque appel à options : la liste que donne le message de « clé
/// inconnue », et les clés de la doc que l'appel ne reconnaît pas.
const SCRIPT_OPTIONS: &str = "
import re
appels = {
    'plot': lambda k: pyst.plot(b, a, **{k: 0}),
    'set_graph': lambda k: pyst.set_graph(g, **{k: 0}),
    'set_column_meta': lambda k: pyst.set_column_meta(big, **{k: 0}),
}
doc = {'plot': @PLOT@, 'set_graph': @SET_GRAPH@, 'set_column_meta': @META@}
for nom, appel in appels.items():
    try:
        appel('cle_inexistante_zzz')
        print('ACCEPTEES|%s|acceptee sans erreur' % nom)
    except pyst.InvalidValueError as e:
        m = re.search(r'\\(accepted: (.*)\\)', str(e))
        print('ACCEPTEES|%s|%s' % (nom, m.group(1).replace(' ', '') if m else 'liste absente : ' + str(e)))
    rejetees = []
    for cle in doc[nom]:
        try:
            appel(cle)
        except pyst.InvalidValueError as e:
            if str(e).startswith(('unknown plot option', 'unknown graph option', 'unknown column metadata field')):
                rejetees.append(cle)
        except Exception:
            pass
    print('REJETEES|%s|%s' % (nom, ','.join(rejetees)))
";

#[test]
fn les_cles_d_options_sont_celles_de_la_liste_figee() {
    let banc = banc();
    let blocs = blocs();
    let mut ecarts = Vec::new();
    let mut script = SCRIPT_OPTIONS.to_owned();
    for (marque, appel) in [
        ("@PLOT@", "plot"),
        ("@SET_GRAPH@", "set_graph"),
        ("@META@", "set_column_meta"),
    ] {
        let cles = bloc(&blocs, "options", appel).unwrap_or_default();
        script = script.replace(marque, &liste_python(&cles));
    }
    let sortie = sortie(&banc, &script);

    for appel in ["plot", "set_graph", "set_column_meta"] {
        let attendu = bloc(&blocs, "options", appel);
        let annonce = sortie["ACCEPTEES"]
            .iter()
            .find_map(|l| l.strip_prefix(&format!("{appel}|")))
            .unwrap_or_else(|| panic!("ACCEPTEES|{appel} absent"));
        if annonce.contains("liste absente") || annonce.contains("sans erreur") {
            ecarts.push(format!("options {appel} : {annonce}"));
            continue;
        }
        comparer(
            &mut ecarts,
            &format!("options {appel} (liste du message d'erreur)"),
            attendu,
            &ensemble(annonce),
        );
        let rejetees = sortie["REJETEES"]
            .iter()
            .find_map(|l| l.strip_prefix(&format!("{appel}|")))
            .unwrap_or_default();
        if !rejetees.is_empty() {
            ecarts.push(format!(
                "options {appel} : cles de la doc que le code ne reconnait pas : {rejetees}"
            ));
        }
    }
    exiger_sans_ecart(ecarts);
}

// --- 3. Clés des dictionnaires rendus ------------------------------------------------

const SCRIPT_RETOURS: &str = "
def cles(d):
    return ','.join(sorted(d.keys()))
r = pyst.info(a)
print('info|' + cles(r))
print('info.column|' + cles(r['column']))
deux = pyst.plot([a, b], None)
print('series.item|' + cles(pyst.series(deux)[0]))
print('series_groups.item|' + cles(pyst.series_groups(deux)[0]))
r = pyst.analysis.baseline(b, a, method='line', points=[(1, 1), (3, 3)], keep_baseline=True)
print('analysis|' + cles(r))
print('analysis.baseline.columns|' + cles(r['columns']))
r = pyst.analysis.correct(b, a)
print('analysis.correct.columns|' + cles(r['columns']))
r = pyst.analysis.smooth(big, bigx, window=3)
print('analysis.smooth.columns|' + cles(r['columns']))
r = pyst.analysis.baseline([a, b], a, method='line', points=[(1, 1), (3, 3)])
print('analysis.batch|' + cles(r))
print('analysis.batch.item|' + cles(r['series'][0]))
print('readiness|' + cles(pyst.analysis.readiness('fit', model='linear')))
print('readiness.needs_points|' + cles(pyst.analysis.readiness('baseline', method='anchors')))
";

#[test]
fn les_cles_des_dictionnaires_rendus_sont_celles_de_la_liste_figee() {
    let banc = banc();
    let blocs = blocs();
    let sortie = sortie(&banc, SCRIPT_RETOURS);
    let mut ecarts = Vec::new();
    let mut verifies = BTreeSet::new();
    for (etiquette, lignes) in &sortie {
        // Les clés d'un lot de séries sont celles d'un résultat seul.
        let nom = if etiquette == "analysis.batch.item" {
            "analysis"
        } else {
            etiquette.as_str()
        };
        verifies.insert(nom.to_owned());
        comparer(
            &mut ecarts,
            &format!("retour {etiquette}"),
            bloc(&blocs, "returns", nom),
            &ensemble(&lignes[0]),
        );
    }
    // Aucun bloc « returns » de la doc ne doit échapper au test.
    let documentes = noms(&blocs, "returns");
    let oublies: Vec<_> = documentes.difference(&verifies).collect();
    if !oublies.is_empty() {
        ecarts.push(format!(
            "blocs `returns` de la doc que le test ne verifie pas : {oublies:?}"
        ));
    }
    exiger_sans_ecart(ecarts);
}

// --- 4. Valeurs énumérées ------------------------------------------------------------

/// Enumérations **acceptées en entrée** : le corps de `essai(v)` doit réussir
/// pour chaque valeur de la doc ; avec `refus`, une valeur inventée doit lever
/// `InvalidValueError`.
struct Entree {
    nom: &'static str,
    corps: &'static str,
    refus: bool,
}

const ENTREES: &[Entree] = &[
    Entree {
        nom: "column_type",
        corps: "c = pyst.create_column(sh, None, column_type=v)\n    pyst.set_column_type(c, v)",
        refus: true,
    },
    Entree {
        nom: "role",
        corps: "c = pyst.create_column(sh)\n    pyst.set_column_meta(c, role=v)\n    assert pyst.info(c)['column']['role'] == v",
        refus: true,
    },
    Entree {
        nom: "plot_type",
        corps: "pyst.plot(b, a, plot_type=v)",
        refus: true,
    },
    Entree {
        nom: "line_style",
        corps: "pyst.plot(b, a, line_style=v)",
        refus: true,
    },
    Entree {
        nom: "symbol",
        corps: "pyst.plot(b, a, symbol=v)",
        refus: true,
    },
    Entree {
        nom: "scale",
        corps: "pyst.plot(b, a, x_scale=v, y_scale=v)\n    pyst.set_graph(g, x_scale=v, y_scale=v)",
        refus: true,
    },
    Entree {
        nom: "axis",
        corps: "pyst.plot(b, a, axis=v)",
        refus: true,
    },
    Entree {
        nom: "legend_position",
        corps: "pyst.set_graph(g, legend_position=v)",
        refus: true,
    },
    Entree {
        nom: "shape_kind",
        corps: "pyst.add_shape(g, v, 0, 0, 1, 1)",
        refus: true,
    },
    Entree {
        nom: "export_extension",
        corps: "pyst.export_graph(g, os.path.join(tmp, 'f.' + v))",
        refus: true,
    },
    Entree {
        nom: "analysis_kind",
        corps: "pyst.analysis.readiness(v)",
        refus: true,
    },
    Entree {
        nom: "baseline_method",
        corps: "kw = {'constant': {'points': [(1.0, 1.0)]}, 'anchors': {'points': [(1.0, 1.0), (2.0, 2.0)]}, 'line': {'points': [(1.0, 1.0), (3.0, 3.0)]}, 'zones': {'zones': [(1.0, 2.0)]}}.get(v, {})\n    pyst.analysis.readiness('baseline', method=v, **kw)",
        refus: true,
    },
    Entree {
        nom: "baseline_op",
        corps: "pyst.analysis.readiness('baseline', method='anchors', points=[(1.0, 1.0), (2.0, 2.0)], op=v)",
        refus: true,
    },
    Entree {
        nom: "integrate_under",
        corps: "pyst.analysis.readiness('integrate', under=v)",
        refus: true,
    },
    Entree {
        nom: "smooth_method",
        corps: "pyst.analysis.smooth(big, bigx, method=v, window=5)",
        refus: true,
    },
    // Un texte inconnu y est lu comme une formule : pas de refus à vérifier.
    Entree {
        nom: "fit_model",
        corps: "pyst.analysis.readiness('fit', model=v)",
        refus: false,
    },
];

/// Enumérations **rendues en sortie** (le test les observe).
const SORTIES: &[&str] = &["kind", "type", "role", "readiness_reason"];

/// Valeurs qu'aucun script ne sait produire : exclues de la complétude.
const IMPRODUCTIBLES: &[(&str, &str)] = &[("kind", "unknown")];

const SCRIPT_OBSERVE: &str = "
pyst.create_column(sh, 'Mixte', column_type='mixed')
pyst.create_column(sh, 'Date', column_type='datetime')
vus = {'kind': set(), 'type': set(), 'role': set()}
def marche(h):
    i = pyst.info(h)
    vus['kind'].add(i['kind'])
    if i['column']:
        vus['type'].add(i['column']['type'])
        vus['role'].add(i['column']['role'])
    for enfant in pyst.children(h):
        marche(enfant)
marche(pyst.root())
for nom in sorted(vus):
    print('OBS|%s|%s' % (nom, ','.join(sorted(vus[nom]))))
raisons = set()
for appel in (
    lambda: pyst.analysis.readiness('baseline', method='anchors'),
    lambda: pyst.analysis.readiness('baseline', method='anchors', points=[(1.0, 1.0), (2.0, 2.0)]),
    lambda: pyst.analysis.readiness('integrate', ranges=[(float('nan'), float('nan'))]),
    lambda: pyst.analysis.readiness('integrate'),
    lambda: pyst.analysis.readiness('fit', model='x +'),
    lambda: pyst.analysis.readiness('fit', model='3'),
    lambda: pyst.analysis.readiness('fit', model='polynomial', degree=0),
):
    try:
        r = appel()['reason']
        if r is not None:
            raisons.add(r)
    except pyst.InvalidValueError:
        pass
print('OBS|readiness_reason|%s' % ','.join(sorted(raisons)))
";

#[test]
fn les_valeurs_enumerees_sont_celles_de_la_liste_figee() {
    let banc = banc();
    let blocs = blocs();
    let mut ecarts = Vec::new();

    // Couverture : chaque énumération de la doc a un essai, et inversement.
    let documentees = noms(&blocs, "enum");
    let couvertes: BTreeSet<String> = ENTREES
        .iter()
        .map(|e| e.nom.to_owned())
        .chain(SORTIES.iter().map(|s| (*s).to_owned()))
        .collect();
    comparer(
        &mut ecarts,
        "enumerations couvertes par le test",
        Some(documentees),
        &couvertes,
    );

    // En entrée.
    for entree in ENTREES {
        let Some(valeurs) = bloc(&blocs, "enum", entree.nom) else {
            continue;
        };
        let script = format!(
            "def essai(v):\n    {corps}\n\
             refusees = []\n\
             for v in {valeurs}:\n    try:\n        essai(v)\n    except Exception as e:\n        refusees.append('%s: %r' % (v, e))\n\
             print('REFUSEES|' + ' ; '.join(refusees))\n\
             if {refus}:\n    try:\n        essai('valeur_inventee_zzz')\n        print('INVENTEE|acceptee')\n    except pyst.InvalidValueError:\n        print('INVENTEE|refusee')\n    except Exception as e:\n        print('INVENTEE|autre erreur %r' % e)\n\
             else:\n    print('INVENTEE|refusee')\n",
            corps = entree.corps,
            valeurs = liste_python(&valeurs),
            refus = if entree.refus { "True" } else { "False" },
        );
        let sortie = sortie(&banc, &script);
        let refusees = un(&sortie, "REFUSEES");
        if !refusees.is_empty() {
            ecarts.push(format!(
                "enum {} : valeurs de la doc que le code refuse : {refusees}",
                entree.nom
            ));
        }
        let inventee = un(&sortie, "INVENTEE");
        if inventee != "refusee" {
            ecarts.push(format!(
                "enum {} : une valeur hors liste n'est pas refusee ({inventee})",
                entree.nom
            ));
        }
    }

    // En sortie.
    let sortie = sortie(&banc, SCRIPT_OBSERVE);
    let observees: BTreeMap<&str, BTreeSet<String>> = sortie["OBS"]
        .iter()
        .filter_map(|l| l.split_once('|'))
        .map(|(nom, valeurs)| (nom, ensemble(valeurs)))
        .collect();
    for nom in SORTIES {
        let Some(documentees) = bloc(&blocs, "enum", nom) else {
            ecarts.push(format!("enum {nom} : bloc absent de la doc"));
            continue;
        };
        let vues = observees.get(nom).cloned().unwrap_or_default();
        let hors_liste: Vec<_> = vues.difference(&documentees).collect();
        if !hors_liste.is_empty() {
            ecarts.push(format!(
                "enum {nom} : valeurs rendues par le code, pas dans la doc : {hors_liste:?}"
            ));
        }
        // Complétude pour les énumérations que l'exemple sait toutes produire.
        if matches!(*nom, "kind" | "type") {
            let attendues: Vec<_> = documentees
                .iter()
                .filter(|v| !IMPRODUCTIBLES.contains(&(*nom, v.as_str())))
                .filter(|v| !vues.contains(*v))
                .collect();
            if !attendues.is_empty() {
                ecarts.push(format!(
                    "enum {nom} : valeurs de la doc jamais rendues par le code : {attendues:?}"
                ));
            }
        }
    }
    exiger_sans_ecart(ecarts);
}
