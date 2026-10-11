//! Exec tool rows read as shell: the chip header shows the command in mono
//! with bash syntax tones instead of a flat sentence, and the expanded
//! invocation is a `$ `-prompted shell block in the same tones.
//!
//! Tones are tokenized once, when the row model is built (rows are cached by
//! fingerprint, and built off the UI thread), and resolved to theme colors
//! only at paint, so a theme switch never re-tokenizes.

use std::ops::Range;

use gpui::{
    AnyElement, FontFeatures, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    StyledText, TextRun, div, px,
};
use zeron_proto::shell_command::{ExecBadge, ExecVerb, summarize};
use zeron_syntax::{HighlightKind, HighlightSpan};

use super::{
    CALL_WRAP_COLS, OUTPUT_DETAIL_MAX_LINES, TOOL_TEXT_SIZE, ToolDetail, ToolMetrics, WRAP_INDENT,
    host_badge, path_badge, single_line, wrap_ranges,
};
use crate::{theme::Theme, typography::ui_rems};

/// A one-line header never shows more than this many chars; the cap bounds
/// tokenizing a pathological one-liner whose tail is ellipsized anyway.
const HEADER_MAX_CHARS: usize = 320;
/// Sources above this render in the plain tone rather than tokenizing.
const HIGHLIGHT_MAX_BYTES: usize = 16 * 1024;
/// The shell block's first line opens with the prompt; every later line takes
/// a gutter of the same width, so the command text keeps one left edge.
const PROMPT: &str = "$ ";
const GUTTER: &str = "  ";

/// The paint role of one shell token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellTone {
    /// A command name or shell keyword: the brightest token.
    Program,
    /// `-x`/`--flag` and operators: plumbing, muted.
    Flag,
    /// Quoted strings, and single-argument subjects (a search pattern).
    String,
    Comment,
    /// The shell block's `$ ` prompt.
    Prompt,
    /// Everything else, one step below the program.
    Plain,
}

impl ShellTone {
    fn from_kind(kind: HighlightKind) -> Self {
        match kind {
            HighlightKind::Function | HighlightKind::Keyword => Self::Program,
            HighlightKind::Constant | HighlightKind::Operator => Self::Flag,
            HighlightKind::String | HighlightKind::StringSpecial | HighlightKind::Escape => {
                Self::String
            }
            HighlightKind::Comment => Self::Comment,
            _ => Self::Plain,
        }
    }

    fn color(self, theme: &Theme) -> gpui::Hsla {
        match self {
            Self::Program => theme.text,
            Self::Flag | Self::Prompt => theme.text_muted,
            Self::String => theme.syntax.string,
            Self::Comment => theme.syntax.comment,
            Self::Plain => theme.text.opacity(0.85),
        }
    }
}

/// One line of shell text with its tones.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellLine {
    pub(super) text: SharedString,
    /// Contiguous `(byte len, tone)` runs covering `text`.
    pub(super) tones: Vec<(usize, ShellTone)>,
}

impl ShellLine {
    fn plain(text: String, tone: ShellTone) -> Self {
        let tones = if text.is_empty() {
            Vec::new()
        } else {
            vec![(text.len(), tone)]
        };
        Self {
            text: text.into(),
            tones,
        }
    }

    /// `text` tokenized as one line of bash.
    fn highlighted(text: String) -> Self {
        let spans = highlight_lines(&text)
            .and_then(|lines| lines.into_iter().next())
            .unwrap_or_default();
        let tones = tones_for(text.len(), &spans);
        Self {
            text: text.into(),
            tones,
        }
    }

