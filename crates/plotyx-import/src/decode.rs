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
