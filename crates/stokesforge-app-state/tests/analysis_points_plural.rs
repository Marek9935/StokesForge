//! #336: "Place at least N points" agrees with N (R10), in both languages.
//! Clean loaders: the global language is not touched.

use stokesforge_app_state::i18n::{build_loader, Language};
use stokesforge_app_state::tr;

fn phrase(language: Language, needed: i64) -> String {
    let loader = build_loader(language);
    tr!(in &loader; "analysis-issue-needs-points", needed = needed, have = 0_i64)
}

#[test]
fn the_points_to_place_message_agrees_with_the_count() {
    let en = |n| phrase(Language::English, n);
    assert!(en(1).contains("1 point ("), "{}", en(1));
    assert!(en(2).contains("2 points ("), "{}", en(2));
    let fr = |n| phrase(Language::French, n);
    assert!(fr(1).contains("1 point ("), "{}", fr(1));
    assert!(fr(3).contains("3 points ("), "{}", fr(3));
}