    /// Mono text runs; a failed call paints in the danger tint instead.
    pub(super) fn text_runs(&self, theme: &Theme, failed: bool) -> Vec<TextRun> {
        let font = gpui::font(theme.font_mono.clone());
        self.tones
            .iter()
            .map(|&(len, tone)| TextRun {
                len,
                font: font.clone(),
                color: if failed {
                    theme.danger
                } else {
                    tone.color(theme)
                },
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect()
    }

    pub(super) fn element(&self, theme: &Theme, failed: bool) -> StyledText {
        StyledText::new(self.text.clone()).with_runs(self.text_runs(theme, failed))
    }
}

/// Line-relative bash spans for `source`, or `None` when it is too large or
/// the grammar fails (the caller then paints plain).
fn highlight_lines(source: &str) -> Option<Vec<Vec<HighlightSpan>>> {
    if source.len() > HIGHLIGHT_MAX_BYTES {
        return None;
    }
    zeron_syntax::highlight(zeron_syntax::HighlightRequest {
        source,
        path: None,
        fence_tag: Some("bash"),
    })
    .ok()
    .map(|document| document.lines)
}

/// Fill the gaps between `spans` with [`ShellTone::Plain`] and merge equal
/// neighbours, so the runs cover exactly `len` bytes.
fn tones_for(len: usize, spans: &[HighlightSpan]) -> Vec<(usize, ShellTone)> {
    let mut tones: Vec<(usize, ShellTone)> = Vec::new();
    let mut push = |n: usize, tone: ShellTone| {
        if n == 0 {
            return;
        }
        match tones.last_mut() {
            Some((last, last_tone)) if *last_tone == tone => *last += n,
            _ => tones.push((n, tone)),
        }
    };
    let mut cursor = 0;
    for span in spans {
        let start = span.range.start.clamp(cursor, len);
        let end = span.range.end.clamp(start, len);
        push(start - cursor, ShellTone::Plain);
        push(end - start, ShellTone::from_kind(span.kind));
        cursor = end;
    }
    push(len - cursor, ShellTone::Plain);
    tones
}

/// `tones` restricted to the byte `range` of the text they cover.
fn slice_tones(tones: &[(usize, ShellTone)], range: Range<usize>) -> Vec<(usize, ShellTone)> {
    let mut out = Vec::new();
    let mut start = 0;
    for &(len, tone) in tones {
        let end = start + len;
        let overlap = end.min(range.end).saturating_sub(start.max(range.start));
        if overlap > 0 {
            out.push((overlap, tone));
        }
        start = end;
    }
    out
}

/// The expanded invocation of an Exec call: the raw command verbatim (no
/// plumbing stripped), tokenized as one bash document so multi-line
/// constructs tone correctly, then cut into visual lines within
/// [`CALL_WRAP_COLS`] including the prompt gutter. `None` for a blank
/// command.
pub(super) fn shell_block(command: &str) -> Option<ToolDetail> {
    let source = command.trim_start_matches(['\n', '\r']).trim_end();
    if source.is_empty() {
        return None;
    }
    let spans = highlight_lines(source).unwrap_or_default();
    let cols = CALL_WRAP_COLS - PROMPT.len();
    let mut lines = Vec::new();
    let mut truncated_by = 0;
    for (line_ix, line) in source.lines().enumerate() {
        let ranges = wrap_ranges(line, cols);
        let room = OUTPUT_DETAIL_MAX_LINES - lines.len();
        truncated_by += ranges.len().saturating_sub(room);
        if room == 0 {
            continue;
        }
        let tones = tones_for(
            line.len(),
            spans.get(line_ix).map(Vec::as_slice).unwrap_or_default(),
        );
        for (chunk_ix, range) in ranges.into_iter().take(room).enumerate() {
            let (lead, lead_tone) = match (lines.is_empty(), chunk_ix) {
                (true, _) => (PROMPT.to_owned(), ShellTone::Prompt),
                (false, 0) => (GUTTER.to_owned(), ShellTone::Plain),
                (false, _) => (format!("{GUTTER}{WRAP_INDENT}"), ShellTone::Plain),
            };
            let mut line_tones = vec![(lead.len(), lead_tone)];
            line_tones.extend(slice_tones(&tones, range.clone()));
            lines.push(ShellLine {
                text: format!("{lead}{}", &line[range]).into(),
                tones: line_tones,
            });
        }
    }
    Some(ToolDetail::Shell {
        lines,
        truncated_by,
    })
}

/// Whether a search's scope path is a folder. The command only names the
/// path, so this is a cheap guess rather than a filesystem check: a trailing
/// `/`, `.` or `..`, or a last component without an extension (`src/shell`)
/// reads as a folder; anything with one (`main.rs`) as a file. A bare file
/// like `Makefile` is therefore shown as a folder.
pub(super) fn path_is_directory(path: &str) -> bool {
    path.ends_with('/')
        || matches!(path, "." | "..")
        || std::path::Path::new(path).extension().is_none()
}

/// The one-line form of a header subject, capped at [`HEADER_MAX_CHARS`].
fn header_text(text: &str) -> String {
    let line = single_line(text);
    match line.char_indices().nth(HEADER_MAX_CHARS) {
        Some((cut, _)) => line[..cut].to_owned(),
        None => line,
    }
}

/// What trails an Exec header's command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellTrailer {
    /// Further commands after the first, as a "+N" pill.
    More(usize),
    /// A script body's line count.
    Lines(usize),
}

/// The badge a header shows for its subject's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeaderBadge {
    /// A file or folder, with its file-type icon.
    Path(ExecBadge),
    /// A web host, behind the globe. Leads the header: the host is the
    /// fact, the page after it the detail.
    Host(String),
}

