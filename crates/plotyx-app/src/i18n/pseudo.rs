//! Pseudo-langue `en-XA`, fabriquée **à la volée** depuis le catalogue `en`
//! (issue #41) : aucun fichier à tenir à jour, elle suit l'anglais clé par clé.
//!
//! Elle sert à *voir* ce que la traduction ne couvre pas, sans savoir lire une
//! autre langue :
//!
//! - les lettres sont accentuées (« Save Project » → « Šàṽé Þŕöĵéçţ »), donc un
//!   texte reste lisible mais reconnaissable ;
//! - chaque message est encadré de `[[ ]]` : **un texte sans `[[ ]]` à l'écran n'est
//!   pas passé par le catalogue** (littéral resté dans le code), et un `]]`
//!   manquant signale un texte **tronqué** par la mise en page ;
//! - chaque message est allongé de 30 % (le français est plus long que
//!   l'anglais) : une disposition qui ne tient pas en `en-XA` ne tiendra pas en
//!   français.
//!
//! La transformation passe par l'arbre syntaxique de Fluent (`fluent-syntax`),
//! pas par une expression régulière : seuls les **textes** sont touchés, jamais
//! les `{ $variable }`, les références `{ -terme }`, les clés de variante
//! (`[one]`, `*[other]`) ni les identifiants. C'est ce qui garantit que la
//! pseudo-langue accepte les mêmes arguments que l'anglais.
//!
//! Dans un `select` (pluriel), chaque variante est allongée séparément ; le
//! cadre `[[ ]]` n'entoure que le message entier. Les termes (`-nom = …`) sont
//! seulement accentués : ils entrent dans d'autres messages, déjà encadrés.

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use fluent_syntax::{parser, serializer};

/// Ouvre un message de la pseudo-langue. ASCII, pour que toute police
/// l'affiche (`⟦ ⟧` sortaient en carrés « glyphe absent » avec Noto Sans).
pub const OPEN: &str = "[[";
/// Ferme un message de la pseudo-langue. Absent à l'écran : texte tronqué.
pub const CLOSE: &str = "]]";
/// Allongement visé, en pourcent du texte d'origine.
pub const EXTRA_PERCENT: usize = 30;
/// Lettre qui sert à allonger : accentuée, de largeur courante.
const FILLER: char = 'ŏ';

/// Un littéral de chaîne Fluent, `{ "…" }`.
fn literal(value: &str) -> PatternElement<String> {
    PatternElement::Placeable {
        expression: Expression::Inline(InlineExpression::StringLiteral {
            value: value.to_string(),
        }),
    }
}

/// Produit la pseudo-langue d'un catalogue Fluent. Une source qui ne
/// s'analyse pas entièrement garde ses entrées valides ; les morceaux
/// illisibles restent tels quels (invariant 10 : on dégrade, on ne panique pas
/// — le test `le_catalogue_anglais_s_analyse_sans_erreur` tient l'anglais).
pub fn localize(source: &str) -> String {
    let mut resource = match parser::parse(source.to_string()) {
        Ok(resource) => resource,
        Err((resource, _errors)) => resource,
    };
    for entry in &mut resource.body {
        match entry {
            Entry::Message(message) => {
                if let Some(pattern) = &mut message.value {
                    localize_pattern(pattern, Role::Message);
                }
                for attribute in &mut message.attributes {
                    localize_pattern(&mut attribute.value, Role::Message);
                }
            }
            Entry::Term(term) => {
                localize_pattern(&mut term.value, Role::Term);
                for attribute in &mut term.attributes {
                    localize_pattern(&mut attribute.value, Role::Term);
                }
            }
            _ => {}
        }
    }
    serializer::serialize(&resource)
}

/// Ce qu'on fait d'un motif selon sa place.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Message ou attribut entier : accentué, allongé, encadré.
    Message,
    /// Variante d'un `select` : accentuée et allongée, sans cadre.
    Variant,
    /// Terme : accentué seulement.
    Term,
}

fn localize_pattern(pattern: &mut Pattern<String>, role: Role) {
    // `letters` décide s'il y a du texte à allonger ; `length` (espaces compris)
    // fixe l'allongement : 30 % de la longueur *affichée*, pas des seules
    // lettres, sinon un message riche en espaces (une phrase d'erreur) resterait
    // en deçà de +30 %.
    let mut letters = 0usize;
    let mut length = 0usize;
    for element in &mut pattern.elements {
        match element {
            PatternElement::TextElement { value } => {
                *value = value.chars().map(accent).collect();
                letters += value.chars().filter(|c| !c.is_whitespace()).count();
                length += value.chars().count();
            }
            PatternElement::Placeable { expression } => localize_expression(expression, role),
        }
    }
    let padding = || {
        let extra = (length * EXTRA_PERCENT).div_ceil(100);
        format!(" {}", FILLER.to_string().repeat(extra.max(1)))
    };
    match role {
        Role::Term => {}
        Role::Variant => {
            if letters > 0 {
                pattern
                    .elements
                    .push(PatternElement::TextElement { value: padding() });
            }
        }
        Role::Message => {
            // Le cadre est un littéral `{ "[[" }`, pas du texte : un `[` en
            // tête de ligne d'un motif sur plusieurs lignes se lirait comme
            // une clé de variante. Fluent n'isole pas les littéraux (FSI/PDI).
            if letters > 0 {
                pattern
                    .elements
                    .push(PatternElement::TextElement { value: padding() });
            }
            pattern.elements.insert(0, literal(OPEN));
            pattern.elements.push(literal(CLOSE));
        }
    }
}

