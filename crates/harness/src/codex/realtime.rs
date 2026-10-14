//! Ephemeral native subscription voice in the same app-server process as text/MCP.
//! WebSocket appendAudio is deliberately unavailable: in Codex 0.159 it needs an API key.
#[path = "realtime_host.rs"]
mod host;
use crate::jsonrpc::{Incoming, RpcClient};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
type AudioAbort = Arc<Mutex<Option<(u64, CancellationToken)>>>;
use zeron_proto::voice::*;

/// What the backing Codex model is told when a call starts
/// (`realtimeStartInstructions`). The parameter replaces Codex's own framing
/// of a realtime session instead of adding to it, so the opening restates
/// that framing. The rest is what Zeron adds: when its MCP tools are the
/// route, by what the user wants to end up with, and what keeps a call
/// moving. Tools are named because Codex otherwise searches for them in every
/// session. `{WHERE}` is [`whereabouts`].
const EXECUTOR_INSTRUCTIONS: &str = r#"Realtime voice conversation started in Zeron.

You are the backend executor behind a voice intermediary. The user does not talk to you directly: their words reach you as a transcript that may be unpunctuated or misrecognized, and your reply is summarized aloud. If no work is needed, answer in one line.

Work as you normally would, with your usual tools. {WHERE}

Zeron adds one capability: its MCP tools (`mcp__zeron__*`) operate the user's other chats, which run Codex, Claude Code and other agents across projects and devices. Route by what the user wants to end up with:
- An answer or a contained change here and now (read code, run a command, edit a file, look something up): do it yourself.
- Anything about chats, agents, projects or devices ("what's running", "what did it say", "tell it to…", "stop it"): `list_chats`, `read_chat`, `send_message`, `interrupt_chat`.
- Work that should outlive this call, run in parallel, live in another project or device, or go to a named agent or model: `create_chat` or `create_chats` with `wait: false` and a self-contained prompt that ends by asking the chat to report back to this one with `send_message` when it is done or blocked. Use `kind: "chat"` when it is its own piece of work, `"side"` when it only helps this conversation. Set harness or model only if the user named one.
Your own subagents are for splitting work you do yourself; "delegate", "hand off" or an agent's name means a Zeron chat.

Keep the call moving:
- Answer in seconds. Never block on another chat for more than a few seconds: start it, say it has started, check when asked.
- A message from another Zeron chat is a report, not a user request: tell the user what it says in a sentence, and never reply just to acknowledge.
- If a chat is waiting on a question, ask the user and pass on their answer. Ask your own questions in your reply, not through an input prompt.
- Before anything destructive or outward-facing (delete, force-push, send, publish, spend), say what you will do and wait for a yes.
- If a name could match several chats or projects, ask which.

Write for the ear: outcome first, one or two plain sentences, in the user's language. No tables, code, paths or ids unless asked; name chats by title."#;

/// What the realtime voice model is told, as an initial developer item. It
/// has no MCP tools: it hands requests to the backing model or stays silent,
/// and Codex's own voice prompt already covers brevity and handing over. This
/// adds what is Zeron's, and corrects that prompt's assumption that the user
/// can read the backend's text: on the call screen they cannot.
const VOICE_INSTRUCTIONS: &str = r#"You are running inside Zeron, an app where the user runs many coding-agent chats (Codex, Claude Code and others) across projects and devices. Besides normal Codex work, the backend can list, read, start, message, stop and answer those chats, so hand such requests over like any other.

- The user is on a call screen and usually cannot read the backend's text. Say the result itself in one or two sentences; do not point them to the screen.
- Work handed to another chat belongs to that chat: name the chat and its agent, and say it has started, not that it is done.
- Never read out ids, paths, URLs or hashes. Refer to chats and projects by name.
- If you did not catch a chat, project or agent name, ask before handing the request over.
- Reports from other chats can arrive unprompted: say each one in a sentence when it arrives."#;

/// What the backing model is told when the call ends
/// (`realtimeEndInstructions`): Codex's own text for it, and one rule that
/// outlives the call, since chats it started may still report afterwards.
const END_INSTRUCTIONS: &str = "Realtime conversation ended.\n\nSubsequent user input will return to typed text rather than transcript-style text. Do not assume recognition errors or missing punctuation once realtime has ended. Resume normal chat behavior: the voice rules above no longer apply, except that a message from another Zeron chat is still a report, not a user request.";

