//! Presentation summary of a shell command, shared by every viewport.
//!
//! Agents wrap the command they mean in plumbing: `cd repo && rg foo src
//! 2>&1 | head -50`. [`summarize`] strips that plumbing from a display copy
//! and names what is left ("Searched foo in src"), so a Run chip reads like
//! the dedicated Read/Search tools do. The raw command is never rewritten:
//! the expanded invocation block still shows it verbatim.
//!
//! Classification is deliberately conservative. Anything the small
//! quote-aware lexer below does not fully understand (subshells, `if`/`for`
//! blocks, unterminated quotes) reads as a plain "Ran" with the command as
//! written.

use std::ops::Range;

/// What a shell command did, as a chip verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecVerb {
    /// Anything not recognized below.
    Ran,
    /// `cat`, `head`, `tail`, `nl`, `less`, `bat`, or `sed -n '…p'` on one file.
    Read,
    /// `rg` or `grep` with a pattern.
    Searched,
    /// `ls`, `tree`, or a read-only `find`.
    Listed,
    /// A test runner: `cargo test`, `pytest`, `npm test`, `go test`, …
    Tested,
    /// A `git` subcommand; the subject starts at the subcommand.
    Git,
    /// A heredoc or inline script (`python - <<EOF`, `node -e '…'`).
    RanScript,
}

impl ExecVerb {
    /// The chip label: present participle while the call runs, past tense
    /// once it resolves (the same rule as every other tool label).
    pub fn label(self, running: bool) -> &'static str {
        let (live, done) = match self {
            Self::Ran => ("Running", "Ran"),
            Self::Read => ("Reading", "Read"),
            Self::Searched => ("Searching", "Searched"),
            Self::Listed => ("Listing", "Listed"),
            Self::Tested => ("Testing", "Tested"),
            Self::Git => ("Running git", "Ran git"),
            Self::RanScript => ("Running script", "Ran script"),
        };
        if running { live } else { done }
    }
}

/// The file or folder a classified command acts on, for a path badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecBadge {
    /// The one file a Read command prints, or the file a heredoc writes.
    File(String),
    /// The folder a Listed command walks ("." when none is given).
    Folder(String),
    /// The path a search is scoped to; it may be a file or a folder.
    Path(String),
}

/// The presentation summary of one `ToolCall::Exec` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecSummary {
    pub verb: ExecVerb,
    /// What the verb acts on: the search pattern, the file read, the test
    /// target, the git subcommand and its arguments, a script's first
    /// meaningful line, or (for [`ExecVerb::Ran`]) the first command of
    /// [`Self::display`]. Quotes are kept as written except where the
    /// subject is a single argument (pattern, path, target).
    pub subject: String,
    pub badge: Option<ExecBadge>,
    /// The command with its plumbing stripped: a leading `cd <dir> &&`,
    /// leading `NAME=value` assignments, and a trailing `2>&1`,
    /// `| head -n N` / `| tail -n N` or `; echo $?`. A verbatim slice of the
    /// raw command, so a syntax highlighter can tokenize it directly.
    pub display: String,
    /// How many further commands follow the first in [`Self::display`]
    /// (joined by `&&`, `||`, `;`, `|`, `&` or a newline), for a "+N" pill.
    /// Zero for a single command.
    pub extra_segments: usize,
    /// Line count of the script body, for [`ExecVerb::RanScript`].
    pub script_lines: Option<usize>,
}

/// Summarizes `command` for display. Pure and cheap enough to run per frame.
pub fn summarize(command: &str) -> ExecSummary {
    let trimmed = command.trim();
    let opaque = || ExecSummary {
        verb: ExecVerb::Ran,
        subject: trimmed.to_owned(),
        badge: None,
        display: trimmed.to_owned(),
        extra_segments: 0,
        script_lines: None,
    };
    let Some(mut list) = parse(command) else {
        return opaque();
    };
    if list.cmds.is_empty() {
        return opaque();
    }
    strip_noise(command, &mut list);
    let first = &list.cmds[0];
    let last = &list.cmds[list.cmds.len() - 1];
    let mut end = last.end();
    if let Some(amp) = list.trailing_amp {
        end = end.max(amp);
    }
    let display = command[first.start()..end].to_owned();
    if let [cmd] = list.cmds.as_slice()
        && let Some(summary) = classify(command, cmd, &display)
    {
        return summary;
    }
    ExecSummary {
        verb: ExecVerb::Ran,
        subject: command[first.start()..first.end()].to_owned(),
        badge: None,
        extra_segments: list.cmds.len() - 1,
        display,
        script_lines: None,
    }
}

