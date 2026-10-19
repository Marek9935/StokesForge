//! Le texte de la boîte « R² faible » et de la pastille (#290), en anglais et
//! en français, singulier et pluriel. Un seul test : la langue est globale au
//! processus, et ce fichier est le sien.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::r_squared::{
    badge_text, group_by_kind, summary_text, total, LowRSquaredGroup,
};

fn groups() -> Vec<LowRSquaredGroup> {
    // Désordre volontaire, et un type en deux morceaux : 7 + 5 reconvolutions.
    group_by_kind([
        (OperationKind::Fit, 1),
        (OperationKind::Reconvolution, 7),
        (OperationKind::Peaks, 0),
        (OperationKind::Reconvolution, 5),
    ])
}

#[test]
fn the_counts_group_by_type_in_a_fixed_order() {
    let found = groups();
    assert_eq!(
        found,
        vec![
            LowRSquaredGroup {
                kind: OperationKind::Fit,
                count: 1
            },
            LowRSquaredGroup {
                kind: OperationKind::Reconvolution,
                count: 12
            },
        ],
        "types dans l'ordre de OperationKind::ALL, zéro écarté, totaux fusionnés"
    );
    assert_eq!(total(&found), 13);
    assert!(group_by_kind([]).is_empty());
}

#[test]
fn the_summary_and_the_badge_are_plural_aware_in_english_and_french() {
    let _garde = LANGUE.lock().unwrap_or_else(|e| e.into_inner());
    i18n::set_language(Language::English);
    let text = summary_text(&groups());
    assert_eq!(
        text,
        "R² < 0.6 for:\n• 1 fit\n• 12 reconvolutions\nSee the Results pane for the values."
    );
    assert_eq!(badge_text(13), "13 low R²");

    i18n::set_language(Language::French);
    let text = summary_text(&groups());
    assert_eq!(
        text,
        "R² < 0,6 pour :\n• 1 ajustement\n• 12 reconvolutions\nLes valeurs sont dans le volet Résultats."
    );
    assert_eq!(badge_text(1), "1 R² faible");
    assert_eq!(badge_text(13), "13 R² faibles");
    // Chaque type a son nom au glossaire, au singulier comme au pluriel.
    for kind in OperationKind::ALL {
        for count in [1, 2] {
            let line =
                stokesforge_app_state::r_squared::group_line(&LowRSquaredGroup { kind, count });
            assert!(
                line.starts_with(&count.to_string()) && line.len() > 3,
                "{line}"
            );
        }
    }
    i18n::set_language(Language::English);
}

// ---- Blindage (#290) : noms par type, pluriels, regroupement de la boîte ----

use std::sync::Mutex;
use stokesforge_app_state::r_squared::group_line;

/// La langue est globale au processus : les tests de ce fichier se la passent.
static LANGUE: Mutex<()> = Mutex::new(());

fn avec_langue<T>(language: Language, f: impl FnOnce() -> T) -> T {
    let _garde = LANGUE.lock().unwrap_or_else(|e| e.into_inner());
    i18n::set_language(language);
    let out = f();
    i18n::set_language(Language::English);
    out
}

/// Match exhaustif sans `_` : un type ajouté à `OperationKind` casse la
/// compilation de ce test, qui force alors à lui donner un nom au glossaire.
fn rang(kind: OperationKind) -> usize {
    match kind {
        OperationKind::Baseline => 0,
        OperationKind::Correct => 1,
        OperationKind::Integrate => 2,
        OperationKind::Fit => 3,
        OperationKind::Peaks => 4,
        OperationKind::Reconvolution => 5,
        OperationKind::DirectDeconvolution => 6,
    }
}

fn ligne(kind: OperationKind, count: usize) -> String {
    group_line(&LowRSquaredGroup { kind, count })
}

/// Le nom d'un type sans son compteur : « 12 reconvolutions » -> « reconvolutions ».
fn nom(kind: OperationKind, count: usize) -> String {
    let line = ligne(kind, count);
    let prefix = format!("{count} ");
    line.strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("« {line} » ne commence pas par « {prefix}»"))
        .to_string()
}

#[test]
fn tous_les_types_ont_un_nom_singulier_et_pluriel_distincts_en_fr_et_en() {
    for language in [Language::English, Language::French] {
        avec_langue(language, || {
            assert_eq!(OperationKind::ALL.len(), 7);
            let mut singuliers = Vec::new();
            let mut pluriels = Vec::new();
            for (i, kind) in OperationKind::ALL.into_iter().enumerate() {
                assert_eq!(rang(kind), i, "ALL est dans l'ordre des rangs");
                let (un, deux) = (nom(kind, 1), nom(kind, 2));
                assert!(!un.trim().is_empty(), "{kind:?} {language:?}");
                assert!(!deux.trim().is_empty(), "{kind:?} {language:?}");
                assert_ne!(un, deux, "{kind:?} {language:?} : singulier = pluriel");
                assert!(
                    !un.contains("low-r2") && !deux.contains("low-r2"),
                    "clé non résolue : {un} / {deux}"
                );
                singuliers.push(un);
                pluriels.push(deux);
            }
            for noms in [&singuliers, &pluriels] {
                let mut tri = noms.clone();
                tri.sort();
                tri.dedup();
                assert_eq!(tri.len(), noms.len(), "noms de types confondus : {noms:?}");
            }
        });
    }
}

