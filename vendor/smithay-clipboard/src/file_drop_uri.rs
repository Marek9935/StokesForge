//! Conversion transitoire #72 : URI de dépôt vers chemins locaux.

use std::collections::HashSet;
use std::path::PathBuf;

/// URI locales seulement : décodage percent-encoded, commentaires et CRLF.
/// Une entrée distante/invalide ne devient jamais un chemin relatif.
pub(super) fn local_files(bytes: &[u8]) -> Vec<PathBuf> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        // url::Url normalise aussi « file:relatif » en chemin absolu : ce
        // n'est pas une URI de fichier absolue fournie par la source.
        if !line
            .split_once(':')
            .is_some_and(|(_, rest)| rest.starts_with('/'))
        {
            continue;
        }
        let Ok(uri) = url::Url::parse(line) else {
            continue;
        };
        if uri.scheme() != "file" || uri.query().is_some() || uri.fragment().is_some() {
            continue;
        }
        let Ok(path) = uri.to_file_path() else {
            continue;
        };
        use std::os::unix::ffi::OsStrExt;
        if path.is_absolute()
            && !path.as_os_str().as_bytes().contains(&0)
            && seen.insert(path.clone())
        {
            paths.push(path);
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::local_files;
    use std::path::PathBuf;

    #[test]
    fn uri_locales_commentaires_et_encodage() {
        assert_eq!(local_files(b"# comment\r\nfile:///tmp/a%20b.csv\r\nfile://localhost/tmp/c.csv\r\nfile:///tmp/a%20b.csv\r\n"),
            vec![PathBuf::from("/tmp/a b.csv"), PathBuf::from("/tmp/c.csv")]);
    }

    #[test]
    fn refuse_les_uri_distantes_relatives_et_invalides() {
        assert!(local_files(b"https://example.org/a\nfile://server/tmp/a\n/tmp/a\nfile:relative.csv\nfile:///tmp/a#frag\nfile:///tmp/a?query\nfile:///tmp/%00\n").is_empty());
        assert!(local_files(&[0xff]).is_empty());
    }

    #[test]
    fn preserve_accents_pourcentages_et_octets_de_nom_unix() {
        let paths = local_files(b"file:///tmp/%C3%A9chantillon%25.csv\nfile:///tmp/%FF.csv\n");
        assert_eq!(paths[0], PathBuf::from("/tmp/échantillon%.csv"));
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(paths[1].as_os_str().as_bytes(), b"/tmp/\xff.csv");
    }
}
