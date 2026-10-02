//! Décodage des octets d'un fichier en texte, avant tout découpage.
//!
//! Les logiciels d'appareil sous Windows (Spectra Manager de Jasco, entre
//! autres) écrivent encore leurs exports dans la page de code locale — en
//! France, Windows-1252 : « 19.91 °C » y donne l'octet `0xB0`, invalide en
//! UTF-8. Un tel fichier n'est pas corrompu ; le refuser ferait échouer tout
//! l'import pour un seul caractère d'un commentaire de pied de page.
//!
//! Ordre d'essai, du plus sûr au plus permissif :
//!
//! 1. marque d'ordre UTF-16 (`FF FE` / `FE FF`) → UTF-16 ;
//! 2. UTF-8 valide (marque `EF BB BF` retirée si présente) → tel quel ;
//! 3. sinon, si les octets ressemblent à du texte → Windows-1252.
//!
//! Windows-1252 accepte les 256 octets sans échec ; c'est donc le filtre
//! « ressemble à du texte » qui distingue un fichier texte d'un binaire :
//! aucun caractère de contrôle C0 hors tabulation, saut de ligne, retour
//! chariot, saut de page, et `0x1A` (fin de fichier DOS). Un fichier binaire
//! en contient presque toujours dans ses premiers octets ; un export texte
//! jamais.

use std::borrow::Cow;

use crate::parse::ImportError;

/// Décode `bytes` en texte selon l'ordre décrit dans la doc du module.
/// `Err(ImportError::NotText)` pour un fichier binaire. N'emprunte que
/// dans le cas courant (UTF-8 valide) ; les autres cas allouent une copie.
pub fn decode_text(bytes: &[u8]) -> Result<Cow<'_, str>, ImportError> {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| windows_1252(b)).collect()),
    };
    if text.chars().any(is_binary_control) {
        return Err(ImportError::NotText);
    }
    Ok(text)
}

/// Décode `bytes` avec l'encodage **choisi** dans la recette
/// ([`crate::recipe::Encoding`]) ; `Auto` se comporte comme [`decode_text`].
/// Un encodage forcé ne fait jamais échouer l'import pour un octet invalide
/// (il devient `U+FFFD`) ; seul un fichier de type binaire est refusé.
pub fn decode_with(
    bytes: &[u8],
    encoding: crate::recipe::Encoding,
) -> Result<Cow<'_, str>, ImportError> {
    use crate::recipe::Encoding;
    let text: Cow<'_, str> = match encoding {
        Encoding::Auto => return decode_text(bytes),
        Encoding::Utf8 => {
            let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            String::from_utf8_lossy(body)
        }
        Encoding::Windows1252 => Cow::Owned(bytes.iter().map(|&b| windows_1252(b)).collect()),
        Encoding::Latin1 => Cow::Owned(bytes.iter().map(|&b| b as char).collect()),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let little = encoding == Encoding::Utf16Le;
            let body = bytes
                .strip_prefix(&[0xFF, 0xFE])
                .or_else(|| bytes.strip_prefix(&[0xFE, 0xFF]))
                .unwrap_or(bytes);
            let (pairs, _odd) = body.as_chunks::<2>();
            let units = pairs.iter().map(|&pair| {
                if little {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            });
            Cow::Owned(
                char::decode_utf16(units)
                    .map(|unit| unit.unwrap_or('\u{FFFD}'))
                    .collect(),
            )
        }
    };
    if text.chars().any(is_binary_control) {
        return Err(ImportError::NotText);
    }
    Ok(text)
}

/// Nom de l'encodage que [`decode_text`] retiendrait pour `bytes`, pour
/// l'afficher dans l'assistant d'import ; `None` pour un fichier binaire.
pub fn detect_encoding(bytes: &[u8]) -> Option<&'static str> {
    decode_text(bytes).ok()?;
    Some(if bytes.starts_with(&[0xFF, 0xFE]) {
        "UTF-16 (petit-boutiste)"
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        "UTF-16 (gros-boutiste)"
    } else {
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        if std::str::from_utf8(body).is_ok() {
            "UTF-8"
        } else {
            "Windows-1252"
        }
    })
}

