//! Source-aware Markdown editing helpers. The draft is always plain Markdown.
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Bold,
    Italic,
    Code,
}

pub fn faces(text: &str) -> Vec<(Range<usize>, Face)> {
    Parser::new_ext(text, Options::ENABLE_TASKLISTS)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            let face = match event {
                Event::Start(Tag::Strong) => Face::Bold,
                Event::Start(Tag::Emphasis) => Face::Italic,
                Event::Start(Tag::CodeBlock(_)) | Event::Code(_) => Face::Code,
                _ => return None,
            };
            Some((range, face))
        })
        .collect()
}

/// Neutral, source-relative code colors. Markdown's injection registry uses
/// every language bundled by zeron-syntax, just like the transcript renderer.
/// Work is bounded and runs on the background executor; unsupported fences
/// retain their ordinary code styling.
pub fn syntax_spans(text: &str) -> Vec<zeron_syntax::HighlightSpan> {
    let Ok(document) = zeron_syntax::highlight_with_limits(
        zeron_syntax::HighlightRequest {
            source: text,
            path: None,
            fence_tag: Some("markdown"),
        },
        zeron_syntax::HighlightLimits {
            max_source_bytes: 128 * 1024,
            max_spans: 16_000,
        },
        None,
    ) else {
        return Vec::new();
    };
    let mut offset = 0;
    text.split('\n')
        .zip(document.lines)
        .flat_map(|(line, spans)| {
            let start = offset;
            offset += line.len() + 1;
            spans
                .into_iter()
                .map(move |span| zeron_syntax::HighlightSpan {
                    range: start + span.range.start..start + span.range.end,
                    kind: span.kind,
                })
        })
        .collect()
}

pub fn in_code(text: &str, cursor: usize) -> bool {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return false;
    }
    let mut parsed_code = Vec::new();
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TASKLISTS).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                if range.start <= cursor && cursor < range.end {
                    return true;
                }
                if range.end == cursor {
                    match kind {
                        pulldown_cmark::CodeBlockKind::Indented => return true,
                        pulldown_cmark::CodeBlockKind::Fenced(_) => {
                            let source = &text[range.clone()];
                            let mut lines = source.lines();
                            let opening = lines.next().unwrap_or_default().trim_start();
                            let delimiter = opening.chars().next().unwrap_or('`');
                            let count = opening.chars().take_while(|c| *c == delimiter).count();
                            let closed = lines.last().is_some_and(|line| {
                                let line = line.trim();
                                line.len() >= count && line.chars().all(|c| c == delimiter)
                            });
                            if !closed {
                                return true;
                            }
                        }
                    }
                }
                parsed_code.push(range);
            }
            Event::Code(_) => {
                if range.start <= cursor && cursor < range.end {
                    return true;
                }
                parsed_code.push(range);
            }
            _ => {}
        }
    }
    // Pulldown intentionally leaves unfinished inline spans as prose. Match
    // delimiter RUNS, skipping valid parsed spans (which can contain backticks
    // of another length), rather than counting individual backticks.
    let before = &text[..cursor];
    let mut at = before.rfind("\n\n").map_or(0, |i| i + 2);
    let bytes = before.as_bytes();
    let mut delimiter = None;
    while at < bytes.len() {
        if let Some(range) = parsed_code.iter().find(|range| range.contains(&at)) {
            at = range.end;
        } else if bytes[at] == b'\\' && delimiter.is_none() {
            at += 2;
        } else if bytes[at] == b'`' {
            let count = bytes[at..].iter().take_while(|b| **b == b'`').count();
            if delimiter == Some(count) {
                delimiter = None;
            } else if delimiter.is_none() {
                delimiter = Some(count);
            }
            at += count;
        } else {
            at += 1;
        }
    }
    delimiter.is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPrefix {
    pub indent: usize,
    pub end: usize,
    pub next: String,
    pub bullet: Option<usize>,
}

pub fn list_prefix(line: &str) -> Option<ListPrefix> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let (marker_len, next, bullet) = if matches!(rest, "-" | "*" | "+") {
        (1, format!("{rest} "), Some(indent))
    } else if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        (2, rest[..2].to_string(), Some(indent))
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let delimiter = rest.get(digits..digits + 2)?;
        if delimiter != ". " && delimiter != ") " {
            return None;
        }
        let number: u32 = rest[..digits].parse().ok()?;
        (digits + 2, format!("{}{delimiter}", number + 1), None)
    };
    let task = rest
        .get(marker_len..)
        .is_some_and(|s| s.starts_with("[ ] ") || s.starts_with("[x] ") || s.starts_with("[X] "));
    Some(ListPrefix {
        indent,
        end: indent + marker_len + if task { 4 } else { 0 },
        next: format!(
            "{}{next}{}",
            &line[..indent],
            if task { "[ ] " } else { "" }
        ),
        bullet,
    })
}

