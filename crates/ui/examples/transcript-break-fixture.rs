//! Throwaway stress page for the transcript (break skill). Every scenario is
//! a real `Transcript` (read-only doc instance) fed fixture entries, rendered
//! in three containers: 320 px, squeezed by a flex sibling, and full width.
//! Sections are tabs. Isolated engine and data in a temp dir; no agent runs.
//!
//! `--capture <dir>` writes one PNG per page of every section and quits;
//! `--light` flips appearance.
use gpui::{
    AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use serde_json::{Value, json};
use std::sync::Arc;
use zeron_proto::ToolCall;
use zeron_ui::transcript::Transcript;
use zeron_ui::*;

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

const T0: i64 = 1788900000000;

fn text(id: &str, text: &str) -> Value {
    json!({"id": id, "kind": "text", "text": text})
}
fn tool(id: &str, call: ToolCall, output: &str, is_error: bool) -> Value {
    json!({"id": id, "kind": "tool", "call": call, "isError": is_error, "resolved": true, "output": output})
}
fn running(id: &str, call: ToolCall) -> Value {
    json!({"id": id, "kind": "tool", "call": call, "isError": false, "resolved": false})
}
fn edit(id: &str, path: &str, additions: u64, deletions: u64) -> Value {
    json!({"id": id, "kind": "tool", "isError": false, "resolved": true,
        "call": ToolCall::EditFile { path: path.into(), old_string: None, new_string: None },
        "diffStats": [{"path": path, "additions": additions, "deletions": deletions}]})
}
fn exec(command: &str) -> ToolCall {
    ToolCall::Exec {
        command: command.into(),
    }
}
fn unknown(name: &str, input: Option<Value>) -> ToolCall {
    ToolCall::Unknown {
        name: name.into(),
        input,
    }
}
fn spawn(id: &str, title: &str, model: Option<&str>, status: &str) -> Value {
    let input = model.map(|m| json!({"model": m}));
    let name = if title.is_empty() {
        "Agent".to_string()
    } else {
        format!("Agent: {title}")
    };
    json!({"id": id, "kind": "tool", "isError": status == "failed", "resolved": status != "running",
        "call": unknown(&name, input), "subagentRef": format!("sub-{id}"), "subagentStatus": status})
}
fn question(header: &str, question: &str) -> Value {
    json!({"id": format!("q-{header}"), "header": header, "question": question,
        "options": ["Yes", "No"], "multiSelect": false, "multiline": false})
}
fn input(id: &str, resolved: bool, questions: Vec<Value>) -> Value {
    json!({"id": id, "kind": "input", "requestId": format!("req-{id}"), "resolved": resolved, "questions": questions})
}
fn error(id: &str, message: &str) -> Value {
    json!({"id": id, "kind": "error", "message": message})
}
fn user(id: &str, body: &str) -> Value {
    json!({"id": id, "role": "user", "createdAt": T0, "parts": [text(&format!("{id}t"), body)]})
}
fn reply(id: &str, status: &str, parts: Vec<Value>) -> Value {
    json!({"id": id, "role": "assistant", "createdAt": T0 + 1000, "status": status, "parts": parts})
}
/// A turn still streaming "now", for running spinners and live clocks.
fn live(id: &str, parts: Vec<Value>) -> Value {
    let started = chrono::Utc::now().timestamp_millis() - 14_000;
    json!({"id": id, "role": "assistant", "createdAt": started, "status": "streaming", "parts": parts})
}
fn done(parts: Vec<Value>) -> Vec<Value> {
    vec![reply("a", "complete", parts)]
}

const UNBROKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG5Eb2UiLCJpYXQiOjE1MTYyMzkwMjJ9SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c";
const LONG_PATH: &str = "crates/ui/src/really/deeply/nested/module/hierarchy/that/keeps/going/and/going/transcript_row_rendering_with_an_extremely_long_file_name.rs";
const LONG_URL: &str = "https://docs.rs/gpui/latest/gpui/struct.ListState.html?search=virtualization&filter=struct&page=4&utm_source=transcript&utm_medium=agent&utm_campaign=stress";
const SENTENCES: &str = "The build failed because the linker could not find the system library it needed. It looked in every configured search path and tried both the static and dynamic variants. Install the development headers for the library, then run the build again from a clean target directory.";

/// What visibly broke in the look, keyed by scenario label.
const NOTES: &[(&str, &str)] = &[
    ("Typical: one row per call kind, one failed", "at 320 the group summary clips to 'Ran 1 c' with no ellipsis; path badges lose their names (Edited shows only the icon, Fetched 'do… /g…')"),
    ("Empty strings: command, path, pattern, MCP names, tool name", "the MCP row reads 'Called ·' and the search row 'Searched in', with nothing around them"),
    ("Unbreakable strings: token, path, URL, regex, MCP tool", "shell, regex and URL text clip mid-glyph with no ellipsis; badges shrink to a sliver of icon; full width elides the URL host to '…'"),
    ("Several lines: heredoc script, long flag list", "squeezed: the script row shows a stray ':' fragment where the cut lands"),
    ("Loading: calls still running under a live turn", "at 320 the shell detail clips mid-glyph ('--sh') instead of ending in an ellipsis"),
    ("Change counts: zero, huge, add-only, remove-only", "at 320 and squeezed the file badge is dropped or reduced to its icon, while the counts keep their room"),
    ("Details open: long output, inline diff, failed output", "at 320 the search path badge shows only '…' and the edit badge only 'm…'"),
    ("Zero entries", "blank region, no empty state"),
    ("Realistic group, collapsed: every verb, a failure, change totals", "at 320 the summary clips to 'Ran 1 c' with no ellipsis"),
    ("Ten times: 60 calls, collapsed", "at 320 the summary vanishes; the header is only '+1350 −450' and '3 failed'"),
    ("Ten times: 60 calls, open", "same header at 320; Edited rows drop the file name for the counts"),
    ("Every call failed", "at 320 every command clips mid-character ('-p (')"),
    ("Content length: no title, one word, sentences, unbreakable, long model", "a long model name pushes the title and the open arrow out of the row at 320 and squeezed, and itself clips without ellipsis"),
    ("Wait for agents, alone and after spawns", "the group header and its only row both read 'Waited for agents'"),
    ("Resolved and waiting", "waiting rows drop the question: at 320 only 'Rebase' is left, squeezed shows a lone '…'"),
    ("Empty header and question", "only the icon and 'Waiting for your answer' remain"),
    ("Several sentences, unbreakable, long header", "a long header pushes the question out and clips without ellipsis; 'Waiting for your answer' clips at 320"),
    ("Twelve questions in one request", "at 320 the question text is gone and 'Waiting for your a' clips"),
    ("Content length: empty, one word, sentences, unbreakable, multi-line trace", "the empty message is an empty red chip; the multi-line panic trace collapses onto one line; 'https://' breaks between its slashes"),
    ("Interrupted: after work, and with nothing done", "the second interrupted turn, which has no parts, shows no Interrupted marker"),
    ("User bubble: one word, typical, sentences, unbreakable", "the URL breaks between the two slashes of 'https://'"),
    ("User bubble: empty, and a long prompt that folds", "the empty prompt leaves a blank gap with no bubble"),
    ("Markdown: headings, lists, code, unbreakable URL", "the link breaks between the two slashes of 'https://'"),
    ("Markdown tables: narrow, numeric, wide", "at 320 the wide table shows two of six columns with no sign the rest exist"),
    ("Thought process and fork marker: long and empty titles", "an empty source title leaves a blank line under 'Forked from'; the Thought process header and its row repeat the same words"),
    ("RTL and mixed direction", "wrapped RTL lines stack in reverse order (the sentence's end on the first line); Arabic words split mid-word; RTL paragraphs sit left-aligned; the Arabic shell detail truncates from the wrong end"),
    ("Numbers that should align", "at 320 the edit rows drop their file names and the header clips 'Edited 2 fil' without ellipsis"),
];

struct Scenario {
    label: &'static str,
    /// What visibly broke, marked after the look.
    note: Option<&'static str>,
    height: f32,
    open: bool,
    /// Also open every row's detail (shell block, diff, output).
    details: bool,
    live: bool,
    entries: Vec<Value>,
}

fn sc(label: &'static str, height: u32, entries: Vec<Value>) -> Scenario {
    Scenario {
        label,
        note: None,
        height: height as f32,
        open: true,
        details: false,
        live: false,
        entries,
    }
}

fn sections() -> Vec<(&'static str, Vec<Scenario>)> {
    let catalogue = vec![
        tool("c1", exec("cargo test -p zeron-ui -- --test-threads=1"), "ok", false),
        tool("c2", ToolCall::ReadFile { path: "README.md".into() }, "", false),
        tool("c3", ToolCall::WriteFile { path: "docs/notes.md".into(), content: None }, "", false),
        edit("c4", "crates/ui/src/lib.rs", 3, 1),
        tool("c5", ToolCall::ApplyPatch { path: None }, "", false),
        tool("c6", ToolCall::Search { pattern: "fn render".into(), path: Some("crates/ui/src".into()) }, "12", false),
        tool("c7", ToolCall::Glob { pattern: "**/*.rs".into() }, "412", false),
        tool("c8", ToolCall::WebFetch { url: "https://docs.rs/gpui".into(), prompt: None }, "", false),
        tool("c9", ToolCall::WebSearch { query: "gpui list virtualization".into() }, "", false),
        tool("c10", ToolCall::Mcp { server: "github".into(), tool: "list_pull_requests".into(), input: None }, "", false),
        tool("c11", unknown("Skill", Some(json!({"skill": "better-ui"}))), "", false),
        tool("c12", unknown("Bash", None), "permission denied", true),
    ];
    let many: Vec<Value> = (0..60)
        .map(|i| match i % 4 {
            0 => tool(&format!("m{i}"), exec(&format!("cargo check -p crate-{i}")), "ok", i % 7 == 0),
            1 => tool(&format!("m{i}"), ToolCall::ReadFile { path: format!("crates/ui/src/file_{i}.rs") }, "", false),
            2 => edit(&format!("m{i}"), &format!("crates/ui/src/file_{i}.rs"), i as u64 * 3, i as u64),
            _ => tool(&format!("m{i}"), ToolCall::Search { pattern: format!("needle_{i}"), path: None }, "", false),
        })
        .collect();

    vec![
        (
            "Tool rows",
            vec![
                sc("Typical: one row per call kind, one failed", 640, done(catalogue.clone())),
                sc(
                    "Empty strings: command, path, pattern, MCP names, tool name",
                    420,
                    done(vec![
                        tool("e1", exec(""), "", false),
                        tool("e2", ToolCall::ReadFile { path: "".into() }, "", false),
                        tool("e3", ToolCall::Search { pattern: "".into(), path: Some("".into()) }, "", false),
                        tool("e4", ToolCall::Mcp { server: "".into(), tool: "".into(), input: None }, "", false),
                        tool("e5", unknown("", None), "", false),
                        tool("e6", ToolCall::WebFetch { url: "".into(), prompt: None }, "", false),
                    ]),
                ),
                sc(
                    "One word each",
                    360,
                    done(vec![
                        tool("w1", exec("ls"), "", false),
                        tool("w2", ToolCall::ReadFile { path: "a".into() }, "", false),
                        tool("w3", ToolCall::Search { pattern: "x".into(), path: None }, "", false),
                        tool("w4", ToolCall::Mcp { server: "gh".into(), tool: "pr".into(), input: None }, "", false),
                    ]),
                ),
                sc(
                    "Unbreakable strings: token, path, URL, regex, MCP tool",
                    460,
                    done(vec![
                        tool("u1", exec(&format!("echo {UNBROKEN}")), "", false),
                        tool("u2", ToolCall::ReadFile { path: LONG_PATH.into() }, "", false),
                        tool("u3", ToolCall::WebFetch { url: LONG_URL.into(), prompt: None }, "", false),
                        tool("u4", ToolCall::Search { pattern: format!("(?i)^{}$", UNBROKEN), path: Some(LONG_PATH.into()) }, "", false),
                        tool("u5", ToolCall::Mcp { server: "codex_apps".into(), tool: format!("cloudflare.{UNBROKEN}"), input: None }, "", false),
                        edit("u6", LONG_PATH, 12, 4),
                    ]),
                ),
                sc(
                    "Several lines: heredoc script, long flag list",
                    300,
                    done(vec![
                        tool("l1", exec("python3 - <<'EOF'\nimport json, sys\nfor line in sys.stdin:\n    print(json.loads(line)['id'])\nEOF"), "", false),
                        tool("l2", exec("npm run build -- --mode=production --outDir=dist/release --sourcemap=hidden --minify=esbuild --target=es2022 --emptyOutDir --reportCompressedSize=false"), "", false),
                    ]),
                ),
                Scenario {
                    live: true,
                    ..sc(
                        "Loading: calls still running under a live turn",
                        380,
                        vec![live(
                            "a",
                            vec![
                                tool("r0", exec("git status --short"), "", false),
                                running("r1", exec("cargo test -p zeron-ui -- --test-threads=1")),
                                running("r2", ToolCall::ReadFile { path: LONG_PATH.into() }),
                            ],
                        )],
                    )
                },
                sc(
                    "Change counts: zero, huge, add-only, remove-only",
                    330,
                    done(vec![
                        edit("d1", "a.rs", 0, 0),
                        edit("d2", "crates/ui/src/transcript.rs", 99_999, 88_888),
                        edit("d3", "b.rs", 1, 0),
                        edit("d4", "c.rs", 0, 1),
                    ]),
                ),
                Scenario { details: true, ..sc(
                    "Details open: long output, inline diff, failed output",
                    640,
                    done(vec![
                        tool("o1", exec("rg -n 'fn ' crates/ui/src | head -n 40"),
                            &(1..=40).map(|i| format!("crates/ui/src/transcript.rs:{}:    fn render_row_{i}(&mut self, cx: &mut Context<Self>) {UNBROKEN}", i * 97)).collect::<Vec<_>>().join("\n"), false),
                        json!({"id": "o2", "kind": "tool", "isError": false, "resolved": true,
                            "call": ToolCall::EditFile { path: "crates/ui/src/motion.rs".into(), old_string: None, new_string: None },
                            "diff": {"path": "crates/ui/src/motion.rs",
                                "oldText": "pub const FADE_QUICK: MotionSpec = MotionSpec::new(150, EASE);\n",
                                "newText": format!("pub const FADE_QUICK: MotionSpec = MotionSpec::new(150, EASE);\npub const {UNBROKEN}: u8 = 0;\n")}}),
                        tool("o3", exec("cargo build"), SENTENCES, true),
                    ]),
                ) },
            ],
        ),
        (
            "Groups & quantity",
            vec![
                sc("Zero entries", 120, vec![]),
                sc("A turn with no parts", 120, done(vec![])),
                Scenario { open: false, ..sc("One call, collapsed", 120, done(vec![tool("g1", exec("ls"), "", false)])) },
                Scenario { open: false, ..sc("Realistic group, collapsed: every verb, a failure, change totals", 140, done(catalogue.clone())) },
                Scenario { open: false, ..sc("Ten times: 60 calls, collapsed", 140, done(many.clone())) },
                sc("Ten times: 60 calls, open", 640, done(many.clone())),
                sc(
                    "Every call failed",
                    320,
                    done((0..6).map(|i| tool(&format!("f{i}"), exec(&format!("cargo build -p crate-{i}")), "error", true)).collect()),
                ),
                sc(
                    "Text between groups, many short groups",
                    420,
                    done((0..5).flat_map(|i| vec![text(&format!("n{i}"), "Checking the next crate."), tool(&format!("s{i}"), exec("cargo check"), "", false)]).collect()),
                ),
            ],
        ),
        (
            "Agents",
            vec![
                Scenario {
                    live: true,
                    ..sc(
                        "Typical fan-out: done, running, failed, with and without a model",
                        240,
                        vec![live(
                            "a",
                            vec![
                                spawn("s1", "Audit the transcript tests", Some("sonnet"), "done"),
                                spawn("s2", "Explore the markdown crate", None, "running"),
                                spawn("s3", "Port the sidebar", Some("haiku"), "failed"),
                            ],
                        )],
                    )
                },
                sc(
                    "Content length: no title, one word, sentences, unbreakable, long model",
                    360,
                    done(vec![
                        spawn("t1", "", None, "done"),
                        spawn("t2", "scout", None, "done"),
                        spawn("t3", SENTENCES, Some("opus"), "done"),
                        spawn("t4", UNBROKEN, Some("sonnet"), "done"),
                        spawn("t5", "Review", Some("claude-opus-5-5-20261001-extended-context-preview"), "done"),
                    ]),
                ),
                sc(
                    "Ten times: 30 spawns",
                    640,
                    done((0..30).map(|i| spawn(&format!("x{i}"), &format!("Reviewer {i}: check crate {i}"), Some("haiku"), ["done", "failed", "done"][i % 3])).collect()),
                ),
                sc(
                    "Wait for agents, alone and after spawns",
                    260,
                    done(vec![
                        spawn("w1", "Audit", None, "done"),
                        tool("w2", unknown("Wait for agents", None), "", false),
                        text("w3", "All three reviewers are back."),
                    ]),
                ),
            ],
        ),
        (
            "Questions",
            vec![
                sc(
                    "Resolved and waiting",
                    180,
                    done(vec![
                        input("q1", true, vec![question("Scope", "Which crate should the fix land in?")]),
                        input("q2", false, vec![question("Rebase", "Rebase onto main before pushing?")]),
                    ]),
                ),
                sc(
                    "Empty header and question",
                    140,
                    done(vec![input("q3", false, vec![question("", "")])]),
                ),
                sc(
                    "Several sentences, unbreakable, long header",
                    240,
                    done(vec![
                        input("q4", false, vec![question("Left edge", SENTENCES)]),
                        input("q5", true, vec![question("Token", UNBROKEN)]),
                        input("q6", true, vec![question("A header that is itself a whole sentence long", "Keep it?")]),
                    ]),
                ),
                sc(
                    "Twelve questions in one request",
                    140,
                    done(vec![input("q7", false, (0..12).map(|i| question(&format!("Q{i}"), &format!("Question number {i}?"))).collect())]),
                ),
            ],
        ),
        (
            "Errors & connection",
            vec![
                sc(
                    "Content length: empty, one word, sentences, unbreakable, multi-line trace",
                    560,
                    done(vec![
                        error("e1", ""),
                        text("b1", "—"),
                        error("e2", "Timeout"),
                        text("b2", "—"),
                        error("e3", SENTENCES),
                        text("b3", "—"),
                        error("e4", &format!("fetch failed: {LONG_URL}{UNBROKEN}")),
                        text("b4", "—"),
                        error("e5", "thread 'main' panicked at crates/engine/src/run.rs:412:9:\ncalled `Option::unwrap()` on a `None` value\nstack backtrace:\n   0: rust_begin_unwind\n   1: core::panicking::panic_fmt"),
                    ]),
                ),
                sc(
                    "Repeats and JSON bodies",
                    300,
                    done({
                        let mut parts: Vec<Value> = (0..120).map(|i| error(&format!("r{i}"), "io: Too many open files (os error 24)")).collect();
                        parts.push(text("t", "Then an API body:"));
                        parts.push(error("j1", "{\"type\":\"error\",\"status\":529,\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}"));
                        parts.push(error("j2", "{\"error\":{}}"));
                        parts
                    }),
                ),
                Scenario {
                    live: true,
                    ..sc(
                        "Loading: reconnecting now",
                        140,
                        vec![live("a", vec![text("p", "Working."), error("c1", "Reconnecting... 1/5"), error("c2", "Reconnecting... 2/5")])],
                    )
                },
                sc(
                    "Recovered, lost after one retry, lost after five",
                    320,
                    vec![
                        reply("a1", "complete", vec![error("c1", "Reconnecting... 2/5"), error("c2", "Reconnecting... 3/5"), text("p", "Carried on.")]),
                        reply("a2", "complete", vec![error("c3", "Reconnecting... 1/1")]),
                        reply("a3", "complete", (1..=5).map(|i| error(&format!("c4{i}"), &format!("Reconnecting... {i}/5"))).collect()),
                    ],
                ),
                sc(
                    "Interrupted: after work, and with nothing done",
                    300,
                    vec![
                        reply("a1", "aborted", vec![text("p", "Starting."), tool("g", exec("git log"), "", false)]),
                        user("u", "Stop."),
                        reply("a2", "aborted", vec![]),
                    ],
                ),
            ],
        ),
        (
            "Messages & text",
            vec![
                sc(
                    "User bubble: one word, typical, sentences, unbreakable",
                    560,
                    vec![
                        user("u1", "ok"),
                        user("u2", "Tidy the transcript's tool rows and check the suite still passes."),
                        user("u3", SENTENCES),
                        user("u4", &format!("{LONG_URL}{UNBROKEN}")),
                    ],
                ),
                sc(
                    "User bubble: empty, and a long prompt that folds",
                    420,
                    vec![
                        user("u1", ""),
                        user("u2", &(1..=14).map(|i| format!("{i}. Check that line {i} still wraps inside the bubble.")).collect::<Vec<_>>().join("\n")),
                    ],
                ),
                sc(
                    "Markdown: headings, lists, code, unbreakable URL",
                    620,
                    done(vec![text("m", &format!("## A heading that runs long enough to wrap at narrow widths\n\n- one\n- a list item with `inline_code_{UNBROKEN}`\n  - nested\n\n```rust\nfn main() {{ println!(\"{UNBROKEN}\"); }}\n```\n\nSee {LONG_URL}."))]),
                ),
                sc(
                    "Markdown tables: narrow, numeric, wide",
                    560,
                    done(vec![text("t", "| Crate | Files | Added |\n| :--- | ---: | ---: |\n| zeron-ui | 4 | 1,284 |\n| zeron-proto | 11 | 96 |\n\n| Command | Output | Exit | Duration | Host | Notes |\n| --- | --- | --- | --- | --- | --- |\n| `cargo test -p zeron-ui --lib -- --test-threads=1` | 1678 passed | 0 | 4m 12s | macbook | serial on macOS |")]),
                ),
                Scenario { details: true, ..sc(
                    "Thought process and fork marker: long and empty titles",
                    420,
                    vec![reply(
                        "a",
                        "complete",
                        vec![
                            json!({"id": "f1", "kind": "fork", "sourceChatId": "c1", "sourceTitle": SENTENCES}),
                            json!({"id": "f2", "kind": "fork", "sourceChatId": "c2", "sourceTitle": ""}),
                            json!({"id": "r", "kind": "reasoning", "text": SENTENCES}),
                            text("p", "Done."),
                        ],
                    )],
                ) },
            ],
        ),
        (
            "Content shape",
            vec![
                sc(
                    "Emoji alone and mixed",
                    420,
                    vec![
                        user("u", "🎉"),
                        reply(
                            "a",
                            "complete",
                            vec![
                                text("p", "Shipped 🚀 the fix 👍🏽 for 🇫🇷 users"),
                                tool("t", exec("echo '🔥 hot path 🔥'"), "", false),
                                spawn("s", "Review 🧪 tests 👀", None, "done"),
                                input("q", true, vec![question("🎨", "Keep the 🌈 theme?")]),
                                error("e", "💥 crashed in 🧵 worker"),
                            ],
                        ),
                    ],
                ),
                sc(
                    "RTL and mixed direction",
                    420,
                    vec![
                        user("u", "أصلح شريط الأدوات ثم شغّل الاختبارات"),
                        reply(
                            "a",
                            "complete",
                            vec![
                                text("p", "שלום! הרצתי את `cargo test` על השרת ו-1678 בדיקות עברו."),
                                tool("t", exec("echo 'مرحبا بالعالم'"), "", false),
                                spawn("s", "مراجعة الشريط الجانبي", None, "done"),
                                input("q", true, vec![question("النطاق", "أي حزمة يجب أن يذهب إليها الإصلاح؟")]),
                                error("e", "خطأ: تعذر العثور على الملف crates/ui/src/lib.rs"),
                            ],
                        ),
                    ],
                ),
                sc(
                    "Diacritics and tall scripts",
                    420,
                    vec![reply(
                        "a",
                        "complete",
                        vec![
                            text("p", "Ứng dụng đã được cập nhật. ฏ์ฐู ปฏิบัติ. हिन्दी में परीक्षण। Z̴̡̛͎a̷̧͈l̸̢̛g̵̨̛o̶̧̕"),
                            tool("t", ToolCall::ReadFile { path: "docs/Ứng-dụng/ปฏิบัติ.md".into() }, "", false),
                            spawn("s", "Kiểm tra ỹ̃ Ặ ส้ม हिं", None, "done"),
                            input("q", true, vec![question("Ặ", "Giữ lại ฏ์ฐู không?")]),
                            error("e", "Z̴̡̛͎a̷̧͈l̸̢̛g̵̨̛o̶̧̕ ỹ̃ Ặ failed"),
                        ],
                    )],
                ),
                sc(
                    "Numbers that should align",
                    300,
                    done(vec![
                        edit("n1", "one.rs", 1111, 1111),
                        edit("n2", "eight.rs", 8888, 8888),
                        text("t", "| Run | Passed | Time |\n| --- | ---: | ---: |\n| 1 | 1111 | 11.11s |\n| 2 | 8888 | 88.88s |"),
                    ]),
                ),
            ],
        ),
    ]
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

struct Cell {
    label: &'static str,
    note: Option<&'static str>,
    height: f32,
    widths: [Entity<Transcript>; 3],
}

struct Page {
    sections: Vec<(&'static str, Vec<Cell>)>,
    active: usize,
    /// Capture renders one scenario at one width at a time.
    solo: Option<(usize, usize)>,
    scroll: ScrollHandle,
}

const SQUEEZE_TOTAL: f32 = 560.0;
const SQUEEZE_SIBLING: f32 = 220.0;

impl Render for Page {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::Theme::of(cx).clone();
        let label = |text: SharedString| {
            div()
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(text)
        };
        let tabs = div().flex().gap(px(6.0)).p(px(12.0)).children(
            self.sections.iter().enumerate().map(|(ix, (title, _))| {
                let active = ix == self.active;
                div()
                    .id(("tab", ix))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .text_color(if active { theme.text } else { theme.text_muted })
                    .when(active, |t| t.bg(theme.element_active))
                    .hover(|t| t.bg(theme.element_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.active = ix;
                        page.scroll.set_offset(gpui::point(px(0.), px(0.)));
                        cx.notify();
                    }))
                    .child(*title)
            }),
        );
        let (_, cells) = &self.sections[self.active];
        let body = div()
            .id("page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(16.0))
            .pb(px(40.0))
            .flex()
            .flex_col()
            .gap(px(28.0))
            .children({
                let solo = self.solo;
                let theme = theme.clone();
                cells
                    .iter()
                    .enumerate()
                    .filter(move |(ix, _)| solo.is_none_or(|(s, _)| s == *ix))
                    .map(move |(_, cell)| {
                        let show = |w: usize| solo.is_none_or(|(_, sw)| sw == w);
                        let frame = |t: &Entity<Transcript>| {
                            div()
                                .h(px(cell.height))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .overflow_hidden()
                                .child(t.clone())
                        };
                        let caption = |text: &'static str| {
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(text)
                        };
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(div().text_size(px(13.0)).child(cell.label))
                            .when_some(cell.note, |c, note| {
                                c.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme.danger)
                                        .child(format!("Broke: {note}")),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .gap(px(16.0))
                                    .when(show(0), |row| {
                                        row.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .child(caption("320 px"))
                                                .child(frame(&cell.widths[0]).w(px(320.0))),
                                        )
                                    })
                                    .when(show(1), |row| {
                                        row.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .child(caption("Squeezed by a sibling"))
                                                .child(
                                                    div()
                                                        .w(px(SQUEEZE_TOTAL))
                                                        .flex()
                                                        .child(frame(&cell.widths[1]).flex_1().min_w_0())
                                                        .child(
                                                            div()
                                                                .w(px(SQUEEZE_SIBLING))
                                                                .flex_none()
                                                                .h(px(cell.height))
                                                                .bg(theme.surface_raised),
                                                        ),
                                                ),
                                        )
                                    }),
                            )
                            .when(show(2), |c| {
                                c.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(caption("Full width"))
                                        .child(frame(&cell.widths[2])),
                                )
                            })
                    })
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .pt(px(28.0))
            .child(tabs)
            .child(body)
    }
}

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let core = runtime.block_on(async {
        zeron_engine::EngineCore::assemble(
            &temp.path().join("engine"),
            Arc::new(zeron_engine::default_registry()),
            zeron_proto::HarnessId::ClaudeCode,
            None,
        )
    })?;
    let ipc_port = port();
    let _ipc = runtime.block_on(zeron_engine::serve_ipc(ipc_port, core.rpc_service()))?;
    let data = temp.path().join("ui");
    std::fs::create_dir(&data)?;
    let device = core.device_id.clone();
    let args: Vec<String> = std::env::args().collect();
    let capture = args
        .iter()
        .position(|a| a == "--capture")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    let appearance_mode = if args.iter().any(|a| a == "--light") {
        appearance::AppearanceMode::Light
    } else {
        appearance::AppearanceMode::Dark
    };

    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let settings = settings::UiSettings::default();
            settings::init(settings.clone(), data.clone(), cx);
            let fonts = typography::register_fonts(cx);
            typography::init(
                settings.ui_font_family.clone(),
                settings.ui_font_size,
                settings.terminal_font_family.clone(),
                settings.terminal_font_size,
                settings.code_font_family.clone(),
                settings.code_font_size,
                fonts,
                cx,
            );
            theme_library::init(data.clone(), cx);
            appearance::init(
                appearance_mode,
                settings.theme_selection,
                settings.accent,
                settings.surface,
                cx,
            );
            history::init(
                settings.git_history_columns,
                settings.git_history_column_widths,
                settings.git_history_column_order,
                settings.git_history_author_display,
                cx,
            );
            composer::init(cx, settings.composer_send_behavior);
            terminal::panel::init(cx);
            app_menus::init(cx);

            let plan = sections();
            let state = cx.new(|_| {
                let mut s = state::AppState::new();
                s.connection = zeron_proto::view::ConnectionStatus::Ready;
                s.local_device_id = Some(device.clone());
                for (sx, (_, scenarios)) in plan.iter().enumerate() {
                    for (ix, scenario) in scenarios.iter().enumerate() {
                        let entries: Vec<Value> = scenario
                            .entries
                            .iter()
                            .cloned()
                            .map(|mut e| {
                                e["deviceId"] = json!(device);
                                e
                            })
                            .collect();
                        s.set_subagent_snapshot(
                            format!("break-{sx}-{ix}"),
                            serde_json::from_value(Value::Array(entries))
                                .expect("fixture entries decode"),
                        );
                    }
                }
                s
            });
            let window = cx
                .open_window(
                    WindowOptions {
                        window_background: theme::Theme::of(cx).window_background_appearance(),
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(40.), px(40.)),
                            size(px(1440.), px(900.)),
                        ))),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some("Transcript break".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(gpui::point(px(10.), px(8.))),
                        }),
                        ..Default::default()
                    },
                    |_, cx| {
                        cx.new(|cx| {
                            let sections = plan
                                .iter()
                                .enumerate()
                                .map(|(sx, (title, scenarios))| {
                                    let cells = scenarios
                                        .iter()
                                        .enumerate()
                                        .map(|(ix, s)| {
                                            let doc = format!("break-{sx}-{ix}");
                                            let make = |cx: &mut Context<Page>| {
                                                let state = state.clone();
                                                let doc = doc.clone();
                                                let live = s.live;
                                                cx.new(|cx| Transcript::for_doc(state, doc, live, cx))
                                            };
                                            Cell {
                                                label: s.label,
                                                note: s.note.or_else(|| NOTES.iter().find(|(l, _)| *l == s.label).map(|(_, n)| *n)),
                                                height: s.height,
                                                widths: [make(cx), make(cx), make(cx)],
                                            }
                                        })
                                        .collect();
                                    (*title, cells)
                                })
                                .collect();
                            Page {
                                sections,
                                active: 0,
                                solo: None,
                                scroll: ScrollHandle::new(),
                            }
                        })
                    },
                )
                .unwrap();
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.activate(true);
            let opens: Vec<Vec<(bool, bool)>> = plan
                .iter()
                .map(|(_, s)| s.iter().map(|s| (s.open, s.details)).collect())
                .collect();
            cx.spawn(async move |cx| {
                let timer = |cx: &gpui::AsyncApp, ms: u64| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(ms))
                };
                timer(cx, 600).await;
                state.update(cx, |_, cx| cx.notify());
                timer(cx, 400).await;
                // Open what each scenario asks for, then park every list at
                // its top (frozen docs land at the end once on open).
                let pages = window
                    .update(cx, |page, _, _| {
                        page.sections
                            .iter()
                            .map(|(_, cells)| {
                                cells.iter().map(|c| c.widths.clone()).collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                for (sx, cells) in pages.iter().enumerate() {
                    for (ix, widths) in cells.iter().enumerate() {
                        for t in widths {
                            t.update(cx, |t, cx| {
                                match opens[sx][ix] {
                                    (_, true) => t.fixture_open_all(cx),
                                    (true, false) => t.fixture_tool_images(true, cx),
                                    _ => {}
                                }
                            });
                        }
                    }
                }
                timer(cx, 600).await;
                for cells in &pages {
                    for widths in cells {
                        for t in widths {
                            t.update(cx, |t, cx| t.fixture_appshots_start(cx));
                        }
                    }
                }
                let Some(dir) = capture else { return };
                std::fs::create_dir_all(&dir).ok();
                for sx in 0..pages.len() {
                    window
                        .update(cx, |page, _, cx| {
                            page.active = sx;
                            page.scroll.set_offset(gpui::point(px(0.), px(0.)));
                            cx.notify();
                        })
                        .ok();
                    timer(cx, 1500).await;
                    for cells in &pages {
                        for widths in cells {
                            for t in widths {
                                t.update(cx, |t, cx| t.fixture_appshots_start(cx));
                            }
                        }
                    }
                    for ix in 0..pages[sx].len() {
                        for wx in 0..3 {
                            window
                                .update(cx, |page, _, cx| {
                                    page.solo = Some((ix, wx));
                                    cx.notify();
                                })
                                .ok();
                            timer(cx, 600).await;
                            gpui::AnyWindowHandle::from(window)
                                .update(cx, |_, w, cx| {
                                    w.draw(cx).clear();
                                    if let Ok(image) = w.render_to_image() {
                                        image
                                            .save(dir.join(format!("s{sx}-c{ix:02}-w{wx}.png")))
                                            .ok();
                                    }
                                })
                                .ok();
                        }
                    }
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    runtime.block_on(core.shutdown());
    Ok(())
}