fn decode_utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> Result<Cow<'_, str>, ImportError> {
    let (pairs, odd) = bytes.as_chunks::<2>();
    if !odd.is_empty() {
        return Err(ImportError::NotText);
    }
    let units = pairs.iter().map(|&pair| word(pair));
    let text: String = char::decode_utf16(units)
        .collect::<Result<_, _>>()
        .map_err(|_| ImportError::NotText)?;
    if text.chars().any(is_binary_control) {
        return Err(ImportError::NotText);
    }
    Ok(Cow::Owned(text))
}

fn is_binary_control(c: char) -> bool {
    c.is_ascii_control() && !matches!(c, '\t' | '\n' | '\r' | '\x0C' | '\x1A')
}

/// Un octet Windows-1252 vers son caractère. Identique à Latin-1 hors de
/// `0x80..=0x9F`, où Windows-1252 place des caractères typographiques ; les
/// cinq positions non attribuées retombent sur le contrôle C1 de Latin-1.
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_valide_est_emprunte_tel_quel() {
        let text = decode_text("λ;1,5\n".as_bytes()).unwrap();
        assert!(matches!(text, Cow::Borrowed("λ;1,5\n")));
    }

    #[test]
    fn marque_utf8_est_retiree() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFa;b").unwrap(), "a;b");
    }

    #[test]
    fn windows_1252_en_repli() {
        // « 19.91 °C » et « 5 € » écrits en page de code Windows française.
        assert_eq!(
            decode_text(b"T;19.91 \xB0C\nP;5 \x80").unwrap(),
            "T;19.91 °C\nP;5 €"
        );
    }

    #[test]
    fn utf16_avec_marque() {
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("x;°".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("x;°".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_text(&le).unwrap(), "x;°");
        assert_eq!(decode_text(&be).unwrap(), "x;°");
    }

    #[test]
    fn controles_binaires_refuses() {
        assert_eq!(decode_text(b"a\x00b"), Err(ImportError::NotText));
        assert_eq!(decode_text(b"a\x01\xFF"), Err(ImportError::NotText));
        assert_eq!(decode_text(&[0xFF, 0xFE, 0x61]), Err(ImportError::NotText));
    }

    #[test]
    fn controles_de_texte_acceptes() {
        assert_eq!(
            decode_text(b"a\tb\r\n\x0Cc\x1A").unwrap(),
            "a\tb\r\n\x0Cc\x1A"
        );
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::detect_encoding;

    #[test]
    fn names_the_encoding() {
        assert_eq!(detect_encoding(b"a;b\n"), Some("UTF-8"));
        assert_eq!(detect_encoding(&[b'1', 0xB0, b'C']), Some("Windows-1252"));
        assert_eq!(
            detect_encoding(&[0xFF, 0xFE, b'a', 0]),
            Some("UTF-16 (petit-boutiste)")
        );
        assert_eq!(detect_encoding(&[0, 1, 2, 3]), None);
    }
}

#[cfg(test)]
mod forced_encoding_tests {
    use super::decode_with;
    use crate::recipe::Encoding;

    #[test]
    fn a_forced_encoding_overrides_detection() {
        // « 1°C » en UTF-8 valide, lu de force comme Windows-1252 : deux
        // caractères pour le degré.
        let utf8 = "1°C".as_bytes();
        assert_eq!(decode_with(utf8, Encoding::Utf8).unwrap(), "1°C");
        assert_eq!(decode_with(utf8, Encoding::Windows1252).unwrap(), "1Â°C");
        assert_eq!(decode_with(&[b'1', 0xB0], Encoding::Latin1).unwrap(), "1°");
        assert_eq!(decode_with(&[b'1', 0xB0], Encoding::Auto).unwrap(), "1°");
    }

    #[test]
    fn utf16_without_a_bom_and_invalid_utf8_do_not_fail() {
        assert_eq!(
            decode_with(&[b'a', 0, b'b', 0], Encoding::Utf16Le).unwrap(),
            "ab"
        );
        assert_eq!(
            decode_with(&[0, b'a', 0, b'b'], Encoding::Utf16Be).unwrap(),
            "ab"
        );
        let bad = decode_with(&[b'a', 0xFF, b'b'], Encoding::Utf8).unwrap();
        assert!(bad.contains('\u{FFFD}'));
        assert!(decode_with(&[0, 1, 2], Encoding::Utf8).is_err());
    }
}