// ---------------------------------------------------------------------------
// Lexing and structure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    And,
    Or,
    Semi,
    Newline,
    Amp,
    Pipe,
}

#[derive(Debug, Clone)]
struct Heredoc {
    span: Range<usize>,
    body: Range<usize>,
}

/// One simple command: its words (raw spans) and an attached heredoc body.
#[derive(Debug, Clone, Default)]
struct Cmd {
    words: Vec<Range<usize>>,
    heredoc: Option<Heredoc>,
}

impl Cmd {
    fn start(&self) -> usize {
        self.words[0].start
    }

    fn end(&self) -> usize {
        let words = self.words[self.words.len() - 1].end;
        self.heredoc
            .as_ref()
            .map_or(words, |h| h.span.end.max(words))
    }
}

/// A flat command list: `ops[i]` joins `cmds[i]` and `cmds[i + 1]`.
#[derive(Debug, Default)]
struct List {
    cmds: Vec<Cmd>,
    ops: Vec<Op>,
    /// End of a trailing background `&`, kept in the display.
    trailing_amp: Option<usize>,
}

enum Token {
    Word(Range<usize>),
    Op(Op, Range<usize>),
    Heredoc(Heredoc),
}

/// Words that open or close compound shell syntax. A command list holding
/// one of them is left to read as written.
const RESERVED: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "select", "function", "{", "}", "[[", "]]",
];

fn parse(src: &str) -> Option<List> {
    let mut list = List::default();
    let mut current = Cmd::default();
    let mut pending_op: Option<Op> = None;
    for token in lex(src)? {
        match token {
            Token::Word(span) => {
                if current.words.is_empty() {
                    if RESERVED.contains(&&src[span.clone()]) {
                        return None;
                    }
                    if let Some(op) = pending_op.take() {
                        list.ops.push(op);
                    }
                    list.trailing_amp = None;
                }
                current.words.push(span);
            }
            Token::Heredoc(heredoc) => {
                if current.words.is_empty() {
                    return None;
                }
                current.heredoc = Some(heredoc);
            }
            Token::Op(op, span) => {
                if current.words.is_empty() {
                    let after_joiner = matches!(pending_op, Some(Op::And | Op::Or | Op::Pipe));
                    match op {
                        // Blank lines carry nothing, and a newline after
                        // `&&` or `|` continues the same list.
                        Op::Newline => continue,
                        Op::Semi if !after_joiner => continue,
                        _ => return None,
                    }
                }
                list.cmds.push(std::mem::take(&mut current));
                if op == Op::Amp {
                    list.trailing_amp = Some(span.end);
                }
                pending_op = Some(op);
            }
        }
    }
    if !current.words.is_empty() {
        list.cmds.push(current);
    } else if matches!(pending_op, Some(Op::And | Op::Or | Op::Pipe)) {
        return None;
    }
    Some(list)
}