/// Where the conversation works, for [`EXECUTOR_INSTRUCTIONS`]. One without
/// a project runs in the home directory, which is no place to work on a
/// codebase from.
fn whereabouts(cwd: &str) -> String {
    let path = std::path::Path::new(cwd);
    let home = crate::executable::home_dir();
    match path.file_name().and_then(|name| name.to_str()) {
        Some(project) if home.as_deref() != Some(path) => {
            format!("This conversation works in {project} ({cwd}).")
        }
        _ => "This conversation has no project: it runs in the home directory. Lookups and one-off commands are fine here; work on a codebase belongs in a Zeron chat in that project.".to_owned(),
    }
}

/// Start options still on trial, off unless the environment asks:
/// `ZERON_VOICE_REASONING_STATUS=1` relays the backing model's public
/// reasoning summaries to the voice model as quiet context, which may fill
/// long turns; `ZERON_VOICE_ACK_FILLER=0|1` forces the acknowledgement a
/// delegation produces off or on, instead of the Realtime API's default;
/// `ZERON_VOICE_STARTUP_CONTEXT=0` starts without Codex's startup context
/// (recent work and directory trees), to measure what building it costs a
/// call's start.
fn trials(params: &mut Value) {
    let flag = |name: &str| match std::env::var(name).ok()?.as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    };
    if flag("ZERON_VOICE_REASONING_STATUS") == Some(true) {
        params["backendReasoningStatus"] = json!(true);
    }
    if let Some(filler) = flag("ZERON_VOICE_ACK_FILLER") {
        params["delegationAckFiller"] = json!(filler);
    }
    if flag("ZERON_VOICE_STARTUP_CONTEXT") == Some(false) {
        params["includeStartupContext"] = json!(false);
    }
}

/// The `thread/realtime/start` request, the same for a call whose audio this
/// device carries and one a remote client does.
fn start_params(
    thread: &str,
    sdp: &str,
    session_id: &str,
    voice: Option<String>,
    context: &ThreadContext,
) -> Value {
    let mut params = json!({
        "threadId": thread,
        "transport": {"type": "webrtc", "sdp": sdp},
        "version": "v3",
        "outputModality": "audio",
        "realtimeSessionId": session_id,
        "clientManagedHandoffs": false,
        "includeStartupContext": true,
        "realtimeStartInstructions":
            EXECUTOR_INSTRUCTIONS.replace("{WHERE}", &whereabouts(&context.cwd)),
        "realtimeEndInstructions": END_INSTRUCTIONS,
        "initialItems": [{"role": "developer", "text": VOICE_INSTRUCTIONS}],
    });
    if let Some(voice) = voice {
        params["voice"] = json!(voice);
    }
    trials(&mut params);
    params
}

#[derive(Clone)]
pub(super) struct ThreadContext {
    pub cwd: String,
    pub model_provider: Option<String>,
}

/// How long a successful probe stands in for the one a start would repeat.
/// Covers the owner-attach window; identity changes drop it sooner.
const PROBE_FRESH: Duration = Duration::from_secs(10);

pub enum VoiceCommand {
    Probe {
        external: bool,
        /// Also read ordinary usage. It is a network round trip that only
        /// informs the eligibility view: the native backend decides spend.
        usage: bool,
        reply: oneshot::Sender<Result<VoiceEligibility, VoiceRejection>>,
    },
    Start {
        voice: Option<String>,
        session_id: String,
        generation: u64,
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
    External {
        voice: Option<String>,
        session_id: String,
        generation: u64,
        offer: remote::Sdp,
        reply: oneshot::Sender<Result<remote::Sdp, VoiceRejection>>,
    },
    Mute {
        muted: bool,
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
    Append {
        frame: VoiceFrame,
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), VoiceRejection>>,
    },
}
pub struct RealtimeControls {
    audio_abort: AudioAbort,
    invalidated: Arc<std::sync::atomic::AtomicBool>,
    pub commands: mpsc::Receiver<VoiceCommand>,
    pub events: broadcast::Sender<VoiceEvent>,
}
#[derive(Clone)]
pub struct RealtimeHandle {
    audio_abort: AudioAbort,
    invalidated: Arc<std::sync::atomic::AtomicBool>,
    pub commands: mpsc::Sender<VoiceCommand>,
    pub events: broadcast::Sender<VoiceEvent>,
}
impl RealtimeHandle {
    pub fn invalidated(&self) -> bool {
        self.invalidated.load(Ordering::Acquire)
    }
    /// Immediately closes native capture even if the control actor is waiting on I/O.
    pub fn abort_voice(&self, generation: u64) {
        if let Some((current, cancel)) = self.audio_abort.lock().unwrap().as_ref() {
            if *current == generation {
                cancel.cancel();
            }
        }
    }

