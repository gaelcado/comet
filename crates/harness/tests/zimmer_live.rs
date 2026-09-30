//! The REAL `zimmer acp` binary driven through Zeron's [`AcpHarness`], with
//! Zimmer's scripted mock provider (no network, no API spend) and an isolated
//! `ZIMMER_HOME`/`ZIMMER_CONFIG` per test.
//!
//! Zimmer is not publicly distributed, so these tests skip (pass with a note)
//! unless `ZIMMER_ACP_EXECUTABLE` names a built binary:
//!
//! ```sh
//! cargo build -p zimmer-cli --manifest-path /path/to/zimmer/Cargo.toml
//! ZIMMER_ACP_EXECUTABLE=/path/to/zimmer/target/debug/zimmer \
//!   cargo test -p zeron-harness --test zimmer_live -- --nocapture
//! ```

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use zeron_harness::{
    AcpHarness, CancellationToken, Harness, HarnessError, RunControls, SteerMessage,
};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, ReasoningLevel, RunRequest, SandboxLevel, SteeringMode,
    ToolCall, UserInputAnswer,
};

const LADDER: [ReasoningLevel; 5] = [
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::XHigh,
    ReasoningLevel::Max,
];

/// The real binary, or `None` (the test then skips) when it isn't configured.
fn zimmer_binary() -> Option<PathBuf> {
    let path = std::env::var_os("ZIMMER_ACP_EXECUTABLE").filter(|p| !p.is_empty())?;
    let path = PathBuf::from(path);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!(
            "skipping: ZIMMER_ACP_EXECUTABLE={} is not a file",
            path.display()
        );
        None
    }
}

macro_rules! require_zimmer {
    () => {
        match zimmer_binary() {
            Some(binary) => binary,
            None => {
                eprintln!("skipping: set ZIMMER_ACP_EXECUTABLE to a built `zimmer` binary");
                return;
            }
        }
    };
}

/// One isolated Zimmer install: its own data home, an empty config, a mock
/// script, a record of every model request, and a workspace.
struct Rig {
    dir: tempfile::TempDir,
    binary: PathBuf,
}

impl Rig {
    fn new(binary: PathBuf) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        std::fs::create_dir_all(dir.path().join("workspace")).unwrap();
        std::fs::write(dir.path().join("config.toml"), "").unwrap();
        Self { dir, binary }
    }

    fn workspace(&self) -> PathBuf {
        self.dir.path().join("workspace").canonicalize().unwrap()
    }

    fn record_path(&self) -> PathBuf {
        self.dir.path().join("requests.jsonl")
    }

    fn wire_path(&self) -> PathBuf {
        self.dir.path().join("wire.jsonl")
    }

    /// Every model request Zimmer's mock provider received, in order.
    fn requests(&self) -> Vec<Value> {
        std::fs::read_to_string(self.record_path())
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Everything Zeron wrote to the agent's stdin (only with `tee_wire`).
    fn wire(&self) -> Vec<Value> {
        std::fs::read_to_string(self.wire_path())
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// A launcher that pins Zimmer to this rig and its mock provider. Zeron
    /// appends nothing but the spec's `acp` argument, so the wrapper adds
    /// `--model mock`. `tee_wire` also captures Zeron's side of the wire.
    fn harness(&self, name: &str, script: Value, tee_wire: bool) -> AcpHarness {
        let root = self.dir.path();
        let script_path = root.join(format!("{name}.json"));
        std::fs::write(&script_path, serde_json::to_vec(&script).unwrap()).unwrap();
        let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "'\\''"));
        let launch = format!("{} \"$@\" --model mock", quote(&self.binary));
        let body = format!(
            "#!/bin/sh\n\
             export ZIMMER_HOME={home}\n\
             export ZIMMER_CONFIG={config}\n\
             export ZIMMER_MOCK_SCRIPT={script}\n\
             export ZIMMER_MOCK_RECORD={record}\n\
             {run}\n",
            home = quote(&root.join("home")),
            config = quote(&root.join("config.toml")),
            script = quote(&script_path),
            record = quote(&self.record_path()),
            run = if tee_wire {
                format!("tee -a {} | {launch}", quote(&self.wire_path()))
            } else {
                format!("exec {launch}")
            },
        );
        let wrapper = root.join(format!("{name}.sh"));
        std::fs::write(&wrapper, body).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        AcpHarness::zimmer()
            .with_executable(wrapper)
            .with_handshake_timeout(Duration::from_secs(30))
            .with_model_discovery_timeout(Duration::from_secs(30))
            .with_graces(Duration::from_secs(5), Duration::from_secs(3))
    }

    fn request(&self, prompt: &str) -> RunRequest {
        RunRequest {
            mcp: None,
            prompt: prompt.into(),
            harness: Some(HarnessId::Zimmer),
            model: None,
            reasoning: None,
            model_options: serde_json::Map::new(),
            cwd: self.workspace().display().to_string(),
            sandbox: SandboxLevel::WorkspaceWrite,
            auto_approve: true,
            attachments: Vec::new(),
            worktree: None,
            resume: None,
        }
    }
}