fn lex(src: &str) -> Option<Vec<Token>> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    // Delimiters (and `<<-` tab stripping) of heredocs whose bodies start
    // after the current line.
    let mut pending: Vec<(String, bool)> = Vec::new();
    let mut delimiter_next = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\\' if bytes.get(i + 1) == Some(&b'\n') => i += 2,
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'\n' => {
                let newline = i..i + 1;
                i += 1;
                for (delimiter, strip_tabs) in pending.drain(..) {
                    let start = i;
                    let mut body_end = bytes.len();
                    let mut end = bytes.len();
                    while i < bytes.len() {
                        let line_end = src[i..].find('\n').map_or(bytes.len(), |n| i + n);
                        let line = src[i..line_end].trim_end_matches('\r');
                        let line = if strip_tabs {
                            line.trim_start_matches('\t')
                        } else {
                            line
                        };
                        if line == delimiter {
                            body_end = i;
                            end = line_end;
                            i = line_end;
                            break;
                        }
                        i = (line_end + 1).min(bytes.len());
                    }
                    tokens.push(Token::Heredoc(Heredoc {
                        span: start..end,
                        body: start..body_end.max(start),
                    }));
                }
                tokens.push(Token::Op(Op::Newline, newline));
            }
            b'&' if bytes.get(i + 1) == Some(&b'&') => {
                tokens.push(Token::Op(Op::And, i..i + 2));
                i += 2;
            }
            b'|' if bytes.get(i + 1) == Some(&b'|') => {
                tokens.push(Token::Op(Op::Or, i..i + 2));
                i += 2;
            }
            b'|' => {
                let len = if bytes.get(i + 1) == Some(&b'&') {
                    2
                } else {
                    1
                };
                tokens.push(Token::Op(Op::Pipe, i..i + len));
                i += len;
            }
            b';' => {
                tokens.push(Token::Op(Op::Semi, i..i + 1));
                i += 1;
            }
            b'&' if bytes.get(i + 1) != Some(&b'>') => {
                tokens.push(Token::Op(Op::Amp, i..i + 1));
                i += 1;
            }
            // Subshells and grouping: leave the whole command as written.
            b'(' | b')' => return None,
            _ => {
                let start = i;
                i = word_end(bytes, i)?;
                let word = &src[start..i];
                if delimiter_next {
                    delimiter_next = false;
                    pending.last_mut()?.0 = unquote(word);
                } else if let Some(rest) = word.strip_prefix("<<")
                    && !rest.starts_with('<')
                {
                    let (strip_tabs, rest) = match rest.strip_prefix('-') {
                        Some(rest) => (true, rest),
                        None => (false, rest),
                    };
                    pending.push((unquote(rest), strip_tabs));
                    delimiter_next = rest.is_empty();
                }
                tokens.push(Token::Word(start..i));
            }
        }
    }
    Some(tokens)
}

/// End of the word starting at `i`, honoring quotes, escapes, `$(…)`,
/// backticks and redirections (`2>&1`, `&>file`, `<(…)` stay one word).
/// `None` on an unterminated quote or substitution.
fn word_end(bytes: &[u8], mut i: usize) -> Option<usize> {
    let mut depth = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' => i += 1 + bytes[i + 1..].iter().position(|&b| b == b'\'')? + 1,
            b'"' => {
                i += 1;
                loop {
                    match *bytes.get(i)? {
                        b'\\' => i += 2,
                        b'"' => break,
                        _ => i += 1,
                    }
                }
                i += 1;
            }
            b'`' => i += 1 + bytes[i + 1..].iter().position(|&b| b == b'`')? + 1,
            b'\\' => i += 2,
            b'(' if i > 0 && matches!(bytes[i - 1], b'$' | b'<' | b'>') => {
                depth += 1;
                i += 1;
            }
            b'(' if depth > 0 => {
                depth += 1;
                i += 1;
            }
            b')' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            b'<' | b'>' => {
                i += 1;
                if bytes.get(i) == Some(&b'&') {
                    i += 1;
                }
            }
            b'&' if bytes.get(i + 1) == Some(&b'>') => i += 1,
            _ if depth > 0 => i += 1,
            b' ' | b'\t' | b'\r' | b'\n' | b'|' | b'&' | b';' | b'(' | b')' => break,
            _ => i += 1,
        }
    }
    (depth == 0).then_some(i.min(bytes.len()))
}