    /// Full eligibility, including ordinary usage.
    pub async fn probe(&self) -> Result<VoiceEligibility, VoiceRejection> {
        self.probe_mode(false, true).await
    }
    /// What a local start needs verified; the start that follows reuses it.
    pub async fn probe_start(&self) -> Result<VoiceEligibility, VoiceRejection> {
        self.probe_mode(false, false).await
    }
    pub async fn probe_external(&self) -> Result<VoiceEligibility, VoiceRejection> {
        self.probe_mode(true, false).await
    }
    async fn probe_mode(
        &self,
        external: bool,
        usage: bool,
    ) -> Result<VoiceEligibility, VoiceRejection> {
        let (reply, rx) = oneshot::channel();
        tokio::time::timeout(Duration::from_secs(15), async {
            self.commands
                .send(VoiceCommand::Probe {
                    external,
                    usage,
                    reply,
                })
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            rx.await.map_err(|_| VoiceRejection::Protocol)?
        })
        .await
        .map_err(|_| VoiceRejection::Protocol)?
    }

    pub async fn stop(&self) -> Result<(), VoiceRejection> {
        let (reply, rx) = oneshot::channel();
        tokio::time::timeout(Duration::from_secs(8), async {
            self.commands
                .send(VoiceCommand::Stop { reply })
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            rx.await.map_err(|_| VoiceRejection::Protocol)?
        })
        .await
        .map_err(|_| VoiceRejection::Protocol)?
    }
    pub async fn mute(&self, muted: bool) -> Result<(), VoiceRejection> {
        let (reply, rx) = oneshot::channel();
        tokio::time::timeout(Duration::from_secs(8), async {
            self.commands
                .send(VoiceCommand::Mute { muted, reply })
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            rx.await.map_err(|_| VoiceRejection::Protocol)?
        })
        .await
        .map_err(|_| VoiceRejection::Protocol)?
    }
}
pub fn channel() -> (
    RealtimeHandle,
    RealtimeControls,
    broadcast::Receiver<VoiceEvent>,
) {
    let audio_abort = Arc::default();
    let invalidated = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (commands, rx) = mpsc::channel(8);
    let (events, receiver) = broadcast::channel(32);
    (
        RealtimeHandle {
            audio_abort: Arc::clone(&audio_abort),
            invalidated: invalidated.clone(),
            commands,
            events: events.clone(),
        },
        RealtimeControls {
            audio_abort,
            invalidated,
            commands: rx,
            events,
        },
        receiver,
    )
}
pub(crate) struct BridgeTask(tokio::task::JoinHandle<()>);
impl Drop for BridgeTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn attach(
    client: RpcClient,
    thread: String,
    executable: PathBuf,
    context: ThreadContext,
    controls: RealtimeControls,
) -> BridgeTask {
    let abort = controls.audio_abort.clone();
    let invalidated = controls.invalidated.clone();
    let (mut wire, overflow) = client.subscribe_voice(move || {
        invalidated.store(true, Ordering::Release);
        if let Some((_, cancel)) = abort.lock().unwrap().as_ref() {
            cancel.cancel();
        }
    });
    BridgeTask(tokio::spawn(async move {
        let RealtimeControls {
            audio_abort,
            invalidated,
            mut commands,
            events,
        } = controls;
        let mut native: Option<host::NativeHost> = None;
        let mut external_active = false;
        let mut external_reply: Option<oneshot::Sender<Result<remote::Sdp, VoiceRejection>>> = None;
        let mut starting: Option<
            futures::future::BoxFuture<'static, Result<Started, VoiceRejection>>,
        > = None;
        let mut start_reply: Option<oneshot::Sender<Result<(), VoiceRejection>>> = None;
        let mut answer_tx: Option<oneshot::Sender<String>> = None;
        let mut accepted_tx: Option<oneshot::Sender<()>> = None;
        let mut expected_session = String::new();
        // Voices from the last successful probe and whether it covered the
        // local helper. One start consumes it instead of probing again.
        let mut verified: Option<(Instant, bool, Vec<String>)> = None;
        let server_live = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut generation = 0;
        let mut sequence = 0;
        // How often the call's audio process is asked how loud each voice
        // is. Codex's voice is known to the orb only by this, so it is read
        // often enough to tell one syllable from the next.
        let mut timer = tokio::time::interval(Duration::from_millis(40));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { biased;
                command=commands.recv()=>match command {
                    Some(VoiceCommand::Probe{external,usage,reply})=>{let result=if invalidated.load(Ordering::Acquire){Err(VoiceRejection::InvalidLease)}else{probe_mode(&client,&executable,&context,external,usage).await};
                        verified=result.as_ref().ok().map(|eligible|(Instant::now(),!external,eligible.voices.clone()));
                        let _=reply.send(if invalidated.load(Ordering::Acquire){Err(VoiceRejection::InvalidLease)}else{result});},
                    Some(VoiceCommand::Start{voice,session_id,generation:next,reply})=>{
                        if invalidated.load(Ordering::Acquire){let _=reply.send(Err(VoiceRejection::InvalidLease));continue;}
                        if external_active || native.is_some() || starting.is_some() {let _=reply.send(Err(VoiceRejection::Busy));continue;}
                        generation=next;sequence=0;
                        let voices=verified.take().filter(|(at,local,_)|*local&&at.elapsed()<PROBE_FRESH).map(|v|v.2);
                        let abort=CancellationToken::new();
                        {let mut slot=audio_abort.lock().unwrap();
                            if invalidated.load(Ordering::Acquire){let _=reply.send(Err(VoiceRejection::InvalidLease));continue;}
                            *slot=Some((next,abort.clone()));
                        }
                        let (answer,answer_rx)=oneshot::channel();let (accepted,accepted_rx)=oneshot::channel();
                        answer_tx=Some(answer);accepted_tx=Some(accepted);expected_session=session_id.clone();
                        starting=Some(Box::pin({let client=client.clone();let thread=thread.clone();let executable=executable.clone();let context=context.clone();let server_live=server_live.clone();async move {
                            start(client,thread,executable,context,voice,voices,session_id,accepted_rx,answer_rx,abort,server_live).await.map(Started::Local)
                        }}));
                        start_reply=Some(reply);
                    },
                    Some(VoiceCommand::External{voice,session_id,generation:next,offer,reply})=>{
                        if invalidated.load(Ordering::Acquire){let _=reply.send(Err(VoiceRejection::InvalidLease));continue;}
                        if external_active || native.is_some() || starting.is_some(){let _=reply.send(Err(VoiceRejection::Busy));continue;}
                        generation=next;sequence=0;
                        let voices=verified.take().filter(|(at,..)|at.elapsed()<PROBE_FRESH).map(|v|v.2);
                        let abort=CancellationToken::new();
                        *audio_abort.lock().unwrap()=Some((next,abort.clone()));
                        let (answer,answer_rx)=oneshot::channel();let (accepted,accepted_rx)=oneshot::channel();
                        answer_tx=Some(answer);accepted_tx=Some(accepted);expected_session=session_id.clone();
                        let client=client.clone();let thread=thread.clone();let context=context.clone();let server_live=server_live.clone();
                        starting=Some(Box::pin(async move {
                            tokio::select!{biased;
                                _=abort.cancelled()=>Err(VoiceRejection::InvalidLease),
                                answer=start_external(client,thread,context,voice,voices,session_id,offer,accepted_rx,answer_rx,server_live)=>answer.map(Started::External),
                            }
                        }));
                        external_reply=Some(reply);
                    },
                    Some(VoiceCommand::Mute{muted,reply})=>{
                        let result=if let Some(host)=native.as_mut(){host.controls(muted).await}else{Err(VoiceRejection::Busy)};
                        let failed=result.is_err();let _=reply.send(result);
                        if failed {native=None;let _=events.send(VoiceEvent::Closed{generation,reason:Some(VoiceRejection::Protocol)});}
                    },
                    Some(VoiceCommand::Append{reply,..})=>{let _=reply.send(Err(VoiceRejection::Unsupported));},
                    Some(VoiceCommand::Stop{reply})=>{
                        let had_session=server_live.load(Ordering::Acquire);starting=None;native=None;external_active=false;
                        if let Some(reply)=external_reply.take(){let _=reply.send(Err(VoiceRejection::InvalidLease));}
                        if let Some(pending)=start_reply.take(){let _=pending.send(Err(VoiceRejection::InvalidLease));}
                        let result=if had_session{
                            let result=stop(&client,&thread).await;
                            // Native stop acknowledges submission first. Drain its terminal event
                            // before a successor can relabel old notifications with a new generation.
                            let closed=tokio::time::timeout(Duration::from_secs(3),async {
                                while let Some(notification)=wire.recv().await {
                                    if let Incoming::Notification{method,params}=notification {
                                        if params["threadId"]!=thread{continue;}
                                        if method=="thread/realtime/closed" && params["reason"]=="requested"{server_live.store(false,Ordering::Release);return Ok::<_,VoiceRejection>(());}
                                        if let Some(event)=normalize(&thread,generation,&mut sequence,&method,&params){let _=events.send(event);}
                                    }
                                }
                                Err(VoiceRejection::Protocol)
                            }).await;
                            if result.is_err()||!matches!(closed,Ok(Ok(()))){let _=reply.send(Err(VoiceRejection::Protocol));break;}
                            result
                        }else{Ok(())};
                        let _=events.send(VoiceEvent::Closed{generation,reason:None});let _=reply.send(result);
                    },
                    None=>break,
                },
                result=async{starting.as_mut().unwrap().await},if starting.is_some()=>{
                    starting=None;
                    match result {
                        Ok(Started::Local(host))=>{native=Some(host);if let Some(reply)=start_reply.take(){let _=reply.send(Ok(()));}},
                        Ok(Started::External(answer))=>{external_active=true;if let Some(reply)=external_reply.take(){let _=reply.send(Ok(answer));}},
                        Err(reason)=>{
                        if let Some(reply)=external_reply.take(){let _=reply.send(Err(reason));}
                        if let Some(reply)=start_reply.take(){let _=reply.send(Err(reason));}
                        let _=events.send(VoiceEvent::Closed{generation,reason:Some(reason)});
                    }}
                },
                notification=wire.recv()=>match notification {
                    Some(Incoming::Notification{method,params})=>{
                        if method=="thread/realtime/closed" && params["threadId"]==thread {server_live.store(false,Ordering::Release);}
                        if method=="account/updated" {verified=None;native=None;starting=None;external_active=false;if let Some(reply)=external_reply.take(){let _=reply.send(Err(VoiceRejection::ChatgptRequired));}if let Some(reply)=start_reply.take(){let _=reply.send(Err(VoiceRejection::ChatgptRequired));}let _=events.send(VoiceEvent::Closed{generation,reason:Some(VoiceRejection::ChatgptRequired)});continue;}
                        if params["threadId"]==thread && starting.is_some() {
                            if method=="thread/realtime/started" {
                                if params["version"]=="v3" && params["realtimeSessionId"]==expected_session {if let Some(tx)=accepted_tx.take(){let _=tx.send(());}}
                                else {starting=None;if let Some(reply)=external_reply.take(){let _=reply.send(Err(VoiceRejection::Unsupported));}if let Some(reply)=start_reply.take(){let _=reply.send(Err(VoiceRejection::Unsupported));}let _=events.send(VoiceEvent::Closed{generation,reason:Some(VoiceRejection::Unsupported)});}
                                continue;
                            }
                            if method=="thread/realtime/sdp" {
                                if let Some(sdp)=params["sdp"].as_str().filter(|s|!s.is_empty()&&s.len()<=64*1024) {if let Some(tx)=answer_tx.take(){let _=tx.send(sdp.to_owned());}}
                                continue;
                            }
                        }
                        if external_active || native.is_some() || starting.is_some() {
                            if let Some(event)=normalize(&thread,generation,&mut sequence,&method,&params) {
                                let terminal=matches!(event,VoiceEvent::Closed{..}); let _=events.send(event);
                                if terminal {native=None;starting=None;external_active=false;if let Some(reply)=external_reply.take(){let _=reply.send(Err(VoiceRejection::Protocol));}if let Some(reply)=start_reply.take(){let _=reply.send(Err(VoiceRejection::Protocol));}}
                            }
                        }
                    },Some(_)=>{},None=>break,
                },
                _=timer.tick()=>{
                    if overflow.load(Ordering::Acquire)||client.is_closed(){break;}
                    if let Some(host)=native.as_mut(){ match host.levels().await {
                        Ok((microphone,speaker))=>{let _=events.send(VoiceEvent::Levels{generation,microphone,speaker});},
                        Err(reason)=>{native=None;let _=events.send(VoiceEvent::Closed{generation,reason:Some(reason)});}
                    }}
                }
            }
        }
        let had_session = starting.take().is_some()
            || native.is_some()
            || external_active
            || server_live.load(Ordering::Acquire);
        drop(native);
        if had_session {
            let _ = stop(&client, &thread).await;
        }
        let _ = events.send(VoiceEvent::Closed {
            generation,
            reason: Some(VoiceRejection::Protocol),
        });
    }))
}
enum Started {
    Local(host::NativeHost),
    External(remote::Sdp),
}

