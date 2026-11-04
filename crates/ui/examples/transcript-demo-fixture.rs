//! Throwaway demo of the transcript tool-row polish: shell rows, path badges,
//! change counts, failed markers, wrapped invocations, sideways output, and a
//! running call's spinner and elapsed clock under a live session.
//! Isolated engine and data in a temp dir; no agent runs. Close to exit.
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use serde_json::{Value, json};
use std::sync::Arc;
use zeron_proto::ToolCall;
use zeron_ui::*;

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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

fn text(id: &str, text: &str) -> Value {
    json!({"id": id, "kind": "text", "text": text})
}

fn working_session(chat_id: &str, device: &str) -> zeron_proto::Session {
    let now = chrono::Utc::now();
    zeron_proto::Session {
        last_completed_turn: None,
        running_subagents: 0,
        chat_id: chat_id.into(),
        device_id: device.into(),
        status: zeron_proto::SessionStatus::Working,
        started_at: Some(now),
        updated_at: now,
    }
}

fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(workspace.join("crates/ui/src"))?;
    let cwd = workspace.to_string_lossy().into_owned();
    let runtime = tokio::runtime::Runtime::new()?;
    let core = runtime.block_on(async {
        zeron_engine::EngineCore::assemble(
            &temp.path().join("engine"),
            Arc::new(zeron_engine::default_registry()),
            zeron_proto::HarnessId::ClaudeCode,
            None,
        )
    })?;
    core.workspace
        .create_chat("demo", None, Some(&core.device_id), None, Some(cwd))?;
    core.workspace.rename_chat("demo", "Transcript polish demo")?;
    let ipc_port = port();
    let _ipc = runtime.block_on(zeron_engine::serve_ipc(ipc_port, core.rpc_service()))?;
    let data = temp.path().join("ui");
    std::fs::create_dir(&data)?;
    let boot = EngineBootConfig {
        data_dir: data.clone(),
        ipc_port,
        edge_url: String::new(),
        edge_token: None,
        org_id: None,
        workos_client_id: None,
        default_harness: zeron_proto::HarnessId::ClaudeCode,
    };
    let chats = core.workspace.read_chats()?;
    let device = core.device_id.clone();
    // `--capture <dir>` renders PNGs and quits; `--height <px>` makes the
    // window tall enough to capture a whole turn; `--light` flips appearance.
    let args: Vec<String> = std::env::args().collect();
    let arg = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let capture = arg("--capture").map(std::path::PathBuf::from);
    let window_height: f32 = arg("--height").and_then(|h| h.parse().ok()).unwrap_or(900.0);
    // `--ui-size <px>` sets the interface text size (12–20).
    let ui_size = arg("--ui-size")
        .and_then(|s| s.parse::<f32>().ok())
        .and_then(|px| typography::UiFontSize::ALL.into_iter().find(|s| s.pixels() == px));
    let appearance_mode = if args.iter().any(|a| a == "--light") {
        appearance::AppearanceMode::Light
    } else {
        appearance::AppearanceMode::Dark
    };

    let long_line = format!(
        "warning: field `selection_key` is never read --> crates/ui/src/transcript.rs:9256:5 {}",
        "| this line is deliberately long so it scrolls sideways instead of being cut off ".repeat(3)
    );
    let test_output = format!(
        "   Compiling zeron-ui v0.2.110\n{long_line}\n    Finished `test` profile in 41.2s\n     Running unittests src/lib.rs\n\nrunning 1677 tests\ntest result: ok. 1677 passed; 0 failed"
    );
    let showcase = json!([
        {"id": "u7", "role": "user", "createdAt": 1788900005800_i64, "deviceId": device,
         "parts": [text("t7", "Ouput many type of \"formats\" of answers (mermaid, tables, lists, texts, etc....) do not create anything, just do this in this convo")]},
        {"id": "a10", "role": "assistant", "createdAt": 1788900005810_i64, "deviceId": device, "status": "complete",
         "parts": [text("p12", include_str!("/tmp/formats/showcase1.md"))]},
        {"id": "u8", "role": "user", "createdAt": 1788900005820_i64, "deviceId": device,
         "parts": [text("t8", "Do much more, different ones")]},
        {"id": "a11", "role": "assistant", "createdAt": 1788900005830_i64, "deviceId": device, "status": "complete",
         "parts": [text("p13", include_str!("/tmp/formats/showcase2.md"))]},
    ]);
    let mut entries = json!([
        {"id": "u1", "role": "user", "createdAt": 1788900000000_i64, "deviceId": device,
         "parts": [text("t", "Tidy the transcript's tool rows and check the suite still passes.")]},
        {"id": "a1", "role": "assistant", "createdAt": 1788900001000_i64, "deviceId": device, "status": "complete",
         "parts": [
            text("p0", "Looking around first. **Expand the group**, then any row: Exec rows open into a highlighted shell block."),
            tool("r1", exec("git status --short"), " M crates/ui/src/transcript.rs\n?? crates/ui/src/transcript/shell_rows.rs", false),
            tool("r2", exec("rg -n \"fn path_badge\" crates/ui/src"), "crates/ui/src/transcript.rs:9308:fn path_badge(", false),
            tool("r3", exec("sed -n '9300,9360p' crates/ui/src/transcript.rs"), "fn path_badge(\n    identity: FileIconIdentity<'_>,\n    failed: bool,", false),
            tool("r4", exec("cat Cargo.toml"), "[workspace]\nmembers = [\"crates/*\"]", false),
            tool("r5", exec("ls crates/ui/src/transcript"), "shell_rows.rs", false),
            tool("r6", ToolCall::ReadFile { path: "crates/ui/src/transcript.rs".into() }, "", false),
            tool("r7", ToolCall::Search { pattern: "ToolMetrics".into(), path: Some("crates/ui/src".into()) }, "crates/ui/src/transcript.rs", false),
         ]},
        {"id": "a2", "role": "assistant", "createdAt": 1788900002000_i64, "deviceId": device, "status": "complete",
         "parts": [
            text("p1", "Editing. File badges carry **+N −M** change counts."),
            edit("e1", "crates/ui/src/transcript.rs", 42, 17),
            edit("e2", "crates/ui/src/transcript/shell_rows.rs", 128, 0),
            edit("e3", "crates/proto/src/view.rs", 6, 3),
            tool("e4", ToolCall::WriteFile { path: "crates/proto/src/shell_command.rs".into(), content: None }, "", false),
         ]},
        {"id": "a3", "role": "assistant", "createdAt": 1788900003000_i64, "deviceId": device, "status": "streaming",
         "parts": [
            text("p2", "Verifying. A failed call gets a non-colour marker; a long invocation wraps at word and operator boundaries; long output lines scroll sideways. The last call is **still running**: its row spins and counts."),
            tool("v1", exec("cargo check -p zeron-ui --tests --message-format short"), "error[E0063]: missing field `shell` in initializer of `transcript::ToolItem`", true),
            tool("v2", exec("cargo test -p zeron-ui -- --test-threads=1 2>&1 | grep -E '^test result|FAILED|panicked' | head -40 && rustfmt --edition 2024 --check crates/ui/src/transcript.rs crates/ui/src/transcript/shell_rows.rs"), &test_output, false),
            tool("v3", exec("git commit -qam \"Transcript: tidy change counts, path badges and the git verb\""), "", false),
            running("v4", exec("cargo test -p zeron-ui -- --test-threads=1")),
         ]},
        {"id": "u2", "role": "user", "createdAt": 1788900004000_i64, "deviceId": device,
         "parts": [text("t2", "Now show me one of every tool row kind.")]},
        {"id": "a4", "role": "assistant", "createdAt": 1788900005000_i64, "deviceId": device, "status": "complete",
         "parts": [
            {"id": "th1", "kind": "reasoning", "text": "**Thought process** chip: the agent's reasoning, flattened into styled lines.\n\nIt wraps at the detail width and keeps `inline code` and *emphasis*."},
            text("p3", "The catalogue, one row per `ToolCall` variant plus the spawn, input and error parts."),
            tool("c1", exec("make build"), "ok", false),
            tool("c2", ToolCall::ReadFile { path: "README.md".into() }, "", false),
            tool("c3", ToolCall::WriteFile { path: "docs/notes.md".into(), content: None }, "", false),
            edit("c4", "crates/ui/src/lib.rs", 3, 1),
            tool("c5", ToolCall::ApplyPatch { path: Some("crates/ui/src/shell.rs".into()) }, "", false),
            tool("c6", ToolCall::ApplyPatch { path: None }, "", false),
            tool("c7", ToolCall::Search { pattern: "fn render".into(), path: Some("crates/ui/src".into()) }, "12 matches", false),
            tool("c8", ToolCall::Search { pattern: "TODO".into(), path: None }, "3 matches", false),
            tool("c9", ToolCall::Glob { pattern: "**/*.rs".into() }, "412 files", false),
            tool("c10", ToolCall::WebFetch { url: "https://docs.rs/gpui".into(), prompt: Some("Find the list element docs".into()) }, "Fetched 48 KB", false),
            tool("c11", ToolCall::WebSearch { query: "gpui uniform_list virtualization".into() }, "5 results", false),
            tool("c13", ToolCall::Mcp { server: "github".into(), tool: "list_pull_requests".into(), input: Some(json!({"repo": "zeronsh/comet", "state": "open"})) }, "14 open", false),
            tool("c14", ToolCall::Unknown { name: "NotebookEdit".into(), input: Some(json!({"cell": 3})) }, "", false),
            tool("c15", ToolCall::Unknown { name: "Bash".into(), input: None }, "permission denied", true),
            {"id": "s1", "kind": "tool", "isError": false, "resolved": true,
             "call": ToolCall::Unknown { name: "Agent: Audit the transcript tests".into(), input: Some(json!({"model": "sonnet"})) },
             "subagentRef": "sub-done", "subagentStatus": "done", "subagentTail": "Found 3 flaky tests."},
            {"id": "s2", "kind": "tool", "isError": false, "resolved": false,
             "call": ToolCall::Unknown { name: "Agent: Explore the markdown crate".into(), input: None },
             "subagentRef": "sub-running", "subagentStatus": "running"},
            {"id": "s3", "kind": "tool", "isError": true, "resolved": true,
             "call": ToolCall::Unknown { name: "Agent: Port the sidebar".into(), input: None },
             "subagentRef": "sub-failed", "subagentStatus": "failed"},
            {"id": "q1", "kind": "input", "requestId": "req-1", "resolved": true,
             "questions": [{"id": "q", "header": "Scope", "question": "Which crate should the fix land in?",
                            "options": ["zeron-ui", "zeron-proto"], "multiSelect": false, "prefill": null, "multiline": false}]},
            {"id": "err1", "kind": "error", "message": "The agent process exited with code 137 (out of memory)."},
            text("p4", "That is every kind the transcript knows how to draw."),
         ]},
        {"id": "u4", "role": "user", "createdAt": 1788900005100_i64, "deviceId": device,
         "parts": [text("t4", "Fan out reviewers, drive the app, and make a cover image.")]},
        {"id": "a6", "role": "assistant", "createdAt": 1788900005200_i64, "deviceId": device, "status": "complete",
         "parts": [
            text("p6", "Patterns from real sessions: skills, tool search, MCP calls, image generation, and a reviewer fan-out."),
            tool("m1", ToolCall::Unknown { name: "Skill".into(), input: Some(json!({"skill": "zeron-gpui"})) }, "", false),
            tool("m2", ToolCall::Unknown { name: "ToolSearch".into(), input: Some(json!({"query": "select:mcp__zeron__list_chats"})) }, "", false),
            tool("m3", ToolCall::Mcp { server: "zeron".into(), tool: "list_chats".into(), input: Some(json!({"limit": 40})) }, "40 chats", false),
            tool("m4", ToolCall::Mcp { server: "zeron".into(), tool: "send_message".into(), input: Some(json!({"chat": "Sidebar", "message": "Rebase onto main", "wait": false})) }, "sent", false),
            tool("m5", ToolCall::Mcp { server: "cua_repl".into(), tool: "js".into(), input: Some(json!({"code": "await app.click('#composer-send'); await app.screenshot()"})) }, "ok", false),
            tool("m6", ToolCall::Mcp { server: "codex_apps".into(), tool: "cloudflare.execute".into(), input: Some(json!({"query": "SELECT count(*) FROM requests"})) }, "Error: token expired", true),
            tool("m7", exec("pnpm --filter landing build"), "built in 6.1s", false),
            tool("m8", ToolCall::Unknown { name: "Generate image".into(), input: Some(json!({"prompt": "An engraved black-and-ivory cat beside a terminal, restrained violet light"})) }, "", false),
            tool("m9", ToolCall::Unknown { name: "Monitor".into(), input: Some(json!({"command": "tail -f target/test.log"})) }, "", false),
            {"id": "s4", "kind": "tool", "isError": false, "resolved": true,
             "call": ToolCall::Unknown { name: "Agent: ui_test_scout".into(), input: Some(json!({"model": "haiku"})) },
             "subagentRef": "sub-scout", "subagentStatus": "done"},
            {"id": "s5", "kind": "tool", "isError": false, "resolved": false,
             "call": ToolCall::Unknown { name: "Agent: Investigate voice UI performance".into(), input: Some(json!({"model": "opus"})) },
             "subagentRef": "sub-voice", "subagentStatus": "running"},
            {"id": "s6", "kind": "tool", "isError": true, "resolved": true,
             "call": ToolCall::Unknown { name: "Agent: Final mergeability review PR 677".into(), input: None },
             "subagentRef": "sub-review", "subagentStatus": "failed"},
            tool("m10", ToolCall::Unknown { name: "Wait for agents".into(), input: None }, "", false),
            {"id": "rc1", "kind": "error", "message": "Reconnecting... 2/5"},
            {"id": "rc2", "kind": "error", "message": "Reconnecting... 3/5"},
            {"id": "rc3", "kind": "error", "message": "Reconnecting... 4/5"},
            {"id": "rc4", "kind": "error", "message": "Reconnecting... 5/5"},
            text("p7", "The connection dropped for a moment and the run carried on: four retries fold into one quiet row."),
            {"id": "q3", "kind": "input", "requestId": "req-3", "resolved": true,
             "questions": [
                {"id": "qa", "header": "Left edge", "question": "When the sidebar is collapsed, its drag-to-open handle overlaps the window's left resize edge. How should it behave?",
                 "options": ["Window resize wins (Recommended)", "Shrink the handle", "Leave as on main"], "multiSelect": false, "multiline": false},
                {"id": "qb", "header": "Visuel", "question": "Quels autres éléments veux-tu conserver ?",
                 "options": ["Ligne active hors des fondus", "Libellés de sections"], "multiSelect": true, "multiline": false}]},
         ]},
        {"id": "u5", "role": "user", "createdAt": 1788900005300_i64, "deviceId": device,
         "parts": [text("t5", "Try the Codex harness on the release build.")]},
        {"id": "a7", "role": "assistant", "createdAt": 1788900005400_i64, "deviceId": device, "status": "complete",
         "parts": [
            text("p8", "Errors as they really arrive: the same one three times, an API's raw JSON body, and a missing harness."),
            tool("f1", exec("cargo build --release -p zeron-app"), "error: could not compile `zeron-engine`", true),
            {"id": "x20", "kind": "error", "message": "io: Too many open files (os error 24)"},
            {"id": "x21", "kind": "error", "message": "io: Too many open files (os error 24)"},
            {"id": "x22", "kind": "error", "message": "io: Too many open files (os error 24)"},
            text("p9", "Raised the file limit and retried."),
            {"id": "x23", "kind": "error", "message": "{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'gpt-6.1-sol' model is not supported with this method.\"}}"},
            {"id": "x24", "kind": "error", "message": "harness binary not found: /Users/gael/.local/bin/codex"},
         ]},
        {"id": "u6", "role": "user", "createdAt": 1788900005500_i64, "deviceId": device,
         "parts": [text("t6", "Stop, I'll do it by hand.")]},
        {"id": "a8", "role": "assistant", "createdAt": 1788900005600_i64, "deviceId": device, "status": "aborted",
         "parts": [
            text("p10", "Starting on the release notes."),
            tool("g1", exec("git log --oneline v0.2.110..HEAD"), "54954e4d2 Settings: quieter Providers page", false),
            edit("g2", "CHANGELOG.md", 12, 0),
         ]},
        {"id": "a9", "role": "assistant", "createdAt": 1788900005700_i64, "deviceId": device, "status": "complete",
         "parts": [
            text("p11", "When the retries run out, the turn ends on an error instead."),
            {"id": "rl1", "kind": "error", "message": "Reconnecting... 2/2"},
         ]},
        {"id": "u3", "role": "user", "createdAt": 1788900006000_i64, "deviceId": device,
         "parts": [text("t3", &format!("A long prompt folds behind Show more.\n\n{}", (1..=12).map(|i| format!("{i}. Check that line {i} of this prompt still wraps inside the bubble and that the fold keeps the first five lines.")).collect::<Vec<_>>().join("\n")))]},
        {"id": "a5", "role": "assistant", "createdAt": 1788900007000_i64, "deviceId": device, "status": "complete",
         "parts": [
            {"id": "fork1", "kind": "fork", "sourceChatId": "chat-origin", "sourceTitle": "Palette and transcript polish"},
            text("p5", "Edge cases: scripts, hard splits, reads by range, long output, an inline diff, a failed edit, outside paths."),
            text("p5t", "**Audit, transcript tool rows and chips.** Verified by code and by rendered captures.\n\n| Severity | Location | Before | After | Why |\n| --- | --- | --- | --- | --- |\n| MEDIUM | `transcript.rs` group header | one clipped string, failure count last | summary ellipsizes; `+N −M` and `N failed` in an untruncated tail | Truncate without losing content: the fact being cut was the one that mattered |\n| MEDIUM | `transcript.rs:10130` model label, elapsed label | 11 px | 12 px (`TOOL_LABEL_SIZE`) | Size floor: the project already names 12 px as the UI text floor |\n| LOW | `crates/ui/src/transcript/shell_rows.rs` | fixed `px()` sizes | `ui_rems` | Type scale |\n\nTotals, right-aligned:\n\n| Crate | Files | Added | Removed |\n| :--- | ---: | ---: | ---: |\n| zeron-ui | 4 | 1,284 | 312 |\n| zeron-proto | 1 | 96 | 8 |\n\nA wide one:\n\n| Command | Output | Exit | Duration | Host | Notes |\n| --- | --- | --- | --- | --- | --- |\n| `cargo test -p zeron-ui --lib -- --test-threads=1` | 1678 passed | 0 | 4m 12s | macbook | serial on macOS, the parallel suite aborts |\n| `cargo check -p zeron-ui` | ok | 0 | 38s | macbook | per commit |"),
            tool("x1", exec("python3 - <<'EOF'\nimport json, sys\nfor line in sys.stdin:\n    print(json.loads(line)['id'])\nEOF"), "t1\nt2", false),
            tool("x2", exec("node -e 'console.log(require(\"./package.json\").version)'"), "0.2.110", false),
            tool("x3", exec("sed -n '120,160p' crates/ui/src/transcript/shell_rows.rs"), "pub(super) fn text_runs(", false),
            tool("x4", exec("head -n 40 README.md"), "# Zeron", false),
            tool("x5", exec("find crates -name '*.rs' -newer Cargo.lock"), "crates/ui/src/transcript.rs", false),
            tool("x6", exec("cargo test -p zeron-proto"), "test result: ok. 74 passed", false),
            tool("x7", exec("npm run build -- --mode=production --outDir=dist/release --sourcemap=hidden --minify=esbuild --target=es2022 --emptyOutDir --reportCompressedSize=false"), "built in 4.2s", false),
            tool("x8", exec("cat /etc/hosts"), "127.0.0.1 localhost", false),
            tool("x9", ToolCall::ReadFile { path: "C:\\Users\\gael\\work\\comet\\crates\\ui\\src\\lib.rs".into() }, "", false),
            tool("x10", exec("rg -n 'fn ' crates/ui/src --type rust | head -n 40"), &(1..=40).map(|i| format!("crates/ui/src/transcript.rs:{}:    fn render_row_{i}(&mut self, cx: &mut Context<Self>)", i * 97)).collect::<Vec<_>>().join("\n"), false),
            {"id": "x11", "kind": "tool", "isError": false, "resolved": true,
             "call": ToolCall::EditFile { path: "crates/ui/src/motion.rs".into(), old_string: None, new_string: None },
             "diff": {"path": "crates/ui/src/motion.rs",
                      "oldText": "pub const FADE_QUICK: MotionSpec = MotionSpec::new(150, EASE);\npub const MENU_IN: MotionSpec = MotionSpec::new(140, EASE);\n",
                      "newText": "pub const FADE_QUICK: MotionSpec = MotionSpec::new(150, EASE);\n/// The row settle crossfade.\npub const ROW_SETTLE: MotionSpec = MotionSpec::new(300, EASE_OUT_SOFT);\npub const MENU_IN: MotionSpec = MotionSpec::new(140, EASE);\n"}},
            {"id": "x12", "kind": "tool", "isError": true, "resolved": true,
             "call": ToolCall::EditFile { path: "crates/ui/src/missing.rs".into(), old_string: None, new_string: None },
             "output": "File does not exist."},
            tool("x13", ToolCall::Mcp { server: "playwright".into(), tool: "browser_navigate".into(), input: Some(json!({"url": "https://zeron.sh/docs/getting-started?ref=demo", "waitUntil": "networkidle", "timeout": 30000, "headers": {"Accept-Language": "fr-FR"}})) }, "Navigated", false),
            {"id": "q2", "kind": "input", "requestId": "req-2", "resolved": false,
             "questions": [{"id": "q", "header": "Rebase", "question": "Rebase onto main before pushing?",
                            "options": ["Yes", "No"], "multiSelect": false, "prefill": null, "multiline": false}]},
         ]},
    ]);
    {
        let list = entries.as_array_mut().unwrap();
        let at = list.iter().position(|e| e["id"] == "u3").unwrap();
        let extra = showcase.as_array().unwrap().clone();
        list.splice(at..at, extra);
    }

    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let mut settings = settings::UiSettings::default();
            if let Some(size) = ui_size {
                settings.ui_font_size = size;
            }
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
            let session_device = device.clone();
            let state = cx.new(|_| {
                let mut s = state::AppState::new();
                s.connection = zeron_proto::view::ConnectionStatus::Ready;
                s.workspace_scope = Some(zeron_proto::WorkspaceScope::Development);
                s.local_device_id = Some(device.clone());
                s.devices = vec![
                    serde_json::from_value(json!({"id": device, "name": "This device",
                    "platform": std::env::consts::OS, "lastSeenAt": null}))
                    .unwrap(),
                ];
                s.chats = chats;
                s.selected_chat = Some("demo".into());
                s.auto_selected = true;
                s.chats_synced = true;
                s.spaces_synced = true;
                s.no_project = true;
                s.sessions = vec![working_session("demo", &session_device)];
                s
            });
            // The app's own chrome: no native titlebar, inset traffic lights
            // over the custom-drawn strip (lib.rs main window).
            let window = cx
                .open_window(
                    WindowOptions {
                        window_background: theme::Theme::of(cx).window_background_appearance(),
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(40.), px(40.)),
                            size(px(1200.), px(window_height)),
                        ))),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: None,
                            appears_transparent: true,
                            traffic_light_position: Some(gpui::point(px(14.), px(14.))),
                        }),
                        app_owns_titlebar_drag: true,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| shell::Shell::new(state.clone(), boot, cx)),
                )
                .unwrap();
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.activate(true);
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(800))
                    .await;
                state.update(cx, |s, cx| {
                    s.receive_transcript_frame(
                        zeron_doc::TranscriptFrame::Reset {
                            reset: serde_json::from_value(entries).unwrap(),
                        },
                        cx,
                    )
                    .unwrap();
                    cx.notify();
                });
                if let Some(dir) = capture {
                    // Collapsed, then every group open, then quit.
                    std::fs::create_dir_all(&dir).ok();
                    let shot = |cx: &mut gpui::AsyncApp, name: &str| {
                        let path = dir.join(format!("{name}.png"));
                        // The untyped handle: drawing must not hold the
                        // Shell lease it renders.
                        gpui::AnyWindowHandle::from(window)
                            .update(cx, |_, w, cx| {
                                w.draw(cx).clear();
                                if let Ok(image) = w.render_to_image() {
                                    image.save(&path).ok();
                                }
                            })
                            .ok();
                    };
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(2500))
                        .await;
                    let transcript = window
                        .update(cx, |shell, _, _| shell.fixture_transcript())
                        .unwrap();
                    // Page through from the top with real wheel input (which
                    // also breaks the tail pin), collapsed then expanded.
                    for (phase, open) in [("collapsed", false), ("expanded", true)] {
                        transcript.update(cx, |t, cx| {
                            t.fixture_tool_images(open, cx);
                            t.fixture_appshots_start(cx);
                        });
                        let wheel = |cx: &mut gpui::AsyncApp, dy: f32| {
                            gpui::AnyWindowHandle::from(window)
                                .update(cx, |_, w, cx| {
                                    w.dispatch_event(
                                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                            position: gpui::point(px(800.), px(400.)),
                                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                                px(0.),
                                                px(dy),
                                            )),
                                            ..Default::default()
                                        }),
                                        cx,
                                    );
                                })
                                .ok();
                        };
                        wheel(cx, 20000.0);
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(1500))
                            .await;
                        for page in 0..60 {
                            shot(cx, &format!("{phase}-{page:02}"));
                            wheel(cx, -360.0);
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(600))
                                .await;
                        }
                    }
                    cx.update(|cx| cx.quit());
                    return;
                }
                // Keep the fixture session fresh: a Working session goes
                // stale without heartbeats, and the row's spinner follows it.
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(5))
                        .await;
                    state.update(cx, |s, cx| {
                        for session in &mut s.sessions {
                            session.updated_at = chrono::Utc::now();
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
        });
    runtime.block_on(core.shutdown());
    Ok(())
}
