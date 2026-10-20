//! Les phrases des avertissements de calcul (#281, #203) dans les deux langues
//! du catalogue : aucune vide, aucune clé nue, le nombre interpolé, l'espace
//! insécable française devant « : ». Chargeurs propres (`build_loader`) : la
//! langue globale n'est pas touchée.

use stokesforge_app_state::i18n::{build_loader, Language};
use stokesforge_app_state::tr;

/// Les clés `analysis-warning-*` du catalogue `en`, et leur texte dans `loader`.
fn phrases(language: Language) -> Vec<(&'static str, String)> {
    let loader = build_loader(language);
    vec![
        (
            "analysis-warning-step-not-uniform",
            tr!(in &loader; "analysis-warning-step-not-uniform"),
        ),
        (
            "analysis-warning-irf-step",
            tr!(in &loader; "analysis-warning-irf-step"),
        ),
        (
            "analysis-warning-negative-clipped",
            tr!(in &loader; "analysis-warning-negative-clipped", count = 4_i64),
        ),
        (
            "analysis-warning-not-converged",
            tr!(in &loader; "analysis-warning-not-converged"),
        ),
        (
            "analysis-warning-undetermined",
            tr!(in &loader; "analysis-warning-undetermined", count = 3_i64),
        ),
        (
            "analysis-warning-irf-truncated",
            tr!(in &loader; "analysis-warning-irf-truncated"),
        ),
        (
            "analysis-warning-bridged-reference-gap",
            tr!(in &loader; "analysis-warning-bridged-reference-gap", count = 3_i64),
        ),
        (
            "analysis-warning-division-by-zero",
            tr!(in &loader; "analysis-warning-division-by-zero", count = 2_i64),
        ),
    ]
}

#[test]
fn chaque_avertissement_a_une_phrase_non_vide_et_distincte_en_anglais_et_en_francais() {
    for language in [Language::English, Language::French] {
        let found = phrases(language);
        for (key, text) in &found {
            assert!(!text.trim().is_empty(), "{language:?} {key}");
            assert!(!text.contains(key), "{language:?} : clé nue {key}");
            assert!(!text.contains("{ $"), "{language:?} {key} : {text}");
        }
        let mut texts: Vec<&String> = found.iter().map(|(_, t)| t).collect();
        texts.sort();
        texts.dedup();
        assert_eq!(
            texts.len(),
            found.len(),
            "{language:?} : phrases distinctes"
        );
    }
}

#[test]
fn la_phrase_de_non_convergence_dit_l_arret_et_la_piste_dans_les_deux_langues() {
    let en = phrases(Language::English)[3].1.clone();
    let fr = phrases(Language::French)[3].1.clone();
    assert!(
        en.contains("did not converge") && en.contains("starting values"),
        "{en}"
    );
    assert!(
        fr.contains("pas convergé") && fr.contains("valeurs de départ"),
        "{fr}"
    );
    assert_ne!(en, fr);
}

#[test]
fn les_nombres_sont_interpoles_sans_separateur_de_milliers() {
    let loader_en = build_loader(Language::English);
    let loader_fr = build_loader(Language::French);
    let n = 12_345_i64;
    for text in [
        tr!(in &loader_en; "analysis-warning-undetermined", count = n),
        tr!(in &loader_fr; "analysis-warning-undetermined", count = n),
        tr!(in &loader_en; "analysis-warning-negative-clipped", count = n),
        tr!(in &loader_fr; "analysis-warning-negative-clipped", count = n),
        tr!(in &loader_en; "analysis-warning-bridged-reference-gap", count = n),
        tr!(in &loader_fr; "analysis-warning-bridged-reference-gap", count = n),
        tr!(in &loader_en; "analysis-warning-division-by-zero", count = n),
        tr!(in &loader_fr; "analysis-warning-division-by-zero", count = n),
    ] {
        assert!(text.contains("12345"), "{text}");
    }
}

#[test]
fn en_francais_un_deux_points_est_precede_d_une_espace_insecable() {
    for (key, text) in phrases(Language::French) {
        for (i, c) in text.char_indices() {
            if c == ':' {
                let before = text[..i].chars().next_back();
                assert!(
                    matches!(before, Some('\u{a0}' | '\u{202f}')),
                    "{key} : « : » sans espace insécable devant dans {text:?}"
                );
            }
        }
    }
}

#[test]
fn en_anglais_le_deux_points_est_colle() {
    for (key, text) in phrases(Language::English) {
        assert!(!text.contains(" :"), "{key} : {text:?}");
    }
}