fn controls() -> (RunControls, mpsc::Sender<SteerMessage>, CancellationToken) {
    let (steer_tx, steer_rx) = mpsc::channel(8);
    let token = CancellationToken::new();
    let controls = RunControls {
        execution_lease: None,
        request_input: Box::new(move |questions| {
            let (tx, rx) = oneshot::channel();
            let answers: Vec<UserInputAnswer> = questions
                .iter()
                .map(|q| UserInputAnswer {
                    question_id: q.id.clone(),
                    labels: vec!["Yes".into()],
                })
                .collect();
            let _ = tx.send(answers);
            rx
        }),
        steering: steer_rx,
        interrupt: token.clone(),
    };
    (controls, steer_tx, token)
}

/// Collect events through the first `Done`, running `on_event` on each.
async fn collect_turn(
    stream: BoxStream<'static, Result<AgentEvent, HarnessError>>,
    mut on_event: impl FnMut(&AgentEvent),
) -> Vec<AgentEvent> {
    tokio::time::timeout(Duration::from_secs(60), async move {
        let mut stream = stream;
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            let event = event.expect("stream event");
            on_event(&event);
            let done = matches!(event, AgentEvent::Done { .. });
            events.push(event);
            if done {
                break;
            }
        }
        events
    })
    .await
    .expect("turn finished in time")
}

fn dones(events: &[AgentEvent]) -> Vec<(DoneStatus, Option<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Done { status, error, .. } => Some((*status, error.clone())),
            _ => None,
        })
        .collect()
}

fn text(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn session_id(events: &[AgentEvent]) -> String {
    events
        .iter()
        .find_map(|e| match e {
            AgentEvent::SessionStarted {
                harness,
                session_id,
                ..
            } => {
                assert_eq!(*harness, HarnessId::Zimmer);
                Some(session_id.clone())
            }
            _ => None,
        })
        .expect("SessionStarted")
}

fn say(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

#[test]
fn descriptor_surface_matches_registry_expectations() {
    let zimmer = AcpHarness::zimmer();
    assert_eq!(zimmer.id(), HarnessId::Zimmer);
    assert_eq!(zimmer.display_name(), "Zimmer");
    assert!(zimmer.supports_steering());
    assert_eq!(zimmer.steering_mode(), SteeringMode::StepBoundary);
    assert_eq!(zimmer.reasoning_levels(), &LADDER);
    // The pass-through row names the configured model when discovery fails.
    let fallback = zimmer.fallback_models();
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].id, "default");
}

/// An override naming a missing binary reads as not installed (never a
/// composer entry that fails at launch), whatever PATH holds.
#[tokio::test]
async fn missing_override_is_not_installed() {
    let zimmer = AcpHarness::zimmer().with_executable("/nonexistent/zimmer");
    assert!(!zimmer.installed());
    let Err(error) = zimmer.resolve_program(false).await else {
        panic!("a missing override must not resolve");
    };
    assert!(
        matches!(&error, HarnessError::NotInstalled(m) if m.contains("/nonexistent/zimmer")),
        "{error}"
    );
}

