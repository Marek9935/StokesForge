//! Texte tabulé (TSV) des cellules — le format que le presse-papiers échange
//! avec un tableur (Excel, LibreOffice) (#259).
//!
//! Une case reste telle quelle tant qu'elle ne contient ni tabulation, ni
//! saut de ligne, ni guillemet ; sinon elle est entourée de guillemets et ses
//! guillemets sont doublés (`a"b` → `"a""b"`). Une ligne se termine par `\n`
//! (la dernière aussi). C'est le format que lisent et écrivent les tableurs :
//! un texte à plusieurs lignes ne décale donc plus les cases suivantes.
//!
//! Ce module ne connaît ni la sélection, ni l'affichage d'une case manquante,
//! ni egui : il met en texte des rangées de chaînes déjà prêtes. Le choix des
//! cases copiées et leur mise en texte restent à l'appelant.
//!
//! # Lecture ([`decode`])
//!
//! - fin de ligne `\n` ou `\r\n` (un tableur sous Windows écrit `\r\n`) ;
//! - une fin de ligne **terminale** ne crée pas de ligne de plus ;
//! - dans une case entre guillemets, tout est gardé tel quel (`\r\n`
//!   compris) et `""` redevient `"` ;
//! - un guillemet qui n'ouvre pas la case (`5" tube`) est un caractère
//!   ordinaire ; un guillemet jamais refermé court jusqu'à la fin du texte —
//!   un presse-papiers tiers mal formé ne perd rien.
//!
//! `decode("")` est vide ; `decode(&encode(rows)) == rows` pour toute rangée
//! d'au moins une case (une rangée sans case s'écrit `\n`, qui se relit comme
//! une rangée d'une case vide).

/// Vrai si `cell` doit être entourée de guillemets pour survivre à
/// [`decode`] : tabulation, saut de ligne (`\n` ou `\r`) ou guillemet.
fn needs_quotes(cell: &str) -> bool {
    cell.contains(['\t', '\n', '\r', '"'])
}

/// Une case, entre guillemets si [`needs_quotes`] — sans séparateur ni fin de
/// ligne. C'est aussi ce qu'on pousse au presse-papiers pour une case seule.
pub fn encode_cell(cell: &str) -> String {
    if needs_quotes(cell) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Met `rows` en texte tabulé : `\t` entre les cases, `\n` après chaque
/// ligne, la dernière comprise.
pub fn encode(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push('\t');
            }
            out.push_str(&encode_cell(cell));
        }
        out.push('\n');
    }
    out
}