pub fn newline_edit(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    if in_code(text, cursor) {
        return None;
    }
    let start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
    let line = &text[start..end];
    let prefix = list_prefix(line)?;
    if cursor < start + prefix.end {
        return None;
    }
    if line[prefix.end..].trim().is_empty() {
        // Leaving an empty nested item outdents one level first.
        let retained = prefix.indent.saturating_sub(2);
        return Some((start..end, line[..retained].to_string()));
    }
    Some((cursor..cursor, format!("\n{}", prefix.next)))
}

/// Hidden delimiters outside the active logical line, and typographic bullets.
/// Each replacement retains a source range for caret/IME mapping.
pub fn decorations(text: &str, active: Range<usize>) -> Vec<(Range<usize>, String)> {
    let mut edits = Vec::new();
    let faces = faces(text);
    for (range, face) in faces.iter().cloned() {
        if range.start <= active.end && range.end >= active.start {
            continue;
        }
        let source = &text[range.clone()];
        let n = match face {
            Face::Bold => 2,
            Face::Italic => 1,
            Face::Code if source.starts_with('`') && !source.starts_with("```") => {
                source.bytes().take_while(|b| *b == b'`').count()
            }
            Face::Code => continue,
        };
        if range.len() > 2 * n {
            edits.push((range.start..range.start + n, String::new()));
            edits.push((range.end - n..range.end, String::new()));
        }
    }
    let mut at = 0;
    for line in text.split('\n') {
        if !faces
            .iter()
            .any(|(r, f)| *f == Face::Code && r.contains(&at))
        {
            if let Some(prefix) = list_prefix(line) {
                if let Some(bullet) = prefix.bullet {
                    if !(at <= active.end && at + line.len() >= active.start)
                        && line
                            .get(bullet + 2..bullet + 5)
                            .is_some_and(|s| matches!(s, "[ ]" | "[x]" | "[X]"))
                    {
                        let checked = line.as_bytes()[bullet + 3] != b' ';
                        edits.push((
                            at + bullet..at + bullet + 5,
                            if checked { "☑" } else { "☐" }.into(),
                        ));
                    } else {
                        edits.push((at + bullet..at + bullet + 1, "•".into()));
                    }
                }
            }
        }
        at += line.len() + 1;
    }
    edits.sort_by_key(|(r, _)| r.start);
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_boundaries_and_delimiter_runs() {
        for text in [
            "    - shell-command",
            "    $skill",
            "``code $",
            "````rust\ncode\n```",
        ] {
            assert!(in_code(text, text.len()), "{text:?}");
        }
        for text in [
            "``literal ` backtick`` $skill",
            "```rust\ncode\n```",
            "`code` $skill",
        ] {
            assert!(!in_code(text, text.len()), "{text:?}");
        }
        assert_eq!(newline_edit("    - shell-command", 19), None);
    }

    #[test]
    fn injected_colors_use_source_offsets_after_unicode_and_mentions() {
        let text = "héllo [file](zeron-file:src/main.rs)\n```rust\nfn main() { let café = 42; }\n```\n```python\ndef hello(): pass\n```";
        let spans = syntax_spans(text);
        assert!(spans.iter().any(|span| &text[span.range.clone()] == "fn"
            && span.kind == zeron_syntax::HighlightKind::Keyword));
        assert!(spans.iter().any(|span| &text[span.range.clone()] == "def"
            && span.kind == zeron_syntax::HighlightKind::Keyword));
        assert!(syntax_spans(&"x".repeat(128 * 1024 + 1)).is_empty());
    }

    #[test]
    fn list_continuation_exit_and_code() {
        assert_eq!(newline_edit("9. item", 7), Some((7..7, "\n10. ".into())));
        assert_eq!(
            newline_edit("  - [x] done", 12),
            Some((12..12, "\n  - [ ] ".into()))
        );
        assert_eq!(newline_edit("- ", 2), Some((0..2, "".into())));
        assert_eq!(newline_edit("```\n- item", 10), None);
        assert!(in_code("say `/$", 7));
        assert!(!in_code("say \\` $", 8));
    }
    #[test]
    fn empty_bullets_and_active_items_render_as_bullets() {
        let text = "-\n- \n-";
        let edits = decorations(text, 5..6);
        assert_eq!(
            edits,
            vec![(0..1, "•".into()), (2..3, "•".into()), (5..6, "•".into())]
        );
        assert_eq!(newline_edit("-", 1), Some((0..1, String::new())));
        assert_eq!(decorations("- item", 0..6), vec![(0..1, "•".into())]);
        assert!(list_prefix("-word").is_none());
        assert!(decorations("```\n-\n```", 0..3).is_empty());
    }

    #[test]
    fn decoration_keeps_active_syntax_editable() {
        let text = "**bold**\n- item\nactive";
        assert!(decorations(text, 0..8).iter().all(|(r, _)| r.start >= 9));
        let edits = decorations(text, 16..22);
        assert!(edits.contains(&(0..2, String::new())));
        assert!(edits.contains(&(9..10, "•".into())));
    }
}