/// (a) One turn streams text and completes; the ladder and model list come
/// off Zimmer's own `configOptions`, and the chosen effort reaches the model.
#[tokio::test]
async fn real_zimmer_streams_a_turn_and_applies_the_effort() {
    let binary = require_zimmer!();
    let rig = Rig::new(binary);
    let harness = rig.harness("text", json!([say("hello from zimmer")]), false);

    let models = harness.models().await.expect("models discovered");
    assert!(!models.is_empty(), "{models:?}");
    for model in &models {
        assert_eq!(model.reasoning_levels, LADDER, "{model:?}");
    }

    let (controls, _steer, _token) = controls();
    let mut request = rig.request("say hello");
    request.reasoning = Some(ReasoningLevel::High);
    let stream = harness.run(request, controls).await.expect("run starts");
    let events = collect_turn(stream, |_| {}).await;
    session_id(&events);
    assert_eq!(text(&events), "hello from zimmer", "{events:?}");
    assert_eq!(dones(&events), vec![(DoneStatus::Completed, None)]);

    // `thought_level=high` was applied through session/set_config_option.
    let requests = rig.requests();
    let last = requests.last().expect("a recorded model request");
    assert_eq!(last["reasoning"], "high", "{last}");
}

/// (b) A scripted `read` of a workspace file surfaces as a typed tool call
/// with its result, then the turn completes.
#[tokio::test]
async fn real_zimmer_tool_call_surfaces_as_a_tool_event() {
    let binary = require_zimmer!();
    let rig = Rig::new(binary);
    std::fs::write(rig.workspace().join("notes.txt"), "zimmer-notes-marker\n").unwrap();
    let harness = rig.harness(
        "tool",
        json!([
            { "content": [{
                "type": "tool_call", "id": "call-1", "name": "read",
                "arguments": { "path": "notes.txt" }
            }] },
            say("read it"),
        ]),
        false,
    );
    let (controls, _steer, _token) = controls();
    let stream = harness
        .run(rig.request("read notes.txt"), controls)
        .await
        .expect("run starts");
    let events = collect_turn(stream, |_| {}).await;

    let (tool_id, call) = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::ToolCall { id, call } => Some((id.clone(), call.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no tool call: {events:?}"));
    assert!(
        matches!(&call, ToolCall::ReadFile { path } if path.ends_with("notes.txt")),
        "{call:?}"
    );
    let result = events.iter().find_map(|e| match e {
        AgentEvent::ToolResult {
            id,
            is_error,
            output,
            ..
        } if *id == tool_id => Some((*is_error, output.clone())),
        _ => None,
    });
    let (is_error, output) = result.unwrap_or_else(|| panic!("no tool result: {events:?}"));
    assert!(!is_error, "{output:?}");
    assert!(
        output
            .as_deref()
            .is_some_and(|o| o.contains("zimmer-notes-marker")),
        "{output:?}"
    );
    assert!(text(&events).contains("read it"), "{events:?}");
    assert_eq!(dones(&events), vec![(DoneStatus::Completed, None)]);
}

/// (c) A steer during a slow turn is injected through `_session/steering`
/// (detected from `initialize._meta`), within the SAME `session/prompt`.
#[tokio::test]
async fn real_zimmer_steer_injects_mid_turn_via_the_extension() {
    let binary = require_zimmer!();
    let rig = Rig::new(binary);
    let slow = "one two three four five six seven eight nine ten eleven twelve";
    let harness = rig.harness(
        "steer",
        json!([
            { "content": [{ "type": "text", "text": slow }], "chunk_delay_ms": 150 },
            say("steered reply"),
        ]),
        true,
    );
    let (controls, steer, _token) = controls();
    let stream = harness
        .run(rig.request("count slowly"), controls)
        .await
        .expect("run starts");
    let mut sent = false;
    let events = collect_turn(stream, |event| {
        if !sent && matches!(event, AgentEvent::TextDelta { .. }) {
            sent = true;
            steer
                .try_send(SteerMessage {
                    prompt: "redirect please".into(),
                    message_id: None,
                })
                .expect("steer sent");
        }
    })
    .await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Steered { .. })),
        "{events:?}"
    );
    assert!(text(&events).ends_with("steered reply"), "{events:?}");
    assert_eq!(dones(&events), vec![(DoneStatus::Completed, None)]);

    // On the wire: one prompt, one steering call carrying the text.
    let wire = rig.wire();
    let methods: Vec<&str> = wire.iter().filter_map(|m| m["method"].as_str()).collect();
    assert_eq!(
        methods.iter().filter(|m| **m == "session/prompt").count(),
        1,
        "{methods:?}"
    );
    let steering: Vec<&Value> = wire
        .iter()
        .filter(|m| m["method"] == "_session/steering")
        .collect();
    assert_eq!(steering.len(), 1, "{methods:?}");
    assert!(
        steering[0].to_string().contains("redirect please"),
        "{}",
        steering[0]
    );
    // The model saw the steer as a user message before its second answer.
    let requests = rig.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(
        requests[1]["messages"]
            .to_string()
            .contains("redirect please")
    );
}

