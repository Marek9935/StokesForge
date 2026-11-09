//! The "To check" points of the file shown: what the screen is not sure of,
//! as sentences. They come from three places:
//!
//! * the **findings** of the check view that are doubts (a line that may be
//!   times, units that disagree, a count that does not match...);
//! * the **detector's own doubts** (`Sniff::doubts`: text rows inside the
//!   data, several blocks of numbers, a comma that may be a decimal, a JCAMP
//!   problem);
//! * a **tie** between two templates that meet as many conditions.
//!
//! A point already answered for this kind of file (`RecipeStore::is_remembered`)
//! is not listed. "Mark as Checked" answers a point without touching the
//! recipe and remembers the answer.

use stokesforge_import::jcamp::JcampIssue;
use stokesforge_import::sniff::Doubt;

use super::import_check::{CheckKind, Confidence, Piece};
use super::AppState;
use crate::tr;

/// Where a point to check comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToCheckSource {
    /// A doubt of the check view; the review asks its question.
    Finding(CheckKind),
    /// A doubt of the detector.
    Detector,
    /// Two templates meet as many conditions: `others` are the ones not used.
    Tie { others: Vec<String> },
}

/// What the import window says about the file shown, in its status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShownFileStatus {
    /// The file cannot be read.
    Refused,
    /// This many points to check (findings, detector doubts, a tie).
    Check(usize),
    /// Nothing to check.
    Ready,
}

/// One point of the "To check" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToCheckPoint {
    /// Id of the doubt: the key of the answer remembered for the kind of file.
    pub id: String,
    pub source: ToCheckSource,
    pub sentence: String,
    /// 1-based lines of the file to show for "Check Now".
    pub lines: Vec<usize>,
}

/// The id of the doubt "these templates fit the file as well": the names of
/// the template used and of the others, sorted, so that it does not depend on
/// which one won.
pub(super) fn tie_id(chosen: &str, others: &[String]) -> String {
    let mut names: Vec<&str> = std::iter::once(chosen)
        .chain(others.iter().map(String::as_str))
        .collect();
    names.sort_unstable();
    format!("template-tie:{}", names.join("|"))
}

/// The sentence of a doubt of the detector.
fn detector_sentence(doubt: &Doubt) -> String {
    match doubt {
        Doubt::TextRowsInData { count, lines } => {
            let count: usize = *count;
            let first: usize = lines.first().copied().unwrap_or(0);
            tr!(
                "import-doubt-text-rows-in-data",
                count = count,
                first = first
            )
        }
        Doubt::SeveralBlocks {
            count,
            rows_total,
            split,
        } => {
            let (count, rows): (usize, usize) = (*count, *rows_total);
            if *split {
                tr!(
                    "import-doubt-several-blocks-split",
                    count = count,
                    rows = rows
                )
            } else {
                tr!("import-doubt-several-blocks", count = count, rows = rows)
            }
        }
        Doubt::DecimalOrSeparator { line } => {
            let line: usize = *line;
            tr!("import-doubt-decimal-or-separator", line = line)
        }
        Doubt::Jcamp { block, issue } => {
            let block: usize = *block + 1;
            match issue {
                JcampIssue::NPointsMismatch { declared, decoded } => {
                    let (declared, decoded): (i64, usize) = (*declared, *decoded);
                    tr!(
                        "import-doubt-jcamp-npoints",
                        block = block,
                        declared = declared,
                        decoded = decoded
                    )
                }
                JcampIssue::NtuplesNotRead => {
                    tr!("import-doubt-jcamp-ntuples-not-read", block = block)
                }
                JcampIssue::UnreadableTable => {
                    tr!("import-doubt-jcamp-unreadable-table", block = block)
                }
                JcampIssue::BadLines { count } => {
                    let count: usize = *count;
                    tr!("import-doubt-jcamp-bad-lines", block = block, count = count)
                }
                JcampIssue::DupCapped => tr!("import-doubt-jcamp-dup-capped", block = block),
                JcampIssue::PointsCapped => {
                    tr!("import-doubt-jcamp-points-capped", block = block)
                }
                JcampIssue::NmrLeftOut { blocks } => {
                    let count: usize = *blocks;
                    tr!("import-doubt-jcamp-nmr-left-out", count = count)
                }
            }
        }
    }
}