/// The shell value of one word: quotes removed, backslash escapes resolved.
/// Expansions (`$VAR`, `$(…)`) stay literal.
fn unquote(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars();
    let mut double = false;
    while let Some(c) = chars.next() {
        match c {
            '\'' if !double => out.extend(chars.by_ref().take_while(|&c| c != '\'')),
            '"' => double = !double,
            '\\' => {
                if let Some(next) = chars.next() {
                    if double && !matches!(next, '"' | '\\' | '$' | '`') {
                        out.push('\\');
                    }
                    out.push(next);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Noise stripping
// ---------------------------------------------------------------------------

fn strip_noise(src: &str, list: &mut List) {
    let word = |cmd: &Cmd, ix: usize| unquote(&src[cmd.words[ix].clone()]);
    // Leading `cd <dir> &&` (or `;`): the agent re-entering its own cwd.
    while list.cmds.len() > 1
        && word(&list.cmds[0], 0) == "cd"
        && list.cmds[0].words.len() <= 2
        && matches!(list.ops[0], Op::And | Op::Semi | Op::Newline)
    {
        list.cmds.remove(0);
        list.ops.remove(0);
    }
    loop {
        let n = list.cmds.len();
        let last = &list.cmds[n - 1];
        let words: Vec<String> = (0..last.words.len()).map(|ix| word(last, ix)).collect();
        let joined_by = n.checked_sub(2).map(|ix| list.ops[ix]);
        let line_limit = joined_by == Some(Op::Pipe) && is_line_limit(&words);
        let exit_echo =
            matches!(joined_by, Some(Op::Semi | Op::Newline)) && words == ["echo", "$?"];
        if line_limit || exit_echo {
            list.cmds.pop();
            list.ops.pop();
        } else if last.heredoc.is_none() && words.len() > 1 && words[words.len() - 1] == "2>&1" {
            list.cmds[n - 1].words.pop();
        } else {
            break;
        }
    }
    // Leading `NAME=value` assignments before the first program.
    let first = &mut list.cmds[0];
    let assignments = first
        .words
        .iter()
        .take_while(|span| is_assignment(&src[(*span).clone()]))
        .count();
    if assignments < first.words.len() {
        first.words.drain(..assignments);
    }
}

/// `head`/`tail` that only caps the line count of a pipe.
fn is_line_limit(words: &[String]) -> bool {
    let count = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !matches!(words[0].as_str(), "head" | "tail") {
        return false;
    }
    match &words[1..] {
        [flag] => flag
            .strip_prefix('-')
            .is_some_and(|n| count(n) || n.strip_prefix('n').is_some_and(count)),
        [flag, n] => flag == "-n" && count(n),
        _ => false,
    }
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Parsed arguments of one command: flags with their values, then operands.
struct Args {
    /// `(flag, value)` for every flag, the value present only for flags in
    /// the program's value table.
    flags: Vec<(String, Option<String>)>,
    operands: Vec<String>,
}

impl Args {
    fn value_of(&self, names: &[&str]) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(flag, _)| names.contains(&flag.as_str()))
            .filter_map(|(_, value)| value.as_deref())
            .collect()
    }

    fn has(&self, names: &[&str]) -> bool {
        self.flags
            .iter()
            .any(|(flag, _)| names.contains(&flag.as_str()))
    }
}

/// Splits `words` into flags and operands. Short flags may cluster (`-rn`);
/// a flag named in `takes_value` (space-separated) consumes the rest of its
/// cluster or the next word.
/// `None` on a redirection, which a classified command must not carry.
fn args(words: &[String], takes_value: &str) -> Option<Args> {
    let takes_value = |flag: &str| takes_value.split_whitespace().any(|f| f == flag);
    let mut out = Args {
        flags: Vec::new(),
        operands: Vec::new(),
    };
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if word.contains('>') || word.starts_with('<') {
            return None;
        }
        if word == "--" {
            out.operands.extend(words.by_ref().cloned());
        } else if let Some(long) = word.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (format!("--{name}"), Some(value.to_owned())),
                None if takes_value(word) => (word.clone(), Some(words.next()?.clone())),
                None => (word.clone(), None),
            };
            out.flags.push((name, value));
        } else if let Some(cluster) = word.strip_prefix('-')
            && !cluster.is_empty()
        {
            for (ix, c) in cluster.char_indices() {
                let flag = format!("-{c}");
                if takes_value(&flag) {
                    let rest = &cluster[ix + c.len_utf8()..];
                    let value = if rest.is_empty() {
                        words.next()?.clone()
                    } else {
                        rest.to_owned()
                    };
                    out.flags.push((flag, Some(value)));
                    break;
                }
                out.flags.push((flag, None));
            }
        } else {
            out.operands.push(word.clone());
        }
    }
    Some(out)
}

fn classify(src: &str, cmd: &Cmd, display: &str) -> Option<ExecSummary> {
    let words: Vec<String> = cmd
        .words
        .iter()
        .map(|span| unquote(&src[span.clone()]))
        .collect();
    let program = words[0].rsplit('/').next().unwrap_or_default();
    let rest = &words[1..];
    let summary = |verb, subject: String, badge| ExecSummary {
        verb,
        subject,
        badge,
        display: display.to_owned(),
        extra_segments: 0,
        script_lines: None,
    };
    // The raw text from word `ix` to the end, quotes as written.
    let tail_from = |ix: usize| src[cmd.words[ix].start..cmd.end()].trim().to_owned();
    // A test run whose target is everything from word `ix`, if any.
    let tested = |ix: usize| {
        let target = cmd.words.get(ix).map(|_| tail_from(ix));
        Some(test_summary(display, target))
    };

    if let Some(heredoc) = &cmd.heredoc {
        let target = words
            .windows(2)
            .find(|pair| matches!(pair[0].as_str(), ">" | ">>"))
            .map(|pair| pair[1].clone())
            .or_else(|| {
                words.iter().find_map(|w| {
                    let path = w.strip_prefix(">>").or_else(|| w.strip_prefix('>'))?;
                    (!path.is_empty() && !path.starts_with('&')).then(|| path.to_owned())
                })
            });
        return Some(script(
            &src[heredoc.body.clone()],
            display,
            target.map(ExecBadge::File),
        ));
    }

    if program.starts_with("python")
        && rest.first().is_some_and(|w| w == "-m")
        && rest.get(1).is_some_and(|w| w == "pytest")
    {
        return tested(3);
    }
    let inline_flag: &[&str] = match program {
        p if p.starts_with("python") => &["-c"],
        "node" | "deno" => &["-e", "--eval", "-p", "--print"],
        "ruby" | "perl" => &["-e", "-E"],
        "bash" | "sh" | "zsh" => &["-c"],
        _ => &[],
    };
    if let Some(ix) = rest.iter().position(|w| inline_flag.contains(&w.as_str()))
        && rest[..ix].iter().all(|w| w.starts_with('-'))
    {
        return Some(script(rest.get(ix + 1)?, display, None));
    }

    match program {
        "cat" | "nl" | "less" | "more" | "bat" | "head" | "tail" | "sed" => {
            let parsed = args(
                rest,
                match program {
                    "head" | "tail" => "-n -c --lines --bytes",
                    "bat" => "-r --line-range -l --language -H --highlight-line --style --theme",
                    "sed" => "-e --expression",
                    _ => "",
                },
            )?;
            if program == "tail" && parsed.has(&["-f", "-F", "--follow"]) {
                return None;
            }
            let mut files = parsed.operands.as_slice();
            if program == "sed" {
                if !parsed.has(&["-n", "--quiet", "--silent"]) || parsed.has(&["-i", "--in-place"])
                {
                    return None;
                }
                let scripts = parsed.value_of(&["-e", "--expression"]);
                let script = match scripts.as_slice() {
                    [] => {
                        let (script, rest) = files.split_first()?;
                        files = rest;
                        script.as_str()
                    }
                    [script] => *script,
                    _ => return None,
                };
                let prints_only = script.ends_with('p')
                    && script
                        .bytes()
                        .all(|b| b.is_ascii_digit() || matches!(b, b',' | b'$' | b'p' | b';'));
                if !prints_only {
                    return None;
                }
            }
            let [file] = files else { return None };
            if file == "-" {
                return None;
            }
            Some(summary(
                ExecVerb::Read,
                file.clone(),
                Some(ExecBadge::File(file.clone())),
            ))
        }
        "rg" | "grep" | "egrep" | "fgrep" => {
            let parsed = args(
                rest,
                if program == "rg" {
                    "-e --regexp -f --file -g --glob --iglob -t --type -T --type-not -A \
                     --after-context -B --before-context -C --context -m --max-count -j \
                     --threads -M --max-columns --max-depth -E --encoding -r --replace --sort \
                     --sortr --type-add --color"
                } else {
                    "-e --regexp -f --file -A --after-context -B --before-context -C --context \
                     -m --max-count -d --directories -D --devices --include --exclude \
                     --exclude-dir --label"
                },
            )?;
            if parsed.has(&["-f", "--file"]) {
                return None;
            }
            let explicit = parsed.value_of(&["-e", "--regexp"]);
            let (pattern, paths) = match explicit.first() {
                Some(pattern) => (pattern.to_string(), parsed.operands.as_slice()),
                None => {
                    let (pattern, paths) = parsed.operands.split_first()?;
                    (pattern.clone(), paths)
                }
            };
            let badge = match paths {
                [path] => Some(ExecBadge::Path(path.clone())),
                _ => None,
            };
            Some(summary(ExecVerb::Searched, pattern, badge))
        }
        "ls" | "tree" => {
            let parsed = args(
                rest,
                if program == "ls" {
                    "-I --ignore -w --width -T --tabsize"
                } else {
                    "-L -P -I -o -H -T --filelimit --charset"
                },
            )?;
            let folder = match parsed.operands.as_slice() {
                [] => ".".to_owned(),
                [folder] => folder.clone(),
                _ => return None,
            };
            Some(summary(
                ExecVerb::Listed,
                folder.clone(),
                Some(ExecBadge::Folder(folder)),
            ))
        }
        "find" => {
            let roots = rest
                .iter()
                .take_while(|w| !w.starts_with('-') && !matches!(w.as_str(), "(" | "!" | ")"))
                .count();
            let (roots, expression) = rest.split_at(roots);
            let mutates = expression.iter().any(|w| {
                matches!(
                    w.as_str(),
                    "-exec" | "-execdir" | "-ok" | "-okdir" | "-delete" | "-fprint" | "-fls"
                ) || w.contains('>')
            });
            if mutates {
                return None;
            }
            let folder = match roots {
                [] => ".".to_owned(),
                [folder] => folder.clone(),
                _ => return None,
            };
            let name = expression
                .windows(2)
                .find(|pair| matches!(pair[0].as_str(), "-name" | "-iname" | "-path" | "-ipath"))
                .map(|pair| pair[1].clone());
            Some(summary(
                ExecVerb::Listed,
                name.unwrap_or_else(|| folder.clone()),
                Some(ExecBadge::Folder(folder)),
            ))
        }
        "cargo"
            if rest.first().is_some_and(|w| w == "test")
                || rest.first().is_some_and(|w| w == "nextest")
                    && rest.get(1).is_some_and(|w| w == "run") =>
        {
            let start = if rest[0] == "test" { 2 } else { 3 };
            let parsed = args(&words[start.min(words.len())..], "-p --package");
            let packages = parsed
                .as_ref()
                .map(|parsed| parsed.value_of(&["-p", "--package"]).join(" "))
                .unwrap_or_default();
            if !packages.is_empty() {
                return Some(test_summary(display, Some(packages)));
            }
            tested(start)
        }
        "pytest" | "py.test" => tested(1),
        "go" if rest.first().is_some_and(|w| w == "test") => tested(2),
        "npm" | "pnpm" | "bun" | "yarn" => {
            let start = match rest {
                [test, ..] if test == "test" => 2,
                [run, test, ..] if run == "run" && test == "test" => 3,
                _ => return None,
            };
            tested(start)
        }
        "git" => {
            let mut ix = 1;
            while let Some(word) = words.get(ix)
                && word.starts_with('-')
            {
                ix += if matches!(word.as_str(), "-C" | "-c") {
                    2
                } else {
                    1
                };
            }
            cmd.words.get(ix)?;
            Some(summary(ExecVerb::Git, tail_from(ix), None))
        }
        _ => None,
    }
}

fn test_summary(display: &str, target: Option<String>) -> ExecSummary {
    ExecSummary {
        verb: ExecVerb::Tested,
        subject: target.unwrap_or_else(|| display.to_owned()),
        badge: None,
        display: display.to_owned(),
        extra_segments: 0,
        script_lines: None,
    }
}

/// An inline or heredoc script: its first line that is neither blank nor a
/// comment names it.
fn script(body: &str, display: &str, badge: Option<ExecBadge>) -> ExecSummary {
    let subject = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or(display)
        .to_owned();
    ExecSummary {
        verb: ExecVerb::RanScript,
        subject,
        badge,
        display: display.to_owned(),
        extra_segments: 0,
        script_lines: Some(body.lines().count()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_strip_their_plumbing_and_name_what_they_do() {
        use ExecBadge::{File, Folder, Path};
        use ExecVerb::*;
        let path = |p: &str| Some(Path(p.into()));
        // (command, verb, subject, badge, display, extra segments, script lines)
        type Case = (
            &'static str,
            ExecVerb,
            &'static str,
            Option<ExecBadge>,
            &'static str,
            usize,
            Option<usize>,
        );
        #[rustfmt::skip]
        let cases: &[Case] = &[
            ("make", Ran, "make", None, "make", 0, None),
            ("cd /repo && make build", Ran, "make build", None, "make build", 0, None),
            ("make 2>&1", Ran, "make", None, "make", 0, None),
            ("cd crates/ui && rg -n 'fn render' src 2>&1 | head -n 40", Searched, "fn render", path("src"), "rg -n 'fn render' src", 0, None),
            ("cargo build 2>&1 | tail -20", Ran, "cargo build", None, "cargo build", 0, None),
            ("make check; echo $?", Ran, "make check", None, "make check", 0, None),
            ("RUST_LOG=debug FOO=1 cargo run", Ran, "cargo run", None, "cargo run", 0, None),
            ("cat src/main.rs", Read, "src/main.rs", Some(File("src/main.rs".into())), "cat src/main.rs", 0, None),
            ("sed -n '1,40p' crates/proto/src/view.rs", Read, "crates/proto/src/view.rs", Some(File("crates/proto/src/view.rs".into())), "sed -n '1,40p' crates/proto/src/view.rs", 0, None),
            ("sed -i 's/a/b/' file.rs", Ran, "sed -i 's/a/b/' file.rs", None, "sed -i 's/a/b/' file.rs", 0, None),
            ("grep -rn \"TODO\" .", Searched, "TODO", path("."), "grep -rn \"TODO\" .", 0, None),
            ("rg -A 3 -e needle", Searched, "needle", None, "rg -A 3 -e needle", 0, None),
            ("ls -la crates", Listed, "crates", Some(Folder("crates".into())), "ls -la crates", 0, None),
            ("find . -name '*.rs' -type f", Listed, "*.rs", Some(Folder(".".into())), "find . -name '*.rs' -type f", 0, None),
            ("find . -name '*.tmp' -delete", Ran, "find . -name '*.tmp' -delete", None, "find . -name '*.tmp' -delete", 0, None),
            ("cargo test -p zeron-proto -- --nocapture", Tested, "zeron-proto", None, "cargo test -p zeron-proto -- --nocapture", 0, None),
            ("pytest tests/test_x.py -q", Tested, "tests/test_x.py -q", None, "pytest tests/test_x.py -q", 0, None),
            ("npm test", Tested, "npm test", None, "npm test", 0, None),
            ("go test ./...", Tested, "./...", None, "go test ./...", 0, None),
            ("git status --short", Git, "status --short", None, "git status --short", 0, None),
            ("git -C repo commit -m 'a && b | c'", Git, "commit -m 'a && b | c'", None, "git -C repo commit -m 'a && b | c'", 0, None),
            ("python3 - <<'EOF'\n# probe\nimport sys\nprint(sys.version)\nEOF", RanScript, "import sys", None, "python3 - <<'EOF'\n# probe\nimport sys\nprint(sys.version)\nEOF", 0, Some(3)),
            ("cat <<EOF > notes.md\nhello\nEOF", RanScript, "hello", Some(File("notes.md".into())), "cat <<EOF > notes.md\nhello\nEOF", 0, Some(1)),
            ("node -e 'console.log(1)'", RanScript, "console.log(1)", None, "node -e 'console.log(1)'", 0, Some(1)),
            ("cd x && make && make test && ./run", Ran, "make", None, "make && make test && ./run", 2, None),
            ("echo 'a | b && c' | wc -l", Ran, "echo 'a | b && c'", None, "echo 'a | b && c' | wc -l", 1, None),
            ("for f in *.rs; do wc -l $f; done", Ran, "for f in *.rs; do wc -l $f; done", None, "for f in *.rs; do wc -l $f; done", 0, None),
            ("echo \"unterminated", Ran, "echo \"unterminated", None, "echo \"unterminated", 0, None),
        ];
        for (command, verb, subject, badge, display, extra_segments, script_lines) in cases {
            let summary = summarize(command);
            assert_eq!(
                summary,
                ExecSummary {
                    verb: *verb,
                    subject: subject.to_string(),
                    badge: badge.clone(),
                    display: display.to_string(),
                    extra_segments: *extra_segments,
                    script_lines: *script_lines,
                },
                "{command}"
            );
        }
    }
}