/// Descend dans les expressions qui contiennent des motifs (un `select`, un
/// placeable imbriqué) ; une variable, une référence ou un littéral ne sont
/// jamais touchés.
fn localize_expression(expression: &mut Expression<String>, role: Role) {
    match expression {
        Expression::Select { variants, .. } => {
            let inner = if role == Role::Term {
                Role::Term
            } else {
                Role::Variant
            };
            for variant in variants {
                localize_pattern(&mut variant.value, inner);
            }
        }
        Expression::Inline(InlineExpression::Placeable { expression }) => {
            localize_expression(expression, role);
        }
        Expression::Inline(_) => {}
    }
}

/// Une lettre ASCII devient une lettre accentuée ; le reste est inchangé.
fn accent(c: char) -> char {
    match c {
        'a' => 'à',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'î',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ĺ',
        'm' => 'ɱ',
        'n' => 'ñ',
        'o' => 'ö',
        'p' => 'þ',
        'q' => 'ɋ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ü',
        'v' => 'ṽ',
        'w' => 'ŵ',
        'x' => 'ẋ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'À',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'F' => 'Ƒ',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Î',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ĺ',
        'M' => 'Ṁ',
        'N' => 'Ñ',
        'O' => 'Ö',
        'P' => 'Þ',
        'Q' => 'Ɋ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Ü',
        'V' => 'Ṽ',
        'W' => 'Ŵ',
        'X' => 'Ẋ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un catalogue qui réunit tout ce que la transformation ne doit pas
    /// abîmer : variable, pluriel, terme, attribut, commentaire, littéral.
    const SAMPLE: &str = "\
# Un commentaire
-brand = PlotYx
save = Save Project
open-file = Open { $name } now
count =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }
about = About { -brand }
    .hint = Shows the version
literal = Say { \"hi\" }
";

    #[test]
    fn la_pseudo_langue_garde_la_meme_forme_de_catalogue() {
        let pseudo = localize(SAMPLE);
        let (resource, errors) = match parser::parse(pseudo.as_str()) {
            Ok(resource) => (resource, vec![]),
            Err((resource, errors)) => (resource, errors),
        };
        assert!(
            errors.is_empty(),
            "la pseudo-langue s'analyse : {errors:?}\n{pseudo}"
        );
        let (original, _) = match parser::parse(SAMPLE) {
            Ok(resource) => (resource, ()),
            Err((resource, _)) => (resource, ()),
        };
        let ids = |resource: &fluent_syntax::ast::Resource<&str>| -> Vec<String> {
            resource
                .body
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Message(message) => Some(message.id.name.to_string()),
                    Entry::Term(term) => Some(format!("-{}", term.id.name)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(ids(&resource), ids(&original), "mêmes clés, même ordre");
    }

    #[test]
    fn un_message_est_accentue_encadre_et_allonge() {
        let pseudo = localize("save = Save Project\n");
        let text = pseudo
            .strip_prefix("save = ")
            .and_then(|rest| rest.strip_suffix('\n'))
            .expect("une ligne");
        // Le source sérialisé porte le cadre en littéraux `{ "[[" }`.
        let (open, close) = (format!("{{ \"{OPEN}\" }}"), format!("{{ \"{CLOSE}\" }}"));
        assert!(text.starts_with(&open), "{text}");
        assert!(text.ends_with(&close), "{text}");
        assert!(text.contains("Šàṽé Þŕöĵéçţ"), "accentué : {text}");
        let inner = text.trim_start_matches(&open).trim_end_matches(&close);
        assert!(
            inner.chars().count() >= "Save Project".chars().count() * 13 / 10,
            "allongé d'au moins 30 % : {inner}"
        );
    }

    #[test]
    fn les_variables_les_termes_et_les_cles_ne_sont_pas_touches() {
        let pseudo = localize(SAMPLE);
        for fragment in [
            "{ $name }",
            "{ $count }",
            "[one]",
            "*[other]",
            "{ -brand }",
            "-brand = ÞĺöţÝẋ",
            ".hint = ",
            "{ \"hi\" }",
            "# Un commentaire",
        ] {
            assert!(
                pseudo.contains(fragment),
                "{fragment:?} absent de :\n{pseudo}"
            );
        }
        assert!(
            !pseudo.contains("-brand = { \"[[\" }"),
            "un terme n'est pas encadré"
        );
    }

    #[test]
    fn chaque_variante_d_un_pluriel_est_allongee() {
        let pseudo = localize(SAMPLE);
        assert!(pseudo.contains("[one] { $count } îţéɱ ŏ"), "{pseudo}");
        assert!(pseudo.contains("*[other] { $count } îţéɱš ŏ"), "{pseudo}");
    }

    #[test]
    fn un_message_sans_texte_est_quand_meme_encadre() {
        let pseudo = localize("only = { $n }\n");
        assert!(pseudo.contains("{ \"[[\" }{ $n }{ \"]]\" }"), "{pseudo}");
    }
}