impl AppState {
    /// The points to check for the file shown, in the order they are listed:
    /// the findings that are doubts, then the detector's, then a tie.
    pub fn to_check(&self) -> Vec<ToCheckPoint> {
        let (Some(state), Some((path, kind))) = (self.import_state.as_ref(), self.check_kind_id())
        else {
            return Vec::new();
        };
        let mut points = Vec::new();
        if let Some(view) = self.import_check() {
            for finding in view.doubts() {
                let sentence: String = finding
                    .pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Text(text) | Piece::Chip { text, .. } => text.as_str(),
                    })
                    .collect();
                points.push(ToCheckPoint {
                    id: finding.kind.doubt_id().to_string(),
                    source: ToCheckSource::Finding(finding.kind),
                    sentence,
                    lines: finding.evidence.iter().map(|e| e.line).collect(),
                });
            }
        }
        let template = state.checks.template_of_file.get(&path);
        // A template decided the file: the detector has no say.
        if template.is_none() {
            if let Some(Ok(found)) = state.auto.sniffs.get(&path) {
                for doubt in &found.doubts {
                    if state.store.is_remembered(&kind, doubt.id()) {
                        continue;
                    }
                    let lines = match doubt {
                        Doubt::TextRowsInData { lines, .. } => lines.clone(),
                        Doubt::DecimalOrSeparator { line } => vec![*line],
                        Doubt::SeveralBlocks { .. } => {
                            found.blocks.iter().map(|b| b.first_line).collect()
                        }
                        Doubt::Jcamp { .. } => Vec::new(),
                    };
                    points.push(ToCheckPoint {
                        id: doubt.id().to_string(),
                        source: ToCheckSource::Detector,
                        sentence: detector_sentence(doubt),
                        lines,
                    });
                }
            }
        }
        if let (Some(chosen), Some(others)) = (template, state.checks.tied_of_file.get(&path)) {
            if !others.is_empty() {
                let id = tie_id(chosen, others);
                if !state.store.is_remembered(&kind, &id) {
                    points.push(ToCheckPoint {
                        sentence: tr!(
                            "import-doubt-template-tie",
                            chosen = chosen.clone(),
                            others = others.join(", ")
                        ),
                        id,
                        source: ToCheckSource::Tie {
                            others: others.clone(),
                        },
                        lines: Vec::new(),
                    });
                }
            }
        }
        points
    }

    /// How many points the file shown has to check.
    pub fn to_check_count(&self) -> usize {
        self.to_check().len()
    }

    /// The status of the file shown: the same points as the footer counts,
    /// never only the check view's findings (a tie or a detector doubt is a
    /// point to check too).
    pub fn shown_file_status(&self) -> ShownFileStatus {
        let marks = self.import_file_marks();
        let shown = self
            .import_state()
            .map_or(0, |s| s.preview_index)
            .min(marks.len().saturating_sub(1));
        let refused = marks.get(shown).is_some_and(|mark| mark.refusal.is_some());
        if refused {
            return ShownFileStatus::Refused;
        }
        match self.to_check_count() {
            0 => ShownFileStatus::Ready,
            n => ShownFileStatus::Check(n),
        }
    }

    /// "Mark as Checked": the point is answered, the recipe stays as it is,
    /// and the answer is remembered for the kind of file.
    pub fn to_check_mark(&mut self, id: &str) {
        self.check_remember(id);
    }

    /// Does the check view still call `kind` a doubt? (For the review.)
    pub fn check_is_doubt(&self, kind: CheckKind) -> bool {
        self.import_check()
            .and_then(|v| v.finding(kind).map(|f| f.confidence == Confidence::Check))
            .unwrap_or(false)
    }
}
