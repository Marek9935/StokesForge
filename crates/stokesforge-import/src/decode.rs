//! Decoding of a file's bytes into text, before any splitting.
//!
//! Device software on Windows (Jasco's Spectra Manager, among others) still
//! writes its exports in the local code page. In France that is Windows-1252:
//! "19.91 °C" gives the byte `0xB0` there, invalid in UTF-8. Such a file is
//! not corrupt; rejecting it would fail the whole import for a single
//! character in a footer comment.
//!
//! Order of attempts, from safest to most permissive:
//!
//! 1. UTF-16 byte order mark (`FF FE` / `FE FF`) → UTF-16;
//! 2. valid UTF-8 (`EF BB BF` mark removed if present) → as is;
//! 3. otherwise, **mixed decoding**: each valid UTF-8 sequence is kept, each
//!    invalid byte is read as Windows-1252 (an almost-UTF-8 file with a single
//!    invalid byte keeps its other accents);
//! 4. in every case, if the bytes do not look like text → rejected.
//!
//! Accepted risk: a file really written in Windows-1252 whose two consecutive
//! bytes happen to form a valid UTF-8 sequence (`Ã©`, bytes `C3 A9`) is read
//! as `é`, not `Ã©`. For this rare case, the encoding is forced in the recipe
//! (`Encoding::Windows1252`).
//!
//! A fully Windows-1252 file (with no valid UTF-8 sequence) is read as
//! before. Windows-1252 accepts all 256 bytes without failure; so it is the
//! "looks like text" filter that tells a text file from a binary: no C0
//! control character other than tab, line feed, carriage return, form feed,
//! and `0x1A` (DOS end of file). A binary file almost always contains some in
//! its first bytes; a text export never does.

use std::borrow::Cow;

use crate::parse::ImportError;

/// Decodes `bytes` into text in the order described in the module doc.
/// `Err(ImportError::NotText)` for a binary file. Borrows only in the common
/// case (valid UTF-8); the other cases allocate a copy.
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
        Err(_) => Cow::Owned(decode_mixed(bytes)),
    };
    if text.chars().any(is_binary_control) {
        return Err(ImportError::NotText);
    }
    Ok(text)
}

/// Mixed decoding: valid UTF-8 sequences are kept, each invalid byte is read
/// as Windows-1252. Panics on no byte (invariant 10).
fn decode_mixed(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(chunk.invalid().iter().map(|&b| windows_1252(b)));
    }
    out
}

/// Decodes `bytes` with the encoding **chosen** in the recipe
/// ([`crate::recipe::Encoding`]); `Auto` behaves like [`decode_text`]. A
/// forced encoding never fails the import for an invalid byte (it becomes
/// `U+FFFD`); only a binary-like file is rejected.
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

/// Name of the encoding [`decode_text`] would pick for `bytes`, to show it in
/// the import wizard; `None` for a binary file.
pub fn detect_encoding(bytes: &[u8]) -> Option<&'static str> {
    decode_text(bytes).ok()?;
    Some(if bytes.starts_with(&[0xFF, 0xFE]) {
        "UTF-16 (little-endian)"
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        "UTF-16 (big-endian)"
    } else {
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        if std::str::from_utf8(body).is_err() {
            if !has_valid_multibyte(body) {
                "Windows-1252"
            } else {
                "UTF-8 (+ Windows-1252)"
            }
        } else {
            "UTF-8"
        }
    })
}

/// True if `bytes` contains at least one valid multi-byte UTF-8 sequence
/// (otherwise mixed decoding does not differ from pure Windows-1252).
fn has_valid_multibyte(bytes: &[u8]) -> bool {
    bytes.utf8_chunks().any(|c| !c.valid().is_ascii())
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

/// A Windows-1252 byte to its character. Identical to Latin-1 outside
/// `0x80..=0x9F`, where Windows-1252 places typographic characters; the five
/// unassigned positions fall back to Latin-1's C1 control.
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
    fn valid_utf8_is_borrowed_as_is() {
        let text = decode_text("λ;1,5\n".as_bytes()).unwrap();
        assert!(matches!(text, Cow::Borrowed("λ;1,5\n")));
    }

    #[test]
    fn utf8_bom_is_removed() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFa;b").unwrap(), "a;b");
    }

    #[test]
    fn windows_1252_as_a_fallback() {
        // "19.91 °C" and "5 €" written in the French Windows code page.
        assert_eq!(
            decode_text(b"T;19.91 \xB0C\nP;5 \x80").unwrap(),
            "T;19.91 °C\nP;5 €"
        );
    }

    #[test]
    fn an_isolated_invalid_byte_in_utf8_is_read_as_windows_1252() {
        // "°C ok" in valid UTF-8, then a lone 0xB0: the first degree sign is
        // kept, the second read as Windows-1252, so no global mojibake.
        let mut bytes = "°C ok".as_bytes().to_vec();
        bytes.push(0xB0);
        assert_eq!(decode_text(&bytes).unwrap(), "°C ok°");
    }

    #[test]
    fn byte_e9_followed_by_ascii_gives_e_acute() {
        assert_eq!(decode_text(b"caf\xE9 noir").unwrap(), "café noir");
        // Lead byte truncated at the end of the file: no panic.
        assert_eq!(decode_text(b"a\xE2\x82").unwrap(), "a\u{e2}\u{201a}");
    }

    #[test]
    fn utf16_with_a_bom() {
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
    fn binary_controls_are_rejected() {
        assert_eq!(decode_text(b"a\x00b"), Err(ImportError::NotText));
        assert_eq!(decode_text(b"a\x01\xFF"), Err(ImportError::NotText));
        assert_eq!(decode_text(&[0xFF, 0xFE, 0x61]), Err(ImportError::NotText));
    }

    #[test]
    fn text_controls_are_accepted() {
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
        let mut mixed = "°C ok".as_bytes().to_vec();
        mixed.push(0xB0);
        assert_eq!(detect_encoding(&mixed), Some("UTF-8 (+ Windows-1252)"));
        assert_eq!(
            detect_encoding(&[0xFF, 0xFE, b'a', 0]),
            Some("UTF-16 (little-endian)")
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
        // "1°C" in valid UTF-8, forcibly read as Windows-1252: two characters
        // for the degree sign.
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