async fn probe_mode(
    client: &RpcClient,
    executable: &std::path::Path,
    context: &ThreadContext,
    external: bool,
    usage: bool,
) -> Result<VoiceEligibility, VoiceRejection> {
    if !external {
        host::helper_path(executable)?;
    }
    tokio::time::timeout(
        Duration::from_secs(15),
        probe_details(client, context, usage),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
}
async fn probe_details(
    client: &RpcClient,
    context: &ThreadContext,
    usage: bool,
) -> Result<VoiceEligibility, VoiceRejection> {
    if context.model_provider.as_deref() != Some("openai")
        || !std::path::Path::new(&context.cwd).is_absolute()
    {
        return Err(VoiceRejection::ChatgptRequired);
    }
    // Independent reads: together they cost the slowest one, not their sum.
    let (account, config, limits, voices) = tokio::join!(
        client.request("account/read", json!({"refreshToken":false})),
        client.request(
            "config/read",
            json!({"includeLayers":false,"cwd":context.cwd}),
        ),
        async {
            if usage {
                Some(client.request("account/rateLimits/read", json!({})).await)
            } else {
                None
            }
        },
        client.request("thread/realtime/listVoices", json!({})),
    );
    let account = account.map_err(|_| VoiceRejection::Protocol)?;
    if account["account"]["type"] != "chatgpt" {
        return Err(VoiceRejection::ChatgptRequired);
    }
    let config = config.map_err(|_| VoiceRejection::Protocol)?;
    let config = &config["config"];
    if config["model_provider"]
        .as_str()
        .is_some_and(|p| p != "openai")
        || config["model_providers"]["openai"].is_object()
        || [
            "experimental_realtime_ws_base_url",
            "experimental_realtime_webrtc_call_base_url",
            "experimental_realtime_ws_model",
        ]
        .iter()
        .any(|k| !config[*k].is_null())
    {
        return Err(VoiceRejection::ChatgptRequired);
    }
    // Ordinary quota may be exhausted while permitted account credits remain.
    // The native backend applies spend controls and the final usage decision.
    let ordinary = match limits {
        Some(limits) => {
            limits.map_err(|_| VoiceRejection::Protocol)?["ordinaryUsageAllowed"].as_bool()
        }
        None => None,
    };
    let v = voices.map_err(|_| VoiceRejection::Unsupported)?;
    let voices = v["voices"]["v1"]
        .as_array()
        .ok_or(VoiceRejection::Unsupported)?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    Ok(VoiceEligibility {
        available: true,
        reason: None,
        ordinary_usage_allowed: ordinary,
        credits_excluded: false,
        format: None,
        duplex_verified: true,
        native_webrtc: true,
        voices,
    })
}
/// Logs how long one startup stage took, so a slow call start names its stage.
fn stage(name: &'static str, since: &mut Instant) {
    tracing::info!(
        target: "zeron_harness::codex::voice",
        stage = name,
        ms = since.elapsed().as_millis() as u64,
        "voice start"
    );
    *since = Instant::now();
}
async fn start(
    client: RpcClient,
    thread: String,
    executable: PathBuf,
    context: ThreadContext,
    voice: Option<String>,
    voices: Option<Vec<String>>,
    session_id: String,
    accepted: oneshot::Receiver<()>,
    answer: oneshot::Receiver<String>,
    abort: CancellationToken,
    server_live: Arc<std::sync::atomic::AtomicBool>,
) -> Result<host::NativeHost, VoiceRejection> {
    let mut since = Instant::now();
    let voices = match voices {
        Some(voices) => voices,
        None => {
            probe_mode(&client, &executable, &context, false, false)
                .await?
                .voices
        }
    };
    if voice.as_ref().is_some_and(|v| !voices.contains(v)) {
        return Err(VoiceRejection::Unsupported);
    }
    stage("eligibility", &mut since);
    let mut host = host::NativeHost::open(&host::helper_path(&executable)?, abort).await?;
    let offer = host.exchange(json!({"type":"startTransport"}), 20).await?;
    stage("helper offer", &mut since);
    let sdp = offer["sdp"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 64 * 1024)
        .ok_or(VoiceRejection::Protocol)?;
    if offer["type"] != "offer" {
        return Err(VoiceRejection::Protocol);
    }
    let params = start_params(&thread, sdp, &session_id, voice, &context);
    server_live.store(true, Ordering::Release);
    tokio::time::timeout(
        Duration::from_secs(30),
        client.request("thread/realtime/start", params),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
    .map_err(|_| VoiceRejection::Protocol)?;
    stage("realtime start request", &mut since);
    tokio::time::timeout(Duration::from_secs(30), accepted)
        .await
        .map_err(|_| VoiceRejection::Protocol)?
        .map_err(|_| VoiceRejection::Protocol)?;
    let answer = tokio::time::timeout(Duration::from_secs(30), answer)
        .await
        .map_err(|_| VoiceRejection::Protocol)?
        .map_err(|_| VoiceRejection::Protocol)?;
    stage("provider answer", &mut since);
    host.expect(
        json!({"type":"applyAnswer","sdp":answer}),
        "transportReady",
        20,
    )
    .await?;
    stage("transport ready", &mut since);
    host.expect(json!({"type":"openDevices"}), "devicesOpened", 5)
        .await
        .map_err(|_| VoiceRejection::DeviceUnavailable)?;
    stage("devices opened", &mut since);
    // Keep devices muted/suppressed until the engine confirms the owner stream.
    Ok(host)
}
async fn start_external(
    client: RpcClient,
    thread: String,
    context: ThreadContext,
    voice: Option<String>,
    voices: Option<Vec<String>>,
    session_id: String,
    offer: remote::Sdp,
    accepted: oneshot::Receiver<()>,
    answer: oneshot::Receiver<String>,
    server_live: Arc<std::sync::atomic::AtomicBool>,
) -> Result<remote::Sdp, VoiceRejection> {
    let mut since = Instant::now();
    let voices = match voices {
        Some(voices) => voices,
        None => {
            tokio::time::timeout(
                Duration::from_secs(15),
                probe_details(&client, &context, false),
            )
            .await
            .map_err(|_| VoiceRejection::Protocol)??
            .voices
        }
    };
    if voice.as_ref().is_some_and(|v| !voices.contains(v)) {
        return Err(VoiceRejection::Unsupported);
    }
    stage("eligibility", &mut since);
    let params = start_params(&thread, offer.expose(), &session_id, voice, &context);
    server_live.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(90), async {
        client
            .request("thread/realtime/start", params)
            .await
            .map_err(|_| VoiceRejection::Protocol)?;
        accepted.await.map_err(|_| VoiceRejection::Protocol)?;
        let answer = remote::Sdp::new(answer.await.map_err(|_| VoiceRejection::Protocol)?);
        stage("provider answer", &mut since);
        answer
    })
    .await
    .map_err(|_| VoiceRejection::Protocol)?
}

async fn stop(client: &RpcClient, thread: &str) -> Result<(), VoiceRejection> {
    tokio::time::timeout(
        Duration::from_secs(2),
        client.request("thread/realtime/stop", json!({"threadId":thread})),
    )
    .await
    .map_err(|_| VoiceRejection::Protocol)?
    .map_err(|_| VoiceRejection::Protocol)?;
    Ok(())
}

/// Canonical completed items only. Legacy transcript/done has no stable item
/// identity and is deliberately not committed alongside the canonical stream.
pub(crate) fn normalize(
    thread: &str,
    generation: u64,
    sequence: &mut u64,
    method: &str,
    p: &Value,
) -> Option<VoiceEvent> {
    if p.get("threadId")?.as_str()? != thread {
        return None;
    }
    match method {
        "thread/realtime/item/completed" => {
            let item = p.get("item")?;
            if item.get("type")?.as_str()? != "transcriptSegment" {
                return None;
            }
            let text = item.get("text")?.as_str()?;
            if text.len() > MAX_TRANSCRIPT_BYTES {
                return None;
            }
            let role = match item.get("role")?.as_str()? {
                "user" => VoiceRole::User,
                "assistant" => VoiceRole::Assistant,
                _ => return None,
            };
            Some(VoiceEvent::Final {
                transcript: VoiceTranscript {
                    session_id: item.get("realtimeSessionId")?.as_str()?.to_owned(),
                    item_id: item.get("id")?.as_str()?.to_owned(),
                    role,
                    text: text.to_owned(),
                    promoted_message_id: None,
                },
            })
        }
        "thread/realtime/outputAudio/delta" => {
            let a = p.get("audio")?;
            let data = a.get("data")?.as_str()?;
            if data.len() > MAX_AUDIO_BYTES.div_ceil(3) * 4 {
                return None;
            }
            let format = VoiceFormat {
                encoding: VoiceEncoding::Pcm16Le,
                sample_rate: u32::try_from(a.get("sampleRate")?.as_u64()?).ok()?,
                channels: u16::try_from(a.get("numChannels")?.as_u64()?).ok()?,
            };
            if !format.validate(STANDARD.decode(data).ok()?.len()) {
                return None;
            }
            *sequence += 1;
            Some(VoiceEvent::Audio {
                frame: VoiceFrame {
                    generation,
                    sequence: *sequence,
                    format,
                    data: data.to_owned(),
                    item_id: a.get("itemId").and_then(Value::as_str).map(str::to_owned),
                },
            })
        }
        "thread/realtime/item/transcript/delta" => {
            let text = p.get("delta")?.as_str()?;
            (text.len() <= MAX_TRANSCRIPT_BYTES).then(|| VoiceEvent::Partial {
                generation,
                item_id: p.get("itemId").and_then(Value::as_str).map(str::to_owned),
                text: text.to_owned(),
                role: None,
            })
        }
        // An utterance begins: its deltas name no speaker, this does. Its
        // text arrives through those deltas and the completed item.
        "thread/realtime/item/started" => {
            let item = p.get("item")?;
            if item.get("type")?.as_str()? != "transcriptSegment" {
                return None;
            }
            let role = match item.get("role")?.as_str()? {
                "user" => VoiceRole::User,
                "assistant" => VoiceRole::Assistant,
                _ => return None,
            };
            Some(VoiceEvent::Partial {
                generation,
                item_id: Some(item.get("id")?.as_str()?.to_owned()),
                text: String::new(),
                role: Some(role),
            })
        }
        "thread/realtime/itemAdded"
            if matches!(
                p["item"]["type"].as_str(),
                Some("input_audio_buffer.speech_started" | "response.cancelled")
            ) =>
        {
            Some(VoiceEvent::InvalidatePlayout {
                generation,
                item_id: p["item"]["item_id"].as_str().unwrap_or_default().to_owned(),
            })
        }
        "thread/realtime/closed" | "thread/realtime/error" => {
            // The event carries no cause; the log keeps what Codex gave.
            let said = |key| p.get(key).and_then(Value::as_str).unwrap_or_default();
            let (message, reason) = (said("message"), said("reason"));
            tracing::warn!(
                target: "zeron_harness::codex::voice",
                method,
                message,
                reason,
                "realtime session over"
            );
            Some(VoiceEvent::Closed {
                generation,
                reason: (method.ends_with("error")).then_some(VoiceRejection::Protocol),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_items_do_not_duplicate_legacy_or_foreign_threads() {
        let p = json!({"threadId":"parent", "item":{"type":"transcriptSegment", "id":"item", "realtimeSessionId":"voice", "role":"user", "text":"hello"}});
        let mut seq = 0;
        assert!(matches!(
            normalize("parent", 1, &mut seq, "thread/realtime/item/completed", &p),
            Some(VoiceEvent::Final { .. })
        ));
        assert!(normalize("other", 1, &mut seq, "thread/realtime/item/completed", &p).is_none());
        assert!(normalize("parent", 1, &mut seq, "thread/realtime/transcript/done", &p).is_none());
    }
    #[test]
    fn each_model_is_told_what_is_its_own() {
        let context = |cwd: &str| ThreadContext {
            cwd: cwd.to_owned(),
            model_provider: None,
        };
        let params = start_params(
            "t",
            "offer",
            "s",
            Some("ember".into()),
            &context("/work/comet"),
        );
        let executor = params["realtimeStartInstructions"].as_str().unwrap();
        // The backing model keeps Codex's framing, which this text replaces,
        // knows where it works, and has Zeron's tools by name.
        assert!(executor.contains("backend executor behind a voice intermediary"));
        assert!(executor.contains("This conversation works in comet (/work/comet)."));
        assert!(executor.contains("`create_chat`") && executor.contains("mcp__zeron__"));
        assert!(!executor.contains("{WHERE}"));
        // The voice model has no MCP tools and is never told to use any.
        let voice = &params["initialItems"];
        assert_eq!(voice.as_array().unwrap().len(), 1);
        assert_eq!(voice[0]["role"], "developer");
        let voice = voice[0]["text"].as_str().unwrap();
        assert_ne!(voice, executor);
        assert!(!voice.contains("mcp__") && !voice.contains("`create_chat`"));
        assert!(voice.contains("cannot read the backend's text"));
        // Chats the call started may report after it ends.
        assert!(
            params["realtimeEndInstructions"]
                .as_str()
                .unwrap()
                .contains("still a report")
        );
        assert_eq!(params["voice"], "ember");
        assert_eq!(params["includeStartupContext"], true);
        // Without a project the conversation runs at home: no codebase there.
        let home = crate::executable::home_dir().unwrap();
        let params = start_params("t", "offer", "s", None, &context(home.to_str().unwrap()));
        let executor = params["realtimeStartInstructions"].as_str().unwrap();
        assert!(executor.contains("This conversation has no project"));
        assert!(params.get("voice").is_none());
    }
    #[test]
    fn a_started_utterance_names_its_speaker_before_any_text() {
        let p = json!({"threadId":"parent", "item":{"type":"transcriptSegment", "id":"item", "realtimeSessionId":"voice", "role":"assistant", "text":""}});
        let mut seq = 0;
        let Some(VoiceEvent::Partial {
            generation,
            item_id,
            text,
            role,
        }) = normalize("parent", 7, &mut seq, "thread/realtime/item/started", &p)
        else {
            panic!("a started transcript segment opens its utterance");
        };
        assert_eq!((generation, item_id.as_deref()), (7, Some("item")));
        assert!(text.is_empty());
        assert_eq!(role, Some(VoiceRole::Assistant));
        // Session markers and promotions are not utterances.
        let marker = json!({"threadId":"parent", "item":{"type":"realtimeSessionStarted", "id":"s", "realtimeSessionId":"voice"}});
        assert!(
            normalize(
                "parent",
                7,
                &mut seq,
                "thread/realtime/item/started",
                &marker
            )
            .is_none()
        );
    }
}
