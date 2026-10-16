//! Format de texte : les balises Typst que la barre de format pose (issue #63).
//!
//! Les titres, titres d'axe, entrées de légende et textes libres sont déjà du
//! **markup Typst** (`docs/rendu.md`, issue #32) : mettre un mot en gras,
//! c'est l'envelopper dans `#strong[…]`. Aucun champ n'est donc ajouté au
//! modèle ni au `.stf` pour le gras, l'italique, la couleur et le corps d'un
//! passage ; la barre écrit dans le texte, et chaque écriture passe par la
//! même action annulable que la saisie. **La police n'en fait plus partie**
//! (issue #78) : c'est un champ de la zone, posé par `StokesForgeApp::format_zone` ;
//! [`TextFormat::Font`] reste ce que la barre demande, mais n'arrive ici que si
//! aucune zone n'est visée.
//!
//! Tout ici est pur (texte et indices de caractères), testé sans fenêtre. Les
//! indices sont des **indices de caractères**, comme ceux du curseur d'egui.

use std::ops::Range;

/// Une mise en forme que la barre sait poser.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFormat {
    Bold,
    Italic,
    Subscript,
    Superscript,
    /// Le mode mathématique `$…$`.
    Formula,
    /// Couleur `#RRGGBB` ou `#RRGGBBAA`. Une couleur vide (automatique) ou
    /// invalide ne pose rien.
    Color(String),
    /// Corps en points Typst.
    Size(f64),
    /// Retire le corps explicite de la zone (héritage du rendu).
    AutoSize,
    /// Nom de famille d'une police, embarquée ou du système.
    Font(String),
    /// Alignement du texte dans sa boîte (#244) : un champ de la zone, jamais
    /// une balise.
    Align(stokesforge_plot::TextAlign),
}

/// Le texte après mise en forme, et la sélection à rendre au champ.
#[derive(Debug, Clone, PartialEq)]
pub struct Formatted {
    pub text: String,
    /// Indices de caractères. Vide (`n..n`) : curseur seul.
    pub selection: Range<usize>,
}

impl TextFormat {
    /// Les balises ouvrante et fermante, ou `None` si le réglage ne pose rien.
    fn tags(&self) -> Option<(String, String)> {
        let call = |head: String| Some((format!("#{head}["), "]".to_owned()));
        match self {
            TextFormat::Bold => call("strong".into()),
            TextFormat::Italic => call("emph".into()),
            TextFormat::Subscript => call("sub".into()),
            TextFormat::Superscript => call("super".into()),
            TextFormat::Formula => Some(("$".into(), "$".into())),
            TextFormat::Color(color) if valid_color(color) => {
                call(format!("text(fill: rgb(\"{color}\"))"))
            }
            TextFormat::Size(size) if size.is_finite() && *size > 0.0 => {
                call(format!("text(size: {size}pt)"))
            }
            TextFormat::Color(_)
            | TextFormat::Size(_)
            | TextFormat::AutoSize
            | TextFormat::Align(_) => None,
            TextFormat::Font(name) => {
                let name = name.trim();
                if name.is_empty() {
                    None
                } else {
                    let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
                    call(format!("text(font: \"{quoted}\")"))
                }
            }
        }
    }

    /// Ces réglages se défont en les reposant (gras, italique, indice,
    /// exposant, formule) ; couleur, corps et police s'empilent.
    fn toggles(&self) -> bool {
        matches!(
            self,
            TextFormat::Bold
                | TextFormat::Italic
                | TextFormat::Subscript
                | TextFormat::Superscript
                | TextFormat::Formula
        )
    }
}