/// (d) An interrupt during a hung model call ends the turn Interrupted via
/// `session/cancel`, well before the SIGTERM escalation grace.
#[tokio::test]
async fn real_zimmer_interrupt_ends_the_turn_interrupted() {
    let binary = require_zimmer!();
    let rig = Rig::new(binary);
    let harness = rig.harness(
        "interrupt",
        json!([{ "content": [{ "type": "text", "text": "working " }], "hang": true }]),
        false,
    );
    let (controls, _steer, token) = controls();
    let stream = harness
        .run(rig.request("never finish"), controls)
        .await
        .expect("run starts");
    let mut cancelled_at = None;
    let events = collect_turn(stream, |event| {
        if cancelled_at.is_none() && matches!(event, AgentEvent::TextDelta { .. }) {
            cancelled_at = Some(Instant::now());
            token.cancel();
        }
    })
    .await;
    let elapsed = cancelled_at
        .expect("text streamed before the hang")
        .elapsed();
    assert_eq!(dones(&events), vec![(DoneStatus::Interrupted, None)]);
    assert!(
        elapsed < Duration::from_secs(5),
        "cancel took {elapsed:?}: the agent needed signal escalation"
    );
}

/// (e) A second process resumes the first one's session through
/// `session/load`: same session id, no fresh-session fallback, and the
/// model sees the earlier exchange.
#[tokio::test]
async fn real_zimmer_resumes_a_session_with_session_load() {
    let binary = require_zimmer!();
    let rig = Rig::new(binary);

    let first = rig.harness("first", json!([say("first answer")]), false);
    let (controls, _steer, _token) = controls();
    let stream = first
        .run(rig.request("remember the word pelican"), controls)
        .await
        .expect("run starts");
    let events = collect_turn(stream, |_| {}).await;
    assert_eq!(dones(&events), vec![(DoneStatus::Completed, None)]);
    let id = session_id(&events);
    drop(events);

    let second = rig.harness("second", json!([say("second answer")]), true);
    let (controls, _steer, _token) = self::controls();
    let mut request = rig.request("what was the word?");
    request.resume = Some(id.clone());
    let stream = second.run(request, controls).await.expect("run starts");
    let events = collect_turn(stream, |_| {}).await;

    assert_eq!(session_id(&events), id);
    assert!(
        !events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "fell back to a fresh session: {events:?}"
    );
    // The replayed history is dropped; only the live turn streams.
    assert_eq!(text(&events), "second answer", "{events:?}");
    assert_eq!(dones(&events), vec![(DoneStatus::Completed, None)]);
    assert!(
        rig.wire().iter().any(|m| m["method"] == "session/load"),
        "{:?}",
        rig.wire()
    );
    let requests = rig.requests();
    let resumed = requests.last().expect("a recorded model request");
    let history = resumed["messages"].to_string();
    assert!(
        history.contains("pelican") && history.contains("first answer"),
        "{history}"
    );
}
