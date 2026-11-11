// SPDX-License-Identifier: GPL-3.0-or-later

//! The information tree Horiba's Origin-based software stores in each book.
//!
//! It sits in the window's label field, after the label: one or more
//! `@${[...]}<OriginStorage>...</OriginStorage>` blocks. The one holding an
//! `<HJYInfo>` element describes the experiment (software version, type,
//! slits, integration time...). It is flattened here into one metadata line
//! per non-empty leaf, keyed by its dotted path from the `OriginStorage`
//! root (`HJYInfo.ExpSummary.ExpType`). A small bounded scanner, not an XML
//! parser: a malformed tree keeps the lines read so far.

use crate::metadata::MetadataLine;

/// Deepest element nesting followed.
const MAX_DEPTH: usize = 32;
/// Lines kept at most.
const MAX_LINES: usize = 1024;

const ROOT_OPEN: &str = "<OriginStorage>";
const ROOT_CLOSE: &str = "</OriginStorage>";

struct Element<'a> {
    name: &'a str,
    text: String,
    has_child: bool,
}

/// The metadata lines of the Horiba tree found in `storage`, if any.
pub(crate) fn info_lines(storage: &str) -> Vec<MetadataLine> {
    let Some(info_at) = storage.find("<HJYInfo>") else {
        return Vec::new();
    };
    let start = storage[..info_at].rfind(ROOT_OPEN).unwrap_or(info_at);
    let tail = &storage[start..];
    let xml = tail.find(ROOT_CLOSE).map_or(tail, |end| &tail[..end]);
    flatten(xml)
}

fn flatten(xml: &str) -> Vec<MetadataLine> {
    let mut lines = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut i = 0;
    while i < xml.len() && lines.len() < MAX_LINES {
        let rest = &xml[i..];
        if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
            let Some(end) = cdata.find("]]>") else { break };
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&cdata[..end]);
            }
            i += "<![CDATA[".len() + end + "]]>".len();
        } else if rest.starts_with("<!--") || rest.starts_with("<?") {
            let close = if rest.starts_with("<!--") {
                "-->"
            } else {
                "?>"
            };
            let Some(end) = rest.find(close) else { break };
            i += end + close.len();
        } else if let Some(tag_body) = rest.strip_prefix('<') {
            let Some(end) = tag_body.find('>') else { break };
            let tag = &tag_body[..end];
            i += 1 + end + 1;
            if tag.starts_with('/') {
                let Some(done) = stack.pop() else { break };
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
                let value = clean(&done.text);
                if !done.has_child && !value.is_empty() {
                    let mut path: Vec<&str> = stack.iter().skip(1).map(|e| e.name).collect();
                    path.push(done.name);
                    lines.push(MetadataLine {
                        key: path.join("."),
                        value,
                    });
                }
                if stack.is_empty() {
                    break;
                }
            } else {
                let self_closing = tag.ends_with('/');
                let name = tag
                    .trim_end_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
                if self_closing {
                    continue;
                }
                if stack.len() >= MAX_DEPTH {
                    break;
                }
                stack.push(Element {
                    name,
                    text: String::new(),
                    has_child: false,
                });
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&unescape(&rest[..end]));
            }
            i += end;
        }
    }
    lines
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Origin writes a newline as the four characters `\D\A`; a metadata value
/// is one line.
fn clean(text: &str) -> String {
    text.replace("\\D\\A", " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(storage: &str) -> Vec<(String, String)> {
        info_lines(storage)
            .into_iter()
            .map(|l| (l.key, l.value))
            .collect()
    }

    #[test]
    fn flattens_leaves_by_path_and_skips_empty_ones() {
        let got = pairs(
            "@${[0|4|TREE|1|2]}<OriginStorage><ExpProperties><Start></Start></ExpProperties>\
             <HJYInfo><ExpSummary><Comment><![CDATA[a <b> & c\\D\\A]]></Comment>\
             <ExpType>X &amp; Y</ExpType></ExpSummary></HJYInfo></OriginStorage>@${[x]}<OriginStorage><HWND>0</HWND></OriginStorage>",
        );
        assert_eq!(
            got,
            vec![
                ("HJYInfo.ExpSummary.Comment".into(), "a <b> & c".into()),
                ("HJYInfo.ExpSummary.ExpType".into(), "X & Y".into()),
            ]
        );
    }

    #[test]
    fn no_tree_no_lines_and_malformed_tree_keeps_what_was_read() {
        assert!(info_lines("@${[x]}<OriginStorage><HWND>0</HWND></OriginStorage>").is_empty());
        let got = pairs("<OriginStorage><HJYInfo><A>1</A><B>2</B><C");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn nesting_is_bounded() {
        let mut xml = String::from("<OriginStorage><HJYInfo>");
        for _ in 0..10_000 {
            xml.push_str("<a>");
        }
        assert!(info_lines(&xml).is_empty());
    }
}