#[test]
fn les_pluriels_suivent_la_regle_du_catalogue_en_anglais_et_en_francais() {
    let valeurs = [0usize, 1, 2, 5, 12, 21, 100, 101];
    // (anglais : 0 est pluriel ; français : 0 et 1 sont singuliers)
    avec_langue(Language::English, || {
        for n in valeurs {
            let attendu = if n == 1 { "fit" } else { "fits" };
            assert_eq!(ligne(OperationKind::Fit, n), format!("{n} {attendu}"));
            let attendu = if n == 1 {
                "peak decomposition"
            } else {
                "peak decompositions"
            };
            assert_eq!(ligne(OperationKind::Peaks, n), format!("{n} {attendu}"));
        }
        assert_eq!(badge_text(0), "0 low R²");
        assert_eq!(badge_text(21), "21 low R²");
    });
    avec_langue(Language::French, || {
        for n in valeurs {
            let attendu = if n <= 1 { "ajustement" } else { "ajustements" };
            assert_eq!(
                ligne(OperationKind::Fit, n),
                format!("{n} {attendu}"),
                "n = {n}"
            );
            let attendu = if n <= 1 {
                "déconvolution directe"
            } else {
                "déconvolutions directes"
            };
            assert_eq!(
                ligne(OperationKind::DirectDeconvolution, n),
                format!("{n} {attendu}"),
                "n = {n}"
            );
            let attendu = if n <= 1 { "R² faible" } else { "R² faibles" };
            assert_eq!(badge_text(n), format!("{n} {attendu}"), "n = {n}");
        }
    });
}

#[test]
fn le_message_de_la_boite_pour_un_type_deux_types_et_tous_les_types() {
    avec_langue(Language::English, || {
        let un = group_by_kind([(OperationKind::Fit, 1)]);
        assert_eq!(
            summary_text(&un),
            "R² < 0.6 for:\n• 1 fit\nSee the Results pane for the values."
        );
        let douze = group_by_kind([(OperationKind::Reconvolution, 12)]);
        assert_eq!(
            summary_text(&douze),
            "R² < 0.6 for:\n• 12 reconvolutions\nSee the Results pane for the values."
        );
        let deux = group_by_kind([(OperationKind::Reconvolution, 12), (OperationKind::Fit, 1)]);
        assert_eq!(
            summary_text(&deux),
            "R² < 0.6 for:\n• 1 fit\n• 12 reconvolutions\nSee the Results pane for the values."
        );
        // Tous les types, passés dans l'ordre inverse : l'ordre de sortie est
        // celui de ALL, une ligne par type, chacune avec son compteur.
        let tous = group_by_kind(
            OperationKind::ALL
                .into_iter()
                .rev()
                .map(|k| (k, rang(k) + 1)),
        );
        assert_eq!(tous.len(), 7);
        let texte = summary_text(&tous);
        let lignes: Vec<&str> = texte.lines().collect();
        assert_eq!(lignes.len(), 9, "tête + 7 types + renvoi : {texte}");
        assert_eq!(lignes[0], "R² < 0.6 for:");
        for kind in OperationKind::ALL {
            assert_eq!(
                lignes[1 + rang(kind)],
                format!("• {}", ligne(kind, rang(kind) + 1)),
                "{kind:?}"
            );
        }
        assert_eq!(total(&tous), 28);
    });
}

#[test]
fn le_regroupement_fusionne_et_ecarte_les_zeros_quel_que_soit_l_ordre() {
    let a = group_by_kind([
        (OperationKind::Peaks, 2),
        (OperationKind::Fit, 3),
        (OperationKind::Peaks, 4),
        (OperationKind::Fit, 0),
    ]);
    let b = group_by_kind([
        (OperationKind::Fit, 3),
        (OperationKind::Peaks, 6),
        (OperationKind::Baseline, 0),
    ]);
    assert_eq!(a, b, "fusion et ordre indépendants de l'ordre d'entrée");
    assert_eq!(
        a.iter().map(|g| (g.kind, g.count)).collect::<Vec<_>>(),
        vec![(OperationKind::Fit, 3), (OperationKind::Peaks, 6)]
    );
    assert!(group_by_kind([(OperationKind::Fit, 0)]).is_empty());
}