/// What an Exec chip header shows after its verb.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShellHeader {
    /// What the command did, so the row's icon can follow it: a `cat` row
    /// carries the Read tool's document, an `rg` row the magnifier.
    pub(super) verb: ExecVerb,
    /// The mono one-liner; `None` when the badge alone names the subject
    /// (a Read of one file, a listing of one folder).
    pub(super) command: Option<ShellLine>,
    /// The file or folder the command acts on, after the command (joined by
    /// "in") or alone; or the host a web call reached, before its page.
    pub(super) badge: Option<HeaderBadge>,
    pub(super) trailer: Option<ShellTrailer>,
}

/// Summarize and tokenize an Exec command for its chip header. `None` for a
/// blank command, which has nothing to show.
pub(super) fn shell_header(command: &str) -> Option<ShellHeader> {
    if command.trim().is_empty() {
        return None;
    }
    let summary = summarize(command);
    let argument = |text: &str| Some(ShellLine::plain(header_text(text), ShellTone::String));
    let header = match (summary.verb, summary.badge) {
        (ExecVerb::Read, Some(badge @ ExecBadge::File(_))) => ShellHeader {
            verb: summary.verb,
            command: None,
            badge: Some(HeaderBadge::Path(badge)),
            trailer: None,
        },
        (ExecVerb::Listed, Some(ExecBadge::Folder(folder))) => ShellHeader {
            verb: summary.verb,
            command: (summary.subject != folder)
                .then(|| argument(&summary.subject))
                .flatten(),
            badge: Some(HeaderBadge::Path(ExecBadge::Folder(folder))),
            trailer: None,
        },
        (ExecVerb::Searched, badge) => ShellHeader {
            verb: summary.verb,
            command: argument(&summary.subject),
            badge: badge.map(HeaderBadge::Path),
            trailer: None,
        },
        // A script's first line is the script's language, not bash.
        (ExecVerb::RanScript, _) => ShellHeader {
            verb: summary.verb,
            command: Some(ShellLine::plain(
                header_text(&summary.subject),
                ShellTone::Plain,
            )),
            badge: None,
            trailer: summary
                .script_lines
                .filter(|&n| n > 0)
                .map(ShellTrailer::Lines),
        },
        (ExecVerb::Ran | ExecVerb::Git, _) if summary.extra_segments > 0 => ShellHeader {
            verb: summary.verb,
            command: Some(ShellLine::highlighted(header_text(&summary.subject))),
            badge: None,
            trailer: Some(ShellTrailer::More(summary.extra_segments)),
        },
        // Git reads like any other Ran row: the whole `git …` line.
        (ExecVerb::Ran | ExecVerb::Git, _) => ShellHeader {
            verb: summary.verb,
            command: Some(ShellLine::highlighted(header_text(&summary.display))),
            badge: None,
            trailer: None,
        },
        (_, _) => ShellHeader {
            verb: summary.verb,
            command: Some(ShellLine::highlighted(header_text(&summary.subject))),
            badge: None,
            trailer: None,
        },
    };
    Some(header)
}

/// The native Search and Glob tools' header, in the shell search's form:
/// the pattern in the string tone, then "in" and the scope's badge. The same
/// action reads the same whether the agent ran `rg` or its search tool.
pub(super) fn pattern_header(
    verb: ExecVerb,
    pattern: &str,
    scope: Option<&str>,
) -> Option<ShellHeader> {
    if pattern.trim().is_empty() {
        return None;
    }
    Some(ShellHeader {
        verb,
        command: Some(ShellLine::plain(header_text(pattern), ShellTone::String)),
        badge: scope
            .filter(|scope| !scope.trim().is_empty())
            .map(|scope| HeaderBadge::Path(ExecBadge::Path(scope.to_owned()))),
        trailer: None,
    })
}