/// Relit un texte tabulé (voir la doc du module). Un texte vide donne aucune
/// ligne ; un texte qui ne finit pas par une fin de ligne garde sa dernière
/// ligne.
pub fn decode(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    // Au début d'une case (début de texte, de ligne, ou après `\t`) : seul
    // endroit où un guillemet ouvre une case entre guillemets.
    let mut at_cell_start = true;
    // Vrai tant que la ligne en cours a reçu un caractère ou un séparateur :
    // distingue « fin de texte après une fin de ligne » (rien à ajouter)
    // d'une dernière ligne sans fin de ligne.
    let mut pending = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if at_cell_start => {
                pending = true;
                at_cell_start = false;
                while let Some(inner) = chars.next() {
                    if inner != '"' {
                        cell.push(inner);
                    } else if chars.peek() == Some(&'"') {
                        chars.next();
                        cell.push('"');
                    } else {
                        break;
                    }
                }
            }
            '\t' => {
                row.push(std::mem::take(&mut cell));
                at_cell_start = true;
                pending = true;
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                at_cell_start = true;
                pending = false;
            }
            // `\r\n` : le `\r` ne fait pas partie de la case.
            '\r' if chars.peek() == Some(&'\n') => {}
            other => {
                cell.push(other);
                at_cell_start = false;
                pending = true;
            }
        }
    }
    if pending {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, encode_cell};

    fn rows(data: &[&[&str]]) -> Vec<Vec<String>> {
        data.iter()
            .map(|row| row.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    fn roundtrip(data: &[&[&str]]) {
        let rows = rows(data);
        let text = encode(&rows);
        assert_eq!(decode(&text), rows, "texte intermédiaire : {text:?}");
    }

    #[test]
    fn une_case_simple_reste_telle_quelle() {
        assert_eq!(encode(&rows(&[&["a", "b"], &["c", "d"]])), "a\tb\nc\td\n");
        roundtrip(&[&["a", "b"], &["c", "d"]]);
    }

    #[test]
    fn une_tabulation_dans_une_case_ne_decale_rien() {
        assert_eq!(encode(&rows(&[&["a\tb", "c"]])), "\"a\tb\"\tc\n");
        roundtrip(&[&["a\tb", "c"]]);
    }

    #[test]
    fn un_saut_de_ligne_dans_une_case_ne_decale_rien() {
        assert_eq!(encode(&rows(&[&["a\nb", "c"]])), "\"a\nb\"\tc\n");
        roundtrip(&[&["a\nb", "c"], &["d", "e"]]);
        roundtrip(&[&["a\r\nb", "c"], &["d", "e"]]);
        roundtrip(&[&["a\rb", "c"]]);
        roundtrip(&[&["fin\n", "\nstart"]]);
    }

    #[test]
    fn un_guillemet_est_double() {
        assert_eq!(
            encode(&rows(&[&["il a dit \"oui\""]])),
            "\"il a dit \"\"oui\"\"\"\n"
        );
        roundtrip(&[&["il a dit \"oui\""]]);
        roundtrip(&[&["\""]]);
        roundtrip(&[&["\"", "\"\""]]);
        roundtrip(&[&["\"x"]]);
    }

    #[test]
    fn un_espace_insecable_est_garde() {
        roundtrip(&[&["\u{00A0}", "a\u{00A0}b", "\u{00A0}\n\u{00A0}"]]);
        assert_eq!(encode(&rows(&[&["\u{00A0}"]])), "\u{00A0}\n");
    }

    #[test]
    fn des_cases_vides_font_l_aller_retour() {
        roundtrip(&[&[""]]);
        roundtrip(&[&["", ""]]);
        roundtrip(&[&["a", ""], &["", "b"], &["", ""]]);
        roundtrip(&[&["a", "", "c"]]);
    }

    #[test]
    fn plusieurs_lignes_et_colonnes_font_l_aller_retour() {
        roundtrip(&[
            &["x", "y\tz", "w"],
            &["1", "2\n3", "\"4\""],
            &["", "\u{00A0}", "6"],
        ]);
        assert!(decode("").is_empty());
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn encode_cell_n_ajoute_ni_separateur_ni_fin_de_ligne() {
        assert_eq!(encode_cell("a"), "a");
        assert_eq!(encode_cell("a\nb"), "\"a\nb\"");
        assert_eq!(encode_cell("a\"b"), "\"a\"\"b\"");
    }

    // -- Lecture d'un tableur externe --

    #[test]
    fn un_tableur_windows_termine_ses_lignes_par_crlf() {
        assert_eq!(
            decode("a\tb\r\nc\td\r\n"),
            rows(&[&["a", "b"], &["c", "d"]])
        );
        // Case entre guillemets : le `\r\n` interne est gardé.
        assert_eq!(decode("\"a\r\nb\"\tc\r\n"), rows(&[&["a\r\nb", "c"]]));
    }

    #[test]
    fn la_fin_de_ligne_terminale_ne_cree_pas_de_ligne() {
        assert_eq!(decode("a\n"), rows(&[&["a"]]));
        assert_eq!(decode("a"), rows(&[&["a"]]));
        assert_eq!(decode("a\n\n"), rows(&[&["a"], &[""]]));
        assert_eq!(decode("\n"), rows(&[&[""]]));
        assert_eq!(decode("a\t"), rows(&[&["a", ""]]));
        assert_eq!(
            decode("0\t10\n0.5\t\n"),
            rows(&[&["0", "10"], &["0.5", ""]])
        );
    }

    #[test]
    fn un_guillemet_hors_debut_de_case_est_ordinaire() {
        assert_eq!(decode("5\" tube\tb"), rows(&[&["5\" tube", "b"]]));
        assert_eq!(decode("a\"b\"c"), rows(&[&["a\"b\"c"]]));
    }

    #[test]
    fn un_guillemet_jamais_ferme_court_jusqu_a_la_fin() {
        assert_eq!(decode("\"a\tb\nc"), rows(&[&["a\tb\nc"]]));
    }
}