fn valid_color(color: &str) -> bool {
    let hex = color.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Applique `format` à `selection` de `text`.
///
/// - sélection non vide : elle est enveloppée, et reste sélectionnée à
///   l'intérieur des balises ;
/// - curseur seul : les balises sont insérées et le curseur se place entre
///   elles ;
/// - un réglage à bascule déjà posé exactement sur la sélection (balises dans
///   la sélection ou juste autour d'elle) est retiré.
///
/// Les indices hors du texte sont ramenés dedans, une sélection inversée est
/// remise dans l'ordre. Un réglage qui ne pose rien rend le texte inchangé.
pub fn apply(text: &str, selection: Range<usize>, format: &TextFormat) -> Formatted {
    let len = text.chars().count();
    let (start, end) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    let (start, end) = (start.min(len), end.min(len));
    let unchanged = Formatted {
        text: text.to_owned(),
        selection: start..end,
    };
    let Some((open, close)) = format.tags() else {
        return unchanged;
    };
    let (open_len, close_len) = (open.chars().count(), close.chars().count());
    let (from, to) = (byte_at(text, start), byte_at(text, end));
    let selected = &text[from..to];

    if format.toggles() && start != end {
        // Les balises sont dans la sélection.
        if selected.len() >= open.len() + close.len()
            && selected.starts_with(&open)
            && selected.ends_with(&close)
        {
            let inner = &selected[open.len()..selected.len() - close.len()];
            return Formatted {
                text: format!("{}{inner}{}", &text[..from], &text[to..]),
                selection: start..end - open_len - close_len,
            };
        }
        // Les balises sont juste autour de la sélection.
        if text[..from].ends_with(&open) && text[to..].starts_with(&close) {
            return Formatted {
                text: format!(
                    "{}{selected}{}",
                    &text[..from - open.len()],
                    &text[to + close.len()..]
                ),
                selection: start - open_len..end - open_len,
            };
        }
    }

    Formatted {
        text: format!("{}{open}{selected}{close}{}", &text[..from], &text[to..]),
        selection: start + open_len..end + open_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, selection: Range<usize>, format: TextFormat) -> (String, Range<usize>) {
        let done = apply(text, selection, &format);
        (done.text, done.selection)
    }

    #[test]
    fn une_selection_est_enveloppee_et_reste_selectionnee_dedans() {
        let (text, selection) = applied("Abs 280 nm", 4..7, TextFormat::Bold);
        assert_eq!(text, "Abs #strong[280] nm");
        assert_eq!(selection, 12..15);
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(chars[selection].iter().collect::<String>(), "280");
    }

    #[test]
    fn un_curseur_seul_insere_les_balises_et_se_place_dedans() {
        let (text, selection) = applied("ab", 1..1, TextFormat::Subscript);
        assert_eq!(text, "a#sub[]b");
        assert_eq!(selection, 6..6);
        assert_eq!(
            applied("", 0..0, TextFormat::Formula),
            ("$$".to_owned(), 1..1)
        );
    }

    #[test]
    fn gras_italique_indice_exposant_formule_ont_chacun_leur_balise() {
        for (format, expected) in [
            (TextFormat::Bold, "#strong[x]"),
            (TextFormat::Italic, "#emph[x]"),
            (TextFormat::Subscript, "#sub[x]"),
            (TextFormat::Superscript, "#super[x]"),
            (TextFormat::Formula, "$x$"),
        ] {
            assert_eq!(applied("x", 0..1, format).0, expected);
        }
    }

    #[test]
    fn couleur_corps_et_police_posent_un_appel_text() {
        assert_eq!(
            applied("x", 0..1, TextFormat::Color("#ff0000".into())).0,
            "#text(fill: rgb(\"#ff0000\"))[x]"
        );
        assert_eq!(
            applied("x", 0..1, TextFormat::Size(14.0)).0,
            "#text(size: 14pt)[x]"
        );
        assert_eq!(
            applied("x", 0..1, TextFormat::Font("Fira Sans".into())).0,
            "#text(font: \"Fira Sans\")[x]"
        );
    }

    #[test]
    fn un_nom_de_police_avec_guillemet_est_echappe() {
        assert_eq!(
            applied("x", 0..1, TextFormat::Font("A\"B".into())).0,
            "#text(font: \"A\\\"B\")[x]"
        );
    }

    #[test]
    fn un_reglage_qui_ne_pose_rien_laisse_le_texte() {
        for format in [
            TextFormat::Color(String::new()),
            TextFormat::Color("rouge".into()),
            TextFormat::Size(f64::NAN),
            TextFormat::Size(0.0),
            TextFormat::Font("  ".into()),
        ] {
            assert_eq!(applied("x", 0..1, format), ("x".to_owned(), 0..1));
        }
    }

    #[test]
    fn reposer_un_reglage_a_bascule_le_retire() {
        // Balises autour de la sélection.
        let (text, selection) = applied("a #strong[280] b", 10..13, TextFormat::Bold);
        assert_eq!(text, "a 280 b");
        assert_eq!(selection, 2..5);
        // Balises dans la sélection.
        let (text, selection) = applied("a #emph[280] b", 2..12, TextFormat::Italic);
        assert_eq!(text, "a 280 b");
        assert_eq!(selection, 2..5);
        // Un autre réglage ne défait rien.
        assert_eq!(
            applied("#strong[x]", 8..9, TextFormat::Italic).0,
            "#strong[#emph[x]]"
        );
        // Couleur et corps s'empilent.
        assert_eq!(
            applied("x", 0..1, TextFormat::Size(9.0)).0,
            "#text(size: 9pt)[x]"
        );
    }

    #[test]
    fn les_indices_sont_des_caracteres_pas_des_octets() {
        let (text, selection) = applied("λ nm é", 0..1, TextFormat::Italic);
        assert_eq!(text, "#emph[λ] nm é");
        assert_eq!(selection, 6..7);
        let (text, _) = applied("λ nm é", 5..6, TextFormat::Bold);
        assert_eq!(text, "λ nm #strong[é]");
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn une_selection_inversee_ou_hors_du_texte_est_ramenee_dedans() {
        assert_eq!(applied("abc", 3..1, TextFormat::Bold).0, "a#strong[bc]");
        assert_eq!(applied("abc", 1..99, TextFormat::Bold).0, "a#strong[bc]");
        assert_eq!(applied("abc", 50..60, TextFormat::Bold).0, "abc#strong[]");
    }
}
