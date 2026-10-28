//! Grammar of Origin object paths: `[Book]Sheet!Col`, with optional folders
//! as a prefix (`/Manips/[Book1]Sheet1!ColA`).

use crate::error::{CoreError, CoreResult};
use crate::name::RESERVED_NAME_CHARS;

/// Deepest level designated by a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathDepth {
    /// The path designates a folder (or the root).
    Folder,
    /// The path designates a window (workbook, graph, matrix, note).
    Window,
    /// The path designates a sheet inside a workbook.
    Sheet,
    /// The path designates a column inside a sheet.
    Column,
}

/// Parsed path. Fields are `None` when absent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectPath {
    /// Folder segments, from the root (or the relative starting point) towards the sheet.
    pub folders: Vec<String>,
    /// Name of the window (workbook, graph, matrix, note), if present.
    pub window: Option<String>,
    /// Name of the sheet, if present.
    pub sheet: Option<String>,
    /// Name of the column, if present.
    pub column: Option<String>,
    /// `true` if the path is anchored at the root (prefixed by `/`).
    pub rooted: bool,
}

impl ObjectPath {
    /// Deepest level designated by this path.
    pub fn depth(&self) -> PathDepth {
        if self.column.is_some() {
            PathDepth::Column
        } else if self.sheet.is_some() {
            PathDepth::Sheet
        } else if self.window.is_some() {
            PathDepth::Window
        } else {
            PathDepth::Folder
        }
    }
}

impl std::fmt::Display for ObjectPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format_path(self))
    }
}

impl std::str::FromStr for ObjectPath {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_path(s)
    }
}

/// Validates a path segment (folder, window, sheet or column): not
/// empty, no edge space, no reserved character.
fn check_segment(segment: &str) -> CoreResult<&str> {
    if segment.is_empty() || segment != segment.trim() {
        return Err(CoreError::Path(format!("invalid segment: {segment}")));
    }
    if segment.contains(RESERVED_NAME_CHARS) {
        return Err(CoreError::Path(format!("invalid segment: {segment}")));
    }
    Ok(segment)
}

/// Splits a folder prefix (`Manips/Jour1/`) into validated segments,
/// ignoring empty segments (repeated `//`).
fn parse_folder_segments(prefix: &str) -> CoreResult<Vec<String>> {
    prefix
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| check_segment(segment).map(str::to_string))
        .collect()
}

/// Parses a path. `Err(CoreError::Path(_))` if the syntax is invalid.
pub fn parse_path(text: &str) -> CoreResult<ObjectPath> {
    let raw = text.trim();
    if raw.is_empty() {
        return Err(CoreError::Path("empty path".to_string()));
    }

    if !raw.contains('[') {
        if raw.contains(']') || raw.contains('!') {
            return Err(CoreError::Path("malformed path".to_string()));
        }
        let rooted = raw.starts_with('/');
        let folders = parse_folder_segments(raw)?;
        if folders.is_empty() && !rooted {
            return Err(CoreError::Path("malformed path".to_string()));
        }
        return Ok(ObjectPath {
            folders,
            rooted,
            ..Default::default()
        });
    }

    let Some((prefix, rest)) = raw.split_once('[') else {
        return Err(CoreError::Path("malformed path".to_string()));
    };

    if !prefix.is_empty() && !prefix.ends_with('/') {
        return Err(CoreError::Path(
            "folder prefix not ending with '/'".to_string(),
        ));
    }
    let rooted = prefix.starts_with('/');
    let folders = parse_folder_segments(prefix)?;

    let Some((window, tail)) = rest.split_once(']') else {
        return Err(CoreError::Path("missing closing bracket".to_string()));
    };
    let window = check_segment(window)?.to_string();

    if tail.contains('[') || tail.contains(']') {
        return Err(CoreError::Path("extra bracket".to_string()));
    }

    let (sheet_part, bang_present, column_part) = match tail.split_once('!') {
        Some((sheet_part, column_part)) => (sheet_part, true, column_part),
        None => (tail, false, ""),
    };

    let sheet = if sheet_part.is_empty() {
        None
    } else {
        Some(check_segment(sheet_part)?.to_string())
    };

    let column = if bang_present {
        let column = check_segment(column_part)?.to_string();
        if sheet.is_none() {
            return Err(CoreError::Path("column without a worksheet".to_string()));
        }
        Some(column)
    } else {
        None
    };

    Ok(ObjectPath {
        folders,
        window: Some(window),
        sheet,
        column,
        rooted,
    })
}