/// A WebFetch header: the host as a badge, then the page it reached (path
/// and query, in the string tone) when there is more than the root. The
/// verb is Ran so the row keeps the call's own globe.
pub(super) fn web_header(url: &str) -> Option<ShellHeader> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let Some(parsed) = url::Url::parse(url).ok().filter(|u| u.host_str().is_some()) else {
        // Not a URL the parser accepts: show it as written.
        return Some(ShellHeader {
            verb: ExecVerb::Ran,
            command: Some(ShellLine::plain(header_text(url), ShellTone::String)),
            badge: None,
            trailer: None,
        });
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_owned();
    let mut page = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        page.push('?');
        page.push_str(query);
    }
    Some(ShellHeader {
        verb: ExecVerb::Ran,
        command: (page != "/" && !page.is_empty())
            .then(|| ShellLine::plain(header_text(&page), ShellTone::String)),
        badge: Some(HeaderBadge::Host(host)),
        trailer: None,
    })
}

/// A WebSearch header: the query in the string tone, like a search pattern.
pub(super) fn query_header(query: &str) -> Option<ShellHeader> {
    if query.trim().is_empty() {
        return None;
    }
    Some(ShellHeader {
        verb: ExecVerb::Ran,
        command: Some(ShellLine::plain(header_text(query), ShellTone::String)),
        badge: None,
        trailer: None,
    })
}

impl ShellHeader {
    /// Height of the header's detail slot: the badge is taller than a line.
    pub(super) fn slot_height(&self, metrics: ToolMetrics) -> f32 {
        if self.badge.is_some() {
            metrics.badge_slot()
        } else {
            metrics.label_line()
        }
    }

    /// The header's detail slot: mono command, optional "in" + badge, and
    /// the trailer, on one ellipsized line. The command is the slot that
    /// shrinks; the trailer always stays readable.
    pub(super) fn element(
        &self,
        failed: bool,
        hover_text: bool,
        metrics: ToolMetrics,
        theme: &Theme,
    ) -> AnyElement {
        let tabular = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let mut row = div()
            .min_w_0()
            .h(px(self.slot_height(metrics)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0));
        let command = self.command.as_ref().map(|command| {
            div()
                .min_w_0()
                .truncate()
                // Code text keeps its absolute size; the row's rem size
                // is for the UI labels around it.
                .text_size(px(TOOL_TEXT_SIZE))
                .child(command.element(theme, failed))
        });
        let badge = self.badge.as_ref().map(|badge| match badge {
            HeaderBadge::Path(path) => {
                let identity = match path {
                    ExecBadge::Folder(path) => {
                        crate::file_icons::FileIconIdentity::directory(path, false)
                    }
                    ExecBadge::Path(path) if path_is_directory(path) => {
                        crate::file_icons::FileIconIdentity::directory(path, false)
                    }
                    ExecBadge::File(path) | ExecBadge::Path(path) => {
                        crate::file_icons::FileIconIdentity::file(path)
                    }
                };
                path_badge(identity, failed, hover_text, None, metrics, theme)
            }
            HeaderBadge::Host(host) => host_badge(host, failed, hover_text, metrics, theme),
        });
        match (&self.badge, command, badge) {
            // A host leads; its page follows with no joiner.
            (Some(HeaderBadge::Host(_)), command, Some(badge)) => {
                row = row.child(badge);
                if let Some(command) = command {
                    row = row.child(command);
                }
            }
            (_, command, badge) => {
                let joined = command.is_some() && badge.is_some();
                if let Some(command) = command {
                    row = row.child(command);
                }
                if joined {
                    row = row.child(
                        div()
                            .flex_none()
                            .text_color(if failed {
                                theme.danger
                            } else {
                                theme.text_muted
                            })
                            .child("in"),
                    );
                }
                if let Some(badge) = badge {
                    row = row.child(badge);
                }
            }
        }
        match self.trailer {
            Some(ShellTrailer::More(n)) => {
                row = row.child(
                    div()
                        .flex_none()
                        .h(ui_rems(16.0))
                        .px(px(5.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(theme.hairline(0.08))
                        .text_size(ui_rems(12.0))
                        .line_height(ui_rems(16.0))
                        .font_features(tabular)
                        .text_color(theme.text_muted)
                        .child(SharedString::from(format!("+{n}"))),
                );
            }
            Some(ShellTrailer::Lines(n)) => {
                row = row.child(
                    div()
                        .flex_none()
                        .font_features(tabular)
                        .text_color(theme.text_faint)
                        .child(SharedString::from(if n == 1 {
                            "1 line".to_owned()
                        } else {
                            format!("{n} lines")
                        })),
                );
            }
            None => {}
        }
        row.into_any_element()
    }
}