/// Canonical text form.
pub fn format_path(path: &ObjectPath) -> String {
    let mut prefix = path.folders.join("/");
    if path.rooted {
        prefix = format!("/{prefix}");
    }

    let Some(window) = &path.window else {
        return if prefix.is_empty() {
            "/".to_string()
        } else {
            prefix
        };
    };

    if !prefix.is_empty() && !prefix.ends_with('/') {
        prefix.push('/');
    }
    let mut out = format!("{prefix}[{window}]");
    if let Some(sheet) = &path.sheet {
        out.push_str(sheet);
    }
    if let Some(column) = &path.column {
        out.push('!');
        out.push_str(column);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_full_column_path() {
        let path = parse_path("[Book1]Sheet1!ColA").expect("valid path");
        assert!(path.folders.is_empty());
        assert_eq!(path.window.as_deref(), Some("Book1"));
        assert_eq!(path.sheet.as_deref(), Some("Sheet1"));
        assert_eq!(path.column.as_deref(), Some("ColA"));
        assert_eq!(path.depth(), PathDepth::Column);
        assert!(!path.rooted);
    }

    #[test]
    fn parses_sheet_path() {
        let path = parse_path("[Book1]Sheet1").expect("valid path");
        assert_eq!(path.depth(), PathDepth::Sheet);
        assert_eq!(path.column, None);
    }

    #[test]
    fn parses_window_only_path() {
        let path = parse_path("[Graph1]").expect("valid path");
        assert_eq!(path.depth(), PathDepth::Window);
    }

    #[test]
    fn parses_root() {
        let path = parse_path("/").expect("valid path");
        assert!(path.folders.is_empty());
        assert!(path.rooted);
        assert_eq!(path.depth(), PathDepth::Folder);
    }

    #[test]
    fn parses_rooted_folder_path() {
        let path = parse_path("/Manips/Jour1").expect("valid path");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert!(path.rooted);
    }

    #[test]
    fn parses_relative_folder_path() {
        let path = parse_path("Manips/Jour1").expect("valid path");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert!(!path.rooted);
    }

    #[test]
    fn ignores_empty_segments_between_double_slashes() {
        let path = parse_path("/Manips//Jour1").expect("valid path");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
    }

    #[test]
    fn parses_folder_prefixed_column_path() {
        let path = parse_path("/Manips/Jour1/[Book1]Sheet1!ColA").expect("valid path");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert_eq!(path.window.as_deref(), Some("Book1"));
        assert!(path.rooted);
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let path = parse_path("  [Book1]Sheet1  ").expect("valid path");
        assert_eq!(path.sheet.as_deref(), Some("Sheet1"));
    }

    #[test]
    fn rejects_malformed_paths() {
        let cases = [
            "",
            "   ",
            "[Book1",
            "Book1]Sheet1",
            "[]Sheet1",
            "[Book1]!ColA",
            "Manips[Book1]Sheet1",
            "[Book1]Sheet1!",
            "[Book1]Sheet[2]",
            "a!b",
        ];
        for case in cases {
            assert!(
                matches!(parse_path(case), Err(CoreError::Path(_))),
                "expected CoreError::Path for {case:?}"
            );
        }
    }

    #[test]
    fn round_trips_through_format_path() {
        let cases = [
            "/",
            "/Manips",
            "/Manips/Jour1",
            "[Book1]",
            "[Book1]Sheet1",
            "[Book1]Sheet1!ColA",
            "/Manips/[Book1]Sheet1!ColA",
        ];
        for case in cases {
            let parsed = parse_path(case).expect("valid path");
            assert_eq!(format_path(&parsed), case, "round-trip for {case:?}");
            assert_eq!(parsed.to_string(), case, "Display for {case:?}");
        }
    }

    #[test]
    fn from_str_matches_parse_path() {
        let case = "[Book1]Sheet1!ColA";
        assert_eq!(ObjectPath::from_str(case), parse_path(case));
        assert_eq!(case.parse::<ObjectPath>(), parse_path(case));
    }
}
